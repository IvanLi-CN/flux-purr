#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeInputOutcome {
    sample: flux_purr_firmware::frontpanel::FrontPanelSampleResult,
    needs_redraw: bool,
    skip_iteration: bool,
    pairing_opened_by_usb: bool,
}
#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_finish_frontpanel_event(
    state: &mut RuntimeLoopState,
    event: flux_purr_firmware::frontpanel::KeyEvent,
    elapsed_ms: u64,
    heater_enabled_before: bool,
    active_cooling_enabled_before: bool,
    interaction_handled: bool,
) -> bool {
    let mut needs_redraw = interaction_handled;
    needs_redraw |=
        runtime_apply_cooling_feedback(state, active_cooling_enabled_before, elapsed_ms);
    let (heater_feedback, heater_needs_redraw) =
        runtime_apply_heater_feedback(state, heater_enabled_before, elapsed_ms);
    if maybe_play_frontpanel_ui_input_feedback(
        interaction_handled,
        heater_feedback,
        &mut state.buzzer,
        elapsed_ms,
    ) {
        info!(
            "ui input feedback -> route={=str} key={=str} gesture={=str}",
            route_label(state.ui_state.route),
            event.key.label(),
            event.gesture.label(),
        );
    }
    runtime_persist_frontpanel_memory(state, interaction_handled, elapsed_ms);
    needs_redraw |= heater_needs_redraw;
    needs_redraw
}
#[cfg(target_arch = "xtensa")]
impl RuntimeInputOutcome {
    fn skip(
        sample: flux_purr_firmware::frontpanel::FrontPanelSampleResult,
        needs_redraw: bool,
        pairing_opened_by_usb: bool,
    ) -> Self {
        Self {
            sample,
            needs_redraw,
            skip_iteration: true,
            pairing_opened_by_usb,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeUsbInputOutcome {
    needs_redraw: bool,
    control_command_processed: bool,
    response_pending: bool,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeLanInputOutcome {
    needs_redraw: bool,
    command_processed: bool,
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
impl RuntimeLanInputOutcome {
    fn processed(needs_redraw: bool) -> Self {
        Self {
            needs_redraw,
            command_processed: true,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_apply_pd_snapshot(state: &mut RuntimeLoopState) -> bool {
    let mut needs_redraw = false;
    let current_pd_observation = state.pd_port.observation();
    if current_pd_observation.is_none() {
        HeaterPwmGate::force_off();
    }
    if pd_status_log_key(current_pd_observation) != state.last_pd_status_log_key {
        match current_pd_observation {
            Some(observation) => info!(
                "pd status update status=0x{=u8:02x} pd={=bool} epr={=bool} epr_exist={=bool} current_raw=0x{=u8:02x} current_ma={=u16}",
                observation.status_raw,
                observation.status.pd_active,
                observation.status.epr_active,
                observation.status.epr_exist,
                observation.current_raw,
                observation.current_ma,
            ),
            None => info!("pd status update read=failed"),
        }
        state.last_pd_status_log_key = pd_status_log_key(current_pd_observation);
    }
    state.last_pd_observation = current_pd_observation;
    needs_redraw |= apply_pd_contract_observation(
        current_pd_observation,
        &mut state.pd_contract_ready,
        &mut state.ui_state,
        &mut state.calibration_runtime_state,
        &mut state.manual_pps_state,
        &mut state.heater_pwm,
        &mut state.last_heater_duty,
    );

    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_usb_snapshot_line(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> Option<bool> {
    let response = process_eeprom_snapshot_line(
        state.transport.usb_rx_line.as_str(),
        &mut state.transport.eeprom_snapshot_session,
        &mut state.eeprom_i2c,
        &mut state.memory_commit_due_ms,
        elapsed_ms,
        state.last_heater_duty != 0,
    )
    .await?;
    let storage_failed = eeprom_snapshot_storage_failure(&response);
    if storage_failed {
        mark_eeprom_required(
            &mut state.ui_state,
            &mut state.calibration_runtime_state,
            &mut state.manual_pps_state,
            &mut state.memory_commit_due_ms,
            None,
        );
    }
    start_eeprom_snapshot_response(
        &mut state.transport.usb_response_writer,
        &response,
        state.transport.usb_tx_buf,
        Instant::now().as_millis(),
    );
    state.transport.usb_rx_line.clear();
    Some(storage_failed)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_usb_control_line(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> (bool, bool) {
    let pd_observation_for_control = state.last_pd_observation;
    let heater_duty_for_control = state.last_heater_duty;
    let (control_needs_redraw, response) = process_control_line(
        state.transport.usb_rx_line.as_str(),
        ControlLineContext {
            controller: &mut state.controller,
            ui_state: &mut state.ui_state,
            memory_config: &mut state.memory_config,
            last_persisted_memory_config: &mut state.last_persisted_memory_config,
            preview_heater_curve: &mut state.preview_heater_curve,
            memory_commit_due_ms: &mut state.memory_commit_due_ms,
            memory_sequence: &mut state.memory_sequence,
            persistence_source: state.persistence_source,
            persistence_record_state: state.persistence_record_state,
            eeprom_i2c: &mut state.eeprom_i2c,
            pd_controller: state.pd_port.controller_kind(),
            pd_port: &state.pd_port,
            calibration_runtime_state: &mut state.calibration_runtime_state,
            thermal_plant_workspace: state.thermal_plant_workspace,
            elapsed_ms,
            last_pd_observation: pd_observation_for_control,
            heater_power_backend: &mut state.heater_power_backend,
            heater_controller: &mut state.heater_controller,
            pid_snapshot: state.last_pid_snapshot,
            manual_pps: &mut state.manual_pps_state,
            fan_command: state.fan_command,
            current_rtd_fault: state.current_rtd_fault,
            overtemp_attention_acknowledged: &mut state.overtemp_attention_acknowledged,
            attention_pending_after_fault_clear: &mut state.attention_pending_after_fault_clear,
            overtemp_forced_fan_active: &mut state.overtemp_forced_fan_active,
            next_attention_reminder_ms: &mut state.next_attention_reminder_ms,
            buzzer: &mut state.buzzer,
            thermal_control_profile_preview: &mut state.thermal_control_profile_preview,
            last_raw_state: state.last_raw_state,
            latest_status_temp_c: state.latest_display_temp_c,
            latest_control_temp_c: state.latest_temp_c,
            control_measurement_guarded: state.control_measurement_guarded,
            latest_rtd_raw_adc_mv: state.latest_rtd_raw_adc_mv,
            latest_rtd_raw_adc_min_mv: state.latest_rtd_raw_adc_min_mv,
            latest_rtd_raw_adc_max_mv: state.latest_rtd_raw_adc_max_mv,
            latest_vin_raw_adc_mv: state.latest_vin_raw_adc_mv,
            latest_vin_mv: state.latest_vin_mv,
            last_heater_duty: heater_duty_for_control,
            heater_control_timing: state.heater_control_timing,
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await;
    let mut needs_redraw = control_needs_redraw;
    let mutation_requested =
        usb_mutating_request_id(state.transport.usb_rx_line.as_str()).is_some();
    needs_redraw |= disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
        calibration_runtime_state: &mut state.calibration_runtime_state,
        backend: &mut state.heater_power_backend,
        manual_pps: &mut state.manual_pps_state,
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        hold_pps_governor: &mut state.hold_pps_governor,
        ui_state: &mut state.ui_state,
        last_heater_duty: &mut state.last_heater_duty,
        measured_vin_mv: state.latest_vin_mv,
    })
    .await;
    let _ = usb_start_response_frame(
        &mut state.transport.usb_response_writer,
        &response,
        state.transport.usb_tx_buf,
        Instant::now().as_millis(),
    );
    state.transport.usb_rx_line.clear();
    let mutation_succeeded = mutation_requested && usb_mutation_succeeded(&response);
    (needs_redraw, mutation_succeeded)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_usb_line(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> (bool, bool) {
    if runtime_process_usb_snapshot_line(state, elapsed_ms)
        .await
        .is_some()
    {
        return (false, true);
    }
    let mut mutating_request_id = usb_mutating_request_id(state.transport.usb_rx_line.as_str());
    if let Some(request_id) = mutating_request_id.as_ref()
        && usb_mutating_request_id_is_recent(
            &state.transport.usb_recent_mutating_request_ids,
            request_id,
        )
    {
        let response = usb_error_response(
            request_id.clone(),
            "request_replayed",
            "The request may already have executed; reconcile device state and use a new requestId.",
        );
        let _ = usb_start_response_frame(
            &mut state.transport.usb_response_writer,
            &response,
            state.transport.usb_tx_buf,
            Instant::now().as_millis(),
        );
        state.transport.usb_rx_line.clear();
        return (false, true);
    }
    let (needs_redraw, mutation_succeeded) =
        runtime_process_usb_control_line(state, elapsed_ms).await;
    if mutation_succeeded && let Some(request_id) = mutating_request_id.take() {
        remember_mutating_request_id(
            &mut state.transport.usb_recent_mutating_request_ids,
            request_id,
        );
    }
    (needs_redraw, true)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn runtime_drain_usb_input(state: &mut RuntimeLoopState, elapsed_ms: u64) -> (bool, bool) {
    let mut usb_bytes_processed = 0_u16;
    while usb_bytes_processed < PD_RUNTIME_USB_BYTE_BUDGET {
        let byte = match state.transport.usb_serial.read_byte() {
            Ok(byte) => byte,
            Err(_) => return (false, false),
        };
        usb_bytes_processed = usb_bytes_processed.saturating_add(1);
        if byte == b'\n' {
            if state.transport.usb_rx_overflowed {
                state.transport.usb_rx_overflowed = false;
                state.transport.usb_rx_line.clear();
                let response = usb_line_too_long_response();
                let _ = usb_start_response_frame(
                    &mut state.transport.usb_response_writer,
                    &response,
                    state.transport.usb_tx_buf,
                    Instant::now().as_millis(),
                );
                return (false, true);
            }
            return runtime_process_usb_line(state, elapsed_ms).await;
        }
        if byte != b'\r' {
            append_usb_control_byte(
                &mut *state.transport.usb_rx_line,
                &mut state.transport.usb_rx_overflowed,
                byte,
            );
        }
    }
    (false, false)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_usb_input(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> RuntimeUsbInputOutcome {
    let mut needs_redraw = false;
    #[cfg(feature = "net_http")]
    let mut control_command_processed = false;
    #[cfg(not(feature = "net_http"))]
    let control_command_processed = false;
    #[cfg(feature = "web_serial")]
    let usb_response_state = usb_pump_response(
        &mut state.transport.usb_serial,
        &mut state.transport.usb_response_writer,
        state.transport.usb_tx_buf,
        Instant::now().as_millis(),
    );
    #[cfg(feature = "web_serial")]
    if matches!(usb_response_state, UsbResponsePumpOutcome::Fault) {
        state.transport.usb_transport_faulted = true;
        state.transport.usb_recovery_marker_failed = false;
        state.transport.usb_recovery_writer.abort();
        return RuntimeUsbInputOutcome {
            needs_redraw,
            control_command_processed,
            response_pending: true,
        };
    }
    #[cfg(feature = "web_serial")]
    if state.transport.usb_transport_faulted {
        if !state.transport.usb_recovery_marker_failed
            && state.transport.usb_recovery_writer.is_complete()
            && !usb_start_transport_recovery(
                &mut state.transport.usb_recovery_writer,
                state.transport.usb_tx_buf,
                Instant::now().as_millis(),
            )
        {
            state.transport.usb_recovery_marker_failed = true;
        }
        if !state.transport.usb_recovery_marker_failed {
            match usb_pump_recovery_response(
                &mut state.transport.usb_serial,
                &mut state.transport.usb_recovery_writer,
                state.transport.usb_tx_buf,
                Instant::now().as_millis(),
            ) {
                UsbResponsePumpOutcome::Idle => {
                    state.transport.usb_transport_faulted = false;
                }
                UsbResponsePumpOutcome::Fault => {
                    state.transport.usb_recovery_marker_failed = true;
                }
                UsbResponsePumpOutcome::Pending => {}
            }
        }
        if state.transport.usb_recovery_marker_failed {
            warn!("USB recovery marker failed; dropping the bounded recovery frame");
            state.transport.usb_transport_faulted = false;
            state.transport.usb_recovery_marker_failed = false;
            state.transport.usb_recovery_writer.abort();
        } else {
            return RuntimeUsbInputOutcome {
                needs_redraw,
                control_command_processed,
                response_pending: true,
            };
        }
    }
    #[cfg(feature = "web_serial")]
    if matches!(usb_response_state, UsbResponsePumpOutcome::Idle) {
        let _ = state
            .transport
            .persistence_log_sink
            .flush_one(&mut state.transport.usb_serial);
    }
    #[cfg(feature = "web_serial")]
    if matches!(usb_response_state, UsbResponsePumpOutcome::Idle) {
        let (line_needs_redraw, _line_processed) = runtime_drain_usb_input(state, elapsed_ms).await;
        needs_redraw |= line_needs_redraw;
        #[cfg(feature = "net_http")]
        {
            control_command_processed |= _line_processed;
        }
    }

    RuntimeUsbInputOutcome {
        needs_redraw,
        control_command_processed,
        #[cfg(feature = "web_serial")]
        response_pending: !state.transport.usb_response_writer.is_complete(),
        #[cfg(not(feature = "web_serial"))]
        response_pending: false,
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) async fn runtime_lan_direct_response(
    _state: &mut RuntimeLoopState,
    command: &flux_purr_firmware::net_http::ControlMailboxCommand,
) -> Option<(u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>)> {
    match (command.endpoint, command.method) {
        (LanEndpoint::Identity, HttpMethod::Get) => {
            let identity = flux_purr_firmware::net::lan_identity().await;
            Some(lan_json_response(&identity))
        }
        (LanEndpoint::Network, HttpMethod::Get) => {
            let network = flux_purr_firmware::net::lan_network_summary().await;
            Some(lan_json_response(&network))
        }
        _ => None,
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) async fn runtime_process_lan_control(
    state: &mut RuntimeLoopState,
    line: &str,
    elapsed_ms: u64,
) -> (bool, (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>)) {
    let pd_observation_for_control = state.last_pd_observation;
    let heater_duty_for_control = state.last_heater_duty;
    let result = process_control_line(
        line,
        ControlLineContext {
            controller: &mut state.controller,
            ui_state: &mut state.ui_state,
            memory_config: &mut state.memory_config,
            last_persisted_memory_config: &mut state.last_persisted_memory_config,
            preview_heater_curve: &mut state.preview_heater_curve,
            memory_commit_due_ms: &mut state.memory_commit_due_ms,
            memory_sequence: &mut state.memory_sequence,
            persistence_source: state.persistence_source,
            persistence_record_state: state.persistence_record_state,
            eeprom_i2c: &mut state.eeprom_i2c,
            pd_controller: state.pd_port.controller_kind(),
            pd_port: &state.pd_port,
            calibration_runtime_state: &mut state.calibration_runtime_state,
            thermal_plant_workspace: state.thermal_plant_workspace,
            elapsed_ms,
            last_pd_observation: pd_observation_for_control,
            heater_power_backend: &mut state.heater_power_backend,
            heater_controller: &mut state.heater_controller,
            pid_snapshot: state.last_pid_snapshot,
            manual_pps: &mut state.manual_pps_state,
            fan_command: state.fan_command,
            current_rtd_fault: state.current_rtd_fault,
            overtemp_attention_acknowledged: &mut state.overtemp_attention_acknowledged,
            attention_pending_after_fault_clear: &mut state.attention_pending_after_fault_clear,
            overtemp_forced_fan_active: &mut state.overtemp_forced_fan_active,
            next_attention_reminder_ms: &mut state.next_attention_reminder_ms,
            buzzer: &mut state.buzzer,
            thermal_control_profile_preview: &mut state.thermal_control_profile_preview,
            last_raw_state: state.last_raw_state,
            latest_status_temp_c: state.latest_display_temp_c,
            latest_control_temp_c: state.latest_temp_c,
            control_measurement_guarded: state.control_measurement_guarded,
            latest_rtd_raw_adc_mv: state.latest_rtd_raw_adc_mv,
            latest_rtd_raw_adc_min_mv: state.latest_rtd_raw_adc_min_mv,
            latest_rtd_raw_adc_max_mv: state.latest_rtd_raw_adc_max_mv,
            latest_vin_raw_adc_mv: state.latest_vin_raw_adc_mv,
            latest_vin_mv: state.latest_vin_mv,
            last_heater_duty: heater_duty_for_control,
            heater_control_timing: state.heater_control_timing,
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await;
    disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
        calibration_runtime_state: &mut state.calibration_runtime_state,
        backend: &mut state.heater_power_backend,
        manual_pps: &mut state.manual_pps_state,
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        hold_pps_governor: &mut state.hold_pps_governor,
        ui_state: &mut state.ui_state,
        last_heater_duty: &mut state.last_heater_duty,
        measured_vin_mv: state.latest_vin_mv,
    })
    .await;
    let network_summary = flux_purr_firmware::net::lan_network_summary().await;
    let (control_needs_redraw, response) = result;
    (
        control_needs_redraw,
        lan_frame_response(&response, network_summary),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) fn runtime_reject_lan_command(
    response_slot: u8,
    request_id: u32,
    code: &str,
    message: &str,
) -> RuntimeLanInputOutcome {
    flux_purr_firmware::net::respond_to_command(
        response_slot,
        request_id,
        409,
        lan_error_json(code, message),
        false,
    );
    RuntimeLanInputOutcome::processed(false)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_lan(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
    _control_command_processed: bool,
) -> RuntimeLanInputOutcome {
    let mut needs_redraw = false;
    #[cfg(feature = "net_http")]
    if !_control_command_processed
        && let Some(command) = flux_purr_firmware::net::try_receive_command()
    {
        let request_id = command.request_id;
        let response_slot = command.response_slot;
        let is_mutation = matches!(
            command.method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Delete
        );
        let lease_active = flux_purr_firmware::net::command_lease_is_active(&command).await;
        if is_mutation && !lease_active {
            return runtime_reject_lan_command(
                response_slot,
                request_id,
                "lease_expired",
                "The LAN lease expired before this command reached the control loop.",
            );
        }
        if is_mutation
            && flux_purr_firmware::net_http::validate_control_revision(
                command.expected_revision,
                flux_purr_firmware::net::current_control_revision(),
            )
            .is_err()
        {
            return runtime_reject_lan_command(
                response_slot,
                request_id,
                "stale_write",
                "The control state changed after this client last read it.",
            );
        }
        let direct_response = runtime_lan_direct_response(state, &command).await;
        if let Some((status, body)) = direct_response {
            flux_purr_firmware::net::respond_to_command(
                response_slot,
                request_id,
                status,
                body,
                false,
            );
            return RuntimeLanInputOutcome::processed(needs_redraw);
        }
        let line = match lan_command_to_control_line(&command) {
            Ok(line) => line,
            Err(message) => {
                flux_purr_firmware::net::respond_to_command(
                    response_slot,
                    request_id,
                    400,
                    lan_error_json("unsupported_lan_command", message),
                    false,
                );
                return RuntimeLanInputOutcome::processed(needs_redraw);
            }
        };
        let (control_needs_redraw, (status, body)) =
            runtime_process_lan_control(state, line.as_str(), elapsed_ms).await;
        needs_redraw |= control_needs_redraw;
        flux_purr_firmware::net::respond_to_command(
            response_slot,
            request_id,
            status,
            body,
            is_mutation,
        );
    }

    RuntimeLanInputOutcome {
        needs_redraw,
        command_processed: false,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_reconcile_network_state(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let mut needs_redraw = false;
    #[cfg(feature = "net_http")]
    let persisted_token_change = flux_purr_firmware::net::take_persisted_token_change().await;
    #[cfg(feature = "net_http")]
    if let Some(token) = persisted_token_change {
        state.memory_config.lan_pairing_token = token;
        state.memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
    }

    needs_redraw |= apply_pd_contract_observation(
        state.last_pd_observation,
        &mut state.pd_contract_ready,
        &mut state.ui_state,
        &mut state.calibration_runtime_state,
        &mut state.manual_pps_state,
        &mut state.heater_pwm,
        &mut state.last_heater_duty,
    );

    needs_redraw |= disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
        calibration_runtime_state: &mut state.calibration_runtime_state,
        backend: &mut state.heater_power_backend,
        manual_pps: &mut state.manual_pps_state,
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        hold_pps_governor: &mut state.hold_pps_governor,
        ui_state: &mut state.ui_state,
        last_heater_duty: &mut state.last_heater_duty,
        measured_vin_mv: state.latest_vin_mv,
    })
    .await;
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_input(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> RuntimeInputOutcome {
    let raw_state = state.inputs.sample();
    let sample = state.controller.sample_with_capabilities(
        elapsed_ms,
        raw_state,
        state.ui_state.gesture_capabilities(),
    );
    let route_before_usb_control = state.ui_state.route;
    let usb_input = runtime_process_usb_input(state, elapsed_ms).await;
    let mut needs_redraw = usb_input.needs_redraw;
    let pairing_opened_by_usb = route_before_usb_control != FrontPanelRoute::WifiInfo
        && state.ui_state.route == FrontPanelRoute::WifiInfo;
    if pairing_opened_by_usb {
        state.suppress_pairing_input_until_released = true;
    }
    if usb_input.response_pending {
        return RuntimeInputOutcome::skip(sample, needs_redraw, pairing_opened_by_usb);
    }

    let lan_input =
        runtime_process_lan(state, elapsed_ms, usb_input.control_command_processed).await;
    needs_redraw |= lan_input.needs_redraw;
    if lan_input.command_processed {
        return RuntimeInputOutcome::skip(sample, needs_redraw, pairing_opened_by_usb);
    }
    needs_redraw |= runtime_reconcile_network_state(state, elapsed_ms).await;
    RuntimeInputOutcome {
        sample,
        needs_redraw,
        skip_iteration: false,
        pairing_opened_by_usb,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_process_frontpanel_raw_state(
    state: &mut RuntimeLoopState,
    raw_state: FrontPanelRawState,
) -> bool {
    if raw_state == state.last_raw_state {
        return false;
    }
    if should_consume_attention_raw_input(
        overtemp_attention_requires_ack(
            is_overtemp_fault(state.current_rtd_fault),
            state.overtemp_attention_acknowledged,
            state.attention_pending_after_fault_clear,
        ),
        state.suppress_attention_ack_input,
        state.last_raw_state,
        raw_state,
    ) && acknowledge_overtemp_attention(
        is_overtemp_fault(state.current_rtd_fault),
        &mut state.overtemp_attention_acknowledged,
        &mut state.attention_pending_after_fault_clear,
        &mut state.overtemp_forced_fan_active,
        &mut state.next_attention_reminder_ms,
        &mut state.buzzer,
    ) {
        state.suppress_attention_ack_input = true;
        state.suppress_attention_ack_event_seen = false;
        state.suppress_attention_ack_clear_after_ms = None;
        state.suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
        state.suppress_attention_ack_waits_for_event =
            raw_state.first_pressed().is_some_and(|raw_key| {
                let key = FrontPanelKeyMap::default().logical_from_raw(raw_key);
                let gestures = state.ui_state.gesture_capabilities().gestures_for(key);
                if gestures.supports(KeyGesture::DoublePress) {
                    state.suppress_attention_ack_clear_delay_ms =
                        FRONTPANEL_DOUBLE_CLICK_MS.saturating_add(FRONTPANEL_DEBOUNCE_MS);
                }
                gestures.supports(KeyGesture::ShortPress)
                    || gestures.supports(KeyGesture::DoublePress)
                    || gestures.supports(KeyGesture::LongPress)
            });
        info!(
            "fault attention reminder acknowledged -> consume raw input mask={=u8}",
            raw_state.pressed_mask(),
        );
    }
    state.ui_state.set_raw_state(raw_state);
    state.last_raw_state = raw_state;
    info!("raw mask={=u8}", raw_state.pressed_mask());
    state.runtime_mode == FrontPanelRuntimeMode::KeyTest
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_retry_persistence_io(
    state: &mut RuntimeLoopState,
    retry_domains: PersistDomainMask,
    retry_blank_initialization: bool,
) -> Result<(), MemoryCommitFailure> {
    #[cfg(feature = "web_serial")]
    let retry_log_sink = &mut state.transport.persistence_log_sink as &mut dyn PersistenceLogSink;
    #[cfg(not(feature = "web_serial"))]
    let retry_log_sink = &mut state.transport.persistence_log_sink as &mut dyn PersistenceLogSink;
    if state.prepared_layout_recovery_pending {
        return recover_prepared_fpr2_layout(
            &mut state.eeprom_i2c,
            state.memory_sequence,
            retry_log_sink,
            state.eeprom_record_staging,
        )
        .await;
    }
    if state.eeprom_data_incompatible {
        let mut scratch = new_memory_io_scratch();
        let (legacy_record, read_failed) = load_legacy_eeprom_memory_record(
            &mut state.eeprom_i2c,
            &mut scratch,
            state.eeprom_record_staging,
        )
        .await;
        if let Some(record) = legacy_record {
            state.memory_sequence = record.sequence;
            state.memory_config = record.config;
            return migrate_legacy_memory_config(
                &mut state.eeprom_i2c,
                state.memory_sequence,
                &state.memory_config,
                retry_log_sink,
                state.eeprom_record_staging,
            )
            .await
            .map(|sequence| {
                state.memory_sequence = sequence;
            });
        }
        return Err(MemoryCommitFailure {
            error: if read_failed {
                MemoryCommitError::VerifyUnreadable
            } else {
                MemoryCommitError::VerifyMismatch
            },
            phase: "legacy-retry-read",
            attempt: 1,
            sequence: state.memory_sequence,
            domain: PersistDomain::LayoutMarker,
            slot: PersistSlot::Single,
        });
    }
    if retry_blank_initialization {
        return match initialize_fpr2_defaults(
            &mut state.eeprom_i2c,
            retry_log_sink,
            state.eeprom_record_staging,
        )
        .await
        {
            Some(sequence) => {
                state.memory_sequence = sequence;
                Ok(())
            }
            None => Err(MemoryCommitFailure {
                error: MemoryCommitError::VerifyUnreadable,
                phase: "active-init-retry",
                attempt: 1,
                sequence: state.memory_sequence.saturating_add(1),
                domain: PersistDomain::LayoutMarker,
                slot: PersistSlot::A,
            }),
        };
    }
    commit_memory_config_now(
        &mut state.eeprom_i2c,
        CommitMemoryConfigInput {
            memory_sequence: &mut state.memory_sequence,
            memory_config: &state.memory_config,
            domains_to_write: retry_domains,
            persistence_log_sink: retry_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_retry_persistence(state: &mut RuntimeLoopState) -> bool {
    let changed =
        persist_domain_mask_between(&state.memory_config, &state.last_persisted_memory_config);
    let retry_blank_initialization = state.eeprom_required
        && state.memory_sequence == 0
        && !state.eeprom_data_incompatible
        && !state.prepared_layout_recovery_pending;
    let retry_domains = if state.eeprom_required {
        PersistDomainMask::ALL
    } else {
        changed.union(PersistDomainMask::from_fault(
            state.ui_state.persistence_fault.as_ref(),
        ))
    };
    let retry_format_recovery = retry_blank_initialization
        || state.prepared_layout_recovery_pending
        || state.eeprom_data_incompatible;
    let retry_result =
        runtime_retry_persistence_io(state, retry_domains, retry_blank_initialization).await;
    match retry_result {
        Ok(()) => {
            if retry_format_recovery {
                state.last_persisted_memory_config = state.memory_config.clone();
                state.eeprom_required = false;
                state.eeprom_data_incompatible = false;
                state.prepared_layout_recovery_pending = false;
                state.ui_state.eeprom_required = false;
                state.ui_state.eeprom_data_incompatible = false;
                state.ui_state.persistence_fault = None;
                state.ui_state.persistence_fault_attention_pending = false;
                let _ = state.ui_state.set_dashboard_presentation(
                    flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                );
            } else {
                copy_persisted_domains(
                    &mut state.last_persisted_memory_config,
                    &state.memory_config,
                    retry_domains,
                );
                let retried_safety_domain = retry_domains
                    .includes(PersistDomain::SafetyCalibration)
                    || retry_domains.includes(PersistDomain::ThermalPolicy);
                if retried_safety_domain {
                    state.eeprom_required = false;
                    state.ui_state.eeprom_required = false;
                    state.ui_state.eeprom_data_incompatible = false;
                    state.ui_state.persistence_fault = None;
                    state.ui_state.persistence_fault_attention_pending = false;
                } else if !state.ui_state.persistence_locked() {
                    state.ui_state.persistence_fault = None;
                    state.ui_state.persistence_fault_attention_pending = false;
                }
            }
            state.persistence_source = "eeprom";
            state.persistence_record_state = "valid";
            true
        }
        Err(error) => {
            state.ui_state.persistence_fault = Some(persistence_fault_from_commit(error));
            state.ui_state.persistence_fault_attention_pending = true;
            true
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_reconcile_frontpanel_pairing(
    state: &mut RuntimeLoopState,
    route_before: FrontPanelRoute,
) {
    if route_before != FrontPanelRoute::WifiInfo
        && state.ui_state.route == FrontPanelRoute::WifiInfo
    {
        #[cfg(feature = "net_http")]
        {
            let code = flux_purr_firmware::net::enter_pairing().await;
            state.ui_state.enter_wifi_pairing(code);
            info!("LAN pairing window opened from WiFi Info page");
        }
        #[cfg(not(feature = "net_http"))]
        state.ui_state.leave_wifi_pairing();
    } else if route_before == FrontPanelRoute::WifiInfo
        && state.ui_state.route != FrontPanelRoute::WifiInfo
    {
        #[cfg(feature = "net_http")]
        {
            flux_purr_firmware::net::leave_pairing().await;
            state.ui_state.leave_wifi_pairing();
            info!("LAN pairing window closed after leaving WiFi Info page");
        }
        #[cfg(not(feature = "net_http"))]
        state.ui_state.leave_wifi_pairing();
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_apply_cooling_feedback(
    state: &mut RuntimeLoopState,
    active_cooling_enabled_before: bool,
    elapsed_ms: u64,
) -> bool {
    if state.ui_state.active_cooling_enabled == active_cooling_enabled_before {
        return false;
    }
    state.buzzer.request_feedback(
        BuzzerCueSource::FrontPanel,
        if state.ui_state.active_cooling_enabled {
            BuzzerCueId::ActiveCoolingOn
        } else {
            BuzzerCueId::ActiveCoolingOff
        },
        elapsed_ms,
    );
    info!(
        "active cooling policy -> {=str}",
        if state.ui_state.active_cooling_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    if state.ui_state.active_cooling_enabled {
        state.cooling_disabled_lock_latched = false;
        state.cooling_disabled_lock_armed = true;
    }
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_apply_heater_feedback(
    state: &mut RuntimeLoopState,
    heater_enabled_before: bool,
    elapsed_ms: u64,
) -> (bool, bool) {
    if state.ui_state.heater_enabled == heater_enabled_before {
        return (false, false);
    }
    if !state.ui_state.heater_enabled {
        state.buzzer.request_feedback(
            BuzzerCueSource::FrontPanel,
            BuzzerCueId::HeaterOff,
            elapsed_ms,
        );
        info!("heater arm -> off");
        return (true, false);
    }
    if state.cooling_disabled_lock_latched {
        state.cooling_disabled_lock_latched = false;
        state.cooling_disabled_lock_armed = false;
        info!("heater re-arm -> cleared cooling-disabled lock");
    }
    if state.heater_controller.fault_latched().is_some() {
        if let Some(reason) = state.current_rtd_fault {
            state.ui_state.heater_enabled = false;
            state.buzzer.request_feedback(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::HeaterReject,
                elapsed_ms,
            );
            info!("heater re-arm blocked reason={=str}", reason.label());
            return (true, true);
        }
        state.heater_controller.clear_fault_latch();
        state.buzzer.request_feedback(
            BuzzerCueSource::FrontPanel,
            BuzzerCueId::HeaterOn,
            elapsed_ms,
        );
        info!("heater re-arm -> cleared latched fault");
        return (true, false);
    }
    state.buzzer.request_feedback(
        BuzzerCueSource::FrontPanel,
        BuzzerCueId::HeaterOn,
        elapsed_ms,
    );
    info!("heater arm -> on");
    (true, false)
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_persist_frontpanel_memory(
    state: &mut RuntimeLoopState,
    interaction_handled: bool,
    elapsed_ms: u64,
) {
    if !interaction_handled {
        return;
    }
    let next_memory_config = memory_config_from_ui(&state.ui_state, &state.memory_config);
    if next_memory_config == state.memory_config {
        return;
    }
    state.memory_config = next_memory_config;
    state.memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
    info!(
        "memory dirty -> debounce_until_ms={=u64} target_c={=i16} slot={=u8} active_cooling={=bool}",
        state.memory_commit_due_ms.unwrap_or(0),
        state.memory_config.target_temp_c,
        state.memory_config.selected_preset_slot as u8,
        state.memory_config.active_cooling_enabled,
    );
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_read_heater_sensors(
    state: &mut RuntimeLoopState,
    current_request_mv: u16,
) -> (RtdSample, bool) {
    let mut needs_redraw = false;
    let previous_vin_raw_adc_mv = state.latest_vin_raw_adc_mv;
    let mut rtd_sample = read_rtd_sample(
        &mut state.adc1,
        &mut state.rtd_adc_pin,
        state.adc_curve.as_ref(),
        &state.memory_config,
    )
    .await;
    if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) = read_calibrated_vin_mv(
        &mut state.adc1,
        &mut state.vin_adc_pin,
        state.adc_curve.as_ref(),
        &state.memory_config,
    )
    .await
    {
        let retry_rtd_after_power_step = should_retry_rtd_sample_after_power_step(
            state.last_rtd_sample_request_mv,
            current_request_mv,
            previous_vin_raw_adc_mv,
            raw_adc_mv,
        );
        state.latest_vin_raw_adc_mv = raw_adc_mv;
        if state.latest_vin_mv != vin_mv {
            state.latest_vin_mv = vin_mv;
            needs_redraw = true;
        }
        needs_redraw |= reconcile_pd_contract_with_vin(
            &mut state.pd_contract_vin_guard,
            state.last_pd_observation,
            Some(vin_mv),
            PdTimestamp::now().as_millis(),
            PdContractVinContext {
                pd_port: &state.pd_port,
                last_pd_observation: &mut state.last_pd_observation,
                pd_contract_ready: &mut state.pd_contract_ready,
                ui_state: &mut state.ui_state,
                calibration_runtime_state: &mut state.calibration_runtime_state,
                manual_pps_state: &mut state.manual_pps_state,
                heater_pwm: &mut state.heater_pwm,
                last_heater_duty: &mut state.last_heater_duty,
            },
        );
        info!(
            "vin sample raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
            raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
        );
        if retry_rtd_after_power_step {
            rtd_sample = read_rtd_sample(
                &mut state.adc1,
                &mut state.rtd_adc_pin,
                state.adc_curve.as_ref(),
                &state.memory_config,
            )
            .await;
        }
    }
    (rtd_sample, needs_redraw)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_frontpanel_input(
    state: &mut RuntimeLoopState,
    sample: flux_purr_firmware::frontpanel::FrontPanelSampleResult,
    elapsed_ms: u64,
    pairing_opened_by_usb: bool,
) -> bool {
    let mut needs_redraw = false;
    needs_redraw |= runtime_process_frontpanel_raw_state(state, sample.raw_state);

    for event in sample.events {
        if state.suppress_pairing_input_until_released {
            continue;
        }
        let route_before = state.ui_state.route;
        let heater_enabled_before = state.ui_state.heater_enabled;
        let active_cooling_enabled_before = state.ui_state.active_cooling_enabled;
        info!(
            "key raw={=str} logical={=str} gesture={=str} at_ms={=u64}",
            event.raw_key.label(),
            event.key.label(),
            event.gesture.label(),
            event.at_ms,
        );
        if state.suppress_attention_ack_input {
            info!(
                "fault attention acknowledgement suppresses event raw={=str} logical={=str} gesture={=str}",
                event.raw_key.label(),
                event.key.label(),
                event.gesture.label(),
            );
            state.suppress_attention_ack_event_seen = true;
            continue;
        }
        if acknowledge_overtemp_attention(
            is_overtemp_fault(state.current_rtd_fault),
            &mut state.overtemp_attention_acknowledged,
            &mut state.attention_pending_after_fault_clear,
            &mut state.overtemp_forced_fan_active,
            &mut state.next_attention_reminder_ms,
            &mut state.buzzer,
        ) {
            info!(
                "fault attention reminder acknowledged -> consume input raw={=str} logical={=str} gesture={=str}",
                event.raw_key.label(),
                event.key.label(),
                event.gesture.label(),
            );
            continue;
        }
        let interaction_handled = state.ui_state.handle_event(event);
        if state.ui_state.take_persistence_retry_request() {
            needs_redraw |= runtime_retry_persistence(state).await;
        }
        runtime_reconcile_frontpanel_pairing(state, route_before).await;
        needs_redraw |= runtime_finish_frontpanel_event(
            state,
            event,
            elapsed_ms,
            heater_enabled_before,
            active_cooling_enabled_before,
            interaction_handled,
        );
    }
    if state.suppress_pairing_input_until_released
        && !pairing_opened_by_usb
        && sample.raw_state.first_pressed().is_none()
    {
        state.suppress_pairing_input_until_released = false;
    }
    if state.suppress_attention_ack_input
        && state.suppress_attention_ack_waits_for_event
        && sample.raw_state.pressed_mask() == 0
        && state.suppress_attention_ack_clear_after_ms.is_none()
    {
        state.suppress_attention_ack_clear_after_ms =
            Some(elapsed_ms.saturating_add(state.suppress_attention_ack_clear_delay_ms));
    }
    if should_clear_attention_ack_suppression(
        state.suppress_attention_ack_input,
        state.suppress_attention_ack_waits_for_event,
        state.suppress_attention_ack_event_seen,
        sample.raw_state,
        state.suppress_attention_ack_clear_after_ms,
        elapsed_ms,
    ) {
        state.suppress_attention_ack_input = false;
        state.suppress_attention_ack_waits_for_event = false;
        state.suppress_attention_ack_event_seen = false;
        state.suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
        state.suppress_attention_ack_clear_after_ms = None;
    }

    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_apply_rtd_sample(
    state: &mut RuntimeLoopState,
    rtd_sample: RtdSample,
    current_request_mv: u16,
    elapsed_ms: u64,
) -> (Option<f32>, bool) {
    let mut needs_redraw = false;
    preserve_rtd_control_guard_when_heater_disabled(
        state.ui_state.heater_enabled,
        &mut state.control_measurement_guarded,
    );
    let calibration_live_rtd_temp_c = match rtd_sample {
        RtdSample::Valid(measurement) => {
            state.latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
            state.latest_rtd_raw_adc_min_mv = measurement.raw_adc_min_mv;
            state.latest_rtd_raw_adc_max_mv = measurement.raw_adc_max_mv;
            needs_redraw |= apply_valid_rtd_measurement(
                RuntimeDisplayTemperatureState {
                    ui_state: &mut state.ui_state,
                    latest_display_temp_c: &mut state.latest_display_temp_c,
                    latest_display_temp_i16: &mut state.latest_display_temp_i16,
                },
                RuntimeControlTemperatureState {
                    latest_control_temp_c: &mut state.latest_temp_c,
                    latest_control_temp_i16: &mut state.latest_temp_i16,
                    transition_guard: &mut state.rtd_pps_transition_guard,
                    measurement_guard: &mut state.rtd_control_measurement_guard,
                    control_measurement_guarded: &mut state.control_measurement_guarded,
                    heater_controller: &mut state.heater_controller,
                },
                current_request_mv,
                elapsed_ms,
                measurement.temp_c,
            );
            state.current_rtd_fault = overtemp_fault_from_control_temperature(measurement.temp_c);
            Some(measurement.temp_c)
        }
        RtdSample::Fault { adc_mv, reason } => {
            state.latest_rtd_raw_adc_mv = adc_mv.unwrap_or(0);
            state.latest_rtd_raw_adc_min_mv = state.latest_rtd_raw_adc_mv;
            state.latest_rtd_raw_adc_max_mv = state.latest_rtd_raw_adc_mv;
            state.current_rtd_fault = Some(reason);
            state.rtd_control_measurement_guard.clear();
            state.control_measurement_guarded = false;
            clear_runtime_temperature(&mut state.latest_temp_c, &mut state.latest_temp_i16);
            needs_redraw |= retain_runtime_display_temperature(
                &mut state.ui_state,
                &mut state.latest_display_temp_c,
                &mut state.latest_display_temp_i16,
            );
            info!(
                "rtd fault adc_mv={=u16} reason={=str} heater_arm={=bool}",
                adc_mv.unwrap_or(0),
                reason.label(),
                state.ui_state.heater_enabled,
            );
            None
        }
    };
    state.last_rtd_sample_request_mv = current_request_mv;
    (calibration_live_rtd_temp_c, needs_redraw)
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_update_fault_attention(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let mut needs_redraw = false;
    if let Some(reason) = state.current_rtd_fault
        && state.heater_controller.latch_fault(reason)
    {
        state.ui_state.heater_enabled = false;
        needs_redraw = true;
        info!("heater fault latched reason={=str}", reason.label());
    }
    let fault_present = is_overtemp_fault(state.current_rtd_fault);
    let attention_state_changed = update_fault_attention_state(
        fault_present,
        FaultAttentionState {
            last_fault_present: &mut state.last_fault_present,
            attention_acknowledged: &mut state.overtemp_attention_acknowledged,
            attention_pending_after_fault_clear: &mut state.attention_pending_after_fault_clear,
            forced_fan_active: &mut state.overtemp_forced_fan_active,
            protection_alarm: &mut state.protection_alarm,
            next_attention_reminder_ms: &mut state.next_attention_reminder_ms,
        },
        state.latest_display_temp_i16,
        &mut state.buzzer,
        elapsed_ms,
    );
    if attention_state_changed && fault_present {
        info!("protection alarm -> active");
    } else if attention_state_changed {
        info!(
            "protection cleared -> reminder pending interval_ms={=u64}",
            BUZZER_ATTENTION_REMINDER_INTERVAL_MS,
        );
    }
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_refresh_source_capabilities(state: &mut RuntimeLoopState) -> bool {
    if state.pd_port.controller_kind() != ControllerKind::Fusb302b {
        return false;
    }
    let capabilities = state.pd_port.capabilities();
    if capabilities == state.last_fusb302b_power_capabilities {
        return false;
    }
    state.last_fusb302b_power_capabilities = capabilities;
    state.heater_power_backend =
        refresh_fusb302b_heater_power_backend(state.heater_power_backend, capabilities);
    state.manual_pps_state = ManualPpsState::from_fusb302b_capabilities(capabilities);
    state.hold_pps_governor = HoldPpsGovernor::new();
    info!("fusb302b source capabilities changed; refreshed heater power bounds");
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_update_calibration_job(
    state: &mut RuntimeLoopState,
    current_pd_observation: Option<PdStatusObservation>,
    calibration_live_rtd_temp_c: Option<f32>,
) -> bool {
    update_calibration_runtime_state(
        &mut state.calibration_runtime_state,
        &state.manual_pps_state,
        state.latest_rtd_raw_adc_mv,
        state.latest_vin_raw_adc_mv,
    );
    let thermal_plant_was_running = state.calibration_runtime_state.mode
        == CalibrationMode::ThermalPlant
        && state.calibration_runtime_state.job.kind == Some(CalibrationJobKind::ThermalPlant)
        && state.calibration_runtime_state.job.status == CalibrationJobStatus::Running;
    if state.current_rtd_fault.is_some() && thermal_plant_was_running {
        calibration_job_fail(
            &mut state.calibration_runtime_state,
            ManualPpsError::WriteFailed,
            true,
            &mut state.manual_pps_state,
        );
    } else {
        let calibration_temp_c = thermal_plant_calibration_temperature_c(
            state.calibration_runtime_state,
            calibration_live_rtd_temp_c,
            state.latest_temp_c,
        );
        update_calibration_job_state(
            &mut state.calibration_runtime_state,
            &mut state.memory_config,
            &mut state.manual_pps_state,
            state.thermal_plant_workspace,
            CalibrationJobUpdateInput {
                latest_rtd_raw_adc_mv: state.latest_rtd_raw_adc_mv,
                latest_vin_raw_adc_mv: state.latest_vin_raw_adc_mv,
                latest_temp_c: calibration_temp_c,
                pd_current_ma: current_pd_observation
                    .map(|observation| observation.current_ma)
                    .unwrap_or(0),
                latest_vin_mv: state.latest_vin_mv,
                heater_duty_percent: state.last_heater_duty,
            },
        );
    }
    thermal_plant_was_running
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_reconcile_heater_arming(
    state: &mut RuntimeLoopState,
    calibration_live_rtd_temp_c: Option<f32>,
    thermal_plant_was_running: bool,
) -> (bool, bool) {
    let calibration_output_temp_c = thermal_plant_calibration_temperature_c(
        state.calibration_runtime_state,
        calibration_live_rtd_temp_c,
        state.latest_temp_c,
    );
    let force_output_off = !state.pd_contract_ready
        || thermal_plant_output_must_be_off(
            state.calibration_runtime_state,
            thermal_plant_was_running,
            calibration_output_temp_c,
        );
    let mut needs_redraw = disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
        calibration_runtime_state: &mut state.calibration_runtime_state,
        backend: &mut state.heater_power_backend,
        manual_pps: &mut state.manual_pps_state,
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        hold_pps_governor: &mut state.hold_pps_governor,
        ui_state: &mut state.ui_state,
        last_heater_duty: &mut state.last_heater_duty,
        measured_vin_mv: state.latest_vin_mv,
    })
    .await;
    if state.calibration_runtime_state.mode != CalibrationMode::Off
        && state.calibration_runtime_state.heater_enabled
        && state.current_rtd_fault.is_none()
        && state.heater_controller.fault_latched().is_some()
    {
        state.heater_controller.clear_fault_latch();
        info!("calibration heater re-arm -> cleared latched fault");
    }
    let mut desired_heater_enabled = reconcile_runtime_heater_enabled(
        state.ui_state.heater_enabled,
        state.calibration_runtime_state,
        state.current_rtd_fault,
        state.cooling_disabled_lock_latched,
        state.heater_controller.fault_latched().is_some(),
        thermal_model_heater_allowed(
            &state.memory_config,
            state.calibration_runtime_state,
            state.manual_pps_state,
        ),
        state.pd_contract_ready,
    );
    desired_heater_enabled = consume_thermal_plant_completion_disarm(
        &mut state.calibration_runtime_state,
        desired_heater_enabled,
    );
    if state.ui_state.persistence_locked() {
        desired_heater_enabled = false;
    }
    if state.ui_state.heater_enabled != desired_heater_enabled {
        state.ui_state.heater_enabled = desired_heater_enabled;
        needs_redraw = true;
    }
    if force_output_off {
        state.ui_state.heater_enabled = false;
    }
    (force_output_off, needs_redraw)
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_update_heater_controller(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
    force_output_off: bool,
) -> (HeaterPidSnapshot, u8, bool) {
    let runtime_plant = thermal_plant_projection_for_runtime(&state.memory_config);
    let runtime_source_limits = state.manual_pps_state.heater_source_limits();
    let max_power_mw = heater_available_power_mw_for_temp(
        state.latest_temp_c,
        runtime_source_limits.map(|(_, max_mv, _)| max_mv),
        runtime_source_limits.map(|(_, _, max_ma)| max_ma),
        preview_heater_curve_config(state.preview_heater_curve.as_ref()),
        &state.memory_config,
    );
    let thermal_plant_calibration_running = state.calibration_runtime_state.mode
        == CalibrationMode::ThermalPlant
        && state.calibration_runtime_state.job.status == CalibrationJobStatus::Running;
    let pid_snapshot = if thermal_plant_calibration_running {
        thermal_plant_calibration_snapshot(
            state.latest_temp_c,
            state.calibration_runtime_state.heater_enabled,
        )
    } else if state.calibration_runtime_state.mode == CalibrationMode::Off {
        match runtime_plant {
            Some((model, ambient_temp_c)) => {
                state
                    .heater_controller
                    .update_thermal_plant_at(ThermalPlantRuntimeInput {
                        target_temp_c: state.ui_state.target_temp_c,
                        measured_temp_c: state.latest_temp_c,
                        ambient_temp_c,
                        heater_enabled: state.ui_state.heater_enabled,
                        model,
                        max_power_mw: max_power_mw as f32,
                        now_ms: elapsed_ms,
                    })
            }
            None => state.heater_controller.update_at(
                state.ui_state.target_temp_c,
                state.latest_temp_c,
                false,
                None,
                elapsed_ms,
            ),
        }
    } else {
        state.heater_controller.update_at(
            state
                .calibration_runtime_state
                .model_target_temp_c
                .unwrap_or(state.ui_state.target_temp_c),
            state.latest_temp_c,
            state.ui_state.heater_enabled,
            None,
            elapsed_ms,
        )
    };
    state.last_pid_snapshot = pid_snapshot;
    let requested_duty_percent = if force_output_off {
        0
    } else {
        pid_snapshot.duty_percent
    };
    let mut needs_redraw = false;
    if state.ui_state.heater_output_percent != requested_duty_percent {
        state.ui_state.heater_output_percent = requested_duty_percent;
        needs_redraw = true;
    }
    (pid_snapshot, requested_duty_percent, needs_redraw)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeHeaterOutputContext<'a> {
    state: &'a mut RuntimeLoopState,
    pid_snapshot: HeaterPidSnapshot,
    requested_duty_percent: u8,
    force_output_off: bool,
    thermal_plant_calibration_running: bool,
    active_thermal_settings: ThermalControlProfileSettings,
    current_pd_observation: Option<PdStatusObservation>,
    control_started_ms: u64,
    elapsed_ms: u64,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_apply_heater_output(context: RuntimeHeaterOutputContext<'_>) -> bool {
    let RuntimeHeaterOutputContext {
        state,
        pid_snapshot,
        requested_duty_percent,
        force_output_off,
        thermal_plant_calibration_running,
        active_thermal_settings,
        current_pd_observation,
        control_started_ms,
        elapsed_ms,
    } = context;
    let output_changed = apply_heater_power_output(HeaterPowerOutputContext {
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        backend: &mut state.heater_power_backend,
        hold_pps_governor: &mut state.hold_pps_governor,
        manual_pps: &mut state.manual_pps_state,
        pd_observation: current_pd_observation,
        measured_heater_mv: state.latest_vin_mv,
        current_temp_c: state.latest_temp_c,
        duty_percent: requested_duty_percent,
        heater_enabled: if force_output_off {
            false
        } else if thermal_plant_calibration_running {
            state.calibration_runtime_state.heater_enabled
        } else {
            state.ui_state.heater_enabled
        },
        control_phase: pid_snapshot.phase,
        control_error_c: pid_snapshot.error_c,
        filtered_slope_c_per_s: pid_snapshot.filtered_slope_c_per_s,
        warmup_soft_start_percent: pid_snapshot.warmup_soft_start_percent,
        last_physical_duty_percent: &mut state.last_heater_duty,
        preview_heater_curve: preview_heater_curve_config(state.preview_heater_curve.as_ref()),
        memory_config: &state.memory_config,
        active_thermal_settings,
        now_ms: elapsed_ms,
    })
    .await;
    state.heater_control_timing.cycle_ms = Instant::now()
        .as_millis()
        .saturating_sub(state.runtime_started_ms)
        .saturating_sub(control_started_ms)
        .min(u64::from(u16::MAX)) as u16;
    output_changed
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_persist_completed_thermal_plant(state: &mut RuntimeLoopState) {
    let commit_result = commit_memory_config_now(
        &mut state.eeprom_i2c,
        CommitMemoryConfigInput {
            memory_sequence: &mut state.memory_sequence,
            memory_config: &state.memory_config,
            domains_to_write: PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
            #[cfg(feature = "web_serial")]
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            #[cfg(not(feature = "web_serial"))]
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await;
    match commit_result {
        Ok(()) => {
            copy_persisted_domains(
                &mut state.last_persisted_memory_config,
                &state.memory_config,
                PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
            );
            state.memory_commit_due_ms = None;
        }
        Err(error) => {
            let code = error.code();
            restore_persisted_memory_domains(
                &mut state.memory_config,
                &mut state.ui_state,
                &state.last_persisted_memory_config,
                PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
            );
            mark_eeprom_required(
                &mut state.ui_state,
                &mut state.calibration_runtime_state,
                &mut state.manual_pps_state,
                &mut state.memory_commit_due_ms,
                Some(persistence_fault_from_commit(error)),
            );
            calibration_job_fail(
                &mut state.calibration_runtime_state,
                ManualPpsError::WriteFailed,
                false,
                &mut state.manual_pps_state,
            );
            info!("thermal plant activation commit failed reason={=str}", code);
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_reconcile_heater_status(
    state: &mut RuntimeLoopState,
    current_pd_observation: Option<PdStatusObservation>,
) -> bool {
    let mut needs_redraw = false;
    if state.ui_state.manual_pps_enabled != state.manual_pps_state.enabled {
        state.ui_state.manual_pps_enabled = state.manual_pps_state.enabled;
        needs_redraw = true;
    }
    let next_pd_contract_mv = effective_pd_contract_mv(
        &state.manual_pps_state,
        current_pd_observation,
        state.heater_power_backend,
    );
    if state.ui_state.pd_contract_mv != next_pd_contract_mv {
        state.ui_state.pd_contract_mv = next_pd_contract_mv;
        needs_redraw = true;
    }
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_log_heater_cycle(
    state: &RuntimeLoopState,
    pid_snapshot: HeaterPidSnapshot,
    requested_duty_percent: u8,
    current_pd_observation: Option<PdStatusObservation>,
) {
    let next_pd_contract_mv = effective_pd_contract_mv(
        &state.manual_pps_state,
        current_pd_observation,
        state.heater_power_backend,
    );
    info!(
        "heater loop set_c={=i16} temp_c={=f32} control={=u8}% physical={=u8}% pd_mv={=u16} backend={=str} mos_gate={=u8}% error_c={=f32} control_error_c={=f32} temp_avg_c={=f32} phase={=str} arm={=bool} fault={=str}",
        state.ui_state.target_temp_c,
        state.latest_temp_c,
        requested_duty_percent,
        requested_duty_percent,
        next_pd_contract_mv,
        state.heater_power_backend.label(),
        state.last_heater_duty,
        pid_snapshot.error_c,
        pid_snapshot.control_error_c,
        pid_snapshot.filtered_temp_c,
        pid_snapshot.phase.label(),
        state.ui_state.heater_enabled,
        state
            .heater_controller
            .fault_latched()
            .map(|reason| reason.label())
            .unwrap_or("none"),
    );
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_commit_deferred_memory(state: &mut RuntimeLoopState, elapsed_ms: u64) {
    discard_deferred_memory_commit_for_incompatible_eeprom(
        state.ui_state.persistence_locked(),
        &mut state.memory_commit_due_ms,
    );
    #[cfg(feature = "web_serial")]
    let eeprom_snapshot_active = state.transport.eeprom_snapshot_session.active;
    #[cfg(not(feature = "web_serial"))]
    let eeprom_snapshot_active = false;
    if eeprom_snapshot_active
        || state
            .memory_commit_due_ms
            .is_none_or(|due_ms| elapsed_ms < due_ms)
    {
        return;
    }
    state.memory_commit_due_ms = None;
    let commit_domains =
        persist_domain_mask_between(&state.memory_config, &state.last_persisted_memory_config);
    let commit_result = commit_memory_config_now(
        &mut state.eeprom_i2c,
        CommitMemoryConfigInput {
            memory_sequence: &mut state.memory_sequence,
            memory_config: &state.memory_config,
            domains_to_write: commit_domains,
            #[cfg(feature = "web_serial")]
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            #[cfg(not(feature = "web_serial"))]
            persistence_log_sink: &mut state.transport.persistence_log_sink,
            record_staging: state.eeprom_record_staging,
        },
    )
    .await;
    match commit_result {
        Ok(()) => copy_persisted_domains(
            &mut state.last_persisted_memory_config,
            &state.memory_config,
            commit_domains,
        ),
        Err(error) => runtime_handle_memory_commit_failure(state, error, commit_domains),
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_handle_memory_commit_failure(
    state: &mut RuntimeLoopState,
    error: MemoryCommitFailure,
    commit_domains: PersistDomainMask,
) {
    let requires_heater_lock = memory_failure_requires_heater_lock(error);
    let fault = persistence_fault_from_commit(error);
    if requires_heater_lock {
        restore_persisted_memory_domains(
            &mut state.memory_config,
            &mut state.ui_state,
            &state.last_persisted_memory_config,
            commit_domains,
        );
        mark_eeprom_required(
            &mut state.ui_state,
            &mut state.calibration_runtime_state,
            &mut state.manual_pps_state,
            &mut state.memory_commit_due_ms,
            Some(fault),
        );
    } else {
        state.ui_state.persistence_fault = Some(fault);
        state.ui_state.persistence_fault_attention_pending = true;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_reconcile_persistence_and_cooling(state: &mut RuntimeLoopState) -> bool {
    let mut needs_redraw = false;
    if state.ui_state.eeprom_required && !state.eeprom_required {
        state.eeprom_required = true;
        state.persistence_source = "none";
        state.persistence_record_state = "unavailable";
    } else if state.ui_state.eeprom_data_incompatible && state.persistence_record_state == "valid" {
        state.persistence_record_state = "incompatible";
    }
    let (next_latched, next_armed, lock_just_latched) = reconcile_cooling_disabled_lock(
        state.ui_state.active_cooling_enabled,
        state.latest_display_temp_i16,
        is_sensor_fault(state.current_rtd_fault),
        state.cooling_disabled_lock_latched,
        state.cooling_disabled_lock_armed,
    );
    if state.cooling_disabled_lock_latched != next_latched
        || state.cooling_disabled_lock_armed != next_armed
    {
        state.cooling_disabled_lock_latched = next_latched;
        state.cooling_disabled_lock_armed = next_armed;
        needs_redraw = true;
    }
    if lock_just_latched {
        state.ui_state.heater_enabled = false;
        info!(
            "cooling-disabled safety lock latched temp_c={=i16}",
            state.latest_display_temp_i16
        );
    }
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_force_heater_safe_off(
    state: &mut RuntimeLoopState,
    active_thermal_settings: ThermalControlProfileSettings,
    elapsed_ms: u64,
) -> bool {
    if state.ui_state.heater_enabled
        || (state.last_heater_duty == 0 && state.ui_state.heater_output_percent == 0)
    {
        return false;
    }
    state.ui_state.heater_output_percent = 0;
    let _ = apply_heater_power_output(HeaterPowerOutputContext {
        pd_port: &state.pd_port,
        heater_pwm: &mut state.heater_pwm,
        backend: &mut state.heater_power_backend,
        hold_pps_governor: &mut state.hold_pps_governor,
        manual_pps: &mut state.manual_pps_state,
        pd_observation: state.last_pd_observation,
        measured_heater_mv: state.latest_vin_mv,
        current_temp_c: state.latest_temp_c,
        duty_percent: 0,
        heater_enabled: false,
        control_phase: HeaterControlPhase::Warmup,
        control_error_c: -1.0,
        filtered_slope_c_per_s: -1.0,
        warmup_soft_start_percent: 0,
        last_physical_duty_percent: &mut state.last_heater_duty,
        preview_heater_curve: preview_heater_curve_config(state.preview_heater_curve.as_ref()),
        memory_config: &state.memory_config,
        active_thermal_settings,
        now_ms: elapsed_ms,
    })
    .await;
    let next_pd_contract_mv = effective_pd_contract_mv(
        &state.manual_pps_state,
        state.last_pd_observation,
        state.heater_power_backend,
    );
    state.ui_state.pd_contract_mv = next_pd_contract_mv;
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn runtime_update_fan_and_ui(state: &mut RuntimeLoopState, elapsed_ms: u64) -> bool {
    let mut fan_decision = fan_policy_decision_with_modes(
        state.latest_display_temp_i16,
        elapsed_ms,
        state.ui_state.heater_enabled,
        state.heater_enabled_last_cycle && !state.ui_state.heater_enabled,
        state.ui_state.post_heat_cooling_mode,
        state.ui_state.heating_fan_guard_mode,
        (
            state.fan_policy_state,
            is_sensor_fault(state.current_rtd_fault),
        ),
    );
    if state.calibration_runtime_state.mode == CalibrationMode::ThermalPlant
        && state.calibration_runtime_state.job.status == CalibrationJobStatus::Running
    {
        fan_decision = FanPolicyDecision {
            state: FanPolicyState::Disabled,
            command: FanHardwareCommand::disabled(),
            display_state: FanDisplayState::Off,
            source: FanPolicySource::Idle,
            output_level: FanOutputLevel::Off,
        };
    }
    if let Some(forced_fan_state) = overtemp_forced_fan_state(
        state.latest_display_temp_i16,
        state.overtemp_forced_fan_active,
    ) {
        let command = forced_fan_state.command(elapsed_ms);
        fan_decision = FanPolicyDecision {
            state: forced_fan_state,
            command,
            display_state: fan_display_state_for_policy(
                FanPolicySource::Safety,
                state.ui_state.post_heat_cooling_mode,
                command,
            ),
            source: FanPolicySource::Safety,
            output_level: fan_output_level_for_command(command),
        };
    }
    state.fan_policy_state = fan_decision.state;
    state.fan_command = fan_decision.command;
    state.heater_enabled_last_cycle = state.ui_state.heater_enabled;
    apply_fan_output(
        &mut state.fan_enable,
        &mut state.fan_pwm,
        state.fan_command,
        &mut state.last_fan_command,
    );
    let persistence_locked = state.ui_state.persistence_locked();
    sync_frontpanel_runtime_state(
        &mut state.ui_state,
        fan_decision,
        next_heater_lock_reason_with_persistence(
            persistence_locked,
            state.heater_controller.fault_latched(),
            state.cooling_disabled_lock_latched,
            thermal_model_heater_allowed(
                &state.memory_config,
                state.calibration_runtime_state,
                state.manual_pps_state,
            ),
            state.pd_contract_ready,
        ),
        elapsed_ms,
    )
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_reconcile_network_and_status(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let mut needs_redraw = false;
    #[cfg(feature = "net_http")]
    let runtime_network_summary = flux_purr_firmware::net::lan_network_summary().await;
    #[cfg(feature = "net_http")]
    if state
        .ui_state
        .apply_network_summary(runtime_network_summary)
    {
        needs_redraw = true;
    }
    if maybe_play_protection_alarm(
        is_overtemp_fault(state.current_rtd_fault),
        &mut state.protection_alarm,
        &mut state.buzzer,
        elapsed_ms,
    ) {
        info!("protection alarm -> replay");
    }
    if maybe_play_attention_reminder(
        state.attention_pending_after_fault_clear,
        is_overtemp_fault(state.current_rtd_fault),
        &mut state.next_attention_reminder_ms,
        &mut state.buzzer,
        elapsed_ms,
    ) {
        info!("fault attention reminder -> chirp");
    }
    let status_light_elapsed_ms = Instant::now()
        .as_millis()
        .saturating_sub(state.status_light_started_ms);
    let status_light_state = select_status_light_state(StatusLightInputs {
        booting: status_light_elapsed_ms < STATUS_LIGHT_BOOT_DURATION_MS,
        thermal_runaway: is_overtemp_fault(state.current_rtd_fault),
        thermal_runaway_attention_pending: state.attention_pending_after_fault_clear,
        sensor_fault: is_sensor_fault(state.current_rtd_fault),
        cooling_disabled_overtemp: state.cooling_disabled_lock_latched,
        heater_interlocked: matches!(
            state.ui_state.heater_lock_reason,
            Some(
                HeaterLockReason::PdContractUnavailable
                    | HeaterLockReason::ThermalModelMissingForSourceClass
            )
        ),
        calibration_active: state.calibration_runtime_state.mode != CalibrationMode::Off,
        heater_enabled: state.ui_state.heater_enabled,
        fan_enabled: state.fan_command.enabled,
    });
    set_status_light_state(status_light_state);
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_control_heater(state: &mut RuntimeLoopState, elapsed_ms: u64) -> bool {
    let mut needs_redraw = false;
    if elapsed_ms >= state.next_control_deadline_ms {
        let control_started_ms = elapsed_ms;
        state.heater_control_timing.interval_ms = control_started_ms
            .saturating_sub(state.last_control_ms)
            .min(u64::from(u16::MAX)) as u16;
        state.last_control_ms = elapsed_ms;
        state.next_control_deadline_ms =
            next_heater_control_deadline_ms(state.next_control_deadline_ms, control_started_ms);
        let active_thermal_control_profile = active_thermal_control_profile(
            &state.memory_config,
            state.thermal_control_profile_preview,
            &state.manual_pps_state,
        );
        let active_thermal_settings = active_thermal_control_profile
            .filter(|_| state.calibration_runtime_state.mode != CalibrationMode::Off)
            .map(|profile| profile.settings)
            .unwrap_or_default();

        let current_request_mv = state.heater_power_backend.pd_request_mv();
        let (rtd_sample, sensor_needs_redraw) =
            runtime_read_heater_sensors(state, current_request_mv).await;
        needs_redraw |= sensor_needs_redraw;
        let (calibration_live_rtd_temp_c, rtd_needs_redraw) =
            runtime_apply_rtd_sample(state, rtd_sample, current_request_mv, elapsed_ms);
        needs_redraw |= rtd_needs_redraw;

        let current_pd_observation = state.last_pd_observation;
        needs_redraw |= runtime_update_fault_attention(state, elapsed_ms);
        let fusb302b_capabilities_changed = runtime_refresh_source_capabilities(state);
        if fusb302b_capabilities_changed {
            needs_redraw = true;
        }
        let memory_before_calibration_job = state.memory_config.clone();
        let thermal_plant_was_running = runtime_update_calibration_job(
            state,
            current_pd_observation,
            calibration_live_rtd_temp_c,
        );
        if fusb302b_capabilities_changed {
            disarm_calibration_after_capability_refresh(
                &mut state.calibration_runtime_state,
                &mut state.manual_pps_state,
            );
        }
        let thermal_plant_completed = state.calibration_runtime_state.mode == CalibrationMode::Off
            && state.calibration_runtime_state.job.kind == Some(CalibrationJobKind::ThermalPlant)
            && state.calibration_runtime_state.job.status == CalibrationJobStatus::Completed;
        if state.memory_config != memory_before_calibration_job {
            if thermal_plant_completed {
                runtime_persist_completed_thermal_plant(state).await;
            } else {
                state.memory_commit_due_ms =
                    Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            }
        }
        let (force_thermal_plant_output_off, arming_needs_redraw) =
            runtime_reconcile_heater_arming(
                state,
                calibration_live_rtd_temp_c,
                thermal_plant_was_running,
            )
            .await;
        needs_redraw |= arming_needs_redraw;
        let (pid_snapshot, requested_duty_percent, controller_needs_redraw) =
            runtime_update_heater_controller(state, elapsed_ms, force_thermal_plant_output_off);
        needs_redraw |= controller_needs_redraw;
        let thermal_plant_calibration_running = state.calibration_runtime_state.mode
            == CalibrationMode::ThermalPlant
            && state.calibration_runtime_state.job.status == CalibrationJobStatus::Running;
        needs_redraw |= runtime_apply_heater_output(RuntimeHeaterOutputContext {
            state,
            pid_snapshot,
            requested_duty_percent,
            force_output_off: force_thermal_plant_output_off,
            thermal_plant_calibration_running,
            active_thermal_settings,
            current_pd_observation,
            control_started_ms,
            elapsed_ms,
        })
        .await;
        needs_redraw |= runtime_reconcile_heater_status(state, current_pd_observation);
        runtime_log_heater_cycle(
            state,
            pid_snapshot,
            requested_duty_percent,
            current_pd_observation,
        );
    }

    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_process_pending_safety(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let active_thermal_settings = state.active_thermal_settings;
    let mut needs_redraw = runtime_reconcile_persistence_and_cooling(state);
    needs_redraw |= runtime_force_heater_safe_off(state, active_thermal_settings, elapsed_ms).await;
    needs_redraw |= runtime_update_fan_and_ui(state, elapsed_ms);
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_persist_and_update_safety(
    state: &mut RuntimeLoopState,
    elapsed_ms: u64,
) -> bool {
    let active_thermal_settings = state.active_thermal_settings;
    let mut needs_redraw = false;
    runtime_commit_deferred_memory(state, elapsed_ms).await;

    needs_redraw |= runtime_reconcile_persistence_and_cooling(state);

    needs_redraw |= runtime_force_heater_safe_off(state, active_thermal_settings, elapsed_ms).await;

    needs_redraw |= runtime_update_fan_and_ui(state, elapsed_ms);

    needs_redraw |= runtime_reconcile_network_and_status(state, elapsed_ms).await;
    state.ui_refresh_pending |= needs_redraw;
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn runtime_refresh_display(state: &mut RuntimeLoopState, elapsed_ms: u64) {
    if state.ui_refresh_pending && elapsed_ms >= state.next_ui_refresh_ms {
        let display_flush_result = run_display_operation_with_snapshot_and_heater(
            flush_ui(&mut state.display, state.canvas, &state.ui_state),
            &state.pd_port,
            &mut state.last_pd_observation,
            &mut state.heater_pwm,
            &mut state.last_heater_duty,
        )
        .await;
        let pd_redraw_after_flush = apply_pd_contract_observation(
            state.last_pd_observation,
            &mut state.pd_contract_ready,
            &mut state.ui_state,
            &mut state.calibration_runtime_state,
            &mut state.manual_pps_state,
            &mut state.heater_pwm,
            &mut state.last_heater_duty,
        );
        state.ui_refresh_pending = pd_redraw_after_flush;
        match display_flush_result {
            Some(Ok(())) => log_ui_state(&state.ui_state),
            Some(Err(_)) | None => {
                // A failed or timed-out async SPI transaction must not be
                // reused: its chip-select and panel state can be incomplete
                // after an interrupted transfer. Stop heat first, force the
                // source back toward fixed PD, keep cooling active, then
                // remain in the USB-readable terminal recovery path.
                warn!("frontpanel UI refresh failed; entering recovery");
                state.manual_pps_state.clear();
                state.calibration_runtime_state.heater_enabled = false;
                state.calibration_runtime_state.mode = CalibrationMode::Off;
                state
                    .calibration_runtime_state
                    .immediate_heater_disarm_pending = true;
                state.ui_state.heater_enabled = false;
                state.ui_state.heater_output_percent = 0;
                apply_heater_duty(&mut state.heater_pwm, 0, &mut state.last_heater_duty);
                let _ = disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
                    calibration_runtime_state: &mut state.calibration_runtime_state,
                    backend: &mut state.heater_power_backend,
                    manual_pps: &mut state.manual_pps_state,
                    pd_port: &state.pd_port,
                    heater_pwm: &mut state.heater_pwm,
                    hold_pps_governor: &mut state.hold_pps_governor,
                    ui_state: &mut state.ui_state,
                    last_heater_duty: &mut state.last_heater_duty,
                    measured_vin_mv: state.latest_vin_mv,
                })
                .await;
                apply_fan_output(
                    &mut state.fan_enable,
                    &mut state.fan_pwm,
                    FanHardwareCommand::from_profile(FanVoltageProfile::Full),
                    &mut state.last_fan_command,
                );
                #[cfg(feature = "web_serial")]
                run_usb_recovery_control_loop(
                    &mut state.transport.usb_serial,
                    &mut *state.transport.usb_rx_line,
                    state.transport.usb_tx_buf,
                    &state.memory_config,
                    StatusLightState::HeaterInterlocked,
                    UsbRecoveryPhase::RuntimeFault,
                )
                .await;

                #[cfg(not(feature = "web_serial"))]
                panic!("frontpanel UI refresh timed out");
            }
        }
        state.next_ui_refresh_ms =
            elapsed_ms.saturating_add(DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS);
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run_runtime_loop(mut state: Box<RuntimeLoopState>) -> ! {
    loop {
        #[cfg(feature = "web_serial")]
        embassy_futures::yield_now().await;
        record_runtime_heartbeat();
        let elapsed_ms = Instant::now()
            .as_millis()
            .saturating_sub(state.runtime_started_ms);
        let mut needs_redraw = runtime_apply_pd_snapshot(&mut state);
        let input = runtime_process_input(&mut state, elapsed_ms).await;
        needs_redraw |= input.needs_redraw;
        if input.skip_iteration {
            needs_redraw |= runtime_process_frontpanel_input(
                &mut state,
                input.sample,
                elapsed_ms,
                input.pairing_opened_by_usb,
            )
            .await;
            needs_redraw |= runtime_control_heater(&mut state, elapsed_ms).await;
            needs_redraw |= runtime_process_pending_safety(&mut state, elapsed_ms).await;
            state.ui_refresh_pending |= needs_redraw;
            continue;
        }
        needs_redraw |= runtime_process_frontpanel_input(
            &mut state,
            input.sample,
            elapsed_ms,
            input.pairing_opened_by_usb,
        )
        .await;
        needs_redraw |= runtime_control_heater(&mut state, elapsed_ms).await;
        needs_redraw |= runtime_persist_and_update_safety(&mut state, elapsed_ms).await;
        state.ui_refresh_pending |= needs_redraw;
        runtime_refresh_display(&mut state, elapsed_ms).await;
    }
}

#[cfg(not(target_arch = "xtensa"))]
pub fn host_main() {
    println!(
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3,web_serial,net_http"
    );
}
