import SwiftUI

struct EventsView: View {
    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var selectedDay = Calendar.current.startOfDay(for: Date())

    private var isToday: Bool { Calendar.current.isDateInToday(selectedDay) }

    var body: some View {
        VStack(spacing: 0) {
            dayNavigator
            Divider()
            content
        }
        .navigationTitle("Données")
        // Runs on first appearance and again each time the selected day changes.
        .task(id: selectedDay) { await viewModel.loadEvents(for: selectedDay) }
    }

    // MARK: - Day navigation

    private var dayNavigator: some View {
        HStack {
            Button { shiftDay(by: -1) } label: { Image(systemName: "chevron.left") }
            Spacer()
            VStack(spacing: 2) {
                Text(selectedDay, format: .dateTime.weekday(.wide).day().month(.wide).year())
                    .font(.headline)
                if !isToday {
                    Button("Aujourd'hui") { selectedDay = Calendar.current.startOfDay(for: Date()) }
                        .font(.footnote)
                }
            }
            Spacer()
            Button { shiftDay(by: 1) } label: { Image(systemName: "chevron.right") }
                .disabled(isToday)
        }
        .padding()
    }

    private func shiftDay(by days: Int) {
        if let next = Calendar.current.date(byAdding: .day, value: days, to: selectedDay) {
            selectedDay = next
        }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        if viewModel.isLoading && viewModel.events.isEmpty {
            Spacer()
            ProgressView("Chargement…")
            Spacer()
        } else if viewModel.events.isEmpty {
            Spacer()
            Text("Aucune donnée")
                .foregroundStyle(.secondary)
            Spacer()
        } else {
            List(viewModel.events) { event in
                Text(rowText(for: event))
                    .font(.system(.footnote, design: .monospaced))
            }
            .listStyle(.plain)
        }
    }

    /// One compact line per poll: time, inverter powers, battery SOC, Tesla state, car commands.
    private func rowText(for event: TGSEvent) -> String {
        let inv = event.inverter
        var parts = [
            event.timestamp.formatted(date: .omitted, time: .standard),
            "PV \(Int(inv.pPv))W", "Load \(Int(inv.pLoad))W", "Grid \(Int(inv.pGrid))W",
            "Akku \(Int(inv.pAkku))W", "SOC \(Int(inv.batterySoc))%"
        ]
        if let tesla = event.tesla {
            let state = tesla.charging ? "charging \(tesla.chargingAmps)A" : tesla.plugged ? "plugged" : "idle"
            parts.append("Tesla \(state)")
        } else {
            parts.append("Tesla —")
        }
        if !event.actions.isEmpty {
            parts.append(event.actions.map(\.text).joined(separator: ","))
        }
        return parts.joined(separator: " · ")
    }
}
