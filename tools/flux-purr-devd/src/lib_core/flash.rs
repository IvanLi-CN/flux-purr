fn is_recoverable_serial_error_message(message: &str) -> bool {
    message.contains("Broken pipe")
        || message.contains("broken pipe")
        || message.contains("No such file or directory")
        || message.contains("Connection reset")
        || message.contains("Connection aborted")
        || message.contains("UnexpectedEof")
        || message.contains("Device not configured")
        || message.contains("device not configured")
}

async fn flash_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FlashRequest>,
) -> Result<Json<FlashResult>, HttpError> {
    let artifact_id = payload.artifact.artifact_id.clone();
    let dry_run_approval = flash_dry_run_approval(&payload)?;
    let port_path =
        resolve_flash_port(&state, &device_id, Some(&payload.lease_id), payload.dry_run)?;
    verify_flash_artifact(&state, &device_id, &payload.artifact, &artifact_id)?;

    if payload.dry_run {
        return Ok(Json(record_flash_dry_run(
            &state,
            &device_id,
            artifact_id,
            dry_run_approval,
        )?));
    }

    require_flash_dry_run(&state, &device_id, &dry_run_approval, &artifact_id)?;
    require_flash_confirmation(&state, &device_id, &payload.confirm, &artifact_id)?;
    require_real_flash_enabled(&state, &device_id, &artifact_id)?;
    execute_flash(
        &state,
        &device_id,
        &payload.artifact,
        &artifact_id,
        &port_path,
    )
    .await
}

fn resolve_flash_port(
    state: &AppState,
    device_id: &str,
    lease_id: Option<&str>,
    dry_run: bool,
) -> Result<String, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.require_lease(device_id, lease_id)?;
    let device = state_lock
        .devices
        .get(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    match device.transport {
        DeviceTransport::NativeSerial => device.port_path.clone().ok_or_else(|| {
            HttpError::bad_request("missing_port", "Native serial device has no port path.")
        }),
        DeviceTransport::Mock if dry_run => Ok(String::new()),
        DeviceTransport::Mock => Err(HttpError::bad_request(
            "real_flash_requires_native_serial",
            "Real flash requires a native serial target.",
        )),
        DeviceTransport::Lan => Err(HttpError::bad_request(
            "lan_flash_unsupported",
            "Firmware flashing is unavailable through the DEVD LAN bridge.",
        )),
    }
}

fn verify_flash_artifact(
    state: &AppState,
    device_id: &str,
    artifact: &FirmwareArtifact,
    artifact_id: &str,
) -> Result<(), HttpError> {
    let verification = verify_artifact(artifact, state.config.artifact_root.as_deref())
        .map_err(sanitize_io_error)?;
    if verification.verified {
        return Ok(());
    }
    state.emit(event(
        device_id,
        "flash",
        "artifact verification failed",
        json!({ "artifactId": artifact_id, "code": "artifact_verify_failed" }),
    ));
    Err(HttpError::bad_request(
        "artifact_verify_failed",
        "Firmware artifact verification failed.",
    ))
}

fn record_flash_dry_run(
    state: &AppState,
    device_id: &str,
    artifact_id: String,
    approval: FlashDryRunApproval,
) -> Result<FlashResult, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock
        .dry_run_passes
        .insert(device_id.to_string(), approval);
    if let Some(device) = state_lock.devices.get_mut(device_id) {
        device.selected_artifact_id = Some(artifact_id.clone());
    }
    drop(state_lock);
    state.emit(event(
        device_id,
        "flash",
        "artifact dry-run passed",
        json!({ "artifactId": artifact_id, "dryRun": true }),
    ));
    Ok(FlashResult {
        artifact_id,
        dry_run: true,
        status: "passed".to_string(),
        message: "Artifact verified; no flash write performed.".to_string(),
    })
}

fn require_flash_dry_run(
    state: &AppState,
    device_id: &str,
    approval: &FlashDryRunApproval,
    artifact_id: &str,
) -> Result<(), HttpError> {
    let state_lock = state.lock()?;
    if state_lock.dry_run_passes.get(device_id) == Some(approval) {
        return Ok(());
    }
    drop(state_lock);
    state.emit(event(
        device_id,
        "flash",
        "real flash blocked",
        json!({ "artifactId": artifact_id, "code": "dry_run_required" }),
    ));
    Err(HttpError::forbidden(
        "dry_run_required",
        "Real flash requires a successful dry-run for the same lease and artifact manifest.",
    ))
}

fn require_flash_confirmation(
    state: &AppState,
    device_id: &str,
    confirm: &Option<String>,
    artifact_id: &str,
) -> Result<(), HttpError> {
    if confirm.as_deref() == Some("FLASH") {
        return Ok(());
    }
    state.emit(event(
        device_id,
        "flash",
        "real flash blocked",
        json!({ "artifactId": artifact_id, "code": "confirmation_required" }),
    ));
    Err(HttpError::forbidden(
        "confirmation_required",
        "Real flash requires confirm=FLASH.",
    ))
}

fn require_real_flash_enabled(
    state: &AppState,
    device_id: &str,
    artifact_id: &str,
) -> Result<(), HttpError> {
    if state.config.allow_real_flash {
        return Ok(());
    }
    state.emit(event(
        device_id,
        "flash",
        "real flash blocked",
        json!({ "artifactId": artifact_id, "code": "real_flash_disabled" }),
    ));
    Err(HttpError::forbidden(
        "real_flash_disabled",
        "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
    ))
}
