import SwiftUI

struct DashboardView: View {
    @EnvironmentObject var viewModel: TGSInstallationEnvironment

    private let refreshInterval: Duration = .seconds(10)

    var body: some View {
        NavigationStack {
            ScrollView {
                if let runtime = viewModel.runtime {
                    VStack(spacing: 16) {
                        if let inverter = runtime.inverter {
                            InverterCard(data: inverter)
                        } else {
                            Image("inverter_fronius_gen24")
                            ContentUnavailableView(
                                "No inverter data",
                                systemImage: "exclamationmark.triangle",
                                description: Text("Waiting for the first poll cycle.")
                            )
                        }

                        if let tesla = runtime.tesla {
                            TeslaStatusCard(state: tesla)
                        }
                    }
                    .padding()
                } else if viewModel.isLoading {
                    ProgressView("Loading…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ContentUnavailableView(
                        "Unable to connect",
                        systemImage: "wifi.slash",
                        description: Text(viewModel.errorMessage ?? "Check server connection.")
                    )
                }
            }
            .navigationTitle("Dashboard")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button { Task { await viewModel.refreshRuntime() } } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }
            }
            .refreshable { await viewModel.refreshRuntime() }
            .task {
                await viewModel.refreshRuntime()
                while !Task.isCancelled {
                    try? await Task.sleep(for: refreshInterval)
                    guard !Task.isCancelled else { return }
                    await viewModel.refreshRuntime()
                }
            }
        }
    }
}

private struct InverterCard: View {
    let data: InverterData?

    var body: some View {
        GroupBox {
            if let data {
                VStack(spacing: 12) {
                    Image("inverter_fronius_gen24")
                        .frame(maxWidth: .infinity, alignment: .center)
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                        PowerRow(label: "Solar (PV)", value: data.pPv, color: .yellow)
                        PowerRow(label: "House load", value: data.pLoad, color: .blue)
                        PowerRow(label: "Grid", value: data.pGrid, color: .orange)
                        PowerRow(label: "Battery", value: data.pAkku, color: .green)
                    }
                }
            } else {
                Image("inverter_fronius_gen24")
                    .frame(maxWidth: .infinity, alignment: .center)
                ContentUnavailableView(
                    "No inverter data",
                    systemImage: "exclamationmark.triangle",
                    description: Text("Waiting for the first poll cycle.")
                )
            }
        } label: {
            Text(titleText)
        }
    }

    private var titleText: String {
        guard let data else { return "Solar Inverter" }
        return "Solar Inverter (\(data.timestamp.formatted(date: .omitted, time: .standard)))"
    }
}

private struct PowerRow: View {
    let label: String
    let value: Double
    let color: Color

    var body: some View {
        GridRow {
            Circle().fill(color).frame(width: 8, height: 8)
            Text(label).foregroundStyle(.secondary)
            Spacer()
            Text(String(format: "%.0f W", value)).monospacedDigit().bold()
        }
    }
}

private struct TeslaStatusCard: View {
    let state: TeslaState

    var body: some View {
        GroupBox("Tesla") {
            Image("tesla")
            LabeledContent("Connected", value: state.connected ? "Yes" : "No")
            LabeledContent("Plugged", value: state.plugged ? "Yes" : "No")
            LabeledContent("Charging", value: state.charging ? "Yes" : "No")
            LabeledContent("Amps", value: "\(state.chargingAmps) A")
        }
    }
}
