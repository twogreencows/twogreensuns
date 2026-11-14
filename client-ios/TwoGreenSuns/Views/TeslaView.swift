import SwiftUI

struct TeslaView: View {
    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var showErrorAlert = false

    var body: some View {
        Form {
            discoverySection
            chargingSection
            stateSection
        }
        .navigationTitle("Tesla")
        .alert("Car command failed",
               isPresented: $showErrorAlert,
               presenting: viewModel.errorMessage) { _ in
            Button("OK", role: .cancel) { }
        } message: { message in
            Text(message)
        }
    }

    private var discoverySection: some View {
        Section("Discovery") {
            Button("Discover Tesla via BLE") {
                //Task { await viewModel.teslaDiscover() }
            }
        }
    }

    private var chargingSection: some View {
        Section("Charging") {
            Button("Start charging") {
                performCarCommand { await viewModel.carStartCharging() }
            }
            .tint(.green)

            Button("Stop charging") {
                performCarCommand { await viewModel.carStopCharging() }
            }
            .tint(.red)
        }
        .disabled(viewModel.isSendingCarCommand)
    }

    private var stateSection: some View {
        Section("Current state") {
            Button {
                performCarCommand { await viewModel.carReadChargeState() }
            } label: {
                HStack {
                    Text("Get charge state")
                    Spacer()
                    if viewModel.isSendingCarCommand { ProgressView() }
                }
            }
            .disabled(viewModel.isSendingCarCommand)

            if let tesla = viewModel.runtime?.tesla {
                LabeledContent("Present", value: tesla.present ? "Yes" : "No")
                LabeledContent("Connected", value: tesla.connected ? "Yes" : "No")
                LabeledContent("Plugged", value: tesla.plugged ? "Yes" : "No")
                LabeledContent("Charging", value: tesla.charging ? "Yes" : "No")
                LabeledContent("Charging amps", value: "\(tesla.chargingAmps) A")
                LabeledContent("Charge limit", value: "\(tesla.chargeLimitPercent) %")
            } else {
                Text("No car state yet")
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// Runs a car command and surfaces any error it leaves in the view model.
    private func performCarCommand(_ command: @escaping () async -> Void) {
        viewModel.errorMessage = nil
        Task {
            await command()
            if viewModel.errorMessage != nil { showErrorAlert = true }
        }
    }
}
