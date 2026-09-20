#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_heater_power_output<PWM>(
    mut context: HeaterPowerOutputContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    let manual_pps_active = context.manual_pps.enabled;
    let _ = release_terminal_fixed_pd_disarm_for_manual_pps(context.backend, manual_pps_active);
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
            if context.heater_enabled {
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
    match *context.backend {
        HeaterPowerBackend::FixedPdPwmFallback { .. } => {
            apply_fixed_pd_backend(context, manual_pps_active).await
        }
        HeaterPowerBackend::PpsMos { .. } => {
            apply_fixed_pd_backend(context, manual_pps_active).await
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn prepare_manual_pps<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> Option<bool>
where
    PWM: SetDutyCycle,
{
    if !context.manual_pps.enabled {
        return Some(false);
    }
    context.hold_pps_governor.reset();
    let target_mv = match require_manual_pps_target(context.manual_pps) {
        Ok(target_mv) => target_mv,
        Err(_) => {
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            return Some(true);
        }
    };
    let target_ma = context.manual_pps.target_ma.unwrap_or(0);
    if !context
        .pd_observation
        .is_some_and(|observation| observation.status.pd_active)
    {
        context.manual_pps.fail(ManualPpsError::PdNotReady);
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
        let _ = context.pd_port.restore_automatic_idle_contract();
        return Some(false);
    }
    if !manual_pps_request_required(
        *context.manual_pps,
        context.pd_port.controller_kind(),
        context.pd_observation,
    ) {
        return Some(false);
    }
    match context.pd_port.request_pps_voltage(target_mv) {
        PdContractRequestState::Confirmed => {
            context.manual_pps.applied_mv = Some(target_mv);
            info!(
                "manual pps override applied mv={=u16} ma={=u16}",
                target_mv, target_ma
            );
            Some(true)
        }
        PdContractRequestState::Pending => {
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            None
        }
        PdContractRequestState::Failed => {
            context.manual_pps.fail(ManualPpsError::WriteFailed);
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            let _ = context.pd_port.restore_automatic_idle_contract();
            info!(
                "manual pps override cleared reason={=str}",
                ManualPpsError::WriteFailed.code()
            );
            Some(false)
        }
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
    if !context.manual_pps.consume_automatic_restore_pending() {
        return;
    }
    match *context.backend {
        HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request,
            terminal_fixed_pd_disarmed,
            ..
        } => {
            *context.backend = HeaterPowerBackend::FixedPdPwmFallback {
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
            *context.backend = HeaterPowerBackend::PpsMos {
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
pub(crate) async fn apply_fixed_pd_fallback<PWM>(
    context: HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
) -> bool
where
    PWM: SetDutyCycle,
{
    let HeaterPowerOutputContext {
        pd_port,
        heater_pwm,
        backend,
        hold_pps_governor: _,
        manual_pps: _,
        pd_observation,
        measured_heater_mv: _,
        current_temp_c,
        duty_percent,
        heater_enabled: _,
        control_phase: _,
        control_error_c: _,
        filtered_slope_c_per_s: _,
        warmup_soft_start_percent,
        last_physical_duty_percent,
        preview_heater_curve,
        memory_config,
        active_thermal_settings,
        now_ms: _,
    } = context;
    match *backend {
        HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request_confirmed,
            fixed_request,
            terminal_fixed_pd_disarmed,
        } => {
            if !fixed_request_confirmed && !manual_pps_active {
                match pd_port.restore_automatic_idle_contract() {
                    PdContractRequestState::Confirmed => {
                        *backend = HeaterPowerBackend::FixedPdPwmFallback {
                            reason,
                            fixed_request_confirmed: true,
                            fixed_request,
                            terminal_fixed_pd_disarmed,
                        };
                        info!("heater backend fallback fixed-pd contract confirmed");
                    }
                    PdContractRequestState::Pending => {
                        apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                        info!(
                            "heater backend fallback waiting for fixed-pd contract reason={=str}",
                            reason.label(),
                        );
                        return false;
                    }
                    PdContractRequestState::Failed => {
                        apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                        info!(
                            "heater backend fallback fixed-pd request failed reason={=str}",
                            reason.label(),
                        );
                        return false;
                    }
                }
            }
            let negotiated_current_ma = pd_observation
                .filter(|observation| observation.status.pd_active)
                .map(|observation| observation.current_ma)
                .unwrap_or(0);
            let safe_duty_percent = fixed_pd_pwm_duty_percent(
                duty_percent,
                current_temp_c,
                pd_observation
                    .and_then(|observation| observation.contract_voltage_mv)
                    .unwrap_or_else(|| fixed_request.millivolts()),
                negotiated_current_ma,
                active_thermal_settings.heater_current_reserve_ma,
                preview_heater_curve,
                memory_config,
            );
            apply_heater_duty(
                heater_pwm,
                apply_warmup_soft_start(safe_duty_percent, warmup_soft_start_percent),
                last_physical_duty_percent,
            );
            false
        }
        _ => false,
    }
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PpsOperatingLimits {
    pps_min_mv: u16,
    adjustable_max_mv: u16,
    capability_max_ma: u16,
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
        } => Some(PpsOperatingLimits {
            pps_min_mv,
            adjustable_max_mv,
            capability_max_ma,
            current_request_mv,
        }),
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
    match context
        .pd_port
        .request_fixed_voltage(HEATER_CURRENT_LIMIT_FALLBACK_REQUEST)
    {
        PdContractRequestState::Confirmed => {
            set_pps_current_limit_backend(
                context.backend,
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                None,
                true,
            );
            CurrentLimitContractResult::Ready
        }
        PdContractRequestState::Pending => {
            set_pps_current_limit_backend(
                context.backend,
                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                Some(
                    context
                        .now_ms
                        .saturating_add(HEATER_PPS_LARGE_TRANSITION_MS),
                ),
                false,
            );
            CurrentLimitContractResult::Pending
        }
        PdContractRequestState::Failed => {
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
        let settled_gate_duty_percent = heater_physical_pwm_percent(
            context.duty_percent,
            source_request_ceiling_mv,
            current_request_mv,
            context.warmup_soft_start_percent,
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
pub(crate) struct PpsVoltageTransition {
    request_mv: u16,
    request_mode: ch224q::AdjustableVoltageMode,
    mode_changed: bool,
    voltage_changed: bool,
    request_transition_pending: bool,
    gate_duty_percent: u8,
    blank_heater: bool,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn fallback_from_adjustable_request<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    let fixed_request_confirmed = matches!(
        context.pd_port.restore_automatic_idle_contract(),
        PdContractRequestState::Confirmed
    );
    *context.backend = HeaterPowerBackend::FixedPdPwmFallback {
        reason: HeaterPowerBackendReason::AdjustableRequestFailed,
        fixed_request_confirmed,
        fixed_request: ch224q::VoltageRequest::V12,
        terminal_fixed_pd_disarmed: false,
    };
    if fixed_request_confirmed {
        let negotiated_current_ma = context
            .pd_observation
            .filter(|observation| observation.status.pd_active)
            .map(|observation| observation.current_ma)
            .unwrap_or(0);
        let safe_duty_percent = fixed_pd_pwm_duty_percent(
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
        );
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
    if !(request.voltage_changed || request.mode_changed) || request.request_transition_pending {
        return None;
    }
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        ..
    } = *context.backend
    else {
        return Some(false);
    };
    match context.pd_port.request_pps_voltage(request.request_mv) {
        PdContractRequestState::Confirmed => {}
        PdContractRequestState::Pending => {
            apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
            return Some(false);
        }
        PdContractRequestState::Failed => {
            return Some(fallback_from_adjustable_request(context).await);
        }
    }
    if should_restore_gate_after_adjustable_request(request.blank_heater, request.gate_duty_percent)
    {
        apply_heater_duty(
            context.heater_pwm,
            request.gate_duty_percent,
            context.last_physical_duty_percent,
        );
    }
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
    Some(true)
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn apply_pps_voltage_request<PWM>(
    context: &mut HeaterPowerOutputContext<'_, PWM>,
    manual_pps_active: bool,
    safe_max_mv: u16,
    source_request_ceiling_mv: u16,
    control_floor_mv: u16,
) -> bool
where
    PWM: SetDutyCycle,
{
    let HeaterPowerBackend::PpsMos {
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
    let automatic_request_mv = heater_adjustable_request_mv(
        context.duty_percent,
        context.heater_enabled,
        current_request_mv,
        idle_request_mv,
        control_floor_mv,
        source_request_ceiling_mv,
    );
    let request_mv = if manual_pps_active {
        automatic_request_mv
    } else {
        context
            .hold_pps_governor
            .request_mv(HoldPpsRequestInput {
                phase: context.control_phase,
                duty_percent: context.duty_percent,
                actual_error_c: context.control_error_c,
                filtered_slope_c_per_s: context.filtered_slope_c_per_s,
                current_request_mv,
                control_floor_mv,
                safe_max_mv: source_request_ceiling_mv,
                now_ms: context.now_ms,
            })
            .unwrap_or(automatic_request_mv)
    };
    let request_mode = adjustable_mode_for_request(request_mv, pps_max_mv);
    let mode_changed = !manual_pps_active && current_mode != Some(request_mode);
    let voltage_changed = !manual_pps_active && current_request_mv != request_mv;
    let request_transition_pending = !manual_pps_active && context.now_ms < next_request_at_ms;
    let gate_duty_percent = heater_physical_pwm_percent(
        context.duty_percent,
        source_request_ceiling_mv,
        current_request_mv,
        context.warmup_soft_start_percent,
    );
    let blank_heater =
        should_blank_heater_for_adjustable_request(current_request_mv, request_mv, mode_changed);
    if gate_duty_percent == 0 || blank_heater {
        apply_heater_duty(context.heater_pwm, 0, context.last_physical_duty_percent);
    }
    if let Some(result) = apply_pps_voltage_transition(
        &mut *context,
        PpsVoltageTransition {
            request_mv,
            request_mode,
            mode_changed,
            voltage_changed,
            request_transition_pending,
            gate_duty_percent,
            blank_heater,
        },
    )
    .await
    {
        return result;
    }
    let active_request_gate_duty_percent = if request_transition_pending {
        heater_physical_pwm_percent(
            context.duty_percent,
            source_request_ceiling_mv,
            current_request_mv,
            context.warmup_soft_start_percent,
        )
    } else {
        gate_duty_percent
    };
    apply_heater_duty(
        context.heater_pwm,
        active_request_gate_duty_percent,
        context.last_physical_duty_percent,
    );
    if voltage_changed || mode_changed {
        info!(
            "heater pps request temp_c={=f32} control={=u8}% safe_heater_mv={=u16} source_ceiling_mv={=u16} control_floor_mv={=u16} request_mv={=u16}",
            context.current_temp_c,
            context.duty_percent,
            safe_max_mv,
            source_request_ceiling_mv,
            control_floor_mv,
            request_mv,
        );
    }
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
    let Some(limits) = pps_operating_limits(&context) else {
        return false;
    };
    let effective_current_limit_ma =
        effective_pps_current_limit_ma(limits.capability_max_ma, context.pd_observation);
    let safe_max_mv = production_pps_request_ceiling_mv(
        context.current_temp_c,
        effective_current_limit_ma,
        context.active_thermal_settings.heater_current_reserve_ma,
        limits.adjustable_max_mv,
        context.preview_heater_curve,
        context.memory_config,
    );
    let source_request_ceiling_mv = heater_source_request_ceiling_mv(
        safe_max_mv,
        limits.current_request_mv,
        context.measured_heater_mv,
        limits.adjustable_max_mv,
    );
    let control_floor_mv = effective_auto_adjustable_working_floor_mv(
        context.active_thermal_settings,
        limits.pps_min_mv,
        limits.adjustable_max_mv,
    );
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
    if let Some(result) = handle_pps_settle_period(&mut context, source_request_ceiling_mv).await {
        return result;
    }
    apply_pps_voltage_request(
        &mut context,
        manual_pps_active,
        safe_max_mv,
        source_request_ceiling_mv,
        control_floor_mv,
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
