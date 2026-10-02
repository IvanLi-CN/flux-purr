use super::*;

pub(crate) fn render_human(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if payload.get("firmwareKind").and_then(Value::as_str) == Some("ram_bringup")
        && payload.get("capability").is_some()
    {
        return render_ram_response(payload);
    }
    if matches!(
        payload.get("operation").and_then(Value::as_str),
        Some("flash" | "recover")
    ) && payload.get("espflash").is_some()
    {
        return render_espflash_human(payload);
    }
    if payload.get("active").and_then(Value::as_bool).is_some() {
        return render_pairing_code(payload);
    }
    if let Some(devices) = payload.get("devices").and_then(Value::as_array) {
        return Ok(format!("Devices: {}", devices.len()));
    }
    if let Some(device) = payload.get("deviceId").and_then(Value::as_str) {
        return render_device_status(payload, device);
    }
    if payload.get("artifactId").is_some() && payload.get("status").is_some() {
        return Ok(format!(
            "Flash {}: {}",
            payload
                .get("artifactId")
                .and_then(Value::as_str)
                .unwrap_or("-"),
            payload.get("status").and_then(Value::as_str).unwrap_or("-")
        ));
    }
    if payload.get("rtdAdc").is_some() && payload.get("vinAdc").is_some() {
        return render_calibration_summary(payload);
    }
    if payload.get("kind").and_then(Value::as_str) == Some("thermal_self_test") {
        return render_thermal_summary(payload, "Thermal self-test");
    }
    if payload.get("kind").and_then(Value::as_str) == Some("thermal_self_test_replay") {
        return render_thermal_summary(payload, "Thermal replay");
    }
    if payload.get("operation").and_then(Value::as_str)
        == Some("thermal_report.rerender_legacy_preliminary_review_bundle")
    {
        return Ok(format!(
            "Thermal report bundle: {}",
            payload
                .get("bundleIndexHtml")
                .and_then(Value::as_str)
                .unwrap_or("-")
        ));
    }
    if payload.get("runId").is_some() && payload.get("sampleCount").is_some() {
        return render_calibration_run(payload);
    }
    if payload.get("hardware").is_some() || payload.get("usb").is_some() {
        return Ok(serde_json::to_string_pretty(&redact_cli_sensitive(
            payload,
        ))?);
    }
    if payload.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok("OK".to_string());
    }
    Ok(serde_json::to_string_pretty(&redact_cli_sensitive(
        payload,
    ))?)
}

fn render_ram_response(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let status = if payload.get("ok").and_then(Value::as_bool) == Some(true) {
        "PASS"
    } else {
        "FAIL"
    };
    let capability = payload
        .get("capability")
        .and_then(Value::as_str)
        .unwrap_or("-");
    if status == "PASS" {
        super::ram_run::validate_ram_success_response(payload, capability)
            .map_err(|error| format!("invalid RAM success response: {error}"))?;
    }
    if capability == "test_pd_sink" {
        return render_pd_hil_summary(payload);
    }
    let result = payload.get("result").unwrap_or(&Value::Null);
    let detail = result.get("detail").and_then(Value::as_str).unwrap_or("-");
    let mut output = format!(
        "RAM {status} capability={capability} detail={detail} heater={} pd={} eeprom={}",
        result.get("heater").and_then(Value::as_str).unwrap_or("-"),
        result.get("pd").and_then(Value::as_str).unwrap_or("-"),
        result.get("eeprom").and_then(Value::as_str).unwrap_or("-"),
    );
    if let Some(effect) = result.get("effect").and_then(Value::as_str) {
        output.push_str(&format!(" effect={effect}"));
    }
    if let Some(buttons) = result.get("buttons").and_then(Value::as_object) {
        output.push_str(" buttons=");
        for (index, name) in ["center", "right", "down", "left", "up"]
            .into_iter()
            .enumerate()
        {
            if index != 0 {
                output.push(',');
            }
            let state = if buttons.get(name).and_then(Value::as_bool).unwrap_or(false) {
                "pressed"
            } else {
                "released"
            };
            output.push_str(&format!("{name}={state}"));
        }
    }
    if let Some(interaction) = result.get("interaction").and_then(Value::as_object) {
        let events = interaction
            .get("events")
            .and_then(Value::as_array)
            .map(|events| {
                events
                    .iter()
                    .filter_map(|event| {
                        let key = event.get("key").and_then(Value::as_str)?;
                        let gesture = event.get("gesture").and_then(Value::as_str)?;
                        let effect = event.get("effect").and_then(Value::as_str)?;
                        let elapsed_ms = event.get("elapsedMs").and_then(Value::as_u64)?;
                        Some(format!("{key}:{gesture}:{effect}@+{elapsed_ms}ms"))
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let stop_reason = interaction
            .get("stopReason")
            .and_then(Value::as_str)
            .unwrap_or("inactivity_timeout");
        output.push_str(&format!(
            " interaction={stop_reason}:{}s events=[{}]",
            interaction
                .get("inactivityTimeoutSeconds")
                .or_else(|| interaction.get("timeoutSeconds"))
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            events
        ));
    }
    if let Some(adc) = result.get("adc").and_then(Value::as_object) {
        output.push_str(&format!(
            " adc=vin:{} rtd:{}",
            adc.get("vin").and_then(Value::as_u64).unwrap_or_default(),
            adc.get("rtd").and_then(Value::as_u64).unwrap_or_default(),
        ));
    }
    append_ram_power_evidence(&mut output, result);
    if let Some(i2c) = result.get("i2c").and_then(Value::as_object) {
        output.push_str(&format!(
            " i2c=0x{:02x}:0x{:02x}=0x{:02x}",
            i2c.get("address")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            i2c.get("register")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            i2c.get("value").and_then(Value::as_u64).unwrap_or_default(),
        ));
    }
    Ok(output)
}

fn render_pd_hil_summary(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let result = payload
        .get("result")
        .ok_or("PD HIL summary is missing result")?;
    let overall = result
        .get("overall")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_ascii_uppercase();
    let tiers = result
        .get("tiers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = |status: &str| {
        tiers
            .iter()
            .filter(|tier| tier.get("status").and_then(Value::as_str) == Some(status))
            .count()
    };
    let final_reset = result
        .get("finalReset")
        .and_then(Value::as_object)
        .ok_or("PD HIL summary is missing finalReset")?;
    let vin_validation = result
        .get("policy")
        .and_then(Value::as_object)
        .and_then(|policy| policy.get("vinValidation"))
        .and_then(Value::as_str)
        .unwrap_or("adc");
    Ok(format!(
        "PD HIL SUMMARY overall={} vinValidation={} pass={} unsupported={} negotiation_failed={} measurement_failed={} recovery_failed={} reset={} pd={} vbusObservation={}mV evidence={}",
        overall,
        vin_validation,
        count("pass"),
        count("unsupported"),
        count("negotiation_failed"),
        count("measurement_failed"),
        count("recovery_failed"),
        final_reset
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_ascii_uppercase(),
        result.get("pd").and_then(Value::as_str).unwrap_or("-"),
        final_reset
            .get("defaultVbusMv")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        payload
            .get("evidenceDir")
            .and_then(Value::as_str)
            .unwrap_or("-"),
    ))
}

fn append_ram_power_evidence(output: &mut String, result: &Value) {
    let Some(power) = result.get("power").and_then(Value::as_object) else {
        return;
    };
    let measured = power
        .get("measured")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let voltage_ok = power
        .get("voltageOk")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let input_mv = power.get("inputMv").and_then(Value::as_u64).unwrap_or(0);
    let minimum_mv = power
        .get("minimumMv")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if measured && voltage_ok {
        output.push_str(&format!(
            " power=input:{}mV minimum:{}mV status=ok",
            input_mv, minimum_mv
        ));
    } else if measured {
        output.push_str(&format!(
            " WARNING=fan_input_below_minimum power=input:{}mV minimum:{}mV",
            input_mv, minimum_mv
        ));
    } else {
        output.push_str(&format!(
            " WARNING=fan_input_unavailable power=input=unknown minimum:{}mV",
            minimum_mv
        ));
    }
}

pub(crate) fn render_pairing_code(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if payload.get("active").and_then(Value::as_bool) == Some(true) {
        let code = payload
            .get("code")
            .and_then(Value::as_str)
            .ok_or("LAN pairing code response is missing the code")?;
        return Ok(format!("LAN pairing code: {code}"));
    }
    Ok("LAN pairing code is inactive. Open WiFi Info on the device first.".to_string())
}

pub(crate) fn render_device_status(
    payload: &Value,
    device: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(format!(
        "{} target={}C current={}C heater={} cooling={}",
        device,
        payload
            .get("targetTempC")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        payload
            .get("currentTempC")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        payload
            .get("heaterEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        payload
            .get("activeCoolingEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ))
}

pub(crate) fn render_calibration_summary(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let sample_count = |key: &str| {
        payload
            .get(key)
            .and_then(|channel| channel.get("samples"))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter(|item| !item.is_null()).count())
            .unwrap_or(0)
    };
    Ok(format!(
        "Calibration: rtd_adc={} samples vin_adc={} samples",
        sample_count("rtdAdc"),
        sample_count("vinAdc")
    ))
}

pub(crate) fn render_thermal_summary(
    payload: &Value,
    label: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(format!(
        "{} {}: {} samples passed={}",
        label,
        payload.get("runId").and_then(Value::as_str).unwrap_or("-"),
        payload
            .get("sampleCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        payload
            .get("validation")
            .and_then(|validation| validation.get("passed"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ))
}

pub(crate) fn render_calibration_run(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(format!(
        "Calibration run {}: {} samples stop={} complete={}",
        payload.get("runId").and_then(Value::as_str).unwrap_or("-"),
        payload
            .get("sampleCount")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        payload
            .get("stopReason")
            .and_then(Value::as_str)
            .unwrap_or("-"),
        payload
            .get("complete")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ))
}

pub(crate) fn render_espflash_human(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let operation = payload
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("flash");
    let diagnostics = match operation {
        "flash" => vec![(
            "flash",
            payload.get("espflash").ok_or("flash diagnostics missing")?,
        )],
        "recover" => {
            let espflash = payload
                .get("espflash")
                .ok_or("recover diagnostics missing")?;
            vec![
                (
                    "erase",
                    espflash.get("erase").ok_or("erase diagnostics missing")?,
                ),
                (
                    "flash",
                    espflash.get("flash").ok_or("flash diagnostics missing")?,
                ),
            ]
        }
        _ => Vec::new(),
    };
    let mut sections = Vec::with_capacity(diagnostics.len());
    for (label, diagnostic) in diagnostics {
        let phases = diagnostic
            .get("phases")
            .and_then(Value::as_array)
            .map(|phases| {
                phases
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            })
            .filter(|phases| !phases.is_empty())
            .unwrap_or_else(|| "<none observed>".to_string());
        let stdout = diagnostic
            .get("stdout")
            .and_then(Value::as_str)
            .filter(|output| !output.is_empty())
            .unwrap_or("<empty>");
        let stderr = diagnostic
            .get("stderr")
            .and_then(Value::as_str)
            .filter(|output| !output.is_empty())
            .unwrap_or("<empty>");
        sections.push(format!(
            "{}: phases={} final={} diagnosis={}\nstdout:\n{}\nstderr:\n{}",
            label,
            phases,
            diagnostic
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            diagnostic
                .get("diagnosis")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            stdout,
            stderr,
        ));
    }
    Ok(format!(
        "{} completed\n{}",
        operation,
        sections.join("\n\n")
    ))
}
