use std::{
    collections::{HashMap, VecDeque},
    fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{process::Command, sync::broadcast};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::cors::CorsLayer;

pub const DEFAULT_EVENT_LIMIT: usize = 1_000;
pub const DEFAULT_LOG_LIMIT: usize = 2_000;
pub const DEFAULT_TRACE_LIMIT: usize = 2_000;
pub const DEFAULT_LEASE_TTL_MS: u64 = 8_000;
pub const DEFAULT_BAUD_RATE: u32 = 115_200;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub artifact_root: Option<PathBuf>,
    pub allow_dev_cors: bool,
    pub allow_real_flash: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:30080".parse().unwrap(),
            artifact_root: None,
            allow_dev_cors: false,
            allow_real_flash: false,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: AppConfig,
    inner: Arc<Mutex<DevdState>>,
    events: broadcast::Sender<DevdEvent>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let (events, _) = broadcast::channel(DEFAULT_EVENT_LIMIT);
        let mut state = DevdState::default();
        state.seed_mock_device();

        Self {
            config,
            inner: Arc::new(Mutex::new(state)),
            events,
        }
    }

    pub fn test() -> Self {
        Self::new(AppConfig::default())
    }

    pub fn lease_device(&self, device_id: &str) -> Result<WebLease, HttpError> {
        let mut state = self.lock()?;
        state.create_lease(device_id)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, DevdState>, HttpError> {
        self.inner
            .lock()
            .map_err(|_| HttpError::internal("state lock poisoned"))
    }

    fn emit(&self, event: DevdEvent) {
        if let Ok(mut state) = self.inner.lock() {
            state.push_event(event.clone());
        }
        let _ = self.events.send(event);
    }
}

#[derive(Debug, Default)]
struct DevdState {
    devices: HashMap<String, DeviceRecord>,
    leases: HashMap<String, WebLease>,
    dry_run_passes: HashMap<String, String>,
    sequence: u64,
}

impl DevdState {
    fn seed_mock_device(&mut self) {
        let device = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
        self.devices.insert(device.id.clone(), device);
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.sequence = self.sequence.saturating_add(1);
        format!("{prefix}-{}-{}", now_millis(), self.sequence)
    }

    fn push_event(&mut self, event: DevdEvent) {
        for device in self.devices.values_mut() {
            if event.device_id.as_deref() == Some(&device.id) {
                push_bounded(&mut device.events, event.clone(), DEFAULT_EVENT_LIMIT);
            }
        }
    }

    fn cleanup_leases(&mut self) {
        let now = Instant::now();
        self.leases.retain(|_, lease| lease.expires_at > now);
    }

    fn create_lease(&mut self, device_id: &str) -> Result<WebLease, HttpError> {
        self.cleanup_leases();
        if !self.devices.contains_key(device_id) {
            return Err(HttpError::not_found(
                "device_not_found",
                "Device not found.",
            ));
        }
        if let Some(existing) = self
            .leases
            .values()
            .find(|lease| lease.device_id == device_id && lease.expires_at > Instant::now())
        {
            return Err(HttpError::conflict(
                "lease_conflict",
                "Another client owns the active USB lease.",
                json!({ "leaseId": existing.lease_id }),
            ));
        }

        let lease = WebLease {
            lease_id: self.next_id("lease"),
            device_id: device_id.to_string(),
            expires_at: Instant::now() + Duration::from_millis(DEFAULT_LEASE_TTL_MS),
            ttl_ms: DEFAULT_LEASE_TTL_MS,
        };
        self.leases.insert(lease.lease_id.clone(), lease.clone());
        Ok(lease)
    }

    fn require_lease(&mut self, device_id: &str, lease_id: Option<&str>) -> Result<(), HttpError> {
        self.cleanup_leases();
        let Some(lease_id) = lease_id else {
            return Err(HttpError::forbidden(
                "lease_required",
                "A valid device lease is required.",
            ));
        };
        let Some(lease) = self.leases.get(lease_id) else {
            return Err(HttpError::forbidden(
                "lease_expired",
                "The device lease expired.",
            ));
        };
        if lease.device_id != device_id {
            return Err(HttpError::forbidden(
                "lease_device_mismatch",
                "The lease belongs to another device.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub id: String,
    pub display_name: String,
    pub port_path: Option<String>,
    pub transport: DeviceTransport,
    pub connection: ConnectionState,
    pub identity: Identity,
    pub network: NetworkSummary,
    pub status: ControlPlaneStatus,
    pub selected_artifact_id: Option<String>,
    pub logs: VecDeque<LogEntry>,
    pub trace: VecDeque<TraceEntry>,
    pub events: VecDeque<DevdEvent>,
}

impl DeviceRecord {
    fn mock(id: &str, transport: DeviceTransport) -> Self {
        let identity = Identity {
            device_id: id.to_string(),
            firmware_version: "fw/v0.4.0-dev".to_string(),
            build_id: "devd-mock".to_string(),
            git_sha: "unknown".to_string(),
            board: "esp32-s3".to_string(),
            api_version: "2026-05-23".to_string(),
            protocol_version: "flux-purr.usb.v1".to_string(),
            hostname: id.to_string(),
            capabilities: vec![
                "identity".to_string(),
                "status".to_string(),
                "network".to_string(),
                "wifi_config".to_string(),
                "monitor".to_string(),
                "firmware_check".to_string(),
                "flash".to_string(),
            ],
        };
        let network = NetworkSummary {
            state: NetworkState::Connected,
            ssid: Some("FluxPurr-Lab".to_string()),
            ip: Some("192.168.31.42".to_string()),
            gateway: Some("192.168.31.1".to_string()),
            dns: vec!["192.168.31.1".to_string()],
            wifi_rssi: Some(-54),
            last_error: None,
        };
        let status = ControlPlaneStatus {
            mode: "sampling".to_string(),
            uptime_seconds: 123,
            current_temp_c: 183.6,
            target_temp_c: 220,
            heater_enabled: true,
            heater_output_percent: 22,
            active_cooling_enabled: true,
            fan_display_state: "AUTO".to_string(),
            fan_enabled: true,
            fan_pwm_permille: 500,
            voltage_mv: 20_010,
            current_ma: 840,
            board_temp_centi: 3_840,
            pd_request_mv: 20_000,
            pd_contract_mv: 20_000,
            pd_state: "ready".to_string(),
            frontpanel_key: None,
            network: network.clone(),
        };

        Self {
            id: id.to_string(),
            display_name: "Flux Purr mock target".to_string(),
            port_path: None,
            transport,
            connection: ConnectionState::Connected,
            identity,
            network,
            status,
            selected_artifact_id: None,
            logs: VecDeque::new(),
            trace: VecDeque::new(),
            events: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTransport {
    Mock,
    NativeSerial,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Busy,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String,
    pub firmware_version: String,
    pub build_id: String,
    pub git_sha: String,
    pub board: String,
    pub api_version: String,
    pub protocol_version: String,
    pub hostname: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    Disabled,
    Idle,
    Saving,
    Connecting,
    Connected,
    Error,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSummary {
    pub state: NetworkState,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub wifi_rssi: Option<i16>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPlaneStatus {
    pub mode: String,
    pub uptime_seconds: u32,
    pub current_temp_c: f32,
    pub target_temp_c: i16,
    pub heater_enabled: bool,
    pub heater_output_percent: u8,
    pub active_cooling_enabled: bool,
    pub fan_display_state: String,
    pub fan_enabled: bool,
    pub fan_pwm_permille: u16,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: String,
    pub frontpanel_key: Option<String>,
    pub network: NetworkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebLease {
    pub lease_id: String,
    pub device_id: String,
    #[serde(skip, default = "expired_instant")]
    pub expires_at: Instant,
    pub ttl_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevdEvent {
    pub id: String,
    pub timestamp: String,
    pub device_id: Option<String>,
    pub kind: String,
    pub message: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub id: String,
    pub timestamp: String,
    pub direction: String,
    pub frame_type: String,
    pub request_id: Option<String>,
    pub summary: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiConfigRequest {
    pub lease_id: String,
    pub op: WifiConfigOp,
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub auto_reconnect: Option<bool>,
    pub telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WifiConfigOp {
    Set,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareArtifact {
    pub artifact_id: String,
    pub name: String,
    pub version: String,
    pub git_sha: String,
    pub build_id: String,
    pub target_chip: String,
    pub profile: String,
    pub features: Vec<String>,
    pub protocol: String,
    pub files: Vec<ArtifactFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactFile {
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub flash_address: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVerifyRequest {
    pub artifact: FirmwareArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVerifyResult {
    pub artifact_id: String,
    pub verified: bool,
    pub files: Vec<ArtifactFileResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactFileResult {
    pub kind: String,
    pub sha256: String,
    pub size: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashRequest {
    pub lease_id: String,
    pub artifact: FirmwareArtifact,
    pub dry_run: bool,
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashResult {
    pub artifact_id: String,
    pub dry_run: bool,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct HttpError {
    status: StatusCode,
    error: ApiError,
}

impl HttpError {
    fn internal(message: &str) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            message,
            true,
        )
    }

    fn not_found(code: &str, message: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message, false)
    }

    fn bad_request(code: &str, message: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message, false)
    }

    fn forbidden(code: &str, message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message, true)
    }

    fn conflict(code: &str, message: &str, details: Value) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            error: ApiError {
                code: code.to_string(),
                message: message.to_string(),
                retryable: true,
                details: Some(details),
            },
        }
    }

    fn new(status: StatusCode, code: &str, message: &str, retryable: bool) -> Self {
        Self {
            status,
            error: ApiError {
                code: code.to_string(),
                message: message.to_string(),
                retryable,
                details: None,
            },
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.error }))).into_response()
    }
}

#[derive(Debug, Deserialize)]
pub struct LeaseQuery {
    pub lease_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BindRequest {
    pub alias: Option<String>,
}

pub fn app(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/health", get(health))
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
        .route("/api/v1/devices/{device_id}/network", get(device_network))
        .route("/api/v1/devices/{device_id}/status", get(device_status))
        .route("/api/v1/devices/{device_id}/events", get(device_events))
        .route("/api/v1/devices/{device_id}/wifi", put(configure_wifi))
        .route("/api/v1/artifacts/verify", post(verify_artifact_route))
        .route("/api/v1/devices/{device_id}/flash", post(flash_device))
        .with_state(state.clone());

    if state.config.allow_dev_cors {
        router = router.layer(
            CorsLayer::new()
                .allow_origin(HeaderValue::from_static("http://localhost:5173"))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE]),
        );
    }

    router
}

async fn health(State(state): State<AppState>) -> Result<Json<Value>, HttpError> {
    let state_lock = state.lock()?;
    Ok(Json(json!({
        "name": "flux-purr-devd",
        "version": env!("CARGO_PKG_VERSION"),
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
    let serial_devices = scan_serial_devices();
    let mut state_lock = state.lock()?;
    for device in serial_devices {
        state_lock
            .devices
            .entry(device.id.clone())
            .or_insert(device);
    }
    let devices = state_lock.devices.values().cloned().collect::<Vec<_>>();
    Ok(Json(json!({ "devices": devices })))
}

async fn bind_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<BindRequest>,
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
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
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
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
) -> Result<Json<DeviceRecord>, HttpError> {
    let mut state_lock = state.lock()?;
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
        let mut state_lock = state.lock()?;
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
    let mut state_lock = state.lock()?;
    let removed = state_lock.leases.remove(&lease_id);
    Ok(Json(json!({ "released": removed.is_some() })))
}

async fn device_identity(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<Identity>, HttpError> {
    let mut state_lock = state.lock()?;
    if requires_lease(&state_lock, &device_id) {
        state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    }
    Ok(Json(device(&state_lock, &device_id)?.identity.clone()))
}

async fn device_network(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<NetworkSummary>, HttpError> {
    let mut state_lock = state.lock()?;
    if requires_lease(&state_lock, &device_id) {
        state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    }
    Ok(Json(device(&state_lock, &device_id)?.network.clone()))
}

async fn device_status(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Query(query): Query<LeaseQuery>,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    let mut state_lock = state.lock()?;
    if requires_lease(&state_lock, &device_id) {
        state_lock.require_lease(&device_id, query.lease_id.as_deref())?;
    }
    Ok(Json(device(&state_lock, &device_id)?.status.clone()))
}

async fn device_events(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, axum::Error>>>, HttpError> {
    if !state.lock()?.devices.contains_key(&device_id) {
        return Err(HttpError::not_found(
            "device_not_found",
            "Device not found.",
        ));
    }
    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(move |event| {
        let device_id = device_id.clone();
        match event {
            Ok(event) if event.device_id.as_deref() == Some(&device_id) => {
                let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
                Some(Ok(Event::default().event(event.kind).data(data)))
            }
            _ => None,
        }
    });
    Ok(Sse::new(stream))
}

async fn configure_wifi(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<WifiConfigRequest>,
) -> Result<Json<Value>, HttpError> {
    let mut state_lock = state.lock()?;
    state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    match payload.op {
        WifiConfigOp::Clear => {
            device.network = NetworkSummary {
                state: NetworkState::Disabled,
                ssid: None,
                ip: None,
                gateway: None,
                dns: Vec::new(),
                wifi_rssi: None,
                last_error: None,
            };
        }
        WifiConfigOp::Set => {
            device.network.state = NetworkState::Saving;
            device.network.ssid = payload.ssid.clone();
            device.network.last_error = None;
        }
    }
    device.status.network = device.network.clone();
    let redacted = json!({
        "accepted": true,
        "network": device.network,
        "wifi": {
            "op": payload.op,
            "ssid": payload.ssid,
            "password": payload.password.as_ref().map(|_| "<redacted>"),
            "autoReconnect": payload.auto_reconnect,
            "telemetryIntervalMs": payload.telemetry_interval_ms
        }
    });
    Ok(Json(redacted))
}

async fn verify_artifact_route(
    State(state): State<AppState>,
    Json(payload): Json<ArtifactVerifyRequest>,
) -> Result<Json<ArtifactVerifyResult>, HttpError> {
    verify_artifact(&payload.artifact, state.config.artifact_root.as_deref())
        .map(Json)
        .map_err(sanitize_io_error)
}

async fn flash_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FlashRequest>,
) -> Result<Json<FlashResult>, HttpError> {
    let port_path = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        let device = state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
        match device.transport {
            DeviceTransport::NativeSerial => device.port_path.clone().ok_or_else(|| {
                HttpError::bad_request("missing_port", "Native serial device has no port path.")
            })?,
            DeviceTransport::Mock if payload.dry_run => String::new(),
            DeviceTransport::Mock => {
                return Err(HttpError::bad_request(
                    "real_flash_requires_native_serial",
                    "Real flash requires a native serial target.",
                ));
            }
        }
    };

    let verification = verify_artifact(&payload.artifact, state.config.artifact_root.as_deref())
        .map_err(sanitize_io_error)?;
    if !verification.verified {
        return Err(HttpError::bad_request(
            "artifact_verify_failed",
            "Firmware artifact verification failed.",
        ));
    }

    if payload.dry_run {
        let mut state_lock = state.lock()?;
        state_lock
            .dry_run_passes
            .insert(device_id.clone(), payload.artifact.artifact_id.clone());
        return Ok(Json(FlashResult {
            artifact_id: payload.artifact.artifact_id,
            dry_run: true,
            status: "passed".to_string(),
            message: "Artifact verified; no flash write performed.".to_string(),
        }));
    }

    {
        let state_lock = state.lock()?;
        let prior = state_lock.dry_run_passes.get(&device_id);
        if prior != Some(&payload.artifact.artifact_id) {
            return Err(HttpError::forbidden(
                "dry_run_required",
                "Real flash requires a successful dry-run for the same artifact.",
            ));
        }
    }

    if payload.confirm.as_deref() != Some("FLASH") {
        return Err(HttpError::forbidden(
            "confirmation_required",
            "Real flash requires confirm=FLASH.",
        ));
    }

    if !state.config.allow_real_flash {
        return Err(HttpError::forbidden(
            "real_flash_disabled",
            "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
        ));
    }

    run_espflash(
        &payload.artifact,
        state.config.artifact_root.as_deref(),
        &port_path,
    )
    .await?;
    Ok(Json(FlashResult {
        artifact_id: payload.artifact.artifact_id,
        dry_run: false,
        status: "completed".to_string(),
        message: "espflash command completed.".to_string(),
    }))
}

pub fn scan_serial_devices() -> Vec<DeviceRecord> {
    let mut devices = Vec::new();
    let Ok(ports) = serialport::available_ports() else {
        return devices;
    };

    for port in ports {
        let (id, display_name) = match &port.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                let serial = info
                    .serial_number
                    .clone()
                    .unwrap_or_else(|| port.port_name.replace('/', "_"));
                (
                    format!("serial-{:04x}-{:04x}-{serial}", info.vid, info.pid),
                    info.product
                        .clone()
                        .unwrap_or_else(|| "USB serial device".to_string()),
                )
            }
            _ => (
                format!("serial-{}", port.port_name.replace('/', "_")),
                "Serial device".to_string(),
            ),
        };
        let mut device = DeviceRecord::mock(&id, DeviceTransport::NativeSerial);
        device.display_name = display_name;
        device.port_path = Some(port.port_name);
        device.connection = ConnectionState::Disconnected;
        device.identity.capabilities = vec![
            "identity".to_string(),
            "status".to_string(),
            "network".to_string(),
            "wifi_config".to_string(),
            "monitor".to_string(),
        ];
        devices.push(device);
    }

    devices
}

pub fn verify_artifact(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
) -> io::Result<ArtifactVerifyResult> {
    let mut files = Vec::new();
    for file in &artifact.files {
        let path = resolve_artifact_path(root, &file.path);
        let bytes = fs::read(&path)?;
        let size = bytes.len() as u64;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        let ok = size == file.size && digest == file.sha256;
        files.push(ArtifactFileResult {
            kind: file.kind.clone(),
            sha256: digest,
            size,
            ok,
        });
    }
    Ok(ArtifactVerifyResult {
        artifact_id: artifact.artifact_id.clone(),
        verified: !files.is_empty() && files.iter().all(|file| file.ok),
        files,
    })
}

fn resolve_artifact_path(root: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(root) = root {
        root.join(path)
    } else {
        path
    }
}

async fn run_espflash(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<(), HttpError> {
    let args = build_espflash_args(artifact, root, port_path)?;
    let status = Command::new("espflash")
        .args(args)
        .status()
        .await
        .map_err(|_| HttpError::internal("Failed to start espflash."))?;

    if status.success() {
        Ok(())
    } else {
        Err(HttpError::internal("espflash returned a non-zero status."))
    }
}

fn build_espflash_args(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    let Some(app_image) = artifact.files.iter().find(|file| file.kind == "app") else {
        return Err(HttpError::bad_request(
            "missing_app_image",
            "Artifact does not contain an app image.",
        ));
    };
    let flash_address = app_image
        .flash_address
        .ok_or_else(|| HttpError::bad_request("missing_flash_address", "Missing flash address."))?;
    let path = resolve_artifact_path(root, &app_image.path);
    Ok(vec![
        "write-bin".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--non-interactive".to_string(),
        flash_address.to_string(),
        path.to_string_lossy().into_owned(),
    ])
}

fn requires_lease(state: &DevdState, device_id: &str) -> bool {
    state
        .devices
        .get(device_id)
        .map(|device| device.transport == DeviceTransport::NativeSerial)
        .unwrap_or(true)
}

fn device<'a>(state: &'a DevdState, device_id: &str) -> Result<&'a DeviceRecord, HttpError> {
    state
        .devices
        .get(device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))
}

fn push_bounded<T>(values: &mut VecDeque<T>, value: T, limit: usize) {
    if values.len() >= limit {
        values.pop_front();
    }
    values.push_back(value);
}

fn event(device_id: &str, kind: &str, message: &str, payload: Value) -> DevdEvent {
    DevdEvent {
        id: format!("event-{}", now_millis()),
        timestamp: timestamp(),
        device_id: Some(device_id.to_string()),
        kind: kind.to_string(),
        message: message.to_string(),
        payload,
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn timestamp() -> String {
    now_millis().to_string()
}

fn expired_instant() -> Instant {
    Instant::now()
}

fn sanitize_io_error(error: io::Error) -> HttpError {
    HttpError::bad_request(
        "artifact_io_error",
        &format!("Artifact file error: {}", error.kind()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lease_conflict_and_expiry_are_enforced() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let conflict = state.lease_device("mock-fp-lab-01").unwrap_err();
        assert_eq!(conflict.status, StatusCode::CONFLICT);

        {
            let mut inner = state.lock().unwrap();
            inner.leases.get_mut(&lease.lease_id).unwrap().expires_at =
                Instant::now() - Duration::from_millis(1);
            inner.cleanup_leases();
        }

        assert!(state.lease_device("mock-fp-lab-01").is_ok());
    }

    #[test]
    fn bounded_queue_rotates_oldest_entries() {
        let mut values = VecDeque::new();
        push_bounded(&mut values, 1, 2);
        push_bounded(&mut values, 2, 2);
        push_bounded(&mut values, 3, 2);
        assert_eq!(values.into_iter().collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn artifact_verify_checks_hash_and_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("firmware.bin");
        fs::write(&file_path, b"flux-purr").unwrap();
        let digest = format!("sha256:{:x}", Sha256::digest(b"flux-purr"));
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "debug".to_string(),
            features: vec!["web_serial".to_string()],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: "app".to_string(),
                path: "firmware.bin".to_string(),
                sha256: digest.clone(),
                size: 9,
                flash_address: Some(0x10000),
            }],
        };

        let result = verify_artifact(&artifact, Some(dir.path())).unwrap();
        assert!(result.verified);
        assert_eq!(result.files[0].sha256, digest);
    }

    #[test]
    fn artifact_verify_reports_hash_mismatch() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("firmware.bin");
        fs::write(&file_path, b"flux-purr").unwrap();
        let artifact = FirmwareArtifact {
            artifact_id: "bad-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "debug".to_string(),
            features: vec!["web_serial".to_string()],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: "app".to_string(),
                path: "firmware.bin".to_string(),
                sha256: "sha256:bad".to_string(),
                size: 9,
                flash_address: Some(0x10000),
            }],
        };

        let result = verify_artifact(&artifact, Some(dir.path())).unwrap();
        assert!(!result.verified);
        assert!(!result.files[0].ok);
    }

    #[test]
    fn artifact_verify_rejects_empty_file_list() {
        let artifact = FirmwareArtifact {
            artifact_id: "empty-artifact".to_string(),
            name: "Empty".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "debug".to_string(),
            features: Vec::new(),
            protocol: "flux-purr.usb.v1".to_string(),
            files: Vec::new(),
        };

        let result = verify_artifact(&artifact, None).unwrap();
        assert!(!result.verified);
    }

    #[test]
    fn real_flash_args_are_bound_to_explicit_port_and_artifact_root() {
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec!["web_serial".to_string()],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: "app".to_string(),
                path: "firmware.bin".to_string(),
                sha256: "sha256:test".to_string(),
                size: 9,
                flash_address: Some(0x10000),
            }],
        };

        let dir = tempdir().unwrap();
        let args =
            build_espflash_args(&artifact, Some(dir.path()), "/dev/cu.usbmodem21221401").unwrap();

        assert!(
            args.windows(2)
                .any(|pair| pair == ["--port", "/dev/cu.usbmodem21221401"])
        );
        assert!(args.contains(&"65536".to_string()));
        assert!(args.iter().any(|arg| arg.ends_with("firmware.bin")));
    }

    #[test]
    fn wifi_response_redacts_password_shape() {
        let request = WifiConfigRequest {
            lease_id: "lease-1".to_string(),
            op: WifiConfigOp::Set,
            ssid: Some("FluxPurr-Lab".to_string()),
            password: Some("secret-pass".to_string()),
            auto_reconnect: Some(true),
            telemetry_interval_ms: Some(500),
        };
        let value = json!({
            "wifi": {
                "op": request.op,
                "ssid": request.ssid,
                "password": request.password.as_ref().map(|_| "<redacted>")
            }
        });
        assert!(value.to_string().contains("<redacted>"));
        assert!(!value.to_string().contains("secret-pass"));
    }
}
