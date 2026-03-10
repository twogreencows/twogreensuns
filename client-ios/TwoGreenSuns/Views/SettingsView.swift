import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var viewModel: TGSEnvironnement
    @State private var draft: AppConfig?

    var body: some View {
        NavigationStack {
            Group {
                if var cfg = draft {
                    Form {
                        Section("Inverter") {
                            LabeledContent("IP Address") {
                                TextField("192.168.1.x", text: Binding(
                                    get: { cfg.inverterIpv4Address },
                                    set: { cfg.inverterIpv4Address = $0; draft = cfg }
                                ))
                                .keyboardType(.decimalPad)
                                .multilineTextAlignment(.trailing)
                            }
                            LabeledContent("Poll interval (min)") {
                                TextField("10", value: Binding(
                                    get: { cfg.pollingIntervalMinutes },
                                    set: { cfg.pollingIntervalMinutes = $0; draft = cfg }
                                ), format: .number)
                                .keyboardType(.numberPad)
                                .multilineTextAlignment(.trailing)
                            }
                        }

                        Section("Tesla") {
                            LabeledContent("VIN") {
                                TextField("optional", text: Binding(
                                    get: { cfg.teslaVin ?? "" },
                                    set: { cfg.teslaVin = $0.isEmpty ? nil : $0; draft = cfg }
                                ))
                                .multilineTextAlignment(.trailing)
                            }
                            LabeledContent("Max amps") {
                                TextField("16", value: Binding(
                                    get: { cfg.teslaMaxAmps },
                                    set: { cfg.teslaMaxAmps = $0; draft = cfg }
                                ), format: .number)
                                .keyboardType(.numberPad)
                                .multilineTextAlignment(.trailing)
                            }
                        }

                        Section {
                            Button("Save") {
                                Task { await viewModel.saveConfig(cfg) }
                            }
                        }
                    }
                } else {
                    ProgressView("Loading config…")
                        .task { await viewModel.loadConfig(); draft = viewModel.config }
                }
            }
            .navigationTitle("Settings")
        }
    }
}
