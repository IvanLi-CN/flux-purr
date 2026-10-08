use crate::memory::ThermalControlProfilePointConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurveClass {
    OnCurve,
    BrakeLate,
    Underpowered,
    Oscillatory,
    Insufficient,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
    pub hold_off_sample_count: u32,
    pub hold_entry_output_permille: Option<u16>,
    pub hold_entry_pulse_only: bool,
    pub hold_threshold_crossed: bool,
    pub hold_mean_error_c: Option<f64>,
    pub hold_max_below_c: Option<f64>,
    pub first_hold_error_c: Option<f64>,
    pub hold_median_output_permille: Option<u16>,
    pub hold_p90_output_permille: Option<u16>,
    pub hold_max_output_permille: Option<u16>,
    pub approach_median_output_permille: Option<u16>,
    pub approach_floor_only: bool,
    pub approach_output_range: Option<(u16, u16)>,
    pub approach_median_slope_c_per_s: Option<f64>,
    pub curve_class: CurveClass,
    pub first_hold_after_warmup_ms: Option<u64>,
    pub stable_window_after_warmup_ms: Option<u64>,
    pub settle_limit_ms: u64,
    pub scout_hold_complete: bool,
    pub stable_window_verified: bool,
    pub first_band_observed: bool,
    pub first_band_exit_centi_c: Option<i16>,
    pub deadline_temperature_centi_c: Option<i16>,
}

pub(crate) fn observed_low_side_hold_failure(evidence: TuneEvidence) -> bool {
    evidence.tunable_stop
        && evidence.overshoot_c <= 3.0
        && evidence.first_band_observed
        && evidence.hold_sample_count >= 3
        && matches!(
            stability_gate_evidence(evidence).0,
            GateFailure::MissedLow | GateFailure::BrokeLow
        )
}

pub(crate) fn unobserved_low_side_failure(evidence: TuneEvidence) -> bool {
    evidence.tunable_stop
        && evidence.overshoot_c <= 3.0
        && evidence.hold_sample_count == 0
        && matches!(
            stability_gate_evidence(evidence).0,
            GateFailure::MissedLow | GateFailure::BrokeLow
        )
}

struct HoldRippleInputs {
    equilibrium: u16,
    hold_p90: u16,
    bursty_low_temp_hold: bool,
    entering_below: f64,
    entering_above: f64,
}

pub(crate) fn tune_point(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    if !evidence.tunable_stop {
        return previous;
    }
    if let Some(braked) = low_power_coasting_candidate(previous, evidence) {
        return braked;
    }
    if let Some(damped) = coasting_high_side_candidate(previous, evidence) {
        return damped;
    }
    if let Some(damped) = limited_hold_high_side_candidate(previous, evidence) {
        return damped;
    }
    if let Some(trimmed) = hold_ripple_trim_candidate(previous, evidence) {
        return trimmed;
    }
    if let Some(conservative) = conservative_high_side_candidate(previous, evidence) {
        return conservative;
    }
    if let Some(recovery) = saturated_hold_ripple_candidate(previous, evidence) {
        return recovery;
    }
    if let Some(recovery) = hold_reheat_headroom_candidate(previous, evidence) {
        return recovery;
    }
    if let Some(recovery) = hold_recovery_candidate(previous, evidence) {
        return recovery;
    }
    let predicted = tune_base_point(previous, evidence);
    let proposed = apply_stability_gate(previous, predicted, evidence);
    preserve_unobserved_low_side_hold(previous, proposed, evidence)
}

fn preserve_unobserved_low_side_hold(
    previous: ThermalControlProfilePointConfig,
    mut proposed: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    let (failure, _) = stability_gate_evidence(evidence);
    if evidence.hold_sample_count == 0
        && matches!(failure, GateFailure::MissedLow | GateFailure::BrokeLow)
    {
        // A pre-Hold deficit only supports Approach feedback. Changing unused
        // Hold settings invents progress and prevents the distinct-point search.
        proposed.hold_power_permille = previous.hold_power_permille;
        proposed.hold_reheat_power_permille = previous.hold_reheat_power_permille;
        proposed.hold_entry_centi_c = previous.hold_entry_centi_c;
        proposed.hold_exit_centi_c = previous.hold_exit_centi_c;
        proposed.hold_on_centi_c = previous.hold_on_centi_c;
        proposed.hold_off_centi_c = previous.hold_off_centi_c;
        proposed.hold_kp_permille_per_c = previous.hold_kp_permille_per_c;
        proposed.hold_ki_permille_per_c_tick = previous.hold_ki_permille_per_c_tick;
        proposed.hold_blend_ticks = previous.hold_blend_ticks;
        proposed.hold_lead_ticks = previous.hold_lead_ticks;
        proposed.overshoot_cutoff_centi_c = previous.overshoot_cutoff_centi_c;
    }
    proposed
}

pub(crate) fn bounded_failure_alternative(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    attempt: u16,
) -> Option<ThermalControlProfilePointConfig> {
    if !evidence.tunable_stop || attempt >= 32 {
        return None;
    }
    let (failure, _) = stability_gate_evidence(evidence);
    let high = matches!(failure, GateFailure::BrokeHigh | GateFailure::MissedHigh)
        || evidence.overshoot_c > 3.0;
    let low = !high && matches!(failure, GateFailure::BrokeLow | GateFailure::MissedLow);
    if high && low_power_coasting_evidence(evidence) {
        return earlier_braking_candidate(previous, 40 * (attempt + 1));
    }
    if !high && !low {
        return None;
    }
    let hold_observed = evidence.first_band_observed && evidence.hold_sample_count >= 3;
    if high && hold_observed && (hold_off_after_entry(evidence) || limited_hold_outputs(evidence)) {
        if evidence.target_c <= 150 && evidence.approach_floor_only {
            return earlier_braking_candidate(previous, 40 * (attempt + 1));
        }
        return coasting_failure_alternative(previous, attempt);
    }
    let step = attempt / 4 + 1;
    let channel = attempt % 4;
    let mut point = previous;
    match (high, hold_observed, channel) {
        (true, true, 0) => {
            point.hold_power_permille = previous
                .hold_power_permille
                .saturating_sub(20 * step)
                .max(50)
                .min(previous.hold_power_permille)
        }
        (true, true, 1) => {
            point.hold_kp_permille_per_c = previous
                .hold_kp_permille_per_c
                .saturating_sub(2 * step)
                .max(8)
                .min(previous.hold_kp_permille_per_c)
        }
        (true, _, 2) | (true, false, 0) => {
            point.brake_distance_centi_c = previous
                .brake_distance_centi_c
                .saturating_add(40 * step)
                .min(5_000)
        }
        (true, _, _) => {
            point.approach_damping_exponent_permille = previous
                .approach_damping_exponent_permille
                .saturating_add(100 * step)
                .min(4_000)
        }
        (false, true, 0) => {
            point.hold_reheat_power_permille = previous
                .hold_reheat_power_permille
                .saturating_add(40 * step)
                .min(1_000)
        }
        (false, true, 1) => {
            point.hold_kp_permille_per_c = previous
                .hold_kp_permille_per_c
                .saturating_add(4 * step)
                .min(1_000)
                .max(previous.hold_kp_permille_per_c)
        }
        (false, true, 2) => {
            point.hold_power_permille = previous
                .hold_power_permille
                .saturating_add(40 * step)
                .min(1_000)
        }
        (false, true, _) => {
            point.hold_on_centi_c = previous
                .hold_on_centi_c
                .saturating_sub(5 * step)
                .max(5)
                .min(previous.hold_on_centi_c)
        }
        (false, false, 0 | 2) => {
            point.brake_distance_centi_c = previous
                .brake_distance_centi_c
                .saturating_sub(40 * step)
                .max(100)
        }
        (false, false, _) => {
            point.approach_damping_exponent_permille = previous
                .approach_damping_exponent_permille
                .saturating_sub(100 * step)
                .max(100)
        }
    }
    (point != previous).then_some(point)
}

fn coasting_failure_alternative(
    previous: ThermalControlProfilePointConfig,
    attempt: u16,
) -> Option<ThermalControlProfilePointConfig> {
    let step = attempt / 4 + 1;
    let mut point = previous;
    if attempt.is_multiple_of(2) {
        point.approach_damping_exponent_permille = previous
            .approach_damping_exponent_permille
            .saturating_add(100 * step)
            .min(4_000);
    } else {
        point.brake_distance_centi_c = previous
            .brake_distance_centi_c
            .saturating_add(40 * step)
            .min(5_000);
    }
    (point != previous).then_some(point)
}

fn coasting_high_side_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    if !evidence.tunable_stop
        || evidence.target_c > 150
        || !evidence.full_speed_failed
        || evidence.stable_window_verified
        || !evidence.first_band_observed
        || evidence.hold_sample_count < 3
        || !hold_off_after_entry(evidence)
        || stability_gate_evidence(evidence).0 != GateFailure::BrokeHigh
    {
        return None;
    }
    if evidence.approach_floor_only {
        // Damping cannot reduce a model request already clamped to its minimum.
        // Remove stored Warmup energy through the existing braking channel.
        let step = round_positive((evidence.overshoot_c - 1.5).max(0.5) * 100.0).clamp(40, 100);
        // Keep this classified failure at its bound; falling through would
        // change unused channels and fabricate an effective proposal.
        return Some(earlier_braking_candidate(previous, step).unwrap_or(previous));
    }
    let step = round_positive((evidence.overshoot_c - 1.5).max(0.5) * 1_000.0).clamp(500, 1_000);
    let mut point = previous;
    point.approach_damping_exponent_permille = previous
        .approach_damping_exponent_permille
        .saturating_add(step)
        .min(4_000);
    (point != previous).then_some(point)
}

fn low_power_coasting_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    if !low_power_coasting_evidence(evidence) {
        return None;
    }
    // A small above-floor pulse is not floor-only evidence. Nevertheless,
    // almost unpowered Hold with a large residual rise requires removal
    // of stored Warmup heat before tuning an already almost-minimal Approach.
    // Correct the measured excess toward the middle of the stable band.
    let step = round_positive((evidence.overshoot_c - 0.75).max(0.4) * 100.0).clamp(40, 500);
    Some(earlier_braking_candidate(previous, step).unwrap_or(previous))
}

fn low_power_coasting_evidence(evidence: TuneEvidence) -> bool {
    if !evidence.tunable_stop
        || evidence.target_c > 150
        || !evidence.full_speed_failed
        || evidence.stable_window_verified
        || !evidence.first_band_observed
        || evidence.hold_sample_count < 3
        || !(evidence.hold_off_sample_count == evidence.hold_sample_count
            || (evidence.hold_entry_pulse_only
                && evidence.hold_off_sample_count.saturating_mul(10)
                    >= evidence.hold_sample_count.saturating_mul(9)))
        || stability_gate_evidence(evidence).0 != GateFailure::BrokeHigh
        || !evidence.approach_output_range.is_some_and(|(min, max)| {
            min >= crate::thermal_plant::APPROACH_MIN_PERMILLE
                && max <= 1_000
                && (max <= 100
                    || evidence
                        .approach_median_output_permille
                        .is_some_and(|median| median <= 100))
        })
    {
        return false;
    }
    true
}

fn earlier_braking_candidate(
    previous: ThermalControlProfilePointConfig,
    step_centi_c: u16,
) -> Option<ThermalControlProfilePointConfig> {
    let point = ThermalControlProfilePointConfig {
        brake_distance_centi_c: previous
            .brake_distance_centi_c
            .saturating_add(step_centi_c)
            .min(5_000),
        ..previous
    };
    (point != previous).then_some(point)
}

fn hold_off_after_entry(evidence: TuneEvidence) -> bool {
    evidence.hold_off_sample_count == evidence.hold_sample_count
        || (evidence.hold_entry_pulse_only
            && evidence.hold_off_sample_count.saturating_mul(10)
                >= evidence.hold_sample_count.saturating_mul(9))
        || (evidence
            .hold_entry_output_permille
            .is_some_and(|output| (1..=50).contains(&output))
            && evidence.hold_off_sample_count.saturating_add(1) == evidence.hold_sample_count)
}

fn limited_hold_outputs(evidence: TuneEvidence) -> bool {
    evidence.hold_sample_count >= 3
        && evidence
            .hold_max_output_permille
            .is_some_and(|maximum| maximum <= 100)
        && evidence.hold_off_sample_count.saturating_mul(5)
            >= evidence.hold_sample_count.saturating_mul(4)
}

fn limited_hold_high_side_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    if !evidence.tunable_stop
        || evidence.target_c > 150
        || !evidence.full_speed_failed
        || evidence.stable_window_verified
        || !evidence.first_band_observed
        || !limited_hold_outputs(evidence)
        || !evidence.hold_max_below_c.is_some_and(|below| below <= 1.5)
        || evidence.over_c <= 1.5
        || !evidence.approach_output_range.is_some_and(|(min, max)| {
            min >= crate::thermal_plant::APPROACH_MIN_PERMILLE && max > 100
        })
    {
        return None;
    }
    // This is limited observed Hold supply, not a zero-power coast. Keep
    // the handoff and Hold unchanged while reducing effective Approach heat.
    let step = round_positive((evidence.overshoot_c - 1.5).max(0.5) * 1_000.0).clamp(500, 1_000);
    Some(ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: previous
            .approach_damping_exponent_permille
            .saturating_add(step)
            .min(4_000),
        ..previous
    })
}

fn saturated_hold_ripple_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    let lower_band = i32::from(evidence.target_c) * 100 - 150;
    if !evidence.first_band_observed
        || !evidence.stable_window_verified
        || evidence.full_speed_failed
        || evidence.hold_sample_count < 3
        || evidence.hold_off_sample_count < 3
        || evidence.hold_off_sample_count.saturating_mul(20) < evidence.hold_sample_count
        || !(0.5..=3.0).contains(&evidence.overshoot_c)
        || !(3.0..=4.0).contains(&evidence.hold_p2p_c)
        || evidence.hold_p2p_c == 3.0
        || evidence
            .first_band_exit_centi_c
            .is_none_or(|temperature| i32::from(temperature) >= lower_band)
        || evidence
            .hold_median_output_permille
            .is_none_or(|value| value < 900)
        || evidence
            .hold_p90_output_permille
            .is_none_or(|value| value < 950)
        || previous.hold_power_permille < 900
        || previous.hold_reheat_power_permille < 900
    {
        return None;
    }
    // The approach reached its window on time and Hold crossed both sides.
    // Full-output/cutoff ripple needs an earlier, smaller recovery pulse;
    // increasing an already saturated Approach cannot correct that cycle.
    let mut point = previous;
    point.hold_power_permille = point.hold_power_permille.saturating_sub(100).max(700);
    point.hold_reheat_power_permille = point
        .hold_reheat_power_permille
        .saturating_sub(100)
        .max(point.hold_power_permille);
    point.hold_lead_ticks = point.hold_lead_ticks.saturating_sub(1);
    point.hold_on_centi_c = point.hold_on_centi_c.saturating_sub(5).max(5);
    point.hold_blend_ticks = 1;
    (point != previous).then_some(point)
}

fn hold_recovery_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    let lower_band = i32::from(evidence.target_c) * 100 - 150;
    if !evidence.first_band_observed
        || evidence.hold_sample_count < 3
        || evidence.overshoot_c > 3.0
        || evidence.hold_max_below_c.is_some_and(|below| below <= 1.5)
        || evidence
            .first_band_exit_centi_c
            .is_none_or(|temperature| i32::from(temperature) >= lower_band)
        || evidence
            .hold_median_output_permille
            .is_none_or(|output| output > 100)
    {
        return None;
    }
    // The approach already reached the band. A low-side exit while Hold is
    // mostly off is a recovery problem, not evidence for more Approach energy.
    let mut point = previous;
    point.hold_on_centi_c = previous.hold_on_centi_c.saturating_sub(5).max(5);
    point.hold_reheat_power_permille = previous
        .hold_reheat_power_permille
        .saturating_add(80)
        .max(previous.hold_power_permille)
        .min(1000);
    point.hold_kp_permille_per_c = previous.hold_kp_permille_per_c.saturating_add(20).min(1000);
    point.hold_blend_ticks = 1;
    Some(point)
}

fn hold_reheat_headroom_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    let lower_band = i32::from(evidence.target_c) * 100 - 150;
    let model_floor_handoff = evidence.approach_floor_only
        && evidence.approach_median_output_permille
            == Some(crate::thermal_plant::APPROACH_MIN_PERMILLE);
    let legacy_high_output_handoff = previous.approach_floor_power_permille >= 900
        && evidence
            .approach_median_output_permille
            .is_some_and(|output| output >= 900);
    if evidence.target_c < 180
        || !evidence.first_band_observed
        || evidence.hold_sample_count < 3
        || evidence.hold_off_sample_count < 3
        || evidence.hold_max_below_c.is_some_and(|below| below <= 1.5)
        || evidence.overshoot_c > 1.5
        || evidence.hold_p2p_c > 3.0
        || evidence
            .first_hold_after_warmup_ms
            .is_none_or(|time| time > evidence.settle_limit_ms)
        || evidence
            .first_band_exit_centi_c
            .is_none_or(|temperature| i32::from(temperature) >= lower_band)
        || !(model_floor_handoff || legacy_high_output_handoff)
        || evidence
            .hold_median_output_permille
            .is_none_or(|value| !(101..950).contains(&value))
        || evidence
            .hold_p90_output_permille
            .is_none_or(|value| value > 1000)
    {
        return None;
    }
    // Timely physical Hold followed by a low-band exit is not a missing
    // Approach handoff, including the model's measured 5% braking handoff.
    // Its retained profile clamps do not describe the effective trajectory.
    // Preserve the smaller sustaining baseline and restore
    // only the low-side reheat headroom lost by a ripple trim. A PI peak
    // at full output does not exhaust the configured base or lead headroom.
    let mut point = previous;
    point.hold_reheat_power_permille = point
        .hold_reheat_power_permille
        .saturating_add(80)
        .min(1000);
    point.hold_on_centi_c = point.hold_on_centi_c.saturating_sub(5).max(5);
    point.hold_lead_ticks = point.hold_lead_ticks.saturating_sub(1);
    point.hold_blend_ticks = 1;
    (point != previous).then_some(point)
}

pub(crate) fn reseed_after_hold_confirm(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    if !evidence.tunable_stop {
        return previous;
    }
    if let Some(braked) = low_power_coasting_candidate(previous, evidence) {
        return braked;
    }
    if let Some(damped) = coasting_high_side_candidate(previous, evidence) {
        return damped;
    }
    if let Some(damped) = limited_hold_high_side_candidate(previous, evidence) {
        return damped;
    }
    // Firmware-only high-temperature exception: use the complete confirmation
    // evidence before the frozen parsed-stage projection drops its band facts.
    if evidence.target_c > 150
        && let Some(conservative) = conservative_high_side_candidate(previous, evidence)
    {
        return conservative;
    }
    if let Some(recovery) = saturated_hold_ripple_candidate(previous, evidence) {
        return recovery;
    }
    if let Some(recovery) = hold_reheat_headroom_candidate(previous, evidence) {
        return recovery;
    }
    if let Some(recovery) = hold_recovery_candidate(previous, evidence) {
        return recovery;
    }
    // The frozen external confirmation path tunes parsed stage metrics/guard,
    // then applies a separate confirmation nudge from the full audit analysis.
    // Its parsed stage has no curve/hold analysis. Preserve that projection
    // here; scout/batch evidence and all recorded device facts stay intact.
    let base = TuneEvidence {
        residual_c: evidence.overshoot_c,
        under_c: 0.0,
        over_c: 0.0,
        hold_sample_count: 0,
        hold_off_sample_count: 0,
        hold_entry_output_permille: None,
        hold_mean_error_c: None,
        hold_max_below_c: None,
        first_hold_error_c: None,
        hold_median_output_permille: None,
        hold_p90_output_permille: None,
        approach_median_output_permille: None,
        approach_median_slope_c_per_s: None,
        curve_class: CurveClass::Insufficient,
        ..evidence
    };
    let mut point = tune_base_point(previous, base);
    let (failure, _) = stability_gate_evidence(evidence);
    if evidence.target_c > 150 {
        reseed_high_temperature_hold(&mut point, evidence, failure);
    } else {
        reseed_low_temperature_hold(&mut point, evidence, failure);
    }
    preserve_unobserved_low_side_hold(previous, point, evidence)
}

fn reseed_high_temperature_hold(
    point: &mut ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    failure: GateFailure,
) {
    let median = evidence.hold_median_output_permille.unwrap_or(0);
    let p90 = evidence.hold_p90_output_permille.unwrap_or(0);
    if matches!(failure, GateFailure::BrokeHigh | GateFailure::LowMargin)
        && median >= 900
        && p90 >= 950
    {
        point.hold_power_permille = point.hold_power_permille.saturating_sub(220).max(700);
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_sub(260)
            .max(point.hold_power_permille)
            .max(720);
        point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(4).max(8);
        point.overshoot_cutoff_centi_c = point.overshoot_cutoff_centi_c.saturating_sub(40).max(120);
        point.hold_entry_centi_c = point.hold_entry_centi_c.saturating_sub(20).max(120);
        point.hold_exit_centi_c = point.hold_exit_centi_c.saturating_sub(20).max(120);
    }
    if matches!(failure, GateFailure::BrokeLow | GateFailure::MissedLow)
        && (median == 0 || p90 < 900)
    {
        point.brake_distance_centi_c = point.brake_distance_centi_c.saturating_sub(40).max(120);
        point.approach_lead_ticks = point.approach_lead_ticks.saturating_sub(1);
        point.hold_power_permille = point
            .hold_power_permille
            .saturating_add(80)
            .clamp(700, 1_000);
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_add(60)
            .max(point.hold_power_permille)
            .min(1_000);
        point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_add(4).min(24);
        point.hold_on_centi_c = point.hold_on_centi_c.saturating_add(4).min(60);
    }
}

fn reseed_low_temperature_hold(
    point: &mut ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    failure: GateFailure,
) {
    let high_side = matches!(failure, GateFailure::BrokeHigh | GateFailure::MissedHigh);
    let within_gate = evidence.scout_hold_complete
        && evidence
            .stable_window_after_warmup_ms
            .is_some_and(|start| start <= evidence.settle_limit_ms)
        && failure == GateFailure::Other;
    let exceeds_hard_gate =
        (high_side || within_gate) && (evidence.overshoot_c > 3.0 || evidence.hold_p2p_c > 3.0);
    if !exceeds_hard_gate
        && !(high_side && (evidence.overshoot_c > 1.5 || evidence.hold_p2p_c > 1.5))
    {
        return;
    }
    let floor_c = if exceeds_hard_gate { 3.0 } else { 1.5 };
    let severity = (evidence.overshoot_c - floor_c)
        .max(evidence.hold_p2p_c - floor_c)
        .max(0.0);
    let mild = exceeds_hard_gate && severity < 1.0;
    let brake_step = if mild {
        40
    } else {
        round_positive(severity * 75.0 + 80.0).clamp(80, 350)
    };
    let lead_step = if severity >= 2.0 { 2 } else { u16::from(!mild) };
    let trim = if severity >= 2.0 {
        120
    } else if mild {
        40
    } else {
        80
    };
    point.brake_distance_centi_c = point
        .brake_distance_centi_c
        .saturating_add(brake_step)
        .clamp(450, 5_000);
    point.approach_lead_ticks = point.approach_lead_ticks.saturating_add(lead_step).min(12);
    point.approach_floor_power_permille = point
        .approach_floor_power_permille
        .saturating_sub(trim)
        .max(point.hold_power_permille)
        .max(100);
    point.approach_power_permille = point
        .approach_power_permille
        .saturating_sub(trim)
        .max(point.approach_floor_power_permille.saturating_add(80))
        .min(1_000);
    point.hold_reheat_power_permille = point
        .hold_reheat_power_permille
        .saturating_sub(trim / 2)
        .max(point.hold_power_permille);
    point.hold_kp_permille_per_c = point
        .hold_kp_permille_per_c
        .saturating_sub(4)
        .max(8)
        .min(point.hold_kp_permille_per_c);
    point.hold_blend_ticks = point.hold_blend_ticks.saturating_sub(1).max(1);
    point.overshoot_cutoff_centi_c = point.overshoot_cutoff_centi_c.saturating_sub(40).max(80);
    point.hold_off_centi_c = point
        .hold_off_centi_c
        .min(point.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
}

fn tune_base_point(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    let inputs = base_tuning_inputs(previous, evidence);
    let BaseTuningInputs {
        equilibrium,
        hold_p90,
        near_target_power,
        entering_below,
        entering_above,
        hold_gate_lag,
        approach_underpowered,
        high_temp_power_limited,
        stability_overshoot,
        entry_residual_dominant,
        overshoot_dominant,
        low_temp_hold_entry_carry,
        hold_ripple,
        bursty_low_temp_hold,
        timely_hold_late_stability,
    } = inputs;
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
            HoldRippleInputs {
                equilibrium,
                hold_p90,
                bursty_low_temp_hold,
                entering_below,
                entering_above,
            },
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

struct BaseTuningInputs {
    equilibrium: u16,
    hold_p90: u16,
    near_target_power: u16,
    entering_below: f64,
    entering_above: f64,
    hold_gate_lag: bool,
    approach_underpowered: bool,
    high_temp_power_limited: bool,
    stability_overshoot: bool,
    entry_residual_dominant: bool,
    overshoot_dominant: bool,
    low_temp_hold_entry_carry: bool,
    hold_ripple: bool,
    bursty_low_temp_hold: bool,
    timely_hold_late_stability: bool,
}

fn base_tuning_inputs(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> BaseTuningInputs {
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
    BaseTuningInputs {
        equilibrium,
        hold_p90,
        near_target_power,
        entering_below,
        entering_above,
        hold_gate_lag,
        approach_underpowered,
        high_temp_power_limited,
        stability_overshoot,
        entry_residual_dominant,
        overshoot_dominant,
        low_temp_hold_entry_carry,
        hold_ripple,
        bursty_low_temp_hold,
        timely_hold_late_stability,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GateFailure {
    LowMargin,
    MissedLow,
    MissedHigh,
    BrokeLow,
    BrokeHigh,
    Other,
}

fn stability_gate_evidence(evidence: TuneEvidence) -> (GateFailure, f64) {
    if evidence.scout_hold_complete
        && evidence.stable_window_verified
        && let Some(started) = evidence.stable_window_after_warmup_ms
        && started <= evidence.settle_limit_ms
    {
        let margin = if evidence.target_c > 150 { 500 } else { 1_000 };
        return (
            if evidence.settle_limit_ms - started < margin {
                GateFailure::LowMargin
            } else {
                GateFailure::Other
            },
            0.0,
        );
    }
    let target = i32::from(evidence.target_c) * 100;
    let lower = target - 150;
    let upper = target + 150;
    // A first low-band exit can be an Approach measurement fluctuation.
    // With physical Hold never below band, stopped or limited Hold supply and a later
    // upper-band excursion, the observed failure is stored heat, not reheat.
    // Preserve the first-exit fact and every qualification gate unchanged.
    if evidence.first_band_observed
        && evidence.hold_sample_count >= 3
        && (hold_off_after_entry(evidence) || limited_hold_outputs(evidence))
        && evidence.hold_max_below_c.is_some_and(|below| below <= 1.5)
        && evidence.over_c > 1.5
    {
        return (GateFailure::BrokeHigh, evidence.over_c - 1.5);
    }
    let temperature = if evidence.first_band_observed {
        evidence.first_band_exit_centi_c
    } else {
        evidence.deadline_temperature_centi_c
    };
    match temperature.map(i32::from) {
        Some(value) if value < lower => (
            if evidence.first_band_observed {
                GateFailure::BrokeLow
            } else {
                GateFailure::MissedLow
            },
            f64::from(lower - value) / 100.0,
        ),
        Some(value) if value > upper => (
            if evidence.first_band_observed {
                GateFailure::BrokeHigh
            } else {
                GateFailure::MissedHigh
            },
            f64::from(value - upper) / 100.0,
        ),
        _ => (GateFailure::Other, 0.0),
    }
}

fn apply_stability_gate(
    previous: ThermalControlProfilePointConfig,
    point: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> ThermalControlProfilePointConfig {
    let (failure, gap_c) = stability_gate_evidence(evidence);
    if matches!(
        failure,
        GateFailure::MissedLow | GateFailure::BrokeLow | GateFailure::LowMargin
    ) {
        apply_low_side_gate(previous, point, evidence, failure, gap_c)
    } else if matches!(failure, GateFailure::BrokeHigh | GateFailure::MissedHigh) {
        apply_high_side_gate(previous, point, evidence.target_c, gap_c)
    } else {
        point
    }
}

fn apply_low_side_gate(
    previous: ThermalControlProfilePointConfig,
    mut point: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    failure: GateFailure,
    gap_c: f64,
) -> ThermalControlProfilePointConfig {
    let low_temperature = evidence.target_c <= 150;
    let brake_step = if low_temperature {
        match failure {
            GateFailure::LowMargin => 100,
            GateFailure::MissedLow if gap_c <= 1.5 => 160,
            GateFailure::MissedLow => 300,
            _ if gap_c <= 0.5 => 120,
            _ if gap_c <= 1.5 => 180,
            _ => 260,
        }
    } else if failure == GateFailure::LowMargin {
        120
    } else if gap_c <= 0.5 {
        180
    } else if gap_c <= 1.5 {
        260
    } else {
        360
    };
    let brake_floor = if low_temperature { 450 } else { 150 };
    point.brake_distance_centi_c = previous
        .brake_distance_centi_c
        .saturating_sub(brake_step)
        .max(brake_floor.min(previous.brake_distance_centi_c));
    let power_step = if low_temperature {
        if failure == GateFailure::LowMargin {
            80
        } else if gap_c <= 1.5 {
            120
        } else {
            240
        }
    } else if gap_c <= 1.0 {
        180
    } else {
        260
    };
    point.approach_floor_power_permille = previous
        .approach_floor_power_permille
        .saturating_add(power_step)
        .max(previous.hold_power_permille)
        .min(1_000);
    point.approach_power_permille = previous
        .approach_power_permille
        .saturating_add(power_step)
        .max(point.approach_floor_power_permille.saturating_add(80))
        .min(1_000);
    point.approach_damping_exponent_permille = point
        .approach_damping_exponent_permille
        .saturating_sub(if low_temperature { 180 } else { 260 })
        .max(100);
    if low_temperature {
        point.approach_lead_ticks = previous.approach_lead_ticks.saturating_sub(1).min(12);
        point = bound_low_side_step(previous, point, evidence, failure, gap_c);
    }
    point
}

fn bound_low_side_step(
    previous: ThermalControlProfilePointConfig,
    mut point: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    failure: GateFailure,
    gap_c: f64,
) -> ThermalControlProfilePointConfig {
    // Bound the complete prediction after the gate correction, using
    // the frozen strategy's observed overshoot/ripple safety margin.
    let safe_step = evidence.overshoot_c <= 1.5 && evidence.hold_p2p_c <= 2.0;
    let (power_limit, brake_limit, damping_limit) =
        if failure == GateFailure::MissedLow && gap_c >= 1.5 && safe_step {
            (240, 300, 360)
        } else if gap_c <= 0.5 && safe_step {
            (120, 120, 180)
        } else {
            (80, 80, 120)
        };
    point.approach_floor_power_permille = point.approach_floor_power_permille.min(
        previous
            .approach_floor_power_permille
            .saturating_add(power_limit),
    );
    point.approach_power_permille = point
        .approach_power_permille
        .min(previous.approach_power_permille.saturating_add(power_limit))
        .max(point.approach_floor_power_permille.saturating_add(80));
    point.brake_distance_centi_c = point
        .brake_distance_centi_c
        .max(previous.brake_distance_centi_c.saturating_sub(brake_limit));
    point.approach_damping_exponent_permille = point.approach_damping_exponent_permille.max(
        previous
            .approach_damping_exponent_permille
            .saturating_sub(damping_limit),
    );
    point.approach_lead_ticks = point
        .approach_lead_ticks
        .max(previous.approach_lead_ticks.saturating_sub(1));
    point
}

fn apply_high_side_gate(
    previous: ThermalControlProfilePointConfig,
    mut point: ThermalControlProfilePointConfig,
    target_c: i16,
    gap_c: f64,
) -> ThermalControlProfilePointConfig {
    let low_temperature = target_c <= 150;
    let severe = low_temperature && gap_c >= 1.5;
    let (min_brake, max_brake) = if severe {
        (140, 240)
    } else if low_temperature {
        (80, 140)
    } else {
        (120, 180)
    };
    point.brake_distance_centi_c = point.brake_distance_centi_c.clamp(
        previous.brake_distance_centi_c.saturating_add(min_brake),
        previous.brake_distance_centi_c.saturating_add(max_brake),
    );
    if low_temperature {
        point.brake_distance_centi_c = point.brake_distance_centi_c.max(450);
    }
    point.approach_lead_ticks = point
        .approach_lead_ticks
        .max(
            previous
                .approach_lead_ticks
                .saturating_add(if severe { 2 } else { 1 }),
        )
        .min(12);
    point.approach_damping_exponent_permille = point
        .approach_damping_exponent_permille
        .clamp(
            previous
                .approach_damping_exponent_permille
                .saturating_add(50),
            previous
                .approach_damping_exponent_permille
                .saturating_add(180),
        )
        .max(180);
    if low_temperature {
        trim_low_temperature_high_side(previous, &mut point, severe);
    } else if point.hold_power_permille >= 900 {
        point.hold_power_permille = point.hold_power_permille.saturating_sub(180).max(780);
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_sub(200)
            .max(point.hold_power_permille)
            .max(800);
        point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(2).max(10);
        point.overshoot_cutoff_centi_c = point.overshoot_cutoff_centi_c.saturating_sub(20).max(140);
    }
    point
}

fn trim_low_temperature_high_side(
    previous: ThermalControlProfilePointConfig,
    point: &mut ThermalControlProfilePointConfig,
    severe: bool,
) {
    let trim = if severe { 180 } else { 80 };
    point.approach_floor_power_permille = previous
        .approach_floor_power_permille
        .saturating_sub(trim)
        .max(previous.hold_power_permille)
        .max(100);
    point.approach_power_permille = previous
        .approach_power_permille
        .saturating_sub(trim)
        .max(point.approach_floor_power_permille.saturating_add(80))
        .min(1_000);
    point.hold_reheat_power_permille = previous
        .hold_reheat_power_permille
        .saturating_sub(if severe { 100 } else { trim / 2 })
        .max(previous.hold_power_permille);
    point.hold_kp_permille_per_c = previous.hold_kp_permille_per_c.saturating_sub(4).max(8);
    if severe {
        point.hold_ki_permille_per_c_tick = previous.hold_ki_permille_per_c_tick.saturating_sub(1);
        point.hold_blend_ticks = previous.hold_blend_ticks.saturating_add(1).min(8);
    }
    point.overshoot_cutoff_centi_c = previous.overshoot_cutoff_centi_c.saturating_sub(40).max(80);
    point.hold_off_centi_c = previous
        .hold_off_centi_c
        .min(point.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
}

fn hold_ripple_trim_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    if evidence.target_c > 150
        || !evidence.scout_hold_complete
        || !evidence.stable_window_verified
        || evidence.full_speed_failed
        || evidence.overshoot_c > 3.0
        || !(3.0..=4.0).contains(&evidence.hold_p2p_c)
    {
        return None;
    }
    let mut point = previous;
    point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(4).max(8);
    point.hold_blend_ticks = point.hold_blend_ticks.saturating_add(1).min(8);
    (point != previous).then_some(point)
}

fn conservative_high_side_candidate(
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
) -> Option<ThermalControlProfilePointConfig> {
    if evidence.target_c > 240 {
        return None;
    }
    let severity_c = evidence
        .overshoot_c
        .max(evidence.hold_p2p_c)
        .max(evidence.residual_c);
    let upper = i32::from(evidence.target_c) * 100 + 150;
    let broke_high = evidence
        .first_band_exit_centi_c
        .is_some_and(|temperature| i32::from(temperature) > upper);
    let missed_upper_band = !evidence.first_band_observed
        && evidence
            .deadline_temperature_centi_c
            .is_some_and(|temperature| {
                let temperature = i32::from(temperature);
                temperature > upper
            });
    if !broke_high && !missed_upper_band {
        return None;
    }
    // An unverified upper-band break is a failed stability gate even when
    // overshoot and p2p remain below their separate 3C hard limits. The
    // firmware high-temperature exception must outrank Underpowered here.
    // A later exit after a verified window does not create this exception.
    let high_temperature_window_failure =
        evidence.target_c > 150 && evidence.full_speed_failed && !evidence.stable_window_verified;
    if severity_c <= 3.0 && !high_temperature_window_failure {
        return None;
    }

    let trim = if severity_c >= 20.0 { 180 } else { 100 };
    let mut point = previous;
    point.brake_distance_centi_c = point
        .brake_distance_centi_c
        .saturating_add(if severity_c >= 20.0 { 500 } else { 300 })
        .min(5_000);
    point.hold_entry_centi_c = point.hold_entry_centi_c.saturating_add(100).min(450);
    point.hold_exit_centi_c = point
        .hold_exit_centi_c
        .saturating_add(80)
        .min(point.hold_entry_centi_c.saturating_sub(20).max(20));
    point.approach_floor_power_permille = point
        .approach_floor_power_permille
        .saturating_sub(trim)
        .max(100);
    point.approach_power_permille = point
        .approach_power_permille
        .saturating_sub(trim)
        .max(point.approach_floor_power_permille.saturating_add(80))
        .min(1_000);
    point.hold_power_permille = point
        .hold_power_permille
        .saturating_sub(trim / 2)
        .max(100)
        .min(previous.hold_power_permille);
    point.hold_reheat_power_permille = point
        .hold_reheat_power_permille
        .saturating_sub(trim / 2)
        .max(point.hold_power_permille);
    point.hold_kp_permille_per_c = point
        .hold_kp_permille_per_c
        .saturating_sub(4)
        .max(8)
        .min(previous.hold_kp_permille_per_c);
    point.overshoot_cutoff_centi_c = point
        .overshoot_cutoff_centi_c
        .saturating_sub(40)
        .max(point.hold_off_centi_c.saturating_add(40));
    point.hold_off_centi_c = point
        .hold_off_centi_c
        .min(point.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
    (point != previous).then_some(point)
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
    apply_overshoot_equilibrium(point, previous, evidence, equilibrium);
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
    point.hold_kp_permille_per_c = point
        .hold_kp_permille_per_c
        .saturating_sub(4)
        .max(8)
        .min(previous.hold_kp_permille_per_c);
}

fn apply_overshoot_equilibrium(
    point: &mut ThermalControlProfilePointConfig,
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    equilibrium: u16,
) {
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
}

fn apply_hold_ripple(
    point: &mut ThermalControlProfilePointConfig,
    previous: ThermalControlProfilePointConfig,
    evidence: TuneEvidence,
    inputs: HoldRippleInputs,
) {
    let HoldRippleInputs {
        equilibrium,
        hold_p90,
        bursty_low_temp_hold,
        entering_below,
        entering_above,
    } = inputs;
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
    #[test]
    fn model_floor_headroom_requires_complete_timely_low_side_hold_evidence() {
        let previous = point(240);
        let facts = TuneEvidence {
            overshoot_c: 0.16,
            hold_p2p_c: 1.86,
            hold_sample_count: 26,
            hold_off_sample_count: 3,
            hold_median_output_permille: Some(833),
            hold_p90_output_permille: Some(1000),
            approach_median_output_permille: Some(50),
            approach_floor_only: true,
            first_hold_after_warmup_ms: Some(713),
            first_band_observed: true,
            first_band_exit_centi_c: Some(23830),
            settle_limit_ms: 5000,
            ..evidence(240)
        };
        assert!(hold_reheat_headroom_candidate(previous, facts).is_some());
        for unavailable in [
            TuneEvidence {
                approach_floor_only: false,
                ..facts
            },
            TuneEvidence {
                approach_median_output_permille: None,
                ..facts
            },
            TuneEvidence {
                approach_median_output_permille: Some(100),
                ..facts
            },
            TuneEvidence {
                first_hold_after_warmup_ms: Some(5001),
                ..facts
            },
            TuneEvidence {
                first_hold_after_warmup_ms: None,
                ..facts
            },
            TuneEvidence {
                first_band_exit_centi_c: Some(24160),
                ..facts
            },
            TuneEvidence {
                first_band_exit_centi_c: None,
                ..facts
            },
            TuneEvidence {
                first_band_observed: false,
                ..facts
            },
            TuneEvidence {
                hold_sample_count: 2,
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 2,
                ..facts
            },
            TuneEvidence {
                overshoot_c: 1.51,
                ..facts
            },
            TuneEvidence {
                hold_p2p_c: 3.01,
                ..facts
            },
            TuneEvidence {
                hold_median_output_permille: Some(950),
                ..facts
            },
            TuneEvidence {
                hold_p90_output_permille: None,
                ..facts
            },
        ] {
            assert!(hold_reheat_headroom_candidate(previous, unavailable).is_none());
        }
        assert_eq!(
            tune_point(
                previous,
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                }
            ),
            previous
        );
    }

    #[test]
    fn saturated_pi_peak_does_not_hide_configured_reheat_or_timing_headroom() {
        // Captured 240C scout: physical Hold starts after 2343ms, then
        // coasting and delayed recovery break the band at 238.42C.
        let mut previous = point(240);
        previous.brake_distance_centi_c = 150;
        previous.warmup_reenter_centi_c = 1000;
        previous.approach_floor_power_permille = 1000;
        previous.approach_power_permille = 1000;
        previous.approach_damping_exponent_permille = 100;
        previous.approach_tail_window_centi_c = 0;
        previous.hold_power_permille = 900;
        previous.hold_reheat_power_permille = 980;
        previous.hold_on_centi_c = 5;
        previous.hold_off_centi_c = 80;
        previous.hold_kp_permille_per_c = 15;
        previous.hold_entry_centi_c = 150;
        previous.hold_exit_centi_c = 160;
        previous.hold_ki_permille_per_c_tick = 1;
        previous.hold_blend_ticks = 1;
        previous.overshoot_cutoff_centi_c = 180;
        previous.hold_lead_ticks = 4;
        let facts = TuneEvidence {
            overshoot_c: 0.06,
            hold_p2p_c: 1.94,
            residual_c: 0.5,
            under_c: 1.72,
            over_c: 0.06,
            hold_sample_count: 11,
            hold_off_sample_count: 4,
            first_hold_error_c: Some(0.44),
            hold_median_output_permille: Some(810),
            hold_p90_output_permille: Some(1000),
            approach_median_output_permille: Some(1000),
            first_hold_after_warmup_ms: Some(2343),
            first_band_observed: true,
            first_band_exit_centi_c: Some(23842),
            settle_limit_ms: 5000,
            curve_class: CurveClass::OnCurve,
            ..evidence(240)
        };
        let mut expected = previous;
        expected.hold_reheat_power_permille = 1000;
        expected.hold_lead_ticks = 3;
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
        let mut earlier = expected;
        earlier.hold_lead_ticks = 2;
        assert_eq!(tune_point(expected, facts), earlier);
        expected.hold_lead_ticks = 0;
        assert!(hold_reheat_headroom_candidate(expected, facts).is_none());
        assert!(
            hold_reheat_headroom_candidate(
                previous,
                TuneEvidence {
                    hold_p90_output_permille: None,
                    ..facts
                }
            )
            .is_none()
        );
        assert_eq!(
            tune_point(
                previous,
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                }
            ),
            previous
        );
    }

    #[test]
    fn low_band_exit_after_saturated_approach_restores_reheat_headroom() {
        // Captured 240C scout after the 1000 -> 900 Hold trim. Approach
        // remains full, but Hold loses its window at 238.46C after coasting.
        let mut previous = point(240);
        previous.brake_distance_centi_c = 150;
        previous.warmup_reenter_centi_c = 1000;
        previous.approach_floor_power_permille = 1000;
        previous.approach_power_permille = 1000;
        previous.approach_damping_exponent_permille = 100;
        previous.approach_tail_window_centi_c = 0;
        previous.hold_power_permille = 900;
        previous.hold_reheat_power_permille = 900;
        previous.hold_on_centi_c = 6;
        previous.hold_off_centi_c = 80;
        previous.hold_kp_permille_per_c = 15;
        previous.hold_entry_centi_c = 150;
        previous.hold_exit_centi_c = 160;
        previous.hold_ki_permille_per_c_tick = 1;
        previous.hold_blend_ticks = 1;
        previous.overshoot_cutoff_centi_c = 180;
        previous.hold_lead_ticks = 5;
        let facts = TuneEvidence {
            hold_p2p_c: 1.52,
            residual_c: 0.32,
            under_c: 1.54,
            hold_sample_count: 22,
            hold_off_sample_count: 4,
            first_hold_error_c: Some(0.34),
            hold_median_output_permille: Some(910),
            hold_p90_output_permille: Some(920),
            approach_median_output_permille: Some(1000),
            first_hold_after_warmup_ms: Some(1073),
            first_band_observed: true,
            first_band_exit_centi_c: Some(23846),
            settle_limit_ms: 5000,
            curve_class: CurveClass::OnCurve,
            ..evidence(240)
        };
        let next = tune_point(previous, facts);
        assert!(next.hold_reheat_power_permille > previous.hold_reheat_power_permille);
        assert_eq!(next.hold_power_permille, previous.hold_power_permille);
        assert_eq!(next.approach_floor_power_permille, 1000);
        assert_eq!(next.brake_distance_centi_c, 150);
        assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
        assert!(next.hold_lead_ticks < previous.hold_lead_ticks);
        assert_eq!(next.hold_reheat_power_permille, 980);
        assert_eq!(reseed_after_hold_confirm(previous, facts), next);
        assert_eq!(
            tune_point(
                previous,
                TuneEvidence {
                    curve_class: CurveClass::Underpowered,
                    ..facts
                }
            ),
            next
        );
        for unavailable in [
            TuneEvidence {
                first_band_observed: false,
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 0,
                ..facts
            },
            TuneEvidence {
                overshoot_c: 3.1,
                ..facts
            },
            TuneEvidence {
                hold_p90_output_permille: None,
                ..facts
            },
            TuneEvidence {
                first_hold_after_warmup_ms: Some(6000),
                ..facts
            },
        ] {
            assert!(hold_reheat_headroom_candidate(previous, unavailable).is_none());
        }
        assert_eq!(
            tune_point(
                previous,
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                }
            ),
            previous
        );
    }

    #[test]
    fn saturated_hold_confirmation_uses_distinct_ripple_recovery() {
        // Captured 240C confirmation: the approach established a window in
        // 792ms; Hold alternated full output and cutoff, then broke low.
        let mut previous = point(240);
        previous.brake_distance_centi_c = 150;
        previous.warmup_reenter_centi_c = 1000;
        previous.approach_floor_power_permille = 1000;
        previous.approach_power_permille = 1000;
        previous.approach_damping_exponent_permille = 100;
        previous.approach_tail_window_centi_c = 0;
        previous.hold_power_permille = 1000;
        previous.hold_reheat_power_permille = 1000;
        previous.hold_on_centi_c = 11;
        previous.hold_off_centi_c = 80;
        previous.hold_kp_permille_per_c = 15;
        previous.hold_entry_centi_c = 150;
        previous.hold_exit_centi_c = 160;
        previous.hold_ki_permille_per_c_tick = 1;
        previous.hold_blend_ticks = 1;
        previous.overshoot_cutoff_centi_c = 180;
        previous.hold_lead_ticks = 6;
        let facts = TuneEvidence {
            overshoot_c: 1.26,
            hold_p2p_c: 3.16,
            under_c: 1.90,
            over_c: 1.26,
            hold_sample_count: 268,
            hold_off_sample_count: 51,
            hold_median_output_permille: Some(1000),
            hold_p90_output_permille: Some(1000),
            first_band_observed: true,
            first_band_exit_centi_c: Some(23808),
            stable_window_after_warmup_ms: Some(792),
            stable_window_verified: true,
            scout_hold_complete: true,
            full_speed_failed: false,
            ..evidence(240)
        };
        let next = reseed_after_hold_confirm(previous, facts);
        assert_ne!(next, previous);
        assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
        assert_eq!(
            next.approach_power_permille,
            previous.approach_power_permille
        );
        assert_eq!(next.approach_floor_power_permille, 1000);
        assert!(next.hold_power_permille < previous.hold_power_permille);
        assert!(next.hold_lead_ticks < previous.hold_lead_ticks);
        assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
        assert_eq!(tune_point(previous, facts), next);
        for unavailable in [
            TuneEvidence {
                full_speed_failed: true,
                ..facts
            },
            TuneEvidence {
                overshoot_c: 0.0,
                ..facts
            },
            TuneEvidence {
                hold_p90_output_permille: None,
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 0,
                ..facts
            },
        ] {
            assert!(saturated_hold_ripple_candidate(previous, unavailable).is_none());
        }
        assert_eq!(
            reseed_after_hold_confirm(
                previous,
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                }
            ),
            previous
        );
    }

    #[test]
    fn saturated_240c_low_hold_exit_changes_recovery_not_approach() {
        let mut previous = point(240);
        previous.brake_distance_centi_c = 150;
        previous.approach_floor_power_permille = 1000;
        previous.approach_power_permille = 1000;
        previous.hold_power_permille = 1000;
        previous.hold_reheat_power_permille = 1000;
        previous.hold_on_centi_c = 11;
        previous.hold_kp_permille_per_c = 15;
        let facts = TuneEvidence {
            first_band_observed: true,
            first_band_exit_centi_c: Some(23827),
            hold_sample_count: 20,
            hold_median_output_permille: Some(0),
            overshoot_c: 0.73,
            under_c: 1.73,
            curve_class: CurveClass::Underpowered,
            ..evidence(240)
        };
        let next = tune_point(previous, facts);
        assert_ne!(next, previous);
        assert_eq!(
            next.approach_power_permille,
            previous.approach_power_permille
        );
        assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
        assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
        assert!(next.hold_kp_permille_per_c > previous.hold_kp_permille_per_c);
    }
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

    #[test]
    fn missing_high_temperature_hold_does_not_invent_hold_feedback() {
        let mut previous = point(240);
        previous.brake_distance_centi_c = 150;
        previous.approach_damping_exponent_permille = 100;
        previous.hold_power_permille = 838;
        previous.hold_reheat_power_permille = 838;
        previous.hold_kp_permille_per_c = 30;
        previous.hold_ki_permille_per_c_tick = 1;
        let facts = TuneEvidence {
            approach_median_output_permille: Some(1_000),
            approach_median_slope_c_per_s: Some(0.2),
            curve_class: CurveClass::Underpowered,
            settle_limit_ms: 5_000,
            deadline_temperature_centi_c: Some(23_700),
            ..evidence(240)
        };
        for next in [
            tune_point(previous, facts),
            reseed_after_hold_confirm(previous, facts),
        ] {
            assert_eq!(next.hold_power_permille, previous.hold_power_permille);
            assert_eq!(
                next.hold_reheat_power_permille,
                previous.hold_reheat_power_permille
            );
            assert_eq!(next.hold_kp_permille_per_c, previous.hold_kp_permille_per_c);
            assert_eq!(next.hold_on_centi_c, previous.hold_on_centi_c);
            assert_eq!(next.hold_off_centi_c, previous.hold_off_centi_c);
            assert_eq!(next.hold_entry_centi_c, previous.hold_entry_centi_c);
            assert_eq!(next.hold_exit_centi_c, previous.hold_exit_centi_c);
            assert_eq!(
                next.overshoot_cutoff_centi_c,
                previous.overshoot_cutoff_centi_c
            );
        }
    }

    #[test]
    fn coasting_failure_alternatives_change_damping_before_unused_hold_feedback() {
        let previous = point(60);
        let facts = TuneEvidence {
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_175),
            hold_sample_count: 14,
            hold_off_sample_count: 14,
            ..evidence(60)
        };
        let first = bounded_failure_alternative(previous, facts, 0).unwrap();
        assert_eq!(first.approach_damping_exponent_permille, 1_100);
        let expected = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 1_100,
            ..previous
        };
        assert_eq!(first, expected);
        for attempt in 0..32 {
            let next = bounded_failure_alternative(previous, facts, attempt).unwrap();
            assert_eq!(next.hold_power_permille, previous.hold_power_permille);
            assert_eq!(
                next.hold_reheat_power_permille,
                previous.hold_reheat_power_permille
            );
            assert_eq!(next.hold_kp_permille_per_c, previous.hold_kp_permille_per_c);
            assert!(next.brake_distance_centi_c >= previous.brake_distance_centi_c);
            assert!(
                next.approach_damping_exponent_permille
                    >= previous.approach_damping_exponent_permille
            );
        }
    }

    #[test]
    fn distinct_failure_alternatives_preserve_direction_and_missing_hold() {
        let previous = point(60);
        for high in [true, false] {
            let facts = TuneEvidence {
                first_band_observed: true,
                first_band_exit_centi_c: Some(if high { 6_160 } else { 5_840 }),
                hold_sample_count: 0,
                overshoot_c: 0.0,
                ..evidence(60)
            };
            for attempt in 0..32 {
                let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
                assert_eq!(
                    alternative.hold_power_permille,
                    previous.hold_power_permille
                );
                assert_eq!(
                    alternative.hold_reheat_power_permille,
                    previous.hold_reheat_power_permille
                );
                if high {
                    assert!(alternative.brake_distance_centi_c >= previous.brake_distance_centi_c);
                    assert!(
                        alternative.approach_damping_exponent_permille
                            >= previous.approach_damping_exponent_permille
                    );
                } else {
                    assert!(alternative.brake_distance_centi_c <= previous.brake_distance_centi_c);
                    assert!(
                        alternative.approach_damping_exponent_permille
                            <= previous.approach_damping_exponent_permille
                    );
                }
            }
            assert!(
                bounded_failure_alternative(
                    previous,
                    TuneEvidence {
                        tunable_stop: false,
                        ..facts
                    },
                    0
                )
                .is_none()
            );
            assert!(bounded_failure_alternative(previous, facts, 32).is_none());
        }
    }

    #[test]
    fn saturated_failure_alternatives_do_not_reverse_or_fabricate_progress() {
        let mut previous = point(60);
        previous.brake_distance_centi_c = 5_000;
        previous.approach_damping_exponent_permille = 4_000;
        previous.hold_power_permille = 40;
        previous.hold_kp_permille_per_c = 4;
        let facts = TuneEvidence {
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_160),
            hold_sample_count: 3,
            overshoot_c: 0.0,
            ..evidence(60)
        };
        for attempt in 0..32 {
            assert!(bounded_failure_alternative(previous, facts, attempt).is_none());
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
            hold_off_sample_count: 0,
            hold_entry_output_permille: None,
            hold_entry_pulse_only: false,
            hold_threshold_crossed: false,
            hold_mean_error_c: None,
            hold_max_below_c: None,
            first_hold_error_c: None,
            hold_median_output_permille: None,
            hold_p90_output_permille: None,
            hold_max_output_permille: None,
            approach_median_output_permille: None,
            approach_floor_only: false,
            approach_output_range: None,
            approach_median_slope_c_per_s: None,
            curve_class: CurveClass::Insufficient,
            first_hold_after_warmup_ms: None,
            stable_window_after_warmup_ms: None,
            settle_limit_ms: 10_000,
            scout_hold_complete: false,
            stable_window_verified: false,
            first_band_observed: false,
            first_band_exit_centi_c: None,
            deadline_temperature_centi_c: None,
        }
    }

    #[test]
    fn unverified_timely_restart_keeps_the_broken_high_side_failure() {
        let mut facts = evidence(80);
        facts.scout_hold_complete = true;
        facts.stable_window_after_warmup_ms = Some(18_252);
        facts.settle_limit_ms = 30_000;
        facts.first_band_observed = true;
        facts.first_band_exit_centi_c = Some(8_155);
        assert!(stability_gate_evidence(facts) == (GateFailure::BrokeHigh, 0.05));
        facts.stable_window_verified = true;
        assert!(stability_gate_evidence(facts) == (GateFailure::Other, 0.0));
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
    fn observed_60c_high_side_failure_must_not_accelerate_next_candidate() {
        let mut previous = point(60);
        previous.brake_distance_centi_c = 1_100;
        previous.approach_lead_ticks = 6;
        previous.approach_power_permille = 450;
        previous.approach_floor_power_permille = 160;
        previous.approach_damping_exponent_permille = 4_000;
        previous.hold_power_permille = 135;
        previous.hold_reheat_power_permille = 170;
        previous.hold_entry_centi_c = 220;
        previous.hold_exit_centi_c = 400;
        previous.hold_lead_ticks = 6;
        previous.hold_blend_ticks = 1;

        let mut facts = evidence(60);
        facts.curve_class = CurveClass::Underpowered;
        facts.overshoot_c = 3.19;
        facts.hold_p2p_c = 5.22;
        facts.residual_c = 5.22;
        facts.under_c = 2.03;
        facts.over_c = 3.19;
        facts.hold_sample_count = 20;
        facts.first_hold_error_c = Some(2.03);
        facts.hold_median_output_permille = Some(0);
        facts.hold_p90_output_permille = Some(0);
        facts.first_band_observed = true;
        facts.first_band_exit_centi_c = Some(6_154);

        let tuned = tune_point(previous, facts);
        assert_eq!(tuned.brake_distance_centi_c, 1_400);
        assert_eq!(tuned.approach_lead_ticks, 6);
        assert_eq!(tuned.approach_power_permille, 350);
        assert_eq!(tuned.approach_floor_power_permille, 100);
    }

    #[test]
    fn observed_60c_lower_band_miss_accelerates_next_candidate() {
        let mut previous = point(60);
        previous.brake_distance_centi_c = 1_400;
        previous.approach_power_permille = 350;
        previous.approach_floor_power_permille = 100;
        previous.approach_damping_exponent_permille = 4_000;
        previous.hold_power_permille = 100;
        previous.hold_reheat_power_permille = 120;
        previous.hold_entry_centi_c = 320;
        previous.hold_exit_centi_c = 300;
        previous.approach_lead_ticks = 6;
        previous.hold_lead_ticks = 6;
        let mut facts = evidence(60);
        facts.curve_class = CurveClass::Underpowered;
        facts.overshoot_c = 1.05;
        facts.hold_p2p_c = 1.50;
        facts.residual_c = 3.58;
        facts.under_c = 3.17;
        facts.over_c = 0.84;
        facts.hold_sample_count = 40;
        facts.first_hold_error_c = Some(3.17);
        facts.first_band_observed = false;
        facts.deadline_temperature_centi_c = Some(5_792);

        let tuned = tune_point(previous, facts);

        assert!(tuned.brake_distance_centi_c < previous.brake_distance_centi_c);
        assert!(tuned.approach_power_permille > previous.approach_power_permille);
        assert!(tuned.approach_floor_power_permille > previous.approach_floor_power_permille);
    }

    fn high_temperature_point(target_c: i16) -> ThermalControlProfilePointConfig {
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 839,
            approach_power_permille: 667,
            approach_floor_power_permille: 600,
            approach_damping_exponent_permille: 1_834,
            approach_tail_window_centi_c: 167,
            approach_lead_ticks: 6,
            hold_power_permille: 600,
            hold_reheat_power_permille: 600,
            hold_entry_centi_c: 270,
            hold_exit_centi_c: 258,
            hold_on_centi_c: 20,
            hold_off_centi_c: 67,
            overshoot_cutoff_centi_c: 140,
            hold_kp_permille_per_c: 12,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 1,
            hold_lead_ticks: 6,
            ..point(target_c)
        }
    }

    fn high_temperature_failure(target_c: i16) -> TuneEvidence {
        TuneEvidence {
            // A slow Approach classification must not override measured high-side failure.
            curve_class: CurveClass::Underpowered,
            overshoot_c: 2.63,
            hold_p2p_c: 3.73,
            residual_c: 3.73,
            first_band_observed: true,
            first_band_exit_centi_c: Some(target_c * 100 + 151),
            settle_limit_ms: 15_000,
            ..evidence(target_c)
        }
    }

    #[test]
    fn floor_only_coast_alternatives_use_remaining_braking_without_fake_progress() {
        let previous = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_318,
            approach_damping_exponent_permille: 1_050,
            ..point(60)
        };
        let facts = TuneEvidence {
            overshoot_c: 1.7,
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_151),
            hold_sample_count: 15,
            hold_off_sample_count: 15,
            approach_floor_only: true,
            ..evidence(60)
        };
        for attempt in 0..32 {
            let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
            assert_eq!(
                alternative,
                ThermalControlProfilePointConfig {
                    brake_distance_centi_c: 1_318 + 40 * (attempt + 1),
                    ..previous
                }
            );
        }
        let saturated = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 5_000,
            ..previous
        };
        assert_eq!(tune_point(saturated, facts), saturated);
        assert_eq!(reseed_after_hold_confirm(saturated, facts), saturated);
        for attempt in 0..32 {
            assert!(bounded_failure_alternative(saturated, facts, attempt).is_none());
        }
    }

    #[test]
    fn limited_hold_high_side_feedback_requires_complete_low_output_facts() {
        let previous = point(60);
        let facts = TuneEvidence {
            overshoot_c: 2.62,
            over_c: 2.62,
            hold_max_below_c: Some(0.49),
            first_band_observed: true,
            first_band_exit_centi_c: Some(5_848),
            hold_sample_count: 22,
            hold_off_sample_count: 20,
            hold_max_output_permille: Some(60),
            approach_output_range: Some((50, 990)),
            ..evidence(60)
        };
        let tuned = limited_hold_high_side_candidate(previous, facts).unwrap();
        assert_eq!(
            tuned,
            ThermalControlProfilePointConfig {
                approach_damping_exponent_permille: 2_000,
                ..previous
            }
        );
        for unavailable in [
            TuneEvidence {
                tunable_stop: false,
                ..facts
            },
            TuneEvidence {
                target_c: 160,
                ..facts
            },
            TuneEvidence {
                stable_window_verified: true,
                ..facts
            },
            TuneEvidence {
                full_speed_failed: false,
                ..facts
            },
            TuneEvidence {
                first_band_observed: false,
                ..facts
            },
            TuneEvidence {
                hold_sample_count: 2,
                hold_off_sample_count: 2,
                ..facts
            },
            TuneEvidence {
                hold_max_output_permille: None,
                ..facts
            },
            TuneEvidence {
                hold_max_output_permille: Some(101),
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 17,
                ..facts
            },
            TuneEvidence {
                hold_max_below_c: Some(1.51),
                ..facts
            },
            TuneEvidence {
                hold_max_below_c: None,
                ..facts
            },
            TuneEvidence {
                over_c: 1.5,
                ..facts
            },
            TuneEvidence {
                approach_output_range: Some((0, 990)),
                ..facts
            },
            TuneEvidence {
                approach_output_range: Some((50, 100)),
                ..facts
            },
            TuneEvidence {
                approach_output_range: None,
                ..facts
            },
        ] {
            assert!(limited_hold_high_side_candidate(previous, unavailable).is_none());
        }
    }

    #[test]
    fn captured_coasting_high_side_feedback_changes_only_effective_damping() {
        let previous = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_553,
            hold_power_permille: 50,
            hold_reheat_power_permille: 50,
            hold_kp_permille_per_c: 21,
            ..point(60)
        };
        // Calibration-only 60C: all physical Hold samples request zero,
        // yet stored Approach heat carries the trace above the upper band.
        let facts = TuneEvidence {
            overshoot_c: 4.96,
            hold_p2p_c: 5.18,
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_161),
            hold_sample_count: 13,
            hold_off_sample_count: 13,
            ..evidence(60)
        };
        let expected = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_000,
            ..previous
        };
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
        for unavailable in [
            TuneEvidence {
                tunable_stop: false,
                ..facts
            },
            TuneEvidence {
                target_c: 160,
                ..facts
            },
            TuneEvidence {
                full_speed_failed: false,
                ..facts
            },
            TuneEvidence {
                stable_window_verified: true,
                ..facts
            },
            TuneEvidence {
                first_band_observed: false,
                ..facts
            },
            TuneEvidence {
                first_band_exit_centi_c: Some(5_840),
                ..facts
            },
            TuneEvidence {
                hold_sample_count: 2,
                hold_off_sample_count: 2,
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 12,
                ..facts
            },
        ] {
            assert!(coasting_high_side_candidate(previous, unavailable).is_none());
        }
        let saturated = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 4_000,
            ..previous
        };
        assert!(coasting_high_side_candidate(saturated, facts).is_none());
        for (overshoot_c, step) in [(1.59, 500), (2.24, 740), (20.0, 1_000)] {
            let tuned = coasting_high_side_candidate(
                previous,
                TuneEvidence {
                    overshoot_c,
                    ..facts
                },
            )
            .unwrap();
            assert_eq!(tuned.approach_damping_exponent_permille, 1_000 + step);
        }
    }

    #[test]
    fn captured_single_low_hold_entry_pulse_still_corrects_approach_heat() {
        let previous = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_553,
            approach_damping_exponent_permille: 2_000,
            approach_power_permille: 1_000,
            approach_floor_power_permille: 50,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 50,
            hold_reheat_power_permille: 50,
            hold_kp_permille_per_c: 21,
            hold_ki_permille_per_c_tick: 1,
            hold_off_centi_c: 5,
            overshoot_cutoff_centi_c: 25,
            hold_blend_ticks: 4,
            hold_lead_ticks: 4,
            ..point(60)
        };
        // ac83f9d trial 3: one 50-permille entrance sample, then fifteen
        // zero requests while stored heat carries temperature above band.
        let facts = TuneEvidence {
            overshoot_c: 1.93,
            hold_p2p_c: 2.40,
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_153),
            hold_sample_count: 16,
            hold_off_sample_count: 15,
            hold_entry_output_permille: Some(50),
            ..evidence(60)
        };
        let expected = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_500,
            ..previous
        };
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
        let alternative = bounded_failure_alternative(previous, facts, 0).unwrap();
        assert_eq!(alternative.approach_damping_exponent_permille, 2_100);
        assert_eq!(
            alternative.hold_power_permille,
            previous.hold_power_permille
        );
        for unavailable in [
            TuneEvidence {
                hold_entry_output_permille: None,
                ..facts
            },
            TuneEvidence {
                hold_entry_output_permille: Some(0),
                ..facts
            },
            TuneEvidence {
                hold_entry_output_permille: Some(51),
                ..facts
            },
            TuneEvidence {
                hold_off_sample_count: 14,
                ..facts
            },
        ] {
            assert!(coasting_high_side_candidate(previous, unavailable).is_none());
        }
    }

    #[test]
    fn high_side_trim_never_raises_calibration_only_hold_or_gain() {
        let facts = TuneEvidence {
            overshoot_c: 4.96,
            hold_p2p_c: 5.18,
            first_band_observed: true,
            first_band_exit_centi_c: Some(6_161),
            ..evidence(60)
        };
        for baseline in [40, 50, 92, 100, 150] {
            for gain in [4, 8, 21] {
                let previous = ThermalControlProfilePointConfig {
                    hold_power_permille: baseline,
                    hold_reheat_power_permille: baseline,
                    hold_kp_permille_per_c: gain,
                    ..point(60)
                };
                for next in [
                    tune_point(previous, facts),
                    reseed_after_hold_confirm(previous, facts),
                ] {
                    assert!(next.hold_power_permille <= previous.hold_power_permille);
                    assert!(next.hold_reheat_power_permille <= previous.hold_reheat_power_permille);
                    assert!(next.hold_reheat_power_permille >= next.hold_power_permille);
                    assert!(next.hold_kp_permille_per_c <= previous.hold_kp_permille_per_c);
                    assert!(next.brake_distance_centi_c > previous.brake_distance_centi_c);
                }
            }
        }
    }

    #[test]
    fn high_temperature_band_failure_uses_bounded_energy_trim_for_trial_and_confirmation() {
        for target_c in [160, 180, 220, 240] {
            let previous = high_temperature_point(target_c);
            let facts = high_temperature_failure(target_c);
            let expected = ThermalControlProfilePointConfig {
                brake_distance_centi_c: 1_139,
                approach_power_permille: 580,
                approach_floor_power_permille: 500,
                hold_power_permille: 550,
                hold_reheat_power_permille: 550,
                hold_entry_centi_c: 370,
                hold_exit_centi_c: 338,
                hold_kp_permille_per_c: 8,
                overshoot_cutoff_centi_c: 107,
                ..previous
            };
            assert_eq!(tune_point(previous, facts), expected);
            assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
            // The effective floor is still constrained by the production Hold baseline.
            assert_eq!(
                expected
                    .approach_floor_power_permille
                    .max(expected.hold_power_permille)
                    .max(100),
                550
            );
        }
    }

    #[test]
    fn high_temperature_lower_miss_keeps_acceleration_and_does_not_trim_energy() {
        let previous = high_temperature_point(160);
        let facts = TuneEvidence {
            first_band_observed: false,
            first_band_exit_centi_c: None,
            deadline_temperature_centi_c: Some(15_700),
            ..high_temperature_failure(160)
        };
        assert!(conservative_high_side_candidate(previous, facts).is_none());
        let tuned = tune_point(previous, facts);
        assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
        assert!(tuned.approach_power_permille >= previous.approach_power_permille);
    }

    #[test]
    fn high_temperature_trim_requires_high_side_and_a_failed_gate_within_scope() {
        let previous = high_temperature_point(160);
        let facts = high_temperature_failure(160);
        let no_high_side = TuneEvidence {
            first_band_exit_centi_c: None,
            deadline_temperature_centi_c: None,
            ..facts
        };
        assert!(conservative_high_side_candidate(previous, no_high_side).is_none());
        let no_thermal_failure = TuneEvidence {
            full_speed_failed: false,
            stable_window_verified: true,
            overshoot_c: 3.0,
            hold_p2p_c: 3.0,
            residual_c: 3.0,
            ..facts
        };
        assert!(conservative_high_side_candidate(previous, no_thermal_failure).is_none());
        let safety_failure = TuneEvidence {
            tunable_stop: false,
            ..facts
        };
        assert_eq!(tune_point(previous, safety_failure), previous);
        assert_eq!(
            reseed_after_hold_confirm(previous, safety_failure),
            previous
        );
        assert!(
            conservative_high_side_candidate(
                high_temperature_point(250),
                high_temperature_failure(250)
            )
            .is_none()
        );
    }

    #[test]
    fn unverified_high_temperature_upper_exit_trims_even_below_thermal_hard_limits() {
        // Captured 160C scout 3: the window breaks at 161.71C while the
        // Underpowered classification otherwise raises the floor 700 -> 760.
        let previous = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 801,
            approach_floor_power_permille: 700,
            approach_power_permille: 780,
            hold_power_permille: 449,
            hold_reheat_power_permille: 494,
            ..high_temperature_point(160)
        };
        for target_c in [160, 180, 220, 240] {
            let previous = ThermalControlProfilePointConfig {
                target_temp_c: target_c,
                ..previous
            };
            let facts = TuneEvidence {
                curve_class: CurveClass::Underpowered,
                overshoot_c: 1.60,
                hold_p2p_c: 1.67,
                residual_c: 1.67,
                first_band_observed: true,
                first_band_exit_centi_c: Some(target_c * 100 + 171),
                ..evidence(target_c)
            };
            for tuned in [
                tune_point(previous, facts),
                reseed_after_hold_confirm(previous, facts),
            ] {
                assert_eq!(tuned.approach_floor_power_permille, 600);
                assert_eq!(tuned.approach_power_permille, 680);
                assert_eq!(tuned.hold_power_permille, 399);
                assert_eq!(tuned.hold_reheat_power_permille, 444);
                assert_eq!(tuned.brake_distance_centi_c, 1101);
            }
            let missed_upper = TuneEvidence {
                first_band_observed: false,
                first_band_exit_centi_c: None,
                deadline_temperature_centi_c: Some(target_c * 100 + 171),
                ..facts
            };
            assert!(tune_point(previous, missed_upper).approach_floor_power_permille < 700);
            let verified = TuneEvidence {
                full_speed_failed: false,
                stable_window_verified: true,
                ..facts
            };
            assert!(conservative_high_side_candidate(previous, verified).is_none());
        }
        let low_target = TuneEvidence {
            first_band_observed: true,
            first_band_exit_centi_c: Some(10_171),
            overshoot_c: 1.71,
            hold_p2p_c: 1.71,
            ..evidence(100)
        };
        assert!(conservative_high_side_candidate(point(100), low_target).is_none());
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
    fn completed_small_hold_ripple_uses_trim_without_approach_acceleration() {
        let mut previous = point(60);
        previous.hold_kp_permille_per_c = 8;
        previous.hold_blend_ticks = 1;
        let mut facts = evidence(60);
        facts.full_speed_failed = false;
        facts.scout_hold_complete = true;
        facts.stable_window_verified = true;
        facts.hold_sample_count = 35;
        facts.overshoot_c = 1.41;
        facts.hold_p2p_c = 3.58;
        let tuned = tune_point(previous, facts);
        assert_eq!(tuned.hold_blend_ticks, 2);
        assert_eq!(tuned.hold_kp_permille_per_c, 8);
        assert_eq!(
            tuned.brake_distance_centi_c,
            previous.brake_distance_centi_c
        );
        assert_eq!(
            tuned.approach_power_permille,
            previous.approach_power_permille
        );

        facts.stable_window_verified = false;
        assert_ne!(tune_point(previous, facts), tuned);
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
