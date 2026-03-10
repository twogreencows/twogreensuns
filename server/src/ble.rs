use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Debug)]
pub enum BleError {
    DiscoveryFailed(String),
    ConnectionFailed(String),
    CommandFailed(String),
}

impl std::fmt::Display for BleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BleError::DiscoveryFailed(msg) => write!(f, "Discovery failed: {}", msg),
            BleError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            BleError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TeslaState {
    pub connected: bool,
    pub charging: bool,
    pub charging_amps: u8,
}

#[async_trait]
pub trait TeslaInterface: Send + Sync {
    async fn discover(&mut self, vin: &str) -> Result<(), BleError>;
    async fn connect(&mut self) -> Result<(), BleError>;
    async fn start_charging(&self) -> Result<(), BleError>;
    async fn stop_charging(&self) -> Result<(), BleError>;
    async fn set_charging_amps(&self, amps: u8) -> Result<(), BleError>;
    async fn status(&self) -> TeslaState;
}

pub struct MockTesla {
    pub state: TeslaState,
}

impl MockTesla {
    pub fn new() -> Self {
        debug!("[ble:MockTesla::new] Enter");
        debug!("[ble:MockTesla::new] Exit");
        MockTesla {
            state: TeslaState::default(),
        }
    }
}

#[async_trait]
impl TeslaInterface for MockTesla {
    async fn discover(&mut self, vin: &str) -> Result<(), BleError> {
        debug!("[ble:MockTesla::discover] Enter");
        debug!("  --> VIN = '{}'", vin);
        self.state.connected = true;
        info!("[ble:MockTesla::discover] Mock discovered VIN '{}'", vin);
        debug!("[ble:MockTesla::discover] Exit");
        Ok(())
    }

    async fn connect(&mut self) -> Result<(), BleError> {
        debug!("[ble:MockTesla::connect] Enter");
        self.state.connected = true;
        info!("[ble:MockTesla::connect] Mock connected");
        debug!("[ble:MockTesla::connect] Exit");
        Ok(())
    }

    async fn start_charging(&self) -> Result<(), BleError> {
        debug!("[ble:MockTesla::start_charging] Enter");
        info!("[ble:MockTesla::start_charging] Mock start charging");
        debug!("[ble:MockTesla::start_charging] Exit");
        Ok(())
    }

    async fn stop_charging(&self) -> Result<(), BleError> {
        debug!("[ble:MockTesla::stop_charging] Enter");
        info!("[ble:MockTesla::stop_charging] Mock stop charging");
        debug!("[ble:MockTesla::stop_charging] Exit");
        Ok(())
    }

    async fn set_charging_amps(&self, amps: u8) -> Result<(), BleError> {
        debug!("[ble:MockTesla::set_charging_amps] Enter");
        debug!("  --> amps = {}", amps);
        info!("[ble:MockTesla::set_charging_amps] Mock set charging amps to {}A", amps);
        debug!("[ble:MockTesla::set_charging_amps] Exit");
        Ok(())
    }

    async fn status(&self) -> TeslaState {
        debug!("[ble:MockTesla::status] Enter");
        debug!("  --> connected={} charging={} amps={}", self.state.connected, self.state.charging, self.state.charging_amps);
        debug!("[ble:MockTesla::status] Exit");
        self.state.clone()
    }
}

pub struct BtleplugTesla {
    // btleplug manager + peripheral — populated during discover/connect
}

impl BtleplugTesla {
    pub fn new() -> Self {
        debug!("[ble:BtleplugTesla::new] Enter");
        debug!("[ble:BtleplugTesla::new] Exit");
        BtleplugTesla {}
    }
}

#[async_trait]
impl TeslaInterface for BtleplugTesla {
    async fn discover(&mut self, _vin: &str) -> Result<(), BleError> {
        debug!("[ble:BtleplugTesla::discover] Enter");
        debug!("[ble:BtleplugTesla::discover] Exit (not yet implemented)");
        // TODO: implement btleplug BLE scan
        Err(BleError::DiscoveryFailed(
            "BtleplugTesla not yet implemented".to_string(),
        ))
    }

    async fn connect(&mut self) -> Result<(), BleError> {
        debug!("[ble:BtleplugTesla::connect] Enter");
        debug!("[ble:BtleplugTesla::connect] Exit (not yet implemented)");
        Err(BleError::ConnectionFailed(
            "BtleplugTesla not yet implemented".to_string(),
        ))
    }

    async fn start_charging(&self) -> Result<(), BleError> {
        debug!("[ble:BtleplugTesla::start_charging] Enter");
        debug!("[ble:BtleplugTesla::start_charging] Exit (not yet implemented)");
        Err(BleError::CommandFailed(
            "BtleplugTesla not yet implemented".to_string(),
        ))
    }

    async fn stop_charging(&self) -> Result<(), BleError> {
        debug!("[ble:BtleplugTesla::stop_charging] Enter");
        debug!("[ble:BtleplugTesla::stop_charging] Exit (not yet implemented)");
        Err(BleError::CommandFailed(
            "BtleplugTesla not yet implemented".to_string(),
        ))
    }

    async fn set_charging_amps(&self, _amps: u8) -> Result<(), BleError> {
        debug!("[ble:BtleplugTesla::set_charging_amps] Enter");
        debug!("[ble:BtleplugTesla::set_charging_amps] Exit (not yet implemented)");
        Err(BleError::CommandFailed(
            "BtleplugTesla not yet implemented".to_string(),
        ))
    }

    async fn status(&self) -> TeslaState {
        debug!("[ble:BtleplugTesla::status] Enter");
        debug!("[ble:BtleplugTesla::status] Exit");
        TeslaState::default()
    }
}
