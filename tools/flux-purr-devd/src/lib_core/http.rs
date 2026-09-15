fn is_allowed_dev_origin(origin: &HeaderValue) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    if authority.contains(['/', '?', '#', '@']) {
        return false;
    }
    is_loopback_origin_authority(authority)
}

fn is_loopback_origin_authority(authority: &str) -> bool {
    if let Some(rest) = authority.strip_prefix("localhost") {
        return has_optional_port(rest);
    }
    if let Some(rest) = authority.strip_prefix("127.0.0.1") {
        return has_optional_port(rest);
    }
    if let Some(rest) = authority.strip_prefix("[::1]") {
        return has_optional_port(rest);
    }
    false
}

fn has_optional_port(rest: &str) -> bool {
    rest.is_empty()
        || rest.strip_prefix(':').is_some_and(|port| {
            !port.is_empty() && port.chars().all(|value| value.is_ascii_digit())
        })
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, HttpError> {
    let state_lock = state.lock()?;
    Ok(Json(json!({
        "name": "flux-purr-devd",
        "version": PRODUCT_VERSION,
        "channel": PRODUCT_CHANNEL,
        "sourceSha": PRODUCT_SOURCE_SHA,
        "buildId": PRODUCT_BUILD_ID,
        "bind": state.config.bind.to_string(),
        "deviceCount": state_lock.devices.len(),
        "limits": {
            "events": DEFAULT_EVENT_LIMIT,
            "logs": DEFAULT_LOG_LIMIT,
            "trace": DEFAULT_TRACE_LIMIT
        }
    })))
}

async fn list_devices(State(state): State<AppState>) -> Result<Json<Value>, HttpError> {
    let serial_devices = scan_serial_devices(state.config.serial_port.as_deref());
    let mut state_lock = state.lock()?;
    refresh_serial_devices(&mut state_lock, serial_devices);
    let devices = state_lock
        .devices
        .values()
        .cloned()
        .map(trim_device_record_for_list)
        .map(device_list_payload)
        .collect::<Vec<_>>();
    Ok(Json(json!({ "devices": devices })))
}

async fn list_lan_devices() -> Result<Json<Value>, HttpError> {
    let config = read_user_config()
        .map_err(|_| HttpError::internal("failed to read local LAN device registry"))?;
    let devices = config
        .lan_devices
        .iter()
        .map(lan::LanDeviceSummary::from)
        .collect::<Vec<_>>();
    Ok(Json(
        json!({ "devices": devices, "discovery": "manual-or-mdns-refresh" }),
    ))
}

async fn refresh_lan_mdns() -> Result<Json<Value>, HttpError> {
    let discovered = lan::discover_mdns(Duration::from_secs(2))
        .await
        .map_err(|error| HttpError::bad_request("lan_mdns_failed", &error.to_string()))?;
    let devices = persist_lan_discoveries(discovered)?;
    Ok(Json(
        json!({ "devices": devices, "source": "explicit_mdns_refresh" }),
    ))
}

async fn scan_lan_cidr(Json(request): Json<lan::LanScanRequest>) -> Result<Json<Value>, HttpError> {
    let discovered = lan::discover_cidr(request)
        .await
        .map_err(|error| HttpError::bad_request("lan_scan_failed", &error.to_string()))?;
    let devices = persist_lan_discoveries(discovered)?;
    Ok(Json(
        json!({ "devices": devices, "source": "explicit_cidr_scan" }),
    ))
}

fn persist_lan_discoveries(
    discoveries: Vec<lan::LanDiscovery>,
) -> Result<Vec<lan::LanDeviceSummary>, HttpError> {
    let mut config = read_user_config()
        .map_err(|_| HttpError::internal("failed to read local LAN device registry"))?;
    let mut summaries = Vec::with_capacity(discoveries.len());
    for discovery in discoveries {
        let Some(device) = lan::device_from_discovery(discovery) else {
            continue;
        };
        let id = device.id.clone();
        lan::merge_lan_device(&mut config.lan_devices, device);
        if let Some(saved) = config
            .lan_devices
            .iter()
            .find(|candidate| candidate.id == id)
        {
            summaries.push(lan::LanDeviceSummary::from(saved));
        }
    }
    write_user_config(&config)
        .map_err(|_| HttpError::internal("failed to persist local LAN device registry"))?;
    Ok(summaries)
}

async fn pair_lan_device(
    Json(request): Json<lan::LanPairRequest>,
) -> Result<Json<lan::LanDeviceSummary>, HttpError> {
    let device = lan::pair_device(request)
        .await
        .map_err(|error| HttpError::bad_request("lan_pairing_failed", &error.to_string()))?;
    let summary = lan::LanDeviceSummary::from(&device);
    let mut config = read_user_config()
        .map_err(|_| HttpError::internal("failed to read local LAN device registry"))?;
    lan::merge_lan_device(&mut config.lan_devices, device);
    write_user_config(&config)
        .map_err(|_| HttpError::internal("failed to persist local LAN device registry"))?;
    Ok(Json(summary))
}

/// Establish a DEVD-owned control route for an already paired LAN device.
/// The browser receives only the verified public record; the pairing token
/// remains in DEVD's local registry and is never serialized by this endpoint.
async fn connect_lan_device(
    State(state): State<AppState>,
    AxumPath(lan_device_id): AxumPath<String>,
) -> Result<Json<Value>, HttpError> {
    let configured = read_user_config()
        .map_err(|_| HttpError::internal("failed to read local LAN device registry"))?
        .lan_devices
        .into_iter()
        .find(|device| device.id == lan_device_id)
        .ok_or_else(|| {
            HttpError::not_found("lan_device_not_found", "LAN device is not registered.")
        })?;

    if configured.pairing_token.is_none() {
        return Err(HttpError::conflict(
            "lan_pairing_required",
            "Pair this LAN device before connecting it through DEVD.",
            json!({ "deviceId": configured.id }),
        ));
    }

    let identity = lan_bridge_read::<Identity>(&configured, "identity").await?;
    validate_lan_bridge_identity(&identity)?;
    let network = lan_bridge_read::<NetworkSummary>(&configured, "network").await?;
    let status = lan_bridge_read::<ControlPlaneStatus>(&configured, "status").await?;
    let bridge_id = bridge_lan_device_id(&configured.id);
    let record = DeviceRecord::lan_bridge(bridge_id.clone(), identity, network, status);

    {
        let mut state_lock = state.lock()?;
        state_lock.devices.insert(bridge_id.clone(), record.clone());
    }
    state.emit(event(
        &bridge_id,
        "lan",
        "DEVD LAN bridge identity verified",
        json!({ "transport": "lan", "lanDeviceId": configured.id }),
    ));
    Ok(Json(device_list_payload(record)))
}

fn bridge_lan_device_id(lan_device_id: &str) -> String {
    format!("devd-{lan_device_id}")
}

fn lan_device_id_for_bridge(device_id: &str) -> Result<&str, HttpError> {
    device_id.strip_prefix("devd-lan-").ok_or_else(|| {
        HttpError::bad_request(
            "invalid_lan_bridge_device",
            "Invalid DEVD LAN bridge device ID.",
        )
    })
}

fn lan_bridge_config(target: &DeviceRecord) -> Result<lan::LanDeviceConfig, HttpError> {
    let lan_device_id = lan_device_id_for_bridge(&target.id)?;
    read_user_config()
        .map_err(|_| HttpError::internal("failed to read local LAN device registry"))?
        .lan_devices
        .into_iter()
        .find(|device| device.id.strip_prefix("lan-") == Some(lan_device_id))
        .ok_or_else(|| {
            HttpError::not_found("lan_device_not_found", "LAN device is not registered.")
        })
}

async fn lan_bridge_read<T: DeserializeOwned>(
    device: &lan::LanDeviceConfig,
    path: &str,
) -> Result<T, HttpError> {
    let value = lan::authorized_json(device, Method::GET, path, None, None)
        .await
        .map_err(lan_bridge_error)?;
    serde_json::from_value(value).map_err(|_| {
        HttpError::bad_request(
            "lan_bridge_invalid_response",
            "LAN device returned an invalid response.",
        )
    })
}

async fn lan_bridge_write<T: DeserializeOwned>(
    device: &lan::LanDeviceConfig,
    path: &str,
    method: Method,
    body: Option<Value>,
) -> Result<T, HttpError> {
    let lease: LanBridgeLease = serde_json::from_value(
        lan::authorized_json(device, Method::POST, "leases", None, None)
            .await
            .map_err(lan_bridge_error)?,
    )
    .map_err(|_| {
        HttpError::bad_request(
            "lan_bridge_invalid_response",
            "LAN device returned an invalid lease response.",
        )
    })?;

    let result = lan::authorized_json(device, method, path, Some(&lease.lease_id), body).await;
    // Release the short-lived remote write lease after the write reaches its
    // terminal HTTP response. A release failure is non-authoritative: the
    // firmware expires the lease and the write result is still device fact.
    let _ = lan::authorized_json(
        device,
        Method::DELETE,
        "leases",
        Some(&lease.lease_id),
        None,
    )
    .await;
    let value = result.map_err(lan_bridge_error)?;
    serde_json::from_value(value).map_err(|_| {
        HttpError::bad_request(
            "lan_bridge_invalid_response",
            "LAN device returned an invalid control response.",
        )
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanBridgeLease {
    lease_id: String,
}

fn lan_bridge_payload<T: Serialize>(payload: &T) -> Result<Value, HttpError> {
    let mut body = serde_json::to_value(payload)
        .map_err(|_| HttpError::internal("failed to encode LAN control request"))?;
    if let Value::Object(fields) = &mut body {
        // The daemon lease authorizes the client-to-DEVD hop. The firmware
        // lease is created above and must be the only lease forwarded.
        fields.remove("leaseId");
    }
    Ok(body)
}

fn lan_bridge_error(error: lan::LanClientError) -> HttpError {
    match error {
        lan::LanClientError::RemoteApi {
            status,
            code,
            message,
            retryable,
        } => HttpError::new(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            &code,
            &message,
            retryable,
        ),
        error => HttpError::bad_request("lan_bridge_request_failed", &error.to_string()),
    }
}

fn validate_lan_bridge_identity(identity: &Identity) -> Result<(), HttpError> {
    let valid = !identity.device_id.trim().is_empty()
        && identity.api_version == "2026-05-29"
        && identity.protocol_version == "flux-purr.usb.v1"
        && ["identity", "network", "status"].iter().all(|capability| {
            identity
                .capabilities
                .iter()
                .any(|value| value == capability)
        });
    if valid {
        Ok(())
    } else {
        Err(HttpError::bad_request(
            "unknown_lan_device",
            "The LAN endpoint did not identify as a compatible Flux Purr device.",
        ))
    }
}

async fn reset_lan_pairing(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<Value>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "LAN pairing reset is available only through an active USB/devd lease.",
        ));
    }
    serial_clear_lan_pairing(&state, &target).await?;
    state.emit(event(
        &device_id,
        "lan",
        "LAN pairing token cleared through USB lease",
        json!({ "token": "<redacted>" }),
    ));
    Ok(Json(json!({ "cleared": true })))
}

async fn get_lan_pairing_code(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<LanPairingCode>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        let target = state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone();
        if target.transport == DeviceTransport::NativeSerial {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        target
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "LAN pairing code is available only through the USB/devd transport.",
        ));
    }
    Ok(Json(serial_lan_pairing_code(&state, &target).await?))
}

async fn open_lan_pairing_window(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<LanPairingCode>, HttpError> {
    let target = native_lan_pairing_target(&state, &device_id, query.lease_id.as_deref())?;
    let code = serial_open_lan_pairing_window(&state, &target).await?;
    state.emit(event(
        &device_id,
        "lan",
        "LAN pairing window opened through USB lease",
        json!({ "code": "<redacted>" }),
    ));
    Ok(Json(code))
}

async fn close_lan_pairing_window(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<Value>, HttpError> {
    let target = native_lan_pairing_target(&state, &device_id, query.lease_id.as_deref())?;
    serial_close_lan_pairing_window(&state, &target).await?;
    state.emit(event(
        &device_id,
        "lan",
        "LAN pairing window closed through USB lease",
        json!({ "code": "<redacted>" }),
    ));
    Ok(Json(json!({ "closed": true })))
}

fn native_lan_pairing_target(
    state: &AppState,
    device_id: &str,
    lease_id: Option<&str>,
) -> Result<DeviceRecord, HttpError> {
    let mut state_lock = state.lock()?;
    let target = state_lock
        .devices
        .get(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
        .clone();
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "LAN pairing window is available only through the USB/devd transport.",
        ));
    }
    state_lock.require_lease(device_id, lease_id)?;
    Ok(target)
}

async fn bind_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
    Json(payload): Json<BindRequest>,
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    if let Some(alias) = payload.alias {
        device.display_name = alias;
    }
    Ok(Json(device.clone()))
}

async fn connect_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    device.connection = ConnectionState::Connected;
    Ok(Json(device.clone()))
}

async fn disconnect_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    device.connection = ConnectionState::Disconnected;
    Ok(Json(device.clone()))
}

async fn create_lease(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
) -> Result<Json<WebLease>, HttpError> {
    let lease = {
        let serial_devices = scan_serial_devices(state.config.serial_port.as_deref());
        let mut state_lock = state.lock()?;
        refresh_serial_devices(&mut state_lock, serial_devices);
        state_lock.create_lease(&device_id)?
    };
    state.emit(event(
        &device_id,
        "lease",
        "lease created",
        json!({ "leaseId": lease.lease_id }),
    ));
    Ok(Json(lease))
}

async fn heartbeat_lease(
    State(state): State<AppState>,
    AxumPath(lease_id): AxumPath<String>,
) -> Result<Json<WebLease>, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.cleanup_leases();
    let lease = state_lock
        .leases
        .get_mut(&lease_id)
        .ok_or_else(|| HttpError::forbidden("lease_expired", "The device lease expired."))?;
    lease.expires_at = Instant::now() + Duration::from_millis(DEFAULT_LEASE_TTL_MS);
    lease.ttl_ms = DEFAULT_LEASE_TTL_MS;
    Ok(Json(lease.clone()))
}

async fn delete_lease(
    State(state): State<AppState>,
    AxumPath(lease_id): AxumPath<String>,
) -> Result<Json<Value>, HttpError> {
    let removed = {
        let mut state_lock = state.lock()?;
        state_lock.leases.remove(&lease_id)
    };
    if let Some(lease) = removed.as_ref() {
        state.emit(event(
            &lease.device_id,
            "lease",
            "lease released",
            json!({ "leaseId": lease.lease_id }),
        ));
    }
    Ok(Json(json!({ "released": removed.is_some() })))
}

async fn device_identity(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<Identity>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        device(&state_lock, &device_id)?.clone()
    };
    if target.transport == DeviceTransport::NativeSerial {
        let identity =
            match serial_request_payload::<Identity>(&state, &target, "get_identity", "identity")
                .await
            {
                Ok(identity) => identity,
                Err(error) => {
                    record_serial_bridge_error(&state, &device_id, "identity", &error);
                    return Err(error);
                }
            };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.identity = identity.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(identity));
    }
    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let identity = lan_bridge_read::<Identity>(&configured, "identity").await?;
        validate_lan_bridge_identity(&identity)?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.identity = identity.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(identity));
    }
    Ok(Json(target.identity))
}

async fn device_install_status(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<InstallStatus>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        device(&state_lock, &device_id)?.clone()
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "Install status requires native USB serial transport.",
        ));
    }

    serial_request_payload::<InstallStatus>(&state, &target, "get_install_status", "install_status")
        .await
        .map(Json)
}

async fn device_network(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<NetworkSummary>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        device(&state_lock, &device_id)?.clone()
    };
    if target.transport == DeviceTransport::NativeSerial {
        let network = match serial_request_payload::<NetworkSummary>(
            &state,
            &target,
            "get_network",
            "network",
        )
        .await
        {
            Ok(network) => network,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "network", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.network = network.clone();
            device.status.network = network.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(network));
    }
    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let network = lan_bridge_read::<NetworkSummary>(&configured, "network").await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.network = network.clone();
            device.status.network = network.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(network));
    }
    Ok(Json(target.network))
}

async fn device_status(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        device(&state_lock, &device_id)?.clone()
    };
    if target.transport == DeviceTransport::NativeSerial {
        let status = match serial_request_payload::<ControlPlaneStatus>(
            &state,
            &target,
            "get_status",
            "status",
        )
        .await
        {
            Ok(status) => status,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "status", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.status = status.clone();
            device.network = status.network.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(status));
    }
    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let status = lan_bridge_read::<ControlPlaneStatus>(&configured, "status").await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.network = status.network.clone();
            device.status = status.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(status));
    }
    Ok(Json(target.status))
}

async fn device_calibration(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<CalibrationState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let calibration = match serial_calibration_get(&state, &target).await {
            Ok(calibration) => calibration,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "calibration", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.calibration = calibration.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(calibration));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let calibration = lan_bridge_read::<CalibrationState>(&configured, "calibration").await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.calibration = calibration.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(calibration));
    }

    Ok(Json(target.calibration))
}

async fn configure_calibration(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<CalibrationConfigRequest>,
) -> Result<Json<CalibrationState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let calibration = match serial_calibration_config(&state, &target, &payload).await {
            Ok(calibration) => calibration,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "calibration_config", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.calibration = calibration.clone();
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_calibration_event(&state, &device_id, &payload.op, &calibration);
        return Ok(Json(calibration));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let calibration = lan_bridge_write::<CalibrationState>(
            &configured,
            "calibration",
            Method::PUT,
            Some(lan_bridge_payload(&payload)?),
        )
        .await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.calibration = calibration.clone();
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_calibration_event(&state, &device_id, &payload.op, &calibration);
        return Ok(Json(calibration));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    apply_mock_calibration_config(&mut device.calibration, &payload)?;
    let calibration = device.calibration.clone();
    drop(state_lock);
    emit_calibration_event(&state, &device_id, &payload.op, &calibration);
    Ok(Json(calibration))
}

async fn device_calibration_job(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let job = match serial_calibration_job_get(&state, &target).await {
            Ok(job) => job,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "calibration_job", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.status.calibration.job = job.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(job));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let job = lan_bridge_read::<CalibrationJobState>(&configured, "calibration/job").await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.status.calibration.job = job.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(job));
    }

    Ok(Json(target.status.calibration.job))
}

fn thermal_plant_trace_page(
    snapshot: &ThermalPlantRunSnapshot,
    after_sample: u8,
) -> ThermalPlantRunSnapshot {
    let start = after_sample.min(snapshot.trace_page.total_samples);
    let mut page = snapshot.clone();
    page.trace_page.start_sample = start;
    page.trace_page.points = snapshot
        .trace_page
        .points
        .iter()
        .filter(|point| point.sample_index >= start)
        .take(16)
        .cloned()
        .collect();
    page.trace_page.next_sample = page
        .trace_page
        .points
        .last()
        .map(|point| point.sample_index.saturating_add(1))
        .filter(|next| *next < snapshot.trace_page.total_samples);
    page
}

async fn device_thermal_plant_run(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<ThermalPlantRunQuery>,
) -> Result<Json<ThermalPlantRunSnapshot>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    let after_sample = query.after_sample.unwrap_or(0);
    if target.transport == DeviceTransport::NativeSerial {
        let snapshot = serial_thermal_plant_run_get(&state, &target, after_sample).await?;
        return Ok(Json(snapshot));
    }
    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let path = if after_sample == 0 {
            "calibration/thermal-plant/run".to_string()
        } else {
            format!("calibration/thermal-plant/run?after_sample={after_sample}")
        };
        let snapshot = lan_bridge_read::<ThermalPlantRunSnapshot>(&configured, &path).await?;
        return Ok(Json(snapshot));
    }
    Ok(Json(thermal_plant_trace_page(
        &target.thermal_plant_run,
        after_sample,
    )))
}

async fn configure_calibration_job(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<CalibrationJobRequest>,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    match target.transport {
        DeviceTransport::NativeSerial => {
            configure_native_calibration_job(&state, &device_id, &target, &payload).await
        }
        DeviceTransport::Lan => {
            configure_lan_calibration_job(&state, &device_id, &target, &payload).await
        }
        DeviceTransport::Mock => configure_mock_calibration_job(&state, &device_id, &payload),
    }
}

async fn configure_native_calibration_job(
    state: &AppState,
    device_id: &str,
    target: &DeviceRecord,
    payload: &CalibrationJobRequest,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let job = match serial_calibration_job_config(state, target, payload).await {
        Ok(job) => job,
        Err(error) => {
            record_serial_bridge_error(state, device_id, "calibration_job", &error);
            return Err(error);
        }
    };
    store_calibration_job(state, device_id, job)
}

async fn configure_lan_calibration_job(
    state: &AppState,
    device_id: &str,
    target: &DeviceRecord,
    payload: &CalibrationJobRequest,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let configured = lan_bridge_config(target)?;
    let job = lan_bridge_write::<CalibrationJobState>(
        &configured,
        "calibration/job",
        Method::POST,
        Some(lan_bridge_payload(payload)?),
    )
    .await?;
    store_calibration_job(state, device_id, job)
}

fn store_calibration_job(
    state: &AppState,
    device_id: &str,
    job: CalibrationJobState,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let mut state_lock = state.lock()?;
    if let Some(device) = state_lock.devices.get_mut(device_id) {
        device.status.calibration.job = job.clone();
        device.connection = ConnectionState::Connected;
    }
    Ok(Json(job))
}

fn configure_mock_calibration_job(
    state: &AppState,
    device_id: &str,
    payload: &CalibrationJobRequest,
) -> Result<Json<CalibrationJobState>, HttpError> {
    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    match payload.op {
        CalibrationJobOp::Cancel => cancel_mock_calibration_job(device),
        CalibrationJobOp::Start => start_mock_calibration_job(device, payload.kind)?,
    }
    Ok(Json(device.status.calibration.job.clone()))
}

fn cancel_mock_calibration_job(device: &mut DeviceRecord) {
    if device.status.calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    device.status.calibration.job = CalibrationJobState {
        status: CalibrationJobStatus::Canceled,
        ..CalibrationJobState::default()
    };
    disarm_mock_thermal_plant(&mut device.status);
    device.status.calibration.mode = CalibrationMode::Off;
    if let Some(attempt) = device.thermal_plant_run.attempt.as_mut() {
        attempt.status = CalibrationJobStatus::Canceled;
        attempt.phase = Some(ThermalPlantRunPhase::Cooling);
        attempt.restart_allowed = true;
        attempt.duty_percent = 0;
        attempt.heater_voltage_mv = 0;
        attempt.error = None;
    }
}

fn start_mock_calibration_job(
    device: &mut DeviceRecord,
    kind: Option<CalibrationJobKind>,
) -> Result<(), HttpError> {
    if device.status.calibration.job.status == CalibrationJobStatus::Running {
        return Err(HttpError::bad_request(
            "heater_disarm_pending",
            "The previous heater session is still being physically disarmed.",
        ));
    }
    let kind = kind.ok_or_else(|| {
        HttpError::bad_request(
            "calibration_job_kind_required",
            "Calibration auto job requires a job kind.",
        )
    })?;
    let mut next_request_mv = device.status.calibration.pps_mv;
    let mut thermal_request_mv = device
        .status
        .calibration
        .pps_mv
        .unwrap_or(DEFAULT_PD_REQUEST_MV);
    if kind == CalibrationJobKind::ThermalPlantAuto {
        let (source, request_mv) = thermal_plant_start_request_for_device(device)?;
        thermal_request_mv = request_mv;
        apply_mock_thermal_plant_start(device, source.max_ma, request_mv);
        next_request_mv = Some(request_mv);
    }
    device.status.calibration.job = CalibrationJobState {
        kind: Some(kind),
        status: CalibrationJobStatus::Running,
        progress_percent: 0,
        samples_collected: 0,
        next_request_mv,
        message: None,
    };
    if kind == CalibrationJobKind::ThermalPlantAuto {
        initialize_mock_thermal_plant_run(device, thermal_request_mv);
    }
    Ok(())
}

fn apply_mock_thermal_plant_start(device: &mut DeviceRecord, max_ma: u16, request_mv: u16) {
    device.status.calibration.mode = CalibrationMode::ThermalPlant;
    disarm_mock_thermal_plant(&mut device.status);
    device.status.manual_pps_enabled = true;
    device.status.manual_pps_mv = Some(request_mv);
    device.status.manual_pps_ma = Some(max_ma);
    device.status.pd_request_mv = request_mv;
    device.status.pd_contract_mv = request_mv;
    device.status.voltage_mv = u32::from(request_mv);
    device.status.calibration.pps_enabled = true;
    device.status.calibration.pps_mv = Some(request_mv);
    device.status.calibration.pps_ma = Some(max_ma);
}

fn initialize_mock_thermal_plant_run(device: &mut DeviceRecord, request_mv: u16) {
    let next_run_id = device
        .thermal_plant_run
        .attempt
        .as_ref()
        .map(|attempt| attempt.run_id.saturating_add(1))
        .unwrap_or(1);
    device.thermal_plant_run.attempt = Some(ThermalPlantRunAttempt {
        run_id: next_run_id,
        status: CalibrationJobStatus::Running,
        phase: Some(ThermalPlantRunPhase::Ambient),
        progress_percent: 0,
        elapsed_ms: 0,
        current_temp_centi_c: 2500,
        heater_voltage_mv: request_mv,
        duty_percent: 0,
        sample_count: 0,
        restart_allowed: false,
        error: None,
    });
    device.thermal_plant_run.trace_page = ThermalPlantTracePage::default();
    device.thermal_plant_run.provisional_curve = None;
}

fn disarm_mock_thermal_plant(status: &mut ControlPlaneStatus) {
    status.heater_enabled = false;
    status.heater_output_percent = 0;
    status.heater_physical_output_percent = 0;
    status.manual_pps_enabled = false;
    status.manual_pps_mv = None;
    status.manual_pps_ma = None;
    status.pd_request_mv = DEFAULT_PD_REQUEST_MV;
    status.pd_contract_mv = DEFAULT_PD_REQUEST_MV;
    status.voltage_mv = u32::from(DEFAULT_PD_REQUEST_MV);
    status.manual_pps_error = None;
    status.calibration.heater_enabled = false;
    status.calibration.pps_enabled = false;
    status.calibration.pps_mv = None;
    status.calibration.pps_ma = None;
}

async fn device_heater_curve(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<HeaterCurveState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        if requires_lease(&state_lock, &device_id) {
            state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
        }
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let heater_curve = match serial_heater_curve_get(&state, &target).await {
            Ok(heater_curve) => heater_curve,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "heater_curve", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let heater_curve = lan_bridge_read::<HeaterCurveState>(&configured, "heater-curve").await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    Ok(Json(target.heater_curve))
}

async fn configure_heater_curve(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<HeaterCurveConfigRequest>,
) -> Result<Json<HeaterCurveState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let heater_curve = match serial_heater_curve_config(&state, &target, &payload).await {
            Ok(heater_curve) => heater_curve,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "heater_curve_config", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let heater_curve = lan_bridge_write::<HeaterCurveState>(
            &configured,
            "heater-curve",
            Method::PUT,
            Some(lan_bridge_payload(&payload)?),
        )
        .await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    match payload.op {
        HeaterCurveConfigOp::Preview => {
            let package = payload.package.clone().ok_or_else(|| {
                HttpError::bad_request(
                    "heater_curve_package_required",
                    "Heater curve preview requires a package.",
                )
            })?;
            validate_heater_curve_package(&package)?;
            device.heater_curve.preview = Some(normalize_heater_curve_package(package));
        }
        HeaterCurveConfigOp::ClearPreview => {
            device.heater_curve.preview = None;
        }
    }
    Ok(Json(device.heater_curve.clone()))
}

async fn save_heater_curve(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<HeaterCurveSaveRequest>,
) -> Result<Json<HeaterCurveState>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };

    if target.transport == DeviceTransport::NativeSerial {
        let heater_curve = match serial_heater_curve_save(&state, &target).await {
            Ok(heater_curve) => heater_curve,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "heater_curve_save", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let heater_curve = lan_bridge_write::<HeaterCurveState>(
            &configured,
            "heater-curve/save",
            Method::POST,
            None,
        )
        .await?;
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.heater_curve = heater_curve.clone();
            device.connection = ConnectionState::Connected;
        }
        return Ok(Json(heater_curve));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    let preview = device.heater_curve.preview.clone().ok_or_else(|| {
        HttpError::bad_request(
            "heater_curve_preview_required",
            "Heater curve save requires an active preview package.",
        )
    })?;
    device.heater_curve.active = preview;
    Ok(Json(device.heater_curve.clone()))
}

async fn device_events(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, axum::Error>>>, HttpError> {
    let backlog = device_event_backlog(&state, &device_id)?;
    let replay = tokio_stream::iter(
        backlog
            .into_iter()
            .map(|event| Ok(devd_event_to_sse(event))),
    );
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |event| {
        let device_id = device_id.clone();
        match event {
            Ok(event) if event.device_id.as_deref() == Some(&device_id) => {
                Some(Ok(devd_event_to_sse(event)))
            }
            _ => None,
        }
    });
    Ok(Sse::new(replay.chain(stream)))
}

fn device_event_backlog(state: &AppState, device_id: &str) -> Result<Vec<DevdEvent>, HttpError> {
    let state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;

    Ok(device
        .events
        .iter()
        .rev()
        .take(DEVICE_EVENT_REPLAY_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

fn trim_device_record_for_list(mut device: DeviceRecord) -> DeviceRecord {
    device.events = device
        .events
        .iter()
        .rev()
        .take(DEVICE_LIST_EVENT_LIMIT)
        .cloned()
        .map(summarize_device_list_event)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    device
}

fn summarize_device_list_event(mut event: DevdEvent) -> DevdEvent {
    if event.kind == "transport"
        && let Some(payload) = event.payload.as_object_mut()
    {
        payload.remove("frame");
    }
    event
}

fn device_list_payload(device: DeviceRecord) -> Value {
    json!({
        "id": device.id,
        "displayName": device.display_name,
        "portPath": device.port_path,
        "transport": device.transport,
        "connection": device.connection,
        "identity": device.identity,
        "network": device.network,
        "status": device.status,
        "events": device.events,
    })
}
