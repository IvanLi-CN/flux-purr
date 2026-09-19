#[allow(unused_imports)]
use super::*;

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) struct ControlLineContext<'a, 'i, 'e> {
    pub(crate) controller: &'a mut FrontPanelInputController,
    pub(crate) ui_state: &'a mut FrontPanelUiState,
    pub(crate) memory_config: &'a mut MemoryConfig,
    pub(crate) last_persisted_memory_config: &'a mut MemoryConfig,
    pub(crate) preview_heater_curve: &'a mut Option<HeaterCurvePreview>,
    pub(crate) memory_commit_due_ms: &'a mut Option<u64>,
    pub(crate) memory_sequence: &'a mut u32,
    pub(crate) persistence_source: &'static str,
    pub(crate) persistence_record_state: &'static str,
    pub(crate) eeprom_i2c: &'a mut I2c<'i>,
    pub(crate) pd_controller: ControllerKind,
    pub(crate) pd_port: &'e PdPort,
    pub(crate) calibration_runtime_state: &'a mut CalibrationRuntimeState,
    pub(crate) thermal_plant_workspace: &'a mut CalibrationThermalPlantWorkspace,
    pub(crate) elapsed_ms: u64,
    pub(crate) last_pd_observation: Option<PdStatusObservation>,
    pub(crate) heater_power_backend: &'a mut HeaterPowerBackend,
    pub(crate) heater_controller: &'a mut HeaterController,
    pub(crate) pid_snapshot: HeaterPidSnapshot,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) fan_command: FanHardwareCommand,
    pub(crate) current_rtd_fault: Option<HeaterFaultReason>,
    pub(crate) overtemp_attention_acknowledged: &'a mut bool,
    pub(crate) attention_pending_after_fault_clear: &'a mut bool,
    pub(crate) overtemp_forced_fan_active: &'a mut bool,
    pub(crate) next_attention_reminder_ms: &'a mut Option<u64>,
    pub(crate) buzzer: &'a mut BuzzerRuntime,
    pub(crate) thermal_control_profile_preview: &'a mut Option<ThermalControlProfile>,
    pub(crate) last_raw_state: FrontPanelRawState,
    pub(crate) latest_status_temp_c: f32,
    pub(crate) latest_control_temp_c: f32,
    pub(crate) control_measurement_guarded: bool,
    pub(crate) latest_rtd_raw_adc_mv: u16,
    pub(crate) latest_rtd_raw_adc_min_mv: u16,
    pub(crate) latest_rtd_raw_adc_max_mv: u16,
    pub(crate) latest_vin_raw_adc_mv: u16,
    pub(crate) latest_vin_mv: u32,
    pub(crate) last_heater_duty: u8,
    pub(crate) heater_control_timing: HeaterControlTiming,
    pub(crate) persistence_log_sink: &'a mut dyn PersistenceLogSink,
    pub(crate) record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_control_line(
    line: &str,
    mut context: ControlLineContext<'_, '_, '_>,
) -> (bool, UsbFrame) {
    let active_profile = active_thermal_control_profile(
        context.memory_config,
        *context.thermal_control_profile_preview,
        context.manual_pps,
    );
    let (needs_redraw, response) = dispatch_control_frame(&mut context, line, active_profile).await;
    (needs_redraw, response)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn control_runtime_status_context(
    context: &ControlLineContext<'_, '_, '_>,
    active_profile: Option<ThermalControlProfile>,
    manual_pps: ManualPpsState,
    heater_fault_latched: Option<HeaterFaultReason>,
    attention_pending: bool,
) -> UsbRuntimeStatusContext {
    UsbRuntimeStatusContext {
        elapsed_ms: context.elapsed_ms,
        pd_controller: context.pd_controller,
        last_pd_observation: context.last_pd_observation,
        heater_power_backend: *context.heater_power_backend,
        pid_snapshot: context.pid_snapshot,
        heater_control_timing: context.heater_control_timing,
        heater_physical_output_percent: context.last_heater_duty,
        manual_pps,
        fan_command: context.fan_command,
        current_rtd_fault: context.current_rtd_fault,
        heater_fault_latched,
        attention_pending_after_fault_clear: attention_pending,
        thermal_control_profile_preview: context.thermal_control_profile_preview.is_some(),
        active_thermal_control_profile: active_profile,
        last_raw_state: context.last_raw_state,
        latest_status_temp_c: context.latest_status_temp_c,
        latest_control_temp_c: context.latest_control_temp_c,
        control_measurement_guarded: context.control_measurement_guarded,
        latest_rtd_raw_adc_mv: context.latest_rtd_raw_adc_mv,
        latest_rtd_raw_adc_min_mv: context.latest_rtd_raw_adc_min_mv,
        latest_rtd_raw_adc_max_mv: context.latest_rtd_raw_adc_max_mv,
        latest_vin_raw_adc_mv: context.latest_vin_raw_adc_mv,
        vin_mv: context.latest_vin_mv,
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn dispatch_control_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    line: &str,
    active_profile: Option<ThermalControlProfile>,
) -> (bool, UsbFrame) {
    match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => {
            process_request_frame(context, request_id, op, active_profile).await
        }
        Ok(UsbFrame::WifiConfig { request_id, config }) => {
            process_wifi_config_frame(context, request_id, config).await
        }
        Ok(UsbFrame::RuntimeConfig { request_id, config }) => {
            process_runtime_config_frame(context, request_id, config, active_profile)
        }
        #[cfg(feature = "buzzer-test")]
        Ok(UsbFrame::BuzzerTest {
            request_id,
            command,
        }) => process_buzzer_test_frame(context, request_id, command),
        Ok(UsbFrame::CalibrationConfig { request_id, config }) => {
            process_calibration_config_frame(context, request_id, config).await
        }
        Ok(UsbFrame::CalibrationJob {
            request_id,
            command,
        }) => process_calibration_job_frame(context, request_id, command),
        Ok(UsbFrame::ThermalPlantRun {
            request_id,
            after_sample,
        }) => process_thermal_plant_run_frame(context, request_id, after_sample),
        Ok(UsbFrame::HeaterCurveConfig { request_id, config }) => {
            process_heater_curve_config_frame(context, request_id, config)
        }
        Ok(UsbFrame::HeaterCurveSave { request_id }) => {
            process_heater_curve_save_frame(context, request_id).await
        }
        Ok(UsbFrame::EepromMaintenance {
            request_id,
            command,
        }) => process_eeprom_maintenance_frame(context, request_id, command).await,
        Ok(UsbFrame::Response { request_id, .. }) => (
            false,
            usb_error_response(
                request_id,
                "unsupported_frame",
                "Host response frames are ignored.",
            ),
        ),
        Ok(_) => (
            false,
            UsbFrame::Error {
                request_id: None,
                error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
            },
        ),
        Err(UsbFrameError::MalformedJson) => (
            false,
            UsbFrame::Error {
                request_id: None,
                error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
            },
        ),
        Err(UsbFrameError::OutputTooSmall) => (
            false,
            UsbFrame::Error {
                request_id: None,
                error: ApiError::new(
                    "output_too_small",
                    "USB JSONL frame exceeded buffer.",
                    false,
                ),
            },
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_request_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    op: UsbRequestOp,
    active_profile: Option<ThermalControlProfile>,
) -> (bool, UsbFrame) {
    match op {
        UsbRequestOp::GetIdentity => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::Identity(Box::new(hardware_identity())),
            ),
        ),
        UsbRequestOp::GetInstallStatus => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::InstallStatus(InstallStatus::from_runtime(
                    InstallRuntimeSnapshot {
                        config: context.memory_config,
                        persistence_source: context.persistence_source,
                        record_state: context.persistence_record_state,
                        record_sequence: *context.memory_sequence,
                        sensor_ready: context.current_rtd_fault.is_none()
                            && context.latest_status_temp_c.is_finite(),
                        heater_fault_latched: context.heater_controller.fault_latched().is_some(),
                        persistence_locked: context.ui_state.persistence_locked(),
                        last_persistence_fault: context.ui_state.persistence_fault.clone(),
                        persistence_fault_attention_pending: context
                            .ui_state
                            .persistence_fault_attention_pending,
                    },
                )),
            ),
        ),
        UsbRequestOp::CompleteSetup => process_complete_setup(context, request_id),
        UsbRequestOp::ResetPersistence => process_reset_persistence(context, request_id),
        UsbRequestOp::GetNetwork
        | UsbRequestOp::GetLanPairingCode
        | UsbRequestOp::OpenLanPairingWindow
        | UsbRequestOp::CloseLanPairingWindow
        | UsbRequestOp::ClearLanPairingToken => {
            process_network_request(context, request_id, op).await
        }
        UsbRequestOp::GetStatus => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::Status(usb_runtime_status(
                    context.ui_state,
                    context.memory_config,
                    context.calibration_runtime_state,
                    control_runtime_status_context(
                        context,
                        active_profile,
                        *context.manual_pps,
                        context.heater_controller.fault_latched(),
                        *context.attention_pending_after_fault_clear,
                    ),
                )),
            ),
        ),
        UsbRequestOp::GetCalibration => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(
                    context.memory_config,
                )),
            ),
        ),
        UsbRequestOp::GetCalibrationJob => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(context.calibration_runtime_state).job,
                ),
            ),
        ),
        UsbRequestOp::GetHeaterCurve => (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    context.memory_config,
                    context
                        .preview_heater_curve
                        .as_ref()
                        .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
                )),
            ),
        ),
        UsbRequestOp::SetLogLevel => (false, usb_response(request_id, UsbResponsePayload::Ack)),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_complete_setup(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; setup cannot be completed.",
            ),
        );
    }
    let sensor_ready =
        context.current_rtd_fault.is_none() && context.latest_status_temp_c.is_finite();
    let calibration_ready = flux_purr_firmware::memory::adc_calibration_fit(
        &context.memory_config.adc_calibration,
        flux_purr_firmware::memory::AdcCalibrationChannel::Rtd,
    )
    .sample_count
        >= 2
        && flux_purr_firmware::memory::adc_calibration_fit(
            &context.memory_config.adc_calibration,
            flux_purr_firmware::memory::AdcCalibrationChannel::Vin,
        )
        .sample_count
            >= 2
        && context
            .memory_config
            .active_heater_curve
            .points
            .iter()
            .flatten()
            .count()
            >= 2;
    let result = context
        .memory_config
        .complete_setup(sensor_ready, calibration_ready);
    let response = match result {
        Ok(()) => {
            *context.memory_commit_due_ms =
                Some(context.elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            usb_response(request_id, UsbResponsePayload::Ack)
        }
        Err(flux_purr_firmware::memory::SetupCompletionError::SensorNotReady) => {
            usb_error_response(
                request_id,
                "sensor_unready",
                "Sensor readiness is required before setup completion.",
            )
        }
        Err(flux_purr_firmware::memory::SetupCompletionError::CalibrationRequired) => {
            usb_error_response(
                request_id,
                "calibration_required",
                "Calibration is required before setup completion.",
            )
        }
    };
    (false, response)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_reset_persistence(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; persistence cannot be reset.",
            ),
        );
    }
    context.memory_config.reset_for_commissioning();
    apply_memory_config_to_ui(context.ui_state, context.memory_config);
    *context.memory_commit_due_ms =
        Some(context.elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
    (true, usb_response(request_id, UsbResponsePayload::Ack))
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_network_request(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    op: UsbRequestOp,
) -> (bool, UsbFrame) {
    match op {
        UsbRequestOp::GetNetwork => process_get_network(context, request_id).await,
        UsbRequestOp::GetLanPairingCode => process_get_lan_pairing_code(request_id),
        UsbRequestOp::OpenLanPairingWindow => process_open_lan_pairing(context, request_id).await,
        UsbRequestOp::CloseLanPairingWindow => process_close_lan_pairing(context, request_id).await,
        UsbRequestOp::ClearLanPairingToken => {
            process_clear_lan_pairing_token(context, request_id).await
        }
        _ => unreachable!("non-network request routed to process_network_request"),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_get_network(
    _context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    #[cfg(feature = "net_http")]
    let network = flux_purr_firmware::net::lan_network_summary().await;
    #[cfg(not(feature = "net_http"))]
    let network = network_from_memory(_context.memory_config);
    (
        false,
        usb_response(request_id, UsbResponsePayload::Network(network)),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_get_lan_pairing_code(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    #[cfg(feature = "net_http")]
    {
        let code = flux_purr_firmware::net::pairing_code();
        (
            false,
            usb_response(
                request_id,
                UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
            ),
        )
    }
    #[cfg(not(feature = "net_http"))]
    (
        false,
        usb_error_response(
            request_id,
            "lan_unavailable",
            "LAN pairing is disabled in this firmware build.",
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_open_lan_pairing(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    #[cfg(feature = "net_http")]
    {
        let code = flux_purr_firmware::net::enter_pairing().await;
        context.ui_state.enter_wifi_pairing(code);
        (
            true,
            usb_response(
                request_id,
                UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
            ),
        )
    }
    #[cfg(not(feature = "net_http"))]
    lan_unavailable_response(request_id)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_close_lan_pairing(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    #[cfg(feature = "net_http")]
    {
        flux_purr_firmware::net::leave_pairing().await;
        context.ui_state.leave_wifi_pairing();
        context.ui_state.route = FrontPanelRoute::Dashboard;
        (true, usb_response(request_id, UsbResponsePayload::Ack))
    }
    #[cfg(not(feature = "net_http"))]
    lan_unavailable_response(request_id)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_clear_lan_pairing_token(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    #[cfg(feature = "net_http")]
    {
        if context.ui_state.persistence_locked() {
            return (
                false,
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; LAN pairing token cannot be cleared.",
                ),
            );
        }
        flux_purr_firmware::net::clear_token_from_usb().await;
        context.memory_config.lan_pairing_token = None;
        *context.memory_commit_due_ms =
            Some(context.elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
        info!("LAN pairing token cleared by USB control request");
        (false, usb_response(request_id, UsbResponsePayload::Ack))
    }
    #[cfg(not(feature = "net_http"))]
    lan_unavailable_response(request_id)
}

#[cfg(all(
    target_arch = "xtensa",
    feature = "web_serial",
    not(feature = "net_http")
))]
pub(crate) fn lan_unavailable_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    (
        false,
        usb_error_response(
            request_id,
            "lan_unavailable",
            "LAN pairing is disabled in this firmware build.",
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_wifi_config_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: WifiConfigCommand,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() && !matches!(config.op, WifiConfigOp::Cancel) {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; Wi-Fi persistence is locked.",
            ),
        );
    }
    #[cfg(feature = "net_http")]
    let network = match config.op {
        WifiConfigOp::Cancel => {
            let result = flux_purr_firmware::net::cancel_wifi_connection().await;
            match result {
                Ok(network) => network,
                Err(error) => {
                    return (
                        false,
                        usb_error_response(request_id, error.code(), error.message()),
                    );
                }
            }
        }
        WifiConfigOp::Set | WifiConfigOp::Clear => {
            config.apply_to(context.memory_config);
            flux_purr_firmware::net::apply_wifi_config(context.memory_config).await
        }
    };
    #[cfg(not(feature = "net_http"))]
    let network = match config.op {
        WifiConfigOp::Cancel => {
            return (
                false,
                usb_error_response(
                    request_id,
                    "wifi_cancel_unavailable",
                    "WiFi cancellation is unavailable in this firmware build.",
                ),
            );
        }
        WifiConfigOp::Set | WifiConfigOp::Clear => {
            config.apply_to(context.memory_config);
            network_from_memory(context.memory_config)
        }
    };
    if !matches!(config.op, WifiConfigOp::Cancel) {
        apply_memory_config_to_ui(context.ui_state, context.memory_config);
        *context.memory_commit_due_ms =
            Some(context.elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
    }
    (
        !matches!(config.op, WifiConfigOp::Cancel),
        usb_response(
            request_id,
            UsbResponsePayload::Wifi(WifiConfigReceipt {
                wifi: config.redacted_summary(),
                network,
            }),
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_runtime_config_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    mut config: RuntimeConfigCommand,
    active_profile: Option<ThermalControlProfile>,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked()
        && (config.heater_enabled == Some(true)
            || config.manual_pps_enabled == Some(true)
            || config.calibration.is_some())
    {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
            ),
        );
    }
    let previous_memory_config = context.memory_config.clone();
    let heater_toggle_requested = config.heater_enabled.is_some();
    let heater_rearm_requested = config.heater_enabled == Some(true);
    let overtemp_active = is_overtemp_fault(context.current_rtd_fault);
    acknowledge_runtime_attention(context, &config, overtemp_active);
    reject_runtime_rearm_if_attention_pending(
        context,
        &mut config,
        heater_rearm_requested,
        overtemp_active,
    );
    if should_clear_runtime_fault_latch(
        heater_rearm_requested,
        context.current_rtd_fault,
        context.heater_controller.fault_latched(),
    ) {
        context.heater_controller.clear_fault_latch();
        info!("heater runtime re-arm -> cleared latched fault");
    }
    let status_context = control_runtime_status_context(
        context,
        active_profile,
        *context.manual_pps,
        context.heater_controller.fault_latched(),
        *context.attention_pending_after_fault_clear,
    );
    let response = usb_runtime_config_response(
        request_id,
        config,
        UsbRuntimeConfigInput {
            ui_state: context.ui_state,
            memory_config: context.memory_config,
            manual_pps: context.manual_pps,
            thermal_control_profile_preview: context.thermal_control_profile_preview,
            calibration: context.calibration_runtime_state,
            context: status_context,
        },
    );
    if heater_toggle_requested {
        context
            .controller
            .clear_pending_short_press(RawFrontPanelKey::CenterBoot);
    }
    if *context.memory_config != previous_memory_config {
        *context.memory_commit_due_ms =
            Some(context.elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
    }
    (true, response)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn acknowledge_runtime_attention(
    context: &mut ControlLineContext<'_, '_, '_>,
    config: &RuntimeConfigCommand,
    overtemp_active: bool,
) {
    if config.fault_attention_acknowledged == Some(true)
        && acknowledge_overtemp_attention(
            overtemp_active,
            context.overtemp_attention_acknowledged,
            context.attention_pending_after_fault_clear,
            context.overtemp_forced_fan_active,
            context.next_attention_reminder_ms,
            context.buzzer,
        )
    {
        info!("overtemp attention acknowledged");
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn reject_runtime_rearm_if_attention_pending(
    context: &mut ControlLineContext<'_, '_, '_>,
    config: &mut RuntimeConfigCommand,
    heater_rearm_requested: bool,
    overtemp_active: bool,
) {
    if heater_rearm_requested && (overtemp_active || *context.attention_pending_after_fault_clear) {
        config.heater_enabled = Some(false);
        context.buzzer.request_feedback(
            BuzzerCueSource::RuntimeControl,
            BuzzerCueId::HeaterReject,
            context.elapsed_ms,
        );
        info!("heater runtime arm rejected by overtemp attention state");
    }
}

#[cfg(all(
    target_arch = "xtensa",
    feature = "web_serial",
    feature = "buzzer-test"
))]
pub(crate) fn process_buzzer_test_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: BuzzerTestCommand,
) -> (bool, UsbFrame) {
    if !command.is_valid() {
        return (
            false,
            usb_error_response(
                request_id,
                "invalid_buzzer_test_command",
                "buzzer_test requires exactly the fields for its operation.",
            ),
        );
    }
    if command.op != BuzzerTestOp::Status
        && (context.ui_state.heater_enabled
            || context.current_rtd_fault.is_some()
            || context.heater_controller.fault_latched().is_some()
            || *context.attention_pending_after_fault_clear)
    {
        return (
            false,
            usb_error_response(
                request_id,
                "buzzer_test_interlocked",
                "Buzzer test requires heater-off with no active or pending thermal fault.",
            ),
        );
    }
    let response = match command.op {
        BuzzerTestOp::Status => UsbFrame::BuzzerTestResponse {
            request_id,
            status: Box::new(buzzer_test_status()),
        },
        BuzzerTestOp::Trigger => submit_buzzer_test_if_idle(request_id, command, true),
        BuzzerTestOp::Run => submit_buzzer_test_if_idle(request_id, command, false),
        BuzzerTestOp::Stop => {
            BuzzerRuntime::submit_test(command);
            UsbFrame::BuzzerTestResponse {
                request_id,
                status: Box::new(buzzer_test_status()),
            }
        }
    };
    (false, response)
}

#[cfg(all(
    target_arch = "xtensa",
    feature = "web_serial",
    feature = "buzzer-test"
))]
pub(crate) fn submit_buzzer_test_if_idle(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: BuzzerTestCommand,
    reject_when_running: bool,
) -> UsbFrame {
    let status = buzzer_test_status();
    let busy = status.state == BuzzerTestSessionState::Running;
    if busy == reject_when_running {
        return usb_error_response(
            request_id,
            "buzzer_test_busy",
            "A buzzer test scenario is already running.",
        );
    }
    BuzzerRuntime::submit_test(command);
    UsbFrame::BuzzerTestResponse {
        request_id,
        status: Box::new(status),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_calibration_config_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: CalibrationConfigCommand,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
            ),
        );
    }
    if thermal_plant_calibration_job_running(*context.calibration_runtime_state) {
        return (
            false,
            usb_error_response(
                request_id,
                ManualPpsError::CalibrationInProgress.code(),
                "Automatic thermal-model calibration is running; calibration inputs are locked.",
            ),
        );
    }
    let previous_memory_config = context.memory_config.clone();
    let response = usb_calibration_config_response(
        request_id.clone(),
        config,
        context.memory_config,
        context.latest_rtd_raw_adc_mv,
        context.latest_vin_raw_adc_mv,
    );
    if previous_memory_config
        .thermal_plant_transient_active
        .is_some()
        && previous_memory_config.adc_calibration != context.memory_config.adc_calibration
    {
        disarm_calibration_after_transient_input_change(
            context.calibration_runtime_state,
            context.manual_pps,
        );
    }
    if *context.memory_config != previous_memory_config {
        let changed_domains =
            persist_domain_mask_between(context.memory_config, &previous_memory_config);
        if let Err(error) = commit_memory_config_now(
            context.eeprom_i2c,
            CommitMemoryConfigInput {
                memory_sequence: context.memory_sequence,
                memory_config: context.memory_config,
                domains_to_write: changed_domains,
                persistence_log_sink: context.persistence_log_sink,
                record_staging: context.record_staging,
            },
        )
        .await
        {
            restore_persisted_memory_domains(
                context.memory_config,
                context.ui_state,
                context.last_persisted_memory_config,
                changed_domains,
            );
            let fault = persistence_fault_from_commit(error);
            if memory_failure_requires_heater_lock(error) {
                mark_eeprom_required(
                    context.ui_state,
                    context.calibration_runtime_state,
                    context.manual_pps,
                    context.memory_commit_due_ms,
                    Some(fault),
                );
            } else {
                context.ui_state.persistence_fault = Some(fault);
                context.ui_state.persistence_fault_attention_pending = true;
            }
            return (
                false,
                usb_error_response(
                    request_id,
                    "memory_commit_failed",
                    "Calibration draft could not be persisted.",
                ),
            );
        }
        copy_persisted_domains(
            context.last_persisted_memory_config,
            context.memory_config,
            changed_domains,
        );
        *context.memory_commit_due_ms = None;
    }
    (false, response)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_calibration_job_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: CalibrationJobCommandWire,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
            ),
        );
    }
    if matches!(command.op, CalibrationJobOpWire::Start)
        && !pd_contract_allows_calibration(context.pd_controller, context.last_pd_observation)
    {
        return (
            false,
            usb_error_response(
                request_id,
                "pd_performance_not_guaranteed",
                "Calibration requires a performance-guaranteed PPS contract.",
            ),
        );
    }
    (
        false,
        usb_calibration_job_response(
            request_id,
            command,
            context.calibration_runtime_state,
            context.memory_config,
            context.manual_pps,
            context.thermal_plant_workspace,
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_thermal_plant_run_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    after_sample: u8,
) -> (bool, UsbFrame) {
    (
        false,
        usb_response(
            request_id,
            UsbResponsePayload::ThermalPlantRun(thermal_plant_run_snapshot_wire(
                context.calibration_runtime_state,
                context.memory_config,
                context.thermal_plant_workspace,
                after_sample,
                context.latest_status_temp_c,
                context.latest_vin_mv,
                context.last_heater_duty,
            )),
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn process_heater_curve_config_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: HeaterCurveConfigCommand,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
            ),
        );
    }
    (
        false,
        usb_heater_curve_config_response(
            request_id,
            config,
            context.memory_config,
            context.preview_heater_curve,
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_heater_curve_save_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
) -> (bool, UsbFrame) {
    if context.ui_state.persistence_locked() {
        return (
            false,
            usb_error_response(
                request_id,
                "eeprom_required",
                "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
            ),
        );
    }
    if thermal_plant_calibration_job_running(*context.calibration_runtime_state) {
        return (
            false,
            usb_error_response(
                request_id,
                ManualPpsError::CalibrationInProgress.code(),
                "Automatic thermal-model calibration is running; calibration inputs are locked.",
            ),
        );
    }
    let Some(preview) = *context.preview_heater_curve else {
        return (
            false,
            usb_error_response(
                request_id,
                "heater_curve_preview_required",
                "Heater curve save requires an active preview package.",
            ),
        );
    };
    let previous_memory_config = context.memory_config.clone();
    context.memory_config.active_heater_curve = preview.curve;
    if let Some(raw_observations) = preview.raw_observations {
        context.memory_config.heater_curve_raw_observations = raw_observations;
    }
    context.memory_config.sanitize();
    if context.memory_config.heater_curve_raw_observations
        != previous_memory_config.heater_curve_raw_observations
        && previous_memory_config
            .thermal_plant_transient_active
            .is_some()
    {
        context.memory_config.heater_curve_transaction_id = None;
        invalidate_transient_thermal_plant(context.memory_config);
        disarm_calibration_after_transient_input_change(
            context.calibration_runtime_state,
            context.manual_pps,
        );
    }
    if let Err(error) = persist_heater_curve(context).await {
        let code = error.code();
        let message = error.message();
        restore_persisted_memory_domains(
            context.memory_config,
            context.ui_state,
            context.last_persisted_memory_config,
            PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
        );
        mark_eeprom_required(
            context.ui_state,
            context.calibration_runtime_state,
            context.manual_pps,
            context.memory_commit_due_ms,
            Some(persistence_fault_from_commit(error)),
        );
        return (false, usb_error_response(request_id, code, message));
    }
    copy_persisted_domains(
        context.last_persisted_memory_config,
        context.memory_config,
        PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
    );
    *context.memory_commit_due_ms = None;
    (
        false,
        usb_response(
            request_id,
            UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                context.memory_config,
                context
                    .preview_heater_curve
                    .as_ref()
                    .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
            )),
        ),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn persist_heater_curve(
    context: &mut ControlLineContext<'_, '_, '_>,
) -> Result<(), MemoryCommitFailure> {
    commit_memory_config_now(
        context.eeprom_i2c,
        CommitMemoryConfigInput {
            memory_sequence: context.memory_sequence,
            memory_config: context.memory_config,
            domains_to_write: PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
            persistence_log_sink: context.persistence_log_sink,
            record_staging: context.record_staging,
        },
    )
    .await
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) async fn process_eeprom_maintenance_frame(
    context: &mut ControlLineContext<'_, '_, '_>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: EepromMaintenanceCommand,
) -> (bool, UsbFrame) {
    if context.last_heater_duty != 0 {
        return (
            false,
            usb_error_response(
                request_id,
                "heater_output_active",
                "EEPROM maintenance requires physical heater output to be off.",
            ),
        );
    }
    let op = command.op;
    if raw_eeprom_operation_mutates(op) {
        begin_mutating_eeprom_maintenance(
            context.ui_state,
            context.calibration_runtime_state,
            context.manual_pps,
            context.memory_commit_due_ms,
        );
        if !matches!(
            context.pd_port.restore_automatic_idle_contract(),
            PdContractRequestState::Confirmed
        ) {
            return (
                false,
                usb_error_response(
                    request_id,
                    "eeprom_power_disarm_failed",
                    "EEPROM maintenance could not restore fixed PD.",
                ),
            );
        }
    } else {
        context.ui_state.heater_enabled = false;
        calibration_job_canceled(context.calibration_runtime_state, context.manual_pps);
    }
    let response = usb_eeprom_maintenance_response(
        request_id,
        command,
        context.eeprom_i2c,
        context.elapsed_ms,
    )
    .await;
    if matches!(&response, UsbFrame::Response { ok: true, .. }) {
        apply_successful_eeprom_maintenance_operation(
            op,
            context.ui_state,
            context.memory_config,
            context.memory_commit_due_ms,
        );
        if matches!(op, EepromMaintenanceOp::Erase) {
            *context.preview_heater_curve = None;
            *context.thermal_control_profile_preview = None;
        }
    } else if eeprom_storage_failure_response(&response) {
        context.ui_state.eeprom_required = true;
    }
    (true, response)
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_pairing_code_payload(code: Option<[u8; 4]>) -> LanPairingCode {
    let code = code.map(|digits| {
        let mut rendered = heapless::String::new();
        for digit in digits {
            let _ = rendered.push(digit as char);
        }
        rendered
    });
    LanPairingCode {
        active: code.is_some(),
        code,
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_command_to_control_line(
    command: &ControlMailboxCommand,
) -> Result<heapless::String<USB_CONTROL_LINE_CAPACITY>, &'static str> {
    let request_op = match (command.endpoint, command.method) {
        (LanEndpoint::Identity, HttpMethod::Get) => Some("get_identity"),
        (LanEndpoint::Network, HttpMethod::Get) => Some("get_network"),
        (LanEndpoint::Status | LanEndpoint::Events, HttpMethod::Get) => Some("get_status"),
        (LanEndpoint::Calibration, HttpMethod::Get) => Some("get_calibration"),
        (LanEndpoint::CalibrationJob, HttpMethod::Get) => Some("get_calibration_job"),
        (LanEndpoint::HeaterCurve, HttpMethod::Get) => Some("get_heater_curve"),
        _ => None,
    };
    let mut line = heapless::String::new();
    if let Some(op) = request_op {
        write!(
            line,
            r#"{{"type":"request","requestId":"lan","op":"{op}"}}"#
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    if command.endpoint == LanEndpoint::ThermalPlantRun && command.method == HttpMethod::Get {
        write!(
            line,
            r#"{{"type":"thermal_plant_run","requestId":"lan","afterSample":{}}}"#,
            command.after_sample.unwrap_or(0)
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    // Saving the current heater-curve preview has no payload in the shared
    // USB JSONL contract. Do not force it through the JSON-object adapter.
    if command.endpoint == LanEndpoint::HeaterCurveSave {
        write!(line, r#"{{"type":"heater_curve_save","requestId":"lan"}}"#)
            .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    // The LAN API keeps a thermal-profile payload focused on the profile
    // operation itself. USB JSONL carries the same operation under runtime
    // config's `thermalControlProfile` field.
    if command.endpoint == LanEndpoint::ThermalProfile {
        let body = command.body.as_ref().trim();
        if !body.starts_with('{') || !body.ends_with('}') {
            return Err("LAN command body must be a JSON object");
        }
        write!(
            line,
            r#"{{"type":"runtime_config","requestId":"lan","thermalControlProfile":{body}}}"#
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    let frame_type = match command.endpoint {
        LanEndpoint::Runtime => "runtime_config",
        LanEndpoint::Calibration => "calibration_config",
        LanEndpoint::CalibrationJob => "calibration_job",
        LanEndpoint::HeaterCurve => "heater_curve_config",
        _ => return Err("LAN endpoint does not accept this method"),
    };
    let fields = command
        .body
        .as_ref()
        .trim()
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .ok_or("LAN command body must be a JSON object")?
        .trim();
    write!(line, r#"{{"type":"{frame_type}","requestId":"lan""#)
        .map_err(|_| "LAN request is too large")?;
    if !fields.is_empty() {
        line.push(',').map_err(|_| "LAN request is too large")?;
        line.push_str(fields)
            .map_err(|_| "LAN request is too large")?;
    }
    line.push('}').map_err(|_| "LAN request is too large")?;
    Ok(line)
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_error_json(code: &str, message: &str) -> heapless::String<LAN_HTTP_BODY_MAX_LEN> {
    let mut body = heapless::String::new();
    let _ = write!(
        body,
        r#"{{"error":{{"code":"{code}","message":"{message}"}}}}"#
    );
    body
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_frame_response(
    frame: &UsbFrame,
    network: flux_purr_firmware::control_plane::NetworkSummary,
) -> (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>) {
    match frame {
        UsbFrame::Response {
            ok: true,
            result: Some(result),
            ..
        } => match result {
            UsbResponsePayload::Identity(value) => lan_json_response(value),
            UsbResponsePayload::InstallStatus(_) => (
                404,
                lan_error_json(
                    "unsupported_operation",
                    "Install status is available only through USB/devd.",
                ),
            ),
            UsbResponsePayload::Network(value) => lan_json_response(value),
            UsbResponsePayload::Status(value) => {
                let mut status = value.clone();
                status.network = network;
                lan_json_response(&status)
            }
            UsbResponsePayload::LanPairingCode(value) => lan_json_response(value),
            UsbResponsePayload::Wifi(value) => lan_json_response(value),
            UsbResponsePayload::Calibration(value) => lan_json_response(value),
            UsbResponsePayload::CalibrationJob(value) => lan_json_response(value),
            UsbResponsePayload::ThermalPlantRun(value) => lan_json_response(value),
            UsbResponsePayload::HeaterCurve(value) => lan_json_response(value),
            UsbResponsePayload::EepromBytes(_) => (
                404,
                lan_error_json(
                    "unsupported_operation",
                    "EEPROM maintenance is available only through USB/devd.",
                ),
            ),
            UsbResponsePayload::Ack => {
                let mut body = heapless::String::new();
                let _ = body.push_str(r#"{"accepted":true}"#);
                (200, body)
            }
        },
        UsbFrame::Response {
            error: Some(error), ..
        }
        | UsbFrame::Error { error, .. } => (
            lan_error_status(error.code.as_str()),
            lan_error_json(error.code.as_str(), error.message.as_str()),
        ),
        _ => (
            500,
            lan_error_json(
                "invalid_control_response",
                "Control loop returned an invalid LAN response.",
            ),
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_json_response<T: Serialize>(
    value: &T,
) -> (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>) {
    let mut buffer = [0u8; LAN_HTTP_BODY_MAX_LEN];
    match serde_json_core::to_slice(value, &mut buffer) {
        Ok(written) => match core::str::from_utf8(&buffer[..written]) {
            Ok(json) => {
                let mut body = heapless::String::new();
                let _ = body.push_str(json);
                (200, body)
            }
            Err(_) => (
                500,
                lan_error_json(
                    "invalid_control_response",
                    "Control response was not valid UTF-8.",
                ),
            ),
        },
        Err(_) => (
            500,
            lan_error_json(
                "response_too_large",
                "Control response exceeded the LAN envelope.",
            ),
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn lan_error_status(code: &str) -> u16 {
    if code.starts_with("invalid_")
        || code.ends_with("_required")
        || matches!(
            code,
            "malformed_json" | "unsupported_frame" | "unsupported_lan_command"
        )
    {
        400
    } else {
        409
    }
}
