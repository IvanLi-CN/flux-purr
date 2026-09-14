#[cfg(any(target_arch = "xtensa", test))]
fn active_thermal_control_profile(
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
fn resolve_thermal_profile_bank(
    mode: ThermalProfileMode,
    manual_pps: &ManualPpsState,
) -> ThermalProfileBank {
    if mode == ThermalProfileMode::Auto && manual_pps.has_matching_pps_apdo(20_000, 5_000) {
        ThermalProfileBank::Pps5a
    } else {
        mode.default_bank()
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn thermal_control_runtime_wire(
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
fn default_thermal_control_target(target_temp_c: i16) -> ThermalControlTarget {
    default_thermal_control_target_with_settings(
        target_temp_c,
        ThermalControlProfileSettings::default(),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
#[expect(
    clippy::too_many_lines,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
fn default_thermal_control_target_with_settings(
    target_temp_c: i16,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    let target = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
    let brake_distance_centi_c = if target <= 100 {
        450
    } else if target <= 180 {
        700
    } else if target <= 250 {
        1_000
    } else {
        1_400
    };
    let approach_power_permille = if target <= 100 {
        380
    } else if target <= 180 {
        320
    } else if target <= 250 {
        260
    } else {
        220
    };
    let warmup_power_permille = 1_000;
    let approach_floor_power_permille = if target <= 100 {
        120
    } else if target <= 180 {
        200
    } else if target <= 250 {
        320
    } else {
        380
    };
    let hold_power_permille = if target <= 100 {
        180
    } else if target <= 180 {
        220
    } else if target <= 250 {
        260
    } else {
        300
    };
    let approach_damping_exponent_permille: u16 = if target <= 100 {
        1_400
    } else if target <= 140 {
        1_000
    } else if target <= 180 {
        800
    } else if target <= 220 {
        550
    } else {
        350
    };
    ThermalControlTarget {
        brake_distance_c: brake_distance_centi_c as f32 / 100.0,
        warmup_power_permille,
        warmup_reenter_error_c: settings.warmup_reenter_error_c,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent: f32::from(approach_damping_exponent_permille) / 1_000.0,
        approach_tail_window_c: 0.0,
        hold_power_permille,
        hold_entry_error_c: if target <= 60 {
            0.35
        } else if target <= 100 {
            0.25
        } else if target <= 140 {
            0.20
        } else if target <= 180 {
            0.18
        } else if target <= 220 {
            0.15
        } else {
            0.12
        },
        hold_exit_error_c: if target <= 60 {
            1.6
        } else if target <= 100 {
            1.2
        } else if target <= 140 {
            1.0
        } else if target <= 180 {
            0.9
        } else if target <= 220 {
            0.8
        } else {
            0.7
        },
        hold_off_error_c: if target <= 60 {
            0.7
        } else if target <= 100 {
            0.8
        } else if target <= 140 {
            0.9
        } else if target <= 180 {
            1.0
        } else if target <= 220 {
            1.2
        } else {
            1.5
        },
        overshoot_cutoff_c: if target <= 60 {
            0.8
        } else if target <= 100 {
            0.9
        } else if target <= 140 {
            1.0
        } else if target <= 180 {
            1.2
        } else if target <= 220 {
            1.4
        } else {
            1.6
        },
        hold_kp_permille_per_c: if target <= 60 {
            70.0
        } else if target <= 100 {
            55.0
        } else if target <= 140 {
            42.0
        } else if target <= 180 {
            30.0
        } else if target <= 220 {
            22.0
        } else {
            18.0
        },
        hold_ki_permille_per_c_tick: if target <= 60 {
            3.0
        } else if target <= 100 {
            2.0
        } else if target <= 140 {
            1.5
        } else {
            1.0
        },
        hold_blend_ticks: if target <= 100 {
            16
        } else if target <= 180 {
            12
        } else {
            8
        },
        hold_reheat_power_permille: hold_power_permille,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        hold_on_error_c: settings.hold_on_error_c,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[expect(
    clippy::too_many_lines,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
fn interpolate_thermal_control_target(
    target_temp_c: i16,
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    if lower.target_temp_c >= upper.target_temp_c {
        return ThermalControlTarget {
            brake_distance_c: lower.brake_distance_centi_c as f32 / 100.0,
            warmup_power_permille: 1_000,
            warmup_reenter_error_c: f32::from(lower.warmup_reenter_centi_c) / 100.0,
            approach_power_permille: lower.approach_power_permille,
            approach_floor_power_permille: lower.approach_floor_power_permille,
            approach_damping_exponent: f32::from(lower.approach_damping_exponent_permille)
                / 1_000.0,
            approach_tail_window_c: f32::from(lower.approach_tail_window_centi_c) / 100.0,
            hold_power_permille: lower.hold_power_permille,
            hold_reheat_power_permille: lower.hold_reheat_power_permille,
            hold_entry_error_c: f32::from(lower.hold_entry_centi_c) / 100.0,
            hold_exit_error_c: f32::from(lower.hold_exit_centi_c) / 100.0,
            hold_on_error_c: f32::from(lower.hold_on_centi_c) / 100.0,
            hold_off_error_c: f32::from(lower.hold_off_centi_c) / 100.0,
            overshoot_cutoff_c: f32::from(lower.overshoot_cutoff_centi_c) / 100.0,
            hold_kp_permille_per_c: f32::from(lower.hold_kp_permille_per_c),
            hold_ki_permille_per_c_tick: f32::from(lower.hold_ki_permille_per_c_tick),
            hold_blend_ticks: lower.hold_blend_ticks.clamp(1, u16::from(u8::MAX)) as u8,
            approach_lead_ticks: lower.approach_lead_ticks.min(u16::from(u8::MAX)) as u8,
            hold_lead_ticks: lower.hold_lead_ticks.min(u16::from(u8::MAX)) as u8,
            settings,
        };
    }

    let span = f32::from(upper.target_temp_c - lower.target_temp_c);
    let ratio = (f32::from(target_temp_c - lower.target_temp_c) / span).clamp(0.0, 1.0);
    let lerp_u16 = |left: u16, right: u16, upper_bound: u16| -> u16 {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp_u16(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
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
    let interpolated_brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_hold_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    };
    let low_temp_reheat_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    };
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    ThermalControlTarget {
        brake_distance_c: f32::from(interpolated_brake_distance) / 100.0,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: f32::from(lerp_u16(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            5_000,
        )) / 100.0,
        approach_power_permille: lerp_u16(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp_u16(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent: f32::from(lerp_u16(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        )) / 1_000.0,
        approach_tail_window_c: f32::from(lerp_u16(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
        )) / 100.0,
        hold_power_permille: scale_low_temp_hold(lerp_u16(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp_u16(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        hold_entry_error_c: f32::from(lerp_u16(
            lower.hold_entry_centi_c,
            upper.hold_entry_centi_c,
            5_000,
        )) / 100.0,
        hold_exit_error_c: f32::from(lerp_u16(
            lower.hold_exit_centi_c,
            upper.hold_exit_centi_c,
            5_000,
        )) / 100.0,
        hold_on_error_c: f32::from(lerp_u16(
            lower.hold_on_centi_c,
            upper.hold_on_centi_c,
            5_000,
        )) / 100.0,
        hold_off_error_c: f32::from(lerp_u16(
            lower.hold_off_centi_c,
            upper.hold_off_centi_c,
            5_000,
        )) / 100.0,
        overshoot_cutoff_c: f32::from(lerp_u16(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        )) / 100.0,
        hold_kp_permille_per_c: f32::from(lerp_u16(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        )),
        hold_ki_permille_per_c_tick: f32::from(lerp_u16(
            lower.hold_ki_permille_per_c_tick,
            upper.hold_ki_permille_per_c_tick,
            10_000,
        )),
        hold_blend_ticks: lerp_u16(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)) as u8,
        approach_lead_ticks: lerp_u16(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
        hold_lead_ticks: lerp_u16(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn percent_from_permille(permille: u16) -> u8 {
    ((u32::from(permille.min(1_000)) + 5) / 10).min(100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
fn control_cycles_from_profile_ticks(profile_ticks: u16) -> u16 {
    if profile_ticks == 0 {
        return 0;
    }
    let numerator = u32::from(profile_ticks) * HEATER_PROFILE_TICK_MS as u32;
    let denominator = HEATER_CONTROL_INTERVAL_MS as u32;
    numerator.div_ceil(denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
fn scaled_filter_alpha_for_control_interval(alpha_per_profile_tick: f32) -> f32 {
    let alpha = alpha_per_profile_tick.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return 0.0;
    }
    if alpha >= 1.0 {
        return 1.0;
    }
    let profile_fraction = HEATER_CONTROL_INTERVAL_MS as f32 / HEATER_PROFILE_TICK_MS as f32;
    (1.0 - (1.0 - alpha).powf(profile_fraction)).clamp(0.0, 1.0)
}

#[cfg(any(target_arch = "xtensa", test))]
fn scaled_hold_ki_for_control_interval(ki_per_profile_tick: f32) -> f32 {
    ki_per_profile_tick.max(0.0)
        * (HEATER_CONTROL_INTERVAL_MS as f32 / HEATER_PROFILE_TICK_MS as f32)
}

#[cfg(any(target_arch = "xtensa", test))]
fn next_heater_control_deadline_ms(deadline_ms: u64, control_started_ms: u64) -> u64 {
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
const fn pd_runtime_service_due(now_ms: u64, deadline_ms: u64) -> bool {
    now_ms >= deadline_ms
}

#[cfg(any(target_arch = "xtensa", test))]
fn next_pd_runtime_service_deadline_ms(deadline_ms: u64, service_started_ms: u64) -> u64 {
    let next_deadline_ms = deadline_ms.saturating_add(PD_RUNTIME_SERVICE_INTERVAL_MS);
    if next_deadline_ms > service_started_ms {
        return next_deadline_ms;
    }

    let missed_intervals = service_started_ms
        .saturating_sub(next_deadline_ms)
        .saturating_div(PD_RUNTIME_SERVICE_INTERVAL_MS)
        .saturating_add(1);
    next_deadline_ms.saturating_add(missed_intervals.saturating_mul(PD_RUNTIME_SERVICE_INTERVAL_MS))
}

#[cfg(any(target_arch = "xtensa", test))]
fn fixed_contract_liveness_probe_due(
    contract_kind: ContractKind,
    contract_ready: bool,
    refresh_pending: bool,
    last_probe_at_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    contract_ready
        && contract_kind == ContractKind::Fixed
        && !refresh_pending
        && match last_probe_at_ms {
            Some(last_probe_at_ms) => {
                fusb302b::source_capabilities_retry_due(last_probe_at_ms, now_ms)
            }
            None => false,
        }
}

#[cfg(any(target_arch = "xtensa", test))]
fn warmup_handoff_error_c(
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
fn warmup_handoff_ready(
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
fn hold_effective_base_permille(
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
struct BuzzerHardwareState {
    frequency_hz: Option<u32>,
    duty_percent: u8,
    generation: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_timer_reconfiguration_needed(
    configured_frequency_hz: u32,
    next_state: BuzzerHardwareState,
) -> bool {
    next_state
        .frequency_hz
        .is_some_and(|frequency_hz| frequency_hz != configured_frequency_hz)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuzzerHardwareAction {
    StopTimer,
    Retune(u32),
    SetDutyPercent(u8),
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_hardware_actions(
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
fn mcpwm_timer_frequency_hz(prescaler: u8, period_ticks: u16) -> u32 {
    MCPWM_PERIPHERAL_CLOCK_HZ / (u32::from(prescaler) + 1) / (u32::from(period_ticks) + 1)
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_timer_period_ticks(frequency_hz: u32) -> Option<u16> {
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
fn buzzer_observed_frequency_hz(rising_edges: u16, window_ms: u32) -> Option<u32> {
    if window_ms == 0 {
        return None;
    }
    Some(u32::from(rising_edges).saturating_mul(1_000) / window_ms)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterController {
    fault_latched: Option<HeaterFaultReason>,
    last_target_temp_c: i16,
    filtered_temp_c: Option<f32>,
    previous_filtered_temp_c: Option<f32>,
    filtered_slope_c_per_profile_tick: f32,
    previous_measured_temp_c: Option<f32>,
    phase: HeaterControlPhase,
    phase_ticks: u16,
    recovering_from_hold: bool,
    duty_percent: u8,
    hold_entry_output_percent: u8,
    hold_integral_c: f32,
    hold_coast_active: bool,
    hold_coast_cooling_samples: u8,
    heater_was_enabled: bool,
    warmup_started_at_ms: Option<u64>,
    thermal_plant_controller: ThermalPlantController,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalPlantRuntimeInput {
    target_temp_c: i16,
    measured_temp_c: f32,
    ambient_temp_c: f32,
    heater_enabled: bool,
    model: flux_purr_firmware::memory::ThermalPlantProjection,
    max_power_mw: f32,
    now_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterController {
    const fn new() -> Self {
        Self {
            fault_latched: None,
            last_target_temp_c: 0,
            filtered_temp_c: None,
            previous_filtered_temp_c: None,
            filtered_slope_c_per_profile_tick: 0.0,
            previous_measured_temp_c: None,
            phase: HeaterControlPhase::Warmup,
            phase_ticks: 0,
            recovering_from_hold: false,
            duty_percent: 0,
            hold_entry_output_percent: 0,
            hold_integral_c: 0.0,
            hold_coast_active: false,
            hold_coast_cooling_samples: 0,
            heater_was_enabled: false,
            warmup_started_at_ms: None,
            thermal_plant_controller: ThermalPlantController::new(),
        }
    }

    const fn fault_latched(self) -> Option<HeaterFaultReason> {
        self.fault_latched
    }

    fn clear_fault_latch(&mut self) {
        self.fault_latched = None;
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        self.thermal_plant_controller.reset();
    }

    fn reseed_measurement(&mut self, measured_temp_c: f32) {
        self.filtered_temp_c = Some(measured_temp_c);
        self.previous_filtered_temp_c = Some(measured_temp_c);
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = Some(measured_temp_c);
    }

    fn latch_fault(&mut self, reason: HeaterFaultReason) -> bool {
        let changed = self.fault_latched != Some(reason);
        self.fault_latched = Some(reason);
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        self.thermal_plant_controller.reset();
        changed
    }

    #[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[expect(
    clippy::too_many_lines,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
    fn update_thermal_plant_at(&mut self, input: ThermalPlantRuntimeInput) -> HeaterPidSnapshot {
        let ThermalPlantRuntimeInput {
            target_temp_c,
            measured_temp_c,
            ambient_temp_c,
            heater_enabled,
            model,
            max_power_mw,
            now_ms,
        } = input;
        if !heater_enabled || self.fault_latched.is_some() {
            self.thermal_plant_controller.reset();
            self.reseed_measurement(measured_temp_c);
            self.duty_percent = 0;
            self.heater_was_enabled = false;
            return HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                control_error_c: f32::from(target_temp_c) - measured_temp_c,
                filtered_temp_c: measured_temp_c,
                filtered_slope_c_per_s: 0.0,
                coast_active: false,
                phase: HeaterControlPhase::Warmup,
            };
        }
        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
            return self.update_thermal_plant_at(ThermalPlantRuntimeInput {
                target_temp_c,
                measured_temp_c,
                ambient_temp_c,
                heater_enabled: false,
                model,
                max_power_mw,
                now_ms,
            });
        }
        if !self.heater_was_enabled || self.last_target_temp_c != target_temp_c {
            self.thermal_plant_controller.reset();
            self.reseed_measurement(measured_temp_c);
            self.warmup_started_at_ms = Some(now_ms);
        }
        self.heater_was_enabled = true;
        self.last_target_temp_c = target_temp_c;
        let previous = self.filtered_temp_c.unwrap_or(measured_temp_c);
        let filtered_temp_c = previous + 0.75 * (measured_temp_c - previous);
        let raw_slope_c_per_s =
            (filtered_temp_c - previous) * (1_000.0 / HEATER_CONTROL_INTERVAL_MS as f32);
        let slope_c_per_s = self.filtered_slope_c_per_profile_tick
            + THERMAL_PLANT_SLOPE_FILTER_ALPHA
                * (raw_slope_c_per_s - self.filtered_slope_c_per_profile_tick);
        self.filtered_slope_c_per_profile_tick = slope_c_per_s;
        self.previous_filtered_temp_c = self.filtered_temp_c;
        self.filtered_temp_c = Some(filtered_temp_c);
        let output = self
            .thermal_plant_controller
            .update(ThermalPlantControlInput {
                model,
                target_temp_c: f32::from(target_temp_c),
                current_temp_c: filtered_temp_c,
                ambient_temp_c,
                slope_c_per_s,
                dt_s: HEATER_CONTROL_INTERVAL_MS as f32 / 1_000.0,
                max_power_mw,
            });
        let duty_percent = if max_power_mw <= 0.0 {
            0
        } else {
            round_to_u16_nonnegative(output.requested_power_mw * 100.0 / max_power_mw).min(100)
                as u8
        };
        self.duty_percent = duty_percent;
        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let phase = if error_c.abs() <= 1.5 {
            HeaterControlPhase::Hold
        } else if error_c <= 3.0 {
            HeaterControlPhase::Approach
        } else {
            HeaterControlPhase::Warmup
        };
        self.phase = phase;
        HeaterPidSnapshot {
            duty_percent,
            warmup_soft_start_percent: if phase == HeaterControlPhase::Warmup {
                self.warmup_started_at_ms
                    .map(|started_at_ms| {
                        (now_ms.saturating_sub(started_at_ms).saturating_mul(100)
                            / HEATER_WARMUP_SOFT_START_MS)
                            .min(100) as u8
                    })
                    .unwrap_or(100)
            } else {
                100
            },
            error_c,
            control_error_c: output.predicted_error_c,
            filtered_temp_c,
            filtered_slope_c_per_s: slope_c_per_s,
            coast_active: duty_percent == 0 && slope_c_per_s > 0.0,
            phase,
        }
    }

    #[cfg(test)]
    fn update(
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

#[expect(
    clippy::too_many_lines,
    clippy::excessive_nesting,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
    fn update_at(
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
        let previous_measured_temp_c = self
            .previous_measured_temp_c
            .or(self.filtered_temp_c)
            .unwrap_or(measured_temp_c);
        self.previous_measured_temp_c = Some(measured_temp_c);

        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
        }

        if !heater_enabled || self.fault_latched.is_some() {
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
            self.filtered_slope_c_per_profile_tick = 0.0;
            self.previous_measured_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.recovering_from_hold = false;
            self.duty_percent = 0;
            self.hold_entry_output_percent = 0;
            self.hold_integral_c = 0.0;
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
            self.heater_was_enabled = false;
            self.warmup_started_at_ms = None;
            return HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                control_error_c: f32::from(target_temp_c) - measured_temp_c,
                filtered_temp_c: measured_temp_c,
                filtered_slope_c_per_s: 0.0,
                coast_active: false,
                phase: self.phase,
            };
        }

        if target_temp_c != last_target_temp_c {
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
            self.filtered_slope_c_per_profile_tick = 0.0;
            self.previous_measured_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.recovering_from_hold = false;
            self.duty_percent = 0;
            self.hold_entry_output_percent = 0;
            self.hold_integral_c = 0.0;
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
            self.warmup_started_at_ms = Some(now_ms);
        }

        if !self.heater_was_enabled {
            self.warmup_started_at_ms = Some(now_ms);
        }
        self.heater_was_enabled = true;

        let control_target = thermal_profile
            .map(|profile| profile.control_target(target_temp_c))
            .unwrap_or_else(|| default_thermal_control_target(target_temp_c));
        let settings = control_target.settings;
        let filter_alpha = scaled_filter_alpha_for_control_interval(settings.temp_filter_alpha);
        let approach_max_cycles =
            control_cycles_from_profile_ticks(u16::from(settings.approach_max_ticks)).max(1);
        let hold_blend_cycles =
            control_cycles_from_profile_ticks(u16::from(control_target.hold_blend_ticks)).max(1);
        let hold_ki =
            scaled_hold_ki_for_control_interval(control_target.hold_ki_permille_per_c_tick);
        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let previous_error_c = f32::from(target_temp_c) - previous_measured_temp_c;
        let last_filtered_temp_c = self.filtered_temp_c;
        let filtered_temp_c = if let Some(previous_filtered_temp_c) = last_filtered_temp_c {
            previous_filtered_temp_c + filter_alpha * (measured_temp_c - previous_filtered_temp_c)
        } else {
            measured_temp_c
        };
        let instantaneous_slope_c_per_profile_tick = last_filtered_temp_c
            .map(|last| {
                (filtered_temp_c - last)
                    * (HEATER_PROFILE_TICK_MS as f32 / HEATER_CONTROL_INTERVAL_MS as f32)
            })
            .unwrap_or(0.0);
        let slope_filter_alpha = filter_alpha.sqrt();
        self.filtered_slope_c_per_profile_tick += slope_filter_alpha
            * (instantaneous_slope_c_per_profile_tick - self.filtered_slope_c_per_profile_tick);
        let filtered_temp_slope_c_per_profile_tick = self.filtered_slope_c_per_profile_tick;
        self.previous_filtered_temp_c = last_filtered_temp_c;
        self.filtered_temp_c = Some(filtered_temp_c);
        let control_error_c = f32::from(target_temp_c) - filtered_temp_c;
        let approach_projected_temp_c = filtered_temp_c
            + (filtered_temp_slope_c_per_profile_tick
                * f32::from(control_target.approach_lead_ticks));
        let hold_projected_temp_c = filtered_temp_c
            + (filtered_temp_slope_c_per_profile_tick * f32::from(control_target.hold_lead_ticks));
        let approach_control_error_c = f32::from(target_temp_c) - approach_projected_temp_c;
        let hold_control_error_c = f32::from(target_temp_c) - hold_projected_temp_c;
        let hold_prediction_guard_c = control_target.hold_on_error_c.max(0.05) * 2.0;
        let hold_prediction_blocks_reheat = error_c > 0.0
            && error_c <= hold_prediction_guard_c
            && filtered_temp_slope_c_per_profile_tick > 0.0
            && hold_control_error_c <= 0.0;
        let hold_filter_lag_blocks_reheat =
            error_c <= 0.0 && control_error_c > 0.0 && filtered_temp_slope_c_per_profile_tick > 0.0;
        let hold_actual_overshoot_blocks_reheat = error_c <= 0.0
            && control_error_c <= 0.0
            && (control_error_c - error_c) >= 0.05
            && filtered_temp_slope_c_per_profile_tick > 0.0;
        let approach_guard_error_c = approach_control_error_c.min(error_c);
        let hold_entry_gate_c = control_target.hold_entry_error_c.max(0.05);
        let hold_entry_measurement_margin_c = 0.5;
        let hold_state_ready = control_error_c <= control_target.hold_exit_error_c;
        let actual_crossed_target_ready =
            error_c <= 0.0 && previous_error_c <= 0.0 && approach_control_error_c <= 0.0;
        let hold_guard_error_c = if error_c >= 0.0 {
            let filter_lag_allowance_c = control_target
                .hold_on_error_c
                .max(0.05)
                .min(hold_entry_gate_c);
            hold_control_error_c
                .max(0.0)
                .min(error_c + filter_lag_allowance_c)
        } else {
            // Above target, predictive lead was cutting hold power too aggressively and
            // creating wide bang-bang cycles. Keep actual-temperature guard on the
            // overshoot side and reserve predictive lead for under-target recovery.
            error_c
        };
        let hold_reenter_error_c = control_target
            .hold_exit_error_c
            .max(control_target.hold_on_error_c)
            .max(hold_entry_gate_c + HEATER_HOLD_PHASE_HYSTERESIS_C);
        // A projected crossing alone is not enough to coast. The physical sensor and the
        // filtered controller state must both be inside the profile's Hold exit gate first.
        // Otherwise a high rising slope can cut heating several degrees below target.
        let approach_coast_gate_c = hold_entry_gate_c.max(control_target.hold_exit_error_c)
            + (hold_entry_measurement_margin_c * 2.0);
        let approach_predictive_coast_ready = approach_control_error_c <= 0.0
            && error_c <= approach_coast_gate_c
            && control_error_c <= control_target.hold_exit_error_c;
        // Phase residency follows the actual plate temperature. Filtered and projected errors
        // shape power, but using their lag to leave Hold creates rapid Hold/Approach oscillation.
        let hold_exit_error_c = error_c;
        let brake_distance_c = control_target
            .brake_distance_c
            .max(control_target.hold_entry_error_c + 0.1);
        let warmup_handoff_error_c = warmup_handoff_error_c(
            brake_distance_c,
            control_target.warmup_reenter_error_c,
            filtered_temp_slope_c_per_profile_tick,
            control_target.approach_lead_ticks,
        );

        let mut next_phase = self.phase;
        let previous_phase = self.phase;
        match self.phase {
            HeaterControlPhase::Warmup => {
                if warmup_handoff_ready(
                    error_c,
                    previous_error_c,
                    control_error_c,
                    brake_distance_c,
                    warmup_handoff_error_c,
                ) {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
            HeaterControlPhase::Approach => {
                let timeout_hold_ready = self.phase_ticks >= approach_max_cycles
                    && error_c <= hold_entry_gate_c
                    && hold_state_ready;
                // Re-enter warmup only when the actual plate reading is well below the brake
                // boundary. Filter lag after a rising sample must not undo a deliberate brake.
                if error_c >= brake_distance_c + control_target.warmup_reenter_error_c {
                    next_phase = HeaterControlPhase::Warmup;
                } else if (approach_control_error_c <= hold_entry_gate_c
                    && error_c <= hold_entry_gate_c
                    && previous_error_c <= hold_entry_gate_c + hold_entry_measurement_margin_c)
                    || actual_crossed_target_ready
                    || timeout_hold_ready
                {
                    next_phase = HeaterControlPhase::Hold;
                }
            }
            HeaterControlPhase::Hold => {
                if hold_exit_error_c >= hold_reenter_error_c
                    && previous_error_c >= hold_reenter_error_c
                {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
        }

        if next_phase != self.phase {
            self.phase = next_phase;
            self.phase_ticks = 0;
            self.recovering_from_hold = previous_phase == HeaterControlPhase::Hold
                && self.phase == HeaterControlPhase::Approach;
            if self.phase == HeaterControlPhase::Warmup {
                self.warmup_started_at_ms = Some(now_ms);
            }
        } else {
            self.phase_ticks = self.phase_ticks.saturating_add(1);
            if self.phase != HeaterControlPhase::Approach {
                self.recovering_from_hold = false;
            }
        }

        if previous_phase != self.phase {
            if self.phase == HeaterControlPhase::Hold {
                let hold_entry_coast_guard_c = control_target
                    .hold_exit_error_c
                    .max(control_target.hold_on_error_c.max(0.05) * 2.0);
                let hold_entry_zero_output_ready = self.duty_percent == 0
                    && error_c <= control_target.hold_on_error_c.max(0.05) * 2.0;
                let hold_entry_projection_ready = self.duty_percent > 0
                    && error_c <= hold_entry_coast_guard_c
                    && (approach_control_error_c <= 0.0 || hold_control_error_c <= 0.0);
                self.hold_coast_active = filtered_temp_slope_c_per_profile_tick > 0.0
                    && (actual_crossed_target_ready
                        || error_c <= 0.0
                        || control_error_c <= 0.0
                        || hold_entry_zero_output_ready
                        || hold_entry_projection_ready);
                self.hold_coast_cooling_samples = 0;
                if actual_crossed_target_ready {
                    self.filtered_temp_c = Some(measured_temp_c);
                    self.previous_filtered_temp_c = Some(measured_temp_c);
                }
                let hold_entry_guard_error_c = if error_c > 0.0 {
                    hold_guard_error_c
                } else {
                    error_c
                };
                let hold_base_permille = hold_effective_base_permille(
                    hold_entry_guard_error_c,
                    hold_reenter_error_c,
                    control_target,
                );
                let positive_integral_limit = if hold_ki > 0.0 {
                    ((1_000.0 - hold_base_permille) / hold_ki).clamp(0.0, 255.0)
                } else {
                    0.0
                };
                let previous_output_permille = f32::from(self.duty_percent.min(100)) * 10.0;
                let hold_entry_base_permille = hold_base_permille
                    + (control_target.hold_kp_permille_per_c * hold_entry_guard_error_c);
                let actual_preload_ratio = if error_c > 0.0 {
                    (error_c / hold_entry_gate_c).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let projected_preload_ratio = if approach_guard_error_c > 0.0 {
                    (approach_guard_error_c / hold_entry_gate_c).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let preload_ratio = actual_preload_ratio.min(projected_preload_ratio);
                let carry_permille = (previous_output_permille - hold_entry_base_permille).max(0.0);
                let hold_entry_output_permille =
                    hold_entry_base_permille + (carry_permille * preload_ratio);
                self.hold_entry_output_percent =
                    percent_from_permille(hold_entry_output_permille.clamp(0.0, 1_000.0) as u16);
                self.hold_integral_c = if hold_ki > 0.0 {
                    ((carry_permille * preload_ratio) / hold_ki).clamp(0.0, positive_integral_limit)
                } else {
                    0.0
                };
            } else {
                self.hold_coast_active = false;
                self.hold_coast_cooling_samples = 0;
                self.hold_entry_output_percent = 0;
                if self.phase != HeaterControlPhase::Hold {
                    self.hold_integral_c = 0.0;
                }
            }
        }
        let coast_raw_cooling = measured_temp_c + 0.05 < previous_measured_temp_c
            && error_c >= control_target.hold_on_error_c.max(0.05);
        if self.hold_coast_active && coast_raw_cooling {
            self.hold_coast_cooling_samples = self.hold_coast_cooling_samples.saturating_add(1);
        } else {
            self.hold_coast_cooling_samples = 0;
        }
        let coast_plate_is_cooling =
            filtered_temp_slope_c_per_profile_tick <= -0.02 && self.hold_coast_cooling_samples >= 2;
        if self.hold_coast_active && coast_plate_is_cooling {
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
            self.phase_ticks = 0;
            self.hold_entry_output_percent = 0;
        }
        let duty_percent = if self.hold_coast_active
            || hold_guard_error_c <= -control_target.overshoot_cutoff_c
        {
            self.hold_integral_c = 0.0;
            0
        } else {
            match self.phase {
                HeaterControlPhase::Warmup => {
                    percent_from_permille(control_target.warmup_power_permille)
                }
                HeaterControlPhase::Approach => {
                    if approach_predictive_coast_ready {
                        0
                    } else {
                        let span = (brake_distance_c - control_target.hold_entry_error_c).max(0.1);
                        let ratio = ((approach_guard_error_c - control_target.hold_entry_error_c)
                            / span)
                            .clamp(0.0, 1.0);
                        let shaped_ratio = ratio.powf(control_target.approach_damping_exponent);
                        let sustain_floor =
                            f32::from(approach_sustain_floor_permille(control_target, error_c));
                        let approach_ceiling =
                            f32::from(control_target.approach_power_permille.min(1_000))
                                .max(sustain_floor);
                        let requested_permille =
                            sustain_floor + ((approach_ceiling - sustain_floor) * shaped_ratio);
                        percent_from_permille(requested_permille.clamp(0.0, 1_000.0) as u16)
                    }
                }
                HeaterControlPhase::Hold => {
                    if hold_prediction_blocks_reheat
                        || hold_filter_lag_blocks_reheat
                        || hold_actual_overshoot_blocks_reheat
                    {
                        self.hold_integral_c = 0.0;
                        0
                    } else {
                        let hold_base_permille = hold_effective_base_permille(
                            hold_guard_error_c,
                            hold_reenter_error_c,
                            control_target,
                        );
                        let positive_integral_limit = if hold_ki > 0.0 {
                            ((1_000.0 - hold_base_permille) / hold_ki).clamp(0.0, 255.0)
                        } else {
                            0.0
                        };
                        self.hold_integral_c = if hold_ki > 0.0 {
                            // The integral term only represents missing equilibrium heat. Letting it
                            // go negative turns a brief overshoot into a long zero-output valley.
                            (self.hold_integral_c + hold_guard_error_c)
                                .clamp(0.0, positive_integral_limit)
                        } else {
                            0.0
                        };
                        let mut requested_permille = hold_base_permille
                            + (control_target.hold_kp_permille_per_c * hold_guard_error_c)
                            + (hold_ki * self.hold_integral_c);
                        if hold_guard_error_c <= -control_target.hold_off_error_c {
                            self.hold_integral_c = 0.0;
                            let taper_span = (control_target.overshoot_cutoff_c
                                - control_target.hold_off_error_c)
                                .max(0.05);
                            let overshoot_c =
                                (-hold_guard_error_c).max(control_target.hold_off_error_c);
                            let taper_ratio = ((control_target.overshoot_cutoff_c - overshoot_c)
                                / taper_span)
                                .clamp(0.0, 1.0);
                            requested_permille =
                                requested_permille.clamp(0.0, 1_000.0) * taper_ratio;
                        }
                        let pi_percent =
                            percent_from_permille(requested_permille.clamp(0.0, 1_000.0) as u16);
                        if hold_guard_error_c > 0.0 && self.phase_ticks < hold_blend_cycles {
                            let blend_ratio = f32::from(self.phase_ticks.saturating_add(1))
                                / f32::from(hold_blend_cycles.max(1));
                            ((f32::from(self.hold_entry_output_percent)
                                + ((f32::from(pi_percent)
                                    - f32::from(self.hold_entry_output_percent))
                                    * blend_ratio))
                                .clamp(0.0, 100.0)
                                + 0.5) as u8
                        } else {
                            pi_percent
                        }
                    }
                }
            }
        };
        let predictive_coast_active = self.hold_coast_active
            || (self.phase == HeaterControlPhase::Approach && approach_predictive_coast_ready);
        let duty_percent = if predictive_coast_active {
            duty_percent
        } else {
            self.apply_under_target_reheat_floor(
                duty_percent,
                error_c,
                hold_control_error_c,
                control_target,
            )
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
            filtered_slope_c_per_s: filtered_temp_slope_c_per_profile_tick,
            coast_active: self.hold_coast_active,
            phase: self.phase,
        }
    }

    fn apply_under_target_reheat_floor(
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
                approach_sustain_floor_permille(control_target, error_c)
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
