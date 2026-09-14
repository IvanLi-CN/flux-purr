#[expect(
    clippy::too_many_lines,
    reason = "CLI workflow or fixture preserves an ordered protocol scenario"
)]
async fn collect_calibration_run(
    client: &Client,
    default_devd: &str,
    args: CalibrationCollectArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let resolved = resolve_target(args.target, default_devd)?;
    let source_current_ma = parse_pps_amps(&args.source_current_a)?;
    let run_started_unix_ms = current_unix_millis();
    let run_id = format!(
        "cal-{}-{}-{}mA",
        run_started_unix_ms,
        slugify_path_component(&resolved.device),
        source_current_ma
    );
    let run_dir = args.output_dir.join(&run_id);
    fs::create_dir_all(&run_dir)?;
    let samples_path = run_dir.join("samples.ndjson");
    let summary_path = run_dir.join("run.json");
    let mut samples_writer = BufWriter::new(File::create(&samples_path)?);

    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());

    let mut stop_reason = None::<&'static str>;
    let mut threshold_sample_index = None::<usize>;
    let mut stopped_sample_index = None::<usize>;
    let mut sample_index = 0usize;
    let mut samples_count = 0usize;
    let mut current_temp_stats: Option<CalibrationSeriesStats> = None;
    let mut voltage_stats: Option<CalibrationSeriesStats> = None;
    let mut current_ma_stats: Option<CalibrationSeriesStats> = None;
    let mut heater_output_stats: Option<CalibrationSeriesStats> = None;
    let mut board_temp_stats: Option<CalibrationSeriesStats> = None;
    let mut rtd_raw_stats: Option<CalibrationSeriesStats> = None;
    let mut vin_raw_stats: Option<CalibrationSeriesStats> = None;
    let mut first_status_snapshot: Option<Value> = None;
    let mut last_status_snapshot: Option<Value> = None;
    let mut heater_started = false;
    let mut heater_stopped = false;
    let mut final_status_snapshot = None::<Value>;
    let mut loop_started = tokio::time::Instant::now();
    let sample_interval = Duration::from_millis(args.sample_interval_ms.max(1));
    let max_runtime = Duration::from_secs(args.max_runtime_seconds.max(1));

    let collect_result = async {
        if !args.dry_run {
            let initial_status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
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
            let body = json!({
                "heaterEnabled": true,
                "targetTempC": args.target_temp_c,
            });
            request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(body),
            )
            .await?;
            heater_started = true;
            let readback = request_leased(
                client,
                &resolved,
                &lease.lease_id,
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
                || readback_target != args.target_temp_c as i32
            {
                return Err("heater start readback did not match requested runtime state".into());
            }
        }

        loop_started = tokio::time::Instant::now();
        let deadline = loop_started + max_runtime;
        let mut next_tick = loop_started;

        loop {
            if tokio::time::Instant::now() >= deadline {
                stop_reason = Some("max_runtime");
                break;
            }

            let status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let current_temp_c = require_status_f64(&status, "currentTempC")?;
            let voltage_mv = require_status_u64(&status, "voltageMv")? as f64;
            let current_ma = require_status_u64(&status, "currentMa")? as f64;
            let heater_output_percent = require_status_u64(&status, "heaterOutputPercent")? as f64;
            let board_temp_centi = require_status_i32(&status, "boardTempCenti")? as f64;
            let rtd_raw_adc_mv = require_status_u16(&status, "rtdRawAdcMv")? as f64;
            let vin_raw_adc_mv = require_status_u16(&status, "vinRawAdcMv")? as f64;

            observe_series(&mut current_temp_stats, current_temp_c);
            observe_series(&mut voltage_stats, voltage_mv);
            observe_series(&mut current_ma_stats, current_ma);
            observe_series(&mut heater_output_stats, heater_output_percent);
            observe_series(&mut board_temp_stats, board_temp_centi);
            observe_series(&mut rtd_raw_stats, rtd_raw_adc_mv);
            observe_series(&mut vin_raw_stats, vin_raw_adc_mv);

            let phase = if args.dry_run {
                "dry_run"
            } else {
                "warmup"
            };
            let status_snapshot = status_snapshot(&status)?;
            if first_status_snapshot.is_none() {
                first_status_snapshot = Some(status_snapshot.clone());
            }
            last_status_snapshot = Some(status_snapshot.clone());
            let captured_at_unix_ms = current_unix_millis();
            let elapsed_ms = captured_at_unix_ms.saturating_sub(run_started_unix_ms);
            let sample = json!({
                "runId": run_id.clone(),
                "sampleIndex": sample_index,
                "capturedAtUnixMs": captured_at_unix_ms,
                "elapsedMs": elapsed_ms,
                "phase": phase,
                "sourceCurrentMa": source_current_ma,
                "status": status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            samples_count += 1;

            if !args.dry_run && current_temp_c >= f64::from(args.stop_temp_c) {
                stop_reason = Some("temperature_threshold");
                threshold_sample_index = Some(sample_index);
                break;
            }

            sample_index = sample_index.saturating_add(1);
            let target_tick = next_tick + sample_interval;
            next_tick = target_tick;
            tokio::time::sleep_until(target_tick).await;
        }

        if !args.dry_run {
            let _ = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(thermal_self_test_runtime_body(false, args.target_temp_c)),
            )
            .await?;
            heater_stopped = true;
            let stop_status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            let stop_snapshot = status_snapshot(&stop_status)?;
            let captured_at_unix_ms = current_unix_millis();
            let elapsed_ms = captured_at_unix_ms.saturating_sub(run_started_unix_ms);
            let sample = json!({
                "runId": run_id.clone(),
                "sampleIndex": sample_index.saturating_add(1),
                "capturedAtUnixMs": captured_at_unix_ms,
                "elapsedMs": elapsed_ms,
                "phase": "stopped",
                "sourceCurrentMa": source_current_ma,
                "status": stop_status,
            });
            writeln!(samples_writer, "{}", serde_json::to_string(&sample)?)?;
            samples_writer.flush()?;
            samples_count += 1;
            stopped_sample_index = Some(sample_index.saturating_add(1));
            final_status_snapshot = Some(stop_snapshot.clone());
            last_status_snapshot = Some(stop_snapshot);
        } else {
            final_status_snapshot = last_status_snapshot.clone();
            stop_reason = Some("max_runtime");
        }

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;

    if heater_started && !heater_stopped {
        let _ = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::PUT,
            "/runtime",
            Some(thermal_self_test_runtime_body(false, args.target_temp_c)),
        )
        .await;
    }

    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();

    collect_result?;

    let duration_ms = current_unix_millis().saturating_sub(run_started_unix_ms);
    let summary = json!({
        "ok": true,
        "runId": run_id.clone(),
        "dryRun": args.dry_run,
        "target": {
            "deviceId": resolved.device.clone(),
            "hardwareId": resolved.hardware_id.clone(),
            "devd": resolved.devd.clone(),
        },
        "source": {
            "deviceId": args.source_device_id,
            "mode": "manual_cc",
            "currentMa": source_current_ma,
        },
        "parameters": {
            "targetTempC": args.target_temp_c,
            "stopTempC": args.stop_temp_c,
            "sampleIntervalMs": args.sample_interval_ms.max(1),
            "maxRuntimeSeconds": args.max_runtime_seconds.max(1),
        },
        "files": {
            "runDir": run_dir,
            "summaryPath": summary_path,
            "samplesPath": samples_path,
        },
        "sampleCount": samples_count,
        "durationMs": duration_ms,
        "stopReason": stop_reason.unwrap_or("max_runtime"),
        "complete": args.dry_run || stop_reason == Some("temperature_threshold"),
        "thresholdSampleIndex": threshold_sample_index,
        "stoppedSampleIndex": stopped_sample_index,
        "startStatus": first_status_snapshot,
        "finalStatus": final_status_snapshot,
        "stats": {
            "currentTempC": current_temp_stats.map(|stats| stats.to_value()),
            "voltageMv": voltage_stats.map(|stats| stats.to_value()),
            "currentMa": current_ma_stats.map(|stats| stats.to_value()),
            "heaterOutputPercent": heater_output_stats.map(|stats| stats.to_value()),
            "boardTempCenti": board_temp_stats.map(|stats| stats.to_value()),
            "rtdRawAdcMv": rtd_raw_stats.map(|stats| stats.to_value()),
            "vinRawAdcMv": vin_raw_stats.map(|stats| stats.to_value()),
        }
    });

    fs::write(&summary_path, serde_json::to_vec_pretty(&summary)?)?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(summary)
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
