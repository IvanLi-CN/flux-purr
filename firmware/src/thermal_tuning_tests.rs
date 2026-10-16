#[test]
fn no_op_and_cyclic_candidate_proposals_are_not_reheated() {
    let mut runtime = started_single_target_runtime();
    let original = runtime.candidate_profile.unwrap();
    assert!(!runtime.record_fresh_proposal(Some(original)));
    let mut changed = original;
    changed.points[0].as_mut().unwrap().hold_kp_permille_per_c += 1;
    runtime.candidate_profile = Some(changed);
    assert!(runtime.record_fresh_proposal(Some(original)));
    runtime.candidate_profile = Some(original);
    assert!(!runtime.record_fresh_proposal(Some(changed)));
    runtime.exhaust_candidate_search(60).unwrap();
    assert!(!runtime.armed);
    assert!(runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("candidate_search_exhausted_no_distinct_safe_proposal")));
}

use super::*;
use crate::memory::{ThermalControlProfilePointConfig, ThermalControlProfileSettingsConfig};

#[test]
fn captured_guarded_deadline_cannot_create_a_low_side_bracket() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_guarded_deadline.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    for (index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        if index == 0 {
            assert_eq!(damping_failure_side(facts), Some(true));
            runtime.remember_damping_failure(60, facts);
        } else {
            assert_eq!(runtime.deadline_nearest_distance_ms, 5_020);
            assert_eq!(facts.deadline_temperature_centi_c, None, "{facts:?}");
            assert_eq!(damping_failure_side(facts), None, "{facts:?}");
            runtime.remember_damping_failure(60, facts);
            assert_eq!(runtime.observed_damping_failures.len(), 1);
            assert!(!runtime.prepare_damping_bracket(runtime.candidate_profile.unwrap()));
        }
    }
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn deadline_witness_requires_a_nearby_accepted_sample() {
    for (elapsed_ms, fresh, expected) in [
        (9_000, true, Some(5_700)),
        (11_000, true, Some(5_700)),
        (8_999, true, None),
        (11_001, true, None),
        (10_000, false, None),
    ] {
        let mut runtime = started_single_target_runtime();
        runtime.warmup_exited_at_ms = Some(0);
        let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_700);
        observed.measurement_accepted = Some(fresh);
        runtime.update_trial_metrics(&observed, 60);
        assert_eq!(runtime.deadline_temperature_evidence(), expected);
    }
}

#[test]
fn captured_timely_hold_with_late_pps_window_keeps_searching_approach() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_timely_hold_late_window.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        10_000,
    );
    assert_eq!(facts.first_hold_after_warmup_ms, Some(9_659));
    assert_eq!(facts.stable_window_after_warmup_ms, Some(10_075));
    assert_eq!(facts.hold_sample_count, 2);
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(60, facts),
        "{facts:?}"
    );
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: previous.brake_distance_centi_c - 40,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

fn replay_damping_bracket_trial(
    runtime: &mut ThermalTuningRuntime,
    trial: &serde_json::Value,
) -> TrialEvaluation {
    runtime.reset_trial_metrics();
    let wire: ThermalTuningCandidatePoint = serde_json::from_value(trial["point"].clone()).unwrap();
    let mut profile = profile_with_targets(&[wire.target_c]);
    let point = profile.points[0].as_mut().unwrap();
    macro_rules! copy_fields {
        ($($field:ident),+ $(,)?) => { $(point.$field = wire.$field;)+ };
    }
    copy_fields!(
        brake_distance_centi_c,
        warmup_power_permille,
        warmup_reenter_centi_c,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks
    );
    runtime
        .install_candidate_profile(profile, "fixture_replay")
        .unwrap();
    runtime.trial_index = trial["trialIndex"].as_u64().unwrap() as u8;
    for row in trial["samples"].as_array().unwrap() {
        let mut observed = sample(
            row[0].as_u64().unwrap(),
            serde_json::from_value(row[2].clone()).unwrap(),
            row[1].as_i64().unwrap() as i16,
        );
        observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
        observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
        observed.measurement_accepted = row[5].as_bool();
        if let Some(requested_mv) = row.get(6).and_then(serde_json::Value::as_u64) {
            observed.requested_mv = requested_mv as u16;
        }
        observed.raw_temperature_centi_c = row
            .get(7)
            .and_then(serde_json::Value::as_i64)
            .map(|value| value as i16);
        runtime.current_elapsed_ms = observed.elapsed_ms;
        observed.target_c = wire.target_c;
        runtime.update_trial_metrics(&observed, wire.target_c);
    }
    let decision = &trial["decision"];
    TrialEvaluation {
        score: serde_json::from_value(decision["score"].clone()).unwrap(),
        passes: false,
        warmup_full: decision["warmupFull"].as_bool().unwrap(),
        max_overshoot_centi_c: decision["maxOvershootCentiC"].as_u64().map(|v| v as u16),
        hold_peak_to_peak_centi_c: decision["holdPeakToPeakCentiC"].as_u64().map(|v| v as u16),
        settle_time_ms: decision["settleTimeMs"].as_u64(),
        full_speed_limit_ms: decision["hardGateFullSpeedToStableMs"].as_u64().unwrap() as u32,
    }
}

#[test]
fn captured_coarse_braking_boundary_retains_an_effective_search_direction() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_start_boundary.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        10_000,
    );
    assert_eq!(previous.brake_distance_centi_c, 2_461);
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(60, facts),
        "{facts:?}"
    );
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_000,
            ..previous
        }
    );
    for rejected in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            approach_floor_only: true,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 1_000)),
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 0,
            ..facts
        },
    ] {
        assert!(braking_boundary_damping_candidate(previous, rejected).is_none());
    }
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_confirmed_window_hold_ripple_retains_search_feedback() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_start_boundary.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert_eq!(reseed_after_hold_confirm(previous, facts), previous);
    let expected = tune_point(previous, facts);
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}"
    );
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next, expected);
    assert!(!same_effective_search_point(previous, next));
    assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
    assert_eq!(
        next.approach_damping_exponent_permille,
        previous.approach_damping_exponent_permille
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_opposite_response_horizons_explore_the_measured_interval() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_horizon_bracket.fixture.json")).unwrap();
    let trials = fixture["trials"].as_array().unwrap();
    for order in [[0, 1], [1, 0]] {
        let mut runtime = started_single_target_runtime();
        let mut last_facts = None;
        let mut high_point = None;
        for index in order {
            let evaluation = replay_damping_bracket_trial(&mut runtime, &trials[index]);
            let facts = runtime.candidate_failure_evidence(
                60,
                evaluation.max_overshoot_centi_c,
                evaluation.hold_peak_to_peak_centi_c,
                evaluation.settle_time_ms,
                10_000,
            );
            assert_eq!(damping_failure_side(facts), Some(index == 0), "{facts:?}");
            if index == 0 {
                high_point = runtime.candidate_profile.unwrap().points[0];
            }
            runtime.remember_damping_failure(60, facts);
            last_facts = Some(facts);
        }
        assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(
            next,
            ThermalControlProfilePointConfig {
                approach_lead_ticks: 1,
                ..high_point.unwrap()
            }
        );
        assert_eq!(runtime.accepted_target_mask, 0);
    }
}

#[test]
fn captured_narrow_damping_bracket_at_minimum_horizon_explores_braking() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_narrow_horizon.fixture.json")).unwrap();
    let trials = fixture["trials"].as_array().unwrap();
    for order in [[0, 1, 2], [2, 0, 1]] {
        let mut runtime = started_single_target_runtime();
        let mut last_facts = None;
        let mut high_point = None;
        for trial_index in order {
            let evaluation = replay_damping_bracket_trial(&mut runtime, &trials[trial_index]);
            if trial_index == 2 {
                high_point = runtime.candidate_profile.unwrap().points[0];
            }
            let facts = runtime.candidate_failure_evidence(
                60,
                evaluation.max_overshoot_centi_c,
                evaluation.hold_peak_to_peak_centi_c,
                evaluation.settle_time_ms,
                10_000,
            );
            assert!(damping_failure_side(facts).is_some(), "{facts:?}");
            runtime.remember_damping_failure(60, facts);
            last_facts = Some(facts);
        }
        let high_point = high_point.unwrap();
        assert_eq!(high_point.approach_lead_ticks, 0);
        assert_eq!(high_point.approach_damping_exponent_permille, 1_577);
        assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
        assert_eq!(
            runtime.candidate_profile.unwrap().points[0].unwrap(),
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: high_point.brake_distance_centi_c + 40,
                ..high_point
            }
        );
        assert_eq!(runtime.accepted_target_mask, 0);
    }
}

#[test]
fn captured_narrow_damping_bracket_near_start_gap_uses_a_legal_braking_step() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_narrow_boundary.fixture.json")).unwrap();
    let trials = fixture["trials"].as_array().unwrap();
    for order in [[0, 1], [1, 0]] {
        let mut runtime = started_single_target_runtime();
        let mut last_facts = None;
        let mut high_point = None;
        for trial_index in order {
            let evaluation = replay_damping_bracket_trial(&mut runtime, &trials[trial_index]);
            if trial_index == 1 {
                high_point = runtime.candidate_profile.unwrap().points[0];
            }
            let facts = runtime.candidate_failure_evidence(
                60,
                evaluation.max_overshoot_centi_c,
                evaluation.hold_peak_to_peak_centi_c,
                evaluation.settle_time_ms,
                10_000,
            );
            assert!(damping_failure_side(facts).is_some(), "{facts:?}");
            runtime.remember_damping_failure(60, facts);
            last_facts = Some(facts);
        }
        let high_point = high_point.unwrap();
        assert_eq!(high_point.brake_distance_centi_c, 2_476);
        assert_eq!(high_point.approach_damping_exponent_permille, 1_218);
        assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
        assert_eq!(
            runtime.candidate_profile.unwrap().points[0].unwrap(),
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 2_496,
                ..high_point
            }
        );
        assert_eq!(runtime.accepted_target_mask, 0);
    }
}

#[test]
fn captured_low_supply_hold_failure_changes_only_approach_damping() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_low_supply_hold.fixture.json")).unwrap();
    for trial in fixture["trials"].as_array().unwrap() {
        let mut runtime = started_single_target_runtime();
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        assert!(facts.hold_sample_count >= 3, "{facts:?}");
        assert!(facts.hold_max_output_permille.unwrap() <= 100, "{facts:?}");
        assert!(facts.hold_p90_output_permille.unwrap() <= 50, "{facts:?}");
        assert!(facts.over_c > 1.5, "{facts:?}");
        assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, facts));
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(
            next.approach_damping_exponent_permille > previous.approach_damping_exponent_permille
        );
        assert_eq!(
            next,
            ThermalControlProfilePointConfig {
                approach_damping_exponent_permille: next.approach_damping_exponent_permille,
                ..previous
            },
            "trial {}: {facts:?}",
            trial["trialIndex"]
        );
        assert_eq!(runtime.accepted_target_mask, 0);
    }
}

#[test]
fn captured_small_high_side_failure_uses_the_contracted_damping_step() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_small_high_side_step.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        10_000,
    );
    assert_eq!(evaluation.max_overshoot_centi_c, Some(158));
    assert_eq!(previous.approach_damping_exponent_permille, 1_000);
    assert_eq!(previous.brake_distance_centi_c, 2_471);
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, facts));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 1_500,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_late_stability_failure_refines_the_same_run_braking_interval() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_late_stability_bracket.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let mut last_facts = None;
    for (index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        if index == 0 {
            assert_eq!(facts.stable_window_after_warmup_ms, Some(10_012));
            assert!(facts.hold_sample_count >= 2);
            assert_eq!(facts.first_band_exit_centi_c, None);
            assert!(!evaluation.passes);
        }
        runtime.remember_damping_failure(60, facts);
        last_facts = Some(facts);
    }
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(previous.brake_distance_centi_c, 2_426);
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_446,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn captured_guarded_timely_handoff_retains_the_braking_interval() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_guarded_handoff.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut high_point = None;
    let mut last_facts = None;
    for (index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        if index == 0 {
            high_point = runtime.candidate_profile.unwrap().points[0];
            assert_eq!(damping_failure_side(facts), Some(true));
        } else if index == 1 {
            assert_eq!(facts.hold_sample_count, 1);
            assert_eq!(facts.first_hold_after_warmup_ms, Some(9_932));
            assert!(facts.guarded_hold_handoff);
            assert_eq!(facts.stable_window_after_warmup_ms, None);
            assert_eq!(facts.deadline_temperature_centi_c, Some(5_951));
            assert_eq!(facts.first_band_exit_centi_c, None);
            assert!(facts.first_band_observed);
        }
        runtime.remember_damping_failure(60, facts);
        last_facts = Some(facts);
    }
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_834,
            ..high_point.unwrap()
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn guarded_handoff_feedback_requires_fresh_timely_in_band_observations() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_guarded_handoff.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let trial = &fixture["trials"][1];
    let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        None,
        10_000,
    );
    assert!(interrupted_stability_handoff_failure(facts));
    for rejected in [
        TuneEvidence {
            guarded_hold_handoff: false,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            stable_window_after_warmup_ms: Some(9_900),
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 0,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(10_001),
            ..facts
        },
        TuneEvidence {
            deadline_temperature_centi_c: None,
            ..facts
        },
        TuneEvidence {
            deadline_temperature_centi_c: Some(5_949),
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(6_151),
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 540)),
            ..facts
        },
        TuneEvidence {
            approach_floor_only: true,
            ..facts
        },
        TuneEvidence {
            target_c: 160,
            deadline_temperature_centi_c: Some(15_951),
            ..facts
        },
    ] {
        assert!(!interrupted_stability_handoff_failure(rejected));
        assert_ne!(damping_failure_side(rejected), Some(false));
    }
    let mut spike = trial.clone();
    spike["samples"].as_array_mut().unwrap().last_mut().unwrap()[7] = 7_000.into();
    replay_damping_bracket_trial(&mut runtime, &spike);
    assert!(!runtime.guarded_hold_handoff);
    runtime.reset_trial_metrics();
    assert!(!runtime.guarded_hold_handoff);
}

#[test]
fn captured_braking_errors_refine_the_same_run_interval() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_weighted_braking.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut facts = None;
    for trial in fixture["trials"].as_array().unwrap().iter().take(2) {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let observed = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        runtime.remember_damping_failure(60, observed);
        facts = Some(observed);
    }
    assert_eq!(facts.unwrap().deadline_temperature_centi_c, Some(5_900));
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, facts.unwrap()));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_302,
            ..previous
        }
    );
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][2]);
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        10_000,
    );
    assert_eq!(facts.over_c, 1.53);
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, facts));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_418,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_late_stability_proposal_prefers_the_existing_failed_interval() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_late_stability_bracket.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let mut last_facts = None;
    for index in [1, 0] {
        let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][index]);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        runtime.remember_damping_failure(60, facts);
        last_facts = Some(facts);
    }
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(previous.brake_distance_centi_c, 2_466);
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_446,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_late_stability_failure_requires_real_timely_hold_and_no_band_exit() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_late_stability_bracket.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        10_000,
    );
    assert_eq!(damping_failure_side(facts), Some(false));
    assert_eq!(braking_failure_weight(facts, false), None);
    for invalid in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 1,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(6_152),
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: None,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(10_001),
            ..facts
        },
        TuneEvidence {
            stable_window_after_warmup_ms: None,
            ..facts
        },
        TuneEvidence {
            stable_window_after_warmup_ms: Some(10_000),
            ..facts
        },
    ] {
        assert!(!late_stability_handoff_failure(invalid), "{invalid:?}");
    }
    assert_eq!(
        damping_failure_side(TuneEvidence {
            approach_output_range: Some((0, 1_000)),
            ..facts
        }),
        None
    );
}

#[test]
fn duplicate_weighted_braking_proposal_uses_the_remaining_midpoint() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_weighted_braking.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    for trial in fixture["trials"].as_array().unwrap().iter().take(2) {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        runtime.remember_damping_failure(60, facts);
    }
    let original = runtime.candidate_profile.unwrap();
    assert!(runtime.prepare_failure_bracket(original, FailureBracketAxis::Braking));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0]
            .unwrap()
            .brake_distance_centi_c,
        2_302
    );
    assert!(runtime.prepare_failure_bracket(original, FailureBracketAxis::Braking));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0]
            .unwrap()
            .brake_distance_centi_c,
        2_147
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn weighted_braking_remains_inside_the_measured_interval() {
    let mut high = ObservedDampingFailure {
        point: ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_000,
            ..profile_with_targets(&[60]).points[0].unwrap()
        },
        trial_index: 0,
        high_side: true,
        floor_only: false,
        braking_error_centi_c: Some(100),
    };
    let mut low = ObservedDampingFailure {
        point: ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_000,
            ..high.point
        },
        trial_index: 1,
        high_side: false,
        floor_only: false,
        braking_error_centi_c: Some(100),
    };
    for (high_error, low_error, expected) in [
        (100, 100, 1_500),
        (u16::MAX, 1, 1_750),
        (1, u16::MAX, 1_250),
    ] {
        high.braking_error_centi_c = Some(high_error);
        low.braking_error_centi_c = Some(low_error);
        assert_eq!(weighted_braking_value(high, low), Some(expected));
    }
    low.braking_error_centi_c = None;
    assert_eq!(weighted_braking_value(high, low), None);
    low.braking_error_centi_c = Some(0);
    assert_eq!(weighted_braking_value(high, low), None);
    low.braking_error_centi_c = Some(100);
    low.point.brake_distance_centi_c = 1_001;
    assert_eq!(weighted_braking_value(high, low), None);
}

#[test]
fn captured_narrow_damping_interval_searches_the_approach_horizon() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_high_temperature_feedback.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    for trial in fixture["trials"].as_array().unwrap().iter().skip(2) {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        runtime.remember_damping_failure(60, facts);
    }
    let original = runtime.candidate_profile.unwrap();
    assert!(runtime.prepare_damping_bracket(original));
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next.approach_lead_ticks, 3);
    assert_eq!(next.approach_damping_exponent_permille, 1_464);
    assert_eq!(
        next.brake_distance_centi_c,
        original.points[0].unwrap().brake_distance_centi_c
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_physical_handoff_deficit_does_not_mutate_unused_hold_fields() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_high_temperature_feedback.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert_eq!(facts.hold_sample_count, 0, "{facts:?}");
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}"
    );
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 60,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_timed_hold_prefix_cannot_exhaust_on_a_sample_count_boundary() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_short_timed_hold.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert_eq!(facts.hold_sample_count, 24);
    assert_eq!(facts.first_band_exit_centi_c, None);
    assert_eq!(facts.hold_pps_wait_interruptions, 1);
    let original = runtime.candidate_profile.unwrap();
    for unavailable in [
        TuneEvidence {
            hold_sample_count: 11,
            ..facts
        },
        TuneEvidence {
            hold_pps_wait_interruptions: 0,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(21849),
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
    ] {
        assert!(!runtime.prepare_short_approach_hold_handoff(original, 220, unavailable));
    }
    let ended_at = runtime.current_elapsed_ms;
    runtime.current_elapsed_ms = runtime.first_controller_hold_elapsed_ms.unwrap() + 1_999;
    assert!(!runtime.prepare_short_approach_hold_handoff(original, 220, facts));
    runtime.current_elapsed_ms = ended_at;
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}",
    );
    assert_ne!(runtime.candidate_profile, Some(original));
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn captured_short_approach_preserves_observed_hold_search() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_short_approach_hold.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert_eq!(facts.approach_output_range, None);
    assert!(facts.hold_sample_count >= 30, "{facts:?}");
    assert_eq!(facts.first_band_exit_centi_c, None);
    assert_eq!(facts.hold_pps_wait_interruptions, 1);
    assert!(facts.over_c > facts.under_c + 0.5);
    let original = runtime.candidate_profile.unwrap();
    for invalid in [
        TuneEvidence {
            hold_pps_wait_interruptions: 0,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 11,
            ..facts
        },
        TuneEvidence {
            over_c: 1.51,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(22151),
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(5_001),
            ..facts
        },
        TuneEvidence {
            under_c: 1.4,
            over_c: 0.8,
            ..facts
        },
    ] {
        assert!(!runtime.prepare_short_approach_hold_handoff(original, 220, invalid));
        assert_eq!(runtime.candidate_profile, Some(original));
    }
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}"
    );
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: previous.brake_distance_centi_c + 40,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn captured_in_band_interruption_keeps_a_distinct_handoff_proposal() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_in_band_interruption.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert!(!evaluation.passes);
    assert!(facts.hold_sample_count >= 30, "{facts:?}");
    assert_eq!(facts.first_band_exit_centi_c, None);
    assert_eq!(facts.stable_window_after_warmup_ms, None);
    for invalid in [
        TuneEvidence {
            under_c: 1.51,
            ..facts
        },
        TuneEvidence {
            over_c: 1.51,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(21849),
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 29,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 340)),
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(5_001),
            ..facts
        },
        TuneEvidence {
            deadline_temperature_centi_c: None,
            ..facts
        },
    ] {
        assert!(!in_band_interrupted_stability_failure(invalid));
    }
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}"
    );
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 60,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn captured_in_band_hold_wait_proposes_a_measured_baseline_instead_of_exhausting() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_high_temperature_feedback.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        220,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        evaluation.settle_time_ms,
        5_000,
    );
    assert!(facts.hold_sample_count >= 30, "{facts:?}");
    assert!(!facts.stable_window_verified);
    assert_eq!(facts.hold_pps_wait_interruptions, 1);
    let original = runtime.candidate_profile.unwrap();
    for unavailable in [
        TuneEvidence {
            hold_pps_wait_interruptions: 0,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 29,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(5_001),
            ..facts
        },
        TuneEvidence {
            under_c: 1.51,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
    ] {
        assert!(!runtime.prepare_in_band_hold_baseline(original, 220, unavailable));
        assert_eq!(runtime.candidate_profile, Some(original));
    }
    assert!(
        runtime.prepare_fresh_failure_proposal_from_evidence(220, facts),
        "{facts:?}"
    );
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(next.hold_power_permille < previous.hold_power_permille);
    assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
    assert_eq!(
        next.approach_damping_exponent_permille,
        previous.approach_damping_exponent_permille
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    runtime.reset_trial_metrics();
    assert_eq!(runtime.hold_pps_wait_interruptions, 0);
}

#[test]
fn captured_late_hold_entry_preserves_the_same_run_damping_bracket() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_late_hold_bracket.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let mut first = None;
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        first.get_or_insert(runtime.candidate_profile.unwrap().points[0].unwrap());
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        if trial["trialIndex"] == 4 {
            assert_eq!(facts.hold_sample_count, 1);
            assert_eq!(facts.first_hold_after_warmup_ms, Some(10_112));
            assert_eq!(damping_failure_side(facts), Some(false), "{facts:?}");
            for invalid in [
                TuneEvidence {
                    hold_sample_count: 3,
                    ..facts
                },
                TuneEvidence {
                    first_hold_after_warmup_ms: Some(9_999),
                    ..facts
                },
                TuneEvidence {
                    stable_window_after_warmup_ms: Some(9_999),
                    ..facts
                },
                TuneEvidence {
                    overshoot_c: 0.51,
                    ..facts
                },
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                },
            ] {
                assert_eq!(damping_failure_side(invalid), None);
            }
        }
        runtime.advance_after_trial(60, evaluation).unwrap();
    }
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 1_445,
            ..first.unwrap()
        }
    );
    assert!(runtime.armed);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("same_run_damping_bracket_trials_1_4_midpoint_1445")));
}

#[test]
fn captured_warmup_timeout_cannot_outrank_measured_approach_progress() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_warmup_rank.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 220;
    let mut observed_scores = std::vec::Vec::new();
    for trial in fixture["trials"].as_array().unwrap() {
        let mut evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        evaluation.score = runtime.trial_score(
            220,
            false,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
        );
        observed_scores.push(evaluation.score);
    }
    assert!(
        score_precedes(&observed_scores[1], &observed_scores[0]),
        "{observed_scores:?}"
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    // Historical score vectors remain comparable. A real unobserved
    // Warmup timeout now follows the separate preparation-retry path,
    // rather than manufacturing an Approach parameter comparison.
}

#[test]
fn cold_start_warmup_timeout_retries_the_unchanged_point_within_budget() {
    for power_class in [
        ThermalTuningPowerClass::Pps3a,
        ThermalTuningPowerClass::Pps5a,
    ] {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-cold-warmup"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class,
                targets: &[220],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[220])),
            })
            .unwrap();
        let original = runtime.candidate_profile;
        let original_hash = runtime.candidate_hash.clone();
        let mut elapsed_ms = 0;
        // Captured B run 1791950156245: both 180 s trials end during
        // full-output Warmup. Their different starting temperatures are
        // physical preparation evidence, not Approach parameter feedback.
        for (start, end) in [(5_984, 18_704), (18_680, 21_624)] {
            let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Cooldown, start);
            cooling.target_c = 220;
            cooling.trial_index = runtime.trial_index;
            cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
            cooling.pps_contract.kind = power_class;
            cooling.pps_contract.current_ma = power_class.required_current_ma();
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_sample(cooling).unwrap();
            let mut heating = sample(elapsed_ms + 1, ThermalTuningPhase::Warmup, start);
            heating.target_c = 220;
            heating.trial_index = runtime.trial_index;
            heating.candidate_hash = runtime.candidate_hash.clone().unwrap();
            heating.pps_contract = ThermalTuningPpsContract {
                kind: power_class,
                current_ma: power_class.required_current_ma(),
                ..heating.pps_contract
            };
            runtime.record_sample(heating.clone()).unwrap();
            elapsed_ms += TRIAL_TIMEOUT_MS + 1;
            heating.elapsed_ms = elapsed_ms;
            heating.temperature_centi_c = end;
            runtime.record_sample(heating).unwrap();
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
            assert_eq!(runtime.candidate_profile, original);
            assert_eq!(runtime.candidate_hash, original_hash);
            assert!(runtime.proposed_points.is_empty());
            assert!(runtime.armed);
            assert_eq!(runtime.accepted_target_mask, 0);
            assert_eq!(runtime.failed_target_mask, 0);
            assert!(
                runtime.events.iter().any(|event| event.reason.as_deref()
                    == Some("warmup_timeout_retry_from_warmer_start"))
            );
            assert!(
                runtime
                    .events
                    .iter()
                    .filter_map(|event| event.decision.as_ref())
                    .any(|decision| {
                        decision.stop_reason == "trial stage timeout"
                            && decision.disposition == "rejected"
                    })
            );
            elapsed_ms += 1;
        }
    }
}

#[test]
fn unobserved_warmup_timeout_has_no_inactive_stage_parameter_feedback() {
    for (start, end, fresh, full, origin) in [
        (20_000, 21_624, true, true, 0),
        (18_680, 18_800, true, true, 0),
        (18_680, 21_624, false, true, 0),
        (18_680, 21_624, true, false, 0),
        (18_680, 21_624, true, true, THERMAL_TUNING_TARGET_BUDGET_MS),
    ] {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-unobserved-warmup"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[220],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[220])),
            })
            .unwrap();
        let original = runtime.candidate_profile;
        let original_hash = runtime.candidate_hash.clone();
        let mut cooling = sample(0, ThermalTuningPhase::Cooldown, start);
        cooling.target_c = 220;
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling).unwrap();
        let mut heating = sample(origin + 1, ThermalTuningPhase::Warmup, start);
        heating.target_c = 220;
        runtime.record_sample(heating.clone()).unwrap();
        if runtime.armed {
            heating.elapsed_ms += TRIAL_TIMEOUT_MS;
            heating.temperature_centi_c = end;
            heating.measurement_accepted = Some(fresh);
            if !full {
                heating.heater_output_permille = 500;
            }
            runtime.record_sample(heating).unwrap();
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
        assert!(!runtime.armed);
        assert_eq!(runtime.candidate_profile, original);
        assert_eq!(runtime.candidate_hash, original_hash);
        assert_eq!(runtime.accepted_target_mask, 0);
        assert!(
            !runtime
                .events
                .iter()
                .any(|event| event.reason.as_deref()
                    == Some("warmup_timeout_retry_from_warmer_start"))
        );
    }
}

#[test]
fn captured_calibrated_braking_keeps_other_effective_parameters_fixed() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_calibrated_braking.fixture.json"
    ))
    .unwrap();
    let trial = &fixture["trials"][2];
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 100;
    let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        100,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        None,
        10_000,
    );
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(100, facts));
    assert_eq!(
        runtime.candidate_profile.unwrap().points[0].unwrap(),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_226,
            ..previous
        },
        "{facts:?}"
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_calibrated_braking_brackets_a_floor_only_high_endpoint() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_calibrated_braking.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    runtime.targets_c[0] = 80;
    let mut last_facts = None;
    for trial in &fixture["trials"].as_array().unwrap()[..2] {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            80,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        if trial["trialIndex"] == 3 {
            assert!(facts.approach_floor_only);
            assert_eq!(facts.approach_output_range, Some((50, 50)));
            assert_eq!(bracket_failure_side(facts), Some(true));
            assert_eq!(damping_failure_side(facts), None);
            assert_eq!(
                bracket_failure_side(TuneEvidence {
                    approach_output_range: Some((50, 60)),
                    ..facts
                }),
                None
            );
        }
        runtime.remember_damping_failure(80, facts);
        last_facts = Some(facts);
    }
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let floor_endpoint = *runtime
        .observed_damping_failures
        .iter()
        .find(|failure| failure.floor_only)
        .unwrap();
    assert!(FailureBracketAxis::Braking.observes(floor_endpoint));
    assert!(!FailureBracketAxis::Damping.observes(floor_endpoint));
    assert!(!FailureBracketAxis::ResponseHorizon.observes(floor_endpoint));
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(80, last_facts.unwrap()));
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(
        (1_233..1_311).contains(&next.brake_distance_centi_c),
        "{next:?}"
    );
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: next.brake_distance_centi_c,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(
        runtime
            .events
            .iter()
            .any(|event| event.reason.as_deref().is_some_and(
                |reason| reason.starts_with("same_run_braking_bracket_trials_3_5_weighted_")
            ))
    );
}

#[test]
fn captured_opposite_braking_failures_weight_the_same_run_interval() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_braking_bracket.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut first_point = None;
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        first_point.get_or_insert(runtime.candidate_profile.unwrap().points[0].unwrap());
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        assert_eq!(
            damping_failure_side(facts),
            Some(trial["trialIndex"] == 0),
            "{facts:?}"
        );
        runtime.advance_after_trial(60, evaluation).unwrap();
    }
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_739,
            ..first_point.unwrap()
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("same_run_braking_bracket_trials_0_1_weighted_1739")));
}

#[test]
fn braking_bracket_rejects_changed_parameters_and_cross_run_evidence() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_braking_bracket.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        runtime.remember_damping_failure(60, facts);
    }
    let mut changed = runtime.candidate_profile.unwrap();
    changed.points[0]
        .as_mut()
        .unwrap()
        .approach_damping_exponent_permille += 1;
    assert!(!runtime.prepare_failure_bracket(changed, FailureBracketAxis::Braking));
    let original = runtime.candidate_profile.unwrap();
    runtime.reset_for_run();
    assert!(!runtime.prepare_failure_bracket(original, FailureBracketAxis::Braking));
}

#[test]
fn captured_opposite_damping_failures_bisect_the_same_run_interval() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut last_facts = None;
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            evaluation.settle_time_ms,
            10_000,
        );
        // Preserve the recorded points without caching today's proposal
        // between historical trials as if it had already been tested.
        runtime.remember_damping_failure(60, facts);
        last_facts = Some(facts);
    }
    assert!(runtime.prepare_fresh_failure_proposal_from_evidence(60, last_facts.unwrap()));
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next.brake_distance_centi_c, 2_328);
    assert_eq!(next.approach_damping_exponent_permille, 2_500);
    assert_eq!(next.hold_power_permille, 56);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("same_run_damping_bracket_trials_5_6_midpoint_2500")));
}

#[test]
fn captured_saturated_damping_coast_search_uses_remaining_trajectory_horizon() {
    let trial: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_saturated_damping.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &trial);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    let facts = runtime.candidate_failure_evidence(
        60,
        evaluation.max_overshoot_centi_c,
        evaluation.hold_peak_to_peak_centi_c,
        None,
        evaluation.full_speed_limit_ms,
    );
    assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
    assert_eq!(facts.approach_output_range, Some((50, 790)));
    assert_eq!(facts.overshoot_c, 1.57);
    runtime.advance_after_trial(60, evaluation).unwrap();
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next.approach_lead_ticks, 2);
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            approach_lead_ticks: 2,
            ..previous
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_coarse_brake_preserves_progress_inside_the_warmup_start_gap() {
    let trial: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_brake_backoff.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &trial);
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(previous.brake_distance_centi_c, 1_654);
    runtime.advance_after_trial(60, evaluation).unwrap();
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next.brake_distance_centi_c, 2_144);
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 2_144,
            ..previous
        }
    );
    assert!(runtime.armed);
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_failed_damping_comparison_explores_the_existing_braking_channel() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_failed_damping.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        runtime.advance_after_trial(60, evaluation).unwrap();
    }
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(next.brake_distance_centi_c, 1_820);
    assert_eq!(next.approach_damping_exponent_permille, 2_110);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(
        runtime
            .events
            .iter()
            .any(|event| event.reason.as_deref()
                == Some("failed_damping_comparison_explores_braking"))
    );
}

#[test]
fn captured_limited_hold_damping_comparison_explores_braking() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_limited_comparison.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let mut current_point = None;
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        current_point.get_or_insert(runtime.candidate_profile.unwrap().points[0].unwrap());
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        assert_eq!(damping_failure_side(facts), Some(true));
        assert!(
            facts
                .hold_max_output_permille
                .is_some_and(|output| output <= 100)
        );
        assert!(facts.hold_off_sample_count * 5 >= facts.hold_sample_count * 4);
        assert!(facts.hold_off_sample_count * 10 < facts.hold_sample_count * 9);
        runtime.advance_after_trial(60, evaluation).unwrap();
    }
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        next,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_750,
            ..current_point.unwrap()
        }
    );
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(
        runtime
            .events
            .iter()
            .any(|event| event.reason.as_deref()
                == Some("failed_damping_comparison_explores_braking"))
    );
}

#[test]
fn captured_limited_hold_comparison_rejects_sustained_or_unqualified_heat() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_limited_comparison.fixture.json"
    ))
    .unwrap();
    let mut runtime = started_single_target_runtime();
    let mut pairs = std::vec::Vec::new();
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        pairs.push((
            runtime.candidate_profile,
            Some(runtime.candidate_failure_evidence(
                60,
                evaluation.max_overshoot_centi_c,
                evaluation.hold_peak_to_peak_centi_c,
                None,
                10_000,
            )),
        ));
    }
    let ((current, facts), (predicted, predicted_facts)) = (pairs[0], pairs[1]);
    assert!(failed_damping_comparison_needs_braking(
        current,
        predicted,
        facts,
        predicted_facts,
        60,
    ));
    let facts = facts.unwrap();
    for rejected in [
        TuneEvidence {
            hold_max_output_permille: Some(101),
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 0,
            hold_entry_pulse_only: false,
            hold_p90_output_permille: Some(51),
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 0,
            hold_entry_pulse_only: false,
            hold_p90_output_permille: None,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 540)),
            ..facts
        },
        TuneEvidence {
            over_c: 0.0,
            under_c: 2.0,
            ..facts
        },
    ] {
        assert!(!failed_damping_comparison_needs_braking(
            current,
            predicted,
            Some(rejected),
            predicted_facts,
            60,
        ));
    }
}

#[test]
fn failed_damping_braking_requires_matching_measured_high_side_failures() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_failed_damping.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut pairs = std::vec::Vec::new();
    for trial in fixture["trials"].as_array().unwrap() {
        let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
        pairs.push((
            runtime.candidate_profile,
            Some(runtime.candidate_failure_evidence(
                60,
                evaluation.max_overshoot_centi_c,
                evaluation.hold_peak_to_peak_centi_c,
                None,
                10_000,
            )),
        ));
    }
    let ((current, facts), (predicted, predicted_facts)) = (pairs[0], pairs[1]);
    assert!(failed_damping_comparison_needs_braking(
        current,
        predicted,
        facts,
        predicted_facts,
        60
    ));
    let facts = facts.unwrap();
    for rejected in [
        None,
        Some(TuneEvidence {
            target_c: 80,
            ..facts
        }),
        Some(TuneEvidence {
            stable_window_verified: true,
            ..facts
        }),
        Some(TuneEvidence {
            hold_off_sample_count: 0,
            hold_entry_pulse_only: false,
            hold_median_output_permille: Some(500),
            hold_p90_output_permille: Some(600),
            hold_max_output_permille: Some(800),
            ..facts
        }),
    ] {
        assert!(!failed_damping_comparison_needs_braking(
            current,
            predicted,
            rejected,
            predicted_facts,
            60
        ));
    }
    let mut different = predicted.unwrap();
    different.points[0].as_mut().unwrap().brake_distance_centi_c += 1;
    assert!(!failed_damping_comparison_needs_braking(
        current,
        Some(different),
        Some(facts),
        predicted_facts,
        60
    ));
}

#[test]
fn completed_scout_supplies_measured_current_without_reheating_it() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let score = evaluation.score;
    let current = runtime.candidate_profile;
    runtime.trial_purpose = ThermalTuningTrialPurpose::Scout;
    runtime.advance_after_trial(60, evaluation).unwrap();
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::BatchPredicted
    );
    assert_eq!(runtime.trial_index, 6);
    assert_eq!(runtime.batch_current_profile, current);
    assert_eq!(runtime.batch_current_score, Some(score));
    assert!(!runtime.batch_current_confirmable);
    assert!(runtime.batch_current_failure_evidence.is_some());
    assert_ne!(runtime.candidate_profile, current);
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn damping_bracket_rejects_changed_parameters_and_cross_run_evidence() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let high = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let facts = runtime.candidate_failure_evidence(
        60,
        high.max_overshoot_centi_c,
        high.hold_peak_to_peak_centi_c,
        None,
        10_000,
    );
    assert_eq!(damping_failure_side(facts), Some(true));
    for invalid in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            approach_floor_only: true,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 300)),
            ..facts
        },
    ] {
        assert_eq!(damping_failure_side(invalid), None);
    }
    runtime.remember_damping_failure(60, facts);
    let low = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
    let low_facts = runtime.candidate_failure_evidence(
        60,
        low.max_overshoot_centi_c,
        low.hold_peak_to_peak_centi_c,
        None,
        10_000,
    );
    assert_eq!(damping_failure_side(low_facts), Some(false));
    runtime.candidate_profile.as_mut().unwrap().points[0]
        .as_mut()
        .unwrap()
        .brake_distance_centi_c += 1;
    runtime.remember_damping_failure(60, low_facts);
    assert!(!runtime.prepare_damping_bracket(runtime.candidate_profile.unwrap()));
    runtime.reset_for_run();
    assert!(runtime.observed_damping_failures.is_empty());
}

#[test]
fn fan_sample_digest_matches_web_and_binds_applied_command() {
    let (event, _) = serde_json_core::from_slice::<ThermalTuningEvent>(include_bytes!(
        "../../web/src/features/control-plane-demo/thermal-tuning-fan.fixture.json"
    ))
    .unwrap();
    let expected = digest_event_chain(core::slice::from_ref(&event));
    assert_eq!(
        expected.as_str(),
        "966de02dc5498cccd6d899bb359104a1e1e83a0ec4d7dca65da7fd9dc6546ea6"
    );
    let mut altered = event;
    altered
        .sample
        .as_mut()
        .unwrap()
        .fan
        .as_mut()
        .unwrap()
        .enabled = false;
    assert_ne!(expected, digest_event_chain(&[altered]));
}

#[test]
fn canonical_candidate_hash_matches_web_wire_fixture() {
    let point = |target_temp_c| ThermalControlProfilePointConfig {
        target_temp_c,
        brake_distance_centi_c: 1,
        warmup_power_permille: 1,
        warmup_reenter_centi_c: 1,
        approach_power_permille: 1,
        approach_floor_power_permille: 1,
        approach_damping_exponent_permille: 1,
        approach_tail_window_centi_c: 1,
        hold_power_permille: 1,
        hold_reheat_power_permille: 1,
        hold_entry_centi_c: 1,
        hold_exit_centi_c: 1,
        hold_on_centi_c: 1,
        hold_off_centi_c: 1,
        overshoot_cutoff_centi_c: 1,
        hold_kp_permille_per_c: 1,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 1,
        approach_lead_ticks: 1,
        hold_lead_ticks: 1,
    };
    let profile = ThermalControlProfileConfig {
        settings: ThermalControlProfileSettingsConfig {
            temp_filter_alpha_permille: 1,
            warmup_reenter_centi_c: 1,
            hold_entry_centi_c: 1,
            hold_exit_centi_c: 1,
            hold_on_centi_c: 1,
            hold_off_centi_c: 1,
            overshoot_cutoff_centi_c: 1,
            approach_max_ticks: 1,
            approach_min_power_ratio_permille: 1,
            hold_kp_permille_per_c: 1,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 1,
            hold_reheat_power_permille: 1,
            approach_lead_ticks: 1,
            hold_lead_ticks: 1,
            auto_adjustable_working_floor_mv: 1,
            heater_current_reserve_ma: 1,
        },
        points: [
            Some(point(60)),
            Some(point(80)),
            Some(point(100)),
            Some(point(120)),
            Some(point(140)),
            Some(point(160)),
            Some(point(180)),
            Some(point(220)),
            None,
            None,
        ],
    };
    assert_eq!(
        hash_profile(&profile).unwrap().as_str(),
        "sha256:896e40c0087904f10eee7db9ddacad5e391cfaf5915923aacf27f83f3ddefe53"
    );
}

#[test]
fn decision_event_digest_fixture_for_web_export() {
    let event: ThermalTuningEvent = serde_json::from_value(serde_json::json!({
        "sequence": 0,
        "elapsedMs": 4500,
        "kind": "decision",
        "phase": "hold",
        "targetC": 60,
        "trialIndex": 1,
        "sample": null,
        "reason": "candidate_adopted",
        "trial": null,
        "decision": {
            "disposition": "adopted",
            "purpose": "hold_confirm",
            "maxOvershootCentiC": 125,
            "holdPeakToPeakCentiC": 155,
            "settleTimeMs": 7800,
            "score": [0.0, 1.0, 1.25, 2.0, 3.5, 4.0, 5.25, 6.0, 7.5, 8.0, 9.25],
            "warmupFull": true,
            "approachToHold": true,
            "holdConfirm": true,
            "hardGateOvershootCentiC": 300,
            "hardGateHoldPeakToPeakCentiC": 300,
            "hardGateFullSpeedToStableMs": 12000,
            "freeze": true,
            "prune": false,
            "stopReason": "accepted"
        }
    }))
    .unwrap();
    let mut encoded = [0u8; 4096];
    let length = serde_json_core::to_slice(&event, &mut encoded).unwrap();
    assert!(
        core::str::from_utf8(&encoded[..length])
            .unwrap()
            .contains("\"score\":[0.0,1.0,1.25")
    );
    assert_eq!(
        digest_event_chain(&[event]).as_str(),
        "97d020d2c3f902b33a1d48b53e684e2c5e93d7bd76eb6138f249ccbc3e51e893"
    );
}

#[test]
fn decision_digest_quantizes_float_score_for_cross_target_stability() {
    let mut event = serde_json::from_value::<ThermalTuningEvent>(serde_json::json!({
        "sequence": 0,
        "elapsedMs": 0,
        "kind": "decision",
        "phase": "hold",
        "targetC": 60,
        "trialIndex": 0,
        "sample": null,
        "reason": "accepted",
        "trial": null,
        "decision": {
            "disposition": "accepted",
            "purpose": "hold_confirm",
            "maxOvershootCentiC": 100,
            "holdPeakToPeakCentiC": 100,
            "settleTimeMs": 1000,
            "score": [1.2345670, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            "warmupFull": true,
            "approachToHold": true,
            "holdConfirm": true,
            "hardGateOvershootCentiC": 300,
            "hardGateHoldPeakToPeakCentiC": 300,
            "hardGateFullSpeedToStableMs": 10000,
            "freeze": true,
            "prune": false,
            "stopReason": "accepted"
        }
    }))
    .unwrap();
    let baseline = digest_event_chain(&[event.clone()]);
    event.decision.as_mut().unwrap().score[0] += 0.0000001;
    assert_eq!(baseline, digest_event_chain(&[event]));
}

fn id(value: &str) -> TuningId {
    let mut result = TuningId::new();
    result.push_str(value).unwrap();
    result
}

fn hash(value: &str) -> TuningHash {
    let mut result = TuningHash::new();
    result.push_str(value).unwrap();
    result
}

fn profile_with_targets(targets: &[i16]) -> ThermalControlProfileConfig {
    let mut profile = ThermalControlProfileConfig::default();
    for (index, target_c) in targets.iter().copied().enumerate() {
        profile.points[index] = Some(ThermalControlProfilePointConfig {
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
            hold_off_centi_c: 5,
            overshoot_cutoff_centi_c: 25,
            hold_kp_permille_per_c: 120,
            hold_ki_permille_per_c_tick: 12,
            hold_blend_ticks: 12,
            approach_lead_ticks: 0,
            hold_lead_ticks: 0,
        });
    }
    profile
}

pub(super) fn sample(
    elapsed_ms: u64,
    phase: ThermalTuningPhase,
    temperature_centi_c: i16,
) -> ThermalTuningSample {
    ThermalTuningSample {
        sequence: 999,
        elapsed_ms,
        target_c: 60,
        trial_index: 0,
        candidate_id: id("candidate-hold"),
        candidate_hash: hash("hash-hold"),
        temperature_centi_c,
        raw_temperature_centi_c: None,
        vin_mv: 12_000,
        pps_contract: ThermalTuningPpsContract {
            voltage_mv: 21_000,
            current_ma: 5_000,
            kind: ThermalTuningPowerClass::Pps5a,
            confirmed: true,
        },
        requested_mv: 21_000,
        pps_power_ceiling_mv: 21_000,
        heater_requested_output_permille: if phase == ThermalTuningPhase::Warmup {
            1_000
        } else {
            400
        },
        heater_output_permille: if phase == ThermalTuningPhase::Warmup {
            1_000
        } else {
            380
        },
        measurement_valid: true,
        measurement_accepted: None,
        phase,
        fan: None,
    }
}

fn record_initial_cooldown(runtime: &mut ThermalTuningRuntime, elapsed_ms: u64) {
    let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 3_400);
    cooling.trial_index = runtime.trial_index;
    cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
    cooling.heater_requested_output_permille = 0;
    cooling.heater_output_permille = 0;
    runtime.record_sample(cooling).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
}

#[test]
fn cooldown_accepts_idle_request_when_trial_contract_is_confirmed() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-power-staging"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();

    let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_400);
    cooling.heater_requested_output_permille = 0;
    cooling.heater_output_permille = 0;
    cooling.requested_mv = 12_000;
    cooling.pps_contract.voltage_mv = 12_000;
    runtime.record_sample(cooling.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);

    cooling.elapsed_ms = 300;
    cooling.pps_contract.voltage_mv = 12_000;
    runtime.record_sample(cooling.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);

    cooling.elapsed_ms = 600;
    cooling.pps_contract.voltage_mv = 21_000;
    runtime.record_sample(cooling).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
}

#[test]
fn cooldown_requires_current_accepted_temperature_without_pausing_budget() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-current-temperature"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    for (elapsed_ms, temperature_centi_c) in [(0, 4_307), (300, 3_454)] {
        let mut observed = sample(
            elapsed_ms,
            ThermalTuningPhase::Cooldown,
            temperature_centi_c,
        );
        observed.heater_requested_output_permille = 0;
        observed.heater_output_permille = 0;
        runtime
            .record_live_sample_with_cooldown_admission(observed, false)
            .unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    }
    assert!(!runtime.events.iter().any(|event| event.trial.is_some()));
    assert_eq!(runtime.current_elapsed_ms, 300);
    let temperatures: std::vec::Vec<_> = runtime
        .events
        .iter()
        .filter_map(|event| {
            event
                .sample
                .as_ref()
                .map(|sample| sample.temperature_centi_c)
        })
        .collect();
    assert_eq!(temperatures, [4_307, 3_454]);

    let mut accepted = sample(600, ThermalTuningPhase::Cooldown, 3_454);
    accepted.heater_requested_output_permille = 0;
    accepted.heater_output_permille = 0;
    let mut exhausted = runtime.clone();
    let mut pending = accepted.clone();
    pending.elapsed_ms = THERMAL_TUNING_TARGET_BUDGET_MS;
    exhausted
        .record_live_sample_with_cooldown_admission(pending, false)
        .unwrap();
    assert!(!exhausted.armed);
    assert!(exhausted.events.iter().any(|event| {
        event.reason.as_deref() == Some("target_budget_exhausted_during_cooldown")
    }));
    assert!(!exhausted.events.iter().any(|event| event.trial.is_some()));

    runtime
        .record_live_sample_with_cooldown_admission(accepted, true)
        .unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    assert_eq!(runtime.trial_start_temperature_centi_c, Some(3_454));
}

#[test]
fn firmware_60c_initial_and_retry_trials_start_at_35c() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-35c-cooldown"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();

    let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_501);
    cooling.heater_requested_output_permille = 0;
    cooling.heater_output_permille = 0;
    runtime.record_sample(cooling.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    cooling.elapsed_ms = 300;
    cooling.temperature_centi_c = 3_500;
    runtime.record_sample(cooling).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    let first = runtime
        .events
        .iter()
        .find_map(|event| event.trial.as_ref())
        .unwrap();
    assert_eq!(first.start_temperature_centi_c, Some(3_500));
    assert_eq!(first.cooldown_threshold_centi_c, Some(3_500));

    for (elapsed_ms, phase, temperature_centi_c) in [
        (300, ThermalTuningPhase::Warmup, 3_500),
        (1_300, ThermalTuningPhase::Approach, 4_500),
        (11_300, ThermalTuningPhase::Approach, 5_000),
    ] {
        runtime
            .record_sample(sample(elapsed_ms, phase, temperature_centi_c))
            .unwrap();
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.trial_index, 1);
    let closed_first = runtime
        .events
        .iter()
        .filter_map(|event| event.trial.as_ref())
        .find(|trial| trial.trial_index == 0 && trial.end_sequence.is_some())
        .unwrap();
    assert_eq!(closed_first.start_temperature_centi_c, Some(3_500));
    assert_eq!(closed_first.cooldown_threshold_centi_c, Some(3_500));

    let mut retry = sample(11_600, ThermalTuningPhase::Cooldown, 3_501);
    retry.trial_index = runtime.trial_index;
    retry.candidate_hash = runtime.candidate_hash.clone().unwrap();
    retry.heater_requested_output_permille = 0;
    retry.heater_output_permille = 0;
    runtime.record_sample(retry.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    retry.elapsed_ms = 11_900;
    retry.temperature_centi_c = 3_500;
    runtime.record_sample(retry).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    let retry_trial = runtime
        .events
        .iter()
        .filter_map(|event| event.trial.as_ref())
        .find(|trial| trial.trial_index == 1)
        .unwrap();
    assert_eq!(retry_trial.start_temperature_centi_c, Some(3_500));
    assert_eq!(retry_trial.cooldown_threshold_centi_c, Some(3_500));
    assert_eq!(cooldown_threshold_c(80), 40);
    assert_eq!(cooldown_threshold_c(240), 220);
}

#[test]
fn high_temperature_cooldown_keeps_strict_160_and_200_boundaries() {
    for (target, threshold) in [
        (160, 120),
        (161, 131),
        (180, 150),
        (200, 170),
        (201, 181),
        (220, 200),
        (240, 220),
    ] {
        assert_eq!(cooldown_threshold_c(target), threshold);
    }
}

#[test]
fn every_high_temperature_trial_waits_for_and_records_its_cooldown_boundary() {
    for (target, threshold) in [(160, 12_000), (180, 15_000), (220, 20_000)] {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-high-temperature-cooldown"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[target],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[target])),
            })
            .unwrap();
        for purpose in [
            ThermalTuningTrialPurpose::Scout,
            ThermalTuningTrialPurpose::BatchCurrent,
            ThermalTuningTrialPurpose::BatchPredicted,
            ThermalTuningTrialPurpose::HoldConfirm,
        ] {
            if purpose != ThermalTuningTrialPurpose::Scout {
                runtime
                    .schedule_trial(purpose, "cooling before test trial")
                    .unwrap();
            }
            let mut cooling = sample(
                runtime.current_elapsed_ms + 300,
                ThermalTuningPhase::Cooldown,
                threshold + 1,
            );
            cooling.target_c = target;
            cooling.trial_index = runtime.trial_index;
            cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_sample(cooling.clone()).unwrap();
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
            cooling.elapsed_ms += 300;
            cooling.temperature_centi_c = threshold;
            runtime.record_sample(cooling).unwrap();
            assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
            let trial = runtime
                .events
                .iter()
                .rev()
                .find_map(|event| event.trial.as_ref())
                .unwrap();
            assert_eq!(trial.purpose, purpose);
            assert_eq!(trial.start_temperature_centi_c, Some(threshold));
            assert_eq!(trial.cooldown_threshold_centi_c, Some(threshold));
        }
    }
}

#[test]
fn high_side_trace_stops_when_the_stable_window_breaks_before_deadline() {
    let mut profile = profile_with_targets(&[60]);
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 1_100;
    point.approach_lead_ticks = 6;
    point.approach_power_permille = 450;
    point.approach_floor_power_permille = 160;
    point.approach_damping_exponent_permille = 4_000;
    point.hold_power_permille = 135;
    point.hold_reheat_power_permille = 170;
    point.hold_entry_centi_c = 220;
    point.hold_exit_centi_c = 400;
    point.hold_lead_ticks = 6;
    point.hold_blend_ticks = 1;
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("high-side-trace"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    let mut cooldown = sample(0, ThermalTuningPhase::Cooldown, 3_470);
    cooldown.heater_requested_output_permille = 0;
    cooldown.heater_output_permille = 0;
    runtime.record_sample(cooldown).unwrap();
    for (elapsed_ms, phase, temperature_centi_c) in [
        (1_000, ThermalTuningPhase::Warmup, 3_470),
        (17_000, ThermalTuningPhase::Warmup, 4_800),
        (17_587, ThermalTuningPhase::Approach, 4_823),
        (21_131, ThermalTuningPhase::Approach, 5_725),
        (21_488, ThermalTuningPhase::Hold, 5_797),
        (21_932, ThermalTuningPhase::Hold, 5_855),
        (24_800, ThermalTuningPhase::Hold, 6_148),
        (25_147, ThermalTuningPhase::Hold, 6_154),
        (27_573, ThermalTuningPhase::Hold, 6_319),
        (27_916, ThermalTuningPhase::Hold, 6_307),
    ] {
        let mut observed = sample(elapsed_ms, phase, temperature_centi_c);
        if phase == ThermalTuningPhase::Hold {
            observed.heater_requested_output_permille = 0;
            observed.heater_output_permille = 0;
        }
        runtime.record_sample(observed).unwrap();
    }
    assert_eq!(runtime.trial_index, 1);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    let decision_event = runtime
        .events
        .iter()
        .find(|event| event.decision.is_some())
        .unwrap();
    assert_eq!(decision_event.elapsed_ms, 27_916);
    assert_eq!(
        decision_event.decision.as_ref().unwrap().stop_reason,
        "full-speed-to-stable budget exhausted"
    );
    let selected = runtime.batch_predicted_profile.unwrap();
    let selected_point = selected.points[0].unwrap();
    assert_eq!(selected_point.brake_distance_centi_c, 1_400);
    assert_eq!(selected_point.approach_power_permille, 350);
}

#[test]
fn live_samples_retain_100ms_soft_start_before_full_output() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-soft-start"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    record_initial_cooldown(&mut runtime, 0);
    for (elapsed_ms, output) in [(50, 0), (100, 500), (150, 1_000), (200, 1_000)] {
        let mut warmup = sample(elapsed_ms, ThermalTuningPhase::Warmup, 3_400);
        warmup.heater_output_permille = output;
        runtime.record_live_sample(warmup).unwrap();
    }
    let outputs: std::vec::Vec<_> = runtime
        .events
        .iter()
        .filter_map(|event| {
            event
                .sample
                .as_ref()
                .filter(|sample| sample.phase == ThermalTuningPhase::Warmup)
                .map(|sample| (sample.elapsed_ms, sample.heater_output_permille))
        })
        .collect();
    assert_eq!(outputs, [(50, 0), (100, 500), (150, 1_000)]);
    assert!(runtime.warmup_full_output);
}

#[test]
fn live_samples_follow_host_cadence_without_skipping_safety() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-live-cadence"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    for elapsed_ms in (0..=1_000).step_by(100) {
        let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 7_000);
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_live_sample(cooling).unwrap();
    }
    assert_eq!(
        runtime
            .events
            .iter()
            .filter(|event| event.kind == ThermalTuningEventKind::Sample)
            .count(),
        6
    );

    let mut invalid = sample(1_050, ThermalTuningPhase::Warmup, 7_000);
    invalid.heater_requested_output_permille = 0;
    invalid.heater_output_permille = 0;
    invalid.pps_contract.confirmed = false;
    assert_eq!(
        runtime.record_live_sample(invalid),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!runtime.armed);
}

#[test]
fn live_sample_pages_remain_contiguous_beyond_event_capacity() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-live-pages"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    let mut cursor = None;
    let mut received = Vec::<u32, 512>::new();
    for elapsed_ms in (0..=90_000).step_by(100) {
        let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 7_000);
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_live_sample(cooling).unwrap();
        if elapsed_ms % 1_200 == 0 {
            let page = runtime.events_after(cursor);
            for event in page.events {
                received.push(event.sequence).unwrap();
            }
            cursor = page.next_sequence.or(cursor);
        }
    }
    loop {
        let page = runtime.events_after(cursor);
        if page.events.is_empty() {
            break;
        }
        for event in page.events {
            received.push(event.sequence).unwrap();
        }
        cursor = page.next_sequence;
    }
    assert!(!runtime.trace().gap);
    assert!(received.len() > THERMAL_TUNING_EVENT_CAPACITY);
    assert!(
        received
            .iter()
            .enumerate()
            .all(|(index, sequence)| *sequence == index as u32)
    );
}

fn record_current_trial_sample(
    runtime: &mut ThermalTuningRuntime,
    elapsed_ms: u64,
    phase: ThermalTuningPhase,
    temperature_centi_c: i16,
) {
    let mut observed = sample(elapsed_ms, phase, temperature_centi_c);
    observed.trial_index = runtime.trial_index;
    observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
    if phase == ThermalTuningPhase::Cooldown {
        observed.heater_requested_output_permille = 0;
        observed.heater_output_permille = 0;
    }
    runtime.record_sample(observed).unwrap();
}

fn record_dense_hold(runtime: &mut ThermalTuningRuntime, first: u64, last: u64, temperature: i16) {
    for elapsed in (first..=last).step_by(300) {
        record_current_trial_sample(runtime, elapsed, ThermalTuningPhase::Hold, temperature);
        if runtime.events.len() > 128 {
            let cursor = drain_test_events(runtime, runtime.last_acknowledged_sequence);
            runtime.events_after(cursor);
        }
    }
}

fn drain_test_events(runtime: &mut ThermalTuningRuntime, mut cursor: Option<u32>) -> Option<u32> {
    loop {
        let page = runtime.events_after(cursor);
        if page.events.is_empty() {
            return cursor;
        }
        cursor = page.next_sequence;
    }
}

fn started_single_target_runtime() -> ThermalTuningRuntime {
    started_runtime_with_profile(profile_with_targets(&[60]))
}

#[test]
fn restarted_run_can_propose_a_point_tested_only_in_the_previous_run() {
    let original = profile_with_targets(&[60]);
    let request = || ThermalTuningStart {
        run_id: id("restart-run"),
        candidate_id: id("restart-candidate"),
        candidate_hash: hash("restart-hash"),
        power_class: ThermalTuningPowerClass::Pps5a,
        targets: &[60],
        pps_ready: true,
        thermal_model_available: true,
        calibration_conflict: false,
        candidate_profile: Some(original),
    };
    let mut proposed = original;
    proposed.points[0].as_mut().unwrap().brake_distance_centi_c += 40;
    for canceled in [true, false] {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.start(request()).unwrap();
        runtime.candidate_profile = Some(proposed);
        assert!(runtime.record_fresh_proposal(Some(original)));
        assert!(!runtime.record_fresh_proposal(Some(original)));
        if canceled {
            runtime.cancel("host interrupted").unwrap();
        } else {
            runtime.fail("target budget exhausted");
        }
        runtime.start(request()).unwrap();
        runtime.candidate_profile = Some(proposed);
        assert!(runtime.record_fresh_proposal(Some(original)));
        assert!(!runtime.record_fresh_proposal(Some(original)));
        assert_eq!(runtime.accepted_target_mask, 0);
    }
}

#[test]
fn fixed_idle_preparation_never_admits_an_unconfirmed_trial() {
    let request = || ThermalTuningStart {
        run_id: id("run-preparing-power"),
        candidate_id: id("candidate-hold"),
        candidate_hash: hash("hash-hold"),
        power_class: ThermalTuningPowerClass::Pps5a,
        targets: &[60],
        pps_ready: false,
        thermal_model_available: true,
        calibration_conflict: false,
        candidate_profile: Some(profile_with_targets(&[60])),
    };
    let mut strict = ThermalTuningRuntime::new();
    assert!(strict.start(request()).is_err());
    assert!(!strict.armed);

    let mut preparing = ThermalTuningRuntime::new();
    preparing.start_preparing_power_at(request(), 0).unwrap();
    assert_eq!(preparing.phase, ThermalTuningPhase::Cooldown);
    let mut unconfirmed = sample(0, ThermalTuningPhase::Cooldown, 3_400);
    unconfirmed.heater_requested_output_permille = 0;
    unconfirmed.heater_output_permille = 0;
    unconfirmed.pps_contract.confirmed = false;
    assert!(preparing.record_sample(unconfirmed).is_err());
    assert!(!preparing.armed);

    let mut confirmed = ThermalTuningRuntime::new();
    confirmed.start_preparing_power_at(request(), 0).unwrap();
    record_initial_cooldown(&mut confirmed, 0);
    assert_eq!(confirmed.phase, ThermalTuningPhase::Warmup);
}

fn started_runtime_with_profile(profile: ThermalControlProfileConfig) -> ThermalTuningRuntime {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-speed-gate"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    record_initial_cooldown(&mut runtime, 0);
    runtime
}

#[test]
fn approach_at_allowed_floor_can_continue_after_original_deadline() {
    let mut profile = profile_with_targets(&[60]);
    let point = profile.points[0].as_mut().unwrap();
    point.approach_floor_power_permille = 100;
    point.approach_power_permille = 180;
    point.hold_power_permille = 60;
    let mut runtime = started_runtime_with_profile(profile);
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
    for elapsed_ms in [1_000, 5_000, 11_000] {
        let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
        observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
        observed.heater_requested_output_permille = 50;
        observed.heater_output_permille = 170;
        runtime.record_sample(observed).unwrap();
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
    assert_eq!(runtime.trial_index, 0);
    assert_eq!(runtime.trial_timing.unwrap().policy_version, 5);
    assert!(runtime.events.iter().all(|event| event.decision.is_none()));
}

#[test]
fn a_hold_window_break_cannot_obtain_a_retrospective_approach_extension() {
    let mut profile = profile_with_targets(&[60]);
    let point = profile.points[0].as_mut().unwrap();
    point.approach_floor_power_permille = 100;
    point.hold_power_permille = 60;
    let mut runtime = started_runtime_with_profile(profile);
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
    for elapsed in [1_000, 2_000] {
        let mut observed = sample(elapsed, ThermalTuningPhase::Approach, 5_500);
        observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
        observed.heater_requested_output_permille = 100;
        runtime.record_sample(observed).unwrap();
    }
    record_dense_hold(&mut runtime, 3_000, 10_800, 6_000);
    record_current_trial_sample(&mut runtime, 11_000, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    record_current_trial_sample(&mut runtime, 12_000, ThermalTuningPhase::Hold, 6_170);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "rejected");
    assert!(decision.timing.unwrap().extended_at_ms.is_none());
}

#[test]
fn deadline_coast_preserves_bound_floor_evidence_in_the_live_trial() {
    let mut profile = profile_with_targets(&[60]);
    let point = profile.points[0].as_mut().unwrap();
    point.approach_floor_power_permille = 100;
    point.hold_power_permille = 60;
    let mut runtime = started_runtime_with_profile(profile);
    let candidate_hash = runtime.candidate_hash.clone().unwrap();
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
    for (elapsed_ms, request) in [(1_000, 50), (5_000, 50), (11_000, 0)] {
        let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
        observed.candidate_hash = candidate_hash.clone();
        observed.heater_requested_output_permille = request;
        observed.heater_output_permille = request;
        runtime.record_sample(observed).unwrap();
    }
    assert!(runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
    assert_eq!(runtime.trial_index, 0);
    assert_eq!(runtime.candidate_hash.as_ref(), Some(&candidate_hash));
    assert!(runtime.trial_timing.unwrap().valid_extension());
    assert!(runtime.events.iter().all(|event| event.decision.is_none()));
}

pub(super) fn extended_floor_runtime() -> ThermalTuningRuntime {
    let mut profile = profile_with_targets(&[60]);
    let point = profile.points[0].as_mut().unwrap();
    point.approach_floor_power_permille = 100;
    point.hold_power_permille = 60;
    let mut runtime = started_runtime_with_profile(profile);
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
    for elapsed_ms in (1_000..=12_000).step_by(300) {
        let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
        observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
        observed.heater_requested_output_permille = 50;
        runtime.record_sample(observed).unwrap();
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
    assert!(runtime.trial_timing.unwrap().valid_extension());
    runtime
}

#[test]
fn extended_scout_still_requires_a_full_stable_window_and_qualified_hold() {
    let mut runtime = extended_floor_runtime();
    for elapsed_ms in (13_000..=25_000).step_by(300) {
        record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "passed");
    assert_eq!(decision.settle_time_ms, Some(12_000));
    assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
    assert!(!decision.hold_confirm);
    assert!(!decision.freeze);
    assert!(decision.timing.unwrap().valid_extension());
    assert!(!runtime.save_confirmed);
}

#[test]
fn ordinary_scout_waits_for_a_timely_restarted_stable_window() {
    let mut runtime = started_single_target_runtime();
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
    for elapsed_ms in (1_000..=8_800).step_by(300) {
        let temperature = if elapsed_ms == 1_000 { 6_000 } else { 6_170 };
        let phase = if elapsed_ms == 1_000 {
            ThermalTuningPhase::Approach
        } else {
            ThermalTuningPhase::Hold
        };
        record_current_trial_sample(&mut runtime, elapsed_ms, phase, temperature);
    }
    record_dense_hold(&mut runtime, 9_100, 13_300, 6_000);
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.trial_index, 0);
    assert!(runtime.events.iter().all(|event| event.decision.is_none()));
    assert_eq!(runtime.stable_window_started_at_ms, Some(9_100));
    assert_eq!(runtime.stable_window_verified_at_ms, None);
    assert_eq!(runtime.trial_timing.unwrap().extended_at_ms, None);

    record_dense_hold(&mut runtime, 13_600, 19_000, 6_000);
    record_current_trial_sample(&mut runtime, 19_100, ThermalTuningPhase::Hold, 6_000);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "passed");
    assert_eq!(decision.settle_time_ms, Some(8_100));
    assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 10_000);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    assert!(!runtime.save_confirmed);
}

#[test]
fn live_recording_verifies_stability_with_observed_control_intervals() {
    for control_interval_ms in [68, 83, 96] {
        let mut runtime = started_single_target_runtime();
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        runtime
            .record_live_sample(sample(1_000, ThermalTuningPhase::Approach, 5_900))
            .unwrap();
        for elapsed_ms in (1_068..=20_000).step_by(control_interval_ms) {
            let temperature = if elapsed_ms < 9_000 { 6_170 } else { 6_000 };
            runtime
                .record_live_sample(sample(elapsed_ms, ThermalTuningPhase::Hold, temperature))
                .unwrap();
            if runtime.phase == ThermalTuningPhase::Cooldown {
                break;
            }
        }
        assert_eq!(
            runtime.phase,
            ThermalTuningPhase::Cooldown,
            "control interval {control_interval_ms}ms must not starve the stable recorder"
        );
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "passed");
        let timing = decision.timing.unwrap();
        let window_ms = timing.stable_window_verified_at_ms.unwrap()
            - timing.stable_window_started_at_ms.unwrap();
        assert!((10_000..=11_000).contains(&window_ms));
    }
}

#[test]
fn long_scout_cannot_claim_independent_confirmation() {
    let mut runtime = extended_floor_runtime();
    for elapsed_ms in (13_000..=75_000).step_by(340) {
        record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert!(runtime.stable_window_verified_at_ms.is_none());
    for elapsed_ms in (75_040..=85_000).step_by(200) {
        record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
        if runtime.phase == ThermalTuningPhase::Cooldown {
            break;
        }
    }
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.purpose, ThermalTuningTrialPurpose::Scout);
    assert_eq!(decision.disposition.as_str(), "passed");
    assert!(!decision.hold_confirm);
    assert!(!decision.freeze);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    assert!(!runtime.save_confirmed);
}

fn extended_scout_after_minimum_hold(window_start_ms: u64) -> ThermalTuningRuntime {
    let mut runtime = extended_floor_runtime();
    for elapsed_ms in (13_000..window_start_ms).step_by(300) {
        let temperature = if elapsed_ms == 13_000 { 6_000 } else { 6_170 };
        record_current_trial_sample(
            &mut runtime,
            elapsed_ms,
            ThermalTuningPhase::Hold,
            temperature,
        );
    }
    record_current_trial_sample(
        &mut runtime,
        window_start_ms,
        ThermalTuningPhase::Hold,
        6_000,
    );
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.trial_index, 0);
    assert_eq!(runtime.stable_window_started_at_ms, Some(window_start_ms));
    assert_eq!(runtime.stable_window_verified_at_ms, None);
    runtime
}

#[test]
fn extended_scout_waits_for_the_same_window_beyond_minimum_hold() {
    let mut runtime = extended_scout_after_minimum_hold(20_200);
    let candidate_hash = runtime.candidate_hash.clone();
    record_dense_hold(&mut runtime, 20_500, 25_000, 6_000);
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.trial_index, 0);
    assert!(runtime.events.iter().all(|event| event.decision.is_none()));

    record_dense_hold(&mut runtime, 25_300, 30_100, 6_000);
    record_current_trial_sample(&mut runtime, 30_200, ThermalTuningPhase::Hold, 6_000);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "passed");
    assert_eq!(decision.settle_time_ms, Some(19_200));
    assert_eq!(
        decision.timing.unwrap().stable_window_verified_at_ms,
        Some(30_200)
    );
    assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
    assert_eq!(runtime.candidate_hash, candidate_hash);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    assert!(!runtime.save_confirmed);
}

#[test]
fn waiting_scout_still_stops_when_its_window_breaks_after_the_cap() {
    let mut runtime = extended_scout_after_minimum_hold(30_500);
    record_current_trial_sample(&mut runtime, 30_800, ThermalTuningPhase::Hold, 6_000);
    record_current_trial_sample(&mut runtime, 31_100, ThermalTuningPhase::Hold, 6_000);
    record_current_trial_sample(&mut runtime, 31_200, ThermalTuningPhase::Hold, 6_170);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "rejected");
    assert_eq!(
        decision.stop_reason,
        "full-speed-to-stable budget exhausted"
    );
    assert_eq!(decision.timing.unwrap().stable_window_verified_at_ms, None);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.target_started_elapsed_ms, 0);
}

#[test]
fn waiting_scout_still_disarms_on_invalid_measurements() {
    let mut runtime = extended_scout_after_minimum_hold(20_200);
    record_dense_hold(&mut runtime, 20_500, 25_000, 6_000);
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.trial_index, 0);
    let mut invalid = sample(25_300, ThermalTuningPhase::Hold, 6_000);
    invalid.candidate_hash = runtime.candidate_hash.clone().unwrap();
    invalid.measurement_valid = false;
    assert_eq!(
        runtime.record_sample(invalid),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
}

#[test]
fn extension_stops_at_its_absolute_deadline_without_resetting_the_budget() {
    let mut runtime = extended_floor_runtime();
    record_current_trial_sample(&mut runtime, 31_000, ThermalTuningPhase::Approach, 5_500);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.target_started_elapsed_ms, 0);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "rejected");
    assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
    assert_eq!(decision.timing.unwrap().original_limit_ms, 10_000);
}

#[test]
fn extended_confirmation_keeps_all_sixty_seconds_and_never_automatically_saves() {
    let mut runtime = extended_floor_runtime();
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    for elapsed_ms in (13_000..=72_700).step_by(300) {
        record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
    }
    assert!(runtime.armed);
    assert_eq!(runtime.accepted_target_mask, 0);
    record_current_trial_sample(&mut runtime, 73_000, ThermalTuningPhase::Hold, 6_000);
    assert!(!runtime.armed);
    assert_eq!(runtime.accepted_target_mask, target_bit(60));
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "adopted");
    assert!(decision.hold_confirm);
    assert!(decision.freeze);
    assert!(decision.timing.unwrap().valid_extension());
    assert!(!runtime.save_confirmed);
    assert!(runtime.save_receipt.is_none());
}

#[test]
fn extended_trial_cannot_ignore_overshoot_or_an_invalid_sensor() {
    let mut overshoot = extended_floor_runtime();
    record_current_trial_sample(&mut overshoot, 13_000, ThermalTuningPhase::Hold, 6_301);
    assert_eq!(overshoot.phase, ThermalTuningPhase::Cooldown);
    let decision = overshoot
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "rejected");
    assert_eq!(
        decision.stop_reason.as_str(),
        "conditional extension thermal gate failed"
    );
    let mut invalid = extended_floor_runtime();
    let mut observed = sample(13_000, ThermalTuningPhase::Approach, 5_500);
    observed.candidate_hash = invalid.candidate_hash.clone().unwrap();
    observed.measurement_valid = false;
    assert_eq!(
        invalid.record_sample(observed),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!invalid.armed);
    assert_eq!(
        invalid.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
}

#[test]
fn scout_and_batch_stop_at_full_speed_deadline_without_stable_window() {
    for purpose in [
        ThermalTuningTrialPurpose::Scout,
        ThermalTuningTrialPurpose::BatchCurrent,
        ThermalTuningTrialPurpose::BatchPredicted,
    ] {
        let mut runtime = started_single_target_runtime();
        runtime.trial_purpose = purpose;
        if purpose != ThermalTuningTrialPurpose::Scout {
            runtime.batch_current_profile = runtime.candidate_profile;
        }
        for (elapsed_ms, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_900),
            (1_000, ThermalTuningPhase::Approach, 5_000),
            (5_000, ThermalTuningPhase::Hold, 5_500),
            (11_000, ThermalTuningPhase::Hold, 5_500),
        ] {
            record_current_trial_sample(&mut runtime, elapsed_ms, phase, temperature);
        }
        if purpose == ThermalTuningTrialPurpose::BatchPredicted {
            // This isolated deadline harness has no preceding current
            // trial. Never substitute the losing trial's facts for it.
            assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
            assert!(!runtime.armed);
        } else {
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown, "{purpose:?}");
            assert_eq!(runtime.trial_index, 1, "{purpose:?}");
        }
        let decision_event = runtime
            .events
            .iter()
            .find(|event| event.decision.is_some())
            .unwrap();
        let decision = decision_event.decision.as_ref().unwrap();
        assert_eq!(decision_event.elapsed_ms, 11_000);
        assert_eq!(decision.purpose, purpose);
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert_eq!(decision.max_overshoot_centi_c, Some(0));
        assert_eq!(decision.hold_peak_to_peak_centi_c, None);
        assert!(!decision.hold_confirm);
    }
}

#[test]
fn live_trial_five_replay_stops_before_qualified_hold_despite_controller_hold() {
    let mut runtime = started_single_target_runtime();
    // Actual phase/temperature boundary observations from run
    // firmware-pps5a-1790966844116, trial 5, relative to its trial start.
    for (elapsed_ms, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_495),
        (4_061, ThermalTuningPhase::Approach, 4_314),
        (13_840, ThermalTuningPhase::Hold, 5_582),
    ] {
        runtime
            .record_live_sample(sample(elapsed_ms, phase, temperature))
            .unwrap();
    }
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert!(runtime.first_hold_elapsed_ms.is_none());
    // Controller Hold cannot extend the full-speed-to-stable deadline.
    runtime
        .record_live_sample(sample(14_061, ThermalTuningPhase::Hold, 5_593))
        .unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.trial_index, 1);
    let event = runtime
        .events
        .iter()
        .find(|event| event.decision.is_some())
        .unwrap();
    let decision = event.decision.as_ref().unwrap();
    assert_eq!(event.elapsed_ms, 14_061);
    assert_eq!(
        decision.stop_reason,
        "full-speed-to-stable budget exhausted"
    );
    assert!(decision.settle_time_ms.is_none());
    assert!(decision.hold_peak_to_peak_centi_c.is_none());
    assert_eq!(runtime.accepted_target_mask, 0);
}

#[test]
fn live_deadline_checks_high_temperature_and_broken_timely_window_each_cycle() {
    for (target, window_start, elapsed_ms, temperature) in [
        (180, None, 6_000, 17_000),
        (60, Some(10_900), 11_010, 5_840),
    ] {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-deadline"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[target],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[target])),
            })
            .unwrap();
        let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_400);
        cooling.target_c = target;
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling).unwrap();
        for (at_ms, phase, temp) in [
            (0, ThermalTuningPhase::Warmup, target * 100 - 4_000),
            (1_000, ThermalTuningPhase::Approach, target * 100 - 2_000),
            (
                elapsed_ms - 110,
                ThermalTuningPhase::Hold,
                if window_start.is_some() {
                    target * 100 - 100
                } else {
                    target * 100 - 2_000
                },
            ),
        ] {
            let mut observed = sample(at_ms, phase, temp);
            observed.target_c = target;
            runtime.record_live_sample(observed).unwrap();
        }
        assert_eq!(runtime.stable_window_started_at_ms, window_start);
        // A second sample in the same phase establishes normal recording
        // cadence. The failure only 50ms later must bypass that cadence.
        let recent_temp = if window_start.is_some() {
            target * 100 - 100
        } else {
            temperature
        };
        let mut recent = sample(elapsed_ms - 50, ThermalTuningPhase::Hold, recent_temp);
        recent.target_c = target;
        runtime.record_live_sample(recent).unwrap();
        let mut observed = sample(elapsed_ms, ThermalTuningPhase::Hold, temperature);
        observed.target_c = target;
        runtime.record_live_sample(observed).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown, "{target}C");
        let event = runtime
            .events
            .iter()
            .find(|event| event.decision.is_some())
            .unwrap();
        assert_eq!(event.elapsed_ms, elapsed_ms);
        assert_eq!(
            event.decision.as_ref().unwrap().stop_reason,
            "full-speed-to-stable budget exhausted"
        );
    }
}

#[test]
fn hold_confirm_settle_timeout_rejects_trial_and_cools_for_retry() {
    let mut runtime = started_single_target_runtime();
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 4_000))
        .unwrap();
    runtime
        .record_sample(sample(11_000, ThermalTuningPhase::Approach, 4_500))
        .unwrap();
    assert!(runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.trial_index, 1);
    assert!(runtime.events.iter().any(|event| {
        event.kind == ThermalTuningEventKind::Sample && event.elapsed_ms == 11_000
    }));
    assert!(runtime.events.iter().any(|event| {
        event.kind == ThermalTuningEventKind::Decision
            && event.decision.as_ref().is_some_and(|decision| {
                decision.stop_reason == "full-speed-to-stable budget exhausted"
            })
    }));
}

#[test]
fn hold_observation_starts_near_target_after_early_hold_phase() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_600))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_000))
        .unwrap();
    runtime
        .record_sample(sample(2_000, ThermalTuningPhase::Hold, 5_696))
        .unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, None);
    assert_eq!(runtime.hold_output_count, 1);
    assert_eq!(runtime.first_hold_temperature_centi_c, Some(5_696));
    assert_eq!(runtime.min_hold_temperature_centi_c, None);

    runtime
        .record_sample(sample(6_000, ThermalTuningPhase::Hold, 5_970))
        .unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, Some(6_000));
    record_dense_hold(&mut runtime, 6_100, 9_900, 5_970);
    record_dense_hold(&mut runtime, 10_000, 15_900, 6_131);
    record_current_trial_sample(&mut runtime, 16_000, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.trial_index, 0);
    record_dense_hold(&mut runtime, 16_100, 17_900, 6_000);
    record_current_trial_sample(&mut runtime, 18_000, ThermalTuningPhase::Hold, 6_000);
    assert!(runtime.events.iter().any(|event| {
        event.kind == ThermalTuningEventKind::Decision
            && event.decision.as_ref().is_some_and(|decision| {
                decision.disposition.as_str() == "passed"
                    && decision.hold_peak_to_peak_centi_c == Some(161)
            })
    }));
}

#[test]
fn hold_entry_request_is_retained_without_hiding_later_supply() {
    for (entry, later) in [(50, 0), (0, 50), (51, 0), (50, 50)] {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_500))
            .unwrap();
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_000))
            .unwrap();
        for (elapsed, temperature, output) in [
            (2_000, 5_953, entry),
            (2_200, 5_990, later),
            (2_400, 6_160, 0),
        ] {
            let mut observation = sample(elapsed, ThermalTuningPhase::Hold, temperature);
            observation.heater_requested_output_permille = output;
            runtime.record_sample(observation).unwrap();
        }
        let facts = runtime.candidate_failure_evidence(60, Some(160), Some(207), None, 10_000);
        assert_eq!(facts.hold_entry_output_permille, Some(entry));
        assert_eq!(facts.hold_sample_count, 3);
        assert_eq!(
            facts.hold_off_sample_count,
            u32::from(entry == 0) + u32::from(later == 0) + 1
        );
    }
}

#[test]
fn warmup_requires_sustained_logical_and_physical_full_output() {
    let mut runtime = started_single_target_runtime();
    let mut soft_start = sample(0, ThermalTuningPhase::Warmup, 3_000);
    soft_start.heater_output_permille = 800;
    runtime.record_sample(soft_start).unwrap();
    assert!(!runtime.warmup_full_output);
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Warmup, 3_100))
        .unwrap();
    assert!(runtime.warmup_full_output);
    let mut bounded = sample(1_500, ThermalTuningPhase::Warmup, 3_200);
    bounded.heater_output_permille = 970;
    runtime.record_sample(bounded.clone()).unwrap();
    bounded.elapsed_ms = 3_500;
    runtime.record_sample(bounded.clone()).unwrap();
    assert!(!runtime.warmup_full_output_invalid);
    bounded.elapsed_ms = 3_551;
    runtime.record_sample(bounded).unwrap();
    assert!(runtime.warmup_full_output_invalid);
}

#[test]
fn warmup_logical_drop_cannot_recover_after_full_output() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    let mut reduced = sample(100, ThermalTuningPhase::Warmup, 3_100);
    reduced.heater_requested_output_permille = 994;
    runtime.record_sample(reduced).unwrap();
    runtime
        .record_sample(sample(200, ThermalTuningPhase::Warmup, 3_200))
        .unwrap();
    assert!(runtime.warmup_full_output_invalid);
}

#[test]
fn strategy_statistics_ignore_pps_pwm_for_the_same_logical_requests() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[60]));
    for (elapsed_ms, phase, temp, requested, pwm) in [
        (1_000, ThermalTuningPhase::Approach, 5_300, 680, 710),
        (1_250, ThermalTuningPhase::Approach, 5_400, 760, 950),
        (1_500, ThermalTuningPhase::Hold, 6_000, 300, 600),
        (1_750, ThermalTuningPhase::Hold, 6_010, 400, 900),
    ] {
        let mut measured = sample(elapsed_ms, phase, temp);
        measured.heater_requested_output_permille = requested;
        measured.heater_output_permille = pwm;
        runtime.update_trial_metrics(&measured, 60);
    }
    let evidence = runtime.candidate_failure_evidence(60, Some(10), Some(10), None, 10_000);
    assert_eq!(evidence.approach_median_output_permille, Some(760));
    assert_eq!(evidence.hold_median_output_permille, Some(400));
    assert_eq!(evidence.hold_p90_output_permille, Some(400));
    assert_eq!(
        runtime.trial_score(60, false, Some(10), Some(10), None)[9],
        0.0
    );
}

#[test]
fn eleven_component_score_uses_local_hold_and_approach_evidence() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.warmup_full_output = true;
    runtime.stable_window_verified_at_ms = Some(14_000);
    runtime.trial_start_elapsed_ms = Some(0);
    runtime.first_band_elapsed_ms = Some(5_200);
    runtime.hold_output_histogram[300] = 3;
    runtime.hold_output_histogram[400] = 1;
    runtime.hold_output_count = 4;
    runtime
        .approach_curve_samples
        .push(ThermalCurveSample {
            elapsed_ms: 1_000,
            temperature_centi_c: 5_000,
        })
        .unwrap();
    runtime.approach_curve_last_elapsed_ms = Some(6_000);
    runtime
        .approach_curve_samples
        .push(ThermalCurveSample {
            elapsed_ms: 6_000,
            temperature_centi_c: 6_000,
        })
        .unwrap();
    assert_eq!(
        runtime.trial_score(60, true, Some(200), Some(150), Some(4_000)),
        [
            0.0, 0.0, 0.0, 0.0, 0.0052, 0.0, 4_000.0, 2.0, 1.5, 100.0, 0.0
        ]
    );
    let low_margin = runtime.trial_score(60, true, Some(200), Some(150), Some(9_500));
    assert_eq!(low_margin[3], 1.0);
    assert_eq!(low_margin[4], 0.2552);
    assert_eq!(low_margin[5], 500.0);
}

#[test]
fn failed_scout_compares_current_and_predicted_before_separate_hold_confirm() {
    let mut runtime = started_single_target_runtime();
    for (elapsed, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_000),
        (300, ThermalTuningPhase::Approach, 5_500),
        (600, ThermalTuningPhase::Hold, 6_400),
        (700, ThermalTuningPhase::Hold, 6_000),
    ] {
        record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
    }
    record_dense_hold(&mut runtime, 800, 12_500, 6_000);
    record_current_trial_sample(&mut runtime, 12_700, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::BatchPredicted
    );
    assert_eq!(runtime.trial_index, 1);
    assert!(runtime.batch_predicted_profile.is_some());

    record_current_trial_sample(&mut runtime, 13_000, ThermalTuningPhase::Cooldown, 3_400);
    for (elapsed, phase, temperature) in [
        (13_100, ThermalTuningPhase::Warmup, 3_000),
        (13_400, ThermalTuningPhase::Approach, 5_500),
        (13_700, ThermalTuningPhase::Hold, 6_000),
    ] {
        record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
    }
    record_dense_hold(&mut runtime, 13_800, 25_700, 6_000);
    record_current_trial_sample(&mut runtime, 25_700, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    assert_eq!(runtime.trial_index, 2);
    assert!(runtime.preview_update_pending);
    assert!(
        runtime
            .events
            .iter()
            .any(|event| { event.reason.as_deref() == Some("candidate_batch_selected_predicted") })
    );

    record_current_trial_sample(&mut runtime, 26_000, ThermalTuningPhase::Cooldown, 3_400);
    for (elapsed, phase, temperature) in [
        (26_100, ThermalTuningPhase::Warmup, 3_000),
        (26_400, ThermalTuningPhase::Approach, 5_500),
        (26_700, ThermalTuningPhase::Hold, 6_000),
    ] {
        record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
    }
    record_dense_hold(&mut runtime, 26_800, 38_800, 6_000);
    record_current_trial_sample(&mut runtime, 38_800, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    assert_eq!(runtime.trial_index, 2);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert_eq!(runtime.seed_source.as_str(), "batch_selected_predicted");
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Running
    );
    record_dense_hold(&mut runtime, 38_900, 86_700, 6_000);
    record_current_trial_sample(&mut runtime, 86_700, ThermalTuningPhase::Hold, 6_000);
    assert_ne!(runtime.accepted_target_mask, 0);
}

#[test]
fn captured_failed_batch_winner_must_change_before_another_scout() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_failed_batch.fixture.json")).unwrap();
    for predicted_wins in [true, false] {
        let mut runtime = started_single_target_runtime();
        let mut current = runtime.candidate_profile.unwrap();
        let point = current.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 1_259;
        point.approach_damping_exponent_permille = 1_000;
        point.approach_lead_ticks = 0;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 50;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 56;
        point.hold_reheat_power_permille = 56;
        point.hold_kp_permille_per_c = 43;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 2;
        point.hold_lead_ticks = 2;
        let mut predicted = current;
        predicted.points[0]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille = 1_500;
        runtime.candidate_profile = Some(current);
        runtime.batch_current_profile = Some(current);
        runtime.batch_predicted_profile = Some(predicted);
        runtime.trial_purpose = ThermalTuningTrialPurpose::BatchCurrent;
        runtime.trial_index = 4;
        for (index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
            for row in trial["samples"].as_array().unwrap() {
                let mut observed = sample(
                    row[0].as_u64().unwrap(),
                    serde_json::from_value(row[2].clone()).unwrap(),
                    row[1].as_i64().unwrap() as i16,
                );
                observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
                observed.heater_output_permille = observed.heater_requested_output_permille;
                runtime.update_trial_metrics(&observed, 60);
            }
            let decision = &trial["decision"];
            let mut score: [f64; 11] = serde_json::from_value(decision["score"].clone()).unwrap();
            // Exercise the other winner's cache without changing physical
            // observations, failure facts or either qualification result.
            if !predicted_wins && index == 0 {
                score = [0.0; 11];
            }
            runtime
                .advance_after_trial(
                    60,
                    TrialEvaluation {
                        score,
                        passes: false,
                        warmup_full: true,
                        max_overshoot_centi_c: Some(
                            decision["maxOvershootCentiC"].as_u64().unwrap() as u16,
                        ),
                        hold_peak_to_peak_centi_c: Some(
                            decision["holdPeakToPeakCentiC"].as_u64().unwrap() as u16,
                        ),
                        settle_time_ms: None,
                        full_speed_limit_ms: 10_000,
                    },
                )
                .unwrap();
        }
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(runtime.trial_purpose, ThermalTuningTrialPurpose::Scout);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.accepted_target_mask, 0);
        // The predicted prefix is unpowered. The current prefix has
        // 17/19 zero Hold samples and qualifies as limited supply, but
        // cannot claim the stricter 90% entrance exception. A losing
        // damping comparison must now use its own braking escape.
        assert_eq!(
            next.approach_damping_exponent_permille,
            if predicted_wins { 1_500 } else { 1_000 }
        );
        assert_eq!(
            next.brake_distance_centi_c,
            if predicted_wins { 1_371 } else { 1_299 }
        );
        assert_eq!(next.hold_power_permille, 56);
        assert_eq!(next.hold_reheat_power_permille, 56);
        assert!(runtime.preview_update_pending);
        assert!(runtime.preview_receipt.is_none());
        assert_eq!(runtime.trial_index, 6);
        assert!(runtime.batch_current_failure_evidence.is_none());
        assert!(runtime.batch_current_profile.is_none());
    }
}

#[test]
fn candidate_scores_rank_lexicographically() {
    let mut current = [0.0; 11];
    let mut predicted = [0.0; 11];
    current[2] = 1.0;
    predicted[2] = 0.0;
    predicted[10] = 1.0e12;
    assert!(score_precedes(&predicted, &current));
    assert!(!score_precedes(&current, &predicted));
}

#[test]
fn failed_rank_does_not_trade_lower_excursion_for_six_ms_sample_jitter() {
    // Real 39d62c7 trial 4/5: the better trace loses six milliseconds
    // at sampled band endpoints, well below the 200ms recording cadence.
    let mut scores = std::vec::Vec::new();
    for (span, excursion, overshoot, p2p) in [(3_644, 25, 175, 215), (3_638, 10, 160, 203)] {
        let mut runtime = started_single_target_runtime();
        runtime.first_band_elapsed_ms = Some(1_000);
        runtime.first_band_exit_centi_c = Some(6_150 + excursion as i16);
        runtime.longest_stable_span_ms = span;
        runtime.max_band_excursion_centi_c = excursion;
        scores.push(runtime.trial_score(60, false, Some(overshoot), Some(p2p), None));
        assert!(runtime.stable_window_verified_at_ms.is_none());
    }
    assert_eq!(scores[0][5], scores[1][5]);
    assert!(score_precedes(&scores[1], &scores[0]));
}

#[test]
fn failed_rank_prefers_lower_excursion_over_fast_unstable_band_entry() {
    // 8f32f34 trial 4/5: stronger damping reaches the band later,
    // so a truncated partial span must not reward the hotter point.
    let mut scores = std::vec::Vec::new();
    for (span, excursion, overshoot, p2p) in [(4_175, 15, 165, 211), (3_658, 6, 156, 202)] {
        let mut runtime = started_single_target_runtime();
        runtime.first_band_elapsed_ms = Some(1_000);
        runtime.first_band_exit_centi_c = Some(6_150 + excursion as i16);
        runtime.longest_stable_span_ms = span;
        runtime.max_band_excursion_centi_c = excursion;
        scores.push(runtime.trial_score(60, false, Some(overshoot), Some(p2p), None));
        assert!(runtime.stable_window_verified_at_ms.is_none());
    }
    assert!(score_precedes(&scores[1], &scores[0]));
    assert!(!score_precedes(&scores[0], &scores[1]));
}

#[test]
fn captured_failed_rank_preserves_full_warmup_before_thermal_progress() {
    // 9485eb63 A60 trials 2/3: a 0.01C smaller peak cannot outweigh
    // the predicted trial's missing full-output Warmup witness.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_warmup_search.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    assert!(runtime.warmup_full_output && !runtime.warmup_full_output_invalid);
    let current_score = runtime.trial_score(60, false, Some(158), Some(161), None);
    replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
    assert!(!runtime.warmup_full_output);
    let predicted_score = runtime.trial_score(60, false, Some(157), Some(179), None);
    assert!(score_precedes(&current_score, &predicted_score));
    runtime.warmup_full_output = true;
    runtime.warmup_full_output_invalid = true;
    assert!(score_precedes(
        &current_score,
        &runtime.trial_score(60, false, Some(157), Some(179), None)
    ));
}

#[test]
fn captured_start_gap_failure_uses_another_effective_search_channel() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_warmup_search.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
    let original = runtime.candidate_profile.unwrap();
    runtime.advance_after_trial(60, evaluation).unwrap();
    let next = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(next.brake_distance_centi_c < 2_500);
    assert!(!same_effective_search_point(
        original.points[0].unwrap(),
        next
    ));
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert!(runtime.armed);
    assert!(!runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("candidate_search_exhausted_no_distinct_safe_proposal")));
}

#[test]
fn search_rejects_a_brake_that_covers_the_allowed_start_gap() {
    let mut runtime = started_single_target_runtime();
    let mut original = runtime.candidate_profile.unwrap();
    original.points[0].as_mut().unwrap().brake_distance_centi_c = 2_452;
    let mut proposed = original;
    proposed.points[0].as_mut().unwrap().brake_distance_centi_c = 2_532;
    runtime.candidate_profile = Some(proposed);
    assert!(!runtime.record_fresh_proposal(Some(original)));
    assert!(runtime.proposed_points.is_empty());
    proposed = original;
    proposed.points[0]
        .as_mut()
        .unwrap()
        .approach_damping_exponent_permille += 100;
    runtime.candidate_profile = Some(proposed);
    assert!(runtime.record_fresh_proposal(Some(original)));
}

#[test]
fn unverified_band_restart_cannot_improve_the_timing_gate_score() {
    let mut runtime = started_single_target_runtime();
    runtime.first_band_elapsed_ms = Some(1_000);
    runtime.max_band_excursion_centi_c = 2;
    runtime.longest_stable_span_ms = 3_738;
    let broken = runtime.trial_score(60, false, Some(152), Some(210), None);
    let just_restarted = runtime.trial_score(60, false, Some(152), Some(210), Some(10_010));
    assert_eq!(broken[2], just_restarted[2]);
    assert!(runtime.stable_window_verified_at_ms.is_none());
}

#[test]
fn captured_failed_rank_prefers_the_longer_calibration_only_window() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_failed_rank.fixture.json")).unwrap();
    let mut scores = std::vec::Vec::new();
    for trial in fixture["trials"].as_array().unwrap() {
        let mut runtime = started_single_target_runtime();
        runtime.warmup_exited_at_ms = trial["warmupExitMs"].as_u64();
        runtime.trial_start_elapsed_ms = runtime.warmup_exited_at_ms;
        for row in trial["samples"].as_array().unwrap() {
            let observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            runtime.update_stable_window(&observed, 60);
            runtime.update_stability_evidence(&observed, 60);
        }
        assert!(runtime.stable_window_verified_at_ms.is_none());
        let score = runtime.trial_score(
            60,
            false,
            Some(trial["overshootCentiC"].as_u64().unwrap() as u16),
            Some(trial["holdP2pCentiC"].as_u64().unwrap() as u16),
            None,
        );
        assert!(score[1] > 0.0);
        scores.push(score);
    }
    assert!(score_precedes(&scores[1], &scores[0]));
}

#[test]
fn captured_eight_target_a60_coast_corrects_warmup_energy_first() {
    // firmware-pps5a-1791463860752, trial 0: Approach requests only
    // 50/70 permille, followed by a completely unpowered Hold rising
    // from 60.21 to 66.05 C. Reducing Approach damping alone cannot
    // remove the heat already deposited during Warmup.
    let mut runtime = started_single_target_runtime();
    let mut profile = runtime.candidate_profile.unwrap();
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 780;
    point.approach_damping_exponent_permille = 1_000;
    point.approach_lead_ticks = 0;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 50;
    point.approach_tail_window_centi_c = 0;
    point.hold_power_permille = 56;
    point.hold_reheat_power_permille = 56;
    point.hold_kp_permille_per_c = 43;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 2;
    point.hold_lead_ticks = 2;
    let previous = *point;
    runtime.candidate_profile = Some(profile);
    for (elapsed, temperature, phase, requested) in [
        (17_504, 5_251, ThermalTuningPhase::Approach, 50),
        (19_281, 5_750, ThermalTuningPhase::Approach, 50),
        (19_517, 5_813, ThermalTuningPhase::Approach, 70),
        (19_719, 5_834, ThermalTuningPhase::Approach, 70),
        (19_946, 5_851, ThermalTuningPhase::Approach, 50),
        (20_870, 5_992, ThermalTuningPhase::Approach, 50),
        (20_931, 6_021, ThermalTuningPhase::Hold, 0),
        (21_267, 6_080, ThermalTuningPhase::Hold, 0),
        (22_419, 6_172, ThermalTuningPhase::Hold, 0),
        (27_049, 6_605, ThermalTuningPhase::Hold, 0),
    ] {
        let mut observed = sample(elapsed, phase, temperature);
        observed.heater_requested_output_permille = requested;
        observed.heater_output_permille = requested;
        runtime.update_trial_metrics(&observed, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(605), Some(585), None, 10_000);
    assert!(!facts.approach_floor_only);
    assert_eq!(facts.approach_output_range, Some((50, 70)));
    assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
    let proposed = tune_point(previous, facts);
    assert!(
        proposed.brake_distance_centi_c > previous.brake_distance_centi_c,
        "coasting after low Approach power must reduce stored Warmup energy: {proposed:?}"
    );
    assert_eq!(proposed.brake_distance_centi_c, 1_780);
    assert_eq!(
        proposed,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_780,
            ..previous
        }
    );
    assert_eq!(reseed_after_hold_confirm(previous, facts), proposed);
    let mut heated = facts;
    heated.approach_output_range = Some((50, 1_000));
    heated.approach_median_output_permille = Some(500);
    assert_eq!(tune_point(previous, heated), proposed);
    heated.overshoot_c = 2.0;
    assert_ne!(tune_point(previous, heated).brake_distance_centi_c, 1_780);
    let mut protected = facts;
    protected.approach_output_range = Some((0, 70));
    assert_ne!(tune_point(previous, protected), proposed);
    runtime.reset_trial_metrics();
    assert!(runtime.model_approach_output_range.is_none());
}

#[test]
fn captured_a60_small_hold_pulses_select_effective_approach_damping() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_limited_hold.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut profile = runtime.candidate_profile.unwrap();
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 1_243;
    point.approach_damping_exponent_permille = 1_000;
    point.approach_lead_ticks = 0;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 50;
    point.approach_tail_window_centi_c = 0;
    point.hold_power_permille = 56;
    point.hold_reheat_power_permille = 56;
    point.hold_kp_permille_per_c = 43;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 2;
    point.hold_lead_ticks = 2;
    let previous = *point;
    runtime.candidate_profile = Some(profile);
    for row in fixture["samples"].as_array().unwrap() {
        let mut observed = sample(
            row[0].as_u64().unwrap(),
            serde_json::from_value(row[2].clone()).unwrap(),
            row[1].as_i64().unwrap() as i16,
        );
        observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
        observed.heater_output_permille = observed.heater_requested_output_permille;
        runtime.update_trial_metrics(&observed, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(262), Some(311), None, 10_000);
    assert!(!facts.hold_entry_pulse_only);
    assert_eq!(facts.hold_sample_count, 22);
    assert_eq!(facts.hold_off_sample_count, 20);
    assert_eq!(facts.hold_max_output_permille, Some(60));
    assert_eq!(facts.hold_max_below_c, Some(0.49));
    let expected = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 2_000,
        ..previous
    };
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
}

#[test]
fn captured_a60_brief_hold_entry_power_still_corrects_warmup_energy() {
    // firmware-pps5a-1791468629135 trial 0: two 6% readings 60 ms
    // apart at Hold entry, then no power throughout the residual rise.
    let mut runtime = started_single_target_runtime();
    let mut profile = runtime.candidate_profile.unwrap();
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 780;
    point.approach_damping_exponent_permille = 1_000;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 50;
    point.approach_tail_window_centi_c = 0;
    point.hold_power_permille = 56;
    point.hold_reheat_power_permille = 56;
    point.hold_kp_permille_per_c = 43;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 2;
    point.hold_lead_ticks = 2;
    let previous = *point;
    runtime.candidate_profile = Some(profile);
    for (elapsed, temperature, phase, requested) in [
        (17_742, 5_223, ThermalTuningPhase::Approach, 50),
        (19_430, 5_733, ThermalTuningPhase::Approach, 50),
        (20_735, 5_945, ThermalTuningPhase::Approach, 50),
        (20_798, 5_963, ThermalTuningPhase::Hold, 60),
        (20_858, 5_960, ThermalTuningPhase::Hold, 60),
        (21_123, 6_009, ThermalTuningPhase::Hold, 0),
        (21_323, 6_071, ThermalTuningPhase::Hold, 0),
        (21_576, 6_106, ThermalTuningPhase::Hold, 0),
        (21_782, 6_124, ThermalTuningPhase::Hold, 0),
        (22_031, 6_144, ThermalTuningPhase::Hold, 0),
        (22_286, 6_181, ThermalTuningPhase::Hold, 0),
        (22_521, 6_192, ThermalTuningPhase::Hold, 0),
        (22_774, 6_206, ThermalTuningPhase::Hold, 0),
        (22_982, 6_224, ThermalTuningPhase::Hold, 0),
        (23_234, 6_245, ThermalTuningPhase::Hold, 0),
        (23_446, 6_283, ThermalTuningPhase::Hold, 0),
        (23_698, 6_321, ThermalTuningPhase::Hold, 0),
        (23_915, 6_341, ThermalTuningPhase::Hold, 0),
        (24_153, 6_331, ThermalTuningPhase::Hold, 0),
        (24_365, 6_409, ThermalTuningPhase::Hold, 0),
        (24_617, 6_429, ThermalTuningPhase::Hold, 0),
        (24_871, 6_442, ThermalTuningPhase::Hold, 0),
        (25_133, 6_461, ThermalTuningPhase::Hold, 0),
        (25_395, 6_475, ThermalTuningPhase::Hold, 0),
        (25_647, 6_506, ThermalTuningPhase::Hold, 0),
        (25_913, 6_496, ThermalTuningPhase::Hold, 0),
        (26_114, 6_533, ThermalTuningPhase::Hold, 0),
        (26_378, 6_560, ThermalTuningPhase::Hold, 0),
        (26_631, 6_546, ThermalTuningPhase::Hold, 0),
        (26_844, 6_553, ThermalTuningPhase::Hold, 0),
        (27_098, 6_572, ThermalTuningPhase::Hold, 0),
        (27_307, 6_564, ThermalTuningPhase::Hold, 0),
        (27_507, 6_552, ThermalTuningPhase::Hold, 0),
        (27_771, 6_576, ThermalTuningPhase::Hold, 0),
    ] {
        let mut observed = sample(elapsed, phase, temperature);
        observed.heater_requested_output_permille = requested;
        observed.heater_output_permille = requested;
        runtime.update_trial_metrics(&observed, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(576), Some(616), None, 10_000);
    assert!(facts.hold_entry_pulse_only);
    let higher_approach = TuneEvidence {
        approach_floor_only: false,
        approach_output_range: Some((50, 700)),
        approach_median_output_permille: Some(700),
        ..facts
    };
    assert_eq!(
        tune_point(previous, higher_approach),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_780,
            ..previous
        }
    );
    let proposed = tune_point(previous, facts);
    assert_eq!(
        proposed,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_780,
            ..previous
        }
    );
    let mut later_reheat = sample(28_000, ThermalTuningPhase::Hold, 6_570);
    later_reheat.heater_requested_output_permille = 60;
    later_reheat.heater_output_permille = 60;
    runtime.update_trial_metrics(&later_reheat, 60);
    let reheated = runtime.candidate_failure_evidence(60, Some(576), Some(616), None, 10_000);
    assert!(!reheated.hold_entry_pulse_only);
    assert_ne!(tune_point(previous, reheated), proposed);
}

#[test]
fn captured_a60_approach_band_jitter_does_not_invent_a_hold_deficit() {
    // firmware-pps5a-1791470799796 trial 3: the first band exit is
    // 58.48 C in Approach, but every physical Hold request is zero
    // while temperature rises to 62.61 C. More reheat is the wrong sign.
    let mut runtime = started_single_target_runtime();
    let mut profile = runtime.candidate_profile.unwrap();
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 1_280;
    point.approach_damping_exponent_permille = 1_000;
    point.approach_lead_ticks = 0;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 50;
    point.approach_tail_window_centi_c = 0;
    point.hold_power_permille = 56;
    point.hold_reheat_power_permille = 56;
    point.hold_kp_permille_per_c = 43;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_on_centi_c = 30;
    point.hold_blend_ticks = 2;
    point.hold_lead_ticks = 2;
    let previous = *point;
    runtime.candidate_profile = Some(profile);
    for (elapsed, temperature, phase, requested) in [
        (413_103, 4_726, ThermalTuningPhase::Approach, 50),
        (413_164, 4_734, ThermalTuningPhase::Approach, 50),
        (413_379, 4_801, ThermalTuningPhase::Approach, 50),
        (415_026, 5_223, ThermalTuningPhase::Approach, 940),
        (415_250, 5_292, ThermalTuningPhase::Approach, 940),
        (415_452, 5_324, ThermalTuningPhase::Approach, 940),
        (415_700, 5_412, ThermalTuningPhase::Approach, 490),
        (415_933, 5_453, ThermalTuningPhase::Approach, 490),
        (416_137, 5_490, ThermalTuningPhase::Approach, 280),
        (416_343, 5_537, ThermalTuningPhase::Approach, 200),
        (416_578, 5_579, ThermalTuningPhase::Approach, 110),
        (416_819, 5_626, ThermalTuningPhase::Approach, 60),
        (417_087, 5_668, ThermalTuningPhase::Approach, 60),
        (417_329, 5_731, ThermalTuningPhase::Approach, 140),
        (417_592, 5_765, ThermalTuningPhase::Approach, 50),
        (417_833, 5_825, ThermalTuningPhase::Approach, 50),
        (418_043, 5_857, ThermalTuningPhase::Approach, 90),
        (418_265, 5_848, ThermalTuningPhase::Approach, 90),
        (418_520, 5_892, ThermalTuningPhase::Approach, 50),
        (418_779, 5_898, ThermalTuningPhase::Approach, 50),
        (418_987, 5_930, ThermalTuningPhase::Approach, 50),
        (419_117, 5_966, ThermalTuningPhase::Hold, 0),
        (419_194, 5_964, ThermalTuningPhase::Hold, 0),
        (419_413, 5_968, ThermalTuningPhase::Hold, 0),
        (419_656, 6_012, ThermalTuningPhase::Hold, 0),
        (419_868, 6_039, ThermalTuningPhase::Hold, 0),
        (420_113, 6_075, ThermalTuningPhase::Hold, 0),
        (420_345, 6_094, ThermalTuningPhase::Hold, 0),
        (420_590, 6_114, ThermalTuningPhase::Hold, 0),
        (420_802, 6_126, ThermalTuningPhase::Hold, 0),
        (421_043, 6_122, ThermalTuningPhase::Hold, 0),
        (421_289, 6_142, ThermalTuningPhase::Hold, 0),
        (421_501, 6_160, ThermalTuningPhase::Hold, 0),
        (421_747, 6_158, ThermalTuningPhase::Hold, 0),
        (421_947, 6_164, ThermalTuningPhase::Hold, 0),
        (422_182, 6_186, ThermalTuningPhase::Hold, 0),
        (422_422, 6_179, ThermalTuningPhase::Hold, 0),
        (422_701, 6_193, ThermalTuningPhase::Hold, 0),
        (422_941, 6_197, ThermalTuningPhase::Hold, 0),
        (423_174, 6_261, ThermalTuningPhase::Hold, 0),
    ] {
        let mut observed = sample(elapsed, phase, temperature);
        observed.heater_requested_output_permille = requested;
        observed.heater_output_permille = requested;
        runtime.update_trial_metrics(&observed, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(261), Some(297), None, 10_000);
    assert_eq!(facts.first_band_exit_centi_c, Some(5_848));
    assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
    assert_eq!(facts.hold_max_below_c, Some(0.36));
    assert_eq!(facts.over_c, 2.61);
    let expected = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 2_000,
        ..previous
    };
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    for attempt in 0..8 {
        let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
        assert_eq!(alternative.hold_reheat_power_permille, 56);
        assert_eq!(alternative.hold_kp_permille_per_c, 43);
        assert!(
            alternative.approach_damping_exponent_permille > 1_000
                || alternative.brake_distance_centi_c > 1_280
        );
    }
    // A genuinely cold physical Hold must retain the opposite feedback.
    let cold_hold = TuneEvidence {
        hold_max_below_c: Some(2.0),
        under_c: 2.0,
        over_c: 0.0,
        overshoot_c: 0.0,
        ..facts
    };
    let recovery = tune_point(previous, cold_hold);
    assert_eq!(recovery.hold_reheat_power_permille, 136);
    assert_eq!(recovery.approach_damping_exponent_permille, 1_000);
}

#[test]
fn captured_floor_only_coast_search_changes_braking_not_unused_damping() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_floor_coast.fixture.json")).unwrap();
    let mut runtime = started_single_target_runtime();
    let mut profile = runtime.candidate_profile.unwrap();
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 1_318;
    point.approach_damping_exponent_permille = 1_050;
    point.approach_lead_ticks = 1;
    point.approach_floor_power_permille = 100;
    point.approach_power_permille = 920;
    point.hold_power_permille = 54;
    point.hold_reheat_power_permille = 54;
    point.hold_kp_permille_per_c = 23;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 3;
    point.hold_lead_ticks = 3;
    point.hold_on_centi_c = 30;
    point.hold_off_centi_c = 5;
    point.hold_entry_centi_c = 90;
    point.hold_exit_centi_c = 200;
    point.overshoot_cutoff_centi_c = 80;
    point.approach_tail_window_centi_c = 0;
    point.warmup_power_permille = 1_000;
    point.warmup_reenter_centi_c = 400;
    let previous = *point;
    runtime.candidate_profile = Some(profile);
    for row in fixture["samples"].as_array().unwrap() {
        let mut observed = sample(
            row[0].as_u64().unwrap(),
            serde_json::from_value(row[2].clone()).unwrap(),
            row[1].as_i64().unwrap() as i16,
        );
        observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
        observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
        runtime.update_trial_metrics(&observed, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(170), Some(212), None, 10_000);
    assert_eq!(facts.hold_sample_count, 15);
    assert_eq!(facts.hold_off_sample_count, 15);
    assert!(facts.first_band_exit_centi_c.is_some_and(|t| t > 6_150));
    assert!(!facts.stable_window_verified);
    assert!(facts.approach_floor_only);
    let proposed = tune_point(previous, facts);
    assert!(proposed.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert_eq!(
        proposed,
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: proposed.brake_distance_centi_c,
            ..previous
        }
    );
    assert_eq!(reseed_after_hold_confirm(previous, facts), proposed);
}

#[test]
fn floor_only_evidence_requires_actual_approach_and_resets_per_trial() {
    for nonfloor in [0, 51, 1_000] {
        let mut runtime = started_single_target_runtime();
        let mut warmup = sample(0, ThermalTuningPhase::Warmup, 3_500);
        warmup.heater_requested_output_permille = 1_000;
        runtime.update_trial_metrics(&warmup, 60);
        for elapsed in [200, 400, 600] {
            let mut observed = sample(elapsed, ThermalTuningPhase::Approach, 5_000);
            observed.heater_requested_output_permille = 50;
            runtime.update_trial_metrics(&observed, 60);
            let facts = runtime.candidate_failure_evidence(60, None, None, None, 10_000);
            assert_eq!(facts.approach_floor_only, elapsed == 600);
        }
        let mut different = sample(800, ThermalTuningPhase::Approach, 5_050);
        different.heater_requested_output_permille = nonfloor;
        runtime.update_trial_metrics(&different, 60);
        assert!(
            !runtime
                .candidate_failure_evidence(60, None, None, None, 10_000)
                .approach_floor_only
        );
        runtime.reset_trial_metrics();
        assert!(
            !runtime
                .candidate_failure_evidence(60, None, None, None, 10_000)
                .approach_floor_only
        );
    }
}

#[test]
fn repeated_high_side_proposal_searches_remaining_effective_parameters() {
    let mut runtime = started_single_target_runtime();
    runtime.first_band_elapsed_ms = Some(5_000);
    runtime.first_band_exit_centi_c = Some(6_159);
    runtime.min_controller_hold_temperature_centi_c = Some(5_950);
    runtime.max_controller_hold_temperature_centi_c = Some(6_159);
    runtime.first_hold_temperature_centi_c = Some(5_950);
    runtime.max_temperature_centi_c = Some(6_159);
    runtime.hold_output_count = 10;
    let original = runtime.candidate_profile;
    runtime.mutate_candidate_for_failure(60, Some(159), Some(209), None, 10_000);
    assert!(runtime.record_fresh_proposal(original));
    let already_proposed = runtime.candidate_profile;
    runtime.candidate_profile = original;
    let score = runtime.trial_score(60, false, Some(159), Some(209), None);
    let evidence = runtime.candidate_failure_evidence(60, Some(159), Some(209), None, 10_000);
    runtime
        .prepare_candidate_batch(60, score, evidence)
        .unwrap();
    assert!(runtime.armed);
    assert_ne!(runtime.batch_predicted_profile, already_proposed);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::BatchPredicted
    );
}

#[test]
fn failed_window_progress_rejects_sparse_or_interrupted_samples() {
    for interval in [500, 1_100] {
        let mut runtime = started_single_target_runtime();
        runtime.warmup_exited_at_ms = Some(0);
        for elapsed in (0..=9_900).step_by(interval) {
            runtime.update_stable_window(&sample(elapsed, ThermalTuningPhase::Hold, 6_000), 60);
        }
        assert_eq!(runtime.longest_stable_span_ms, 0);
        assert!(runtime.stable_window_verified_at_ms.is_none());
    }
    let mut runtime = started_single_target_runtime();
    runtime.warmup_exited_at_ms = Some(0);
    for elapsed in [0, 300, 600, 900, 2_001, 2_301, 2_601] {
        runtime.update_stable_window(&sample(elapsed, ThermalTuningPhase::Hold, 6_000), 60);
    }
    assert_eq!(runtime.longest_stable_span_ms, 900);
}

#[test]
fn missing_240c_hold_searches_braking_instead_of_unused_hold_settings() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.targets_c.push(240).unwrap();
    let mut original = profile_with_targets(&[240]);
    let point = original.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 150;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 1_000;
    point.approach_damping_exponent_permille = 100;
    point.hold_power_permille = 838;
    point.hold_reheat_power_permille = 838;
    point.overshoot_cutoff_centi_c = 300;
    original.sanitize();
    runtime.candidate_profile = Some(original);
    runtime.candidate_hash = hash_profile(&original);
    runtime.deadline_nearest_temperature_centi_c = Some(23_700);
    runtime.deadline_nearest_distance_ms = 0;
    runtime.approach_output_count = 8;
    runtime.approach_output_histogram[1_000] = 8;
    runtime.approach_slopes_c_per_s.push(0.2).unwrap();
    assert!(runtime.prepare_fresh_failure_proposal(240, Some(0), None, None, 5_000));
    let before = original.points[0].unwrap();
    let after = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(after.brake_distance_centi_c < before.brake_distance_centi_c);
    assert_eq!(after.hold_power_permille, before.hold_power_permille);
    assert_eq!(
        after.hold_reheat_power_permille,
        before.hold_reheat_power_permille
    );
    assert_eq!(after.hold_kp_permille_per_c, before.hold_kp_permille_per_c);
    assert!(runtime.events.iter().any(|event| event.reason.as_deref()
        == Some("repeated_proposal_replaced_by_distinct_effective_point")));
}

#[test]
fn captured_model_floor_handoff_recovers_hold_without_legacy_clamps() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_model_hold.fixture.json")).unwrap();
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[240]);
    let wire: ThermalTuningCandidatePoint =
        serde_json::from_value(fixture["point"].clone()).unwrap();
    let previous = ThermalControlProfilePointConfig {
        target_temp_c: wire.target_c,
        brake_distance_centi_c: wire.brake_distance_centi_c,
        warmup_power_permille: wire.warmup_power_permille,
        approach_power_permille: wire.approach_power_permille,
        approach_floor_power_permille: wire.approach_floor_power_permille,
        approach_damping_exponent_permille: wire.approach_damping_exponent_permille,
        approach_tail_window_centi_c: wire.approach_tail_window_centi_c,
        hold_power_permille: wire.hold_power_permille,
        hold_on_centi_c: wire.hold_on_centi_c,
        hold_off_centi_c: wire.hold_off_centi_c,
        hold_kp_permille_per_c: wire.hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick: wire.hold_ki_permille_per_c_tick,
        hold_reheat_power_permille: wire.hold_reheat_power_permille,
        hold_entry_centi_c: wire.hold_entry_centi_c,
        hold_exit_centi_c: wire.hold_exit_centi_c,
        warmup_reenter_centi_c: wire.warmup_reenter_centi_c,
        approach_lead_ticks: wire.approach_lead_ticks,
        hold_lead_ticks: wire.hold_lead_ticks,
        hold_blend_ticks: wire.hold_blend_ticks,
        overshoot_cutoff_centi_c: wire.overshoot_cutoff_centi_c,
    };
    profile.points[0] = Some(previous);
    profile.sanitize();
    runtime.candidate_profile = Some(profile);
    runtime.trial_start_elapsed_ms = fixture["trialStartedAtMs"].as_u64();
    runtime.warmup_exited_at_ms = fixture["warmupExitedAtMs"].as_u64();
    runtime.warmup_full_output = true;
    for row in fixture["samples"].as_array().unwrap() {
        let mut observed = sample(
            row[0].as_u64().unwrap(),
            serde_json::from_value(row[2].clone()).unwrap(),
            row[1].as_i64().unwrap() as i16,
        );
        observed.target_c = 240;
        observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
        observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
        runtime.current_elapsed_ms = observed.elapsed_ms;
        runtime.update_trial_metrics(&observed, 240);
    }
    let facts = runtime.candidate_failure_evidence(240, Some(16), Some(186), None, 5_000);
    assert!(facts.approach_floor_only);
    assert_eq!(facts.approach_median_output_permille, Some(50));
    assert!(facts.first_hold_after_warmup_ms.unwrap() <= 5_000);
    assert_eq!(facts.first_band_exit_centi_c, Some(23_830));
    assert!(runtime.stable_window_verified_at_ms.is_none());
    assert_eq!(runtime.accepted_target_mask, 0);
    let mut expected = previous;
    expected.hold_reheat_power_permille = 913;
    expected.hold_on_centi_c = 25;
    expected.hold_lead_ticks = 2;
    expected.hold_blend_ticks = 1;
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
}

#[test]
fn captured_same_run_hold_failure_survives_a_pre_hold_retry() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_same_run_hold.fixture.json")).unwrap();
    let mut runtime = ThermalTuningRuntime::new();
    runtime.targets_c.push(240).unwrap();
    runtime.armed = true;
    let mut original = profile_with_targets(&[240]);
    let point = original.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 110;
    point.approach_power_permille = 1_000;
    point.approach_floor_power_permille = 833;
    point.approach_damping_exponent_permille = 100;
    point.approach_tail_window_centi_c = 0;
    point.hold_power_permille = 833;
    point.hold_reheat_power_permille = 833;
    point.hold_kp_permille_per_c = 38;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 3;
    point.hold_lead_ticks = 3;
    original.sanitize();
    let previous = original.points[0].unwrap();
    for trial in fixture["trials"].as_array().unwrap() {
        runtime.reset_trial_metrics();
        runtime.candidate_profile = Some(original);
        runtime.candidate_hash = hash_profile(&original);
        runtime.trial_index = trial["trialIndex"].as_u64().unwrap() as u8;
        runtime.trial_purpose = ThermalTuningTrialPurpose::Scout;
        runtime.trial_start_elapsed_ms = trial["trialStartedAtMs"].as_u64();
        runtime.warmup_exited_at_ms = trial["warmupExitedAtMs"].as_u64();
        runtime.target_started_elapsed_ms = runtime.trial_start_elapsed_ms.unwrap();
        runtime.warmup_full_output = true;
        for row in trial["samples"].as_array().unwrap() {
            let mut observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            observed.target_c = 240;
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
            runtime.current_elapsed_ms = observed.elapsed_ms;
            runtime.update_trial_metrics(&observed, 240);
        }
        assert!(runtime.stable_window_verified_at_ms.is_none());
        if runtime.trial_index == 5 {
            assert!(runtime.hold_output_count >= 3);
            assert_eq!(runtime.first_band_exit_centi_c, Some(23_842));
            runtime.finish_trial(runtime.current_elapsed_ms).unwrap();
            assert_eq!(runtime.accepted_target_mask, 0);
        } else {
            assert_eq!(runtime.hold_output_count, 0);
            runtime.proposed_points.clear();
            runtime.proposed_points.push(previous).unwrap();
            for distance in [100, 70, 60] {
                let mut tried = previous;
                tried.brake_distance_centi_c = distance;
                runtime.proposed_points.push(tried).unwrap();
            }
            assert!(runtime.prepare_fresh_failure_proposal(240, Some(0), None, None, 5_000));
            let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
            assert_eq!(
                proposed.brake_distance_centi_c,
                previous.brake_distance_centi_c
            );
            assert_eq!(proposed.approach_damping_exponent_permille, 100);
            assert!(proposed.hold_reheat_power_permille > previous.hold_reheat_power_permille);
            assert_eq!(runtime.hold_output_count, 0);
            assert!(runtime.stable_window_verified_at_ms.is_none());
            assert_eq!(runtime.accepted_target_mask, 0);
            assert!(runtime.events.iter().any(|event| event.reason.as_deref()
                == Some("same_run_hold_failure_trial_5_proposed_distinct_point")));
        }
    }
}

#[test]
fn retained_hold_feedback_rejects_another_point_or_live_failure_direction() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.targets_c.push(240).unwrap();
    let mut original = profile_with_targets(&[240]);
    original.points[0].as_mut().unwrap().brake_distance_centi_c = 100;
    runtime.candidate_profile = Some(original);
    runtime.first_band_elapsed_ms = Some(1_000);
    runtime.first_band_exit_centi_c = Some(23_840);
    runtime.hold_output_count = 8;
    runtime.hold_output_histogram[900] = 8;
    let prior = runtime.candidate_failure_evidence(240, Some(0), Some(150), None, 5_000);
    runtime.remember_observed_hold_failure(240, prior);
    assert_eq!(runtime.observed_hold_failures.len(), 1);
    runtime.reset_trial_metrics();
    runtime.deadline_nearest_temperature_centi_c = Some(23_700);
    let live = runtime.candidate_failure_evidence(240, Some(0), None, None, 5_000);
    for incompatible in [
        TuneEvidence {
            tunable_stop: false,
            ..live
        },
        TuneEvidence {
            deadline_temperature_centi_c: Some(24_200),
            ..live
        },
        TuneEvidence {
            target_c: 220,
            deadline_temperature_centi_c: Some(21_700),
            ..live
        },
    ] {
        assert!(!runtime.prepare_observed_hold_failure_proposal(original, 0, incompatible));
    }
    let mut another_point = original;
    another_point.points[0]
        .as_mut()
        .unwrap()
        .hold_power_permille += 1;
    assert!(!runtime.prepare_observed_hold_failure_proposal(another_point, 0, live));
    assert_eq!(runtime.hold_output_count, 0);
    assert_eq!(runtime.accepted_target_mask, 0);
    runtime.reset_for_run();
    assert!(runtime.observed_hold_failures.is_empty());
}

#[test]
fn retained_hold_feedback_requires_measured_low_failure_and_clears_on_target_change() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.targets_c.extend_from_slice(&[240, 140]).unwrap();
    runtime.candidate_profile = Some(profile_with_targets(&[240, 140]));
    runtime.armed = true;
    runtime.first_band_elapsed_ms = Some(1_000);
    runtime.first_band_exit_centi_c = Some(23_840);
    runtime.hold_output_count = 8;
    let prior = runtime.candidate_failure_evidence(240, Some(0), Some(150), None, 5_000);
    for invalid in [
        TuneEvidence {
            tunable_stop: false,
            ..prior
        },
        TuneEvidence {
            hold_sample_count: 0,
            ..prior
        },
        TuneEvidence {
            overshoot_c: 3.01,
            ..prior
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(24_200),
            ..prior
        },
    ] {
        runtime.remember_observed_hold_failure(240, invalid);
        assert!(runtime.observed_hold_failures.is_empty());
    }
    runtime.remember_observed_hold_failure(240, prior);
    assert_eq!(runtime.observed_hold_failures.len(), 1);
    runtime.advance_target().unwrap();
    assert!(runtime.observed_hold_failures.is_empty());
}

#[test]
fn captured_severe_trajectory_coast_corrects_handoff_above_low_supply_threshold() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("thermal_tuning_trajectory_coast.fixture.json")).unwrap();
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[60]);
    let previous = profile.points[0].as_mut().unwrap();
    previous.brake_distance_centi_c = 780;
    previous.approach_damping_exponent_permille = 1_000;
    previous.hold_power_permille = 56;
    previous.hold_reheat_power_permille = 56;
    runtime.candidate_profile = Some(profile);
    for recorded in fixture["samples"].as_array().unwrap() {
        let phase = serde_json::from_value(recorded["phase"].clone()).unwrap();
        let mut observation = sample(
            recorded["elapsedMs"].as_u64().unwrap(),
            phase,
            recorded["temperatureCentiC"].as_i64().unwrap() as i16,
        );
        observation.heater_requested_output_permille =
            recorded["requestPermille"].as_u64().unwrap() as u16;
        observation.heater_output_permille = observation.heater_requested_output_permille;
        runtime.update_trial_metrics(&observation, 60);
    }
    let facts = runtime.candidate_failure_evidence(60, Some(685), Some(683), None, 10_000);
    assert_eq!(facts.approach_median_output_permille, Some(130));
    assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
    runtime.mutate_candidate_for_failure(60, Some(685), Some(683), None, 10_000);
    let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(proposed.brake_distance_centi_c >= 1_780);
    assert_eq!(proposed.approach_damping_exponent_permille, 1_000);
    assert_eq!(proposed.hold_power_permille, 56);
}

#[test]
fn captured_trajectory_hold_deficit_recovers_hold_before_approach() {
    // B-group 36aa5d68, 220C batch-predicted trial 2. These are the
    // complete recorded Approach/Hold requests, including the one stop.
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[220]);
    let point = profile.points[0].as_mut().unwrap();
    point.brake_distance_centi_c = 100;
    point.approach_damping_exponent_permille = 910;
    point.approach_floor_power_permille = 800;
    point.hold_power_permille = 800;
    point.hold_reheat_power_permille = 800;
    point.hold_lead_ticks = 1;
    point.hold_blend_ticks = 1;
    point.hold_kp_permille_per_c = 235;
    point.hold_ki_permille_per_c_tick = 3;
    runtime.candidate_profile = Some(profile);
    for (elapsed, phase, temperature, output) in [
        (712_878, ThermalTuningPhase::Approach, 21_909, 50),
        (712_959, ThermalTuningPhase::Approach, 21_946, 1_000),
        (713_212, ThermalTuningPhase::Approach, 21_928, 980),
        (713_441, ThermalTuningPhase::Approach, 21_901, 1_000),
        (713_651, ThermalTuningPhase::Approach, 21_916, 1_000),
        (713_881, ThermalTuningPhase::Approach, 21_946, 1_000),
        (714_096, ThermalTuningPhase::Hold, 21_984, 870),
        (714_158, ThermalTuningPhase::Hold, 21_957, 870),
        (714_362, ThermalTuningPhase::Hold, 21_929, 890),
        (714_574, ThermalTuningPhase::Hold, 21_973, 880),
        (714_780, ThermalTuningPhase::Hold, 21_980, 850),
        (715_039, ThermalTuningPhase::Hold, 22_012, 0),
        (715_272, ThermalTuningPhase::Hold, 21_972, 810),
        (715_513, ThermalTuningPhase::Hold, 21_913, 1_000),
        (715_729, ThermalTuningPhase::Hold, 21_965, 900),
        (715_930, ThermalTuningPhase::Hold, 21_935, 900),
        (716_130, ThermalTuningPhase::Hold, 21_927, 990),
        (716_339, ThermalTuningPhase::Hold, 21_902, 1_000),
        (716_593, ThermalTuningPhase::Hold, 21_930, 980),
        (716_808, ThermalTuningPhase::Hold, 21_953, 980),
        (717_023, ThermalTuningPhase::Hold, 21_925, 1_000),
        (717_249, ThermalTuningPhase::Hold, 21_933, 1_000),
        (717_463, ThermalTuningPhase::Hold, 21_899, 1_000),
        (717_681, ThermalTuningPhase::Hold, 21_908, 1_000),
        (717_901, ThermalTuningPhase::Hold, 21_874, 1_000),
        (718_141, ThermalTuningPhase::Hold, 21_874, 1_000),
        (718_354, ThermalTuningPhase::Hold, 21_873, 1_000),
        (718_627, ThermalTuningPhase::Hold, 21_851, 1_000),
        (718_778, ThermalTuningPhase::Hold, 21_838, 1_000),
    ] {
        let mut observation = sample(elapsed, phase, temperature);
        observation.target_c = 220;
        observation.heater_requested_output_permille = output;
        observation.heater_output_permille = output;
        runtime.update_trial_metrics(&observation, 220);
    }
    let facts = runtime.candidate_failure_evidence(220, Some(12), Some(174), None, 5_000);
    assert_eq!(facts.first_hold_after_warmup_ms, Some(1_218));
    assert_eq!(facts.first_band_exit_centi_c, Some(21_838));
    assert_eq!(facts.hold_off_sample_count, 1);
    assert!(facts.hold_median_output_permille.unwrap() >= 950);
    runtime.mutate_candidate_for_failure(220, Some(12), Some(174), None, 5_000);
    let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(proposed.hold_power_permille >= 900);
    assert!(proposed.hold_reheat_power_permille >= 900);
    assert_eq!(proposed.hold_lead_ticks, 0);
    assert_eq!(proposed.brake_distance_centi_c, 100);
    assert_eq!(proposed.approach_damping_exponent_permille, 910);
}

#[test]
fn trajectory_coasting_pulse_corrects_warmup_handoff_before_damping() {
    // A-group 36aa5d68 scout: a brief 20% Approach pulse,
    // mostly 5-10%, then entirely unpowered Hold still rises to 66.71C.
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[60]);
    let initial = profile.points[0].as_mut().unwrap();
    initial.brake_distance_centi_c = 780;
    initial.approach_damping_exponent_permille = 1_000;
    runtime.candidate_profile = Some(profile);
    for (elapsed, phase, temperature, output) in [
        (10_641, ThermalTuningPhase::Warmup, 3_425, 1_000),
        (17_259, ThermalTuningPhase::Approach, 5_247, 50),
        (17_338, ThermalTuningPhase::Approach, 5_257, 50),
        (17_575, ThermalTuningPhase::Approach, 5_341, 140),
        (17_820, ThermalTuningPhase::Approach, 5_439, 50),
        (18_061, ThermalTuningPhase::Approach, 5_521, 110),
        (18_323, ThermalTuningPhase::Approach, 5_558, 180),
        (18_561, ThermalTuningPhase::Approach, 5_631, 110),
        (18_816, ThermalTuningPhase::Approach, 5_698, 200),
        (19_056, ThermalTuningPhase::Approach, 5_767, 100),
        (19_315, ThermalTuningPhase::Approach, 5_825, 170),
        (19_546, ThermalTuningPhase::Approach, 5_856, 80),
        (19_789, ThermalTuningPhase::Approach, 5_901, 70),
        (20_035, ThermalTuningPhase::Approach, 5_925, 80),
        (20_499, ThermalTuningPhase::Hold, 6_009, 0),
        (21_836, ThermalTuningPhase::Hold, 6_183, 0),
        (22_532, ThermalTuningPhase::Hold, 6_264, 0),
        (27_172, ThermalTuningPhase::Hold, 6_671, 0),
    ] {
        let mut observation = sample(elapsed, phase, temperature);
        observation.heater_requested_output_permille = output;
        observation.heater_output_permille = output;
        runtime.update_trial_metrics(&observation, 60);
    }
    runtime.mutate_candidate_for_failure(60, Some(671), Some(705), None, 10_000);
    let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert!(proposed.brake_distance_centi_c > 780);
    assert_eq!(proposed.approach_damping_exponent_permille, 1_000);
}

#[test]
fn legacy_approach_clamps_alone_are_not_distinct_model_proposals() {
    let mut runtime = started_single_target_runtime();
    let original = runtime.candidate_profile.unwrap();
    let point = runtime.candidate_profile.as_mut().unwrap().points[0]
        .as_mut()
        .unwrap();
    point.approach_power_permille += 10;
    point.approach_floor_power_permille += 10;
    assert!(!runtime.record_fresh_proposal(Some(original)));
}

#[test]
fn candidate_preview_identity_uses_the_bounded_production_profile() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[60]);
    profile.points[0]
        .as_mut()
        .unwrap()
        .approach_damping_exponent_permille = 4_050;
    runtime
        .install_candidate_profile(profile, "predicted_from_scout")
        .unwrap();
    let mut effective = runtime.candidate_profile.unwrap();
    effective.sanitize();
    assert_eq!(runtime.candidate_profile, Some(effective));
    assert_eq!(runtime.candidate_hash, hash_profile(&effective));
}

#[test]
fn preview_receipt_rejects_an_unbounded_profile_identity() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[60]);
    profile.points[0]
        .as_mut()
        .unwrap()
        .approach_damping_exponent_permille = 4_050;
    runtime.candidate_profile = Some(profile);
    runtime.candidate_hash = hash_profile(&profile);
    runtime.armed = true;
    assert_eq!(
        runtime.record_preview_applied(),
        Err(ThermalTuningError::IdentityMismatch)
    );
    assert!(!runtime.armed);
    assert!(runtime.preview_receipt.is_none());
}

#[test]
fn high_side_candidate_mutation_bounds_damping_before_hashing() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&[60]);
    profile.points[0]
        .as_mut()
        .unwrap()
        .approach_damping_exponent_permille = 4_000;
    runtime
        .install_candidate_profile(profile, "initial")
        .unwrap();
    for (elapsed, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_500),
        (1_000, ThermalTuningPhase::Approach, 5_850),
        (2_000, ThermalTuningPhase::Hold, 5_950),
        (3_000, ThermalTuningPhase::Hold, 6_153),
    ] {
        runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
    }
    runtime.mutate_candidate_for_failure(60, Some(153), Some(203), None, 10_000);
    let effective = runtime.candidate_profile.unwrap();
    assert_eq!(
        effective.points[0]
            .unwrap()
            .approach_damping_exponent_permille,
        4_000
    );
    assert_eq!(runtime.candidate_hash, hash_profile(&effective));
}

#[test]
fn local_trial_samples_select_conservative_high_side_candidate() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[60]));
    for (elapsed, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_000),
        (1_000, ThermalTuningPhase::Approach, 5_910),
        (2_000, ThermalTuningPhase::Hold, 6_090),
        (3_000, ThermalTuningPhase::Hold, 6_770),
    ] {
        runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
    }
    assert_eq!(runtime.first_hold_temperature_centi_c, Some(6_090));
    assert_eq!(runtime.hold_output_count, 2);
    assert_eq!(runtime.hold_error_sum_centi_c, -860);
    runtime.mutate_candidate_for_failure(60, Some(770), Some(680), None, 10_000);
    let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(tuned.brake_distance_centi_c, 600);
    assert_eq!(tuned.approach_power_permille, 650);
    assert_eq!(tuned.approach_floor_power_permille, 300);
    assert_eq!(tuned.hold_power_permille, 300);
}

#[test]
fn high_temperature_local_band_exit_trims_the_next_preview_candidate() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[160]));
    let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
    for (elapsed, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 12_000),
        (1_000, ThermalTuningPhase::Approach, 15_900),
        (2_000, ThermalTuningPhase::Hold, 15_990),
        (3_000, ThermalTuningPhase::Hold, 16_263),
    ] {
        runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 160);
    }
    runtime.mutate_candidate_for_failure(160, Some(263), Some(373), None, 15_000);
    let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(
        tuned.brake_distance_centi_c,
        previous.brake_distance_centi_c + 300
    );
    assert_eq!(
        tuned.approach_floor_power_permille,
        previous.approach_floor_power_permille - 100
    );
    assert_eq!(tuned.hold_power_permille, previous.hold_power_permille - 50);
    assert!(tuned.approach_power_permille < previous.approach_power_permille);
    assert_eq!(tuned.warmup_power_permille, 1_000);
    assert!(runtime.preview_update_pending);
    assert!(runtime.preview_receipt.is_none());
    assert_eq!(
        runtime.candidate_hash,
        hash_profile(&runtime.candidate_profile.unwrap())
    );
}

#[test]
fn captured_160c_high_and_low_prefixes_select_different_candidate_directions() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/thermal-160c-high-low-replays.json"
    ))
    .unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let wire: ThermalTuningCandidatePoint =
            serde_json::from_value(fixture["point"].clone()).unwrap();
        let mut profile = profile_with_targets(&[160]);
        let previous = profile.points[0].as_mut().unwrap();
        macro_rules! copy_fields {
            ($($field:ident),+ $(,)?) => { $(previous.$field = wire.$field;)+ };
        }
        copy_fields!(
            brake_distance_centi_c,
            warmup_power_permille,
            warmup_reenter_centi_c,
            approach_power_permille,
            approach_floor_power_permille,
            approach_damping_exponent_permille,
            approach_tail_window_centi_c,
            hold_power_permille,
            hold_reheat_power_permille,
            hold_entry_centi_c,
            hold_exit_centi_c,
            hold_on_centi_c,
            hold_off_centi_c,
            overshoot_cutoff_centi_c,
            hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick,
            hold_blend_ticks,
            approach_lead_ticks,
            hold_lead_ticks
        );
        let previous = *previous;
        let decision = &fixture["decision"];
        let captured: ThermalTuningTimingEvidence =
            serde_json::from_value(decision["timing"].clone()).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile);
        runtime.trial_timing = Some(ThermalTuningTimingEvidence::for_point(previous));
        runtime.trial_purpose = serde_json::from_value(decision["purpose"].clone()).unwrap();
        for row in fixture["samples"].as_array().unwrap() {
            let elapsed = row[0].as_u64().unwrap();
            if captured.extended_at_ms.is_some_and(|at| elapsed >= at) {
                runtime.trial_timing.as_mut().unwrap().extended_at_ms = captured.extended_at_ms;
            }
            let phase = serde_json::from_value(row[1].clone()).unwrap();
            let mut observed = sample(elapsed, phase, row[2].as_i64().unwrap() as i16);
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
            runtime.update_trial_metrics(&observed, 160);
        }
        runtime.mutate_candidate_for_failure(
            160,
            decision["maxOvershootCentiC"]
                .as_u64()
                .map(|value| value as u16),
            decision["holdPeakToPeakCentiC"]
                .as_u64()
                .map(|value| value as u16),
            decision["settleTimeMs"].as_u64(),
            decision["hardGateFullSpeedToStableMs"].as_u64().unwrap() as u32,
        );
        let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
        if fixture["side"] == "high" {
            assert_eq!(tuned.approach_floor_power_permille, 500);
            assert_eq!(tuned.hold_power_permille, 550);
            assert_eq!(tuned.brake_distance_centi_c, 1_139);
        } else {
            assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
            assert!(tuned.hold_power_permille >= previous.hold_power_permille);
        }
    }
}

#[test]
fn controller_hold_analysis_keeps_entry_before_qualification_timer() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[60]));
    for (elapsed, phase, temperature) in [
        (1_000, ThermalTuningPhase::Approach, 5_000),
        (2_000, ThermalTuningPhase::Hold, 5_687),
        (3_000, ThermalTuningPhase::Approach, 5_800),
    ] {
        runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
    }
    assert_eq!(runtime.first_controller_hold_elapsed_ms, Some(2_000));
    assert_eq!(runtime.first_hold_temperature_centi_c, Some(5_687));
    assert_eq!(runtime.hold_output_count, 2);
    assert_eq!(runtime.approach_output_count, 0);
    assert!(runtime.first_hold_elapsed_ms.is_none());
    assert!(runtime.min_hold_temperature_centi_c.is_none());
    runtime.update_trial_metrics(&sample(4_000, ThermalTuningPhase::Hold, 5_950), 60);
    runtime.update_trial_metrics(&sample(5_000, ThermalTuningPhase::Hold, 6_060), 60);
    assert_eq!(runtime.first_hold_elapsed_ms, Some(4_000));
    assert_eq!(runtime.hold_output_count, 4);
    assert_eq!(runtime.hold_error_sum_centi_c, 503);
    assert_eq!(runtime.min_controller_hold_temperature_centi_c, Some(5_687));
    assert_eq!(runtime.max_controller_hold_temperature_centi_c, Some(6_060));
    assert_eq!(runtime.min_hold_temperature_centi_c, Some(5_950));
    assert_eq!(runtime.max_hold_temperature_centi_c, Some(6_060));
    runtime.reset_trial_metrics();
    assert!(runtime.first_controller_hold_elapsed_ms.is_none());
    assert!(runtime.min_controller_hold_temperature_centi_c.is_none());
    assert!(runtime.max_controller_hold_temperature_centi_c.is_none());
}

#[test]
fn next_trial_drops_previous_near_target_statistics() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[60]));
    runtime.update_trial_metrics(&sample(1_000, ThermalTuningPhase::Approach, 5_950), 60);
    runtime.update_trial_metrics(&sample(2_000, ThermalTuningPhase::Hold, 6_010), 60);
    assert_eq!(runtime.approach_output_count, 1);
    assert_eq!(runtime.hold_output_count, 1);
    runtime.reset_trial_metrics();
    assert_eq!(runtime.approach_output_count, 0);
    assert_eq!(runtime.hold_output_count, 0);
    assert!(runtime.first_hold_temperature_centi_c.is_none());
    assert!(runtime.previous_sample_at_ms.is_none());
}

#[test]
fn even_approach_slope_samples_use_main_strategy_upper_median() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile_with_targets(&[220]));
    runtime.approach_output_histogram[960] = 1;
    runtime.approach_output_count = 1;
    runtime.approach_slopes_c_per_s.push(0.4).unwrap();
    runtime.approach_slopes_c_per_s.push(1.2).unwrap();
    runtime.mutate_candidate_for_failure(220, Some(0), None, None, 5_000);
    let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
    assert_eq!(tuned.approach_floor_power_permille, 520);
    assert_eq!(tuned.hold_power_permille, 350);
}

#[test]
fn late_approach_curve_keeps_real_hil_class() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "../../tools/flux-purr-devd/tests/fixtures/thermal-60c-late-window-replays.json"
    ))
    .unwrap();
    let fixture = &fixtures[1];
    assert_eq!(fixture["sourceTrialIndex"], 6);
    let mut runtime = ThermalTuningRuntime::new();
    for row in fixture["samples"].as_array().unwrap() {
        let phase: ThermalTuningPhase = serde_json::from_value(row[4].clone()).unwrap();
        if phase == ThermalTuningPhase::Warmup {
            continue;
        }
        let measured = sample(
            row[0].as_u64().unwrap(),
            phase,
            row[1].as_i64().unwrap() as i16,
        );
        runtime.record_approach_curve_sample(&measured);
        if phase == ThermalTuningPhase::Hold {
            break;
        }
    }
    assert!(runtime.approach_tail_count > 0);
    assert!(matches!(
        runtime.strategy_curve_class(60),
        CurveClass::Underpowered
    ));
}

#[test]
fn approach_tail_keeps_exact_error_without_unbounded_sample_storage() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut sample = sample(1_000, ThermalTuningPhase::Approach, 5_000);
    runtime.record_approach_curve_sample(&sample);
    sample.elapsed_ms = 6_000;
    sample.temperature_centi_c = 6_000;
    runtime.record_approach_curve_sample(&sample);
    sample.elapsed_ms = 12_000;
    sample.temperature_centi_c = 5_900;
    runtime.record_approach_curve_sample(&sample);
    assert_eq!(runtime.approach_curve_samples.len(), 2);
    assert_eq!(runtime.approach_tail_count, 1);
    let mean_error = runtime.approach_curve_mean_abs_error_c(60).unwrap();
    assert!((mean_error - 3.5 / 3.0).abs() < 1.0e-9);
}

#[test]
fn target_budget_replaces_the_two_trial_cap() {
    let mut runtime = started_single_target_runtime();
    let mut timestamp = 0;
    for expected_trial in 0..3 {
        if expected_trial > 0 {
            let mut cooling = sample(timestamp, ThermalTuningPhase::Cooldown, 3_400);
            cooling.trial_index = expected_trial;
            cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_sample(cooling).unwrap();
        }
        let mut warmup = sample(timestamp, ThermalTuningPhase::Warmup, 3_000);
        warmup.trial_index = expected_trial;
        warmup.candidate_hash = runtime.candidate_hash.clone().unwrap();
        runtime.record_sample(warmup).unwrap();
        let mut approach = sample(timestamp + 1_000, ThermalTuningPhase::Approach, 4_000);
        approach.trial_index = expected_trial;
        approach.candidate_hash = runtime.candidate_hash.clone().unwrap();
        runtime.record_sample(approach.clone()).unwrap();
        approach.elapsed_ms = timestamp + 11_000;
        approach.temperature_centi_c = 4_500;
        runtime.record_sample(approach).unwrap();
        assert_eq!(runtime.trial_index, expected_trial + 1);
        assert!(runtime.armed);
        timestamp += 12_000;
    }
    let mut budget = sample(
        THERMAL_TUNING_TARGET_BUDGET_MS,
        ThermalTuningPhase::Cooldown,
        3_400,
    );
    budget.trial_index = runtime.trial_index;
    budget.candidate_hash = runtime.candidate_hash.clone().unwrap();
    budget.heater_requested_output_permille = 0;
    budget.heater_output_permille = 0;
    runtime.record_sample(budget).unwrap();
    assert!(!runtime.armed);
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
}

#[test]
fn hold_completion_at_target_budget_cannot_qualify() {
    let mut runtime = started_single_target_runtime();
    for (elapsed_ms, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_000),
        (1_000, ThermalTuningPhase::Approach, 5_500),
        (1_100, ThermalTuningPhase::Hold, 6_000),
    ] {
        runtime
            .record_sample(sample(elapsed_ms, phase, temperature))
            .unwrap();
    }
    record_dense_hold(&mut runtime, 1_200, 11_100, 6_000);
    record_current_trial_sample(
        &mut runtime,
        THERMAL_TUNING_TARGET_BUDGET_MS,
        ThermalTuningPhase::Hold,
        6_000,
    );
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
    assert!(runtime.save_receipt.is_none());
    assert!(runtime.events.iter().any(|event| {
        event.kind == ThermalTuningEventKind::Decision
            && event.decision.as_ref().is_some_and(|decision| {
                decision.stop_reason == "target monotonic tuning budget exhausted"
            })
    }));
}

#[test]
fn removed_240c_target_is_rejected_before_heating() {
    for power_class in [
        ThermalTuningPowerClass::Pps3a,
        ThermalTuningPowerClass::Pps5a,
    ] {
        let mut runtime = ThermalTuningRuntime::new();
        let result = runtime.start(ThermalTuningStart {
            run_id: id("run-removed-target"),
            candidate_id: id("candidate-removed-target"),
            candidate_hash: hash("hash-removed-target"),
            power_class,
            targets: &[240],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[240])),
        });
        assert_eq!(result, Err(ThermalTuningError::InvalidTargets));
        assert!(!runtime.armed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
    }
}

#[test]
fn full_scope_only_seeds_from_already_confirmed_recursive_bounds() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .targets_c
        .extend_from_slice(&THERMAL_TUNING_EXECUTION_ORDER_C)
        .unwrap();
    let mut visited = 0;
    for target in THERMAL_TUNING_EXECUTION_ORDER_C {
        if let Some((lower, upper)) = runtime.required_accepted_bounds(target) {
            assert!(lower < target && target < upper);
            assert_ne!(visited & target_bit(lower), 0);
            assert_ne!(visited & target_bit(upper), 0);
        } else {
            assert!(target == 60 || target == 220);
        }
        visited |= target_bit(target);
    }
    assert_eq!(
        visited.count_ones() as usize,
        THERMAL_TUNING_TARGETS_C.len()
    );
}

#[test]
fn full_scope_prunes_midpoints_without_both_accepted_endpoints() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-pruned"),
            candidate_id: id("candidate-pruned"),
            candidate_hash: hash("hash-pruned"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&THERMAL_TUNING_TARGETS_C)),
        })
        .unwrap();
    runtime.accepted_target_mask = target_bit(60);
    runtime.advance_target().unwrap();
    assert_eq!(runtime.targets_c[runtime.target_index], 220);
    runtime.failed_target_mask = target_bit(220);
    runtime.advance_target().unwrap();
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.failed_target_mask.count_ones(), 7);
    assert_eq!(
        runtime
            .events
            .iter()
            .filter(|event| {
                event.reason.as_deref() == Some("interval_pruned_without_accepted_bounds")
            })
            .count(),
        6
    );
}

#[test]
fn full_scope_midpoint_preserves_its_calibration_seed() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&THERMAL_TUNING_TARGETS_C);
    profile.points[0].as_mut().unwrap().brake_distance_centi_c = 400;
    profile.points[7].as_mut().unwrap().brake_distance_centi_c = 800;
    profile.points[3].as_mut().unwrap().brake_distance_centi_c = 300;
    let calibrated_midpoint = profile.points[3].unwrap();
    let lower_point = profile.points[0].unwrap();
    let upper_point = profile.points[7].unwrap();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-bounds"),
            candidate_id: id("candidate-bounds"),
            candidate_hash: hash("hash-bounds"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    runtime.accepted_target_mask = target_bit(60);
    runtime.advance_target().unwrap();
    runtime.accepted_target_mask |= target_bit(220);
    runtime.advance_target().unwrap();
    assert_eq!(runtime.targets_c[runtime.target_index], 120);
    let snapshot = runtime.candidate_snapshot(Some(120));
    assert_eq!(
        (snapshot.lower_target_c, snapshot.upper_target_c),
        (Some(60), Some(220))
    );
    assert_eq!(snapshot.seed_source, "initial_target_seed");
    let current = runtime.candidate_profile.unwrap();
    assert_eq!(current.points[3], Some(calibrated_midpoint));
    assert_eq!(current.points[0], Some(lower_point));
    assert_eq!(current.points[7], Some(upper_point));
    assert_ne!(runtime.candidate_hash.as_deref(), Some("hash-bounds"));
    assert!(runtime.preview_update_pending);
    assert!(runtime.armed);
}

#[test]
fn sample_elapsed_time_is_relative_to_the_run_not_firmware_uptime() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start_at(
            ThermalTuningStart {
                run_id: id("run-speed-gate"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            },
            99_950,
        )
        .unwrap();
    record_initial_cooldown(&mut runtime, 99_950);
    runtime
        .record_sample(sample(100_000, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(100_050, ThermalTuningPhase::Approach, 3_010))
        .unwrap();
    let elapsed: std::vec::Vec<u64> = runtime
        .events
        .iter()
        .filter_map(|event| event.sample.as_ref().map(|sample| sample.elapsed_ms))
        .collect();
    assert_eq!(elapsed, [0, 50, 100]);
    assert_eq!(runtime.current_elapsed_ms, 100);
}

#[test]
fn verified_window_keeps_its_first_witness_during_later_hold_excursions() {
    // The 240C confirmation's early 10s window is valid, but later Hold
    // leaves the absolute band while staying inside its separate p2p gate.
    let mut runtime = ThermalTuningRuntime::new();
    runtime.warmup_exited_at_ms = Some(1_000);
    for elapsed_ms in (1_000..=11_000).step_by(250) {
        runtime.update_stable_window(&sample(elapsed_ms, ThermalTuningPhase::Hold, 23_950), 240);
    }
    assert_eq!(runtime.stable_window_started_at_ms, Some(1_000));
    assert_eq!(runtime.stable_window_verified_at_ms, Some(11_000));
    for (elapsed_ms, temperature) in [(11_250, 24_000), (11_500, 23_830), (11_750, 23_950)] {
        runtime.update_stable_window(
            &sample(elapsed_ms, ThermalTuningPhase::Hold, temperature),
            240,
        );
    }
    assert_eq!(runtime.stable_window_started_at_ms, Some(1_000));
    assert_eq!(runtime.stable_window_verified_at_ms, Some(11_000));
}

#[test]
fn pps_wait_after_deadline_preserves_only_a_bounded_provisional_window() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    record_dense_hold(&mut runtime, 9_000, 11_200, 6_000);
    let count = runtime.stable_window_sample_count;
    let mut waiting = sample(11_400, ThermalTuningPhase::Hold, 6_000);
    waiting.requested_mv = 20_500;
    waiting.pps_contract.voltage_mv = 20_500;
    waiting.measurement_accepted = Some(false);
    waiting.raw_temperature_centi_c = Some(6_015);
    runtime.record_sample(waiting.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.stable_window_started_at_ms, Some(9_000));
    assert_eq!(
        runtime.stable_window_sample_count, count,
        "held samples are not witnesses"
    );
    assert_eq!(runtime.stable_window_verified_at_ms, None);
    waiting.elapsed_ms = 11_600;
    waiting.measurement_accepted = Some(true);
    runtime.record_sample(waiting).unwrap();
    assert_eq!(runtime.stable_window_started_at_ms, Some(9_000));
    record_dense_hold(&mut runtime, 11_800, 19_000, 6_000);
    assert_eq!(runtime.stable_window_verified_at_ms, Some(19_000));
}

#[test]
fn applied_pps_edge_preserves_a_window_after_an_intermediate_fresh_sample() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    record_dense_hold(&mut runtime, 9_000, 10_800, 6_000);
    let mut requested = sample(11_000, ThermalTuningPhase::Hold, 6_000);
    requested.requested_mv = 20_500;
    // A fresh sample can still carry the old, confirmed contract.
    runtime.record_sample(requested.clone()).unwrap();
    requested.elapsed_ms = 11_122;
    requested.pps_contract.voltage_mv = 20_500;
    runtime.record_sample(requested.clone()).unwrap();
    let count = runtime.stable_window_sample_count;
    requested.elapsed_ms = 11_239;
    requested.measurement_accepted = Some(false);
    requested.raw_temperature_centi_c = Some(6_010);
    runtime.record_sample(requested).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
    assert_eq!(runtime.stable_window_started_at_ms, Some(9_000));
    assert_eq!(runtime.stable_window_sample_count, count);
    assert_eq!(runtime.stable_window_verified_at_ms, None);
    record_dense_hold(&mut runtime, 11_400, 19_000, 6_000);
    record_current_trial_sample(&mut runtime, 19_000, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.stable_window_verified_at_ms, Some(19_000));
}

#[test]
fn rejected_temperature_sample_breaks_qualification_without_disarming() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    runtime
        .record_live_sample(sample(600, ThermalTuningPhase::Hold, 6_000))
        .unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, Some(600));
    let mut wire = serde_json::to_value(sample(650, ThermalTuningPhase::Hold, 6_000)).unwrap();
    wire["measurementAccepted"] = serde_json::json!(false);
    wire["rawTemperatureCentiC"] = serde_json::json!(8_249);
    runtime
        .record_live_sample(serde_json::from_value(wire).unwrap())
        .unwrap();
    assert!(runtime.armed);
    assert_eq!(runtime.current_elapsed_ms, 650);
    assert_eq!(runtime.first_hold_elapsed_ms, None);
    assert_eq!(runtime.stable_window_started_at_ms, None);
    assert_eq!(runtime.max_temperature_centi_c, Some(6_000));
    assert_eq!(runtime.previous_sample_at_ms, None);
    runtime
        .record_sample(sample(900, ThermalTuningPhase::Hold, 6_010))
        .unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, Some(900));
    assert_eq!(runtime.stable_window_started_at_ms, Some(900));
}

#[test]
fn rejected_temperature_keeps_wall_deadline_and_sensor_fault_gate() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    let mut rejected = sample(11_000, ThermalTuningPhase::Approach, 6_000);
    rejected.measurement_accepted = Some(false);
    runtime.record_sample(rejected.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    let decision = runtime
        .events
        .iter()
        .find_map(|event| event.decision.as_ref())
        .unwrap();
    assert_eq!(decision.disposition.as_str(), "rejected");
    assert_eq!(
        decision.stop_reason,
        "full-speed-to-stable budget exhausted"
    );

    let mut runtime = started_single_target_runtime();
    rejected.measurement_valid = false;
    assert_eq!(
        runtime.record_sample(rejected),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!runtime.armed);
}

#[test]
fn hold_confirmation_restarts_after_approach_recovery_without_losing_extrema() {
    let mut runtime = started_single_target_runtime();
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_000);
    record_current_trial_sample(&mut runtime, 300, ThermalTuningPhase::Approach, 5_500);
    record_dense_hold(&mut runtime, 600, 14_400, 6_120);
    let stable_witness = runtime.stable_window_verified_at_ms;
    assert!(stable_witness.is_some());
    // A 220C confirmation's 14.133s + recovery + 41.117s shape, translated
    // to this single-target harness. Neither Hold segment qualifies as 60s.
    record_current_trial_sample(&mut runtime, 14_733, ThermalTuningPhase::Approach, 5_832);
    assert_eq!(runtime.first_hold_elapsed_ms, None);
    assert_eq!(runtime.min_hold_temperature_centi_c, Some(5_832));
    record_dense_hold(&mut runtime, 19_489, 60_589, 6_000);
    record_current_trial_sample(&mut runtime, 60_606, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.armed);
    assert_eq!(runtime.first_hold_elapsed_ms, Some(19_489));
    assert_eq!(runtime.stable_window_verified_at_ms, stable_witness);
    assert_eq!(runtime.max_hold_temperature_centi_c, Some(6_120));
    record_dense_hold(&mut runtime, 60_889, 79_189, 6_000);
    record_current_trial_sample(&mut runtime, 79_489, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.accepted_target_mask, target_bit(60));
}

#[test]
fn rejected_approach_sample_still_ends_the_hold_segment_during_pps_pause() {
    let mut runtime = started_single_target_runtime();
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_000);
    record_current_trial_sample(&mut runtime, 300, ThermalTuningPhase::Approach, 5_500);
    record_dense_hold(&mut runtime, 600, 12_600, 6_000);
    let verified = runtime.stable_window_verified_at_ms;
    let mut recovering = sample(12_800, ThermalTuningPhase::Approach, 6_000);
    recovering.requested_mv = 20_500;
    recovering.pps_contract.voltage_mv = 20_500;
    recovering.measurement_accepted = Some(false);
    recovering.raw_temperature_centi_c = Some(6_010);
    runtime.record_live_sample(recovering).unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, None);
    assert_eq!(runtime.stable_window_verified_at_ms, verified);
    record_current_trial_sample(&mut runtime, 13_000, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.first_hold_elapsed_ms, Some(13_000));
}

#[test]
fn rejected_temperature_restarts_independent_confirmation() {
    let mut runtime = started_single_target_runtime();
    runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    record_dense_hold(&mut runtime, 600, 20_000, 6_000);
    let verified = runtime.stable_window_verified_at_ms;
    assert!(verified.is_some());
    let mut rejected = sample(20_100, ThermalTuningPhase::Hold, 6_000);
    rejected.measurement_accepted = Some(false);
    runtime.record_live_sample(rejected).unwrap();
    assert_eq!(runtime.first_hold_elapsed_ms, None);
    assert_eq!(runtime.stable_window_verified_at_ms, verified);
    record_dense_hold(&mut runtime, 20_400, 61_000, 6_000);
    assert_eq!(runtime.first_hold_elapsed_ms, Some(20_400));
    assert_eq!(runtime.accepted_target_mask, 0);
    assert!(runtime.armed);
    record_dense_hold(&mut runtime, 61_300, 80_200, 6_000);
    record_current_trial_sample(&mut runtime, 80_400, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.accepted_target_mask, target_bit(60));
}

#[test]
fn stable_window_started_before_deadline_can_finish_after_it() {
    let mut runtime = started_single_target_runtime();
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(1_000, ThermalTuningPhase::Approach, 4_000))
        .unwrap();
    runtime
        .record_sample(sample(9_000, ThermalTuningPhase::Approach, 5_900))
        .unwrap();
    record_dense_hold(&mut runtime, 9_100, 18_700, 6_010);
    record_current_trial_sample(&mut runtime, 19_000, ThermalTuningPhase::Hold, 6_010);
    assert!(runtime.armed);
    assert_eq!(runtime.stable_window_started_at_ms, Some(9_000));
    assert_eq!(runtime.stable_window_verified_at_ms, Some(19_000));
}

#[test]
fn preview_receipt_requires_the_applied_canonical_candidate_hash() {
    let profile = profile_with_targets(&[60]);
    let candidate_hash = hash_profile(&profile).unwrap();
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-preview"),
            candidate_id: id("candidate-preview"),
            candidate_hash: candidate_hash.clone(),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    assert!(runtime.preview_update_pending);
    runtime.record_preview_applied().unwrap();
    assert_eq!(
        runtime.preview_receipt.unwrap().ram_applied_hash,
        candidate_hash
    );

    let mut mismatch = ThermalTuningRuntime::new();
    mismatch
        .start(ThermalTuningStart {
            run_id: id("run-mismatch"),
            candidate_id: id("candidate-mismatch"),
            candidate_hash: hash("wrong-hash"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    assert_eq!(
        mismatch.record_preview_applied(),
        Err(ThermalTuningError::IdentityMismatch)
    );
    assert!(!mismatch.armed);
    assert!(mismatch.preview_receipt.is_none());
}

#[test]
fn save_receipt_binds_eeprom_generation_and_verified_bank_hash() {
    for power_class in [
        ThermalTuningPowerClass::Pps3a,
        ThermalTuningPowerClass::Pps5a,
    ] {
        let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
        profile.sanitize();
        let candidate_hash = hash_profile(&profile).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-save"),
                candidate_id: id("candidate-save"),
                candidate_hash: candidate_hash.clone(),
                power_class,
                targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        runtime.record_preview_applied().unwrap();
        runtime.sealed = true;
        runtime.armed = false;
        runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        let delivered = runtime.events_after(None).next_sequence;
        runtime.events_after(delivered);
        let receipt = runtime
            .confirm_save(ThermalTuningSaveConfirmation {
                run_id: "run-save",
                candidate_id: "candidate-save",
                candidate_hash: candidate_hash.as_str(),
                power_class,
                readback_verified: true,
                bank_revision: 17,
                bank_hash: candidate_hash.clone(),
            })
            .unwrap();
        assert_eq!(receipt.bank_revision, 17);
        assert_eq!(receipt.bank_hash, candidate_hash);
        assert_eq!(receipt.bank, power_class);
        assert!(receipt.atomic && receipt.readback_verified);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Passed
        );
    }
}

#[test]
fn save_rejects_delivered_but_unacknowledged_events() {
    let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
    profile.sanitize();
    let candidate_hash = hash_profile(&profile).unwrap();
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-unacked"),
            candidate_id: id("candidate-unacked"),
            candidate_hash: candidate_hash.clone(),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    runtime.record_preview_applied().unwrap();
    runtime.sealed = true;
    runtime.armed = false;
    runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
    runtime.events_after(None);

    assert_eq!(
        runtime.validate_save_identity(
            "run-unacked",
            "candidate-unacked",
            candidate_hash.as_str(),
            ThermalTuningPowerClass::Pps5a,
        ),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert_eq!(
        runtime.review_disposition,
        ThermalTuningReviewDisposition::ReviewIncomplete
    );
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
    assert!(runtime.save_receipt.is_none());
    assert!(
        runtime
            .events
            .iter()
            .any(|event| { event.reason.as_deref() == Some("event_ack_incomplete") })
    );
}

#[test]
fn admits_pps3a_with_the_same_disarmed_preparation() {
    let mut runtime = ThermalTuningRuntime::new();
    let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
    profile.sanitize();
    let result = runtime.start(ThermalTuningStart {
        run_id: id("run-3a"),
        candidate_id: id("candidate-3a"),
        candidate_hash: hash_profile(&profile).unwrap(),
        power_class: ThermalTuningPowerClass::Pps3a,
        targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
        pps_ready: true,
        thermal_model_available: true,
        calibration_conflict: false,
        candidate_profile: Some(profile),
    });
    assert_eq!(result, Ok(()));
    assert!(runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Running
    );
}

#[test]
fn three_amp_sampling_requires_its_confirmed_current_and_run_identity() {
    for current_ma in [3_000, 2_999] {
        let mut profile = profile_with_targets(&[60]);
        profile.sanitize();
        let candidate_hash = hash_profile(&profile).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-3a"),
                candidate_id: id("candidate-hold"),
                candidate_hash: candidate_hash.clone(),
                power_class: ThermalTuningPowerClass::Pps3a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        let mut observed = sample(300, ThermalTuningPhase::Cooldown, 3_400);
        observed.candidate_hash = candidate_hash;
        observed.pps_contract.kind = ThermalTuningPowerClass::Pps3a;
        observed.pps_contract.current_ma = current_ma;
        observed.heater_requested_output_permille = 0;
        observed.heater_output_permille = 0;
        let result = runtime.record_sample(observed);
        assert_eq!(result.is_ok(), current_ma == 3_000);
        assert_eq!(runtime.armed, current_ma == 3_000);
    }
}

#[test]
fn seal_cannot_promote_an_unfinished_trial() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-5a"),
            candidate_id: id("candidate-5a"),
            candidate_hash: hash("hash-5a"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    assert_eq!(runtime.seal(), Err(ThermalTuningError::InvalidTransition));
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Running
    );
    assert_eq!(
        runtime.confirm_save(ThermalTuningSaveConfirmation {
            run_id: "run-5a",
            candidate_id: "candidate-5a",
            candidate_hash: "hash-5a",
            power_class: ThermalTuningPowerClass::Pps5a,
            readback_verified: true,
            bank_revision: 1,
            bank_hash: hash("hash-5a"),
        }),
        Err(ThermalTuningError::NotSealed)
    );
}

#[test]
fn candidate_seal_requires_prior_ack_and_creates_a_new_ack_boundary() {
    let mut runtime = ThermalTuningRuntime::new();
    let profile = profile_with_targets(&[60]);
    let candidate_hash = hash_profile(&profile).unwrap();
    runtime.run_id = Some(id("run-seal"));
    runtime.candidate_id = Some(id("candidate-seal"));
    runtime.candidate_hash = Some(candidate_hash.clone());
    runtime.candidate_profile = Some(profile);
    runtime.power_class = Some(ThermalTuningPowerClass::Pps5a);
    runtime.phase = ThermalTuningPhase::Completed;
    runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
    runtime.preview_receipt = Some(ThermalTuningPreviewReceipt {
        run_id: id("run-seal"),
        candidate_id: id("candidate-seal"),
        candidate_hash: candidate_hash.clone(),
        power_class: ThermalTuningPowerClass::Pps5a,
        ram_applied_hash: candidate_hash,
    });
    runtime.push_event(
        ThermalTuningEventKind::Decision,
        Some(60),
        Some(0),
        None,
        Some("target completed"),
    );
    let first_page = runtime.events_after(None);
    assert_eq!(runtime.seal(), Err(ThermalTuningError::InvalidTransition));
    assert!(!runtime.sealed);
    runtime.events_after(first_page.next_sequence);
    runtime.seal().unwrap();
    assert!(runtime.sealed);
    assert_eq!(runtime.trace().last_sequence, Some(1));
    runtime.seal().unwrap();
    assert_eq!(runtime.trace().last_sequence, Some(1));
    assert_eq!(runtime.cancel("owner_cancel"), Ok(()));
    assert!(!runtime.sealed);
}

#[test]
fn unsealed_ready_candidate_can_be_canceled() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
    assert_eq!(runtime.cancel("owner_cancel"), Ok(()));
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Canceled
    );
}

#[test]
fn unsafe_sample_disarms_runtime() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run"),
            candidate_id: id("candidate"),
            candidate_hash: hash("hash"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    let sample = ThermalTuningSample {
        sequence: 1,
        elapsed_ms: 300,
        target_c: 60,
        trial_index: 0,
        candidate_id: id("candidate"),
        candidate_hash: hash("hash"),
        temperature_centi_c: 2500,
        raw_temperature_centi_c: None,
        vin_mv: 12000,
        pps_contract: ThermalTuningPpsContract {
            voltage_mv: 12000,
            current_ma: 5000,
            kind: ThermalTuningPowerClass::Pps3a,
            confirmed: true,
        },
        requested_mv: 12000,
        pps_power_ceiling_mv: 12_000,
        heater_requested_output_permille: 1000,
        heater_output_permille: 0,
        measurement_valid: true,
        measurement_accepted: None,
        phase: ThermalTuningPhase::Warmup,
        fan: None,
    };
    assert_eq!(
        runtime.record_sample(sample),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
}

#[test]
fn milliwatts_in_voltage_ceiling_disarms_runtime() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    let mut invalid = sample(300, ThermalTuningPhase::Warmup, 2500);
    invalid.pps_power_ceiling_mv = 105_000;

    assert_eq!(
        runtime.record_sample(invalid),
        Err(ThermalTuningError::InvalidTransition)
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
}

#[test]
fn completes_sixty_second_hold_and_keeps_zero_based_paged_trace() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();

    record_initial_cooldown(&mut runtime, 0);
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    let mut after_sequence = None;
    let mut trial_open_sequence = None;
    let mut closed_trial = None;
    for elapsed_ms in (600..=12_600).step_by(300) {
        runtime
            .record_sample(sample(elapsed_ms, ThermalTuningPhase::Hold, 6_000))
            .unwrap();
    }
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::HoldConfirm
    );
    record_initial_cooldown(&mut runtime, 12_900);
    let mut warmup = sample(13_000, ThermalTuningPhase::Warmup, 3_000);
    warmup.trial_index = 1;
    runtime.record_sample(warmup).unwrap();
    let mut approach = sample(13_300, ThermalTuningPhase::Approach, 5_500);
    approach.trial_index = 1;
    runtime.record_sample(approach).unwrap();
    for elapsed_ms in (13_600..=73_600).step_by(300) {
        let mut hold = sample(elapsed_ms, ThermalTuningPhase::Hold, 6_000);
        hold.trial_index = 1;
        runtime.record_sample(hold).unwrap();
        loop {
            let page = runtime.events_after(after_sequence);
            if page.events.is_empty() {
                break;
            }
            trial_open_sequence = trial_open_sequence.or(first_open_trial(&page.events));
            closed_trial = closed_trial.or(closed_confirmation_trial(&page.events));
            after_sequence = page.next_sequence;
        }
    }

    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::CandidateReady
    );
    assert!(!runtime.sealed);
    assert_eq!(runtime.trace().first_sequence, Some(0));
    assert!(runtime.trace().last_sequence.unwrap_or_default() > 200);
    assert_eq!(trial_open_sequence, Some(2));
    assert!(closed_trial.is_some_and(|trial: ThermalTuningTrialRecord| {
        trial.end_sequence.is_some()
            && trial.start_elapsed_ms == 13_000
            && trial.end_elapsed_ms == Some(73_600)
    }));
    assert_single_target_save_gates(&mut runtime);
}

fn first_open_trial(events: &[ThermalTuningEvent]) -> Option<u32> {
    events
        .iter()
        .find(|event| {
            event.kind == ThermalTuningEventKind::CandidateTrial
                && event
                    .trial
                    .as_ref()
                    .is_some_and(|trial| trial.end_sequence.is_none())
        })
        .map(|event| event.sequence)
}

fn closed_confirmation_trial(events: &[ThermalTuningEvent]) -> Option<ThermalTuningTrialRecord> {
    events
        .iter()
        .filter_map(|event| event.trial.as_ref())
        .find(|trial| {
            trial.end_sequence.is_some() && trial.purpose == ThermalTuningTrialPurpose::HoldConfirm
        })
        .cloned()
}

fn assert_single_target_save_gates(runtime: &mut ThermalTuningRuntime) {
    let digest_before_ack = runtime.trace().digest;
    runtime.events_after(runtime.trace().last_sequence);
    assert_eq!(runtime.trace().digest, digest_before_ack);
    assert!(!runtime.trace().gap);
    assert_eq!(
        runtime.confirm_save(ThermalTuningSaveConfirmation {
            run_id: "run-hold",
            candidate_id: "candidate-hold",
            candidate_hash: "hash-hold",
            power_class: ThermalTuningPowerClass::Pps5a,
            readback_verified: true,
            bank_revision: 1,
            bank_hash: hash("hash-hold"),
        }),
        Err(ThermalTuningError::NotSealed)
    );
    runtime.preview_receipt = Some(ThermalTuningPreviewReceipt {
        run_id: id("run-hold"),
        candidate_id: id("candidate-hold"),
        candidate_hash: hash("hash-hold"),
        power_class: ThermalTuningPowerClass::Pps5a,
        ram_applied_hash: hash("hash-hold"),
    });
    runtime.preview_update_pending = false;
    runtime.seal().unwrap();
    assert!(runtime.sealed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Sealed);
    assert_ne!(runtime.trace().digest, digest_before_ack);
    let seal_page = runtime.events_after(None);
    assert!(seal_page.events.iter().any(|event| {
        event.reason.as_deref() == Some("candidate sealed for explicit save confirmation")
    }));
    runtime.events_after(seal_page.next_sequence);
    assert_eq!(
        runtime.confirm_save(ThermalTuningSaveConfirmation {
            run_id: "run-hold",
            candidate_id: "candidate-hold",
            candidate_hash: "hash-hold",
            power_class: ThermalTuningPowerClass::Pps5a,
            readback_verified: true,
            bank_revision: 1,
            bank_hash: hash("hash-hold"),
        }),
        Err(ThermalTuningError::InvalidTargets)
    );
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
}

#[test]
fn caller_failure_context_precedes_canonical_disarmed_failure() {
    let mut runtime = started_single_target_runtime();
    let before = runtime.trace().last_sequence.unwrap();
    let digest_before = runtime.trace().digest;
    runtime.fail_with_context(
        "pd_contract_lost",
        &[
            "pd:power p=fault f=reset",
            "pd:power a=none r=none ticket=0",
        ],
    );
    let tail = &runtime.events[runtime.events.len() - 3..];
    assert_eq!(tail[0].sequence, before + 1);
    assert_eq!(tail[0].reason.as_deref(), Some("pd:power p=fault f=reset"));
    assert_eq!(
        tail[1].reason.as_deref(),
        Some("pd:power a=none r=none ticket=0")
    );
    assert_eq!(tail[2].reason.as_deref(), Some("pd_contract_lost"));
    assert!(
        tail.iter()
            .all(|event| event.kind == ThermalTuningEventKind::Safety)
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
    assert_eq!(
        runtime.review_disposition,
        ThermalTuningReviewDisposition::Complete
    );
    assert!(runtime.trace().complete);
    assert_ne!(runtime.trace().digest, digest_before);
    let digest = runtime.trace().digest;
    runtime.recompute_trace_digest();
    assert_eq!(runtime.trace().digest, digest);
}

#[test]
fn caller_failure_context_overflow_stays_review_incomplete() {
    let mut runtime = started_single_target_runtime();
    while runtime.events.len() < THERMAL_TUNING_EVENT_CAPACITY - 1 {
        runtime.push_phase_transition_event("retained unacknowledged event");
    }
    runtime.fail_with_context(
        "pd_contract_lost",
        &[
            "pd:power p=fault f=reset",
            "pd:power a=none r=none ticket=0",
        ],
    );
    assert!(!runtime.armed);
    assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
    assert_eq!(
        runtime.review_disposition,
        ThermalTuningReviewDisposition::ReviewIncomplete
    );
    assert!(runtime.trace().gap);
    assert!(!runtime.trace().complete);
}

#[test]
fn event_ack_cannot_skip_undelivered_evidence() {
    let mut runtime = started_single_target_runtime();
    let first_page = runtime.events_after(None);
    let delivered = first_page.next_sequence.unwrap();
    assert!(runtime.trace().last_sequence.unwrap() >= delivered);

    let skipped = runtime.events_after(Some(delivered + 1));
    assert!(!runtime.armed);
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
    assert_eq!(
        runtime.review_disposition,
        ThermalTuningReviewDisposition::ReviewIncomplete
    );
    assert!(skipped.trace.gap);
    assert!(runtime.events.iter().any(|event| {
        event.kind == ThermalTuningEventKind::Safety
            && event.reason.as_deref() == Some("event_ack_skipped_unconfirmed")
    }));
}

#[test]
fn accepted_target_cools_before_the_next_target_arms() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60, 80],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60, 80])),
        })
        .unwrap();
    record_initial_cooldown(&mut runtime, 0);
    for (elapsed_ms, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_000),
        (300, ThermalTuningPhase::Approach, 5_500),
        (600, ThermalTuningPhase::Hold, 6_000),
    ] {
        runtime
            .record_sample(sample(elapsed_ms, phase, temperature))
            .unwrap();
    }
    record_dense_hold(&mut runtime, 700, 12_400, 6_000);
    record_current_trial_sample(&mut runtime, 12_600, ThermalTuningPhase::Hold, 6_000);
    record_initial_cooldown(&mut runtime, 12_900);
    for (elapsed_ms, phase, temperature) in [
        (13_000, ThermalTuningPhase::Warmup, 3_000),
        (13_300, ThermalTuningPhase::Approach, 5_500),
        (13_600, ThermalTuningPhase::Hold, 6_000),
    ] {
        let mut next = sample(elapsed_ms, phase, temperature);
        next.trial_index = 1;
        runtime.record_sample(next).unwrap();
    }
    record_dense_hold(&mut runtime, 13_700, 73_400, 6_000);
    record_current_trial_sample(&mut runtime, 73_600, ThermalTuningPhase::Hold, 6_000);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.target_index, 1);
    let mut cooling = sample(73_900, ThermalTuningPhase::Warmup, 5_000);
    cooling.target_c = 80;
    cooling.heater_requested_output_permille = 0;
    cooling.heater_output_permille = 0;
    runtime.record_sample(cooling.clone()).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    cooling.elapsed_ms = 74_200;
    cooling.temperature_centi_c = 3_900;
    runtime.record_sample(cooling).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    assert_eq!(runtime.trial_index, 0);
}

#[test]
fn failed_trial_changes_candidate_only_at_cooldown_boundary() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    record_initial_cooldown(&mut runtime, 0);
    for (elapsed_ms, phase, temperature) in [
        (0, ThermalTuningPhase::Warmup, 3_000),
        (300, ThermalTuningPhase::Approach, 5_500),
        (600, ThermalTuningPhase::Hold, 6_000),
    ] {
        runtime
            .record_sample(sample(elapsed_ms, phase, temperature))
            .unwrap();
    }
    record_dense_hold(&mut runtime, 700, 10_600, 6_000);
    record_dense_hold(&mut runtime, 10_700, 12_500, 6_400);
    record_current_trial_sample(&mut runtime, 12_600, ThermalTuningPhase::Hold, 6_400);
    assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
    assert_eq!(runtime.trial_index, 1);
    assert!(runtime.preview_update_pending);
    assert_ne!(runtime.candidate_hash.as_deref(), Some("hash-hold"));
    let mut cooling = sample(60_900, ThermalTuningPhase::Warmup, 3_400);
    cooling.trial_index = 1;
    cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
    cooling.heater_requested_output_permille = 0;
    cooling.heater_output_permille = 0;
    runtime.record_sample(cooling).unwrap();
    assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
}

#[test]
fn hold_confirmation_uses_elapsed_time_not_sample_count() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    record_initial_cooldown(&mut runtime, 0);
    runtime
        .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
        .unwrap();
    runtime
        .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
        .unwrap();
    runtime
        .record_sample(sample(600, ThermalTuningPhase::Hold, 6_000))
        .unwrap();
    record_dense_hold(&mut runtime, 700, 10_600, 6_000);
    for _ in 0..100 {
        runtime
            .record_sample(sample(10_900, ThermalTuningPhase::Hold, 6_000))
            .unwrap();
    }
    assert!(runtime.armed);
    assert!(!runtime.sealed);
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Running
    );
}

#[test]
fn sample_candidate_identity_mismatch_disarms() {
    let mut runtime = ThermalTuningRuntime::new();
    runtime
        .start(ThermalTuningStart {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        })
        .unwrap();
    let mut wrong = sample(300, ThermalTuningPhase::Warmup, 3_000);
    wrong.candidate_hash = hash("another-candidate");
    assert_eq!(
        runtime.record_sample(wrong),
        Err(ThermalTuningError::IdentityMismatch)
    );
    assert!(!runtime.armed);
    assert_eq!(
        runtime.terminal_disposition,
        ThermalTuningTerminalDisposition::Failed
    );
}
