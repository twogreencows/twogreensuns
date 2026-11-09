import SwiftUI

struct OneInstallationView: View {
    let service: TGSDiscoveredService

    @StateObject private var viewModel: TGSInstallationEnvironment
    @State private var selectedTab = 0

    init(service: TGSDiscoveredService) {
        self.service = service
        _viewModel = StateObject(wrappedValue: TGSInstallationEnvironment(service: service))
    }

    private var title: String {
        let name = viewModel.config?.installationDisplayName ?? ""
        return name.isEmpty ? service.name : name
    }

    var body: some View {
        TabView(selection: $selectedTab) {
            DashboardView()
                .tabItem { Label("Dashboard", systemImage: "sun.max.fill") }
                .tag(0)

            TeslaView()
                .tabItem { Label("Tesla", systemImage: "bolt.car.fill") }
                .tag(1)

            SettingsView()
                .tabItem { Label("Config", systemImage: "gear") }
                .tag(2)
        }
        .navigationTitle(title)
        .task {
            await viewModel.loadConfig()
            await viewModel.refreshRuntime()
        }
        .environmentObject(viewModel)
    }
}
