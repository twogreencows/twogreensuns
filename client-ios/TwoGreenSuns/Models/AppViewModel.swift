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

//    func teslaDiscover() async {
//        do {
//            try await api.teslaDiscover()
//            await refreshStatus()
//        } catch {
//            errorMessage = error.localizedDescription
//        }
//    }

    //func teslaStartCharging() async {
    //    do {
//            try await api.teslaStartCharging()
//            await refreshStatus()
//        } catch {
//            errorMessage = error.localizedDescription
//        }
//    }
//
//    func teslaStopCharging() async {
//        do {
//            try await api.teslaStopCharging()
//            await refreshStatus()
//        } catch {
//            errorMessage = error.localizedDescription
//        }
//    }

//    func teslaSetAmps(_ amps: Int) async {
//        do {
//            try await api.teslaSetAmps(amps)
//            await refreshStatus()
//        } catch {
//            errorMessage = error.localizedDescription
//        }
//    }
}
