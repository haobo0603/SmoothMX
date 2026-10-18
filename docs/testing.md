# Verification plan

Current checks: cargo fmt, cargo clippy, cargo test, and scripts/build-macos.sh.

Seven Rust tests cover interface grouping/device separation, unknown locations, receiver classification, JSON escaping, cached-identity removal, immediate shutdown during startup, and snapshot delivery followed by shutdown. Lifecycle tests exercise native IOKit without changing hardware. Swift compilation and visual verification supplement them.

Hardware follow-up: switch the mouse off/on and verify disappearance/reappearance; repeat after sleep/wake; test a receiver independently. These actions are manual and have not yet been verified. Do not disconnect the user's active mouse automatically.

As features arrive, add meaningful protocol parser tests for malformed/truncated reports, request/response correlation, timeouts and reconnects; replay sanitized hardware fixtures. Test profile precedence, configuration migration and stale revisions. Hardware tests must verify button restoration, duplicate suppression, key release, permission denial, service crashes, sleep/wake and reconnects. Record results by device and transport. Do not represent compilation or replay tests as proof of device compatibility.
