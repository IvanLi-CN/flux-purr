use super::*;

use espflash::{
    command::Command as RomCommand,
    connection::{Connection, ResetAfterOperation, ResetBeforeOperation, SecurityInfo},
    target::Chip,
};
use object::{Object as _, ObjectSection as _, ObjectSegment as _, SectionFlags};
use serialport::{FlowControl, SerialPortType, UsbPortInfo};

#[cfg(target_os = "macos")]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RamIdentityObservation {
    #[serde(default)]
    firmware_kind: Option<flux_purr_devd::FirmwareKind>,
    build_id: String,
    capabilities: Vec<String>,
}

#[derive(Debug)]
struct RamRuntimeIdentity {
    identity: RamIdentityObservation,
}

pub(crate) fn direct_ram_run(
    command: RamRunCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        RamRunCommand::Preview(args) => ram_run_action(
            &args.options,
            args.preview.command(),
            args.preview.command().to_string(),
        ),
        RamRunCommand::Test(args) => ram_run_action(
            &args.options,
            args.test.command(),
            args.test.command().to_string(),
        ),
        RamRunCommand::Exit(args) => ram_run_exit(&args.port),
    }
}

fn ram_run_action(
    options: &RamRunOptions,
    command: &str,
    operation: String,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(&options.port)?;
    let _serial_lock =
        flux_purr_devd::acquire_serial_port_lock(&options.port, Duration::from_secs(30))
            .map_err(io::Error::other)?;
    if options.theme.is_some() && !command.starts_with("preview_") {
        return Err("--theme is only valid for ram-run preview commands".into());
    }

    let expected_build_id = expected_ram_build_id();
    let existing = probe_ram_identity(&options.port)?;
    let elf = default_ram_bringup_elf();
    validate_ram_bringup_elf(&elf)?;

    if options.install {
        let flash = direct_flash_with_program_allow_missing_app_descriptor(
            FlashArgs {
                port: options.port.clone(),
                elf: Some(elf),
                skip_backup: options.skip_backup,
                confirm: options.confirm.clone(),
                keep_download_mode: false,
            },
            &resolve_espflash_program(),
            true,
        )?;
        let identity = wait_for_ram_identity(&options.port, &expected_build_id)?;
        ensure_ram_capability(&identity, command)?;
        let response = send_ram_command(&options.port, command, options.theme)?;
        return Ok(json!({
            "ok": true,
            "operation": operation,
            "port": options.port,
            "mode": "installed",
            "loaded": true,
            "reused": false,
            "identity": identity.identity,
            "flash": flash,
            "response": response,
            "warning": "Bring-up now replaces the sole factory Product Firmware slot; flash Product Firmware explicitly to restore it.",
        }));
    }

    let reusable = !options.reload
        && existing
            .as_ref()
            .is_some_and(|runtime| ram_identity_matches(runtime, &expected_build_id, command));
    let mut ram_serial: Option<Box<dyn RamJsonlSerial>> = if !reusable {
        let espflash_program = resolve_espflash_program();
        ensure_pinned_ram_espflash(&espflash_program)?;
        ram_download_preflight(&options.port)?;
        let _diagnostics =
            run_espflash_command(&espflash_program, &ram_load_args(&options.port, &elf))?;
        Some(
            open_ram_jsonl_serial_retry(&options.port, Duration::from_secs(15)).map_err(
                |error| {
                    format!(
                        "RAM load completed but JSONL reopen failed on {}: {error}",
                        options.port
                    )
                },
            )?,
        )
    } else {
        None
    };

    let identity = if let Some(serial) = ram_serial.as_mut() {
        wait_for_ram_identity_on_serial(&mut **serial, &expected_build_id)?
    } else {
        wait_for_ram_identity(&options.port, &expected_build_id)?
    };
    ensure_ram_capability(&identity, command)?;
    let response = if let Some(serial) = ram_serial.as_mut() {
        send_ram_command_on_serial(&options.port, &mut **serial, command, options.theme)?
    } else {
        send_ram_command(&options.port, command, options.theme)?
    };
    Ok(json!({
        "ok": true,
        "operation": operation,
        "port": options.port,
        "mode": "ram",
        "loaded": !reusable,
        "reused": reusable,
        "identity": identity.identity,
        "response": response,
    }))
}

fn ensure_ram_capability(
    identity: &RamRuntimeIdentity,
    command: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if identity
        .identity
        .capabilities
        .iter()
        .any(|value| value == command)
    {
        return Ok(());
    }
    Err(format!(
        "RAM Bring-up build {} does not advertise command {command}",
        identity.identity.build_id
    )
    .into())
}

fn ram_run_exit(port: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(port)?;
    let _serial_lock = flux_purr_devd::acquire_serial_port_lock(port, Duration::from_secs(30))
        .map_err(io::Error::other)?;
    let output = run_external_with_timeout(
        &resolve_espflash_program(),
        &[
            "reset",
            "--chip",
            "esp32s3",
            "--port",
            port,
            "--before",
            "usb-reset",
            "--after",
            "hard-reset",
            "--non-interactive",
        ],
        Duration::from_secs(30),
    )?;
    if !output.success {
        return Err(format!(
            "failed to reset {port} into the installed application{}: {}",
            if output.timed_out {
                " before the 30-second deadline"
            } else {
                ""
            },
            output.stderr.trim()
        )
        .into());
    }
    match wait_for_product_identity(port)? {
        flux_purr_devd::FirmwareKind::Product => Ok(json!({
            "ok": true,
            "operation": "exit",
            "port": port,
            "reset": true,
            "firmwareKind": "product",
        })),
        flux_purr_devd::FirmwareKind::RamBringup => Err(
            "reset completed but Product Firmware is not installed; use the explicit product `flash` command to restore it".into(),
        ),
    }
}

fn default_ram_bringup_elf() -> PathBuf {
    flux_purr_repo_root()
        .join("firmware/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup")
}

fn expected_ram_build_id() -> String {
    if let Ok(build_id) = std::env::var("FLUX_PURR_BUILD_ID")
        && (16..=64).contains(&build_id.len())
    {
        return build_id;
    }
    ProcessCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(flux_purr_repo_root())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().chars().take(16).collect())
        .filter(|value: &String| value.len() == 16)
        .unwrap_or_else(|| "unknown".to_string())
}

fn validate_ram_bringup_elf(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !path.is_file() {
        return Err(format!(
            "RAM Bring-up ELF does not exist: {}\nBuild it with: cargo +esp build -p flux-purr-ram-bringup --target xtensa-esp32s3-none-elf --target-dir firmware/target --release",
            path.display()
        )
        .into());
    }
    let data = fs::read(path)?;
    let elf = object::read::elf::ElfFile32::<object::Endianness>::parse(data.as_slice())
        .map_err(|error| format!("invalid RAM Bring-up ELF: {error}"))?;
    let entry = elf.entry();
    if !ram_address_is_internal(entry) {
        return Err(format!(
            "RAM Bring-up ELF entry 0x{entry:08x} is outside ESP32-S3 internal RAM"
        )
        .into());
    }
    let mut loadable_segments = 0;
    for segment in elf.segments() {
        let address = segment.address();
        let size = segment.size();
        if size == 0 {
            continue;
        }
        let end = address
            .checked_add(size)
            .ok_or("RAM Bring-up ELF segment address overflows")?;
        if !ram_range_is_internal(address, end) {
            return Err(format!(
                "RAM Bring-up ELF PT_LOAD 0x{address:08x}..0x{end:08x} is outside ESP32-S3 internal RAM"
            )
            .into());
        }
        loadable_segments += 1;
    }
    for section in elf.sections() {
        let header = section.elf_section_header();
        let section_type = header.sh_type.get(elf.endian());
        let section_flags = section.flags();
        let is_allocated = !matches!(section_flags, SectionFlags::None)
            && !matches!(section_flags, SectionFlags::Elf { sh_flags: 0 });
        if section.size() == 0
            || !matches!(
                section_type,
                object::elf::SHT_PROGBITS | object::elf::SHT_INIT_ARRAY
            )
            || header.sh_offset.get(elf.endian()) == 0
            || section.address() == 0
            || !is_allocated
        {
            continue;
        }
        let address = section.address();
        let end = address
            .checked_add(section.size())
            .ok_or("RAM Bring-up ELF section address overflows")?;
        if !ram_range_is_internal(address, end) {
            return Err(format!(
                "RAM Bring-up ELF loadable section 0x{address:08x}..0x{end:08x} is outside ESP32-S3 internal RAM"
            )
            .into());
        }
    }
    if loadable_segments == 0 {
        return Err("RAM Bring-up ELF contains no non-empty PT_LOAD segments".into());
    }
    Ok(())
}

const RAM_INTERNAL_RANGES: &[(u64, u64)] = &[
    (0x4037_8000, 0x403e_0000),
    (0x3fc8_8000, 0x3fd0_0000),
    (0x600f_e000, 0x6010_0000),
    (0x5000_0000, 0x5000_2000),
];

fn ram_range_is_internal(start: u64, end: u64) -> bool {
    RAM_INTERNAL_RANGES
        .iter()
        .any(|(range_start, range_end)| start >= *range_start && end <= *range_end)
}

fn ram_address_is_internal(address: u64) -> bool {
    ram_range_is_internal(address, address.saturating_add(1))
}

fn ram_load_args(port: &str, elf: &Path) -> Vec<String> {
    vec![
        "flash".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port.into(),
        "--before".into(),
        "usb-reset".into(),
        "--after".into(),
        "no-reset".into(),
        "--non-interactive".into(),
        "--no-stub".into(),
        "--ram".into(),
        elf.to_string_lossy().into_owned(),
    ]
}

fn ensure_pinned_ram_espflash(
    program: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = ProcessCommand::new(program).arg("--version").output()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if version.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version
    };
    if !output.status.success() {
        return Err(format!(
            "RAM execution could not query pinned espflash 4.5.0; command failed for {}",
            program.display()
        )
        .into());
    }
    validate_pinned_ram_espflash_version(&version)
        .map_err(|error| format!("{error}; got {version:?} from {}", program.display()).into())
}

fn validate_pinned_ram_espflash_version(
    version: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if version == "espflash 4.5.0" {
        Ok(())
    } else {
        Err("RAM execution requires pinned espflash 4.5.0".into())
    }
}

fn ram_download_preflight(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut connection = open_ram_rom_connection(port).map_err(|error| {
        format!(
            "RAM execution ROM preflight failed on {port}: {error}; no Flash fallback was attempted"
        )
    })?;
    let chip = connection
        .detect_chip(false)
        .map_err(|error| format!("RAM execution requires ESP32-S3 chip detection: {error}"))?;
    if chip != Chip::Esp32s3 {
        return Err(format!(
            "RAM execution requires ESP32-S3, ROM detected {chip:?}; no Flash fallback was attempted"
        )
        .into());
    }
    let security = match connection.command(RomCommand::GetSecurityInfo)? {
        espflash::command::CommandResponseValue::Vector(bytes) => {
            SecurityInfo::try_from(bytes.as_slice())?
        }
        response => {
            return Err(
                format!("ROM returned an unexpected security response: {response:?}").into(),
            );
        }
    };
    eprintln!("RAM preflight security flags: 0x{:08x}", security.flags);
    if security.flags & (1 << 2) != 0 {
        return Err(
            "RAM execution is unavailable: ROM security flags enable secure download; no Flash fallback was attempted"
                .into(),
        );
    }
    if security.flags & (1 << 8) != 0 {
        return Err(
            "RAM execution is unavailable: ROM security flags disable USB access/download; no Flash fallback was attempted"
                .into(),
        );
    }
    Ok(())
}

fn open_ram_rom_connection(
    port: &str,
) -> Result<Connection, Box<dyn std::error::Error + Send + Sync>> {
    let port_info = serialport::available_ports()?
        .into_iter()
        .find(|candidate| candidate.port_name == port)
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    let usb_info = match port_info.port_type {
        SerialPortType::UsbPort(info) => info,
        SerialPortType::PciPort | SerialPortType::Unknown => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
        _ => return Err("authorized port is not a supported USB serial target".into()),
    };
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_secs(3))
        .open_native()?;
    let mut connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::NoReset,
        ResetBeforeOperation::UsbReset,
        115_200,
    );
    connection.begin()?;
    connection.set_timeout(Duration::from_secs(3))?;
    Ok(connection)
}

#[cfg(test)]
pub(crate) fn classify_ram_preflight_output(
    port: &str,
    success: bool,
    stdout: &str,
    stderr: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if !success {
        return Err(format!(
            "RAM execution preflight failed for {port}; ROM Download Mode or the selected transport is unavailable: {}",
            stderr.trim()
        )
        .into());
    }
    for blocked in [
        "secure uart download",
        "secure uart",
        "secure download",
        "download mode disabled",
        "rom download disabled",
        "uart download disabled",
        "usb serial-jtag download disabled",
        "usb download mode disabled",
    ] {
        if combined.contains(blocked) {
            return Err(format!(
                "RAM execution is unavailable: ROM reported {blocked}; no Flash fallback was attempted"
            )
            .into());
        }
    }
    if let Some(flags) = parse_security_flags(&combined) {
        if flags & (1 << 2) != 0 {
            return Err("RAM execution is unavailable: ROM security flags enable secure download; no Flash fallback was attempted".into());
        }
        if flags & (1 << 8) != 0 {
            return Err("RAM execution is unavailable: ROM security flags disable USB access/download; no Flash fallback was attempted".into());
        }
    } else {
        return Err("RAM execution preflight could not verify ROM security flags; no Flash fallback was attempted".into());
    }
    Ok(())
}

#[cfg(test)]
fn parse_security_flags(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let line = line.trim().to_ascii_lowercase();
        let value = line.strip_prefix("flags:")?;
        let token = value.split_whitespace().next()?;
        u32::from_str_radix(token.trim_start_matches("0x"), 16).ok()
    })
}

fn probe_ram_identity(
    port: &str,
) -> Result<Option<RamRuntimeIdentity>, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-preflight-{}", current_unix_millis());
    let request = json!({ "type": "request", "requestId": request_id, "op": "get_identity" });
    let response = match exchange_jsonl(port, &request, Duration::from_millis(900)) {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    let Some(response) = response else {
        return Ok(None);
    };
    Ok(decode_ram_identity_response(&response))
}

fn probe_ram_identity_on_serial(
    serial: &mut dyn RamJsonlSerial,
) -> Result<Option<RamRuntimeIdentity>, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-preflight-{}", current_unix_millis());
    let request = json!({ "type": "request", "requestId": request_id, "op": "get_identity" });
    let response = match exchange_jsonl_on_serial(serial, &request, Duration::from_millis(900)) {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    let Some(response) = response else {
        return Ok(None);
    };
    Ok(decode_ram_identity_response(&response))
}

fn decode_ram_identity_response(response: &Value) -> Option<RamRuntimeIdentity> {
    let identity = match response.get("type").and_then(Value::as_str) {
        Some("response") if response.get("ok").and_then(Value::as_bool) == Some(true) => {
            response.get("result")?.get("identity")?.clone()
        }
        Some("hello") => response.get("identity")?.clone(),
        _ => return None,
    };
    Some(RamRuntimeIdentity {
        identity: serde_json::from_value(identity).ok()?,
    })
}

fn ram_identity_matches(
    runtime: &RamRuntimeIdentity,
    expected_build_id: &str,
    command: &str,
) -> bool {
    runtime.identity.firmware_kind == Some(flux_purr_devd::FirmwareKind::RamBringup)
        && runtime.identity.build_id == expected_build_id
        && runtime
            .identity
            .capabilities
            .iter()
            .any(|value| value == command)
}

fn wait_for_ram_identity(
    port: &str,
    expected_build_id: &str,
) -> Result<RamRuntimeIdentity, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = StdInstant::now() + Duration::from_secs(15);
    loop {
        if let Some(identity) = probe_ram_identity(port)?
            && identity.identity.firmware_kind == Some(flux_purr_devd::FirmwareKind::RamBringup)
            && identity.identity.build_id == expected_build_id
        {
            return Ok(identity);
        }
        if StdInstant::now() >= deadline {
            return Err(format!(
                "RAM load completed without a matching ram_bringup identity for build {expected_build_id}; application kind remains unknown"
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_ram_identity_on_serial(
    serial: &mut dyn RamJsonlSerial,
    expected_build_id: &str,
) -> Result<RamRuntimeIdentity, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = StdInstant::now() + Duration::from_secs(15);
    loop {
        if let Some(identity) = probe_ram_identity_on_serial(serial)?
            && identity.identity.firmware_kind == Some(flux_purr_devd::FirmwareKind::RamBringup)
            && identity.identity.build_id == expected_build_id
        {
            return Ok(identity);
        }
        if StdInstant::now() >= deadline {
            return Err(format!(
                "RAM load completed without a matching ram_bringup identity for build {expected_build_id}; application kind remains unknown"
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_product_identity(
    port: &str,
) -> Result<flux_purr_devd::FirmwareKind, Box<dyn std::error::Error + Send + Sync>> {
    // Product boot performs PD discovery and display/ADC initialization before
    // the runtime USB loop is ready. Keep the probe on the caller's exact port
    // long enough for that bounded startup, while retaining strict identity
    // classification for legacy or malformed responses.
    const PRODUCT_IDENTITY_WAIT: Duration = Duration::from_secs(30);
    let deadline = StdInstant::now() + PRODUCT_IDENTITY_WAIT;
    loop {
        if let Some(identity) = probe_ram_identity(port)? {
            match identity.identity.firmware_kind {
                Some(flux_purr_devd::FirmwareKind::Product) => {
                    return Ok(flux_purr_devd::FirmwareKind::Product);
                }
                Some(flux_purr_devd::FirmwareKind::RamBringup) => {
                    return Err(
                        "reset completed but ram_bringup is still running; Product Firmware was not assumed"
                            .into(),
                    );
                }
                None => {
                    return Err(
                        "reset completed but installed Product Firmware uses the legacy identity protocol without firmwareKind; Product Firmware was not assumed"
                            .into(),
                    );
                }
            }
        }
        if StdInstant::now() >= deadline {
            return Err(format!(
                "reset completed but application identity is unknown after {} seconds; Product Firmware was not assumed",
                PRODUCT_IDENTITY_WAIT.as_secs()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn send_ram_command(
    port: &str,
    command: &str,
    theme: Option<RamTheme>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-command-{}", current_unix_millis());
    let request = json!({
        "type": "ram_bringup",
        "requestId": request_id,
        "command": command,
        "theme": theme.map(RamTheme::as_str),
    });
    send_ram_command_with_request(port, command, &request)
}

fn send_ram_command_with_request(
    port: &str,
    command: &str,
    request: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let response = exchange_jsonl_retry_same_port(port, request, ram_command_timeout(command))?
        .ok_or_else(|| format!("RAM Bring-up command {command} returned no JSONL response"))?;
    validate_ram_command_response(command, response)
}

fn send_ram_command_on_serial(
    port: &str,
    serial: &mut dyn RamJsonlSerial,
    command: &str,
    theme: Option<RamTheme>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-command-{}", current_unix_millis());
    let request = json!({
        "type": "ram_bringup",
        "requestId": request_id,
        "command": command,
        "theme": theme.map(RamTheme::as_str),
    });
    let response = match exchange_jsonl_on_serial(serial, &request, ram_command_timeout(command)) {
        Ok(Some(response)) => response,
        Ok(None) => {
            return send_ram_command_with_request(port, command, &request);
        }
        Err(error) if is_transient_device_error(error.as_ref()) => {
            // USB Serial/JTAG can briefly disappear while the restricted fan
            // output is enabled. Reopen only the caller's exact port and reuse
            // the same request id; the firmware caches completed command
            // responses so a retry cannot run the fan twice.
            return send_ram_command_with_request(port, command, &request);
        }
        Err(error) => return Err(error),
    };
    validate_ram_command_response(command, response)
}

fn validate_ram_command_response(
    command: &str,
    response: Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if response.get("type").and_then(Value::as_str) != Some("response")
        || response.get("ok").and_then(Value::as_bool) != Some(true)
    {
        return Err(
            format!("RAM Bring-up command {command} returned an unsuccessful response").into(),
        );
    }
    Ok(response)
}

fn is_transient_device_error(error: &(dyn std::error::Error + 'static)) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("device not configured")
        || message.contains("input/output error")
        || message.contains("no such file or directory")
}

fn ram_command_timeout(command: &str) -> Duration {
    match command {
        "preview_display" | "preview_status_light" | "test_fan" => Duration::from_secs(30),
        "preview_frontpanel" => Duration::from_secs(120),
        _ => Duration::from_secs(5),
    }
}

trait RamJsonlSerial: Read + Write {}
impl<T: Read + Write + ?Sized> RamJsonlSerial for T {}

fn open_ram_jsonl_serial(port: &str) -> io::Result<Box<dyn RamJsonlSerial>> {
    #[cfg(target_os = "macos")]
    {
        // USB Serial/JTAG has no modem-control contract. Opening it through
        // serialport would toggle DTR on some macOS driver versions and reset
        // the target after a RAM load, so use the same raw non-blocking path as
        // the regular devd transport.
        File::options()
            .read(true)
            .write(true)
            .custom_flags(0x0004)
            .open(port)
            .map(|file| Box::new(file) as Box<dyn RamJsonlSerial>)
    }

    #[cfg(not(target_os = "macos"))]
    serialport::new(port, 115_200)
        .preserve_dtr_on_open()
        .exclusive(false)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(100))
        .open_native()
        .map(|port| Box::new(port) as Box<dyn RamJsonlSerial>)
        .map_err(io::Error::other)
}

fn open_ram_jsonl_serial_retry(
    port: &str,
    timeout: Duration,
) -> io::Result<Box<dyn RamJsonlSerial>> {
    let deadline = StdInstant::now() + timeout;
    loop {
        match open_ram_jsonl_serial(port) {
            Ok(serial) => return Ok(serial),
            Err(error) if is_transient_device_error(&error) => {
                if StdInstant::now() >= deadline {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error),
        }
    }
}

fn exchange_jsonl(
    port: &str,
    request: &Value,
    timeout: Duration,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let mut serial = open_ram_jsonl_serial(port)?;
    exchange_jsonl_on_serial(&mut *serial, request, timeout)
}

fn exchange_jsonl_retry_same_port(
    port: &str,
    request: &Value,
    timeout: Duration,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = StdInstant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(StdInstant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        match exchange_jsonl(port, request, remaining) {
            Ok(response) => return Ok(response),
            Err(error) if is_transient_device_error(error.as_ref()) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error),
        }
    }
}

fn exchange_jsonl_on_serial(
    serial: &mut dyn RamJsonlSerial,
    request: &Value,
    timeout: Duration,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = request
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    serial.write_all(serde_json::to_string(request)?.as_bytes())?;
    serial.write_all(b"\n")?;
    serial.flush()?;
    let deadline = StdInstant::now() + timeout;
    let mut line = Vec::new();
    loop {
        if StdInstant::now() >= deadline {
            return Ok(None);
        }
        let mut byte = [0_u8; 1];
        match serial.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => {
                let value = serde_json::from_slice::<Value>(&line).ok();
                line.clear();
                let is_identity_hello = request.get("op").and_then(Value::as_str)
                    == Some("get_identity")
                    && value
                        .as_ref()
                        .and_then(|value| value.get("type"))
                        .and_then(Value::as_str)
                        == Some("hello")
                    && value
                        .as_ref()
                        .and_then(|value| value.get("identity"))
                        .is_some();
                if is_identity_hello
                    || value
                        .as_ref()
                        .and_then(|value| value.get("requestId"))
                        .and_then(Value::as_str)
                        == Some(request_id)
                {
                    return Ok(value);
                }
            }
            Ok(1) if line.len() < 8 * 1024 => line.push(byte[0]),
            Ok(1) => line.clear(),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[derive(Debug)]
struct ExternalProcessOutput {
    success: bool,
    stderr: String,
    timed_out: bool,
}

fn run_external_with_timeout(
    program: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
    timeout: Duration,
) -> Result<ExternalProcessOutput, io::Error> {
    let mut child = ProcessCommand::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = StdInstant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if StdInstant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut _stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut _stdout)?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    Ok(ExternalProcessOutput {
        success: status.success(),
        stderr,
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_run_cli_defaults_to_non_persistent_execution() {
        let cli = Cli::try_parse_from([
            "flux-purr",
            "ram-run",
            "preview",
            "display",
            "--port",
            "/dev/cu.test",
        ])
        .unwrap();
        let Command::RamRun {
            command: RamRunCommand::Preview(args),
        } = cli.command
        else {
            panic!("ram preview parses");
        };
        assert!(!args.options.reload);
        assert!(!args.options.install);
        assert!(!args.options.skip_backup);
        assert!(args.options.confirm.is_none());
        assert!(args.options.theme.is_none());
    }

    #[test]
    fn unknown_or_product_identity_can_never_reuse_ram_session() {
        let product = RamRuntimeIdentity {
            identity: RamIdentityObservation {
                firmware_kind: Some(flux_purr_devd::FirmwareKind::Product),
                build_id: "build-1".to_string(),
                capabilities: vec!["test_adc".to_string()],
            },
        };
        assert!(!ram_identity_matches(&product, "build-1", "test_adc"));

        let legacy = RamRuntimeIdentity {
            identity: RamIdentityObservation {
                firmware_kind: None,
                build_id: "build-1".to_string(),
                capabilities: vec!["test_adc".to_string()],
            },
        };
        assert!(!ram_identity_matches(&legacy, "build-1", "test_adc"));
    }

    #[test]
    fn malformed_or_legacy_identity_is_unknown_to_ram_probe() {
        assert!(
            decode_ram_identity_response(&json!({
                "type": "response",
                "ok": true,
                "result": {"identity": {
                    "firmwareVersion": "0.1.0",
                    "buildId": "build-1",
                    "capabilities": []
                }}
            }))
            .is_some_and(|runtime| runtime.identity.firmware_kind.is_none())
        );
        assert!(
            decode_ram_identity_response(&json!({
                "type": "response",
                "ok": true,
                "result": {"identity": {"firmwareKind": "not-a-kind"}}
            }))
            .is_none()
        );
        assert!(decode_ram_identity_response(&json!({"type": "rom-log"})).is_none());
        assert!(
            decode_ram_identity_response(&json!({
                "type": "response",
                "ok": false,
                "result": {"identity": {"firmwareKind": "ram_bringup"}}
            }))
            .is_none()
        );
    }

    #[test]
    fn hello_identity_frame_is_accepted_for_post_reset_probe() {
        let identity = decode_ram_identity_response(&json!({
            "type": "hello",
            "identity": {
                "firmwareKind": "product",
                "buildId": "build-1",
                "capabilities": ["identity"]
            }
        }))
        .expect("hello carries an identity payload");
        assert_eq!(
            identity.identity.firmware_kind,
            Some(flux_purr_devd::FirmwareKind::Product)
        );
    }

    #[test]
    fn ram_preflight_rejects_secure_or_disabled_download_without_fallback() {
        let error = classify_ram_preflight_output(
            "/dev/cu.test",
            true,
            "ROM: secure UART download enabled",
            "",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("no Flash fallback"));
        assert!(classify_ram_preflight_output("/dev/cu.test", false, "", "timeout").is_err());
    }

    #[test]
    fn ram_preflight_parses_rom_security_flags() {
        assert_eq!(parse_security_flags("Flags: 0x00000004"), Some(0x00000004));
        assert_eq!(
            parse_security_flags("  flags: 0x00000100 other"),
            Some(0x100)
        );
        let error = classify_ram_preflight_output("/dev/cu.test", true, "Flags: 0x00000004", "")
            .unwrap_err()
            .to_string();
        assert!(error.contains("secure download"));
        let error = classify_ram_preflight_output("/dev/cu.test", true, "Flags: 0x00000100", "")
            .unwrap_err()
            .to_string();
        assert!(error.contains("USB access/download"));
    }

    #[test]
    fn long_ram_commands_have_time_to_finish_before_timeout() {
        assert_eq!(ram_command_timeout("test_fan"), Duration::from_secs(30));
        assert_eq!(
            ram_command_timeout("preview_frontpanel"),
            Duration::from_secs(120)
        );
        assert_eq!(ram_command_timeout("test_adc"), Duration::from_secs(5));
    }

    #[test]
    fn ram_load_uses_pinned_espflash_rom_path() {
        let args = ram_load_args("/dev/cu.test", Path::new("bringup.elf"));
        assert_eq!(args.first().map(String::as_str), Some("flash"));
        assert!(args.windows(2).any(|pair| pair == ["--ram", "bringup.elf"]));
        assert!(args.iter().any(|value| value == "--no-stub"));
        assert!(args.windows(2).any(|pair| pair == ["--chip", "esp32s3"]));
    }

    #[test]
    fn ram_load_rejects_unpinned_espflash_versions() {
        assert!(validate_pinned_ram_espflash_version("espflash 4.5.0").is_ok());
        assert!(validate_pinned_ram_espflash_version("espflash 4.6.0").is_err());
        assert!(validate_pinned_ram_espflash_version("espflash 4.5.0\nwarning").is_err());
    }

    #[test]
    fn ram_ranges_reject_flash_mapped_segments() {
        assert!(ram_range_is_internal(0x4037_8000, 0x4037_9000));
        assert!(ram_range_is_internal(0x3fc8_8000, 0x3fc8_9000));
        assert!(!ram_range_is_internal(0x4200_0000, 0x4200_1000));
        assert!(!ram_range_is_internal(0x3c00_0000, 0x3c00_1000));
    }
}
