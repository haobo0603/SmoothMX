# Device support

## Verified discovery

| Device-reported model | Transport | Vendor | Product | Result |
| --- | --- | --- | --- | --- |
| MX Master 3 | Bluetooth Low Energy | 0x046D | 0xB023 | CLI and SwiftUI enumeration and mouse usage classification verified on 2026-10-18 |

Physical reconnect, power-cycle, sleep/wake, receiver paths, battery and settings are not verified. Device detection does not establish HID++ support. Live updates are implemented using IOKit callbacks; lifecycle and removal logic have automated tests.

USB interfaces sharing a nonzero location and VID/PID are grouped. Bluetooth interfaces remain separate unless the OS represents them as one device. Receiver identification currently uses its reported name; unrecognized receiver names may remain unresolved or appear as a mouse interface. No claim is made about devices paired behind receivers.

Track each model × transport × macOS version with discovery, battery, DPI, buttons, wheel, SmartShift, thumb wheel, gestures, sleep/wake and restore results. Distinguish unsupported from untested. Avoid serial numbers in diagnostics and fixtures.

References: https://pwr-solaar.github.io/Solaar/devices/ and https://github.com/PixlOne/logiops/wiki/Configuration . Protocol references only; review licensing before code reuse.
