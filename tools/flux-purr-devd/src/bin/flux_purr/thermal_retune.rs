use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use super::{
    ThermalStageResult, parse_thermal_targets, parse_thermal_targets_from_summary,
    read_ndjson_values, render_thermal_self_test_report_html, thermal_candidate_point_mut,
    thermal_candidate_profile_to_value, thermal_rebuild_profile_from_anchor_targets,
    thermal_replay_applied_profile, thermal_replay_full_speed_to_stable,
    thermal_replay_stage_analysis, thermal_replay_stage_samples, thermal_stage_result_from_value,
    tune_thermal_candidate_point, validate_thermal_applied_results,
};

#[derive(Debug, Clone)]
pub(super) struct ThermalRetuneInput {
    pub(super) run_dir: PathBuf,
    pub(super) optimize_targets_c: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ThermalRetuneOutput {
    pub(super) summary: Value,
    pub(super) candidate_profile: Value,
    summary_path: PathBuf,
    samples_path: PathBuf,
    report_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct ThermalRetuneApplyReceipt {
    pub(super) ok: bool,
    pub(super) target: Value,
    pub(super) preview_response: Option<Value>,
    pub(super) status_readback: Option<Value>,
    pub(super) error: Option<String>,
}

impl ThermalRetuneApplyReceipt {
    fn to_value(&self) -> Value {
        json!({
            "op": "thermalControlProfile.preview",
            "ok": self.ok,
            "target": self.target,
            "previewResponse": self.preview_response,
            "statusReadback": self.status_readback,
            "error": self.error,
        })
    }
}

impl ThermalRetuneOutput {
    pub(super) fn write_apply_preview_receipt(
        &mut self,
        receipt: ThermalRetuneApplyReceipt,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.summary["applyPreview"] = receipt.to_value();
        write_json_pretty(&self.summary_path, &self.summary)?;
        fs::write(
            &self.report_path,
            render_thermal_self_test_report_html(&self.summary, &self.samples_path)?,
        )?;
        Ok(())
    }
}

pub(super) fn retune_thermal_self_test_run(
    input: ThermalRetuneInput,
) -> Result<ThermalRetuneOutput, Box<dyn std::error::Error + Send + Sync>> {
    let summary_path = input.run_dir.join("run.json");
    let samples_path = input.run_dir.join("samples.ndjson");
    let summary: Value = serde_json::from_slice(&fs::read(&summary_path)?)?;
    if summary.get("kind").and_then(Value::as_str) != Some("thermal_self_test") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "thermal retune requires a thermal self-test run.json",
        )
        .into());
    }
    let target_temps_c = parse_thermal_targets_from_summary(&summary, "targetsC")?;
    let optimize_targets_c = if let Some(optimize_targets_c) = input.optimize_targets_c.as_deref() {
        parse_thermal_targets(Some(optimize_targets_c))?
    } else {
        parse_thermal_targets_from_summary(&summary, "optimizeTargetsC")?
    };
    let original_applied = summary
        .get("applied")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal summary missing applied results",
            )
        })?;
    let samples = read_ndjson_values(&samples_path)?;
    let mut candidate_profile =
        thermal_replay_applied_profile(&summary, &samples, &target_temps_c)?;
    let mut candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
    let mut applied_results = Vec::new();
    let mut tuning_steps = Vec::<Value>::new();

    for (stage_index, original_result) in original_applied.iter().enumerate() {
        let mut replayed = thermal_stage_result_from_value(original_result)?;
        let stage_samples = thermal_replay_stage_samples(&samples, replayed.target_temp_c)?;
        replayed.sample_count = stage_samples.len();
        replayed.analysis = thermal_replay_stage_analysis(&stage_samples, replayed.target_temp_c);
        replayed.full_speed_to_stable =
            thermal_replay_full_speed_to_stable(&stage_samples, replayed.target_temp_c);
        if optimize_targets_c.contains(&replayed.target_temp_c) {
            if let Some(point) =
                thermal_candidate_point_mut(&mut candidate_profile, replayed.target_temp_c)
            {
                *point = tune_thermal_candidate_point(*point, &replayed);
            }
            thermal_rebuild_profile_from_anchor_targets(
                &mut candidate_profile,
                &optimize_targets_c,
            );
            candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
            tuning_steps.push(json!({
                "stageIndex": stage_index,
                "targetTempC": replayed.target_temp_c,
                "result": replayed.to_value(),
                "candidateProfile": candidate_profile_value.clone(),
            }));
        }
        applied_results.push(replayed);
    }

    let validation = validate_thermal_applied_results(&applied_results, &target_temps_c);
    let replay_summary_path = input.run_dir.join("run.replayed.json");
    let replay_candidate_path = input
        .run_dir
        .join("thermal-profile.replayed.candidate.json");
    let replay_report_path = input.run_dir.join("report.replayed.html");
    let replay_summary = json!({
        "kind": "thermal_self_test_replay",
        "ok": validation.get("passed").and_then(Value::as_bool) == Some(true),
        "runId": summary.get("runId").cloned().unwrap_or(Value::Null),
        "replayOf": summary_path,
        "target": summary.get("target").cloned().unwrap_or(Value::Null),
        "source": summary.get("source").cloned().unwrap_or(Value::Null),
        "parameters": {
            "targetsC": target_temps_c,
            "optimizeTargetsC": optimize_targets_c,
            "sampleIntervalMs": summary.pointer("/parameters/sampleIntervalMs").cloned().unwrap_or(Value::Null),
            "effectiveSampleIntervalMs": summary.pointer("/parameters/effectiveSampleIntervalMs").cloned().unwrap_or(Value::Null),
            "holdSeconds": summary.pointer("/parameters/holdSeconds").cloned().unwrap_or(Value::Null),
            "stageTimeoutSeconds": summary.pointer("/parameters/stageTimeoutSeconds").cloned().unwrap_or(Value::Null),
            "runtimeRearmAttempts": summary.pointer("/parameters/runtimeRearmAttempts").cloned().unwrap_or(Value::Null),
            "cooldownTempC": summary.pointer("/parameters/cooldownTempC").cloned().unwrap_or(Value::Null),
            "cooldownTimeoutSeconds": summary.pointer("/parameters/cooldownTimeoutSeconds").cloned().unwrap_or(Value::Null),
            "limits": summary.pointer("/parameters/limits").cloned().unwrap_or(Value::Null),
            "seedProfileFile": summary.pointer("/parameters/seedProfileFile").cloned().unwrap_or(Value::Null),
        },
        "files": {
            "runDir": input.run_dir,
            "summaryPath": replay_summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": replay_candidate_path,
            "reportHtmlPath": replay_report_path,
        },
        "sampleCount": summary.get("sampleCount").cloned().unwrap_or(Value::Null),
        "candidateProfile": candidate_profile_value.clone(),
        "profilePersistence": "not_saved",
        "tuningSteps": tuning_steps,
        "applied": applied_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
    });
    write_json_pretty(&replay_candidate_path, &candidate_profile_value)?;
    write_json_pretty(&replay_summary_path, &replay_summary)?;
    fs::write(
        &replay_report_path,
        render_thermal_self_test_report_html(&replay_summary, &samples_path)?,
    )?;
    Ok(ThermalRetuneOutput {
        summary: replay_summary,
        candidate_profile: candidate_profile_value,
        summary_path: replay_summary_path,
        samples_path,
        report_path: replay_report_path,
    })
}

fn write_json_pretty(
    path: &Path,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
