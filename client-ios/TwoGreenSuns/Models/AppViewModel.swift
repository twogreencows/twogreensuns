import Foundation

// MARK: - App-level environment (discovery only)

@MainActor
final class TGSAppEnvironment: ObservableObject {
    @Published var discoveredServices: [TGSDiscoveredService] = []

    private let bonjourBrowser = TGSBonjourBrowser()

    init() {
        bonjourBrowser.onServicesChanged = { [weak self] services in
            Task { @MainActor [weak self] in
                self?.discoveredServices = services.sorted { $0.name < $1.name }
            }
        }
        bonjourBrowser.start()
    }
}

// MARK: - Per-installation environment

@MainActor
final class TGSInstallationEnvironment: ObservableObject {
    @Published var runtime: RuntimeResponse?
    @Published var config: TGSConfiguration?
    @Published var isLoading = false
    @Published var errorMessage: String?

    let api: TGSAPIClient

    init(service: TGSDiscoveredService) {
        print(service.baseURL);
        let baseURL = URL(string: "\(service.baseURL)/api/v1")!
        self.api = TGSAPIClient(baseURL: baseURL)
    }

    func refreshRuntime() async {
        print("refreshRuntime");
        isLoading = true
        defer { isLoading = false }
        do {
            runtime = try await api.runtime()
            print(runtime)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadConfig() async {
        do {
            config = try await api.getConfig()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func saveConfig(_ cfg: TGSConfiguration) async {
        do {
            try await api.putConfig(cfg)
            config = cfg
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: - Events

    @Published var events: [TGSEvent] = []

    /// Loads the events of one calendar day in the device's local time zone.
    /// The local midnight boundaries are converted to UTC instants for the server.
    func loadEvents(for day: Date) async {
        let calendar = Calendar.current
        let start = calendar.startOfDay(for: day)
        guard let end = calendar.date(byAdding: .day, value: 1, to: start) else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            events = try await api.events(from: start, to: end)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: - Car

    @Published var isSendingCarCommand = false

    func carStartCharging() async { await sendCarCommand("start") }
    func carStopCharging() async { await sendCarCommand("stop") }
    func carReadChargeState() async { await sendCarCommand("chargestate") }

    /// Sends a command and stores the car state the server returns.
    private func sendCarCommand(_ command: String, value: Int? = nil) async {
        isSendingCarCommand = true
        defer { isSendingCarCommand = false }
        do {
            let tesla = try await api.carCommand(command, value: value)
            runtime = RuntimeResponse(inverter: runtime?.inverter, tesla: tesla)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
