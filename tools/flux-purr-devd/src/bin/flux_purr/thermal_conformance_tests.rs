use super::super::{
    ThermalApproachGuardTracker, ThermalReplayStageSample, thermal_replay_stage_analysis,
};
use super::*;
use flux_purr_firmware::{
    control_plane::ThermalControlProfileWire,
    thermal_tuning::{
        ThermalTuningDecisionRecord, ThermalTuningPhase, ThermalTuningPowerClass,
        ThermalTuningPpsContract, ThermalTuningRuntime, ThermalTuningSample, ThermalTuningStart,
        ThermalTuningTrialPurpose, TuningId, hash_profile,
    },
};

// Captured after the physical fan repair. No external current/power measurements
// are synthesized; strategy analysis uses the recorded logical request. MOS PWM
// remains an independent runtime sample and does not become a power estimate.
#[test]
fn firmware_160c_prefix_scores_and_approved_high_side_candidates() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-160c-live-rtd-replays.json"
    ))
    .unwrap();
    let mut differences = Vec::new();
    for fixture in fixtures {
        let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
        let rows = fixture["samples"].as_array().unwrap();
        assert!(!conditional_replay_continues(
            &profile,
            rows,
            ThermalTuningTrialPurpose::Scout
        ));
        let (external, failure, stage) =
            external_candidate_and_stage(&profile, rows, "full_speed_to_stable_timeout");
        // The approved 160–240C strategy deliberately uses the conservative
        // branch for these high-side failures. Keep exact external score and
        // unaffected-candidate comparisons; assert the four full golden
        // vectors separately instead of claiming universal external parity.
        let golden = match fixture["sourceTrialIndex"].as_u64().unwrap() {
            0 => Some((1259, 580, 660)),
            6 => Some((1079, 760, 840)),
            9 => Some((1199, 840, 920)),
            12 => Some((1319, 900, 980)),
            _ => None,
        };
        let expected = if let Some((brake, floor, ceiling)) = golden {
            assert_eq!(failure, "stable_window_broke_high");
            let mut point = thermal_candidate_profile_from_value(profile.clone()).points[0];
            point.brake_distance_centi_c = brake;
            point.approach_floor_power_permille = floor;
            point.approach_power_permille = ceiling;
            point.hold_power_permille = 550;
            point.hold_reheat_power_permille = 550;
            point.hold_entry_centi_c = 370;
            point.hold_exit_centi_c = 338;
            point.overshoot_cutoff_centi_c = 107;
            point.hold_kp_permille_per_c = 8;
            assert_ne!(point, external, "high-side exception must not accelerate");
            point
        } else {
            external
        };
        let actual = firmware_candidate(profile.clone(), rows);
        if actual != expected {
            differences.push(format!(
                "trial {} ({failure}): firmware {actual:?}; external {expected:?}",
                fixture["sourceTrialIndex"]
            ));
        }
        let (_, _, decisions) = replay_firmware_trials_and_capture_decisions(
            profile.clone(),
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let (_, count) = replay_firmware_trials_and_capture(
            profile.clone(),
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let (_, _, prefix_stage) =
            external_candidate_and_stage(&profile, &rows[..count], "full_speed_to_stable_timeout");
        assert_failed_prefix_score(&decisions[0], &prefix_stage, &rows[..count], 160);
        let expected_score = external_score_for_stage(&stage, rows, 160);
        let live_score: [f64; 11] =
            serde_json::from_value(fixture["sourceDecisionScore"].clone()).unwrap();
        for (dimension, (actual, expected)) in
            live_score.into_iter().zip(expected_score).enumerate()
        {
            if (actual - expected).abs() > 0.001 {
                differences.push(format!(
                    "trial {} live score[{dimension}]: device {actual}; external {expected}",
                    fixture["sourceTrialIndex"]
                ));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn firmware_candidate_matches_external_lower_band_hil_replay() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-scout-lower-band.json"
    ))
    .unwrap();
    let profile_value = json!({
        "settings": fixture["settings"], "points": [fixture["point"]]
    });
    if conditional_replay_continues(
        &profile_value,
        fixture["samples"].as_array().unwrap(),
        ThermalTuningTrialPurpose::Scout,
    ) {
        return;
    }
    let (expected, failure_class) =
        external_candidate(&profile_value, fixture["samples"].as_array().unwrap());
    assert_eq!(failure_class, "missed_lower_band_before_limit");
    let actual = firmware_candidate(profile_value, fixture["samples"].as_array().unwrap());
    assert_eq!(
        actual, expected,
        "same real scout evidence must produce the external strategy's candidate"
    );
}

#[test]
fn policy2_60c_overshoot_replay_selects_the_frozen_reduced_energy_candidate() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-policy2-overshoot.json"
    ))
    .unwrap();
    let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
    let rows = fixture["samples"].as_array().unwrap();
    let previous = thermal_candidate_profile_from_value(profile.clone()).points[0];
    let (expected, failure) =
        external_candidate_with_stop(&profile, rows, "full_speed_to_stable_timeout");
    let actual = firmware_candidate(profile, rows);
    assert_eq!(failure, "stable_window_broke_high");
    assert_eq!(actual, expected);
    assert!(actual.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(actual.approach_power_permille < previous.approach_power_permille);
    assert!(actual.approach_floor_power_permille < previous.approach_floor_power_permille);
    assert_eq!(actual.brake_distance_centi_c, 1_400);
    assert_eq!(actual.approach_power_permille, 350);
    assert_eq!(actual.approach_floor_power_permille, 100);
    assert_eq!(actual.hold_power_permille, 100);
    assert_eq!(actual.hold_reheat_power_permille, 120);
}

#[test]
fn firmware_candidate_matches_external_late_window_hil_replays() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-late-window-replays.json"
    ))
    .unwrap();
    let mut differences = Vec::new();
    for fixture in fixtures {
        let profile_value = json!({
            "settings": fixture["settings"], "points": [fixture["point"]]
        });
        let rows = fixture["samples"].as_array().unwrap();
        if conditional_replay_continues(&profile_value, rows, ThermalTuningTrialPurpose::Scout) {
            continue;
        }
        let (expected, failure_class) = external_candidate(&profile_value, rows);
        let actual = firmware_candidate(profile_value, rows);
        if actual != expected {
            differences.push(format!(
                "trial {} ({failure_class}): firmware {actual:?}; external {expected:?}",
                fixture["sourceTrialIndex"]
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn firmware_candidate_matches_external_hard_deadline_hil_replays() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-hard-deadline-replays.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
        let rows = fixture["samples"].as_array().unwrap();
        if conditional_replay_continues(&profile, rows, ThermalTuningTrialPurpose::Scout) {
            continue;
        }
        let (mut expected, failure) =
            external_candidate_with_stop(&profile, rows, "full_speed_to_stable_timeout");
        if fixture["sourceTrialIndex"] == 3 {
            // Approved Hold-recovery exception: keep Approach unchanged and
            // recover the observed low-side Hold sooner.
            expected = thermal_candidate_profile_from_value(profile.clone()).points[0];
            expected.hold_reheat_power_permille += 80;
            expected.hold_on_centi_c -= 5;
            expected.hold_kp_permille_per_c += 20;
        }
        let actual = firmware_candidate(profile, rows);
        assert_eq!(
            actual, expected,
            "trial {} ({failure})",
            fixture["sourceTrialIndex"]
        );
    }
}

#[test]
fn firmware_candidate_matches_external_100ms_startup_hil_replays() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-100ms-startup-replays.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
        let rows = fixture["samples"].as_array().unwrap();
        if conditional_replay_continues(&profile, rows, ThermalTuningTrialPurpose::Scout) {
            continue;
        }
        let (expected, failure) =
            external_candidate_with_stop(&profile, rows, "full_speed_to_stable_timeout");
        let actual = firmware_candidate(profile, rows);
        assert_eq!(
            actual, expected,
            "100ms startup trial {} ({failure})",
            fixture["sourceTrialIndex"]
        );
    }
}

#[test]
fn firmware_100ms_startup_scores_use_the_complete_failed_prefix() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-100ms-startup-replays.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
        let rows = fixture["samples"].as_array().unwrap();
        if conditional_replay_continues(&profile, rows, ThermalTuningTrialPurpose::Scout) {
            continue;
        }
        let (_, count) = replay_firmware_trials_and_capture(
            profile.clone(),
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let rows = &rows[..count];
        let (_, _, stage) =
            external_candidate_and_stage(&profile, rows, "full_speed_to_stable_timeout");
        let (_, _, decisions) = replay_firmware_trials_and_capture_decisions(
            profile,
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let actual = decisions
            .first()
            .expect("replayed trial must close with a decision");
        assert_failed_prefix_score(actual, &stage, rows, 60);
    }
}

#[test]
fn firmware_80c_candidates_and_prefix_scores_preserve_historical_evidence() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-80c-observed-time-replays.json"
    ))
    .unwrap();
    assert_eq!(
        fixtures.len(),
        10,
        "retain every closed trial from the completed run"
    );
    for fixture in fixtures {
        let profile = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
        let rows = fixture["samples"].as_array().unwrap();
        if conditional_replay_continues(&profile, rows, ThermalTuningTrialPurpose::Scout) {
            continue;
        }
        let (mut expected, failure, _) =
            external_candidate_and_stage(&profile, rows, "full_speed_to_stable_timeout");
        if fixture["sourceTrialIndex"] == 0 {
            // Finite Hold supply with a real high-side excursion uses the
            // approved braking-only correction; it cannot alter Hold entry.
            expected = thermal_candidate_profile_from_value(profile.clone()).points[0];
            expected.brake_distance_centi_c += 1_000;
        }
        assert_eq!(
            firmware_candidate(profile.clone(), rows),
            expected,
            "80C trial {} ({failure}) candidate",
            fixture["sourceTrialIndex"]
        );
        let (_, _, decisions) = replay_firmware_trials_and_capture_decisions(
            profile.clone(),
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let (_, count) = replay_firmware_trials_and_capture(
            profile.clone(),
            rows,
            ThermalTuningTrialPurpose::Scout,
            1,
        );
        let (_, _, prefix_stage) =
            external_candidate_and_stage(&profile, &rows[..count], "full_speed_to_stable_timeout");
        // Historical firmware used physical PWM for output statistics. Retain
        // that original score as evidence, independently of the corrected
        // logical-request comparison below; do not rewrite captured fixtures.
        let mut historical_rows = rows.clone();
        for row in &mut historical_rows {
            row[2] = row[3].clone();
        }
        let (_, _, historical_stage) = external_candidate_and_stage(
            &profile,
            &historical_rows,
            "full_speed_to_stable_timeout",
        );
        let historical_score = external_score_for_stage(&historical_stage, &historical_rows, 80);
        let live_score: [f64; 11] =
            serde_json::from_value(fixture["sourceDecisionScore"].clone()).unwrap();
        for (index, (actual, expected)) in live_score.iter().zip(historical_score).enumerate() {
            assert!(
                (actual - expected).abs() < 0.000_01,
                "historical 80C trial {} score {index}: device {actual}; MOS projection {expected}",
                fixture["sourceTrialIndex"]
            );
        }
        let actual = decisions
            .first()
            .expect("trial must close at its real hard boundary");
        assert_failed_prefix_score(actual, &prefix_stage, &rows[..count], 80);
    }
}

fn external_score_for_stage(stage: &ThermalStageResult, rows: &[Value], target: i16) -> [f64; 11] {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("samples.ndjson");
    let origin = rows[0][0].as_u64().unwrap();
    let samples = rows
        .iter()
        .map(|row| {
            json!({
                "targetTempC": target, "elapsedMs": row[0].as_u64().unwrap() - origin,
                "phase": row[4], "status": {
                    "currentTempC": row[1].as_f64().unwrap() / 100.0,
                    "heaterOutputPercent": row[2].as_f64().unwrap() / 10.0,
                    "heaterPhysicalOutputPercent": row[3].as_f64().unwrap() / 10.0,
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, samples).unwrap();
    candidate_score(
        &json!({"applied": [stage.to_value()], "files": {"samplesPath": path}}),
        target,
    )
    .0
}

// The external runner is frozen. Current failed-prefix ranking intentionally
// differs in dimensions 4..8; retain shared physical metrics and independently
// check the approved excursion/continuous-span ordering on the retained rows.
fn assert_failed_prefix_score(
    decision: &ThermalTuningDecisionRecord,
    stage: &ThermalStageResult,
    rows: &[Value],
    target: i16,
) {
    let legacy = external_score_for_stage(stage, rows, target);
    let score = decision.score;
    assert_eq!(score[0], legacy[0]);
    assert_eq!(score[1], 1.0, "historical failure cannot become completed");
    assert!(score[2] >= 1.0, "failed prefix retains a gate penalty");
    let limit = if target > 150 { 5_000 } else { 10_000 };
    if let Some((excursion, span_ms)) = failed_prefix_band_evidence(rows, target, limit) {
        assert!((score[4] - excursion).abs() < 0.000_01);
        let ranked_span = span_ms / 200 * 200;
        let expected_span = 1.0 - (ranked_span as f64 / 10_000.0).min(1.0);
        assert!((score[5] - expected_span).abs() < 0.000_01);
    } else {
        assert_eq!(score[5], 1.0);
    }
    for (actual_index, legacy_index) in [(6, 7), (7, 8), (8, 6), (9, 9), (10, 10)] {
        assert!(
            (score[actual_index] - legacy[legacy_index]).abs() < 0.000_01,
            "score[{actual_index}] = {}; physical metric = {}",
            score[actual_index],
            legacy[legacy_index],
        );
    }
}

fn failed_prefix_band_evidence(rows: &[Value], target: i16, limit_ms: u64) -> Option<(f64, u64)> {
    let warmup_exit = rows.iter().find(|row| row[4] != "warmup")?[0].as_u64()?;
    let first_band = rows.iter().position(|row| {
        row[0].as_u64().unwrap() >= warmup_exit
            && row[0].as_u64().unwrap() <= warmup_exit + limit_ms
            && (row[1].as_f64().unwrap() - f64::from(target) * 100.0).abs() <= 150.0
    })?;
    let mut excursion: f64 = 0.0;
    let mut longest = 0;
    let mut start = None;
    let mut previous = 0;
    let mut count = 0;
    for row in &rows[first_band..] {
        let elapsed = row[0].as_u64().unwrap();
        let outside =
            ((row[1].as_f64().unwrap() - f64::from(target) * 100.0).abs() - 150.0).max(0.0);
        excursion = excursion.max(outside / 100.0);
        if outside > 0.0 || elapsed.saturating_sub(previous) > 1_000 {
            start = None;
            count = 0;
        }
        previous = elapsed;
        if outside > 0.0 {
            continue;
        }
        let duration = elapsed - *start.get_or_insert(elapsed);
        count += 1;
        if (count - 1) * 1_000 >= duration * 3 {
            longest = longest.max(duration);
        }
    }
    Some((excursion, longest))
}

#[test]
fn firmware_hold_confirm_reseed_matches_external_35c_hil_replay() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-hold-confirm-lower-band.json"
    ))
    .unwrap();
    let profile_value = json!({"settings": fixture["settings"], "points": [fixture["point"]]});
    let rows = fixture["samples"].as_array().unwrap();
    let (expected, result) =
        external_confirmation_reseed(&profile_value, rows, "full_speed_to_stable_timeout");
    assert_eq!(result.analysis.hold_sample_count, 0);
    assert_eq!(expected.brake_distance_centi_c, 1_280);
    assert_eq!(expected.approach_power_permille, 350);
    assert_eq!(expected.approach_floor_power_permille, 180);
    assert_eq!(expected.approach_damping_exponent_permille, 3_910);
    assert_eq!(expected.approach_lead_ticks, 3);
    if conditional_replay_continues(&profile_value, rows, ThermalTuningTrialPurpose::HoldConfirm) {
        return;
    }
    let runtime = replay_firmware_trials(
        profile_value,
        rows,
        ThermalTuningTrialPurpose::HoldConfirm,
        1,
    );
    assert_eq!(runtime.trial_purpose, ThermalTuningTrialPurpose::Scout);
    assert_eq!(runtime.trial_index, 1);
    let actual = candidate_point_from_runtime(&runtime);
    assert_eq!(
        actual, expected,
        "failed 60s confirmation must reseed with the external confirmation strategy, not the scout batch strategy"
    );
}

#[test]
fn confirmation_reseed_vectors_preserve_approved_high_and_low_side_branches() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-hold-confirm-lower-band.json"
    ))
    .unwrap();
    for (target, stop, rows) in [
        (
            60,
            "full_speed_to_stable_timeout",
            json!([
                [0, 4000, 1000, 1000, "warmup"],
                [1000, 5500, 150, 150, "approach"],
                [2000, 6000, 100, 100, "hold"],
                [8000, 6240, 0, 0, "hold"],
                [11000, 6240, 0, 0, "hold"]
            ]),
        ),
        (
            60,
            "completed",
            json!([
                [0, 4000, 1000, 1000, "warmup"],
                [1000, 5500, 150, 150, "approach"],
                [2000, 6000, 100, 100, "hold"],
                [12000, 6000, 100, 100, "hold"],
                [62000, 6380, 0, 0, "hold"]
            ]),
        ),
        (
            180,
            "full_speed_to_stable_timeout",
            json!([
                [0, 14000, 1000, 1000, "warmup"],
                [1000, 17500, 950, 950, "approach"],
                [2000, 18000, 1000, 1000, "hold"],
                [4000, 18240, 950, 950, "hold"],
                [6000, 18240, 950, 950, "hold"]
            ]),
        ),
        (
            180,
            "full_speed_to_stable_timeout",
            json!([
                [0, 14000, 1000, 1000, "warmup"],
                [1000, 17500, 500, 500, "approach"],
                [2000, 18000, 600, 600, "hold"],
                [4000, 17760, 500, 500, "hold"],
                [6000, 17760, 500, 500, "hold"]
            ]),
        ),
    ] {
        let mut point = fixture["point"].clone();
        point["targetTempC"] = json!(target);
        let profile = json!({"settings": fixture["settings"], "points": [point]});
        let mut dense = rows.as_array().unwrap().clone();
        if stop == "completed" {
            // This is a synthetic unit vector, not a captured HIL trace. Supply
            // the continuous evidence that its completed classification requires.
            dense = vec![dense[0].clone(), dense[1].clone()];
            dense.extend(
                (2_000..62_000)
                    .step_by(300)
                    .map(|elapsed| json!([elapsed, 6000, 100, 100, "hold"])),
            );
            dense.push(json!([62000, 6380, 0, 0, "hold"]));
        }
        let rows = dense.as_slice();
        let (mut expected, _) = external_confirmation_reseed(&profile, rows, stop);
        if target == 180 && rows[3][1].as_i64().unwrap() > i64::from(target) * 100 {
            // The high-side branch is shared by Scout and HoldConfirm; it
            // must brake earlier rather than raise the low Hold baseline.
            expected = thermal_candidate_profile_from_value(profile.clone()).points[0];
            expected.brake_distance_centi_c = 1_700;
            expected.approach_power_permille = 250;
            expected.hold_reheat_power_permille = 100;
            expected.hold_entry_centi_c = 420;
            expected.hold_exit_centi_c = 380;
        }
        let runtime =
            replay_firmware_trials(profile, rows, ThermalTuningTrialPurpose::HoldConfirm, 1);
        assert_eq!(
            runtime.trial_index, 1,
            "{target}C {stop} must finish the failed confirm"
        );
        assert_eq!(
            candidate_point_from_runtime(&runtime),
            expected,
            "{target}C {stop}"
        );
    }
}

fn external_confirmation_reseed(
    profile_value: &Value,
    rows: &[Value],
    stop_reason: &'static str,
) -> (ThermalCandidatePoint, ThermalStageResult) {
    let samples = replay_samples(rows);
    let current = thermal_candidate_profile_from_value(profile_value.clone()).points[0];
    let target = current.target_temp_c;
    let mut stable = ThermalFullSpeedStableTracker::new(target);
    let mut guard = ThermalApproachGuardTracker::new(target, current.hold_entry_centi_c);
    for sample in &samples {
        stable.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
        guard.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
    }
    let hold_start = samples.iter().position(|sample| {
        sample.control_phase_in_hold
            && sample.current_temp_c >= f64::from(target) - 0.5
            && (sample.current_temp_c - f64::from(target)).abs() <= 3.0
    });
    let hold = hold_start.map(|start| &samples[start..]).unwrap_or(&[]);
    let max_temp = samples
        .iter()
        .map(|sample| sample.current_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let p2p = if hold.is_empty() {
        f64::INFINITY
    } else {
        hold.iter()
            .map(|sample| sample.current_temp_c)
            .fold(f64::NEG_INFINITY, f64::max)
            - hold
                .iter()
                .map(|sample| sample.current_temp_c)
                .fold(f64::INFINITY, f64::min)
    };
    let result = ThermalStageResult {
        target_temp_c: target,
        rise_time_ms: hold.first().map_or(0, |sample| sample.elapsed_ms),
        max_overshoot_c: (max_temp - f64::from(target)).max(0.0),
        hold_peak_to_peak_c: p2p,
        sample_count: samples.len(),
        stop_reason,
        terminal_runtime_drop_reason: None,
        analysis: thermal_replay_stage_analysis(&samples, target),
        guard: guard.finalize(),
        full_speed_to_stable: stable.finalize(),
    };
    let stage = result.to_value();
    let chart_samples = samples
        .iter()
        .map(|sample| {
            json!({
                "t": sample.elapsed_ms as f64 / 1000.0, "temp": sample.current_temp_c
            })
        })
        .collect::<Vec<_>>();
    let expected = apply_hold_confirm_reseed_nudge(
        tune_thermal_candidate_point(current, &thermal_stage_result_from_value(&stage).unwrap()),
        &stage,
        &chart_samples,
        target,
    );
    (expected, result)
}

fn replay_samples(rows: &[Value]) -> Vec<ThermalReplayStageSample> {
    let origin = rows[0][0].as_u64().unwrap();
    rows.iter()
        .map(|row| ThermalReplayStageSample {
            elapsed_ms: row[0].as_u64().unwrap() - origin,
            current_temp_c: row[1].as_f64().unwrap() / 100.0,
            heater_output_percent: (row[2].as_u64().unwrap() / 10) as u8,
            measurement_accepted: true,
            control_phase: Some(row[4].as_str().unwrap().to_owned()),
            control_phase_in_hold: row[4] == "hold",
            source_voltage_mv: None,
            source_current_ma: None,
            source_power_mw: None,
        })
        .collect()
}

// A truncated historical recording cannot manufacture the additional samples
// required by the approved exception. Check its policy/identity facts separately
// from original-deadline candidate/score comparisons. Fresh HIL and dense runtime
// regressions exercise the eventual outcome; this helper never synthesizes a tail.
fn conditional_replay_continues(
    profile: &Value,
    rows: &[Value],
    purpose: ThermalTuningTrialPurpose,
) -> bool {
    let (runtime, _, decisions) =
        replay_firmware_trials_and_capture_decisions(profile.clone(), rows, purpose, 1);
    let timing = runtime
        .current_timing_evidence()
        .or_else(|| decisions.iter().find_map(|decision| decision.timing));
    if timing.is_none_or(|timing| timing.extended_at_ms.is_none()) {
        return false;
    }
    let timing = timing.unwrap();
    assert!(
        timing.valid_extension(),
        "historical prefix must prove both exception prerequisites"
    );
    assert_eq!(
        timing.original_limit_ms,
        if profile["points"][0]["targetTempC"].as_i64().unwrap() > 150 {
            5_000
        } else {
            10_000
        }
    );
    assert!(!runtime.save_confirmed);
    assert_eq!(
        runtime.accepted_target_mask, 0,
        "a short historical prefix cannot qualify confirmation"
    );
    if purpose == ThermalTuningTrialPurpose::Scout {
        assert_eq!(
            candidate_point_from_runtime(&runtime),
            thermal_candidate_profile_from_value(profile.clone()).points[0],
            "extension must retain the same candidate"
        );
    }
    true
}

fn external_candidate(profile_value: &Value, rows: &[Value]) -> (ThermalCandidatePoint, String) {
    // Historical scouts captured beyond today's hard deadline. Compare both
    // strategies with only the evidence the current runtime actually retained;
    // a later qualified Hold cannot supply ripple to an already-stopped trial.
    let (_, count) = replay_firmware_trials_and_capture(
        profile_value.clone(),
        rows,
        ThermalTuningTrialPurpose::Scout,
        1,
    );
    external_candidate_with_stop(
        profile_value,
        &rows[..count],
        "full_speed_to_stable_timeout",
    )
}

fn external_candidate_with_stop(
    profile_value: &Value,
    rows: &[Value],
    stop_reason: &'static str,
) -> (ThermalCandidatePoint, String) {
    let (point, failure, _) = external_candidate_and_stage(profile_value, rows, stop_reason);
    (point, failure)
}

fn external_candidate_and_stage(
    profile_value: &Value,
    rows: &[Value],
    stop_reason: &'static str,
) -> (ThermalCandidatePoint, String, ThermalStageResult) {
    let origin = rows[0][0].as_u64().unwrap();
    let samples: Vec<ThermalReplayStageSample> = rows
        .iter()
        .map(|row| ThermalReplayStageSample {
            elapsed_ms: row[0].as_u64().unwrap() - origin,
            current_temp_c: row[1].as_f64().unwrap() / 100.0,
            heater_output_percent: (row[2].as_u64().unwrap() / 10) as u8,
            measurement_accepted: true,
            control_phase: Some(row[4].as_str().unwrap().to_owned()),
            control_phase_in_hold: row[4] == "hold",
            source_voltage_mv: None,
            source_current_ma: None,
            source_power_mw: None,
        })
        .collect();
    let host_profile = thermal_candidate_profile_from_value(profile_value.clone());
    let current = host_profile.points[0];
    let target = current.target_temp_c;
    let mut stable = ThermalFullSpeedStableTracker::new(target);
    let mut guard = ThermalApproachGuardTracker::new(target, current.hold_entry_centi_c);
    for sample in &samples {
        stable.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
        guard.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
    }
    let valid_hold_start = samples.iter().position(|sample| {
        sample.control_phase_in_hold
            && sample.current_temp_c >= f64::from(target) - 0.5
            && (sample.current_temp_c - f64::from(target)).abs() <= 3.0
    });
    let valid_hold = valid_hold_start
        .map(|start| &samples[start..])
        .unwrap_or(&[]);
    let max_temp = samples
        .iter()
        .map(|s| s.current_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let result = ThermalStageResult {
        target_temp_c: target,
        rise_time_ms: valid_hold.first().map_or(0, |sample| sample.elapsed_ms),
        max_overshoot_c: (max_temp - f64::from(target)).max(0.0),
        hold_peak_to_peak_c: valid_hold
            .iter()
            .map(|s| s.current_temp_c)
            .fold(f64::NEG_INFINITY, f64::max)
            - valid_hold
                .iter()
                .map(|s| s.current_temp_c)
                .fold(f64::INFINITY, f64::min),
        sample_count: samples.len(),
        stop_reason,
        terminal_runtime_drop_reason: None,
        analysis: thermal_replay_stage_analysis(&samples, target),
        guard: guard.finalize(),
        full_speed_to_stable: stable.finalize(),
    };
    let chart_samples: Vec<Value> = samples
        .iter()
        .map(|sample| {
            json!({
                "t": sample.elapsed_ms as f64 / 1000.0, "temp": sample.current_temp_c
            })
        })
        .collect();
    let evidence = stability_evidence_for_stage(&result.to_value(), &chart_samples, target);
    let failure_class = evidence["failureClass"].as_str().unwrap().to_owned();
    let predicted = apply_flagship_gate_nudge(
        &current,
        tune_thermal_candidate_point(current, &result),
        &evidence,
        target,
    );
    let predicted = bound_low_temperature_candidate_step(&current, predicted, &evidence, target);
    let selected = select_evidence_specific_candidate(
        current,
        predicted,
        hold_ripple_trim_candidate(current, &result, &evidence, target),
        conservative_high_side_candidate(current, &result, &evidence, target),
        failure_class.clone(),
    );
    (
        selected.map_or(current, |(_, point)| point),
        failure_class,
        result,
    )
}

fn firmware_candidate(profile_value: Value, rows: &[Value]) -> ThermalCandidatePoint {
    // The completed scout is now reused directly: its boundary already
    // exposes BatchPredicted, without replaying a redundant BatchCurrent.
    let runtime = replay_firmware_trials(profile_value, rows, ThermalTuningTrialPurpose::Scout, 1);
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::BatchPredicted
    );
    candidate_point_from_runtime(&runtime)
}

fn candidate_point_from_runtime(runtime: &ThermalTuningRuntime) -> ThermalCandidatePoint {
    let actual_wire: ThermalControlProfileWire = runtime.candidate_profile.unwrap().into();
    let actual_profile =
        thermal_candidate_profile_from_value(serde_json::to_value(actual_wire).unwrap());
    actual_profile.points[0]
}

fn replay_firmware_trials(
    profile_value: Value,
    rows: &[Value],
    purpose: ThermalTuningTrialPurpose,
    trial_count: u8,
) -> Box<ThermalTuningRuntime> {
    replay_firmware_trials_and_capture(profile_value, rows, purpose, trial_count).0
}

fn replay_firmware_trials_and_capture(
    profile_value: Value,
    rows: &[Value],
    purpose: ThermalTuningTrialPurpose,
    trial_count: u8,
) -> (Box<ThermalTuningRuntime>, usize) {
    let (runtime, count, _) =
        replay_firmware_trials_and_capture_decisions(profile_value, rows, purpose, trial_count);
    (runtime, count)
}

fn replay_firmware_trials_and_capture_decisions(
    profile_value: Value,
    rows: &[Value],
    purpose: ThermalTuningTrialPurpose,
    trial_count: u8,
) -> (
    Box<ThermalTuningRuntime>,
    usize,
    Vec<ThermalTuningDecisionRecord>,
) {
    let origin = rows[0][0].as_u64().unwrap();
    let mut wire_value = profile_value;
    wire_value["points"]
        .as_array_mut()
        .unwrap()
        .resize(10, Value::Null);
    let wire: ThermalControlProfileWire = serde_json::from_value(wire_value).unwrap();
    let target_c = wire.points[0].unwrap().target_temp_c;
    let profile = wire.into();
    // The bounded device event buffer is large. Keep it on the host heap
    // instead of copying it through every replay helper's test-thread stack.
    let mut runtime = Box::new(ThermalTuningRuntime::new());
    let candidate_id: TuningId = "candidate-replay".try_into().unwrap();
    runtime
        .start(ThermalTuningStart {
            run_id: "run-replay".try_into().unwrap(),
            candidate_id: candidate_id.clone(),
            candidate_hash: hash_profile(&profile).unwrap(),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[target_c],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    runtime.trial_purpose = purpose;
    let mut cursor = None;
    let mut captured_rows = 0;
    let mut decisions = Vec::new();
    // Stop at each real trial boundary; never supply a historical tail to a
    // candidate that has already been rejected by the current timing rules.
    for trial_index in 0..trial_count {
        let offset = trial_index as u64 * 100_000;
        let hash = runtime.candidate_hash.clone().unwrap();
        let mut cooling = ThermalTuningSample {
            sequence: 0,
            elapsed_ms: offset,
            target_c,
            trial_index,
            candidate_id: candidate_id.clone(),
            candidate_hash: hash,
            // Cross the active cooldown gate before replaying unmodified historical samples.
            temperature_centi_c: 3_400,
            raw_temperature_centi_c: None,
            vin_mv: 21_000,
            pps_contract: ThermalTuningPpsContract {
                voltage_mv: 21_000,
                current_ma: 5_000,
                kind: ThermalTuningPowerClass::Pps5a,
                confirmed: true,
            },
            requested_mv: 21_000,
            pps_power_ceiling_mv: 21_000,
            heater_requested_output_permille: 0,
            heater_output_permille: 0,
            measurement_valid: true,
            measurement_accepted: None,
            phase: ThermalTuningPhase::Cooldown,
            fan: None,
        };
        runtime.record_sample(cooling.clone()).unwrap();
        for (row_index, row) in rows.iter().enumerate() {
            cooling.elapsed_ms = offset + row[0].as_u64().unwrap() - origin;
            cooling.temperature_centi_c = row[1].as_i64().unwrap() as i16;
            cooling.heater_requested_output_permille = row[2].as_u64().unwrap() as u16;
            cooling.heater_output_permille = row[3].as_u64().unwrap() as u16;
            cooling.phase = serde_json::from_value(row[4].clone()).unwrap();
            runtime.record_sample(cooling.clone()).unwrap();
            captured_rows = row_index + 1;
            cursor = drain_replay_decisions(&mut runtime, cursor, &mut decisions);
            if runtime.trial_index != trial_index {
                break;
            }
        }
        if runtime.phase != ThermalTuningPhase::Cooldown {
            break;
        }
    }
    (runtime, captured_rows, decisions)
}

fn drain_replay_decisions(
    runtime: &mut ThermalTuningRuntime,
    mut cursor: Option<u32>,
    decisions: &mut Vec<ThermalTuningDecisionRecord>,
) -> Option<u32> {
    loop {
        let page = runtime.events_after(cursor);
        if page.events.is_empty() {
            return cursor;
        }
        decisions.extend(
            page.events
                .iter()
                .filter_map(|event| event.decision.clone()),
        );
        cursor = page.next_sequence;
    }
}
