mod application;
mod blediscoverer;
mod blecar;
mod configuration;
mod dispatcher;
mod fronius;
mod http_api;
mod mdns;
mod runtime;
mod hex_utils;
mod tesla;

use std::sync::Arc;

use clap::Parser;
use tracing::{debug, info};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, reload, util::SubscriberInitExt};

use crate::application::{TGSApplication, LogLevelSetterFunction};
use crate::configuration::TGSDispatcherConfiguration;
use crate::dispatcher::dispatcher_run;
use crate::fronius::inverter_discoverer_run;
use crate::http_api::http_api_run;
use crate::blediscoverer::ble_discoverer_run;
use crate::mdns::mdns_run;
use crate::runtime::TGSDispatcherRuntime;

use tracing_subscriber::fmt::{self, format::Writer, FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

struct AlignedFormatter;

impl<S, N> FormatEvent<S, N> for AlignedFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event( &self, ctx: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &tracing::Event<'_>) -> std::fmt::Result {
        let meta = event.metadata();
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
        // pad level to 5, target to 35
        write!(writer, "{}  {:>5} {:<35} ", ts, meta.level(), meta.target())?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

#[derive(Parser)]
#[command(about = "TwoGreenSuns home energy management server")]
struct TGSToolArgs {
}

#[tokio::main]
async fn main() {
    let args = TGSToolArgs::parse();

        //Read config and set runtime
    TGSDispatcherConfiguration::ensure_directory_configuration();
    let config = TGSDispatcherConfiguration::load_last_configuration();
    let runtime = TGSDispatcherRuntime::new(config, String::new());

        //init log level and put layer
    let level = runtime.tgsbox_log_level;
    info!("level is {level}");
    let (filter_layer, reload_handle) = reload::Layer::new(level);
    tracing_subscriber::registry().with(filter_layer).with(fmt::layer().event_format(AlignedFormatter)).init();

    let log_level_setter: LogLevelSetterFunction = Arc::new(move |level_str: &str| {
        let new_level: LevelFilter = level_str.parse().map_err(|_| format!("invalid log level: '{}'", level_str))?;
        reload_handle.reload(new_level).map_err(|e| e.to_string())
    });

    info!("[main:main] Enter: log level is {level}");
    info!("[runtime:new] platform={} hostname={} inverter={} http_port={}",
        runtime.tgsbox_platform, runtime.tgsbox_server_hostname,
        runtime.tgsdevice_inverter_ipv4_address, runtime.tgsbox_server_httport);

    let tgsbox_port = runtime.tgsbox_server_httport;
    let tgsbox_hostname = runtime.tgsbox_server_hostname.clone();
    let (blecar_commandchannel_tx, blecar_commandchannel_rx) = tokio::sync::mpsc::channel(32);
    let state = Arc::new(TGSApplication::new(runtime, log_level_setter, blecar_commandchannel_tx));

    debug!("[main:main] Spawning all tasks");
    tokio::join!(
            async { mdns_run(tgsbox_hostname, tgsbox_port).await },
            async { inverter_discoverer_run(state.clone()).await },
            async { dispatcher_run(state.clone()).await },
            async { http_api_run(state.clone(), tgsbox_port).await },
            async { ble_discoverer_run(state.clone(), blecar_commandchannel_rx).await },
        );
}
