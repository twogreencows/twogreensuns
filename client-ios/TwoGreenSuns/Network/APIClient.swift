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

final class TGSAPIClient {
    private let baseURL: URL
    private let session: URLSession
    private let decoder: JSONDecoder

    private static let iso8601WithFractionalSeconds: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    private static let iso8601 = ISO8601DateFormatter()

    init(baseURL: URL) {
        self.baseURL = baseURL
        self.session = URLSession.shared
        self.decoder = JSONDecoder()
        self.decoder.dateDecodingStrategy = .custom { decoder in
            let container = try decoder.singleValueContainer()
            let string = try container.decode(String.self)
            if let date = Self.iso8601WithFractionalSeconds.date(from: string) {
                return date
            }
            if let date = Self.iso8601.date(from: string) {
                return date
            }
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Expected ISO 8601 date, got \(string)"
            )
        }
        logger.debug("[TGSAPIClient:init] baseURL=\(baseURL.absoluteString)")
    }

    // MARK: - Status

    func runtime() async throws -> RuntimeResponse {
        try await get("runtime")
    }

    // MARK: - Config

    func getConfig() async throws -> TGSConfiguration {
        try await get("config")
    }

    func putConfig(_ config: TGSConfiguration) async throws {
        try await put("config", body: config)
    }

    // MARK: - Events

    /// Returns every event with `from <= timestamp < to`. Both bounds are sent as
    /// RFC 3339 UTC strings, so callers can pass local-day boundaries directly.
    func events(from: Date, to: Date) async throws -> [TGSEvent] {
        try await get("events", query: [
            URLQueryItem(name: "from", value: Self.iso8601.string(from: from)),
            URLQueryItem(name: "to", value: Self.iso8601.string(from: to))
        ])
    }

    // MARK: - Log level

    func setLogLevel(_ level: String) async throws {
        struct Payload: Encodable { let level: String }
        try await put("log/level", body: Payload(level: level))
    }

    // MARK: - Tesla

    /*func teslaDiscover() async throws {
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
    }*/

    // MARK: - Helpers

    private func get<T: Decodable>(_ path: String, query: [URLQueryItem] = []) async throws -> T {
        var components = URLComponents(url: baseURL.appendingPathComponent(path), resolvingAgainstBaseURL: false)
        if !query.isEmpty { components?.queryItems = query }
        guard let url = components?.url else { throw APIError.invalidURL }
        print("sending to \(url)")
        let (data, response) = try await session.data(from: url)
        print("[APIClient] raw response: \(String(data: data, encoding: .utf8) ?? "<binary>")")
        print(response)
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
