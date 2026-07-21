use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value, json};

const DEFAULT_PROFILE_MODE: &str = "100w";
const DEFAULT_RESOLVED_BANK: &str = "pps5a";
const DEFAULT_DETECTED_SOURCE_CLASS: &str = "pps5a";
const DEFAULT_SOURCE_PRESET: &str = "21V / 5.0A";
const DEFAULT_PROVIDER: &str = "IsolaPurr";
const DEFAULT_PORT_PATH: &str = "/dev/cu.usbmodem2111401";
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
        .unwrap_or(DEFAULT_PORT_PATH)
        .to_string();

    let selected_mode = legacy_bundle
        .get("selectedMode")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROFILE_MODE)
        .to_string();
    let resolved_bank = legacy_bundle
        .get("resolvedBank")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_RESOLVED_BANK)
        .to_string();
    let detected_source_class = legacy_bundle
        .get("detectedSourceClass")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_DETECTED_SOURCE_CLASS)
        .to_string();
    let source_preset = legacy_bundle
        .get("sourcePreset")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_SOURCE_PRESET)
        .to_string();
    let provider = legacy_bundle
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROVIDER)
        .to_string();
    let generated_at = legacy_bundle
        .get("generatedAt")
        .cloned()
        .unwrap_or_else(|| json!(current_unix_millis()));

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
        "flagshipTargetsC": bundle.get("flagshipTargetsC").cloned().unwrap_or(Value::Null),
    }))
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
        .find(|token| bytes[index..].starts_with(**token))
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
                    .filter_map(|sample| {
                        let source = sample.get("sourceTelemetry").and_then(Value::as_object);
                        Some(json!({
                            "t": round_decimal(sample.get("elapsedMs").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0, 3),
                            "temp": sample.get("currentTempC").cloned().unwrap_or(Value::Null),
                            "filtered": sample.get("heaterFilteredTempC").cloned().unwrap_or(Value::Null),
                            "command": sample.get("heaterOutputPercent").cloned().unwrap_or(Value::Null),
                            "output": sample.get("heaterPhysicalOutputPercent").cloned().unwrap_or(Value::Null),
                            "requestV": Value::Null,
                            "phase": sample.get("heaterControlPhase").cloned().unwrap_or(Value::Null),
                            "sourceVoltageV": number_to_json(source.and_then(|payload| payload.get("voltageMv").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                            "sourceCurrentA": number_to_json(source.and_then(|payload| payload.get("currentMa").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                            "sourcePowerW": number_to_json(source.and_then(|payload| payload.get("powerMw").and_then(Value::as_f64)).map(|value| round_decimal(value / 1000.0, 3))),
                        }))
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
    source_preset: &str,
    provider: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let entries: Vec<Value> = entries.iter().map(sort_entry_samples).collect();
    fs::create_dir_all(bundle_dir)?;
    let samples_path = bundle_dir.join("samples.ndjson");
    let mut sample_lines = String::new();
    for entry in &entries {
        if let Some(rounds) = entry.get("rounds").and_then(Value::as_array) {
            if !rounds.is_empty() {
                for attempt in rounds {
                    if attempt.get("evidenceValid").and_then(Value::as_bool) == Some(false) {
                        continue;
                    }
                    for sample in attempt
                        .get("samples")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let mut enriched = sample.clone();
                        enriched["targetTempC"] =
                            entry.get("target").cloned().unwrap_or(Value::Null);
                        enriched["attemptNumber"] =
                            attempt.get("round").cloned().unwrap_or(Value::Null);
                        enriched["attemptType"] =
                            attempt.get("attemptType").cloned().unwrap_or(Value::Null);
                        enriched["candidateName"] =
                            attempt.get("candidateName").cloned().unwrap_or(Value::Null);
                        enriched["selected"] = attempt
                            .get("selected")
                            .cloned()
                            .unwrap_or_else(|| json!(false));
                        sample_lines.push_str(&serde_json::to_string(&enriched)?);
                        sample_lines.push('\n');
                    }
                }
                continue;
            }
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
    let flagship_targets_c = entries
        .iter()
        .filter_map(|entry| entry.get("target").and_then(Value::as_i64))
        .collect::<Vec<_>>();

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
        "flagshipTargetsC": flagship_targets_c,
        "sourcePreset": source_preset,
        "provider": provider,
        "sourceDeviceId": source_id,
        "deviceId": device_id,
        "port": port_path,
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
        .get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("target").and_then(Value::as_i64))
        .map(|target| format!("{target}°C"))
        .collect::<Vec<_>>()
        .join(" / ");
    let html_data = json!({
        "generatedAt": bundle.get("generatedAt").cloned().unwrap_or(Value::Null),
        "title": format!("Flux Purr 100W / pps5a {target_label} preliminary review"),
        "subtitle": format!("当前只收口 {target_label}。full-speed-to-stable 按目标温度使用动态门槛：≤150°C 为 10s，>150°C 为 5s；轮次详情展示全部有效调参轮次、预算结果与 hold confirm。"),
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
        "runs": bundle.get("runs").cloned().unwrap_or_else(|| json!([])),
        "history": build_history(
            bundle
                .get("runs")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ),
    });
    fs::write(&index_html_path, render_baseline_html(&html_data)?)?;
    Ok(bundle)
}

fn render_baseline_html(data: &Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let data_json = serde_json::to_string(data)?;
    Ok(REPORT_TEMPLATE
        .replace(DATA_PLACEHOLDER, &data_json)
        .replace("{{", "{")
        .replace("}}", "}"))
}

#[cfg(test)]
mod tests {
    use super::sanitize_non_finite_json_numbers;

    #[test]
    fn sanitize_non_finite_json_numbers_replaces_bare_tokens_only() {
        let input = r#"{"ok":[Infinity,-Infinity,NaN],"label":"Infinity","nested":{"x":Infinity}}"#;
        let sanitized = sanitize_non_finite_json_numbers(input);
        assert_eq!(
            sanitized,
            r#"{"ok":[null,null,null],"label":"Infinity","nested":{"x":null}}"#
        );
    }
}
