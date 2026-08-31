import SwiftUI

struct OneInstallationView: View {
    let service: TGSDiscoveredService

    @StateObject private var viewModel: TGSInstallationEnvironment

    init(service: TGSDiscoveredService) {
        self.service = service
        _viewModel = StateObject(wrappedValue: TGSInstallationEnvironment(service: service))
    }

    var body: some View {
        TabView {
            DashboardView()
                .tabItem { Label("Dashboard", systemImage: "sun.max.fill") }

            TeslaView()
                .tabItem { Label("Tesla", systemImage: "bolt.car.fill") }

            SettingsView()
                .tabItem { Label("Config", systemImage: "gear") }
        }
        .navigationTitle(service.name)
        .task { await viewModel.refreshRuntime() }
        .environmentObject(viewModel)
    }
}
