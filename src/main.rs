mod app_state;
mod ble;
mod configuration;
mod dispatcher;
mod fronius;
mod http_api;
mod mdns;
mod protocol;
mod tesla;

use std::sync::Arc;

use tracing::debug;
use tracing_subscriber::{filter::LevelFilter, fmt, layer::SubscriberExt, reload, util::SubscriberInitExt};

use crate::app_state::{AppState, LogLevelSetter};
use crate::configuration::TwoGreenSunsDispatcherConfiguration;
use crate::dispatcher::dispatcher_run;
use crate::http_api::http_api_run;
use crate::mdns::mdns_run;

#[tokio::main]
async fn main() {
    let level = std::env::var("TWOGREENSUNS_LOG")
        .ok()
        .and_then(|v| v.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::DEBUG);

    let (filter_layer, reload_handle) = reload::Layer::new(level);
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer())
        .init();

    let log_level_setter: LogLevelSetter = Arc::new(move |level_str: &str| {
        let new_level: LevelFilter = level_str
            .parse()
            .map_err(|_| format!("invalid log level: '{}'", level_str))?;
        reload_handle.reload(new_level).map_err(|e| e.to_string())
    });

    debug!("[main:main] Enter");

    TwoGreenSunsDispatcherConfiguration::ensure_directory_configuration();
    let config = TwoGreenSunsDispatcherConfiguration::load_last_configuration();
    let port = config.http_port;
    let hostname = config.mdns_hostname.clone();

    let state = Arc::new(AppState::new(config, log_level_setter));

    debug!("[main:main] Spawning dispatcher, HTTP API, and mDNS");
    tokio::join!(
        dispatcher_run(state.clone()),
        http_api_run(state.clone(), port),
        mdns_run(hostname, port),
    );
}
