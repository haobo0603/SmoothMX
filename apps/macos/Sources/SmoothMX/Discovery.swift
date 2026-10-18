import Foundation
import CSmoothMX

struct MouseDevice: Decodable, Identifiable {
    let id: String
    let name: String
    let vendor_id: UInt32
    let product_id: UInt32
    let transport: String
    let kind: String
    let interface_count: Int

    var detail: String {
        switch kind {
        case "receiver": "Receiver detected. Paired mouse identification needs HID++ support."
        case "mouse": "Mouse detected. Select it in the sidebar to configure pointing, scrolling, buttons, and gestures."
        default: "Logitech HID interface detected. Device classification is unresolved."
        }
    }
}
struct DiscoverySnapshot: Decodable {
    let schema_version: Int
    let devices: [MouseDevice]
    let error: String?
}

private final class CallbackBox: Sendable {
    let receive: @Sendable (Data) -> Void
    init(receive: @escaping @Sendable (Data) -> Void) { self.receive = receive }
}
private func receiveSnapshot(_ context: UnsafeMutableRawPointer?, _ json: UnsafePointer<CChar>?) {
    guard let context, let json else { return }
    let box = Unmanaged<CallbackBox>.fromOpaque(context).takeUnretainedValue()
    // Copy borrowed bytes before Rust returns from the callback.
    box.receive(Data(bytes: json, count: strlen(json)))
}
private final class DiscoverySession {
    let context: UnsafeMutableRawPointer
    let handle: UnsafeMutableRawPointer

    init(receive: @escaping @Sendable (Data) -> Void) throws {
        let context = Unmanaged.passRetained(CallbackBox(receive: receive)).toOpaque()
        guard let handle = smoothmx_discovery_start(receiveSnapshot, context) else {
            Unmanaged<CallbackBox>.fromOpaque(context).release()
            throw NSError(domain: "SmoothMX", code: 1, userInfo: [NSLocalizedDescriptionKey: "Could not start the discovery thread."])
        }
        self.context = context
        self.handle = handle
    }
    deinit {
        smoothmx_discovery_stop(handle)
        Unmanaged<CallbackBox>.fromOpaque(context).release()
    }
}

@MainActor
final class DiscoveryModel: ObservableObject {
    @Published private(set) var devices: [MouseDevice] = []
    @Published private(set) var error: String?
    @Published private(set) var scanning = true
    private var session: DiscoverySession?

    init() {
        do {
            session = try DiscoverySession { [weak self] data in
                Task { @MainActor [weak self] in self?.receive(data) }
            }
        } catch {
            self.error = error.localizedDescription
            scanning = false
        }
    }
    private func receive(_ data: Data) {
        scanning = false
        do {
            let snapshot = try JSONDecoder().decode(DiscoverySnapshot.self, from: data)
            guard snapshot.schema_version == 1 else {
                error = "The discovery backend returned an unsupported data version."
                devices = []
                return
            }
            devices = snapshot.devices
            error = snapshot.error
        } catch {
            devices = []
            self.error = "Could not read the discovery result: \(error.localizedDescription)"
        }
    }
}
