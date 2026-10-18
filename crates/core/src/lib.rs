//! Shared configuration and capability model for SmoothMX.
//! Read-only HID discovery is provided by smoothmx-platform-macos.
//! Settings changes and input interception are not implemented yet.

/// Features must be discovered on the connected device before being enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Battery,
    Dpi,
    ButtonRemapping,
    SmartShift,
    ThumbWheel,
    Gestures,
}

pub const PRODUCT_NAME: &str = "SmoothMX";
pub const API_VERSION: u32 = 1;

/// One HID interface, identified for the current connection by registry ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidInterface {
    pub registry_id: u64,
    pub name: String,
    pub vendor_id: u32,
    pub product_id: u32,
    pub transport: String,
    pub location_id: u32,
    pub usage_page: u32,
    pub usage: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub vendor_id: u32,
    pub product_id: u32,
    pub transport: String,
    pub kind: &'static str,
    pub interface_count: usize,
}

/// Group only USB interfaces sharing a nonzero physical location and VID/PID.
/// Do not merge devices by product name or guess receiver children.
pub fn group_devices(interfaces: impl IntoIterator<Item = HidInterface>) -> Vec<Device> {
    let mut groups = std::collections::BTreeMap::<String, Vec<HidInterface>>::new();
    for item in interfaces {
        let key = if item.transport == "USB" && item.location_id != 0 {
            format!(
                "usb-{:x}-{:x}-{:x}",
                item.location_id, item.vendor_id, item.product_id
            )
        } else {
            format!("hid-{:x}", item.registry_id)
        };
        groups.entry(key).or_default().push(item);
    }
    groups
        .into_iter()
        .map(|(id, mut items)| {
            items.sort_by_key(|item| item.registry_id);
            let receiver = items
                .iter()
                .find(|item| item.name.to_ascii_lowercase().contains("receiver"));
            let mouse = items
                .iter()
                .find(|item| item.usage_page == 1 && item.usage == 2);
            let chosen = receiver.or(mouse).unwrap_or(&items[0]);
            Device {
                id,
                name: chosen.name.clone(),
                vendor_id: chosen.vendor_id,
                product_id: chosen.product_id,
                transport: chosen.transport.clone(),
                kind: if receiver.is_some() {
                    "receiver"
                } else if mouse.is_some() {
                    "mouse"
                } else {
                    "unresolved"
                },
                interface_count: items.len(),
            }
        })
        .collect()
}

pub fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            c if c <= '\u{1f}' => {
                use std::fmt::Write;
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

pub fn devices_json(devices: &[Device]) -> String {
    let records: Vec<_> = devices.iter().map(|d| format!(
        "{{\"id\":{},\"name\":{},\"vendor_id\":{},\"product_id\":{},\"transport\":{},\"kind\":{},\"interface_count\":{}}}",
        json_string(&d.id), json_string(&d.name), d.vendor_id, d.product_id,
        json_string(&d.transport), json_string(d.kind), d.interface_count,
    )).collect();
    format!(
        "{{\"schema_version\":1,\"devices\":[{}],\"error\":null}}",
        records.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn interface(id: u64, location: u32, name: &str) -> HidInterface {
        HidInterface {
            registry_id: id,
            location_id: location,
            name: name.into(),
            vendor_id: 0x046d,
            product_id: 123,
            transport: "USB".into(),
            usage_page: 1,
            usage: 2,
        }
    }
    #[test]
    fn groups_interfaces_but_not_identical_mice_at_different_ports() {
        let devices = group_devices([
            interface(1, 10, "Mouse"),
            interface(2, 10, "Mouse"),
            interface(3, 20, "Mouse"),
        ]);
        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices
                .iter()
                .find(|d| d.id == "usb-a-46d-7b")
                .unwrap()
                .interface_count,
            2
        );
    }
    #[test]
    fn missing_location_does_not_merge_devices() {
        assert_eq!(
            group_devices([interface(1, 0, "Mouse"), interface(2, 0, "Mouse")]).len(),
            2
        );
    }
    #[test]
    fn receiver_mouse_usage_is_not_a_physical_mouse_claim() {
        assert_eq!(
            group_devices([interface(1, 10, "USB Receiver")])[0].kind,
            "receiver"
        );
    }
    #[test]
    fn escapes_device_supplied_strings() {
        assert_eq!(json_string("a\"\\\n\0é"), "\"a\\\"\\\\\\u000a\\u0000é\"");
    }
}

pub mod input;
