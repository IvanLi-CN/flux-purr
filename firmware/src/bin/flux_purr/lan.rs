#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
struct ControlLineContext<'a, 'i, 'e, PWM> {
    controller: &'a mut FrontPanelInputController,
    ui_state: &'a mut FrontPanelUiState,
    memory_config: &'a mut MemoryConfig,
    last_persisted_memory_config: &'a mut MemoryConfig,
    preview_heater_curve: &'a mut Option<HeaterCurvePreview>,
    memory_commit_due_ms: &'a mut Option<u64>,
    memory_sequence: &'a mut u32,
    persistence_source: &'static str,
    persistence_record_state: &'static str,
    pd_i2c: &'a mut I2c<'i, esp_hal::Blocking>,
    pd_controller: ControllerKind,
    pd_port: &'a mut PdPort,
    eeprom_pd_service: &'a mut EepromPdServiceContext<'e, PWM>,
    calibration_runtime_state: &'a mut CalibrationRuntimeState,
    thermal_plant_workspace: &'a mut CalibrationThermalPlantWorkspace,
    elapsed_ms: u64,
    last_pd_observation: Option<PdStatusObservation>,
    pd_contract_ready: &'a mut bool,
    heater_power_backend: &'a mut HeaterPowerBackend,
    heater_controller: &'a mut HeaterController,
    pid_snapshot: HeaterPidSnapshot,
    manual_pps: &'a mut ManualPpsState,
    fan_command: FanHardwareCommand,
    current_rtd_fault: Option<HeaterFaultReason>,
    overtemp_attention_acknowledged: &'a mut bool,
    attention_pending_after_fault_clear: &'a mut bool,
    overtemp_forced_fan_active: &'a mut bool,
    next_attention_reminder_ms: &'a mut Option<u64>,
    buzzer: &'a mut BuzzerRuntime,
    thermal_control_profile_preview: &'a mut Option<ThermalControlProfile>,
    last_raw_state: FrontPanelRawState,
    latest_status_temp_c: f32,
    latest_control_temp_c: f32,
    control_measurement_guarded: bool,
    latest_rtd_raw_adc_mv: u16,
    latest_rtd_raw_adc_min_mv: u16,
    latest_rtd_raw_adc_max_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_vin_mv: u32,
    last_heater_duty: u8,
    heater_control_timing: HeaterControlTiming,
    persistence_log_sink: &'a mut dyn PersistenceLogSink,
    record_staging: &'a mut [u8; EEPROM_RECORD_STAGING_BYTES],
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[expect(
    clippy::too_many_lines,
    clippy::excessive_nesting,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
async fn process_control_line<PWM>(
    line: &str,
    context: ControlLineContext<'_, '_, '_, PWM>,
) -> (bool, UsbFrame)
where
    PWM: SetDutyCycle,
{
    let ControlLineContext {
        controller,
        ui_state,
        memory_config,
        last_persisted_memory_config,
        preview_heater_curve,
        memory_commit_due_ms,
        memory_sequence,
        persistence_source,
        persistence_record_state,
        pd_i2c,
        pd_controller,
        pd_port,
        eeprom_pd_service,
        calibration_runtime_state,
        thermal_plant_workspace,
        elapsed_ms,
        last_pd_observation,
        pd_contract_ready,
        heater_power_backend,
        heater_controller,
        pid_snapshot,
        manual_pps,
        fan_command,
        current_rtd_fault,
        overtemp_attention_acknowledged,
        attention_pending_after_fault_clear,
        overtemp_forced_fan_active,
        next_attention_reminder_ms,
        buzzer,
        thermal_control_profile_preview,
        last_raw_state,
        latest_status_temp_c,
        latest_control_temp_c,
        control_measurement_guarded,
        latest_rtd_raw_adc_mv,
        latest_rtd_raw_adc_min_mv,
        latest_rtd_raw_adc_max_mv,
        latest_vin_raw_adc_mv,
        latest_vin_mv,
        last_heater_duty,
        heater_control_timing,
        persistence_log_sink,
        record_staging,
    } = context;
    let mut needs_redraw = false;
    let active_thermal_control_profile =
        active_thermal_control_profile(memory_config, *thermal_control_profile_preview, manual_pps);
    let runtime_context =
        |manual_pps_value: ManualPpsState,
         heater_fault_latched: Option<HeaterFaultReason>,
         attention_pending_after_fault_clear_value: bool| UsbRuntimeStatusContext {
            elapsed_ms,
            pd_controller,
            last_pd_observation,
            heater_power_backend: *heater_power_backend,
            pid_snapshot,
            heater_control_timing,
            heater_physical_output_percent: last_heater_duty,
            manual_pps: manual_pps_value,
            fan_command,
            current_rtd_fault,
            heater_fault_latched,
            attention_pending_after_fault_clear: attention_pending_after_fault_clear_value,
            thermal_control_profile_preview: thermal_control_profile_preview.is_some(),
            active_thermal_control_profile,
            last_raw_state,
            latest_status_temp_c,
            latest_control_temp_c,
            control_measurement_guarded,
            latest_rtd_raw_adc_mv,
            latest_rtd_raw_adc_min_mv,
            latest_rtd_raw_adc_max_mv,
            latest_vin_raw_adc_mv,
            vin_mv: latest_vin_mv,
        };
    #[cfg(feature = "net_http")]
    let initial_network_summary = {
        let mut pd_network_service = PdNetworkServiceContext {
            eeprom: &mut *eeprom_pd_service,
            pd_contract_ready,
            ui_state,
            calibration_runtime_state,
            manual_pps,
        };
        run_network_operation_with_pd(
            flux_purr_firmware::net::lan_network_summary(),
            pd_i2c,
            pd_port,
            &mut pd_network_service,
        )
        .await
    };
    #[cfg(feature = "net_http")]
    if ui_state.apply_network_summary(initial_network_summary) {
        // Status and network requests must observe the same device-owned
        // snapshot even during the first control-loop ticks after boot.
        needs_redraw = true;
    }
    let response = match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => match op {
            UsbRequestOp::GetIdentity => usb_response(
                request_id,
                UsbResponsePayload::Identity(Box::new(hardware_identity())),
            ),
            UsbRequestOp::GetInstallStatus => usb_response(
                request_id,
                UsbResponsePayload::InstallStatus(InstallStatus::from_runtime(
                    InstallRuntimeSnapshot {
                        config: memory_config,
                        persistence_source,
                        record_state: persistence_record_state,
                        record_sequence: *memory_sequence,
                        sensor_ready: current_rtd_fault.is_none()
                            && latest_status_temp_c.is_finite(),
                        heater_fault_latched: heater_controller.fault_latched().is_some(),
                        persistence_locked: ui_state.persistence_locked(),
                        last_persistence_fault: ui_state.persistence_fault.clone(),
                        persistence_fault_attention_pending: ui_state
                            .persistence_fault_attention_pending,
                    },
                )),
            ),
            UsbRequestOp::CompleteSetup => {
                if ui_state.persistence_locked() {
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; setup cannot be completed.",
                    )
                } else {
                    let sensor_ready =
                        current_rtd_fault.is_none() && latest_status_temp_c.is_finite();
                    let calibration_ready = flux_purr_firmware::memory::adc_calibration_fit(
                        &memory_config.adc_calibration,
                        flux_purr_firmware::memory::AdcCalibrationChannel::Rtd,
                    )
                    .sample_count
                        >= 2
                        && flux_purr_firmware::memory::adc_calibration_fit(
                            &memory_config.adc_calibration,
                            flux_purr_firmware::memory::AdcCalibrationChannel::Vin,
                        )
                        .sample_count
                            >= 2
                        && memory_config
                            .active_heater_curve
                            .points
                            .iter()
                            .flatten()
                            .count()
                            >= 2;
                    match memory_config.complete_setup(sensor_ready, calibration_ready) {
                        Ok(()) => {
                            *memory_commit_due_ms =
                                Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                            usb_response(request_id, UsbResponsePayload::Ack)
                        }
                        Err(flux_purr_firmware::memory::SetupCompletionError::SensorNotReady) => {
                            usb_error_response(
                                request_id,
                                "sensor_unready",
                                "Sensor readiness is required before setup completion.",
                            )
                        }
                        Err(
                            flux_purr_firmware::memory::SetupCompletionError::CalibrationRequired,
                        ) => usb_error_response(
                            request_id,
                            "calibration_required",
                            "Calibration is required before setup completion.",
                        ),
                    }
                }
            }
            UsbRequestOp::ResetPersistence => {
                if ui_state.persistence_locked() {
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; persistence cannot be reset.",
                    )
                } else {
                    memory_config.reset_for_commissioning();
                    apply_memory_config_to_ui(ui_state, memory_config);
                    *memory_commit_due_ms =
                        Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                    needs_redraw = true;
                    usb_response(request_id, UsbResponsePayload::Ack)
                }
            }
            UsbRequestOp::GetNetwork => {
                #[cfg(feature = "net_http")]
                let network = {
                    let mut pd_network_service = PdNetworkServiceContext {
                        eeprom: &mut *eeprom_pd_service,
                        pd_contract_ready,
                        ui_state,
                        calibration_runtime_state,
                        manual_pps,
                    };
                    run_network_operation_with_pd(
                        flux_purr_firmware::net::lan_network_summary(),
                        pd_i2c,
                        pd_port,
                        &mut pd_network_service,
                    )
                    .await
                };
                #[cfg(not(feature = "net_http"))]
                let network = network_from_memory(memory_config);
                usb_response(request_id, UsbResponsePayload::Network(network))
            }
            UsbRequestOp::GetStatus => usb_response(
                request_id,
                UsbResponsePayload::Status(usb_runtime_status(
                    ui_state,
                    memory_config,
                    calibration_runtime_state,
                    runtime_context(
                        *manual_pps,
                        heater_controller.fault_latched(),
                        *attention_pending_after_fault_clear,
                    ),
                )),
            ),
            UsbRequestOp::GetCalibration => usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibrationJob => usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration_runtime_state).job,
                ),
            ),
            UsbRequestOp::GetHeaterCurve => usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    memory_config,
                    preview_heater_curve
                        .as_ref()
                        .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
                )),
            ),
            UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
            UsbRequestOp::GetLanPairingCode => {
                #[cfg(feature = "net_http")]
                {
                    let code = flux_purr_firmware::net::pairing_code();
                    usb_response(
                        request_id,
                        UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
                    )
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::OpenLanPairingWindow => {
                #[cfg(feature = "net_http")]
                {
                    let code = {
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut *eeprom_pd_service,
                            pd_contract_ready,
                            ui_state,
                            calibration_runtime_state,
                            manual_pps,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::enter_pairing(),
                            pd_i2c,
                            pd_port,
                            &mut pd_network_service,
                        )
                        .await
                    };
                    ui_state.enter_wifi_pairing(code);
                    needs_redraw = true;
                    usb_response(
                        request_id,
                        UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
                    )
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::CloseLanPairingWindow => {
                #[cfg(feature = "net_http")]
                {
                    let mut pd_network_service = PdNetworkServiceContext {
                        eeprom: &mut *eeprom_pd_service,
                        pd_contract_ready,
                        ui_state,
                        calibration_runtime_state,
                        manual_pps,
                    };
                    run_network_operation_with_pd(
                        flux_purr_firmware::net::leave_pairing(),
                        pd_i2c,
                        pd_port,
                        &mut pd_network_service,
                    )
                    .await;
                    ui_state.leave_wifi_pairing();
                    ui_state.route = FrontPanelRoute::Dashboard;
                    needs_redraw = true;
                    usb_response(request_id, UsbResponsePayload::Ack)
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::ClearLanPairingToken => {
                #[cfg(feature = "net_http")]
                {
                    if ui_state.persistence_locked() {
                        usb_error_response(
                            request_id,
                            "eeprom_required",
                            "EEPROM_REQUIRED: persistent configuration is unavailable; LAN pairing token cannot be cleared.",
                        )
                    } else {
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut *eeprom_pd_service,
                            pd_contract_ready,
                            ui_state,
                            calibration_runtime_state,
                            manual_pps,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::clear_token_from_usb(),
                            pd_i2c,
                            pd_port,
                            &mut pd_network_service,
                        )
                        .await;
                        memory_config.lan_pairing_token = None;
                        *memory_commit_due_ms =
                            Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                        info!("LAN pairing token cleared by USB control request");
                        usb_response(request_id, UsbResponsePayload::Ack)
                    }
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
        },
        Ok(UsbFrame::WifiConfig { request_id, config }) => {
            if ui_state.persistence_locked() && !matches!(config.op, WifiConfigOp::Cancel) {
                return (
                    needs_redraw,
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
                    let result = {
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut *eeprom_pd_service,
                            pd_contract_ready,
                            ui_state,
                            calibration_runtime_state,
                            manual_pps,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::cancel_wifi_connection(),
                            pd_i2c,
                            pd_port,
                            &mut pd_network_service,
                        )
                        .await
                    };
                    match result {
                        Ok(network) => network,
                        Err(error) => {
                            return (
                                needs_redraw,
                                usb_error_response(request_id, error.code(), error.message()),
                            );
                        }
                    }
                }
                WifiConfigOp::Set | WifiConfigOp::Clear => {
                    config.apply_to(memory_config);
                    let mut pd_network_service = PdNetworkServiceContext {
                        eeprom: &mut *eeprom_pd_service,
                        pd_contract_ready,
                        ui_state,
                        calibration_runtime_state,
                        manual_pps,
                    };
                    run_network_operation_with_pd(
                        flux_purr_firmware::net::apply_wifi_config(memory_config),
                        pd_i2c,
                        pd_port,
                        &mut pd_network_service,
                    )
                    .await
                }
            };
            #[cfg(not(feature = "net_http"))]
            let network = match config.op {
                WifiConfigOp::Cancel => {
                    return (
                        needs_redraw,
                        usb_error_response(
                            request_id,
                            "wifi_cancel_unavailable",
                            "WiFi cancellation is unavailable in this firmware build.",
                        ),
                    );
                }
                WifiConfigOp::Set | WifiConfigOp::Clear => {
                    config.apply_to(memory_config);
                    network_from_memory(memory_config)
                }
            };
            if !matches!(config.op, WifiConfigOp::Cancel) {
                apply_memory_config_to_ui(ui_state, memory_config);
                *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                needs_redraw = true;
            }
            usb_response(
                request_id,
                UsbResponsePayload::Wifi(WifiConfigReceipt {
                    wifi: config.redacted_summary(),
                    network,
                }),
            )
        }
        Ok(UsbFrame::RuntimeConfig {
            request_id,
            mut config,
        }) => {
            if ui_state.persistence_locked()
                && (config.heater_enabled == Some(true)
                    || config.manual_pps_enabled == Some(true)
                    || config.calibration.is_some())
            {
                return (
                    needs_redraw,
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                    ),
                );
            }
            let previous_memory_config = memory_config.clone();
            let heater_toggle_requested = config.heater_enabled.is_some();
            let heater_rearm_requested = config.heater_enabled == Some(true);
            let overtemp_active = is_overtemp_fault(current_rtd_fault);
            if config.fault_attention_acknowledged == Some(true)
                && acknowledge_overtemp_attention(
                    overtemp_active,
                    overtemp_attention_acknowledged,
                    attention_pending_after_fault_clear,
                    overtemp_forced_fan_active,
                    next_attention_reminder_ms,
                    buzzer,
                )
            {
                info!("overtemp attention acknowledged");
            }
            if heater_rearm_requested && (overtemp_active || *attention_pending_after_fault_clear) {
                config.heater_enabled = Some(false);
                buzzer.request_feedback(
                    BuzzerCueSource::RuntimeControl,
                    BuzzerCueId::HeaterReject,
                    elapsed_ms,
                );
                info!("heater runtime arm rejected by overtemp attention state");
            }
            if should_clear_runtime_fault_latch(
                heater_rearm_requested,
                current_rtd_fault,
                heater_controller.fault_latched(),
            ) {
                heater_controller.clear_fault_latch();
                info!("heater runtime re-arm -> cleared latched fault");
            }
            let status_context = runtime_context(
                *manual_pps,
                heater_controller.fault_latched(),
                *attention_pending_after_fault_clear,
            );
            let response = usb_runtime_config_response(
                request_id,
                config,
                UsbRuntimeConfigInput {
                    ui_state,
                    memory_config,
                    manual_pps,
                    thermal_control_profile_preview,
                    calibration: calibration_runtime_state,
                    context: status_context,
                },
            );
            if heater_toggle_requested {
                controller.clear_pending_short_press(RawFrontPanelKey::CenterBoot);
            }
            if *memory_config != previous_memory_config {
                *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            }
            needs_redraw = true;
            response
        }
        #[cfg(feature = "buzzer-test")]
        Ok(UsbFrame::BuzzerTest {
            request_id,
            command,
        }) => {
            if !command.is_valid() {
                usb_error_response(
                    request_id,
                    "invalid_buzzer_test_command",
                    "buzzer_test requires exactly the fields for its operation.",
                )
            } else if command.op != BuzzerTestOp::Status
                && (ui_state.heater_enabled
                    || current_rtd_fault.is_some()
                    || heater_controller.fault_latched().is_some()
                    || *attention_pending_after_fault_clear)
            {
                usb_error_response(
                    request_id,
                    "buzzer_test_interlocked",
                    "Buzzer test requires heater-off with no active or pending thermal fault.",
                )
            } else {
                match command.op {
                    BuzzerTestOp::Status => UsbFrame::BuzzerTestResponse {
                        request_id,
                        status: Box::new(buzzer_test_status()),
                    },
                    BuzzerTestOp::Trigger => {
                        let status = buzzer_test_status();
                        if status.state == BuzzerTestSessionState::Running {
                            usb_error_response(
                                request_id,
                                "buzzer_test_busy",
                                "A buzzer test scenario is already running.",
                            )
                        } else {
                            BuzzerRuntime::submit_test(command);
                            UsbFrame::BuzzerTestResponse {
                                request_id,
                                status: Box::new(status),
                            }
                        }
                    }
                    BuzzerTestOp::Run => {
                        let status = buzzer_test_status();
                        if status.state != BuzzerTestSessionState::Running {
                            BuzzerRuntime::submit_test(command);
                            UsbFrame::BuzzerTestResponse {
                                request_id,
                                status: Box::new(status),
                            }
                        } else {
                            usb_error_response(
                                request_id,
                                "buzzer_test_busy",
                                "A buzzer test scenario is already running.",
                            )
                        }
                    }
                    BuzzerTestOp::Stop => {
                        BuzzerRuntime::submit_test(command);
                        UsbFrame::BuzzerTestResponse {
                            request_id,
                            status: Box::new(buzzer_test_status()),
                        }
                    }
                }
            }
        }
        Ok(UsbFrame::CalibrationConfig { request_id, config }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if thermal_plant_calibration_job_running(*calibration_runtime_state) {
                usb_error_response(
                    request_id,
                    ManualPpsError::CalibrationInProgress.code(),
                    "Automatic thermal-model calibration is running; calibration inputs are locked.",
                )
            } else {
                let previous_memory_config = memory_config.clone();
                let response = usb_calibration_config_response(
                    request_id.clone(),
                    config,
                    memory_config,
                    latest_rtd_raw_adc_mv,
                    latest_vin_raw_adc_mv,
                );
                if previous_memory_config
                    .thermal_plant_transient_active
                    .is_some()
                    && previous_memory_config.adc_calibration != memory_config.adc_calibration
                {
                    disarm_calibration_after_transient_input_change(
                        calibration_runtime_state,
                        manual_pps,
                    );
                }
                if *memory_config != previous_memory_config {
                    let changed_domains =
                        persist_domain_mask_between(memory_config, &previous_memory_config);
                    match commit_memory_config_now(
                        pd_i2c,
                        pd_port,
                        eeprom_pd_service,
                        CommitMemoryConfigInput {
                            memory_sequence,
                            memory_config,
                            domains_to_write: changed_domains,
                            persistence_log_sink,
                            record_staging,
                        },
                    )
                    .await
                    {
                        Ok(()) => {
                            copy_persisted_domains(
                                last_persisted_memory_config,
                                memory_config,
                                changed_domains,
                            );
                            *memory_commit_due_ms = None;
                        }
                        Err(error) => {
                            restore_persisted_memory_domains(
                                memory_config,
                                ui_state,
                                last_persisted_memory_config,
                                changed_domains,
                            );
                            let fault = persistence_fault_from_commit(error);
                            if memory_failure_requires_heater_lock(error) {
                                mark_eeprom_required(
                                    ui_state,
                                    calibration_runtime_state,
                                    manual_pps,
                                    memory_commit_due_ms,
                                    Some(fault),
                                );
                            } else {
                                ui_state.persistence_fault = Some(fault);
                                ui_state.persistence_fault_attention_pending = true;
                            }
                            return (
                                needs_redraw,
                                usb_error_response(
                                    request_id,
                                    "memory_commit_failed",
                                    "Calibration draft could not be persisted.",
                                ),
                            );
                        }
                    }
                }
                response
            }
        }
        Ok(UsbFrame::CalibrationJob {
            request_id,
            command,
        }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if matches!(command.op, CalibrationJobOpWire::Start)
                && !pd_contract_allows_calibration(pd_controller, last_pd_observation)
            {
                let (code, message) = (
                    "pd_performance_not_guaranteed",
                    "Calibration requires a performance-guaranteed PPS contract.",
                );
                usb_error_response(request_id, code, message)
            } else {
                usb_calibration_job_response(
                    request_id,
                    command,
                    calibration_runtime_state,
                    memory_config,
                    manual_pps,
                    thermal_plant_workspace,
                )
            }
        }
        Ok(UsbFrame::ThermalPlantRun {
            request_id,
            after_sample,
        }) => usb_response(
            request_id,
            UsbResponsePayload::ThermalPlantRun(thermal_plant_run_snapshot_wire(
                calibration_runtime_state,
                memory_config,
                thermal_plant_workspace,
                after_sample,
                latest_status_temp_c,
                latest_vin_mv,
                last_heater_duty,
            )),
        ),
        Ok(UsbFrame::HeaterCurveConfig { request_id, config }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else {
                usb_heater_curve_config_response(
                    request_id,
                    config,
                    memory_config,
                    preview_heater_curve,
                )
            }
        }
        Ok(UsbFrame::HeaterCurveSave { request_id }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if thermal_plant_calibration_job_running(*calibration_runtime_state) {
                usb_error_response(
                    request_id,
                    ManualPpsError::CalibrationInProgress.code(),
                    "Automatic thermal-model calibration is running; calibration inputs are locked.",
                )
            } else if let Some(preview) = *preview_heater_curve {
                let previous_memory_config = memory_config.clone();
                memory_config.active_heater_curve = preview.curve;
                if let Some(raw_observations) = preview.raw_observations {
                    memory_config.heater_curve_raw_observations = raw_observations;
                }
                memory_config.sanitize();
                let raw_observations_changed = previous_memory_config.heater_curve_raw_observations
                    != memory_config.heater_curve_raw_observations;
                if raw_observations_changed
                    && previous_memory_config
                        .thermal_plant_transient_active
                        .is_some()
                {
                    memory_config.heater_curve_transaction_id = None;
                    invalidate_transient_thermal_plant(memory_config);
                    disarm_calibration_after_transient_input_change(
                        calibration_runtime_state,
                        manual_pps,
                    );
                }
                if let Err(error) = commit_memory_config_now(
                    pd_i2c,
                    pd_port,
                    eeprom_pd_service,
                    CommitMemoryConfigInput {
                        memory_sequence,
                        memory_config,
                        domains_to_write: PersistDomainMask::SAFETY
                            .union(PersistDomainMask::THERMAL_PLANT),
                        persistence_log_sink,
                        record_staging,
                    },
                )
                .await
                {
                    let code = error.code();
                    let message = error.message();
                    restore_persisted_memory_domains(
                        memory_config,
                        ui_state,
                        last_persisted_memory_config,
                        PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
                    );
                    mark_eeprom_required(
                        ui_state,
                        calibration_runtime_state,
                        manual_pps,
                        memory_commit_due_ms,
                        Some(persistence_fault_from_commit(error)),
                    );
                    return (needs_redraw, usb_error_response(request_id, code, message));
                }
                copy_persisted_domains(
                    last_persisted_memory_config,
                    memory_config,
                    PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
                );
                *memory_commit_due_ms = None;
                usb_response(
                    request_id,
                    UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                        memory_config,
                        preview_heater_curve
                            .as_ref()
                            .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
                    )),
                )
            } else {
                usb_error_response(
                    request_id,
                    "heater_curve_preview_required",
                    "Heater curve save requires an active preview package.",
                )
            }
        }
        Ok(UsbFrame::EepromMaintenance {
            request_id,
            command,
        }) => {
            if last_heater_duty != 0 {
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
                    ui_state,
                    calibration_runtime_state,
                    manual_pps,
                    memory_commit_due_ms,
                );
                if !matches!(
                    request_pd_fixed_voltage(pd_i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST).await,
                    PdContractRequestState::Confirmed
                ) {
                    return (
                        needs_redraw,
                        usb_error_response(
                            request_id,
                            "eeprom_power_disarm_failed",
                            "EEPROM maintenance could not restore fixed PD.",
                        ),
                    );
                }
            } else {
                ui_state.heater_enabled = false;
                calibration_job_canceled(calibration_runtime_state, manual_pps);
            }
            needs_redraw = true;
            let response = usb_eeprom_maintenance_response(
                request_id,
                command,
                pd_i2c,
                pd_port,
                eeprom_pd_service,
                elapsed_ms,
            )
            .await;
            if matches!(&response, UsbFrame::Response { ok: true, .. }) {
                apply_successful_eeprom_maintenance_operation(
                    op,
                    ui_state,
                    memory_config,
                    memory_commit_due_ms,
                );
                if matches!(op, EepromMaintenanceOp::Erase) {
                    *preview_heater_curve = None;
                    *thermal_control_profile_preview = None;
                }
            } else if eeprom_storage_failure_response(&response) {
                ui_state.eeprom_required = true;
            }
            response
        }
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
    };

    (needs_redraw, response)
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_pairing_code_payload(code: Option<[u8; 4]>) -> LanPairingCode {
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
fn lan_command_to_control_line(
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
        let body = command.body.as_str().trim();
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
        .as_str()
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
fn lan_error_json(code: &str, message: &str) -> heapless::String<LAN_HTTP_BODY_MAX_LEN> {
    let mut body = heapless::String::new();
    let _ = write!(
        body,
        r#"{{"error":{{"code":"{code}","message":"{message}"}}}}"#
    );
    body
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_frame_response(
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
fn lan_json_response<T: Serialize>(value: &T) -> (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>) {
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
fn lan_error_status(code: &str) -> u16 {
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
