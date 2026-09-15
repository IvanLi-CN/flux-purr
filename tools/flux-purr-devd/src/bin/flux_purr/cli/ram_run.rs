use super::*;

#[cfg(target_os = "macos")]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(target_os = "macos")]
const MACOS_O_NONBLOCK: i32 = 0x0004;

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
    if !reusable {
        ram_download_preflight(&options.port)?;
        run_espflash_command(
            &resolve_espflash_program(),
            &ram_load_args(&options.port, &elf),
        )?;
    }

    let identity = wait_for_ram_identity(&options.port, &expected_build_id)?;
    ensure_ram_capability(&identity, command)?;
    let response = send_ram_command(&options.port, command, options.theme)?;
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
    validate_local_elf(path)
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
        "--no-stub".into(),
        "--ram".into(),
        "--non-interactive".into(),
        elf.to_string_lossy().into_owned(),
    ]
}

fn ram_download_preflight(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = run_external_with_timeout(
        &resolve_espflash_program(),
        &ram_download_probe_args(port),
        Duration::from_secs(30),
    )?;
    classify_ram_preflight_output(
        port,
        output.success && !output.timed_out,
        &output.stdout,
        &output.stderr,
    )
}

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

fn parse_security_flags(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let line = line.trim().to_ascii_lowercase();
        let value = line.strip_prefix("flags:")?;
        let token = value.split_whitespace().next()?;
        u32::from_str_radix(token.trim_start_matches("0x"), 16).ok()
    })
}

fn ram_download_probe_args(port: &str) -> Vec<String> {
    vec![
        "--skip-update-check".into(),
        "board-info".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port.into(),
        "--before".into(),
        "usb-reset".into(),
        "--after".into(),
        "no-reset".into(),
        "--no-stub".into(),
        "--non-interactive".into(),
    ]
}

fn probe_ram_identity(
    port: &str,
) -> Result<Option<RamRuntimeIdentity>, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-preflight-{}", current_unix_millis());
    let request = json!({ "type": "request", "requestId": request_id, "op": "get_identity" });
    let Some(response) = exchange_jsonl(port, &request, Duration::from_millis(900))? else {
        return Ok(None);
    };
    Ok(decode_ram_identity_response(&response))
}

fn decode_ram_identity_response(response: &Value) -> Option<RamRuntimeIdentity> {
    if response.get("type").and_then(Value::as_str) != Some("response")
        || response.get("ok").and_then(Value::as_bool) != Some(true)
    {
        return None;
    }
    let identity = response.get("result")?.get("identity")?.clone();
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

fn wait_for_product_identity(
    port: &str,
) -> Result<flux_purr_devd::FirmwareKind, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = StdInstant::now() + Duration::from_secs(5);
    loop {
        if let Some(identity) = probe_ram_identity(port)?
            && let Some(kind) = identity.identity.firmware_kind
        {
            if kind == flux_purr_devd::FirmwareKind::Product {
                return Ok(kind);
            }
        }
        if StdInstant::now() >= deadline {
            return Err("reset completed but application identity is unknown; Product Firmware was not assumed".into());
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
    let response = exchange_jsonl(port, &request, ram_command_timeout(command))?
        .ok_or_else(|| format!("RAM Bring-up command {command} returned no JSONL response"))?;
    if response.get("type").and_then(Value::as_str) != Some("response")
        || response.get("ok").and_then(Value::as_bool) != Some(true)
    {
        return Err(
            format!("RAM Bring-up command {command} returned an unsuccessful response").into(),
        );
    }
    Ok(response)
}

fn ram_command_timeout(command: &str) -> Duration {
    match command {
        "preview_display" | "preview_status_light" | "test_fan" => Duration::from_secs(30),
        "preview_frontpanel" => Duration::from_secs(60),
        _ => Duration::from_secs(5),
    }
}

trait RamJsonlSerial: Read + Write {}
impl<T: Read + Write + ?Sized> RamJsonlSerial for T {}

fn open_ram_jsonl_serial(port: &str) -> io::Result<Box<dyn RamJsonlSerial>> {
    #[cfg(target_os = "macos")]
    if port.starts_with("/dev/cu.usbmodem") {
        let file = File::options()
            .read(true)
            .write(true)
            .custom_flags(MACOS_O_NONBLOCK)
            .open(port)?;
        return Ok(Box::new(file));
    }
    serialport::new(port, 115_200)
        .timeout(Duration::from_millis(100))
        .open()
        .map(|port| Box::new(port) as Box<dyn RamJsonlSerial>)
        .map_err(io::Error::other)
}

fn exchange_jsonl(
    port: &str,
    request: &Value,
    timeout: Duration,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = request
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut serial = open_ram_jsonl_serial(port)?;
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
                if value
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
    stdout: String,
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
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)?;
    }
    Ok(ExternalProcessOutput {
        success: status.success(),
        stdout,
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
    fn ram_load_is_explicitly_non_persistent() {
        let args = ram_load_args("/dev/cu.test", Path::new("bringup.elf"));
        assert!(args.iter().any(|arg| arg == "--ram"));
        assert!(args.iter().any(|arg| arg == "--no-stub"));
        assert!(!args.iter().any(|arg| arg == "--partition-table"));
        assert!(!args.iter().any(|arg| arg == "hard-reset"));
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
            Duration::from_secs(60)
        );
        assert_eq!(ram_command_timeout("test_adc"), Duration::from_secs(5));
    }
}
