import Foundation
import os

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
        resolved.remove(TGSDiscoveredService(name: service.name, host: "", port: 0))
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
        let service = TGSDiscoveredService(name: sender.name, host: host, port: sender.port)
        logger.info("[TGSBonjourBrowser:didResolveAddress] '\(service.name)' → \(service.baseURL)")
        resolved.insert(service)
        onServicesChanged?(resolved)
        cleanup(sender)
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        logger.warning("[TGSBonjourBrowser:didNotResolve] '\(sender.name)' failed: \(errorDict)")
        cleanup(sender)
    }

    private func cleanup(_ service: NetService) {
        resolvingServices.removeAll { $0 === service }
    }
}
