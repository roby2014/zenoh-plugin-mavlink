use std::sync::Arc;

use protocol::Protocol;
use tokio::task::JoinSet;
use tracing::{debug, error, info};
use tracing::{info_span, Instrument};
use zenoh::ke;
use zenoh::liveliness::LivelinessToken;
use zenoh::prelude::r#async::AsyncResolve;
use zenoh::prelude::*;
use zenoh::Result as ZResult;
use zenoh::{
    plugins::{RunningPlugin, RunningPluginTrait, ZenohPlugin},
    runtime::Runtime,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};

pub mod config;
pub mod mavlink_connection;
pub mod protocol;
use config::Config;
use zenoh_util::core::zerror;

const KE_PREFIX_LIVELINESS_GROUP: &str = "zenoh-plugin-mavlink";

pub struct MAVLinkPlugin;

impl PluginControl for MAVLinkPlugin {}
impl ZenohPlugin for MAVLinkPlugin {}
impl Plugin for MAVLinkPlugin {
    type StartArgs = Runtime;
    type Instance = RunningPlugin;

    const DEFAULT_NAME: &'static str = "mavlink";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<RunningPlugin> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh_util::try_init_log_from_env();

        let runtime_conf = runtime.config().lock();
        let plugin_conf = runtime_conf
            .plugin(name)
            .ok_or_else(|| zerror!("Plugin `{}`: missing config", name))?;
        info!("{:?}", plugin_conf.clone());
        let config: Config = serde_json::from_value(plugin_conf.clone())
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
        async_std::task::spawn(run(runtime.clone(), config));
        Ok(Box::new(MAVLinkPlugin))
    }
}

impl RunningPluginTrait for MAVLinkPlugin {}

pub async fn run(runtime: Runtime, config: Config) {
    debug!(
        "Zenoh MAVLink plugin {}",
        MAVLinkPlugin::PLUGIN_LONG_VERSION
    );
    debug!("Zenoh MAVLink plugin {:?}", config);

    // open zenoh-net Session
    let zsession = match zenoh::init(runtime).res_async().await {
        Ok(session) => Arc::new(session),
        Err(e) => {
            error!("Unable to init zenoh session for ZeroMQ plugin: {e:?}");
            return;
        }
    };

    // create group member using the group_member_id if configured, or the Session ID otherwise
    let member_id = match config.group_member_id {
        Some(ref id) => id.clone(),
        None => zsession.zid().into_keyexpr(),
    };
    let member = match zsession
        .liveliness()
        .declare_token(
            keyexpr::new(KE_PREFIX_LIVELINESS_GROUP).unwrap() / &zsession.zid().into_keyexpr(),
        )
        .res_async()
        .await
    {
        Ok(member) => member,
        Err(e) => {
            error!("Unable to declare liveliness token for ZeroMQ plugin: {e:?}");
            return;
        }
    };

    let mav_plugin = MAVLinkPluginRuntime {
        config,
        zsession: &zsession,
        _member: member,
        _member_id: member_id,
    };

    mav_plugin.run().await;
}

pub(crate) struct MAVLinkPluginRuntime<'a> {
    config: Config,
    // Note: &'a Arc<Session> here to keep the ownership of Session outside this struct
    // and be able to store the publishers/subscribers it creates in this same struct.
    zsession: &'a Arc<Session>,
    _member: LivelinessToken<'a>,
    _member_id: OwnedKeyExpr,
}

impl<'a> MAVLinkPluginRuntime<'a> {
    async fn run(&self) {
        // spawn broadcast channel
        let (tx, mut rx) =
            tokio::sync::broadcast::channel::<Protocol>(self.config.broadcast_channel_capacity);

        // spawn task for each mavlink connection
        let mut set = JoinSet::new();
        for mav_conn in self.config.mavlink_connections.clone() {
            debug!("spawning task for {mav_conn:?}");
            set.spawn(mav_conn.handle((tx.clone(), rx.resubscribe())));
        }

        // launch task to handle outgoing data for the zenoh network
        let zsession = self.zsession.clone();
        tokio::spawn(async move {
            let publisher = zsession
                .declare_publisher(ke!("mavlink/out"))
                .res_async()
                .await
                .unwrap();

            match rx.recv().await {
                Ok(msg) => {
                    publisher
                        .put("TODO: publish mavlink message")
                        .res_async()
                        .await
                        .unwrap();
                    debug!("forwarded message from broadcast channel to zenoh");
                }
                Err(e) => {
                    error!("failed to read from broadcast channel: {e}");
                }
            }
        })
        .instrument(info_span!("zenoh_pub_mav_out"))
        .await
        .unwrap();

        // launch task to handle incoming data for the zenoh network
        let zsession = self.zsession.clone();
        tokio::spawn(async move {
            let subscriber = zsession
                .declare_subscriber(ke!("mavlink/in"))
                .res_async()
                .await
                .unwrap();

            while let Ok(sample) = subscriber.recv_async().await {
                debug!("received message from zenoh");
                // TODO: serialize to mavlink raw?
                // TODO: tx.send(msg)
                debug!("forwarded message from zenoh to broadcast channel");
            }
        })
        .instrument(info_span!("zenoh_sub_mav_in"))
        .await
        .unwrap();

        // only abort if all mavlink tasks were shutdown for some reason
        while let Some(res) = set.join_next().await {
            error!("task ended because: {res:?}");
        }

        error!("all connections aborted!");
    }
}
