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

#[expect(
    clippy::too_many_lines,
    reason = "firmware update handler preserves verification and event ordering"
)]
async fn local_firmware_update(
    State(state): State<AppState>,
    Json(payload): Json<LocalFirmwareUpdateRequest>,
) -> Result<Json<Value>, HttpError> {
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

    let target = {
        let serial_devices = scan_serial_devices(Some(Path::new(&port)));
        let mut state_lock = state.lock()?;
        refresh_serial_devices(&mut state_lock, serial_devices);
        state_lock
            .devices
            .values()
            .find(|device| {
                device.transport == DeviceTransport::NativeSerial
                    && device.port_path.as_deref() == Some(port.as_str())
            })
            .cloned()
            .ok_or_else(|| {
                HttpError::bad_request(
                    "serial_port_not_found",
                    "The supplied serial port is not present in the current device set.",
                )
            })?
    };
    if target.connection == ConnectionState::Error {
        return Err(HttpError::bad_request(
            "serial_port_missing",
            "The supplied serial port is not available; no replacement port will be selected.",
        ));
    }

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
    let identity_result =
        serial_request_payload::<Identity>(&state, &target, "get_identity", "identity").await;
    let install_result = serial_request_payload::<InstallStatus>(
        &state,
        &target,
        "get_install_status",
        "install_status",
    )
    .await;
    let verified = identity_result.as_ref().is_ok_and(|identity| {
        identity.firmware_version == bundle.manifest.identity.version
            && identity.git_sha == bundle.manifest.identity.source_sha
            && identity.build_id == bundle.manifest.identity.build_id
    }) && install_result.as_ref().is_ok_and(|status| {
        status.layout_id == bundle.manifest.layout.id
            && status.layout_version == bundle.manifest.layout.version
            && status.partition_table_sha256 == bundle.manifest.layout.partition_table_sha256
    });
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

#[expect(
    clippy::too_many_lines,
    reason = "firmware operation handler preserves dry-run and safety gates"
)]
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
    let initial_stage = if payload.dry_run {
        "artifact"
    } else {
        "authorization"
    };
    progress.stage_started(initial_stage, json!({}));

    let bundle_path = state.bundle_store.path().join(format!(
        "{}.fluxpurr-fw",
        payload.artifact_id.trim_start_matches("sha256:")
    ));
    let bundle = progress.require(firmware_bundle::read_bundle(&bundle_path).map_err(|error| {
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
    if payload.dry_run {
        progress.stage_completed("artifact", json!({}));
        progress.stage_started("transport", json!({}));
    }

    let target_result = {
        let mut inner = state.lock()?;
        inner
            .require_lease(&device_id, Some(&payload.lease_id))
            .and_then(|_| {
                let device = inner
                    .devices
                    .get(&device_id)
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
    let target = progress.require(target_result)?;
    let port_path = target
        .port_path
        .clone()
        .unwrap_or_else(|| "mock://esp32s3".into());
    let mock_identity = target.identity.device_id.clone();
    let transport = target.transport;
    let mut current_version = target.identity.firmware_version.clone();
    let mut status = target.status.clone();

    // An update preserves an existing Flux Purr installation, so it must stop
    // heat and use fresh runtime facts from this exact serial target before it
    // enters ROM mode. Discovery state can be stale while a heater is running.
    if payload.operation == FirmwareOperation::Update {
        let identity = match transport {
            DeviceTransport::NativeSerial => {
                let (identity, live_status) = progress.require(
                    refresh_native_update_runtime_facts(&state, &target, &payload.lease_id).await,
                )?;
                status = live_status;
                Some(identity)
            }
            DeviceTransport::Mock => {
                status.heater_enabled = false;
                status.heater_output_percent = 0;
                status.heater_physical_output_percent = 0;
                None
            }
            DeviceTransport::Lan => unreachable!(),
        };
        if let Some(identity) = identity.as_ref() {
            current_version = identity.firmware_version.clone();
        }
        if let Ok(mut inner) = state.lock()
            && let Some(device) = inner.devices.get_mut(&device_id)
            && device.port_path.as_deref() == target.port_path.as_deref()
        {
            if let Some(identity) = identity {
                device.identity = identity;
            }
            device.network = status.network.clone();
            device.status = status.clone();
            device.connection = ConnectionState::Connected;
        }
        progress.require(validate_update_runtime_facts(
            transport,
            &current_version,
            &status,
        ))?;
    }
    if payload.dry_run {
        progress.stage_completed("transport", json!({}));
        progress.stage_started("rom_reset", json!({}));
    }

    let security_result = match transport {
        DeviceTransport::Mock => RomSecurityInfo {
            rom_mac: mock_identity,
            secure_boot_enabled: false,
            flash_encryption_enabled: false,
            secure_download_mode_enabled: false,
            response_known: true,
            chip_is_esp32s3: true,
            flash_size_bytes: 4 * 1024 * 1024,
            package_matches: true,
        },
        DeviceTransport::NativeSerial => {
            progress.require(probe_native_rom_security(&state, &port_path).await)?
        }
        DeviceTransport::Lan => unreachable!(),
    };
    let security = security_result;
    if payload.dry_run {
        progress.stage_completed("rom_reset", json!({}));
        progress.stage_started("chip_flash_security", json!({}));
    }
    progress.require(security.validate_for_flash())?;
    let rom_mac = security.rom_mac.clone();
    if payload.dry_run {
        progress.stage_completed("chip_flash_security", json!({}));
    }

    if payload.operation == FirmwareOperation::Update {
        let current_semver = semver::Version::parse(
            current_version
                .trim_start_matches("fw/")
                .trim_start_matches('v'),
        );
        let target_semver =
            semver::Version::parse(bundle.manifest.identity.version.trim_start_matches('v'));
        if current_semver
            .ok()
            .zip(target_semver.ok())
            .is_some_and(|(current, target)| target < current)
            && !payload.allow_downgrade
        {
            return Err(HttpError::forbidden(
                "downgrade_confirmation_required",
                "The target firmware is older; explicit allowDowngrade is required.",
            ));
        }
    }
    if payload.dry_run {
        progress.stage_started("preflight", json!({}));
    }

    let preflight_digest = firmware_preflight_digest(
        &payload,
        &device_id,
        &port_path,
        &rom_mac,
        &bundle.bundle_sha256,
    );
    if payload.dry_run {
        let token = {
            let mut inner = state.lock()?;
            let token = inner.next_id("firmware-approval");
            inner.firmware_approvals.insert(
                token.clone(),
                FirmwareApproval {
                    lease_id: payload.lease_id.clone(),
                    device_id: device_id.clone(),
                    port_path,
                    rom_mac,
                    bundle_sha256: bundle.bundle_sha256.clone(),
                    operation: payload.operation,
                    allow_downgrade: payload.allow_downgrade,
                    preflight_digest,
                    expires_at: Instant::now() + Duration::from_secs(5 * 60),
                },
            );
            token
        };
        progress.stage_completed("preflight", json!({}));
        progress.operation_completed("passed");
        return Ok(Json(FirmwareOperationResult {
            operation_id: progress.operation_id().to_string(),
            artifact_id: bundle.bundle_sha256,
            operation: payload.operation,
            dry_run: true,
            outcome: "passed".into(),
            approval_token: Some(token),
            approval_expires_in_ms: Some(5 * 60 * 1000),
            stages: firmware_preflight_stages(),
            message: "Preflight passed; no flash write performed.".into(),
        }));
    }

    let token = progress.require(payload.approval_token.as_deref().ok_or_else(|| {
        HttpError::forbidden(
            "approval_required",
            "Execution requires a current single-use approval token.",
        )
    }))?;
    let approval_result = {
        let mut inner = state.lock()?;
        inner.firmware_approvals.remove(token).ok_or_else(|| {
            HttpError::forbidden(
                "approval_invalid",
                "The approval token is invalid or already used.",
            )
        })
    };
    let approval = progress.require(approval_result)?;
    if approval.expires_at <= Instant::now()
        || approval.lease_id != payload.lease_id
        || approval.device_id != device_id
        || approval.port_path != port_path
        || approval.rom_mac != rom_mac
        || approval.bundle_sha256 != bundle.bundle_sha256
        || approval.operation != payload.operation
        || approval.allow_downgrade != payload.allow_downgrade
        || approval.preflight_digest != preflight_digest
    {
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
    if transport != DeviceTransport::NativeSerial {
        return Err(progress.fail(HttpError::bad_request(
            "real_flash_requires_native_serial",
            "Real flash requires a native serial target.",
        )));
    }
    progress.stage_completed("authorization", json!({}));

    run_bundle_flash_transaction(
        &state,
        &bundle,
        payload.operation,
        &port_path,
        &mut progress,
    )
    .await?;
    let target_result = {
        let inner = match state.lock() {
            Ok(inner) => inner,
            Err(error) => return Err(progress.fail(error)),
        };
        inner
            .devices
            .get(&device_id)
            .cloned()
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))
    };
    let target = progress.require(target_result)?;
    progress.stage_started("runtime_reconnect", json!({}));
    // ESP32-S3 USB Serial/JTAG accepts the initial identity request during the
    // early boot control loop. The following install-status request reports
    // `startup_busy` until the runtime emits `boot_stage=runtime_ready`; the
    // serial exchange then performs exactly one retry on that marker. Sending
    // both requests only after the marker is not reliable on this transport.
    let identity =
        serial_request_payload::<Identity>(&state, &target, "get_identity", "identity").await;
    let install_status = serial_request_payload::<InstallStatus>(
        &state,
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
    let verified = identity.as_ref().is_ok_and(|identity| {
        identity.firmware_version == bundle.manifest.identity.version
            && identity.git_sha == bundle.manifest.identity.source_sha
            && identity.build_id == bundle.manifest.identity.build_id
    }) && install_status.as_ref().is_ok_and(|status| {
        status.layout_id == bundle.manifest.layout.id
            && status.layout_version == bundle.manifest.layout.version
            && status.partition_table_sha256 == bundle.manifest.layout.partition_table_sha256
    });
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
        artifact_id: bundle.bundle_sha256,
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

#[expect(
    clippy::too_many_lines,
    reason = "flash transaction keeps serial exclusivity and recovery ordering"
)]
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
    for segment in &bundle.manifest.segments {
        let bytes = progress.require(bundle.images.get(&segment.path).ok_or_else(|| {
            HttpError::internal("validated bundle segment disappeared before execution")
        }))?;
        progress.require(
            fs::write(
                workspace.path().join(format!("{:?}.bin", segment.kind)),
                bytes,
            )
            .map_err(|error| HttpError::internal(&format!("failed to stage segment: {error}"))),
        )?;
    }
    let program = resolve_espflash_program();
    let common = vec![
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port_path.into(),
        "--non-interactive".into(),
    ];
    let initial_reset = if is_esp_usb_serial_jtag_port(port_path) {
        "usb-reset"
    } else {
        "default-reset"
    };
    if operation == FirmwareOperation::Update {
        progress.stage_started("write_segments", json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.iter().map(|segment| segment.length).sum::<u64>(),
            "unit": "bytes",
        }));
    }
    if operation == FirmwareOperation::InstallRecovery {
        progress.stage_started("erase", json!({}));
        let mut args = vec!["erase-flash".into()];
        args.extend(common.clone());
        args.extend([
            "--before".into(),
            initial_reset.into(),
            "--after".into(),
            "no-reset".into(),
        ]);
        progress.require(require_bundle_espflash_success(&program, &args, port_path).await)?;
        progress.stage_completed("erase", json!({}));
        progress.stage_started("write_segments", json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.iter().map(|segment| segment.length).sum::<u64>(),
            "unit": "bytes",
        }));
    }
    let total_bytes = bundle
        .manifest
        .segments
        .iter()
        .map(|segment| segment.length)
        .sum::<u64>();
    let mut completed_bytes = 0_u64;
    for segment in &bundle.manifest.segments {
        let path = workspace.path().join(format!("{:?}.bin", segment.kind));
        let args = build_bundle_write_bin_args(&common, "no-reset", segment.address, &path);
        progress.require(require_bundle_espflash_success(&program, &args, port_path).await)?;
        completed_bytes = completed_bytes.saturating_add(segment.length);
        progress.stage_progress(
            "write_segments",
            json!({
                "completedUnits": completed_bytes,
                "totalUnits": total_bytes,
                "unit": "bytes",
            }),
        );
    }
    progress.stage_completed(
        "write_segments",
        json!({
            "completedUnits": completed_bytes,
            "totalUnits": total_bytes,
            "unit": "bytes",
        }),
    );
    progress.stage_started(
        "rom_md5",
        json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.len(),
            "unit": "segments",
        }),
    );
    for (index, segment) in bundle.manifest.segments.iter().enumerate() {
        let checksum = build_checksum_md5_args(&common, segment.address, segment.length);
        let output = progress
            .require(require_bundle_espflash_success(&program, &checksum, port_path).await)?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        if !stdout.contains(&segment.md5) {
            return Err(progress.fail(HttpError::internal(
                "ROM MD5 did not match the validated bundle segment.",
            )));
        }
        progress.stage_progress(
            "rom_md5",
            json!({
                "completedUnits": index + 1,
                "totalUnits": bundle.manifest.segments.len(),
                "unit": "segments",
            }),
        );
    }
    progress.stage_completed(
        "rom_md5",
        json!({
            "completedUnits": bundle.manifest.segments.len(),
            "totalUnits": bundle.manifest.segments.len(),
            "unit": "segments",
        }),
    );
    progress.stage_started("reset", json!({}));
    let mut reset = vec!["reset".into()];
    reset.extend(common);
    progress.require(require_bundle_espflash_success(&program, &reset, port_path).await)?;
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
