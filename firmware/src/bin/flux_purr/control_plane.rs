#[allow(unused_imports)]
use super::*;

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_runtime_status_with_calibration(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    calibration: &CalibrationRuntimeState,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    let pd_contract_mv = effective_pd_contract_mv(
        &context.manual_pps,
        context.last_pd_observation,
        context.heater_power_backend,
    );
    let pd_state = if context.current_rtd_fault.is_some() {
        PdState::Fault
    } else if context
        .last_pd_observation
        .is_some_and(|observation| observation.status.pd_active)
    {
        PdState::Ready
    } else if pd_contract_mv <= 5_000 {
        PdState::Fallback5V
    } else {
        PdState::Negotiating
    };
    let frontpanel_key = context
        .last_raw_state
        .first_pressed()
        .map(|raw_key| FrontPanelKeyMap::default().logical_from_raw(raw_key));

    let mut status = runtime_status_base(
        ui_state,
        memory_config,
        &context,
        pd_contract_mv,
        pd_state,
        frontpanel_key,
    );
    populate_status_measurements(&mut status, ui_state, &context);
    populate_status_contract(&mut status, &context, pd_contract_mv, pd_state);
    populate_status_control_and_faults(&mut status, ui_state, &context);
    populate_status_calibration(&mut status, ui_state, memory_config, calibration, &context);
    status
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn populate_status_measurements(
    status: &mut ControlPlaneStatus,
    ui_state: &FrontPanelUiState,
    context: &UsbRuntimeStatusContext,
) {
    status.target_temp_c = ui_state.target_temp_c;
    status.post_heat_cooling_mode = ui_state.post_heat_cooling_mode;
    status.heating_fan_guard_mode = ui_state.heating_fan_guard_mode;
    status.fan_policy_source = ui_state.fan_policy_source;
    status.fan_output_level = ui_state.fan_output_level;
    status.rtd_raw_adc_mv = context.latest_rtd_raw_adc_mv;
    status.rtd_raw_adc_min_mv = context.latest_rtd_raw_adc_min_mv;
    status.rtd_raw_adc_max_mv = context.latest_rtd_raw_adc_max_mv;
    status.rtd_raw_adc_spread_mv = context
        .latest_rtd_raw_adc_max_mv
        .saturating_sub(context.latest_rtd_raw_adc_min_mv);
    status.vin_raw_adc_mv = context.latest_vin_raw_adc_mv;
    *status.adc_diagnostics = adc_diagnostics_wire();
    status.manual_pps_enabled = context.manual_pps.enabled;
    status.manual_pps_mv = context.manual_pps.target_mv;
    status.manual_pps_ma = context.manual_pps.target_ma;
    status.pps_capability_min_mv = context.manual_pps.capability_min_mv;
    status.pps_capability_max_mv = context.manual_pps.capability_max_mv;
    status.pps_capability_max_ma = context.manual_pps.capability_max_ma;
    status.manual_pps_error = context.manual_pps.error.map(manual_pps_error_code);
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn populate_status_contract(
    status: &mut ControlPlaneStatus,
    context: &UsbRuntimeStatusContext,
    pd_contract_mv: u16,
    pd_state: PdState,
) {
    // This is contract metadata, not a VBUS current measurement. The legacy
    // `currentMa` field remains CH224Q telemetry.
    status.pd_controller = error_code_string(context.pd_controller.as_str());
    let observed_contract = context
        .last_pd_observation
        .map(|observation| observation.contract)
        .filter(|contract| *contract != Contract::none());
    let fusb_contract_pending =
        context.pd_controller == ControllerKind::Fusb302b && observed_contract.is_none();
    let fallback_contract_kind = if matches!(
        context.heater_power_backend,
        HeaterPowerBackend::PpsMos { .. }
    ) {
        ContractKind::Pps
    } else {
        ContractKind::Fixed
    };
    let contract_kind =
        observed_contract
            .map(|contract| contract.kind)
            .unwrap_or(if fusb_contract_pending {
                ContractKind::None
            } else {
                fallback_contract_kind
            });
    status.pd_contract_kind = error_code_string(contract_kind.as_str());
    status.pd_contract_current_ma = if fusb_contract_pending {
        0
    } else {
        observed_contract
            .map(|contract| contract.current_ma)
            .or(context.manual_pps.target_ma)
            .or(context.manual_pps.capability_max_ma)
            .unwrap_or(0)
    };
    let contract_voltage_mv = observed_contract
        .map(|contract| contract.voltage_mv)
        .unwrap_or(if fusb_contract_pending {
            0
        } else {
            pd_contract_mv
        });
    status.pd_contract_power_mw =
        (u32::from(contract_voltage_mv) * u32::from(status.pd_contract_current_ma)) / 1_000;
    status.pd_performance_guaranteed = if fusb_contract_pending {
        false
    } else {
        observed_contract
            .map(Contract::performance_guaranteed)
            .unwrap_or(
                matches!(pd_state, PdState::Ready)
                    && pd_contract_mv >= 20_000
                    && status.pd_contract_current_ma >= 3_000,
            )
    };
    status.pd_degraded_reason =
        if matches!(pd_state, PdState::Ready) && !status.pd_performance_guaranteed {
            Some(error_code_string("pd_contract_below_20v"))
        } else if !matches!(pd_state, PdState::Ready) {
            Some(error_code_string(
                if context.pd_controller == ControllerKind::Fusb302b {
                    fusb302b_degraded_reason()
                } else {
                    "pd_contract_unavailable"
                },
            ))
        } else {
            None
        };
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn populate_status_control_and_faults(
    status: &mut ControlPlaneStatus,
    ui_state: &FrontPanelUiState,
    context: &UsbRuntimeStatusContext,
) {
    status.heater_fault_reason = context.heater_fault_latched.map(|reason| {
        let mut value = heapless::String::new();
        let _ = value.push_str(reason.label());
        value
    });
    status.fault_attention_pending = context.attention_pending_after_fault_clear;
    status.persistence_fault = ui_state.persistence_fault.clone();
    status.persistence_fault_attention_pending = ui_state.persistence_fault_attention_pending;
    status.heater_lock_reason = ui_state.heater_lock_reason.map(Into::into);
    let mut heater_control_phase = heapless::String::new();
    let _ = heater_control_phase.push_str(context.pid_snapshot.phase.label());
    status.heater_control_phase = Some(heater_control_phase);
    status.heater_error_c = Some(context.pid_snapshot.error_c);
    status.heater_control_error_c = Some(context.pid_snapshot.control_error_c);
    status.heater_control_temp_c = Some(context.latest_control_temp_c);
    status.heater_control_measurement_guarded = context.control_measurement_guarded;
    status.heater_filtered_temp_c = Some(context.pid_snapshot.filtered_temp_c);
    status.heater_filtered_slope_c_per_s = Some(context.pid_snapshot.filtered_slope_c_per_s);
    status.heater_coast_active = context.pid_snapshot.coast_active;
    status.heater_control_interval_ms = context.heater_control_timing.interval_ms;
    status.heater_control_cycle_ms = context.heater_control_timing.cycle_ms;
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn populate_status_calibration(
    status: &mut ControlPlaneStatus,
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    calibration: &CalibrationRuntimeState,
    context: &UsbRuntimeStatusContext,
) {
    status.calibration = calibration_runtime_state_to_wire(calibration);
    status.thermal_control_profile_preview = context.thermal_control_profile_preview;
    let resolved_bank =
        resolve_thermal_profile_bank(memory_config.thermal_profile_mode, &context.manual_pps);
    let mut thermal_profile_mode = heapless::String::new();
    let _ = thermal_profile_mode.push_str(memory_config.thermal_profile_mode.as_str());
    status.thermal_profile_mode = thermal_profile_mode;
    let mut thermal_profile_resolved_bank = heapless::String::new();
    let _ = thermal_profile_resolved_bank.push_str(resolved_bank.as_str());
    status.thermal_profile_resolved_bank = thermal_profile_resolved_bank;
    status.thermal_control = if calibration.mode == CalibrationMode::ThermalPlant {
        flux_purr_firmware::control_plane::ThermalControlRuntimeWire::default()
    } else {
        thermal_control_runtime_wire(
            ui_state.target_temp_c,
            context.active_thermal_control_profile,
            context.thermal_control_profile_preview,
        )
    };
    status.thermal_plant_model = thermal_plant_runtime_wire(memory_config);
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn runtime_status_base(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    context: &UsbRuntimeStatusContext,
    pd_contract_mv: u16,
    pd_state: PdState,
    frontpanel_key: Option<flux_purr_firmware::frontpanel::FrontPanelKey>,
) -> Box<ControlPlaneStatus> {
    ControlPlaneStatus::boxed_from_device_status(
        DeviceStatus {
            mode: if context.current_rtd_fault.is_some() {
                DeviceMode::Fault
            } else if ui_state.heater_enabled {
                DeviceMode::Sampling
            } else {
                DeviceMode::Idle
            },
            voltage_mv: context.vin_mv,
            current_ma: u32::from(
                context
                    .last_pd_observation
                    .map(|observation| observation.current_ma)
                    .unwrap_or(0),
            ),
            board_temp_centi: temp_c_to_centi_c(context.latest_status_temp_c),
            rtd_raw_adc_mv: 0,
            vin_raw_adc_mv: 0,
            pd_request_mv: context
                .manual_pps
                .target_mv
                .filter(|_| context.manual_pps.enabled)
                .unwrap_or_else(|| context.heater_power_backend.pd_request_mv()),
            pd_contract_mv,
            pd_state,
            heater_output_percent: ui_state.heater_output_percent,
            heater_physical_output_percent: context.heater_physical_output_percent,
            fan_enabled: ui_state.fan_enabled,
            fan_pwm_permille: context.fan_command.pwm_permille,
            frontpanel_key,
        },
        memory_config,
        (context.elapsed_ms / 1_000).min(u64::from(u32::MAX)) as u32,
        ui_state.network.clone(),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_runtime_status(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    calibration: &CalibrationRuntimeState,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    usb_runtime_status_with_calibration(ui_state, memory_config, calibration, context)
}

#[cfg(test)]
pub(crate) fn usb_runtime_status(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    let calibration = context.calibration;
    usb_runtime_status_with_calibration(ui_state, memory_config, &calibration, context)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) struct UsbRuntimeConfigInput<'a> {
    pub(crate) ui_state: &'a mut FrontPanelUiState,
    pub(crate) memory_config: &'a mut MemoryConfig,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) thermal_control_profile_preview: &'a mut Option<ThermalControlProfile>,
    pub(crate) calibration: &'a mut CalibrationRuntimeState,
    pub(crate) context: UsbRuntimeStatusContext,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_runtime_config_response_with_calibration(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    input: UsbRuntimeConfigInput<'_>,
) -> UsbFrame {
    let UsbRuntimeConfigInput {
        ui_state,
        memory_config,
        manual_pps,
        thermal_control_profile_preview,
        calibration,
        mut context,
    } = input;
    if let Some(error) = runtime_config_validation(&config, *calibration) {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        };
    }

    if let Some(calibration_command) = config.calibration
        && let Err(error) =
            apply_calibration_control_config(&calibration_command, calibration, manual_pps)
    {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(ApiError::new(error.code(), error.message(), false)),
        };
    }
    if let Err(error) = apply_manual_pps_config(&config, *calibration, manual_pps) {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(ApiError::new(error.code(), error.message(), false)),
        };
    }
    apply_runtime_config_state(
        &config,
        ui_state,
        memory_config,
        manual_pps,
        *calibration,
        thermal_control_profile_preview,
    );
    ui_state.manual_pps_enabled = manual_pps.enabled;
    context.manual_pps = *manual_pps;
    context.thermal_control_profile_preview = thermal_control_profile_preview.is_some();
    context.active_thermal_control_profile =
        active_thermal_control_profile(memory_config, *thermal_control_profile_preview, manual_pps);

    usb_response(
        request_id,
        UsbResponsePayload::Status(usb_runtime_status_with_calibration(
            ui_state,
            memory_config,
            calibration,
            context,
        )),
    )
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn runtime_config_validation(
    config: &RuntimeConfigCommand,
    calibration: CalibrationRuntimeState,
) -> Option<ApiError> {
    if config.fan_policy_conflicts() {
        return Some(ApiError::new(
            "fan_policy_conflict",
            "activeCoolingEnabled conflicts with postHeatCoolingMode.",
            false,
        ));
    }
    let manual_pps_requested = config.manual_pps_enabled.is_some()
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some();
    if thermal_plant_calibration_job_running(calibration)
        && (manual_pps_requested
            || config.calibration.is_some()
            || config.heater_enabled == Some(true))
    {
        return Some(ApiError::new(
            ManualPpsError::CalibrationInProgress.code(),
            "Manual PPS and heater controls cannot override a running thermal-model calibration.",
            false,
        ));
    }
    match config.thermal_control_profile {
        Some(ThermalControlProfileCommand {
            op: ThermalControlProfileOp::Preview | ThermalControlProfileOp::Save,
            profile: None,
            ..
        }) => Some(ApiError::new(
            "thermal_profile_required",
            "thermalControlProfile.profile is required for preview/save.",
            false,
        )),
        Some(ThermalControlProfileCommand {
            op: ThermalControlProfileOp::Save,
            profile: Some(profile),
            ..
        }) if profile.points.iter().flatten().count()
            > THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS =>
        {
            Some(ApiError::new(
                "thermal_profile_too_many_saved_points",
                "saved thermal profiles support at most 10 populated points.",
                false,
            ))
        }
        Some(ThermalControlProfileCommand {
            op: ThermalControlProfileOp::ClearPreview | ThermalControlProfileOp::ClearSaved,
            profile: Some(_),
            ..
        }) => Some(ApiError::new(
            "thermal_profile_clear_payload",
            "thermalControlProfile.profile must be omitted for clear operations.",
            false,
        )),
        _ => None,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn apply_runtime_config_state(
    config: &RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &ManualPpsState,
    calibration: CalibrationRuntimeState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
) {
    if let Some(command) = config.thermal_control_profile {
        match command.op {
            ThermalControlProfileOp::Preview => {
                if let Some(profile) = command.profile {
                    *thermal_control_profile_preview = Some(ThermalControlProfile::from(profile));
                }
            }
            ThermalControlProfileOp::ClearPreview => *thermal_control_profile_preview = None,
            ThermalControlProfileOp::Save => *thermal_control_profile_preview = None,
            ThermalControlProfileOp::ClearSaved => {}
        }
    }
    let heater_enabled = if config.heater_enabled == Some(true)
        && !thermal_model_heater_allowed(memory_config, calibration, *manual_pps)
    {
        Some(false)
    } else {
        config.heater_enabled
    };
    config.apply_to(memory_config);
    apply_memory_config_to_ui(ui_state, memory_config);
    if let Some(heater_enabled) = heater_enabled {
        ui_state.heater_enabled = heater_enabled;
    }
    if calibration.mode != CalibrationMode::Off {
        if let Some(heater_enabled) = config
            .calibration
            .and_then(|calibration| calibration.heater_enabled)
        {
            ui_state.heater_enabled = heater_enabled;
        }
        apply_rtd_calibration_hold_target(ui_state, calibration);
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn apply_rtd_calibration_hold_target(
    ui_state: &mut FrontPanelUiState,
    calibration: CalibrationRuntimeState,
) {
    if calibration.mode != CalibrationMode::RtdAdc || !calibration.heater_enabled {
        return;
    }
    let Some(target_adc_mv) = calibration.target_adc_mv else {
        return;
    };
    let target_c = pt1000_temperature_c_from_resistance(
        rtd_resistance_ohms_from_mv(target_adc_mv)
            .unwrap_or_else(|_| pt1000_resistance_ohms_at(0.0)),
    );
    let rounded_c = if target_c >= 0.0 {
        (target_c + 0.5) as i16
    } else {
        (target_c - 0.5) as i16
    };
    ui_state.target_temp_c =
        rounded_c.clamp(FRONTPANEL_TARGET_TEMP_MIN_C, FRONTPANEL_TARGET_TEMP_MAX_C);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_runtime_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    input: UsbRuntimeConfigInput<'_>,
) -> UsbFrame {
    usb_runtime_config_response_with_calibration(request_id, config, input)
}

#[cfg(test)]
pub(crate) fn usb_runtime_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    context: UsbRuntimeStatusContext,
) -> (UsbFrame, CalibrationRuntimeState) {
    let mut calibration = context.calibration;
    let response = usb_runtime_config_response_with_calibration(
        request_id,
        config,
        UsbRuntimeConfigInput {
            ui_state,
            memory_config,
            manual_pps,
            thermal_control_profile_preview,
            calibration: &mut calibration,
            context,
        },
    );
    (response, calibration)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn apply_manual_pps_config(
    config: &RuntimeConfigCommand,
    calibration: CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    let manual_pps_requested = config.manual_pps_enabled.is_some()
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some();
    if calibration.immediate_heater_disarm_pending
        && manual_pps_requested
        && config.manual_pps_enabled != Some(false)
    {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    if calibration.mode == CalibrationMode::ThermalPlant
        && calibration.job.status == CalibrationJobStatus::Running
        && manual_pps_requested
    {
        return Err(ManualPpsError::CalibrationInProgress);
    }
    if config.manual_pps_enabled == Some(false) {
        manual_pps.clear();
        return Ok(());
    }

    if config.manual_pps_enabled == Some(true)
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some()
    {
        let target_mv = config
            .manual_pps_mv
            .or(manual_pps.target_mv)
            .ok_or(ManualPpsError::InvalidVoltage)?;
        let target_ma = config.manual_pps_ma.or(manual_pps.target_ma);
        manual_pps.enable(ManualPpsOwner::Debug, target_mv, target_ma)?;
    }

    Ok(())
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn apply_calibration_control_config(
    config: &CalibrationControlCommand,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    let requests_mutation = config.mode.is_some()
        || config.pps_enabled.is_some()
        || config.pps_mv.is_some()
        || config.heater_enabled.is_some()
        || config.target_adc_mv.is_some();
    if thermal_plant_calibration_job_running(*calibration) && requests_mutation {
        return Err(ManualPpsError::CalibrationInProgress);
    }
    if calibration.immediate_heater_disarm_pending && requests_mutation {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    if config.mode == Some(CalibrationModeWire::ThermalPlant) {
        return Err(ManualPpsError::ThermalPlantManagedByJob);
    }
    if let Some(mode) = config.mode {
        calibration.mode = mode.into();
        if calibration.mode == CalibrationMode::Off {
            calibration.pps_enabled = false;
            calibration.heater_enabled = false;
            calibration.target_adc_mv = None;
            calibration.stable = false;
            calibration.stability_error_mv = None;
            calibration.job = CalibrationJobState::default();
            calibration.job_data = None;
            if manual_pps.owner == ManualPpsOwner::Calibration {
                manual_pps.clear();
                calibration.immediate_heater_disarm_pending = true;
            }
        }
    }

    if let Some(target_adc_mv) = config.target_adc_mv {
        calibration.target_adc_mv = Some(target_adc_mv);
    }

    if let Some(heater_enabled) = config.heater_enabled {
        calibration.heater_enabled = heater_enabled;
    }

    if config.pps_enabled == Some(false) {
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
        if manual_pps.owner == ManualPpsOwner::Calibration {
            manual_pps.clear();
        }
        return Ok(());
    }

    if config.pps_enabled == Some(true) || config.pps_mv.is_some() {
        let target_mv = config
            .pps_mv
            .or(calibration.pps_mv)
            .ok_or(ManualPpsError::InvalidVoltage)?;
        let target_ma = calibration.pps_ma.or(manual_pps.target_ma);
        manual_pps.enable(ManualPpsOwner::Calibration, target_mv, target_ma)?;
        calibration.pps_enabled = true;
        calibration.pps_mv = manual_pps.target_mv;
        calibration.pps_ma = manual_pps.target_ma;
        calibration.error = None;
    }

    Ok(())
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn thermal_plant_calibration_job_running(calibration: CalibrationRuntimeState) -> bool {
    calibration.mode == CalibrationMode::ThermalPlant
        && calibration.job.status == CalibrationJobStatus::Running
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn update_calibration_runtime_state(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &ManualPpsState,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) {
    calibration.pps_enabled = manual_pps.enabled && manual_pps.owner == ManualPpsOwner::Calibration;
    if calibration.pps_enabled {
        calibration.pps_mv = manual_pps.target_mv;
        calibration.pps_ma = manual_pps.target_ma;
        calibration.error = manual_pps.error;
    } else if manual_pps.owner != ManualPpsOwner::Calibration {
        calibration.pps_mv = None;
        calibration.pps_ma = None;
        calibration.error = None;
    }

    let observed_adc_mv = match calibration.mode {
        CalibrationMode::RtdAdc => Some(latest_rtd_raw_adc_mv),
        CalibrationMode::VinAdc => Some(latest_vin_raw_adc_mv),
        CalibrationMode::Off | CalibrationMode::HeaterCurve | CalibrationMode::ThermalPlant => None,
    };

    calibration.stability_error_mv = calibration
        .target_adc_mv
        .zip(observed_adc_mv)
        .map(|(target, observed)| (i32::from(observed) - i32::from(target)) as i16);
    calibration.stable = calibration
        .stability_error_mv
        .is_some_and(|error_mv| error_mv.abs() <= 8);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn calibration_job_fail(
    calibration: &mut CalibrationRuntimeState,
    error: ManualPpsError,
    clear_manual_pps: bool,
    manual_pps: &mut ManualPpsState,
) {
    let thermal_plant_job = calibration.job.kind == Some(CalibrationJobKind::ThermalPlant);
    calibration.job.status = CalibrationJobStatus::Failed;
    calibration.job.message = Some(error);
    calibration.job_data = None;
    calibration.model_target_temp_c = None;
    calibration.heater_enabled = false;
    if thermal_plant_job {
        calibration.mode = CalibrationMode::Off;
        calibration.immediate_heater_disarm_pending = true;
    }
    if clear_manual_pps && manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn calibration_job_complete(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    samples_collected: u8,
    next_request_mv: Option<u16>,
) {
    calibration.job.kind = Some(kind);
    calibration.job.status = CalibrationJobStatus::Completed;
    calibration.job.progress_percent = 100;
    calibration.job.samples_collected = samples_collected;
    calibration.job.next_request_mv = next_request_mv;
    calibration.job.message = None;
    calibration.job_data = None;
    calibration.model_target_temp_c = None;
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn calibration_job_canceled(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    if calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    calibration.job.status = CalibrationJobStatus::Canceled;
    calibration.job.message = None;
    calibration.job_data = None;
    calibration.heater_enabled = false;
    calibration.mode = CalibrationMode::Off;
    calibration.immediate_heater_disarm_pending = true;
    if manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn disarm_calibration_after_transient_input_change(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    calibration.heater_enabled = false;
    calibration.pps_enabled = false;
    calibration.pps_mv = None;
    calibration.pps_ma = None;
    if manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
    }
    calibration.immediate_heater_disarm_pending = true;
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn disarm_calibration_after_capability_refresh(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    if calibration.heater_enabled
        || calibration.pps_enabled
        || manual_pps.owner == ManualPpsOwner::Calibration
    {
        disarm_calibration_after_transient_input_change(calibration, manual_pps);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn calibration_job_start_with_workspace(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> Result<(), ManualPpsError> {
    if calibration.job.status == CalibrationJobStatus::Running
        || calibration.immediate_heater_disarm_pending
    {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    match kind {
        CalibrationJobKind::VinAdc => {
            start_vin_calibration_job(calibration, memory_config, manual_pps, kind)
        }
        CalibrationJobKind::ThermalPlant => start_thermal_plant_calibration_job(
            calibration,
            manual_pps,
            thermal_plant_workspace,
            kind,
        ),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn start_vin_calibration_job(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    kind: CalibrationJobKind,
) -> Result<(), ManualPpsError> {
    if calibration.mode != CalibrationMode::VinAdc {
        return Err(ManualPpsError::InvalidVoltage);
    }
    let min_mv = manual_pps
        .capability_min_mv
        .ok_or(ManualPpsError::NoPpsCapability)?
        .max(5_000);
    let max_mv = manual_pps
        .capability_max_mv
        .ok_or(ManualPpsError::NoPpsCapability)?
        .min(28_000);
    let next_request_mv = min_mv.div_ceil(100) * 100;
    let target_ma = manual_pps
        .maximum_pps_current_for_target(next_request_mv)
        .ok_or(ManualPpsError::NoPpsCapability)?;
    manual_pps.enable(
        ManualPpsOwner::Calibration,
        next_request_mv,
        Some(target_ma),
    )?;
    memory_config.adc_calibration.vin.clear();
    memory_config.sanitize();
    calibration.pps_enabled = true;
    calibration.pps_mv = Some(next_request_mv);
    calibration.pps_ma = Some(target_ma);
    calibration.heater_enabled = false;
    calibration.job = CalibrationJobState {
        kind: Some(kind),
        status: CalibrationJobStatus::Running,
        progress_percent: 0,
        samples_collected: 0,
        next_request_mv: Some(next_request_mv),
        message: None,
    };
    calibration.job_data = Some(CalibrationJobData::VinAdc(CalibrationVinAutoJob {
        start_request_mv: next_request_mv,
        next_request_mv,
        max_request_mv: max_mv,
        target_ma,
        settle_ticks: 0,
        stable_ticks: 0,
        last_observed_mv: None,
        sample_count: 0,
        samples: [None; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
    }));
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn start_thermal_plant_calibration_job(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    workspace: &mut CalibrationThermalPlantWorkspace,
    kind: CalibrationJobKind,
) -> Result<(), ManualPpsError> {
    let (_, source_max_mv, source_current_ma) = manual_pps
        .thermal_plant_source_limits()
        .ok_or(ManualPpsError::ThermalPlantSourceUnsupported)?;
    let request_mv = source_max_mv;
    manual_pps.enable(
        ManualPpsOwner::Calibration,
        request_mv,
        Some(source_current_ma),
    )?;
    calibration.mode = CalibrationMode::ThermalPlant;
    calibration.pps_enabled = true;
    calibration.pps_mv = manual_pps.target_mv;
    calibration.pps_ma = manual_pps.target_ma;
    calibration.heater_enabled = false;
    calibration.model_target_temp_c = None;
    calibration.job = CalibrationJobState {
        kind: Some(kind),
        status: CalibrationJobStatus::Running,
        progress_percent: 0,
        samples_collected: 0,
        next_request_mv: Some(request_mv),
        message: None,
    };
    workspace.next_run_id = workspace.next_run_id.wrapping_add(1).max(1);
    workspace.job = Some(CalibrationThermalPlantAutoJob {
        run_id: workspace.next_run_id,
        phase: ThermalPlantAutoPhase::Ambient,
        source_max_mv,
        source_current_ma,
        ambient_raw_rtd_adc_mv: 0,
        idle_samples: 0,
        heater_curve: ThermalPlantCurveSampler::default(),
        elapsed_ticks: 0,
        phase_started_tick: 0,
        sample_count: 0,
        last_saved_temp_c: 0.0,
        last_saved_tick: 0,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_125mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    });
    calibration.job_data = Some(CalibrationJobData::ThermalPlant);
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn calibration_job_start(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> Result<(), ManualPpsError> {
    calibration_job_start_with_workspace(
        calibration,
        kind,
        memory_config,
        manual_pps,
        thermal_plant_workspace,
    )
}

#[cfg(test)]
std::thread_local! {
    static TEST_THERMAL_PLANT_WORKSPACE: core::cell::RefCell<CalibrationThermalPlantWorkspace> =
        core::cell::RefCell::new(CalibrationThermalPlantWorkspace::default());
}

#[cfg(test)]
pub(crate) fn calibration_job_start(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        calibration_job_start_with_workspace(
            calibration,
            kind,
            memory_config,
            manual_pps,
            &mut workspace.borrow_mut(),
        )
    })
}

#[cfg(test)]
pub(crate) fn test_thermal_plant_phase() -> Option<ThermalPlantAutoPhase> {
    TEST_THERMAL_PLANT_WORKSPACE
        .with(|workspace| workspace.borrow().job.as_ref().map(|job| job.phase))
}

#[cfg(test)]
pub(crate) fn test_install_thermal_plant_job(job: CalibrationThermalPlantAutoJob) {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        workspace.borrow_mut().job = Some(job);
    });
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn monotonic_smooth_heater_curve_points(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
) {
    if points.len() <= 1 {
        return;
    }

    for index in 1..points.len() {
        if points[index].resistance_milliohms < points[index - 1].resistance_milliohms {
            points[index].resistance_milliohms = points[index - 1].resistance_milliohms;
        }
        if points[index].temp_centi_c <= points[index - 1].temp_centi_c {
            points[index].temp_centi_c = points[index - 1].temp_centi_c.saturating_add(1);
        }
    }

    if points.len() >= 3 {
        let original = points.clone();
        for index in 1..(points.len() - 1) {
            let left = u32::from(original[index - 1].resistance_milliohms);
            let center = u32::from(original[index].resistance_milliohms);
            let right = u32::from(original[index + 1].resistance_milliohms);
            points[index].resistance_milliohms = ((left + center + right) / 3) as u16;
        }
        for index in 1..points.len() {
            if points[index].resistance_milliohms < points[index - 1].resistance_milliohms {
                points[index].resistance_milliohms = points[index - 1].resistance_milliohms;
            }
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn enforce_heater_curve_model_floor(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
) {
    for point in points {
        let temp_c = f32::from(point.temp_centi_c) / 100.0;
        point.resistance_milliohms = point
            .resistance_milliohms
            .max(heater_curve_model_floor_milliohms(temp_c));
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_curve_model_floor_milliohms(temp_c: f32) -> u16 {
    round_to_u16_nonnegative(default_estimated_heater_resistance_ohms(temp_c) * 1000.0)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_vin_auto_draft_samples(
    collected: &[Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
    sample_count: usize,
) -> heapless::Vec<AdcCalibrationSample, ADC_CALIBRATION_MAX_SAMPLES> {
    let mut dense =
        heapless::Vec::<AdcCalibrationSample, CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES>::new();
    for sample in collected.iter().take(sample_count).flatten() {
        let _ = dense.push(*sample);
    }

    let mut selected = heapless::Vec::<AdcCalibrationSample, ADC_CALIBRATION_MAX_SAMPLES>::new();
    if dense.is_empty() {
        return selected;
    }
    if dense.len() <= ADC_CALIBRATION_MAX_SAMPLES {
        for sample in dense {
            let _ = selected.push(sample);
        }
        return selected;
    }

    let last_index = dense.len() - 1;
    let bucket_count = ADC_CALIBRATION_MAX_SAMPLES - 1;
    let mut previous_index = None::<usize>;
    for slot in 0..ADC_CALIBRATION_MAX_SAMPLES {
        let index = if slot == 0 {
            0
        } else if slot == ADC_CALIBRATION_MAX_SAMPLES - 1 {
            last_index
        } else {
            ((slot * last_index) + (bucket_count / 2)) / bucket_count
        };
        if previous_index == Some(index) {
            continue;
        }
        previous_index = Some(index);
        let _ = selected.push(dense[index]);
    }
    selected
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn commit_vin_auto_samples_to_draft(
    memory_config: &mut MemoryConfig,
    collected: &[Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
    sample_count: usize,
) -> usize {
    let selected = select_vin_auto_draft_samples(collected, sample_count);
    memory_config.adc_calibration.vin.clear();
    for sample in selected {
        let _ = memory_config.adc_calibration.vin.insert(sample);
    }
    memory_config.sanitize();
    memory_config.adc_calibration.vin.sample_count()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_curve_from_transient_bins(
    bins: &[ThermalPlantCurveBin; 4],
) -> Option<HeaterCurveConfig> {
    let mut measured = heapless::Vec::<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>::new();
    for bin in bins {
        let Some((temp_centi_c, resistance_milliohms)) = bin.averaged_point() else {
            continue;
        };
        let _ = measured.push(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }
    if measured.is_empty() {
        return None;
    }
    monotonic_smooth_heater_curve_points(&mut measured);
    enforce_heater_curve_model_floor(&mut measured);

    let mut compacted = heapless::Vec::<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>::new();
    push_heater_curve_point_monotonic(
        &mut compacted,
        default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C),
    );
    push_heater_curve_point_monotonic(
        &mut compacted,
        default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C),
    );
    for point in measured {
        push_heater_curve_point_monotonic(&mut compacted, point);
    }

    let mut points = [None; HEATER_CURVE_MAX_POINTS];
    for (index, point) in compacted.into_iter().enumerate() {
        points[index] = Some(point);
    }
    Some(HeaterCurveConfig { points })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_curve_raw_observations_from_transient_bins(
    cold_bin: ThermalPlantCurveBin,
    bins: &[ThermalPlantCurveBin; 4],
) -> Option<HeaterCurveRawObservations> {
    let mut points = [None; HEATER_CURVE_MAX_POINTS];
    let mut count = 0;
    if let Some(point) = cold_bin.averaged_raw_observation() {
        points[count] = Some(point);
        count += 1;
    }
    for bin in bins {
        if let Some(point) = bin.averaged_raw_observation() {
            points[count] = Some(point);
            count += 1;
        }
    }
    (count >= 2).then_some(HeaterCurveRawObservations { points })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_curve_samples_ready(job: &ThermalPlantCurveSampler) -> bool {
    job.cold_bin.samples >= THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN
        && job
            .bins
            .iter()
            .all(|bin| bin.samples >= THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_heater_curve_point(temp_c: f32) -> HeaterCurvePoint {
    HeaterCurvePoint {
        temp_centi_c: round_to_i16(temp_c * 100.0),
        resistance_milliohms: round_to_u16_nonnegative(
            default_estimated_heater_resistance_ohms(temp_c) * 1_000.0,
        ),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn push_heater_curve_point_monotonic(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
    mut point: HeaterCurvePoint,
) {
    if let Some(previous) = points.last().copied() {
        if point.temp_centi_c <= previous.temp_centi_c {
            point.temp_centi_c = previous.temp_centi_c.saturating_add(1);
        }
        if point.resistance_milliohms < previous.resistance_milliohms {
            point.resistance_milliohms = previous.resistance_milliohms;
        }
    }
    let _ = points.push(point);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn record_thermal_plant_transient_sample(
    job: &mut CalibrationThermalPlantAutoJob,
    raw_rtd_adc_mv: u16,
    latest_temp_c: f32,
    latest_vin_mv: u32,
    duty_percent: u8,
    force: bool,
) -> bool {
    if raw_rtd_adc_mv == 0 || !latest_temp_c.is_finite() || duty_percent > 100 {
        return false;
    }
    let elapsed_ticks = job.elapsed_ticks.min(u32::from(u16::MAX)) as u16;
    let should_record = force
        || job.sample_count < 24
        || (latest_temp_c - job.last_saved_temp_c).abs() >= THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C;
    if !should_record {
        return true;
    }
    let index = usize::from(job.sample_count);
    if index >= THERMAL_PLANT_TRANSIENT_MAX_SAMPLES
        || (index > 0 && elapsed_ticks <= job.last_saved_tick)
    {
        return false;
    }
    job.samples[index] = ThermalPlantTransientSample {
        elapsed_ticks,
        raw_rtd_adc_mv,
        heater_voltage_125mv: quantize_thermal_plant_heater_voltage_mv(latest_vin_mv),
        duty_percent,
    };
    job.sample_count = job.sample_count.saturating_add(1);
    job.last_saved_temp_c = latest_temp_c;
    job.last_saved_tick = elapsed_ticks;
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_sample_power_mw(
    sample: ThermalPlantTransientSample,
    temp_c: f32,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> Option<f32> {
    if sample.duty_percent == 0 {
        return Some(0.0);
    }
    let voltage_v =
        f32::from(thermal_plant_heater_voltage_mv(sample.heater_voltage_125mv)) / 1_000.0;
    let resistance_ohms =
        estimated_heater_resistance_ohms(temp_c, preview_heater_curve, memory_config);
    (voltage_v > 0.0 && resistance_ohms.is_finite() && resistance_ohms > 0.1).then_some(
        voltage_v * voltage_v / resistance_ohms * 1_000.0 * f32::from(sample.duty_percent) / 100.0,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn solve_transient_normal_equations(
    normal: [[f32; 3]; 3],
    rhs: [f32; 3],
    mask: u8,
) -> Option<[f32; 3]> {
    let mut selected = [0usize; 3];
    let mut count = 0usize;
    for column in 0..3 {
        if mask & (1 << column) != 0 {
            selected[count] = column;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mut matrix = [[0.0_f32; 3]; 3];
    let mut target = [0.0_f32; 3];
    for row in 0..count {
        target[row] = rhs[selected[row]];
        for column in 0..count {
            matrix[row][column] = normal[selected[row]][selected[column]];
        }
    }
    for pivot in 0..count {
        let mut best = pivot;
        for row in pivot + 1..count {
            if matrix[row][pivot].abs() > matrix[best][pivot].abs() {
                best = row;
            }
        }
        if matrix[best][pivot].abs() < 1.0e-6 {
            return None;
        }
        if best != pivot {
            matrix.swap(best, pivot);
            target.swap(best, pivot);
        }
        let divisor = matrix[pivot][pivot];
        for value in matrix[pivot][pivot..count].iter_mut() {
            *value /= divisor;
        }
        target[pivot] /= divisor;
        for row in 0..count {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            let pivot_row = matrix[pivot];
            for (offset, value) in matrix[row][pivot..count].iter_mut().enumerate() {
                *value -= factor * pivot_row[pivot + offset];
            }
            target[row] -= factor * target[pivot];
        }
    }
    let mut output = [0.0_f32; 3];
    for index in 0..count {
        output[selected[index]] = target[index];
    }
    Some(output)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn determinant_3x3(matrix: [[f32; 3]; 3]) -> f32 {
    matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_fit_row(
    samples: &[ThermalPlantTransientSample],
    temperatures_c: &[f32],
    powers_mw: &[f32],
    index: usize,
    delay_ticks: u16,
    ambient_temp_c: f32,
) -> Option<([f32; 3], f32)> {
    const MIN_DERIVATIVE_WINDOW_TICKS: u16 = 10;
    let previous_index = (0..index).rev().find(|candidate| {
        samples[index]
            .elapsed_ticks
            .saturating_sub(samples[*candidate].elapsed_ticks)
            >= MIN_DERIVATIVE_WINDOW_TICKS
    })?;
    let dt_ticks = samples[index]
        .elapsed_ticks
        .saturating_sub(samples[previous_index].elapsed_ticks);
    let delayed_tick = samples[index].elapsed_ticks.saturating_sub(delay_ticks);
    let delayed_index = (0..index)
        .rev()
        .find(|candidate| samples[*candidate].elapsed_ticks <= delayed_tick)?;
    let mid_temp_c = (temperatures_c[index] + temperatures_c[previous_index]) * 0.5;
    let ambient_kelvin = ambient_temp_c + 273.15;
    let kelvin = mid_temp_c + 273.15;
    Some((
        [
            (temperatures_c[index] - temperatures_c[previous_index])
                / (f32::from(dt_ticks) * HEATER_CONTROL_INTERVAL_MS as f32 / 1_000.0),
            (mid_temp_c - ambient_temp_c).max(0.0),
            kelvin.powi(4) - ambient_kelvin.powi(4),
        ],
        powers_mw[delayed_index],
    ))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) struct TransientFitData {
    samples: [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    count: usize,
    ambient_temp_c: f32,
    temperatures_c: [f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    powers_mw: [f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn prepare_transient_fit(
    samples: &[ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    sample_count: u8,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
    ambient_raw_rtd_adc_mv: u16,
) -> Option<TransientFitData> {
    let count = usize::from(sample_count);
    if count < 24 {
        return None;
    }
    let ambient_temp_c = projected_rtd_temperature_c(memory_config, ambient_raw_rtd_adc_mv)?;
    let mut temperatures_c = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powers_mw = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powered_max_temp_c = f32::MIN;
    let mut powered_peak_index = None;
    for (index, sample) in samples[..count].iter().enumerate() {
        let temperature_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)?;
        let power_mw =
            transient_sample_power_mw(*sample, temperature_c, preview_heater_curve, memory_config)?;
        if !temperature_c.is_finite() || !power_mw.is_finite() || power_mw < 0.0 {
            return None;
        }
        if sample.duty_percent == 100 && temperature_c > powered_max_temp_c {
            powered_max_temp_c = temperature_c;
            powered_peak_index = Some(index);
        }
        temperatures_c[index] = temperature_c;
        powers_mw[index] = power_mw;
    }
    let powered_peak_index = powered_peak_index?;
    let last_sample_is_cooling = samples[..count]
        .last()
        .is_some_and(|sample| sample.duty_percent == 0);
    if powered_max_temp_c < THERMAL_PLANT_TARGET_TEMP_C
        || (temperatures_c[0] - ambient_temp_c).abs() > 8.0
        || powered_peak_index + 1 >= count
        || !last_sample_is_cooling
        || temperatures_c[count - 1] > THERMAL_PLANT_COOL_COMPLETE_TEMP_C
    {
        return None;
    }
    Some(TransientFitData {
        samples: *samples,
        count,
        ambient_temp_c,
        temperatures_c,
        powers_mw,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_fit_scales(
    data: &TransientFitData,
    delay_ticks: u16,
) -> Option<([f32; 3], usize)> {
    let mut scale_sums = [0.0_f32; 3];
    let mut row_count = 0usize;
    for index in 1..data.count {
        if let Some((values, _)) = transient_fit_row(
            &data.samples[..data.count],
            &data.temperatures_c[..data.count],
            &data.powers_mw[..data.count],
            index,
            delay_ticks,
            data.ambient_temp_c,
        ) {
            for column in 0..3 {
                scale_sums[column] += values[column] * values[column];
            }
            row_count += 1;
        }
    }
    if row_count < 12 {
        return None;
    }
    let scales = scale_sums.map(|sum| (sum / row_count as f32).sqrt());
    (!scales
        .iter()
        .any(|scale| !scale.is_finite() || *scale < 1.0e-6))
    .then_some((scales, row_count))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) type TransientFitNormalMatrix = ([[f32; 3]; 3], [f32; 3], f32, usize);

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_fit_normal_matrix(
    data: &TransientFitData,
    delay_ticks: u16,
    scales: [f32; 3],
) -> Option<TransientFitNormalMatrix> {
    let mut normal = [[0.0_f32; 3]; 3];
    let mut rhs = [0.0_f32; 3];
    let mut power_sum_sq = 0.0_f32;
    let mut row_count = 0usize;
    for index in 1..data.count {
        let Some((values, target)) = transient_fit_row(
            &data.samples[..data.count],
            &data.temperatures_c[..data.count],
            &data.powers_mw[..data.count],
            index,
            delay_ticks,
            data.ambient_temp_c,
        ) else {
            continue;
        };
        let normalized = [
            values[0] / scales[0],
            values[1] / scales[1],
            values[2] / scales[2],
        ];
        for row in 0..3 {
            rhs[row] += normalized[row] * target;
            for column in 0..3 {
                normal[row][column] += normalized[row] * normalized[column];
            }
        }
        power_sum_sq += target * target;
        row_count += 1;
    }
    let gram = normal.map(|row| row.map(|value| value / row_count as f32));
    (determinant_3x3(gram).abs() >= 1.0e-5 && power_sum_sq > 1.0).then_some((
        normal,
        rhs,
        power_sum_sq,
        row_count,
    ))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_projection_is_valid(
    projection: ThermalPlantProjection,
    residual: f32,
) -> bool {
    projection.thermal_capacity_mj_per_c.is_finite()
        && projection.convection_mw_per_c.is_finite()
        && projection.radiation_mw_per_k4.is_finite()
        && (100.0..=2_000_000.0).contains(&projection.thermal_capacity_mj_per_c)
        && (0.0..=THERMAL_PLANT_TRANSIENT_MAX_CONVECTION_MW_PER_C)
            .contains(&projection.convection_mw_per_c)
        && (0.0..=THERMAL_PLANT_TRANSIENT_MAX_RADIATION_MW_PER_K4)
            .contains(&projection.radiation_mw_per_k4)
        && residual <= 0.20
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn transient_projection_for_delay(
    data: &TransientFitData,
    delay_ticks: u16,
) -> Option<(ThermalPlantProjection, f32)> {
    let (scales, _row_count) = transient_fit_scales(data, delay_ticks)?;
    let (normal, rhs, power_sum_sq, _row_count) =
        transient_fit_normal_matrix(data, delay_ticks, scales)?;
    let mut best = None;
    for mask in 1..8_u8 {
        let Some(solution) = solve_transient_normal_equations(normal, rhs, mask) else {
            continue;
        };
        if solution.iter().any(|value| *value < -1.0e-4) {
            continue;
        }
        let coefficients = [
            (solution[0] / scales[0]).max(0.0),
            (solution[1] / scales[1]).max(0.0),
            (solution[2] / scales[2]).max(0.0),
        ];
        let mut residual_sum_sq = 0.0_f32;
        for index in 1..data.count {
            let Some((values, target)) = transient_fit_row(
                &data.samples[..data.count],
                &data.temperatures_c[..data.count],
                &data.powers_mw[..data.count],
                index,
                delay_ticks,
                data.ambient_temp_c,
            ) else {
                continue;
            };
            let predicted = coefficients[0] * values[0]
                + coefficients[1] * values[1]
                + coefficients[2] * values[2];
            let residual = target - predicted;
            residual_sum_sq += residual * residual;
        }
        let residual = (residual_sum_sq / power_sum_sq).sqrt();
        let projection = ThermalPlantProjection {
            thermal_capacity_mj_per_c: coefficients[0],
            convection_mw_per_c: coefficients[1],
            radiation_mw_per_k4: coefficients[2],
            transport_delay_ms: u32::from(delay_ticks) * HEATER_CONTROL_INTERVAL_MS as u32,
        };
        if transient_projection_is_valid(projection, residual)
            && best.is_none_or(|(_, current)| residual < current)
        {
            best = Some((projection, residual));
        }
    }
    best
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn best_transient_projection(
    data: &TransientFitData,
) -> Option<(ThermalPlantProjection, f32)> {
    (0..=200_u16)
        .filter_map(|delay_ticks| transient_projection_for_delay(data, delay_ticks))
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fit_thermal_plant_transient(
    transaction_id: u32,
    ambient_raw_rtd_adc_mv: u16,
    samples: &[ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    sample_count: u8,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> Option<(ThermalPlantTransientTransaction, f32)> {
    let data = prepare_transient_fit(
        samples,
        sample_count,
        preview_heater_curve,
        memory_config,
        ambient_raw_rtd_adc_mv,
    )?;
    let (projection, residual) = best_transient_projection(&data)?;
    let transaction = ThermalPlantTransientTransaction {
        transaction_id: transaction_id.max(1),
        ambient_raw_rtd_adc_mv,
        sample_count,
        projection: ThermalPlantProjectionRecord::from_projection(projection),
        samples: data.samples,
    };
    flux_purr_firmware::memory::thermal_plant_transient_transaction_is_complete(&transaction)
        .then_some((transaction, residual))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn rebuild_transient_thermal_plant_for_current_inputs(
    memory_config: &mut MemoryConfig,
) -> bool {
    let Some(previous) = memory_config.thermal_plant_transient_active else {
        return false;
    };

    let rebuilt = has_persisted_heater_resistance_curve(memory_config)
        .then(|| {
            fit_thermal_plant_transient(
                previous.transaction_id,
                previous.ambient_raw_rtd_adc_mv,
                &previous.samples,
                previous.sample_count,
                None,
                memory_config,
            )
            .map(|(transaction, _)| transaction)
        })
        .flatten();
    let rebuilt = if let Some(rebuilt) = rebuilt {
        rebuilt
    } else {
        let mut invalid = previous;
        invalid.projection = ThermalPlantProjectionRecord {
            convection_mw_per_c_bits: 0,
            radiation_mw_per_k4_bits: 0,
            thermal_capacity_mj_per_c_bits: 0,
            transport_delay_ms: 0,
        };
        invalid
    };
    memory_config.thermal_plant_transient_active = Some(rebuilt);
    true
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
pub(crate) fn invalidate_transient_thermal_plant(memory_config: &mut MemoryConfig) {
    if let Some(transaction) = memory_config.thermal_plant_transient_active.as_mut() {
        transaction.projection = ThermalPlantProjectionRecord {
            convection_mw_per_c_bits: 0,
            radiation_mw_per_k4_bits: 0,
            thermal_capacity_mj_per_c_bits: 0,
            transport_delay_ms: 0,
        };
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
pub(crate) struct CalibrationJobUpdateInput {
    pub(crate) latest_rtd_raw_adc_mv: u16,
    pub(crate) latest_vin_raw_adc_mv: u16,
    pub(crate) latest_temp_c: f32,
    pub(crate) pd_current_ma: u16,
    pub(crate) latest_vin_mv: u32,
    pub(crate) heater_duty_percent: u8,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_calibration_job_state_with_workspace(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
    input: CalibrationJobUpdateInput,
) {
    if calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    let Some(job_data) = calibration.job_data else {
        return;
    };

    match job_data {
        CalibrationJobData::VinAdc(job) => {
            update_vin_calibration_job(calibration, memory_config, manual_pps, job, input);
        }
        CalibrationJobData::ThermalPlant => {
            update_thermal_plant_calibration_job(
                calibration,
                memory_config,
                manual_pps,
                thermal_plant_workspace,
                input,
            );
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_vin_calibration_job(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    mut job: CalibrationVinAutoJob,
    input: CalibrationJobUpdateInput,
) {
    let Some(target_ma) = configure_vin_calibration_pps(calibration, manual_pps, &mut job) else {
        return;
    };
    job.target_ma = target_ma;
    update_vin_calibration_stability(&mut job, input.latest_vin_raw_adc_mv, manual_pps);
    if job.stable_ticks >= 2
        && !record_vin_calibration_sample(calibration, memory_config, manual_pps, &mut job)
    {
        return;
    }
    let span_mv = job
        .max_request_mv
        .saturating_sub(job.start_request_mv)
        .max(1);
    let done_mv = job
        .next_request_mv
        .saturating_sub(job.start_request_mv)
        .min(span_mv);
    calibration.job.progress_percent =
        ((u32::from(done_mv) * 100) / u32::from(span_mv)).min(99) as u8;
    calibration.job_data = Some(CalibrationJobData::VinAdc(job));
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn configure_vin_calibration_pps(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationVinAutoJob,
) -> Option<u16> {
    if manual_pps.error.is_some() || !manual_pps.enabled {
        calibration_job_fail(
            calibration,
            manual_pps.error.unwrap_or(ManualPpsError::WriteFailed),
            false,
            manual_pps,
        );
        return None;
    }
    let Some(target_ma) = manual_pps.maximum_pps_current_for_target(job.next_request_mv) else {
        calibration_job_fail(
            calibration,
            ManualPpsError::NoPpsCapability,
            false,
            manual_pps,
        );
        return None;
    };
    if calibration.pps_mv == Some(job.next_request_mv) && calibration.pps_ma == Some(target_ma) {
        return Some(target_ma);
    }
    match manual_pps.enable(
        ManualPpsOwner::Calibration,
        job.next_request_mv,
        Some(target_ma),
    ) {
        Ok(()) => {
            calibration.pps_enabled = true;
            calibration.pps_mv = Some(job.next_request_mv);
            calibration.pps_ma = Some(target_ma);
            job.settle_ticks = 0;
            job.stable_ticks = 0;
            job.last_observed_mv = None;
            Some(target_ma)
        }
        Err(error) => {
            calibration_job_fail(calibration, error, false, manual_pps);
            None
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_vin_calibration_stability(
    job: &mut CalibrationVinAutoJob,
    latest_vin_raw_adc_mv: u16,
    manual_pps: &ManualPpsState,
) {
    let requested_mv = manual_pps.target_mv.unwrap_or(job.next_request_mv);
    let request_locked = (i64::from(requested_mv) - i64::from(job.next_request_mv)).abs() <= 100;
    let raw_adc_stable = job.last_observed_mv.is_some_and(|previous_mv| {
        (i32::from(previous_mv) - i32::from(latest_vin_raw_adc_mv)).abs() <= 8
    });
    let moved_from_previous_sample = job.sample_count == 0
        || job.samples[usize::from(job.sample_count.saturating_sub(1))]
            .map(|sample| {
                (i32::from(sample.observed_mv) - i32::from(latest_vin_raw_adc_mv)).abs()
                    >= i32::from(CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV)
            })
            .unwrap_or(true);
    job.settle_ticks = if request_locked {
        job.settle_ticks.saturating_add(1)
    } else {
        0
    };
    job.stable_ticks = if job.settle_ticks >= 3 && raw_adc_stable && moved_from_previous_sample {
        job.stable_ticks.saturating_add(1)
    } else {
        0
    };
    job.last_observed_mv = Some(latest_vin_raw_adc_mv);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn record_vin_calibration_sample(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationVinAutoJob,
) -> bool {
    let Some(observed_mv) = job.last_observed_mv else {
        return true;
    };
    if usize::from(job.sample_count) >= job.samples.len() {
        calibration_job_fail(calibration, ManualPpsError::WriteFailed, false, manual_pps);
        return false;
    }
    job.samples[usize::from(job.sample_count)] = Some(AdcCalibrationSample {
        observed_mv,
        expected_mv: vin_adc_mv_for_input_mv(u32::from(job.next_request_mv)),
        reference_temp_deci_c: None,
        target_adc_mv: None,
        reference_vin_mv: Some(job.next_request_mv),
    });
    job.sample_count = job.sample_count.saturating_add(1);
    calibration.job.samples_collected = calibration.job.samples_collected.saturating_add(1);
    let next_mv = job.next_request_mv.saturating_add(1_000);
    if next_mv > job.max_request_mv {
        let stored = commit_vin_auto_samples_to_draft(
            memory_config,
            &job.samples,
            usize::from(job.sample_count),
        );
        if stored == 0 {
            calibration_job_fail(calibration, ManualPpsError::WriteFailed, false, manual_pps);
            return false;
        }
        calibration_job_complete(
            calibration,
            CalibrationJobKind::VinAdc,
            calibration.job.samples_collected,
            None,
        );
        return false;
    }
    job.next_request_mv = next_mv;
    job.settle_ticks = 0;
    job.stable_ticks = 0;
    job.last_observed_mv = None;
    calibration.job.next_request_mv = Some(next_mv);
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_thermal_plant_calibration_job(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    workspace: &mut CalibrationThermalPlantWorkspace,
    input: CalibrationJobUpdateInput,
) {
    let Some(job) = workspace.job.as_mut() else {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return;
    };
    if manual_pps.error.is_some()
        || !manual_pps.enabled
        || manual_pps.owner != ManualPpsOwner::Calibration
        || !manual_pps.has_matching_pps_apdo(20_000, job.source_current_ma)
    {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            manual_pps
                .error
                .unwrap_or(ManualPpsError::ThermalPlantSourceUnsupported),
        );
        return;
    }
    let Some(recorded_temp_c) =
        projected_rtd_temperature_c(memory_config, input.latest_rtd_raw_adc_mv)
    else {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return;
    };
    job.elapsed_ticks = job.elapsed_ticks.saturating_add(1);
    let phase_ok = match job.phase {
        ThermalPlantAutoPhase::Ambient => {
            update_thermal_ambient_phase(calibration, manual_pps, job, input, recorded_temp_c)
        }
        ThermalPlantAutoPhase::Heating => {
            update_thermal_heating_phase(calibration, manual_pps, job, input, recorded_temp_c)
        }
        ThermalPlantAutoPhase::Cooling => update_thermal_cooling_phase(
            calibration,
            memory_config,
            manual_pps,
            job,
            input,
            recorded_temp_c,
        ),
    };
    if phase_ok {
        calibration.job.samples_collected = job.sample_count;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fail_thermal_plant_calibration(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    error: ManualPpsError,
) {
    calibration_job_fail(calibration, error, true, manual_pps);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_thermal_ambient_phase(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationThermalPlantAutoJob,
    input: CalibrationJobUpdateInput,
    recorded_temp_c: f32,
) -> bool {
    calibration.heater_enabled = false;
    job.ambient_raw_rtd_adc_mv = if job.idle_samples == 0 {
        input.latest_rtd_raw_adc_mv
    } else {
        ((u32::from(job.ambient_raw_rtd_adc_mv) * u32::from(job.idle_samples)
            + u32::from(input.latest_rtd_raw_adc_mv))
            / (u32::from(job.idle_samples) + 1)) as u16
    };
    job.idle_samples = job.idle_samples.saturating_add(1);
    calibration.job.progress_percent = 1;
    if job.idle_samples < THERMAL_PLANT_AMBIENT_TICKS {
        return true;
    }
    if !record_thermal_plant_transient_sample(
        job,
        input.latest_rtd_raw_adc_mv,
        recorded_temp_c,
        input.latest_vin_mv,
        0,
        true,
    ) {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return false;
    }
    job.phase = ThermalPlantAutoPhase::Heating;
    job.phase_started_tick = job.elapsed_ticks;
    calibration.heater_enabled = true;
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_thermal_heating_phase(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationThermalPlantAutoJob,
    input: CalibrationJobUpdateInput,
    recorded_temp_c: f32,
) -> bool {
    let request_mv = job.source_max_mv;
    if manual_pps.target_mv != Some(request_mv)
        && let Err(error) = manual_pps.enable(
            ManualPpsOwner::Calibration,
            request_mv,
            Some(job.source_current_ma),
        )
    {
        fail_thermal_plant_calibration(calibration, manual_pps, error);
        return false;
    }
    calibration.pps_enabled = true;
    calibration.pps_mv = Some(request_mv);
    calibration.pps_ma = Some(job.source_current_ma);
    calibration.job.next_request_mv = Some(request_mv);
    if input.heater_duty_percent > 0 && (input.latest_vin_mv == 0 || input.pd_current_ma == 0) {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return false;
    }
    if input.heater_duty_percent > 0 {
        observe_thermal_curve(job, input);
    }
    let timed_out =
        job.elapsed_ticks.saturating_sub(job.phase_started_tick) > THERMAL_PLANT_HEAT_TIMEOUT_TICKS;
    if timed_out
        || !record_thermal_plant_transient_sample(
            job,
            input.latest_rtd_raw_adc_mv,
            recorded_temp_c,
            input.latest_vin_mv,
            input.heater_duty_percent,
            input.latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C,
        )
    {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return false;
    }
    calibration.job.progress_percent =
        (2.0 + (input.latest_temp_c / THERMAL_PLANT_TARGET_TEMP_C).clamp(0.0, 1.0) * 58.0) as u8;
    if input.latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C {
        calibration.heater_enabled = false;
        job.phase = ThermalPlantAutoPhase::Cooling;
        job.phase_started_tick = job.elapsed_ticks;
        calibration.job.progress_percent = 60;
    } else {
        calibration.heater_enabled = true;
    }
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn observe_thermal_curve(
    job: &mut CalibrationThermalPlantAutoJob,
    input: CalibrationJobUpdateInput,
) {
    if job.heater_curve.cold_bin.contains(input.latest_temp_c) {
        job.heater_curve.cold_bin.observe_electrical(
            input.latest_temp_c,
            input.latest_rtd_raw_adc_mv,
            input.latest_vin_mv,
            input.pd_current_ma,
        );
    }
    for bin in &mut job.heater_curve.bins {
        if bin.contains(input.latest_temp_c) {
            bin.observe_electrical(
                input.latest_temp_c,
                input.latest_rtd_raw_adc_mv,
                input.latest_vin_mv,
                input.pd_current_ma,
            );
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_thermal_cooling_phase(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationThermalPlantAutoJob,
    input: CalibrationJobUpdateInput,
    recorded_temp_c: f32,
) -> bool {
    calibration.heater_enabled = false;
    let cooling_complete = thermal_plant_cooling_complete(input.latest_temp_c, recorded_temp_c);
    let timed_out =
        job.elapsed_ticks.saturating_sub(job.phase_started_tick) > THERMAL_PLANT_COOL_TIMEOUT_TICKS;
    if timed_out
        || !record_thermal_plant_transient_sample(
            job,
            input.latest_rtd_raw_adc_mv,
            recorded_temp_c,
            input.latest_vin_mv,
            input.heater_duty_percent,
            cooling_complete,
        )
    {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return false;
    }
    calibration.job.progress_percent = (60.0
        + ((THERMAL_PLANT_TARGET_TEMP_C - input.latest_temp_c)
            / (THERMAL_PLANT_TARGET_TEMP_C - THERMAL_PLANT_COOL_COMPLETE_TEMP_C))
            .clamp(0.0, 1.0)
            * 39.0) as u8;
    if cooling_complete {
        complete_thermal_plant_calibration(
            calibration,
            memory_config,
            manual_pps,
            job,
            input.latest_rtd_raw_adc_mv,
        )
    } else {
        true
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn complete_thermal_plant_calibration(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    job: &mut CalibrationThermalPlantAutoJob,
    latest_rtd_raw_adc_mv: u16,
) -> bool {
    if !thermal_plant_curve_samples_ready(&job.heater_curve) {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::HeaterCurveCoverageInsufficient,
        );
        return false;
    }
    let Some(curve) = heater_curve_from_transient_bins(&job.heater_curve.bins) else {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::HeaterCurveCoverageInsufficient,
        );
        return false;
    };
    let Some(raw_observations) = heater_curve_raw_observations_from_transient_bins(
        job.heater_curve.cold_bin,
        &job.heater_curve.bins,
    ) else {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::HeaterCurveCoverageInsufficient,
        );
        return false;
    };
    let transaction_id = (u32::from(job.ambient_raw_rtd_adc_mv) << 16)
        ^ u32::from(latest_rtd_raw_adc_mv)
        ^ job.elapsed_ticks
        ^ 0x5452_4e53;
    let Some((transaction, _residual)) = fit_thermal_plant_transient(
        transaction_id,
        job.ambient_raw_rtd_adc_mv,
        &job.samples,
        job.sample_count,
        Some(&curve),
        memory_config,
    ) else {
        fail_thermal_plant_calibration(
            calibration,
            manual_pps,
            ManualPpsError::ThermalPlantProjectionInvalid,
        );
        return false;
    };
    memory_config.active_heater_curve = curve;
    memory_config.heater_curve_raw_observations = raw_observations;
    memory_config.heater_curve_transaction_id = Some(transaction.transaction_id);
    memory_config.thermal_plant_transient_active = Some(transaction);
    memory_config.thermal_plant_active = None;
    memory_config.sanitize();
    manual_pps.clear();
    calibration.pps_enabled = false;
    calibration.pps_mv = None;
    calibration.pps_ma = None;
    calibration.immediate_heater_disarm_pending = true;
    calibration.mode = CalibrationMode::Off;
    calibration.thermal_plant_completion_disarm_pending = true;
    calibration.job.samples_collected = job.sample_count;
    calibration_job_complete(
        calibration,
        CalibrationJobKind::ThermalPlant,
        job.sample_count,
        None,
    );
    false
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn update_calibration_job_state(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
    input: CalibrationJobUpdateInput,
) {
    update_calibration_job_state_with_workspace(
        calibration,
        memory_config,
        manual_pps,
        thermal_plant_workspace,
        input,
    );
}

#[cfg(test)]
pub(crate) fn update_calibration_job_state(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    input: CalibrationJobUpdateInput,
) {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        update_calibration_job_state_with_workspace(
            calibration,
            memory_config,
            manual_pps,
            &mut workspace.borrow_mut(),
            input,
        );
    });
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_calibration_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) -> UsbFrame {
    let previous_adc_calibration = memory_config.adc_calibration;
    let result = apply_calibration_config_command(
        config,
        memory_config,
        latest_rtd_raw_adc_mv,
        latest_vin_raw_adc_mv,
    );

    if let Err(error) = result {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(*error),
        };
    }
    memory_config.sanitize();
    if memory_config.adc_calibration != previous_adc_calibration {
        rebuild_transient_thermal_plant_for_current_inputs(memory_config);
    }
    usb_response(
        request_id,
        UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn apply_calibration_config_command(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) -> Result<usize, Box<ApiError>> {
    match config.op {
        CalibrationConfigOp::Capture => capture_calibration_sample(
            config,
            memory_config,
            latest_rtd_raw_adc_mv,
            latest_vin_raw_adc_mv,
        ),
        CalibrationConfigOp::Delete => delete_calibration_sample(config, memory_config),
        CalibrationConfigOp::Clear => clear_calibration_channel(config, memory_config),
        CalibrationConfigOp::Import => import_calibration_command(config, memory_config),
        CalibrationConfigOp::SetActiveSlot => set_calibration_slot(config, memory_config),
        CalibrationConfigOp::SetSlotFit => set_calibration_fit(config, memory_config),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn capture_calibration_sample(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) -> Result<usize, Box<ApiError>> {
    let channel = config.channel.ok_or(Box::new(ApiError::new(
        "calibration_channel_required",
        "Calibration capture requires a channel.",
        false,
    )))?;
    let observed_mv = config.observed_mv.unwrap_or(match channel {
        CalibrationChannelWire::RtdAdc => latest_rtd_raw_adc_mv,
        CalibrationChannelWire::VinAdc => latest_vin_raw_adc_mv,
    });
    let expected_mv =
        expected_calibration_adc_mv(&config, channel).ok_or(Box::new(ApiError::new(
            "calibration_reference_required",
            "Calibration capture requires a valid physical reference.",
            false,
        )))?;
    let reference_temp_deci_c = config.reference_temp_c.map(|temp_c| {
        let scaled = if temp_c >= 0.0 {
            temp_c * 10.0 + 0.5
        } else {
            temp_c * 10.0 - 0.5
        };
        (scaled as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
    });
    let sample = AdcCalibrationSample {
        observed_mv,
        expected_mv,
        reference_temp_deci_c,
        target_adc_mv: config
            .target_adc_mv
            .filter(|_| channel == CalibrationChannelWire::RtdAdc),
        reference_vin_mv: config
            .reference_vin_mv
            .and_then(|millivolts| u16::try_from(millivolts).ok())
            .filter(|_| channel == CalibrationChannelWire::VinAdc),
    };
    memory_config
        .adc_calibration
        .channel_mut(channel.as_memory_channel())
        .insert(sample)
        .ok_or(Box::new(ApiError::new(
            "calibration_samples_full",
            "Calibration channel already has 8 samples.",
            false,
        )))
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn delete_calibration_sample(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
) -> Result<usize, Box<ApiError>> {
    let channel = config.channel.ok_or(Box::new(ApiError::new(
        "calibration_channel_required",
        "Calibration delete requires a channel.",
        false,
    )))?;
    let index = config.sample_index.ok_or(Box::new(ApiError::new(
        "calibration_index_required",
        "Calibration delete requires sampleIndex.",
        false,
    )))?;
    if memory_config
        .adc_calibration
        .channel_mut(channel.as_memory_channel())
        .delete(index)
    {
        Ok(index)
    } else {
        Err(Box::new(ApiError::new(
            "calibration_sample_not_found",
            "Calibration sample index was not present.",
            false,
        )))
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn clear_calibration_channel(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
) -> Result<usize, Box<ApiError>> {
    let channel = config.channel.ok_or(Box::new(ApiError::new(
        "calibration_channel_required",
        "Calibration clear requires a channel.",
        false,
    )))?;
    memory_config
        .adc_calibration
        .channel_mut(channel.as_memory_channel())
        .clear();
    Ok(0)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn import_calibration_command(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
) -> Result<usize, Box<ApiError>> {
    let state = config.state.ok_or(Box::new(ApiError::new(
        "calibration_state_required",
        "Calibration import requires a state.",
        false,
    )))?;
    import_calibration_state(memory_config, state);
    Ok(0)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn set_calibration_slot(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
) -> Result<usize, Box<ApiError>> {
    let channel = config.channel.ok_or(Box::new(ApiError::new(
        "calibration_channel_required",
        "Calibration slot switch requires a channel.",
        false,
    )))?;
    let slot = config.slot.ok_or(Box::new(ApiError::new(
        "calibration_slot_required",
        "Calibration slot switch requires a slot.",
        false,
    )))?;
    memory_config
        .adc_calibration
        .channel_mut(channel.as_memory_channel())
        .active_slot = slot.into();
    Ok(0)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn set_calibration_fit(
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
) -> Result<usize, Box<ApiError>> {
    let channel = config.channel.ok_or(Box::new(ApiError::new(
        "calibration_channel_required",
        "Calibration slot fit update requires a channel.",
        false,
    )))?;
    let slot = config.slot.ok_or(Box::new(ApiError::new(
        "calibration_slot_required",
        "Calibration slot fit update requires a slot.",
        false,
    )))?;
    let fit = config.fit.ok_or(Box::new(ApiError::new(
        "calibration_fit_required",
        "Calibration slot fit update requires gain and offset.",
        false,
    )))?;
    *memory_config
        .adc_calibration
        .channel_mut(channel.as_memory_channel())
        .slot_fit_mut(slot.into()) = fit.to_memory();
    Ok(0)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn import_calibration_state(
    memory_config: &mut MemoryConfig,
    state: CalibrationStateWire,
) {
    import_calibration_channel_state(
        &mut memory_config.adc_calibration.rtd,
        state.rtd_adc.samples,
        state.rtd_adc.slots.a,
        state.rtd_adc.slots.b,
        state.rtd_adc.active_slot,
    );
    import_calibration_channel_state(
        &mut memory_config.adc_calibration.vin,
        state.vin_adc.samples,
        state.vin_adc.slots.a,
        state.vin_adc.slots.b,
        state.vin_adc.active_slot,
    );
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn import_calibration_channel_state(
    channel: &mut flux_purr_firmware::memory::AdcCalibrationChannelConfig,
    samples: [Option<CalibrationSampleWire>; ADC_CALIBRATION_MAX_SAMPLES],
    slot_a: CalibrationSlotFitWire,
    slot_b: CalibrationSlotFitWire,
    active_slot: CalibrationSlotIdWire,
) {
    channel.samples = samples_from_wire(samples);
    channel.slots.a = slot_a.to_memory();
    channel.slots.b = slot_b.to_memory();
    channel.active_slot = active_slot.into();
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_heater_curve_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: HeaterCurveConfigCommand,
    memory_config: &MemoryConfig,
    preview_heater_curve: &mut Option<HeaterCurvePreview>,
) -> UsbFrame {
    match config.op {
        HeaterCurveConfigOp::Preview => {
            let Some(package) = config.package else {
                return usb_error_response(
                    request_id,
                    "heater_curve_package_required",
                    "Heater curve preview requires a package.",
                );
            };
            let raw_observations = package.raw_observations_to_memory();
            let mut curve = package.to_memory();
            curve.points.sort_unstable_by_key(|point| {
                point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX)
            });
            *preview_heater_curve = Some(HeaterCurvePreview {
                curve,
                raw_observations,
            });
        }
        HeaterCurveConfigOp::ClearPreview => {
            *preview_heater_curve = None;
        }
    }
    usb_response(
        request_id,
        UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
            memory_config,
            preview_heater_curve
                .as_ref()
                .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
        )),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_calibration_job_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: CalibrationJobCommandWire,
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> UsbFrame {
    match command.op {
        CalibrationJobOpWire::Cancel => {
            calibration_job_canceled(calibration, manual_pps);
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration).job,
                ),
            )
        }
        CalibrationJobOpWire::Start => {
            let Some(kind) = command.kind.map(CalibrationJobKind::from) else {
                return usb_error_response(
                    request_id,
                    "calibration_job_kind_required",
                    "Calibration auto job requires a job kind.",
                );
            };
            if let Err(error) = calibration_job_start(
                calibration,
                kind,
                memory_config,
                manual_pps,
                thermal_plant_workspace,
            ) {
                return usb_error_response(request_id, error.code(), error.message());
            }
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration).job,
                ),
            )
        }
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn expected_calibration_adc_mv(
    config: &CalibrationConfigCommand,
    channel: CalibrationChannelWire,
) -> Option<u16> {
    if let Some(expected_mv) = config.expected_mv {
        return Some(expected_mv);
    }

    match channel {
        CalibrationChannelWire::RtdAdc => config.target_adc_mv,
        CalibrationChannelWire::VinAdc => config.reference_vin_mv.map(vin_adc_mv_for_input_mv),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_write_frame(
    usb: &mut RawUsbSerialJtag,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    usb_write_frame_to(usb, frame, tx_buf);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn usb_write_response_frame(
    usb: &mut RawUsbSerialJtag,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    if let Ok(line) = write_usb_frame(frame, tx_buf) {
        let _ = usb_write_response_bytes(usb, line.as_bytes()).await;
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn usb_write_frame_to<T: UsbControlTx>(
    tx: &mut T,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    if let Ok(line) = write_usb_frame(frame, tx_buf) {
        let _ = usb_write_bytes_bounded(tx, line.as_bytes());
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn usb_write_response_bytes<T: UsbControlTx>(tx: &mut T, bytes: &[u8]) -> bool {
    let deadline = Instant::now() + Duration::from_millis(USB_CONTROL_RESPONSE_TIMEOUT_MS);
    let mut writer = UsbResponseWriter::new(bytes);

    while !writer.is_complete() {
        if Instant::now() >= deadline {
            return false;
        }
        match writer.step(tx) {
            Ok(true) => return true,
            Ok(false) | Err(UsbTxError::WouldBlock) => {
                embassy_futures::yield_now().await;
            }
            Err(UsbTxError::Other) => return false,
        }
    }

    true
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) struct UsbResponseWriter<'a> {
    bytes: &'a [u8],
    offset: usize,
    packet_len: usize,
    flush_pending: bool,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
impl<'a> UsbResponseWriter<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            packet_len: 0,
            flush_pending: false,
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.offset == self.bytes.len() && self.packet_len == 0 && !self.flush_pending
    }

    /// Performs at most one non-blocking endpoint operation. The async caller
    /// yields after every step so an always-ready endpoint cannot monopolize
    /// the executor while a large response is in flight.
    pub(crate) fn step<T: UsbControlTx>(&mut self, tx: &mut T) -> Result<bool, UsbTxError> {
        if self.flush_pending {
            match tx.flush_tx_nb() {
                Ok(()) => {
                    self.packet_len = 0;
                    self.flush_pending = false;
                    Ok(self.is_complete())
                }
                Err(error) => Err(error),
            }
        } else if self.offset < self.bytes.len() {
            match tx.write_byte_nb(self.bytes[self.offset]) {
                Ok(()) => {
                    self.offset += 1;
                    self.packet_len += 1;
                    self.flush_pending = self.packet_len == USB_CONTROL_TX_PACKET_LEN;
                    Ok(false)
                }
                Err(error) => Err(error),
            }
        } else if self.packet_len != 0 {
            self.flush_pending = true;
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

#[cfg(test)]
pub(crate) fn usb_write_response_frame_to<T: UsbControlTx>(
    tx: &mut T,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    if let Ok(line) = write_usb_frame(frame, tx_buf) {
        let _ = usb_write_bytes_bounded(tx, line.as_bytes());
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UsbTxError {
    WouldBlock,
    #[cfg_attr(all(target_arch = "xtensa", feature = "web_serial"), allow(dead_code))]
    Other,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) trait UsbControlTx {
    fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError>;
    fn flush_tx_nb(&mut self) -> Result<(), UsbTxError>;
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
impl UsbControlTx for RawUsbSerialJtag {
    fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
        self.inner.write_byte_nb(byte).map_err(|err| match err {
            nb::Error::WouldBlock => UsbTxError::WouldBlock,
            nb::Error::Other(_) => UsbTxError::Other,
        })
    }

    fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
        self.inner.flush_tx_nb().map_err(|err| match err {
            nb::Error::WouldBlock => UsbTxError::WouldBlock,
            nb::Error::Other(_) => UsbTxError::Other,
        })
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) trait PersistenceLogSink {
    fn write_line(&mut self, line: &[u8]);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
impl PersistenceLogSink for RawUsbSerialJtag {
    fn write_line(&mut self, line: &[u8]) {
        // Persistence diagnostics are best-effort. They must never wait for a
        // host endpoint while the normal executor owns UI and safety work.
        let _ = usb_write_bytes_bounded(self, line);
    }
}

#[cfg(all(target_arch = "xtensa", not(feature = "web_serial")))]
pub(crate) struct NoopPersistenceLogSink;

#[cfg(all(target_arch = "xtensa", not(feature = "web_serial")))]
impl PersistenceLogSink for NoopPersistenceLogSink {
    fn write_line(&mut self, _line: &[u8]) {}
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_write_bytes_bounded<T: UsbControlTx>(tx: &mut T, bytes: &[u8]) -> bool {
    // Diagnostics remain best-effort. Runtime JSONL responses use a separate
    // yielding packet writer so they cannot busy-wait an executor turn.
    let mut packet_len = 0;
    for byte in bytes {
        if !usb_write_byte_bounded(tx, *byte, &mut packet_len) {
            return false;
        }
    }

    usb_flush_tx_bounded(tx)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_write_byte_bounded<T: UsbControlTx>(
    tx: &mut T,
    byte: u8,
    packet_len: &mut usize,
) -> bool {
    match tx.write_byte_nb(byte) {
        Ok(()) => {
            *packet_len += 1;
            usb_flush_full_packet_if_needed(tx, packet_len)
        }
        Err(_) => false,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_flush_full_packet_if_needed<T: UsbControlTx>(
    tx: &mut T,
    packet_len: &mut usize,
) -> bool {
    if *packet_len < USB_CONTROL_TX_PACKET_LEN {
        return true;
    }
    if !usb_flush_tx_bounded(tx) {
        return false;
    }
    *packet_len = 0;
    true
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_flush_tx_bounded<T: UsbControlTx>(tx: &mut T) -> bool {
    tx.flush_tx_nb().is_ok()
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    result: UsbResponsePayload,
) -> UsbFrame {
    UsbFrame::Response {
        request_id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn hardware_identity() -> Identity {
    #[cfg(target_arch = "xtensa")]
    {
        Identity::firmware_from_mac(esp_hal::efuse::Efuse::mac_address())
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        Identity::firmware_from_mac([0xa0, 0xf2, 0x62, 0xf2, 0x0d, 0x6c])
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_error_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
    message: &'static str,
) -> UsbFrame {
    usb_error_response_with_retryable(request_id, code, message, false)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_error_response_with_retryable(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
    message: &'static str,
    retryable: bool,
) -> UsbFrame {
    UsbFrame::Response {
        request_id,
        ok: false,
        result: None,
        error: Some(ApiError::new(code, message, retryable)),
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_early_response(line: &str, memory_config: &MemoryConfig) -> UsbFrame {
    match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => {
            usb_early_request_response(request_id, op, memory_config)
        }
        Ok(UsbFrame::WifiConfig { request_id, .. })
        | Ok(UsbFrame::RuntimeConfig { request_id, .. })
        | Ok(UsbFrame::CalibrationJob { request_id, .. })
        | Ok(UsbFrame::CalibrationConfig { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Configuration writes are not available until hardware initialization completes.",
            true,
        ),
        Ok(UsbFrame::ThermalPlantRun { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Thermal-model run snapshots are not available until hardware initialization completes.",
            true,
        ),
        Ok(UsbFrame::Response { request_id, .. }) => usb_error_response(
            request_id,
            "unsupported_frame",
            "Host response frames are ignored.",
        ),
        Ok(_) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
        },
        Err(UsbFrameError::MalformedJson) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
        },
        Err(UsbFrameError::OutputTooSmall) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new(
                "output_too_small",
                "USB JSONL frame exceeded buffer.",
                false,
            ),
        },
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_early_request_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    op: UsbRequestOp,
    memory_config: &MemoryConfig,
) -> UsbFrame {
    match op {
        UsbRequestOp::GetIdentity => usb_response(
            request_id,
            UsbResponsePayload::Identity(Box::new(hardware_identity())),
        ),
        UsbRequestOp::GetInstallStatus => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Install status is unavailable until EEPROM restoration completes.",
            true,
        ),
        UsbRequestOp::CompleteSetup | UsbRequestOp::ResetPersistence => {
            usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "Persistence changes are unavailable until EEPROM restoration completes.",
                true,
            )
        }
        // The boot-time memory argument is still the zero-value placeholder
        // until the main loop has completed EEPROM restoration. Never
        // expose it as a network snapshot: a configured device would appear
        // transiently disabled or connected with no address and the host
        // could persist that false state. The devd read path retries this
        // explicit startup boundary until the runtime owns the snapshot.
        UsbRequestOp::GetNetwork => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Network status is not available until EEPROM and WiFi initialization completes.",
            true,
        ),
        UsbRequestOp::GetCalibration => usb_response(
            request_id,
            UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
        ),
        UsbRequestOp::GetCalibrationJob => usb_response(
            request_id,
            UsbResponsePayload::CalibrationJob(CalibrationRuntimeStateWire::default().job),
        ),
        UsbRequestOp::GetHeaterCurve => usb_response(
            request_id,
            UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(memory_config, None)),
        ),
        UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
        UsbRequestOp::GetLanPairingCode => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "LAN pairing code is not available until runtime initialization completes.",
            true,
        ),
        UsbRequestOp::OpenLanPairingWindow | UsbRequestOp::CloseLanPairingWindow => {
            usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "LAN pairing window is not available until runtime initialization completes.",
                true,
            )
        }
        UsbRequestOp::ClearLanPairingToken => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "LAN pairing reset is not available until runtime initialization completes.",
            true,
        ),
        UsbRequestOp::GetStatus => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Runtime status is not available until hardware initialization completes.",
            true,
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn poll_usb_early_control(
    usb: &mut RawUsbSerialJtag,
    rx_line: &mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    memory_config: &MemoryConfig,
) {
    let mut bytes_processed = 0_u16;
    loop {
        if bytes_processed >= PD_RUNTIME_USB_BYTE_BUDGET {
            break;
        }
        match usb.read_byte() {
            Ok(b'\n') => {
                bytes_processed = bytes_processed.saturating_add(1);
                let response = usb_early_response(rx_line.as_str(), memory_config);
                usb_write_response_frame(usb, &response, tx_buf).await;
                rx_line.clear();
            }
            Ok(b'\r') => {
                bytes_processed = bytes_processed.saturating_add(1);
            }
            Ok(byte) => {
                bytes_processed = bytes_processed.saturating_add(1);
                if rx_line.push(char::from(byte)).is_err() {
                    rx_line.clear();
                }
            }
            Err(nb::Error::WouldBlock) => break,
            Err(_) => break,
        }
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn append_usb_recovery_byte(
    rx_line: &mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    byte: u8,
) {
    if rx_line.push(char::from(byte)).is_err() {
        rx_line.clear();
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn run_usb_recovery_control_loop(
    usb: &mut RawUsbSerialJtag,
    rx_line: &mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    memory_config: &MemoryConfig,
    status_light_state: StatusLightState,
    phase: UsbRecoveryPhase,
) -> ! {
    set_status_light_state(status_light_state);
    let mut elapsed_ms = 0_u64;
    loop {
        loop {
            match usb.read_byte() {
                Ok(b'\n') => {
                    let response = usb_recovery_response_for_phase(
                        rx_line.as_str(),
                        memory_config,
                        elapsed_ms,
                        phase,
                    );
                    usb_write_response_frame(usb, &response, tx_buf).await;
                    rx_line.clear();
                }
                Ok(b'\r') => {}
                Ok(byte) => {
                    append_usb_recovery_byte(rx_line, byte);
                }
                Err(nb::Error::WouldBlock) => break,
                Err(_) => break,
            }
        }
        EmbassyTimer::after_millis(20).await;
        elapsed_ms = elapsed_ms.saturating_add(20);
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UsbRecoveryPhase {
    BeforePersistentState,
    RuntimeFault,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_recovery_response_for_phase(
    line: &str,
    memory_config: &MemoryConfig,
    elapsed_ms: u64,
    phase: UsbRecoveryPhase,
) -> UsbFrame {
    match phase {
        // Persistence restoration and Wi-Fi initialization have not happened,
        // so only the early USB contract may answer. In particular, it keeps
        // network and runtime status behind the retryable startup boundary.
        UsbRecoveryPhase::BeforePersistentState => usb_early_response(line, memory_config),
        UsbRecoveryPhase::RuntimeFault => usb_recovery_response(line, memory_config, elapsed_ms),
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_recovery_status(
    memory_config: &MemoryConfig,
    elapsed_ms: u64,
) -> Box<ControlPlaneStatus> {
    let mut status = ControlPlaneStatus::boxed_from_device_status(
        DeviceStatus {
            mode: DeviceMode::Fault,
            voltage_mv: 0,
            current_ma: 0,
            board_temp_centi: -100,
            rtd_raw_adc_mv: 0,
            vin_raw_adc_mv: 0,
            pd_request_mv: DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
            pd_contract_mv: 0,
            pd_state: PdState::Fault,
            heater_output_percent: 0,
            heater_physical_output_percent: 0,
            fan_enabled: false,
            fan_pwm_permille: FAN_FULL_SPEED_PWM_PERMILLE,
            frontpanel_key: None,
        },
        memory_config,
        (elapsed_ms / 1_000).min(u64::from(u32::MAX)) as u32,
        network_from_memory(memory_config),
    );
    status.calibration = CalibrationRuntimeStateWire::default();
    status
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_recovery_response(
    line: &str,
    memory_config: &MemoryConfig,
    elapsed_ms: u64,
) -> UsbFrame {
    match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => {
            usb_recovery_request_response(request_id, op, memory_config, elapsed_ms)
        }
        Ok(UsbFrame::WifiConfig { request_id, .. })
        | Ok(UsbFrame::RuntimeConfig { request_id, .. })
        | Ok(UsbFrame::CalibrationConfig { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "Runtime writes are unavailable because hardware bring-up did not complete.",
            true,
        ),
        Ok(UsbFrame::ThermalPlantRun { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "Thermal-model run snapshots are unavailable because hardware bring-up did not complete.",
            true,
        ),
        Ok(UsbFrame::Response { request_id, .. }) => usb_error_response(
            request_id,
            "unsupported_frame",
            "Host response frames are ignored.",
        ),
        Ok(_) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
        },
        Err(UsbFrameError::MalformedJson) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
        },
        Err(UsbFrameError::OutputTooSmall) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new(
                "output_too_small",
                "USB JSONL frame exceeded buffer.",
                false,
            ),
        },
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn usb_recovery_request_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    op: UsbRequestOp,
    memory_config: &MemoryConfig,
    elapsed_ms: u64,
) -> UsbFrame {
    match op {
        UsbRequestOp::GetIdentity => usb_response(
            request_id,
            UsbResponsePayload::Identity(Box::new(hardware_identity())),
        ),
        UsbRequestOp::GetInstallStatus => usb_response(
            request_id,
            UsbResponsePayload::InstallStatus(InstallStatus::from_runtime(
                InstallRuntimeSnapshot {
                    config: memory_config,
                    persistence_source: "defaults",
                    record_state: "incompatible",
                    record_sequence: 0,
                    sensor_ready: false,
                    heater_fault_latched: true,
                    persistence_locked: true,
                    last_persistence_fault: None,
                    persistence_fault_attention_pending: true,
                },
            )),
        ),
        UsbRequestOp::CompleteSetup | UsbRequestOp::ResetPersistence => usb_error_response(
            request_id,
            "hardware_bringup_failed",
            "Persistence changes are unavailable because hardware bring-up failed.",
        ),
        UsbRequestOp::GetNetwork => usb_response(
            request_id,
            UsbResponsePayload::Network(network_from_memory(memory_config)),
        ),
        UsbRequestOp::GetStatus => usb_response(
            request_id,
            UsbResponsePayload::Status(usb_recovery_status(memory_config, elapsed_ms)),
        ),
        UsbRequestOp::GetCalibration => usb_response(
            request_id,
            UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
        ),
        UsbRequestOp::GetCalibrationJob => usb_response(
            request_id,
            UsbResponsePayload::CalibrationJob(CalibrationJobStateWire::default()),
        ),
        UsbRequestOp::GetHeaterCurve => usb_response(
            request_id,
            UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(memory_config, None)),
        ),
        UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
        UsbRequestOp::GetLanPairingCode => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "LAN pairing code is unavailable because hardware bring-up did not complete.",
            true,
        ),
        UsbRequestOp::OpenLanPairingWindow | UsbRequestOp::CloseLanPairingWindow => {
            usb_error_response_with_retryable(
                request_id,
                "hardware_bringup_failed",
                "LAN pairing window is unavailable because hardware bring-up did not complete.",
                true,
            )
        }
        UsbRequestOp::ClearLanPairingToken => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "LAN pairing reset is unavailable because hardware bring-up did not complete.",
            true,
        ),
    }
}
