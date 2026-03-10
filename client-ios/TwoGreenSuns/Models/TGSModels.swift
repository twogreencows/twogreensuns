import Foundation

struct TGSDiscoveredService: Identifiable, Hashable {
    let name: String
    let host: String
    let port: Int

    var id: String { name }
    var baseURL: String { "http://\(host):\(port)" }

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.name == rhs.name }
    func hash(into hasher: inout Hasher) { hasher.combine(name) }
}

struct InverterData: Codable {
    let pPv: Double
    let pLoad: Double
    let pGrid: Double
    let pAkku: Double
    let timestamp: String

    enum CodingKeys: String, CodingKey {
        case pPv = "p_pv"
        case pLoad = "p_load"
        case pGrid = "p_grid"
        case pAkku = "p_akku"
        case timestamp
    }
}

struct TeslaState: Codable {
    let connected: Bool
    let charging: Bool
    let chargingAmps: Int

    enum CodingKeys: String, CodingKey {
        case connected
        case charging
        case chargingAmps = "charging_amps"
    }
}

struct StatusResponse: Codable {
    let inverter: InverterData?
    let tesla: TeslaState?
}

struct AppConfig: Codable {
    var uuid: String
    var inverterIpv4Address: String
    var installationDisplayName: String
    var installationLatitude: Double
    var installationLongitude: Double
    var pollingIntervalMinutes: Int
    var teslaVin: String?
    var teslaMaxAmps: Int
    var httpPort: Int
    var mdnsHostname: String
    var useMockBle: Bool

    enum CodingKeys: String, CodingKey {
        case uuid
        case inverterIpv4Address = "inverter_ipv4_address"
        case installationDisplayName = "installation_display_name"
        case installationLatitude = "installation_latitude"
        case installationLongitude = "installation_longitude"
        case pollingIntervalMinutes = "polling_interval_minutes"
        case teslaVin = "tesla_vin"
        case teslaMaxAmps = "tesla_max_amps"
        case httpPort = "http_port"
        case mdnsHostname = "mdns_hostname"
        case useMockBle = "use_mock_ble"
    }
}
