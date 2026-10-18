import AppKit
import ApplicationServices
import CSmoothMX

struct HardwareButton: Decodable {
    let id: UInt16
    let divertable: Bool
    let gesture: Bool
}
struct HardwareSnapshot: Decodable {
    let ready: Bool
    let applied: Bool
    let error: String?
    let dpi_values: [UInt32]?
    let dpi: UInt32?
    let smart_shift: Bool?
    let wheel_mode: UInt32?
    let threshold: UInt32?
    let vertical: Bool?
    let horizontal: Bool?
    let buttons: [HardwareButton]?
}

// All event outputs arrive serially on the Rust worker. No JSON, UI tasks,
// or input-event taps are used in the keyboard/mouse/scroll output path.
private final class HardwareCallbacks: @unchecked Sendable {
    let snapshot: @Sendable (Data) -> Void
    private var remainderX = 0.0
    private var remainderY = 0.0
    private let source = CGEventSource(stateID: .privateState)
    init(snapshot: @escaping @Sendable (Data) -> Void) { self.snapshot = snapshot }
    func output(_ value: SmoothMXOutput) {
        switch value.kind {
        case 1:
            var flags: CGEventFlags = []
            if value.modifiers & 1 != 0 { flags.insert(.maskCommand) }
            if value.modifiers & 2 != 0 { flags.insert(.maskShift) }
            if value.modifiers & 4 != 0 { flags.insert(.maskAlternate) }
            if value.modifiers & 8 != 0 { flags.insert(.maskControl) }
            for down in [true, false] {
                let event = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(value.code), keyDown: down)
                event?.flags = flags; event?.post(tap: .cghidEventTap)
            }
        case 2:
            guard let button = CGMouseButton(rawValue: value.code), let location = CGEvent(source: nil)?.location else { return }
            let down = value.x != 0
            let type: CGEventType = value.code == 0 ? (down ? .leftMouseDown : .leftMouseUp) : value.code == 1 ? (down ? .rightMouseDown : .rightMouseUp) : (down ? .otherMouseDown : .otherMouseUp)
            CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: location, mouseButton: button)?.post(tap: .cghidEventTap)
        case 3:
            remainderX += value.x; remainderY += value.y
            let x = remainderX.rounded(.towardZero); let y = remainderY.rounded(.towardZero)
            remainderX -= x; remainderY -= y
            guard x != 0 || y != 0 else { return }
            CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 2,
                    wheel1: Int32(clamping: Int(y)), wheel2: Int32(clamping: Int(x)), wheel3: 0)?.post(tap: .cghidEventTap)
        case 4:
            for down in [true, false] {
                let flags = NSEvent.ModifierFlags(rawValue: down ? 0xA00 : 0xB00)
                let data = Int(value.code << 16) | (down ? 0xA00 : 0xB00)
                NSEvent.otherEvent(with: .systemDefined, location: .zero, modifierFlags: flags, timestamp: 0,
                    windowNumber: 0, context: nil, subtype: 8, data1: data, data2: -1)?.cgEvent?.post(tap: .cghidEventTap)
            }
        default: break
        }
    }
}
private func hardwareSnapshot(_ context: UnsafeMutableRawPointer?, _ json: UnsafePointer<CChar>?) {
    guard let context, let json else { return }
    let box = Unmanaged<HardwareCallbacks>.fromOpaque(context).takeUnretainedValue()
    box.snapshot(Data(bytes: json, count: strlen(json)))
}
private func hardwareOutput(_ context: UnsafeMutableRawPointer?, _ output: SmoothMXOutput) {
    guard let context else { return }
    Unmanaged<HardwareCallbacks>.fromOpaque(context).takeUnretainedValue().output(output)
}
final class HardwareSession {
    private let handle: UnsafeMutableRawPointer
    private let context: UnsafeMutableRawPointer
    init(registryID: UInt64, receive: @escaping @Sendable (Data) -> Void) throws {
        let context = Unmanaged.passRetained(HardwareCallbacks(snapshot: receive)).toOpaque()
        guard let handle = smoothmx_hardware_start(registryID, hardwareSnapshot, hardwareOutput, context) else {
            Unmanaged<HardwareCallbacks>.fromOpaque(context).release()
            throw ConfigurationError.invalid
        }
        self.context = context; self.handle = handle
    }
    func apply(_ settings: MouseSettings) throws {
        try settings.validate()
        guard settings.words.withUnsafeBufferPointer({ smoothmx_hardware_apply(handle, $0.baseAddress, $0.count) }) == 0 else { throw ConfigurationError.invalid }
    }
    func restore() { _ = smoothmx_hardware_apply(handle, nil, 0) }
    deinit { smoothmx_hardware_stop(handle); Unmanaged<HardwareCallbacks>.fromOpaque(context).release() }
}
