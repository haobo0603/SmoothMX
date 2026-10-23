# Architecture

Status: in-process discovery, direct-Bluetooth HID++ control, input policy, and the native settings UI are implemented; the resident service architecture remains proposed.

Current discovery path: SwiftUI → narrow C ABI → Rust discovery worker → IOHIDManager. The CLI calls the same discovery backend synchronously. The worker sleeps on a CFRunLoop and publishes JSON snapshots only when normalized devices change. Shutdown signals a dedicated run-loop source and joins the worker before Swift releases the callback context.

Current settings path: SwiftUI → validated fixed-width C ABI settings → one Rust hardware worker → IOHIDDevice/HID++. The worker serializes requests, correlates responses by feature/function/software ID, applies only discovered features, verifies supported readbacks, and captures original values for best-effort restoration. Diverted input is processed by a bounded Rust state machine; the narrow output callback asks native Core Graphics/AppKit APIs to emit shortcuts, mouse buttons, media keys, or pixel scrolling. Capability and state snapshots, including cached battery state, return as bounded JSON. Direct settings currently accept Bluetooth devices only; receiver children are not guessed.

Only Logitech vendor 0x046D is matched. Mouse classification checks primary usage and DeviceUsagePairs. USB interfaces are grouped only with matching nonzero LocationID, vendor and product IDs. Other interfaces use per-connection registry IDs; these are not persistent profile identifiers. Receiver names take classification precedence over mouse usages, and no paired child device is invented. Removal uses cached interface identity because its registry service may already be gone. Automatic reconnect may use a unique device metadata match, but never guesses when multiple devices match.

FFI declarations live in platform-macos/src/native.rs and are checked against the installed SDK headers. CF Create/Copy results use RAII; borrowed device properties are not released. Callback state is accessed only on its owning discovery thread. No third-party Rust dependencies are required.

SwiftUI app / Rust CLI → local control API → per-user agent → Rust engine → macOS adapters → mouse.

The agent owns all device configuration. HID++ feature discovery determines supported operations. Use IOHIDManager/IOHIDDevice for discovery and reports, Core Graphics for necessary input interception/injection, and AppKit foreground-application notifications for profiles. A narrow C ABI may bridge Rust and native framework code. Validate permissions against the final process identity early.

Keep the control path (configuration, serialization, UI notifications, disk) separate from the input path (bounded event processing and action dispatch). Never block input callbacks waiting on HID requests or clients. Preserve ordinary OS pointer handling wherever possible. Use bounded queues, request deadlines, cancellation on disconnect, and reconnect capability discovery.

Proposed agent: launchd per-user agent registered via SMAppService. Start with one event loop and add a worker only where measured blocking requires it. No kernel extension planned. GUI lifecycle must not govern remapping lifecycle.

Use JSON-RPC 2.0 over an owner-restricted Unix-domain socket for control messages, not raw input events. Verify local peer identity. Review whether XPC better fits signed helper deployment during the macOS feasibility spike; record the decision before freezing IPC.

Existing crates: core, platform-macos, cli. HID++ currently lives in the macOS adapter while the supported transport is macOS-only. Future API/service crates should be added when the resident agent exists rather than as empty libraries. No Linux adapter is planned.
