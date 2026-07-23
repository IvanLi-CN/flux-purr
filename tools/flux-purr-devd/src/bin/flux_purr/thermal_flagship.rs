use std::{
    cmp::Ordering,
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

use reqwest::{Client, Method};
use serde_json::{Value, json};

use super::{
    TargetSelector, ThermalCandidatePoint, ThermalCandidateProfile, ThermalFlagshipTuneArgs,
    ThermalFullSpeedStableTracker, ThermalProfileMode, ThermalSelfTestArgs,
    ThermalSelfTestEvaluationMode, collect_batch_thermal_self_test,
    collect_single_thermal_self_test, current_unix_millis,
    load_thermal_default_seed_candidate_profile, parse_thermal_targets,
    parse_thermal_targets_preserve_order, request_json, resolve_target, thermal_candidate_point,
    thermal_candidate_point_from_heater_parameters, thermal_candidate_profile_from_value,
    thermal_candidate_profile_to_value, thermal_heater_parameters_value,
    thermal_interpolated_candidate_point, thermal_rebuild_profile_from_anchor_targets,
    thermal_retune, thermal_stage_result_from_value, tune_thermal_candidate_point,
};

const DEFAULT_OUTPUT_ROOT: &str = "thermal-self-test-runs";
const SCOUT_STAGE_TIMEOUT_SECONDS: u64 = 180;
const SCOUT_WARMUP_TIMEOUT_SECONDS: u64 = 180;
const FLAGSHIP_ENVIRONMENT_RETRY_LIMIT: u8 = 1;
const SOURCE_PRESET_100W: &str = "21V / 5.0A";
const SOURCE_PRESET_65W: &str = "20V / 3.25A";
const PROVIDER_ISOLAPURR: &str = "IsolaPurr";

#[derive(Debug, Clone)]
struct SelfTestRun {
    summary: Value,
    run_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct SelectedBatchRun {
    summary: Value,
    candidate_profile_file: PathBuf,
    score: CandidateScore,
}

#[derive(Debug, Clone, Copy)]
struct CandidateScore([f64; 10]);

impl CandidateScore {
    fn to_value(self) -> Value {
        Value::Array(
            self.0
                .into_iter()
                .map(|value| {
                    if value.is_finite() {
                        json!(value)
                    } else {
                        Value::Null
                    }
                })
                .collect(),
        )
    }
}

impl PartialEq for CandidateScore {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for CandidateScore {}

impl PartialOrd for CandidateScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CandidateScore {
    fn cmp(&self, other: &Self) -> Ordering {
        for (left, right) in self.0.iter().zip(other.0.iter()) {
            match left.partial_cmp(right).unwrap_or(Ordering::Equal) {
                Ordering::Equal => continue,
                ordering => return ordering,
            }
        }
        Ordering::Equal
    }
}

pub(super) async fn run_flagship_tuning(
    client: &Client,
    default_devd: &str,
    args: ThermalFlagshipTuneArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let target_selector = flagship_target_selector(&args);
    let anchors_c = parse_thermal_targets(Some(&args.anchor_targets_c))?;
    let validation_targets_c = parse_thermal_targets(Some(&args.validation_targets_c))?;
    let tune_targets_c = parse_thermal_targets_preserve_order(Some(&args.tune_targets_c))?;
    validate_flagship_scope(&anchors_c, &validation_targets_c, &tune_targets_c)?;
    let output_root = effective_output_root(&args.output_root);
    fs::create_dir_all(&output_root)?;
    let bundle_dir = args
        .bundle_dir
        .clone()
        .unwrap_or_else(|| output_root.join("preliminary-review-bundle"));

    let resolved = if args.dry_run {
        None
    } else {
        Some(resolve_target(target_selector.clone(), default_devd)?)
    };
    let device_id = resolved
        .as_ref()
        .map(|target| target.device.clone())
        .unwrap_or_else(|| "mock-fp-lab-01".to_string());
    let port_path = if let Some(resolved) = resolved.as_ref() {
        resolve_device_port_path(client, &resolved.devd, &resolved.device)
            .await
            .unwrap_or_else(|| "unknown-port".to_string())
    } else {
        "dry-run".to_string()
    };

    let mut current_profile = initial_sparse_profile(&args, &anchors_c)?;
    let initial_profile_path = output_root.join("seed").join("initial-sparse-profile.json");
    write_json_pretty(&initial_profile_path, &current_profile)?;

    let mut review_entries = Vec::<Value>::new();
    for target_temp_c in tune_targets_c.iter().copied() {
        let (updated_profile, entry) = tune_flagship_target(
            client,
            default_devd,
            &args,
            &target_selector,
            current_profile,
            target_temp_c,
            &anchors_c,
            &output_root.join(format!("target-{target_temp_c}")),
        )
        .await?;
        current_profile = updated_profile;
        write_json_pretty(
            &output_root
                .join(format!("target-{target_temp_c}"))
                .join("review-entry.json"),
            &entry,
        )?;
        write_json_pretty(
            &output_root
                .join(format!("target-{target_temp_c}"))
                .join("accepted-sparse-profile.json"),
            &current_profile,
        )?;
        review_entries.push(entry);
        write_json_pretty(
            &output_root.join("review-entries.json"),
            &Value::Array(review_entries.clone()),
        )?;
        write_json_pretty(
            &output_root.join("review-candidate-profile.json"),
            &current_profile,
        )?;
    }
    for target_temp_c in validation_targets_c.iter().copied() {
        let entry = validate_flagship_target(
            client,
            default_devd,
            &args,
            &target_selector,
            &current_profile,
            target_temp_c,
            &output_root.join(format!("validation-{target_temp_c}")),
        )
        .await?;
        write_json_pretty(
            &output_root
                .join(format!("validation-{target_temp_c}"))
                .join("review-entry.json"),
            &entry,
        )?;
        review_entries.push(entry);
        write_json_pretty(
            &output_root.join("review-entries.json"),
            &Value::Array(review_entries.clone()),
        )?;
        if validation_entry_should_trigger_supplemental_tuning(
            review_entries
                .last()
                .ok_or("missing validation review entry")?,
        ) {
            let supplemental_anchors_c = supplemental_anchor_targets(&anchors_c, target_temp_c);
            let normalized_profile =
                normalize_sparse_profile_value(&current_profile, &supplemental_anchors_c)?;
            let (updated_profile, mut supplemental_entry) = tune_flagship_target(
                client,
                default_devd,
                &args,
                &target_selector,
                normalized_profile,
                target_temp_c,
                &supplemental_anchors_c,
                &output_root.join(format!("validation-{target_temp_c}-supplemental-tune")),
            )
            .await?;
            supplemental_entry["targetRole"] = json!("supplemental_tuning");
            supplemental_entry["supplementalForTargetC"] = json!(target_temp_c);
            supplemental_entry["supplementalReason"] =
                json!("validation_failed_with_valid_evidence");
            current_profile = updated_profile;
            write_json_pretty(
                &output_root
                    .join(format!("validation-{target_temp_c}-supplemental-tune"))
                    .join("review-entry.json"),
                &supplemental_entry,
            )?;
            write_json_pretty(
                &output_root.join("review-candidate-profile.json"),
                &current_profile,
            )?;
            review_entries.push(supplemental_entry);
            write_json_pretty(
                &output_root.join("review-entries.json"),
                &Value::Array(review_entries.clone()),
            )?;
        }
    }

    let bundle = super::thermal_report::write_preliminary_review_bundle(
        &bundle_dir,
        &current_profile,
        review_entries.clone(),
        &args.source_id,
        &device_id,
        &port_path,
        i64::try_from(args.per_target_budget_seconds).unwrap_or(i64::MAX),
        json!(current_unix_millis()),
        args.profile_mode.as_str(),
        flagship_resolved_bank(args.profile_mode),
        flagship_detected_source_class(args.profile_mode),
        &anchors_c,
        &validation_targets_c,
        &tune_targets_c,
        source_preset(args.profile_mode),
        PROVIDER_ISOLAPURR,
    )?;

    let supplemental_candidate_ready_targets_c =
        unique_i64(review_entries.iter().filter_map(|entry| {
            (entry.get("targetRole").and_then(Value::as_str) == Some("supplemental_tuning")
                && entry.get("candidateReady").and_then(Value::as_bool) == Some(true))
            .then(|| entry.get("target").and_then(Value::as_i64))
            .flatten()
        }));
    let supplemental_candidate_ready_targets_set = supplemental_candidate_ready_targets_c
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let validation_failed_targets_c = unique_i64(review_entries.iter().filter_map(|entry| {
        if entry.get("targetRole").and_then(Value::as_str) == Some("validation")
            && entry.get("budgetOutcome").and_then(Value::as_str) != Some("validation_passed")
        {
            entry
                .get("target")
                .and_then(Value::as_i64)
                .filter(|target| !supplemental_candidate_ready_targets_set.contains(target))
        } else {
            None
        }
    }));
    let summary_ok = review_entries.iter().all(|entry| {
        let target = entry.get("target").and_then(Value::as_i64);
        match entry.get("targetRole").and_then(Value::as_str) {
            Some("validation") => {
                entry.get("budgetOutcome").and_then(Value::as_str) == Some("validation_passed")
                    || target.is_some_and(|target| {
                        supplemental_candidate_ready_targets_set.contains(&target)
                    })
            }
            _ => entry.get("candidateReady").and_then(Value::as_bool) == Some(true),
        }
    });

    Ok(json!({
        "ok": summary_ok,
        "kind": "thermal_flagship_tuning",
        "mode": if args.dry_run { "dry_run" } else { "real_hil" },
        "profileMode": args.profile_mode.as_str(),
        "resolvedBank": flagship_resolved_bank(args.profile_mode),
        "detectedSourceClass": flagship_detected_source_class(args.profile_mode),
        "anchorsC": anchors_c,
        "validationTargetsC": validation_targets_c,
        "tuneTargetsC": tune_targets_c,
        "perTargetBudgetSeconds": args.per_target_budget_seconds,
        "maxTuningRounds": effective_round_limit(&args),
        "scoutHoldSeconds": args.scout_hold_seconds,
        "confirmHoldSeconds": args.confirm_hold_seconds,
        "outputRoot": display_path(&output_root),
        "acceptedProfilePath": display_path(&output_root.join("review-candidate-profile.json")),
        "bundleDir": display_path(&bundle_dir),
        "bundleJson": bundle.pointer("/files/bundleJson").cloned().unwrap_or(Value::Null),
        "bundleIndexHtml": bundle.pointer("/files/indexHtml").cloned().unwrap_or(Value::Null),
        "reviewOutcomes": review_entries.iter().map(|entry| {
            (
                entry.get("target").and_then(Value::as_i64).unwrap_or_default().to_string(),
                entry.get("budgetOutcome").cloned().unwrap_or(Value::Null),
            )
        }).collect::<serde_json::Map<String, Value>>(),
        "candidateDispositions": review_entries.iter().map(|entry| {
            (
                entry.get("target").and_then(Value::as_i64).unwrap_or_default().to_string(),
                entry.get("candidateDisposition").cloned().unwrap_or(Value::Null),
            )
        }).collect::<serde_json::Map<String, Value>>(),
        "candidateReadyTargetsC": unique_i64(review_entries.iter().filter_map(|entry| {
            (entry.get("candidateReady").and_then(Value::as_bool) == Some(true))
                .then(|| entry.get("target").and_then(Value::as_i64))
                .flatten()
        })),
        "supplementalTuningTargetsC": unique_i64(review_entries.iter().filter_map(|entry| {
            (entry.get("targetRole").and_then(Value::as_str) == Some("supplemental_tuning"))
                .then(|| entry.get("target").and_then(Value::as_i64))
                .flatten()
        })),
        "supplementalCandidateReadyTargetsC": supplemental_candidate_ready_targets_c,
        "validationPassedTargetsC": unique_i64(review_entries.iter().filter_map(|entry| {
            if entry.get("targetRole").and_then(Value::as_str) == Some("validation")
                && entry.get("budgetOutcome").and_then(Value::as_str) == Some("validation_passed")
            {
                entry.get("target").and_then(Value::as_i64)
            } else {
                None
            }
        })),
        "validationFailedTargetsC": validation_failed_targets_c,
    }))
}

fn unique_i64(values: impl IntoIterator<Item = i64>) -> Vec<i64> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn validation_entry_should_trigger_supplemental_tuning(entry: &Value) -> bool {
    entry.get("targetRole").and_then(Value::as_str) == Some("validation")
        && entry.get("budgetOutcome").and_then(Value::as_str) == Some("validation_failed")
        && entry
            .get("validTestCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
}

fn supplemental_anchor_targets(anchors_c: &[i16], target_temp_c: i16) -> Vec<i16> {
    let mut targets = anchors_c.to_vec();
    targets.push(target_temp_c);
    targets.sort_unstable();
    targets.dedup();
    targets
}

#[allow(clippy::too_many_arguments)]
async fn tune_flagship_target(
    client: &Client,
    default_devd: &str,
    args: &ThermalFlagshipTuneArgs,
    target_selector: &TargetSelector,
    mut current_profile: Value,
    target_temp_c: i16,
    anchors_c: &[i16],
    workspace_dir: &Path,
) -> Result<(Value, Value), Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(workspace_dir)?;
    let budget_started_at = Instant::now();
    let cooldown_temp_c = cooldown_threshold(target_temp_c);
    let mut rounds = Vec::<Value>::new();
    let mut last_summary = synthetic_failure_summary(target_temp_c, "no_round_completed");
    let mut budget_outcome = "not_converged".to_string();
    let mut round_index = 0u32;

    loop {
        if budget_exhausted(budget_started_at, args.per_target_budget_seconds) {
            budget_outcome = "budget_exhausted".to_string();
            break;
        }
        if effective_round_limit(args).is_some_and(|limit| round_index >= limit) {
            break;
        }
        round_index += 1;
        let round_dir = workspace_dir.join(format!("round-{round_index}"));
        fs::create_dir_all(&round_dir)?;
        let round_seed = round_dir.join("current-sparse.json");
        write_json_pretty(
            &round_seed,
            &target_local_profile_window(&current_profile, target_temp_c)?,
        )?;

        let mut scout_retry_count = 0u8;
        let scout = loop {
            let scout = match run_budgeted_self_test(
                client,
                default_devd,
                args,
                target_selector,
                SelfTestRequest {
                    seed_profile_file: Some(round_seed.clone()),
                    candidate_profile_files: Vec::new(),
                    target_temp_c,
                    hold_seconds: args.scout_hold_seconds,
                    output_dir: round_dir.join("scout"),
                    evaluation_mode: ThermalSelfTestEvaluationMode::TuningScout,
                    cooldown_temp_c,
                    budget_started_at,
                },
            )
            .await
            {
                Ok(run) => run,
                Err(error) if error.to_string().contains("target_budget_exhausted") => {
                    budget_outcome = "budget_exhausted".to_string();
                    last_summary =
                        synthetic_failure_summary(target_temp_c, "target_budget_exhausted");
                    break None;
                }
                Err(error) => {
                    let message = error.to_string();
                    if scout_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                        && flagship_retryable_environment_error_message(&message)
                    {
                        scout_retry_count += 1;
                        continue;
                    }
                    budget_outcome = "environment_blocked".to_string();
                    last_summary = synthetic_failure_summary(
                        target_temp_c,
                        &format!("round_execution_failed: {message}"),
                    );
                    break None;
                }
            };
            ensure_expected_source(&scout.summary, args.profile_mode)?;
            last_summary = scout.summary.clone();
            rounds.push(round_record_from_summary(
                &scout.summary,
                target_temp_c,
                rounds.len() + 1,
                &format!("tuning {round_index} / scout"),
                explicit_point_value(&current_profile, target_temp_c),
                "scout",
                Some(round_index),
                None,
                false,
                None,
                budget_elapsed_seconds(budget_started_at),
            ));
            if run_is_disqualified(&scout.summary, target_temp_c) {
                if scout_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                    && flagship_retryable_environment_summary(&scout.summary, target_temp_c)
                {
                    scout_retry_count += 1;
                    continue;
                }
                budget_outcome = "environment_blocked".to_string();
                break None;
            }
            break Some(scout);
        };
        let Some(scout) = scout else {
            break;
        };
        if !warmup_output_is_full(&scout.summary, target_temp_c) {
            budget_outcome = "not_converged".to_string();
            break;
        }
        if scout_current_is_promotable(&scout.summary, target_temp_c) {
            let mut confirm_retry_count = 0u8;
            let confirm = loop {
                let confirm = match run_hold_confirm_for_profile(
                    client,
                    default_devd,
                    args,
                    target_selector,
                    &current_profile,
                    target_temp_c,
                    workspace_dir,
                    round_index,
                    cooldown_temp_c,
                    budget_started_at,
                )
                .await
                {
                    Ok(run) => run,
                    Err(error) if error.to_string().contains("target_budget_exhausted") => {
                        budget_outcome = "budget_exhausted".to_string();
                        last_summary =
                            synthetic_failure_summary(target_temp_c, "target_budget_exhausted");
                        break None;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        if confirm_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                            && flagship_retryable_environment_error_message(&message)
                        {
                            confirm_retry_count += 1;
                            continue;
                        }
                        budget_outcome = "environment_blocked".to_string();
                        last_summary = synthetic_failure_summary(
                            target_temp_c,
                            &format!("hold_confirm_failed: {message}"),
                        );
                        break None;
                    }
                };
                ensure_expected_source(&confirm.summary, args.profile_mode)?;
                last_summary = confirm.summary.clone();
                rounds.push(round_record_from_summary(
                    &confirm.summary,
                    target_temp_c,
                    rounds.len() + 1,
                    "hold confirm",
                    explicit_point_value(&current_profile, target_temp_c),
                    "hold_confirm",
                    Some(round_index),
                    None,
                    true,
                    None,
                    budget_elapsed_seconds(budget_started_at),
                ));
                if run_is_disqualified(&confirm.summary, target_temp_c) {
                    if confirm_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                        && flagship_retryable_environment_summary(&confirm.summary, target_temp_c)
                    {
                        confirm_retry_count += 1;
                        continue;
                    }
                    budget_outcome = "environment_blocked".to_string();
                    break None;
                }
                break Some(confirm);
            };
            let Some(confirm) = confirm else {
                break;
            };
            if confirm
                .summary
                .pointer("/validation/passed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                budget_outcome = "completed".to_string();
                break;
            }
            if let Some(reseeded) =
                reseed_after_failed_hold_confirm(&current_profile, target_temp_c, &confirm.summary)?
            {
                current_profile = normalize_sparse_profile_value(&reseeded, anchors_c)?;
                write_json_pretty(
                    &workspace_dir.join(format!("hold-confirm-{round_index}-reseed.json")),
                    &current_profile,
                )?;
            }
            budget_outcome = "not_converged".to_string();
            continue;
        }

        let retuned =
            thermal_retune::retune_thermal_self_test_run(thermal_retune::ThermalRetuneInput {
                run_dir: scout.run_dir.clone(),
                optimize_targets_c: Some(target_temp_c.to_string()),
            })?;
        let retuned_profile =
            normalize_sparse_profile_value(&retuned.candidate_profile, anchors_c)?;
        write_json_pretty(
            &round_dir.join("thermal-profile.replayed.sparse.json"),
            &retuned_profile,
        )?;
        let variants = candidate_variants(
            &current_profile,
            &retuned_profile,
            &scout.summary,
            target_temp_c,
            anchors_c,
        )?;
        let candidate_paths =
            write_candidate_variants(&round_dir.join("candidates"), &variants, target_temp_c)?;

        let mut batch_retry_count = 0u8;
        let batch_outcome = loop {
            let batch = match run_budgeted_self_test(
                client,
                default_devd,
                args,
                target_selector,
                SelfTestRequest {
                    seed_profile_file: None,
                    candidate_profile_files: candidate_paths.clone(),
                    target_temp_c,
                    hold_seconds: args.scout_hold_seconds,
                    output_dir: round_dir.join("batch"),
                    evaluation_mode: ThermalSelfTestEvaluationMode::TuningScout,
                    cooldown_temp_c,
                    budget_started_at,
                },
            )
            .await
            {
                Ok(run) => run,
                Err(error) if error.to_string().contains("target_budget_exhausted") => {
                    budget_outcome = "budget_exhausted".to_string();
                    last_summary =
                        synthetic_failure_summary(target_temp_c, "target_budget_exhausted");
                    break None;
                }
                Err(error) => {
                    let message = error.to_string();
                    if batch_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                        && flagship_retryable_environment_error_message(&message)
                    {
                        batch_retry_count += 1;
                        continue;
                    }
                    budget_outcome = "environment_blocked".to_string();
                    last_summary = synthetic_failure_summary(
                        target_temp_c,
                        &format!("batch_execution_failed: {message}"),
                    );
                    break None;
                }
            };
            ensure_batch_source(&batch.summary, args.profile_mode)?;
            let diagnostic_best = match choose_best_batch_run(&batch.summary, target_temp_c) {
                Ok(best) => best,
                Err(error) => {
                    rounds.extend(batch_attempt_records(
                        &batch.summary,
                        target_temp_c,
                        rounds.len() + 1,
                        round_index,
                        "",
                        budget_elapsed_seconds(budget_started_at),
                    ));
                    last_summary = first_batch_run_summary(&batch.summary).unwrap_or_else(|| {
                        synthetic_failure_summary(target_temp_c, "batch_no_selected_candidate")
                    });
                    if batch_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                        && flagship_retryable_environment_batch_summary(
                            &batch.summary,
                            target_temp_c,
                        )
                    {
                        batch_retry_count += 1;
                        continue;
                    }
                    budget_outcome = "environment_blocked".to_string();
                    last_summary = synthetic_failure_summary(
                        target_temp_c,
                        &format!("batch_execution_failed: {error}"),
                    );
                    break None;
                }
            };
            let promoted_best = choose_promotable_batch_run(&batch.summary, target_temp_c)?;
            let selected_best = promoted_best
                .clone()
                .unwrap_or_else(|| diagnostic_best.clone());
            let selected_run_id = selected_best
                .summary
                .get("runId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            rounds.extend(batch_attempt_records(
                &batch.summary,
                target_temp_c,
                rounds.len() + 1,
                round_index,
                &selected_run_id,
                budget_elapsed_seconds(budget_started_at),
            ));
            break Some((promoted_best, selected_best));
        };
        let Some((promoted_best, selected_best)) = batch_outcome else {
            break;
        };
        let chosen_profile = read_json(&selected_best.candidate_profile_file)?;
        current_profile = normalize_sparse_profile_value(&chosen_profile, anchors_c)?;
        write_json_pretty(
            &round_dir.join("accepted-sparse-profile.json"),
            &current_profile,
        )?;
        last_summary = selected_best.summary.clone();

        if promoted_best.is_none() {
            continue;
        }
        if budget_exhausted(budget_started_at, args.per_target_budget_seconds) {
            budget_outcome = "budget_exhausted".to_string();
            break;
        }
        let mut confirm_retry_count = 0u8;
        let confirm = loop {
            let confirm = match run_hold_confirm_for_profile(
                client,
                default_devd,
                args,
                target_selector,
                &current_profile,
                target_temp_c,
                workspace_dir,
                round_index,
                cooldown_temp_c,
                budget_started_at,
            )
            .await
            {
                Ok(run) => run,
                Err(error) if error.to_string().contains("target_budget_exhausted") => {
                    budget_outcome = "budget_exhausted".to_string();
                    last_summary =
                        synthetic_failure_summary(target_temp_c, "target_budget_exhausted");
                    break None;
                }
                Err(error) => {
                    let message = error.to_string();
                    if confirm_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                        && flagship_retryable_environment_error_message(&message)
                    {
                        confirm_retry_count += 1;
                        continue;
                    }
                    budget_outcome = "environment_blocked".to_string();
                    last_summary = synthetic_failure_summary(
                        target_temp_c,
                        &format!("hold_confirm_failed: {message}"),
                    );
                    break None;
                }
            };
            ensure_expected_source(&confirm.summary, args.profile_mode)?;
            last_summary = confirm.summary.clone();
            rounds.push(round_record_from_summary(
                &confirm.summary,
                target_temp_c,
                rounds.len() + 1,
                "hold confirm",
                explicit_point_value(&current_profile, target_temp_c),
                "hold_confirm",
                Some(round_index),
                None,
                true,
                None,
                budget_elapsed_seconds(budget_started_at),
            ));
            if run_is_disqualified(&confirm.summary, target_temp_c) {
                if confirm_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                    && flagship_retryable_environment_summary(&confirm.summary, target_temp_c)
                {
                    confirm_retry_count += 1;
                    continue;
                }
                budget_outcome = "environment_blocked".to_string();
                break None;
            }
            break Some(confirm);
        };
        let Some(confirm) = confirm else {
            break;
        };
        if confirm
            .summary
            .pointer("/validation/passed")
            .and_then(Value::as_bool)
            == Some(true)
        {
            budget_outcome = "completed".to_string();
            break;
        }
        if let Some(reseeded) =
            reseed_after_failed_hold_confirm(&current_profile, target_temp_c, &confirm.summary)?
        {
            current_profile = normalize_sparse_profile_value(&reseeded, anchors_c)?;
            write_json_pretty(
                &workspace_dir.join(format!("hold-confirm-{round_index}-reseed.json")),
                &current_profile,
            )?;
        }
        budget_outcome = "not_converged".to_string();
    }

    let entry = review_target_entry(
        target_temp_c,
        &budget_outcome,
        budget_elapsed_seconds(budget_started_at),
        rounds,
        &last_summary,
        &current_profile,
        args.confirm_hold_seconds,
    );
    Ok((current_profile, entry))
}

#[allow(clippy::too_many_arguments)]
async fn validate_flagship_target(
    client: &Client,
    default_devd: &str,
    args: &ThermalFlagshipTuneArgs,
    target_selector: &TargetSelector,
    current_profile: &Value,
    target_temp_c: i16,
    workspace_dir: &Path,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(workspace_dir)?;
    let budget_started_at = Instant::now();
    let cooldown_temp_c = cooldown_threshold(target_temp_c);
    let seed_profile_file = workspace_dir.join("final-sparse-profile.json");
    let validation_profile = validation_preview_profile_for_target(current_profile, target_temp_c)?;
    write_json_pretty(&seed_profile_file, &validation_profile)?;

    let mut rounds = Vec::<Value>::new();
    let mut validation_retry_count = 0u8;

    let (budget_outcome, last_summary) = loop {
        if budget_exhausted(budget_started_at, args.per_target_budget_seconds) {
            break (
                "budget_exhausted".to_string(),
                synthetic_failure_summary(target_temp_c, "target_budget_exhausted"),
            );
        }
        let attempt_number = rounds.len() + 1;
        let validation = match run_budgeted_self_test(
            client,
            default_devd,
            args,
            target_selector,
            SelfTestRequest {
                seed_profile_file: Some(seed_profile_file.clone()),
                candidate_profile_files: Vec::new(),
                target_temp_c,
                hold_seconds: args.confirm_hold_seconds,
                output_dir: workspace_dir.join(format!("attempt-{attempt_number}")),
                evaluation_mode: ThermalSelfTestEvaluationMode::HoldConfirm,
                cooldown_temp_c,
                budget_started_at,
            },
        )
        .await
        {
            Ok(run) => run,
            Err(error) if error.to_string().contains("target_budget_exhausted") => {
                break (
                    "budget_exhausted".to_string(),
                    synthetic_failure_summary(target_temp_c, "target_budget_exhausted"),
                );
            }
            Err(error) => {
                let message = error.to_string();
                if validation_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                    && flagship_retryable_environment_error_message(&message)
                {
                    validation_retry_count += 1;
                    continue;
                }
                break (
                    "environment_blocked".to_string(),
                    synthetic_failure_summary(
                        target_temp_c,
                        &format!("validation_execution_failed: {message}"),
                    ),
                );
            }
        };

        let source_ok = ensure_expected_source(&validation.summary, args.profile_mode).is_ok();
        let run_summary = validation.summary.clone();
        rounds.push(round_record_from_summary(
            &validation.summary,
            target_temp_c,
            rounds.len() + 1,
            "validation / final profile",
            effective_profile_point(&validation_profile, target_temp_c),
            "validation",
            None,
            Some("final-profile"),
            true,
            None,
            budget_elapsed_seconds(budget_started_at),
        ));

        if !source_ok {
            break ("environment_blocked".to_string(), run_summary);
        }
        if run_is_disqualified(&validation.summary, target_temp_c) {
            if validation_retry_count < FLAGSHIP_ENVIRONMENT_RETRY_LIMIT
                && flagship_retryable_environment_summary(&validation.summary, target_temp_c)
            {
                validation_retry_count += 1;
                continue;
            }
            break ("environment_blocked".to_string(), run_summary);
        }
        break (
            if validation
                .summary
                .pointer("/validation/passed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                "validation_passed".to_string()
            } else {
                "validation_failed".to_string()
            },
            run_summary,
        );
    };

    Ok(validation_target_entry(
        target_temp_c,
        &budget_outcome,
        budget_elapsed_seconds(budget_started_at),
        rounds,
        &last_summary,
        &validation_profile,
        args.confirm_hold_seconds,
    ))
}

fn validation_preview_profile_for_target(
    profile_value: &Value,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    target_local_profile_window(profile_value, target_temp_c)
}

fn target_local_profile_window(
    profile_value: &Value,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut profile = thermal_candidate_profile_from_value(profile_value.clone());
    profile.points.sort_by_key(|point| point.target_temp_c);
    let target_point = thermal_candidate_point(&profile, target_temp_c)
        .or_else(|| thermal_interpolated_candidate_point(&profile, target_temp_c))
        .or_else(|| {
            thermal_candidate_point_from_heater_parameters(&thermal_heater_parameters_value(
                target_temp_c,
                Some(profile_value),
                "preview",
            ))
            .ok()
        })
        .ok_or_else(|| format!("target profile window could not materialize {target_temp_c}C"))?;
    let mut window = Vec::<ThermalCandidatePoint>::new();
    if let Some(lower) = profile
        .points
        .iter()
        .copied()
        .rev()
        .find(|point| point.target_temp_c < target_temp_c)
    {
        window.push(lower);
    }
    window.push(target_point);
    if let Some(upper) = profile
        .points
        .iter()
        .copied()
        .find(|point| point.target_temp_c > target_temp_c)
    {
        window.push(upper);
    }
    window.sort_by_key(|point| point.target_temp_c);
    window.dedup_by_key(|point| point.target_temp_c);
    Ok(thermal_candidate_profile_to_value(
        &ThermalCandidateProfile {
            settings: profile.settings,
            points: window,
        },
    ))
}

#[derive(Debug, Clone)]
struct SelfTestRequest {
    seed_profile_file: Option<PathBuf>,
    candidate_profile_files: Vec<PathBuf>,
    target_temp_c: i16,
    hold_seconds: u64,
    output_dir: PathBuf,
    evaluation_mode: ThermalSelfTestEvaluationMode,
    cooldown_temp_c: f64,
    budget_started_at: Instant,
}

async fn run_budgeted_self_test(
    client: &Client,
    default_devd: &str,
    args: &ThermalFlagshipTuneArgs,
    target_selector: &TargetSelector,
    request: SelfTestRequest,
) -> Result<SelfTestRun, Box<dyn std::error::Error + Send + Sync>> {
    let remaining =
        budget_remaining_seconds(request.budget_started_at, args.per_target_budget_seconds);
    let (cooldown_timeout_seconds, stage_timeout_seconds, warmup_timeout_seconds) =
        step_timeouts_for_budget(remaining).ok_or("target_budget_exhausted")?;
    let requested_output_dir = request.output_dir.clone();
    let self_args = ThermalSelfTestArgs {
        target: target_selector.clone(),
        source_kind: args.source_kind,
        source_id: args.source_id.clone(),
        source_url: args.source_url.clone(),
        profile_mode: args.profile_mode,
        source_voltage_v: args.source_voltage_v.clone(),
        source_current_a: args.source_current_a.clone(),
        source_power_watts: args.source_power_watts.unwrap_or(0),
        source_mode: args.source_mode.clone(),
        sample_interval_ms: args.sample_interval_ms,
        evaluation_mode: request.evaluation_mode,
        hold_seconds: request.hold_seconds,
        stage_timeout_seconds,
        warmup_timeout_seconds,
        runtime_rearm_attempts: args.runtime_rearm_attempts,
        calibration_run: false,
        optimize_targets_c: Some(request.target_temp_c.to_string()),
        skip_optimize: true,
        cooldown_temp_c: request.cooldown_temp_c,
        cooldown_timeout_seconds,
        targets_c: Some(request.target_temp_c.to_string()),
        seed_profile_file: request.seed_profile_file,
        candidate_profile_files: request.candidate_profile_files,
        output_dir: request.output_dir,
        dry_run: args.dry_run,
    };
    let summary = if self_args.candidate_profile_files.is_empty() {
        collect_single_thermal_self_test(client, default_devd, self_args, false).await?
    } else {
        collect_batch_thermal_self_test(client, default_devd, self_args, request.target_temp_c)
            .await?
    };
    if summary_is_pre_sample_cooldown_timeout(&summary) {
        let message = summary
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("thermal self-test cooldown precondition exhausted target budget");
        return Err(format!("target_budget_exhausted: {message}").into());
    }
    let run_dir = summary_run_dir(&summary).unwrap_or_else(|_| {
        summary
            .get("batchId")
            .and_then(Value::as_str)
            .map(|batch_id| requested_output_dir.join(batch_id))
            .unwrap_or(requested_output_dir)
    });
    Ok(SelfTestRun { summary, run_dir })
}

#[allow(clippy::too_many_arguments)]
async fn run_hold_confirm_for_profile(
    client: &Client,
    default_devd: &str,
    args: &ThermalFlagshipTuneArgs,
    target_selector: &TargetSelector,
    current_profile: &Value,
    target_temp_c: i16,
    workspace_dir: &Path,
    round_index: u32,
    cooldown_temp_c: f64,
    budget_started_at: Instant,
) -> Result<SelfTestRun, Box<dyn std::error::Error + Send + Sync>> {
    let hold_seed = workspace_dir.join(format!("hold-confirm-{round_index}-seed.json"));
    write_json_pretty(
        &hold_seed,
        &target_local_profile_window(current_profile, target_temp_c)?,
    )?;
    run_budgeted_self_test(
        client,
        default_devd,
        args,
        target_selector,
        SelfTestRequest {
            seed_profile_file: Some(hold_seed),
            candidate_profile_files: Vec::new(),
            target_temp_c,
            hold_seconds: args.confirm_hold_seconds,
            output_dir: workspace_dir.join(format!("hold-confirm-{round_index}")),
            evaluation_mode: ThermalSelfTestEvaluationMode::HoldConfirm,
            cooldown_temp_c,
            budget_started_at,
        },
    )
    .await
}

fn initial_sparse_profile(
    args: &ThermalFlagshipTuneArgs,
    anchors_c: &[i16],
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let profile = if let Some(seed_profile_file) = args.seed_profile_file.as_ref() {
        read_json(seed_profile_file)?
    } else {
        let bank = flagship_resolved_bank(args.profile_mode);
        let (profile, _) = load_thermal_default_seed_candidate_profile(bank)?;
        thermal_candidate_profile_to_value(&profile)
    };
    normalize_sparse_profile_value(&profile, anchors_c)
}

fn normalize_sparse_profile_value(
    profile_value: &Value,
    anchors_c: &[i16],
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let profile = thermal_candidate_profile_from_value(profile_value.clone());
    let mut points = Vec::<ThermalCandidatePoint>::new();
    for target_temp_c in anchors_c.iter().copied() {
        let point = thermal_candidate_point(&profile, target_temp_c)
            .or_else(|| thermal_interpolated_candidate_point(&profile, target_temp_c))
            .or_else(|| {
                thermal_candidate_point_from_heater_parameters(&thermal_heater_parameters_value(
                    target_temp_c,
                    Some(profile_value),
                    "preview",
                ))
                .ok()
            })
            .ok_or_else(|| {
                format!("sparse normalization could not materialize {target_temp_c}C")
            })?;
        points.push(point);
    }
    Ok(thermal_candidate_profile_to_value(
        &ThermalCandidateProfile {
            settings: profile.settings,
            points,
        },
    ))
}

fn candidate_variants(
    current_profile: &Value,
    retuned_profile: &Value,
    scout_summary: &Value,
    target_temp_c: i16,
    anchors_c: &[i16],
) -> Result<Vec<(String, Value)>, Box<dyn std::error::Error + Send + Sync>> {
    let current = thermal_candidate_profile_from_value(current_profile.clone());
    let current_point = thermal_candidate_point(&current, target_temp_c)
        .ok_or_else(|| format!("current profile missing {target_temp_c}C point"))?;
    let retuned = thermal_candidate_profile_from_value(retuned_profile.clone());
    let scout_stage = stage_for_target(scout_summary, target_temp_c)?;
    let scout_result = thermal_stage_result_from_value(&scout_stage)?;
    let scout_samples = samples_for_target(scout_summary, target_temp_c);
    let mut predicted_point = thermal_candidate_point(&retuned, target_temp_c)
        .unwrap_or_else(|| tune_thermal_candidate_point(current_point, &scout_result));
    if predicted_point == current_point {
        predicted_point = tune_thermal_candidate_point(current_point, &scout_result);
    }
    predicted_point = apply_flagship_gate_nudge(
        &current_point,
        predicted_point,
        &stability_evidence_for_stage(&scout_stage, &scout_samples, target_temp_c),
        target_temp_c,
    );

    let mut variants = vec![("current".to_string(), current_profile.clone())];
    if predicted_point != current_point {
        let mut predicted = current.clone();
        if let Some(point) = super::thermal_candidate_point_mut(&mut predicted, target_temp_c) {
            *point = predicted_point;
        } else {
            predicted.points.push(predicted_point);
        }
        thermal_rebuild_profile_from_anchor_targets(&mut predicted, anchors_c);
        variants.push((
            stability_failure_class(&scout_stage, &scout_samples, target_temp_c),
            normalize_sparse_profile_value(
                &thermal_candidate_profile_to_value(&predicted),
                anchors_c,
            )?,
        ));
    }
    Ok(variants)
}

fn apply_flagship_gate_nudge(
    current_point: &ThermalCandidatePoint,
    mut point: ThermalCandidatePoint,
    evidence: &Value,
    target_temp_c: i16,
) -> ThermalCandidatePoint {
    let failure_class = evidence
        .get("failureClass")
        .and_then(Value::as_str)
        .unwrap_or("");
    if matches!(
        failure_class,
        "missed_lower_band_before_limit" | "stable_window_broke_low" | "within_gate_low_margin"
    ) {
        let stable_band_centi_c = (ThermalFullSpeedStableTracker::STABLE_BAND_C * 100.0) as u16;
        if failure_class == "missed_lower_band_before_limit" {
            point.brake_distance_centi_c = point.brake_distance_centi_c.min(stable_band_centi_c);
        } else {
            point.brake_distance_centi_c = point
                .brake_distance_centi_c
                .saturating_sub(if target_temp_c <= 150 { 180 } else { 120 })
                .max(stable_band_centi_c);
        }
        let approach_floor_target = if target_temp_c <= 150 { 650 } else { 900 };
        let approach_power_target = if target_temp_c <= 150 { 800 } else { 1_000 };
        point.approach_floor_power_permille = point
            .approach_floor_power_permille
            .max(approach_floor_target)
            .min(1_000);
        point.approach_power_permille = point
            .approach_power_permille
            .max(approach_power_target)
            .min(1_000);
        point.approach_damping_exponent_permille = point
            .approach_damping_exponent_permille
            .saturating_sub(if target_temp_c <= 150 { 600 } else { 300 })
            .max(100);
    }
    if matches!(
        failure_class,
        "stable_window_broke_high" | "missed_upper_band_before_limit"
    ) {
        let min_brake_delta = if target_temp_c <= 150 { 80 } else { 120 };
        let max_brake_delta = if target_temp_c <= 150 { 140 } else { 180 };
        let min_brake = current_point
            .brake_distance_centi_c
            .saturating_add(min_brake_delta);
        let max_brake = current_point
            .brake_distance_centi_c
            .saturating_add(max_brake_delta);
        point.brake_distance_centi_c = point.brake_distance_centi_c.clamp(min_brake, max_brake);
        point.approach_lead_ticks = point
            .approach_lead_ticks
            .max(current_point.approach_lead_ticks.saturating_add(1))
            .min(3);
        point.approach_damping_exponent_permille = point
            .approach_damping_exponent_permille
            .max(
                current_point
                    .approach_damping_exponent_permille
                    .saturating_add(50),
            )
            .min(
                current_point
                    .approach_damping_exponent_permille
                    .saturating_add(180),
            )
            .max(180);
        if target_temp_c > 150 && point.hold_power_permille >= 900 {
            point.hold_power_permille = point.hold_power_permille.saturating_sub(180).max(780);
            point.hold_reheat_power_permille = point
                .hold_reheat_power_permille
                .saturating_sub(200)
                .max(point.hold_power_permille)
                .max(800);
            point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(2).max(10);
            point.overshoot_cutoff_centi_c =
                point.overshoot_cutoff_centi_c.saturating_sub(20).max(140);
        }
    }
    point
}

fn write_candidate_variants(
    candidates_dir: &Path,
    variants: &[(String, Value)],
    target_temp_c: i16,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(candidates_dir)?;
    let mut paths = Vec::new();
    for (name, profile) in variants {
        let path = candidates_dir.join(format!("{}.json", slug(name)));
        write_json_pretty(&path, &target_local_profile_window(profile, target_temp_c)?)?;
        paths.push(path);
    }
    Ok(paths)
}

fn choose_best_batch_run(
    batch_summary: &Value,
    target_temp_c: i16,
) -> Result<SelectedBatchRun, Box<dyn std::error::Error + Send + Sync>> {
    let runs = batch_summary
        .get("runs")
        .and_then(Value::as_array)
        .ok_or_else(|| batch_summary_error(batch_summary))?;
    let mut ranked = Vec::new();
    for run in runs {
        let score = candidate_score(run, target_temp_c);
        let candidate_profile_file = candidate_profile_file(run)?;
        ranked.push(SelectedBatchRun {
            summary: run.clone(),
            candidate_profile_file,
            score,
        });
    }
    ranked.sort_by_key(|item| item.score);
    let best = ranked
        .into_iter()
        .next()
        .ok_or_else(|| batch_summary_error(batch_summary))?;
    if best.score.0[0] >= 1.0 {
        return Err(format!(
            "all batch candidates for {target_temp_c}C are disqualified by source/runtime/sample-rate faults"
        )
        .into());
    }
    Ok(best)
}

fn batch_summary_error(batch_summary: &Value) -> String {
    batch_summary
        .get("error")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or("thermal batch summary has no runs")
        .to_string()
}

fn choose_promotable_batch_run(
    batch_summary: &Value,
    target_temp_c: i16,
) -> Result<Option<SelectedBatchRun>, Box<dyn std::error::Error + Send + Sync>> {
    let mut promotable = Vec::new();
    for run in batch_summary
        .get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !stage_reference_gate_satisfied(run, target_temp_c) {
            continue;
        }
        promotable.push(SelectedBatchRun {
            summary: run.clone(),
            candidate_profile_file: candidate_profile_file(run)?,
            score: candidate_score(run, target_temp_c),
        });
    }
    promotable.sort_by_key(|item| item.score);
    Ok(promotable.into_iter().next())
}

fn candidate_score(summary: &Value, target_temp_c: i16) -> CandidateScore {
    let stage = stage_for_target(summary, target_temp_c).unwrap_or(Value::Null);
    let metrics = stage_metrics(&stage);
    let samples = samples_for_target(summary, target_temp_c);
    let evidence = stability_evidence_for_stage(&stage, &samples, target_temp_c);
    let failure_class = evidence
        .get("failureClass")
        .and_then(Value::as_str)
        .unwrap_or("");
    let temperature_gap = evidence
        .get("temperatureGapC")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let first_band_at_ms = evidence.get("firstBandAtMs").and_then(Value::as_f64);
    let mut stability_progress_penalty = match failure_class {
        "within_gate" => 0.0,
        "within_gate_low_margin" => 0.25,
        "stable_window_broke_low" | "stable_window_broke_high" => 1.0,
        "band_entry_not_observed" => 2.0,
        "missed_lower_band_before_limit" | "missed_upper_band_before_limit" => 3.0,
        _ => 4.0,
    };
    if let Some(first_band_at_ms) = first_band_at_ms {
        stability_progress_penalty += (first_band_at_ms / 1_000_000.0).min(0.5);
    }
    stability_progress_penalty += temperature_gap.min(5.0);
    let settle_time_ms = metrics.get("settleTimeMs").and_then(Value::as_f64);
    let full_speed_limit_ms = metrics.get("fullSpeedLimitMs").and_then(Value::as_f64);
    let full_speed_margin_penalty = settle_time_ms
        .zip(full_speed_limit_ms)
        .map(|(settle, limit)| {
            (settle - (limit - if target_temp_c > 150 { 500.0 } else { 1_000.0 })).max(0.0)
        })
        .unwrap_or(1.0e12);
    let hold_median = metrics
        .get("holdMedianOutputPermille")
        .and_then(Value::as_f64);
    let hold_p90 = metrics.get("holdP90OutputPermille").and_then(Value::as_f64);
    CandidateScore([
        if run_is_disqualified(summary, target_temp_c) {
            1.0
        } else {
            0.0
        },
        if metrics.get("stopReason").and_then(Value::as_str) == Some("completed") {
            0.0
        } else {
            1.0
        },
        if failure_class == "within_gate_low_margin" {
            1.0
        } else {
            0.0
        },
        stability_progress_penalty,
        full_speed_margin_penalty,
        settle_time_ms.unwrap_or(1.0e12),
        metrics
            .get("maxOvershootC")
            .and_then(Value::as_f64)
            .unwrap_or(1.0e12),
        metrics
            .get("holdPeakToPeakC")
            .and_then(Value::as_f64)
            .unwrap_or(1.0e12),
        hold_median
            .zip(hold_p90)
            .map(|(median, p90)| (p90 - median).abs())
            .unwrap_or(1.0e12),
        metrics
            .get("approachCurveMeanAbsErrorC")
            .and_then(Value::as_f64)
            .unwrap_or(1.0e12),
    ])
}

fn stage_reference_gate_satisfied(summary: &Value, target_temp_c: i16) -> bool {
    if run_is_disqualified(summary, target_temp_c) || !warmup_output_is_full(summary, target_temp_c)
    {
        return false;
    }
    let Ok(stage) = stage_for_target(summary, target_temp_c) else {
        return false;
    };
    if stage.get("stopReason").and_then(Value::as_str) != Some("completed") {
        return false;
    }
    let samples = samples_for_target(summary, target_temp_c);
    let evidence = stability_evidence_for_stage(&stage, &samples, target_temp_c);
    evidence.get("failureClass").and_then(Value::as_str) == Some("within_gate")
}

fn batch_attempt_records(
    batch_summary: &Value,
    target_temp_c: i16,
    first_round_number: usize,
    tuning_round: u32,
    selected_run_id: &str,
    budget_elapsed_seconds_value: u64,
) -> Vec<Value> {
    let mut records = Vec::new();
    for run in batch_summary
        .get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let candidate_profile = run.get("candidateProfile").cloned().unwrap_or(Value::Null);
        let candidate_name = candidate_profile_file(run).ok().and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        });
        let run_id = run.get("runId").and_then(Value::as_str).unwrap_or_default();
        records.push(round_record_from_summary(
            run,
            target_temp_c,
            first_round_number + records.len(),
            &format!(
                "tuning {tuning_round} / {}",
                candidate_name.as_deref().unwrap_or("candidate")
            ),
            explicit_point_value(&candidate_profile, target_temp_c),
            "batch_candidate",
            Some(tuning_round),
            candidate_name.as_deref(),
            run_id == selected_run_id,
            Some(candidate_score(run, target_temp_c).to_value()),
            budget_elapsed_seconds_value,
        ));
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn round_record_from_summary(
    summary: &Value,
    target_temp_c: i16,
    round_number: usize,
    label: &str,
    point: Option<Value>,
    attempt_type: &str,
    tuning_round: Option<u32>,
    candidate_name: Option<&str>,
    selected: bool,
    score: Option<Value>,
    budget_elapsed_seconds_value: u64,
) -> Value {
    let stage = stage_for_target(summary, target_temp_c).unwrap_or(Value::Null);
    let analysis = stage.get("analysis").cloned().unwrap_or(Value::Null);
    let stable = stage
        .get("fullSpeedToStable")
        .cloned()
        .unwrap_or(Value::Null);
    let guard = stage.get("guard").cloned().unwrap_or(Value::Null);
    let samples = samples_for_target(summary, target_temp_c);
    json!({
        "round": round_number,
        "label": label,
        "attemptType": attempt_type,
        "tuningRound": tuning_round,
        "candidateName": candidate_name,
        "selected": selected,
        "evidenceValid": !run_is_disqualified(summary, target_temp_c),
        "summaryPath": summary.pointer("/files/summaryPath").and_then(Value::as_str).map(display_path_string).unwrap_or_default(),
        "point": sanitize_point(point.as_ref(), Some(target_temp_c)).unwrap_or(Value::Null),
        "samples": samples,
        "failures": target_failure_summary(summary, target_temp_c),
        "result": {
            "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
            "terminalRuntimeDropReason": stage.get("terminalRuntimeDropReason").cloned().unwrap_or(Value::Null),
            "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
            "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
            "analysis": analysis.clone(),
            "guard": guard,
            "fullSpeedToStable": stable.clone(),
            "settleTimeMs": stable.get("settleTimeMs").cloned().unwrap_or(Value::Null),
            "fullSpeedLimitMs": stable.get("limitMs").cloned().unwrap_or(Value::Null),
            "approachCurveMeanAbsErrorC": analysis.get("approachCurveMeanAbsErrorC").cloned().unwrap_or(Value::Null),
            "approachCurveDeviationClass": analysis.get("approachCurveDeviationClass").cloned().unwrap_or(Value::Null),
            "approachReferenceDurationDeltaMs": analysis.get("approachReferenceDurationDeltaMs").cloned().unwrap_or(Value::Null),
            "approachReferencePeakDeltaC": analysis.get("approachReferencePeakDeltaC").cloned().unwrap_or(Value::Null),
            "approachReferenceClass": analysis.get("approachReferenceClass").cloned().unwrap_or(Value::Null),
            "stabilityEvidence": stability_evidence_for_stage(&stage, &samples, target_temp_c),
            "warmupOutputFull": warmup_output_is_full(summary, target_temp_c),
            "budgetElapsedSeconds": budget_elapsed_seconds_value,
            "score": score,
        }
    })
}

fn review_target_entry(
    target_temp_c: i16,
    budget_outcome: &str,
    time_spent_seconds: u64,
    rounds: Vec<Value>,
    final_summary: &Value,
    accepted_profile: &Value,
    confirm_hold_seconds: u64,
) -> Value {
    let final_stage = stage_for_target(final_summary, target_temp_c).ok();
    let final_samples = if final_stage.is_some() {
        samples_for_target(final_summary, target_temp_c)
    } else {
        Vec::new()
    };
    let fallback_evidence = if final_stage.is_none() || final_samples.is_empty() {
        fallback_round_evidence(&rounds)
    } else {
        None
    };
    let stage = fallback_evidence
        .as_ref()
        .map(|(result, _)| result.clone())
        .or_else(|| final_stage.clone())
        .or_else(|| fallback_round_result(&rounds))
        .unwrap_or(Value::Null);
    let samples = if !final_samples.is_empty() {
        final_samples
    } else if let Some((_, samples)) = fallback_evidence {
        samples
    } else {
        fallback_round_samples(&rounds).unwrap_or_default()
    };
    let truth_point = effective_profile_point(accepted_profile, target_temp_c);
    let effective_point = truth_point
        .clone()
        .or_else(|| effective_point_from_samples(&samples, target_temp_c));
    let failures = target_failure_summary(final_summary, target_temp_c);
    let run_id = final_summary
        .get("runId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("target-{target_temp_c}"));
    let has_valid_evidence = rounds
        .iter()
        .any(|item| item.get("evidenceValid").and_then(Value::as_bool) != Some(false));
    let candidate_ready = effective_point.is_some()
        && has_valid_evidence
        && review_candidate_metric_gate_passes(&stage, target_temp_c);
    json!({
        "runId": run_id,
        "target": target_temp_c,
        "targetTempC": target_temp_c,
        "targetRole": "tuning",
        "ok": budget_outcome == "completed",
        "candidateReady": candidate_ready,
        "candidateDisposition": candidate_disposition_for_target(
            budget_outcome,
            candidate_ready,
        ),
        "saved": false,
        "evidence": "preliminary_review",
        "budgetOutcome": budget_outcome,
        "timeSpentSeconds": time_spent_seconds,
        "roundCount": rounds.len(),
        "validTestCount": rounds.iter().filter(|item| item.get("evidenceValid").and_then(Value::as_bool) != Some(false)).count(),
        "invalidTestCount": rounds.iter().filter(|item| item.get("evidenceValid").and_then(Value::as_bool) == Some(false)).count(),
        "approachReference": {
            "targetTempC": target_temp_c,
            "variantId": "full_speed_to_stable_gate",
            "passed": true,
            "limitMs": if target_temp_c > 150 { 5_000 } else { 10_000 },
            "failureReason": Value::Null,
        },
        "point": effective_point.clone().unwrap_or(Value::Null),
        "truthPoint": truth_point.unwrap_or(Value::Null),
        "pointSource": if effective_point.is_some() { "review_candidate_snapshot" } else { "sample_parameters" },
        "rounds": rounds,
        "result": stage.clone(),
        "failures": failures.clone(),
        "samples": samples,
        "holdCheck": hold_check(final_summary, target_temp_c, budget_outcome, confirm_hold_seconds, &stage, &failures),
    })
}

fn validation_target_entry(
    target_temp_c: i16,
    budget_outcome: &str,
    time_spent_seconds: u64,
    rounds: Vec<Value>,
    final_summary: &Value,
    accepted_profile: &Value,
    confirm_hold_seconds: u64,
) -> Value {
    let review_outcome = if budget_outcome == "validation_passed" {
        "completed"
    } else {
        "not_converged"
    };
    let mut entry = review_target_entry(
        target_temp_c,
        review_outcome,
        time_spent_seconds,
        rounds,
        final_summary,
        accepted_profile,
        confirm_hold_seconds,
    );
    entry["targetRole"] = json!("validation");
    entry["ok"] = json!(budget_outcome == "validation_passed");
    entry["candidateReady"] = json!(false);
    entry["candidateDisposition"] = json!(validation_disposition_for_target(budget_outcome));
    entry["budgetOutcome"] = json!(budget_outcome);
    entry["pointSource"] = json!("validation_final_profile");
    entry
}

fn candidate_disposition_for_target(budget_outcome: &str, candidate_ready: bool) -> &'static str {
    if budget_outcome == "completed" && candidate_ready {
        "acceptance_passed"
    } else if candidate_ready {
        "candidate_ready"
    } else if budget_outcome == "environment_blocked" {
        "environment_blocked"
    } else if budget_outcome == "budget_exhausted" {
        "budget_exhausted_without_candidate"
    } else {
        "not_available"
    }
}

fn validation_disposition_for_target(budget_outcome: &str) -> &'static str {
    match budget_outcome {
        "validation_passed" => "validation_passed",
        "validation_failed" => "validation_failed",
        "environment_blocked" => "environment_blocked",
        "budget_exhausted" => "validation_budget_exhausted",
        _ => "validation_not_available",
    }
}

fn fallback_round_result(rounds: &[Value]) -> Option<Value> {
    fallback_round_evidence(rounds)
        .map(|(result, _)| result)
        .or_else(|| rounds.iter().rev().find_map(reportable_round_result))
}

fn reportable_round_result(round: &Value) -> Option<Value> {
    if round.get("evidenceValid").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let result = round.get("result")?.clone();
    if result_is_empty(&result) {
        return None;
    }
    Some(result)
}

fn result_is_empty(result: &Value) -> bool {
    result.is_null()
        || (result.get("stopReason").is_none()
            && result.get("analysis").is_none()
            && result.get("fullSpeedToStable").is_none())
}

fn metric_value(result: &Value, paths: &[&str]) -> Option<f64> {
    paths
        .iter()
        .find_map(|path| result.pointer(path).and_then(Value::as_f64))
}

fn review_candidate_metric_gate_passes(result: &Value, target_temp_c: i16) -> bool {
    if result.get("stopReason").and_then(Value::as_str) != Some("completed") {
        return false;
    }
    let Some(overshoot_c) = metric_value(result, &["/maxOvershootC"]) else {
        return false;
    };
    if overshoot_c > 3.0 {
        return false;
    }
    let Some(hold_p2p_c) = metric_value(result, &["/holdPeakToPeakC"]) else {
        return false;
    };
    if hold_p2p_c > 3.0 {
        return false;
    }
    if result
        .pointer("/fullSpeedToStable/failureReason")
        .or_else(|| result.get("failureReason"))
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty())
    {
        return false;
    }
    let Some(settle_time_ms) = metric_value(
        result,
        &["/fullSpeedToStable/settleTimeMs", "/settleTimeMs"],
    ) else {
        return false;
    };
    let limit_ms = metric_value(result, &["/fullSpeedToStable/limitMs", "/fullSpeedLimitMs"])
        .unwrap_or(if target_temp_c > 150 {
            5_000.0
        } else {
            10_000.0
        });
    settle_time_ms <= limit_ms
}

fn fallback_round_evidence(rounds: &[Value]) -> Option<(Value, Vec<Value>)> {
    rounds
        .iter()
        .rev()
        .filter(|round| round.get("selected").and_then(Value::as_bool) == Some(true))
        .find_map(reportable_round_evidence)
        .or_else(|| rounds.iter().rev().find_map(reportable_round_evidence))
}

fn reportable_round_evidence(round: &Value) -> Option<(Value, Vec<Value>)> {
    let result = reportable_round_result(round)?;
    let samples = round
        .get("samples")
        .and_then(Value::as_array)
        .filter(|samples| !samples.is_empty())
        .cloned()?;
    Some((result, samples))
}

fn fallback_round_samples(rounds: &[Value]) -> Option<Vec<Value>> {
    fallback_round_evidence(rounds)
        .map(|(_, samples)| samples)
        .or_else(|| {
            rounds.iter().rev().find_map(|round| {
                if round.get("evidenceValid").and_then(Value::as_bool) == Some(false) {
                    return None;
                }
                round
                    .get("samples")
                    .and_then(Value::as_array)
                    .filter(|samples| !samples.is_empty())
                    .cloned()
            })
        })
}

fn hold_check(
    summary: &Value,
    target_temp_c: i16,
    budget_outcome: &str,
    confirm_hold_seconds: u64,
    stage: &Value,
    failures: &[Value],
) -> Value {
    let analysis = stage.get("analysis").cloned().unwrap_or(Value::Null);
    let guard = stage.get("guard").cloned().unwrap_or(Value::Null);
    let is_hold_confirm = summary
        .pointer("/parameters/evaluationMode")
        .and_then(Value::as_str)
        == Some("hold-confirm");
    let failure_reason = failures
        .first()
        .and_then(|failure| {
            failure
                .get("reason")
                .or_else(|| failure.get("failureReason"))
                .cloned()
        })
        .unwrap_or_else(|| {
            if is_hold_confirm {
                Value::Null
            } else {
                json!(budget_outcome)
            }
        });
    json!({
        "confirmRunId": summary.get("runId").cloned().unwrap_or(Value::Null),
        "passed": is_hold_confirm && summary.pointer("/validation/passed").and_then(Value::as_bool).unwrap_or(false),
        "failureReason": failure_reason,
        "holdSeconds": confirm_hold_seconds,
        "maxOvershootC": stage.get("maxOvershootC").cloned().unwrap_or(Value::Null),
        "holdPeakToPeakC": stage.get("holdPeakToPeakC").cloned().unwrap_or(Value::Null),
        "firstHoldAtMs": guard.get("firstHoldAtMs").cloned().unwrap_or(Value::Null),
        "holdMedianOutputPermille": analysis.get("holdMedianOutputPermille").cloned().unwrap_or(Value::Null),
        "holdP90OutputPermille": analysis.get("holdP90OutputPermille").cloned().unwrap_or(Value::Null),
        "approachSource": analysis.get("approachSource").cloned().unwrap_or(Value::Null),
        "holdSource": analysis.get("holdSource").cloned().unwrap_or(Value::Null),
        "sourceRunPath": summary.pointer("/files/summaryPath").cloned().unwrap_or(Value::Null),
        "stopReason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
        "targetTempC": target_temp_c,
    })
}

fn reseed_after_failed_hold_confirm(
    profile: &Value,
    target_temp_c: i16,
    confirm_summary: &Value,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let mut candidate = thermal_candidate_profile_from_value(profile.clone());
    let Some(current_point) = thermal_candidate_point(&candidate, target_temp_c) else {
        return Ok(None);
    };
    let stage = stage_for_target(confirm_summary, target_temp_c)?;
    let stage_result = thermal_stage_result_from_value(&stage)?;
    let samples = samples_for_target(confirm_summary, target_temp_c);
    let predicted = apply_hold_confirm_reseed_nudge(
        tune_thermal_candidate_point(current_point, &stage_result),
        &stage,
        &samples,
        target_temp_c,
    );
    if predicted == current_point {
        return Ok(None);
    }
    if let Some(point) = super::thermal_candidate_point_mut(&mut candidate, target_temp_c) {
        *point = predicted;
    }
    Ok(Some(thermal_candidate_profile_to_value(&candidate)))
}

fn apply_hold_confirm_reseed_nudge(
    mut point: ThermalCandidatePoint,
    stage: &Value,
    samples: &[Value],
    target_temp_c: i16,
) -> ThermalCandidatePoint {
    let evidence = stability_evidence_for_stage(stage, samples, target_temp_c);
    let failure_class = evidence
        .get("failureClass")
        .and_then(Value::as_str)
        .unwrap_or("");
    let analysis = stage.get("analysis").unwrap_or(&Value::Null);
    let hold_median = analysis
        .get("holdMedianOutputPermille")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u16;
    let hold_p90 = analysis
        .get("holdP90OutputPermille")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u16;
    if target_temp_c > 150
        && matches!(
            failure_class,
            "stable_window_broke_high" | "within_gate_low_margin"
        )
        && hold_median >= 900
        && hold_p90 >= 950
    {
        point.hold_power_permille = point.hold_power_permille.saturating_sub(220).max(700);
        point.hold_reheat_power_permille = point
            .hold_reheat_power_permille
            .saturating_sub(260)
            .max(point.hold_power_permille)
            .max(720);
        point.hold_kp_permille_per_c = point.hold_kp_permille_per_c.saturating_sub(4).max(8);
        point.overshoot_cutoff_centi_c = point.overshoot_cutoff_centi_c.saturating_sub(40).max(120);
        point.hold_entry_centi_c = point.hold_entry_centi_c.saturating_sub(20).max(120);
        point.hold_exit_centi_c = point.hold_exit_centi_c.saturating_sub(20).max(120);
    }
    point
}

fn stage_for_target(
    summary: &Value,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    summary
        .get("applied")
        .and_then(Value::as_array)
        .and_then(|stages| {
            stages.iter().find(|stage| {
                stage.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target_temp_c))
            })
        })
        .cloned()
        .ok_or_else(|| format!("missing applied stage for {target_temp_c}C").into())
}

fn stage_metrics(stage: &Value) -> serde_json::Map<String, Value> {
    let analysis = stage.get("analysis").unwrap_or(&Value::Null);
    let stable = stage.get("fullSpeedToStable").unwrap_or(&Value::Null);
    let mut metrics = serde_json::Map::new();
    for (key, value) in [
        ("stopReason", stage.get("stopReason")),
        ("maxOvershootC", stage.get("maxOvershootC")),
        ("holdPeakToPeakC", stage.get("holdPeakToPeakC")),
        (
            "approachCurveMeanAbsErrorC",
            analysis.get("approachCurveMeanAbsErrorC"),
        ),
        (
            "approachReferenceDurationDeltaMs",
            analysis.get("approachReferenceDurationDeltaMs"),
        ),
        (
            "approachReferencePeakDeltaC",
            analysis.get("approachReferencePeakDeltaC"),
        ),
        (
            "approachReferenceClass",
            analysis.get("approachReferenceClass"),
        ),
        ("settleTimeMs", stable.get("settleTimeMs")),
        ("fullSpeedLimitMs", stable.get("limitMs")),
        ("failureReason", stable.get("failureReason")),
        (
            "holdMedianOutputPermille",
            analysis.get("holdMedianOutputPermille"),
        ),
        (
            "holdP90OutputPermille",
            analysis.get("holdP90OutputPermille"),
        ),
    ] {
        metrics.insert(key.to_string(), value.cloned().unwrap_or(Value::Null));
    }
    metrics
}

fn stability_evidence_for_stage(stage: &Value, samples: &[Value], target_temp_c: i16) -> Value {
    let stable = stage.get("fullSpeedToStable").unwrap_or(&Value::Null);
    let mut evidence = classify_stability_evidence(
        samples,
        target_temp_c,
        stable.get("warmupExitedAtMs").and_then(Value::as_f64),
        stable.get("limitMs").and_then(Value::as_f64),
    );
    let settle_ms = stable.get("settleTimeMs").and_then(Value::as_f64);
    let limit_ms = stable.get("limitMs").and_then(Value::as_f64);
    let required_margin_ms = if target_temp_c > 150 { 500.0 } else { 1_000.0 };
    if stage.get("stopReason").and_then(Value::as_str) == Some("completed")
        && let Some(settle_ms) = settle_ms
    {
        evidence["failureClass"] = json!("within_gate");
        if let Some(limit_ms) = limit_ms
            && limit_ms - settle_ms < required_margin_ms
        {
            evidence["failureClass"] = json!("within_gate_low_margin");
            evidence["timeMarginMs"] = json!(limit_ms - settle_ms);
            evidence["requiredTimeMarginMs"] = json!(required_margin_ms);
        }
    }
    evidence
}

fn classify_stability_evidence(
    samples: &[Value],
    target_temp_c: i16,
    warmup_exited_at_ms: Option<f64>,
    limit_ms: Option<f64>,
) -> Value {
    let Some(exit_ms) = warmup_exited_at_ms else {
        return json!({"failureClass": "insufficient_evidence"});
    };
    let Some(limit_ms) = limit_ms else {
        return json!({"failureClass": "insufficient_evidence"});
    };
    let lower = f64::from(target_temp_c) - ThermalFullSpeedStableTracker::STABLE_BAND_C;
    let upper = f64::from(target_temp_c) + ThermalFullSpeedStableTracker::STABLE_BAND_C;
    let deadline_ms = exit_ms + limit_ms;
    let mut observed = Vec::<(f64, f64)>::new();
    for sample in samples {
        let elapsed = sample.get("t").and_then(Value::as_f64);
        let temp = sample.get("temp").and_then(Value::as_f64);
        if let Some((elapsed, temp)) = elapsed.zip(temp) {
            let elapsed_ms = elapsed * 1_000.0;
            if elapsed_ms >= exit_ms {
                observed.push((elapsed_ms, temp));
            }
        }
    }
    if observed.is_empty() {
        return json!({"failureClass": "insufficient_evidence"});
    }
    let deadline_sample = observed
        .iter()
        .min_by(|left, right| {
            (left.0 - deadline_ms)
                .abs()
                .partial_cmp(&(right.0 - deadline_ms).abs())
                .unwrap_or(Ordering::Equal)
        })
        .copied()
        .unwrap_or((deadline_ms, f64::from(target_temp_c)));
    let first_band_index = observed.iter().position(|(elapsed_ms, temp)| {
        *elapsed_ms <= deadline_ms && *temp >= lower && *temp <= upper
    });
    let mut evidence = json!({
        "failureClass": "within_gate",
        "lowerBandC": lower,
        "upperBandC": upper,
        "deadlineAtMs": deadline_ms.round() as i64,
        "deadlineTempC": deadline_sample.1,
        "firstBandAtMs": Value::Null,
        "bandExitTempC": Value::Null,
        "temperatureGapC": 0.0,
    });
    let Some(index) = first_band_index else {
        if deadline_sample.1 < lower {
            evidence["failureClass"] = json!("missed_lower_band_before_limit");
            evidence["temperatureGapC"] = json!(lower - deadline_sample.1);
        } else if deadline_sample.1 > upper {
            evidence["failureClass"] = json!("missed_upper_band_before_limit");
            evidence["temperatureGapC"] = json!(deadline_sample.1 - upper);
        } else {
            evidence["failureClass"] = json!("band_entry_not_observed");
        }
        return evidence;
    };
    evidence["firstBandAtMs"] = json!(observed[index].0.round() as i64);
    for (_, temp) in observed.iter().skip(index + 1) {
        if *temp > upper {
            evidence["failureClass"] = json!("stable_window_broke_high");
            evidence["bandExitTempC"] = json!(temp);
            evidence["temperatureGapC"] = json!(temp - upper);
            break;
        }
        if *temp < lower {
            evidence["failureClass"] = json!("stable_window_broke_low");
            evidence["bandExitTempC"] = json!(temp);
            evidence["temperatureGapC"] = json!(lower - temp);
            break;
        }
    }
    evidence
}

fn stability_failure_class(stage: &Value, samples: &[Value], target_temp_c: i16) -> String {
    stability_evidence_for_stage(stage, samples, target_temp_c)
        .get("failureClass")
        .and_then(Value::as_str)
        .unwrap_or("predicted")
        .to_string()
}

fn run_is_disqualified(summary: &Value, target_temp_c: i16) -> bool {
    let Ok(stage) = stage_for_target(summary, target_temp_c) else {
        return true;
    };
    let stop_reason = stage
        .get("stopReason")
        .and_then(Value::as_str)
        .unwrap_or("");
    if matches!(
        stop_reason,
        "heater_disarmed"
            | "target_mismatch"
            | "profile_target_mismatch"
            | "runtime_reset"
            | "sample_rate_below_minimum"
            | "sample_rate_below_3hz"
            | "source_telemetry_stale"
            | "source_fault"
            | "status_request_failed"
            | "temperature_sample_glitch"
    ) {
        return true;
    }
    if stage
        .get("terminalRuntimeDropReason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty())
    {
        return true;
    }
    validation_failures_for_target(summary, target_temp_c)
        .iter()
        .any(|failure| {
            let reason = failure
                .get("reason")
                .or_else(|| failure.get("failureReason"))
                .or_else(|| failure.get("stopReason"))
                .and_then(Value::as_str)
                .unwrap_or("");
            reason.contains("source")
                || reason.contains("sample_rate")
                || reason.contains("target_mismatch")
                || reason.contains("heater_disarmed")
                || reason.contains("temperature_sample_glitch")
        })
}

fn flagship_retryable_environment_error_message(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("isolapurr usb-c telemetry did not advance")
        || lowered.contains("source telemetry refresh failed before stage")
        || lowered.contains("isolapurr power show timed out")
        || lowered.contains("source recovery failed")
        || lowered.contains("source_fault")
        || lowered.contains("source_telemetry_stale")
        || lowered.contains("status_request_failed")
        || lowered.contains("temperature_sample_glitch")
        || lowered.contains("runtime_reset")
        || lowered.contains("sample_rate_below")
        || lowered.contains("usb_response_timeout")
        || lowered.contains("connection reset by peer")
        || lowered.contains("tcp connect error")
        || lowered.contains("deadline has elapsed")
        || lowered.contains("connect refused")
        || lowered.contains("disqualified by source/runtime/sample-rate faults")
}

fn flagship_retryable_environment_failure_reason(reason: &str) -> bool {
    matches!(
        reason,
        "source_telemetry_stale"
            | "source_fault"
            | "status_request_failed"
            | "temperature_sample_glitch"
            | "runtime_reset"
            | "sample_rate_below_minimum"
            | "sample_rate_below_3hz"
    ) || flagship_retryable_environment_error_message(reason)
}

fn flagship_retryable_environment_summary(summary: &Value, target_temp_c: i16) -> bool {
    if summary
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(flagship_retryable_environment_error_message)
    {
        return true;
    }
    if validation_failures_for_target(summary, target_temp_c)
        .iter()
        .filter_map(|failure| {
            failure
                .get("reason")
                .or_else(|| failure.get("failureReason"))
                .or_else(|| failure.get("stopReason"))
                .and_then(Value::as_str)
        })
        .any(flagship_retryable_environment_failure_reason)
    {
        return true;
    }
    let Ok(stage) = stage_for_target(summary, target_temp_c) else {
        return false;
    };
    stage
        .get("stopReason")
        .and_then(Value::as_str)
        .is_some_and(flagship_retryable_environment_failure_reason)
        || stage
            .get("terminalRuntimeDropReason")
            .and_then(Value::as_str)
            .is_some_and(flagship_retryable_environment_failure_reason)
}

fn flagship_retryable_environment_batch_summary(batch_summary: &Value, target_temp_c: i16) -> bool {
    if batch_summary
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(flagship_retryable_environment_error_message)
    {
        return true;
    }
    let Some(runs) = batch_summary.get("runs").and_then(Value::as_array) else {
        return false;
    };
    !runs.is_empty()
        && runs
            .iter()
            .all(|run| flagship_retryable_environment_summary(run, target_temp_c))
}

fn first_batch_run_summary(batch_summary: &Value) -> Option<Value> {
    batch_summary
        .get("runs")
        .and_then(Value::as_array)
        .and_then(|runs| runs.first())
        .cloned()
}

fn samples_for_target(summary: &Value, target_temp_c: i16) -> Vec<Value> {
    let Some(samples_path) = summary
        .pointer("/files/samplesPath")
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(samples_path) else {
        return Vec::new();
    };
    let mut samples = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|sample| {
            sample.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target_temp_c))
        })
        .map(normalized_sample)
        .collect::<Vec<_>>();
    sort_samples_by_time(&mut samples);
    samples
}

fn normalized_sample(sample: Value) -> Value {
    let status = sample.get("status").unwrap_or(&Value::Null);
    let heater = sample.get("heaterTelemetry").unwrap_or(&Value::Null);
    let source = sample.get("sourceTelemetry").unwrap_or(&Value::Null);
    let request_mv = status
        .get("pdRequestMv")
        .or_else(|| heater.get("ppsRequestMv"))
        .or_else(|| status.get("voltageMv"))
        .or_else(|| heater.get("hotplateVoltageMv"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    json!({
        "t": round_decimal(sample.get("elapsedMs").and_then(Value::as_f64).unwrap_or(0.0) / 1_000.0, 3),
        "temp": status.get("currentTempC").or_else(|| heater.get("currentTempC")).cloned().unwrap_or(Value::Null),
        "filtered": status.get("heaterFilteredTempC").or_else(|| heater.get("heaterFilteredTempC")).cloned().unwrap_or(Value::Null),
        "command": status.get("heaterOutputPercent").or_else(|| heater.get("heaterOutputPercent")).cloned().unwrap_or(Value::Null),
        "output": status.get("heaterPhysicalOutputPercent").or_else(|| heater.get("heaterPhysicalOutputPercent")).cloned().unwrap_or(Value::Null),
        "requestV": round_decimal(request_mv / 1_000.0, 3),
        "phase": sample.get("phase").cloned().unwrap_or(Value::Null),
        "sourceVoltageV": source.get("voltageMv").and_then(Value::as_f64).map(|value| round_decimal(value / 1_000.0, 3)),
        "sourceCurrentA": source.get("currentMa").and_then(Value::as_f64).map(|value| round_decimal(value / 1_000.0, 3)),
        "sourcePowerW": source.get("powerMw").and_then(Value::as_f64).map(|value| round_decimal(value / 1_000.0, 3)),
        "parameters": sample.get("heaterParameters").cloned().unwrap_or(Value::Null),
    })
}

fn sort_samples_by_time(samples: &mut [Value]) {
    samples.sort_by(|left, right| {
        let left_t = left.get("t").and_then(Value::as_f64).unwrap_or(f64::MAX);
        let right_t = right.get("t").and_then(Value::as_f64).unwrap_or(f64::MAX);
        left_t.partial_cmp(&right_t).unwrap_or(Ordering::Equal)
    });
}

fn warmup_output_is_full(summary: &Value, target_temp_c: i16) -> bool {
    let outputs = samples_for_target(summary, target_temp_c)
        .into_iter()
        .filter(|sample| sample.get("phase").and_then(Value::as_str) == Some("warmup"))
        .filter_map(|sample| sample.get("output").and_then(Value::as_f64))
        .collect::<Vec<_>>();
    let Some(first_full_index) = outputs.iter().position(|output| *output >= 99.5) else {
        return false;
    };
    outputs
        .iter()
        .skip(first_full_index)
        .all(|output| *output >= 99.5)
}

fn scout_current_is_promotable(summary: &Value, target_temp_c: i16) -> bool {
    !run_is_disqualified(summary, target_temp_c)
        && warmup_output_is_full(summary, target_temp_c)
        && stage_reference_gate_satisfied(summary, target_temp_c)
}

fn summary_is_pre_sample_cooldown_timeout(summary: &Value) -> bool {
    let sample_count = summary.get("sampleCount").and_then(Value::as_u64);
    let applied_empty = summary
        .get("applied")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    let cooldown_error = summary
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| {
            error.contains("requires cooldown") || error.contains("cooldown precondition")
        });
    sample_count == Some(0) && applied_empty && cooldown_error
}

fn validation_failures_for_target(summary: &Value, target_temp_c: i16) -> Vec<Value> {
    summary
        .pointer("/validation/failures")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|failure| {
            failure.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target_temp_c))
        })
        .cloned()
        .collect()
}

fn target_failure_summary(summary: &Value, target_temp_c: i16) -> Vec<Value> {
    let failures = validation_failures_for_target(summary, target_temp_c);
    if !failures.is_empty() {
        if failures_are_only_missing_stage(&failures)
            && let Some(error_failure) = summary_error_failure(summary, target_temp_c)
        {
            return vec![error_failure];
        }
        return failures;
    }
    if let Ok(stage) = stage_for_target(summary, target_temp_c)
        && stage.get("stopReason").and_then(Value::as_str) != Some("completed")
    {
        return vec![json!({
            "targetTempC": target_temp_c,
            "reason": stage.get("stopReason").cloned().unwrap_or(Value::Null),
        })];
    }
    if let Some(error_failure) = summary_error_failure(summary, target_temp_c) {
        return vec![error_failure];
    }
    Vec::new()
}

fn failures_are_only_missing_stage(failures: &[Value]) -> bool {
    !failures.is_empty()
        && failures
            .iter()
            .all(|failure| failure.get("reason").and_then(Value::as_str) == Some("missing_stage"))
}

fn summary_error_failure(summary: &Value, target_temp_c: i16) -> Option<Value> {
    let error = summary.get("error").and_then(Value::as_str)?.trim();
    if error.is_empty() {
        return None;
    }
    Some(json!({
        "targetTempC": target_temp_c,
        "phase": "summary",
        "reason": "execution_error",
        "message": error,
    }))
}

fn explicit_point_value(profile: &Value, target_temp_c: i16) -> Option<Value> {
    profile
        .get("points")
        .and_then(Value::as_array)?
        .iter()
        .find(|point| {
            point.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target_temp_c))
        })
        .cloned()
}

fn effective_profile_point(profile: &Value, target_temp_c: i16) -> Option<Value> {
    let explicit = explicit_point_value(profile, target_temp_c);
    sanitize_point(explicit.as_ref(), Some(target_temp_c)).or_else(|| {
        let interpolated = thermal_heater_parameters_value(target_temp_c, Some(profile), "preview");
        sanitize_point(Some(&interpolated), Some(target_temp_c))
    })
}

fn effective_point_from_samples(samples: &[Value], target_temp_c: i16) -> Option<Value> {
    samples
        .iter()
        .filter_map(|sample| sample.get("parameters"))
        .find_map(|point| sanitize_point(Some(point), Some(target_temp_c)))
}

fn sanitize_point(point: Option<&Value>, target_temp_c: Option<i16>) -> Option<Value> {
    let point = point?.as_object()?;
    let mut sanitized = serde_json::Map::new();
    for field in [
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
    ] {
        if let Some(value) = point.get(field) {
            sanitized.insert(field.to_string(), value.clone());
        }
    }
    if let Some(target_temp_c) = target_temp_c {
        sanitized.insert("targetTempC".to_string(), json!(target_temp_c));
    }
    (!sanitized.is_empty()).then_some(Value::Object(sanitized))
}

fn candidate_profile_file(
    run: &Value,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    run.pointer("/parameters/candidateProfileFile")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "batch run missing parameters.candidateProfileFile".into())
}

fn summary_run_dir(summary: &Value) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    summary
        .pointer("/files/runDir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "thermal summary missing files.runDir".into())
}

fn synthetic_failure_summary(target_temp_c: i16, reason: &str) -> Value {
    json!({
        "kind": "thermal_self_test",
        "runId": format!("synthetic-{target_temp_c}-{reason}"),
        "source": {
            "selectedMode": "100w",
            "resolvedBank": "pps5a",
            "detectedSourceClass": "pps5a",
        },
        "parameters": {
            "targetsC": [target_temp_c],
            "evaluationMode": "tuning-scout",
        },
        "files": {
            "summaryPath": format!("thermal-self-test-runs/synthetic-{target_temp_c}.json"),
            "samplesPath": format!("thermal-self-test-runs/synthetic-{target_temp_c}.ndjson"),
        },
        "applied": [{
            "targetTempC": target_temp_c,
            "stopReason": reason,
            "maxOvershootC": Value::Null,
            "holdPeakToPeakC": Value::Null,
            "analysis": {},
            "fullSpeedToStable": {},
            "guard": {},
        }],
        "validation": {
            "passed": false,
            "expectedTargetsC": [target_temp_c],
            "failures": [{"targetTempC": target_temp_c, "reason": reason}],
        },
        "tuningSteps": [],
    })
}

fn ensure_expected_source(
    summary: &Value,
    profile_mode: ThermalProfileMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let source = summary
        .get("source")
        .and_then(Value::as_object)
        .ok_or("thermal summary missing source")?;
    let expected_mode = profile_mode.as_str();
    let expected_bank = flagship_resolved_bank(profile_mode);
    let expected_class = flagship_detected_source_class(profile_mode);
    for (key, expected) in [
        ("selectedMode", expected_mode),
        ("resolvedBank", expected_bank),
        ("detectedSourceClass", expected_class),
    ] {
        if source.get(key).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "{key} mismatch: expected {expected}, got {}",
                source.get(key).cloned().unwrap_or(Value::Null)
            )
            .into());
        }
    }
    Ok(())
}

fn ensure_batch_source(
    batch_summary: &Value,
    profile_mode: ThermalProfileMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for run in batch_summary
        .get("runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        ensure_expected_source(run, profile_mode)?;
    }
    Ok(())
}

async fn resolve_device_port_path(client: &Client, devd: &str, device_id: &str) -> Option<String> {
    request_json(client, Method::GET, devd, "/api/v1/devices", None)
        .await
        .ok()?
        .get("devices")?
        .as_array()?
        .iter()
        .find(|device| device.get("id").and_then(Value::as_str) == Some(device_id))
        .and_then(|device| {
            device
                .get("portPath")
                .or_else(|| device.get("port"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn flagship_target_selector(args: &ThermalFlagshipTuneArgs) -> TargetSelector {
    if args.dry_run && args.target.device.is_none() && args.target.hardware.is_none() {
        TargetSelector {
            device: Some("mock-fp-lab-01".to_string()),
            hardware: None,
        }
    } else {
        args.target.clone()
    }
}

fn validate_flagship_scope(
    anchors_c: &[i16],
    validation_targets_c: &[i16],
    tune_targets_c: &[i16],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if anchors_c.is_empty() || validation_targets_c.is_empty() || tune_targets_c.is_empty() {
        return Err("flagship target lists must be non-empty".into());
    }
    for target in tune_targets_c {
        if !anchors_c.contains(target) {
            return Err(format!("tune target {target}C must be present in anchor targets").into());
        }
    }
    Ok(())
}

fn effective_output_root(output_root: &Path) -> PathBuf {
    if output_root == Path::new(DEFAULT_OUTPUT_ROOT) {
        output_root.join(format!("flagship-pps5a-sprint-{}", current_unix_millis()))
    } else {
        output_root.to_path_buf()
    }
}

fn cooldown_threshold(target_temp_c: i16) -> f64 {
    if target_temp_c < 80 {
        35.0
    } else {
        f64::from(target_temp_c - 40)
    }
}

fn budget_elapsed_seconds(started_at: Instant) -> u64 {
    started_at.elapsed().as_secs()
}

fn budget_remaining_seconds(started_at: Instant, budget_seconds: u64) -> u64 {
    budget_seconds.saturating_sub(budget_elapsed_seconds(started_at))
}

fn budget_exhausted(started_at: Instant, budget_seconds: u64) -> bool {
    budget_remaining_seconds(started_at, budget_seconds) == 0
}

fn step_timeouts_for_budget(remaining_seconds: u64) -> Option<(u64, u64, u64)> {
    if remaining_seconds <= SCOUT_STAGE_TIMEOUT_SECONDS {
        return None;
    }
    Some((
        remaining_seconds
            .saturating_sub(SCOUT_STAGE_TIMEOUT_SECONDS)
            .max(1),
        SCOUT_STAGE_TIMEOUT_SECONDS,
        SCOUT_WARMUP_TIMEOUT_SECONDS,
    ))
}

fn effective_round_limit(args: &ThermalFlagshipTuneArgs) -> Option<u32> {
    args.max_tuning_rounds.or(args.dry_run.then_some(1))
}

fn flagship_resolved_bank(profile_mode: ThermalProfileMode) -> &'static str {
    match profile_mode {
        ThermalProfileMode::W65 => "pps3a",
        ThermalProfileMode::W100 | ThermalProfileMode::Auto => "pps5a",
    }
}

fn flagship_detected_source_class(profile_mode: ThermalProfileMode) -> &'static str {
    flagship_resolved_bank(profile_mode)
}

fn source_preset(profile_mode: ThermalProfileMode) -> &'static str {
    match profile_mode {
        ThermalProfileMode::W65 => SOURCE_PRESET_65W,
        ThermalProfileMode::W100 | ThermalProfileMode::Auto => SOURCE_PRESET_100W,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_test_samples(samples: &[Value]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("thermal-flagship-test-{unique}.ndjson"));
        let body = samples
            .iter()
            .map(|sample| serde_json::to_string(sample).expect("serialize sample"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{body}\n")).expect("write samples");
        path
    }

    fn write_test_json(value: &Value, suffix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("thermal-flagship-test-{unique}-{suffix}.json"));
        fs::write(
            &path,
            serde_json::to_vec_pretty(value).expect("serialize json"),
        )
        .expect("write json");
        path
    }

    fn scout_summary_with_samples(
        samples_path: &Path,
        settle_time_ms: Option<i64>,
        stop_reason: &str,
    ) -> Value {
        json!({
            "files": {
                "samplesPath": samples_path,
            },
            "validation": {
                "failures": [],
            },
            "applied": [{
                "targetTempC": 220,
                "stopReason": stop_reason,
                "terminalRuntimeDropReason": null,
                "analysis": {},
                "fullSpeedToStable": {
                    "limitMs": 5000,
                    "settleTimeMs": settle_time_ms,
                    "warmupExitedAtMs": 1000,
                },
            }],
        })
    }

    fn sample(elapsed_ms: i64, phase: &str, temp_c: f64, output_percent: i64) -> Value {
        json!({
            "targetTempC": 220,
            "elapsedMs": elapsed_ms,
            "phase": phase,
            "status": {
                "currentTempC": temp_c,
                "heaterFilteredTempC": temp_c,
                "heaterOutputPercent": output_percent,
                "heaterPhysicalOutputPercent": output_percent,
                "pdRequestMv": 21000,
            },
            "sourceTelemetry": {
                "voltageMv": 21000,
                "currentMa": 3000,
                "powerMw": 63000,
            },
        })
    }

    #[test]
    fn promotable_scout_current_allows_direct_hold_confirm_fast_path() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 170.0, 100),
            sample(600, "warmup", 178.0, 100),
            sample(1100, "approach", 218.7, 100),
            sample(2000, "hold", 219.6, 98),
            sample(3000, "hold", 220.4, 98),
            sample(4000, "hold", 220.8, 97),
        ]);
        let summary = scout_summary_with_samples(&samples_path, Some(3_200), "completed");

        assert!(scout_current_is_promotable(&summary, 220));

        let _ = fs::remove_file(samples_path);
    }

    #[test]
    fn promotable_scout_current_rejects_low_margin_completion() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 170.0, 100),
            sample(600, "warmup", 178.0, 100),
            sample(1100, "approach", 218.7, 100),
            sample(2000, "hold", 219.6, 98),
            sample(3000, "hold", 220.4, 98),
            sample(4000, "hold", 220.8, 97),
        ]);
        let summary = scout_summary_with_samples(&samples_path, Some(4_700), "completed");

        assert!(!scout_current_is_promotable(&summary, 220));

        let _ = fs::remove_file(samples_path);
    }

    #[test]
    fn pre_sample_cooldown_timeout_is_target_budget_exhaustion() {
        let summary = json!({
            "applied": [],
            "error": "thermal self-test requires cooldown to <= 35.0C, got 36.7C",
            "parameters": {
                "cooldownTempC": 35.0,
                "cooldownTimeoutSeconds": 118,
                "targetsC": [60],
            },
            "sampleCount": 0,
            "validation": {
                "expectedTargetsC": [60],
                "failures": [{
                    "phase": "applied",
                    "reason": "missing_stage",
                    "targetTempC": 60,
                }],
                "passed": false,
            },
        });

        assert!(summary_is_pre_sample_cooldown_timeout(&summary));
    }

    #[test]
    fn sampled_thermal_failure_is_not_cooldown_budget_exhaustion() {
        let summary = json!({
            "applied": [{
                "sampleCount": 56,
                "stopReason": "full_speed_to_stable_timeout",
                "targetTempC": 60,
            }],
            "sampleCount": 56,
            "validation": {
                "expectedTargetsC": [60],
                "failures": [{
                    "reason": "incomplete_stage",
                    "targetTempC": 60,
                }],
                "passed": false,
            },
        });

        assert!(!summary_is_pre_sample_cooldown_timeout(&summary));
    }

    #[test]
    fn retryable_environment_summary_detects_source_telemetry_stale() {
        let summary = json!({
            "error": "isolapurr USB-C telemetry did not advance for 2235ms",
            "validation": {
                "failures": [{
                    "phase": "applied",
                    "reason": "missing_stage",
                    "targetTempC": 140,
                }],
            },
            "applied": [],
        });

        assert!(flagship_retryable_environment_summary(&summary, 140));
    }

    #[test]
    fn retryable_environment_summary_detects_temperature_sample_glitch() {
        let summary = json!({
            "validation": {
                "failures": [{
                    "phase": "applied",
                    "reason": "incomplete_stage",
                    "stopReason": "temperature_sample_glitch",
                    "targetTempC": 140,
                }],
            },
            "applied": [{
                "targetTempC": 140,
                "stopReason": "temperature_sample_glitch",
                "terminalRuntimeDropReason": "temperature_sample_glitch",
            }],
        });

        assert!(flagship_retryable_environment_summary(&summary, 140));
    }

    #[test]
    fn retryable_environment_summary_rejects_plain_thermal_not_converged() {
        let summary = json!({
            "validation": {
                "failures": [{
                    "phase": "applied",
                    "reason": "incomplete_stage",
                    "stopReason": "full_speed_to_stable_timeout",
                    "targetTempC": 140,
                }],
            },
            "applied": [{
                "targetTempC": 140,
                "stopReason": "full_speed_to_stable_timeout",
                "terminalRuntimeDropReason": null,
            }],
        });

        assert!(!flagship_retryable_environment_summary(&summary, 140));
    }

    #[test]
    fn retryable_environment_batch_summary_accepts_top_level_batch_error_without_runs() {
        let summary = json!({
            "error": "isolapurr USB-C telemetry did not advance for 2141ms",
            "runs": [],
        });

        assert!(flagship_retryable_environment_batch_summary(&summary, 140));
    }

    #[test]
    fn choose_best_batch_run_prefers_top_level_batch_error_when_runs_are_missing() {
        let summary = json!({
            "error": "isolapurr USB-C telemetry did not advance for 2141ms",
            "runs": [],
        });

        let error = choose_best_batch_run(&summary, 140).unwrap_err();
        assert_eq!(
            error.to_string(),
            "isolapurr USB-C telemetry did not advance for 2141ms"
        );
    }

    #[test]
    fn target_failure_summary_prefers_execution_error_over_missing_stage() {
        let summary = json!({
            "error": "isolapurr USB-C telemetry did not advance for 2181ms",
            "validation": {
                "failures": [{
                    "phase": "applied",
                    "reason": "missing_stage",
                    "targetTempC": 220,
                }],
            },
        });

        let failures = target_failure_summary(&summary, 220);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["reason"], "execution_error");
        assert_eq!(
            failures[0]["message"],
            "isolapurr USB-C telemetry did not advance for 2181ms"
        );
    }

    #[test]
    fn review_target_entry_falls_back_to_last_valid_round_result() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 170.0, 100),
            sample(800, "warmup", 182.0, 100),
            sample(1200, "approach", 218.7, 100),
            sample(2200, "hold", 219.7, 96),
            sample(3200, "hold", 220.1, 95),
            sample(4200, "hold", 220.4, 95),
        ]);
        let valid_summary = json!({
            "runId": "valid-run",
            "files": {
                "samplesPath": samples_path,
                "summaryPath": "/tmp/valid-run.json",
            },
            "validation": {
                "failures": [],
                "passed": true,
            },
            "applied": [{
                "targetTempC": 220,
                "stopReason": "completed",
                "maxOvershootC": 1.3,
                "holdPeakToPeakC": 1.6,
                "analysis": {
                    "holdMedianOutputPermille": 820,
                    "holdP90OutputPermille": 860,
                },
                "guard": {
                    "firstHoldAtMs": 2200,
                },
                "fullSpeedToStable": {
                    "limitMs": 5000,
                    "settleTimeMs": 3200,
                    "warmupExitedAtMs": 1000,
                },
            }],
        });
        let summary_path = write_test_json(&valid_summary, "summary");
        let mut valid_summary_for_round = valid_summary.clone();
        valid_summary_for_round["files"]["summaryPath"] =
            json!(summary_path.to_string_lossy().into_owned());
        let round = round_record_from_summary(
            &valid_summary_for_round,
            220,
            1,
            "tuning 1 / scout",
            Some(json!({
                "targetTempC": 220,
                "holdPowerPermille": 780,
            })),
            "scout",
            Some(1),
            None,
            true,
            None,
            42,
        );
        let terminal_summary = json!({
            "runId": "terminal-run",
            "error": "isolapurr USB-C telemetry did not advance for 2181ms",
            "files": {
                "summaryPath": "/tmp/terminal-run.json",
            },
            "validation": {
                "failures": [{
                    "phase": "applied",
                    "reason": "missing_stage",
                    "targetTempC": 220,
                }],
                "passed": false,
            },
            "applied": [],
        });
        let accepted_profile = json!({
            "settings": {},
            "points": [{
                "targetTempC": 220,
                "holdPowerPermille": 780,
                "holdReheatPowerPermille": 820,
            }],
        });

        let entry = review_target_entry(
            220,
            "environment_blocked",
            42,
            vec![round],
            &terminal_summary,
            &accepted_profile,
            60,
        );

        assert_eq!(entry["result"]["stopReason"], "completed");
        assert_eq!(
            entry["result"]["fullSpeedToStable"]["settleTimeMs"],
            json!(3200)
        );
        assert_eq!(entry["samples"].as_array().map(Vec::len), Some(6));
        assert_eq!(entry["failures"][0]["reason"], "execution_error");

        let _ = fs::remove_file(samples_path);
        let _ = fs::remove_file(summary_path);
    }

    #[test]
    fn review_target_entry_rejects_metric_failed_candidate_ready() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 58.0, 100),
            sample(5000, "warmup", 70.0, 100),
            sample(15075, "approach", 91.0, 80),
            sample(19000, "hold", 101.0, 70),
            sample(25000, "hold", 109.0, 70),
        ]);
        let summary = json!({
            "runId": "metric-failed-run",
            "files": {
                "samplesPath": samples_path,
                "summaryPath": "/tmp/metric-failed-run.json",
            },
            "validation": {
                "failures": [],
                "passed": false,
            },
            "applied": [{
                "targetTempC": 100,
                "stopReason": "full_speed_to_stable_timeout",
                "maxOvershootC": 9.04,
                "holdPeakToPeakC": 11.88,
                "analysis": {
                    "holdMedianOutputPermille": 600,
                    "holdP90OutputPermille": 900,
                },
                "guard": {
                    "firstHoldAtMs": 19000,
                },
                "fullSpeedToStable": {
                    "limitMs": 10000,
                    "settleTimeMs": null,
                    "warmupExitedAtMs": 15075,
                    "failureReason": "full_speed_to_stable_timeout",
                },
            }],
        });
        let round = round_record_from_summary(
            &summary,
            100,
            1,
            "tuning 1 / batch",
            Some(json!({
                "targetTempC": 100,
                "holdPowerPermille": 500,
            })),
            "batch_candidate",
            Some(1),
            Some("bad-candidate"),
            true,
            None,
            1200,
        );
        let accepted_profile = json!({
            "settings": {},
            "points": [{
                "targetTempC": 100,
                "holdPowerPermille": 500,
                "holdReheatPowerPermille": 620,
            }],
        });

        let entry = review_target_entry(
            100,
            "budget_exhausted",
            1200,
            vec![round],
            &summary,
            &accepted_profile,
            60,
        );

        assert_eq!(entry["candidateReady"], false);
        assert_eq!(
            entry["candidateDisposition"],
            "budget_exhausted_without_candidate"
        );
        assert_eq!(entry["result"]["holdPeakToPeakC"], json!(11.88));

        let _ = fs::remove_file(samples_path);
    }

    #[test]
    fn validation_target_entry_passes_without_promoting_candidate() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 170.0, 100),
            sample(800, "warmup", 182.0, 100),
            sample(1200, "approach", 218.7, 100),
            sample(2200, "hold", 219.7, 96),
            sample(3200, "hold", 220.1, 95),
            sample(4200, "hold", 220.4, 95),
        ]);
        let summary = json!({
            "runId": "validation-run",
            "files": {
                "samplesPath": samples_path,
                "summaryPath": "/tmp/validation-run.json",
            },
            "parameters": {
                "evaluationMode": "hold-confirm",
            },
            "validation": {
                "failures": [],
                "passed": true,
            },
            "applied": [{
                "targetTempC": 220,
                "stopReason": "completed",
                "maxOvershootC": 1.1,
                "holdPeakToPeakC": 1.5,
                "analysis": {
                    "holdMedianOutputPermille": 780,
                    "holdP90OutputPermille": 820,
                },
                "guard": {
                    "firstHoldAtMs": 2200,
                },
                "fullSpeedToStable": {
                    "limitMs": 5000,
                    "settleTimeMs": 3200,
                    "warmupExitedAtMs": 1000,
                },
            }],
        });
        let round = round_record_from_summary(
            &summary,
            220,
            1,
            "validation / final profile",
            Some(json!({
                "targetTempC": 220,
                "holdPowerPermille": 780,
            })),
            "validation",
            None,
            Some("final-profile"),
            true,
            None,
            64,
        );
        let accepted_profile = json!({
            "settings": {},
            "points": [{
                "targetTempC": 220,
                "holdPowerPermille": 780,
                "holdReheatPowerPermille": 820,
            }],
        });

        let entry = validation_target_entry(
            220,
            "validation_passed",
            64,
            vec![round],
            &summary,
            &accepted_profile,
            60,
        );

        assert_eq!(entry["targetRole"], "validation");
        assert_eq!(entry["budgetOutcome"], "validation_passed");
        assert_eq!(entry["ok"], true);
        assert_eq!(entry["candidateReady"], false);
        assert_eq!(entry["candidateDisposition"], "validation_passed");
        assert_eq!(entry["pointSource"], "validation_final_profile");
        assert_eq!(entry["samples"].as_array().map(Vec::len), Some(6));

        let _ = fs::remove_file(samples_path);
    }

    #[test]
    fn validation_preview_profile_materializes_out_of_anchor_target_without_mutating_anchors() {
        let accepted_profile = json!({
            "settings": {},
            "points": [
                {
                    "targetTempC": 60,
                    "warmupPowerPermille": 1000,
                    "approachPowerPermille": 800,
                    "approachFloorPowerPermille": 650,
                    "holdPowerPermille": 135,
                    "holdReheatPowerPermille": 170
                },
                {
                    "targetTempC": 220,
                    "warmupPowerPermille": 1000,
                    "approachPowerPermille": 1000,
                    "approachFloorPowerPermille": 1000,
                    "holdPowerPermille": 1000,
                    "holdReheatPowerPermille": 1000
                }
            ],
        });

        let validation_profile =
            validation_preview_profile_for_target(&accepted_profile, 240).expect("240C profile");

        let materialized_count = validation_profile["points"]
            .as_array()
            .map(|points| points.iter().filter(|point| !point.is_null()).count());

        assert_eq!(accepted_profile["points"].as_array().map(Vec::len), Some(2));
        assert!(explicit_point_value(&validation_profile, 240).is_some());
        assert_eq!(materialized_count, Some(2));
    }

    #[test]
    fn validation_failure_with_valid_evidence_triggers_supplemental_tuning() {
        let failed = json!({
            "targetRole": "validation",
            "budgetOutcome": "validation_failed",
            "validTestCount": 1,
        });
        let passed = json!({
            "targetRole": "validation",
            "budgetOutcome": "validation_passed",
            "validTestCount": 1,
        });
        let invalid = json!({
            "targetRole": "validation",
            "budgetOutcome": "environment_blocked",
            "validTestCount": 0,
        });

        assert!(validation_entry_should_trigger_supplemental_tuning(&failed));
        assert!(!validation_entry_should_trigger_supplemental_tuning(
            &passed
        ));
        assert!(!validation_entry_should_trigger_supplemental_tuning(
            &invalid
        ));
        assert_eq!(
            supplemental_anchor_targets(&[60, 100, 140], 120),
            vec![60, 100, 120, 140]
        );
    }

    #[test]
    fn failed_hold_confirm_trims_saturated_high_temp_hold_power() {
        let samples_path = write_test_samples(&[
            sample(0, "warmup", 170.0, 100),
            sample(800, "warmup", 182.0, 100),
            sample(1200, "approach", 218.7, 100),
            sample(2200, "hold", 219.7, 100),
            sample(3200, "hold", 220.6, 100),
            sample(4200, "hold", 221.8, 100),
        ]);
        let profile = json!({
            "settings": {},
            "points": [{
                "targetTempC": 220,
                "brakeDistanceCentiC": 330,
                "warmupPowerPermille": 1000,
                "approachPowerPermille": 1000,
                "approachFloorPowerPermille": 1000,
                "approachDampingExponentPermille": 100,
                "approachTailWindowCentiC": 0,
                "holdPowerPermille": 1000,
                "holdReheatPowerPermille": 1000,
                "holdEntryCentiC": 150,
                "holdExitCentiC": 160,
                "holdOnCentiC": 5,
                "holdOffCentiC": 80,
                "overshootCutoffCentiC": 180,
                "holdKpPermillePerC": 14,
                "holdKiPermillePerCTick": 1,
                "holdBlendTicks": 1,
                "approachLeadTicks": 0,
                "holdLeadTicks": 0
            }]
        });
        let summary = json!({
            "files": {
                "samplesPath": samples_path,
            },
            "validation": {
                "failures": [{
                    "targetTempC": 220,
                    "reason": "full_speed_to_stable_missing",
                }],
            },
            "applied": [{
                "targetTempC": 220,
                "stopReason": "full_speed_to_stable_timeout",
                "riseTimeMs": 4100,
                "maxOvershootC": 1.8,
                "holdPeakToPeakC": 2.1,
                "sampleCount": 120,
                "analysis": {
                    "holdMedianOutputPermille": 1000,
                    "holdP90OutputPermille": 1000,
                },
                "guard": {
                    "holdThresholdTempC": 218.5,
                },
                "fullSpeedToStable": {
                    "limitMs": 5000,
                    "settleTimeMs": null,
                    "warmupExitedAtMs": 1000,
                },
            }],
        });

        let reseeded =
            reseed_after_failed_hold_confirm(&profile, 220, &summary).expect("reseed result");
        let point = reseeded
            .and_then(|value| explicit_point_value(&value, 220))
            .expect("220C point");

        assert_eq!(
            point.get("holdPowerPermille").and_then(Value::as_i64),
            Some(780)
        );
        assert_eq!(
            point.get("holdReheatPowerPermille").and_then(Value::as_i64),
            Some(780)
        );
        assert_eq!(
            point.get("holdKpPermillePerC").and_then(Value::as_i64),
            Some(10)
        );

        let _ = fs::remove_file(samples_path);
    }

    #[test]
    fn high_side_gate_nudge_clamps_predicted_brake_and_trims_hold() {
        let current = ThermalCandidatePoint {
            target_temp_c: 220,
            brake_distance_centi_c: 330,
            warmup_power_permille: 1000,
            approach_power_permille: 1000,
            approach_floor_power_permille: 1000,
            approach_damping_exponent_permille: 100,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 1000,
            hold_reheat_power_permille: 1000,
            hold_entry_centi_c: 150,
            hold_exit_centi_c: 160,
            hold_on_centi_c: 5,
            hold_off_centi_c: 80,
            overshoot_cutoff_centi_c: 180,
            hold_kp_permille_per_c: 14,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 1,
            approach_lead_ticks: 0,
            hold_lead_ticks: 0,
        };
        let predicted = ThermalCandidatePoint {
            brake_distance_centi_c: 680,
            approach_damping_exponent_permille: 200,
            approach_lead_ticks: 1,
            hold_power_permille: 1000,
            hold_reheat_power_permille: 1000,
            hold_kp_permille_per_c: 10,
            overshoot_cutoff_centi_c: 180,
            ..current
        };
        let nudged = apply_flagship_gate_nudge(
            &current,
            predicted,
            &json!({"failureClass": "stable_window_broke_high"}),
            220,
        );

        assert_eq!(nudged.brake_distance_centi_c, 510);
        assert_eq!(nudged.hold_power_permille, 820);
        assert_eq!(nudged.hold_reheat_power_permille, 820);
        assert_eq!(nudged.approach_lead_ticks, 1);
        assert_eq!(nudged.overshoot_cutoff_centi_c, 160);
    }
}

fn read_json(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json_pretty(
    path: &Path,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn display_path(path: &Path) -> String {
    match env::current_dir() {
        Ok(cwd) => path
            .strip_prefix(&cwd)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| path.display().to_string()),
        Err(_) => path.display().to_string(),
    }
}

fn display_path_string(path: &str) -> String {
    display_path(Path::new(path))
}

fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn round_decimal(value: f64, places: i32) -> f64 {
    let factor = 10_f64.powi(places);
    (value * factor).round() / factor
}
