#[expect(
    clippy::too_many_lines,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cli = Cli::parse();
    let direct_flash_command = matches!(&cli.command, Command::Flash(_) | Command::Recover(_));
    let explicit_devd_endpoint = devd_flag_was_supplied();
    if direct_flash_command && explicit_devd_endpoint {
        return Err("flash and recover are direct-serial commands and do not accept --devd".into());
    }
    let mut managed_devd = None;
    if should_start_managed_devd(direct_flash_command, explicit_devd_endpoint) {
        let managed = ManagedDevd::start().await?;
        cli.devd = managed.endpoint.to_string_lossy().into_owned();
        managed_devd = Some(managed);
    } else if !direct_flash_command {
        validate_local_control_endpoint(&cli.devd)?;
    }
    let client = Client::new();
    let payload = match cli.command {
        Command::Devices => {
            request_json(&client, Method::GET, &cli.devd, "/api/v1/devices", None).await?
        }
        Command::Lan { command } => match command {
            LanCommand::Devices => {
                let config = read_user_config()?;
                json!({
                    "devices": config.lan_devices.iter().map(flux_purr_devd::lan::LanDeviceSummary::from).collect::<Vec<_>>()
                })
            }
            LanCommand::Refresh => {
                json!({ "devices": persist_cli_lan_discoveries(discover_mdns(Duration::from_secs(2)).await?)? })
            }
            LanCommand::Scan(args) => {
                json!({ "devices": persist_cli_lan_discoveries(discover_cidr(LanScanRequest { cidr: args.cidr }).await?)? })
            }
            LanCommand::Reset(target) => {
                request_with_lease(
                    &client,
                    resolve_target(target, &cli.devd)?,
                    Method::POST,
                    "/lan-pairing/reset",
                    None,
                )
                .await?
            }
            LanCommand::Pair(args) => {
                let device = pair_device(LanPairRequest {
                    base_url: args.base_url,
                    code: args.code,
                })
                .await?;
                let summary = flux_purr_devd::lan::LanDeviceSummary::from(&device);
                let mut config = read_user_config()?;
                merge_lan_device(&mut config.lan_devices, device);
                write_user_config(&config)?;
                serde_json::to_value(summary)?
            }
            LanCommand::PairingCode(selector) => {
                request_device_read(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    "/lan-pairing/code",
                )
                .await?
            }
            LanCommand::PairingOpen(selector) => {
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::POST,
                    "/lan-pairing/window",
                    None,
                )
                .await?
            }
            LanCommand::PairingClose(selector) => {
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::DELETE,
                    "/lan-pairing/window",
                    None,
                )
                .await?
            }
            LanCommand::Status(args) => {
                let device = resolve_lan_target(&args.id)?;
                authorized_json(&device, Method::GET, "status", None, None).await?
            }
            LanCommand::RuntimeSet(args) => {
                let device = resolve_lan_target(&args.target.id)?;
                let body = json!({
                    "targetTempC": args.target_temp_c,
                    "activeCoolingEnabled": args.active_cooling,
                    "postHeatCoolingMode": args.post_heat_cooling,
                    "heatingFanGuardMode": args.heating_fan_guard,
                    "heaterEnabled": args.heater_enabled,
                });
                lan_api_request(&device, Method::PUT, "runtime", Some(body)).await?
            }
            LanCommand::Request(args) => {
                let device = resolve_lan_target(&args.target.id)?;
                let body = match (args.body, args.body_file) {
                    (Some(value), None) => Some(serde_json::from_str(&value)?),
                    (None, Some(path)) => Some(read_json_file(&path)?),
                    (None, None) => None,
                    (Some(_), Some(_)) => unreachable!("clap rejects conflicting body arguments"),
                };
                lan_api_request(&device, args.method.as_reqwest(), &args.path, body).await?
            }
        },
        Command::Identity(selector) => {
            request_device_read(&client, resolve_target(selector, &cli.devd)?, "/identity").await?
        }
        Command::Status(selector) => {
            request_device_read(&client, resolve_target(selector, &cli.devd)?, "/status").await?
        }
        Command::Runtime { command } => match command {
            RuntimeCommand::Get(selector) => {
                request_device_read(&client, resolve_target(selector, &cli.devd)?, "/status")
                    .await?
            }
            RuntimeCommand::Set(args) => {
                let resolved = resolve_target(args.target.clone(), &cli.devd)?;
                let body = runtime_body(&client, &resolved, args).await?;
                request_with_lease(&client, resolved, Method::PUT, "/runtime", Some(body)).await?
            }
        },
        Command::Buzzer { command } => match command {
            BuzzerCommand::Test(args) => {
                let BuzzerTestArgs {
                    target,
                    cue,
                    scenario,
                    repeat,
                    stop,
                    status,
                } = args;
                buzzer_test(
                    &client,
                    resolve_target(target, &cli.devd)?,
                    cue,
                    scenario,
                    repeat,
                    stop,
                    status,
                )
                .await?
            }
            BuzzerCommand::Play(args) => {
                if cli.json {
                    return Err("buzzer play is interactive and cannot be used with --json".into());
                }
                buzzer_play_interactive(
                    &client,
                    resolve_target(args.target, &cli.devd)?,
                    args.pointer,
                )
                .await?;
                return Ok(());
            }
        },
        Command::Pd { command } => match command {
            PdCommand::Pps { command } => match command {
                PpsCommand::Set(args) => {
                    let millivolts = parse_pps_volts(&args.volts)?;
                    let mut body = json!({
                        "manualPpsEnabled": true,
                        "manualPpsMv": millivolts,
                    });
                    if let Some(amps) = &args.amps {
                        body["manualPpsMa"] = json!(parse_pps_amps(amps)?);
                    }
                    request_with_lease(
                        &client,
                        resolve_target(args.target.clone(), &cli.devd)?,
                        Method::PUT,
                        "/runtime",
                        Some(body),
                    )
                    .await?
                }
                PpsCommand::Clear(selector) => {
                    let body = json!({"manualPpsEnabled": false});
                    request_with_lease(
                        &client,
                        resolve_target(selector, &cli.devd)?,
                        Method::PUT,
                        "/runtime",
                        Some(body),
                    )
                    .await?
                }
            },
        },
        Command::Wifi { command } => match command {
            WifiCommand::Set(args) => {
                let resolved = resolve_target(args.target.clone(), &cli.devd)?;
                let static_ipv4 = static_ipv4_value(
                    args.static_ip,
                    args.static_prefix_len,
                    args.static_gateway,
                    args.static_dns,
                )?;
                let body = wifi_set_body(
                    args.ssid,
                    args.password,
                    static_ipv4,
                    args.telemetry_interval_ms,
                );
                request_with_lease(&client, resolved, Method::PUT, "/wifi", Some(body)).await?
            }
            WifiCommand::Clear(selector) => {
                let body = json!({"op": WifiConfigOp::Clear});
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::PUT,
                    "/wifi",
                    Some(body),
                )
                .await?
            }
            WifiCommand::Cancel(selector) => {
                let body = json!({"op": WifiConfigOp::Cancel});
                request_with_lease(
                    &client,
                    resolve_target(selector, &cli.devd)?,
                    Method::PUT,
                    "/wifi",
                    Some(body),
                )
                .await?
            }
        },
        Command::Calibration { command } => {
            handle_calibration_command(&client, &cli.devd, command).await?
        }
        Command::CalibrationMode { command } => {
            handle_calibration_mode_command(&client, &cli.devd, command).await?
        }
        Command::HeaterCurve { command } => {
            handle_heater_curve_command(&client, &cli.devd, command).await?
        }
        Command::Thermal { command } => handle_thermal_command(&client, &cli.devd, command).await?,
        Command::Update(args) => update_from_local_bundle(&client, &cli.devd, args).await?,
        Command::Flash(args) => direct_flash(args).await?,
        Command::Recover(args) => direct_recover(args).await?,
        Command::Eeprom { command } => handle_eeprom_command(&client, &cli.devd, command).await?,
        Command::Monitor(args) => {
            monitor_once(
                &client,
                resolve_target(args.target.clone(), &cli.devd)?,
                args.tail,
            )
            .await?
        }
        Command::Hardware { command } => {
            handle_hardware_command(&client, &cli.devd, command).await?
        }
        Command::UsbPort { command } => handle_usb_port_command(command)?,
    };

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&redact_cli_sensitive(&payload))?
        );
    } else {
        println!("{}", render_human(&payload)?);
    }
    drop(managed_devd);
    Ok(())
}

fn devd_flag_was_supplied() -> bool {
    std::env::args_os().skip(1).any(|argument| {
        let argument = argument.to_string_lossy();
        argument == "--devd" || argument.starts_with("--devd=")
    })
}

const fn should_start_managed_devd(
    direct_flash_command: bool,
    explicit_devd_endpoint: bool,
) -> bool {
    !direct_flash_command && !explicit_devd_endpoint
}

struct ManagedDevd {
    endpoint: PathBuf,
    _directory: tempfile::TempDir,
    child: Child,
}

impl Drop for ManagedDevd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn update_from_local_bundle(
    _client: &Client,
    devd: &str,
    args: UpdateArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(&args.port)?;
    if args
        .bundle
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("fluxpurr-fw")
    {
        return Err("一般用户 update requires a local .fluxpurr-fw bundle".into());
    }
    let bundle_bytes = fs::read(&args.bundle)?;
    let bundle = firmware_bundle::read_bundle_bytes(&bundle_bytes)?;
    let executable_catalog = std::env::current_exe()?.with_file_name(INTEGRITY_CATALOG_FILE);
    let catalog = firmware_bundle::read_integrity_catalog(&executable_catalog)?;
    firmware_bundle::verify_bundle_against_catalog(&bundle, &catalog)?;
    let imported =
        local_control_request_bytes(devd, "POST", "/api/v1/firmware-bundles", bundle_bytes).await?;
    if !(200..300).contains(&imported.status) {
        return Err(format!(
            "local devd bundle import failed: status={} body={}",
            imported.status, imported.body
        )
        .into());
    }
    request_json(
        _client,
        Method::POST,
        devd,
        "/api/v1/firmware-update",
        Some(json!({
            "port": args.port,
            "artifactId": bundle.bundle_sha256,
        })),
    )
    .await
}

async fn direct_flash(args: FlashArgs) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let program = resolve_espflash_program();
    direct_flash_with_program(args, &program, true)
}

fn direct_flash_with_program(
    args: FlashArgs,
    program: &Path,
    require_real_flash_enablement: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let backup_directory = if args.skip_backup {
        None
    } else {
        Some(developer_backup_directory()?)
    };
    direct_flash_with_program_inner(
        args,
        program,
        require_real_flash_enablement,
        read_eeprom_snapshot,
        detect_rom_download_mode,
        backup_directory.as_deref(),
    )
}

type SnapshotReader = fn(&str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>;
type RomProbe = fn(&str) -> bool;

fn direct_flash_with_program_inner(
    args: FlashArgs,
    program: &Path,
    require_real_flash_enablement: bool,
    snapshot_reader: SnapshotReader,
    rom_probe: RomProbe,
    backup_directory: Option<&Path>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(&args.port)?;
    if args.skip_backup && args.confirm.as_deref() != Some("NO_EEPROM_BACKUP") {
        return Err("--skip-backup requires --confirm NO_EEPROM_BACKUP".into());
    }
    let elf = args.elf.unwrap_or_else(default_release_elf);
    validate_local_elf(&elf)?;
    let partition_table = embedded_partition_table()?;
    if require_real_flash_enablement {
        ensure_real_flash_enabled()?;
    }
    if !args.skip_backup && rom_probe(&args.port) {
        return Err(
            "EEPROM backup preflight blocked: the Device is in ESP32-S3 ROM download mode and cannot serve the application EEPROM snapshot protocol. To proceed intentionally without a backup, use --skip-backup --confirm NO_EEPROM_BACKUP; firmware was not written."
                .into(),
        );
    }
    let backup_path = if args.skip_backup {
        None
    } else {
        let snapshot = match snapshot_reader(&args.port) {
            Ok(snapshot) => snapshot,
            Err(error) if snapshot_error_may_be_rom_mode(error.as_ref()) => {
                if rom_probe(&args.port) {
                    return Err(
                        "EEPROM backup preflight blocked: the Device is in ESP32-S3 ROM download mode and cannot serve the application EEPROM snapshot protocol. To proceed intentionally without a backup, use --skip-backup --confirm NO_EEPROM_BACKUP; firmware was not written."
                            .into(),
                    );
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let directory = backup_directory.ok_or("developer backup directory is unavailable")?;
        Some(developer_backup::write_atomic(directory, &snapshot)?)
    };
    let flash_args = direct_elf_flash_args(&args.port, partition_table.path(), &elf)?;
    let espflash = run_espflash_command(program, &flash_args)?;
    Ok(
        json!({"ok": true, "operation": "flash", "port": args.port, "elf": elf, "backup": backup_path, "espflash": espflash}),
    )
}

async fn direct_recover(
    args: RecoverArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(&args.port)?;
    if args.confirm != "ERASE" {
        return Err("recover requires --confirm ERASE".into());
    }
    validate_local_elf(&args.elf)?;
    let partition_table = embedded_partition_table()?;
    ensure_real_flash_enabled()?;
    let program = resolve_espflash_program();
    let erase_args = direct_erase_flash_args(&args.port);
    let erase_diagnostics = run_espflash_command(&program, &erase_args)?;
    let flash_args = direct_elf_flash_args(&args.port, partition_table.path(), &args.elf)?;
    let flash_diagnostics = run_espflash_command(&program, &flash_args)?;
    Ok(
        json!({"ok": true, "operation": "recover", "port": args.port, "elf": args.elf, "eeprom": "untouched", "espflash": {"erase": erase_diagnostics, "flash": flash_diagnostics}}),
    )
}

fn validate_serial_port(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if port.trim().is_empty()
        || port.contains("://")
        || port.starts_with("tcp:")
        || port.parse::<std::net::SocketAddr>().is_ok()
    {
        return Err("an explicit local serial --port is required".into());
    }
    Ok(())
}

fn validate_local_elf(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !path.is_file() {
        return Err(format!("local ELF does not exist: {}", path.display()).into());
    }
    if fs::read(path)?.get(0..4) != Some(b"\x7fELF") {
        return Err(format!("local artifact is not an ELF: {}", path.display()).into());
    }
    Ok(())
}

fn embedded_partition_table()
-> Result<tempfile::NamedTempFile, Box<dyn std::error::Error + Send + Sync>> {
    let mut file = tempfile::Builder::new()
        .prefix("flux-purr-partitions-")
        .suffix(".csv")
        .tempfile()?;
    file.write_all(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../firmware/partitions.csv"
    )))?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn direct_elf_flash_args(
    port: &str,
    partition_table: &Path,
    elf: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(vec![
        "flash".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port.into(),
        "--non-interactive".into(),
        "--after".into(),
        "hard-reset".into(),
        "--partition-table".into(),
        partition_table
            .to_str()
            .ok_or("invalid partition table path")?
            .into(),
        elf.to_str().ok_or("invalid ELF path")?.into(),
    ])
}

fn direct_erase_flash_args(port: &str) -> Vec<String> {
    vec![
        "erase-flash".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port.into(),
        "--non-interactive".into(),
        "--after".into(),
        "no-reset".into(),
    ]
}

fn ensure_real_flash_enabled() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::var("FLUX_PURR_DEVD_ALLOW_REAL_FLASH").as_deref() != Ok("1") {
        return Err(
            "real flashing is disabled; set FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1 only with explicit port authorization"
                .into(),
        );
    }
    Ok(())
}

fn default_release_elf() -> PathBuf {
    flux_purr_repo_root().join("firmware/target/xtensa-esp32s3-none-elf/release/flux-purr")
}

fn resolve_espflash_program() -> PathBuf {
    std::env::var_os("FLUX_PURR_ESPFLASH")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("espflash"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EspflashDiagnostics {
    command: String,
    success: bool,
    exit_code: Option<i32>,
    phase: String,
    phases: Vec<String>,
    diagnosis: String,
    hint: String,
    stdout: String,
    stderr: String,
}

fn run_espflash_command(
    program: &Path,
    args: &[String],
) -> Result<EspflashDiagnostics, Box<dyn std::error::Error + Send + Sync>> {
    let output = ProcessCommand::new(program).args(args).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let diagnostics = classify_espflash_diagnostics(
        args.first().map(String::as_str).unwrap_or("unknown"),
        output.status.code(),
        &stdout,
        &stderr,
    );
    if !output.status.success() {
        return Err(format_espflash_failure(&diagnostics).into());
    }
    Ok(diagnostics)
}

fn classify_espflash_diagnostics(
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> EspflashDiagnostics {
    let mut phases = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let Some(phase) = espflash_phase_for_line(line) else {
            continue;
        };
        if phases.last().map(String::as_str) != Some(phase) {
            phases.push(phase.to_string());
        }
    }
    let success = exit_code == Some(0);
    let phase = if success {
        "complete".to_string()
    } else {
        phases
            .last()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    };
    let diagnosis = espflash_diagnosis_for(&phase, success);
    let hint = espflash_hint(&phase, success);
    EspflashDiagnostics {
        command: command.to_string(),
        success,
        exit_code,
        phase,
        phases,
        diagnosis,
        hint,
        stdout: truncate_espflash_output(stdout),
        stderr: truncate_espflash_output(stderr),
    }
}

fn espflash_phase_for_line(line: &str) -> Option<&'static str> {
    let line = line.to_ascii_lowercase();
    if line.contains("flashend")
        || line.contains("bootloader returned an error")
        || line.contains("flash end")
        || line.contains("reboot")
    {
        return Some("finalize");
    }
    if line.contains("hash of data verified")
        || line.contains("verify")
        || line.contains("checksum")
    {
        return Some("verify");
    }
    if line.contains("writing") || line.contains("write segment") {
        return Some("write");
    }
    if line.contains("erasing") || line.contains("erase flash") {
        return Some("erase");
    }
    if line.contains("connect")
        || line.contains("chip type")
        || line.contains("packet header")
        || line.contains("serial")
    {
        return Some("connect");
    }
    None
}

fn espflash_diagnosis_for(phase: &str, success: bool) -> String {
    if success {
        return "completed".to_string();
    }
    match phase {
        "connect" => "connection".to_string(),
        "erase" | "write" => "flash_write".to_string(),
        "verify" => "verification".to_string(),
        "finalize" => "flash_finalization".to_string(),
        _ => "unknown".to_string(),
    }
}

fn espflash_hint(phase: &str, success: bool) -> String {
    if success {
        return "espflash reported completion for all observed stages".to_string();
    }
    match phase {
        "connect" => {
            "check ESP32-S3 boot mode/download mode, the explicit serial link, USB cable/driver, and board power"
                .to_string()
        }
        "erase" | "write" => {
            "check SPI flash power, wiring, erase/write protection, and supply stability"
                .to_string()
        }
        "verify" => {
            "flash contents did not verify; check SPI flash integrity, power stability, and image layout"
                .to_string()
        }
        "finalize" => {
            "ROM rejected flash finalization or reset; the image may have been written, but write completeness is unconfirmed; check boot strap, reset circuit, USB link, and power"
                .to_string()
        }
        _ => "inspect the preserved espflash output; the failure phase was not identified".to_string(),
    }
}

fn truncate_espflash_output(output: &str) -> String {
    const MAX_OUTPUT_BYTES: usize = 16 * 1024;
    if output.len() <= MAX_OUTPUT_BYTES {
        return output.to_string();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n<output truncated>", &output[..end])
}

fn format_espflash_failure(diagnostics: &EspflashDiagnostics) -> String {
    format!(
        "espflash `{}` failed at phase `{}` (exit_code={:?} diagnosis={}): {}\nphases: {}\nstdout:\n{}\nstderr:\n{}",
        diagnostics.command,
        diagnostics.phase,
        diagnostics.exit_code,
        diagnostics.diagnosis,
        diagnostics.hint,
        if diagnostics.phases.is_empty() {
            "<none>".to_string()
        } else {
            diagnostics.phases.join(" -> ")
        },
        if diagnostics.stdout.is_empty() {
            "<empty>"
        } else {
            &diagnostics.stdout
        },
        if diagnostics.stderr.is_empty() {
            "<empty>"
        } else {
            &diagnostics.stderr
        },
    )
}

fn developer_backup_directory() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    Ok(flux_purr_devd::user_config_dir()?.join("developer-flash-backups"))
}

fn read_eeprom_snapshot(port: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    match read_eeprom_snapshot_protocol(port) {
        Ok(snapshot) => Ok(snapshot),
        Err(error) if snapshot_protocol_compatibility_fallback(error.as_ref()) => {
            read_legacy_eeprom_snapshot(port)
        }
        Err(error) => Err(error),
    }
}

fn snapshot_protocol_compatibility_fallback(error: &dyn std::error::Error) -> bool {
    error
        .to_string()
        .contains("none matched the EEPROM snapshot request")
}

fn read_eeprom_snapshot_protocol(
    port: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    const SNAPSHOT_SESSION_TIMEOUT: Duration = Duration::from_secs(30);
    let mut serial = serialport::new(port, 115_200)
        .timeout(Duration::from_secs(2))
        .open()?;
    let deadline = StdInstant::now() + SNAPSHOT_SESSION_TIMEOUT;
    let session_id = format!("snapshot-{}", current_unix_millis());
    let open =
        json!({"op":"eeprom_snapshot_open","requestId":session_id,"capacity":8192,"chunkMax":32});
    write_snapshot_request(&mut *serial, &open)?;
    let open_response = read_snapshot_response(&mut *serial, &session_id, deadline)?;
    if open_response.get("capacity").and_then(Value::as_u64) != Some(8192)
        || open_response.get("chunkMax").and_then(Value::as_u64) != Some(32)
    {
        return Err("EEPROM snapshot negotiated an invalid capacity or chunk size".into());
    }
    let mut snapshot = Vec::with_capacity(8192);
    for offset in (0..8192_u32).step_by(32) {
        write_snapshot_request(
            &mut *serial,
            &json!({"op":"eeprom_snapshot_read","requestId":session_id,"offset":offset,"length":32}),
        )?;
        let response = read_snapshot_response(&mut *serial, &session_id, deadline)?;
        if response.get("offset").and_then(Value::as_u64) != Some(u64::from(offset)) {
            return Err("snapshot response returned an unexpected offset".into());
        }
        let bytes = response
            .get("bytes")
            .and_then(Value::as_array)
            .ok_or("snapshot response missing bytes")?;
        if bytes.len() != 32 {
            return Err("snapshot response returned an invalid chunk".into());
        }
        for byte in bytes {
            let value = byte
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or("snapshot byte is not an octet")?;
            snapshot.push(value);
        }
    }
    let digest = format!("sha256:{:x}", Sha256::digest(&snapshot));
    write_snapshot_request(
        &mut *serial,
        &json!({"op":"eeprom_snapshot_close","requestId":session_id,"sha256":digest}),
    )?;
    let response = read_snapshot_response(&mut *serial, &session_id, deadline)?;
    if response.get("sha256").and_then(Value::as_str) != Some(digest.as_str()) {
        return Err("EEPROM snapshot hash verification failed".into());
    }
    Ok(snapshot)
}

fn read_legacy_eeprom_snapshot(
    port: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    const LEGACY_SESSION_TIMEOUT: Duration = Duration::from_secs(30);
    let mut serial = serialport::new(port, 115_200)
        .timeout(Duration::from_secs(2))
        .open()?;
    let deadline = StdInstant::now() + LEGACY_SESSION_TIMEOUT;
    let session_id = format!("eeprom-legacy-{}", current_unix_millis());
    let mut image = Vec::with_capacity(EEPROM_CAPACITY_BYTES);

    for offset in (0..EEPROM_CAPACITY_BYTES as u32).step_by(EEPROM_CHUNK_BYTES) {
        let length = (EEPROM_CAPACITY_BYTES - offset as usize).min(EEPROM_CHUNK_BYTES);
        let request = json!({
            "type": "eeprom_maintenance",
            "requestId": session_id.clone(),
            "op": "read",
            "offset": offset,
            "length": length,
        });
        write_snapshot_request(&mut *serial, &request)?;
        let response = read_snapshot_response(&mut *serial, &session_id, deadline)?;
        let bytes = response
            .get("result")
            .and_then(|result| result.get("eeprom_bytes"))
            .and_then(Value::as_array)
            .ok_or("legacy EEPROM response did not include bytes")?;
        if bytes.len() != length {
            return Err(format!(
                "legacy EEPROM read returned {} bytes, expected {length}",
                bytes.len()
            )
            .into());
        }
        for byte in bytes {
            let value = byte
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or("legacy EEPROM response contained a non-octet")?;
            image.push(value);
        }
    }

    Ok(image)
}

fn write_snapshot_request(
    serial: &mut dyn serialport::SerialPort,
    value: &Value,
) -> io::Result<()> {
    serial.write_all(serde_json::to_string(value).unwrap().as_bytes())?;
    serial.write_all(b"\n")?;
    serial.flush()
}

#[derive(Debug, Default)]
struct SnapshotResponseObservation {
    nonempty_lines: u16,
    json_lines: u16,
}

fn snapshot_timeout_error(observation: &SnapshotResponseObservation) -> io::Error {
    let message = match (observation.nonempty_lines, observation.json_lines) {
        (0, _) => {
            "EEPROM backup preflight failed: no USB JSONL response from the Device. The Device application may be stopped, in ROM download mode, or unreachable through this USB data path; EEPROM health is unknown and firmware was not written."
        }
        (_, 0) => {
            "EEPROM backup preflight failed: the Device emitted non-JSON serial output but did not acknowledge the EEPROM snapshot request. Its application firmware does not provide the required USB JSONL snapshot protocol; EEPROM health is unknown and firmware was not written."
        }
        _ => {
            "EEPROM backup preflight failed: the Device emitted USB JSONL responses but none matched the EEPROM snapshot request. Its application firmware is incompatible with the required snapshot protocol; EEPROM health is unknown and firmware was not written."
        }
    };
    io::Error::new(io::ErrorKind::TimedOut, message)
}

fn snapshot_rejection_error(error_code: &str) -> io::Error {
    let message = match error_code {
        "heater_active" => {
            "EEPROM backup preflight blocked: the Device reports active heater output. Disable heating before developer flash; firmware was not written."
        }
        "eeprom_unavailable" => {
            "EEPROM backup preflight failed: the Device reports that the external M24C64 EEPROM is not detected. Inspect EEPROM population, power, and I2C wiring; firmware was not written."
        }
        "eeprom_read_failed" => {
            "EEPROM backup preflight failed: the Device reports an M24C64 EEPROM read failure. Inspect EEPROM power and I2C signal integrity; firmware was not written."
        }
        "snapshot_hash_mismatch" => {
            "EEPROM backup preflight failed: the Device reports an EEPROM snapshot hash mismatch. Inspect M24C64 and I2C stability; firmware was not written."
        }
        "snapshot_session_invalid"
        | "snapshot_range_required"
        | "snapshot_range_invalid"
        | "snapshot_incomplete"
        | "session_required"
        | "malformed_snapshot"
        | "snapshot_op_unsupported"
        | "output_too_small" => {
            "EEPROM backup preflight failed: the Device rejected the snapshot protocol. Its application firmware is incompatible with this developer flash flow; EEPROM health is unknown and firmware was not written."
        }
        "digest_format_failed" => {
            "EEPROM backup preflight failed: the Device could not format the EEPROM snapshot digest. Its application firmware requires diagnosis; firmware was not written."
        }
        _ => {
            "EEPROM backup preflight failed: the Device returned an unrecognized snapshot error. EEPROM health is unknown and firmware was not written."
        }
    };
    io::Error::other(message)
}

fn snapshot_error_may_be_rom_mode(error: &dyn std::error::Error) -> bool {
    let message = error.to_string();
    message.contains("no USB JSONL response") || message.contains("non-JSON serial output")
}

fn detect_rom_download_mode(port: &str) -> bool {
    let program = resolve_espflash_program();
    ProcessCommand::new(program)
        .args(rom_download_probe_args(port))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn rom_download_probe_args(port: &str) -> Vec<String> {
    vec![
        "--skip-update-check".into(),
        "board-info".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port.into(),
        "--before".into(),
        "no-reset".into(),
        "--after".into(),
        "no-reset".into(),
        "--no-stub".into(),
        "--non-interactive".into(),
    ]
}

fn read_snapshot_response<R: Read + ?Sized>(
    serial: &mut R,
    request_id: &str,
    deadline: StdInstant,
) -> io::Result<Value> {
    let mut observation = SnapshotResponseObservation::default();
    loop {
        if StdInstant::now() >= deadline {
            return Err(snapshot_timeout_error(&observation));
        }
        let mut bytes = Vec::new();
        loop {
            if StdInstant::now() >= deadline {
                return Err(snapshot_timeout_error(&observation));
            }
            let mut byte = [0_u8; 1];
            match serial.read(&mut byte) {
                Ok(1) if byte[0] == b'\n' => break,
                Ok(1) => bytes.push(byte[0]),
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                    return Err(snapshot_timeout_error(&observation));
                }
                Err(error) => return Err(error),
            }
        }
        if bytes.iter().any(|byte| !byte.is_ascii_whitespace()) {
            observation.nonempty_lines = observation.nonempty_lines.saturating_add(1);
        }
        let value = match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => {
                observation.json_lines = observation.json_lines.saturating_add(1);
                value
            }
            Err(_) => continue,
        };
        if value.get("requestId").and_then(Value::as_str) != Some(request_id) {
            continue;
        }
        if value.get("ok").and_then(Value::as_bool) != Some(true) {
            let error_code = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(snapshot_rejection_error(error_code));
        }
        return Ok(value);
    }
}

impl ManagedDevd {
    async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let directory = tempfile::Builder::new()
            .prefix("flux-purr-devd-")
            .tempdir()?;
        let endpoint = directory.path().join("control.sock");
        let sibling = std::env::current_exe()?.with_file_name("flux-purr-devd");
        let (program, prefix_args): (PathBuf, Vec<String>) = if sibling.is_file() {
            (sibling, Vec::new())
        } else {
            (
                PathBuf::from("cargo"),
                vec![
                    "run".into(),
                    "--quiet".into(),
                    "--manifest-path".into(),
                    env!("CARGO_MANIFEST_DIR").into(),
                    "--bin".into(),
                    "flux-purr-devd".into(),
                    "--".into(),
                ],
            )
        };
        let child = ProcessCommand::new(program)
            .args(prefix_args)
            .args(["serve", "--control-socket"])
            .arg(&endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !endpoint.exists() {
            if tokio::time::Instant::now() >= deadline {
                return Err("managed devd did not create its local control socket".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok(Self {
            endpoint,
            _directory: directory,
            child,
        })
    }
}

async fn request_json(
    _client: &Client,
    method: Method,
    base: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    // Existing in-process command tests use an HTTP mock server. This branch is
    // compiled only for tests; production CLI requests always use local CBOR.
    #[cfg(test)]
    if base.starts_with("http://") || base.starts_with("https://") {
        let mut url = Url::parse(base)?;
        let (request_path, query) = path.split_once('?').unwrap_or((path, ""));
        url.set_path(request_path);
        url.set_query((!query.is_empty()).then_some(query));
        let mut request = _client.request(method, url);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let response_body = response.text().await?;
        if !status.is_success() {
            return Err(format!("HTTP {status} body={response_body}").into());
        }
        return Ok(serde_json::from_str(&response_body)?);
    }

    let response = local_control_request(base, method.as_str(), path, body).await?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "local devd request failed: status={} body={}",
            response.status, response.body
        )
        .into());
    }
    Ok(response.body)
}

const EEPROM_CAPACITY_BYTES: usize = 8 * 1024;
const EEPROM_CHUNK_BYTES: usize = 32;

#[expect(
    clippy::too_many_lines,
    clippy::excessive_nesting,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
async fn handle_eeprom_command(
    client: &Client,
    devd: &str,
    command: EepromCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        EepromCommand::Export(args) => {
            let resolved = resolve_target(args.target, devd)?;
            let lease = create_lease(client, &resolved).await?;
            let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
            let result = async {
                let mut image = Vec::with_capacity(EEPROM_CAPACITY_BYTES);
                for offset in (0..EEPROM_CAPACITY_BYTES).step_by(EEPROM_CHUNK_BYTES) {
                    let length = (EEPROM_CAPACITY_BYTES - offset).min(EEPROM_CHUNK_BYTES);
                    let value = request_leased(
                        client,
                        &resolved,
                        &lease.lease_id,
                        Method::POST,
                        "/eeprom",
                        Some(json!({
                            "op": "read",
                            "offset": offset,
                            "length": length,
                        })),
                    )
                    .await?;
                    let bytes: Vec<u8> =
                        serde_json::from_value(value.get("bytes").cloned().unwrap_or(Value::Null))?;
                    if bytes.len() != length {
                        return Err(format!(
                            "EEPROM read returned {} bytes, expected {length}",
                            bytes.len()
                        )
                        .into());
                    }
                    image.extend_from_slice(&bytes);
                }
                fs::write(&args.output, &image)?;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(json!({
                    "path": args.output,
                    "bytes": image.len(),
                }))
            }
            .await;
            let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
            heartbeat.abort();
            result
        }
        EepromCommand::Import(args) => {
            let image = fs::read(&args.input)?;
            if image.len() != EEPROM_CAPACITY_BYTES {
                return Err(
                    format!("EEPROM image must be exactly {EEPROM_CAPACITY_BYTES} bytes").into(),
                );
            }
            let resolved = resolve_target(args.target, devd)?;
            let lease = create_lease(client, &resolved).await?;
            let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
            let result = async {
                for (offset, chunk) in image.chunks(EEPROM_CHUNK_BYTES).enumerate() {
                    request_leased(
                        client,
                        &resolved,
                        &lease.lease_id,
                        Method::POST,
                        "/eeprom",
                        Some(json!({
                            "op": "write",
                            "offset": offset * EEPROM_CHUNK_BYTES,
                            "bytes": chunk,
                        })),
                    )
                    .await?;
                }
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(json!({
                    "bytes": image.len(),
                    "rebootRequired": true,
                }))
            }
            .await;
            let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
            heartbeat.abort();
            result
        }
        EepromCommand::Erase(args) => {
            if args.confirm != "ERASE EEPROM" {
                return Err("erase requires --confirm 'ERASE EEPROM'".into());
            }
            let resolved = resolve_target(args.target, devd)?;
            let lease = create_lease(client, &resolved).await?;
            let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
            let result = async {
                request_leased(
                    client,
                    &resolved,
                    &lease.lease_id,
                    Method::POST,
                    "/eeprom",
                    Some(json!({ "op": "erase" })),
                )
                .await?;
                for offset in (0..EEPROM_CAPACITY_BYTES).step_by(EEPROM_CHUNK_BYTES) {
                    let value = request_leased(
                        client,
                        &resolved,
                        &lease.lease_id,
                        Method::POST,
                        "/eeprom",
                        Some(json!({
                            "op": "read",
                            "offset": offset,
                            "length": EEPROM_CHUNK_BYTES,
                        })),
                    )
                    .await?;
                    let bytes: Vec<u8> =
                        serde_json::from_value(value.get("bytes").cloned().unwrap_or(Value::Null))?;
                    if bytes.len() != EEPROM_CHUNK_BYTES || bytes.iter().any(|byte| *byte != 0xff) {
                        return Err(
                            format!("EEPROM erase verification failed at offset {offset}").into(),
                        );
                    }
                }
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(json!({
                    "erased": true,
                    "bytes": EEPROM_CAPACITY_BYTES,
                    "rebootRequired": true,
                }))
            }
            .await;
            let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
            heartbeat.abort();
            result
        }
    }
}

async fn request_with_lease(
    client: &Client,
    resolved: ResolvedUsbTarget,
    method: Method,
    suffix: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = request_leased(client, &resolved, &lease.lease_id, method, suffix, body).await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let value = result?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(value)
}

async fn request_device_read(
    client: &Client,
    resolved: ResolvedUsbTarget,
    suffix: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    request_with_lease(client, resolved, Method::GET, suffix, None).await
}

async fn request_leased(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    method: Method,
    suffix: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let path = format!(
        "/api/v1/devices/{}{}",
        encode_path_segment(&resolved.device),
        suffix
    );
    let mut path = path.to_string();
    if body.is_none() {
        // Lease-bearing control endpoints can be GET, POST, or DELETE. A
        // body-less DELETE still needs the same query lease as a body-less
        // read, otherwise the daemon correctly rejects the operation.
        path.push_str("?lease_id=");
        path.push_str(lease_id);
    }
    let mut body = body;
    if let Some(body_value) = body.as_mut()
        && let Some(object) = body_value.as_object_mut()
    {
        object.insert("leaseId".to_string(), Value::String(lease_id.to_string()));
    }
    request_json(client, method, &resolved.devd, &path, body).await
}

async fn request_thermal_status_with_retry(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    request_thermal_status_with_retry_config(
        client,
        resolved,
        lease_id,
        Duration::from_millis(THERMAL_STATUS_REQUEST_TIMEOUT_MS),
        THERMAL_STATUS_REQUEST_RETRY_ATTEMPTS,
    )
    .await
}

async fn request_thermal_status_with_retry_config(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    timeout: Duration,
    max_attempts: usize,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let attempts = max_attempts.max(1);
    let timeout_ms = timeout.as_millis();
    let mut last_error = "thermal status read did not start".to_string();
    for attempt in 0..attempts {
        match tokio::time::timeout(
            timeout,
            request_leased(client, resolved, lease_id, Method::GET, "/status", None),
        )
        .await
        {
            Ok(Ok(status)) => return Ok(status),
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => {
                last_error = format!("thermal /status timed out after {timeout_ms}ms");
            }
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(Duration::from_millis(
                THERMAL_STATUS_REQUEST_RETRY_BACKOFF_MS * (attempt as u64 + 1),
            ))
            .await;
        }
    }
    Err(format!("thermal /status failed after {attempts} attempt(s): {last_error}").into())
}

const THERMAL_RUNTIME_WRITE_RETRY_ATTEMPTS: usize = 2;
const THERMAL_RUNTIME_WRITE_RETRY_BACKOFF_MS: u64 = 150;

async fn request_thermal_runtime_with_retry(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    body: Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let attempts = THERMAL_RUNTIME_WRITE_RETRY_ATTEMPTS.max(1);
    let mut last_error = "thermal /runtime write did not start".to_string();
    for attempt in 0..attempts {
        match request_leased(
            client,
            resolved,
            lease_id,
            Method::PUT,
            "/runtime",
            Some(body.clone()),
        )
        .await
        {
            Ok(status) => return Ok(status),
            Err(error) => {
                last_error = error.to_string();
                if attempt + 1 >= attempts
                    || !thermal_retryable_runtime_write_error_message(&last_error)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(
                    THERMAL_RUNTIME_WRITE_RETRY_BACKOFF_MS * (attempt as u64 + 1),
                ))
                .await;
            }
        }
    }
    Err(format!("thermal /runtime write failed after {attempts} attempt(s): {last_error}").into())
}

fn thermal_retryable_runtime_write_error_message(message: &str) -> bool {
    message.contains("usb_response_timeout")
        || (message.contains("\"code\":\"serial_io_failed\"")
            && (message.contains("Broken pipe")
                || message.contains("broken pipe")
                || message.contains("Connection reset")
                || message.contains("Connection aborted")
                || message.contains("UnexpectedEof")
                || message.contains("Device not configured")
                || message.contains("device not configured")))
}
