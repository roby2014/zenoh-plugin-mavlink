use zenoh::config::Config;
use zenoh_plugin_mavlink::mavlink_connection::MAVLinkConnection;
use zenoh_plugin_trait::Plugin;

use crate::zenoh_args::CommonArgs;

//
// All Bridge arguments
//
#[derive(clap::Parser, Clone, Debug)]
#[command(version=zenoh_plugin_mavlink::MAVLinkPlugin::PLUGIN_VERSION,
    long_version=zenoh_plugin_mavlink::MAVLinkPlugin::PLUGIN_LONG_VERSION,
    about="Zenoh bridge for MAVLink",
)]
pub struct BridgeArgs {
    #[command(flatten)]
    pub session_args: CommonArgs,

    #[arg(short, long, num_args = 0.., value_parser(parse_mavlink_connection_vec))]
    pub mavlink_connections: Option<std::vec::Vec<MAVLinkConnection>>,

    #[arg(short, long)]
    pub broadcast_channel_capacity: Option<usize>,

    // Configures HTTP interface for the REST API (disabled by default, setting this option enables it). Accepted values:
    ///  - a port number
    ///  - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface).
    #[arg(short, long, value_name = "PORT | IP:PORT", verbatim_doc_comment)]
    pub rest_http_port: Option<String>,

    /// Experimental!! Run a watchdog thread that monitors the bridge's async executor and
    /// reports as error log any stalled status during the specified period [default: 1.0 second]
    #[arg(short, long, value_name = "FLOAT", default_missing_value = "1.0")]
    pub watchdog: Option<Option<f32>>,
}

fn parse_mavlink_connection_vec(val: &str) -> Result<Vec<MAVLinkConnection>, String> {
    serde_json::from_str(val).map_err(|e| e.to_string())?
}

impl From<BridgeArgs> for Config {
    fn from(value: BridgeArgs) -> Self {
        (&value).into()
    }
}

impl From<&BridgeArgs> for Config {
    fn from(args: &BridgeArgs) -> Self {
        let mut config = (&args.session_args).into();

        insert_json5_option(&mut config, "plugins/mavlink/broadcast_channel_capacity", &args.broadcast_channel_capacity);
        // FIXME: fix settings. unable to pass vector via CLI
        insert_json5_option(
            &mut config,
            "plugins/mavlink/mavlink_connections",
            &args.mavlink_connections,
        );
  
        insert_json5_option(&mut config, "plugins/rest/http_port", &args.rest_http_port);

        config
    }
}

pub(crate) fn insert_json5<T>(config: &mut Config, key: &str, value: &T)
where
    T: Sized + serde::Serialize,
{
    config
        .insert_json5(key, &serde_json::to_string(value).unwrap())
        .unwrap();
}

pub(crate) fn insert_json5_option<T>(config: &mut Config, key: &str, value: &Option<T>)
where
    T: Sized + serde::Serialize,
{
    if let Some(v) = value {
        config
            .insert_json5(key, &serde_json::to_string(v).unwrap())
            .unwrap();
    }
}

pub(crate) fn insert_json5_list<T>(config: &mut Config, key: &str, values: &Vec<T>)
where
    T: Sized + serde::Serialize,
{
    if !values.is_empty() {
        config
            .insert_json5(key, &serde_json::to_string(values).unwrap())
            .unwrap();
    }
}
