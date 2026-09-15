pub(crate) use super::*;

#[cfg(unix)]
pub(crate) use std::os::fd::AsRawFd;
#[cfg(target_os = "macos")]
pub(crate) use std::os::unix::fs::OpenOptionsExt;
pub(crate) use std::{
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

pub(crate) use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, Method, Request, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post, put},
};
pub(crate) use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
pub(crate) use serde_json::{Value, json};
pub(crate) use sha2::{Digest, Sha256};
pub(crate) use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
    sync::broadcast,
};
pub(crate) use tokio_stream::{StreamExt, wrappers::BroadcastStream};
pub(crate) use tower::ServiceExt;
pub(crate) use tower_http::cors::{AllowOrigin, Any, CorsLayer};

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
pub const DEFAULT_DEVD_ENDPOINT: &str = "flux-purr-devd.sock";
pub(crate) static LOCAL_CONTROL_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) const DEFAULT_PD_REQUEST_MV: u16 = 20_000;
pub(crate) const PPS_HARDWARE_MIN_MV: u16 = 5_000;
pub(crate) const PPS_HARDWARE_MAX_MV: u16 = 28_000;
pub(crate) const AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN: u16 = PPS_HARDWARE_MIN_MV;
pub(crate) const AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT: u16 = 5_000;
pub(crate) const HEATER_PID_TARGET_MIN_C: i16 = 0;
pub(crate) const HEATER_PID_TARGET_MAX_C: i16 = 400;
pub(crate) const THERMAL_PROFILE_ANCHOR_TARGETS_C: [i16; 6] = [60, 100, 140, 180, 220, 250];
pub(crate) const THERMAL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX: u16 = 4_000;
pub(crate) const THERMAL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX: u16 = 375;
pub(crate) const THERMAL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX: u16 = 1_000;
pub(crate) const ADC_CALIBRATION_MAX_SAMPLES: usize = 8;
pub(crate) const HEATER_CURVE_MAX_POINTS: usize = 8;
pub(crate) const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
pub(crate) const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
pub(crate) const USER_CONFIG_FILE: &str = "config.json";
pub(crate) const HARDWARE_REGISTRY_FILE: &str = "devices.json";
pub(crate) const DEFAULT_APP_FLASH_ADDRESS: u64 = 0x10000;
pub(crate) const DEFAULT_PARTITION_TABLE_FLASH_ADDRESS: u64 = 0x8000;
pub(crate) const ESPFLASH_COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
pub(crate) const ESPFLASH_USB_RESET_RETRY_DELAY: Duration = Duration::from_secs(1);
pub(crate) const FRONT_PANEL_PRESET_COUNT: usize = 10;
pub(crate) const SERIAL_RPC_TIMEOUT: Duration = Duration::from_millis(12_000);
pub(crate) const LEASE_REAPER_INTERVAL: Duration = Duration::from_secs(1);
// Opening an ESP32-S3 USB Serial/JTAG port can reset the device. Read-only
// requests are idempotent and must remain alive through USB enumeration,
// front-panel startup, and PD bring-up so the first native CLI query is usable.
// Opening USB Serial/JTAG can reset the MCU. Allow a full cold boot plus
// hardware discovery before declaring a read-only request unavailable.
pub(crate) const SERIAL_READ_ONLY_RPC_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const POST_FLASH_BOOT_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const RUNTIME_READY_BOOT_STAGE: &str = "boot_stage=runtime_ready";
pub(crate) const SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(50);
pub(crate) const SERIAL_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const SERIAL_STARTUP_RETRY_DELAY: Duration = Duration::from_millis(100);
pub(crate) const SERIAL_LINE_LIMIT: usize = 8 * 1024;
// `serialport` configures termios and flushes both queues on macOS. USB
// Serial/JTAG can interpret that control traffic as a host reset, so the
// ESP32-S3 path uses an unconfigured raw descriptor instead.
#[cfg(target_os = "macos")]
pub(crate) const MACOS_O_NONBLOCK: i32 = 0x0004;
#[cfg(unix)]
pub(crate) const LOCK_EX: i32 = 2;
#[cfg(unix)]
pub(crate) const LOCK_NB: i32 = 4;
#[cfg(unix)]
pub(crate) const LOCK_UN: i32 = 8;

pub(crate) static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SerialRetryPolicy {
    ReadOnly,
    SingleShot,
}

#[cfg(unix)]
unsafe extern "C" {
    pub(crate) fn flock(fd: i32, operation: i32) -> i32;
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind: SocketAddr,
    pub control_socket: Option<PathBuf>,
    pub artifact_root: Option<PathBuf>,
    pub allow_dev_cors: bool,
    pub allow_real_flash: bool,
    pub serial_port: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_devd_endpoint: Option<String>,
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
            control_socket: None,
            artifact_root: None,
            allow_dev_cors: true,
            allow_real_flash: false,
            serial_port: None,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) config: AppConfig,
    pub(crate) inner: Arc<Mutex<DevdState>>,
    pub(crate) events: broadcast::Sender<DevdEvent>,
    pub(crate) serial_rpc: Arc<tokio::sync::Mutex<()>>,
    pub(crate) serial_sessions: Arc<Mutex<SerialSessionMap>>,
    pub(crate) bundle_store: Arc<tempfile::TempDir>,
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

    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, DevdState>, HttpError> {
        self.inner
            .lock()
            .map_err(|_| HttpError::internal("state lock poisoned"))
    }

    pub(crate) fn emit(&self, event: DevdEvent) {
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

    pub(crate) async fn reap_expired_leases(&self) -> Result<usize, HttpError> {
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
            remove_expired_serial_sessions(&state, &active_device_ids, &expired, &mut sessions);
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

pub(crate) fn remove_expired_serial_sessions(
    state: &DevdState,
    active_device_ids: &HashSet<&str>,
    expired: &[WebLease],
    sessions: &mut SerialSessionMap,
) {
    for lease in expired {
        if active_device_ids.contains(lease.device_id.as_str()) {
            continue;
        }
        let Some(port_path) = state
            .devices
            .get(&lease.device_id)
            .and_then(|device| device.port_path.as_deref())
        else {
            continue;
        };
        sessions.remove(port_path);
    }
}

#[derive(Debug, Default)]
pub(crate) struct DevdState {
    pub(crate) devices: HashMap<String, DeviceRecord>,
    pub(crate) leases: HashMap<String, WebLease>,
    pub(crate) dry_run_passes: HashMap<String, FlashDryRunApproval>,
    pub(crate) firmware_approvals: HashMap<String, FirmwareApproval>,
    sequence: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct FirmwareApproval {
    pub(crate) lease_id: String,
    pub(crate) device_id: String,
    pub(crate) port_path: String,
    pub(crate) rom_mac: String,
    pub(crate) bundle_sha256: String,
    pub(crate) operation: FirmwareOperation,
    pub(crate) allow_downgrade: bool,
    pub(crate) preflight_digest: String,
    pub(crate) expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlashDryRunApproval {
    pub(crate) lease_id: String,
    pub(crate) artifact_fingerprint: String,
}

impl DevdState {
    pub(crate) fn seed_mock_device(&mut self) {
        let device = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
        self.devices.insert(device.id.clone(), device);
    }

    pub(crate) fn next_id(&mut self, prefix: &str) -> String {
        self.sequence = self.sequence.saturating_add(1);
        format!("{prefix}-{}-{}", now_millis(), self.sequence)
    }

    pub(crate) fn push_event(&mut self, event: DevdEvent) {
        for device in self.devices.values_mut() {
            if event.device_id.as_deref() == Some(&device.id) {
                push_bounded(&mut device.events, event.clone(), DEFAULT_EVENT_LIMIT);
            }
        }
    }

    pub(crate) fn cleanup_leases(&mut self) -> Vec<WebLease> {
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

    pub(crate) fn create_lease(&mut self, device_id: &str) -> Result<WebLease, HttpError> {
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

    pub(crate) fn require_lease(
        &mut self,
        device_id: &str,
        lease_id: Option<&str>,
    ) -> Result<(), HttpError> {
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
    pub(crate) mock_pps_apdos: Vec<MockPpsApdo>,
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
pub(crate) struct MockPpsApdo {
    pub(crate) min_mv: u16,
    pub(crate) max_mv: u16,
    pub(crate) max_ma: u16,
}

pub(crate) fn mock_thermal_plant_snapshot() -> ThermalPlantRunSnapshot {
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

pub(crate) fn mock_identity(id: &str) -> Identity {
    Identity {
        device_id: id.to_string(),
        firmware_version: "fw/v0.4.0-dev".to_string(),
        build_id: "devd-mock".to_string(),
        git_sha: "unknown".to_string(),
        board: "esp32-s3".to_string(),
        api_version: "2026-05-29".to_string(),
        protocol_version: "flux-purr.usb.v1".to_string(),
        hostname: id.to_string(),
        capabilities: [
            "identity",
            "status",
            "network",
            "calibration",
            "thermal_plant_run",
            "wifi_config",
            "wifi_state_v2",
            "monitor",
            "firmware_check",
            "flash",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    }
}

pub(crate) fn mock_network() -> NetworkSummary {
    NetworkSummary {
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
    }
}

pub(crate) fn mock_status(network: &NetworkSummary) -> ControlPlaneStatus {
    ControlPlaneStatus {
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
        post_heat_cooling_mode: "normal".to_string(),
        heating_fan_guard_mode: "medium".to_string(),
        fan_policy_source: "post_heat".to_string(),
        fan_output_level: "medium".to_string(),
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
        persistence_fault: None,
        persistence_fault_attention_pending: false,
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
    }
}

pub(crate) fn native_placeholder_network() -> NetworkSummary {
    NetworkSummary {
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
    }
}

pub(crate) fn native_placeholder_status(network: &NetworkSummary) -> ControlPlaneStatus {
    ControlPlaneStatus {
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
        post_heat_cooling_mode: "normal".to_string(),
        heating_fan_guard_mode: "medium".to_string(),
        fan_policy_source: "idle".to_string(),
        fan_output_level: "off".to_string(),
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
        persistence_fault: None,
        persistence_fault_attention_pending: false,
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
    }
}

pub(crate) fn native_placeholder_identity() -> Identity {
    Identity {
        device_id: String::new(),
        firmware_version: "unknown".to_string(),
        build_id: "native-serial-placeholder".to_string(),
        git_sha: "unknown".to_string(),
        board: "unknown".to_string(),
        api_version: "2026-05-29".to_string(),
        protocol_version: "flux-purr.usb.v1".to_string(),
        hostname: String::new(),
        capabilities: [
            "identity",
            "status",
            "network",
            "thermal_plant_run",
            "wifi_config",
            "monitor",
            "firmware_check",
            "flash",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    }
}

impl DeviceRecord {
    pub(crate) fn mock(id: &str, transport: DeviceTransport) -> Self {
        let identity = mock_identity(id);
        let network = mock_network();
        let status = mock_status(&network);

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

    pub(crate) fn native_serial_placeholder(
        id: &str,
        display_name: String,
        port_path: String,
    ) -> Self {
        let network = native_placeholder_network();
        let status = native_placeholder_status(&network);

        Self {
            id: id.to_string(),
            display_name,
            port_path: Some(port_path),
            transport: DeviceTransport::NativeSerial,
            connection: ConnectionState::Disconnected,
            identity: native_placeholder_identity(),
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

    pub(crate) fn lan_bridge(
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_persistence_fault: Option<PersistenceFault>,
    #[serde(default)]
    pub persistence_fault_attention_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistenceFault {
    pub code: String,
    pub phase: String,
    pub attempt: u8,
    pub sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    pub message: String,
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
    pub(crate) fn is_not_older_than(&self, current: &Self) -> bool {
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
pub(crate) struct UsbWifiConfigReceipt {
    pub(crate) network: NetworkSummary,
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
    #[serde(default)]
    pub post_heat_cooling_mode: String,
    #[serde(default)]
    pub heating_fan_guard_mode: String,
    #[serde(default)]
    pub fan_policy_source: String,
    #[serde(default)]
    pub fan_output_level: String,
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
    pub persistence_fault: Option<PersistenceFault>,
    #[serde(default)]
    pub persistence_fault_attention_pending: bool,
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

pub(crate) fn default_thermal_profile_mode() -> String {
    "65w".to_string()
}

pub(crate) fn default_thermal_profile_resolved_bank() -> String {
    "pps3a".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MockThermalCandidateSettings {
    pub(crate) temp_filter_alpha_permille: u16,
    pub(crate) warmup_reenter_centi_c: u16,
    pub(crate) hold_entry_centi_c: u16,
    pub(crate) hold_exit_centi_c: u16,
    pub(crate) hold_on_centi_c: u16,
    pub(crate) hold_off_centi_c: u16,
    pub(crate) overshoot_cutoff_centi_c: u16,
    pub(crate) approach_max_ticks: u16,
    pub(crate) approach_min_power_ratio_permille: u16,
    pub(crate) hold_kp_permille_per_c: u16,
    pub(crate) hold_ki_permille_per_c_tick: u16,
    pub(crate) hold_blend_ticks: u16,
    pub(crate) hold_reheat_power_permille: u16,
    pub(crate) approach_lead_ticks: u16,
    pub(crate) hold_lead_ticks: u16,
    pub(crate) auto_adjustable_working_floor_mv: u16,
    pub(crate) heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MockThermalCandidatePoint {
    pub(crate) target_temp_c: i16,
    pub(crate) brake_distance_centi_c: u16,
    pub(crate) warmup_power_permille: u16,
    pub(crate) approach_power_permille: u16,
    pub(crate) approach_floor_power_permille: u16,
    pub(crate) approach_damping_exponent_permille: u16,
    pub(crate) approach_tail_window_centi_c: u16,
    pub(crate) hold_power_permille: u16,
    pub(crate) hold_reheat_power_permille: u16,
    pub(crate) warmup_reenter_centi_c: u16,
    pub(crate) hold_entry_centi_c: u16,
    pub(crate) hold_exit_centi_c: u16,
    pub(crate) hold_on_centi_c: u16,
    pub(crate) hold_off_centi_c: u16,
    pub(crate) overshoot_cutoff_centi_c: u16,
    pub(crate) hold_kp_permille_per_c: u16,
    pub(crate) hold_ki_permille_per_c_tick: u16,
    pub(crate) hold_blend_ticks: u16,
    pub(crate) approach_lead_ticks: u16,
    pub(crate) hold_lead_ticks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MockThermalCandidateProfile {
    pub(crate) settings: MockThermalCandidateSettings,
    pub(crate) points: Vec<MockThermalCandidatePoint>,
}
