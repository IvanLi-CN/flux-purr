pub(crate) use super::*;

#[derive(Debug, Deserialize)]
pub struct BindRequest {
    pub alias: Option<String>,
}

/// Versioned local-only control protocol used by the CLI and the native daemon.
/// The wire format is a four-byte big-endian length followed by a CBOR frame.
pub const LOCAL_CONTROL_PROTOCOL_VERSION: u16 = 1;
pub(crate) const LOCAL_CONTROL_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalControlRequest {
    pub version: u16,
    pub request_id: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub body_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalControlResponse {
    pub version: u16,
    pub request_id: String,
    pub status: u16,
    pub body: Value,
}

pub fn validate_local_control_endpoint(endpoint: &str) -> io::Result<PathBuf> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty()
        || endpoint.contains("://")
        || endpoint.starts_with("tcp:")
        || endpoint.parse::<SocketAddr>().is_ok()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "devd endpoint must be a local Unix socket or named pipe, not a URL or TCP address",
        ));
    }
    let path = PathBuf::from(endpoint);
    #[cfg(unix)]
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "devd endpoint must be an absolute Unix socket path",
        ));
    }
    Ok(path)
}

pub(crate) fn local_control_request_id() -> String {
    let sequence = LOCAL_CONTROL_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("cli-{}-{sequence}", now_millis())
}

#[cfg(unix)]
pub async fn local_control_request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<LocalControlResponse, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::UnixStream;

    let endpoint = validate_local_control_endpoint(endpoint)?;
    let mut stream = UnixStream::connect(endpoint).await?;
    let request = LocalControlRequest {
        version: LOCAL_CONTROL_PROTOCOL_VERSION,
        request_id: local_control_request_id(),
        method: method.to_string(),
        path: path.to_string(),
        body,
        body_bytes: None,
    };
    let request_id = request.request_id.clone();
    write_local_control_frame(&mut stream, &request).await?;
    let response: LocalControlResponse = read_local_control_frame(&mut stream).await?;
    if response.version != LOCAL_CONTROL_PROTOCOL_VERSION {
        return Err("devd returned an unsupported local-control protocol version".into());
    }
    if response.request_id != request_id {
        return Err("devd returned a response for a different local-control request".into());
    }
    Ok(response)
}

#[cfg(unix)]
pub async fn local_control_request_bytes(
    endpoint: &str,
    method: &str,
    path: &str,
    body_bytes: Vec<u8>,
) -> Result<LocalControlResponse, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::UnixStream;

    let endpoint = validate_local_control_endpoint(endpoint)?;
    let mut stream = UnixStream::connect(endpoint).await?;
    let request = LocalControlRequest {
        version: LOCAL_CONTROL_PROTOCOL_VERSION,
        request_id: local_control_request_id(),
        method: method.to_string(),
        path: path.to_string(),
        body: None,
        body_bytes: Some(body_bytes),
    };
    let request_id = request.request_id.clone();
    write_local_control_frame(&mut stream, &request).await?;
    let response: LocalControlResponse = read_local_control_frame(&mut stream).await?;
    if response.version != LOCAL_CONTROL_PROTOCOL_VERSION {
        return Err("devd returned an unsupported local-control protocol version".into());
    }
    if response.request_id != request_id {
        return Err("devd returned a response for a different local-control request".into());
    }
    Ok(response)
}

#[cfg(not(unix))]
pub async fn local_control_request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<LocalControlResponse, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let endpoint = validate_local_control_endpoint(endpoint)?;
    let mut stream = ClientOptions::new().open(endpoint)?;
    let request = LocalControlRequest {
        version: LOCAL_CONTROL_PROTOCOL_VERSION,
        request_id: local_control_request_id(),
        method: method.to_string(),
        path: path.to_string(),
        body,
        body_bytes: None,
    };
    let request_id = request.request_id.clone();
    write_local_control_frame(&mut stream, &request).await?;
    let response: LocalControlResponse = read_local_control_frame(&mut stream).await?;
    if response.version != LOCAL_CONTROL_PROTOCOL_VERSION {
        return Err("devd returned an unsupported local-control protocol version".into());
    }
    if response.request_id != request_id {
        return Err("devd returned a response for a different local-control request".into());
    }
    Ok(response)
}

#[cfg(not(unix))]
pub async fn local_control_request_bytes(
    endpoint: &str,
    method: &str,
    path: &str,
    body_bytes: Vec<u8>,
) -> Result<LocalControlResponse, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let endpoint = validate_local_control_endpoint(endpoint)?;
    let mut stream = ClientOptions::new().open(endpoint)?;
    let request = LocalControlRequest {
        version: LOCAL_CONTROL_PROTOCOL_VERSION,
        request_id: local_control_request_id(),
        method: method.to_string(),
        path: path.to_string(),
        body: None,
        body_bytes: Some(body_bytes),
    };
    let request_id = request.request_id.clone();
    write_local_control_frame(&mut stream, &request).await?;
    let response: LocalControlResponse = read_local_control_frame(&mut stream).await?;
    if response.version != LOCAL_CONTROL_PROTOCOL_VERSION {
        return Err("devd returned an unsupported local-control protocol version".into());
    }
    if response.request_id != request_id {
        return Err("devd returned a response for a different local-control request".into());
    }
    Ok(response)
}

#[cfg(unix)]
pub async fn serve_local_control(
    listener: tokio::net::UnixListener,
    state: AppState,
) -> io::Result<()> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let result = handle_local_control_connection(&mut stream, state).await;
            if let Err(error) = result {
                let _ = write_local_control_frame(
                    &mut stream,
                    &LocalControlResponse {
                        version: LOCAL_CONTROL_PROTOCOL_VERSION,
                        request_id: "unknown".into(),
                        status: 500,
                        body: json!({"error": error.to_string()}),
                    },
                )
                .await;
            }
        });
    }
}

#[cfg(windows)]
pub async fn serve_local_control(endpoint: String, state: AppState) -> io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    loop {
        let mut server = ServerOptions::new().create(&endpoint)?;
        server.connect().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let _ = handle_local_control_connection(&mut server, state).await;
        });
    }
}

pub(crate) async fn handle_local_control_connection<T>(
    stream: &mut T,
    state: AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
{
    let request: LocalControlRequest = read_local_control_frame(stream).await?;
    if request.version != LOCAL_CONTROL_PROTOCOL_VERSION {
        return Err("unsupported local-control protocol version".into());
    }
    let method = request.method.parse::<Method>()?;
    let builder = Request::builder().method(method).uri(&request.path).header(
        "content-type",
        if request.body_bytes.is_some() {
            "application/octet-stream"
        } else {
            "application/json"
        },
    );
    let body = match (request.body, request.body_bytes) {
        (Some(body), None) => Body::from(serde_json::to_vec(&body)?),
        (None, Some(body)) => Body::from(body),
        (None, None) => Body::empty(),
        (Some(_), Some(_)) => {
            return Err("local control request cannot contain JSON and bytes".into());
        }
    };
    let response = app(state)
        .oneshot(builder.body(body)?)
        .await
        .map_err(|error| io::Error::other(error.to_string()))?;
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), LOCAL_CONTROL_MAX_FRAME_BYTES).await?;
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes).to_string()}));
    write_local_control_frame(
        stream,
        &LocalControlResponse {
            version: LOCAL_CONTROL_PROTOCOL_VERSION,
            request_id: request.request_id,
            status,
            body,
        },
    )
    .await?;
    Ok(())
}

pub(crate) async fn write_local_control_frame<T, V>(stream: &mut T, value: &V) -> io::Result<()>
where
    T: AsyncWrite + Unpin,
    V: Serialize,
{
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if bytes.len() > LOCAL_CONTROL_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local-control frame too large",
        ));
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await
}

pub(crate) async fn read_local_control_frame<T, V>(stream: &mut T) -> io::Result<V>
where
    T: AsyncRead + Unpin,
    V: DeserializeOwned,
{
    let length = stream.read_u32().await? as usize;
    if length == 0 || length > LOCAL_CONTROL_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid local-control frame length",
        ));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    ciborium::de::from_reader(bytes.as_slice())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

pub fn app(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/api/v1/lan/devices", get(list_lan_devices))
        .route("/api/v1/lan/discovery/mdns", post(refresh_lan_mdns))
        .route("/api/v1/lan/discovery/scan", post(scan_lan_cidr))
        .route("/api/v1/lan/pair", post(pair_lan_device))
        .route(
            "/api/v1/lan/devices/{lan_device_id}/connect",
            post(connect_lan_device),
        )
        .route(
            "/api/v1/devices/{device_id}/lan-pairing/reset",
            post(reset_lan_pairing),
        )
        .route(
            "/api/v1/devices/{device_id}/lan-pairing/code",
            get(get_lan_pairing_code),
        )
        .route(
            "/api/v1/devices/{device_id}/lan-pairing/window",
            post(open_lan_pairing_window).delete(close_lan_pairing_window),
        )
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/devices/{device_id}/bind", post(bind_device))
        .route("/api/v1/devices/{device_id}/connect", post(connect_device))
        .route(
            "/api/v1/devices/{device_id}/disconnect",
            post(disconnect_device),
        )
        .route("/api/v1/devices/{device_id}/leases", post(create_lease))
        .route("/api/v1/leases/{lease_id}/heartbeat", post(heartbeat_lease))
        .route("/api/v1/leases/{lease_id}", delete(delete_lease))
        .route("/api/v1/devices/{device_id}/identity", get(device_identity))
        .route(
            "/api/v1/devices/{device_id}/install-status",
            get(device_install_status),
        )
        .route("/api/v1/devices/{device_id}/network", get(device_network))
        .route("/api/v1/devices/{device_id}/status", get(device_status))
        .route("/api/v1/devices/{device_id}/events", get(device_events))
        .route("/api/v1/devices/{device_id}/wifi", put(configure_wifi))
        .route(
            "/api/v1/devices/{device_id}/runtime",
            put(configure_runtime),
        )
        .route(
            "/api/v1/devices/{device_id}/buzzer-test",
            post(configure_buzzer_test),
        )
        .route(
            "/api/v1/devices/{device_id}/calibration",
            get(device_calibration).put(configure_calibration),
        )
        .route(
            "/api/v1/devices/{device_id}/calibration/job",
            get(device_calibration_job).post(configure_calibration_job),
        )
        .route(
            "/api/v1/devices/{device_id}/calibration/thermal-plant/run",
            get(device_thermal_plant_run),
        )
        .route(
            "/api/v1/devices/{device_id}/eeprom",
            post(configure_eeprom_maintenance),
        )
        .route(
            "/api/v1/devices/{device_id}/heater-curve",
            get(device_heater_curve).put(configure_heater_curve),
        )
        .route(
            "/api/v1/devices/{device_id}/heater-curve/save",
            post(save_heater_curve),
        )
        .route("/api/v1/artifacts", get(list_artifacts_route))
        .route("/api/v1/artifacts/verify", post(verify_artifact_route))
        .route("/api/v1/devices/{device_id}/flash", post(flash_device))
        .route(
            "/api/v1/firmware-bundles",
            get(list_firmware_bundles).post(import_firmware_bundle),
        )
        .route("/api/v1/firmware-update", post(local_firmware_update))
        .route(
            "/api/v1/devices/{device_id}/firmware",
            post(firmware_operation),
        )
        .with_state(state.clone());

    if state.config.allow_dev_cors {
        router = router.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _| {
                    is_allowed_dev_origin(origin)
                }))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers(Any),
        );
    }

    router
}
