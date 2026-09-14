async fn execute_flash(
    state: &AppState,
    device_id: &str,
    artifact: &FirmwareArtifact,
    artifact_id: &str,
    port_path: &str,
) -> Result<Json<FlashResult>, HttpError> {
    state.emit(event(
        device_id,
        "flash",
        "real flash started",
        json!({ "artifactId": artifact_id, "dryRun": false }),
    ));
    if let Err(error) = run_espflash_with_exclusive_serial(
        state,
        artifact,
        state.config.artifact_root.as_deref(),
        port_path,
    )
    .await
    {
        state.emit(event(
            device_id,
            "flash",
            "real flash failed",
            json!({ "artifactId": artifact_id, "code": error.error.code }),
        ));
        return Err(error);
    }
    state.emit(event(
        device_id,
        "flash",
        "firmware boot observation started",
        json!({ "artifactId": artifact_id }),
    ));
    let boot = match observe_post_flash_boot(state, device_id, port_path).await {
        Ok(boot) => boot,
        Err(error) => {
            state.emit(event(
                device_id,
                "flash",
                "firmware boot verification failed",
                json!({
                    "artifactId": artifact_id,
                    "code": error.error.code,
                    "message": error.error.message,
                }),
            ));
            return Err(error);
        }
    };
    {
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(device_id) {
            device.selected_artifact_id = Some(artifact_id.to_string());
        }
    }
    state.emit(event(
        device_id,
        "flash",
        "real flash completed and firmware reached runtime_ready",
        json!({
            "artifactId": artifact_id,
            "dryRun": false,
            "resetCount": boot.reset_count,
            "lastStage": boot.last_stage,
        }),
    ));
    Ok(Json(FlashResult {
        artifact_id: artifact_id.to_string(),
        dry_run: false,
        status: "completed".to_string(),
        message: "espflash completed and firmware reached runtime_ready.".to_string(),
    }))
}

pub fn scan_serial_devices(serial_port: Option<&Path>) -> Vec<DeviceRecord> {
    let available_ports = serialport::available_ports().ok().unwrap_or_default();
    scan_serial_devices_from_available(serial_port, &available_ports)
}

fn scan_serial_devices_from_available(
    serial_port: Option<&Path>,
    available_ports: &[serialport::SerialPortInfo],
) -> Vec<DeviceRecord> {
    let Some(serial_port) = serial_port else {
        return available_ports
            .iter()
            .filter(|port| is_flux_purr_usb_candidate(port))
            .map(|port| serial_device_record(&port.port_name, Some(port)))
            .collect();
    };
    let port_name = serial_port.to_string_lossy().into_owned();
    if !serial_port.exists() {
        return vec![missing_serial_device_record(&port_name, available_ports)];
    }

    let port_info = available_ports
        .iter()
        .find(|port| port.port_name == port_name);
    vec![serial_device_record(&port_name, port_info)]
}

fn is_flux_purr_usb_candidate(port: &serialport::SerialPortInfo) -> bool {
    port.port_name.starts_with("/dev/cu.usbmodem")
        || matches!(
            &port.port_type,
            serialport::SerialPortType::UsbPort(info) if info.vid == 0x303a
        )
}

fn refresh_serial_devices(state: &mut DevdState, serial_devices: Vec<DeviceRecord>) {
    let serial_ids = serial_devices
        .iter()
        .map(|device| device.id.clone())
        .collect::<HashSet<_>>();

    state.devices.retain(|_, device| {
        device.transport != DeviceTransport::NativeSerial || serial_ids.contains(&device.id)
    });
    state
        .leases
        .retain(|_, lease| state.devices.contains_key(&lease.device_id));

    for device in serial_devices {
        if let Some(existing) = state.devices.get_mut(&device.id) {
            existing.display_name = device.display_name;
            existing.port_path = device.port_path;
            existing.transport = device.transport;
        } else {
            state.devices.insert(device.id.clone(), device);
        }
    }
}

fn serial_device_record(
    port_name: &str,
    port_info: Option<&serialport::SerialPortInfo>,
) -> DeviceRecord {
    let (id, display_name) = match port_info.map(|port| &port.port_type) {
        Some(serialport::SerialPortType::UsbPort(info)) => {
            let serial = info
                .serial_number
                .clone()
                .unwrap_or_else(|| port_name.replace('/', "_"));
            (
                format!("serial-{:04x}-{:04x}-{serial}", info.vid, info.pid),
                info.product
                    .clone()
                    .unwrap_or_else(|| "USB serial device".to_string()),
            )
        }
        _ => (
            format!("serial-{}", port_name.replace('/', "_")),
            "Authorized serial device".to_string(),
        ),
    };
    DeviceRecord::native_serial_placeholder(&id, display_name, port_name.to_string())
}

fn missing_serial_device_record(
    port_name: &str,
    available_ports: &[serialport::SerialPortInfo],
) -> DeviceRecord {
    let mut device = serial_device_record(port_name, None);
    let candidates = available_ports
        .iter()
        .filter(|port| {
            matches!(
                &port.port_type,
                serialport::SerialPortType::UsbPort(info) if info.vid == 0x303a
            )
        })
        .map(|port| port.port_name.clone())
        .collect::<Vec<_>>();
    let candidate_summary = if candidates.is_empty() {
        "No alternate Espressif serial port is currently enumerated.".to_string()
    } else {
        format!(
            "Observed alternate Espressif serial ports: {}.",
            candidates.join(", ")
        )
    };
    device.connection = ConnectionState::Error;
    device.network.state = NetworkState::Error;
    device.network.last_error = Some(format!(
        "Authorized serial port {port_name} is missing. {candidate_summary}"
    ));
    device.status.network = device.network.clone();
    device.events.push_back(event(
        &device.id,
        "serial",
        "authorized serial port missing",
        json!({
            "code": "authorized_port_missing",
            "portPath": port_name,
            "candidates": candidates,
        }),
    ));
    device
}

pub fn verify_artifact(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
) -> io::Result<ArtifactVerifyResult> {
    let mut files = Vec::new();
    for file in &artifact.files {
        let path = resolve_verified_artifact_path(root, &file.path)?;
        let bytes = fs::read(&path)?;
        let size = bytes.len() as u64;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        let ok = size == file.size && digest == file.sha256;
        files.push(ArtifactFileResult {
            kind: file.kind.clone(),
            sha256: digest,
            size,
            ok,
        });
    }

    Ok(ArtifactVerifyResult {
        artifact_id: artifact.artifact_id.clone(),
        verified: !files.is_empty() && files.iter().all(|file| file.ok),
        files,
    })
}

pub fn discover_firmware_artifacts(root: Option<&Path>) -> io::Result<Vec<FirmwareArtifact>> {
    let candidates = [
        (
            "local-esp32s3-release",
            "Local ESP32-S3 release (buzzer test)",
            "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
            "release + web_serial + net_http + buzzer-test",
            vec![
                "web_serial".to_string(),
                "net_http".to_string(),
                "buzzer-test".to_string(),
            ],
            "elf",
        ),
        (
            "local-esp32s3-release-buzzer-observe",
            "Local ESP32-S3 release (buzzer observe)",
            "firmware/target/buzzer-observe/xtensa-esp32s3-none-elf/release/flux-purr",
            "release + web_serial + net_http + buzzer-test + buzzer-observe",
            vec![
                "web_serial".to_string(),
                "net_http".to_string(),
                "buzzer-test".to_string(),
                "buzzer-observe".to_string(),
            ],
            "elf",
        ),
        (
            "local-host-release",
            "Local host release",
            "firmware/target/release/flux-purr",
            "host release",
            Vec::new(),
            "host_binary",
        ),
    ];
    let mut artifacts = Vec::new();

    for (artifact_id, name, path, profile, features, kind) in candidates {
        let resolved_path = resolve_artifact_path(root, path);
        if !resolved_path.is_file() {
            continue;
        }

        let bytes = fs::read(&resolved_path)?;
        let size = bytes.len() as u64;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        artifacts.push(FirmwareArtifact {
            artifact_id: artifact_id.to_string(),
            name: name.to_string(),
            version: "local-build".to_string(),
            git_sha: option_env!("VERGEN_GIT_SHA")
                .unwrap_or("unknown")
                .to_string(),
            build_id: digest
                .trim_start_matches("sha256:")
                .chars()
                .take(12)
                .collect(),
            target_chip: if artifact_id.contains("esp32s3") {
                "esp32s3".to_string()
            } else {
                "host".to_string()
            },
            profile: profile.to_string(),
            features,
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: kind.to_string(),
                path: path.to_string(),
                sha256: digest,
                size,
                flash_address: if kind == "app" {
                    Some(DEFAULT_APP_FLASH_ADDRESS)
                } else {
                    None
                },
            }],
        });
    }

    Ok(artifacts)
}
