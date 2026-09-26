use crate::memory::ThermalControlProfilePointConfig;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurveClass {
    OnCurve,
    BrakeLate,
    Underpowered,
    Oscillatory,
    Insufficient,
}

#[derive(Clone, Copy)]
pub(crate) struct TuneEvidence {
    pub target_c: i16,
    pub tunable_stop: bool,
    pub full_speed_failed: bool,
    pub overshoot_c: f64,
    pub hold_p2p_c: f64,
    pub residual_c: f64,
    pub under_c: f64,
    pub over_c: f64,
    pub hold_sample_count: u32,
    pub hold_threshold_crossed: bool,
    pub hold_mean_error_c: Option<f64>,
    pub hold_max_below_c: Option<f64>,
    pub first_hold_error_c: Option<f64>,
    pub hold_median_output_permille: Option<u16>,
    pub hold_p90_output_permille: Option<u16>,
    pub approach_median_output_permille: Option<u16>,
    pub approach_median_slope_c_per_s: Option<f64>,
    pub curve_class: CurveClass,
    pub first_hold_after_warmup_ms: Option<u64>,
    pub stable_window_after_warmup_ms: Option<u64>,
    pub settle_limit_ms: u64,
}

pub(crate) fn tune_point(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    if !evidence.tunable_stop {
        return previous;
    }
    let curve_needs_tuning = matches!(
        evidence.curve_class,
        CurveClass::BrakeLate | CurveClass::Underpowered | CurveClass::Oscillatory
    );
    if !evidence.full_speed_failed
        && evidence.overshoot_c <= 3.0
        && evidence.hold_p2p_c <= 3.0
        && !curve_needs_tuning
    {
        return previous;
    }

    let equilibrium = evidence
        .hold_median_output_permille
        .unwrap_or(previous.hold_power_permille);
    let hold_p90 = evidence
        .hold_p90_output_permille
        .unwrap_or(previous.hold_reheat_power_permille.max(equilibrium));
    let near_target_power = evidence
        .approach_median_output_permille
        .unwrap_or(previous.approach_floor_power_permille.max(equilibrium));
    let entering_below = evidence.first_hold_error_c.unwrap_or(0.0).max(0.0);
    let entering_above = (-evidence.first_hold_error_c.unwrap_or(0.0)).max(0.0);
    let hold_gate_lag = evidence.full_speed_failed
        && evidence.hold_sample_count == 0
        && evidence.hold_threshold_crossed
        && evidence.curve_class != CurveClass::Underpowered;
    let approach_underpowered =
        (evidence.full_speed_failed && evidence.hold_sample_count == 0 && !hold_gate_lag)
            || evidence.curve_class == CurveClass::Underpowered;
    let high_temp_power_limited = approach_underpowered
        && evidence.target_c >= 180
        && near_target_power >= 900
        && evidence
            .approach_median_slope_c_per_s
            .is_some_and(|slope| slope <= 1.0);
    let starved_low_temp_hold = evidence.full_speed_failed
        && evidence.target_c <= 120
        && evidence.hold_sample_count > 0
        && hold_p90 == 0
        && evidence.hold_mean_error_c.is_some_and(|error| error > 0.2)
        && evidence.under_c > evidence.over_c + 0.4;
    let stability_overshoot = evidence.full_speed_failed
        && evidence.hold_sample_count > 0
        && evidence.over_c > 1.5
        && (evidence.target_c <= 120 || evidence.over_c >= evidence.under_c)
        && !starved_low_temp_hold;
    let entry_residual_dominant = evidence.first_hold_error_c.is_some()
        && evidence.residual_c >= 1.5
        && evidence.over_c >= 1.5
        && evidence.over_c >= evidence.under_c + 0.4;
    let overshoot_dominant = evidence.curve_class == CurveClass::BrakeLate
        || evidence.overshoot_c > 3.0
        || stability_overshoot
        || entry_residual_dominant
        || (evidence.residual_c > 2.5
            && entering_above > 0.5
            && evidence.over_c > evidence.under_c);
    let low_temp_hold_entry_carry = evidence.full_speed_failed
        && evidence.target_c <= 120
        && evidence.hold_sample_count > 0
        && hold_p90 > previous.hold_power_permille
        && evidence.hold_p2p_c > 2.0
        && entering_below >= 0.4
        && evidence.residual_c >= 1.5
        && evidence.over_c > 1.5;
    let hold_ripple = evidence.curve_class == CurveClass::Oscillatory
        || (evidence.hold_sample_count > 0 && evidence.hold_p2p_c > 3.0);
    let bursty_low_temp_hold = evidence.target_c <= 120
        && evidence.hold_median_output_permille == Some(0)
        && hold_p90 >= previous.hold_power_permille.saturating_add(20)
        && evidence.under_c > evidence.over_c + 0.4
        && evidence.hold_mean_error_c.is_some_and(|error| error > 0.2);
    let timely_hold_late_stability = evidence.full_speed_failed
        && evidence
            .first_hold_after_warmup_ms
            .is_some_and(|time| time <= evidence.settle_limit_ms)
        && evidence
            .stable_window_after_warmup_ms
            .is_some_and(|time| time > evidence.settle_limit_ms)
        && evidence.under_c <= 1.5
        && evidence.over_c <= 1.5;

    let mut tuned = previous;
    if high_temp_power_limited {
        apply_high_temp_power_limit(&mut tuned, near_target_power);
    } else if timely_hold_late_stability {
        tuned.hold_exit_centi_c = tuned
            .hold_exit_centi_c
            .max(round_positive((evidence.under_c + 0.2) * 100.0).clamp(80, 300));
    } else if low_temp_hold_entry_carry {
        let before = tuned.hold_lead_ticks;
        tuned.hold_lead_ticks = tuned
            .hold_lead_ticks
            .saturating_add(if evidence.residual_c >= 2.0 { 2 } else { 1 })
            .min(8);
        if tuned.hold_lead_ticks == before {
            tuned.hold_power_permille = tuned.hold_power_permille.saturating_sub(20).max(40);
            tuned.hold_off_centi_c = tuned.hold_off_centi_c.saturating_add(20).min(400);
        }
        tuned.hold_reheat_power_permille = tuned
            .hold_reheat_power_permille
            .saturating_sub(30)
            .max(tuned.hold_power_permille);
    } else if hold_gate_lag {
        tuned.approach_lead_ticks = tuned
            .approach_lead_ticks
            .saturating_add(if evidence.target_c >= 120 { 1 } else { 2 })
            .min(12);
    } else if approach_underpowered {
        apply_approach_underpowered(&mut tuned, evidence.target_c, near_target_power);
    } else if overshoot_dominant {
        apply_overshoot(
            &mut tuned,
            previous,
            evidence,
            equilibrium,
            stability_overshoot,
            entry_residual_dominant,
        );
    } else if hold_ripple {
        apply_hold_ripple(
            &mut tuned,
            previous,
            evidence,
            equilibrium,
            hold_p90,
            bursty_low_temp_hold,
            entering_below,
            entering_above,
        );
    } else {
        return previous;
    }
    tuned.hold_reheat_power_permille = tuned
        .hold_reheat_power_permille
        .max(tuned.hold_power_permille);
    tuned.approach_power_permille = tuned
        .approach_power_permille
        .max(tuned.approach_floor_power_permille)
        .min(1_000);
    tuned.warmup_power_permille = tuned
        .warmup_power_permille
        .max(tuned.approach_power_permille)
        .min(1_000);
    tuned
}

fn apply_high_temp_power_limit(point: &mut ThermalControlProfilePointConfig, near_target: u16) {
    point.hold_entry_centi_c = 150;
    point.hold_exit_centi_c = point.hold_exit_centi_c.max(160);
    point.brake_distance_centi_c = point
        .hold_entry_centi_c
        .saturating_add(10)
        .clamp(100, 5_000);
    point.warmup_power_permille = 1_000;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 1_000;
    point.approach_damping_exponent_permille = 100;
    point.approach_lead_ticks = 0;
    point.hold_power_permille = near_target.saturating_add(50).clamp(950, 1_000);
    point.hold_reheat_power_permille = 1_000;
    point.hold_off_centi_c = point.hold_off_centi_c.min(80);
    point.overshoot_cutoff_centi_c = point
        .overshoot_cutoff_centi_c
        .min(180)
        .max(point.hold_off_centi_c.saturating_add(40));
}

fn apply_approach_underpowered(
    point: &mut ThermalControlProfilePointConfig,
    target_c: i16,
    near_target: u16,
) {
    let power_step = if target_c >= 180 { 120 } else { 80 };
    let lead_step = (point.approach_lead_ticks / 2).max(2);
    point.approach_floor_power_permille = step_toward(
        point.approach_floor_power_permille,
        near_target
            .saturating_add(power_step)
            .max(point.approach_floor_power_permille),
        power_step,
        point.hold_power_permille,
        1_000,
    );
    point.approach_power_permille = point
        .approach_power_permille
        .max(point.approach_floor_power_permille.saturating_add(80))
        .min(1_000);
    point.approach_damping_exponent_permille = point
        .approach_damping_exponent_permille
        .saturating_sub(90)
        .clamp(100, 4_000);
    point.approach_lead_ticks = point.approach_lead_ticks.saturating_sub(lead_step);
    point.brake_distance_centi_c = point
        .brake_distance_centi_c
        .saturating_sub(if target_c <= 120 { 120 } else { 60 })
        .max(100);
}

fn apply_overshoot(
    point: &mut ThermalControlProfilePointConfig,
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    equilibrium: u16,
    stability_overshoot: bool,
    entry_residual_dominant: bool,
) {
    let coast_gate_limited = evidence.target_c <= 120
        && stability_overshoot
        && evidence.residual_c <= 3.5
        && evidence.hold_sample_count > 0;
    let bounded_entry_residual = coast_gate_limited
        && evidence
            .first_hold_after_warmup_ms
            .is_some_and(|elapsed| elapsed <= evidence.settle_limit_ms);
    let brake_step = if bounded_entry_residual {
        80
    } else if coast_gate_limited {
        0
    } else {
        round_positive(evidence.residual_c * 100.0 + evidence.overshoot_c * 70.0)
            .clamp(if stability_overshoot { 100 } else { 80 }, 350)
    };
    point.brake_distance_centi_c = point
        .brake_distance_centi_c
        .saturating_add(brake_step)
        .clamp(100, 5_000);
    let damping_step = if bounded_entry_residual {
        50
    } else if evidence.residual_c > 3.0 {
        200
    } else {
        100
    };
    point.approach_damping_exponent_permille = point
        .approach_damping_exponent_permille
        .saturating_add(damping_step)
        .clamp(100, 4_000);

    if bounded_entry_residual {
        if evidence.first_hold_after_warmup_ms.unwrap_or_default() >= 9_000 {
            point.hold_entry_centi_c = point.hold_entry_centi_c.saturating_add(40).min(250);
        } else {
            let cutoff_step = round_positive((evidence.over_c - 1.5) * 100.0).clamp(20, 60);
            point.overshoot_cutoff_centi_c = point
                .overshoot_cutoff_centi_c
                .saturating_sub(cutoff_step)
                .max(50);
            point.hold_off_centi_c = point
                .hold_off_centi_c
                .min(point.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
        }
    } else if point.approach_lead_ticks < 12 {
        let step = if evidence.target_c <= 120 && evidence.residual_c > 3.0 {
            2
        } else {
            1
        };
        point.approach_lead_ticks = point.approach_lead_ticks.saturating_add(step).min(12);
    }
    if coast_gate_limited && !entry_residual_dominant && !bounded_entry_residual {
        let coast_step =
            round_positive((evidence.residual_c - 0.8).max(evidence.overshoot_c - 1.5) * 100.0)
                .clamp(50, 250);
        point.hold_exit_centi_c = point
            .hold_exit_centi_c
            .saturating_add(coast_step)
            .min(point.brake_distance_centi_c.saturating_sub(10))
            .clamp(point.hold_entry_centi_c, 5_000);
    }
    point.approach_floor_power_permille = point
        .approach_floor_power_permille
        .max(equilibrium.saturating_sub(30));
    let equilibrium_ready = !evidence.full_speed_failed || evidence.hold_sample_count >= 120;
    let equilibrium_observed = equilibrium_ready
        && (evidence.hold_max_below_c.is_some_and(|value| value >= 0.3)
            || evidence
                .hold_mean_error_c
                .is_some_and(|value| value.abs() <= 1.0));
    let coasted_through_hold = equilibrium_ready
        && evidence.hold_max_below_c.is_some_and(|value| value <= 0.1)
        && evidence.hold_mean_error_c.is_some_and(|value| value < -1.5)
        && evidence
            .hold_median_output_permille
            .is_some_and(|value| value <= previous.hold_power_permille);
    if coasted_through_hold {
        let step = if evidence.residual_c > 5.0 { 100 } else { 60 };
        point.approach_floor_power_permille = point
            .approach_floor_power_permille
            .saturating_sub(step)
            .max(point.hold_power_permille);
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_sub(step)
            .max(point.hold_power_permille);
    }
    if equilibrium_observed {
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_sub(100)
            .max(point.hold_power_permille);
        point.hold_power_permille = step_toward(
            point.hold_power_permille,
            equilibrium.saturating_sub(30),
            80,
            0,
            1_000,
        );
    }
    if evidence.target_c <= 120
        && evidence.residual_c > 5.0
        && evidence
            .approach_median_slope_c_per_s
            .is_some_and(|slope| slope > 2.0)
    {
        point.warmup_power_permille = point
            .warmup_power_permille
            .saturating_sub(250)
            .max(point.approach_power_permille);
    }
    point.hold_blend_ticks = point.hold_blend_ticks.saturating_sub(3).max(1);
    point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(4).max(8);
}

fn apply_hold_ripple(
    point: &mut ThermalControlProfilePointConfig,
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    equilibrium: u16,
    hold_p90: u16,
    bursty_low_temp_hold: bool,
    entering_below: f64,
    entering_above: f64,
) {
    let ripple_equilibrium = if bursty_low_temp_hold {
        previous
            .hold_power_permille
            .saturating_add(30)
            .max(hold_p90.saturating_sub(100))
    } else {
        equilibrium
    };
    let high_temp_entry_carry = evidence.target_c >= 180
        && entering_below >= 0.2
        && evidence.residual_c >= 1.6
        && evidence.over_c >= 1.0;
    let saturated_high_temp_hold = evidence.target_c >= 180
        && equilibrium >= 950
        && hold_p90 >= 990
        && evidence.over_c >= 1.0
        && evidence.under_c >= 1.0;
    let reheat_gap = hold_p90.saturating_sub(if bursty_low_temp_hold {
        ripple_equilibrium
    } else {
        equilibrium
    });
    let bounded_gap = if evidence.over_c >= evidence.under_c {
        (reheat_gap / 2).clamp(40, 100)
    } else {
        reheat_gap.clamp(80, 160)
    };
    point.hold_power_permille =
        step_toward(point.hold_power_permille, ripple_equilibrium, 80, 0, 1_000);
    let floor_target = if evidence.under_c > evidence.over_c {
        previous
            .approach_floor_power_permille
            .max(point.hold_power_permille.saturating_add(20))
    } else {
        point.hold_power_permille.saturating_add(20)
    };
    point.approach_floor_power_permille = point
        .approach_floor_power_permille
        .max(floor_target)
        .min(1_000);
    point.hold_reheat_power_permille = point
        .hold_power_permille
        .saturating_add(bounded_gap)
        .max(point.approach_floor_power_permille.saturating_add(40))
        .min(1_000);
    if evidence.under_c > 3.0 {
        point.hold_exit_centi_c = point
            .hold_exit_centi_c
            .max(round_positive(evidence.under_c * 100.0 * 0.67).clamp(100, 300));
        if evidence.target_c >= 180 {
            point.hold_lead_ticks = point.hold_lead_ticks.saturating_add(1).min(8);
        }
    }
    if saturated_high_temp_hold {
        let hold_off_c = f64::from(point.hold_off_centi_c) / 100.0;
        let cutoff_c = ((evidence.over_c - 0.7 * hold_off_c) / 0.3)
            .max(hold_off_c + 0.4)
            .clamp(1.2, 6.0);
        point.overshoot_cutoff_centi_c = point
            .overshoot_cutoff_centi_c
            .max(round_positive(cutoff_c * 100.0));
        point.hold_blend_ticks = point.hold_blend_ticks.clamp(1, 4);
    } else if high_temp_entry_carry {
        point.hold_on_centi_c = step_toward(point.hold_on_centi_c, 140, 60, 20, 250);
        point.hold_off_centi_c = point.hold_off_centi_c.saturating_sub(30).max(40);
        point.hold_blend_ticks = point.hold_blend_ticks.saturating_sub(4).max(1);
        point.hold_kp_permille_per_c = point
            .hold_kp_permille_per_c
            .saturating_add(6)
            .clamp(8, 10_000);
    } else if entering_above > 0.2 || evidence.over_c > evidence.under_c {
        point.hold_on_centi_c = point.hold_on_centi_c.saturating_add(30).clamp(20, 250);
        point.hold_exit_centi_c = point
            .hold_exit_centi_c
            .max(point.hold_on_centi_c)
            .clamp(20, 500);
        point.hold_blend_ticks = point.hold_blend_ticks.saturating_sub(3).max(1);
        point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(3).max(8);
    } else {
        point.hold_kp_permille_per_c = point
            .hold_kp_permille_per_c
            .saturating_add(3)
            .clamp(8, 10_000);
    }
}

fn round_positive(value: f64) -> u16 {
    (value.max(0.0) + 0.5) as u16
}

fn step_toward(current: u16, target: u16, step: u16, min: u16, max: u16) -> u16 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let bounded = target.clamp(min, max);
    if bounded > current {
        current
            .saturating_add((bounded - current).min(step))
            .clamp(min, max)
    } else {
        current
            .saturating_sub((current - bounded).min(step))
            .clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(target_c: i16) -> ThermalControlProfilePointConfig {
        ThermalControlProfilePointConfig {
            target_temp_c: target_c,
            brake_distance_centi_c: 300,
            warmup_power_permille: 1_000,
            warmup_reenter_centi_c: 400,
            approach_power_permille: 750,
            approach_floor_power_permille: 400,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 200,
            hold_power_permille: 350,
            hold_reheat_power_permille: 450,
            hold_entry_centi_c: 90,
            hold_exit_centi_c: 200,
            hold_on_centi_c: 30,
            hold_off_centi_c: 50,
            overshoot_cutoff_centi_c: 150,
            hold_kp_permille_per_c: 120,
            hold_ki_permille_per_c_tick: 12,
            hold_blend_ticks: 12,
            approach_lead_ticks: 0,
            hold_lead_ticks: 0,
        }
    }

    fn evidence(target_c: i16) -> TuneEvidence {
        TuneEvidence {
            target_c,
            tunable_stop: true,
            full_speed_failed: true,
            overshoot_c: 0.0,
            hold_p2p_c: 0.0,
            residual_c: 0.0,
            under_c: 0.0,
            over_c: 0.0,
            hold_sample_count: 0,
            hold_threshold_crossed: false,
            hold_mean_error_c: None,
            hold_max_below_c: None,
            first_hold_error_c: None,
            hold_median_output_permille: None,
            hold_p90_output_permille: None,
            approach_median_output_permille: None,
            approach_median_slope_c_per_s: None,
            curve_class: CurveClass::Insufficient,
            first_hold_after_warmup_ms: None,
            stable_window_after_warmup_ms: None,
            settle_limit_ms: 10_000,
        }
    }

    #[test]
    fn severe_residual_matches_main_strategy_vector() {
        let mut previous = point(60);
        previous.brake_distance_centi_c = 1_210;
        previous.approach_damping_exponent_permille = 1_040;
        previous.approach_power_permille = 520;
        previous.approach_lead_ticks = 7;
        previous.hold_power_permille = 90;
        previous.hold_reheat_power_permille = 340;
        let mut facts = evidence(60);
        facts.overshoot_c = 7.7;
        facts.hold_p2p_c = 6.8;
        facts.residual_c = 6.8;
        facts.over_c = 7.7;
        facts.hold_sample_count = 19;
        facts.first_hold_error_c = Some(-0.9);
        facts.hold_median_output_permille = Some(0);
        facts.hold_mean_error_c = Some(-4.61);
        facts.hold_max_below_c = Some(0.0);
        let tuned = tune_point(previous, facts);
        assert_eq!(tuned.brake_distance_centi_c, 1_560);
        assert_eq!(tuned.approach_damping_exponent_permille, 1_240);
        assert_eq!(tuned.approach_lead_ticks, 9);
        assert_eq!(tuned.approach_floor_power_permille, 400);
        assert_eq!(tuned.hold_power_permille, 90);
        assert_eq!(tuned.hold_reheat_power_permille, 340);
    }

    #[test]
    fn bounded_residual_matches_main_coast_gate_vector() {
        let mut previous = point(60);
        previous.brake_distance_centi_c = 1_910;
        let mut facts = evidence(60);
        facts.overshoot_c = 3.3;
        facts.hold_p2p_c = 2.4;
        facts.residual_c = 2.4;
        facts.over_c = 3.3;
        facts.hold_sample_count = 7;
        let tuned = tune_point(previous, facts);
        assert_eq!(tuned.brake_distance_centi_c, 1_910);
        assert_eq!(tuned.hold_exit_centi_c, 380);
    }

    #[test]
    fn high_temperature_power_limited_branch_matches_main() {
        let previous = point(220);
        let mut facts = evidence(220);
        facts.curve_class = CurveClass::Underpowered;
        facts.approach_median_output_permille = Some(960);
        facts.approach_median_slope_c_per_s = Some(0.35);
        let tuned = tune_point(previous, facts);
        assert_eq!(tuned.warmup_power_permille, 1_000);
        assert_eq!(tuned.approach_power_permille, 1_000);
        assert_eq!(tuned.approach_floor_power_permille, 1_000);
        assert_eq!(tuned.hold_power_permille, 1_000);
        assert_eq!(tuned.hold_reheat_power_permille, 1_000);
        assert_eq!(tuned.approach_lead_ticks, 0);
    }

    #[test]
    fn unsafe_or_incomplete_evidence_never_changes_candidate() {
        let previous = point(60);
        let mut facts = evidence(60);
        facts.tunable_stop = false;
        facts.overshoot_c = 9.0;
        assert_eq!(tune_point(previous, facts), previous);
    }
}
