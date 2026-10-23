import SwiftUI
import AppKit

struct SettingsView: View {
    let device: MouseDevice
    @ObservedObject var model: SettingsModel
    @State private var tab = 0
    private var settings: Binding<MouseSettings> { $model.profiles[model.profileIndex].settings }
    private var ready: Bool { model.deviceID == device.id && model.capabilities?.ready == true }
    private var battery: BatteryInfo? {
        model.deviceID == device.id ? (model.batteryInfo ?? device.batteryInfo) : device.batteryInfo
    }
    var body: some View {
        GeometryReader { geometry in
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(device.name).font(.title.bold())
                        Text(device.transport).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(ready ? "Reconnect settings" : "Read capabilities") { model.connect(device) }
                        .disabled(model.busy || model.active)
                }
                HStack {
                    Picker("Editing profile", selection: $model.selectedProfile) {
                        ForEach(model.profiles) { Text($0.name).tag($0.id) }
                    }.frame(maxWidth: 380)
                    Spacer()
                    if model.active { Label("Active", systemImage: "checkmark.circle.fill").foregroundStyle(.green) }
                    if model.busy { ProgressView().controlSize(.small) }
                }
                HStack(spacing: 24) {
                    if let battery, let level = battery.levelPercent {
                        BatteryStatusView(level: level, charging: battery.charging)
                    } else if ready {
                        Label("Battery unavailable", systemImage: "battery.0")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                TabView(selection: $tab) {
                    pointAndScroll.tabItem { Text("Point & Scroll") }.tag(0)
                    buttons.tabItem { Text("Buttons & Gestures") }.tag(1)
                    profiles.tabItem { Text("Application Profiles") }.tag(2)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .layoutPriority(1)
                .clipped()
                footer
            }
            .padding(20)
            .frame(
                width: geometry.size.width,
                height: geometry.size.height,
                alignment: .topLeading
            )
        }
        .onAppear { model.refreshPermissions() }
    }
    private var footer: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(model.message).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                HStack(spacing: 8) { permissionControls }
                    .fixedSize(horizontal: true, vertical: false)
                Spacer()
                Button("Pause & Restore") { model.pause() }.disabled(!model.active || model.busy)
                Button("Save & Apply") { model.applySelected() }.buttonStyle(.borderedProminent)
                    .disabled(!ready || model.busy || !model.inputMonitoring)
            }
        }
    }
    @ViewBuilder private var permissionControls: some View {
        if model.inputMonitoring {
            Menu("Permissions") {
                Button("Open Input Monitoring settings…") { model.openInputMonitoringSettings() }
                if model.accessibility {
                    Button("Open Accessibility settings…") { model.openAccessibilitySettings() }
                } else {
                    Button("Request Accessibility") { model.requestAccessibility() }
                    Button("Open Accessibility settings…") { model.openAccessibilitySettings() }
                }
                Button("Recheck permissions") { model.refreshPermissions() }
            }
            .frame(width: 105)
        } else {
            Button("Request Input Monitoring") { model.requestInputMonitoring() }
            Button("Open Settings…") { model.openInputMonitoringSettings() }
            Menu("More…") {
                if model.accessibility {
                    Button("Open Accessibility settings…") { model.openAccessibilitySettings() }
                } else {
                    Button("Request Accessibility") { model.requestAccessibility() }
                    Button("Open Accessibility settings…") { model.openAccessibilitySettings() }
                }
                Button("Recheck permissions") { model.refreshPermissions() }
            }
            .frame(width: 82)
        }
        Label(
            model.inputMonitoring ? "Input Monitoring enabled" : "Input Monitoring required",
            systemImage: model.inputMonitoring ? "checkmark.circle.fill" : "exclamationmark.triangle.fill"
        )
        .font(.caption)
        .foregroundStyle(model.inputMonitoring ? Color.secondary : Color.orange)
        if !model.accessibility {
            Text("Actions need Accessibility").font(.caption).foregroundStyle(.secondary)
        }
    }
    private var pointAndScroll: some View {
        Form {
            Section("Pointer") {
                Picker("Pointer speed · sensor DPI", selection: settings.dpi) {
                    Text("Keep current").tag(UInt32(0))
                    ForEach(model.capabilities?.dpi_values ?? [], id: \.self) { value in Text("\(value) DPI").tag(value) }
                }.disabled(!ready || (model.capabilities?.dpi_values ?? []).isEmpty)
                Text("Changes mouse sensor sensitivity. macOS acceleration stays as configured in System Settings.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Main wheel") {
                Toggle("SmartShift", isOn: settings.smartShift)
                    .disabled(!ready || model.capabilities?.smart_shift != true)
                if settings.wrappedValue.smartShift {
                    valueSlider("SmartShift threshold", value: settings.threshold, range: 1...254, suffix: "")
                    Text("Lower thresholds switch to free-spin sooner.").font(.caption).foregroundStyle(.secondary)
                } else {
                    Picker("Wheel mode", selection: settings.wheelMode) {
                        Text("Ratchet").tag(UInt32(2)); Text("Free-spin").tag(UInt32(1))
                    }
                }
                Toggle("Customize vertical scrolling", isOn: settings.vertical)
                    .disabled(!ready || model.capabilities?.vertical != true)
                Group {
                    valueSlider("Scrolling speed", value: settings.verticalSpeed, range: 10...500, suffix: "%")
                    Picker("Direction", selection: settings.verticalInvert) {
                        Text("Standard").tag(false); Text("Natural").tag(true)
                    }
                    Toggle("Smooth scrolling", isOn: settings.smooth)
                }.disabled(!settings.wrappedValue.vertical)
            }
            Section("Thumb wheel") {
                Toggle("Customize thumb wheel", isOn: settings.horizontal)
                    .disabled(!ready || model.capabilities?.horizontal != true)
                Group {
                    valueSlider("Thumb-wheel speed", value: settings.horizontalSpeed, range: 10...500, suffix: "%")
                    Toggle("Reverse thumb-wheel direction", isOn: settings.horizontalInvert)
                    Text("Leave both actions as “Do nothing” for horizontal scrolling. Assign actions to turn rotation into shortcuts.")
                        .font(.caption).foregroundStyle(.secondary)
                    ActionPicker(title: "Positive rotation", action: settings.horizontalActions[0])
                    ActionPicker(title: "Negative rotation", action: settings.horizontalActions[1])
                }.disabled(!settings.wrappedValue.horizontal)
            }
        }.formStyle(.grouped).disabled(model.busy || !ready)
    }
    private var buttons: some View {
        Form {
            Section {
                Text("Only buttons reported as reprogrammable by your mouse can be changed. Native behavior remains until Save & Apply.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            ForEach(0..<5, id: \.self) { index in
                let ids: [UInt16] = [0x53, 0x56, 0x52, 0xc4, 0xc3]
                let names = ["Back", "Forward", "Middle click", "Wheel-mode button", "Thumb gesture button"]
                let support = model.capabilities?.buttons?.first { $0.id == ids[index] }
                Section(names[index]) {
                    Toggle("Customize", isOn: settings.buttons[index].enabled)
                        .disabled(support?.divertable != true)
                    if settings.wrappedValue.buttons[index].enabled {
                        Toggle("Hold and move for gestures", isOn: settings.buttons[index].gesture)
                            .disabled(support?.gesture != true)
                        ActionPicker(title: "Press", action: settings.buttons[index].actions[0])
                        if settings.wrappedValue.buttons[index].gesture {
                            ForEach(1..<5, id: \.self) { direction in
                                ActionPicker(title: ["Press", "Up", "Down", "Left", "Right"][direction], action: settings.buttons[index].actions[direction])
                            }
                        }
                    }
                    if ready && support?.divertable != true { Text("Not reported as reprogrammable.").font(.caption).foregroundStyle(.secondary) }
                }
            }
            Section("Gesture recognition") {
                valueSlider("Movement threshold", value: settings.gestureThreshold, range: 10...500, suffix: " counts")
                Text("One action runs on release. A short movement counts as a press. Desktop actions use your macOS keyboard shortcuts.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped).disabled(model.busy || !ready)
    }
    private var profiles: some View {
        Form {
            Section("Application profiles") {
                Text("Add an application to give it its own settings. Saved profiles activate when that application becomes frontmost. Other applications use All applications.")
                HStack {
                    Button("Add application…") { model.addApplication() }
                    Button("Remove selected profile") { model.removeSelectedProfile() }.disabled(model.selectedProfile == "*")
                }
                ForEach(model.profiles) { profile in
                    HStack {
                        Text(profile.name); Spacer()
                        if profile.id == model.selectedProfile { Image(systemName: "checkmark") }
                    }.contentShape(Rectangle()).onTapGesture { model.selectedProfile = profile.id }
                }
                Text("A new profile starts as a copy of the global profile; later edits are independent. Save & Apply saves all profile edits.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("While SmoothMX is running") {
                Text("Closing the window keeps customization active. Pause & Restore or Quit restores the settings captured when you connected. After a disconnect, SmoothMX reconnects only when exactly one mouse matches and resumes the saved foreground profile if customization was active.")
                Text("This development build does not have crash recovery. If it exits unexpectedly while controls are customized, power-cycle the mouse to restore normal reporting.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.formStyle(.grouped).disabled(model.busy)
    }
    private func valueSlider(_ title: String, value: Binding<Double>, range: ClosedRange<Double>, suffix: String) -> some View {
        HStack {
            Text(title).frame(width: 170, alignment: .leading)
            Slider(value: value, in: range, step: 1)
            Text("\(Int(value.wrappedValue))\(suffix)").monospacedDigit().frame(width: 85, alignment: .trailing)
        }
    }
}

struct BatteryStatusView: View {
    let level: Int
    let charging: Bool?
    var compact = false

    private var boundedLevel: Int { min(100, max(0, level)) }
    private var icon: String {
        if charging == true { return "batterycharging.full" }
        return switch boundedLevel {
        case 0..<13: "battery.0"
        case 13..<38: "battery.25"
        case 38..<63: "battery.50"
        case 63..<88: "battery.75"
        default: "battery.100"
        }
    }
    private var color: Color {
        if charging == true { return .green }
        if boundedLevel <= 15 { return .red }
        if boundedLevel <= 30 { return .orange }
        return .secondary
    }
    private var title: String {
        charging == true ? "Charging · \(boundedLevel)%" : "\(boundedLevel)%"
    }

    var body: some View {
        if compact {
            Label(title, systemImage: icon)
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(color)
                .accessibilityLabel("Mouse battery \(title)")
        } else {
            VStack(alignment: .leading, spacing: 5) {
                Label(title, systemImage: icon)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(color)
                ProgressView(value: Double(boundedLevel), total: 100)
                    .progressViewStyle(.linear)
                    .tint(color)
                    .frame(width: 170)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Mouse battery \(title)")
        }
    }
}

private struct ActionPicker: View {
    let title: String
    @Binding var action: MouseAction
    @State private var recording = false
    private var selection: Binding<Int> {
        Binding(get: { MouseAction.presets.firstIndex(where: { $0.1 == action }) ?? -1 }, set: { index in
            if index >= 0 { action = MouseAction.presets[index].1; recording = false } else { recording = true }
        })
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Picker(title, selection: selection) {
                ForEach(MouseAction.presets.indices, id: \.self) { i in Text(MouseAction.presets[i].0).tag(i) }
                Text("Custom keyboard shortcut…").tag(-1)
            }
            if recording || selection.wrappedValue == -1 {
                ShortcutRecorder(action: $action).frame(height: 28)
            }
        }
    }
}
private struct ShortcutRecorder: NSViewRepresentable {
    @Binding var action: MouseAction
    func makeNSView(context: Context) -> RecorderView { let view = RecorderView(); view.onRecord = { action = $0 }; return view }
    func updateNSView(_ view: RecorderView, context: Context) {
        view.onRecord = { action = $0 }
        view.label = action.kind == 1 ? "Click to record · key \(action.code), modifiers \(action.modifiers)" : "Click here, then press a shortcut"
        view.needsDisplay = true
    }
    final class RecorderView: NSView {
        var onRecord: ((MouseAction) -> Void)?
        var label = "Click here, then press a shortcut"
        override var acceptsFirstResponder: Bool { true }
        override func mouseDown(with event: NSEvent) { window?.makeFirstResponder(self); label = "Press your shortcut (Escape cancels)"; needsDisplay = true }
        override func keyDown(with event: NSEvent) {
            if event.keyCode == 53 { window?.makeFirstResponder(nil); return }
            var flags: UInt32 = 0
            if event.modifierFlags.contains(.command) { flags |= 1 }
            if event.modifierFlags.contains(.shift) { flags |= 2 }
            if event.modifierFlags.contains(.option) { flags |= 4 }
            if event.modifierFlags.contains(.control) { flags |= 8 }
            onRecord?(MouseAction(kind: 1, code: UInt32(event.keyCode), modifiers: flags))
            window?.makeFirstResponder(nil)
        }
        override func performKeyEquivalent(with event: NSEvent) -> Bool {
            guard window?.firstResponder === self else { return false }; keyDown(with: event); return true
        }
        override func draw(_ dirtyRect: NSRect) {
            NSColor.controlBackgroundColor.setFill(); NSBezierPath(roundedRect: bounds, xRadius: 5, yRadius: 5).fill()
            (label as NSString).draw(at: NSPoint(x: 8, y: 5), withAttributes: [.font: NSFont.systemFont(ofSize: 12), .foregroundColor: NSColor.secondaryLabelColor])
        }
    }
}
