import os
import Darwin

private let logger = Logger(subsystem: "com.twogreensuns", category: "TGSBonjourBrowser")

import Network
import Foundation

final class TGSBonjourBrowser {
    private var browser: NWBrowser?
    private var resolvingConnections: [String: NWConnection] = [:] // keyed by service name
    private var resolved: Set<TGSDiscoveredService> = []
    private let queue = DispatchQueue(label: "com.twogreensuns.bonjourbrowser")

    /// TCP parameters pinned to IPv4 — the simulator otherwise resolves to a link-local
    /// IPv6 endpoint we can't easily connect to.
    private static let tcpIPv4: NWParameters = {
        let params = NWParameters.tcp
        if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
            ip.version = .v4
        }
        return params
    }()

    var onServicesChanged: ((Set<TGSDiscoveredService>) -> Void)?

    func start() {
        logger.debug("[TGSBonjourBrowser:start] Browsing for _twogreensuns._tcp. on local.")

        let params = NWParameters()
        params.includePeerToPeer = true

        let descriptor = NWBrowser.Descriptor.bonjour(type: "_twogreensuns._tcp", domain: "local.")
        let b = NWBrowser(for: descriptor, using: params)

        b.stateUpdateHandler = { [weak self] state in
            if case let .failed(error) = state {
                logger.warning("[TGSBonjourBrowser:stateUpdate] Browse error: \(error)")
            }
        }

        b.browseResultsChangedHandler = { [weak self] results, changes in
            self?.handle(changes: changes)
        }

        b.start(queue: queue)
        browser = b
    }

    func stop() {
        browser?.cancel()
        browser = nil
        resolvingConnections.values.forEach { $0.cancel() }
        resolvingConnections.removeAll()
    }

    // MARK: - Browse result handling

    private func handle(changes: Set<NWBrowser.Result.Change>) {
        print(changes)
        for change in changes {
            switch change {
            case .added(let result):
                resolve(result)
            case .removed(let result):
                remove(result)
            default:
                break
            }
        }
    }

    private func resolve(_ result: NWBrowser.Result) {
        guard case let .service(name, _, _, _) = result.endpoint else { return }
        logger.debug("[TGSBonjourBrowser:resolve] Found '\(name)' — resolving...")
        print(result)
        print(result.endpoint)
        let connection = NWConnection(to: result.endpoint, using: Self.tcpIPv4)
        resolvingConnections[name] = connection

        connection.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.handleResolved(name: name, connection: connection)
            case .failed(let error):
                logger.warning("[TGSBonjourBrowser:resolve] '\(name)' failed: \(error)")
                self.cleanup(name)
            default:
                break
            }
        }

        connection.start(queue: queue)

        // Manual timeout — mirrors the old resolve(withTimeout: 5)
        queue.asyncAfter(deadline: .now() + 5) { [weak self] in
            guard let self, self.resolvingConnections[name] != nil else { return }
            logger.warning("[TGSBonjourBrowser:resolve] '\(name)' timed out")
            self.cleanup(name)
        }
    }

    private func handleResolved(name: String, connection: NWConnection) {
        guard case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint else {
            logger.warning("[TGSBonjourBrowser:resolve] '\(name)' resolved but no remote endpoint")
            cleanup(name)
            return
        }

        guard case let .ipv4(v4) = host else {
            logger.warning("[TGSBonjourBrowser:resolve] '\(name)' resolved to a non-IPv4 address — skipping")
            cleanup(name)
            return
        }

        let raw = "\(v4)"
        let ip = raw.split(separator: "%").first.map(String.init) ?? raw
        let hostString = hostString(from: host)

        let service = TGSDiscoveredService(
            name: name,
            host: hostString,
            port: Int(port.rawValue),
            ipAddress: ip
        )

        logger.info("[TGSBonjourBrowser:resolve] '\(service.name)' → \(service.baseURL) (\(ip))")
        resolved.insert(service)
        onServicesChanged?(resolved)
        cleanup(name)
    }

    private func remove(_ result: NWBrowser.Result) {
        guard case let .service(name, _, _, _) = result.endpoint else { return }
        logger.debug("[TGSBonjourBrowser:remove] Removed '\(name)'")
        resolved.remove(TGSDiscoveredService(name: name, host: "", port: 0, ipAddress: ""))
        onServicesChanged?(resolved)
    }

    // MARK: - Helpers

    private func hostString(from host: NWEndpoint.Host) -> String {
        switch host {
        case .name(let hostname, _): return hostname
        case .ipv4(let addr): return "\(addr)"
        case .ipv6(let addr): return "\(addr)"
        @unknown default: return ""
        }
    }

    private func cleanup(_ name: String) {
        resolvingConnections[name]?.cancel()
        resolvingConnections.removeValue(forKey: name)
    }
}

/*import Foundation
import os
import Darwin

private let logger = Logger(subsystem: "com.twogreensuns", category: "TGSBonjourBrowser")

final class TGSBonjourBrowser: NSObject {
    private var browser: NetServiceBrowser?
    private var resolvingServices: [NetService] = []
    private var resolved: Set<TGSDiscoveredService> = []

    var onServicesChanged: ((Set<TGSDiscoveredService>) -> Void)?

    func start() {
        logger.debug("[TGSBonjourBrowser:start] Browsing for _twogreensuns._tcp. on local.")
        let b = NetServiceBrowser()
        b.delegate = self
        b.searchForServices(ofType: "_twogreensuns._tcp.", inDomain: "local.")
        browser = b
    }

    func stop() {
        browser?.stop()
        browser = nil
    }
}

// MARK: - NetServiceBrowserDelegate

extension TGSBonjourBrowser: NetServiceBrowserDelegate {
    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        logger.debug("[TGSBonjourBrowser:didFind] Found '\(service.name)' — resolving...")
        resolvingServices.append(service)
        service.delegate = self
        service.resolve(withTimeout: 5)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool) {
        logger.debug("[TGSBonjourBrowser:didRemove] Removed '\(service.name)'")
        resolved.remove(TGSDiscoveredService(name: service.name, host: "", port: 0, ipAddress: ""))
        onServicesChanged?(resolved)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
        logger.warning("[TGSBonjourBrowser:didNotSearch] Browse error: \(errorDict)")
    }
}

// MARK: - NetServiceDelegate

extension TGSBonjourBrowser: NetServiceDelegate {
    func netServiceDidResolveAddress(_ sender: NetService) {
        
        guard let host = sender.hostName, sender.port > 0 else {
            logger.warning("[TGSBonjourBrowser:didResolveAddress] '\(sender.name)' resolved but host/port invalid")
            cleanup(sender)
            return
        }
        logger.info("'\(sender)'")        
        let ip = ipv4Address(from: sender)
        let service = TGSDiscoveredService(name: sender.name, host: host, port: sender.port, ipAddress: ip)
        logger.info("[TGSBonjourBrowser:didResolveAddress] '\(service.name)' → \(service.baseURL) (\(ip))")
        resolved.insert(service)
        onServicesChanged?(resolved)
        cleanup(sender)
    }

    private func ipv4Address(from service: NetService) -> String {
        print(service)
        guard let addresses = service.addresses else { return "" }
        print(service.addresses)
        for data in addresses {
            let ip = data.withUnsafeBytes { ptr -> String? in
                guard ptr.load(as: sockaddr.self).sa_family == UInt8(AF_INET) else { return nil }
                var addr = ptr.load(as: sockaddr_in.self)
                var buffer = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                inet_ntop(AF_INET, &addr.sin_addr, &buffer, socklen_t(INET_ADDRSTRLEN))
                return String(cString: buffer)
            }
            if let ip { return ip }
        }
        return ""
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        logger.warning("[TGSBonjourBrowser:didNotResolve] '\(sender.name)' failed: \(errorDict)")
        cleanup(sender)
    }

    private func cleanup(_ service: NetService) {
        resolvingServices.removeAll { $0 === service }
    }
}
*/
