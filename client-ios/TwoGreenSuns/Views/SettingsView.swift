

import SwiftUI
import MapKit
import CoreLocation

struct SettingsView: View {
    @EnvironmentObject var viewModel: TGSInstallationEnvironment
    @State private var config: TGSConfiguration?
    @State private var backupConfig: TGSConfiguration?
    @State private var isLoading = true
    @State private var isEditing = false
    @State private var showErrorAlert = false
    @State private var addressQuery = ""
    @State private var isGeocoding = false
    @State private var geocodeError: String?

    var body: some View {
        content
            .navigationTitle("Configuration")
            .task {
                await viewModel.loadConfig()
                config = viewModel.config
                isLoading = false
            }
            .alert("Couldn't save configuration",
                   isPresented: $showErrorAlert,
                   presenting: viewModel.errorMessage) { _ in
                Button("OK", role: .cancel) { }
            } message: { message in
                Text(message)
            }
    }

    // MARK: - Edit lifecycle

    private func beginEditing() {
        backupConfig = config
        isEditing = true
        Task { await prefillAddressFromConfig() }
    }

    private func cancelEditing() {
        config = backupConfig
        backupConfig = nil
        isEditing = false
        addressQuery = ""
        geocodeError = nil
    }

    private func commitEditing() {
        guard let config else { return }
        viewModel.errorMessage = nil
        Task {
            await viewModel.saveConfig(config)
            if viewModel.errorMessage != nil {
                showErrorAlert = true
            } else {
                backupConfig = nil
                isEditing = false
                addressQuery = ""
                geocodeError = nil
            }
        }
    }

    // MARK: - Geocoding

    private func geocodeAddress() async {
        let query = addressQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return }
        isGeocoding = true
        geocodeError = nil
        defer { isGeocoding = false }
        do {
            guard let coordinate = try await CLGeocoder().geocodeAddressString(query).first?.location?.coordinate else {
                geocodeError = "No match for that address"
                return
            }
            config?.installationLatitude = coordinate.latitude
            config?.installationLongitude = coordinate.longitude
        } catch {
            geocodeError = "Couldn't look up that address"
        }
    }

    private func prefillAddressFromConfig() async {
        guard let config, config.installationLatitude != 0 || config.installationLongitude != 0 else { return }
        let location = CLLocation(latitude: config.installationLatitude, longitude: config.installationLongitude)
        if let placemark = try? await CLGeocoder().reverseGeocodeLocation(location).first {
            addressQuery = [placemark.name, placemark.locality, placemark.postalCode, placemark.country]
                .compactMap { $0 }
                .joined(separator: ", ")
        }
    }

    @ViewBuilder
    private var content: some View {
        if isLoading {
            ProgressView("Loading config…")
        } else if config != nil {
            Form {
                if isEditing { nameSection }
                locationSection
                costSection
                inverterSection
                teslaSection
                editSection
            }
        } else {
            Text("Couldn't load config")
                .foregroundStyle(.secondary)
        }
    }
    @ViewBuilder
    private var nameSection: some View {
        Section("Installation") {
            TextField("Display name", text: Binding(
                get: { config?.installationDisplayName ?? "" },
                set: { config?.installationDisplayName = $0 }
            ))
        }
    }

    @ViewBuilder
    private var editSection: some View {
        Section {
            if isEditing {
                Button {
                    commitEditing()
                } label: {
                    Text("Save").frame(maxWidth: .infinity, alignment: .center)
                }
                Button(role: .cancel) {
                    cancelEditing()
                } label: {
                    Text("Cancel").frame(maxWidth: .infinity, alignment: .center)
                }
                .foregroundStyle(.red)
            } else {
                Button {
                    beginEditing()
                } label: {
                    Text("Edit").frame(maxWidth: .infinity, alignment: .center)
                }
            }
        }
    }

    @ViewBuilder
    private var locationSection: some View {
        Section("Location") {
            if isEditing {
                TextField("Street, city, country", text: $addressQuery)
                    .textContentType(.fullStreetAddress)
                    .submitLabel(.search)
                    .onSubmit { Task { await geocodeAddress() } }

                Button {
                    Task { await geocodeAddress() }
                } label: {
                    HStack {
                        Text("Find coordinates")
                        Spacer()
                        if isGeocoding { ProgressView() }
                    }
                }
                .disabled(addressQuery.trimmingCharacters(in: .whitespaces).isEmpty || isGeocoding)

                if let geocodeError {
                    Text(geocodeError)
                        .font(.footnote)
                        .foregroundStyle(.red)
                } else if let config {
                    LabeledContent("Latitude",
                                   value: config.installationLatitude.formatted(.number.precision(.fractionLength(5))))
                    LabeledContent("Longitude",
                                   value: config.installationLongitude.formatted(.number.precision(.fractionLength(5))))
                }
            } else {
                Map(initialPosition: .region(
                    MKCoordinateRegion(
                        center: CLLocationCoordinate2D(latitude: config?.installationLatitude ?? 0.0, longitude: config?.installationLongitude ?? 0.0),
                                span: MKCoordinateSpan(latitudeDelta: 0.002, longitudeDelta: 0.002)
                            )
                        )) {
                            Marker("Location", coordinate: CLLocationCoordinate2D(latitude: config?.installationLatitude ?? 0.0, longitude: config?.installationLongitude ?? 0.0))
                        }
                        .disabled(true) // makes it non-interactive (no pan/zoom/rotate)
                        .frame(maxWidth: .infinity, minHeight: 200, maxHeight: 200)
                        .listRowInsets(EdgeInsets())
            }
        }
    }
    
    @ViewBuilder
    private var costSection: some View {
        Section("Cost") {
            if let segments = config?.providerCosts, !segments.isEmpty {
                ForEach(segments.indices, id: \.self) { index in
                    costRow(at: index)
                }
                .onDelete(perform: deleteSegment)
                .deleteDisabled(!isEditing)
            } else {
                Text("No cost data")
                    .foregroundStyle(.secondary)
            }

            if isEditing {
                Button {
                    addSegment()
                } label: {
                    Label("Add segment", systemImage: "plus")
                }
            }
        }
    }

    @ViewBuilder
    private func costRow(at index: Int) -> some View {
        HStack {
            if isEditing {
                TextField("HH:mm", text: Binding(
                    get: { Self.displayTime(config?.providerCosts[index].start ?? "00:00:00") },
                    set: { newValue in
                        guard let seconds = Self.secondsFromTimeString(newValue) else { return }
                        config?.providerCosts[index].start = Self.timeStringFromSeconds(seconds)
                        recomputeEndTimes()
                    }
                ))
                .frame(width: 70)
                .keyboardType(.numbersAndPunctuation)
            } else {
                Text(Self.displayTime(config?.providerCosts[index].start ?? "00:00:00"))
                    .frame(width: 70, alignment: .leading)
            }

            Text("–")
                .foregroundStyle(.secondary)

            Text(Self.displayTime(config?.providerCosts[index].end ?? "00:00:00"))
                .foregroundStyle(.secondary)
                .frame(width: 50, alignment: .leading)

            Spacer()

            if isEditing {
                TextField("Value", value: Binding(
                    get: { config?.providerCosts[index].value ?? 0 },
                    set: { config?.providerCosts[index].value = $0 }
                ), format: .number.precision(.fractionLength(2)))
                .keyboardType(.decimalPad)
                .multilineTextAlignment(.trailing)
                .frame(width: 60)

                Text("€/kWh")
                    .foregroundStyle(.secondary)
            } else {
                Text("\(config?.providerCosts[index].value.formatted(.number.precision(.fractionLength(2))) ?? "") €/kWh")
                    .foregroundStyle(.secondary)
                    .frame(width: 90, alignment: .trailing)
            }
        }
    }

    // MARK: - Cost segment helpers

    private func deleteSegment(at offsets: IndexSet) {
        config?.providerCosts.remove(atOffsets: offsets)
        recomputeEndTimes()
    }

    private func addSegment() {
        let lastEndSeconds = config?.providerCosts.last.flatMap { Self.secondsFromTimeString($0.end) } ?? 0
        let newSegment = TimeSegment(
            start: Self.timeStringFromSeconds(lastEndSeconds % (24 * 3600)),
            end: Self.timeStringFromSeconds(0),
            value: 0
        )
        config?.providerCosts.append(newSegment)
        recomputeEndTimes()
    }

    /// Re-derives `end` for every segment from the next segment's `start`,
    /// same rule as the server's day decoding: last one closes the day, and any
    /// non-increasing start implies a midnight rollover.
    private func recomputeEndTimes() {
        guard var segments = config?.providerCosts, !segments.isEmpty else { return }

        for index in segments.indices {
            let startSeconds = Self.secondsFromTimeString(segments[index].start) ?? 0
            let nextStartSeconds = index + 1 < segments.count
                ? (Self.secondsFromTimeString(segments[index + 1].start) ?? 0)
                : 24 * 3600

            let endSeconds = nextStartSeconds > startSeconds
                ? nextStartSeconds
                : nextStartSeconds + 24 * 3600

            segments[index].end = Self.timeStringFromSeconds(endSeconds)
        }

        config?.providerCosts = segments
    }

    private static func displayTime(_ text: String) -> String {
        guard let seconds = secondsFromTimeString(text) else { return text }
        let normalized = seconds % (24 * 3600)
        return String(format: "%02d:%02d", normalized / 3600, (normalized % 3600) / 60)
    }

    /// Accepts "HH:mm" or "HH:mm:ss".
    private static func secondsFromTimeString(_ text: String) -> Int? {
        let parts = text.split(separator: ":").compactMap { Int($0) }
        guard parts.count >= 2, parts[0] >= 0, parts[0] < 24, parts[1] >= 0, parts[1] < 60 else {
            return nil
        }
        let seconds = parts.count > 2 ? parts[2] : 0
        return parts[0] * 3600 + parts[1] * 60 + seconds
    }

    private static func timeStringFromSeconds(_ seconds: Int) -> String {
        let normalized = ((seconds % (24 * 3600)) + 24 * 3600) % (24 * 3600)
        let h = normalized / 3600
        let m = (normalized % 3600) / 60
        let s = normalized % 60
        return String(format: "%02d:%02d:%02d", h, m, s)
    }

    @ViewBuilder
    private var inverterSection: some View {
        Section("Inverter") {
            LabeledContent("Poll interval (min)") {
                if isEditing {
                    TextField("10", value: Binding(
                        get: { config?.pollingIntervalMinutes ?? 0 },
                        set: { config?.pollingIntervalMinutes = $0 }
                    ), format: .number)
                    .keyboardType(.numberPad)
                    .multilineTextAlignment(.trailing)
                } else {
                    Text(config?.pollingIntervalMinutes.formatted() ?? "")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private var teslaSection: some View {
        Section("Tesla") {
            LabeledContent("VIN") {
                if isEditing {
                    TextField("optional", text: Binding(
                        get: { config?.teslaVin ?? "" },
                        set: { config?.teslaVin = $0.isEmpty ? nil : $0 }
                    ))
                    .multilineTextAlignment(.trailing)
                } else {
                    Text(config?.teslaVin ?? "optional")
                        .foregroundStyle(.secondary)
                }
            }
            LabeledContent("Max amps") {
                if isEditing {
                    TextField("16", value: Binding(
                        get: { config?.teslaMaxChargingAmps ?? 0 },
                        set: { config?.teslaMaxChargingAmps = $0 }
                    ), format: .number)
                    .keyboardType(.numberPad)
                    .multilineTextAlignment(.trailing)
                } else {
                    Text(config?.teslaMaxChargingAmps.formatted() ?? "")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

}
