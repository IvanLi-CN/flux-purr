async fn collect_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if args.candidate_profile_files.is_empty() {
        return collect_single_thermal_self_test(client, default_devd, args, true).await;
    }
    if args.seed_profile_file.is_some() {
        return Err(
            "thermal batch self-test cannot combine --seed-profile-file with --candidate-profile-file"
                .into(),
        );
    }
    if !args.skip_optimize {
        return Err("thermal batch self-test requires --skip-optimize".into());
    }
    let target_temps_c = parse_thermal_targets(args.targets_c.as_deref())?;
    if target_temps_c.len() != 1 {
        return Err("thermal batch self-test requires exactly one --targets-c value".into());
    }
    collect_batch_thermal_self_test(client, default_devd, args, target_temps_c[0]).await
}

fn thermal_batch_restart_temp_c(target_temp_c: i16, requested_cooldown_temp_c: f64) -> f64 {
    if (requested_cooldown_temp_c - 40.0).abs() > f64::EPSILON {
        return requested_cooldown_temp_c;
    }
    f64::from((target_temp_c - 30).max(40))
}

fn thermal_source_defaults_for_class(source_class: &str) -> (u16, u16) {
    match source_class {
        "pps5a" => (21_000, 5_000),
        _ => (20_000, 3_250),
    }
}

fn read_thermal_bench_source_class(
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => read_isolapurr_configured_source_class(source_url, source_id),
    }
}

fn resolve_thermal_source_selection(
    args: &ThermalSelfTestArgs,
) -> Result<ThermalSourceSelection, Box<dyn std::error::Error + Send + Sync>> {
    let detected_source_class = match args.profile_mode {
        ThermalProfileMode::Auto => {
            read_thermal_bench_source_class(args.source_kind, &args.source_url, &args.source_id)?
        }
        ThermalProfileMode::W65 => thermal_source_class(20_000, 3_250),
        ThermalProfileMode::W100 => thermal_source_class(21_000, 5_000),
    };
    let resolved_bank = args
        .profile_mode
        .explicit_bank()
        .unwrap_or(detected_source_class);
    let (default_voltage_mv, default_current_ma) = args
        .profile_mode
        .explicit_source_defaults()
        .unwrap_or_else(|| thermal_source_defaults_for_class(detected_source_class));
    Ok(ThermalSourceSelection {
        resolved_bank,
        detected_source_class,
        detected_source_class_basis: "configured_capability",
        default_voltage_mv,
        default_current_ma,
    })
}

fn thermal_self_test_uses_point_local_profile(
    selection: &ThermalSourceSelection,
    calibration_run: bool,
) -> bool {
    !calibration_run && selection.resolved_bank != "pps3a"
}

fn thermal_source_request(
    args: &ThermalSelfTestArgs,
    selection: &ThermalSourceSelection,
) -> Result<(u16, u16), Box<dyn std::error::Error + Send + Sync>> {
    let voltage_mv = args
        .source_voltage_v
        .as_deref()
        .map(parse_pps_volts)
        .transpose()?
        .unwrap_or(selection.default_voltage_mv);
    let current_ma = args
        .source_current_a
        .as_deref()
        .map(parse_pps_amps)
        .transpose()?
        .unwrap_or(selection.default_current_ma);
    Ok((voltage_mv, current_ma))
}

fn thermal_default_source_power_watts_for_bank(bank: &str) -> u16 {
    match bank {
        "pps5a" => THERMAL_SOURCE_100W_POWER_WATTS as u16,
        _ => THERMAL_SOURCE_65W_POWER_WATTS as u16,
    }
}

fn thermal_effective_source_power_watts(
    args: &ThermalSelfTestArgs,
    selection: &ThermalSourceSelection,
) -> u16 {
    if args.source_power_watts > 0 {
        args.source_power_watts
    } else {
        thermal_default_source_power_watts_for_bank(selection.resolved_bank)
    }
}

async fn collect_batch_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_selection = resolve_thermal_source_selection(&args)?;
    let use_point_local_profile =
        thermal_self_test_uses_point_local_profile(&source_selection, args.calibration_run);
    let source_power_watts = thermal_effective_source_power_watts(&args, &source_selection);
    let (source_voltage_mv, source_current_ma) = thermal_source_request(&args, &source_selection)?;
    let restart_temp_c = thermal_batch_restart_temp_c(target_temp_c, args.cooldown_temp_c);
    let batch_id = format!(
        "thermal-batch-{}-{}",
        current_unix_millis(),
        slugify_path_component(&resolved.device)
    );
    let batch_dir = args.output_dir.join(&batch_id);
    fs::create_dir_all(&batch_dir)?;
    let (runs, batch_error) = if args.dry_run {
        let runs = collect_batch_dry_runs(ThermalBatchDryInput {
            resolved: &resolved,
            args: &args,
            source_selection: &source_selection,
            source_power_watts,
            target_temp_c,
            restart_temp_c,
            source_voltage_mv,
            source_current_ma,
            batch_id: &batch_id,
            batch_dir: &batch_dir,
        })?;
        (runs, None)
    } else {
        let live = collect_batch_live_runs(ThermalBatchLiveInput {
            client,
            resolved: &resolved,
            args: &args,
            source_selection: &source_selection,
            source_power_watts,
            use_point_local_profile,
            target_temp_c,
            restart_temp_c,
            source_voltage_mv,
            source_current_ma,
            batch_id: &batch_id,
            batch_dir: &batch_dir,
        })
        .await?;
        (live.runs, live.error)
    };

    let passed_candidates = runs
        .iter()
        .filter(|run| run.pointer("/validation/passed").and_then(Value::as_bool) == Some(true))
        .count();
    let summary = json!({
        "kind": "thermal_self_test_batch",
        "ok": batch_error.is_none() && passed_candidates > 0 && runs.len() == args.candidate_profile_files.len(),
        "batchId": batch_id,
        "targetTempC": target_temp_c,
        "restartTempC": restart_temp_c,
        "candidateCount": args.candidate_profile_files.len(),
        "completedCandidateCount": runs.len(),
        "passedCandidateCount": passed_candidates,
        "profilePersistence": "not_saved",
        "sourceHeldAcrossCandidates": !args.dry_run,
        "evaluationMode": args.evaluation_mode.as_str(),
        "runs": runs,
        "error": batch_error,
    });
    fs::write(
        batch_dir.join("batch.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
}

struct ThermalBatchDryInput<'a> {
    resolved: &'a ResolvedUsbTarget,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    target_temp_c: i16,
    restart_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    batch_id: &'a str,
    batch_dir: &'a Path,
}

fn collect_batch_dry_runs(
    input: ThermalBatchDryInput<'_>,
) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalBatchDryInput {
        resolved,
        args,
        source_selection,
        source_power_watts,
        target_temp_c,
        restart_temp_c,
        source_voltage_mv,
        source_current_ma,
        batch_id,
        batch_dir,
    } = input;
    args.candidate_profile_files
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate_file)| {
            let imported = serde_json::from_slice::<Value>(&fs::read(candidate_file)?)?;
            let profile = thermal_candidate_profile_to_value(
                &thermal_candidate_profile_from_value(imported),
            );
            let run_id = format!("{batch_id}-candidate-{candidate_index}");
            let run_dir = batch_dir.join(format!("candidate-{candidate_index}"));
            fs::create_dir_all(&run_dir)?;
            let samples_path = run_dir.join("samples.ndjson");
            let mut samples_writer = BufWriter::new(File::create(&samples_path)?);
            let mut sample_index = 0usize;
            let results = write_dry_thermal_ladder(DryThermalLadderInput {
                samples_writer: &mut samples_writer,
                run_id: &run_id,
                test_phase: "applied",
                source_voltage_mv,
                source_current_ma,
                thermal_profile: Some(&profile),
                heater_parameter_mode: "preview",
                target_temps_c: &[target_temp_c],
                sample_index: &mut sample_index,
            })?;
            samples_writer.flush()?;
            let validation =
                validate_thermal_applied_results(&results, &[target_temp_c], args.evaluation_mode);
            let mut summary = thermal_batch_candidate_summary(ThermalBatchCandidateSummaryInput {
                run_id: &run_id,
                resolved,
                args,
                source_selection,
                source_power_watts,
                candidate_file,
                candidate_index,
                target_temp_c,
                restart_temp_c,
                source_voltage_mv,
                source_current_ma,
                run_dir: &run_dir,
                samples_path: &samples_path,
                profile: &profile,
                results: &results,
                validation,
                sample_count: sample_index,
            });
            thermal_summary_attach_source_analysis_from_ndjson(&mut summary, &samples_path)?;
            write_thermal_batch_candidate_files(&summary, &run_dir)?;
            Ok(summary)
        })
        .collect()
}

struct ThermalBatchLiveInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    use_point_local_profile: bool,
    target_temp_c: i16,
    restart_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    batch_id: &'a str,
    batch_dir: &'a Path,
}

struct ThermalBatchLiveOutcome {
    runs: Vec<Value>,
    error: Option<String>,
}

async fn collect_batch_live_runs(
    input: ThermalBatchLiveInput<'_>,
) -> Result<ThermalBatchLiveOutcome, Box<dyn std::error::Error + Send + Sync>> {
    validate_thermal_bench_source_tools(input.args.source_kind)?;
    let (initial_source_telemetry, lease) = prepare_thermal_source_and_lease(
        ThermalSourceLeaseInput {
            resolved: input.resolved,
            source_kind: input.args.source_kind,
            config: ThermalSourceConfig {
                client: input.client,
                source_url: &input.args.source_url,
                source_id: &input.args.source_id,
                source_mode: &input.args.source_mode,
                profile_mode: input.args.profile_mode,
                source_power_watts: input.source_power_watts,
                voltage_mv: input.source_voltage_mv,
                current_limit_ma: input.source_current_ma,
            },
        },
    )
    .await?;
    let heartbeat = spawn_heartbeat(
        input.client.clone(),
        input.resolved.devd.clone(),
        lease.clone(),
    );
    let mut runs = Vec::new();
    let test_result = tokio::select! {
        result = collect_batch_live_candidates(&input, &lease, initial_source_telemetry, &mut runs) => result,
        _ = thermal_execution_deadline(input.args.execution_deadline) => Err("target_budget_exhausted: thermal batch self-test deadline reached; heater cleanup requested".into()),
        signal = tokio::signal::ctrl_c() => {
            signal?;
            Err("thermal batch self-test interrupted; heater cleanup requested".into())
        }
    };
    heartbeat.abort();
    let mut error = test_result.err().map(|value| value.to_string());
    if let Err(cleanup) =
        force_thermal_self_test_shutdown(input.client, input.resolved, &lease.lease_id).await
        && error.is_none()
    {
        error = Some(format!("thermal batch cleanup failed: {cleanup}"));
    }
    let _ = release_lease(input.client, &input.resolved.devd, &lease.lease_id).await;
    if let Err(cleanup) = restore_thermal_bench_source_default(
        input.client,
        input.args.source_kind,
        &input.args.source_url,
        &input.args.source_id,
    )
    .await
        && error.is_none()
    {
        error = Some(format!(
            "{} cleanup failed: {cleanup}",
            input.args.source_kind.as_str()
        ));
    }
    Ok(ThermalBatchLiveOutcome { runs, error })
}

async fn thermal_execution_deadline(deadline: Option<std::time::Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn collect_batch_live_candidates(
    input: &ThermalBatchLiveInput<'_>,
    lease: &Lease,
    initial_source_telemetry: BenchSourceLiveTelemetry,
    runs: &mut Vec<Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut source_sampler = BenchSourceTelemetrySampler::new(
        input.args.source_kind,
        &input.args.source_url,
        initial_source_telemetry,
    );
    for (candidate_index, candidate_file) in input
        .args
        .candidate_profile_files
        .iter()
        .enumerate()
    {
        let summary = collect_batch_live_candidate(ThermalBatchLiveCandidateInput {
            input,
            lease,
            candidate_file,
            candidate_index,
            source_sampler: &mut source_sampler,
        })
        .await?;
        runs.push(summary);
    }
    Ok(())
}

struct ThermalBatchLiveCandidateInput<'a> {
    input: &'a ThermalBatchLiveInput<'a>,
    lease: &'a Lease,
    candidate_file: &'a Path,
    candidate_index: usize,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
}

async fn collect_batch_live_candidate(
    candidate: ThermalBatchLiveCandidateInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let input = candidate.input;
    let profile = load_batch_candidate_profile(candidate.candidate_file)?;
    request_thermal_runtime_with_retry(
        input.client,
        input.resolved,
        &candidate.lease.lease_id,
        thermal_self_test_cooldown_runtime_body(),
    )
    .await?;
    wait_for_cooldown(
        input.client,
        input.resolved,
        &candidate.lease.lease_id,
        input.restart_temp_c,
        Duration::from_secs(input.args.cooldown_timeout_seconds.max(1)),
    )
    .await?;
    let run_id = format!("{}-candidate-{}", input.batch_id, candidate.candidate_index);
    let run_dir = input
        .batch_dir
        .join(format!("candidate-{}", candidate.candidate_index));
    fs::create_dir_all(&run_dir)?;
    let samples_path = run_dir.join("samples.ndjson");
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);
    let mut sample_index = 0usize;
    let heater_parameters = thermal_batch_candidate_heater_parameters(input.target_temp_c, &profile);
    refresh_batch_candidate_source(input.args, input.source_power_watts, candidate.source_sampler)
        .await?;
    wait_for_cooldown(
        input.client,
        input.resolved,
        &candidate.lease.lease_id,
        input.restart_temp_c,
        Duration::from_secs(input.args.cooldown_timeout_seconds.max(1)),
    )
    .await?;
    let arm_status = preview_prepare_and_arm_thermal_self_test_target(ThermalPreviewTargetInput {
        client: input.client,
        resolved: input.resolved,
        lease_id: &candidate.lease.lease_id,
        profile_mode: input.args.profile_mode,
        profile: &profile,
        target_temp_c: input.target_temp_c,
        heater_parameters: &heater_parameters,
        use_legacy_profile: input.use_point_local_profile,
    })
    .await?;
    let result = run_thermal_stage(ThermalStageInput {
        client: input.client,
        resolved: input.resolved,
        lease_id: &candidate.lease.lease_id,
        samples_writer: &mut samples_writer,
        run_id: &run_id,
        test_phase: "applied",
        target_temp_c: input.target_temp_c,
        source_voltage_mv: input.source_voltage_mv,
        source_current_ma: input.source_current_ma,
        heater_parameters: &heater_parameters,
        runtime_profile: &profile,
        args: input.args,
        source_sampler: candidate.source_sampler,
        sample_index: &mut sample_index,
        initial_status: Some(arm_status),
    })
    .await?;
    let _ = arm_thermal_self_test_heater(
        input.client,
        input.resolved,
        &candidate.lease.lease_id,
        false,
        input.target_temp_c,
    )
    .await?;
    samples_writer.flush()?;
    complete_batch_live_candidate(ThermalBatchCandidateCompletionInput {
        candidate: &candidate,
        input,
        run_id: &run_id,
        run_dir: &run_dir,
        samples_path: &samples_path,
        profile: &profile,
        result,
        sample_count: sample_index,
    })
}

struct ThermalBatchCandidateCompletionInput<'a> {
    candidate: &'a ThermalBatchLiveCandidateInput<'a>,
    input: &'a ThermalBatchLiveInput<'a>,
    run_id: &'a str,
    run_dir: &'a Path,
    samples_path: &'a Path,
    profile: &'a Value,
    result: ThermalStageResult,
    sample_count: usize,
}

fn complete_batch_live_candidate(
    completion: ThermalBatchCandidateCompletionInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let results = vec![completion.result];
    let validation = validate_thermal_applied_results(
        &results,
        &[completion.input.target_temp_c],
        completion.input.args.evaluation_mode,
    );
    finalize_thermal_batch_candidate(ThermalBatchCandidateSummaryInput {
        run_id: completion.run_id,
        resolved: completion.input.resolved,
        args: completion.input.args,
        source_selection: completion.input.source_selection,
        source_power_watts: completion.input.source_power_watts,
        candidate_file: completion.candidate.candidate_file,
        candidate_index: completion.candidate.candidate_index,
        target_temp_c: completion.input.target_temp_c,
        restart_temp_c: completion.input.restart_temp_c,
        source_voltage_mv: completion.input.source_voltage_mv,
        source_current_ma: completion.input.source_current_ma,
        run_dir: completion.run_dir,
        samples_path: completion.samples_path,
        profile: completion.profile,
        results: &results,
        validation,
        sample_count: completion.sample_count,
    })
}

fn thermal_batch_candidate_heater_parameters(target_temp_c: i16, profile: &Value) -> Value {
    thermal_heater_parameters_value(target_temp_c, Some(profile), "preview")
}

async fn refresh_batch_candidate_source(
    args: &ThermalSelfTestArgs,
    source_power_watts: u16,
    source_sampler: &mut BenchSourceTelemetrySampler,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    refresh_thermal_source_sampler_before_stage(args, source_power_watts, source_sampler).await
}

fn load_batch_candidate_profile(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let imported = serde_json::from_slice::<Value>(&fs::read(path)?)?;
    Ok(thermal_candidate_profile_to_value(&thermal_candidate_profile_from_value(imported)))
}

struct ThermalBatchCandidateSummaryInput<'a> {
    run_id: &'a str,
    resolved: &'a ResolvedUsbTarget,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    candidate_file: &'a Path,
    candidate_index: usize,
    target_temp_c: i16,
    restart_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    run_dir: &'a Path,
    samples_path: &'a Path,
    profile: &'a Value,
    results: &'a [ThermalStageResult],
    validation: Value,
    sample_count: usize,
}

fn thermal_batch_candidate_summary(input: ThermalBatchCandidateSummaryInput<'_>) -> Value {
    let ThermalBatchCandidateSummaryInput {
        run_id,
        resolved,
        args,
        source_selection,
        source_power_watts,
        candidate_file,
        candidate_index,
        target_temp_c,
        restart_temp_c,
        source_voltage_mv,
        source_current_ma,
        run_dir,
        samples_path,
        profile,
        results,
        validation,
        sample_count,
    } = input;
    let complete = validation.get("passed").and_then(Value::as_bool) == Some(true);
    json!({
        "kind": "thermal_self_test",
        "ok": complete,
        "runId": run_id,
        "dryRun": args.dry_run,
        "batchCandidateIndex": candidate_index,
        "target": {
            "deviceId": resolved.device,
            "hardwareId": resolved.hardware_id,
            "devd": resolved.devd,
        },
        "source": thermal_source_summary_value(
            args,
            source_selection,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
        ),
        "parameters": {
            "targetsC": [target_temp_c],
            "candidateProfileFile": candidate_file,
            "evaluationMode": args.evaluation_mode.as_str(),
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "effectiveSampleIntervalMs": effective_thermal_sample_interval_ms(args.sample_interval_ms),
            "holdSeconds": args.hold_seconds.max(1),
            "stageTimeoutSeconds": args.stage_timeout_seconds.max(1),
            "warmupTimeoutSeconds": args.warmup_timeout_seconds.max(1),
            "runtimeRearmAttempts": args.runtime_rearm_attempts,
            "cooldownTempC": restart_temp_c,
            "cooldownTimeoutSeconds": args.cooldown_timeout_seconds.max(1),
            "batchCandidate": true,
            "limits": {
                "overshootC": 3.0,
                "holdPeakToPeakC": 3.0,
                "minimumSampleRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
                "sampleRateWindowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
                "fullSpeedToStableMsByTarget": {
                    "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                    "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
                },
                "fullSpeedStableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
                "fullSpeedStableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
                "fullSpeedToStableHardGate": args.evaluation_mode.enforces_stage_limits(),
                "approachThresholdTimeoutMs": 10_000,
                "approachHoldTimeoutMs": 30_000,
                "approachWarmupReentry": "fail"
            }
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": run_dir.join("run.json"),
            "samplesPath": samples_path,
            "candidateProfilePath": run_dir.join("thermal-profile.candidate.json"),
        },
        "candidateProfile": profile,
        "profilePersistence": if args.dry_run { "dry_run" } else { "not_saved" },
        "tuningSteps": [],
        "applied": results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
        "sampleCount": sample_count,
        "complete": complete,
        "error": null,
    })
}

fn finalize_thermal_batch_candidate(
    input: ThermalBatchCandidateSummaryInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let samples_path = input.samples_path.to_path_buf();
    let run_dir = input.run_dir.to_path_buf();
    let mut summary = thermal_batch_candidate_summary(input);
    thermal_summary_attach_source_analysis_from_ndjson(&mut summary, &samples_path)?;
    write_thermal_batch_candidate_files(&summary, &run_dir)?;
    Ok(summary)
}

fn write_thermal_batch_candidate_files(
    summary: &Value,
    run_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    fs::write(
        run_dir.join("thermal-profile.candidate.json"),
        serde_json::to_vec_pretty(&summary["candidateProfile"])?,
    )?;
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_vec_pretty(summary)?,
    )?;
    Ok(())
}

struct ThermalSelfTestRunSetup {
    resolved: ResolvedUsbTarget,
    source_selection: ThermalSourceSelection,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temps_c: Vec<i16>,
    optimize_targets_c: Vec<i16>,
    effective_seed_profile_file: Option<PathBuf>,
    run_id: String,
    run_dir: PathBuf,
    samples_path: PathBuf,
    summary_path: PathBuf,
    candidate_path: PathBuf,
    candidate_profile: ThermalCandidateProfile,
    candidate_profile_value: Value,
}

fn prepare_thermal_self_test_run(
    default_devd: &str,
    args: &ThermalSelfTestArgs,
) -> Result<ThermalSelfTestRunSetup, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_selection = resolve_thermal_source_selection(args)?;
    let source_power_watts = thermal_effective_source_power_watts(args, &source_selection);
    let (source_voltage_mv, source_current_ma) = thermal_source_request(args, &source_selection)?;
    let target_temps_c = parse_thermal_targets(args.targets_c.as_deref())?;
    let optimize_targets_c = if args.skip_optimize {
        Vec::new()
    } else {
        resolve_optimization_targets(&target_temps_c, args.optimize_targets_c.as_deref())?
    };
    let (candidate_profile, effective_seed_profile_file) =
        load_self_test_seed_profile(args, &source_selection)?;
    let candidate_profile_value = thermal_candidate_profile_to_value(&candidate_profile);
    let run_started_unix_ms = current_unix_millis();
    let run_id = format!(
        "thermal-{}-{}",
        run_started_unix_ms,
        slugify_path_component(&resolved.device)
    );
    let run_dir = args.output_dir.join(&run_id);
    let samples_path = run_dir.join("samples.ndjson");
    let summary_path = run_dir.join("run.json");
    let candidate_path = run_dir.join("thermal-profile.candidate.json");
    Ok(ThermalSelfTestRunSetup {
        resolved,
        source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        optimize_targets_c,
        effective_seed_profile_file,
        run_id,
        run_dir,
        samples_path,
        summary_path,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
    })
}

fn load_self_test_seed_profile(
    args: &ThermalSelfTestArgs,
    source_selection: &ThermalSourceSelection,
) -> Result<
    (ThermalCandidateProfile, Option<PathBuf>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    if let Some(seed_profile_file) = args.seed_profile_file.as_ref() {
        let profile = thermal_candidate_profile_from_value(serde_json::from_slice(&fs::read(
            seed_profile_file,
        )?)?);
        return Ok((profile, Some(seed_profile_file.clone())));
    }
    load_thermal_default_seed_candidate_profile(source_selection.resolved_bank)
}

async fn collect_single_thermal_self_test(
    client: &Client,
    default_devd: &str,
    args: ThermalSelfTestArgs,
    save_profile_on_pass: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let setup = prepare_thermal_self_test_run(default_devd, &args)?;
    let ThermalSelfTestRunSetup {
        resolved,
        source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        optimize_targets_c,
        effective_seed_profile_file,
        run_id,
        run_dir,
        samples_path,
        summary_path,
        candidate_path,
        mut candidate_profile,
        mut candidate_profile_value,
    } = setup;
    fs::create_dir_all(&run_dir)?;
    fs::write(
        &candidate_path,
        serde_json::to_vec_pretty(&candidate_profile_value)?,
    )?;
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);

    let mut sample_index = 0usize;
    let mut applied_results = Vec::new();
    let mut run_error = None::<String>;
    let mut saved_profile_retained = false;
    let mut tuning_steps = Vec::<Value>::new();
    let mut discarded_environment_attempts = Vec::<Value>::new();

    if args.dry_run {
        applied_results = write_dry_thermal_ladder(DryThermalLadderInput {
            samples_writer: &mut samples_writer,
            run_id: &run_id,
            test_phase: "applied",
            source_voltage_mv,
            source_current_ma,
            thermal_profile: Some(&candidate_profile_value),
            heater_parameter_mode: "saved",
            target_temps_c: &target_temps_c,
            sample_index: &mut sample_index,
        })?;
    } else {
        run_live_thermal_self_test(ThermalLiveSelfTestInput {
            client,
            resolved: &resolved,
            args: &args,
            source_selection: &source_selection,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
            target_temps_c: &target_temps_c,
            optimize_targets_c: &optimize_targets_c,
            run_id: &run_id,
            candidate_path: &candidate_path,
            candidate_profile: &mut candidate_profile,
            candidate_profile_value: &mut candidate_profile_value,
            samples_writer: &mut samples_writer,
            sample_index: &mut sample_index,
            applied_results: &mut applied_results,
            run_error: &mut run_error,
            saved_profile_retained: &mut saved_profile_retained,
            tuning_steps: &mut tuning_steps,
            discarded_environment_attempts: &mut discarded_environment_attempts,
            save_profile_on_pass,
        })
        .await?;
    }

    samples_writer.flush()?;
    let validation =
        validate_thermal_applied_results(&applied_results, &target_temps_c, args.evaluation_mode);
    let complete = run_error.is_none() && validation["passed"].as_bool() == Some(true);
    write_thermal_self_test_summary(ThermalSelfTestSummaryInput {
        run_id: &run_id,
        args: &args,
        resolved: &resolved,
        source_selection: &source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c: &target_temps_c,
        optimize_targets_c: &optimize_targets_c,
        effective_seed_profile_file: effective_seed_profile_file.as_ref(),
        run_dir: &run_dir,
        summary_path: &summary_path,
        samples_path: &samples_path,
        candidate_path: &candidate_path,
        candidate_profile_value: &candidate_profile_value,
        saved_profile_retained,
        tuning_steps: &tuning_steps,
        discarded_environment_attempts: &discarded_environment_attempts,
        applied_results: &applied_results,
        validation: &validation,
        sample_index,
        complete,
        run_error: &run_error,
    })
}

struct ThermalSelfTestSummaryInput<'a> {
    run_id: &'a str,
    args: &'a ThermalSelfTestArgs,
    resolved: &'a ResolvedUsbTarget,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temps_c: &'a [i16],
    optimize_targets_c: &'a [i16],
    effective_seed_profile_file: Option<&'a PathBuf>,
    run_dir: &'a Path,
    summary_path: &'a Path,
    samples_path: &'a Path,
    candidate_path: &'a Path,
    candidate_profile_value: &'a Value,
    saved_profile_retained: bool,
    tuning_steps: &'a [Value],
    discarded_environment_attempts: &'a [Value],
    applied_results: &'a [ThermalStageResult],
    validation: &'a Value,
    sample_index: usize,
    complete: bool,
    run_error: &'a Option<String>,
}

fn write_thermal_self_test_summary(
    input: ThermalSelfTestSummaryInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let summary_path = input.summary_path;
    let samples_path = input.samples_path;
    let mut summary = thermal_self_test_summary(input);
    thermal_summary_attach_source_analysis_from_ndjson(&mut summary, samples_path)?;
    fs::write(summary_path, serde_json::to_vec_pretty(&summary)?)?;
    if let Some(id) = summary
        .get("target")
        .and_then(|target| target.get("hardwareId"))
        .and_then(Value::as_str)
    {
        let _ = remember_usb(
            id,
            summary["target"]["deviceId"].as_str().unwrap_or_default(),
            summary["target"]["devd"]
                .as_str()
                .unwrap_or(DEFAULT_DEVD_ENDPOINT),
        );
    }
    Ok(summary)
}

fn thermal_self_test_summary(input: ThermalSelfTestSummaryInput<'_>) -> Value {
    let ThermalSelfTestSummaryInput {
        run_id,
        args,
        resolved,
        source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        optimize_targets_c,
        effective_seed_profile_file,
        run_dir,
        summary_path,
        samples_path,
        candidate_path,
        candidate_profile_value,
        saved_profile_retained,
        tuning_steps,
        discarded_environment_attempts,
        applied_results,
        validation,
        sample_index,
        complete,
        run_error,
    } = input;
    json!({
        "kind": "thermal_self_test",
        "ok": complete,
        "runId": run_id,
        "dryRun": args.dry_run,
        "target": {
            "deviceId": resolved.device,
            "hardwareId": resolved.hardware_id,
            "devd": resolved.devd,
        },
        "source": thermal_source_summary_value(
            args,
            source_selection,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
        ),
        "parameters": thermal_self_test_parameters(
            args,
            target_temps_c,
            optimize_targets_c,
            effective_seed_profile_file,
            !saved_profile_retained,
        ),
        "files": {
            "runDir": run_dir,
            "summaryPath": summary_path,
            "samplesPath": samples_path,
            "candidateProfilePath": candidate_path,
        },
        "candidateProfile": candidate_profile_value,
        "profilePersistence": if args.dry_run {
            "dry_run"
        } else if saved_profile_retained {
            "saved_tuned_candidate"
        } else {
            "not_saved"
        },
        "tuningSteps": tuning_steps,
        "discardedEnvironmentAttempts": discarded_environment_attempts,
        "applied": applied_results.iter().map(ThermalStageResult::to_value).collect::<Vec<_>>(),
        "validation": validation,
        "sampleCount": sample_index,
        "complete": complete,
        "error": run_error,
    })
}

fn thermal_self_test_parameters(
    args: &ThermalSelfTestArgs,
    target_temps_c: &[i16],
    optimize_targets_c: &[i16],
    effective_seed_profile_file: Option<&PathBuf>,
    batch_candidate: bool,
) -> Value {
    json!({
        "targetsC": target_temps_c,
        "optimizeTargetsC": optimize_targets_c,
        "seedProfileFile": effective_seed_profile_file.map(|path| path.to_string_lossy().into_owned()),
        "batchCandidate": batch_candidate,
        "evaluationMode": args.evaluation_mode.as_str(),
        "sampleIntervalMs": args.sample_interval_ms.max(1),
        "effectiveSampleIntervalMs": effective_thermal_sample_interval_ms(args.sample_interval_ms),
        "holdSeconds": args.hold_seconds.max(1),
        "stageTimeoutSeconds": args.stage_timeout_seconds.max(1),
        "warmupTimeoutSeconds": args.warmup_timeout_seconds.max(1),
        "runtimeRearmAttempts": args.runtime_rearm_attempts,
        "cooldownTempC": args.cooldown_temp_c,
        "cooldownTimeoutSeconds": args.cooldown_timeout_seconds.max(1),
        "limits": {
            "overshootC": 3.0,
            "holdPeakToPeakC": 3.0,
            "minimumSampleRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
            "sampleRateWindowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
            "fullSpeedToStableMsByTarget": {
                "lte150C": ThermalFullSpeedStableTracker::LOW_TEMP_SETTLE_LIMIT_MS,
                "gt150C": ThermalFullSpeedStableTracker::HIGH_TEMP_SETTLE_LIMIT_MS,
            },
            "fullSpeedStableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
            "fullSpeedStableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
            "fullSpeedToStableHardGate": args.evaluation_mode.enforces_stage_limits(),
            "approachThresholdTimeoutMs": 10_000,
            "approachHoldTimeoutMs": 30_000,
            "approachWarmupReentry": "fail"
        }
    })
}

struct ThermalLiveSelfTestInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temps_c: &'a [i16],
    optimize_targets_c: &'a [i16],
    run_id: &'a str,
    candidate_path: &'a Path,
    candidate_profile: &'a mut ThermalCandidateProfile,
    candidate_profile_value: &'a mut Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    applied_results: &'a mut Vec<ThermalStageResult>,
    run_error: &'a mut Option<String>,
    saved_profile_retained: &'a mut bool,
    tuning_steps: &'a mut Vec<Value>,
    discarded_environment_attempts: &'a mut Vec<Value>,
    save_profile_on_pass: bool,
}

async fn run_live_thermal_self_test(
    input: ThermalLiveSelfTestInput<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalLiveSelfTestInput {
        client,
        resolved,
        args,
        source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        optimize_targets_c,
        run_id,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
        samples_writer,
        sample_index,
        applied_results,
        run_error,
        saved_profile_retained,
        tuning_steps,
        discarded_environment_attempts,
        save_profile_on_pass,
    } = input;
    validate_thermal_bench_source_tools(args.source_kind)?;
    let (initial_source_telemetry, lease) =
        prepare_thermal_source_and_lease(ThermalSourceLeaseInput {
            resolved,
            source_kind: args.source_kind,
            config: ThermalSourceConfig {
                client,
                source_url: &args.source_url,
                source_id: &args.source_id,
                source_mode: &args.source_mode,
                profile_mode: args.profile_mode,
                source_power_watts,
                voltage_mv: source_voltage_mv,
                current_limit_ma: source_current_ma,
            },
        })
        .await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let test_result = tokio::select! {
        result = run_live_thermal_test_future(ThermalLiveTestFutureInput {
            client,
            resolved,
            lease: &lease,
            args,
            source_selection,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
            target_temps_c,
            optimize_targets_c,
            run_id,
            candidate_path,
            candidate_profile,
            candidate_profile_value,
            samples_writer,
            sample_index,
            applied_results,
            saved_profile_retained,
            tuning_steps,
            discarded_environment_attempts,
            save_profile_on_pass,
        }, initial_source_telemetry) => result,
        _ = thermal_execution_deadline(args.execution_deadline) => Err("target_budget_exhausted: thermal self-test deadline reached; heater cleanup requested".into()),
        signal = tokio::signal::ctrl_c() => {
            signal?;
            Err("thermal self-test interrupted; heater cleanup requested".into())
        }
    };
    heartbeat.abort();
    if let Err(error) = test_result {
        *run_error = Some(error.to_string());
    }
    if let Err(error) =
        force_thermal_self_test_shutdown(client, resolved, &lease.lease_id).await
        && run_error.is_none()
    {
        *run_error = Some(format!("thermal self-test cleanup failed: {error}"));
    }
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    if let Err(error) = restore_thermal_bench_source_default(
        client,
        args.source_kind,
        &args.source_url,
        &args.source_id,
    )
    .await
        && run_error.is_none()
    {
        *run_error = Some(format!("{} cleanup failed: {error}", args.source_kind.as_str()));
    }
    Ok(())
}

struct ThermalLiveTestFutureInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temps_c: &'a [i16],
    optimize_targets_c: &'a [i16],
    run_id: &'a str,
    candidate_path: &'a Path,
    candidate_profile: &'a mut ThermalCandidateProfile,
    candidate_profile_value: &'a mut Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    applied_results: &'a mut Vec<ThermalStageResult>,
    saved_profile_retained: &'a mut bool,
    tuning_steps: &'a mut Vec<Value>,
    discarded_environment_attempts: &'a mut Vec<Value>,
    save_profile_on_pass: bool,
}

async fn run_live_thermal_test_future(
    input: ThermalLiveTestFutureInput<'_>,
    initial_source_telemetry: BenchSourceLiveTelemetry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalLiveTestFutureInput {
        client,
        resolved,
        lease,
        args,
        source_selection,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        optimize_targets_c,
        run_id,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
        samples_writer,
        sample_index,
        applied_results,
        saved_profile_retained,
        tuning_steps,
        discarded_environment_attempts,
        save_profile_on_pass,
    } = input;
    let mut source_sampler = prepare_live_thermal_test(
        client,
        resolved,
        lease,
        args,
        initial_source_telemetry,
    )
    .await?;
    let optimization_completed = run_thermal_optimization_stages(ThermalOptimizationInput {
        client,
        resolved,
        lease,
        args,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        optimize_targets_c,
        run_id,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
        samples_writer,
        sample_index,
        tuning_steps,
        source_sampler: &mut source_sampler,
        use_point_local_profile: thermal_self_test_uses_point_local_profile(
            source_selection,
            args.calibration_run,
        ),
    })
    .await?;
    if optimization_completed {
        cool_down_after_optimization(client, resolved, lease, args, optimize_targets_c).await?;
        run_thermal_applied_stages(ThermalAppliedStagesInput {
            client,
            resolved,
            lease,
            args,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
            target_temps_c,
            run_id,
            candidate_profile_value,
            samples_writer,
            sample_index,
            applied_results,
            tuning_steps,
            discarded_environment_attempts,
            source_sampler: &mut source_sampler,
            use_point_local_profile: thermal_self_test_uses_point_local_profile(
                source_selection,
                args.calibration_run,
            ),
        })
        .await?;
    }
    let validation =
        validate_thermal_applied_results(applied_results, target_temps_c, args.evaluation_mode);
    if validation["passed"].as_bool() == Some(true) && save_profile_on_pass && !args.calibration_run
    {
        save_thermal_profile_on_pass(ThermalProfileSaveInput {
            client,
            resolved,
            lease,
            args,
            source_selection,
            candidate_profile,
            saved_profile_retained,
        })
        .await?;
    }
    Ok(())
}

async fn prepare_live_thermal_test(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease: &Lease,
    args: &ThermalSelfTestArgs,
    initial_source_telemetry: BenchSourceLiveTelemetry,
) -> Result<BenchSourceTelemetrySampler, Box<dyn std::error::Error + Send + Sync>> {
    let source_sampler = BenchSourceTelemetrySampler::new(
        args.source_kind,
        &args.source_url,
        initial_source_telemetry,
    );
    request_thermal_runtime_with_retry(
        client,
        resolved,
        &lease.lease_id,
        thermal_self_test_cooldown_runtime_body(),
    )
    .await?;
    wait_for_cooldown(
        client,
        resolved,
        &lease.lease_id,
        args.cooldown_temp_c,
        Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
    )
    .await?;
    Ok(source_sampler)
}

async fn cool_down_after_optimization(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease: &Lease,
    args: &ThermalSelfTestArgs,
    optimize_targets_c: &[i16],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if optimize_targets_c.is_empty() {
        return Ok(());
    }
    request_thermal_runtime_with_retry(
        client,
        resolved,
        &lease.lease_id,
        thermal_self_test_cooldown_runtime_body(),
    )
    .await?;
    wait_for_cooldown(
        client,
        resolved,
        &lease.lease_id,
        args.cooldown_temp_c,
        Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
    )
    .await
}

struct ThermalAppliedStagesInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temps_c: &'a [i16],
    run_id: &'a str,
    candidate_profile_value: &'a Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    applied_results: &'a mut Vec<ThermalStageResult>,
    tuning_steps: &'a mut Vec<Value>,
    discarded_environment_attempts: &'a mut Vec<Value>,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    use_point_local_profile: bool,
}

async fn run_thermal_applied_stages(
    input: ThermalAppliedStagesInput<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalAppliedStagesInput {
        client,
        resolved,
        lease,
        args,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temps_c,
        run_id,
        candidate_profile_value,
        samples_writer,
        sample_index,
        applied_results,
        tuning_steps,
        discarded_environment_attempts,
        source_sampler,
        use_point_local_profile,
    } = input;
    for (stage_index, target_temp_c) in target_temps_c.iter().copied().enumerate() {
        let result = run_thermal_applied_stage(ThermalAppliedStageInput {
            client,
            resolved,
            lease,
            args,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
            target_temp_c,
            run_id,
            candidate_profile_value,
            samples_writer,
            sample_index,
            discarded_environment_attempts,
            source_sampler,
            use_point_local_profile,
        })
        .await?;
        tuning_steps.push(json!({
            "phase": "applied",
            "stageIndex": stage_index,
            "targetTempC": target_temp_c,
            "result": result.to_value(),
            "candidateProfile": candidate_profile_value.clone(),
        }));
        let complete = result.stop_reason == "completed";
        applied_results.push(result);
        if !complete {
            break;
        }
    }
    Ok(())
}

struct ThermalAppliedStageInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temp_c: i16,
    run_id: &'a str,
    candidate_profile_value: &'a Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    discarded_environment_attempts: &'a mut Vec<Value>,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    use_point_local_profile: bool,
}

async fn run_thermal_applied_stage(
    input: ThermalAppliedStageInput<'_>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalAppliedStageInput {
        client,
        resolved,
        lease,
        args,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temp_c,
        run_id,
        candidate_profile_value,
        samples_writer,
        sample_index,
        discarded_environment_attempts,
        source_sampler,
        use_point_local_profile,
    } = input;
    let heater_parameters =
        thermal_heater_parameters_value(target_temp_c, Some(candidate_profile_value), "preview");
    let mut retries_remaining = args.runtime_rearm_attempts;
    let mut attempt_index = 0u8;
    loop {
        wait_for_cooldown(
            client,
            resolved,
            &lease.lease_id,
            args.cooldown_temp_c,
            Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
        )
        .await?;
        refresh_thermal_source_sampler_before_stage(args, source_power_watts, source_sampler).await?;
        let arm_status = preview_prepare_and_arm_thermal_self_test_target(ThermalPreviewTargetInput {
            client,
            resolved,
            lease_id: &lease.lease_id,
            profile_mode: args.profile_mode,
            profile: candidate_profile_value,
            target_temp_c,
            heater_parameters: &heater_parameters,
            use_legacy_profile: use_point_local_profile,
        })
        .await?;
        let test_phase = if attempt_index == 0 {
            "applied"
        } else {
            "applied_environment_retry"
        };
        let attempt = run_thermal_stage(ThermalStageInput {
            client,
            resolved,
            lease_id: &lease.lease_id,
            samples_writer,
            run_id,
            test_phase,
            target_temp_c,
            source_voltage_mv,
            source_current_ma,
            heater_parameters: &heater_parameters,
            runtime_profile: candidate_profile_value,
            args,
            source_sampler,
            sample_index,
            initial_status: Some(arm_status),
        })
        .await?;
        let _ =
            arm_thermal_self_test_heater(client, resolved, &lease.lease_id, false, target_temp_c)
                .await?;
        if retries_remaining > 0 && thermal_stage_should_retry_after_environment_fault(&attempt) {
            discarded_environment_attempts.push(json!({
                "targetTempC": target_temp_c,
                "attemptIndex": attempt_index,
                "result": attempt.to_value(),
                "restartTempC": args.cooldown_temp_c,
                "retriesRemaining": retries_remaining.saturating_sub(1),
            }));
            retries_remaining = retries_remaining.saturating_sub(1);
            attempt_index = attempt_index.saturating_add(1);
            continue;
        }
        return Ok(attempt);
    }
}

struct ThermalProfileSaveInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_selection: &'a ThermalSourceSelection,
    candidate_profile: &'a ThermalCandidateProfile,
    saved_profile_retained: &'a mut bool,
}

async fn save_thermal_profile_on_pass(
    input: ThermalProfileSaveInput<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalProfileSaveInput {
        client,
        resolved,
        lease,
        args,
        source_selection,
        candidate_profile,
        saved_profile_retained,
    } = input;
    let persisted_profile_value =
        thermal_candidate_profile_to_value(&thermal_profile_for_persistence(candidate_profile)?);
    let save_status = request_leased(
        client,
        resolved,
        &lease.lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({
            "thermalControlProfile": {
                "op": "save",
                "bank": source_selection.resolved_bank,
                "profile": persisted_profile_value,
            },
            "thermalProfileMode": args.profile_mode.as_str(),
        })),
    )
    .await?;
    if require_status_bool(&save_status, "thermalControlProfilePreview")? {
        return Err("thermal profile save unexpectedly left preview mode enabled".into());
    }
    *saved_profile_retained = true;
    Ok(())
}

struct ThermalOptimizationInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    optimize_targets_c: &'a [i16],
    run_id: &'a str,
    candidate_path: &'a Path,
    candidate_profile: &'a mut ThermalCandidateProfile,
    candidate_profile_value: &'a mut Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    tuning_steps: &'a mut Vec<Value>,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    use_point_local_profile: bool,
}

async fn run_thermal_optimization_stages(
    input: ThermalOptimizationInput<'_>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalOptimizationInput {
        client,
        resolved,
        lease,
        args,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        optimize_targets_c,
        run_id,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
        samples_writer,
        sample_index,
        tuning_steps,
        source_sampler,
        use_point_local_profile,
    } = input;
    for (stage_index, target_temp_c) in optimize_targets_c.iter().copied().enumerate() {
        let result = run_thermal_optimization_stage(ThermalOptimizationStageInput {
            client,
            resolved,
            lease,
            args,
            source_power_watts,
            source_voltage_mv,
            source_current_ma,
            target_temp_c,
            run_id,
            candidate_path,
            candidate_profile,
            candidate_profile_value,
            samples_writer,
            sample_index,
            anchor_targets_c: optimize_targets_c,
            source_sampler,
            use_point_local_profile,
        })
        .await?;
        tuning_steps.push(json!({
            "phase": "optimize",
            "stageIndex": stage_index,
            "targetTempC": target_temp_c,
            "result": result.to_value(),
            "candidateProfile": candidate_profile_value.clone(),
        }));
        if !thermal_stage_can_continue_tuning(&result) {
            return Ok(false);
        }
    }
    Ok(true)
}

struct ThermalOptimizationStageInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a ThermalSelfTestArgs,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    target_temp_c: i16,
    run_id: &'a str,
    candidate_path: &'a Path,
    candidate_profile: &'a mut ThermalCandidateProfile,
    candidate_profile_value: &'a mut Value,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    anchor_targets_c: &'a [i16],
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    use_point_local_profile: bool,
}

async fn run_thermal_optimization_stage(
    input: ThermalOptimizationStageInput<'_>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalOptimizationStageInput {
        client,
        resolved,
        lease,
        args,
        source_power_watts,
        source_voltage_mv,
        source_current_ma,
        target_temp_c,
        run_id,
        candidate_path,
        candidate_profile,
        candidate_profile_value,
        samples_writer,
        sample_index,
        anchor_targets_c,
        source_sampler,
        use_point_local_profile,
    } = input;
    let heater_parameters =
        thermal_heater_parameters_value(target_temp_c, Some(candidate_profile_value), "preview");
    wait_for_cooldown(
        client,
        resolved,
        &lease.lease_id,
        args.cooldown_temp_c,
        Duration::from_secs(args.cooldown_timeout_seconds.max(1)),
    )
    .await?;
    refresh_thermal_source_sampler_before_stage(args, source_power_watts, source_sampler).await?;
    let arm_status = preview_prepare_and_arm_thermal_self_test_target(ThermalPreviewTargetInput {
        client,
        resolved,
        lease_id: &lease.lease_id,
        profile_mode: args.profile_mode,
        profile: candidate_profile_value,
        target_temp_c,
        heater_parameters: &heater_parameters,
        use_legacy_profile: use_point_local_profile,
    })
    .await?;
    let result = run_thermal_stage(ThermalStageInput {
        client,
        resolved,
        lease_id: &lease.lease_id,
        samples_writer,
        run_id,
        test_phase: "optimize",
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters: &heater_parameters,
        runtime_profile: candidate_profile_value,
        args,
        source_sampler,
        sample_index,
        initial_status: Some(arm_status),
    })
    .await?;
    if let Some(point) = thermal_candidate_point_mut(candidate_profile, target_temp_c) {
        *point = tune_thermal_candidate_point(*point, &result);
    }
    thermal_rebuild_profile_from_anchor_targets(candidate_profile, anchor_targets_c);
    *candidate_profile_value = thermal_candidate_profile_to_value(candidate_profile);
    fs::write(candidate_path, serde_json::to_vec_pretty(candidate_profile_value)?)?;
    let _ = arm_thermal_self_test_heater(
        client,
        resolved,
        &lease.lease_id,
        false,
        target_temp_c,
    )
    .await?;
    Ok(result)
}

#[cfg(test)]
fn default_thermal_candidate_profile() -> Value {
    thermal_candidate_profile_to_value(&thermal_seed_candidate_profile())
}

const THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE: &str = "thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd63w-pps3a/accepted-full-range-20hz/thermal-profile.accepted.json";
const THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE: &str = "thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd100w-pps5a/accepted-full-range-20hz/thermal-profile.accepted.json";
const THERMAL_PPS5A_TUNING_SEED_RELATIVE: &str =
    "thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json";

fn flux_purr_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn thermal_default_seed_candidates_for_bank(bank: &str) -> Vec<PathBuf> {
    let repo_root = flux_purr_repo_root();
    match bank {
        "pps5a" => vec![
            repo_root.join(THERMAL_PPS5A_ACCEPTED_SEED_RELATIVE),
            repo_root.join(THERMAL_PPS5A_TUNING_SEED_RELATIVE),
        ],
        "pps3a" => vec![repo_root.join(THERMAL_PPS3A_ACCEPTED_SEED_RELATIVE)],
        _ => Vec::new(),
    }
}

fn load_thermal_default_seed_candidate_profile(
    bank: &str,
) -> Result<(ThermalCandidateProfile, Option<PathBuf>), Box<dyn std::error::Error + Send + Sync>> {
    for path in thermal_default_seed_candidates_for_bank(bank) {
        if path.exists() {
            let imported = serde_json::from_slice::<Value>(&fs::read(&path)?)?;
            return Ok((thermal_candidate_profile_from_value(imported), Some(path)));
        }
    }
    Ok((thermal_seed_candidate_profile(), None))
}

fn thermal_seed_candidate_profile() -> ThermalCandidateProfile {
    ThermalCandidateProfile {
        settings: thermal_default_settings(),
        points: THERMAL_PROFILE_ANCHOR_TARGETS_C
            .iter()
            .copied()
            .map(thermal_default_target_point)
            .collect(),
    }
}

fn thermal_profile_for_persistence(
    profile: &ThermalCandidateProfile,
) -> Result<ThermalCandidateProfile, std::io::Error> {
    if profile.points.len() > THERMAL_CONTROL_PROFILE_MAX_POINTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "thermal profile has {} points; firmware supports at most {THERMAL_CONTROL_PROFILE_MAX_POINTS}",
                profile.points.len()
            ),
        ));
    }
    Ok(profile.clone())
}

fn thermal_default_settings() -> ThermalCandidateSettings {
    ThermalCandidateSettings {
        temp_filter_alpha_permille: 750,
        approach_max_ticks: 250,
        approach_min_power_ratio_permille: 500,
        auto_adjustable_working_floor_mv: 5_000,
        heater_current_reserve_ma: 200,
    }
}

fn thermal_default_target_point(target_temp_c: i16) -> ThermalCandidatePoint {
    let (
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        warmup_reenter_centi_c,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = thermal_default_target_values(target_temp_c);
    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c: 0,
        hold_power_permille,
        hold_reheat_power_permille,
        warmup_reenter_centi_c,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    }
}

fn thermal_candidate_profile_to_value(profile: &ThermalCandidateProfile) -> Value {
    let mut points = profile
        .points
        .iter()
        .copied()
        .map(|point| {
            json!({
                "targetTempC": point.target_temp_c,
                "brakeDistanceCentiC": point.brake_distance_centi_c,
                "warmupPowerPermille": 1_000,
                "approachPowerPermille": point.approach_power_permille,
                "approachFloorPowerPermille": point.approach_floor_power_permille,
                "approachDampingExponentPermille": point.approach_damping_exponent_permille,
                "approachTailWindowCentiC": point.approach_tail_window_centi_c,
                "holdPowerPermille": point.hold_power_permille,
                "holdReheatPowerPermille": point.hold_reheat_power_permille,
                "warmupReenterCentiC": point.warmup_reenter_centi_c,
                "holdEntryCentiC": point.hold_entry_centi_c,
                "holdExitCentiC": point.hold_exit_centi_c,
                "holdOnCentiC": point.hold_on_centi_c,
                "holdOffCentiC": point.hold_off_centi_c,
                "overshootCutoffCentiC": point.overshoot_cutoff_centi_c,
                "holdKpPermillePerC": point.hold_kp_permille_per_c,
                "holdKiPermillePerCTick": point.hold_ki_permille_per_c_tick,
                "holdBlendTicks": point.hold_blend_ticks,
                "approachLeadTicks": point.approach_lead_ticks,
                "holdLeadTicks": point.hold_lead_ticks,
            })
        })
        .collect::<Vec<_>>();
    while points.len() < THERMAL_CONTROL_PROFILE_MAX_POINTS {
        points.push(Value::Null);
    }
    json!({
        "settings": thermal_candidate_settings_to_value(profile.settings),
        "points": points
    })
}

fn thermal_candidate_profile_from_value(imported: Value) -> ThermalCandidateProfile {
    let profile = thermal_profile_package_from_value(imported);
    let settings_value = profile.get("settings").cloned().unwrap_or(Value::Null);
    let points_value = profile
        .get("points")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    ThermalCandidateProfile {
        settings: thermal_candidate_settings_from_value(&settings_value),
        points: thermal_candidate_points_from_values(&points_value, &settings_value),
    }
}

fn thermal_candidate_settings_from_value(value: &Value) -> ThermalCandidateSettings {
    let defaults = thermal_default_settings();
    ThermalCandidateSettings {
        temp_filter_alpha_permille: value_u16(value, "tempFilterAlphaPermille")
            .unwrap_or(defaults.temp_filter_alpha_permille),
        approach_max_ticks: value_u16(value, "approachMaxTicks")
            .unwrap_or(defaults.approach_max_ticks),
        approach_min_power_ratio_permille: value_u16(value, "approachMinPowerRatioPermille")
            .unwrap_or(defaults.approach_min_power_ratio_permille),
        auto_adjustable_working_floor_mv: value_u16(value, "autoAdjustableWorkingFloorMv")
            .unwrap_or(defaults.auto_adjustable_working_floor_mv),
        heater_current_reserve_ma: value_u16(value, "heaterCurrentReserveMa")
            .unwrap_or(defaults.heater_current_reserve_ma),
    }
}

fn thermal_candidate_points_from_values(
    values: &[Value],
    settings: &Value,
) -> Vec<ThermalCandidatePoint> {
    let targets = if values.is_empty() {
        THERMAL_PROFILE_ANCHOR_TARGETS_C.to_vec()
    } else {
        values
            .iter()
            .filter_map(|point| value_i16(point, "targetTempC"))
            .collect()
    };
    targets
        .into_iter()
        .map(|target| {
            let point = values
                .iter()
                .find(|value| value_i16(value, "targetTempC") == Some(target))
                .cloned()
                .unwrap_or(Value::Null);
            thermal_candidate_point_from_value(target, &point, settings)
        })
        .collect()
}

fn thermal_candidate_point_from_value(
    target_temp_c: i16,
    value: &Value,
    settings: &Value,
) -> ThermalCandidatePoint {
    let defaults = thermal_default_target_point(target_temp_c);
    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: value_u16(value, "brakeDistanceCentiC")
            .unwrap_or(defaults.brake_distance_centi_c),
        warmup_power_permille: 1_000,
        approach_power_permille: value_u16(value, "approachPowerPermille")
            .unwrap_or(defaults.approach_power_permille),
        approach_floor_power_permille: value_u16(value, "approachFloorPowerPermille")
            .unwrap_or(defaults.approach_floor_power_permille),
        approach_damping_exponent_permille: value_u16(value, "approachDampingExponentPermille")
            .unwrap_or(defaults.approach_damping_exponent_permille),
        approach_tail_window_centi_c: value_u16(value, "approachTailWindowCentiC")
            .unwrap_or(defaults.approach_tail_window_centi_c),
        hold_power_permille: value_u16(value, "holdPowerPermille")
            .unwrap_or(defaults.hold_power_permille),
        hold_reheat_power_permille: inherited_point_u16(
            value,
            "holdReheatPowerPermille",
            settings,
            "holdReheatPowerPermille",
            defaults.hold_reheat_power_permille,
        ),
        warmup_reenter_centi_c: inherited_point_u16(value, "warmupReenterCentiC", settings, "warmupReenterCentiC", defaults.warmup_reenter_centi_c),
        hold_entry_centi_c: inherited_point_u16(value, "holdEntryCentiC", settings, "holdEntryCentiC", defaults.hold_entry_centi_c),
        hold_exit_centi_c: inherited_point_u16(value, "holdExitCentiC", settings, "holdExitCentiC", defaults.hold_exit_centi_c),
        hold_on_centi_c: inherited_point_u16(value, "holdOnCentiC", settings, "holdOnCentiC", defaults.hold_on_centi_c),
        hold_off_centi_c: inherited_point_u16(value, "holdOffCentiC", settings, "holdOffCentiC", defaults.hold_off_centi_c),
        overshoot_cutoff_centi_c: inherited_point_u16(value, "overshootCutoffCentiC", settings, "overshootCutoffCentiC", defaults.overshoot_cutoff_centi_c),
        hold_kp_permille_per_c: inherited_point_u16(value, "holdKpPermillePerC", settings, "holdKpPermillePerC", defaults.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: inherited_point_u16(value, "holdKiPermillePerCTick", settings, "holdKiPermillePerCTick", defaults.hold_ki_permille_per_c_tick),
        hold_blend_ticks: inherited_point_u16(value, "holdBlendTicks", settings, "holdBlendTicks", defaults.hold_blend_ticks),
        approach_lead_ticks: inherited_point_u16(value, "approachLeadTicks", settings, "approachLeadTicks", defaults.approach_lead_ticks),
        hold_lead_ticks: inherited_point_u16(value, "holdLeadTicks", settings, "holdLeadTicks", defaults.hold_lead_ticks),
    }
}

fn value_u16(value: &Value, key: &str) -> Option<u16> {
    value.get(key).and_then(Value::as_u64).and_then(|value| u16::try_from(value).ok())
}

fn value_i16(value: &Value, key: &str) -> Option<i16> {
    value.get(key).and_then(Value::as_i64).and_then(|value| i16::try_from(value).ok())
}

fn inherited_point_u16(
    point_value: &Value,
    point_key: &str,
    legacy_settings: &Value,
    legacy_key: &str,
    default_value: u16,
) -> u16 {
    if let Some(value) = point_value
        .get(point_key)
        .and_then(Value::as_u64)
        .map(|value| value as u16)
        && (value != 0 || legacy_settings.get(legacy_key).is_none())
    {
        return value;
    }
    legacy_settings
        .get(legacy_key)
        .and_then(Value::as_u64)
        .map(|value| value as u16)
        .unwrap_or(default_value)
}

fn thermal_candidate_settings_to_value(settings: ThermalCandidateSettings) -> Value {
    json!({
        "tempFilterAlphaPermille": settings.temp_filter_alpha_permille,
        "approachMaxTicks": settings.approach_max_ticks,
        "approachMinPowerRatioPermille": settings.approach_min_power_ratio_permille,
        "autoAdjustableWorkingFloorMv": settings.auto_adjustable_working_floor_mv,
        "heaterCurrentReserveMa": settings.heater_current_reserve_ma,
    })
}

fn effective_thermal_sample_interval_ms(requested_ms: u64) -> u64 {
    requested_ms.clamp(1, 300)
}

const THERMAL_MIN_SAMPLE_RATE_HZ: f64 = 3.0;
const THERMAL_SAMPLE_RATE_WINDOW_MS: u64 = 3_000;
const THERMAL_SAMPLE_RATE_FAILURE_GRACE_MS: u64 = 3_000;
const THERMAL_MEASUREMENT_GUARD_FAILURE_GRACE_MS: u64 = 2_000;
const THERMAL_HEATER_OUTPUT_START_TIMEOUT_MS: u64 = 2_000;
const THERMAL_COOLDOWN_POLL_INTERVAL_MS: u64 = 1_000;
const THERMAL_COOLDOWN_EPSILON_C: f64 = 0.15;

#[derive(Debug, Clone, Default)]
struct ThermalMeasurementGuardTracker {
    guarded_since_ms: Option<u64>,
}

impl ThermalMeasurementGuardTracker {
    fn observe(&mut self, measurement_guarded: bool, elapsed_ms: u64) -> bool {
        if !measurement_guarded {
            self.guarded_since_ms = None;
            return false;
        }
        let guarded_since_ms = *self.guarded_since_ms.get_or_insert(elapsed_ms);
        elapsed_ms.saturating_sub(guarded_since_ms) >= THERMAL_MEASUREMENT_GUARD_FAILURE_GRACE_MS
    }
}

impl ThermalSampleRateTracker {
    fn new() -> Self {
        Self {
            elapsed_ms: Vec::with_capacity(32),
            below_minimum_since_ms: None,
        }
    }

    fn observe(&mut self, elapsed_ms: u64) -> ThermalSampleRateObservation {
        let interval_ms = self
            .elapsed_ms
            .last()
            .map(|previous| elapsed_ms.saturating_sub(*previous));
        self.elapsed_ms.push(elapsed_ms);
        let cutoff_ms = elapsed_ms.saturating_sub(THERMAL_SAMPLE_RATE_WINDOW_MS);
        self.elapsed_ms.retain(|sample_ms| *sample_ms >= cutoff_ms);
        let rolling_rate_hz = if elapsed_ms >= THERMAL_SAMPLE_RATE_WINDOW_MS {
            Some(
                self.elapsed_ms.len().saturating_sub(1) as f64 * 1_000.0
                    / THERMAL_SAMPLE_RATE_WINDOW_MS as f64,
            )
        } else {
            None
        };
        let below_minimum =
            rolling_rate_hz.is_some_and(|rate_hz| rate_hz < THERMAL_MIN_SAMPLE_RATE_HZ);
        if below_minimum {
            self.below_minimum_since_ms.get_or_insert(elapsed_ms);
        } else {
            self.below_minimum_since_ms = None;
        }
        let violation = self.below_minimum_since_ms.is_some_and(|started_at_ms| {
            elapsed_ms.saturating_sub(started_at_ms) >= THERMAL_SAMPLE_RATE_FAILURE_GRACE_MS
        });
        ThermalSampleRateObservation {
            interval_ms,
            rolling_rate_hz,
            violation,
        }
    }
}

type ThermalDefaultTargetValues = (
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
);

fn thermal_default_target_values(target_temp_c: i16) -> ThermalDefaultTargetValues {
    if target_temp_c <= 60 {
        (
            // The verified low-temperature point predicts stored heat during Approach, enters
            // Hold early, then coasts at zero output until the plate starts falling again.
            1_310, 1_000, 590, 510, 1_320, 60, 60, 1_000, 200, 540, 30, 120, 150, 8, 2, 1, 4, 2,
        )
    } else if target_temp_c <= 100 {
        (
            1_100, 1_000, 420, 220, 1_400, 170, 260, 1_000, 12, 60, 30, 180, 230, 55, 2, 6, 9, 0,
        )
    } else if target_temp_c <= 140 {
        (
            1_000, 1_000, 420, 200, 1_000, 280, 340, 1_000, 10, 55, 30, 160, 220, 22, 1, 1, 4, 0,
        )
    } else if target_temp_c <= 180 {
        (
            650, 1_000, 760, 460, 800, 450, 620, 1_000, 15, 70, 25, 240, 300, 20, 1, 3, 4, 0,
        )
    } else if target_temp_c <= 220 {
        (
            520, 1_000, 760, 600, 550, 620, 700, 1_000, 8, 50, 14, 240, 320, 22, 1, 2, 2, 0,
        )
    } else {
        (
            500, 1_000, 960, 860, 350, 850, 930, 1_000, 10, 55, 14, 320, 420, 12, 1, 1, 1, 0,
        )
    }
}

fn thermal_default_settings_value() -> Value {
    thermal_candidate_settings_to_value(thermal_default_settings())
}

fn resolve_optimization_targets(
    requested_targets_c: &[i16],
    optimize_targets_c: Option<&str>,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(optimize_targets_c) = optimize_targets_c {
        return parse_thermal_targets(Some(optimize_targets_c));
    }
    if requested_targets_c.len() <= 3 {
        return Ok(requested_targets_c.to_vec());
    }
    let min_target = requested_targets_c[0];
    let max_target = *requested_targets_c.last().unwrap_or(&min_target);
    let midpoint = (f64::from(min_target) + f64::from(max_target)) / 2.0;
    let middle_target = requested_targets_c[1..requested_targets_c.len() - 1]
        .iter()
        .copied()
        .min_by(|left, right| {
            (f64::from(*left) - midpoint)
                .abs()
                .partial_cmp(&(f64::from(*right) - midpoint).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(min_target);
    Ok(vec![min_target, middle_target, max_target])
}

fn thermal_candidate_point(
    profile: &ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<ThermalCandidatePoint> {
    profile
        .points
        .iter()
        .copied()
        .find(|point| point.target_temp_c == target_temp_c)
}

fn thermal_interpolated_candidate_point(
    profile: &ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<ThermalCandidatePoint> {
    if let Some(point) = thermal_candidate_point(profile, target_temp_c) {
        return Some(point);
    }
    let mut points = profile.points.clone();
    points.sort_by_key(|point| point.target_temp_c);
    let lower = points
        .iter()
        .copied()
        .rev()
        .find(|point| point.target_temp_c < target_temp_c)?;
    let upper = points
        .iter()
        .copied()
        .find(|point| point.target_temp_c > target_temp_c)?;
    Some(thermal_interpolate_candidate_point(target_temp_c, lower, upper))
}

fn thermal_interpolate_candidate_point(
    target_temp_c: i16,
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
) -> ThermalCandidatePoint {
    let ratio = f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c);
    let lerp = |left: u16, right: u16, upper_bound: u16| {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let (interpolated_brake_distance, low_temp_hold_scale, low_temp_reheat_scale) =
        thermal_interpolation_adjustments(lower, upper, ratio, linear_brake_distance);
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    let default_point = thermal_default_target_point(target_temp_c);
    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: interpolated_brake_distance,
        warmup_power_permille: 1_000,
        approach_power_permille: lerp(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent_permille: lerp(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            4_000,
        ),
        approach_tail_window_centi_c: lerp(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            5_000,
        ),
        hold_power_permille: scale_low_temp_hold(lerp(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        warmup_reenter_centi_c: lerp(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            5_000,
        )
        .max(default_point.warmup_reenter_centi_c.min(5_000)),
        hold_entry_centi_c: lerp(lower.hold_entry_centi_c, upper.hold_entry_centi_c, 5_000),
        hold_exit_centi_c: lerp(lower.hold_exit_centi_c, upper.hold_exit_centi_c, 5_000),
        hold_on_centi_c: lerp(lower.hold_on_centi_c, upper.hold_on_centi_c, 5_000),
        hold_off_centi_c: lerp(lower.hold_off_centi_c, upper.hold_off_centi_c, 5_000),
        overshoot_cutoff_centi_c: lerp(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        ),
        hold_kp_permille_per_c: lerp(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        ),
        hold_ki_permille_per_c_tick: {
            let interpolated = lerp(
                lower.hold_ki_permille_per_c_tick,
                upper.hold_ki_permille_per_c_tick,
                10_000,
            );
            if interpolated == 0 {
                default_point.hold_ki_permille_per_c_tick
            } else {
                interpolated
            }
        },
        hold_blend_ticks: lerp(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)),
        approach_lead_ticks: lerp(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        ),
        hold_lead_ticks: lerp(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        ),
    }
}

fn thermal_interpolation_adjustments(
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
    ratio: f32,
    linear_brake_distance: u16,
) -> (u16, f32, f32) {
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 { 0.55 } else { 0.20 }
    } else {
        0.0
    };
    let brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_band = lower.target_temp_c >= 60 && upper.target_temp_c <= 100;
    let hold_scale = if low_temp_band {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    };
    let reheat_scale = if low_temp_band {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    };
    (brake_distance, hold_scale, reheat_scale)
}

fn thermal_candidate_point_mut(
    profile: &mut ThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<&mut ThermalCandidatePoint> {
    profile
        .points
        .iter_mut()
        .find(|point| point.target_temp_c == target_temp_c)
}

fn thermal_rebuild_profile_from_anchor_targets(
    profile: &mut ThermalCandidateProfile,
    anchor_targets_c: &[i16],
) {
    let mut anchors = anchor_targets_c
        .iter()
        .filter_map(|target_temp_c| thermal_candidate_point(profile, *target_temp_c))
        .collect::<Vec<_>>();
    anchors.sort_by_key(|point| point.target_temp_c);
    if anchors.is_empty() {
        return;
    }
    for point in &mut profile.points {
        if anchors
            .iter()
            .any(|anchor| anchor.target_temp_c == point.target_temp_c)
        {
            continue;
        }
        if point.target_temp_c < anchors[0].target_temp_c
            || point.target_temp_c > anchors[anchors.len() - 1].target_temp_c
        {
            continue;
        }
        let mut lower = anchors[0];
        let mut upper = *anchors.last().unwrap_or(&anchors[0]);
        for anchor in &anchors {
            if anchor.target_temp_c <= point.target_temp_c {
                lower = *anchor;
            }
            if anchor.target_temp_c >= point.target_temp_c {
                upper = *anchor;
                break;
            }
        }
        *point = rebuild_thermal_candidate_point_from_anchor_relations(
            point.target_temp_c,
            lower,
            upper,
        );
    }
    profile.points.sort_by_key(|point| point.target_temp_c);
}

fn rebuild_thermal_candidate_point_from_anchor_relations(
    target_temp_c: i16,
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
) -> ThermalCandidatePoint {
    if lower.target_temp_c >= upper.target_temp_c {
        return ThermalCandidatePoint {
            target_temp_c,
            ..lower
        };
    }
    thermal_rebuild_interpolated_point(target_temp_c, lower, upper)
}

#[derive(Clone, Copy)]
struct ThermalRebuildPowerValues {
    warmup: u16,
    approach: u16,
    approach_floor: u16,
    hold: u16,
    hold_reheat: u16,
}

#[derive(Clone, Copy)]
struct ThermalRebuildControlValues {
    brake_distance: u16,
    approach_damping: u16,
    approach_tail_window: u16,
    warmup_reenter: u16,
    hold_entry: u16,
    hold_exit: u16,
    hold_on: u16,
    hold_off: u16,
    overshoot_cutoff: u16,
    hold_kp: u16,
    hold_ki: u16,
    hold_blend: u16,
    approach_lead: u16,
    hold_lead: u16,
}

fn thermal_rebuild_interpolated_point(
    target_temp_c: i16,
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
) -> ThermalCandidatePoint {
    let ratio = (f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c))
    .clamp(0.0, 1.0);
    let defaults = thermal_default_target_point(target_temp_c);
    let lower_defaults = thermal_default_target_point(lower.target_temp_c);
    let upper_defaults = thermal_default_target_point(upper.target_temp_c);
    let power = thermal_rebuild_power_values(lower, upper, lower_defaults, upper_defaults, defaults, ratio);
    let control = thermal_rebuild_control_values(lower, upper, lower_defaults, upper_defaults, defaults, ratio);
    ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: control.brake_distance,
        warmup_power_permille: power.warmup,
        approach_power_permille: power.approach,
        approach_floor_power_permille: power.approach_floor,
        approach_damping_exponent_permille: control.approach_damping,
        approach_tail_window_centi_c: control.approach_tail_window,
        hold_power_permille: power.hold,
        hold_reheat_power_permille: power.hold_reheat,
        warmup_reenter_centi_c: control.warmup_reenter,
        hold_entry_centi_c: control.hold_entry,
        hold_exit_centi_c: control.hold_exit,
        hold_on_centi_c: control.hold_on,
        hold_off_centi_c: control.hold_off,
        overshoot_cutoff_centi_c: control.overshoot_cutoff,
        hold_kp_permille_per_c: control.hold_kp,
        hold_ki_permille_per_c_tick: control.hold_ki,
        hold_blend_ticks: control.hold_blend,
        approach_lead_ticks: control.approach_lead,
        hold_lead_ticks: control.hold_lead,
    }
}

fn thermal_rebuild_power_values(
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
    lower_defaults: ThermalCandidatePoint,
    upper_defaults: ThermalCandidatePoint,
    defaults: ThermalCandidatePoint,
    ratio: f32,
) -> ThermalRebuildPowerValues {
    let hold = scale_power_from_defaults(defaults.hold_power_permille, lower.hold_power_permille, lower_defaults.hold_power_permille, upper.hold_power_permille, upper_defaults.hold_power_permille, ratio);
    let approach_floor = scale_power_from_defaults(defaults.approach_floor_power_permille, lower.approach_floor_power_permille, lower_defaults.approach_floor_power_permille, upper.approach_floor_power_permille, upper_defaults.approach_floor_power_permille, ratio).max(hold).min(1_000);
    let default_gap = defaults.approach_power_permille.saturating_sub(defaults.approach_floor_power_permille);
    let approach = scale_power_from_defaults(defaults.approach_power_permille, lower.approach_power_permille, lower_defaults.approach_power_permille, upper.approach_power_permille, upper_defaults.approach_power_permille, ratio).max(approach_floor.saturating_add((default_gap / 3).max(10))).min(1_000);
    let hold_reheat = scale_power_from_defaults(defaults.hold_reheat_power_permille, lower.hold_reheat_power_permille, lower_defaults.hold_reheat_power_permille, upper.hold_reheat_power_permille, upper_defaults.hold_reheat_power_permille, ratio).max(hold).max(approach_floor);
    ThermalRebuildPowerValues { warmup: 1_000, approach, approach_floor, hold, hold_reheat }
}

fn thermal_rebuild_control_values(
    lower: ThermalCandidatePoint,
    upper: ThermalCandidatePoint,
    lower_defaults: ThermalCandidatePoint,
    upper_defaults: ThermalCandidatePoint,
    defaults: ThermalCandidatePoint,
    ratio: f32,
) -> ThermalRebuildControlValues {
    let shift = |target, lower_value, lower_default, upper_value, upper_default, max| shift_from_defaults(target, lower_value, lower_default, upper_value, upper_default, max, ratio);
    ThermalRebuildControlValues {
        brake_distance: shift(defaults.brake_distance_centi_c, lower.brake_distance_centi_c, lower_defaults.brake_distance_centi_c, upper.brake_distance_centi_c, upper_defaults.brake_distance_centi_c, 5_000),
        approach_damping: shift(defaults.approach_damping_exponent_permille, lower.approach_damping_exponent_permille, lower_defaults.approach_damping_exponent_permille, upper.approach_damping_exponent_permille, upper_defaults.approach_damping_exponent_permille, 4_000),
        approach_tail_window: shift(defaults.approach_tail_window_centi_c, lower.approach_tail_window_centi_c, lower_defaults.approach_tail_window_centi_c, upper.approach_tail_window_centi_c, upper_defaults.approach_tail_window_centi_c, 5_000),
        warmup_reenter: shift(defaults.warmup_reenter_centi_c, lower.warmup_reenter_centi_c, lower_defaults.warmup_reenter_centi_c, upper.warmup_reenter_centi_c, upper_defaults.warmup_reenter_centi_c, 5_000),
        hold_entry: shift(defaults.hold_entry_centi_c, lower.hold_entry_centi_c, lower_defaults.hold_entry_centi_c, upper.hold_entry_centi_c, upper_defaults.hold_entry_centi_c, 5_000),
        hold_exit: shift(defaults.hold_exit_centi_c, lower.hold_exit_centi_c, lower_defaults.hold_exit_centi_c, upper.hold_exit_centi_c, upper_defaults.hold_exit_centi_c, 5_000),
        hold_on: shift(defaults.hold_on_centi_c, lower.hold_on_centi_c, lower_defaults.hold_on_centi_c, upper.hold_on_centi_c, upper_defaults.hold_on_centi_c, 5_000),
        hold_off: shift(defaults.hold_off_centi_c, lower.hold_off_centi_c, lower_defaults.hold_off_centi_c, upper.hold_off_centi_c, upper_defaults.hold_off_centi_c, 5_000),
        overshoot_cutoff: shift(defaults.overshoot_cutoff_centi_c, lower.overshoot_cutoff_centi_c, lower_defaults.overshoot_cutoff_centi_c, upper.overshoot_cutoff_centi_c, upper_defaults.overshoot_cutoff_centi_c, 5_000),
        hold_kp: shift(defaults.hold_kp_permille_per_c, lower.hold_kp_permille_per_c, lower_defaults.hold_kp_permille_per_c, upper.hold_kp_permille_per_c, upper_defaults.hold_kp_permille_per_c, 10_000),
        hold_ki: shift(defaults.hold_ki_permille_per_c_tick, lower.hold_ki_permille_per_c_tick, lower_defaults.hold_ki_permille_per_c_tick, upper.hold_ki_permille_per_c_tick, upper_defaults.hold_ki_permille_per_c_tick, 10_000),
        hold_blend: shift(defaults.hold_blend_ticks, lower.hold_blend_ticks, lower_defaults.hold_blend_ticks, upper.hold_blend_ticks, upper_defaults.hold_blend_ticks, u16::from(u8::MAX)),
        approach_lead: shift(defaults.approach_lead_ticks, lower.approach_lead_ticks, lower_defaults.approach_lead_ticks, upper.approach_lead_ticks, upper_defaults.approach_lead_ticks, u16::from(u8::MAX)),
        hold_lead: shift(defaults.hold_lead_ticks, lower.hold_lead_ticks, lower_defaults.hold_lead_ticks, upper.hold_lead_ticks, upper_defaults.hold_lead_ticks, u16::from(u8::MAX)),
    }
}

fn scale_power_from_defaults(
    target_default: u16,
    lower_value: u16,
    lower_default: u16,
    upper_value: u16,
    upper_default: u16,
    ratio: f32,
) -> u16 {
    let lower_scale = if lower_default == 0 { 1.0 } else { lower_value as f32 / lower_default as f32 };
    let upper_scale = if upper_default == 0 { 1.0 } else { upper_value as f32 / upper_default as f32 };
    ((target_default as f32) * (lower_scale + ((upper_scale - lower_scale) * ratio)) + 0.5).clamp(0.0, 1_000.0) as u16
}

fn shift_from_defaults(
    target_default: u16,
    lower_value: u16,
    lower_default: u16,
    upper_value: u16,
    upper_default: u16,
    max: u16,
    ratio: f32,
) -> u16 {
    let lower_delta = i32::from(lower_value) - i32::from(lower_default);
    let upper_delta = i32::from(upper_value) - i32::from(upper_default);
    ((target_default as f32) + (lower_delta as f32 + ((upper_delta - lower_delta) as f32 * ratio)) + 0.5).clamp(0.0, f32::from(max)) as u16
}

#[derive(Clone)]
struct ThermalTuneMetrics {
    settle_limit_ms: u64,
    full_speed_failed: bool,
    overshoot_c: f64,
    residual_c: f64,
    under_c: f64,
    over_c: f64,
    hold_p2p_c: f64,
    equilibrium: u16,
    hold_p90: u16,
    near_target_power: u16,
    curve_needs_tuning: bool,
    entering_below_target: f64,
    entering_above_target: f64,
    hold_gate_lag: bool,
    approach_only_underpowered: bool,
    high_temp_power_limited: bool,
    stability_overshoot: bool,
    entry_residual_dominant: bool,
    overshoot_dominant: bool,
    timely_hold_but_late_stability: bool,
    low_temp_hold_entry_carry: bool,
    hold_ripple: bool,
    bursty_low_temp_hold: bool,
}

impl ThermalTuneMetrics {
    fn from_result(previous: ThermalCandidatePoint, result: &ThermalStageResult) -> Self {
        let analysis = &result.analysis;
        let settle_limit_ms = ThermalFullSpeedStableTracker::settle_limit_ms_for_target(result.target_temp_c);
        let full_speed_failed = result.stop_reason != "completed"
            || result.full_speed_to_stable.settle_time_ms.is_some_and(|value| value > settle_limit_ms)
            || result.full_speed_to_stable.failure_reason.is_some();
        let overshoot_c = result.max_overshoot_c.max(0.0);
        let residual_c = analysis.residual_heat_after_hold_entry_c.unwrap_or(overshoot_c).max(0.0);
        let under_c = analysis.hold_max_below_target_c.unwrap_or(0.0).max(0.0);
        let over_c = analysis.hold_max_above_target_c.unwrap_or(0.0).max(0.0);
        let hold_p2p_c = result.hold_peak_to_peak_c.max(0.0);
        let equilibrium = analysis.hold_median_output_permille.unwrap_or(previous.hold_power_permille);
        let hold_p90 = analysis.hold_p90_output_permille.unwrap_or(previous.hold_reheat_power_permille.max(equilibrium));
        let near_target_power = analysis.approach_median_output_permille.unwrap_or(previous.approach_floor_power_permille.max(equilibrium));
        let curve_class = analysis.approach_curve_deviation_class;
        let curve_needs_tuning = matches!(curve_class, Some("brake_late_or_residual" | "underpowered_or_early_coast" | "oscillatory_near_target"));
        let curve_overshoot = curve_class == Some("brake_late_or_residual");
        let curve_underpowered = curve_class == Some("underpowered_or_early_coast");
        let curve_oscillation = curve_class == Some("oscillatory_near_target");
        let entering_below_target = analysis.first_hold_error_c.unwrap_or(0.0).max(0.0);
        let entering_above_target = (-analysis.first_hold_error_c.unwrap_or(0.0)).max(0.0);
        let hold_gate_lag = full_speed_failed && analysis.hold_sample_count == 0 && result.guard.hold_threshold_crossed_at_ms.is_some() && !curve_underpowered;
        let approach_only_underpowered = (full_speed_failed && analysis.hold_sample_count == 0 && !hold_gate_lag) || curve_underpowered;
        let high_temp_power_limited = approach_only_underpowered && result.target_temp_c >= 180 && near_target_power >= 900 && analysis.approach_median_slope_c_per_s.is_some_and(|slope| slope <= 1.0);
        let starved_low_temp_hold = full_speed_failed && result.target_temp_c <= 120 && analysis.hold_sample_count > 0 && hold_p90 == 0 && analysis.hold_mean_error_c.is_some_and(|error| error > 0.2) && under_c > over_c + 0.4;
        let stability_overshoot = full_speed_failed && analysis.hold_sample_count > 0 && over_c > ThermalFullSpeedStableTracker::STABLE_BAND_C && (result.target_temp_c <= 120 || over_c >= under_c) && !starved_low_temp_hold;
        let entry_residual_dominant = analysis.first_hold_error_c.is_some() && residual_c >= 1.5 && over_c >= ThermalFullSpeedStableTracker::STABLE_BAND_C && over_c >= under_c + 0.4;
        let overshoot_dominant = curve_overshoot || overshoot_c > 3.0 || stability_overshoot || entry_residual_dominant || (residual_c > 2.5 && entering_above_target > 0.5 && over_c > under_c);
        let timely_hold_but_late_stability = thermal_timely_hold_late_stability(result, settle_limit_ms, under_c, over_c, full_speed_failed);
        let low_temp_hold_entry_carry = full_speed_failed && result.target_temp_c <= 120 && analysis.hold_sample_count > 0 && analysis.hold_p90_output_permille.is_some_and(|output| output > previous.hold_power_permille) && hold_p2p_c > ThermalFullSpeedStableTracker::STABLE_BAND_C + 0.5 && entering_below_target >= 0.4 && residual_c >= ThermalFullSpeedStableTracker::STABLE_BAND_C && over_c > ThermalFullSpeedStableTracker::STABLE_BAND_C;
        let hold_ripple = curve_oscillation || (analysis.hold_sample_count > 0 && hold_p2p_c > 3.0);
        let bursty_low_temp_hold = result.target_temp_c <= 120 && analysis.hold_median_output_permille == Some(0) && hold_p90 >= previous.hold_power_permille.saturating_add(20) && under_c > over_c + 0.4 && analysis.hold_mean_error_c.is_some_and(|error| error > 0.2);
        Self { settle_limit_ms, full_speed_failed, overshoot_c, residual_c, under_c, over_c, hold_p2p_c, equilibrium, hold_p90, near_target_power, curve_needs_tuning, entering_below_target, entering_above_target, hold_gate_lag, approach_only_underpowered, high_temp_power_limited, stability_overshoot, entry_residual_dominant, overshoot_dominant, timely_hold_but_late_stability, low_temp_hold_entry_carry, hold_ripple, bursty_low_temp_hold }
    }
}

fn thermal_timely_hold_late_stability(
    result: &ThermalStageResult,
    settle_limit_ms: u64,
    under_c: f64,
    over_c: f64,
    full_speed_failed: bool,
) -> bool {
    full_speed_failed
        && result.guard.first_hold_at_ms.zip(result.full_speed_to_stable.warmup_exited_at_ms).is_some_and(|(first, warmup)| first.saturating_sub(warmup) <= settle_limit_ms)
        && result.full_speed_to_stable.stable_window_started_at_ms.zip(result.full_speed_to_stable.warmup_exited_at_ms).is_some_and(|(started, warmup)| started.saturating_sub(warmup) > settle_limit_ms)
        && under_c <= ThermalFullSpeedStableTracker::STABLE_BAND_C
        && over_c <= ThermalFullSpeedStableTracker::STABLE_BAND_C
}

fn tune_thermal_candidate_point(
    previous: ThermalCandidatePoint,
    result: &ThermalStageResult,
) -> ThermalCandidatePoint {
    if !thermal_stage_can_tune(result) {
        return previous;
    }
    let analysis = &result.analysis;
    let metrics = ThermalTuneMetrics::from_result(previous, result);
    let ThermalTuneMetrics {
        full_speed_failed,
        overshoot_c,
        residual_c,
        under_c,
        hold_p2p_c,
        near_target_power,
        curve_needs_tuning,
        hold_gate_lag,
        approach_only_underpowered,
        high_temp_power_limited,
        overshoot_dominant,
        timely_hold_but_late_stability,
        low_temp_hold_entry_carry,
        hold_ripple,
        ..
    } = metrics.clone();
    let mut tuned = previous;

    if !full_speed_failed && overshoot_c <= 3.0 && hold_p2p_c <= 3.0 && !curve_needs_tuning {
        return previous;
    }

    if high_temp_power_limited {
        apply_high_temp_power_limit(&mut tuned, near_target_power);
    } else if timely_hold_but_late_stability {
        apply_timely_hold_late_stability(&mut tuned, under_c);
    } else if low_temp_hold_entry_carry {
        apply_low_temp_hold_entry(&mut tuned, residual_c);
    } else if hold_gate_lag {
        apply_hold_gate_lag(&mut tuned, result.target_temp_c);
    } else if approach_only_underpowered {
        apply_approach_underpowered(&mut tuned, result.target_temp_c, near_target_power);
    } else if overshoot_dominant {
        apply_overshoot_tuning(&mut tuned, previous, result, analysis, &metrics);
    } else if hold_ripple {
        apply_hold_ripple_tuning(&mut tuned, previous, result, analysis, &metrics);
    } else {
        return previous;
    }

    tuned.hold_reheat_power_permille = tuned
        .hold_reheat_power_permille
        .max(tuned.hold_power_permille);
    tuned.approach_power_permille = tuned
        .approach_power_permille
        .max(tuned.approach_floor_power_permille)
        .min(1_000);
    tuned.warmup_power_permille = tuned
        .warmup_power_permille
        .max(tuned.approach_power_permille)
        .min(1_000);
    tuned
}

fn apply_overshoot_brake(
    tuned: &mut ThermalCandidatePoint,
    result: &ThermalStageResult,
    analysis: &ThermalStageAnalysis,
    metrics: &ThermalTuneMetrics,
) -> (bool, bool) {
    let bounded_low_temp_entry_residual = result.target_temp_c <= 120
        && metrics.stability_overshoot
        && metrics.residual_c <= 3.5
        && analysis.hold_sample_count > 0
        && result
            .guard
            .first_hold_at_ms
            .zip(result.full_speed_to_stable.warmup_exited_at_ms)
            .is_some_and(|(first, warmup)| first.saturating_sub(warmup) <= metrics.settle_limit_ms);
    let coast_gate_limited = metrics.stability_overshoot
        && result.target_temp_c <= 120
        && metrics.residual_c <= 3.5
        && analysis.hold_sample_count > 0;
    let brake_step = if bounded_low_temp_entry_residual {
        80
    } else if coast_gate_limited {
        0
    } else {
        ((metrics.residual_c * 100.0) + (metrics.overshoot_c * 70.0))
            .round()
            .clamp(if metrics.stability_overshoot { 100.0 } else { 80.0 }, 350.0) as u16
    };
    tuned.brake_distance_centi_c = tuned.brake_distance_centi_c.saturating_add(brake_step).clamp(100, 5_000);
    let damping_step = if bounded_low_temp_entry_residual { 50 } else if metrics.residual_c > 3.0 { 200 } else { 100 };
    tuned.approach_damping_exponent_permille = tuned.approach_damping_exponent_permille.saturating_add(damping_step).clamp(100, 4_000);
    (bounded_low_temp_entry_residual, coast_gate_limited)
}

fn apply_overshoot_tuning(
    tuned: &mut ThermalCandidatePoint,
    previous: ThermalCandidatePoint,
    result: &ThermalStageResult,
    analysis: &ThermalStageAnalysis,
    metrics: &ThermalTuneMetrics,
) {
        let (bounded_low_temp_entry_residual, coast_gate_limited) =
            apply_overshoot_brake(tuned, result, analysis, metrics);
        apply_overshoot_entry_adjustment(tuned, result, metrics, bounded_low_temp_entry_residual);
        if coast_gate_limited && !metrics.entry_residual_dominant && !bounded_low_temp_entry_residual {
            let coast_step = ((metrics.residual_c - 0.8).max(metrics.overshoot_c - 1.5) * 100.0)
                .round()
                .clamp(50.0, 250.0) as u16;
            tuned.hold_exit_centi_c = tuned
                .hold_exit_centi_c
                .saturating_add(coast_step)
                .min(tuned.brake_distance_centi_c.saturating_sub(10))
                .clamp(tuned.hold_entry_centi_c, 5_000);
        }
        tuned.approach_floor_power_permille = tuned
            .approach_floor_power_permille
            .max(metrics.equilibrium.saturating_sub(30));
        let equilibrium_observation_ready = !metrics.full_speed_failed || analysis.hold_sample_count >= 120;
        let equilibrium_was_observed = equilibrium_observation_ready
            && (analysis
                .hold_max_below_target_c
                .is_some_and(|below_target_c| below_target_c >= 0.3)
                || analysis
                    .hold_mean_error_c
                    .is_some_and(|mean_error_c| mean_error_c.abs() <= 1.0));
        let coasted_through_hold = equilibrium_observation_ready
            && analysis
                .hold_max_below_target_c
                .is_some_and(|below_target_c| below_target_c <= 0.1)
            && analysis
                .hold_mean_error_c
                .is_some_and(|mean_error_c| mean_error_c < -1.5)
            && analysis
                .hold_median_output_permille
                .is_some_and(|output| output <= previous.hold_power_permille);
        if coasted_through_hold {
            let floor_step = if metrics.residual_c > 5.0 { 100 } else { 60 };
            tuned.approach_floor_power_permille = tuned
                .approach_floor_power_permille
                .saturating_sub(floor_step)
                .max(tuned.hold_power_permille);
            tuned.hold_reheat_power_permille = tuned
                .hold_reheat_power_permille
                .saturating_sub(floor_step)
                .max(tuned.hold_power_permille);
        }
        if equilibrium_was_observed {
            tuned.hold_reheat_power_permille = tuned
                .hold_reheat_power_permille
                .saturating_sub(100)
                .max(tuned.hold_power_permille);
            tuned.hold_power_permille = step_toward_u16(
                tuned.hold_power_permille,
                metrics.equilibrium.saturating_sub(30),
                80,
                0,
                1_000,
            );
        }
        if result.target_temp_c <= 120
            && metrics.residual_c > 5.0
            && analysis
                .approach_median_slope_c_per_s
                .is_some_and(|slope_c_per_s| slope_c_per_s > 2.0)
        {
            tuned.warmup_power_permille = tuned
                .warmup_power_permille
                .saturating_sub(250)
                .max(tuned.approach_power_permille);
        }
        tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(3).max(1);
        tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_sub(4).max(8);
}

fn apply_overshoot_entry_adjustment(
    tuned: &mut ThermalCandidatePoint,
    result: &ThermalStageResult,
    metrics: &ThermalTuneMetrics,
    bounded_low_temp_entry_residual: bool,
) {
    if bounded_low_temp_entry_residual {
        let hold_delay_ms = result.guard.first_hold_at_ms.zip(result.full_speed_to_stable.warmup_exited_at_ms).map(|(first, warmup)| first.saturating_sub(warmup)).unwrap_or_default();
        if hold_delay_ms >= 9_000 {
            tuned.hold_entry_centi_c = tuned.hold_entry_centi_c.saturating_add(40).min(250);
        } else {
            let cutoff_step = ((metrics.over_c - ThermalFullSpeedStableTracker::STABLE_BAND_C) * 100.0).round().clamp(20.0, 60.0) as u16;
            tuned.overshoot_cutoff_centi_c = tuned.overshoot_cutoff_centi_c.saturating_sub(cutoff_step).max(50);
            tuned.hold_off_centi_c = tuned.hold_off_centi_c.min(tuned.overshoot_cutoff_centi_c.saturating_sub(40).max(50));
        }
    } else if tuned.approach_lead_ticks < 12 {
        let lead_step = if result.target_temp_c <= 120 && metrics.residual_c > 3.0 { 2 } else { 1 };
        tuned.approach_lead_ticks = tuned.approach_lead_ticks.saturating_add(lead_step).min(12);
    }
}

fn apply_hold_ripple_tuning(
    tuned: &mut ThermalCandidatePoint,
    previous: ThermalCandidatePoint,
    result: &ThermalStageResult,
    analysis: &ThermalStageAnalysis,
    metrics: &ThermalTuneMetrics,
) {
    let hold_ripple_equilibrium = if metrics.bursty_low_temp_hold {
        previous.hold_power_permille.saturating_add(30).max(metrics.hold_p90.saturating_sub(100))
    } else {
        metrics.equilibrium
    };
    let high_temp_entry_carry = result.target_temp_c >= 180 && metrics.entering_below_target >= 0.2 && metrics.residual_c >= 1.6 && metrics.over_c >= 1.0;
    let saturated_high_temp_hold = result.target_temp_c >= 180 && metrics.equilibrium >= 950 && metrics.hold_p90 >= 990 && metrics.over_c >= 1.0 && metrics.under_c >= 1.0;
    let reheat_gap = metrics.hold_p90.saturating_sub(if metrics.bursty_low_temp_hold { hold_ripple_equilibrium } else { metrics.equilibrium });
    let bounded_reheat_gap = if metrics.over_c >= metrics.under_c { (reheat_gap / 2).clamp(40, 100) } else { reheat_gap.clamp(80, 160) };
    tuned.hold_power_permille = step_toward_u16(tuned.hold_power_permille, hold_ripple_equilibrium, 80, 0, 1_000);
    let approach_floor_target = if metrics.under_c > metrics.over_c { previous.approach_floor_power_permille.max(tuned.hold_power_permille.saturating_add(20)) } else { tuned.hold_power_permille.saturating_add(20) };
    tuned.approach_floor_power_permille = tuned.approach_floor_power_permille.max(approach_floor_target).min(1_000);
    tuned.hold_reheat_power_permille = tuned.hold_power_permille.saturating_add(bounded_reheat_gap).max(tuned.approach_floor_power_permille.saturating_add(40)).min(1_000);
    apply_ripple_hold_window(tuned, result.target_temp_c, metrics.under_c);
    if saturated_high_temp_hold {
        let hold_off_c = f64::from(tuned.hold_off_centi_c) / 100.0;
        let cutoff_target_c = ((metrics.over_c - (0.7 * hold_off_c)) / 0.3).max(hold_off_c + 0.4).clamp(1.2, 6.0);
        tuned.overshoot_cutoff_centi_c = tuned.overshoot_cutoff_centi_c.max((cutoff_target_c * 100.0).round() as u16);
        tuned.hold_blend_ticks = tuned.hold_blend_ticks.clamp(1, 4);
    } else if high_temp_entry_carry {
        tuned.hold_on_centi_c = step_toward_u16(tuned.hold_on_centi_c, 140, 60, 20, 250);
        tuned.hold_off_centi_c = tuned.hold_off_centi_c.saturating_sub(30).max(40);
        tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(4).max(1);
        tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_add(6).clamp(8, 10_000);
    } else if metrics.entering_above_target > 0.2 || metrics.over_c > metrics.under_c {
        tuned.hold_on_centi_c = tuned.hold_on_centi_c.saturating_add(30).clamp(20, 250);
        tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max(tuned.hold_on_centi_c).clamp(20, 500);
        tuned.hold_blend_ticks = tuned.hold_blend_ticks.saturating_sub(3).max(1);
        tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_sub(3).max(8);
    } else {
        tuned.hold_kp_permille_per_c = tuned.hold_kp_permille_per_c.saturating_add(3).clamp(8, 10_000);
    }
    let _ = analysis;
}

fn apply_ripple_hold_window(tuned: &mut ThermalCandidatePoint, target_temp_c: i16, under_c: f64) {
    if under_c <= 3.0 {
        return;
    }
    tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max((under_c * 100.0 * 0.67).round().clamp(100.0, 300.0) as u16);
    if target_temp_c >= 180 {
        tuned.hold_lead_ticks = tuned.hold_lead_ticks.saturating_add(1).min(8);
    }
}

fn apply_high_temp_power_limit(tuned: &mut ThermalCandidatePoint, near_target_power: u16) {
    let stable_entry_centi_c = (ThermalFullSpeedStableTracker::STABLE_BAND_C * 100.0).round() as u16;
    tuned.hold_entry_centi_c = stable_entry_centi_c;
    tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max(stable_entry_centi_c.saturating_add(10));
    tuned.brake_distance_centi_c = tuned.hold_entry_centi_c.saturating_add(10).clamp(100, 5_000);
    tuned.warmup_power_permille = 1_000;
    tuned.approach_power_permille = 1_000;
    tuned.approach_floor_power_permille = 1_000;
    tuned.approach_damping_exponent_permille = 100;
    tuned.approach_lead_ticks = 0;
    tuned.hold_power_permille = near_target_power.saturating_add(50).clamp(950, 1_000);
    tuned.hold_reheat_power_permille = 1_000;
    tuned.hold_off_centi_c = tuned.hold_off_centi_c.min(80);
    tuned.overshoot_cutoff_centi_c = tuned.overshoot_cutoff_centi_c.min(180).max(tuned.hold_off_centi_c.saturating_add(40));
}

fn apply_timely_hold_late_stability(tuned: &mut ThermalCandidatePoint, under_c: f64) {
    let hold_exit_target = ((under_c + 0.2) * 100.0).round().clamp(80.0, 300.0) as u16;
    tuned.hold_exit_centi_c = tuned.hold_exit_centi_c.max(hold_exit_target);
}

fn apply_low_temp_hold_entry(tuned: &mut ThermalCandidatePoint, residual_c: f64) {
    let previous_hold_lead_ticks = tuned.hold_lead_ticks;
    tuned.hold_lead_ticks = tuned.hold_lead_ticks.saturating_add(if residual_c >= 2.0 { 2 } else { 1 }).min(8);
    if tuned.hold_lead_ticks == previous_hold_lead_ticks {
        tuned.hold_power_permille = tuned.hold_power_permille.saturating_sub(20).max(40);
        tuned.hold_off_centi_c = tuned.hold_off_centi_c.saturating_add(20).min(400);
    }
    tuned.hold_reheat_power_permille = tuned.hold_reheat_power_permille.saturating_sub(30).max(tuned.hold_power_permille);
}

fn apply_hold_gate_lag(tuned: &mut ThermalCandidatePoint, target_temp_c: i16) {
    let lead_step = if target_temp_c >= 120 { 1 } else { 2 };
    tuned.approach_lead_ticks = tuned.approach_lead_ticks.saturating_add(lead_step).min(12);
}

fn apply_approach_underpowered(tuned: &mut ThermalCandidatePoint, target_temp_c: i16, near_target_power: u16) {
    let power_step = if target_temp_c >= 180 { 120 } else { 80 };
    let lead_step = (tuned.approach_lead_ticks / 2).max(2);
    tuned.approach_floor_power_permille = step_toward_u16(tuned.approach_floor_power_permille, near_target_power.saturating_add(power_step).max(tuned.approach_floor_power_permille), power_step, tuned.hold_power_permille, 1_000);
    tuned.approach_power_permille = tuned.approach_power_permille.max(tuned.approach_floor_power_permille.saturating_add(80)).min(1_000);
    tuned.approach_damping_exponent_permille = tuned.approach_damping_exponent_permille.saturating_sub(90).clamp(100, 4_000);
    tuned.approach_lead_ticks = tuned.approach_lead_ticks.saturating_sub(lead_step);
    tuned.brake_distance_centi_c = tuned.brake_distance_centi_c.saturating_sub(if target_temp_c <= 120 { 120 } else { 60 }).max(100);
}

fn step_toward_u16(current: u16, target: u16, max_step: u16, min: u16, max: u16) -> u16 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let bounded_target = target.clamp(min, max);
    if bounded_target > current {
        current
            .saturating_add((bounded_target - current).min(max_step))
            .clamp(min, max)
    } else {
        current
            .saturating_sub((current - bounded_target).min(max_step))
            .clamp(min, max)
    }
}

fn percentile_u16(values: &[u16], percentile: f64) -> Option<u16> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0) + 0.5) as usize;
    sorted.get(index.min(sorted.len() - 1)).copied()
}

fn percentile_f64(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0) + 0.5) as usize;
    sorted.get(index.min(sorted.len() - 1)).copied()
}

fn thermal_heater_parameters_value(
    target_temp_c: i16,
    thermal_profile: Option<&Value>,
    mode: &'static str,
) -> Value {
    let interpolated_profile = thermal_profile
        .cloned()
        .map(thermal_candidate_profile_from_value);
    let effective_settings = interpolated_profile
        .as_ref()
        .map(|profile| profile.settings)
        .unwrap_or_else(thermal_default_settings);
    let interpolated_point = interpolated_profile
        .as_ref()
        .and_then(|profile| thermal_interpolated_candidate_point(profile, target_temp_c))
        .map(thermal_effective_candidate_point);
    let point_value = interpolated_point.map(|point| {
        thermal_candidate_profile_to_value(&ThermalCandidateProfile {
            settings: effective_settings,
            points: vec![point],
        })["points"][0]
            .clone()
    });
    let values = thermal_heater_parameter_values(target_temp_c, point_value.as_ref());
    let settings = thermal_profile
        .and_then(|profile| profile.get("settings"))
        .cloned()
        .unwrap_or_else(thermal_default_settings_value);
    json!({
        "mode": mode,
        "targetTempC": target_temp_c,
        "warmupPowerPermille": values.warmup_power_permille,
        "brakeDistanceCentiC": values.brake_distance_centi_c,
        "approachPowerPermille": values.approach_power_permille,
        "approachFloorPowerPermille": values.approach_floor_power_permille,
        "approachDampingExponentPermille": values.approach_damping_exponent_permille,
        "approachTailWindowCentiC": values.approach_tail_window_centi_c,
        "holdPowerPermille": values.hold_power_permille,
        "holdReheatPowerPermille": values.hold_reheat_power_permille,
        "warmupReenterCentiC": values.warmup_reenter_centi_c,
        "holdEntryCentiC": values.hold_entry_centi_c,
        "holdExitCentiC": values.hold_exit_centi_c,
        "holdOnCentiC": values.hold_on_centi_c,
        "holdOffCentiC": values.hold_off_centi_c,
        "overshootCutoffCentiC": values.overshoot_cutoff_centi_c,
        "holdKpPermillePerC": values.hold_kp_permille_per_c,
        "holdKiPermillePerCTick": values.hold_ki_permille_per_c_tick,
        "holdBlendTicks": values.hold_blend_ticks,
        "approachLeadTicks": values.approach_lead_ticks,
        "holdLeadTicks": values.hold_lead_ticks,
        "settings": settings,
    })
}

struct ThermalHeaterParameterValues {
    warmup_power_permille: u16,
    brake_distance_centi_c: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

fn thermal_heater_parameter_values(
    target_temp_c: i16,
    point: Option<&Value>,
) -> ThermalHeaterParameterValues {
    let defaults = thermal_default_target_values(target_temp_c);
    let get = |key: &str, default: u16| point.and_then(|value| value_u16(value, key)).unwrap_or(default);
    ThermalHeaterParameterValues {
        brake_distance_centi_c: get("brakeDistanceCentiC", defaults.0),
        warmup_power_permille: 1_000,
        approach_power_permille: get("approachPowerPermille", defaults.2),
        approach_floor_power_permille: get("approachFloorPowerPermille", defaults.3),
        approach_damping_exponent_permille: get("approachDampingExponentPermille", defaults.4),
        approach_tail_window_centi_c: get("approachTailWindowCentiC", 0),
        hold_power_permille: get("holdPowerPermille", defaults.5),
        hold_reheat_power_permille: get("holdReheatPowerPermille", defaults.6),
        warmup_reenter_centi_c: get("warmupReenterCentiC", defaults.7),
        hold_entry_centi_c: get("holdEntryCentiC", defaults.8),
        hold_exit_centi_c: get("holdExitCentiC", defaults.9),
        hold_on_centi_c: get("holdOnCentiC", defaults.10),
        hold_off_centi_c: get("holdOffCentiC", defaults.11),
        overshoot_cutoff_centi_c: get("overshootCutoffCentiC", defaults.12),
        hold_kp_permille_per_c: get("holdKpPermillePerC", defaults.13),
        hold_ki_permille_per_c_tick: get("holdKiPermillePerCTick", defaults.14),
        hold_blend_ticks: get("holdBlendTicks", defaults.15),
        approach_lead_ticks: get("approachLeadTicks", defaults.16),
        hold_lead_ticks: get("holdLeadTicks", defaults.17),
    }
}

fn thermal_target_scoped_preview_profile_value(profile: &Value, target_temp_c: i16) -> Value {
    let effective = thermal_heater_parameters_value(target_temp_c, Some(profile), "preview");
    let settings = effective
        .get("settings")
        .cloned()
        .unwrap_or_else(thermal_default_settings_value);
    let mut point = effective.as_object().cloned().unwrap_or_default();
    point.remove("mode");
    point.remove("settings");

    let mut points = vec![Value::Null; THERMAL_CONTROL_PROFILE_MAX_POINTS];
    points[0] = Value::Object(point);
    json!({
        "settings": settings,
        "points": points,
    })
}

fn thermal_effective_candidate_point(mut point: ThermalCandidatePoint) -> ThermalCandidatePoint {
    point.warmup_power_permille = 1_000;
    point
}

impl BenchSourceLiveTelemetry {
    fn to_value(&self) -> Value {
        json!({
            "voltageMv": self.voltage_mv,
            "currentMa": self.current_ma,
            "powerMw": self.power_mw,
            "sampleUptimeMs": self.sample_uptime_ms,
            "status": self.status,
        })
    }
}

fn heater_telemetry_value(
    status: &Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let heater_output_percent = require_status_u64(status, "heaterOutputPercent")?;
    Ok(json!({
        "currentTempC": require_status_f64(status, "currentTempC")?,
        "hotplateVoltageMv": require_status_u64(status, "voltageMv")?,
        "ppsRequestMv": require_status_u16(status, "pdRequestMv")?,
        "ppsContractMv": require_status_u16(status, "pdContractMv")?,
        "heaterEnabled": status
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: heaterEnabled"))?,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": status
            .get("heaterPhysicalOutputPercent")
            .and_then(Value::as_u64)
            .unwrap_or(heater_output_percent),
        "heaterControlIntervalMs": status
            .get("heaterControlIntervalMs")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        "heaterControlCycleMs": status
            .get("heaterControlCycleMs")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        "heaterErrorC": status
            .get("heaterErrorC")
            .and_then(Value::as_f64),
        "heaterControlErrorC": status
            .get("heaterControlErrorC")
            .and_then(Value::as_f64),
        "heaterFilteredTempC": status
            .get("heaterFilteredTempC")
            .and_then(Value::as_f64),
        "heaterFilteredSlopeCPerS": status
            .get("heaterFilteredSlopeCPerS")
            .and_then(Value::as_f64),
        "heaterCoastActive": status
            .get("heaterCoastActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "thermalControl": status
            .get("thermalControl")
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: thermalControl"))?,
    }))
}

struct DryThermalLadderInput<'a> {
    samples_writer: &'a mut BufWriter<File>,
    run_id: &'a str,
    test_phase: &'a str,
    source_voltage_mv: u16,
    source_current_ma: u16,
    thermal_profile: Option<&'a Value>,
    heater_parameter_mode: &'static str,
    target_temps_c: &'a [i16],
    sample_index: &'a mut usize,
}

fn write_dry_thermal_ladder(
    input: DryThermalLadderInput<'_>,
) -> Result<Vec<ThermalStageResult>, Box<dyn std::error::Error + Send + Sync>> {
    let DryThermalLadderInput {
        samples_writer,
        run_id,
        test_phase,
        source_voltage_mv,
        source_current_ma,
        thermal_profile,
        heater_parameter_mode,
        target_temps_c,
        sample_index,
    } = input;
    target_temps_c
        .iter()
        .copied()
        .map(|target_temp_c| {
            write_dry_thermal_stage(DryThermalStageInput {
                samples_writer,
                run_id,
                test_phase,
                source_voltage_mv,
                source_current_ma,
                thermal_profile,
                heater_parameter_mode,
                target_temp_c,
                sample_index,
            })
        })
        .collect()
}

struct DryThermalStageInput<'a> {
    samples_writer: &'a mut BufWriter<File>,
    run_id: &'a str,
    test_phase: &'a str,
    source_voltage_mv: u16,
    source_current_ma: u16,
    thermal_profile: Option<&'a Value>,
    heater_parameter_mode: &'static str,
    target_temp_c: i16,
    sample_index: &'a mut usize,
}

fn write_dry_thermal_stage(
    input: DryThermalStageInput<'_>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let DryThermalStageInput {
        samples_writer,
        run_id,
        test_phase,
        source_voltage_mv,
        source_current_ma,
        thermal_profile,
        heater_parameter_mode,
        target_temp_c,
        sample_index,
    } = input;
    let rise_time_ms = u64::from(target_temp_c as u16) * 980;
    let max_overshoot_c = 1.8;
    let hold_peak_to_peak_c = 1.6;
    let heater_parameters =
        thermal_heater_parameters_value(target_temp_c, thermal_profile, heater_parameter_mode);
    let mut synthetic_stage_samples = Vec::new();
    for phase in ["warmup", "hold"] {
        let sample = dry_thermal_stage_sample(DryThermalStageSampleInput {
            run_id,
            test_phase,
            target_temp_c,
            source_voltage_mv,
            source_current_ma,
            heater_parameters: &heater_parameters,
            sample_index: *sample_index,
            phase,
            rise_time_ms,
            max_overshoot_c,
        });
        writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
        synthetic_stage_samples.push(dry_thermal_replay_sample(
            phase,
            target_temp_c,
            source_voltage_mv,
            rise_time_ms,
            max_overshoot_c,
        ));
        *sample_index = sample_index.saturating_add(1);
    }
    let guard = dry_thermal_guard(target_temp_c, rise_time_ms);
    let full_speed_to_stable = dry_thermal_full_speed_analysis(rise_time_ms);
    let mut analysis = thermal_replay_stage_analysis(&synthetic_stage_samples, target_temp_c);
    thermal_stage_populate_approach_curve_analysis(
        &mut analysis,
        &synthetic_stage_samples,
        target_temp_c,
    );
    analysis.approach_median_output_permille = heater_parameters
        .get("approachFloorPowerPermille")
        .and_then(Value::as_u64)
        .map(|value| value as u16);
    analysis.approach_median_slope_c_per_s = Some(0.35);
    analysis.hold_median_output_permille = heater_parameters
        .get("holdPowerPermille")
        .and_then(Value::as_u64)
        .map(|value| value as u16);
    analysis.hold_p90_output_permille = heater_parameters
        .get("holdReheatPowerPermille")
        .and_then(Value::as_u64)
        .map(|value| value as u16);
    analysis.hold_mean_error_c = Some(0.0);
    analysis.hold_max_above_target_c = Some(max_overshoot_c / 2.0);
    analysis.hold_max_below_target_c = Some(hold_peak_to_peak_c / 2.0);
    Ok(ThermalStageResult {
        target_temp_c,
        rise_time_ms,
        max_overshoot_c,
        hold_peak_to_peak_c,
        sample_count: 2,
        stop_reason: "completed",
        terminal_runtime_drop_reason: None,
        analysis,
        guard,
        full_speed_to_stable,
    })
}

struct DryThermalStageSampleInput<'a> {
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    sample_index: usize,
    phase: &'a str,
    rise_time_ms: u64,
    max_overshoot_c: f64,
}

fn dry_thermal_stage_sample(input: DryThermalStageSampleInput<'_>) -> Value {
    let DryThermalStageSampleInput {
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        sample_index,
        phase,
        rise_time_ms,
        max_overshoot_c,
    } = input;
    let heater_output_percent = if phase == "warmup" { 100 } else { 26 };
    let temp_c = if phase == "warmup" {
        f64::from(target_temp_c) - 0.5
    } else {
        f64::from(target_temp_c) + max_overshoot_c / 2.0
    };
    let elapsed_ms = if phase == "warmup" {
        rise_time_ms.saturating_sub(1_000)
    } else {
        rise_time_ms
    };
    json!({
        "runId": run_id,
        "sampleIndex": sample_index,
        "capturedAtUnixMs": current_unix_millis(),
        "elapsedMs": elapsed_ms,
        "testPhase": test_phase,
        "phase": phase,
        "targetTempC": target_temp_c,
        "source": {
            "mode": "dry_run",
            "requestedVoltageMv": source_voltage_mv,
            "requestedCurrentLimitMa": source_current_ma,
        },
        "sourceTelemetry": {
            "voltageMv": source_voltage_mv,
            "currentMa": 0,
            "powerMw": 0,
            "sampleUptimeMs": 0,
            "status": "dry_run",
        },
        "heaterTelemetry": {
            "currentTempC": temp_c,
            "hotplateVoltageMv": source_voltage_mv,
            "ppsRequestMv": source_voltage_mv,
            "ppsContractMv": source_voltage_mv,
            "heaterEnabled": true,
            "heaterOutputPercent": heater_output_percent,
            "heaterPhysicalOutputPercent": heater_output_percent,
        },
        "heaterParameters": heater_parameters,
        "status": synthetic_thermal_status(
            target_temp_c,
            temp_c,
            source_voltage_mv,
            source_current_ma,
            heater_output_percent,
        ),
    })
}

fn dry_thermal_replay_sample(
    phase: &str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    rise_time_ms: u64,
    max_overshoot_c: f64,
) -> ThermalReplayStageSample {
    let heater_output_percent = if phase == "warmup" { 100 } else { 26 };
    let current_temp_c = if phase == "warmup" {
        f64::from(target_temp_c) - 0.5
    } else {
        f64::from(target_temp_c) + max_overshoot_c / 2.0
    };
    ThermalReplayStageSample {
        elapsed_ms: if phase == "warmup" {
            rise_time_ms.saturating_sub(1_000)
        } else {
            rise_time_ms
        },
        current_temp_c,
        heater_output_percent,
        control_phase: Some(if phase == "warmup" {
            "approach".to_string()
        } else {
            "hold".to_string()
        }),
        control_phase_in_hold: phase == "hold",
        source_voltage_mv: Some(u64::from(source_voltage_mv)),
        source_current_ma: Some(0),
        source_power_mw: Some(0),
    }
}

fn dry_thermal_guard(target_temp_c: i16, rise_time_ms: u64) -> ThermalApproachGuardAnalysis {
    ThermalApproachGuardAnalysis {
        hold_threshold_temp_c: f64::from(target_temp_c) - 0.5,
        approach_started_at_ms: Some(rise_time_ms.saturating_sub(1_000)),
        hold_threshold_crossed_at_ms: Some(rise_time_ms),
        first_hold_at_ms: Some(rise_time_ms),
        warmup_reentered_at_ms: None,
    }
}

fn dry_thermal_full_speed_analysis(rise_time_ms: u64) -> ThermalFullSpeedStableAnalysis {
    ThermalFullSpeedStableAnalysis {
        warmup_exited_at_ms: Some(rise_time_ms.saturating_sub(7_500)),
        stable_window_started_at_ms: Some(rise_time_ms),
        stable_window_verified_at_ms: Some(rise_time_ms),
        settle_time_ms: Some(7_500),
        failure_reason: None,
    }
}

fn synthetic_thermal_status(
    target_temp_c: i16,
    current_temp_c: f64,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_output_percent: u8,
) -> Value {
    json!({
        "mode": "sampling",
        "heaterEnabled": true,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": heater_output_percent,
        "currentTempC": current_temp_c,
        "targetTempC": target_temp_c,
        "voltageMv": source_voltage_mv,
        "currentMa": source_current_ma,
        "boardTempCenti": (current_temp_c * 100.0) as i32,
        "rtdRawAdcMv": 1000,
        "vinRawAdcMv": 1000,
        "pdRequestMv": source_voltage_mv,
        "pdContractMv": source_voltage_mv,
        "pdState": "ready",
        "manualPpsEnabled": false,
        "fanEnabled": true,
        "fanPwmPermille": 500,
    })
}

async fn arm_thermal_self_test_target(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    arm_thermal_self_test_heater(client, resolved, lease_id, true, target_temp_c).await
}

async fn preview_and_prepare_thermal_self_test_target(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    profile_mode: ThermalProfileMode,
    profile: &Value,
    target_temp_c: i16,
    heater_parameters: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let preview_profile = thermal_target_scoped_preview_profile_value(profile, target_temp_c);
    let preview_status = request_thermal_runtime_with_retry(
        client,
        resolved,
        lease_id,
        thermal_profile_preview_runtime_body(profile_mode, preview_profile),
    )
    .await?;
    verify_thermal_profile_mode_readback(&preview_status, profile_mode)?;
    if !require_status_bool(&preview_status, "thermalControlProfilePreview")? {
        return Err("thermal profile preview did not enable preview mode".into());
    }

    let status =
        arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    verify_thermal_control_readback(&status, heater_parameters, "preview")
}

struct ThermalPreviewTargetInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    profile_mode: ThermalProfileMode,
    profile: &'a Value,
    target_temp_c: i16,
    heater_parameters: &'a Value,
    use_legacy_profile: bool,
}

async fn preview_prepare_and_arm_thermal_self_test_target(
    input: ThermalPreviewTargetInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalPreviewTargetInput {
        client,
        resolved,
        lease_id,
        profile_mode,
        profile,
        target_temp_c,
        heater_parameters,
        use_legacy_profile,
    } = input;
    if !use_legacy_profile {
        return arm_thermal_self_test_target(client, resolved, lease_id, target_temp_c).await;
    }
    let mut last_error = None::<String>;
    for attempt in 1..=3 {
        let attempt_result = async {
            preview_and_prepare_thermal_self_test_target(
                client,
                resolved,
                lease_id,
                profile_mode,
                profile,
                target_temp_c,
                heater_parameters,
            )
            .await?;
            let status =
                arm_thermal_self_test_target(client, resolved, lease_id, target_temp_c).await?;
            verify_thermal_profile_mode_readback(&status, profile_mode)?;
            verify_thermal_control_readback(&status, heater_parameters, "preview")?;
            Ok::<Value, Box<dyn std::error::Error + Send + Sync>>(status)
        }
        .await;
        match attempt_result {
            Ok(status) => return Ok(status),
            Err(error)
                if attempt < 3
                    && thermal_preview_activation_retryable_error_message(&error.to_string()) =>
            {
                last_error = Some(error.to_string());
                tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| "thermal preview activation did not complete".to_string())
        .into())
}

fn verify_thermal_profile_mode_readback(
    status: &Value,
    expected_mode: ThermalProfileMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let expected_mode_value = expected_mode.as_str();
    let expected_bank = expected_thermal_profile_mode_bank(status, expected_mode)?;
    if status.get("thermalProfileMode").and_then(Value::as_str) != Some(expected_mode_value) {
        return Err(format!(
            "thermal profile mode readback mismatch: expected {expected_mode_value}, got {}",
            status
                .get("thermalProfileMode")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }
    if status
        .get("thermalProfileResolvedBank")
        .and_then(Value::as_str)
        != Some(expected_bank)
    {
        return Err(format!(
            "thermal profile bank readback mismatch: expected {expected_bank}, got {}",
            status
                .get("thermalProfileResolvedBank")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }
    Ok(())
}

fn verify_thermal_control_readback(
    status: &Value,
    expected: &Value,
    expected_source: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !require_status_bool(status, "thermalControlProfilePreview")? {
        return Err("thermal profile preview was cleared before heater arm".into());
    }
    let actual = status
        .get("thermalControl")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "status missing object: thermalControl",
            )
        })?;
    if actual.get("profileActive").and_then(Value::as_bool) != Some(true) {
        return Err("thermal control readback reports no active profile".into());
    }
    if actual.get("profileCoversTarget").and_then(Value::as_bool) != Some(true) {
        return Err("thermal control profile does not cover the requested target".into());
    }
    if actual.get("profileSource").and_then(Value::as_str) != Some(expected_source) {
        return Err(format!(
            "thermal control profile source mismatch: expected {expected_source}, got {}",
            actual
                .get("profileSource")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
        .into());
    }

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
    const SETTINGS_FIELDS: &[&str] = &[
        "tempFilterAlphaPermille",
        "approachMaxTicks",
        "approachMinPowerRatioPermille",
        "autoAdjustableWorkingFloorMv",
        "heaterCurrentReserveMa",
    ];

    let expected_point = expected.as_object().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "heater parameters must be a JSON object",
        )
    })?;
    for field in POINT_FIELDS {
        verify_thermal_control_readback_field(actual, expected_point, field)?;
    }
    let expected_settings = expected
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "heater parameters missing settings",
            )
        })?;
    for field in SETTINGS_FIELDS {
        verify_thermal_control_readback_field(actual, expected_settings, field)?;
    }
    Ok(())
}

fn verify_thermal_control_readback_field(
    actual: &serde_json::Map<String, Value>,
    expected: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let expected_value = expected.get(field).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("heater parameters missing integer field: {field}"),
        )
    })?;
    let actual_value = actual.get(field).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("thermal control readback missing integer field: {field}"),
        )
    })?;
    let tolerance = thermal_control_readback_field_tolerance(field);
    if actual_value.abs_diff(expected_value) > tolerance {
        return Err(format!(
            "thermal control readback mismatch for {field}: expected {expected_value}, got {actual_value}"
        )
        .into());
    }
    Ok(())
}

fn thermal_control_readback_field_tolerance(field: &str) -> u64 {
    if field != "targetTempC" && field.ends_with("CentiC") {
        1
    } else {
        0
    }
}

fn thermal_preview_activation_retryable_error_message(message: &str) -> bool {
    message.contains("thermal profile preview")
        || message.contains("thermal profile mode readback mismatch")
        || message.contains("thermal profile bank readback mismatch")
        || message.contains("thermal control profile source mismatch")
        || message.contains("thermal control profile does not cover the requested target")
        || message.contains("thermal control readback reports no active profile")
        || message.contains("thermal control readback mismatch")
}

fn thermal_self_test_runtime_body(heater_enabled: bool, target_temp_c: i16) -> Value {
    let mut body = json!({
        "heaterEnabled": heater_enabled,
        "targetTempC": target_temp_c,
    });
    if !heater_enabled {
        body["activeCoolingEnabled"] = json!(true);
    }
    body
}

fn thermal_source_summary_value(
    args: &ThermalSelfTestArgs,
    selection: &ThermalSourceSelection,
    source_power_watts: u16,
    source_voltage_mv: u16,
    source_current_ma: u16,
) -> Value {
    json!({
        "kind": args.source_kind.as_str(),
        "id": args.source_id,
        "deviceId": args.source_id,
        "mode": args.source_mode,
        "capabilityPowerWatts": source_power_watts,
        "selectedMode": args.profile_mode.as_str(),
        "resolvedBank": selection.resolved_bank,
        "detectedSourceClass": selection.detected_source_class,
        "detectedSourceClassBasis": selection.detected_source_class_basis,
        "preset": {
            "voltageMv": source_voltage_mv,
            "currentLimitMa": source_current_ma,
            "ppsEnabled": true,
            "pdFixedEnabled": true,
        },
        "url": args.source_url,
        "voltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
        "currentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
        "usbCPath": if args.source_mode == "manual-forced" { "forced-on" } else { "pd-auto" },
    })
}

fn thermal_source_class(source_voltage_mv: u16, source_current_ma: u16) -> &'static str {
    if source_voltage_mv >= 20_000 && source_current_ma >= 5_000 {
        "pps5a"
    } else {
        "pps3a"
    }
}

fn thermal_self_test_cooldown_runtime_body() -> Value {
    json!({
        "heaterEnabled": false,
        "activeCoolingEnabled": true,
        "thermalControlProfile": {
            "op": "clear_preview"
        }
    })
}

fn thermal_runtime_readback_matches(
    status: &Value,
    heater_enabled: bool,
    target_temp_c: i16,
) -> bool {
    status.get("targetTempC").and_then(Value::as_i64) == Some(i64::from(target_temp_c))
        && status.get("heaterEnabled").and_then(Value::as_bool) == Some(heater_enabled)
        && (heater_enabled
            || status.get("activeCoolingEnabled").and_then(Value::as_bool) == Some(true))
}

async fn wait_for_thermal_runtime_readback(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    initial_status: Value,
    heater_enabled: bool,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let deadline =
        tokio::time::Instant::now() + Duration::from_millis(THERMAL_RUNTIME_READBACK_TIMEOUT_MS);
    let mut status = initial_status;
    loop {
        if thermal_runtime_readback_matches(&status, heater_enabled, target_temp_c) {
            return Ok(status);
        }
        if tokio::time::Instant::now() >= deadline {
            let readback_target = status
                .get("targetTempC")
                .and_then(Value::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string());
            let readback_enabled = status
                .get("heaterEnabled")
                .and_then(Value::as_bool)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string());
            return Err(format!(
                "heater runtime readback did not settle: expected target={target_temp_c} enabled={heater_enabled}, got target={readback_target} enabled={readback_enabled}"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(THERMAL_RUNTIME_READBACK_POLL_MS)).await;
        status = request_thermal_status_with_retry(client, resolved, lease_id).await?;
    }
}

async fn arm_thermal_self_test_heater(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    heater_enabled: bool,
    target_temp_c: i16,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut status = request_thermal_runtime_with_retry(
        client,
        resolved,
        lease_id,
        thermal_self_test_runtime_body(heater_enabled, target_temp_c),
    )
    .await?;
    if heater_enabled && status.get("faultAttentionPending").and_then(Value::as_bool) == Some(true)
    {
        if status
            .get("currentTempC")
            .and_then(Value::as_f64)
            .is_some_and(|temperature_c| temperature_c >= 420.0)
        {
            return Err(
                "heater runtime arm blocked while absolute over-temperature protection is active"
                    .into(),
            );
        }
        let mut acknowledge_body = thermal_self_test_runtime_body(false, target_temp_c);
        acknowledge_body["faultAttentionAcknowledged"] = json!(true);
        let acknowledged_status =
            request_thermal_runtime_with_retry(client, resolved, lease_id, acknowledge_body)
                .await?;
        if acknowledged_status
            .get("faultAttentionPending")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err("heater runtime fault attention acknowledgement did not settle".into());
        }
        status = request_thermal_runtime_with_retry(
            client,
            resolved,
            lease_id,
            thermal_self_test_runtime_body(true, target_temp_c),
        )
        .await?;
    }
    let status = wait_for_thermal_runtime_readback(
        client,
        resolved,
        lease_id,
        status,
        heater_enabled,
        target_temp_c,
    )
    .await?;
    let readback_target = require_status_i32(&status, "targetTempC")?;
    if readback_target != i32::from(target_temp_c) {
        return Err(format!(
            "heater runtime readback target mismatch: expected {target_temp_c}, got {readback_target}"
        )
        .into());
    }
    if status.get("heaterEnabled").and_then(Value::as_bool) != Some(heater_enabled) {
        return Err(format!(
            "heater runtime readback enable mismatch: expected {heater_enabled}, got {}",
            status
                .get("heaterEnabled")
                .and_then(Value::as_bool)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )
        .into());
    }
    if !heater_enabled && status.get("activeCoolingEnabled").and_then(Value::as_bool) != Some(true)
    {
        return Err(format!(
            "heater runtime readback activeCooling mismatch: expected true, got {}",
            status
                .get("activeCoolingEnabled")
                .and_then(Value::as_bool)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )
        .into());
    }
    Ok(status)
}

async fn refresh_thermal_source_sampler_before_stage(
    args: &ThermalSelfTestArgs,
    source_power_watts: u16,
    source_sampler: &mut BenchSourceTelemetrySampler,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match source_sampler.refresh().await {
        Ok(()) => Ok(()),
        Err(error)
            if args.runtime_rearm_attempts > 0
                && thermal_source_probe_transient_error(error.as_ref()) =>
        {
            let recovered = recover_thermal_bench_source_after_stale(
                args.source_kind,
                &args.source_url,
                &args.source_id,
                args.profile_mode,
                source_power_watts,
            )
            .await
            .map_err(|recovery_error| {
                format!(
                    "source telemetry refresh failed before stage: {error}; source recovery failed: {recovery_error}"
                )
            })?;
            *source_sampler =
                BenchSourceTelemetrySampler::new(args.source_kind, &args.source_url, recovered);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

struct ThermalStageInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    samples_writer: &'a mut BufWriter<File>,
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    runtime_profile: &'a Value,
    args: &'a ThermalSelfTestArgs,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    sample_index: &'a mut usize,
    initial_status: Option<Value>,
}

struct ThermalStageState {
    target_temp_c: i16,
    stage_timeout: Duration,
    warmup_timeout: Duration,
    hold_duration: Duration,
    sample_interval: Duration,
    control_target: ThermalCandidatePoint,
    source_power_watts: u16,
    use_point_local_profile: bool,
    started: tokio::time::Instant,
    deadline: tokio::time::Instant,
    next_tick: tokio::time::Instant,
    hold_tracker: ThermalHoldTracker,
    analyzer: ThermalStageAnalyzer,
    approach_guard: ThermalApproachGuardTracker,
    full_speed_tracker: ThermalFullSpeedStableTracker,
    max_temp_c: f64,
    stage_sample_count: usize,
    recorded_samples: Vec<ThermalReplayStageSample>,
    stop_reason: &'static str,
    terminal_runtime_drop_reason: Option<&'static str>,
    last_uptime_seconds: Option<u64>,
    sample_rate_tracker: ThermalSampleRateTracker,
    measurement_guard_tracker: ThermalMeasurementGuardTracker,
    heater_output_seen: bool,
    runtime_rearm_attempts_remaining: u8,
    next_status: Option<Value>,
}

impl ThermalStageState {
    fn new(
        args: &ThermalSelfTestArgs,
        target_temp_c: i16,
        control_target: ThermalCandidatePoint,
        source_power_watts: u16,
        use_point_local_profile: bool,
        initial_status: Option<Value>,
    ) -> Self {
        let started = tokio::time::Instant::now();
        let stage_timeout = Duration::from_secs(args.stage_timeout_seconds.max(1));
        Self {
            target_temp_c,
            stage_timeout,
            warmup_timeout: Duration::from_secs(args.warmup_timeout_seconds.max(1)),
            hold_duration: Duration::from_secs(args.hold_seconds.max(1)),
            sample_interval: Duration::from_millis(effective_thermal_sample_interval_ms(
                args.sample_interval_ms,
            )),
            control_target,
            source_power_watts,
            use_point_local_profile,
            started,
            deadline: started + stage_timeout,
            next_tick: started,
            hold_tracker: ThermalHoldTracker::new(
                target_temp_c,
                Duration::from_secs(args.hold_seconds.max(1)),
            ),
            analyzer: ThermalStageAnalyzer::new(target_temp_c),
            approach_guard: ThermalApproachGuardTracker::new(
                target_temp_c,
                control_target.hold_entry_centi_c,
            ),
            full_speed_tracker: ThermalFullSpeedStableTracker::new(target_temp_c),
            max_temp_c: f64::NEG_INFINITY,
            stage_sample_count: 0,
            recorded_samples: Vec::new(),
            stop_reason: "timeout",
            terminal_runtime_drop_reason: None,
            last_uptime_seconds: None,
            sample_rate_tracker: ThermalSampleRateTracker::new(),
            measurement_guard_tracker: ThermalMeasurementGuardTracker::default(),
            heater_output_seen: false,
            runtime_rearm_attempts_remaining: args.runtime_rearm_attempts,
            next_status: initial_status,
        }
    }
}

async fn run_thermal_stage(
    input: ThermalStageInput<'_>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalStageInput {
        client,
        resolved,
        lease_id,
        samples_writer,
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        runtime_profile,
        args,
        source_sampler,
        sample_index,
        initial_status,
    } = input;
    let control_target = thermal_candidate_point_from_heater_parameters(heater_parameters)?;
    let source_selection = resolve_thermal_source_selection(args)?;
    let use_point_local_profile =
        thermal_self_test_uses_point_local_profile(&source_selection, args.calibration_run);
    let source_power_watts = thermal_effective_source_power_watts(args, &source_selection);
    let mut state = ThermalStageState::new(
        args,
        target_temp_c,
        control_target,
        source_power_watts,
        use_point_local_profile,
        initial_status,
    );

    run_thermal_stage_loop(ThermalStageLoopInput {
        client,
        resolved,
        lease_id,
        samples_writer,
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        runtime_profile,
        args,
        source_sampler,
        sample_index,
        state: &mut state,
    })
    .await?;
    complete_thermal_stage(ThermalStageCompletionInput {
        client,
        resolved,
        lease_id,
        target_temp_c,
        started: state.started,
        hold_tracker: &state.hold_tracker,
        full_speed_tracker: &mut state.full_speed_tracker,
        max_temp_c: state.max_temp_c,
        recorded_samples: &state.recorded_samples,
        stage_sample_count: state.stage_sample_count,
        stop_reason: state.stop_reason,
        terminal_runtime_drop_reason: state.terminal_runtime_drop_reason,
        analyzer: &state.analyzer,
        approach_guard: &mut state.approach_guard,
    })
    .await
}

struct ThermalStageCompletionInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    target_temp_c: i16,
    started: tokio::time::Instant,
    hold_tracker: &'a ThermalHoldTracker,
    full_speed_tracker: &'a mut ThermalFullSpeedStableTracker,
    max_temp_c: f64,
    recorded_samples: &'a [ThermalReplayStageSample],
    stage_sample_count: usize,
    stop_reason: &'static str,
    terminal_runtime_drop_reason: Option<&'static str>,
    analyzer: &'a ThermalStageAnalyzer,
    approach_guard: &'a mut ThermalApproachGuardTracker,
}

async fn complete_thermal_stage(
    input: ThermalStageCompletionInput<'_>,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalStageCompletionInput {
        client,
        resolved,
        lease_id,
        target_temp_c,
        started,
        hold_tracker,
        full_speed_tracker,
        max_temp_c,
        recorded_samples,
        stage_sample_count,
        stop_reason,
        terminal_runtime_drop_reason,
        analyzer,
        approach_guard,
    } = input;
    if stop_reason != "completed" {
        arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    }
    let guard = approach_guard.finalize();
    let full_speed_to_stable = full_speed_tracker.finalize();
    let mut analysis = if max_temp_c.is_finite() {
        analyzer.finalize(max_temp_c)
    } else {
        ThermalStageAnalysis::default()
    };
    thermal_stage_populate_approach_curve_analysis(&mut analysis, recorded_samples, target_temp_c);
    Ok(ThermalStageResult {
        target_temp_c,
        rise_time_ms: hold_tracker
            .rise_time_ms()
            .unwrap_or_else(|| started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        max_overshoot_c: (max_temp_c - f64::from(target_temp_c)).max(0.0),
        hold_peak_to_peak_c: hold_tracker.peak_to_peak_c(),
        sample_count: stage_sample_count,
        stop_reason,
        terminal_runtime_drop_reason,
        analysis,
        guard,
        full_speed_to_stable,
    })
}

struct ThermalStageLoopInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    samples_writer: &'a mut BufWriter<File>,
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    runtime_profile: &'a Value,
    args: &'a ThermalSelfTestArgs,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    sample_index: &'a mut usize,
    state: &'a mut ThermalStageState,
}

async fn run_thermal_stage_loop(
    input: ThermalStageLoopInput<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut input = input;
    loop {
        let now = tokio::time::Instant::now();
        if now >= input.state.deadline {
            break;
        }
        if !thermal_stage_iteration(&mut input, now).await? {
            break;
        }
        input.state.next_tick += input.state.sample_interval;
        tokio::time::sleep_until(input.state.next_tick).await;
    }
    Ok(())
}

async fn thermal_stage_iteration(
    input: &mut ThermalStageLoopInput<'_>,
    now: tokio::time::Instant,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let state = &mut *input.state;
    let (source_telemetry, source_telemetry_stale_ms) = match input.source_sampler.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error)
            if state.runtime_rearm_attempts_remaining > 0
                && thermal_source_telemetry_stale_error(error.as_ref()) =>
        {
            recover_thermal_stage_iteration_source(input, error).await?;
            return Ok(true);
        }
        Err(error) => return Err(error),
    };
    let status = match state.next_status.take() {
        Some(status) => status,
        None => match request_thermal_status_with_retry(input.client, input.resolved, input.lease_id).await {
            Ok(status) => status,
            Err(_) => {
                state.stop_reason = "status_request_failed";
                return Ok(false);
            }
        },
    };
    if let Some(reason) =
        thermal_runtime_drop_reason(&status, input.target_temp_c, state.last_uptime_seconds)
    {
        if let Some(rearmed_status) = handle_thermal_runtime_drop(ThermalRuntimeDropInput {
            client: input.client,
            resolved: input.resolved,
            lease_id: input.lease_id,
            run_id: input.run_id,
            test_phase: input.test_phase,
            target_temp_c: input.target_temp_c,
            source_voltage_mv: input.source_voltage_mv,
            source_current_ma: input.source_current_ma,
            heater_parameters: input.heater_parameters,
            args: input.args,
            source_telemetry: &source_telemetry,
            source_telemetry_stale_ms,
            source_power_watts: state.source_power_watts,
            samples_writer: input.samples_writer,
            sample_index: input.sample_index,
            stage_sample_count: &mut state.stage_sample_count,
            runtime_rearm_attempts_remaining: &mut state.runtime_rearm_attempts_remaining,
            reason,
            status: &status,
            started: state.started,
        })
        .await?
        {
            state.next_status = Some(rearmed_status);
            reset_thermal_stage_after_rearm(state);
            return Ok(true);
        }
        state.stop_reason = reason.as_str();
        state.terminal_runtime_drop_reason = Some(reason.as_str());
        return Ok(false);
    }
    state.last_uptime_seconds = status.get("uptimeSeconds").and_then(Value::as_u64);
    record_thermal_stage_observation(ThermalStageObservationInput {
        status: &status,
        source_telemetry: &source_telemetry,
        source_telemetry_stale_ms,
        run_id: input.run_id,
        test_phase: input.test_phase,
        target_temp_c: input.target_temp_c,
        source_voltage_mv: input.source_voltage_mv,
        source_current_ma: input.source_current_ma,
        heater_parameters: input.heater_parameters,
        args: input.args,
        started: state.started,
        now,
        warmup_timeout: state.warmup_timeout,
        sample_index: input.sample_index,
        stage_sample_count: &mut state.stage_sample_count,
        recorded_samples: &mut state.recorded_samples,
        stop_reason: &mut state.stop_reason,
        max_temp_c: &mut state.max_temp_c,
        heater_output_seen: &mut state.heater_output_seen,
        sample_rate_tracker: &mut state.sample_rate_tracker,
        measurement_guard_tracker: &mut state.measurement_guard_tracker,
        analyzer: &mut state.analyzer,
        hold_tracker: &mut state.hold_tracker,
        approach_guard: &mut state.approach_guard,
        full_speed_tracker: &mut state.full_speed_tracker,
        samples_writer: input.samples_writer,
    })?;
    Ok(state.stop_reason == "timeout")
}

async fn recover_thermal_stage_iteration_source(
    input: &mut ThermalStageLoopInput<'_>,
    error: Box<dyn std::error::Error + Send + Sync>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = &mut *input.state;
    let next_status = recover_thermal_stage_source(ThermalStageSourceRecoveryInput {
        client: input.client,
        resolved: input.resolved,
        lease_id: input.lease_id,
        run_id: input.run_id,
        test_phase: input.test_phase,
        target_temp_c: input.target_temp_c,
        source_voltage_mv: input.source_voltage_mv,
        source_current_ma: input.source_current_ma,
        heater_parameters: input.heater_parameters,
        runtime_profile: input.runtime_profile,
        args: input.args,
        source_sampler: input.source_sampler,
        samples_writer: input.samples_writer,
        sample_index: input.sample_index,
        stage_sample_count: &mut state.stage_sample_count,
        runtime_rearm_attempts_remaining: &mut state.runtime_rearm_attempts_remaining,
        source_power_watts: state.source_power_watts,
        use_point_local_profile: state.use_point_local_profile,
        error,
        started: state.started,
    })
    .await?;
    state.next_status = Some(next_status);
    reset_thermal_stage_after_rearm(state);
    Ok(())
}

fn reset_thermal_stage_after_rearm(state: &mut ThermalStageState) {
    state.started = tokio::time::Instant::now();
    state.deadline = state.started + state.stage_timeout;
    state.next_tick = state.started;
    state.hold_tracker = ThermalHoldTracker::new(state.target_temp_c, state.hold_duration);
    state.analyzer = ThermalStageAnalyzer::new(state.target_temp_c);
    state.approach_guard = ThermalApproachGuardTracker::new(
        state.target_temp_c,
        state.control_target.hold_entry_centi_c,
    );
    state.full_speed_tracker = ThermalFullSpeedStableTracker::new(state.target_temp_c);
    state.max_temp_c = f64::NEG_INFINITY;
    state.recorded_samples.clear();
    state.last_uptime_seconds = None;
    state.sample_rate_tracker = ThermalSampleRateTracker::new();
    state.measurement_guard_tracker = ThermalMeasurementGuardTracker::default();
    state.heater_output_seen = false;
}

struct ThermalStageSourceRecoveryInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    runtime_profile: &'a Value,
    args: &'a ThermalSelfTestArgs,
    source_sampler: &'a mut BenchSourceTelemetrySampler,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    stage_sample_count: &'a mut usize,
    runtime_rearm_attempts_remaining: &'a mut u8,
    source_power_watts: u16,
    use_point_local_profile: bool,
    error: Box<dyn std::error::Error + Send + Sync>,
    started: tokio::time::Instant,
}

async fn recover_thermal_stage_source(
    input: ThermalStageSourceRecoveryInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalStageSourceRecoveryInput {
        client,
        resolved,
        lease_id,
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        runtime_profile,
        args,
        source_sampler,
        samples_writer,
        sample_index,
        stage_sample_count,
        runtime_rearm_attempts_remaining,
        source_power_watts,
        use_point_local_profile,
        error,
        started,
    } = input;
    *runtime_rearm_attempts_remaining = runtime_rearm_attempts_remaining.saturating_sub(1);
    let stale_ms = source_sampler.latest_stale_ms();
    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let status =
        arm_thermal_self_test_heater(client, resolved, lease_id, false, target_temp_c).await?;
    let sample = json!({
        "runId": run_id,
        "sampleIndex": *sample_index,
        "capturedAtUnixMs": current_unix_millis(),
        "elapsedMs": elapsed_ms,
        "testPhase": test_phase,
        "phase": "source_recovery",
        "targetTempC": target_temp_c,
        "source": {
            "mode": args.source_mode,
            "capabilityPowerWatts": source_power_watts,
            "requestedVoltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
            "requestedCurrentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
        },
        "sourceTelemetry": source_sampler.latest().to_value(),
        "sourceTelemetryStaleMs": stale_ms,
        "sourceRecovery": {
            "reason": "source_telemetry_stale",
            "error": error.to_string(),
            "runtimeRearmAttemptsRemaining": *runtime_rearm_attempts_remaining,
        },
        "heaterTelemetry": heater_telemetry_value(&status)?,
        "heaterParameters": heater_parameters,
        "status": status,
    });
    writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
    samples_writer.flush()?;
    *sample_index = sample_index.saturating_add(1);
    *stage_sample_count = stage_sample_count.saturating_add(1);
    let recovered = recover_thermal_bench_source_after_stale(
        args.source_kind,
        &args.source_url,
        &args.source_id,
        args.profile_mode,
        source_power_watts,
    )
    .await?;
    *source_sampler =
        BenchSourceTelemetrySampler::new(args.source_kind, &args.source_url, recovered);
    preview_prepare_and_arm_thermal_self_test_target(ThermalPreviewTargetInput {
        client,
        resolved,
        lease_id,
        profile_mode: args.profile_mode,
        profile: runtime_profile,
        target_temp_c,
        heater_parameters,
        use_legacy_profile: use_point_local_profile,
    })
    .await
}

struct ThermalRuntimeDropInput<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease_id: &'a str,
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    args: &'a ThermalSelfTestArgs,
    source_telemetry: &'a BenchSourceLiveTelemetry,
    source_telemetry_stale_ms: u64,
    source_power_watts: u16,
    samples_writer: &'a mut BufWriter<File>,
    sample_index: &'a mut usize,
    stage_sample_count: &'a mut usize,
    runtime_rearm_attempts_remaining: &'a mut u8,
    reason: ThermalRuntimeDropReason,
    status: &'a Value,
    started: tokio::time::Instant,
}

async fn handle_thermal_runtime_drop(
    input: ThermalRuntimeDropInput<'_>,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalRuntimeDropInput {
        client,
        resolved,
        lease_id,
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        args,
        source_telemetry,
        source_telemetry_stale_ms,
        source_power_watts,
        samples_writer,
        sample_index,
        stage_sample_count,
        runtime_rearm_attempts_remaining,
        reason,
        status,
        started,
    } = input;
    let recoverable =
        *runtime_rearm_attempts_remaining > 0 && thermal_recoverable_sensor_fault(status);
    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let sample = json!({
        "runId": run_id,
        "sampleIndex": *sample_index,
        "capturedAtUnixMs": current_unix_millis(),
        "elapsedMs": elapsed_ms,
        "testPhase": test_phase,
        "phase": "runtime_rearm",
        "targetTempC": target_temp_c,
        "source": {
            "mode": args.source_mode,
            "capabilityPowerWatts": source_power_watts,
            "requestedVoltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
            "requestedCurrentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
        },
        "sourceTelemetry": source_telemetry.to_value(),
        "sourceTelemetryStaleMs": source_telemetry_stale_ms,
        "heaterTelemetry": heater_telemetry_value(status)?,
        "heaterParameters": heater_parameters,
        "runtimeDropReason": reason.as_str(),
        "runtimeRearmAttemptsRemaining": *runtime_rearm_attempts_remaining,
        "status": status,
    });
    writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
    samples_writer.flush()?;
    *sample_index = sample_index.saturating_add(1);
    *stage_sample_count = stage_sample_count.saturating_add(1);
    if !recoverable {
        return Ok(None);
    }
    *runtime_rearm_attempts_remaining = runtime_rearm_attempts_remaining.saturating_sub(1);
    Ok(Some(
        arm_thermal_self_test_heater(client, resolved, lease_id, true, target_temp_c).await?,
    ))
}

struct ThermalStageObservationInput<'a> {
    status: &'a Value,
    source_telemetry: &'a BenchSourceLiveTelemetry,
    source_telemetry_stale_ms: u64,
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    args: &'a ThermalSelfTestArgs,
    started: tokio::time::Instant,
    now: tokio::time::Instant,
    warmup_timeout: Duration,
    sample_index: &'a mut usize,
    stage_sample_count: &'a mut usize,
    recorded_samples: &'a mut Vec<ThermalReplayStageSample>,
    stop_reason: &'a mut &'static str,
    max_temp_c: &'a mut f64,
    heater_output_seen: &'a mut bool,
    sample_rate_tracker: &'a mut ThermalSampleRateTracker,
    measurement_guard_tracker: &'a mut ThermalMeasurementGuardTracker,
    analyzer: &'a mut ThermalStageAnalyzer,
    hold_tracker: &'a mut ThermalHoldTracker,
    approach_guard: &'a mut ThermalApproachGuardTracker,
    full_speed_tracker: &'a mut ThermalFullSpeedStableTracker,
    samples_writer: &'a mut BufWriter<File>,
}

struct ThermalStageMeasurement<'a> {
    current_temp_c: f64,
    heater_output_percent: u8,
    elapsed_ms: u64,
    sample_rate: ThermalSampleRateObservation,
    control_measurement_guarded: bool,
    measurement_guard_violation: bool,
    control_phase: Option<&'a str>,
    control_phase_in_hold: bool,
    phase: &'a str,
}

struct ThermalStageMeasurementInput<'a> {
    status: &'a Value,
    target_temp_c: i16,
    started: tokio::time::Instant,
    now: tokio::time::Instant,
    warmup_timeout: Duration,
    heater_output_seen: &'a mut bool,
    max_temp_c: &'a mut f64,
    sample_rate_tracker: &'a mut ThermalSampleRateTracker,
    measurement_guard_tracker: &'a mut ThermalMeasurementGuardTracker,
    analyzer: &'a mut ThermalStageAnalyzer,
    hold_tracker: &'a mut ThermalHoldTracker,
    stop_reason: &'a mut &'static str,
    approach_guard: &'a mut ThermalApproachGuardTracker,
    full_speed_tracker: &'a mut ThermalFullSpeedStableTracker,
    enforce_stage_limits: bool,
}

fn measure_thermal_stage(input: ThermalStageMeasurementInput<'_>) -> Result<ThermalStageMeasurement<'_>, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalStageMeasurementInput {
        status,
        target_temp_c,
        started,
        now,
        warmup_timeout,
        heater_output_seen,
        max_temp_c,
        sample_rate_tracker,
        measurement_guard_tracker,
        analyzer,
        hold_tracker,
        stop_reason,
        approach_guard,
        full_speed_tracker,
        enforce_stage_limits,
    } = input;
    let current_temp_c = thermal_control_temperature_c(status, None)?;
    let heater_output_percent =
        require_status_u64(status, "heaterOutputPercent")?.min(u64::from(u8::MAX)) as u8;
    *heater_output_seen |= heater_output_percent > 0;
    *max_temp_c = max_temp_c.max(current_temp_c);
    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let sample_rate = sample_rate_tracker.observe(elapsed_ms);
    let control_measurement_guarded = status
        .get("heaterControlMeasurementGuarded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let measurement_guard_violation =
        measurement_guard_tracker.observe(control_measurement_guarded, elapsed_ms);
    let control_phase = status.get("heaterControlPhase").and_then(Value::as_str);
    let control_phase_in_hold = control_phase.is_some_and(|phase| phase == "hold");
    analyzer.observe(
        current_temp_c,
        heater_output_percent,
        elapsed_ms,
        control_phase_in_hold,
    );
    let observation = hold_tracker.observe(current_temp_c, elapsed_ms, now, control_phase_in_hold);
    let mut phase = match observation {
        ThermalHoldObservation::Warmup => "warmup",
        ThermalHoldObservation::Hold | ThermalHoldObservation::Completed => {
            if observation == ThermalHoldObservation::Completed {
                *stop_reason = "completed";
            }
            "hold"
        }
    };
    if let Some(control_phase) = control_phase {
        phase = control_phase;
    }
    update_thermal_stage_stop_reason(ThermalStageStopInput {
        stop_reason,
        sample_rate_violation: sample_rate.violation,
        measurement_guard_violation,
        heater_output_seen: *heater_output_seen,
        elapsed_ms,
        current_temp_c,
        target_temp_c,
        approach_guard,
        full_speed_tracker,
        control_phase,
        started,
        warmup_timeout,
        enforce_stage_limits,
    });
    Ok(ThermalStageMeasurement {
        current_temp_c,
        heater_output_percent,
        elapsed_ms,
        sample_rate,
        control_measurement_guarded,
        measurement_guard_violation,
        control_phase,
        control_phase_in_hold,
        phase,
    })
}

fn record_thermal_stage_observation(
    input: ThermalStageObservationInput<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalStageObservationInput {
        status,
        source_telemetry,
        source_telemetry_stale_ms,
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        args,
        started,
        now,
        warmup_timeout,
        sample_index,
        stage_sample_count,
        recorded_samples,
        stop_reason,
        max_temp_c,
        heater_output_seen,
        sample_rate_tracker,
        measurement_guard_tracker,
        analyzer,
        hold_tracker,
        approach_guard,
        full_speed_tracker,
        samples_writer,
    } = input;
    let measurement = measure_thermal_stage(ThermalStageMeasurementInput {
        status,
        target_temp_c,
        started,
        now,
        warmup_timeout,
        heater_output_seen,
        max_temp_c,
        sample_rate_tracker,
        measurement_guard_tracker,
        analyzer,
        hold_tracker,
        stop_reason,
        approach_guard,
        full_speed_tracker,
        enforce_stage_limits: args.evaluation_mode.enforces_stage_limits(),
    })?;
    let ThermalStageMeasurement {
        current_temp_c,
        heater_output_percent,
        elapsed_ms,
        sample_rate,
        control_measurement_guarded,
        measurement_guard_violation,
        control_phase,
        control_phase_in_hold,
        phase,
    } = measurement;
    let sample = live_thermal_stage_sample(LiveThermalStageSampleInput {
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        args,
        source_telemetry,
        source_telemetry_stale_ms,
        status,
        sample_index: *sample_index,
        phase,
        elapsed_ms,
        sample_rate,
        control_measurement_guarded,
        measurement_guard_violation,
    })?;
    let replay_sample = ThermalReplayStageSample {
        elapsed_ms,
        current_temp_c,
        heater_output_percent,
        control_phase: control_phase.map(ToOwned::to_owned),
        control_phase_in_hold,
        source_voltage_mv: Some(source_telemetry.voltage_mv),
        source_current_ma: Some(source_telemetry.current_ma),
        source_power_mw: Some(source_telemetry.power_mw),
    };
    append_thermal_stage_sample(
        samples_writer,
        sample,
        recorded_samples,
        replay_sample,
        sample_index,
        stage_sample_count,
    )?;
    Ok(())
}

fn append_thermal_stage_sample(
    samples_writer: &mut BufWriter<File>,
    sample: Value,
    recorded_samples: &mut Vec<ThermalReplayStageSample>,
    replay_sample: ThermalReplayStageSample,
    sample_index: &mut usize,
    stage_sample_count: &mut usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
    samples_writer.flush()?;
    recorded_samples.push(replay_sample);
    *sample_index = sample_index.saturating_add(1);
    *stage_sample_count = stage_sample_count.saturating_add(1);
    Ok(())
}

struct ThermalStageStopInput<'a> {
    stop_reason: &'a mut &'static str,
    sample_rate_violation: bool,
    measurement_guard_violation: bool,
    heater_output_seen: bool,
    elapsed_ms: u64,
    current_temp_c: f64,
    target_temp_c: i16,
    approach_guard: &'a mut ThermalApproachGuardTracker,
    full_speed_tracker: &'a mut ThermalFullSpeedStableTracker,
    control_phase: Option<&'a str>,
    started: tokio::time::Instant,
    warmup_timeout: Duration,
    enforce_stage_limits: bool,
}

fn update_thermal_stage_stop_reason(input: ThermalStageStopInput<'_>) {
    let ThermalStageStopInput {
        stop_reason,
        sample_rate_violation,
        measurement_guard_violation,
        heater_output_seen,
        elapsed_ms,
        current_temp_c,
        target_temp_c,
        approach_guard,
        full_speed_tracker,
        control_phase,
        started,
        warmup_timeout,
        enforce_stage_limits,
    } = input;
    if *stop_reason == "timeout" && sample_rate_violation {
        *stop_reason = "sample_rate_below_3hz";
    }
    if *stop_reason == "timeout" && measurement_guard_violation {
        *stop_reason = "temperature_sample_glitch";
    }
    if *stop_reason == "timeout"
        && !heater_output_seen
        && elapsed_ms >= THERMAL_HEATER_OUTPUT_START_TIMEOUT_MS
        && current_temp_c < f64::from(target_temp_c) - 1.0
    {
        *stop_reason = "heater_no_output";
    }
    if *stop_reason == "timeout" {
        approach_guard.observe(current_temp_c, elapsed_ms, control_phase);
    }
    let full_speed_stop_reason = if *stop_reason == "timeout" {
        match full_speed_tracker.observe(current_temp_c, elapsed_ms, control_phase) {
            ThermalFullSpeedStableObservation::Failed(reason) => Some(reason),
            _ => None,
        }
    } else {
        None
    };
    if *stop_reason == "timeout"
        && full_speed_tracker.warmup_exited_at_ms.is_none()
        && started.elapsed() >= warmup_timeout
    {
        *stop_reason = "warmup_timeout";
    }
    if *stop_reason == "timeout"
        && enforce_stage_limits
        && let Some(reason) = full_speed_stop_reason
    {
        *stop_reason = reason;
    }
}

struct LiveThermalStageSampleInput<'a> {
    run_id: &'a str,
    test_phase: &'a str,
    target_temp_c: i16,
    source_voltage_mv: u16,
    source_current_ma: u16,
    heater_parameters: &'a Value,
    args: &'a ThermalSelfTestArgs,
    source_telemetry: &'a BenchSourceLiveTelemetry,
    source_telemetry_stale_ms: u64,
    status: &'a Value,
    sample_index: usize,
    phase: &'a str,
    elapsed_ms: u64,
    sample_rate: ThermalSampleRateObservation,
    control_measurement_guarded: bool,
    measurement_guard_violation: bool,
}

fn live_thermal_stage_sample(
    input: LiveThermalStageSampleInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let LiveThermalStageSampleInput {
        run_id,
        test_phase,
        target_temp_c,
        source_voltage_mv,
        source_current_ma,
        heater_parameters,
        args,
        source_telemetry,
        source_telemetry_stale_ms,
        status,
        sample_index,
        phase,
        elapsed_ms,
        sample_rate,
        control_measurement_guarded,
        measurement_guard_violation,
    } = input;
    Ok(json!({
        "runId": run_id,
        "sampleIndex": sample_index,
        "capturedAtUnixMs": current_unix_millis(),
        "elapsedMs": elapsed_ms,
        "testPhase": test_phase,
        "phase": phase,
        "targetTempC": target_temp_c,
        "source": {
            "mode": args.source_mode,
            "requestedVoltageMv": (args.source_mode == "manual-forced").then_some(source_voltage_mv),
            "requestedCurrentLimitMa": (args.source_mode == "manual-forced").then_some(source_current_ma),
        },
        "sourceTelemetry": source_telemetry.to_value(),
        "sourceTelemetryStaleMs": source_telemetry_stale_ms,
        "heaterTelemetry": heater_telemetry_value(status)?,
        "heaterParameters": heater_parameters,
        "sampling": {
            "intervalMs": sample_rate.interval_ms,
            "rollingRateHz": sample_rate.rolling_rate_hz,
            "minimumRateHz": THERMAL_MIN_SAMPLE_RATE_HZ,
            "windowMs": THERMAL_SAMPLE_RATE_WINDOW_MS,
            "rateViolation": sample_rate.violation,
            "controlMeasurementGuarded": control_measurement_guarded,
            "measurementGuardGraceMs": THERMAL_MEASUREMENT_GUARD_FAILURE_GRACE_MS,
            "measurementGuardViolation": measurement_guard_violation,
        },
        "status": status,
    }))
}

async fn wait_for_cooldown(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
    cooldown_temp_c: f64,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        // A cooling window follows a deliberately disarmed stage; a transient serial
        // timeout here must not turn an already-classified environment retry into a
        // terminal HIL failure.
        let status = request_thermal_status_with_retry(client, resolved, lease_id).await?;
        let current_temp_c = require_status_f64(&status, "currentTempC")?;
        if cooldown_target_reached(current_temp_c, cooldown_temp_c) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "thermal self-test requires cooldown to <= {cooldown_temp_c:.1}C, got {current_temp_c:.1}C"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(THERMAL_COOLDOWN_POLL_INTERVAL_MS)).await;
    }
}

fn cooldown_target_reached(current_temp_c: f64, cooldown_temp_c: f64) -> bool {
    current_temp_c <= cooldown_temp_c + THERMAL_COOLDOWN_EPSILON_C
}

fn thermal_stage_can_continue_tuning(stage: &ThermalStageResult) -> bool {
    matches!(
        stage.stop_reason,
        "completed"
            | "warmup_timeout"
            | "full_speed_to_stable_timeout"
            | "approach_threshold_timeout"
            | "approach_hold_timeout"
            | "approach_reentered_warmup"
    )
}

fn thermal_stage_can_tune(stage: &ThermalStageResult) -> bool {
    matches!(
        stage.stop_reason,
        "completed"
            | "timeout"
            | "warmup_timeout"
            | "full_speed_to_stable_timeout"
            | "approach_threshold_timeout"
            | "approach_hold_timeout"
            | "approach_reentered_warmup"
    )
}

fn validate_thermal_applied_results(
    applied: &[ThermalStageResult],
    expected_targets_c: &[i16],
    evaluation_mode: ThermalSelfTestEvaluationMode,
) -> Value {
    let mut failures = Vec::new();
    for target_temp_c in expected_targets_c {
        if !applied
            .iter()
            .any(|stage| stage.target_temp_c == *target_temp_c)
        {
            failures.push(json!({
                "targetTempC": target_temp_c,
                "phase": "applied",
                "reason": "missing_stage",
            }));
        }
    }
    for applied_stage in applied {
        if applied_stage.stop_reason != "completed" {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "phase": "applied",
                "reason": "incomplete_stage",
                "stopReason": applied_stage.stop_reason,
                "guard": {
                    "approachStartedAtMs": applied_stage.guard.approach_started_at_ms,
                    "holdThresholdCrossedAtMs": applied_stage.guard.hold_threshold_crossed_at_ms,
                    "firstHoldAtMs": applied_stage.guard.first_hold_at_ms,
                    "warmupReenteredAtMs": applied_stage.guard.warmup_reentered_at_ms,
                },
            }));
        }
        if thermal_stage_stop_reason_is_environment_fault(applied_stage.stop_reason)
            || applied_stage
                .terminal_runtime_drop_reason
                .is_some_and(thermal_stage_stop_reason_is_environment_fault)
        {
            continue;
        }
        if evaluation_mode.reports_stage_limits() && applied_stage.max_overshoot_c > 3.0 {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "reason": "overshoot",
                "value": applied_stage.max_overshoot_c,
                "limit": 3.0,
            }));
        }
        if evaluation_mode.reports_stage_limits() && applied_stage.hold_peak_to_peak_c > 3.0 {
            failures.push(json!({
                "targetTempC": applied_stage.target_temp_c,
                "reason": "hold_p2p",
                "value": applied_stage.hold_peak_to_peak_c,
                "limit": 3.0,
            }));
        }
        if evaluation_mode.reports_stage_limits() {
            let limit_ms = ThermalFullSpeedStableTracker::settle_limit_ms_for_target(
                applied_stage.target_temp_c,
            );
            match applied_stage.full_speed_to_stable.settle_time_ms {
                Some(value) if value <= limit_ms => {}
                Some(value) => failures.push(json!({
                    "targetTempC": applied_stage.target_temp_c,
                    "reason": "full_speed_to_stable",
                    "value": value,
                    "limit": limit_ms,
                    "failureReason": applied_stage.full_speed_to_stable.failure_reason,
                })),
                None => failures.push(json!({
                    "targetTempC": applied_stage.target_temp_c,
                    "reason": "full_speed_to_stable_missing",
                    "limit": limit_ms,
                    "warmupExitedAtMs": applied_stage.full_speed_to_stable.warmup_exited_at_ms,
                    "stableWindowStartedAtMs": applied_stage.full_speed_to_stable.stable_window_started_at_ms,
                    "failureReason": applied_stage.full_speed_to_stable.failure_reason,
                })),
            }
        }
    }
    json!({
        "passed": failures.is_empty() && !expected_targets_c.is_empty(),
        "expectedTargetsC": expected_targets_c,
        "failures": failures,
    })
}

fn read_bench_source_live_telemetry(
    source_kind: BenchSourceKind,
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => read_isolapurr_live_telemetry(source_url),
    }
}

fn validate_thermal_bench_source_tools(
    source_kind: BenchSourceKind,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => validate_isolapurr_tools(),
    }
}

async fn restore_thermal_bench_source_default(
    client: &Client,
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => {
            ensure_isolapurr_thermal_capability(
                source_url,
                source_id,
                THERMAL_SOURCE_100W_POWER_WATTS as u16,
            )?;
            set_isolapurr_output_auto(client, source_url, source_id).await
        }
    }
}

async fn recover_thermal_bench_source_after_stale(
    source_kind: BenchSourceKind,
    source_url: &str,
    source_id: &str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => recover_isolapurr_runtime_output_gate(
            source_url,
            source_id,
            profile_mode,
            source_power_watts,
        ),
    }
}

struct ThermalSourceConfig<'a> {
    client: &'a Client,
    source_url: &'a str,
    source_id: &'a str,
    source_mode: &'a str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
    voltage_mv: u16,
    current_limit_ma: u16,
}

struct ThermalSourceLeaseInput<'a> {
    resolved: &'a ResolvedUsbTarget,
    source_kind: BenchSourceKind,
    config: ThermalSourceConfig<'a>,
}

async fn prepare_thermal_bench_source(
    source_kind: BenchSourceKind,
    config: &ThermalSourceConfig<'_>,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    match source_kind {
        BenchSourceKind::Isolapurr => prepare_isolapurr_thermal_source(config).await,
    }
}

fn read_isolapurr_live_telemetry(
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    for attempt in 1..=ISOLAPURR_LIVE_TELEMETRY_ATTEMPTS {
        let result = isolapurr_cli_json_read_once_with_timeout(
            source_url,
            &["power", "show"],
            ISOLAPURR_LIVE_TELEMETRY_TIMEOUT,
        )
        .and_then(|power| parse_isolapurr_live_telemetry(&power).map_err(Into::into));
        match result {
            Ok(telemetry) => return Ok(telemetry),
            Err(error) => {
                let message = error.to_string();
                if attempt >= ISOLAPURR_LIVE_TELEMETRY_ATTEMPTS
                    || !(isolapurr_cli_transient_error_message(&message)
                        || isolapurr_live_telemetry_transient_error_message(&message))
                {
                    return Err(error);
                }
                last_error = Some(message);
                std::thread::sleep(isolapurr_read_retry_delay(attempt));
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| "isolapurr live telemetry did not stabilize".to_string())
        .into())
}

fn validate_isolapurr_tools() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tool = "isolapurr";
    let output = ProcessCommand::new(tool)
        .arg("--help")
        .output()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "{tool} --help failed to start; install the required host tooling: {error}"
                ),
            )
        })?;
    if !output.status.success() {
        return Err(format!("{tool} --help exited with {}", output.status).into());
    }
    Ok(())
}

fn parse_isolapurr_live_telemetry(ports: &Value) -> Result<BenchSourceLiveTelemetry, String> {
    let port_c = ports
        .get("ports")
        .and_then(Value::as_array)
        .or_else(|| {
            ports
                .get("ports")
                .and_then(|ports| ports.get("ports"))
                .and_then(Value::as_array)
        })
        .and_then(|ports| {
            ports.iter().find(|port| {
                port.get("portId").and_then(Value::as_str) == Some("port_c")
                    || port.get("label").and_then(Value::as_str) == Some("USB-C")
            })
        })
        .ok_or_else(|| "isolapurr ports missing USB-C telemetry".to_string())?;
    let telemetry = port_c
        .get("telemetry")
        .and_then(Value::as_object)
        .or_else(|| port_c.get("telemetry_raw").and_then(Value::as_object))
        .ok_or_else(|| "isolapurr USB-C telemetry missing object".to_string())?;
    let status = json_str_any(telemetry, &["status"]).unwrap_or("unknown");
    let state = port_c.get("state").cloned().unwrap_or(Value::Null);
    let voltage_mv = json_u64_any(telemetry, &["voltage_mv", "voltageMv"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing voltage status={status} state={state}")
    })?;
    let current_ma = json_u64_any(telemetry, &["current_ma", "currentMa"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing current status={status} state={state}")
    })?;
    let power_mw = json_u64_any(telemetry, &["power_mw", "powerMw"]).ok_or_else(|| {
        format!("isolapurr USB-C telemetry missing power status={status} state={state}")
    })?;
    let sample_uptime_ms = json_u64_any(telemetry, &["sample_uptime_ms", "sampleUptimeMs"])
        .ok_or_else(|| {
            format!("isolapurr USB-C telemetry missing sample uptime status={status} state={state}")
        })?;
    Ok(BenchSourceLiveTelemetry {
        voltage_mv,
        current_ma,
        power_mw,
        sample_uptime_ms,
        status: status.to_string(),
    })
}

async fn set_isolapurr_output_auto(
    _client: &Client,
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_usb_c_path_is_default(&config) {
        let response = isolapurr_cli_json(
            source_url,
            &["power", "output", "manual", "--usb-c-path", "automatic"],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_usb_c_path_is_default(&response)
        {
            return Err(format!(
                "isolapurr auto output path-normalization did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_is_auto(&config) {
        let response = isolapurr_cli_json(source_url, &["power", "output", "auto"])?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_value_is_auto(&response)
        {
            return Err(format!(
                "isolapurr auto output command did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_is_auto(&config) {
        return Err(format!(
            "isolapurr power output auto readback mismatch for source_url={source_url}"
        )
        .into());
    }
    if !isolapurr_power_config_path_is_automatic(&config) {
        return Err(format!(
            "isolapurr power output auto left USB-C path in a non-automatic state for source_url={source_url}"
        )
        .into());
    }
    Ok(())
}

fn recover_isolapurr_runtime_output_gate(
    source_url: &str,
    device_id: &str,
    profile_mode: ThermalProfileMode,
    source_power_watts: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    if profile_mode.explicit_bank().is_some() {
        ensure_isolapurr_thermal_capability(source_url, device_id, source_power_watts)?;
    } else {
        ensure_isolapurr_auto_thermal_capability(source_url, device_id)?;
    }
    let recovery = (|| {
        ensure_isolapurr_runtime_output_disabled(source_url)?;
        std::thread::sleep(Duration::from_secs(2));
        ensure_isolapurr_runtime_output_recovered(source_url)
    })();
    match recovery {
        Ok(telemetry) => Ok(telemetry),
        Err(error) => {
            let restore_error = restore_isolapurr_runtime_output_enabled_best_effort(source_url)
                .err()
                .map(|restore_error| format!("; best-effort restore failed: {restore_error}"))
                .unwrap_or_default();
            Err(format!(
                "isolapurr runtime output recovery failed source_url={source_url}: {error}{restore_error}"
            )
            .into())
        }
    }
}

async fn prepare_isolapurr_thermal_source(
    config: &ThermalSourceConfig<'_>,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let ThermalSourceConfig {
        client,
        source_url,
        source_id: device_id,
        source_mode,
        profile_mode,
        source_power_watts,
        voltage_mv,
        current_limit_ma,
    } = *config;
    if source_mode == "manual-forced" {
        set_isolapurr_output_manual(client, source_url, device_id, voltage_mv, current_limit_ma)
            .await
    } else {
        if profile_mode.explicit_bank().is_some() {
            ensure_isolapurr_thermal_capability(source_url, device_id, source_power_watts)?;
        } else {
            ensure_isolapurr_auto_thermal_capability(source_url, device_id)?;
        }
        set_isolapurr_output_auto(client, source_url, device_id).await?;
        let telemetry = ensure_isolapurr_live_telemetry_ready(source_url)?;
        validate_isolapurr_ready_voltage(&telemetry)?;
        Ok(telemetry)
    }
}

async fn prepare_thermal_source_and_lease(
    input: ThermalSourceLeaseInput<'_>,
) -> Result<(BenchSourceLiveTelemetry, Lease), Box<dyn std::error::Error + Send + Sync>> {
    let ThermalSourceLeaseInput {
        resolved,
        source_kind,
        config,
    } = input;
    let telemetry = prepare_thermal_bench_source(source_kind, &config).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    match create_ready_thermal_lease(config.client, resolved).await {
        Ok((lease, _status)) => Ok((telemetry, lease)),
        Err(error) => {
            match restore_thermal_bench_source_default(
                config.client,
                source_kind,
                config.source_url,
                config.source_id,
            )
            .await
            {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}; {} cleanup after lease failure also failed: {cleanup_error}",
                    source_kind.as_str()
                )
                .into()),
            }
        }
    }
}

fn ensure_isolapurr_thermal_capability(
    source_url: &str,
    device_id: &str,
    source_power_watts: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let required_power_watts = u64::from(source_power_watts);
    let requires_pps_5a = required_power_watts == THERMAL_SOURCE_100W_POWER_WATTS;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_has_thermal_capability(
        &config,
        required_power_watts,
        requires_pps_5a,
    ) {
        let power_watts = required_power_watts.to_string();
        let pd_pps_5a = if requires_pps_5a { "true" } else { "false" };
        let response = isolapurr_cli_json(
            source_url,
            &[
                "power",
                "source-capability",
                "set",
                "--power-watts",
                &power_watts,
                "--pd",
                "true",
                "--pps3-limit-ma",
                "5000",
                "--pd-pps-5a",
                pd_pps_5a,
                "--pps",
                "true",
            ],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_has_thermal_capability(
                response.get("config").unwrap_or(&response),
                required_power_watts,
                requires_pps_5a,
            )
        {
            return Err("isolapurr source capability command did not acknowledge success".into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_has_thermal_capability(
        &config,
        required_power_watts,
        requires_pps_5a,
    ) {
        return Err(
            format!(
                "isolapurr source capability readback must confirm {required_power_watts}W, PD Fixed, and PPS"
            )
            .into(),
        );
    }
    Ok(())
}

fn ensure_isolapurr_auto_thermal_capability(
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let config = read_isolapurr_power_config(source_url)?;
    let source_class = isolapurr_configured_thermal_source_class(&config).ok_or_else(|| {
        format!(
            "isolapurr source capability readback must advertise PPS with 20V coverage for auto thermal mode source_url={source_url}"
        )
    })?;
    if source_class != "pps3a" && source_class != "pps5a" {
        return Err(format!("unsupported isolapurr thermal source class: {source_class}").into());
    }
    Ok(())
}

fn read_isolapurr_configured_source_class(
    source_url: &str,
    device_id: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let config = read_isolapurr_power_config(source_url)?;
    isolapurr_configured_thermal_source_class(&config).ok_or_else(|| {
        format!(
            "isolapurr source capability readback must advertise PPS with 20V coverage source_url={source_url}"
        )
        .into()
    })
}

fn isolapurr_configured_thermal_source_class(config: &Value) -> Option<&'static str> {
    let capability = config.get("capability").unwrap_or(config);
    let fixed_voltages = capability
        .pointer("/pd/fixed_voltages_mv")
        .or_else(|| capability.pointer("/pd/fixedVoltagesMv"))
        .and_then(Value::as_array)?;
    let pps_enabled = capability.pointer("/pd/pps").and_then(Value::as_bool) == Some(true);
    let pd_enabled = capability.pointer("/protocols/pd").and_then(Value::as_bool) == Some(true);
    let covers_20v = fixed_voltages
        .iter()
        .filter_map(Value::as_u64)
        .any(|voltage_mv| voltage_mv >= 20_000);
    if !pps_enabled || !pd_enabled || !covers_20v {
        return None;
    }
    let pps_max_ma = json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["pps3_limit_ma", "pps3LimitMa"],
    )
    .or_else(|| {
        capability
            .pointer("/current/pps3_limit_ma")
            .or_else(|| capability.pointer("/current/pps3LimitMa"))
            .or_else(|| capability.pointer("/pd/pps3_limit_ma"))
            .or_else(|| capability.pointer("/pd/pps3LimitMa"))
            .and_then(Value::as_u64)
    })
    .unwrap_or(0);
    Some(if pps_max_ma >= 5_000 {
        "pps5a"
    } else {
        "pps3a"
    })
}

fn isolapurr_power_config_has_thermal_capability(
    config: &Value,
    required_power_watts: u64,
    requires_pps_5a: bool,
) -> bool {
    let capability = config.get("capability").unwrap_or(config);
    let pps3_limit_ma = json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["pps3_limit_ma", "pps3LimitMa"],
    )
    .or_else(|| {
        capability
            .pointer("/current/pps3_limit_ma")
            .or_else(|| capability.pointer("/current/pps3LimitMa"))
            .or_else(|| capability.pointer("/pd/pps3_limit_ma"))
            .or_else(|| capability.pointer("/pd/pps3LimitMa"))
            .and_then(Value::as_u64)
    });
    let pd_pps_5a = capability
        .pointer("/current/pd_pps_5a")
        .or_else(|| capability.pointer("/current/pdPps5a"))
        .or_else(|| capability.pointer("/pd/pd_pps_5a"))
        .or_else(|| capability.pointer("/pd/pdPps5a"))
        .or_else(|| capability.get("pd_pps_5a"))
        .or_else(|| capability.get("pdPps5a"))
        .and_then(Value::as_bool);
    json_u64_any(
        capability.as_object().unwrap_or(&serde_json::Map::new()),
        &["power_watts", "powerWatts"],
    ) == Some(required_power_watts)
        && capability.pointer("/protocols/pd").and_then(Value::as_bool) == Some(true)
        && capability.pointer("/pd/pps").and_then(Value::as_bool) == Some(true)
        && capability
            .pointer("/pd/fixed_voltages_mv")
            .or_else(|| capability.pointer("/pd/fixedVoltagesMv"))
            .and_then(Value::as_array)
            .is_some_and(|voltages| !voltages.is_empty())
        && (!requires_pps_5a || (pps3_limit_ma >= Some(5_000) && pd_pps_5a == Some(true)))
}

fn validate_isolapurr_ready_voltage(
    telemetry: &BenchSourceLiveTelemetry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if telemetry.status != "ok" {
        return Err(format!(
            "isolapurr USB-C output is not ready status={} voltage={}mV current={}mA",
            telemetry.status, telemetry.voltage_mv, telemetry.current_ma
        )
        .into());
    }
    if telemetry.voltage_mv <= THERMAL_SOURCE_MIN_READY_VOLTAGE_MV {
        return Err(format!(
            "isolapurr USB-C output is not above 5V actual={}mV",
            telemetry.voltage_mv
        )
        .into());
    }
    Ok(())
}

async fn set_isolapurr_output_manual(
    _client: &Client,
    source_url: &str,
    device_id: &str,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    validate_isolapurr_device_identity(source_url, device_id)?;
    let mut config = read_isolapurr_power_config(source_url)?;
    if !isolapurr_power_config_value_matches_manual(&config, voltage_mv, current_limit_ma) {
        let response = isolapurr_cli_json(
            source_url,
            &[
                "power",
                "output",
                "manual",
                "--voltage-mv",
                &voltage_mv.to_string(),
                "--current-limit-ma",
                &current_limit_ma.to_string(),
                "--usb-c-path",
                "forced-on",
            ],
        )?;
        if !isolapurr_cli_write_succeeded(&response)
            && !isolapurr_power_config_value_matches_manual(&response, voltage_mv, current_limit_ma)
        {
            return Err(format!(
                "isolapurr manual output command did not acknowledge success source_url={source_url}"
            )
            .into());
        }
        config = read_isolapurr_power_config(source_url)?;
    }
    if !isolapurr_power_config_value_matches_manual(&config, voltage_mv, current_limit_ma) {
        return Err(format!(
            "isolapurr power output manual readback mismatch for source_url={source_url}"
        )
        .into());
    }
    ensure_isolapurr_live_telemetry_ready(source_url)
}

fn validate_isolapurr_device_identity(
    source_url: &str,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let status = isolapurr_cli_json_read(source_url, &["status"])?;
    if !isolapurr_status_identity_matches(&status, device_id) {
        let actual = isolapurr_status_device_id(&status).unwrap_or("unknown");
        return Err(format!(
            "isolapurr identity mismatch source_url={source_url} expected_device_id={device_id} actual_device_id={actual}"
        )
        .into());
    }
    Ok(())
}

fn isolapurr_power_config_value_matches_manual(
    config: &Value,
    voltage_mv: u16,
    current_limit_ma: u16,
) -> bool {
    config
        .as_object()
        .and_then(|config| json_str_any(config, &["tps_mode", "tpsMode"]))
        .is_some_and(|mode| mode == "manual")
        && config
            .get("manual")
            .and_then(Value::as_object)
            .is_some_and(|manual| {
                let manual_current_ma =
                    json_u64_any(manual, &["current_limit_ma", "currentLimitMa"]);
                let current_matches = manual_current_ma == Some(u64::from(current_limit_ma))
                    // A 100W source cannot sustain 21V * 5A (105W). The released
                    // IsolaPurr firmware quantizes the resulting 100W limit to 4.75A.
                    || (voltage_mv == 21_000
                        && current_limit_ma == 5_000
                        && manual_current_ma == Some(4_750)
                        && isolapurr_power_config_has_thermal_capability(
                            config,
                            THERMAL_SOURCE_100W_POWER_WATTS,
                            true,
                        ));
                json_u64_any(manual, &["voltage_mv", "voltageMv"]) == Some(u64::from(voltage_mv))
                    && current_matches
                    && matches!(
                        json_str_any(manual, &["usb_c_path_mode", "usbCPathMode"]),
                        Some("force" | "forced-on")
                    )
                    && matches!(
                        json_str_any(manual, &["path_policy", "pathPolicy"]),
                        Some("force_open" | "force-open")
                    )
            })
}

fn isolapurr_status_identity_matches(status: &Value, device_id: &str) -> bool {
    isolapurr_status_device_id(status) == Some(device_id)
}

fn isolapurr_status_device_id(status: &Value) -> Option<&str> {
    status
        .get("device")
        .or_else(|| status.get("result")?.get("device"))
        .and_then(Value::as_object)
        .and_then(|device| json_str_any(device, &["device_id", "deviceId"]))
}

fn isolapurr_power_config_value_is_auto(config: &Value) -> bool {
    config
        .as_object()
        .and_then(|config| json_str_any(config, &["tps_mode", "tpsMode"]))
        .is_some_and(|mode| mode == "auto_follow" || mode == "autoFollow")
}

fn isolapurr_power_config_usb_c_path_is_default(config: &Value) -> bool {
    config
        .get("manual")
        .and_then(Value::as_object)
        .is_some_and(|manual| {
            matches!(
                json_str_any(manual, &["usb_c_path_mode", "usbCPathMode"]),
                Some("default" | "automatic")
            )
        })
}

fn isolapurr_power_config_path_is_automatic(config: &Value) -> bool {
    isolapurr_power_config_usb_c_path_is_default(config)
        && config
            .get("manual")
            .and_then(Value::as_object)
            .is_some_and(|manual| {
                matches!(
                    json_str_any(manual, &["path_policy", "pathPolicy"]),
                    Some("auto")
                )
            })
}

fn read_isolapurr_power_config(
    source_url: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let response = isolapurr_cli_json_read(source_url, &["power", "config", "show"])?;
    Ok(response.get("config").cloned().unwrap_or(response))
}

fn ensure_isolapurr_live_telemetry_ready(
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    for attempt in 1..=6 {
        let result = isolapurr_cli_json_read(source_url, &["power", "show"])
            .and_then(|power| parse_isolapurr_live_telemetry(&power).map_err(Into::into));
        match result {
            Ok(telemetry) => return Ok(telemetry),
            Err(error) => {
                last_error = Some(error.to_string());
                if attempt < 6 {
                    std::thread::sleep(isolapurr_read_retry_delay(attempt));
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| "isolapurr USB-C telemetry unavailable".to_string())
        .into())
}

fn set_isolapurr_runtime_output_enabled(
    source_url: &str,
    enabled: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let enabled_arg = if enabled { "true" } else { "false" };
    let response = isolapurr_cli_json(
        source_url,
        &["power", "runtime", "output", "--enabled", enabled_arg],
    )?;
    if isolapurr_cli_write_succeeded(&response)
        || isolapurr_runtime_output_enabled(&response) == Some(enabled)
    {
        return Ok(());
    }
    Err(format!(
        "isolapurr runtime output command did not acknowledge enabled={enabled} source_url={source_url}"
    )
    .into())
}

fn ensure_isolapurr_runtime_output_disabled(
    source_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    set_isolapurr_runtime_output_enabled(source_url, false)?;
    for attempt in 1..=8 {
        let power = isolapurr_cli_json_read(source_url, &["power", "show"])?;
        let output_enabled = isolapurr_runtime_output_enabled(&power);
        if output_enabled == Some(false) && isolapurr_usb_c_output_is_off(&power) {
            return Ok(());
        }
        last_error = Some(format!(
            "runtime output disable not settled readback={} usb_c_off={}",
            output_enabled
                .map(|value| value.to_string())
                .unwrap_or_else(|| "missing".to_string()),
            isolapurr_usb_c_output_is_off(&power)
        ));
        if output_enabled != Some(false) {
            let _ = set_isolapurr_runtime_output_enabled(source_url, false);
        }
        if attempt < 8 {
            std::thread::sleep(isolapurr_runtime_recovery_delay(attempt));
        }
    }
    Err(last_error
        .unwrap_or_else(|| "isolapurr runtime output did not disable".to_string())
        .into())
}

fn ensure_isolapurr_runtime_output_recovered(
    source_url: &str,
) -> Result<BenchSourceLiveTelemetry, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None::<String>;
    let mut first_ready_sample_uptime_ms = None::<u64>;
    set_isolapurr_runtime_output_enabled(source_url, true)?;
    for attempt in 1..=10 {
        let power = isolapurr_cli_json_read(source_url, &["power", "show"])?;
        match isolapurr_runtime_output_ready_telemetry(&power, first_ready_sample_uptime_ms) {
            Ok(telemetry) if first_ready_sample_uptime_ms.is_some() => return Ok(telemetry),
            Ok(telemetry) => {
                first_ready_sample_uptime_ms = Some(telemetry.sample_uptime_ms);
                last_error = Some(
                    "waiting for USB-C telemetry to advance after runtime output enable".into(),
                );
            }
            Err(error) => {
                last_error = Some(error);
                if isolapurr_runtime_output_enabled(&power) != Some(true) {
                    let _ = set_isolapurr_runtime_output_enabled(source_url, true);
                }
            }
        }
        if attempt < 10 {
            std::thread::sleep(isolapurr_runtime_recovery_delay(attempt));
        }
    }
    Err(last_error
        .unwrap_or_else(|| "isolapurr runtime output did not recover".to_string())
        .into())
}

fn isolapurr_runtime_output_ready_telemetry(
    value: &Value,
    previous_ready_sample_uptime_ms: Option<u64>,
) -> Result<BenchSourceLiveTelemetry, String> {
    if isolapurr_runtime_output_enabled(value) != Some(true) {
        return Err("runtime output readback is not enabled".to_string());
    }
    let telemetry = parse_isolapurr_live_telemetry(value)?;
    validate_isolapurr_ready_voltage(&telemetry).map_err(|error| error.to_string())?;
    if let Some(previous_uptime_ms) = previous_ready_sample_uptime_ms
        && telemetry.sample_uptime_ms <= previous_uptime_ms
    {
        return Err(format!(
            "USB-C telemetry did not advance during runtime output recovery previous={previous_uptime_ms} current={}",
            telemetry.sample_uptime_ms
        ));
    }
    Ok(telemetry)
}

fn restore_isolapurr_runtime_output_enabled_best_effort(source_url: &str) -> Result<(), String> {
    let mut last_error = None::<String>;
    for attempt in 1..=3 {
        let command_error = set_isolapurr_runtime_output_enabled(source_url, true)
            .err()
            .map(|error| error.to_string());
        match isolapurr_cli_json_read(source_url, &["power", "show"]) {
            Ok(power) if isolapurr_runtime_output_enabled(&power) == Some(true) => return Ok(()),
            Ok(power) => {
                let readback_error = format!(
                    "runtime output readback is {}",
                    isolapurr_runtime_output_enabled(&power)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "missing".to_string())
                );
                last_error = Some(match command_error {
                    Some(command_error) => format!("{command_error}; {readback_error}"),
                    None => readback_error,
                });
            }
            Err(error) => {
                last_error = Some(match command_error {
                    Some(command_error) => format!("{command_error}; power show failed: {error}"),
                    None => error.to_string(),
                });
            }
        }
        if attempt < 3 {
            std::thread::sleep(isolapurr_runtime_recovery_delay(attempt));
        }
    }
    Err(last_error.unwrap_or_else(|| "runtime output restore did not complete".to_string()))
}

fn isolapurr_runtime_recovery_delay(attempt: usize) -> Duration {
    Duration::from_millis(250 * attempt.min(5) as u64)
}

fn isolapurr_runtime_output_enabled(value: &Value) -> Option<bool> {
    value
        .pointer("/config/runtime/output_enabled")
        .or_else(|| value.pointer("/config/runtime/outputEnabled"))
        .or_else(|| value.pointer("/runtime/output_enabled"))
        .or_else(|| value.pointer("/runtime/outputEnabled"))
        .and_then(Value::as_bool)
}

fn isolapurr_usb_c_output_is_off(value: &Value) -> bool {
    let Some(usb_c) = value.pointer("/diagnostics/usb_c_actual") else {
        return false;
    };
    let status = usb_c.get("status").and_then(Value::as_str);
    let current_ma = usb_c
        .get("current_ma")
        .or_else(|| usb_c.get("currentMa"))
        .and_then(Value::as_u64);
    let power_mw = usb_c
        .get("power_mw")
        .or_else(|| usb_c.get("powerMw"))
        .and_then(Value::as_u64);
    status != Some("ok") || (current_ma == Some(0) && power_mw == Some(0))
}

fn thermal_source_telemetry_stale_error(error: &(dyn std::error::Error + Send + Sync)) -> bool {
    let message = error.to_string();
    message.contains("USB-C telemetry did not advance")
        || message.contains("source telemetry stale")
}

fn thermal_source_probe_transient_error(error: &(dyn std::error::Error + Send + Sync)) -> bool {
    let message = error.to_string();
    thermal_source_telemetry_stale_error(error)
        || isolapurr_cli_transient_error_message(&message)
        || isolapurr_live_telemetry_transient_error_message(&message)
}

fn isolapurr_cli_transient_error_message(message: &str) -> bool {
    message.contains("timed out after")
        || message.contains("error sending request for url")
        || message.contains("client error (Connect)")
        || message.contains("tcp connect error")
        || message.contains("Connection refused")
}

fn isolapurr_live_telemetry_transient_error_message(message: &str) -> bool {
    message.contains("isolapurr ports missing USB-C telemetry")
        || message.contains("isolapurr USB-C telemetry missing object")
        || message.contains("isolapurr USB-C telemetry missing voltage")
        || message.contains("isolapurr USB-C telemetry missing current")
        || message.contains("isolapurr USB-C telemetry missing power")
        || message.contains("isolapurr USB-C telemetry missing sample uptime")
        || message.contains("status=not_inserted")
        || message.contains("status=unknown")
}

fn isolapurr_cli_json_read(
    source_url: &str,
    args: &[&str],
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    isolapurr_cli_json_read_with_timeout(source_url, args, Duration::from_secs(5), 6)
}

fn isolapurr_cli_json_read_with_timeout(
    source_url: &str,
    args: &[&str],
    timeout: Duration,
    attempts: usize,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let attempts = attempts.max(1);
    let mut last_error = None::<String>;
    for attempt in 1..=attempts {
        match isolapurr_cli_json_read_once_with_timeout(source_url, args, timeout) {
            Ok(value) => return Ok(value),
            Err(error) => {
                let message = error.to_string();
                if attempt >= attempts || !isolapurr_cli_transient_error_message(&message) {
                    return Err(message.into());
                }
                last_error = Some(message);
                std::thread::sleep(isolapurr_read_retry_delay(attempt));
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| format!("isolapurr {} read did not complete", args.join(" ")))
        .into())
}

fn isolapurr_cli_json_read_once_with_timeout(
    source_url: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    isolapurr_cli_json_with_timeout(source_url, args, timeout)
}

fn isolapurr_read_retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(250 * attempt.min(4) as u64)
}

fn isolapurr_cli_json(
    source_url: &str,
    args: &[&str],
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    isolapurr_cli_json_with_timeout(source_url, args, Duration::from_secs(5))
}

fn isolapurr_cli_json_with_timeout(
    source_url: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut command = ProcessCommand::new("isolapurr");
    command.arg("--json");
    command.args(args);
    command.args(["--url", source_url]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let deadline = StdInstant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if StdInstant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "isolapurr {} timed out after {}ms",
                args.join(" "),
                timeout.as_millis()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout)?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr)?;
    }
    if !status.success() {
        return Err(format!(
            "isolapurr {} exited with {}; stderr={}",
            args.join(" "),
            status,
            String::from_utf8_lossy(&stderr).trim()
        )
        .into());
    }
    let stdout = String::from_utf8(stdout)?;
    serde_json::from_str(stdout.trim()).map_err(Into::into)
}

fn isolapurr_cli_write_succeeded(response: &Value) -> bool {
    (response.get("ok").and_then(Value::as_bool) == Some(true)
        || response.get("accepted").and_then(Value::as_bool) == Some(true))
        && response.get("error").is_none_or(Value::is_null)
}

fn json_u64_any(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_u64))
}

fn json_str_any<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn parse_calibration_channel(
    value: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    match value {
        "rtd" | "rtd-adc" | "temp" | "temperature" => Ok("rtd_adc"),
        "vin" | "vin-adc" | "voltage" | "power" => Ok("vin_adc"),
        _ => Err("calibration channel must be rtd-adc or vin-adc".into()),
    }
}

fn parse_calibration_slot(
    value: &str,
) -> Result<&'static str, Box<dyn std::error::Error + Send + Sync>> {
    match value {
        "a" | "A" => Ok("a"),
        "b" | "B" => Ok("b"),
        _ => Err("calibration slot must be a or b".into()),
    }
}

fn calibration_set_slot_fit_body(
    channel: &str,
    slot: &str,
    gain: f32,
    offset_mv: f32,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    if !gain.is_finite() || gain <= 0.0 {
        return Err("calibration gain must be a finite positive number".into());
    }
    if !offset_mv.is_finite() {
        return Err("calibration offset must be finite".into());
    }
    Ok(json!({
        "op": "set_slot_fit",
        "channel": parse_calibration_channel(channel)?,
        "slot": parse_calibration_slot(slot)?,
        "fit": {
            "gain": gain,
            "offsetMv": offset_mv,
        },
    }))
}

fn calibration_set_active_slot_body(
    channel: &str,
    slot: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(json!({
        "op": "set_active_slot",
        "channel": parse_calibration_channel(channel)?,
        "slot": parse_calibration_slot(slot)?,
    }))
}

fn parse_reference_vin_mv(
    millivolts: Option<u32>,
    volts: Option<&str>,
) -> Result<Option<u32>, Box<dyn std::error::Error + Send + Sync>> {
    if millivolts.is_some() && volts.is_some() {
        return Err("use either --reference-vin-mv or --reference-vin-volts, not both".into());
    }
    if let Some(millivolts) = millivolts {
        return Ok(Some(millivolts));
    }
    volts.map(parse_voltage_to_mv).transpose()
}

#[derive(Debug, Clone)]
struct CalibrationSeriesStats {
    count: u64,
    min: f64,
    max: f64,
    sum: f64,
    first: f64,
    last: f64,
}

impl CalibrationSeriesStats {
    fn new(value: f64) -> Self {
        Self {
            count: 1,
            min: value,
            max: value,
            sum: value,
            first: value,
            last: value,
        }
    }

    fn observe(&mut self, value: f64) {
        self.count = self.count.saturating_add(1);
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
        self.last = value;
    }

    fn to_value(&self) -> Value {
        json!({
            "count": self.count,
            "min": self.min,
            "max": self.max,
            "avg": self.sum / self.count.max(1) as f64,
            "first": self.first,
            "last": self.last,
        })
    }
}

fn observe_series(stats: &mut Option<CalibrationSeriesStats>, value: f64) {
    if let Some(stats) = stats.as_mut() {
        stats.observe(value);
    } else {
        *stats = Some(CalibrationSeriesStats::new(value));
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn slugify_path_component(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.starts_with('-') {
        slug.remove(0);
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "run".to_string()
    } else {
        slug
    }
}

fn require_status_f64(
    status: &Value,
    key: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_f64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing numeric field: {key}"),
        )
        .into()
    })
}

fn require_status_u64(
    status: &Value,
    key: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing integer field: {key}"),
        )
        .into()
    })
}

fn require_status_u16(
    status: &Value,
    key: &str,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let value = require_status_u64(status, key)?;
    u16::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status field out of range: {key}"),
        )
        .into()
    })
}

fn require_status_bool(
    status: &Value,
    key: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    status.get(key).and_then(Value::as_bool).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing boolean field: {key}"),
        )
        .into()
    })
}

fn require_status_i32(
    status: &Value,
    key: &str,
) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let value = status.get(key).and_then(Value::as_i64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status missing integer field: {key}"),
        )
    })?;
    i32::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("status field out of range: {key}"),
        )
        .into()
    })
}

fn status_snapshot(status: &Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let heater_output_percent = require_status_u64(status, "heaterOutputPercent")?;
    Ok(json!({
        "mode": status
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: mode"))?,
        "heaterEnabled": status
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: heaterEnabled"))?,
        "heaterOutputPercent": heater_output_percent,
        "heaterPhysicalOutputPercent": status
            .get("heaterPhysicalOutputPercent")
            .and_then(Value::as_u64)
            .unwrap_or(heater_output_percent),
        "currentTempC": require_status_f64(status, "currentTempC")?,
        "targetTempC": require_status_i32(status, "targetTempC")?,
        "voltageMv": require_status_u64(status, "voltageMv")?,
        "currentMa": require_status_u64(status, "currentMa")?,
        "boardTempCenti": require_status_i32(status, "boardTempCenti")?,
        "rtdRawAdcMv": require_status_u16(status, "rtdRawAdcMv")?,
        "rtdRawAdcMinMv": status.get("rtdRawAdcMinMv").and_then(Value::as_u64),
        "rtdRawAdcMaxMv": status.get("rtdRawAdcMaxMv").and_then(Value::as_u64),
        "rtdRawAdcSpreadMv": status.get("rtdRawAdcSpreadMv").and_then(Value::as_u64),
        "vinRawAdcMv": require_status_u16(status, "vinRawAdcMv")?,
        "pdRequestMv": require_status_u16(status, "pdRequestMv")?,
        "pdContractMv": require_status_u16(status, "pdContractMv")?,
        "pdState": status
            .get("pdState")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: pdState"))?,
        "activeCoolingEnabled": status
            .get("activeCoolingEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: activeCoolingEnabled"))?,
        "fanDisplayState": status
            .get("fanDisplayState")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: fanDisplayState"))?,
        "fanEnabled": status
            .get("fanEnabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status missing field: fanEnabled"))?,
        "fanPwmPermille": require_status_u64(status, "fanPwmPermille")?,
    }))
}
