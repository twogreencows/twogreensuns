import SwiftUI

struct DashboardView: View {
    @EnvironmentObject var viewModel: TGSEnvironnement

    var body: some View {
        NavigationStack {
            ScrollView {
                if let status = viewModel.status {
                    VStack(spacing: 16) {
                        if let inverter = status.inverter {
                            InverterCard(data: inverter)
                        } else {
                            ContentUnavailableView(
                                "No inverter data",
                                systemImage: "exclamationmark.triangle",
                                description: Text("Waiting for the first poll cycle.")
                            )
                        }

                        if let tesla = status.tesla {
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
            .safeAreaInset(edge: .bottom) {
                if !viewModel.discoveredServices.isEmpty {
                    TGSDiscoveredServicesBar(services: viewModel.discoveredServices)
                }
            }
            .navigationTitle("Dashboard")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button { Task { await viewModel.refreshStatus() } } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }
            }
            .refreshable { await viewModel.refreshStatus() }
        }
    }
}

private struct TGSDiscoveredServicesBar: View {
    let services: [TGSDiscoveredService]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Servers on network").font(.caption).foregroundStyle(.secondary)
            ForEach(services) { svc in
                Label {
                    Text(svc.name).bold() + Text("  \(svc.baseURL)").foregroundStyle(.secondary)
                } icon: {
                    Image(systemName: "server.rack").foregroundStyle(.green)
                }
                .font(.caption)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial)
    }
}

private struct InverterCard: View {
    let data: InverterData

    var body: some View {
        GroupBox("Solar Inverter") {
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                PowerRow(label: "Solar (PV)", value: data.pPv, color: .yellow)
                PowerRow(label: "House load", value: data.pLoad, color: .blue)
                PowerRow(label: "Grid", value: data.pGrid, color: .orange)
                PowerRow(label: "Battery", value: data.pAkku, color: .green)
            }
        }
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
            LabeledContent("Connected", value: state.connected ? "Yes" : "No")
            LabeledContent("Charging", value: state.charging ? "Yes" : "No")
            LabeledContent("Amps", value: "\(state.chargingAmps) A")
        }
    }
}
