# Roadmap

## 0 — Scaffold
Rust workspace, macOS CLI, SwiftUI preview, design documentation. No hardware claims.

## 1 — Hardware feasibility (implementation complete; validation in progress)
Implemented for direct Bluetooth devices: Rust discovery, text/JSON CLI, live notifications, serialized HID++ capability discovery, Unified/legacy battery reads, reversible DPI/SmartShift/wheel/button settings, readback checks, safe shutdown, SwiftUI integration, and local app packaging. Verified discovery: MX Master 3 over Bluetooth Low Energy. Protocol and recovery code is covered by automated tests, but that is not a substitute for the remaining physical-device validation.

### Remaining hardware work
Record battery and one change/readback/restore cycle on the target mouse. Validate power-cycle, disconnect/reconnect and sleep/wake behavior. Implement and validate receiver child enumeration separately; receiver configuration is intentionally rejected until then. Exit: hardware evidence and a measured idle baseline.

## 2 — Input feasibility (implementation complete; validation in progress)
The bounded input engine implements button remapping, gestures, vertical/thumb-wheel diversion, duplicate suppression, captured bindings, and synthetic-button cleanup. Validate a physical button remap and restore, permission denial, final helper identity, crash behavior, sleep/wake, disconnect/reconnect, and conflicts with other mouse software. Exit: reliable recovery without leaving buttons diverted.

## 3 — Service and configuration (partially implemented)
The app has versioned, validated, atomically replaced local JSON configuration and application profiles. Still required: independently managed per-user agent, desired-versus-observed revisions, authenticated local IPC, service-backed CLI mutations, migrations, and bounded diagnostics. Exit: UI/CLI changes cannot silently overwrite one another; invalid settings do not affect hardware.

## 4 — Native application (partially implemented)
Capability-driven settings, permission links, buttons, scrolling, gestures, profiles and Accessibility checks are implemented in SwiftUI. Remaining: resident helper registration, final permission onboarding, accessibility polish, and Developer ID app/helper packaging. Exit: UI can quit while the service continues.

## 5 — Advanced behavior and release
Gestures, application profiles, import/export, performance measurements, hardware regression matrix, signing/notarization and clean uninstall.

Deferred: firmware updates, receiver pairing, Flow-style networking, arbitrary scripts, advanced macros. Linux and other Unix implementations are excluded.
