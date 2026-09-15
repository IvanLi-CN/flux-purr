use super::*;

pub(crate) fn render_human(
    payload: &Value,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
