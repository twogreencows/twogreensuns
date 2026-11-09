import SwiftUI

struct TeslaView: View {
    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var targetAmps: Double = 8

    var body: some View {
        Form {
            Section("Discovery") {
                Button("Discover Tesla via BLE") {
                    //Task { await viewModel.teslaDiscover() }
                }
            }

            Section("Charging") {
//                    Button("Start charging") {
//                        //Task { await viewModel.teslaStartCharging() }
//                    }
//                    .tint(.green)
//
//                    Button("Stop charging") {
//                        Task { await viewModel.teslaStopCharging() }
//                    }
//                    .tint(.red)
//
//                    VStack(alignment: .leading) {
//                        Text("Set amps: \(Int(targetAmps)) A")
//                        Slider(value: $targetAmps, in: 1...16, step: 1)
//                        Button("Apply") {
//                            //Task { await viewModel.teslaSetAmps(Int(targetAmps)) }
//                        }
//                        .buttonStyle(.bordered)
//                    }
            }

            if let tesla = viewModel.runtime?.tesla {
                Section("Current state") {
                    LabeledContent("Connected", value: tesla.connected ? "Yes" : "No")
                    LabeledContent("Charging", value: tesla.charging ? "Yes" : "No")
                    LabeledContent("Charging amps", value: "\(tesla.chargingAmps) A")
                }
            }
        }
        .navigationTitle("Tesla")
    }
}
