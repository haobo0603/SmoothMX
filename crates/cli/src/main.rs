use std::{env, fmt, process::ExitCode};

const USAGE_ERROR_EXIT_CODE: u8 = 2;
const CAPABILITIES_USAGE: &str = "Usage: smoothmx capabilities <hid-id>";
const PROBE_USAGE: &str = "Usage: smoothmx probe <hid-id>";
const BATTERY_USAGE: &str = "Usage: smoothmx battery <hid-id>";
const DEVICES_USAGE: &str = "Usage: smoothmx devices [--json]";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    Doctor,
    Devices { json: bool },
    Capabilities { hid_id: u64 },
    Battery { hid_id: u64 },
}

#[derive(Debug, PartialEq, Eq)]
enum CliError {
    InvalidArguments { usage: &'static str },
    InvalidHidId { value: String, reason: HidIdError },
    UnknownCommand { command: String },
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArguments { usage } => write!(formatter, "{usage}"),
            Self::InvalidHidId { value, reason } => {
                write!(formatter, "Invalid HID ID `{value}`: {reason}.")
            }
            Self::UnknownCommand { command } => {
                write!(
                    formatter,
                    "Unknown command `{command}`. Run `smoothmx --help`."
                )
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HidIdError {
    MissingPrefix,
    EmptyValue,
    NonHexadecimal,
    Overflow,
}

impl fmt::Display for HidIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => write!(formatter, "expected the `hid-<hex>` form"),
            Self::EmptyValue => write!(formatter, "expected hexadecimal digits after `hid-`"),
            Self::NonHexadecimal => {
                write!(formatter, "expected only hexadecimal digits after `hid-`")
            }
            Self::Overflow => write!(formatter, "value exceeds the 64-bit HID ID range"),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(command) => execute(command),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(USAGE_ERROR_EXIT_CODE)
        }
    }
}

fn parse_args(args: &[String]) -> Result<Command, CliError> {
    let command = match args {
        [] => Command::Help,
        [arg] if arg == "--help" || arg == "-h" => Command::Help,
        [arg] if arg == "--version" || arg == "-V" => Command::Version,
        [arg] if arg == "doctor" => Command::Doctor,
        [command] if command == "devices" => Command::Devices { json: false },
        [command, flag] if command == "devices" && flag == "--json" => {
            Command::Devices { json: true }
        }
        [command, id] if command == "capabilities" || command == "probe" => Command::Capabilities {
            hid_id: parse_cli_hid_id(id)?,
        },
        [command, id] if command == "battery" => Command::Battery {
            hid_id: parse_cli_hid_id(id)?,
        },
        [command, ..] if command == "capabilities" => {
            return Err(CliError::InvalidArguments {
                usage: CAPABILITIES_USAGE,
            });
        }
        [command, ..] if command == "probe" => {
            return Err(CliError::InvalidArguments { usage: PROBE_USAGE });
        }
        [command, ..] if command == "battery" => {
            return Err(CliError::InvalidArguments {
                usage: BATTERY_USAGE,
            });
        }
        [command, ..] if command == "devices" => {
            return Err(CliError::InvalidArguments {
                usage: DEVICES_USAGE,
            });
        }
        [command, ..] => {
            return Err(CliError::UnknownCommand {
                command: command.clone(),
            });
        }
    };
    Ok(command)
}

fn parse_cli_hid_id(value: &str) -> Result<u64, CliError> {
    parse_hid_id(value).map_err(|reason| CliError::InvalidHidId {
        value: value.to_owned(),
        reason,
    })
}

fn parse_hid_id(value: &str) -> Result<u64, HidIdError> {
    let hexadecimal = value
        .strip_prefix("hid-")
        .ok_or(HidIdError::MissingPrefix)?;
    if hexadecimal.is_empty() {
        return Err(HidIdError::EmptyValue);
    }
    if !hexadecimal.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HidIdError::NonHexadecimal);
    }
    u64::from_str_radix(hexadecimal, 16).map_err(|_| HidIdError::Overflow)
}

fn execute(command: Command) -> ExitCode {
    match command {
        Command::Help => help(),
        Command::Version => {
            println!(
                "{} {}",
                smoothmx_core::PRODUCT_NAME,
                env!("CARGO_PKG_VERSION")
            );
        }
        Command::Doctor => doctor(),
        Command::Devices { json } => return devices(json),
        Command::Capabilities { hid_id } => {
            match smoothmx_platform_macos::hardware::probe(hid_id) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("Capabilities read failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Command::Battery { hid_id } => match smoothmx_platform_macos::hardware::battery(hid_id) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("Battery read failed: {error}");
                return ExitCode::FAILURE;
            }
        },
    }
    ExitCode::SUCCESS
}

fn help() {
    println!("SmoothMX — macOS Logitech mouse discovery and direct Bluetooth HID++ settings");
    println!();
    println!("Usage:");
    println!("  smoothmx <command> [options]");
    println!();
    println!("Commands:");
    println!("  devices [--json]         List discovered Logitech HID devices");
    println!("  capabilities <hid-id>    Read capabilities and current settings as JSON");
    println!("  probe <hid-id>           Compatibility alias for capabilities");
    println!("  battery <hid-id>         Read battery information as JSON");
    println!("  doctor                    Show runtime support without probing hardware");
    println!();
    println!("Options:");
    println!("  -h, --help                Show this help");
    println!("  -V, --version             Show the version");
    println!();
    println!("<hid-id> is a connection-scoped ID printed by `devices`, in `hid-<hex>` form.");
}

fn doctor() {
    println!("SmoothMX diagnostics");
    println!("Platform: {} / {}", env::consts::OS, env::consts::ARCH);
    println!("API version: {}", smoothmx_core::API_VERSION);
    println!("Hardware backend: macOS HID discovery and direct Bluetooth HID++ settings");
    println!("Resident service: not implemented");
    println!("Permissions: not checked");
    println!("Hardware verification: not performed by this command");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parses_non_hardware_commands_and_options() {
        assert_eq!(parse_args(&[]), Ok(Command::Help));
        assert_eq!(parse_args(&arguments(&["-h"])), Ok(Command::Help));
        assert_eq!(parse_args(&arguments(&["--help"])), Ok(Command::Help));
        assert_eq!(parse_args(&arguments(&["-V"])), Ok(Command::Version));
        assert_eq!(parse_args(&arguments(&["doctor"])), Ok(Command::Doctor));
        assert_eq!(
            parse_args(&arguments(&["devices"])),
            Ok(Command::Devices { json: false })
        );
        assert_eq!(
            parse_args(&arguments(&["devices", "--json"])),
            Ok(Command::Devices { json: true })
        );
    }

    #[test]
    fn capabilities_is_canonical_and_probe_remains_an_alias() {
        let expected = Ok(Command::Capabilities { hid_id: 0x1a2b });
        assert_eq!(
            parse_args(&arguments(&["capabilities", "hid-1a2b"])),
            expected
        );
        assert_eq!(
            parse_args(&arguments(&["probe", "hid-1a2b"])),
            Ok(Command::Capabilities { hid_id: 0x1a2b })
        );
        assert_eq!(
            parse_args(&arguments(&["battery", "hid-1A2B"])),
            Ok(Command::Battery { hid_id: 0x1a2b })
        );
    }

    #[test]
    fn rejects_invalid_command_arguments() {
        assert_eq!(
            parse_args(&arguments(&["devices", "--verbose"])),
            Err(CliError::InvalidArguments {
                usage: DEVICES_USAGE
            })
        );
        assert_eq!(
            parse_args(&arguments(&["capabilities"])),
            Err(CliError::InvalidArguments {
                usage: CAPABILITIES_USAGE
            })
        );
        assert_eq!(
            parse_args(&arguments(&["missing"])),
            Err(CliError::UnknownCommand {
                command: "missing".into()
            })
        );
    }

    #[test]
    fn parses_connection_scoped_hid_ids() {
        assert_eq!(parse_hid_id("hid-0"), Ok(0));
        assert_eq!(parse_hid_id("hid-deadBEEF"), Ok(0xdead_beef));
        assert_eq!(parse_hid_id("hid-ffffffffffffffff"), Ok(u64::MAX));
    }

    #[test]
    fn rejects_wrong_or_empty_hid_id_prefixes() {
        assert_eq!(parse_hid_id("1234"), Err(HidIdError::MissingPrefix));
        assert_eq!(parse_hid_id("usb-1234"), Err(HidIdError::MissingPrefix));
        assert_eq!(parse_hid_id("HID-1234"), Err(HidIdError::MissingPrefix));
        assert_eq!(parse_hid_id("hid-"), Err(HidIdError::EmptyValue));
    }

    #[test]
    fn rejects_non_hexadecimal_and_overflowing_hid_ids() {
        assert_eq!(parse_hid_id("hid-not-hex"), Err(HidIdError::NonHexadecimal));
        assert_eq!(parse_hid_id("hid-12 34"), Err(HidIdError::NonHexadecimal));
        assert_eq!(
            parse_hid_id("hid-10000000000000000"),
            Err(HidIdError::Overflow)
        );
    }
}
