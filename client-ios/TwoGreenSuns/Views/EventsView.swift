import SwiftUI
import Charts

struct EventsView: View {
    private enum DisplayMode: CaseIterable, Identifiable {
        case chart, list

        var id: Self { self }

        var title: LocalizedStringResource {
            switch self {
            case .chart: "Chart"
            case .list: "List"
            }
        }
    }

    private enum SortOrder: CaseIterable, Identifiable {
        case newestFirst, oldestFirst

        var id: Self { self }

        var title: LocalizedStringResource {
            switch self {
            case .newestFirst: "Newest first"
            case .oldestFirst: "Oldest first"
            }
        }
    }

    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var selectedDay = Calendar.current.startOfDay(for: Date())
    @State private var displayMode: DisplayMode = .chart
    @State private var sortOrder: SortOrder = .newestFirst

    private var isToday: Bool { Calendar.current.isDateInToday(selectedDay) }

    var body: some View {
        VStack(spacing: 0) {
            dayNavigator
            Picker("Display", selection: $displayMode) {
                ForEach(DisplayMode.allCases) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.bottom, 8)
            Divider()
            content
        }
        .navigationTitle("Data")
        .toolbar {
            // The sort order only applies to the list.
            if displayMode == .list {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Menu {
                        Picker("Order", selection: $sortOrder) {
                            ForEach(SortOrder.allCases) { Text($0.title).tag($0) }
                        }
                    } label: {
                        Label("Order", systemImage: "arrow.up.arrow.down")
                    }
                }
            }
        }
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
                    Button("Today") { selectedDay = Calendar.current.startOfDay(for: Date()) }
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
            ProgressView("Loading…")
            Spacer()
        } else if viewModel.events.isEmpty {
            Spacer()
            Text("No data")
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
        // The server returns events oldest first.
        let rows = sortOrder == .newestFirst ? Array(viewModel.events.reversed()) : viewModel.events
        return List(rows) { event in
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

/// The four inverter power readings plotted on the chart, in legend order.
private enum PowerSeries: CaseIterable {
    case pv, inverterAc, consumption, grid, battery

    /// Localized name shown in the legend and the selection card.
    var name: String {
        switch self {
        case .pv: String(localized: "PV")
        case .inverterAc: String(localized: "Inverter AC")
        case .consumption: String(localized: "Consumption")
        case .grid: String(localized: "Grid")
        case .battery: String(localized: "Battery")
        }
    }

    var color: Color {
        switch self {
        case .pv: .orange
        case .inverterAc: .purple
        case .consumption: .blue
        case .grid: .red
        case .battery: .green
        }
    }
}

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
                PowerSample(id: event.id * PowerSeries.allCases.count + index, timestamp: event.timestamp, series: power.series.name, watts: power.watts)
            }
        }
    }

    /// The four power readings of an event, in legend order.
    private static func powers(of event: TGSEvent) -> [(series: PowerSeries, watts: Double)] {
        let inv = event.inverter
        return [(.pv, inv.pPv), (.inverterAc, inv.pInverterAc), (.consumption, inv.pLoad), (.grid, inv.pGrid), (.battery, inv.pAkku)]
    }

    private var selectedEvent: TGSEvent? {
        selectedTime.flatMap(nearestEvent(to:))
    }

    var body: some View {
        Chart {
            if #available(iOS 18, *) {
                LinePlot(
                    samples,
                    x: .value("Time", \.timestamp),
                    y: .value("Power (W)", \.watts),
                    series: .value("Series", \.series)
                )
                .foregroundStyle(by: .value("Series", \.series))
            } else {
                ForEach(samples) { sample in
                    LineMark(
                        x: .value("Time", sample.timestamp),
                        y: .value("Power (W)", sample.watts),
                        series: .value("Series", sample.series)
                    )
                    .foregroundStyle(by: .value("Series", sample.series))
                }
            }

            if let selected = selectedEvent {
                RuleMark(x: .value("Time", selected.timestamp))
                    .foregroundStyle(.secondary)
                    .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 4]))
                    .annotation(position: .top, spacing: 0,
                                overflowResolution: .init(x: .fit(to: .chart), y: .disabled)) {
                        selectionCard(for: selected)
                    }
            }
        }
        .chartXSelection(value: $selectedTime)
        .chartForegroundStyleScale(domain: PowerSeries.allCases.map(\.name), range: PowerSeries.allCases.map(\.color))
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
                    Circle().fill(power.series.color).frame(width: 6, height: 6)
                    Text(power.series.name)
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
