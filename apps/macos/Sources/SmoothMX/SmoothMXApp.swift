import SwiftUI

@main
struct SmoothMXApp: App {
    @NSApplicationDelegateAdaptor(SmoothMXDelegate.self) private var delegate
    @StateObject private var discovery = DiscoveryModel()
    var body: some Scene {
        WindowGroup("SmoothMX") {
            ContentView(discovery: discovery)
        }
        .defaultSize(width: 1040, height: 780)
        .commands { CommandGroup(after: .appInfo) { Button("Pause customization") { SettingsModel.shared.pause() } } }
    }
}

struct ContentView: View {
    @ObservedObject var discovery: DiscoveryModel
    @ObservedObject private var settings = SettingsModel.shared
    @State private var selected: String? = "overview"
    var body: some View {
        NavigationSplitView {
            List(selection: $selected) {
                Label("Overview", systemImage: "computermouse").tag("overview")
                Section("Mouse settings") {
                    ForEach(discovery.devices.filter { $0.kind == "mouse" }) { device in
                        HStack {
                            Label(device.name, systemImage: "slider.horizontal.3")
                            Spacer()
                            if let battery = battery(for: device), let level = battery.levelPercent {
                                BatteryStatusView(level: level, charging: battery.charging, compact: true)
                                    .help(battery.charging == true ? "Charging · \(level)%" : "Battery \(level)%")
                            }
                        }
                        .tag(device.id)
                    }
                }
            }
            .navigationTitle("SmoothMX")
        } detail: {
            if let device = discovery.devices.first(where: { $0.id == selected }) {
                SettingsView(device: device, model: settings)
            } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Your devices").font(.largeTitle.bold())
                        Text("Connected Logitech hardware, detected directly by macOS.")
                            .foregroundStyle(.secondary)
                    }
                    if !settings.inputMonitoring {
                        HStack(spacing: 10) {
                            Label("Input Monitoring is required to read Logitech battery data.", systemImage: "lock.trianglebadge.exclamationmark")
                                .foregroundStyle(.orange)
                            Spacer()
                            Button("Request Access") { settings.requestInputMonitoring() }
                                .buttonStyle(.borderedProminent)
                            Button("Open Settings…") { settings.openInputMonitoringSettings() }
                        }
                        .padding(14)
                        .background(.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                    }
                    if let error = discovery.error {
                        Label("Discovery unavailable", systemImage: "exclamationmark.triangle")
                            .font(.headline)
                        Text(error).foregroundStyle(.secondary)
                    } else if discovery.scanning {
                        ProgressView("Looking for Logitech devices…")
                    } else if discovery.devices.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Label("No Logitech devices detected", systemImage: "computermouse")
                                .font(.headline)
                            Text("Turn on your mouse and connect it using Bluetooth or its USB receiver. This view updates automatically.")
                                .foregroundStyle(.secondary)
                        }
                    } else {
                        ForEach(discovery.devices) { device in
                            DeviceCard(device: device, battery: battery(for: device))
                        }
                    }
                    Divider()
                    Label(settings.active ? "Customization active" : "Mouse detected · Settings available in the sidebar", systemImage: "checkmark.shield")
                        .font(.callout).foregroundStyle(.secondary)
                    Text(settings.active ? "Close the window to keep running; Pause customization to restore the mouse." : "Choose your mouse in the sidebar to configure it.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                .padding(28)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            }
        }
        .toolbar {
            ToolbarItem(placement: .navigation) {
                if selected != nil && selected != "overview" {
                    Button {
                        selected = "overview"
                    } label: {
                        Label("Overview", systemImage: "chevron.left")
                    }
                    .help("Return to the connected device list")
                }
            }
        }
        .frame(minWidth: 900, minHeight: 650)
        .onAppear { settings.devicesChanged(discovery.devices) }
        .onChange(of: discovery.devices.map(\.id)) { _ in settings.devicesChanged(discovery.devices) }
    }

    private func battery(for device: MouseDevice) -> BatteryInfo? {
        settings.deviceID == device.id ? (settings.batteryInfo ?? device.batteryInfo) : device.batteryInfo
    }
}

@MainActor
final class SmoothMXDelegate: NSObject, NSApplicationDelegate {
    func applicationWillTerminate(_ notification: Notification) {
        SettingsModel.shared.shutdownSynchronously()
    }
}

private struct DeviceCard: View {
    let device: MouseDevice
    let battery: BatteryInfo?
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 16) {
                Image(systemName: device.kind == "receiver" ? "antenna.radiowaves.left.and.right" : "computermouse.fill")
                    .font(.system(size: 32)).foregroundStyle(.tint)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 5) {
                    Text(device.name).font(.title2.bold()).textSelection(.enabled)
                    Label("Connected · \(device.transport)", systemImage: "checkmark.circle.fill")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                if let battery, let level = battery.levelPercent {
                    BatteryStatusView(level: level, charging: battery.charging)
                }
            }
            Text(device.detail).foregroundStyle(.secondary)
            DisclosureGroup("Connection details") {
                Grid(alignment: .leading, horizontalSpacing: 24, verticalSpacing: 8) {
                    GridRow { Text("Vendor ID"); Text(String(format: "0x%04X", device.vendor_id)) }
                    GridRow { Text("Product ID"); Text(String(format: "0x%04X", device.product_id)) }
                    GridRow { Text("HID interfaces"); Text("\(device.interface_count)") }
                    GridRow { Text("Classification"); Text(device.kind.capitalized) }
                }
                .font(.callout).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.top, 10)
            }
        }
        .padding(22)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.background, in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(.quaternary))
    }
}
