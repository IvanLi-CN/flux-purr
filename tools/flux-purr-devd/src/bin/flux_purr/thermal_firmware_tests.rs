use super::super::{Cli, Command, ThermalCommand};
use super::{
    InterruptedRunInput, StatusJournal, V2Input, build_report_runs, build_report_trial,
    event_stream_caught_up, firmware_candidate_profile_hash, firmware_candidate_profile_value,
    firmware_host_watchdog, firmware_profile_mode_ready, firmware_read_command,
    firmware_start_command, is_confirmed_rejected_start, next_event_sequence,
    record_device_identity, render_host_failure, render_interrupted_run, render_unsupported_pps3a,
    report_device_identity, selected_execution_order, trial_events_for_target,
    validate_firmware_args, validate_save_input, verified_saved_receipt, verify_device_trace,
    write_v2_input,
};
use clap::Parser;
use flux_purr_firmware::{
    control_plane::ThermalControlProfileWire,
    memory::ThermalControlProfileConfig,
    thermal_tuning::{ThermalTuningEvent, digest_event_chain, hash_profile},
};
use serde_json::{Value, json};

#[test]
fn firmware_tuning_requires_confirmed_automatic_power_before_mode_changes() {
    for status in [
        json!({"manualPpsEnabled": true, "manualPpsMv": 21000}),
        json!({}),
        json!({"manualPpsEnabled": null}),
    ] {
        let error = super::require_firmware_automatic_power(&status).unwrap_err();
        assert!(error.to_string().contains("manualPpsEnabled=false"));
    }
    assert!(super::require_firmware_automatic_power(&json!({"manualPpsEnabled": false})).is_ok());
}

#[test]
fn firmware_seed_export_keeps_parameters_and_hash() {
    let (seed, _) = super::super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let value = super::materialize_firmware_seed_profile(seed).unwrap();
    let exported = json!({
        "schema": "thermal-tuning-v2", "powerClass": "pps5a",
        "profile": {"bank": "pps5a", "source": "candidate", "value": value}
    });
    let materialized = super::parse_firmware_seed_profile(exported, "pps5a").unwrap();
    assert_eq!(materialized, value);
    assert_eq!(
        firmware_candidate_profile_hash(&materialized).unwrap(),
        firmware_candidate_profile_hash(&value).unwrap()
    );
}

fn cold_start_calibration(
    delay_ms: u32,
    capacity: f32,
    convection: f32,
    radiation: f32,
) -> serde_json::Value {
    json!({
        "tracePage": {"points": [{"sampleIndex": 0, "phase": "ambient", "temperatureCentiC": 3000}]},
        "activeResult": {
            "transactionId": 7,
            "convectionMwPerC": convection,
            "radiationMwPerK4": radiation,
            "thermalCapacityMjPerC": capacity,
            "transportDelayMs": delay_ms,
            "heatResponse": {"delayMs": delay_ms, "decayMs": 1000,
                "rmseC": 0.4, "maximumErrorC": 0.8},
            "curve": {"points": [
                {"tempCentiC": 2000, "resistanceMilliohms": 7000},
                {"tempCentiC": 22000, "resistanceMilliohms": 7000}
            ]}
        }
    })
}

#[test]
fn firmware_cold_seed_uses_each_devices_calibration_instead_of_the_static_table() {
    let runtime = json!({
        "ppsCapabilityMaxMv": 21000, "ppsCapabilityMaxMa": 5000,
        "thermalPlantModel": {"activeTransactionId": 7, "projectionValid": true}
    });
    let calibration_a = cold_start_calibration(2100, 25692.367, 179.62439, 3.0247352e-7);
    let calibration_b = cold_start_calibration(600, 31705.057, 18.24584, 9.931854e-7);
    let a = super::calibrated_firmware_seed_profile(&runtime, &calibration_a).unwrap();
    let b = super::calibrated_firmware_seed_profile(&runtime, &calibration_b).unwrap();
    assert_ne!(
        a["points"][0]["brakeDistanceCentiC"],
        b["points"][0]["brakeDistanceCentiC"]
    );
    assert_ne!(
        a["points"][0]["holdPowerPermille"],
        b["points"][0]["holdPowerPermille"]
    );
    for profile in [&a, &b] {
        assert_eq!(profile["points"][0]["approachLeadTicks"], 0);
        assert_eq!(profile["points"][0]["approachFloorPowerPermille"], 50);
        assert_eq!(profile["points"][0]["warmupPowerPermille"], 1000);
        assert_eq!(profile["points"][0]["holdEntryCentiC"], 50);
    }
    let mut with_history = runtime.clone();
    with_history["thermalControl"] =
        json!({"brakeDistanceCentiC": 5000, "holdPowerPermille": 1000});
    with_history["thermalControlProfile"] =
        json!({"points": [{"targetTempC": 60, "brakeDistanceCentiC": 5000}]});
    assert_eq!(
        super::calibrated_firmware_seed_profile(&with_history, &calibration_a).unwrap(),
        a
    );
    let a_high = &a["points"][7];
    assert!(
        a_high["holdPowerPermille"].as_u64().unwrap()
            > a["points"][0]["holdPowerPermille"].as_u64().unwrap()
    );
}

#[test]
fn firmware_cold_seed_rejects_missing_mismatched_or_invalid_calibration() {
    let runtime = json!({
        "ppsCapabilityMaxMv": 21000, "ppsCapabilityMaxMa": 3000,
        "thermalPlantModel": {"activeTransactionId": 7, "projectionValid": true}
    });
    let calibration = cold_start_calibration(600, 31705.057, 18.24584, 9.931854e-7);
    for (pointer, value) in [
        ("/activeResult/transactionId", json!(8)),
        ("/activeResult/thermalCapacityMjPerC", json!(0)),
        ("/activeResult/transportDelayMs", json!(60001)),
        ("/activeResult/heatResponse", json!(null)),
        ("/activeResult/heatResponse/rmseC", json!(1.1)),
        ("/activeResult/curve/points", json!([])),
        ("/tracePage/points/0/phase", json!("heating")),
    ] {
        let mut invalid = calibration.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            super::calibrated_firmware_seed_profile(&runtime, &invalid).is_err(),
            "{pointer}"
        );
    }
    let mut unavailable = runtime.clone();
    unavailable["thermalPlantModel"]["projectionValid"] = json!(false);
    assert!(super::calibrated_firmware_seed_profile(&unavailable, &calibration).is_err());
    assert!(super::calibrated_firmware_seed_profile(&runtime, &json!({})).is_err());
}

#[test]
fn firmware_cold_seed_preserves_energy_units_source_limits_and_canonical_hash() {
    let runtime = json!({
        "ppsCapabilityMaxMv": 21000, "ppsCapabilityMaxMa": 3000,
        "thermalPlantModel": {"activeTransactionId": 7, "projectionValid": true}
    });
    let calibration = cold_start_calibration(1000, 20000.0, 1000.0, 0.0);
    let profile = super::calibrated_firmware_seed_profile(&runtime, &calibration).unwrap();
    // Pmax=63W, loss=30W, C=20J/C, d=tau=1s, ambient=30C.
    // The linear convection ODE gives about 2.31C before the 60C peak;
    // the fixed 0.5C Hold-entry tolerance is not a braking margin.
    let brake = profile["points"][0]["brakeDistanceCentiC"]
        .as_u64()
        .unwrap();
    assert!((225..=235).contains(&brake), "{brake}");
    assert_eq!(profile["points"][0]["holdPowerPermille"], 476);
    assert_eq!(profile["points"][0]["holdKpPermillePerC"], 40);
    let mut padded = profile.clone();
    padded["points"].as_array_mut().unwrap().resize(
        flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT,
        serde_json::Value::Null,
    );
    let wire: flux_purr_firmware::control_plane::ThermalControlProfileWire =
        serde_json::from_value(padded).unwrap();
    let mut config: flux_purr_firmware::memory::ThermalControlProfileConfig = wire.into();
    config.sanitize();
    assert_eq!(
        super::firmware_candidate_profile_hash(&profile).unwrap(),
        super::hash_profile(&config).unwrap().as_str()
    );
}

#[test]
fn firmware_seed_export_preserves_complete_wire_values() {
    let (seed, _) = super::super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let mut value = super::materialize_firmware_seed_profile(seed).unwrap();
    value["settings"]["approachLeadTicks"] = json!(9);
    value["points"][0]["approachLeadTicks"] = json!(0);
    value["points"][0]["approachPowerPermille"] = json!(282);
    value["points"][7]["approachPowerPermille"] = json!(899);
    value["points"][7]["approachDampingExponentPermille"] = json!(533);
    let hash = firmware_candidate_profile_hash(&value).unwrap();
    let exported = json!({
        "schema": "thermal-tuning-v2", "powerClass": "pps5a", "candidateHash": hash,
        "profile": {"bank": "pps5a", "source": "device_reported", "value": value}
    });
    let materialized = super::parse_firmware_seed_profile(exported, "pps5a").unwrap();
    assert_eq!(materialized, value);
    assert_eq!(
        firmware_candidate_profile_hash(&materialized).unwrap(),
        hash
    );
}

#[test]
fn firmware_seed_export_rejects_stale_hash_bank_and_incomplete_targets() {
    let (seed, _) = super::super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let value = super::materialize_firmware_seed_profile(seed).unwrap();
    let exported = json!({
        "schema": "thermal-tuning-v2", "powerClass": "pps5a",
        "candidateHash": firmware_candidate_profile_hash(&value).unwrap(),
        "profile": {"bank": "pps5a", "value": value}
    });
    let mut stale = exported.clone();
    stale["candidateHash"] = json!("sha256:stale");
    let mut wrong_bank = exported.clone();
    wrong_bank["profile"]["bank"] = json!("pps3a");
    let mut missing_target = exported.clone();
    missing_target["profile"]["value"]["points"][4] = serde_json::Value::Null;
    missing_target
        .as_object_mut()
        .unwrap()
        .remove("candidateHash");
    for invalid in [stale, wrong_bank, missing_target] {
        assert!(super::parse_firmware_seed_profile(invalid, "pps5a").is_err());
    }
}

#[test]
fn firmware_seed_rejects_missing_or_wrong_class_export_without_defaults() {
    for invalid in [
        json!({}),
        json!({"profile": {"bank": "pps5a", "value": {}}}),
        json!({"schema": "thermal-tuning-v2", "powerClass": "pps3a"}),
        json!({"schema": "thermal-tuning-v2", "powerClass": "pps5a"}),
    ] {
        assert!(super::parse_firmware_seed_profile(invalid, "pps5a").is_err());
    }
}

#[test]
fn interrupted_export_never_pairs_stale_profile_with_current_hash() {
    let first = device_journal_fixture();
    let mut profile = first["candidateProfile"].clone();
    profile["points"][0]["approachPowerPermille"] = json!(282);
    let hash = firmware_candidate_profile_hash(&profile).unwrap();
    assert_ne!(hash, first["candidateHash"]);
    for current_profile_received in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut current = first.clone();
        current["candidateHash"] = json!(hash);
        if current_profile_received {
            current["candidateProfile"] = profile.clone();
        } else {
            current.as_object_mut().unwrap().remove("candidateProfile");
        }
        write_v2_input(V2Input {
            input_dir: dir.path(),
            device_id: "bench",
            run_id: "journal-run",
            candidate_id: "journal-candidate",
            candidate_hash: &hash,
            candidate_profile: &first["candidateProfile"],
            statuses: &[first.clone(), current],
            host_interruption: Some("transport_or_runtime_failure"),
            selected_targets: &[60],
            execution_order: &[60],
        })
        .unwrap();
        let exported: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join("thermal-profile.candidate.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(exported["candidateHash"], hash);
        if current_profile_received {
            assert_eq!(exported["profile"]["value"], profile);
            assert_eq!(exported["profile"]["source"], "device_reported");
        } else {
            assert!(exported["profile"]["value"].is_null());
            assert_eq!(
                exported["profile"]["source"],
                "unavailable_matching_candidate_profile"
            );
        }
        let rendered =
            super::thermal_tuning_v2_report::render_v2_bundle(super::ThermalTuningV2ReportInput {
                input_dir: dir.path().to_path_buf(),
                output_dir: Some(dir.path().join("report")),
            })
            .unwrap();
        assert_eq!(rendered["ownerOutcome"], "failed");
    }
}

#[test]
fn web_digest_fixture_matches_shared_firmware_serializer() {
    let events: Vec<ThermalTuningEvent> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/src/features/control-plane-demo/thermal-tuning-digest.fixture.json"
    )))
    .unwrap();
    assert_eq!(
        digest_event_chain(&events[..1]).as_str(),
        "4ea3bc9c23cbf417bbcd6468998d28c493745c28305d92ead01274c3a2f4f9d3"
    );
    assert_eq!(
        digest_event_chain(&events).as_str(),
        "76ee2a15a2e7bef6433ff8deb2e401a6ea483fbcbf1a1b0f5ed54e4eac9ed82b"
    );
}

#[test]
fn fixed_floor_timing_digest_matches_web() {
    let mut values: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/src/features/control-plane-demo/thermal-tuning-digest.fixture.json"
    )))
    .unwrap();
    let timing = &mut values[1]["decision"]["timing"];
    timing["policyVersion"] = json!(2);
    timing["configuredApproachFloorPermille"] = json!(100);
    timing["originalDeadlineObservedAtMs"] = json!(11000);
    timing["originalDeadlinePhase"] = json!("approach");
    let events: Vec<ThermalTuningEvent> = serde_json::from_value(values).unwrap();
    assert_eq!(
        digest_event_chain(&events).as_str(),
        "ed0fa77d928f38f1b346a5f5d1c31737a590ba1ea53e06eacd5c468988ef27fb"
    );
}

#[test]
fn model_floor_timing_digest_matches_web() {
    let mut values: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/src/features/control-plane-demo/thermal-tuning-digest.fixture.json"
    )))
    .unwrap();
    let timing = &mut values[1]["decision"]["timing"];
    timing["policyVersion"] = json!(3);
    timing["configuredApproachFloorPermille"] = json!(420);
    timing["modelApproachFloorPermille"] = json!(100);
    timing["originalDeadlineObservedAtMs"] = json!(11000);
    timing["originalDeadlinePhase"] = json!("approach");
    let events: Vec<ThermalTuningEvent> = serde_json::from_value(values).unwrap();
    assert_eq!(
        digest_event_chain(&events).as_str(),
        "598383346ab2f5d316bd0e9358ff7a1bcce4503dddc317bfb5df6e36773aa01a"
    );
}

#[test]
fn conditional_timing_requires_bound_floor_trigger_and_full_stable_window() {
    let mut trial = json!({
        "targetC": 60,
        "canonicalCandidate": {"point": {"approachFloorPowerPermille": 100, "holdPowerPermille": 100}},
        "fullSpeedToStable": {"settleTimeMs": 12000, "limitMs": 30000},
        "timingEvidenceVerification": {"verified": true},
        "timing": {"policyVersion": 2, "originalLimitMs": 10000, "extensionLimitMs": 30000,
            "allowedApproachFloorPermille": 100, "effectiveApproachFloorPermille": 100,
            "configuredApproachFloorPermille": 100, "originalDeadlineObservedAtMs": 11000,
            "originalDeadlinePhase": "approach",
            "floorObservedAtMs": 2000, "extendedAtMs": 11000, "warmupExitedAtMs": 1000,
            "stableWindowStartedAtMs": 13000, "stableWindowVerifiedAtMs": 23000}
    });
    assert!(super::timing_gate_valid(&trial));
    let mut model_trial = trial.clone();
    model_trial["canonicalCandidate"]["point"]["approachFloorPowerPermille"] = json!(420);
    model_trial["canonicalCandidate"]["point"]["holdPowerPermille"] = json!(650);
    model_trial["timing"]["policyVersion"] = json!(3);
    model_trial["timing"]["configuredApproachFloorPermille"] = json!(420);
    model_trial["timing"]["modelApproachFloorPermille"] = json!(100);
    assert!(super::timing_gate_valid(&model_trial));
    let mut braking_trial = model_trial.clone();
    braking_trial["timing"]["policyVersion"] = json!(4);
    for key in [
        "modelApproachFloorPermille",
        "allowedApproachFloorPermille",
        "effectiveApproachFloorPermille",
    ] {
        braking_trial["timing"][key] = json!(50);
    }
    assert!(super::timing_gate_valid(&braking_trial));
    braking_trial["timing"]["policyVersion"] = json!(3);
    assert!(!super::timing_gate_valid(&braking_trial));
    model_trial["timing"]["modelApproachFloorPermille"] = serde_json::Value::Null;
    assert!(!super::timing_gate_valid(&model_trial));
    for (pointer, value) in [
        ("/timing/floorObservedAtMs", serde_json::Value::Null),
        ("/timing/extendedAtMs", json!(10000)),
        ("/timing/stableWindowVerifiedAtMs", json!(22999)),
        ("/timing/extensionLimitMs", json!(180000)),
        ("/timing/policyVersion", json!(1)),
        ("/timing/configuredApproachFloorPermille", json!(225)),
        ("/timing/originalDeadlinePhase", json!("hold")),
        ("/timing/originalDeadlineObservedAtMs", json!(12000)),
        ("/timingEvidenceVerification/verified", json!(false)),
        ("/canonicalCandidate/point/holdPowerPermille", json!(135)),
        ("/fullSpeedToStable/limitMs", json!(180000)),
    ] {
        let mut invalid = trial.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(!super::timing_gate_valid(&invalid), "{pointer}");
    }
    trial["timing"]["extendedAtMs"] = serde_json::Value::Null;
    trial["fullSpeedToStable"]["limitMs"] = json!(10000);
    assert!(super::timing_gate_valid(&trial)); // Other metric gates still reject 12s > 10s.
}

#[test]
fn pps_pause_qualification_matches_firmware_without_rewriting_historical_policy() {
    let timing: flux_purr_firmware::thermal_tuning_limits::ThermalTuningTimingEvidence =
        serde_json::from_value(json!({"policyVersion": 5, "originalLimitMs": 5000,
            "extensionLimitMs": 15000, "allowedApproachFloorPermille": 50,
            "effectiveApproachFloorPermille": 50, "modelApproachFloorPermille": 50,
            "configuredApproachFloorPermille": 130, "originalDeadlineObservedAtMs": 6000,
            "originalDeadlinePhase": "hold", "floorObservedAtMs": null,
            "extendedAtMs": null, "warmupExitedAtMs": 1000,
            "stableWindowStartedAtMs": 2000, "stableWindowVerifiedAtMs": 12000}))
        .unwrap();
    let mut samples: Vec<serde_json::Value> = (2000..=12000)
        .step_by(250)
        .map(|time| {
            json!({
                "elapsedMs": time, "phase": "hold", "temperatureCentiC": 22000,
                "rawTemperatureCentiC": 21997, "measurementValid": true,
                "measurementAccepted": ![6250,6500].contains(&time),
                "ppsRequestedMv": if time < 6000 {20500} else {21000}
            })
        })
        .collect();
    let check = |timing: &_, samples: &[serde_json::Value]| {
        super::verify_timing_samples(timing, 220, &samples.iter().collect::<Vec<_>>())["verified"]
            == true
    };
    assert!(check(&timing, &samples));
    let mut historical = timing;
    historical.policy_version = 4;
    assert!(!check(&historical, &samples));
    let mut applied_samples = samples.clone();
    for sample in &mut applied_samples {
        let time = sample["elapsedMs"].as_u64().unwrap();
        sample["measurementAccepted"] = json!(![6500, 6750].contains(&time));
        sample["ppsContractConfirmed"] = json!(true);
        sample["ppsContractMv"] = json!(if time < 6250 { 20500 } else { 21000 });
    }
    assert!(check(&timing, &applied_samples));
    assert!(!check(&historical, &applied_samples));
    for confirmed in [false, true] {
        let mut unknown_or_mismatched = applied_samples.clone();
        for sample in &mut unknown_or_mismatched {
            sample["ppsContractConfirmed"] = json!(confirmed);
            if sample["elapsedMs"].as_u64().unwrap() >= 6250 {
                sample["ppsContractMv"] = json!(21250);
            }
        }
        assert!(!check(&timing, &unknown_or_mismatched));
    }
    samples[17]["rawTemperatureCentiC"] = json!(24000);
    assert!(!check(&timing, &samples));
    samples[17]["rawTemperatureCentiC"] = json!(21997);
    for sample in &mut samples {
        if (6250..=7250).contains(&sample["elapsedMs"].as_u64().unwrap()) {
            sample["measurementAccepted"] = json!(false);
        }
    }
    assert!(!check(&timing, &samples));
}

#[test]
fn braking_floor_timing_digest_matches_web() {
    let mut values: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/src/features/control-plane-demo/thermal-tuning-digest.fixture.json"
    )))
    .unwrap();
    let timing = &mut values[1]["decision"]["timing"];
    timing["policyVersion"] = json!(4);
    timing["configuredApproachFloorPermille"] = json!(420);
    for key in [
        "modelApproachFloorPermille",
        "allowedApproachFloorPermille",
        "effectiveApproachFloorPermille",
    ] {
        timing[key] = json!(50);
    }
    timing["originalDeadlineObservedAtMs"] = json!(11000);
    timing["originalDeadlinePhase"] = json!("approach");
    let events: Vec<ThermalTuningEvent> = serde_json::from_value(values).unwrap();
    assert_eq!(
        digest_event_chain(&events).as_str(),
        "a6e67f9f78498c1589641dbfa3a7b4b49ea48bb475f7b2d20fe8e5986fa51e97"
    );
}

#[test]
fn braking_timing_verifies_real_five_percent_floor_and_rejects_raised_requests() {
    let timing: flux_purr_firmware::thermal_tuning_limits::ThermalTuningTimingEvidence =
        serde_json::from_value(json!({"policyVersion": 4, "originalLimitMs": 10000,
            "extensionLimitMs": 30000, "allowedApproachFloorPermille": 50,
            "effectiveApproachFloorPermille": 50, "modelApproachFloorPermille": 50,
            "configuredApproachFloorPermille": 420, "originalDeadlineObservedAtMs": 11000,
            "originalDeadlinePhase": "approach", "floorObservedAtMs": 2000,
            "extendedAtMs": 11000, "warmupExitedAtMs": 1000,
            "stableWindowStartedAtMs": 13000, "stableWindowVerifiedAtMs": 23000}))
        .unwrap();
    let mut samples = vec![
        json!({"elapsedMs": 1000, "phase": "approach", "heaterRequestedOutputPermille": 110, "temperatureCentiC": 5000, "measurementValid": true}),
        json!({"elapsedMs": 2000, "phase": "approach", "heaterRequestedOutputPermille": 50, "temperatureCentiC": 5300, "measurementValid": true}),
        json!({"elapsedMs": 11000, "phase": "approach", "heaterRequestedOutputPermille": 50, "temperatureCentiC": 5800, "measurementValid": true}),
    ];
    for time in (13000..=23000).step_by(250) {
        samples.push(
            json!({"elapsedMs": time, "phase": "hold", "heaterRequestedOutputPermille": 100,
            "temperatureCentiC": 6000, "measurementValid": true}),
        );
    }
    let verify = |samples: &[serde_json::Value]| {
        super::verify_timing_samples(&timing, 60, &samples.iter().collect::<Vec<_>>())
    };
    assert_eq!(verify(&samples)["verified"], true);
    samples[4]["measurementAccepted"] = json!(false);
    assert_eq!(verify(&samples)["verified"], false);
    samples[4]["measurementAccepted"] = json!(true);
    samples[1]["measurementAccepted"] = json!(false);
    assert_eq!(verify(&samples)["verified"], false);
    samples[1]["measurementAccepted"] = json!(true);
    samples[1]["heaterRequestedOutputPermille"] = json!(100);
    assert_eq!(verify(&samples)["verified"], false);
    samples[1]["heaterRequestedOutputPermille"] = json!(50);
    samples[2]["heaterRequestedOutputPermille"] = json!(100);
    assert_eq!(verify(&samples)["verified"], false);
}

#[test]
fn captured_identity_survives_standard_report_and_host_failure_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let input_root = dir.path().join("input");
    let input_dir = input_root.join("identity-run");
    std::fs::create_dir_all(&input_dir).unwrap();
    let identity = json!({
        "deviceId": "288485681c38", "firmwareVersion": "0.27.6-dev.identity",
        "gitSha": "0123456789abcdef0123456789abcdef01234567", "buildId": "build-identity",
        "board": "esp32s3-frontpanel", "capabilities": ["thermal-tuning-v2"]
    });
    record_device_identity(&input_dir, "identity-run", &identity).unwrap();
    let profile = json!({"points": []});
    write_v2_input(V2Input {
        input_dir: &input_dir,
        device_id: "serial-selector",
        run_id: "identity-run",
        candidate_id: "candidate-identity",
        candidate_hash: "sha256:identity",
        candidate_profile: &profile,
        statuses: &[json!({"runId": "identity-run", "candidateId": "candidate-identity", "powerClass": "pps5a", "targetsC": [60]})],
        host_interruption: None,
        selected_targets: &[60],
        execution_order: &[60],
    })
    .unwrap();
    for failure in [false, true] {
        let output_dir = dir
            .path()
            .join(if failure { "failed-report" } else { "report" });
        if failure {
            render_host_failure(super::HostFailureInput {
                device_id: "serial-selector",
                output_root: &input_root,
                bundle_dir: Some(output_dir.clone()),
                run_id: Some("identity-run"),
                power_class: "pps5a",
                reason: "start_not_confirmed",
                no_device_command: false,
                attempted_candidate: None,
                selected_targets: Some(&[60]),
            })
            .unwrap();
        } else {
            super::thermal_tuning_v2_report::render_v2_bundle(super::ThermalTuningV2ReportInput {
                input_dir: input_dir.clone(),
                output_dir: Some(output_dir.clone()),
            })
            .unwrap();
        }
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output_dir.join("run.bundle.json")).unwrap())
                .unwrap();
        assert_eq!(bundle["device"]["id"], "288485681c38");
        assert_eq!(bundle["device"]["selector"], "serial-selector");
        assert_eq!(bundle["device"]["identity"], identity);
        assert_eq!(bundle["firmware"]["version"], identity["firmwareVersion"]);
        assert_eq!(bundle["firmware"]["gitSha"], identity["gitSha"]);
        assert_eq!(bundle["firmware"]["buildId"], identity["buildId"]);
    }
}

#[test]
fn report_identity_is_unavailable_without_capture_and_cannot_cross_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (device, firmware) = report_device_identity(dir.path(), "run-1", "selector").unwrap();
    assert_eq!(device["identity"], serde_json::Value::Null);
    assert_eq!(firmware["version"], serde_json::Value::Null);
    assert_eq!(firmware["availability"], "unavailable_device_identity");
    record_device_identity(
        dir.path(),
        "run-1",
        &json!({
            "deviceId": "device-1", "firmwareVersion": "version-1",
            "gitSha": "git-1", "buildId": "build-1"
        }),
    )
    .unwrap();
    let error = report_device_identity(dir.path(), "run-2", "selector").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("another run"));
}

#[test]
fn firmware_tuning_requires_matching_mode_and_resolved_bank() {
    assert!(!firmware_profile_mode_ready(
        &json!({
            "thermalProfileMode": "65w",
            "thermalProfileResolvedBank": "pps3a"
        }),
        "pps5a"
    ));
    assert!(!firmware_profile_mode_ready(
        &json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps3a"
        }),
        "pps5a"
    ));
    assert!(firmware_profile_mode_ready(
        &json!({
            "thermalProfileMode": "100w",
            "thermalProfileResolvedBank": "pps5a"
        }),
        "pps5a"
    ));
    assert!(firmware_profile_mode_ready(
        &json!({
            "thermalProfileMode": "65w", "thermalProfileResolvedBank": "pps3a"
        }),
        "pps3a"
    ));
    assert!(!firmware_profile_mode_ready(
        &json!({
            "thermalProfileMode": "100w", "thermalProfileResolvedBank": "pps5a"
        }),
        "pps3a"
    ));
}

#[test]
fn three_amp_seed_export_cannot_cross_the_requested_class() {
    let (seed, _) = super::super::load_thermal_default_seed_candidate_profile("pps3a").unwrap();
    let profile = super::materialize_firmware_seed_profile(seed).unwrap();
    let export = json!({"schema": "thermal-tuning-v2", "powerClass": "pps3a",
        "profile": {"bank": "pps3a", "value": profile}});
    assert_eq!(
        super::parse_firmware_seed_profile(export.clone(), "pps3a").unwrap(),
        profile
    );
    assert!(super::parse_firmware_seed_profile(export, "pps5a").is_err());
}

#[test]
fn journal_persists_each_event_page_before_the_next_ack() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = StatusJournal::create(directory.path()).unwrap();
    let mut statuses = Vec::new();
    let first = json!({
        "candidateHash": "hash-1",
        "candidateProfile": {"points": [60]},
        "terminalDisposition": "running",
        "trace": {"lastSequence": 0, "complete": false, "gap": false, "digest": "digest-0"},
        "events": {"events": [{"sequence": 0}], "nextSequence": 0}
    });
    journal.record(&mut statuses, first.clone()).unwrap();
    let mut idle = first.clone();
    idle["events"] = json!({"events": [], "nextSequence": null});
    journal.record(&mut statuses, idle).unwrap();
    assert_eq!(statuses.len(), 1);

    let mut terminal = first;
    terminal["terminalDisposition"] = json!("candidate_ready");
    terminal["trace"] =
        json!({"lastSequence": 1, "complete": true, "gap": false, "digest": "digest-1"});
    terminal["events"] = json!({"events": [{"sequence": 1}], "nextSequence": 1});
    journal.record(&mut statuses, terminal.clone()).unwrap();
    assert!(statuses[1].get("candidateProfile").is_none());

    terminal["events"] = json!({"events": [], "nextSequence": null});
    journal.record(&mut statuses, terminal).unwrap();
    assert_eq!(statuses.len(), 3);
    let recorded =
        std::fs::read_to_string(directory.path().join("device-statuses.ndjson")).unwrap();
    let from_disk: Vec<serde_json::Value> = recorded
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(from_disk, statuses);

    drop(journal);
    let mut journal = StatusJournal::resume(directory.path(), &statuses).unwrap();
    let sealed = json!({
        "candidateHash": "hash-1",
        "candidateProfile": {"points": [60]},
        "sealed": true,
        "terminalDisposition": "candidate_ready",
        "trace": {"lastSequence": 2, "complete": true, "gap": false, "digest": "digest-2"},
        "events": {"events": [{"sequence": 2}], "nextSequence": 2}
    });
    journal.record(&mut statuses, sealed).unwrap();
    assert!(statuses.last().unwrap().get("candidateProfile").is_none());
    let recorded =
        std::fs::read_to_string(directory.path().join("device-statuses.ndjson")).unwrap();
    let from_disk: Vec<serde_json::Value> = recorded
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(from_disk, statuses);
}

#[test]
fn journal_keeps_profile_returned_after_event_pages() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = StatusJournal::create(directory.path()).unwrap();
    let mut statuses = Vec::new();
    journal
        .record(
            &mut statuses,
            json!({"candidateHash": "hash-2", "events": {"events": []}, "trace": {"lastSequence": 2}}),
        )
        .unwrap();
    journal
        .record(
            &mut statuses,
            json!({"candidateHash": "hash-2", "candidateProfile": {"points": [60]}, "events": {"events": []}, "trace": {"lastSequence": 2}}),
        )
        .unwrap();
    journal
        .record(
            &mut statuses,
            json!({"candidateHash": "hash-2", "candidateProfile": {"points": [60]}, "events": {"events": []}, "trace": {"lastSequence": 2}}),
        )
        .unwrap();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[1]["candidateProfile"]["points"], json!([60]));
}

#[test]
fn read_ack_is_bound_to_the_run_identity() {
    assert_eq!(
        firmware_read_command("run-1", Some(17)),
        json!({"op": "read", "runId": "run-1", "afterSequence": 17})
    );
}

#[test]
fn restarted_device_status_cannot_contaminate_the_active_run_journal() {
    let directory = tempfile::tempdir().unwrap();
    let mut journal = StatusJournal::create(directory.path()).unwrap();
    let mut statuses = Vec::new();
    let active = device_journal_fixture();
    journal.record(&mut statuses, active.clone()).unwrap();
    let reset = json!({
        "runId": null, "candidateId": null, "powerClass": null,
        "targetsC": [], "phase": "idle", "armed": false,
        "terminalDisposition": "running",
        "events": {"events": [], "nextSequence": null},
        "trace": {"lastSequence": null, "digest": "", "complete": true, "gap": false}
    });
    let error = journal.record(&mut statuses, reset).unwrap_err();
    assert!(error.to_string().contains("tuning_run_lost"));
    assert_eq!(statuses, [active]);
    let raw = std::fs::read_to_string(directory.path().join("device-statuses.ndjson")).unwrap();
    assert_eq!(raw.lines().count(), 1);
}

#[test]
fn saved_promotion_requires_matching_device_receipts_even_if_review_fails() {
    let preview = json!({
        "runId": "run-1", "candidateId": "candidate-1", "candidateHash": "hash-1",
        "powerClass": "pps5a", "ramAppliedHash": "hash-1"
    });
    let mut save = json!({
        "runId": "run-1", "candidateId": "candidate-1", "candidateHash": "hash-1",
        "powerClass": "pps5a", "bank": "pps5a", "bankRevision": 3,
        "bankHash": "hash-1", "atomic": true, "readbackVerified": true
    });
    assert!(verified_saved_receipt(
        "pps5a",
        "run-1",
        "candidate-1",
        "hash-1",
        Some(&preview),
        Some(&save)
    ));
    save["bankHash"] = json!("other");
    assert!(!verified_saved_receipt(
        "pps5a",
        "run-1",
        "candidate-1",
        "hash-1",
        Some(&preview),
        Some(&save)
    ));
}

#[test]
fn explicit_save_requires_verified_candidate_and_matching_preview_receipt() {
    let physical = [60, 80, 100, 120, 140, 160, 180, 220];
    let runs = physical
        .iter()
        .map(|target| json!({
            "targetC": target,
            "reviewOutcome": "passed",
            "reviewPassed": true,
            "adoptedTrialIndex": 1,
            "trials": [{
                "targetC": target,
                "trialIndex": 1,
                "trialPurpose": "hold_confirm",
                "disposition": "adopted",
                "warmupRequestedOutputPermille": 1000,
                "maxOvershootC": 1.2,
                "holdPeakToPeakC": 1.8,
                "canonicalCandidate": {"point": {"approachFloorPowerPermille": 100, "holdPowerPermille": 100}},
                "timingEvidenceVerification": {"verified": true},
                "holdEvidenceVerification": {"verified": true, "continuousHoldMs": 60000},
                "timing": {"policyVersion": 2, "originalLimitMs": super::original_settle_limit_ms(*target),
                    "extensionLimitMs": super::original_settle_limit_ms(*target) * 3,
                    "allowedApproachFloorPermille": 100, "effectiveApproachFloorPermille": 100,
                    "configuredApproachFloorPermille": 100,
                    "floorObservedAtMs": null, "extendedAtMs": null, "warmupExitedAtMs": 1000,
                    "stableWindowStartedAtMs": 4000, "stableWindowVerifiedAtMs": 14000},
                "fullSpeedToStable": {"settleTimeMs": 3000, "limitMs": super::original_settle_limit_ms(*target)},
                "metrics": {"holdConfirmSeconds": 60, "phasePath": "warmup -> approach -> hold"}
            }]
        }))
        .collect::<Vec<_>>();
    let bundle = json!({
        "schema": "thermal-tuning-v2",
        "runId": "run-1",
        "engine": "firmware",
        "powerClass": "pps5a",
        "targetSet": {
            "physicalC": physical,
            "selectedC": physical,
            "executionOrderC": [60, 220, 120, 80, 100, 160, 140, 180]
        },
        "qualificationScope": {"kind": "default-pps5a-full-batch", "diagnosticOnly": false},
        "reportRuns": runs,
        "terminalDisposition": "candidate_ready",
        "deviceTraceVerification": {"verified": true},
        "candidate": {"candidateId": "candidate-1", "candidateHash": "hash-1"}
    });
    let candidate = json!({
        "runId": "run-1",
        "candidateId": "candidate-1",
        "candidateHash": "hash-1",
        "powerClass": "pps5a",
        "profile": {"bank": "pps5a"},
        "promotion": {
            "previewReceipt": {
                "runId": "run-1", "candidateId": "candidate-1",
                "candidateHash": "hash-1", "ramAppliedHash": "hash-1", "powerClass": "pps5a"
            },
            "saveReceipt": null
        }
    });
    assert!(validate_save_input(&bundle, &candidate).is_ok());
    let mut three_amp_bundle = bundle.clone();
    three_amp_bundle["powerClass"] = json!("pps3a");
    three_amp_bundle["qualificationScope"]["kind"] = json!("default-pps3a-full-batch");
    let mut three_amp_candidate = candidate.clone();
    three_amp_candidate["powerClass"] = json!("pps3a");
    three_amp_candidate["profile"]["bank"] = json!("pps3a");
    three_amp_candidate["promotion"]["previewReceipt"]["powerClass"] = json!("pps3a");
    assert!(validate_save_input(&three_amp_bundle, &three_amp_candidate).is_ok());
    assert!(validate_save_input(&three_amp_bundle, &candidate).is_err());
    assert!(validate_save_input(&bundle, &three_amp_candidate).is_err());
    let mut invalid = bundle.clone();
    invalid["deviceTraceVerification"]["verified"] = json!(false);
    assert!(validate_save_input(&invalid, &candidate).is_err());
    invalid = bundle.clone();
    invalid["terminalDisposition"] = json!("failed");
    assert!(validate_save_input(&invalid, &candidate).is_err());
    let mut invalid_candidate = candidate.clone();
    invalid_candidate["promotion"]["previewReceipt"]["ramAppliedHash"] = json!("other");
    assert!(validate_save_input(&bundle, &invalid_candidate).is_err());
    invalid_candidate = candidate.clone();
    invalid_candidate["promotion"]["saveReceipt"] = json!({"bankHash": "hash-1"});
    assert!(validate_save_input(&bundle, &invalid_candidate).is_err());
    invalid_candidate = candidate.clone();
    invalid_candidate["profile"]["bank"] = json!("pps3a");
    assert!(validate_save_input(&bundle, &invalid_candidate).is_err());

    invalid = bundle.clone();
    invalid["reportRuns"][0]["trials"][0]["disposition"] = json!("rejected");
    assert!(validate_save_input(&invalid, &candidate).is_err());
    invalid = bundle.clone();
    invalid["reportRuns"][0]["trials"][0]["metrics"]["holdConfirmSeconds"] = json!(12);
    assert!(validate_save_input(&invalid, &candidate).is_err());
    invalid = bundle.clone();
    invalid["reportRuns"][0]["trials"][0]["trialPurpose"] = json!("scout");
    assert!(validate_save_input(&invalid, &candidate).is_err());
    invalid = bundle.clone();
    invalid["targetSet"]["selectedC"] = json!([60]);
    assert!(validate_save_input(&invalid, &candidate).is_err());
}

#[test]
fn start_page_advances_ack_and_terminal_waits_for_full_trace() {
    let start = json!({"events": {"nextSequence": 3}, "trace": {"lastSequence": 3}});
    assert_eq!(next_event_sequence(&start), Some(3));
    assert!(event_stream_caught_up(&start, Some(3)));

    let terminal_with_backlog = json!({
        "terminalDisposition": "failed",
        "events": {"nextSequence": 7},
        "trace": {"lastSequence": 12}
    });
    assert!(!event_stream_caught_up(&terminal_with_backlog, Some(7)));
    assert!(event_stream_caught_up(&terminal_with_backlog, Some(12)));
}

#[test]
fn terminal_trace_gap_does_not_wait_for_an_unretained_event() {
    let gap_with_backlog = json!({
        "terminalDisposition": "failed",
        "events": {"events": [{"sequence": 320}], "nextSequence": 320},
        "trace": {"lastSequence": 321, "gap": true}
    });
    assert!(!event_stream_caught_up(&gap_with_backlog, Some(320)));
    let drained_gap = json!({
        "terminalDisposition": "failed",
        "events": {"events": [], "nextSequence": null},
        "trace": {"lastSequence": 321, "gap": true}
    });
    assert!(event_stream_caught_up(&drained_gap, Some(320)));
}

#[test]
fn firmware_start_preserves_recursive_target_order() {
    let command = firmware_start_command(
        "pps5a",
        "run-1",
        "candidate-1",
        "hash-1",
        &json!({}),
        &selected_execution_order(&[60, 80, 100, 120, 140, 160, 180, 220]),
    );
    assert_eq!(
        command["targetsC"],
        json!([60, 220, 120, 80, 100, 160, 140, 180])
    );
    assert_eq!(command["candidateProfile"]["bank"], "pps5a");
}

#[test]
fn firmware_diagnostic_subset_preserves_device_order() {
    let selected = [60, 100, 220];
    let order = selected_execution_order(&selected);
    assert_eq!(order, [60, 220, 100]);
    let command = firmware_start_command(
        "pps3a",
        "run-1",
        "candidate-1",
        "hash-1",
        &json!({}),
        &order,
    );
    assert_eq!(command["targetsC"], json!([60, 220, 100]));
    assert_eq!(command["powerClass"], "pps3a");
    assert_eq!(command["candidateProfile"]["bank"], "pps3a");
}

#[test]
fn firmware_host_watchdog_covers_the_device_budget_per_selected_target() {
    assert_eq!(firmware_host_watchdog(1).as_secs(), 21 * 60);
    assert_eq!(firmware_host_watchdog(8).as_secs(), 8 * 20 * 60 + 60);
}

#[test]
fn hil_failed_endpoint_replay_keeps_cooldown_and_confirm_failure() {
    // Device-local event slice from run firmware-pps5a-1790777485600.
    let samples = vec![
        json!({"sequence": 35, "elapsedMs": 11462, "targetC": 60, "trialIndex": 0, "phase": "warmup", "temperatureCentiC": 3971, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 65, "elapsedMs": 20598, "targetC": 60, "trialIndex": 0, "phase": "hold", "temperatureCentiC": 5802, "heaterRequestedOutputPermille": 0}),
        json!({"sequence": 85, "elapsedMs": 27620, "targetC": 60, "trialIndex": 0, "phase": "hold", "temperatureCentiC": 6409, "heaterRequestedOutputPermille": 0}),
        json!({"sequence": 1562, "elapsedMs": 551458, "targetC": 60, "trialIndex": 1, "phase": "warmup", "temperatureCentiC": 4013, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 1595, "elapsedMs": 561407, "targetC": 60, "trialIndex": 1, "phase": "hold", "temperatureCentiC": 5836, "heaterRequestedOutputPermille": 0}),
        json!({"sequence": 1610, "elapsedMs": 566676, "targetC": 60, "trialIndex": 1, "phase": "hold", "temperatureCentiC": 6160, "heaterRequestedOutputPermille": 0}),
        json!({"sequence": 3395, "elapsedMs": 1200513, "targetC": 240, "trialIndex": 0, "phase": "warmup", "temperatureCentiC": 4834, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 3758, "elapsedMs": 1326766, "targetC": 240, "trialIndex": 0, "phase": "hold", "temperatureCentiC": 23891, "heaterRequestedOutputPermille": 900}),
        json!({"sequence": 3812, "elapsedMs": 1343999, "targetC": 240, "trialIndex": 1, "phase": "warmup", "temperatureCentiC": 19840, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 4005, "elapsedMs": 1411624, "targetC": 240, "trialIndex": 1, "phase": "approach", "temperatureCentiC": 23854, "heaterRequestedOutputPermille": 930}),
    ];
    let ledger = vec![
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"startElapsedMs": 11462, "endElapsedMs": 27620, "startTemperatureCentiC": 3971, "cooldownThresholdCentiC": 4000, "sampleStartSequence": 35, "sampleEndSequence": 85, "endSequence": 86}}),
        json!({"kind": "decision", "targetC": 60, "trialIndex": 0, "decision": {"disposition": "rejected", "purpose": "scout", "stopReason": "full-speed-to-stable budget exhausted", "maxOvershootCentiC": 409, "holdPeakToPeakCentiC": 433, "settleTimeMs": null, "hardGateFullSpeedToStableMs": 10000}}),
        json!({"kind": "candidate_trial", "elapsedMs": 551331, "targetC": 60, "trialIndex": 1, "trial": {"startElapsedMs": 551331, "startTemperatureCentiC": 3999, "cooldownThresholdCentiC": 4000, "endSequence": null}}),
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 1, "trial": {"startElapsedMs": 551458, "endElapsedMs": 566676, "startTemperatureCentiC": 3999, "cooldownThresholdCentiC": 4000, "sampleStartSequence": 1562, "sampleEndSequence": 1610, "endSequence": 1611}}),
        json!({"kind": "decision", "targetC": 60, "trialIndex": 1, "decision": {"disposition": "rejected", "purpose": "batch_current", "stopReason": "full-speed-to-stable budget exhausted", "maxOvershootCentiC": 160, "holdPeakToPeakCentiC": 197, "settleTimeMs": null, "hardGateFullSpeedToStableMs": 10000}}),
        json!({"kind": "decision", "targetC": 60, "trialIndex": 2, "reason": "target_budget_exhausted_during_cooldown"}),
        json!({"kind": "candidate_trial", "targetC": 240, "trialIndex": 0, "trial": {"startElapsedMs": 1200513, "endElapsedMs": 1326766, "startTemperatureCentiC": 4839, "cooldownThresholdCentiC": 20000, "sampleStartSequence": 3395, "sampleEndSequence": 3758, "endSequence": 3759}}),
        json!({"kind": "decision", "targetC": 240, "trialIndex": 0, "decision": {"disposition": "passed", "purpose": "scout", "stopReason": "candidate passed 12s scout hard gates", "maxOvershootCentiC": 2, "holdPeakToPeakCentiC": 120, "settleTimeMs": 2061, "hardGateFullSpeedToStableMs": 5000}}),
        json!({"kind": "candidate_trial", "targetC": 240, "trialIndex": 1, "trial": {"startElapsedMs": 1343999, "endElapsedMs": 1411624, "startTemperatureCentiC": 19965, "cooldownThresholdCentiC": 20000, "sampleStartSequence": 3812, "sampleEndSequence": 4005, "endSequence": 4006}}),
        json!({"kind": "decision", "targetC": 240, "trialIndex": 1, "decision": {"disposition": "rejected", "purpose": "hold_confirm", "stopReason": "full-speed-to-stable budget exhausted", "settleTimeMs": 5095, "hardGateFullSpeedToStableMs": 5000}}),
        json!({"kind": "decision", "targetC": 240, "trialIndex": 14, "reason": "target_budget_exhausted_during_cooldown"}),
    ];
    let runs = build_report_runs(&[60, 240], &samples, &ledger, "candidate-1", "hash-1");
    assert_eq!(runs[0]["reviewOutcome"], "failed");
    assert_eq!(
        runs[0]["stopReason"],
        "target_budget_exhausted_during_cooldown"
    );
    assert_eq!(runs[0]["trials"][0]["maxOvershootC"], 4.09);
    assert_eq!(
        runs[0]["trials"][1]["fullSpeedToStable"]["settleTimeMs"],
        serde_json::Value::Null
    );
    assert_eq!(
        ledger[2]["elapsedMs"].as_u64().unwrap()
            - ledger[0]["trial"]["endElapsedMs"].as_u64().unwrap(),
        523711
    );
    assert_eq!(runs[1]["reviewOutcome"], "failed");
    assert_eq!(
        runs[1]["stopReason"],
        "target_budget_exhausted_during_cooldown"
    );
    assert_eq!(runs[1]["trials"][0]["disposition"], "passed");
    assert_eq!(
        runs[1]["trials"][1]["stopReason"],
        "full-speed-to-stable budget exhausted"
    );
}

#[test]
fn firmware_cli_accepts_only_frozen_5a_diagnostic_targets() {
    let parse = |targets: &str| {
        Cli::try_parse_from([
            "flux-purr",
            "thermal",
            "tune",
            "--engine",
            "firmware",
            "--device",
            "test-device",
            "--tune-targets-c",
            targets,
            "--dry-run",
        ])
        .unwrap()
    };
    let Command::Thermal {
        command: ThermalCommand::Tune(selected),
    } = parse("60,220").command
    else {
        panic!("expected thermal tune command");
    };
    assert_eq!(validate_firmware_args(&selected).unwrap(), [60, 220]);
    let Command::Thermal {
        command: ThermalCommand::Tune(unsupported),
    } = parse("60,240").command
    else {
        panic!("expected thermal tune command");
    };
    assert!(validate_firmware_args(&unsupported).is_err());
}

#[test]
fn interrupted_run_keeps_collected_device_samples_and_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let output_root = dir.path().join("input");
    let report_dir = dir.path().join("report");
    let statuses = vec![json!({
        "runId": "run-1", "candidateId": "candidate-1", "powerClass": "pps5a", "targetsC": [60],
        "terminalDisposition": "running",
        "reviewDisposition": "pending",
        "candidateHash": "hash-device",
        "ppsControlPolicyVersion": "hold-pps-governor-v1",
        "ppsVoltageBounds": {
            "absoluteMinMv": 5000, "absoluteMaxMv": 28000,
            "policyFloorMv": 12000, "apdoMinMv": 11000, "apdoMaxMv": 21000
        },
        "trace": {"firstSequence": 0, "lastSequence": 0, "complete": false, "gap": false, "digest": "unconfirmed"},
        "events": {"events": [{
            "sequence": 0, "elapsedMs": 0, "kind": "sample", "phase": "warmup",
            "targetC": 60, "trialIndex": 0, "reason": null, "trial": null, "decision": null,
            "sample": {
                "sequence": 0, "elapsedMs": 0, "targetC": 60, "trialIndex": 0,
                "candidateId": "candidate-1", "candidateHash": "hash-device",
                "temperatureCentiC": 3500, "vinMv": 12000,
                "ppsContract": {"voltageMv": 12000, "currentMa": 5000, "kind": "pps5a", "confirmed": true},
                "requestedMv": 12000, "ppsPowerCeilingMv": 21000,
                "heaterRequestedOutputPermille": 1000, "heaterOutputPermille": 980,
                "measurementValid": true, "phase": "warmup"
            }
        }]}
    })];
    let rendered = render_interrupted_run(InterruptedRunInput {
        device_id: "device-1",
        output_root: &output_root,
        bundle_dir: Some(report_dir.clone()),
        run_id: "run-1",
        candidate_id: "candidate-1",
        seed_candidate_hash: "hash-seed",
        candidate_profile: &json!({"settings": null, "points": []}),
        statuses: &statuses,
        selected_targets: &[60],
        execution_order: &[60],
        reason: "transport_or_runtime_failure",
    })
    .unwrap();

    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
            .unwrap();
    let samples = std::fs::read_to_string(report_dir.join("samples.ndjson")).unwrap();
    assert_eq!(rendered["ownerOutcome"], "failed");
    assert_eq!(bundle["terminalDisposition"], "failed");
    assert_eq!(bundle["reviewDisposition"], "review_incomplete");
    assert_eq!(bundle["qualificationScope"]["diagnosticOnly"], true);
    assert_eq!(bundle["qualificationScope"]["kind"], "diagnostic-retest");
    assert_eq!(bundle["targetSet"]["selectedC"], json!([60]));
    assert_eq!(
        bundle["hostInterruption"]["lastDeviceTerminalDisposition"],
        "running"
    );
    assert_eq!(bundle["candidate"]["candidateHash"], "hash-device");
    assert_eq!(bundle["ppsControlPolicyVersion"], "hold-pps-governor-v1");
    assert_eq!(bundle["ppsVoltageBounds"]["apdoMaxMv"], 21000);
    assert_eq!(bundle["ppsVoltageBounds"]["availability"], "device_local");
    assert!(samples.contains("\"temperatureCentiC\":3500"));
}

#[tokio::test]
async fn transport_timeout_cancels_and_drains_retained_terminal_events() {
    run_interrupted_transport_case(false, true, false).await;
}

#[test]
fn interrupted_device_journal_renders_retained_samples_without_inventing_a_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let status = device_journal_fixture();
    std::fs::write(
        directory.path().join("device-statuses.ndjson"),
        serde_json::to_string(&status).unwrap() + "\n",
    )
    .unwrap();
    record_device_identity(
        directory.path(),
        "journal-run",
        &json!({
            "deviceId": "bench", "firmwareVersion": "0.27.6-dev.journal",
            "gitSha": "0123456789abcdef0123456789abcdef01234567", "buildId": "journal-build"
        }),
    )
    .unwrap();
    super::prepare_v2_device_journal(directory.path()).unwrap();
    let rendered =
        super::thermal_tuning_v2_report::render_v2_bundle(super::ThermalTuningV2ReportInput {
            input_dir: directory.path().to_path_buf(),
            output_dir: None,
        })
        .unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join("rendered/run.bundle.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(rendered["ownerOutcome"], "failed");
    assert_eq!(bundle["reviewDisposition"], "review_incomplete");
    assert_eq!(bundle["hostInterruption"]["reason"], "host_process_stopped");
    assert_eq!(
        bundle["hostInterruption"]["lastDeviceTerminalDisposition"],
        "running"
    );
    assert_eq!(bundle["firmware"]["version"], "0.27.6-dev.journal");
    assert_eq!(bundle["eventCounts"]["sample"], 1);
    let ledger =
        std::fs::read_to_string(directory.path().join("rendered/decision-ledger.ndjson")).unwrap();
    assert!(
        ledger.trim().is_empty(),
        "missing terminal events must remain missing"
    );
    let raw: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(directory.path().join("device-statuses.ndjson"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(
        raw, status,
        "offline conversion must preserve the original device journal"
    );
}

#[test]
fn offline_device_journal_rejects_mixed_runs_scope_and_candidate_hashes() {
    let first = device_journal_fixture();
    for field in [
        "runId",
        "candidateId",
        "targetsC",
        "powerClass",
        "candidateHash",
    ] {
        let mut other = first.clone();
        other[field] = json!("different");
        assert!(
            super::validate_device_journal_identity(&[first.clone(), other], &first).is_err(),
            "{field}"
        );
    }
}

fn device_journal_fixture() -> serde_json::Value {
    let (seed, _) = super::super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let profile = super::materialize_firmware_seed_profile(seed).unwrap();
    let hash = firmware_candidate_profile_hash(&profile).unwrap();
    json!({
        "runId": "journal-run", "candidateId": "journal-candidate", "candidateHash": hash,
        "candidateProfile": profile, "powerClass": "pps5a", "targetsC": [60],
        "terminalDisposition": "running", "reviewDisposition": "pending", "armed": true,
        "trace": {"firstSequence": 0, "lastSequence": 0, "complete": false, "gap": false, "digest": "unconfirmed"},
        "events": {"events": [{"sequence": 0, "elapsedMs": 0, "kind": "sample", "phase": "cooldown", "targetC": 60, "trialIndex": 0,
            "reason": null, "trial": null, "decision": null, "sample": {
                "sequence": 0, "elapsedMs": 0, "targetC": 60, "trialIndex": 0, "candidateId": "journal-candidate", "candidateHash": hash,
                "temperatureCentiC": 3600, "vinMv": 12000, "ppsContract": {"voltageMv": 12000, "currentMa": 5000, "kind": "pps5a", "confirmed": true},
                "requestedMv": 12000, "ppsPowerCeilingMv": 21000, "heaterRequestedOutputPermille": 0, "heaterOutputPermille": 0,
                "measurementValid": true, "phase": "cooldown"
            }}]}
    })
}

#[tokio::test]
async fn transport_timeout_with_lost_events_remains_a_failed_trace_gap() {
    run_interrupted_transport_case(true, true, false).await;
}

#[tokio::test]
async fn unavailable_device_identity_fails_before_any_tuning_command() {
    run_interrupted_transport_case(false, false, false).await;
}

#[tokio::test]
async fn ctrl_c_cancels_drains_reports_and_releases_the_lease() {
    run_interrupted_transport_case(false, true, true).await;
}

async fn run_interrupted_transport_case(
    trace_gap: bool,
    identity_available: bool,
    interrupt_requested: bool,
) {
    use std::sync::{Arc, Mutex};

    let directory = tempfile::tempdir().unwrap();
    let output_root = directory.path().join("input");
    let report_dir = directory.path().join("report");
    let commands = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let (interrupt_sender, interrupt_receiver) = tokio::sync::oneshot::channel();
    let interrupt_sender = Arc::new(Mutex::new(Some(interrupt_sender)));
    let lease_released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let app = interrupted_transport_app(
        commands.clone(),
        interrupt_sender,
        lease_released.clone(),
        identity_available,
        trace_gap,
        interrupt_requested,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let cli = Cli::parse_from([
        "flux-purr",
        "thermal",
        "tune",
        "--engine",
        "firmware",
        "--device",
        "bench",
        "--tune-targets-c",
        "60",
        "--output-root",
        output_root.to_str().unwrap(),
        "--bundle-dir",
        report_dir.to_str().unwrap(),
    ]);
    let Command::Thermal {
        command: ThermalCommand::Tune(args),
    } = cli.command
    else {
        panic!("expected firmware tune command");
    };
    let interrupt = async move {
        if interrupt_requested {
            interrupt_receiver.await.map_err(std::io::Error::other)
        } else {
            std::future::pending().await
        }
    };
    let rendered =
        super::run_firmware_tuning_until_interrupt(&reqwest::Client::new(), &base, args, interrupt)
            .await
            .unwrap();
    server.abort();
    assert!(lease_released.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(rendered["ownerOutcome"], "failed");
    if !identity_available {
        assert!(commands.lock().unwrap().is_empty());
        let bundle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
                .unwrap();
        assert_eq!(bundle["rejectionReason"], "device_identity_not_confirmed");
        assert_eq!(
            bundle["firmware"]["availability"],
            "unavailable_device_identity"
        );
        return;
    }
    assert_interrupted_transport_evidence(
        &commands,
        &rendered,
        &report_dir,
        trace_gap,
        interrupt_requested,
    );
}

fn interruption_events() -> (Vec<Value>, String) {
    let events: Vec<serde_json::Value> = (0..3)
        .map(|sequence| {
            json!({
                "sequence": sequence, "elapsedMs": sequence * 350,
                "kind": if sequence == 2 { "safety" } else { "phase_transition" },
                "phase": if sequence == 2 { "canceled" } else { "cooldown" },
                "targetC": 60, "trialIndex": 0,
                "reason": if sequence == 2 { Some("canceled") } else { None },
                "sample": null, "trial": null, "decision": null
            })
        })
        .collect();
    let typed_events: Vec<ThermalTuningEvent> = events
        .iter()
        .map(|event| serde_json::from_value(event.clone()).unwrap())
        .collect();
    let digest = digest_event_chain(&typed_events);
    (events, digest.to_string())
}

fn interrupted_transport_app(
    recorded_commands: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    interrupt_sender: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    released: std::sync::Arc<std::sync::atomic::AtomicBool>,
    identity_available: bool,
    trace_gap: bool,
    interrupt_requested: bool,
) -> axum::Router {
    use axum::{
        Json, Router,
        routing::{delete, get, post},
    };
    let (events, digest) = interruption_events();
    Router::new()
        .route("/api/v1/devices/bench/leases", post(|| async {
            Json(json!({"leaseId": "test", "ttlMs": 10000}))
        }))
        .route("/api/v1/leases/test/heartbeat", post(|| async { Json(json!({})) }))
        .route("/api/v1/leases/test", delete(move || {
            let released = released.clone();
            async move {
                released.store(true, std::sync::atomic::Ordering::SeqCst);
                Json(json!({}))
            }
        }))
        .route("/api/v1/devices/bench/identity", get(move || async move {
            Json(if identity_available {
                json!({"deviceId": "bench-identity", "firmwareVersion": "0.27.6-dev.capture",
                    "gitSha": "0123456789abcdef0123456789abcdef01234567", "buildId": "captured-build"})
            } else {
                json!({"deviceId": "bench-identity"})
            })
        }))
        .route("/api/v1/devices/bench/status", get(|| async {
            Json(json!({"thermalProfileMode": "100w", "thermalProfileResolvedBank": "pps5a", "manualPpsEnabled": false,
                "currentTempC": 43.5, "heaterPhysicalOutputPercent": 0,
                "ppsCapabilityMaxMv": 21000, "ppsCapabilityMaxMa": 5000,
                "thermalPlantModel": {"activeTransactionId": 7, "projectionValid": true},
                "fanEnabled": true, "fanOutputLevel": "high", "fanPwmPermille": 0}))
        }))
        .route("/api/v1/devices/bench/calibration/thermal-plant/run", get(|| async {
            Json(cold_start_calibration(2100, 25692.367, 179.62439, 3.0247352e-7))
        }))
        .route("/api/v1/devices/bench/thermal-tuning", post(move |Json(command): Json<serde_json::Value>| {
            let commands = recorded_commands.clone();
            let events = events.clone();
            let digest = digest.clone();
            let interrupt_sender = interrupt_sender.clone();
            async move {
                let (canceled, reads, start) = {
                    let mut commands = commands.lock().unwrap();
                    commands.push(command.clone());
                    (
                        commands.iter().any(|command| command["op"] == "cancel"),
                        commands.iter().filter(|command| command["op"] == "read").count(),
                        commands.iter().find(|command| command["op"] == "start").unwrap().clone(),
                    )
                };
                if command["op"] == "read" && !canceled && reads > 1 {
                    trigger_test_interrupt(interrupt_requested, &interrupt_sender).await;
                    return (axum::http::StatusCode::GATEWAY_TIMEOUT,
                        Json(json!({"error": {"code": "usb_response_timeout", "retryable": true}})));
                }
                let next = if command["op"] == "read" {
                    command["afterSequence"].as_u64().map_or(0, |sequence| sequence + 1)
                } else { 0 };
                let page: Vec<_> = events.iter().filter(|event| event["sequence"].as_u64() == Some(next) && (!trace_gap || next == 0) && (canceled || next == 0))
                    .cloned().collect();
                (axum::http::StatusCode::OK, Json(json!({
                    "runId": start["runId"], "candidateId": start["candidateId"],
                    "candidateHash": start["candidateHash"], "powerClass": "pps5a",
                    "armed": !canceled, "phase": if canceled { "canceled" } else { "cooldown" },
                    "terminalDisposition": if canceled { "canceled" } else { "running" },
                    "reviewDisposition": "review_incomplete",
                    "trace": {"firstSequence": 0, "lastSequence": if canceled { 2 } else { 0 },
                        "complete": canceled && !trace_gap, "gap": canceled && trace_gap, "digest": digest},
                    "events": {"nextSequence": page.last().map(|event| event["sequence"].clone()), "events": page}
                })))
            }
        }))
}

async fn trigger_test_interrupt(
    requested: bool,
    sender: &std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
) {
    if requested {
        sender.lock().unwrap().take().unwrap().send(()).unwrap();
        std::future::pending::<()>().await;
    }
}

fn assert_interrupted_transport_evidence(
    commands: &std::sync::Mutex<Vec<Value>>,
    rendered: &Value,
    report_dir: &std::path::Path,
    trace_gap: bool,
    interrupt_requested: bool,
) {
    let start_profile = commands
        .lock()
        .unwrap()
        .iter()
        .find(|command| command["op"] == "start")
        .unwrap()["candidateProfile"]["profile"]
        .clone();
    assert_ne!(
        start_profile["points"][0]["brakeDistanceCentiC"], 1310,
        "the real start command must carry the calibration-derived seed"
    );
    let seed_path = std::path::PathBuf::from(rendered["rawInputDir"].as_str().unwrap())
        .join("seed-calibration.json");
    let seed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(seed_path).unwrap()).unwrap();
    assert_eq!(seed["source"], "device_active_calibration");
    assert_eq!(seed["candidateProfile"], start_profile);
    let runtime_path = std::path::PathBuf::from(rendered["rawInputDir"].as_str().unwrap())
        .join("device-runtime-statuses.ndjson");
    let runtime_lines = std::fs::read_to_string(runtime_path).unwrap();
    let runtime: Vec<serde_json::Value> = runtime_lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        runtime.len(),
        2,
        "preflight and running snapshots must both be durable"
    );
    assert!(runtime[0]["afterSequence"].is_null());
    assert_eq!(runtime[1]["afterSequence"], 0);
    assert_eq!(runtime[1]["status"]["fanEnabled"], true);
    assert_eq!(runtime[1]["status"]["fanPwmPermille"], 0);
    assert_eq!(runtime[1]["status"]["heaterPhysicalOutputPercent"], 0);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
            .unwrap();
    assert_eq!(bundle["firmware"]["version"], "0.27.6-dev.capture");
    assert_eq!(bundle["device"]["id"], "bench-identity");
    assert_eq!(
        bundle["hostInterruption"]["reason"],
        if interrupt_requested {
            "host_canceled"
        } else {
            "transport_or_runtime_failure"
        },
    );
    assert_eq!(
        bundle["deviceTraceVerification"]["verified"], !trace_gap,
        "cancel must retrieve the retained tail before rendering the failure report"
    );
    let reads_after_cancel: Vec<_> = commands
        .lock()
        .unwrap()
        .iter()
        .skip_while(|command| command["op"] != "cancel")
        .skip(1)
        .map(|command| command["afterSequence"].clone())
        .collect();
    if trace_gap {
        assert_eq!(reads_after_cancel, [json!(0)]);
        assert_eq!(bundle["deviceTrace"]["gap"], true);
    } else {
        assert_eq!(reads_after_cancel, [json!(0), json!(1), json!(2)]);
    }
}

#[test]
fn closed_candidate_trial_replaces_its_open_event() {
    let ledger = vec![
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"startSequence": 1, "endSequence": null}}),
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"startSequence": 1, "endSequence": 9}}),
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 1, "trial": {"startSequence": 10, "endSequence": null}}),
    ];
    let trials = trial_events_for_target(&ledger, 60);
    assert_eq!(trials.len(), 2);
    assert_eq!(trials[0]["trial"]["endSequence"], 9);
    assert!(trials[1]["trial"]["endSequence"].is_null());
}

#[test]
fn target_without_an_adopted_trial_cannot_inherit_the_device_passed_terminal() {
    let runs = build_report_runs(&[60], &[], &[], "candidate-1", "hash-1");
    assert_eq!(runs[0]["reviewOutcome"], "failed");
    assert_eq!(runs[0]["reviewPassed"], false);
    assert_eq!(runs[0]["trialCount"], 0);
}

#[test]
fn elapsed_scout_cannot_be_reported_as_independent_confirmation() {
    let samples = vec![
        json!({"sequence": 1, "targetC": 60, "trialIndex": 0, "phase": "warmup", "elapsedMs": 1000, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 2, "targetC": 60, "trialIndex": 0, "phase": "approach", "elapsedMs": 2000, "heaterRequestedOutputPermille": 400}),
        json!({"sequence": 3, "targetC": 60, "trialIndex": 0, "phase": "hold", "elapsedMs": 82000, "heaterRequestedOutputPermille": 400}),
    ];
    let ledger = vec![
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 0, "trial": {"purpose": "scout", "sampleStartSequence": 1, "sampleEndSequence": 3}}),
        json!({"kind": "decision", "targetC": 60, "trialIndex": 0, "decision": {"purpose": "scout", "disposition": "adopted", "holdConfirm": true, "maxOvershootCentiC": 100, "holdPeakToPeakCentiC": 100, "settleTimeMs": 1000, "hardGateFullSpeedToStableMs": 10000}}),
    ];
    let runs = build_report_runs(&[60], &samples, &ledger, "candidate-1", "hash-1");
    assert_eq!(runs[0]["reviewOutcome"], "failed");
    assert_eq!(runs[0]["reviewPassed"], false);
    assert!(runs[0]["adoptedTrialIndex"].is_null());
    let trial = &runs[0]["trials"][0];
    assert_eq!(
        trial["disposition"], "adopted",
        "retain the device's incorrect raw fact"
    );
    assert_eq!(trial["reviewOutcome"], "failed");
    assert!(trial["metrics"]["holdConfirmSeconds"].is_null());
}

#[test]
fn confirmed_trial_projection_preserves_bound_target_and_timing() {
    let timing = json!({"policyVersion": 2, "originalLimitMs": 10000, "extensionLimitMs": 30000,
        "allowedApproachFloorPermille": 100, "effectiveApproachFloorPermille": 100,
        "configuredApproachFloorPermille": 100, "originalDeadlineObservedAtMs": 11000,
        "originalDeadlinePhase": "approach",
        "floorObservedAtMs": 2000, "extendedAtMs": 11000, "warmupExitedAtMs": 1000,
        "stableWindowStartedAtMs": 13000, "stableWindowVerifiedAtMs": 23000});
    let mut samples = vec![
        json!({"sequence": 1, "targetC": 60, "trialIndex": 1, "phase": "warmup", "elapsedMs": 0, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 2, "targetC": 60, "trialIndex": 1, "phase": "approach", "elapsedMs": 1000, "heaterRequestedOutputPermille": 110, "temperatureCentiC": 5000, "measurementValid": true}),
        json!({"sequence": 3, "targetC": 60, "trialIndex": 1, "phase": "approach", "elapsedMs": 2000, "heaterRequestedOutputPermille": 100, "temperatureCentiC": 5300, "measurementValid": true}),
        json!({"sequence": 4, "targetC": 60, "trialIndex": 1, "phase": "approach", "elapsedMs": 11000, "heaterRequestedOutputPermille": 100, "temperatureCentiC": 5800, "measurementValid": true}),
    ];
    for time in (13000..=73000).step_by(250) {
        samples.push(
            json!({"sequence": samples.len() + 1, "targetC": 60, "trialIndex": 1,
            "phase": "hold", "elapsedMs": time, "heaterRequestedOutputPermille": 100,
            "temperatureCentiC": 6000, "measurementValid": true}),
        );
    }
    let sample_end = samples.len();
    let ledger = vec![
        json!({"kind": "candidate_trial", "targetC": 60, "trialIndex": 1, "trial": {"purpose": "hold_confirm", "sampleStartSequence": 1, "sampleEndSequence": sample_end, "timing": timing, "candidate": {"point": {"approachFloorPowerPermille": 100, "holdPowerPermille": 100}}}}),
        json!({"kind": "decision", "targetC": 60, "trialIndex": 1, "decision": {"purpose": "hold_confirm", "disposition": "adopted", "holdConfirm": true, "maxOvershootCentiC": 100, "holdPeakToPeakCentiC": 100, "settleTimeMs": 12000, "hardGateFullSpeedToStableMs": 30000, "timing": timing}}),
    ];
    let runs = build_report_runs(&[60], &samples, &ledger, "candidate-1", "hash-1");
    assert_eq!(runs[0]["reviewOutcome"], "passed");
    assert_eq!(runs[0]["adoptedTrialIndex"], 1);
    assert_eq!(runs[0]["trials"][0]["targetC"], 60);
    assert!(super::adopted_trial_passes_gates(&runs[0]["trials"][0]));
    let mut recovering = samples.clone();
    for sample in &mut recovering {
        let time = sample["elapsedMs"].as_u64().unwrap();
        if (30_000..32_000).contains(&time) {
            sample["phase"] = json!("approach");
        }
    }
    let rejected = build_report_runs(&[60], &recovering, &ledger, "candidate-1", "hash-1");
    let trial = &rejected[0]["trials"][0];
    assert_eq!(trial["disposition"], "adopted");
    assert_eq!(trial["deviceHoldConfirm"], true);
    assert_eq!(trial["reviewOutcome"], "failed");
    assert_eq!(
        trial["holdEvidenceVerification"]["continuousHoldMs"],
        41_000
    );
    assert!(trial["metrics"]["holdConfirmSeconds"].is_null());
    for (sample_index, field, value) in [
        (2, "heaterRequestedOutputPermille", json!(225)),
        (2, "measurementValid", json!(false)),
        (3, "phase", json!("hold")),
        (3, "temperatureCentiC", json!(6000)),
        (5, "temperatureCentiC", json!(6200)),
    ] {
        let mut changed = samples.clone();
        changed[sample_index][field] = value;
        let runs = build_report_runs(&[60], &changed, &ledger, "candidate-1", "hash-1");
        assert_eq!(runs[0]["reviewOutcome"], "failed", "{field}");
        assert_eq!(runs[0]["trials"][0]["disposition"], "adopted");
    }
}

#[test]
fn continuous_hold_verification_preserves_only_bounded_pps_pauses() {
    let timing: super::ThermalTuningTimingEvidence = serde_json::from_value(json!({
        "policyVersion": 5, "originalLimitMs": 5000, "extensionLimitMs": 15000,
        "allowedApproachFloorPermille": 50, "effectiveApproachFloorPermille": 50,
        "modelApproachFloorPermille": 50, "configuredApproachFloorPermille": 50
    }))
    .unwrap();
    let mut samples: Vec<Value> = (0..=240)
        .map(|index| {
            json!({
                "sequence": index, "elapsedMs": index * 250, "phase": "hold",
                "temperatureCentiC": 22000, "rawTemperatureCentiC": 22000,
                "measurementValid": true, "measurementAccepted": true,
                "ppsRequestedMv": if index < 120 { 21000 } else { 20500 },
                "ppsContractMv": if index < 120 { 21000 } else { 20500 },
                "ppsContractConfirmed": true
            })
        })
        .collect();
    samples[120]["measurementAccepted"] = json!(false);
    let verify = |samples: &[Value]| {
        super::verify_hold_samples(Some(&timing), 220, &samples.iter().collect::<Vec<_>>())
    };
    assert_eq!(verify(&samples)["verified"], true);
    for field in ["ppsRequestedMv", "ppsContractMv"] {
        for invalid in [json!(20500.5), json!(-1), json!(65536)] {
            let mut malformed = samples.clone();
            for sample in &mut malformed {
                sample["ppsRequestedMv"] = json!(21000);
                sample["ppsContractMv"] = json!(21000);
            }
            for sample in malformed.iter_mut().take(120) {
                sample[field] = invalid.clone();
            }
            assert_eq!(verify(&malformed)["verified"], false, "{field}={invalid}");
        }
    }
    samples[120]["phase"] = json!("approach");
    assert_eq!(verify(&samples)["verified"], false);
    samples[120]["phase"] = json!("hold");
    samples[120]["measurementValid"] = json!(false);
    assert_eq!(verify(&samples)["verified"], false);
    samples[120]["measurementValid"] = json!(true);
    let mut missing = samples.clone();
    missing.drain(118..124);
    assert_eq!(verify(&missing)["verified"], false);
    let sparse: Vec<_> = samples.iter().step_by(2).cloned().collect();
    assert_eq!(verify(&sparse)["verified"], false);
    for field in ["temperatureCentiC", "rawTemperatureCentiC"] {
        let mut malformed = samples.clone();
        malformed[120][field] = json!(22000.5);
        assert_eq!(verify(&malformed)["verified"], false, "{field}");
    }
    samples[120]["elapsedMs"] = samples[119]["elapsedMs"].clone();
    assert_eq!(verify(&samples)["reasons"][0], "invalid_sample_order");
}

#[test]
fn trial_report_excludes_cooldown_and_other_phase_warmup_requests() {
    let trial = json!({
        "targetC": 60, "trialIndex": 1,
        "trial": {
            "sampleStartSequence": 2, "sampleEndSequence": 4,
            "startTemperatureCentiC": 3_987, "cooldownThresholdCentiC": 4_000,
            "candidate": {"candidateId": "candidate-1", "candidateHash": "hash-1"}
        }
    });
    let samples = vec![
        json!({"sequence": 1, "targetC": 60, "trialIndex": 1, "phase": "cooldown", "elapsedMs": 1000, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 2, "targetC": 60, "trialIndex": 1, "phase": "warmup", "elapsedMs": 2000, "heaterRequestedOutputPermille": 995}),
        json!({"sequence": 3, "targetC": 60, "trialIndex": 1, "phase": "approach", "elapsedMs": 3000, "heaterRequestedOutputPermille": 1000}),
        json!({"sequence": 4, "targetC": 60, "trialIndex": 1, "phase": "hold", "elapsedMs": 4000, "heaterRequestedOutputPermille": 400}),
        json!({"sequence": 5, "targetC": 60, "trialIndex": 1, "phase": "cooldown", "elapsedMs": 5000, "heaterRequestedOutputPermille": 1000}),
    ];
    let report = build_report_trial(60, &trial, &samples, &[], "candidate-1", "hash-1");
    assert_eq!(report["warmupRequestedOutputPermille"], 995);
    assert_eq!(report["firstSequence"], 2);
    assert_eq!(report["lastSequence"], 4);
    assert_eq!(report["metrics"]["phasePath"], "warmup -> approach -> hold");
    assert_eq!(report["cooldownGate"]["startTemperatureCentiC"], 3_987);
    assert_eq!(report["cooldownGate"]["thresholdCentiC"], 4_000);
    let mut historical = trial.clone();
    historical["trial"]
        .as_object_mut()
        .unwrap()
        .remove("startTemperatureCentiC");
    historical["trial"]
        .as_object_mut()
        .unwrap()
        .remove("cooldownThresholdCentiC");
    let old_report = build_report_trial(60, &historical, &samples, &[], "candidate-1", "hash-1");
    assert!(old_report["cooldownGate"]["startTemperatureCentiC"].is_null());
    assert!(old_report["cooldownGate"]["thresholdCentiC"].is_null());
}

#[test]
fn verifies_device_event_digest_and_rejects_missing_or_changed_evidence() {
    let event: ThermalTuningEvent = serde_json::from_value(json!({
        "sequence": 0,
        "elapsedMs": 0,
        "kind": "safety",
        "phase": "failed",
        "targetC": null,
        "trialIndex": null,
        "sample": null,
        "reason": "pps3a_not_supported",
        "trial": null,
        "decision": null
    }))
    .unwrap();
    let digest = digest_event_chain(std::slice::from_ref(&event));
    assert_eq!(
        digest.as_str(),
        "4ea3bc9c23cbf417bbcd6468998d28c493745c28305d92ead01274c3a2f4f9d3"
    );
    let mut status = json!({
        "events": {"events": [serde_json::to_value(&event).unwrap()]},
        "trace": {"firstSequence": 0, "lastSequence": 0, "complete": true, "gap": false, "digest": digest.as_str()}
    });
    assert_eq!(verify_device_trace(&[status.clone()]).1["verified"], true);

    status["trace"]["digest"] = json!("wrong");
    assert_eq!(verify_device_trace(&[status.clone()]).1["verified"], false);

    status["trace"]["digest"] = json!(digest.as_str());
    status["trace"]["lastSequence"] = json!(1);
    assert_eq!(verify_device_trace(&[status]).1["verified"], false);
}

#[test]
fn default_5a_candidate_hash_matches_firmware_sanitized_profile() {
    let (seed, _) = super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let profile =
        firmware_candidate_profile_value(&super::thermal_candidate_profile_to_value(&seed));
    assert_eq!(profile["points"].as_array().unwrap().len(), 10);
    let host_hash = firmware_candidate_profile_hash(&profile).unwrap();
    let mut wire_value = profile.clone();
    let points = wire_value["points"].as_array_mut().unwrap();
    while points.len() < flux_purr_firmware::frontpanel::FRONTPANEL_PRESET_COUNT {
        points.push(serde_json::Value::Null);
    }
    let wire: ThermalControlProfileWire = serde_json::from_value(wire_value).unwrap();
    let mut config: ThermalControlProfileConfig = wire.into();
    config.sanitize();
    assert_eq!(hash_profile(&config).unwrap().as_str(), host_hash);
}

#[test]
fn default_5a_seed_materializes_every_firmware_target() {
    let (seed, _) = super::load_thermal_default_seed_candidate_profile("pps5a").unwrap();
    let profile = super::materialize_firmware_seed_profile(seed).unwrap();
    let targets: Vec<i16> = profile["points"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|point| point.get("targetTempC").and_then(serde_json::Value::as_i64))
        .map(|target| target as i16)
        .collect();
    assert_eq!(targets, vec![60, 80, 100, 120, 140, 160, 180, 220]);
    assert_eq!(profile["points"].as_array().unwrap().len(), 10);
    assert!(super::firmware_candidate_profile_hash(&profile).is_ok());
}

#[test]
fn pps3a_preflight_creates_a_five_file_failure_without_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let report_dir = dir.path().join("report");
    let result = render_unsupported_pps3a(
        "test-device",
        &dir.path().join("input"),
        Some(report_dir.clone()),
    )
    .unwrap();
    assert_eq!(result["ownerOutcome"], "failed");
    assert_eq!(result["trace"]["complete"], true);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
            .unwrap();
    assert_eq!(bundle["powerClass"], "pps3a");
    assert_eq!(bundle["candidate"]["candidateId"], serde_json::Value::Null);
    assert_eq!(bundle["traceOrigin"], "host_preflight");
    assert_eq!(bundle["eventCounts"]["safety"], 1);
    assert_eq!(
        std::fs::read_to_string(report_dir.join("samples.ndjson")).unwrap(),
        ""
    );
}

#[test]
fn unconfirmed_start_keeps_attempted_identity_and_never_qualifies() {
    let dir = tempfile::tempdir().unwrap();
    let report_dir = dir.path().join("report");
    let profile = json!({"points": []});
    let result = render_host_failure(super::HostFailureInput {
        device_id: "test-device",
        output_root: &dir.path().join("input"),
        bundle_dir: Some(report_dir.clone()),
        run_id: Some("attempted-run"),
        power_class: "pps5a",
        reason: "start_not_confirmed",
        no_device_command: false,
        attempted_candidate: Some(("attempted-candidate", "sha256:attempted", &profile)),
        selected_targets: Some(&[60]),
    })
    .unwrap();
    assert_eq!(result["ownerOutcome"], "failed");
    assert_eq!(result["trace"]["complete"], false);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report_dir.join("run.bundle.json")).unwrap())
            .unwrap();
    assert_eq!(bundle["runId"], "attempted-run");
    assert_eq!(bundle["candidate"]["candidateId"], "attempted-candidate");
    assert_eq!(bundle["rejectionReason"], "start_not_confirmed");
    let candidate: serde_json::Value = serde_json::from_slice(
        &std::fs::read(report_dir.join("thermal-profile.candidate.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(candidate["profile"], profile);
    assert_eq!(candidate["promotion"]["state"], "unavailable");
}

#[test]
fn rejected_start_requires_matching_device_identity_and_disarmed_failure() {
    let mut status = json!({
        "runId": "run-1",
        "candidateId": "candidate-1",
        "candidateHash": "hash-1",
        "powerClass": "pps5a",
        "terminalDisposition": "failed",
        "armed": false
    });
    assert!(is_confirmed_rejected_start(
        "pps5a",
        &status,
        "run-1",
        "candidate-1",
        "hash-1"
    ));
    status["candidateHash"] = json!("other-hash");
    assert!(!is_confirmed_rejected_start(
        "pps5a",
        &status,
        "run-1",
        "candidate-1",
        "hash-1"
    ));
    status["candidateHash"] = json!("hash-1");
    status["armed"] = json!(true);
    assert!(!is_confirmed_rejected_start(
        "pps5a",
        &status,
        "run-1",
        "candidate-1",
        "hash-1"
    ));
}
