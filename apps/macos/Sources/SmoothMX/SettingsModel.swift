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
    @Published private(set) var inputMonitoring = SettingsModel.hasInputMonitoringAccess
    @Published private(set) var activeProfile = "All applications"

    private struct DeviceIdentity: Equatable {
        let name: String
        let vendorID: UInt32
        let productID: UInt32
        let transport: String
        let kind: String

        init(_ device: MouseDevice) {
            name = device.name
            vendorID = device.vendor_id
            productID = device.product_id
            transport = device.transport
            kind = device.kind
        }
    }

    private var session: HardwareSession?
    // Retaining an asynchronously closing session lets app termination wait for
    // it too. This also prevents a replacement session from racing restoration.
    private var closingSession: HardwareSession?
    private var generation = UUID()
    private var saved = Configuration()
    private var pendingSave: Configuration?
    private var desiredActive = false
    private var foreground = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
    private var observer: NSObjectProtocol?
    private var currentProfileID: String?
    private var loadedSettings = false
    private var sessionFailed = false
    private var reconnectTarget: DeviceIdentity?
    private var reconnectAttemptedID: String?
    private var resumeAfterReconnect = false
    private var knownDevices: [MouseDevice] = []
    private var restoreCompletionMessage: String?
    private var terminating = false
    private var inputMonitoringWatch: Task<Void, Never>?

    private static var hasInputMonitoringAccess: Bool {
        IOHIDCheckAccess(kIOHIDRequestTypeListenEvent) == kIOHIDAccessTypeGranted
    }
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
                try config.validate()
                saved = config
                profiles = config.profiles
                loadedSettings = true
            }
        } catch {
            message = "Saved settings could not be loaded. The original file has been left untouched."
        }
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            let bundle = (note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication)?.bundleIdentifier ?? ""
            Task { @MainActor [weak self] in self?.applicationChanged(bundle) }
        }
    }

    var profileIndex: Int { profiles.firstIndex(where: { $0.id == selectedProfile }) ?? 0 }

    func connect(_ device: MouseDevice) {
        guard !busy, !terminating else { return }
        updatePermissionState()
        reconnectTarget = nil
        reconnectAttemptedID = nil
        resumeAfterReconnect = false

        guard directBluetoothID(for: device) != nil else {
            message = "Hardware settings currently support direct Bluetooth connections."
            return
        }
        reconnectTarget = DeviceIdentity(device)
        if !inputMonitoring {
            requestInputMonitoring()
            guard inputMonitoring else { return }
        }
        beginConnection(to: device, automatically: false)
    }

    private func beginConnection(to device: MouseDevice, automatically: Bool) {
        guard let registryID = directBluetoothID(for: device) else { return }
        generation = UUID()
        let token = generation
        let previous = session ?? closingSession
        session = nil
        deviceID = device.id
        capabilities = nil
        batteryInfo = nil
        restoreCompletionMessage = nil
        active = false
        desiredActive = false
        pendingSave = nil
        currentProfileID = nil
        sessionFailed = false
        busy = true
        message = automatically
            ? "Reading mouse battery, supported controls, and current settings…"
            : "Reading supported controls and current settings…"

        guard let previous else {
            openSession(registryID: registryID, token: token, attemptedID: device.id)
            return
        }

        closingSession = previous
        Task { [weak self] in
            await previous.close()
            guard let self else { return }
            if self.closingSession === previous { self.closingSession = nil }
            guard self.generation == token else { return }
            self.openSession(registryID: registryID, token: token, attemptedID: device.id)
        }
    }

    private func openSession(registryID: UInt64, token: UUID, attemptedID: String) {
        guard generation == token else { return }
        do {
            session = try HardwareSession(registryID: registryID) { [weak self] data in
                Task { @MainActor [weak self] in
                    guard let self, self.generation == token else { return }
                    self.receive(data)
                }
            }
        } catch {
            busy = false
            deviceID = nil
            reconnectAttemptedID = attemptedID
            message = "Could not start the hardware session. Check Input Monitoring and close other mouse configuration tools."
        }
    }

    private func receive(_ data: Data) {
        do {
            let result = try JSONDecoder().decode(HardwareSnapshot.self, from: data)
            let shouldResume = resumeAfterReconnect || desiredActive || active
            capabilities = result
            batteryInfo = result.battery
            busy = false
            active = result.applied

            if let error = result.error {
                restoreCompletionMessage = nil
                if !result.ready && error.localizedCaseInsensitiveContains("disconnected") {
                    resumeAfterReconnect = shouldResume
                    desiredActive = false
                    pendingSave = nil
                    sessionFailed = true
                    reconnectAttemptedID = deviceID
                    deviceID = nil
                    capabilities = nil
                    batteryInfo = nil
                    closeCurrentSessionEventually()
                    message = "Mouse disconnected. SmoothMX will reconnect when exactly one matching mouse returns."
                } else {
                    message = error
                    desiredActive = false
                    pendingSave = nil
                    sessionFailed = !result.ready
                }
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
                    try FileManager.default.createDirectory(
                        at: Self.file.deletingLastPathComponent(),
                        withIntermediateDirectories: true
                    )
                    let encoder = JSONEncoder()
                    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
                    try encoder.encode(pending).write(to: Self.file, options: .atomic)
                    saved = pending
                    pendingSave = nil
                } catch {
                    desiredActive = false
                    pendingSave = nil
                    currentProfileID = nil
                    let failure = "Could not save settings; the mouse was restored. \(error.localizedDescription)"
                    restoreCompletionMessage = failure
                    guard let session else {
                        restoreCompletionMessage = nil
                        busy = false
                        message = "Could not save settings, and the hardware session is no longer available."
                        return
                    }
                    busy = true
                    do {
                        try session.restore()
                    } catch {
                        restoreCompletionMessage = nil
                        busy = false
                        message = "Could not save settings or queue restoration. Reconnect the mouse before continuing."
                        return
                    }
                    message = "Could not save settings; restoring the mouse…"
                    return
                }
            }

            if result.ready, resumeAfterReconnect {
                resumeAfterReconnect = false
                desiredActive = true
                currentProfileID = nil
            }
            if let restored = restoreCompletionMessage, !result.applied {
                restoreCompletionMessage = nil
                message = restored
            } else {
                message = result.applied
                    ? "Customization active · \(activeProfile)"
                    : "Ready. Original settings are preserved until you apply changes."
            }
            if desiredActive { applyForegroundIfNeeded() }
        } catch {
            busy = false
            message = "Invalid capability response: \(error.localizedDescription)"
        }
    }

    func applySelected() {
        updatePermissionState()
        guard inputMonitoring else {
            message = "Input Monitoring is required before SmoothMX can apply mouse settings."
            return
        }
        guard let session, capabilities?.ready == true, !busy else { return }
        let config = Configuration(profiles: profiles)
        do {
            try config.validate()
        } catch {
            message = "Settings validation failed; no changes were queued."
            return
        }

        if let frontmost = NSWorkspace.shared.frontmostApplication?.bundleIdentifier,
           frontmost != Bundle.main.bundleIdentifier {
            foreground = frontmost
        }
        let profile = config.profiles.first(where: { $0.id == foreground })
            ?? config.profiles.first(where: { $0.id == "*" })!
        guard !profile.settings.needsAccessibility || accessibility else {
            message = "Enable Accessibility for SmoothMX before applying scrolling or button actions."
            return
        }

        let previousActiveProfile = activeProfile
        let previousProfileID = currentProfileID
        let previousDesiredActive = desiredActive
        pendingSave = config
        desiredActive = true
        busy = true
        activeProfile = profile.name
        currentProfileID = profile.id
        do {
            try session.apply(profile.settings)
        } catch {
            busy = false
            pendingSave = nil
            activeProfile = previousActiveProfile
            currentProfileID = previousProfileID
            desiredActive = previousDesiredActive
            message = "Could not queue settings for the hardware session. Reconnect the mouse and try again."
        }
    }

    func pause() {
        desiredActive = false
        resumeAfterReconnect = false
        restoreCompletionMessage = nil
        pendingSave = nil
        currentProfileID = nil
        if !sessionFailed, let session {
            busy = true
            do {
                try session.restore()
            } catch {
                busy = false
                message = "Could not queue restoration. Reconnect the mouse and try again."
            }
        }
        active = false
    }

    // AppKit calls this during orderly termination. Waiting here guarantees
    // restoration and callback teardown before the process exits.
    func shutdownSynchronously() {
        terminating = true
        inputMonitoringWatch?.cancel()
        inputMonitoringWatch = nil
        desiredActive = false
        resumeAfterReconnect = false
        generation = UUID()
        pendingSave = nil
        restoreCompletionMessage = nil
        active = false
        busy = false
        currentProfileID = nil
        batteryInfo = nil

        let current = session
        let closing = closingSession
        session = nil
        closingSession = nil
        current?.closeSynchronously()
        if let closing, closing !== current { closing.closeSynchronously() }
    }

    func devicesChanged(_ devices: [MouseDevice]) {
        knownDevices = devices
        if let attempted = reconnectAttemptedID,
           !devices.contains(where: { $0.id == attempted }) {
            reconnectAttemptedID = nil
        }

        if let connectedID = deviceID,
           !devices.contains(where: { $0.id == connectedID }) {
            resumeAfterReconnect = resumeAfterReconnect || desiredActive || active
            // The old ID is already absent. Do not blacklist it: IOKit can
            // reuse the registry ID after a full remove/add cycle.
            reconnectAttemptedID = nil
            desiredActive = false
            pendingSave = nil
            restoreCompletionMessage = nil
            currentProfileID = nil
            deviceID = nil
            capabilities = nil
            batteryInfo = nil
            active = false
            busy = false
            closeCurrentSessionEventually()
            message = "Mouse disconnected. SmoothMX will reconnect when exactly one matching mouse returns."
        }
        attemptAutomaticConnection()
    }

    func refreshPermissions() {
        updatePermissionState()
        if inputMonitoring {
            finishInputMonitoringSetup(returnToApp: false)
        }
        attemptAutomaticConnection()
    }

    func requestInputMonitoring() {
        let granted = IOHIDRequestAccess(kIOHIDRequestTypeListenEvent)
        updatePermissionState()
        if granted || inputMonitoring {
            finishInputMonitoringSetup(returnToApp: false)
            return
        }

        message = "Input Monitoring was requested. Use Open Settings when you are ready to enable SmoothMX; this page will stay available."
        watchForInputMonitoringAccess()
    }

    func openInputMonitoringSettings() {
        if !inputMonitoring {
            watchForInputMonitoringAccess()
        }
        NSWorkspace.shared.open(
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")!
        )
    }

    private func watchForInputMonitoringAccess() {
        inputMonitoringWatch?.cancel()
        inputMonitoringWatch = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(400))
                guard let self, !Task.isCancelled else { return }
                guard Self.hasInputMonitoringAccess else { continue }
                self.finishInputMonitoringSetup(returnToApp: true)
                return
            }
        }
    }

    private func finishInputMonitoringSetup(returnToApp: Bool) {
        inputMonitoringWatch?.cancel()
        inputMonitoringWatch = nil
        inputMonitoring = true
        message = "Input Monitoring enabled. Reading the mouse battery and capabilities…"
        attemptAutomaticConnection()
        if returnToApp {
            NSApp.activate(ignoringOtherApps: true)
        }
    }

    func requestAccessibility() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
        updatePermissionState()
        message = accessibility
            ? "Accessibility enabled."
            : "Accessibility was requested. Use Open Settings if the macOS prompt does not appear."
    }

    func openAccessibilitySettings() {
        NSWorkspace.shared.open(
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        )
    }

    func addApplication() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowedContentTypes = [.applicationBundle]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        guard panel.runModal() == .OK,
              let url = panel.url,
              let bundle = Bundle(url: url),
              let id = bundle.bundleIdentifier else { return }
        if !profiles.contains(where: { $0.id == id }) {
            let base = profiles.first(where: { $0.id == "*" })!.settings
            profiles.append(ApplicationProfile(
                id: id,
                name: url.deletingPathExtension().lastPathComponent,
                settings: base
            ))
        }
        selectedProfile = id
    }

    func removeSelectedProfile() {
        guard selectedProfile != "*" else { return }
        profiles.removeAll { $0.id == selectedProfile }
        selectedProfile = "*"
    }

    private func updatePermissionState() {
        accessibility = AXIsProcessTrusted()
        inputMonitoring = Self.hasInputMonitoringAccess
    }

    private func applicationChanged(_ bundle: String) {
        updatePermissionState()
        if !bundle.isEmpty, bundle != Bundle.main.bundleIdentifier {
            foreground = bundle
        }
        // Returning from System Settings after granting Input Monitoring should
        // be enough to continue a pending, uniquely matched reconnect.
        attemptAutomaticConnection()
        if active && !inputMonitoring {
            pause()
            message = "Input Monitoring permission is no longer available. Customization paused."
            return
        }
        let appliedSettings = saved.profiles.first(where: { $0.id == currentProfileID })?.settings
        if active && appliedSettings?.needsAccessibility == true && !accessibility {
            pause()
            message = "Accessibility permission is no longer available. Customization paused."
            return
        }
        guard !bundle.isEmpty, bundle != Bundle.main.bundleIdentifier else { return }
        applyForegroundIfNeeded()
    }

    private func applyForegroundIfNeeded() {
        guard desiredActive, !busy, let session else { return }
        updatePermissionState()
        guard inputMonitoring else {
            pause()
            message = "Input Monitoring is required. Automatic profile activation has been paused."
            return
        }
        let profile = saved.profiles.first(where: { $0.id == foreground })
            ?? saved.profiles.first(where: { $0.id == "*" })!
        guard profile.id != currentProfileID else { return }
        guard !profile.settings.needsAccessibility || accessibility else {
            pause()
            message = "Accessibility is required. Automatic profile activation has been paused."
            return
        }
        let previousActiveProfile = activeProfile
        let previousProfileID = currentProfileID
        busy = true
        activeProfile = profile.name
        currentProfileID = profile.id
        do {
            try session.apply(profile.settings)
        } catch {
            busy = false
            activeProfile = previousActiveProfile
            currentProfileID = previousProfileID
            message = "Could not queue the application profile for the hardware session."
        }
    }

    private func directBluetoothID(for device: MouseDevice) -> UInt64? {
        guard device.kind == "mouse",
              device.transport.localizedCaseInsensitiveContains("Bluetooth"),
              device.id.hasPrefix("hid-") else { return nil }
        return UInt64(device.id.dropFirst(4), radix: 16)
    }

    private func closeCurrentSessionEventually() {
        generation = UUID()
        guard let current = session else { return }
        session = nil
        closingSession = current
        Task { [weak self] in
            await current.close()
            guard let self else { return }
            if self.closingSession === current { self.closingSession = nil }
            self.attemptAutomaticConnection()
        }
    }

    private func attemptAutomaticConnection() {
        guard !terminating, session == nil, closingSession == nil, !busy else { return }
        updatePermissionState()
        guard inputMonitoring else {
            if reconnectTarget != nil {
                message = "A matching mouse is available, but Input Monitoring must be enabled before reconnecting."
            }
            return
        }

        let supported = knownDevices.filter { directBluetoothID(for: $0) != nil }
        let matches: [MouseDevice]
        if let target = reconnectTarget {
            matches = supported.filter { DeviceIdentity($0) == target }
        } else {
            // A unique direct-Bluetooth mouse is safe to select automatically.
            // The session only reads until the user explicitly applies settings.
            matches = supported
        }
        guard matches.count == 1 else {
            if reconnectTarget != nil, matches.count > 1 {
                message = "Multiple identical matching mice are connected. Select the intended mouse manually; SmoothMX will not guess."
            }
            return
        }
        let device = matches[0]
        guard reconnectAttemptedID != device.id else { return }
        reconnectTarget = DeviceIdentity(device)
        reconnectAttemptedID = device.id
        beginConnection(to: device, automatically: true)
    }
}
