import SwiftUI
import Charts

struct EventsView: View {
    private enum DisplayMode: String, CaseIterable, Identifiable {
        case list = "Liste", chart = "Graphique"
        var id: Self { self }
    }

    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var selectedDay = Calendar.current.startOfDay(for: Date())
    @State private var displayMode: DisplayMode = .list

    private var isToday: Bool { Calendar.current.isDateInToday(selectedDay) }

    var body: some View {
        VStack(spacing: 0) {
            dayNavigator
            Picker("Affichage", selection: $displayMode) {
                ForEach(DisplayMode.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.bottom, 8)
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
            switch displayMode {
            case .list: eventList
            case .chart: PowerChart(events: viewModel.events, day: selectedDay)
            }
        }
    }

    private var eventList: some View {
        List(viewModel.events) { event in
            Text(rowText(for: event))
                .font(.system(.footnote, design: .monospaced))
        }
        .listStyle(.plain)
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

// MARK: - Power chart

/// One plotted sample: the value of one power series at one point in time.
private struct PowerSample: Identifiable {
    let id: Int
    let timestamp: Date
    let series: String
    let watts: Double
}

/// Inverter power series over one local day, with a scrub cursor.
/// Samples are flattened once in `init`, so scrubbing (which only touches this
/// view's own state) never re-walks the event list.
private struct PowerChart: View {
    private static let seriesColors: KeyValuePairs<String, Color> = [
        "PV": .orange, "Consommation": .blue, "Réseau": .red, "Batterie": .green
    ]

    private let day: Date
    private let dayEnd: Date
    private let events: [TGSEvent]
    private let samples: [PowerSample]

    @State private var selectedTime: Date?

    init(events: [TGSEvent], day: Date) {
        self.day = day
        self.dayEnd = Calendar.current.date(byAdding: .day, value: 1, to: day) ?? day
        self.events = events
        self.samples = events.flatMap { event in
            Self.powers(of: event).enumerated().map { index, power in
                PowerSample(id: event.id * 4 + index, timestamp: event.timestamp, series: power.series, watts: power.watts)
            }
        }
    }

    /// The four power readings of an event, in legend order.
    private static func powers(of event: TGSEvent) -> [(series: String, watts: Double)] {
        let inv = event.inverter
        return [("PV", inv.pPv), ("Consommation", inv.pLoad), ("Réseau", inv.pGrid), ("Batterie", inv.pAkku)]
    }

    private static func color(for series: String) -> Color {
        seriesColors.first { $0.key == series }?.value ?? .primary
    }

    private var selectedEvent: TGSEvent? {
        selectedTime.flatMap(nearestEvent(to:))
    }

    var body: some View {
        Chart {
            if #available(iOS 18, *) {
                LinePlot(
                    samples,
                    x: .value("Heure", \.timestamp),
                    y: .value("Puissance (W)", \.watts),
                    series: .value("Série", \.series)
                )
                .foregroundStyle(by: .value("Série", \.series))
            } else {
                ForEach(samples) { sample in
                    LineMark(
                        x: .value("Heure", sample.timestamp),
                        y: .value("Puissance (W)", sample.watts),
                        series: .value("Série", sample.series)
                    )
                    .foregroundStyle(by: .value("Série", sample.series))
                }
            }

            if let selected = selectedEvent {
                RuleMark(x: .value("Heure", selected.timestamp))
                    .foregroundStyle(.secondary)
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 4]))
                    .annotation(position: .top, spacing: 0,
                                overflowResolution: .init(x: .fit(to: .chart), y: .disabled)) {
                        selectionCard(for: selected)
                    }
            }
        }
        .chartXSelection(value: $selectedTime)
        .chartForegroundStyleScale(Self.seriesColors)
        .chartXScale(domain: day...dayEnd)
        .chartXAxis {
            AxisMarks(values: .stride(by: .hour, count: 3)) { _ in
                AxisGridLine()
                AxisTick()
                AxisValueLabel(format: .dateTime.hour(.twoDigits(amPM: .omitted)))
            }
        }
        .chartYAxisLabel("W")
        .chartLegend(position: .bottom)
        .padding()
    }

    private func selectionCard(for event: TGSEvent) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(event.timestamp, format: .dateTime.hour().minute().second())
                .font(.caption.bold())
            ForEach(Self.powers(of: event), id: \.series) { power in
                HStack(spacing: 4) {
                    Circle().fill(Self.color(for: power.series)).frame(width: 6, height: 6)
                    Text(power.series)
                    Spacer(minLength: 8)
                    Text("\(Int(power.watts)) W").monospacedDigit()
                }
                .font(.caption2)
            }
        }
        .padding(6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
    }

    /// Binary search over the timestamp-sorted events for the closest one.
    private func nearestEvent(to time: Date) -> TGSEvent? {
        guard !events.isEmpty else { return nil }
        var low = 0
        var high = events.count - 1
        while low < high {
            let mid = (low + high) / 2
            if events[mid].timestamp < time { low = mid + 1 } else { high = mid }
        }
        if low > 0,
           abs(events[low - 1].timestamp.timeIntervalSince(time)) < abs(events[low].timestamp.timeIntervalSince(time)) {
            return events[low - 1]
        }
        return events[low]
    }
}
