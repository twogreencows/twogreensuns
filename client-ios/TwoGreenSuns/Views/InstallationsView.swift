import SwiftUI

struct TGSInstallationsView: View {
    @EnvironmentObject var appEnvironment: TGSAppEnvironment

    var body: some View {
        NavigationStack {
            Group {
                if appEnvironment.discoveredServices.isEmpty {
                    VStack(spacing: 16) {
                        ProgressView()
                            .scaleEffect(1.5)
                        Text("Looking for TwoGreenSuns installations…")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    List(appEnvironment.discoveredServices) { service in
                        NavigationLink(destination: OneInstallationView(service: service)) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(service.name)
                                    .font(.headline)
                                Text(service.ipAddress.isEmpty ? service.host : service.ipAddress)
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 2)
                        }
                    }
                }
            }
            .navigationTitle("Installations")
        }
    }
}
