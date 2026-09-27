pub(crate) use super::*;

pub(crate) fn validate_manual_pps_against_status(
    millivolts: u16,
    milliamps: u16,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    validate_pps_voltage_against_status(millivolts, status)?;
    let Some(max_ma) = status.pps_capability_max_ma else {
        return Err(HttpError::bad_request(
            "manual_pps_no_capability",
            "PPS capability is unavailable.",
        ));
    };
    if milliamps > max_ma {
        return Err(HttpError::bad_request(
            "manual_pps_out_of_range",
            "manualPpsMa is outside the advertised PPS capability.",
        ));
    }
    Ok(())
}

pub(crate) async fn verify_artifact_route(
    State(state): State<AppState>,
    Json(payload): Json<ArtifactVerifyRequest>,
) -> Result<Json<ArtifactVerifyResult>, HttpError> {
    verify_artifact(&payload.artifact, state.config.artifact_root.as_deref())
        .map(Json)
        .map_err(sanitize_io_error)
}

pub(crate) async fn list_artifacts_route(
    State(state): State<AppState>,
) -> Result<Json<FirmwareArtifactCatalog>, HttpError> {
    discover_firmware_artifacts(state.config.artifact_root.as_deref())
        .map(|artifacts| Json(FirmwareArtifactCatalog { artifacts }))
        .map_err(sanitize_io_error)
}

pub(crate) async fn list_firmware_bundles(
    State(state): State<AppState>,
) -> Result<Json<FirmwareBundleCatalog>, HttpError> {
    let mut bundles = Vec::new();
    let entries = fs::read_dir(state.bundle_store.path()).map_err(sanitize_io_error)?;
    for entry in entries {
        let entry = entry.map_err(sanitize_io_error)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("fluxpurr-fw") {
            continue;
        }
        let bundle = firmware_bundle::read_bundle(&entry.path()).map_err(bundle_http_error)?;
        bundles.push(bundle_summary(&bundle));
    }
    bundles.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    Ok(Json(FirmwareBundleCatalog { bundles }))
}

pub(crate) async fn import_firmware_bundle(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<FirmwareBundleSummary>), HttpError> {
    if body.len() as u64 > firmware_bundle::MAX_BUNDLE_BYTES {
        return Err(HttpError::bad_request(
            "bundle_too_large",
            "Firmware bundle exceeds the 8 MiB limit.",
        ));
    }
    let bundle = firmware_bundle::read_bundle_bytes(&body).map_err(bundle_http_error)?;
    let filename = format!(
        "{}.fluxpurr-fw",
        bundle.bundle_sha256.trim_start_matches("sha256:")
    );
    let target = state.bundle_store.path().join(filename);
    if !target.exists() {
        let temp = state.bundle_store.path().join(format!(
            ".import-{}-{}",
            now_millis(),
            EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&temp, &body).map_err(sanitize_io_error)?;
        fs::rename(&temp, &target).map_err(sanitize_io_error)?;
    }
    Ok((StatusCode::CREATED, Json(bundle_summary(&bundle))))
}

pub(crate) async fn local_firmware_update(
    State(state): State<AppState>,
    Json(payload): Json<LocalFirmwareUpdateRequest>,
) -> Result<Json<Value>, HttpError> {
    let (port, bundle) = validate_local_update_request(&state, &payload)?;
    let target = find_local_update_target(&state, &port)?;

    let mut progress = FirmwareOperationProgress::new(
        &state,
        &target.id,
        FirmwareOperation::Update,
        &bundle.bundle_sha256,
        false,
    );
    let usb_identity = capture_native_serial_identity(&port)?;
    progress.operation_started();
    progress.stage_started("authorization", json!({ "port": port }));
    progress.stage_completed("authorization", json!({ "port": port }));
    let preflight_lease_id = format!("local-update-{}", progress.operation_id());
    progress.stage_started("preflight", json!({}));
    let (identity, status) =
        refresh_native_update_runtime_facts(&state, &target, &preflight_lease_id, &usb_identity)
            .await?;
    validate_update_runtime_identity(DeviceTransport::NativeSerial, &identity)?;
    validate_update_runtime_facts(
        DeviceTransport::NativeSerial,
        &identity.firmware_version,
        &status,
    )?;
    let serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    let serial_lock = take_cached_serial_process_lock(&state.serial_sessions, &port)?
        .ok_or_else(|| HttpError::internal("native update lost its serial session lock"))?;
    ensure_native_serial_identity(&port, &usb_identity)?;
    let security = probe_native_rom_security_with_locks(
        &state,
        &port,
        Some(&serial_rpc),
        Some(&serial_lock),
        Some(&usb_identity),
    )
    .await?;
    security.validate_for_flash()?;
    progress.stage_completed("preflight", json!({}));
    run_bundle_flash_transaction(
        &state,
        &bundle,
        FirmwareOperation::Update,
        &port,
        &mut progress,
        FirmwareFlashGuards {
            serial_rpc: Some(serial_rpc),
            serial_lock: Some(serial_lock),
            usb_identity: Some(&usb_identity),
        },
    )
    .await?;
    progress.stage_started("runtime_reconnect", json!({}));
    let verified = verify_reconnected_firmware(&state, &target, &bundle, &usb_identity).await;
    if verified {
        progress.stage_completed("runtime_reconnect", json!({}));
    } else {
        progress.stage_failed("runtime_reconnect", "runtime_verification_failed");
        mark_firmware_runtime_unverified(&state, &target.id);
    }
    let outcome = if verified {
        "verified"
    } else {
        "write_complete_unverified"
    };
    progress.operation_completed(outcome);
    Ok(Json(json!({
        "ok": verified,
        "operation": "update",
        "port": port,
        "artifactId": bundle.bundle_sha256,
        "outcome": outcome,
    })))
}

pub(crate) fn validate_local_update_request(
    state: &AppState,
    payload: &LocalFirmwareUpdateRequest,
) -> Result<(String, firmware_bundle::FirmwareBundle), HttpError> {
    if !state.config.allow_real_flash {
        return Err(HttpError::forbidden(
            "real_flash_disabled",
            "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
        ));
    }
    let port = payload.port.trim().to_owned();
    if port.is_empty()
        || port.contains("://")
        || port.starts_with("tcp:")
        || port.parse::<SocketAddr>().is_ok()
    {
        return Err(HttpError::bad_request(
            "invalid_serial_port",
            "Firmware update requires the exact local serial port supplied by the caller.",
        ));
    }
    let artifact_id = payload.artifact_id.trim();
    if artifact_id.len() != 71
        || !artifact_id.starts_with("sha256:")
        || !artifact_id[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(HttpError::bad_request(
            "invalid_artifact_id",
            "Local firmware update requires a SHA-256 artifact ID.",
        ));
    }
    let bundle_path = state
        .bundle_store
        .path()
        .join(format!("{}.fluxpurr-fw", &artifact_id[7..]));
    let bundle = firmware_bundle::read_bundle(&bundle_path).map_err(bundle_http_error)?;
    if bundle.bundle_sha256 != artifact_id {
        return Err(HttpError::bad_request(
            "artifact_id_mismatch",
            "artifactId does not match the imported bundle content.",
        ));
    }
    Ok((port, bundle))
}

pub(crate) fn find_local_update_target(
    state: &AppState,
    port: &str,
) -> Result<DeviceRecord, HttpError> {
    let serial_devices = scan_serial_devices(Some(Path::new(port)));
    let (target, stale_ports) = {
        let mut state_lock = state.lock()?;
        let stale_ports = refresh_serial_devices(&mut state_lock, serial_devices);
        let target = state_lock
            .devices
            .values()
            .find(|device| {
                device.transport == DeviceTransport::NativeSerial
                    && device
                        .port_path
                        .as_deref()
                        .is_some_and(|candidate| serial_port_paths_match(port, candidate))
            })
            .cloned();
        (target, stale_ports)
    };
    remove_cached_serial_sessions_for_paths(&state.serial_sessions, &stale_ports)?;
    let target = target.ok_or_else(|| {
        HttpError::bad_request(
            "serial_port_not_found",
            "The supplied serial port is not present in the current device set.",
        )
    })?;
    if target.connection == ConnectionState::Error {
        return Err(HttpError::bad_request(
            "serial_port_missing",
            "The supplied serial port is not available; no replacement port will be selected.",
        ));
    }
    Ok(target)
}

pub(crate) async fn verify_reconnected_firmware(
    state: &AppState,
    target: &DeviceRecord,
    bundle: &firmware_bundle::FirmwareBundle,
    usb_identity: &UsbSerialIdentity,
) -> bool {
    let identity = serial_request_payload_with_identity::<Identity>(
        state,
        target,
        "get_identity",
        "identity",
        Some(usb_identity),
    )
    .await;
    let install_status = serial_request_payload_with_identity::<InstallStatus>(
        state,
        target,
        "get_install_status",
        "install_status",
        Some(usb_identity),
    )
    .await;
    identity
        .as_ref()
        .is_ok_and(|identity| runtime_identity_matches_bundle(identity, bundle))
        && install_status.as_ref().is_ok_and(|status| {
            status.layout_id == bundle.manifest.layout.id
                && status.layout_version == bundle.manifest.layout.version
                && status.partition_table_sha256 == bundle.manifest.layout.partition_table_sha256
        })
}

pub(crate) fn runtime_identity_matches_bundle(
    identity: &Identity,
    bundle: &firmware_bundle::FirmwareBundle,
) -> bool {
    identity.firmware_kind == Some(FirmwareKind::Product)
        && identity.firmware_version == bundle.manifest.identity.version
        && identity.git_sha == bundle.manifest.identity.source_sha
        && identity.build_id == bundle.manifest.identity.build_id
}

pub(crate) fn bundle_summary(bundle: &firmware_bundle::FirmwareBundle) -> FirmwareBundleSummary {
    FirmwareBundleSummary {
        artifact_id: bundle.bundle_sha256.clone(),
        source: "local".into(),
        channel: bundle.manifest.identity.channel,
        version: bundle.manifest.identity.version.clone(),
        source_sha: bundle.manifest.identity.source_sha.clone(),
        build_id: bundle.manifest.identity.build_id.clone(),
        bundle_sha256: bundle.bundle_sha256.clone(),
        size: bundle.archive_size,
        layout_id: bundle.manifest.layout.id.clone(),
        operations: vec![
            FirmwareOperation::Update,
            FirmwareOperation::InstallRecovery,
        ],
    }
}

pub(crate) fn bundle_http_error(error: firmware_bundle::BundleError) -> HttpError {
    HttpError::bad_request("firmware_bundle_invalid", &error.to_string())
}

pub(crate) fn validate_update_runtime_facts(
    transport: DeviceTransport,
    current_version: &str,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    if transport == DeviceTransport::NativeSerial
        && (current_version == "unknown" || current_version.trim().is_empty())
    {
        return Err(HttpError::forbidden(
            "update_identity_required",
            "Update requires a verified Flux Purr runtime identity.",
        ));
    }
    if status.heater_enabled || !status.current_temp_c.is_finite() || status.current_temp_c > 40.0 {
        return Err(HttpError::forbidden(
            "update_temperature_gate",
            "Update requires heater off and a valid temperature at or below 40 C.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_update_runtime_identity(
    transport: DeviceTransport,
    identity: &Identity,
) -> Result<(), HttpError> {
    if transport == DeviceTransport::NativeSerial
        && identity.firmware_kind != Some(FirmwareKind::Product)
    {
        return Err(HttpError::forbidden(
            "update_identity_required",
            "Update requires a verified Flux Purr product runtime identity.",
        ));
    }
    Ok(())
}

pub(crate) fn capture_native_serial_identity(
    port_path: &str,
) -> Result<UsbSerialIdentity, HttpError> {
    serial_port_usb_identity(port_path).map_err(|error| {
        HttpError::forbidden(
            "authorized_port_identity_required",
            &format!("Firmware operation requires a stable USB target identity: {error}"),
        )
    })
}

fn ensure_native_serial_identity(
    port_path: &str,
    expected: &UsbSerialIdentity,
) -> Result<(), HttpError> {
    if serial_port_usb_identity_matches(port_path, expected) {
        return Ok(());
    }
    Err(HttpError::forbidden(
        "authorized_port_changed",
        "The authorized USB target changed or disappeared; no replacement port will be selected.",
    ))
}

pub(crate) async fn refresh_native_update_runtime_facts(
    state: &AppState,
    target: &DeviceRecord,
    lease_id: &str,
    usb_identity: &UsbSerialIdentity,
) -> Result<(Identity, ControlPlaneStatus), HttpError> {
    let identity = serial_request_payload_with_identity::<Identity>(
        state,
        target,
        "get_identity",
        "identity",
        Some(usb_identity),
    )
    .await?;
    let _stopped = serial_runtime_config_with_identity(
        state,
        target,
        &RuntimeConfigRequest {
            lease_id: lease_id.to_string(),
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: Some(false),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
        Some(usb_identity),
    )
    .await?;
    let status = serial_request_payload_with_identity::<ControlPlaneStatus>(
        state,
        target,
        "get_status",
        "status",
        Some(usb_identity),
    )
    .await?;
    Ok((identity, status))
}

pub(crate) struct PreparedFirmwareOperation {
    bundle: firmware_bundle::FirmwareBundle,
    target: DeviceRecord,
    port_path: String,
    transport: DeviceTransport,
    current_version: String,
    status: ControlPlaneStatus,
    rom_mac: String,
    usb_identity: Option<UsbSerialIdentity>,
    serial_rpc: Option<tokio::sync::OwnedMutexGuard<()>>,
    serial_lock: Option<SerialPortProcessLock>,
}

pub(crate) struct FirmwareFlashGuards<'a> {
    serial_rpc: Option<tokio::sync::OwnedMutexGuard<()>>,
    serial_lock: Option<SerialPortProcessLock>,
    usb_identity: Option<&'a UsbSerialIdentity>,
}

#[derive(Clone, Copy)]
struct FirmwareFlashTarget<'a> {
    port_path: &'a str,
    usb_identity: Option<&'a UsbSerialIdentity>,
}

pub(crate) async fn firmware_operation(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FirmwareOperationRequest>,
) -> Result<Json<FirmwareOperationResult>, HttpError> {
    let mut progress = FirmwareOperationProgress::new(
        &state,
        &device_id,
        payload.operation,
        &payload.artifact_id,
        payload.dry_run,
    );
    progress.operation_started();
    let mut prepared =
        prepare_firmware_operation(&state, &device_id, &payload, &mut progress).await?;
    if payload.dry_run {
        progress.stage_started("preflight", json!({}));
    }

    let preflight_digest = firmware_preflight_digest(
        &payload,
        &device_id,
        &prepared.port_path,
        &prepared.rom_mac,
        &prepared.bundle.bundle_sha256,
        prepared.usb_identity.as_ref(),
    );
    if payload.dry_run {
        let token =
            create_firmware_approval(&state, &device_id, &payload, &prepared, preflight_digest)?;
        progress.stage_completed("preflight", json!({}));
        progress.operation_completed("passed");
        return Ok(Json(FirmwareOperationResult {
            operation_id: progress.operation_id().to_string(),
            artifact_id: prepared.bundle.bundle_sha256,
            operation: payload.operation,
            dry_run: true,
            outcome: "passed".into(),
            approval_token: Some(token),
            approval_expires_in_ms: Some(5 * 60 * 1000),
            stages: firmware_preflight_stages(),
            message: "Preflight passed; no flash write performed.".into(),
        }));
    }

    authorize_firmware_operation(
        &state,
        &device_id,
        &payload,
        &prepared,
        preflight_digest,
        &mut progress,
    )?;
    progress.stage_completed("authorization", json!({}));

    run_bundle_flash_transaction(
        &state,
        &prepared.bundle,
        payload.operation,
        &prepared.port_path,
        &mut progress,
        FirmwareFlashGuards {
            serial_rpc: prepared.serial_rpc.take(),
            serial_lock: prepared.serial_lock.take(),
            usb_identity: prepared.usb_identity.as_ref(),
        },
    )
    .await?;
    let verified =
        reconnect_firmware_operation(&state, &device_id, &prepared, &mut progress).await?;
    let outcome = if verified {
        progress.stage_completed("runtime_verify", json!({}));
        "verified"
    } else {
        progress.stage_failed("runtime_verify", "runtime_verification_failed");
        "write_complete_unverified"
    };
    progress.operation_completed(outcome);
    Ok(Json(FirmwareOperationResult {
        operation_id: progress.operation_id().to_string(),
        artifact_id: prepared.bundle.bundle_sha256.clone(),
        operation: payload.operation,
        dry_run: false,
        outcome: outcome.into(),
        approval_token: None,
        approval_expires_in_ms: None,
        stages: firmware_execution_stages(payload.operation),
        message: if verified {
            "Firmware bytes and runtime install status verified."
        } else {
            "Firmware bytes verified, but runtime identity or install status did not verify."
        }
        .into(),
    }))
}

pub(crate) async fn prepare_firmware_operation(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    progress: &mut FirmwareOperationProgress,
) -> Result<PreparedFirmwareOperation, HttpError> {
    progress.stage_started(
        if payload.dry_run {
            "artifact"
        } else {
            "authorization"
        },
        json!({}),
    );
    let bundle = load_operation_bundle(state, payload, progress)?;
    if payload.dry_run {
        progress.stage_completed("artifact", json!({}));
        progress.stage_started("transport", json!({}));
    }
    let target = load_operation_target(state, device_id, payload, progress)?;
    let mut prepared = PreparedFirmwareOperation {
        port_path: target
            .port_path
            .clone()
            .unwrap_or_else(|| "mock://esp32s3".into()),
        transport: target.transport,
        current_version: target.identity.firmware_version.clone(),
        status: target.status.clone(),
        target,
        bundle,
        rom_mac: String::new(),
        usb_identity: None,
        serial_rpc: None,
        serial_lock: None,
    };
    if prepared.transport == DeviceTransport::NativeSerial {
        prepared.usb_identity = Some(capture_native_serial_identity(&prepared.port_path)?);
    }
    refresh_operation_facts(state, device_id, payload, &mut prepared, progress).await?;
    if prepared.transport == DeviceTransport::NativeSerial {
        let serial_rpc = progress.require(
            acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await,
        )?;
        let serial_lock = if payload.operation == FirmwareOperation::Update {
            let cached = progress.require(take_cached_serial_process_lock(
                &state.serial_sessions,
                &prepared.port_path,
            ))?;
            cached.ok_or_else(|| {
                progress.fail(HttpError::internal(
                    "native update lost its serial session lock",
                ))
            })?
        } else {
            progress.require(drop_cached_serial_session(
                &state.serial_sessions,
                &prepared.port_path,
            ))?;
            progress.require(
                acquire_serial_process_lock(&prepared.port_path, ESPFLASH_COMMAND_TIMEOUT).await,
            )?
        };
        if let Some(usb_identity) = prepared.usb_identity.as_ref() {
            progress.require(ensure_native_serial_identity(
                &prepared.port_path,
                usb_identity,
            ))?;
        }
        prepared.serial_lock = Some(serial_lock);
        prepared.serial_rpc = Some(serial_rpc);
    }
    if payload.dry_run {
        progress.stage_completed("transport", json!({}));
        progress.stage_started("rom_reset", json!({}));
    }
    prepared.rom_mac = operation_rom_security(
        state,
        &prepared,
        prepared.serial_rpc.as_ref(),
        prepared.serial_lock.as_ref(),
        prepared.usb_identity.as_ref(),
        progress,
    )
    .await?;
    if payload.dry_run {
        progress.stage_completed("rom_reset", json!({}));
        progress.stage_started("chip_flash_security", json!({}));
    }
    validate_operation_downgrade(&prepared, payload)?;
    if payload.dry_run {
        progress.stage_completed("chip_flash_security", json!({}));
    }
    Ok(prepared)
}

pub(crate) fn load_operation_bundle(
    state: &AppState,
    payload: &FirmwareOperationRequest,
    progress: &mut FirmwareOperationProgress,
) -> Result<firmware_bundle::FirmwareBundle, HttpError> {
    let path = state.bundle_store.path().join(format!(
        "{}.fluxpurr-fw",
        payload.artifact_id.trim_start_matches("sha256:")
    ));
    let bundle = progress.require(firmware_bundle::read_bundle(&path).map_err(|error| {
        HttpError::bad_request(
            "firmware_bundle_unavailable",
            &format!("The imported firmware bundle is unavailable: {error}"),
        )
    }))?;
    if bundle.bundle_sha256 != payload.artifact_id {
        return Err(progress.fail(HttpError::bad_request(
            "artifact_id_mismatch",
            "artifactId does not match the imported bundle content.",
        )));
    }
    Ok(bundle)
}

pub(crate) fn load_operation_target(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    progress: &mut FirmwareOperationProgress,
) -> Result<DeviceRecord, HttpError> {
    let target = {
        let mut inner = progress.require(state.lock())?;
        inner
            .require_lease(device_id, Some(&payload.lease_id))
            .and_then(|_| {
                let device = inner
                    .devices
                    .get(device_id)
                    .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
                if device.transport == DeviceTransport::Lan {
                    return Err(HttpError::bad_request(
                        "lan_flash_unsupported",
                        "Firmware flashing is unavailable through the DEVD LAN bridge.",
                    ));
                }
                Ok(device.clone())
            })
    };
    progress.require(target)
}

pub(crate) async fn refresh_operation_facts(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    prepared: &mut PreparedFirmwareOperation,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    if payload.operation != FirmwareOperation::Update {
        return Ok(());
    }
    let identity = match prepared.transport {
        DeviceTransport::NativeSerial => {
            let usb_identity = prepared.usb_identity.as_ref().ok_or_else(|| {
                progress.fail(HttpError::internal("native update is missing USB identity"))
            })?;
            let (identity, status) = progress.require(
                refresh_native_update_runtime_facts(
                    state,
                    &prepared.target,
                    &payload.lease_id,
                    usb_identity,
                )
                .await,
            )?;
            prepared.status = status;
            Some(identity)
        }
        DeviceTransport::Mock => {
            prepared.status.heater_enabled = false;
            prepared.status.heater_output_percent = 0;
            prepared.status.heater_physical_output_percent = 0;
            None
        }
        DeviceTransport::Lan => unreachable!(),
    };
    if let Some(identity) = identity.as_ref() {
        prepared.current_version = identity.firmware_version.clone();
        progress.require(validate_update_runtime_identity(
            prepared.transport,
            identity,
        ))?;
    }
    update_operation_device(state, device_id, prepared, identity)?;
    progress.require(validate_update_runtime_facts(
        prepared.transport,
        &prepared.current_version,
        &prepared.status,
    ))
}

pub(crate) fn update_operation_device(
    state: &AppState,
    device_id: &str,
    prepared: &PreparedFirmwareOperation,
    identity: Option<Identity>,
) -> Result<(), HttpError> {
    let Ok(mut inner) = state.lock() else {
        return Ok(());
    };
    let Some(device) = inner.devices.get_mut(device_id) else {
        return Ok(());
    };
    if device.port_path.as_deref() != prepared.target.port_path.as_deref() {
        return Ok(());
    }
    if let Some(identity) = identity {
        device.identity = identity;
    }
    device.network = prepared.status.network.clone();
    device.status = prepared.status.clone();
    device.connection = ConnectionState::Connected;
    Ok(())
}

pub(crate) async fn operation_rom_security(
    state: &AppState,
    prepared: &PreparedFirmwareOperation,
    serial_rpc: Option<&tokio::sync::OwnedMutexGuard<()>>,
    serial_lock: Option<&SerialPortProcessLock>,
    usb_identity: Option<&UsbSerialIdentity>,
    progress: &mut FirmwareOperationProgress,
) -> Result<String, HttpError> {
    let security = match prepared.transport {
        DeviceTransport::Mock => RomSecurityInfo {
            rom_mac: prepared.target.identity.device_id.clone(),
            secure_boot_enabled: false,
            flash_encryption_enabled: false,
            secure_download_mode_enabled: false,
            response_known: true,
            chip_is_esp32s3: true,
            flash_size_bytes: 4 * 1024 * 1024,
            package_matches: true,
        },
        DeviceTransport::NativeSerial => progress.require(
            probe_native_rom_security_with_locks(
                state,
                &prepared.port_path,
                serial_rpc,
                serial_lock,
                usb_identity,
            )
            .await,
        )?,
        DeviceTransport::Lan => unreachable!(),
    };
    progress.require(security.validate_for_flash())?;
    Ok(security.rom_mac)
}

pub(crate) fn validate_operation_downgrade(
    prepared: &PreparedFirmwareOperation,
    payload: &FirmwareOperationRequest,
) -> Result<(), HttpError> {
    if payload.operation != FirmwareOperation::Update || payload.allow_downgrade {
        return Ok(());
    }
    let current = semver::Version::parse(
        prepared
            .current_version
            .trim_start_matches("fw/")
            .trim_start_matches('v'),
    );
    let target = semver::Version::parse(
        prepared
            .bundle
            .manifest
            .identity
            .version
            .trim_start_matches('v'),
    );
    if current
        .ok()
        .zip(target.ok())
        .is_some_and(|(current, target)| target < current)
    {
        return Err(HttpError::forbidden(
            "downgrade_confirmation_required",
            "The target firmware is older; explicit allowDowngrade is required.",
        ));
    }
    Ok(())
}

pub(crate) fn create_firmware_approval(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    prepared: &PreparedFirmwareOperation,
    preflight_digest: String,
) -> Result<String, HttpError> {
    let mut inner = state.lock()?;
    let token = inner.next_id("firmware-approval");
    inner.firmware_approvals.insert(
        token.clone(),
        FirmwareApproval {
            lease_id: payload.lease_id.clone(),
            device_id: device_id.to_string(),
            port_path: prepared.port_path.clone(),
            usb_identity: prepared.usb_identity.clone(),
            rom_mac: prepared.rom_mac.clone(),
            bundle_sha256: prepared.bundle.bundle_sha256.clone(),
            operation: payload.operation,
            allow_downgrade: payload.allow_downgrade,
            preflight_digest,
            expires_at: Instant::now() + Duration::from_secs(5 * 60),
        },
    );
    Ok(token)
}

pub(crate) fn authorize_firmware_operation(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    prepared: &PreparedFirmwareOperation,
    preflight_digest: String,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let token = progress.require(payload.approval_token.as_deref().ok_or_else(|| {
        HttpError::forbidden(
            "approval_required",
            "Execution requires a current single-use approval token.",
        )
    }))?;
    let approval = progress.require(state.lock()?.firmware_approvals.remove(token).ok_or_else(
        || {
            HttpError::forbidden(
                "approval_invalid",
                "The approval token is invalid or already used.",
            )
        },
    ))?;
    let matches = approval.expires_at > Instant::now()
        && approval.lease_id == payload.lease_id
        && approval.device_id == device_id
        && approval.port_path == prepared.port_path
        && approval.usb_identity == prepared.usb_identity
        && approval.rom_mac == prepared.rom_mac
        && approval.bundle_sha256 == prepared.bundle.bundle_sha256
        && approval.operation == payload.operation
        && approval.allow_downgrade == payload.allow_downgrade
        && approval.preflight_digest == preflight_digest;
    if !matches {
        return Err(progress.fail(HttpError::forbidden(
            "approval_mismatch",
            "The target or preflight facts changed; run preflight again.",
        )));
    }
    let expected_confirm = match payload.operation {
        FirmwareOperation::Update => "FLASH",
        FirmwareOperation::InstallRecovery => "ERASE_INSTALL",
    };
    if payload.confirm.as_deref() != Some(expected_confirm) {
        return Err(progress.fail(HttpError::forbidden(
            "confirmation_required",
            &format!("Execution requires confirm={expected_confirm}."),
        )));
    }
    if !state.config.allow_real_flash {
        return Err(progress.fail(HttpError::forbidden(
            "real_flash_disabled",
            "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
        )));
    }
    if prepared.transport != DeviceTransport::NativeSerial {
        return Err(progress.fail(HttpError::bad_request(
            "real_flash_requires_native_serial",
            "Real flash requires a native serial target.",
        )));
    }
    Ok(())
}

pub(crate) async fn reconnect_firmware_operation(
    state: &AppState,
    device_id: &str,
    prepared: &PreparedFirmwareOperation,
    progress: &mut FirmwareOperationProgress,
) -> Result<bool, HttpError> {
    progress.stage_started("runtime_reconnect", json!({}));
    let target = {
        let inner = progress.require(state.lock())?;
        inner.devices.get(device_id).cloned()
    };
    let Some(target) = target else {
        progress.stage_failed("runtime_reconnect", "device_not_found");
        progress.stage_started("runtime_verify", json!({}));
        return Ok(false);
    };
    let usb_identity = prepared.usb_identity.as_ref().ok_or_else(|| {
        progress.fail(HttpError::internal(
            "native firmware operation is missing USB identity",
        ))
    })?;
    let identity = serial_request_payload_with_identity::<Identity>(
        state,
        &target,
        "get_identity",
        "identity",
        Some(usb_identity),
    )
    .await;
    let install_status = serial_request_payload_with_identity::<InstallStatus>(
        state,
        &target,
        "get_install_status",
        "install_status",
        Some(usb_identity),
    )
    .await;
    if identity.is_ok() && install_status.is_ok() {
        progress.stage_completed("runtime_reconnect", json!({}));
    } else {
        progress.stage_failed("runtime_reconnect", "runtime_reconnect_failed");
    }
    progress.stage_started("runtime_verify", json!({}));
    let verified = identity
        .as_ref()
        .is_ok_and(|identity| runtime_identity_matches_bundle(identity, &prepared.bundle))
        && install_status.as_ref().is_ok_and(|status| {
            status.layout_id == prepared.bundle.manifest.layout.id
                && status.layout_version == prepared.bundle.manifest.layout.version
                && status.partition_table_sha256
                    == prepared.bundle.manifest.layout.partition_table_sha256
        });
    if !verified {
        mark_firmware_runtime_unverified(state, device_id);
    }
    Ok(verified)
}

fn mark_firmware_runtime_unverified(state: &AppState, device_id: &str) {
    if let Ok(mut state_lock) = state.lock()
        && let Some(device) = state_lock.devices.get_mut(device_id)
    {
        device.connection = ConnectionState::Error;
    }
}

pub(crate) async fn run_bundle_flash_transaction(
    state: &AppState,
    bundle: &firmware_bundle::FirmwareBundle,
    operation: FirmwareOperation,
    port_path: &str,
    progress: &mut FirmwareOperationProgress,
    guards: FirmwareFlashGuards<'_>,
) -> Result<(), HttpError> {
    let _serial_rpc = match guards.serial_rpc {
        Some(guard) => guard,
        None => progress.require(
            acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await,
        )?,
    };
    progress.require(drop_cached_serial_session(
        &state.serial_sessions,
        port_path,
    ))?;
    let _serial_lock = match guards.serial_lock {
        Some(lock) => lock,
        None => progress
            .require(acquire_serial_process_lock(port_path, ESPFLASH_COMMAND_TIMEOUT).await)?,
    };
    if let Some(usb_identity) = guards.usb_identity {
        progress.require(ensure_native_serial_identity(port_path, usb_identity))?;
    }
    let target = FirmwareFlashTarget {
        port_path,
        usb_identity: guards.usb_identity,
    };
    let workspace = progress.require(tempfile::tempdir().map_err(|error| {
        HttpError::internal(&format!("failed to create flash workspace: {error}"))
    }))?;
    stage_bundle_segments(bundle, workspace.path(), progress)?;
    let program = resolve_espflash_program();
    let common = espflash_common_args(port_path);
    let initial_reset = if is_esp_usb_serial_jtag_port(port_path) {
        "usb-reset"
    } else {
        "default-reset"
    };
    run_bundle_erase_if_needed(
        operation,
        &program,
        &common,
        initial_reset,
        bundle,
        target,
        progress,
    )
    .await?;
    write_bundle_segments(
        &program,
        &common,
        bundle,
        workspace.path(),
        target,
        progress,
    )
    .await?;
    verify_bundle_checksums(&program, &common, bundle, target, progress).await?;
    reset_after_bundle(&program, &common, target, progress).await?;
    Ok(())
}

pub(crate) fn stage_bundle_segments(
    bundle: &firmware_bundle::FirmwareBundle,
    workspace: &Path,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    for segment in &bundle.manifest.segments {
        let bytes = progress.require(bundle.images.get(&segment.path).ok_or_else(|| {
            HttpError::internal("validated bundle segment disappeared before execution")
        }))?;
        progress.require(
            fs::write(workspace.join(format!("{:?}.bin", segment.kind)), bytes)
                .map_err(|error| HttpError::internal(&format!("failed to stage segment: {error}"))),
        )?;
    }
    Ok(())
}

pub(crate) fn espflash_common_args(port_path: &str) -> Vec<String> {
    vec![
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port_path.into(),
        "--non-interactive".into(),
    ]
}

async fn run_bundle_erase_if_needed(
    operation: FirmwareOperation,
    program: &Path,
    common: &[String],
    initial_reset: &str,
    bundle: &firmware_bundle::FirmwareBundle,
    target: FirmwareFlashTarget<'_>,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let total_bytes = bundle
        .manifest
        .segments
        .iter()
        .map(|segment| segment.length)
        .sum::<u64>();
    if operation == FirmwareOperation::Update {
        progress.stage_started(
            "write_segments",
            json!({ "completedUnits": 0, "totalUnits": total_bytes, "unit": "bytes" }),
        );
        return Ok(());
    }
    progress.stage_started("erase", json!({}));
    let mut args = vec!["erase-flash".into()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".into(),
        initial_reset.into(),
        "--after".into(),
        "no-reset".into(),
    ]);
    progress
        .require(require_bundle_espflash_success_for_target(program, &args, target, true).await)?;
    progress.stage_completed("erase", json!({}));
    progress.stage_started(
        "write_segments",
        json!({ "completedUnits": 0, "totalUnits": total_bytes, "unit": "bytes" }),
    );
    Ok(())
}

async fn write_bundle_segments(
    program: &Path,
    common: &[String],
    bundle: &firmware_bundle::FirmwareBundle,
    workspace: &Path,
    target: FirmwareFlashTarget<'_>,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let total_bytes = bundle
        .manifest
        .segments
        .iter()
        .map(|segment| segment.length)
        .sum::<u64>();
    let mut completed_bytes = 0_u64;
    for segment in &bundle.manifest.segments {
        let path = workspace.join(format!("{:?}.bin", segment.kind));
        let args = build_bundle_write_bin_args(common, "no-reset", segment.address, &path);
        progress.require(
            require_bundle_espflash_success_for_target(program, &args, target, false).await,
        )?;
        completed_bytes = completed_bytes.saturating_add(segment.length);
        progress.stage_progress(
            "write_segments",
            json!({ "completedUnits": completed_bytes, "totalUnits": total_bytes, "unit": "bytes" }),
        );
    }
    progress.stage_completed(
        "write_segments",
        json!({ "completedUnits": completed_bytes, "totalUnits": total_bytes, "unit": "bytes" }),
    );
    Ok(())
}

async fn verify_bundle_checksums(
    program: &Path,
    common: &[String],
    bundle: &firmware_bundle::FirmwareBundle,
    target: FirmwareFlashTarget<'_>,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let total = bundle.manifest.segments.len();
    progress.stage_started(
        "rom_md5",
        json!({ "completedUnits": 0, "totalUnits": total, "unit": "segments" }),
    );
    for (index, segment) in bundle.manifest.segments.iter().enumerate() {
        let checksum = build_checksum_md5_args(common, segment.address, segment.length);
        let output = progress.require(
            require_bundle_espflash_success_for_target(program, &checksum, target, false).await,
        )?;
        if !String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains(&segment.md5)
        {
            return Err(progress.fail(HttpError::internal(
                "ROM MD5 did not match the validated bundle segment.",
            )));
        }
        progress.stage_progress(
            "rom_md5",
            json!({ "completedUnits": index + 1, "totalUnits": total, "unit": "segments" }),
        );
    }
    progress.stage_completed(
        "rom_md5",
        json!({ "completedUnits": total, "totalUnits": total, "unit": "segments" }),
    );
    Ok(())
}

async fn reset_after_bundle(
    program: &Path,
    common: &[String],
    target: FirmwareFlashTarget<'_>,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    progress.stage_started("reset", json!({}));
    let mut reset = vec!["reset".into()];
    reset.extend(common.iter().cloned());
    progress
        .require(require_bundle_espflash_success_for_target(program, &reset, target, true).await)?;
    progress.stage_completed("reset", json!({}));
    Ok(())
}

pub(crate) fn build_checksum_md5_args(common: &[String], address: u64, length: u64) -> Vec<String> {
    let mut args = vec!["checksum-md5".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        "no-reset".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        length.to_string(),
    ]);
    args
}

#[allow(dead_code)]
pub(crate) async fn require_bundle_espflash_success(
    program: &Path,
    args: &[String],
    port_path: &str,
) -> Result<Output, HttpError> {
    require_bundle_espflash_success_for_target(
        program,
        args,
        FirmwareFlashTarget {
            port_path,
            usb_identity: None,
        },
        args.first()
            .is_some_and(|command| matches!(command.as_str(), "erase-flash" | "reset")),
    )
    .await
}

async fn require_bundle_espflash_success_for_target(
    program: &Path,
    args: &[String],
    target: FirmwareFlashTarget<'_>,
    allow_connection_recovery: bool,
) -> Result<Output, HttpError> {
    require_usb_serial_identity(target.port_path, target.usb_identity)?;
    let output = run_espflash_command_with_identity(
        program,
        args,
        ESPFLASH_COMMAND_TIMEOUT,
        target.port_path,
        target.usb_identity,
    )
    .await?;
    require_usb_serial_identity(target.port_path, target.usb_identity)?;
    if output.status.success() {
        return Ok(output);
    }
    if !allow_connection_recovery
        || !is_esp_usb_serial_jtag_port(target.port_path)
        || !espflash_connection_failed(&output)
    {
        return Err(espflash_command_error(program, args, &output));
    }
    let Some(before_index) = args.iter().position(|argument| argument == "--before") else {
        return Err(espflash_command_error(program, args, &output));
    };
    let Some(before_reset) = args.get(before_index + 1).map(String::as_str) else {
        return Err(espflash_command_error(program, args, &output));
    };
    let recovery_modes: &[&str] = match before_reset {
        "no-reset" | "usb-reset" => &["usb-reset", "default-reset"],
        _ => return Err(espflash_command_error(program, args, &output)),
    };

    let mut attempts = vec![espflash_failure_details(program, args, &output)];
    for reset_mode in recovery_modes {
        let retry_args = replace_espflash_before_reset(args, reset_mode)
            .expect("bundle recovery modes require an espflash --before argument");
        tokio::time::sleep(ESPFLASH_USB_RESET_RETRY_DELAY).await;
        require_usb_serial_identity(target.port_path, target.usb_identity)?;
        let retry_output = run_espflash_command_with_identity(
            program,
            &retry_args,
            ESPFLASH_COMMAND_TIMEOUT,
            target.port_path,
            target.usb_identity,
        )
        .await?;
        require_usb_serial_identity(target.port_path, target.usb_identity)?;
        if retry_output.status.success() {
            return Ok(retry_output);
        }
        attempts.push(espflash_failure_details(
            program,
            &retry_args,
            &retry_output,
        ));
        if !espflash_connection_failed(&retry_output) {
            break;
        }
    }

    Err(HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "espflash_failed".to_string(),
            message: "Protected espflash transaction failed after USB recovery attempts."
                .to_string(),
            retryable: true,
            details: Some(json!({ "attempts": attempts })),
        },
    })
}

pub(crate) fn replace_espflash_before_reset(
    args: &[String],
    before_reset: &str,
) -> Option<Vec<String>> {
    let index = args.iter().position(|argument| argument == "--before")?;
    let mut replaced = args.to_vec();
    *replaced.get_mut(index + 1)? = before_reset.to_string();
    Some(replaced)
}

pub(crate) fn espflash_command_error(
    program: &Path,
    args: &[String],
    output: &Output,
) -> HttpError {
    HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "espflash_failed".to_string(),
            message: "Protected espflash transaction failed.".to_string(),
            retryable: true,
            details: Some(espflash_failure_details(program, args, output)),
        },
    }
}

async fn probe_native_rom_security_with_locks(
    state: &AppState,
    port_path: &str,
    serial_rpc: Option<&tokio::sync::OwnedMutexGuard<()>>,
    serial_lock: Option<&SerialPortProcessLock>,
    usb_identity: Option<&UsbSerialIdentity>,
) -> Result<RomSecurityInfo, HttpError> {
    let _owned_serial_rpc = if serial_rpc.is_none() {
        Some(acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?)
    } else {
        None
    };
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let _owned_serial_lock = if serial_lock.is_none() {
        Some(acquire_serial_process_lock(port_path, ESPFLASH_COMMAND_TIMEOUT).await?)
    } else {
        None
    };
    if let Some(usb_identity) = usb_identity {
        ensure_native_serial_identity(port_path, usb_identity)?;
    }
    let port_path = port_path.to_owned();
    let expected_usb_identity = usb_identity.cloned();
    tokio::task::spawn_blocking(move || {
        probe_native_rom_security_blocking(&port_path, expected_usb_identity.as_ref())
    })
    .await
    .map_err(|error| HttpError::internal(&format!("ROM security probe task failed: {error}")))?
    .map_err(|error| {
        HttpError::forbidden(
            "security_info_unknown",
            &format!("ROM security probe failed; flashing is blocked: {error}"),
        )
    })
}

fn probe_native_rom_security_blocking(
    port_path: &str,
    expected_usb_identity: Option<&UsbSerialIdentity>,
) -> Result<RomSecurityInfo, String> {
    use ::espflash::{
        connection::{Connection, ResetAfterOperation, ResetBeforeOperation},
        flasher::Flasher,
    };
    use serialport::{FlowControl, SerialPortType, UsbPortInfo};

    let port_info = serialport::available_ports()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|candidate| serial_port_paths_match(port_path, &candidate.port_name))
        .ok_or_else(|| "authorized serial port is no longer enumerated".to_string())?;
    if expected_usb_identity
        .as_ref()
        .is_some_and(|expected| !expected.matches_port_info(&port_info))
    {
        return Err("authorized USB target changed or disappeared".to_string());
    }
    let usb_info = match port_info.port_type {
        SerialPortType::UsbPort(info) => info,
        SerialPortType::PciPort | SerialPortType::Unknown => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
        _ => return Err("authorized port is not a supported USB serial target".to_string()),
    };
    let serial = serialport::new(port_path, 115_200)
        .flow_control(FlowControl::None)
        .open_native()
        .map_err(|error| error.to_string())?;
    let connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115_200,
    );
    let mut flasher = match Flasher::try_connect(connection, false, true, false, None, None) {
        Ok(flasher) => flasher,
        Err(error) => {
            let (connect_error, mut connection) = *error;
            let reset_error = connection.reset().err();
            return Err(match reset_error {
                Some(reset_error) => format!(
                    "ROM connection failed: {connect_error}; resetting target to runtime failed: {reset_error}"
                ),
                None => format!("ROM connection failed: {connect_error}"),
            });
        }
    };
    let probe_result = (|| -> Result<RomSecurityInfo, String> {
        let info = flasher.security_info().map_err(|error| error.to_string())?;
        let device = flasher.device_info().map_err(|error| error.to_string())?;
        const ESP32S3_EFUSE_BLOCK1: u32 = 0x6000_7044;
        let flash_cap = (flasher
            .connection()
            .read_reg(ESP32S3_EFUSE_BLOCK1 + 12)
            .map_err(|error| error.to_string())?
            >> 27)
            & 0x07;
        let psram_cap = (flasher
            .connection()
            .read_reg(ESP32S3_EFUSE_BLOCK1 + 16)
            .map_err(|error| error.to_string())?
            >> 3)
            & 0x03;
        Ok(RomSecurityInfo {
            rom_mac: device
                .mac_address
                .ok_or_else(|| "ROM MAC is unavailable".to_string())?,
            secure_boot_enabled: info.flags & 0x1 != 0,
            flash_encryption_enabled: info.flash_crypt_cnt.count_ones() % 2 == 1,
            secure_download_mode_enabled: info.flags & 0x4 != 0 || flasher.secure_download_mode(),
            response_known: true,
            chip_is_esp32s3: flasher.chip().to_string() == "esp32s3",
            flash_size_bytes: u64::from(device.flash_size.size()),
            package_matches: flash_cap == 2 && psram_cap == 2,
        })
    })();
    let reset_result = flasher
        .connection()
        .reset()
        .map_err(|error| error.to_string());
    finalize_rom_probe_result(probe_result, reset_result)
}

pub(crate) fn finalize_rom_probe_result<T>(
    probe_result: Result<T, String>,
    reset_result: Result<(), String>,
) -> Result<T, String> {
    match (probe_result, reset_result) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(probe_error), Ok(())) => Err(probe_error),
        (Ok(_), Err(reset_error)) => Err(format!(
            "ROM security probe passed; resetting target to runtime failed: {reset_error}"
        )),
        (Err(probe_error), Err(reset_error)) => Err(format!(
            "{probe_error}; resetting target to runtime failed: {reset_error}"
        )),
    }
}

pub(crate) fn firmware_preflight_stages() -> Vec<String> {
    [
        "artifact",
        "transport",
        "rom_reset",
        "chip_flash_security",
        "preflight",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn firmware_execution_stages(operation: FirmwareOperation) -> Vec<String> {
    let mut stages = vec!["authorization"];
    if operation == FirmwareOperation::InstallRecovery {
        stages.push("erase");
    }
    stages.extend([
        "write_segments",
        "rom_md5",
        "reset",
        "runtime_reconnect",
        "runtime_verify",
    ]);
    stages.into_iter().map(str::to_string).collect()
}
