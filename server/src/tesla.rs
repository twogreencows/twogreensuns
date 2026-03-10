// Tesla BLE characteristic UUIDs and command encoding.
// These are stubs — actual values depend on the Tesla BLE protocol.

pub const TESLA_SERVICE_UUID: &str = "00000211-b2d1-43f0-9b88-960cebf8b91e";
pub const TESLA_CHARGING_CHAR_UUID: &str = "00000212-b2d1-43f0-9b88-960cebf8b91e";
pub const TESLA_STATUS_CHAR_UUID: &str = "00000213-b2d1-43f0-9b88-960cebf8b91e";

pub enum TeslaCommand {
    StartCharging,
    StopCharging,
    SetChargingAmps(u8),
}

pub fn tesla_encode_command(cmd: TeslaCommand) -> Vec<u8> {
    // TODO: implement actual Tesla BLE command encoding
    match cmd {
        TeslaCommand::StartCharging => vec![0x01],
        TeslaCommand::StopCharging => vec![0x00],
        TeslaCommand::SetChargingAmps(amps) => vec![0x02, amps],
    }
}
