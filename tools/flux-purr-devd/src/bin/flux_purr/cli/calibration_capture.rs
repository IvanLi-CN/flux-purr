struct CalibrationRunContext<'a> {
    client: &'a Client,
    resolved: &'a ResolvedUsbTarget,
    lease: &'a Lease,
    args: &'a CalibrationCollectArgs,
    run_id: &'a str,
    source_current_ma: u16,
    run_started_unix_ms: u64,
    sample_interval: Duration,
    max_runtime: Duration,
}

#[derive(Default)]
struct CalibrationCollectionState {
    stop_reason: Option<&'static str>,
    threshold_sample_index: Option<usize>,
    stopped_sample_index: Option<usize>,
    sample_index: usize,
    samples_count: usize,
    current_temp_stats: Option<CalibrationSeriesStats>,
    voltage_stats: Option<CalibrationSeriesStats>,
    current_ma_stats: Option<CalibrationSeriesStats>,
    heater_output_stats: Option<CalibrationSeriesStats>,
    board_temp_stats: Option<CalibrationSeriesStats>,
    rtd_raw_stats: Option<CalibrationSeriesStats>,
    vin_raw_stats: Option<CalibrationSeriesStats>,
    first_status_snapshot: Option<Value>,
    last_status_snapshot: Option<Value>,
    heater_started: bool,
    heater_stopped: bool,
    final_status_snapshot: Option<Value>,
}

async fn collect_calibration_run(
    client: &Client,
    default_devd: &str,
    args: CalibrationCollectArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target.clone(), default_devd)?;
    let source_current_ma = parse_pps_amps(&args.source_current_a)?;
    let run_started_unix_ms = current_unix_millis();
    let run_id = calibration_run_id(run_started_unix_ms, &resolved, source_current_ma);
    let run_dir = args.output_dir.join(&run_id);
    fs::create_dir_all(&run_dir)?;
    let samples_path = run_dir.join("samples.ndjson");
    let summary_path = run_dir.join("run.json");
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let context = CalibrationRunContext {
        client,
        resolved: &resolved,
        lease: &lease,
        args: &args,
        run_id: &run_id,
        source_current_ma,
        run_started_unix_ms,
        sample_interval: Duration::from_millis(args.sample_interval_ms.max(1)),
        max_runtime: Duration::from_secs(args.max_runtime_seconds.max(1)),
    };
    let mut state = CalibrationCollectionState::default();
    let collect_result = run_calibration_collection(&context, &mut state, &mut samples_writer).await;
    cleanup_calibration_heater(&context, &state).await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    collect_result?;
    let summary = calibration_summary(&context, &state, &run_dir, &summary_path, &samples_path);
    fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(summary)
}

fn calibration_run_id(started_unix_ms: u64, resolved: &ResolvedUsbTarget, current_ma: u16) -> String {
    format!(
        "cal-{}-{}-{}mA",
        started_unix_ms,
        slugify_path_component(&resolved.device),
        current_ma
    )
}

async fn run_calibration_collection(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
    samples_writer: &mut BufWriter<File>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    start_calibration_heater(context, state).await?;
    collect_calibration_samples(context, state, samples_writer).await?;
    stop_calibration_heater(context, state, samples_writer).await
}

async fn start_calibration_heater(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if context.args.dry_run {
        return Ok(());
    }
    let initial_status = request_leased(
        context.client,
        context.resolved,
        &context.lease.lease_id,
        Method::GET,
        "/status",
        None,
    )
    .await?;
    let initial_current_temp = require_status_f64(&initial_status, "currentTempC")?;
    if initial_current_temp > 40.0 {
        return Err(format!(
            "calibration collect requires room-temperature start (<= 40C), got {initial_current_temp:.1}C"
        )
        .into());
    }
    request_leased(
        context.client,
        context.resolved,
        &context.lease.lease_id,
        Method::PUT,
        "/runtime",
        Some(json!({
            "heaterEnabled": true,
            "targetTempC": context.args.target_temp_c,
        })),
    )
    .await?;
    state.heater_started = true;
    verify_calibration_heater_start(context).await
}

async fn verify_calibration_heater_start(
    context: &CalibrationRunContext<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let readback = request_leased(
        context.client,
        context.resolved,
        &context.lease.lease_id,
        Method::GET,
        "/status",
        None,
    )
    .await?;
    let readback_target = require_status_i32(&readback, "targetTempC")?;
    if !readback
        .get("heaterEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || readback_target != i32::from(context.args.target_temp_c)
    {
        return Err("heater start readback did not match requested runtime state".into());
    }
    Ok(())
}

async fn collect_calibration_samples(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
    samples_writer: &mut BufWriter<File>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let loop_started = tokio::time::Instant::now();
    let deadline = loop_started + context.max_runtime;
    let mut next_tick = loop_started;
    loop {
        if tokio::time::Instant::now() >= deadline {
            state.stop_reason = Some("max_runtime");
            break;
        }
        let status = request_leased(
            context.client,
            context.resolved,
            &context.lease.lease_id,
            Method::GET,
            "/status",
            None,
        )
        .await?;
        let current_temp_c = record_calibration_sample(context, state, samples_writer, &status, "warmup")?;
        if !context.args.dry_run && current_temp_c >= f64::from(context.args.stop_temp_c) {
            state.stop_reason = Some("temperature_threshold");
            state.threshold_sample_index = Some(state.sample_index);
            break;
        }
        state.sample_index = state.sample_index.saturating_add(1);
        next_tick += context.sample_interval;
        tokio::time::sleep_until(next_tick).await;
    }
    Ok(())
}

fn record_calibration_sample(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
    samples_writer: &mut BufWriter<File>,
    status: &Value,
    phase: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let current_temp_c = require_status_f64(status, "currentTempC")?;
    observe_calibration_status(state, status)?;
    let status_snapshot = status_snapshot(status)?;
    if state.first_status_snapshot.is_none() {
        state.first_status_snapshot = Some(status_snapshot.clone());
    }
    state.last_status_snapshot = Some(status_snapshot);
    let captured_at_unix_ms = current_unix_millis();
    let sample = json!({
        "runId": context.run_id,
        "sampleIndex": state.sample_index,
        "capturedAtUnixMs": captured_at_unix_ms,
        "elapsedMs": captured_at_unix_ms.saturating_sub(context.run_started_unix_ms),
        "phase": phase,
        "sourceCurrentMa": context.source_current_ma,
        "status": status,
    });
    writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
    samples_writer.flush()?;
    state.samples_count += 1;
    Ok(current_temp_c)
}

fn observe_calibration_status(
    state: &mut CalibrationCollectionState,
    status: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    observe_series(&mut state.current_temp_stats, require_status_f64(status, "currentTempC")?);
    observe_series(&mut state.voltage_stats, require_status_u64(status, "voltageMv")? as f64);
    observe_series(&mut state.current_ma_stats, require_status_u64(status, "currentMa")? as f64);
    observe_series(
        &mut state.heater_output_stats,
        require_status_u64(status, "heaterOutputPercent")? as f64,
    );
    observe_series(
        &mut state.board_temp_stats,
        require_status_i32(status, "boardTempCenti")? as f64,
    );
    observe_series(&mut state.rtd_raw_stats, require_status_u16(status, "rtdRawAdcMv")? as f64);
    observe_series(&mut state.vin_raw_stats, require_status_u16(status, "vinRawAdcMv")? as f64);
    Ok(())
}

async fn stop_calibration_heater(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
    samples_writer: &mut BufWriter<File>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if context.args.dry_run {
        state.final_status_snapshot = state.last_status_snapshot.clone();
        state.stop_reason = Some("max_runtime");
        return Ok(());
    }
    request_leased(
        context.client,
        context.resolved,
        &context.lease.lease_id,
        Method::PUT,
        "/runtime",
        Some(thermal_self_test_runtime_body(false, context.args.target_temp_c)),
    )
    .await?;
    state.heater_stopped = true;
    let stop_status = request_leased(
        context.client,
        context.resolved,
        &context.lease.lease_id,
        Method::GET,
        "/status",
        None,
    )
    .await?;
    state.sample_index = state.sample_index.saturating_add(1);
    write_stopped_calibration_sample(context, state, samples_writer, &stop_status)?;
    state.stopped_sample_index = Some(state.sample_index);
    state.final_status_snapshot = state.last_status_snapshot.clone();
    Ok(())
}

fn write_stopped_calibration_sample(
    context: &CalibrationRunContext<'_>,
    state: &mut CalibrationCollectionState,
    samples_writer: &mut BufWriter<File>,
    status: &Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stop_snapshot = status_snapshot(status)?;
    let captured_at_unix_ms = current_unix_millis();
    let sample = json!({
        "runId": context.run_id,
        "sampleIndex": state.sample_index,
        "capturedAtUnixMs": captured_at_unix_ms,
        "elapsedMs": captured_at_unix_ms.saturating_sub(context.run_started_unix_ms),
        "phase": "stopped",
        "sourceCurrentMa": context.source_current_ma,
        "status": status,
    });
    writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
    samples_writer.flush()?;
    state.samples_count += 1;
    state.last_status_snapshot = Some(stop_snapshot.clone());
    state.final_status_snapshot = Some(stop_snapshot);
    Ok(())
}

async fn cleanup_calibration_heater(
    context: &CalibrationRunContext<'_>,
    state: &CalibrationCollectionState,
) {
    if state.heater_started && !state.heater_stopped {
        let _ = request_leased(
            context.client,
            context.resolved,
            &context.lease.lease_id,
            Method::PUT,
            "/runtime",
            Some(thermal_self_test_runtime_body(false, context.args.target_temp_c)),
        )
        .await;
    }
}

fn calibration_summary(
    context: &CalibrationRunContext<'_>,
    state: &CalibrationCollectionState,
    run_dir: &Path,
    summary_path: &Path,
    samples_path: &Path,
) -> Value {
    json!({
        "ok": true,
        "runId": context.run_id,
        "dryRun": context.args.dry_run,
        "target": {
            "deviceId": context.resolved.device,
            "hardwareId": context.resolved.hardware_id,
            "devd": context.resolved.devd,
        },
        "source": {"deviceId": context.args.source_device_id, "mode": "manual_cc", "currentMa": context.source_current_ma},
        "parameters": {
            "targetTempC": context.args.target_temp_c,
            "stopTempC": context.args.stop_temp_c,
            "sampleIntervalMs": context.args.sample_interval_ms.max(1),
            "maxRuntimeSeconds": context.args.max_runtime_seconds.max(1),
        },
        "files": {"runDir": run_dir, "summaryPath": summary_path, "samplesPath": samples_path},
        "sampleCount": state.samples_count,
        "durationMs": current_unix_millis().saturating_sub(context.run_started_unix_ms),
        "stopReason": state.stop_reason.unwrap_or("max_runtime"),
        "complete": context.args.dry_run || state.stop_reason == Some("temperature_threshold"),
        "thresholdSampleIndex": state.threshold_sample_index,
        "stoppedSampleIndex": state.stopped_sample_index,
        "startStatus": state.first_status_snapshot,
        "finalStatus": state.final_status_snapshot,
        "stats": calibration_stats_value(state),
    })
}

fn calibration_stats_value(state: &CalibrationCollectionState) -> Value {
    json!({
        "currentTempC": state.current_temp_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "voltageMv": state.voltage_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "currentMa": state.current_ma_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "heaterOutputPercent": state.heater_output_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "boardTempCenti": state.board_temp_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "rtdRawAdcMv": state.rtd_raw_stats.as_ref().map(CalibrationSeriesStats::to_value),
        "vinRawAdcMv": state.vin_raw_stats.as_ref().map(CalibrationSeriesStats::to_value),
    })
}

async fn create_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
) -> Result<Lease, Box<dyn std::error::Error + Send + Sync>> {
    let path = format!(
        "/api/v1/devices/{}/leases",
        encode_path_segment(&resolved.device)
    );
    let mut last_device_not_found = None::<String>;
    for _attempt in 0..20 {
        match request_json(client, Method::POST, &resolved.devd, &path, None).await {
            Ok(value) => return Ok(serde_json::from_value(value)?),
            Err(error) if error.to_string().contains("status=404") => {
                last_device_not_found = Some(error.to_string());
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(format!(
        "create lease failed for {} after waiting for native device refresh: {}",
        resolved.device,
        last_device_not_found.unwrap_or_else(|| "device_not_found".to_string())
    )
    .into())
}

async fn create_ready_thermal_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
) -> Result<(Lease, Value), Box<dyn std::error::Error + Send + Sync>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut last_error = "device did not become ready".to_string();
    while tokio::time::Instant::now() < deadline {
        match try_create_ready_thermal_lease(client, resolved).await {
            Ok(Some(ready)) => return Ok(ready),
            Ok(None) => {
                last_error = "thermal readiness status did not confirm heater off".into();
            }
            Err(error) => last_error = error.to_string(),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!(
        "thermal device readiness handshake timed out for {}: {last_error}",
        resolved.device
    )
    .into())
}

async fn try_create_ready_thermal_lease(
    client: &Client,
    resolved: &ResolvedUsbTarget,
) -> Result<Option<(Lease, Value)>, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, resolved).await?;
    let mut status = match request_thermal_status_with_retry(client, resolved, &lease.lease_id).await
    {
        Ok(status) => status,
        Err(error) => {
            let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
            return Err(error);
        }
    };
    if status.get("heaterEnabled").and_then(Value::as_bool) == Some(true) {
        force_thermal_self_test_shutdown(client, resolved, &lease.lease_id).await?;
        status = request_thermal_status_with_retry(client, resolved, &lease.lease_id).await?;
    }
    if status.get("heaterEnabled").and_then(Value::as_bool) == Some(false) {
        return Ok(Some((lease, status)));
    }
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    Ok(None)
}

async fn release_lease(
    client: &Client,
    devd: &str,
    lease_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = request_json(
        client,
        Method::DELETE,
        devd,
        &format!("/api/v1/leases/{lease_id}?lease_id={lease_id}"),
        None,
    )
    .await?;
    Ok(())
}

async fn force_thermal_self_test_shutdown(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    lease_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cleanup_body = thermal_self_test_cooldown_runtime_body();
    if request_thermal_runtime_with_retry(client, resolved, lease_id, cleanup_body.clone())
        .await
        .is_ok()
    {
        return Ok(());
    }

    let _ = release_lease(client, &resolved.devd, lease_id).await;
    let recovery_lease = create_lease(client, resolved).await?;
    let recovery_heartbeat = spawn_heartbeat(
        client.clone(),
        resolved.devd.clone(),
        recovery_lease.clone(),
    );
    let result = request_thermal_runtime_with_retry(
        client,
        resolved,
        &recovery_lease.lease_id,
        cleanup_body,
    )
    .await;
    let _ = release_lease(client, &resolved.devd, &recovery_lease.lease_id).await;
    recovery_heartbeat.abort();
    result.map(|_| ())
}

fn spawn_heartbeat(client: Client, devd: String, lease: Lease) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval_ms = (lease.ttl_ms / 2).max(500);
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            if request_json(
                &client,
                Method::POST,
                &devd,
                &format!("/api/v1/leases/{}/heartbeat", lease.lease_id),
                None,
            )
            .await
            .is_err()
            {
                break;
            }
        }
    })
}
