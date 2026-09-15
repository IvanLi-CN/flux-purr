use super::*;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};

struct SnapshotFixtureReader {
    bytes: VecDeque<u8>,
}

impl SnapshotFixtureReader {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.iter().copied().collect(),
        }
    }
}

impl Read for SnapshotFixtureReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(byte) = self.bytes.pop_front() else {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "fixture timeout"));
        };
        buf[0] = byte;
        Ok(1)
    }
}

#[test]
fn encodes_device_id_as_single_path_segment() {
    assert_eq!(
        encode_path_segment("serial-303a-1001-D0:CF"),
        "serial-303a-1001-D0%3ACF"
    );
}

#[test]
fn output_enable_requires_explicit_target_selector() {
    let err = resolve_target(
        TargetSelector {
            device: Some("a".to_string()),
            hardware: Some("b".to_string()),
        },
        DEFAULT_DEVD_ENDPOINT,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("only one"));
}

#[test]
fn eeprom_maintenance_commands_are_explicit_advanced_cli_operations() {
    let export = Cli::try_parse_from([
        "flux-purr",
        "eeprom",
        "export",
        "--device",
        "serial-1",
        "--output",
        "backup.bin",
    ])
    .unwrap();
    assert!(matches!(
        export.command,
        Command::Eeprom {
            command: EepromCommand::Export(_)
        }
    ));

    let erase = Cli::try_parse_from([
        "flux-purr",
        "eeprom",
        "erase",
        "--device",
        "serial-1",
        "--confirm",
        "ERASE EEPROM",
    ])
    .unwrap();
    assert!(matches!(
        erase.command,
        Command::Eeprom {
            command: EepromCommand::Erase(_)
        }
    ));
}

#[test]
fn hardware_upsert_preserves_existing_name_when_unspecified() {
    let mut registry = HardwareRegistry::default();
    upsert_hardware(
        &mut registry,
        SavedHardware {
            id: "bench".to_string(),
            name: Some("Bench".to_string()),
            transport: SavedTransport::Usb,
            device: "dev-1".to_string(),
            devd: Some(DEFAULT_DEVD_ENDPOINT.to_string()),
            last_seen_unix_seconds: Some(1),
        },
    );
    let updated = upsert_hardware(
        &mut registry,
        SavedHardware {
            id: "bench".to_string(),
            name: None,
            transport: SavedTransport::Usb,
            device: "dev-2".to_string(),
            devd: Some(DEFAULT_DEVD_ENDPOINT.to_string()),
            last_seen_unix_seconds: Some(2),
        },
    );
    assert_eq!(updated.name.as_deref(), Some("Bench"));
    assert_eq!(registry.hardware[0].device, "dev-2");
}

#[test]
fn redacts_nested_cli_secrets() {
    let payload = json!({"wifi": {"password": "secret"}, "token": "abc"});
    let redacted = redact_cli_sensitive(&payload);
    assert_eq!(redacted["wifi"]["password"], "<redacted>");
    assert_eq!(redacted["token"], "<redacted>");
}

#[test]
fn renders_active_lan_pairing_code() {
    assert_eq!(
        render_human(&json!({ "active": true, "code": "4827" })).unwrap(),
        "LAN pairing code: 4827"
    );
}

#[test]
fn renders_completed_developer_flash_with_espflash_stages_and_output() {
    let rendered = render_human(&json!({
        "ok": true,
        "operation": "flash",
        "espflash": {
            "phase": "complete",
            "phases": ["connect", "erase", "write", "verify"],
            "diagnosis": "completed",
            "stdout": "Writing at 0x00010000...\nHash of data verified.",
            "stderr": "",
        }
    }))
    .unwrap();

    assert!(rendered.contains("flash completed"));
    assert!(rendered.contains("connect -> erase -> write -> verify"));
    assert!(rendered.contains("Hash of data verified"));
    assert!(rendered.contains("stderr:\n<empty>"));
}

#[test]
fn renders_calibration_collect_summary() {
    let payload = json!({
        "runId": "cal-1",
        "sampleCount": 42,
        "stopReason": "temperature_threshold",
        "complete": true,
    });
    let rendered = render_human(&payload).unwrap();
    assert!(
        rendered
            .contains("Calibration run cal-1: 42 samples stop=temperature_threshold complete=true")
    );
}

#[test]
fn renders_thermal_self_test_summary() {
    let payload = json!({
        "kind": "thermal_self_test",
        "runId": "thermal-1",
        "sampleCount": 36,
        "validation": { "passed": true },
    });
    let rendered = render_human(&payload).unwrap();
    assert!(rendered.contains("Thermal self-test thermal-1: 36 samples passed=true"));
}

#[test]
fn renders_thermal_replay_summary() {
    let payload = json!({
        "kind": "thermal_self_test_replay",
        "runId": "thermal-1",
        "sampleCount": 36,
        "validation": { "passed": false },
    });
    let rendered = render_human(&payload).unwrap();
    assert!(rendered.contains("Thermal replay thermal-1: 36 samples passed=false"));
}

#[test]
fn thermal_candidate_profile_excludes_300c() {
    let profile = default_thermal_candidate_profile();
    let targets = profile["points"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|point| point.get("targetTempC").and_then(Value::as_i64))
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![60, 100, 140, 180, 220, 250]);
    assert_eq!(profile["points"].as_array().unwrap().len(), 10);
}

#[test]
fn parse_thermal_targets_defaults_and_sorts_subset() {
    assert_eq!(
        parse_thermal_targets(None).unwrap(),
        THERMAL_SELF_TEST_DEFAULT_TARGETS_C.to_vec()
    );
    assert_eq!(
        parse_thermal_targets(Some("250,140,250")).unwrap(),
        vec![140, 250]
    );
    assert_eq!(
        parse_thermal_targets(Some("220,100,220")).unwrap(),
        vec![100, 220]
    );
    assert_eq!(
        parse_thermal_targets(Some("240,80,120,80")).unwrap(),
        vec![80, 120, 240]
    );
}

#[test]
fn parse_thermal_targets_preserves_requested_order() {
    assert_eq!(
        parse_thermal_targets_preserve_order(Some("220,100,220")).unwrap(),
        vec![220, 100]
    );
    assert_eq!(
        parse_thermal_targets_preserve_order(Some("140,220,60")).unwrap(),
        vec![140, 220, 60]
    );
}

#[test]
fn resolve_optimization_targets_prefers_sparse_range_covering_subset() {
    assert_eq!(
        resolve_optimization_targets(&[60, 100, 140, 180, 220, 250], None).unwrap(),
        vec![60, 140, 250]
    );
    assert_eq!(
        resolve_optimization_targets(&[140, 250], None).unwrap(),
        vec![140, 250]
    );
}

#[test]
fn parse_thermal_targets_rejects_unsupported_values() {
    let error = parse_thermal_targets(Some("60,300")).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn thermal_heater_parameters_interpolate_between_profile_anchors() {
    let mut profile = thermal_seed_candidate_profile();
    let lower = thermal_candidate_point(&profile, 60).expect("60C anchor");
    let upper = thermal_candidate_point(&profile, 100).expect("100C anchor");
    thermal_candidate_point_mut(&mut profile, 60)
        .expect("60C anchor")
        .brake_distance_centi_c = 800;
    thermal_candidate_point_mut(&mut profile, 100)
        .expect("100C anchor")
        .brake_distance_centi_c = 1_100;
    thermal_candidate_point_mut(&mut profile, 60)
        .expect("60C anchor")
        .approach_tail_window_centi_c = 120;
    thermal_candidate_point_mut(&mut profile, 100)
        .expect("100C anchor")
        .approach_tail_window_centi_c = 280;
    let point = thermal_interpolated_candidate_point(&profile, 80).expect("80C point");
    assert_eq!(point.brake_distance_centi_c, 1_140);
    assert_eq!(point.approach_tail_window_centi_c, 200);
    assert_eq!(
        point.approach_power_permille,
        (lower.approach_power_permille + upper.approach_power_permille).div_ceil(2)
    );

    let value = thermal_candidate_profile_to_value(&profile);
    let parameters = thermal_heater_parameters_value(80, Some(&value), "preview");
    assert_eq!(parameters["targetTempC"], 80);
    assert_eq!(parameters["brakeDistanceCentiC"], 1_140);
    assert_eq!(parameters["approachTailWindowCentiC"], 200);
    assert_eq!(
        parameters["approachPowerPermille"],
        point.approach_power_permille
    );
}

#[test]
fn thermal_heater_parameters_apply_firmware_zero_value_inheritance() {
    let value = json!({
        "settings": {
            "holdKiPermillePerCTick": 1
        },
        "points": [{
            "targetTempC": 60,
            "holdKiPermillePerCTick": 0
        }]
    });
    let parameters = thermal_heater_parameters_value(60, Some(&value), "preview");
    assert_eq!(parameters["holdKiPermillePerCTick"], 1);
}

#[test]
fn thermal_profile_preview_unwraps_runtime_wrapper() {
    let profile = default_thermal_candidate_profile();
    let imported = json!({
        "thermalControlProfile": {
            "op": "preview",
            "profile": profile.clone(),
        }
    });

    assert_eq!(thermal_profile_package_from_value(imported), profile);
}

#[test]
fn thermal_control_readback_rejects_any_effective_parameter_mismatch() {
    let profile = default_thermal_candidate_profile();
    let expected = thermal_heater_parameters_value(60, Some(&profile), "preview");
    let mut actual = expected.as_object().unwrap().clone();
    let settings = actual.remove("settings").unwrap();
    actual.remove("mode");
    actual.insert("profileActive".to_string(), Value::Bool(true));
    actual.insert("profileCoversTarget".to_string(), Value::Bool(true));
    actual.insert(
        "profileSource".to_string(),
        Value::String("preview".to_string()),
    );
    for field in [
        "tempFilterAlphaPermille",
        "approachMaxTicks",
        "approachMinPowerRatioPermille",
        "autoAdjustableWorkingFloorMv",
        "heaterCurrentReserveMa",
    ] {
        actual.insert(field.to_string(), settings[field].clone());
    }
    let status = json!({
        "thermalControlProfilePreview": true,
        "thermalControl": Value::Object(actual.clone()),
    });
    assert!(verify_thermal_control_readback(&status, &expected, "preview").is_ok());

    let mut rounded = actual.clone();
    rounded.insert(
        "brakeDistanceCentiC".to_string(),
        json!(expected["brakeDistanceCentiC"].as_u64().unwrap() + 1),
    );
    let rounded_status = json!({
        "thermalControlProfilePreview": true,
        "thermalControl": Value::Object(rounded),
    });
    assert!(verify_thermal_control_readback(&rounded_status, &expected, "preview").is_ok());

    actual.insert("warmupPowerPermille".to_string(), json!(999));
    let mismatch = json!({
        "thermalControlProfilePreview": true,
        "thermalControl": Value::Object(actual),
    });
    assert!(
        verify_thermal_control_readback(&mismatch, &expected, "preview")
            .unwrap_err()
            .to_string()
            .contains("warmupPowerPermille")
    );
}

#[test]
fn thermal_candidate_profile_parses_existing_profile_value() {
    let profile = default_thermal_candidate_profile();
    let parsed = thermal_candidate_profile_from_value(profile.clone());
    assert_eq!(thermal_candidate_profile_to_value(&parsed), profile);
}

#[test]
fn thermal_candidate_profile_forces_full_power_warmup() {
    let mut profile = default_thermal_candidate_profile();
    profile["points"][0]["warmupPowerPermille"] = json!(760);

    let parsed = thermal_candidate_profile_from_value(profile);
    let normalized = thermal_candidate_profile_to_value(&parsed);
    assert_eq!(normalized["points"][0]["warmupPowerPermille"], 1_000);

    let point = thermal_candidate_point(&parsed, 60).expect("60C point");
    let effective = thermal_effective_candidate_point(point);
    assert_eq!(effective.warmup_power_permille, 1_000);

    let params = thermal_heater_parameters_value(60, Some(&normalized), "preview");
    assert_eq!(params["warmupPowerPermille"], 1_000);
}

#[test]
fn thermal_candidate_profile_keeps_full_power_warmup_after_interpolation() {
    let mut profile = default_thermal_candidate_profile();
    profile["points"][0]["targetTempC"] = json!(60);
    profile["points"][0]["warmupPowerPermille"] = json!(400);
    profile["points"][0]["approachPowerPermille"] = json!(440);
    profile["points"][1]["targetTempC"] = json!(100);
    profile["points"][1]["warmupPowerPermille"] = json!(1000);
    profile["points"][1]["approachPowerPermille"] = json!(445);

    let parsed = thermal_candidate_profile_from_value(profile);
    assert_eq!(parsed.points[0].warmup_power_permille, 1_000);

    let interpolated =
        thermal_interpolated_candidate_point(&parsed, 80).expect("80C interpolated point");
    assert_eq!(interpolated.warmup_power_permille, 1_000);

    let params = thermal_heater_parameters_value(
        80,
        Some(&thermal_candidate_profile_to_value(&parsed)),
        "preview",
    );
    assert_eq!(params["warmupPowerPermille"], 1_000);
}

#[test]
fn thermal_candidate_profile_preserves_explicit_calibration_target() {
    let mut profile = default_thermal_candidate_profile();
    profile["points"][5]["targetTempC"] = json!(80);
    profile["points"][5]["brakeDistanceCentiC"] = json!(450);

    let normalized =
        thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(profile));
    assert_eq!(normalized["points"][5]["targetTempC"], 80);
    assert_eq!(normalized["points"][5]["brakeDistanceCentiC"], 450);
}

#[test]
fn thermal_candidate_profile_normalizes_missing_current_reserve() {
    let mut profile = default_thermal_candidate_profile();
    profile["settings"]
        .as_object_mut()
        .unwrap()
        .remove("heaterCurrentReserveMa");

    let normalized =
        thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(profile));
    assert_eq!(normalized["settings"]["heaterCurrentReserveMa"], 200);
}

#[test]
fn thermal_validation_rejects_incomplete_stages() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 120,
        rise_time_ms: 119_000,
        max_overshoot_c: 1.0,
        hold_peak_to_peak_c: 1.0,
        sample_count: 12,
        stop_reason: "timeout",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[120],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );
    assert_eq!(validation["passed"], false);
    assert_eq!(validation["failures"][0]["reason"], "incomplete_stage");
}

#[test]
fn thermal_validation_skips_stage_limits_for_environment_faults() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 140,
        rise_time_ms: 12_000,
        max_overshoot_c: 88.0,
        hold_peak_to_peak_c: 90.0,
        sample_count: 8,
        stop_reason: "temperature_sample_glitch",
        terminal_runtime_drop_reason: Some("temperature_sample_glitch"),
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[140],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );

    assert_eq!(validation["passed"], false);
    let failures = validation["failures"].as_array().expect("failures array");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["reason"], "incomplete_stage");
    assert_eq!(failures[0]["stopReason"], "temperature_sample_glitch");
}

#[test]
fn thermal_validation_rejects_a_partial_target_ladder() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 60,
        rise_time_ms: 10_000,
        max_overshoot_c: 1.0,
        hold_peak_to_peak_c: 1.0,
        sample_count: 40,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(1_000),
            stable_window_started_at_ms: Some(6_000),
            stable_window_verified_at_ms: Some(16_000),
            settle_time_ms: Some(5_000),
            failure_reason: None,
        },
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[60, 140],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );

    assert_eq!(validation["passed"], false);
    assert_eq!(validation["failures"][0]["reason"], "missing_stage");
    assert_eq!(validation["failures"][0]["targetTempC"], 140);
}

#[test]
fn thermal_tuning_scout_validation_reports_failed_stage_limits() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 140,
        rise_time_ms: 11_500,
        max_overshoot_c: 3.8,
        hold_peak_to_peak_c: 3.6,
        sample_count: 40,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(1_000),
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
            settle_time_ms: None,
            failure_reason: Some("full_speed_to_stable_timeout"),
        },
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[140],
        ThermalSelfTestEvaluationMode::TuningScout,
    );

    assert_eq!(validation["passed"], false);
    let failures = validation["failures"].as_array().expect("failures array");
    assert!(
        failures
            .iter()
            .any(|failure| failure["reason"] == "overshoot")
    );
    assert!(
        failures
            .iter()
            .any(|failure| failure["reason"] == "hold_p2p")
    );
    assert!(
        failures
            .iter()
            .any(|failure| failure["reason"] == "full_speed_to_stable_missing")
    );
}

#[test]
fn thermal_hold_confirm_validation_rejects_slow_full_speed_settle() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 60,
        rise_time_ms: 18_000,
        max_overshoot_c: 1.2,
        hold_peak_to_peak_c: 1.4,
        sample_count: 90,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(1_000),
            stable_window_started_at_ms: Some(19_000),
            stable_window_verified_at_ms: Some(29_000),
            settle_time_ms: Some(18_000),
            failure_reason: Some("full_speed_to_stable_timeout"),
        },
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[60],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );

    assert_eq!(validation["passed"], false);
    assert_eq!(validation["failures"][0]["reason"], "full_speed_to_stable");
    assert_eq!(validation["failures"][0]["limit"], 10_000);
}

#[test]
fn thermal_hold_confirm_validation_uses_five_seconds_above_150c() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 180,
        rise_time_ms: 7_000,
        max_overshoot_c: 1.2,
        hold_peak_to_peak_c: 1.4,
        sample_count: 90,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(1_000),
            stable_window_started_at_ms: Some(7_000),
            stable_window_verified_at_ms: Some(17_000),
            settle_time_ms: Some(6_000),
            failure_reason: Some("full_speed_to_stable_timeout"),
        },
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[180],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );

    assert_eq!(validation["passed"], false);
    assert_eq!(validation["failures"][0]["reason"], "full_speed_to_stable");
    assert_eq!(validation["failures"][0]["limit"], 5_000);
}

#[test]
fn thermal_hold_confirm_validation_allows_ten_seconds_at_or_below_150c() {
    let applied = vec![ThermalStageResult {
        target_temp_c: 150,
        rise_time_ms: 10_000,
        max_overshoot_c: 1.2,
        hold_peak_to_peak_c: 1.4,
        sample_count: 90,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: Some(1_000),
            stable_window_started_at_ms: Some(11_000),
            stable_window_verified_at_ms: Some(21_000),
            settle_time_ms: Some(10_000),
            failure_reason: None,
        },
    }];

    let validation = validate_thermal_applied_results(
        &applied,
        &[150],
        ThermalSelfTestEvaluationMode::HoldConfirm,
    );

    assert_eq!(validation["passed"], true);
    assert_eq!(
        validation["failures"].as_array().map(|items| items.len()),
        Some(0)
    );
}

#[test]
fn thermal_tuning_continues_only_after_controllable_heat_failures() {
    let mut stage = ThermalStageResult {
        target_temp_c: 140,
        rise_time_ms: 10_500,
        max_overshoot_c: 0.0,
        hold_peak_to_peak_c: f64::INFINITY,
        sample_count: 42,
        stop_reason: "full_speed_to_stable_timeout",
        terminal_runtime_drop_reason: None,
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
    };

    assert!(thermal_stage_can_continue_tuning(&stage));
    stage.stop_reason = "warmup_timeout";
    assert!(thermal_stage_can_continue_tuning(&stage));
    stage.stop_reason = "heater_disarmed";
    assert!(!thermal_stage_can_continue_tuning(&stage));
}

#[test]
fn thermal_infrastructure_failure_does_not_change_candidate() {
    let previous = thermal_default_target_point(60);
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 3_708,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 15,
            stop_reason: "sample_rate_below_3hz",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned, previous);
}

#[test]
fn thermal_pre_hold_timeout_does_not_raise_hold_power() {
    let previous = thermal_default_target_point(60);
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 17_680,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 71,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(340),
                approach_sample_count: 30,
                hold_sample_count: 0,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
    assert_eq!(
        tuned.hold_reheat_power_permille,
        previous.hold_reheat_power_permille
    );
    assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
    assert!(tuned.brake_distance_centi_c < previous.brake_distance_centi_c);
}

#[test]
fn thermal_pre_hold_threshold_crossing_advances_lead_without_raising_power() {
    let mut previous = thermal_default_target_point(60);
    previous.brake_distance_centi_c = 1_310;
    previous.approach_power_permille = 590;
    previous.approach_floor_power_permille = 510;
    previous.approach_lead_ticks = 0;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 26_132,
            max_overshoot_c: 2.8,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 249,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(530),
                approach_median_slope_c_per_s: Some(7.5),
                approach_sample_count: 61,
                hold_sample_count: 0,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                approach_started_at_ms: Some(15_871),
                hold_threshold_crossed_at_ms: Some(23_736),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(15_871),
                failure_reason: Some("full_speed_to_stable_timeout"),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.approach_lead_ticks, 2);
    assert_eq!(
        tuned.approach_power_permille,
        previous.approach_power_permille
    );
    assert_eq!(
        tuned.approach_floor_power_permille,
        previous.approach_floor_power_permille
    );
    assert_eq!(
        tuned.brake_distance_centi_c,
        previous.brake_distance_centi_c
    );
    assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
}

#[test]
fn thermal_stability_overshoot_fine_tunes_cutoff_without_advancing_low_temp_lead() {
    let mut previous = thermal_default_target_point(60);
    previous.brake_distance_centi_c = 1_110;
    previous.approach_damping_exponent_permille = 940;
    previous.approach_power_permille = 520;
    previous.approach_floor_power_permille = 400;
    previous.approach_lead_ticks = 6;
    previous.hold_power_permille = 170;
    previous.hold_reheat_power_permille = 440;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 21_115,
            max_overshoot_c: 3.1,
            hold_peak_to_peak_c: 3.0,
            sample_count: 97,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(-0.4),
                hold_median_output_permille: Some(60),
                hold_p90_output_permille: Some(270),
                hold_mean_error_c: Some(-1.25),
                hold_max_above_target_c: Some(3.1),
                hold_max_below_target_c: Some(0.6),
                hold_sample_count: 13,
                residual_heat_after_hold_entry_c: Some(2.0),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(18_000),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(10_000),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(
        tuned.brake_distance_centi_c,
        previous.brake_distance_centi_c + 80
    );
    assert!(tuned.approach_damping_exponent_permille > previous.approach_damping_exponent_permille);
    assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
    assert!(tuned.overshoot_cutoff_centi_c < previous.overshoot_cutoff_centi_c);
    assert_eq!(tuned.approach_floor_power_permille, 400);
    assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
    assert_eq!(
        tuned.hold_reheat_power_permille,
        previous.hold_reheat_power_permille
    );
}

#[test]
fn thermal_low_temp_bursty_hold_ripple_does_not_collapse_hold_power_to_zero() {
    let mut previous = thermal_default_target_point(60);
    previous.approach_power_permille = 450;
    previous.approach_floor_power_permille = 160;
    previous.approach_lead_ticks = 6;
    previous.brake_distance_centi_c = 1_100;
    previous.hold_entry_centi_c = 220;
    previous.hold_exit_centi_c = 400;
    previous.hold_power_permille = 40;
    previous.hold_reheat_power_permille = 200;
    previous.hold_kp_permille_per_c = 12;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 16_991,
            max_overshoot_c: 0.8,
            hold_peak_to_peak_c: 3.2,
            sample_count: 770,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(3.06),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(130),
                hold_mean_error_c: Some(0.69),
                hold_max_above_target_c: Some(0.8),
                hold_max_below_target_c: Some(3.42),
                hold_sample_count: 640,
                residual_heat_after_hold_entry_c: Some(3.86),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, 70);
    assert_eq!(tuned.hold_reheat_power_permille, 200);
    assert_eq!(tuned.hold_kp_permille_per_c, 15);
}

#[test]
fn thermal_low_temp_bursty_hold_ripple_stays_near_current_seed_on_mild_under_target() {
    let mut previous = thermal_default_target_point(60);
    previous.approach_power_permille = 450;
    previous.approach_floor_power_permille = 160;
    previous.approach_lead_ticks = 6;
    previous.brake_distance_centi_c = 1_100;
    previous.hold_entry_centi_c = 220;
    previous.hold_exit_centi_c = 400;
    previous.hold_power_permille = 40;
    previous.hold_reheat_power_permille = 200;
    previous.hold_kp_permille_per_c = 12;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 20_106,
            max_overshoot_c: 1.05,
            hold_peak_to_peak_c: 3.61,
            sample_count: 802,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(130),
                first_hold_error_c: Some(1.68),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(150),
                hold_mean_error_c: Some(0.895),
                hold_max_above_target_c: Some(1.05),
                hold_max_below_target_c: Some(2.78),
                hold_sample_count: 631,
                residual_heat_after_hold_entry_c: Some(2.73),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, 70);
    assert_eq!(tuned.hold_reheat_power_permille, 200);
    assert_eq!(tuned.hold_kp_permille_per_c, 15);
    assert_eq!(tuned.approach_floor_power_permille, 160);
}

#[test]
fn thermal_late_low_temp_hold_entry_moves_hold_gate_earlier() {
    let mut previous = thermal_default_target_point(60);
    previous.approach_lead_ticks = 3;
    previous.overshoot_cutoff_centi_c = 110;
    previous.hold_entry_centi_c = 20;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 25_693,
            max_overshoot_c: 1.9,
            hold_peak_to_peak_c: 1.5,
            sample_count: 260,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(-0.4),
                hold_max_above_target_c: Some(1.9),
                hold_max_below_target_c: Some(0.0),
                hold_sample_count: 17,
                residual_heat_after_hold_entry_c: Some(1.5),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(25_693),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(16_047),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
    assert_eq!(
        tuned.overshoot_cutoff_centi_c,
        previous.overshoot_cutoff_centi_c
    );
    assert_eq!(tuned.hold_entry_centi_c, 60);
}

#[test]
fn thermal_low_temp_hold_entry_carry_adds_hold_lead_without_rebraking() {
    let mut previous = thermal_default_target_point(60);
    previous.approach_lead_ticks = 3;
    previous.hold_lead_ticks = 0;
    previous.hold_power_permille = 60;
    previous.hold_reheat_power_permille = 90;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 21_540,
            max_overshoot_c: 1.8,
            hold_peak_to_peak_c: 2.9,
            sample_count: 204,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(0.6),
                hold_max_above_target_c: Some(1.8),
                hold_max_below_target_c: Some(1.1),
                hold_p90_output_permille: Some(80),
                hold_sample_count: 28,
                residual_heat_after_hold_entry_c: Some(2.4),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(21_201),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(12_782),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
    assert_eq!(
        tuned.overshoot_cutoff_centi_c,
        previous.overshoot_cutoff_centi_c
    );
    assert_eq!(tuned.hold_lead_ticks, 2);
    assert_eq!(tuned.hold_reheat_power_permille, 60);
}

#[test]
fn thermal_low_temp_bounded_entry_residual_prefers_overshoot_cutoff_trim() {
    let mut previous = thermal_default_target_point(100);
    previous.brake_distance_centi_c = 1_000;
    previous.approach_power_permille = 420;
    previous.approach_floor_power_permille = 300;
    previous.approach_damping_exponent_permille = 1_220;
    previous.approach_lead_ticks = 7;
    previous.hold_blend_ticks = 2;
    previous.hold_entry_centi_c = 150;
    previous.hold_exit_centi_c = 120;
    previous.hold_kp_permille_per_c = 20;
    previous.hold_lead_ticks = 8;
    previous.hold_power_permille = 220;
    previous.hold_reheat_power_permille = 220;
    previous.hold_off_centi_c = 180;
    previous.overshoot_cutoff_centi_c = 90;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 22_294,
            max_overshoot_c: 1.55,
            hold_peak_to_peak_c: 1.73,
            sample_count: 83,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(0.88),
                hold_max_above_target_c: Some(1.55),
                hold_max_below_target_c: Some(0.88),
                hold_p90_output_permille: Some(210),
                hold_sample_count: 11,
                residual_heat_after_hold_entry_c: Some(2.43),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(21_652),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(14_498),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.hold_lead_ticks, previous.hold_lead_ticks);
    assert_eq!(tuned.hold_power_permille, previous.hold_power_permille);
    assert_eq!(
        tuned.hold_reheat_power_permille,
        previous.hold_reheat_power_permille
    );
    assert_eq!(tuned.brake_distance_centi_c, 1080);
    assert_eq!(tuned.overshoot_cutoff_centi_c, 70);
    assert_eq!(tuned.hold_off_centi_c, 50);
    assert_eq!(tuned.hold_blend_ticks, 1);
    assert_eq!(tuned.hold_kp_permille_per_c, 16);
}

#[test]
fn thermal_low_temp_moderate_residual_keeps_hold_exit_and_adds_brake() {
    let mut previous = thermal_default_target_point(60);
    previous.brake_distance_centi_c = 1_700;
    previous.approach_power_permille = 450;
    previous.approach_floor_power_permille = 220;
    previous.approach_damping_exponent_permille = 4_000;
    previous.approach_lead_ticks = 6;
    previous.hold_blend_ticks = 1;
    previous.hold_entry_centi_c = 180;
    previous.hold_exit_centi_c = 400;
    previous.hold_kp_permille_per_c = 8;
    previous.hold_power_permille = 135;
    previous.hold_reheat_power_permille = 140;
    previous.hold_off_centi_c = 50;
    previous.overshoot_cutoff_centi_c = 50;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 16_197,
            max_overshoot_c: 1.68,
            hold_peak_to_peak_c: 2.15,
            sample_count: 215,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(1.75),
                hold_mean_error_c: Some(-0.041),
                hold_max_above_target_c: Some(1.68),
                hold_max_below_target_c: Some(1.86),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(0),
                hold_sample_count: 74,
                residual_heat_after_hold_entry_c: Some(3.43),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(14_203),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(5_997),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.brake_distance_centi_c, 1_780);
    assert_eq!(tuned.hold_exit_centi_c, previous.hold_exit_centi_c);
    assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
    assert_eq!(
        tuned.approach_damping_exponent_permille,
        previous.approach_damping_exponent_permille
    );
    assert_eq!(
        tuned.overshoot_cutoff_centi_c,
        previous.overshoot_cutoff_centi_c
    );
}

#[test]
fn thermal_low_temp_hold_entry_carry_trims_hold_power_once_hold_lead_is_maxed() {
    let mut previous = thermal_default_target_point(100);
    previous.brake_distance_centi_c = 1_000;
    previous.approach_power_permille = 420;
    previous.approach_floor_power_permille = 300;
    previous.approach_damping_exponent_permille = 1_220;
    previous.approach_lead_ticks = 7;
    previous.hold_blend_ticks = 2;
    previous.hold_entry_centi_c = 150;
    previous.hold_exit_centi_c = 120;
    previous.hold_kp_permille_per_c = 20;
    previous.hold_lead_ticks = 8;
    previous.hold_power_permille = 180;
    previous.hold_reheat_power_permille = 240;
    previous.hold_off_centi_c = 180;
    previous.overshoot_cutoff_centi_c = 90;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 22_294,
            max_overshoot_c: 2.1,
            hold_peak_to_peak_c: 2.45,
            sample_count: 83,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(0.88),
                hold_max_above_target_c: Some(2.1),
                hold_max_below_target_c: Some(0.88),
                hold_p90_output_permille: Some(260),
                hold_sample_count: 11,
                residual_heat_after_hold_entry_c: Some(2.43),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                first_hold_at_ms: Some(21_652),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                warmup_exited_at_ms: Some(14_498),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.hold_lead_ticks, 8);
    assert_eq!(tuned.hold_power_permille, 160);
    assert_eq!(tuned.hold_reheat_power_permille, 210);
    assert_eq!(tuned.hold_off_centi_c, 200);
    assert_eq!(tuned.approach_lead_ticks, previous.approach_lead_ticks);
}

#[test]
fn thermal_severe_residual_heat_brakes_earlier_and_lowers_approach_floor() {
    let mut previous = thermal_default_target_point(60);
    previous.brake_distance_centi_c = 1_210;
    previous.approach_damping_exponent_permille = 1_040;
    previous.approach_power_permille = 520;
    previous.approach_floor_power_permille = 400;
    previous.approach_lead_ticks = 7;
    previous.hold_power_permille = 90;
    previous.hold_reheat_power_permille = 340;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 18_511,
            max_overshoot_c: 7.7,
            hold_peak_to_peak_c: 6.8,
            sample_count: 92,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(-0.9),
                hold_median_output_permille: Some(0),
                hold_mean_error_c: Some(-4.61),
                hold_max_above_target_c: Some(7.7),
                hold_max_below_target_c: Some(0.0),
                hold_sample_count: 19,
                residual_heat_after_hold_entry_c: Some(6.8),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.brake_distance_centi_c, 1_560);
    assert_eq!(tuned.approach_damping_exponent_permille, 1_240);
    assert_eq!(tuned.approach_lead_ticks, 9);
    assert_eq!(tuned.approach_floor_power_permille, 400);
    assert_eq!(tuned.hold_reheat_power_permille, 340);
    assert_eq!(tuned.hold_power_permille, 90);
}

#[test]
fn thermal_fast_low_temp_residual_reduces_warmup_power() {
    let mut previous = thermal_default_target_point(60);
    previous.warmup_power_permille = 1_000;
    previous.approach_power_permille = 500;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 12_000,
            max_overshoot_c: 9.0,
            hold_peak_to_peak_c: 9.0,
            sample_count: 120,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_slope_c_per_s: Some(3.0),
                hold_sample_count: 120,
                hold_max_above_target_c: Some(9.0),
                residual_heat_after_hold_entry_c: Some(6.0),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.warmup_power_permille, 750);
}

#[test]
fn thermal_bounded_residual_heat_advances_coast_gate_without_moving_warmup_exit() {
    let mut previous = thermal_default_target_point(60);
    previous.brake_distance_centi_c = 1_910;
    previous.hold_exit_centi_c = 200;
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 19_708,
            max_overshoot_c: 3.3,
            hold_peak_to_peak_c: 2.4,
            sample_count: 85,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                hold_max_above_target_c: Some(3.3),
                hold_max_below_target_c: Some(0.0),
                hold_sample_count: 7,
                residual_heat_after_hold_entry_c: Some(2.4),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                failure_reason: Some("full_speed_to_stable_timeout"),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.brake_distance_centi_c, 1_910);
    assert_eq!(tuned.hold_exit_centi_c, 380);
}

#[test]
fn thermal_high_temp_entry_residual_advances_braking_instead_of_raising_hold_power() {
    let mut previous = thermal_default_target_point(220);
    previous.brake_distance_centi_c = 520;
    previous.approach_power_permille = 760;
    previous.approach_floor_power_permille = 720;
    previous.approach_damping_exponent_permille = 550;
    previous.approach_lead_ticks = 2;
    previous.hold_power_permille = 700;
    previous.hold_reheat_power_permille = 780;
    previous.hold_kp_permille_per_c = 19;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 93_722,
            max_overshoot_c: 3.0,
            hold_peak_to_peak_c: 5.4,
            sample_count: 1_476,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(220.6),
                first_hold_error_c: Some(-0.6),
                residual_heat_after_hold_entry_c: Some(2.4),
                approach_median_output_permille: Some(780),
                approach_median_slope_c_per_s: Some(6.14),
                hold_median_output_permille: Some(750),
                hold_p90_output_permille: Some(790),
                hold_mean_error_c: Some(0.44),
                hold_max_above_target_c: Some(3.0),
                hold_max_below_target_c: Some(2.4),
                approach_sample_count: 65,
                hold_sample_count: 541,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
    assert!(tuned.approach_damping_exponent_permille > previous.approach_damping_exponent_permille);
    assert!(tuned.hold_power_permille <= 720);
    assert!(tuned.hold_reheat_power_permille <= previous.hold_reheat_power_permille);
}

#[test]
fn thermal_replay_accepts_missing_hold_metric_for_incomplete_stage() {
    let stage = thermal_stage_result_from_value(&json!({
        "targetTempC": 60,
        "riseTimeMs": 17_680,
        "maxOvershootC": 0.0,
        "holdPeakToPeakC": null,
        "sampleCount": 71,
        "stopReason": "full_speed_to_stable_timeout",
    }))
    .unwrap();

    assert!(stage.hold_peak_to_peak_c.is_infinite());
}

#[test]
fn thermal_replay_accepts_warmup_timeout_stop_reason() {
    let stage = thermal_stage_result_from_value(&json!({
        "targetTempC": 220,
        "riseTimeMs": 45_000,
        "maxOvershootC": 0.0,
        "holdPeakToPeakC": null,
        "sampleCount": 90,
        "stopReason": "warmup_timeout",
    }))
    .unwrap();

    assert_eq!(stage.stop_reason, "warmup_timeout");
    assert!(stage.hold_peak_to_peak_c.is_infinite());
}

#[test]
fn thermal_sample_rate_accepts_measured_four_hz_sequence_with_one_jitter_gap() {
    let mut tracker = ThermalSampleRateTracker::new();
    let mut observation = tracker.observe(73);
    for elapsed_ms in [
        322, 557, 800, 1_067, 1_305, 1_571, 1_816, 2_074, 2_321, 2_575, 2_824, 3_068, 3_322, 3_561,
        3_925,
    ] {
        observation = tracker.observe(elapsed_ms);
        assert!(!observation.violation);
    }
    assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
}

#[test]
fn thermal_sample_rate_rejects_sustained_sub_three_hz_sampling() {
    let mut tracker = ThermalSampleRateTracker::new();
    let mut observation = tracker.observe(0);
    for elapsed_ms in [
        350, 700, 1_050, 1_400, 1_750, 2_100, 2_450, 2_800, 3_150, 3_500, 3_850, 4_200, 4_550,
        4_900, 5_250, 5_600, 5_950, 6_300,
    ] {
        observation = tracker.observe(elapsed_ms);
    }
    assert!(observation.violation);
    assert!(observation.rolling_rate_hz.unwrap() < THERMAL_MIN_SAMPLE_RATE_HZ);
}

#[test]
fn thermal_sample_rate_tolerates_recorded_single_serial_stall() {
    let mut tracker = ThermalSampleRateTracker::new();
    let mut observation = tracker.observe(226);
    for elapsed_ms in [
        315, 412, 503, 596, 704, 816, 937, 1_070, 1_200, 1_313, 1_394, 1_525, 1_593, 1_667, 1_735,
        1_845, 1_946, 2_063, 2_325, 2_446, 2_643, 2_810, 2_912, 3_279, 3_528, 3_694, 3_925, 4_133,
        4_486, 4_738, 5_102, 6_258,
    ] {
        observation = tracker.observe(elapsed_ms);
    }
    assert!(!observation.violation);
    assert!(observation.rolling_rate_hz.unwrap() < THERMAL_MIN_SAMPLE_RATE_HZ);
}

#[test]
fn thermal_sample_rate_tolerates_one_transient_low_rate_window() {
    let mut tracker = ThermalSampleRateTracker::new();
    let mut observation = tracker.observe(0);
    for elapsed_ms in [
        250, 500, 750, 1_000, 1_250, 1_500, 2_300, 2_400, 2_500, 2_600, 2_700, 2_800, 2_900, 3_000,
    ] {
        observation = tracker.observe(elapsed_ms);
    }
    assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
    assert!(!observation.violation);
}

#[test]
fn thermal_sample_rate_tolerates_one_isolated_sampling_stall() {
    let mut tracker = ThermalSampleRateTracker::new();
    let mut observation = tracker.observe(0);
    for elapsed_ms in [
        250, 1_251, 1_350, 1_450, 1_550, 1_650, 1_750, 1_850, 1_950, 2_050, 2_150, 2_250, 2_350,
        2_450, 2_550, 2_650, 2_750, 2_850, 2_950, 3_050,
    ] {
        observation = tracker.observe(elapsed_ms);
    }
    assert!(!observation.violation);
    assert!(observation.rolling_rate_hz.unwrap() >= THERMAL_MIN_SAMPLE_RATE_HZ);
}

#[test]
fn thermal_measurement_guard_rejects_sustained_guarded_samples() {
    let mut tracker = ThermalMeasurementGuardTracker::default();

    assert!(!tracker.observe(false, 0));
    assert!(!tracker.observe(true, 1_000));
    assert!(!tracker.observe(true, 2_999));
    assert!(tracker.observe(true, 3_000));
    assert!(!tracker.observe(false, 3_100));
    assert!(!tracker.observe(true, 4_000));
}

#[test]
fn thermal_environment_faults_are_retryable_without_becoming_applied_results() {
    let guarded = ThermalStageResult {
        target_temp_c: 80,
        rise_time_ms: 31_000,
        max_overshoot_c: 2.0,
        hold_peak_to_peak_c: 2.0,
        sample_count: 100,
        stop_reason: "temperature_sample_glitch",
        terminal_runtime_drop_reason: Some("temperature_sample_glitch"),
        analysis: ThermalStageAnalysis::default(),
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
    };
    let thermal_failure = ThermalStageResult {
        stop_reason: "timeout",
        ..guarded.clone()
    };

    assert!(thermal_stage_should_retry_after_environment_fault(&guarded));
    assert!(!thermal_stage_should_retry_after_environment_fault(
        &thermal_failure
    ));
}

#[test]
fn thermal_timeout_tuning_raises_power_and_reduces_brake() {
    let previous = thermal_default_target_point(250);
    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 250,
            rise_time_ms: 180_000,
            max_overshoot_c: 3.5,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 120,
            stop_reason: "timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis::default(),
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.approach_power_permille >= previous.approach_power_permille);
    assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
    assert!(tuned.hold_power_permille >= previous.hold_power_permille);
    assert!(tuned.brake_distance_centi_c <= previous.brake_distance_centi_c);
}

#[test]
fn thermal_high_temp_power_limit_converges_to_saturated_near_target_profile() {
    let mut previous = thermal_default_target_point(220);
    previous.brake_distance_centi_c = 442;
    previous.warmup_power_permille = 980;
    previous.approach_power_permille = 940;
    previous.approach_floor_power_permille = 760;
    previous.approach_damping_exponent_permille = 250;
    previous.approach_lead_ticks = 2;
    previous.hold_power_permille = 750;
    previous.hold_reheat_power_permille = 850;
    previous.hold_entry_centi_c = 28;
    previous.hold_exit_centi_c = 70;
    previous.hold_off_centi_c = 170;
    previous.overshoot_cutoff_centi_c = 275;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 154_721,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 465,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(930),
                approach_median_slope_c_per_s: Some(0.75),
                approach_sample_count: 55,
                hold_sample_count: 0,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                approach_started_at_ms: Some(144_299),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(144_299),
                failure_reason: Some("full_speed_to_stable_timeout"),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.brake_distance_centi_c, 160);
    assert_eq!(tuned.hold_entry_centi_c, 150);
    assert_eq!(tuned.hold_exit_centi_c, 160);
    assert_eq!(tuned.warmup_power_permille, 1_000);
    assert_eq!(tuned.approach_power_permille, 1_000);
    assert_eq!(tuned.approach_floor_power_permille, 1_000);
    assert_eq!(tuned.approach_lead_ticks, 0);
    assert_eq!(tuned.hold_power_permille, 980);
    assert_eq!(tuned.hold_reheat_power_permille, 1_000);
    assert_eq!(tuned.hold_off_centi_c, 80);
    assert_eq!(tuned.overshoot_cutoff_centi_c, 180);
}

#[test]
fn thermal_pre_hold_timeout_reduces_excessive_predictive_lead() {
    let mut previous = thermal_default_target_point(60);
    previous.approach_lead_ticks = 14;
    previous.brake_distance_centi_c = 1_640;
    previous.approach_floor_power_permille = 90;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 60,
            rise_time_ms: 15_683,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: f64::INFINITY,
            sample_count: 63,
            stop_reason: "full_speed_to_stable_timeout",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(230),
                approach_median_slope_c_per_s: Some(3.58),
                approach_sample_count: 13,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis {
                approach_started_at_ms: Some(5_385),
                ..ThermalApproachGuardAnalysis::default()
            },
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                warmup_exited_at_ms: Some(5_385),
                failure_reason: Some("full_speed_to_stable_timeout"),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.approach_lead_ticks, 7);
    assert_eq!(tuned.brake_distance_centi_c, 1_520);
    assert!(tuned.approach_floor_power_permille > previous.approach_floor_power_permille);
}

#[test]
fn thermal_overshoot_tuning_keeps_or_increases_lead_and_brake() {
    let mut previous = thermal_default_target_point(100);
    previous.approach_lead_ticks = 5;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 31_000,
            max_overshoot_c: 7.9,
            hold_peak_to_peak_c: 7.5,
            sample_count: 300,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(100.4),
                first_hold_error_c: Some(-0.4),
                residual_heat_after_hold_entry_c: Some(7.5),
                approach_median_output_permille: Some(0),
                approach_median_slope_c_per_s: Some(3.0),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(110),
                hold_mean_error_c: Some(-3.6),
                hold_max_above_target_c: Some(7.9),
                hold_max_below_target_c: Some(0.0),
                approach_sample_count: 20,
                hold_sample_count: 240,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(tuned.approach_lead_ticks >= previous.approach_lead_ticks);
    assert!(tuned.approach_floor_power_permille <= previous.approach_floor_power_permille);
}

#[test]
fn thermal_low_temp_overshoot_prefers_more_lead_over_collapsing_power() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: 1_180,
        warmup_power_permille: 260,
        approach_power_permille: 181,
        approach_floor_power_permille: 99,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 58,
        hold_reheat_power_permille: 116,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 35,
        hold_exit_centi_c: 93,
        hold_on_centi_c: 30,
        hold_off_centi_c: 105,
        overshoot_cutoff_centi_c: 151,
        hold_kp_permille_per_c: 13,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 9,
        approach_lead_ticks: 3,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 35_531,
            max_overshoot_c: 10.0,
            hold_peak_to_peak_c: 10.2,
            sample_count: 383,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(101.4),
                first_hold_error_c: Some(-1.4),
                residual_heat_after_hold_entry_c: Some(8.6),
                approach_median_output_permille: Some(140),
                approach_median_slope_c_per_s: Some(9.66),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(10),
                hold_mean_error_c: Some(-5.8),
                hold_max_above_target_c: Some(10.0),
                hold_max_below_target_c: Some(0.2),
                approach_sample_count: 19,
                hold_sample_count: 242,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
    assert!(tuned.approach_floor_power_permille >= 99);
    assert!(tuned.approach_power_permille >= tuned.approach_floor_power_permille);
    assert!(tuned.hold_power_permille <= previous.hold_power_permille);
    assert!(tuned.hold_reheat_power_permille >= tuned.approach_floor_power_permille);
}

#[test]
fn thermal_mid_temp_overshoot_increases_braking_and_softens_reheat() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 140,
        brake_distance_centi_c: 2_180,
        warmup_power_permille: 240,
        approach_power_permille: 160,
        approach_floor_power_permille: 40,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 0,
        hold_reheat_power_permille: 80,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 30,
        hold_exit_centi_c: 87,
        hold_on_centi_c: 30,
        hold_off_centi_c: 99,
        overshoot_cutoff_centi_c: 148,
        hold_kp_permille_per_c: 8,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 8,
        approach_lead_ticks: 18,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 140,
            rise_time_ms: 83_266,
            max_overshoot_c: 3.2,
            hold_peak_to_peak_c: 4.1,
            sample_count: 625,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(140.3),
                first_hold_error_c: Some(-0.3),
                residual_heat_after_hold_entry_c: Some(2.9),
                approach_median_output_permille: Some(40),
                approach_median_slope_c_per_s: Some(2.49),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(70),
                hold_mean_error_c: Some(-0.88),
                hold_max_above_target_c: Some(3.2),
                hold_max_below_target_c: Some(0.9),
                approach_sample_count: 138,
                hold_sample_count: 248,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(tuned.approach_power_permille <= previous.approach_power_permille);
    assert!(tuned.approach_floor_power_permille <= previous.approach_floor_power_permille);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
}

#[test]
fn thermal_mid_temp_hold_swing_does_not_relax_braking() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 140,
        brake_distance_centi_c: 2_471,
        warmup_power_permille: 360,
        approach_power_permille: 140,
        approach_floor_power_permille: 20,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 0,
        hold_reheat_power_permille: 58,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 35,
        hold_exit_centi_c: 107,
        hold_on_centi_c: 30,
        hold_off_centi_c: 99,
        overshoot_cutoff_centi_c: 150,
        hold_kp_permille_per_c: 8,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 12,
        approach_lead_ticks: 18,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 140,
            rise_time_ms: 92_108,
            max_overshoot_c: 2.6,
            hold_peak_to_peak_c: 3.5,
            sample_count: 642,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(140.3),
                first_hold_error_c: Some(-0.3),
                residual_heat_after_hold_entry_c: Some(2.3),
                approach_median_output_permille: Some(0),
                approach_median_slope_c_per_s: Some(3.38),
                hold_median_output_permille: Some(0),
                hold_p90_output_permille: Some(60),
                hold_mean_error_c: Some(-0.58),
                hold_max_above_target_c: Some(2.6),
                hold_max_below_target_c: Some(0.9),
                approach_sample_count: 144,
                hold_sample_count: 247,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c >= previous.brake_distance_centi_c);
    assert!(tuned.approach_lead_ticks <= previous.approach_lead_ticks);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
}

#[test]
fn thermal_tuning_clamps_extreme_power_targets_without_panicking() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: 1_500,
        warmup_power_permille: 900,
        approach_power_permille: 900,
        approach_floor_power_permille: 900,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 980,
        hold_reheat_power_permille: 980,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 25,
        hold_exit_centi_c: 75,
        hold_on_centi_c: 30,
        hold_off_centi_c: 200,
        overshoot_cutoff_centi_c: 350,
        hold_kp_permille_per_c: 18,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 10,
        approach_lead_ticks: 4,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 40_000,
            max_overshoot_c: 0.0,
            hold_peak_to_peak_c: 3.1,
            sample_count: 200,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(99.9),
                first_hold_error_c: Some(0.1),
                residual_heat_after_hold_entry_c: Some(0.1),
                approach_median_output_permille: Some(1_000),
                approach_median_slope_c_per_s: Some(0.2),
                hold_median_output_permille: Some(1_000),
                hold_p90_output_permille: Some(1_000),
                hold_mean_error_c: Some(2.5),
                hold_max_above_target_c: Some(0.0),
                hold_max_below_target_c: Some(4.0),
                approach_sample_count: 40,
                hold_sample_count: 240,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, 1_000);
    assert!(tuned.approach_floor_power_permille <= 1_000);
    assert!(tuned.approach_power_permille <= 1_000);
    assert!(tuned.warmup_power_permille <= 1_000);
}

#[test]
fn thermal_under_target_hold_swing_does_not_collapse_sustain_power() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: 1_286,
        warmup_power_permille: 390,
        approach_power_permille: 260,
        approach_floor_power_permille: 275,
        approach_damping_exponent_permille: 975,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 59,
        hold_reheat_power_permille: 390,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 25,
        hold_exit_centi_c: 75,
        hold_on_centi_c: 30,
        hold_off_centi_c: 198,
        overshoot_cutoff_centi_c: 353,
        hold_kp_permille_per_c: 18,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 10,
        approach_lead_ticks: 4,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 101_928,
            max_overshoot_c: 2.0,
            hold_peak_to_peak_c: 4.3,
            sample_count: 625,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(99.8),
                first_hold_error_c: Some(0.2),
                residual_heat_after_hold_entry_c: Some(2.2),
                approach_median_output_permille: Some(270),
                approach_median_slope_c_per_s: Some(1.976),
                hold_median_output_permille: Some(70),
                hold_p90_output_permille: Some(390),
                hold_mean_error_c: Some(0.056),
                hold_max_above_target_c: Some(2.0),
                hold_max_below_target_c: Some(2.3),
                approach_sample_count: 161,
                hold_sample_count: 218,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert!(tuned.brake_distance_centi_c >= previous.brake_distance_centi_c);
    assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
    assert!(tuned.approach_power_permille >= previous.approach_power_permille);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille.saturating_add(100));
    assert!(tuned.approach_lead_ticks >= previous.approach_lead_ticks);
    assert!(
        tuned.approach_damping_exponent_permille >= previous.approach_damping_exponent_permille
    );
}

#[test]
fn thermal_high_temp_hold_ripple_rebases_equilibrium_without_weakening_approach() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 220,
        brake_distance_centi_c: 442,
        warmup_power_permille: 980,
        approach_power_permille: 940,
        approach_floor_power_permille: 760,
        approach_damping_exponent_permille: 250,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 720,
        hold_reheat_power_permille: 880,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 8,
        hold_exit_centi_c: 45,
        hold_on_centi_c: 14,
        hold_off_centi_c: 205,
        overshoot_cutoff_centi_c: 320,
        hold_kp_permille_per_c: 34,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 4,
        approach_lead_ticks: 2,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 61_499,
            max_overshoot_c: 3.0,
            hold_peak_to_peak_c: 4.8,
            sample_count: 487,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                approach_median_output_permille: Some(890),
                approach_median_slope_c_per_s: Some(2.1978),
                approach_sample_count: 147,
                first_hold_temp_c: Some(221.0),
                first_hold_error_c: Some(-1.0),
                hold_max_above_target_c: Some(3.0),
                hold_max_below_target_c: Some(1.8),
                hold_mean_error_c: Some(0.2719),
                hold_median_output_permille: Some(760),
                hold_p90_output_permille: Some(890),
                hold_sample_count: 242,
                residual_heat_after_hold_entry_c: Some(2.0),
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(
        tuned.approach_power_permille,
        previous.approach_power_permille
    );
    assert!(tuned.approach_floor_power_permille >= tuned.hold_power_permille);
    assert!(tuned.brake_distance_centi_c > previous.brake_distance_centi_c);
    assert!(tuned.approach_lead_ticks > previous.approach_lead_ticks);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
    assert!(tuned.hold_blend_ticks < previous.hold_blend_ticks);
}

#[test]
fn thermal_saturated_high_temp_ripple_widens_taper_instead_of_cutting_power_deeply() {
    let mut previous = thermal_default_target_point(220);
    previous.approach_power_permille = 1_000;
    previous.approach_floor_power_permille = 1_000;
    previous.hold_power_permille = 990;
    previous.hold_reheat_power_permille = 1_000;
    previous.hold_on_centi_c = 75;
    previous.hold_off_centi_c = 50;
    previous.overshoot_cutoff_centi_c = 180;
    previous.hold_blend_ticks = 4;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 123_956,
            max_overshoot_c: 1.5,
            hold_peak_to_peak_c: 3.2,
            sample_count: 614,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(219.7),
                first_hold_error_c: Some(0.3),
                residual_heat_after_hold_entry_c: Some(1.8),
                approach_median_output_permille: Some(1_000),
                approach_median_slope_c_per_s: Some(1.0),
                hold_median_output_permille: Some(990),
                hold_p90_output_permille: Some(1_000),
                hold_mean_error_c: Some(0.11),
                hold_max_above_target_c: Some(1.5),
                hold_max_below_target_c: Some(1.7),
                approach_sample_count: 30,
                hold_sample_count: 201,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, 990);
    assert_eq!(tuned.hold_off_centi_c, 50);
    assert_eq!(tuned.hold_on_centi_c, 75);
    assert_eq!(tuned.overshoot_cutoff_centi_c, 383);
    assert_eq!(tuned.hold_blend_ticks, 4);
}

#[test]
fn thermal_high_temp_entry_carry_ripple_lowers_hold_on_and_blend() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 220,
        brake_distance_centi_c: 500,
        warmup_power_permille: 980,
        approach_power_permille: 920,
        approach_floor_power_permille: 730,
        approach_damping_exponent_permille: 250,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 740,
        hold_reheat_power_permille: 790,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 100,
        hold_exit_centi_c: 170,
        hold_on_centi_c: 200,
        hold_off_centi_c: 250,
        overshoot_cutoff_centi_c: 275,
        hold_kp_permille_per_c: 26,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 10,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 143_139,
            max_overshoot_c: 1.7,
            hold_peak_to_peak_c: 4.1,
            sample_count: 1_154,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(219.6),
                first_hold_error_c: Some(0.4),
                residual_heat_after_hold_entry_c: Some(2.1),
                approach_median_output_permille: Some(810),
                approach_median_slope_c_per_s: Some(2.43),
                hold_median_output_permille: Some(810),
                hold_p90_output_permille: Some(850),
                hold_mean_error_c: Some(0.71),
                hold_max_above_target_c: Some(1.7),
                hold_max_below_target_c: Some(2.4),
                approach_sample_count: 465,
                hold_sample_count: 243,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(
        tuned.approach_power_permille,
        previous.approach_power_permille
    );
    assert!(tuned.approach_floor_power_permille <= tuned.hold_reheat_power_permille);
    assert_eq!(tuned.hold_entry_centi_c, previous.hold_entry_centi_c);
    assert_eq!(tuned.hold_exit_centi_c, previous.hold_exit_centi_c);
    assert!(tuned.hold_on_centi_c < previous.hold_on_centi_c);
    assert!(tuned.hold_on_centi_c <= 160);
    assert!(tuned.hold_off_centi_c < previous.hold_off_centi_c);
    assert!(tuned.hold_blend_ticks < previous.hold_blend_ticks);
    assert!(tuned.hold_blend_ticks <= 6);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
    assert!(tuned.hold_kp_permille_per_c > previous.hold_kp_permille_per_c);
}

#[test]
fn thermal_passing_stage_keeps_existing_candidate() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: 1286,
        warmup_power_permille: 390,
        approach_power_permille: 260,
        approach_floor_power_permille: 275,
        approach_damping_exponent_permille: 975,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 59,
        hold_reheat_power_permille: 390,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 25,
        hold_exit_centi_c: 75,
        hold_on_centi_c: 30,
        hold_off_centi_c: 198,
        overshoot_cutoff_centi_c: 353,
        hold_kp_permille_per_c: 18,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 10,
        approach_lead_ticks: 4,
        hold_lead_ticks: 0,
    };
    let result = ThermalStageResult {
        target_temp_c: 100,
        rise_time_ms: 114_668,
        max_overshoot_c: 0.0,
        hold_peak_to_peak_c: 2.1,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        sample_count: 686,
        analysis: ThermalStageAnalysis {
            approach_sample_count: 343,
            approach_median_output_permille: Some(220),
            approach_median_slope_c_per_s: Some(1.82),
            first_hold_temp_c: Some(99.8),
            first_hold_error_c: Some(0.2),
            hold_sample_count: 241,
            hold_mean_error_c: Some(1.68),
            hold_max_below_target_c: Some(2.3),
            hold_max_above_target_c: Some(0.0),
            hold_median_output_permille: Some(220),
            hold_p90_output_permille: Some(230),
            residual_heat_after_hold_entry_c: Some(0.0),
            ..ThermalStageAnalysis::default()
        },
        guard: ThermalApproachGuardAnalysis::default(),
        full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
    };

    let tuned = tune_thermal_candidate_point(previous, &result);

    assert_eq!(tuned, previous);
}

#[test]
fn thermal_high_temp_deep_hold_drop_widens_residency_and_adds_lead() {
    let mut previous = thermal_default_target_point(220);
    previous.hold_exit_centi_c = 50;
    previous.hold_lead_ticks = 0;

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 220,
            rise_time_ms: 69_621,
            max_overshoot_c: 1.7,
            hold_peak_to_peak_c: 6.2,
            sample_count: 1_136,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_temp_c: Some(221.3),
                first_hold_error_c: Some(-1.3),
                residual_heat_after_hold_entry_c: Some(0.4),
                approach_median_output_permille: Some(740),
                approach_median_slope_c_per_s: Some(3.04),
                hold_median_output_permille: Some(730),
                hold_p90_output_permille: Some(750),
                hold_mean_error_c: Some(1.78),
                hold_max_above_target_c: Some(1.7),
                hold_max_below_target_c: Some(4.5),
                approach_sample_count: 85,
                hold_sample_count: 467,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis {
                settle_time_ms: Some(8_425),
                ..ThermalFullSpeedStableAnalysis::default()
            },
        },
    );

    assert_eq!(tuned.hold_exit_centi_c, 300);
    assert_eq!(tuned.hold_lead_ticks, 1);
    assert!(tuned.hold_power_permille >= 700);
    assert!(tuned.hold_reheat_power_permille >= tuned.hold_power_permille);
}

#[test]
fn thermal_replay_analysis_recovers_positive_approach_slope() {
    let samples = vec![
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 0,
            "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 1_000,
            "heaterTelemetry": { "currentTempC": 94.0, "heaterOutputPercent": 30 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 2_000,
            "heaterTelemetry": { "currentTempC": 96.2, "heaterOutputPercent": 20 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 3_000,
            "heaterTelemetry": { "currentTempC": 100.4, "heaterOutputPercent": 15 },
            "status": { "heaterControlPhase": "hold" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 4_000,
            "heaterTelemetry": { "currentTempC": 103.8, "heaterOutputPercent": 0 },
            "status": { "heaterControlPhase": "hold" },
        }),
    ];

    let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
    let analysis = thermal_replay_stage_analysis(&stage_samples, 100);

    assert!(analysis.approach_median_slope_c_per_s.unwrap_or(0.0) > 0.5);
    assert_eq!(analysis.first_hold_temp_c, Some(100.4));
    assert!(analysis.residual_heat_after_hold_entry_c.unwrap_or(0.0) > 3.0);
    assert_eq!(
        analysis.approach_curve_fit_basis,
        Some("target_error_from_approach_start")
    );
    assert_eq!(
        analysis.approach_curve_preferred_ms,
        Some(THERMAL_APPROACH_CURVE_PREFERRED_MS)
    );
    assert_eq!(
        analysis.approach_curve_limit_ms,
        Some(THERMAL_APPROACH_CURVE_LIMIT_MS)
    );
}

#[test]
fn thermal_replay_uses_guarded_control_temperature_for_metrics() {
    let samples = vec![
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 0,
            "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
            "status": {
                "heaterControlPhase": "approach",
                "heaterControlTempC": 92.0,
                "heaterFilteredTempC": 92.0,
                "currentTempC": 92.0
            },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 1_000,
            "heaterTelemetry": { "currentTempC": 250.0, "heaterOutputPercent": 0 },
            "status": {
                "heaterControlPhase": "hold",
                "heaterControlTempC": 100.4,
                "heaterFilteredTempC": 100.2,
                "currentTempC": 250.0,
                "heaterControlMeasurementGuarded": true
            },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 2_000,
            "heaterTelemetry": { "currentTempC": 248.0, "heaterOutputPercent": 0 },
            "status": {
                "heaterControlPhase": "hold",
                "heaterControlTempC": 100.8,
                "heaterFilteredTempC": 100.5,
                "currentTempC": 248.0,
                "heaterControlMeasurementGuarded": true
            },
        }),
    ];

    let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
    let analysis = thermal_replay_stage_analysis(&stage_samples, 100);

    assert_eq!(analysis.first_hold_temp_c, Some(100.4));
    assert!(analysis.hold_max_above_target_c.unwrap_or_default() < 1.0);
    assert!(
        analysis
            .residual_heat_after_hold_entry_c
            .unwrap_or_default()
            < 1.0
    );
    assert_eq!(samples[1]["heaterTelemetry"]["currentTempC"], json!(250.0));
}

#[test]
fn thermal_replay_analysis_classifies_underpowered_approach_curve() {
    let samples = vec![
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 0,
            "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 2_000,
            "heaterTelemetry": { "currentTempC": 93.5, "heaterOutputPercent": 30 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 6_000,
            "heaterTelemetry": { "currentTempC": 95.0, "heaterOutputPercent": 20 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 10_000,
            "heaterTelemetry": { "currentTempC": 96.1, "heaterOutputPercent": 10 },
            "status": { "heaterControlPhase": "approach" },
        }),
    ];

    let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
    let analysis = thermal_replay_stage_analysis(&stage_samples, 100);

    assert_eq!(
        analysis.approach_curve_deviation_class,
        Some("underpowered_or_early_coast")
    );
    assert_eq!(
        analysis.approach_curve_fitted_ms,
        Some(THERMAL_APPROACH_CURVE_LIMIT_MS)
    );
    assert!(analysis.approach_curve_max_below_c.unwrap_or_default() > 3.0);
    assert_eq!(analysis.approach_curve_tail_uses_half_floor, Some(true));
}

#[test]
fn thermal_replay_source_analysis_summarizes_approach_and_hold_windows() {
    let samples = vec![
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 0,
            "sourceTelemetry": { "voltageMv": 21_000, "currentMa": 3_000, "powerMw": 63_000 },
            "heaterTelemetry": { "currentTempC": 92.0, "heaterOutputPercent": 40 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 1_000,
            "sourceTelemetry": { "voltageMv": 20_000, "currentMa": 2_500, "powerMw": 50_000 },
            "heaterTelemetry": { "currentTempC": 94.0, "heaterOutputPercent": 30 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "warmup",
            "elapsedMs": 2_000,
            "sourceTelemetry": { "voltageMv": 15_000, "currentMa": 1_800, "powerMw": 27_000 },
            "heaterTelemetry": { "currentTempC": 96.2, "heaterOutputPercent": 20 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 3_000,
            "sourceTelemetry": { "voltageMv": 11_000, "currentMa": 1_600, "powerMw": 17_600 },
            "heaterTelemetry": { "currentTempC": 100.4, "heaterOutputPercent": 15 },
            "status": { "heaterControlPhase": "hold" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 100,
            "phase": "hold",
            "elapsedMs": 4_000,
            "sourceTelemetry": { "voltageMv": 10_000, "currentMa": 1_400, "powerMw": 14_000 },
            "heaterTelemetry": { "currentTempC": 103.8, "heaterOutputPercent": 0 },
            "status": { "heaterControlPhase": "hold" },
        }),
    ];

    let stage_samples = thermal_replay_stage_samples(&samples, 100).unwrap();
    let source_analysis = thermal_replay_stage_source_analysis(&stage_samples, 100);

    assert_eq!(source_analysis["approachSource"]["sampleCount"], json!(3));
    assert_eq!(
        source_analysis["approachSource"]["voltageMv"]["min"],
        json!(15_000.0)
    );
    assert_eq!(
        source_analysis["approachSource"]["voltageMv"]["max"],
        json!(21_000.0)
    );
    assert_eq!(
        source_analysis["approachSource"]["currentMa"]["last"],
        json!(1_800.0)
    );
    assert_eq!(source_analysis["holdSource"]["sampleCount"], json!(2));
    assert_eq!(
        source_analysis["holdSource"]["powerMw"]["first"],
        json!(17_600.0)
    );
    assert_eq!(
        source_analysis["holdSource"]["powerMw"]["last"],
        json!(14_000.0)
    );
}

#[test]
fn thermal_replay_preserves_full_batch_candidate_profile() {
    let mut original = thermal_seed_candidate_profile();
    thermal_candidate_point_mut(&mut original, 60)
        .expect("60 point")
        .hold_power_permille = 777;
    let original_value = thermal_candidate_profile_to_value(&original);
    let heater_parameters = thermal_heater_parameters_value(140, Some(&original_value), "preview");
    let summary = json!({
        "candidateProfile": original_value,
        "parameters": { "seedProfileFile": null }
    });
    let samples = vec![json!({
        "testPhase": "applied",
        "targetTempC": 140,
        "heaterParameters": heater_parameters,
    })];

    let replayed = thermal_replay_applied_profile(&summary, &samples, &[140]).unwrap();

    assert_eq!(
        thermal_candidate_point(&replayed, 60)
            .expect("replayed 60 point")
            .hold_power_permille,
        777
    );
}

#[test]
fn thermal_relation_rebuild_scales_power_against_default_curve() {
    let scale_power = |value: u16| ((f32::from(value) * 1.1) + 0.5).min(1_000.0) as u16;
    let lower_default = thermal_default_target_point(100);
    let upper_default = thermal_default_target_point(250);
    let target_default = thermal_default_target_point(140);
    let lower = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: lower_default.brake_distance_centi_c.saturating_add(200),
        warmup_power_permille: scale_power(lower_default.warmup_power_permille),
        approach_power_permille: scale_power(lower_default.approach_power_permille),
        approach_floor_power_permille: scale_power(lower_default.approach_floor_power_permille),
        approach_damping_exponent_permille: lower_default
            .approach_damping_exponent_permille
            .saturating_add(150),
        approach_tail_window_centi_c: lower_default.approach_tail_window_centi_c,
        hold_power_permille: scale_power(lower_default.hold_power_permille),
        hold_reheat_power_permille: scale_power(lower_default.hold_reheat_power_permille),
        warmup_reenter_centi_c: lower_default.warmup_reenter_centi_c,
        hold_entry_centi_c: lower_default.hold_entry_centi_c.saturating_add(5),
        hold_exit_centi_c: lower_default.hold_exit_centi_c.saturating_add(10),
        hold_on_centi_c: lower_default.hold_on_centi_c.saturating_add(5),
        hold_off_centi_c: lower_default.hold_off_centi_c.saturating_add(20),
        overshoot_cutoff_centi_c: lower_default.overshoot_cutoff_centi_c.saturating_add(30),
        hold_kp_permille_per_c: lower_default.hold_kp_permille_per_c.saturating_add(4),
        hold_ki_permille_per_c_tick: lower_default.hold_ki_permille_per_c_tick.saturating_add(1),
        hold_blend_ticks: lower_default.hold_blend_ticks.saturating_add(2),
        approach_lead_ticks: lower_default.approach_lead_ticks.saturating_add(3),
        hold_lead_ticks: lower_default.hold_lead_ticks.saturating_add(1),
    };
    let upper = ThermalCandidatePoint {
        target_temp_c: 250,
        brake_distance_centi_c: upper_default.brake_distance_centi_c.saturating_add(200),
        warmup_power_permille: scale_power(upper_default.warmup_power_permille),
        approach_power_permille: scale_power(upper_default.approach_power_permille),
        approach_floor_power_permille: scale_power(upper_default.approach_floor_power_permille),
        approach_damping_exponent_permille: upper_default
            .approach_damping_exponent_permille
            .saturating_add(150),
        approach_tail_window_centi_c: upper_default.approach_tail_window_centi_c,
        hold_power_permille: scale_power(upper_default.hold_power_permille),
        hold_reheat_power_permille: scale_power(upper_default.hold_reheat_power_permille),
        warmup_reenter_centi_c: upper_default.warmup_reenter_centi_c,
        hold_entry_centi_c: upper_default.hold_entry_centi_c.saturating_add(5),
        hold_exit_centi_c: upper_default.hold_exit_centi_c.saturating_add(10),
        hold_on_centi_c: upper_default.hold_on_centi_c.saturating_add(5),
        hold_off_centi_c: upper_default.hold_off_centi_c.saturating_add(20),
        overshoot_cutoff_centi_c: upper_default.overshoot_cutoff_centi_c.saturating_add(30),
        hold_kp_permille_per_c: upper_default.hold_kp_permille_per_c.saturating_add(4),
        hold_ki_permille_per_c_tick: upper_default.hold_ki_permille_per_c_tick.saturating_add(1),
        hold_blend_ticks: upper_default.hold_blend_ticks.saturating_add(2),
        approach_lead_ticks: upper_default.approach_lead_ticks.saturating_add(3),
        hold_lead_ticks: upper_default.hold_lead_ticks.saturating_add(1),
    };

    let rebuilt = rebuild_thermal_candidate_point_from_anchor_relations(140, lower, upper);

    assert_eq!(
        rebuilt.hold_power_permille,
        scale_power(target_default.hold_power_permille)
    );
    assert_eq!(
        rebuilt.brake_distance_centi_c,
        target_default.brake_distance_centi_c.saturating_add(200)
    );
    assert_eq!(
        rebuilt.approach_damping_exponent_permille,
        target_default
            .approach_damping_exponent_permille
            .saturating_add(150)
    );
    assert!(rebuilt.approach_floor_power_permille >= rebuilt.hold_power_permille);
    assert!(rebuilt.approach_power_permille >= rebuilt.approach_floor_power_permille);
    assert!(rebuilt.warmup_power_permille >= rebuilt.approach_power_permille);
    assert!(rebuilt.hold_reheat_power_permille >= rebuilt.approach_floor_power_permille);
}

#[test]
fn thermal_relation_rebuild_preserves_out_of_span_points() {
    let mut profile = thermal_seed_candidate_profile();
    if let Some(point) = thermal_candidate_point_mut(&mut profile, 220) {
        point.hold_power_permille = 777;
        point.hold_reheat_power_permille = 888;
    }
    if let Some(point) = thermal_candidate_point_mut(&mut profile, 250) {
        point.hold_power_permille = 901;
        point.hold_reheat_power_permille = 944;
    }

    thermal_rebuild_profile_from_anchor_targets(&mut profile, &[60, 100, 140, 180]);

    let point_220 = thermal_candidate_point(&profile, 220).expect("220 point");
    let point_250 = thermal_candidate_point(&profile, 250).expect("250 point");
    assert_eq!(point_220.hold_power_permille, 777);
    assert_eq!(point_220.hold_reheat_power_permille, 888);
    assert_eq!(point_250.hold_power_permille, 901);
    assert_eq!(point_250.hold_reheat_power_permille, 944);
    assert_eq!(point_220.target_temp_c, 220);
    assert_eq!(point_250.target_temp_c, 250);
}

#[test]
fn thermal_persisted_profile_preserves_supplied_point_local_targets() {
    let profile = thermal_candidate_profile_from_value(json!({
        "points": [
            {"targetTempC": 60, "brakeDistanceCentiC": 1100, "approachPowerPermille": 450, "approachFloorPowerPermille": 160, "approachDampingExponentPermille": 4000, "approachTailWindowCentiC": 375, "holdPowerPermille": 135, "holdReheatPowerPermille": 170, "holdEntryCentiC": 220, "holdExitCentiC": 400, "holdOnCentiC": 30, "holdOffCentiC": 40, "overshootCutoffCentiC": 50, "holdKpPermillePerC": 8, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 6, "holdLeadTicks": 6, "warmupPowerPermille": 760},
            {"targetTempC": 80, "brakeDistanceCentiC": 980, "approachPowerPermille": 440, "approachFloorPowerPermille": 220, "approachDampingExponentPermille": 2610, "approachTailWindowCentiC": 375, "holdPowerPermille": 140, "holdReheatPowerPermille": 150, "holdEntryCentiC": 150, "holdExitCentiC": 230, "holdOnCentiC": 20, "holdOffCentiC": 115, "overshootCutoffCentiC": 140, "holdKpPermillePerC": 12, "holdKiPermillePerCTick": 1, "holdBlendTicks": 3, "approachLeadTicks": 7, "holdLeadTicks": 6, "warmupPowerPermille": 850},
            {"targetTempC": 100, "brakeDistanceCentiC": 1340, "approachPowerPermille": 340, "approachFloorPowerPermille": 220, "approachDampingExponentPermille": 1500, "approachTailWindowCentiC": 375, "holdPowerPermille": 220, "holdReheatPowerPermille": 220, "holdEntryCentiC": 150, "holdExitCentiC": 120, "holdOnCentiC": 10, "holdOffCentiC": 50, "overshootCutoffCentiC": 50, "holdKpPermillePerC": 20, "holdKiPermillePerCTick": 1, "holdBlendTicks": 2, "approachLeadTicks": 9, "holdLeadTicks": 8, "warmupPowerPermille": 1000},
            {"targetTempC": 140, "brakeDistanceCentiC": 940, "approachPowerPermille": 420, "approachFloorPowerPermille": 320, "approachDampingExponentPermille": 910, "approachTailWindowCentiC": 0, "holdPowerPermille": 335, "holdReheatPowerPermille": 400, "holdEntryCentiC": 150, "holdExitCentiC": 100, "holdOnCentiC": 10, "holdOffCentiC": 160, "overshootCutoffCentiC": 220, "holdKpPermillePerC": 22, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 2, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
            {"targetTempC": 180, "brakeDistanceCentiC": 650, "approachPowerPermille": 760, "approachFloorPowerPermille": 460, "approachDampingExponentPermille": 800, "approachTailWindowCentiC": 0, "holdPowerPermille": 450, "holdReheatPowerPermille": 620, "holdEntryCentiC": 120, "holdExitCentiC": 70, "holdOnCentiC": 25, "holdOffCentiC": 140, "overshootCutoffCentiC": 300, "holdKpPermillePerC": 20, "holdKiPermillePerCTick": 1, "holdBlendTicks": 3, "approachLeadTicks": 4, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
            {"targetTempC": 220, "brakeDistanceCentiC": 400, "approachPowerPermille": 900, "approachFloorPowerPermille": 800, "approachDampingExponentPermille": 400, "approachTailWindowCentiC": 0, "holdPowerPermille": 750, "holdReheatPowerPermille": 850, "holdEntryCentiC": 120, "holdExitCentiC": 50, "holdOnCentiC": 5, "holdOffCentiC": 240, "overshootCutoffCentiC": 320, "holdKpPermillePerC": 22, "holdKiPermillePerCTick": 1, "holdBlendTicks": 2, "approachLeadTicks": 2, "holdLeadTicks": 0, "warmupPowerPermille": 1000},
            {"targetTempC": 250, "brakeDistanceCentiC": 200, "approachPowerPermille": 1000, "approachFloorPowerPermille": 1000, "approachDampingExponentPermille": 350, "approachTailWindowCentiC": 0, "holdPowerPermille": 850, "holdReheatPowerPermille": 930, "holdEntryCentiC": 150, "holdExitCentiC": 55, "holdOnCentiC": 14, "holdOffCentiC": 320, "overshootCutoffCentiC": 420, "holdKpPermillePerC": 12, "holdKiPermillePerCTick": 1, "holdBlendTicks": 1, "approachLeadTicks": 1, "holdLeadTicks": 0, "warmupPowerPermille": 1000}
        ],
        "settings": {}
    }));

    let persisted = thermal_profile_for_persistence(&profile).expect("seven points fit firmware");

    assert_eq!(persisted, profile);
}

#[test]
fn thermal_persisted_profile_rejects_more_than_firmware_capacity() {
    let profile = ThermalCandidateProfile {
        settings: thermal_default_settings(),
        points: THERMAL_SUPPORTED_TARGETS_C
            .iter()
            .copied()
            .map(thermal_default_target_point)
            .collect(),
    };

    let error = thermal_profile_for_persistence(&profile).unwrap_err();
    assert!(error.to_string().contains("at most 10"));
}

#[test]
fn parses_thermal_self_test_targets_subset_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "self-test",
        "--device",
        "mock-fp-lab-01",
        "--source-id",
        "iso-mock",
        "--source-url",
        "http://127.0.0.1:1",
        "--targets-c",
        "140,250",
    ])
    .expect("parse thermal subset");

    let Command::Thermal {
        command: ThermalCommand::SelfTest(args),
    } = cli.command
    else {
        panic!("expected thermal self-test command");
    };

    assert_eq!(args.targets_c.as_deref(), Some("140,250"));
    assert_eq!(args.source_kind, BenchSourceKind::Isolapurr);
    assert_eq!(
        args.evaluation_mode,
        ThermalSelfTestEvaluationMode::HoldConfirm
    );
    assert_eq!(args.sample_interval_ms, 300);
    assert_eq!(effective_thermal_sample_interval_ms(333), 300);
}

#[test]
fn thermal_model_cli_keeps_only_direct_calibration() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "model",
        "calibrate",
        "--device",
        "mock-fp-lab-01",
    ])
    .expect("parse direct thermal model calibration");
    assert!(matches!(
        cli.command,
        Command::Thermal {
            command: ThermalCommand::Model {
                command: ThermalModelCommand::Calibrate(_),
            },
        }
    ));

    for removed in [
        "validate-candidate",
        "save-candidate",
        "promote-candidate",
        "clear-candidate",
    ] {
        assert!(
            Cli::try_parse_from([
                "flux-purr",
                "thermal",
                "model",
                removed,
                "--device",
                "mock-fp-lab-01",
            ])
            .is_err()
        );
    }
}

#[test]
fn parses_thermal_self_test_batch_candidate_files() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "self-test",
        "--device",
        "mock-fp-lab-01",
        "--source-id",
        "iso-mock",
        "--source-url",
        "http://127.0.0.1:1",
        "--targets-c",
        "140",
        "--skip-optimize",
        "--candidate-profile-file",
        "/tmp/a.json",
        "--candidate-profile-file",
        "/tmp/b.json",
    ])
    .expect("parse thermal batch");

    let Command::Thermal {
        command: ThermalCommand::SelfTest(args),
    } = cli.command
    else {
        panic!("expected thermal self-test command");
    };

    assert_eq!(
        args.candidate_profile_files,
        vec![PathBuf::from("/tmp/a.json"), PathBuf::from("/tmp/b.json")]
    );
}

#[test]
fn thermal_self_test_100w_defaults_effective_source_power_to_100w() {
    let args = ThermalSelfTestArgs {
        target: TargetSelector {
            device: Some("mock-fp-lab-01".to_string()),
            hardware: None,
        },
        source_kind: BenchSourceKind::Isolapurr,
        source_id: "iso-mock".to_string(),
        source_url: "http://127.0.0.1:1".to_string(),
        profile_mode: ThermalProfileMode::W100,
        source_voltage_v: None,
        source_current_a: None,
        source_power_watts: 0,
        source_mode: "auto-follow".to_string(),
        sample_interval_ms: 300,
        evaluation_mode: ThermalSelfTestEvaluationMode::HoldConfirm,
        hold_seconds: 60,
        stage_timeout_seconds: 180,
        warmup_timeout_seconds: 180,
        runtime_rearm_attempts: 1,
        calibration_run: false,
        optimize_targets_c: None,
        skip_optimize: false,
        cooldown_temp_c: 40.0,
        cooldown_timeout_seconds: 7200,
        targets_c: None,
        seed_profile_file: None,
        candidate_profile_files: Vec::new(),
        output_dir: PathBuf::from("thermal-self-test-runs"),
        dry_run: true,
        execution_deadline: None,
    };
    let selection = resolve_thermal_source_selection(&args).expect("source selection");

    assert_eq!(selection.resolved_bank, "pps5a");
    assert_eq!(thermal_effective_source_power_watts(&args, &selection), 100);
    assert!(args.execution_deadline.is_none());
}

#[test]
fn parses_thermal_tune_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "tune",
        "--device",
        "mock-fp-lab-01",
        "--source-id",
        "iso-mock",
        "--source-url",
        "http://127.0.0.1:1",
        "--dry-run",
        "--per-target-budget-seconds",
        "600",
        "--max-tuning-rounds",
        "1",
    ])
    .expect("parse thermal flagship tune");

    let Command::Thermal {
        command: ThermalCommand::Tune(args),
    } = cli.command
    else {
        panic!("expected thermal tune command");
    };

    assert_eq!(args.profile_mode, ThermalProfileMode::W100);
    assert_eq!(args.source_id, "iso-mock");
    assert_eq!(args.per_target_budget_seconds, 600);
    assert_eq!(args.max_tuning_rounds, Some(1));
    assert_eq!(args.runtime_rearm_attempts, 3);
    assert_eq!(args.anchor_targets_c, "60,80,100,120,140,160,180,220,240");
    assert_eq!(
        args.validation_targets_c,
        "60,80,100,120,140,160,180,220,240"
    );
    assert_eq!(args.tune_targets_c, "60,80,100,120,140,160,180,220,240");
    assert!(args.dry_run);
}

#[test]
fn parses_thermal_flagship_tune_alias() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "flagship-tune",
        "--device",
        "mock-fp-lab-01",
        "--source-id",
        "iso-mock",
        "--source-url",
        "http://127.0.0.1:1",
        "--dry-run",
    ])
    .expect("parse thermal flagship-tune alias");

    let Command::Thermal {
        command: ThermalCommand::Tune(args),
    } = cli.command
    else {
        panic!("expected thermal tune command from alias");
    };

    assert_eq!(args.profile_mode, ThermalProfileMode::W100);
    assert_eq!(args.source_id, "iso-mock");
    assert!(args.dry_run);
}

#[test]
fn thermal_batch_restart_uses_target_minus_thirty_with_forty_degree_floor() {
    assert_eq!(thermal_batch_restart_temp_c(60, 40.0), 40.0);
    assert_eq!(thermal_batch_restart_temp_c(100, 40.0), 70.0);
    assert_eq!(thermal_batch_restart_temp_c(220, 40.0), 190.0);
}

#[test]
fn thermal_batch_restart_respects_explicit_cooldown_override() {
    assert_eq!(thermal_batch_restart_temp_c(60, 35.0), 35.0);
    assert_eq!(thermal_batch_restart_temp_c(220, 180.0), 180.0);
}

#[test]
fn parses_thermal_retune_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "retune",
        "--run-dir",
        "/tmp/thermal-run",
        "--optimize-targets-c",
        "100,220",
    ])
    .expect("parse thermal retune");

    let Command::Thermal {
        command: ThermalCommand::Retune(args),
    } = cli.command
    else {
        panic!("expected thermal retune command");
    };

    assert_eq!(args.run_dir, PathBuf::from("/tmp/thermal-run"));
    assert_eq!(args.optimize_targets_c.as_deref(), Some("100,220"));
}

#[test]
fn parses_thermal_report_rerender_legacy_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "report",
        "rerender-legacy",
        "--legacy-bundle-dir",
        "/tmp/legacy-bundle",
        "--output-dir",
        "/tmp/compliant-bundle",
    ])
    .expect("parse thermal report rerender legacy");

    let Command::Thermal {
        command: ThermalCommand::Report { command },
    } = cli.command
    else {
        panic!("expected thermal report command");
    };

    let ThermalReportCommand::RerenderLegacy(args) = command else {
        panic!("expected legacy rerender command");
    };

    assert_eq!(args.legacy_bundle_dir, PathBuf::from("/tmp/legacy-bundle"));
    assert_eq!(
        args.output_dir,
        Some(PathBuf::from("/tmp/compliant-bundle"))
    );
}

#[test]
fn parses_thermal_report_render_self_test_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "thermal",
        "report",
        "render-self-test",
        "--run-dir",
        "/tmp/raw-self-test",
        "--output-dir",
        "/tmp/html-bundle",
    ])
    .expect("parse thermal self-test report command");

    let Command::Thermal {
        command: ThermalCommand::Report { command },
    } = cli.command
    else {
        panic!("expected thermal report command");
    };
    let ThermalReportCommand::RenderSelfTest(args) = command else {
        panic!("expected self-test report command");
    };

    assert_eq!(args.run_dir, vec![PathBuf::from("/tmp/raw-self-test")]);
    assert_eq!(args.output_dir, Some(PathBuf::from("/tmp/html-bundle")));
}

#[test]
fn thermal_hold_tracker_samples_one_minute_after_entering_hold() {
    let start = tokio::time::Instant::now();
    let mut tracker = ThermalHoldTracker::new(120, Duration::from_secs(10));

    assert_eq!(
        tracker.observe(119.6, 1_000, start, false),
        ThermalHoldObservation::Warmup
    );
    assert_eq!(
        tracker.observe(119.6, 1_500, start + Duration::from_millis(500), true),
        ThermalHoldObservation::Hold
    );
    assert_eq!(
        tracker.observe(119.8, 5_000, start + Duration::from_secs(5), true),
        ThermalHoldObservation::Hold
    );
    assert_eq!(
        tracker.observe(119.2, 6_000, start + Duration::from_secs(6), false),
        ThermalHoldObservation::Hold
    );
    assert_eq!(tracker.rise_time_ms(), Some(1_500));
    assert_eq!(
        tracker.observe(119.7, 11_500, start + Duration::from_millis(11_500), false),
        ThermalHoldObservation::Completed
    );
    assert!((tracker.peak_to_peak_c() - 0.6).abs() < 0.001);
}

#[test]
fn thermal_approach_guard_fails_when_hold_threshold_is_not_crossed_in_ten_seconds() {
    let mut guard = ThermalApproachGuardTracker::new(140, 20);

    assert_eq!(guard.observe(132.0, 0, Some("approach")), None);
    assert_eq!(guard.observe(137.9, 10_000, Some("approach")), None);
    assert_eq!(
        guard.observe(137.9, 10_001, Some("approach")),
        Some("approach_threshold_timeout")
    );

    let analysis = guard.finalize();
    assert_eq!(analysis.approach_started_at_ms, Some(0));
    assert_eq!(analysis.hold_threshold_crossed_at_ms, None);
    assert_eq!(analysis.first_hold_at_ms, None);
}

#[test]
fn thermal_approach_guard_fails_when_approach_reenters_warmup_before_hold() {
    let mut guard = ThermalApproachGuardTracker::new(180, 15);

    assert_eq!(guard.observe(170.0, 1_000, Some("approach")), None);
    assert_eq!(
        guard.observe(165.0, 4_000, Some("warmup")),
        Some("approach_reentered_warmup")
    );

    let analysis = guard.finalize();
    assert_eq!(analysis.approach_started_at_ms, Some(1_000));
    assert_eq!(analysis.warmup_reentered_at_ms, Some(4_000));
    assert_eq!(analysis.first_hold_at_ms, None);
}

#[test]
fn thermal_approach_guard_fails_when_hold_is_not_entered_in_thirty_seconds() {
    let mut guard = ThermalApproachGuardTracker::new(220, 8);

    assert_eq!(guard.observe(215.0, 500, Some("approach")), None);
    assert_eq!(guard.observe(219.95, 8_000, Some("approach")), None);
    assert_eq!(guard.observe(219.95, 30_000, Some("approach")), None);
    assert_eq!(
        guard.observe(219.95, 30_501, Some("approach")),
        Some("approach_hold_timeout")
    );

    let analysis = guard.finalize();
    assert_eq!(analysis.approach_started_at_ms, Some(500));
    assert_eq!(analysis.hold_threshold_crossed_at_ms, Some(8_000));
    assert_eq!(analysis.first_hold_at_ms, None);
}

#[test]
fn thermal_full_speed_tracker_accepts_hold_entry_within_ten_seconds() {
    let mut tracker = ThermalFullSpeedStableTracker::new(140);

    assert_eq!(
        tracker.observe(100.0, 0, Some("warmup")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(132.0, 250, Some("approach")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(139.2, 2_000, Some("hold")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(140.0, 11_000, Some("hold")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(140.0, 12_000, Some("approach")),
        ThermalFullSpeedStableObservation::Verified
    );
    assert_eq!(
        tracker.observe(139.0, 12_250, Some("approach")),
        ThermalFullSpeedStableObservation::Verified
    );
    assert_eq!(tracker.finalize().settle_time_ms, Some(1_750));

    let analysis = tracker.finalize();
    assert_eq!(analysis.warmup_exited_at_ms, Some(250));
    assert_eq!(analysis.stable_window_started_at_ms, Some(2_000));
    assert_eq!(analysis.stable_window_verified_at_ms, Some(12_000));
    assert_eq!(analysis.settle_time_ms, Some(1_750));
}

#[test]
fn thermal_full_speed_tracker_starts_budget_when_warmup_phase_exits() {
    let mut tracker = ThermalFullSpeedStableTracker::new(140);

    assert_eq!(
        tracker.observe(100.0, 0, Some("warmup")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(132.0, 250, Some("approach")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(139.2, 2_000, Some("hold")),
        ThermalFullSpeedStableObservation::Pending
    );

    let analysis = tracker.finalize();
    assert_eq!(analysis.warmup_exited_at_ms, Some(250));
    assert_eq!(analysis.stable_window_started_at_ms, Some(2_000));
    assert_eq!(analysis.settle_time_ms, Some(1_750));
}

#[test]
fn thermal_full_speed_tracker_keeps_window_across_active_control_phases() {
    let mut tracker = ThermalFullSpeedStableTracker::new(180);

    assert_eq!(
        tracker.observe(173.0, 0, Some("warmup")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(178.8, 1_000, Some("approach")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(180.4, 6_000, Some("hold")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(179.2, 11_000, Some("approach")),
        ThermalFullSpeedStableObservation::Verified
    );

    let analysis = tracker.finalize();
    assert_eq!(analysis.stable_window_started_at_ms, Some(1_000));
    assert_eq!(analysis.stable_window_verified_at_ms, Some(11_000));
    assert_eq!(analysis.settle_time_ms, Some(0));
}

#[test]
fn thermal_full_speed_tracker_fails_once_a_timely_window_is_impossible() {
    let mut tracker = ThermalFullSpeedStableTracker::new(140);

    assert_eq!(
        tracker.observe(100.0, 0, Some("warmup")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(132.0, 250, Some("approach")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(139.0, 250, Some("approach")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(137.0, 500, Some("warmup")),
        ThermalFullSpeedStableObservation::Pending
    );
    assert_eq!(
        tracker.observe(139.0, 10_251, Some("approach")),
        ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout")
    );
    assert_eq!(
        tracker.finalize().failure_reason,
        Some("full_speed_to_stable_timeout")
    );
}

#[test]
fn thermal_tuning_scout_keeps_collecting_after_full_speed_timeout() {
    let full_speed_stop_reason = "full_speed_to_stable_timeout";

    let scout_stop_reason = if ThermalSelfTestEvaluationMode::TuningScout.enforces_stage_limits() {
        full_speed_stop_reason
    } else {
        "timeout"
    };
    let confirm_stop_reason = if ThermalSelfTestEvaluationMode::HoldConfirm.enforces_stage_limits()
    {
        full_speed_stop_reason
    } else {
        "timeout"
    };

    assert_eq!(scout_stop_reason, "timeout");
    assert_eq!(confirm_stop_reason, "full_speed_to_stable_timeout");
}

#[test]
fn thermal_replay_analysis_includes_post_hold_approach_samples_in_hold_window() {
    let samples = vec![
        json!({
            "testPhase": "applied",
            "targetTempC": 220,
            "phase": "warmup",
            "elapsedMs": 0,
            "heaterTelemetry": { "currentTempC": 216.2, "heaterOutputPercent": 91 },
            "status": { "heaterControlPhase": "approach" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 220,
            "phase": "hold",
            "elapsedMs": 1_000,
            "heaterTelemetry": { "currentTempC": 220.3, "heaterOutputPercent": 74 },
            "status": { "heaterControlPhase": "hold" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 220,
            "phase": "hold",
            "elapsedMs": 2_000,
            "heaterTelemetry": { "currentTempC": 221.0, "heaterOutputPercent": 72 },
            "status": { "heaterControlPhase": "hold" },
        }),
        json!({
            "testPhase": "applied",
            "targetTempC": 220,
            "phase": "hold",
            "elapsedMs": 3_000,
            "heaterTelemetry": { "currentTempC": 218.2, "heaterOutputPercent": 86 },
            "status": { "heaterControlPhase": "approach" },
        }),
    ];

    let stage_samples = thermal_replay_stage_samples(&samples, 220).unwrap();
    let analysis = thermal_replay_stage_analysis(&stage_samples, 220);

    assert_eq!(analysis.first_hold_temp_c, Some(220.3));
    assert_eq!(analysis.hold_sample_count, 3);
    assert_eq!(analysis.hold_median_output_permille, Some(740));
    assert_eq!(analysis.hold_max_above_target_c, Some(1.0));
    assert!((analysis.hold_max_below_target_c.unwrap_or_default() - 1.8).abs() < 0.001);
}

#[test]
fn thermal_curve_oscillation_class_triggers_hold_ripple_tuning_even_below_legacy_p2p_limit() {
    let previous = ThermalCandidatePoint {
        target_temp_c: 100,
        brake_distance_centi_c: 1_000,
        warmup_power_permille: 1_000,
        approach_power_permille: 240,
        approach_floor_power_permille: 130,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 60,
        hold_reheat_power_permille: 120,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 30,
        hold_exit_centi_c: 90,
        hold_on_centi_c: 30,
        hold_off_centi_c: 160,
        overshoot_cutoff_centi_c: 200,
        hold_kp_permille_per_c: 10,
        hold_ki_permille_per_c_tick: 2,
        hold_blend_ticks: 8,
        approach_lead_ticks: 4,
        hold_lead_ticks: 0,
    };

    let tuned = tune_thermal_candidate_point(
        previous,
        &ThermalStageResult {
            target_temp_c: 100,
            rise_time_ms: 8_200,
            max_overshoot_c: 1.0,
            hold_peak_to_peak_c: 2.0,
            sample_count: 140,
            stop_reason: "completed",
            terminal_runtime_drop_reason: None,
            analysis: ThermalStageAnalysis {
                first_hold_error_c: Some(0.1),
                hold_median_output_permille: Some(120),
                hold_p90_output_permille: Some(200),
                hold_mean_error_c: Some(0.2),
                hold_max_above_target_c: Some(0.8),
                hold_max_below_target_c: Some(1.2),
                approach_curve_deviation_class: Some("oscillatory_near_target"),
                approach_curve_max_above_c: Some(0.8),
                approach_curve_max_below_c: Some(1.2),
                hold_sample_count: 40,
                ..ThermalStageAnalysis::default()
            },
            guard: ThermalApproachGuardAnalysis::default(),
            full_speed_to_stable: ThermalFullSpeedStableAnalysis::default(),
        },
    );

    assert_eq!(tuned.hold_power_permille, 120);
    assert_eq!(tuned.approach_floor_power_permille, 140);
    assert_eq!(tuned.hold_reheat_power_permille, 200);
    assert_eq!(tuned.hold_kp_permille_per_c, 13);
}

#[test]
fn step_toward_u16_tolerates_reversed_bounds() {
    assert_eq!(step_toward_u16(1_000, 1_020, 40, 1_020, 1_000), 1_020);
    assert_eq!(step_toward_u16(980, 1_020, 40, 1_020, 1_000), 1_020);
}

#[test]
fn thermal_self_test_detects_runtime_drop_and_disarmed_heater() {
    let running = json!({
        "mode": "sampling",
        "uptimeSeconds": 34,
        "targetTempC": 210,
        "heaterEnabled": true,
    });
    assert_eq!(thermal_runtime_drop_reason(&running, 210, Some(33)), None);

    let latched_fault = json!({
        "mode": "sampling",
        "uptimeSeconds": 34,
        "targetTempC": 210,
        "heaterEnabled": true,
        "heaterFaultReason": "sensor-open",
    });
    assert_eq!(
        thermal_runtime_drop_reason(&latched_fault, 210, Some(33)),
        Some(ThermalRuntimeDropReason::LatchedFault)
    );
    assert!(thermal_recoverable_sensor_fault(&latched_fault));

    let glitch = json!({
        "mode": "sampling",
        "uptimeSeconds": 34,
        "targetTempC": 210,
        "heaterEnabled": true,
        "heaterFaultReason": "sensor-glitch",
    });
    assert_eq!(
        thermal_runtime_drop_reason(&glitch, 210, Some(33)),
        Some(ThermalRuntimeDropReason::LatchedFault)
    );
    assert!(thermal_recoverable_sensor_fault(&glitch));

    let active_fault = json!({
        "mode": "fault",
        "heaterFaultReason": "sensor-open",
    });
    assert!(!thermal_recoverable_sensor_fault(&active_fault));

    let over_temp = json!({
        "mode": "idle",
        "heaterFaultReason": "over-temp",
    });
    assert!(!thermal_recoverable_sensor_fault(&over_temp));

    let reset = json!({
        "mode": "idle",
        "uptimeSeconds": 0,
        "targetTempC": 210,
        "heaterEnabled": false,
    });
    assert_eq!(
        thermal_runtime_drop_reason(&reset, 210, Some(34)),
        Some(ThermalRuntimeDropReason::UptimeReset)
    );

    let idle = json!({
        "mode": "idle",
        "uptimeSeconds": 35,
        "targetTempC": 210,
        "heaterEnabled": false,
    });
    assert_eq!(
        thermal_runtime_drop_reason(&idle, 210, Some(34)),
        Some(ThermalRuntimeDropReason::WrongMode)
    );

    let disarmed = json!({
        "mode": "sampling",
        "uptimeSeconds": 35,
        "targetTempC": 210,
        "heaterEnabled": false,
    });
    assert_eq!(
        thermal_runtime_drop_reason(&disarmed, 210, Some(34)),
        Some(ThermalRuntimeDropReason::HeaterDisarmed)
    );
}

#[test]
fn thermal_runtime_readback_requires_target_and_enable_state() {
    let stale = json!({
        "targetTempC": 140,
        "heaterEnabled": false,
        "activeCoolingEnabled": true,
    });
    let settled = json!({
        "targetTempC": 140,
        "heaterEnabled": true,
        "activeCoolingEnabled": true,
    });

    assert!(!thermal_runtime_readback_matches(&stale, true, 140));
    assert!(thermal_runtime_readback_matches(&settled, true, 140));
    assert!(thermal_runtime_readback_matches(&stale, false, 140));
    assert!(!thermal_runtime_readback_matches(
        &json!({
            "targetTempC": 140,
            "heaterEnabled": false,
            "activeCoolingEnabled": false,
        }),
        false,
        140,
    ));
}

#[test]
fn thermal_self_test_shutdown_body_enables_active_cooling() {
    assert_eq!(
        thermal_self_test_runtime_body(false, 220),
        json!({
            "heaterEnabled": false,
            "targetTempC": 220,
            "activeCoolingEnabled": true,
        })
    );
    assert_eq!(
        thermal_self_test_runtime_body(true, 220),
        json!({
            "heaterEnabled": true,
            "targetTempC": 220,
        })
    );
}

#[test]
fn thermal_self_test_fault_attention_acknowledgement_keeps_heater_off() {
    let mut body = thermal_self_test_runtime_body(false, 140);
    body["faultAttentionAcknowledged"] = json!(true);
    assert_eq!(
        body,
        json!({
            "heaterEnabled": false,
            "targetTempC": 140,
            "activeCoolingEnabled": true,
            "faultAttentionAcknowledged": true,
        })
    );
}

#[test]
fn thermal_self_test_cooldown_body_clears_preview_and_enables_active_cooling() {
    assert_eq!(
        thermal_self_test_cooldown_runtime_body(),
        json!({
            "heaterEnabled": false,
            "activeCoolingEnabled": true,
            "thermalControlProfile": {
                "op": "clear_preview"
            }
        })
    );
}

#[test]
fn isolapurr_power_config_matchers_validate_manual_target_and_auto_mode() {
    let manual_config = json!({
        "tpsMode": "manual",
        "manual": {
            "voltageMv": 20_000,
            "currentLimitMa": 3_250,
            "usbCPathMode": "force",
            "pathPolicy": "force_open",
        }
    });
    let auto_config = json!({
        "tpsMode": "autoFollow",
        "manual": {
            "voltageMv": 20_000,
            "currentLimitMa": 3_250,
            "usbCPathMode": "default",
            "pathPolicy": "auto",
        }
    });

    assert!(isolapurr_power_config_value_matches_manual(
        &manual_config,
        20_000,
        3_250
    ));
    assert!(!isolapurr_power_config_value_is_auto(&manual_config));
    assert!(isolapurr_power_config_value_is_auto(&auto_config));
    assert!(isolapurr_power_config_path_is_automatic(&auto_config));
    assert!(!isolapurr_power_config_value_matches_manual(
        &manual_config,
        20_000,
        3_000
    ));

    let capped_100w_config = json!({
        "tpsMode": "manual",
        "manual": {
            "voltageMv": 21_000,
            "currentLimitMa": 4_750,
            "usbCPathMode": "force",
            "pathPolicy": "force_open",
        },
        "capability": {
            "powerWatts": 100,
            "protocols": { "pd": true },
            "pd": { "pps": true, "fixedVoltagesMv": [9000, 12000, 15000, 20000] },
            "current": { "pps3LimitMa": 5_000, "pdPps5a": true }
        }
    });
    assert!(isolapurr_power_config_value_matches_manual(
        &capped_100w_config,
        21_000,
        5_000
    ));
}

#[test]
fn parses_isolapurr_power_show_nested_usb_c_telemetry() {
    let power_show = json!({
        "ports": {
            "ports": [{
                "portId": "port_c",
                "label": "USB-C",
                "telemetry": {
                    "status": "ok",
                    "voltage_mv": 20_010,
                    "current_ma": 1_240,
                    "power_mw": 24_812,
                    "sample_uptime_ms": 99,
                }
            }]
        }
    });

    let telemetry = parse_isolapurr_live_telemetry(&power_show).unwrap();
    assert_eq!(telemetry.voltage_mv, 20_010);
    assert_eq!(telemetry.current_ma, 1_240);
    assert_eq!(telemetry.power_mw, 24_812);
}

#[test]
fn validates_isolapurr_thermal_capability_and_ready_voltage() {
    let config = json!({
        "capability": {
            "power_watts": 65,
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixed_voltages_mv": [9000, 12000, 15000, 20000]
            }
        }
    });
    assert!(isolapurr_power_config_has_thermal_capability(
        &config,
        THERMAL_SOURCE_65W_POWER_WATTS,
        false,
    ));
    assert!(!isolapurr_power_config_has_thermal_capability(
        &config,
        THERMAL_SOURCE_100W_POWER_WATTS,
        true,
    ));

    let five_amp_config = json!({
        "capability": {
            "powerWatts": 100,
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixedVoltagesMv": [9000, 12000, 15000, 20000, 21000],
                "pps3LimitMa": 5000,
                "pdPps5a": true,
            }
        }
    });
    assert!(isolapurr_power_config_has_thermal_capability(
        &five_amp_config,
        THERMAL_SOURCE_100W_POWER_WATTS,
        true,
    ));
    let released_five_amp_config = json!({
        "capability": {
            "power_watts": 100,
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixed_voltages_mv": [9000, 12000, 15000, 20000]
            },
            "current": {
                "pps3_limit_ma": 5000,
                "pd_pps_5a": true
            }
        }
    });
    assert!(isolapurr_power_config_has_thermal_capability(
        &released_five_amp_config,
        THERMAL_SOURCE_100W_POWER_WATTS,
        true,
    ));

    let ready = BenchSourceLiveTelemetry {
        voltage_mv: 12_034,
        current_ma: 119,
        power_mw: 1_431,
        sample_uptime_ms: 136_324,
        status: "ok".into(),
    };
    assert!(validate_isolapurr_ready_voltage(&ready).is_ok());

    let undervoltage = BenchSourceLiveTelemetry {
        voltage_mv: 5_000,
        ..ready.clone()
    };
    assert!(validate_isolapurr_ready_voltage(&undervoltage).is_err());

    let disconnected = BenchSourceLiveTelemetry {
        status: "not_inserted".into(),
        ..ready
    };
    assert!(validate_isolapurr_ready_voltage(&disconnected).is_err());
}

#[test]
fn isolapurr_runtime_output_readback_and_usb_c_off_are_detected() {
    let disabled = json!({
        "config": {
            "runtime": {
                "output_enabled": false,
            },
        },
        "diagnostics": {
            "usb_c_actual": {
                "status": "ok",
                "current_ma": 0,
                "power_mw": 0,
                "voltage_mv": 2230,
            },
        },
    });
    assert_eq!(isolapurr_runtime_output_enabled(&disabled), Some(false));
    assert!(isolapurr_usb_c_output_is_off(&disabled));

    let disconnected = json!({
        "config": {
            "runtime": {
                "outputEnabled": false,
            },
        },
        "diagnostics": {
            "usb_c_actual": {
                "status": "error",
                "currentMa": null,
                "powerMw": null,
            },
        },
    });
    assert_eq!(isolapurr_runtime_output_enabled(&disconnected), Some(false));
    assert!(isolapurr_usb_c_output_is_off(&disconnected));

    let still_powered = json!({
        "config": {
            "runtime": {
                "output_enabled": false,
            },
        },
        "diagnostics": {
            "usb_c_actual": {
                "status": "ok",
                "current_ma": 43,
                "power_mw": 520,
            },
        },
    });
    assert!(!isolapurr_usb_c_output_is_off(&still_powered));
}

#[test]
fn isolapurr_runtime_output_recovery_requires_ready_advancing_telemetry() {
    let ready = json!({
        "config": {
            "runtime": {
                "output_enabled": true,
            },
        },
        "ports": [
            {
                "portId": "port_c",
                "telemetry": {
                    "status": "ok",
                    "voltage_mv": 12050,
                    "current_ma": 42,
                    "power_mw": 509,
                    "sample_uptime_ms": 100,
                },
            },
        ],
    });
    let first_ready = isolapurr_runtime_output_ready_telemetry(&ready, None).unwrap();
    assert_eq!(first_ready.sample_uptime_ms, 100);
    assert!(
        isolapurr_runtime_output_ready_telemetry(&ready, Some(100))
            .unwrap_err()
            .contains("did not advance")
    );

    let advanced = json!({
        "config": {
            "runtime": {
                "output_enabled": true,
            },
        },
        "ports": [
            {
                "portId": "port_c",
                "telemetry": {
                    "status": "ok",
                    "voltage_mv": 12050,
                    "current_ma": 43,
                    "power_mw": 518,
                    "sample_uptime_ms": 125,
                },
            },
        ],
    });
    assert!(isolapurr_runtime_output_ready_telemetry(&advanced, Some(100)).is_ok());

    let disabled = json!({
        "config": {
            "runtime": {
                "output_enabled": false,
            },
        },
        "ports": [
            {
                "portId": "port_c",
                "telemetry": {
                    "status": "ok",
                    "voltage_mv": 12050,
                    "current_ma": 42,
                    "power_mw": 509,
                    "sample_uptime_ms": 101,
                },
            },
        ],
    });
    assert!(
        isolapurr_runtime_output_ready_telemetry(&disabled, None)
            .unwrap_err()
            .contains("readback is not enabled")
    );

    let undervoltage = json!({
        "config": {
            "runtime": {
                "output_enabled": true,
            },
        },
        "ports": [
            {
                "portId": "port_c",
                "telemetry": {
                    "status": "ok",
                    "voltage_mv": 5000,
                    "current_ma": 0,
                    "power_mw": 0,
                    "sample_uptime_ms": 102,
                },
            },
        ],
    });
    assert!(
        isolapurr_runtime_output_ready_telemetry(&undervoltage, None)
            .unwrap_err()
            .contains("not above 5V")
    );
}

#[test]
fn source_stale_error_classification_is_narrow() {
    assert_eq!(
        THERMAL_SOURCE_TELEMETRY_STALE_TIMEOUT,
        Duration::from_secs(6)
    );
    assert!(
        THERMAL_SOURCE_TELEMETRY_STALE_TIMEOUT
            > ISOLAPURR_LIVE_TELEMETRY_TIMEOUT
                .saturating_mul(ISOLAPURR_LIVE_TELEMETRY_ATTEMPTS as u32)
    );

    let stale = io::Error::other("isolapurr USB-C telemetry did not advance for 2100ms");
    assert!(thermal_source_telemetry_stale_error(&stale));
    assert!(thermal_source_probe_transient_error(&stale));

    let source_stale = io::Error::other("source telemetry stale");
    assert!(thermal_source_telemetry_stale_error(&source_stale));
    assert!(thermal_source_probe_transient_error(&source_stale));

    let timeout = io::Error::other("isolapurr power show timed out after 750ms");
    assert!(thermal_source_probe_transient_error(&timeout));

    let not_inserted = io::Error::other(
        "isolapurr USB-C telemetry missing voltage status=not_inserted state=null",
    );
    assert!(thermal_source_probe_transient_error(&not_inserted));

    let missing_object = io::Error::other("isolapurr USB-C telemetry missing object");
    assert!(thermal_source_probe_transient_error(&missing_object));

    let refused = io::Error::other(
        "isolapurr status exited with exit status: 1; stderr=Error: error sending request for url (http://192.168.31.224/api/v1/info)\n\nCaused by:\n    0: client error (Connect)\n    1: tcp connect error\n    2: Connection refused (os error 61)",
    );
    assert!(thermal_source_probe_transient_error(&refused));
    assert!(thermal_retryable_runtime_write_error_message(
        "HTTP 504 Gateway Timeout body={\"error\":{\"code\":\"usb_response_timeout\",\"details\":null,\"message\":\"Timed out waiting for a matching USB JSONL response.\",\"retryable\":true}}"
    ));

    let status_error = io::Error::other("status_request_failed");
    assert!(!thermal_source_telemetry_stale_error(&status_error));
    assert!(!thermal_source_probe_transient_error(&status_error));
    assert!(!thermal_retryable_runtime_write_error_message(
        "status_request_failed"
    ));
}

#[test]
fn preview_activation_retry_classifier_accepts_profile_fallback_readback_errors() {
    assert!(thermal_preview_activation_retryable_error_message(
        "thermal profile mode readback mismatch: expected 100w, got 65w"
    ));
    assert!(thermal_preview_activation_retryable_error_message(
        "thermal control profile does not cover the requested target"
    ));
    assert!(!thermal_preview_activation_retryable_error_message(
        "heater runtime readback target mismatch: expected 220, got 140"
    ));
}

#[test]
fn isolapurr_configured_thermal_source_class_detects_3a_and_5a_modes() {
    let three_amp_config = json!({
        "capability": {
            "power_watts": 65,
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixed_voltages_mv": [9000, 12000, 15000, 20000]
            },
            "current": {
                "pps3_limit_ma": 3250
            }
        }
    });
    assert_eq!(
        isolapurr_configured_thermal_source_class(&three_amp_config),
        Some("pps3a")
    );

    let five_amp_config = json!({
        "capability": {
            "power_watts": 100,
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixed_voltages_mv": [9000, 12000, 15000, 20000]
            },
            "current": {
                "pps3_limit_ma": 5000
            }
        }
    });
    assert_eq!(
        isolapurr_configured_thermal_source_class(&five_amp_config),
        Some("pps5a")
    );

    let missing_20v = json!({
        "capability": {
            "protocols": { "pd": true },
            "pd": {
                "pps": true,
                "fixed_voltages_mv": [9000, 12000, 15000]
            },
            "current": {
                "pps3_limit_ma": 5000
            }
        }
    });
    assert_eq!(
        isolapurr_configured_thermal_source_class(&missing_20v),
        None
    );
}

#[test]
fn isolapurr_write_response_requires_positive_acknowledgement() {
    assert!(isolapurr_cli_write_succeeded(&json!({"accepted": true})));
    assert!(isolapurr_cli_write_succeeded(&json!({"ok": true})));
    assert!(!isolapurr_cli_write_succeeded(&json!({})));
    assert!(!isolapurr_cli_write_succeeded(&json!({"accepted": false})));
}

#[test]
fn isolapurr_status_identity_must_match_requested_device() {
    let status = json!({
        "device": {
            "device_id": "f293cc9c139e",
            "firmware": {
                "name": "isolapurr-usb-hub",
                "version": "0.5.1"
            }
        }
    });

    assert!(isolapurr_status_identity_matches(&status, "f293cc9c139e"));
    assert!(!isolapurr_status_identity_matches(&status, "856a141cdbd4"));

    let wrapped_status = json!({
        "ok": true,
        "result": {
            "device": {
                "device_id": "f293cc9c139e"
            }
        }
    });
    assert!(isolapurr_status_identity_matches(
        &wrapped_status,
        "f293cc9c139e"
    ));
    assert!(!isolapurr_status_identity_matches(
        &wrapped_status,
        "856a141cdbd4"
    ));
}

#[test]
fn parses_thermal_self_test_command() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "--devd",
        DEFAULT_DEVD_ENDPOINT,
        "thermal",
        "self-test",
        "--device",
        "bench",
        "--source-device-id",
        "iso-1",
        "--source-url",
        "http://192.168.31.122",
        "--dry-run",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Thermal {
            command: ThermalCommand::SelfTest(args),
        } => {
            assert_eq!(args.target.device.as_deref(), Some("bench"));
            assert_eq!(args.source_kind, BenchSourceKind::Isolapurr);
            assert_eq!(args.source_id, "iso-1");
            assert_eq!(args.source_url, "http://192.168.31.122");
            assert_eq!(args.source_mode, "auto-follow");
            assert!(args.dry_run);
        }
        other => panic!("unexpected command parsed: {other:?}"),
    }
}

#[test]
fn calibration_slot_commands_match_the_persisted_slot_contract() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "calibration",
        "set-slot-fit",
        "--device",
        "bench",
        "--channel",
        "vin-adc",
        "--slot",
        "b",
        "--gain",
        "0.9723",
        "--offset-mv",
        "126.4",
    ])
    .unwrap();

    match cli.command {
        Command::Calibration {
            command: CalibrationCommand::SetSlotFit(args),
        } => {
            assert_eq!(args.target.device.as_deref(), Some("bench"));
            assert_eq!(args.channel, "vin-adc");
            assert_eq!(args.slot, "b");
            assert!((args.gain - 0.9723).abs() < f32::EPSILON);
            assert!((args.offset_mv - 126.4).abs() < f32::EPSILON);
        }
        other => panic!("unexpected command parsed: {other:?}"),
    }

    let fit_body = calibration_set_slot_fit_body("vin-adc", "B", 0.9723, 126.4).unwrap();
    assert_eq!(fit_body["op"], "set_slot_fit");
    assert_eq!(fit_body["channel"], "vin_adc");
    assert_eq!(fit_body["slot"], "b");
    assert!((fit_body["fit"]["gain"].as_f64().unwrap() - 0.9723).abs() < 0.000_001);
    assert!((fit_body["fit"]["offsetMv"].as_f64().unwrap() - 126.4).abs() < 0.000_01);
    assert_eq!(
        calibration_set_active_slot_body("vin", "b").unwrap(),
        json!({
            "op": "set_active_slot",
            "channel": "vin_adc",
            "slot": "b",
        })
    );
    assert!(calibration_set_slot_fit_body("vin", "b", 0.0, 1.0).is_err());
    assert!(calibration_set_slot_fit_body("vin", "c", 1.0, 0.0).is_err());
    assert!(calibration_set_slot_fit_body("vin", "b", 1.0, f32::NAN).is_err());
}

#[test]
fn parses_pps_volts_as_100mv_steps() {
    assert_eq!(parse_pps_volts("10.4").unwrap(), 10_400);
    assert_eq!(parse_pps_volts("21").unwrap(), 21_000);
    assert_eq!(parse_pps_volts("28").unwrap(), 28_000);
    assert!(parse_pps_volts("10.45").is_err());
    assert!(parse_pps_volts("4.9").is_err());
    assert!(parse_pps_volts("28.1").is_err());
}

#[test]
fn parses_pps_amps_as_50ma_steps() {
    assert_eq!(parse_pps_amps("2.5").unwrap(), 2_500);
    assert_eq!(parse_pps_amps("3.00").unwrap(), 3_000);
    assert!(parse_pps_amps("2.53").is_err());
    assert!(parse_pps_amps("0").is_err());
}

#[test]
fn thermal_profile_modes_preserve_65w_and_define_100w_defaults() {
    assert_eq!(
        ThermalProfileMode::W65.explicit_source_defaults(),
        Some((20_000, 3_250))
    );
    assert_eq!(
        ThermalProfileMode::W100.explicit_source_defaults(),
        Some((21_000, 5_000))
    );
    assert_eq!(ThermalProfileMode::Auto.explicit_bank(), None);
    assert_eq!(ThermalProfileMode::W100.explicit_bank(), Some("pps5a"));
}

#[test]
fn thermal_default_seed_candidates_follow_bank_current_truth() {
    let (_, pps3a_seed_path) = load_thermal_default_seed_candidate_profile("pps3a").unwrap();
    let (_, pps5a_seed_path) = load_thermal_default_seed_candidate_profile("pps5a").unwrap();

    assert!(
        pps3a_seed_path
            .as_ref()
            .is_some_and(|path| path.ends_with(THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE))
    );
    assert!(pps5a_seed_path.as_ref().is_some_and(|path| {
        path.ends_with(THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE)
            || path.ends_with(THERMAL_PPS5A_TUNING_SEED_RELATIVE)
    }));
}

#[test]
fn thermal_profile_preview_body_preserves_selected_mode() {
    let body = thermal_profile_preview_runtime_body(
        ThermalProfileMode::W100,
        json!({"points": [], "settings": {}}),
    );

    assert_eq!(body["thermalProfileMode"], "100w");
    assert_eq!(body["thermalControlProfile"]["op"], "preview");
    assert!(body["thermalControlProfile"]["profile"].is_object());
}

#[test]
fn thermal_target_scoped_preview_is_complete_and_fits_the_usb_line() {
    let profile = default_thermal_candidate_profile();
    let expected = thermal_heater_parameters_value(140, Some(&profile), "preview");
    let scoped = thermal_target_scoped_preview_profile_value(&profile, 140);
    let points = scoped["points"].as_array().expect("preview points");
    let non_null_points = points
        .iter()
        .filter(|point| !point.is_null())
        .collect::<Vec<_>>();

    assert_eq!(points.len(), THERMAL_CONTROL_PROFILE_MAX_POINTS);
    assert_eq!(non_null_points.len(), 1);
    assert_eq!(scoped["settings"], expected["settings"]);
    assert_eq!(non_null_points[0]["targetTempC"], 140);
    for field in [
        "warmupPowerPermille",
        "brakeDistanceCentiC",
        "approachPowerPermille",
        "approachFloorPowerPermille",
        "approachDampingExponentPermille",
        "approachTailWindowCentiC",
        "holdPowerPermille",
        "holdReheatPowerPermille",
        "warmupReenterCentiC",
        "holdEntryCentiC",
        "holdExitCentiC",
        "holdOnCentiC",
        "holdOffCentiC",
        "overshootCutoffCentiC",
        "holdKpPermillePerC",
        "holdKiPermillePerCTick",
        "holdBlendTicks",
        "approachLeadTicks",
        "holdLeadTicks",
    ] {
        assert_eq!(non_null_points[0][field], expected[field], "{field}");
    }

    let body = thermal_profile_preview_runtime_body(ThermalProfileMode::W100, scoped);
    assert!(
        serde_json::to_vec(&body)
            .expect("preview body serialization")
            .len()
            < 4_096,
        "target-scoped preview must fit inside the firmware USB JSONL limit"
    );
}

#[test]
fn thermal_nine_point_profile_save_fits_the_usb_line() {
    let sparse_profile = thermal_candidate_profile_from_value(default_thermal_candidate_profile());
    let targets = [60, 80, 100, 120, 140, 160, 180, 220, 240];
    let points = targets
        .into_iter()
        .map(|target_temp_c| {
            thermal_interpolated_candidate_point(&sparse_profile, target_temp_c)
                .expect("full-batch target must materialize from the seed")
        })
        .collect();
    let candidate = ThermalCandidateProfile {
        settings: sparse_profile.settings,
        points,
    };
    let profile = thermal_candidate_profile_to_value(
        &thermal_profile_for_persistence(&candidate).expect("nine points fit firmware"),
    );
    let body = thermal_profile_preview_runtime_body(ThermalProfileMode::W100, profile);
    let serialized = serde_json::to_vec(&body).expect("save body serialization");

    assert!(serialized.len() > 4_096);
    assert!(serialized.len() < 8 * 1024);
}

#[test]
fn thermal_profile_preview_readback_requires_the_requested_bank() {
    let matched = json!({
        "thermalProfileMode": "100w",
        "thermalProfileResolvedBank": "pps5a",
    });
    assert!(verify_thermal_profile_mode_readback(&matched, ThermalProfileMode::W100).is_ok());

    let wrong_bank = json!({
        "thermalProfileMode": "100w",
        "thermalProfileResolvedBank": "pps3a",
    });
    assert!(verify_thermal_profile_mode_readback(&wrong_bank, ThermalProfileMode::W100).is_err());
}

#[test]
fn thermal_source_class_uses_the_configured_capability_not_selected_mode() {
    assert_eq!(thermal_source_class(20_000, 3_250), "pps3a");
    assert_eq!(thermal_source_class(21_000, 5_000), "pps5a");
}

#[test]
fn pps3a_self_test_never_activates_point_local_preview() {
    let pps3a = ThermalSourceSelection {
        resolved_bank: "pps3a",
        detected_source_class: "pps3a",
        detected_source_class_basis: "configured_capability",
        default_voltage_mv: 20_000,
        default_current_ma: 3_250,
    };
    let pps5a = ThermalSourceSelection {
        resolved_bank: "pps5a",
        detected_source_class: "pps5a",
        detected_source_class_basis: "configured_capability",
        default_voltage_mv: 21_000,
        default_current_ma: 5_000,
    };

    assert!(!thermal_self_test_uses_point_local_profile(&pps3a, false));
    assert!(!thermal_self_test_uses_point_local_profile(&pps3a, true));
    assert!(thermal_self_test_uses_point_local_profile(&pps5a, false));
    assert!(!thermal_self_test_uses_point_local_profile(&pps5a, true));
}

#[test]
fn calibration_heater_commands_accept_explicit_boolean_values() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "--devd",
        DEFAULT_DEVD_ENDPOINT,
        "calibration-mode",
        "temperature",
        "heater",
        "--enabled",
        "false",
        "--device",
        "bench",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::CalibrationMode {
            command:
                CalibrationModeCommand::Temperature {
                    command: TemperatureCalibrationCommand::Heater(args),
                },
        } => assert!(!args.enabled),
        other => panic!("unexpected command parsed: {other:?}"),
    }
}

#[test]
fn parses_single_artifact_manifest() {
    let artifact = FirmwareArtifact {
        artifact_id: "a".to_string(),
        name: "A".to_string(),
        version: "v".to_string(),
        git_sha: "sha".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: Vec::new(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("artifact.json");
    fs::write(&path, serde_json::to_vec(&artifact).unwrap()).unwrap();
    let artifacts = read_artifact_manifest(&path).unwrap();
    assert_eq!(artifacts[0].artifact_id, "a");
}

fn write_retune_fixture(run_dir: &Path) {
    fs::create_dir_all(run_dir).unwrap();
    let samples_path = run_dir.join("samples.ndjson");
    let profile = default_thermal_candidate_profile();
    let mut samples_writer = BufWriter::new(File::create(&samples_path).unwrap());
    let mut sample_index = 0usize;
    let applied = write_dry_thermal_ladder(DryThermalLadderInput {
        samples_writer: &mut samples_writer,
        run_id: "thermal-fixture",
        test_phase: "applied",
        source_voltage_mv: 20_000,
        source_current_ma: 3_250,
        thermal_profile: Some(&profile),
        heater_parameter_mode: "preview",
        target_temps_c: &[60],
        sample_index: &mut sample_index,
    })
    .unwrap();
    samples_writer.flush().unwrap();
    let summary = json!({
        "kind": "thermal_self_test",
        "ok": true,
        "runId": "thermal-fixture",
        "dryRun": true,
        "target": {
            "deviceId": "bench",
            "hardwareId": Value::Null,
            "devd": DEFAULT_DEVD_ENDPOINT,
        },
        "source": {
            "deviceId": "iso-fixture",
            "mode": "dry_run",
            "url": "http://127.0.0.1:1",
        },
        "parameters": {
            "targetsC": [60],
            "optimizeTargetsC": [],
            "sampleIntervalMs": 300,
            "effectiveSampleIntervalMs": 300,
            "holdSeconds": 60,
            "stageTimeoutSeconds": 300,
            "runtimeRearmAttempts": 1,
            "cooldownTempC": 40.0,
            "cooldownTimeoutSeconds": 7200,
            "limits": {
                "overshootC": 3.0,
                "holdPeakToPeakC": 3.0,
                "fullSpeedToStableMs": {
                    "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                    "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
                },
            },
            "seedProfileFile": Value::Null,
        },
        "selectedMode": "100w",
        "files": {
            "runDir": run_dir,
            "summaryPath": run_dir.join("run.json"),
            "samplesPath": samples_path,
            "candidateProfilePath": run_dir.join("thermal-profile.candidate.json"),
        },
        "candidateProfile": profile,
        "profilePersistence": "dry_run",
        "tuningSteps": [],
        "applied": applied.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validate_thermal_applied_results(
            &applied,
            &[60],
            ThermalSelfTestEvaluationMode::HoldConfirm,
        ),
        "sampleCount": sample_index,
        "complete": true,
        "error": Value::Null,
    });
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

#[test]
fn thermal_retune_offline_writes_replay_artifacts_without_apply_receipt() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());

    let output = thermal_retune::retune_thermal_self_test_run(thermal_retune::ThermalRetuneInput {
        run_dir: dir.path().to_path_buf(),
        optimize_targets_c: None,
    })
    .unwrap();

    assert_eq!(
        output.summary["kind"].as_str(),
        Some("thermal_self_test_replay")
    );
    assert_eq!(
        output.summary["parameters"]["evaluationMode"],
        json!("hold-confirm")
    );
    assert!(output.summary["applied"][0]["analysis"]["approachSource"].is_object());
    assert!(output.summary["applied"][0]["analysis"]["holdSource"].is_object());
    assert!(output.summary.get("applyPreview").is_none());
    assert!(dir.path().join("run.replayed.json").exists());
    assert!(
        dir.path()
            .join("thermal-profile.replayed.candidate.json")
            .exists()
    );
}

fn thermal_report_test_point(target_temp_c: i16) -> Value {
    json!({
        "targetTempC": target_temp_c,
        "brakeDistanceCentiC": 1100,
        "warmupPowerPermille": 1000,
        "approachPowerPermille": 420,
        "approachFloorPowerPermille": 320,
        "approachDampingExponentPermille": 910,
        "approachTailWindowCentiC": 0,
        "holdPowerPermille": 335,
        "holdReheatPowerPermille": 400,
        "holdEntryCentiC": 150,
        "holdExitCentiC": 100,
        "holdOnCentiC": 10,
        "holdOffCentiC": 160,
        "overshootCutoffCentiC": 220,
        "holdKpPermillePerC": 22,
        "holdKiPermillePerCTick": 1,
        "holdBlendTicks": 1,
        "approachLeadTicks": 2,
        "holdLeadTicks": 0
    })
}

#[test]
fn thermal_report_rerender_preliminary_bundle_writes_compliant_artifacts() {
    let (_dir, legacy_dir, output_dir) =
        thermal_report_rerender_preliminary_bundle_writes_compliant_artifacts_fixture();
    let result = thermal_report::rerender_legacy_preliminary_review_bundle(
        thermal_report::ThermalLegacyReportInput {
            legacy_bundle_dir: legacy_dir.clone(),
            output_dir: Some(output_dir.clone()),
        },
    )
    .unwrap();
    let bundle: Value =
        serde_json::from_slice(&fs::read(output_dir.join("run.bundle.json")).unwrap()).unwrap();
    let html = fs::read_to_string(output_dir.join("index.html")).unwrap();

    assert_eq!(result["ok"], true);
    assert_eq!(
        result["operation"],
        "thermal_report.rerender_legacy_preliminary_review_bundle"
    );
    assert_eq!(bundle["kind"], "thermal_self_test_preliminary_bundle");
    assert_eq!(bundle["bundleDisposition"], "preliminary_review");
    assert_eq!(bundle["acceptedProfileRole"], "review_candidate_snapshot");
    assert_eq!(bundle["tuningTargetsC"], json!([60]));
    assert_eq!(bundle["runs"][0]["target"], 60);
    assert_eq!(
        bundle["runs"][0]["pointSource"],
        "review_candidate_snapshot"
    );
    assert_eq!(
        bundle["runs"][0]["rounds"][0]["attemptType"],
        "characterization"
    );
    assert!(html.contains("60°C"));
    assert!(html.contains("preliminary review"));
}

fn thermal_report_rerender_preliminary_bundle_writes_compliant_artifacts_fixture()
-> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let legacy_dir = dir.path().join("legacy");
    let output_dir = dir.path().join("rerendered");
    fs::create_dir_all(&legacy_dir).unwrap();

    let legacy_bundle = thermal_report_preliminary_legacy_bundle();
    fs::write(
        legacy_dir.join("run.bundle.json"),
        serde_json::to_vec_pretty(&legacy_bundle).unwrap(),
    )
    .unwrap();
    fs::write(
        legacy_dir.join("thermal-profile.accepted.json"),
        serde_json::to_vec_pretty(&json!({
            "points": [thermal_report_test_point(60)],
            "settings": {}
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        legacy_dir.join("samples.ndjson"),
        format!(
            "{}\n",
            serde_json::to_string(&json!({
                "targetTempC": 60,
                "elapsedMs": 1000,
                "status": {
                    "currentTempC": 60.2,
                    "heaterFilteredTempC": 60.1,
                    "heaterOutputPercent": 18,
                    "heaterPhysicalOutputPercent": 18,
                    "pdRequestMv": 21000
                },
                "phase": "hold",
                "sourceTelemetry": {
                    "voltageMv": 21000,
                    "currentMa": 300,
                    "powerMw": 6300
                }
            }))
            .unwrap()
        ),
    )
    .unwrap();
    (dir, legacy_dir, output_dir)
}

fn thermal_report_preliminary_legacy_bundle() -> Value {
    let legacy_bundle = json!({
        "kind": "thermal_approach_characterization",
        "runId": "legacy-run",
        "generatedAt": "2026-07-17T12:00:00Z",
        "selectedMode": "100w",
        "resolvedBank": "pps5a",
        "detectedSourceClass": "pps5a",
        "bundleDisposition": "preliminary_review",
        "acceptedProfileRole": "review_candidate_snapshot",
        "source": {
            "sourceDeviceId": "f293cc9c139e"
        },
        "targets": [
            {
                "targetTempC": 60,
                "effectivePoint": thermal_report_test_point(60),
                "variants": [
                    {
                        "variantId": "zero_coast",
                        "variantLabel": "0加热",
                        "valid": true,
                        "tunedPoint": thermal_report_test_point(60),
                        "metrics": {
                            "approachDurationMs": 8200,
                            "peak": 0.75,
                            "rollback": 1.71
                        },
                        "samples": [
                            {
                                "elapsedMs": 0,
                                "currentTempC": 35.0,
                                "heaterFilteredTempC": 35.0,
                                "heaterControlPhase": "warmup",
                                "heaterOutputPercent": 100,
                                "heaterPhysicalOutputPercent": 100,
                                "sourceTelemetry": {
                                    "voltageMv": 21000,
                                    "currentMa": 4800,
                                    "powerMw": 100800
                                }
                            }
                        ]
                    }
                ],
                "holdCheck": {
                    "confirmRunId": "confirm-60",
                    "passed": true,
                    "failureReason": Value::Null,
                    "holdSeconds": 60,
                    "maxOvershootC": 0.75,
                    "holdPeakToPeakC": 1.71,
                    "holdMedianOutputPermille": 0,
                    "holdP90OutputPermille": 100,
                    "approachSource": {"powerMw": {"avg": 22425.0}},
                    "holdSource": {"powerMw": {"avg": 3935.0}},
                    "stopReason": "completed"
                }
            }
        ]
    });
    legacy_bundle
}

#[test]
fn thermal_report_rerender_preserves_nine_point_pps5a_plant_evidence() {
    let (_dir, legacy_dir, output_dir, targets) =
        thermal_report_rerender_preserves_nine_point_pps5a_plant_evidence_fixture();
    thermal_report::rerender_legacy_preliminary_review_bundle(
        thermal_report::ThermalLegacyReportInput {
            legacy_bundle_dir: legacy_dir,
            output_dir: Some(output_dir.clone()),
        },
    )
    .unwrap();

    let bundle: Value =
        serde_json::from_slice(&fs::read(output_dir.join("run.bundle.json")).unwrap()).unwrap();
    let written_samples = fs::read_to_string(output_dir.join("samples.ndjson")).unwrap();
    let html = fs::read_to_string(output_dir.join("index.html")).unwrap();

    assert_eq!(bundle["selectedMode"], "100w");
    assert_eq!(bundle["resolvedBank"], "pps5a");
    assert_eq!(bundle["detectedSourceClass"], "pps5a");
    assert_eq!(bundle["sourceDeviceId"], "f293cc9c139e");
    assert_eq!(bundle["tuningTargetsC"], json!(targets));
    assert_eq!(
        bundle["reportRuns"].as_array().unwrap().len(),
        targets.len()
    );
    assert!(
        bundle["reportRuns"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["reviewPassed"] == Value::Bool(true))
    );
    assert_eq!(written_samples.lines().count(), targets.len());
    for target_temp_c in targets {
        assert!(html.contains(&format!("{target_temp_c}°C")));
    }
}

fn thermal_report_rerender_preserves_nine_point_pps5a_plant_evidence_fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    [i16; 9],
) {
    let dir = tempfile::tempdir().unwrap();
    let legacy_dir = dir.path().join("plant-hil");
    let output_dir = dir.path().join("plant-hil-rerendered");
    fs::create_dir_all(&legacy_dir).unwrap();

    let targets = [60, 80, 100, 120, 140, 160, 180, 220, 240];
    let applied = targets
        .iter()
        .map(|target_temp_c| {
            let limit_ms = if *target_temp_c <= 150 { 10_000 } else { 5_000 };
            json!({
                "targetTempC": target_temp_c,
                "stopReason": "completed",
                "maxOvershootC": 1.0,
                "holdPeakToPeakC": 1.2,
                "analysis": {
                    "holdMedianOutputPermille": 150,
                    "holdP90OutputPermille": 190,
                    "approachSource": {"powerMw": {"avg": 80_000.0}},
                    "holdSource": {"powerMw": {"avg": 14_000.0}}
                },
                "fullSpeedToStable": {
                    "limitMs": limit_ms,
                    "settleTimeMs": 2_000,
                    "failureReason": Value::Null
                },
                "guard": {"firstHoldAtMs": 2_000}
            })
        })
        .collect::<Vec<_>>();
    let samples = targets
        .iter()
        .map(|target_temp_c| {
            json!({
                "targetTempC": target_temp_c,
                "elapsedMs": 2_000,
                "phase": "hold",
                "heaterParameters": thermal_report_test_point(*target_temp_c),
                "status": {
                    "currentTempC": *target_temp_c as f32,
                    "heaterFilteredTempC": *target_temp_c as f32,
                    "heaterOutputPercent": 15,
                    "heaterPhysicalOutputPercent": 15,
                    "pdRequestMv": 21_000
                },
                "sourceTelemetry": {
                    "voltageMv": 21_000,
                    "currentMa": 700,
                    "powerMw": 14_700
                }
            })
        })
        .collect::<Vec<_>>();
    let legacy_bundle = json!({
        "kind": "thermal_self_test_report_bundle",
        "runId": "pps5a-plant-hil",
        "generatedAt": "2026-07-28T12:00:00Z",
        "selectedMode": "100w",
        "resolvedBank": "pps5a",
        "detectedSourceClass": "pps5a",
        "bundleDisposition": "latest_live_report",
        "acceptedProfileRole": "active_thermal_plant",
        "source": {"deviceId": "f293cc9c139e"},
        "target": {"deviceId": "serial-303a-1001-A0:F2:62:F2:0D:6C"},
        "parameters": {"holdSeconds": 60},
        "candidateProfile": {
            "settings": {},
            "points": targets.iter().map(|target_temp_c| thermal_report_test_point(*target_temp_c)).collect::<Vec<_>>()
        },
        "applied": applied,
        "validation": {
            "passed": true,
            "expectedTargetsC": targets,
            "failures": []
        }
    });
    fs::write(
        legacy_dir.join("run.bundle.json"),
        serde_json::to_vec_pretty(&legacy_bundle).unwrap(),
    )
    .unwrap();
    fs::write(
        legacy_dir.join("thermal-profile.accepted.json"),
        serde_json::to_vec_pretty(&legacy_bundle["candidateProfile"]).unwrap(),
    )
    .unwrap();
    fs::write(
        legacy_dir.join("samples.ndjson"),
        samples
            .iter()
            .map(|sample| serde_json::to_string(sample).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    (dir, legacy_dir, output_dir, targets)
}

#[test]
fn thermal_report_rerender_live_bundle_splits_time_reset_attempts() {
    let (dir, legacy_dir) =
        thermal_report_rerender_live_bundle_splits_time_reset_attempts_fixture();
    let result = thermal_report::rerender_legacy_preliminary_review_bundle(
        thermal_report::ThermalLegacyReportInput {
            legacy_bundle_dir: legacy_dir.clone(),
            output_dir: None,
        },
    )
    .unwrap();
    let output_dir = dir.path().join("legacy-live-rerendered");
    let bundle: Value =
        serde_json::from_slice(&fs::read(output_dir.join("run.bundle.json")).unwrap()).unwrap();
    let written_samples: Vec<Value> = fs::read_to_string(output_dir.join("samples.ndjson"))
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(result["ok"], true);
    assert_eq!(bundle["kind"], "thermal_self_test_preliminary_bundle");
    assert_eq!(bundle["tuningTargetsC"], json!([60]));
    assert_eq!(bundle["runs"][0]["roundCount"], 2);
    assert_eq!(
        bundle["runs"][0]["rounds"][0]["attemptType"],
        "legacy_live_report"
    );
    assert_eq!(bundle["runs"][0]["rounds"][0]["selected"], false);
    assert_eq!(bundle["runs"][0]["rounds"][1]["selected"], true);
    assert_eq!(
        bundle["runs"][0]["holdCheck"]["confirmRunId"],
        "legacy-live-run-60"
    );
    assert_eq!(bundle["runs"][0]["samples"][0]["t"], 1.0);
    assert_eq!(bundle["runs"][0]["rounds"][0]["samples"][0]["t"], 5.0);
    assert_eq!(bundle["runs"][0]["rounds"][1]["samples"][0]["t"], 1.0);
    assert_eq!(written_samples[0]["t"], 5.0);
    assert_eq!(written_samples[1]["t"], 1.0);
    assert!(
        result["outputDir"]
            .as_str()
            .unwrap()
            .ends_with("legacy-live-rerendered")
    );
}

#[derive(Clone)]
struct RetuneApplyTestState {
    runtime_requests: Arc<Mutex<Vec<Value>>>,
    preview_enabled_readback: bool,
    profile_covers_target_readback: bool,
}

async fn create_retune_test_lease() -> Json<Value> {
    Json(json!({
        "leaseId": "lease-retune",
        "ttlMs": 60_000,
    }))
}

async fn heartbeat_retune_test_lease() -> Json<Value> {
    Json(json!({
        "leaseId": "lease-retune",
        "ttlMs": 60_000,
    }))
}

async fn release_retune_test_lease() -> Json<Value> {
    Json(json!({ "released": true }))
}

async fn capture_retune_preview(
    State(state): State<RetuneApplyTestState>,
    AxumPath(_device_id): AxumPath<String>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    state.runtime_requests.lock().unwrap().push(payload.clone());
    Json(json!({
        "deviceId": "bench",
        "mode": "idle",
        "targetTempC": 60,
        "currentTempC": 25.0,
        "heaterEnabled": false,
        "thermalControlProfilePreview": true,
    }))
}

async fn retune_status_readback(
    State(state): State<RetuneApplyTestState>,
    AxumPath(_device_id): AxumPath<String>,
) -> Json<Value> {
    let request = state.runtime_requests.lock().unwrap().last().cloned();
    let profile = request
        .as_ref()
        .and_then(|value| value.pointer("/thermalControlProfile/profile"))
        .cloned()
        .unwrap_or(Value::Null);
    let expected = thermal_heater_parameters_value(60, Some(&profile), "preview");
    let mut thermal_control = expected.as_object().cloned().unwrap_or_default();
    if let Some(settings) = thermal_control
        .remove("settings")
        .and_then(|value| value.as_object().cloned())
    {
        thermal_control.extend(settings);
    }
    thermal_control.insert(
        "profileActive".to_string(),
        json!(state.preview_enabled_readback),
    );
    thermal_control.insert(
        "profileCoversTarget".to_string(),
        json!(state.profile_covers_target_readback),
    );
    thermal_control.insert(
        "profileSource".to_string(),
        json!(if state.preview_enabled_readback {
            "preview"
        } else {
            "default"
        }),
    );
    Json(json!({
        "deviceId": "bench",
        "mode": "idle",
        "targetTempC": 60,
        "currentTempC": 25.0,
        "heaterEnabled": false,
        "thermalControlProfilePreview": state.preview_enabled_readback,
        "thermalProfileMode": "100w",
        "thermalProfileResolvedBank": "pps5a",
        "thermalControl": thermal_control,
    }))
}

async fn failing_retune_preview(
    State(state): State<RetuneApplyTestState>,
    AxumPath(_device_id): AxumPath<String>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.runtime_requests.lock().unwrap().push(payload);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "preview failed"})),
    )
}

async fn spawn_retune_apply_server(
    preview_enabled_readback: bool,
    profile_covers_target_readback: bool,
    fail_preview: bool,
) -> (String, Arc<Mutex<Vec<Value>>>, tokio::task::JoinHandle<()>) {
    let runtime_requests = Arc::new(Mutex::new(Vec::new()));
    let state = RetuneApplyTestState {
        runtime_requests: runtime_requests.clone(),
        preview_enabled_readback,
        profile_covers_target_readback,
    };
    let runtime_route = if fail_preview {
        put(failing_retune_preview)
    } else {
        put(capture_retune_preview)
    };
    let app = Router::new()
        .route(
            "/api/v1/devices/{device_id}/leases",
            post(create_retune_test_lease),
        )
        .route(
            "/api/v1/leases/{lease_id}/heartbeat",
            post(heartbeat_retune_test_lease),
        )
        .route(
            "/api/v1/leases/{lease_id}",
            delete(release_retune_test_lease),
        )
        .route("/api/v1/devices/{device_id}/runtime", runtime_route)
        .route(
            "/api/v1/devices/{device_id}/status",
            get(retune_status_readback),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), runtime_requests, server)
}

#[derive(Clone)]
struct ThermalStatusRetryTestState {
    attempts: Arc<AtomicUsize>,
    delayed_attempts: usize,
    delay_ms: u64,
}

async fn flaky_thermal_status_readback(
    State(state): State<ThermalStatusRetryTestState>,
    AxumPath(_device_id): AxumPath<String>,
) -> Json<Value> {
    let attempt = state.attempts.fetch_add(1, Ordering::SeqCst) + 1;
    if attempt <= state.delayed_attempts {
        tokio::time::sleep(Duration::from_millis(state.delay_ms)).await;
    }
    Json(json!({
        "attempt": attempt,
        "currentTempC": 25.0,
        "heaterEnabled": false,
        "thermalControlProfilePreview": false,
    }))
}

async fn thermal_status_retry_ready() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn spawn_flaky_thermal_status_server(
    delayed_attempts: usize,
    delay_ms: u64,
) -> (
    ResolvedUsbTarget,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<()>,
) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let state = ThermalStatusRetryTestState {
        attempts: attempts.clone(),
        delayed_attempts,
        delay_ms,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/ready", get(thermal_status_retry_ready))
        .route(
            "/api/v1/devices/{device_id}/status",
            get(flaky_thermal_status_readback),
        )
        .with_state(state);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        ResolvedUsbTarget {
            device: "bench".to_string(),
            devd: format!("http://{addr}"),
            hardware_id: None,
        },
        attempts,
        server,
    )
}

#[tokio::test]
async fn thermal_status_retry_recovers_after_single_timeout() {
    let (resolved, attempts, server) = spawn_flaky_thermal_status_server(1, 150).await;
    let client = Client::new();
    client
        .get(format!("{}/ready", resolved.devd))
        .send()
        .await
        .unwrap();

    let status = request_thermal_status_with_retry_config(
        &client,
        &resolved,
        "lease-test",
        Duration::from_millis(50),
        2,
    )
    .await
    .unwrap();

    assert_eq!(status["attempt"], 2);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    server.abort();
}

#[tokio::test]
async fn thermal_status_retry_recovers_after_transient_usb_burst() {
    let (resolved, attempts, server) = spawn_flaky_thermal_status_server(4, 150).await;
    let client = Client::new();
    client
        .get(format!("{}/ready", resolved.devd))
        .send()
        .await
        .unwrap();

    let status = request_thermal_status_with_retry_config(
        &client,
        &resolved,
        "lease-test",
        Duration::from_millis(50),
        5,
    )
    .await
    .unwrap();

    assert_eq!(status["attempt"], 5);
    assert_eq!(attempts.load(Ordering::SeqCst), 5);

    server.abort();
}

#[derive(Clone)]
struct ThermalRuntimeReadbackTestState {
    attempts: Arc<AtomicUsize>,
}

async fn delayed_thermal_runtime_readback(
    State(state): State<ThermalRuntimeReadbackTestState>,
    AxumPath(_device_id): AxumPath<String>,
) -> Json<Value> {
    let attempt = state.attempts.fetch_add(1, Ordering::SeqCst) + 1;
    Json(json!({
        "targetTempC": 140,
        "heaterEnabled": attempt >= 3,
        "activeCoolingEnabled": true,
    }))
}

#[tokio::test]
async fn thermal_runtime_readback_waits_for_async_arm_state() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route(
            "/api/v1/devices/{device_id}/status",
            get(delayed_thermal_runtime_readback),
        )
        .with_state(ThermalRuntimeReadbackTestState {
            attempts: attempts.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let resolved = ResolvedUsbTarget {
        device: "bench".to_string(),
        devd: format!("http://{addr}"),
        hardware_id: None,
    };

    let status = wait_for_thermal_runtime_readback(
        &Client::new(),
        &resolved,
        "lease-test",
        json!({
            "targetTempC": 140,
            "heaterEnabled": false,
            "activeCoolingEnabled": true,
        }),
        true,
        140,
    )
    .await
    .unwrap();

    assert_eq!(status["heaterEnabled"], true);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    server.abort();
}

#[tokio::test]
async fn thermal_retune_apply_preview_writes_verified_receipt_and_uses_candidate() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());
    let (base_url, runtime_requests, server) = spawn_retune_apply_server(true, true, false).await;

    let summary = thermal_retune::run_thermal_retune(
        &Client::new(),
        &base_url,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: Some("bench".to_string()),
                hardware: None,
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert_eq!(summary["applyPreview"]["ok"], true);
    assert_eq!(replay_summary["applyPreview"]["ok"], true);
    assert_eq!(
        replay_summary["applyPreview"]["statusReadback"]["thermalControlProfilePreview"],
        true
    );
    let requests = runtime_requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["leaseId"], "lease-retune");
    assert_eq!(requests[0]["thermalProfileMode"], "100w");
    assert_eq!(
        requests[0]["thermalControlProfile"]["op"].as_str(),
        Some("preview")
    );
    assert_eq!(
        requests[0]["thermalControlProfile"]["profile"],
        replay_summary["candidateProfile"]
    );

    server.abort();
}

#[tokio::test]
async fn thermal_retune_apply_preview_failure_preserves_replay_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());
    let (base_url, _runtime_requests, server) = spawn_retune_apply_server(true, true, true).await;

    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        &base_url,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: Some("bench".to_string()),
                hardware: None,
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("HTTP 500"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert!(
        replay_summary["applyPreview"]["error"]
            .as_str()
            .unwrap()
            .contains("HTTP 500")
    );
    assert!(
        dir.path()
            .join("thermal-profile.replayed.candidate.json")
            .exists()
    );

    server.abort();
}

#[tokio::test]
async fn thermal_retune_apply_preview_target_error_preserves_replay_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());

    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        DEFAULT_DEVD_ENDPOINT,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: None,
                hardware: None,
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("requires --device or --hardware"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert_eq!(
        replay_summary["applyPreview"]["target"]["devd"],
        DEFAULT_DEVD_ENDPOINT
    );
    assert!(
        dir.path()
            .join("thermal-profile.replayed.candidate.json")
            .exists()
    );
}

#[tokio::test]
async fn thermal_retune_apply_preview_rejects_ambiguous_target_and_preserves_replay_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());

    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        DEFAULT_DEVD_ENDPOINT,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: Some("bench".to_string()),
                hardware: Some("saved-bench".to_string()),
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("accepts only one of --device or --hardware"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert_eq!(
        replay_summary["applyPreview"]["target"]["deviceId"],
        "bench"
    );
    assert_eq!(
        replay_summary["applyPreview"]["target"]["hardwareId"],
        "saved-bench"
    );
    assert!(
        dir.path()
            .join("thermal-profile.replayed.candidate.json")
            .exists()
    );
}

#[tokio::test]
async fn thermal_retune_apply_preview_missing_saved_hardware_preserves_replay_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());

    let missing_hardware_id = "missing-retune-hardware-7c6c7596f0b64e75";
    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        DEFAULT_DEVD_ENDPOINT,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: None,
                hardware: Some(missing_hardware_id.to_string()),
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("saved hardware not found"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert_eq!(
        replay_summary["applyPreview"]["target"]["hardwareId"],
        missing_hardware_id
    );
    assert!(
        dir.path()
            .join("thermal-profile.replayed.candidate.json")
            .exists()
    );
}

#[tokio::test]
async fn thermal_retune_apply_preview_requires_status_readback_preview_flag() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());
    let (base_url, _runtime_requests, server) =
        spawn_retune_apply_server(false, false, false).await;

    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        &base_url,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: Some("bench".to_string()),
                hardware: None,
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("preview"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert_eq!(
        replay_summary["applyPreview"]["statusReadback"]["thermalControlProfilePreview"],
        false
    );

    server.abort();
}

#[tokio::test]
async fn thermal_retune_apply_preview_requires_profile_to_cover_active_target() {
    let dir = tempfile::tempdir().unwrap();
    write_retune_fixture(dir.path());
    let (base_url, _runtime_requests, server) = spawn_retune_apply_server(true, false, false).await;

    let error = thermal_retune::run_thermal_retune(
        &Client::new(),
        &base_url,
        ThermalRetuneArgs {
            target: TargetSelector {
                device: Some("bench".to_string()),
                hardware: None,
            },
            run_dir: dir.path().to_path_buf(),
            optimize_targets_c: None,
            apply_preview: true,
        },
    )
    .await
    .unwrap_err()
    .to_string();

    let replay_summary: Value =
        serde_json::from_slice(&fs::read(dir.path().join("run.replayed.json")).unwrap()).unwrap();
    assert!(error.contains("does not cover"));
    assert_eq!(replay_summary["applyPreview"]["ok"], false);
    assert_eq!(
        replay_summary["applyPreview"]["statusReadback"]["thermalControl"]["profileCoversTarget"],
        false
    );

    server.abort();
}

fn thermal_report_rerender_live_bundle_splits_time_reset_attempts_fixture()
-> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let legacy_dir = dir.path().join("legacy-live");
    fs::create_dir_all(&legacy_dir).unwrap();

    let legacy_bundle = thermal_report_live_legacy_bundle();
    fs::write(
        legacy_dir.join("run.bundle.json"),
        serde_json::to_vec_pretty(&legacy_bundle).unwrap(),
    )
    .unwrap();
    fs::write(
        legacy_dir.join("thermal-profile.accepted.json"),
        serde_json::to_vec_pretty(&json!({
            "points": [thermal_report_test_point(60)],
            "settings": {}
        }))
        .unwrap(),
    )
    .unwrap();
    let live_samples = thermal_report_live_samples();
    fs::write(
        legacy_dir.join("samples.ndjson"),
        live_samples
            .iter()
            .map(|sample| serde_json::to_string(sample).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    (dir, legacy_dir)
}
fn thermal_report_live_legacy_bundle() -> Value {
    let legacy_bundle = json!({
        "kind": "thermal_self_test_report_bundle",
        "runId": "legacy-live-run",
        "generatedAt": "2026-07-20T10:36:44.251Z",
        "selectedMode": "100w",
        "resolvedBank": "pps5a",
        "detectedSourceClass": "pps5a",
        "bundleDisposition": "latest_live_report",
        "acceptedProfileRole": "review_candidate_snapshot",
        "source": {
            "deviceId": "f293cc9c139e"
        },
        "target": {
            "deviceId": "serial-303a-1001-A0:F2:62:F2:0D:6C"
        },
        "parameters": {
            "holdSeconds": 60
        },
        "sourceRuns": {
            "60": "thermal-self-test-runs/source-run-summaries/60.run.json"
        },
        "candidateProfile": {
            "settings": {},
            "points": [thermal_report_test_point(60)]
        },
        "applied": [
            {
                "targetTempC": 60,
                "stopReason": "completed",
                "maxOvershootC": 0.75,
                "holdPeakToPeakC": 1.71,
                "analysis": {
                    "holdMedianOutputPermille": 0,
                    "holdP90OutputPermille": 100,
                    "approachSource": {"powerMw": {"avg": 22425.0}},
                    "holdSource": {"powerMw": {"avg": 3935.0}}
                },
                "fullSpeedToStable": {
                    "limitMs": 10000,
                    "settleTimeMs": 8200,
                    "failureReason": Value::Null
                },
                "guard": {
                    "firstHoldAtMs": 9100
                }
            }
        ],
        "validation": {
            "passed": true,
            "expectedTargetsC": [60],
            "failures": []
        }
    });
    legacy_bundle
}

fn thermal_report_live_samples() -> Vec<Value> {
    let live_samples = [
        json!({
            "targetTempC": 60,
            "elapsedMs": 5000,
            "status": {
                "currentTempC": 60.8,
                "heaterFilteredTempC": 60.7,
                "heaterOutputPercent": 16,
                "heaterPhysicalOutputPercent": 16,
                "pdRequestMv": 21000
            },
            "phase": "hold",
            "heaterParameters": thermal_report_test_point(60),
            "sourceTelemetry": {
                "voltageMv": 21000,
                "currentMa": 280,
                "powerMw": 5880
            }
        }),
        json!({
            "targetTempC": 60,
            "elapsedMs": 1000,
            "status": {
                "currentTempC": 60.2,
                "heaterFilteredTempC": 60.1,
                "heaterOutputPercent": 18,
                "heaterPhysicalOutputPercent": 18,
                "pdRequestMv": 21000
            },
            "phase": "hold",
            "heaterParameters": thermal_report_test_point(60),
            "sourceTelemetry": {
                "voltageMv": 21000,
                "currentMa": 300,
                "powerMw": 6300
            }
        }),
    ];
    live_samples.to_vec()
}
#[test]
fn cooldown_target_reached_allows_quantized_edge_without_hiding_real_overshoot() {
    assert!(super::cooldown_target_reached(35.1, 35.0));
    assert!(!super::cooldown_target_reached(35.2, 35.0));
}

#[test]
fn static_ipv4_cli_validation_rejects_multicast_values() {
    let result = super::static_ipv4_value(
        Some("224.0.0.1".parse().unwrap()),
        Some(24),
        Some("192.168.31.1".parse().unwrap()),
        Some("1.1.1.1".parse().unwrap()),
    );
    assert!(result.is_err());
}

#[test]
fn wifi_set_omits_unspecified_fields_but_keeps_explicit_empty_password() {
    let omitted = super::wifi_set_body("Ivan".to_string(), None, None, None);
    assert!(omitted.get("password").is_none());
    assert!(omitted.get("staticIpv4").is_none());
    assert!(omitted.get("telemetryIntervalMs").is_none());

    let cleared = super::wifi_set_body("Ivan".to_string(), Some(String::new()), None, None);
    assert_eq!(cleared["password"], "");
}

#[test]
fn buzzer_test_uses_the_devd_request_field_names() {
    let body = super::buzzer_test_body("trigger", Some("ui_input"), None, true);

    assert_eq!(body["op"], "trigger");
    assert_eq!(body["cue"], "ui_input");
    assert!(body["scenario"].is_null());
    assert_eq!(body["repeat"], true);
    assert!(body.get("buzzerCue").is_none());
    assert!(body.get("buzzerScenario").is_none());
}

#[test]
fn buzzer_commands_wait_for_an_authoritative_readback() {
    assert_eq!(
        super::buzzer_capture_delay(
            Some(super::BuzzerCueArg::HeaterOn),
            None,
            false,
            false,
            false,
        ),
        Some(Duration::from_millis(270))
    );
    assert_eq!(
        super::buzzer_capture_delay(
            None,
            Some(super::BuzzerScenarioArg::FeedbackReplace),
            false,
            false,
            false,
        ),
        Some(Duration::from_millis(450))
    );
    assert_eq!(
        super::buzzer_capture_delay(
            None,
            Some(super::BuzzerScenarioArg::ActiveCoolingRetrigger),
            false,
            false,
            false,
        ),
        Some(Duration::from_millis(600))
    );
    assert_eq!(
        super::buzzer_capture_delay(Some(super::BuzzerCueArg::UiInput), None, true, false, false,),
        Some(Duration::from_millis(145))
    );
}

#[test]
fn interactive_continuous_status_is_local_until_the_operator_refreshes() {
    let status = super::buzzer_interactive_repeat_status(super::BuzzerCueArg::HeaterOn);

    assert_eq!(status["state"], "running");
    assert_eq!(status["cue"], "heater_on");
    assert_eq!(status["activeCue"], "heater_on");
    assert_eq!(status["repeat"], true);
    assert_eq!(status["outputTrace"], json!([]));
}

#[test]
fn buzzer_output_trace_summary_exposes_the_timer_readback() {
    let status = json!({
        "outputTrace": [{
            "requestedFrequencyHz": 1680,
            "appliedFrequencyHz": 1739,
            "observedFrequencyHz": 1683,
            "dutyPercent": 50,
        }],
    });

    let summary = super::buzzer_output_trace_summary(&status);

    assert!(summary.contains("requested 1680 Hz"));
    assert!(summary.contains("timer 1739 Hz"));
    assert!(summary.contains("pad 1683 Hz"));
    assert!(summary.contains("duty 50%"));
}

#[test]
fn buzzer_play_accepts_an_explicit_device_selector() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "buzzer",
        "play",
        "--device",
        "serial-direct-id",
    ])
    .unwrap();

    let Command::Buzzer {
        command: BuzzerCommand::Play(BuzzerPlayArgs { target, .. }),
    } = cli.command
    else {
        panic!("buzzer play command parses");
    };
    assert_eq!(target.device.as_deref(), Some("serial-direct-id"));
    assert_eq!(target.hardware, None);
}

#[test]
fn buzzer_play_accepts_pointer_capture_as_an_explicit_mode() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "buzzer",
        "play",
        "--device",
        "serial-direct-id",
        "--pointer",
    ])
    .unwrap();

    let Command::Buzzer {
        command: BuzzerCommand::Play(BuzzerPlayArgs { pointer, .. }),
    } = cli.command
    else {
        panic!("buzzer play command parses");
    };
    assert!(pointer);
}

#[test]
fn firmware_commands_require_the_declared_local_sources_and_port() {
    assert!(Cli::try_parse_from(["flux-purr", "update", "--bundle", "x"]).is_err());
    assert!(Cli::try_parse_from(["flux-purr", "update", "--port", "/dev/cu.test"]).is_err());
    assert!(Cli::try_parse_from(["flux-purr", "flash", "--elf", "x"]).is_err());
    assert!(
        Cli::try_parse_from([
            "flux-purr",
            "recover",
            "--port",
            "/dev/cu.test",
            "--elf",
            "firmware.elf",
            "--confirm",
            "ERASE",
        ])
        .is_ok()
    );
    assert!(validate_local_control_endpoint("http://127.0.0.1:30080").is_err());
}

#[test]
fn direct_firmware_commands_retain_explicit_confirmation_boundaries() {
    let flash = Cli::try_parse_from([
        "flux-purr",
        "flash",
        "--port",
        "/dev/cu.test",
        "--skip-backup",
    ])
    .unwrap();
    let Command::Flash(args) = flash.command else {
        panic!("flash command parses");
    };
    assert!(args.skip_backup);
    assert!(args.confirm.is_none());

    let recover = Cli::try_parse_from([
        "flux-purr",
        "recover",
        "--port",
        "/dev/cu.test",
        "--elf",
        "firmware.elf",
        "--confirm",
        "ERASE",
    ])
    .unwrap();
    let Command::Recover(args) = recover.command else {
        panic!("recover command parses");
    };
    assert_eq!(args.confirm, "ERASE");
}

#[cfg(unix)]
#[test]
fn direct_flash_skip_backup_calls_only_espflash_flash() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let elf = directory.path().join("firmware.elf");
    let calls = directory.path().join("calls");
    let fake_espflash = directory.path().join("espflash");
    fs::write(&elf, b"\x7fELFtest fixture").unwrap();
    fs::write(
        &fake_espflash,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf 'Hash of data verified.\\n'\n",
            calls.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_espflash).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_espflash, permissions).unwrap();

    let result = direct_flash_with_program(
        FlashArgs {
            port: "/dev/cu.contract-test".to_string(),
            elf: Some(elf),
            skip_backup: true,
            confirm: Some("NO_EEPROM_BACKUP".to_string()),
        },
        &fake_espflash,
        false,
    )
    .unwrap();

    let invocations = fs::read_to_string(calls).unwrap();
    assert_eq!(invocations.lines().count(), 1);
    assert!(invocations.starts_with("flash "));
    assert!(!invocations.contains("board-info"));
    assert!(result["backup"].is_null());
    assert_eq!(result["espflash"]["command"], "flash");
}

#[cfg(unix)]
#[test]
fn direct_flash_archives_before_invoking_espflash() {
    use std::os::unix::fs::PermissionsExt;

    fn fixture_snapshot(_port: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![0xa5; developer_backup::EEPROM_SNAPSHOT_BYTES])
    }
    fn fixture_rom_probe(_port: &str) -> bool {
        false
    }

    let directory = tempfile::tempdir().unwrap();
    let elf = directory.path().join("firmware.elf");
    let calls = directory.path().join("calls");
    let fake_espflash = directory.path().join("espflash");
    let backup_directory = directory.path().join("developer-flash-backups");
    fs::create_dir(&backup_directory).unwrap();
    fs::write(&elf, b"\x7fELFtest fixture").unwrap();
    fs::write(
            &fake_espflash,
            format!(
                "#!/bin/sh\ncount=$(find '{}' -maxdepth 1 -name 'backup-*.bin' -type f | wc -l)\n[ \"$count\" -eq 1 ] || exit 9\nprintf '%s\\n' \"$*\" >> '{}'\nprintf 'Hash of data verified.\\n'\n",
                backup_directory.display(),
                calls.display()
            ),
        )
        .unwrap();
    let mut permissions = fs::metadata(&fake_espflash).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_espflash, permissions).unwrap();

    let result = direct_flash_with_program_inner(
        FlashArgs {
            port: "/dev/cu.contract-test".to_string(),
            elf: Some(elf),
            skip_backup: false,
            confirm: None,
        },
        &fake_espflash,
        false,
        fixture_snapshot,
        fixture_rom_probe,
        Some(&backup_directory),
    )
    .unwrap();

    let backup_path = result["backup"].as_str().unwrap();
    assert_eq!(
        fs::read(backup_path).unwrap(),
        vec![0xa5; developer_backup::EEPROM_SNAPSHOT_BYTES]
    );
    assert_eq!(fs::read_to_string(calls).unwrap().lines().count(), 1);
    assert_eq!(result["espflash"]["command"], "flash");
}

#[cfg(unix)]
#[test]
fn direct_flash_blocks_espflash_when_backup_directory_is_unavailable() {
    use std::os::unix::fs::PermissionsExt;

    fn fixture_snapshot(_port: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![0x3c; developer_backup::EEPROM_SNAPSHOT_BYTES])
    }
    fn fixture_rom_probe(_port: &str) -> bool {
        false
    }

    let directory = tempfile::tempdir().unwrap();
    let elf = directory.path().join("firmware.elf");
    let fake_espflash = directory.path().join("espflash");
    let calls = directory.path().join("calls");
    let unavailable_backup_directory = directory.path().join("backup-path");
    fs::write(&unavailable_backup_directory, b"not a directory").unwrap();
    fs::write(&elf, b"\x7fELFtest fixture").unwrap();
    fs::write(
        &fake_espflash,
        format!("printf '%s\\n' \"$*\" >> '{}'\n", calls.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_espflash).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_espflash, permissions).unwrap();

    let error = direct_flash_with_program_inner(
        FlashArgs {
            port: "/dev/cu.contract-test".to_string(),
            elf: Some(elf),
            skip_backup: false,
            confirm: None,
        },
        &fake_espflash,
        false,
        fixture_snapshot,
        fixture_rom_probe,
        Some(&unavailable_backup_directory),
    )
    .unwrap_err();

    assert!(!error.to_string().is_empty());
    assert!(!calls.exists());
}

#[test]
fn eeprom_snapshot_reports_absent_device_output_without_claiming_an_eeprom_fault() {
    let mut reader = SnapshotFixtureReader::from_bytes(b"");

    let error = read_snapshot_response(
        &mut reader,
        "snapshot-test",
        StdInstant::now() + Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("no USB JSONL response"));
    assert!(error.to_string().contains("EEPROM health is unknown"));
    assert!(error.to_string().contains("firmware was not written"));
}

#[test]
fn eeprom_snapshot_reports_protocol_incompatibility_for_unmatched_jsonl() {
    let mut reader =
        SnapshotFixtureReader::from_bytes(b"{\"ok\":true,\"requestId\":\"different-request\"}\n");

    let error = read_snapshot_response(
        &mut reader,
        "snapshot-test",
        StdInstant::now() + Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(
        error
            .to_string()
            .contains("none matched the EEPROM snapshot request")
    );
    assert!(
        error
            .to_string()
            .contains("application firmware is incompatible")
    );
    assert!(!error.to_string().contains("different-request"));
}

#[test]
fn eeprom_snapshot_reports_the_m24c64_hardware_fault_returned_by_the_device() {
    let mut reader = SnapshotFixtureReader::from_bytes(
        b"{\"ok\":false,\"requestId\":\"snapshot-test\",\"error\":\"eeprom_unavailable\"}\n",
    );

    let error = read_snapshot_response(
        &mut reader,
        "snapshot-test",
        StdInstant::now() + Duration::from_secs(1),
    )
    .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(
        error
            .to_string()
            .contains("external M24C64 EEPROM is not detected")
    );
    assert!(error.to_string().contains("I2C wiring"));
    assert!(error.to_string().contains("firmware was not written"));
}

#[test]
fn snapshot_failures_only_trigger_rom_probe_for_missing_or_non_json_output() {
    let no_response = io::Error::new(io::ErrorKind::TimedOut, "no USB JSONL response");
    let non_json = io::Error::new(io::ErrorKind::TimedOut, "non-JSON serial output");
    let eeprom_fault = io::Error::other("external M24C64 EEPROM is not detected");

    assert!(snapshot_error_may_be_rom_mode(&no_response));
    assert!(snapshot_error_may_be_rom_mode(&non_json));
    assert!(!snapshot_error_may_be_rom_mode(&eeprom_fault));
}

#[test]
fn rom_download_probe_uses_only_the_explicit_port_without_reset() {
    let args = rom_download_probe_args("/dev/cu.test");

    assert_eq!(args[0], "--skip-update-check");
    assert_eq!(args[1], "board-info");
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--port" && pair[1] == "/dev/cu.test" })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--before" && pair[1] == "no-reset" })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--after" && pair[1] == "no-reset" })
    );
    assert!(args.iter().any(|arg| arg == "--no-stub"));
    assert!(args.iter().any(|arg| arg == "--non-interactive"));
}

#[test]
fn direct_elf_flash_rebuilds_the_checked_in_partition_layout() {
    let args = direct_elf_flash_args(
        "/dev/cu.test",
        Path::new("firmware/partitions.csv"),
        Path::new("firmware.elf"),
    )
    .unwrap();

    assert_eq!(args[0], "flash");
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--port" && pair[1] == "/dev/cu.test" })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--partition-table" && pair[1] == "firmware/partitions.csv" })
    );
    assert!(!args.iter().any(|arg| arg == "--no-stub"));
    assert_eq!(args.last().map(String::as_str), Some("firmware.elf"));
    assert_eq!(direct_erase_flash_args("/dev/cu.test")[0], "erase-flash");
}

#[test]
fn espflash_diagnostics_preserve_both_streams_and_classify_flash_end_failure() {
    let diagnostics = classify_espflash_diagnostics(
        "flash",
        Some(1),
        "Writing at 0x00010000...\nHash of data verified.\n",
        "Error:   x The bootloader returned an error\n  `FlashEnd` command failed\n",
    );

    assert_eq!(diagnostics.phase, "finalize");
    assert_eq!(diagnostics.phases, vec!["write", "verify", "finalize"]);
    assert!(diagnostics.stdout.contains("Writing at 0x00010000"));
    assert!(diagnostics.stderr.contains("FlashEnd"));
    assert!(
        diagnostics
            .hint
            .contains("image may have been written, but write completeness is unconfirmed")
    );
    let failure = format_espflash_failure(&diagnostics);
    assert!(failure.contains("exit_code=Some(1)"));
    assert!(failure.contains("Writing at 0x00010000"));
    assert!(failure.contains("FlashEnd"));
}

#[test]
fn espflash_diagnostics_report_connection_failure_as_a_transport_problem() {
    let diagnostics = classify_espflash_diagnostics(
        "flash",
        Some(1),
        "",
        "Error: failed to connect to ESP32-S3: timed out waiting for packet header\n",
    );

    assert_eq!(diagnostics.phase, "connect");
    assert!(diagnostics.hint.contains("boot mode"));
    assert!(diagnostics.hint.contains("serial link"));
    assert!(diagnostics.hint.contains("power"));
}

#[cfg(unix)]
#[test]
fn espflash_command_failure_keeps_stdout_and_stderr_from_the_child_process() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let fake_espflash = directory.path().join("espflash");
    fs::write(
            &fake_espflash,
            "#!/bin/sh\nprintf 'Writing at 0x00010000...\\nHash of data verified.\\n'\nprintf 'Error: FlashEnd command failed\\n' >&2\nexit 1\n",
        )
        .unwrap();
    let mut permissions = fs::metadata(&fake_espflash).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_espflash, permissions).unwrap();

    let error = run_espflash_command(&fake_espflash, &["flash".to_string()])
        .unwrap_err()
        .to_string();

    assert!(error.contains("phase `finalize`"));
    assert!(error.contains("Writing at 0x00010000"));
    assert!(error.contains("Hash of data verified"));
    assert!(error.contains("FlashEnd command failed"));
}

#[test]
fn managed_devd_starts_only_when_no_endpoint_was_supplied() {
    assert!(should_start_managed_devd(false, false));
    assert!(!should_start_managed_devd(false, true));
    assert!(!should_start_managed_devd(true, false));
    assert!(!should_start_managed_devd(true, true));
}

#[test]
fn buzzer_play_rejects_a_network_devd_endpoint_after_parsing() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "buzzer",
        "play",
        "--device",
        "serial-direct-id",
        "--devd",
        "http://127.0.0.1:14830",
    ])
    .unwrap();

    assert!(validate_local_control_endpoint(&cli.devd).is_err());
}

#[test]
fn buzzer_test_accepts_loop_as_a_repeat_alias() {
    let cli = Cli::try_parse_from([
        "flux-purr",
        "buzzer",
        "test",
        "--device",
        "serial-direct-id",
        "--cue",
        "ui-input",
        "--loop",
    ])
    .unwrap();

    let Command::Buzzer {
        command: BuzzerCommand::Test(BuzzerTestArgs { repeat, .. }),
    } = cli.command
    else {
        panic!("buzzer test command parses");
    };
    assert!(repeat);
}

#[test]
fn terminal_buzzer_controls_map_keys_and_pointer_to_production_actions() {
    let mut selection = BuzzerTerminalSelection::default();

    assert_eq!(
        buzzer_terminal_key_action(KeyCode::Enter, KeyEventKind::Press, selection, false,),
        Some(BuzzerInteractiveAction::Play {
            cue: BuzzerCueArg::UiInput,
            repeat: false,
            stop_current: false,
        })
    );
    assert_eq!(
        buzzer_terminal_key_action(KeyCode::Char(' '), KeyEventKind::Repeat, selection, false,),
        Some(BuzzerInteractiveAction::Play {
            cue: BuzzerCueArg::UiInput,
            repeat: false,
            stop_current: false,
        })
    );
    assert_eq!(
        buzzer_terminal_key_action(KeyCode::Char('c'), KeyEventKind::Press, selection, false),
        Some(BuzzerInteractiveAction::Play {
            cue: BuzzerCueArg::UiInput,
            repeat: true,
            stop_current: false,
        })
    );
    assert_eq!(
        buzzer_terminal_key_action(KeyCode::Char('l'), KeyEventKind::Press, selection, false),
        Some(BuzzerInteractiveAction::Play {
            cue: BuzzerCueArg::UiInput,
            repeat: true,
            stop_current: false,
        })
    );

    assert!(buzzer_terminal_move_selection(
        &mut selection,
        KeyCode::End,
        KeyEventKind::Press,
    ));
    selection.select_row(buzzer_terminal_scenario_start_row() + 1);
    assert_eq!(
        buzzer_terminal_key_action(KeyCode::Enter, KeyEventKind::Press, selection, false,),
        Some(BuzzerInteractiveAction::RunScenario {
            scenario: BuzzerScenarioArg::FeedbackReplace,
            stop_current: false,
        })
    );
    assert_eq!(
        buzzer_terminal_pointer_action(buzzer_terminal_actions_row(), 0, selection, true,),
        Some(BuzzerInteractiveAction::Stop)
    );
    assert!(
        buzzer_terminal_pointer_action(buzzer_terminal_actions_row(), 30, selection, false,)
            .is_none()
    );
}

#[test]
fn terminal_buzzer_catalogue_uses_every_production_cue() {
    for (index, descriptor) in BUZZER_CUE_CATALOG.iter().enumerate() {
        let selection = BuzzerTerminalSelection { index };
        assert_eq!(
            selection.primary_action(false),
            BuzzerInteractiveAction::Play {
                cue: descriptor.cue,
                repeat: false,
                stop_current: false,
            },
            "missing terminal action for {}",
            descriptor.label,
        );
        assert_eq!(
            selection.continuous_action(false),
            Some(BuzzerInteractiveAction::Play {
                cue: descriptor.cue,
                repeat: true,
                stop_current: false,
            }),
            "missing continuous terminal action for {}",
            descriptor.label,
        );
    }
}

#[test]
fn terminal_buzzer_render_exposes_copyable_default_mode() {
    assert_eq!(buzzer_pointer_mode_label(false), "[M] Pointer mode");
    assert_eq!(buzzer_pointer_mode_label(true), "[M] Copy mode");
}

#[test]
fn interactive_buzzer_play_selects_a_one_shot_cue_after_invalid_input() {
    let status = json!({"state": "idle", "activeCue": null});
    let mut input = BufReader::new("invalid\n1\n2\n1\n".as_bytes());
    let mut output = Vec::new();

    let action = super::prompt_buzzer_play_action(&status, &mut input, &mut output).unwrap();

    assert_eq!(
        action,
        super::BuzzerInteractiveAction::Play {
            cue: super::BuzzerCueArg::HeaterOn,
            repeat: false,
            stop_current: false,
        }
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Enter a number from 1 through 4.")
    );
}

#[test]
fn interactive_buzzer_play_exposes_the_complete_test_session_surface() {
    let status = json!({"state": "idle", "activeCue": null});
    let mut input = BufReader::new("1\n1\n1\n".as_bytes());
    let mut output = Vec::new();

    let _ = super::prompt_buzzer_play_action(&status, &mut input, &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();

    for cue in [
        "UI input",
        "Heater on",
        "Heater off",
        "Active cooling on",
        "Active cooling off",
        "Heater reject",
        "Active cooling reject",
        "Protection alarm",
        "Attention reminder",
    ] {
        assert!(output.contains(cue), "missing production cue: {cue}");
    }
    assert!(output.contains("Run feedback-arbitration scenario"));
    assert!(output.contains("Refresh session status"));
    assert!(output.contains("Exit without changing playback"));
    assert!(output.contains("Play once"));
    assert!(output.contains("Play continuously"));
}

#[test]
fn interactive_buzzer_play_catalog_covers_every_cli_cue() {
    let catalog: Vec<_> = super::BUZZER_CUE_CATALOG
        .iter()
        .map(|descriptor| descriptor.cue)
        .collect();

    assert_eq!(catalog.as_slice(), super::BuzzerCueArg::value_variants());
}

#[test]
fn interactive_buzzer_play_can_start_an_arbitration_scenario() {
    let status = json!({"state": "idle", "activeCue": null});
    let mut input = BufReader::new("2\n2\n".as_bytes());
    let mut output = Vec::new();

    let action = super::prompt_buzzer_play_action(&status, &mut input, &mut output).unwrap();

    assert_eq!(
        action,
        super::BuzzerInteractiveAction::RunScenario {
            scenario: super::BuzzerScenarioArg::FeedbackReplace,
            stop_current: false,
        }
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Feedback-arbitration scenarios:")
    );
}

#[test]
fn interactive_buzzer_play_reports_silence_between_repeated_cues() {
    let status = json!({
        "state": "running",
        "cue": "protection_alarm",
        "activeCue": null,
        "repeat": true,
        "trace": []
    });
    let mut input = BufReader::new("4\n".as_bytes());
    let mut output = Vec::new();

    let action = super::prompt_buzzer_play_action(&status, &mut input, &mut output).unwrap();

    assert_eq!(action, super::BuzzerInteractiveAction::Exit);
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("PWM output is silent between production cue steps or cadence bursts.")
    );
}

#[test]
fn interactive_buzzer_play_requires_an_explicit_replacement_of_running_audio() {
    let status = json!({
        "state": "running",
        "activeCue": null,
        "cue": "protection_alarm"
    });
    let mut input = BufReader::new("3\n1\n8\n2\n".as_bytes());
    let mut output = Vec::new();

    let action = super::prompt_buzzer_play_action(&status, &mut input, &mut output).unwrap();

    assert_eq!(
        action,
        super::BuzzerInteractiveAction::Play {
            cue: super::BuzzerCueArg::ProtectionAlarm,
            repeat: true,
            stop_current: true,
        }
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("The running session will not be stopped automatically.")
    );
}
