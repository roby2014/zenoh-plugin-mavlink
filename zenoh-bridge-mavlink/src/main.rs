use std::str::FromStr;

use clap::{arg, Arg, Command};
use config::ModeDependentValue;
use zenoh::prelude::r#async::*;
use zenoh::{config::ZenohId, runtime::RuntimeBuilder};
use zenoh_plugin_mavlink::MAVLinkPlugin;
use zenoh_plugin_trait::{Plugin, PluginsManager};

macro_rules! insert_json5 {
    ($config: expr, $args: expr, $key: expr, if $name: expr) => {
        if $args.contains_id($name) {
            $config.insert_json5($key, &serde_json::to_string(&$args.get_one::<bool>($name).unwrap()).unwrap()).unwrap();
        }
    };
    ($config: expr, $args: expr, $key: expr, if $name: expr, $($t: tt)*) => {
        if $args.contains_id($name) {
            $config.insert_json5($key, &serde_json::to_string(&$args.get_one::<String>($name).unwrap()$($t)*).unwrap()).unwrap();
        }
    };
    ($config: expr, $args: expr, $key: expr, for $name: expr, $($t: tt)*) => {
        if let Some(value) = $args.get_many::<String>($name) {
            $config.insert_json5($key, &serde_json::to_string(&value$($t)*).unwrap()).unwrap();
        }
    };
}

fn parse_args() -> zenoh::config::Config {
    let mut app = Command::new("Zenoh bridge for MAVLink")
        .version(MAVLinkPlugin::PLUGIN_VERSION)
        .long_version(MAVLinkPlugin::PLUGIN_LONG_VERSION)
        //
        // zenoh related arguments:
        //
        .arg(arg!(-i --id [HEX_STRING] "The identifier (as an hexadecimal string, with odd number of chars - e.g.: 0A0B23...) that zenohd must use.
WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!
If not set, a random UUIDv4 will be used."
            ))
        .arg(arg!(-m --mode [MODE]  "The zenoh session mode.")
            .value_parser(["peer", "client"])
            .default_value("peer")
        )
        .arg(arg!(-c --config [FILE] "The configuration file. Currently, this file must be a valid JSON5 file."))
        .arg(arg!(-l --listen [ENDPOINT]... "A locator on which this Zenoh router will listen for incoming sessions.
Repeat this option to open several listeners."
            ))
        .arg(arg!(-e --connect [ENDPOINT]... "A peer locator this Zenoh router will try to connect to.
Repeat this option to connect to several peers."
            ))
        .arg(Arg::new("no-multicast-scouting").long("no-multicast-scouting").help("By default the zenoh bridge listens and replies to UDP multicast scouting messages for being discovered by peers and routers.\nThis option disables this feature."))
        .arg(Arg::new("rest-http-port").long("rest-http-port").value_names(["PORT","IP:PORT"]).help("Configures HTTP interface for the REST API (disabled by default, setting this option enables it). Accepted values:'\n\t- a port number\n\t- a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)."));

    // Plugin-specific parameters
    app = app.arg(
        Arg::new("fcs-endpoints")
            .short('f')
            .long("fcs-endpoints")
            .value_name("JSON")
            .help("A JSON array of FCS endpoints"),
    );

    let args = app.get_matches();

    // load config file at first
    let mut config = match args.get_one::<String>("config") {
        Some(conf_file) => zenoh::config::Config::from_file(conf_file).unwrap(),
        None => zenoh::config::Config::default(),
    };
    // if "mavlink" plugin conf is not present, add it (empty to use default config)
    if config.plugin("mavlink").is_none() {
        config.insert_json5("plugins/mavlink", "{}").unwrap();
    }

    // apply zenoh related arguments over config
    // NOTE: only if args.occurrences_of()>0 to avoid overriding config with the default arg value
    if args.contains_id("id") {
        config
            .set_id(ZenohId::from_str(args.get_one::<String>("id").unwrap()).unwrap())
            .unwrap();
    }
    if args.contains_id("mode") {
        config
            .set_mode(Some(
                args.get_one::<String>("mode").unwrap().parse().unwrap(),
            ))
            .unwrap();
    }
    if let Some(endpoints) = args.get_many::<String>("connect") {
        config
            .connect
            .endpoints
            .extend(endpoints.map(|p| p.parse().unwrap()))
    }
    if let Some(endpoints) = args.get_many::<String>("listen") {
        config
            .listen
            .endpoints
            .extend(endpoints.map(|p| p.parse().unwrap()))
    }
    if args.contains_id("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }
    if let Some(port) = args.get_one::<String>("rest-http-port") {
        config
            .insert_json5("plugins/rest/http_port", &format!(r#""{port}""#))
            .unwrap();
    }
    // Always add timestamps to publications (required for PublicationCache used in case of TRANSIENT_LOCAL topics)
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();
    // Enable admin space
    config.adminspace.set_enabled(true).unwrap();
    // Enable loading plugins
    config.plugins_loading.set_enabled(true).unwrap();

    insert_json5!(config, args, "plugins/mavlink/fcs_endpoints", for "fcs-endpoints", .collect::<Vec<_>>());

    config
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    zenoh_util::init_log_from_env_or("z=info");
    tracing::info!(
        "zenoh-bridge-mavlink {}",
        zenoh_plugin_mavlink::MAVLinkPlugin::PLUGIN_LONG_VERSION
    );

    let config = parse_args();
    tracing::info!("Zenoh {config:?}");

    let mut plugins_mgr = PluginsManager::static_plugins_only();

    // declare REST plugin if specified in conf
    if config.plugin("rest").is_some() {
        plugins_mgr = plugins_mgr.declare_static_plugin::<zenoh_plugin_rest::RestPlugin, &str>(
            "zenoh_plugin_rest",
            true,
        );
    }

    // declare MAVLink plugin
    plugins_mgr = plugins_mgr
        .declare_static_plugin::<zenoh_plugin_mavlink::MAVLinkPlugin, &str>("mavlink", true);

    // create a zenoh Runtime.
    let mut runtime = match RuntimeBuilder::new(config)
        .plugins_manager(plugins_mgr)
        .build()
        .await
    {
        Ok(runtime) => runtime,
        Err(e) => {
            println!("{e}. Exiting...");
            std::process::exit(-1);
        }
    };
    if let Err(e) = runtime.start().await {
        println!("Failed to start Zenoh runtime: {e}. Exiting...");
        std::process::exit(-1);
    }

    std::future::pending::<Result<(), std::io::Error>>().await
}
