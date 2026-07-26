use std::{
    fs, io,
    path::{Path, PathBuf},
};

use reqwest::{Client, Method};
use serde_json::{Value, json};

use super::{
    ThermalProfileMode, ThermalRetuneArgs, ThermalStageResult, parse_thermal_targets,
    parse_thermal_targets_from_summary, read_ndjson_values, request_with_lease, resolve_target,
    thermal_candidate_point_mut, thermal_candidate_profile_to_value,
    thermal_heater_parameters_value, thermal_profile_preview_runtime_body,
    thermal_rebuild_profile_from_anchor_targets, thermal_replay_applied_profile,
    thermal_replay_full_speed_to_stable, thermal_replay_stage_analysis,
    thermal_replay_stage_samples, thermal_self_test_evaluation_mode_from_summary,
    thermal_stage_result_from_value, thermal_summary_attach_replay_source_analysis,
    tune_thermal_candidate_point, validate_thermal_applied_results,
    verify_thermal_control_readback, verify_thermal_profile_mode_readback,
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
    profile_mode: Option<ThermalProfileMode>,
    summary_path: PathBuf,
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
    let profile_mode = thermal_retune_profile_mode(&summary);
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

    let evaluation_mode = thermal_self_test_evaluation_mode_from_summary(&summary);
    let validation =
        validate_thermal_applied_results(&applied_results, &target_temps_c, evaluation_mode);
    let replay_summary_path = input.run_dir.join("run.replayed.json");
    let replay_candidate_path = input
        .run_dir
        .join("thermal-profile.replayed.candidate.json");
    let mut replay_summary = json!({
        "kind": "thermal_self_test_replay",
        "ok": validation.get("passed").and_then(Value::as_bool) == Some(true),
        "runId": summary.get("runId").cloned().unwrap_or(Value::Null),
        "replayOf": summary_path,
        "target": summary.get("target").cloned().unwrap_or(Value::Null),
        "source": summary.get("source").cloned().unwrap_or(Value::Null),
        "selectedMode": summary.get("selectedMode").cloned().unwrap_or(Value::Null),
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
            "evaluationMode": evaluation_mode.as_str(),
        },
        "files": {
            "runDir": input.run_dir,
            "summaryPath": replay_summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": replay_candidate_path,
        },
        "sampleCount": summary.get("sampleCount").cloned().unwrap_or(Value::Null),
        "candidateProfile": candidate_profile_value.clone(),
        "profilePersistence": "not_saved",
        "tuningSteps": tuning_steps,
        "applied": applied_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
    });
    thermal_summary_attach_replay_source_analysis(&mut replay_summary, &samples)?;
    write_json_pretty(&replay_candidate_path, &candidate_profile_value)?;
    write_json_pretty(&replay_summary_path, &replay_summary)?;
    Ok(ThermalRetuneOutput {
        summary: replay_summary,
        candidate_profile: candidate_profile_value,
        profile_mode,
        summary_path: replay_summary_path,
    })
}

pub(super) async fn run_thermal_retune(
    client: &Client,
    default_devd: &str,
    args: ThermalRetuneArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let apply_preview = args.apply_preview;
    let target = args.target.clone();
    let mut output = retune_thermal_self_test_run(ThermalRetuneInput {
        run_dir: args.run_dir,
        optimize_targets_c: args.optimize_targets_c,
    })?;
    if !apply_preview {
        return Ok(output.summary);
    }

    let target_value = json!({
        "deviceId": target.device,
        "hardwareId": target.hardware,
        "devd": default_devd,
    });
    let resolved = match resolve_target(target, default_devd) {
        Ok(resolved) => resolved,
        Err(error) => {
            output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
                ok: false,
                target: target_value,
                preview_response: None,
                status_readback: None,
                error: Some(error.to_string()),
            })?;
            return Err(error);
        }
    };

    let profile_mode = match output.profile_mode {
        Some(profile_mode) => profile_mode,
        None => {
            let error = io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal retune preview apply requires run.json selectedMode=auto|65w|100w",
            );
            output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
                ok: false,
                target: target_value,
                preview_response: None,
                status_readback: None,
                error: Some(error.to_string()),
            })?;
            return Err(error.into());
        }
    };

    let preview_response = match request_with_lease(
        client,
        resolved.clone(),
        Method::PUT,
        "/runtime",
        Some(thermal_profile_preview_runtime_body(
            profile_mode,
            output.candidate_profile.clone(),
        )),
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
                ok: false,
                target: target_value,
                preview_response: None,
                status_readback: None,
                error: Some(error.to_string()),
            })?;
            return Err(error);
        }
    };

    let status = match request_with_lease(client, resolved, Method::GET, "/status", None).await {
        Ok(status) => status,
        Err(error) => {
            output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
                ok: false,
                target: target_value,
                preview_response: Some(thermal_retune_status_summary(&preview_response)),
                status_readback: None,
                error: Some(error.to_string()),
            })?;
            return Err(error);
        }
    };
    let readback_result = (|| {
        verify_thermal_profile_mode_readback(&status, profile_mode)?;
        let target_temp_c = status
            .get("targetTempC")
            .and_then(Value::as_i64)
            .and_then(|value| i16::try_from(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "thermal retune preview status missing targetTempC",
                )
            })?;
        let expected = thermal_heater_parameters_value(
            target_temp_c,
            Some(&output.candidate_profile),
            "preview",
        );
        verify_thermal_control_readback(&status, &expected, "preview")
    })();
    if let Err(error) = readback_result {
        output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
            ok: false,
            target: target_value,
            preview_response: Some(thermal_retune_status_summary(&preview_response)),
            status_readback: Some(thermal_retune_status_summary(&status)),
            error: Some(error.to_string()),
        })?;
        return Err(error);
    }

    output.write_apply_preview_receipt(ThermalRetuneApplyReceipt {
        ok: true,
        target: target_value,
        preview_response: Some(thermal_retune_status_summary(&preview_response)),
        status_readback: Some(thermal_retune_status_summary(&status)),
        error: None,
    })?;
    Ok(output.summary)
}

fn thermal_retune_profile_mode(summary: &Value) -> Option<ThermalProfileMode> {
    match summary.get("selectedMode").and_then(Value::as_str) {
        Some("auto") => Some(ThermalProfileMode::Auto),
        Some("65w") => Some(ThermalProfileMode::W65),
        Some("100w") => Some(ThermalProfileMode::W100),
        _ => None,
    }
}

fn thermal_retune_status_summary(status: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    for key in [
        "deviceId",
        "mode",
        "targetTempC",
        "currentTempC",
        "heaterEnabled",
        "thermalControlProfilePreview",
        "thermalControl",
        "heaterFaultReason",
        "faultAttentionPending",
    ] {
        if let Some(value) = status.get(key) {
            summary.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(summary)
}

fn write_json_pretty(
    path: &Path,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
