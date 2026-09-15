pub(crate) use super::*;

pub(crate) fn espflash_reset_modes(
    artifact: &FirmwareArtifact,
    port_path: &str,
) -> Vec<&'static str> {
    if artifact.target_chip == "esp32s3" && port_path.contains("usbmodem") {
        vec!["usb-reset", "usb-reset", "default-reset"]
    } else {
        vec!["default-reset"]
    }
}

pub(crate) fn requires_lease(state: &DevdState, device_id: &str) -> bool {
    state
        .devices
        .get(device_id)
        .map(|device| {
            matches!(
                device.transport,
                DeviceTransport::NativeSerial | DeviceTransport::Lan
            )
        })
        .unwrap_or(true)
}

pub(crate) fn device<'a>(
    state: &'a DevdState,
    device_id: &str,
) -> Result<&'a DeviceRecord, HttpError> {
    state
        .devices
        .get(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))
}

pub(crate) fn record_serial_bridge_error(
    state: &AppState,
    device_id: &str,
    stage: &'static str,
    error: &HttpError,
) {
    if let Ok(mut state_lock) = state.lock()
        && let Some(device) = state_lock.devices.get_mut(device_id)
    {
        device.connection = ConnectionState::Error;
    }
    state.emit(event(
        device_id,
        "serial",
        "native serial RPC failed",
        json!({
            "stage": stage,
            "code": error.error.code,
            "message": error.error.message,
            "retryable": error.error.retryable,
        }),
    ));
}

pub(crate) fn emit_wifi_config_event(
    state: &AppState,
    device_id: &str,
    payload: &WifiConfigRequest,
) {
    let message = match payload.op {
        WifiConfigOp::Set | WifiConfigOp::Clear => "wifi config accepted",
        WifiConfigOp::Cancel => "wifi cancellation confirmed",
    };
    state.emit(event(
        device_id,
        "wifi",
        message,
        json!({
            "op": payload.op,
            "ssid": payload.ssid,
            "passwordPresent": payload.password.is_some(),
            "telemetryIntervalMs": payload.telemetry_interval_ms,
        }),
    ));
}

pub(crate) fn emit_runtime_config_event(
    state: &AppState,
    device_id: &str,
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) {
    state.emit(event(
        device_id,
        "runtime",
        "runtime config applied",
        json!({
            "requested": {
                "targetTempC": payload.target_temp_c,
                "selectedPresetSlot": payload.selected_preset_slot,
                "presetsC": payload.presets_c,
                "activeCoolingEnabled": payload.active_cooling_enabled,
                "postHeatCoolingMode": payload.post_heat_cooling_mode,
                "heatingFanGuardMode": payload.heating_fan_guard_mode,
                "heaterEnabled": payload.heater_enabled,
                "manualPpsEnabled": payload.manual_pps_enabled,
                "manualPpsMv": payload.manual_pps_mv,
                "manualPpsMa": payload.manual_pps_ma,
                "faultAttentionAcknowledged": payload.fault_attention_acknowledged,
            },
            "status": {
                "targetTempC": status.target_temp_c,
                "selectedPresetSlot": status.selected_preset_slot,
                "presetsC": status.presets_c,
                "activeCoolingEnabled": status.active_cooling_enabled,
                "postHeatCoolingMode": status.post_heat_cooling_mode,
                "heatingFanGuardMode": status.heating_fan_guard_mode,
                "fanPolicySource": status.fan_policy_source,
                "fanOutputLevel": status.fan_output_level,
                "heaterEnabled": status.heater_enabled,
                "manualPpsEnabled": status.manual_pps_enabled,
                "manualPpsMv": status.manual_pps_mv,
                "manualPpsMa": status.manual_pps_ma,
                "faultAttentionPending": status.fault_attention_pending,
            },
        }),
    ));
}

pub(crate) fn emit_calibration_event(
    state: &AppState,
    device_id: &str,
    op: &CalibrationConfigOp,
    calibration: &CalibrationState,
) {
    state.emit(event(
        device_id,
        "calibration",
        "calibration updated",
        json!({
            "op": op,
            "fittedFit": {
                "rtdAdc": calibration.rtd_adc.fitted_fit,
                "vinAdc": calibration.vin_adc.fitted_fit,
            },
            "slots": {
                "rtdAdc": calibration.rtd_adc.slots,
                "vinAdc": calibration.vin_adc.slots,
            },
            "activeSlot": {
                "rtdAdc": calibration.rtd_adc.active_slot,
                "vinAdc": calibration.vin_adc.active_slot,
            },
            "samples": {
                "rtdAdc": calibration.rtd_adc.samples.iter().flatten().count(),
                "vinAdc": calibration.vin_adc.samples.iter().flatten().count(),
            },
        }),
    ));
}

pub(crate) fn record_transport_event(
    state: &AppState,
    device_id: &str,
    direction: &str,
    transport: &str,
    request_id: &str,
    frame_json: &str,
) {
    let frame = serde_json::from_str::<Value>(frame_json)
        .map(redact_transport_frame)
        .unwrap_or_else(|_| json!({ "raw": frame_json }));
    let frame_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("frame")
        .to_string();
    state.emit(event(
        device_id,
        "transport",
        "transport frame",
        json!({
            "direction": direction,
            "transport": transport,
            "requestId": request_id,
            "frameType": frame_type,
            "frame": frame,
        }),
    ));
}

pub(crate) fn redact_transport_frame(mut frame: Value) -> Value {
    redact_sensitive_fields(&mut frame);
    frame
}

pub(crate) fn redact_sensitive_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, field) in object.iter_mut() {
                if is_sensitive_field_key(key) {
                    *field = Value::String("<redacted>".to_string());
                } else if key.eq_ignore_ascii_case("lan_pairing_code") {
                    redact_lan_pairing_code(field);
                } else {
                    redact_sensitive_fields(field);
                }
            }
        }
        Value::Array(values) => {
            for field in values {
                redact_sensitive_fields(field);
            }
        }
        _ => {}
    }
}

pub(crate) fn redact_lan_pairing_code(value: &mut Value) {
    if let Value::Object(object) = value
        && let Some(code) = object.get_mut("code")
    {
        *code = Value::String("<redacted>".to_string());
    }
}

pub(crate) fn is_sensitive_field_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("password") || key.eq_ignore_ascii_case("psk")
}

pub(crate) fn flash_dry_run_approval(
    payload: &FlashRequest,
) -> Result<FlashDryRunApproval, HttpError> {
    Ok(FlashDryRunApproval {
        lease_id: payload.lease_id.clone(),
        artifact_fingerprint: artifact_fingerprint(&payload.artifact)?,
    })
}

pub(crate) fn artifact_fingerprint(artifact: &FirmwareArtifact) -> Result<String, HttpError> {
    let bytes = serde_json::to_vec(artifact)
        .map_err(|_| HttpError::internal("Failed to fingerprint firmware artifact."))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn push_bounded<T>(values: &mut VecDeque<T>, value: T, limit: usize) {
    if values.len() >= limit {
        values.pop_front();
    }
    values.push_back(value);
}

pub(crate) fn event(device_id: &str, kind: &str, message: &str, payload: Value) -> DevdEvent {
    DevdEvent {
        id: format!(
            "event-{}-{}",
            now_millis(),
            EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
        timestamp: timestamp(),
        device_id: Some(device_id.to_string()),
        kind: kind.to_string(),
        message: message.to_string(),
        payload,
    }
}

pub(crate) fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn timestamp() -> String {
    now_millis().to_string()
}

pub(crate) fn expired_instant() -> Instant {
    Instant::now()
}

pub(crate) fn sanitize_io_error(error: io::Error) -> HttpError {
    HttpError::bad_request(
        "artifact_io_error",
        &format!("Artifact file error: {}", error.kind()),
    )
}
