
use std::sync::Arc;
use btleplug::api::{Central, CentralEvent, CentralState, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, PeripheralId};
use futures::StreamExt;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use getrandom::getrandom;

use crate::application::TGSApplication;
use crate::blecar::{TGSBleCar,ble_name_for_vin, TGSTeslaState};
use crate::hex_utils::*;

// ──────────────────────────────────────────────────────────────────────────────
// Error type
// ──────────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────────
// Shared state
// ──────────────────────────────────────────────────────────────────────────────

// ──────────────────────────────────────────────────────────────────────────────
// BLE discoverer
// ──────────────────────────────────────────────────────────────────────────────

pub struct TGSBLEDiscoverer 
{
    pub is_scanning:bool,
    pub client_public_key:Vec<u8>,
    pub client_private_key:Vec<u8>,
    pub from_destination_address:[u8; 16],
    pub car:Option<TGSBleCar>,

}

pub async fn ble_discoverer_run(state: Arc<TGSApplication>) {
    debug!("[ble:ble_discoverer_run] Enter");

    let vin = state.runtime.read().await.config.tesla_vin.clone();

    let Some(vin) = vin else {
        info!("[ble:ble_discoverer_run] No VIN configured, BLE discovery skipped");
        return;
    };

    let expected_name = ble_name_for_vin(&vin);
    info!("[ble:ble_discoverer_run] Looking for Tesla BLE name '{}'", expected_name);

    let mut discoverer = TGSBLEDiscoverer::new();
    if let Err(e) = discoverer.scan_for_vehicles(state, expected_name).await {
        tracing::error!("[ble:ble_discoverer_run] {}", e);
    }
}

impl TGSBLEDiscoverer {
    pub fn new() -> Self {
        debug!("[ble:BLEDiscoverer::new] Enter");
        let mut from_destination_address = [0u8; 16];
        getrandom::getrandom(&mut from_destination_address).unwrap(); 
        TGSBLEDiscoverer { is_scanning: false,
            client_private_key: Vec::<u8>::from_hexa_string("e249a3e97df9a8cc8341be3b6c46982db6320c46a690597ea1d028f34ce908d0").unwrap(),
            client_public_key:Vec::<u8>::from_hexa_string("044c8c63e21f173074c56a073aa250a1fd72c49fc2945d1ee7c4548afaba3f69c340b32679785603172f625f37cd1ebab0788fef4639706a794b399118340b3467").unwrap(),
            from_destination_address:from_destination_address,
            car:None,
        }
    }

    async fn scan_for_vehicles(&mut self, state: Arc<TGSApplication>, expected_name: String) -> Result<(), BleError> {
        println!("[ble:scan_for_vehicles] Enter");

        let manager = Manager::new().await.map_err(|e| BleError::DiscoveryFailed(format!("BLE manager init failed: {}", e)))?;
        let adapters = manager.adapters().await.map_err(|e| BleError::DiscoveryFailed(format!("Adapter list failed: {}", e)))?;
        let adapter = adapters.into_iter().next().ok_or_else(|| BleError::DiscoveryFailed("No Bluetooth adapter found".to_string()))?;

        adapter.start_scan(ScanFilter::default()).await
            .map_err(|e| BleError::DiscoveryFailed(format!("Scan start failed: {}", e)))?;

        let mut events = adapter.events().await
            .map_err(|e| BleError::DiscoveryFailed(format!("Event stream failed: {}", e)))?;


        let mut devices_to_ignore: HashSet<PeripheralId> = HashSet::new();
        let mut tesla_id: Option<PeripheralId> = None;

        while let Some(event) = events.next().await {

            match event {
                CentralEvent::DeviceDiscovered(ref id)  => {
                    if let Ok(peripheral) = adapter.peripheral(&id).await {
                        if let Ok(Some(properties)) = peripheral.properties().await {
                            if devices_to_ignore.contains(&id) {
                                continue;
                            }
                            if properties.local_name.is_none() {
                                devices_to_ignore.insert(id.clone());            
                            }

                        }
                    }
                },
                CentralEvent::DeviceDisconnected(ref id) => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }
                    //println!("= {:?}", event);

                },
                CentralEvent::DeviceConnected(ref id) => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }
                    //println!("= {:?}", event);
                },

                CentralEvent::DeviceUpdated(ref id) => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }
                    //println!("= {:?}", event);:w
                    //:w
                    //
                },

                CentralEvent::ManufacturerDataAdvertisement{ref id, ref manufacturer_data }  => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }

                    if let Ok(peripheral) = adapter.peripheral(&id).await {
                        if let Ok(Some(properties)) = peripheral.properties().await {
                            //println!("= {:?}", event);
                            //println!("= {:?}", manufacturer_data);
                            //println!("Name is {:?}", properties);
                            if let Some(advertising_name) = properties.advertisement_name {
                                if advertising_name == expected_name {
                                    if self.car.is_none() {
                                        let vin = state.runtime.read().await.config.tesla_vin.clone();
                                        self.car = Some(TGSBleCar {
                                            vin: vin.unwrap(),  
                                            bluetooth_advertising_name: advertising_name.clone(),
                                            client_public_key:self.client_public_key.clone(),
                                            display_name:"Car".to_string(),
                                            client_private_key:self.client_private_key.clone(),
                                            from_destination_address:self.from_destination_address.clone(),

                                            state:TGSTeslaState::default(),
                                        });
                                        println!("WE FOUND HE TSLA");
                                    }
                                }
                            }
                        }
                    }
                },
                CentralEvent::ServiceDataAdvertisement{ref id, ref service_data} => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }

                    if self.car.is_none() {
                        continue;
                    }

                    println!("= {:?}", event);
                },
                CentralEvent::ServicesAdvertisement{ref id , ref services}  => {
                    if devices_to_ignore.contains(&id) {
                        continue;
                    }
                    if self.car.is_none() {
                        continue;
                    }
                    println!("= {:?}", event);
                },
                CentralEvent::StateUpdate(state) => {
                    match state {
                        CentralState::PoweredOff => {
                            info!("[ble:scan_for_vehicles] Adapter powered off");
                            devices_to_ignore.clear();
                            self.car = None;
                        }
                        CentralState::PoweredOn => {
                            info!("[ble:scan_for_vehicles] Adapter powered back on, resuming scan");
                            if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                                tracing::error!("[ble:scan_for_vehicles] Failed to resume scan: {}", e);
                            }
                        }
                        CentralState::Unknown => {}
                    }
                },

                CentralEvent::DeviceServicesModified(_)| CentralEvent::RssiUpdate { .. } => {
                }

            }
        }

        Err(BleError::DiscoveryFailed("BLE event stream ended".to_string()))
    }
}
