use super::*;

pub(crate) async fn handle_calibration_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationCommand::Get(selector) => get_calibration(client, default_devd, selector).await,
        CalibrationCommand::Capture(args) => capture_calibration(client, default_devd, args).await,
        CalibrationCommand::Delete(args) => delete_calibration(client, default_devd, args).await,
        CalibrationCommand::Clear(args) => clear_calibration(client, default_devd, args).await,
        CalibrationCommand::SetSlotFit(args) => {
            set_calibration_slot_fit(client, default_devd, args).await
        }
        CalibrationCommand::SetActiveSlot(args) => {
            set_active_calibration_slot(client, default_devd, args).await
        }
        CalibrationCommand::Import(args) => import_calibration(client, default_devd, args).await,
        CalibrationCommand::Export(args) => export_calibration(client, default_devd, args).await,
        CalibrationCommand::Collect(args) => {
            collect_calibration_run(client, default_devd, args).await
        }
    }
}

pub(crate) async fn get_calibration(
    client: &Client,
    default_devd: &str,
    selector: TargetSelector,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    request_with_lease(
        client,
        resolve_target(selector, default_devd)?,
        Method::GET,
        "/calibration",
        None,
    )
    .await
}

pub(crate) async fn capture_calibration(
    client: &Client,
    default_devd: &str,
    args: CalibrationCaptureArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut body = serde_json::Map::new();
    body.insert("op".to_string(), json!("capture"));
    body.insert(
        "channel".to_string(),
        json!(parse_calibration_channel(&args.channel)?),
    );
    insert_if_some(&mut body, "referenceTempC", args.reference_temp_c);
    insert_if_some(
        &mut body,
        "referenceVinMv",
        parse_reference_vin_mv(args.reference_vin_mv, args.reference_vin_volts.as_deref())?,
    );
    insert_if_some(&mut body, "observedMv", args.observed_mv);
    insert_if_some(&mut body, "expectedMv", args.expected_mv);
    put_calibration(client, default_devd, args.target, Value::Object(body)).await
}

pub(crate) async fn delete_calibration(
    client: &Client,
    default_devd: &str,
    args: CalibrationDeleteArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let body = json!({"op": "delete", "channel": parse_calibration_channel(&args.channel)?, "sampleIndex": args.sample_index});
    put_calibration(client, default_devd, args.target, body).await
}

pub(crate) async fn clear_calibration(
    client: &Client,
    default_devd: &str,
    args: CalibrationChannelArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let body = json!({"op": "clear", "channel": parse_calibration_channel(&args.channel)?});
    put_calibration(client, default_devd, args.target, body).await
}

pub(crate) async fn set_calibration_slot_fit(
    client: &Client,
    default_devd: &str,
    args: CalibrationSetSlotFitArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let body = calibration_set_slot_fit_body(&args.channel, &args.slot, args.gain, args.offset_mv)?;
    put_calibration(client, default_devd, args.target, body).await
}

pub(crate) async fn set_active_calibration_slot(
    client: &Client,
    default_devd: &str,
    args: CalibrationSetActiveSlotArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let body = calibration_set_active_slot_body(&args.channel, &args.slot)?;
    put_calibration(client, default_devd, args.target, body).await
}

pub(crate) async fn import_calibration(
    client: &Client,
    default_devd: &str,
    args: CalibrationImportArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
    put_calibration(
        client,
        default_devd,
        args.target,
        json!({"op": "import", "state": imported}),
    )
    .await
}

pub(crate) async fn export_calibration(
    client: &Client,
    default_devd: &str,
    args: CalibrationExportArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let payload = get_calibration(client, default_devd, args.target).await?;
    if let Some(parent) = args
        .file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.file, serde_json::to_vec_pretty(&payload)?)?;
    Ok(json!({"ok": true, "path": args.file}))
}

pub(crate) async fn put_calibration(
    client: &Client,
    default_devd: &str,
    selector: TargetSelector,
    body: Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    request_with_lease(
        client,
        resolve_target(selector, default_devd)?,
        Method::PUT,
        "/calibration",
        Some(body),
    )
    .await
}

pub(crate) async fn handle_calibration_mode_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationModeCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationModeCommand::Status(selector) => {
            let payload = request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::GET,
                "/status",
                None,
            )
            .await?;
            Ok(payload.get("calibration").cloned().unwrap_or(Value::Null))
        }
        CalibrationModeCommand::Exit(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "mode": "off",
                        "ppsEnabled": false,
                        "heaterEnabled": false
                    }
                })),
            )
            .await
        }
        CalibrationModeCommand::Voltage { command } => {
            handle_voltage_calibration_command(client, default_devd, command).await
        }
        CalibrationModeCommand::Temperature { command } => {
            handle_temperature_calibration_command(client, default_devd, command).await
        }
        CalibrationModeCommand::HeaterCurve { command } => {
            handle_heater_curve_calibration_command(client, default_devd, command).await
        }
    }
}

pub(crate) async fn handle_voltage_calibration_command(
    client: &Client,
    default_devd: &str,
    command: VoltageCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        VoltageCalibrationCommand::Enter(args) => {
            let calibration =
                calibration_pps_payload("vin_adc", args.volts.as_deref(), args.heater_enabled)?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        VoltageCalibrationCommand::Set(args) => {
            let calibration = calibration_pps_payload_partial(args.volts.as_str())?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        VoltageCalibrationCommand::Step(args) => {
            let resolved = resolve_target(args.target, default_devd)?;
            let status =
                request_with_lease(client, resolved.clone(), Method::GET, "/status", None).await?;
            let current_mv = status
                .get("calibration")
                .and_then(|value| value.get("ppsMv"))
                .and_then(Value::as_u64)
                .or_else(|| status.get("manualPpsMv").and_then(Value::as_u64))
                .ok_or("calibration PPS voltage is unavailable")?;
            let next_mv = stepped_pps_mv(current_mv as u16, args.delta_v)?;
            request_with_lease(
                client,
                resolved,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "ppsEnabled": true,
                        "ppsMv": next_mv
                    }
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Capture(args) => {
            let reference_vin_mv = parse_reference_vin_mv(args.millivolts, args.volts.as_deref())?
                .ok_or("voltage capture requires --volts or --millivolts")?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(json!({
                    "op": "capture",
                    "channel": "vin_adc",
                    "referenceVinMv": reference_vin_mv
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Auto(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::POST,
                "/calibration/job",
                Some(json!({
                    "op": "start",
                    "kind": "vin_adc_auto"
                })),
            )
            .await
        }
        VoltageCalibrationCommand::Job { command } => {
            handle_calibration_job_command(client, default_devd, command).await
        }
    }
}

pub(crate) async fn handle_temperature_calibration_command(
    client: &Client,
    default_devd: &str,
    command: TemperatureCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        TemperatureCalibrationCommand::Enter(args) => {
            let mut calibration =
                calibration_pps_payload("rtd_adc", args.volts.as_deref(), args.heater_enabled)?;
            if let Some(target_adc_mv) = args.target_adc_mv {
                calibration["targetAdcMv"] = json!(target_adc_mv);
            }
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        TemperatureCalibrationCommand::SetTarget(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "targetAdcMv": args.target_adc_mv
                    }
                })),
            )
            .await
        }
        TemperatureCalibrationCommand::Heater(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "heaterEnabled": args.enabled
                    }
                })),
            )
            .await
        }
        TemperatureCalibrationCommand::Capture(args) => {
            let mut body = serde_json::Map::new();
            body.insert("op".to_string(), json!("capture"));
            body.insert("channel".to_string(), json!("rtd_adc"));
            body.insert("referenceTempC".to_string(), json!(args.reference_temp_c));
            insert_if_some(&mut body, "observedMv", args.observed_mv);
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/calibration",
                Some(Value::Object(body)),
            )
            .await
        }
    }
}

pub(crate) async fn handle_heater_curve_calibration_command(
    client: &Client,
    default_devd: &str,
    command: HeaterCurveCalibrationCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HeaterCurveCalibrationCommand::Enter(args) => {
            let calibration = calibration_pps_payload(
                "heater_curve",
                args.volts.as_deref(),
                args.heater_enabled,
            )?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Set(args) => {
            let calibration = calibration_pps_payload_partial(args.volts.as_str())?;
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({ "calibration": calibration })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Heater(args) => {
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/runtime",
                Some(json!({
                    "calibration": {
                        "heaterEnabled": args.enabled
                    }
                })),
            )
            .await
        }
        HeaterCurveCalibrationCommand::Job { command } => {
            handle_calibration_job_command(client, default_devd, command).await
        }
    }
}

pub(crate) async fn handle_calibration_job_command(
    client: &Client,
    default_devd: &str,
    command: CalibrationJobCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        CalibrationJobCommand::Status(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::GET,
                "/calibration/job",
                None,
            )
            .await
        }
        CalibrationJobCommand::Cancel(target) => {
            request_with_lease(
                client,
                resolve_target(target, default_devd)?,
                Method::POST,
                "/calibration/job",
                Some(json!({ "op": "cancel" })),
            )
            .await
        }
    }
}

pub(crate) fn calibration_pps_payload(
    mode: &'static str,
    volts: Option<&str>,
    heater_enabled: Option<bool>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut payload = calibration_pps_payload_partial_opt(volts)?;
    payload["mode"] = json!(mode);
    if let Some(heater_enabled) = heater_enabled {
        payload["heaterEnabled"] = json!(heater_enabled);
    }
    Ok(payload)
}

pub(crate) fn calibration_pps_payload_partial(
    volts: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    calibration_pps_payload_partial_opt(Some(volts))
}

pub(crate) fn calibration_pps_payload_partial_opt(
    volts: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut payload = serde_json::Map::new();
    if let Some(volts) = volts {
        payload.insert("ppsEnabled".to_string(), json!(true));
        payload.insert("ppsMv".to_string(), json!(parse_pps_volts(volts)?));
    }
    Ok(Value::Object(payload))
}

pub(crate) fn stepped_pps_mv(
    current_mv: u16,
    delta_v: i16,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let stepped = i32::from(current_mv) + i32::from(delta_v) * 1_000;
    if !(5_000..=28_000).contains(&stepped) {
        return Err("PPS voltage step must stay within 5V..28V.".into());
    }
    Ok(stepped as u16)
}

pub(crate) async fn handle_heater_curve_command(
    client: &Client,
    default_devd: &str,
    command: HeaterCurveCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HeaterCurveCommand::Get(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::GET,
                "/heater-curve",
                None,
            )
            .await
        }
        HeaterCurveCommand::Preview(args) => {
            let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
            let package = imported
                .get("active")
                .cloned()
                .or_else(|| imported.get("package").cloned())
                .unwrap_or(imported);
            request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::PUT,
                "/heater-curve",
                Some(json!({
                    "op": "preview",
                    "package": package,
                })),
            )
            .await
        }
        HeaterCurveCommand::ClearPreview(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::PUT,
                "/heater-curve",
                Some(json!({
                    "op": "clear_preview",
                })),
            )
            .await
        }
        HeaterCurveCommand::Save(selector) => {
            request_with_lease(
                client,
                resolve_target(selector, default_devd)?,
                Method::POST,
                "/heater-curve/save",
                Some(json!({})),
            )
            .await
        }
        HeaterCurveCommand::Export(args) => {
            let payload = request_with_lease(
                client,
                resolve_target(args.target, default_devd)?,
                Method::GET,
                "/heater-curve",
                None,
            )
            .await?;
            if let Some(parent) = args
                .file
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.file, serde_json::to_vec_pretty(&payload)?)?;
            Ok(json!({
                "ok": true,
                "path": args.file,
            }))
        }
    }
}

pub(crate) const THERMAL_SUPPORTED_TARGETS_C: [i16; 11] =
    [60, 80, 100, 120, 140, 160, 180, 200, 220, 240, 250];
pub(crate) const THERMAL_PROFILE_ANCHOR_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 250];
pub(crate) const THERMAL_SELF_TEST_DEFAULT_TARGETS_C: [i16; 3] = [60, 140, 220];
pub(crate) const THERMAL_CONTROL_PROFILE_MAX_POINTS: usize = 10;
pub(crate) const THERMAL_APPROACH_CURVE_PREFERRED_MS: u64 = 5_000;
pub(crate) const THERMAL_APPROACH_CURVE_LIMIT_MS: u64 = 10_000;
pub(crate) const THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C: f64 = 0.5;
pub(crate) const THERMAL_APPROACH_CURVE_CLASS_MARGIN_C: f64 = 0.4;

pub(crate) fn thermal_profile_preview_runtime_body(
    mode: ThermalProfileMode,
    profile: Value,
) -> Value {
    json!({
        "thermalProfileMode": mode.as_str(),
        "thermalControlProfile": {
            "op": "preview",
            "profile": profile,
        }
    })
}

pub(crate) fn expected_thermal_profile_mode_bank(
    status: &Value,
    expected_mode: ThermalProfileMode,
) -> Result<&str, Box<dyn std::error::Error + Send + Sync>> {
    match expected_mode {
        ThermalProfileMode::W65 => Ok("pps3a"),
        ThermalProfileMode::W100 => Ok("pps5a"),
        ThermalProfileMode::Auto => status
            .get("thermalProfileResolvedBank")
            .and_then(Value::as_str)
            .ok_or("status missing thermalProfileResolvedBank".into()),
    }
}

pub(crate) async fn request_thermal_profile_persist_with_resolved_bank(
    client: &Client,
    resolved: ResolvedUsbTarget,
    profile_mode: ThermalProfileMode,
    op: &'static str,
    profile: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = async {
        let bank = if let Some(bank) = profile_mode.explicit_bank() {
            bank.to_string()
        } else {
            let status = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::PUT,
                "/runtime",
                Some(json!({ "thermalProfileMode": profile_mode.as_str() })),
            )
            .await?;
            expected_thermal_profile_mode_bank(&status, profile_mode)?.to_string()
        };
        let mut thermal_control_profile = json!({
            "op": op,
            "bank": bank,
        });
        if let Some(profile) = profile
            && let Some(object) = thermal_control_profile.as_object_mut()
        {
            let profile = if op == "save" {
                thermal_candidate_profile_to_value(&thermal_profile_for_persistence(
                    &thermal_candidate_profile_from_value(profile),
                )?)
            } else {
                profile
            };
            object.insert("profile".to_string(), profile);
        }
        request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::PUT,
            "/runtime",
            Some(json!({
                "thermalProfileMode": profile_mode.as_str(),
                "thermalControlProfile": thermal_control_profile,
            })),
        )
        .await
    }
    .await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let payload = result?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(payload)
}
