#ifndef SMOOTHMX_H
#define SMOOTHMX_H
#include <stdint.h>
#include <stddef.h>
// UTF-8 JSON is borrowed for the callback duration. Copy before returning.
// Callback runs on the Rust discovery thread; never stop from that callback.
typedef void (*SmoothMXSnapshotCallback)(void *context, const char *json);
void *smoothmx_discovery_start(SmoothMXSnapshotCallback callback, void *context);
// Consumes the handle and waits until no callbacks can execute.
void smoothmx_discovery_stop(void *handle);
typedef struct { uint32_t kind; uint32_t code; uint32_t modifiers; double x; double y; } SmoothMXOutput;
typedef void (*SmoothMXOutputCallback)(void *context, SmoothMXOutput output);
void *smoothmx_hardware_start(uint64_t registry_id, SmoothMXSnapshotCallback callback, SmoothMXOutputCallback output, void *context);
int32_t smoothmx_hardware_apply(void *handle, const uint32_t *words, size_t count);
void smoothmx_hardware_stop(void *handle);
#endif
