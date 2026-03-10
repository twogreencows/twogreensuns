import SwiftUI

struct ContentView: View {
    @EnvironmentObject var viewModel: TGSEnvironnement

    var body: some View {
        TabView {
            DashboardView()
                .tabItem { Label("Dashboard", systemImage: "sun.max.fill") }

            TeslaView()
                .tabItem { Label("Tesla", systemImage: "bolt.car.fill") }

            SettingsView()
                .tabItem { Label("Settings", systemImage: "gear") }
        }
        .task { await viewModel.refreshStatus() }
    }
}

#Preview {
    ContentView()
        .environmentObject(TGSEnvironnement())
}
