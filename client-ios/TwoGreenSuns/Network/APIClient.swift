import Foundation
import os

private let logger = Logger(subsystem: "com.twogreensuns", category: "TGSAPIClient")

enum APIError: Error, LocalizedError {
    case invalidURL
    case httpError(Int)
    case decodingError(Error)

    var errorDescription: String? {
        switch self {
        case .invalidURL: return "Invalid server URL"
        case .httpError(let code): return "HTTP \(code)"
        case .decodingError(let e): return "Decoding error: \(e.localizedDescription)"
        }
    }
}

// NetServiceBrowser is deprecated in iOS 15+ in favour of NWBrowser,
// but it remains the simplest API that directly yields host + port after resolution.
final class TGSAPIClient: NSObject {
    private var baseURL: URL
    private let session: URLSession
    private let decoder: JSONDecoder

    private var netServiceBrowser: NetServiceBrowser?
    private var resolvingService: NetService?

    override init() {
        // Fallback URL used until Bonjour resolves the real address.
        baseURL = URL(string: "http://twogreensuns.local:8080/api/v1")!
        session = URLSession.shared
        decoder = JSONDecoder()
        super.init()
        logger.debug("[TGSAPIClient:init] Starting with fallback URL: \(self.baseURL.absoluteString)")
        startBonjourDiscovery()
    }

    // MARK: - Bonjour discovery

    private func startBonjourDiscovery() {
        logger.debug("[TGSAPIClient:startBonjourDiscovery] Browsing for _twogreensuns._tcp. on local.")
        let browser = NetServiceBrowser()
        browser.delegate = self
        browser.searchForServices(ofType: "_twogreensuns._tcp.", inDomain: "local.")
        netServiceBrowser = browser
    }

    // MARK: - Status

    func status() async throws -> StatusResponse {
        try await get("status")
    }

    // MARK: - Config

    func getConfig() async throws -> AppConfig {
        try await get("config")
    }

    func putConfig(_ config: AppConfig) async throws {
        try await put("config", body: config)
    }

    // MARK: - Log level

    func setLogLevel(_ level: String) async throws {
        struct Payload: Encodable { let level: String }
        try await put("log/level", body: Payload(level: level))
    }

    // MARK: - Tesla

    func teslaDiscover() async throws {
        try await post("tesla/discover")
    }

    func teslaStartCharging() async throws {
        try await post("tesla/charge/start")
    }

    func teslaStopCharging() async throws {
        try await post("tesla/charge/stop")
    }

    func teslaSetAmps(_ amps: Int) async throws {
        try await post("tesla/charge/amps/\(amps)")
    }

    // MARK: - Helpers

    private func get<T: Decodable>(_ path: String) async throws -> T {
        let url = baseURL.appendingPathComponent(path)
        let (data, response) = try await session.data(from: url)
        try validate(response)
        return try decode(data)
    }

    private func post(_ path: String) async throws {
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "POST"
        let (_, response) = try await session.data(for: request)
        try validate(response)
    }

    private func put<T: Encodable>(_ path: String, body: T) async throws {
        var request = URLRequest(url: baseURL.appendingPathComponent(path))
        request.httpMethod = "PUT"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)
        let (_, response) = try await session.data(for: request)
        try validate(response)
    }

    private func validate(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse else { return }
        guard (200..<300).contains(http.statusCode) else {
            throw APIError.httpError(http.statusCode)
        }
    }

    private func decode<T: Decodable>(_ data: Data) throws -> T {
        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw APIError.decodingError(error)
        }
    }
}

// MARK: - NetServiceBrowserDelegate

extension TGSAPIClient: NetServiceBrowserDelegate {
    func netServiceBrowser(
        _ browser: NetServiceBrowser,
        didFind service: NetService,
        moreComing: Bool
    ) {
        logger.debug("[TGSAPIClient:didFind] Found service '\(service.name)' — resolving...")
        browser.stop()
        resolvingService = service
        service.delegate = self
        service.resolve(withTimeout: 5)
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
        logger.warning("[TGSAPIClient:didNotSearch] Browse failed: \(errorDict) — keeping fallback URL")
    }
}

// MARK: - NetServiceDelegate

extension TGSAPIClient: NetServiceDelegate {
    func netServiceDidResolveAddress(_ sender: NetService) {
        guard let host = sender.hostName, sender.port > 0 else {
            logger.warning("[TGSAPIClient:didResolveAddress] Resolved but host/port invalid — keeping fallback URL")
            return
        }
        if let url = URL(string: "http://\(host):\(sender.port)/api/v1") {
            baseURL = url
            logger.info("[TGSAPIClient:didResolveAddress] Server URL updated to \(url.absoluteString)")
        }
        resolvingService = nil
    }

    func netService(_ sender: NetService, didNotResolve errorDict: [String: NSNumber]) {
        logger.warning("[TGSAPIClient:didNotResolve] Resolution failed: \(errorDict) — keeping fallback URL")
        resolvingService = nil
    }
}
