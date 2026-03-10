import SwiftUI

@main
struct TwoGreenSunsApp: App {
    @StateObject private var viewModel = TGSEnvironnement()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
        }
    }
}
