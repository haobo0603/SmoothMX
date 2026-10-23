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

const HIDPP_SHORT_REPORT: u8 = 0x10;
const HIDPP_LONG_REPORT: u8 = 0x11;
const HIDPP_SHORT_LENGTH: usize = 7;
const HIDPP_LONG_LENGTH: usize = 20;

fn normalize_report(report_id: u32, data: &[u8]) -> Option<Packet> {
    let report_id = u8::try_from(report_id).ok()?;
    let expected = match report_id {
        HIDPP_SHORT_REPORT => HIDPP_SHORT_LENGTH,
        HIDPP_LONG_REPORT => HIDPP_LONG_LENGTH,
        _ => return None,
    };
    let mut packet = Packet {
        bytes: [0; HIDPP_LONG_LENGTH],
        len: expected,
    };
    if data.len() == expected && data.first() == Some(&report_id) {
        packet.bytes[..expected].copy_from_slice(data);
    } else if data.len() == expected - 1 {
        packet.bytes[0] = report_id;
        packet.bytes[1..expected].copy_from_slice(data);
    } else {
        return None;
    }
    Some(packet)
}

fn encode_request(
    feature: u8,
    function: u8,
    sequence: u8,
    parameters: &[u8],
) -> Result<Packet, String> {
    if function > 0x0f {
        return Err("HID++ function index is out of range".into());
    }
    if !(1..=0x0f).contains(&sequence) {
        return Err("HID++ software ID is out of range".into());
    }
    if parameters.len() > HIDPP_LONG_LENGTH - 4 {
        return Err("HID++ request too large".into());
    }
    let mut packet = Packet {
        bytes: [0; HIDPP_LONG_LENGTH],
        len: HIDPP_LONG_LENGTH,
    };
    packet.bytes[..4].copy_from_slice(&[
        HIDPP_LONG_REPORT,
        0xff,
        feature,
        (function << 4) | sequence,
    ]);
    packet.bytes[4..4 + parameters.len()].copy_from_slice(parameters);
    Ok(packet)
}

struct Rx {
    expected: Cell<Option<(u8, u8)>>,
    response: RefCell<Option<Result<[u8; 16], String>>>,
    controls: Cell<u8>,
    vertical: Cell<u8>,
    horizontal: Cell<u8>,
    battery_unified: Cell<u8>,
    battery_legacy: Cell<u8>,
    battery_update: Cell<Option<BatteryInfo>>,
    vertical_multiplier: Cell<f64>,
    horizontal_multiplier: Cell<f64>,
    engine: RefCell<InputEngine>,
    output: Option<OutputCallback>,
    context: usize,
    toggle: Cell<bool>,
    disconnected: Cell<bool>,
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
        if !matches!(p.len(), HIDPP_SHORT_LENGTH | HIDPP_LONG_LENGTH)
            || ![HIDPP_SHORT_REPORT, HIDPP_LONG_REPORT].contains(&p[0])
            || p[1] != 0xff
        {
            return;
        }
        if let Some((feature, function)) = self.expected.get() {
            if p[2] == feature && p[3] == function {
                let mut response = [0; 16];
                let n = (p.len() - 4).min(16);
                response[..n].copy_from_slice(&p[4..4 + n]);
                self.complete(Ok(response));
                return;
            }
            if p[2] == 0xff && p[3] == feature && p[4] == function {
                self.complete(Err(format!("HID++ error 0x{:02X}", p[5])));
                return;
            }
        }
        if p[3] & 15 != 0 {
            return;
        }
        if p[2] == self.battery_unified.get() && self.battery_unified.get() != 0 {
            let mut status = [0; 16];
            let count = (p.len() - 4).min(status.len());
            status[..count].copy_from_slice(&p[4..4 + count]);
            if let Some(update) = parse_unified_battery(&status) {
                self.battery_update.set(Some(update));
            }
        } else if p[2] == self.battery_legacy.get() && self.battery_legacy.get() != 0 {
            let mut status = [0; 16];
            let count = (p.len() - 4).min(status.len());
            status[..count].copy_from_slice(&p[4..4 + count]);
            if let Some(update) = parse_legacy_battery(&status) {
                self.battery_update.set(Some(update));
            }
        } else if p[2] == self.controls.get() && self.controls.get() != 0 {
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

    fn complete(&self, result: Result<[u8; 16], String>) {
        let mut response = self.response.borrow_mut();
        if response.is_none() {
            *response = Some(result);
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
    if context.is_null() {
        return;
    }
    let rx = unsafe { &*context.cast::<Rx>() };
    if result != 0 {
        if rx.expected.get().is_some() {
            rx.complete(Err(format!("HID input failed (0x{:08X})", result as u32)));
        }
        return;
    }
    if bytes.is_null() || !(1..=128).contains(&len) {
        return;
    }
    let data = unsafe { std::slice::from_raw_parts(bytes, len as usize) };
    if let Some(packet) = normalize_report(report_id, data) {
        rx.packet(&packet.bytes[..packet.len]);
    }
}

unsafe extern "C" fn device_removed(context: *mut c_void, _: i32, _: Ref) {
    if context.is_null() {
        return;
    }
    let rx = unsafe { &*context.cast::<Rx>() };
    rx.disconnected.set(true);
    if rx.expected.get().is_some() {
        rx.complete(Err(DEVICE_DISCONNECTED.into()));
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
    battery: BatteryFeatures,
    dpi_values: Vec<u16>,
    current_dpi: u16,
    smart_state: [u8; 16],
    vertical_state: u8,
    horizontal_state: [u8; 2],
    buttons: Vec<Button>,
}

#[derive(Clone, Copy, Default)]
struct BatteryFeatures {
    unified: u8,
    legacy: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatteryFeature {
    Unified(u8),
    Legacy(u8),
}

fn battery_feature_order(features: BatteryFeatures) -> [Option<BatteryFeature>; 2] {
    [
        (features.unified != 0).then_some(BatteryFeature::Unified(features.unified)),
        (features.legacy != 0).then_some(BatteryFeature::Legacy(features.legacy)),
    ]
}

/// Battery level read from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryInfo {
    /// Percentage (0–100). Zero is a valid reading.
    pub level_percent: u8,
    /// `true` while the device reports an active charging state.
    pub charging: bool,
}

fn parse_unified_battery(status: &[u8; 16]) -> Option<BatteryInfo> {
    // 0x1004 getBatteryStatus directly returns percentage, level flags,
    // charging state, and external-power state in bytes 0..=3. Some devices,
    // including MX Master variants, do not advertise the separate capability
    // bit consistently even though this status response contains a valid SOC.
    if status[0] > 100 || status[2] > 7 {
        return None;
    }
    Some(BatteryInfo {
        level_percent: status[0],
        charging: matches!(status[2], 1 | 2 | 4),
    })
}

fn parse_legacy_battery(status: &[u8; 16]) -> Option<BatteryInfo> {
    // 0x1000 getBatteryLevelStatus returns percentage, next percentage, and
    // charging state in bytes 0..=2. Values 1, 2, and 4 are charging states.
    if status[0] > 100 || status[2] > 7 {
        return None;
    }
    Some(BatteryInfo {
        level_percent: status[0],
        charging: matches!(status[2], 1 | 2 | 4),
    })
}

const DEVICE_DISCONNECTED: &str = "Mouse disconnected. Reconnect it before applying settings.";
const REQUEST_TIMEOUT: Duration = Duration::from_millis(450);
const REQUEST_WAIT_SLICE: Duration = Duration::from_millis(20);
const BATTERY_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestInterruption {
    Stopped,
    Disconnected,
}

impl RequestInterruption {
    fn message(self) -> &'static str {
        match self {
            Self::Stopped => "Hardware session stopped",
            Self::Disconnected => DEVICE_DISCONNECTED,
        }
    }
}

fn request_interruption(
    stopped: bool,
    disconnected: bool,
    allow_stopped: bool,
) -> Option<RequestInterruption> {
    if disconnected {
        Some(RequestInterruption::Disconnected)
    } else if stopped && !allow_stopped {
        Some(RequestInterruption::Stopped)
    } else {
        None
    }
}

fn smart_shift_update(feature: u8, wheel_mode: u8, threshold: u8) -> Option<(u8, [u8; 3])> {
    (feature != 0).then_some((feature, [wheel_mode, threshold, 0]))
}

fn settings_uses_wheel_toggle(settings: &Settings) -> bool {
    (settings.horizontal
        && settings
            .horizontal_actions
            .iter()
            .any(|action| action.kind == 5))
        || settings
            .bindings
            .iter()
            .filter(|binding| binding.enabled)
            .any(|binding| {
                if binding.gesture {
                    binding.actions.iter().any(|action| action.kind == 5)
                } else {
                    binding.actions[0].kind == 5
                }
            })
}

pub struct Session {
    device: Owned,
    _manager: Owned,
    rx: Box<Rx>,
    buffer: Box<[u8; 128]>,
    sequence: Cell<u8>,
    caps: Option<Capabilities>,
    battery: Option<BatteryInfo>,
    changed: bool,
    control: Arc<Mutex<HardwareControl>>,
}
impl Session {
    fn open(
        id: u64,
        output: Option<OutputCallback>,
        context: usize,
        control: Arc<Mutex<HardwareControl>>,
    ) -> Result<Self, String> {
        let manager = manager()?;
        let (device, battery_level) = interfaces(manager.0)
            .into_iter()
            .find(|(_, record)| {
                record.registry_id == id && record.transport.contains("Bluetooth")
            })
            .map(|(device, record)| (device as Ref, record.battery_level))
            .ok_or(
                "Selected Bluetooth device is no longer connected. Receiver settings are not supported yet.",
            )?;
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
            battery_unified: Cell::new(0),
            battery_legacy: Cell::new(0),
            battery_update: Cell::new(None),
            vertical_multiplier: Cell::new(1.0),
            horizontal_multiplier: Cell::new(1.0),
            engine: RefCell::new(InputEngine::default()),
            output,
            context,
            toggle: Cell::new(false),
            disconnected: Cell::new(false),
        });
        let mut result = Self {
            device,
            _manager: manager,
            rx,
            buffer: Box::new([0; 128]),
            sequence: Cell::new(0),
            caps: None,
            battery: battery_level.map(|level_percent| BatteryInfo {
                level_percent,
                charging: false,
            }),
            changed: false,
            control,
        };
        unsafe {
            let rx_context = (&*result.rx as *const Rx).cast_mut().cast();
            IOHIDDeviceRegisterInputReportCallback(
                result.device.0,
                result.buffer.as_mut_ptr(),
                128,
                Some(report),
                rx_context,
            );
            IOHIDDeviceRegisterRemovalCallback(result.device.0, Some(device_removed), rx_context);
            IOHIDDeviceScheduleWithRunLoop(
                result.device.0,
                CFRunLoopGetCurrent(),
                kCFRunLoopDefaultMode,
            );
        }
        Ok(result)
    }
    fn request(&self, feature: u8, function: u8, parameters: &[u8]) -> Result<[u8; 16], String> {
        self.request_with_stop_policy(feature, function, parameters, false)
    }
    fn request_during_shutdown(
        &self,
        feature: u8,
        function: u8,
        parameters: &[u8],
    ) -> Result<[u8; 16], String> {
        self.request_with_stop_policy(feature, function, parameters, true)
    }
    fn interruption(&self, allow_stopped: bool) -> Option<RequestInterruption> {
        let stopped = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stopped;
        request_interruption(stopped, self.rx.disconnected.get(), allow_stopped)
    }
    fn request_with_stop_policy(
        &self,
        feature: u8,
        function: u8,
        parameters: &[u8],
        allow_stopped: bool,
    ) -> Result<[u8; 16], String> {
        if let Some(interruption) = self.interruption(allow_stopped) {
            return Err(interruption.message().into());
        }
        let sequence = (self.sequence.get() % 15) + 1;
        let packet = encode_request(feature, function, sequence, parameters)?;
        self.sequence.set(sequence);
        let correlated_function = packet.bytes[3];
        self.rx.expected.set(Some((feature, correlated_function)));
        self.rx.response.borrow_mut().take();
        let status = unsafe {
            IOHIDDeviceSetReport(
                self.device.0,
                1,
                HIDPP_LONG_REPORT as isize,
                packet.bytes.as_ptr(),
                packet.len as isize,
            )
        };
        if status != 0 {
            self.rx.expected.set(None);
            if let Some(interruption) = self.interruption(allow_stopped) {
                return Err(interruption.message().into());
            }
            return Err(format!("HID write failed (0x{:08X})", status as u32));
        }
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let result = loop {
            if let Some(interruption) = self.interruption(allow_stopped) {
                break Err(interruption.message().into());
            }
            if let Some(reply) = self.rx.response.borrow_mut().take() {
                break reply;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break Err(format!(
                    "Mouse did not answer feature {feature:02X}, function {:X}",
                    correlated_function >> 4
                ));
            }
            let wait = remaining.min(REQUEST_WAIT_SLICE);
            unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, wait.as_secs_f64(), 1);
            }
        };
        self.rx.expected.set(None);
        result
    }
    fn feature(&self, id: u16) -> Result<u8, String> {
        Ok(self.request(0, 0, &id.to_be_bytes())?[0])
    }
    fn initialize_battery(&mut self) -> BatteryFeatures {
        let features = BatteryFeatures {
            unified: self.feature(0x1004).unwrap_or(0),
            legacy: self.feature(0x1000).unwrap_or(0),
        };
        self.rx.battery_unified.set(features.unified);
        self.rx.battery_legacy.set(features.legacy);
        if let Some(battery) = self.read_battery_features(features) {
            self.battery = Some(battery);
        }
        features
    }
    fn probe(&mut self) -> Result<(), String> {
        // Read battery first so it remains available even if an unrelated
        // customization feature cannot be probed on this device.
        let battery = self.initialize_battery();
        let dpi = self.feature(0x2201)?;
        let smart = self.feature(0x2110)?;
        let vertical = self.feature(0x2121)?;
        let horizontal = self.feature(0x2150)?;
        let controls = self.feature(0x1b04)?;
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
        if let Some(interruption) = self.interruption(false) {
            return Err(interruption.message().into());
        }
        Ok(())
    }
    /// Read battery level and charging status from the device.
    pub fn read_battery(&self) -> Option<BatteryInfo> {
        let features = self.caps.as_ref()?.battery;
        self.read_battery_features(features)
    }
    fn read_battery_features(&self, features: BatteryFeatures) -> Option<BatteryInfo> {
        for feature in battery_feature_order(features).into_iter().flatten() {
            let battery =
                match feature {
                    BatteryFeature::Unified(index) => self
                        .request(index, 1, &[])
                        .ok()
                        .as_ref()
                        .and_then(parse_unified_battery),
                    BatteryFeature::Legacy(index) => self
                        .request(index, 0, &[])
                        .ok()
                        .as_ref()
                        .and_then(parse_legacy_battery),
                };
            if battery.is_some() {
                return battery;
            }
        }
        optional_number(self.device.0, "BatteryLevel")
            .and_then(valid_battery_level)
            .map(|level_percent| BatteryInfo {
                level_percent,
                charging: false,
            })
    }
    fn refresh_battery(&mut self) -> bool {
        let Some(battery) = self.read_battery() else {
            return false;
        };
        if self.battery == Some(battery) {
            return false;
        }
        self.battery = Some(battery);
        true
    }
    fn take_battery_update(&mut self) -> bool {
        let Some(battery) = self.rx.battery_update.take() else {
            return false;
        };
        if self.battery == Some(battery) {
            return false;
        }
        self.battery = Some(battery);
        true
    }
    fn disconnected(&self) -> bool {
        self.rx.disconnected.get()
    }
    fn disconnected_snapshot(&self) -> String {
        format!(
            "{{\"ready\":false,\"applied\":false,\"battery\":{},\"error\":{}}}",
            self.battery_json(),
            json_string(DEVICE_DISCONNECTED)
        )
    }
    fn unavailable_snapshot(&self, error: &str) -> String {
        format!(
            "{{\"ready\":false,\"applied\":false,\"battery\":{},\"error\":{}}}",
            self.battery_json(),
            json_string(error)
        )
    }
    fn snapshot(&self, applied: bool, error: Option<&str>) -> String {
        let battery = self.battery_json();
        let Some(c) = &self.caps else {
            return format!(
                "{{\"ready\":false,\"applied\":false,\"battery\":{battery},\"error\":{}}}",
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
            "{{\"ready\":true,\"applied\":{applied},\"dpi_values\":{:?},\"dpi\":{},\"smart_shift\":{},\"wheel_mode\":{},\"threshold\":{},\"vertical\":{},\"horizontal\":{},\"buttons\":[{}],\"battery\":{battery},\"error\":{}}}",
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
    fn battery_json(&self) -> String {
        match self.battery {
            Some(battery) => format!(
                "{{\"level_percent\":{},\"charging\":{}}}",
                battery.level_percent, battery.charging
            ),
            None => "null".into(),
        }
    }
    /// Return battery snapshot as JSON string.
    pub fn battery_snapshot(&self) -> String {
        format!("{{\"battery\":{}}}", self.battery_json())
    }
    fn apply(&mut self, settings: Settings) -> Result<(), String> {
        let c = self.caps.as_ref().ok_or("Capabilities not available")?;
        if settings.dpi != 0 && !c.dpi_values.contains(&settings.dpi) {
            return Err("DPI is not supported by this mouse".into());
        }
        if settings.vertical && c.vertical == 0 || settings.horizontal && c.horizontal == 0 {
            return Err("This mouse lacks the required wheel feature".into());
        }
        if c.smart == 0 && settings_uses_wheel_toggle(&settings) {
            return Err("This mouse cannot run a binding that toggles SmartShift".into());
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
            if let Some((feature, parameters)) =
                smart_shift_update(c.smart, settings.wheel_mode, settings.threshold)
            {
                self.request(feature, 1, &parameters)?;
                let actual = self.request(feature, 0, &[])?;
                if actual[0] != settings.wheel_mode || actual[1] != settings.threshold {
                    return Err("Wheel mode readback did not match".into());
                }
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
        self.restore_with_stop_policy(false)
    }
    fn restore_during_shutdown(&mut self) -> Result<(), String> {
        self.restore_with_stop_policy(true)
    }
    fn restore_request(
        &self,
        feature: u8,
        function: u8,
        parameters: &[u8],
        allow_stopped: bool,
    ) -> Result<[u8; 16], String> {
        if allow_stopped {
            self.request_during_shutdown(feature, function, parameters)
        } else {
            self.request(feature, function, parameters)
        }
    }
    fn restore_with_stop_policy(&mut self, allow_stopped: bool) -> Result<(), String> {
        self.rx
            .engine
            .borrow_mut()
            .release_all(|event| self.rx.emit(event));
        self.rx.engine.borrow_mut().settings = None;
        if !self.changed {
            return Ok(());
        }
        if let Some(interruption) = self.interruption(allow_stopped) {
            if interruption == RequestInterruption::Disconnected {
                self.changed = false;
                return Ok(());
            }
            return Err(interruption.message().into());
        }
        let c = self.caps.as_ref().ok_or("No restore snapshot")?;
        let mut errors = vec![];
        // A user-requested pause can try every independent restore. During
        // shutdown, bound the join delay: one failed request is enough to show
        // that the transport is not responding, so do not wait on every field.
        let mut abort_writes = false;
        for b in &c.buttons {
            if b.flags & 0x30 == 0x30 {
                let cid = b.id.to_be_bytes();
                if let Err(e) = self.restore_request(
                    c.controls,
                    3,
                    &[cid[0], cid[1], 0x22 | (b.reporting[2] & 0x11), 0, 0],
                    allow_stopped,
                ) {
                    errors.push(e);
                    abort_writes = allow_stopped;
                }
                if abort_writes || self.interruption(allow_stopped).is_some() {
                    break;
                }
            }
        }
        if !abort_writes
            && self.interruption(allow_stopped).is_none()
            && c.vertical != 0
            && let Err(e) = self.restore_request(c.vertical, 2, &[c.vertical_state], allow_stopped)
        {
            errors.push(e);
            abort_writes = allow_stopped;
        }
        if !abort_writes
            && self.interruption(allow_stopped).is_none()
            && c.horizontal != 0
            && let Err(e) =
                self.restore_request(c.horizontal, 2, &c.horizontal_state, allow_stopped)
        {
            errors.push(e);
            abort_writes = allow_stopped;
        }
        if !abort_writes
            && self.interruption(allow_stopped).is_none()
            && c.smart != 0
            && let Err(e) = self.restore_request(
                c.smart,
                1,
                &[c.smart_state[0], c.smart_state[1], 0],
                allow_stopped,
            )
        {
            errors.push(e);
            abort_writes = allow_stopped;
        }
        if !abort_writes && self.interruption(allow_stopped).is_none() && c.dpi != 0 {
            let d = c.current_dpi.to_be_bytes();
            if let Err(e) = self.restore_request(c.dpi, 3, &[0, d[0], d[1]], allow_stopped) {
                errors.push(e);
            }
        }
        if self.rx.disconnected.get() {
            self.changed = false;
            return Ok(());
        }
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
            IOHIDDeviceRegisterRemovalCallback(self.device.0, None, ptr::null_mut());
            IOHIDDeviceRegisterInputReportCallback(
                self.device.0,
                self.buffer.as_mut_ptr(),
                self.buffer.len() as isize,
                None,
                ptr::null_mut(),
            );
            IOHIDDeviceUnscheduleFromRunLoop(
                self.device.0,
                CFRunLoopGetCurrent(),
                kCFRunLoopDefaultMode,
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
    s.initialize_battery();
    Ok(s.battery_snapshot())
}

#[derive(Default)]
struct HardwareControl {
    stopped: bool,
    runloop: usize,
    source: usize,
    pending: Option<Option<Settings>>,
}
impl Drop for HardwareControl {
    fn drop(&mut self) {
        unsafe {
            if self.source != 0 {
                CFRelease(self.source as Ref);
            }
            if self.runloop != 0 {
                CFRelease(self.runloop as Ref);
            }
        }
    }
}
pub struct HardwareHandle {
    control: Arc<Mutex<HardwareControl>>,
    worker: Option<thread::JoinHandle<()>>,
}
unsafe extern "C" fn wake(_: *mut c_void) {}
fn control_stopped(control: &Arc<Mutex<HardwareControl>>) -> bool {
    control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stopped
}

fn stop_control(control: &Arc<Mutex<HardwareControl>>) {
    let mut state = control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.stopped = true;
    state.pending = None;
}

fn clear_signal_refs(control: &Arc<Mutex<HardwareControl>>) {
    let (runloop, source) = {
        let mut state = control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let refs = (state.runloop, state.source);
        state.runloop = 0;
        state.source = 0;
        refs
    };
    unsafe {
        if source != 0 {
            CFRelease(source as Ref);
        }
        if runloop != 0 {
            CFRelease(runloop as Ref);
        }
    }
}

fn take_disconnect_notification(disconnected: bool, reported: &mut bool) -> bool {
    if !disconnected || *reported {
        return false;
    }
    *reported = true;
    true
}

fn stop_if_disconnected(
    session: &Session,
    control: &Arc<Mutex<HardwareControl>>,
    callback: SnapshotCallback,
    context: usize,
    reported: &mut bool,
) -> bool {
    if !session.disconnected() {
        return false;
    }
    stop_control(control);
    if take_disconnect_notification(true, reported) {
        emit(callback, context, &session.disconnected_snapshot());
    }
    true
}

fn hardware_run(
    id: u64,
    control: Arc<Mutex<HardwareControl>>,
    callback: SnapshotCallback,
    output: OutputCallback,
    context: usize,
) {
    if control_stopped(&control) {
        return;
    }
    let mut session = match Session::open(id, Some(output), context, Arc::clone(&control)) {
        Ok(mut s) => {
            if s.interruption(false) == Some(RequestInterruption::Stopped) {
                return;
            }
            if let Err(e) = s.probe() {
                let stopped = s.interruption(false) == Some(RequestInterruption::Stopped);
                stop_control(&control);
                if s.disconnected() {
                    emit(callback, context, &s.disconnected_snapshot());
                } else if !stopped {
                    emit(callback, context, &s.snapshot(false, Some(&e)));
                }
                return;
            }
            s
        }
        Err(e) => {
            let should_emit = !control_stopped(&control);
            stop_control(&control);
            if should_emit {
                emit(
                    callback,
                    context,
                    &format!(
                        "{{\"ready\":false,\"applied\":false,\"battery\":null,\"error\":{}}}",
                        json_string(&e)
                    ),
                );
            }
            return;
        }
    };
    if session.disconnected() {
        stop_control(&control);
        emit(callback, context, &session.disconnected_snapshot());
        return;
    }
    if control_stopped(&control) {
        return;
    }
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
            stop_control(&control);
            emit(
                callback,
                context,
                &session.unavailable_snapshot("Cannot create settings control source"),
            );
            return;
        }
        CFRunLoopAddSource(runloop, source.0, kCFRunLoopDefaultMode);
        // HardwareControl keeps independent references. A signaling caller
        // takes its own retains while holding the mutex, so cleanup can clear
        // and release these references without a use-after-free race.
        let retained_runloop = CFRetain(runloop);
        let retained_source = CFRetain(source.0);
        {
            let mut c = control.lock().unwrap_or_else(|p| p.into_inner());
            c.runloop = retained_runloop as usize;
            c.source = retained_source as usize;
        }
        if control_stopped(&control) {
            CFRunLoopRemoveSource(runloop, source.0, kCFRunLoopDefaultMode);
            clear_signal_refs(&control);
            return;
        }
        emit(callback, context, &session.snapshot(false, None));
        let mut applied = false;
        let mut disconnected_reported = false;
        let mut next_battery_refresh = Instant::now() + BATTERY_REFRESH_INTERVAL;
        loop {
            let (stopped, pending) = {
                let mut c = control.lock().unwrap_or_else(|p| p.into_inner());
                (c.stopped, c.pending.take())
            };
            if stopped {
                break;
            }
            if stop_if_disconnected(
                &session,
                &control,
                callback,
                context,
                &mut disconnected_reported,
            ) {
                break;
            }
            if let Some(settings) = pending {
                let requested_applied = settings.is_some();
                let result = if let Some(s) = settings {
                    session.apply(s)
                } else {
                    session.restore()
                };
                if stop_if_disconnected(
                    &session,
                    &control,
                    callback,
                    context,
                    &mut disconnected_reported,
                ) {
                    break;
                }
                if control_stopped(&control) {
                    break;
                }
                applied = requested_applied && result.is_ok();
                emit(
                    callback,
                    context,
                    &session.snapshot(applied, result.as_ref().err().map(String::as_str)),
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
                    if stop_if_disconnected(
                        &session,
                        &control,
                        callback,
                        context,
                        &mut disconnected_reported,
                    ) {
                        break;
                    }
                    if control_stopped(&control) {
                        break;
                    }
                    emit(callback, context, &session.snapshot(applied, Some(&e)));
                }
            }
            if session.take_battery_update() {
                emit(callback, context, &session.snapshot(applied, None));
            }
            if Instant::now() >= next_battery_refresh {
                next_battery_refresh = Instant::now() + BATTERY_REFRESH_INTERVAL;
                if session.refresh_battery() {
                    emit(callback, context, &session.snapshot(applied, None));
                }
                if stop_if_disconnected(
                    &session,
                    &control,
                    callback,
                    context,
                    &mut disconnected_reported,
                ) {
                    break;
                }
            }
            // Input and control sources wake this wait immediately. The bounded
            // timeout also refreshes the displayed battery while the app runs.
            let wait = next_battery_refresh
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1));
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, wait.as_secs_f64(), 1);
        }
        stop_control(&control);
        let result = session.restore_during_shutdown();
        if !stop_if_disconnected(
            &session,
            &control,
            callback,
            context,
            &mut disconnected_reported,
        ) && let Err(e) = result
        {
            emit(
                callback,
                context,
                &session.snapshot(
                    false,
                    Some(&format!("Restore failed: {e}. Power-cycle the mouse.")),
                ),
            );
        }
        CFRunLoopRemoveSource(runloop, source.0, kCFRunLoopDefaultMode);
        clear_signal_refs(&control);
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
fn signal(control: &Arc<Mutex<HardwareControl>>) {
    let (runloop, source) = {
        let c = control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if c.runloop == 0 || c.source == 0 {
            return;
        }
        unsafe { (CFRetain(c.runloop as Ref), CFRetain(c.source as Ref)) }
    };
    unsafe {
        CFRunLoopSourceSignal(source);
        CFRunLoopWakeUp(runloop);
        CFRelease(source);
        CFRelease(runloop);
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
    {
        let mut c = h.control.lock().unwrap_or_else(|p| p.into_inner());
        if c.stopped {
            return -1;
        }
        c.pending = Some(settings);
    }
    signal(&h.control);
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
    }
    signal(&h.control);
    if let Some(worker) = h.worker.take() {
        let _ = worker.join();
    }
}

/// Read battery level and charging status from a device by registry ID.
/// Returns a freshly allocated UTF-8 JSON string; the caller is responsible for freeing it via `smoothmx_hardware_free`.
/// Pass `0` as `registry_id` to return null.
/// # Safety
/// The caller must ensure `registry_id` points to a valid HID++ mouse on this system.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_hardware_battery_read(registry_id: u64) -> *mut c_char {
    if registry_id == 0 {
        return ptr::null_mut();
    }
    let json = (|| -> Result<String, String> {
        let mut s = Session::open(
            registry_id,
            None,
            0,
            Arc::new(Mutex::new(HardwareControl::default())),
        )?;
        s.initialize_battery();
        Ok(s.battery_snapshot())
    })();
    match json {
        Ok(s) => CString::new(s.as_bytes())
            .map(|c| c.into_raw())
            .unwrap_or(ptr::null_mut()),
        Err(_) => CString::new("{\"battery\":null}")
            .map(|c| c.into_raw())
            .unwrap_or(ptr::null_mut()),
    }
}

/// Free a JSON string returned by `smoothmx_hardware_battery_read`.
/// # Safety
/// The pointer must be a valid UTF-8 null-terminated string returned by this library,
/// or `null` (the function is a no-op for null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoothmx_hardware_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smoothmx_core::input::{Action, Binding};

    fn settings() -> Settings {
        Settings {
            dpi: 0,
            wheel_mode: 1,
            threshold: 16,
            vertical: false,
            vertical_gain: 1.0,
            vertical_invert: false,
            smooth: false,
            horizontal: false,
            horizontal_gain: 1.0,
            horizontal_invert: false,
            horizontal_actions: [Action::default(); 2],
            gesture_threshold: 80,
            bindings: [Binding::default(); 5],
        }
    }

    fn rx() -> Rx {
        Rx {
            expected: Cell::new(None),
            response: RefCell::new(None),
            controls: Cell::new(0),
            vertical: Cell::new(0),
            horizontal: Cell::new(0),
            battery_unified: Cell::new(0),
            battery_legacy: Cell::new(0),
            battery_update: Cell::new(None),
            vertical_multiplier: Cell::new(1.0),
            horizontal_multiplier: Cell::new(1.0),
            engine: RefCell::new(InputEngine::default()),
            output: None,
            context: 0,
            toggle: Cell::new(false),
            disconnected: Cell::new(false),
        }
    }

    #[test]
    fn normalizes_only_complete_hidpp_reports() {
        let mut long = [0u8; HIDPP_LONG_LENGTH];
        long[..4].copy_from_slice(&[HIDPP_LONG_REPORT, 0xff, 3, 0x21]);
        let packet = normalize_report(HIDPP_LONG_REPORT.into(), &long).unwrap();
        assert_eq!(&packet.bytes[..packet.len], &long);

        let packet = normalize_report(HIDPP_LONG_REPORT.into(), &long[1..]).unwrap();
        assert_eq!(&packet.bytes[..packet.len], &long);
        assert!(normalize_report(HIDPP_LONG_REPORT.into(), &long[..18]).is_none());
        assert!(normalize_report(HIDPP_SHORT_REPORT.into(), &long[..8]).is_none());
        assert!(normalize_report(0x110, &long).is_none());
    }

    #[test]
    fn encodes_and_bounds_hidpp_requests() {
        let packet = encode_request(3, 2, 15, &[1, 2, 3]).unwrap();
        assert_eq!(&packet.bytes[..7], &[0x11, 0xff, 3, 0x2f, 1, 2, 3]);
        assert!(encode_request(3, 16, 1, &[]).is_err());
        assert!(encode_request(3, 2, 0, &[]).is_err());
        assert!(encode_request(3, 2, 1, &[0; 17]).is_err());
    }

    #[test]
    fn correlates_success_and_error_without_overwriting_first_response() {
        let rx = rx();
        rx.expected.set(Some((3, 0x21)));
        rx.packet(&[0x10, 0xff, 0xff, 3, 0x21, 8, 0]);
        rx.packet(&[0x10, 0xff, 3, 0x21, 7, 8, 9]);
        assert_eq!(
            rx.response.borrow().as_ref().unwrap().as_ref().unwrap_err(),
            "HID++ error 0x08"
        );

        rx.response.borrow_mut().take();
        rx.packet(&[0x10, 0xff, 3, 0x21, 7, 8, 9]);
        let response = *rx.response.borrow().as_ref().unwrap().as_ref().unwrap();
        assert_eq!(&response[..3], &[7, 8, 9]);
    }

    #[test]
    fn shutdown_allows_restore_but_disconnect_always_interrupts() {
        assert_eq!(
            request_interruption(true, false, false),
            Some(RequestInterruption::Stopped)
        );
        assert_eq!(request_interruption(true, false, true), None);
        assert_eq!(
            request_interruption(true, true, true),
            Some(RequestInterruption::Disconnected)
        );
    }

    #[test]
    fn disconnect_notification_is_taken_only_once() {
        let mut reported = false;
        assert!(!take_disconnect_notification(false, &mut reported));
        assert!(take_disconnect_notification(true, &mut reported));
        assert!(!take_disconnect_notification(true, &mut reported));
    }

    #[test]
    fn wheel_toggle_requires_smart_shift_only_when_action_is_reachable() {
        let mut settings = settings();
        settings.bindings[0].actions[0].kind = 5;
        assert!(!settings_uses_wheel_toggle(&settings));

        settings.bindings[0].enabled = true;
        assert!(settings_uses_wheel_toggle(&settings));

        settings.bindings[0] = Binding::default();
        settings.horizontal_actions[1].kind = 5;
        assert!(!settings_uses_wheel_toggle(&settings));
        settings.horizontal = true;
        assert!(settings_uses_wheel_toggle(&settings));

        settings.horizontal = false;
        settings.horizontal_actions = [Action::default(); 2];
        settings.bindings[0].enabled = true;
        settings.bindings[0].actions[4].kind = 5;
        assert!(!settings_uses_wheel_toggle(&settings));
        settings.bindings[0].gesture = true;
        assert!(settings_uses_wheel_toggle(&settings));
    }

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
    fn unified_battery_preserves_zero_percent_and_charging_state() {
        let mut status = [0; 16];
        status[0] = 0;
        status[2] = 4;
        assert_eq!(
            parse_unified_battery(&status),
            Some(BatteryInfo {
                level_percent: 0,
                charging: true,
            })
        );
    }
    #[test]
    fn battery_parsers_reject_invalid_percentages() {
        let mut status = [0; 16];
        status[0] = 101;
        assert_eq!(parse_unified_battery(&status), None);
        assert_eq!(parse_legacy_battery(&status), None);

        status[0] = 50;
        status[2] = 8;
        assert_eq!(parse_unified_battery(&status), None);
        assert_eq!(parse_legacy_battery(&status), None);
    }
    #[test]
    fn legacy_battery_decodes_charging_status() {
        let mut status = [0; 16];
        status[0] = 75;
        status[2] = 4;
        assert_eq!(
            parse_legacy_battery(&status),
            Some(BatteryInfo {
                level_percent: 75,
                charging: true,
            })
        );
    }
    #[test]
    fn battery_features_prefer_unified_then_fall_back_to_legacy() {
        assert_eq!(
            battery_feature_order(BatteryFeatures {
                unified: 3,
                legacy: 7,
            }),
            [
                Some(BatteryFeature::Unified(3)),
                Some(BatteryFeature::Legacy(7)),
            ]
        );
        assert_eq!(
            battery_feature_order(BatteryFeatures {
                unified: 0,
                legacy: 7,
            }),
            [None, Some(BatteryFeature::Legacy(7))]
        );
    }
    #[test]
    fn unified_battery_event_queues_live_update() {
        let rx = rx();
        rx.battery_unified.set(3);
        rx.packet(&[HIDPP_SHORT_REPORT, 0xff, 3, 0, 64, 4, 1]);
        assert_eq!(
            rx.battery_update.take(),
            Some(BatteryInfo {
                level_percent: 64,
                charging: true,
            })
        );
    }
}
