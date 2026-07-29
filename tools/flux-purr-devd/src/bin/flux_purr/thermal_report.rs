use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value, json};

const UNKNOWN_LEGACY_METADATA: &str = "unknown";
const DATA_PLACEHOLDER: &str = "__THERMAL_REPORT_DATA__";
const REPORT_TEMPLATE: &str = include_str!("thermal_preliminary_review_template.html");
const POINT_FIELDS: &[&str] = &[
    "targetTempC",
    "brakeDistanceCentiC",
    "warmupPowerPermille",
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
];

#[derive(Debug, Clone)]
pub(super) struct ThermalLegacyReportInput {
    pub(super) legacy_bundle_dir: PathBuf,
    pub(super) output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(super) struct ThermalSelfTestReportInput {
    pub(super) run_dirs: Vec<PathBuf>,
    pub(super) output_dir: Option<PathBuf>,
}

pub(super) fn rerender_legacy_preliminary_review_bundle(
    input: ThermalLegacyReportInput,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let legacy_bundle_dir = absolute_path(&input.legacy_bundle_dir)?;
    let output_dir = absolute_path(
        &input
            .output_dir
            .unwrap_or_else(|| infer_output_dir(&legacy_bundle_dir)),
    )?;
    if legacy_bundle_dir == output_dir {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rerender output directory must be different from the legacy bundle directory",
        )
        .into());
    }

    let legacy_bundle = read_json(&legacy_bundle_dir.join("run.bundle.json"))?;
    let accepted_profile = read_json(&legacy_bundle_dir.join("thermal-profile.accepted.json"))?;
    let target_samples = grouped_samples(&legacy_bundle_dir.join("samples.ndjson"))?;
    let entries = match legacy_bundle.get("kind").and_then(Value::as_str) {
        Some("thermal_self_test_report_bundle") => {
            legacy_live_report_entries(&legacy_bundle, &accepted_profile, &target_samples)?
        }
        Some("thermal_self_test_preliminary_bundle") => legacy_bundle
            .get("runs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => legacy_preliminary_review_entries(&legacy_bundle, &target_samples)?,
    };

    let source = legacy_bundle.get("source").and_then(Value::as_object);
    let target = legacy_bundle.get("target").and_then(Value::as_object);
    let source_device_id = source
        .and_then(|payload| {
            payload
                .get("sourceDeviceId")
                .or_else(|| payload.get("deviceId"))
                .and_then(Value::as_str)
        })
        .or_else(|| legacy_bundle.get("sourceDeviceId").and_then(Value::as_str))
        .unwrap_or("unknown-source")
        .to_string();
    let device_id = target
        .and_then(|payload| payload.get("deviceId").and_then(Value::as_str))
        .or_else(|| legacy_bundle.get("deviceId").and_then(Value::as_str))
        .or_else(|| source.and_then(|payload| payload.get("deviceId").and_then(Value::as_str)))
        .unwrap_or("unknown-device")
        .to_string();
    let port_path = target
        .and_then(|payload| {
            payload
                .get("port")
                .or_else(|| payload.get("portPath"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            source.and_then(|payload| {
                payload
                    .get("port")
                    .or_else(|| payload.get("portPath"))
                    .and_then(Value::as_str)
            })
        })
        .or_else(|| {
            legacy_bundle
                .get("port")
                .or_else(|| legacy_bundle.get("portPath"))
                .and_then(Value::as_str)
        })
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();

    let selected_mode = legacy_bundle
        .get("selectedMode")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();
    let resolved_bank = legacy_bundle
        .get("resolvedBank")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();
    let detected_source_class = legacy_bundle
        .get("detectedSourceClass")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();
    let source_preset = legacy_bundle
        .get("sourcePreset")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();
    let provider = legacy_bundle
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA)
        .to_string();
    let generated_at = legacy_bundle
        .get("generatedAt")
        .cloned()
        .unwrap_or_else(|| json!(current_unix_millis()));
    let entry_targets_c = entries
        .iter()
        .filter_map(|entry| entry.get("target").and_then(Value::as_i64))
        .filter_map(|target| i16::try_from(target).ok())
        .collect::<Vec<_>>();
    let fallback_targets_c = i16_array_field(&legacy_bundle, "flagshipTargetsC")
        .unwrap_or_else(|| entry_targets_c.clone());
    let tuning_targets_c =
        i16_array_field(&legacy_bundle, "tuningTargetsC").unwrap_or(fallback_targets_c);
    let tuning_execution_order_c = unique_i16_preserve_order(entry_targets_c.clone());

    let bundle = write_preliminary_review_bundle(
        &output_dir,
        &accepted_profile,
        entries,
        &source_device_id,
        &device_id,
        &port_path,
        0,
        generated_at,
        &selected_mode,
        &resolved_bank,
        &detected_source_class,
        &tuning_targets_c,
        &tuning_execution_order_c,
        &source_preset,
        &provider,
    )?;

    Ok(json!({
        "ok": true,
        "operation": "thermal_report.rerender_legacy_preliminary_review_bundle",
        "legacyBundleDir": display_path(&legacy_bundle_dir),
        "outputDir": display_path(&output_dir),
        "bundleJson": bundle.pointer("/files/bundleJson").cloned().unwrap_or(Value::Null),
        "bundleIndexHtml": bundle.pointer("/files/indexHtml").cloned().unwrap_or(Value::Null),
        "samplesPath": bundle.pointer("/files/samplesPath").cloned().unwrap_or(Value::Null),
        "acceptedProfilePath": bundle.pointer("/files/acceptedProfilePath").cloned().unwrap_or(Value::Null),
        "kind": bundle.get("kind").cloned().unwrap_or(Value::Null),
        "bundleDisposition": bundle.get("bundleDisposition").cloned().unwrap_or(Value::Null),
        "acceptedProfileRole": bundle.get("acceptedProfileRole").cloned().unwrap_or(Value::Null),
        "tuningTargetsC": bundle.get("tuningTargetsC").cloned().unwrap_or(Value::Null),
    }))
}

/// Adapt a completed raw self-test into the canonical HTML evidence bundle.
///
/// This intentionally snapshots the active thermal-plant model instead of a
/// point-local thermal profile. The legacy accepted-profile filename is kept
/// only because the report renderer's four-file bundle is a stable contract.
pub(super) fn render_self_test_evidence_bundle(
    input: ThermalSelfTestReportInput,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let run_dirs = input
        .run_dirs
        .iter()
        .map(|path| absolute_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    let run_dir = run_dirs.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "self-test report requires at least one raw run directory",
        )
    })?;
    let output_dir = absolute_path(
        &input
            .output_dir
            .unwrap_or_else(|| infer_self_test_output_dir(run_dir)),
    )?;
    if *run_dir == output_dir {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "self-test report output directory must be different from the raw run directory",
        )
        .into());
    }

    let summaries = run_dirs
        .iter()
        .map(|run_dir| read_json(&run_dir.join("run.json")))
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summaries.first().expect("non-empty raw run directories");
    if summary.get("kind").and_then(Value::as_str) != Some("thermal_self_test") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "self-test report requires a thermal_self_test run.json",
        )
        .into());
    }
    let mut target_temps_c = Vec::new();
    let mut stage_sources = BTreeMap::new();
    let mut all_samples = BTreeMap::<i16, Vec<Value>>::new();
    for (run_dir, summary) in run_dirs.iter().zip(summaries.iter()) {
        if summary.get("kind").and_then(Value::as_str) != Some("thermal_self_test") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "self-test report requires thermal_self_test run.json",
            )
            .into());
        }
        let samples = grouped_samples(&run_dir.join("samples.ndjson"))?;
        let applied = summary
            .get("applied")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "self-test run missing applied")
            })?;
        let run_id = summary
            .get("runId")
            .and_then(Value::as_str)
            .unwrap_or("unknown-self-test")
            .to_string();
        for target in self_test_targets(summary)? {
            target_temps_c.push(target);
            if let Some(stage) = applied.iter().find(|stage| {
                stage.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target))
            }) && stage.get("stopReason").and_then(Value::as_str) == Some("completed")
            {
                stage_sources.insert(
                    target,
                    (
                        summary.clone(),
                        run_id.clone(),
                        stage.clone(),
                        samples.get(&target).cloned().unwrap_or_default(),
                    ),
                );
            }
        }
        for (target, samples) in samples {
            all_samples.entry(target).or_default().extend(samples);
        }
    }
    let target_temps_c = unique_i16_preserve_order(target_temps_c);
    let hold_seconds = summary
        .pointer("/parameters/holdSeconds")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let entries = target_temps_c
        .iter()
        .map(|target_temp_c| {
            let (stage_summary, run_id, stage, raw_samples) =
                stage_sources.get(target_temp_c).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("self-test report has no completed stage for {target_temp_c}C"),
                    )
                })?;
            let samples = normalized_sorted_samples(raw_samples);
            if samples.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("self-test run missing raw samples for {target_temp_c}C"),
                )
                .into());
            }
            let replay_samples = super::thermal_replay_stage_samples(raw_samples, *target_temp_c)?;
            let replay_analysis =
                super::thermal_replay_full_speed_to_stable(&replay_samples, *target_temp_c);
            let full_speed_to_stable = json!({
                "limitMs": if *target_temp_c > 150 { 5_000 } else { 10_000 },
                "stableBandC": 1.5,
                "stableWindowMs": 10_000,
                "warmupExitedAtMs": replay_analysis.warmup_exited_at_ms,
                "stableWindowStartedAtMs": replay_analysis.stable_window_started_at_ms,
                "stableWindowVerifiedAtMs": replay_analysis.stable_window_verified_at_ms,
                "settleTimeMs": replay_analysis.settle_time_ms,
                "failureReason": replay_analysis.failure_reason,
            });
            Ok(self_test_report_entry(
                stage_summary,
                run_id,
                *target_temp_c,
                stage,
                samples,
                hold_seconds,
                full_speed_to_stable,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error + Send + Sync>>>()?;

    let source = summary.get("source").and_then(Value::as_object);
    let source_id = source
        .and_then(|value| {
            value
                .get("deviceId")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)
        })
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let device_id = summary
        .pointer("/target/deviceId")
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let selected_mode = source
        .and_then(|value| value.get("selectedMode").and_then(Value::as_str))
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let resolved_bank = source
        .and_then(|value| value.get("resolvedBank").and_then(Value::as_str))
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let detected_source_class = source
        .and_then(|value| value.get("detectedSourceClass").and_then(Value::as_str))
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let source_preset = self_test_source_preset(source);
    let provider = source
        .and_then(|value| value.get("kind").and_then(Value::as_str))
        .map(provider_name)
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let port_path = summary
        .pointer("/target/port")
        .or_else(|| summary.pointer("/target/portPath"))
        .and_then(Value::as_str)
        .unwrap_or(UNKNOWN_LEGACY_METADATA);
    let active_model = thermal_plant_model_snapshot(&all_samples);
    let accepted_profile = json!({
        "kind": "thermal_plant_model_evidence",
        "role": "runtime_model_snapshot",
        "profileCompatibility": "not_a_point_local_profile",
        "runIds": run_dirs.iter().map(|path| display_path(path)).collect::<Vec<_>>(),
        "model": active_model,
    });

    let bundle = write_preliminary_review_bundle(
        &output_dir,
        &accepted_profile,
        entries,
        source_id,
        device_id,
        port_path,
        0,
        summary
            .get("generatedAt")
            .or_else(|| summary.get("capturedAtUnixMs"))
            .cloned()
            .unwrap_or_else(|| json!(current_unix_millis())),
        selected_mode,
        resolved_bank,
        detected_source_class,
        &target_temps_c,
        &target_temps_c,
        &source_preset,
        provider,
    )?;

    Ok(json!({
        "ok": true,
        "operation": "thermal_report.render_self_test_evidence_bundle",
        "runDirs": run_dirs.iter().map(|path| display_path(path)).collect::<Vec<_>>(),
        "outputDir": display_path(&output_dir),
        "bundleJson": bundle.pointer("/files/bundleJson").cloned().unwrap_or(Value::Null),
        "bundleIndexHtml": bundle.pointer("/files/indexHtml").cloned().unwrap_or(Value::Null),
        "samplesPath": bundle.pointer("/files/samplesPath").cloned().unwrap_or(Value::Null),
        "acceptedProfilePath": bundle.pointer("/files/acceptedProfilePath").cloned().unwrap_or(Value::Null),
        "kind": bundle.get("kind").cloned().unwrap_or(Value::Null),
        "bundleDisposition": bundle.get("bundleDisposition").cloned().unwrap_or(Value::Null),
        "targetsC": target_temps_c,
    }))
}

fn infer_self_test_output_dir(run_dir: &Path) -> PathBuf {
    let name = run_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("thermal-self-test");
    run_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}-html-report"))
}

fn self_test_targets(
    summary: &Value,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let targets = summary
        .pointer("/parameters/targetsC")
        .or_else(|| summary.pointer("/validation/expectedTargetsC"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "self-test run has no declared validation targets",
            )
        })?;
    let targets = targets
        .iter()
        .map(value_as_i16)
        .collect::<Result<Vec<_>, _>>()?;
    if targets.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "self-test run has an empty validation target list",
        )
        .into());
    }
    Ok(unique_i16_preserve_order(targets))
}

fn self_test_report_entry(
    summary: &Value,
    run_id: &str,
    target_temp_c: i16,
    stage: &Value,
    samples: Vec<Value>,
    hold_seconds: u64,
    full_speed_to_stable: Value,
) -> Value {
    let mut failures = validation_failures_for_target(summary, target_temp_c);
    let stage_completed = stage.get("stopReason").and_then(Value::as_str) == Some("completed");
    let full_speed_limit_ms = if target_temp_c > 150 { 5_000 } else { 10_000 };
    let replay_gate_failure_reason = full_speed_to_stable
        .get("failureReason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.is_empty())
        .map(|_| "full_speed_to_stable")
        .or_else(|| {
            match full_speed_to_stable
                .get("settleTimeMs")
                .and_then(Value::as_u64)
            {
                Some(settle_time_ms) if settle_time_ms <= full_speed_limit_ms => None,
                Some(_) => Some("full_speed_to_stable"),
                None => Some("full_speed_to_stable_missing"),
            }
        });
    if stage_completed
        && !failures.iter().any(|failure| {
            matches!(
                failure.get("reason").and_then(Value::as_str),
                Some("full_speed_to_stable" | "full_speed_to_stable_missing")
            )
        })
        && let Some(reason) = replay_gate_failure_reason
    {
        failures.push(json!({
            "targetTempC": target_temp_c,
            "reason": reason,
            "limit": full_speed_limit_ms,
            "settleTimeMs": full_speed_to_stable.get("settleTimeMs").cloned().unwrap_or(Value::Null),
            "warmupExitedAtMs": full_speed_to_stable.get("warmupExitedAtMs").cloned().unwrap_or(Value::Null),
            "stableWindowStartedAtMs": full_speed_to_stable.get("stableWindowStartedAtMs").cloned().unwrap_or(Value::Null),
            "failureReason": full_speed_to_stable.get("failureReason").cloned().unwrap_or(Value::Null),
        }));
    }
    let passed = stage_completed && failures.is_empty();
    let failure_reason = failures
        .first()
        .and_then(|failure| failure.get("reason"))
        .cloned()
        .or_else(|| stage.get("terminalRuntimeDropReason").cloned())
        .unwrap_or(Value::Null);
    let time_spent_seconds = int_round_json(
        samples
            .last()
            .and_then(|sample| sample.get("t"))
            .and_then(Value::as_f64),
    );
    let hold_check = json!({
        "confirmRunId": run_id,
        "passed": passed,
        "failureReason": if passed { Value::Null } else { failure_reason.clone() },
        "holdSeconds": hold_seconds,
        "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
        "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
        "holdMedianOutputPermille": stage.pointer("/analysis/holdMedianOutputPermille").cloned().unwrap_or(Value::Null),
        "holdP90OutputPermille": stage.pointer("/analysis/holdP90OutputPermille").cloned().unwrap_or(Value::Null),
        "approachSource": stage.pointer("/analysis/approachSource").cloned().unwrap_or(Value::Null),
        "holdSource": stage.pointer("/analysis/holdSource").cloned().unwrap_or(Value::Null),
        "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
    });
    let mut result = stage.clone();
    result["fullSpeedToStable"] = full_speed_to_stable;
    let round = json!({
        "round": 1,
        "label": "runtime validation",
        "attemptType": "validation",
        "candidateName": "thermal-plant-model",
        "selected": true,
        "evidenceValid": passed,
        "evidenceInvalidReason": if passed { Value::Null } else { failure_reason.clone() },
        "point": Value::Null,
        "samples": samples.clone(),
        "failures": failures.clone(),
        "result": result.clone(),
    });
    json!({
        "runId": run_id,
        "target": target_temp_c,
        "targetTempC": target_temp_c,
        "targetRole": "validation",
        "ok": passed,
        "candidateReady": false,
        "candidateDisposition": if passed { "validation_passed" } else { "validation_failed" },
        "saved": false,
        "evidence": "thermal_plant_hil",
        "budgetOutcome": if passed { "validation_passed" } else { "validation_failed" },
        "timeSpentSeconds": time_spent_seconds,
        "roundCount": 1,
        "validTestCount": usize::from(passed),
        "invalidTestCount": usize::from(!passed),
        "approachReference": {
            "targetTempC": target_temp_c,
            "variantId": "full_speed_to_stable_gate",
            "passed": passed,
            "limitMs": full_speed_limit_ms,
            "failureReason": if passed { Value::Null } else { failure_reason.clone() },
        },
        "point": Value::Null,
        "truthPoint": Value::Null,
        "pointSource": "thermal_plant_runtime",
        "rounds": [round],
        "result": result,
        "failures": failures,
        "samples": samples,
        "holdCheck": hold_check,
    })
}

fn self_test_source_preset(source: Option<&Map<String, Value>>) -> String {
    let Some(preset) = source
        .and_then(|value| value.get("preset"))
        .and_then(Value::as_object)
    else {
        return UNKNOWN_LEGACY_METADATA.to_string();
    };
    let Some(voltage_mv) = preset.get("voltageMv").and_then(Value::as_u64) else {
        return UNKNOWN_LEGACY_METADATA.to_string();
    };
    let Some(current_ma) = preset.get("currentLimitMa").and_then(Value::as_u64) else {
        return UNKNOWN_LEGACY_METADATA.to_string();
    };
    format!(
        "{}V / {}A PPS auto-follow",
        decimal_milliunits(voltage_mv),
        decimal_milliunits(current_ma)
    )
}

fn decimal_milliunits(value: u64) -> String {
    if value.is_multiple_of(1_000) {
        (value / 1_000).to_string()
    } else if value.is_multiple_of(100) {
        format!("{:.1}", value as f64 / 1_000.0)
    } else {
        format!("{:.2}", value as f64 / 1_000.0)
    }
}

fn provider_name(kind: &str) -> &str {
    match kind {
        "isolapurr" => "IsolaPurr",
        _ => kind,
    }
}

fn thermal_plant_model_snapshot(samples: &BTreeMap<i16, Vec<Value>>) -> Value {
    samples
        .values()
        .flat_map(|samples| samples.iter().rev())
        .find_map(|sample| {
            sample
                .pointer("/status/thermalPlantModel")
                .or_else(|| sample.pointer("/heaterTelemetry/thermalPlantModel"))
                .cloned()
        })
        .unwrap_or(Value::Null)
}

fn infer_output_dir(legacy_bundle_dir: &Path) -> PathBuf {
    let name = legacy_bundle_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("preliminary-review");
    legacy_bundle_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{name}-rerendered"))
}

fn absolute_path(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn display_path(path: &Path) -> String {
    match env::current_dir() {
        Ok(cwd) => match path.strip_prefix(&cwd) {
            Ok(relative) => relative.display().to_string(),
            Err(_) => path.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = fs::read(path)?;
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(value),
        Err(strict_error) => {
            let text = String::from_utf8(bytes)?;
            let sanitized = sanitize_non_finite_json_numbers(&text);
            if sanitized == text {
                return Err(strict_error.into());
            }
            Ok(serde_json::from_str(&sanitized)?)
        }
    }
}

fn sanitize_non_finite_json_numbers(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            out.push(byte as char);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if byte == b'"' {
            in_string = true;
            out.push('"');
            index += 1;
            continue;
        }

        if let Some(length) = non_finite_token_length(bytes, index) {
            out.push_str("null");
            index += length;
            continue;
        }

        out.push(byte as char);
        index += 1;
    }
    out
}

fn non_finite_token_length(bytes: &[u8], index: usize) -> Option<usize> {
    const TOKENS: [&[u8]; 3] = [b"-Infinity", b"Infinity", b"NaN"];
    TOKENS
        .iter()
        .find(|token| bytes[index..].starts_with(token))
        .and_then(|token| {
            let before_ok = index == 0 || is_json_token_delimiter(bytes[index.saturating_sub(1)]);
            let after_index = index + token.len();
            let after_ok =
                after_index >= bytes.len() || is_json_token_delimiter(bytes[after_index]);
            if before_ok && after_ok {
                Some(token.len())
            } else {
                None
            }
        })
}

fn is_json_token_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\n' | b'\r' | b'\t' | b',' | b':' | b'[' | b']' | b'{' | b'}'
    )
}

fn write_json_pretty(
    path: &Path,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("json output path has no parent"))?,
    )?;
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn grouped_samples(
    samples_path: &Path,
) -> Result<BTreeMap<i16, Vec<Value>>, Box<dyn std::error::Error + Send + Sync>> {
    let handle = std::fs::File::open(samples_path)?;
    let reader = BufReader::new(handle);
    let mut groups = BTreeMap::<i16, Vec<Value>>::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let sample: Value = serde_json::from_str(&line)?;
        let target_temp_c = value_as_i16(sample.get("targetTempC").ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "sample missing targetTempC")
        })?)?;
        groups.entry(target_temp_c).or_default().push(sample);
    }
    Ok(groups)
}

fn value_as_i16(value: &Value) -> Result<i16, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(number) = value.as_i64() {
        return Ok(number as i16);
    }
    if let Some(number) = value.as_u64() {
        return Ok(number as i16);
    }
    if let Some(number) = value.as_f64() {
        return Ok(number.round() as i16);
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "expected integer value").into())
}

fn round_decimal(value: f64, places: u32) -> f64 {
    let scale = 10_f64.powi(places as i32);
    (value * scale).round() / scale
}

fn number_to_json(value: Option<f64>) -> Value {
    value.map_or(Value::Null, Value::from)
}

fn int_round_json(value: Option<f64>) -> Value {
    value.map_or(Value::Null, |item| Value::from(item.round() as i64))
}

fn normalized_sample(sample: &Value) -> Value {
    let status = sample.get("status").and_then(Value::as_object);
    let heater = sample.get("heaterTelemetry").and_then(Value::as_object);
    let source = sample.get("sourceTelemetry").and_then(Value::as_object);
    let request_mv = status
        .and_then(|payload| payload.get("pdRequestMv").and_then(Value::as_f64))
        .or_else(|| heater.and_then(|payload| payload.get("ppsRequestMv").and_then(Value::as_f64)))
        .or_else(|| status.and_then(|payload| payload.get("voltageMv").and_then(Value::as_f64)))
        .or_else(|| {
            heater.and_then(|payload| payload.get("hotplateVoltageMv").and_then(Value::as_f64))
        })
        .unwrap_or(0.0);
    json!({
        "t": round_decimal(sample.get("elapsedMs").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0, 3),
        "temp": status.and_then(|payload| payload.get("currentTempC")).cloned().or_else(|| heater.and_then(|payload| payload.get("currentTempC")).cloned()).unwrap_or(Value::Null),
        "filtered": status.and_then(|payload| payload.get("heaterFilteredTempC")).cloned().or_else(|| heater.and_then(|payload| payload.get("heaterFilteredTempC")).cloned()).unwrap_or(Value::Null),
        "control": status.and_then(|payload| payload.get("heaterControlTempC")).cloned().or_else(|| heater.and_then(|payload| payload.get("heaterControlTempC")).cloned()).unwrap_or(Value::Null),
        "controlGuarded": status.and_then(|payload| payload.get("heaterControlMeasurementGuarded")).cloned().unwrap_or(Value::Null),
        "command": status.and_then(|payload| payload.get("heaterOutputPercent")).cloned().or_else(|| heater.and_then(|payload| payload.get("heaterOutputPercent")).cloned()).unwrap_or(Value::Null),
        "output": status.and_then(|payload| payload.get("heaterPhysicalOutputPercent")).cloned().or_else(|| heater.and_then(|payload| payload.get("heaterPhysicalOutputPercent")).cloned()).unwrap_or(Value::Null),
        "requestV": round_decimal(request_mv / 1000.0, 3),
        "phase": sample.get("phase").cloned().unwrap_or(Value::Null),
        "sourceVoltageV": number_to_json(source.and_then(|payload| payload.get("voltageMv").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
        "sourceCurrentA": number_to_json(source.and_then(|payload| payload.get("currentMa").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
        "sourcePowerW": number_to_json(source.and_then(|payload| payload.get("powerMw").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
        "parameters": sample.get("heaterParameters").cloned().unwrap_or(Value::Null),
    })
}

fn sample_time_key(sample: &Value) -> Option<f64> {
    sample
        .get("t")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn sort_samples_by_time(mut samples: Vec<Value>) -> Vec<Value> {
    samples.sort_by(
        |left, right| match (sample_time_key(left), sample_time_key(right)) {
            (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        },
    );
    samples
}

fn normalized_sorted_samples(samples: &[Value]) -> Vec<Value> {
    sort_samples_by_time(
        samples
            .iter()
            .filter(|sample| sample.is_object())
            .map(|sample| {
                if sample.get("t").is_some()
                    && (sample.get("temp").is_some() || sample.get("filtered").is_some())
                {
                    sample.clone()
                } else {
                    normalized_sample(sample)
                }
            })
            .collect(),
    )
}

fn split_samples_on_time_reset(samples: &[Value]) -> Vec<Vec<Value>> {
    let mut segments = Vec::<Vec<Value>>::new();
    let mut current = Vec::<Value>::new();
    let mut last_t = None::<f64>;
    for sample in samples {
        if !sample.is_object() {
            continue;
        }
        let current_t = sample
            .get("elapsedMs")
            .or_else(|| sample.get("t"))
            .and_then(Value::as_f64);
        if !current.is_empty()
            && current_t.is_some()
            && last_t.is_some()
            && current_t.unwrap_or_default() < last_t.unwrap_or_default()
        {
            segments.push(current);
            current = Vec::new();
        }
        current.push(sample.clone());
        if current_t.is_some() {
            last_t = current_t;
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn sanitize_point(point: Option<&Value>, target_temp_c: Option<i16>) -> Option<Value> {
    let object = point?.as_object()?;
    let mut sanitized = Map::new();
    for field in POINT_FIELDS {
        if let Some(value) = object.get(*field) {
            sanitized.insert((*field).to_string(), value.clone());
        }
    }
    if let Some(target_temp_c) = target_temp_c {
        sanitized.insert("targetTempC".to_string(), json!(target_temp_c));
    }
    if sanitized.is_empty() {
        None
    } else {
        Some(Value::Object(sanitized))
    }
}

fn point_map(profile: &Value) -> BTreeMap<i16, Value> {
    let mut points = BTreeMap::new();
    if let Some(entries) = profile.get("points").and_then(Value::as_array) {
        for point in entries {
            let Some(target_temp_c) = point
                .get("targetTempC")
                .and_then(Value::as_i64)
                .map(|value| value as i16)
            else {
                continue;
            };
            points.insert(target_temp_c, point.clone());
        }
    }
    points
}

fn effective_point_from_samples(samples: &[Value], target_temp_c: i16) -> Option<Value> {
    for sample in samples {
        let point = sample
            .get("heaterParameters")
            .or_else(|| sample.get("parameters"));
        if let Some(point) = sanitize_point(point, Some(target_temp_c)) {
            return Some(point);
        }
    }
    None
}

fn validation_failures_for_target(summary: &Value, target_temp_c: i16) -> Vec<Value> {
    summary
        .pointer("/validation/failures")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|failure| {
            failure
                .get("targetTempC")
                .and_then(Value::as_i64)
                .map(|value| value as i16 == target_temp_c)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn legacy_preliminary_review_entries(
    legacy_bundle: &Value,
    grouped_target_samples: &BTreeMap<i16, Vec<Value>>,
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let mut entries = Vec::new();
    let Some(targets) = legacy_bundle.get("targets").and_then(Value::as_array) else {
        return Ok(entries);
    };
    for target_payload in targets {
        let target_temp_c = value_as_i16(target_payload.get("targetTempC").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy preliminary target missing targetTempC",
            )
        })?)?;
        let hold_check = target_payload
            .get("holdCheck")
            .cloned()
            .filter(|value| value.is_object())
            .unwrap_or_else(|| json!({}));
        let variants = target_payload
            .get("variants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let effective_point =
            sanitize_point(target_payload.get("effectivePoint"), Some(target_temp_c));
        let raw_target_samples = grouped_target_samples
            .get(&target_temp_c)
            .cloned()
            .unwrap_or_default();
        let target_segments = split_samples_on_time_reset(&raw_target_samples);
        let top_level_samples = target_segments
            .last()
            .map(|segment| normalized_sorted_samples(segment))
            .unwrap_or_default();

        let mut rounds = Vec::new();
        let selected_round = variants.len().max(1);
        for (index, variant) in variants.iter().enumerate() {
            let Some(variant_object) = variant.as_object() else {
                continue;
            };
            let variant_point =
                sanitize_point(variant_object.get("tunedPoint"), Some(target_temp_c));
            let metrics = variant_object
                .get("metrics")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let variant_samples = sort_samples_by_time(
                variant_object
                    .get("samples")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|sample| {
                        let source = sample.get("sourceTelemetry").and_then(Value::as_object);
                        json!({
                            "t": round_decimal(sample.get("elapsedMs").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0, 3),
                            "temp": sample.get("currentTempC").cloned().unwrap_or(Value::Null),
                            "filtered": sample.get("heaterFilteredTempC").cloned().unwrap_or(Value::Null),
                            "control": sample.get("heaterControlTempC").cloned().unwrap_or(Value::Null),
                            "controlGuarded": sample.get("heaterControlMeasurementGuarded").cloned().unwrap_or(Value::Null),
                            "command": sample.get("heaterOutputPercent").cloned().unwrap_or(Value::Null),
                            "output": sample.get("heaterPhysicalOutputPercent").cloned().unwrap_or(Value::Null),
                            "requestV": Value::Null,
                            "phase": sample.get("heaterControlPhase").cloned().unwrap_or(Value::Null),
                            "sourceVoltageV": number_to_json(source.and_then(|payload| payload.get("voltageMv").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                            "sourceCurrentA": number_to_json(source.and_then(|payload| payload.get("currentMa").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                            "sourcePowerW": number_to_json(source.and_then(|payload| payload.get("powerMw").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                        })
                    })
                    .collect(),
            );
            rounds.push(json!({
                "round": index + 1,
                "label": variant_object.get("variantLabel").cloned().unwrap_or_else(|| json!(format!("variant {}", index + 1))),
                "attemptType": "characterization",
                "candidateName": variant_object.get("variantId").cloned().unwrap_or_else(|| json!(format!("variant_{}", index + 1))),
                "selected": index + 1 == selected_round,
                "evidenceValid": variant_object.get("valid").and_then(Value::as_bool).unwrap_or(true),
                "point": variant_point.unwrap_or(Value::Null),
                "samples": variant_samples,
                "failures": [],
                "result": {
                    "stopReason": "completed",
                    "maxOvershootC": metrics.get("peak").cloned().unwrap_or(Value::Null),
                    "holdPeakToPeakC": metrics.get("rollback").cloned().unwrap_or(Value::Null),
                    "settleTimeMs": metrics.get("approachDurationMs").cloned().unwrap_or(Value::Null),
                },
            }));
        }

        let full_speed_limit_ms = if target_temp_c <= 150 { 10_000 } else { 5_000 };
        let passed = hold_check
            .get("passed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let failure_reason = hold_check
            .get("failureReason")
            .cloned()
            .unwrap_or(Value::Null);
        let failures = if !passed && !hold_check.is_null() {
            vec![json!({
                "targetTempC": target_temp_c,
                "reason": failure_reason.clone(),
            })]
        } else {
            Vec::new()
        };
        entries.push(json!({
            "runId": hold_check.get("confirmRunId").cloned().unwrap_or_else(|| legacy_bundle.get("runId").cloned().unwrap_or_else(|| json!(format!("legacy-{target_temp_c}")))),
            "target": target_temp_c,
            "targetTempC": target_temp_c,
            "ok": passed,
            "saved": false,
            "evidence": "preliminary_review",
            "budgetOutcome": if passed { "completed" } else { "not_converged" },
            "timeSpentSeconds": int_round_json(top_level_samples.last().and_then(|sample| sample.get("t")).and_then(Value::as_f64)),
            "roundCount": rounds.len(),
            "validTestCount": rounds.iter().filter(|round| round.get("evidenceValid").and_then(Value::as_bool) != Some(false)).count(),
            "invalidTestCount": rounds.iter().filter(|round| round.get("evidenceValid").and_then(Value::as_bool) == Some(false)).count(),
            "approachReference": { "limitMs": full_speed_limit_ms },
            "point": effective_point.clone().unwrap_or(Value::Null),
            "truthPoint": effective_point.unwrap_or(Value::Null),
            "pointSource": "review_candidate_snapshot",
            "rounds": rounds,
            "result": {
                "stopReason": hold_check.get("stopReason").cloned().unwrap_or_else(|| if passed { json!("completed") } else { failure_reason.clone() }),
                "maxOvershootC": hold_check.get("maxOvershootC").cloned().unwrap_or(Value::Null),
                "holdPeakToPeakC": hold_check.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
                "fullSpeedToStable": {
                    "limitMs": full_speed_limit_ms,
                    "settleTimeMs": Value::Null,
                    "failureReason": failure_reason.clone(),
                },
                "analysis": {
                    "holdMedianOutputPermille": hold_check.get("holdMedianOutputPermille").cloned().unwrap_or(Value::Null),
                    "holdP90OutputPermille": hold_check.get("holdP90OutputPermille").cloned().unwrap_or(Value::Null),
                    "approachSource": hold_check.get("approachSource").cloned().unwrap_or(Value::Null),
                    "holdSource": hold_check.get("holdSource").cloned().unwrap_or(Value::Null),
                },
            },
            "failures": failures,
            "samples": top_level_samples,
            "holdCheck": hold_check,
        }));
    }
    Ok(entries)
}

fn legacy_live_report_entries(
    legacy_bundle: &Value,
    accepted_profile: &Value,
    grouped_target_samples: &BTreeMap<i16, Vec<Value>>,
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let accepted_points = point_map(accepted_profile);
    let candidate_points = legacy_bundle
        .get("candidateProfile")
        .map(point_map)
        .unwrap_or_default();
    let hold_seconds = legacy_bundle.pointer("/parameters/holdSeconds").cloned();
    let source_runs = legacy_bundle
        .get("sourceRuns")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut entries = Vec::new();
    let Some(applied) = legacy_bundle.get("applied").and_then(Value::as_array) else {
        return Ok(entries);
    };
    for stage in applied {
        let target_temp_c = value_as_i16(stage.get("targetTempC").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy live report stage missing targetTempC",
            )
        })?)?;
        let raw_target_samples = grouped_target_samples
            .get(&target_temp_c)
            .cloned()
            .unwrap_or_default();
        let target_segments = split_samples_on_time_reset(&raw_target_samples);
        let mut segment_rounds = Vec::new();
        for (segment_index, raw_segment) in target_segments.iter().enumerate() {
            let segment_samples = normalized_sorted_samples(raw_segment);
            segment_rounds.push(json!({
                "round": segment_index + 1,
                "label": format!("legacy live review {}", segment_index + 1),
                "attemptType": "legacy_live_report",
                "candidateName": format!("legacy_live_report_{}", segment_index + 1),
                "selected": segment_index + 1 == target_segments.len(),
                "evidenceValid": true,
                "point": effective_point_from_samples(raw_segment, target_temp_c).unwrap_or(Value::Null),
                "samples": segment_samples,
            }));
        }
        let selected_round = segment_rounds.last().cloned();
        let top_level_samples = selected_round
            .as_ref()
            .and_then(|round| round.get("samples").and_then(Value::as_array))
            .cloned()
            .unwrap_or_default();
        let point = sanitize_point(accepted_points.get(&target_temp_c), Some(target_temp_c))
            .or_else(|| sanitize_point(candidate_points.get(&target_temp_c), Some(target_temp_c)))
            .or_else(|| {
                selected_round
                    .as_ref()
                    .and_then(|round| sanitize_point(round.get("point"), Some(target_temp_c)))
            })
            .or_else(|| effective_point_from_samples(&raw_target_samples, target_temp_c))
            .unwrap_or(Value::Null);
        let analysis = stage
            .get("analysis")
            .cloned()
            .filter(|value| value.is_object())
            .unwrap_or_else(|| json!({}));
        let full_speed = stage
            .get("fullSpeedToStable")
            .cloned()
            .filter(|value| value.is_object())
            .unwrap_or_else(|| json!({}));
        let failures = validation_failures_for_target(legacy_bundle, target_temp_c);
        let failure_reason = failures
            .first()
            .and_then(|failure| failure.get("reason"))
            .cloned()
            .or_else(|| full_speed.get("failureReason").cloned())
            .or_else(|| match stage.get("stopReason").and_then(Value::as_str) {
                Some("completed") | None => None,
                Some(reason) => Some(json!(reason)),
            })
            .unwrap_or(Value::Null);
        let passed = failures.is_empty()
            && stage.get("stopReason").and_then(Value::as_str) == Some("completed");
        let confirm_run_id = format!(
            "{}-{}",
            legacy_bundle
                .get("runId")
                .and_then(Value::as_str)
                .unwrap_or("legacy"),
            target_temp_c
        );
        let hold_check = json!({
            "confirmRunId": confirm_run_id,
            "passed": passed,
            "failureReason": failure_reason.clone(),
            "holdSeconds": hold_seconds.clone().unwrap_or(Value::Null),
            "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
            "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
            "firstHoldAtMs": stage.pointer("/guard/firstHoldAtMs").cloned().unwrap_or(Value::Null),
            "holdMedianOutputPermille": analysis.get("holdMedianOutputPermille").cloned().unwrap_or(Value::Null),
            "holdP90OutputPermille": analysis.get("holdP90OutputPermille").cloned().unwrap_or(Value::Null),
            "approachSource": analysis.get("approachSource").cloned().unwrap_or(Value::Null),
            "holdSource": analysis.get("holdSource").cloned().unwrap_or(Value::Null),
            "sourceRunPath": source_runs.get(&target_temp_c.to_string()).cloned().unwrap_or(Value::Null),
            "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
        });
        let round_result = json!({
            "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
            "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
            "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
            "settleTimeMs": full_speed.get("settleTimeMs").cloned().unwrap_or(Value::Null),
            "fullSpeedLimitMs": full_speed.get("limitMs").cloned().unwrap_or(Value::Null),
            "failureReason": full_speed.get("failureReason").cloned().unwrap_or(Value::Null),
        });
        let rounds: Vec<Value> = segment_rounds
            .into_iter()
            .map(|segment_round| {
                let mut round = segment_round;
                if round.get("point").is_none() || round.get("point") == Some(&Value::Null) {
                    round["point"] = point.clone();
                }
                round["failures"] = Value::Array(failures.clone());
                round["result"] = round_result.clone();
                round
            })
            .collect();
        entries.push(json!({
            "runId": hold_check.get("confirmRunId").cloned().unwrap_or(Value::Null),
            "target": target_temp_c,
            "targetTempC": target_temp_c,
            "ok": passed,
            "saved": false,
            "evidence": "preliminary_review",
            "budgetOutcome": if passed { "completed" } else { "not_converged" },
            "timeSpentSeconds": int_round_json(top_level_samples.last().and_then(|sample| sample.get("t")).and_then(Value::as_f64)),
            "roundCount": rounds.len(),
            "validTestCount": rounds.iter().filter(|round| round.get("evidenceValid").and_then(Value::as_bool) != Some(false)).count(),
            "invalidTestCount": 0,
            "approachReference": { "limitMs": full_speed.get("limitMs").cloned().unwrap_or(Value::Null) },
            "point": point.clone(),
            "truthPoint": point.clone(),
            "pointSource": "review_candidate_snapshot",
            "rounds": rounds,
            "result": {
                "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
                "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
                "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
                "fullSpeedToStable": full_speed,
                "analysis": analysis,
            },
            "failures": failures,
            "samples": top_level_samples,
            "holdCheck": hold_check,
        }));
    }
    Ok(entries)
}

fn sort_entry_samples(entry: &Value) -> Value {
    let mut sorted = entry.clone();
    if let Some(samples) = entry.get("samples").and_then(Value::as_array) {
        sorted["samples"] = Value::Array(sort_samples_by_time(samples.clone()));
    }
    if let Some(rounds) = entry.get("rounds").and_then(Value::as_array) {
        sorted["rounds"] = Value::Array(
            rounds
                .iter()
                .filter_map(|round| {
                    let mut sorted_round = round.clone();
                    let samples = round.get("samples").and_then(Value::as_array)?.clone();
                    sorted_round["samples"] = Value::Array(sort_samples_by_time(samples));
                    Some(sorted_round)
                })
                .collect(),
        );
    }
    sorted
}

fn build_history(entries: &[Value]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let settle_time_ms = entry.pointer("/result/fullSpeedToStable/settleTimeMs");
            json!({
                "runId": entry.get("runId").cloned().unwrap_or(Value::Null),
                "target": entry.get("target").cloned().unwrap_or(Value::Null),
                "ok": entry.get("ok").cloned().unwrap_or(Value::Null),
                "overshoot": entry.pointer("/result/maxOvershootC").cloned().unwrap_or(Value::Null),
                "p2p": entry.pointer("/result/holdPeakToPeakC").cloned().unwrap_or(Value::Null),
                "settle": settle_time_ms.and_then(Value::as_f64).map(|value| round_decimal(value / 1000.0, 3)),
            })
        })
        .collect()
}

fn metric_value(result: &Value, paths: &[&str]) -> Option<f64> {
    paths
        .iter()
        .find_map(|path| result.pointer(path).and_then(Value::as_f64))
}

fn candidate_metric_gate(entry: &Value) -> Option<bool> {
    let target_temp_c = entry
        .get("target")
        .or_else(|| entry.get("targetTempC"))
        .and_then(Value::as_i64)?;
    let result = entry.get("result")?;
    if result.is_null() {
        return None;
    }
    let stop_reason = result.get("stopReason").and_then(Value::as_str)?;
    if stop_reason != "completed" {
        return Some(false);
    }
    let overshoot_c = metric_value(result, &["/maxOvershootC"])?;
    if overshoot_c > 3.0 {
        return Some(false);
    }
    let hold_p2p_c = metric_value(result, &["/holdPeakToPeakC"])?;
    if hold_p2p_c > 3.0 {
        return Some(false);
    }
    if result
        .pointer("/fullSpeedToStable/failureReason")
        .or_else(|| result.get("failureReason"))
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty())
    {
        return Some(false);
    }
    let Some(settle_time_ms) = metric_value(
        result,
        &["/fullSpeedToStable/settleTimeMs", "/settleTimeMs"],
    ) else {
        return Some(false);
    };
    let limit_ms = metric_value(result, &["/fullSpeedToStable/limitMs", "/fullSpeedLimitMs"])
        .unwrap_or(if target_temp_c > 150 {
            5_000.0
        } else {
            10_000.0
        });
    Some(settle_time_ms <= limit_ms)
}

fn unique_targets_sorted(entries: &[Value]) -> Vec<i64> {
    entries
        .iter()
        .filter_map(|entry| entry.get("target").and_then(Value::as_i64))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn target_role(entry: &Value) -> &str {
    entry
        .get("targetRole")
        .and_then(Value::as_str)
        .unwrap_or("tuning")
}

fn select_report_entry_index(entries: &[Value]) -> usize {
    entries
        .iter()
        .rposition(|entry| target_role(entry) == "supplemental_tuning")
        .or_else(|| {
            entries.iter().rposition(|entry| {
                target_role(entry) == "validation"
                    && (entry.get("ok").and_then(Value::as_bool) == Some(true)
                        || entry.get("budgetOutcome").and_then(Value::as_str)
                            == Some("validation_passed"))
            })
        })
        .or_else(|| {
            entries
                .iter()
                .rposition(|entry| target_role(entry) != "validation")
        })
        .unwrap_or_else(|| entries.len().saturating_sub(1))
}

fn report_audit_entry(raw_entry_index: usize, entry: &Value) -> Value {
    json!({
        "rawEntryIndex": raw_entry_index,
        "runId": entry.get("runId").cloned().unwrap_or(Value::Null),
        "target": entry.get("target").cloned().unwrap_or(Value::Null),
        "targetTempC": entry.get("targetTempC").cloned().unwrap_or(Value::Null),
        "targetRole": target_role(entry),
        "ok": entry.get("ok").cloned().unwrap_or(Value::Null),
        "reviewOutcome": entry.get("reviewOutcome").cloned().unwrap_or(Value::Null),
        "reviewPassed": entry.get("reviewPassed").cloned().unwrap_or(Value::Null),
        "budgetOutcome": entry.get("budgetOutcome").cloned().unwrap_or(Value::Null),
        "candidateDisposition": entry
            .get("candidateDisposition")
            .cloned()
            .unwrap_or(Value::Null),
        "candidateReady": entry.get("candidateReady").cloned().unwrap_or(Value::Null),
        "timeSpentSeconds": entry.get("timeSpentSeconds").cloned().unwrap_or(Value::Null),
        "roundCount": entry.get("roundCount").cloned().unwrap_or(Value::Null),
        "validTestCount": entry.get("validTestCount").cloned().unwrap_or(Value::Null),
        "invalidTestCount": entry.get("invalidTestCount").cloned().unwrap_or(Value::Null),
        "failures": entry.get("failures").cloned().unwrap_or_else(|| json!([])),
    })
}

fn placeholder_report_run(target_temp_c: i64) -> Value {
    json!({
        "runId": format!("synthetic-{target_temp_c}-not-executed"),
        "target": target_temp_c,
        "targetTempC": target_temp_c,
        "targetRole": "tuning",
        "ok": false,
        "candidateReady": false,
        "candidateDisposition": "not_executed_without_accepted_bounds",
        "saved": false,
        "evidence": "preliminary_review",
        "budgetOutcome": "not_executed_without_accepted_bounds",
        "timeSpentSeconds": 0,
        "roundCount": 0,
        "validTestCount": 0,
        "invalidTestCount": 0,
        "approachReference": {
            "targetTempC": target_temp_c,
            "variantId": "full_speed_to_stable_gate",
            "passed": false,
            "limitMs": if target_temp_c > 150 { 5_000 } else { 10_000 },
            "failureReason": "missing_accepted_bounds"
        },
        "point": Value::Null,
        "truthPoint": Value::Null,
        "pointSource": "not_executed",
        "rounds": [],
        "result": {
            "stopReason": "missing_accepted_bounds",
            "analysis": {},
            "fullSpeedToStable": {
                "failureReason": "missing_accepted_bounds"
            }
        },
        "failures": [{
            "targetTempC": target_temp_c,
            "reason": "missing_accepted_bounds"
        }],
        "samples": [],
        "holdCheck": Value::Null,
        "auditEntryCount": 0,
        "auditEntries": [],
        "auditSummary": [],
        "reviewPassed": false,
        "reviewOutcome": "failed"
    })
}

fn build_report_runs(entries: &[Value], tuning_targets_c: &[i16]) -> Vec<Value> {
    let targets = if tuning_targets_c.is_empty() {
        unique_targets_sorted(entries)
    } else {
        tuning_targets_c
            .iter()
            .copied()
            .map(i64::from)
            .collect::<Vec<_>>()
    };
    targets
        .iter()
        .map(|target| {
            let audit_entries = entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.get("target").and_then(Value::as_i64) == Some(*target))
                .map(|(index, entry)| (index, entry.clone()))
                .collect::<Vec<_>>();
            if audit_entries.is_empty() {
                return placeholder_report_run(*target);
            }
            let audit_values = audit_entries
                .iter()
                .map(|(_, entry)| entry.clone())
                .collect::<Vec<_>>();
            let selected_index = select_report_entry_index(&audit_values);
            let mut report_entry = audit_entries[selected_index].1.clone();
            let audit_roles = audit_entries
                .iter()
                .map(|(_, entry)| json!({
                    "targetRole": target_role(entry),
                    "reviewOutcome": entry.get("reviewOutcome").cloned().unwrap_or(Value::Null),
                    "reviewPassed": entry.get("reviewPassed").cloned().unwrap_or(Value::Null),
                    "budgetOutcome": entry.get("budgetOutcome").cloned().unwrap_or(Value::Null),
                    "candidateDisposition": entry
                        .get("candidateDisposition")
                        .cloned()
                        .unwrap_or(Value::Null),
                    "candidateReady": entry.get("candidateReady").cloned().unwrap_or(Value::Null),
                    "validTestCount": entry.get("validTestCount").cloned().unwrap_or(Value::Null),
                }))
                .collect::<Vec<_>>();
            let audit_receipts = audit_entries
                .iter()
                .map(|(index, entry)| report_audit_entry(*index, entry))
                .collect::<Vec<_>>();
            report_entry["auditEntryCount"] = json!(audit_entries.len());
            report_entry["auditEntries"] = Value::Array(audit_receipts);
            report_entry["auditSummary"] = Value::Array(audit_roles);
            report_entry
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write_preliminary_review_bundle(
    bundle_dir: &Path,
    accepted_profile: &Value,
    entries: Vec<Value>,
    source_id: &str,
    device_id: &str,
    port_path: &str,
    tuning_budget_seconds: i64,
    generated_at: Value,
    selected_mode: &str,
    resolved_bank: &str,
    detected_source_class: &str,
    tuning_targets_c: &[i16],
    tuning_execution_order_c: &[i16],
    source_preset: &str,
    provider: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let entries: Vec<Value> = entries
        .iter()
        .map(sort_entry_samples)
        .map(ensure_candidate_receipt_fields)
        .collect();
    fs::create_dir_all(bundle_dir)?;
    let samples_path = bundle_dir.join("samples.ndjson");
    let mut sample_lines = String::new();
    for entry in &entries {
        if let Some(rounds) = entry.get("rounds").and_then(Value::as_array)
            && !rounds.is_empty()
        {
            for attempt in rounds {
                for sample in attempt
                    .get("samples")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let mut enriched = sample.clone();
                    enriched["targetTempC"] = entry.get("target").cloned().unwrap_or(Value::Null);
                    enriched["attemptNumber"] =
                        attempt.get("round").cloned().unwrap_or(Value::Null);
                    enriched["attemptType"] =
                        attempt.get("attemptType").cloned().unwrap_or(Value::Null);
                    enriched["candidateName"] =
                        attempt.get("candidateName").cloned().unwrap_or(Value::Null);
                    enriched["selected"] = attempt
                        .get("selected")
                        .cloned()
                        .unwrap_or(Value::Bool(false));
                    enriched["evidenceValid"] = attempt
                        .get("evidenceValid")
                        .cloned()
                        .unwrap_or(Value::Bool(true));
                    enriched["evidenceInvalidReason"] = attempt
                        .get("evidenceInvalidReason")
                        .cloned()
                        .unwrap_or(Value::Null);
                    sample_lines.push_str(&serde_json::to_string(&enriched)?);
                    sample_lines.push('\n');
                }
            }
            continue;
        }
        for sample in entry
            .get("samples")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut enriched = sample.clone();
            enriched["targetTempC"] = entry.get("target").cloned().unwrap_or(Value::Null);
            sample_lines.push_str(&serde_json::to_string(&enriched)?);
            sample_lines.push('\n');
        }
    }
    fs::write(&samples_path, sample_lines)?;

    let accepted_profile_path = bundle_dir.join("thermal-profile.accepted.json");
    let run_bundle_path = bundle_dir.join("run.bundle.json");
    let index_html_path = bundle_dir.join("index.html");
    write_json_pretty(&accepted_profile_path, accepted_profile)?;
    let entries_array = Value::Array(entries.clone());
    let report_runs = build_report_runs(&entries, tuning_targets_c);
    let report_runs_array = Value::Array(report_runs.clone());
    let candidate_dispositions = report_runs
        .iter()
        .map(|entry| {
            (
                entry
                    .get("target")
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
                    .to_string(),
                entry
                    .get("candidateDisposition")
                    .cloned()
                    .unwrap_or(Value::Null),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let candidate_ready_targets_c = report_runs
        .iter()
        .filter_map(|entry| {
            if entry.get("candidateReady").and_then(Value::as_bool) == Some(true) {
                entry.get("target").and_then(Value::as_i64)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    let temperature_semantics = json!({
        "primaryChartField": "humanTempC",
        "gateField": "controlTempC",
        "humanTempC": "Human-readable temperature used for report plots.",
        "filteredTempC": "Firmware-filtered temperature when exposed by telemetry.",
        "controlTempC": "Control-loop temperature used by thermal gates and scoring.",
    });

    let bundle = json!({
        "kind": "thermal_self_test_preliminary_bundle",
        "canonicalReportFormat": "html_bundle",
        "bundleDisposition": "preliminary_review",
        "acceptedProfileRole": "review_candidate_snapshot",
        "generatedAt": generated_at,
        "selectedMode": selected_mode,
        "resolvedBank": resolved_bank,
        "detectedSourceClass": detected_source_class,
        "tuningBudgetSeconds": tuning_budget_seconds,
        "tuningWorkflow": tuning_workflow(resolved_bank),
        "tuningTargetsC": tuning_targets_c,
        "tuningExecutionOrderC": tuning_execution_order_c,
        "temperatureSemantics": temperature_semantics,
        "candidateDispositions": candidate_dispositions,
        "candidateReadyTargetsC": set_to_vec(candidate_ready_targets_c),
        "sourcePreset": source_preset,
        "provider": provider,
        "sourceDeviceId": source_id,
        "deviceId": device_id,
        "port": port_path,
        "reportRuns": report_runs_array,
        "targets": entries_array.clone(),
        "runs": entries_array,
        "files": {
            "bundleDir": display_path(bundle_dir),
            "indexHtml": display_path(&index_html_path),
            "bundleJson": display_path(&run_bundle_path),
            "samplesPath": display_path(&samples_path),
            "acceptedProfilePath": display_path(&accepted_profile_path),
        },
    });
    write_json_pretty(&run_bundle_path, &bundle)?;

    let target_label = bundle
        .get("reportRuns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("target").and_then(Value::as_i64))
        .map(|target| format!("{target}°C"))
        .collect::<Vec<_>>()
        .join(" / ");
    let report_identity = report_identity(selected_mode, resolved_bank);
    let html_data = json!({
        "generatedAt": bundle.get("generatedAt").cloned().unwrap_or(Value::Null),
        "title": format!("Flux Purr {report_identity} {target_label} preliminary review"),
        "subtitle": format!("展示本次 {report_identity} full-batch 调优目标：{target_label}。full-speed-to-stable 按目标温度使用动态门槛：≤150°C 为 10s，>150°C 为 5s；轮次详情展示全部有效调优尝试、预算结果与 hold confirm。"),
        "bundleDisposition": bundle.get("bundleDisposition").cloned().unwrap_or(Value::Null),
        "acceptedProfileRole": bundle.get("acceptedProfileRole").cloned().unwrap_or(Value::Null),
        "selectedMode": bundle.get("selectedMode").cloned().unwrap_or(Value::Null),
        "resolvedBank": bundle.get("resolvedBank").cloned().unwrap_or(Value::Null),
        "detectedSourceClass": bundle.get("detectedSourceClass").cloned().unwrap_or(Value::Null),
        "sourcePreset": bundle.get("sourcePreset").cloned().unwrap_or(Value::Null),
        "provider": bundle.get("provider").cloned().unwrap_or(Value::Null),
        "sourceDeviceId": bundle.get("sourceDeviceId").cloned().unwrap_or(Value::Null),
        "deviceId": bundle.get("deviceId").cloned().unwrap_or(Value::Null),
        "port": bundle.get("port").cloned().unwrap_or(Value::Null),
        "tuningBudgetSeconds": bundle.get("tuningBudgetSeconds").cloned().unwrap_or(Value::Null),
        "tuningTargetsC": bundle.get("tuningTargetsC").cloned().unwrap_or(Value::Null),
        "tuningExecutionOrderC": bundle.get("tuningExecutionOrderC").cloned().unwrap_or(Value::Null),
        "candidateDispositions": bundle.get("candidateDispositions").cloned().unwrap_or(Value::Null),
        "candidateReadyTargetsC": bundle.get("candidateReadyTargetsC").cloned().unwrap_or(Value::Null),
        "runs": bundle.get("reportRuns").cloned().unwrap_or_else(|| json!([])),
        "rawRuns": bundle.get("runs").cloned().unwrap_or_else(|| json!([])),
        "history": build_history(
            bundle
                .get("reportRuns")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ),
    });
    fs::write(&index_html_path, render_baseline_html(&html_data)?)?;
    Ok(bundle)
}

fn tuning_workflow(resolved_bank: &str) -> &'static str {
    match resolved_bank {
        "pps5a" => "five_amp_batch",
        "pps3a" => "three_amp_batch",
        _ => "thermal_batch",
    }
}

fn report_identity(selected_mode: &str, resolved_bank: &str) -> String {
    format!("{} / {resolved_bank}", selected_mode.to_uppercase())
}

fn set_to_vec(values: BTreeSet<i64>) -> Vec<i64> {
    values.into_iter().collect()
}

fn unique_i16_preserve_order(values: Vec<i16>) -> Vec<i16> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(*value))
        .collect()
}

fn i16_array_field(value: &Value, field: &str) -> Option<Vec<i16>> {
    let targets = value
        .get(field)?
        .as_array()?
        .iter()
        .filter_map(|item| item.as_i64())
        .filter_map(|target| i16::try_from(target).ok())
        .collect::<Vec<_>>();
    (!targets.is_empty()).then_some(targets)
}

fn ensure_candidate_receipt_fields(mut entry: Value) -> Value {
    let target_role = entry
        .get("targetRole")
        .and_then(Value::as_str)
        .unwrap_or("tuning")
        .to_string();
    let base_candidate_ready = entry
        .get("candidateReady")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            let has_point = entry
                .get("point")
                .or_else(|| entry.get("truthPoint"))
                .is_some_and(|point| !point.is_null());
            let valid_count = entry
                .get("validTestCount")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            target_role != "validation" && has_point && valid_count > 0
        });
    let metric_gate = candidate_metric_gate(&entry);
    let candidate_ready =
        target_role != "validation" && base_candidate_ready && metric_gate == Some(true);
    let metric_gate_failed = metric_gate != Some(true);
    if (target_role == "validation" || base_candidate_ready) && metric_gate_failed {
        entry["ok"] = json!(false);
    }
    entry["candidateReady"] = json!(candidate_ready);
    let existing_disposition = entry
        .get("candidateDisposition")
        .and_then(Value::as_str)
        .map(str::to_string);
    let budget_outcome = entry
        .get("budgetOutcome")
        .and_then(Value::as_str)
        .unwrap_or("");
    let ok = entry.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let disposition = if target_role == "validation"
        && metric_gate == Some(true)
        && (ok || budget_outcome == "validation_passed")
    {
        "validation_passed"
    } else if target_role == "validation"
        && (metric_gate_failed || budget_outcome == "validation_failed")
    {
        "validation_failed"
    } else if target_role == "validation" && budget_outcome == "budget_exhausted" {
        "validation_budget_exhausted"
    } else if (ok || budget_outcome == "completed") && candidate_ready {
        "acceptance_passed"
    } else if candidate_ready {
        "candidate_ready"
    } else if budget_outcome == "environment_blocked" {
        "environment_blocked"
    } else if budget_outcome == "budget_exhausted" {
        "budget_exhausted_without_candidate"
    } else {
        "not_available"
    };
    if existing_disposition.as_deref().is_none()
        || (target_role == "validation" && metric_gate_failed)
        || metric_gate == Some(false)
        || existing_disposition.as_deref() == Some("candidate_ready") && !candidate_ready
        || existing_disposition.as_deref() == Some("acceptance_passed") && !candidate_ready
    {
        entry["candidateDisposition"] = json!(disposition);
    }
    let final_disposition = entry
        .get("candidateDisposition")
        .and_then(Value::as_str)
        .unwrap_or(disposition);
    let review_passed = matches!(
        final_disposition,
        "acceptance_passed" | "validation_passed" | "candidate_ready"
    );
    entry["reviewPassed"] = json!(review_passed);
    entry["reviewOutcome"] = json!(if review_passed { "passed" } else { "failed" });
    entry
}

fn render_baseline_html(data: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let escaped_data = escape_report_html_value(data);
    let data_json = serde_json::to_string(&escaped_data)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let template = REPORT_TEMPLATE.replace("{{", "{").replace("}}", "}");
    Ok(template.replace(DATA_PLACEHOLDER, &data_json))
}

fn escape_report_html_value(value: &Value) -> Value {
    match value {
        Value::String(value) => Value::String(
            value
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#39;"),
        ),
        Value::Array(values) => Value::Array(values.iter().map(escape_report_html_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), escape_report_html_value(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ThermalLegacyReportInput, ThermalSelfTestReportInput, render_baseline_html,
        render_self_test_evidence_bundle, report_identity,
        rerender_legacy_preliminary_review_bundle, sanitize_non_finite_json_numbers,
        sanitize_point, tuning_workflow, write_preliminary_review_bundle,
    };
    use serde_json::{Value, json};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{env, fs, path::Path};

    fn embedded_report_data(bundle_dir: &Path) -> serde_json::Value {
        let html = fs::read_to_string(bundle_dir.join("index.html")).expect("index html");
        let data_start = html.find("const DATA=").expect("embedded data") + "const DATA=".len();
        let data_end = html[data_start..]
            .find(";\n  const COLORS")
            .expect("embedded data terminator")
            + data_start;
        serde_json::from_str(&html[data_start..data_end]).expect("valid embedded report data")
    }

    #[test]
    fn sanitize_non_finite_json_numbers_replaces_bare_tokens_only() {
        let input = r#"{"ok":[Infinity,-Infinity,NaN],"label":"Infinity","nested":{"x":Infinity}}"#;
        let sanitized = sanitize_non_finite_json_numbers(input);
        assert_eq!(
            sanitized,
            r#"{"ok":[null,null,null],"label":"Infinity","nested":{"x":null}}"#
        );
    }

    #[test]
    fn legacy_rerender_point_keeps_point_local_warmup_reentry() {
        let point = sanitize_point(
            Some(&json!({
                "targetTempC": 140,
                "warmupReenterCentiC": 875,
            })),
            Some(140),
        )
        .expect("sanitized point");

        assert_eq!(point["warmupReenterCentiC"], json!(875));
    }

    #[test]
    fn legacy_rerender_does_not_invent_missing_hardware_metadata() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = env::temp_dir().join(format!("thermal-legacy-metadata-test-{unique}"));
        let input_dir = root.join("input");
        let output_dir = root.join("output");
        fs::create_dir_all(&input_dir).expect("create input directory");
        fs::write(
            input_dir.join("run.bundle.json"),
            serde_json::to_vec(&json!({
                "kind": "thermal_self_test_preliminary_bundle",
                "runs": []
            }))
            .expect("serialize bundle"),
        )
        .expect("write bundle");
        fs::write(
            input_dir.join("thermal-profile.accepted.json"),
            br#"{"settings":{},"points":[]}"#,
        )
        .expect("write profile");
        fs::write(input_dir.join("samples.ndjson"), b"").expect("write samples");

        rerender_legacy_preliminary_review_bundle(ThermalLegacyReportInput {
            legacy_bundle_dir: input_dir,
            output_dir: Some(output_dir.clone()),
        })
        .expect("rerender legacy bundle");

        let bundle: Value = serde_json::from_slice(
            &fs::read(output_dir.join("run.bundle.json")).expect("read output bundle"),
        )
        .expect("parse output bundle");
        for field in [
            "port",
            "selectedMode",
            "resolvedBank",
            "detectedSourceClass",
            "sourcePreset",
            "provider",
        ] {
            assert_eq!(bundle[field], json!("unknown"), "field {field}");
        }
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn self_test_renderer_preserves_completed_pps3a_thermal_plant_evidence() {
        let dir = tempfile::tempdir().expect("temp dir");
        let run_dir = dir.path().join("raw-self-test");
        let output_dir = dir.path().join("html-bundle");
        fs::create_dir_all(&run_dir).expect("run dir");
        fs::write(
            run_dir.join("run.json"),
            serde_json::to_vec_pretty(&json!({
                "kind": "thermal_self_test",
                "runId": "thermal-pps3a-240",
                "complete": true,
                "target": {"deviceId": "serial-303a-1001-A0:F2:62:F2:0D:6C"},
                "source": {
                    "kind": "isolapurr",
                    "deviceId": "f293cc9c139e",
                    "selectedMode": "65w",
                    "resolvedBank": "pps3a",
                    "detectedSourceClass": "pps3a",
                    "preset": {"voltageMv": 20000, "currentLimitMa": 3250}
                },
                "parameters": {"targetsC": [240], "holdSeconds": 60},
                "validation": {"expectedTargetsC": [240], "passed": true, "failures": []},
                "applied": [{
                    "targetTempC": 240,
                    "stopReason": "completed",
                    "maxOvershootC": 1.28,
                    "holdPeakToPeakC": 2.01,
                    "fullSpeedToStable": {"limitMs": 5000, "settleTimeMs": 0, "failureReason": null},
                    "analysis": {
                        "holdMedianOutputPermille": 1000,
                        "holdP90OutputPermille": 1000,
                        "approachSource": {"powerMw": {"avg": 51694}},
                        "holdSource": {"powerMw": {"avg": 54241}}
                    }
                }]
            }))
            .expect("write summary"),
        )
        .expect("summary file");
        let samples = [
            json!({
                "targetTempC": 240,
                "testPhase": "applied",
                "elapsedMs": 0,
                "phase": "warmup",
                "heaterTelemetry": {"heaterOutputPercent": 100},
                "status": {"currentTempC": 28.7, "heaterControlTempC": 28.7, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 100, "pdRequestMv": 20000, "thermalPlantModel": {"state": "active", "projectionValid": true}},
                "sourceTelemetry": {"voltageMv": 20000, "currentMa": 2700, "powerMw": 54000}
            }),
            json!({
                "targetTempC": 240,
                "testPhase": "applied",
                "elapsedMs": 1000,
                "phase": "approach",
                "heaterTelemetry": {"heaterOutputPercent": 100},
                "status": {"currentTempC": 230.0, "heaterControlTempC": 230.0, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 100, "pdRequestMv": 20000, "thermalPlantModel": {"state": "active", "projectionValid": true}},
                "sourceTelemetry": {"voltageMv": 20000, "currentMa": 2700, "powerMw": 54000}
            }),
            json!({
                "targetTempC": 240,
                "testPhase": "applied",
                "elapsedMs": 2000,
                "phase": "hold",
                "heaterTelemetry": {"heaterOutputPercent": 100},
                "status": {"currentTempC": 239.0, "heaterControlTempC": 239.0, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 93, "pdRequestMv": 20000, "thermalPlantModel": {"state": "active", "projectionValid": true}},
                "sourceTelemetry": {"voltageMv": 20008, "currentMa": 2859, "powerMw": 57199}
            }),
            json!({
                "targetTempC": 240,
                "testPhase": "applied",
                "elapsedMs": 12000,
                "phase": "hold",
                "heaterTelemetry": {"heaterOutputPercent": 100},
                "status": {"currentTempC": 240.2, "heaterControlTempC": 240.2, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 93, "pdRequestMv": 20000, "thermalPlantModel": {"state": "active", "projectionValid": true}},
                "sourceTelemetry": {"voltageMv": 20008, "currentMa": 2859, "powerMw": 57199}
            }),
        ];
        fs::write(
            run_dir.join("samples.ndjson"),
            samples
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .expect("serialize samples")
                .join("\n")
                + "\n",
        )
        .expect("sample file");

        let result = render_self_test_evidence_bundle(ThermalSelfTestReportInput {
            run_dirs: vec![run_dir],
            output_dir: Some(output_dir.clone()),
        })
        .expect("render self-test evidence");
        let bundle: Value = serde_json::from_slice(
            &fs::read(output_dir.join("run.bundle.json")).expect("read bundle"),
        )
        .expect("parse bundle");
        let model: Value = serde_json::from_slice(
            &fs::read(output_dir.join("thermal-profile.accepted.json")).expect("read model"),
        )
        .expect("parse model");

        assert_eq!(result["ok"], true);
        assert_eq!(bundle["detectedSourceClass"], "pps3a");
        assert_eq!(bundle["sourceDeviceId"], "f293cc9c139e");
        assert_eq!(bundle["sourcePreset"], "20V / 3.25A PPS auto-follow");
        assert_eq!(bundle["reportRuns"][0]["target"], 240);
        assert_eq!(bundle["reportRuns"][0]["targetRole"], "validation");
        assert_eq!(bundle["reportRuns"][0]["reviewPassed"], true);
        assert_eq!(
            bundle["reportRuns"][0]["result"]["fullSpeedToStable"]["warmupExitedAtMs"],
            1_000
        );
        assert_eq!(
            bundle["reportRuns"][0]["result"]["fullSpeedToStable"]["settleTimeMs"],
            1_000
        );
        assert_eq!(bundle["reportRuns"][0]["holdCheck"]["holdSeconds"], 60);
        assert_eq!(model["profileCompatibility"], "not_a_point_local_profile");
        assert_eq!(model["model"]["state"], "active");
        assert!(output_dir.join("index.html").is_file());
        let html = fs::read_to_string(output_dir.join("index.html")).expect("report html");
        assert!(html.contains("full-speed 实测"));
        assert!(!html.contains("逼近阶段"));
        assert_eq!(
            fs::read_to_string(output_dir.join("samples.ndjson"))
                .expect("report samples")
                .lines()
                .count(),
            4
        );
    }

    #[test]
    fn report_workflow_follows_the_resolved_profile_bank() {
        assert_eq!(tuning_workflow("pps5a"), "five_amp_batch");
        assert_eq!(tuning_workflow("pps3a"), "three_amp_batch");
        assert_eq!(report_identity("100w", "pps5a"), "100W / pps5a");
        assert_eq!(report_identity("65w", "pps3a"), "65W / pps3a");
    }

    #[test]
    fn report_html_escapes_embedded_json_script_terminators() {
        let data = json!({"label": "</script><script>alert('x')</script>&\u{2028}\u{2029}"});
        let html = render_baseline_html(&data).expect("report html");

        assert!(!html.contains("</script><script>alert('x')</script>"));
        assert!(html.contains("\\u0026lt;/script\\u0026gt;"));
        let data_start = html.find("const DATA=").expect("embedded data") + "const DATA=".len();
        let data_end = html[data_start..]
            .find(";\n  const COLORS")
            .expect("embedded data terminator")
            + data_start;
        let decoded: Value = serde_json::from_str(&html[data_start..data_end]).expect("valid JSON");
        assert_eq!(
            decoded,
            json!({"label": "&lt;/script&gt;&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;\u{2028}\u{2029}"})
        );
    }

    #[test]
    fn candidate_ready_requires_complete_hard_metric_evidence() {
        let entry = super::ensure_candidate_receipt_fields(json!({
            "target": 100,
            "targetRole": "tuning",
            "candidateReady": true,
            "candidateDisposition": "candidate_ready",
            "budgetOutcome": "budget_exhausted",
            "validTestCount": 4,
            "point": {"targetTempC": 100}
        }));

        assert_eq!(entry["candidateReady"], json!(false));
        assert_eq!(entry["reviewOutcome"], json!("failed"));
        assert_eq!(
            entry["candidateDisposition"],
            json!("budget_exhausted_without_candidate")
        );
    }

    #[test]
    fn validation_review_cannot_pass_when_full_speed_gate_fails() {
        let entry = super::ensure_candidate_receipt_fields(json!({
            "target": 220,
            "targetRole": "validation",
            "ok": true,
            "candidateReady": false,
            "candidateDisposition": "validation_passed",
            "budgetOutcome": "validation_passed",
            "result": {
                "stopReason": "completed",
                "maxOvershootC": 1.0,
                "holdPeakToPeakC": 1.0,
                "fullSpeedToStable": {
                    "settleTimeMs": 6_000,
                    "limitMs": 5_000
                }
            }
        }));

        assert_eq!(entry["ok"], json!(false));
        assert_eq!(entry["candidateDisposition"], json!("validation_failed"));
        assert_eq!(entry["reviewPassed"], json!(false));
        assert_eq!(entry["reviewOutcome"], json!("failed"));
    }

    #[test]
    fn preliminary_review_bundle_keeps_single_target_when_raw_entry_uses_legacy_validation_role() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir = env::temp_dir().join(format!("thermal-report-validation-test-{unique}"));
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [
                    {"targetTempC": 60, "holdPowerPermille": 400},
                    {"targetTempC": 100, "holdPowerPermille": 500}
                ],
            }),
            vec![json!({
                "target": 80,
                "targetTempC": 80,
                "targetRole": "validation",
                "ok": true,
                "candidateReady": false,
                "candidateDisposition": "validation_passed",
                "budgetOutcome": "validation_passed",
                "validTestCount": 1,
                "result": {
                    "stopReason": "completed",
                    "maxOvershootC": 1.0,
                    "holdPeakToPeakC": 1.0,
                    "fullSpeedToStable": {
                        "settleTimeMs": 2_000,
                        "limitMs": 10_000
                    }
                },
                "samples": [{
                    "t": 0.0,
                    "temp": 78.0,
                    "temperature": {
                        "humanTempC": 78.0,
                        "filteredTempC": 77.9,
                        "controlTempC": 77.8
                    }
                }],
                "rounds": [{
                    "round": 1,
                    "attemptType": "validation",
                    "candidateName": "final-profile",
                    "selected": true,
                    "evidenceValid": true,
                    "samples": [{
                        "t": 0.0,
                        "temp": 78.0,
                        "temperature": {
                            "humanTempC": 78.0,
                            "filteredTempC": 77.9,
                            "controlTempC": 77.8
                        }
                    }]
                }, {
                    "round": 2,
                    "attemptType": "validation_retry",
                    "candidateName": "final-profile",
                    "selected": false,
                    "evidenceValid": false,
                    "evidenceInvalidReason": "source_telemetry_stale",
                    "samples": [{
                        "t": 1.0,
                        "temp": 77.5
                    }]
                }]
            })],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[80],
            &[80],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["tuningTargetsC"], json!([80]));
        assert_eq!(bundle["tuningExecutionOrderC"], json!([80]));
        assert_eq!(bundle["runs"][0]["targetRole"], "validation");
        assert_eq!(bundle["reportRuns"][0]["targetRole"], "validation");
        assert_eq!(bundle["reportRuns"][0]["reviewOutcome"], "passed");
        assert_eq!(bundle["reportRuns"][0]["reviewPassed"], true);

        let samples = fs::read_to_string(bundle_dir.join("samples.ndjson")).expect("samples");
        assert!(samples.contains(r#""targetTempC":80"#));
        let sample_lines = samples
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("sample json"))
            .collect::<Vec<_>>();
        assert_eq!(sample_lines.len(), 2);
        assert_eq!(sample_lines[0]["evidenceValid"], true);
        assert_eq!(sample_lines[1]["evidenceValid"], false);
        assert_eq!(
            sample_lines[1]["evidenceInvalidReason"],
            "source_telemetry_stale"
        );

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(embedded_data["runs"][0]["reviewOutcome"], "passed");
        assert_eq!(
            embedded_data["runs"][0]["samples"][0]["temperature"]["humanTempC"],
            json!(78.0)
        );

        let _ = fs::remove_dir_all(bundle_dir);
    }

    #[test]
    fn preliminary_review_bundle_maps_supplemental_candidate_ready_to_passed() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir = env::temp_dir().join(format!("thermal-report-supplemental-test-{unique}"));
        let sample = json!({
            "t": 0.0,
            "temp": 120.0,
            "phase": "hold",
            "command": 60,
            "output": 60,
            "requestV": 12.0,
            "sourcePowerW": 20.0,
            "temperature": {
                "humanTempC": 120.0,
                "filteredTempC": 119.8,
                "controlTempC": 119.7
            }
        });
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [
                    {"targetTempC": 120, "holdPowerPermille": 500}
                ],
            }),
            vec![
                json!({
                    "target": 120,
                    "targetTempC": 120,
                    "targetRole": "validation",
                    "ok": false,
                    "candidateReady": false,
                    "candidateDisposition": "validation_failed",
                    "budgetOutcome": "validation_failed",
                    "validTestCount": 1,
                    "samples": [sample.clone()],
                    "rounds": []
                }),
                json!({
                    "target": 120,
                    "targetTempC": 120,
                    "targetRole": "supplemental_tuning",
                    "ok": false,
                    "candidateReady": true,
                    "candidateDisposition": "candidate_ready",
                    "budgetOutcome": "budget_exhausted",
                    "validTestCount": 2,
                    "samples": [sample],
                    "rounds": [],
                    "result": {
                        "stopReason": "completed",
                        "maxOvershootC": 1.2,
                        "holdPeakToPeakC": 1.8,
                        "fullSpeedToStable": {
                            "limitMs": 10000,
                            "settleTimeMs": 6500,
                            "failureReason": null
                        }
                    }
                }),
            ],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[120],
            &[120],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["tuningTargetsC"], json!([120]));
        assert_eq!(bundle["tuningExecutionOrderC"], json!([120]));
        assert_eq!(bundle["candidateReadyTargetsC"], json!([120]));
        assert_eq!(bundle["runs"].as_array().expect("raw runs").len(), 2);
        assert_eq!(bundle["targets"].as_array().expect("raw targets").len(), 2);
        assert_eq!(
            bundle["reportRuns"].as_array().expect("report runs").len(),
            1
        );
        assert_eq!(bundle["reportRuns"][0]["target"], json!(120));
        assert_eq!(
            bundle["reportRuns"][0]["targetRole"],
            json!("supplemental_tuning")
        );
        assert_eq!(bundle["reportRuns"][0]["reviewOutcome"], json!("passed"));
        assert_eq!(bundle["reportRuns"][0]["reviewPassed"], json!(true));
        assert_eq!(
            bundle["reportRuns"][0]["auditEntries"]
                .as_array()
                .expect("audit entries")
                .len(),
            2
        );
        assert!(
            bundle["reportRuns"][0]["auditEntries"][0]
                .get("samples")
                .is_none()
        );
        assert!(
            bundle["reportRuns"][0]["auditEntries"][0]
                .get("rounds")
                .is_none()
        );
        assert_eq!(
            bundle["reportRuns"][0]["auditSummary"][0]["targetRole"],
            json!("validation")
        );
        assert_eq!(
            bundle["reportRuns"][0]["auditSummary"][1]["targetRole"],
            json!("supplemental_tuning")
        );

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(
            embedded_data["runs"].as_array().expect("html runs").len(),
            1
        );
        assert_eq!(embedded_data["runs"][0]["target"], json!(120));
        assert_eq!(
            embedded_data["runs"][0]["targetRole"],
            json!("supplemental_tuning")
        );
        assert_eq!(embedded_data["runs"][0]["reviewOutcome"], json!("passed"));
        assert_eq!(embedded_data["runs"][0]["reviewPassed"], json!(true));
        let html = fs::read_to_string(bundle_dir.join("index.html")).expect("index html");
        assert!(!html.contains("审计分类"));
        assert!(!html.contains("审计路径"));
        assert!(!html.contains("候选状态"));
        assert_eq!(
            embedded_data["rawRuns"]
                .as_array()
                .expect("html raw runs")
                .len(),
            2
        );
        assert_eq!(embedded_data["rawRuns"][0]["target"], json!(120));
        assert_eq!(
            embedded_data["rawRuns"][0]["targetRole"],
            json!("validation")
        );
        assert_eq!(
            embedded_data["rawRuns"][0]["samples"]
                .as_array()
                .expect("validation samples")
                .len(),
            1
        );
        assert_eq!(
            embedded_data["rawRuns"][1]["targetRole"],
            json!("supplemental_tuning")
        );
        assert_eq!(
            embedded_data["rawRuns"][1]["samples"]
                .as_array()
                .expect("supplemental samples")
                .len(),
            1
        );

        let _ = fs::remove_dir_all(bundle_dir);
    }

    #[test]
    fn preliminary_review_bundle_sorts_report_targets_by_temperature() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir = env::temp_dir().join(format!("thermal-report-sorted-test-{unique}"));
        let sample_for = |target: i64| {
            json!({
                "t": 0.0,
                "temp": target,
                "phase": "hold",
                "command": 50,
                "output": 50
            })
        };
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [
                    {"targetTempC": 60, "holdPowerPermille": 400},
                    {"targetTempC": 80, "holdPowerPermille": 450},
                    {"targetTempC": 100, "holdPowerPermille": 500}
                ],
            }),
            vec![
                json!({
                    "target": 60,
                    "targetTempC": 60,
                    "targetRole": "tuning",
                    "budgetOutcome": "completed",
                    "samples": [sample_for(60)],
                    "rounds": []
                }),
                json!({
                    "target": 100,
                    "targetTempC": 100,
                    "targetRole": "tuning",
                    "budgetOutcome": "completed",
                    "samples": [sample_for(100)],
                    "rounds": []
                }),
                json!({
                    "target": 80,
                    "targetTempC": 80,
                    "targetRole": "validation",
                    "budgetOutcome": "validation_failed",
                    "candidateDisposition": "validation_failed",
                    "samples": [sample_for(80)],
                    "rounds": []
                }),
                json!({
                    "target": 80,
                    "targetTempC": 80,
                    "targetRole": "supplemental_tuning",
                    "budgetOutcome": "completed",
                    "samples": [sample_for(80)],
                    "rounds": []
                }),
            ],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[60, 80, 100],
            &[60, 100, 80],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["tuningTargetsC"], json!([60, 80, 100]));
        assert_eq!(bundle["tuningExecutionOrderC"], json!([60, 100, 80]));
        assert_eq!(
            bundle["reportRuns"]
                .as_array()
                .expect("report runs")
                .iter()
                .map(|entry| entry["target"].clone())
                .collect::<Vec<_>>(),
            vec![json!(60), json!(80), json!(100)]
        );
        assert_eq!(
            bundle["runs"]
                .as_array()
                .expect("raw runs")
                .iter()
                .map(|entry| entry["target"].clone())
                .collect::<Vec<_>>(),
            vec![json!(60), json!(100), json!(80), json!(80)]
        );

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(
            embedded_data["runs"]
                .as_array()
                .expect("html runs")
                .iter()
                .map(|entry| entry["target"].clone())
                .collect::<Vec<_>>(),
            vec![json!(60), json!(80), json!(100)]
        );
        assert_eq!(
            embedded_data["rawRuns"]
                .as_array()
                .expect("html raw runs")
                .iter()
                .map(|entry| entry["target"].clone())
                .collect::<Vec<_>>(),
            vec![json!(60), json!(100), json!(80), json!(80)]
        );

        let _ = fs::remove_dir_all(bundle_dir);
    }

    #[test]
    fn preliminary_review_bundle_materializes_placeholders_and_omits_tier_fields() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir = env::temp_dir().join(format!("thermal-report-placeholder-test-{unique}"));
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [
                    {"targetTempC": 60, "holdPowerPermille": 400},
                    {"targetTempC": 140, "holdPowerPermille": 600}
                ],
            }),
            vec![
                json!({
                    "target": 60,
                    "targetTempC": 60,
                    "targetRole": "tuning",
                    "budgetOutcome": "completed",
                    "candidateDisposition": "acceptance_passed",
                    "candidateReady": true,
                    "samples": [{"t": 0.0, "temp": 60.0}],
                    "rounds": [],
                    "result": {
                        "stopReason": "completed",
                        "maxOvershootC": 1.0,
                        "holdPeakToPeakC": 1.5,
                        "fullSpeedToStable": {
                            "limitMs": 10000,
                            "settleTimeMs": 7000,
                            "failureReason": null
                        }
                    }
                }),
                json!({
                    "target": 140,
                    "targetTempC": 140,
                    "targetRole": "tuning",
                    "budgetOutcome": "budget_exhausted",
                    "candidateDisposition": "budget_exhausted_without_candidate",
                    "candidateReady": false,
                    "samples": [{"t": 0.0, "temp": 140.0}],
                    "rounds": []
                }),
            ],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[60, 100, 140],
            &[60, 140],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["tuningTargetsC"], json!([60, 100, 140]));
        assert_eq!(bundle["tuningExecutionOrderC"], json!([60, 140]));
        assert!(bundle.get("anchorTargetsC").is_none());
        assert!(bundle.get("validationTargetsC").is_none());
        assert!(bundle.get("supplementalTuningTargetsC").is_none());
        assert_eq!(
            bundle["reportRuns"]
                .as_array()
                .expect("report runs")
                .iter()
                .map(|entry| entry["target"].clone())
                .collect::<Vec<_>>(),
            vec![json!(60), json!(100), json!(140)]
        );
        assert_eq!(
            bundle["reportRuns"][1]["budgetOutcome"],
            "not_executed_without_accepted_bounds"
        );
        assert_eq!(bundle["reportRuns"][1]["reviewOutcome"], "failed");
        assert_eq!(bundle["candidateReadyTargetsC"], json!([60]));

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(embedded_data["tuningTargetsC"], json!([60, 100, 140]));
        assert_eq!(embedded_data["tuningExecutionOrderC"], json!([60, 140]));

        let _ = fs::remove_dir_all(bundle_dir);
    }

    #[test]
    fn preliminary_review_bundle_drops_metric_failed_candidate_ready() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir = env::temp_dir().join(format!("thermal-report-metric-gate-test-{unique}"));
        let sample = json!({
            "t": 0.0,
            "temp": 100.0,
            "phase": "hold",
            "command": 80,
            "output": 80,
            "requestV": 18.0
        });
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [{"targetTempC": 100, "holdPowerPermille": 500}],
            }),
            vec![json!({
                "target": 100,
                "targetTempC": 100,
                "targetRole": "tuning",
                "ok": false,
                "candidateReady": true,
                "candidateDisposition": "candidate_ready",
                "budgetOutcome": "budget_exhausted",
                "validTestCount": 11,
                "samples": [sample],
                "rounds": [],
                "point": {
                    "targetTempC": 100,
                    "holdPowerPermille": 500
                },
                "result": {
                    "stopReason": "full_speed_to_stable_timeout",
                    "maxOvershootC": 9.04,
                    "holdPeakToPeakC": 11.88,
                    "fullSpeedToStable": {
                        "limitMs": 10000,
                        "settleTimeMs": null,
                        "failureReason": "full_speed_to_stable_timeout"
                    }
                }
            })],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[100],
            &[100],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["runs"][0]["candidateReady"], json!(false));
        assert_eq!(
            bundle["runs"][0]["candidateDisposition"],
            json!("budget_exhausted_without_candidate")
        );
        assert_eq!(bundle["candidateReadyTargetsC"], json!([]));
        assert_eq!(
            bundle["reportRuns"][0]["candidateDisposition"],
            json!("budget_exhausted_without_candidate")
        );
        assert_eq!(bundle["reportRuns"][0]["reviewOutcome"], json!("failed"));
        assert_eq!(bundle["reportRuns"][0]["reviewPassed"], json!(false));

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(embedded_data["runs"][0]["candidateReady"], json!(false));
        assert_eq!(
            embedded_data["runs"][0]["candidateDisposition"],
            json!("budget_exhausted_without_candidate")
        );
        assert_eq!(embedded_data["runs"][0]["reviewOutcome"], json!("failed"));
        assert_eq!(embedded_data["runs"][0]["reviewPassed"], json!(false));

        let _ = fs::remove_dir_all(bundle_dir);
    }

    #[test]
    fn preliminary_review_bundle_maps_candidate_ready_to_passed() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let bundle_dir =
            env::temp_dir().join(format!("thermal-report-candidate-ready-fail-test-{unique}"));
        let sample = json!({
            "t": 0.0,
            "temp": 60.0,
            "phase": "hold",
            "command": 20,
            "output": 20,
            "requestV": 6.5
        });
        let bundle = write_preliminary_review_bundle(
            &bundle_dir,
            &json!({
                "settings": {},
                "points": [{"targetTempC": 60, "holdPowerPermille": 135}],
            }),
            vec![json!({
                "target": 60,
                "targetTempC": 60,
                "targetRole": "tuning",
                "ok": false,
                "candidateReady": true,
                "candidateDisposition": "candidate_ready",
                "budgetOutcome": "budget_exhausted",
                "validTestCount": 6,
                "samples": [sample],
                "rounds": [],
                "point": {
                    "targetTempC": 60,
                    "holdPowerPermille": 135
                },
                "result": {
                    "stopReason": "completed",
                    "maxOvershootC": 0.56,
                    "holdPeakToPeakC": 1.57,
                    "fullSpeedToStable": {
                        "limitMs": 10000,
                        "settleTimeMs": 7798,
                        "failureReason": null
                    }
                }
            })],
            "f293cc9c139e",
            "mock-fp-lab-01",
            "/dev/cu.usbmodem2111401",
            1200,
            json!(1234567890),
            "100w",
            "pps5a",
            "pps5a",
            &[60],
            &[60],
            "21V / 5.0A",
            "IsolaPurr",
        )
        .expect("bundle");

        assert_eq!(bundle["runs"][0]["candidateReady"], json!(true));
        assert_eq!(
            bundle["runs"][0]["candidateDisposition"],
            json!("candidate_ready")
        );
        assert_eq!(bundle["candidateReadyTargetsC"], json!([60]));
        assert_eq!(bundle["reportRuns"][0]["reviewOutcome"], json!("passed"));
        assert_eq!(bundle["reportRuns"][0]["reviewPassed"], json!(true));

        let embedded_data = embedded_report_data(&bundle_dir);
        assert_eq!(embedded_data["runs"][0]["candidateReady"], json!(true));
        assert_eq!(
            embedded_data["runs"][0]["candidateDisposition"],
            json!("candidate_ready")
        );
        assert_eq!(embedded_data["runs"][0]["reviewOutcome"], json!("passed"));
        assert_eq!(embedded_data["runs"][0]["reviewPassed"], json!(true));

        let _ = fs::remove_dir_all(bundle_dir);
    }
}
