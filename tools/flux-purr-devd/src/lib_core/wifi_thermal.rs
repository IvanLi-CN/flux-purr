pub(crate) use super::*;

pub(crate) fn devd_event_to_sse(event: DevdEvent) -> Event {
    let kind = event.kind.clone();
    let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
    Event::default().event(kind).data(data)
}

pub(crate) async fn configure_wifi(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<WifiConfigRequest>,
) -> Result<Json<Value>, HttpError> {
    if let Some(Some(static_ipv4)) = payload.static_ipv4
        && !static_ipv4_request_is_valid(static_ipv4)
    {
        return Err(HttpError::bad_request(
            "invalid_static_ipv4",
            "staticIpv4 requires a unicast address and a prefix length from 0 through 32.",
        ));
    }
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    if target.transport == DeviceTransport::NativeSerial {
        let network = match serial_wifi_config(&state, &target, &payload).await {
            Ok(network) => network,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "wifi_config", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            if network.is_not_older_than(&device.network) {
                device.network = network.clone();
                device.status.network = network.clone();
            }
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_wifi_config_event(&state, &device_id, &payload);
        return Ok(Json(json!({
            "accepted": true,
            "network": network,
            "wifi": {
                "op": payload.op,
                "ssid": payload.ssid,
                "password": payload.password.as_ref().map(|_| "<redacted>"),
                "telemetryIntervalMs": payload.telemetry_interval_ms
            }
        })));
    }

    if target.transport == DeviceTransport::Lan {
        return Err(HttpError::bad_request(
            "lan_wifi_config_unsupported",
            "WiFi configuration is available only through an active USB/devd target.",
        ));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    device.network = mock_network_after_wifi_config(&device.network, &payload);
    device.status.network = device.network.clone();
    let redacted = json!({
        "accepted": true,
        "network": device.network,
        "wifi": {
            "op": payload.op,
            "ssid": payload.ssid,
            "password": payload.password.as_ref().map(|_| "<redacted>"),
            "telemetryIntervalMs": payload.telemetry_interval_ms
        }
    });
    drop(state_lock);
    emit_wifi_config_event(&state, &device_id, &payload);
    Ok(Json(redacted))
}

pub(crate) fn mock_network_after_wifi_config(
    current: &NetworkSummary,
    payload: &WifiConfigRequest,
) -> NetworkSummary {
    match payload.op {
        WifiConfigOp::Clear => NetworkSummary {
            state: NetworkState::Disabled,
            configuration_generation: current.configuration_generation.wrapping_add(1),
            transition_sequence: current.transition_sequence.wrapping_add(1),
            failure_code: None,
            ssid: None,
            wifi_password_length: 0,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: None,
        },
        WifiConfigOp::Set => {
            let mut network = current.clone();
            // The device receipt exposes only the public connection phase.
            // Persistence/disconnect work is not a host-visible WiFi state.
            network.state = NetworkState::Connecting;
            network.configuration_generation = network.configuration_generation.wrapping_add(1);
            network.transition_sequence = network.transition_sequence.wrapping_add(1);
            network.failure_code = None;
            network.ssid = payload.ssid.clone();
            if let Some(password) = &payload.password {
                network.wifi_password_length = password.len() as u8;
            }
            network.ip = None;
            network.gateway = None;
            network.dns.clear();
            network.wifi_rssi = None;
            network.last_error = None;
            network
        }
        WifiConfigOp::Cancel => {
            let mut network = current.clone();
            // Cancel is a runtime station operation. Stored credentials and
            // their configuration generation remain unchanged.
            network.state = NetworkState::Idle;
            network.transition_sequence = network.transition_sequence.wrapping_add(1);
            network.failure_code = None;
            network.ip = None;
            network.gateway = None;
            network.dns.clear();
            network.wifi_rssi = None;
            network.last_error = None;
            network
        }
    }
}

pub(crate) fn static_ipv4_request_is_valid(value: WifiStaticIpv4Request) -> bool {
    value.prefix_len <= 32
        && is_unicast_static_ipv4(value.address)
        && is_unicast_static_ipv4(value.gateway)
        && is_unicast_static_ipv4(value.dns)
}

pub(crate) fn is_unicast_static_ipv4(address: [u8; 4]) -> bool {
    let first = address[0];
    first != 0 && first != 127 && first < 224
}

pub(crate) async fn configure_runtime(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<RuntimeConfigRequest>,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    validate_runtime_config(&payload)?;
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    match target.transport {
        DeviceTransport::NativeSerial => {
            configure_native_runtime(&state, &device_id, &target, &payload).await
        }
        DeviceTransport::Lan => configure_lan_runtime(&state, &device_id, &target, &payload).await,
        DeviceTransport::Mock => configure_mock_runtime(&state, &device_id, &target, &payload),
    }
}

pub(crate) async fn configure_native_runtime(
    state: &AppState,
    device_id: &str,
    target: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    let status = match serial_runtime_config(state, target, payload).await {
        Ok(status) => status,
        Err(error) => {
            record_serial_bridge_error(state, device_id, "runtime_config", &error);
            return Err(error);
        }
    };
    let mut state_lock = state.lock()?;
    if let Some(device) = state_lock.devices.get_mut(device_id) {
        device.status = status.clone();
        device.network = status.network.clone();
        device.connection = ConnectionState::Connected;
    }
    drop(state_lock);
    emit_runtime_config_event(state, device_id, payload, &status);
    Ok(Json(status))
}

pub(crate) async fn configure_lan_runtime(
    state: &AppState,
    device_id: &str,
    target: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    let configured = lan_bridge_config(target)?;
    let status = lan_bridge_write::<ControlPlaneStatus>(
        &configured,
        "runtime",
        Method::PUT,
        Some(lan_bridge_payload(payload)?),
    )
    .await?;
    let mut state_lock = state.lock()?;
    if let Some(device) = state_lock.devices.get_mut(device_id) {
        device.status = status.clone();
        device.network = status.network.clone();
        device.connection = ConnectionState::Connected;
    }
    drop(state_lock);
    emit_runtime_config_event(state, device_id, payload, &status);
    Ok(Json(status))
}

pub(crate) fn configure_mock_runtime(
    state: &AppState,
    device_id: &str,
    target: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    validate_manual_pps_request_against_status(payload, &target.status)?;
    if let Some(calibration) = payload.calibration.as_ref() {
        validate_calibration_request_against_status(
            calibration,
            &target.status,
            &target.status.calibration,
        )?;
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    reject_mock_runtime_override_while_busy(device, payload)?;
    apply_mock_runtime_fields(device, payload);
    let status = device.status.clone();
    drop(state_lock);
    emit_runtime_config_event(state, device_id, payload, &status);
    Ok(Json(status))
}

pub(crate) fn reject_mock_runtime_override_while_busy(
    device: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<(), HttpError> {
    if !mock_thermal_plant_job_running(&device.status) {
        return Ok(());
    }
    let manual_pps_requested = payload.manual_pps_enabled.is_some()
        || payload.manual_pps_mv.is_some()
        || payload.manual_pps_ma.is_some();
    if manual_pps_requested || payload.calibration.is_some() || payload.heater_enabled == Some(true)
    {
        return Err(HttpError::bad_request(
            "manual_pps_calibration_busy",
            "Manual PPS and heater controls cannot override a running thermal-model calibration.",
        ));
    }
    Ok(())
}

pub(crate) fn apply_mock_runtime_fields(device: &mut DeviceRecord, payload: &RuntimeConfigRequest) {
    if let Some(target_temp_c) = payload.target_temp_c {
        device.status.target_temp_c = target_temp_c;
    }
    if let Some(selected_preset_slot) = payload.selected_preset_slot {
        device.status.selected_preset_slot = Some(selected_preset_slot);
    }
    if let Some(presets_c) = &payload.presets_c {
        device.status.presets_c = Some(presets_c.clone());
        if payload.target_temp_c.is_none()
            && let Some(selected_preset_slot) = device.status.selected_preset_slot
            && let Some(Some(target_temp_c)) = presets_c.get(selected_preset_slot)
        {
            device.status.target_temp_c = *target_temp_c;
        }
    }
    if let Some(active_cooling_enabled) = payload.active_cooling_enabled {
        device.status.active_cooling_enabled = active_cooling_enabled;
    }
    if let Some(mode) = payload.post_heat_cooling_mode.as_deref() {
        device.status.post_heat_cooling_mode = mode.to_string();
        device.status.active_cooling_enabled = mode != "off";
    }
    if let Some(mode) = payload.heating_fan_guard_mode.as_deref() {
        device.status.heating_fan_guard_mode = mode.to_string();
    }
    if let Some(heater_enabled) = payload.heater_enabled {
        device.status.heater_enabled = heater_enabled;
        if !heater_enabled {
            device.status.heater_output_percent = 0;
        }
    }
    if payload.manual_pps_enabled == Some(false) {
        device.status.manual_pps_enabled = false;
        device.status.manual_pps_mv = None;
        device.status.manual_pps_ma = None;
        device.status.pd_request_mv = DEFAULT_PD_REQUEST_MV;
        device.status.pd_contract_mv = DEFAULT_PD_REQUEST_MV;
        device.status.voltage_mv = u32::from(DEFAULT_PD_REQUEST_MV);
        device.status.manual_pps_error = None;
    } else if payload.manual_pps_enabled == Some(true)
        || payload.manual_pps_mv.is_some()
        || payload.manual_pps_ma.is_some()
    {
        let manual_pps_mv = payload
            .manual_pps_mv
            .or(device.status.manual_pps_mv)
            .expect("manual PPS voltage validated");
        let manual_pps_ma = payload
            .manual_pps_ma
            .or(device.status.manual_pps_ma)
            .or(effective_pps_current_capability_ma(&device.status))
            .expect("manual PPS current validated");
        device.status.manual_pps_enabled = true;
        device.status.manual_pps_mv = Some(manual_pps_mv);
        device.status.manual_pps_ma = Some(manual_pps_ma);
        device.status.pd_request_mv = manual_pps_mv;
        device.status.pd_contract_mv = manual_pps_mv;
        device.status.voltage_mv = u32::from(manual_pps_mv);
        device.status.manual_pps_error = None;
    }
    if let Some(calibration) = payload.calibration.as_ref() {
        apply_mock_calibration_runtime_config(&mut device.status, calibration);
    }
    if payload.fault_attention_acknowledged == Some(true) {
        device.status.fault_attention_pending = false;
    }
    if let Some(mode) = payload.thermal_profile_mode.as_deref() {
        device.status.thermal_profile_mode = mode.to_string();
        device.status.thermal_profile_resolved_bank = if mode == "100w"
            || (mode == "auto"
                && device.status.pps_capability_min_mv.unwrap_or(u16::MAX) <= 20_000
                && device.status.pps_capability_max_mv.unwrap_or(0) >= 20_000
                && device.status.pps_capability_max_ma.unwrap_or(0) >= 5_000)
        {
            "pps5a".to_string()
        } else {
            "pps3a".to_string()
        };
    }
    apply_mock_thermal_profile(device, payload.thermal_control_profile.as_ref());
    let active_profile = device.preview_thermal_control_profile.as_ref().or({
        match device.status.thermal_profile_resolved_bank.as_str() {
            "pps5a" => device.saved_thermal_control_profile_pps5a.as_ref(),
            _ => device.saved_thermal_control_profile.as_ref(),
        }
    });
    let preview_active = device.preview_thermal_control_profile.is_some();
    device.status.thermal_control_profile_preview = preview_active;
    device.status.thermal_control =
        mock_thermal_runtime(device.status.target_temp_c, active_profile, preview_active);
}

pub(crate) fn apply_mock_thermal_profile(
    device: &mut DeviceRecord,
    thermal_control_profile: Option<&ThermalControlProfileRequest>,
) {
    let Some(profile) = thermal_control_profile else {
        return;
    };
    let bank = profile
        .bank
        .as_deref()
        .unwrap_or(&device.status.thermal_profile_resolved_bank)
        .to_string();
    match profile.op {
        ThermalControlProfileOp::Preview => {
            device.preview_thermal_control_profile = profile.profile.clone();
        }
        ThermalControlProfileOp::ClearPreview => {
            device.preview_thermal_control_profile = None;
        }
        ThermalControlProfileOp::Save => {
            if bank == "pps5a" {
                device.saved_thermal_control_profile_pps5a = profile.profile.clone();
            } else {
                device.saved_thermal_control_profile = profile.profile.clone();
            }
            device.preview_thermal_control_profile = None;
        }
        ThermalControlProfileOp::ClearSaved => {
            if bank == "pps5a" {
                device.saved_thermal_control_profile_pps5a = None;
            } else {
                device.saved_thermal_control_profile = None;
            }
        }
    }
}

pub(crate) async fn configure_buzzer_test(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<BuzzerTestRequest>,
) -> Result<Json<BuzzerTestStatus>, HttpError> {
    validate_buzzer_test_request(&payload)?;
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
            "Buzzer test is available only through a native USB serial lease.",
        ));
    }
    if !target
        .identity
        .capabilities
        .iter()
        .any(|capability| capability == "buzzer_test")
    {
        return Err(HttpError::bad_request(
            "buzzer_test_unavailable",
            "The connected firmware does not declare the buzzer_test capability.",
        ));
    }

    let status = match serial_buzzer_test(&state, &target, &payload).await {
        Ok(status) => status,
        Err(error) => {
            record_serial_bridge_error(&state, &device_id, "buzzer_test", &error);
            return Err(error);
        }
    };
    state.emit(event(
        &device_id,
        "buzzer_test",
        "buzzer test command completed",
        json!({
            "op": payload.op,
            "cue": payload.cue,
            "scenario": payload.scenario,
            "state": status.state,
            "traceLength": status.trace.len(),
        }),
    ));
    Ok(Json(status))
}

pub(crate) fn apply_mock_calibration_runtime_config(
    status: &mut ControlPlaneStatus,
    calibration: &CalibrationControlRequest,
) {
    let current_ma = effective_pps_current_capability_ma(status);
    if let Some(mode) = calibration.mode {
        status.calibration.mode = mode;
        if mode == CalibrationMode::Off {
            status.calibration = CalibrationRuntimeState::default();
        }
    }

    if let Some(target_adc_mv) = calibration.target_adc_mv {
        status.calibration.target_adc_mv = Some(target_adc_mv);
    }

    if let Some(heater_enabled) = calibration.heater_enabled {
        status.calibration.heater_enabled = heater_enabled;
        status.heater_enabled = heater_enabled;
        if !heater_enabled {
            status.heater_output_percent = 0;
        }
    }

    if calibration.pps_enabled == Some(false) {
        status.calibration.pps_enabled = false;
        status.calibration.pps_mv = None;
        status.calibration.pps_ma = None;
        return;
    }

    if calibration.pps_enabled == Some(true) || calibration.pps_mv.is_some() {
        let pps_mv = calibration
            .pps_mv
            .or(status.calibration.pps_mv)
            .or(status.manual_pps_mv)
            .unwrap_or(status.pd_contract_mv);
        let pps_ma = current_ma.or(status.manual_pps_ma);
        status.calibration.pps_enabled = true;
        status.calibration.pps_mv = Some(pps_mv);
        status.calibration.pps_ma = pps_ma;
        status.manual_pps_enabled = true;
        status.manual_pps_mv = Some(pps_mv);
        status.manual_pps_ma = pps_ma;
        status.pd_request_mv = pps_mv;
        status.pd_contract_mv = pps_mv;
        status.voltage_mv = u32::from(pps_mv);
        status.manual_pps_error = None;
        status.calibration.error = None;
    }

    let observed_mv = match status.calibration.mode {
        CalibrationMode::RtdAdc => status.rtd_raw_adc_mv,
        CalibrationMode::VinAdc => status.vin_raw_adc_mv,
        CalibrationMode::Off | CalibrationMode::HeaterCurve | CalibrationMode::ThermalPlant => None,
    };

    status.calibration.stability_error_mv = status
        .calibration
        .target_adc_mv
        .zip(observed_mv)
        .map(|(target, observed)| (i32::from(observed) - i32::from(target)) as i16);
    status.calibration.stable = status
        .calibration
        .stability_error_mv
        .is_some_and(|error_mv| error_mv.abs() <= 8);
}

pub(crate) fn validate_runtime_config(payload: &RuntimeConfigRequest) -> Result<(), HttpError> {
    if payload
        .post_heat_cooling_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "off" | "normal" | "fast"))
    {
        return Err(HttpError::bad_request(
            "invalid_post_heat_cooling_mode",
            "postHeatCoolingMode must be off, normal, or fast.",
        ));
    }
    if payload
        .heating_fan_guard_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "off" | "low" | "medium" | "high"))
    {
        return Err(HttpError::bad_request(
            "invalid_heating_fan_guard_mode",
            "heatingFanGuardMode must be off, low, medium, or high.",
        ));
    }
    if let (Some(active_cooling_enabled), Some(mode)) = (
        payload.active_cooling_enabled,
        payload.post_heat_cooling_mode.as_deref(),
    ) && active_cooling_enabled != (mode != "off")
    {
        return Err(HttpError::bad_request(
            "fan_policy_conflict",
            "activeCoolingEnabled conflicts with postHeatCoolingMode.",
        ));
    }
    if payload
        .thermal_profile_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "auto" | "65w" | "100w"))
    {
        return Err(HttpError::bad_request(
            "invalid_thermal_profile_mode",
            "thermalProfileMode must be auto, 65w, or 100w.",
        ));
    }
    if payload
        .selected_preset_slot
        .is_some_and(|slot| slot >= FRONT_PANEL_PRESET_COUNT)
    {
        return Err(HttpError::bad_request(
            "invalid_preset_slot",
            "selectedPresetSlot must be between 0 and 9.",
        ));
    }
    if payload
        .presets_c
        .as_ref()
        .is_some_and(|presets| presets.len() != FRONT_PANEL_PRESET_COUNT)
    {
        return Err(HttpError::bad_request(
            "invalid_presets",
            "presetsC must contain exactly 10 values.",
        ));
    }
    if payload.manual_pps_mv.is_some_and(|millivolts| {
        !millivolts.is_multiple_of(100)
            || !(PPS_HARDWARE_MIN_MV..=PPS_HARDWARE_MAX_MV).contains(&millivolts)
    }) {
        return Err(HttpError::bad_request(
            "invalid_manual_pps",
            "manualPpsMv must use 100mV steps and stay within 5000..28000.",
        ));
    }
    if payload
        .manual_pps_ma
        .is_some_and(|milliamps| !milliamps.is_multiple_of(50) || milliamps == 0)
    {
        return Err(HttpError::bad_request(
            "invalid_manual_pps",
            "manualPpsMa must use 50mA steps and be greater than 0.",
        ));
    }
    if let Some(calibration) = payload.calibration.as_ref() {
        validate_calibration_control_request(calibration)?;
    }
    if let Some(thermal_control_profile) = payload.thermal_control_profile.as_ref() {
        validate_thermal_control_profile_request(thermal_control_profile)?;
    }
    Ok(())
}

pub(crate) fn validate_buzzer_test_request(payload: &BuzzerTestRequest) -> Result<(), HttpError> {
    let valid = match payload.op {
        BuzzerTestOp::Trigger => payload.cue.is_some() && payload.scenario.is_none(),
        BuzzerTestOp::Run => payload.cue.is_none() && payload.scenario.is_some(),
        BuzzerTestOp::Stop | BuzzerTestOp::Status => {
            payload.cue.is_none() && payload.scenario.is_none() && !payload.repeat
        }
    };
    if valid {
        Ok(())
    } else {
        Err(HttpError::bad_request(
            "invalid_buzzer_test_command",
            "buzzer test requires exactly the fields for its operation.",
        ))
    }
}

pub(crate) fn validate_thermal_control_profile_request(
    request: &ThermalControlProfileRequest,
) -> Result<(), HttpError> {
    if request
        .bank
        .as_deref()
        .is_some_and(|bank| !matches!(bank, "pps3a" | "pps5a"))
    {
        return Err(HttpError::bad_request(
            "invalid_thermal_profile_bank",
            "thermalControlProfile.bank must be pps3a or pps5a.",
        ));
    }
    match request.op {
        ThermalControlProfileOp::Preview | ThermalControlProfileOp::Save => {
            let profile = request.profile.as_ref().ok_or_else(|| {
                HttpError::bad_request(
                    "thermal_profile_required",
                    "thermalControlProfile.profile is required for preview/save.",
                )
            })?;
            if profile.points.len() != FRONT_PANEL_PRESET_COUNT {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermalControlProfile.profile.points must contain exactly 10 values.",
                ));
            }
            if let Some(settings) = profile.settings.as_ref()
                && (settings.temp_filter_alpha_permille == 0
                    || settings.temp_filter_alpha_permille > 1_000
                    || !(AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN..=PPS_HARDWARE_MAX_MV)
                        .contains(&settings.auto_adjustable_working_floor_mv)
                    || settings.heater_current_reserve_ma > 1_000
                    || settings.approach_min_power_ratio_permille > 1_000
                    || !(1..=255).contains(&settings.approach_max_ticks))
            {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermal profile settings must use 1..1000 alpha, 5000..28000 auto adjustable floor, 0..1000mA heater current reserve, 0..1000 approach-min ratio, and 1..255 approach max ticks.",
                ));
            }
            for point in profile.points.iter().flatten() {
                if point.brake_distance_centi_c == 0
                    || point.warmup_power_permille > 1_000
                    || point.approach_power_permille > 1_000
                    || point.approach_floor_power_permille > 1_000
                    || !(100..=4_000).contains(&point.approach_damping_exponent_permille)
                    || point.hold_power_permille > 1_000
                    || point.hold_reheat_power_permille > 1_000
                    || point.warmup_reenter_centi_c > 5_000
                    || point.hold_entry_centi_c > 5_000
                    || point.hold_exit_centi_c > 5_000
                    || point.hold_on_centi_c > 5_000
                    || point.hold_off_centi_c > 5_000
                    || point.overshoot_cutoff_centi_c > 5_000
                    || point.hold_kp_permille_per_c > 10_000
                    || point.hold_ki_permille_per_c_tick > 10_000
                    || point.hold_blend_ticks > 255
                    || point.approach_lead_ticks > 255
                    || point.hold_lead_ticks > 255
                {
                    return Err(HttpError::bad_request(
                        "invalid_thermal_profile",
                        "thermal profile points must use positive brake distance, 0..1000 permille power, 100..4000 approach damping, <=5000 centi-C warmup/damping thresholds, <=10000 PI gains, and <=255 blend/lead ticks.",
                    ));
                }
            }
        }
        ThermalControlProfileOp::ClearPreview | ThermalControlProfileOp::ClearSaved => {
            if request.profile.is_some() {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermalControlProfile.profile must be omitted for clear operations.",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) const fn default_hold_blend_ticks() -> u16 {
    12
}

pub(crate) const fn default_approach_damping_exponent_permille() -> u16 {
    1_000
}

pub(crate) const fn default_auto_adjustable_working_floor_mv() -> u16 {
    AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT
}

pub(crate) const fn default_heater_current_reserve_ma() -> u16 {
    200
}

pub(crate) fn mock_thermal_default_settings() -> MockThermalCandidateSettings {
    MockThermalCandidateSettings {
        temp_filter_alpha_permille: 750,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 20,
        hold_exit_centi_c: 80,
        hold_on_centi_c: 15,
        hold_off_centi_c: 80,
        overshoot_cutoff_centi_c: 120,
        approach_max_ticks: 250,
        approach_min_power_ratio_permille: 500,
        hold_kp_permille_per_c: 35,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 12,
        hold_reheat_power_permille: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        auto_adjustable_working_floor_mv: AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT,
        heater_current_reserve_ma: 200,
    }
}

pub(crate) type MockThermalTargetValues = (
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
);

pub(crate) fn mock_thermal_default_target_values(target_temp_c: i16) -> MockThermalTargetValues {
    if target_temp_c <= 60 {
        (
            1_310, 1_000, 590, 510, 1_320, 60, 60, 200, 540, 30, 120, 150, 8, 2, 1, 4, 2,
        )
    } else if target_temp_c <= 100 {
        (
            1_100, 1_000, 420, 220, 1_400, 170, 260, 12, 60, 30, 180, 230, 55, 2, 6, 9, 0,
        )
    } else if target_temp_c <= 140 {
        (
            1_000, 1_000, 420, 200, 1_000, 280, 340, 10, 55, 30, 160, 220, 22, 1, 1, 4, 0,
        )
    } else if target_temp_c <= 180 {
        (
            650, 1_000, 760, 460, 800, 450, 620, 15, 70, 25, 240, 300, 20, 1, 3, 4, 0,
        )
    } else if target_temp_c <= 220 {
        (
            520, 1_000, 760, 600, 550, 620, 700, 8, 50, 14, 240, 320, 22, 1, 2, 2, 0,
        )
    } else {
        (
            500, 1_000, 960, 860, 350, 850, 930, 10, 55, 14, 320, 420, 12, 1, 1, 1, 0,
        )
    }
}

pub(crate) fn mock_thermal_default_target_point(target_temp_c: i16) -> MockThermalCandidatePoint {
    let (
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = mock_thermal_default_target_values(target_temp_c);
    MockThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c: 0,
        hold_power_permille,
        hold_reheat_power_permille,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    }
}

pub(crate) fn mock_thermal_profile_from_package(
    package: &ThermalControlProfilePackage,
) -> MockThermalCandidateProfile {
    let settings = mock_thermal_settings_from_package(package.settings);
    let points = thermal_profile_targets(package)
        .into_iter()
        .map(|target_temp_c| mock_thermal_point_from_package(package, target_temp_c))
        .collect();
    MockThermalCandidateProfile { settings, points }
}

pub(crate) fn mock_thermal_settings_from_package(
    settings: Option<ThermalControlProfileSettings>,
) -> MockThermalCandidateSettings {
    settings
        .map(|settings| MockThermalCandidateSettings {
            temp_filter_alpha_permille: settings.temp_filter_alpha_permille,
            warmup_reenter_centi_c: settings.warmup_reenter_centi_c,
            hold_entry_centi_c: settings.hold_entry_centi_c,
            hold_exit_centi_c: settings.hold_exit_centi_c,
            hold_on_centi_c: settings.hold_on_centi_c,
            hold_off_centi_c: settings.hold_off_centi_c,
            overshoot_cutoff_centi_c: settings.overshoot_cutoff_centi_c,
            approach_max_ticks: settings.approach_max_ticks,
            approach_min_power_ratio_permille: settings.approach_min_power_ratio_permille,
            hold_kp_permille_per_c: settings.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: settings.hold_ki_permille_per_c_tick,
            hold_blend_ticks: settings.hold_blend_ticks,
            hold_reheat_power_permille: settings.hold_reheat_power_permille,
            approach_lead_ticks: settings.approach_lead_ticks,
            hold_lead_ticks: settings.hold_lead_ticks,
            auto_adjustable_working_floor_mv: settings.auto_adjustable_working_floor_mv,
            heater_current_reserve_ma: settings.heater_current_reserve_ma,
        })
        .unwrap_or_else(mock_thermal_default_settings)
}

pub(crate) fn thermal_profile_targets(package: &ThermalControlProfilePackage) -> Vec<i16> {
    let explicit = package
        .points
        .iter()
        .flatten()
        .map(|point| point.target_temp_c)
        .collect::<Vec<_>>();
    if explicit.is_empty() {
        THERMAL_PROFILE_ANCHOR_TARGETS_C.to_vec()
    } else {
        explicit
    }
}

pub(crate) fn mock_thermal_point_from_package(
    package: &ThermalControlProfilePackage,
    target_temp_c: i16,
) -> MockThermalCandidatePoint {
    let default_point = mock_thermal_default_target_point(target_temp_c);
    let point = package
        .points
        .iter()
        .flatten()
        .find(|point| point.target_temp_c == target_temp_c);
    MockThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: point
            .map(|point| point.brake_distance_centi_c)
            .unwrap_or(default_point.brake_distance_centi_c),
        warmup_power_permille: point
            .map(|point| point.warmup_power_permille)
            .unwrap_or(default_point.warmup_power_permille),
        approach_power_permille: point
            .map(|point| point.approach_power_permille)
            .unwrap_or(default_point.approach_power_permille),
        approach_floor_power_permille: point
            .map(|point| point.approach_floor_power_permille)
            .unwrap_or(default_point.approach_floor_power_permille),
        approach_damping_exponent_permille: point
            .map(|point| point.approach_damping_exponent_permille)
            .unwrap_or(default_point.approach_damping_exponent_permille),
        approach_tail_window_centi_c: point
            .map(|point| point.approach_tail_window_centi_c)
            .unwrap_or(default_point.approach_tail_window_centi_c),
        hold_power_permille: point
            .map(|point| point.hold_power_permille)
            .unwrap_or(default_point.hold_power_permille),
        hold_reheat_power_permille: point
            .map(|point| point.hold_reheat_power_permille)
            .unwrap_or(default_point.hold_reheat_power_permille),
        warmup_reenter_centi_c: point
            .map(|point| point.warmup_reenter_centi_c)
            .unwrap_or(default_point.warmup_reenter_centi_c),
        hold_entry_centi_c: point
            .map(|point| point.hold_entry_centi_c)
            .unwrap_or(default_point.hold_entry_centi_c),
        hold_exit_centi_c: point
            .map(|point| point.hold_exit_centi_c)
            .unwrap_or(default_point.hold_exit_centi_c),
        hold_on_centi_c: point
            .map(|point| point.hold_on_centi_c)
            .unwrap_or(default_point.hold_on_centi_c),
        hold_off_centi_c: point
            .map(|point| point.hold_off_centi_c)
            .unwrap_or(default_point.hold_off_centi_c),
        overshoot_cutoff_centi_c: point
            .map(|point| point.overshoot_cutoff_centi_c)
            .unwrap_or(default_point.overshoot_cutoff_centi_c),
        hold_kp_permille_per_c: point
            .map(|point| point.hold_kp_permille_per_c)
            .unwrap_or(default_point.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: point
            .map(|point| point.hold_ki_permille_per_c_tick)
            .unwrap_or(default_point.hold_ki_permille_per_c_tick),
        hold_blend_ticks: point
            .map(|point| point.hold_blend_ticks)
            .unwrap_or(default_point.hold_blend_ticks),
        approach_lead_ticks: point
            .map(|point| point.approach_lead_ticks)
            .unwrap_or(default_point.approach_lead_ticks),
        hold_lead_ticks: point
            .map(|point| point.hold_lead_ticks)
            .unwrap_or(default_point.hold_lead_ticks),
    }
}

pub(crate) fn mock_thermal_candidate_point(
    profile: &MockThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<MockThermalCandidatePoint> {
    profile
        .points
        .iter()
        .copied()
        .find(|point| point.target_temp_c == target_temp_c)
}

pub(crate) fn mock_thermal_interpolated_candidate_point(
    profile: &MockThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<MockThermalCandidatePoint> {
    if let Some(point) = mock_thermal_candidate_point(profile, target_temp_c) {
        return Some(point);
    }
    let (lower, upper) = interpolation_bounds(profile, target_temp_c)?;
    let ratio = f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c);
    Some(interpolate_thermal_candidate_point(
        lower,
        upper,
        target_temp_c,
        ratio,
    ))
}

pub(crate) fn interpolation_bounds(
    profile: &MockThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<(MockThermalCandidatePoint, MockThermalCandidatePoint)> {
    let mut points = profile.points.clone();
    points.sort_by_key(|point| point.target_temp_c);
    let lower = points
        .iter()
        .copied()
        .rev()
        .find(|point| point.target_temp_c < target_temp_c)?;
    let upper = points
        .iter()
        .copied()
        .find(|point| point.target_temp_c > target_temp_c)?;
    Some((lower, upper))
}

pub(crate) fn interpolate_thermal_candidate_point(
    lower: MockThermalCandidatePoint,
    upper: MockThermalCandidatePoint,
    target_temp_c: i16,
    ratio: f32,
) -> MockThermalCandidatePoint {
    let values = interpolated_thermal_point_values(lower, upper, ratio);
    MockThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: values.brake_distance_centi_c,
        warmup_power_permille: values.warmup_power_permille,
        approach_power_permille: values.approach_power_permille,
        approach_floor_power_permille: values.approach_floor_power_permille,
        approach_damping_exponent_permille: values.approach_damping_exponent_permille,
        approach_tail_window_centi_c: values.approach_tail_window_centi_c,
        hold_power_permille: values.hold_power_permille,
        hold_reheat_power_permille: values.hold_reheat_power_permille,
        warmup_reenter_centi_c: values.warmup_reenter_centi_c,
        hold_entry_centi_c: values.hold_entry_centi_c,
        hold_exit_centi_c: values.hold_exit_centi_c,
        hold_on_centi_c: values.hold_on_centi_c,
        hold_off_centi_c: values.hold_off_centi_c,
        overshoot_cutoff_centi_c: values.overshoot_cutoff_centi_c,
        hold_kp_permille_per_c: values.hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick: values.hold_ki_permille_per_c_tick,
        hold_blend_ticks: values.hold_blend_ticks,
        approach_lead_ticks: values.approach_lead_ticks,
        hold_lead_ticks: values.hold_lead_ticks,
    }
}

pub(crate) fn interpolated_thermal_point_values(
    lower: MockThermalCandidatePoint,
    upper: MockThermalCandidatePoint,
    ratio: f32,
) -> MockThermalRuntimePointValues {
    let lerp = |left: u16, right: u16, upper_bound: u16| {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = thermal_brake_adjustment(lower, upper);
    let interpolated_brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_hold_scale = thermal_low_temp_hold_scale(lower, upper, midpoint_weight);
    let low_temp_reheat_scale = thermal_low_temp_reheat_scale(lower, upper, midpoint_weight);
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    MockThermalRuntimePointValues {
        brake_distance_centi_c: interpolated_brake_distance,
        warmup_power_permille: lerp(
            lower.warmup_power_permille,
            upper.warmup_power_permille,
            1_000,
        ),
        approach_power_permille: lerp(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent_permille: lerp(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            THERMAL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        ),
        approach_tail_window_centi_c: lerp(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            THERMAL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
        ),
        hold_power_permille: scale_low_temp_hold(lerp(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        warmup_reenter_centi_c: lerp(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            5_000,
        ),
        hold_entry_centi_c: lerp(lower.hold_entry_centi_c, upper.hold_entry_centi_c, 5_000),
        hold_exit_centi_c: lerp(lower.hold_exit_centi_c, upper.hold_exit_centi_c, 5_000),
        hold_on_centi_c: lerp(lower.hold_on_centi_c, upper.hold_on_centi_c, 5_000),
        hold_off_centi_c: lerp(lower.hold_off_centi_c, upper.hold_off_centi_c, 5_000),
        overshoot_cutoff_centi_c: lerp(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        ),
        hold_kp_permille_per_c: lerp(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        ),
        hold_ki_permille_per_c_tick: lerp(
            lower.hold_ki_permille_per_c_tick,
            upper.hold_ki_permille_per_c_tick,
            10_000,
        )
        .max(1),
        hold_blend_ticks: lerp(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)),
        approach_lead_ticks: lerp(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        ),
        hold_lead_ticks: lerp(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        ),
    }
}

pub(crate) fn thermal_brake_adjustment(
    lower: MockThermalCandidatePoint,
    upper: MockThermalCandidatePoint,
) -> f32 {
    if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 {
            0.55
        } else {
            0.20
        }
    } else {
        0.0
    }
}

pub(crate) fn thermal_low_temp_hold_scale(
    lower: MockThermalCandidatePoint,
    upper: MockThermalCandidatePoint,
    midpoint_weight: f32,
) -> f32 {
    if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    }
}

pub(crate) fn thermal_low_temp_reheat_scale(
    lower: MockThermalCandidatePoint,
    upper: MockThermalCandidatePoint,
    midpoint_weight: f32,
) -> f32 {
    if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    }
}

pub(crate) fn mock_thermal_runtime(
    target_temp_c: i16,
    package: Option<&ThermalControlProfilePackage>,
    preview_active: bool,
) -> ThermalControlRuntime {
    let target_temp_c = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
    let profile = package.map(mock_thermal_profile_from_package);
    let profile_source = thermal_profile_source(preview_active, profile.is_some());
    let profile_covers_target = profile
        .as_ref()
        .and_then(|profile| mock_thermal_interpolated_candidate_point(profile, target_temp_c))
        .is_some();
    let point = profile
        .as_ref()
        .and_then(|profile| mock_thermal_interpolated_candidate_point(profile, target_temp_c));
    let values = thermal_runtime_point_values(point, target_temp_c);
    let settings = profile
        .as_ref()
        .map(|profile| profile.settings)
        .unwrap_or_else(mock_thermal_default_settings);
    ThermalControlRuntime {
        profile_active: profile.is_some(),
        profile_covers_target,
        profile_source: profile_source.to_string(),
        target_temp_c,
        brake_distance_centi_c: values.brake_distance_centi_c,
        warmup_power_permille: values.warmup_power_permille,
        approach_power_permille: values.approach_power_permille,
        approach_floor_power_permille: values.approach_floor_power_permille,
        approach_damping_exponent_permille: values.approach_damping_exponent_permille,
        approach_tail_window_centi_c: values.approach_tail_window_centi_c,
        hold_power_permille: values.hold_power_permille,
        hold_reheat_power_permille: values.hold_reheat_power_permille,
        hold_entry_centi_c: values.hold_entry_centi_c,
        hold_exit_centi_c: values.hold_exit_centi_c,
        hold_on_centi_c: values.hold_on_centi_c,
        hold_off_centi_c: values.hold_off_centi_c,
        overshoot_cutoff_centi_c: values.overshoot_cutoff_centi_c,
        hold_kp_permille_per_c: values.hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick: values.hold_ki_permille_per_c_tick,
        hold_blend_ticks: values.hold_blend_ticks,
        approach_lead_ticks: values.approach_lead_ticks,
        hold_lead_ticks: values.hold_lead_ticks,
        temp_filter_alpha_permille: settings.temp_filter_alpha_permille,
        warmup_reenter_centi_c: values.warmup_reenter_centi_c,
        approach_max_ticks: settings.approach_max_ticks,
        approach_min_power_ratio_permille: settings.approach_min_power_ratio_permille,
        auto_adjustable_working_floor_mv: settings.auto_adjustable_working_floor_mv,
        heater_current_reserve_ma: settings
            .heater_current_reserve_ma
            .min(THERMAL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX),
    }
}

pub(crate) struct MockThermalRuntimePointValues {
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

pub(crate) fn thermal_profile_source(preview_active: bool, profile_active: bool) -> &'static str {
    if preview_active {
        "preview"
    } else if profile_active {
        "saved"
    } else {
        "default"
    }
}

pub(crate) fn thermal_runtime_point_values(
    point: Option<MockThermalCandidatePoint>,
    target_temp_c: i16,
) -> MockThermalRuntimePointValues {
    let point = point.unwrap_or_else(|| mock_thermal_default_target_point(target_temp_c));
    MockThermalRuntimePointValues {
        brake_distance_centi_c: point.brake_distance_centi_c,
        warmup_power_permille: point
            .warmup_power_permille
            .max(point.approach_power_permille),
        approach_power_permille: point.approach_power_permille,
        approach_floor_power_permille: point.approach_floor_power_permille,
        approach_damping_exponent_permille: point.approach_damping_exponent_permille,
        approach_tail_window_centi_c: point.approach_tail_window_centi_c,
        hold_power_permille: point.hold_power_permille,
        hold_reheat_power_permille: point.hold_reheat_power_permille,
        warmup_reenter_centi_c: point.warmup_reenter_centi_c,
        hold_entry_centi_c: point.hold_entry_centi_c,
        hold_exit_centi_c: point.hold_exit_centi_c,
        hold_on_centi_c: point.hold_on_centi_c,
        hold_off_centi_c: point.hold_off_centi_c,
        overshoot_cutoff_centi_c: point.overshoot_cutoff_centi_c,
        hold_kp_permille_per_c: point.hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick: point.hold_ki_permille_per_c_tick,
        hold_blend_ticks: point.hold_blend_ticks,
        approach_lead_ticks: point.approach_lead_ticks,
        hold_lead_ticks: point.hold_lead_ticks,
    }
}

pub(crate) fn validate_calibration_control_request(
    calibration: &CalibrationControlRequest,
) -> Result<(), HttpError> {
    if calibration.pps_mv.is_some_and(|millivolts| {
        !millivolts.is_multiple_of(100)
            || !(PPS_HARDWARE_MIN_MV..=PPS_HARDWARE_MAX_MV).contains(&millivolts)
    }) {
        return Err(HttpError::bad_request(
            "invalid_calibration_pps",
            "calibration.ppsMv must use 100mV steps and stay within 5000..28000.",
        ));
    }
    Ok(())
}

pub(crate) fn apply_mock_calibration_config(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    match payload.op {
        CalibrationConfigOp::Capture => capture_calibration_sample(calibration, payload)?,
        CalibrationConfigOp::Delete => delete_calibration_sample(calibration, payload)?,
        CalibrationConfigOp::Clear => clear_calibration_samples(calibration, payload)?,
        CalibrationConfigOp::Import => import_calibration_state(calibration, payload)?,
        CalibrationConfigOp::SetActiveSlot => set_calibration_active_slot(calibration, payload)?,
        CalibrationConfigOp::SetSlotFit => set_calibration_slot_fit(calibration, payload)?,
    }
    calibration.rtd_adc.sanitize_slot_fits();
    calibration.vin_adc.sanitize_slot_fits();
    calibration.refresh_fits();
    Ok(())
}

pub(crate) fn capture_calibration_sample(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let channel = payload.channel.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_channel_required",
            "Calibration capture requires a channel.",
        )
    })?;
    let observed_mv = payload
        .observed_mv
        .unwrap_or_else(|| mock_observed_adc_mv(channel));
    let expected_mv = expected_calibration_adc_mv(payload, channel).ok_or_else(|| {
        HttpError::bad_request(
            "calibration_reference_required",
            "Calibration capture requires a valid physical reference.",
        )
    })?;
    let samples = &mut calibration.channel_mut(channel).samples;
    let Some(slot) = samples.iter_mut().find(|slot| slot.is_none()) else {
        return Err(HttpError::bad_request(
            "calibration_samples_full",
            "Calibration channel already has 8 samples.",
        ));
    };
    *slot = Some(CalibrationSample {
        observed_mv,
        expected_mv,
        reference_temp_c: payload
            .reference_temp_c
            .filter(|_| channel == CalibrationChannel::RtdAdc),
        target_adc_mv: payload
            .target_adc_mv
            .filter(|_| channel == CalibrationChannel::RtdAdc),
        reference_vin_mv: payload
            .reference_vin_mv
            .and_then(|millivolts| u16::try_from(millivolts).ok())
            .filter(|_| channel == CalibrationChannel::VinAdc),
    });
    Ok(())
}

pub(crate) fn delete_calibration_sample(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let channel = payload.channel.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_channel_required",
            "Calibration delete requires a channel.",
        )
    })?;
    let index = payload.sample_index.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_index_required",
            "Calibration delete requires sampleIndex.",
        )
    })?;
    let samples = &mut calibration.channel_mut(channel).samples;
    let Some(slot) = samples.get_mut(index) else {
        return Err(HttpError::bad_request(
            "calibration_sample_not_found",
            "Calibration sample index was not present.",
        ));
    };
    if slot.is_none() {
        return Err(HttpError::bad_request(
            "calibration_sample_not_found",
            "Calibration sample index was not present.",
        ));
    }
    *slot = None;
    compact_calibration_samples(samples);
    Ok(())
}

pub(crate) fn clear_calibration_samples(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let channel = payload.channel.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_channel_required",
            "Calibration clear requires a channel.",
        )
    })?;
    calibration.channel_mut(channel).samples = vec![None; ADC_CALIBRATION_MAX_SAMPLES];
    Ok(())
}

pub(crate) fn import_calibration_state(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let state = payload.state.clone().ok_or_else(|| {
        HttpError::bad_request(
            "calibration_state_required",
            "Calibration import requires state.",
        )
    })?;
    validate_calibration_state(&state)?;
    *calibration = normalize_calibration_state(state);
    Ok(())
}

pub(crate) fn set_calibration_active_slot(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let channel = payload.channel.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_channel_required",
            "Setting active slot requires a channel.",
        )
    })?;
    let slot = payload.slot.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_slot_required",
            "Setting active slot requires slot.",
        )
    })?;
    calibration.channel_mut(channel).active_slot = slot;
    Ok(())
}

pub(crate) fn set_calibration_slot_fit(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    let channel = payload.channel.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_channel_required",
            "Setting slot fit requires a channel.",
        )
    })?;
    let slot = payload.slot.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_slot_required",
            "Setting slot fit requires slot.",
        )
    })?;
    let fit = payload.fit.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_fit_required",
            "Setting slot fit requires gain/offset.",
        )
    })?;
    *calibration.channel_mut(channel).slot_fit_mut(slot) = fit;
    Ok(())
}

pub(crate) fn compact_calibration_samples(samples: &mut Vec<Option<CalibrationSample>>) {
    let mut compacted: Vec<Option<CalibrationSample>> =
        samples.iter().flatten().copied().map(Some).collect();
    compacted.resize(ADC_CALIBRATION_MAX_SAMPLES, None);
    *samples = compacted;
}

pub(crate) fn normalize_calibration_sample(
    sample: CalibrationSample,
    channel: CalibrationChannel,
) -> CalibrationSample {
    match channel {
        CalibrationChannel::RtdAdc => CalibrationSample {
            reference_vin_mv: None,
            ..sample
        },
        CalibrationChannel::VinAdc => CalibrationSample {
            reference_temp_c: None,
            ..sample
        },
    }
}

pub(crate) fn validate_calibration_channel_state(
    channel: &CalibrationChannelState,
) -> Result<(), HttpError> {
    if channel.samples.len() > ADC_CALIBRATION_MAX_SAMPLES {
        return Err(HttpError::bad_request(
            "calibration_samples_too_large",
            "Calibration import supports at most 8 samples per channel.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_calibration_state(state: &CalibrationState) -> Result<(), HttpError> {
    validate_calibration_channel_state(&state.rtd_adc)?;
    validate_calibration_channel_state(&state.vin_adc)?;
    Ok(())
}

pub(crate) fn normalize_calibration_channel_state(
    mut channel_state: CalibrationChannelState,
    channel: CalibrationChannel,
) -> CalibrationChannelState {
    channel_state.samples = channel_state
        .samples
        .into_iter()
        .map(|sample| sample.map(|sample| normalize_calibration_sample(sample, channel)))
        .collect();
    channel_state.refresh(channel);
    channel_state
}

pub(crate) fn normalize_calibration_state(mut state: CalibrationState) -> CalibrationState {
    state.rtd_adc = normalize_calibration_channel_state(state.rtd_adc, CalibrationChannel::RtdAdc);
    state.vin_adc = normalize_calibration_channel_state(state.vin_adc, CalibrationChannel::VinAdc);
    state
}

pub(crate) fn validate_heater_curve_package(package: &HeaterCurvePackage) -> Result<(), HttpError> {
    if package.points.len() > HEATER_CURVE_MAX_POINTS {
        return Err(HttpError::bad_request(
            "heater_curve_package_too_large",
            "Heater curve supports at most 8 points.",
        ));
    }
    if package
        .raw_observations
        .as_ref()
        .is_some_and(|observations| observations.points.len() > HEATER_CURVE_MAX_POINTS)
    {
        return Err(HttpError::bad_request(
            "heater_curve_raw_observations_too_large",
            "Heater curve raw observations support at most 8 points.",
        ));
    }
    Ok(())
}

pub(crate) fn normalize_heater_curve_package(
    mut package: HeaterCurvePackage,
) -> HeaterCurvePackage {
    package
        .points
        .sort_by_key(|point| point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX));
    package.points.resize(HEATER_CURVE_MAX_POINTS, None);
    if let Some(raw_observations) = package.raw_observations.as_mut() {
        raw_observations.points.sort_by_key(|observation| {
            observation
                .map(|observation| observation.raw_rtd_adc_mv)
                .unwrap_or(u16::MAX)
        });
        raw_observations
            .points
            .resize(HEATER_CURVE_MAX_POINTS, None);
    }
    package
}

pub(crate) fn mock_observed_adc_mv(channel: CalibrationChannel) -> u16 {
    match channel {
        CalibrationChannel::RtdAdc => 1_120,
        CalibrationChannel::VinAdc => 1_670,
    }
}

pub(crate) fn expected_calibration_adc_mv(
    payload: &CalibrationConfigRequest,
    channel: CalibrationChannel,
) -> Option<u16> {
    if let Some(expected_mv) = payload.expected_mv {
        return Some(expected_mv);
    }
    match channel {
        CalibrationChannel::RtdAdc => payload.target_adc_mv,
        CalibrationChannel::VinAdc => payload.reference_vin_mv.map(vin_adc_mv_for_input_mv),
    }
}

pub(crate) fn effective_pps_current_capability_ma(status: &ControlPlaneStatus) -> Option<u16> {
    u16::try_from(status.current_ma)
        .ok()
        .filter(|value| *value > 0)
        .or(status.pps_capability_max_ma)
}

pub(crate) fn validate_pps_voltage_against_status(
    millivolts: u16,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    let (Some(min_mv), Some(max_mv)) = (status.pps_capability_min_mv, status.pps_capability_max_mv)
    else {
        return Err(HttpError::bad_request(
            "manual_pps_no_capability",
            "PPS capability is unavailable.",
        ));
    };
    if millivolts < min_mv || millivolts > max_mv {
        return Err(HttpError::bad_request(
            "manual_pps_out_of_range",
            "manualPpsMv is outside the advertised PPS capability.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_manual_pps_request_against_status(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    if payload.manual_pps_enabled != Some(true)
        && payload.manual_pps_mv.is_none()
        && payload.manual_pps_ma.is_none()
    {
        return Ok(());
    }

    let manual_pps_mv = payload
        .manual_pps_mv
        .or(status.manual_pps_mv)
        .ok_or_else(|| HttpError::bad_request("invalid_manual_pps", "manualPpsMv is required."))?;
    let manual_pps_ma = payload
        .manual_pps_ma
        .or(status.manual_pps_ma)
        .or(status.pps_capability_max_ma)
        .ok_or_else(|| HttpError::bad_request("invalid_manual_pps", "manualPpsMa is required."))?;
    validate_manual_pps_against_status(manual_pps_mv, manual_pps_ma, status)
}

pub(crate) fn validate_calibration_request_against_status(
    calibration: &CalibrationControlRequest,
    status: &ControlPlaneStatus,
    current: &CalibrationRuntimeState,
) -> Result<(), HttpError> {
    if calibration.mode == Some(CalibrationMode::ThermalPlant) {
        return Err(HttpError::bad_request(
            "thermal_plant_managed_by_job",
            "Automatic thermal-model calibration is managed by thermal_plant_auto.",
        ));
    }
    let current_ma = effective_pps_current_capability_ma(status);
    if calibration.pps_enabled != Some(true) && calibration.pps_mv.is_none() {
        return Ok(());
    }

    let manual_pps_mv = calibration.pps_mv.or(current.pps_mv).ok_or_else(|| {
        HttpError::bad_request("invalid_calibration_pps", "calibration.ppsMv is required.")
    })?;
    let Some(_manual_pps_ma) = current_ma.or(status.manual_pps_ma) else {
        return Err(HttpError::bad_request(
            "invalid_calibration_pps",
            "Calibration PPS requires a readable PPS current capability.",
        ));
    };
    validate_pps_voltage_against_status(manual_pps_mv, status).map_err(|error| {
        if error.error.code == "manual_pps_no_capability" {
            HttpError::bad_request(
                "calibration_pps_no_capability",
                "PPS capability is unavailable.",
            )
        } else {
            HttpError::bad_request(
                "calibration_pps_out_of_range",
                "Calibration PPS request is outside the advertised PPS capability.",
            )
        }
    })?;
    Ok(())
}

pub(crate) fn mock_thermal_plant_job_running(status: &ControlPlaneStatus) -> bool {
    status.calibration.mode == CalibrationMode::ThermalPlant
        && status.calibration.job.status == CalibrationJobStatus::Running
}

pub(crate) fn thermal_plant_start_request_for_device(
    device: &DeviceRecord,
) -> Result<(MockPpsApdo, u16), HttpError> {
    let source = mock_thermal_plant_source_limits(device).ok_or_else(|| {
        HttpError::bad_request(
            "thermal_plant_source_unsupported",
            "Thermal-plant calibration requires a PPS capability covering 20V at 3A or more.",
        )
    })?;
    Ok((source, source.max_mv))
}

pub(crate) fn mock_thermal_plant_source_limits(device: &DeviceRecord) -> Option<MockPpsApdo> {
    let mut selected = None;
    let mut consider = |candidate: MockPpsApdo| {
        if candidate.min_mv > 20_000 || candidate.max_mv < 20_000 || candidate.max_ma < 3_000 {
            return;
        }
        if selected.is_none_or(|current: MockPpsApdo| {
            candidate.max_ma > current.max_ma
                || (candidate.max_ma == current.max_ma
                    && (candidate.max_mv > current.max_mv
                        || (candidate.max_mv == current.max_mv
                            && candidate.min_mv < current.min_mv)))
        }) {
            selected = Some(candidate);
        }
    };
    if device.mock_pps_apdos.is_empty() {
        if let (Some(min_mv), Some(max_mv), Some(max_ma)) = (
            device.status.pps_capability_min_mv,
            device.status.pps_capability_max_mv,
            device.status.pps_capability_max_ma,
        ) {
            consider(MockPpsApdo {
                min_mv,
                max_mv,
                max_ma,
            });
        }
    } else {
        for apdo in device.mock_pps_apdos.iter().copied() {
            consider(apdo);
        }
    }
    selected
}
