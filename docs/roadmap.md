# Roadmap

## 0 — Scaffold
Rust workspace, macOS CLI, SwiftUI preview, design documentation. No hardware claims.

## 1 — Hardware feasibility (in progress)
Implemented: Rust discovery, text/JSON CLI, SwiftUI integration, live notification plumbing, safe monitor shutdown, and local app packaging. Verified discovery: MX Master 3 over Bluetooth Low Energy. Still pending: physical reconnect/sleep-wake checks, HID++ capabilities, battery, and reversible settings.

### Remaining hardware work
Identify the exact mouse and connection type. Enumerate the intended Logitech HID interfaces, discover HID++ capabilities, read battery information, and change/restore one supported setting. Validate Bluetooth and receiver separately. Exit: hardware evidence and a measured idle baseline.

## 2 — Input feasibility
Remap one supported button and restore it. Validate permission denial, helper identity, duplicate suppression, stuck-key prevention, crash behavior, sleep/wake, disconnect/reconnect, and conflicts with other mouse software. Exit: reliable recovery without leaving buttons diverted.

## 3 — Service and configuration
Versioned TOML, desired versus observed state, profile validation, local IPC, CLI commands, bounded diagnostics. Exit: UI/CLI changes cannot silently overwrite one another; invalid settings do not affect hardware.

## 4 — Native application
Capability-driven settings, permissions onboarding, buttons, scrolling, profiles, accessibility, signed app/helper packaging. Exit: UI can close while service continues.

## 5 — Advanced behavior and release
Gestures, application profiles, import/export, performance measurements, hardware regression matrix, signing/notarization and clean uninstall.

Deferred: firmware updates, receiver pairing, Flow-style networking, arbitrary scripts, advanced macros. Linux and other Unix implementations are excluded.
