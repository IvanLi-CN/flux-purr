#[allow(unused_imports)]
use super::*;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::heater_phase::{HOLD_ENTRY_BELOW_C, hold_entry_ready};

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TuningTemperatureEvidence {
    pub(crate) temperature_centi_c: i16,
    pub(crate) measurement_valid: bool,
    pub(crate) measurement_accepted: bool,
    pub(crate) cooldown_admission_ready: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn tuning_temperature_evidence(
    live_rtd_temp_c: f32,
    sensor_fault: bool,
    control_guarded: bool,
    control_accepted_at_ms: Option<u64>,
    sample_elapsed_ms: u64,
) -> TuningTemperatureEvidence {
    let measurement_valid = !sensor_fault && live_rtd_temp_c.is_finite();
    let measurement_accepted =
        measurement_valid && !control_guarded && control_accepted_at_ms == Some(sample_elapsed_ms);
    TuningTemperatureEvidence {
        temperature_centi_c: (live_rtd_temp_c * 100.0).round() as i16,
        measurement_valid,
        measurement_accepted,
        cooldown_admission_ready: measurement_accepted,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn tuning_temperature_evidence_with_control(
    live_rtd_temp_c: f32,
    control_temp_c: f32,
    sensor_fault: bool,
    control_guarded: bool,
    control_accepted_at_ms: Option<u64>,
    sample_elapsed_ms: u64,
) -> TuningTemperatureEvidence {
    let measurement_valid =
        !sensor_fault && live_rtd_temp_c.is_finite() && control_temp_c.is_finite();
    let measurement_accepted =
        measurement_valid && !control_guarded && control_accepted_at_ms == Some(sample_elapsed_ms);
    TuningTemperatureEvidence {
        temperature_centi_c: (control_temp_c * 100.0).round() as i16,
        measurement_valid,
        measurement_accepted,
        cooldown_admission_ready: measurement_accepted,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn active_thermal_control_profile(
    memory_config: &MemoryConfig,
    preview: Option<ThermalControlProfile>,
    manual_pps: &ManualPpsState,
) -> Option<ThermalControlProfile> {
    preview.or_else(|| {
        ThermalControlProfile::from_saved_config(memory_config.thermal_profile(
            resolve_thermal_profile_bank(memory_config.thermal_profile_mode, manual_pps),
        ))
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn resolve_thermal_profile_bank(
    mode: ThermalProfileMode,
    manual_pps: &ManualPpsState,
) -> ThermalProfileBank {
    if mode == ThermalProfileMode::Auto && manual_pps.has_matching_pps_apdo(20_000, 5_000) {
        ThermalProfileBank::Pps5a
    } else {
        mode.default_bank()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProductionThermalController {
    Unavailable,
    Profile(ThermalProfileBank),
}

#[cfg(any(target_arch = "xtensa", test))]
impl ProductionThermalController {
    pub(crate) const fn is_profile(self) -> bool {
        matches!(self, Self::Profile(_))
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_output_settings(
    profile: Option<ThermalControlProfile>,
    controller: ProductionThermalController,
    calibration_mode: CalibrationMode,
) -> ThermalControlProfileSettings {
    profile
        .filter(|_| controller.is_profile() || calibration_mode != CalibrationMode::Off)
        .map(|profile| profile.settings)
        .unwrap_or_default()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn production_thermal_controller(
    power_state: PowerState,
    mode: ThermalProfileMode,
    manual_pps: &ManualPpsState,
) -> ProductionThermalController {
    let contract_continuous = match power_state.protocol {
        PowerProtocol::Ready => true,
        PowerProtocol::WaitingForAccept | PowerProtocol::WaitingForPsRdy => power_state
            .requested
            .is_some_and(|request| pps_adjustment_is_continuous(power_state, request)),
        _ => false,
    };
    let bank = resolve_thermal_profile_bank(mode, manual_pps);
    let required_ma = match bank {
        ThermalProfileBank::Pps3a => 3_000,
        ThermalProfileBank::Pps5a => 5_000,
    };
    if contract_continuous
        && power_state.available
        && power_state.failure.is_none()
        && power_state.active.is_some_and(|active| {
            active.mode == PdContractRequestMode::Pps && active.operating_current_ma >= required_ma
        })
        && manual_pps.has_matching_pps_apdo(20_000, required_ma)
    {
        ProductionThermalController::Profile(bank)
    } else {
        ProductionThermalController::Unavailable
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_pps_working_contract_pending(
    power_state: PowerState,
    mode: ThermalProfileMode,
    manual_pps: &ManualPpsState,
) -> bool {
    !production_thermal_controller(power_state, mode, manual_pps).is_profile()
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn heater_phase_pps_request_mv(
    governor: &mut HoldPpsGovernor,
    input: HoldPpsRequestInput,
    initial_profile_approach: bool,
) -> Option<u16> {
    if initial_profile_approach {
        // Only Hold, or an Approach recovering from Hold, owns the handoff
        // voltage. A fresh profile Approach must still use the normal down-ramp.
        governor.reset();
        return None;
    }
    governor.request_mv(input)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn thermal_control_runtime_wire(
    target_temp_c: i16,
    profile: Option<ThermalControlProfile>,
    preview_active: bool,
) -> ThermalControlRuntimeWire {
    let profile_active = profile.is_some();
    let profile_covers_target = profile.is_some_and(|value| value.covers_target(target_temp_c));
    let target = profile
        .map(|value| value.control_target(target_temp_c))
        .unwrap_or_else(|| default_thermal_control_target(target_temp_c));
    let mut profile_source = heapless::String::new();
    let _ = profile_source.push_str(if preview_active {
        "preview"
    } else if profile_active {
        "saved"
    } else {
        "default"
    });
    ThermalControlRuntimeWire {
        profile_active,
        profile_covers_target,
        profile_source,
        target_temp_c,
        brake_distance_centi_c: round_to_u16_nonnegative(target.brake_distance_c * 100.0),
        warmup_power_permille: target.warmup_power_permille,
        approach_power_permille: target.approach_power_permille,
        approach_floor_power_permille: target.approach_floor_power_permille,
        approach_damping_exponent_permille: round_to_u16_nonnegative(
            target.approach_damping_exponent * 1_000.0,
        ),
        approach_tail_window_centi_c: round_to_u16_nonnegative(
            target.approach_tail_window_c * 100.0,
        ),
        hold_power_permille: target.hold_power_permille,
        hold_reheat_power_permille: target.hold_reheat_power_permille,
        hold_entry_centi_c: round_to_u16_nonnegative(target.hold_entry_error_c * 100.0),
        hold_exit_centi_c: round_to_u16_nonnegative(target.hold_exit_error_c * 100.0),
        hold_on_centi_c: round_to_u16_nonnegative(target.hold_on_error_c * 100.0),
        hold_off_centi_c: round_to_u16_nonnegative(target.hold_off_error_c * 100.0),
        overshoot_cutoff_centi_c: round_to_u16_nonnegative(target.overshoot_cutoff_c * 100.0),
        hold_kp_permille_per_c: round_to_u16_nonnegative(target.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: round_to_u16_nonnegative(target.hold_ki_permille_per_c_tick),
        hold_blend_ticks: u16::from(target.hold_blend_ticks),
        approach_lead_ticks: u16::from(target.approach_lead_ticks),
        hold_lead_ticks: u16::from(target.hold_lead_ticks),
        temp_filter_alpha_permille: round_to_u16_nonnegative(
            target.settings.temp_filter_alpha * 1_000.0,
        ),
        warmup_reenter_centi_c: round_to_u16_nonnegative(target.warmup_reenter_error_c * 100.0),
        approach_max_ticks: u16::from(target.settings.approach_max_ticks),
        approach_min_power_ratio_permille: round_to_u16_nonnegative(
            target.settings.approach_min_power_ratio * 1_000.0,
        ),
        auto_adjustable_working_floor_mv: target.settings.auto_adjustable_working_floor_mv,
        heater_current_reserve_ma: target.settings.heater_current_reserve_ma,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_thermal_control_target(target_temp_c: i16) -> ThermalControlTarget {
    default_thermal_control_target_with_settings(
        target_temp_c,
        ThermalControlProfileSettings::default(),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_thermal_control_target_with_settings(
    target_temp_c: i16,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    let target = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
    let (
        brake_distance_centi_c,
        approach_power_permille,
        approach_floor_power_permille,
        hold_power_permille,
        approach_damping_exponent_permille,
    ) = default_power_parameters(target);
    let (
        hold_entry_error_c,
        hold_exit_error_c,
        hold_off_error_c,
        overshoot_cutoff_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
    ) = default_hold_parameters(target);
    ThermalControlTarget {
        brake_distance_c: brake_distance_centi_c as f32 / 100.0,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: settings.warmup_reenter_error_c,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent: f32::from(approach_damping_exponent_permille) / 1_000.0,
        approach_tail_window_c: 0.0,
        hold_power_permille,
        hold_entry_error_c,
        hold_exit_error_c,
        hold_off_error_c,
        overshoot_cutoff_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        hold_reheat_power_permille: hold_power_permille,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        hold_on_error_c: settings.hold_on_error_c,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_power_parameters(target: i16) -> (u16, u16, u16, u16, u16) {
    let brake = match target {
        ..=100 => 450,
        101..=180 => 700,
        181..=250 => 1_000,
        _ => 1_400,
    };
    let approach = match target {
        ..=100 => 380,
        101..=180 => 320,
        181..=250 => 260,
        _ => 220,
    };
    let floor = match target {
        ..=100 => 120,
        101..=180 => 200,
        181..=250 => 320,
        _ => 380,
    };
    let hold = match target {
        ..=100 => 180,
        101..=180 => 220,
        181..=250 => 260,
        _ => 300,
    };
    let damping = match target {
        ..=100 => 1_400,
        101..=140 => 1_000,
        141..=180 => 800,
        181..=220 => 550,
        _ => 350,
    };
    (brake, approach, floor, hold, damping)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn default_hold_parameters(target: i16) -> (f32, f32, f32, f32, f32, f32, u8) {
    let hold_entry = match target {
        ..=60 => 0.35,
        61..=100 => 0.25,
        101..=140 => 0.20,
        141..=180 => 0.18,
        181..=220 => 0.15,
        _ => 0.12,
    };
    let hold_exit = match target {
        ..=60 => 1.6,
        61..=100 => 1.2,
        101..=140 => 1.0,
        141..=180 => 0.9,
        181..=220 => 0.8,
        _ => 0.7,
    };
    let hold_off = match target {
        ..=60 => 0.7,
        61..=100 => 0.8,
        101..=140 => 0.9,
        141..=180 => 1.0,
        181..=220 => 1.2,
        _ => 1.5,
    };
    let overshoot = match target {
        ..=60 => 0.8,
        61..=100 => 0.9,
        101..=140 => 1.0,
        141..=180 => 1.2,
        181..=220 => 1.4,
        _ => 1.6,
    };
    let kp = match target {
        ..=60 => 70.0,
        61..=100 => 55.0,
        101..=140 => 42.0,
        141..=180 => 30.0,
        181..=220 => 22.0,
        _ => 18.0,
    };
    let ki = match target {
        ..=60 => 3.0,
        61..=100 => 2.0,
        101..=140 => 1.5,
        _ => 1.0,
    };
    let blend = match target {
        ..=100 => 16,
        101..=180 => 12,
        _ => 8,
    };
    (hold_entry, hold_exit, hold_off, overshoot, kp, ki, blend)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolate_thermal_control_target(
    target_temp_c: i16,
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    if lower.target_temp_c >= upper.target_temp_c {
        return direct_thermal_control_target(lower, settings);
    }

    let span = f32::from(upper.target_temp_c - lower.target_temp_c);
    let ratio = (f32::from(target_temp_c - lower.target_temp_c) / span).clamp(0.0, 1.0);
    interpolate_thermal_control_target_linear(lower, upper, settings, ratio)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn direct_thermal_control_target(
    point: ThermalControlProfilePoint,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    ThermalControlTarget {
        brake_distance_c: point.brake_distance_centi_c as f32 / 100.0,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: f32::from(point.warmup_reenter_centi_c) / 100.0,
        approach_power_permille: point.approach_power_permille,
        approach_floor_power_permille: point.approach_floor_power_permille,
        approach_damping_exponent: f32::from(point.approach_damping_exponent_permille) / 1_000.0,
        approach_tail_window_c: f32::from(point.approach_tail_window_centi_c) / 100.0,
        hold_power_permille: point.hold_power_permille,
        hold_reheat_power_permille: point.hold_reheat_power_permille,
        hold_entry_error_c: f32::from(point.hold_entry_centi_c) / 100.0,
        hold_exit_error_c: f32::from(point.hold_exit_centi_c) / 100.0,
        hold_on_error_c: f32::from(point.hold_on_centi_c) / 100.0,
        hold_off_error_c: f32::from(point.hold_off_centi_c) / 100.0,
        overshoot_cutoff_c: f32::from(point.overshoot_cutoff_centi_c) / 100.0,
        hold_kp_permille_per_c: f32::from(point.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: f32::from(point.hold_ki_permille_per_c_tick),
        hold_blend_ticks: point.hold_blend_ticks.clamp(1, u16::from(u8::MAX)) as u8,
        approach_lead_ticks: point.approach_lead_ticks.min(u16::from(u8::MAX)) as u8,
        hold_lead_ticks: point.hold_lead_ticks.min(u16::from(u8::MAX)) as u8,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolate_thermal_control_target_linear(
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    settings: ThermalControlProfileSettings,
    ratio: f32,
) -> ThermalControlTarget {
    let (brake, approach, floor, damping, tail, hold, reheat) =
        interpolated_power_values(lower, upper, ratio);
    let (entry, exit, on, off, overshoot, kp, ki, blend, approach_lead, hold_lead) =
        interpolated_error_values(lower, upper, ratio);
    ThermalControlTarget {
        brake_distance_c: f32::from(brake) / 100.0,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: f32::from(interpolate_u16(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            ratio,
            5_000,
        )) / 100.0,
        approach_power_permille: approach,
        approach_floor_power_permille: floor,
        approach_damping_exponent: f32::from(damping) / 1_000.0,
        approach_tail_window_c: f32::from(tail) / 100.0,
        hold_power_permille: hold,
        hold_reheat_power_permille: reheat,
        hold_entry_error_c: f32::from(entry) / 100.0,
        hold_exit_error_c: f32::from(exit) / 100.0,
        hold_on_error_c: f32::from(on) / 100.0,
        hold_off_error_c: f32::from(off) / 100.0,
        overshoot_cutoff_c: f32::from(overshoot) / 100.0,
        hold_kp_permille_per_c: f32::from(kp),
        hold_ki_permille_per_c_tick: f32::from(ki),
        hold_blend_ticks: blend,
        approach_lead_ticks: approach_lead,
        hold_lead_ticks: hold_lead,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolate_u16(left: u16, right: u16, ratio: f32, upper_bound: u16) -> u16 {
    (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
        .clamp(0.0, f32::from(upper_bound)) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolated_power_values(
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    ratio: f32,
) -> (u16, u16, u16, u16, u16, u16, u16) {
    let linear_brake = interpolate_u16(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        ratio,
        5_000,
    );
    let midpoint = 4.0 * ratio * (1.0 - ratio);
    let adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 {
            0.55
        } else {
            0.20
        }
    } else {
        0.0
    };
    let brake = (f32::from(linear_brake) * (1.0 - adjustment * midpoint) + 0.5) as u16;
    let hold_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - 0.20 * midpoint
    } else {
        1.0
    };
    let reheat_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - 0.10 * midpoint
    } else {
        1.0
    };
    let scale_hold = |value: u16| (f32::from(value) * hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    (
        brake,
        interpolate_u16(
            lower.approach_power_permille,
            upper.approach_power_permille,
            ratio,
            1_000,
        ),
        interpolate_u16(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            ratio,
            1_000,
        ),
        interpolate_u16(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            ratio,
            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        ),
        interpolate_u16(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            ratio,
            THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
        ),
        scale_hold(interpolate_u16(
            lower.hold_power_permille,
            upper.hold_power_permille,
            ratio,
            1_000,
        )),
        (f32::from(interpolate_u16(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            ratio,
            1_000,
        )) * reheat_scale
            + 0.5) as u16,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn interpolated_error_values(
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    ratio: f32,
) -> (u16, u16, u16, u16, u16, u16, u16, u8, u8, u8) {
    let interpolate =
        |left: u16, right: u16, bound: u16| interpolate_u16(left, right, ratio, bound);
    (
        interpolate(lower.hold_entry_centi_c, upper.hold_entry_centi_c, 5_000),
        interpolate(lower.hold_exit_centi_c, upper.hold_exit_centi_c, 5_000),
        interpolate(lower.hold_on_centi_c, upper.hold_on_centi_c, 5_000),
        interpolate(lower.hold_off_centi_c, upper.hold_off_centi_c, 5_000),
        interpolate(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        ),
        interpolate(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        ),
        interpolate(
            lower.hold_ki_permille_per_c_tick,
            upper.hold_ki_permille_per_c_tick,
            10_000,
        ),
        interpolate(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)) as u8,
        interpolate(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
        interpolate(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn percent_from_permille(permille: u16) -> u8 {
    ((u32::from(permille.min(1_000)) + 5) / 10).min(100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn control_cycles_from_profile_ticks(profile_ticks: u16) -> u16 {
    if profile_ticks == 0 {
        return 0;
    }
    let numerator = u32::from(profile_ticks) * HEATER_PROFILE_TICK_MS as u32;
    let denominator = HEATER_CONTROL_INTERVAL_MS as u32;
    numerator.div_ceil(denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn scaled_filter_alpha_for_control_interval(alpha_per_profile_tick: f32) -> f32 {
    scaled_filter_alpha_for_elapsed(
        alpha_per_profile_tick,
        HEATER_CONTROL_INTERVAL_MS,
        HEATER_PROFILE_TICK_MS,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn scaled_filter_alpha_for_elapsed(alpha: f32, elapsed_ms: u64, reference_ms: u64) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return 0.0;
    }
    if alpha >= 1.0 {
        return 1.0;
    }
    if elapsed_ms == reference_ms {
        return alpha;
    }
    let profile_fraction = elapsed_ms as f32 / reference_ms as f32;
    (1.0 - (1.0 - alpha).powf(profile_fraction)).clamp(0.0, 1.0)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn scaled_hold_ki_for_control_interval(ki_per_profile_tick: f32) -> f32 {
    ki_per_profile_tick.max(0.0)
        * (HEATER_CONTROL_INTERVAL_MS as f32 / HEATER_PROFILE_TICK_MS as f32)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn next_heater_control_deadline_ms(deadline_ms: u64, control_started_ms: u64) -> u64 {
    let next_deadline_ms = deadline_ms.saturating_add(HEATER_CONTROL_INTERVAL_MS);
    if next_deadline_ms > control_started_ms {
        return next_deadline_ms;
    }

    let missed_intervals = control_started_ms
        .saturating_sub(next_deadline_ms)
        .saturating_div(HEATER_CONTROL_INTERVAL_MS)
        .saturating_add(1);
    next_deadline_ms.saturating_add(missed_intervals.saturating_mul(HEATER_CONTROL_INTERVAL_MS))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn warmup_handoff_error_c(
    brake_distance_c: f32,
    warmup_reenter_error_c: f32,
    filtered_slope_c_per_profile_tick: f32,
    approach_lead_ticks: u8,
) -> f32 {
    let predictive_distance_c =
        filtered_slope_c_per_profile_tick.max(0.0) * f32::from(approach_lead_ticks);
    let reentry_margin_c = (warmup_reenter_error_c - HEATER_HOLD_PHASE_HYSTERESIS_C).max(0.0);
    predictive_distance_c
        .max(brake_distance_c)
        .min(brake_distance_c + reentry_margin_c)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn warmup_handoff_ready(
    actual_error_c: f32,
    previous_actual_error_c: f32,
    filtered_error_c: f32,
    brake_distance_c: f32,
    handoff_error_c: f32,
) -> bool {
    let predictive_actual_margin_c = 1.0;
    let max_filtered_lag_for_static_brake_c = 3.0;
    let actual_handoff_delta_c = previous_actual_error_c - actual_error_c;
    let actual_step_confirmed = actual_handoff_delta_c <= handoff_error_c + 1.0;
    let previous_handoff_confirmed = previous_actual_error_c <= handoff_error_c + 1.0;
    let actual_brake_confirmed = actual_error_c <= brake_distance_c
        && previous_actual_error_c <= brake_distance_c + 0.5
        && filtered_error_c <= brake_distance_c + max_filtered_lag_for_static_brake_c
        && actual_step_confirmed;
    let predictive_actual_confirmed = actual_error_c
        <= brake_distance_c + predictive_actual_margin_c
        && previous_actual_error_c <= brake_distance_c + predictive_actual_margin_c + 0.5;
    let predictive_handoff_confirmed = actual_error_c.max(filtered_error_c) <= handoff_error_c
        && previous_handoff_confirmed
        && actual_step_confirmed
        && predictive_actual_confirmed;
    actual_brake_confirmed || predictive_handoff_confirmed
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn hold_effective_base_permille(
    hold_guard_error_c: f32,
    hold_reenter_error_c: f32,
    control_target: ThermalControlTarget,
) -> f32 {
    let hold_power_permille = f32::from(control_target.hold_power_permille.min(1_000));
    let hold_reheat_permille = f32::from(
        control_target
            .hold_reheat_power_permille
            .max(control_target.hold_power_permille)
            .min(1_000),
    );
    if hold_guard_error_c <= 0.0 || hold_reheat_permille <= hold_power_permille {
        return hold_power_permille;
    }

    let ratio = (hold_guard_error_c / hold_reenter_error_c.max(0.05)).clamp(0.0, 1.0);
    hold_power_permille + ((hold_reheat_permille - hold_power_permille) * ratio)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct BuzzerHardwareState {
    pub(crate) frequency_hz: Option<u32>,
    pub(crate) duty_percent: u8,
    pub(crate) generation: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn buzzer_timer_reconfiguration_needed(
    configured_frequency_hz: u32,
    next_state: BuzzerHardwareState,
) -> bool {
    next_state
        .frequency_hz
        .is_some_and(|frequency_hz| frequency_hz != configured_frequency_hz)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuzzerHardwareAction {
    StopTimer,
    Retune(u32),
    SetDutyPercent(u8),
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn buzzer_hardware_actions(
    configured_frequency_hz: u32,
    next_state: BuzzerHardwareState,
) -> heapless::Vec<BuzzerHardwareAction, 4> {
    let mut actions = heapless::Vec::new();
    if buzzer_timer_reconfiguration_needed(configured_frequency_hz, next_state) {
        let frequency_hz = next_state
            .frequency_hz
            .expect("timer reconfiguration requires an audible buzzer frequency");
        let _ = actions.push(BuzzerHardwareAction::SetDutyPercent(0));
        let _ = actions.push(BuzzerHardwareAction::StopTimer);
        let _ = actions.push(BuzzerHardwareAction::Retune(frequency_hz));
    }
    let _ = actions.push(BuzzerHardwareAction::SetDutyPercent(
        next_state.duty_percent,
    ));
    actions
}

#[cfg(any(test, all(target_arch = "xtensa", feature = "buzzer-observe")))]
pub(crate) fn mcpwm_timer_frequency_hz(prescaler: u8, period_ticks: u16) -> u32 {
    MCPWM_PERIPHERAL_CLOCK_HZ / (u32::from(prescaler) + 1) / (u32::from(period_ticks) + 1)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn buzzer_timer_period_ticks(frequency_hz: u32) -> Option<u16> {
    if frequency_hz == 0 {
        return None;
    }
    let timer_clock_hz = MCPWM_PERIPHERAL_CLOCK_HZ / (u32::from(BUZZER_TIMER_PRESCALER) + 1);
    let period_counts = timer_clock_hz
        .saturating_add(frequency_hz / 2)
        .checked_div(frequency_hz)?;
    if period_counts == 0 || period_counts > u32::from(u16::MAX) + 1 {
        return None;
    }
    Some((period_counts - 1) as u16)
}

#[cfg(any(test, all(target_arch = "xtensa", feature = "buzzer-observe")))]
pub(crate) fn buzzer_observed_frequency_hz(rising_edges: u16, window_ms: u32) -> Option<u32> {
    if window_ms == 0 {
        return None;
    }
    Some(u32::from(rising_edges).saturating_mul(1_000) / window_ms)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeaterController {
    pub(crate) fault_latched: Option<HeaterFaultReason>,
    pub(crate) last_target_temp_c: i16,
    pub(crate) filtered_temp_c: Option<f32>,
    pub(crate) previous_filtered_temp_c: Option<f32>,
    pub(crate) filtered_slope_c_per_profile_tick: f32,
    pub(crate) previous_measured_temp_c: Option<f32>,
    pub(crate) last_accepted_measurement_at_ms: Option<u64>,
    pub(crate) phase: HeaterControlPhase,
    pub(crate) phase_ticks: u16,
    pub(crate) phase_tick_remainder_ms: u64,
    pub(crate) profile_last_control_at_ms: Option<u64>,
    pub(crate) profile_measurement_elapsed_ms: u64,
    pub(crate) pps_measurement_anchor_pending: bool,
    pub(crate) recovering_from_hold: bool,
    pub(crate) duty_percent: u8,
    pub(crate) hold_entry_output_percent: u8,
    pub(crate) hold_integral_c: f32,
    pub(crate) hold_coast_active: bool,
    pub(crate) hold_coast_cooling_samples: u8,
    pub(crate) coast_cooling_elapsed_ms: u64,
    pub(crate) coast_cooling_reference_c: Option<f32>,
    pub(crate) natural_cooling_input: Option<ThermalPlantRuntimeInput>,
    pub(crate) applied_heat: flux_purr_firmware::thermal_trajectory::AppliedHeatHistory,
    pub(crate) approach_trajectory:
        Option<flux_purr_firmware::thermal_trajectory::TemperatureTrajectory>,
    pub(crate) approach_last_request_permille: Option<u16>,
    pub(crate) heater_was_enabled: bool,
    pub(crate) warmup_started_at_ms: Option<u64>,
    pub(crate) production_controller: Option<ProductionThermalController>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ThermalPlantRuntimeInput {
    pub(crate) target_temp_c: i16,
    pub(crate) measured_temp_c: f32,
    pub(crate) measurement_accepted: bool,
    pub(crate) ambient_temp_c: f32,
    pub(crate) heater_enabled: bool,
    pub(crate) model: flux_purr_firmware::memory::ThermalPlantProjection,
    pub(crate) max_power_mw: f32,
    pub(crate) now_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
pub(crate) struct ThermalControlFrame {
    control_target: ThermalControlTarget,
    elapsed_ms: u64,
    approach_max_cycles: u16,
    hold_blend_cycles: u16,
    hold_ki: f32,
    error_c: f32,
    previous_error_c: f32,
    filtered_temp_c: f32,
    filtered_slope_c_per_profile_tick: f32,
    control_error_c: f32,
    approach_control_error_c: f32,
    hold_control_error_c: f32,
    hold_prediction_blocks_reheat: bool,
    hold_filter_lag_blocks_reheat: bool,
    hold_actual_overshoot_blocks_reheat: bool,
    approach_guard_error_c: f32,
    hold_entry_gate_c: f32,
    hold_state_ready: bool,
    actual_crossed_target_ready: bool,
    hold_guard_error_c: f32,
    hold_reenter_error_c: f32,
    approach_predictive_coast_ready: bool,
    hold_exit_error_c: f32,
    brake_distance_c: f32,
    warmup_handoff_error_c: f32,
    natural_cooling_rate_c_per_s: Option<f32>,
    natural_cooling_delay_s: f32,
    natural_cooling_feedforward_percent: u8,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn control_cycle_parameters(
    target: ThermalControlTarget,
    elapsed_ms: u64,
) -> (f32, u16, u16, f32) {
    (
        scaled_filter_alpha_for_elapsed(
            target.settings.temp_filter_alpha,
            elapsed_ms,
            HEATER_PROFILE_TICK_MS,
        ),
        control_cycles_from_profile_ticks(u16::from(target.settings.approach_max_ticks)).max(1),
        control_cycles_from_profile_ticks(u16::from(target.hold_blend_ticks)).max(1),
        scaled_hold_ki_for_control_interval(target.hold_ki_permille_per_c_tick),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterController {
    pub(crate) fn initial_profile_approach(&self) -> bool {
        self.production_controller
            .is_some_and(ProductionThermalController::is_profile)
            && self.phase == HeaterControlPhase::Approach
            && !self.recovering_from_hold
    }

    pub(crate) const fn new() -> Self {
        Self {
            fault_latched: None,
            last_target_temp_c: 0,
            filtered_temp_c: None,
            previous_filtered_temp_c: None,
            filtered_slope_c_per_profile_tick: 0.0,
            previous_measured_temp_c: None,
            last_accepted_measurement_at_ms: None,
            phase: HeaterControlPhase::Warmup,
            phase_ticks: 0,
            phase_tick_remainder_ms: 0,
            profile_last_control_at_ms: None,
            profile_measurement_elapsed_ms: 0,
            pps_measurement_anchor_pending: false,
            recovering_from_hold: false,
            duty_percent: 0,
            hold_entry_output_percent: 0,
            hold_integral_c: 0.0,
            hold_coast_active: false,
            hold_coast_cooling_samples: 0,
            coast_cooling_elapsed_ms: 0,
            coast_cooling_reference_c: None,
            natural_cooling_input: None,
            applied_heat: flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new(),
            approach_trajectory: None,
            approach_last_request_permille: None,
            heater_was_enabled: false,
            warmup_started_at_ms: None,
            production_controller: None,
        }
    }

    pub(crate) const fn fault_latched(&self) -> Option<HeaterFaultReason> {
        self.fault_latched
    }

    pub(crate) fn clear_fault_latch(&mut self) {
        self.clear_approach_braking();
        self.applied_heat = flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        self.fault_latched = None;
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.last_accepted_measurement_at_ms = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.phase_tick_remainder_ms = 0;
        self.profile_last_control_at_ms = None;
        self.profile_measurement_elapsed_ms = 0;
        self.pps_measurement_anchor_pending = false;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        self.production_controller = None;
    }

    pub(crate) fn update_production_at(
        &mut self,
        kind: ProductionThermalController,
        input: ThermalPlantRuntimeInput,
        profile: Option<ThermalControlProfile>,
    ) -> (HeaterPidSnapshot, bool) {
        let changed = self.production_controller != Some(kind);
        let disarm_for_source_change =
            self.production_controller.is_some() && changed && input.heater_enabled;
        if changed {
            self.reset_disabled_output(input.target_temp_c, input.measured_temp_c);
            self.production_controller = Some(kind);
        }
        let heater_enabled = input.heater_enabled && !disarm_for_source_change && kind.is_profile();
        self.natural_cooling_input = kind.is_profile().then_some(input);
        self.applied_heat.configure(input.model.transport_delay_ms);
        let snapshot = self.update_at(
            input.target_temp_c,
            input.measured_temp_c,
            heater_enabled,
            profile,
            input.now_ms,
        );
        (snapshot, disarm_for_source_change)
    }

    pub(crate) fn reseed_measurement(&mut self, measured_temp_c: f32) {
        // PPS steps and accepted RTD guard recovery reseed the measurement
        // filter during an active Approach. They do not cancel heat in flight.
        self.filtered_temp_c = Some(measured_temp_c);
        self.previous_filtered_temp_c = Some(measured_temp_c);
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = Some(measured_temp_c);
        self.last_accepted_measurement_at_ms = None;
        self.pps_measurement_anchor_pending = false;
        self.coast_cooling_reference_c = None;
        self.coast_cooling_elapsed_ms = 0;
        self.applied_heat.reset_temperature_observation();
    }

    pub(crate) fn reseed_measurement_after_pps_step(&mut self, measured_temp_c: f32) {
        // A voltage step invalidates the ADC filter's temperature origin, not
        // a trusted positive thermal velocity. Erasing that velocity invents
        // deceleration and lets the model restore heat while the plate rises.
        // Negative velocity cannot survive a reset as cooling evidence.
        let model_approach = self.heater_was_enabled
            && self.fault_latched.is_none()
            && self.phase == HeaterControlPhase::Approach
            && self.model_approach_available();
        let rising_slope = if model_approach && self.filtered_slope_c_per_profile_tick.is_finite() {
            self.filtered_slope_c_per_profile_tick.max(0.0)
        } else {
            0.0
        };
        // The observer origin changes, never the trajectory or applied heat.
        let mut applied_heat = self.applied_heat;
        let last_accepted_measurement_at_ms = self.last_accepted_measurement_at_ms;
        applied_heat.reanchor_temperature();
        self.reseed_measurement(measured_temp_c);
        self.last_accepted_measurement_at_ms = last_accepted_measurement_at_ms;
        self.applied_heat = applied_heat;
        self.filtered_slope_c_per_profile_tick = rising_slope;
        // The held control value still belongs to the old voltage epoch.
        // Only the first accepted new-epoch sample can establish its origin.
        self.pps_measurement_anchor_pending = true;
    }

    pub(crate) fn latch_fault(&mut self, reason: HeaterFaultReason) -> bool {
        self.clear_approach_braking();
        self.applied_heat = flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        let changed = self.fault_latched != Some(reason);
        self.fault_latched = Some(reason);
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.last_accepted_measurement_at_ms = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.phase_tick_remainder_ms = 0;
        self.profile_last_control_at_ms = None;
        self.profile_measurement_elapsed_ms = 0;
        self.pps_measurement_anchor_pending = false;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        changed
    }

    #[cfg(test)]
    pub(crate) fn update(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
        thermal_profile: Option<ThermalControlProfile>,
    ) -> HeaterPidSnapshot {
        self.update_at(
            target_temp_c,
            measured_temp_c,
            heater_enabled,
            thermal_profile,
            0,
        )
    }

    pub(crate) fn update_at(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
        thermal_profile: Option<ThermalControlProfile>,
        now_ms: u64,
    ) -> HeaterPidSnapshot {
        let target_temp_c = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
        let last_target_temp_c = self.last_target_temp_c;
        self.last_target_temp_c = target_temp_c;
        let previous_measured_temp_c =
            if self.pps_measurement_anchor_pending && self.control_measurement_accepted() {
                measured_temp_c
            } else {
                self.previous_measured_temp_c
                    .or(self.filtered_temp_c)
                    .unwrap_or(measured_temp_c)
            };
        if self.control_measurement_accepted() {
            self.previous_measured_temp_c = Some(measured_temp_c);
        }

        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
        }

        if !heater_enabled || self.fault_latched.is_some() {
            return self.reset_disabled_output(target_temp_c, measured_temp_c);
        }

        if target_temp_c != last_target_temp_c {
            self.reset_for_target_change(measured_temp_c, now_ms);
        }

        if self.control_measurement_accepted() {
            self.last_accepted_measurement_at_ms = Some(now_ms);
        }

        if !self.heater_was_enabled {
            self.warmup_started_at_ms = Some(now_ms);
        }
        self.heater_was_enabled = true;
        let elapsed_ms = self.profile_control_elapsed_ms(now_ms);

        let control_target = thermal_profile
            .map(|profile| profile.control_target(target_temp_c))
            .unwrap_or_else(|| default_thermal_control_target(target_temp_c));
        self.observe_temperature_transfer(now_ms, measured_temp_c);
        let frame = self.measure_control_frame(
            target_temp_c,
            measured_temp_c,
            previous_measured_temp_c,
            control_target,
            elapsed_ms,
        );
        let ThermalControlFrame {
            control_target,
            error_c,
            filtered_temp_c,
            filtered_slope_c_per_profile_tick,
            control_error_c,
            hold_control_error_c,
            approach_predictive_coast_ready,
            ..
        } = frame;

        let previous_phase = self.phase;
        self.advance_phase(&frame, now_ms);
        if self.phase != HeaterControlPhase::Approach {
            self.clear_approach_braking();
        }

        if previous_phase != self.phase {
            self.update_hold_transition(&frame, measured_temp_c);
        }
        // With a calibrated delay, a positive sensor derivative describes
        // arriving old heat, not the temperature when this command arrives.
        // The final energy limit owns modelled Hold braking and recovery.
        if self.phase == HeaterControlPhase::Hold && self.model_approach_available() {
            self.hold_coast_active = false;
        }
        self.update_coast_state(&frame, measured_temp_c, previous_measured_temp_c);
        let entered_approach = previous_phase == HeaterControlPhase::Warmup
            && self.phase == HeaterControlPhase::Approach;
        let duty_percent = self.calculate_duty_percent(&frame, entered_approach);
        let predictive_coast_active = self.hold_coast_active
            || (self.phase == HeaterControlPhase::Approach && approach_predictive_coast_ready);
        let model_approach_active =
            self.phase == HeaterControlPhase::Approach && self.model_approach_available();
        let duty_percent = if predictive_coast_active || model_approach_active {
            duty_percent
        } else {
            self.apply_under_target_reheat_floor(
                duty_percent,
                error_c,
                hold_control_error_c,
                control_target,
            )
        };
        let duty_percent = if self.phase == HeaterControlPhase::Hold {
            self.limit_hold_arrival_power(&frame, duty_percent)
        } else {
            duty_percent
        };
        let duty_percent =
            if self.phase != HeaterControlPhase::Warmup && !self.control_measurement_accepted() {
                // The reference can advance during a PPS/sensor blind interval,
                // but an old low reading cannot justify adding more heat.
                duty_percent.min(self.duty_percent)
            } else {
                duty_percent
            };

        self.duty_percent = duty_percent;
        let warmup_soft_start_percent = if self.phase == HeaterControlPhase::Warmup {
            self.warmup_started_at_ms
                .map(|started_at_ms| {
                    let elapsed_ms = now_ms.saturating_sub(started_at_ms);
                    (elapsed_ms.saturating_mul(100) / HEATER_WARMUP_SOFT_START_MS).min(100) as u8
                })
                .unwrap_or(100)
        } else {
            100
        };

        HeaterPidSnapshot {
            duty_percent,
            warmup_soft_start_percent,
            error_c,
            control_error_c,
            filtered_temp_c,
            filtered_slope_c_per_s: filtered_slope_c_per_profile_tick,
            coast_active: self.hold_coast_active,
            phase: self.phase,
        }
    }

    fn reset_disabled_output(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
    ) -> HeaterPidSnapshot {
        self.clear_approach_braking();
        self.applied_heat = flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        self.reseed_measurement(measured_temp_c);
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.phase_tick_remainder_ms = 0;
        self.profile_last_control_at_ms = None;
        self.profile_measurement_elapsed_ms = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        HeaterPidSnapshot {
            duty_percent: 0,
            warmup_soft_start_percent: 0,
            error_c: f32::from(target_temp_c) - measured_temp_c,
            control_error_c: f32::from(target_temp_c) - measured_temp_c,
            filtered_temp_c: measured_temp_c,
            filtered_slope_c_per_s: 0.0,
            coast_active: false,
            phase: self.phase,
        }
    }

    fn update_coast_state(
        &mut self,
        frame: &ThermalControlFrame,
        measured_temp_c: f32,
        previous_measured_temp_c: f32,
    ) {
        // Compare raw temperatures across a real time window. A fixed drop
        // per control frame never triggers at a fast cadence during slow cooling.
        if !self.hold_coast_active {
            self.hold_coast_cooling_samples = 0;
            self.coast_cooling_reference_c = None;
            self.coast_cooling_elapsed_ms = 0;
            return;
        }
        if !self.control_measurement_accepted() {
            return;
        }
        if measured_temp_c > previous_measured_temp_c + 0.05 {
            self.coast_cooling_reference_c = None;
            self.coast_cooling_elapsed_ms = 0;
        }
        let reference = *self
            .coast_cooling_reference_c
            .get_or_insert(previous_measured_temp_c);
        self.coast_cooling_elapsed_ms = self
            .coast_cooling_elapsed_ms
            .saturating_add(frame.elapsed_ms);
        let raw_cooling =
            measured_temp_c + 0.05 < reference && measured_temp_c <= previous_measured_temp_c;
        let model_predicts_low = frame.natural_cooling_rate_c_per_s.is_some_and(|rate| {
            frame.error_c - rate * frame.natural_cooling_delay_s
                >= frame.control_target.hold_on_error_c.max(0.05)
        });
        let cooling = raw_cooling
            && self.coast_cooling_elapsed_ms >= 100
            && ((frame.error_c >= frame.control_target.hold_on_error_c.max(0.05)
                && frame.control_error_c >= frame.control_target.hold_on_error_c.max(0.05))
                || model_predicts_low);
        if self.hold_coast_active && cooling {
            self.hold_coast_cooling_samples = self.hold_coast_cooling_samples.saturating_add(1);
        } else {
            self.hold_coast_cooling_samples = 0;
        }
        let plate_cooling = frame.filtered_slope_c_per_profile_tick <= -0.02
            && self.hold_coast_cooling_samples >= 2;
        if self.hold_coast_active && plate_cooling {
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
            self.phase_ticks = 0;
            self.phase_tick_remainder_ms = 0;
            // Replace predicted passive loss immediately, then blend toward
            // the same profile PI. No extra one-second ramp from zero on recovery.
            self.hold_entry_output_percent = frame.natural_cooling_feedforward_percent;
            self.coast_cooling_reference_c = None;
            self.coast_cooling_elapsed_ms = 0;
        }
    }

    fn calculate_duty_percent(
        &mut self,
        frame: &ThermalControlFrame,
        entered_approach: bool,
    ) -> u8 {
        if self.hold_coast_active
            || frame.hold_guard_error_c <= -frame.control_target.overshoot_cutoff_c
        {
            self.hold_integral_c = 0.0;
            return 0;
        }
        match self.phase {
            HeaterControlPhase::Warmup => {
                percent_from_permille(frame.control_target.warmup_power_permille)
            }
            HeaterControlPhase::Approach => self.calculate_approach_duty(frame, entered_approach),
            HeaterControlPhase::Hold => self.calculate_hold_duty(frame),
        }
    }

    fn calculate_approach_duty(
        &mut self,
        frame: &ThermalControlFrame,
        entered_approach: bool,
    ) -> u8 {
        if let Some(duty_percent) = self.calculate_trajectory_approach_duty(frame, entered_approach)
        {
            return duty_percent;
        }
        let target = frame.control_target;
        let span = (frame.brake_distance_c - target.hold_entry_error_c).max(0.1);
        let ratio =
            ((frame.approach_guard_error_c - target.hold_entry_error_c) / span).clamp(0.0, 1.0);
        let shaped_ratio = ratio.powf(target.approach_damping_exponent);
        let sustain_floor =
            f32::from(approach_sustain_floor_permille(target, frame.error_c).max(100));
        let approach_ceiling =
            f32::from(target.approach_power_permille.min(1_000)).max(sustain_floor);
        percent_from_permille(
            (sustain_floor + ((approach_ceiling - sustain_floor) * shaped_ratio))
                .clamp(0.0, 1_000.0) as u16,
        )
    }

    fn clear_approach_braking(&mut self) {
        self.approach_trajectory = None;
        self.approach_last_request_permille = None;
    }

    fn model_approach_available(&self) -> bool {
        self.natural_cooling_input.is_some_and(|input| {
            input.max_power_mw.is_finite()
                && input.max_power_mw > 0.0
                && flux_purr_firmware::thermal_plant::natural_cooling_rate_c_per_s(
                    input.model,
                    input.measured_temp_c,
                    input.ambient_temp_c,
                )
                .is_some()
        })
    }

    fn control_measurement_accepted(&self) -> bool {
        self.natural_cooling_input
            .is_none_or(|input| input.measurement_accepted)
    }

    fn observe_temperature_transfer(&mut self, now_ms: u64, measured_temp_c: f32) {
        if !self.control_measurement_accepted() || !self.model_approach_available() {
            return;
        }
        if self.pps_measurement_anchor_pending {
            self.applied_heat.reanchor_temperature();
        }
        if let Some(input) = self.natural_cooling_input {
            self.applied_heat.observe_temperature(
                now_ms,
                measured_temp_c,
                flux_purr_firmware::thermal_trajectory::TrajectoryPlant {
                    model: input.model,
                    ambient_c: input.ambient_temp_c,
                    max_power_mw: input.max_power_mw,
                },
            );
        }
    }

    fn calculate_trajectory_approach_duty(
        &mut self,
        frame: &ThermalControlFrame,
        entered_approach: bool,
    ) -> Option<u8> {
        use flux_purr_firmware::thermal_trajectory::{TemperatureTrajectory, TrajectoryPlant};
        let input = self.natural_cooling_input?;
        if !self.model_approach_available() {
            return None;
        }
        let plant = TrajectoryPlant {
            model: input.model,
            ambient_c: input.ambient_temp_c,
            max_power_mw: input.max_power_mw,
        };
        let predicted = self.applied_heat.forecast_from_measurement(
            input.now_ms,
            self.last_accepted_measurement_at_ms.unwrap_or(input.now_ms),
            input.measured_temp_c,
            frame.filtered_slope_c_per_profile_tick,
            plant,
        );
        if entered_approach || self.approach_trajectory.is_none() {
            self.approach_trajectory = Some(TemperatureTrajectory::new(
                input.now_ms,
                predicted.temperature_c,
            ));
            // A Hold recovery has no new full-power Warmup to brake. Keep
            // its current logical request as the upward-slew origin; the
            // model may still reduce it immediately when heat is in flight.
            self.approach_last_request_permille = Some(if self.recovering_from_hold {
                u16::from(self.duty_percent) * 10
            } else {
                flux_purr_firmware::thermal_plant::APPROACH_MIN_PERMILLE
            });
        }
        if entered_approach {
            return Some(percent_from_permille(
                flux_purr_firmware::thermal_plant::APPROACH_MIN_PERMILLE,
            ));
        }
        let horizon_s = frame
            .natural_cooling_delay_s
            .max(f32::from(frame.control_target.approach_lead_ticks))
            .max(0.4);
        let reference = self.approach_trajectory.as_mut()?.advance(
            input.now_ms,
            f32::from(input.target_temp_c),
            horizon_s,
            frame.control_target.approach_damping_exponent,
            plant,
        );
        let requested_permille =
            TemperatureTrajectory::request_permille(reference, predicted, horizon_s, plant);
        let previous_permille = self
            .approach_last_request_permille
            .unwrap_or(flux_purr_firmware::thermal_plant::APPROACH_MIN_PERMILLE);
        let applied_permille = flux_purr_firmware::thermal_trajectory::slew_up_permille(
            previous_permille,
            requested_permille,
            frame.elapsed_ms,
            input.model.transport_delay_ms,
        );
        let applied_permille = if self.control_measurement_accepted() {
            applied_permille
        } else {
            applied_permille.min(previous_permille)
        };
        self.approach_last_request_permille = Some(applied_permille);
        Some(percent_from_permille(applied_permille))
    }

    fn calculate_hold_duty(&mut self, frame: &ThermalControlFrame) -> u8 {
        if !self.model_approach_available()
            && (frame.hold_prediction_blocks_reheat
                || frame.hold_filter_lag_blocks_reheat
                || frame.hold_actual_overshoot_blocks_reheat)
        {
            self.hold_integral_c = 0.0;
            return 0;
        }
        let target = frame.control_target;
        let base_permille = hold_effective_base_permille(
            frame.hold_guard_error_c,
            frame.hold_reenter_error_c,
            target,
        );
        let integral_limit = if frame.hold_ki > 0.0 {
            ((1_000.0 - base_permille) / frame.hold_ki).clamp(0.0, 255.0)
        } else {
            0.0
        };
        self.hold_integral_c = if frame.hold_ki > 0.0 && self.control_measurement_accepted() {
            let elapsed_cycles = frame.elapsed_ms as f32 / HEATER_CONTROL_INTERVAL_MS as f32;
            (self.hold_integral_c + frame.hold_guard_error_c * elapsed_cycles)
                .clamp(0.0, integral_limit)
        } else if frame.hold_ki <= 0.0 {
            0.0
        } else {
            self.hold_integral_c
        };
        let requested_permille = self.hold_requested_permille(frame, base_permille);
        let pi_percent = percent_from_permille(requested_permille.clamp(0.0, 1_000.0) as u16);
        if (frame.hold_guard_error_c > 0.0 || frame.natural_cooling_rate_c_per_s.is_some())
            && self.phase_ticks < frame.hold_blend_cycles
        {
            let blend_ratio = f32::from(self.phase_ticks.saturating_add(1))
                / f32::from(frame.hold_blend_cycles.max(1));
            ((f32::from(self.hold_entry_output_percent)
                + ((f32::from(pi_percent) - f32::from(self.hold_entry_output_percent))
                    * blend_ratio))
                .clamp(0.0, 100.0)
                + 0.5) as u8
        } else {
            pi_percent
        }
    }

    fn limit_hold_arrival_power(&mut self, frame: &ThermalControlFrame, duty_percent: u8) -> u8 {
        let Some(input) = self
            .natural_cooling_input
            .filter(|_| self.model_approach_available())
        else {
            return duty_percent;
        };
        let plant = flux_purr_firmware::thermal_trajectory::TrajectoryPlant {
            model: input.model,
            ambient_c: input.ambient_temp_c,
            max_power_mw: input.max_power_mw,
        };
        let predicted = self.applied_heat.forecast_from_measurement(
            input.now_ms,
            self.last_accepted_measurement_at_ms.unwrap_or(input.now_ms),
            input.measured_temp_c,
            frame.filtered_slope_c_per_profile_tick,
            plant,
        );
        let horizon_s = frame
            .natural_cooling_delay_s
            .max(f32::from(frame.control_target.hold_lead_ticks))
            .max(0.4);
        let arrival_power_mw = (plant.loss_mw(predicted.temperature_c)
            + input.model.thermal_capacity_mj_per_c
                * (f32::from(input.target_temp_c) - predicted.temperature_c)
                / horizon_s)
            .clamp(0.0, input.max_power_mw);
        // Apply after the entry blend so it cannot restore delayed heat into
        // an already high future temperature. Floor conversion never rounds
        // the bounded physical request above the calculated energy limit.
        let limit_percent = (arrival_power_mw / input.max_power_mw * 100.0) as u8;
        if duty_percent > limit_percent {
            self.hold_integral_c = 0.0;
        }
        duty_percent.min(limit_percent)
    }

    fn hold_requested_permille(&mut self, frame: &ThermalControlFrame, base_permille: f32) -> f32 {
        let target = frame.control_target;
        let mut requested = base_permille
            + (target.hold_kp_permille_per_c * frame.hold_guard_error_c)
            + (frame.hold_ki * self.hold_integral_c);
        if frame.hold_guard_error_c <= -target.hold_off_error_c {
            self.hold_integral_c = 0.0;
            let taper_span = (target.overshoot_cutoff_c - target.hold_off_error_c).max(0.05);
            let overshoot_c = (-frame.hold_guard_error_c).max(target.hold_off_error_c);
            let taper_ratio =
                ((target.overshoot_cutoff_c - overshoot_c) / taper_span).clamp(0.0, 1.0);
            requested = requested.clamp(0.0, 1_000.0) * taper_ratio;
        }
        requested
    }

    fn advance_phase(&mut self, frame: &ThermalControlFrame, now_ms: u64) {
        let mut next_phase = self.phase;
        match self.phase {
            HeaterControlPhase::Warmup => {
                if warmup_handoff_ready(
                    frame.error_c,
                    frame.previous_error_c,
                    frame.control_error_c,
                    frame.brake_distance_c,
                    frame.warmup_handoff_error_c,
                ) {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
            HeaterControlPhase::Approach => {
                let timeout_hold_ready =
                    self.phase_ticks >= frame.approach_max_cycles && frame.hold_state_ready;
                let reenter_warmup = !self.model_approach_available()
                    && frame.error_c
                        >= frame.brake_distance_c + frame.control_target.warmup_reenter_error_c;
                let approach_hold_ready = frame.approach_control_error_c <= HOLD_ENTRY_BELOW_C
                    && frame.previous_error_c <= HOLD_ENTRY_BELOW_C + 0.5;
                if reenter_warmup {
                    next_phase = HeaterControlPhase::Warmup;
                } else if hold_entry_ready(frame.error_c)
                    && (approach_hold_ready
                        || frame.actual_crossed_target_ready
                        || timeout_hold_ready)
                {
                    next_phase = HeaterControlPhase::Hold;
                }
            }
            HeaterControlPhase::Hold => {
                if frame.hold_exit_error_c >= frame.hold_reenter_error_c
                    && frame.previous_error_c >= frame.hold_reenter_error_c
                {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
        }

        if next_phase != self.phase {
            let previous_phase = self.phase;
            self.phase = next_phase;
            self.phase_ticks = 0;
            self.phase_tick_remainder_ms = 0;
            self.recovering_from_hold = previous_phase == HeaterControlPhase::Hold
                && self.phase == HeaterControlPhase::Approach;
            if self.phase == HeaterControlPhase::Warmup {
                self.warmup_started_at_ms = Some(now_ms);
            }
        } else {
            let phase_elapsed_ms = self
                .phase_tick_remainder_ms
                .saturating_add(frame.elapsed_ms);
            let elapsed_cycles = phase_elapsed_ms / HEATER_CONTROL_INTERVAL_MS;
            self.phase_tick_remainder_ms = phase_elapsed_ms % HEATER_CONTROL_INTERVAL_MS;
            self.phase_ticks = self
                .phase_ticks
                .saturating_add(elapsed_cycles.min(u64::from(u16::MAX)) as u16);
            if self.phase != HeaterControlPhase::Approach {
                self.recovering_from_hold = false;
            }
        }
    }

    fn update_hold_transition(&mut self, frame: &ThermalControlFrame, measured_temp_c: f32) {
        if self.phase != HeaterControlPhase::Hold {
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
            self.hold_entry_output_percent = 0;
            self.hold_integral_c = 0.0;
            return;
        }

        let target = frame.control_target;
        let coast_guard_c = target
            .hold_exit_error_c
            .max(target.hold_on_error_c.max(0.05) * 2.0);
        let zero_output_ready =
            self.duty_percent == 0 && frame.error_c <= target.hold_on_error_c.max(0.05) * 2.0;
        let projection_ready = self.duty_percent > 0
            && frame.error_c <= coast_guard_c
            && (frame.approach_control_error_c <= 0.0 || frame.hold_control_error_c <= 0.0);
        self.hold_coast_active = frame.filtered_slope_c_per_profile_tick > 0.0
            && (frame.actual_crossed_target_ready
                || frame.error_c <= 0.0
                || frame.control_error_c <= 0.0
                || zero_output_ready
                || projection_ready);
        self.hold_coast_cooling_samples = 0;
        self.coast_cooling_reference_c = None;
        self.coast_cooling_elapsed_ms = 0;
        if frame.actual_crossed_target_ready {
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
        }
        let guard_error_c = if frame.error_c > 0.0 {
            frame.hold_guard_error_c
        } else {
            frame.error_c
        };
        let base_permille =
            hold_effective_base_permille(guard_error_c, frame.hold_reenter_error_c, target);
        let integral_limit = if frame.hold_ki > 0.0 {
            ((1_000.0 - base_permille) / frame.hold_ki).clamp(0.0, 255.0)
        } else {
            0.0
        };
        let previous_output_permille = f32::from(self.duty_percent.min(100)) * 10.0;
        let base_output = base_permille + (target.hold_kp_permille_per_c * guard_error_c);
        let actual_ratio = if frame.error_c > 0.0 {
            (frame.error_c / frame.hold_entry_gate_c).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let projected_ratio = if frame.approach_guard_error_c > 0.0 {
            (frame.approach_guard_error_c / frame.hold_entry_gate_c).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let carry_permille = (previous_output_permille - base_output).max(0.0);
        let preload = carry_permille * actual_ratio.min(projected_ratio);
        self.hold_entry_output_percent =
            percent_from_permille((base_output + preload).clamp(0.0, 1_000.0) as u16);
        self.hold_integral_c = if frame.hold_ki > 0.0 {
            (preload / frame.hold_ki).clamp(0.0, integral_limit)
        } else {
            0.0
        };
    }

    fn natural_cooling_guidance(
        &self,
        target_temp_c: i16,
        measured_temp_c: f32,
        target: ThermalControlTarget,
        slope_c_per_s: f32,
    ) -> (Option<f32>, f32, u8, f32) {
        let rate = self.natural_cooling_input.and_then(|input| {
            flux_purr_firmware::thermal_plant::natural_cooling_rate_c_per_s(
                input.model,
                measured_temp_c,
                input.ambient_temp_c,
            )
        });
        let input = self.natural_cooling_input.filter(|_| rate.is_some());
        let delay_s = input.map_or(0.0, |input| input.model.transport_delay_ms as f32 / 1000.0);
        let feedforward = input
            .filter(|input| input.max_power_mw.is_finite() && input.max_power_mw > 0.0)
            .map_or(0, |input| {
                let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
                    input.model,
                    f32::from(target_temp_c),
                    input.ambient_temp_c,
                );
                percent_from_permille(
                    (loss / input.max_power_mw * 1000.0)
                        .clamp(0.0, f32::from(target.hold_power_permille))
                        as u16,
                )
            });
        // Net rise already includes loss. Infer delayed heat, then subtract the
        // passive loss expected over that delay instead of counting it twice.
        let future_rate = input.and_then(|input| {
            flux_purr_firmware::thermal_plant::natural_cooling_rate_c_per_s(
                input.model,
                measured_temp_c + slope_c_per_s.max(0.0) * delay_s * 0.5,
                input.ambient_temp_c,
            )
        });
        let delayed_heat_rate = (slope_c_per_s - rate.unwrap_or(0.0)).max(0.0);
        let model_brake =
            (delayed_heat_rate + future_rate.or(rate).unwrap_or(0.0)).max(0.0) * delay_s;
        // Keep live environmental prediction, but apply the candidate's braking
        // correction relative to a calibration-only full-power reference. An
        // uncorrected model maximum would mask every lower-distance proposal.
        let corrected_model_brake = input
            .filter(|input| input.max_power_mw.is_finite() && input.max_power_mw > 0.0)
            .map_or(model_brake, |input| {
                let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
                    input.model,
                    f32::from(target_temp_c),
                    input.ambient_temp_c,
                );
                let full_power_rise =
                    ((input.max_power_mw - loss) / input.model.thermal_capacity_mj_per_c).max(0.0);
                let reference_brake = ((full_power_rise * delay_s + HOLD_ENTRY_BELOW_C) * 100.0)
                    .round()
                    .clamp(100.0, 5_000.0)
                    / 100.0;
                model_brake + target.brake_distance_c - reference_brake
            });
        let brake = target.brake_distance_c.max(HOLD_ENTRY_BELOW_C + 0.1).max(
            corrected_model_brake.min(target.brake_distance_c + target.warmup_reenter_error_c),
        );
        (rate, delay_s, feedforward, brake)
    }

    fn hold_guard_error(error_c: f32, predicted_error_c: f32, target: ThermalControlTarget) -> f32 {
        if error_c < 0.0 {
            return error_c;
        }
        let lag_allowance = target
            .hold_on_error_c
            .max(0.05)
            .min(target.hold_entry_error_c.max(0.05));
        predicted_error_c.max(0.0).min(error_c + lag_allowance)
    }

    fn filter_profile_measurement(
        &mut self,
        measured_temp_c: f32,
        filter_alpha: f32,
        nominal_alpha: f32,
        elapsed_ms: u64,
    ) -> (f32, f32) {
        let previous = self.filtered_temp_c;
        let filtered = previous
            .map(|last| last + filter_alpha * (measured_temp_c - last))
            .unwrap_or(measured_temp_c);
        let instantaneous_slope = previous
            .map(|last| (filtered - last) * (HEATER_PROFILE_TICK_MS as f32 / elapsed_ms as f32))
            .unwrap_or(0.0);
        let nominal_slope_alpha = scaled_filter_alpha_for_control_interval(nominal_alpha).sqrt();
        let slope_alpha = scaled_filter_alpha_for_elapsed(
            nominal_slope_alpha,
            elapsed_ms,
            HEATER_CONTROL_INTERVAL_MS,
        );
        self.filtered_slope_c_per_profile_tick +=
            slope_alpha * (instantaneous_slope - self.filtered_slope_c_per_profile_tick);
        self.previous_filtered_temp_c = previous;
        self.filtered_temp_c = Some(filtered);
        (filtered, self.filtered_slope_c_per_profile_tick)
    }

    fn measure_profile_temperature(
        &mut self,
        measured_temp_c: f32,
        nominal_alpha: f32,
        elapsed_ms: u64,
    ) -> (f32, f32) {
        // Outputs keep their control cadence while RTD acceptance can pause.
        // Reusing the held value would invent zero velocity during that pause.
        self.profile_measurement_elapsed_ms = self
            .profile_measurement_elapsed_ms
            .saturating_add(elapsed_ms);
        if !self.control_measurement_accepted() {
            return (
                self.filtered_temp_c.unwrap_or(measured_temp_c),
                self.filtered_slope_c_per_profile_tick,
            );
        }
        if self.pps_measurement_anchor_pending {
            // Neither the ADC origin shift nor the blind settling interval is
            // a thermal derivative. Keep trusted positive Approach velocity;
            // the next accepted interval updates it with ordinary filtering.
            self.pps_measurement_anchor_pending = false;
            self.profile_measurement_elapsed_ms = 0;
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
            return (measured_temp_c, self.filtered_slope_c_per_profile_tick);
        }
        let measurement_elapsed_ms = self.profile_measurement_elapsed_ms;
        self.profile_measurement_elapsed_ms = 0;
        self.filter_profile_measurement(
            measured_temp_c,
            scaled_filter_alpha_for_elapsed(
                nominal_alpha,
                measurement_elapsed_ms,
                HEATER_PROFILE_TICK_MS,
            ),
            nominal_alpha,
            measurement_elapsed_ms,
        )
    }

    fn measure_control_frame(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        previous_measured_temp_c: f32,
        control_target: ThermalControlTarget,
        elapsed_ms: u64,
    ) -> ThermalControlFrame {
        let (_, approach_max_cycles, hold_blend_cycles, hold_ki) =
            control_cycle_parameters(control_target, elapsed_ms);
        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let previous_error_c = f32::from(target_temp_c) - previous_measured_temp_c;
        let (filtered_temp_c, filtered_slope_c_per_profile_tick) = self
            .measure_profile_temperature(
                measured_temp_c,
                control_target.settings.temp_filter_alpha,
                elapsed_ms,
            );
        let control_error_c = f32::from(target_temp_c) - filtered_temp_c;
        let (
            natural_cooling_rate_c_per_s,
            natural_cooling_delay_s,
            natural_cooling_feedforward_percent,
            brake_distance_c,
        ) = self.natural_cooling_guidance(
            target_temp_c,
            measured_temp_c,
            control_target,
            filtered_slope_c_per_profile_tick,
        );
        let approach_projected_temp_c = filtered_temp_c
            + filtered_slope_c_per_profile_tick
                * f32::from(control_target.approach_lead_ticks).max(natural_cooling_delay_s);
        let hold_projected_temp_c = filtered_temp_c
            + (filtered_slope_c_per_profile_tick * f32::from(control_target.hold_lead_ticks));
        let approach_control_error_c = f32::from(target_temp_c) - approach_projected_temp_c;
        let hold_control_error_c = f32::from(target_temp_c) - hold_projected_temp_c;
        let hold_prediction_guard_c = control_target.hold_on_error_c.max(0.05) * 2.0;
        let hold_prediction_blocks_reheat = error_c > 0.0
            && error_c <= hold_prediction_guard_c
            && filtered_slope_c_per_profile_tick > 0.0
            && hold_control_error_c <= 0.0;
        let hold_filter_lag_blocks_reheat =
            error_c <= 0.0 && control_error_c > 0.0 && filtered_slope_c_per_profile_tick > 0.0;
        let hold_actual_overshoot_blocks_reheat = error_c <= 0.0
            && control_error_c <= 0.0
            && (control_error_c - error_c) >= 0.05
            && filtered_slope_c_per_profile_tick > 0.0;
        let approach_guard_error_c = approach_control_error_c.min(error_c);
        let hold_entry_gate_c = control_target.hold_entry_error_c.max(0.05);
        let hold_state_ready = control_error_c <= control_target.hold_exit_error_c;
        let actual_crossed_target_ready =
            error_c <= 0.0 && previous_error_c <= 0.0 && approach_control_error_c <= 0.0;
        let hold_guard_error_c =
            Self::hold_guard_error(error_c, hold_control_error_c, control_target);
        let hold_reenter_error_c = control_target
            .hold_exit_error_c
            .max(control_target.hold_on_error_c)
            .max(hold_entry_gate_c + HEATER_HOLD_PHASE_HYSTERESIS_C);
        let approach_predictive_coast_ready = approach_control_error_c <= 0.0
            && error_c <= control_target.hold_exit_error_c
            && control_error_c <= control_target.hold_exit_error_c;
        let hold_exit_error_c = error_c;
        let warmup_handoff_error_c = warmup_handoff_error_c(
            brake_distance_c,
            control_target.warmup_reenter_error_c,
            filtered_slope_c_per_profile_tick,
            control_target.approach_lead_ticks,
        );
        ThermalControlFrame {
            control_target,
            elapsed_ms,
            approach_max_cycles,
            hold_blend_cycles,
            hold_ki,
            error_c,
            previous_error_c,
            filtered_temp_c,
            filtered_slope_c_per_profile_tick,
            control_error_c,
            approach_control_error_c,
            hold_control_error_c,
            hold_prediction_blocks_reheat,
            hold_filter_lag_blocks_reheat,
            hold_actual_overshoot_blocks_reheat,
            approach_guard_error_c,
            hold_entry_gate_c,
            hold_state_ready,
            actual_crossed_target_ready,
            hold_guard_error_c,
            hold_reenter_error_c,
            approach_predictive_coast_ready,
            hold_exit_error_c,
            brake_distance_c,
            warmup_handoff_error_c,
            natural_cooling_rate_c_per_s,
            natural_cooling_delay_s,
            natural_cooling_feedforward_percent,
        }
    }

    fn reset_for_target_change(&mut self, measured_temp_c: f32, now_ms: u64) {
        self.applied_heat = flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        self.clear_approach_braking();
        self.reseed_measurement(measured_temp_c);
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.phase_tick_remainder_ms = 0;
        self.profile_last_control_at_ms = None;
        self.profile_measurement_elapsed_ms = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.warmup_started_at_ms = Some(now_ms);
    }

    fn profile_control_elapsed_ms(&mut self, now_ms: u64) -> u64 {
        let elapsed_ms = self
            .profile_last_control_at_ms
            .and_then(|previous| now_ms.checked_sub(previous))
            .filter(|elapsed| *elapsed > 0)
            .unwrap_or(HEATER_CONTROL_INTERVAL_MS);
        self.profile_last_control_at_ms = Some(now_ms);
        elapsed_ms
    }

    pub(crate) fn apply_under_target_reheat_floor(
        &self,
        duty_percent: u8,
        error_c: f32,
        hold_control_error_c: f32,
        control_target: ThermalControlTarget,
    ) -> u8 {
        if duty_percent != 0 || error_c <= 0.0 {
            return duty_percent;
        }

        let floor_permille = match self.phase {
            HeaterControlPhase::Warmup => return duty_percent,
            HeaterControlPhase::Approach => {
                approach_sustain_floor_permille(control_target, error_c).max(100)
            }
            HeaterControlPhase::Hold => {
                if hold_control_error_c <= 0.0 {
                    return duty_percent;
                }
                control_target
                    .hold_reheat_power_permille
                    .max(control_target.hold_power_permille)
            }
        };
        percent_from_permille(floor_permille.min(1_000))
    }
}
