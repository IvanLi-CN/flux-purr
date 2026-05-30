#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{self, Read},
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{process::Command, sync::broadcast};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

pub const DEFAULT_EVENT_LIMIT: usize = 1_000;
pub const DEFAULT_LOG_LIMIT: usize = 2_000;
pub const DEFAULT_TRACE_LIMIT: usize = 2_000;
pub const DEFAULT_LEASE_TTL_MS: u64 = 8_000;
pub const DEFAULT_BAUD_RATE: u32 = 115_200;
pub const DEFAULT_SERIAL_PORT: &str = "/dev/cu.usbmodem21221401";
const DEFAULT_APP_FLASH_ADDRESS: u64 = 0x10000;
const FRONT_PANEL_PRESET_COUNT: usize = 10;
const SERIAL_RPC_TIMEOUT: Duration = Duration::from_millis(12_000);
const SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(50);
const SERIAL_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);
const SERIAL_SILENT_RETRY_DELAY: Duration = Duration::from_millis(250);
const SERIAL_LINE_LIMIT: usize = 4_096;
#[cfg(unix)]
const LOCK_EX: i32 = 2;
#[cfg(unix)]
const LOCK_NB: i32 = 4;
#[cfg(unix)]
const LOCK_UN: i32 = 8;

static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(unix)]
unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub artifact_root: Option<PathBuf>,
    pub allow_dev_cors: bool,
    pub allow_real_flash: bool,
    pub serial_port: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:30080".parse().unwrap(),
            artifact_root: None,
            allow_dev_cors: true,
            allow_real_flash: false,
            serial_port: Some(PathBuf::from(DEFAULT_SERIAL_PORT)),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: AppConfig,
    inner: Arc<Mutex<DevdState>>,
    events: broadcast::Sender<DevdEvent>,
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    serial_sessions: Arc<Mutex<SerialSessionMap>>,
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
            serial_rpc: Arc::new(tokio::sync::Mutex::new(())),
            serial_sessions: Arc::new(Mutex::new(HashMap::new())),
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
    dry_run_passes: HashMap<String, DryRunApproval>,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DryRunApproval {
    artifact_id: String,
    manifest_fingerprint: String,
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
            api_version: "2026-05-29".to_string(),
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
            selected_preset_slot: Some(1),
            presets_c: Some(vec![
                Some(50),
                Some(100),
                Some(120),
                Some(150),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ]),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_preset_slot: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presets_c: Option<Vec<Option<i16>>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigRequest {
    pub lease_id: String,
    pub target_temp_c: Option<i16>,
    pub selected_preset_slot: Option<usize>,
    pub presets_c: Option<Vec<Option<i16>>>,
    pub active_cooling_enabled: Option<bool>,
    pub heater_enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WifiConfigOp {
    Set,
    Clear,
}

impl WifiConfigOp {
    const fn usb_op(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbRequestWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbWifiConfigWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_reconnect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbRuntimeConfigWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_temp_c: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_preset_slot: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presets_c: Option<&'a Vec<Option<i16>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_cooling_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heater_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbResponseWire {
    #[serde(rename = "type")]
    frame_type: String,
    request_id: Option<String>,
    ok: Option<bool>,
    result: Option<Value>,
    error: Option<ApiError>,
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
pub struct FirmwareArtifactCatalog {
    pub artifacts: Vec<FirmwareArtifact>,
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
        .route(
            "/api/v1/devices/{device_id}/runtime",
            put(configure_runtime),
        )
        .route("/api/v1/artifacts", get(list_artifacts_route))
        .route("/api/v1/artifacts/verify", post(verify_artifact_route))
        .route("/api/v1/devices/{device_id}/flash", post(flash_device))
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
    let serial_devices = scan_serial_devices(state.config.serial_port.as_deref());
    let mut state_lock = state.lock()?;
    refresh_serial_devices(&mut state_lock, serial_devices);
    let devices = state_lock.devices.values().cloned().collect::<Vec<_>>();
    Ok(Json(json!({ "devices": devices })))
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
    Ok(Json(target.identity))
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
    Ok(Json(target.status))
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

    Ok(device.events.iter().cloned().collect())
}

fn devd_event_to_sse(event: DevdEvent) -> Event {
    let kind = event.kind.clone();
    let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
    Event::default().event(kind).data(data)
}

async fn configure_wifi(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<WifiConfigRequest>,
) -> Result<Json<Value>, HttpError> {
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
        if let Err(error) = serial_wifi_config(&state, &target, &payload).await {
            record_serial_bridge_error(&state, &device_id, "wifi_config", &error);
            return Err(error);
        }
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
                record_serial_bridge_error(&state, &device_id, "network_after_wifi", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.network = network.clone();
            device.status.network = network.clone();
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_wifi_config_event(&state, &device_id, &payload);
        return Ok(Json(json!({
            "accepted": true,
            "network": network,
            "wifi": {
                "op": payload.op,
                "ssid": payload.ssid,
                "password": payload.password.as_ref().map(|_| "<redacted>"),
                "autoReconnect": payload.auto_reconnect,
                "telemetryIntervalMs": payload.telemetry_interval_ms
            }
        })));
    }

    let mut state_lock = state.lock()?;
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
    drop(state_lock);
    emit_wifi_config_event(&state, &device_id, &payload);
    Ok(Json(redacted))
}

async fn configure_runtime(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<RuntimeConfigRequest>,
) -> Result<Json<ControlPlaneStatus>, HttpError> {
    validate_runtime_config(&payload)?;
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
        let status = match serial_runtime_config(&state, &target, &payload).await {
            Ok(status) => status,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "runtime_config", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.status = status.clone();
            device.network = status.network.clone();
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_runtime_config_event(&state, &device_id, &payload, &status);
        return Ok(Json(status));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    if let Some(target_temp_c) = payload.target_temp_c {
        device.status.target_temp_c = target_temp_c;
    }
    if let Some(selected_preset_slot) = payload.selected_preset_slot {
        device.status.selected_preset_slot = Some(selected_preset_slot);
    }
    if let Some(presets_c) = &payload.presets_c {
        device.status.presets_c = Some(presets_c.clone());
        if payload.target_temp_c.is_none()
            && let Some(selected_preset_slot) = device.status.selected_preset_slot
            && let Some(Some(target_temp_c)) = presets_c.get(selected_preset_slot)
        {
            device.status.target_temp_c = *target_temp_c;
        }
    }
    if let Some(active_cooling_enabled) = payload.active_cooling_enabled {
        device.status.active_cooling_enabled = active_cooling_enabled;
    }
    if let Some(heater_enabled) = payload.heater_enabled {
        device.status.heater_enabled = heater_enabled;
        if !heater_enabled {
            device.status.heater_output_percent = 0;
        }
    }
    let status = device.status.clone();
    drop(state_lock);
    emit_runtime_config_event(&state, &device_id, &payload, &status);
    Ok(Json(status))
}

fn validate_runtime_config(payload: &RuntimeConfigRequest) -> Result<(), HttpError> {
    if payload
        .selected_preset_slot
        .is_some_and(|slot| slot >= FRONT_PANEL_PRESET_COUNT)
    {
        return Err(HttpError::bad_request(
            "invalid_preset_slot",
            "selectedPresetSlot must be between 0 and 9.",
        ));
    }
    if payload
        .presets_c
        .as_ref()
        .is_some_and(|presets| presets.len() != FRONT_PANEL_PRESET_COUNT)
    {
        return Err(HttpError::bad_request(
            "invalid_presets",
            "presetsC must contain exactly 10 values.",
        ));
    }
    Ok(())
}

async fn verify_artifact_route(
    State(state): State<AppState>,
    Json(payload): Json<ArtifactVerifyRequest>,
) -> Result<Json<ArtifactVerifyResult>, HttpError> {
    verify_artifact(&payload.artifact, state.config.artifact_root.as_deref())
        .map(Json)
        .map_err(sanitize_io_error)
}

async fn list_artifacts_route(
    State(state): State<AppState>,
) -> Result<Json<FirmwareArtifactCatalog>, HttpError> {
    discover_firmware_artifacts(state.config.artifact_root.as_deref())
        .map(|artifacts| Json(FirmwareArtifactCatalog { artifacts }))
        .map_err(sanitize_io_error)
}

async fn serial_request_payload<T>(
    state: &AppState,
    target: &DeviceRecord,
    op: &'static str,
    payload_key: &'static str,
) -> Result<T, HttpError>
where
    T: DeserializeOwned + Send + 'static,
{
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-{op}", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op,
    })
    .map_err(|_| HttpError::internal("failed to encode USB request"))?;
    let result = serial_exchange(state, &target.id, port_path, request_id, request, true).await?;
    extract_usb_payload(result, payload_key)
}

async fn serial_wifi_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &WifiConfigRequest,
) -> Result<Value, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-wifi", now_millis());
    let request = serde_json::to_string(&UsbWifiConfigWire {
        frame_type: "wifi_config",
        request_id: &request_id,
        op: payload.op.usb_op(),
        ssid: payload.ssid.as_deref(),
        password: payload.password.as_deref(),
        auto_reconnect: payload.auto_reconnect,
        telemetry_interval_ms: payload.telemetry_interval_ms,
    })
    .map_err(|_| HttpError::internal("failed to encode USB WiFi request"))?;
    serial_exchange(state, &target.id, port_path, request_id, request, false).await
}

async fn serial_runtime_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &RuntimeConfigRequest,
) -> Result<ControlPlaneStatus, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-runtime", now_millis());
    let request = serde_json::to_string(&UsbRuntimeConfigWire {
        frame_type: "runtime_config",
        request_id: &request_id,
        target_temp_c: payload.target_temp_c,
        selected_preset_slot: payload.selected_preset_slot,
        presets_c: payload.presets_c.as_ref(),
        active_cooling_enabled: payload.active_cooling_enabled,
        heater_enabled: payload.heater_enabled,
    })
    .map_err(|_| HttpError::internal("failed to encode USB runtime request"))?;
    let result = serial_exchange(state, &target.id, port_path, request_id, request, false).await?;
    extract_usb_payload(result, "status")
}

async fn serial_exchange(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_on_silence: bool,
) -> Result<Value, HttpError> {
    record_transport_event(state, device_id, "tx", "usb_jsonl", &request_id, &request);
    let _serial_rpc = state.serial_rpc.lock().await;
    let serial_sessions = state.serial_sessions.clone();
    let worker_request_id = request_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        serial_exchange_blocking(
            &serial_sessions,
            &port_path,
            &worker_request_id,
            &request,
            retry_on_silence,
        )
    })
    .await
    .map_err(|_| HttpError::internal("serial worker failed"))?;

    match &result {
        Ok(payload) => record_transport_event(
            state,
            device_id,
            "rx",
            "usb_jsonl",
            &request_id,
            &serde_json::to_string(&json!({
                "type": "response",
                "requestId": request_id,
                "ok": true,
                "result": payload,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        ),
        Err(error) => record_transport_event(
            state,
            device_id,
            "rx",
            "usb_jsonl",
            &request_id,
            &serde_json::to_string(&json!({
                "type": "response",
                "requestId": request_id,
                "ok": false,
                "error": error.error,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        ),
    }

    result
}

fn native_port_path(target: &DeviceRecord) -> Result<String, HttpError> {
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "Native serial transport is required.",
        ));
    }
    target.port_path.clone().ok_or_else(|| {
        HttpError::bad_request("missing_port", "Native serial device has no port path.")
    })
}

fn extract_usb_payload<T>(result: Value, payload_key: &'static str) -> Result<T, HttpError>
where
    T: DeserializeOwned,
{
    let payload = result.get(payload_key).cloned().ok_or_else(|| {
        HttpError::new(
            StatusCode::BAD_GATEWAY,
            "usb_payload_missing",
            "USB response did not include the expected payload.",
            true,
        )
    })?;
    serde_json::from_value(payload).map_err(|_| {
        HttpError::new(
            StatusCode::BAD_GATEWAY,
            "usb_payload_decode_failed",
            "USB response payload could not be decoded.",
            true,
        )
    })
}

fn serial_exchange_blocking(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
    request_id: &str,
    request: &str,
    retry_on_silence: bool,
) -> Result<Value, HttpError> {
    let deadline = Instant::now() + SERIAL_RPC_TIMEOUT;
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let mut session = take_or_open_serial_session(&mut serial_sessions, port_path, deadline)?;
    write_serial_request(&mut *session.port, request)?;

    let mut next_silent_retry_at = Instant::now() + SERIAL_SILENT_RETRY_DELAY;
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => {
                maybe_retry_silent_serial_request(
                    &mut *session.port,
                    request,
                    retry_on_silence,
                    &mut next_silent_retry_at,
                    deadline,
                )?;
            }
            Ok(read) => {
                for byte in &read_buf[..read] {
                    if *byte == b'\n' {
                        match decode_usb_response_line(&line, request_id) {
                            Ok(Some(payload)) => {
                                store_serial_session(&mut serial_sessions, port_path, session);
                                return Ok(payload);
                            }
                            Ok(None) => {}
                            Err(error)
                                if is_retryable_startup_busy(&error)
                                    && Instant::now() < deadline =>
                            {
                                std::thread::sleep(SERIAL_STARTUP_RETRY_DELAY);
                                write_serial_request(&mut *session.port, request)?;
                            }
                            Err(error) => {
                                store_serial_session(&mut serial_sessions, port_path, session);
                                return Err(error);
                            }
                        }
                        line.clear();
                    } else if line.len() < SERIAL_LINE_LIMIT {
                        line.push(*byte);
                    } else {
                        line.clear();
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                maybe_retry_silent_serial_request(
                    &mut *session.port,
                    request,
                    retry_on_silence,
                    &mut next_silent_retry_at,
                    deadline,
                )?;
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                write_serial_request(&mut *session.port, request)?;
                next_silent_retry_at = Instant::now() + SERIAL_SILENT_RETRY_DELAY;
                line.clear();
            }
            Err(error) => return Err(serial_io_http_error(error)),
        }
    }

    store_serial_session(&mut serial_sessions, port_path, session);
    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "usb_response_timeout",
        "Timed out waiting for a matching USB JSONL response.",
        true,
    ))
}

type SerialSessionMap = HashMap<String, SerialSession>;

struct SerialSession {
    _serial_lock: SerialPortProcessLock,
    port: Box<dyn serialport::SerialPort>,
}

fn lock_serial_sessions(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
) -> Result<MutexGuard<'_, SerialSessionMap>, HttpError> {
    serial_sessions
        .lock()
        .map_err(|_| HttpError::internal("serial session lock poisoned"))
}

fn take_or_open_serial_session(
    serial_sessions: &mut SerialSessionMap,
    port_path: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    serial_sessions
        .remove(port_path)
        .map(Ok)
        .unwrap_or_else(|| open_serial_session(port_path, deadline))
}

fn store_serial_session(
    serial_sessions: &mut SerialSessionMap,
    port_path: &str,
    session: SerialSession,
) {
    serial_sessions.insert(port_path.to_string(), session);
}

fn release_cached_serial_session(state: &AppState, port_path: &str) -> Result<(), HttpError> {
    let mut serial_sessions = lock_serial_sessions(&state.serial_sessions)?;
    serial_sessions.remove(port_path);
    Ok(())
}

struct SerialPortProcessLock {
    #[cfg(unix)]
    file: File,
}

impl SerialPortProcessLock {
    fn acquire(port_path: &str, deadline: Instant) -> Result<Self, HttpError> {
        #[cfg(unix)]
        {
            let lock_path = serial_lock_path(port_path);
            let file = File::options()
                .create(true)
                .read(true)
                .write(true)
                .open(&lock_path)
                .map_err(|error| {
                    HttpError::new(
                        StatusCode::BAD_GATEWAY,
                        "serial_lock_failed",
                        &format!(
                            "Failed to open serial lock {}: {error}",
                            lock_path.display()
                        ),
                        true,
                    )
                })?;

            while Instant::now() < deadline {
                // SAFETY: flock is called with a valid file descriptor owned by `file`.
                let lock_result = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
                if lock_result == 0 {
                    return Ok(Self { file });
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::WouldBlock {
                    return Err(serial_io_http_error(error));
                }
                std::thread::sleep(SERIAL_READ_TIMEOUT);
            }

            Err(HttpError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "serial_lock_timeout",
                "Timed out waiting for exclusive USB serial access.",
                true,
            ))
        }

        #[cfg(not(unix))]
        {
            let _ = (port_path, deadline);
            Ok(Self {})
        }
    }
}

#[cfg(unix)]
impl Drop for SerialPortProcessLock {
    fn drop(&mut self) {
        // SAFETY: flock is called with a valid file descriptor owned by `self.file`.
        let _ = unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
    }
}

#[cfg(unix)]
fn serial_lock_path(port_path: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(port_path.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::from("flux-purr-devd-serial-");
    for byte in &digest[..8] {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".lock");
    std::env::temp_dir().join(name)
}

fn open_serial_port(port_path: &str) -> Result<Box<dyn serialport::SerialPort>, HttpError> {
    let mut port = serialport::new(port_path, DEFAULT_BAUD_RATE)
        .dtr_on_open(false)
        .timeout(SERIAL_READ_TIMEOUT)
        .open()
        .map_err(|error| {
            HttpError::new(
                StatusCode::BAD_GATEWAY,
                "serial_open_failed",
                &format!("Failed to open serial port: {error}"),
                true,
            )
        })?;
    let _ = port.write_request_to_send(false);
    let _ = port.write_data_terminal_ready(false);
    Ok(port)
}

fn open_serial_session(port_path: &str, deadline: Instant) -> Result<SerialSession, HttpError> {
    let serial_lock = SerialPortProcessLock::acquire(port_path, deadline)?;
    let port = open_serial_port(port_path)?;
    Ok(SerialSession {
        _serial_lock: serial_lock,
        port,
    })
}

fn reopen_serial_session(port_path: &str, deadline: Instant) -> Result<SerialSession, HttpError> {
    while Instant::now() < deadline {
        if Path::new(port_path).exists() {
            match open_serial_session(port_path, deadline) {
                Ok(session) => return Ok(session),
                Err(error) if error.error.retryable => {}
                Err(error) => return Err(error),
            }
        }
        std::thread::sleep(SERIAL_STARTUP_RETRY_DELAY);
    }

    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "serial_reconnect_timeout",
        "Timed out waiting for the USB serial port to reappear.",
        true,
    ))
}

fn write_serial_request(
    port: &mut dyn serialport::SerialPort,
    request: &str,
) -> Result<(), HttpError> {
    port.write_all(request.as_bytes())
        .and_then(|_| port.write_all(b"\n"))
        .and_then(|_| port.flush())
        .map_err(serial_io_http_error)
}

fn maybe_retry_silent_serial_request(
    port: &mut dyn serialport::SerialPort,
    request: &str,
    retry_on_silence: bool,
    next_retry_at: &mut Instant,
    deadline: Instant,
) -> Result<(), HttpError> {
    let now = Instant::now();
    if should_retry_silent_serial_request(retry_on_silence, now, *next_retry_at, deadline) {
        write_serial_request(port, request)?;
        *next_retry_at = now + SERIAL_SILENT_RETRY_DELAY;
    }
    Ok(())
}

fn should_retry_silent_serial_request(
    retry_on_silence: bool,
    now: Instant,
    next_retry_at: Instant,
    deadline: Instant,
) -> bool {
    retry_on_silence && now >= next_retry_at && now < deadline
}

fn is_recoverable_serial_io_error(error: &io::Error) -> bool {
    let message = error.to_string();
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof
    ) || message.contains("Device not configured")
        || message.contains("device not configured")
}

fn is_retryable_startup_busy(error: &HttpError) -> bool {
    error.error.retryable && error.error.code == "startup_busy"
}

fn decode_usb_response_line(line: &[u8], request_id: &str) -> Result<Option<Value>, HttpError> {
    let Ok(text) = std::str::from_utf8(line) else {
        return Ok(None);
    };
    let Ok(frame) = serde_json::from_str::<UsbResponseWire>(text.trim()) else {
        return Ok(None);
    };
    if frame.request_id.as_deref() != Some(request_id) {
        return Ok(None);
    }
    match frame.frame_type.as_str() {
        "response" if frame.ok == Some(true) => Ok(Some(frame.result.unwrap_or(Value::Null))),
        "response" | "error" => Err(usb_frame_error(frame)),
        _ => Ok(None),
    }
}

fn usb_frame_error(frame: UsbResponseWire) -> HttpError {
    HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: frame.error.unwrap_or_else(|| ApiError {
            code: "usb_error".to_string(),
            message: "Firmware returned an unsuccessful USB response.".to_string(),
            retryable: true,
            details: None,
        }),
    }
}

fn serial_io_http_error(error: io::Error) -> HttpError {
    HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        &format!("Serial I/O failed: {error}"),
        true,
    )
}

async fn flash_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FlashRequest>,
) -> Result<Json<FlashResult>, HttpError> {
    let artifact_id = payload.artifact.artifact_id.clone();
    let approval = dry_run_approval(&payload.artifact);
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
        state.emit(event(
            &device_id,
            "flash",
            "artifact verification failed",
            json!({ "artifactId": artifact_id, "code": "artifact_verify_failed" }),
        ));
        return Err(HttpError::bad_request(
            "artifact_verify_failed",
            "Firmware artifact verification failed.",
        ));
    }

    if payload.dry_run {
        let mut state_lock = state.lock()?;
        state_lock
            .dry_run_passes
            .insert(device_id.clone(), approval);
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.selected_artifact_id = Some(artifact_id.clone());
        }
        drop(state_lock);
        state.emit(event(
            &device_id,
            "flash",
            "artifact dry-run passed",
            json!({ "artifactId": artifact_id, "dryRun": true }),
        ));
        return Ok(Json(FlashResult {
            artifact_id,
            dry_run: true,
            status: "passed".to_string(),
            message: "Artifact verified; no flash write performed.".to_string(),
        }));
    }

    {
        let state_lock = state.lock()?;
        let prior = state_lock.dry_run_passes.get(&device_id);
        if prior != Some(&approval) {
            drop(state_lock);
            state.emit(event(
                &device_id,
                "flash",
                "real flash blocked",
                json!({ "artifactId": artifact_id, "code": "dry_run_required" }),
            ));
            return Err(HttpError::forbidden(
                "dry_run_required",
                "Real flash requires a successful dry-run for the same artifact.",
            ));
        }
    }

    if payload.confirm.as_deref() != Some("FLASH") {
        state.emit(event(
            &device_id,
            "flash",
            "real flash blocked",
            json!({ "artifactId": artifact_id, "code": "confirmation_required" }),
        ));
        return Err(HttpError::forbidden(
            "confirmation_required",
            "Real flash requires confirm=FLASH.",
        ));
    }

    if !state.config.allow_real_flash {
        state.emit(event(
            &device_id,
            "flash",
            "real flash blocked",
            json!({ "artifactId": artifact_id, "code": "real_flash_disabled" }),
        ));
        return Err(HttpError::forbidden(
            "real_flash_disabled",
            "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
        ));
    }

    state.emit(event(
        &device_id,
        "flash",
        "real flash started",
        json!({ "artifactId": artifact_id, "dryRun": false }),
    ));
    release_cached_serial_session(&state, &port_path)?;
    if let Err(error) = run_espflash(
        &payload.artifact,
        state.config.artifact_root.as_deref(),
        &port_path,
    )
    .await
    {
        state.emit(event(
            &device_id,
            "flash",
            "real flash failed",
            json!({ "artifactId": artifact_id, "code": error.error.code }),
        ));
        return Err(error);
    }
    {
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.selected_artifact_id = Some(artifact_id.clone());
        }
    }
    state.emit(event(
        &device_id,
        "flash",
        "real flash completed",
        json!({ "artifactId": artifact_id, "dryRun": false }),
    ));
    Ok(Json(FlashResult {
        artifact_id,
        dry_run: false,
        status: "completed".to_string(),
        message: "espflash command completed.".to_string(),
    }))
}

pub fn scan_serial_devices(serial_port: Option<&Path>) -> Vec<DeviceRecord> {
    let Some(serial_port) = serial_port else {
        return Vec::new();
    };
    if !serial_port.exists() {
        return Vec::new();
    }

    let port_name = serial_port.to_string_lossy().into_owned();
    let port_info = serialport::available_ports()
        .ok()
        .and_then(|ports| ports.into_iter().find(|port| port.port_name == port_name));
    vec![serial_device_record(&port_name, port_info.as_ref())]
}

fn refresh_serial_devices(state: &mut DevdState, serial_devices: Vec<DeviceRecord>) {
    let serial_ids = serial_devices
        .iter()
        .map(|device| device.id.clone())
        .collect::<HashSet<_>>();

    state.devices.retain(|_, device| {
        device.transport != DeviceTransport::NativeSerial || serial_ids.contains(&device.id)
    });
    state
        .leases
        .retain(|_, lease| state.devices.contains_key(&lease.device_id));

    for device in serial_devices {
        if let Some(existing) = state.devices.get_mut(&device.id) {
            existing.display_name = device.display_name;
            existing.port_path = device.port_path;
            existing.transport = device.transport;
        } else {
            state.devices.insert(device.id.clone(), device);
        }
    }
}

fn serial_device_record(
    port_name: &str,
    port_info: Option<&serialport::SerialPortInfo>,
) -> DeviceRecord {
    let (id, display_name) = match port_info.map(|port| &port.port_type) {
        Some(serialport::SerialPortType::UsbPort(info)) => {
            let serial = info
                .serial_number
                .clone()
                .unwrap_or_else(|| port_name.replace('/', "_"));
            (
                format!("serial-{:04x}-{:04x}-{serial}", info.vid, info.pid),
                info.product
                    .clone()
                    .unwrap_or_else(|| "USB serial device".to_string()),
            )
        }
        _ => (
            format!("serial-{}", port_name.replace('/', "_")),
            "Authorized serial device".to_string(),
        ),
    };
    let mut device = DeviceRecord::mock(&id, DeviceTransport::NativeSerial);
    device.display_name = display_name;
    device.port_path = Some(port_name.to_string());
    device.connection = ConnectionState::Disconnected;
    device.identity.capabilities = vec![
        "identity".to_string(),
        "status".to_string(),
        "network".to_string(),
        "wifi_config".to_string(),
        "monitor".to_string(),
        "firmware_check".to_string(),
        "flash".to_string(),
    ];
    device
}

fn dry_run_approval(artifact: &FirmwareArtifact) -> DryRunApproval {
    let manifest = serde_json::to_vec(artifact)
        .expect("FirmwareArtifact serialization should not fail for in-memory manifest data");
    DryRunApproval {
        artifact_id: artifact.artifact_id.clone(),
        manifest_fingerprint: format!("sha256:{:x}", Sha256::digest(manifest)),
    }
}

pub fn verify_artifact(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
) -> io::Result<ArtifactVerifyResult> {
    let mut files = Vec::new();
    for file in &artifact.files {
        let path = resolve_verified_artifact_path(root, &file.path)?;
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

pub fn discover_firmware_artifacts(root: Option<&Path>) -> io::Result<Vec<FirmwareArtifact>> {
    let candidates = [
        (
            "local-esp32s3-release",
            "Local ESP32-S3 release",
            "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
            "release + web_serial",
            vec!["web_serial".to_string()],
            "elf",
        ),
        (
            "local-host-release",
            "Local host release",
            "firmware/target/release/flux-purr",
            "host release",
            Vec::new(),
            "host_binary",
        ),
    ];
    let mut artifacts = Vec::new();

    for (artifact_id, name, path, profile, features, kind) in candidates {
        let resolved_path = resolve_artifact_path(root, path);
        if !resolved_path.is_file() {
            continue;
        }

        let bytes = fs::read(&resolved_path)?;
        let size = bytes.len() as u64;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        artifacts.push(FirmwareArtifact {
            artifact_id: artifact_id.to_string(),
            name: name.to_string(),
            version: "local-build".to_string(),
            git_sha: option_env!("VERGEN_GIT_SHA")
                .unwrap_or("unknown")
                .to_string(),
            build_id: digest
                .trim_start_matches("sha256:")
                .chars()
                .take(12)
                .collect(),
            target_chip: if artifact_id.contains("esp32s3") {
                "esp32s3".to_string()
            } else {
                "host".to_string()
            },
            profile: profile.to_string(),
            features,
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: kind.to_string(),
                path: path.to_string(),
                sha256: digest,
                size,
                flash_address: if kind == "app" {
                    Some(DEFAULT_APP_FLASH_ADDRESS)
                } else {
                    None
                },
            }],
        });
    }

    Ok(artifacts)
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

fn resolve_verified_artifact_path(root: Option<&Path>, path: &str) -> io::Result<PathBuf> {
    let relative = PathBuf::from(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact paths must stay inside the configured artifact root",
        ));
    }

    let base = fs::canonicalize(root.unwrap_or_else(|| Path::new(".")))?;
    let candidate = fs::canonicalize(base.join(relative))?;
    if !candidate.starts_with(&base) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "artifact path must stay inside the configured artifact root",
        ));
    }

    Ok(candidate)
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
    if let Some(elf_image) = artifact.files.iter().find(|file| file.kind == "elf") {
        let path = resolve_artifact_path(root, &elf_image.path);
        return Ok(vec![
            "flash".to_string(),
            "--chip".to_string(),
            artifact.target_chip.clone(),
            "--port".to_string(),
            port_path.to_string(),
            "--non-interactive".to_string(),
            "--after".to_string(),
            "hard-reset".to_string(),
            path.to_string_lossy().into_owned(),
        ]);
    }

    let Some(app_image) = artifact.files.iter().find(|file| file.kind == "app") else {
        return Err(HttpError::bad_request(
            "missing_flash_image",
            "Artifact does not contain an ELF or raw app image.",
        ));
    };
    let flash_address = app_image.flash_address.ok_or_else(|| {
        HttpError::bad_request("missing_flash_address", "Missing app flash address.")
    })?;
    let path = resolve_artifact_path(root, &app_image.path);
    Ok(vec![
        "write-bin".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--non-interactive".to_string(),
        "--after".to_string(),
        "hard-reset".to_string(),
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

fn record_serial_bridge_error(
    state: &AppState,
    device_id: &str,
    stage: &'static str,
    error: &HttpError,
) {
    if let Ok(mut state_lock) = state.lock()
        && let Some(device) = state_lock.devices.get_mut(device_id)
    {
        device.connection = ConnectionState::Error;
        device.network.state = if error.error.code == "usb_response_timeout" {
            NetworkState::Timeout
        } else {
            NetworkState::Error
        };
        device.network.last_error = Some(error.error.message.clone());
        device.status.network = device.network.clone();
    }
    state.emit(event(
        device_id,
        "serial",
        "native serial RPC failed",
        json!({
            "stage": stage,
            "code": error.error.code,
            "message": error.error.message,
            "retryable": error.error.retryable,
        }),
    ));
}

fn emit_wifi_config_event(state: &AppState, device_id: &str, payload: &WifiConfigRequest) {
    state.emit(event(
        device_id,
        "wifi",
        "wifi config accepted",
        json!({
            "op": payload.op,
            "ssid": payload.ssid,
            "passwordPresent": payload.password.is_some(),
            "autoReconnect": payload.auto_reconnect,
            "telemetryIntervalMs": payload.telemetry_interval_ms,
        }),
    ));
}

fn emit_runtime_config_event(
    state: &AppState,
    device_id: &str,
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) {
    state.emit(event(
        device_id,
        "runtime",
        "runtime config applied",
        json!({
            "requested": {
                "targetTempC": payload.target_temp_c,
                "selectedPresetSlot": payload.selected_preset_slot,
                "presetsC": payload.presets_c,
                "activeCoolingEnabled": payload.active_cooling_enabled,
                "heaterEnabled": payload.heater_enabled,
            },
            "status": {
                "targetTempC": status.target_temp_c,
                "selectedPresetSlot": status.selected_preset_slot,
                "presetsC": status.presets_c,
                "activeCoolingEnabled": status.active_cooling_enabled,
                "heaterEnabled": status.heater_enabled,
            },
        }),
    ));
}

fn record_transport_event(
    state: &AppState,
    device_id: &str,
    direction: &str,
    transport: &str,
    request_id: &str,
    frame_json: &str,
) {
    let frame = serde_json::from_str::<Value>(frame_json)
        .map(redact_transport_frame)
        .unwrap_or_else(|_| json!({ "raw": frame_json }));
    let frame_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("frame")
        .to_string();
    state.emit(event(
        device_id,
        "transport",
        "transport frame",
        json!({
            "direction": direction,
            "transport": transport,
            "requestId": request_id,
            "frameType": frame_type,
            "frame": frame,
        }),
    ));
}

fn redact_transport_frame(mut frame: Value) -> Value {
    redact_transport_secrets(&mut frame);
    frame
}

fn redact_transport_secrets(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if is_transport_secret_key(key) && !value.is_null() {
                    *value = Value::String("<redacted>".to_string());
                } else {
                    redact_transport_secrets(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_transport_secrets(value);
            }
        }
        _ => {}
    }
}

fn is_transport_secret_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("password") || key.eq_ignore_ascii_case("psk")
}

fn push_bounded<T>(values: &mut VecDeque<T>, value: T, limit: usize) {
    if values.len() >= limit {
        values.pop_front();
    }
    values.push_back(value);
}

fn event(device_id: &str, kind: &str, message: &str, payload: Value) -> DevdEvent {
    DevdEvent {
        id: format!(
            "event-{}-{}",
            now_millis(),
            EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
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
    fn dev_cors_origin_guard_allows_only_local_development_origins() {
        for origin in [
            "http://localhost:43690",
            "http://127.0.0.1:43690",
            "http://[::1]:43690",
            "https://localhost:43690",
        ] {
            assert!(
                is_allowed_dev_origin(&origin.parse::<HeaderValue>().unwrap()),
                "{origin} should be allowed"
            );
        }

        for origin in [
            "https://example.com",
            "http://localhost.evil.test:43690",
            "http://127.0.0.1.evil.test:43690",
            "file://localhost/console.html",
        ] {
            assert!(
                !is_allowed_dev_origin(&origin.parse::<HeaderValue>().unwrap()),
                "{origin} should be rejected"
            );
        }
    }

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

    #[tokio::test]
    async fn release_lease_records_device_event() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();

        let response = delete_lease(State(state.clone()), AxumPath(lease.lease_id.clone()))
            .await
            .unwrap()
            .0;

        assert_eq!(response["released"], true);
        let inner = state.lock().unwrap();
        let device = inner.devices.get("mock-fp-lab-01").unwrap();
        assert!(device.events.iter().any(|event| {
            event.kind == "lease"
                && event.message == "lease released"
                && event.payload["leaseId"] == lease.lease_id
        }));
    }

    #[test]
    fn device_event_backlog_replays_existing_bounded_events() {
        let state = AppState::test();
        state.emit(event(
            "mock-fp-lab-01",
            "lease",
            "lease created",
            json!({ "leaseId": "lease-test" }),
        ));
        state.emit(event(
            "other-device",
            "lease",
            "lease created",
            json!({ "leaseId": "lease-other" }),
        ));

        let backlog = device_event_backlog(&state, "mock-fp-lab-01").unwrap();

        assert_eq!(backlog.len(), 1);
        assert_eq!(backlog[0].kind, "lease");
        assert_eq!(backlog[0].payload["leaseId"], "lease-test");
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
    fn event_ids_are_unique_inside_same_millisecond_window() {
        let first = event(
            "mock-fp-lab-01",
            "runtime",
            "runtime config applied",
            json!({}),
        );
        let second = event(
            "mock-fp-lab-01",
            "runtime",
            "runtime config applied",
            json!({}),
        );

        assert_ne!(first.id, second.id);
    }

    #[test]
    fn transport_events_preserve_frame_data_and_redact_passwords() {
        let state = AppState::test();
        record_transport_event(
            &state,
            "mock-fp-lab-01",
            "tx",
            "usb_jsonl",
            "req-1",
            r#"{"type":"wifi_config","requestId":"req-1","ssid":"FluxPurr-Lab","password":"secret-pass"}"#,
        );

        let inner = state.lock().unwrap();
        let device = inner.devices.get("mock-fp-lab-01").unwrap();
        let transport_event = device
            .events
            .iter()
            .find(|event| event.kind == "transport")
            .unwrap();

        assert_eq!(transport_event.payload["direction"], "tx");
        assert_eq!(transport_event.payload["frame"]["ssid"], "FluxPurr-Lab");
        assert_eq!(transport_event.payload["frame"]["password"], "<redacted>");
        assert!(
            !serde_json::to_string(&transport_event.payload)
                .unwrap()
                .contains("secret-pass")
        );
    }

    #[test]
    fn transport_events_redact_nested_passwords() {
        let state = AppState::test();
        record_transport_event(
            &state,
            "mock-fp-lab-01",
            "rx",
            "usb_jsonl",
            "req-1",
            r#"{"type":"response","requestId":"req-1","ok":true,"result":{"wifi":{"ssid":"FluxPurr-Lab","password":"secret-pass","credentials":[{"psk":"nested-psk"}]}}}"#,
        );

        let inner = state.lock().unwrap();
        let device = inner.devices.get("mock-fp-lab-01").unwrap();
        let transport_event = device
            .events
            .iter()
            .find(|event| event.kind == "transport")
            .unwrap();

        assert_eq!(
            transport_event.payload["frame"]["result"]["wifi"]["password"],
            "<redacted>"
        );
        assert_eq!(
            transport_event.payload["frame"]["result"]["wifi"]["credentials"][0]["psk"],
            "<redacted>"
        );
        let encoded = serde_json::to_string(&transport_event.payload).unwrap();
        assert!(!encoded.contains("secret-pass"));
        assert!(!encoded.contains("nested-psk"));
    }

    #[test]
    fn serial_scan_ignores_missing_authorized_port() {
        let dir = tempdir().unwrap();
        let missing_port = dir.path().join("missing-usbmodem");

        assert!(scan_serial_devices(Some(&missing_port)).is_empty());
    }

    #[test]
    fn native_serial_devices_advertise_devd_flash_capabilities() {
        let device = serial_device_record("/dev/cu.usbmodem-test", None);

        assert_eq!(device.transport, DeviceTransport::NativeSerial);
        assert!(
            device
                .identity
                .capabilities
                .contains(&"firmware_check".to_string())
        );
        assert!(device.identity.capabilities.contains(&"flash".to_string()));
    }

    #[test]
    fn serial_refresh_removes_stale_native_devices_and_leases() {
        let mut state = DevdState::default();
        state.seed_mock_device();
        let mut serial_device = DeviceRecord::mock("serial-stale", DeviceTransport::NativeSerial);
        serial_device.port_path = Some("/dev/tty.Bluetooth-Incoming-Port".to_string());
        state
            .devices
            .insert(serial_device.id.clone(), serial_device.clone());
        state.leases.insert(
            "lease-stale".to_string(),
            WebLease {
                lease_id: "lease-stale".to_string(),
                device_id: serial_device.id,
                expires_at: Instant::now() + Duration::from_secs(1),
                ttl_ms: DEFAULT_LEASE_TTL_MS,
            },
        );

        refresh_serial_devices(&mut state, Vec::new());

        assert!(state.devices.contains_key("mock-fp-lab-01"));
        assert!(!state.devices.contains_key("serial-stale"));
        assert!(state.leases.is_empty());
    }

    #[test]
    fn serial_refresh_preserves_native_error_diagnostics() {
        let mut state = DevdState::default();
        let mut existing = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
        existing.port_path = Some("/dev/cu.usbmodem-test".to_string());
        existing.connection = ConnectionState::Error;
        existing.network.state = NetworkState::Timeout;
        existing.network.last_error = Some("Timed out waiting for USB response.".to_string());
        existing.events.push_back(event(
            "serial-known",
            "serial",
            "native serial RPC failed",
            json!({ "code": "usb_response_timeout" }),
        ));
        state.devices.insert(existing.id.clone(), existing);

        let mut refreshed = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
        refreshed.display_name = "USB JTAG/serial debug unit".to_string();
        refreshed.port_path = Some("/dev/cu.usbmodem-test".to_string());
        refreshed.connection = ConnectionState::Disconnected;

        refresh_serial_devices(&mut state, vec![refreshed]);

        let device = state.devices.get("serial-known").unwrap();
        assert_eq!(device.display_name, "USB JTAG/serial debug unit");
        assert_eq!(device.connection, ConnectionState::Error);
        assert_eq!(device.network.state, NetworkState::Timeout);
        assert_eq!(
            device.network.last_error.as_deref(),
            Some("Timed out waiting for USB response.")
        );
        assert_eq!(device.events.len(), 1);
    }

    #[test]
    fn serial_bridge_error_marks_device_and_records_event() {
        let state = AppState::test();
        let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
        serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
        {
            let mut inner = state.lock().unwrap();
            inner
                .devices
                .insert(serial_device.id.clone(), serial_device);
        }

        let error = HttpError::new(
            StatusCode::GATEWAY_TIMEOUT,
            "usb_response_timeout",
            "Timed out waiting for a matching USB JSONL response.",
            true,
        );

        record_serial_bridge_error(&state, "serial-known", "identity", &error);

        let inner = state.lock().unwrap();
        let device = inner.devices.get("serial-known").unwrap();
        assert_eq!(device.connection, ConnectionState::Error);
        assert_eq!(device.network.state, NetworkState::Timeout);
        assert_eq!(
            device.network.last_error.as_deref(),
            Some("Timed out waiting for a matching USB JSONL response.")
        );
        assert_eq!(device.events.len(), 1);
        assert_eq!(device.events[0].kind, "serial");
        assert_eq!(device.events[0].payload["stage"], "identity");
        assert_eq!(device.events[0].payload["code"], "usb_response_timeout");
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
    fn artifact_verify_rejects_paths_outside_artifact_root() {
        let dir = tempdir().unwrap();
        let artifact = FirmwareArtifact {
            artifact_id: "escaped-artifact".to_string(),
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
                path: "../firmware.bin".to_string(),
                sha256: "sha256:bad".to_string(),
                size: 9,
                flash_address: Some(0x10000),
            }],
        };
        let parent_escape = verify_artifact(&artifact, Some(dir.path())).unwrap_err();
        assert_eq!(parent_escape.kind(), io::ErrorKind::PermissionDenied);

        let mut absolute_artifact = artifact;
        absolute_artifact.files[0].path = "/etc/hosts".to_string();
        let absolute_escape = verify_artifact(&absolute_artifact, Some(dir.path())).unwrap_err();
        assert_eq!(absolute_escape.kind(), io::ErrorKind::PermissionDenied);
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
    fn artifact_catalog_discovers_local_build_outputs() {
        let dir = tempdir().unwrap();
        let artifact_path = dir
            .path()
            .join("firmware/target/xtensa-esp32s3-none-elf/release");
        fs::create_dir_all(&artifact_path).unwrap();
        fs::write(artifact_path.join("flux-purr"), b"firmware-image").unwrap();

        let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_id, "local-esp32s3-release");
        assert_eq!(artifacts[0].target_chip, "esp32s3");
        assert_eq!(artifacts[0].profile, "release + web_serial");
        assert_eq!(artifacts[0].features, ["web_serial"]);
        assert_eq!(artifacts[0].files[0].kind, "elf");
        assert_eq!(artifacts[0].files[0].size, 14);
        assert_eq!(artifacts[0].files[0].flash_address, None);
        assert!(artifacts[0].files[0].sha256.starts_with("sha256:"));
    }

    #[test]
    fn real_flash_args_flash_elf_and_hard_reset() {
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
                kind: "elf".to_string(),
                path: "firmware.elf".to_string(),
                sha256: "sha256:test".to_string(),
                size: 9,
                flash_address: None,
            }],
        };

        let dir = tempdir().unwrap();
        let args =
            build_espflash_args(&artifact, Some(dir.path()), "/dev/cu.usbmodem21221401").unwrap();

        assert_eq!(args[0], "flash");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--port", "/dev/cu.usbmodem21221401"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--after", "hard-reset"])
        );
        assert!(args.iter().any(|arg| arg.ends_with("firmware.elf")));
        assert!(!args.contains(&"65536".to_string()));
    }

    #[test]
    fn real_flash_args_write_raw_app_bin_with_explicit_address() {
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
                flash_address: Some(DEFAULT_APP_FLASH_ADDRESS),
            }],
        };

        let dir = tempdir().unwrap();
        let args =
            build_espflash_args(&artifact, Some(dir.path()), "/dev/cu.usbmodem21221401").unwrap();

        assert_eq!(args[0], "write-bin");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--port", "/dev/cu.usbmodem21221401"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--after", "hard-reset"])
        );
        assert_eq!(
            args.iter()
                .position(|arg| arg == &DEFAULT_APP_FLASH_ADDRESS.to_string())
                .map(|index| index + 1)
                .and_then(|index| args.get(index))
                .map(|arg| arg.ends_with("firmware.bin")),
            Some(true)
        );
        assert!(args.iter().any(|arg| arg.ends_with("firmware.bin")));
    }

    #[tokio::test]
    async fn runtime_endpoint_requires_valid_lease() {
        let state = AppState::test();
        let error = configure_runtime(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: "missing-lease".to_string(),
                target_temp_c: Some(230),
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(error.error.code, "lease_expired");
    }

    #[tokio::test]
    async fn daemon_local_device_mutations_require_valid_lease() {
        let state = AppState::test();
        let missing_lease = bind_device(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(LeaseQuery {
                lease_id: Some("missing-lease".to_string()),
            }),
            Json(BindRequest {
                alias: Some("Bench Alias".to_string()),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(missing_lease.status, StatusCode::FORBIDDEN);
        assert_eq!(missing_lease.error.code, "lease_expired");

        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let bound = bind_device(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(LeaseQuery {
                lease_id: Some(lease.lease_id.clone()),
            }),
            Json(BindRequest {
                alias: Some("Bench Alias".to_string()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(bound.display_name, "Bench Alias");

        let connected = connect_device(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(LeaseQuery {
                lease_id: Some(lease.lease_id.clone()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(connected.connection, ConnectionState::Connected);

        let disconnected = disconnect_device(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(LeaseQuery {
                lease_id: Some(lease.lease_id),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(disconnected.connection, ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn wifi_and_runtime_successes_record_safe_events() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();

        let _ = configure_wifi(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(WifiConfigRequest {
                lease_id: lease.lease_id.clone(),
                op: WifiConfigOp::Set,
                ssid: Some("FluxPurr-Lab".to_string()),
                password: Some("secret-pass".to_string()),
                auto_reconnect: Some(true),
                telemetry_interval_ms: Some(500),
            }),
        )
        .await
        .unwrap();

        let _ = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id,
                target_temp_c: Some(231),
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: Some(false),
                heater_enabled: Some(false),
            }),
        )
        .await
        .unwrap();

        let inner = state.lock().unwrap();
        let device = inner.devices.get("mock-fp-lab-01").unwrap();
        let wifi_event = device
            .events
            .iter()
            .find(|event| event.kind == "wifi" && event.message == "wifi config accepted")
            .unwrap();
        assert_eq!(wifi_event.payload["ssid"], "FluxPurr-Lab");
        assert_eq!(wifi_event.payload["passwordPresent"], true);
        assert!(
            !serde_json::to_string(&wifi_event.payload)
                .unwrap()
                .contains("secret-pass")
        );

        let runtime_event = device
            .events
            .iter()
            .find(|event| event.kind == "runtime" && event.message == "runtime config applied")
            .unwrap();
        assert_eq!(runtime_event.payload["status"]["targetTempC"], 231);
        assert_eq!(
            runtime_event.payload["status"]["activeCoolingEnabled"],
            false
        );
        assert_eq!(runtime_event.payload["status"]["heaterEnabled"], false);
    }

    #[tokio::test]
    async fn real_flash_requires_dry_run_confirmation_and_allow_flag() {
        let dir = tempdir().unwrap();
        let artifact = test_artifact_with_file(dir.path(), "firmware.bin", b"firmware-image");
        let state = AppState::new(AppConfig {
            artifact_root: Some(dir.path().to_path_buf()),
            ..AppConfig::default()
        });
        let mut native = DeviceRecord::mock("serial-test", DeviceTransport::NativeSerial);
        native.port_path = Some("/dev/cu.usbmodem21221401".to_string());
        {
            let mut inner = state.lock().unwrap();
            inner.devices.insert(native.id.clone(), native);
        }
        let lease = state.lease_device("serial-test").unwrap();

        let without_dry_run = flash_device(
            State(state.clone()),
            AxumPath("serial-test".to_string()),
            Json(FlashRequest {
                lease_id: lease.lease_id.clone(),
                artifact: artifact.clone(),
                dry_run: false,
                confirm: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(without_dry_run.status, StatusCode::FORBIDDEN);
        assert_eq!(without_dry_run.error.code, "dry_run_required");

        let dry_run = flash_device(
            State(state.clone()),
            AxumPath("serial-test".to_string()),
            Json(FlashRequest {
                lease_id: lease.lease_id.clone(),
                artifact: artifact.clone(),
                dry_run: true,
                confirm: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(dry_run.dry_run);
        assert_eq!(dry_run.status, "passed");
        {
            let inner = state.lock().unwrap();
            let device = inner.devices.get("serial-test").unwrap();
            assert_eq!(
                device.selected_artifact_id.as_deref(),
                Some("test-artifact")
            );
            assert!(device.events.iter().any(|event| {
                event.kind == "flash"
                    && event.message == "artifact dry-run passed"
                    && event.payload["artifactId"] == "test-artifact"
            }));
        }

        let changed_artifact =
            test_artifact_with_file(dir.path(), "firmware-v2.bin", b"firmware-image-v2");
        let changed_manifest = flash_device(
            State(state.clone()),
            AxumPath("serial-test".to_string()),
            Json(FlashRequest {
                lease_id: lease.lease_id.clone(),
                artifact: changed_artifact,
                dry_run: false,
                confirm: Some("FLASH".to_string()),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(changed_manifest.status, StatusCode::FORBIDDEN);
        assert_eq!(changed_manifest.error.code, "dry_run_required");

        let without_confirm = flash_device(
            State(state.clone()),
            AxumPath("serial-test".to_string()),
            Json(FlashRequest {
                lease_id: lease.lease_id.clone(),
                artifact: artifact.clone(),
                dry_run: false,
                confirm: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(without_confirm.status, StatusCode::FORBIDDEN);
        assert_eq!(without_confirm.error.code, "confirmation_required");

        let flash_disabled = flash_device(
            State(state.clone()),
            AxumPath("serial-test".to_string()),
            Json(FlashRequest {
                lease_id: lease.lease_id,
                artifact,
                dry_run: false,
                confirm: Some("FLASH".to_string()),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(flash_disabled.status, StatusCode::FORBIDDEN);
        assert_eq!(flash_disabled.error.code, "real_flash_disabled");
        {
            let inner = state.lock().unwrap();
            let device = inner.devices.get("serial-test").unwrap();
            assert!(device.events.iter().any(|event| {
                event.kind == "flash"
                    && event.message == "real flash blocked"
                    && event.payload["code"] == "real_flash_disabled"
            }));
        }
    }

    #[test]
    fn release_cached_serial_session_removes_only_target_port() {
        let state = AppState::test();
        let dir = tempdir().unwrap();
        {
            let mut sessions = state.serial_sessions.lock().unwrap();
            sessions.insert(
                "/dev/cu.usbmodem-target".to_string(),
                test_serial_session(&dir, "target.lock"),
            );
            sessions.insert(
                "/dev/cu.usbmodem-other".to_string(),
                test_serial_session(&dir, "other.lock"),
            );
        }

        release_cached_serial_session(&state, "/dev/cu.usbmodem-target").unwrap();

        let sessions = state.serial_sessions.lock().unwrap();
        assert!(!sessions.contains_key("/dev/cu.usbmodem-target"));
        assert!(sessions.contains_key("/dev/cu.usbmodem-other"));
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

    #[test]
    fn usb_response_decoder_ignores_logs_and_selects_matching_request() {
        assert!(
            decode_usb_response_line(b"INFO firmware booted", "req-1")
                .unwrap()
                .is_none()
        );
        assert!(
            decode_usb_response_line(
                br#"{"type":"response","requestId":"other","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
                "req-1"
            )
            .unwrap()
            .is_none()
        );

        let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
            "req-1",
        )
        .unwrap()
        .unwrap();

        let network = extract_usb_payload::<NetworkSummary>(payload, "network").unwrap();
        assert_eq!(network.state, NetworkState::Disabled);
    }

    #[test]
    fn usb_response_decoder_extracts_runtime_config_status_payload() {
        let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"runtime-1","ok":true,"result":{"status":{"mode":"sampling","uptimeSeconds":12,"currentTempC":194.0,"targetTempC":240,"heaterEnabled":true,"heaterOutputPercent":25,"activeCoolingEnabled":false,"fanDisplayState":"AUTO","fanEnabled":true,"fanPwmPermille":500,"voltageMv":20000,"currentMa":850,"boardTempCenti":1940,"pdRequestMv":20000,"pdContractMv":20000,"pdState":"ready","frontpanelKey":null,"network":{"state":"idle","dns":[],"wifiRssi":null}}}}"#,
            "runtime-1",
        )
        .unwrap()
        .unwrap();

        let status = extract_usb_payload::<ControlPlaneStatus>(payload, "status").unwrap();

        assert_eq!(status.target_temp_c, 240);
        assert!(status.heater_enabled);
        assert!(!status.active_cooling_enabled);
    }

    #[test]
    fn usb_response_decoder_maps_firmware_errors() {
        let error = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":false,"error":{"code":"bad_op","message":"Bad op","retryable":false}}"#,
            "req-1",
        )
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.error.code, "bad_op");
        assert!(!error.error.retryable);
    }

    #[test]
    fn usb_response_decoder_maps_documented_error_frames() {
        let error = decode_usb_response_line(
            br#"{"type":"error","requestId":"req-1","error":{"code":"bad_frame","message":"Malformed JSONL frame.","retryable":false}}"#,
            "req-1",
        )
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.error.code, "bad_frame");
        assert!(!error.error.retryable);
    }

    #[test]
    fn usb_response_decoder_marks_startup_busy_retryable() {
        let error = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":false,"error":{"code":"startup_busy","message":"Runtime status is not available until hardware initialization completes.","retryable":true}}"#,
            "req-1",
        )
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert!(is_retryable_startup_busy(&error));
    }

    #[test]
    fn silent_serial_retry_only_applies_to_read_only_policy_window() {
        let now = Instant::now();
        let retry_at = now - Duration::from_millis(1);
        let deadline = now + Duration::from_millis(100);

        assert!(should_retry_silent_serial_request(
            true, now, retry_at, deadline
        ));
        assert!(!should_retry_silent_serial_request(
            false, now, retry_at, deadline
        ));
        assert!(!should_retry_silent_serial_request(
            true,
            now,
            now + Duration::from_millis(1),
            deadline
        ));
        assert!(!should_retry_silent_serial_request(
            true, now, retry_at, now
        ));
    }

    fn test_artifact_with_file(root: &Path, relative_path: &str, bytes: &[u8]) -> FirmwareArtifact {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();

        FirmwareArtifact {
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
                path: relative_path.to_string(),
                sha256: format!("sha256:{:x}", Sha256::digest(bytes)),
                size: bytes.len() as u64,
                flash_address: Some(0x10000),
            }],
        }
    }

    fn test_serial_session(dir: &tempfile::TempDir, lock_name: &str) -> SerialSession {
        #[cfg(unix)]
        let serial_lock = SerialPortProcessLock {
            file: File::options()
                .create(true)
                .read(true)
                .write(true)
                .open(dir.path().join(lock_name))
                .unwrap(),
        };
        #[cfg(not(unix))]
        let serial_lock = SerialPortProcessLock {};

        SerialSession {
            _serial_lock: serial_lock,
            port: Box::new(MockSerialPort),
        }
    }

    #[derive(Default)]
    struct MockSerialPort;

    impl io::Read for MockSerialPort {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    impl io::Write for MockSerialPort {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl serialport::SerialPort for MockSerialPort {
        fn name(&self) -> Option<String> {
            Some("mock".to_string())
        }

        fn baud_rate(&self) -> serialport::Result<u32> {
            Ok(DEFAULT_BAUD_RATE)
        }

        fn data_bits(&self) -> serialport::Result<serialport::DataBits> {
            Ok(serialport::DataBits::Eight)
        }

        fn flow_control(&self) -> serialport::Result<serialport::FlowControl> {
            Ok(serialport::FlowControl::None)
        }

        fn parity(&self) -> serialport::Result<serialport::Parity> {
            Ok(serialport::Parity::None)
        }

        fn stop_bits(&self) -> serialport::Result<serialport::StopBits> {
            Ok(serialport::StopBits::One)
        }

        fn timeout(&self) -> Duration {
            SERIAL_READ_TIMEOUT
        }

        fn set_baud_rate(&mut self, _baud_rate: u32) -> serialport::Result<()> {
            Ok(())
        }

        fn set_data_bits(&mut self, _data_bits: serialport::DataBits) -> serialport::Result<()> {
            Ok(())
        }

        fn set_flow_control(
            &mut self,
            _flow_control: serialport::FlowControl,
        ) -> serialport::Result<()> {
            Ok(())
        }

        fn set_parity(&mut self, _parity: serialport::Parity) -> serialport::Result<()> {
            Ok(())
        }

        fn set_stop_bits(&mut self, _stop_bits: serialport::StopBits) -> serialport::Result<()> {
            Ok(())
        }

        fn set_timeout(&mut self, _timeout: Duration) -> serialport::Result<()> {
            Ok(())
        }

        fn write_request_to_send(&mut self, _level: bool) -> serialport::Result<()> {
            Ok(())
        }

        fn write_data_terminal_ready(&mut self, _level: bool) -> serialport::Result<()> {
            Ok(())
        }

        fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
            Ok(false)
        }

        fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
            Ok(false)
        }

        fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
            Ok(false)
        }

        fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
            Ok(false)
        }

        fn bytes_to_read(&self) -> serialport::Result<u32> {
            Ok(0)
        }

        fn bytes_to_write(&self) -> serialport::Result<u32> {
            Ok(0)
        }

        fn clear(&self, _buffer_to_clear: serialport::ClearBuffer) -> serialport::Result<()> {
            Ok(())
        }

        fn try_clone(&self) -> serialport::Result<Box<dyn serialport::SerialPort>> {
            Ok(Box::new(Self))
        }

        fn set_break(&self) -> serialport::Result<()> {
            Ok(())
        }

        fn clear_break(&self) -> serialport::Result<()> {
            Ok(())
        }
    }
}
