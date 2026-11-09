//! BLE discoverer — the sole owner of the Bluetooth adapter and the Tesla
//! peripheral.
//!
//! One `select!` loop multiplexes three inputs:
//!   * `CentralEvent`s  — track presence (advertisements) and connection state
//!   * `TGSBleCarRequest`s  — reads and commands from the dispatcher and HTTP API
//!   * a 60s heartbeat  — drop the car once it has stopped advertising
//!
//! Requests use the Tokio actor pattern. Callers send
//! `(TGSBleCarRequest, oneshot::Sender<Result<TGSTeslaState, BleError>>)` down an
//! `mpsc`; this task is the only consumer, so every BLE operation is serialized
//! in FIFO order. Each request carries its own `oneshot` reply channel, so an
//! answer can only reach the caller that made that request — there is no shared
//! return path and no cross-talk. If this task drops a reply sender without
//! answering (shutdown), that caller's `.await` resolves to `Err(RecvError)`.

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashSet;

use btleplug::api::{Central, CentralEvent, CentralState, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral, PeripheralId};
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::application::TGSApplication;
use crate::blecar::{ble_name_for_vin, BleError, TGSBleCar, TGSTeslaState, TGSBleCarReply, TGSBleCarRequest};
use crate::hex_utils::*;

/// A car that stops advertising for this long (while not connected) is considered gone.
const GONE_TIMEOUT: Duration = Duration::from_secs(90);
/// How often the housekeeping tick runs.
const HEARTBEAT: Duration = Duration::from_secs(60);

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

pub async fn ble_discoverer_run( state: Arc<TGSApplication>, blecar_commandchannel_rx: mpsc::Receiver<(TGSBleCarRequest, TGSBleCarReply)>) {
    debug!("[ble:ble_discoverer_run] Enter");

    let vin = state.runtime.read().await.config.tesla_vin.clone();

    //filter on both None and is_empty
    let Some(vin) = vin.filter(|v| !v.is_empty())  else {
        info!("[ble:ble_discoverer_run] No VIN configured, BLE discovery skipped");
        return;
    };

    let expected_ble_advertising_name = ble_name_for_vin(&vin);
    info!("[ble:ble_discoverer_run] Looking for Tesla BLE name '{}'", expected_ble_advertising_name);

    let mut discoverer = TGSBLEDiscoverer::new();
    if let Err(e) = discoverer.run_car_link(state, expected_ble_advertising_name, blecar_commandchannel_rx).await {
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

    async fn run_car_link( &mut self, state: Arc<TGSApplication>, expected_name: String,
        mut blecar_commandchannel_rx: mpsc::Receiver<(TGSBleCarRequest, TGSBleCarReply)>,
    ) -> Result<(), BleError> {
        tracing::debug!("[ble:run_car_link] Enter");

        // start bluetooth stack
        let bt_manager = Manager::new().await.map_err(|e| BleError::DiscoveryFailed(format!("BLE manager init failed: {}", e)))?;
        let bt_adapters = bt_manager.adapters().await.map_err(|e| BleError::DiscoveryFailed(format!("Adapter list failed: {}", e)))?;
        let bt_adapter = bt_adapters.into_iter().next().ok_or_else(|| BleError::DiscoveryFailed("No Bluetooth adapter found".to_string()))?;

        bt_adapter.start_scan(ScanFilter::default()).await
            .map_err(|e| BleError::DiscoveryFailed(format!("Scan start failed: {}", e)))?;

        let mut bt_events = bt_adapter.events().await
            .map_err(|e| BleError::DiscoveryFailed(format!("Event stream failed: {}", e)))?;

        let mut bt_devices_to_ignore: HashSet<PeripheralId> = HashSet::new();

        // start heartbeat to see if car is still there
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.tick().await; // discard the immediate first tick

        // Cleared if every sender is dropped, which disables that select branch
        // so a closed channel does not spin the loop.
        let mut rx_open = true;

        loop {
            tokio::select! {
                // BLE adapter events: device discovered/updated/disconnected, adapter
                // powered on/off, advertisement data. Tracks Tesla presence/connection
                // state as it happens on the air, independent of any API request.
                maybe_bt_event = bt_events.next() => {
                    let Some(bt_event) = maybe_bt_event else { break };
                    self.handle_bluetooth_event(&bt_adapter, &state, &expected_name, &mut bt_devices_to_ignore, bt_event).await;
                }
                // API/dispatcher requests arriving over the actor channel (see module
                // doc comment): read the Tesla's charge state or apply a charging
                // command. This is the only place requests are serviced, so BLE
                // operations stay serialized in FIFO order against the events above.
                maybe_app_request = blecar_commandchannel_rx.recv(), if rx_open => {
                    match maybe_app_request {
                        Some((app_request, reply)) => {
                            let result = self.handle_application_request(&bt_adapter, &state, app_request).await;
                            let _ = reply.send(result);
                        }
                        None => {
                            info!("[ble:run_car_link] Tesla request channel closed");
                            rx_open = false;
                        }
                    }
                }
                // Housekeeping tick: drop the car record if it has stopped advertising
                // for GONE_TIMEOUT without being connected.
                _ = heartbeat.tick() => {
                    self.handle_housekeeping_heartbeat(&state).await;
                }
            }
        }

        Err(BleError::DiscoveryFailed("BLE event stream ended".to_string()))
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Event handling
    // ──────────────────────────────────────────────────────────────────────────

    async fn handle_bluetooth_event( &mut self, adapter: &Adapter, state: &Arc<TGSApplication>,
        expected_name: &str, bt_devices_to_ignore: &mut HashSet<PeripheralId>, event: CentralEvent) {

        match event {
            //we simply ignore device with no name as a commodity to reduce the noise
            CentralEvent::DeviceDiscovered(ref id) => {
                if bt_devices_to_ignore.contains(id) {
                    return;
                }
                let Ok(peripheral) = adapter.peripheral(id).await else { return };
                let Ok(Some(properties)) = peripheral.properties().await else { return };
                if properties.local_name.is_none() {
                    bt_devices_to_ignore.insert(id.clone());
                }
            }

            //A device is disconnected. If this is the car we simply mark it as disconected
            CentralEvent::DeviceDisconnected(ref id) => {
                if self.car_matches(id) {
                    info!("[ble:handle_bluetooth_event] Tesla GATT link dropped; keeping car, watching adverts");
                    if let Some(car) = self.car.as_mut() {
                        car.state.connected = false;
                        // fresh grace window: adverts must confirm the car is still around
                        car.last_seen = Instant::now();
                    }
                    self.mirror_tesla_state(state).await;
                    let _ = adapter.start_scan(ScanFilter::default()).await;
                }
            }

            CentralEvent::ManufacturerDataAdvertisement { ref id, .. } => {
                let Ok(peripheral) = adapter.peripheral(id).await else { return };
                let Ok(Some(properties)) = peripheral.properties().await else { return };
                if properties.advertisement_name.as_deref() == Some(expected_name) {
                    // A matching Tesla must never stay in the ignore set.
                    bt_devices_to_ignore.remove(id);
                    self.on_tesla_advert(adapter, id, true, state).await;
                } else if self.car_matches(id) && !bt_devices_to_ignore.contains(id) {
                    self.on_tesla_advert(adapter, id, false, state).await;
                }
            }

            CentralEvent::ServiceDataAdvertisement { ref id, .. }
            | CentralEvent::ServicesAdvertisement { ref id, .. }
            | CentralEvent::DeviceUpdated(ref id) => {
                if self.car_matches(id) {
                    self.on_tesla_advert(adapter, id, false, state).await;
                }
            }

            CentralEvent::DeviceConnected(_) => {}

            CentralEvent::StateUpdate(central_state) => {
                match central_state {
                    CentralState::PoweredOff => {
                        info!("[ble:handle_bluetooth_event] Adapter powered off");
                        bt_devices_to_ignore.clear();
                        self.car = None;
                        self.mirror_tesla_state(state).await;
                    }
                    CentralState::PoweredOn => {
                        info!("[ble:handle_bluetooth_event] Adapter powered back on, resuming scan");
                        if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
                            tracing::error!("[ble:handle_bluetooth_event] Failed to resume scan: {}", e);
                        }
                    }
                    CentralState::Unknown => {}
                }
            }

            CentralEvent::DeviceServicesModified(_) | CentralEvent::RssiUpdate { .. } => {}
        }
    }

    /// Refresh liveness for the Tesla; create the car record on first sighting.
    ///
    /// `name_matched` is true when this advertisement carried the expected Tesla
    /// name (vs. merely coming from the peripheral id we already track).
    async fn on_tesla_advert(&mut self, adapter: &Adapter, id: &PeripheralId, name_matched: bool, state: &Arc<TGSApplication>) {
        enum Action { Ignore, Refresh, Adopt, Create }

        let action = match &self.car {
            Some(car) if &car.peripheral_id == id => Action::Refresh,
            // Name matches but the id differs and no live connection: the
            // vehicle's resolvable BLE address rotated — adopt the new id.
            Some(car) if name_matched && !car.state.connected => Action::Adopt,
            Some(_) => Action::Ignore,
            None if name_matched => Action::Create,
            None => Action::Ignore,
        };

        match action {
            Action::Ignore => {}

            Action::Refresh => {
                let flipped = {
                    let car = self.car.as_mut().unwrap();
                    car.last_seen = Instant::now();
                    let was_present = car.state.present;
                    car.state.present = true;
                    !was_present
                };
                if flipped {
                    self.mirror_tesla_state(state).await;
                }
            }

            Action::Adopt => {
                {
                    let car = self.car.as_mut().unwrap();
                    info!("[ble:on_tesla_advert] Tesla BLE address changed {:?} -> {:?}", car.peripheral_id, id);
                    car.peripheral_id = id.clone();
                    car.last_seen = Instant::now();
                    car.state.present = true;
                }
                self.mirror_tesla_state(state).await;
            }

            Action::Create => {
                let vin = state.runtime.read().await.config.tesla_vin.clone().unwrap_or_default();
                info!("[ble:on_tesla_advert] Tesla found (peripheral {:?})", id);
                self.car = Some(TGSBleCar {
                    vin,
                    bluetooth_advertising_name: String::new(),
                    client_public_key: self.client_public_key.clone(),
                    display_name: "Car".to_string(),
                    client_private_key: self.client_private_key.clone(),
                    from_destination_address: self.from_destination_address,
                    bluetooth_tx: None,
                    bluetooth_rx: None,
                    bluetooth_rssi: 0.0,
                    vcsec_session: None,
                    infotainment_session: None,
                    peripheral_id: id.clone(),
                    last_seen: Instant::now(),
                    state: TGSTeslaState { present: true, ..Default::default() },
                });
                //println!("{:?}", self.car);
                //self.mirror_tesla_state(state).await;

                info!("[ble:on_tesla_advert] post car" );
                // Don't stay lazy — connect and fetch state right away instead
                // of waiting for the first request/poll tick.
                let peripheral = match self.ensure_connected(adapter, state).await {
                    Ok(peripheral) => peripheral,
                    Err(e) => {
                        warn!("[ble:on_tesla_advert] Eager connect failed: {}", e);
                        return;
                    }
                };
                if let Some(car) = self.car.as_mut() {
                    match car.read_charge_state(&peripheral).await {
                        Ok(tesla_state) => info!("[ble:on_tesla_advert] Eager read_charge_state: {:?}", tesla_state),
                        Err(e) => warn!("[ble:on_tesla_advert] Eager read_charge_state failed: {}", e),
                    }
                }
                self.mirror_tesla_state(state).await;
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Request handling
    // ──────────────────────────────────────────────────────────────────────────

    async fn handle_application_request( &mut self, adapter: &Adapter, state: &Arc<TGSApplication>, req: TGSBleCarRequest) -> Result<TGSTeslaState, BleError> {
        let peripheral = self.ensure_connected(adapter, state).await?;
        let snapshot = {
            let car = self.car.as_mut().ok_or_else(|| BleError::CommandFailed("no car present".to_string()))?;
            match req {
                TGSBleCarRequest::ReadState => car.read_charge_state(&peripheral).await?,
                TGSBleCarRequest::Action(cmd) => { car.apply(&peripheral, &cmd).await?; car.state.clone() }
            }
        };
        self.mirror_tesla_state(state).await;
        Ok(snapshot)
    }

    /// Make sure a GATT connection to the car is up, connecting if needed.
    async fn ensure_connected(&mut self, adapter: &Adapter, state: &Arc<TGSApplication>) -> Result<Peripheral, BleError> {
        debug!("[ble:ensure_connected] Enter");
        let Some(id) = self.car.as_ref().map(|c| c.peripheral_id.clone()) else {

            debug!("[ble:ensure_connected] No car present");
            return Err(BleError::ConnectionFailed("no car present".to_string()));
        };
        let peripheral = adapter.peripheral(&id).await
            .map_err(|e| BleError::ConnectionFailed(format!("peripheral lookup failed: {}", e)))?;
        debug!("peripheral {:?}", peripheral);
        if let Some(car) = self.car.as_mut() {
            car.ensure_connection(&peripheral).await?;
        }

        if let Some(car) = self.car.as_mut() {
            car.state.connected = true;
            car.last_seen = Instant::now();
        }
        self.mirror_tesla_state(state).await;
        Ok(peripheral)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Housekeeping
    // ──────────────────────────────────────────────────────────────────────────

    async fn handle_housekeeping_heartbeat(&mut self, state: &Arc<TGSApplication>) {
        let gone = self.car.as_ref().map(|c| !c.state.connected && c.last_seen.elapsed() > GONE_TIMEOUT).unwrap_or(false);
        if gone {
            info!("[ble:handle_housekeeping_heartbeat] Tesla gone (no advert for {}s) — clearing car", GONE_TIMEOUT.as_secs());
            self.car = None;
            self.mirror_tesla_state(state).await;
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Helpers
    // ──────────────────────────────────────────────────────────────────────────

    fn car_matches(&self, id: &PeripheralId) -> bool {
        self.car.as_ref().map(|c| &c.peripheral_id == id).unwrap_or(false)
    }

    /// Publish the current car state into the app-wide `tesla_state` (or the
    /// default when there is no car), so the HTTP API and dispatcher see it.
    async fn mirror_tesla_state(&self, state: &Arc<TGSApplication>) {
        debug!("[ble_discoverer] mirror_tesla_state in");
        let snapshot = self.car.as_ref().map(|c| c.state.clone()).unwrap_or_default();
        *state.tesla_state.write().await = snapshot;
        debug!("[ble_discoverer] mirror_tesla_state out");
    }
}
