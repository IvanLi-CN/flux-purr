fn validate_manual_pps_against_status(
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

async fn verify_artifact_route(
    State(state): State<AppState>,
    Json(payload): Json<ArtifactVerifyRequest>,
) -> Result<Json<ArtifactVerifyResult>, HttpError> {
    verify_artifact(&payload.artifact, state.config.artifact_root.as_deref())
        .map(Json)
        .map_err(sanitize_io_error)
}

async fn list_artifacts_route(
    State(state): State<AppState>,
) -> Result<Json<FirmwareArtifactCatalog>, HttpError> {
    discover_firmware_artifacts(state.config.artifact_root.as_deref())
        .map(|artifacts| Json(FirmwareArtifactCatalog { artifacts }))
        .map_err(sanitize_io_error)
}

async fn list_firmware_bundles(
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

async fn import_firmware_bundle(
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

async fn local_firmware_update(
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
    progress.operation_started();
    progress.stage_started("authorization", json!({ "port": port }));
    progress.stage_completed("authorization", json!({ "port": port }));
    let preflight_lease_id = format!("local-update-{}", progress.operation_id());
    progress.stage_started("preflight", json!({}));
    let (identity, status) =
        refresh_native_update_runtime_facts(&state, &target, &preflight_lease_id).await?;
    validate_update_runtime_facts(
        DeviceTransport::NativeSerial,
        &identity.firmware_version,
        &status,
    )?;
    let security = probe_native_rom_security(&state, &port).await?;
    security.validate_for_flash()?;
    progress.stage_completed("preflight", json!({}));
    run_bundle_flash_transaction(
        &state,
        &bundle,
        FirmwareOperation::Update,
        &port,
        &mut progress,
    )
    .await?;
    progress.stage_started("runtime_reconnect", json!({}));
    let verified = verify_reconnected_firmware(&state, &target, &bundle).await;
    if verified {
        progress.stage_completed("runtime_reconnect", json!({}));
    } else {
        progress.stage_failed("runtime_reconnect", "runtime_verification_failed");
    }
    let outcome = if verified {
        "verified"
    } else {
        "write_complete_unverified"
    };
    progress.operation_completed(outcome);
    Ok(Json(json!({
        "ok": true,
        "operation": "update",
        "port": port,
        "artifactId": bundle.bundle_sha256,
        "outcome": outcome,
    })))
}

fn validate_local_update_request(
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

fn find_local_update_target(state: &AppState, port: &str) -> Result<DeviceRecord, HttpError> {
    let serial_devices = scan_serial_devices(Some(Path::new(port)));
    let mut state_lock = state.lock()?;
    refresh_serial_devices(&mut state_lock, serial_devices);
    let target = state_lock
        .devices
        .values()
        .find(|device| {
            device.transport == DeviceTransport::NativeSerial
                && device.port_path.as_deref() == Some(port)
        })
        .cloned()
        .ok_or_else(|| {
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

async fn verify_reconnected_firmware(
    state: &AppState,
    target: &DeviceRecord,
    bundle: &firmware_bundle::FirmwareBundle,
) -> bool {
    let identity = serial_request_payload::<Identity>(state, target, "get_identity", "identity").await;
    let install_status = serial_request_payload::<InstallStatus>(
        state,
        target,
        "get_install_status",
        "install_status",
    )
    .await;
    identity.as_ref().is_ok_and(|identity| {
        identity.firmware_version == bundle.manifest.identity.version
            && identity.git_sha == bundle.manifest.identity.source_sha
            && identity.build_id == bundle.manifest.identity.build_id
    }) && install_status.as_ref().is_ok_and(|status| {
        status.layout_id == bundle.manifest.layout.id
            && status.layout_version == bundle.manifest.layout.version
            && status.partition_table_sha256 == bundle.manifest.layout.partition_table_sha256
    })
}

fn bundle_summary(bundle: &firmware_bundle::FirmwareBundle) -> FirmwareBundleSummary {
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

fn bundle_http_error(error: firmware_bundle::BundleError) -> HttpError {
    HttpError::bad_request("firmware_bundle_invalid", &error.to_string())
}

fn validate_update_runtime_facts(
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

async fn refresh_native_update_runtime_facts(
    state: &AppState,
    target: &DeviceRecord,
    lease_id: &str,
) -> Result<(Identity, ControlPlaneStatus), HttpError> {
    let identity =
        serial_request_payload::<Identity>(state, target, "get_identity", "identity").await?;
    let _stopped = serial_runtime_config(
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
    )
    .await?;
    let status =
        serial_request_payload::<ControlPlaneStatus>(state, target, "get_status", "status").await?;
    Ok((identity, status))
}

struct PreparedFirmwareOperation {
    bundle: firmware_bundle::FirmwareBundle,
    target: DeviceRecord,
    port_path: String,
    transport: DeviceTransport,
    current_version: String,
    status: ControlPlaneStatus,
    rom_mac: String,
}

async fn firmware_operation(
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
    let prepared = prepare_firmware_operation(&state, &device_id, &payload, &mut progress).await?;
    if payload.dry_run {
        progress.stage_started("preflight", json!({}));
    }

    let preflight_digest = firmware_preflight_digest(
        &payload,
        &device_id,
        &prepared.port_path,
        &prepared.rom_mac,
        &prepared.bundle.bundle_sha256,
    );
    if payload.dry_run {
        let token = create_firmware_approval(&state, &device_id, &payload, &prepared, preflight_digest)?;
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

    authorize_firmware_operation(&state, &device_id, &payload, &prepared, preflight_digest, &mut progress)?;
    progress.stage_completed("authorization", json!({}));

    run_bundle_flash_transaction(
        &state,
        &prepared.bundle,
        payload.operation,
        &prepared.port_path,
        &mut progress,
    )
    .await?;
    let verified = reconnect_firmware_operation(&state, &device_id, &prepared, &mut progress).await?;
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

async fn prepare_firmware_operation(
    state: &AppState,
    device_id: &str,
    payload: &FirmwareOperationRequest,
    progress: &mut FirmwareOperationProgress,
) -> Result<PreparedFirmwareOperation, HttpError> {
    progress.stage_started(
        if payload.dry_run { "artifact" } else { "authorization" },
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
    };
    refresh_operation_facts(state, device_id, payload, &mut prepared, progress).await?;
    if payload.dry_run {
        progress.stage_completed("transport", json!({}));
        progress.stage_started("rom_reset", json!({}));
    }
    prepared.rom_mac = operation_rom_security(state, &prepared, progress).await?;
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

fn load_operation_bundle(
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

fn load_operation_target(
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

async fn refresh_operation_facts(
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
            let (identity, status) = progress
                .require(refresh_native_update_runtime_facts(state, &prepared.target, &payload.lease_id).await)?;
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
    }
    update_operation_device(state, device_id, prepared, identity)?;
    progress.require(validate_update_runtime_facts(
        prepared.transport,
        &prepared.current_version,
        &prepared.status,
    ))
}

fn update_operation_device(
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

async fn operation_rom_security(
    state: &AppState,
    prepared: &PreparedFirmwareOperation,
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
        DeviceTransport::NativeSerial => {
            progress.require(probe_native_rom_security(state, &prepared.port_path).await)?
        }
        DeviceTransport::Lan => unreachable!(),
    };
    progress.require(security.validate_for_flash())?;
    Ok(security.rom_mac)
}

fn validate_operation_downgrade(
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
    if current.ok().zip(target.ok()).is_some_and(|(current, target)| target < current) {
        return Err(HttpError::forbidden(
            "downgrade_confirmation_required",
            "The target firmware is older; explicit allowDowngrade is required.",
        ));
    }
    Ok(())
}

fn create_firmware_approval(
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

fn authorize_firmware_operation(
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
    let approval = progress.require(state.lock()?.firmware_approvals.remove(token).ok_or_else(|| {
        HttpError::forbidden("approval_invalid", "The approval token is invalid or already used.")
    }))?;
    let matches = approval.expires_at > Instant::now()
        && approval.lease_id == payload.lease_id
        && approval.device_id == device_id
        && approval.port_path == prepared.port_path
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

async fn reconnect_firmware_operation(
    state: &AppState,
    device_id: &str,
    prepared: &PreparedFirmwareOperation,
    progress: &mut FirmwareOperationProgress,
) -> Result<bool, HttpError> {
    let target = {
        let inner = progress.require(state.lock())?;
        inner
            .devices
            .get(device_id)
            .cloned()
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))
    }?;
    progress.stage_started("runtime_reconnect", json!({}));
    let identity = serial_request_payload::<Identity>(state, &target, "get_identity", "identity").await;
    let install_status = serial_request_payload::<InstallStatus>(
        state,
        &target,
        "get_install_status",
        "install_status",
    )
    .await;
    if identity.is_ok() && install_status.is_ok() {
        progress.stage_completed("runtime_reconnect", json!({}));
    } else {
        progress.stage_failed("runtime_reconnect", "runtime_reconnect_failed");
    }
    progress.stage_started("runtime_verify", json!({}));
    Ok(identity.as_ref().is_ok_and(|identity| {
        identity.firmware_version == prepared.bundle.manifest.identity.version
            && identity.git_sha == prepared.bundle.manifest.identity.source_sha
            && identity.build_id == prepared.bundle.manifest.identity.build_id
    }) && install_status.as_ref().is_ok_and(|status| {
        status.layout_id == prepared.bundle.manifest.layout.id
            && status.layout_version == prepared.bundle.manifest.layout.version
            && status.partition_table_sha256 == prepared.bundle.manifest.layout.partition_table_sha256
    }))
}

async fn run_bundle_flash_transaction(
    state: &AppState,
    bundle: &firmware_bundle::FirmwareBundle,
    operation: FirmwareOperation,
    port_path: &str,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let _serial_rpc = progress.require(
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await,
    )?;
    progress.require(drop_cached_serial_session(
        &state.serial_sessions,
        port_path,
    ))?;
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
        port_path,
        bundle,
        progress,
    )
    .await?;
    write_bundle_segments(&program, &common, port_path, bundle, workspace.path(), progress).await?;
    verify_bundle_checksums(&program, &common, port_path, bundle, progress).await?;
    reset_after_bundle(&program, &common, port_path, progress).await?;
    Ok(())
}

fn stage_bundle_segments(
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

fn espflash_common_args(port_path: &str) -> Vec<String> {
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
    port_path: &str,
    bundle: &firmware_bundle::FirmwareBundle,
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
    progress.require(require_bundle_espflash_success(program, &args, port_path).await)?;
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
    port_path: &str,
    bundle: &firmware_bundle::FirmwareBundle,
    workspace: &Path,
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
        progress.require(require_bundle_espflash_success(program, &args, port_path).await)?;
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
    port_path: &str,
    bundle: &firmware_bundle::FirmwareBundle,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let total = bundle.manifest.segments.len();
    progress.stage_started(
        "rom_md5",
        json!({ "completedUnits": 0, "totalUnits": total, "unit": "segments" }),
    );
    for (index, segment) in bundle.manifest.segments.iter().enumerate() {
        let checksum = build_checksum_md5_args(common, segment.address, segment.length);
        let output = progress
            .require(require_bundle_espflash_success(program, &checksum, port_path).await)?;
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
    port_path: &str,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    progress.stage_started("reset", json!({}));
    let mut reset = vec!["reset".into()];
    reset.extend(common.iter().cloned());
    progress.require(require_bundle_espflash_success(program, &reset, port_path).await)?;
    progress.stage_completed("reset", json!({}));
    Ok(())
}

fn build_checksum_md5_args(common: &[String], address: u64, length: u64) -> Vec<String> {
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

async fn require_bundle_espflash_success(
    program: &Path,
    args: &[String],
    port_path: &str,
) -> Result<Output, HttpError> {
    let output = run_espflash_command_with_timeout(program, args, ESPFLASH_COMMAND_TIMEOUT).await?;
    if output.status.success() {
        return Ok(output);
    }
    if !is_esp_usb_serial_jtag_port(port_path) || !espflash_connection_failed(&output) {
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
        let retry_output =
            run_espflash_command_with_timeout(program, &retry_args, ESPFLASH_COMMAND_TIMEOUT)
                .await?;
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

fn replace_espflash_before_reset(args: &[String], before_reset: &str) -> Option<Vec<String>> {
    let index = args.iter().position(|argument| argument == "--before")?;
    let mut replaced = args.to_vec();
    *replaced.get_mut(index + 1)? = before_reset.to_string();
    Some(replaced)
}

fn espflash_command_error(program: &Path, args: &[String], output: &Output) -> HttpError {
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

async fn probe_native_rom_security(
    state: &AppState,
    port_path: &str,
) -> Result<RomSecurityInfo, HttpError> {
    use espflash::{
        connection::{Connection, ResetAfterOperation, ResetBeforeOperation},
        flasher::Flasher,
    };
    use serialport::{FlowControl, SerialPortType, UsbPortInfo};

    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let port_path = port_path.to_owned();
    tokio::task::spawn_blocking(move || {
        let port_info = serialport::available_ports()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|candidate| candidate.port_name == port_path)
            .ok_or_else(|| "authorized serial port is no longer enumerated".to_string())?;
        let usb_info = match port_info.port_type {
            SerialPortType::UsbPort(info) => info,
            SerialPortType::PciPort | SerialPortType::Unknown => UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
            },
            _ => {
                return Err(String::from(
                    "authorized port is not a supported USB serial target",
                ));
            }
        };
        let serial = serialport::new(&port_path, 115_200)
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
        let mut flasher = Flasher::connect(connection, false, true, false, None, None)
            .map_err(|error| error.to_string())?;
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

fn firmware_preflight_stages() -> Vec<String> {
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

fn firmware_execution_stages(operation: FirmwareOperation) -> Vec<String> {
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
