import AppKit
@preconcurrency import ApplicationServices
import IOKit.hid

@MainActor
final class SettingsModel: ObservableObject {
    static let shared = SettingsModel()
    @Published var profiles = [ApplicationProfile.global]
    @Published var selectedProfile = "*"
    @Published private(set) var capabilities: HardwareSnapshot?
    @Published private(set) var batteryInfo: BatteryInfo?
    @Published private(set) var deviceID: String?
    @Published private(set) var busy = false
    @Published private(set) var active = false
    @Published private(set) var message = "Read mouse capabilities to begin."
    @Published private(set) var accessibility = AXIsProcessTrusted()
    @Published private(set) var activeProfile = "All applications"
    private var session: HardwareSession?
    private var generation = UUID()
    private var saved = Configuration()
    private var pendingSave: Configuration?
    private var desiredActive = false
    private var foreground = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
    private var observer: NSObjectProtocol?
    private var currentProfileID: String?
    private var loadedSettings = false
    private var sessionFailed = false
    private var registryID: UInt64?
    private static var file: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("SmoothMX/settings.json")
    }
    private init() {
        do {
            if FileManager.default.fileExists(atPath: Self.file.path) {
                let data = try Data(contentsOf: Self.file)
                guard data.count <= 1_000_000 else { throw ConfigurationError.invalid }
                let config = try JSONDecoder().decode(Configuration.self, from: data)
                try config.validate(); saved = config; profiles = config.profiles; loadedSettings = true
            }
        } catch { message = "Saved settings could not be loaded. The original file has been left untouched." }
        observer = NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] note in
            let bundle = (note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication)?.bundleIdentifier ?? ""
            Task { @MainActor [weak self] in self?.applicationChanged(bundle) }
        }
    }
    var profileIndex: Int { profiles.firstIndex(where: { $0.id == selectedProfile }) ?? 0 }
    func connect(_ device: MouseDevice) {
        guard !busy else { return }
        shutdown(); deviceID = device.id; capabilities = nil; busy = true
        message = "Reading supported controls and current settings…"
        let id = UInt64(device.id.replacingOccurrences(of: "hid-", with: ""), radix: 16)
        guard let id, device.transport.contains("Bluetooth") else {
            registryID = nil; busy = false; message = "Hardware settings currently support direct Bluetooth connections."; return
        }
        registryID = id; generation = UUID(); let token = generation
        do {
            session = try HardwareSession(registryID: id) { [weak self] data in
                Task { @MainActor [weak self] in
                    guard let self, self.generation == token else { return }
                    self.receive(data)
                }
            }
        } catch { busy = false; message = "Could not start the hardware session." }
    }
    private func receive(_ data: Data) {
        do {
            let result = try JSONDecoder().decode(HardwareSnapshot.self, from: data)
            capabilities = result; busy = false; active = result.applied
            if let error = result.error {
                message = error; desiredActive = false; pendingSave = nil; sessionFailed = !result.ready
                return
            }
            sessionFailed = false
            if result.ready, !loadedSettings {
                let i = profiles.firstIndex(where: { $0.id == "*" }) ?? 0
                profiles[i].settings.dpi = result.dpi ?? 0
                profiles[i].settings.wheelMode = result.wheel_mode == 1 ? 1 : 2
                profiles[i].settings.smartShift = (result.threshold ?? 255) < 255 && result.wheel_mode != 1
                profiles[i].settings.threshold = Double(min(254, max(1, result.threshold ?? 16)))
                loadedSettings = true
            }
            if result.applied, let pending = pendingSave {
                do {
                    try FileManager.default.createDirectory(at: Self.file.deletingLastPathComponent(), withIntermediateDirectories: true)
                    let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
                    try encoder.encode(pending).write(to: Self.file, options: .atomic)
                    saved = pending; pendingSave = nil
                } catch {
                    session?.restore(); desiredActive = false; pendingSave = nil
                    message = "Could not save settings; restoring the mouse. \(error.localizedDescription)"
                    return
                }
            }
            message = result.applied ? "Customization active · \(activeProfile)" : "Ready. Original settings are preserved until you apply changes."
            if desiredActive { applyForegroundIfNeeded() }
            // Fetch battery info asynchronously when capabilities are ready.
            if result.ready || (result.error != nil) {
                Task { await self.fetchBattery() }
            }
        } catch { busy = false; message = "Invalid capability response: \(error.localizedDescription)" }
    }
    private func fetchBattery() async {
        guard let registryID, !busy else { return }
        batteryInfo = HardwareSession.battery(registryID: registryID)
    }
    func applySelected() {
        guard let session, capabilities?.ready == true, !busy else { return }
        accessibility = AXIsProcessTrusted()
        let profile = profiles[profileIndex]
        guard !profile.settings.needsAccessibility || accessibility else {
            message = "Enable Accessibility for SmoothMX before applying scrolling or button actions."; return
        }
        do {
            let config = Configuration(profiles: profiles); try config.validate()
            pendingSave = config; desiredActive = true; busy = true
            activeProfile = profile.name; currentProfileID = profile.id; foreground = profile.id
            try session.apply(profile.settings)
        } catch { busy = false; pendingSave = nil; message = "Settings validation failed; no changes were queued." }
    }
    func pause() {
        desiredActive = false; pendingSave = nil; currentProfileID = nil
        if !sessionFailed { busy = session != nil; session?.restore() }
        active = false
    }
    func shutdown() {
        desiredActive = false; generation = UUID(); session = nil; pendingSave = nil
        active = false; busy = false; currentProfileID = nil
    }
    func devicesChanged(_ devices: [MouseDevice]) {
        if let id = deviceID, !devices.contains(where: { $0.id == id }) {
            shutdown(); capabilities = nil; deviceID = nil
            message = "Mouse disconnected. Reconnect, read capabilities, then apply your saved profile."
        }
    }
    func refreshPermissions() { accessibility = AXIsProcessTrusted() }
    func requestInputMonitoring() {
        _ = IOHIDRequestAccess(kIOHIDRequestTypeListenEvent)
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")!)
    }
    func requestAccessibility() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }
    func addApplication() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = false; panel.canChooseFiles = true
        panel.allowedContentTypes = [.applicationBundle]; panel.directoryURL = URL(fileURLWithPath: "/Applications")
        guard panel.runModal() == .OK, let url = panel.url, let bundle = Bundle(url: url), let id = bundle.bundleIdentifier else { return }
        if !profiles.contains(where: { $0.id == id }) {
            let base = profiles.first(where: { $0.id == "*" })!.settings
            profiles.append(ApplicationProfile(id: id, name: url.deletingPathExtension().lastPathComponent, settings: base))
        }
        selectedProfile = id
    }
    func removeSelectedProfile() {
        guard selectedProfile != "*" else { return }
        profiles.removeAll { $0.id == selectedProfile }; selectedProfile = "*"
    }
    private func applicationChanged(_ bundle: String) {
        refreshPermissions()
        if active && !accessibility { pause(); message = "Accessibility permission is no longer available. Customization paused."; return }
        guard !bundle.isEmpty, bundle != Bundle.main.bundleIdentifier else { return }
        foreground = bundle; applyForegroundIfNeeded()
    }
    private func applyForegroundIfNeeded() {
        guard desiredActive, !busy, let session else { return }
        let profile = saved.profiles.first(where: { $0.id == foreground }) ?? saved.profiles.first(where: { $0.id == "*" })!
        guard profile.id != currentProfileID else { return }
        guard !profile.settings.needsAccessibility || AXIsProcessTrusted() else { pause(); return }
        do {
            busy = true; activeProfile = profile.name; currentProfileID = profile.id
            try session.apply(profile.settings)
        } catch { busy = false; message = "Could not activate the application profile." }
    }
}
