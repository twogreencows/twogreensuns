use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use tracing::debug;

use crate::ble::{BtleplugTesla, MockTesla, TeslaInterface, TeslaState};
use crate::configuration::TwoGreenSunsDispatcherConfiguration;
use crate::fronius::InverterData;

/// Closure that accepts a level name (e.g. "debug", "info") and reloads the
/// global tracing filter.  Stored in AppState so the HTTP API can call it.
pub type LogLevelSetter = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct AppState {
    pub config: Arc<RwLock<TwoGreenSunsDispatcherConfiguration>>,
    pub inverter_state: Arc<RwLock<Option<InverterData>>>,
    pub tesla_state: Arc<RwLock<TeslaState>>,
    pub tesla: Arc<Mutex<Box<dyn TeslaInterface>>>,
    pub log_level_setter: LogLevelSetter,
}

impl AppState {
    pub fn new(
        config: TwoGreenSunsDispatcherConfiguration,
        log_level_setter: LogLevelSetter,
    ) -> Self {
        debug!("[app_state:new] Enter");

        let use_mock = config.use_mock_ble;
        debug!("  --> use_mock_ble = {}", use_mock);

        let tesla: Box<dyn TeslaInterface> = if use_mock {
            debug!("  --> creating MockTesla");
            Box::new(MockTesla::new())
        } else {
            debug!("  --> creating BtleplugTesla");
            Box::new(BtleplugTesla::new())
        };

        debug!("[app_state:new] Exit");
        AppState {
            config: Arc::new(RwLock::new(config)),
            inverter_state: Arc::new(RwLock::new(None)),
            tesla_state: Arc::new(RwLock::new(TeslaState::default())),
            tesla: Arc::new(Mutex::new(tesla)),
            log_level_setter,
        }
    }
}
