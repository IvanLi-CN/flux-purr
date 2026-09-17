#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn approach_sustain_floor_permille(
    control_target: ThermalControlTarget,
    error_c: f32,
) -> u16 {
    let full_floor = control_target
        .approach_floor_power_permille
        .max(control_target.hold_power_permille)
        .min(1_000);
    let tail_floor = control_target.hold_power_permille.min(full_floor);
    let tail_window_c = control_target.approach_tail_window_c.max(0.0);
    if tail_window_c <= f32::EPSILON || full_floor == tail_floor {
        return full_floor;
    }

    let tail_progress =
        ((error_c - control_target.hold_entry_error_c) / tail_window_c).clamp(0.0, 1.0);
    (f32::from(tail_floor)
        + ((f32::from(full_floor) - f32::from(tail_floor)) * tail_progress)
        + 0.5) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FanVoltageProfile {
    Minimum,
    SafeHalf,
    Full,
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanVoltageProfile {
    const fn pwm_permille(self) -> u16 {
        match self {
            Self::Minimum => FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
            Self::SafeHalf => FAN_HALF_SPEED_PWM_PERMILLE,
            Self::Full => FAN_FULL_SPEED_PWM_PERMILLE,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FanHardwareCommand {
    pub(crate) enabled: bool,
    pub(crate) pwm_permille: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanHardwareCommand {
    pub(crate) const fn disabled() -> Self {
        Self {
            enabled: false,
            pwm_permille: FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
        }
    }

    pub(crate) const fn from_profile(profile: FanVoltageProfile) -> Self {
        Self {
            enabled: true,
            pwm_permille: profile.pwm_permille(),
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FanPolicyState {
    Disabled,
    ActiveCooling,
    PostHeatMedium,
    PostHeatCooldown {
        until_ms: u64,
        profile: FanVoltageProfile,
    },
    SafeHalf,
    Full,
    #[cfg(test)]
    ActiveCoolingCooldown {
        until_ms: u64,
    },
    #[cfg(test)]
    CoolingDisabledPulse {
        duty_percent: u8,
    },
    HeatingGuardPulse {
        duty_percent: u8,
        pwm_permille: u16,
    },
    HeatingGuardContinuous,
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanPolicyState {
    pub(crate) const fn command(self, elapsed_ms: u64) -> FanHardwareCommand {
        match self {
            Self::Disabled => FanHardwareCommand::disabled(),
            Self::ActiveCooling => FanHardwareCommand::from_profile(FanVoltageProfile::Full),
            Self::PostHeatMedium => FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf),
            Self::PostHeatCooldown { until_ms, profile } => {
                if elapsed_ms < until_ms {
                    FanHardwareCommand::from_profile(profile)
                } else {
                    FanHardwareCommand::disabled()
                }
            }
            Self::SafeHalf => FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf),
            Self::Full => FanHardwareCommand::from_profile(FanVoltageProfile::Full),
            #[cfg(test)]
            Self::ActiveCoolingCooldown { until_ms } => {
                if elapsed_ms < until_ms {
                    FanHardwareCommand::from_profile(FanVoltageProfile::Minimum)
                } else {
                    FanHardwareCommand::disabled()
                }
            }
            #[cfg(test)]
            Self::CoolingDisabledPulse { duty_percent } => {
                if duty_percent == 0 {
                    return FanHardwareCommand::disabled();
                }

                let elapsed_in_period_ms = elapsed_ms % FAN_PULSE_PERIOD_MS;
                let on_window_ms = FAN_PULSE_PERIOD_MS.saturating_mul(duty_percent as u64) / 100;
                FanHardwareCommand {
                    enabled: elapsed_in_period_ms < on_window_ms,
                    pwm_permille: FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
                }
            }
            Self::HeatingGuardPulse {
                duty_percent,
                pwm_permille,
            } => {
                if duty_percent == 0 {
                    return FanHardwareCommand::disabled();
                }
                let elapsed_in_period_ms = elapsed_ms % 10_000;
                let on_window_ms = 10_000u64.saturating_mul(duty_percent as u64) / 100;
                FanHardwareCommand {
                    enabled: elapsed_in_period_ms < on_window_ms,
                    pwm_permille,
                }
            }
            Self::HeatingGuardContinuous => {
                FanHardwareCommand::from_profile(FanVoltageProfile::Minimum)
            }
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FanPolicyDecision {
    pub(crate) state: FanPolicyState,
    pub(crate) command: FanHardwareCommand,
    pub(crate) display_state: FanDisplayState,
    pub(crate) source: FanPolicySource,
    pub(crate) output_level: FanOutputLevel,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn is_sensor_fault(reason: Option<HeaterFaultReason>) -> bool {
    matches!(
        reason,
        Some(
            HeaterFaultReason::SensorShort
                | HeaterFaultReason::SensorOpen
                | HeaterFaultReason::AdcReadFailed
        )
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn is_overtemp_fault(reason: Option<HeaterFaultReason>) -> bool {
    reason == Some(HeaterFaultReason::OverTemp)
}

#[cfg(test)]
pub(crate) fn auto_cooling_command(
    current_temp_c: i16,
    elapsed_ms: u64,
    previous_state: FanPolicyState,
) -> FanPolicyState {
    if current_temp_c >= ACTIVE_COOLING_FAN_MIN_TEMP_C {
        FanPolicyState::ActiveCooling
    } else {
        match previous_state {
            FanPolicyState::Full | FanPolicyState::ActiveCooling => {
                FanPolicyState::ActiveCoolingCooldown {
                    until_ms: elapsed_ms.saturating_add(AUTO_COOLING_FAN_COOLDOWN_MS),
                }
            }
            FanPolicyState::ActiveCoolingCooldown { until_ms } if elapsed_ms < until_ms => {
                FanPolicyState::ActiveCoolingCooldown { until_ms }
            }
            _ => FanPolicyState::Disabled,
        }
    }
}

#[cfg(test)]
pub(crate) fn cooling_disabled_pulse_duty_percent(current_temp_c: i16) -> u8 {
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C {
        return 0;
    }

    (((current_temp_c - COOLING_DISABLED_PULSE_START_TEMP_C) / 10) as u8).min(25)
}

#[cfg(test)]
pub(crate) fn heating_fan_pulse_duty_percent(current_temp_c: i16) -> u8 {
    cooling_disabled_pulse_duty_percent(current_temp_c)
        .saturating_mul(2)
        .min(HEATING_FAN_PULSE_MAX_DUTY_PERCENT)
}

#[cfg(test)]
pub(crate) fn heating_fan_state(current_temp_c: i16, heater_output_percent: u8) -> FanPolicyState {
    if current_temp_c > COOLING_DISABLED_FAN_FULL_TEMP_C {
        return FanPolicyState::Full;
    }
    if current_temp_c > COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return FanPolicyState::SafeHalf;
    }
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C || heater_output_percent == 0 {
        return FanPolicyState::Disabled;
    }

    let duty_percent = heating_fan_pulse_duty_percent(current_temp_c);
    if duty_percent == 0 {
        return FanPolicyState::Disabled;
    }

    FanPolicyState::CoolingDisabledPulse { duty_percent }
}

#[cfg(test)]
pub(crate) fn cooling_disabled_state(current_temp_c: i16) -> FanPolicyState {
    if current_temp_c > COOLING_DISABLED_FAN_FULL_TEMP_C {
        return FanPolicyState::Full;
    }
    if current_temp_c > COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return FanPolicyState::SafeHalf;
    }
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C {
        return FanPolicyState::Disabled;
    }

    let duty_percent = cooling_disabled_pulse_duty_percent(current_temp_c);
    if duty_percent == 0 {
        return FanPolicyState::Disabled;
    }

    FanPolicyState::CoolingDisabledPulse { duty_percent }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fan_display_state_for_command(
    active_cooling_enabled: bool,
    command: FanHardwareCommand,
) -> FanDisplayState {
    if !active_cooling_enabled {
        FanDisplayState::Off
    } else if command.enabled {
        FanDisplayState::Run
    } else {
        FanDisplayState::Auto
    }
}

#[cfg(test)]
pub(crate) fn fan_policy_decision(
    current_temp_c: i16,
    elapsed_ms: u64,
    heater_enabled: bool,
    heater_output_percent: u8,
    active_cooling_enabled: bool,
    previous_state: FanPolicyState,
    hold_previous_output: bool,
) -> FanPolicyDecision {
    let state = if hold_previous_output {
        previous_state
    } else if heater_enabled {
        heating_fan_state(current_temp_c, heater_output_percent)
    } else if active_cooling_enabled {
        auto_cooling_command(current_temp_c, elapsed_ms, previous_state)
    } else {
        cooling_disabled_state(current_temp_c)
    };
    let command = state.command(elapsed_ms);

    FanPolicyDecision {
        state,
        command,
        display_state: fan_display_state_for_command(active_cooling_enabled, command),
        source: if active_cooling_enabled {
            FanPolicySource::PostHeat
        } else {
            FanPolicySource::Idle
        },
        output_level: fan_output_level_for_command(command),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fan_output_level_for_command(command: FanHardwareCommand) -> FanOutputLevel {
    if !command.enabled {
        FanOutputLevel::Off
    } else if command.pwm_permille == FAN_FULL_SPEED_PWM_PERMILLE {
        FanOutputLevel::High
    } else if command.pwm_permille <= FAN_HALF_SPEED_PWM_PERMILLE {
        FanOutputLevel::Medium
    } else if command.pwm_permille >= FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE {
        FanOutputLevel::Low
    } else {
        FanOutputLevel::Limited
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fan_display_state_for_policy(
    source: FanPolicySource,
    post_heat_mode: PostHeatCoolingMode,
    command: FanHardwareCommand,
) -> FanDisplayState {
    if matches!(source, FanPolicySource::Safety) {
        FanDisplayState::Safe
    } else {
        fan_display_state_for_command(post_heat_mode.is_enabled(), command)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolate_limited_fan_pwm(current_temp_c: i16) -> u16 {
    const LIMITED_OUTPUT_START_C: i16 = 150;
    const LIMITED_OUTPUT_END_C: i16 = 240;
    const LIMITED_OUTPUT_PWM_PERMILLE: u16 = 700;
    let progress = u32::from(
        current_temp_c
            .saturating_sub(LIMITED_OUTPUT_START_C)
            .clamp(0, LIMITED_OUTPUT_END_C - LIMITED_OUTPUT_START_C) as u16,
    );
    let span = u32::from(FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE - LIMITED_OUTPUT_PWM_PERMILLE);
    FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE.saturating_sub(
        ((span * progress) / u32::from((LIMITED_OUTPUT_END_C - LIMITED_OUTPUT_START_C) as u16))
            as u16,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn guard_pulse_percent(current_temp_c: i16, mode: HeatingFanGuardMode) -> u8 {
    let (start_c, full_c) = match mode {
        HeatingFanGuardMode::Low => (100, 200),
        HeatingFanGuardMode::Medium => (80, 150),
        HeatingFanGuardMode::Off | HeatingFanGuardMode::High => return 0,
    };
    if current_temp_c <= start_c {
        return 0;
    }
    let span = (full_c - start_c) as u32;
    let progress = u32::from((current_temp_c - start_c).min(full_c - start_c) as u16);
    (10 + ((40 * progress) / span)) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fan_policy_decision_with_modes(
    current_temp_c: i16,
    elapsed_ms: u64,
    heater_enabled: bool,
    heater_just_disabled: bool,
    post_heat_mode: PostHeatCoolingMode,
    guard_mode: HeatingFanGuardMode,
    previous: (FanPolicyState, bool),
) -> FanPolicyDecision {
    let (previous_state, hold_previous_output) = previous;
    let (state, source) = if hold_previous_output {
        (previous_state, FanPolicySource::Safety)
    } else if heater_enabled {
        match guard_mode {
            HeatingFanGuardMode::High if current_temp_c > 80 => (
                FanPolicyState::HeatingGuardContinuous,
                FanPolicySource::HeatingGuard,
            ),
            HeatingFanGuardMode::Low | HeatingFanGuardMode::Medium => {
                heating_guard_pulse_state(current_temp_c, guard_mode)
            }
            HeatingFanGuardMode::Off | HeatingFanGuardMode::High => {
                (FanPolicyState::Disabled, FanPolicySource::HeatingGuard)
            }
        }
    } else if post_heat_mode.is_enabled()
        && (heater_just_disabled
            || matches!(
                previous_state,
                FanPolicyState::PostHeatMedium
                    | FanPolicyState::PostHeatCooldown { .. }
                    | FanPolicyState::ActiveCooling
            ))
    {
        let state = match previous_state {
            FanPolicyState::PostHeatCooldown { until_ms, profile } => {
                if elapsed_ms < until_ms {
                    FanPolicyState::PostHeatCooldown { until_ms, profile }
                } else {
                    FanPolicyState::Disabled
                }
            }
            _ => match post_heat_mode {
                PostHeatCoolingMode::Normal => {
                    if current_temp_c > 40 {
                        FanPolicyState::PostHeatMedium
                    } else {
                        FanPolicyState::PostHeatCooldown {
                            until_ms: elapsed_ms.saturating_add(AUTO_COOLING_FAN_COOLDOWN_MS),
                            profile: FanVoltageProfile::Minimum,
                        }
                    }
                }
                PostHeatCoolingMode::Fast => {
                    if current_temp_c > 60 {
                        FanPolicyState::ActiveCooling
                    } else if current_temp_c > 40 {
                        FanPolicyState::PostHeatMedium
                    } else {
                        FanPolicyState::PostHeatCooldown {
                            until_ms: elapsed_ms.saturating_add(AUTO_COOLING_FAN_COOLDOWN_MS),
                            profile: FanVoltageProfile::SafeHalf,
                        }
                    }
                }
                PostHeatCoolingMode::Off => FanPolicyState::Disabled,
            },
        };
        (state, FanPolicySource::PostHeat)
    } else {
        (FanPolicyState::Disabled, FanPolicySource::Idle)
    };
    let command = state.command(elapsed_ms);
    FanPolicyDecision {
        state,
        command,
        display_state: fan_display_state_for_policy(source, post_heat_mode, command),
        source,
        output_level: fan_output_level_for_command(command),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heating_guard_pulse_state(
    current_temp_c: i16,
    guard_mode: HeatingFanGuardMode,
) -> (FanPolicyState, FanPolicySource) {
    let duty_percent = guard_pulse_percent(current_temp_c, guard_mode);
    if duty_percent == 0 {
        return (FanPolicyState::Disabled, FanPolicySource::HeatingGuard);
    }
    let pwm_permille = matches!(guard_mode, HeatingFanGuardMode::Medium)
        .then_some(current_temp_c)
        .filter(|temp_c| *temp_c > 150)
        .map(interpolate_limited_fan_pwm)
        .unwrap_or(FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE);
    (
        FanPolicyState::HeatingGuardPulse {
            duty_percent,
            pwm_permille,
        },
        FanPolicySource::HeatingGuard,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn overtemp_forced_fan_state(
    current_temp_c: i16,
    forced_fan_active: bool,
) -> Option<FanPolicyState> {
    if !forced_fan_active || current_temp_c < FORCED_COOLING_FAN_MIN_TEMP_C {
        None
    } else if current_temp_c > FORCED_COOLING_FAN_FULL_TEMP_C {
        Some(FanPolicyState::Full)
    } else {
        Some(FanPolicyState::SafeHalf)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn startup_pd_contract_ready(observation: Option<PdStatusObservation>) -> bool {
    observation.is_some_and(|observation| observation.status.pd_active)
}

// Source_Capabilities may arrive immediately after CC attachment. Reserve one
// bounded startup window for the policy before shared-I2C initialization work.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const STARTUP_PD_SERVICE_BUDGET_MS: u64 = 750;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn startup_pd_service_should_continue(
    fusb302b_present: bool,
    contract_ready: bool,
    service_available: bool,
    elapsed_ms: u64,
) -> bool {
    fusb302b_present
        && service_available
        && !contract_ready
        && elapsed_ms < STARTUP_PD_SERVICE_BUDGET_MS
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupSequenceStage {
    BacklightReady,
    PdServiceComplete,
    DisplayReady,
    StartupFrameReady,
    OtherInitialization,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartupSequence {
    completed: Option<StartupSequenceStage>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl StartupSequence {
    pub(crate) const fn new() -> Self {
        Self { completed: None }
    }

    pub(crate) const fn advance(&mut self, next: StartupSequenceStage) -> bool {
        let expected = match self.completed {
            None => StartupSequenceStage::BacklightReady,
            Some(StartupSequenceStage::BacklightReady) => StartupSequenceStage::PdServiceComplete,
            Some(StartupSequenceStage::PdServiceComplete) => StartupSequenceStage::DisplayReady,
            Some(StartupSequenceStage::DisplayReady) => StartupSequenceStage::StartupFrameReady,
            Some(StartupSequenceStage::StartupFrameReady) => {
                StartupSequenceStage::OtherInitialization
            }
            Some(StartupSequenceStage::OtherInitialization) => return false,
        };
        match (expected, next) {
            (StartupSequenceStage::BacklightReady, StartupSequenceStage::BacklightReady)
            | (StartupSequenceStage::PdServiceComplete, StartupSequenceStage::PdServiceComplete)
            | (StartupSequenceStage::DisplayReady, StartupSequenceStage::DisplayReady)
            | (StartupSequenceStage::StartupFrameReady, StartupSequenceStage::StartupFrameReady)
            | (
                StartupSequenceStage::OtherInitialization,
                StartupSequenceStage::OtherInitialization,
            ) => {
                self.completed = Some(next);
                true
            }
            _ => false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupFrontPanelPresentation {
    Splash,
    Calibration,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn startup_frontpanel_presentation(
    runtime_mode: FrontPanelRuntimeMode,
) -> StartupFrontPanelPresentation {
    match runtime_mode {
        FrontPanelRuntimeMode::App => StartupFrontPanelPresentation::Splash,
        FrontPanelRuntimeMode::KeyTest => StartupFrontPanelPresentation::Calibration,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn next_heater_lock_reason(
    heater_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    thermal_model_heater_allowed: bool,
    pd_contract_ready: bool,
) -> Option<HeaterLockReason> {
    if is_sensor_fault(heater_fault) {
        Some(HeaterLockReason::SensorFault)
    } else if heater_fault == Some(HeaterFaultReason::OverTemp) {
        Some(HeaterLockReason::HardOvertemp)
    } else if cooling_disabled_lock_latched {
        Some(HeaterLockReason::CoolingDisabledOvertemp)
    } else if !pd_contract_ready {
        Some(HeaterLockReason::PdContractUnavailable)
    } else if !thermal_model_heater_allowed {
        Some(HeaterLockReason::ThermalModelMissingForSourceClass)
    } else {
        None
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn next_heater_lock_reason_with_persistence(
    persistence_locked: bool,
    heater_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    thermal_model_heater_allowed: bool,
    pd_contract_ready: bool,
) -> Option<HeaterLockReason> {
    if persistence_locked {
        Some(HeaterLockReason::PersistenceRequired)
    } else {
        next_heater_lock_reason(
            heater_fault,
            cooling_disabled_lock_latched,
            thermal_model_heater_allowed,
            pd_contract_ready,
        )
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn next_dashboard_warning_visible(
    elapsed_ms: u64,
    heater_lock_reason: Option<HeaterLockReason>,
) -> bool {
    heater_lock_reason.is_some()
        && (elapsed_ms / DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS).is_multiple_of(2)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn reconcile_cooling_disabled_lock(
    active_cooling_enabled: bool,
    current_temp_c: i16,
    has_sensor_fault: bool,
    latched: bool,
    armed: bool,
) -> (bool, bool, bool) {
    if active_cooling_enabled {
        return (false, true, latched);
    }
    if has_sensor_fault {
        return (latched, armed, false);
    }
    if current_temp_c <= COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return (latched, true, false);
    }
    if armed {
        return (true, false, !latched);
    }

    (latched, armed, false)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn is_overtemp_sample(temp_c: f32) -> bool {
    temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn overtemp_fault_from_control_temperature(temp_c: f32) -> Option<HeaterFaultReason> {
    is_overtemp_sample(temp_c).then_some(HeaterFaultReason::OverTemp)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn clear_runtime_temperature(latest_temp_c: &mut f32, latest_temp_i16: &mut i16) {
    *latest_temp_c = 0.0;
    *latest_temp_i16 = 0;
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn sync_runtime_temperature_ui(
    ui_state: &mut FrontPanelUiState,
    current_temp_c: i16,
    current_temp_deci_c: i16,
) -> bool {
    let mut needs_redraw = false;
    if ui_state.current_temp_c != current_temp_c {
        ui_state.current_temp_c = current_temp_c;
        needs_redraw = true;
    }
    if ui_state.current_temp_deci_c != current_temp_deci_c {
        ui_state.current_temp_deci_c = current_temp_deci_c;
        needs_redraw = true;
    }
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_runtime_display_temperature(
    ui_state: &mut FrontPanelUiState,
    latest_display_temp_c: &mut f32,
    latest_display_temp_i16: &mut i16,
    temp_c: f32,
) -> bool {
    *latest_display_temp_c = temp_c;
    *latest_display_temp_i16 = temp_c_to_whole_c(temp_c);
    let mut needs_redraw = false;
    if matches!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::Initializing
            | flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault
    ) {
        needs_redraw = ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
        );
    }
    needs_redraw |=
        sync_runtime_temperature_ui(ui_state, *latest_display_temp_i16, temp_c_to_deci_c(temp_c));
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) struct RuntimeDisplayTemperatureState<'a> {
    pub(crate) ui_state: &'a mut FrontPanelUiState,
    pub(crate) latest_display_temp_c: &'a mut f32,
    pub(crate) latest_display_temp_i16: &'a mut i16,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) struct RuntimeControlTemperatureState<'a> {
    pub(crate) latest_control_temp_c: &'a mut f32,
    pub(crate) latest_control_temp_i16: &'a mut i16,
    pub(crate) transition_guard: &'a mut RtdPpsTransitionGuard,
    pub(crate) measurement_guard: &'a mut RtdControlMeasurementGuard,
    pub(crate) control_measurement_guarded: &'a mut bool,
    pub(crate) heater_controller: &'a mut HeaterController,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn apply_valid_rtd_measurement(
    display: RuntimeDisplayTemperatureState<'_>,
    control: RuntimeControlTemperatureState<'_>,
    request_mv: u16,
    now_ms: u64,
    measurement_temp_c: f32,
) -> bool {
    let needs_redraw = update_runtime_display_temperature(
        display.ui_state,
        display.latest_display_temp_c,
        display.latest_display_temp_i16,
        measurement_temp_c,
    );
    if let Some(control_temp_c) = accept_rtd_control_sample_after_pps_transition(
        control.transition_guard,
        control.heater_controller,
        control.measurement_guard,
        *control.latest_control_temp_c,
        request_mv,
        now_ms,
        measurement_temp_c,
    ) {
        *control.latest_control_temp_c = control_temp_c;
        *control.latest_control_temp_i16 = temp_c_to_whole_c(control_temp_c);
    }
    *control.control_measurement_guarded = control.measurement_guard.guarded;
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn retain_runtime_display_temperature(
    ui_state: &mut FrontPanelUiState,
    latest_display_temp_c: &mut f32,
    latest_display_temp_i16: &mut i16,
) -> bool {
    sync_runtime_temperature_ui(
        ui_state,
        *latest_display_temp_i16,
        temp_c_to_deci_c(*latest_display_temp_c),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RtdPpsTransitionGuard {
    request_mv: Option<u16>,
    blocked_until_ms: Option<u64>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl RtdPpsTransitionGuard {
    pub(crate) fn new(request_mv: u16) -> Self {
        Self {
            request_mv: Some(request_mv),
            ..Self::default()
        }
    }

    pub(crate) fn observe(&mut self, request_mv: u16, now_ms: u64) -> (bool, bool) {
        let request_changed = self.request_mv != Some(request_mv);
        self.request_mv = Some(request_mv);
        if request_changed {
            self.blocked_until_ms =
                Some(now_ms.saturating_add(RTD_CONTROL_SAMPLE_STABLE_AFTER_REQUEST_MS));
            return (false, true);
        }

        let accept_control_sample = match self.blocked_until_ms {
            Some(blocked_until_ms) if now_ms < blocked_until_ms => false,
            Some(_) => {
                self.blocked_until_ms = None;
                return (true, true);
            }
            None => true,
        };

        (accept_control_sample, false)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RtdControlMeasurementGuard {
    pub(crate) last_accepted_temp_c: Option<f32>,
    pub(crate) last_accepted_at_ms: Option<u64>,
    pub(crate) guarded_candidate_temp_c: Option<f32>,
    pub(crate) guarded_candidate_since_ms: Option<u64>,
    pub(crate) guarded: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl RtdControlMeasurementGuard {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn reseed(&mut self, temp_c: f32, now_ms: u64) {
        self.last_accepted_temp_c = Some(temp_c);
        self.last_accepted_at_ms = Some(now_ms);
        self.guarded_candidate_temp_c = None;
        self.guarded_candidate_since_ms = None;
        self.guarded = false;
    }

    pub(crate) fn observe(&mut self, measurement_temp_c: f32, now_ms: u64) -> Option<f32> {
        self.guarded = false;
        let Some(last_temp_c) = self.last_accepted_temp_c else {
            self.reseed(measurement_temp_c, now_ms);
            return Some(measurement_temp_c);
        };
        let elapsed_ms = now_ms
            .saturating_sub(self.last_accepted_at_ms.unwrap_or(now_ms))
            .max(25);
        let max_delta_c = (RTD_CONTROL_MAX_SLEW_C_PER_S * (elapsed_ms as f32 / 1_000.0))
            .min(RTD_CONTROL_MAX_ACCEPTED_STEP_C);
        if (measurement_temp_c - last_temp_c).abs() > max_delta_c {
            let candidate_is_consistent = self.guarded_candidate_temp_c.is_some_and(|candidate| {
                (measurement_temp_c - candidate).abs() <= RTD_CONTROL_GUARD_RECOVERY_BAND_C
            });
            if !candidate_is_consistent {
                self.guarded_candidate_temp_c = Some(measurement_temp_c);
                self.guarded_candidate_since_ms = Some(now_ms);
            }
            if candidate_is_consistent
                && now_ms.saturating_sub(self.guarded_candidate_since_ms.unwrap_or(now_ms))
                    >= RTD_CONTROL_GUARD_RECOVERY_WINDOW_MS
            {
                self.reseed(measurement_temp_c, now_ms);
                return Some(measurement_temp_c);
            }
            self.guarded = true;
            return None;
        }

        self.reseed(measurement_temp_c, now_ms);
        Some(measurement_temp_c)
    }

    pub(crate) fn observe_with_heater_duty(
        &mut self,
        measurement_temp_c: f32,
        now_ms: u64,
        heater_duty_percent: u8,
    ) -> Option<f32> {
        if let (Some(last_temp_c), Some(last_at_ms)) =
            (self.last_accepted_temp_c, self.last_accepted_at_ms)
        {
            let elapsed_ms = now_ms.saturating_sub(last_at_ms).max(25);
            let max_unpowered_rise_c = (RTD_CONTROL_MAX_UNPOWERED_RISE_C_PER_S
                * (elapsed_ms as f32 / 1_000.0))
                .min(RTD_CONTROL_MAX_UNPOWERED_RISE_STEP_C);
            if heater_duty_percent == 0
                && measurement_temp_c > last_temp_c
                && measurement_temp_c - last_temp_c > max_unpowered_rise_c
            {
                // A plate can retain heat after coasting, but it cannot make a
                // multi-degree step while the heater output is physically off.
                // Keep the raw reading visible and preserve the last trusted
                // control temperature instead of allowing a persistent ADC
                // artifact to reseed the control loop.
                self.guarded_candidate_temp_c = None;
                self.guarded_candidate_since_ms = None;
                self.guarded = true;
                return None;
            }
        }

        self.observe(measurement_temp_c, now_ms)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn preserve_rtd_control_guard_when_heater_disabled(
    heater_enabled: bool,
    measurement_guarded: &mut bool,
) {
    if !heater_enabled {
        *measurement_guarded = false;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn accept_rtd_control_sample_after_pps_transition(
    transition_guard: &mut RtdPpsTransitionGuard,
    heater_controller: &mut HeaterController,
    measurement_guard: &mut RtdControlMeasurementGuard,
    last_control_temp_c: f32,
    request_mv: u16,
    now_ms: u64,
    measurement_temp_c: f32,
) -> Option<f32> {
    let (accept_control_sample, reseed_filter) = transition_guard.observe(request_mv, now_ms);
    if reseed_filter {
        heater_controller.reseed_measurement(last_control_temp_c);
        measurement_guard.reseed(last_control_temp_c, now_ms);
    }
    let was_guarded = measurement_guard.guarded;
    let accepted = accept_control_sample
        .then(|| {
            measurement_guard.observe_with_heater_duty(
                measurement_temp_c,
                now_ms,
                heater_controller.duty_percent,
            )
        })
        .flatten();
    if was_guarded && accepted.is_some() {
        heater_controller.reseed_measurement(measurement_temp_c);
    }
    accepted
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_retry_rtd_sample_after_power_step(
    previous_request_mv: u16,
    current_request_mv: u16,
    previous_vin_raw_adc_mv: u16,
    current_vin_raw_adc_mv: u16,
) -> bool {
    previous_request_mv != current_request_mv
        || previous_vin_raw_adc_mv.abs_diff(current_vin_raw_adc_mv)
            >= RTD_RETRY_AFTER_VIN_STEP_RAW_ADC_DELTA_MV
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn should_clear_runtime_fault_latch(
    heater_rearm_requested: bool,
    current_rtd_fault: Option<HeaterFaultReason>,
    latched_fault: Option<HeaterFaultReason>,
) -> bool {
    heater_rearm_requested && current_rtd_fault.is_none() && latched_fault.is_some()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) trait BuzzerCueSink {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64);
    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64);
    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64);
    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    );
    fn clear_attention(&mut self);
    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64);
}

#[cfg(test)]
impl BuzzerCueSink for BuzzerArbiter {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64) {
        let _ = BuzzerArbiter::request_feedback(self, source, cue, now_ms);
    }

    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::activate_protection(self, source, now_ms);
    }

    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::request_protection_replay(self, source, now_ms);
    }

    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) {
        let _ = BuzzerArbiter::enter_attention_pending(self);
        let _ = BuzzerArbiter::request_attention_reminder(self, source, now_ms);
    }

    fn clear_attention(&mut self) {
        let _ = BuzzerArbiter::clear_attention(self);
    }

    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::request_attention_reminder(self, source, now_ms);
    }
}

#[cfg(target_arch = "xtensa")]
impl BuzzerCueSink for BuzzerRuntime {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64) {
        BuzzerRuntime::request_feedback(self, source, cue, now_ms);
    }

    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::activate_protection(self, source, now_ms);
    }

    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::request_protection_replay(self, source, now_ms);
    }

    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) {
        BuzzerRuntime::enter_attention_pending_and_request_reminder(self, source, now_ms);
    }

    fn clear_attention(&mut self) {
        BuzzerRuntime::clear_attention(self);
    }

    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::request_attention_reminder(self, source, now_ms);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) struct FaultAttentionState<'a> {
    pub(crate) last_fault_present: &'a mut bool,
    pub(crate) attention_acknowledged: &'a mut bool,
    pub(crate) attention_pending_after_fault_clear: &'a mut bool,
    pub(crate) forced_fan_active: &'a mut bool,
    pub(crate) protection_alarm: &'a mut ProtectionAlarmCadence,
    pub(crate) next_attention_reminder_ms: &'a mut Option<u64>,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn update_fault_attention_state<B: BuzzerCueSink>(
    fault_present: bool,
    state: FaultAttentionState<'_>,
    current_temp_c: i16,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    let FaultAttentionState {
        last_fault_present,
        attention_acknowledged,
        attention_pending_after_fault_clear,
        forced_fan_active,
        protection_alarm,
        next_attention_reminder_ms,
    } = state;
    let mut changed = false;

    if fault_present && !*last_fault_present {
        *attention_acknowledged = false;
        *attention_pending_after_fault_clear = false;
        *forced_fan_active = true;
        *next_attention_reminder_ms = None;
        protection_alarm.arm(now_ms);
        buzzer.activate_protection(BuzzerCueSource::ThermalProtection, now_ms);
        changed = true;
    } else if !fault_present && *last_fault_present {
        *attention_pending_after_fault_clear = !*attention_acknowledged;
        protection_alarm.clear();
        if *attention_pending_after_fault_clear {
            // The protection alarm has just stopped. Give the operator an
            // immediate reminder, then keep the existing ten-second cadence.
            buzzer.enter_attention_pending_and_request_reminder(
                BuzzerCueSource::ThermalAttention,
                now_ms,
            );
            *next_attention_reminder_ms =
                Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        } else {
            buzzer.clear_attention();
            *next_attention_reminder_ms = None;
        }
        changed = true;
    }

    if *forced_fan_active
        && (*attention_acknowledged || current_temp_c < FORCED_COOLING_FAN_MIN_TEMP_C)
    {
        *forced_fan_active = false;
        changed = true;
    }

    *last_fault_present = fault_present;
    changed
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn overtemp_attention_requires_ack(
    overtemp_active: bool,
    attention_acknowledged: bool,
    attention_pending_after_fault_clear: bool,
) -> bool {
    (overtemp_active && !attention_acknowledged) || attention_pending_after_fault_clear
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn acknowledge_overtemp_attention<B: BuzzerCueSink>(
    overtemp_active: bool,
    attention_acknowledged: &mut bool,
    attention_pending_after_fault_clear: &mut bool,
    forced_fan_active: &mut bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut B,
) -> bool {
    if !overtemp_attention_requires_ack(
        overtemp_active,
        *attention_acknowledged,
        *attention_pending_after_fault_clear,
    ) {
        return false;
    }

    *attention_acknowledged = true;
    *attention_pending_after_fault_clear = false;
    *forced_fan_active = false;
    *next_attention_reminder_ms = None;
    if !overtemp_active {
        buzzer.clear_attention();
    }
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn maybe_play_protection_alarm<B: BuzzerCueSink>(
    fault_present: bool,
    protection_alarm: &mut ProtectionAlarmCadence,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !protection_alarm.replay_due(fault_present, now_ms) {
        return false;
    }
    buzzer.request_protection_replay(BuzzerCueSource::ThermalProtection, now_ms);
    true
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_consume_attention_raw_input(
    attention_pending_after_fault_clear: bool,
    suppressing_current_input: bool,
    previous_raw_state: FrontPanelRawState,
    current_raw_state: FrontPanelRawState,
) -> bool {
    attention_pending_after_fault_clear
        && !suppressing_current_input
        && current_raw_state != previous_raw_state
        && current_raw_state.pressed_mask() != 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn should_clear_attention_ack_suppression(
    suppressing_current_input: bool,
    waits_for_delayed_event: bool,
    suppressed_event_seen: bool,
    current_raw_state: FrontPanelRawState,
    clear_after_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    suppressing_current_input
        && current_raw_state.pressed_mask() == 0
        && (!waits_for_delayed_event
            || suppressed_event_seen
            || clear_after_ms.is_some_and(|deadline| now_ms >= deadline))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn maybe_play_attention_reminder<B: BuzzerCueSink>(
    attention_pending_after_fault_clear: bool,
    fault_present: bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !attention_pending_after_fault_clear || fault_present {
        return false;
    }

    if next_attention_reminder_ms.is_some_and(|next| now_ms >= next) {
        buzzer.request_attention_reminder(BuzzerCueSource::ThermalAttention, now_ms);
        *next_attention_reminder_ms =
            Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        return true;
    }

    false
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn maybe_play_frontpanel_ui_input_feedback<B: BuzzerCueSink>(
    interaction_handled: bool,
    specialized_feedback_played: bool,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !interaction_handled || specialized_feedback_played {
        return false;
    }

    buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, now_ms);
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_buzzer_decision(decision: BuzzerDecision) {
    info!(
        "buzzer arbitration source={=str} cue={=str} disposition={=str}",
        decision.source.label(),
        decision.cue.label(),
        decision.disposition.label(),
    );
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn temp_c_to_deci_c(temp_c: f32) -> i16 {
    let scaled = temp_c * 10.0;
    let rounded = if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    };
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn temp_c_to_centi_c(temp_c: f32) -> i32 {
    let scaled = temp_c * 100.0;
    let rounded = if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    };
    rounded.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn temp_c_to_whole_c(temp_c: f32) -> i16 {
    let rounded = if temp_c >= 0.0 {
        temp_c + 0.5
    } else {
        temp_c - 0.5
    };
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RtdMeasurement {
    pub(crate) raw_adc_mv: u16,
    pub(crate) raw_adc_min_mv: u16,
    pub(crate) raw_adc_max_mv: u16,
    pub(crate) adc_mv: u16,
    pub(crate) resistance_ohms: f32,
    pub(crate) temp_c: f32,
    pub(crate) current_temp_c: i16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RtdAdcBatch {
    pub(crate) mean_mv: f32,
    pub(crate) min_mv: u16,
    pub(crate) max_mv: u16,
    pub(crate) mean_raw_code: u16,
    pub(crate) min_raw_code: u16,
    pub(crate) max_raw_code: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AdcConvertedSample {
    pub(crate) raw_code: u16,
    pub(crate) calibrated_mv: u16,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static ADC_CALIBRATION_SOURCE: AtomicU8 = AtomicU8::new(2);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static ADC_EFUSE_VERSION: AtomicU8 = AtomicU8::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static ADC_INIT_CODE: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static ADC_REFERENCE_CODE: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static ADC_REFERENCE_MV: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static RTD_RAW_CODE_MEAN: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static RTD_RAW_CODE_MIN: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static RTD_RAW_CODE_MAX: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) static VIN_RAW_CODE_MEAN: AtomicU16 = AtomicU16::new(0);

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_IDLE: u8 = 0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_WAITING_CC_ATTACH: u8 = 1;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_WAITING_SOURCE_CAPS: u8 = 2;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_SOURCE_CAPS_REQUESTED: u8 = 3;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_WAITING_ACCEPT: u8 = 4;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_WAITING_PS_RDY: u8 = 5;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_RECOVERING: u8 = 6;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_FAULT: u8 = 7;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED: u8 = 8;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN: u8 = 9;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_PROTECTION: u8 = 10;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_MISSING_CRC: u8 = 11;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_MISSING_SOP: u8 = 12;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_UNSUPPORTED_SOP: u8 = 13;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_RX_I2C_ERROR: u8 = 14;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_TX_I2C_ERROR: u8 = 15;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_NO_USABLE_CONTRACT: u8 = 16;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_RX_PARTIAL: u8 = 17;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_DIAG_REQUEST_TIMEOUT: u8 = 19;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_PARTIAL_RX_TIMEOUT_MS: u64 = 250;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS: u64 = 1_500;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) static FUSB302B_DIAGNOSTIC: AtomicU8 = AtomicU8::new(FUSB302B_DIAG_IDLE);
