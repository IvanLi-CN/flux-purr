#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
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
pub const DEVICE_LIST_EVENT_LIMIT: usize = 24;
pub const DEVICE_EVENT_REPLAY_LIMIT: usize = 120;
pub const DEFAULT_LEASE_TTL_MS: u64 = 8_000;
pub const DEFAULT_BAUD_RATE: u32 = 115_200;
pub const DEFAULT_SERIAL_PORT: &str = "/dev/cu.usbmodem21221401";
pub const DEFAULT_DEVD_URL: &str = "http://127.0.0.1:30080";
const DEFAULT_PD_REQUEST_MV: u16 = 20_000;
const PPS_HARDWARE_MIN_MV: u16 = 5_000;
const PPS_HARDWARE_MAX_MV: u16 = 28_000;
const ADC_CALIBRATION_MAX_SAMPLES: usize = 8;
const HEATER_CURVE_MAX_POINTS: usize = 8;
const RTD_DEFAULT_HIGH_MV: u16 = 2_800;
const VIN_DEFAULT_HIGH_MV: u16 = 2_337;
const RTD_REFERENCE_RESISTOR_OHMS: f32 = 2_490.0;
const RTD_DIVIDER_SUPPLY_MV: f32 = 3_000.0;
const PT1000_R0_OHMS: f32 = 1_000.0;
const PT1000_A: f32 = 3.9083e-3;
const PT1000_B: f32 = -5.775e-7;
const PT1000_C: f32 = -4.183e-12;
const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
const USER_CONFIG_FILE: &str = "config.json";
const HARDWARE_REGISTRY_FILE: &str = "devices.json";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SerialRetryPolicy {
    ReadOnly,
    SingleShot,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_devd_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_serial_port: Option<String>,
}

pub fn user_config_dir() -> io::Result<PathBuf> {
    if let Some(home) = env::var_os("FLUX_PURR_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }

    match env::consts::OS {
        "macos" => env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("Flux Purr")
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set")),
        "windows" => env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|appdata| appdata.join("Flux Purr"))
            .or_else(|| {
                env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config").join("flux-purr"))
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "APPDATA or USERPROFILE is not set")
            }),
        _ => env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .map(|xdg| xdg.join("flux-purr"))
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config").join("flux-purr"))
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "XDG_CONFIG_HOME or HOME is not set",
                )
            }),
    }
}

pub fn user_config_path() -> io::Result<PathBuf> {
    Ok(user_config_dir()?.join(USER_CONFIG_FILE))
}

pub fn hardware_registry_path() -> io::Result<PathBuf> {
    Ok(user_config_dir()?.join(HARDWARE_REGISTRY_FILE))
}

pub fn read_user_config() -> io::Result<UserConfig> {
    let path = user_config_path()?;
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(UserConfig::default());
    }
    serde_json::from_str(&content)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn write_user_config(config: &UserConfig) -> io::Result<()> {
    let path = user_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(config)?)
}

pub fn read_default_serial_port_from_user_config() -> Option<PathBuf> {
    read_user_config()
        .ok()
        .and_then(|config| config.default_serial_port)
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
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
        let state = DevdState::default();

        Self {
            config,
            inner: Arc::new(Mutex::new(state)),
            events,
            serial_rpc: Arc::new(tokio::sync::Mutex::new(())),
            serial_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn test() -> Self {
        let state = Self::new(AppConfig::default());
        state
            .inner
            .lock()
            .expect("test devd state lock")
            .seed_mock_device();
        state
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
    dry_run_passes: HashMap<String, FlashDryRunApproval>,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashDryRunApproval {
    lease_id: String,
    artifact_fingerprint: String,
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
    pub calibration: CalibrationState,
    pub heater_curve: HeaterCurveState,
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
                "calibration".to_string(),
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
            rtd_raw_adc_mv: Some(1_123),
            vin_raw_adc_mv: Some(1_678),
            pd_request_mv: DEFAULT_PD_REQUEST_MV,
            pd_contract_mv: DEFAULT_PD_REQUEST_MV,
            pd_state: "ready".to_string(),
            manual_pps_enabled: false,
            manual_pps_mv: None,
            manual_pps_ma: None,
            pps_capability_min_mv: Some(5_000),
            pps_capability_max_mv: Some(21_000),
            pps_capability_max_ma: Some(3_000),
            manual_pps_error: None,
            calibration: CalibrationRuntimeState::default(),
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
            calibration: CalibrationState::default(),
            heater_curve: HeaterCurveState::default(),
            selected_artifact_id: None,
            logs: VecDeque::new(),
            trace: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    fn native_serial_placeholder(id: &str, display_name: String, port_path: String) -> Self {
        let network = NetworkSummary {
            state: NetworkState::Idle,
            ssid: None,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: None,
        };
        let status = ControlPlaneStatus {
            mode: "idle".to_string(),
            uptime_seconds: 0,
            current_temp_c: 0.0,
            target_temp_c: 220,
            selected_preset_slot: None,
            presets_c: None,
            heater_enabled: false,
            heater_output_percent: 0,
            active_cooling_enabled: true,
            fan_display_state: "OFF".to_string(),
            fan_enabled: false,
            fan_pwm_permille: 0,
            voltage_mv: 0,
            current_ma: 0,
            board_temp_centi: 0,
            rtd_raw_adc_mv: None,
            vin_raw_adc_mv: None,
            pd_request_mv: DEFAULT_PD_REQUEST_MV,
            pd_contract_mv: 0,
            pd_state: "unknown".to_string(),
            manual_pps_enabled: false,
            manual_pps_mv: None,
            manual_pps_ma: None,
            pps_capability_min_mv: None,
            pps_capability_max_mv: None,
            pps_capability_max_ma: None,
            manual_pps_error: None,
            calibration: CalibrationRuntimeState::default(),
            frontpanel_key: None,
            network: network.clone(),
        };

        Self {
            id: id.to_string(),
            display_name,
            port_path: Some(port_path),
            transport: DeviceTransport::NativeSerial,
            connection: ConnectionState::Disconnected,
            identity: Identity {
                device_id: id.to_string(),
                firmware_version: "unknown".to_string(),
                build_id: "native-serial-placeholder".to_string(),
                git_sha: "unknown".to_string(),
                board: "unknown".to_string(),
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
            },
            network,
            status,
            calibration: CalibrationState::default(),
            heater_curve: HeaterCurveState::default(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtd_raw_adc_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vin_raw_adc_mv: Option<u16>,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: String,
    #[serde(default)]
    pub manual_pps_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_pps_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_pps_ma: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pps_capability_min_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pps_capability_max_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pps_capability_max_ma: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_pps_error: Option<String>,
    #[serde(default)]
    pub calibration: CalibrationRuntimeState,
    pub frontpanel_key: Option<String>,
    pub network: NetworkSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationMode {
    #[default]
    Off,
    VinAdc,
    RtdAdc,
    HeaterCurve,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobKind {
    VinAdcAuto,
    HeaterCurveAuto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobOp {
    Start,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationJobState {
    pub kind: Option<CalibrationJobKind>,
    pub status: CalibrationJobStatus,
    pub progress_percent: u8,
    pub samples_collected: u8,
    pub next_request_mv: Option<u16>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationRuntimeState {
    pub mode: CalibrationMode,
    pub pps_enabled: bool,
    pub pps_mv: Option<u16>,
    pub pps_ma: Option<u16>,
    pub heater_enabled: bool,
    pub target_adc_mv: Option<u16>,
    pub stable: bool,
    pub stability_error_mv: Option<i16>,
    pub error: Option<String>,
    pub job: CalibrationJobState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationChannel {
    RtdAdc,
    VinAdc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSample {
    pub observed_mv: u16,
    pub expected_mv: u16,
    pub reference_temp_c: Option<f32>,
    pub reference_vin_mv: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationPackage {
    pub rtd_adc: Vec<Option<CalibrationSample>>,
    pub vin_adc: Vec<Option<CalibrationSample>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationFit {
    pub gain: f32,
    pub offset_mv: f32,
    pub custom_sample_count: usize,
    pub default_sample_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationFits {
    pub rtd_adc: CalibrationFit,
    pub vin_adc: CalibrationFit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationState {
    pub active: CalibrationPackage,
    pub draft: CalibrationPackage,
    pub active_fit: CalibrationFits,
    pub draft_fit: CalibrationFits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePoint {
    pub temp_centi_c: i16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePackage {
    pub points: Vec<Option<HeaterCurvePoint>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveState {
    pub active: HeaterCurvePackage,
    pub preview: Option<HeaterCurvePackage>,
}

impl Default for CalibrationPackage {
    fn default() -> Self {
        Self {
            rtd_adc: vec![None; ADC_CALIBRATION_MAX_SAMPLES],
            vin_adc: vec![None; ADC_CALIBRATION_MAX_SAMPLES],
        }
    }
}

impl Default for HeaterCurvePackage {
    fn default() -> Self {
        Self {
            points: vec![None; HEATER_CURVE_MAX_POINTS],
        }
    }
}

impl Default for HeaterCurveState {
    fn default() -> Self {
        Self {
            active: HeaterCurvePackage::default(),
            preview: None,
        }
    }
}

impl Default for CalibrationState {
    fn default() -> Self {
        let package = CalibrationPackage::default();
        Self::from_packages(package.clone(), package)
    }
}

impl CalibrationState {
    fn from_packages(active: CalibrationPackage, draft: CalibrationPackage) -> Self {
        Self {
            active_fit: CalibrationFits::from_package(&active),
            draft_fit: CalibrationFits::from_package(&draft),
            active,
            draft,
        }
    }

    fn refresh_fits(&mut self) {
        self.active_fit = CalibrationFits::from_package(&self.active);
        self.draft_fit = CalibrationFits::from_package(&self.draft);
    }
}

impl CalibrationFits {
    fn from_package(package: &CalibrationPackage) -> Self {
        Self {
            rtd_adc: fit_calibration_channel(&package.rtd_adc, CalibrationChannel::RtdAdc),
            vin_adc: fit_calibration_channel(&package.vin_adc, CalibrationChannel::VinAdc),
        }
    }
}

impl CalibrationPackage {
    fn channel_mut(&mut self, channel: CalibrationChannel) -> &mut Vec<Option<CalibrationSample>> {
        match channel {
            CalibrationChannel::RtdAdc => &mut self.rtd_adc,
            CalibrationChannel::VinAdc => &mut self.vin_adc,
        }
    }
}

fn fit_calibration_channel(
    samples: &[Option<CalibrationSample>],
    channel: CalibrationChannel,
) -> CalibrationFit {
    let custom: Vec<CalibrationSample> = samples.iter().flatten().copied().collect();
    let defaults = default_calibration_samples(channel);
    let default_sample_count = if custom.len() < 2 { defaults.len() } else { 0 };
    let mut points = if custom.len() < 2 {
        defaults.to_vec()
    } else {
        Vec::new()
    };
    points.extend(custom.iter().copied());
    if points.len() < 2 {
        return CalibrationFit {
            gain: 1.0,
            offset_mv: 0.0,
            custom_sample_count: custom.len(),
            default_sample_count,
        };
    }

    let n = points.len() as f32;
    let sum_x = points
        .iter()
        .map(|sample| sample.observed_mv as f32)
        .sum::<f32>();
    let sum_y = points
        .iter()
        .map(|sample| sample.expected_mv as f32)
        .sum::<f32>();
    let sum_xx = points
        .iter()
        .map(|sample| {
            let x = sample.observed_mv as f32;
            x * x
        })
        .sum::<f32>();
    let sum_xy = points
        .iter()
        .map(|sample| sample.observed_mv as f32 * sample.expected_mv as f32)
        .sum::<f32>();
    let denominator = (n * sum_xx) - (sum_x * sum_x);
    let (gain, offset_mv) = if denominator.abs() < f32::EPSILON {
        (1.0, (sum_y - sum_x) / n)
    } else {
        let gain = ((n * sum_xy) - (sum_x * sum_y)) / denominator;
        (gain, (sum_y - gain * sum_x) / n)
    };
    CalibrationFit {
        gain,
        offset_mv,
        custom_sample_count: custom.len(),
        default_sample_count,
    }
}

fn default_calibration_samples(channel: CalibrationChannel) -> [CalibrationSample; 2] {
    match channel {
        CalibrationChannel::RtdAdc => [
            CalibrationSample {
                observed_mv: 0,
                expected_mv: 0,
                reference_temp_c: None,
                reference_vin_mv: None,
            },
            CalibrationSample {
                observed_mv: RTD_DEFAULT_HIGH_MV,
                expected_mv: RTD_DEFAULT_HIGH_MV,
                reference_temp_c: None,
                reference_vin_mv: None,
            },
        ],
        CalibrationChannel::VinAdc => [
            CalibrationSample {
                observed_mv: 0,
                expected_mv: 0,
                reference_temp_c: None,
                reference_vin_mv: None,
            },
            CalibrationSample {
                observed_mv: VIN_DEFAULT_HIGH_MV,
                expected_mv: VIN_DEFAULT_HIGH_MV,
                reference_temp_c: None,
                reference_vin_mv: None,
            },
        ],
    }
}

fn rtd_adc_mv_for_temperature_c(temp_c: f32) -> u16 {
    let resistance = {
        let polynomial = 1.0 + PT1000_A * temp_c + PT1000_B * temp_c * temp_c;
        if temp_c >= 0.0 {
            PT1000_R0_OHMS * polynomial
        } else {
            PT1000_R0_OHMS * (polynomial + PT1000_C * (temp_c - 100.0) * temp_c * temp_c * temp_c)
        }
    };
    ((RTD_DIVIDER_SUPPLY_MV * resistance) / (RTD_REFERENCE_RESISTOR_OHMS + resistance))
        .round()
        .clamp(0.0, u16::MAX as f32) as u16
}

fn vin_adc_mv_for_input_mv(input_mv: u32) -> u16 {
    let denominator = VIN_DIVIDER_R_HIGH_OHMS + VIN_DIVIDER_R_LOW_OHMS;
    input_mv
        .saturating_mul(VIN_DIVIDER_R_LOW_OHMS)
        .checked_div(denominator)
        .unwrap_or(0)
        .min(u32::from(u16::MAX)) as u16
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
    pub manual_pps_enabled: Option<bool>,
    pub manual_pps_mv: Option<u16>,
    pub manual_pps_ma: Option<u16>,
    pub calibration: Option<CalibrationControlRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationControlRequest {
    pub mode: Option<CalibrationMode>,
    pub pps_enabled: Option<bool>,
    pub pps_mv: Option<u16>,
    pub heater_enabled: Option<bool>,
    pub target_adc_mv: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationConfigRequest {
    pub lease_id: String,
    pub op: CalibrationConfigOp,
    pub channel: Option<CalibrationChannel>,
    pub reference_temp_c: Option<f32>,
    pub reference_vin_mv: Option<u32>,
    pub observed_mv: Option<u16>,
    pub expected_mv: Option<u16>,
    pub sample_index: Option<usize>,
    pub package: Option<CalibrationPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationApplyRequest {
    pub lease_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveConfigRequest {
    pub lease_id: String,
    pub op: HeaterCurveConfigOp,
    pub package: Option<HeaterCurvePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveSaveRequest {
    pub lease_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationJobRequest {
    pub lease_id: String,
    pub op: CalibrationJobOp,
    pub kind: Option<CalibrationJobKind>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationConfigOp {
    Capture,
    Delete,
    Clear,
    Import,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeaterCurveConfigOp {
    Preview,
    ClearPreview,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    manual_pps_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manual_pps_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manual_pps_ma: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    calibration: Option<&'a CalibrationControlRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbCalibrationConfigWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: CalibrationConfigOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<CalibrationChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_temp_c: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_vin_mv: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a CalibrationPackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbCalibrationApplyWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbHeaterCurveConfigWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: HeaterCurveConfigOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    heater_curve: Option<&'a HeaterCurvePackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbHeaterCurveSaveWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbCalibrationJobWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: CalibrationJobOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<CalibrationJobKind>,
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
        .route(
            "/api/v1/devices/{device_id}/calibration",
            get(device_calibration).put(configure_calibration),
        )
        .route(
            "/api/v1/devices/{device_id}/calibration/apply",
            post(apply_calibration),
        )
        .route(
            "/api/v1/devices/{device_id}/calibration/job",
            get(device_calibration_job).post(configure_calibration_job),
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
    let devices = state_lock
        .devices
        .values()
        .cloned()
        .map(trim_device_record_for_list)
        .map(device_list_payload)
        .collect::<Vec<_>>();
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

async fn apply_calibration(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<CalibrationApplyRequest>,
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

    if target.status.heater_enabled || target.status.heater_output_percent != 0 {
        return Err(HttpError::forbidden(
            "calibration_apply_heater_active",
            "Calibration cannot be applied while the heater is active.",
        ));
    }

    if target.transport == DeviceTransport::NativeSerial {
        let calibration = match serial_calibration_apply(&state, &target).await {
            Ok(calibration) => calibration,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "calibration_apply", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.calibration = calibration.clone();
            device.connection = ConnectionState::Connected;
        }
        drop(state_lock);
        emit_calibration_apply_event(&state, &device_id, &calibration);
        return Ok(Json(calibration));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    device.calibration.active = device.calibration.draft.clone();
    device.calibration.refresh_fits();
    let calibration = device.calibration.clone();
    drop(state_lock);
    emit_calibration_apply_event(&state, &device_id, &calibration);
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

    Ok(Json(target.status.calibration.job))
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

    if target.transport == DeviceTransport::NativeSerial {
        let job = match serial_calibration_job_config(&state, &target, &payload).await {
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

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    match payload.op {
        CalibrationJobOp::Cancel => {
            device.status.calibration.job = CalibrationJobState {
                status: CalibrationJobStatus::Canceled,
                ..CalibrationJobState::default()
            };
            device.status.calibration.heater_enabled = false;
            device.status.calibration.pps_enabled = false;
            device.status.calibration.pps_mv = None;
            device.status.calibration.pps_ma = None;
        }
        CalibrationJobOp::Start => {
            let kind = payload.kind.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_job_kind_required",
                    "Calibration auto job requires a job kind.",
                )
            })?;
            device.status.calibration.job = CalibrationJobState {
                kind: Some(kind),
                status: CalibrationJobStatus::Running,
                progress_percent: 0,
                samples_collected: 0,
                next_request_mv: device.status.calibration.pps_mv,
                message: None,
            };
        }
    }
    Ok(Json(device.status.calibration.job.clone()))
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
    if event.kind == "transport" {
        if let Some(payload) = event.payload.as_object_mut() {
            payload.remove("frame");
        }
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

    validate_manual_pps_request_against_status(&payload, &target.status)?;
    if let Some(calibration) = payload.calibration.as_ref() {
        validate_calibration_request_against_status(
            calibration,
            &target.status,
            &target.status.calibration,
        )?;
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
    if payload.manual_pps_enabled == Some(false) {
        device.status.manual_pps_enabled = false;
        device.status.manual_pps_mv = None;
        device.status.manual_pps_ma = None;
        device.status.pd_request_mv = DEFAULT_PD_REQUEST_MV;
        device.status.pd_contract_mv = DEFAULT_PD_REQUEST_MV;
        device.status.voltage_mv = u32::from(DEFAULT_PD_REQUEST_MV);
        device.status.manual_pps_error = None;
    } else if payload.manual_pps_enabled == Some(true)
        || payload.manual_pps_mv.is_some()
        || payload.manual_pps_ma.is_some()
    {
        let manual_pps_mv = payload
            .manual_pps_mv
            .or(device.status.manual_pps_mv)
            .expect("manual PPS voltage validated");
        let manual_pps_ma = payload
            .manual_pps_ma
            .or(device.status.manual_pps_ma)
            .or(effective_pps_current_capability_ma(&device.status))
            .expect("manual PPS current validated");
        device.status.manual_pps_enabled = true;
        device.status.manual_pps_mv = Some(manual_pps_mv);
        device.status.manual_pps_ma = Some(manual_pps_ma);
        device.status.pd_request_mv = manual_pps_mv;
        device.status.pd_contract_mv = manual_pps_mv;
        device.status.voltage_mv = u32::from(manual_pps_mv);
        device.status.manual_pps_error = None;
    }
    if let Some(calibration) = payload.calibration.as_ref() {
        apply_mock_calibration_runtime_config(&mut device.status, calibration);
    }
    let status = device.status.clone();
    drop(state_lock);
    emit_runtime_config_event(&state, &device_id, &payload, &status);
    Ok(Json(status))
}

fn apply_mock_calibration_runtime_config(
    status: &mut ControlPlaneStatus,
    calibration: &CalibrationControlRequest,
) {
    let current_ma = effective_pps_current_capability_ma(status);
    if let Some(mode) = calibration.mode {
        status.calibration.mode = mode;
        if mode == CalibrationMode::Off {
            status.calibration = CalibrationRuntimeState::default();
        }
    }

    if let Some(target_adc_mv) = calibration.target_adc_mv {
        status.calibration.target_adc_mv = Some(target_adc_mv);
    }

    if let Some(heater_enabled) = calibration.heater_enabled {
        status.calibration.heater_enabled = heater_enabled;
        status.heater_enabled = heater_enabled;
        if !heater_enabled {
            status.heater_output_percent = 0;
        }
    }

    if calibration.pps_enabled == Some(false) {
        status.calibration.pps_enabled = false;
        status.calibration.pps_mv = None;
        status.calibration.pps_ma = None;
        return;
    }

    if calibration.pps_enabled == Some(true) || calibration.pps_mv.is_some() {
        let pps_mv = calibration
            .pps_mv
            .or(status.calibration.pps_mv)
            .or(status.manual_pps_mv)
            .unwrap_or(status.pd_contract_mv);
        let pps_ma = current_ma.or(status.manual_pps_ma);
        status.calibration.pps_enabled = true;
        status.calibration.pps_mv = Some(pps_mv);
        status.calibration.pps_ma = pps_ma;
        status.manual_pps_enabled = true;
        status.manual_pps_mv = Some(pps_mv);
        status.manual_pps_ma = pps_ma;
        status.pd_request_mv = pps_mv;
        status.pd_contract_mv = pps_mv;
        status.voltage_mv = u32::from(pps_mv);
        status.manual_pps_error = None;
        status.calibration.error = None;
    }

    let observed_mv = match status.calibration.mode {
        CalibrationMode::RtdAdc => status.rtd_raw_adc_mv,
        CalibrationMode::VinAdc => status.vin_raw_adc_mv,
        CalibrationMode::Off | CalibrationMode::HeaterCurve => None,
    };

    status.calibration.stability_error_mv = status
        .calibration
        .target_adc_mv
        .zip(observed_mv)
        .map(|(target, observed)| (i32::from(observed) - i32::from(target)) as i16);
    status.calibration.stable = status
        .calibration
        .stability_error_mv
        .is_some_and(|error_mv| error_mv.abs() <= 8);
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
    if payload.manual_pps_mv.is_some_and(|millivolts| {
        !millivolts.is_multiple_of(100)
            || !(PPS_HARDWARE_MIN_MV..=PPS_HARDWARE_MAX_MV).contains(&millivolts)
    }) {
        return Err(HttpError::bad_request(
            "invalid_manual_pps",
            "manualPpsMv must use 100mV steps and stay within 5000..28000.",
        ));
    }
    if payload
        .manual_pps_ma
        .is_some_and(|milliamps| !milliamps.is_multiple_of(50) || milliamps == 0)
    {
        return Err(HttpError::bad_request(
            "invalid_manual_pps",
            "manualPpsMa must use 50mA steps and be greater than 0.",
        ));
    }
    if let Some(calibration) = payload.calibration.as_ref() {
        validate_calibration_control_request(calibration)?;
    }
    Ok(())
}

fn validate_calibration_control_request(
    calibration: &CalibrationControlRequest,
) -> Result<(), HttpError> {
    if calibration.pps_mv.is_some_and(|millivolts| {
        !millivolts.is_multiple_of(100)
            || !(PPS_HARDWARE_MIN_MV..=PPS_HARDWARE_MAX_MV).contains(&millivolts)
    }) {
        return Err(HttpError::bad_request(
            "invalid_calibration_pps",
            "calibration.ppsMv must use 100mV steps and stay within 5000..28000.",
        ));
    }
    Ok(())
}

fn apply_mock_calibration_config(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) -> Result<(), HttpError> {
    match payload.op {
        CalibrationConfigOp::Capture => {
            let channel = payload.channel.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_channel_required",
                    "Calibration capture requires a channel.",
                )
            })?;
            let observed_mv = payload
                .observed_mv
                .unwrap_or_else(|| mock_observed_adc_mv(channel));
            let expected_mv = expected_calibration_adc_mv(payload, channel).ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_reference_required",
                    "Calibration capture requires a valid physical reference.",
                )
            })?;
            let samples = calibration.draft.channel_mut(channel);
            let Some(slot) = samples.iter_mut().find(|slot| slot.is_none()) else {
                return Err(HttpError::bad_request(
                    "calibration_samples_full",
                    "Calibration channel already has 8 samples.",
                ));
            };
            *slot = Some(CalibrationSample {
                observed_mv,
                expected_mv,
                reference_temp_c: payload
                    .reference_temp_c
                    .filter(|_| channel == CalibrationChannel::RtdAdc),
                reference_vin_mv: payload
                    .reference_vin_mv
                    .and_then(|millivolts| u16::try_from(millivolts).ok())
                    .filter(|_| channel == CalibrationChannel::VinAdc),
            });
        }
        CalibrationConfigOp::Delete => {
            let channel = payload.channel.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_channel_required",
                    "Calibration delete requires a channel.",
                )
            })?;
            let index = payload.sample_index.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_index_required",
                    "Calibration delete requires sampleIndex.",
                )
            })?;
            let samples = calibration.draft.channel_mut(channel);
            let Some(slot) = samples.get_mut(index) else {
                return Err(HttpError::bad_request(
                    "calibration_sample_not_found",
                    "Calibration sample index was not present.",
                ));
            };
            if slot.is_none() {
                return Err(HttpError::bad_request(
                    "calibration_sample_not_found",
                    "Calibration sample index was not present.",
                ));
            }
            *slot = None;
            compact_calibration_samples(samples);
        }
        CalibrationConfigOp::Clear => {
            let channel = payload.channel.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_channel_required",
                    "Calibration clear requires a channel.",
                )
            })?;
            *calibration.draft.channel_mut(channel) = vec![None; ADC_CALIBRATION_MAX_SAMPLES];
        }
        CalibrationConfigOp::Import => {
            let package = payload.package.clone().ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_package_required",
                    "Calibration import requires a package.",
                )
            })?;
            validate_calibration_package(&package)?;
            calibration.draft = normalize_calibration_package(package);
        }
    }
    calibration.refresh_fits();
    Ok(())
}

fn compact_calibration_samples(samples: &mut Vec<Option<CalibrationSample>>) {
    let mut compacted: Vec<Option<CalibrationSample>> =
        samples.iter().flatten().copied().map(Some).collect();
    compacted.resize(ADC_CALIBRATION_MAX_SAMPLES, None);
    *samples = compacted;
}

fn normalize_calibration_sample(
    sample: CalibrationSample,
    channel: CalibrationChannel,
) -> CalibrationSample {
    match channel {
        CalibrationChannel::RtdAdc => CalibrationSample {
            reference_vin_mv: None,
            ..sample
        },
        CalibrationChannel::VinAdc => CalibrationSample {
            reference_temp_c: None,
            ..sample
        },
    }
}

fn validate_calibration_package(package: &CalibrationPackage) -> Result<(), HttpError> {
    if package.rtd_adc.len() > ADC_CALIBRATION_MAX_SAMPLES
        || package.vin_adc.len() > ADC_CALIBRATION_MAX_SAMPLES
    {
        return Err(HttpError::bad_request(
            "calibration_package_too_large",
            "Calibration import supports at most 8 samples per channel.",
        ));
    }
    Ok(())
}

fn normalize_calibration_package(mut package: CalibrationPackage) -> CalibrationPackage {
    package.rtd_adc = package
        .rtd_adc
        .into_iter()
        .map(|sample| {
            sample.map(|sample| normalize_calibration_sample(sample, CalibrationChannel::RtdAdc))
        })
        .collect();
    package.vin_adc = package
        .vin_adc
        .into_iter()
        .map(|sample| {
            sample.map(|sample| normalize_calibration_sample(sample, CalibrationChannel::VinAdc))
        })
        .collect();
    compact_calibration_samples(&mut package.rtd_adc);
    compact_calibration_samples(&mut package.vin_adc);
    package
}

fn validate_heater_curve_package(package: &HeaterCurvePackage) -> Result<(), HttpError> {
    if package.points.len() > HEATER_CURVE_MAX_POINTS {
        return Err(HttpError::bad_request(
            "heater_curve_package_too_large",
            "Heater curve supports at most 8 points.",
        ));
    }
    Ok(())
}

fn normalize_heater_curve_package(mut package: HeaterCurvePackage) -> HeaterCurvePackage {
    package
        .points
        .sort_by_key(|point| point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX));
    package.points.resize(HEATER_CURVE_MAX_POINTS, None);
    package
}

fn mock_observed_adc_mv(channel: CalibrationChannel) -> u16 {
    match channel {
        CalibrationChannel::RtdAdc => 1_120,
        CalibrationChannel::VinAdc => 1_670,
    }
}

fn expected_calibration_adc_mv(
    payload: &CalibrationConfigRequest,
    channel: CalibrationChannel,
) -> Option<u16> {
    if let Some(expected_mv) = payload.expected_mv {
        return Some(expected_mv);
    }
    match channel {
        CalibrationChannel::RtdAdc => payload.reference_temp_c.map(rtd_adc_mv_for_temperature_c),
        CalibrationChannel::VinAdc => payload.reference_vin_mv.map(vin_adc_mv_for_input_mv),
    }
}

fn effective_pps_current_capability_ma(status: &ControlPlaneStatus) -> Option<u16> {
    u16::try_from(status.current_ma)
        .ok()
        .filter(|value| *value > 0)
        .or(status.pps_capability_max_ma)
}

fn validate_pps_voltage_against_status(
    millivolts: u16,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    let (Some(min_mv), Some(max_mv)) = (status.pps_capability_min_mv, status.pps_capability_max_mv)
    else {
        return Err(HttpError::bad_request(
            "manual_pps_no_capability",
            "PPS capability is unavailable.",
        ));
    };
    if millivolts < min_mv || millivolts > max_mv {
        return Err(HttpError::bad_request(
            "manual_pps_out_of_range",
            "manualPpsMv is outside the advertised PPS capability.",
        ));
    }
    Ok(())
}

fn validate_manual_pps_request_against_status(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    if payload.manual_pps_enabled != Some(true)
        && payload.manual_pps_mv.is_none()
        && payload.manual_pps_ma.is_none()
    {
        return Ok(());
    }

    let manual_pps_mv = payload
        .manual_pps_mv
        .or(status.manual_pps_mv)
        .ok_or_else(|| HttpError::bad_request("invalid_manual_pps", "manualPpsMv is required."))?;
    let manual_pps_ma = payload
        .manual_pps_ma
        .or(status.manual_pps_ma)
        .or(status.pps_capability_max_ma)
        .ok_or_else(|| HttpError::bad_request("invalid_manual_pps", "manualPpsMa is required."))?;
    validate_manual_pps_against_status(manual_pps_mv, manual_pps_ma, status)
}

fn validate_calibration_request_against_status(
    calibration: &CalibrationControlRequest,
    status: &ControlPlaneStatus,
    current: &CalibrationRuntimeState,
) -> Result<(), HttpError> {
    let current_ma = effective_pps_current_capability_ma(status);
    if calibration.pps_enabled != Some(true) && calibration.pps_mv.is_none() {
        return Ok(());
    }

    let manual_pps_mv = calibration.pps_mv.or(current.pps_mv).ok_or_else(|| {
        HttpError::bad_request("invalid_calibration_pps", "calibration.ppsMv is required.")
    })?;
    let Some(_manual_pps_ma) = current_ma.or(status.manual_pps_ma) else {
        return Err(HttpError::bad_request(
            "invalid_calibration_pps",
            "Calibration PPS requires a readable PPS current capability.",
        ));
    };
    validate_pps_voltage_against_status(manual_pps_mv, status).map_err(|error| {
        if error.error.code == "manual_pps_no_capability" {
            HttpError::bad_request(
                "calibration_pps_no_capability",
                "PPS capability is unavailable.",
            )
        } else {
            HttpError::bad_request(
                "calibration_pps_out_of_range",
                "Calibration PPS request is outside the advertised PPS capability.",
            )
        }
    })?;
    Ok(())
}

fn validate_manual_pps_against_status(
    millivolts: u16,
    milliamps: u16,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    validate_pps_voltage_against_status(millivolts, status)?;
    let Some(max_ma) = status.pps_capability_max_ma else {
        return Err(HttpError::bad_request(
            "manual_pps_no_capability",
            "PPS capability is unavailable.",
        ));
    };
    if milliamps > max_ma {
        return Err(HttpError::bad_request(
            "manual_pps_out_of_range",
            "manualPpsMa is outside the advertised PPS capability.",
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
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::ReadOnly,
    )
    .await?;
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
    serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await
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
        manual_pps_enabled: payload.manual_pps_enabled,
        manual_pps_mv: payload.manual_pps_mv,
        manual_pps_ma: payload.manual_pps_ma,
        calibration: payload.calibration.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB runtime request"))?;
    match serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await
    {
        Ok(result) => extract_usb_payload(result, "status"),
        Err(error) if should_reconcile_runtime_config_timeout(&error) => {
            match serial_request_payload::<ControlPlaneStatus>(
                state,
                target,
                "get_status",
                "status",
            )
            .await
            {
                Ok(status) if runtime_config_matches_status(payload, &status) => Ok(status),
                Ok(_) | Err(_) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

async fn serial_calibration_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationState, HttpError> {
    serial_request_payload::<CalibrationState>(state, target, "get_calibration", "calibration")
        .await
}

async fn serial_calibration_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &CalibrationConfigRequest,
) -> Result<CalibrationState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-calibration", now_millis());
    let request = serde_json::to_string(&UsbCalibrationConfigWire {
        frame_type: "calibration_config",
        request_id: &request_id,
        op: payload.op,
        channel: payload.channel,
        reference_temp_c: payload.reference_temp_c,
        reference_vin_mv: payload.reference_vin_mv,
        observed_mv: payload.observed_mv,
        expected_mv: payload.expected_mv,
        sample_index: payload.sample_index,
        package: payload.package.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB calibration request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "calibration")
}

async fn serial_calibration_apply(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-calibration-apply", now_millis());
    let request = serde_json::to_string(&UsbCalibrationApplyWire {
        frame_type: "calibration_apply",
        request_id: &request_id,
    })
    .map_err(|_| HttpError::internal("failed to encode USB calibration apply request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "calibration")
}

async fn serial_calibration_job_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationJobState, HttpError> {
    serial_request_payload::<CalibrationJobState>(
        state,
        target,
        "get_calibration_job",
        "calibration_job",
    )
    .await
}

async fn serial_calibration_job_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &CalibrationJobRequest,
) -> Result<CalibrationJobState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-calibration-job", now_millis());
    let request = serde_json::to_string(&UsbCalibrationJobWire {
        frame_type: "calibration_job",
        request_id: &request_id,
        op: payload.op,
        kind: payload.kind,
    })
    .map_err(|_| HttpError::internal("failed to encode USB calibration job request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "calibration_job")
}

async fn serial_heater_curve_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<HeaterCurveState, HttpError> {
    serial_request_payload::<HeaterCurveState>(state, target, "get_heater_curve", "heater_curve")
        .await
}

async fn serial_heater_curve_config(
    state: &AppState,
    target: &DeviceRecord,
    payload: &HeaterCurveConfigRequest,
) -> Result<HeaterCurveState, HttpError> {
    let package = if let Some(package) = payload.package.as_ref() {
        validate_heater_curve_package(package)?;
        Some(normalize_heater_curve_package(package.clone()))
    } else {
        None
    };
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-heater-curve", now_millis());
    let request = serde_json::to_string(&UsbHeaterCurveConfigWire {
        frame_type: "heater_curve_config",
        request_id: &request_id,
        op: payload.op,
        heater_curve: package.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode USB heater curve request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "heater_curve")
}

async fn serial_heater_curve_save(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<HeaterCurveState, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-heater-curve-save", now_millis());
    let request = serde_json::to_string(&UsbHeaterCurveSaveWire {
        frame_type: "heater_curve_save",
        request_id: &request_id,
    })
    .map_err(|_| HttpError::internal("failed to encode USB heater curve save request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "heater_curve")
}

async fn serial_exchange(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
) -> Result<Value, HttpError> {
    record_transport_event(state, device_id, "tx", "usb_jsonl", &request_id, &request);
    let _serial_rpc = state.serial_rpc.lock().await;
    let serial_sessions = state.serial_sessions.clone();
    let worker_request_id = request_id.clone();
    let worker_device_id = device_id.to_string();
    let worker_events = state.events.clone();
    let worker_inner = state.inner.clone();
    let result = tokio::task::spawn_blocking(move || {
        serial_exchange_blocking(
            &worker_inner,
            &worker_events,
            &worker_device_id,
            &serial_sessions,
            &port_path,
            &worker_request_id,
            &request,
            retry_policy,
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
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
    request_id: &str,
    request: &str,
    retry_policy: SerialRetryPolicy,
) -> Result<Value, HttpError> {
    let deadline = Instant::now() + SERIAL_RPC_TIMEOUT;
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let mut session = take_or_open_serial_session(&mut serial_sessions, port_path, deadline)?;
    session = write_serial_request_with_reopen(session, port_path, request, deadline)?;

    let mut next_silent_retry_at = Instant::now() + SERIAL_SILENT_RETRY_DELAY;
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => {
                maybe_retry_silent_serial_request(
                    &mut *session.port,
                    request,
                    retry_policy,
                    &mut next_silent_retry_at,
                    deadline,
                )?;
            }
            Ok(read) => {
                for byte in &read_buf[..read] {
                    if *byte == b'\n' {
                        emit_serial_log_line(state, events, device_id, &line);
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
                                session = write_serial_request_with_reopen(
                                    session, port_path, request, deadline,
                                )?;
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
                    retry_policy,
                    &mut next_silent_retry_at,
                    deadline,
                )?;
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                session = write_serial_request_with_reopen(session, port_path, request, deadline)?;
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

fn emit_serial_log_line(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    line: &[u8],
) {
    let Ok(message) = std::str::from_utf8(line) else {
        return;
    };
    let message = message.trim();
    if message.is_empty() || message.starts_with('{') {
        return;
    }

    let event = event(
        device_id,
        "serial",
        "native serial monitor line",
        json!({
            "code": "firmware_log",
            "line": message,
        }),
    );

    if let Ok(mut inner) = state.lock() {
        inner.push_event(event.clone());
    }
    let _ = events.send(event);
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

fn write_serial_request_with_reopen(
    mut session: SerialSession,
    port_path: &str,
    request: &str,
    deadline: Instant,
) -> Result<SerialSession, HttpError> {
    match write_serial_request(&mut *session.port, request) {
        Ok(()) => Ok(session),
        Err(error) if is_recoverable_write_http_error(&error) => {
            drop(session);
            let mut reopened = reopen_serial_session(port_path, deadline)?;
            write_serial_request(&mut *reopened.port, request)?;
            Ok(reopened)
        }
        Err(error) => Err(error),
    }
}

fn maybe_retry_silent_serial_request(
    port: &mut dyn serialport::SerialPort,
    request: &str,
    retry_policy: SerialRetryPolicy,
    next_retry_at: &mut Instant,
    deadline: Instant,
) -> Result<(), HttpError> {
    let now = Instant::now();
    if should_retry_silent_serial_request(retry_policy, now, *next_retry_at, deadline) {
        write_serial_request(port, request)?;
        *next_retry_at = now + SERIAL_SILENT_RETRY_DELAY;
    }
    Ok(())
}

fn should_retry_silent_serial_request(
    retry_policy: SerialRetryPolicy,
    now: Instant,
    next_retry_at: Instant,
    deadline: Instant,
) -> bool {
    matches!(retry_policy, SerialRetryPolicy::ReadOnly) && now >= next_retry_at && now < deadline
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

fn should_reconcile_runtime_config_timeout(error: &HttpError) -> bool {
    error.error.retryable && error.error.code == "usb_response_timeout"
}

fn runtime_config_matches_status(
    payload: &RuntimeConfigRequest,
    status: &ControlPlaneStatus,
) -> bool {
    if payload
        .target_temp_c
        .is_some_and(|target_temp_c| status.target_temp_c != target_temp_c)
    {
        return false;
    }
    if payload
        .selected_preset_slot
        .is_some_and(|selected_preset_slot| {
            status.selected_preset_slot != Some(selected_preset_slot)
        })
    {
        return false;
    }
    if let Some(presets_c) = payload.presets_c.as_ref()
        && status.presets_c.as_ref() != Some(presets_c)
    {
        return false;
    }
    if payload
        .active_cooling_enabled
        .is_some_and(|enabled| status.active_cooling_enabled != enabled)
    {
        return false;
    }
    if payload
        .heater_enabled
        .is_some_and(|enabled| status.heater_enabled != enabled)
    {
        return false;
    }
    if payload
        .manual_pps_enabled
        .is_some_and(|enabled| status.manual_pps_enabled != enabled)
    {
        return false;
    }
    if payload
        .manual_pps_mv
        .is_some_and(|manual_pps_mv| status.manual_pps_mv != Some(manual_pps_mv))
    {
        return false;
    }
    if payload
        .manual_pps_ma
        .is_some_and(|manual_pps_ma| status.manual_pps_ma != Some(manual_pps_ma))
    {
        return false;
    }
    if let Some(calibration) = payload.calibration.as_ref() {
        if calibration
            .mode
            .is_some_and(|mode| status.calibration.mode != mode)
        {
            return false;
        }
        if calibration
            .pps_enabled
            .is_some_and(|enabled| status.calibration.pps_enabled != enabled)
        {
            return false;
        }
        if calibration
            .pps_mv
            .is_some_and(|pps_mv| status.calibration.pps_mv != Some(pps_mv))
        {
            return false;
        }
        if calibration.heater_enabled.is_some_and(|enabled| {
            status.calibration.heater_enabled != enabled || status.heater_enabled != enabled
        }) {
            return false;
        }
        if calibration
            .target_adc_mv
            .is_some_and(|target_adc_mv| status.calibration.target_adc_mv != Some(target_adc_mv))
        {
            return false;
        }
    }
    true
}

fn decode_usb_response_line(line: &[u8], request_id: &str) -> Result<Option<Value>, HttpError> {
    let Ok(text) = std::str::from_utf8(line) else {
        return Ok(None);
    };
    let Ok(frame) = serde_json::from_str::<UsbResponseWire>(text.trim()) else {
        return Ok(None);
    };
    if frame.frame_type != "response" || frame.request_id.as_deref() != Some(request_id) {
        return Ok(None);
    }
    if frame.ok == Some(true) {
        return Ok(Some(frame.result.unwrap_or(Value::Null)));
    }

    Err(HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: frame.error.unwrap_or_else(|| ApiError {
            code: "usb_error".to_string(),
            message: "Firmware returned an unsuccessful USB response.".to_string(),
            retryable: true,
            details: None,
        }),
    })
}

fn serial_io_http_error(error: io::Error) -> HttpError {
    HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        &format!("Serial I/O failed: {error}"),
        true,
    )
}

fn is_recoverable_write_http_error(error: &HttpError) -> bool {
    error.error.code == "serial_io_failed"
        && error.error.retryable
        && error
            .error
            .message
            .strip_prefix("Serial I/O failed: ")
            .map(is_recoverable_serial_error_message)
            .unwrap_or(false)
}

fn is_recoverable_serial_error_message(message: &str) -> bool {
    message.contains("Broken pipe")
        || message.contains("broken pipe")
        || message.contains("No such file or directory")
        || message.contains("Connection reset")
        || message.contains("Connection aborted")
        || message.contains("UnexpectedEof")
        || message.contains("Device not configured")
        || message.contains("device not configured")
}

async fn flash_device(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FlashRequest>,
) -> Result<Json<FlashResult>, HttpError> {
    let artifact_id = payload.artifact.artifact_id.clone();
    let dry_run_approval = flash_dry_run_approval(&payload)?;
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
            .insert(device_id.clone(), dry_run_approval.clone());
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
        if prior != Some(&dry_run_approval) {
            drop(state_lock);
            state.emit(event(
                &device_id,
                "flash",
                "real flash blocked",
                json!({ "artifactId": artifact_id, "code": "dry_run_required" }),
            ));
            return Err(HttpError::forbidden(
                "dry_run_required",
                "Real flash requires a successful dry-run for the same lease and artifact manifest.",
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
    if let Err(error) = run_espflash_with_exclusive_serial(
        &state,
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
    let port_name = serial_port.to_string_lossy().into_owned();
    let available_ports = serialport::available_ports().ok().unwrap_or_default();
    if !serial_port.exists() {
        return vec![missing_serial_device_record(&port_name, &available_ports)];
    }

    let port_info = available_ports
        .iter()
        .find(|port| port.port_name == port_name);
    vec![serial_device_record(&port_name, port_info)]
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
    DeviceRecord::native_serial_placeholder(&id, display_name, port_name.to_string())
}

fn missing_serial_device_record(
    port_name: &str,
    available_ports: &[serialport::SerialPortInfo],
) -> DeviceRecord {
    let mut device = serial_device_record(port_name, None);
    let candidates = available_ports
        .iter()
        .filter(|port| {
            matches!(
                &port.port_type,
                serialport::SerialPortType::UsbPort(info) if info.vid == 0x303a
            )
        })
        .map(|port| port.port_name.clone())
        .collect::<Vec<_>>();
    let candidate_summary = if candidates.is_empty() {
        "No alternate Espressif serial port is currently enumerated.".to_string()
    } else {
        format!(
            "Observed alternate Espressif serial ports: {}.",
            candidates.join(", ")
        )
    };
    device.connection = ConnectionState::Error;
    device.network.state = NetworkState::Error;
    device.network.last_error = Some(format!(
        "Authorized serial port {port_name} is missing. {candidate_summary}"
    ));
    device.status.network = device.network.clone();
    device.events.push_back(event(
        &device.id,
        "serial",
        "authorized serial port missing",
        json!({
            "code": "authorized_port_missing",
            "portPath": port_name,
            "candidates": candidates,
        }),
    ));
    device
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
            "local-esp32s3-release-root-target",
            "Local ESP32-S3 release (root target)",
            "target/xtensa-esp32s3-none-elf/release/flux-purr",
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

async fn run_espflash_with_exclusive_serial(
    state: &AppState,
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<(), HttpError> {
    let _serial_rpc = state.serial_rpc.lock().await;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    run_espflash(artifact, root, port_path).await
}

fn drop_cached_serial_session(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<(), HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    serial_sessions.remove(port_path);
    Ok(())
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
            "-S".to_string(),
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
        "-S".to_string(),
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
        let preserve_missing_port_diagnostic = error.error.code == "serial_open_failed"
            && device
                .network
                .last_error
                .as_deref()
                .is_some_and(|message| message.starts_with("Authorized serial port "));
        if !preserve_missing_port_diagnostic {
            device.network.last_error = Some(error.error.message.clone());
        }
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
                "manualPpsEnabled": payload.manual_pps_enabled,
                "manualPpsMv": payload.manual_pps_mv,
                "manualPpsMa": payload.manual_pps_ma,
            },
            "status": {
                "targetTempC": status.target_temp_c,
                "selectedPresetSlot": status.selected_preset_slot,
                "presetsC": status.presets_c,
                "activeCoolingEnabled": status.active_cooling_enabled,
                "heaterEnabled": status.heater_enabled,
                "manualPpsEnabled": status.manual_pps_enabled,
                "manualPpsMv": status.manual_pps_mv,
                "manualPpsMa": status.manual_pps_ma,
            },
        }),
    ));
}

fn emit_calibration_event(
    state: &AppState,
    device_id: &str,
    op: &CalibrationConfigOp,
    calibration: &CalibrationState,
) {
    state.emit(event(
        device_id,
        "calibration",
        "calibration draft updated",
        json!({
            "op": op,
            "draftFit": calibration.draft_fit,
            "draftSamples": {
                "rtdAdc": calibration.draft.rtd_adc.iter().flatten().count(),
                "vinAdc": calibration.draft.vin_adc.iter().flatten().count(),
            },
        }),
    ));
}

fn emit_calibration_apply_event(state: &AppState, device_id: &str, calibration: &CalibrationState) {
    state.emit(event(
        device_id,
        "calibration",
        "calibration applied",
        json!({
            "activeFit": calibration.active_fit,
            "activeSamples": {
                "rtdAdc": calibration.active.rtd_adc.iter().flatten().count(),
                "vinAdc": calibration.active.vin_adc.iter().flatten().count(),
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
    redact_sensitive_fields(&mut frame);
    frame
}

fn redact_sensitive_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, field) in object.iter_mut() {
                if is_sensitive_field_key(key) {
                    *field = Value::String("<redacted>".to_string());
                } else {
                    redact_sensitive_fields(field);
                }
            }
        }
        Value::Array(values) => {
            for field in values {
                redact_sensitive_fields(field);
            }
        }
        _ => {}
    }
}

fn is_sensitive_field_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("password") || key.eq_ignore_ascii_case("psk")
}

fn flash_dry_run_approval(payload: &FlashRequest) -> Result<FlashDryRunApproval, HttpError> {
    Ok(FlashDryRunApproval {
        lease_id: payload.lease_id.clone(),
        artifact_fingerprint: artifact_fingerprint(&payload.artifact)?,
    })
}

fn artifact_fingerprint(artifact: &FirmwareArtifact) -> Result<String, HttpError> {
    let bytes = serde_json::to_vec(artifact)
        .map_err(|_| HttpError::internal("Failed to fingerprint firmware artifact."))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
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
        for index in 0..(DEVICE_EVENT_REPLAY_LIMIT + 7) {
            state.emit(event(
                "mock-fp-lab-01",
                "lease",
                "lease created",
                json!({ "leaseId": format!("lease-{index}") }),
            ));
        }
        state.emit(event(
            "other-device",
            "lease",
            "lease created",
            json!({ "leaseId": "lease-other" }),
        ));

        let backlog = device_event_backlog(&state, "mock-fp-lab-01").unwrap();

        assert_eq!(backlog.len(), DEVICE_EVENT_REPLAY_LIMIT);
        assert_eq!(backlog[0].kind, "lease");
        assert_eq!(backlog[0].payload["leaseId"], "lease-7");
        assert_eq!(
            backlog[DEVICE_EVENT_REPLAY_LIMIT - 1].payload["leaseId"],
            format!("lease-{}", DEVICE_EVENT_REPLAY_LIMIT + 6)
        );
    }

    #[tokio::test]
    async fn list_devices_trims_inline_event_backlog_for_polling_clients() {
        let state = AppState::test();
        {
            let mut inner = state.lock().unwrap();
            let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
            for index in 0..(DEVICE_LIST_EVENT_LIMIT + 7) {
                push_bounded(
                    &mut device.events,
                    event(
                        "mock-fp-lab-01",
                        "transport",
                        "transport frame",
                        json!({
                            "direction": "rx",
                            "transport": "usb_jsonl",
                            "frameType": "response",
                            "requestId": format!("req-{index}"),
                            "frame": {
                                "type": "response",
                                "requestId": format!("req-{index}"),
                                "ok": true,
                                "result": {
                                    "calibration": {
                                        "active": {
                                            "vinAdc": [
                                                {
                                                    "expectedMv": 417,
                                                    "observedMv": 279
                                                }
                                            ]
                                        }
                                    }
                                }
                            },
                        }),
                    ),
                    DEFAULT_EVENT_LIMIT,
                );
            }
        }

        let response = list_devices(State(state)).await.unwrap().0;
        let devices = response["devices"].as_array().unwrap();
        let device = devices
            .iter()
            .find(|device| device["id"] == "mock-fp-lab-01")
            .unwrap();
        let events = device["events"].as_array().unwrap();

        assert_eq!(events.len(), DEVICE_LIST_EVENT_LIMIT);
        assert!(device.get("calibration").is_none());
        assert!(device.get("heaterCurve").is_none());
        assert!(device.get("logs").is_none());
        assert!(device.get("trace").is_none());
        assert_eq!(events[0]["payload"]["requestId"], "req-7");
        assert!(events[0]["payload"].get("frame").is_none());
        assert_eq!(
            events[DEVICE_LIST_EVENT_LIMIT - 1]["payload"]["requestId"],
            format!("req-{}", DEVICE_LIST_EVENT_LIMIT + 6)
        );
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
            r#"{"type":"wifi_config","requestId":"req-1","ssid":"FluxPurr-Lab","password":"secret-pass","result":{"wifi":{"psk":"nested-secret"}}}"#,
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
        assert_eq!(
            transport_event.payload["frame"]["result"]["wifi"]["psk"],
            "<redacted>"
        );
        assert!(
            !serde_json::to_string(&transport_event.payload)
                .unwrap()
                .contains("secret-pass")
        );
        assert!(
            !serde_json::to_string(&transport_event.payload)
                .unwrap()
                .contains("nested-secret")
        );
    }

    #[test]
    fn serial_scan_ignores_missing_authorized_port() {
        let dir = tempdir().unwrap();
        let missing_port = dir.path().join("missing-usbmodem");

        let devices = scan_serial_devices(Some(&missing_port));

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].connection, ConnectionState::Error);
        assert_eq!(devices[0].network.state, NetworkState::Error);
        assert!(
            devices[0]
                .network
                .last_error
                .as_deref()
                .is_some_and(|message| {
                    message.starts_with(&format!(
                        "Authorized serial port {} is missing.",
                        missing_port.display()
                    ))
                })
        );
        assert_eq!(devices[0].events.len(), 1);
        assert_eq!(
            devices[0].events[0].message,
            "authorized serial port missing"
        );
        assert_eq!(
            devices[0].events[0].payload["code"],
            "authorized_port_missing"
        );
    }

    #[test]
    fn native_serial_devices_advertise_devd_flash_capabilities() {
        let device = serial_device_record("/dev/cu.usbmodem-test", None);

        assert_eq!(device.transport, DeviceTransport::NativeSerial);
        assert_eq!(device.connection, ConnectionState::Disconnected);
        assert_eq!(device.identity.build_id, "native-serial-placeholder");
        assert_eq!(device.identity.board, "unknown");
        assert_eq!(device.status.current_temp_c, 0.0);
        assert!(!device.status.heater_enabled);
        assert_eq!(device.status.pd_contract_mv, 0);
        assert_eq!(device.network.state, NetworkState::Idle);
        assert_eq!(device.network.ssid, None);
        assert!(
            device
                .identity
                .capabilities
                .contains(&"firmware_check".to_string())
        );
        assert!(device.identity.capabilities.contains(&"flash".to_string()));
    }

    #[test]
    fn native_serial_placeholder_does_not_reuse_mock_hot_state() {
        let device = serial_device_record("/dev/cu.usbmodem-test", None);

        assert_ne!(device.identity.build_id, "devd-mock");
        assert_ne!(device.status.current_temp_c, 183.6);
        assert_ne!(device.network.ssid.as_deref(), Some("FluxPurr-Lab"));
        assert_eq!(device.status.mode, "idle");
        assert_eq!(device.status.network.state, NetworkState::Idle);
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
    fn serial_monitor_log_line_records_serial_event_without_overwriting_errors() {
        let state = AppState::test();
        let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
        serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
        {
            let mut inner = state.lock().unwrap();
            inner
                .devices
                .insert(serial_device.id.clone(), serial_device);
        }

        emit_serial_log_line(
            &state.inner,
            &state.events,
            "serial-known",
            b"INFO heater runtime disabled by safety gate",
        );

        let inner = state.lock().unwrap();
        let device = inner.devices.get("serial-known").unwrap();
        assert_eq!(device.events.len(), 1);
        assert_eq!(device.events[0].kind, "serial");
        assert_eq!(device.events[0].message, "native serial monitor line");
        assert_eq!(device.events[0].payload["code"], "firmware_log");
        assert_eq!(
            device.events[0].payload["line"],
            "INFO heater runtime disabled by safety gate"
        );
    }

    #[test]
    fn serial_open_failed_preserves_missing_authorized_port_diagnostic() {
        let state = AppState::test();
        let device = missing_serial_device_record("/dev/cu.usbmodem-test", &[]);
        {
            let mut inner = state.lock().unwrap();
            inner.devices.insert(device.id.clone(), device);
        }

        let error = HttpError::new(
            StatusCode::BAD_GATEWAY,
            "serial_open_failed",
            "Failed to open serial port: No such file or directory",
            true,
        );

        record_serial_bridge_error(&state, "serial-_dev_cu.usbmodem-test", "identity", &error);

        let inner = state.lock().unwrap();
        let device = inner.devices.get("serial-_dev_cu.usbmodem-test").unwrap();
        assert_eq!(device.connection, ConnectionState::Error);
        assert_eq!(device.network.state, NetworkState::Error);
        assert!(device.network.last_error.as_deref().is_some_and(|message| {
            message.starts_with("Authorized serial port /dev/cu.usbmodem-test is missing.")
        }));
        assert_eq!(
            device.events.back().unwrap().message,
            "native serial RPC failed"
        );
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
    fn artifact_catalog_discovers_root_target_build_outputs() {
        let dir = tempdir().unwrap();
        let artifact_path = dir.path().join("target/xtensa-esp32s3-none-elf/release");
        fs::create_dir_all(&artifact_path).unwrap();
        fs::write(artifact_path.join("flux-purr"), b"firmware-image-root").unwrap();

        let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.artifact_id == "local-esp32s3-release-root-target")
        );
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
        assert!(args.contains(&"-S".to_string()));
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
        assert!(args.contains(&DEFAULT_APP_FLASH_ADDRESS.to_string()));
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
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
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
                lease_id: lease.lease_id.clone(),
                target_temp_c: Some(231),
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: Some(false),
                heater_enabled: Some(false),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
            }),
        )
        .await
        .unwrap();

        {
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

        let invalid_manual = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: Some(199),
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(true),
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(invalid_manual.error.code, "invalid_manual_pps");
        {
            let inner = state.lock().unwrap();
            let device = inner.devices.get("mock-fp-lab-01").unwrap();
            assert_eq!(device.status.target_temp_c, 231);
            assert!(!device.status.manual_pps_enabled);
        }

        let manual_status = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(true),
                manual_pps_mv: Some(10_400),
                manual_pps_ma: Some(2_500),
                calibration: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(manual_status.manual_pps_enabled);
        assert_eq!(manual_status.manual_pps_mv, Some(10_400));
        assert_eq!(manual_status.manual_pps_ma, Some(2_500));
        assert_eq!(manual_status.pd_contract_mv, 10_400);

        let cleared_status = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id,
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(false),
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(!cleared_status.manual_pps_enabled);
        assert_eq!(cleared_status.manual_pps_mv, None);
        assert_eq!(cleared_status.manual_pps_ma, None);
        assert_eq!(cleared_status.pd_contract_mv, DEFAULT_PD_REQUEST_MV);
        assert_eq!(cleared_status.voltage_mv, u32::from(DEFAULT_PD_REQUEST_MV));
    }

    #[tokio::test]
    async fn calibration_runtime_uses_readback_current_and_ignores_stale_calibration_current() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        {
            let mut inner = state.lock().unwrap();
            let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
            device.status.current_ma = 1_350;
            device.status.manual_pps_ma = None;
            device.status.pps_capability_max_ma = Some(3_000);
            device.status.calibration.pps_ma = Some(2_500);
        }

        let status = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id,
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: Some(CalibrationControlRequest {
                    mode: Some(CalibrationMode::VinAdc),
                    pps_enabled: Some(true),
                    pps_mv: Some(12_000),
                    heater_enabled: Some(false),
                    target_adc_mv: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(status.calibration.pps_enabled);
        assert_eq!(status.calibration.pps_mv, Some(12_000));
        assert_eq!(status.calibration.pps_ma, Some(1_350));
        assert_eq!(status.manual_pps_ma, Some(1_350));
    }

    #[tokio::test]
    async fn calibration_runtime_falls_back_to_capability_current_when_readback_is_missing() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        {
            let mut inner = state.lock().unwrap();
            let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
            device.status.current_ma = 0;
            device.status.manual_pps_ma = None;
            device.status.pps_capability_max_ma = Some(3_000);
            device.status.calibration.pps_ma = Some(2_500);
        }

        let status = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id,
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: Some(CalibrationControlRequest {
                    mode: Some(CalibrationMode::VinAdc),
                    pps_enabled: Some(true),
                    pps_mv: Some(12_000),
                    heater_enabled: Some(false),
                    target_adc_mv: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;

        assert!(status.calibration.pps_enabled);
        assert_eq!(status.calibration.pps_ma, Some(3_000));
        assert_eq!(status.manual_pps_ma, Some(3_000));
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
        let changed_without_dry_run = flash_device(
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
        assert_eq!(changed_without_dry_run.status, StatusCode::FORBIDDEN);
        assert_eq!(changed_without_dry_run.error.code, "dry_run_required");

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
    fn runtime_config_matcher_accepts_matching_calibration_status() {
        let payload = RuntimeConfigRequest {
            lease_id: "lease-1".to_string(),
            target_temp_c: Some(45),
            selected_preset_slot: Some(2),
            presets_c: Some(vec![
                Some(50),
                Some(100),
                Some(150),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ]),
            active_cooling_enabled: Some(false),
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: Some(CalibrationControlRequest {
                mode: Some(CalibrationMode::RtdAdc),
                pps_enabled: Some(true),
                pps_mv: Some(12_000),
                heater_enabled: Some(true),
                target_adc_mv: Some(930),
            }),
        };
        let status = ControlPlaneStatus {
            mode: "sampling".to_string(),
            uptime_seconds: 12,
            current_temp_c: 31.5,
            target_temp_c: 45,
            selected_preset_slot: Some(2),
            presets_c: payload.presets_c.clone(),
            heater_enabled: true,
            heater_output_percent: 12,
            active_cooling_enabled: false,
            fan_display_state: "AUTO".to_string(),
            fan_enabled: true,
            fan_pwm_permille: 500,
            voltage_mv: 12_000,
            current_ma: 2_800,
            board_temp_centi: 3150,
            rtd_raw_adc_mv: Some(934),
            vin_raw_adc_mv: Some(1003),
            pd_request_mv: 12_000,
            pd_contract_mv: 12_000,
            pd_state: "ready".to_string(),
            manual_pps_enabled: true,
            manual_pps_mv: Some(12_000),
            manual_pps_ma: Some(3_000),
            pps_capability_min_mv: Some(5_000),
            pps_capability_max_mv: Some(21_000),
            pps_capability_max_ma: Some(3_000),
            manual_pps_error: None,
            calibration: CalibrationRuntimeState {
                mode: CalibrationMode::RtdAdc,
                pps_enabled: true,
                pps_mv: Some(12_000),
                pps_ma: Some(3_000),
                heater_enabled: true,
                target_adc_mv: Some(930),
                stable: true,
                stability_error_mv: Some(4),
                error: None,
                job: CalibrationJobState::default(),
            },
            frontpanel_key: None,
            network: NetworkSummary {
                state: NetworkState::Idle,
                ssid: None,
                ip: None,
                gateway: None,
                dns: Vec::new(),
                wifi_rssi: None,
                last_error: None,
            },
        };

        assert!(runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn runtime_config_matcher_rejects_mismatched_calibration_status() {
        let payload = RuntimeConfigRequest {
            lease_id: "lease-1".to_string(),
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: Some(CalibrationControlRequest {
                mode: Some(CalibrationMode::VinAdc),
                pps_enabled: Some(true),
                pps_mv: Some(16_000),
                heater_enabled: Some(false),
                target_adc_mv: None,
            }),
        };
        let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        status.calibration.mode = CalibrationMode::VinAdc;
        status.calibration.pps_enabled = true;
        status.calibration.pps_mv = Some(12_000);
        status.calibration.heater_enabled = false;

        assert!(!runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn silent_serial_retry_only_applies_to_read_only_policy_window() {
        let now = Instant::now();
        let retry_at = now - Duration::from_millis(1);
        let deadline = now + Duration::from_millis(100);

        assert!(should_retry_silent_serial_request(
            SerialRetryPolicy::ReadOnly,
            now,
            retry_at,
            deadline
        ));
        assert!(!should_retry_silent_serial_request(
            SerialRetryPolicy::SingleShot,
            now,
            retry_at,
            deadline
        ));
        assert!(!should_retry_silent_serial_request(
            SerialRetryPolicy::ReadOnly,
            now,
            now + Duration::from_millis(1),
            deadline
        ));
        assert!(!should_retry_silent_serial_request(
            SerialRetryPolicy::ReadOnly,
            now,
            retry_at,
            now
        ));
    }

    #[test]
    fn write_stage_recoverable_serial_http_errors_are_detected() {
        let broken_pipe = HttpError::new(
            StatusCode::BAD_GATEWAY,
            "serial_io_failed",
            "Serial I/O failed: Broken pipe",
            true,
        );
        assert!(is_recoverable_write_http_error(&broken_pipe));

        let disappeared_port = HttpError::new(
            StatusCode::BAD_GATEWAY,
            "serial_io_failed",
            "Serial I/O failed: No such file or directory",
            true,
        );
        assert!(is_recoverable_write_http_error(&disappeared_port));

        let permanent = HttpError::new(
            StatusCode::BAD_GATEWAY,
            "serial_io_failed",
            "Serial I/O failed: Permission denied",
            true,
        );
        assert!(!is_recoverable_write_http_error(&permanent));

        let other_code = HttpError::new(
            StatusCode::BAD_GATEWAY,
            "usb_payload_decode_failed",
            "USB response payload could not be decoded.",
            true,
        );
        assert!(!is_recoverable_write_http_error(&other_code));
    }

    #[cfg(unix)]
    #[test]
    fn serial_lock_is_not_reentrant_until_previous_session_is_dropped() {
        let port_path = "/tmp/flux-purr-devd-test-port";
        let deadline = Instant::now() + Duration::from_millis(50);

        let first = SerialPortProcessLock::acquire(port_path, deadline).unwrap();

        let second = match SerialPortProcessLock::acquire(
            port_path,
            Instant::now() + Duration::from_millis(50),
        ) {
            Ok(_) => panic!("second serial lock should time out while first session is alive"),
            Err(error) => error,
        };
        assert_eq!(second.error.code, "serial_lock_timeout");

        drop(first);

        let reopened =
            SerialPortProcessLock::acquire(port_path, Instant::now() + Duration::from_millis(50));
        assert!(reopened.is_ok());
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
}
