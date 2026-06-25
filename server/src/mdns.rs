// mDNS advertising + hostname detection — two implementations selected at compile time:
//   macOS : dns-sd crate → delegates to the system's mDNSResponder via Bonjour C API
//   Linux : raw UDP multicast socket (works fine; no system daemon owns port 5353)

// ──────────────────────────────────────────────────────────────────────────────
// Hostname detection
// ──────────────────────────────────────────────────────────────────────────────

/// Returns the machine hostname via `sysctl kern.hostname` on macOS.
#[cfg(target_os = "macos")]
pub fn mdns_get_hostname() -> String {
    use sysctl::{Ctl, CtlValue, Sysctl};
    match Ctl::new("kern.hostname").and_then(|ctl| ctl.value()) {
        Ok(CtlValue::String(name)) => name,
        _ => {
            tracing::warn!("[mdns:mdns_get_hostname] sysctl kern.hostname failed, using fallback");
            "twogreensuns".to_string()
        }
    }
}

/// Returns the machine hostname by reading `/etc/hostname` on Linux (Raspberry Pi OS / Debian).
#[cfg(not(target_os = "macos"))]
pub fn mdns_get_hostname() -> String {
    match std::fs::read_to_string("/etc/hostname") {
        Ok(name) => name.trim().to_string(),
        Err(e) => {
            tracing::warn!("[mdns:mdns_get_hostname] /etc/hostname unreadable ({}), using fallback", e);
            "twogreensuns".to_string()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// macOS implementation
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
pub async fn mdns_run(hostname: String, port: u16) {
    use dns_sd::DNSService;
    use tracing::{debug, error, info};

    //debug!("[mdns:mdns_run] Enter (macOS / Bonjour)");

    let service_name = hostname.clone();

    //debug!(
    //    "  --> registering {} on port {} via DNSServiceRegister",
    //    service_name, port
    //);

    let svc = DNSService::register(
        Some(&service_name),   // instance name shown in discovery
        "_twogreensuns._tcp",  // service type (no .local. — dns-sd adds it)
        None,                  // domain  → default (.local.)
        None,                  // host    → this machine's hostname
        port,
        &[],                   // TXT records
    );

    match svc {
        Err(e) => {
            error!(" ==> DNSServiceRegister failed: {:?}", e);
            return;
        }
        Ok(svc) => {
            info!(
                " ==> Advertising {}:{} as _twogreensuns._tcp.local.",
                hostname, port
            );
            // Keep the registration alive until the task is cancelled.
            // dns-sd drops the registration when DNSService is dropped.
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                //debug!("[mdns:mdns_run] Still alive (Bonjour handles re-announcements)");
                // Reference svc so it is not dropped before the loop.
                let _ = &svc;
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Linux implementation (raw UDP multicast — original code)
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(not(target_os = "macos"))]
pub async fn mdns_run(hostname: String, port: u16) {
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::UdpSocket;
    use tokio::time::Duration;
    use tracing::{debug, error, info};

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_run] Enter (Linux / raw UDP)");

    debug!("  --> binding UDP socket on 0.0.0.0:{}", MDNS_PORT);
    let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], MDNS_PORT)))
        .await
        .expect("Failed to bind mDNS socket");

    debug!("  --> joining multicast group {}", MDNS_ADDR);
    socket
        .join_multicast_v4(MDNS_ADDR, Ipv4Addr::UNSPECIFIED)
        .expect("Failed to join mDNS multicast group");

    let service_name = format!("{}._twogreensuns._tcp.local.", hostname);
    let fqdn = format!("{}.local.", hostname);

    info!(
        "[mdns:mdns_run] Advertising {}:{} as _twogreensuns._tcp.local.",
        hostname, port
    );

    debug!("  --> sending gratuitous announcement");
    mdns_announce(&socket, &service_name, &fqdn, port).await;

    let mut announce_ticker = tokio::time::interval(Duration::from_secs(60));
    announce_ticker.tick().await; // consume the immediate first tick

    let mut buf = vec![0u8; 4096];
    loop {
        tokio::select! {
            _ = announce_ticker.tick() => {
                debug!("[mdns:mdns_run] Periodic re-announcement");
                mdns_announce(&socket, &service_name, &fqdn, port).await;
            }
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, src)) => {
                        let packet = &buf[..len];
                        debug!("[mdns:mdns_run] Received {} bytes from {}", len, src);
                        if let Some(response) = mdns_handle_query(packet, &service_name, &fqdn, port) {
                            let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));
                            debug!("  --> query matches us, sending response");
                            let _ = socket.send_to(&response, dest).await;
                        }
                    }
                    Err(e) => error!("[mdns:mdns_run] Receive error: {}", e),
                }
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// mDNS service discovery — shared across all platforms
// ──────────────────────────────────────────────────────────────────────────────

/// Sends a PTR query for `service_type` (e.g. `"_Fronius-SE-Inverter._tcp"`) and
/// returns the first IPv4 address seen in any A record in the responses, or
/// `None` if nothing replies within `timeout_secs` seconds.
///
/// Binds to a random source port so it coexists with mDNSResponder on macOS.
/// The QU (unicast-response) bit is set in the question; per RFC 6762 §5.4
/// a responder MUST reply unicast when the source port is not 5353.
pub async fn mdns_discover_service_address(service_type: &str, timeout_secs: u64) -> Option<String> {
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::UdpSocket;
    use tokio::time::Duration;
    use tracing::{debug, warn};

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_discover_service_address] Enter (service={})", service_type);

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => {
            warn!("[mdns:mdns_discover_service_address] bind failed: {}", e);
            return None;
        }
    };
    if let Err(e) = socket.set_multicast_ttl_v4(255) {
        warn!("  --> set_multicast_ttl_v4 failed: {}", e);
    }

    let service_fqdn = format!("{}.local.", service_type);
    let query = mdns_build_query(&service_fqdn);
    let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));

    if let Err(e) = socket.send_to(&query, dest).await {
        warn!("[mdns:mdns_discover_service_address] send failed: {}", e);
        return None;
    }
    debug!("  --> sent PTR query for {}", service_fqdn);

    let mut buf = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            debug!("[mdns:mdns_discover_service_address] timeout, no service found");
            return None;
        }

        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                debug!("  --> received {} bytes from {}", len, src);
                let packet = &buf[..len];
                // Must be a DNS response (QR bit set in flags)
                if packet.len() < 4 || (packet[2] & 0x80) == 0 {
                    continue;
                }
                for (_name, rtype, rdata) in mdns_parse_records(packet) {
                    if rtype == 0x0001 && rdata.len() == 4 {
                        let ip = format!("{}.{}.{}.{}", rdata[0], rdata[1], rdata[2], rdata[3]);
                        debug!("[mdns:mdns_discover_service_address] Exit (found {})", ip);
                        return Some(ip);
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("[mdns:mdns_discover_service_address] recv error: {}", e);
                return None;
            }
            Err(_) => {
                debug!("[mdns:mdns_discover_service_address] timeout");
                return None;
            }
        }
    }
}

/// Builds a DNS PTR query for `service_fqdn` with the QU (unicast-response) bit set.
fn mdns_build_query(service_fqdn: &str) -> Vec<u8> {
    let mut pkt = Vec::new();
    pkt.extend_from_slice(&[0x00, 0x00]); // Transaction ID
    pkt.extend_from_slice(&[0x00, 0x00]); // Flags: standard query
    pkt.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    pkt.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0
    mdns_append_name(&mut pkt, service_fqdn);
    pkt.extend_from_slice(&[0x00, 0x0C]); // QTYPE  = PTR
    pkt.extend_from_slice(&[0x80, 0x01]); // QCLASS = IN | QU bit
    pkt
}

/// Parses the answer, authority, and additional resource-record sections of a
/// DNS/mDNS packet.  Returns `(name, rtype, rdata)` for each record.
fn mdns_parse_records(packet: &[u8]) -> Vec<(String, u16, Vec<u8>)> {
    if packet.len() < 12 {
        return vec![];
    }
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let ancount = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let nscount = u16::from_be_bytes([packet[8], packet[9]]) as usize;
    let arcount = u16::from_be_bytes([packet[10], packet[11]]) as usize;

    let mut pos = 12;
    // Skip questions
    for _ in 0..qdcount {
        match mdns_read_name(packet, pos) {
            Some((_, next)) => pos = next + 4, // +4 for QTYPE(2) + QCLASS(2)
            None => return vec![],
        }
    }

    let mut records = Vec::new();
    for _ in 0..(ancount + nscount + arcount) {
        let (name, next) = match mdns_read_name(packet, pos) {
            Some(x) => x,
            None => break,
        };
        pos = next;
        if pos + 10 > packet.len() {
            break;
        }
        let rtype = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
        // Skip RCLASS(2) + TTL(4); read RDLENGTH(2)
        let rdlength = u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlength > packet.len() {
            break;
        }
        records.push((name, rtype, packet[pos..pos + rdlength].to_vec()));
        pos += rdlength;
    }
    records
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers used only by the Linux path
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(not(target_os = "macos"))]
async fn mdns_announce(
    socket: &tokio::net::UdpSocket,
    service_name: &str,
    fqdn: &str,
    port: u16,
) {
    use std::net::{Ipv4Addr, SocketAddr};
    use tracing::{debug, error};

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_announce] Enter");
    debug!("  --> service_name={} fqdn={} port={}", service_name, fqdn, port);
    let packet = mdns_build_response(service_name, fqdn, port);
    let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));
    if let Err(e) = socket.send_to(&packet, dest).await {
        error!("[mdns:mdns_announce] Send error: {}", e);
    }
    debug!("[mdns:mdns_announce] Exit");
}

#[cfg(not(target_os = "macos"))]
fn mdns_handle_query(
    packet: &[u8],
    service_name: &str,
    fqdn: &str,
    port: u16,
) -> Option<Vec<u8>> {
    use tracing::debug;

    debug!("[mdns:mdns_handle_query] Enter");
    if packet.len() < 12 {
        debug!("[mdns:mdns_handle_query] Exit (packet too short)");
        return None;
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if (flags >> 15) != 0 {
        debug!("[mdns:mdns_handle_query] Exit (not a query, ignoring)");
        return None;
    }
    let questions = mdns_parse_query(packet);
    debug!("  --> questions: {:?}", questions);
    let matches_us = questions.iter().any(|q| q.contains("_twogreensuns"));
    debug!("  --> matches_us={}", matches_us);
    debug!("[mdns:mdns_handle_query] Exit");
    if matches_us {
        Some(mdns_build_response(service_name, fqdn, port))
    } else {
        None
    }
}

#[cfg(not(target_os = "macos"))]
pub fn mdns_build_response(service_name: &str, fqdn: &str, port: u16) -> Vec<u8> {
    let mut pkt = Vec::new();

    pkt.extend_from_slice(&[0x00, 0x00]); // Transaction ID
    pkt.extend_from_slice(&[0x84, 0x00]); // Flags: QR=1, Authoritative=1
    pkt.extend_from_slice(&[0x00, 0x00]); // QDCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x03]); // ANCOUNT = 3
    pkt.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0

    let service_type = "_twogreensuns._tcp.local.";

    // PTR record
    mdns_append_name(&mut pkt, service_type);
    pkt.extend_from_slice(&[0x00, 0x0C]); // TYPE = PTR
    pkt.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
    pkt.extend_from_slice(&[0x00, 0x00, 0x11, 0x94]); // TTL = 4500
    let ptr_rdata = mdns_encode_name(service_name);
    pkt.extend_from_slice(&(ptr_rdata.len() as u16).to_be_bytes());
    pkt.extend_from_slice(&ptr_rdata);

    // SRV record
    mdns_append_name(&mut pkt, service_name);
    pkt.extend_from_slice(&[0x00, 0x21]); // TYPE = SRV
    pkt.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL = 120
    let fqdn_encoded = mdns_encode_name(fqdn);
    let srv_rdata_len = (2 + 2 + 2 + fqdn_encoded.len()) as u16;
    pkt.extend_from_slice(&srv_rdata_len.to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // Priority
    pkt.extend_from_slice(&[0x00, 0x00]); // Weight
    pkt.extend_from_slice(&port.to_be_bytes());
    pkt.extend_from_slice(&fqdn_encoded);

    // A record (placeholder IP)
    mdns_append_name(&mut pkt, fqdn);
    pkt.extend_from_slice(&[0x00, 0x01]); // TYPE = A
    pkt.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL = 120
    pkt.extend_from_slice(&[0x00, 0x04]); // RDLENGTH = 4
    pkt.extend_from_slice(&[0, 0, 0, 0]); // Placeholder IP

    pkt
}

#[cfg(not(target_os = "macos"))]
pub fn mdns_parse_query(packet: &[u8]) -> Vec<String> {
    if packet.len() < 12 {
        return vec![];
    }
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let mut questions = Vec::new();
    let mut pos = 12;

    for _ in 0..qdcount {
        match mdns_read_name(packet, pos) {
            Some((name, next_pos)) => {
                questions.push(name);
                pos = next_pos + 4;
            }
            None => break,
        }
    }
    questions
}

fn mdns_encode_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for label in name.trim_end_matches('.').split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0x00);
    out
}

fn mdns_append_name(pkt: &mut Vec<u8>, name: &str) {
    pkt.extend_from_slice(&mdns_encode_name(name));
}

fn mdns_read_name(packet: &[u8], start: usize) -> Option<(String, usize)> {
    let mut pos = start;
    let mut labels: Vec<String> = Vec::new();

    while pos < packet.len() {
        let len = packet[pos] as usize;
        if len == 0 {
            return Some((labels.join("."), pos + 1));
        }
        if len & 0xC0 == 0xC0 {
            return Some((labels.join("."), pos + 2));
        }
        pos += 1;
        if pos + len > packet.len() {
            return None;
        }
        labels.push(std::str::from_utf8(&packet[pos..pos + len]).ok()?.to_string());
        pos += len;
    }
    None
}
