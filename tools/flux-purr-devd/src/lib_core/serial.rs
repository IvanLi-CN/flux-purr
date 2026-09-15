pub(crate) use super::*;

pub(crate) fn firmware_preflight_digest(
    payload: &FirmwareOperationRequest,
    device_id: &str,
    port_path: &str,
    rom_mac: &str,
    bundle_sha256: &str,
) -> String {
    let value = json!({
        "leaseId": payload.lease_id,
        "deviceId": device_id,
        "portPath": port_path,
        "romMac": rom_mac,
        "bundleSha256": bundle_sha256,
        "operation": payload.operation,
        "allowDowngrade": payload.allow_downgrade,
    });
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&value).unwrap()))
    )
}

pub(crate) async fn serial_request_payload<T>(
    state: &AppState,
    target: &DeviceRecord,
    op: &'static str,
    payload_key: &'static str,
) -> Result<T, HttpError>
where
    T: DeserializeOwned + Send + 'static,
{
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-{op}", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op,
    })
    .map_err(|_| HttpError::internal("failed to encode USB request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::ReadOnly,
    )
    .await?;
    extract_usb_payload(result, payload_key)
}

pub(crate) async fn serial_wifi_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &WifiConfigRequest,
) -> Result<NetworkSummary, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-wifi", now_millis());
    let request = serde_json::to_string(&UsbWifiConfigWire {
        frame_type: "wifi_config",
        request_id: &request_id,
        op: payload.op.usb_op(),
        ssid: payload.ssid.as_deref(),
        password: payload.password.as_deref(),
        static_ipv4: payload.static_ipv4,
        telemetry_interval_ms: payload.telemetry_interval_ms,
    })
    .map_err(|_| HttpError::internal("failed to encode USB WiFi request"))?;
    let response = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_wifi_config_network(response, payload.op == WifiConfigOp::Cancel)
}

pub(crate) fn extract_wifi_config_network(
    result: Value,
    accepts_idle_cancellation: bool,
) -> Result<NetworkSummary, HttpError> {
    let receipt = extract_usb_payload::<UsbWifiConfigReceipt>(result, "wifi")?;
    if receipt.network.configuration_generation == 0 || receipt.network.transition_sequence == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_wifi_receipt",
            "The device returned an unversioned WiFi receipt.",
            true,
        ));
    }
    if matches!(
        receipt.network.state,
        NetworkState::Saving | NetworkState::Timeout
    ) || (receipt.network.state == NetworkState::Idle && !accepts_idle_cancellation)
    {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_wifi_receipt",
            "The device returned a non-public WiFi state.",
            false,
        ));
    }
    Ok(receipt.network)
}

pub(crate) async fn serial_clear_lan_pairing(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<(), HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-lan-reset", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op: "clear_lan_pairing_token",
    })
    .map_err(|_| HttpError::internal("failed to encode USB LAN reset request"))?;
    let _ = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    Ok(())
}

pub(crate) async fn serial_lan_pairing_code(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<LanPairingCode, HttpError> {
    let code = serial_request_payload::<LanPairingCode>(
        state,
        target,
        "get_lan_pairing_code",
        "lan_pairing_code",
    )
    .await?;
    validate_lan_pairing_code(code)
}

pub(crate) async fn serial_open_lan_pairing_window(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<LanPairingCode, HttpError> {
    let code = serial_request_payload::<LanPairingCode>(
        state,
        target,
        "open_lan_pairing_window",
        "lan_pairing_code",
    )
    .await?;
    validate_lan_pairing_code(code)
}

pub(crate) async fn serial_close_lan_pairing_window(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<(), HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-lan-pairing-close", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op: "close_lan_pairing_window",
    })
    .map_err(|_| HttpError::internal("failed to encode USB LAN pairing-window close request"))?;
    let _ = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    Ok(())
}

pub(crate) fn validate_lan_pairing_code(code: LanPairingCode) -> Result<LanPairingCode, HttpError> {
    let valid_code = code
        .code
        .as_deref()
        .is_some_and(|value| value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit()));
    if (code.active && !valid_code) || (!code.active && code.code.is_some()) {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_lan_pairing_code",
            "USB response returned an invalid LAN pairing-code state.",
            true,
        ));
    }
    Ok(code)
}

pub(crate) async fn serial_runtime_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<ControlPlaneStatus, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-runtime", now_millis());
    let request = serde_json::to_string(&UsbRuntimeConfigWire {
        frame_type: "runtime_config",
        request_id: &request_id,
        target_temp_c: payload.target_temp_c,
        selected_preset_slot: payload.selected_preset_slot,
        presets_c: payload.presets_c.as_ref(),
        active_cooling_enabled: payload.active_cooling_enabled,
        post_heat_cooling_mode: payload.post_heat_cooling_mode.as_ref(),
        heating_fan_guard_mode: payload.heating_fan_guard_mode.as_ref(),
        heater_enabled: payload.heater_enabled,
        manual_pps_enabled: payload.manual_pps_enabled,
        manual_pps_mv: payload.manual_pps_mv,
        manual_pps_ma: payload.manual_pps_ma,
        fault_attention_acknowledged: payload.fault_attention_acknowledged,
        calibration: payload.calibration.as_ref(),
        thermal_profile_mode: payload.thermal_profile_mode.as_ref(),
        thermal_control_profile: payload.thermal_control_profile.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB runtime request"))?;
    match serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await
    {
        Ok(result) => extract_usb_payload(result, "status"),
        Err(error) if should_reconcile_runtime_config_timeout(&error) => {
            match serial_request_payload::<ControlPlaneStatus>(
                state,
                target,
                "get_status",
                "status",
            )
            .await
            {
                Ok(status) if runtime_config_matches_status(payload, &status) => Ok(status),
                Ok(_) | Err(_) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

pub(crate) async fn serial_buzzer_test(
    state: &AppState,
    target: &DeviceRecord,
    payload: &BuzzerTestRequest,
) -> Result<BuzzerTestStatus, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-buzzer-test", now_millis());
    let request = serde_json::to_string(&UsbBuzzerTestWire {
        frame_type: "buzzer_test",
        request_id: &request_id,
        op: payload.op,
        buzzer_cue: payload.cue,
        buzzer_scenario: payload.scenario,
        repeat: payload.repeat,
    })
    .map_err(|_| HttpError::internal("failed to encode USB buzzer test request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "buzzer_test")
}

pub(crate) async fn serial_calibration_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationState, HttpError> {
    let mut calibration =
        serial_request_payload::<CalibrationState>(state, target, "get_calibration", "calibration")
            .await?;
    merge_live_calibration_metadata(&mut calibration, &target.calibration);
    Ok(calibration)
}

pub(crate) async fn serial_calibration_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &CalibrationConfigRequest,
) -> Result<CalibrationState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-calibration", now_millis());
    let request = serde_json::to_string(&UsbCalibrationConfigWire {
        frame_type: "calibration_config",
        request_id: &request_id,
        op: payload.op,
        channel: payload.channel,
        reference_temp_c: payload.reference_temp_c,
        reference_vin_mv: payload.reference_vin_mv,
        target_adc_mv: payload.target_adc_mv,
        observed_mv: payload.observed_mv,
        expected_mv: payload.expected_mv,
        sample_index: payload.sample_index,
        state: payload.state.as_ref(),
        slot: payload.slot,
        fit: payload.fit.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB calibration request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    let mut calibration = extract_usb_payload(result, "calibration")?;
    backfill_live_calibration_capture(&mut calibration, payload);
    merge_live_calibration_metadata(&mut calibration, &target.calibration);
    Ok(calibration)
}

pub(crate) fn backfill_live_calibration_capture(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) {
    if payload.op != CalibrationConfigOp::Capture
        || payload.channel != Some(CalibrationChannel::RtdAdc)
    {
        return;
    }
    let Some(sample) = calibration.rtd_adc.samples.iter_mut().flatten().last() else {
        return;
    };
    if sample.reference_temp_c.is_none() {
        sample.reference_temp_c = payload.reference_temp_c;
    }
    if sample.target_adc_mv.is_none() {
        sample.target_adc_mv = payload.target_adc_mv;
    }
    if let Some(target_adc_mv) = payload.target_adc_mv {
        sample.expected_mv = target_adc_mv;
    }
    calibration.rtd_adc.refresh(CalibrationChannel::RtdAdc);
}

pub(crate) fn merge_live_calibration_metadata(
    calibration: &mut CalibrationState,
    previous: &CalibrationState,
) {
    merge_live_rtd_sample_metadata(&mut calibration.rtd_adc.samples, &previous.rtd_adc.samples);
    calibration.refresh_fits();
}

pub(crate) fn merge_live_rtd_sample_metadata(
    samples: &mut [Option<CalibrationSample>],
    previous: &[Option<CalibrationSample>],
) {
    for sample in samples.iter_mut().flatten() {
        if sample.reference_temp_c.is_some() && sample.target_adc_mv.is_some() {
            continue;
        }
        let Some(existing) = previous.iter().flatten().find(|existing| {
            existing.observed_mv == sample.observed_mv && existing.expected_mv == sample.expected_mv
        }) else {
            continue;
        };
        if sample.reference_temp_c.is_none() {
            sample.reference_temp_c = existing.reference_temp_c;
        }
        if sample.target_adc_mv.is_none() {
            sample.target_adc_mv = existing.target_adc_mv;
        }
    }
}

pub(crate) async fn serial_calibration_job_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationJobState, HttpError> {
    serial_request_payload::<CalibrationJobState>(
        state,
        target,
        "get_calibration_job",
        "calibration_job",
    )
    .await
}

pub(crate) async fn serial_thermal_plant_run_get(
    state: &AppState,
    target: &DeviceRecord,
    after_sample: u8,
) -> Result<ThermalPlantRunSnapshot, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-thermal-plant-run", now_millis());
    let request = serde_json::to_string(&UsbThermalPlantRunWire {
        frame_type: "thermal_plant_run",
        request_id: &request_id,
        after_sample,
    })
    .map_err(|_| HttpError::internal("failed to encode thermal plant run request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::ReadOnly,
    )
    .await?;
    extract_usb_payload(result, "thermal_plant_run")
}

pub(crate) async fn serial_calibration_job_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &CalibrationJobRequest,
) -> Result<CalibrationJobState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-calibration-job", now_millis());
    let request = serde_json::to_string(&UsbCalibrationJobWire {
        frame_type: "calibration_job",
        request_id: &request_id,
        op: payload.op,
        kind: payload.kind,
    })
    .map_err(|_| HttpError::internal("failed to encode USB calibration job request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "calibration_job")
}

pub(crate) const EEPROM_CAPACITY_BYTES: usize = 8 * 1024;
pub(crate) const EEPROM_MAINTENANCE_CHUNK_MAX: usize = 32;

pub(crate) fn validate_eeprom_maintenance_request(
    payload: &EepromMaintenanceRequest,
) -> Result<(), HttpError> {
    match payload.op {
        EepromMaintenanceOp::Read => {
            let (Some(offset), Some(length)) = (payload.offset, payload.length) else {
                return Err(HttpError::bad_request(
                    "eeprom_range_required",
                    "EEPROM read requires offset and length.",
                ));
            };
            if length == 0
                || usize::from(length) > EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + usize::from(length) > EEPROM_CAPACITY_BYTES
                || payload.bytes.is_some()
            {
                return Err(HttpError::bad_request(
                    "eeprom_range_invalid",
                    "EEPROM read range is invalid.",
                ));
            }
        }
        EepromMaintenanceOp::Write => {
            let (Some(offset), Some(bytes)) = (payload.offset, payload.bytes.as_ref()) else {
                return Err(HttpError::bad_request(
                    "eeprom_write_required",
                    "EEPROM write requires offset and bytes.",
                ));
            };
            if bytes.is_empty()
                || bytes.len() > EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + bytes.len() > EEPROM_CAPACITY_BYTES
                || payload.length.is_some()
            {
                return Err(HttpError::bad_request(
                    "eeprom_range_invalid",
                    "EEPROM write range is invalid.",
                ));
            }
        }
        EepromMaintenanceOp::Erase => {
            if payload.offset.is_some() || payload.length.is_some() || payload.bytes.is_some() {
                return Err(HttpError::bad_request(
                    "eeprom_erase_payload_invalid",
                    "EEPROM erase does not accept a range or content.",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) async fn serial_eeprom_maintenance(
    state: &AppState,
    target: &DeviceRecord,
    payload: &EepromMaintenanceRequest,
) -> Result<EepromMaintenanceResponse, HttpError> {
    validate_eeprom_maintenance_request(payload)?;
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-eeprom", now_millis());
    let request = serde_json::to_string(&UsbEepromMaintenanceWire {
        frame_type: "eeprom_maintenance",
        request_id: &request_id,
        op: payload.op,
        offset: payload.offset,
        length: payload.length,
        bytes: payload.bytes.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode EEPROM maintenance request"))?;
    let result = serial_exchange_sensitive(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        match payload.op {
            EepromMaintenanceOp::Read => SerialRetryPolicy::ReadOnly,
            EepromMaintenanceOp::Write | EepromMaintenanceOp::Erase => {
                SerialRetryPolicy::SingleShot
            }
        },
    )
    .await?;
    let bytes = if payload.op == EepromMaintenanceOp::Read {
        Some(extract_usb_payload(result, "eeprom_bytes")?)
    } else {
        None
    };
    Ok(EepromMaintenanceResponse { bytes })
}

pub(crate) async fn configure_eeprom_maintenance(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<EepromMaintenanceRequest>,
) -> Result<Json<EepromMaintenanceResponse>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "EEPROM maintenance requires native USB serial transport.",
        ));
    }
    serial_eeprom_maintenance(&state, &target, &payload)
        .await
        .map(Json)
}

pub(crate) async fn serial_heater_curve_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<HeaterCurveState, HttpError> {
    serial_request_payload::<HeaterCurveState>(state, target, "get_heater_curve", "heater_curve")
        .await
}

pub(crate) async fn serial_heater_curve_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &HeaterCurveConfigRequest,
) -> Result<HeaterCurveState, HttpError> {
    let package = if let Some(package) = payload.package.as_ref() {
        validate_heater_curve_package(package)?;
        Some(normalize_heater_curve_package(package.clone()))
    } else {
        None
    };
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-heater-curve", now_millis());
    let request = serde_json::to_string(&UsbHeaterCurveConfigWire {
        frame_type: "heater_curve_config",
        request_id: &request_id,
        op: payload.op,
        heater_curve: package.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB heater curve request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "heater_curve")
}

pub(crate) async fn serial_heater_curve_save(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<HeaterCurveState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-heater-curve-save", now_millis());
    let request = serde_json::to_string(&UsbHeaterCurveSaveWire {
        frame_type: "heater_curve_save",
        request_id: &request_id,
    })
    .map_err(|_| HttpError::internal("failed to encode USB heater curve save request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "heater_curve")
}

pub(crate) async fn serial_exchange(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
) -> Result<Value, HttpError> {
    serial_exchange_with_visibility(
        state,
        device_id,
        port_path,
        request_id,
        request,
        retry_policy,
        true,
    )
    .await
}

pub(crate) async fn serial_exchange_sensitive(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
) -> Result<Value, HttpError> {
    serial_exchange_with_visibility(
        state,
        device_id,
        port_path,
        request_id,
        request,
        retry_policy,
        false,
    )
    .await
}

pub(crate) async fn serial_exchange_with_visibility(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
    record_payload: bool,
) -> Result<Value, HttpError> {
    if record_payload {
        record_transport_event(state, device_id, "tx", "usb_jsonl", &request_id, &request);
    }
    let serial_sessions = state.serial_sessions.clone();
    let worker_request_id = request_id.clone();
    let worker_device_id = device_id.to_string();
    let worker_events = state.events.clone();
    let worker_inner = state.inner.clone();
    let result = spawn_serial_worker(state.serial_rpc.clone(), move || {
        serial_exchange_blocking(SerialExchangeContext {
            state: &worker_inner,
            events: &worker_events,
            device_id: &worker_device_id,
            serial_sessions: &serial_sessions,
            port_path: &port_path,
            request_id: &worker_request_id,
            request: &request,
            retry_policy,
        })
    })
    .await?;

    if !record_payload {
        return result;
    }

    match &result {
        Ok(payload) => record_transport_event(
            state,
            device_id,
            "rx",
            "usb_jsonl",
            &request_id,
            &serde_json::to_string(&json!({
                "type": "response",
                "requestId": request_id,
                "ok": true,
                "result": payload,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        ),
        Err(error) => record_transport_event(
            state,
            device_id,
            "rx",
            "usb_jsonl",
            &request_id,
            &serde_json::to_string(&json!({
                "type": "response",
                "requestId": request_id,
                "ok": false,
                "error": error.error,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        ),
    }

    result
}

pub(crate) async fn spawn_serial_worker<T, F>(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    worker: F,
) -> Result<T, HttpError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    spawn_serial_worker_with_timeout(serial_rpc, SERIAL_RPC_TIMEOUT, worker).await
}

pub(crate) async fn spawn_serial_worker_with_timeout<T, F>(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    lock_timeout: Duration,
    worker: F,
) -> Result<T, HttpError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let serial_rpc = acquire_serial_rpc_with_timeout(serial_rpc, lock_timeout).await?;
    tokio::task::spawn_blocking(move || {
        let _serial_rpc = serial_rpc;
        worker()
    })
    .await
    .map_err(|_| HttpError::internal("serial worker failed"))
}

pub(crate) fn native_port_path(target: &DeviceRecord) -> Result<String, HttpError> {
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "Native serial transport is required.",
        ));
    }
    target.port_path.clone().ok_or_else(|| {
        HttpError::bad_request("missing_port", "Native serial device has no port path.")
    })
}

pub(crate) fn extract_usb_payload<T>(
    result: Value,
    payload_key: &'static str,
) -> Result<T, HttpError>
where
    T: DeserializeOwned,
{
    let payload = result.get(payload_key).cloned().ok_or_else(|| {
        HttpError::new(
            StatusCode::BAD_GATEWAY,
            "usb_payload_missing",
            "USB response did not include the expected payload.",
            true,
        )
    })?;
    serde_json::from_value(payload).map_err(|error| HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "usb_payload_decode_failed".to_string(),
            message: "USB response payload could not be decoded.".to_string(),
            retryable: true,
            details: Some(json!({
                "payloadKey": payload_key,
                "decodeError": error.to_string(),
            })),
        },
    })
}

pub(crate) struct SerialExchangeContext<'a> {
    state: &'a Arc<Mutex<DevdState>>,
    events: &'a broadcast::Sender<DevdEvent>,
    device_id: &'a str,
    serial_sessions: &'a Arc<Mutex<SerialSessionMap>>,
    port_path: &'a str,
    request_id: &'a str,
    request: &'a str,
    retry_policy: SerialRetryPolicy,
}

pub(crate) fn serial_exchange_blocking(
    context: SerialExchangeContext<'_>,
) -> Result<Value, HttpError> {
    let SerialExchangeContext {
        state,
        events,
        device_id,
        serial_sessions,
        port_path,
        request_id,
        request,
        retry_policy,
    } = context;
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let deadline = Instant::now() + serial_rpc_timeout(retry_policy);
    let mut session = take_or_open_serial_session(&mut serial_sessions, port_path, deadline)?;
    session = write_serial_request_with_reopen(session, port_path, request, deadline)?;

    // A USB Serial/JTAG port may reset the target as it opens. Do not keep
    // resending a JSONL command while the runtime is starting: the firmware
    // will later process every queued duplicate and pollute the following RPC.
    // `startup_busy` is the one explicit signal that a status request needs a
    // retry after the observable runtime-ready marker.
    let mut retry_after_runtime_ready = false;
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();
    let mut discarding_overlong_line = false;

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => std::thread::sleep(SERIAL_READ_TIMEOUT),
            Ok(read) => {
                let result = process_serial_read_chunk(
                    SerialResponseLineContext {
                        state,
                        events,
                        device_id,
                        port_path,
                        request_id,
                        request,
                        deadline,
                    },
                    &read_buf[..read],
                    &mut line,
                    &mut discarding_overlong_line,
                    session,
                    &mut serial_sessions,
                    retry_after_runtime_ready,
                )?;
                match result {
                    SerialChunkResult::Continue {
                        next_session,
                        retry_after_runtime_ready: next_retry,
                    } => {
                        session = next_session;
                        retry_after_runtime_ready = next_retry;
                    }
                    SerialChunkResult::Response(payload) => return Ok(payload),
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if error.kind() == io::ErrorKind::WouldBlock {
                    std::thread::sleep(SERIAL_READ_TIMEOUT);
                }
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                session = write_serial_request_with_reopen(session, port_path, request, deadline)?;
                retry_after_runtime_ready = false;
                line.clear();
                discarding_overlong_line = false;
            }
            Err(error) => return Err(serial_io_http_error(error)),
        }
    }

    // Keep the USB-JTAG session open after a timeout. Reopening this class of
    // port can reset the MCU; JSONL newline framing and request-id matching let
    // the next RPC discard any stale partial response without reopening it.
    store_serial_session(&mut serial_sessions, port_path, session);
    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "usb_response_timeout",
        "Timed out waiting for a matching USB JSONL response.",
        true,
    ))
}

pub(crate) fn observe_post_flash_boot_blocking(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<BootObservation, HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let deadline = Instant::now() + POST_FLASH_BOOT_TIMEOUT;
    let mut session = reopen_serial_session(port_path, deadline)?;
    let mut observation = BootObservation::default();
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();
    let mut discarding_overlong_line = false;

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => {}
            Ok(read) => {
                if process_boot_read_chunk(
                    state,
                    events,
                    device_id,
                    &read_buf[..read],
                    &mut line,
                    &mut discarding_overlong_line,
                    &mut observation,
                )? {
                    store_serial_session(&mut serial_sessions, port_path, session);
                    return Ok(observation);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if error.kind() == io::ErrorKind::WouldBlock {
                    std::thread::sleep(SERIAL_READ_TIMEOUT);
                }
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                line.clear();
                discarding_overlong_line = false;
            }
            Err(error) => return Err(serial_io_http_error(error)),
        }
    }

    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "firmware_boot_timeout",
        &format!(
            "Firmware did not reach runtime_ready within {} seconds; last stage: {}.",
            POST_FLASH_BOOT_TIMEOUT.as_secs(),
            observation.last_stage.as_deref().unwrap_or("none")
        ),
        false,
    ))
}

pub(crate) async fn observe_post_flash_boot(
    state: &AppState,
    device_id: &str,
    port_path: &str,
) -> Result<BootObservation, HttpError> {
    let state_lock = state.inner.clone();
    let events = state.events.clone();
    let device_id = device_id.to_string();
    let serial_sessions = state.serial_sessions.clone();
    let port_path = port_path.to_string();
    spawn_serial_worker_with_timeout(
        state.serial_rpc.clone(),
        POST_FLASH_BOOT_TIMEOUT + SERIAL_RPC_TIMEOUT,
        move || {
            observe_post_flash_boot_blocking(
                &state_lock,
                &events,
                &device_id,
                &serial_sessions,
                &port_path,
            )
        },
    )
    .await?
}

pub(crate) fn serial_rpc_timeout(retry_policy: SerialRetryPolicy) -> Duration {
    match retry_policy {
        SerialRetryPolicy::ReadOnly => SERIAL_READ_ONLY_RPC_TIMEOUT,
        SerialRetryPolicy::SingleShot => SERIAL_RPC_TIMEOUT,
    }
}

pub(crate) fn emit_serial_log_line(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    line: &[u8],
) {
    let Ok(message) = std::str::from_utf8(line) else {
        return;
    };
    let message = message.trim();
    if message.is_empty() || message.starts_with('{') {
        return;
    }

    let raw_event = event(
        device_id,
        "serial",
        "native serial monitor line",
        json!({
            "code": "firmware_log",
            "line": message,
        }),
    );
    let persistence_event = parse_persistence_fault_log(message).map(|payload| {
        event(
            device_id,
            "persistence_fault",
            "firmware persistence fault",
            payload,
        )
    });

    if let Ok(mut inner) = state.lock() {
        inner.push_event(raw_event.clone());
        if let Some(persistence_event) = persistence_event.as_ref() {
            inner.push_event(persistence_event.clone());
        }
    }
    let _ = events.send(raw_event);
    if let Some(persistence_event) = persistence_event {
        let _ = events.send(persistence_event);
    }
}

pub(crate) fn parse_persistence_fault_log(message: &str) -> Option<Value> {
    let terminal = if message.starts_with("PERSISTENCE_COMMIT_FAILED ") {
        true
    } else if message.starts_with("PERSISTENCE_COMMIT_ATTEMPT_FAILED ") {
        false
    } else {
        return None;
    };

    let mut code = None;
    let mut phase = None;
    let mut attempt = None;
    let mut sequence = None;
    let mut slot = None;
    for token in message.split_whitespace().skip(1) {
        let (key, value) = token.split_once('=')?;
        match key {
            "code" => code = Some(value.to_string()),
            "phase" => phase = Some(value.to_string()),
            "attempt" => attempt = value.parse::<u8>().ok(),
            "sequence" => sequence = value.parse::<u32>().ok(),
            "slot" => slot = Some(value.to_string()),
            _ => {}
        }
    }

    Some(json!({
        "event": if terminal { "commit_failed" } else { "commit_attempt_failed" },
        "code": code?,
        "phase": phase?,
        "attempt": attempt?,
        "sequence": sequence?,
        "slot": slot?,
    }))
}

pub(crate) fn serial_line_is_usb_reset_marker(line: &[u8]) -> bool {
    matches!(
        std::str::from_utf8(line).map(str::trim),
        Ok("reset_reason=core_usb_uart" | "reset_reason=core_usb_jtag")
    )
}

pub(crate) fn serial_line_is_runtime_ready(line: &[u8]) -> bool {
    matches!(
        std::str::from_utf8(line).map(str::trim),
        Ok(RUNTIME_READY_BOOT_STAGE)
    )
}

pub(crate) fn serial_line_finished(
    line: &mut Vec<u8>,
    discarding_overlong_line: &mut bool,
    byte: u8,
) -> bool {
    if byte == b'\n' {
        if *discarding_overlong_line {
            *discarding_overlong_line = false;
            line.clear();
            return false;
        }
        return true;
    }
    if !*discarding_overlong_line {
        if line.len() < SERIAL_LINE_LIMIT {
            line.push(byte);
        } else {
            line.clear();
            *discarding_overlong_line = true;
        }
    }
    false
}

pub(crate) enum SerialLineAction {
    Continue(bool),
    Response(Value),
    Failure(HttpError),
}

#[derive(Clone, Copy)]
pub(crate) struct SerialResponseLineContext<'a> {
    state: &'a Arc<Mutex<DevdState>>,
    events: &'a broadcast::Sender<DevdEvent>,
    device_id: &'a str,
    port_path: &'a str,
    request_id: &'a str,
    request: &'a str,
    deadline: Instant,
}

pub(crate) enum SerialChunkResult {
    Continue {
        next_session: SerialSession,
        retry_after_runtime_ready: bool,
    },
    Response(Value),
}

pub(crate) fn process_serial_read_chunk(
    context: SerialResponseLineContext<'_>,
    bytes: &[u8],
    line: &mut Vec<u8>,
    discarding_overlong_line: &mut bool,
    mut session: SerialSession,
    serial_sessions: &mut SerialSessionMap,
    mut retry_after_runtime_ready: bool,
) -> Result<SerialChunkResult, HttpError> {
    for byte in bytes {
        if !serial_line_finished(line, discarding_overlong_line, *byte) {
            continue;
        }
        let (next_session, action) =
            process_serial_response_line(context, session, retry_after_runtime_ready, line)?;
        session = next_session;
        match action {
            SerialLineAction::Continue(next_retry) => retry_after_runtime_ready = next_retry,
            SerialLineAction::Response(payload) => {
                store_serial_session(serial_sessions, context.port_path, session);
                return Ok(SerialChunkResult::Response(payload));
            }
            SerialLineAction::Failure(error) => {
                store_serial_session(serial_sessions, context.port_path, session);
                return Err(error);
            }
        }
        line.clear();
    }
    Ok(SerialChunkResult::Continue {
        next_session: session,
        retry_after_runtime_ready,
    })
}

pub(crate) fn process_serial_response_line(
    context: SerialResponseLineContext<'_>,
    session: SerialSession,
    retry_after_runtime_ready: bool,
    line: &[u8],
) -> Result<(SerialSession, SerialLineAction), HttpError> {
    emit_serial_log_line(context.state, context.events, context.device_id, line);
    if serial_line_is_usb_reset_marker(line) {
        // Opening an ESP32-S3 USB Serial/JTAG port can itself trigger this
        // reset. Keep the fd open until the runtime-ready marker arrives.
        return Ok((session, SerialLineAction::Continue(true)));
    }
    if should_retry_request_after_runtime_ready(
        retry_after_runtime_ready,
        line,
        Instant::now(),
        context.deadline,
    ) {
        let session = write_serial_request_with_reopen(
            session,
            context.port_path,
            context.request,
            context.deadline,
        )?;
        return Ok((session, SerialLineAction::Continue(false)));
    }
    let action = match decode_usb_response_line(line, context.request_id) {
        Ok(Some(payload)) => SerialLineAction::Response(payload),
        Ok(None) => SerialLineAction::Continue(retry_after_runtime_ready),
        Err(error) if is_retryable_startup_busy(&error) && Instant::now() < context.deadline => {
            SerialLineAction::Continue(true)
        }
        Err(error) => SerialLineAction::Failure(error),
    };
    Ok((session, action))
}

pub(crate) fn process_boot_observation_line(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    line: &[u8],
    observation: &mut BootObservation,
) -> Result<bool, HttpError> {
    emit_serial_log_line(state, events, device_id, line);
    let Ok(text) = std::str::from_utf8(line) else {
        return Ok(false);
    };
    observation.observe_line(text)
}

pub(crate) fn process_boot_read_chunk(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    bytes: &[u8],
    line: &mut Vec<u8>,
    discarding_overlong_line: &mut bool,
    observation: &mut BootObservation,
) -> Result<bool, HttpError> {
    for byte in bytes {
        if !serial_line_finished(line, discarding_overlong_line, *byte) {
            continue;
        }
        if process_boot_observation_line(state, events, device_id, line, observation)? {
            return Ok(true);
        }
        line.clear();
    }
    Ok(false)
}

pub(crate) type SerialSessionMap = HashMap<String, SerialSession>;

pub(crate) struct SerialSession {
    _serial_lock: SerialPortProcessLock,
    port: Box<dyn SerialSessionPort>,
}

pub(crate) trait SerialSessionPort: Read + Write + Send {
    fn begin_write(&mut self) -> Result<(), HttpError>;
    fn finish_write(&mut self) -> Result<(), HttpError>;
}

impl SerialSessionPort for Box<dyn serialport::SerialPort> {
    fn begin_write(&mut self) -> Result<(), HttpError> {
        self.set_timeout(SERIAL_WRITE_TIMEOUT)
            .map_err(serial_timeout_config_http_error)
    }

    fn finish_write(&mut self) -> Result<(), HttpError> {
        self.set_timeout(SERIAL_READ_TIMEOUT)
            .map_err(serial_timeout_config_http_error)
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct RawUsbSerialJtagPort {
    file: File,
}

#[cfg(target_os = "macos")]
impl Read for RawUsbSerialJtagPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

#[cfg(target_os = "macos")]
impl Write for RawUsbSerialJtagPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(target_os = "macos")]
impl SerialSessionPort for RawUsbSerialJtagPort {
    fn begin_write(&mut self) -> Result<(), HttpError> {
        Ok(())
    }

    fn finish_write(&mut self) -> Result<(), HttpError> {
        Ok(())
    }
}

pub(crate) fn lock_serial_sessions(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
) -> Result<MutexGuard<'_, SerialSessionMap>, HttpError> {
    serial_sessions
        .lock()
        .map_err(|_| HttpError::internal("serial session lock poisoned"))
}

pub(crate) fn take_or_open_serial_session(
    serial_sessions: &mut SerialSessionMap,
    port_path: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    serial_sessions
        .remove(port_path)
        .map(Ok)
        .unwrap_or_else(|| open_serial_session(port_path, deadline))
}

pub(crate) fn store_serial_session(
    serial_sessions: &mut SerialSessionMap,
    port_path: &str,
    session: SerialSession,
) {
    serial_sessions.insert(port_path.to_string(), session);
}

pub(crate) struct SerialPortProcessLock {
    #[cfg(unix)]
    file: File,
}

impl SerialPortProcessLock {
    pub(crate) fn acquire(port_path: &str, deadline: Instant) -> Result<Self, HttpError> {
        #[cfg(unix)]
        {
            Self::acquire_unix(port_path, deadline).map(|file| Self { file })
        }

        #[cfg(not(unix))]
        {
            let _ = (port_path, deadline);
            Ok(Self {})
        }
    }

    #[cfg(unix)]
    pub(crate) fn acquire_unix(port_path: &str, deadline: Instant) -> Result<File, HttpError> {
        let lock_path = serial_lock_path(port_path);
        let file = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| {
                HttpError::new(
                    StatusCode::BAD_GATEWAY,
                    "serial_lock_failed",
                    &format!(
                        "Failed to open serial lock {}: {error}",
                        lock_path.display()
                    ),
                    true,
                )
            })?;
        while Instant::now() < deadline {
            // SAFETY: flock is called with a valid file descriptor owned by `file`.
            if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
                return Ok(file);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::WouldBlock {
                return Err(serial_io_http_error(error));
            }
            std::thread::sleep(SERIAL_READ_TIMEOUT);
        }
        Err(HttpError::new(
            StatusCode::GATEWAY_TIMEOUT,
            "serial_lock_timeout",
            "Timed out waiting for exclusive USB serial access.",
            true,
        ))
    }
}

#[cfg(unix)]
impl Drop for SerialPortProcessLock {
    fn drop(&mut self) {
        // SAFETY: flock is called with a valid file descriptor owned by `self.file`.
        let _ = unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
    }
}

#[cfg(unix)]
pub(crate) fn serial_lock_path(port_path: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(port_path.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::from("flux-purr-devd-serial-");
    for byte in &digest[..8] {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".lock");
    std::env::temp_dir().join(name)
}

pub(crate) fn is_esp_usb_serial_jtag_port(port_path: &str) -> bool {
    port_path.starts_with("/dev/cu.usbmodem")
}

pub(crate) fn open_serial_port(port_path: &str) -> Result<Box<dyn SerialSessionPort>, HttpError> {
    #[cfg(target_os = "macos")]
    if is_esp_usb_serial_jtag_port(port_path) {
        let file = File::options()
            .read(true)
            .write(true)
            .custom_flags(MACOS_O_NONBLOCK)
            .open(port_path)
            .map_err(|error| {
                HttpError::new(
                    StatusCode::BAD_GATEWAY,
                    "serial_open_failed",
                    &format!("Failed to open serial port: {error}"),
                    true,
                )
            })?;
        return Ok(Box::new(RawUsbSerialJtagPort { file }));
    }

    // USB Serial/JTAG does not use modem-control lines.  Explicit DTR/RTS writes
    // can reset an attached MCU, so leave those lines entirely to the driver.
    serialport::new(port_path, DEFAULT_BAUD_RATE)
        .timeout(SERIAL_READ_TIMEOUT)
        .open()
        .map(|port| Box::new(port) as Box<dyn SerialSessionPort>)
        .map_err(|error| {
            HttpError::new(
                StatusCode::BAD_GATEWAY,
                "serial_open_failed",
                &format!("Failed to open serial port: {error}"),
                true,
            )
        })
}

pub(crate) fn open_serial_session(
    port_path: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    let serial_lock = SerialPortProcessLock::acquire(port_path, deadline)?;
    let port = open_serial_port(port_path)?;
    Ok(SerialSession {
        _serial_lock: serial_lock,
        port,
    })
}

pub(crate) fn reopen_serial_session(
    port_path: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    while Instant::now() < deadline {
        if Path::new(port_path).exists() {
            match open_serial_session(port_path, deadline) {
                Ok(session) => return Ok(session),
                Err(error) if error.error.retryable => {}
                Err(error) => return Err(error),
            }
        }
        std::thread::sleep(SERIAL_STARTUP_RETRY_DELAY);
    }

    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "serial_reconnect_timeout",
        "Timed out waiting for the USB serial port to reappear.",
        true,
    ))
}

pub(crate) fn write_serial_request(
    port: &mut dyn SerialSessionPort,
    request: &str,
) -> Result<(), HttpError> {
    validate_serial_request_len(request)?;
    port.begin_write()?;
    let write_result = port
        .write_all(request.as_bytes())
        .and_then(|_| port.write_all(b"\n"))
        .and_then(|_| port.flush());
    let restore_result = port.finish_write();
    write_result.map_err(serial_io_http_error)?;
    restore_result
}

pub(crate) fn validate_serial_request_len(request: &str) -> Result<(), HttpError> {
    if request.len().saturating_add(1) > SERIAL_LINE_LIMIT {
        return Err(HttpError::bad_request(
            "usb_request_too_large",
            "USB JSONL request exceeds the firmware line limit.",
        ));
    }
    Ok(())
}

pub(crate) fn serial_timeout_config_http_error(error: serialport::Error) -> HttpError {
    HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_timeout_config_failed",
        &format!("Failed to configure serial timeout: {error}"),
        true,
    )
}

pub(crate) fn write_serial_request_with_reopen(
    mut session: SerialSession,
    port_path: &str,
    request: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    match write_serial_request(&mut *session.port, request) {
        Ok(()) => Ok(session),
        Err(error) if is_recoverable_write_http_error(&error) => {
            drop(session);
            let mut reopened = reopen_serial_session(port_path, deadline)?;
            write_serial_request(&mut *reopened.port, request)?;
            Ok(reopened)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn should_retry_request_after_runtime_ready(
    retry_pending: bool,
    line: &[u8],
    now: Instant,
    deadline: Instant,
) -> bool {
    retry_pending && now < deadline && serial_line_is_runtime_ready(line)
}

pub(crate) fn is_recoverable_serial_io_error(error: &io::Error) -> bool {
    let message = error.to_string();
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof
    ) || message.contains("Device not configured")
        || message.contains("device not configured")
}

pub(crate) fn is_retryable_startup_busy(error: &HttpError) -> bool {
    error.error.retryable && error.error.code == "startup_busy"
}

pub(crate) fn should_reconcile_runtime_config_timeout(error: &HttpError) -> bool {
    error.error.retryable && error.error.code == "usb_response_timeout"
}

pub(crate) fn runtime_config_matches_status(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> bool {
    runtime_request_matches_basic_fields(payload, status)
        && runtime_request_matches_calibration(payload, status)
        && runtime_request_matches_thermal_profile(payload, status)
}

pub(crate) fn runtime_request_matches_basic_fields(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> bool {
    if payload
        .target_temp_c
        .is_some_and(|target_temp_c| status.target_temp_c != target_temp_c)
    {
        return false;
    }
    if payload
        .thermal_profile_mode
        .as_deref()
        .is_some_and(|mode| status.thermal_profile_mode != mode)
    {
        return false;
    }
    if payload
        .selected_preset_slot
        .is_some_and(|selected_preset_slot| {
            status.selected_preset_slot != Some(selected_preset_slot)
        })
    {
        return false;
    }
    if let Some(presets_c) = payload.presets_c.as_ref()
        && status.presets_c.as_ref() != Some(presets_c)
    {
        return false;
    }
    if payload
        .active_cooling_enabled
        .is_some_and(|enabled| status.active_cooling_enabled != enabled)
    {
        return false;
    }
    if payload
        .post_heat_cooling_mode
        .as_deref()
        .is_some_and(|mode| status.post_heat_cooling_mode != mode)
    {
        return false;
    }
    if payload
        .heating_fan_guard_mode
        .as_deref()
        .is_some_and(|mode| status.heating_fan_guard_mode != mode)
    {
        return false;
    }
    if payload
        .heater_enabled
        .is_some_and(|enabled| status.heater_enabled != enabled)
    {
        return false;
    }
    if payload
        .manual_pps_enabled
        .is_some_and(|enabled| status.manual_pps_enabled != enabled)
    {
        return false;
    }
    if payload
        .manual_pps_mv
        .is_some_and(|manual_pps_mv| status.manual_pps_mv != Some(manual_pps_mv))
    {
        return false;
    }
    if payload
        .manual_pps_ma
        .is_some_and(|manual_pps_ma| status.manual_pps_ma != Some(manual_pps_ma))
    {
        return false;
    }
    if payload.fault_attention_acknowledged == Some(true) && status.fault_attention_pending {
        return false;
    }
    true
}

pub(crate) fn runtime_request_matches_calibration(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> bool {
    if let Some(calibration) = payload.calibration.as_ref() {
        if calibration
            .mode
            .is_some_and(|mode| status.calibration.mode != mode)
        {
            return false;
        }
        if calibration
            .pps_enabled
            .is_some_and(|enabled| status.calibration.pps_enabled != enabled)
        {
            return false;
        }
        if calibration
            .pps_mv
            .is_some_and(|pps_mv| status.calibration.pps_mv != Some(pps_mv))
        {
            return false;
        }
        if calibration.heater_enabled.is_some_and(|enabled| {
            status.calibration.heater_enabled != enabled || status.heater_enabled != enabled
        }) {
            return false;
        }
        if calibration
            .target_adc_mv
            .is_some_and(|target_adc_mv| status.calibration.target_adc_mv != Some(target_adc_mv))
        {
            return false;
        }
    }
    true
}

pub(crate) fn runtime_request_matches_thermal_profile(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> bool {
    if let Some(profile) = payload.thermal_control_profile.as_ref() {
        match profile.op {
            ThermalControlProfileOp::Preview => {
                let expected =
                    mock_thermal_runtime(status.target_temp_c, profile.profile.as_ref(), true);
                if !status.thermal_control_profile_preview || status.thermal_control != expected {
                    return false;
                }
            }
            ThermalControlProfileOp::ClearPreview => {
                if status.thermal_control_profile_preview
                    || status.thermal_control.profile_source == "preview"
                {
                    return false;
                }
            }
            ThermalControlProfileOp::Save => {
                let expected =
                    mock_thermal_runtime(status.target_temp_c, profile.profile.as_ref(), false);
                if status.thermal_control_profile_preview
                    || status.thermal_control.profile_source != "saved"
                    || status.thermal_control != expected
                {
                    return false;
                }
            }
            ThermalControlProfileOp::ClearSaved => {
                if status.thermal_control.profile_source == "saved" {
                    return false;
                }
            }
        }
    }
    true
}

pub(crate) fn decode_usb_response_line(
    line: &[u8],
    request_id: &str,
) -> Result<Option<Value>, HttpError> {
    const FRAME_PREFIX: &[u8] = br#"{"type":"#;
    for (offset, candidate) in line.windows(FRAME_PREFIX.len()).enumerate() {
        if candidate != FRAME_PREFIX {
            continue;
        }
        let mut frames =
            serde_json::Deserializer::from_slice(&line[offset..]).into_iter::<UsbResponseWire>();
        let Some(Ok(frame)) = frames.next() else {
            continue;
        };
        if let Some(payload) = decode_usb_response_frame(frame, request_id)? {
            return Ok(Some(payload));
        }
    }
    Ok(None)
}

pub(crate) fn decode_usb_response_frame(
    frame: UsbResponseWire,
    request_id: &str,
) -> Result<Option<Value>, HttpError> {
    if frame.frame_type == "error" && frame.request_id.as_deref() == Some(request_id) {
        return Err(HttpError {
            status: StatusCode::BAD_GATEWAY,
            error: frame.error.unwrap_or_else(|| ApiError {
                code: "usb_error".to_string(),
                message: "Firmware returned an unsuccessful USB error frame.".to_string(),
                retryable: true,
                details: None,
            }),
        });
    }
    if frame.frame_type != "response" || frame.request_id.as_deref() != Some(request_id) {
        return Ok(None);
    }
    if frame.ok == Some(true) {
        return Ok(Some(frame.result.unwrap_or(Value::Null)));
    }

    Err(HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: frame.error.unwrap_or_else(|| ApiError {
            code: "usb_error".to_string(),
            message: "Firmware returned an unsuccessful USB response.".to_string(),
            retryable: true,
            details: None,
        }),
    })
}

pub(crate) fn serial_io_http_error(error: io::Error) -> HttpError {
    HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        &format!("Serial I/O failed: {error}"),
        true,
    )
}

pub(crate) fn is_recoverable_write_http_error(error: &HttpError) -> bool {
    error.error.code == "serial_io_failed"
        && error.error.retryable
        && error
            .error
            .message
            .strip_prefix("Serial I/O failed: ")
            .map(is_recoverable_serial_error_message)
            .unwrap_or(false)
}
