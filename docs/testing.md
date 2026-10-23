# Verification plan

Current checks: `cargo fmt`, warning-free `cargo clippy`, `cargo test`, and `scripts/build-macos.sh`. The GitHub Actions workflow runs the same macOS checks so Swift compilation is not accidentally omitted.

Rust tests cover interface grouping/device separation, receiver classification, JSON escaping, discovery battery serialization, cached-identity removal, monitor startup/shutdown, CLI parsing, settings validation, battery feature fallback/parsing/events, DPI parsing, gesture direction, captured bindings, multi-button cleanup, and bounded wheel/action behavior. Lifecycle tests exercise native IOKit discovery without changing hardware. The release Swift build and app packaging/signature checks supplement them; real device writes remain a separate manual validation step.

Hardware follow-up: switch the mouse off/on and verify disappearance/reappearance; repeat after sleep/wake; test a receiver independently. These actions are manual and have not yet been verified. Do not disconnect the user's active mouse automatically.

Next, separate the HID++ transport behind a replayable test interface and add malformed/truncated reports, request/response correlation, errors, cancellation, timeouts and reconnect fixtures. Add Swift tests for profile precedence, persistence failures and configuration migrations once those migrations exist. Hardware tests must verify button restoration, duplicate suppression, key release, permission denial, service crashes, sleep/wake and reconnects. Record results by device and transport. Do not represent compilation or replay tests as proof of device compatibility.
