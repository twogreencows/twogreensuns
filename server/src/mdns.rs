
use tracing::{debug, error, info, warn};

// mDNS advertising + hostname detection — two implementations selected at compile time:
//   macOS : dns-sd crate → delegates to the system's mDNSResponder via Bonjour C API
//   Linux : avahi-daemon via D-Bus when present (it owns UDP 5353 on most distros),
//           else a raw UDP multicast socket responder as a fallback

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
            warn!("[mdns:mdns_get_hostname] sysctl kern.hostname failed, using fallback");
            "twogreensuns".to_string()
        }
    }
}

/// Returns the machine hostname by reading `/etc/hostname` on Linux (Raspberry Pi OS / Debian).
#[cfg(target_os = "linux")]
pub fn mdns_get_hostname() -> String {
    match std::fs::read_to_string("/etc/hostname") {
        Ok(name) => name.trim().to_string(),
        Err(e) => {
            warn!("[mdns:mdns_get_hostname] /etc/hostname unreadable ({}), using fallback", e);
            "twogreensuns".to_string()
        }
    }
}

/// Returns the machine hostname via the `COMPUTERNAME` environment variable on Windows.
#[cfg(target_os = "windows")]
pub fn mdns_get_hostname() -> String {
    match std::env::var("COMPUTERNAME") {
        Ok(name) => name,
        Err(e) => {
            warn!("[mdns:mdns_get_hostname] COMPUTERNAME env var unavailable ({}), using fallback", e);
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

    debug!("[mdns:mdns_run] Enter (macOS / Bonjour)");

    let service_name = hostname.clone();
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
            info!( " ==> Advertising {}:{} as _twogreensuns._tcp.local.", hostname, port);
            // Keep the registration alive until the task is cancelled.
            // dns-sd drops the registration when DNSService is dropped.
            // Having the loop avoids the drop as svc scope is the one of the OK match
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                // Reference svc so it is not dropped before the loop.
                let _ = &svc;
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Windows implementation — not yet supported.
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
pub async fn mdns_run(_hostname: String, _port: u16) {
    debug!("[mdns:mdns_run] Enter (Windows)");
    warn!("[mdns:mdns_run] mDNS advertising not available on Windows");
}

// ──────────────────────────────────────────────────────────────────────────────
// Linux implementation — prefer avahi-daemon over D-Bus when present (most
// Raspberry Pi OS / Debian systems already run it, and it owns UDP 5353), and
// fall back to the raw UDP multicast responder when it's not there.
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
pub async fn mdns_run(hostname: String, port: u16) {

    debug!("[mdns:mdns_run] Enter (Linux)");
    match mdns_avahi_publish(&hostname, port).await {
        Some(_connection) => {
            info!( " => Advertising {}:{} as _twogreensuns._tcp.local. via avahi (D-Bus)", hostname, port);
            // Avahi frees the entry group automatically when this D-Bus
            // connection closes, so just keep it alive for the process lifetime.
            std::future::pending::<()>().await;
        }
        None => {
            info!(" ==> Avahi not available, falling back to raw UDP mDNS responder");
            mdns_run_raw_udp(hostname, port).await;
        }
    }
}

/// Tries to publish `_twogreensuns._tcp` for `hostname:port` through
/// avahi-daemon's D-Bus API (reusing the `dbus`/`dbus-tokio` crates already
/// pulled in by `btleplug`'s BlueZ backend, rather than adding a second D-Bus
/// client stack). Returns `None` (rather than erroring) whenever avahi isn't
/// reachable, so the caller can fall back to the raw UDP responder.
/// The returned connection must be kept alive for as long as the
/// advertisement should stay published — avahi withdraws it automatically
/// when the connection closes (on drop, or if the process dies).
#[cfg(target_os = "linux")]
async fn mdns_avahi_publish(hostname: &str, port: u16) -> Option<std::sync::Arc<dbus::nonblock::SyncConnection>> {
    use dbus::nonblock::Proxy;
    use std::time::Duration;

    const AVAHI_DEST: &str = "org.freedesktop.Avahi";
    const SERVER_IFACE: &str = "org.freedesktop.Avahi.Server";
    const GROUP_IFACE: &str = "org.freedesktop.Avahi.EntryGroup";
    const AVAHI_IF_UNSPEC: i32 = -1;
    const AVAHI_PROTO_UNSPEC: i32 = -1;
    const CALL_TIMEOUT: Duration = Duration::from_secs(5);

    debug!("[mdns:mdns_avahi_publish] Publishing through Avahi");

    let (resource, conn) = match dbus_tokio::connection::new_system_sync() {
        Ok(pair) => pair,
        Err(e) => {
            tracing::debug!("[mdns:mdns_avahi_publish] no system D-Bus available: {}", e);
            return None;
        }
    };
    tokio::spawn(async move {
        let err = resource.await;
        tracing::error!("[mdns:mdns_avahi_publish] D-Bus connection lost: {}", err);
    });

    let server = Proxy::new(AVAHI_DEST, "/", CALL_TIMEOUT, conn.clone());

    // Cheap call to confirm avahi-daemon actually owns AVAHI_DEST on the bus.
    let version_check: Result<(String,), dbus::Error> =
        server.method_call(SERVER_IFACE, "GetVersionString", ()).await;
    if let Err(e) = version_check {
        debug!("[mdns:mdns_avahi_publish] avahi-daemon not reachable on D-Bus: {}", e);
        return None;
    }

    let group_path: dbus::Path<'static> = match server
        .method_call::<(dbus::Path<'static>,), _, _, _>(SERVER_IFACE, "EntryGroupNew", ()).await
    {
        Ok((path,)) => path,
        Err(e) => {
            warn!("[mdns:mdns_avahi_publish]  EntryGroupNew failed: {}", e);
            return None;
        }
    };

    let group = Proxy::new(AVAHI_DEST, group_path, CALL_TIMEOUT, conn.clone());

    let txt: Vec<Vec<u8>> = vec![];
    let add_service_result: Result<(), dbus::Error> = group.method_call( GROUP_IFACE, "AddService",
            (
                AVAHI_IF_UNSPEC,
                AVAHI_PROTO_UNSPEC,
                0u32, // flags
                hostname.to_string(),
                "_twogreensuns._tcp".to_string(),
                String::new(), // domain -> default (.local)
                String::new(), // host -> default (this machine)
                port,
                txt,
            ),
        ).await;

    if let Err(e) = add_service_result {
        warn!("[mdns:mdns_avahi_publish]  AddService failed: {}", e);
        return None;
    }

    let commit_result: Result<(), dbus::Error> = group.method_call(GROUP_IFACE, "Commit", ()).await;
    if let Err(e) = commit_result {
        warn!("[mdns:mdns_avahi_publish]  Commit failed: {}", e);
        return None;
    }

    debug!("[mdns:mdns_avahi_publish] Exit (published)");
    Some(conn)
}

// ──────────────────────────────────────────────────────────────────────────────
// Linux fallback (raw UDP multicast — original code, used only when avahi
// isn't present to answer through)
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
async fn mdns_run_raw_udp(hostname: String, port: u16) {
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::UdpSocket;
    use tokio::time::Duration;

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_run] Enter (Linux / raw UDP)");

    // ── Setup: bind on the mDNS port and join the multicast group so we can
    //    both send announcements/responses and receive other hosts' queries.
    debug!( "  --> binding UDP socket on 0.0.0.0:{} (SO_REUSEADDR/REUSEPORT for Avahi coexistence)", MDNS_PORT);
    let socket = {
        use socket2::{Domain, Protocol, Socket, Type};
        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .expect("Failed to create raw socket");
        sock.set_reuse_address(true).expect("Failed to set SO_REUSEADDR");
        #[cfg(unix)]
        sock.set_reuse_port(true).expect("Failed to set SO_REUSEPORT");
        sock.set_nonblocking(true).expect("Failed to set non-blocking");
        let addr: SocketAddr = ([0, 0, 0, 0], MDNS_PORT).into();
        sock.bind(&addr.into()).expect("Failed to bind mDNS socket");
        UdpSocket::from_std(sock.into()).expect("Failed to convert to tokio UdpSocket")
    };

    debug!("  --> joining multicast group {}", MDNS_ADDR);
    socket.join_multicast_v4(MDNS_ADDR, Ipv4Addr::UNSPECIFIED)
        .expect("Failed to join mDNS multicast group");

    let service_name = format!("{}._twogreensuns._tcp.local.", hostname);
    let fqdn = format!("{}.local.", hostname);

    info!(
        "[mdns:mdns_run] Advertising {}:{} as _twogreensuns._tcp.local.",
        hostname, port
    );

    // ── Announce: send one unsolicited advertisement immediately on startup,
    //    so browsers already listening pick us up without having to query.
    debug!("  --> sending gratuitous announcement");
    mdns_announce(&socket, &service_name, &fqdn, port).await;

    // Re-announce periodically (every 60s): keeps our records from expiring
    // out of other hosts' mDNS caches, and covers any browser that started
    // listening after our one-shot startup announcement above.
    let mut announce_ticker = tokio::time::interval(Duration::from_secs(60));
    announce_ticker.tick().await; // consume the immediate first tick

    // ── Main loop: race the re-announce ticker against incoming packets.
    //    Every received packet is handed to mdns_handle_query, which decides
    //    whether it's a query for us and, if so, returns the advertisement
    //    to send back (see mdns_handle_query for exactly what it matches).
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
pub async fn mdns_discover_address_for_service(service_type: &str, timeout_secs: u64) -> Option<String> {
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::UdpSocket;
    use tokio::time::Duration;

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_discover_address_for_service] Enter (service={})", service_type);

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => {
            warn!("[mdns:mdns_discover_address_for_service] bind failed: {}", e);
            return None;
        }
    };
    if let Err(e) = socket.set_multicast_ttl_v4(255) {
        warn!("  --> set_multicast_ttl_v4 failed: {}", e);
    }

    let service_fqdn = format!("{}.local.", service_type);
    let query = mdns_build_discover_query(&service_fqdn);
    let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));

    if let Err(e) = socket.send_to(&query, dest).await {
        warn!("[mdns:mdns_discover_address_for_service] send failed: {}", e);
        return None;
    }
    debug!("  --> sent PTR query for {}", service_fqdn);

    let mut buf = vec![0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            debug!("[mdns:mdns_discover_address_for_service] timeout, no service found");
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
                        debug!("[mdns:mdns_discover_address_for_service] Exit (found {})", ip);
                        return Some(ip);
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("[mdns:mdns_discover_address_for_service] recv error: {}", e);
                return None;
            }
            Err(_) => {
                debug!("[mdns:mdns_discover_address_for_service] timeout");
                return None;
            }
        }
    }
}

/// Builds a DNS PTR query for `service_fqdn` with the QU (unicast-response) bit set.
fn mdns_build_discover_query(service_fqdn: &str) -> Vec<u8> {
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
#[cfg(target_os = "linux")]
async fn mdns_announce( socket: &tokio::net::UdpSocket, service_name: &str, fqdn: &str, port: u16) {
    use std::net::{Ipv4Addr, SocketAddr};

    const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    const MDNS_PORT: u16 = 5353;

    debug!("[mdns:mdns_announce] Enter");
    debug!("  --> service_name={} fqdn={} port={}", service_name, fqdn, port);
    let packet = mdns_build_advertisment_response(service_name, fqdn, port);
    let dest = SocketAddr::from((MDNS_ADDR, MDNS_PORT));
    if let Err(e) = socket.send_to(&packet, dest).await {
        error!("[mdns:mdns_announce] Send error: {}", e);
    }
    debug!("[mdns:mdns_announce] Exit");
}

/// Decides whether an incoming mDNS packet is a query we should answer, and
/// if so builds the advertisement response.
///
/// Supported: any query (PTR, SRV, TXT, or ANY — QTYPE is not inspected)
/// whose question name contains `_twogreensuns`, i.e. our service-type PTR
/// browse query or our own instance's SRV/TXT query. Handles packets with
/// multiple questions (QDCOUNT > 1).
///
/// Not supported / ignored: responses (QR bit set), the
/// `_services._dns-sd._udp.local.` service-type enumeration meta-query,
/// hostname A/AAAA lookups, and any other service's queries. No
/// known-answer suppression (RFC 6762 §7.1) — we always reply in full even
/// if the querier already has the record cached.
#[cfg(target_os = "linux")]
fn mdns_handle_query( packet: &[u8], service_name: &str, fqdn: &str, port: u16) -> Option<Vec<u8>> {
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
    let questions = mdns_parse_queried_service_names(packet);
    debug!("  --> questions: {:?}", questions);
    let matches_us = questions.iter().any(|q| q.contains("_twogreensuns"));
    debug!("  --> matches_us={}", matches_us);
    debug!("[mdns:mdns_handle_query] Exit");
    if matches_us {
        Some(mdns_build_advertisment_response(service_name, fqdn, port))
    } else {
        None
    }
}

/// Returns this machine's local IPv4 address as octets, for use in the mDNS A record.
/// Falls back to 0.0.0.0 if it can't be determined (loopback-only host, no network, etc.).
#[cfg(target_os = "linux")]
fn mdns_local_ipv4_octets() -> [u8; 4] {
    use std::net::IpAddr;

    match local_ip_address::local_ip() {
        Ok(IpAddr::V4(ip)) => ip.octets(),
        Ok(IpAddr::V6(_)) => {
            warn!("[mdns:mdns_local_ipv4_octets] local_ip() returned an IPv6 address, using 0.0.0.0");
            [0, 0, 0, 0]
        }
        Err(e) => {
            warn!("[mdns:mdns_local_ipv4_octets] failed to determine local IP: {}", e);
            [0, 0, 0, 0]
        }
    }
}

/// Builds the PTR+SRV+A record packet advertising `_twogreensuns._tcp`, used
/// both for periodic gratuitous announcements and for answering queries.
#[cfg(target_os = "linux")]
pub fn mdns_build_advertisment_response(service_name: &str, fqdn: &str, port: u16) -> Vec<u8> {
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

    // A record
    mdns_append_name(&mut pkt, fqdn);
    pkt.extend_from_slice(&[0x00, 0x01]); // TYPE = A
    pkt.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
    pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL = 120
    pkt.extend_from_slice(&[0x00, 0x04]); // RDLENGTH = 4
    pkt.extend_from_slice(&mdns_local_ipv4_octets());

    pkt
}

// TODO: naming in this file could still be improved further (e.g. distinguishing
// "service names" from full DNS question names once TXT/multi-question support is added).
#[cfg(target_os = "linux")]
pub fn mdns_parse_queried_service_names(packet: &[u8]) -> Vec<String> {
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
