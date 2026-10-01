use super::super::{
    ThermalApproachGuardTracker, ThermalReplayStageSample, thermal_replay_stage_analysis,
};
use super::*;
use flux_purr_firmware::{
    control_plane::ThermalControlProfileWire,
    thermal_tuning::{
        ThermalTuningPhase, ThermalTuningPowerClass, ThermalTuningPpsContract,
        ThermalTuningRuntime, ThermalTuningSample, ThermalTuningStart, ThermalTuningTrialPurpose,
        TuningId, hash_profile,
    },
};

// Captured after the physical fan repair. No external current/power measurements
// are synthesized; this comparison uses temperature, control phase and MOS PWM.
#[test]
fn firmware_candidate_matches_external_lower_band_hil_replay() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/thermal-60c-scout-lower-band.json"
    ))
    .unwrap();
    let profile_value = json!({
        "settings": fixture["settings"], "points": [fixture["point"]]
    });
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

fn external_candidate(profile_value: &Value, rows: &[Value]) -> (ThermalCandidatePoint, String) {
    let origin = rows[0][0].as_u64().unwrap();
    let samples: Vec<ThermalReplayStageSample> = rows
        .iter()
        .map(|row| ThermalReplayStageSample {
            elapsed_ms: row[0].as_u64().unwrap() - origin,
            current_temp_c: row[1].as_f64().unwrap() / 100.0,
            heater_output_percent: (row[3].as_u64().unwrap() / 10) as u8,
            control_phase: Some(row[4].as_str().unwrap().to_owned()),
            control_phase_in_hold: row[4] == "hold",
            source_voltage_mv: None,
            source_current_ma: None,
            source_power_mw: None,
        })
        .collect();
    let host_profile = thermal_candidate_profile_from_value(profile_value.clone());
    let current = host_profile.points[0];
    let mut stable = ThermalFullSpeedStableTracker::new(60);
    let mut guard = ThermalApproachGuardTracker::new(60, current.hold_entry_centi_c);
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
    let valid_hold_start = samples
        .iter()
        .position(|sample| {
            sample.control_phase_in_hold
                && sample.current_temp_c >= 59.5
                && (sample.current_temp_c - 60.0).abs() <= 3.0
        })
        .unwrap();
    let valid_hold = &samples[valid_hold_start..];
    let max_temp = samples
        .iter()
        .map(|s| s.current_temp_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let result = ThermalStageResult {
        target_temp_c: 60,
        rise_time_ms: valid_hold[0].elapsed_ms,
        max_overshoot_c: (max_temp - 60.0).max(0.0),
        hold_peak_to_peak_c: valid_hold
            .iter()
            .map(|s| s.current_temp_c)
            .fold(f64::NEG_INFINITY, f64::max)
            - valid_hold
                .iter()
                .map(|s| s.current_temp_c)
                .fold(f64::INFINITY, f64::min),
        sample_count: samples.len(),
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: thermal_replay_stage_analysis(&samples, 60),
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
    let evidence = stability_evidence_for_stage(&result.to_value(), &chart_samples, 60);
    let failure_class = evidence["failureClass"].as_str().unwrap().to_owned();
    let predicted = apply_flagship_gate_nudge(
        &current,
        tune_thermal_candidate_point(current, &result),
        &evidence,
        60,
    );
    let predicted = bound_low_temperature_candidate_step(&current, predicted, &evidence, 60);
    let selected = select_evidence_specific_candidate(
        current,
        predicted,
        hold_ripple_trim_candidate(current, &result, &evidence, 60),
        conservative_high_side_candidate(current, &result, &evidence, 60),
        failure_class.clone(),
    );
    (selected.map_or(current, |(_, point)| point), failure_class)
}

fn firmware_candidate(profile_value: Value, rows: &[Value]) -> ThermalCandidatePoint {
    let origin = rows[0][0].as_u64().unwrap();
    let mut wire_value = profile_value;
    wire_value["points"]
        .as_array_mut()
        .unwrap()
        .resize(10, Value::Null);
    let wire: ThermalControlProfileWire = serde_json::from_value(wire_value).unwrap();
    let profile = wire.into();
    let mut runtime = ThermalTuningRuntime::new();
    let candidate_id: TuningId = "candidate-replay".try_into().unwrap();
    runtime
        .start(ThermalTuningStart {
            run_id: "run-replay".try_into().unwrap(),
            candidate_id: candidate_id.clone(),
            candidate_hash: hash_profile(&profile).unwrap(),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        })
        .unwrap();
    let mut cursor = None;
    // Drive both the scout and current batch through the real runtime so the
    // public profile contains the predicted candidate at its trial boundary.
    for trial_index in 0..2 {
        let offset = trial_index as u64 * 100_000;
        let hash = runtime.candidate_hash.clone().unwrap();
        let mut cooling = ThermalTuningSample {
            sequence: 0,
            elapsed_ms: offset,
            target_c: 60,
            trial_index,
            candidate_id: candidate_id.clone(),
            candidate_hash: hash,
            temperature_centi_c: 3_900,
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
            phase: ThermalTuningPhase::Cooldown,
        };
        runtime.record_sample(cooling.clone()).unwrap();
        for row in rows {
            cooling.elapsed_ms = offset + row[0].as_u64().unwrap() - origin;
            cooling.temperature_centi_c = row[1].as_i64().unwrap() as i16;
            cooling.heater_requested_output_permille = row[2].as_u64().unwrap() as u16;
            cooling.heater_output_permille = row[3].as_u64().unwrap() as u16;
            cooling.phase = serde_json::from_value(row[4].clone()).unwrap();
            runtime.record_sample(cooling.clone()).unwrap();
            loop {
                let page = runtime.events_after(cursor);
                if page.events.is_empty() {
                    break;
                }
                cursor = page.next_sequence;
            }
        }
    }
    assert_eq!(
        runtime.trial_purpose,
        ThermalTuningTrialPurpose::BatchPredicted
    );
    let actual_wire: ThermalControlProfileWire = runtime.candidate_profile.unwrap().into();
    let actual_profile =
        thermal_candidate_profile_from_value(serde_json::to_value(actual_wire).unwrap());
    actual_profile.points[0]
}
