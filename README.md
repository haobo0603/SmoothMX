# SmoothMX

A macOS-only mouse configuration project for Logitech MX Master devices. Rust powers the discovery backend and CLI; SwiftUI provides the native interface.

## Current status

Rust implements event-driven IOKit discovery plus a serialized, reversible HID++ session for direct Bluetooth connections. The native app exposes capability-driven DPI, SmartShift, main/thumb-wheel, button, gesture, battery, and per-application profile controls. Settings are validated before writes, read back where the protocol permits, and restored when customization is paused or the app exits normally.

Discovery is verified on the development machine with an **MX Master 3 over Bluetooth Low Energy**, vendor `0x046D`, product `0xB023`. HID++ battery and settings paths are implemented but still require a recorded real-hardware validation pass. Receiver child identification/configuration, a resident background agent and IPC, crash recovery, signing/notarization, and physical reconnect/sleep-wake validation remain pending. The app never changes a device merely by discovering it or reading capabilities.

## Development

Install stable Rust using rustup, and Xcode from the Mac App Store. Rustfmt and Clippy are used for checks. Initial hardware target: Apple Silicon; minimum planned OS: macOS 13.

```sh
cargo run -p smoothmx-cli -- devices
cargo run -p smoothmx-cli -- devices --json
cargo run -p smoothmx-cli -- capabilities hid-<registry-id>
cargo run -p smoothmx-cli -- battery hid-<registry-id>
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/build-macos.sh
open apps/macos/.build/SmoothMX.app
```

The build script selects Xcode, builds the Rust static library and CLI in release mode, builds SwiftUI, and creates a locally ad-hoc signed app. It defaults to `/Applications/Xcode.app`; override `DEVELOPER_DIR` if needed. Debug symbols are omitted for this local preview build. This is not a notarized distribution build.

The Swift package links `target/release/libsmoothmx_platform_macos.a`; build that Rust target before invoking Swift directly. The app currently owns the hardware session. Closing its window leaves the macOS app process running; quitting the process restores the captured device state and ends customization. There is not yet an independently managed resident agent.

## Design

- Target architecture: one event-driven per-user background agent so the settings UI can quit independently.
- Native IOKit HID access; Core Graphics input APIs only where required.
- Discover device capabilities before offering settings.
- Local-only configuration and IPC; no network service or accounts.
- macOS UI and macOS CLI; Linux and other Unix implementations are out of scope.

See [roadmap](docs/roadmap.md), [architecture](docs/architecture.md), [API proposal](docs/api.md), and [development setup](docs/development.md).

No open-source license has been selected. Do not copy third-party implementation code without a license review.
