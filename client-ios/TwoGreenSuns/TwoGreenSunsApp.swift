import SwiftUI

@main
struct TwoGreenSunsApp: App {
    @StateObject private var appEnvironment = TGSAppEnvironment()

    var body: some Scene {
        WindowGroup {
            TGSInstallationsView()
                .environmentObject(appEnvironment)
        }
    }
}
