use super::*;

fn point(floor: u16, hold: u16) -> ThermalControlProfilePointConfig {
    ThermalControlProfilePointConfig {
        target_temp_c: 60,
        approach_floor_power_permille: floor,
        hold_power_permille: hold,
        warmup_power_permille: 1_000,
        warmup_reenter_centi_c: 1_000,
        brake_distance_centi_c: 1_400,
        approach_power_permille: 350,
        approach_damping_exponent_permille: 4_000,
        approach_tail_window_centi_c: 375,
        hold_reheat_power_permille: 120,
        hold_entry_centi_c: 320,
        hold_exit_centi_c: 300,
        hold_on_centi_c: 30,
        hold_off_centi_c: 50,
        overshoot_cutoff_centi_c: 90,
        hold_kp_permille_per_c: 8,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 1,
        approach_lead_ticks: 6,
        hold_lead_ticks: 6,
    }
}

#[test]
fn extension_requires_both_logical_floor_and_original_timeout() {
    let mut timing = ThermalTuningTimingEvidence::for_point(point(100, 60));
    timing.warmup_exited_at_ms = Some(1_000);
    assert!(!timing.try_extend(11_000, true));
    timing.observe_approach_request(5_000, 100);
    assert!(!timing.try_extend(10_000, false));
    assert_eq!(timing.active_limit_ms(), 10_000);
    assert!(timing.try_extend(11_000, true));
    assert_eq!(timing.active_limit_ms(), 30_000);
    assert!(timing.valid_extension());
    assert!(timing.try_extend(25_000, true));
    assert_eq!(
        timing.extended_at_ms,
        Some(11_000),
        "never reset the origin"
    );
}

#[test]
fn higher_floor_and_protective_zero_cannot_create_extension_evidence() {
    for (floor, request) in [(160, 160), (100, 0), (100, 140)] {
        let mut timing = ThermalTuningTimingEvidence::for_point(point(floor, 60));
        timing.observe_approach_request(5_000, request);
        assert!(!timing.try_extend(11_000, true));
        assert_eq!(timing.active_limit_ms(), 10_000);
    }
}

#[test]
fn a_previous_floor_touch_does_not_excuse_later_higher_approach_requests() {
    let mut timing = ThermalTuningTimingEvidence::for_point(point(100, 60));
    timing.observe_approach_request(5_000, 100);
    timing.observe_approach_request(8_000, 150);
    assert!(!timing.try_extend(11_000, true));
}

#[test]
fn protective_zero_preserves_prior_floor_evidence_without_creating_it() {
    let mut timing = ThermalTuningTimingEvidence::for_point(point(100, 60));
    timing.warmup_exited_at_ms = Some(1_000);
    timing.observe_approach_request(5_000, 100);
    timing.observe_approach_request(11_000, 0);
    assert_eq!(timing.floor_observed_at_ms, Some(5_000));
    assert!(timing.try_extend(11_000, true));
    assert!(timing.valid_extension());

    let mut never_at_floor = ThermalTuningTimingEvidence::for_point(point(100, 60));
    never_at_floor.observe_approach_request(11_000, 0);
    assert!(!never_at_floor.try_extend(11_000, true));

    let mut raised = ThermalTuningTimingEvidence::for_point(point(100, 60));
    raised.observe_approach_request(5_000, 100);
    raised.observe_approach_request(8_000, 150);
    raised.observe_approach_request(11_000, 0);
    assert!(!raised.try_extend(11_000, true));
}

#[test]
fn captured_80c_deadline_zero_keeps_the_actual_floor_witness() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/thermal-80c-floor-zero-replay.json"
    ))
    .unwrap();
    let recorded: ThermalTuningTimingEvidence =
        serde_json::from_value(fixture["timing"].clone()).unwrap();
    let mut candidate = point(163, 163);
    candidate.target_temp_c = 80;
    let mut timing = ThermalTuningTimingEvidence::for_point(candidate);
    timing.warmup_exited_at_ms = recorded.warmup_exited_at_ms;
    let mut last_elapsed = 0;
    for row in fixture["samples"].as_array().unwrap() {
        last_elapsed = row[1].as_u64().unwrap();
        if row[2] == "approach" {
            timing.observe_approach_request(last_elapsed, row[4].as_u64().unwrap() as u16);
        }
    }
    assert_eq!(last_elapsed, 2_590_001);
    assert!(recorded.floor_observed_at_ms.is_none());
    assert!(timing.floor_observed_at_ms.is_some());
    assert!(timing.try_extend(last_elapsed, true));
    assert!(timing.valid_extension());
    assert_eq!(timing.active_limit_ms(), 30_000);
    assert_eq!(timing.warmup_exited_at_ms, recorded.warmup_exited_at_ms);
}

#[test]
fn effective_floor_preserves_the_production_hold_baseline_and_rounding() {
    let mut timing = ThermalTuningTimingEvidence::for_point(point(135, 135));
    assert_eq!(timing.allowed_approach_floor_permille, 135);
    assert_eq!(timing.effective_approach_floor_permille, 135);
    timing.observe_approach_request(5_000, 140);
    assert!(timing.try_extend(11_000, true));
    let mut above = ThermalTuningTimingEvidence::for_point(point(160, 135));
    above.observe_approach_request(5_000, 140);
    assert!(!above.try_extend(11_000, true));
}

#[test]
fn evidence_cannot_claim_an_unbounded_or_premature_exception() {
    let mut timing = ThermalTuningTimingEvidence::for_point(point(100, 60));
    timing.warmup_exited_at_ms = Some(1_000);
    timing.observe_approach_request(5_000, 100);
    timing.try_extend(11_000, true);
    assert!(timing.valid_extension());
    timing.extension_limit_ms = 180_000;
    assert!(!timing.valid_extension());
    timing.extension_limit_ms = 30_000;
    timing.extended_at_ms = Some(10_999);
    assert!(!timing.valid_extension());
}

#[test]
fn high_temperature_exception_remains_bounded_and_separate_from_trial_timeout() {
    let mut high = point(100, 60);
    high.target_temp_c = 240;
    let timing = ThermalTuningTimingEvidence::for_point(high);
    assert_eq!(timing.original_limit_ms, 5_000);
    assert_eq!(timing.extension_limit_ms, 15_000);
    assert_eq!(TRIAL_TIMEOUT_MS, 180_000);
}
