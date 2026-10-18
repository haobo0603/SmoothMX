# SmoothMX

A macOS-only mouse configuration project for Logitech MX Master devices. Rust powers the discovery backend and CLI; SwiftUI provides the native interface.

## Current status

Read-only mouse discovery is implemented in Rust and shared by the CLI and SwiftUI app. The app uses IOKit connection/removal callbacks on one discovery thread, with no periodic scans or input capture. Your mouse settings are not changed.

Verified on the development machine: **MX Master 3 over Bluetooth Low Energy**, vendor `0x046D`, product `0xB023`. Receiver child identification, battery, HID++ settings, remapping, and background service IPC are not implemented. Physical reconnect and sleep/wake checks remain pending.

## Development

Install stable Rust using rustup, and Xcode from the Mac App Store. Rustfmt and Clippy are used for checks. Initial hardware target: Apple Silicon; minimum planned OS: macOS 13.

```sh
cargo run -p smoothmx-cli -- devices
cargo run -p smoothmx-cli -- devices --json
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/build-macos.sh
open apps/macos/.build/SmoothMX.app
```

The build script selects Xcode, builds the Rust static library and CLI in release mode, builds SwiftUI, and creates a locally ad-hoc signed app. It defaults to `/Applications/Xcode.app`; override `DEVELOPER_DIR` if needed. Debug symbols are omitted for this local preview build. This is not a notarized distribution build.

The Swift package links `target/release/libsmoothmx_platform_macos.a`; build that Rust target before invoking Swift directly. The app currently hosts discovery itself; there is no resident agent when the app closes.

## Design

- One event-driven per-user background agent; settings UI can quit independently.
- Native IOKit HID access; Core Graphics input APIs only where required.
- Discover device capabilities before offering settings.
- Local-only configuration and IPC; no network service or accounts.
- macOS UI and macOS CLI; Linux and other Unix implementations are out of scope.

See [roadmap](docs/roadmap.md), [architecture](docs/architecture.md), [API proposal](docs/api.md), and [development setup](docs/development.md).

No open-source license has been selected. Do not copy third-party implementation code without a license review.
