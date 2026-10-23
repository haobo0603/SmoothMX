//! Minimal declarations from the macOS SDK. All ownership stays on the discovery thread.
#![allow(non_snake_case)]
use std::ffi::{c_char, c_void};
pub type Ref = *const c_void;
pub type DeviceCallback = unsafe extern "C" fn(*mut c_void, i32, Ref, Ref);
pub type HidCallback = unsafe extern "C" fn(*mut c_void, i32, Ref);
#[repr(C)]
pub struct SourceContext {
    pub version: isize,
    pub info: *mut c_void,
    pub retain: Option<unsafe extern "C" fn(Ref) -> Ref>,
    pub release: Option<unsafe extern "C" fn(Ref)>,
    pub description: Option<unsafe extern "C" fn(Ref) -> Ref>,
    pub equal: Option<unsafe extern "C" fn(Ref, Ref) -> u8>,
    pub hash: Option<unsafe extern "C" fn(Ref) -> usize>,
    pub schedule: Option<unsafe extern "C" fn(*mut c_void, Ref, Ref)>,
    pub cancel: Option<unsafe extern "C" fn(*mut c_void, Ref, Ref)>,
    pub perform: Option<unsafe extern "C" fn(*mut c_void)>,
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub static kCFRunLoopDefaultMode: Ref;
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;
    pub fn CFRelease(value: Ref);
    pub fn CFGetTypeID(value: Ref) -> usize;
    pub fn CFStringGetTypeID() -> usize;
    pub fn CFNumberGetTypeID() -> usize;
    pub fn CFArrayGetTypeID() -> usize;
    pub fn CFDictionaryGetTypeID() -> usize;
    pub fn CFArrayGetCount(array: Ref) -> isize;
    pub fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    pub fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    pub fn CFStringCreateWithCString(allocator: Ref, text: *const c_char, encoding: u32) -> Ref;
    pub fn CFStringGetCString(value: Ref, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    pub fn CFNumberCreate(allocator: Ref, kind: isize, value: Ref) -> Ref;
    pub fn CFNumberGetValue(value: Ref, kind: isize, output: *mut c_void) -> u8;
    pub fn CFDictionaryCreate(
        allocator: Ref,
        keys: *const Ref,
        values: *const Ref,
        count: isize,
        key_callbacks: Ref,
        value_callbacks: Ref,
    ) -> Ref;
    pub fn CFSetGetCount(set: Ref) -> isize;
    pub fn CFSetGetValues(set: Ref, values: *mut Ref);
    pub fn CFRunLoopGetCurrent() -> Ref;
    pub fn CFRunLoopRun();
    pub fn CFRunLoopStop(runloop: Ref);
    pub fn CFRunLoopWakeUp(runloop: Ref);
    pub fn CFRunLoopSourceCreate(allocator: Ref, order: isize, context: *mut SourceContext) -> Ref;
    pub fn CFRunLoopAddSource(runloop: Ref, source: Ref, mode: Ref);
    pub fn CFRunLoopRemoveSource(runloop: Ref, source: Ref, mode: Ref);
    pub fn CFRunLoopSourceSignal(source: Ref);
}
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOHIDManagerCreate(allocator: Ref, options: u32) -> Ref;
    pub fn IOHIDManagerSetDeviceMatching(manager: Ref, matching: Ref);
    pub fn IOHIDManagerCopyDevices(manager: Ref) -> Ref;
    pub fn IOHIDManagerRegisterDeviceMatchingCallback(
        manager: Ref,
        callback: Option<DeviceCallback>,
        context: *mut c_void,
    );
    pub fn IOHIDManagerRegisterDeviceRemovalCallback(
        manager: Ref,
        callback: Option<DeviceCallback>,
        context: *mut c_void,
    );
    pub fn IOHIDManagerScheduleWithRunLoop(manager: Ref, runloop: Ref, mode: Ref);
    pub fn IOHIDManagerUnscheduleFromRunLoop(manager: Ref, runloop: Ref, mode: Ref);
    pub fn IOHIDDeviceGetProperty(device: Ref, key: Ref) -> Ref;
    pub fn IOHIDDeviceGetService(device: Ref) -> u32;
    pub fn IORegistryEntryGetRegistryEntryID(service: u32, id: *mut u64) -> i32;
}

pub type ReportCallback = unsafe extern "C" fn(*mut c_void, i32, Ref, u32, u32, *mut u8, isize);
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRetain(value: Ref) -> Ref;
    pub fn CFRunLoopRunInMode(mode: Ref, seconds: f64, return_after_source: u8) -> i32;
}
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOHIDDeviceOpen(device: Ref, options: u32) -> i32;
    pub fn IOHIDDeviceClose(device: Ref, options: u32) -> i32;
    pub fn IOHIDDeviceScheduleWithRunLoop(device: Ref, runloop: Ref, mode: Ref);
    pub fn IOHIDDeviceUnscheduleFromRunLoop(device: Ref, runloop: Ref, mode: Ref);
    pub fn IOHIDDeviceRegisterInputReportCallback(
        device: Ref,
        report: *mut u8,
        length: isize,
        callback: Option<ReportCallback>,
        context: *mut c_void,
    );
    pub fn IOHIDDeviceRegisterRemovalCallback(
        device: Ref,
        callback: Option<HidCallback>,
        context: *mut c_void,
    );
    pub fn IOHIDDeviceSetReport(
        device: Ref,
        report_type: u32,
        report_id: isize,
        report: *const u8,
        length: isize,
    ) -> i32;
}
