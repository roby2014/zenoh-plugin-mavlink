use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liveliness::{ke_liveliness_plugin, ke_liveliness_pub, ke_liveliness_sub};
use protocol::Protocol;
use tokio::task::{JoinHandle, JoinSet};
use tracing::{debug, error, info};
use tracing::{info_span, Instrument};
use zenoh::{
    internal::{
        plugins::{RunningPlugin, RunningPluginTrait, ZenohPlugin},
        runtime::Runtime,
        zerror,
    },
    key_expr::format::keformat,
    liveliness::LivelinessToken,
    Result as ZResult, Session,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};

pub mod config;
pub mod liveliness;
pub mod mavlink_connection;
pub mod protocol;
use config::Config;

lazy_static::lazy_static! {
    static ref WORK_THREAD_NUM: AtomicUsize = AtomicUsize::new(config::DEFAULT_WORK_THREAD_NUM);
    static ref MAX_BLOCK_THREAD_NUM: AtomicUsize = AtomicUsize::new(config::DEFAULT_MAX_BLOCK_THREAD_NUM);
    // The global runtime is used in the dynamic plugins, which we can't get the current runtime
    static ref TOKIO_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
               .worker_threads(WORK_THREAD_NUM.load(Ordering::SeqCst))
               .max_blocking_threads(MAX_BLOCK_THREAD_NUM.load(Ordering::SeqCst))
               .enable_all()
               .build()
               .expect("Unable to create runtime");
}
#[inline(always)]
pub(crate) fn spawn_runtime<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), spawn on the current runtime
            rt.spawn(task)
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), spawn on the global runtime
            TOKIO_RUNTIME.spawn(task)
        }
    }
}

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
        zenoh::try_init_log_from_env();

        let runtime_conf = runtime.config().lock();
        let plugin_conf = runtime_conf
            .plugin(name)
            .ok_or_else(|| zerror!("Plugin `{}`: missing config", name))?;
        info!("{:?}", plugin_conf.clone());
        let config: Config = serde_json::from_value(plugin_conf.clone())
            .map_err(|e| zerror!("Plugin `{}` configuration error: {}", name, e))?;
        WORK_THREAD_NUM.store(config.work_thread_num, Ordering::SeqCst);
        MAX_BLOCK_THREAD_NUM.store(config.max_block_thread_num, Ordering::SeqCst);

        spawn_runtime(run(runtime.clone(), config));
        Ok(Box::new(MAVLinkPlugin))
    }
}

impl RunningPluginTrait for MAVLinkPlugin {}

#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(MAVLinkPlugin);

pub async fn run(runtime: Runtime, config: Config) {
    debug!(
        "Zenoh MAVLink plugin {}",
        MAVLinkPlugin::PLUGIN_LONG_VERSION
    );
    debug!("Zenoh MAVLink plugin {:?}", config);

    // open zenoh-net Session
    let zsession = match zenoh::session::init(runtime).await {
        Ok(session) => Arc::new(session),
        Err(e) => {
            error!("Unable to init zenoh session for MAVLink plugin: {e:?}");
            return;
        }
    };

    // Declare plugin's liveliness token
    let ke_liveliness = keformat!(
        ke_liveliness_plugin::formatter(),
        zenoh_id = zsession.zid().into_keyexpr()
    )
    .unwrap();
    let member = match zsession.liveliness().declare_token(ke_liveliness).await {
        Ok(member) => member,
        Err(e) => {
            tracing::error!(
                "Unable to declare liveliness token for MAVLink plugin : {:?}",
                e
            );
            return;
        }
    };

    let mav_plugin = MAVLinkPluginRuntime {
        config: Arc::new(config),
        zsession,
        _member: member,
    };

    mav_plugin.run().await;
}

pub(crate) struct MAVLinkPluginRuntime {
    config: Arc<Config>,
    zsession: Arc<Session>,
    _member: LivelinessToken,
}

impl MAVLinkPluginRuntime {
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
            let ke = keformat!(ke_liveliness_pub::formatter(), zenoh_id = "*",).unwrap();
            let publisher = zsession.declare_publisher(ke).await.unwrap();

            match rx.recv().await {
                Ok(msg) => {
                    publisher
                        .put("TODO: publish mavlink message")
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
            let ke = keformat!(ke_liveliness_sub::formatter(), zenoh_id = "*",).unwrap();
            let subscriber = zsession.declare_subscriber(ke).await.unwrap();

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
