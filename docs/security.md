# Permissions and recovery

Run per-user. Request only the Accessibility/Input Monitoring permissions required by implemented features; validate the exact API/process behavior on target macOS releases. Do not ask for permission merely to open the settings UI.

Restrict local IPC access and validate peers, lengths and schemas. Do not log key contents or raw user input. Diagnostics must redact device identifiers by default. No network listener or telemetry planned.

Keep normal mouse behavior when possible. Track diverted controls and pressed synthetic keys; restore/release on normal shutdown. Validate crash/disconnect recovery on real hardware rather than assuming cleanup handlers always run. Never enable diversion until recovery has been demonstrated. Detect or document conflicts with Options+ and other remappers.
