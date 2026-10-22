#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn apply_permitted_heater_duty<PWM>(
    heater_pwm: &mut PWM,
    requested_percent: u8,
    last_percent: &mut u8,
    permit_active: bool,
) -> Result<(), PWM::Error>
where
    PWM: embedded_hal::pwm::SetDutyCycle,
{
    let effective_percent = if permit_active { requested_percent } else { 0 };
    // The PD task can clear the physical pin independently of this cache.
    // Reassert every permitted frame, even when the requested duty is unchanged.
    heater_pwm.set_duty_cycle_percent(effective_percent)?;
    *last_percent = effective_percent;
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTOMATIC_PPS_TICKET_WAIT_MS: u64 = FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS + 500;

#[cfg(test)]
pub(crate) fn live_pps_gate_duty_percent(
    duty_percent: u8,
    heater_enabled: bool,
    blank_heater: bool,
    source_ceiling_mv: u16,
    active_voltage_bound_mv: u16,
    warmup_soft_start_percent: u8,
) -> u8 {
    if !heater_enabled || blank_heater {
        return 0;
    }
    heater_physical_pwm_percent(
        duty_percent,
        source_ceiling_mv,
        active_voltage_bound_mv,
        warmup_soft_start_percent,
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomaticPpsTicketDecision {
    Wait,
    Confirmed,
    Failed,
}

#[cfg(test)]
pub(crate) fn automatic_pps_ticket_decision(
    outcome: Option<TicketOutcome>,
    started_at_ms: u64,
    now_ms: u64,
    requested_mv: u16,
    requested_ma: u16,
) -> AutomaticPpsTicketDecision {
    match outcome {
        Some(TicketOutcome::Confirmed(active))
            if active.mode == PdContractRequestMode::Pps
                && active.voltage_mv == requested_mv
                && active.operating_current_ma == requested_ma =>
        {
            AutomaticPpsTicketDecision::Confirmed
        }
        Some(_) => AutomaticPpsTicketDecision::Failed,
        None if now_ms.saturating_sub(started_at_ms) <= AUTOMATIC_PPS_TICKET_WAIT_MS => {
            AutomaticPpsTicketDecision::Wait
        }
        None => AutomaticPpsTicketDecision::Failed,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct HeaterPowerOutputContext<'a, PWM> {
    pub(crate) pd_port: &'a PdPort,
    pub(crate) heater_pwm: &'a mut PWM,
    pub(crate) backend: &'a mut HeaterPowerBackend,
    pub(crate) pending_automatic_power_request: &'a mut Option<PendingAutomaticPowerRequest>,
    pub(crate) hold_pps_governor: &'a mut HoldPpsGovernor,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) pd_observation: Option<PdStatusObservation>,
    pub(crate) measured_heater_mv: u32,
    pub(crate) vin_raw_adc_mv: u16,
    pub(crate) rtd_raw_adc_mv: u16,
    pub(crate) current_temp_c: f32,
    pub(crate) source_capabilities: SourceCapabilitiesView,
    pub(crate) requested_power_mw: u32,
    pub(crate) duty_percent: u8,
    pub(crate) heater_enabled: bool,
    pub(crate) normal_auto: bool,
    pub(crate) supply_scan: bool,
    pub(crate) automatic_supply_response_latched: &'a mut bool,
    pub(crate) tuning_power_stage: bool,
    pub(crate) control_phase: HeaterControlPhase,
    pub(crate) initial_profile_approach: bool,
    pub(crate) control_error_c: f32,
    pub(crate) filtered_slope_c_per_s: f32,
    pub(crate) warmup_soft_start_percent: u8,
    pub(crate) last_physical_duty_percent: &'a mut u8,
    pub(crate) preview_heater_curve: Option<&'a HeaterCurveConfig>,
    pub(crate) memory_config: &'a MemoryConfig,
    pub(crate) active_thermal_settings: ThermalControlProfileSettings,
    pub(crate) now_ms: u64,
}
