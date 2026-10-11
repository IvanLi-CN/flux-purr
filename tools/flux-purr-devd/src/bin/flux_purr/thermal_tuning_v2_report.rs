use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::current_unix_millis;

const DATA_PLACEHOLDER: &str = "__THERMAL_TUNING_V2_DATA__";
const REPORT_TEMPLATE: &str = include_str!("thermal_tuning_v2_report_template.html");
const PPS_ABSOLUTE_MIN_MV: u64 = 5_000;
const PPS_ABSOLUTE_MAX_MV: u64 = 28_000;
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
pub(crate) struct ThermalTuningV2ReportInput {
    pub(crate) input_dir: PathBuf,
    pub(crate) output_dir: Option<PathBuf>,
}

pub(crate) fn render_v2_bundle(
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

    let event_audit = audit_events(&bundle, &samples, &ledger);
    let report_runs = normalized_report_runs(&bundle, &samples, &ledger)?;
    let safety_summary = safety_summary(&bundle, &ledger);
    bundle["reportRuns"] = Value::Array(report_runs.clone());
    bundle["rawRuns"] = bundle
        .get("rawRuns")
        .cloned()
        .unwrap_or_else(|| Value::Array(report_runs.clone()));
    bundle["trace"] = event_audit.trace.clone();
    bundle["eventCounts"] = event_audit.counts.clone();
    bundle["safetySummary"] = safety_summary.clone();
    bundle["generatedBy"] = json!("flux-purr thermal report render-v2");
    bundle["generatedAt"] = json!(current_unix_millis());
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
    let promotion_verified = candidate_promotion_verified(&bundle, &candidate);
    let qualified = trace_complete
        && all_runs_passed
        && promotion_verified
        && bundle
            .pointer("/qualificationScope/diagnosticOnly")
            .and_then(Value::as_bool)
            == Some(false)
        && bundle.get("terminalDisposition").and_then(Value::as_str) == Some("passed");
    let promotion_inconsistent = bundle.get("terminalDisposition").and_then(Value::as_str)
        == Some("passed")
        && !promotion_verified;
    if !trace_complete || promotion_inconsistent {
        bundle["reviewDisposition"] = json!("review_incomplete");
    }
    bundle["ownerOutcome"] = json!(if qualified { "passed" } else { "failed" });

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

fn candidate_promotion_verified(bundle: &Value, candidate: &Value) -> bool {
    if bundle.get("engine").and_then(Value::as_str) != Some("firmware")
        || candidate
            .pointer("/promotion/state")
            .and_then(Value::as_str)
            != Some("saved")
        || bundle
            .pointer("/candidate/promotionState")
            .and_then(Value::as_str)
            != Some("saved")
    {
        return false;
    }
    let run_id = bundle.get("runId").and_then(Value::as_str);
    let candidate_id = candidate.get("candidateId").and_then(Value::as_str);
    let candidate_hash = candidate.get("candidateHash").and_then(Value::as_str);
    let power_class = bundle.get("powerClass").and_then(Value::as_str);
    if candidate_id.is_none()
        || candidate_hash.is_none()
        || candidate_id
            != bundle
                .pointer("/candidate/candidateId")
                .and_then(Value::as_str)
        || candidate_hash
            != bundle
                .pointer("/candidate/candidateHash")
                .and_then(Value::as_str)
        || !matches!(power_class, Some("pps3a" | "pps5a"))
        || candidate.pointer("/profile/bank").and_then(Value::as_str) != power_class
        || candidate.get("powerClass").and_then(Value::as_str) != power_class
    {
        return false;
    }
    let preview = &candidate["promotion"]["previewReceipt"];
    let save = &candidate["promotion"]["saveReceipt"];
    for receipt in [preview, save] {
        if receipt.get("runId").and_then(Value::as_str) != run_id
            || receipt.get("candidateId").and_then(Value::as_str) != candidate_id
            || receipt.get("candidateHash").and_then(Value::as_str) != candidate_hash
            || receipt.get("powerClass").and_then(Value::as_str) != power_class
        {
            return false;
        }
    }
    preview.get("ramAppliedHash").and_then(Value::as_str) == candidate_hash
        && save.get("bank").and_then(Value::as_str) == power_class
        && save.get("bankRevision").and_then(Value::as_u64).is_some()
        && save.get("bankHash").and_then(Value::as_str) == candidate_hash
        && save.get("atomic").and_then(Value::as_bool) == Some(true)
        && save.get("readbackVerified").and_then(Value::as_bool) == Some(true)
}

#[derive(Debug)]
struct EventAudit {
    trace: Value,
    counts: Value,
}

fn audit_events(bundle: &Value, samples: &[Value], ledger: &[Value]) -> EventAudit {
    let mut by_sequence = BTreeMap::<u64, Value>::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut reasons = Vec::<String>::new();
    let host_preflight_3a = bundle.get("traceOrigin").and_then(Value::as_str)
        == Some("host_preflight")
        && bundle.get("powerClass").and_then(Value::as_str) == Some("pps3a")
        && samples.is_empty()
        && ledger.len() == 1
        && ledger[0].get("kind").and_then(Value::as_str) == Some("safety")
        && ledger[0].get("reason").and_then(Value::as_str) == Some("pps3a_not_supported");
    if bundle.get("engine").and_then(Value::as_str) == Some("firmware")
        && !host_preflight_3a
        && bundle
            .pointer("/deviceTraceVerification/verified")
            .and_then(Value::as_bool)
            != Some(true)
    {
        reasons.push("firmware device trace was not verified".to_string());
    }
    let apdo_max_mv = bundle
        .pointer("/ppsVoltageBounds/apdoMaxMv")
        .and_then(Value::as_u64);
    let absolute_max_mv = bundle
        .pointer("/ppsVoltageBounds/absoluteMaxMv")
        .and_then(Value::as_u64);
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
                audit_sample_fields(object, sequence, apdo_max_mv, absolute_max_mv, &mut reasons);
            }
        }
    }
    let first = by_sequence.keys().next().copied();
    let last = by_sequence.keys().next_back().copied();
    if bundle.get("engine").and_then(Value::as_str) == Some("firmware")
        && !host_preflight_3a
        && (first
            != bundle
                .pointer("/deviceTrace/firstSequence")
                .and_then(Value::as_u64)
            || last
                != bundle
                    .pointer("/deviceTrace/lastSequence")
                    .and_then(Value::as_u64))
    {
        reasons.push("report event range does not match verified device trace".to_string());
    }
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

fn safety_summary(bundle: &Value, ledger: &[Value]) -> Value {
    if let Some(event) = ledger
        .iter()
        .filter(|event| event.get("kind").and_then(Value::as_str) == Some("safety"))
        .min_by_key(|event| {
            event
                .get("sequence")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX)
        })
    {
        return json!({
            "kind": "safety",
            "source": "device_event",
            "reason": event.get("reason").cloned().unwrap_or_else(|| json!("device_safety_event")),
            "sequence": event.get("sequence").cloned().unwrap_or(Value::Null),
            "elapsedMs": event.get("elapsedMs").cloned().unwrap_or(Value::Null),
            "phase": event.get("phase").cloned().unwrap_or(Value::Null),
            "heaterState": event.get("heaterState").cloned().unwrap_or(Value::Null),
            "origin": event.get("origin").cloned().unwrap_or(Value::Null),
        });
    }

    if let Some(interruption) = bundle.get("hostInterruption").and_then(Value::as_object) {
        return json!({
            "kind": "safety",
            "source": "host_interruption",
            "reason": interruption.get("reason").cloned().unwrap_or_else(|| json!("host_interruption")),
            "sequence": Value::Null,
            "elapsedMs": Value::Null,
            "phase": "failed",
            "heaterState": interruption.get("deviceState").cloned().unwrap_or_else(|| json!("unconfirmed")),
            "origin": "host",
        });
    }

    if let Some(reason) = bundle.get("rejectionReason").and_then(Value::as_str) {
        return json!({
            "kind": "safety",
            "source": "host_preflight",
            "reason": reason,
            "sequence": 0,
            "elapsedMs": 0,
            "phase": "failed",
            "heaterState": "not_commanded",
            "origin": "host",
        });
    }

    if bundle.get("terminalDisposition").and_then(Value::as_str) == Some("failed") {
        let trace_gap = bundle.pointer("/deviceTrace/gap").and_then(Value::as_bool) == Some(true);
        return json!({
            "kind": "safety",
            "source": if trace_gap { "trace_audit" } else { "device_terminal" },
            "reason": if trace_gap { "trace_gap" } else { "device_terminal_failed_without_safety_event" },
            "sequence": Value::Null,
            "elapsedMs": Value::Null,
            "phase": "failed",
            "heaterState": "unconfirmed",
            "origin": "report_audit",
        });
    }

    Value::Null
}

fn audit_sample_fields(
    object: &Map<String, Value>,
    sequence: u64,
    apdo_max_mv: Option<u64>,
    absolute_max_mv: Option<u64>,
    reasons: &mut Vec<String>,
) {
    for field in REQUIRED_SAMPLE_FIELDS {
        if !object.contains_key(*field) {
            reasons.push(format!("sequence {sequence} sample missing {field}"));
        }
    }
    for field in ["heaterRequestedOutputPermille", "heaterOutputPermille"] {
        if object
            .get(field)
            .and_then(Value::as_u64)
            .is_none_or(|value| value > 1000)
        {
            reasons.push(format!("sequence {sequence} invalid {field}"));
        }
    }
    let ceiling_mv = object.get("ppsPowerCeilingMv").and_then(Value::as_u64);
    let requested_mv = object.get("ppsRequestedMv").and_then(Value::as_u64);
    if !matches!(
        (ceiling_mv, requested_mv),
        (Some(ceiling), Some(requested))
            if (PPS_ABSOLUTE_MIN_MV..=PPS_ABSOLUTE_MAX_MV).contains(&ceiling)
                && requested <= ceiling
                && apdo_max_mv.is_none_or(|apdo_max| ceiling == apdo_max)
                && absolute_max_mv.is_none_or(|absolute_max| ceiling <= absolute_max)
    ) {
        reasons.push(format!(
            "sequence {sequence} invalid PPS request voltage ceiling (mV)"
        ));
    }
}

fn normalized_report_runs(
    bundle: &Value,
    samples: &[Value],
    ledger: &[Value],
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(runs) = bundle.get("reportRuns").and_then(Value::as_array) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thermal-tuning-v2 run.bundle.json missing reportRuns",
        )
        .into());
    };
    let mut normalized = runs.to_vec();
    let execution_order = bundle
        .pointer("/targetSet/executionOrderC")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    let selected_targets = bundle
        .pointer("/targetSet/selectedC")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_else(|| execution_order.clone());
    let safety = safety_summary(bundle, ledger);
    let terminal_reason = safety
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            (bundle.get("terminalDisposition").and_then(Value::as_str) == Some("failed"))
                .then(|| "run_terminated".to_owned())
        });
    let last_attempted_target = execution_order.iter().rev().find(|target| {
        normalized.iter().any(|run| {
            run.get("targetC")
                .or_else(|| run.get("targetTempC"))
                .and_then(Value::as_i64)
                == Some(**target)
                && run_has_evidence(run, samples, **target)
        })
    });
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
        let has_evidence = target.is_some_and(|target| run_has_evidence(run, samples, target));
        let target_budget_reason =
            target.and_then(|target| target_budget_stop_reason(ledger, target));
        run["executionStatus"] = json!(if has_evidence {
            "attempted"
        } else {
            "not_attempted"
        });
        if !has_evidence {
            let target_decision = ledger.iter().rev().find_map(|event| {
                (event.get("kind").and_then(Value::as_str) == Some("decision")
                    && event.get("targetC").and_then(Value::as_i64) == target
                    && event.get("trialIndex").is_none_or(Value::is_null))
                .then(|| event.get("reason").and_then(Value::as_str))
                .flatten()
            });
            let outside_scope = target.is_some_and(|target| !selected_targets.contains(&target));
            let blocked_reason = if outside_scope {
                "outside_selected_scope"
            } else {
                target_decision
                    .or(terminal_reason.as_deref())
                    .unwrap_or("run_terminated_before_target")
            };
            run["stopReason"] = json!(target_decision.unwrap_or("run_terminated_before_target"));
            run["blockedBy"] = json!({
                "targetC": if target_decision.is_some() { None } else { last_attempted_target.copied() },
                "reason": blocked_reason,
            });
        } else {
            run["blockedBy"] = Value::Null;
            if let Some(reason) = target_budget_reason {
                run["stopReason"] = json!(reason);
            }
        }
        let Some(trials) = run.get_mut("trials").and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(target) = target else { continue };
        for trial in trials {
            let start = trial
                .pointer("/cooldownGate/startTemperatureCentiC")
                .and_then(Value::as_i64);
            let threshold = trial
                .pointer("/cooldownGate/thresholdCentiC")
                .and_then(Value::as_i64);
            trial["cooldownGate"] = json!({
                "startTemperatureCentiC": start,
                "thresholdCentiC": threshold,
                "availability": if start.is_some() && threshold.is_some() {
                    "device_local"
                } else {
                    "unavailable_device_trial_record"
                }
            });
            let Some(trial_index) = trial.get("trialIndex").and_then(Value::as_i64) else {
                continue;
            };
            let Some((first_sequence, last_sequence, first_elapsed, last_elapsed)) =
                sample_bounds(samples, target, trial_index)
            else {
                continue;
            };
            fill_trial_bounds(
                trial,
                [first_sequence, last_sequence, first_elapsed, last_elapsed],
            );
        }
    }
    Ok(normalized)
}

pub(super) fn target_budget_stop_reason(ledger: &[Value], target: i64) -> Option<&str> {
    ledger.iter().rev().find_map(|event| {
        (event.get("kind").and_then(Value::as_str) == Some("decision")
            && event.get("targetC").and_then(Value::as_i64) == Some(target))
        .then(|| event.get("reason").and_then(Value::as_str))
        .flatten()
        .filter(|reason| reason.starts_with("target_budget_exhausted"))
    })
}

fn run_has_evidence(run: &Value, samples: &[Value], target: i64) -> bool {
    run.get("trials")
        .and_then(Value::as_array)
        .is_some_and(|trials| !trials.is_empty())
        || samples
            .iter()
            .any(|sample| sample.get("targetC").and_then(Value::as_i64) == Some(target))
}

fn fill_trial_bounds(trial: &mut Value, bounds: [u64; 4]) {
    for (field, value) in [
        "firstSequence",
        "lastSequence",
        "firstElapsedMs",
        "lastElapsedMs",
    ]
    .into_iter()
    .zip(bounds)
    {
        if trial.get(field).and_then(Value::as_u64).is_none() {
            trial[field] = json!(value);
        }
    }
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
        let bundle = read_json(&output.join("run.bundle.json")).unwrap();
        assert_eq!(
            bundle["reportRuns"][0]["trials"][0]["cooldownGate"]["startTemperatureCentiC"],
            3_500
        );
        assert_eq!(
            bundle["reportRuns"][0]["trials"][0]["cooldownGate"]["thresholdCentiC"],
            4_000
        );
        assert_eq!(
            bundle["reportRuns"][0]["trials"][0]["cooldownGate"]["availability"],
            "device_local"
        );

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
    fn diagnostic_subset_never_qualifies_even_with_a_saved_device_receipt() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let bundle_path = dir.path().join("run.bundle.json");
        let mut bundle = read_json(&bundle_path).unwrap();
        bundle["qualificationScope"] = json!({
            "kind": "diagnostic-retest", "targetCount": 1, "diagnosticOnly": true
        });
        fs::write(&bundle_path, serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();
        let output = dir.path().join("out");
        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        let rendered = read_json(&output.join("run.bundle.json")).unwrap();
        assert_eq!(rendered["terminalDisposition"], "passed");
        assert_eq!(rendered["qualificationScope"]["diagnosticOnly"], true);
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
    fn renders_global_safety_event_before_trial_details() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        fs::write(
            dir.path().join("decision-ledger.ndjson"),
            "{\"sequence\":0,\"elapsedMs\":0,\"kind\":\"safety\",\"phase\":\"failed\",\"reason\":\"heater_interlock_disarmed\",\"heaterState\":\"disarmed\"}\n",
        )
        .unwrap();
        let output = dir.path().join("out");
        render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        let bundle = read_json(&output.join("run.bundle.json")).unwrap();
        assert!(
            bundle["generatedAt"]
                .as_u64()
                .is_some_and(|value| value > 0)
        );
        assert_eq!(
            bundle["safetySummary"]["reason"],
            "heater_interlock_disarmed"
        );
        assert_eq!(bundle["safetySummary"]["source"], "device_event");
        let html = fs::read_to_string(output.join("index.html")).unwrap();
        assert!(html.contains("heater_interlock_disarmed"));
        assert!(html.contains("safetySummary"));
    }

    #[test]
    fn missing_save_receipt_fails_without_rewriting_device_terminal_fact() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let candidate_path = dir.path().join("thermal-profile.candidate.json");
        let mut candidate = read_json(&candidate_path).unwrap();
        candidate["promotion"]["saveReceipt"] = Value::Null;
        fs::write(
            &candidate_path,
            serde_json::to_vec_pretty(&candidate).unwrap(),
        )
        .unwrap();

        let output = dir.path().join("out");
        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        let bundle = read_json(&output.join("run.bundle.json")).unwrap();
        assert_eq!(bundle["terminalDisposition"], "passed");
        assert_eq!(bundle["reviewDisposition"], "review_incomplete");
    }

    #[test]
    fn complete_candidate_review_remains_complete_before_explicit_save() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let bundle_path = dir.path().join("run.bundle.json");
        let mut bundle = read_json(&bundle_path).unwrap();
        bundle["terminalDisposition"] = json!("candidate_ready");
        bundle["reviewDisposition"] = json!("complete");
        fs::write(&bundle_path, serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();
        let candidate_path = dir.path().join("thermal-profile.candidate.json");
        let mut candidate = read_json(&candidate_path).unwrap();
        candidate["promotion"]["state"] = json!("sealed");
        candidate["promotion"]["saveReceipt"] = Value::Null;
        fs::write(
            &candidate_path,
            serde_json::to_vec_pretty(&candidate).unwrap(),
        )
        .unwrap();

        let output = dir.path().join("out");
        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        let rendered = read_json(&output.join("run.bundle.json")).unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        assert_eq!(rendered["terminalDisposition"], "candidate_ready");
        assert_eq!(rendered["reviewDisposition"], "complete");
    }

    #[test]
    fn rejects_contract_power_milliwatts_as_voltage_ceiling() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let bundle_path = dir.path().join("run.bundle.json");
        let mut input_bundle = read_json(&bundle_path).unwrap();
        input_bundle["ppsVoltageBounds"]["absoluteMaxMv"] = Value::Null;
        input_bundle["ppsVoltageBounds"]["apdoMaxMv"] = Value::Null;
        fs::write(&bundle_path, serde_json::to_vec(&input_bundle).unwrap()).unwrap();
        let samples_path = dir.path().join("samples.ndjson");
        let mut sample: Value =
            serde_json::from_str(&fs::read_to_string(&samples_path).unwrap()).unwrap();
        sample["ppsPowerCeilingMv"] = json!(105_000);
        fs::write(&samples_path, format!("{sample}\n")).unwrap();

        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(dir.path().join("out")),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "failed");
        let bundle = read_json(&dir.path().join("out/run.bundle.json")).unwrap();
        assert_eq!(bundle["reviewDisposition"], "review_incomplete");
        assert!(
            result["trace"]["gap"]["details"]
                .as_array()
                .unwrap()
                .iter()
                .any(|detail| detail
                    .as_str()
                    .unwrap()
                    .contains("PPS request voltage ceiling"))
        );
    }

    #[test]
    fn accepts_valid_voltage_ceiling_when_device_bounds_are_unavailable() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let bundle_path = dir.path().join("run.bundle.json");
        let mut input_bundle = read_json(&bundle_path).unwrap();
        input_bundle["ppsVoltageBounds"]["absoluteMaxMv"] = Value::Null;
        input_bundle["ppsVoltageBounds"]["apdoMaxMv"] = Value::Null;
        fs::write(&bundle_path, serde_json::to_vec(&input_bundle).unwrap()).unwrap();

        let result = render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(dir.path().join("out")),
        })
        .unwrap();
        assert_eq!(result["ownerOutcome"], "passed");
        assert_eq!(result["trace"]["complete"], true);
    }

    #[test]
    fn embeds_json_without_mutating_report_string_values() {
        let html = render_html(&json!({"value": "a&b < c \"quoted\""})).unwrap();
        assert!(html.contains("a\\u0026b \\u003c c \\\"quoted\\\""));
        assert!(!html.contains("a&amp;b") && !html.contains("a&quot;quoted&quot;"));
    }

    #[test]
    fn historical_trial_without_cooldown_evidence_is_explicitly_unavailable() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let input_path = dir.path().join("run.bundle.json");
        let mut input = read_json(&input_path).unwrap();
        input["reportRuns"][0]["trials"][0]
            .as_object_mut()
            .unwrap()
            .remove("cooldownGate");
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let output = dir.path().join("out");
        render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        let bundle = read_json(&output.join("run.bundle.json")).unwrap();
        let gate = &bundle["reportRuns"][0]["trials"][0]["cooldownGate"];
        assert!(gate["startTemperatureCentiC"].is_null());
        assert!(gate["thresholdCentiC"].is_null());
        assert_eq!(gate["availability"], "unavailable_device_trial_record");
    }

    #[test]
    fn report_template_binds_all_charts_to_selected_trial_and_phase_band() {
        assert!(REPORT_TEMPLATE.contains(".selection-dock{position:sticky;top:0"));
        assert!(REPORT_TEMPLATE.contains("class=\"selection-dock\""));
        assert!(REPORT_TEMPLATE.contains("id=\"voltageEvidence\""));
        assert!(
            REPORT_TEMPLATE
                .contains("s.measurementAccepted===false||s.measurementValid===false?NaN")
        );
        assert!(REPORT_TEMPLATE.contains("const trialSamples=samplesForTrial();"));
        assert!(REPORT_TEMPLATE.contains("drawChart('temperatureChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawChart('controlChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawChart('voltageChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("drawChart('currentChart',samples,"));
        assert!(REPORT_TEMPLATE.contains("chartValue(s.ppsContractMa)"));
        assert!(REPORT_TEMPLATE.contains("mA · safety ceiling · local evidence"));
        assert!(REPORT_TEMPLATE.contains("const chartNumber=value=>"));
        assert!(REPORT_TEMPLATE.contains("const tickLabels="));
        assert!(REPORT_TEMPLATE.contains("canvas.dataset.plotLeft=String(left)"));
        assert!(REPORT_TEMPLATE.contains("Hold (controller)"));
        assert!(REPORT_TEMPLATE.contains("chartGrid.hidden=samples.length===0"));
        assert!(REPORT_TEMPLATE.contains("该目标尚无候选试验样本"));
        assert!(REPORT_TEMPLATE.contains(".summary-metrics strong{display:block;color:var(--ink);font:650 13px ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere}"));
        assert!(
            REPORT_TEMPLATE.contains("drawPhaseBand(ctx,samples,left,plotW,startT,spanT,height)")
        );
        assert!(REPORT_TEMPLATE.contains("sample.phase!=='cooldown'"));
        assert!(REPORT_TEMPLATE.contains("function postTrialSamples()"));
        assert!(REPORT_TEMPLATE.contains("sample.phase==='cooldown'"));
        assert!(
            REPORT_TEMPLATE.contains("chartRange==='full'?[...trialSamples,...postTrialSamples()]")
        );
        assert!(REPORT_TEMPLATE.contains("stableBand:[activeTarget-1.5,activeTarget+1.5]"));
        assert!(REPORT_TEMPLATE.contains("['trial duration',ms(trialDuration)]"));
        assert!(REPORT_TEMPLATE.contains(
            "['qualified Hold',`${ms(holdDuration)} / ${confirm?'60.00s confirm':'12.00s scout'}`]"
        ));
        assert!(REPORT_TEMPLATE.contains("['post-stop peak',postPeak==null?'unavailable'"));
        assert!(REPORT_TEMPLATE.contains("for(const id of CHART_IDS){const canvas=$(id);canvas.addEventListener('pointermove',moveHover)"));
        assert!(REPORT_TEMPLATE.contains("line.dataset.sequence=String(sample.sequence)"));
        assert!(REPORT_TEMPLATE.contains("tooltip.dataset.sequence=String(sample.sequence)"));
        assert!(REPORT_TEMPLATE.contains("value!=null&&Number.isFinite(Number(value))"));
        assert!(REPORT_TEMPLATE.contains("centiC(cooldown.startTemperatureCentiC)"));
        assert!(REPORT_TEMPLATE.contains("centiC(cooldown.thresholdCentiC)"));
        assert!(
            REPORT_TEMPLATE.contains("value==null||!Number.isFinite(Number(value))?'unavailable'")
        );
        assert!(
            REPORT_TEMPLATE.contains("const lo=unit==='%'?0:ymin-pad, hi=unit==='%'?100:ymax+pad")
        );
        assert!(REPORT_TEMPLATE.contains("class=\"chart-legend\""));
        assert!(REPORT_TEMPLATE.contains("canvas.dataset.sampleCount=String(samples.length)"));
        assert!(REPORT_TEMPLATE.contains("if(last)last.endElapsedMs=startElapsedMs"));
        assert!(REPORT_TEMPLATE.contains("run.executionStatus||'—'"));
        assert!(REPORT_TEMPLATE.contains("run.blockedBy?.reason||run.stopReason||'—'"));
        assert!(REPORT_TEMPLATE.contains("new Date(BUNDLE.generatedAt).toLocaleString"));
        assert!(!REPORT_TEMPLATE.contains("BUNDLE.generatedAt||'mock'"));
    }

    #[test]
    fn marks_unattempted_targets_and_their_blocking_target() {
        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        let bundle_path = dir.path().join("run.bundle.json");
        let mut input_bundle = read_json(&bundle_path).unwrap();
        input_bundle["targetSet"] = json!({
            "physicalC": [60, 80],
            "selectedC": [60, 80],
            "executionOrderC": [60, 80]
        });
        input_bundle["terminalDisposition"] = json!("failed");
        input_bundle["reviewDisposition"] = json!("review_incomplete");
        input_bundle["deviceTrace"]["gap"] = json!(true);
        input_bundle["reportRuns"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "targetC": 80,
                "reviewOutcome": "failed",
                "reviewPassed": false,
                "trialCount": 0,
                "trials": []
            }));
        fs::write(&bundle_path, serde_json::to_vec(&input_bundle).unwrap()).unwrap();

        let output = dir.path().join("out");
        render_v2_bundle(ThermalTuningV2ReportInput {
            input_dir: dir.path().to_path_buf(),
            output_dir: Some(output.clone()),
        })
        .unwrap();
        let bundle = read_json(&output.join("run.bundle.json")).unwrap();
        let runs = bundle["reportRuns"].as_array().unwrap();
        let attempted = runs.iter().find(|run| run["targetC"] == 60).unwrap();
        let unattempted = runs.iter().find(|run| run["targetC"] == 80).unwrap();
        assert_eq!(attempted["executionStatus"], "attempted");
        assert_eq!(unattempted["executionStatus"], "not_attempted");
        assert_eq!(unattempted["stopReason"], "run_terminated_before_target");
        assert_eq!(unattempted["blockedBy"]["targetC"], 60);
        assert_eq!(unattempted["blockedBy"]["reason"], "trace_gap");
    }

    #[test]
    fn preserves_device_interval_prune_reason_for_unattempted_target() {
        let bundle = json!({
            "targetSet": {"selectedC": [60, 140], "executionOrderC": [60, 140]},
            "terminalDisposition": "failed",
            "reportRuns": [
                {"targetC": 60, "reviewOutcome": "failed", "trials": []},
                {"targetC": 140, "reviewOutcome": "failed", "trials": []}
            ]
        });
        let samples = vec![json!({"kind": "sample", "targetC": 60})];
        let ledger = vec![json!({
            "kind": "decision",
            "targetC": 140,
            "trialIndex": null,
            "reason": "interval_pruned_without_accepted_bounds"
        })];

        let runs = normalized_report_runs(&bundle, &samples, &ledger).unwrap();
        let pruned = runs.iter().find(|run| run["targetC"] == 140).unwrap();
        assert_eq!(pruned["executionStatus"], "not_attempted");
        assert_eq!(
            pruned["stopReason"],
            "interval_pruned_without_accepted_bounds"
        );
        assert_eq!(
            pruned["blockedBy"]["reason"],
            "interval_pruned_without_accepted_bounds"
        );
        assert!(pruned["blockedBy"]["targetC"].is_null());
    }

    #[test]
    fn attempted_target_uses_device_budget_terminal_reason_on_rerender() {
        let bundle = json!({
            "targetSet": {"selectedC": [60], "executionOrderC": [60]},
            "terminalDisposition": "failed",
            "reportRuns": [{"targetC": 60, "reviewOutcome": "failed", "trials": [{"trialIndex": 1, "stopReason": "full-speed-to-stable budget exhausted"}]}]
        });
        let samples = vec![json!({"kind": "sample", "targetC": 60, "trialIndex": 1})];
        let ledger = vec![json!({
            "kind": "decision", "targetC": 60, "trialIndex": 2,
            "reason": "target_budget_exhausted_during_cooldown"
        })];
        let runs = normalized_report_runs(&bundle, &samples, &ledger).unwrap();
        assert_eq!(runs[0]["executionStatus"], "attempted");
        assert_eq!(
            runs[0]["stopReason"],
            "target_budget_exhausted_during_cooldown"
        );
        assert_eq!(
            runs[0]["trials"][0]["stopReason"],
            "full-speed-to-stable budget exhausted"
        );
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
                "candidate": {"candidateId": "candidate-60", "candidateHash": "sha256:mock", "promotionState": "saved"},
                "ppsVoltageBounds": {"absoluteMaxMv": 28000, "apdoMaxMv": 21000},
                "targetSet": {"physicalC": [60], "selectedC": [60], "executionOrderC": [60]},
                "qualificationScope": {"kind": "default-pps5a-full-batch", "targetCount": 1, "diagnosticOnly": false},
                "reportRuns": [{
                    "targetC": 60,
                    "reviewOutcome": "passed",
                    "reviewPassed": true,
                    "adoptedTrialIndex": 1,
                    "trialCount": 2,
                    "metrics": {"maxOvershootC": 1.1, "holdPeakToPeakC": 1.2, "holdConfirmSeconds": 60},
                    "trials": [{"trialIndex": 1, "disposition": "adopted", "maxOvershootC": 1.1, "holdPeakToPeakC": 1.2, "cooldownGate": {"startTemperatureCentiC": 3500, "thresholdCentiC": 4000}}]
                }],
                "terminalDisposition": "passed",
                "reviewDisposition": "passed",
                "deviceTrace": {"firstSequence": 0, "lastSequence": 1, "complete": true, "gap": false, "digest": "mock-fixture"},
                "deviceTraceVerification": {"verified": true, "source": "mock-fixture", "reasons": []}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("thermal-profile.candidate.json"),
            serde_json::to_vec_pretty(&json!({
                "schema": "thermal-tuning-v2", "runId": "mock-run", "candidateId": "candidate-60", "candidateHash": "sha256:mock",
                "promotion": {
                    "state": "saved",
                    "previewReceipt": {"runId": "mock-run", "candidateId": "candidate-60", "candidateHash": "sha256:mock", "powerClass": "pps5a", "ramAppliedHash": "sha256:mock"},
                    "saveReceipt": {"runId": "mock-run", "candidateId": "candidate-60", "candidateHash": "sha256:mock", "powerClass": "pps5a", "bank": "pps5a", "bankRevision": 1, "bankHash": "sha256:mock", "atomic": true, "readbackVerified": true}
                },
                "powerClass": "pps5a",
                "profile": {"bank": "pps5a", "targetC": 60}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.join("samples.ndjson"),
            "{\"sequence\":0,\"elapsedMs\":0,\"kind\":\"sample\",\"targetC\":60,\"trialIndex\":1,\"candidateId\":\"candidate-60\",\"candidateHash\":\"sha256:mock\",\"temperatureCentiC\":3500,\"vinMv\":12000,\"ppsContractMv\":21000,\"ppsContractMa\":5000,\"ppsContractKind\":\"pps\",\"ppsContractConfirmed\":true,\"ppsRequestedMv\":21000,\"ppsPowerCeilingMv\":21000,\"heaterRequestedOutputPermille\":1000,\"heaterOutputPermille\":980,\"measurementValid\":true,\"phase\":\"warmup\"}\n",
        )
        .unwrap();
        fs::write(
            dir.join("decision-ledger.ndjson"),
            "{\"sequence\":1,\"kind\":\"decision\",\"targetC\":60,\"trialIndex\":1,\"disposition\":\"adopted\"}\n",
        )
        .unwrap();
    }
}
