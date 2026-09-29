#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTOMATIC_PPS_TICKET_WAIT_MS: u64 = FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS + 500;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomaticPpsTicketDecision {
    Wait,
    Confirmed,
    Failed,
}

#[cfg(any(target_arch = "xtensa", test))]
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
#[derive(Clone, Copy)]
pub(crate) struct PendingAutomaticPpsTransition {
    pub(crate) ticket: PowerTicket,
    pub(crate) request_mv: u16,
    pub(crate) request_mode: ch224q::AdjustableVoltageMode,
    pub(crate) mode_changed: bool,
    pub(crate) blank_heater: bool,
    pub(crate) gate_duty_percent: u8,
    pub(crate) started_at_ms: u64,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct HeaterPowerOutputContext<'a, PWM> {
    pub(crate) pd_port: &'a PdPort,
    pub(crate) heater_pwm: &'a mut PWM,
    pub(crate) backend: &'a mut HeaterPowerBackend,
    pub(crate) pending_automatic_pps: &'a mut Option<PendingAutomaticPpsTransition>,
    pub(crate) hold_pps_governor: &'a mut HoldPpsGovernor,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) pd_observation: Option<PdStatusObservation>,
    pub(crate) measured_heater_mv: u32,
    pub(crate) current_temp_c: f32,
    pub(crate) duty_percent: u8,
    pub(crate) heater_enabled: bool,
    pub(crate) tuning_power_stage: bool,
    pub(crate) control_phase: HeaterControlPhase,
    pub(crate) control_error_c: f32,
    pub(crate) filtered_slope_c_per_s: f32,
    pub(crate) warmup_soft_start_percent: u8,
    pub(crate) last_physical_duty_percent: &'a mut u8,
    pub(crate) preview_heater_curve: Option<&'a HeaterCurveConfig>,
    pub(crate) memory_config: &'a MemoryConfig,
    pub(crate) active_thermal_settings: ThermalControlProfileSettings,
    pub(crate) now_ms: u64,
}
