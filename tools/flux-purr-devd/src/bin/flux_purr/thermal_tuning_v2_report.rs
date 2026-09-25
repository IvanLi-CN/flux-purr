use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const DATA_PLACEHOLDER: &str = "__THERMAL_TUNING_V2_DATA__";
const REPORT_TEMPLATE: &str = include_str!("thermal_tuning_v2_report_template.html");
const ALLOWED_EVENT_KINDS: &[&str] = &[
    "sample",
    "phase_transition",
    "candidate_trial",
    "decision",
    "safety",
];
const REQUIRED_SAMPLE_FIELDS: &[&str] = &[
    "sequence",
    "elapsedMs",
    "kind",
    "targetC",
    "trialIndex",
    "candidateId",
    "candidateHash",
    "temperatureCentiC",
    "vinMv",
    "ppsContractMv",
    "ppsContractMa",
    "ppsContractKind",
    "ppsContractConfirmed",
    "ppsRequestedMv",
    "ppsPowerCeilingMv",
    "heaterRequestedOutputPermille",
    "heaterOutputPermille",
    "measurementValid",
    "phase",
];

#[derive(Debug, Clone)]
pub(super) struct ThermalTuningV2ReportInput {
    pub(super) input_dir: PathBuf,
    pub(super) output_dir: Option<PathBuf>,
}

pub(super) fn render_v2_bundle(
    input: ThermalTuningV2ReportInput,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let input_dir = absolute_path(&input.input_dir)?;
    let output_dir = absolute_path(
        &input
            .output_dir
            .unwrap_or_else(|| input_dir.join("rendered")),
    )?;
    if input_dir == output_dir {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "v2 report output directory must be different from the input directory",
        )
        .into());
    }

    let mut bundle = read_json(&input_dir.join("run.bundle.json"))?;
    let samples = read_ndjson(&input_dir.join("samples.ndjson"))?;
    let ledger = read_ndjson(&input_dir.join("decision-ledger.ndjson"))?;
    let candidate = read_json(&input_dir.join("thermal-profile.candidate.json"))?;
    validate_bundle(&bundle)?;
    validate_candidate(&bundle, &candidate)?;

    let event_audit = audit_events(&samples, &ledger);
    let report_runs = normalized_report_runs(&bundle, &samples)?;
    bundle["reportRuns"] = Value::Array(report_runs.clone());
    bundle["rawRuns"] = bundle
        .get("rawRuns")
        .cloned()
        .unwrap_or_else(|| Value::Array(report_runs.clone()));
    bundle["trace"] = event_audit.trace.clone();
    bundle["eventCounts"] = event_audit.counts.clone();
    bundle["generatedBy"] = json!("flux-purr thermal report render-v2");
    bundle["files"] = json!({
        "index.html": "index.html",
        "run.bundle.json": "run.bundle.json",
        "samples.ndjson": "samples.ndjson",
        "thermal-profile.candidate.json": "thermal-profile.candidate.json",
        "decision-ledger.ndjson": "decision-ledger.ndjson"
    });

    let all_runs_passed = report_runs.iter().all(|run| {
        run.get("reviewOutcome").and_then(Value::as_str) == Some("passed")
            && run.get("reviewPassed").and_then(Value::as_bool) == Some(true)
    });
    let trace_complete = event_audit.trace.get("complete").and_then(Value::as_bool) == Some(true);
    if trace_complete && all_runs_passed {
        if bundle
            .get("reviewDisposition")
            .and_then(Value::as_str)
            .is_none()
        {
            bundle["reviewDisposition"] = json!("passed");
        }
        if bundle
            .get("terminalDisposition")
            .and_then(Value::as_str)
            .is_none()
        {
            bundle["terminalDisposition"] = json!("completed");
        }
    } else {
        bundle["reviewDisposition"] = json!("review_incomplete");
        bundle["terminalDisposition"] = json!("failed");
    }
    bundle["ownerOutcome"] = json!(if trace_complete && all_runs_passed {
        "passed"
    } else {
        "failed"
    });

    fs::create_dir_all(&output_dir)?;
    let samples_bytes = ndjson_bytes(&samples)?;
    let ledger_bytes = ndjson_bytes(&ledger)?;
    let candidate_bytes = serde_json::to_vec_pretty(&candidate)?;
    fs::write(output_dir.join("samples.ndjson"), &samples_bytes)?;
    fs::write(output_dir.join("decision-ledger.ndjson"), &ledger_bytes)?;
    fs::write(
        output_dir.join("thermal-profile.candidate.json"),
        &candidate_bytes,
    )?;

    let mut html_bundle = bundle.clone();
    html_bundle["fileDigests"] = json!({});
    let html_data = json!({
        "bundle": html_bundle,
        "samples": samples,
        "ledger": ledger,
        "candidate": candidate,
    });
    let html = render_html(&html_data)?;
    fs::write(output_dir.join("index.html"), html.as_bytes())?;

    let mut file_digests = Map::new();
    file_digests.insert(
        "index.html".to_string(),
        json!(sha256_digest(html.as_bytes())),
    );
    file_digests.insert(
        "samples.ndjson".to_string(),
        json!(sha256_digest(&samples_bytes)),
    );
    file_digests.insert(
        "thermal-profile.candidate.json".to_string(),
        json!(sha256_digest(&candidate_bytes)),
    );
    file_digests.insert(
        "decision-ledger.ndjson".to_string(),
        json!(sha256_digest(&ledger_bytes)),
    );
    bundle["fileDigests"] = Value::Object(file_digests.clone());
    bundle["fileDigestBasis"] =
        json!("run.bundle.json hashes itself with its own digest field omitted");
    bundle["fileDigests"]["run.bundle.json"] = Value::Null;
    let bundle_basis = serde_json::to_vec_pretty(&bundle)?;
    let bundle_digest = sha256_digest(&bundle_basis);
    bundle["fileDigests"]["run.bundle.json"] = json!(bundle_digest);
    let bundle_bytes = serde_json::to_vec_pretty(&bundle)?;
    fs::write(output_dir.join("run.bundle.json"), &bundle_bytes)?;

    Ok(json!({
        "ok": true,
        "operation": "thermal_report.render_v2",
        "inputDir": display_path(&input_dir),
        "outputDir": display_path(&output_dir),
        "schema": bundle.get("schema").cloned().unwrap_or(Value::Null),
        "runId": bundle.get("runId").cloned().unwrap_or(Value::Null),
        "ownerOutcome": bundle.get("ownerOutcome").cloned().unwrap_or(Value::Null),
        "trace": bundle.get("trace").cloned().unwrap_or(Value::Null),
        "files": bundle.get("files").cloned().unwrap_or(Value::Null),
    }))
}

#[derive(Debug)]
struct EventAudit {
    trace: Value,
    counts: Value,
}

fn audit_events(samples: &[Value], ledger: &[Value]) -> EventAudit {
    let mut by_sequence = BTreeMap::<u64, Value>::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut reasons = Vec::<String>::new();
    for (stream, events) in [
        ("samples.ndjson", samples),
        ("decision-ledger.ndjson", ledger),
    ] {
        for event in events {
            let Some(object) = event.as_object() else {
                reasons.push(format!("{stream}: event is not an object"));
                continue;
            };
            let Some(sequence) = object.get("sequence").and_then(Value::as_u64) else {
                reasons.push(format!("{stream}: event missing sequence"));
                continue;
            };
            let Some(kind) = object.get("kind").and_then(Value::as_str) else {
                reasons.push(format!("{stream}: sequence {sequence} missing kind"));
                continue;
            };
            if !ALLOWED_EVENT_KINDS.contains(&kind) {
                reasons.push(format!(
                    "{stream}: sequence {sequence} has unsupported kind {kind}"
                ));
            }
            *counts.entry(kind.to_string()).or_default() += 1;
            if by_sequence.insert(sequence, event.clone()).is_some() {
                reasons.push(format!("duplicate sequence {sequence}"));
            }
            if kind == "sample" {
                for field in REQUIRED_SAMPLE_FIELDS {
                    if !object.contains_key(*field) {
                        reasons.push(format!("sequence {sequence} sample missing {field}"));
                    }
                }
                if object
                    .get("heaterRequestedOutputPermille")
                    .and_then(Value::as_u64)
                    .is_none_or(|value| value > 1000)
                {
                    reasons.push(format!(
                        "sequence {sequence} invalid heaterRequestedOutputPermille"
                    ));
                }
                if object
                    .get("heaterOutputPermille")
                    .and_then(Value::as_u64)
                    .is_none_or(|value| value > 1000)
                {
                    reasons.push(format!("sequence {sequence} invalid heaterOutputPermille"));
                }
            }
        }
    }
    let first = by_sequence.keys().next().copied();
    let last = by_sequence.keys().next_back().copied();
    if first != Some(0) {
        reasons.push("sequence does not start at zero".to_string());
    }
    if let (Some(first), Some(last)) = (first, last) {
        for sequence in first..=last {
            if !by_sequence.contains_key(&sequence) {
                reasons.push(format!("missing sequence {sequence}"));
            }
        }
    } else {
        reasons.push("no trace events".to_string());
    }
    let ordered = by_sequence.values().cloned().collect::<Vec<_>>();
    let trace_digest = sha256_digest(&ndjson_bytes(&ordered).unwrap_or_default());
    let gap = (!reasons.is_empty()).then(|| {
        json!({
            "reason": "trace_gap",
            "details": reasons,
        })
    });
    EventAudit {
        trace: json!({
            "firstSequence": first,
            "lastSequence": last,
            "complete": gap.is_none(),
            "digest": trace_digest,
            "gap": gap,
        }),
        counts: Value::Object(
            counts
                .into_iter()
                .map(|(key, value)| (key, json!(value)))
                .collect(),
        ),
    }
}

fn normalized_report_runs(
    bundle: &Value,
    samples: &[Value],
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(runs) = bundle.get("reportRuns").and_then(Value::as_array) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thermal-tuning-v2 run.bundle.json missing reportRuns",
        )
        .into());
    };
    let mut normalized = runs.to_vec();
    normalized.sort_by_key(|run| {
        run.get("targetC")
            .or_else(|| run.get("targetTempC"))
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX)
    });
    for run in &mut normalized {
        let outcome = run
            .get("reviewOutcome")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "passed" | "failed"))
            .unwrap_or("failed")
            .to_string();
        let passed = outcome == "passed";
        run["reviewOutcome"] = Value::String(outcome);
        run["reviewPassed"] = json!(passed);

        let target = run
            .get("targetC")
            .or_else(|| run.get("targetTempC"))
            .and_then(Value::as_i64);
        let Some(trials) = run.get_mut("trials").and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(target) = target else { continue };
        for trial in trials {
            let Some(trial_index) = trial.get("trialIndex").and_then(Value::as_i64) else {
                continue;
            };
            let Some((first_sequence, last_sequence, first_elapsed, last_elapsed)) =
                sample_bounds(samples, target, trial_index)
            else {
                continue;
            };
            for (field, value) in [
                ("firstSequence", first_sequence),
                ("lastSequence", last_sequence),
                ("firstElapsedMs", first_elapsed),
                ("lastElapsedMs", last_elapsed),
            ] {
                if trial.get(field).and_then(Value::as_u64).is_none() {
                    trial[field] = json!(value);
                }
            }
        }
    }
    Ok(normalized)
}

fn sample_bounds(samples: &[Value], target: i64, trial_index: i64) -> Option<(u64, u64, u64, u64)> {
    let mut bounds: Option<(u64, u64, u64, u64)> = None;
    for sample in samples {
        let object = sample.as_object()?;
        if object.get("targetC").and_then(Value::as_i64) != Some(target)
            || object.get("trialIndex").and_then(Value::as_i64) != Some(trial_index)
        {
            continue;
        }
        let sequence = object.get("sequence").and_then(Value::as_u64)?;
        let elapsed = object.get("elapsedMs").and_then(Value::as_u64)?;
        bounds = Some(match bounds {
            None => (sequence, sequence, elapsed, elapsed),
            Some((first_sequence, last_sequence, first_elapsed, last_elapsed)) => (
                first_sequence.min(sequence),
                last_sequence.max(sequence),
                first_elapsed.min(elapsed),
                last_elapsed.max(elapsed),
            ),
        });
    }
    bounds
}

fn validate_bundle(bundle: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bundle.get("schema").and_then(Value::as_str) != Some("thermal-tuning-v2") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "run.bundle.json schema must be thermal-tuning-v2",
        )
        .into());
    }
    for field in ["runId", "engine", "powerClass", "device", "firmware"] {
        if bundle.get(field).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("run.bundle.json missing {field}"),
            )
            .into());
        }
    }
    let run_id = bundle
        .get("runId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if run_id.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "runId must not be empty").into());
    }
    if bundle.get("targetSet").and_then(Value::as_object).is_none() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "targetSet must be an object").into(),
        );
    }
    Ok(())
}

fn validate_candidate(
    bundle: &Value,
    candidate: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let run_id = bundle.get("runId").and_then(Value::as_str);
    for field in [
        "schema",
        "runId",
        "candidateId",
        "candidateHash",
        "promotion",
    ] {
        if candidate.get(field).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal-profile.candidate.json missing {field}"),
            )
            .into());
        }
    }
    if candidate.get("schema").and_then(Value::as_str) != Some("thermal-tuning-v2")
        || candidate.get("runId").and_then(Value::as_str) != run_id
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "candidate identity does not match run bundle",
        )
        .into());
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn read_ndjson(path: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let file = fs::File::open(path)?;
    let mut values = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} line {} is invalid JSON: {error}",
                    path.display(),
                    index + 1
                ),
            )
        })?);
    }
    Ok(values)
}

fn ndjson_bytes(values: &[Value]) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend_from_slice(&serde_json::to_vec(value)?);
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn render_html(data: &Value) -> Result<String, serde_json::Error> {
    let data_json = serde_json::to_string(data)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(REPORT_TEMPLATE.replace(DATA_PLACEHOLDER, &data_json))
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
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
        Ok(cwd) => path.strip_prefix(cwd).map_or_else(
            |_| path.display().to_string(),
            |relative| relative.display().to_string(),
        ),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn renders_complete_five_file_bundle_from_sample_and_result_data() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let output = dir.path().join("out");
        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        assert_eq!(result["schema"], "thermal-tuning-v2");
        assert_eq!(result["ownerOutcome"], "passed");
        for file in [
            "index.html",
            "run.bundle.json",
            "samples.ndjson",
            "thermal-profile.candidate.json",
            "decision-ledger.ndjson",
        ] {
            assert!(output.join(file).is_file(), "missing {file}");
        }
        let html = fs::read_to_string(output.join("index.html")).unwrap();
        assert!(html.contains("Firmware Thermal Tuning V2"));
        assert!(html.contains("pps5a"));

        let bundle: Value = read_json(&output.join("run.bundle.json")).unwrap();
        let mut bundle_basis = bundle.clone();
        bundle_basis["fileDigests"]["run.bundle.json"] = Value::Null;
        let expected_bundle_digest =
            sha256_digest(&serde_json::to_vec_pretty(&bundle_basis).unwrap());
        assert_eq!(
            bundle["fileDigests"]["run.bundle.json"],
            expected_bundle_digest
        );
        assert_eq!(
            bundle["fileDigests"]["samples.ndjson"],
            sha256_digest(&fs::read(output.join("samples.ndjson")).unwrap())
        );
    }

    #[test]
    fn renders_trace_gap_as_failed_review_without_silencing_the_gap() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        fs::write(
            dir.path().join("decision-ledger.ndjson"),
            "{\"sequence\":2,\"kind\":\"decision\"}\n",
        )
        .unwrap();
        let output = dir.path().join("out");
        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        let bundle: Value = read_json(&output.join("run.bundle.json")).unwrap();
        assert_eq!(bundle["trace"]["complete"], false);
        assert_eq!(bundle["reviewDisposition"], "review_incomplete");
        assert_eq!(bundle["trace"]["gap"]["reason"], "trace_gap");
    }

    #[test]
    fn embeds_json_without_mutating_report_string_values() {
        let html = render_html(&json!({"value": "a&b < c \"quoted\""})).unwrap();
        assert!(html.contains("a\\u0026b \\u003c c \\\"quoted\\\""));
        assert!(!html.contains("a&amp;b") && !html.contains("a&quot;quoted&quot;"));
    }

    #[test]
    fn report_template_binds_all_charts_to_selected_trial_and_phase_band() {
        assert!(REPORT_TEMPLATE.contains("const samples=samplesForTrial();"));
        assert!(REPORT_TEMPLATE.contains("drawChart('temperatureChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawChart('controlChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawChart('voltageChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawPhaseBand(ctx,samples,left,plotW,maxT,height)"));
        assert!(REPORT_TEMPLATE.contains("canvas.dataset.sampleCount=String(samples.length)"));
        assert!(REPORT_TEMPLATE.contains("if(last)last.endElapsedMs=elapsedMs"));
    }

    fn write_fixture(dir: &Path) {
        fs::write(
            dir.join("run.bundle.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "thermal-tuning-v2",
                "runId": "mock-run",
                "engine": "firmware",
                "device": {"id": "mock-device"},
                "firmware": {"version": "mock-fw"},
                "powerClass": "pps5a",
                "targetSet": {"physicalC": [60], "selectedC": [60], "executionOrderC": [60]},
                "reportRuns": [{
                    "targetC": 60,
                    "reviewOutcome": "passed",
                    "reviewPassed": true,
                    "adoptedTrialIndex": 1,
                    "trialCount": 2,
                    "metrics": {"maxOvershootC": 1.1, "holdPeakToPeakC": 1.2, "holdConfirmSeconds": 60},
                    "trials": [{"trialIndex": 1, "disposition": "adopted", "maxOvershootC": 1.1, "holdPeakToPeakC": 1.2}]
                }],
                "terminalDisposition": "completed",
                "reviewDisposition": "passed"
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("thermal-profile.candidate.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "thermal-tuning-v2", "runId": "mock-run", "candidateId": "candidate-60", "candidateHash": "sha256:mock", "promotion": {"state": "sealed"}, "profile": {"targetC": 60}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("samples.ndjson"),
            "{\"sequence\":0,\"elapsedMs\":0,\"kind\":\"sample\",\"targetC\":60,\"trialIndex\":1,\"candidateId\":\"candidate-60\",\"candidateHash\":\"sha256:mock\",\"temperatureCentiC\":3500,\"vinMv\":12000,\"ppsContractMv\":21000,\"ppsContractMa\":5000,\"ppsContractKind\":\"pps\",\"ppsContractConfirmed\":true,\"ppsRequestedMv\":21000,\"ppsPowerCeilingMv\":105000,\"heaterRequestedOutputPermille\":1000,\"heaterOutputPermille\":980,\"measurementValid\":true,\"phase\":\"warmup\"}\n",
        )
        .unwrap();
        fs::write(
            dir.join("decision-ledger.ndjson"),
            "{\"sequence\":1,\"kind\":\"decision\",\"targetC\":60,\"trialIndex\":1,\"disposition\":\"adopted\"}\n",
        )
        .unwrap();
    }
}
