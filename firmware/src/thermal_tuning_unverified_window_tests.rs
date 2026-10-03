//! Replays from the current branch's authorized 80C HIL; no synthetic tail.
use super::*;
use crate::control_plane::ThermalControlProfileWire;

fn captured_runtime(fixture: &serde_json::Value) -> ThermalTuningRuntime {
    let mut points = serde_json::json!([fixture["point"]]);
    while points.as_array().unwrap().len() < 10 {
        points.as_array_mut().unwrap().push(serde_json::Value::Null);
    }
    let wire: ThermalControlProfileWire = serde_json::from_value(serde_json::json!({
        "settings": fixture["settings"], "points": points
    }))
    .unwrap();
    let profile: ThermalControlProfileConfig = wire.into();
    let mut runtime = ThermalTuningRuntime::new();
    runtime.candidate_profile = Some(profile);
    runtime.reset_trial_metrics();
    runtime.trial_timing = Some(ThermalTuningTimingEvidence::for_point(
        profile.points[0].unwrap(),
    ));
    for row in fixture["samples"].as_array().unwrap() {
        let mut sample = super::tests::sample(
            row[0].as_u64().unwrap(),
            serde_json::from_value(row[4].clone()).unwrap(),
            row[1].as_i64().unwrap() as i16,
        );
        sample.target_c = 80;
        sample.heater_requested_output_permille = row[2].as_u64().unwrap() as u16;
        sample.heater_output_permille = row[3].as_u64().unwrap() as u16;
        runtime.update_trial_metrics(&sample, 80);
        runtime.maybe_extend_trial(sample.elapsed_ms, 80);
    }
    runtime
}

#[test]
fn captured_unverified_windows_remain_failed_for_candidate_search() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_unverified_window.fixture.json"
    ))
    .unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let runtime = captured_runtime(fixture);
        assert!(runtime.trial_timing.unwrap().valid_extension());
        assert_eq!(runtime.stable_window_verified_at_ms, None);
        let started =
            runtime.stable_window_started_at_ms.unwrap() - runtime.warmup_exited_at_ms.unwrap();
        assert_eq!(
            started,
            fixture["decision"]["settleTimeMs"].as_u64().unwrap()
        );
        assert!(started < 30_000);
        let evidence = runtime.candidate_failure_evidence(
            80,
            Some(fixture["decision"]["maxOvershootCentiC"].as_u64().unwrap() as u16),
            Some(
                fixture["decision"]["holdPeakToPeakCentiC"]
                    .as_u64()
                    .unwrap() as u16,
            ),
            Some(started),
            runtime.active_settle_limit_ms(80),
        );
        assert!(
            evidence.full_speed_failed,
            "trial {}",
            fixture["sourceTrialIndex"]
        );
    }
}

#[test]
fn captured_extended_window_break_is_classified_from_the_actual_high_side() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_unverified_window.fixture.json"
    ))
    .unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let runtime = captured_runtime(fixture);
        assert!(runtime.first_band_elapsed_ms.is_some());
        assert!(
            runtime
                .first_band_exit_centi_c
                .is_some_and(|value| value > 8_150)
        );
    }
}

#[test]
fn extension_replaces_only_the_deadline_temperature_cache() {
    let mut runtime = super::tests::extended_floor_runtime();
    let exit = runtime.warmup_exited_at_ms.unwrap();
    assert_eq!(runtime.active_settle_limit_ms(60), 30_000);
    let at_cap = super::tests::sample(exit + 30_000, ThermalTuningPhase::Approach, 5_800);
    runtime.update_trial_metrics(&at_cap, 60);
    assert_eq!(runtime.deadline_nearest_temperature_centi_c, Some(5_800));
    assert_eq!(runtime.deadline_nearest_distance_ms, 0);
    // Later polls must not clear the nearest evidence or the original clock.
    runtime.maybe_extend_trial(exit + 30_100, 60);
    assert_eq!(runtime.deadline_nearest_temperature_centi_c, Some(5_800));
    assert_eq!(runtime.warmup_exited_at_ms, Some(exit));
}

#[test]
fn captured_unverified_windows_cannot_receive_passing_metric_scores() {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
        "thermal_tuning_unverified_window.fixture.json"
    ))
    .unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let runtime = captured_runtime(fixture);
        let score = runtime.trial_score(
            80,
            true,
            Some(fixture["decision"]["maxOvershootCentiC"].as_u64().unwrap() as u16),
            Some(
                fixture["decision"]["holdPeakToPeakCentiC"]
                    .as_u64()
                    .unwrap() as u16,
            ),
            fixture["decision"]["settleTimeMs"].as_u64(),
        );
        assert_eq!(score[1], 1.0, "unverified window is an incomplete stage");
        assert!(score[2] >= 1.0, "unverified window cannot pass metric gate");
        assert!(
            score[4] >= 1.0,
            "broken window must retain progress penalty"
        );
    }
}
