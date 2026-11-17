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
    let pInverterAc: Double
    let pLoad: Double
    let pGrid: Double
    let batterySoc:Double
    
    let timestamp: Date

    enum CodingKeys: String, CodingKey {
        case pPv = "p_pv"
        case pInverterAc = "p_inverter_ac"
        case pLoad = "p_load"
        case pGrid = "p_grid"
        case pAkku = "p_akku"
        case batterySoc = "battery_soc"
        case timestamp = "timestamp"
        case  acVoltage = "ac_voltage"
    }

    /// `p_inverter_ac` is decoded leniently so events stored before the field existed still load.
    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        acVoltage = try container.decode(Double.self, forKey: .acVoltage)
        pAkku = try container.decode(Double.self, forKey: .pAkku)
        pPv = try container.decode(Double.self, forKey: .pPv)
        pInverterAc = try container.decodeIfPresent(Double.self, forKey: .pInverterAc) ?? 0
        pLoad = try container.decode(Double.self, forKey: .pLoad)
        pGrid = try container.decode(Double.self, forKey: .pGrid)
        batterySoc = try container.decode(Double.self, forKey: .batterySoc)
        timestamp = try container.decode(Date.self, forKey: .timestamp)
    }
}

struct TeslaState: Codable {
    let present: Bool
    let connected: Bool
    let plugged: Bool
    let charging: Bool
    let chargingAmps: Int
    let chargeLimitPercent: Int
    let currentChargeLevel: Int

    enum CodingKeys: String, CodingKey {
        case present, connected, plugged, charging
        case chargingAmps = "charging_amps"
        case chargeLimitPercent = "charge_limit_percent"
        case currentChargeLevel = "current_charge_level"
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

/// One morning target: the car battery percent wanted on a weekday, 1 = Monday to 7 = Sunday.
/// The server writes it as a two-element array, like `TimeSegment`.
struct TGSMorningTarget: Codable {
    var weekday: Int
    var percent: Int

    init(weekday: Int, percent: Int) {
        self.weekday = weekday
        self.percent = percent
    }

    init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        weekday = try container.decode(Int.self)
        percent = try container.decode(Int.self)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(weekday)
        try container.encode(percent)
    }
}

/// Matches the tracing `LevelFilter` names the server writes to `log_level`.
enum TGSLogLevel: String, CaseIterable, Identifiable {
    case trace = "TRACE", debug = "DEBUG", info = "INFO", warn = "WARN", error = "ERROR", off = "OFF"
    var id: Self { self }
    var title: String { rawValue.capitalized }
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
    /// Not shown in the UI yet, but it must be sent back on save: the server resets it when absent.
    var carMorningTargetCharge: [TGSMorningTarget]
    /// When true the dispatcher observes but never commands the car.
    var bypass: Bool
    /// Kept as a string so an unexpected server value still decodes; see `TGSLogLevel`.
    var logLevel: String

    enum CodingKeys: String, CodingKey {
        case uuid
        case installationDisplayName = "installation_display_name"
        case installationLatitude = "installation_latitude"
        case installationLongitude = "installation_longitude"
        case pollingIntervalSeconds = "polling_interval_seconds"
        case teslaVin = "tesla_vin"
        case teslaMaxChargingAmps = "tesla_max_charging_amps"
        case providerCosts = "provider_costs"
        case carMorningTargetCharge = "car_morning_target_charge"
        case bypass
        case logLevel = "log_level"
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
