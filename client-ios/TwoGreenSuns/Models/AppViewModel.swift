import Foundation
import Combine

@MainActor
final class TGSEnvironnement: ObservableObject {
    @Published var status: StatusResponse?
    @Published var config: AppConfig?
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var discoveredServices: [TGSDiscoveredService] = []

    let api = TGSAPIClient()
    private let bonjourBrowser = TGSBonjourBrowser()

    init() {
        bonjourBrowser.onServicesChanged = { [weak self] services in
            Task { @MainActor [weak self] in
                self?.discoveredServices = services.sorted { $0.name < $1.name }
            }
        }
        bonjourBrowser.start()
    }

    func refreshStatus() async {
        isLoading = true
        defer { isLoading = false }
        do {
            status = try await api.status()
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

    func saveConfig(_ cfg: AppConfig) async {
        do {
            try await api.putConfig(cfg)
            config = cfg
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func teslaDiscover() async {
        do {
            try await api.teslaDiscover()
            await refreshStatus()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func teslaStartCharging() async {
        do {
            try await api.teslaStartCharging()
            await refreshStatus()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func teslaStopCharging() async {
        do {
            try await api.teslaStopCharging()
            await refreshStatus()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func teslaSetAmps(_ amps: Int) async {
        do {
            try await api.teslaSetAmps(amps)
            await refreshStatus()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
