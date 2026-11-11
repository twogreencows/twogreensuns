import Foundation

struct TGSDiscoveredService: Identifiable, Hashable {
    let name: String
    let host: String
    let port: Int
    let ipAddress: String

    var id: String { name }
    var baseURL: String { "http://\(ipAddress):\(port)" }

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.name == rhs.name }
    func hash(into hasher: inout Hasher) { hasher.combine(name) }
}




struct InverterData: Codable {
    let acVoltage:Double
    let pAkku: Double
    let pPv: Double
    let pLoad: Double
    let pGrid: Double
    let batterySoc:Double
    
    let timestamp: Date

    enum CodingKeys: String, CodingKey {
        case pPv = "p_pv"
        case pLoad = "p_load"
        case pGrid = "p_grid"
        case pAkku = "p_akku"
        case batterySoc = "battery_soc"
        case timestamp = "timestamp"
        case  acVoltage = "ac_voltage"
    }
}

struct TeslaState: Codable {
    let connected: Bool
    let plugged: Bool
    let charging: Bool
    let chargingAmps: Int

    enum CodingKeys: String, CodingKey {
        case plugged = "plugged"
        case connected = "connected"
        case charging = "charging"
        case chargingAmps = "charging_amps"
    }
}


struct RuntimeResponse: Codable {
    //var uuid: String
    let inverter: InverterData?
    let tesla: TeslaState?
}

/// Mirrors the server's `TGSTeslaCarCommand` enum. serde writes unit variants as
/// plain strings ("WakeUp") and value variants as one-key objects ({"SetChargingAmps": 12}).
struct TGSTeslaCarCommand: Codable {
    let name: String
    let value: Int?

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let text = try? container.decode(String.self) {
            name = text; value = nil
        } else {
            let dict = try container.decode([String: Int].self)
            name = dict.keys.first ?? "?"; value = dict.values.first
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        if let value { try container.encode([name: value]) } else { try container.encode(name) }
    }

    var text: String { value.map { "\(name)(\($0))" } ?? name }
}

/// One poll cycle stored by the server: inverter reading, car state, and the commands sent.
struct TGSEvent: Codable, Identifiable {
    let id: Int
    let timestamp: Date
    let inverter: InverterData
    let tesla: TeslaState?
    let actions: [TGSTeslaCarCommand]
}

struct NaiveTime: Comparable {
    let secondsSinceMidnight: Int

    init(hour: Int, minute: Int, second: Int = 0) {
        self.secondsSinceMidnight = hour * 3600 + minute * 60 + second
    }

    static func < (lhs: NaiveTime, rhs: NaiveTime) -> Bool {
        lhs.secondsSinceMidnight < rhs.secondsSinceMidnight
    }
}

struct TimeSegment: Codable {
    var start: String
    var end: String
    var value: Double

    init(start: String, end: String, value: Double) {
        self.start = start
        self.end = end
        self.value = value
    }

    // Custom decoding: read the array positionally
    init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        start = try container.decode(String.self)
        end = try container.decode(String.self)
        value = try container.decode(Double.self)
    }

    // Custom encoding, if you need to write it back out
    func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(start)
        try container.encode(end)
        try container.encode(value)
    }
}

struct TGSConfiguration: Codable {
    var uuid: String
    var installationDisplayName: String
    var installationLatitude: Double
    var installationLongitude: Double
    var pollingIntervalSeconds: Int
    var teslaVin: String?
    var teslaMaxChargingAmps: Int
    var providerCosts:[TimeSegment]

    enum CodingKeys: String, CodingKey {
        case uuid
        case installationDisplayName = "installation_display_name"
        case installationLatitude = "installation_latitude"
        case installationLongitude = "installation_longitude"
        case pollingIntervalSeconds = "polling_interval_seconds"
        case teslaVin = "tesla_vin"
        case teslaMaxChargingAmps = "tesla_max_charging_amps"
        case providerCosts = "provider_costs"
    }

    var pollingIntervalMinutes: Double {
        get { Double(pollingIntervalSeconds) / 60 }
        set { pollingIntervalSeconds = Int(newValue * 60) }
    }
    
}


//struct AppConfig: Codable {
//    
//    
//    
//   // var httpPort: Int
//  //  var mdnsHostname: String
//
//    enum CodingKeys: String, CodingKey {
//        case uuid
//        case inverterIpv4Address = "inverter_ipv4_address"
//        case installationDisplayName = "installation_display_name"
//        case installationLatitude = "installation_latitude"
//        case installationLongitude = "installation_longitude"
//        case pollingIntervalMinutes = "polling_interval_minutes"
//        case teslaVin = "tesla_vin"
//        case teslaMaxAmps = "tesla_max_amps"
//        case httpPort = "http_port"
//        case mdnsHostname = "mdns_hostname"
//    }
//}
