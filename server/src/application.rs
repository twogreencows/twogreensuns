use std::sync::Arc;
use tokio::sync::{ Notify, RwLock};

use tracing::debug;

use crate::blecar::{TGSTeslaState};
use crate::runtime::TGSDispatcherRuntime;
use crate::fronius::TGSInverterDataEntry;

/// Closure that accepts a level name (e.g. "debug", "info") and reloads the
/// global tracing filter.  Stored in AppState so the HTTP API can call it.
pub type LogLevelSetter = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct TGSApplication {
    //There should be inverter+inverter state for symetry (this is a device)
    //
    pub runtime: Arc<RwLock<TGSDispatcherRuntime>>,
    pub inverter_state: Arc<RwLock<Option<TGSInverterDataEntry>>>,
    pub tesla_state: Arc<RwLock<TGSTeslaState>>,
    pub log_level_setter: LogLevelSetter,
    /// Notified when the Fronius inverter address becomes known.
    /// The dispatcher waits on this before starting its poll loop.
    pub inverter_address_notify: Arc<Notify>,
}

impl TGSApplication {
    pub fn new( runtime: TGSDispatcherRuntime, log_level_setter: LogLevelSetter) -> Self {
        debug!("[application:new] Enter");

        TGSApplication {
            runtime: Arc::new(RwLock::new(runtime)),
            inverter_state: Arc::new(RwLock::new(None)),
            tesla_state: Arc::new(RwLock::new(TGSTeslaState::default())),
            log_level_setter,
            inverter_address_notify: Arc::new(Notify::new()),
        }
    }
}
