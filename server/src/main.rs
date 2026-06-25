mod application;
mod blediscoverer;
mod blecar;
mod configuration;
mod dispatcher;
mod fronius;
mod http_api;
mod mdns;
mod protocol;
mod runtime;

use std::sync::Arc;

use clap::Parser;
use tracing::{debug, info};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, reload, util::SubscriberInitExt};

use crate::application::{TGSApplication, LogLevelSetter};
use crate::configuration::TGSDispatcherConfiguration;
use crate::dispatcher::dispatcher_run;
use crate::fronius::fronius_discovery_run;
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
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
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
    /// Disable Bonjour/mDNS advertising
    #[arg(long)]
    no_bonjour: bool,
}

#[tokio::main]
async fn main() {
    let args = TGSToolArgs::parse();

    let level = std::env::var("TWOGREENSUNS_LOG").ok().and_then(|v| v.parse::<LevelFilter>().ok()).unwrap_or(LevelFilter::INFO);

    let (filter_layer, reload_handle) = reload::Layer::new(level);
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer().event_format(AlignedFormatter))
        .init();

    let log_level_setter: LogLevelSetter = Arc::new(move |level_str: &str| {
        let new_level: LevelFilter = level_str
            .parse()
            .map_err(|_| format!("invalid log level: '{}'", level_str))?;
        reload_handle.reload(new_level).map_err(|e| e.to_string())
    });

    debug!("[main:main] Enter");

    TGSDispatcherConfiguration::ensure_directory_configuration();
    let config = TGSDispatcherConfiguration::load_last_configuration();

    // Start with no inverter address — fronius_discovery_run will find it
    // asynchronously and notify the dispatcher when ready.
    let runtime = TGSDispatcherRuntime::new(config, String::new());
    let port = runtime.tgsbox_server_httport;
    let hostname = runtime.tgsbox_server_hostname.clone();
    let state = Arc::new(TGSApplication::new(runtime, log_level_setter));

    if args.no_bonjour {
        info!("[main:main] Bonjour/mDNS disabled");
        tokio::join!(
            async { fronius_discovery_run(state.clone()).await },
            async { dispatcher_run(state.clone()).await },
            async { http_api_run(state.clone(), port).await },
            async { ble_discoverer_run().await },
        );
    } else {
        debug!("[main:main] Spawning all tasks");
        tokio::join!(
            async { mdns_run(hostname, port).await },
            async { fronius_discovery_run(state.clone()).await },
            async { dispatcher_run(state.clone()).await },
            async { http_api_run(state.clone(), port).await },
            async { ble_discoverer_run().await },
        );
    }
}
