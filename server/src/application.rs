use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

use tracing::debug;

use crate::blecar::{TGSTeslaState, TGSBleCarCommandChannelTx};
use crate::runtime::TGSDispatcherRuntime;
use crate::fronius::TGSInverterDataEntry;
use crate::storage::TGSStorage;

/// Closure that accepts a level name (e.g. "debug", "info") and reloads the
/// global tracing filter.  Stored in AppState so the HTTP API can call it.
pub type LogLevelSetterFunction = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct TGSApplication {
    //There should be inverter+inverter state for symetry (this is a device)
    //
    pub runtime: Arc<RwLock<TGSDispatcherRuntime>>,
    pub inverter_state: Arc<RwLock<Option<TGSInverterDataEntry>>>,
    pub tesla_state: Arc<RwLock<TGSTeslaState>>,

    /// Function to dynamically set the log level
    pub log_level_setter: LogLevelSetterFunction, //Function
                                                  //
    /// Notified when the Fronius inverter address becomes known.
    /// The dispatcher waits on this before starting its poll loop.
    pub inverter_address_notify: Arc<Notify>, //Notification channel
    
    /// Send requests channel to the BLE discoverer task (owns the connection + peripheral).
    pub blecar_commandchannel_tx: TGSBleCarCommandChannelTx,//Communication channel

    /// SQLite event history (one row per dispatcher poll cycle).
    pub storage: TGSStorage,
}

impl TGSApplication {
    pub fn new( runtime: TGSDispatcherRuntime, log_level_setter: LogLevelSetterFunction, blecar_commandchannel_tx: TGSBleCarCommandChannelTx, storage: TGSStorage) -> Self {
        debug!("[application:new] Enter");

        TGSApplication {
            runtime: Arc::new(RwLock::new(runtime)),
            inverter_state: Arc::new(RwLock::new(None)),
            tesla_state: Arc::new(RwLock::new(TGSTeslaState::default())),
            log_level_setter,
            inverter_address_notify: Arc::new(Notify::new()),
            blecar_commandchannel_tx,
            storage,
        }
    }
}
