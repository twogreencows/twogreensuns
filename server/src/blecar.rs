// Tesla BLE characteristic UUIDs and command encoding.
// These are stubs — actual values depend on the Tesla BLE protocol.
use sha1::Sha1;
use sha1::Digest;
use btleplug::api::Characteristic;
use serde::{Deserialize, Serialize};

pub const BLETESLA_VEHICLESERVICE_UUID: &str = "00000211-b2d1-43f0-9b88-960cebf8b91e";
pub const BLETESLA_TOVEHICLE_UUID: &str = "00000212-b2d1-43f0-9b88-960cebf8b91e";
pub const BLETESLA_FROMVEHICLE_UUID: &str = "00000213-b2d1-43f0-9b88-960cebf8b91e";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TGSTeslaState {
    pub connected: bool,
    pub plugged: bool,
    pub charging: bool,
    pub charging_amps: u8,
}


pub fn ble_name_for_vin(vin: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(vin.as_bytes());
    let digest = hasher.finalize();
    let hex = hex::encode(digest);
    format!("S{}C", &hex[..16])
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TGSTeslaCarCommand {
    WakeUp,
    StartCharging,
    StopCharging,
    SetChargingAmps(u8),
    SetChargeLimit(u8),
}

#[derive(Clone, Debug)]
pub enum TGSTeslaQueryState{
    NoAction,
    PendingSessionAnswer,
    PendingWakeUpAnswer,
    PendingCommandAnswer,
}

#[derive(Clone, Debug)]
pub struct TGSBleCar {
//displayName
 pub vin:String,
 pub bluetooth_advertising_name:String,
 pub display_name:String,
 pub client_public_key:Vec<u8>,
 pub client_private_key:Vec<u8>,
 pub from_destination_address:[u8; 16],

 pub bluetoothTX:Option<Characteristic>,
 pub bluetoothRX:Option<Characteristic>,
 pub bluetoothRssi: f32,

 pub state:TGSTeslaState,
}
