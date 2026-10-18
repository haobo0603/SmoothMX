# Performance budget

Targets, not measured results. Measure release builds on recorded hardware/macOS versions with a specified mouse and transport.

- One resident agent with UI closed.
- Agent steady-state resident memory target: at most 25 MiB; also report macOS physical footprint.
- Idle CPU target: at most 0.1% of one core averaged over 60 seconds.
- Added callback-to-dispatch processing latency: p99 at most 1 ms under defined input load; excludes radio/OS delivery.
- Start with 1–2 application-owned persistent threads; report framework threads separately.
- No routine per-event allocation, unbounded queues, or input logging.

Measure idle, sustained scrolling, gestures, profile switching, UI open/closed, disconnected device, and sleep/wake. Record CPU, memory, wakeups, latency distributions, queue high-water marks, and drops. A single microbenchmark does not establish end-to-end performance. Use Instruments/Time Profiler and Allocations as the implementation develops.
