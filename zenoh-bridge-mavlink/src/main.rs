use std::time::{Duration, SystemTime};

use async_liveliness_monitor::LivelinessMonitor;
use bridge_args::BridgeArgs;
use clap::Parser;
use zenoh::{
    config::{self, Config},
    internal::{plugins::PluginsManager, runtime::RuntimeBuilder},
};
use zenoh_config::ModeDependentValue;
use zenoh_plugin_trait::Plugin;

mod zenoh_args;
mod bridge_args;

const MAVLINK_ARG_START_FLAG: &str = "--mavlink-args";
const MAVLINK_ARG_END_FLAG: &str = "--";

fn parse_args() -> (Option<f32>, Config) {
    let mut mavlink_args = vec!["mavlink-args".to_string()];
    let mut user_args = Vec::new();
    let mut in_mavlink_args_section = false;
    for arg in std::env::args() {
        match arg.as_str() {
            MAVLINK_ARG_START_FLAG => in_mavlink_args_section = true,
            MAVLINK_ARG_END_FLAG => in_mavlink_args_section = false,
            _ if in_mavlink_args_section => mavlink_args.push(arg),
            _ => user_args.push(arg),
        }
    }

    // Create config parsing user-defined args
    let bridge_args = BridgeArgs::parse_from(user_args);
    let watchdog_opt = bridge_args.watchdog.flatten();
    let mut config: Config = bridge_args.into();

    // Always add timestamps to publications (required for PublicationCache used in case of TRANSIENT_LOCAL topics)
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();

    // Enable admin space
    config.adminspace.set_enabled(true).unwrap();
    // Enable loading plugins
    config.plugins_loading.set_enabled(true).unwrap();

    (watchdog_opt, config)
}

#[tokio::main]
async fn main() {
    zenoh::init_log_from_env_or("z=info");
    tracing::info!(
        "zenoh-bridge-mavlink {}",
        zenoh_plugin_mavlink::MAVLinkPlugin::PLUGIN_LONG_VERSION
    );

    let (watchdog_period, config) = parse_args();
    tracing::info!("Zenoh {config:?}");

    if let Some(period) = watchdog_period {
        run_watchdog(period);
    }

    let mut plugins_mgr = PluginsManager::static_plugins_only();

    // declare REST plugin if specified in conf
    if config.plugin("rest").is_some() {
        plugins_mgr.declare_static_plugin::<zenoh_plugin_rest::RestPlugin, &str>("rest", true);
    }

    // declare MAVLink plugin
    plugins_mgr.declare_static_plugin::<zenoh_plugin_mavlink::MAVLinkPlugin, &str>("mavlink", true);

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

    futures::future::pending::<()>().await;
}

fn run_watchdog(period: f32) {
    let sleep_time = Duration::from_secs_f32(period);
    // max delta accepted for watchdog thread sleep period
    let max_sleep_delta = Duration::from_millis(50);
    // 1st threshold of duration since last report => debug info if exceeded
    let report_threshold_1 = Duration::from_millis(10);
    // 2nd threshold of duration since last report => debug warn if exceeded
    let report_threshold_2 = Duration::from_millis(100);

    assert!(
        sleep_time > report_threshold_2,
        "Watchdog period must be greater than {} seconds",
        report_threshold_2.as_secs_f32()
    );

    // Start a Liveliness Monitor thread for tokio Runtime
    let (_task, monitor) = LivelinessMonitor::start(tokio::task::spawn);
    std::thread::spawn(move || {
        tracing::debug!(
            "Watchdog started with period {} sec",
            sleep_time.as_secs_f32()
        );
        loop {
            let before = SystemTime::now();
            std::thread::sleep(sleep_time);
            let elapsed = SystemTime::now().duration_since(before).unwrap();

            // Monitor watchdog thread itself
            if elapsed > sleep_time + max_sleep_delta {
                tracing::warn!(
                    "Watchdog thread slept more than configured: {} seconds",
                    elapsed.as_secs_f32()
                );
            }
            // check last LivelinessMonitor's report
            let report = monitor.latest_report();
            if report.elapsed() > report_threshold_1 {
                if report.elapsed() > sleep_time {
                    tracing::error!(
                        "Watchdog detecting tokio is stalled! No task scheduling since {} seconds",
                        report.elapsed().as_secs_f32()
                    );
                } else if report.elapsed() > report_threshold_2 {
                    tracing::warn!(
                        "Watchdog detecting tokio was not scheduling tasks during the last {} ms",
                        report.elapsed().as_micros()
                    );
                } else {
                    tracing::info!(
                        "Watchdog detecting tokio was not scheduling tasks during the last {} ms",
                        report.elapsed().as_micros()
                    );
                }
            }
        }
    });
}
