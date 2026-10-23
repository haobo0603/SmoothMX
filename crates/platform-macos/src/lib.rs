//! Read-only macOS HID discovery. No device open, input capture, or report writes.
#[cfg(not(target_os = "macos"))]
compile_error!("SmoothMX currently supports macOS only");
mod native;
use native::*;
use smoothmx_core::{Device, HidInterface, devices_json, group_devices, json_string};
use std::{
    collections::BTreeMap,
    ffi::{CString, c_char, c_void},
    ptr,
    sync::{Arc, Mutex},
    thread,
};

const UTF8: u32 = 0x08000100;
const SINT32: isize = 3;
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
fn cf_string(value: &str) -> Owned {
    let text = CString::new(value).expect("internal property key has no NUL");
    Owned(unsafe { CFStringCreateWithCString(ptr::null(), text.as_ptr(), UTF8) })
}
fn manager() -> Result<Owned, String> {
    unsafe {
        let manager = Owned(IOHIDManagerCreate(ptr::null(), 0));
        if manager.0.is_null() {
            return Err("macOS could not create the HID discovery manager".into());
        }
        let key = cf_string("VendorID");
        let vendor: i32 = 0x046d;
        let value = Owned(CFNumberCreate(
            ptr::null(),
            SINT32,
            (&vendor as *const i32).cast(),
        ));
        if key.0.is_null() || value.0.is_null() {
            return Err("Cannot allocate vendor matching properties".into());
        }
        let matching = Owned(CFDictionaryCreate(
            ptr::null(),
            &key.0,
            &value.0,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        ));
        if matching.0.is_null() {
            return Err("Cannot allocate vendor matching dictionary".into());
        }
        IOHIDManagerSetDeviceMatching(manager.0, matching.0);
        Ok(manager)
    }
}
fn property(device: Ref, key: &str) -> Ref {
    let key = cf_string(key);
    if key.0.is_null() {
        return ptr::null();
    }
    unsafe { IOHIDDeviceGetProperty(device, key.0) }
}
fn number(device: Ref, key: &str) -> u32 {
    optional_number(device, key).unwrap_or_default()
}
fn optional_number(device: Ref, key: &str) -> Option<u32> {
    unsafe {
        let value = property(device, key);
        let mut result: i32 = 0;
        if value.is_null()
            || CFGetTypeID(value) != CFNumberGetTypeID()
            || CFNumberGetValue(value, SINT32, (&mut result as *mut i32).cast()) == 0
            || result < 0
        {
            return None;
        }
        Some(result as u32)
    }
}
fn valid_battery_level(level: u32) -> Option<u8> {
    (level <= 100).then_some(level as u8)
}
fn string(device: Ref, key: &str) -> Option<String> {
    unsafe {
        let value = property(device, key);
        if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
            return None;
        }
        // Bound device-provided metadata; never grow memory based on untrusted lengths.
        let mut bytes = [0u8; 2048];
        if CFStringGetCString(value, bytes.as_mut_ptr().cast(), bytes.len() as isize, UTF8) == 0 {
            return None;
        }
        let end = bytes.iter().position(|b| *b == 0)?;
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
}
fn registry_id(device: Ref) -> Option<u64> {
    let mut id = 0;
    let result =
        unsafe { IORegistryEntryGetRegistryEntryID(IOHIDDeviceGetService(device), &mut id) };
    (result == 0).then_some(id)
}
fn has_mouse_usage(device: Ref) -> bool {
    if number(device, "PrimaryUsagePage") == 1 && number(device, "PrimaryUsage") == 2 {
        return true;
    }
    unsafe {
        let pairs = property(device, "DeviceUsagePairs");
        if pairs.is_null() || CFGetTypeID(pairs) != CFArrayGetTypeID() {
            return false;
        }
        let page_key = cf_string("DeviceUsagePage");
        let usage_key = cf_string("DeviceUsage");
        if page_key.0.is_null() || usage_key.0.is_null() {
            return false;
        }
        for index in 0..CFArrayGetCount(pairs) {
            let pair = CFArrayGetValueAtIndex(pairs, index);
            if pair.is_null() || CFGetTypeID(pair) != CFDictionaryGetTypeID() {
                continue;
            }
            let page = CFDictionaryGetValue(pair, page_key.0);
            let usage = CFDictionaryGetValue(pair, usage_key.0);
            if page.is_null()
                || usage.is_null()
                || CFGetTypeID(page) != CFNumberGetTypeID()
                || CFGetTypeID(usage) != CFNumberGetTypeID()
            {
                continue;
            }
            let mut page_value: i32 = 0;
            let mut usage_value: i32 = 0;
            CFNumberGetValue(page, SINT32, (&mut page_value as *mut i32).cast());
            CFNumberGetValue(usage, SINT32, (&mut usage_value as *mut i32).cast());
            if page_value == 1 && usage_value == 2 {
                return true;
            }
        }
    }
    false
}
fn read_interface(device: Ref) -> Option<HidInterface> {
    let vendor_id = number(device, "VendorID");
    let mouse_usage = has_mouse_usage(device);
    if vendor_id != 0x046d {
        return None;
    }
    Some(HidInterface {
        registry_id: registry_id(device)?,
        name: string(device, "Product")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unidentified Logitech HID device".into()),
        vendor_id,
        product_id: number(device, "ProductID"),
        transport: string(device, "Transport").unwrap_or_else(|| "Unknown".into()),
        location_id: number(device, "LocationID"),
        usage_page: if mouse_usage {
            1
        } else {
            number(device, "PrimaryUsagePage")
        },
        usage: if mouse_usage {
            2
        } else {
            number(device, "PrimaryUsage")
        },
        battery_level: optional_number(device, "BatteryLevel").and_then(valid_battery_level),
    })
}
fn interfaces(manager: Ref) -> BTreeMap<usize, HidInterface> {
    unsafe {
        let set = Owned(IOHIDManagerCopyDevices(manager));
        if set.0.is_null() {
            return BTreeMap::new();
        }
        let mut devices = vec![ptr::null(); CFSetGetCount(set.0) as usize];
        CFSetGetValues(set.0, devices.as_mut_ptr());
        devices
            .into_iter()
            .filter_map(|device| read_interface(device).map(|record| (device as usize, record)))
            .collect()
    }
}
pub fn discover() -> Result<Vec<Device>, String> {
    let manager = manager()?;
    Ok(group_devices(interfaces(manager.0).into_values()))
}

/// Called on the discovery thread. The UTF-8 JSON pointer is valid only during the call.
pub type SnapshotCallback = unsafe extern "C" fn(*mut c_void, *const c_char);
struct State {
    devices: BTreeMap<usize, HidInterface>,
    callback: SnapshotCallback,
    context: usize,
    last_json: String,
}
impl State {
    fn publish(&mut self) {
        let json = devices_json(&group_devices(self.devices.values().cloned()));
        if json == self.last_json {
            return;
        }
        emit(self.callback, self.context, &json);
        self.last_json = json;
    }
}
fn emit(callback: SnapshotCallback, context: usize, json: &str) {
    if let Ok(message) = CString::new(json) {
        unsafe { callback(context as *mut c_void, message.as_ptr()) }
    }
}
unsafe extern "C" fn added(context: *mut c_void, result: i32, _: Ref, device: Ref) {
    if result != 0 {
        return;
    }
    let state = unsafe { &mut *context.cast::<State>() };
    if let Some(record) = read_interface(device) {
        state.devices.insert(device as usize, record);
        state.publish();
    }
}
unsafe extern "C" fn removed(context: *mut c_void, _: i32, _: Ref, device: Ref) {
    let state = unsafe { &mut *context.cast::<State>() };
    // The registry service may already be gone; use the callback's identity,
    // cached while connected, instead of querying a removed device.
    state.devices.remove(&(device as usize));
    state.publish();
}
// Raw CF pointers are stored as integers only for signaling under this lock.
// The discovery thread owns them; it clears them under the lock before releasing.
#[derive(Default)]
struct Control {
    stopped: bool,
    runloop: usize,
    source: usize,
}
pub struct Monitor {
    control: Arc<Mutex<Control>>,
    worker: Option<thread::JoinHandle<()>>,
}
unsafe extern "C" fn stop_runloop(_: *mut c_void) {
    unsafe {
        CFRunLoopStop(CFRunLoopGetCurrent());
    }
}
fn run(
    control: Arc<Mutex<Control>>,
    callback: SnapshotCallback,
    context: usize,
) -> Result<(), String> {
    let manager = manager()?;
    let mut state = State {
        devices: interfaces(manager.0),
        callback,
        context,
        last_json: String::new(),
    };
    unsafe {
        let runloop = CFRunLoopGetCurrent();
        let mut source_context = SourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            description: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(stop_runloop),
        };
        let source = Owned(CFRunLoopSourceCreate(ptr::null(), 0, &mut source_context));
        if source.0.is_null() {
            return Err("Cannot create discovery shutdown source".into());
        }
        CFRunLoopAddSource(runloop, source.0, kCFRunLoopDefaultMode);
        {
            let mut guard = control.lock().unwrap_or_else(|p| p.into_inner());
            if guard.stopped {
                CFRunLoopRemoveSource(runloop, source.0, kCFRunLoopDefaultMode);
                return Ok(());
            }
            guard.runloop = runloop as usize;
            guard.source = source.0 as usize;
        }
        let state_ptr = (&mut state as *mut State).cast();
        IOHIDManagerRegisterDeviceMatchingCallback(manager.0, Some(added), state_ptr);
        IOHIDManagerRegisterDeviceRemovalCallback(manager.0, Some(removed), state_ptr);
        IOHIDManagerScheduleWithRunLoop(manager.0, runloop, kCFRunLoopDefaultMode);
        state.publish();
        CFRunLoopRun();
        IOHIDManagerUnscheduleFromRunLoop(manager.0, runloop, kCFRunLoopDefaultMode);
        IOHIDManagerRegisterDeviceMatchingCallback(manager.0, None, ptr::null_mut());
        IOHIDManagerRegisterDeviceRemovalCallback(manager.0, None, ptr::null_mut());
        {
            let mut guard = control.lock().unwrap_or_else(|p| p.into_inner());
            guard.runloop = 0;
            guard.source = 0;
        }
        CFRunLoopRemoveSource(runloop, source.0, kCFRunLoopDefaultMode);
    }
    Ok(())
}

/// Start an event-driven monitor. Returns null only when a worker cannot be created.
/// # Safety
/// Callback/context must remain valid until stop returns. Callback must not unwind,
/// block indefinitely, or call stop on its own monitor. Copy JSON before returning.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_discovery_start(
    callback: SnapshotCallback,
    context: *mut c_void,
) -> *mut Monitor {
    let control = Arc::new(Mutex::new(Control::default()));
    let worker_control = Arc::clone(&control);
    let context = context as usize;
    let worker = thread::Builder::new()
        .name("smoothmx-discovery".into())
        .spawn(move || {
            if let Err(error) = run(worker_control, callback, context) {
                emit(
                    callback,
                    context,
                    &format!(
                        "{{\"schema_version\":1,\"devices\":[],\"error\":{}}}",
                        json_string(&error)
                    ),
                );
            }
        });
    match worker {
        Ok(worker) => Box::into_raw(Box::new(Monitor {
            control,
            worker: Some(worker),
        })),
        Err(_) => ptr::null_mut(),
    }
}
/// Stop and join discovery. No callbacks occur after return.
/// # Safety
/// Pass a live handle from start exactly once, from outside its callback thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_discovery_stop(handle: *mut Monitor) {
    if handle.is_null() {
        return;
    }
    let mut monitor = unsafe { Box::from_raw(handle) };
    {
        let mut guard = monitor.control.lock().unwrap_or_else(|p| p.into_inner());
        guard.stopped = true;
        if guard.source != 0 {
            unsafe {
                CFRunLoopSourceSignal(guard.source as Ref);
                CFRunLoopWakeUp(guard.runloop as Ref);
            }
        }
    }
    if let Some(worker) = monitor.worker.take() {
        let _ = worker.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn ignore(_: *mut c_void, _: *const c_char) {}
    #[test]
    fn removal_uses_cached_identity_without_querying_disconnected_hardware() {
        let record = HidInterface {
            registry_id: 42,
            name: "Mouse".into(),
            vendor_id: 0x046d,
            product_id: 1,
            transport: "Bluetooth".into(),
            location_id: 0,
            usage_page: 1,
            usage: 2,
            battery_level: None,
        };
        let mut state = State {
            devices: BTreeMap::from([(123usize, record)]),
            callback: ignore,
            context: 0,
            last_json: String::new(),
        };
        unsafe {
            removed(
                (&mut state as *mut State).cast(),
                0,
                ptr::null(),
                123usize as Ref,
            );
        }
        assert!(state.devices.is_empty());
        assert_eq!(state.last_json, devices_json(&[]));
    }
    #[test]
    fn immediate_shutdown_handles_startup_race() {
        for _ in 0..20 {
            unsafe {
                let monitor = smoothmx_discovery_start(ignore, ptr::null_mut());
                assert!(!monitor.is_null());
                smoothmx_discovery_stop(monitor);
            }
        }
    }
    #[test]
    fn running_monitor_delivers_snapshot_and_stops() {
        unsafe extern "C" fn notify(context: *mut c_void, _: *const c_char) {
            let sender = unsafe { &*context.cast::<std::sync::mpsc::Sender<()>>() };
            let _ = sender.send(());
        }
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        unsafe {
            let handle = smoothmx_discovery_start(
                notify,
                (&sender as *const std::sync::mpsc::Sender<()>)
                    .cast_mut()
                    .cast(),
            );
            assert!(!handle.is_null());
            let received = receiver.recv_timeout(std::time::Duration::from_secs(5));
            smoothmx_discovery_stop(handle);
            assert!(received.is_ok());
        }
    }
    #[test]
    fn battery_property_preserves_zero_and_rejects_invalid_percentage() {
        assert_eq!(valid_battery_level(0), Some(0));
        assert_eq!(valid_battery_level(100), Some(100));
        assert_eq!(valid_battery_level(101), None);
    }
}

pub mod hardware;
