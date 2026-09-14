fn resolve_artifact_path(root: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(root) = root {
        root.join(path)
    } else {
        path
    }
}

fn resolve_verified_artifact_path(root: Option<&Path>, path: &str) -> io::Result<PathBuf> {
    let relative = PathBuf::from(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact paths must stay inside the configured artifact root",
        ));
    }

    let base = fs::canonicalize(root.unwrap_or_else(|| Path::new(".")))?;
    let candidate = fs::canonicalize(base.join(relative))?;
    if !candidate.starts_with(&base) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact path must stay inside the configured artifact root",
        ));
    }

    Ok(candidate)
}

async fn run_espflash_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        build_espflash_args_with_reset_mode(artifact, root, port_path, before_reset)
    })
    .await
}

#[expect(
    clippy::excessive_nesting,
    reason = "reset fallback preserves ordered retry and recovery semantics"
)]
async fn run_espflash_with_reset_fallback_with_program<F>(
    program: &Path,
    artifact: &FirmwareArtifact,
    port_path: &str,
    build_commands: F,
) -> Result<(), HttpError>
where
    F: Fn(&str) -> Result<Vec<Vec<String>>, HttpError>,
{
    let reset_modes = espflash_reset_modes(artifact, port_path);
    for (mode_index, before_reset) in reset_modes.iter().enumerate() {
        let commands = build_commands(before_reset)?;
        let mut retry_with_next_reset = false;

        for args in commands {
            let output =
                run_espflash_command_with_timeout(program, &args, ESPFLASH_COMMAND_TIMEOUT).await?;

            if !output.status.success() {
                if espflash_flash_end_requires_reset(&args, &output) {
                    let reset_args = build_espflash_reset_args(artifact, port_path, before_reset)?;
                    let reset_output = run_espflash_command_with_timeout(
                        program,
                        &reset_args,
                        ESPFLASH_COMMAND_TIMEOUT,
                    )
                    .await?;

                    if reset_output.status.success() {
                        // The ROM accepted the image data but rejected the final
                        // run-user-code transition. Reset once, then let the caller
                        // require the normal runtime-ready verification.
                        return Ok(());
                    }

                    return Err(HttpError::internal_with_details(
                        "flash_recovery_reset_failed",
                        "espflash reached FlashEnd but the recovery reset failed.",
                        json!({
                            "flashAttempt": espflash_failure_details(program, &args, &output),
                            "resetAttempt": espflash_failure_details(program, &reset_args, &reset_output),
                        }),
                    ));
                }
                retry_with_next_reset =
                    mode_index + 1 < reset_modes.len() && espflash_connection_failed(&output);
                if retry_with_next_reset {
                    if is_esp_usb_serial_jtag_port(port_path) {
                        tokio::time::sleep(ESPFLASH_USB_RESET_RETRY_DELAY).await;
                    }
                    break;
                }
                return Err(HttpError::internal_with_details(
                    "flash_tool_failed",
                    "espflash returned a non-zero status.",
                    espflash_failure_details(program, &args, &output),
                ));
            }
        }

        if !retry_with_next_reset {
            return Ok(());
        }
    }

    Err(HttpError::internal(
        "espflash did not complete a reset attempt.",
    ))
}

async fn run_espflash_command_with_timeout(
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Output, HttpError> {
    let mut command = Command::new(program);
    command.args(args).kill_on_drop(true);
    tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            HttpError::internal_with_details(
                "flash_tool_timeout",
                "espflash did not finish before the command deadline.",
                json!({
                    "program": program,
                    "args": args,
                    "timeoutMs": timeout.as_millis(),
                }),
            )
        })?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_unavailable",
                "Failed to start espflash.",
                json!({
                    "program": program,
                    "error": error.to_string(),
                }),
            )
        })
}

fn build_espflash_reset_args(
    artifact: &FirmwareArtifact,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    Ok(vec![
        "reset".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "hard-reset".to_string(),
        "--non-interactive".to_string(),
    ])
}

fn build_bundle_write_bin_args(
    common: &[String],
    before_reset: &str,
    address: u64,
    input_path: &Path,
) -> Vec<String> {
    let mut args = vec!["write-bin".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        input_path.to_string_lossy().into_owned(),
    ]);
    args
}

async fn run_flash_transaction_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    run_espflash_with_program(artifact, root, port_path, program).await
}

fn resolve_espflash_program() -> PathBuf {
    if let Some(program) = env::var_os("FLUX_PURR_ESPFLASH").filter(|value| !value.is_empty()) {
        return PathBuf::from(program);
    }
    if let Some(cargo_home) = env::var_os("CARGO_HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(cargo_home).join("bin").join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("espflash")
}

fn espflash_failure_details(program: &Path, args: &[String], output: &Output) -> Value {
    json!({
        "program": program,
        "args": args,
        "exitCode": output.status.code(),
        "stdout": bounded_espflash_output(&output.stdout),
        "stderr": bounded_espflash_output(&output.stderr),
    })
}

fn espflash_connection_failed(output: &Output) -> bool {
    espflash_connection_failure_text(&bounded_espflash_output(&output.stderr))
        || espflash_connection_failure_text(&bounded_espflash_output(&output.stdout))
}

fn espflash_flash_end_requires_reset(args: &[String], output: &Output) -> bool {
    args.first().map(String::as_str) == Some("flash")
        && bounded_espflash_output(&output.stderr).contains("Error while running FlashEnd command")
}

fn espflash_connection_failure_text(output: &str) -> bool {
    let output = output.to_ascii_lowercase();
    output.contains("failed to connect to the device")
        || output.contains("error while connecting to device")
        || output.contains("no such device or address")
        || output.contains("broken pipe")
}

fn bounded_espflash_output(bytes: &[u8]) -> String {
    const MAX_OUTPUT_BYTES: usize = 4_096;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.trim().to_string();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{} [truncated]", text[..end].trim())
}

async fn run_espflash_with_exclusive_serial(
    state: &AppState,
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<(), HttpError> {
    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let program = resolve_espflash_program();
    run_flash_transaction_with_program(artifact, root, port_path, &program).await
}

async fn acquire_serial_rpc_with_timeout(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    timeout: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>, HttpError> {
    tokio::time::timeout(timeout, serial_rpc.lock_owned())
        .await
        .map_err(|_| {
            HttpError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "serial_lock_timeout",
                "Timed out waiting for exclusive USB serial access.",
                true,
            )
        })
}

fn drop_cached_serial_session(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<(), HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    serial_sessions.remove(port_path);
    Ok(())
}

fn build_espflash_args_with_reset_mode(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<Vec<String>>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    let partition_table = firmware_partition_table_path(root)?;
    if let Some(elf_image) = artifact.files.iter().find(|file| file.kind == "elf") {
        let path = resolve_artifact_path(root, &elf_image.path);
        let mut args = vec![
            "flash".to_string(),
            "--chip".to_string(),
            artifact.target_chip.clone(),
            "--port".to_string(),
            port_path.to_string(),
            "--before".to_string(),
            before_reset.to_string(),
            "--non-interactive".to_string(),
            "--no-stub".to_string(),
            "--after".to_string(),
            "hard-reset".to_string(),
        ];
        args.push("--partition-table".to_string());
        args.push(partition_table.to_string_lossy().into_owned());
        args.push(path.to_string_lossy().into_owned());
        return Ok(vec![args]);
    }

    let Some(app_image) = artifact.files.iter().find(|file| file.kind == "app") else {
        return Err(HttpError::bad_request(
            "missing_flash_image",
            "Artifact does not contain an ELF or raw app image.",
        ));
    };
    let app_address = app_image.flash_address.ok_or_else(|| {
        HttpError::bad_request("missing_flash_address", "Missing app flash address.")
    })?;
    let partition_table_binary = firmware_partition_table_binary_path(root)?;
    let app_path = resolve_artifact_path(root, &app_image.path);
    let common = vec![
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--non-interactive".to_string(),
    ];
    let mut partition_table_args = vec!["write-bin".to_string()];
    partition_table_args.extend(common.clone());
    partition_table_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        DEFAULT_PARTITION_TABLE_FLASH_ADDRESS.to_string(),
        partition_table_binary.to_string_lossy().into_owned(),
    ]);
    let mut app_args = vec!["write-bin".to_string()];
    app_args.extend(common.clone());
    app_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        app_address.to_string(),
        app_path.to_string_lossy().into_owned(),
    ]);
    // espflash write-bin leaves the target in its loader. Reset explicitly once both images land.
    let mut reset_args = vec!["reset".to_string()];
    reset_args.extend(common);
    reset_args.extend(["--before".to_string(), before_reset.to_string()]);
    Ok(vec![partition_table_args, app_args, reset_args])
}

fn firmware_partition_table_path(root: Option<&Path>) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires an artifact root containing firmware/partitions.csv.",
        ));
    };
    let partition_table = root.join("firmware/partitions.csv");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires firmware/partitions.csv for the partition table.",
        ))
    }
}

fn firmware_partition_table_binary_path(root: Option<&Path>) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires an artifact root containing firmware/partitions.bin.",
        ));
    };
    let partition_table = root.join("firmware/partitions.bin");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires firmware/partitions.bin for the partition table.",
        ))
    }
}
