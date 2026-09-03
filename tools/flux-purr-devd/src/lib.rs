#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(target_os = "macos")]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    fs::{self, File},
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    process::Output,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{process::Command, sync::broadcast};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

pub mod firmware_bundle;
pub mod lan;

pub const PRODUCT_VERSION: &str = env!("FLUX_PURR_PRODUCT_VERSION");
pub const PRODUCT_CHANNEL: &str = env!("FLUX_PURR_PRODUCT_CHANNEL");
pub const PRODUCT_SOURCE_SHA: &str = env!("FLUX_PURR_PRODUCT_SOURCE_SHA");
pub const PRODUCT_BUILD_ID: &str = env!("FLUX_PURR_PRODUCT_BUILD_ID");

pub const DEFAULT_EVENT_LIMIT: usize = 1_000;
pub const DEFAULT_LOG_LIMIT: usize = 2_000;
pub const DEFAULT_TRACE_LIMIT: usize = 2_000;
pub const DEVICE_LIST_EVENT_LIMIT: usize = 24;
pub const DEVICE_EVENT_REPLAY_LIMIT: usize = 120;
pub const DEFAULT_LEASE_TTL_MS: u64 = 30_000;
pub const DEFAULT_BAUD_RATE: u32 = 115_200;
pub const DEFAULT_DEVD_URL: &str = "http://127.0.0.1:30080";
const DEFAULT_PD_REQUEST_MV: u16 = 20_000;
const PPS_HARDWARE_MIN_MV: u16 = 5_000;
const PPS_HARDWARE_MAX_MV: u16 = 28_000;
const AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN: u16 = PPS_HARDWARE_MIN_MV;
const AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT: u16 = 5_000;
const HEATER_PID_TARGET_MIN_C: i16 = 0;
const HEATER_PID_TARGET_MAX_C: i16 = 400;
const THERMAL_PROFILE_ANCHOR_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 250];
const THERMAL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX: u16 = 4_000;
const THERMAL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX: u16 = 375;
const THERMAL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX: u16 = 1_000;
const ADC_CALIBRATION_MAX_SAMPLES: usize = 8;
const HEATER_CURVE_MAX_POINTS: usize = 8;
const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
const USER_CONFIG_FILE: &str = "config.json";
const HARDWARE_REGISTRY_FILE: &str = "devices.json";
const DEFAULT_APP_FLASH_ADDRESS: u64 = 0x10000;
const DEFAULT_PARTITION_TABLE_FLASH_ADDRESS: u64 = 0x8000;
const PARTITION_TABLE_FLASH_SIZE: u64 = 0x1000;
const FLASH_CONFIG_MIN_SIZE: u64 = 0x2000;
const LEGACY_FLASH_CONFIG_OFFSET: u64 = 0x110000;
const LEGACY_FLASH_CONFIG_SIZE: u64 = 0x2000;
const FLASH_CONFIG_LABEL: &str = "flux_cfg";
const ESPFLASH_COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
const ESPFLASH_USB_RESET_RETRY_DELAY: Duration = Duration::from_secs(1);
const FRONT_PANEL_PRESET_COUNT: usize = 10;
const SERIAL_RPC_TIMEOUT: Duration = Duration::from_millis(12_000);
const LEASE_REAPER_INTERVAL: Duration = Duration::from_secs(1);
// Opening an ESP32-S3 USB Serial/JTAG port can reset the device. Read-only
// requests are idempotent and must remain alive through USB enumeration,
// front-panel startup, and PD bring-up so the first native CLI query is usable.
// Opening USB Serial/JTAG can reset the MCU. Allow a full cold boot plus
// hardware discovery before declaring a read-only request unavailable.
const SERIAL_READ_ONLY_RPC_TIMEOUT: Duration = Duration::from_secs(30);
const POST_FLASH_BOOT_TIMEOUT: Duration = Duration::from_secs(90);
const RUNTIME_READY_BOOT_STAGE: &str = "boot_stage=runtime_ready";
const SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(50);
const SERIAL_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const SERIAL_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);
const SERIAL_LINE_LIMIT: usize = 8 * 1024;
// `serialport` configures termios and flushes both queues on macOS. USB
// Serial/JTAG can interpret that control traffic as a host reset, so the
// ESP32-S3 path uses an unconfigured raw descriptor instead.
#[cfg(target_os = "macos")]
const MACOS_O_NONBLOCK: i32 = 0x0004;
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lan_devices: Vec<lan::LanDeviceConfig>,
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
    fs::write(&path, serde_json::to_vec_pretty(config)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:30080".parse().unwrap(),
            artifact_root: None,
            allow_dev_cors: true,
            allow_real_flash: false,
            serial_port: None,
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
    bundle_store: Arc<tempfile::TempDir>,
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
            bundle_store: Arc::new(
                tempfile::Builder::new()
                    .prefix("flux-purr-bundles-")
                    .tempdir()
                    .expect("create private firmware bundle store"),
            ),
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

    pub async fn run_lease_reaper(self) {
        let mut interval = tokio::time::interval(LEASE_REAPER_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let _ = self.reap_expired_leases().await;
        }
    }

    async fn reap_expired_leases(&self) -> Result<usize, HttpError> {
        let _serial_rpc =
            acquire_serial_rpc_with_timeout(self.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
        let expired = {
            let mut state = self.lock()?;
            let expired = state.cleanup_leases();
            let active_device_ids = state
                .leases
                .values()
                .map(|lease| lease.device_id.as_str())
                .collect::<HashSet<_>>();
            let mut sessions = lock_serial_sessions(&self.serial_sessions)?;
            for lease in &expired {
                if active_device_ids.contains(lease.device_id.as_str()) {
                    continue;
                }
                if let Some(port_path) = state
                    .devices
                    .get(&lease.device_id)
                    .and_then(|device| device.port_path.as_deref())
                {
                    sessions.remove(port_path);
                }
            }
            expired
        };
        for lease in &expired {
            self.emit(event(
                &lease.device_id,
                "lease",
                "lease expired",
                json!({ "leaseId": lease.lease_id }),
            ));
        }
        Ok(expired.len())
    }
}

#[derive(Debug, Default)]
struct DevdState {
    devices: HashMap<String, DeviceRecord>,
    leases: HashMap<String, WebLease>,
    dry_run_passes: HashMap<String, FlashDryRunApproval>,
    firmware_approvals: HashMap<String, FirmwareApproval>,
    sequence: u64,
}

#[derive(Debug, Clone)]
struct FirmwareApproval {
    lease_id: String,
    device_id: String,
    port_path: String,
    rom_mac: String,
    bundle_sha256: String,
    operation: FirmwareOperation,
    allow_downgrade: bool,
    preflight_digest: String,
    expires_at: Instant,
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

    fn cleanup_leases(&mut self) -> Vec<WebLease> {
        let now = Instant::now();
        let expired_ids = self
            .leases
            .iter()
            .filter(|(_, lease)| lease.expires_at <= now)
            .map(|(lease_id, _)| lease_id.clone())
            .collect::<Vec<_>>();
        expired_ids
            .into_iter()
            .filter_map(|lease_id| self.leases.remove(&lease_id))
            .collect()
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
    #[serde(default, skip_serializing, skip_deserializing)]
    mock_pps_apdos: Vec<MockPpsApdo>,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub preview_thermal_control_profile: Option<ThermalControlProfilePackage>,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub saved_thermal_control_profile: Option<ThermalControlProfilePackage>,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub saved_thermal_control_profile_pps5a: Option<ThermalControlProfilePackage>,
    pub calibration: CalibrationState,
    pub heater_curve: HeaterCurveState,
    #[serde(default)]
    pub thermal_plant_run: ThermalPlantRunSnapshot,
    pub selected_artifact_id: Option<String>,
    pub logs: VecDeque<LogEntry>,
    pub trace: VecDeque<TraceEntry>,
    pub events: VecDeque<DevdEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MockPpsApdo {
    min_mv: u16,
    max_mv: u16,
    max_ma: u16,
}

fn mock_thermal_plant_snapshot() -> ThermalPlantRunSnapshot {
    let curve_points = [
        (25, 5_674),
        (61, 6_089),
        (102, 6_583),
        (162, 7_307),
        (220, 8_011),
    ]
    .into_iter()
    .map(|(temp_c, resistance_ohms)| {
        Some(HeaterCurvePoint {
            temp_centi_c: temp_c * 100,
            resistance_milliohms: resistance_ohms,
        })
    })
    .chain(std::iter::repeat(None))
    .take(HEATER_CURVE_MAX_POINTS)
    .collect::<Vec<_>>();
    let curve = HeaterCurvePackage {
        points: curve_points,
        raw_observations: None,
    };
    let temperatures = [
        25, 35, 52, 78, 112, 148, 182, 207, 220, 205, 174, 138, 102, 80,
    ];
    let points = temperatures
        .into_iter()
        .enumerate()
        .map(|(index, temperature)| ThermalPlantTracePoint {
            sample_index: index as u8,
            elapsed_ms: index as u32 * 30_000,
            temperature_centi_c: temperature * 100,
            heater_voltage_mv: if index < 9 { 21_000 } else { 0 },
            duty_percent: if index < 9 { 100 } else { 0 },
            phase: if index == 0 {
                ThermalPlantRunPhase::Ambient
            } else if index < 9 {
                ThermalPlantRunPhase::Heating
            } else {
                ThermalPlantRunPhase::Cooling
            },
        })
        .collect();
    ThermalPlantRunSnapshot {
        version: 1,
        attempt: Some(ThermalPlantRunAttempt {
            run_id: 7,
            status: CalibrationJobStatus::Completed,
            phase: Some(ThermalPlantRunPhase::Cooling),
            progress_percent: 100,
            elapsed_ms: 420_000,
            current_temp_centi_c: 8000,
            heater_voltage_mv: 0,
            duty_percent: 0,
            sample_count: 14,
            restart_allowed: true,
            error: None,
        }),
        trace_page: ThermalPlantTracePage {
            start_sample: 0,
            next_sample: None,
            total_samples: 14,
            points,
        },
        provisional_curve: None,
        active_result: Some(ThermalPlantActiveResult {
            transaction_id: 7,
            curve,
            convection_mw_per_c: Some(120.0),
            radiation_mw_per_k4: Some(0.0002),
            thermal_capacity_mj_per_c: Some(42_000.0),
            transport_delay_ms: Some(500),
        }),
    }
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
                "thermal_plant_run".to_string(),
                "wifi_config".to_string(),
                "wifi_state_v2".to_string(),
                "monitor".to_string(),
                "firmware_check".to_string(),
                "flash".to_string(),
            ],
        };
        let network = NetworkSummary {
            state: NetworkState::Connected,
            configuration_generation: 1,
            transition_sequence: 1,
            failure_code: None,
            ssid: Some("FluxPurr-Lab".to_string()),
            wifi_password_length: 11,
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
            heater_physical_output_percent: 22,
            active_cooling_enabled: true,
            fan_display_state: "AUTO".to_string(),
            fan_enabled: true,
            fan_pwm_permille: 500,
            voltage_mv: 20_010,
            current_ma: 840,
            board_temp_centi: 3_840,
            rtd_raw_adc_mv: Some(1_123),
            rtd_raw_adc_min_mv: Some(1_122),
            rtd_raw_adc_max_mv: Some(1_124),
            rtd_raw_adc_spread_mv: Some(2),
            vin_raw_adc_mv: Some(1_678),
            adc_diagnostics: None,
            pd_request_mv: DEFAULT_PD_REQUEST_MV,
            pd_contract_mv: DEFAULT_PD_REQUEST_MV,
            pd_state: "ready".to_string(),
            // The default devd fixture exercises legacy PPS calibration. The
            // console's FUSB302B fixture separately models its bounded PPS
            // path and fixed-PDO fallback.
            pd_controller: Some("ch224q".to_string()),
            pd_contract_kind: Some("pps".to_string()),
            pd_contract_current_ma: Some(3_000),
            pd_contract_power_mw: Some(60_000),
            pd_performance_guaranteed: Some(true),
            pd_degraded_reason: None,
            manual_pps_enabled: false,
            manual_pps_mv: None,
            manual_pps_ma: None,
            pps_capability_min_mv: Some(5_000),
            pps_capability_max_mv: Some(21_000),
            pps_capability_max_ma: Some(3_000),
            manual_pps_error: None,
            heater_fault_reason: None,
            fault_attention_pending: false,
            heater_lock_reason: None,
            heater_control_phase: None,
            heater_error_c: None,
            heater_control_error_c: None,
            heater_control_temp_c: None,
            heater_control_measurement_guarded: false,
            heater_filtered_temp_c: None,
            heater_filtered_slope_c_per_s: None,
            heater_coast_active: false,
            heater_control_interval_ms: 0,
            heater_control_cycle_ms: 0,
            calibration: CalibrationRuntimeState::default(),
            thermal_control_profile_preview: false,
            thermal_profile_mode: "65w".to_string(),
            thermal_profile_resolved_bank: "pps3a".to_string(),
            thermal_control: ThermalControlRuntime::default(),
            thermal_plant_model: ThermalPlantRuntime::default(),
            frontpanel_key: None,
            frontpanel_route: None,
            frontpanel_presented_route: None,
            frontpanel_presentation_count: None,
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
            mock_pps_apdos: vec![MockPpsApdo {
                min_mv: 5_000,
                max_mv: 21_000,
                max_ma: 3_000,
            }],
            preview_thermal_control_profile: None,
            saved_thermal_control_profile: None,
            saved_thermal_control_profile_pps5a: None,
            calibration: CalibrationState::default(),
            heater_curve: HeaterCurveState::default(),
            thermal_plant_run: mock_thermal_plant_snapshot(),
            selected_artifact_id: None,
            logs: VecDeque::new(),
            trace: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    fn native_serial_placeholder(id: &str, display_name: String, port_path: String) -> Self {
        let network = NetworkSummary {
            state: NetworkState::Idle,
            configuration_generation: 0,
            transition_sequence: 0,
            failure_code: None,
            ssid: None,
            wifi_password_length: 0,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: None,
        };
        let status = ControlPlaneStatus {
            mode: "idle".to_string(),
            uptime_seconds: 0,
            current_temp_c: -1.0,
            target_temp_c: 220,
            selected_preset_slot: None,
            presets_c: None,
            heater_enabled: false,
            heater_output_percent: 0,
            heater_physical_output_percent: 0,
            active_cooling_enabled: true,
            fan_display_state: "OFF".to_string(),
            fan_enabled: false,
            fan_pwm_permille: 0,
            voltage_mv: 0,
            current_ma: 0,
            board_temp_centi: -100,
            rtd_raw_adc_mv: None,
            rtd_raw_adc_min_mv: None,
            rtd_raw_adc_max_mv: None,
            rtd_raw_adc_spread_mv: None,
            vin_raw_adc_mv: None,
            adc_diagnostics: None,
            pd_request_mv: DEFAULT_PD_REQUEST_MV,
            pd_contract_mv: 0,
            pd_state: "unknown".to_string(),
            pd_controller: Some("unknown".to_string()),
            pd_contract_kind: Some("none".to_string()),
            pd_contract_current_ma: None,
            pd_contract_power_mw: None,
            pd_performance_guaranteed: Some(false),
            pd_degraded_reason: Some("pd_contract_unavailable".to_string()),
            manual_pps_enabled: false,
            manual_pps_mv: None,
            manual_pps_ma: None,
            pps_capability_min_mv: None,
            pps_capability_max_mv: None,
            pps_capability_max_ma: None,
            manual_pps_error: None,
            heater_fault_reason: None,
            fault_attention_pending: false,
            heater_lock_reason: None,
            heater_control_phase: None,
            heater_error_c: None,
            heater_control_error_c: None,
            heater_control_temp_c: None,
            heater_control_measurement_guarded: false,
            heater_filtered_temp_c: None,
            heater_filtered_slope_c_per_s: None,
            heater_coast_active: false,
            heater_control_interval_ms: 0,
            heater_control_cycle_ms: 0,
            calibration: CalibrationRuntimeState::default(),
            thermal_control_profile_preview: false,
            thermal_profile_mode: "65w".to_string(),
            thermal_profile_resolved_bank: "pps3a".to_string(),
            thermal_control: ThermalControlRuntime::default(),
            thermal_plant_model: ThermalPlantRuntime::default(),
            frontpanel_key: None,
            frontpanel_route: None,
            frontpanel_presented_route: None,
            frontpanel_presentation_count: None,
            network: network.clone(),
        };

        Self {
            id: id.to_string(),
            display_name,
            port_path: Some(port_path),
            transport: DeviceTransport::NativeSerial,
            connection: ConnectionState::Disconnected,
            identity: Identity {
                device_id: String::new(),
                firmware_version: "unknown".to_string(),
                build_id: "native-serial-placeholder".to_string(),
                git_sha: "unknown".to_string(),
                board: "unknown".to_string(),
                api_version: "2026-05-29".to_string(),
                protocol_version: "flux-purr.usb.v1".to_string(),
                hostname: String::new(),
                capabilities: vec![
                    "identity".to_string(),
                    "status".to_string(),
                    "network".to_string(),
                    "thermal_plant_run".to_string(),
                    "wifi_config".to_string(),
                    "monitor".to_string(),
                    "firmware_check".to_string(),
                    "flash".to_string(),
                ],
            },
            network,
            status,
            mock_pps_apdos: Vec::new(),
            preview_thermal_control_profile: None,
            saved_thermal_control_profile: None,
            saved_thermal_control_profile_pps5a: None,
            calibration: CalibrationState::default(),
            heater_curve: HeaterCurveState::default(),
            thermal_plant_run: ThermalPlantRunSnapshot::default(),
            selected_artifact_id: None,
            logs: VecDeque::new(),
            trace: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    fn lan_bridge(
        id: String,
        identity: Identity,
        network: NetworkSummary,
        status: ControlPlaneStatus,
    ) -> Self {
        let mut record = Self::mock(&id, DeviceTransport::Lan);
        record.display_name = if identity.hostname.trim().is_empty() {
            identity.device_id.clone()
        } else {
            identity.hostname.clone()
        };
        record.port_path = None;
        record.connection = ConnectionState::Connected;
        record.identity = identity;
        record.network = network;
        record.status = status;
        record.status.network = record.network.clone();
        record.logs.clear();
        record.trace.clear();
        record.events.clear();
        record
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTransport {
    Mock,
    NativeSerial,
    Lan,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub layout_id: String,
    pub layout_version: u32,
    pub partition_table_sha256: String,
    pub persistence_source: String,
    pub record_state: String,
    pub record_sequence: u32,
    pub commissioning_required: bool,
    pub setup_reason: Option<String>,
    pub sensor_state: String,
    pub heater_locked: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFailureCode {
    DisconnectTimedOut,
    ConfigurationFailed,
    AssociationRejected,
    AssociationTimedOut,
    Ipv4TimedOut,
    StationDisconnected,
    LanStartupFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSummary {
    pub state: NetworkState,
    #[serde(default)]
    pub configuration_generation: u32,
    #[serde(default)]
    pub transition_sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<NetworkFailureCode>,
    pub ssid: Option<String>,
    #[serde(default)]
    pub wifi_password_length: u8,
    pub ip: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub wifi_rssi: Option<i16>,
    pub last_error: Option<String>,
}

impl NetworkSummary {
    fn is_not_older_than(&self, current: &Self) -> bool {
        self.configuration_generation > current.configuration_generation
            || (self.configuration_generation == current.configuration_generation
                && self.transition_sequence >= current.transition_sequence)
            // Both counters falling together is a device reboot. The first
            // receipt after the reboot is current device fact, not an old
            // packet from the previous boot.
            || (self.configuration_generation < current.configuration_generation
                && self.transition_sequence < current.transition_sequence)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbWifiConfigReceipt {
    network: NetworkSummary,
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
    #[serde(default)]
    pub heater_physical_output_percent: u8,
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
    pub rtd_raw_adc_min_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtd_raw_adc_max_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtd_raw_adc_spread_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vin_raw_adc_mv: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adc_diagnostics: Option<AdcDiagnostics>,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_controller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_contract_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_contract_current_ma: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_contract_power_mw: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_performance_guaranteed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_degraded_reason: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_fault_reason: Option<String>,
    #[serde(default)]
    pub fault_attention_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_lock_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_control_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_error_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_control_error_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_control_temp_c: Option<f32>,
    #[serde(default)]
    pub heater_control_measurement_guarded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_filtered_temp_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heater_filtered_slope_c_per_s: Option<f32>,
    #[serde(default)]
    pub heater_coast_active: bool,
    #[serde(default)]
    pub heater_control_interval_ms: u16,
    #[serde(default)]
    pub heater_control_cycle_ms: u16,
    #[serde(default)]
    pub calibration: CalibrationRuntimeState,
    #[serde(default)]
    pub thermal_control_profile_preview: bool,
    #[serde(default = "default_thermal_profile_mode")]
    pub thermal_profile_mode: String,
    #[serde(default = "default_thermal_profile_resolved_bank")]
    pub thermal_profile_resolved_bank: String,
    #[serde(default)]
    pub thermal_control: ThermalControlRuntime,
    #[serde(default)]
    pub thermal_plant_model: ThermalPlantRuntime,
    pub frontpanel_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontpanel_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontpanel_presented_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontpanel_presentation_count: Option<u32>,
    pub network: NetworkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdcDiagnostics {
    pub calibration_source: String,
    pub efuse_version: u8,
    pub attenuation_db: u8,
    pub init_code: Option<u16>,
    pub reference_code: Option<u16>,
    pub reference_mv: Option<u16>,
    pub rtd_raw_code_mean: u16,
    pub rtd_raw_code_min: u16,
    pub rtd_raw_code_max: u16,
    pub rtd_raw_code_spread: u16,
    pub vin_raw_code_mean: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlRuntime {
    pub profile_active: bool,
    pub profile_covers_target: bool,
    pub profile_source: String,
    pub target_temp_c: i16,
    pub brake_distance_centi_c: u16,
    pub warmup_power_permille: u16,
    pub approach_power_permille: u16,
    pub approach_floor_power_permille: u16,
    pub approach_damping_exponent_permille: u16,
    #[serde(default)]
    pub approach_tail_window_centi_c: u16,
    pub hold_power_permille: u16,
    pub hold_reheat_power_permille: u16,
    pub hold_entry_centi_c: u16,
    pub hold_exit_centi_c: u16,
    pub hold_on_centi_c: u16,
    pub hold_off_centi_c: u16,
    pub overshoot_cutoff_centi_c: u16,
    pub hold_kp_permille_per_c: u16,
    pub hold_ki_permille_per_c_tick: u16,
    pub hold_blend_ticks: u16,
    pub approach_lead_ticks: u16,
    pub hold_lead_ticks: u16,
    pub temp_filter_alpha_permille: u16,
    pub warmup_reenter_centi_c: u16,
    pub approach_max_ticks: u16,
    pub approach_min_power_ratio_permille: u16,
    pub auto_adjustable_working_floor_mv: u16,
    pub heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRuntime {
    pub state: String,
    pub active_transaction_id: Option<u32>,
    pub projection_valid: bool,
    pub convection_mw_per_c: Option<f32>,
    pub radiation_mw_per_k4: Option<f32>,
    pub thermal_capacity_mj_per_c: Option<f32>,
    pub transport_delay_ms: Option<u32>,
}

fn default_thermal_profile_mode() -> String {
    "65w".to_string()
}

fn default_thermal_profile_resolved_bank() -> String {
    "pps3a".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MockThermalCandidateSettings {
    temp_filter_alpha_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    approach_max_ticks: u16,
    approach_min_power_ratio_permille: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    hold_reheat_power_permille: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
    auto_adjustable_working_floor_mv: u16,
    heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MockThermalCandidatePoint {
    target_temp_c: i16,
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MockThermalCandidateProfile {
    settings: MockThermalCandidateSettings,
    points: Vec<MockThermalCandidatePoint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationMode {
    #[default]
    Off,
    VinAdc,
    RtdAdc,
    HeaterCurve,
    ThermalPlant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobKind {
    VinAdcAuto,
    ThermalPlantAuto,
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
    pub target_adc_mv: Option<u16>,
    pub reference_vin_mv: Option<u16>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationFit {
    pub gain: f32,
    pub offset_mv: f32,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSlotFit {
    pub gain: f32,
    pub offset_mv: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationSlotId {
    A,
    B,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSlotSet {
    pub a: CalibrationSlotFit,
    pub b: CalibrationSlotFit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationChannelState {
    pub samples: Vec<Option<CalibrationSample>>,
    pub fitted_fit: CalibrationFit,
    pub slots: CalibrationSlotSet,
    pub active_slot: CalibrationSlotId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationState {
    pub rtd_adc: CalibrationChannelState,
    pub vin_adc: CalibrationChannelState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePoint {
    pub temp_centi_c: i16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveRawObservation {
    pub raw_rtd_adc_mv: u16,
    pub heater_voltage_mv: u16,
    pub heater_current_ma: u16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveRawObservations {
    pub points: Vec<Option<HeaterCurveRawObservation>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePackage {
    pub points: Vec<Option<HeaterCurvePoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_observations: Option<HeaterCurveRawObservations>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveState {
    pub active: HeaterCurvePackage,
    pub preview: Option<HeaterCurvePackage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eeprom_probe: Option<HeaterCurveEepromProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThermalPlantRunPhase {
    #[default]
    Ambient,
    Heating,
    Cooling,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantTracePoint {
    pub sample_index: u8,
    pub elapsed_ms: u32,
    pub temperature_centi_c: i16,
    pub heater_voltage_mv: u16,
    pub duty_percent: u8,
    pub phase: ThermalPlantRunPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantTracePage {
    pub start_sample: u8,
    pub next_sample: Option<u8>,
    pub total_samples: u8,
    #[serde(default)]
    pub points: Vec<ThermalPlantTracePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantProvisionalCurve {
    pub state: String,
    pub coverage_percent: u8,
    pub curve: HeaterCurvePackage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRunAttempt {
    pub run_id: u32,
    pub status: CalibrationJobStatus,
    pub phase: Option<ThermalPlantRunPhase>,
    pub progress_percent: u8,
    pub elapsed_ms: u32,
    pub current_temp_centi_c: i16,
    pub heater_voltage_mv: u16,
    pub duty_percent: u8,
    pub sample_count: u8,
    pub restart_allowed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantActiveResult {
    pub transaction_id: u32,
    pub curve: HeaterCurvePackage,
    pub convection_mw_per_c: Option<f32>,
    pub radiation_mw_per_k4: Option<f32>,
    pub thermal_capacity_mj_per_c: Option<f32>,
    pub transport_delay_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRunSnapshot {
    pub version: u8,
    pub attempt: Option<ThermalPlantRunAttempt>,
    pub trace_page: ThermalPlantTracePage,
    pub provisional_curve: Option<ThermalPlantProvisionalCurve>,
    pub active_result: Option<ThermalPlantActiveResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveEepromProbe {
    pub present: bool,
    #[serde(default)]
    pub current_read_present: bool,
    #[serde(default)]
    pub random_read_present: bool,
    #[serde(default)]
    pub bus_current_read_addresses: Vec<Option<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl Default for HeaterCurvePackage {
    fn default() -> Self {
        Self {
            points: vec![None; HEATER_CURVE_MAX_POINTS],
            raw_observations: None,
        }
    }
}

impl Default for CalibrationState {
    fn default() -> Self {
        Self {
            rtd_adc: CalibrationChannelState::default(),
            vin_adc: CalibrationChannelState {
                fitted_fit: fit_calibration_channel(
                    &[None; ADC_CALIBRATION_MAX_SAMPLES],
                    CalibrationChannel::VinAdc,
                ),
                ..CalibrationChannelState::default()
            },
        }
    }
}

impl Default for CalibrationChannelState {
    fn default() -> Self {
        let samples = vec![None; ADC_CALIBRATION_MAX_SAMPLES];
        Self {
            fitted_fit: fit_calibration_channel(&samples, CalibrationChannel::RtdAdc),
            samples,
            slots: CalibrationSlotSet::default(),
            active_slot: CalibrationSlotId::A,
        }
    }
}

impl Default for CalibrationSlotFit {
    fn default() -> Self {
        Self {
            gain: 1.0,
            offset_mv: 0.0,
        }
    }
}

impl CalibrationState {
    fn channel_mut(&mut self, channel: CalibrationChannel) -> &mut CalibrationChannelState {
        match channel {
            CalibrationChannel::RtdAdc => &mut self.rtd_adc,
            CalibrationChannel::VinAdc => &mut self.vin_adc,
        }
    }

    fn refresh_fits(&mut self) {
        self.rtd_adc.refresh(CalibrationChannel::RtdAdc);
        self.vin_adc.refresh(CalibrationChannel::VinAdc);
    }
}

impl CalibrationChannelState {
    fn refresh(&mut self, channel: CalibrationChannel) {
        if channel == CalibrationChannel::RtdAdc {
            sanitize_web_facing_rtd_samples(&mut self.samples);
        } else {
            compact_calibration_samples(&mut self.samples);
        }
        self.fitted_fit = fit_calibration_channel(&self.samples, channel);
    }

    fn sanitize_slot_fits(&mut self) {
        sanitize_calibration_slot_fit(&mut self.slots.a);
        sanitize_calibration_slot_fit(&mut self.slots.b);
    }

    fn slot_fit_mut(&mut self, slot: CalibrationSlotId) -> &mut CalibrationSlotFit {
        match slot {
            CalibrationSlotId::A => &mut self.slots.a,
            CalibrationSlotId::B => &mut self.slots.b,
        }
    }
}

fn sanitize_calibration_slot_fit(fit: &mut CalibrationSlotFit) {
    if !fit.gain.is_finite() || fit.gain == 0.0 {
        fit.gain = 1.0;
    }
    if !fit.offset_mv.is_finite() {
        fit.offset_mv = 0.0;
    }
}

fn fit_calibration_channel(
    samples: &[Option<CalibrationSample>],
    channel: CalibrationChannel,
) -> CalibrationFit {
    let custom: Vec<CalibrationSample> = samples
        .iter()
        .flatten()
        .copied()
        .filter(|sample| is_web_facing_calibration_sample(*sample, channel))
        .collect();
    if custom.is_empty() {
        return CalibrationFit {
            gain: 1.0,
            offset_mv: 0.0,
            sample_count: 0,
        };
    }
    if custom.len() == 1 {
        let sample = custom[0];
        return CalibrationFit {
            gain: 1.0,
            offset_mv: sample.expected_mv as f32 - sample.observed_mv as f32,
            sample_count: 1,
        };
    }
    let points = custom;

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
        sample_count: points.len(),
    }
}

fn is_web_facing_calibration_sample(
    sample: CalibrationSample,
    channel: CalibrationChannel,
) -> bool {
    match channel {
        CalibrationChannel::RtdAdc => {
            sample.reference_temp_c.is_some() && sample.target_adc_mv.is_some()
        }
        CalibrationChannel::VinAdc => true,
    }
}

fn sanitize_web_facing_rtd_samples(samples: &mut Vec<Option<CalibrationSample>>) {
    let mut compacted: Vec<Option<CalibrationSample>> = samples
        .iter()
        .flatten()
        .copied()
        .filter(|sample| is_web_facing_calibration_sample(*sample, CalibrationChannel::RtdAdc))
        .map(Some)
        .collect();
    compacted.resize(ADC_CALIBRATION_MAX_SAMPLES, None);
    *samples = compacted;
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
    /// `None` preserves the stored static address, while `Some(None)` carries
    /// an explicit JSON `null` to return the station to DHCP.
    #[serde(
        default,
        deserialize_with = "deserialize_static_ipv4_patch",
        skip_serializing_if = "Option::is_none"
    )]
    pub static_ipv4: Option<Option<WifiStaticIpv4Request>>,
    pub telemetry_interval_ms: Option<u32>,
}

/// The USB-only host-tool view of a transient, front-panel-scoped LAN pairing
/// code. It is deliberately never persisted in daemon state or events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanPairingCode {
    pub active: bool,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WifiStaticIpv4Request {
    pub address: [u8; 4],
    pub prefix_len: u8,
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

/// Preserve the three API states for `staticIpv4`: a missing field keeps the
/// current mode, JSON `null` switches back to DHCP, and an object sets static
/// IPv4. Nested `Option` derives otherwise merge the first two states.
fn deserialize_static_ipv4_patch<'de, D>(
    deserializer: D,
) -> Result<Option<Option<WifiStaticIpv4Request>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<WifiStaticIpv4Request>::deserialize(deserializer).map(Some)
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
    pub fault_attention_acknowledged: Option<bool>,
    pub calibration: Option<CalibrationControlRequest>,
    pub thermal_profile_mode: Option<String>,
    pub thermal_control_profile: Option<ThermalControlProfileRequest>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerDebugOp {
    Trigger,
    Run,
    Stop,
    Status,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerDebugCue {
    UiInput,
    HeaterOn,
    HeaterOff,
    ActiveCoolingOn,
    ActiveCoolingOff,
    HeaterReject,
    ActiveCoolingReject,
    ProtectionAlarm,
    AttentionReminder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerDebugScenario {
    FeedbackCoalesce,
    FeedbackReplace,
    ActiveCoolingRetrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerDebugRequest {
    pub lease_id: String,
    pub op: BuzzerDebugOp,
    pub cue: Option<BuzzerDebugCue>,
    pub scenario: Option<BuzzerDebugScenario>,
    #[serde(default)]
    pub repeat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerDebugSessionState {
    Idle,
    Running,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerDebugDecision {
    pub source: String,
    pub cue: String,
    pub disposition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerDebugTraceEvent {
    pub elapsed_ms: u32,
    pub decision: BuzzerDebugDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerDebugOutputTraceEvent {
    pub elapsed_ms: u32,
    pub requested_frequency_hz: Option<u32>,
    pub applied_frequency_hz: u32,
    pub duty_percent: u8,
    pub generation: u32,
    pub timer_prescaler: u8,
    pub timer_period_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerDebugStatus {
    pub state: BuzzerDebugSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<BuzzerDebugScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue: Option<BuzzerDebugCue>,
    #[serde(default)]
    pub repeat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_cue: Option<String>,
    pub trace: Vec<BuzzerDebugTraceEvent>,
    #[serde(default)]
    pub output_trace: Vec<BuzzerDebugOutputTraceEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalControlProfileOp {
    Preview,
    ClearPreview,
    Save,
    ClearSaved,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfilePoint {
    pub target_temp_c: i16,
    pub brake_distance_centi_c: u16,
    #[serde(default)]
    pub warmup_power_permille: u16,
    pub approach_power_permille: u16,
    pub approach_floor_power_permille: u16,
    #[serde(default = "default_approach_damping_exponent_permille")]
    pub approach_damping_exponent_permille: u16,
    #[serde(default)]
    pub approach_tail_window_centi_c: u16,
    pub hold_power_permille: u16,
    #[serde(default)]
    pub hold_reheat_power_permille: u16,
    #[serde(default)]
    pub warmup_reenter_centi_c: u16,
    #[serde(default)]
    pub hold_entry_centi_c: u16,
    #[serde(default)]
    pub hold_exit_centi_c: u16,
    #[serde(default)]
    pub hold_on_centi_c: u16,
    #[serde(default)]
    pub hold_off_centi_c: u16,
    #[serde(default)]
    pub overshoot_cutoff_centi_c: u16,
    #[serde(default)]
    pub hold_kp_permille_per_c: u16,
    #[serde(default)]
    pub hold_ki_permille_per_c_tick: u16,
    #[serde(default)]
    pub hold_blend_ticks: u16,
    #[serde(default)]
    pub approach_lead_ticks: u16,
    #[serde(default)]
    pub hold_lead_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfilePackage {
    pub settings: Option<ThermalControlProfileSettings>,
    pub points: Vec<Option<ThermalControlProfilePoint>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfileSettings {
    pub temp_filter_alpha_permille: u16,
    #[serde(default)]
    pub warmup_reenter_centi_c: u16,
    #[serde(default)]
    pub hold_entry_centi_c: u16,
    #[serde(default)]
    pub hold_exit_centi_c: u16,
    #[serde(default)]
    pub hold_on_centi_c: u16,
    #[serde(default)]
    pub hold_off_centi_c: u16,
    #[serde(default)]
    pub overshoot_cutoff_centi_c: u16,
    pub approach_max_ticks: u16,
    pub approach_min_power_ratio_permille: u16,
    #[serde(default)]
    pub hold_kp_permille_per_c: u16,
    #[serde(default)]
    pub hold_ki_permille_per_c_tick: u16,
    #[serde(default = "default_hold_blend_ticks")]
    pub hold_blend_ticks: u16,
    #[serde(default)]
    pub hold_reheat_power_permille: u16,
    #[serde(default)]
    pub approach_lead_ticks: u16,
    #[serde(default)]
    pub hold_lead_ticks: u16,
    #[serde(default = "default_auto_adjustable_working_floor_mv")]
    pub auto_adjustable_working_floor_mv: u16,
    #[serde(default = "default_heater_current_reserve_ma")]
    pub heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfileRequest {
    pub op: ThermalControlProfileOp,
    #[serde(default)]
    pub bank: Option<String>,
    pub profile: Option<ThermalControlProfilePackage>,
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
    pub target_adc_mv: Option<u16>,
    pub observed_mv: Option<u16>,
    pub expected_mv: Option<u16>,
    pub sample_index: Option<usize>,
    pub state: Option<CalibrationState>,
    pub slot: Option<CalibrationSlotId>,
    pub fit: Option<CalibrationSlotFit>,
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
pub enum EepromMaintenanceOp {
    Read,
    Write,
    Erase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EepromMaintenanceRequest {
    pub lease_id: String,
    pub op: EepromMaintenanceOp,
    pub offset: Option<u16>,
    pub length: Option<u8>,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EepromMaintenanceResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationConfigOp {
    Capture,
    Delete,
    Clear,
    Import,
    SetActiveSlot,
    SetSlotFit,
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
    Cancel,
}

impl WifiConfigOp {
    const fn usb_op(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
            Self::Cancel => "cancel",
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
struct UsbThermalPlantRunWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    after_sample: u8,
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
    static_ipv4: Option<Option<WifiStaticIpv4Request>>,
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
    fault_attention_acknowledged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    calibration: Option<&'a CalibrationControlRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thermal_profile_mode: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thermal_control_profile: Option<&'a ThermalControlProfileRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbBuzzerDebugWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: BuzzerDebugOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    buzzer_cue: Option<BuzzerDebugCue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    buzzer_scenario: Option<BuzzerDebugScenario>,
    repeat: bool,
}

#[cfg(test)]
fn encode_usb_runtime_mode_for_test(mode: &String) -> String {
    serde_json::to_string(&UsbRuntimeConfigWire {
        frame_type: "runtime_config",
        request_id: "mode-test",
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: None,
        calibration: None,
        thermal_profile_mode: Some(mode),
        thermal_control_profile: None,
    })
    .expect("runtime mode wire must serialize")
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
    target_adc_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'a CalibrationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<CalibrationSlotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<&'a CalibrationSlotFit>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbEepromMaintenanceWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a str,
    op: EepromMaintenanceOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<&'a Vec<u8>>,
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

#[derive(Debug, Default, PartialEq, Eq)]
struct BootObservation {
    reset_count: u8,
    saw_boot_progress: bool,
    last_stage: Option<String>,
}

impl BootObservation {
    fn observe_line(&mut self, line: &str) -> Result<bool, HttpError> {
        let line = line.trim();
        if line.starts_with("reset_reason=") {
            self.reset_count = self.reset_count.saturating_add(1);
            if self.reset_count > 1 {
                return Err(HttpError::new(
                    StatusCode::BAD_GATEWAY,
                    "firmware_reboot_loop",
                    "Firmware reset more than once before reaching runtime_ready.",
                    false,
                ));
            }
        }
        if line.starts_with("boot_stage=") {
            if line != RUNTIME_READY_BOOT_STAGE {
                self.saw_boot_progress = true;
                self.last_stage = Some(line.to_string());
            } else if self.saw_boot_progress {
                self.last_stage = Some(line.to_string());
            }
            return Ok(self.saw_boot_progress && line == RUNTIME_READY_BOOT_STAGE);
        }
        let lowercase = line.to_ascii_lowercase();
        if lowercase.contains("guru meditation")
            || lowercase.contains("watchdog")
            || lowercase.contains("panic")
        {
            return Err(HttpError::new(
                StatusCode::BAD_GATEWAY,
                "firmware_boot_failed",
                &format!("Firmware failed during boot: {line}"),
                false,
            ));
        }
        Ok(false)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareBundleCatalog {
    pub bundles: Vec<FirmwareBundleSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareBundleSummary {
    pub artifact_id: String,
    pub source: String,
    pub channel: firmware_bundle::BundleChannel,
    pub version: String,
    pub source_sha: String,
    pub build_id: String,
    pub bundle_sha256: String,
    pub size: u64,
    pub layout_id: String,
    pub operations: Vec<FirmwareOperation>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareOperation {
    Update,
    InstallRecovery,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FirmwareOperationRequest {
    pub lease_id: String,
    pub artifact_id: String,
    pub operation: FirmwareOperation,
    pub dry_run: bool,
    pub approval_token: Option<String>,
    pub confirm: Option<String>,
    #[serde(default)]
    pub allow_downgrade: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareOperationResult {
    pub operation_id: String,
    pub artifact_id: String,
    pub operation: FirmwareOperation,
    pub dry_run: bool,
    pub outcome: String,
    pub approval_token: Option<String>,
    pub approval_expires_in_ms: Option<u64>,
    pub stages: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FirmwareOperationPhase {
    Preflight,
    Execution,
}

struct FirmwareOperationProgress {
    state: AppState,
    device_id: String,
    operation_id: String,
    phase: FirmwareOperationPhase,
    operation: FirmwareOperation,
    artifact_id: String,
    sequence: u64,
    active_stage: Option<String>,
}

impl FirmwareOperationProgress {
    fn new(
        state: &AppState,
        device_id: &str,
        operation: FirmwareOperation,
        artifact_id: &str,
        dry_run: bool,
    ) -> Self {
        Self {
            state: state.clone(),
            device_id: device_id.to_string(),
            operation_id: format!(
                "firmware-operation-{}-{}",
                now_millis(),
                EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            phase: if dry_run {
                FirmwareOperationPhase::Preflight
            } else {
                FirmwareOperationPhase::Execution
            },
            operation,
            artifact_id: artifact_id.to_string(),
            sequence: 0,
            active_stage: None,
        }
    }

    fn operation_id(&self) -> &str {
        &self.operation_id
    }

    fn emit(&mut self, event_name: &str, stage: Option<&str>, details: Value) {
        self.sequence = self.sequence.saturating_add(1);
        let mut payload = json!({
            "schemaVersion": 1,
            "operationId": self.operation_id,
            "phase": self.phase,
            "operation": self.operation,
            "artifactId": self.artifact_id,
            "sequence": self.sequence,
            "event": event_name,
        });
        if let Some(stage) = stage {
            payload["stage"] = Value::String(stage.to_string());
        }
        if let (Some(payload), Some(details)) = (payload.as_object_mut(), details.as_object()) {
            payload.extend(details.clone());
        }
        self.state.emit(event(
            &self.device_id,
            "firmware_operation",
            event_name,
            payload,
        ));
    }

    fn operation_started(&mut self) {
        self.emit("operation_started", None, json!({}));
    }

    fn stage_started(&mut self, stage: &str, details: Value) {
        self.active_stage = Some(stage.to_string());
        self.emit("stage_started", Some(stage), details);
    }

    fn stage_progress(&mut self, stage: &str, details: Value) {
        self.emit("stage_progress", Some(stage), details);
    }

    fn stage_completed(&mut self, stage: &str, details: Value) {
        self.emit("stage_completed", Some(stage), details);
        self.active_stage = None;
    }

    fn stage_failed(&mut self, stage: &str, code: &str) {
        self.emit("stage_failed", Some(stage), json!({ "code": code }));
        self.active_stage = None;
    }

    fn operation_completed(&mut self, outcome: &str) {
        self.emit("operation_completed", None, json!({ "outcome": outcome }));
    }

    fn fail(&mut self, error: HttpError) -> HttpError {
        let outcome = if self.phase == FirmwareOperationPhase::Preflight
            || self.active_stage.as_deref() == Some("authorization")
        {
            "blocked"
        } else {
            "failed"
        };
        if let Some(stage) = self.active_stage.clone() {
            self.stage_failed(&stage, &error.error.code);
        }
        self.operation_completed(outcome);
        error
    }

    fn require<T>(&mut self, result: Result<T, HttpError>) -> Result<T, HttpError> {
        result.map_err(|error| self.fail(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomSecurityInfo {
    pub rom_mac: String,
    pub secure_boot_enabled: bool,
    pub flash_encryption_enabled: bool,
    pub secure_download_mode_enabled: bool,
    pub response_known: bool,
    pub chip_is_esp32s3: bool,
    pub flash_size_bytes: u64,
    pub package_matches: bool,
}

impl RomSecurityInfo {
    fn validate_for_flash(&self) -> Result<(), HttpError> {
        if !self.response_known {
            return Err(HttpError::forbidden(
                "security_info_unknown",
                "The ROM security response is unknown; flashing is blocked.",
            ));
        }
        if self.secure_boot_enabled
            || self.flash_encryption_enabled
            || self.secure_download_mode_enabled
        {
            return Err(HttpError::forbidden(
                "security_features_enabled",
                "Secure Boot, Flash Encryption, or Secure Download Mode blocks this installer.",
            ));
        }
        if !self.chip_is_esp32s3
            || self.flash_size_bytes != 4 * 1024 * 1024
            || !self.package_matches
        {
            return Err(HttpError::forbidden(
                "target_mismatch",
                "The target must be an ESP32-S3 with exactly 4 MiB Flash.",
            ));
        }
        Ok(())
    }
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

    fn internal_with_details(code: &str, message: &str, details: Value) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: ApiError {
                code: code.to_string(),
                message: message.to_string(),
                retryable: false,
                details: Some(details),
            },
        }
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
pub struct ThermalPlantRunQuery {
    pub lease_id: Option<String>,
    pub after_sample: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct BindRequest {
    pub alias: Option<String>,
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
            "/api/v1/devices/{device_id}/buzzer-debug",
            post(configure_buzzer_debug),
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

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let job = lan_bridge_write::<CalibrationJobState>(
            &configured,
            "calibration/job",
            Method::POST,
            Some(lan_bridge_payload(&payload)?),
        )
        .await?;
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
            if device.status.calibration.job.status != CalibrationJobStatus::Running {
                return Ok(Json(device.status.calibration.job.clone()));
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
        CalibrationJobOp::Start => {
            if device.status.calibration.job.status == CalibrationJobStatus::Running {
                return Err(HttpError::bad_request(
                    "heater_disarm_pending",
                    "The previous heater session is still being physically disarmed.",
                ));
            }
            let kind = payload.kind.ok_or_else(|| {
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
                device.status.calibration.mode = CalibrationMode::ThermalPlant;
                disarm_mock_thermal_plant(&mut device.status);
                device.status.manual_pps_enabled = true;
                device.status.manual_pps_mv = Some(request_mv);
                device.status.manual_pps_ma = Some(source.max_ma);
                device.status.pd_request_mv = request_mv;
                device.status.pd_contract_mv = request_mv;
                device.status.voltage_mv = u32::from(request_mv);
                device.status.calibration.pps_enabled = true;
                device.status.calibration.pps_mv = Some(request_mv);
                device.status.calibration.pps_ma = Some(source.max_ma);
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
                    heater_voltage_mv: thermal_request_mv,
                    duty_percent: 0,
                    sample_count: 0,
                    restart_allowed: false,
                    error: None,
                });
                device.thermal_plant_run.trace_page = ThermalPlantTracePage::default();
                device.thermal_plant_run.provisional_curve = None;
            }
        }
    }
    Ok(Json(device.status.calibration.job.clone()))
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
    if let Some(Some(static_ipv4)) = payload.static_ipv4
        && !static_ipv4_request_is_valid(static_ipv4)
    {
        return Err(HttpError::bad_request(
            "invalid_static_ipv4",
            "staticIpv4 requires a unicast address and a prefix length from 0 through 32.",
        ));
    }
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
        let network = match serial_wifi_config(&state, &target, &payload).await {
            Ok(network) => network,
            Err(error) => {
                record_serial_bridge_error(&state, &device_id, "wifi_config", &error);
                return Err(error);
            }
        };
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            if network.is_not_older_than(&device.network) {
                device.network = network.clone();
                device.status.network = network.clone();
            }
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
                "telemetryIntervalMs": payload.telemetry_interval_ms
            }
        })));
    }

    if target.transport == DeviceTransport::Lan {
        return Err(HttpError::bad_request(
            "lan_wifi_config_unsupported",
            "WiFi configuration is available only through an active USB/devd target.",
        ));
    }

    let mut state_lock = state.lock()?;
    let device = state_lock
        .devices
        .get_mut(&device_id)
        .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
    device.network = mock_network_after_wifi_config(&device.network, &payload);
    device.status.network = device.network.clone();
    let redacted = json!({
        "accepted": true,
        "network": device.network,
        "wifi": {
            "op": payload.op,
            "ssid": payload.ssid,
            "password": payload.password.as_ref().map(|_| "<redacted>"),
            "telemetryIntervalMs": payload.telemetry_interval_ms
        }
    });
    drop(state_lock);
    emit_wifi_config_event(&state, &device_id, &payload);
    Ok(Json(redacted))
}

fn mock_network_after_wifi_config(
    current: &NetworkSummary,
    payload: &WifiConfigRequest,
) -> NetworkSummary {
    match payload.op {
        WifiConfigOp::Clear => NetworkSummary {
            state: NetworkState::Disabled,
            configuration_generation: current.configuration_generation.wrapping_add(1),
            transition_sequence: current.transition_sequence.wrapping_add(1),
            failure_code: None,
            ssid: None,
            wifi_password_length: 0,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: None,
        },
        WifiConfigOp::Set => {
            let mut network = current.clone();
            // The device receipt exposes only the public connection phase.
            // Persistence/disconnect work is not a host-visible WiFi state.
            network.state = NetworkState::Connecting;
            network.configuration_generation = network.configuration_generation.wrapping_add(1);
            network.transition_sequence = network.transition_sequence.wrapping_add(1);
            network.failure_code = None;
            network.ssid = payload.ssid.clone();
            if let Some(password) = &payload.password {
                network.wifi_password_length = password.len() as u8;
            }
            network.ip = None;
            network.gateway = None;
            network.dns.clear();
            network.wifi_rssi = None;
            network.last_error = None;
            network
        }
        WifiConfigOp::Cancel => {
            let mut network = current.clone();
            // Cancel is a runtime station operation. Stored credentials and
            // their configuration generation remain unchanged.
            network.state = NetworkState::Idle;
            network.transition_sequence = network.transition_sequence.wrapping_add(1);
            network.failure_code = None;
            network.ip = None;
            network.gateway = None;
            network.dns.clear();
            network.wifi_rssi = None;
            network.last_error = None;
            network
        }
    }
}

fn static_ipv4_request_is_valid(value: WifiStaticIpv4Request) -> bool {
    value.prefix_len <= 32
        && is_unicast_static_ipv4(value.address)
        && is_unicast_static_ipv4(value.gateway)
        && is_unicast_static_ipv4(value.dns)
}

fn is_unicast_static_ipv4(address: [u8; 4]) -> bool {
    let first = address[0];
    first != 0 && first != 127 && first < 224
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

    if target.transport == DeviceTransport::Lan {
        let configured = lan_bridge_config(&target)?;
        let status = lan_bridge_write::<ControlPlaneStatus>(
            &configured,
            "runtime",
            Method::PUT,
            Some(lan_bridge_payload(&payload)?),
        )
        .await?;
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
    if mock_thermal_plant_job_running(&device.status) {
        let manual_pps_requested = payload.manual_pps_enabled.is_some()
            || payload.manual_pps_mv.is_some()
            || payload.manual_pps_ma.is_some();
        if manual_pps_requested
            || payload.calibration.is_some()
            || payload.heater_enabled == Some(true)
        {
            return Err(HttpError::bad_request(
                "manual_pps_calibration_busy",
                "Manual PPS and heater controls cannot override a running thermal-model calibration.",
            ));
        }
    }
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
    if payload.fault_attention_acknowledged == Some(true) {
        device.status.fault_attention_pending = false;
    }
    if let Some(mode) = payload.thermal_profile_mode.as_deref() {
        device.status.thermal_profile_mode = mode.to_string();
        device.status.thermal_profile_resolved_bank = if mode == "100w"
            || (mode == "auto"
                && device.status.pps_capability_min_mv.unwrap_or(u16::MAX) <= 20_000
                && device.status.pps_capability_max_mv.unwrap_or(0) >= 20_000
                && device.status.pps_capability_max_ma.unwrap_or(0) >= 5_000)
        {
            "pps5a".to_string()
        } else {
            "pps3a".to_string()
        };
    }
    if let Some(thermal_control_profile) = payload.thermal_control_profile.as_ref() {
        let bank = thermal_control_profile
            .bank
            .as_deref()
            .unwrap_or(&device.status.thermal_profile_resolved_bank)
            .to_string();
        match thermal_control_profile.op {
            ThermalControlProfileOp::Preview => {
                device.preview_thermal_control_profile = thermal_control_profile.profile.clone();
            }
            ThermalControlProfileOp::ClearPreview => {
                device.preview_thermal_control_profile = None;
            }
            ThermalControlProfileOp::Save => {
                if bank == "pps5a" {
                    device.saved_thermal_control_profile_pps5a =
                        thermal_control_profile.profile.clone();
                } else {
                    device.saved_thermal_control_profile = thermal_control_profile.profile.clone();
                }
                device.preview_thermal_control_profile = None;
            }
            ThermalControlProfileOp::ClearSaved => {
                if bank == "pps5a" {
                    device.saved_thermal_control_profile_pps5a = None;
                } else {
                    device.saved_thermal_control_profile = None;
                }
            }
        }
    }
    let active_profile = device.preview_thermal_control_profile.as_ref().or({
        match device.status.thermal_profile_resolved_bank.as_str() {
            "pps5a" => device.saved_thermal_control_profile_pps5a.as_ref(),
            _ => device.saved_thermal_control_profile.as_ref(),
        }
    });
    let preview_active = device.preview_thermal_control_profile.is_some();
    device.status.thermal_control_profile_preview = preview_active;
    device.status.thermal_control =
        mock_thermal_runtime(device.status.target_temp_c, active_profile, preview_active);
    let status = device.status.clone();
    drop(state_lock);
    emit_runtime_config_event(&state, &device_id, &payload, &status);
    Ok(Json(status))
}

async fn configure_buzzer_debug(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<BuzzerDebugRequest>,
) -> Result<Json<BuzzerDebugStatus>, HttpError> {
    validate_buzzer_debug_request(&payload)?;
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "Buzzer debug is available only through a native USB serial lease.",
        ));
    }
    if !target
        .identity
        .capabilities
        .iter()
        .any(|capability| capability == "buzzer_debug")
    {
        return Err(HttpError::bad_request(
            "buzzer_debug_unavailable",
            "The connected firmware does not declare the buzzer_debug capability.",
        ));
    }

    let status = match serial_buzzer_debug(&state, &target, &payload).await {
        Ok(status) => status,
        Err(error) => {
            record_serial_bridge_error(&state, &device_id, "buzzer_debug", &error);
            return Err(error);
        }
    };
    state.emit(event(
        &device_id,
        "buzzer_debug",
        "buzzer debug command completed",
        json!({
            "op": payload.op,
            "cue": payload.cue,
            "scenario": payload.scenario,
            "state": status.state,
            "traceLength": status.trace.len(),
        }),
    ));
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
        CalibrationMode::Off | CalibrationMode::HeaterCurve | CalibrationMode::ThermalPlant => None,
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
        .thermal_profile_mode
        .as_deref()
        .is_some_and(|mode| !matches!(mode, "auto" | "65w" | "100w"))
    {
        return Err(HttpError::bad_request(
            "invalid_thermal_profile_mode",
            "thermalProfileMode must be auto, 65w, or 100w.",
        ));
    }
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
    if let Some(thermal_control_profile) = payload.thermal_control_profile.as_ref() {
        validate_thermal_control_profile_request(thermal_control_profile)?;
    }
    Ok(())
}

fn validate_buzzer_debug_request(payload: &BuzzerDebugRequest) -> Result<(), HttpError> {
    let valid = match payload.op {
        BuzzerDebugOp::Trigger => payload.cue.is_some() && payload.scenario.is_none(),
        BuzzerDebugOp::Run => payload.cue.is_none() && payload.scenario.is_some(),
        BuzzerDebugOp::Stop | BuzzerDebugOp::Status => {
            payload.cue.is_none() && payload.scenario.is_none() && !payload.repeat
        }
    };
    if valid {
        Ok(())
    } else {
        Err(HttpError::bad_request(
            "invalid_buzzer_debug_command",
            "buzzer debug requires exactly the fields for its operation.",
        ))
    }
}

fn validate_thermal_control_profile_request(
    request: &ThermalControlProfileRequest,
) -> Result<(), HttpError> {
    if request
        .bank
        .as_deref()
        .is_some_and(|bank| !matches!(bank, "pps3a" | "pps5a"))
    {
        return Err(HttpError::bad_request(
            "invalid_thermal_profile_bank",
            "thermalControlProfile.bank must be pps3a or pps5a.",
        ));
    }
    match request.op {
        ThermalControlProfileOp::Preview | ThermalControlProfileOp::Save => {
            let profile = request.profile.as_ref().ok_or_else(|| {
                HttpError::bad_request(
                    "thermal_profile_required",
                    "thermalControlProfile.profile is required for preview/save.",
                )
            })?;
            if profile.points.len() != FRONT_PANEL_PRESET_COUNT {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermalControlProfile.profile.points must contain exactly 10 values.",
                ));
            }
            if let Some(settings) = profile.settings.as_ref()
                && (settings.temp_filter_alpha_permille == 0
                    || settings.temp_filter_alpha_permille > 1_000
                    || !(AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN..=PPS_HARDWARE_MAX_MV)
                        .contains(&settings.auto_adjustable_working_floor_mv)
                    || settings.heater_current_reserve_ma > 1_000
                    || settings.approach_min_power_ratio_permille > 1_000
                    || !(1..=255).contains(&settings.approach_max_ticks))
            {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermal profile settings must use 1..1000 alpha, 5000..28000 auto adjustable floor, 0..1000mA heater current reserve, 0..1000 approach-min ratio, and 1..255 approach max ticks.",
                ));
            }
            for point in profile.points.iter().flatten() {
                if point.brake_distance_centi_c == 0
                    || point.warmup_power_permille > 1_000
                    || point.approach_power_permille > 1_000
                    || point.approach_floor_power_permille > 1_000
                    || !(100..=4_000).contains(&point.approach_damping_exponent_permille)
                    || point.hold_power_permille > 1_000
                    || point.hold_reheat_power_permille > 1_000
                    || point.warmup_reenter_centi_c > 5_000
                    || point.hold_entry_centi_c > 5_000
                    || point.hold_exit_centi_c > 5_000
                    || point.hold_on_centi_c > 5_000
                    || point.hold_off_centi_c > 5_000
                    || point.overshoot_cutoff_centi_c > 5_000
                    || point.hold_kp_permille_per_c > 10_000
                    || point.hold_ki_permille_per_c_tick > 10_000
                    || point.hold_blend_ticks > 255
                    || point.approach_lead_ticks > 255
                    || point.hold_lead_ticks > 255
                {
                    return Err(HttpError::bad_request(
                        "invalid_thermal_profile",
                        "thermal profile points must use positive brake distance, 0..1000 permille power, 100..4000 approach damping, <=5000 centi-C warmup/damping thresholds, <=10000 PI gains, and <=255 blend/lead ticks.",
                    ));
                }
            }
        }
        ThermalControlProfileOp::ClearPreview | ThermalControlProfileOp::ClearSaved => {
            if request.profile.is_some() {
                return Err(HttpError::bad_request(
                    "invalid_thermal_profile",
                    "thermalControlProfile.profile must be omitted for clear operations.",
                ));
            }
        }
    }
    Ok(())
}

const fn default_hold_blend_ticks() -> u16 {
    12
}

const fn default_approach_damping_exponent_permille() -> u16 {
    1_000
}

const fn default_auto_adjustable_working_floor_mv() -> u16 {
    AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT
}

const fn default_heater_current_reserve_ma() -> u16 {
    200
}

fn mock_thermal_default_settings() -> MockThermalCandidateSettings {
    MockThermalCandidateSettings {
        temp_filter_alpha_permille: 750,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c: 20,
        hold_exit_centi_c: 80,
        hold_on_centi_c: 15,
        hold_off_centi_c: 80,
        overshoot_cutoff_centi_c: 120,
        approach_max_ticks: 250,
        approach_min_power_ratio_permille: 500,
        hold_kp_permille_per_c: 35,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 12,
        hold_reheat_power_permille: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        auto_adjustable_working_floor_mv: AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT,
        heater_current_reserve_ma: 200,
    }
}

type MockThermalTargetValues = (
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
    u16,
);

fn mock_thermal_default_target_values(target_temp_c: i16) -> MockThermalTargetValues {
    if target_temp_c <= 60 {
        (
            1_310, 1_000, 590, 510, 1_320, 60, 60, 200, 540, 30, 120, 150, 8, 2, 1, 4, 2,
        )
    } else if target_temp_c <= 100 {
        (
            1_100, 1_000, 420, 220, 1_400, 170, 260, 12, 60, 30, 180, 230, 55, 2, 6, 9, 0,
        )
    } else if target_temp_c <= 140 {
        (
            1_000, 1_000, 420, 200, 1_000, 280, 340, 10, 55, 30, 160, 220, 22, 1, 1, 4, 0,
        )
    } else if target_temp_c <= 180 {
        (
            650, 1_000, 760, 460, 800, 450, 620, 15, 70, 25, 240, 300, 20, 1, 3, 4, 0,
        )
    } else if target_temp_c <= 220 {
        (
            520, 1_000, 760, 600, 550, 620, 700, 8, 50, 14, 240, 320, 22, 1, 2, 2, 0,
        )
    } else {
        (
            500, 1_000, 960, 860, 350, 850, 930, 10, 55, 14, 320, 420, 12, 1, 1, 1, 0,
        )
    }
}

fn mock_thermal_default_target_point(target_temp_c: i16) -> MockThermalCandidatePoint {
    let (
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = mock_thermal_default_target_values(target_temp_c);
    MockThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c: 0,
        hold_power_permille,
        hold_reheat_power_permille,
        warmup_reenter_centi_c: 1_000,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    }
}

fn mock_thermal_profile_from_package(
    package: &ThermalControlProfilePackage,
) -> MockThermalCandidateProfile {
    let default_settings = mock_thermal_default_settings();
    let settings = package
        .settings
        .map(|settings| MockThermalCandidateSettings {
            temp_filter_alpha_permille: settings.temp_filter_alpha_permille,
            warmup_reenter_centi_c: settings.warmup_reenter_centi_c,
            hold_entry_centi_c: settings.hold_entry_centi_c,
            hold_exit_centi_c: settings.hold_exit_centi_c,
            hold_on_centi_c: settings.hold_on_centi_c,
            hold_off_centi_c: settings.hold_off_centi_c,
            overshoot_cutoff_centi_c: settings.overshoot_cutoff_centi_c,
            approach_max_ticks: settings.approach_max_ticks,
            approach_min_power_ratio_permille: settings.approach_min_power_ratio_permille,
            hold_kp_permille_per_c: settings.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: settings.hold_ki_permille_per_c_tick,
            hold_blend_ticks: settings.hold_blend_ticks,
            hold_reheat_power_permille: settings.hold_reheat_power_permille,
            approach_lead_ticks: settings.approach_lead_ticks,
            hold_lead_ticks: settings.hold_lead_ticks,
            auto_adjustable_working_floor_mv: settings.auto_adjustable_working_floor_mv,
            heater_current_reserve_ma: settings.heater_current_reserve_ma,
        })
        .unwrap_or(default_settings);
    let point_targets = {
        let explicit = package
            .points
            .iter()
            .flatten()
            .map(|point| point.target_temp_c)
            .collect::<Vec<_>>();
        if explicit.is_empty() {
            THERMAL_PROFILE_ANCHOR_TARGETS_C.to_vec()
        } else {
            explicit
        }
    };
    let points = point_targets
        .into_iter()
        .map(|target_temp_c| {
            let default_point = mock_thermal_default_target_point(target_temp_c);
            let point = package
                .points
                .iter()
                .flatten()
                .find(|point| point.target_temp_c == target_temp_c);
            MockThermalCandidatePoint {
                target_temp_c,
                brake_distance_centi_c: point
                    .map(|point| point.brake_distance_centi_c)
                    .unwrap_or(default_point.brake_distance_centi_c),
                warmup_power_permille: point
                    .map(|point| point.warmup_power_permille)
                    .unwrap_or(default_point.warmup_power_permille),
                approach_power_permille: point
                    .map(|point| point.approach_power_permille)
                    .unwrap_or(default_point.approach_power_permille),
                approach_floor_power_permille: point
                    .map(|point| point.approach_floor_power_permille)
                    .unwrap_or(default_point.approach_floor_power_permille),
                approach_damping_exponent_permille: point
                    .map(|point| point.approach_damping_exponent_permille)
                    .unwrap_or(default_point.approach_damping_exponent_permille),
                approach_tail_window_centi_c: point
                    .map(|point| point.approach_tail_window_centi_c)
                    .unwrap_or(default_point.approach_tail_window_centi_c),
                hold_power_permille: point
                    .map(|point| point.hold_power_permille)
                    .unwrap_or(default_point.hold_power_permille),
                hold_reheat_power_permille: point
                    .map(|point| point.hold_reheat_power_permille)
                    .unwrap_or(default_point.hold_reheat_power_permille),
                warmup_reenter_centi_c: point
                    .map(|point| point.warmup_reenter_centi_c)
                    .unwrap_or(default_point.warmup_reenter_centi_c),
                hold_entry_centi_c: point
                    .map(|point| point.hold_entry_centi_c)
                    .unwrap_or(default_point.hold_entry_centi_c),
                hold_exit_centi_c: point
                    .map(|point| point.hold_exit_centi_c)
                    .unwrap_or(default_point.hold_exit_centi_c),
                hold_on_centi_c: point
                    .map(|point| point.hold_on_centi_c)
                    .unwrap_or(default_point.hold_on_centi_c),
                hold_off_centi_c: point
                    .map(|point| point.hold_off_centi_c)
                    .unwrap_or(default_point.hold_off_centi_c),
                overshoot_cutoff_centi_c: point
                    .map(|point| point.overshoot_cutoff_centi_c)
                    .unwrap_or(default_point.overshoot_cutoff_centi_c),
                hold_kp_permille_per_c: point
                    .map(|point| point.hold_kp_permille_per_c)
                    .unwrap_or(default_point.hold_kp_permille_per_c),
                hold_ki_permille_per_c_tick: point
                    .map(|point| point.hold_ki_permille_per_c_tick)
                    .unwrap_or(default_point.hold_ki_permille_per_c_tick),
                hold_blend_ticks: point
                    .map(|point| point.hold_blend_ticks)
                    .unwrap_or(default_point.hold_blend_ticks),
                approach_lead_ticks: point
                    .map(|point| point.approach_lead_ticks)
                    .unwrap_or(default_point.approach_lead_ticks),
                hold_lead_ticks: point
                    .map(|point| point.hold_lead_ticks)
                    .unwrap_or(default_point.hold_lead_ticks),
            }
        })
        .collect();
    MockThermalCandidateProfile { settings, points }
}

fn mock_thermal_candidate_point(
    profile: &MockThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<MockThermalCandidatePoint> {
    profile
        .points
        .iter()
        .copied()
        .find(|point| point.target_temp_c == target_temp_c)
}

fn mock_thermal_interpolated_candidate_point(
    profile: &MockThermalCandidateProfile,
    target_temp_c: i16,
) -> Option<MockThermalCandidatePoint> {
    if let Some(point) = mock_thermal_candidate_point(profile, target_temp_c) {
        return Some(point);
    }
    let mut points = profile.points.clone();
    points.sort_by_key(|point| point.target_temp_c);
    let lower = points
        .iter()
        .copied()
        .rev()
        .find(|point| point.target_temp_c < target_temp_c)?;
    let upper = points
        .iter()
        .copied()
        .find(|point| point.target_temp_c > target_temp_c)?;
    let ratio = f32::from(target_temp_c - lower.target_temp_c)
        / f32::from(upper.target_temp_c - lower.target_temp_c);
    let lerp = |left: u16, right: u16, upper_bound: u16| {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 {
            0.55
        } else {
            0.20
        }
    } else {
        0.0
    };
    let interpolated_brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_hold_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    };
    let low_temp_reheat_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    };
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    Some(MockThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: interpolated_brake_distance,
        warmup_power_permille: lerp(
            lower.warmup_power_permille,
            upper.warmup_power_permille,
            1_000,
        ),
        approach_power_permille: lerp(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent_permille: lerp(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            THERMAL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        ),
        approach_tail_window_centi_c: lerp(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            THERMAL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
        ),
        hold_power_permille: scale_low_temp_hold(lerp(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        warmup_reenter_centi_c: lerp(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            5_000,
        ),
        hold_entry_centi_c: lerp(lower.hold_entry_centi_c, upper.hold_entry_centi_c, 5_000),
        hold_exit_centi_c: lerp(lower.hold_exit_centi_c, upper.hold_exit_centi_c, 5_000),
        hold_on_centi_c: lerp(lower.hold_on_centi_c, upper.hold_on_centi_c, 5_000),
        hold_off_centi_c: lerp(lower.hold_off_centi_c, upper.hold_off_centi_c, 5_000),
        overshoot_cutoff_centi_c: lerp(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        ),
        hold_kp_permille_per_c: lerp(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        ),
        hold_ki_permille_per_c_tick: {
            let interpolated = lerp(
                lower.hold_ki_permille_per_c_tick,
                upper.hold_ki_permille_per_c_tick,
                10_000,
            );
            interpolated.max(1)
        },
        hold_blend_ticks: lerp(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)),
        approach_lead_ticks: lerp(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        ),
        hold_lead_ticks: lerp(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        ),
    })
}

fn mock_thermal_runtime(
    target_temp_c: i16,
    package: Option<&ThermalControlProfilePackage>,
    preview_active: bool,
) -> ThermalControlRuntime {
    let target_temp_c = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
    let profile = package.map(mock_thermal_profile_from_package);
    let profile_source = if preview_active {
        "preview"
    } else if profile.is_some() {
        "saved"
    } else {
        "default"
    };
    let profile_covers_target = profile
        .as_ref()
        .and_then(|profile| mock_thermal_interpolated_candidate_point(profile, target_temp_c))
        .is_some();
    let point = profile
        .as_ref()
        .and_then(|profile| mock_thermal_interpolated_candidate_point(profile, target_temp_c));
    let (
        brake_distance_centi_c,
        _default_warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        hold_power_permille,
        hold_reheat_power_permille,
        warmup_reenter_centi_c,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
    ) = point
        .map(|point| {
            (
                point.brake_distance_centi_c,
                point.warmup_power_permille,
                point.approach_power_permille,
                point.approach_floor_power_permille,
                point.approach_damping_exponent_permille,
                point.hold_power_permille,
                point.hold_reheat_power_permille,
                point.warmup_reenter_centi_c,
                point.hold_entry_centi_c,
                point.hold_exit_centi_c,
                point.hold_on_centi_c,
                point.hold_off_centi_c,
                point.overshoot_cutoff_centi_c,
                point.hold_kp_permille_per_c,
                point.hold_ki_permille_per_c_tick,
                point.hold_blend_ticks,
                point.approach_lead_ticks,
                point.hold_lead_ticks,
            )
        })
        .unwrap_or_else(|| {
            let default_point = mock_thermal_default_target_point(target_temp_c);
            (
                default_point.brake_distance_centi_c,
                default_point.warmup_power_permille,
                default_point.approach_power_permille,
                default_point.approach_floor_power_permille,
                default_point.approach_damping_exponent_permille,
                default_point.hold_power_permille,
                default_point.hold_reheat_power_permille,
                default_point.warmup_reenter_centi_c,
                default_point.hold_entry_centi_c,
                default_point.hold_exit_centi_c,
                default_point.hold_on_centi_c,
                default_point.hold_off_centi_c,
                default_point.overshoot_cutoff_centi_c,
                default_point.hold_kp_permille_per_c,
                default_point.hold_ki_permille_per_c_tick,
                default_point.hold_blend_ticks,
                default_point.approach_lead_ticks,
                default_point.hold_lead_ticks,
            )
        });
    let warmup_power_permille = if let Some(point) = point {
        point
            .warmup_power_permille
            .max(point.approach_power_permille)
    } else {
        1_000
    };
    let settings = profile
        .as_ref()
        .map(|profile| profile.settings)
        .unwrap_or_else(mock_thermal_default_settings);
    ThermalControlRuntime {
        profile_active: profile.is_some(),
        profile_covers_target,
        profile_source: profile_source.to_string(),
        target_temp_c,
        brake_distance_centi_c,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent_permille,
        approach_tail_window_centi_c: point
            .map(|point| point.approach_tail_window_centi_c)
            .unwrap_or_default(),
        hold_power_permille,
        hold_reheat_power_permille,
        hold_entry_centi_c,
        hold_exit_centi_c,
        hold_on_centi_c,
        hold_off_centi_c,
        overshoot_cutoff_centi_c,
        hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick,
        hold_blend_ticks,
        approach_lead_ticks,
        hold_lead_ticks,
        temp_filter_alpha_permille: settings.temp_filter_alpha_permille,
        warmup_reenter_centi_c,
        approach_max_ticks: settings.approach_max_ticks,
        approach_min_power_ratio_permille: settings.approach_min_power_ratio_permille,
        auto_adjustable_working_floor_mv: settings.auto_adjustable_working_floor_mv,
        heater_current_reserve_ma: settings
            .heater_current_reserve_ma
            .min(THERMAL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX),
    }
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
            let samples = &mut calibration.channel_mut(channel).samples;
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
                target_adc_mv: payload
                    .target_adc_mv
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
            let samples = &mut calibration.channel_mut(channel).samples;
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
            calibration.channel_mut(channel).samples = vec![None; ADC_CALIBRATION_MAX_SAMPLES];
        }
        CalibrationConfigOp::Import => {
            let state = payload.state.clone().ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_state_required",
                    "Calibration import requires state.",
                )
            })?;
            validate_calibration_state(&state)?;
            *calibration = normalize_calibration_state(state);
        }
        CalibrationConfigOp::SetActiveSlot => {
            let channel = payload.channel.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_channel_required",
                    "Setting active slot requires a channel.",
                )
            })?;
            let slot = payload.slot.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_slot_required",
                    "Setting active slot requires slot.",
                )
            })?;
            calibration.channel_mut(channel).active_slot = slot;
        }
        CalibrationConfigOp::SetSlotFit => {
            let channel = payload.channel.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_channel_required",
                    "Setting slot fit requires a channel.",
                )
            })?;
            let slot = payload.slot.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_slot_required",
                    "Setting slot fit requires slot.",
                )
            })?;
            let fit = payload.fit.ok_or_else(|| {
                HttpError::bad_request(
                    "calibration_fit_required",
                    "Setting slot fit requires gain/offset.",
                )
            })?;
            *calibration.channel_mut(channel).slot_fit_mut(slot) = fit;
        }
    }
    calibration.rtd_adc.sanitize_slot_fits();
    calibration.vin_adc.sanitize_slot_fits();
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

fn validate_calibration_channel_state(channel: &CalibrationChannelState) -> Result<(), HttpError> {
    if channel.samples.len() > ADC_CALIBRATION_MAX_SAMPLES {
        return Err(HttpError::bad_request(
            "calibration_samples_too_large",
            "Calibration import supports at most 8 samples per channel.",
        ));
    }
    Ok(())
}

fn validate_calibration_state(state: &CalibrationState) -> Result<(), HttpError> {
    validate_calibration_channel_state(&state.rtd_adc)?;
    validate_calibration_channel_state(&state.vin_adc)?;
    Ok(())
}

fn normalize_calibration_channel_state(
    mut channel_state: CalibrationChannelState,
    channel: CalibrationChannel,
) -> CalibrationChannelState {
    channel_state.samples = channel_state
        .samples
        .into_iter()
        .map(|sample| sample.map(|sample| normalize_calibration_sample(sample, channel)))
        .collect();
    channel_state.refresh(channel);
    channel_state
}

fn normalize_calibration_state(mut state: CalibrationState) -> CalibrationState {
    state.rtd_adc = normalize_calibration_channel_state(state.rtd_adc, CalibrationChannel::RtdAdc);
    state.vin_adc = normalize_calibration_channel_state(state.vin_adc, CalibrationChannel::VinAdc);
    state
}

fn validate_heater_curve_package(package: &HeaterCurvePackage) -> Result<(), HttpError> {
    if package.points.len() > HEATER_CURVE_MAX_POINTS {
        return Err(HttpError::bad_request(
            "heater_curve_package_too_large",
            "Heater curve supports at most 8 points.",
        ));
    }
    if package
        .raw_observations
        .as_ref()
        .is_some_and(|observations| observations.points.len() > HEATER_CURVE_MAX_POINTS)
    {
        return Err(HttpError::bad_request(
            "heater_curve_raw_observations_too_large",
            "Heater curve raw observations support at most 8 points.",
        ));
    }
    Ok(())
}

fn normalize_heater_curve_package(mut package: HeaterCurvePackage) -> HeaterCurvePackage {
    package
        .points
        .sort_by_key(|point| point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX));
    package.points.resize(HEATER_CURVE_MAX_POINTS, None);
    if let Some(raw_observations) = package.raw_observations.as_mut() {
        raw_observations.points.sort_by_key(|observation| {
            observation
                .map(|observation| observation.raw_rtd_adc_mv)
                .unwrap_or(u16::MAX)
        });
        raw_observations
            .points
            .resize(HEATER_CURVE_MAX_POINTS, None);
    }
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
        CalibrationChannel::RtdAdc => payload.target_adc_mv,
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
    if calibration.mode == Some(CalibrationMode::ThermalPlant) {
        return Err(HttpError::bad_request(
            "thermal_plant_managed_by_job",
            "Automatic thermal-model calibration is managed by thermal_plant_auto.",
        ));
    }
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

fn mock_thermal_plant_job_running(status: &ControlPlaneStatus) -> bool {
    status.calibration.mode == CalibrationMode::ThermalPlant
        && status.calibration.job.status == CalibrationJobStatus::Running
}

fn thermal_plant_start_request_for_device(
    device: &DeviceRecord,
) -> Result<(MockPpsApdo, u16), HttpError> {
    let source = mock_thermal_plant_source_limits(device).ok_or_else(|| {
        HttpError::bad_request(
            "thermal_plant_source_unsupported",
            "Thermal-plant calibration requires a PPS capability covering 20V at 3A or more.",
        )
    })?;
    Ok((source, source.max_mv))
}

fn mock_thermal_plant_source_limits(device: &DeviceRecord) -> Option<MockPpsApdo> {
    let mut selected = None;
    let mut consider = |candidate: MockPpsApdo| {
        if candidate.min_mv > 20_000 || candidate.max_mv < 20_000 || candidate.max_ma < 3_000 {
            return;
        }
        if selected.is_none_or(|current: MockPpsApdo| {
            candidate.max_ma > current.max_ma
                || (candidate.max_ma == current.max_ma
                    && (candidate.max_mv > current.max_mv
                        || (candidate.max_mv == current.max_mv
                            && candidate.min_mv < current.min_mv)))
        }) {
            selected = Some(candidate);
        }
    };
    if device.mock_pps_apdos.is_empty() {
        if let (Some(min_mv), Some(max_mv), Some(max_ma)) = (
            device.status.pps_capability_min_mv,
            device.status.pps_capability_max_mv,
            device.status.pps_capability_max_ma,
        ) {
            consider(MockPpsApdo {
                min_mv,
                max_mv,
                max_ma,
            });
        }
    } else {
        for apdo in device.mock_pps_apdos.iter().copied() {
            consider(apdo);
        }
    }
    selected
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

async fn list_firmware_bundles(
    State(state): State<AppState>,
) -> Result<Json<FirmwareBundleCatalog>, HttpError> {
    let mut bundles = Vec::new();
    let entries = fs::read_dir(state.bundle_store.path()).map_err(sanitize_io_error)?;
    for entry in entries {
        let entry = entry.map_err(sanitize_io_error)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("fluxpurr-fw") {
            continue;
        }
        let bundle = firmware_bundle::read_bundle(&entry.path()).map_err(bundle_http_error)?;
        bundles.push(bundle_summary(&bundle));
    }
    bundles.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    Ok(Json(FirmwareBundleCatalog { bundles }))
}

async fn import_firmware_bundle(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<FirmwareBundleSummary>), HttpError> {
    if body.len() as u64 > firmware_bundle::MAX_BUNDLE_BYTES {
        return Err(HttpError::bad_request(
            "bundle_too_large",
            "Firmware bundle exceeds the 8 MiB limit.",
        ));
    }
    let bundle = firmware_bundle::read_bundle_bytes(&body).map_err(bundle_http_error)?;
    let filename = format!(
        "{}.fluxpurr-fw",
        bundle.bundle_sha256.trim_start_matches("sha256:")
    );
    let target = state.bundle_store.path().join(filename);
    if !target.exists() {
        let temp = state.bundle_store.path().join(format!(
            ".import-{}-{}",
            now_millis(),
            EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&temp, &body).map_err(sanitize_io_error)?;
        fs::rename(&temp, &target).map_err(sanitize_io_error)?;
    }
    Ok((StatusCode::CREATED, Json(bundle_summary(&bundle))))
}

fn bundle_summary(bundle: &firmware_bundle::FirmwareBundle) -> FirmwareBundleSummary {
    FirmwareBundleSummary {
        artifact_id: bundle.bundle_sha256.clone(),
        source: "local".into(),
        channel: bundle.manifest.identity.channel,
        version: bundle.manifest.identity.version.clone(),
        source_sha: bundle.manifest.identity.source_sha.clone(),
        build_id: bundle.manifest.identity.build_id.clone(),
        bundle_sha256: bundle.bundle_sha256.clone(),
        size: bundle.archive_size,
        layout_id: bundle.manifest.layout.id.clone(),
        operations: vec![
            FirmwareOperation::Update,
            FirmwareOperation::InstallRecovery,
        ],
    }
}

fn bundle_http_error(error: firmware_bundle::BundleError) -> HttpError {
    HttpError::bad_request("firmware_bundle_invalid", &error.to_string())
}

fn validate_update_runtime_facts(
    transport: DeviceTransport,
    current_version: &str,
    status: &ControlPlaneStatus,
) -> Result<(), HttpError> {
    if transport == DeviceTransport::NativeSerial
        && (current_version == "unknown" || current_version.trim().is_empty())
    {
        return Err(HttpError::forbidden(
            "update_identity_required",
            "Update requires a verified Flux Purr runtime identity.",
        ));
    }
    if status.heater_enabled || !status.current_temp_c.is_finite() || status.current_temp_c > 40.0 {
        return Err(HttpError::forbidden(
            "update_temperature_gate",
            "Update requires heater off and a valid temperature at or below 40 C.",
        ));
    }
    Ok(())
}

async fn refresh_native_update_runtime_facts(
    state: &AppState,
    target: &DeviceRecord,
    lease_id: &str,
) -> Result<(Identity, ControlPlaneStatus), HttpError> {
    let identity =
        serial_request_payload::<Identity>(state, target, "get_identity", "identity").await?;
    let _stopped = serial_runtime_config(
        state,
        target,
        &RuntimeConfigRequest {
            lease_id: lease_id.to_string(),
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            heater_enabled: Some(false),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
    )
    .await?;
    let status =
        serial_request_payload::<ControlPlaneStatus>(state, target, "get_status", "status").await?;
    Ok((identity, status))
}

async fn firmware_operation(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<FirmwareOperationRequest>,
) -> Result<Json<FirmwareOperationResult>, HttpError> {
    let mut progress = FirmwareOperationProgress::new(
        &state,
        &device_id,
        payload.operation,
        &payload.artifact_id,
        payload.dry_run,
    );
    progress.operation_started();
    let initial_stage = if payload.dry_run {
        "artifact"
    } else {
        "authorization"
    };
    progress.stage_started(initial_stage, json!({}));

    let bundle_path = state.bundle_store.path().join(format!(
        "{}.fluxpurr-fw",
        payload.artifact_id.trim_start_matches("sha256:")
    ));
    let bundle = progress.require(firmware_bundle::read_bundle(&bundle_path).map_err(|error| {
        HttpError::bad_request(
            "firmware_bundle_unavailable",
            &format!("The imported firmware bundle is unavailable: {error}"),
        )
    }))?;
    if bundle.bundle_sha256 != payload.artifact_id {
        return Err(progress.fail(HttpError::bad_request(
            "artifact_id_mismatch",
            "artifactId does not match the imported bundle content.",
        )));
    }
    if payload.dry_run {
        progress.stage_completed("artifact", json!({}));
        progress.stage_started("transport", json!({}));
    }

    let target_result = {
        let mut inner = state.lock()?;
        inner
            .require_lease(&device_id, Some(&payload.lease_id))
            .and_then(|_| {
                let device = inner
                    .devices
                    .get(&device_id)
                    .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?;
                if device.transport == DeviceTransport::Lan {
                    return Err(HttpError::bad_request(
                        "lan_flash_unsupported",
                        "Firmware flashing is unavailable through the DEVD LAN bridge.",
                    ));
                }
                Ok(device.clone())
            })
    };
    let target = progress.require(target_result)?;
    let port_path = target
        .port_path
        .clone()
        .unwrap_or_else(|| "mock://esp32s3".into());
    let mock_identity = target.identity.device_id.clone();
    let transport = target.transport;
    let mut current_version = target.identity.firmware_version.clone();
    let mut status = target.status.clone();

    // An update preserves an existing Flux Purr installation, so it must stop
    // heat and use fresh runtime facts from this exact serial target before it
    // enters ROM mode. Discovery state can be stale while a heater is running.
    if payload.operation == FirmwareOperation::Update {
        let identity = match transport {
            DeviceTransport::NativeSerial => {
                let (identity, live_status) = progress.require(
                    refresh_native_update_runtime_facts(&state, &target, &payload.lease_id).await,
                )?;
                status = live_status;
                Some(identity)
            }
            DeviceTransport::Mock => {
                status.heater_enabled = false;
                status.heater_output_percent = 0;
                status.heater_physical_output_percent = 0;
                None
            }
            DeviceTransport::Lan => unreachable!(),
        };
        if let Some(identity) = identity.as_ref() {
            current_version = identity.firmware_version.clone();
        }
        if let Ok(mut inner) = state.lock() {
            if let Some(device) = inner.devices.get_mut(&device_id) {
                if device.port_path.as_deref() == target.port_path.as_deref() {
                    if let Some(identity) = identity {
                        device.identity = identity;
                    }
                    device.network = status.network.clone();
                    device.status = status.clone();
                    device.connection = ConnectionState::Connected;
                }
            }
        }
        progress.require(validate_update_runtime_facts(
            transport,
            &current_version,
            &status,
        ))?;
    }
    if payload.dry_run {
        progress.stage_completed("transport", json!({}));
        progress.stage_started("rom_reset", json!({}));
    }

    let security_result = match transport {
        DeviceTransport::Mock => RomSecurityInfo {
            rom_mac: mock_identity,
            secure_boot_enabled: false,
            flash_encryption_enabled: false,
            secure_download_mode_enabled: false,
            response_known: true,
            chip_is_esp32s3: true,
            flash_size_bytes: 4 * 1024 * 1024,
            package_matches: true,
        },
        DeviceTransport::NativeSerial => {
            progress.require(probe_native_rom_security(&state, &port_path).await)?
        }
        DeviceTransport::Lan => unreachable!(),
    };
    let security = security_result;
    if payload.dry_run {
        progress.stage_completed("rom_reset", json!({}));
        progress.stage_started("chip_flash_security", json!({}));
    }
    progress.require(security.validate_for_flash())?;
    let rom_mac = security.rom_mac.clone();
    if payload.dry_run {
        progress.stage_completed("chip_flash_security", json!({}));
        progress.stage_started("layout_config", json!({}));
    }

    let source_partition_hash = progress.require(
        async {
            let mut source_partition_hash = None;
            if payload.operation == FirmwareOperation::Update {
                if transport == DeviceTransport::NativeSerial {
                    let source_hash = probe_native_partition_hash(&state, &port_path).await?;
                    if !firmware_bundle::source_partition_hash_supported(
                        &source_hash,
                        &bundle.manifest.migrations,
                    )
                    .map_err(bundle_http_error)?
                    {
                        return Err(HttpError::forbidden(
                            "source_layout_unsupported",
                            "The current partition-table hash has no declared supported migration.",
                        ));
                    }
                    source_partition_hash = Some(source_hash);
                }
                let current_semver = semver::Version::parse(
                    current_version
                        .trim_start_matches("fw/")
                        .trim_start_matches('v'),
                );
                let target_semver = semver::Version::parse(
                    bundle.manifest.identity.version.trim_start_matches('v'),
                );
                if current_semver
                    .ok()
                    .zip(target_semver.ok())
                    .is_some_and(|(current, target)| target < current)
                    && !payload.allow_downgrade
                {
                    return Err(HttpError::forbidden(
                        "downgrade_confirmation_required",
                        "The target firmware is older; explicit allowDowngrade is required.",
                    ));
                }
            }
            Ok(source_partition_hash)
        }
        .await,
    )?;
    if payload.dry_run {
        progress.stage_completed("layout_config", json!({}));
        progress.stage_started("preflight", json!({}));
    }

    let preflight_digest = firmware_preflight_digest(
        &payload,
        &device_id,
        &port_path,
        &rom_mac,
        &bundle.bundle_sha256,
        source_partition_hash.as_deref(),
    );
    if payload.dry_run {
        let token = {
            let mut inner = state.lock()?;
            let token = inner.next_id("firmware-approval");
            inner.firmware_approvals.insert(
                token.clone(),
                FirmwareApproval {
                    lease_id: payload.lease_id.clone(),
                    device_id: device_id.clone(),
                    port_path,
                    rom_mac,
                    bundle_sha256: bundle.bundle_sha256.clone(),
                    operation: payload.operation,
                    allow_downgrade: payload.allow_downgrade,
                    preflight_digest,
                    expires_at: Instant::now() + Duration::from_secs(5 * 60),
                },
            );
            token
        };
        progress.stage_completed("preflight", json!({}));
        progress.operation_completed("passed");
        return Ok(Json(FirmwareOperationResult {
            operation_id: progress.operation_id().to_string(),
            artifact_id: bundle.bundle_sha256,
            operation: payload.operation,
            dry_run: true,
            outcome: "passed".into(),
            approval_token: Some(token),
            approval_expires_in_ms: Some(5 * 60 * 1000),
            stages: firmware_preflight_stages(),
            message: "Preflight passed; no flash write performed.".into(),
        }));
    }

    let token = progress.require(payload.approval_token.as_deref().ok_or_else(|| {
        HttpError::forbidden(
            "approval_required",
            "Execution requires a current single-use approval token.",
        )
    }))?;
    let approval_result = {
        let mut inner = state.lock()?;
        inner.firmware_approvals.remove(token).ok_or_else(|| {
            HttpError::forbidden(
                "approval_invalid",
                "The approval token is invalid or already used.",
            )
        })
    };
    let approval = progress.require(approval_result)?;
    if approval.expires_at <= Instant::now()
        || approval.lease_id != payload.lease_id
        || approval.device_id != device_id
        || approval.port_path != port_path
        || approval.rom_mac != rom_mac
        || approval.bundle_sha256 != bundle.bundle_sha256
        || approval.operation != payload.operation
        || approval.allow_downgrade != payload.allow_downgrade
        || approval.preflight_digest != preflight_digest
    {
        return Err(progress.fail(HttpError::forbidden(
            "approval_mismatch",
            "The target or preflight facts changed; run preflight again.",
        )));
    }
    let expected_confirm = match payload.operation {
        FirmwareOperation::Update => "FLASH",
        FirmwareOperation::InstallRecovery => "ERASE_INSTALL",
    };
    if payload.confirm.as_deref() != Some(expected_confirm) {
        return Err(progress.fail(HttpError::forbidden(
            "confirmation_required",
            &format!("Execution requires confirm={expected_confirm}."),
        )));
    }
    if !state.config.allow_real_flash {
        return Err(progress.fail(HttpError::forbidden(
            "real_flash_disabled",
            "Real flashing is disabled unless FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1.",
        )));
    }
    if transport != DeviceTransport::NativeSerial {
        return Err(progress.fail(HttpError::bad_request(
            "real_flash_requires_native_serial",
            "Real flash requires a native serial target.",
        )));
    }
    progress.stage_completed("authorization", json!({}));

    run_bundle_flash_transaction(
        &state,
        &bundle,
        payload.operation,
        &port_path,
        source_partition_hash.as_deref(),
        &mut progress,
    )
    .await?;
    let target_result = {
        let inner = match state.lock() {
            Ok(inner) => inner,
            Err(error) => return Err(progress.fail(error)),
        };
        inner
            .devices
            .get(&device_id)
            .cloned()
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))
    };
    let target = progress.require(target_result)?;
    progress.stage_started("runtime_reconnect", json!({}));
    // ESP32-S3 USB Serial/JTAG accepts the initial identity request during the
    // early boot control loop. The following install-status request reports
    // `startup_busy` until the runtime emits `boot_stage=runtime_ready`; the
    // serial exchange then performs exactly one retry on that marker. Sending
    // both requests only after the marker is not reliable on this transport.
    let identity =
        serial_request_payload::<Identity>(&state, &target, "get_identity", "identity").await;
    let install_status = serial_request_payload::<InstallStatus>(
        &state,
        &target,
        "get_install_status",
        "install_status",
    )
    .await;
    if identity.is_ok() && install_status.is_ok() {
        progress.stage_completed("runtime_reconnect", json!({}));
    } else {
        progress.stage_failed("runtime_reconnect", "runtime_reconnect_failed");
    }
    progress.stage_started("runtime_verify", json!({}));
    let verified = identity.as_ref().is_ok_and(|identity| {
        identity.firmware_version == bundle.manifest.identity.version
            && identity.git_sha == bundle.manifest.identity.source_sha
            && identity.build_id == bundle.manifest.identity.build_id
    }) && install_status.as_ref().is_ok_and(|status| {
        status.layout_id == bundle.manifest.layout.id
            && status.layout_version == bundle.manifest.layout.version
            && status.partition_table_sha256 == bundle.manifest.layout.partition_table_sha256
    });
    let outcome = if verified {
        progress.stage_completed("runtime_verify", json!({}));
        "verified"
    } else {
        progress.stage_failed("runtime_verify", "runtime_verification_failed");
        "write_complete_unverified"
    };
    progress.operation_completed(outcome);
    Ok(Json(FirmwareOperationResult {
        operation_id: progress.operation_id().to_string(),
        artifact_id: bundle.bundle_sha256,
        operation: payload.operation,
        dry_run: false,
        outcome: outcome.into(),
        approval_token: None,
        approval_expires_in_ms: None,
        stages: firmware_execution_stages(payload.operation),
        message: if verified {
            "Firmware bytes and runtime install status verified."
        } else {
            "Firmware bytes verified, but runtime identity or install status did not verify."
        }
        .into(),
    }))
}

async fn probe_native_partition_hash(
    state: &AppState,
    port_path: &str,
) -> Result<String, HttpError> {
    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let workspace = tempfile::tempdir().map_err(|error| {
        HttpError::internal(&format!("failed to create preflight workspace: {error}"))
    })?;
    let output_path = workspace.path().join("partition-table.bin");
    let args = vec![
        "read-flash".into(),
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port_path.into(),
        "--non-interactive".into(),
        "0x8000".into(),
        "0x1000".into(),
        output_path.to_string_lossy().into_owned(),
    ];
    require_espflash_success(&resolve_espflash_program(), &args).await?;
    let bytes = fs::read(output_path).map_err(|error| {
        HttpError::internal(&format!("failed to read partition preflight: {error}"))
    })?;
    if bytes.len() != 0x1000 {
        return Err(HttpError::forbidden(
            "source_layout_unknown",
            "The target partition table could not be read exactly.",
        ));
    }
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

async fn run_bundle_flash_transaction(
    state: &AppState,
    bundle: &firmware_bundle::FirmwareBundle,
    operation: FirmwareOperation,
    port_path: &str,
    source_partition_hash: Option<&str>,
    progress: &mut FirmwareOperationProgress,
) -> Result<(), HttpError> {
    let _serial_rpc = progress.require(
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await,
    )?;
    progress.require(drop_cached_serial_session(
        &state.serial_sessions,
        port_path,
    ))?;
    let workspace = progress.require(tempfile::tempdir().map_err(|error| {
        HttpError::internal(&format!("failed to create flash workspace: {error}"))
    }))?;
    for segment in &bundle.manifest.segments {
        let bytes = progress.require(bundle.images.get(&segment.path).ok_or_else(|| {
            HttpError::internal("validated bundle segment disappeared before execution")
        }))?;
        progress.require(
            fs::write(
                workspace.path().join(format!("{:?}.bin", segment.kind)),
                bytes,
            )
            .map_err(|error| HttpError::internal(&format!("failed to stage segment: {error}"))),
        )?;
    }
    let program = resolve_espflash_program();
    let common = vec![
        "--chip".into(),
        "esp32s3".into(),
        "--port".into(),
        port_path.into(),
        "--non-interactive".into(),
    ];
    let initial_reset = if is_esp_usb_serial_jtag_port(port_path) {
        "usb-reset"
    } else {
        "default-reset"
    };
    let preserved_config = if operation == FirmwareOperation::Update {
        progress.stage_started("write_segments", json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.iter().map(|segment| segment.length).sum::<u64>(),
            "unit": "bytes",
        }));
        let source_partition_hash = progress.require(
            source_partition_hash.ok_or_else(|| HttpError::internal("missing source layout")),
        )?;
        let copy = progress.require(
            firmware_bundle::config_copy_plan(source_partition_hash, &bundle.manifest.migrations)
                .map_err(bundle_http_error),
        )?;
        let path = workspace.path().join("preserved-flux-cfg.bin");
        let args = build_bundle_read_flash_args(
            &common,
            initial_reset,
            copy.source_address,
            copy.length,
            &path,
        );
        progress.require(require_bundle_espflash_success(&program, &args, port_path).await)?;
        let bytes =
            progress.require(fs::read(&path).map_err(|error| {
                HttpError::internal(&format!("failed to stage flux_cfg: {error}"))
            }))?;
        if bytes.len() as u64 != copy.length {
            return Err(progress.fail(HttpError::internal("flux_cfg staging length differs.")));
        }
        Some((path, bytes, copy))
    } else {
        None
    };
    if operation == FirmwareOperation::InstallRecovery {
        progress.stage_started("erase", json!({}));
        let mut args = vec!["erase-flash".into()];
        args.extend(common.clone());
        args.extend([
            "--before".into(),
            initial_reset.into(),
            "--after".into(),
            "no-reset".into(),
        ]);
        progress.require(require_bundle_espflash_success(&program, &args, port_path).await)?;
        progress.stage_completed("erase", json!({}));
        progress.stage_started("write_segments", json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.iter().map(|segment| segment.length).sum::<u64>(),
            "unit": "bytes",
        }));
    }
    let total_bytes = bundle
        .manifest
        .segments
        .iter()
        .map(|segment| segment.length)
        .sum::<u64>();
    let mut completed_bytes = 0_u64;
    for segment in &bundle.manifest.segments {
        let path = workspace.path().join(format!("{:?}.bin", segment.kind));
        let args = build_bundle_write_bin_args(&common, "no-reset", segment.address, &path);
        progress.require(require_bundle_espflash_success(&program, &args, port_path).await)?;
        completed_bytes = completed_bytes.saturating_add(segment.length);
        progress.stage_progress(
            "write_segments",
            json!({
                "completedUnits": completed_bytes,
                "totalUnits": total_bytes,
                "unit": "bytes",
            }),
        );
    }
    progress.stage_completed(
        "write_segments",
        json!({
            "completedUnits": completed_bytes,
            "totalUnits": total_bytes,
            "unit": "bytes",
        }),
    );
    progress.stage_started(
        "rom_md5",
        json!({
            "completedUnits": 0,
            "totalUnits": bundle.manifest.segments.len(),
            "unit": "segments",
        }),
    );
    for (index, segment) in bundle.manifest.segments.iter().enumerate() {
        let checksum = build_checksum_md5_args(&common, segment.address, segment.length);
        let output = progress
            .require(require_bundle_espflash_success(&program, &checksum, port_path).await)?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        if !stdout.contains(&segment.md5) {
            return Err(progress.fail(HttpError::internal(
                "ROM MD5 did not match the validated bundle segment.",
            )));
        }
        progress.stage_progress(
            "rom_md5",
            json!({
                "completedUnits": index + 1,
                "totalUnits": bundle.manifest.segments.len(),
                "unit": "segments",
            }),
        );
    }
    if let Some((path, expected, copy)) = preserved_config {
        let write = build_bundle_write_bin_args(&common, "no-reset", copy.target_address, &path);
        progress.require(require_bundle_espflash_success(&program, &write, port_path).await)?;
        let verified_path = workspace.path().join("verified-flux-cfg.bin");
        let read = build_bundle_read_flash_args(
            &common,
            "no-reset",
            copy.target_address,
            copy.length,
            &verified_path,
        );
        progress.require(require_bundle_espflash_success(&program, &read, port_path).await)?;
        let actual = progress.require(fs::read(verified_path).map_err(|error| {
            HttpError::internal(&format!("failed to verify preserved flux_cfg: {error}"))
        }))?;
        if actual != expected {
            return Err(progress.fail(HttpError::internal("flux_cfg byte verification failed.")));
        }
    }
    progress.stage_completed(
        "rom_md5",
        json!({
            "completedUnits": bundle.manifest.segments.len(),
            "totalUnits": bundle.manifest.segments.len(),
            "unit": "segments",
        }),
    );
    progress.stage_started("reset", json!({}));
    let mut reset = vec!["reset".into()];
    reset.extend(common);
    progress.require(require_bundle_espflash_success(&program, &reset, port_path).await)?;
    progress.stage_completed("reset", json!({}));
    Ok(())
}

fn build_checksum_md5_args(common: &[String], address: u64, length: u64) -> Vec<String> {
    let mut args = vec!["checksum-md5".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        "no-reset".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        length.to_string(),
    ]);
    args
}

async fn require_espflash_success(program: &Path, args: &[String]) -> Result<Output, HttpError> {
    let output = run_espflash_command_with_timeout(program, args, ESPFLASH_COMMAND_TIMEOUT).await?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(espflash_command_error(program, args, &output))
    }
}

async fn require_bundle_espflash_success(
    program: &Path,
    args: &[String],
    port_path: &str,
) -> Result<Output, HttpError> {
    let output = run_espflash_command_with_timeout(program, args, ESPFLASH_COMMAND_TIMEOUT).await?;
    if output.status.success() {
        return Ok(output);
    }
    if !is_esp_usb_serial_jtag_port(port_path) || !espflash_connection_failed(&output) {
        return Err(espflash_command_error(program, args, &output));
    }
    let Some(before_index) = args.iter().position(|argument| argument == "--before") else {
        return Err(espflash_command_error(program, args, &output));
    };
    let Some(before_reset) = args.get(before_index + 1).map(String::as_str) else {
        return Err(espflash_command_error(program, args, &output));
    };
    let recovery_modes: &[&str] = match before_reset {
        "no-reset" | "usb-reset" => &["usb-reset", "default-reset"],
        _ => return Err(espflash_command_error(program, args, &output)),
    };

    let mut attempts = vec![espflash_failure_details(program, args, &output)];
    for reset_mode in recovery_modes {
        let retry_args = replace_espflash_before_reset(args, reset_mode)
            .expect("bundle recovery modes require an espflash --before argument");
        tokio::time::sleep(ESPFLASH_USB_RESET_RETRY_DELAY).await;
        let retry_output =
            run_espflash_command_with_timeout(program, &retry_args, ESPFLASH_COMMAND_TIMEOUT)
                .await?;
        if retry_output.status.success() {
            return Ok(retry_output);
        }
        attempts.push(espflash_failure_details(
            program,
            &retry_args,
            &retry_output,
        ));
        if !espflash_connection_failed(&retry_output) {
            break;
        }
    }

    Err(HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "espflash_failed".to_string(),
            message: "Protected espflash transaction failed after USB recovery attempts."
                .to_string(),
            retryable: true,
            details: Some(json!({ "attempts": attempts })),
        },
    })
}

fn replace_espflash_before_reset(args: &[String], before_reset: &str) -> Option<Vec<String>> {
    let index = args.iter().position(|argument| argument == "--before")?;
    let mut replaced = args.to_vec();
    *replaced.get_mut(index + 1)? = before_reset.to_string();
    Some(replaced)
}

fn espflash_command_error(program: &Path, args: &[String], output: &Output) -> HttpError {
    HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "espflash_failed".to_string(),
            message: "Protected espflash transaction failed.".to_string(),
            retryable: true,
            details: Some(espflash_failure_details(program, args, output)),
        },
    }
}

async fn probe_native_rom_security(
    state: &AppState,
    port_path: &str,
) -> Result<RomSecurityInfo, HttpError> {
    use espflash::{
        connection::{Connection, ResetAfterOperation, ResetBeforeOperation},
        flasher::Flasher,
    };
    use serialport::{FlowControl, SerialPortType, UsbPortInfo};

    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let port_path = port_path.to_owned();
    tokio::task::spawn_blocking(move || {
        let port_info = serialport::available_ports()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|candidate| candidate.port_name == port_path)
            .ok_or_else(|| "authorized serial port is no longer enumerated".to_string())?;
        let usb_info = match port_info.port_type {
            SerialPortType::UsbPort(info) => info,
            SerialPortType::PciPort | SerialPortType::Unknown => UsbPortInfo {
                vid: 0,
                pid: 0,
                serial_number: None,
                manufacturer: None,
                product: None,
            },
            _ => {
                return Err(String::from(
                    "authorized port is not a supported USB serial target",
                ));
            }
        };
        let serial = serialport::new(&port_path, 115_200)
            .flow_control(FlowControl::None)
            .open_native()
            .map_err(|error| error.to_string())?;
        let connection = Connection::new(
            serial,
            usb_info,
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::DefaultReset,
            115_200,
        );
        let mut flasher = Flasher::connect(connection, false, true, false, None, None)
            .map_err(|error| error.to_string())?;
        let info = flasher.security_info().map_err(|error| error.to_string())?;
        let device = flasher.device_info().map_err(|error| error.to_string())?;
        const ESP32S3_EFUSE_BLOCK1: u32 = 0x6000_7044;
        let flash_cap = (flasher
            .connection()
            .read_reg(ESP32S3_EFUSE_BLOCK1 + 12)
            .map_err(|error| error.to_string())?
            >> 27)
            & 0x07;
        let psram_cap = (flasher
            .connection()
            .read_reg(ESP32S3_EFUSE_BLOCK1 + 16)
            .map_err(|error| error.to_string())?
            >> 3)
            & 0x03;
        Ok(RomSecurityInfo {
            rom_mac: device
                .mac_address
                .ok_or_else(|| "ROM MAC is unavailable".to_string())?,
            secure_boot_enabled: info.flags & 0x1 != 0,
            flash_encryption_enabled: info.flash_crypt_cnt.count_ones() % 2 == 1,
            secure_download_mode_enabled: info.flags & 0x4 != 0 || flasher.secure_download_mode(),
            response_known: true,
            chip_is_esp32s3: flasher.chip().to_string() == "esp32s3",
            flash_size_bytes: u64::from(device.flash_size.size()),
            package_matches: flash_cap == 2 && psram_cap == 2,
        })
    })
    .await
    .map_err(|error| HttpError::internal(&format!("ROM security probe task failed: {error}")))?
    .map_err(|error| {
        HttpError::forbidden(
            "security_info_unknown",
            &format!("ROM security probe failed; flashing is blocked: {error}"),
        )
    })
}

fn firmware_preflight_stages() -> Vec<String> {
    [
        "artifact",
        "transport",
        "rom_reset",
        "chip_flash_security",
        "layout_config",
        "preflight",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn firmware_execution_stages(operation: FirmwareOperation) -> Vec<String> {
    let mut stages = vec!["authorization"];
    if operation == FirmwareOperation::InstallRecovery {
        stages.push("erase");
    }
    stages.extend([
        "write_segments",
        "rom_md5",
        "reset",
        "runtime_reconnect",
        "runtime_verify",
    ]);
    stages.into_iter().map(str::to_string).collect()
}

fn firmware_preflight_digest(
    payload: &FirmwareOperationRequest,
    device_id: &str,
    port_path: &str,
    rom_mac: &str,
    bundle_sha256: &str,
    source_partition_hash: Option<&str>,
) -> String {
    let value = json!({
        "leaseId": payload.lease_id,
        "deviceId": device_id,
        "portPath": port_path,
        "romMac": rom_mac,
        "bundleSha256": bundle_sha256,
        "sourcePartitionTableSha256": source_partition_hash,
        "operation": payload.operation,
        "allowDowngrade": payload.allow_downgrade,
    });
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&value).unwrap()))
    )
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
) -> Result<NetworkSummary, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-wifi", now_millis());
    let request = serde_json::to_string(&UsbWifiConfigWire {
        frame_type: "wifi_config",
        request_id: &request_id,
        op: payload.op.usb_op(),
        ssid: payload.ssid.as_deref(),
        password: payload.password.as_deref(),
        static_ipv4: payload.static_ipv4,
        telemetry_interval_ms: payload.telemetry_interval_ms,
    })
    .map_err(|_| HttpError::internal("failed to encode USB WiFi request"))?;
    let response = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_wifi_config_network(response, payload.op == WifiConfigOp::Cancel)
}

fn extract_wifi_config_network(
    result: Value,
    accepts_idle_cancellation: bool,
) -> Result<NetworkSummary, HttpError> {
    let receipt = extract_usb_payload::<UsbWifiConfigReceipt>(result, "wifi")?;
    if receipt.network.configuration_generation == 0 || receipt.network.transition_sequence == 0 {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_wifi_receipt",
            "The device returned an unversioned WiFi receipt.",
            true,
        ));
    }
    if matches!(
        receipt.network.state,
        NetworkState::Saving | NetworkState::Timeout
    ) || (receipt.network.state == NetworkState::Idle && !accepts_idle_cancellation)
    {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_wifi_receipt",
            "The device returned a non-public WiFi state.",
            false,
        ));
    }
    Ok(receipt.network)
}

async fn serial_clear_lan_pairing(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<(), HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-lan-reset", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op: "clear_lan_pairing_token",
    })
    .map_err(|_| HttpError::internal("failed to encode USB LAN reset request"))?;
    let _ = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    Ok(())
}

async fn serial_lan_pairing_code(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<LanPairingCode, HttpError> {
    let code = serial_request_payload::<LanPairingCode>(
        state,
        target,
        "get_lan_pairing_code",
        "lan_pairing_code",
    )
    .await?;
    validate_lan_pairing_code(code)
}

async fn serial_open_lan_pairing_window(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<LanPairingCode, HttpError> {
    let code = serial_request_payload::<LanPairingCode>(
        state,
        target,
        "open_lan_pairing_window",
        "lan_pairing_code",
    )
    .await?;
    validate_lan_pairing_code(code)
}

async fn serial_close_lan_pairing_window(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<(), HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-lan-pairing-close", now_millis());
    let request = serde_json::to_string(&UsbRequestWire {
        frame_type: "request",
        request_id: &request_id,
        op: "close_lan_pairing_window",
    })
    .map_err(|_| HttpError::internal("failed to encode USB LAN pairing-window close request"))?;
    let _ = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    Ok(())
}

fn validate_lan_pairing_code(code: LanPairingCode) -> Result<LanPairingCode, HttpError> {
    let valid_code = code
        .code
        .as_deref()
        .is_some_and(|value| value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit()));
    if (code.active && !valid_code) || (!code.active && code.code.is_some()) {
        return Err(HttpError::new(
            StatusCode::BAD_GATEWAY,
            "invalid_lan_pairing_code",
            "USB response returned an invalid LAN pairing-code state.",
            true,
        ));
    }
    Ok(code)
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
        fault_attention_acknowledged: payload.fault_attention_acknowledged,
        calibration: payload.calibration.as_ref(),
        thermal_profile_mode: payload.thermal_profile_mode.as_ref(),
        thermal_control_profile: payload.thermal_control_profile.as_ref(),
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

async fn serial_buzzer_debug(
    state: &AppState,
    target: &DeviceRecord,
    payload: &BuzzerDebugRequest,
) -> Result<BuzzerDebugStatus, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-buzzer-debug", now_millis());
    let request = serde_json::to_string(&UsbBuzzerDebugWire {
        frame_type: "buzzer_debug",
        request_id: &request_id,
        op: payload.op,
        buzzer_cue: payload.cue,
        buzzer_scenario: payload.scenario,
        repeat: payload.repeat,
    })
    .map_err(|_| HttpError::internal("failed to encode USB buzzer debug request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::SingleShot,
    )
    .await?;
    extract_usb_payload(result, "buzzer_debug")
}

async fn serial_calibration_get(
    state: &AppState,
    target: &DeviceRecord,
) -> Result<CalibrationState, HttpError> {
    let mut calibration =
        serial_request_payload::<CalibrationState>(state, target, "get_calibration", "calibration")
            .await?;
    merge_live_calibration_metadata(&mut calibration, &target.calibration);
    Ok(calibration)
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
        target_adc_mv: payload.target_adc_mv,
        observed_mv: payload.observed_mv,
        expected_mv: payload.expected_mv,
        sample_index: payload.sample_index,
        state: payload.state.as_ref(),
        slot: payload.slot,
        fit: payload.fit.as_ref(),
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
    let mut calibration = extract_usb_payload(result, "calibration")?;
    backfill_live_calibration_capture(&mut calibration, payload);
    merge_live_calibration_metadata(&mut calibration, &target.calibration);
    Ok(calibration)
}

fn backfill_live_calibration_capture(
    calibration: &mut CalibrationState,
    payload: &CalibrationConfigRequest,
) {
    if payload.op != CalibrationConfigOp::Capture
        || payload.channel != Some(CalibrationChannel::RtdAdc)
    {
        return;
    }
    let Some(sample) = calibration.rtd_adc.samples.iter_mut().flatten().last() else {
        return;
    };
    if sample.reference_temp_c.is_none() {
        sample.reference_temp_c = payload.reference_temp_c;
    }
    if sample.target_adc_mv.is_none() {
        sample.target_adc_mv = payload.target_adc_mv;
    }
    if let Some(target_adc_mv) = payload.target_adc_mv {
        sample.expected_mv = target_adc_mv;
    }
    calibration.rtd_adc.refresh(CalibrationChannel::RtdAdc);
}

fn merge_live_calibration_metadata(
    calibration: &mut CalibrationState,
    previous: &CalibrationState,
) {
    merge_live_rtd_sample_metadata(&mut calibration.rtd_adc.samples, &previous.rtd_adc.samples);
    calibration.refresh_fits();
}

fn merge_live_rtd_sample_metadata(
    samples: &mut [Option<CalibrationSample>],
    previous: &[Option<CalibrationSample>],
) {
    for sample in samples.iter_mut().flatten() {
        if sample.reference_temp_c.is_some() && sample.target_adc_mv.is_some() {
            continue;
        }
        let Some(existing) = previous.iter().flatten().find(|existing| {
            existing.observed_mv == sample.observed_mv && existing.expected_mv == sample.expected_mv
        }) else {
            continue;
        };
        if sample.reference_temp_c.is_none() {
            sample.reference_temp_c = existing.reference_temp_c;
        }
        if sample.target_adc_mv.is_none() {
            sample.target_adc_mv = existing.target_adc_mv;
        }
    }
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

async fn serial_thermal_plant_run_get(
    state: &AppState,
    target: &DeviceRecord,
    after_sample: u8,
) -> Result<ThermalPlantRunSnapshot, HttpError> {
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-thermal-plant-run", now_millis());
    let request = serde_json::to_string(&UsbThermalPlantRunWire {
        frame_type: "thermal_plant_run",
        request_id: &request_id,
        after_sample,
    })
    .map_err(|_| HttpError::internal("failed to encode thermal plant run request"))?;
    let result = serial_exchange(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        SerialRetryPolicy::ReadOnly,
    )
    .await?;
    extract_usb_payload(result, "thermal_plant_run")
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

const EEPROM_CAPACITY_BYTES: usize = 8 * 1024;
const EEPROM_MAINTENANCE_CHUNK_MAX: usize = 32;

fn validate_eeprom_maintenance_request(
    payload: &EepromMaintenanceRequest,
) -> Result<(), HttpError> {
    match payload.op {
        EepromMaintenanceOp::Read => {
            let (Some(offset), Some(length)) = (payload.offset, payload.length) else {
                return Err(HttpError::bad_request(
                    "eeprom_range_required",
                    "EEPROM read requires offset and length.",
                ));
            };
            if length == 0
                || usize::from(length) > EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + usize::from(length) > EEPROM_CAPACITY_BYTES
                || payload.bytes.is_some()
            {
                return Err(HttpError::bad_request(
                    "eeprom_range_invalid",
                    "EEPROM read range is invalid.",
                ));
            }
        }
        EepromMaintenanceOp::Write => {
            let (Some(offset), Some(bytes)) = (payload.offset, payload.bytes.as_ref()) else {
                return Err(HttpError::bad_request(
                    "eeprom_write_required",
                    "EEPROM write requires offset and bytes.",
                ));
            };
            if bytes.is_empty()
                || bytes.len() > EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + bytes.len() > EEPROM_CAPACITY_BYTES
                || payload.length.is_some()
            {
                return Err(HttpError::bad_request(
                    "eeprom_range_invalid",
                    "EEPROM write range is invalid.",
                ));
            }
        }
        EepromMaintenanceOp::Erase => {
            if payload.offset.is_some() || payload.length.is_some() || payload.bytes.is_some() {
                return Err(HttpError::bad_request(
                    "eeprom_erase_payload_invalid",
                    "EEPROM erase does not accept a range or content.",
                ));
            }
        }
    }
    Ok(())
}

async fn serial_eeprom_maintenance(
    state: &AppState,
    target: &DeviceRecord,
    payload: &EepromMaintenanceRequest,
) -> Result<EepromMaintenanceResponse, HttpError> {
    validate_eeprom_maintenance_request(payload)?;
    let port_path = native_port_path(target)?;
    let request_id = format!("devd-{}-eeprom", now_millis());
    let request = serde_json::to_string(&UsbEepromMaintenanceWire {
        frame_type: "eeprom_maintenance",
        request_id: &request_id,
        op: payload.op,
        offset: payload.offset,
        length: payload.length,
        bytes: payload.bytes.as_ref(),
    })
    .map_err(|_| HttpError::internal("failed to encode EEPROM maintenance request"))?;
    let result = serial_exchange_sensitive(
        state,
        &target.id,
        port_path,
        request_id,
        request,
        match payload.op {
            EepromMaintenanceOp::Read => SerialRetryPolicy::ReadOnly,
            EepromMaintenanceOp::Write | EepromMaintenanceOp::Erase => {
                SerialRetryPolicy::SingleShot
            }
        },
    )
    .await?;
    let bytes = if payload.op == EepromMaintenanceOp::Read {
        Some(extract_usb_payload(result, "eeprom_bytes")?)
    } else {
        None
    };
    Ok(EepromMaintenanceResponse { bytes })
}

async fn configure_eeprom_maintenance(
    State(state): State<AppState>,
    AxumPath(device_id): AxumPath<String>,
    Json(payload): Json<EepromMaintenanceRequest>,
) -> Result<Json<EepromMaintenanceResponse>, HttpError> {
    let target = {
        let mut state_lock = state.lock()?;
        state_lock.require_lease(&device_id, Some(&payload.lease_id))?;
        state_lock
            .devices
            .get(&device_id)
            .ok_or_else(|| HttpError::not_found("device_not_found", "Device not found."))?
            .clone()
    };
    if target.transport != DeviceTransport::NativeSerial {
        return Err(HttpError::bad_request(
            "native_serial_required",
            "EEPROM maintenance requires native USB serial transport.",
        ));
    }
    serial_eeprom_maintenance(&state, &target, &payload)
        .await
        .map(Json)
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
    serial_exchange_with_visibility(
        state,
        device_id,
        port_path,
        request_id,
        request,
        retry_policy,
        true,
    )
    .await
}

async fn serial_exchange_sensitive(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
) -> Result<Value, HttpError> {
    serial_exchange_with_visibility(
        state,
        device_id,
        port_path,
        request_id,
        request,
        retry_policy,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn serial_exchange_with_visibility(
    state: &AppState,
    device_id: &str,
    port_path: String,
    request_id: String,
    request: String,
    retry_policy: SerialRetryPolicy,
    record_payload: bool,
) -> Result<Value, HttpError> {
    if record_payload {
        record_transport_event(state, device_id, "tx", "usb_jsonl", &request_id, &request);
    }
    let serial_sessions = state.serial_sessions.clone();
    let worker_request_id = request_id.clone();
    let worker_device_id = device_id.to_string();
    let worker_events = state.events.clone();
    let worker_inner = state.inner.clone();
    let result = spawn_serial_worker(state.serial_rpc.clone(), move || {
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
    .await?;

    if !record_payload {
        return result;
    }

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

async fn spawn_serial_worker<T, F>(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    worker: F,
) -> Result<T, HttpError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    spawn_serial_worker_with_timeout(serial_rpc, SERIAL_RPC_TIMEOUT, worker).await
}

async fn spawn_serial_worker_with_timeout<T, F>(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    lock_timeout: Duration,
    worker: F,
) -> Result<T, HttpError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let serial_rpc = acquire_serial_rpc_with_timeout(serial_rpc, lock_timeout).await?;
    tokio::task::spawn_blocking(move || {
        let _serial_rpc = serial_rpc;
        worker()
    })
    .await
    .map_err(|_| HttpError::internal("serial worker failed"))
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
    serde_json::from_value(payload).map_err(|error| HttpError {
        status: StatusCode::BAD_GATEWAY,
        error: ApiError {
            code: "usb_payload_decode_failed".to_string(),
            message: "USB response payload could not be decoded.".to_string(),
            retryable: true,
            details: Some(json!({
                "payloadKey": payload_key,
                "decodeError": error.to_string(),
            })),
        },
    })
}

#[allow(clippy::too_many_arguments)]
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
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let deadline = Instant::now() + serial_rpc_timeout(retry_policy);
    let mut session = take_or_open_serial_session(&mut serial_sessions, port_path, deadline)?;
    session = write_serial_request_with_reopen(session, port_path, request, deadline)?;

    // A USB Serial/JTAG port may reset the target as it opens. Do not keep
    // resending a JSONL command while the runtime is starting: the firmware
    // will later process every queued duplicate and pollute the following RPC.
    // `startup_busy` is the one explicit signal that a status request needs a
    // retry after the observable runtime-ready marker.
    let mut retry_after_runtime_ready = false;
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();
    let mut discarding_overlong_line = false;

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => std::thread::sleep(SERIAL_READ_TIMEOUT),
            Ok(read) => {
                for byte in &read_buf[..read] {
                    if serial_line_finished(&mut line, &mut discarding_overlong_line, *byte) {
                        emit_serial_log_line(state, events, device_id, &line);
                        if serial_line_is_usb_reset_marker(&line) {
                            // Opening an ESP32-S3 USB Serial/JTAG port can itself
                            // trigger this reset. Keep the fd open so the firmware
                            // can complete its startup and answer the request; an
                            // actual I/O failure below remains the reopen signal.
                            retry_after_runtime_ready = true;
                        } else if should_retry_request_after_runtime_ready(
                            retry_after_runtime_ready,
                            &line,
                            Instant::now(),
                            deadline,
                        ) {
                            session = write_serial_request_with_reopen(
                                session, port_path, request, deadline,
                            )?;
                            retry_after_runtime_ready = false;
                        } else {
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
                                    // The request is known not to have run. Wait for the
                                    // runtime-ready marker before sending its one safe retry.
                                    retry_after_runtime_ready = true;
                                }
                                Err(error) => {
                                    store_serial_session(&mut serial_sessions, port_path, session);
                                    return Err(error);
                                }
                            }
                        }
                        line.clear();
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if error.kind() == io::ErrorKind::WouldBlock {
                    std::thread::sleep(SERIAL_READ_TIMEOUT);
                }
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                session = write_serial_request_with_reopen(session, port_path, request, deadline)?;
                retry_after_runtime_ready = false;
                line.clear();
                discarding_overlong_line = false;
            }
            Err(error) => return Err(serial_io_http_error(error)),
        }
    }

    // Keep the USB-JTAG session open after a timeout. Reopening this class of
    // port can reset the MCU; JSONL newline framing and request-id matching let
    // the next RPC discard any stale partial response without reopening it.
    store_serial_session(&mut serial_sessions, port_path, session);
    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "usb_response_timeout",
        "Timed out waiting for a matching USB JSONL response.",
        true,
    ))
}

fn observe_post_flash_boot_blocking(
    state: &Arc<Mutex<DevdState>>,
    events: &broadcast::Sender<DevdEvent>,
    device_id: &str,
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<BootObservation, HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    let deadline = Instant::now() + POST_FLASH_BOOT_TIMEOUT;
    let mut session = reopen_serial_session(port_path, deadline)?;
    let mut observation = BootObservation::default();
    let mut read_buf = [0_u8; 256];
    let mut line = Vec::new();
    let mut discarding_overlong_line = false;

    while Instant::now() < deadline {
        match session.port.read(&mut read_buf) {
            Ok(0) => {}
            Ok(read) => {
                for byte in &read_buf[..read] {
                    if !serial_line_finished(&mut line, &mut discarding_overlong_line, *byte) {
                        continue;
                    }
                    emit_serial_log_line(state, events, device_id, &line);
                    if let Ok(text) = std::str::from_utf8(&line) {
                        if observation.observe_line(text)? {
                            store_serial_session(&mut serial_sessions, port_path, session);
                            return Ok(observation);
                        }
                    }
                    line.clear();
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if error.kind() == io::ErrorKind::WouldBlock {
                    std::thread::sleep(SERIAL_READ_TIMEOUT);
                }
            }
            Err(error) if is_recoverable_serial_io_error(&error) => {
                drop(session);
                session = reopen_serial_session(port_path, deadline)?;
                line.clear();
                discarding_overlong_line = false;
            }
            Err(error) => return Err(serial_io_http_error(error)),
        }
    }

    Err(HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "firmware_boot_timeout",
        &format!(
            "Firmware did not reach runtime_ready within {} seconds; last stage: {}.",
            POST_FLASH_BOOT_TIMEOUT.as_secs(),
            observation.last_stage.as_deref().unwrap_or("none")
        ),
        false,
    ))
}

async fn observe_post_flash_boot(
    state: &AppState,
    device_id: &str,
    port_path: &str,
) -> Result<BootObservation, HttpError> {
    let state_lock = state.inner.clone();
    let events = state.events.clone();
    let device_id = device_id.to_string();
    let serial_sessions = state.serial_sessions.clone();
    let port_path = port_path.to_string();
    spawn_serial_worker_with_timeout(
        state.serial_rpc.clone(),
        POST_FLASH_BOOT_TIMEOUT + SERIAL_RPC_TIMEOUT,
        move || {
            observe_post_flash_boot_blocking(
                &state_lock,
                &events,
                &device_id,
                &serial_sessions,
                &port_path,
            )
        },
    )
    .await?
}

fn serial_rpc_timeout(retry_policy: SerialRetryPolicy) -> Duration {
    match retry_policy {
        SerialRetryPolicy::ReadOnly => SERIAL_READ_ONLY_RPC_TIMEOUT,
        SerialRetryPolicy::SingleShot => SERIAL_RPC_TIMEOUT,
    }
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

fn serial_line_is_usb_reset_marker(line: &[u8]) -> bool {
    matches!(
        std::str::from_utf8(line).map(str::trim),
        Ok("reset_reason=core_usb_uart" | "reset_reason=core_usb_jtag")
    )
}

fn serial_line_is_runtime_ready(line: &[u8]) -> bool {
    matches!(
        std::str::from_utf8(line).map(str::trim),
        Ok(RUNTIME_READY_BOOT_STAGE)
    )
}

fn serial_line_finished(line: &mut Vec<u8>, discarding_overlong_line: &mut bool, byte: u8) -> bool {
    if byte == b'\n' {
        if *discarding_overlong_line {
            *discarding_overlong_line = false;
            line.clear();
            return false;
        }
        return true;
    }
    if !*discarding_overlong_line {
        if line.len() < SERIAL_LINE_LIMIT {
            line.push(byte);
        } else {
            line.clear();
            *discarding_overlong_line = true;
        }
    }
    false
}

type SerialSessionMap = HashMap<String, SerialSession>;

struct SerialSession {
    _serial_lock: SerialPortProcessLock,
    port: Box<dyn SerialSessionPort>,
}

trait SerialSessionPort: Read + Write + Send {
    fn begin_write(&mut self) -> Result<(), HttpError>;
    fn finish_write(&mut self) -> Result<(), HttpError>;
}

impl SerialSessionPort for Box<dyn serialport::SerialPort> {
    fn begin_write(&mut self) -> Result<(), HttpError> {
        self.set_timeout(SERIAL_WRITE_TIMEOUT)
            .map_err(serial_timeout_config_http_error)
    }

    fn finish_write(&mut self) -> Result<(), HttpError> {
        self.set_timeout(SERIAL_READ_TIMEOUT)
            .map_err(serial_timeout_config_http_error)
    }
}

#[cfg(target_os = "macos")]
struct RawUsbSerialJtagPort {
    file: File,
}

#[cfg(target_os = "macos")]
impl Read for RawUsbSerialJtagPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

#[cfg(target_os = "macos")]
impl Write for RawUsbSerialJtagPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(target_os = "macos")]
impl SerialSessionPort for RawUsbSerialJtagPort {
    fn begin_write(&mut self) -> Result<(), HttpError> {
        Ok(())
    }

    fn finish_write(&mut self) -> Result<(), HttpError> {
        Ok(())
    }
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
                .truncate(false)
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

fn is_esp_usb_serial_jtag_port(port_path: &str) -> bool {
    port_path.starts_with("/dev/cu.usbmodem")
}

fn open_serial_port(port_path: &str) -> Result<Box<dyn SerialSessionPort>, HttpError> {
    #[cfg(target_os = "macos")]
    if is_esp_usb_serial_jtag_port(port_path) {
        let file = File::options()
            .read(true)
            .write(true)
            .custom_flags(MACOS_O_NONBLOCK)
            .open(port_path)
            .map_err(|error| {
                HttpError::new(
                    StatusCode::BAD_GATEWAY,
                    "serial_open_failed",
                    &format!("Failed to open serial port: {error}"),
                    true,
                )
            })?;
        return Ok(Box::new(RawUsbSerialJtagPort { file }));
    }

    // USB Serial/JTAG does not use modem-control lines.  Explicit DTR/RTS writes
    // can reset an attached MCU, so leave those lines entirely to the driver.
    serialport::new(port_path, DEFAULT_BAUD_RATE)
        .timeout(SERIAL_READ_TIMEOUT)
        .open()
        .map(|port| Box::new(port) as Box<dyn SerialSessionPort>)
        .map_err(|error| {
            HttpError::new(
                StatusCode::BAD_GATEWAY,
                "serial_open_failed",
                &format!("Failed to open serial port: {error}"),
                true,
            )
        })
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

fn write_serial_request(port: &mut dyn SerialSessionPort, request: &str) -> Result<(), HttpError> {
    validate_serial_request_len(request)?;
    port.begin_write()?;
    let write_result = port
        .write_all(request.as_bytes())
        .and_then(|_| port.write_all(b"\n"))
        .and_then(|_| port.flush());
    let restore_result = port.finish_write();
    write_result.map_err(serial_io_http_error)?;
    restore_result
}

fn validate_serial_request_len(request: &str) -> Result<(), HttpError> {
    if request.len().saturating_add(1) > SERIAL_LINE_LIMIT {
        return Err(HttpError::bad_request(
            "usb_request_too_large",
            "USB JSONL request exceeds the firmware line limit.",
        ));
    }
    Ok(())
}

fn serial_timeout_config_http_error(error: serialport::Error) -> HttpError {
    HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_timeout_config_failed",
        &format!("Failed to configure serial timeout: {error}"),
        true,
    )
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

fn should_retry_request_after_runtime_ready(
    retry_pending: bool,
    line: &[u8],
    now: Instant,
    deadline: Instant,
) -> bool {
    retry_pending && now < deadline && serial_line_is_runtime_ready(line)
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
        .thermal_profile_mode
        .as_deref()
        .is_some_and(|mode| status.thermal_profile_mode != mode)
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
    if payload.fault_attention_acknowledged == Some(true) && status.fault_attention_pending {
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
    if let Some(profile) = payload.thermal_control_profile.as_ref() {
        match profile.op {
            ThermalControlProfileOp::Preview => {
                let expected =
                    mock_thermal_runtime(status.target_temp_c, profile.profile.as_ref(), true);
                if !status.thermal_control_profile_preview || status.thermal_control != expected {
                    return false;
                }
            }
            ThermalControlProfileOp::ClearPreview => {
                if status.thermal_control_profile_preview
                    || status.thermal_control.profile_source == "preview"
                {
                    return false;
                }
            }
            ThermalControlProfileOp::Save => {
                let expected =
                    mock_thermal_runtime(status.target_temp_c, profile.profile.as_ref(), false);
                if status.thermal_control_profile_preview
                    || status.thermal_control.profile_source != "saved"
                    || status.thermal_control != expected
                {
                    return false;
                }
            }
            ThermalControlProfileOp::ClearSaved => {
                if status.thermal_control.profile_source == "saved" {
                    return false;
                }
            }
        }
    }
    true
}

fn decode_usb_response_line(line: &[u8], request_id: &str) -> Result<Option<Value>, HttpError> {
    const FRAME_PREFIX: &[u8] = br#"{"type":"#;
    for (offset, candidate) in line.windows(FRAME_PREFIX.len()).enumerate() {
        if candidate != FRAME_PREFIX {
            continue;
        }
        let mut frames =
            serde_json::Deserializer::from_slice(&line[offset..]).into_iter::<UsbResponseWire>();
        let Some(Ok(frame)) = frames.next() else {
            continue;
        };
        if let Some(payload) = decode_usb_response_frame(frame, request_id)? {
            return Ok(Some(payload));
        }
    }
    Ok(None)
}

fn decode_usb_response_frame(
    frame: UsbResponseWire,
    request_id: &str,
) -> Result<Option<Value>, HttpError> {
    if frame.frame_type == "error" && frame.request_id.as_deref() == Some(request_id) {
        return Err(HttpError {
            status: StatusCode::BAD_GATEWAY,
            error: frame.error.unwrap_or_else(|| ApiError {
                code: "usb_error".to_string(),
                message: "Firmware returned an unsuccessful USB error frame.".to_string(),
                retryable: true,
                details: None,
            }),
        });
    }
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
            DeviceTransport::Lan => {
                return Err(HttpError::bad_request(
                    "lan_flash_unsupported",
                    "Firmware flashing is unavailable through the DEVD LAN bridge.",
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
    state.emit(event(
        &device_id,
        "flash",
        "firmware boot observation started",
        json!({ "artifactId": artifact_id }),
    ));
    let boot = match observe_post_flash_boot(&state, &device_id, &port_path).await {
        Ok(boot) => boot,
        Err(error) => {
            state.emit(event(
                &device_id,
                "flash",
                "firmware boot verification failed",
                json!({
                    "artifactId": artifact_id,
                    "code": error.error.code,
                    "message": error.error.message,
                }),
            ));
            return Err(error);
        }
    };
    {
        let mut state_lock = state.lock()?;
        if let Some(device) = state_lock.devices.get_mut(&device_id) {
            device.selected_artifact_id = Some(artifact_id.clone());
        }
    }
    state.emit(event(
        &device_id,
        "flash",
        "real flash completed and firmware reached runtime_ready",
        json!({
            "artifactId": artifact_id,
            "dryRun": false,
            "resetCount": boot.reset_count,
            "lastStage": boot.last_stage,
        }),
    ));
    Ok(Json(FlashResult {
        artifact_id,
        dry_run: false,
        status: "completed".to_string(),
        message: "espflash completed and firmware reached runtime_ready.".to_string(),
    }))
}

pub fn scan_serial_devices(serial_port: Option<&Path>) -> Vec<DeviceRecord> {
    let available_ports = serialport::available_ports().ok().unwrap_or_default();
    scan_serial_devices_from_available(serial_port, &available_ports)
}

fn scan_serial_devices_from_available(
    serial_port: Option<&Path>,
    available_ports: &[serialport::SerialPortInfo],
) -> Vec<DeviceRecord> {
    let Some(serial_port) = serial_port else {
        return available_ports
            .iter()
            .filter(|port| is_flux_purr_usb_candidate(port))
            .map(|port| serial_device_record(&port.port_name, Some(port)))
            .collect();
    };
    let port_name = serial_port.to_string_lossy().into_owned();
    if !serial_port.exists() {
        return vec![missing_serial_device_record(&port_name, available_ports)];
    }

    let port_info = available_ports
        .iter()
        .find(|port| port.port_name == port_name);
    vec![serial_device_record(&port_name, port_info)]
}

fn is_flux_purr_usb_candidate(port: &serialport::SerialPortInfo) -> bool {
    port.port_name.starts_with("/dev/cu.usbmodem")
        || matches!(
            &port.port_type,
            serialport::SerialPortType::UsbPort(info) if info.vid == 0x303a
        )
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
            "local-esp32s3-release-buzzer-debug",
            "Local ESP32-S3 release (buzzer debug)",
            "firmware/target/buzzer-debug/xtensa-esp32s3-none-elf/release/flux-purr",
            "release + web_serial + net_http + buzzer-debug",
            vec![
                "web_serial".to_string(),
                "net_http".to_string(),
                "buzzer-debug".to_string(),
            ],
            "elf",
        ),
        (
            "local-esp32s3-release",
            "Local ESP32-S3 release",
            "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
            "release + web_serial + net_http",
            vec!["web_serial".to_string(), "net_http".to_string()],
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

async fn run_espflash_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        build_espflash_args_with_reset_mode(artifact, root, port_path, before_reset)
    })
    .await
}

async fn run_espflash_with_reset_fallback_with_program<F>(
    program: &Path,
    artifact: &FirmwareArtifact,
    port_path: &str,
    build_commands: F,
) -> Result<(), HttpError>
where
    F: Fn(&str) -> Result<Vec<Vec<String>>, HttpError>,
{
    let reset_modes = espflash_reset_modes(artifact, port_path);
    for (mode_index, before_reset) in reset_modes.iter().enumerate() {
        let commands = build_commands(before_reset)?;
        let mut retry_with_next_reset = false;

        for args in commands {
            let output =
                run_espflash_command_with_timeout(program, &args, ESPFLASH_COMMAND_TIMEOUT).await?;

            if !output.status.success() {
                if espflash_flash_end_requires_reset(&args, &output) {
                    let reset_args = build_espflash_reset_args(artifact, port_path, before_reset)?;
                    let reset_output = run_espflash_command_with_timeout(
                        program,
                        &reset_args,
                        ESPFLASH_COMMAND_TIMEOUT,
                    )
                    .await?;

                    if reset_output.status.success() {
                        // The ROM accepted the image data but rejected the final
                        // run-user-code transition. Reset once, then let the caller
                        // require the normal runtime-ready verification.
                        return Ok(());
                    }

                    return Err(HttpError::internal_with_details(
                        "flash_recovery_reset_failed",
                        "espflash reached FlashEnd but the recovery reset failed.",
                        json!({
                            "flashAttempt": espflash_failure_details(program, &args, &output),
                            "resetAttempt": espflash_failure_details(program, &reset_args, &reset_output),
                        }),
                    ));
                }
                retry_with_next_reset =
                    mode_index + 1 < reset_modes.len() && espflash_connection_failed(&output);
                if retry_with_next_reset {
                    if is_esp_usb_serial_jtag_port(port_path) {
                        tokio::time::sleep(ESPFLASH_USB_RESET_RETRY_DELAY).await;
                    }
                    break;
                }
                return Err(HttpError::internal_with_details(
                    "flash_tool_failed",
                    "espflash returned a non-zero status.",
                    espflash_failure_details(program, &args, &output),
                ));
            }
        }

        if !retry_with_next_reset {
            return Ok(());
        }
    }

    Err(HttpError::internal(
        "espflash did not complete a reset attempt.",
    ))
}

async fn run_espflash_command_with_timeout(
    program: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Output, HttpError> {
    let mut command = Command::new(program);
    command.args(args).kill_on_drop(true);
    tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            HttpError::internal_with_details(
                "flash_tool_timeout",
                "espflash did not finish before the command deadline.",
                json!({
                    "program": program,
                    "args": args,
                    "timeoutMs": timeout.as_millis(),
                }),
            )
        })?
        .map_err(|error| {
            HttpError::internal_with_details(
                "flash_tool_unavailable",
                "Failed to start espflash.",
                json!({
                    "program": program,
                    "error": error.to_string(),
                }),
            )
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashPartitionRange {
    label: String,
    offset: u64,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashConfigMigrationPlan {
    source: FlashPartitionRange,
    destination: FlashPartitionRange,
}

struct FlashConfigStaging {
    _workspace: tempfile::TempDir,
    plan: FlashConfigMigrationPlan,
    source_path: PathBuf,
}

fn flash_config_restore_required(plan: &FlashConfigMigrationPlan) -> bool {
    plan.source.offset != plan.destination.offset || plan.source.size != plan.destination.size
}

fn flash_partition_ranges(table: &esp_idf_part::PartitionTable) -> Vec<FlashPartitionRange> {
    table
        .partitions()
        .iter()
        .map(|partition| FlashPartitionRange {
            label: partition.name(),
            offset: u64::from(partition.offset()),
            size: u64::from(partition.size()),
        })
        .collect()
}

fn ranges_overlap(left: &FlashPartitionRange, right: &FlashPartitionRange) -> bool {
    left.offset < right.offset.saturating_add(right.size)
        && right.offset < left.offset.saturating_add(left.size)
}

fn range_contains(container: &FlashPartitionRange, value: &FlashPartitionRange) -> bool {
    container.offset <= value.offset
        && value.offset.saturating_add(value.size)
            <= container.offset.saturating_add(container.size)
}

fn flash_partition_by_label<'a>(
    partitions: &'a [FlashPartitionRange],
    label: &str,
) -> Option<&'a FlashPartitionRange> {
    partitions.iter().find(|partition| partition.label == label)
}

fn plan_flash_config_migration(
    current: &[FlashPartitionRange],
    target: &[FlashPartitionRange],
) -> Result<Option<FlashConfigMigrationPlan>, HttpError> {
    let destination = flash_partition_by_label(target, FLASH_CONFIG_LABEL)
        .cloned()
        .ok_or_else(|| {
            HttpError::bad_request(
                "flash_config_destination_missing",
                "The firmware partition table does not declare flux_cfg.",
            )
        })?;
    if destination.size < FLASH_CONFIG_MIN_SIZE {
        return Err(HttpError::bad_request(
            "flash_config_destination_too_small",
            "The target flux_cfg partition is too small to preserve device configuration.",
        ));
    }

    let source = match flash_partition_by_label(current, FLASH_CONFIG_LABEL).cloned() {
        Some(source) => {
            if source.size < FLASH_CONFIG_MIN_SIZE {
                return Err(HttpError::bad_request(
                    "flash_config_source_too_small",
                    "The current flux_cfg partition is too small to preserve device configuration.",
                ));
            }
            source
        }
        None => {
            let legacy = FlashPartitionRange {
                label: "legacy_raw".to_string(),
                offset: LEGACY_FLASH_CONFIG_OFFSET,
                size: LEGACY_FLASH_CONFIG_SIZE,
            };
            let legacy_is_outside_current_layout = !current
                .iter()
                .any(|partition| ranges_overlap(partition, &legacy));
            let legacy_is_inside_old_factory = current.iter().any(|partition| {
                partition.label == "factory" && range_contains(partition, &legacy)
            });
            if !legacy_is_outside_current_layout && !legacy_is_inside_old_factory {
                return Ok(None);
            }
            legacy
        }
    };

    if source.size > destination.size {
        return Err(HttpError::bad_request(
            "flash_config_destination_too_small",
            "The target flux_cfg partition is too small to preserve device configuration.",
        ));
    }
    if source.offset != destination.offset
        && current
            .iter()
            .any(|partition| ranges_overlap(partition, &destination))
    {
        return Err(HttpError::bad_request(
            "flash_config_destination_in_use",
            "The target flux_cfg address is used by the current device layout; refusing to flash.",
        ));
    }

    Ok(Some(FlashConfigMigrationPlan {
        source,
        destination,
    }))
}

fn parse_flash_partition_table(
    bytes: Vec<u8>,
    code: &str,
    message: &str,
) -> Result<Vec<FlashPartitionRange>, HttpError> {
    if bytes.len() < 2 {
        return Err(HttpError::bad_request(code, message));
    }
    let table = esp_idf_part::PartitionTable::try_from(bytes)
        .map_err(|_| HttpError::bad_request(code, message))?;
    Ok(flash_partition_ranges(&table))
}

fn target_flash_partition_ranges(
    root: Option<&Path>,
) -> Result<Vec<FlashPartitionRange>, HttpError> {
    let path = firmware_partition_table_path(root)?;
    let bytes = fs::read(path).map_err(|_| {
        HttpError::bad_request(
            "firmware_partition_table_invalid",
            "Unable to read firmware/partitions.csv.",
        )
    })?;
    parse_flash_partition_table(
        bytes,
        "firmware_partition_table_invalid",
        "The firmware partition table is invalid.",
    )
}

fn flash_config_staging_path(workspace: &Path, name: &str) -> PathBuf {
    workspace.join(name)
}

fn build_espflash_read_flash_args(
    artifact: &FirmwareArtifact,
    port_path: &str,
    before_reset: &str,
    address: u64,
    size: u64,
    output_path: &Path,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    Ok(vec![
        "read-flash".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--before".to_string(),
        before_reset.to_string(),
        "--non-interactive".to_string(),
        "--no-stub".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        format!("0x{size:x}"),
        output_path.to_string_lossy().into_owned(),
    ])
}

fn build_espflash_write_bin_args(
    artifact: &FirmwareArtifact,
    port_path: &str,
    before_reset: &str,
    address: u64,
    input_path: &Path,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    Ok(vec![
        "write-bin".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--before".to_string(),
        before_reset.to_string(),
        "--non-interactive".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        input_path.to_string_lossy().into_owned(),
    ])
}

fn build_espflash_reset_args(
    artifact: &FirmwareArtifact,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<String>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    Ok(vec![
        "reset".to_string(),
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "hard-reset".to_string(),
        "--non-interactive".to_string(),
    ])
}

fn build_bundle_read_flash_args(
    common: &[String],
    before_reset: &str,
    address: u64,
    size: u64,
    output_path: &Path,
) -> Vec<String> {
    let mut args = vec!["read-flash".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--no-stub".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        format!("0x{size:x}"),
        output_path.to_string_lossy().into_owned(),
    ]);
    args
}

fn build_bundle_write_bin_args(
    common: &[String],
    before_reset: &str,
    address: u64,
    input_path: &Path,
) -> Vec<String> {
    let mut args = vec!["write-bin".to_string()];
    args.extend(common.iter().cloned());
    args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        format!("0x{address:x}"),
        input_path.to_string_lossy().into_owned(),
    ]);
    args
}

async fn stage_flash_config_before_app_flash_with_program(
    program: &Path,
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<Option<FlashConfigStaging>, HttpError> {
    let workspace = tempfile::tempdir().map_err(|_| {
        HttpError::internal_with_details(
            "flash_config_workspace_unavailable",
            "Unable to create secure temporary storage for configuration preservation.",
            json!({}),
        )
    })?;
    let current_table_path = flash_config_staging_path(workspace.path(), "current-partitions.bin");
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        Ok(vec![build_espflash_read_flash_args(
            artifact,
            port_path,
            before_reset,
            DEFAULT_PARTITION_TABLE_FLASH_ADDRESS,
            PARTITION_TABLE_FLASH_SIZE,
            &current_table_path,
        )?])
    })
    .await?;

    let current_table = fs::read(&current_table_path).map_err(|_| {
        HttpError::bad_request(
            "flash_partition_table_unreadable",
            "Unable to read the current device partition table; refusing to flash.",
        )
    })?;
    let current = parse_flash_partition_table(
        current_table,
        "flash_partition_table_invalid",
        "The current device partition table is invalid; refusing to flash.",
    )?;
    let target = target_flash_partition_ranges(root)?;
    let Some(plan) = plan_flash_config_migration(&current, &target)? else {
        return Ok(None);
    };

    let source_path = flash_config_staging_path(workspace.path(), "flux_cfg-source.bin");
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        Ok(vec![build_espflash_read_flash_args(
            artifact,
            port_path,
            before_reset,
            plan.source.offset,
            plan.source.size,
            &source_path,
        )?])
    })
    .await?;
    let source = fs::read(&source_path).map_err(|_| {
        HttpError::bad_request(
            "flash_config_backup_unreadable",
            "Unable to read the current device configuration; refusing to flash.",
        )
    })?;
    if source.len() != usize::try_from(plan.source.size).unwrap_or(usize::MAX) {
        return Err(HttpError::bad_request(
            "flash_config_backup_incomplete",
            "The current device configuration could not be read completely; refusing to flash.",
        ));
    }

    Ok(Some(FlashConfigStaging {
        _workspace: workspace,
        plan,
        source_path,
    }))
}

async fn restore_flash_config_after_app_flash_with_program(
    program: &Path,
    artifact: &FirmwareArtifact,
    port_path: &str,
    staging: &FlashConfigStaging,
) -> Result<(), HttpError> {
    // The app image does not overlap flux_cfg when the partition range is unchanged.
    if !flash_config_restore_required(&staging.plan) {
        return Ok(());
    }
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        Ok(vec![build_espflash_write_bin_args(
            artifact,
            port_path,
            before_reset,
            staging.plan.destination.offset,
            &staging.source_path,
        )?])
    })
    .await?;

    let verification_path =
        flash_config_staging_path(staging._workspace.path(), "flux_cfg-verify-after-flash.bin");
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        Ok(vec![build_espflash_read_flash_args(
            artifact,
            port_path,
            before_reset,
            staging.plan.destination.offset,
            staging.plan.source.size,
            &verification_path,
        )?])
    })
    .await?;

    let source = fs::read(&staging.source_path).map_err(|_| {
        HttpError::internal_with_details(
            "flash_config_backup_unreadable",
            "Unable to read the preserved device configuration.",
            json!({}),
        )
    })?;
    let verification = fs::read(&verification_path).map_err(|_| {
        HttpError::internal_with_details(
            "flash_config_verify_unreadable",
            "Unable to verify the restored device configuration.",
            json!({}),
        )
    })?;
    if verification != source {
        return Err(HttpError::bad_request(
            "flash_config_verify_failed",
            "Device configuration restoration could not be verified.",
        ));
    }
    run_espflash_with_reset_fallback_with_program(program, artifact, port_path, |before_reset| {
        Ok(vec![build_espflash_reset_args(
            artifact,
            port_path,
            before_reset,
        )?])
    })
    .await?;
    Ok(())
}

async fn run_flash_transaction_with_program(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    program: &Path,
) -> Result<(), HttpError> {
    let staging =
        stage_flash_config_before_app_flash_with_program(program, artifact, root, port_path)
            .await?;
    run_espflash_with_program(artifact, root, port_path, program).await?;
    if let Some(staging) = staging.as_ref() {
        restore_flash_config_after_app_flash_with_program(program, artifact, port_path, staging)
            .await?;
    }
    Ok(())
}

fn resolve_espflash_program() -> PathBuf {
    if let Some(program) = env::var_os("FLUX_PURR_ESPFLASH").filter(|value| !value.is_empty()) {
        return PathBuf::from(program);
    }
    if let Some(cargo_home) = env::var_os("CARGO_HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(cargo_home).join("bin").join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join("espflash");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("espflash")
}

fn espflash_failure_details(program: &Path, args: &[String], output: &Output) -> Value {
    json!({
        "program": program,
        "args": args,
        "exitCode": output.status.code(),
        "stdout": bounded_espflash_output(&output.stdout),
        "stderr": bounded_espflash_output(&output.stderr),
    })
}

fn espflash_connection_failed(output: &Output) -> bool {
    espflash_connection_failure_text(&bounded_espflash_output(&output.stderr))
        || espflash_connection_failure_text(&bounded_espflash_output(&output.stdout))
}

fn espflash_flash_end_requires_reset(args: &[String], output: &Output) -> bool {
    args.first().map(String::as_str) == Some("flash")
        && bounded_espflash_output(&output.stderr).contains("Error while running FlashEnd command")
}

fn espflash_connection_failure_text(output: &str) -> bool {
    let output = output.to_ascii_lowercase();
    output.contains("failed to connect to the device")
        || output.contains("error while connecting to device")
        || output.contains("no such device or address")
        || output.contains("broken pipe")
}

fn bounded_espflash_output(bytes: &[u8]) -> String {
    const MAX_OUTPUT_BYTES: usize = 4_096;
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.trim().to_string();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{} [truncated]", text[..end].trim())
}

async fn run_espflash_with_exclusive_serial(
    state: &AppState,
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
) -> Result<(), HttpError> {
    let _serial_rpc =
        acquire_serial_rpc_with_timeout(state.serial_rpc.clone(), SERIAL_RPC_TIMEOUT).await?;
    drop_cached_serial_session(&state.serial_sessions, port_path)?;
    let program = resolve_espflash_program();
    run_flash_transaction_with_program(artifact, root, port_path, &program).await
}

async fn acquire_serial_rpc_with_timeout(
    serial_rpc: Arc<tokio::sync::Mutex<()>>,
    timeout: Duration,
) -> Result<tokio::sync::OwnedMutexGuard<()>, HttpError> {
    tokio::time::timeout(timeout, serial_rpc.lock_owned())
        .await
        .map_err(|_| {
            HttpError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "serial_lock_timeout",
                "Timed out waiting for exclusive USB serial access.",
                true,
            )
        })
}

fn drop_cached_serial_session(
    serial_sessions: &Arc<Mutex<SerialSessionMap>>,
    port_path: &str,
) -> Result<(), HttpError> {
    let mut serial_sessions = lock_serial_sessions(serial_sessions)?;
    serial_sessions.remove(port_path);
    Ok(())
}

fn build_espflash_args_with_reset_mode(
    artifact: &FirmwareArtifact,
    root: Option<&Path>,
    port_path: &str,
    before_reset: &str,
) -> Result<Vec<Vec<String>>, HttpError> {
    if port_path.is_empty() {
        return Err(HttpError::bad_request(
            "missing_port",
            "Real flash requires an explicit serial port.",
        ));
    }
    let partition_table = firmware_partition_table_path(root)?;
    if let Some(elf_image) = artifact.files.iter().find(|file| file.kind == "elf") {
        let path = resolve_artifact_path(root, &elf_image.path);
        let mut args = vec![
            "flash".to_string(),
            "--chip".to_string(),
            artifact.target_chip.clone(),
            "--port".to_string(),
            port_path.to_string(),
            "--before".to_string(),
            before_reset.to_string(),
            "--non-interactive".to_string(),
            "--no-stub".to_string(),
            "--after".to_string(),
            "hard-reset".to_string(),
        ];
        args.push("--partition-table".to_string());
        args.push(partition_table.to_string_lossy().into_owned());
        args.push(path.to_string_lossy().into_owned());
        return Ok(vec![args]);
    }

    let Some(app_image) = artifact.files.iter().find(|file| file.kind == "app") else {
        return Err(HttpError::bad_request(
            "missing_flash_image",
            "Artifact does not contain an ELF or raw app image.",
        ));
    };
    let app_address = app_image.flash_address.ok_or_else(|| {
        HttpError::bad_request("missing_flash_address", "Missing app flash address.")
    })?;
    let partition_table_binary = firmware_partition_table_binary_path(root)?;
    let app_path = resolve_artifact_path(root, &app_image.path);
    let common = vec![
        "--chip".to_string(),
        artifact.target_chip.clone(),
        "--port".to_string(),
        port_path.to_string(),
        "--non-interactive".to_string(),
    ];
    let mut partition_table_args = vec!["write-bin".to_string()];
    partition_table_args.extend(common.clone());
    partition_table_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        DEFAULT_PARTITION_TABLE_FLASH_ADDRESS.to_string(),
        partition_table_binary.to_string_lossy().into_owned(),
    ]);
    let mut app_args = vec!["write-bin".to_string()];
    app_args.extend(common.clone());
    app_args.extend([
        "--before".to_string(),
        before_reset.to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
        app_address.to_string(),
        app_path.to_string_lossy().into_owned(),
    ]);
    // espflash write-bin leaves the target in its loader. Reset explicitly once both images land.
    let mut reset_args = vec!["reset".to_string()];
    reset_args.extend(common);
    reset_args.extend(["--before".to_string(), before_reset.to_string()]);
    Ok(vec![partition_table_args, app_args, reset_args])
}

fn firmware_partition_table_path(root: Option<&Path>) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires an artifact root containing firmware/partitions.csv.",
        ));
    };
    let partition_table = root.join("firmware/partitions.csv");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Firmware flashing requires firmware/partitions.csv so flux_cfg is installed.",
        ))
    }
}

fn firmware_partition_table_binary_path(root: Option<&Path>) -> Result<PathBuf, HttpError> {
    let Some(root) = root else {
        return Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires an artifact root containing firmware/partitions.bin.",
        ));
    };
    let partition_table = root.join("firmware/partitions.bin");
    if partition_table.is_file() {
        Ok(partition_table)
    } else {
        Err(HttpError::bad_request(
            "firmware_partition_table_required",
            "Raw app flashing requires firmware/partitions.bin so flux_cfg is installed.",
        ))
    }
}

fn espflash_reset_modes(artifact: &FirmwareArtifact, port_path: &str) -> Vec<&'static str> {
    if artifact.target_chip == "esp32s3" && port_path.contains("usbmodem") {
        vec!["usb-reset", "usb-reset", "default-reset"]
    } else {
        vec!["default-reset"]
    }
}

fn requires_lease(state: &DevdState, device_id: &str) -> bool {
    state
        .devices
        .get(device_id)
        .map(|device| {
            matches!(
                device.transport,
                DeviceTransport::NativeSerial | DeviceTransport::Lan
            )
        })
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
    let message = match payload.op {
        WifiConfigOp::Set | WifiConfigOp::Clear => "wifi config accepted",
        WifiConfigOp::Cancel => "wifi cancellation confirmed",
    };
    state.emit(event(
        device_id,
        "wifi",
        message,
        json!({
            "op": payload.op,
            "ssid": payload.ssid,
            "passwordPresent": payload.password.is_some(),
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
                "faultAttentionAcknowledged": payload.fault_attention_acknowledged,
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
                "faultAttentionPending": status.fault_attention_pending,
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
        "calibration updated",
        json!({
            "op": op,
            "fittedFit": {
                "rtdAdc": calibration.rtd_adc.fitted_fit,
                "vinAdc": calibration.vin_adc.fitted_fit,
            },
            "slots": {
                "rtdAdc": calibration.rtd_adc.slots,
                "vinAdc": calibration.vin_adc.slots,
            },
            "activeSlot": {
                "rtdAdc": calibration.rtd_adc.active_slot,
                "vinAdc": calibration.vin_adc.active_slot,
            },
            "samples": {
                "rtdAdc": calibration.rtd_adc.samples.iter().flatten().count(),
                "vinAdc": calibration.vin_adc.samples.iter().flatten().count(),
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
                } else if key.eq_ignore_ascii_case("lan_pairing_code") {
                    redact_lan_pairing_code(field);
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

fn redact_lan_pairing_code(value: &mut Value) {
    if let Value::Object(object) = value
        && let Some(code) = object.get_mut("code")
    {
        *code = Value::String("<redacted>".to_string());
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
    use std::process::ExitStatus;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    fn flash_partition_layout(entries: &[(&str, u64, u64)]) -> Vec<FlashPartitionRange> {
        entries
            .iter()
            .map(|(label, offset, size)| FlashPartitionRange {
                label: (*label).to_string(),
                offset: *offset,
                size: *size,
            })
            .collect()
    }

    #[cfg(unix)]
    fn test_flash_artifact() -> FirmwareArtifact {
        FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec![],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![ArtifactFile {
                kind: "elf".to_string(),
                path: "firmware.elf".to_string(),
                sha256: "sha256:test".to_string(),
                size: 4,
                flash_address: None,
            }],
        }
    }

    #[cfg(unix)]
    fn write_flash_transaction_fixture(
        root: &Path,
        reject_staging_write: bool,
    ) -> (PathBuf, PathBuf, PathBuf) {
        let firmware_dir = root.join("firmware");
        fs::create_dir_all(&firmware_dir).unwrap();
        fs::write(
            firmware_dir.join("partitions.csv"),
            "nvs,data,nvs,0x9000,0x6000\nfactory,app,factory,0x10000,0x200000\nflux_cfg,data,0x06,0x210000,0x2000\n",
        )
        .unwrap();
        fs::write(root.join("firmware.elf"), b"ELF!").unwrap();

        let current_table = esp_idf_part::PartitionTable::try_from(
            b"nvs,data,nvs,0x9000,0x6000\nfactory,app,factory,0x10000,0x100000\nflux_cfg,data,0x06,0x110000,0x2000\n".to_vec(),
        )
        .unwrap()
        .to_bin()
        .unwrap();
        let table_path = root.join("current-partitions.bin");
        let source_path = root.join("current-flux-cfg.bin");
        let destination_path = root.join("target-flux-cfg.bin");
        let log_path = root.join("espflash-actions.log");
        fs::write(&table_path, current_table).unwrap();
        fs::write(&source_path, vec![0x5A; LEGACY_FLASH_CONFIG_SIZE as usize]).unwrap();
        fs::write(
            &destination_path,
            vec![0xFF; LEGACY_FLASH_CONFIG_SIZE as usize],
        )
        .unwrap();

        let stage_write = if reject_staging_write {
            "exit 42".to_string()
        } else {
            format!("cp \"$last\" \"{}\"", destination_path.display())
        };
        let script = root.join("fake-espflash.sh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$1\" >> \"{}\"\naction=\"$1\"\nlast=\"\"\naddress=\"\"\nfor arg in \"$@\"; do\n  last=\"$arg\"\n  case \"$arg\" in\n    0x8000|0x110000|0x210000) address=\"$arg\" ;;\n  esac\ndone\nif [ \"$action\" = \"read-flash\" ]; then\n  case \"$address\" in\n    0x8000) cp \"{}\" \"$last\" ;;\n    0x110000) cp \"{}\" \"$last\" ;;\n    0x210000) cp \"{}\" \"$last\" ;;\n    *) exit 43 ;;\n  esac\n  exit 0\nfi\nif [ \"$action\" = \"write-bin\" ]; then\n  if [ \"$address\" = \"0x210000\" ]; then\n    {}\n  else\n    exit 44\n  fi\n  exit 0\nfi\nif [ \"$action\" = \"flash\" ] || [ \"$action\" = \"reset\" ]; then\n  exit 0\nfi\nexit 45\n",
                log_path.display(),
                table_path.display(),
                source_path.display(),
                destination_path.display(),
                stage_write,
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&script, permissions).unwrap();

        (script, source_path, destination_path)
    }

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
    fn transport_events_redact_lan_pairing_codes() {
        let state = AppState::test();
        record_transport_event(
            &state,
            "mock-fp-lab-01",
            "rx",
            "usb_jsonl",
            "pairing-code-1",
            r#"{"type":"response","requestId":"pairing-code-1","ok":true,"result":{"lan_pairing_code":{"active":true,"code":"4827"}}}"#,
        );

        let inner = state.lock().unwrap();
        let event = inner.devices["mock-fp-lab-01"]
            .events
            .iter()
            .find(|event| event.kind == "transport")
            .unwrap();
        assert_eq!(
            event.payload["frame"]["result"]["lan_pairing_code"]["code"],
            "<redacted>"
        );
        assert!(
            !serde_json::to_string(&event.payload)
                .unwrap()
                .contains("4827")
        );
    }

    #[test]
    fn lan_pairing_code_requires_a_consistent_active_state() {
        assert!(
            validate_lan_pairing_code(LanPairingCode {
                active: true,
                code: Some("4827".to_string()),
            })
            .is_ok()
        );
        assert!(
            validate_lan_pairing_code(LanPairingCode {
                active: false,
                code: None,
            })
            .is_ok()
        );
        assert!(
            validate_lan_pairing_code(LanPairingCode {
                active: true,
                code: None,
            })
            .is_err()
        );
        assert!(
            validate_lan_pairing_code(LanPairingCode {
                active: false,
                code: Some("abcd".to_string()),
            })
            .is_err()
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
    fn serial_scan_without_fixed_target_lists_all_espressif_candidates() {
        let ports = vec![
            serialport::SerialPortInfo {
                port_name: "/dev/cu.usbmodem-a".to_string(),
                port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                    vid: 0x303a,
                    pid: 0x1001,
                    serial_number: Some("candidate-a".to_string()),
                    manufacturer: Some("Espressif".to_string()),
                    product: Some("USB JTAG/serial debug unit".to_string()),
                }),
            },
            serialport::SerialPortInfo {
                port_name: "/dev/cu.usbmodem-b".to_string(),
                port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                    vid: 0x303a,
                    pid: 0x1001,
                    serial_number: Some("candidate-b".to_string()),
                    manufacturer: Some("Espressif".to_string()),
                    product: Some("USB JTAG/serial debug unit".to_string()),
                }),
            },
            serialport::SerialPortInfo {
                port_name: "/dev/cu.other".to_string(),
                port_type: serialport::SerialPortType::Unknown,
            },
        ];

        let devices = scan_serial_devices_from_available(None, &ports);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].port_path.as_deref(), Some("/dev/cu.usbmodem-a"));
        assert_eq!(devices[1].port_path.as_deref(), Some("/dev/cu.usbmodem-b"));
        assert!(
            devices
                .iter()
                .all(|device| device.identity.device_id.is_empty())
        );
        assert!(
            devices
                .iter()
                .all(|device| device.identity.hostname.is_empty())
        );
    }

    #[test]
    fn native_serial_devices_advertise_devd_flash_capabilities() {
        let device = serial_device_record("/dev/cu.usbmodem-test", None);

        assert_eq!(device.transport, DeviceTransport::NativeSerial);
        assert_eq!(device.connection, ConnectionState::Disconnected);
        assert_eq!(device.identity.device_id, "");
        assert_eq!(device.identity.hostname, "");
        assert_eq!(device.identity.build_id, "native-serial-placeholder");
        assert_eq!(device.identity.board, "unknown");
        assert_eq!(device.status.current_temp_c, -1.0);
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

    #[tokio::test]
    async fn create_lease_refreshes_authorized_serial_device_before_lookup() {
        let dir = tempdir().unwrap();
        let port_path = dir.path().join("authorized-usbmodem");
        fs::write(&port_path, b"placeholder").unwrap();
        let config = AppConfig {
            serial_port: Some(port_path.clone()),
            ..AppConfig::default()
        };
        let state = AppState::new(config);
        let device = serial_device_record(port_path.to_str().unwrap(), None);

        let lease = create_lease(State(state.clone()), AxumPath(device.id.clone()))
            .await
            .unwrap()
            .0;

        assert_eq!(lease.device_id, device.id);
        let state_lock = state.lock().unwrap();
        assert!(state_lock.devices.contains_key(&lease.device_id));
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
    fn serial_bridge_error_preserves_wifi_state_and_records_event() {
        let state = AppState::test();
        let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
        serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
        serial_device.network.state = NetworkState::Connected;
        serial_device.network.ssid = Some("FluxPurr-Lab".to_string());
        serial_device.network.wifi_rssi = Some(-47);
        serial_device.status.network = serial_device.network.clone();
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
        assert_eq!(device.network.state, NetworkState::Connected);
        assert_eq!(device.network.wifi_rssi, Some(-47));
        assert_eq!(device.network.last_error, None);
        assert_eq!(device.status.network.state, NetworkState::Connected);
        assert_eq!(device.status.network.wifi_rssi, Some(-47));
        assert_eq!(device.status.network.last_error, None);
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
    fn artifact_catalog_uses_only_the_canonical_firmware_target() {
        let dir = tempdir().unwrap();
        let canonical_path = dir
            .path()
            .join("firmware/target/xtensa-esp32s3-none-elf/release");
        fs::create_dir_all(&canonical_path).unwrap();
        fs::write(canonical_path.join("flux-purr"), b"current-firmware-image").unwrap();

        let stale_path = dir.path().join("target/xtensa-esp32s3-none-elf/release");
        fs::create_dir_all(&stale_path).unwrap();
        fs::write(stale_path.join("flux-purr"), b"stale-firmware-image").unwrap();

        let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_id, "local-esp32s3-release");
        assert_eq!(artifacts[0].target_chip, "esp32s3");
        assert_eq!(artifacts[0].profile, "release + web_serial + net_http");
        assert_eq!(artifacts[0].features, ["web_serial", "net_http"]);
        assert_eq!(artifacts[0].files[0].kind, "elf");
        assert_eq!(
            artifacts[0].files[0].path,
            "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr"
        );
        assert_eq!(artifacts[0].files[0].size, 22);
        assert_eq!(artifacts[0].files[0].flash_address, None);
        assert!(artifacts[0].files[0].sha256.starts_with("sha256:"));
    }

    #[test]
    fn artifact_catalog_exposes_debug_firmware_separately() {
        let dir = tempdir().unwrap();
        let debug_path = dir
            .path()
            .join("firmware/target/buzzer-debug/xtensa-esp32s3-none-elf/release");
        fs::create_dir_all(&debug_path).unwrap();
        fs::write(debug_path.join("flux-purr"), b"debug-firmware-image").unwrap();

        let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].artifact_id,
            "local-esp32s3-release-buzzer-debug"
        );
        assert_eq!(
            artifacts[0].features,
            ["web_serial", "net_http", "buzzer-debug"]
        );
        assert_eq!(
            artifacts[0].files[0].path,
            "firmware/target/buzzer-debug/xtensa-esp32s3-none-elf/release/flux-purr"
        );
    }

    #[test]
    fn usb_runtime_wire_serializes_thermal_profile_mode() {
        let json = encode_usb_runtime_mode_for_test(&"100w".to_string());

        assert!(json.contains(r#""thermalProfileMode":"100w""#));
    }

    #[test]
    fn usb_runtime_wire_serializes_fault_attention_acknowledgement() {
        let json = serde_json::to_value(UsbRuntimeConfigWire {
            frame_type: "runtime_config",
            request_id: "attention-test",
            target_temp_c: Some(140),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: Some(true),
            heater_enabled: Some(false),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: Some(true),
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        })
        .unwrap();

        assert_eq!(json["faultAttentionAcknowledged"], true);
    }

    #[test]
    fn usb_buzzer_debug_wire_only_serializes_fixed_cue_or_scenario_requests() {
        let wire = UsbBuzzerDebugWire {
            frame_type: "buzzer_debug",
            request_id: "buzzer-1",
            op: BuzzerDebugOp::Run,
            buzzer_cue: None,
            buzzer_scenario: Some(BuzzerDebugScenario::ActiveCoolingRetrigger),
            repeat: false,
        };
        let json = serde_json::to_value(wire).unwrap();

        assert_eq!(json["type"], "buzzer_debug");
        assert_eq!(json["op"], "run");
        assert_eq!(json["buzzerScenario"], "active_cooling_retrigger");
        assert!(json.get("buzzerCue").is_none());
        assert!(json.get("frequencyHz").is_none());
        assert!(json.get("dutyPercent").is_none());
    }

    #[test]
    fn buzzer_debug_request_validation_accepts_only_the_operation_shape() {
        let valid = BuzzerDebugRequest {
            lease_id: "lease-1".to_string(),
            op: BuzzerDebugOp::Trigger,
            cue: Some(BuzzerDebugCue::UiInput),
            scenario: None,
            repeat: false,
        };
        assert!(validate_buzzer_debug_request(&valid).is_ok());

        let invalid = BuzzerDebugRequest {
            scenario: Some(BuzzerDebugScenario::FeedbackCoalesce),
            ..valid
        };
        let error = validate_buzzer_debug_request(&invalid).unwrap_err();
        assert_eq!(error.error.code, "invalid_buzzer_debug_command");
    }

    #[tokio::test]
    async fn buzzer_debug_rejects_firmware_without_the_development_capability() {
        let state = AppState::test();
        let device_id = "native-buzzer-debug-test";
        {
            let mut state_lock = state.lock().unwrap();
            state_lock.devices.insert(
                device_id.to_string(),
                DeviceRecord::native_serial_placeholder(
                    device_id,
                    "Native buzzer debug target".to_string(),
                    "/dev/null".to_string(),
                ),
            );
        }
        let lease = state.lease_device(device_id).unwrap();

        let error = configure_buzzer_debug(
            State(state),
            AxumPath(device_id.to_string()),
            Json(BuzzerDebugRequest {
                lease_id: lease.lease_id,
                op: BuzzerDebugOp::Status,
                cue: None,
                scenario: None,
                repeat: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.code, "buzzer_debug_unavailable");
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
        std::fs::create_dir_all(dir.path().join("firmware")).unwrap();
        std::fs::write(
            dir.path().join("firmware/partitions.csv"),
            "flux_cfg,data,0x06,0x210000,0x2000",
        )
        .unwrap();
        let commands = build_espflash_args_with_reset_mode(
            &artifact,
            Some(dir.path()),
            "/dev/cu.usbmodem21221401",
            "usb-reset",
        )
        .unwrap();
        assert_eq!(commands.len(), 1);
        let args = &commands[0];

        assert_eq!(args[0], "flash");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--port", "/dev/cu.usbmodem21221401"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--before", "usb-reset"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--after", "hard-reset"])
        );
        assert!(args.iter().any(|argument| argument == "--no-stub"));
        assert!(!args.contains(&"-S".to_string()));
        assert!(args.iter().any(|arg| arg.ends_with("firmware.elf")));
        assert!(args.windows(2).any(|pair| {
            pair[0] == "--partition-table" && pair[1].ends_with("firmware/partitions.csv")
        }));
        assert!(!args.contains(&"65536".to_string()));
    }

    #[test]
    fn real_flash_args_write_raw_app_bin_with_partition_table_and_reset() {
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
        std::fs::create_dir_all(dir.path().join("firmware")).unwrap();
        std::fs::write(
            dir.path().join("firmware/partitions.csv"),
            "flux_cfg,data,0x06,0x210000,0x2000",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("firmware/partitions.bin"),
            b"partition-table",
        )
        .unwrap();
        let commands = build_espflash_args_with_reset_mode(
            &artifact,
            Some(dir.path()),
            "/dev/cu.usbmodem21221401",
            "usb-reset",
        )
        .unwrap();

        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0][0], "write-bin");
        assert!(commands[0].windows(2).any(|pair| {
            pair == [
                DEFAULT_PARTITION_TABLE_FLASH_ADDRESS.to_string(),
                dir.path()
                    .join("firmware/partitions.bin")
                    .to_string_lossy()
                    .into_owned(),
            ]
        }));
        assert_eq!(commands[1][0], "write-bin");
        assert!(commands[1].windows(2).any(|pair| {
            pair == [
                DEFAULT_APP_FLASH_ADDRESS.to_string(),
                dir.path()
                    .join("firmware.bin")
                    .to_string_lossy()
                    .into_owned(),
            ]
        }));
        assert_eq!(commands[2][0], "reset");
        assert!(
            commands[2]
                .windows(2)
                .any(|pair| pair == ["--before", "usb-reset"])
        );
    }

    #[test]
    fn non_native_serial_ports_keep_the_default_reset_mode() {
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec![],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![],
        };
        assert_eq!(
            espflash_reset_modes(&artifact, "/dev/cu.usbserial-1410"),
            ["default-reset"]
        );
    }

    #[test]
    fn usbmodem_flash_retries_usb_reset_before_default_reset_without_manual_boot_mode() {
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec![],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![],
        };

        assert_eq!(
            espflash_reset_modes(&artifact, "/dev/cu.usbmodem2111401"),
            ["usb-reset", "usb-reset", "default-reset"]
        );
    }

    #[test]
    fn broken_pipe_is_an_espflash_connection_failure() {
        assert!(espflash_connection_failure_text(
            "IO error while using serial port: Broken pipe"
        ));
        assert!(espflash_connection_failure_text(
            "Error while connecting to device: No such file or directory (os error 2)"
        ));
        assert!(!espflash_connection_failure_text(
            "Image verification failed after flash"
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn usbmodem_connection_failure_retries_usb_reset_before_default_reset() {
        let dir = tempdir().unwrap();
        let program = dir.path().join("retrying-espflash.sh");
        let attempts = dir.path().join("attempts.log");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$1\" >> \"{}\"\nattempts=$(wc -l < \"{}\")\nif [ \"$1\" = \"usb-reset\" ] && [ \"$attempts\" -eq 2 ]; then\n  exit 0\nfi\nprintf '%s\\n' 'Broken pipe' >&2\nexit 1\n",
                attempts.display(),
                attempts.display(),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions).unwrap();

        run_espflash_with_reset_fallback_with_program(
            &program,
            &test_flash_artifact(),
            "/dev/cu.usbmodem2111401",
            |before_reset| Ok(vec![vec![before_reset.to_string()]]),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(attempts).unwrap(),
            "usb-reset\nusb-reset\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bundle_flash_retries_a_transient_connection_failure() {
        let dir = tempdir().unwrap();
        let program = dir.path().join("retrying-bundle-espflash.sh");
        let attempts = dir.path().join("attempts.log");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}\"\nattempts=$(wc -l < \"{}\")\nif [ \"$attempts\" -eq 1 ]; then\n  printf '%s\\n' 'Broken pipe' >&2\n  exit 1\nfi\n",
                attempts.display(),
                attempts.display(),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions).unwrap();

        require_bundle_espflash_success(
            &program,
            &[
                "write-bin".to_string(),
                "--before".to_string(),
                "no-reset".to_string(),
            ],
            "/dev/cu.usbmodem2111401",
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(attempts).unwrap(),
            "write-bin --before no-reset\nwrite-bin --before usb-reset\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bundle_recovery_retries_initial_usb_reset_before_default_reset() {
        let dir = tempdir().unwrap();
        let program = dir.path().join("retrying-recovery-espflash.sh");
        let attempts = dir.path().join("attempts.log");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}\"\ncase \"$*\" in\n  *'--before default-reset'*) exit 0 ;;\nesac\nprintf '%s\\n' 'Broken pipe' >&2\nexit 1\n",
                attempts.display(),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions).unwrap();

        require_bundle_espflash_success(
            &program,
            &[
                "erase-flash".to_string(),
                "--before".to_string(),
                "usb-reset".to_string(),
            ],
            "/dev/cu.usbmodem2111401",
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(attempts).unwrap(),
            "erase-flash --before usb-reset\nerase-flash --before usb-reset\nerase-flash --before default-reset\n"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn espflash_subprocess_timeout_is_reported_without_hanging_the_request() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let program = dir.path().join("stuck-espflash");
        std::fs::write(&program, "#!/bin/sh\nsleep 5\n").unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions).unwrap();

        let started = Instant::now();
        let error = run_espflash_command_with_timeout(
            &program,
            &["read-flash".to_string()],
            Duration::from_millis(50),
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "flash_tool_timeout");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn flash_serial_lock_wait_is_bounded() {
        let serial_rpc = Arc::new(tokio::sync::Mutex::new(()));
        let held = serial_rpc.clone().lock_owned().await;

        let started = Instant::now();
        let error = acquire_serial_rpc_with_timeout(serial_rpc, Duration::from_millis(25))
            .await
            .unwrap_err();

        assert_eq!(error.error.code, "serial_lock_timeout");
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(held);
    }

    #[test]
    fn flash_config_migration_stages_config_before_expanding_the_app_partition() {
        let current = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x100000),
            ("flux_cfg", 0x110000, 0x2000),
        ]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        assert_eq!(
            plan_flash_config_migration(&current, &target).unwrap(),
            Some(FlashConfigMigrationPlan {
                source: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x110000,
                    size: 0x2000,
                },
                destination: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x210000,
                    size: 0x2000,
                },
            })
        );
    }

    #[test]
    fn flash_config_migration_keeps_a_backup_when_the_partition_is_unchanged() {
        let layout = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        assert_eq!(
            plan_flash_config_migration(&layout, &layout).unwrap(),
            Some(FlashConfigMigrationPlan {
                source: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x210000,
                    size: 0x2000,
                },
                destination: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x210000,
                    size: 0x2000,
                },
            })
        );
    }

    #[test]
    fn flash_config_migration_rejects_a_destination_used_by_the_current_layout() {
        let current = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x100000),
            ("flux_cfg", 0x110000, 0x2000),
            ("reserved", 0x210000, 0x2000),
        ]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        let error = plan_flash_config_migration(&current, &target).unwrap_err();

        assert_eq!(error.error.code, "flash_config_destination_in_use");
    }

    #[test]
    fn flash_config_migration_rejects_a_smaller_destination_partition() {
        let current = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x100000),
            ("flux_cfg", 0x110000, 0x2000),
        ]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x1000),
        ]);

        let error = plan_flash_config_migration(&current, &target).unwrap_err();

        assert_eq!(error.error.code, "flash_config_destination_too_small");
    }

    #[test]
    fn flash_config_migration_preserves_an_unpartitioned_legacy_record() {
        let current =
            flash_partition_layout(&[("nvs", 0x9000, 0x6000), ("factory", 0x10000, 0x100000)]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        assert_eq!(
            plan_flash_config_migration(&current, &target).unwrap(),
            Some(FlashConfigMigrationPlan {
                source: FlashPartitionRange {
                    label: "legacy_raw".to_string(),
                    offset: LEGACY_FLASH_CONFIG_OFFSET,
                    size: LEGACY_FLASH_CONFIG_SIZE,
                },
                destination: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x210000,
                    size: 0x2000,
                },
            })
        );
    }

    #[test]
    fn flash_config_migration_preserves_legacy_record_inside_old_factory_partition() {
        let current =
            flash_partition_layout(&[("nvs", 0x9000, 0x6000), ("factory", 0x10000, 0x200000)]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        assert_eq!(
            plan_flash_config_migration(&current, &target).unwrap(),
            Some(FlashConfigMigrationPlan {
                source: FlashPartitionRange {
                    label: "legacy_raw".to_string(),
                    offset: LEGACY_FLASH_CONFIG_OFFSET,
                    size: LEGACY_FLASH_CONFIG_SIZE,
                },
                destination: FlashPartitionRange {
                    label: "flux_cfg".to_string(),
                    offset: 0x210000,
                    size: 0x2000,
                },
            })
        );
    }

    #[test]
    fn flash_config_transport_commands_use_rom_reads_without_intermediate_reset() {
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec![],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![],
        };
        let source = Path::new("/private/tmp/flux_cfg-source.bin");
        let read = build_espflash_read_flash_args(
            &artifact,
            "/dev/cu.usbmodem2111401",
            "usb-reset",
            LEGACY_FLASH_CONFIG_OFFSET,
            LEGACY_FLASH_CONFIG_SIZE,
            source,
        )
        .unwrap();
        let write = build_espflash_write_bin_args(
            &artifact,
            "/dev/cu.usbmodem2111401",
            "usb-reset",
            0x210000,
            source,
        )
        .unwrap();

        assert_eq!(read[0], "read-flash");
        assert!(read.windows(2).any(|pair| pair == ["--after", "no-reset"]));
        assert!(read.iter().any(|argument| argument == "--no-stub"));
        assert!(read.windows(2).any(|pair| pair == ["0x110000", "0x2000"]));
        assert_eq!(write[0], "write-bin");
        assert!(write.windows(2).any(|pair| pair == ["--after", "no-reset"]));
        assert!(
            write
                .windows(2)
                .any(|pair| pair == ["0x210000", source.to_str().unwrap()])
        );
    }

    #[test]
    fn transient_usb_jtag_connection_errors_are_retryable() {
        assert!(espflash_connection_failure_text(
            "Error while connecting to device: No such device or address"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn flash_end_error_is_recoverable_only_for_a_complete_flash_command() {
        let flash_args = vec!["flash".to_string()];
        let write_args = vec!["write-bin".to_string()];
        let output = Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: b"Error: Error while running FlashEnd command".to_vec(),
        };

        assert!(espflash_flash_end_requires_reset(&flash_args, &output));
        assert!(!espflash_flash_end_requires_reset(&write_args, &output));
    }

    #[test]
    fn flash_end_recovery_reset_is_explicit_and_uses_the_authorized_usb_port() {
        let artifact = FirmwareArtifact {
            artifact_id: "test-artifact".to_string(),
            name: "Test".to_string(),
            version: "fw/test".to_string(),
            git_sha: "abc".to_string(),
            build_id: "build".to_string(),
            target_chip: "esp32s3".to_string(),
            profile: "release".to_string(),
            features: vec![],
            protocol: "flux-purr.usb.v1".to_string(),
            files: vec![],
        };

        let args =
            build_espflash_reset_args(&artifact, "/dev/cu.usbmodem2111401", "usb-reset").unwrap();

        assert_eq!(args[0], "reset");
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--port", "/dev/cu.usbmodem2111401"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--before", "usb-reset"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--after", "hard-reset"])
        );
    }

    #[test]
    fn unchanged_config_range_does_not_need_post_flash_restore() {
        let unchanged = FlashConfigMigrationPlan {
            source: FlashPartitionRange {
                label: "flux_cfg".to_string(),
                offset: 0x210000,
                size: 0x2000,
            },
            destination: FlashPartitionRange {
                label: "flux_cfg".to_string(),
                offset: 0x210000,
                size: 0x2000,
            },
        };
        let moved = FlashConfigMigrationPlan {
            source: FlashPartitionRange {
                label: "legacy_raw".to_string(),
                offset: 0x110000,
                size: 0x2000,
            },
            destination: unchanged.destination.clone(),
        };

        assert!(!flash_config_restore_required(&unchanged));
        assert!(flash_config_restore_required(&moved));
    }

    #[test]
    fn bundle_flash_commands_keep_usb_serial_jtag_in_loader_until_final_reset() {
        let common = vec![
            "--chip".to_string(),
            "esp32s3".to_string(),
            "--port".to_string(),
            "/dev/cu.usbmodem2111401".to_string(),
            "--non-interactive".to_string(),
        ];
        let config_read = build_bundle_read_flash_args(
            &common,
            "usb-reset",
            LEGACY_FLASH_CONFIG_OFFSET,
            LEGACY_FLASH_CONFIG_SIZE,
            Path::new("/private/tmp/flux_cfg.bin"),
        );
        let segment_write = build_bundle_write_bin_args(
            &common,
            "no-reset",
            0x10_000,
            Path::new("/private/tmp/app.bin"),
        );

        assert!(config_read.iter().any(|argument| argument == "--no-stub"));
        assert!(
            config_read
                .windows(2)
                .any(|pair| pair == ["--before", "usb-reset"])
        );
        assert!(
            config_read
                .windows(2)
                .any(|pair| pair == ["--after", "no-reset"])
        );
        assert!(
            segment_write
                .windows(2)
                .any(|pair| pair == ["--before", "no-reset"])
        );
        assert!(
            segment_write
                .windows(2)
                .any(|pair| pair == ["--after", "no-reset"])
        );
    }

    #[test]
    fn bundle_retry_replaces_only_the_recoverable_no_reset_mode() {
        let command = vec![
            "write-bin".to_string(),
            "--before".to_string(),
            "no-reset".to_string(),
            "--after".to_string(),
            "no-reset".to_string(),
        ];

        assert_eq!(
            replace_espflash_before_reset(&command, "usb-reset"),
            Some(vec![
                "write-bin".to_string(),
                "--before".to_string(),
                "usb-reset".to_string(),
                "--after".to_string(),
                "no-reset".to_string(),
            ])
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn flash_transaction_restores_and_verifies_preserved_config_after_writing_the_app() {
        let root = tempdir().unwrap();
        let (program, source, destination) = write_flash_transaction_fixture(root.path(), false);

        run_flash_transaction_with_program(
            &test_flash_artifact(),
            Some(root.path()),
            "/dev/cu.usbmodem2111401",
            &program,
        )
        .await
        .unwrap();

        assert_eq!(fs::read(&destination).unwrap(), fs::read(&source).unwrap());
        assert_eq!(
            fs::read_to_string(root.path().join("espflash-actions.log"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec![
                "read-flash",
                "read-flash",
                "flash",
                "write-bin",
                "read-flash",
                "reset"
            ]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn flash_transaction_reports_restore_failure_after_writing_the_app() {
        let root = tempdir().unwrap();
        let (program, source, destination) = write_flash_transaction_fixture(root.path(), true);

        let error = run_flash_transaction_with_program(
            &test_flash_artifact(),
            Some(root.path()),
            "/dev/cu.usbmodem2111401",
            &program,
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "flash_tool_failed");
        assert_ne!(fs::read(&destination).unwrap(), fs::read(&source).unwrap());
        assert_eq!(
            fs::read_to_string(root.path().join("espflash-actions.log"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec!["read-flash", "read-flash", "flash", "write-bin"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn flash_transaction_never_writes_the_app_before_config_backup_is_complete() {
        let root = tempdir().unwrap();
        let (program, source, _) = write_flash_transaction_fixture(root.path(), false);
        fs::remove_file(&source).unwrap();

        let error = run_flash_transaction_with_program(
            &test_flash_artifact(),
            Some(root.path()),
            "/dev/cu.usbmodem2111401",
            &program,
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "flash_tool_failed");
        assert_eq!(
            fs::read_to_string(root.path().join("espflash-actions.log"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec!["read-flash", "read-flash"]
        );
    }

    #[test]
    fn flash_partition_table_parser_rejects_truncated_device_data() {
        let error = parse_flash_partition_table(
            vec![0xAA],
            "flash_partition_table_invalid",
            "The current device partition table is invalid; refusing to flash.",
        )
        .unwrap_err();

        assert_eq!(error.error.code, "flash_partition_table_invalid");
    }

    #[test]
    fn flash_config_migration_does_not_claim_data_from_an_overwritten_legacy_range() {
        let current = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x100000),
            ("reserved", 0x110000, 0x2000),
        ]);
        let target = flash_partition_layout(&[
            ("nvs", 0x9000, 0x6000),
            ("factory", 0x10000, 0x200000),
            ("flux_cfg", 0x210000, 0x2000),
        ]);

        assert_eq!(
            plan_flash_config_migration(&current, &target).unwrap(),
            None
        );
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(error.error.code, "lease_expired");
    }

    #[tokio::test]
    async fn native_serial_reads_and_pairing_window_operations_require_an_active_lease() {
        let state = AppState::test();
        {
            let mut state_lock = state.lock().unwrap();
            state_lock.devices.insert(
                "native-test".to_string(),
                DeviceRecord::native_serial_placeholder(
                    "native-test",
                    "Native test target".to_string(),
                    "/dev/null".to_string(),
                ),
            );
        }

        let pairing_error = get_lan_pairing_code(
            State(state.clone()),
            AxumPath("native-test".to_string()),
            Query(LeaseQuery { lease_id: None }),
        )
        .await
        .unwrap_err();
        assert_eq!(pairing_error.status, StatusCode::FORBIDDEN);
        assert_eq!(pairing_error.error.code, "lease_required");

        let pairing_window_error = open_lan_pairing_window(
            State(state.clone()),
            AxumPath("native-test".to_string()),
            Query(LeaseQuery { lease_id: None }),
        )
        .await
        .unwrap_err();
        assert_eq!(pairing_window_error.status, StatusCode::FORBIDDEN);
        assert_eq!(pairing_window_error.error.code, "lease_required");

        let status_error = device_status(
            State(state),
            AxumPath("native-test".to_string()),
            Query(LeaseQuery { lease_id: None }),
        )
        .await
        .unwrap_err();
        assert_eq!(status_error.status, StatusCode::FORBIDDEN);
        assert_eq!(status_error.error.code, "lease_required");
    }

    fn test_thermal_control_profile_point(target_temp_c: i16) -> ThermalControlProfilePoint {
        ThermalControlProfilePoint {
            target_temp_c,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 1_000,
            warmup_reenter_centi_c: 400,
            approach_power_permille: 500,
            approach_floor_power_permille: 300,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 300,
            hold_reheat_power_permille: 350,
            hold_entry_centi_c: 150,
            hold_exit_centi_c: 100,
            hold_on_centi_c: 20,
            hold_off_centi_c: 100,
            overshoot_cutoff_centi_c: 200,
            hold_kp_permille_per_c: 20,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 2,
            approach_lead_ticks: 2,
            hold_lead_ticks: 1,
        }
    }

    #[tokio::test]
    async fn runtime_endpoint_previews_and_clears_thermal_control_profile() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let preview = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::Preview,
                    bank: None,
                    profile: Some(ThermalControlProfilePackage {
                        settings: None,
                        points: vec![
                            Some(ThermalControlProfilePoint {
                                target_temp_c: 100,
                                brake_distance_centi_c: 700,
                                warmup_power_permille: 320,
                                warmup_reenter_centi_c: 0,
                                approach_power_permille: 320,
                                approach_floor_power_permille: 220,
                                approach_damping_exponent_permille: 1_000,
                                approach_tail_window_centi_c: 0,
                                hold_power_permille: 220,
                                hold_reheat_power_permille: 0,
                                hold_entry_centi_c: 0,
                                hold_exit_centi_c: 0,
                                hold_on_centi_c: 0,
                                hold_off_centi_c: 0,
                                overshoot_cutoff_centi_c: 0,
                                hold_kp_permille_per_c: 0,
                                hold_ki_permille_per_c_tick: 0,
                                hold_blend_ticks: 0,
                                approach_lead_ticks: 0,
                                hold_lead_ticks: 0,
                            }),
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                            None,
                        ],
                    }),
                }),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(preview.thermal_control_profile_preview);
        assert!(preview.thermal_control.profile_active);
        assert_eq!(preview.thermal_control.profile_source, "preview");

        let clear_saved = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::ClearSaved,
                    bank: None,
                    profile: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(clear_saved.thermal_control_profile_preview);
        assert!(clear_saved.thermal_control.profile_active);
        assert_eq!(clear_saved.thermal_control.profile_source, "preview");

        let clear = configure_runtime(
            State(state),
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
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::ClearPreview,
                    bank: None,
                    profile: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(!clear.thermal_control_profile_preview);
        assert_eq!(clear.thermal_control.profile_source, "default");
        assert!(!clear.thermal_control.profile_active);
    }

    #[tokio::test]
    async fn runtime_endpoint_saves_and_clears_saved_thermal_control_profile() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let points = (0..FRONT_PANEL_PRESET_COUNT)
            .map(|index| {
                Some(test_thermal_control_profile_point(
                    60 + i16::try_from(index).unwrap() * 20,
                ))
            })
            .collect();
        let save = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::Save,
                    bank: None,
                    profile: Some(ThermalControlProfilePackage {
                        settings: None,
                        points,
                    }),
                }),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(!save.thermal_control_profile_preview);
        assert!(save.thermal_control.profile_active);
        assert_eq!(save.thermal_control.profile_source, "saved");

        let clear_saved = configure_runtime(
            State(state),
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
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::ClearSaved,
                    bank: None,
                    profile: None,
                }),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(!clear_saved.thermal_control_profile_preview);
        assert_eq!(clear_saved.thermal_control.profile_source, "default");
        assert!(!clear_saved.thermal_control.profile_active);
    }

    #[test]
    fn thermal_profile_clear_preview_rejects_profile_payload() {
        let error = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
            op: ThermalControlProfileOp::ClearPreview,
            bank: None,
            profile: Some(ThermalControlProfilePackage {
                settings: None,
                points: vec![None; FRONT_PANEL_PRESET_COUNT],
            }),
        })
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.code, "invalid_thermal_profile");
    }

    #[test]
    fn thermal_profile_rejects_out_of_range_warmup_reentry() {
        let mut point = test_thermal_control_profile_point(140);
        point.warmup_reenter_centi_c = 5_001;
        let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
        points[0] = Some(point);

        let error = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
            op: ThermalControlProfileOp::Preview,
            bank: None,
            profile: Some(ThermalControlProfilePackage {
                settings: None,
                points,
            }),
        })
        .unwrap_err();

        assert_eq!(error.error.code, "invalid_thermal_profile");
    }

    #[test]
    fn runtime_config_rejects_unknown_thermal_profile_enums() {
        let mut payload = RuntimeConfigRequest {
            lease_id: "lease-1".to_string(),
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: Some("turbo".to_string()),
            thermal_control_profile: None,
        };
        assert_eq!(
            validate_runtime_config(&payload).unwrap_err().error.code,
            "invalid_thermal_profile_mode"
        );

        payload.thermal_profile_mode = None;
        payload.thermal_control_profile = Some(ThermalControlProfileRequest {
            op: ThermalControlProfileOp::ClearSaved,
            bank: Some("pps9a".to_string()),
            profile: None,
        });
        assert_eq!(
            validate_runtime_config(&payload).unwrap_err().error.code,
            "invalid_thermal_profile_bank"
        );
    }

    #[tokio::test]
    async fn mock_persistence_to_inactive_bank_does_not_switch_the_resolved_bank() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
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
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: Some(ThermalControlProfileRequest {
                    op: ThermalControlProfileOp::Save,
                    bank: Some("pps5a".to_string()),
                    profile: Some(ThermalControlProfilePackage {
                        settings: None,
                        points: vec![None; FRONT_PANEL_PRESET_COUNT],
                    }),
                }),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(status.thermal_profile_resolved_bank, "pps3a");
        assert_eq!(status.thermal_control.profile_source, "default");
        assert!(
            state
                .lock()
                .unwrap()
                .devices
                .get("mock-fp-lab-01")
                .unwrap()
                .saved_thermal_control_profile_pps5a
                .is_some()
        );
    }

    #[test]
    fn thermal_profile_preview_accepts_the_ch224q_5v_floor() {
        let result = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
            op: ThermalControlProfileOp::Preview,
            bank: None,
            profile: Some(ThermalControlProfilePackage {
                settings: Some(ThermalControlProfileSettings {
                    temp_filter_alpha_permille: 700,
                    warmup_reenter_centi_c: 400,
                    hold_entry_centi_c: 90,
                    hold_exit_centi_c: 200,
                    hold_on_centi_c: 30,
                    hold_off_centi_c: 5,
                    overshoot_cutoff_centi_c: 25,
                    approach_max_ticks: 5,
                    approach_min_power_ratio_permille: 0,
                    hold_kp_permille_per_c: 120,
                    hold_ki_permille_per_c_tick: 12,
                    hold_blend_ticks: 12,
                    hold_reheat_power_permille: 0,
                    approach_lead_ticks: 0,
                    hold_lead_ticks: 0,
                    auto_adjustable_working_floor_mv: PPS_HARDWARE_MIN_MV,
                    heater_current_reserve_ma: 200,
                }),
                points: vec![None; FRONT_PANEL_PRESET_COUNT],
            }),
        });

        assert!(result.is_ok());
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
                static_ipv4: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
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
    async fn thermal_plant_mock_job_requires_20v_three_amp_capability() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let started = configure_calibration_job(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id.clone(),
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(started.status, CalibrationJobStatus::Running);
        assert_eq!(started.kind, Some(CalibrationJobKind::ThermalPlantAuto));
        assert_eq!(started.next_request_mv, Some(21_000));
        assert_eq!(
            state
                .lock()
                .unwrap()
                .devices
                .get("mock-fp-lab-01")
                .unwrap()
                .status
                .calibration
                .mode,
            CalibrationMode::ThermalPlant
        );

        let state_lock = state.lock().unwrap();
        let status = &state_lock.devices.get("mock-fp-lab-01").unwrap().status;
        assert!(!status.heater_enabled);
        assert_eq!(status.heater_output_percent, 0);
        assert_eq!(status.heater_physical_output_percent, 0);
        assert_eq!(status.manual_pps_mv, Some(21_000));
        assert_eq!(status.calibration.pps_mv, Some(21_000));
        drop(state_lock);

        let _ = configure_calibration_job(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id.clone(),
                op: CalibrationJobOp::Cancel,
                kind: None,
            }),
        )
        .await
        .unwrap();

        {
            let mut state_lock = state.lock().unwrap();
            state_lock
                .devices
                .get_mut("mock-fp-lab-01")
                .unwrap()
                .status
                .pps_capability_max_ma = Some(2_999);
            state_lock
                .devices
                .get_mut("mock-fp-lab-01")
                .unwrap()
                .mock_pps_apdos[0]
                .max_ma = 2_999;
        }
        let error = configure_calibration_job(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id,
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "thermal_plant_source_unsupported");
    }

    #[tokio::test]
    async fn thermal_plant_mock_job_locks_runtime_overrides_and_state_transitions() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();

        let _ = configure_calibration_job(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id.clone(),
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap();

        let repeated_start = configure_calibration_job(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id.clone(),
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(repeated_start.error.code, "heater_disarm_pending");

        let manual_override = configure_runtime(
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
                manual_pps_mv: Some(20_000),
                manual_pps_ma: Some(3_000),
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(manual_override.error.code, "manual_pps_calibration_busy");

        let heater_override = configure_runtime(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(RuntimeConfigRequest {
                lease_id: lease.lease_id.clone(),
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: Some(true),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
                thermal_profile_mode: None,
                fault_attention_acknowledged: None,
                thermal_control_profile: None,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(heater_override.error.code, "manual_pps_calibration_busy");

        let canceled = configure_calibration_job(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id.clone(),
                op: CalibrationJobOp::Cancel,
                kind: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(canceled.status, CalibrationJobStatus::Canceled);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .devices
                .get("mock-fp-lab-01")
                .unwrap()
                .status
                .calibration
                .mode,
            CalibrationMode::Off
        );

        let idle_cancel = configure_calibration_job(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id,
                op: CalibrationJobOp::Cancel,
                kind: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(idle_cancel.status, CalibrationJobStatus::Canceled);
    }

    #[tokio::test]
    async fn thermal_plant_mock_job_requires_one_matching_apdo() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        {
            let mut state_lock = state.lock().unwrap();
            let device = state_lock.devices.get_mut("mock-fp-lab-01").unwrap();
            device.status.pps_capability_min_mv = Some(5_000);
            device.status.pps_capability_max_mv = Some(21_000);
            device.status.pps_capability_max_ma = Some(5_000);
            device.mock_pps_apdos = vec![
                MockPpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 5_000,
                },
                MockPpsApdo {
                    min_mv: 20_000,
                    max_mv: 21_000,
                    max_ma: 1_000,
                },
            ];
        }

        let error = configure_calibration_job(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id,
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "thermal_plant_source_unsupported");
    }

    #[tokio::test]
    async fn thermal_plant_mock_job_accepts_an_apdo_whose_range_starts_at_20v() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        {
            let mut state_lock = state.lock().unwrap();
            let device = state_lock.devices.get_mut("mock-fp-lab-01").unwrap();
            device.status.pps_capability_min_mv = Some(20_000);
            device.status.pps_capability_max_mv = Some(21_000);
            device.status.pps_capability_max_ma = Some(3_000);
            device.mock_pps_apdos = vec![MockPpsApdo {
                min_mv: 20_000,
                max_mv: 21_000,
                max_ma: 3_000,
            }];
        }

        let started = configure_calibration_job(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Json(CalibrationJobRequest {
                lease_id: lease.lease_id,
                op: CalibrationJobOp::Start,
                kind: Some(CalibrationJobKind::ThermalPlantAuto),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(started.status, CalibrationJobStatus::Running);
        assert_eq!(started.next_request_mv, Some(21_000));
    }

    #[tokio::test]
    async fn thermal_plant_run_reader_pages_cooling_trace_without_raw_adc() {
        let state = AppState::test();
        let lease = state.lease_device("mock-fp-lab-01").unwrap();

        let first = device_thermal_plant_run(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(ThermalPlantRunQuery {
                lease_id: Some(lease.lease_id.clone()),
                after_sample: Some(0),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(first.trace_page.points.len(), 14);
        assert_eq!(first.trace_page.next_sample, None);
        assert!(
            first
                .trace_page
                .points
                .iter()
                .any(|point| point.phase == ThermalPlantRunPhase::Cooling)
        );
        let serialized = serde_json::to_string(&first).unwrap();
        assert!(!serialized.contains("rawAdc"));
        assert!(serialized.len() < 8 * 1024);

        let tail = device_thermal_plant_run(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(ThermalPlantRunQuery {
                lease_id: Some(lease.lease_id),
                after_sample: Some(8),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(tail.trace_page.start_sample, 8);
        assert_eq!(tail.trace_page.points.len(), 6);
        assert_eq!(tail.trace_page.points[0].sample_index, 8);
    }

    #[test]
    fn calibration_slot_fit_normalizes_invalid_coefficients() {
        let mut device = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
        device.calibration.rtd_adc.slots.a = CalibrationSlotFit {
            gain: 0.0,
            offset_mv: f32::NAN,
        };
        device.calibration.rtd_adc.sanitize_slot_fits();

        assert_eq!(device.calibration.rtd_adc.slots.a.gain, 1.0);
        assert_eq!(device.calibration.rtd_adc.slots.a.offset_mv, 0.0);
    }

    #[test]
    fn manual_calibration_cannot_select_the_thermal_plant_runtime_state() {
        let status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        let error = validate_calibration_request_against_status(
            &CalibrationControlRequest {
                mode: Some(CalibrationMode::ThermalPlant),
                pps_enabled: None,
                pps_mv: None,
                heater_enabled: None,
                target_adc_mv: None,
            },
            &status,
            &status.calibration,
        )
        .unwrap_err();

        assert_eq!(error.error.code, "thermal_plant_managed_by_job");
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
            static_ipv4: Some(Some(WifiStaticIpv4Request {
                address: [192, 168, 31, 42],
                prefix_len: 24,
                gateway: [192, 168, 31, 1],
                dns: [1, 1, 1, 1],
            })),
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
    fn wifi_receipt_rejects_unversioned_and_malformed_network_snapshots() {
        let unversioned = extract_wifi_config_network(
            json!({
                "wifi": {
                    "network": {
                        "state": "saving",
                        "ssid": null,
                        "wifiPasswordLength": 0,
                        "ip": null,
                        "gateway": null,
                        "dns": [],
                        "wifiRssi": null,
                        "lastError": null
                    }
                }
            }),
            false,
        )
        .unwrap_err();
        assert_eq!(unversioned.error.code, "invalid_wifi_receipt");

        let malformed =
            extract_wifi_config_network(json!({ "wifi": { "network": 42 } }), false).unwrap_err();
        assert_eq!(malformed.error.code, "usb_payload_decode_failed");

        let non_public = extract_wifi_config_network(
            json!({
                "wifi": {
                    "network": {
                        "state": "saving",
                        "configurationGeneration": 1,
                        "transitionSequence": 1,
                        "ssid": null,
                        "wifiPasswordLength": 0,
                        "ip": null,
                        "gateway": null,
                        "dns": [],
                        "wifiRssi": null,
                        "lastError": null
                    }
                }
            }),
            false,
        )
        .unwrap_err();
        assert_eq!(non_public.error.code, "invalid_wifi_receipt");
        assert_eq!(
            non_public.error.message,
            "The device returned a non-public WiFi state."
        );
    }

    #[test]
    fn wifi_cancel_receipt_accepts_only_the_confirmed_idle_snapshot() {
        let receipt = extract_wifi_config_network(
            json!({
                "wifi": {
                    "network": {
                        "state": "idle",
                        "configurationGeneration": 4,
                        "transitionSequence": 12,
                        "ssid": "FluxPurr-Lab",
                        "wifiPasswordLength": 11,
                        "ip": null,
                        "gateway": null,
                        "dns": [],
                        "wifiRssi": null,
                        "lastError": null
                    }
                }
            }),
            true,
        )
        .unwrap();

        assert_eq!(receipt.state, NetworkState::Idle);
        assert_eq!(receipt.ssid.as_deref(), Some("FluxPurr-Lab"));
    }

    #[test]
    fn wifi_static_ipv4_preserves_absent_and_forwards_explicit_dhcp_clear() {
        let absent: WifiConfigRequest = serde_json::from_value(json!({
            "leaseId": "lease-1",
            "op": "set",
            "autoReconnect": false
        }))
        .unwrap();
        let clear: WifiConfigRequest = serde_json::from_value(json!({
            "leaseId": "lease-1",
            "op": "set",
            "staticIpv4": null
        }))
        .unwrap();

        assert!(absent.static_ipv4.is_none());
        assert!(matches!(clear.static_ipv4, Some(None)));

        let clear_wire = serde_json::to_value(UsbWifiConfigWire {
            frame_type: "wifi_config",
            request_id: "wifi-clear",
            op: "set",
            ssid: None,
            password: None,
            static_ipv4: clear.static_ipv4,
            telemetry_interval_ms: None,
        })
        .unwrap();
        let absent_wire = serde_json::to_value(UsbWifiConfigWire {
            frame_type: "wifi_config",
            request_id: "wifi-preserve",
            op: "set",
            ssid: None,
            password: None,
            static_ipv4: absent.static_ipv4,
            telemetry_interval_ms: None,
        })
        .unwrap();

        assert!(clear_wire["staticIpv4"].is_null());
        assert!(absent_wire.get("staticIpv4").is_none());
    }

    #[test]
    fn static_ipv4_validation_rejects_non_unicast_values() {
        let valid = WifiStaticIpv4Request {
            address: [192, 168, 31, 42],
            prefix_len: 24,
            gateway: [192, 168, 31, 1],
            dns: [1, 1, 1, 1],
        };
        assert!(static_ipv4_request_is_valid(valid));
        assert!(!static_ipv4_request_is_valid(WifiStaticIpv4Request {
            address: [224, 0, 0, 1],
            ..valid
        }));
        assert!(!static_ipv4_request_is_valid(WifiStaticIpv4Request {
            dns: [0, 0, 0, 0],
            ..valid
        }));
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
    fn usb_response_decoder_extracts_a_matching_frame_appended_to_a_boot_log() {
        let payload = decode_usb_response_line(
            br#"I (181) esp_image: segment 1: paddr=00061018 vaddr=3fc91988 size{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
            "req-1",
        )
        .unwrap()
        .unwrap();

        let network = extract_usb_payload::<NetworkSummary>(payload, "network").unwrap();
        assert_eq!(network.state, NetworkState::Disabled);
    }

    #[test]
    fn serial_line_reader_discards_an_overlong_frame_before_the_next_response() {
        let mut line = Vec::new();
        let mut discarding = false;
        for _ in 0..=SERIAL_LINE_LIMIT {
            assert!(!serial_line_finished(&mut line, &mut discarding, b'x'));
        }
        assert!(discarding);
        assert!(!serial_line_finished(&mut line, &mut discarding, b'\n'));
        assert!(!discarding);
        assert!(line.is_empty());

        let response = br#"{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#;
        for byte in response {
            assert!(!serial_line_finished(&mut line, &mut discarding, *byte));
        }
        assert!(serial_line_finished(&mut line, &mut discarding, b'\n'));
        assert!(decode_usb_response_line(&line, "req-1").unwrap().is_some());
    }

    #[test]
    fn usb_reset_markers_are_observed_without_reopening_the_session() {
        assert!(serial_line_is_usb_reset_marker(
            b"reset_reason=core_usb_uart"
        ));
        assert!(serial_line_is_usb_reset_marker(
            b" reset_reason=core_usb_jtag\r"
        ));
        assert!(!serial_line_is_usb_reset_marker(
            b"reset_reason=core_software"
        ));
        assert!(!serial_line_is_usb_reset_marker(
            b"boot_stage=lan_heap_ready"
        ));
        assert!(!serial_line_is_usb_reset_marker(
            br#"{\"type\":\"response\",\"requestId\":\"req-1\",\"ok\":true}"#
        ));
    }

    #[test]
    fn post_flash_boot_requires_runtime_ready() {
        let mut observation = BootObservation::default();

        assert!(
            !observation
                .observe_line("reset_reason=core_software")
                .unwrap()
        );
        assert!(
            !observation
                .observe_line("boot_stage=adc_init_complete")
                .unwrap()
        );
        assert!(observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
        assert_eq!(observation.reset_count, 1);
        assert_eq!(
            observation.last_stage.as_deref(),
            Some(RUNTIME_READY_BOOT_STAGE)
        );
    }

    #[test]
    fn post_flash_boot_rejects_a_stale_runtime_ready_marker() {
        let mut observation = BootObservation::default();

        assert!(!observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
        assert!(observation.last_stage.is_none());
        assert!(
            !observation
                .observe_line("boot_stage=display_init_complete")
                .unwrap()
        );
        assert!(observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
    }

    #[test]
    fn post_flash_boot_rejects_reboot_loops_and_panics() {
        let mut reboot = BootObservation::default();
        reboot.observe_line("reset_reason=core_software").unwrap();
        let error = reboot
            .observe_line("reset_reason=core_software")
            .unwrap_err();
        assert_eq!(error.error.code, "firmware_reboot_loop");

        let error = BootObservation::default()
            .observe_line("Guru Meditation Error: Core 0 panic'ed")
            .unwrap_err();
        assert_eq!(error.error.code, "firmware_boot_failed");
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
        assert!(status.adc_diagnostics.is_none());
    }

    #[test]
    fn usb_response_decoder_extracts_buzzer_debug_payload() {
        let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"buzzer-1","ok":true,"result":{"buzzer_debug":{"state":"complete","scenario":"feedback_replace","activeCue":"heater_on","trace":[{"elapsedMs":30,"decision":{"source":"developer_debug","cue":"heater_on","disposition":"replaced"}}],"outputTrace":[{"elapsedMs":90,"requestedFrequencyHz":1680,"appliedFrequencyHz":1739,"dutyPercent":50,"generation":2,"timerPrescaler":22,"timerPeriodTicks":999}]}}}"#,
            "buzzer-1",
        )
        .unwrap()
        .unwrap();

        let status = extract_usb_payload::<BuzzerDebugStatus>(payload, "buzzer_debug").unwrap();
        assert_eq!(status.state, BuzzerDebugSessionState::Complete);
        assert_eq!(status.scenario, Some(BuzzerDebugScenario::FeedbackReplace));
        assert_eq!(status.active_cue.as_deref(), Some("heater_on"));
        assert_eq!(status.trace[0].decision.disposition, "replaced");
        assert_eq!(status.output_trace[0].applied_frequency_hz, 1_739);
    }

    #[test]
    fn usb_response_decoder_preserves_adc_diagnostics() {
        let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"status-adc","ok":true,"result":{"status":{"mode":"sampling","uptimeSeconds":12,"currentTempC":31.5,"targetTempC":240,"heaterEnabled":false,"heaterOutputPercent":0,"activeCoolingEnabled":false,"fanDisplayState":"OFF","fanEnabled":false,"fanPwmPermille":0,"voltageMv":20000,"currentMa":0,"boardTempCenti":3150,"adcDiagnostics":{"calibrationSource":"efuse","efuseVersion":1,"attenuationDb":6,"initCode":1850,"referenceCode":1600,"referenceMv":850,"rtdRawCodeMean":2100,"rtdRawCodeMin":2098,"rtdRawCodeMax":2102,"rtdRawCodeSpread":4,"vinRawCodeMean":1800},"pdRequestMv":20000,"pdContractMv":20000,"pdState":"ready","frontpanelKey":null,"network":{"state":"idle","dns":[],"wifiRssi":null}}}}"#,
            "status-adc",
        )
        .unwrap()
        .unwrap();

        let status = extract_usb_payload::<ControlPlaneStatus>(payload, "status").unwrap();
        let diagnostics = status.adc_diagnostics.expect("ADC diagnostics present");

        assert_eq!(diagnostics.calibration_source, "efuse");
        assert_eq!(diagnostics.rtd_raw_code_spread, 4);
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
    fn usb_response_decoder_ignores_requestless_firmware_frame_errors() {
        assert!(decode_usb_response_line(
            br#"{"type":"error","requestId":null,"error":{"code":"malformed_json","message":"Malformed USB JSONL frame.","retryable":false}}"#,
            "req-1",
        )
        .unwrap()
        .is_none());
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
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: None,
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
            heater_physical_output_percent: 12,
            active_cooling_enabled: false,
            fan_display_state: "AUTO".to_string(),
            fan_enabled: true,
            fan_pwm_permille: 500,
            voltage_mv: 12_000,
            current_ma: 2_800,
            board_temp_centi: 3150,
            rtd_raw_adc_mv: Some(934),
            rtd_raw_adc_min_mv: Some(933),
            rtd_raw_adc_max_mv: Some(935),
            rtd_raw_adc_spread_mv: Some(2),
            vin_raw_adc_mv: Some(1003),
            adc_diagnostics: None,
            pd_request_mv: 12_000,
            pd_contract_mv: 12_000,
            pd_state: "ready".to_string(),
            pd_controller: Some("fusb302b".to_string()),
            pd_contract_kind: Some("pps".to_string()),
            pd_contract_current_ma: Some(3_000),
            pd_contract_power_mw: Some(36_000),
            pd_performance_guaranteed: Some(false),
            pd_degraded_reason: Some("pd_contract_below_20v".to_string()),
            manual_pps_enabled: false,
            manual_pps_mv: None,
            manual_pps_ma: None,
            pps_capability_min_mv: Some(5_000),
            pps_capability_max_mv: Some(21_000),
            pps_capability_max_ma: Some(3_000),
            manual_pps_error: None,
            fault_attention_pending: false,
            heater_fault_reason: None,
            heater_lock_reason: None,
            heater_control_phase: None,
            heater_error_c: None,
            heater_control_error_c: None,
            heater_control_temp_c: None,
            heater_control_measurement_guarded: false,
            heater_filtered_temp_c: None,
            heater_filtered_slope_c_per_s: None,
            heater_coast_active: false,
            heater_control_interval_ms: 0,
            heater_control_cycle_ms: 0,
            thermal_control_profile_preview: false,
            thermal_profile_mode: "65w".to_string(),
            thermal_profile_resolved_bank: "pps3a".to_string(),
            thermal_control: ThermalControlRuntime::default(),
            thermal_plant_model: ThermalPlantRuntime::default(),
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
            frontpanel_route: None,
            frontpanel_presented_route: None,
            frontpanel_presentation_count: None,
            network: NetworkSummary {
                state: NetworkState::Idle,
                configuration_generation: 0,
                transition_sequence: 0,
                failure_code: None,
                ssid: None,
                wifi_password_length: 0,
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
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: None,
        };
        let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        status.calibration.mode = CalibrationMode::VinAdc;
        status.calibration.pps_enabled = true;
        status.calibration.pps_mv = Some(12_000);
        status.calibration.heater_enabled = false;

        assert!(!runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn runtime_config_matcher_requires_attention_to_be_cleared_after_acknowledgement() {
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
            fault_attention_acknowledged: Some(true),
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        };
        let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        status.fault_attention_pending = true;
        assert!(!runtime_config_matches_status(&payload, &status));
        status.fault_attention_pending = false;
        assert!(runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn runtime_config_matcher_reconciles_preview_from_status_flag() {
        let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
        points[0] = Some(ThermalControlProfilePoint {
            target_temp_c: 120,
            brake_distance_centi_c: 700,
            warmup_power_permille: 320,
            warmup_reenter_centi_c: 0,
            approach_power_permille: 320,
            approach_floor_power_permille: 220,
            approach_damping_exponent_permille: 1_000,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 220,
            hold_reheat_power_permille: 0,
            hold_entry_centi_c: 0,
            hold_exit_centi_c: 0,
            hold_on_centi_c: 0,
            hold_off_centi_c: 0,
            overshoot_cutoff_centi_c: 0,
            hold_kp_permille_per_c: 0,
            hold_ki_permille_per_c_tick: 0,
            hold_blend_ticks: 0,
            approach_lead_ticks: 0,
            hold_lead_ticks: 0,
        });
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
            calibration: None,
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: Some(ThermalControlProfileRequest {
                op: ThermalControlProfileOp::Preview,
                bank: None,
                profile: Some(ThermalControlProfilePackage {
                    settings: None,
                    points,
                }),
            }),
        };
        let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        status.thermal_control_profile_preview = true;
        status.thermal_control = mock_thermal_runtime(
            status.target_temp_c,
            payload
                .thermal_control_profile
                .as_ref()
                .and_then(|profile| profile.profile.as_ref()),
            true,
        );

        assert!(runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn runtime_config_matcher_reconciles_saved_profile_runtime() {
        let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
        points[0] = Some(ThermalControlProfilePoint {
            target_temp_c: 220,
            brake_distance_centi_c: 520,
            warmup_power_permille: 1_000,
            warmup_reenter_centi_c: 0,
            approach_power_permille: 760,
            approach_floor_power_permille: 600,
            approach_damping_exponent_permille: 550,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 620,
            hold_reheat_power_permille: 700,
            hold_entry_centi_c: 8,
            hold_exit_centi_c: 50,
            hold_on_centi_c: 14,
            hold_off_centi_c: 240,
            overshoot_cutoff_centi_c: 320,
            hold_kp_permille_per_c: 22,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 2,
            approach_lead_ticks: 2,
            hold_lead_ticks: 0,
        });
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
            calibration: None,
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: Some(ThermalControlProfileRequest {
                op: ThermalControlProfileOp::Save,
                bank: None,
                profile: Some(ThermalControlProfilePackage {
                    settings: None,
                    points,
                }),
            }),
        };
        let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
        status.thermal_control = mock_thermal_runtime(
            status.target_temp_c,
            payload
                .thermal_control_profile
                .as_ref()
                .and_then(|profile| profile.profile.as_ref()),
            false,
        );

        assert!(runtime_config_matches_status(&payload, &status));
    }

    #[test]
    fn rtd_capture_expected_mv_uses_target_adc_before_temperature_curve() {
        let payload = CalibrationConfigRequest {
            lease_id: "lease-1".to_string(),
            op: CalibrationConfigOp::Capture,
            channel: Some(CalibrationChannel::RtdAdc),
            reference_temp_c: Some(49.0),
            reference_vin_mv: None,
            target_adc_mv: Some(1_000),
            observed_mv: None,
            expected_mv: None,
            sample_index: None,
            state: None,
            slot: None,
            fit: None,
        };

        assert_eq!(
            expected_calibration_adc_mv(&payload, CalibrationChannel::RtdAdc),
            Some(1_000)
        );
    }

    #[test]
    fn rtd_capture_expected_mv_requires_target_adc_without_explicit_expected() {
        let payload = CalibrationConfigRequest {
            lease_id: "lease-1".to_string(),
            op: CalibrationConfigOp::Capture,
            channel: Some(CalibrationChannel::RtdAdc),
            reference_temp_c: Some(49.0),
            reference_vin_mv: None,
            target_adc_mv: None,
            observed_mv: None,
            expected_mv: None,
            sample_index: None,
            state: None,
            slot: None,
            fit: None,
        };

        assert_eq!(
            expected_calibration_adc_mv(&payload, CalibrationChannel::RtdAdc),
            None
        );
    }

    #[test]
    fn backfills_live_rtd_capture_metadata_for_legacy_firmware_response() {
        let mut calibration = CalibrationState::default();
        calibration.rtd_adc.samples[0] = Some(CalibrationSample {
            observed_mv: 1_001,
            expected_mv: 970,
            reference_temp_c: None,
            target_adc_mv: None,
            reference_vin_mv: None,
        });
        let payload = CalibrationConfigRequest {
            lease_id: "lease-1".to_string(),
            op: CalibrationConfigOp::Capture,
            channel: Some(CalibrationChannel::RtdAdc),
            reference_temp_c: Some(49.0),
            reference_vin_mv: None,
            target_adc_mv: Some(1_000),
            observed_mv: None,
            expected_mv: Some(1_000),
            sample_index: None,
            state: None,
            slot: None,
            fit: None,
        };

        backfill_live_calibration_capture(&mut calibration, &payload);

        let sample = calibration.rtd_adc.samples[0].expect("sample should exist");
        assert_eq!(sample.observed_mv, 1_001);
        assert_eq!(sample.expected_mv, 1_000);
        assert_eq!(sample.reference_temp_c, Some(49.0));
        assert_eq!(sample.target_adc_mv, Some(1_000));
    }

    #[test]
    fn merges_live_rtd_sample_metadata_on_refresh() {
        let mut previous = CalibrationState::default();
        previous.rtd_adc.samples[0] = Some(CalibrationSample {
            observed_mv: 1_001,
            expected_mv: 1_000,
            reference_temp_c: Some(49.0),
            target_adc_mv: Some(1_000),
            reference_vin_mv: None,
        });
        let mut refreshed = CalibrationState::default();
        refreshed.rtd_adc.samples[0] = Some(CalibrationSample {
            observed_mv: 1_001,
            expected_mv: 1_000,
            reference_temp_c: None,
            target_adc_mv: None,
            reference_vin_mv: None,
        });

        merge_live_calibration_metadata(&mut refreshed, &previous);

        let sample = refreshed.rtd_adc.samples[0].expect("sample should exist");
        assert_eq!(sample.reference_temp_c, Some(49.0));
        assert_eq!(sample.target_adc_mv, Some(1_000));
    }

    #[test]
    fn incomplete_live_rtd_samples_are_not_web_facing_samples() {
        let mut calibration = CalibrationState::default();
        calibration.rtd_adc.samples[0] = Some(CalibrationSample {
            observed_mv: 1_001,
            expected_mv: 970,
            reference_temp_c: None,
            target_adc_mv: None,
            reference_vin_mv: None,
        });

        calibration.refresh_fits();

        assert!(calibration.rtd_adc.samples.iter().all(Option::is_none));
        assert_eq!(calibration.rtd_adc.fitted_fit.sample_count, 0);
        assert_eq!(calibration.rtd_adc.fitted_fit.gain, 1.0);
        assert_eq!(calibration.rtd_adc.fitted_fit.offset_mv, 0.0);
    }

    #[test]
    fn serial_requests_only_retry_after_an_observed_runtime_ready_marker() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(100);

        assert!(!should_retry_request_after_runtime_ready(
            true,
            b"boot_stage=display_ready",
            now,
            deadline
        ));
        assert!(!should_retry_request_after_runtime_ready(
            false,
            RUNTIME_READY_BOOT_STAGE.as_bytes(),
            now,
            deadline
        ));
        assert!(!should_retry_request_after_runtime_ready(
            true,
            RUNTIME_READY_BOOT_STAGE.as_bytes(),
            now,
            now
        ));
        assert!(should_retry_request_after_runtime_ready(
            true,
            RUNTIME_READY_BOOT_STAGE.as_bytes(),
            now,
            deadline
        ));
        assert_eq!(
            serial_rpc_timeout(SerialRetryPolicy::ReadOnly),
            SERIAL_READ_ONLY_RPC_TIMEOUT
        );
        assert_eq!(
            serial_rpc_timeout(SerialRetryPolicy::SingleShot),
            SERIAL_RPC_TIMEOUT
        );
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

    #[test]
    fn serial_request_line_limit_accepts_full_line_and_rejects_overflow() {
        assert!(validate_serial_request_len(&"x".repeat(SERIAL_LINE_LIMIT - 1)).is_ok());
        let error = validate_serial_request_len(&"x".repeat(SERIAL_LINE_LIMIT)).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.code, "usb_request_too_large");
    }

    #[cfg(unix)]
    #[test]
    fn serial_lock_is_not_reentrant_until_previous_session_is_dropped() {
        let port_path = "/tmp/flux-purr-devd-test-port";
        let deadline = Instant::now() + Duration::from_millis(250);

        let first = SerialPortProcessLock::acquire(port_path, deadline).unwrap();

        let second = match SerialPortProcessLock::acquire(
            port_path,
            Instant::now() + Duration::from_millis(250),
        ) {
            Ok(_) => panic!("second serial lock should time out while first session is alive"),
            Err(error) => error,
        };
        assert_eq!(second.error.code, "serial_lock_timeout");

        drop(first);

        let reopened =
            SerialPortProcessLock::acquire(port_path, Instant::now() + Duration::from_millis(250));
        assert!(reopened.is_ok());
    }

    #[tokio::test]
    async fn cancelled_request_keeps_serial_rpc_locked_until_worker_finishes() {
        let serial_rpc = Arc::new(tokio::sync::Mutex::new(()));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let worker_lock = serial_rpc.clone();
        let request = tokio::spawn(async move {
            spawn_serial_worker(worker_lock, move || {
                started_tx.send(()).unwrap();
                finish_rx.blocking_recv().unwrap();
            })
            .await
        });

        tokio::time::timeout(Duration::from_secs(1), started_rx)
            .await
            .unwrap()
            .unwrap();
        request.abort();
        tokio::task::yield_now().await;
        assert!(serial_rpc.try_lock().is_err());

        let error =
            spawn_serial_worker_with_timeout(serial_rpc.clone(), Duration::from_millis(25), || ())
                .await
                .unwrap_err();
        assert_eq!(error.error.code, "serial_lock_timeout");

        finish_tx.send(()).unwrap();
        let _serial_rpc = tokio::time::timeout(Duration::from_secs(1), serial_rpc.lock())
            .await
            .unwrap();
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

    #[test]
    fn wifi_config_acceptance_replaces_stale_error_with_connecting_summary() {
        let current = NetworkSummary {
            state: NetworkState::Error,
            configuration_generation: 4,
            transition_sequence: 11,
            failure_code: Some(NetworkFailureCode::AssociationRejected),
            ssid: Some("Old-Network".to_string()),
            wifi_password_length: 8,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: Some("WiFi association failed.".to_string()),
        };
        let payload = WifiConfigRequest {
            lease_id: "lease-1".to_string(),
            op: WifiConfigOp::Set,
            ssid: Some("FluxPurr-Lab".to_string()),
            password: Some("secret-pass".to_string()),
            static_ipv4: None,
            telemetry_interval_ms: Some(500),
        };

        let accepted = mock_network_after_wifi_config(&current, &payload);
        assert_eq!(accepted.state, NetworkState::Connecting);
        assert_eq!(accepted.configuration_generation, 5);
        assert_eq!(accepted.transition_sequence, 12);
        assert_eq!(accepted.ssid.as_deref(), Some("FluxPurr-Lab"));
        assert_eq!(accepted.wifi_password_length, 11);
        assert_eq!(accepted.last_error, None);
    }

    #[test]
    fn wifi_cancel_receipt_preserves_credentials_and_reports_idle() {
        let current = NetworkSummary {
            state: NetworkState::Connecting,
            configuration_generation: 4,
            transition_sequence: 11,
            failure_code: None,
            ssid: Some("FluxPurr-Lab".to_string()),
            wifi_password_length: 11,
            ip: Some("192.168.31.42".to_string()),
            gateway: Some("192.168.31.1".to_string()),
            dns: vec!["1.1.1.1".to_string()],
            wifi_rssi: Some(-48),
            last_error: None,
        };
        let payload = WifiConfigRequest {
            lease_id: "lease-1".to_string(),
            op: WifiConfigOp::Cancel,
            ssid: None,
            password: None,
            static_ipv4: None,
            telemetry_interval_ms: None,
        };

        let cancelled = mock_network_after_wifi_config(&current, &payload);

        assert_eq!(cancelled.state, NetworkState::Idle);
        assert_eq!(
            cancelled.configuration_generation,
            current.configuration_generation
        );
        assert_eq!(
            cancelled.transition_sequence,
            current.transition_sequence + 1
        );
        assert_eq!(cancelled.ssid, current.ssid);
        assert_eq!(cancelled.wifi_password_length, current.wifi_password_length);
        assert_eq!(cancelled.ip, None);
        assert_eq!(cancelled.wifi_rssi, None);
    }

    #[test]
    fn network_snapshot_accepts_the_first_receipt_after_a_device_reboot() {
        let mut current = NetworkSummary {
            state: NetworkState::Connected,
            configuration_generation: 9,
            transition_sequence: 48,
            failure_code: None,
            ssid: Some("FluxPurr-Lab".to_string()),
            wifi_password_length: 8,
            ip: Some("192.168.1.42".to_string()),
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: Some(-42),
            last_error: None,
        };
        current.configuration_generation = 1;
        current.transition_sequence = 2;

        let previous = NetworkSummary {
            configuration_generation: 9,
            transition_sequence: 48,
            ..current.clone()
        };
        assert!(current.is_not_older_than(&previous));
    }

    #[test]
    fn lan_bridge_payload_keeps_the_remote_lease_authoritative() {
        let payload = RuntimeConfigRequest {
            lease_id: "local-lease".to_string(),
            target_temp_c: Some(120),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        };

        let body = lan_bridge_payload(&payload).unwrap();
        assert_eq!(body["targetTempC"], 120);
        assert!(body.get("leaseId").is_none());
    }

    #[test]
    fn lan_bridge_error_preserves_remote_conflict_status_and_code() {
        let error = lan_bridge_error(lan::LanClientError::RemoteApi {
            status: StatusCode::CONFLICT,
            code: "stale_write".to_string(),
            message: "The control state changed after this client last read it.".to_string(),
            retryable: false,
        });

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.error.code, "stale_write");
        assert!(!error.error.retryable);
    }

    #[test]
    fn lan_bridge_error_preserves_remote_unauthorized_status_and_code() {
        let error = lan_bridge_error(lan::LanClientError::RemoteApi {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized".to_string(),
            message: "The pairing token is not authorized for this device.".to_string(),
            retryable: false,
        });

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert_eq!(error.error.code, "unauthorized");
        assert!(!error.error.retryable);
    }

    #[test]
    fn eeprom_maintenance_validation_keeps_raw_chunks_bounded_only_by_transport() {
        let raw = EepromMaintenanceRequest {
            lease_id: "lease-1".to_string(),
            op: EepromMaintenanceOp::Write,
            offset: Some(8_160),
            length: None,
            bytes: Some(vec![0, 255, 17, 34]),
        };
        assert!(validate_eeprom_maintenance_request(&raw).is_ok());

        let out_of_range = EepromMaintenanceRequest {
            offset: Some(8_191),
            bytes: Some(vec![1, 2]),
            ..raw.clone()
        };
        assert_eq!(
            validate_eeprom_maintenance_request(&out_of_range)
                .unwrap_err()
                .error
                .code,
            "eeprom_range_invalid"
        );

        let erase_with_content = EepromMaintenanceRequest {
            op: EepromMaintenanceOp::Erase,
            offset: None,
            length: None,
            bytes: Some(vec![0xff]),
            ..raw
        };
        assert_eq!(
            validate_eeprom_maintenance_request(&erase_with_content)
                .unwrap_err()
                .error
                .code,
            "eeprom_erase_payload_invalid"
        );
    }

    #[test]
    fn eeprom_maintenance_write_ack_omits_read_bytes() {
        let response = EepromMaintenanceResponse { bytes: None };
        let value = serde_json::to_value(response).unwrap();

        assert!(value.get("bytes").is_none());
    }

    #[test]
    fn golden_wifi_fixture_has_monotonic_versioned_snapshots() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../fixtures/wifi-provisioning-v2.json"))
                .unwrap();
        for trace in fixture["traces"].as_array().unwrap() {
            let mut previous = 0;
            for snapshot in trace["snapshots"].as_array().unwrap() {
                assert!(snapshot["configurationGeneration"].as_u64().unwrap() > 0);
                let sequence = snapshot["transitionSequence"].as_u64().unwrap();
                assert!(sequence > previous);
                previous = sequence;
            }
        }
    }

    fn seed_test_bundle(state: &AppState) -> String {
        let output = state.bundle_store.path().join("seed.fluxpurr-fw");
        let bundle = firmware_bundle::build_bundle(
            &output,
            firmware_bundle::BundleIdentity {
                version: "0.1.0".into(),
                source_sha: "e9754917ee23481dd30571fb7a78cb2c486b82a3".into(),
                build_id: "0123456789abcdef".into(),
                channel: firmware_bundle::BundleChannel::Local,
            },
            &vec![0x11; 0x4000],
            include_bytes!("../../../firmware/partitions.bin"),
            &vec![0x33; 0x4000],
            Vec::new(),
        )
        .unwrap();
        let canonical = state.bundle_store.path().join(format!(
            "{}.fluxpurr-fw",
            bundle.bundle_sha256.trim_start_matches("sha256:")
        ));
        fs::rename(output, canonical).unwrap();
        bundle.bundle_sha256
    }

    #[test]
    fn security_info_fails_closed_for_each_protected_state() {
        let safe = RomSecurityInfo {
            rom_mac: "00:11:22:33:44:55".into(),
            secure_boot_enabled: false,
            flash_encryption_enabled: false,
            secure_download_mode_enabled: false,
            response_known: true,
            chip_is_esp32s3: true,
            flash_size_bytes: 4 * 1024 * 1024,
            package_matches: true,
        };
        assert!(safe.validate_for_flash().is_ok());
        for blocked in [
            RomSecurityInfo {
                secure_boot_enabled: true,
                ..safe.clone()
            },
            RomSecurityInfo {
                flash_encryption_enabled: true,
                ..safe.clone()
            },
            RomSecurityInfo {
                secure_download_mode_enabled: true,
                ..safe.clone()
            },
            RomSecurityInfo {
                response_known: false,
                ..safe.clone()
            },
            RomSecurityInfo {
                chip_is_esp32s3: false,
                ..safe.clone()
            },
            RomSecurityInfo {
                flash_size_bytes: 8 * 1024 * 1024,
                ..safe.clone()
            },
            RomSecurityInfo {
                package_matches: false,
                ..safe.clone()
            },
        ] {
            assert!(blocked.validate_for_flash().is_err());
        }
    }

    #[tokio::test]
    async fn recovery_preflight_allows_hot_or_foreign_mock_without_physical_confirmation() {
        let state = AppState::test();
        let artifact_id = seed_test_bundle(&state);
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let result = firmware_operation(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".into()),
            Json(FirmwareOperationRequest {
                lease_id: lease.lease_id,
                artifact_id,
                operation: FirmwareOperation::InstallRecovery,
                dry_run: true,
                approval_token: None,
                confirm: None,
                allow_downgrade: false,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(result.outcome, "passed");
        assert!(result.approval_token.is_some());
        assert_eq!(result.stages, firmware_preflight_stages());
        assert!(!result.stages.contains(&"erase".to_string()));
        assert!(!result.stages.contains(&"write_segments".to_string()));
        assert_eq!(
            firmware_execution_stages(FirmwareOperation::InstallRecovery),
            vec![
                "authorization",
                "erase",
                "write_segments",
                "rom_md5",
                "reset",
                "runtime_reconnect",
                "runtime_verify",
            ]
        );

        let events = state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .events
            .iter()
            .filter(|event| {
                event.kind == "firmware_operation"
                    && event.payload["operationId"] == result.operation_id
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 14);
        for (index, event) in events.iter().enumerate() {
            assert_eq!(event.payload["sequence"], (index + 1) as u64);
            assert_eq!(event.payload["phase"], "preflight");
            assert_eq!(event.payload["operation"], "install_recovery");
        }
        assert_eq!(
            events.first().unwrap().payload["event"],
            "operation_started"
        );
        assert_eq!(
            events.last().unwrap().payload["event"],
            "operation_completed"
        );
        assert_eq!(events.last().unwrap().payload["outcome"], "passed");
    }

    #[tokio::test]
    async fn update_preflight_blocks_active_heater_and_high_temperature() {
        let state = AppState::test();
        let artifact_id = seed_test_bundle(&state);
        let lease = state.lease_device("mock-fp-lab-01").unwrap();
        let error = firmware_operation(
            State(state.clone()),
            AxumPath("mock-fp-lab-01".into()),
            Json(FirmwareOperationRequest {
                lease_id: lease.lease_id,
                artifact_id,
                operation: FirmwareOperation::Update,
                dry_run: true,
                approval_token: None,
                confirm: None,
                allow_downgrade: false,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.error.code, "update_temperature_gate");
        let events = state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .events
            .iter()
            .filter(|event| event.kind == "firmware_operation")
            .cloned()
            .collect::<Vec<_>>();
        let blocked_stage = events
            .iter()
            .rev()
            .find(|event| event.payload["event"] == "stage_failed")
            .expect("update temperature gate must report a failed preflight stage");
        assert_eq!(blocked_stage.payload["stage"], "transport");
        assert_eq!(blocked_stage.payload["code"], "update_temperature_gate");
        assert_eq!(
            events.last().unwrap().payload["event"],
            "operation_completed"
        );
        assert_eq!(events.last().unwrap().payload["outcome"], "blocked");
        assert!(
            events
                .iter()
                .all(|event| event.payload["stage"] != "rom_reset")
        );
        let state_lock = state.lock().unwrap();
        let status = &state_lock.devices.get("mock-fp-lab-01").unwrap().status;
        assert!(!status.heater_enabled);
        assert_eq!(status.heater_output_percent, 0);
    }

    #[test]
    fn update_runtime_gate_uses_live_identity_and_thermal_facts() {
        let cached = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
        let mut live_status = cached.status.clone();
        live_status.heater_enabled = true;
        live_status.current_temp_c = 31.0;

        let error = validate_update_runtime_facts(
            DeviceTransport::NativeSerial,
            "fw/v0.18.3",
            &live_status,
        )
        .unwrap_err();
        assert_eq!(error.error.code, "update_temperature_gate");

        let error =
            validate_update_runtime_facts(DeviceTransport::NativeSerial, "unknown", &cached.status)
                .unwrap_err();
        assert_eq!(error.error.code, "update_identity_required");
    }

    #[test]
    fn firmware_execution_progress_reports_ordered_authoritative_units() {
        let state = AppState::test();
        let mut progress = FirmwareOperationProgress::new(
            &state,
            "mock-fp-lab-01",
            FirmwareOperation::Update,
            "sha256:test",
            false,
        );
        let operation_id = progress.operation_id().to_string();

        progress.operation_started();
        progress.stage_started(
            "write_segments",
            json!({
                "completedUnits": 0,
                "totalUnits": 300,
                "unit": "bytes",
            }),
        );
        progress.stage_progress(
            "write_segments",
            json!({
                "completedUnits": 100,
                "totalUnits": 300,
                "unit": "bytes",
            }),
        );
        progress.stage_completed(
            "write_segments",
            json!({
                "completedUnits": 300,
                "totalUnits": 300,
                "unit": "bytes",
            }),
        );
        progress.operation_completed("verified");

        let events = state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .events
            .iter()
            .filter(|event| {
                event.kind == "firmware_operation" && event.payload["operationId"] == operation_id
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 5);
        assert_eq!(events[2].payload["event"], "stage_progress");
        assert_eq!(events[2].payload["completedUnits"], 100);
        assert_eq!(events[2].payload["totalUnits"], 300);
        assert_eq!(events[2].payload["unit"], "bytes");
        assert!(
            events
                .windows(2)
                .all(|window| window[0].payload["sequence"].as_u64()
                    < window[1].payload["sequence"].as_u64())
        );
    }

    #[test]
    fn rom_md5_keeps_the_existing_rom_session_until_final_reset() {
        let common = vec![
            "--chip".to_string(),
            "esp32s3".to_string(),
            "--port".to_string(),
            "/dev/cu.usbmodem2111401".to_string(),
            "--non-interactive".to_string(),
        ];

        assert_eq!(
            build_checksum_md5_args(&common, 0x10000, 0x200000),
            vec![
                "checksum-md5",
                "--chip",
                "esp32s3",
                "--port",
                "/dev/cu.usbmodem2111401",
                "--non-interactive",
                "--before",
                "no-reset",
                "--after",
                "no-reset",
                "0x10000",
                "2097152",
            ]
        );
    }

    #[test]
    fn install_status_accepts_null_setup_reason_after_commissioning() {
        let status: InstallStatus = serde_json::from_value(json!({
            "layoutId": "flux-purr.esp32s3fh4r2.factory",
            "layoutVersion": 1,
            "partitionTableSha256": "sha256:fec3c8b36e60ece8780cf75b4125a7171d3a3def71d5ca6ac706f4e431391f1e",
            "persistenceSource": "eeprom",
            "recordState": "valid",
            "recordSequence": 7,
            "commissioningRequired": false,
            "setupReason": null,
            "sensorState": "ready",
            "heaterLocked": false
        }))
        .unwrap();

        assert_eq!(status.setup_reason, None);
    }

    #[tokio::test]
    async fn install_status_endpoint_rejects_non_native_transports() {
        let state = AppState::test();

        let error = device_install_status(
            State(state),
            AxumPath("mock-fp-lab-01".to_string()),
            Query(LeaseQuery { lease_id: None }),
        )
        .await
        .expect_err("mock transport must not expose install status");

        assert_eq!(error.error.code, "native_serial_required");
    }
}
