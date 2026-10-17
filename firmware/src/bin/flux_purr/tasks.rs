#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
const PPS_REQUEST_RETRY_DELAY_MS: u64 = 200;

#[cfg(target_arch = "xtensa")]
fn request_working_fixed_contract(
    pd_port: &PdPort,
    request_mv: u16,
    request_ma: u16,
) -> PdRequestState {
    if working_fixed_contract_confirmed(
        pd_port.snapshot().settled_observation(),
        request_mv,
        request_ma,
    ) {
        return PdRequestState::Confirmed;
    }
    let Ok(request) = PdContractRequest::fixed(request_mv, request_ma) else {
        return PdRequestState::Failed;
    };
    pd_port.request_fixed_contract(request)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn working_fixed_contract_confirmed(
    observation: Option<PdStatusObservation>,
    request_mv: u16,
    request_ma: u16,
) -> bool {
    pd_observation_confirms_fixed_contract(observation, request_mv)
        && observation.is_some_and(|observation| observation.current_ma >= request_ma)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn working_pps_request_confirmed(
    observation: Option<PdStatusObservation>,
    request: PdContractRequest,
) -> bool {
    observation.is_some_and(|observation| {
        observation.status.pd_active
            && observation.contract.kind == ContractKind::Pps
            && observation.contract.voltage_mv == request.voltage_mv()
            && observation.contract.current_ma >= request.operating_current_ma()
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn ticket_outcome_confirms_request(
    outcome: TicketOutcome,
    request: PdContractRequest,
) -> bool {
    matches!(
        outcome,
        TicketOutcome::Confirmed(contract)
            if contract.mode == request.mode()
                && contract.voltage_mv == request.voltage_mv()
                && contract.operating_current_ma >= request.operating_current_ma()
                && request.pps_range().is_none_or(|range| contract.pps_range == Some(range))
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_pps_transition_requires_request(
    voltage_changed: bool,
    mode_changed: bool,
    safety_recovery_required: bool,
    request_transition_pending: bool,
) -> bool {
    (voltage_changed || mode_changed || safety_recovery_required) && !request_transition_pending
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_pps_transition_should_clear_pwm(
    voltage_changed: bool,
    mode_changed: bool,
    safety_recovery_required: bool,
    request_transition_pending: bool,
    gate_duty_percent: u8,
    blank_heater: bool,
) -> bool {
    gate_duty_percent == 0
        || blank_heater
        || (automatic_pps_transition_requires_request(
            voltage_changed,
            mode_changed,
            safety_recovery_required,
            request_transition_pending,
        ) && safety_recovery_required)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_pps_transition_should_restore_pwm(
    request_confirmed: bool,
    safety_recovery_required: bool,
    blank_heater: bool,
    gate_duty_percent: u8,
) -> bool {
    request_confirmed && !safety_recovery_required && !blank_heater && gate_duty_percent > 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn calibration_contract_matches(
    observation: Option<PdStatusObservation>,
    target_mv: Option<u16>,
    target_ma: Option<u16>,
    pending: bool,
) -> bool {
    !pending
        && target_mv.is_some()
        && target_ma.is_some()
        && observation.is_some_and(|observed| {
            observed.status.pd_active
                && observed.contract.kind == ContractKind::Pps
                && observed.contract_voltage_mv == target_mv
                && Some(observed.current_ma) == target_ma
        })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn supply_scan_contract_matches(
    snapshot: PdServiceSnapshot,
    manual: &ManualPpsState,
) -> bool {
    calibration_contract_matches(
        snapshot.settled_observation(),
        manual.target_mv,
        manual.target_ma,
        manual.pending_power_ticket.is_some(),
    ) && manual_pps_contract_request(manual, true)
        .is_some_and(|request| request_matches_active(request, snapshot.active))
}

#[cfg(target_arch = "xtensa")]
fn automatic_response_sample_ready<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    observed_mv: u16,
) -> bool {
    let HeaterPowerBackend::PpsMos {
        current_request_mv,
        next_request_at_ms,
        ..
    } = *context.backend
    else {
        return false;
    };
    automatic_supply_response_sample_ready(
        observed_mv,
        current_request_mv,
        next_request_at_ms,
        context.pd_port.snapshot().transition_pending,
        context.now_ms,
    )
}

#[cfg(target_arch = "xtensa")]
fn automatic_output_response_fault<PWM>(context: &mut HeaterPowerOutputContext<'_, PWM>) -> bool {
    if context.normal_auto {
        // The runtime cache is only a status hint. Before any automatic output
        // decision, use the PD service's confirmed snapshot so a capability or
        // contract change cannot authorize PWM for one more control cycle.
        let snapshot = context.pd_port.snapshot();
        context.pd_observation = snapshot.conducting_observation();
        context.source_capabilities = snapshot.source;
        if flux_purr_firmware::heater_supply::wifi_hardware_started().is_none() {
            return true;
        }
        context
            .manual_pps
            .refresh_fusb302b_capabilities_preserving_intent(snapshot.capabilities);
    }
    let response_matches = context.pd_observation.is_none_or(|observed| {
        !automatic_response_sample_ready(context, observed.contract.voltage_mv)
            || observed.contract.kind != ContractKind::Pps
            || context.manual_pps.automatic_voltage_response_matches(
                context.memory_config,
                context.current_temp_c,
                observed.contract.voltage_mv,
                observed.current_ma.min(observed.contract.current_ma),
                context.vin_raw_adc_mv,
                context
                    .pd_port
                    .snapshot()
                    .active
                    .and_then(|active| active.pps_range),
            )
    });
    let response_latched = automatic_supply_response_fault_latched(
        *context.automatic_supply_response_latched,
        context.normal_auto,
        context.heater_enabled,
        *context.last_physical_duty_percent,
        response_matches,
    );
    if response_latched && !*context.automatic_supply_response_latched {
        info!("automatic heater paused reason=supply-response-mismatch; stop and re-arm required");
    }
    *context.automatic_supply_response_latched = response_latched;
    response_latched
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_heater_power_output<PWM>(
    mut context: HeaterPowerOutputContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    if flash_preparation::is_active() {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return false;
    }
    if automatic_output_response_fault(&mut context) {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return false;
    }
    if !context.normal_auto
        && let Some(pending) = context.pending_automatic_power_request.take()
    {
        context.pd_port.discard_ticket(pending.ticket);
    }
    let manual_pps_active = context.manual_pps.enabled;
    let _ = release_terminal_fixed_pd_disarm_for_new_work(
        context.backend,
        context.heater_enabled,
        manual_pps_active,
        context.tuning_power_stage,
    );
    if context.backend.terminal_fixed_pd_disarmed() {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return false;
    }
    let Some(manual_pps_request_changed) = prepare_manual_pps(&mut context).await else {
        return false;
    };
    if context.manual_pps.owner == ManualPpsOwner::Calibration && context.manual_pps.enabled {
        let previous_duty_percent = *context.last_physical_duty_percent;
        apply_heater_duty(
            context.heater_pwm,
            if context.heater_enabled
                && (!context.supply_scan
                    || supply_scan_contract_matches(
                        context
                            .pd_port
                            .snapshot()
                            .with_fresh_observation(context.now_ms),
                        context.manual_pps,
                    ))
            {
                context.duty_percent
            } else {
                0
            },
            context.last_physical_duty_percent,
        );
        return manual_pps_request_changed
            || *context.last_physical_duty_percent != previous_duty_percent;
    }
    reset_backend_after_manual_pps_restore(&mut context);
    if automatic_heater_power_is_idle(
        context.heater_enabled,
        context.duty_percent,
        context.manual_pps.enabled,
        context.tuning_power_stage,
    ) {
        if let Some(pending) = context.pending_automatic_power_request.take() {
            context.pd_port.discard_ticket(pending.ticket);
        }
        // The runtime owns standby and fan-only supply. Forget cached working
        // power so the next heating intent confirms its supply before PWM.
        reset_automatic_heater_backend(context.backend);
        context.hold_pps_governor.reset();
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return false;
    }
    match *context.backend {
        HeaterPowerBackend::FixedPdPwmFallback { .. } => {
            apply_fixed_pd_backend(context, manual_pps_active).await
        }
        HeaterPowerBackend::PpsMos { .. } => apply_pps_backend(context, manual_pps_active).await,
    }
}

#[cfg(target_arch = "xtensa")]
fn prepare_automatic_idle_restore<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    match context.manual_pps.automatic_restore_phase() {
        AutomaticRestorePhase::NotRequired | AutomaticRestorePhase::ResetBackend => {
            return Some(false);
        }
        AutomaticRestorePhase::AwaitIdle(ticket) => {
            if let Some(outcome) = context.pd_port.try_take_ticket(ticket) {
                context.manual_pps.complete_automatic_restore(outcome);
                if context.manual_pps.automatic_restore_phase()
                    == AutomaticRestorePhase::ResetBackend
                {
                    return Some(false);
                }
            }
        }
        AutomaticRestorePhase::RequestIdle => {
            match context.pd_port.restore_automatic_idle_contract() {
                PdRequestState::Confirmed => {
                    context.manual_pps.automatic_restore_confirmed = true;
                    return Some(false);
                }
                PdRequestState::Pending(ticket) => {
                    context.manual_pps.automatic_restore_ticket = Some(ticket);
                }
                PdRequestState::Failed => {}
            }
        }
    }
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    None
}

#[cfg(target_arch = "xtensa")]
fn complete_manual_pps_ticket<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    ticket: PowerTicket,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    let outcome = context.pd_port.try_take_ticket(ticket)?;
    let confirmed_mv = context.manual_pps.pending_power_request_mv;
    context.manual_pps.pending_power_ticket = None;
    context.manual_pps.pending_power_request_mv = None;
    match outcome {
        TicketOutcome::Confirmed(_) => {
            context.manual_pps.applied_mv = confirmed_mv;
        }
        _ => {
            context.manual_pps.fail(ManualPpsError::WriteFailed);
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        }
    }
    Some(false)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn prepare_manual_pps<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    if context.manual_pps.enabled
        && let Some(ticket) = context.manual_pps.automatic_restore_ticket.take()
    {
        context.pd_port.discard_ticket(ticket);
    }
    if let Some(ticket) = context.manual_pps.pending_power_ticket {
        if !context.manual_pps.enabled {
            context.pd_port.discard_ticket(ticket);
            context.manual_pps.pending_power_ticket = None;
            context.manual_pps.pending_power_request_mv = None;
        } else {
            return complete_manual_pps_ticket(context, ticket);
        }
    }
    if !context.manual_pps.enabled {
        return prepare_automatic_idle_restore(context);
    }
    context.hold_pps_governor.reset();
    let target_mv = match require_manual_pps_target(context.manual_pps) {
        Ok(target_mv) => target_mv,
        Err(_) => {
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            return None;
        }
    };
    if context.pd_port.controller_kind() != ControllerKind::Fusb302b
        && !context
            .pd_observation
            .is_some_and(|observation| observation.status.pd_active)
    {
        context.manual_pps.fail(ManualPpsError::PdNotReady);
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return None;
    }
    if !manual_pps_request_required(
        *context.manual_pps,
        context.pd_port.controller_kind(),
        context.pd_observation,
    ) && (!context.supply_scan
        || supply_scan_contract_matches(
            context
                .pd_port
                .snapshot()
                .with_fresh_observation(context.now_ms),
            context.manual_pps,
        ))
    {
        return Some(false);
    }
    let request = manual_pps_contract_request(context.manual_pps, context.supply_scan);
    let Some(request) = request else {
        context.manual_pps.fail(ManualPpsError::InvalidCurrent);
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        return None;
    };
    if context.supply_scan {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    }
    match context.pd_port.request_pps_contract_for(
        if context.manual_pps.owner == ManualPpsOwner::Calibration {
            PowerIntentOwner::Calibration
        } else {
            PowerIntentOwner::ManualOperator
        },
        request,
    ) {
        PdRequestState::Pending(ticket) => {
            context.manual_pps.pending_power_ticket = Some(ticket);
            context.manual_pps.pending_power_request_mv = Some(target_mv);
            if context.pd_port.pps_request_requires_heater_pause(request) {
                apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            }
            None
        }
        PdRequestState::Confirmed => {
            context.manual_pps.applied_mv = Some(target_mv);
            Some(true)
        }
        PdRequestState::Failed => {
            fail_manual_pps_write(context);
            Some(false)
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn fail_manual_pps_write<PWM: SetDutyCycle>(context: &mut HeaterPowerOutputContext<'_, PWM>) {
    context.manual_pps.fail(ManualPpsError::WriteFailed);
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    info!(
        "manual pps override cleared reason={=str}",
        ManualPpsError::WriteFailed.code()
    );
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn manual_pps_contract_request(
    manual: &ManualPpsState,
    supply_scan: bool,
) -> Option<PdContractRequest> {
    let voltage = manual.target_mv?;
    let current = manual.target_ma?;
    let request = if supply_scan && current < MIN_HEATER_CONTRACT_MA {
        PdContractRequest::calibration_pps(voltage, current)
    } else {
        PdContractRequest::pps(voltage, current)
    }
    .ok()?;
    if supply_scan {
        request.within_pps_range(manual.supply_scan_range?)
    } else {
        Some(request)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn require_manual_pps_target(
    manual_pps: &mut ManualPpsState,
) -> Result<u16, ManualPpsError> {
    let Some(target_mv) = manual_pps.target_mv else {
        manual_pps.fail(ManualPpsError::InvalidVoltage);
        return Err(ManualPpsError::InvalidVoltage);
    };
    Ok(target_mv)
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn reset_backend_after_manual_pps_restore<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) where
    PWM: SetDutyCycle,
{
    if context.manual_pps.automatic_restore_phase() != AutomaticRestorePhase::ResetBackend
        || !context.manual_pps.consume_automatic_restore_pending()
    {
        return;
    }
    reset_automatic_heater_backend(context.backend);
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_power_is_idle(
    heater_enabled: bool,
    duty_percent: u8,
    manual_pps_enabled: bool,
    tuning_power_stage: bool,
) -> bool {
    !heater_enabled && duty_percent == 0 && !manual_pps_enabled && !tuning_power_stage
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn reset_automatic_heater_backend(backend: &mut HeaterPowerBackend) {
    match *backend {
        HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request,
            terminal_fixed_pd_disarmed,
            ..
        } => {
            *backend = HeaterPowerBackend::FixedPdPwmFallback {
                reason,
                fixed_request_confirmed: false,
                fixed_request,
                terminal_fixed_pd_disarmed,
            };
        }
        HeaterPowerBackend::PpsMos {
            pps_min_mv,
            idle_request_mv,
            pps_max_mv,
            adjustable_max_mv,
            capability_max_ma,
            ..
        } => {
            *backend = HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv,
                pps_max_mv,
                adjustable_max_mv,
                capability_max_ma,
                current_mode: None,
                current_request_mv: idle_request_mv,
                settle_until_ms: None,
                next_request_at_ms: 0,
                current_limit_fixed_pwm_active: false,
                current_limit_fixed_request_confirmed: false,
                terminal_fixed_pd_disarmed: false,
            };
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_fixed_pd_backend<PWM>(
    context: HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
) -> bool
where
    PWM: SetDutyCycle,
{
    if matches!(
        *context.backend,
        HeaterPowerBackend::FixedPdPwmFallback { .. }
    ) {
        return apply_fixed_pd_fallback(context, manual_pps_active).await;
    }
    apply_pps_backend(context, manual_pps_active).await
}

#[cfg(target_arch = "xtensa")]
fn fixed_fallback_duty_percent<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    fixed_request: ch224q::VoltageRequest,
    fixed_request_current_ma: u16,
    negotiated_current_ma: u16,
) -> u8 {
    if context.normal_auto {
        let resistance_ohms = automatic_heater_resistance_ohms(
            context.current_temp_c,
            context.preview_heater_curve,
            context.memory_config,
        );
        let confirmed_mv = context
            .pd_observation
            .filter(|observation| {
                observation.status.pd_active
                    && observation.contract.kind == ContractKind::Fixed
                    && observation.contract_voltage_mv == Some(fixed_request.millivolts())
                    && observation.current_ma >= fixed_request_current_ma
            })
            .and_then(|observation| observation.contract_voltage_mv);
        confirmed_mv.map_or(0, |voltage_mv| {
            automatic_heater_pwm_percent_for_power(
                context.requested_power_mw,
                resistance_ohms,
                voltage_mv,
                context.warmup_soft_start_percent,
            )
        })
    } else {
        fixed_pd_pwm_duty_percent(
            context.duty_percent,
            context.current_temp_c,
            context
                .pd_observation
                .and_then(|observation| observation.contract_voltage_mv)
                .unwrap_or_else(|| fixed_request.millivolts()),
            negotiated_current_ma,
            context.active_thermal_settings.heater_current_reserve_ma,
            context.preview_heater_curve,
            context.memory_config,
        )
    }
}

#[cfg(target_arch = "xtensa")]
fn request_fixed_fallback_contract<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    fixed_request: ch224q::VoltageRequest,
    fixed_request_current_ma: u16,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    match request_working_fixed_contract(
        context.pd_port,
        fixed_request.millivolts(),
        fixed_request_current_ma,
    ) {
        PdRequestState::Confirmed => Some(true),
        PdRequestState::Pending(ticket) => {
            if context.normal_auto {
                let Ok(request) =
                    PdContractRequest::fixed(fixed_request.millivolts(), fixed_request_current_ma)
                else {
                    context.pd_port.discard_ticket(ticket);
                    return Some(false);
                };
                *context.pending_automatic_power_request = Some(PendingAutomaticPowerRequest {
                    started_at_ms: context.now_ms,
                    ticket,
                    request,
                    pause_heater: true,
                });
            } else {
                context.pd_port.discard_ticket(ticket);
            }
            None
        }
        PdRequestState::Failed => Some(false),
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_fixed_pd_fallback<PWM>(
    mut context: HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
) -> bool
where
    PWM: SetDutyCycle,
{
    match poll_pending_automatic_power_ticket(&mut context) {
        AutomaticPowerTicketPoll::Pending => return false,
        AutomaticPowerTicketPoll::None
        | AutomaticPowerTicketPoll::Confirmed
        | AutomaticPowerTicketPoll::Failed => {}
    }
    let HeaterPowerBackend::FixedPdPwmFallback {
        reason,
        fixed_request_confirmed: initial_fixed_request_confirmed,
        fixed_request: initial_fixed_request,
        terminal_fixed_pd_disarmed,
    } = *context.backend
    else {
        return false;
    };
    let (fixed_request, fixed_request_current_ma, mut fixed_request_confirmed) =
        if context.normal_auto {
            let resistance_ohms = automatic_heater_resistance_ohms(
                context.current_temp_c,
                context.preview_heater_curve,
                context.memory_config,
            );
            let Some((request, request_ma)) =
                automatic_heater_fixed_request(context.source_capabilities, resistance_ohms)
            else {
                apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
                info!(
                    "automatic heater paused: no safe Fixed candidate temp_c={=f32}",
                    context.current_temp_c,
                );
                return false;
            };
            let observed_matches = context.pd_observation.is_some_and(|observation| {
                observation.status.pd_active
                    && observation.contract.kind == ContractKind::Fixed
                    && observation.contract_voltage_mv == Some(request.millivolts())
                    && observation.current_ma >= request_ma
            });
            (
                request,
                request_ma,
                initial_fixed_request_confirmed && observed_matches,
            )
        } else {
            (
                initial_fixed_request,
                MIN_HEATER_CONTRACT_MA,
                initial_fixed_request_confirmed,
            )
        };
    if !fixed_request_confirmed && !manual_pps_active {
        let Some(request_confirmed_now) =
            request_fixed_fallback_contract(&mut context, fixed_request, fixed_request_current_ma)
        else {
            return false;
        };
        if !request_confirmed_now {
            return false;
        }
        fixed_request_confirmed = true;
        *context.backend = HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request_confirmed: true,
            fixed_request,
            terminal_fixed_pd_disarmed,
        };
        info!("heater backend fallback fixed-pd contract confirmed");
    }
    if context.normal_auto && fixed_request != initial_fixed_request {
        *context.backend = HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request_confirmed,
            fixed_request,
            terminal_fixed_pd_disarmed,
        };
    }
    let negotiated_current_ma = context
        .pd_observation
        .filter(|observation| observation.status.pd_active)
        .map(|observation| observation.current_ma)
        .unwrap_or(0);
    let safe_duty_percent = fixed_fallback_duty_percent(
        &context,
        fixed_request,
        fixed_request_current_ma,
        negotiated_current_ma,
    );
    apply_heater_duty(
        context.heater_pwm,
        apply_warmup_soft_start(safe_duty_percent, context.warmup_soft_start_percent),
        context.last_physical_duty_percent,
    );
    false
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PpsOperatingLimits {
    pps_min_mv: u16,
    adjustable_max_mv: u16,
    capability_max_ma: u16,
    request_current_ma: u16,
    current_request_mv: u16,
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn pps_operating_limits<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
) -> Option<PpsOperatingLimits>
where
    PWM: SetDutyCycle,
{
    match *context.backend {
        HeaterPowerBackend::PpsMos {
            pps_min_mv,
            adjustable_max_mv,
            capability_max_ma,
            current_request_mv,
            ..
        } => {
            if context.normal_auto {
                let candidate = context.manual_pps.automatic_heater_pps_candidate(
                    context.current_temp_c,
                    context.preview_heater_curve,
                    context.memory_config,
                    Some(current_request_mv),
                )?;
                Some(PpsOperatingLimits {
                    pps_min_mv: candidate.min_mv,
                    adjustable_max_mv: candidate.allowed_mv,
                    capability_max_ma: candidate.current_ma,
                    request_current_ma: candidate.current_ma,
                    current_request_mv,
                })
            } else {
                Some(PpsOperatingLimits {
                    pps_min_mv,
                    adjustable_max_mv,
                    capability_max_ma,
                    request_current_ma: capability_max_ma,
                    current_request_mv,
                })
            }
        }
        HeaterPowerBackend::FixedPdPwmFallback { .. } => None,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn set_pps_current_limit_backend(
    backend: &mut HeaterPowerBackend,
    request_mv: u16,
    settle_until_ms: Option<u64>,
    confirmed: bool,
) {
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        ..
    } = *backend
    else {
        return;
    };
    *backend = HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode: None,
        current_request_mv: request_mv,
        settle_until_ms,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: true,
        current_limit_fixed_request_confirmed: confirmed,
        terminal_fixed_pd_disarmed: false,
    };
}

#[cfg(target_arch = "xtensa")]
pub(crate) enum CurrentLimitContractResult {
    Ready,
    Pending,
    Failed,
}

#[cfg(test)]
pub(crate) fn current_limit_fallback_requires_confirmed_pps(
    observation: Option<PdStatusObservation>,
) -> bool {
    observation.is_some_and(|observation| {
        observation.status.pd_active && observation.contract.kind == ContractKind::Pps
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn retry_pps_startup_after_fixed_contract_failure(
    observation: Option<PdStatusObservation>,
) -> bool {
    observation.is_some_and(|observation| {
        observation.status.pd_active && observation.contract.kind == ContractKind::Fixed
    })
}

#[cfg(target_arch = "xtensa")]
fn defer_automatic_pps_retry_after_fixed_failure<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    if !retry_pps_startup_after_fixed_contract_failure(context.pd_observation) {
        return false;
    }
    if let HeaterPowerBackend::PpsMos {
        next_request_at_ms, ..
    } = context.backend
    {
        *next_request_at_ms = context.now_ms.saturating_add(PPS_REQUEST_RETRY_DELAY_MS);
    }
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn ensure_current_limit_contract<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    confirmed: bool,
    settle_until_ms: Option<u64>,
) -> CurrentLimitContractResult
where
    PWM: SetDutyCycle,
{
    if confirmed {
        return CurrentLimitContractResult::Ready;
    }
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    if let Some(settle_until_ms) = settle_until_ms {
        if context.now_ms < settle_until_ms {
            return CurrentLimitContractResult::Pending;
        }
        if pd_observation_confirms_fixed_contract(
            context.pd_observation,
            HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
        ) {
            set_pps_current_limit_backend(
                context.backend,
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                None,
                true,
            );
            return CurrentLimitContractResult::Ready;
        }
    }
    let Ok(fixed_request) = PdContractRequest::fixed(
        HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
        MIN_HEATER_CONTRACT_MA,
    ) else {
        return CurrentLimitContractResult::Failed;
    };
    match context.pd_port.request_fixed_contract(fixed_request) {
        PdRequestState::Pending(ticket) => {
            context.pd_port.discard_ticket(ticket);
            CurrentLimitContractResult::Pending
        }
        PdRequestState::Confirmed => {
            set_pps_current_limit_backend(
                context.backend,
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                None,
                true,
            );
            CurrentLimitContractResult::Ready
        }
        PdRequestState::Failed => {
            set_pps_current_limit_backend(
                context.backend,
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                None,
                false,
            );
            CurrentLimitContractResult::Failed
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_pps_current_limit_fallback<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
    safe_max_mv: u16,
    control_floor_mv: u16,
    effective_current_limit_ma: u16,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    let HeaterPowerBackend::PpsMos {
        current_limit_fixed_pwm_active,
        current_limit_fixed_request_confirmed,
        settle_until_ms,
        ..
    } = *context.backend
    else {
        return None;
    };
    if context.normal_auto && safe_max_mv < control_floor_mv {
        return Some(fallback_from_adjustable_request(context).await);
    }
    if context.normal_auto {
        // Automatic mode keeps a safe PPS floor and allocates low power with
        // PWM. The legacy fixed-voltage fallback remains for calibration and
        // manual PPS paths only.
        return None;
    }
    if !should_apply_current_limit_fixed_pwm_fallback(
        context.duty_percent,
        manual_pps_active,
        current_limit_fixed_pwm_active,
        safe_max_mv,
        control_floor_mv,
    ) {
        return None;
    }
    context.hold_pps_governor.reset();
    match ensure_current_limit_contract(
        context,
        current_limit_fixed_request_confirmed,
        settle_until_ms,
    )
    .await
    {
        CurrentLimitContractResult::Pending => return Some(false),
        CurrentLimitContractResult::Failed => {
            info!(
                "heater current-limit fallback waiting fixed_mv={=u16} safe_max_mv={=u16} control_floor_mv={=u16} current_limit_ma={=u16}",
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                safe_max_mv,
                control_floor_mv,
                effective_current_limit_ma,
            );
            return Some(false);
        }
        CurrentLimitContractResult::Ready => {}
    }
    let fallback_duty_percent = apply_warmup_soft_start(
        current_limit_fixed_pwm_duty_percent(
            context.duty_percent,
            context.current_temp_c,
            effective_current_limit_ma,
            context.preview_heater_curve,
            context.memory_config,
        ),
        context.warmup_soft_start_percent,
    );
    apply_heater_duty(
        context.heater_pwm,
        fallback_duty_percent,
        context.last_physical_duty_percent,
    );
    info!(
        "heater current-limit fallback active temp_c={=f32} current_limit_ma={=u16} safe_max_mv={=u16} control_floor_mv={=u16} fixed_mv={=u16} duty={=u8}%",
        context.current_temp_c,
        effective_current_limit_ma,
        safe_max_mv,
        control_floor_mv,
        HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
        fallback_duty_percent,
    );
    Some(true)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn handle_pps_settle_period<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    source_request_ceiling_mv: u16,
    request_current_ma: u16,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode,
        current_request_mv,
        settle_until_ms,
        next_request_at_ms,
        ..
    } = *context.backend
    else {
        return Some(false);
    };
    let settle_until_ms = settle_until_ms?;
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    if context.now_ms < settle_until_ms {
        return Some(false);
    }
    *context.backend = HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode,
        current_request_mv,
        settle_until_ms: None,
        next_request_at_ms,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: false,
    };
    if current_request_mv <= source_request_ceiling_mv {
        let settled_gate_duty_percent = pps_confirmed_gate_duty_percent(
            context,
            current_request_mv,
            source_request_ceiling_mv,
            request_current_ma,
        );
        apply_heater_duty(
            context.heater_pwm,
            settled_gate_duty_percent,
            context.last_physical_duty_percent,
        );
        return Some(true);
    }
    None
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PendingAutomaticPowerRequest {
    pub(crate) ticket: PowerTicket,
    pub(crate) request: PdContractRequest,
    pub(crate) started_at_ms: u64,
    pub(crate) pause_heater: bool,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct PpsVoltageTransition {
    request_mv: u16,
    request_current_ma: u16,
    request_mode: ch224q::AdjustableVoltageMode,
    mode_changed: bool,
    voltage_changed: bool,
    request_transition_pending: bool,
    safety_recovery_required: bool,
    gate_duty_percent: u8,
    blank_heater: bool,
}

#[cfg(target_arch = "xtensa")]
fn pps_transition_should_clear_pwm(request: &PpsVoltageTransition) -> bool {
    automatic_pps_transition_should_clear_pwm(
        request.voltage_changed,
        request.mode_changed,
        request.safety_recovery_required,
        request.request_transition_pending,
        request.gate_duty_percent,
        request.blank_heater,
    )
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn fallback_from_adjustable_request<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    match poll_pending_automatic_power_ticket(context) {
        AutomaticPowerTicketPoll::Pending => return false,
        AutomaticPowerTicketPoll::None
        | AutomaticPowerTicketPoll::Confirmed
        | AutomaticPowerTicketPoll::Failed => {}
    }
    let (fixed_request, fixed_request_current_ma) = if context.normal_auto {
        let resistance_ohms = automatic_heater_resistance_ohms(
            context.current_temp_c,
            context.preview_heater_curve,
            context.memory_config,
        );
        let Some(candidate) =
            automatic_heater_fixed_request(context.source_capabilities, resistance_ohms)
        else {
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            info!(
                "automatic heater paused: no safe Fixed fallback temp_c={=f32}",
                context.current_temp_c,
            );
            return false;
        };
        candidate
    } else {
        (ch224q::VoltageRequest::V12, MIN_HEATER_CONTRACT_MA)
    };
    let fixed_request_confirmed =
        request_fixed_fallback_contract(context, fixed_request, fixed_request_current_ma)
            .unwrap_or(false);
    *context.backend = HeaterPowerBackend::FixedPdPwmFallback {
        reason: HeaterPowerBackendReason::AdjustableRequestFailed,
        fixed_request_confirmed,
        fixed_request,
        terminal_fixed_pd_disarmed: false,
    };
    if fixed_request_confirmed && context.heater_enabled && context.pd_observation.is_some() {
        let negotiated_current_ma = context
            .pd_observation
            .filter(|observation| observation.status.pd_active)
            .map(|observation| observation.current_ma)
            .unwrap_or(0);
        let safe_duty_percent = if context.normal_auto {
            let resistance_ohms = automatic_heater_resistance_ohms(
                context.current_temp_c,
                context.preview_heater_curve,
                context.memory_config,
            );
            context
                .pd_observation
                .filter(|observation| {
                    observation.status.pd_active
                        && observation.contract.kind == ContractKind::Fixed
                        && observation.contract_voltage_mv == Some(fixed_request.millivolts())
                        && observation.current_ma >= fixed_request_current_ma
                })
                .and_then(|observation| observation.contract_voltage_mv)
                .map_or(0, |voltage_mv| {
                    automatic_heater_pwm_percent_for_power(
                        context.requested_power_mw,
                        resistance_ohms,
                        voltage_mv,
                        context.warmup_soft_start_percent,
                    )
                })
        } else {
            fixed_pd_pwm_duty_percent(
                context.duty_percent,
                context.current_temp_c,
                context
                    .pd_observation
                    .and_then(|observation| observation.contract_voltage_mv)
                    .unwrap_or(FUSB302B_INITIAL_PPS_REQUEST_MV),
                negotiated_current_ma,
                context.active_thermal_settings.heater_current_reserve_ma,
                context.preview_heater_curve,
                context.memory_config,
            )
        };
        apply_heater_duty(
            context.heater_pwm,
            apply_warmup_soft_start(safe_duty_percent, context.warmup_soft_start_percent),
            context.last_physical_duty_percent,
        );
    }
    info!(
        "heater backend fallback -> reason={=str} fixed_request_confirmed={=bool}",
        HeaterPowerBackendReason::AdjustableRequestFailed.label(),
        fixed_request_confirmed,
    );
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_pps_voltage_transition<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    request: PpsVoltageTransition,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    if !automatic_pps_transition_requires_request(
        request.voltage_changed,
        request.mode_changed,
        request.safety_recovery_required,
        request.request_transition_pending,
    ) {
        return None;
    }
    let request_contract = if context.normal_auto {
        context.manual_pps.automatic_heater_request_contract(
            context.current_temp_c,
            context.memory_config,
            request.request_mv,
            request.request_current_ma,
            Some(context.backend.pd_request_mv()),
        )
    } else {
        PdContractRequest::pps(request.request_mv, request.request_current_ma).ok()
    };
    let Some(request_contract) = request_contract else {
        return Some(fallback_from_adjustable_request(context).await);
    };
    let request_state = if working_pps_request_confirmed(
        context.pd_port.snapshot().settled_observation(),
        request_contract,
    ) && request_contract.pps_range().is_none_or(|range| {
        context
            .pd_port
            .snapshot()
            .active
            .is_some_and(|active| active.pps_range == Some(range))
    }) {
        PdRequestState::Confirmed
    } else {
        context
            .pd_port
            .request_pps_contract_for(PowerIntentOwner::AutomaticThermal, request_contract)
    };
    match request_state {
        PdRequestState::Pending(ticket) => {
            if context.normal_auto {
                *context.pending_automatic_power_request = Some(PendingAutomaticPowerRequest {
                    started_at_ms: context.now_ms,
                    ticket,
                    request: request_contract,
                    pause_heater: request.blank_heater || request.safety_recovery_required,
                });
            } else {
                context.pd_port.discard_ticket(ticket);
            }
            if request.blank_heater || request.safety_recovery_required {
                apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            } else if context.normal_auto {
                apply_heater_duty(
                    context.heater_pwm,
                    request.gate_duty_percent,
                    context.last_physical_duty_percent,
                );
            }
            return Some(false);
        }
        PdRequestState::Confirmed => {}
        PdRequestState::Failed => {
            if defer_automatic_pps_retry_after_fixed_failure(context) {
                return Some(false);
            }
            return Some(fallback_from_adjustable_request(context).await);
        }
    }
    if !working_pps_request_confirmed(
        context.pd_port.snapshot().settled_observation(),
        request_contract,
    ) {
        return Some(fallback_from_adjustable_request(context).await);
    }
    if automatic_pps_transition_should_restore_pwm(
        true,
        request.safety_recovery_required,
        request.blank_heater,
        request.gate_duty_percent,
    ) {
        apply_heater_duty(
            context.heater_pwm,
            request.gate_duty_percent,
            context.last_physical_duty_percent,
        );
    }
    commit_confirmed_pps_transition(context, request);
    Some(true)
}

#[cfg(target_arch = "xtensa")]
fn commit_confirmed_pps_transition<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    request: PpsVoltageTransition,
) {
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        ..
    } = *context.backend
    else {
        return;
    };
    *context.backend = HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode: Some(request.request_mode),
        current_request_mv: request.request_mv,
        settle_until_ms: request.blank_heater.then_some(
            context
                .now_ms
                .saturating_add(pps_request_transition_ms(request.mode_changed)),
        ),
        next_request_at_ms: context
            .now_ms
            .saturating_add(pps_request_transition_ms(request.mode_changed)),
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: false,
    };
}

#[cfg(target_arch = "xtensa")]
fn pps_request_mv_for_context<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
    current_request_mv: u16,
    idle_request_mv: u16,
    control_floor_mv: u16,
    source_request_ceiling_mv: u16,
) -> u16 {
    let automatic_request_mv = if context.tuning_power_stage {
        source_request_ceiling_mv
    } else if context.normal_auto {
        let resistance_ohms = automatic_heater_resistance_ohms(
            context.current_temp_c,
            context.preview_heater_curve,
            context.memory_config,
        );
        automatic_heater_request_mv_from_power(
            context.requested_power_mw,
            resistance_ohms,
            control_floor_mv,
            source_request_ceiling_mv,
        )
    } else {
        heater_adjustable_request_mv(
            context.duty_percent,
            context.heater_enabled,
            current_request_mv,
            idle_request_mv,
            control_floor_mv,
            source_request_ceiling_mv,
        )
    };
    if manual_pps_active
        || context.tuning_power_stage
        || (context.normal_auto
            && !matches!(
                context.control_phase,
                HeaterControlPhase::Hold | HeaterControlPhase::Approach
            ))
    {
        automatic_request_mv
    } else {
        heater_phase_pps_request_mv(
            context.hold_pps_governor,
            HoldPpsRequestInput {
                phase: context.control_phase,
                duty_percent: context.duty_percent,
                actual_error_c: context.control_error_c,
                filtered_slope_c_per_s: context.filtered_slope_c_per_s,
                current_request_mv,
                control_floor_mv,
                safe_max_mv: source_request_ceiling_mv,
                now_ms: context.now_ms,
            },
            context.initial_profile_approach,
        )
        .unwrap_or(automatic_request_mv)
    }
}

#[cfg(target_arch = "xtensa")]
fn pps_confirmed_gate_duty_percent<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    current_request_mv: u16,
    source_request_ceiling_mv: u16,
    _request_current_ma: u16,
) -> u8 {
    if context.normal_auto {
        pending_automatic_pps_duty(context, false)
    } else {
        heater_physical_pwm_percent(
            context.duty_percent,
            source_request_ceiling_mv,
            current_request_mv,
            context.warmup_soft_start_percent,
        )
    }
}

#[cfg(target_arch = "xtensa")]
fn active_pps_request_gate_duty_percent<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    request_transition_pending: bool,
    current_request_mv: u16,
    source_request_ceiling_mv: u16,
    request_current_ma: u16,
    gate_duty_percent: u8,
) -> u8
where
    PWM: SetDutyCycle,
{
    if request_transition_pending {
        pps_confirmed_gate_duty_percent(
            context,
            current_request_mv,
            source_request_ceiling_mv,
            request_current_ma,
        )
    } else {
        gate_duty_percent
    }
}

#[cfg(target_arch = "xtensa")]
struct PpsVoltageRequestLog {
    safe_max_mv: u16,
    source_request_ceiling_mv: u16,
    control_floor_mv: u16,
    request_mv: u16,
    voltage_changed: bool,
    mode_changed: bool,
    safety_recovery_required: bool,
}

#[cfg(target_arch = "xtensa")]
struct PpsTransitionSafetyInput {
    manual_pps_active: bool,
    next_request_at_ms: u64,
}

#[cfg(target_arch = "xtensa")]
struct PpsTransitionSafety {
    recovery_required: bool,
    request_pending: bool,
}

#[cfg(target_arch = "xtensa")]
fn pps_transition_safety<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    input: PpsTransitionSafetyInput,
) -> PpsTransitionSafety {
    let pd_snapshot = context.pd_port.snapshot();
    let recovery_required = context.normal_auto
        && context
            .manual_pps
            .automatic_heater_confirmed_pps_candidate(
                context.current_temp_c,
                context.memory_config,
                pd_snapshot,
            )
            .is_none();
    PpsTransitionSafety {
        recovery_required,
        request_pending: !input.manual_pps_active
            && !recovery_required
            && (context.now_ms < input.next_request_at_ms || pd_snapshot.transition_pending),
    }
}

#[cfg(target_arch = "xtensa")]
fn log_pps_voltage_request<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    request: PpsVoltageRequestLog,
) where
    PWM: SetDutyCycle,
{
    if request.voltage_changed || request.mode_changed || request.safety_recovery_required {
        info!(
            "heater pps request temp_c={=f32} control={=u8}% safe_heater_mv={=u16} source_ceiling_mv={=u16} control_floor_mv={=u16} request_mv={=u16}",
            context.current_temp_c,
            context.duty_percent,
            request.safe_max_mv,
            request.source_request_ceiling_mv,
            request.control_floor_mv,
            request.request_mv,
        );
    }
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
enum AutomaticPowerTicketPoll {
    None,
    Pending,
    Confirmed,
    Failed,
}

#[cfg(target_arch = "xtensa")]
fn pending_automatic_pps_is_safe<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    request: PdContractRequest,
) -> bool {
    automatic_pps_request_is_safe(context, request, request.pps_range())
}

#[cfg(target_arch = "xtensa")]
fn automatic_pps_request_is_safe<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    request: PdContractRequest,
    confirmed_range: Option<PpsAdjustmentRange>,
) -> bool {
    let Some(candidate) = context.manual_pps.automatic_heater_pps_for_contract(
        context.current_temp_c,
        context.memory_config,
        request.voltage_mv(),
        request.operating_current_ma(),
        confirmed_range,
    ) else {
        return false;
    };
    request.mode() == PdContractRequestMode::Pps
        && request.operating_current_ma() >= MIN_HEATER_CONTRACT_MA
        && (candidate.min_mv..=candidate.allowed_mv).contains(&request.voltage_mv())
}

#[cfg(target_arch = "xtensa")]
fn pending_automatic_pps_duty<PWM>(
    context: &HeaterPowerOutputContext<'_, PWM>,
    pause_heater: bool,
) -> u8 {
    if pause_heater {
        return 0;
    }
    let snapshot = context.pd_port.snapshot();
    if context
        .manual_pps
        .automatic_heater_confirmed_pps_candidate(
            context.current_temp_c,
            context.memory_config,
            snapshot,
        )
        .is_none()
    {
        return 0;
    }
    let Some(active) = snapshot.active else {
        return 0;
    };
    automatic_heater_pwm_percent_for_power(
        context.requested_power_mw,
        automatic_heater_resistance_ohms(
            context.current_temp_c,
            context.preview_heater_curve,
            context.memory_config,
        ),
        active.voltage_mv,
        context.warmup_soft_start_percent,
    )
}

#[cfg(target_arch = "xtensa")]
fn poll_pending_automatic_power_ticket<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> AutomaticPowerTicketPoll
where
    PWM: SetDutyCycle,
{
    let Some(pending) = *context.pending_automatic_power_request else {
        return AutomaticPowerTicketPoll::None;
    };
    if pending.request.mode() == PdContractRequestMode::Pps
        && !pending_automatic_pps_is_safe(context, pending.request)
    {
        // The old target may still finish in the PD service. It must neither
        // block a direct safety request nor authorize output when it arrives.
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        context.pd_port.discard_ticket(pending.ticket);
        *context.pending_automatic_power_request = None;
        return AutomaticPowerTicketPoll::None;
    }
    let Some(outcome) = context.pd_port.try_take_ticket(pending.ticket) else {
        if context.now_ms.saturating_sub(pending.started_at_ms) > AUTOMATIC_PPS_TICKET_WAIT_MS {
            context.pd_port.discard_ticket(pending.ticket);
            *context.pending_automatic_power_request = None;
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            return AutomaticPowerTicketPoll::Failed;
        }
        let duty = pending_automatic_pps_duty(context, pending.pause_heater);
        apply_heater_duty(context.heater_pwm, duty, context.last_physical_duty_percent);
        return AutomaticPowerTicketPoll::Pending;
    };
    *context.pending_automatic_power_request = None;
    let settled_observation = context.pd_port.snapshot().settled_observation();
    let observation_confirms = match pending.request.mode() {
        PdContractRequestMode::Fixed => working_fixed_contract_confirmed(
            settled_observation,
            pending.request.voltage_mv(),
            pending.request.operating_current_ma(),
        ),
        PdContractRequestMode::Pps => {
            working_pps_request_confirmed(settled_observation, pending.request)
        }
    };
    if ticket_outcome_confirms_request(outcome, pending.request) && observation_confirms {
        context.pd_observation = settled_observation;
        if pending.request.mode() == PdContractRequestMode::Pps
            && let HeaterPowerBackend::PpsMos {
                current_mode,
                current_request_mv,
                settle_until_ms,
                next_request_at_ms,
                ..
            } = context.backend
        {
            *current_request_mv = pending.request.voltage_mv();
            *current_mode = Some(ch224q::AdjustableVoltageMode::Pps);
            *settle_until_ms = pending.pause_heater.then_some(
                context
                    .now_ms
                    .saturating_add(pps_request_transition_ms(true)),
            );
            *next_request_at_ms = context
                .now_ms
                .saturating_add(pps_request_transition_ms(false));
        }
        AutomaticPowerTicketPoll::Confirmed
    } else {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        AutomaticPowerTicketPoll::Failed
    }
}

#[cfg(target_arch = "xtensa")]
async fn handle_pending_automatic_power_request<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    match poll_pending_automatic_power_ticket(context) {
        AutomaticPowerTicketPoll::Pending => Some(false),
        AutomaticPowerTicketPoll::Failed => {
            if defer_automatic_pps_retry_after_fixed_failure(context) {
                Some(false)
            } else {
                Some(fallback_from_adjustable_request(context).await)
            }
        }
        AutomaticPowerTicketPoll::None | AutomaticPowerTicketPoll::Confirmed => None,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_pps_voltage_request<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
    safe_max_mv: u16,
    source_request_ceiling_mv: u16,
    control_floor_mv: u16,
    request_current_ma: u16,
) -> bool
where
    PWM: SetDutyCycle,
{
    if let Some(result) = handle_pending_automatic_power_request(context).await {
        return result;
    }
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        current_mode,
        current_request_mv,
        next_request_at_ms,
        ..
    } = *context.backend
    else {
        return false;
    };
    let request_mv = pps_request_mv_for_context(
        context,
        manual_pps_active,
        current_request_mv,
        idle_request_mv,
        control_floor_mv,
        source_request_ceiling_mv,
    );
    let request_mode = adjustable_mode_for_request(request_mv, pps_max_mv);
    let mode_changed = !manual_pps_active && current_mode != Some(request_mode);
    let voltage_changed = !manual_pps_active && current_request_mv != request_mv;
    let transition_safety = pps_transition_safety(
        context,
        PpsTransitionSafetyInput {
            manual_pps_active,
            next_request_at_ms,
        },
    );
    let safety_recovery_required = transition_safety.recovery_required;
    let request_transition_pending = transition_safety.request_pending;
    let gate_duty_percent = pps_confirmed_gate_duty_percent(
        context,
        current_request_mv,
        source_request_ceiling_mv,
        request_current_ma,
    );
    let blank_heater = should_blank_heater_for_pps_range_change(
        current_request_mv,
        request_mv,
        mode_changed,
        pps_min_mv,
        pps_max_mv,
    );
    let transition = PpsVoltageTransition {
        request_mv,
        request_current_ma,
        request_mode,
        mode_changed,
        voltage_changed,
        request_transition_pending,
        safety_recovery_required,
        gate_duty_percent,
        blank_heater,
    };
    if pps_transition_should_clear_pwm(&transition) {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    }
    if let Some(result) = apply_pps_voltage_transition(&mut *context, transition).await {
        return result;
    }
    let active_request_gate_duty_percent = active_pps_request_gate_duty_percent(
        context,
        request_transition_pending,
        current_request_mv,
        source_request_ceiling_mv,
        request_current_ma,
        gate_duty_percent,
    );
    apply_heater_duty(
        context.heater_pwm,
        active_request_gate_duty_percent,
        context.last_physical_duty_percent,
    );
    log_pps_voltage_request(
        context,
        PpsVoltageRequestLog {
            safe_max_mv,
            source_request_ceiling_mv,
            control_floor_mv,
            request_mv,
            voltage_changed,
            mode_changed,
            safety_recovery_required,
        },
    );
    false
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_pps_backend<PWM>(
    context: HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
) -> bool
where
    PWM: SetDutyCycle,
{
    let mut context = context;
    if let Some(result) = handle_pending_automatic_power_request(&mut context).await {
        return result;
    }
    let Some(limits) = pps_operating_limits(&context) else {
        if context.normal_auto {
            return fallback_from_adjustable_request(&mut context).await;
        }
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        if context.normal_auto && context.duty_percent != 0 {
            info!(
                "automatic heater paused: no safe PPS candidate temp_c={=f32} power_mw={=u32}",
                context.current_temp_c, context.requested_power_mw,
            );
        }
        return false;
    };
    let effective_current_limit_ma =
        effective_pps_current_limit_ma(limits.capability_max_ma, context.pd_observation);
    let request_current_ma = limits.request_current_ma;
    let safe_max_mv = limits.adjustable_max_mv;
    let source_request_ceiling_mv = heater_source_request_ceiling_mv(
        safe_max_mv,
        limits.current_request_mv,
        context.measured_heater_mv,
        limits.adjustable_max_mv,
    );
    let control_floor_mv = if context.normal_auto {
        limits.pps_min_mv
    } else {
        effective_auto_adjustable_working_floor_mv(
            context.active_thermal_settings,
            limits.pps_min_mv,
            limits.adjustable_max_mv,
        )
    };
    if context.normal_auto && context.now_ms % 1_000 < HEATER_CONTROL_INTERVAL_MS {
        let (resistance_ohms, resistance_source) = estimated_heater_resistance_with_source(
            context.current_temp_c,
            context.preview_heater_curve,
            context.memory_config,
        );
        let boundary_source = context
            .manual_pps
            .automatic_heater_pps_candidate(
                context.current_temp_c,
                context.preview_heater_curve,
                context.memory_config,
                Some(limits.current_request_mv),
            )
            .map_or("unavailable", |candidate| {
                if candidate.boundary_applied {
                    "whole-device-boundary"
                } else {
                    "preset-fallback"
                }
            });
        info!(
            "automatic heater safety boundary={=str} temp_c={=f32} estimated_resistance_ohms={=f32} resistance_source={=str} budget_ma={=u16} safe_mv={=u16} power_mw={=u32}",
            boundary_source,
            context.current_temp_c,
            resistance_ohms,
            resistance_source.label(),
            request_current_ma,
            safe_max_mv,
            (f32::from(safe_max_mv) * f32::from(safe_max_mv) / resistance_ohms.max(0.001) / 1_000.0)
                as u32,
        );
    }
    if let Some(result) = apply_pps_current_limit_fallback(
        &mut context,
        manual_pps_active,
        safe_max_mv,
        control_floor_mv,
        effective_current_limit_ma,
    )
    .await
    {
        return result;
    }
    if let Some(result) =
        handle_pps_settle_period(&mut context, source_request_ceiling_mv, request_current_ma).await
    {
        return result;
    }
    apply_pps_voltage_request(
        &mut context,
        manual_pps_active,
        safe_max_mv,
        source_request_ceiling_mv,
        control_floor_mv,
        request_current_ma,
    )
    .await
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_fan_output<PWM>(
    fan_enable: &mut Output<'_>,
    fan_pwm: &mut PWM,
    command: FanHardwareCommand,
    last_command: &mut Option<FanHardwareCommand>,
) where
    PWM: SetDutyCycle,
{
    if last_command.is_some_and(|last| last == command) {
        return;
    }

    let duty_percent = pwm_percent_from_permille(command.pwm_permille);
    let _ = fan_pwm.set_duty_cycle_percent(duty_percent);
    if command.enabled {
        fan_enable.set_high();
    } else {
        fan_enable.set_low();
    }
    info!(
        "fan runtime -> {=str} gpio35={=str} gpio36 duty={=u8}% pwm_permille={=u16} freq={=u32}Hz",
        if command.enabled { "run" } else { "off" },
        if command.enabled { "on" } else { "off" },
        duty_percent,
        command.pwm_permille,
        FAN_PWM_FREQUENCY_HZ,
    );
    *last_command = Some(command);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_buzzer_output<'a, PWM>(
    buzzer_timer: &mut esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'a>>,
    buzzer_pwm: &mut PWM,
    peripheral_clock: &PeripheralClockConfig,
    output: BuzzerOutput,
    last_state: &mut BuzzerHardwareState,
    configured_frequency_hz: &mut u32,
) -> bool
where
    PWM: SetDutyCycle,
{
    let next_state = BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent.min(100),
        generation: output.generation,
    };
    if *last_state == next_state {
        return false;
    }

    for action in buzzer_hardware_actions(*configured_frequency_hz, next_state) {
        match action {
            // Duty updates on GPIO48 are immediate. Quiesce the output before
            // touching Timer2 so a retune cannot preserve a partial old cycle.
            BuzzerHardwareAction::SetDutyPercent(duty_percent) => {
                let _ = buzzer_pwm.set_duty_cycle_percent(duty_percent);
            }
            BuzzerHardwareAction::StopTimer => buzzer_timer.stop(),
            BuzzerHardwareAction::Retune(next_frequency_hz) => {
                let period_ticks = buzzer_timer_period_ticks(next_frequency_hz)
                    .expect("buzzer frequency is outside the Timer2 period range");
                let timer_cfg = peripheral_clock.timer_clock_with_prescaler(
                    period_ticks,
                    PwmWorkingMode::Increase,
                    BUZZER_TIMER_PRESCALER,
                );
                buzzer_timer.set_counter(0, CounterDirection::Increasing);
                buzzer_timer.start(timer_cfg);
                *configured_frequency_hz = next_frequency_hz;
            }
        }
    }
    info!(
        "buzzer output -> freq_hz={=u32} duty={=u8}% gen={=u32}",
        next_state.frequency_hz.unwrap_or(0),
        next_state.duty_percent,
        next_state.generation,
    );
    *last_state = next_state;
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) const BUZZER_COMMAND_CAPACITY: usize = 32;

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone)]
pub(crate) enum BuzzerRuntimeCommand {
    Feedback {
        source: BuzzerCueSource,
        cue: BuzzerCueId,
    },
    ProtectionReplay {
        source: BuzzerCueSource,
    },
    RequestAttentionReminder {
        source: BuzzerCueSource,
    },
    #[cfg(feature = "buzzer-test")]
    Test(BuzzerTestCommand),
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum BuzzerSafetyCommand {
    ActivateProtection { source: BuzzerCueSource },
    EnterAttentionPendingAndRequestReminder { source: BuzzerCueSource },
    ClearAttention,
}

#[cfg(target_arch = "xtensa")]
pub(crate) static BUZZER_COMMANDS: Channel<
    CriticalSectionRawMutex,
    BuzzerRuntimeCommand,
    BUZZER_COMMAND_CAPACITY,
> = Channel::new();

#[cfg(target_arch = "xtensa")]
pub(crate) static BUZZER_SAFETY_COMMAND: Signal<CriticalSectionRawMutex, BuzzerSafetyCommand> =
    Signal::new();

// GPIO48 transitions have 25-80 ms deadlines. Keep their sole owner out of the
// thread-mode executor, where display or control work can otherwise defer a cue
// transition until the next cooperative poll.
#[cfg(target_arch = "xtensa")]
pub(crate) static mut BUZZER_REALTIME_EXECUTOR_STORAGE: MaybeUninit<InterruptExecutor<1>> =
    MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
const fn initial_buzzer_test_status() -> BuzzerTestStatus {
    BuzzerTestStatus {
        state: BuzzerTestSessionState::Idle,
        scenario: None,
        cue: None,
        repeat: false,
        active_cue: None,
        trace: heapless::Vec::new(),
        #[cfg(feature = "buzzer-observe")]
        output_trace: heapless::Vec::new(),
    }
}

/// Runtime callers submit cue requests. The dedicated task owns arbitration,
/// cue progression, and every GPIO48 PWM write.
#[cfg(target_arch = "xtensa")]
#[derive(Default)]
pub(crate) struct BuzzerRuntime;

#[cfg(target_arch = "xtensa")]
impl BuzzerRuntime {
    pub(crate) fn submit(command: BuzzerRuntimeCommand) {
        if BUZZER_COMMANDS.try_send(command).is_err() {
            // Feedback is best effort, but this capacity is intentionally far
            // larger than one front-panel/control-loop iteration can produce.
            warn!("buzzer command mailbox full");
        }
    }

    pub(crate) fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, _: u64) {
        Self::submit(BuzzerRuntimeCommand::Feedback { source, cue });
    }

    pub(crate) fn activate_protection(&mut self, source: BuzzerCueSource, _: u64) {
        BUZZER_SAFETY_COMMAND.signal(BuzzerSafetyCommand::ActivateProtection { source });
    }

    pub(crate) fn request_protection_replay(&mut self, source: BuzzerCueSource, _: u64) {
        Self::submit(BuzzerRuntimeCommand::ProtectionReplay { source });
    }

    pub(crate) fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        _: u64,
    ) {
        BUZZER_SAFETY_COMMAND
            .signal(BuzzerSafetyCommand::EnterAttentionPendingAndRequestReminder { source });
    }

    pub(crate) fn clear_attention(&mut self) {
        BUZZER_SAFETY_COMMAND.signal(BuzzerSafetyCommand::ClearAttention);
    }

    pub(crate) fn request_attention_reminder(&mut self, source: BuzzerCueSource, _: u64) {
        Self::submit(BuzzerRuntimeCommand::RequestAttentionReminder { source });
    }

    #[cfg(feature = "buzzer-test")]
    pub(crate) fn submit_test(command: BuzzerTestCommand) {
        Self::submit(BuzzerRuntimeCommand::Test(command));
    }
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) type BuzzerTestStatusMutex =
    BlockingMutex<CriticalSectionRawMutex, RefCell<BuzzerTestStatus>>;

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) static BUZZER_TEST_STATUS: BuzzerTestStatusMutex =
    BlockingMutex::new(RefCell::new(initial_buzzer_test_status()));

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) fn buzzer_test_status() -> BuzzerTestStatus {
    BUZZER_TEST_STATUS.lock(|status| status.borrow().clone())
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
#[cfg(feature = "buzzer-observe")]
pub(crate) fn publish_buzzer_test_status(
    session: &BuzzerTestSession,
    arbiter: &BuzzerArbiter,
    output_trace: &BuzzerTestOutputTrace,
) {
    let mut status = session.status(arbiter.active_cue());
    status.output_trace = output_trace.events.clone();
    BUZZER_TEST_STATUS.lock(|published| *published.borrow_mut() = status);
}

#[cfg(all(
    target_arch = "xtensa",
    feature = "buzzer-test",
    not(feature = "buzzer-observe")
))]
pub(crate) fn publish_buzzer_test_status(session: &BuzzerTestSession, arbiter: &BuzzerArbiter) {
    let status = session.status(arbiter.active_cue());
    BUZZER_TEST_STATUS.lock(|published| *published.borrow_mut() = status);
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
pub(crate) struct BuzzerTestOutputTrace {
    started_at_ms: u64,
    last_recorded_ms: u64,
    last_pad_rising_edges: u16,
    events: heapless::Vec<BuzzerTestOutputTraceEvent, BUZZER_TEST_OUTPUT_TRACE_CAPACITY>,
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
impl BuzzerTestOutputTrace {
    const fn new() -> Self {
        Self {
            started_at_ms: 0,
            last_recorded_ms: 0,
            last_pad_rising_edges: 0,
            events: heapless::Vec::new(),
        }
    }

    fn reset(&mut self, now_ms: u64, pad_rising_edges: u16) {
        self.started_at_ms = now_ms;
        self.last_recorded_ms = now_ms;
        self.last_pad_rising_edges = pad_rising_edges;
        self.events.clear();
    }

    fn record(&mut self, now_ms: u64, output: BuzzerOutput, pad_rising_edges: u16) {
        // The buzzer task exclusively owns timer2 writes. This direct PAC
        // access reads CFG0 only, after `apply_buzzer_output` has completed.
        let cfg0 = unsafe { (&*esp_hal::peripherals::MCPWM0::PTR).timer(2).cfg0().read() };
        let timer_prescaler = cfg0.prescale().bits();
        let timer_period_ticks = cfg0.period().bits();
        let observed_window_ms = now_ms
            .saturating_sub(self.last_recorded_ms)
            .min(u64::from(u32::MAX)) as u32;
        let observed_rising_edges = pad_rising_edges.wrapping_sub(self.last_pad_rising_edges);
        if let Some(previous) = self.events.last_mut() {
            previous.observed_window_ms = observed_window_ms;
            previous.observed_rising_edges = observed_rising_edges;
            previous.observed_frequency_hz = (previous.duty_percent > 0)
                .then(|| buzzer_observed_frequency_hz(observed_rising_edges, observed_window_ms))
                .flatten();
        }
        self.last_recorded_ms = now_ms;
        self.last_pad_rising_edges = pad_rising_edges;
        if self.events.len() == BUZZER_TEST_OUTPUT_TRACE_CAPACITY {
            let _ = self.events.remove(0);
        }
        let _ = self.events.push(BuzzerTestOutputTraceEvent {
            elapsed_ms: now_ms
                .saturating_sub(self.started_at_ms)
                .min(u64::from(u32::MAX)) as u32,
            requested_frequency_hz: output.frequency_hz,
            applied_frequency_hz: mcpwm_timer_frequency_hz(timer_prescaler, timer_period_ticks),
            observed_frequency_hz: None,
            observed_rising_edges: 0,
            observed_window_ms: 0,
            duty_percent: output.duty_percent.min(100),
            generation: output.generation,
            timer_prescaler,
            timer_period_ticks,
        });
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_buzzer_command(
    command: BuzzerRuntimeCommand,
    arbiter: &mut BuzzerArbiter,
    now_ms: u64,
    #[cfg(feature = "buzzer-test")] test_session: &mut BuzzerTestSession,
) {
    let decision = match command {
        BuzzerRuntimeCommand::Feedback { source, cue } => {
            Some(arbiter.request_feedback(source, cue, now_ms))
        }
        BuzzerRuntimeCommand::ProtectionReplay { source } => {
            Some(arbiter.request_protection_replay(source, now_ms))
        }
        BuzzerRuntimeCommand::RequestAttentionReminder { source } => {
            Some(arbiter.request_attention_reminder(source, now_ms))
        }
        #[cfg(feature = "buzzer-test")]
        BuzzerRuntimeCommand::Test(command) => {
            apply_buzzer_test_command(test_session, arbiter, command, now_ms);
            None
        }
    };
    if let Some(decision) = decision {
        log_buzzer_decision(decision);
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_buzzer_safety_command(
    command: BuzzerSafetyCommand,
    arbiter: &mut BuzzerArbiter,
    now_ms: u64,
    #[cfg(feature = "buzzer-test")] test_session: &mut BuzzerTestSession,
) {
    #[cfg(feature = "buzzer-test")]
    test_session.cancel_for_safety(arbiter, now_ms);
    let decision = match command {
        BuzzerSafetyCommand::ActivateProtection { source } => {
            Some(arbiter.activate_protection(source, now_ms))
        }
        BuzzerSafetyCommand::EnterAttentionPendingAndRequestReminder { source } => {
            if let Some(decision) = arbiter.enter_attention_pending() {
                log_buzzer_decision(decision);
            }
            Some(arbiter.request_attention_reminder(source, now_ms))
        }
        BuzzerSafetyCommand::ClearAttention => arbiter.clear_attention(),
    };
    if let Some(decision) = decision {
        log_buzzer_decision(decision);
    }
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) fn log_buzzer_decisions<I>(decisions: I)
where
    I: IntoIterator<Item = BuzzerDecision>,
{
    for decision in decisions {
        log_buzzer_decision(decision);
    }
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) fn apply_buzzer_test_command(
    test_session: &mut BuzzerTestSession,
    arbiter: &mut BuzzerArbiter,
    command: BuzzerTestCommand,
    now_ms: u64,
) {
    match command.op {
        BuzzerTestOp::Status => (),
        BuzzerTestOp::Trigger => {
            let cue = command.cue.expect("validated buzzer test trigger cue");
            let should_playback = command.repeat
                || matches!(
                    cue,
                    BuzzerCueId::ProtectionAlarm | BuzzerCueId::AttentionReminder
                );
            if !should_playback {
                let decision = test_session.trigger_feedback(arbiter, cue, now_ms);
                log_buzzer_decision(decision);
                return;
            }
            match test_session.start_playback(arbiter, cue, command.repeat, now_ms) {
                Ok(decisions) => log_buzzer_decisions(decisions),
                Err(_) => warn!("buzzer test command ignored while a session is active"),
            }
        }
        BuzzerTestOp::Run => match test_session.start_scenario(
            arbiter,
            command.scenario.expect("validated buzzer test scenario"),
            now_ms,
        ) {
            Ok(decisions) => {
                log_buzzer_decisions(decisions);
            }
            Err(_) => warn!("buzzer test command ignored while a session is active"),
        },
        BuzzerTestOp::Stop => {
            if let Some(decision) = test_session.stop_playback(arbiter, now_ms) {
                log_buzzer_decision(decision);
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) enum BuzzerTaskWake {
    Command(BuzzerRuntimeCommand),
    Safety(BuzzerSafetyCommand),
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn wait_for_buzzer_wake(next_deadline_ms: Option<u64>) -> Option<BuzzerTaskWake> {
    if let Some(deadline_ms) = next_deadline_ms {
        let delay_ms = deadline_ms
            .saturating_sub(Instant::now().as_millis())
            .max(1);
        match select3(
            BUZZER_COMMANDS.receive(),
            BUZZER_SAFETY_COMMAND.wait(),
            EmbassyTimer::after_millis(delay_ms),
        )
        .await
        {
            Either3::First(command) => Some(BuzzerTaskWake::Command(command)),
            Either3::Second(command) => Some(BuzzerTaskWake::Safety(command)),
            Either3::Third(_) => None,
        }
    } else {
        match select(BUZZER_COMMANDS.receive(), BUZZER_SAFETY_COMMAND.wait()).await {
            Either::First(command) => Some(BuzzerTaskWake::Command(command)),
            Either::Second(command) => Some(BuzzerTaskWake::Safety(command)),
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
pub(crate) async fn run_buzzer_task(
    mut buzzer_timer: esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'static>>,
    mut buzzer_pwm: PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 2, true>,
    peripheral_clock: PeripheralClockConfig,
    #[cfg(feature = "buzzer-observe")] buzzer_edge_counter: Unit<'static, 0>,
) -> ! {
    let mut arbiter = BuzzerArbiter::new();
    let mut applied = BuzzerHardwareState::default();
    let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
    #[cfg(feature = "buzzer-test")]
    let mut test_session = BuzzerTestSession::new();
    #[cfg(feature = "buzzer-observe")]
    let mut output_trace = BuzzerTestOutputTrace::new();

    loop {
        let now_ms = Instant::now().as_millis();
        #[cfg(feature = "buzzer-test")]
        for decision in test_session.advance(&mut arbiter, now_ms) {
            log_buzzer_decision(decision);
        }

        let tick = arbiter.tick(now_ms);
        if let Some(decision) = tick.deferred_start {
            log_buzzer_decision(decision);
            #[cfg(feature = "buzzer-test")]
            test_session.record_deferred_start(now_ms, decision);
        }
        #[cfg(feature = "buzzer-test")]
        for decision in test_session.settle_after_tick(&mut arbiter, now_ms) {
            log_buzzer_decision(decision);
        }
        let output = arbiter.output();
        let _output_changed = apply_buzzer_output(
            &mut buzzer_timer,
            &mut buzzer_pwm,
            &peripheral_clock,
            output,
            &mut applied,
            &mut configured_frequency_hz,
        );
        #[cfg(feature = "buzzer-observe")]
        if _output_changed {
            output_trace.record(now_ms, output, buzzer_edge_counter.value() as u16);
        }
        #[cfg(all(feature = "buzzer-test", feature = "buzzer-observe"))]
        publish_buzzer_test_status(&test_session, &arbiter, &output_trace);
        #[cfg(all(feature = "buzzer-test", not(feature = "buzzer-observe")))]
        publish_buzzer_test_status(&test_session, &arbiter);

        let next_deadline_ms = arbiter.next_transition_ms();
        #[cfg(feature = "buzzer-test")]
        let next_deadline_ms = match test_session.next_deadline_ms() {
            Some(test_deadline_ms) => Some(match next_deadline_ms {
                Some(deadline_ms) => deadline_ms.min(test_deadline_ms),
                None => test_deadline_ms,
            }),
            None => next_deadline_ms,
        };

        let wake = wait_for_buzzer_wake(next_deadline_ms).await;

        if let Some(wake) = wake {
            let now_ms = Instant::now().as_millis();
            match wake {
                BuzzerTaskWake::Command(command) => {
                    #[cfg(feature = "buzzer-test")]
                    if matches!(
                        &command,
                        BuzzerRuntimeCommand::Test(BuzzerTestCommand {
                            op: BuzzerTestOp::Trigger | BuzzerTestOp::Run,
                            ..
                        })
                    ) {
                        #[cfg(feature = "buzzer-observe")]
                        output_trace.reset(now_ms, buzzer_edge_counter.value() as u16);
                    }
                    #[cfg(feature = "buzzer-test")]
                    apply_buzzer_command(command, &mut arbiter, now_ms, &mut test_session);
                    #[cfg(not(feature = "buzzer-test"))]
                    apply_buzzer_command(command, &mut arbiter, now_ms);
                }
                BuzzerTaskWake::Safety(command) => {
                    #[cfg(feature = "buzzer-test")]
                    apply_buzzer_safety_command(command, &mut arbiter, now_ms, &mut test_session);
                    #[cfg(not(feature = "buzzer-test"))]
                    apply_buzzer_safety_command(command, &mut arbiter, now_ms);
                }
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_status_light_output(
    red: &mut Output<'_>,
    green: &mut Output<'_>,
    blue: &mut Output<'_>,
    output: RgbChannels,
    last_output: &mut Option<RgbChannels>,
) {
    if last_output.is_some_and(|last| last == output) {
        return;
    }

    // LED1 is common-anode: low GPIO output sinks the selected color channel.
    if output.red {
        red.set_low();
    } else {
        red.set_high();
    }
    if output.green {
        green.set_low();
    } else {
        green.set_high();
    }
    if output.blue {
        blue.set_low();
    } else {
        blue.set_high();
    }
    *last_output = Some(output);
}

#[cfg(target_arch = "xtensa")]
pub(crate) static STATUS_LIGHT_STATE: AtomicU8 = AtomicU8::new(StatusLightState::Booting as u8);

#[cfg(target_arch = "xtensa")]
pub(crate) fn set_status_light_state(state: StatusLightState) {
    STATUS_LIGHT_STATE.store(state as u8, Ordering::Relaxed);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn status_light_state_from_code(code: u8) -> StatusLightState {
    match code {
        0 => StatusLightState::Booting,
        1 => StatusLightState::Ready,
        2 => StatusLightState::Heating,
        3 => StatusLightState::Cooling,
        4 => StatusLightState::Calibration,
        5 => StatusLightState::HeaterInterlocked,
        6 => StatusLightState::CoolingDisabledOvertemp,
        7 => StatusLightState::SensorFault,
        8 => StatusLightState::ThermalRunawayAttentionPending,
        9 => StatusLightState::ThermalRunaway,
        _ => StatusLightState::Booting,
    }
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
pub(crate) async fn run_status_light_task(
    mut red: Output<'static>,
    mut green: Output<'static>,
    mut blue: Output<'static>,
    started_ms: u64,
) -> ! {
    let mut last_output = None;
    loop {
        refresh_status_light(
            &mut red,
            &mut green,
            &mut blue,
            started_ms,
            status_light_state_from_code(STATUS_LIGHT_STATE.load(Ordering::Relaxed)),
            &mut last_output,
        );
        EmbassyTimer::after_millis(20).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn refresh_status_light(
    red: &mut Output<'_>,
    green: &mut Output<'_>,
    blue: &mut Output<'_>,
    started_ms: u64,
    state: StatusLightState,
    last_output: &mut Option<RgbChannels>,
) {
    let elapsed_ms = Instant::now().as_millis().saturating_sub(started_ms);
    apply_status_light_output(
        red,
        green,
        blue,
        status_light_output(state, elapsed_ms),
        last_output,
    );
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn sync_frontpanel_runtime_state(
    ui_state: &mut FrontPanelUiState,
    fan_decision: FanPolicyDecision,
    heater_lock_reason: Option<HeaterLockReason>,
    elapsed_ms: u64,
) -> bool {
    let mut changed = false;

    if ui_state.fan_enabled != fan_decision.command.enabled {
        ui_state.fan_enabled = fan_decision.command.enabled;
        changed = true;
    }
    if ui_state.fan_display_state != fan_decision.display_state {
        ui_state.fan_display_state = fan_decision.display_state;
        changed = true;
    }
    if ui_state.fan_policy_source != fan_decision.source {
        ui_state.fan_policy_source = fan_decision.source;
        changed = true;
    }
    if ui_state.fan_output_level != fan_decision.output_level {
        ui_state.fan_output_level = fan_decision.output_level;
        changed = true;
    }
    if ui_state.heater_lock_reason != heater_lock_reason {
        ui_state.heater_lock_reason = heater_lock_reason;
        changed = true;
    }

    let dashboard_warning_visible = next_dashboard_warning_visible(elapsed_ms, heater_lock_reason);
    if ui_state.dashboard_warning_visible != dashboard_warning_visible {
        ui_state.dashboard_warning_visible = dashboard_warning_visible;
        changed = true;
    }

    changed
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[derive(Clone, Copy)]
pub(crate) struct UsbRuntimeStatusContext {
    pub(crate) elapsed_ms: u64,
    pub(crate) pd_controller: ControllerKind,
    pub(crate) last_pd_observation: Option<PdStatusObservation>,
    pub(crate) heater_power_backend: HeaterPowerBackend,
    pub(crate) pid_snapshot: HeaterPidSnapshot,
    pub(crate) heater_control_timing: HeaterControlTiming,
    pub(crate) heater_physical_output_percent: u8,
    pub(crate) manual_pps: ManualPpsState,
    #[cfg(test)]
    pub(crate) calibration: CalibrationRuntimeState,
    pub(crate) fan_command: FanHardwareCommand,
    pub(crate) fan_working_power_pending: bool,
    pub(crate) current_rtd_fault: Option<HeaterFaultReason>,
    pub(crate) heater_fault_latched: Option<HeaterFaultReason>,
    pub(crate) attention_pending_after_fault_clear: bool,
    pub(crate) thermal_control_profile_preview: bool,
    pub(crate) active_thermal_control_profile: Option<ThermalControlProfile>,
    pub(crate) last_raw_state: FrontPanelRawState,
    pub(crate) latest_status_temp_c: f32,
    pub(crate) latest_control_temp_c: f32,
    pub(crate) control_measurement_guarded: bool,
    pub(crate) latest_rtd_raw_adc_mv: u16,
    pub(crate) latest_rtd_raw_adc_min_mv: u16,
    pub(crate) latest_rtd_raw_adc_max_mv: u16,
    pub(crate) latest_vin_raw_adc_mv: u16,
    pub(crate) vin_mv: u32,
}
