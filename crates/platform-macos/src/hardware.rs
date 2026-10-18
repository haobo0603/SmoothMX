//! Serialized HID++ transport and reversible customization session.
use super::*;
use smoothmx_core::input::{CONTROL_IDS, InputEngine, Output, Settings};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

pub type OutputCallback = unsafe extern "C" fn(*mut c_void, Output);
#[derive(Clone, Copy)]
struct Packet {
    bytes: [u8; 20],
    len: usize,
}
struct Rx {
    expected: Cell<Option<(u8, u8)>>,
    response: RefCell<Option<Result<[u8; 16], String>>>,
    controls: Cell<u8>,
    vertical: Cell<u8>,
    horizontal: Cell<u8>,
    vertical_multiplier: Cell<f64>,
    horizontal_multiplier: Cell<f64>,
    engine: RefCell<InputEngine>,
    output: Option<OutputCallback>,
    context: usize,
    toggle: Cell<bool>,
}
impl Rx {
    fn emit(&self, event: Output) {
        if event.kind == 5 {
            self.toggle.set(true);
        } else if let Some(callback) = self.output {
            unsafe { callback(self.context as *mut c_void, event) }
        }
    }
    fn packet(&self, p: &[u8]) {
        if p.len() < 7 || ![0x10, 0x11].contains(&p[0]) || p[1] != 0xff {
            return;
        }
        if let Some((feature, function)) = self.expected.get() {
            if p[2] == feature && p[3] == function {
                let mut response = [0; 16];
                let n = (p.len() - 4).min(16);
                response[..n].copy_from_slice(&p[4..4 + n]);
                *self.response.borrow_mut() = Some(Ok(response));
                return;
            }
            if p[2] == 0xff && p[3] == feature && p[4] == function {
                *self.response.borrow_mut() = Some(Err(format!("HID++ error 0x{:02X}", p[5])));
                return;
            }
        }
        if p[3] & 15 != 0 {
            return;
        }
        if p[2] == self.controls.get() && self.controls.get() != 0 {
            if p[3] >> 4 == 0 && p.len() >= 12 {
                let ids = [
                    u16::from_be_bytes([p[4], p[5]]),
                    u16::from_be_bytes([p[6], p[7]]),
                    u16::from_be_bytes([p[8], p[9]]),
                    u16::from_be_bytes([p[10], p[11]]),
                ];
                self.engine.borrow_mut().buttons(&ids, |e| self.emit(e));
            } else if p[3] >> 4 == 1 && p.len() >= 8 {
                self.engine.borrow_mut().motion(
                    i16::from_be_bytes([p[4], p[5]]),
                    i16::from_be_bytes([p[6], p[7]]),
                );
            }
        } else if p[2] == self.vertical.get() && self.vertical.get() != 0 && p[3] == 0 {
            let multiplier = if p[4] & 0x10 != 0 {
                self.vertical_multiplier.get()
            } else {
                1.0
            };
            self.engine.borrow_mut().wheel(
                false,
                i16::from_be_bytes([p[5], p[6]]),
                multiplier,
                |e| self.emit(e),
            );
        } else if p[2] == self.horizontal.get() && self.horizontal.get() != 0 && p[3] == 0 {
            self.engine.borrow_mut().wheel(
                true,
                i16::from_be_bytes([p[4], p[5]]),
                self.horizontal_multiplier.get(),
                |e| self.emit(e),
            );
        }
    }
}
unsafe extern "C" fn report(
    context: *mut c_void,
    result: i32,
    _: Ref,
    _: u32,
    report_id: u32,
    bytes: *mut u8,
    len: isize,
) {
    if result != 0 || bytes.is_null() || !(1..=128).contains(&len) {
        return;
    }
    let rx = unsafe { &*context.cast::<Rx>() };
    let data = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    if data[0] == report_id as u8 {
        rx.packet(data);
    } else if [0x10, 0x11].contains(&report_id) && data.len() < 20 {
        let mut p = [0u8; 20];
        p[0] = report_id as u8;
        p[1..=data.len()].copy_from_slice(data);
        rx.packet(&p[..data.len() + 1]);
    }
}
#[derive(Clone, Copy)]
struct Button {
    id: u16,
    flags: u8,
    raw: bool,
    reporting: [u8; 16],
}
struct Capabilities {
    dpi: u8,
    smart: u8,
    vertical: u8,
    horizontal: u8,
    controls: u8,
    battery: u8,
    dpi_values: Vec<u16>,
    current_dpi: u16,
    smart_state: [u8; 16],
    vertical_state: u8,
    horizontal_state: [u8; 2],
    buttons: Vec<Button>,
}

/// Battery level read from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryInfo {
    /// Percentage (0–100), or 0 if unreadable.
    pub level_percent: u8,
    /// true when the device is connected to a power source.
    pub charging: bool,
}
pub struct Session {
    device: Owned,
    _manager: Owned,
    rx: Box<Rx>,
    buffer: Box<[u8; 128]>,
    sequence: Cell<u8>,
    caps: Option<Capabilities>,
    changed: bool,
}
impl Session {
    fn open(
        id: u64,
        output: Option<OutputCallback>,
        context: usize,
        _control: Arc<Mutex<HardwareControl>>,
    ) -> Result<Self, String> {
        let manager = manager()?;
        let device=interfaces(manager.0).into_iter().find(|(_,d)|d.registry_id==id && d.transport.contains("Bluetooth"))
            .map(|(p,_)|p as Ref).ok_or("Selected Bluetooth device is no longer connected. Receiver settings are not supported yet.")?;
        let device = Owned(unsafe { CFRetain(device) });
        let status = unsafe { IOHIDDeviceOpen(device.0, 0) };
        if status != 0 {
            return Err(format!(
                "macOS denied HID access (0x{:08X}). Check Input Monitoring permission and close other mouse configuration tools.",
                status as u32
            ));
        }
        let rx = Box::new(Rx {
            expected: Cell::new(None),
            response: RefCell::new(None),
            controls: Cell::new(0),
            vertical: Cell::new(0),
            horizontal: Cell::new(0),
            vertical_multiplier: Cell::new(1.0),
            horizontal_multiplier: Cell::new(1.0),
            engine: RefCell::new(InputEngine::default()),
            output,
            context,
            toggle: Cell::new(false),
        });
        let mut result = Self {
            device,
            _manager: manager,
            rx,
            buffer: Box::new([0; 128]),
            sequence: Cell::new(0),
            caps: None,
            changed: false,
        };
        unsafe {
            IOHIDDeviceRegisterInputReportCallback(
                result.device.0,
                result.buffer.as_mut_ptr(),
                128,
                Some(report),
                (&*result.rx as *const Rx).cast_mut().cast(),
            );
            IOHIDDeviceScheduleWithRunLoop(
                result.device.0,
                CFRunLoopGetCurrent(),
                kCFRunLoopDefaultMode,
            );
        }
        Ok(result)
    }
    fn request(&self, feature: u8, function: u8, parameters: &[u8]) -> Result<[u8; 16], String> {
        if parameters.len() > 16 {
            return Err("HID++ request too large".into());
        }
        let sequence = (self.sequence.get() % 15) + 1;
        self.sequence.set(sequence);
        let function = (function << 4) | sequence;
        let mut packet = Packet {
            bytes: [0; 20],
            len: 20,
        };
        packet.bytes[..4].copy_from_slice(&[0x11, 0xff, feature, function]);
        packet.bytes[4..4 + parameters.len()].copy_from_slice(parameters);
        self.rx.expected.set(Some((feature, function)));
        self.rx.response.borrow_mut().take();
        let status = unsafe {
            IOHIDDeviceSetReport(
                self.device.0,
                1,
                0x11,
                packet.bytes.as_ptr(),
                packet.len as isize,
            )
        };
        if status != 0 {
            self.rx.expected.set(None);
            return Err(format!("HID write failed (0x{:08X})", status as u32));
        }
        let deadline = Instant::now() + Duration::from_millis(450);
        let result = loop {
            if let Some(reply) = self.rx.response.borrow_mut().take() {
                break reply;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break Err(format!(
                    "Mouse did not answer feature {feature:02X}, function {:X}",
                    function >> 4
                ));
            }
            unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, remaining.as_secs_f64(), 1);
            }
        };
        self.rx.expected.set(None);
        result
    }
    fn feature(&self, id: u16) -> Result<u8, String> {
        Ok(self.request(0, 0, &id.to_be_bytes())?[0])
    }
    fn probe(&mut self) -> Result<(), String> {
        let dpi = self.feature(0x2201)?;
        let smart = self.feature(0x2110)?;
        let vertical = self.feature(0x2121)?;
        let horizontal = self.feature(0x2150)?;
        let controls = self.feature(0x1b04)?;
        let battery = self.feature(0x0015)?;
        let mut caps = Capabilities {
            dpi,
            smart,
            vertical,
            horizontal,
            controls,
            battery,
            dpi_values: vec![],
            current_dpi: 0,
            smart_state: [0; 16],
            vertical_state: 0,
            horizontal_state: [0; 2],
            buttons: vec![],
        };
        if dpi != 0 {
            caps.dpi_values = decode_dpi(&self.request(dpi, 1, &[0])?)?;
            let p = self.request(dpi, 2, &[0])?;
            caps.current_dpi = u16::from_be_bytes([p[1], p[2]]);
        }
        if smart != 0 {
            caps.smart_state = self.request(smart, 0, &[])?;
        }
        if vertical != 0 {
            self.rx
                .vertical_multiplier
                .set(self.request(vertical, 0, &[])?[0].max(1) as f64);
            caps.vertical_state = self.request(vertical, 1, &[])?[0];
        }
        if horizontal != 0 {
            let info = self.request(horizontal, 0, &[])?;
            let native = u16::from_be_bytes([info[0], info[1]]).max(1);
            let diverted = u16::from_be_bytes([info[2], info[3]]).max(1);
            self.rx
                .horizontal_multiplier
                .set(diverted as f64 / native as f64);
            let p = self.request(horizontal, 1, &[])?;
            caps.horizontal_state = [p[0], p[1] & 1];
        }
        if controls != 0 {
            let count = self.request(controls, 0, &[])?[0].min(64);
            for i in 0..count {
                let p = self.request(controls, 1, &[i])?;
                let id = u16::from_be_bytes([p[0], p[1]]);
                if CONTROL_IDS.contains(&id) {
                    caps.buttons.push(Button {
                        id,
                        flags: p[4],
                        raw: p[8] & 1 != 0,
                        reporting: self.request(controls, 2, &id.to_be_bytes())?,
                    });
                }
            }
        }
        self.rx.controls.set(controls);
        self.rx.vertical.set(vertical);
        self.rx.horizontal.set(horizontal);
        self.caps = Some(caps);
        Ok(())
    }
    /// Read battery level and charging status from the device.
    pub fn read_battery(&self) -> Option<BatteryInfo> {
        let c = self.caps.as_ref()?;
        if c.battery == 0 {
            return None;
        }
        let p = self.request(c.battery, 0, &[]).ok()?;
        // HID++ Battery feature (0x0015): byte[0] = level (0-100), byte[1] = status flags
        // Status: bit 0 = 1 when charging
        let level_percent = p[0];
        let charging = p[1] & 0x01 != 0;
        Some(BatteryInfo {
            level_percent,
            charging,
        })
    }
    fn snapshot(&self, applied: bool, error: Option<&str>) -> String {
        let Some(c) = &self.caps else {
            return format!(
                "{{\"ready\":false,\"applied\":false,\"error\":{}}}",
                error.map(json_string).unwrap_or("null".into())
            );
        };
        let buttons = c
            .buttons
            .iter()
            .map(|b| {
                format!(
                    "{{\"id\":{},\"divertable\":{},\"gesture\":{}}}",
                    b.id,
                    b.flags & 0x30 == 0x30,
                    b.raw
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"ready\":true,\"applied\":{applied},\"dpi_values\":{:?},\"dpi\":{},\"smart_shift\":{},\"wheel_mode\":{},\"threshold\":{},\"vertical\":{},\"horizontal\":{},\"buttons\":[{}],\"error\":{}}}",
            c.dpi_values,
            c.current_dpi,
            c.smart != 0,
            c.smart_state[0],
            c.smart_state[1],
            c.vertical != 0,
            c.horizontal != 0,
            buttons,
            error.map(json_string).unwrap_or("null".into())
        )
    }
    /// Return battery snapshot as JSON string.
    pub fn battery_snapshot(&self) -> String {
        let Some(c) = &self.caps else {
            return "{\"battery\":null}".to_string();
        };
        if c.battery == 0 {
            return "{\"battery\":null}".to_string();
        }
        match self.read_battery() {
            Some(bat) => format!(
                "{{\"battery\":{{\"level_percent\":{},\"charging\":{}}}}}",
                bat.level_percent, bat.charging
            ),
            None => "{\"battery\":null}".to_string(),
        }
    }
    fn apply(&mut self, settings: Settings) -> Result<(), String> {
        let c = self.caps.as_ref().ok_or("Capabilities not available")?;
        if settings.dpi != 0 && !c.dpi_values.contains(&settings.dpi) {
            return Err("DPI is not supported by this mouse".into());
        }
        if settings.vertical && c.vertical == 0 || settings.horizontal && c.horizontal == 0 {
            return Err("This mouse lacks the required wheel feature".into());
        }
        if c.smart == 0 {
            return Err("This mouse lacks the supported SmartShift feature".into());
        }
        for (i, binding) in settings.bindings.iter().enumerate() {
            if binding.enabled {
                let b = c
                    .buttons
                    .iter()
                    .find(|b| b.id == CONTROL_IDS[i])
                    .ok_or("Button not available")?;
                if b.flags & 0x30 != 0x30 || (binding.gesture && !b.raw) {
                    return Err("This button does not support the selected mapping".into());
                }
                if b.reporting[2] & 0x15 != 0 && !self.changed {
                    return Err("Another utility has diverted this button. Close it before enabling SmoothMX.".into());
                }
            }
        }
        if !self.changed
            && ((settings.vertical && c.vertical_state & 1 != 0)
                || (settings.horizontal && c.horizontal_state[0] != 0))
        {
            return Err(
                "Another utility owns wheel reporting. Close it before enabling SmoothMX.".into(),
            );
        }
        self.rx.engine.borrow_mut().release_all(|e| self.rx.emit(e));
        self.changed = true; // Track even partially applied writes for restoration.
        self.rx.engine.borrow_mut().settings = Some(settings.clone());
        let result = (|| {
            if settings.dpi != 0 {
                let d = settings.dpi.to_be_bytes();
                self.request(c.dpi, 3, &[0, d[0], d[1]])?;
                let actual = self.request(c.dpi, 2, &[0])?;
                if u16::from_be_bytes([actual[1], actual[2]]) != settings.dpi {
                    return Err("DPI readback did not match".into());
                }
            }
            self.request(c.smart, 1, &[settings.wheel_mode, settings.threshold, 0])?;
            let actual = self.request(c.smart, 0, &[])?;
            if actual[0] != settings.wheel_mode || actual[1] != settings.threshold {
                return Err("Wheel mode readback did not match".into());
            }
            if c.vertical != 0 {
                let mode = if settings.vertical {
                    if settings.smooth { 3 } else { 1 }
                } else {
                    c.vertical_state
                };
                self.request(c.vertical, 2, &[mode])?;
                if self.request(c.vertical, 1, &[])?[0] != mode {
                    return Err("Vertical wheel readback did not match".into());
                }
            }
            if c.horizontal != 0 {
                let mode = if settings.horizontal {
                    [1, 0]
                } else {
                    c.horizontal_state
                };
                self.request(c.horizontal, 2, &mode)?;
                let p = self.request(c.horizontal, 1, &[])?;
                if p[0] != mode[0] || p[1] & 1 != mode[1] {
                    return Err("Thumb wheel readback did not match".into());
                }
            }
            for (i, binding) in settings.bindings.iter().enumerate() {
                if let Some(b) = c
                    .buttons
                    .iter()
                    .find(|b| b.id == CONTROL_IDS[i] && b.flags & 0x30 == 0x30)
                {
                    let flags = if binding.enabled {
                        0x23 | if binding.gesture { 0x10 } else { 0 }
                    } else {
                        0x22 | (b.reporting[2] & 0x11)
                    };
                    let cid = b.id.to_be_bytes();
                    self.request(c.controls, 3, &[cid[0], cid[1], flags, 0, 0])?;
                    let p = self.request(c.controls, 2, &cid)?;
                    if p[2] & 0x11 != flags & 0x11 {
                        return Err("Button reporting readback did not match".into());
                    }
                }
            }
            Ok(())
        })();
        if let Err(ref e) = result {
            let cleanup = self.restore();
            if let Err(cleanup_err) = cleanup {
                return Err(format!(
                    "{}; restoration also failed: {cleanup_err}. Power-cycle the mouse.",
                    e
                ));
            }
        }
        result
    }
    fn restore(&mut self) -> Result<(), String> {
        if !self.changed {
            return Ok(());
        }
        let c = self.caps.as_ref().ok_or("No restore snapshot")?;
        let mut errors = vec![];
        for b in &c.buttons {
            if b.flags & 0x30 == 0x30 {
                let cid = b.id.to_be_bytes();
                if let Err(e) = self.request(
                    c.controls,
                    3,
                    &[cid[0], cid[1], 0x22 | (b.reporting[2] & 0x11), 0, 0],
                ) {
                    errors.push(e);
                }
            }
        }
        if c.vertical != 0
            && let Err(e) = self.request(c.vertical, 2, &[c.vertical_state])
        {
            errors.push(e);
        }
        if c.horizontal != 0
            && let Err(e) = self.request(c.horizontal, 2, &c.horizontal_state)
        {
            errors.push(e);
        }
        if c.smart != 0
            && let Err(e) = self.request(c.smart, 1, &[c.smart_state[0], c.smart_state[1], 0])
        {
            errors.push(e);
        }
        if c.dpi != 0 {
            let d = c.current_dpi.to_be_bytes();
            if let Err(e) = self.request(c.dpi, 3, &[0, d[0], d[1]]) {
                errors.push(e);
            }
        }
        self.rx.engine.borrow_mut().release_all(|e| self.rx.emit(e));
        self.rx.engine.borrow_mut().settings = None;
        self.changed = !errors.is_empty();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            IOHIDDeviceUnscheduleFromRunLoop(
                self.device.0,
                CFRunLoopGetCurrent(),
                kCFRunLoopDefaultMode,
            );
            IOHIDDeviceRegisterInputReportCallback(
                self.device.0,
                ptr::null_mut(),
                0,
                None,
                ptr::null_mut(),
            );
            IOHIDDeviceClose(self.device.0, 0);
        }
    }
}
fn decode_dpi(p: &[u8; 16]) -> Result<Vec<u16>, String> {
    let values = p[1..13]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_be_bytes([c[0], c[1]]))
        .take_while(|v| *v != 0)
        .collect::<Vec<_>>();
    if values.len() == 3 && values[1] > 0xe000 {
        let step = values[1] & 0x1fff;
        if values[0] > values[2] || values[2] >= 0xe000 {
            return Err("Invalid DPI range".into());
        }
        return Ok((values[0]..=values[2]).step_by(step as usize).collect());
    }
    if values.is_empty() || values.iter().any(|v| *v >= 0xe000) {
        return Err("Unsupported DPI list format".into());
    }
    Ok(values)
}
pub fn probe(id: u64) -> Result<String, String> {
    let mut s = Session::open(
        id,
        None,
        0,
        Arc::new(Mutex::new(HardwareControl::default())),
    )?;
    s.probe()?;
    Ok(s.snapshot(false, None))
}

/// Read battery information from a device by registry ID.
pub fn battery(id: u64) -> Result<String, String> {
    let mut s = Session::open(
        id,
        None,
        0,
        Arc::new(Mutex::new(HardwareControl::default())),
    )?;
    s.probe()?;
    Ok(s.battery_snapshot())
}

#[derive(Default)]
struct HardwareControl {
    stopped: bool,
    runloop: usize,
    source: usize,
    pending: Option<Option<Settings>>,
}
pub struct HardwareHandle {
    control: Arc<Mutex<HardwareControl>>,
    worker: Option<thread::JoinHandle<()>>,
}
unsafe extern "C" fn wake(_: *mut c_void) {}
fn hardware_run(
    id: u64,
    control: Arc<Mutex<HardwareControl>>,
    callback: SnapshotCallback,
    output: OutputCallback,
    context: usize,
) {
    let mut session = match Session::open(id, Some(output), context, Arc::clone(&control)) {
        Ok(mut s) => {
            if let Err(e) = s.probe() {
                emit(callback, context, &s.snapshot(false, Some(&e)));
                return;
            }
            s
        }
        Err(e) => {
            emit(
                callback,
                context,
                &format!(
                    "{{\"ready\":false,\"applied\":false,\"error\":{}}}",
                    json_string(&e)
                ),
            );
            return;
        }
    };
    emit(callback, context, &session.snapshot(false, None));
    unsafe {
        let runloop = CFRunLoopGetCurrent();
        let mut ctx = SourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            description: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(wake),
        };
        let source = Owned(CFRunLoopSourceCreate(ptr::null(), 0, &mut ctx));
        if source.0.is_null() {
            emit(
                callback,
                context,
                &session.snapshot(false, Some("Cannot create settings control source")),
            );
            return;
        }
        CFRunLoopAddSource(runloop, source.0, kCFRunLoopDefaultMode);
        {
            let mut c = control.lock().unwrap_or_else(|p| p.into_inner());
            c.runloop = runloop as usize;
            c.source = source.0 as usize;
        }
        loop {
            let (stopped, pending) = {
                let mut c = control.lock().unwrap_or_else(|p| p.into_inner());
                (c.stopped, c.pending.take())
            };
            if stopped {
                break;
            }
            if let Some(settings) = pending {
                let applied = settings.is_some();
                let result = if let Some(s) = settings {
                    session.apply(s)
                } else {
                    session.restore()
                };
                emit(
                    callback,
                    context,
                    &session.snapshot(
                        applied && result.is_ok(),
                        result.as_ref().err().map(String::as_str),
                    ),
                );
            }
            if session.rx.toggle.replace(false)
                && let Some(c) = &session.caps
                && c.smart != 0
            {
                let result = session.request(c.smart, 0, &[]).and_then(|p| {
                    session.request(c.smart, 1, &[if p[0] == 1 { 2 } else { 1 }, 0, 0])
                });
                if let Err(e) = result {
                    emit(
                        callback,
                        context,
                        &session.snapshot(session.changed, Some(&e)),
                    );
                }
            }
            // Source is persistent: a signal before the wait is not lost.
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0e9, 1);
        }
        let result = session.restore();
        if let Err(e) = result {
            emit(
                callback,
                context,
                &session.snapshot(
                    false,
                    Some(&format!("Restore failed: {e}. Power-cycle the mouse.")),
                ),
            );
        }
        {
            let mut c = control.lock().unwrap_or_else(|p| p.into_inner());
            c.runloop = 0;
            c.source = 0;
        }
        CFRunLoopRemoveSource(runloop, source.0, kCFRunLoopDefaultMode);
    }
}
/// # Safety
/// Callbacks/context must remain valid until stop returns; callbacks must not stop the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_hardware_start(
    id: u64,
    callback: SnapshotCallback,
    output: OutputCallback,
    context: *mut c_void,
) -> *mut HardwareHandle {
    let control = Arc::new(Mutex::new(HardwareControl::default()));
    let c = Arc::clone(&control);
    let context = context as usize;
    match thread::Builder::new()
        .name("smoothmx-settings".into())
        .spawn(move || hardware_run(id, c, callback, output, context))
    {
        Ok(worker) => Box::into_raw(Box::new(HardwareHandle {
            control,
            worker: Some(worker),
        })),
        Err(_) => ptr::null_mut(),
    }
}
fn signal(c: &HardwareControl) {
    if c.source != 0 {
        unsafe {
            CFRunLoopSourceSignal(c.source as Ref);
            CFRunLoopWakeUp(c.runloop as Ref);
        }
    }
}
/// # Safety
/// Live handle and valid array of count words required. Null words restores defaults captured at session start.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_hardware_apply(
    handle: *mut HardwareHandle,
    words: *const u32,
    count: usize,
) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let settings = if words.is_null() {
        None
    } else {
        if count != Settings::WORDS {
            return -2;
        }
        match Settings::decode(unsafe { std::slice::from_raw_parts(words, count) }) {
            Ok(s) => Some(s),
            Err(_) => return -2,
        }
    };
    let h = unsafe { &*handle };
    let mut c = h.control.lock().unwrap_or_else(|p| p.into_inner());
    if c.stopped {
        return -1;
    }
    c.pending = Some(settings);
    signal(&c);
    0
}
/// # Safety
/// Consumes the handle exactly once; do not call from a callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_hardware_stop(handle: *mut HardwareHandle) {
    if handle.is_null() {
        return;
    }
    let mut h = unsafe { Box::from_raw(handle) };
    {
        let mut c = h.control.lock().unwrap_or_else(|p| p.into_inner());
        c.stopped = true;
        signal(&c);
    }
    if let Some(worker) = h.worker.take() {
        let _ = worker.join();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpi_range() {
        let mut p = [0; 16];
        p[1..7].copy_from_slice(&[0, 200, 0xe0, 50, 0x0f, 0xa0]);
        let list = decode_dpi(&p).unwrap();
        assert_eq!(list.first(), Some(&200));
        assert_eq!(list.last(), Some(&4000));
    }
    #[test]
    fn rejects_invalid_dpi_step() {
        let mut p = [0; 16];
        p[1..7].copy_from_slice(&[0, 200, 0xe0, 0, 0x0f, 0xa0]);
        assert!(decode_dpi(&p).is_err());
    }
    #[test]
    fn battery_info_struct_clippy() {
        let bat = BatteryInfo {
            level_percent: 75,
            charging: true,
        };
        assert_eq!(bat.level_percent, 75);
        assert!(bat.charging);
    }
}
