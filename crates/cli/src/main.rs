use std::{env, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => help(),
        [arg] if arg == "--help" || arg == "-h" => help(),
        [arg] if arg == "--version" || arg == "-V" => {
            println!(
                "{} {}",
                smoothmx_core::PRODUCT_NAME,
                env!("CARGO_PKG_VERSION")
            );
        }
        [arg] if arg == "doctor" => {
            println!("SmoothMX development scaffold");
            println!("Platform: {} / {}", env::consts::OS, env::consts::ARCH);
            println!("API version: {}", smoothmx_core::API_VERSION);
            println!("Hardware backend: read-only macOS HID discovery");
            println!("Background service: not implemented");
            println!("Permissions: not checked");
        }
        [command] if command == "devices" => return devices(false),
        [command, flag] if command == "devices" && flag == "--json" => return devices(true),
        [command, id] if command == "probe" => {
            let id = id.trim_start_matches("hid-");
            match u64::from_str_radix(id, 16)
                .map_err(|e| e.to_string())
                .and_then(smoothmx_platform_macos::hardware::probe)
            {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("Probe failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        [command, id] if command == "battery" => {
            let id = id.trim_start_matches("hid-");
            match u64::from_str_radix(id, 16)
                .map_err(|e| e.to_string())
                .and_then(smoothmx_platform_macos::hardware::battery)
            {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("Battery read failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        _ => {
            eprintln!("Unknown command. Run smoothmx --help.");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn help() {
    println!("SmoothMX — macOS mouse discovery");
    println!("Usage: smoothmx [--help | --version | doctor | devices [--json] | battery <hid-id>]");
}

fn devices(json: bool) -> ExitCode {
    match smoothmx_platform_macos::discover() {
        Ok(devices) => {
            if json {
                println!("{}", smoothmx_core::devices_json(&devices));
            } else if devices.is_empty() {
                println!(
                    "No Logitech HID devices detected. Check the mouse connection or receiver."
                );
            } else {
                for device in devices {
                    println!("{} · {} · {}", device.name, device.transport, device.kind);
                    println!(
                        "  ID: {} | VID: 0x{:04X} | PID: 0x{:04X} | Interfaces: {}",
                        device.id, device.vendor_id, device.product_id, device.interface_count
                    );
                    if device.kind == "receiver" {
                        println!("  Paired mouse identification requires HID++ (not implemented).");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                println!(
                    "{{\"schema_version\":1,\"devices\":[],\"error\":{}}}",
                    smoothmx_core::json_string(&error)
                );
            } else {
                eprintln!("Discovery failed: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
