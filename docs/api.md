# Control API proposal — v1

The resident-service API below is not implemented. The current native app uses the in-process C ABI documented later in this file. The service should use JSON-RPC 2.0; subscriptions are a documented extension using server notifications. Negotiate API version before mutations. Limit message size and subscriber queues.

| Method | Purpose |
| --- | --- |
| system.getInfo | Version and backend status |
| devices.list | Connected device identifiers and transport |
| devices.getCapabilities | Supported settings, ranges and controls |
| devices.getState | Desired/observed state, battery and active profile |
| profiles.list/get/put/delete | Manage profiles |
| profiles.activate | Activate a validated profile |
| settings.validate | Validate without applying |
| settings.apply | Apply with configuration revision and per-setting results |
| events.subscribe | Bounded device/state/error notifications |
| diagnostics.snapshot | Sanitized diagnostics; no captured input |

Errors include UNSUPPORTED_FEATURE, PERMISSION_DENIED, DEVICE_DISCONNECTED, REVISION_CONFLICT, TIMEOUT and INVALID_SETTING. Assign numeric JSON-RPC application codes before implementation.

Validate all changes before writes; device writes are not generally atomic. Return individual results and observed state after partial failure. Never claim transactional rollback without hardware verification. Reject stale revisions. Persist configuration through atomic replacement and schema migrations.

Planned service-backed CLI additions: status, set, bind, profile, and config validate. Currently help, version, doctor, devices, devices --json, capabilities (with probe as a compatibility alias), and battery are implemented. Hardware commands accept only the connection-scoped `hid-<hex>` identifiers returned by discovery. No public raw HID endpoint exists.

## Current discovery ABI (implemented)

`smoothmx_discovery_start(callback, context)` returns an opaque handle or null on worker creation failure. It sends UTF-8 JSON snapshots on the discovery thread. `smoothmx_discovery_stop(handle)` consumes the handle and joins the worker; no callbacks occur after return. Never call stop from its callback. JSON memory is borrowed only during callback execution.

Snapshot schema: `schema_version: 1`, `devices: [...]`, `error: string|null`. Each device contains id, name, vendor_id, product_id, transport, kind, interface_count, and `battery_level` (0–100 or null when macOS does not expose it). IDs are connection-scoped, not persistent. CLI --json emits the same schema. This ABI is separate from the future service JSON-RPC API.

`smoothmx_hardware_start(registry_id, snapshot_callback, output_callback, context)` starts one serialized direct-Bluetooth HID++ session. The first snapshot reports whether capability discovery is ready. Ready snapshots contain supported DPI values, current DPI and wheel state, feature flags, reprogrammable buttons, cached battery state, applied state, and an optional error. Unified/legacy battery events update that cached state, with a five-minute read as a fallback while the session is open. `smoothmx_hardware_apply` accepts exactly 105 versioned `uint32_t` words after both Swift and Rust validation; a null settings pointer requests restoration. `smoothmx_hardware_stop` consumes the handle, restores captured state when the device remains connected, releases synthetic pressed buttons, and waits until callbacks have stopped.

Hardware snapshots are internal and currently unversioned; they must not be treated as the future service protocol. The output callback is also internal and carries a fixed C struct for bounded shortcut, mouse-button, scroll, media-key, and device-worker actions. Neither ABI exposes raw HID reports.
