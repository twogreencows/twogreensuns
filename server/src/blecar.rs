// Tesla BLE characteristic UUIDs and command encoding.
// These are stubs — actual values depend on the Tesla BLE protocol.
use sha1::Sha1;
use sha1::Digest;
use std::time::{Duration, Instant};
use btleplug::api::{CharPropFlags, Characteristic, Peripheral as _, WriteType};
use btleplug::platform::{Peripheral, PeripheralId};
use futures::StreamExt;
use prost::Message as _;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::hex_utils::HexaString;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use aes_gcm::aead::{Aead, KeyInit, Payload as AeadPayload};
use aes_gcm::{Aes128Gcm, Nonce};

use crate::tesla::car_server::{self, charging_start_stop_action::ChargingAction, vehicle_action::VehicleActionMsg};
use crate::tesla::signatures::{key_identity::IdentityType, signature_data::SigType, AesGcmPersonalizedSignatureData, KeyIdentity, SessionInfo, SessionInfoStatus, SignatureData, SignatureType, Tag};
use crate::tesla::universal_message::routable_message::{Payload, SubSigData};
use crate::tesla::universal_message::{self, Destination, Domain, Flags, RoutableMessage, SessionInfoRequest};
use crate::tesla::vcsec::{self, unsigned_message::SubMessage, RkeActionE};

pub const BLETESLA_VEHICLESERVICE_UUID: &str = "00000211-b2d1-43f0-9b88-960cebf8b91e";
pub const BLETESLA_TOVEHICLE_UUID: &str = "00000212-b2d1-43f0-9b88-960cebf8b91e";
pub const BLETESLA_FROMVEHICLE_UUID: &str = "00000213-b2d1-43f0-9b88-960cebf8b91e";

/// Chunk size for ToVehicle writes: default ATT MTU (23) minus 3 bytes of header.
/// Safe on any link. The negotiated MTU could allow more (the iOS app uses 115),
/// but btleplug does not expose it and an oversized WithoutResponse write is
/// silently dropped, so we stay on the floor value.
const BLE_WRITE_CHUNK: usize = 20;
/// How many times a missing session is requested before giving up on a command.
const SESSION_REQUEST_ATTEMPTS: u32 = 3;
/// How long to wait for a complete reply from the car. BlueZ (Raspberry Pi) is
/// slower to deliver notifications than CoreBluetooth, so it gets more room.
#[cfg(target_os = "macos")]
const BLE_REPLY_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(not(target_os = "macos"))]
const BLE_REPLY_TIMEOUT: Duration = Duration::from_secs(15);

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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TGSTeslaState {
    /// The vehicle has been seen advertising over BLE recently.
    pub present: bool,
    /// A GATT connection to the vehicle is currently established.
    pub connected: bool,
    /// There is a cable connected to the car
    pub plugged: bool,
    /// The car is charging
    pub charging: bool,
    /// The charging level in Amps
    pub charging_amps: u8,
    /// The current charge limit
    pub charge_limit_percent: u8,
}


pub fn ble_name_for_vin(vin: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(vin.as_bytes());
    let digest = hasher.finalize();
    let hex = hex::encode(digest);
    format!("S{}C", &hex[..16])
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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

/// Session established with one domain of the car (each domain has its own
/// epoch, counter, clock and shared secret).
#[derive(Clone, Debug)]
pub struct TGSBleSession {
    pub tag: Vec<u8>,
    pub counter: u32,
    pub clock_time: u32,
    pub epoch: Vec<u8>,
    /// First 16 bytes of SHA1(ECDH shared secret): the base of every session key.
    pub short_shared_secret: [u8; 16],
    /// Whether the session info tag computed on our side matched the car's.
    pub valid: bool,
    /// When the session info was received: the car clock is `clock_time` + elapsed.
    pub received_at: Instant,
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

 pub bluetooth_tx:Option<Characteristic>,
 pub bluetooth_rx:Option<Characteristic>,
 pub bluetooth_rssi: f32,

 // One session per Tesla domain, filled by decipher_session_message
 pub vcsec_session: Option<TGSBleSession>,
 pub infotainment_session: Option<TGSBleSession>,

 /// BLE peripheral this car was discovered as — used to correlate later events.
 pub peripheral_id: PeripheralId,
 /// Last time an advertisement was received from `peripheral_id`.
 pub last_seen: Instant,

 pub state:TGSTeslaState,
}

// ──────────────────────────────────────────────────────────────────────────────
// BLE task request/reply protocol
//
// The BLE task (see `blediscoverer`) is the sole owner of the Tesla peripheral.
// Callers reach it through an `mpsc` and get their answer back on a private
// `oneshot` created per request — see `TGSBleCarReply`.
// ──────────────────────────────────────────────────────────────────────────────

/// A request sent to the BLE task, paired with a [`TGSBleCarReply`] to answer on.
#[derive(Debug)]
pub enum TGSBleCarRequest {
    /// Read the current charging status / limit / amps from the vehicle.
    ReadState,
    /// Apply a charging command to the vehicle.
    Action(TGSTeslaCarCommand),
}

/// The reply half for one [`TGSBleCarRequest`]: a single-use channel created on the
/// caller's stack and handed to the BLE task with the request. Only the caller
/// that made the request can receive on it, so answers never cross between
/// callers. If the BLE task drops it without replying (e.g. shutdown), the
/// caller's `.await` resolves to `Err(RecvError)`.
pub type TGSBleCarReply = oneshot::Sender<Result<TGSTeslaState, BleError>>;

/// Multi-producer handle to the BLE task's request inbox. Cloned by the
/// dispatcher and every HTTP handler; the BLE task is the single consumer, so
/// all Bluetooth work is serialized in FIFO order.
pub type TGSBleCarCommandChannelTx = mpsc::Sender<(TGSBleCarRequest, TGSBleCarReply)>;

impl TGSBleCar {
    /// Read charging status / limit / amps from the vehicle over BLE.
    ///
    /// TODO: implement the real Tesla VCSEC / infotainment BLE read. For now this
    /// is a stub that leaves the last known state untouched.
    pub async fn read_charge_state(&mut self, peripheral: &Peripheral) -> Result<TGSTeslaState, BleError> {
        debug!("[blecar:read_charge_state] Enter");
        let (domain, payload) = self.build_read_charge_state_payload();
        if let Err(e) = self.wake_up(peripheral).await {
            warn!("[blecar:read_charge_state] wake up failed, continuing anyway: {}", e);
        }
        let reply = self.send_signed(peripheral, domain, &payload).await?;
        let response = car_server::Response::decode(reply.as_slice()).map_err(|e| BleError::CommandFailed(format!("decode charge state response: {}", e)))?;
        debug!("  --> response {:?}", response);
        Self::check_action_status(&response)?;
        match response.response_msg {
            Some(car_server::response::ResponseMsg::VehicleData(car_server::VehicleData { charge_state: Some(charge_state), .. })) => self.update_state_from_charge_state(&charge_state),
            _ => warn!("[blecar:read_charge_state] response holds no charge state"),
        }
        Ok(self.state.clone())
    }

    /// Encode and send a signed BLE command to the vehicle.
    ///
    /// TODO: implement the real Tesla command encoding + signing. For now this is
    /// a stub that mutates the local state so the rest of the pipeline (state
    /// mirror, HTTP API, dispatcher) can be exercised end to end.
    pub async fn apply(&mut self, peripheral: &Peripheral, cmd: &TGSTeslaCarCommand) -> Result<(), BleError> {
        debug!("[blecar:apply] Enter cmd={:?}", cmd);
        let (domain, payload) = self.build_command_payload(cmd)?;
        if domain == Domain::Infotainment {
            if let Err(e) = self.wake_up(peripheral).await {
                warn!("[blecar:apply] wake up failed, continuing anyway: {}", e);
            }
        }
        let reply = self.send_signed(peripheral, domain, &payload).await?;
        match domain {
            Domain::Infotainment => {
                let response = car_server::Response::decode(reply.as_slice()).map_err(|e| BleError::CommandFailed(format!("decode response: {}", e)))?;
                debug!("  --> response {:?}", response);
                Self::check_action_status(&response)?;
            }
            _ => match vcsec::FromVcsecMessage::decode(reply.as_slice()) {
                Ok(message) => debug!("  --> response {:?}", message),
                Err(e) => warn!("[blecar:apply] cannot decode reply: {}", e),
            },
        }
        match cmd {
            TGSTeslaCarCommand::StartCharging      => self.state.charging = true,
            TGSTeslaCarCommand::StopCharging       => self.state.charging = false,
            TGSTeslaCarCommand::SetChargingAmps(a) => self.state.charging_amps = *a,
            TGSTeslaCarCommand::SetChargeLimit(p)  => self.state.charge_limit_percent = *p,
            TGSTeslaCarCommand::WakeUp             => {}
        }
        Ok(())
    }

    /// Makes sure the GATT link is up. When a new connection is needed: connects,
    /// discovers the ToVehicle/FromVehicle characteristics, subscribes to
    /// FromVehicle and requests the session info.
    pub async fn ensure_connection(&mut self, peripheral: &Peripheral) -> Result<(), BleError> {
        debug!("[blecar:ensure_connection] Enter");
        if peripheral.is_connected().await.unwrap_or(false) {
            return Ok(());
        }

        debug!("[blecar:ensure_connection] connecting to Tesla");
        peripheral.connect().await
            .map_err(|e| BleError::ConnectionFailed(format!("connect failed: {}", e)))?;
        peripheral.discover_services().await.ok();

        let from_vehicle_uuid = Uuid::parse_str(BLETESLA_FROMVEHICLE_UUID).expect("valid FromVehicle UUID constant");
        let to_vehicle_uuid = Uuid::parse_str(BLETESLA_TOVEHICLE_UUID).expect("valid ToVehicle UUID constant");

        for characteristic in peripheral.characteristics() {
            debug!("[blecar:ensure_connection] Enumerating characteristic {}", characteristic.uuid);

            if characteristic.properties.contains(CharPropFlags::NOTIFY)
                || characteristic.properties.contains(CharPropFlags::INDICATE)
            {
                debug!("  --> can set notify for {}", characteristic.uuid);
            }

            if characteristic.uuid == from_vehicle_uuid {
                if let Err(e) = peripheral.subscribe(&characteristic).await {
                    warn!("[blecar:ensure_connection] subscribe to FromVehicle failed: {}", e);
                }
                self.bluetooth_rx = Some(characteristic.clone());
            }

            if characteristic.uuid == to_vehicle_uuid {
                self.bluetooth_tx = Some(characteristic.clone());
            }
        }

        // Only the vehicle security domain answers while the car sleeps. The infotainment
        // session is requested lazily by send_signed, once the car has been woken up.
        self.request_session(peripheral, Domain::VehicleSecurity).await;
        Ok(())
    }

    /// Requests and deciphers a session with one domain. Failures are logged, not returned.
    async fn request_session(&mut self, peripheral: &Peripheral, domain: Domain) {
        let message = self.build_session_info_request_message(domain);
        debug!("[blecar:request_session] Sending session info request to {:?}", domain);
        debug!("  --> structure {:?}", message);
        match self.send_message(peripheral, &message.encode_to_vec(), &message.uuid).await {
            Ok(reply) => {
                debug!("[blecar:request_session] session info response ({} bytes): {}", reply.len(), reply.as_hexa_string());
                if let Err(e) = self.decipher_session_message(domain, &reply) {
                    warn!("[blecar:request_session] decipher session message failed for {:?}: {}", domain, e);
                }
            }
            Err(e) => warn!("[blecar:request_session] session info request to {:?} failed: {}", domain, e),
        }
    }

    /// Sends `body` to the car: prefixes it with its 2-byte big-endian length,
    /// writes it to ToVehicle in chunks, then reassembles the length-prefixed
    /// replies from FromVehicle notifications. The car also sends messages nobody asked
    /// for, so only the reply whose `request_uuid` is the `uuid` of our request is returned
    /// (prefix stripped); the others are skipped.
    pub async fn send_message(&self, peripheral: &Peripheral, body: &[u8], request_uuid: &[u8]) -> Result<Vec<u8>, BleError> {
        debug!("[blecar:send_message] Enter");
        let (Some(tx), Some(rx)) = (self.bluetooth_tx.as_ref(), self.bluetooth_rx.as_ref()) else {
            return Err(BleError::CommandFailed("ToVehicle/FromVehicle characteristics not available".to_string()));
        };

        // Subscribe to the notification stream BEFORE writing so the reply can't be missed.
        let mut notifications = peripheral.notifications().await
            .map_err(|e| BleError::CommandFailed(format!("notifications stream failed: {}", e)))?;

        let mut framed = Vec::with_capacity(body.len() + 2);
        framed.extend_from_slice(&(body.len() as u16).to_be_bytes());
        framed.extend_from_slice(body);
        debug!("  --> send hex {}", framed.as_hexa_string());

        for chunk in framed.chunks(BLE_WRITE_CHUNK) {
            peripheral.write(tx, chunk, WriteType::WithoutResponse).await
                .map_err(|e| BleError::CommandFailed(format!("write failed: {}", e)))?;
        }

        // Reassemble: first 2 bytes = big-endian length, then accumulate until complete.
        let mut buffer: Vec<u8> = Vec::new();
        let reply = tokio::time::timeout(BLE_REPLY_TIMEOUT, async {
            while let Some(notification) = notifications.next().await {
                if notification.uuid != rx.uuid { continue; }
                debug!("  --> chunk hex {}", notification.value.as_hexa_string());
                buffer.extend_from_slice(&notification.value);
                // Several frames can follow each other: take them one by one.
                while buffer.len() >= 2 {
                    let expected = u16::from_be_bytes([buffer[0], buffer[1]]) as usize;
                    if buffer.len() < 2 + expected { break; }
                    let frame: Vec<u8> = buffer.drain(..2 + expected).skip(2).collect();
                    match RoutableMessage::decode(frame.as_slice()) {
                        Ok(message) if message.request_uuid == request_uuid => return Some(frame),
                        Ok(message) => debug!("  --> ignoring unsolicited message {:?}", message),
                        Err(e) => debug!("  --> ignoring undecodable message: {}", e),
                    }
                }
            }
            None
        }).await
            .map_err(|_| BleError::CommandFailed("timeout waiting for car reply".to_string()))?
            .ok_or_else(|| BleError::CommandFailed("notification stream closed".to_string()))?;

        debug!("  --> reply hex {}", reply.as_hexa_string());
        Ok(reply)
    }

    /// Deciphers the session info answer (already stripped of its length prefix).
    /// Stores counter / clock time / epoch on the car, computes the shared secret
    /// from the car public key (ECDH, SHA1, first 16 bytes) and verifies the
    /// session info tag with it. See
    /// https://github.com/teslamotors/vehicle-command/blob/main/pkg/protocol/protocol.md
    pub fn decipher_session_message(&mut self, domain: Domain, reply: &[u8]) -> Result<(), BleError> {
        debug!("[blecar:decipher_session_message] Enter ({:?})", domain);
        let fail = |m: &str| BleError::CommandFailed(m.to_string());

        let message = RoutableMessage::decode(reply).map_err(|e| BleError::CommandFailed(format!("decode RoutableMessage: {}", e)))?;
        debug!("  --> message {:?}", message);

        // The tag, and the raw SessionInfo bytes (needed verbatim for the metadata).
        let Some(SubSigData::SignatureData(SignatureData { sig_type: Some(SigType::SessionInfoTag(hmac_data)), .. })) = message.sub_sig_data else {
            return Err(fail("no session info tag in reply"));
        };
        let Some(Payload::SessionInfo(session_info_bytes)) = message.payload else {
            return Err(fail("no session info in reply"));
        };
        let info = SessionInfo::decode(session_info_bytes.as_slice()).map_err(|e| BleError::CommandFailed(format!("decode SessionInfo: {}", e)))?;
        debug!("  --> session info {:?}", info);
        if info.status == SessionInfoStatus::KeyNotOnWhitelist as i32 {
            warn!("[blecar:decipher_session_message] our key is not on the car whitelist");
        }

        // ECDH with the car public key (65-byte SEC1), then SHA1 and keep the first 16 bytes.
        let secret = p256::SecretKey::from_slice(&self.client_private_key).map_err(|e| BleError::CommandFailed(format!("client private key: {}", e)))?;
        let car_public = p256::PublicKey::from_sec1_bytes(&info.public_key).map_err(|e| BleError::CommandFailed(format!("car public key: {}", e)))?;
        let full_secret = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), car_public.as_affine());
        let sha1 = Sha1::digest(full_secret.raw_secret_bytes());
        let mut shared_secret = [0u8; 16];
        shared_secret.copy_from_slice(&sha1[..16]);
        debug!("  --> short shared secret {}", shared_secret.to_vec().as_hexa_string());

        // sessionInfoKey = HMAC-SHA256(key: shortSecret, msg: "session info")
        let session_info_key = hmac_sha256(&shared_secret, b"session info");

        // TLV metadata: signature type, VIN, challenge (request uuid), END, then the raw SessionInfo.
        let mut metadata = Vec::new();
        for (tag, value) in [
            (Tag::SignatureType as u8, vec![SignatureType::Hmac as u8]),
            (Tag::Personalization as u8, self.vin.as_bytes().to_vec()),
            (Tag::Challenge as u8, message.request_uuid.clone()),
        ] {
            metadata.push(tag);
            metadata.push(value.len() as u8);
            metadata.extend_from_slice(&value);
        }
        metadata.push(Tag::End as u8);
        metadata.extend_from_slice(&session_info_bytes);
        debug!("  --> metadata {}", metadata.as_hexa_string());

        // Compare our tag with the car's.
        let computed_tag = hmac_sha256(&session_info_key, &metadata);
        let valid = computed_tag == hmac_data.tag;
        debug!("  --> validity of session is {}", valid);

        let session = TGSBleSession {
            tag: hmac_data.tag,
            counter: info.counter,
            clock_time: info.clock_time,
            epoch: info.epoch,
            short_shared_secret: shared_secret,
            valid,
            received_at: Instant::now(),
        };
        match domain {
            Domain::VehicleSecurity => self.vcsec_session = Some(session),
            Domain::Infotainment => self.infotainment_session = Some(session),
            Domain::Broadcast => return Err(fail("no session for the broadcast domain")),
        }
        Ok(())
    }

    /// Copies what we care about from the car's charge state into our own state.
    fn update_state_from_charge_state(&mut self, charge_state: &car_server::ChargeState) {
        use car_server::charge_state::{cable_type, charging_state, OptionalChargeCurrentRequest, OptionalChargeLimitSoc};
        debug!("[blecar:update_state_from_charge_state] Enter");

        if let Some(charging_state::Type::Charging(_)) = charge_state.charging_state.as_ref().and_then(|c| c.r#type) {
            self.state.charging = true;
        } else {
            self.state.charging = false;
        }
        // Sna means no cable detected.
        self.state.plugged = match charge_state.conn_charge_cable.as_ref().and_then(|c| c.r#type) {
            Some(cable_type::Type::Sna(_)) | None => false,
            Some(_) => true,
        };
        if let Some(OptionalChargeCurrentRequest::ChargeCurrentRequest(amps)) = charge_state.optional_charge_current_request {
            self.state.charging_amps = amps.clamp(0, u8::MAX as i32) as u8;
        }
        if let Some(OptionalChargeLimitSoc::ChargeLimitSoc(percent)) = charge_state.optional_charge_limit_soc {
            self.state.charge_limit_percent = percent.clamp(0, u8::MAX as i32) as u8;
        }
    }

    /// Turns an action status other than OK into an error. A missing status counts as OK.
    fn check_action_status(response: &car_server::Response) -> Result<(), BleError> {
        match response.action_status.as_ref() {
            Some(status) if status.result != car_server::OperationStatusE::OperationstatusOk as i32 => {
                Err(BleError::CommandFailed(format!("car refused the action: {:?}", status)))
            }
            _ => Ok(()),
        }
    }

    fn has_session(&self, domain: Domain) -> bool {
        match domain {
            Domain::VehicleSecurity => self.vcsec_session.is_some(),
            Domain::Infotainment => self.infotainment_session.is_some(),
            Domain::Broadcast => false,
        }
    }

    /// Wakes the car through the vehicle security domain. Harmless if it is already awake.
    async fn wake_up(&mut self, peripheral: &Peripheral) -> Result<(), BleError> {
        debug!("[blecar:wake_up] Enter");
        let (domain, payload) = self.build_command_payload(&TGSTeslaCarCommand::WakeUp)?;
        let reply = self.send_signed(peripheral, domain, &payload).await?;
        match vcsec::FromVcsecMessage::decode(reply.as_slice()) {
            Ok(message) => debug!("  --> wake up reply {:?}", message),
            Err(e) => warn!("[blecar:wake_up] cannot decode reply: {}", e),
        }
        Ok(())
    }

    /// Signs `payload` for `domain`, sends it and returns the decoded reply. Requests the
    /// session first when there is none yet (for instance the infotainment one after a wake up).
    /// A reply carrying a message fault is returned as an error. Returns the reply payload, decrypted.
    async fn send_signed(&mut self, peripheral: &Peripheral, domain: Domain, payload: &[u8]) -> Result<Vec<u8>, BleError> {
        debug!("[blecar:send_signed] Enter ({:?})", domain);
        // A car that has just been woken up may need a few tries before the domain answers.
        for attempt in 1..=SESSION_REQUEST_ATTEMPTS {
            if self.has_session(domain) {
                break;
            }
            debug!("  --> no {:?} session yet, request attempt {}/{}", domain, attempt, SESSION_REQUEST_ATTEMPTS);
            self.request_session(peripheral, domain).await;
        }

        let message = self.build_signed_message(domain, payload)?;
        debug!("  --> structure {:?}", message);
        // The tag of our request is part of the authenticated data of the answer.
        let request_tag = match &message.sub_sig_data {
            Some(SubSigData::SignatureData(SignatureData { sig_type: Some(SigType::AesGcmPersonalizedData(data)), .. })) => data.tag.clone(),
            _ => return Err(BleError::CommandFailed("signed message without AES-GCM data".to_string())),
        };
        let reply_bytes = self.send_message(peripheral, &message.encode_to_vec(), &message.uuid).await?;
        debug!("  --> reply hex {}", reply_bytes.as_hexa_string());
        let reply = RoutableMessage::decode(reply_bytes.as_slice()).map_err(|e| BleError::CommandFailed(format!("decode reply: {}", e)))?;
        if let Some(status) = reply.signed_message_status.as_ref() {
            if status.signed_message_fault != 0 {
                return Err(BleError::CommandFailed(format!("car replied with fault {} (operation status {})", status.signed_message_fault, status.operation_status)));
            }
        }
        self.decrypt_response(domain, &request_tag, &reply)
    }

    /// Returns the payload of a reply, decrypting it (AES-128-GCM, same key as the request)
    /// when the car encrypted it. The authenticated data is the SHA256 of the response
    /// metadata, which binds the answer to the request tag it replies to.
    fn decrypt_response(&self, domain: Domain, request_tag: &[u8], reply: &RoutableMessage) -> Result<Vec<u8>, BleError> {
        debug!("[blecar:decrypt_response] Enter ({:?})", domain);
        let fail = |m: &str| BleError::CommandFailed(m.to_string());

        let Some(Payload::ProtobufMessageAsBytes(payload)) = reply.payload.as_ref() else {
            return Err(fail("reply has no payload"));
        };
        let Some(SubSigData::SignatureData(SignatureData { sig_type: Some(SigType::AesGcmResponseData(gcm)), .. })) = reply.sub_sig_data.as_ref() else {
            debug!("  --> reply is not encrypted");
            return Ok(payload.clone());
        };
        if gcm.nonce.len() != 12 {
            return Err(fail("bad nonce length in reply"));
        }
        let session = match domain {
            Domain::VehicleSecurity => self.vcsec_session.as_ref(),
            Domain::Infotainment => self.infotainment_session.as_ref(),
            Domain::Broadcast => None,
        }.ok_or_else(|| fail("no session for this domain"))?;

        let reply_domain = match reply.from_destination.as_ref().and_then(|d| d.sub_destination.as_ref()) {
            Some(universal_message::destination::SubDestination::Domain(d)) => *d as u8,
            _ => domain as u8,
        };
        // Request hash: the authentication method of the request, then its tag.
        let mut request_hash = vec![SignatureType::AesGcmPersonalized as u8];
        request_hash.extend_from_slice(request_tag);
        let fault = reply.signed_message_status.as_ref().map(|s| s.signed_message_fault as u32).unwrap_or(0);

        let mut metadata = Vec::new();
        metadata_add(&mut metadata, Tag::SignatureType, &[SignatureType::AesGcmResponse as u8]);
        metadata_add(&mut metadata, Tag::Domain, &[reply_domain]);
        metadata_add(&mut metadata, Tag::Personalization, self.vin.as_bytes());
        metadata_add(&mut metadata, Tag::Counter, &gcm.counter.to_be_bytes());
        metadata_add(&mut metadata, Tag::Flags, &reply.flags.to_be_bytes());
        metadata_add(&mut metadata, Tag::RequestHash, &request_hash);
        metadata_add(&mut metadata, Tag::Fault, &fault.to_be_bytes());
        metadata.push(Tag::End as u8);
        debug!("  --> metadata {}", metadata.as_hexa_string());
        let metadata_hash = Sha256::digest(&metadata);

        // AES-GCM wants the ciphertext followed by its tag.
        let mut sealed = payload.clone();
        sealed.extend_from_slice(&gcm.tag);
        let cipher = Aes128Gcm::new_from_slice(&session.short_shared_secret).map_err(|e| BleError::CommandFailed(format!("AES key: {}", e)))?;
        let plaintext = cipher.decrypt(Nonce::from_slice(&gcm.nonce), AeadPayload { msg: &sealed, aad: &metadata_hash })
            .map_err(|e| BleError::CommandFailed(format!("AES-GCM decrypt: {}", e)))?;
        debug!("  --> plaintext hex {}", plaintext.as_hexa_string());
        Ok(plaintext)
    }

    /// Builds the unsigned payload of a command and the domain it must be sent to.
    /// WakeUp goes to the vehicle security domain, charging commands to infotainment.
    pub fn build_command_payload(&self, cmd: &TGSTeslaCarCommand) -> Result<(Domain, Vec<u8>), BleError> {
        debug!("[blecar:build_command_payload] Enter {:?}", cmd);
        let vehicle_action = match cmd {
            TGSTeslaCarCommand::WakeUp => {
                let message = vcsec::UnsignedMessage { sub_message: Some(SubMessage::RkeAction(RkeActionE::RkeActionWakeVehicle as i32)) };
                return Ok((Domain::VehicleSecurity, message.encode_to_vec()));
            }
            TGSTeslaCarCommand::StartCharging => VehicleActionMsg::ChargingStartStopAction(car_server::ChargingStartStopAction { charging_action: Some(ChargingAction::Start(car_server::Void {})) }),
            TGSTeslaCarCommand::StopCharging => VehicleActionMsg::ChargingStartStopAction(car_server::ChargingStartStopAction { charging_action: Some(ChargingAction::Stop(car_server::Void {})) }),
            TGSTeslaCarCommand::SetChargingAmps(amps) => VehicleActionMsg::SetChargingAmpsAction(car_server::SetChargingAmpsAction { charging_amps: *amps as i32 }),
            TGSTeslaCarCommand::SetChargeLimit(percent) => VehicleActionMsg::ChargingSetLimitAction(car_server::ChargingSetLimitAction { percent: *percent as i32 }),
        };
        Ok((Domain::Infotainment, Self::infotainment_action(vehicle_action)))
    }

    /// Builds the unsigned payload asking the infotainment domain for the charge state.
    pub fn build_read_charge_state_payload(&self) -> (Domain, Vec<u8>) {
        debug!("[blecar:build_read_charge_state_payload] Enter");
        let get_vehicle_data = car_server::GetVehicleData { get_charge_state: Some(car_server::GetChargeState {}), ..Default::default() };
        (Domain::Infotainment, Self::infotainment_action(VehicleActionMsg::GetVehicleData(get_vehicle_data)))
    }

    fn infotainment_action(vehicle_action: VehicleActionMsg) -> Vec<u8> {
        car_server::Action {
            action_msg: Some(car_server::action::ActionMsg::VehicleAction(car_server::VehicleAction { vehicle_action_msg: Some(vehicle_action) })),
        }.encode_to_vec()
    }

    /// Signs `payload` for `domain` (AES-128-GCM keyed by the session shared secret)
    /// and wraps it in a RoutableMessage ready to be sent. Consumes one session counter.
    pub fn build_signed_message(&mut self, domain: Domain, payload: &[u8]) -> Result<RoutableMessage, BleError> {
        debug!("[blecar:build_signed_message] Enter ({:?})", domain);
        let fail = |m: &str| BleError::CommandFailed(m.to_string());

        let session = match domain {
            Domain::VehicleSecurity => self.vcsec_session.as_mut(),
            Domain::Infotainment => self.infotainment_session.as_mut(),
            Domain::Broadcast => None,
        }.ok_or_else(|| fail("no session for this domain"))?;

        // Increment the counter. The car clock is the session clock time plus the time since we got it;
        // the command is valid for 5 seconds from now.
        session.counter += 1;
        let counter = session.counter;
        let expires_at = session.clock_time + session.received_at.elapsed().as_secs() as u32 + 5;
        let epoch = session.epoch.clone();
        let shared_secret = session.short_shared_secret;

        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).expect("getrandom failed");
        let flags = 1u32 << Flags::FlagEncryptResponse as u32;

        // Metadata, authenticated through its SHA256: signature type, domain, VIN, epoch, expiry, counter, flags.
        let mut metadata = Vec::new();
        metadata_add(&mut metadata, Tag::SignatureType, &[SignatureType::AesGcmPersonalized as u8]);
        metadata_add(&mut metadata, Tag::Domain, &[domain as u8]);
        metadata_add(&mut metadata, Tag::Personalization, self.vin.as_bytes());
        metadata_add(&mut metadata, Tag::Epoch, &epoch);
        metadata_add(&mut metadata, Tag::ExpiresAt, &expires_at.to_be_bytes());
        metadata_add(&mut metadata, Tag::Counter, &counter.to_be_bytes());
        metadata_add(&mut metadata, Tag::Flags, &flags.to_be_bytes());
        metadata.push(Tag::End as u8);
        debug!("  --> metadata {}", metadata.as_hexa_string());
        let metadata_hash = Sha256::digest(&metadata);

        let cipher = Aes128Gcm::new_from_slice(&shared_secret).map_err(|e| BleError::CommandFailed(format!("AES key: {}", e)))?;
        let mut sealed = cipher.encrypt(Nonce::from_slice(&nonce), AeadPayload { msg: payload, aad: &metadata_hash })
            .map_err(|e| BleError::CommandFailed(format!("AES-GCM encrypt: {}", e)))?;
        let tag = sealed.split_off(sealed.len() - 16);

        let mut uuid = [0u8; 16];
        getrandom::getrandom(&mut uuid).expect("getrandom failed");

        Ok(RoutableMessage {
            to_destination: Some(Destination { sub_destination: Some(universal_message::destination::SubDestination::Domain(domain as i32)) }),
            from_destination: Some(Destination { sub_destination: Some(universal_message::destination::SubDestination::RoutingAddress(self.from_destination_address.to_vec())) }),
            signed_message_status: None,
            request_uuid: Vec::new(),
            uuid: uuid.to_vec(),
            flags,
            payload: Some(Payload::ProtobufMessageAsBytes(sealed)),
            sub_sig_data: Some(SubSigData::SignatureData(SignatureData {
                signer_identity: Some(KeyIdentity { identity_type: Some(IdentityType::PublicKey(self.client_public_key.clone())) }),
                sig_type: Some(SigType::AesGcmPersonalizedData(AesGcmPersonalizedSignatureData {
                    epoch,
                    nonce: nonce.to_vec(),
                    counter,
                    expires_at,
                    tag,
                })),
            })),
        })
    }

    /// Builds the RoutableMessage that requests a new BLE session with the
    /// vehicle's given domain, carrying our client public key so it can
    /// derive the shared session key.
    pub fn build_session_info_request_message(&self, domain: Domain) -> RoutableMessage {
        debug!("[blecar:build_session_info_request_message] Enter ({:?})", domain);

        let to_destination = Destination {
            sub_destination: Some(universal_message::destination::SubDestination::Domain(domain as i32)),
        };
        let from_destination = Destination {
            sub_destination: Some(universal_message::destination::SubDestination::RoutingAddress(self.from_destination_address.to_vec())),
        };
        debug!("  --> from_destination_address {}", self.from_destination_address.to_vec().as_hexa_string());

        let session_info_request = SessionInfoRequest {
            public_key: self.client_public_key.clone(),
            challenge: Vec::new(),
        };

        let mut uuid_bytes = [0u8; 16];
        getrandom::getrandom(&mut uuid_bytes).expect("getrandom failed");
        debug!("  --> uuid {}", uuid_bytes.to_vec().as_hexa_string());

        RoutableMessage {
            to_destination: Some(to_destination),
            from_destination: Some(from_destination),
            signed_message_status: None,
            request_uuid: Vec::new(),
            uuid: uuid_bytes.to_vec(),
            flags: 0,
            payload: Some(universal_message::routable_message::Payload::SessionInfoRequest(session_info_request)),
            sub_sig_data: None,
        }
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Appends one TLV entry (tag, length, value) to the metadata.
fn metadata_add(metadata: &mut Vec<u8>, tag: Tag, value: &[u8]) {
    metadata.push(tag as u8);
    metadata.push(value.len() as u8);
    metadata.extend_from_slice(value);
}
