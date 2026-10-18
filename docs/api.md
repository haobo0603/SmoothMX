# Control API proposal — v1

Not implemented. Use JSON-RPC 2.0; subscriptions are a documented extension using server notifications. Negotiate API version before mutations. Limit message size and subscriber queues.

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

Planned CLI: devices, capabilities, status, set, bind, profile, config validate, doctor. Currently help, version, doctor, devices, and devices --json are implemented. No public raw HID endpoint.

## Current discovery ABI (implemented)

`smoothmx_discovery_start(callback, context)` returns an opaque handle or null on worker creation failure. It sends UTF-8 JSON snapshots on the discovery thread. `smoothmx_discovery_stop(handle)` consumes the handle and joins the worker; no callbacks occur after return. Never call stop from its callback. JSON memory is borrowed only during callback execution.

Snapshot schema: `schema_version: 1`, `devices: [...]`, `error: string|null`. Each device contains id, name, vendor_id, product_id, transport, kind, and interface_count. IDs are connection-scoped, not persistent. CLI --json emits the same schema. This ABI is separate from the future service JSON-RPC API.
