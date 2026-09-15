use super::*;

pub(crate) fn parse_pps_volts(
    value: &str,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("PPS voltage must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 1
    {
        return Err("PPS voltage must use at most one decimal place".into());
    }

    let whole_mv: u32 = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_mv = if fractional.is_empty() {
        0
    } else {
        u32::from(fractional.as_bytes()[0] - b'0') * 100
    };
    let millivolts = whole_mv.saturating_add(fractional_mv);
    if !(5_000..=28_000).contains(&millivolts) {
        return Err("PPS voltage must stay within the hardware 5.0V to 28.0V range".into());
    }

    Ok(millivolts as u16)
}

pub(crate) fn parse_voltage_to_mv(
    value: &str,
) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("voltage must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 3
    {
        return Err("voltage must use at most three decimal places".into());
    }

    let whole_mv = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_mv = match fractional.len() {
        0 => 0,
        1 => u32::from(fractional.as_bytes()[0] - b'0') * 100,
        2 => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
        }
        _ => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
                + u32::from(fractional.as_bytes()[2] - b'0')
        }
    };
    Ok(whole_mv.saturating_add(fractional_mv))
}

pub(crate) fn parse_pps_amps(value: &str) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err("PPS current must be a positive decimal value".into());
    }

    let (whole, fractional) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 2
    {
        return Err("PPS current must use at most two decimal places".into());
    }

    let whole_ma: u32 = whole.parse::<u32>()?.saturating_mul(1_000);
    let fractional_ma = match fractional.len() {
        0 => 0,
        1 => u32::from(fractional.as_bytes()[0] - b'0') * 100,
        _ => {
            u32::from(fractional.as_bytes()[0] - b'0') * 100
                + u32::from(fractional.as_bytes()[1] - b'0') * 10
        }
    };
    let milliamps = whole_ma.saturating_add(fractional_ma);
    if milliamps == 0 || milliamps > u32::from(u16::MAX) || !milliamps.is_multiple_of(50) {
        return Err("PPS current must be greater than 0A and use 0.05A steps".into());
    }

    Ok(milliamps as u16)
}

pub(crate) fn parse_thermal_targets(
    value: Option<&str>,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    parse_thermal_targets_impl(value, false)
}

#[cfg(test)]
pub(crate) fn parse_thermal_targets_preserve_order(
    value: Option<&str>,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    parse_thermal_targets_impl(value, true)
}

pub(crate) fn parse_thermal_targets_impl(
    value: Option<&str>,
    preserve_input_order: bool,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(value) = value else {
        return Ok(THERMAL_SELF_TEST_DEFAULT_TARGETS_C.to_vec());
    };
    let mut targets = Vec::new();
    for raw in value.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err("thermal targets must not contain empty items".into());
        }
        let target = trimmed.parse::<i16>()?;
        if !THERMAL_SUPPORTED_TARGETS_C.contains(&target) {
            return Err(format!(
                "thermal target {target}C is unsupported; allowed: 60,80,100,120,140,160,180,200,220,240,250"
            )
            .into());
        }
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    if targets.is_empty() {
        return Err("thermal targets must not be empty".into());
    }
    if !preserve_input_order {
        targets.sort_unstable_by_key(|target| {
            THERMAL_SUPPORTED_TARGETS_C
                .iter()
                .position(|candidate| candidate == target)
                .unwrap_or(usize::MAX)
        });
    }
    Ok(targets)
}

pub(crate) fn insert_if_some<T: Serialize>(
    body: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        body.insert(key.to_string(), json!(value));
    }
}

#[cfg(test)]
pub(crate) fn read_artifact_manifest(
    path: &Path,
) -> Result<Vec<FirmwareArtifact>, Box<dyn std::error::Error + Send + Sync>> {
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    if let Ok(catalog) = serde_json::from_value::<FirmwareArtifactCatalog>(value.clone()) {
        return Ok(catalog.artifacts);
    }
    if let Ok(artifact) = serde_json::from_value::<FirmwareArtifact>(value.clone()) {
        return Ok(vec![artifact]);
    }
    serde_json::from_value::<Vec<FirmwareArtifact>>(value).map_err(Into::into)
}

pub(crate) async fn monitor_once(
    client: &Client,
    resolved: ResolvedUsbTarget,
    tail: usize,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let lease = create_lease(client, &resolved).await?;
    let devices_result =
        request_json(client, Method::GET, &resolved.devd, "/api/v1/devices", None).await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    let devices = devices_result?;
    let events = devices
        .get("devices")
        .and_then(Value::as_array)
        .and_then(|devices| {
            devices
                .iter()
                .find(|device| device.get("id").and_then(Value::as_str) == Some(&resolved.device))
        })
        .and_then(|device| device.get("events"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let start = events.len().saturating_sub(tail);
    Ok(json!({"device": resolved.device, "events": &events[start..]}))
}

pub(crate) async fn handle_hardware_command(
    client: &Client,
    default_devd: &str,
    command: HardwareCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        HardwareCommand::Available => {
            let registry = read_hardware_registry()?;
            let devd_devices =
                request_json(client, Method::GET, default_devd, "/api/v1/devices", None)
                    .await
                    .unwrap_or_else(|error| json!({"error": error.to_string()}));
            Ok(json!({
                "path": path_string(hardware_registry_path()?),
                "devd": default_devd,
                "usb": {
                    "devices": devd_devices,
                    "remembered": registry.hardware,
                }
            }))
        }
        HardwareCommand::Recent => {
            let mut registry = read_hardware_registry()?;
            registry.hardware.sort_by_key(|hardware| {
                std::cmp::Reverse(hardware.last_seen_unix_seconds.unwrap_or(0))
            });
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "hardware": registry.hardware}),
            )
        }
        HardwareCommand::List => {
            let registry = read_hardware_registry()?;
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "hardware": registry.hardware}),
            )
        }
        HardwareCommand::Path => Ok(json!({"path": path_string(hardware_registry_path()?)})),
        HardwareCommand::Save {
            id,
            name,
            device,
            devd,
        } => {
            let mut registry = read_hardware_registry()?;
            let hardware = SavedHardware {
                id,
                name,
                transport: SavedTransport::Usb,
                device,
                devd: devd.or_else(|| Some(default_devd.to_string())),
                last_seen_unix_seconds: Some(current_unix_seconds()),
            };
            let saved = upsert_hardware(&mut registry, hardware);
            write_hardware_registry(&registry)?;
            Ok(json!({"path": path_string(hardware_registry_path()?), "hardware": saved}))
        }
        HardwareCommand::Forget { id } => {
            let mut registry = read_hardware_registry()?;
            let before = registry.hardware.len();
            registry.hardware.retain(|hardware| hardware.id != id);
            write_hardware_registry(&registry)?;
            Ok(
                json!({"path": path_string(hardware_registry_path()?), "id": id, "removed": registry.hardware.len() != before}),
            )
        }
    }
}

pub(crate) fn handle_usb_port_command(
    command: UsbPortCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        UsbPortCommand::Set { port } => {
            let mut config = read_user_config().unwrap_or_default();
            config.default_serial_port = Some(port.clone());
            write_user_config(&config)?;
            Ok(
                json!({"ok": true, "defaultSerialPort": port, "configPath": path_string(flux_purr_devd::user_config_path()?)}),
            )
        }
        UsbPortCommand::Show => {
            let config = read_user_config().unwrap_or_default();
            Ok(
                json!({"configPath": path_string(flux_purr_devd::user_config_path()?), "defaultSerialPort": config.default_serial_port}),
            )
        }
    }
}

pub(crate) fn resolve_target(
    selector: TargetSelector,
    default_devd: &str,
) -> Result<ResolvedUsbTarget, Box<dyn std::error::Error + Send + Sync>> {
    match (selector.device, selector.hardware) {
        (Some(_), Some(_)) => Err("command accepts only one of --device or --hardware".into()),
        (Some(device), None) => Ok(ResolvedUsbTarget {
            device,
            devd: default_devd.to_string(),
            hardware_id: None,
        }),
        (None, Some(id)) => {
            let registry = read_hardware_registry()?;
            let hardware = registry
                .hardware
                .iter()
                .find(|hardware| hardware.id == id)
                .ok_or_else(|| format!("saved hardware not found: {id}"))?;
            Ok(ResolvedUsbTarget {
                device: hardware.device.clone(),
                devd: hardware
                    .devd
                    .clone()
                    .unwrap_or_else(|| default_devd.to_string()),
                hardware_id: Some(id),
            })
        }
        (None, None) => Err("command requires --device or --hardware".into()),
    }
}

pub(crate) fn resolve_lan_target(
    id: &str,
) -> Result<LanDeviceConfig, Box<dyn std::error::Error + Send + Sync>> {
    read_user_config()?
        .lan_devices
        .into_iter()
        .find(|device| device.id == id)
        .ok_or_else(|| format!("saved LAN device not found: {id}").into())
}

pub(crate) fn persist_cli_lan_discoveries(
    discoveries: Vec<flux_purr_devd::lan::LanDiscovery>,
) -> Result<Vec<flux_purr_devd::lan::LanDeviceSummary>, Box<dyn std::error::Error + Send + Sync>> {
    let mut config = read_user_config()?;
    let mut summaries = Vec::with_capacity(discoveries.len());
    for discovery in discoveries {
        let Some(device) = device_from_discovery(discovery) else {
            continue;
        };
        let id = device.id.clone();
        merge_lan_device(&mut config.lan_devices, device);
        let saved = config
            .lan_devices
            .iter()
            .find(|candidate| candidate.id == id)
            .ok_or("failed to persist LAN discovery")?;
        summaries.push(flux_purr_devd::lan::LanDeviceSummary::from(saved));
    }
    write_user_config(&config)?;
    Ok(summaries)
}

pub(crate) async fn lan_api_request(
    device: &LanDeviceConfig,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let path = normalize_lan_api_path(path)?;
    let requires_lease = method != Method::GET && path != "leases";
    if !requires_lease {
        return Ok(authorized_json(device, method, &path, None, body).await?);
    }

    let lease = authorized_json(device, Method::POST, "leases", None, None).await?;
    let lease_id = lease
        .get("leaseId")
        .and_then(Value::as_str)
        .ok_or("LAN lease response missing leaseId")?
        .to_owned();
    let result = authorized_json(device, method, &path, Some(&lease_id), body).await;
    let _ = authorized_json(device, Method::DELETE, "leases", Some(&lease_id), None).await;
    Ok(result?)
}

pub(crate) fn normalize_lan_api_path(
    value: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let path = value.trim().trim_matches('/');
    if path.is_empty()
        || path.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
    {
        return Err("LAN API path must be a relative /api/v1 path without traversal".into());
    }
    Ok(path.to_owned())
}

pub(crate) fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

pub(crate) fn read_json_file(
    path: &Path,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub(crate) fn path_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn read_hardware_registry() -> io::Result<HardwareRegistry> {
    let path = hardware_registry_path()?;
    if !path.exists() {
        return Ok(HardwareRegistry::default());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(HardwareRegistry::default());
    }
    serde_json::from_str(&content)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn write_hardware_registry(registry: &HardwareRegistry) -> io::Result<()> {
    let path = hardware_registry_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(registry)?)
}

pub(crate) fn remember_usb(id: &str, device: &str, devd: &str) -> io::Result<()> {
    let mut registry = read_hardware_registry()?;
    upsert_hardware(
        &mut registry,
        SavedHardware {
            id: id.to_string(),
            name: None,
            transport: SavedTransport::Usb,
            device: device.to_string(),
            devd: Some(devd.to_string()),
            last_seen_unix_seconds: Some(current_unix_seconds()),
        },
    );
    write_hardware_registry(&registry)
}

pub(crate) fn upsert_hardware(
    registry: &mut HardwareRegistry,
    mut hardware: SavedHardware,
) -> SavedHardware {
    if let Some(existing) = registry
        .hardware
        .iter_mut()
        .find(|existing| existing.id == hardware.id)
    {
        if hardware.name.is_none() {
            hardware.name = existing.name.clone();
        }
        *existing = hardware.clone();
    } else {
        registry.hardware.push(hardware.clone());
    }
    registry
        .hardware
        .sort_by(|left, right| left.id.cmp(&right.id));
    hardware
}

pub(crate) fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn hardware_registry_schema_version() -> u8 {
    1
}

pub(crate) fn redact_cli_sensitive(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let key_lc = key.to_ascii_lowercase();
                    if matches!(
                        key_lc.as_str(),
                        "password" | "psk" | "passphrase" | "secret" | "token"
                    ) {
                        (key.clone(), Value::String("<redacted>".to_string()))
                    } else {
                        (key.clone(), redact_cli_sensitive(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_cli_sensitive).collect()),
        _ => value.clone(),
    }
}
