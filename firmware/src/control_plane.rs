use alloc::boxed::Box;
use heapless::{String, Vec};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    DeviceMode, DeviceStatus, PdState,
    frontpanel::{FRONTPANEL_PRESET_COUNT, FrontPanelKey, HeaterLockReason},
    memory::{
        ADC_CALIBRATION_MAX_SAMPLES, AdcCalibrationChannel, AdcCalibrationFit,
        AdcCalibrationSample, AdcCalibrationSlotFit, AdcCalibrationSlotId, HEATER_CURVE_MAX_POINTS,
        HeaterCurveConfig, HeaterCurvePoint, HeaterCurveRawObservation, HeaterCurveRawObservations,
        MEMORY_WIFI_PASSWORD_MAX_LEN, MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig,
        ThermalControlProfileConfig, ThermalControlProfilePointConfig,
        ThermalControlProfileSettingsConfig, ThermalProfileBank, ThermalProfileMode,
        WifiStaticIpv4Config, adc_calibration_fit,
    },
};

pub const CONTROL_PLANE_API_VERSION: &str = "2026-05-29";
pub const USB_PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const USB_FRAMING: &str = "jsonl";
pub const DEVICE_ID_MAX_LEN: usize = 48;
pub const BUILD_ID_MAX_LEN: usize = 48;
pub const GIT_SHA_MAX_LEN: usize = 40;
pub const HOSTNAME_MAX_LEN: usize = 64;
pub const CAPABILITY_MAX_LEN: usize = 24;
pub const CAPABILITY_COUNT_MAX: usize = 16;
// A fully materialized 9-point thermal profile save request is about 5 KiB.
// Keep one shared bound for firmware and devd JSONL frames so it can persist.
pub const USB_LINE_MAX_LEN: usize = 8 * 1024;
pub const REQUEST_ID_MAX_LEN: usize = 48;
pub const ERROR_CODE_MAX_LEN: usize = 48;
pub const ERROR_MESSAGE_MAX_LEN: usize = 160;
pub const EEPROM_MAINTENANCE_CHUNK_MAX: usize = 32;
pub const THERMAL_TUNING_TRACE_PAGE_MAX: usize = 16;
pub const THERMAL_TUNING_TARGET_COUNT: usize = 9;
pub const THERMAL_TUNING_HASH_HEX_LEN: usize = 64;
pub const THERMAL_TUNING_ID_HEX_LEN: usize = 32;
pub const THERMAL_TUNING_PROFILE_CANONICAL_HEX_LEN: usize =
    flux_purr_thermal_tuning_core::CANDIDATE_PROFILE_CANONICAL_BYTES * 2;
pub const THERMAL_TUNING_CAPABILITY_ID_MAX_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    /// No WiFi credentials are configured. This is the absence state, not a
    /// connection attempt result.
    Disabled,
    /// Legacy/internal state. Firmware WiFi summaries normalize it to
    /// `connecting` when credentials exist.
    Idle,
    /// Internal persistence/disconnect stage; never published by firmware
    /// WiFi summaries.
    Saving,
    Connecting,
    Connected,
    Error,
    /// Legacy/internal timeout state. Firmware WiFi summaries normalize it to
    /// `error` so hardware exposes only connecting/connected/error outcomes.
    Timeout,
}

/// Safe, finite reasons for a terminal WiFi state. Driver text remains
/// diagnostic-only and must never decide user-facing presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSummary {
    pub state: NetworkState,
    #[serde(default)]
    pub configuration_generation: u32,
    #[serde(default)]
    pub transition_sequence: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<NetworkFailureCode>,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    pub wifi_password_length: u8,
    pub ip: Option<String<48>>,
    pub gateway: Option<String<48>>,
    pub dns: Vec<String<48>, 2>,
    pub wifi_rssi: Option<i16>,
    pub last_error: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

impl Default for NetworkSummary {
    fn default() -> Self {
        Self {
            state: NetworkState::Disabled,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String<DEVICE_ID_MAX_LEN>,
    pub firmware_version: String<32>,
    pub build_id: String<BUILD_ID_MAX_LEN>,
    pub git_sha: String<GIT_SHA_MAX_LEN>,
    pub board: String<24>,
    pub api_version: String<16>,
    pub protocol_version: String<24>,
    pub hostname: String<HOSTNAME_MAX_LEN>,
    pub capabilities: Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_tuning: Option<ThermalTuningCapabilityWire>,
}

impl Identity {
    pub fn firmware_default() -> Self {
        let mut capabilities = Vec::new();
        push_str(&mut capabilities, "identity");
        push_str(&mut capabilities, "status");
        push_str(&mut capabilities, "network");
        push_str(&mut capabilities, "calibration");
        push_str(&mut capabilities, "thermal_plant_run");
        push_str(&mut capabilities, "thermal_tuning_run_v1");
        push_str(&mut capabilities, "install_status");
        #[cfg(feature = "web_serial")]
        {
            push_str(&mut capabilities, "usb_jsonl");
            push_str(&mut capabilities, "wifi_config");
            push_str(&mut capabilities, "monitor");
            push_str(&mut capabilities, "wifi_state_v2");
        }
        #[cfg(feature = "net_http")]
        {
            push_str(&mut capabilities, "lan_http");
            push_str(&mut capabilities, "lan_pairing");
        }
        Self {
            device_id: string("flux-purr-s3-001"),
            firmware_version: string(env!("FLUX_PURR_FW_VERSION")),
            build_id: string(env!("FLUX_PURR_BUILD_ID")),
            git_sha: string(env!("FLUX_PURR_SOURCE_SHA")),
            board: string("esp32-s3"),
            api_version: string(CONTROL_PLANE_API_VERSION),
            protocol_version: string(USB_PROTOCOL_VERSION),
            hostname: string("flux-purr-s3-001"),
            capabilities,
            thermal_tuning: Some(thermal_tuning_capability()),
        }
    }

    pub fn firmware_from_mac(mac: [u8; 6]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut identity = Self::firmware_default();
        identity.device_id.clear();
        identity.hostname.clear();
        let _ = identity.hostname.push_str("flux-purr-");
        for byte in mac {
            let high = HEX[usize::from(byte >> 4)] as char;
            let low = HEX[usize::from(byte & 0x0f)] as char;
            let _ = identity.device_id.push(high);
            let _ = identity.device_id.push(low);
            let _ = identity.hostname.push(high);
            let _ = identity.hostname.push(low);
        }
        identity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FanDisplayState {
    Off,
    Auto,
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdcCalibrationSourceWire {
    Efuse,
    RuntimeFallback,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdcDiagnosticsWire {
    pub calibration_source: AdcCalibrationSourceWire,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPlaneStatus {
    pub mode: DeviceModeWire,
    pub uptime_seconds: u32,
    pub current_temp_c: f32,
    pub target_temp_c: i16,
    pub selected_preset_slot: usize,
    pub presets_c: [Option<i16>; FRONTPANEL_PRESET_COUNT],
    pub heater_enabled: bool,
    pub heater_output_percent: u8,
    #[serde(default)]
    pub heater_physical_output_percent: u8,
    pub active_cooling_enabled: bool,
    pub fan_display_state: FanDisplayState,
    pub fan_enabled: bool,
    pub fan_pwm_permille: u16,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
    pub rtd_raw_adc_mv: u16,
    #[serde(default)]
    pub rtd_raw_adc_min_mv: u16,
    #[serde(default)]
    pub rtd_raw_adc_max_mv: u16,
    #[serde(default)]
    pub rtd_raw_adc_spread_mv: u16,
    pub vin_raw_adc_mv: u16,
    #[serde(default)]
    pub adc_diagnostics: Box<AdcDiagnosticsWire>,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: PdStateWire,
    #[serde(default = "default_pd_controller_wire")]
    pub pd_controller: String<ERROR_CODE_MAX_LEN>,
    #[serde(default = "default_pd_contract_kind_wire")]
    pub pd_contract_kind: String<ERROR_CODE_MAX_LEN>,
    #[serde(default)]
    pub pd_contract_current_ma: u16,
    #[serde(default)]
    pub pd_contract_power_mw: u32,
    #[serde(default)]
    pub pd_performance_guaranteed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pd_degraded_reason: Option<String<ERROR_CODE_MAX_LEN>>,
    pub manual_pps_enabled: bool,
    pub manual_pps_mv: Option<u16>,
    pub manual_pps_ma: Option<u16>,
    pub pps_capability_min_mv: Option<u16>,
    pub pps_capability_max_mv: Option<u16>,
    pub pps_capability_max_ma: Option<u16>,
    pub manual_pps_error: Option<String<ERROR_CODE_MAX_LEN>>,
    #[serde(default)]
    pub heater_fault_reason: Option<String<ERROR_CODE_MAX_LEN>>,
    #[serde(default)]
    pub fault_attention_pending: bool,
    pub heater_lock_reason: Option<String<ERROR_CODE_MAX_LEN>>,
    pub heater_control_phase: Option<String<ERROR_CODE_MAX_LEN>>,
    pub heater_error_c: Option<f32>,
    pub heater_control_error_c: Option<f32>,
    #[serde(default)]
    pub heater_control_temp_c: Option<f32>,
    #[serde(default)]
    pub heater_control_measurement_guarded: bool,
    pub heater_filtered_temp_c: Option<f32>,
    #[serde(default)]
    pub heater_filtered_slope_c_per_s: Option<f32>,
    #[serde(default)]
    pub heater_coast_active: bool,
    #[serde(default)]
    pub heater_control_interval_ms: u16,
    #[serde(default)]
    pub heater_control_cycle_ms: u16,
    pub calibration: CalibrationRuntimeStateWire,
    pub thermal_control_profile_preview: bool,
    #[serde(default = "default_thermal_profile_mode_wire")]
    pub thermal_profile_mode: String<ERROR_CODE_MAX_LEN>,
    #[serde(default = "default_thermal_profile_resolved_bank_wire")]
    pub thermal_profile_resolved_bank: String<ERROR_CODE_MAX_LEN>,
    #[serde(
        default,
        skip_serializing_if = "thermal_control_runtime_wire_is_default"
    )]
    pub thermal_control: ThermalControlRuntimeWire,
    #[serde(default)]
    pub thermal_plant_model: ThermalPlantRuntimeWire,
    pub frontpanel_key: Option<FrontPanelKeyWire>,
    pub network: NetworkSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlRuntimeWire {
    pub profile_active: bool,
    pub profile_covers_target: bool,
    pub profile_source: String<ERROR_CODE_MAX_LEN>,
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

fn thermal_control_runtime_wire_is_default(value: &ThermalControlRuntimeWire) -> bool {
    value == &ThermalControlRuntimeWire::default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRuntimeWire {
    pub state: String<ERROR_CODE_MAX_LEN>,
    pub active_transaction_id: Option<u32>,
    pub projection_valid: bool,
    pub convection_mw_per_c: Option<f32>,
    pub radiation_mw_per_k4: Option<f32>,
    pub thermal_capacity_mj_per_c: Option<f32>,
    pub transport_delay_ms: Option<u32>,
}

pub const THERMAL_PLANT_TRACE_PAGE_MAX: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalPlantRunPhaseWire {
    Ambient,
    Heating,
    Cooling,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantTracePointWire {
    pub sample_index: u8,
    pub elapsed_ms: u32,
    pub temperature_centi_c: i16,
    pub heater_voltage_mv: u16,
    pub duty_percent: u8,
    pub phase: ThermalPlantRunPhaseWire,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantTracePageWire {
    pub start_sample: u8,
    pub next_sample: Option<u8>,
    pub total_samples: u8,
    pub points: Vec<ThermalPlantTracePointWire, THERMAL_PLANT_TRACE_PAGE_MAX>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantProvisionalCurveWire {
    pub state: String<16>,
    pub coverage_percent: u8,
    pub curve: HeaterCurvePackageWire,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRunAttemptWire {
    pub run_id: u32,
    pub status: CalibrationJobStatusWire,
    pub phase: Option<ThermalPlantRunPhaseWire>,
    pub progress_percent: u8,
    pub elapsed_ms: u32,
    pub current_temp_centi_c: i16,
    pub heater_voltage_mv: u16,
    pub duty_percent: u8,
    pub sample_count: u8,
    pub restart_allowed: bool,
    pub error: Option<String<ERROR_CODE_MAX_LEN>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantActiveResultWire {
    pub transaction_id: u32,
    pub curve: HeaterCurvePackageWire,
    pub convection_mw_per_c: Option<f32>,
    pub radiation_mw_per_k4: Option<f32>,
    pub thermal_capacity_mj_per_c: Option<f32>,
    pub transport_delay_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRunSnapshotWire {
    pub version: u8,
    pub attempt: Option<ThermalPlantRunAttemptWire>,
    pub trace_page: ThermalPlantTracePageWire,
    pub provisional_curve: Option<ThermalPlantProvisionalCurveWire>,
    pub active_result: Option<ThermalPlantActiveResultWire>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningPowerClassWire {
    Pps3a,
    Pps5a,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTraceCapabilityWire {
    pub paged: bool,
    pub acknowledged: bool,
    pub sealed_review: bool,
    pub buffer_capacity: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningCapabilityWire {
    pub id: String<THERMAL_TUNING_CAPABILITY_ID_MAX_LEN>,
    pub evidence_schema: String<THERMAL_TUNING_CAPABILITY_ID_MAX_LEN>,
    pub supported_power_classes: Vec<ThermalTuningPowerClassWire, 2>,
    pub target_schedule_c: [i16; THERMAL_TUNING_TARGET_COUNT],
    pub physical_targets_c: [i16; THERMAL_TUNING_TARGET_COUNT],
    pub trace: ThermalTuningTraceCapabilityWire,
    pub candidate_promotion: bool,
}

fn thermal_tuning_capability() -> ThermalTuningCapabilityWire {
    let mut supported_power_classes = Vec::new();
    let _ = supported_power_classes.push(ThermalTuningPowerClassWire::Pps3a);
    let _ = supported_power_classes.push(ThermalTuningPowerClassWire::Pps5a);
    ThermalTuningCapabilityWire {
        id: string("thermal_tuning_run_v1"),
        evidence_schema: string("thermal_tuning_evidence_v2"),
        supported_power_classes,
        target_schedule_c: flux_purr_thermal_tuning_core::EXECUTION_ORDER_C,
        physical_targets_c: flux_purr_thermal_tuning_core::PHYSICAL_TARGETS_C,
        trace: ThermalTuningTraceCapabilityWire {
            paged: true,
            acknowledged: true,
            sealed_review: true,
            buffer_capacity: crate::thermal_tuning::THERMAL_TUNING_TRACE_CAPACITY as u16,
        },
        candidate_promotion: true,
    }
}

impl ThermalTuningPowerClassWire {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pps3a => "pps3a",
            Self::Pps5a => "pps5a",
        }
    }
}

impl From<flux_purr_thermal_tuning_core::PpsPowerClass> for ThermalTuningPowerClassWire {
    fn from(value: flux_purr_thermal_tuning_core::PpsPowerClass) -> Self {
        match value {
            flux_purr_thermal_tuning_core::PpsPowerClass::Pps3a => Self::Pps3a,
            flux_purr_thermal_tuning_core::PpsPowerClass::Pps5a => Self::Pps5a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningRunStateWire {
    Idle,
    Running,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningPhaseWire {
    Idle,
    CooldownWait,
    Scout,
    Retune,
    HoldConfirm,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningTerminalDispositionWire {
    Completed,
    Failed,
    Cancelled,
    BudgetExhausted,
    SafetyDisarmed,
    ReviewIncomplete,
    InterruptedReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningReviewStateWire {
    NotApplicable,
    Recording,
    AwaitingSeal,
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningPromotionStateWire {
    Unavailable,
    AwaitingReview,
    Ready,
    Previewed,
    Saved,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningTraceKindWire {
    Sample,
    PhaseTransition,
    CandidateTrial,
    Decision,
    Safety,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningTargetDispositionWire {
    Pending,
    Accepted,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningEligibilityWire {
    pub ready: bool,
    pub reasons: Vec<String<ERROR_CODE_MAX_LEN>, 8>,
    pub active_owner: Option<String<ERROR_CODE_MAX_LEN>>,
}

impl Default for ThermalTuningEligibilityWire {
    fn default() -> Self {
        Self {
            ready: false,
            reasons: Vec::new(),
            active_owner: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTargetProgressWire {
    pub accepted_c: Vec<i16, THERMAL_TUNING_TARGET_COUNT>,
    pub failed_c: Vec<i16, THERMAL_TUNING_TARGET_COUNT>,
    pub skipped_c: Vec<i16, THERMAL_TUNING_TARGET_COUNT>,
}

impl Default for ThermalTuningTargetProgressWire {
    fn default() -> Self {
        Self {
            accepted_c: Vec::new(),
            failed_c: Vec::new(),
            skipped_c: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningReviewWire {
    pub state: ThermalTuningReviewStateWire,
    pub reason: Option<String<ERROR_CODE_MAX_LEN>>,
    pub acknowledged_through: Option<u64>,
    pub terminal_sequence: Option<u64>,
    pub trace_digest: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
}

impl Default for ThermalTuningReviewWire {
    fn default() -> Self {
        Self {
            state: ThermalTuningReviewStateWire::NotApplicable,
            reason: None,
            acknowledged_through: None,
            terminal_sequence: None,
            trace_digest: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningCandidateWire {
    pub candidate_id: Option<String<THERMAL_TUNING_ID_HEX_LEN>>,
    pub candidate_hash: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_profile_hex: Option<String<THERMAL_TUNING_PROFILE_CANONICAL_HEX_LEN>>,
    pub power_class: Option<ThermalTuningPowerClassWire>,
    pub promotion_state: ThermalTuningPromotionStateWire,
}

impl Default for ThermalTuningCandidateWire {
    fn default() -> Self {
        Self {
            candidate_id: None,
            candidate_hash: None,
            canonical_profile_hex: None,
            power_class: None,
            promotion_state: ThermalTuningPromotionStateWire::Unavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningJournalWire {
    pub last_run_id: Option<String<REQUEST_ID_MAX_LEN>>,
    pub last_disposition: Option<ThermalTuningTerminalDispositionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_reason: Option<String<ERROR_CODE_MAX_LEN>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningRunWire {
    pub run_id: String<REQUEST_ID_MAX_LEN>,
    pub state: ThermalTuningRunStateWire,
    pub power_class: Option<ThermalTuningPowerClassWire>,
    pub phase: ThermalTuningPhaseWire,
    pub current_target_c: Option<i16>,
    pub target_progress: ThermalTuningTargetProgressWire,
    pub terminal_disposition: Option<ThermalTuningTerminalDispositionWire>,
    pub eligibility: ThermalTuningEligibilityWire,
    pub review: ThermalTuningReviewWire,
    pub candidate: ThermalTuningCandidateWire,
    pub journal: ThermalTuningJournalWire,
}

impl Default for ThermalTuningRunWire {
    fn default() -> Self {
        Self {
            run_id: string("idle"),
            state: ThermalTuningRunStateWire::Idle,
            power_class: None,
            phase: ThermalTuningPhaseWire::Idle,
            current_target_c: None,
            target_progress: ThermalTuningTargetProgressWire::default(),
            terminal_disposition: None,
            eligibility: ThermalTuningEligibilityWire::default(),
            review: ThermalTuningReviewWire::default(),
            candidate: ThermalTuningCandidateWire::default(),
            journal: ThermalTuningJournalWire::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTraceEventWire {
    pub sequence: u64,
    pub elapsed_ms: u32,
    pub kind: ThermalTuningTraceKindWire,
    pub phase: Option<ThermalTuningPhaseWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_phase: Option<ThermalTuningPhaseWire>,
    pub target_c: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_index: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<String<THERMAL_TUNING_ID_HEX_LEN>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_candidate_point_hex: Option<String<68>>,
    pub temperature_centi_c: Option<i16>,
    pub vin_mv: Option<u16>,
    pub pps_contract_mv: Option<u16>,
    pub pps_contract_ma: Option<u16>,
    pub heater_output_permille: Option<u16>,
    pub measurement_valid: Option<bool>,
    pub disposition: Option<ThermalTuningTargetDispositionWire>,
    pub score_tracking: Option<i32>,
    pub score_energy: Option<i32>,
    pub score_overshoot: Option<i32>,
    pub score_stability: Option<i32>,
    pub score_settle_ms: Option<u32>,
    pub score_hold_mean_absolute_error_centi: Option<i32>,
    pub score_output_switches: Option<u16>,
    pub interval_lower_boundary_c: Option<i16>,
    pub interval_upper_boundary_c: Option<i16>,
    pub interval_pruned: Option<bool>,
    pub candidate_frozen: Option<bool>,
    pub gates: Option<u16>,
    pub candidate_hash: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_reason: Option<String<ERROR_CODE_MAX_LEN>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_start_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_end_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_start_elapsed_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_end_elapsed_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTracePageWire {
    pub earliest_sequence: u64,
    pub emitted_through: Option<u64>,
    pub next_after_sequence: u64,
    pub acknowledged_through: Option<u64>,
    pub digest_through_page: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
    pub events: Box<Vec<ThermalTuningTraceEventWire, THERMAL_TUNING_TRACE_PAGE_MAX>>,
}

impl Default for ThermalTuningTracePageWire {
    fn default() -> Self {
        Self {
            earliest_sequence: 0,
            emitted_through: None,
            next_after_sequence: 0,
            acknowledged_through: None,
            digest_through_page: None,
            events: Box::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningRunSnapshotWire {
    pub schema: String<32>,
    pub run: ThermalTuningRunWire,
    pub page: ThermalTuningTracePageWire,
}

impl Default for ThermalTuningRunSnapshotWire {
    fn default() -> Self {
        Self {
            schema: string("thermal_tuning_run_v1"),
            run: ThermalTuningRunWire::default(),
            page: ThermalTuningTracePageWire::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningRunOpWire {
    Get,
    Start,
    Cancel,
    AckTrace,
    SealReview,
    Preview,
    DiscardPreview,
    Save,
}

impl ThermalTuningRunOpWire {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Start => "start",
            Self::Cancel => "cancel",
            Self::AckTrace => "ack_trace",
            Self::SealReview => "seal_review",
            Self::Preview => "preview",
            Self::DiscardPreview => "discard_preview",
            Self::Save => "save",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningRunCommandWire {
    pub op: ThermalTuningRunOpWire,
    pub request_id: String<REQUEST_ID_MAX_LEN>,
    pub run_id: Option<String<REQUEST_ID_MAX_LEN>>,
    pub power_class: Option<ThermalTuningPowerClassWire>,
    pub after_sequence: Option<u64>,
    pub limit: Option<u16>,
    pub through_sequence: Option<u64>,
    pub trace_digest: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
    pub candidate_id: Option<String<THERMAL_TUNING_ID_HEX_LEN>>,
    pub candidate_hash: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
}

impl ControlPlaneStatus {
    pub fn from_device_status(
        status: DeviceStatus,
        memory: &MemoryConfig,
        uptime_seconds: u32,
        network: NetworkSummary,
    ) -> Self {
        *Self::boxed_from_device_status(status, memory, uptime_seconds, network)
    }

    #[inline(never)]
    pub fn boxed_from_device_status(
        status: DeviceStatus,
        memory: &MemoryConfig,
        uptime_seconds: u32,
        network: NetworkSummary,
    ) -> Box<Self> {
        let mut result = Box::<Self>::new_uninit();
        // Keep the large destination write in a non-inlinable ABI boundary so
        // the caller never materializes `ControlPlaneStatus` on its task stack.
        unsafe {
            flux_purr_write_control_plane_status(
                result.as_mut_ptr(),
                &status,
                memory,
                uptime_seconds,
                &network,
            );
        }
        // SAFETY: `write_device_status` initializes every field before this value is exposed.
        unsafe { result.assume_init() }
    }

    #[inline(never)]
    fn write_device_status(
        out: *mut Self,
        status: DeviceStatus,
        memory: &MemoryConfig,
        uptime_seconds: u32,
        network: NetworkSummary,
    ) {
        let heater_output_percent = status.heater_output_percent.min(100);
        let fan_display_state = if !memory.active_cooling_enabled {
            FanDisplayState::Off
        } else if status.fan_enabled {
            FanDisplayState::Run
        } else {
            FanDisplayState::Auto
        };

        // SAFETY: callers provide an exclusive uninitialized destination.
        unsafe {
            out.write(Self {
                mode: status.mode.into(),
                uptime_seconds,
                current_temp_c: status.board_temp_centi as f32 / 100.0,
                target_temp_c: memory.target_temp_c,
                selected_preset_slot: memory.selected_preset_slot,
                presets_c: memory.presets_c,
                heater_enabled: matches!(status.mode, DeviceMode::Sampling),
                heater_output_percent,
                heater_physical_output_percent: status.heater_physical_output_percent.min(100),
                active_cooling_enabled: memory.active_cooling_enabled,
                fan_display_state,
                fan_enabled: status.fan_enabled,
                fan_pwm_permille: status.fan_pwm_permille,
                voltage_mv: status.voltage_mv,
                current_ma: status.current_ma,
                board_temp_centi: status.board_temp_centi,
                rtd_raw_adc_mv: status.rtd_raw_adc_mv,
                rtd_raw_adc_min_mv: 0,
                rtd_raw_adc_max_mv: 0,
                rtd_raw_adc_spread_mv: 0,
                vin_raw_adc_mv: status.vin_raw_adc_mv,
                adc_diagnostics: Box::new(AdcDiagnosticsWire::default()),
                pd_request_mv: status.pd_request_mv,
                pd_contract_mv: status.pd_contract_mv,
                pd_state: status.pd_state.into(),
                pd_controller: default_pd_controller_wire(),
                pd_contract_kind: default_pd_contract_kind_wire(),
                pd_contract_current_ma: 0,
                pd_contract_power_mw: 0,
                pd_performance_guaranteed: false,
                pd_degraded_reason: Some(default_pd_degraded_reason_wire()),
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
                calibration: CalibrationRuntimeStateWire::default(),
                thermal_control_profile_preview: false,
                thermal_profile_mode: string(memory.thermal_profile_mode.as_str()),
                thermal_profile_resolved_bank: string(
                    memory.thermal_profile_mode.default_bank().as_str(),
                ),
                thermal_control: ThermalControlRuntimeWire::default(),
                thermal_plant_model: ThermalPlantRuntimeWire::default(),
                frontpanel_key: status.frontpanel_key.map(Into::into),
                network,
            });
        }
    }

    pub fn with_runtime_target_temp_c(mut self, target_temp_c: i16) -> Self {
        self.target_temp_c = target_temp_c;
        self
    }
}

/// Writes a complete status snapshot into caller-owned storage.
///
/// The status object is intentionally written through an ABI boundary. This
/// prevents the Xtensa optimizer from lowering a boxed response into a large
/// return-value temporary on the ProCPU executor stack.
///
/// # Safety
///
/// `out` must be valid for an exclusive write of one `ControlPlaneStatus`.
/// `status`, `memory`, and `network` must be valid, aligned pointers to live
/// values for the duration of this call. None of the pointers may alias `out`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flux_purr_write_control_plane_status(
    out: *mut ControlPlaneStatus,
    status: *const DeviceStatus,
    memory: *const MemoryConfig,
    uptime_seconds: u32,
    network: *const NetworkSummary,
) {
    // SAFETY: the caller provides valid, exclusive pointers for the duration
    // of this synchronous write.
    unsafe {
        ControlPlaneStatus::write_device_status(
            out,
            *status,
            &*memory,
            uptime_seconds,
            (*network).clone(),
        );
    }
}

impl From<HeaterLockReason> for String<ERROR_CODE_MAX_LEN> {
    fn from(value: HeaterLockReason) -> Self {
        string(value.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationModeWire {
    Off,
    VinAdc,
    RtdAdc,
    HeaterCurve,
    ThermalPlant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobKindWire {
    VinAdcAuto,
    ThermalPlantAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobStatusWire {
    Idle,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationJobOpWire {
    Start,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationJobStateWire {
    pub kind: Option<CalibrationJobKindWire>,
    pub status: CalibrationJobStatusWire,
    pub progress_percent: u8,
    pub samples_collected: u8,
    pub next_request_mv: Option<u16>,
    pub message: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

impl Default for CalibrationJobStateWire {
    fn default() -> Self {
        Self {
            kind: None,
            status: CalibrationJobStatusWire::Idle,
            progress_percent: 0,
            samples_collected: 0,
            next_request_mv: None,
            message: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationControlCommand {
    pub mode: Option<CalibrationModeWire>,
    pub pps_enabled: Option<bool>,
    pub pps_mv: Option<u16>,
    pub heater_enabled: Option<bool>,
    pub target_adc_mv: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationRuntimeStateWire {
    pub mode: CalibrationModeWire,
    pub pps_enabled: bool,
    pub pps_mv: Option<u16>,
    pub pps_ma: Option<u16>,
    pub heater_enabled: bool,
    pub target_adc_mv: Option<u16>,
    pub stable: bool,
    pub stability_error_mv: Option<i16>,
    pub error: Option<String<ERROR_CODE_MAX_LEN>>,
    pub job: CalibrationJobStateWire,
}

impl Default for CalibrationRuntimeStateWire {
    fn default() -> Self {
        Self {
            mode: CalibrationModeWire::Off,
            pps_enabled: false,
            pps_mv: None,
            pps_ma: None,
            heater_enabled: false,
            target_adc_mv: None,
            stable: false,
            stability_error_mv: None,
            error: None,
            job: CalibrationJobStateWire::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationJobCommandWire {
    pub op: CalibrationJobOpWire,
    pub kind: Option<CalibrationJobKindWire>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModeWire {
    Idle,
    Sampling,
    Fault,
}

impl From<DeviceMode> for DeviceModeWire {
    fn from(value: DeviceMode) -> Self {
        match value {
            DeviceMode::Idle => Self::Idle,
            DeviceMode::Sampling => Self::Sampling,
            DeviceMode::Fault => Self::Fault,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdStateWire {
    Negotiating,
    Ready,
    #[serde(rename = "fallback_5v")]
    Fallback5v,
    Fault,
}

impl From<PdState> for PdStateWire {
    fn from(value: PdState) -> Self {
        match value {
            PdState::Negotiating => Self::Negotiating,
            PdState::Ready => Self::Ready,
            PdState::Fallback5V => Self::Fallback5v,
            PdState::Fault => Self::Fault,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontPanelKeyWire {
    Center,
    Right,
    Down,
    Left,
    Up,
}

impl From<FrontPanelKey> for FrontPanelKeyWire {
    fn from(value: FrontPanelKey) -> Self {
        match value {
            FrontPanelKey::Center => Self::Center,
            FrontPanelKey::Right => Self::Right,
            FrontPanelKey::Down => Self::Down,
            FrontPanelKey::Left => Self::Left,
            FrontPanelKey::Up => Self::Up,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiConfigCommand {
    pub op: WifiConfigOp,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    /// `None` preserves the stored password; `Some("")` explicitly clears it.
    pub password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    /// `None` means the field was absent and therefore preserves the existing
    /// address. `Some(None)` is an explicit JSON null that clears it.
    #[serde(
        default,
        deserialize_with = "deserialize_static_ipv4_patch",
        skip_serializing_if = "Option::is_none"
    )]
    pub static_ipv4: Option<Option<WifiStaticIpv4Wire>>,
    pub telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiStaticIpv4Wire {
    pub address: [u8; 4],
    pub prefix_len: u8,
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

/// A missing `staticIpv4` field preserves the saved address, while an explicit
/// JSON `null` requests DHCP. `serde_json_core` otherwise collapses both into
/// the same nested `Option` value.
fn deserialize_static_ipv4_patch<'de, D>(
    deserializer: D,
) -> Result<Option<Option<WifiStaticIpv4Wire>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<WifiStaticIpv4Wire>::deserialize(deserializer).map(Some)
}

impl From<WifiStaticIpv4Wire> for WifiStaticIpv4Config {
    fn from(value: WifiStaticIpv4Wire) -> Self {
        Self {
            address: value.address,
            prefix_len: value.prefix_len,
            gateway: value.gateway,
            dns: value.dns,
        }
    }
}

impl From<WifiStaticIpv4Config> for WifiStaticIpv4Wire {
    fn from(value: WifiStaticIpv4Config) -> Self {
        Self {
            address: value.address,
            prefix_len: value.prefix_len,
            gateway: value.gateway,
            dns: value.dns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WifiConfigOp {
    Set,
    Clear,
    Cancel,
}

impl WifiConfigCommand {
    pub fn apply_to(&self, config: &mut MemoryConfig) {
        match self.op {
            WifiConfigOp::Clear => {
                config.wifi_ssid.clear();
                config.wifi_password.clear();
                config.wifi_static_ipv4 = None;
            }
            WifiConfigOp::Cancel => {}
            WifiConfigOp::Set => {
                config.wifi_ssid.clear();
                if let Some(ssid) = &self.ssid {
                    let _ = config.wifi_ssid.push_str(ssid);
                }
                if let Some(password) = &self.password {
                    config.wifi_password.clear();
                    let _ = config.wifi_password.push_str(password);
                }
                if let Some(static_ipv4) = self.static_ipv4 {
                    config.wifi_static_ipv4 = static_ipv4.map(Into::into);
                }
                if let Some(interval) = self.telemetry_interval_ms {
                    config.telemetry_interval_ms = interval.max(1);
                }
            }
        }
        config.sanitize();
    }

    pub fn redacted_summary(&self) -> RedactedWifiConfig {
        RedactedWifiConfig {
            op: self.op,
            ssid: self.ssid.clone(),
            password: self.password.as_ref().map(|_| string("<redacted>")),
            static_ipv4: self.static_ipv4.flatten(),
            telemetry_interval_ms: self.telemetry_interval_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedWifiConfig {
    pub op: WifiConfigOp,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    pub password: Option<String<16>>,
    pub static_ipv4: Option<WifiStaticIpv4Wire>,
    pub telemetry_interval_ms: Option<u32>,
}

/// Receipt returned as soon as firmware accepts a WiFi configuration. The
/// network snapshot is authoritative: host adapters must not synthesize it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiConfigReceipt {
    pub wifi: RedactedWifiConfig,
    pub network: NetworkSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigCommand {
    pub target_temp_c: Option<i16>,
    pub selected_preset_slot: Option<usize>,
    pub presets_c: Option<[Option<i16>; FRONTPANEL_PRESET_COUNT]>,
    pub active_cooling_enabled: Option<bool>,
    pub heater_enabled: Option<bool>,
    pub manual_pps_enabled: Option<bool>,
    pub manual_pps_mv: Option<u16>,
    pub manual_pps_ma: Option<u16>,
    pub fault_attention_acknowledged: Option<bool>,
    pub calibration: Option<CalibrationControlCommand>,
    pub thermal_profile_mode: Option<ThermalProfileModeWire>,
    pub thermal_control_profile: Option<ThermalControlProfileCommand>,
}

impl RuntimeConfigCommand {
    pub fn apply_to(&self, config: &mut MemoryConfig) {
        if let Some(target_temp_c) = self.target_temp_c {
            config.target_temp_c = target_temp_c;
        }
        if let Some(selected_preset_slot) = self.selected_preset_slot {
            config.selected_preset_slot = selected_preset_slot;
        }
        if let Some(presets_c) = self.presets_c {
            config.presets_c = presets_c;
            if self.target_temp_c.is_none()
                && let Some(target_temp_c) = config
                    .presets_c
                    .get(config.selected_preset_slot)
                    .and_then(|preset| *preset)
            {
                config.target_temp_c = target_temp_c;
            }
        }
        if let Some(active_cooling_enabled) = self.active_cooling_enabled {
            config.active_cooling_enabled = active_cooling_enabled;
        }
        if let Some(mode) = self.thermal_profile_mode {
            config.thermal_profile_mode = mode.into();
        }
        if let Some(thermal_profile) = self.thermal_control_profile {
            let bank = thermal_profile
                .bank
                .map(Into::into)
                .unwrap_or_else(|| config.thermal_profile_mode.default_bank());
            match thermal_profile.op {
                ThermalControlProfileOp::Save => {
                    if let Some(profile) = thermal_profile.profile {
                        *config.thermal_profile_mut(bank) = profile.into();
                    }
                }
                ThermalControlProfileOp::ClearSaved => {
                    *config.thermal_profile_mut(bank) = ThermalControlProfileConfig::default();
                }
                ThermalControlProfileOp::Preview | ThermalControlProfileOp::ClearPreview => {}
            }
        }
        config.sanitize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThermalProfileModeWire {
    Auto,
    #[serde(rename = "65w")]
    W65,
    #[serde(rename = "100w")]
    W100,
}

impl From<ThermalProfileModeWire> for ThermalProfileMode {
    fn from(value: ThermalProfileModeWire) -> Self {
        match value {
            ThermalProfileModeWire::Auto => Self::Auto,
            ThermalProfileModeWire::W65 => Self::W65,
            ThermalProfileModeWire::W100 => Self::W100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThermalProfileBankWire {
    Pps3a,
    Pps5a,
}

impl From<ThermalProfileBankWire> for ThermalProfileBank {
    fn from(value: ThermalProfileBankWire) -> Self {
        match value {
            ThermalProfileBankWire::Pps3a => Self::Pps3a,
            ThermalProfileBankWire::Pps5a => Self::Pps5a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalControlProfileOp {
    Preview,
    ClearPreview,
    Save,
    ClearSaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfilePointWire {
    pub target_temp_c: i16,
    pub brake_distance_centi_c: u16,
    #[serde(default)]
    pub warmup_power_permille: u16,
    #[serde(default)]
    pub warmup_reenter_centi_c: u16,
    pub approach_power_permille: u16,
    pub approach_floor_power_permille: u16,
    #[serde(default = "default_approach_damping_exponent_permille_wire")]
    pub approach_damping_exponent_permille: u16,
    #[serde(default)]
    pub approach_tail_window_centi_c: u16,
    pub hold_power_permille: u16,
    #[serde(default)]
    pub hold_reheat_power_permille: u16,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfileWire {
    pub settings: Option<ThermalControlProfileSettingsWire>,
    pub points: [Option<ThermalControlProfilePointWire>; FRONTPANEL_PRESET_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfileSettingsWire {
    pub temp_filter_alpha_permille: u16,
    pub warmup_reenter_centi_c: u16,
    pub hold_entry_centi_c: u16,
    pub hold_exit_centi_c: u16,
    pub hold_on_centi_c: u16,
    pub hold_off_centi_c: u16,
    pub overshoot_cutoff_centi_c: u16,
    pub approach_max_ticks: u16,
    pub approach_min_power_ratio_permille: u16,
    pub hold_kp_permille_per_c: u16,
    pub hold_ki_permille_per_c_tick: u16,
    #[serde(default = "default_hold_blend_ticks_wire")]
    pub hold_blend_ticks: u16,
    #[serde(default)]
    pub hold_reheat_power_permille: u16,
    #[serde(default)]
    pub approach_lead_ticks: u16,
    #[serde(default)]
    pub hold_lead_ticks: u16,
    #[serde(default = "default_auto_adjustable_working_floor_mv_wire")]
    pub auto_adjustable_working_floor_mv: u16,
    #[serde(default = "default_heater_current_reserve_ma_wire")]
    pub heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalControlProfileCommand {
    pub op: ThermalControlProfileOp,
    #[serde(default)]
    pub bank: Option<ThermalProfileBankWire>,
    pub profile: Option<ThermalControlProfileWire>,
}

fn default_pd_controller_wire() -> String<ERROR_CODE_MAX_LEN> {
    string("unknown")
}

fn default_pd_contract_kind_wire() -> String<ERROR_CODE_MAX_LEN> {
    string("none")
}

fn default_pd_degraded_reason_wire() -> String<ERROR_CODE_MAX_LEN> {
    string("pd_contract_unavailable")
}

fn default_thermal_profile_mode_wire() -> String<ERROR_CODE_MAX_LEN> {
    string("65w")
}

fn default_thermal_profile_resolved_bank_wire() -> String<ERROR_CODE_MAX_LEN> {
    string("pps3a")
}

impl From<ThermalControlProfilePointWire> for ThermalControlProfilePointConfig {
    fn from(value: ThermalControlProfilePointWire) -> Self {
        Self {
            target_temp_c: value.target_temp_c,
            brake_distance_centi_c: value.brake_distance_centi_c,
            warmup_power_permille: value.warmup_power_permille,
            warmup_reenter_centi_c: value.warmup_reenter_centi_c,
            approach_power_permille: value.approach_power_permille,
            approach_floor_power_permille: value.approach_floor_power_permille,
            approach_damping_exponent_permille: value.approach_damping_exponent_permille,
            approach_tail_window_centi_c: value.approach_tail_window_centi_c,
            hold_power_permille: value.hold_power_permille,
            hold_reheat_power_permille: value.hold_reheat_power_permille,
            hold_entry_centi_c: value.hold_entry_centi_c,
            hold_exit_centi_c: value.hold_exit_centi_c,
            hold_on_centi_c: value.hold_on_centi_c,
            hold_off_centi_c: value.hold_off_centi_c,
            overshoot_cutoff_centi_c: value.overshoot_cutoff_centi_c,
            hold_kp_permille_per_c: value.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: value.hold_ki_permille_per_c_tick,
            hold_blend_ticks: value.hold_blend_ticks,
            approach_lead_ticks: value.approach_lead_ticks,
            hold_lead_ticks: value.hold_lead_ticks,
        }
    }
}

impl From<ThermalControlProfileWire> for ThermalControlProfileConfig {
    fn from(value: ThermalControlProfileWire) -> Self {
        let mut config = ThermalControlProfileConfig::default();
        if let Some(settings) = value.settings {
            config.settings = settings.into();
        }
        for (index, point) in value.points.into_iter().enumerate() {
            config.points[index] = point.map(Into::into);
        }
        config
    }
}

impl From<ThermalControlProfileSettingsWire> for ThermalControlProfileSettingsConfig {
    fn from(value: ThermalControlProfileSettingsWire) -> Self {
        Self {
            temp_filter_alpha_permille: value.temp_filter_alpha_permille,
            warmup_reenter_centi_c: value.warmup_reenter_centi_c,
            hold_entry_centi_c: value.hold_entry_centi_c,
            hold_exit_centi_c: value.hold_exit_centi_c,
            hold_on_centi_c: value.hold_on_centi_c,
            hold_off_centi_c: value.hold_off_centi_c,
            overshoot_cutoff_centi_c: value.overshoot_cutoff_centi_c,
            approach_max_ticks: value.approach_max_ticks,
            approach_min_power_ratio_permille: value.approach_min_power_ratio_permille,
            hold_kp_permille_per_c: value.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: value.hold_ki_permille_per_c_tick,
            hold_blend_ticks: value.hold_blend_ticks,
            hold_reheat_power_permille: value.hold_reheat_power_permille,
            approach_lead_ticks: value.approach_lead_ticks,
            hold_lead_ticks: value.hold_lead_ticks,
            auto_adjustable_working_floor_mv: value.auto_adjustable_working_floor_mv,
            heater_current_reserve_ma: value.heater_current_reserve_ma,
        }
    }
}

const fn default_hold_blend_ticks_wire() -> u16 {
    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT
}

const fn default_approach_damping_exponent_permille_wire() -> u16 {
    crate::memory::THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT
}

const fn default_auto_adjustable_working_floor_mv_wire() -> u16 {
    crate::memory::THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT
}

const fn default_heater_current_reserve_ma_wire() -> u16 {
    crate::memory::THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_DEFAULT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationChannelWire {
    RtdAdc,
    VinAdc,
}

impl CalibrationChannelWire {
    pub const fn as_memory_channel(self) -> AdcCalibrationChannel {
        match self {
            Self::RtdAdc => AdcCalibrationChannel::Rtd,
            Self::VinAdc => AdcCalibrationChannel::Vin,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSampleWire {
    pub observed_mv: u16,
    pub expected_mv: u16,
    pub reference_temp_c: Option<f32>,
    pub target_adc_mv: Option<u16>,
    pub reference_vin_mv: Option<u16>,
}

impl From<AdcCalibrationSample> for CalibrationSampleWire {
    fn from(value: AdcCalibrationSample) -> Self {
        Self {
            observed_mv: value.observed_mv,
            expected_mv: value.expected_mv,
            reference_temp_c: value.reference_temp_deci_c.map(|value| value as f32 / 10.0),
            target_adc_mv: value.target_adc_mv,
            reference_vin_mv: value.reference_vin_mv,
        }
    }
}

impl From<CalibrationSampleWire> for AdcCalibrationSample {
    fn from(value: CalibrationSampleWire) -> Self {
        Self {
            observed_mv: value.observed_mv,
            expected_mv: value.expected_mv,
            reference_temp_deci_c: value.reference_temp_c.map(|temp_c| {
                let scaled = if temp_c >= 0.0 {
                    temp_c * 10.0 + 0.5
                } else {
                    temp_c * 10.0 - 0.5
                };
                (scaled as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
            }),
            target_adc_mv: value.target_adc_mv,
            reference_vin_mv: value.reference_vin_mv,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationFitWire {
    pub gain: f32,
    pub offset_mv: f32,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSlotFitWire {
    pub gain: f32,
    pub offset_mv: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationSlotIdWire {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSlotSetWire {
    pub a: CalibrationSlotFitWire,
    pub b: CalibrationSlotFitWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationChannelStateWire {
    pub samples: [Option<CalibrationSampleWire>; ADC_CALIBRATION_MAX_SAMPLES],
    pub fitted_fit: CalibrationFitWire,
    pub slots: CalibrationSlotSetWire,
    pub active_slot: CalibrationSlotIdWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationStateWire {
    pub rtd_adc: CalibrationChannelStateWire,
    pub vin_adc: CalibrationChannelStateWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePointWire {
    pub temp_centi_c: i16,
    pub resistance_milliohms: u16,
}

impl From<HeaterCurvePoint> for HeaterCurvePointWire {
    fn from(value: HeaterCurvePoint) -> Self {
        Self {
            temp_centi_c: value.temp_centi_c,
            resistance_milliohms: value.resistance_milliohms,
        }
    }
}

impl From<HeaterCurvePointWire> for HeaterCurvePoint {
    fn from(value: HeaterCurvePointWire) -> Self {
        Self {
            temp_centi_c: value.temp_centi_c,
            resistance_milliohms: value.resistance_milliohms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurvePackageWire {
    pub points: [Option<HeaterCurvePointWire>; HEATER_CURVE_MAX_POINTS],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_observations: Option<HeaterCurveRawObservationsWire>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveRawObservationWire {
    pub raw_rtd_adc_mv: u16,
    pub heater_voltage_mv: u16,
    pub heater_current_ma: u16,
    pub resistance_milliohms: u16,
}

impl From<HeaterCurveRawObservation> for HeaterCurveRawObservationWire {
    fn from(value: HeaterCurveRawObservation) -> Self {
        Self {
            raw_rtd_adc_mv: value.raw_rtd_adc_mv,
            heater_voltage_mv: value.heater_voltage_mv,
            heater_current_ma: value.heater_current_ma,
            resistance_milliohms: value.resistance_milliohms,
        }
    }
}

impl From<HeaterCurveRawObservationWire> for HeaterCurveRawObservation {
    fn from(value: HeaterCurveRawObservationWire) -> Self {
        Self {
            raw_rtd_adc_mv: value.raw_rtd_adc_mv,
            heater_voltage_mv: value.heater_voltage_mv,
            heater_current_ma: value.heater_current_ma,
            resistance_milliohms: value.resistance_milliohms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveRawObservationsWire {
    pub points: [Option<HeaterCurveRawObservationWire>; HEATER_CURVE_MAX_POINTS],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveStateWire {
    pub active: HeaterCurvePackageWire,
    pub preview: Option<HeaterCurvePackageWire>,
    #[serde(default)]
    pub eeprom_probe: Option<HeaterCurveEepromProbeWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveEepromProbeWire {
    pub present: bool,
    #[serde(default)]
    pub current_read_present: bool,
    #[serde(default)]
    pub random_read_present: bool,
    pub address: Option<u8>,
    #[serde(default)]
    pub bus_current_read_addresses: [Option<u8>; 16],
    pub last_error: Option<String<ERROR_CODE_MAX_LEN>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaterCurveConfigOp {
    Preview,
    ClearPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaterCurveConfigCommand {
    pub op: HeaterCurveConfigOp,
    pub package: Option<HeaterCurvePackageWire>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationConfigOp {
    Capture,
    Delete,
    Clear,
    Import,
    SetActiveSlot,
    SetSlotFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationConfigCommand {
    pub op: CalibrationConfigOp,
    pub channel: Option<CalibrationChannelWire>,
    pub reference_temp_c: Option<f32>,
    pub reference_vin_mv: Option<u32>,
    pub target_adc_mv: Option<u16>,
    pub observed_mv: Option<u16>,
    pub expected_mv: Option<u16>,
    pub sample_index: Option<usize>,
    pub state: Option<CalibrationStateWire>,
    pub slot: Option<CalibrationSlotIdWire>,
    pub fit: Option<CalibrationSlotFitWire>,
}

impl HeaterCurvePackageWire {
    pub fn from_memory(
        config: &HeaterCurveConfig,
        raw_observations: Option<&HeaterCurveRawObservations>,
    ) -> Self {
        let mut points = [None; HEATER_CURVE_MAX_POINTS];
        for (index, point) in config.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        let raw_observations = raw_observations.map(|raw_observations| {
            let mut points = [None; HEATER_CURVE_MAX_POINTS];
            for (index, point) in raw_observations.points.into_iter().enumerate() {
                points[index] = point.map(Into::into);
            }
            HeaterCurveRawObservationsWire { points }
        });
        Self {
            points,
            raw_observations,
        }
    }

    pub fn to_memory(self) -> HeaterCurveConfig {
        let mut points = [None; HEATER_CURVE_MAX_POINTS];
        for (index, point) in self.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        HeaterCurveConfig { points }
    }

    pub fn raw_observations_to_memory(self) -> Option<HeaterCurveRawObservations> {
        self.raw_observations.map(|raw_observations| {
            let mut points = [None; HEATER_CURVE_MAX_POINTS];
            for (index, point) in raw_observations.points.into_iter().enumerate() {
                points[index] = point.map(Into::into);
            }
            HeaterCurveRawObservations { points }
        })
    }
}

pub fn calibration_state_from_memory(config: &MemoryConfig) -> CalibrationStateWire {
    CalibrationStateWire {
        rtd_adc: CalibrationChannelStateWire::from_memory(
            config.adc_calibration.rtd.samples,
            adc_calibration_fit(&config.adc_calibration, AdcCalibrationChannel::Rtd),
            config.adc_calibration.rtd.slots,
            config.adc_calibration.rtd.active_slot,
        ),
        vin_adc: CalibrationChannelStateWire::from_memory(
            config.adc_calibration.vin.samples,
            adc_calibration_fit(&config.adc_calibration, AdcCalibrationChannel::Vin),
            config.adc_calibration.vin.slots,
            config.adc_calibration.vin.active_slot,
        ),
    }
}

pub fn heater_curve_state_from_memory(
    config: &MemoryConfig,
    preview: Option<(&HeaterCurveConfig, Option<&HeaterCurveRawObservations>)>,
) -> HeaterCurveStateWire {
    HeaterCurveStateWire {
        active: HeaterCurvePackageWire::from_memory(
            &config.active_heater_curve,
            Some(&config.heater_curve_raw_observations),
        ),
        preview: preview.map(|(curve, raw_observations)| {
            HeaterCurvePackageWire::from_memory(curve, raw_observations)
        }),
        eeprom_probe: None,
    }
}

impl CalibrationFitWire {
    fn from_memory(fit: AdcCalibrationFit) -> Self {
        Self {
            gain: fit.gain,
            offset_mv: fit.offset_mv,
            sample_count: fit.sample_count,
        }
    }
}

impl CalibrationSlotFitWire {
    fn from_memory(fit: AdcCalibrationSlotFit) -> Self {
        Self {
            gain: fit.gain,
            offset_mv: fit.offset_mv,
        }
    }

    pub fn to_memory(self) -> AdcCalibrationSlotFit {
        AdcCalibrationSlotFit {
            gain: self.gain,
            offset_mv: self.offset_mv,
        }
    }
}

impl From<AdcCalibrationSlotId> for CalibrationSlotIdWire {
    fn from(value: AdcCalibrationSlotId) -> Self {
        match value {
            AdcCalibrationSlotId::A => Self::A,
            AdcCalibrationSlotId::B => Self::B,
        }
    }
}

impl From<CalibrationSlotIdWire> for AdcCalibrationSlotId {
    fn from(value: CalibrationSlotIdWire) -> Self {
        match value {
            CalibrationSlotIdWire::A => Self::A,
            CalibrationSlotIdWire::B => Self::B,
        }
    }
}

impl CalibrationSlotSetWire {
    fn from_memory(slots: crate::memory::AdcCalibrationSlots) -> Self {
        Self {
            a: CalibrationSlotFitWire::from_memory(slots.a),
            b: CalibrationSlotFitWire::from_memory(slots.b),
        }
    }
}

impl CalibrationChannelStateWire {
    fn from_memory(
        samples: [Option<AdcCalibrationSample>; ADC_CALIBRATION_MAX_SAMPLES],
        fitted_fit: AdcCalibrationFit,
        slots: crate::memory::AdcCalibrationSlots,
        active_slot: AdcCalibrationSlotId,
    ) -> Self {
        Self {
            samples: samples_to_wire(samples),
            fitted_fit: CalibrationFitWire::from_memory(fitted_fit),
            slots: CalibrationSlotSetWire::from_memory(slots),
            active_slot: active_slot.into(),
        }
    }
}

fn samples_to_wire(
    samples: [Option<AdcCalibrationSample>; ADC_CALIBRATION_MAX_SAMPLES],
) -> [Option<CalibrationSampleWire>; ADC_CALIBRATION_MAX_SAMPLES] {
    let mut out = [None; ADC_CALIBRATION_MAX_SAMPLES];
    for (index, sample) in samples.into_iter().enumerate() {
        out[index] = sample.map(Into::into);
    }
    out
}

pub fn samples_from_wire(
    samples: [Option<CalibrationSampleWire>; ADC_CALIBRATION_MAX_SAMPLES],
) -> [Option<AdcCalibrationSample>; ADC_CALIBRATION_MAX_SAMPLES] {
    let mut out = [None; ADC_CALIBRATION_MAX_SAMPLES];
    for (index, sample) in samples.into_iter().enumerate() {
        out[index] = sample.map(Into::into);
    }
    out
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UsbFrame {
    Hello {
        protocol_version: String<24>,
        framing: String<8>,
        identity: Identity,
        capabilities: Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>,
    },
    Request {
        request_id: String<REQUEST_ID_MAX_LEN>,
        op: UsbRequestOp,
    },
    WifiConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: WifiConfigCommand,
    },
    RuntimeConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: RuntimeConfigCommand,
    },
    CalibrationConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: CalibrationConfigCommand,
    },
    CalibrationJob {
        request_id: String<REQUEST_ID_MAX_LEN>,
        command: CalibrationJobCommandWire,
    },
    ThermalPlantRun {
        request_id: String<REQUEST_ID_MAX_LEN>,
        after_sample: u8,
    },
    ThermalTuningRun {
        command: ThermalTuningRunCommandWire,
    },
    HeaterCurveConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: HeaterCurveConfigCommand,
    },
    HeaterCurveSave {
        request_id: String<REQUEST_ID_MAX_LEN>,
    },
    EepromMaintenance {
        request_id: String<REQUEST_ID_MAX_LEN>,
        command: EepromMaintenanceCommand,
    },
    Response {
        request_id: String<REQUEST_ID_MAX_LEN>,
        ok: bool,
        result: Option<UsbResponsePayload>,
        error: Option<ApiError>,
    },
    Status {
        status: Box<ControlPlaneStatus>,
    },
    Log {
        level: String<8>,
        message: String<ERROR_MESSAGE_MAX_LEN>,
    },
    Error {
        request_id: Option<String<REQUEST_ID_MAX_LEN>>,
        error: ApiError,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbFrameWire {
    #[serde(rename = "type")]
    frame_type: String<24>,
    #[serde(rename = "protocolVersion", skip_serializing_if = "Option::is_none")]
    protocol_version: Option<String<24>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    framing: Option<String<8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<Box<Identity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Box<Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>>>,
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    op: Option<String<24>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    #[serde(
        default,
        deserialize_with = "deserialize_static_ipv4_patch",
        skip_serializing_if = "Option::is_none"
    )]
    static_ipv4: Option<Option<WifiStaticIpv4Wire>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    telemetry_interval_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_temp_c: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_preset_slot: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presets_c: Option<[Option<i16>; FRONTPANEL_PRESET_COUNT]>,
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
    calibration: Option<Box<CalibrationControlCommand>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thermal_profile_mode: Option<ThermalProfileModeWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thermal_control_profile: Option<Box<ThermalControlProfileCommand>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<CalibrationChannelWire>,
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
    #[serde(
        rename = "kind",
        alias = "jobKind",
        skip_serializing_if = "Option::is_none"
    )]
    job_kind: Option<CalibrationJobKindWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_sample: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    through_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_digest: Option<Box<String<THERMAL_TUNING_HASH_HEX_LEN>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<Box<String<REQUEST_ID_MAX_LEN>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    power_class: Option<ThermalTuningPowerClassWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_id: Option<Box<String<THERMAL_TUNING_ID_HEX_LEN>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_hash: Option<Box<String<THERMAL_TUNING_HASH_HEX_LEN>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<Box<CalibrationStateWire>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<CalibrationSlotIdWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<CalibrationSlotFitWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    heater_curve: Option<Box<HeaterCurvePackageWire>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    length: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<Vec<u8, EEPROM_MAINTENANCE_CHUNK_MAX>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Box<UsbResponsePayload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ApiError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<Box<ControlPlaneStatus>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<String<8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<Box<String<ERROR_MESSAGE_MAX_LEN>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbStatusResponseWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a String<REQUEST_ID_MAX_LEN>,
    ok: bool,
    result: UsbStatusPayloadWire<'a>,
}

#[derive(Serialize)]
struct UsbStatusPayloadWire<'a> {
    status: &'a ControlPlaneStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbResponseFrameWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    #[serde(rename = "requestId")]
    request_id: &'a String<REQUEST_ID_MAX_LEN>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a UsbResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a ApiError>,
}

/// Narrow response wire for firmware-owned thermal tuning.
///
/// `UsbFrame` intentionally supports every control-plane payload and is too
/// large to materialize on the front-panel task stack while a tuning snapshot
/// is being returned. This wire references the PSRAM-backed snapshot directly.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsbThermalTuningResponseFrameWire<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    #[serde(rename = "requestId")]
    request_id: &'a String<REQUEST_ID_MAX_LEN>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ThermalTuningResponsePayloadWire<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a ApiError>,
}

#[derive(Serialize)]
struct ThermalTuningResponsePayloadWire<'a> {
    #[serde(rename = "thermal_tuning_run")]
    snapshot: &'a ThermalTuningRunSnapshotWire,
}

// Inbound frames deliberately use a narrow, type-specific parser.  Keeping
// every optional field in `UsbFrameWire` is convenient for serialization, but
// makes the first status request materialize every control-plane shape on the
// ProCPU stack.  The front-panel task has a finite hardware stack, so parse the
// discriminator first and construct only the requested frame variant.
#[derive(Deserialize)]
struct UsbFrameTypeWire {
    #[serde(rename = "type")]
    frame_type: String<24>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbHelloInboundWire {
    protocol_version: Option<String<24>>,
    framing: Option<String<8>>,
    identity: Option<Identity>,
    capabilities: Option<Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbRequestInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<24>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbWifiConfigHeaderWire<'a> {
    request_id: Option<&'a str>,
    op: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbWifiConfigSetInboundWire<'a> {
    request_id: Option<&'a str>,
    ssid: Option<&'a str>,
    password: Option<&'a str>,
    #[serde(default, deserialize_with = "deserialize_static_ipv4_patch")]
    static_ipv4: Option<Option<WifiStaticIpv4Wire>>,
    telemetry_interval_ms: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbRuntimeConfigInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    target_temp_c: Option<i16>,
    selected_preset_slot: Option<usize>,
    presets_c: Option<[Option<i16>; FRONTPANEL_PRESET_COUNT]>,
    active_cooling_enabled: Option<bool>,
    heater_enabled: Option<bool>,
    manual_pps_enabled: Option<bool>,
    manual_pps_mv: Option<u16>,
    manual_pps_ma: Option<u16>,
    fault_attention_acknowledged: Option<bool>,
    calibration: Option<CalibrationControlCommand>,
    thermal_profile_mode: Option<ThermalProfileModeWire>,
    thermal_control_profile: Option<ThermalControlProfileCommand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbCalibrationConfigInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<24>>,
    channel: Option<CalibrationChannelWire>,
    reference_temp_c: Option<f32>,
    reference_vin_mv: Option<u32>,
    target_adc_mv: Option<u16>,
    observed_mv: Option<u16>,
    expected_mv: Option<u16>,
    sample_index: Option<usize>,
    state: Option<CalibrationStateWire>,
    slot: Option<CalibrationSlotIdWire>,
    fit: Option<CalibrationSlotFitWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbCalibrationJobInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<24>>,
    #[serde(rename = "kind", alias = "jobKind")]
    kind: Option<CalibrationJobKindWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbThermalPlantRunInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    after_sample: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbThermalTuningRunInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<ThermalTuningRunOpWire>,
    run_id: Option<String<REQUEST_ID_MAX_LEN>>,
    power_class: Option<ThermalTuningPowerClassWire>,
    after_sequence: Option<u64>,
    limit: Option<u16>,
    through_sequence: Option<u64>,
    trace_digest: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
    candidate_id: Option<String<THERMAL_TUNING_ID_HEX_LEN>>,
    candidate_hash: Option<String<THERMAL_TUNING_HASH_HEX_LEN>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbHeaterCurveConfigInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<24>>,
    heater_curve: Option<HeaterCurvePackageWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbRequestIdInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EepromMaintenanceOp {
    Read,
    Write,
    Erase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EepromMaintenanceCommand {
    pub op: EepromMaintenanceOp,
    pub offset: Option<u16>,
    pub length: Option<u8>,
    pub bytes: Option<Vec<u8, EEPROM_MAINTENANCE_CHUNK_MAX>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbEepromMaintenanceInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<EepromMaintenanceOp>,
    offset: Option<u16>,
    length: Option<u8>,
    bytes: Option<Vec<u8, EEPROM_MAINTENANCE_CHUNK_MAX>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbResponseInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    ok: Option<bool>,
    result: Option<UsbResponsePayload>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct UsbStatusInboundWire {
    status: Option<ControlPlaneStatus>,
}

#[derive(Deserialize)]
struct UsbLogInboundWire {
    level: Option<String<8>>,
    message: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbErrorInboundWire {
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    error: Option<ApiError>,
}

impl TryFrom<UsbFrameWire> for UsbFrame {
    type Error = UsbFrameError;

    fn try_from(value: UsbFrameWire) -> Result<Self, <UsbFrame as TryFrom<UsbFrameWire>>::Error> {
        match value.frame_type.as_str() {
            "hello" => Ok(UsbFrame::Hello {
                protocol_version: value.protocol_version.ok_or(UsbFrameError::MalformedJson)?,
                framing: value.framing.ok_or(UsbFrameError::MalformedJson)?,
                identity: *value.identity.ok_or(UsbFrameError::MalformedJson)?,
                capabilities: *value.capabilities.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "request" => Ok(UsbFrame::Request {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                op: parse_usb_request_op(value.op.as_deref())?,
            }),
            "wifi_config" => Ok(UsbFrame::WifiConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: WifiConfigCommand {
                    op: parse_wifi_config_op(value.op.as_deref())?,
                    ssid: value.ssid,
                    password: value.password,
                    static_ipv4: value.static_ipv4,
                    telemetry_interval_ms: value.telemetry_interval_ms,
                },
            }),
            "runtime_config" => Ok(UsbFrame::RuntimeConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: RuntimeConfigCommand {
                    target_temp_c: value.target_temp_c,
                    selected_preset_slot: value.selected_preset_slot,
                    presets_c: value.presets_c,
                    active_cooling_enabled: value.active_cooling_enabled,
                    heater_enabled: value.heater_enabled,
                    manual_pps_enabled: value.manual_pps_enabled,
                    manual_pps_mv: value.manual_pps_mv,
                    manual_pps_ma: value.manual_pps_ma,
                    fault_attention_acknowledged: value.fault_attention_acknowledged,
                    calibration: value.calibration.map(|calibration| *calibration),
                    thermal_profile_mode: value.thermal_profile_mode,
                    thermal_control_profile: value.thermal_control_profile.map(|profile| *profile),
                },
            }),
            "calibration_config" => Ok(UsbFrame::CalibrationConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: CalibrationConfigCommand {
                    op: parse_calibration_config_op(value.op.as_deref())?,
                    channel: value.channel,
                    reference_temp_c: value.reference_temp_c,
                    reference_vin_mv: value.reference_vin_mv,
                    target_adc_mv: value.target_adc_mv,
                    observed_mv: value.observed_mv,
                    expected_mv: value.expected_mv,
                    sample_index: value.sample_index,
                    state: value.state.map(|state| *state),
                    slot: value.slot,
                    fit: value.fit,
                },
            }),
            "calibration_job" => Ok(UsbFrame::CalibrationJob {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                command: CalibrationJobCommandWire {
                    op: parse_calibration_job_op(value.op.as_deref())?,
                    kind: value.job_kind,
                },
            }),
            "thermal_plant_run" => Ok(UsbFrame::ThermalPlantRun {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                after_sample: value.after_sample.unwrap_or(0),
            }),
            "thermal_tuning_run" => Ok(UsbFrame::ThermalTuningRun {
                command: ThermalTuningRunCommandWire {
                    op: parse_thermal_tuning_run_op(value.op.as_deref())?,
                    request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                    run_id: value.run_id.map(|value| *value),
                    power_class: value.power_class,
                    after_sequence: value.after_sequence,
                    limit: value.limit,
                    through_sequence: value.through_sequence,
                    trace_digest: value.trace_digest.map(|value| *value),
                    candidate_id: value.candidate_id.map(|value| *value),
                    candidate_hash: value.candidate_hash.map(|value| *value),
                },
            }),
            "heater_curve_config" => Ok(UsbFrame::HeaterCurveConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: HeaterCurveConfigCommand {
                    op: parse_heater_curve_config_op(value.op.as_deref())?,
                    package: value.heater_curve.map(|curve| *curve),
                },
            }),
            "heater_curve_save" => Ok(UsbFrame::HeaterCurveSave {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "response" => Ok(UsbFrame::Response {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                ok: value.ok.ok_or(UsbFrameError::MalformedJson)?,
                result: value.result.map(|result| *result),
                error: value.error,
            }),
            "status" => Ok(UsbFrame::Status {
                status: value.status.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "log" => Ok(UsbFrame::Log {
                level: value.level.ok_or(UsbFrameError::MalformedJson)?,
                message: *value.message.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "error" => Ok(UsbFrame::Error {
                request_id: value.request_id,
                error: value.error.ok_or(UsbFrameError::MalformedJson)?,
            }),
            _ => Err(UsbFrameError::MalformedJson),
        }
    }
}

impl From<&UsbFrame> for UsbFrameWire {
    fn from(value: &UsbFrame) -> Self {
        let mut wire = UsbFrameWire {
            frame_type: String::new(),
            protocol_version: None,
            framing: None,
            identity: None,
            capabilities: None,
            request_id: None,
            op: None,
            ssid: None,
            password: None,
            static_ipv4: None,
            telemetry_interval_ms: None,
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
            thermal_control_profile: None,
            channel: None,
            reference_temp_c: None,
            reference_vin_mv: None,
            target_adc_mv: None,
            observed_mv: None,
            expected_mv: None,
            sample_index: None,
            job_kind: None,
            after_sample: None,
            after_sequence: None,
            limit: None,
            through_sequence: None,
            trace_digest: None,
            run_id: None,
            power_class: None,
            candidate_id: None,
            candidate_hash: None,
            state: None,
            slot: None,
            fit: None,
            heater_curve: None,
            offset: None,
            length: None,
            bytes: None,
            ok: None,
            result: None,
            error: None,
            status: None,
            level: None,
            message: None,
        };

        match value {
            UsbFrame::Hello {
                protocol_version,
                framing,
                identity,
                capabilities,
            } => {
                wire.frame_type = string("hello");
                wire.protocol_version = Some(protocol_version.clone());
                wire.framing = Some(framing.clone());
                wire.identity = Some(Box::new(identity.clone()));
                wire.capabilities = Some(Box::new(capabilities.clone()));
            }
            UsbFrame::Request { request_id, op } => {
                wire.frame_type = string("request");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(op.as_str()));
            }
            UsbFrame::WifiConfig { request_id, config } => {
                wire.frame_type = string("wifi_config");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(config.op.as_str()));
                wire.ssid = config.ssid.clone();
                wire.password = config.password.clone();
                wire.static_ipv4 = config.static_ipv4;
                wire.telemetry_interval_ms = config.telemetry_interval_ms;
            }
            UsbFrame::RuntimeConfig { request_id, config } => {
                wire.frame_type = string("runtime_config");
                wire.request_id = Some(request_id.clone());
                wire.target_temp_c = config.target_temp_c;
                wire.selected_preset_slot = config.selected_preset_slot;
                wire.presets_c = config.presets_c;
                wire.active_cooling_enabled = config.active_cooling_enabled;
                wire.heater_enabled = config.heater_enabled;
                wire.manual_pps_enabled = config.manual_pps_enabled;
                wire.manual_pps_mv = config.manual_pps_mv;
                wire.manual_pps_ma = config.manual_pps_ma;
                wire.calibration = config.calibration.map(Box::new);
                wire.thermal_profile_mode = config.thermal_profile_mode;
                wire.thermal_control_profile = config.thermal_control_profile.map(Box::new);
            }
            UsbFrame::CalibrationConfig { request_id, config } => {
                wire.frame_type = string("calibration_config");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(config.op.as_str()));
                wire.channel = config.channel;
                wire.reference_temp_c = config.reference_temp_c;
                wire.reference_vin_mv = config.reference_vin_mv;
                wire.target_adc_mv = config.target_adc_mv;
                wire.observed_mv = config.observed_mv;
                wire.expected_mv = config.expected_mv;
                wire.sample_index = config.sample_index;
                wire.state = config.state.map(Box::new);
                wire.slot = config.slot;
                wire.fit = config.fit;
            }
            UsbFrame::CalibrationJob {
                request_id,
                command,
            } => {
                wire.frame_type = string("calibration_job");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(command.op.as_str()));
                wire.job_kind = command.kind;
            }
            UsbFrame::ThermalPlantRun {
                request_id,
                after_sample,
            } => {
                wire.frame_type = string("thermal_plant_run");
                wire.request_id = Some(request_id.clone());
                wire.after_sample = Some(*after_sample);
            }
            UsbFrame::ThermalTuningRun { command } => {
                wire.frame_type = string("thermal_tuning_run");
                wire.request_id = Some(command.request_id.clone());
                wire.op = Some(string(command.op.as_str()));
                wire.run_id = command.run_id.clone().map(Box::new);
                wire.power_class = command.power_class;
                wire.after_sequence = command.after_sequence;
                wire.limit = command.limit;
                wire.through_sequence = command.through_sequence;
                wire.trace_digest = command.trace_digest.clone().map(Box::new);
                wire.candidate_id = command.candidate_id.clone().map(Box::new);
                wire.candidate_hash = command.candidate_hash.clone().map(Box::new);
            }
            UsbFrame::HeaterCurveConfig { request_id, config } => {
                wire.frame_type = string("heater_curve_config");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(config.op.as_str()));
                wire.heater_curve = config.package.map(Box::new);
            }
            UsbFrame::HeaterCurveSave { request_id } => {
                wire.frame_type = string("heater_curve_save");
                wire.request_id = Some(request_id.clone());
            }
            UsbFrame::EepromMaintenance {
                request_id,
                command,
            } => {
                wire.frame_type = string("eeprom_maintenance");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(match command.op {
                    EepromMaintenanceOp::Read => "read",
                    EepromMaintenanceOp::Write => "write",
                    EepromMaintenanceOp::Erase => "erase",
                }));
                wire.offset = command.offset;
                wire.length = command.length;
                wire.bytes = command.bytes.clone();
            }
            UsbFrame::Response {
                request_id,
                ok,
                result,
                error,
            } => {
                wire.frame_type = string("response");
                wire.request_id = Some(request_id.clone());
                wire.ok = Some(*ok);
                wire.result = result.clone().map(Box::new);
                wire.error = error.clone();
            }
            UsbFrame::Status { status } => {
                wire.frame_type = string("status");
                wire.status = Some(status.clone());
            }
            UsbFrame::Log { level, message } => {
                wire.frame_type = string("log");
                wire.level = Some(level.clone());
                wire.message = Some(Box::new(message.clone()));
            }
            UsbFrame::Error { request_id, error } => {
                wire.frame_type = string("error");
                wire.request_id = request_id.clone();
                wire.error = Some(error.clone());
            }
        }

        wire
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbRequestOp {
    GetIdentity,
    GetInstallStatus,
    CompleteSetup,
    ResetPersistence,
    GetNetwork,
    GetStatus,
    GetLanPairingCode,
    OpenLanPairingWindow,
    CloseLanPairingWindow,
    GetCalibration,
    GetCalibrationJob,
    GetThermalTuningRun,
    GetHeaterCurve,
    SetLogLevel,
    ClearLanPairingToken,
}

impl UsbRequestOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::GetIdentity => "get_identity",
            Self::GetInstallStatus => "get_install_status",
            Self::CompleteSetup => "complete_setup",
            Self::ResetPersistence => "reset_persistence",
            Self::GetNetwork => "get_network",
            Self::GetStatus => "get_status",
            Self::GetLanPairingCode => "get_lan_pairing_code",
            Self::OpenLanPairingWindow => "open_lan_pairing_window",
            Self::CloseLanPairingWindow => "close_lan_pairing_window",
            Self::GetCalibration => "get_calibration",
            Self::GetCalibrationJob => "get_calibration_job",
            Self::GetThermalTuningRun => "get_thermal_tuning_run",
            Self::GetHeaterCurve => "get_heater_curve",
            Self::SetLogLevel => "set_log_level",
            Self::ClearLanPairingToken => "clear_lan_pairing_token",
        }
    }
}

impl HeaterCurveConfigOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::ClearPreview => "clear_preview",
        }
    }
}

impl CalibrationConfigOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Delete => "delete",
            Self::Clear => "clear",
            Self::Import => "import",
            Self::SetActiveSlot => "set_active_slot",
            Self::SetSlotFit => "set_slot_fit",
        }
    }
}

impl CalibrationJobOpWire {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Cancel => "cancel",
        }
    }
}

impl WifiConfigOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
            Self::Cancel => "cancel",
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbResponsePayload {
    Identity(Box<Identity>),
    InstallStatus(InstallStatus),
    Network(NetworkSummary),
    Status(Box<ControlPlaneStatus>),
    LanPairingCode(LanPairingCode),
    Wifi(WifiConfigReceipt),
    Calibration(CalibrationStateWire),
    CalibrationJob(CalibrationJobStateWire),
    ThermalPlantRun(ThermalPlantRunSnapshotWire),
    ThermalTuningRun(Box<ThermalTuningRunSnapshotWire>),
    HeaterCurve(HeaterCurveStateWire),
    EepromBytes(Vec<u8, EEPROM_MAINTENANCE_CHUNK_MAX>),
    Ack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub layout_id: String<48>,
    pub layout_version: u32,
    pub partition_table_sha256: String<72>,
    pub persistence_source: String<32>,
    pub record_state: String<24>,
    pub record_sequence: u32,
    pub commissioning_required: bool,
    pub setup_reason: Option<String<32>>,
    pub sensor_state: String<24>,
    pub heater_locked: bool,
}

impl InstallStatus {
    pub fn from_runtime(
        config: &crate::memory::MemoryConfig,
        persistence_source: &str,
        record_state: &str,
        record_sequence: u32,
        sensor_ready: bool,
        heater_fault_latched: bool,
    ) -> Self {
        Self {
            layout_id: string("flux-purr.esp32s3fh4r2.factory"),
            layout_version: 1,
            partition_table_sha256: string(
                "sha256:fec3c8b36e60ece8780cf75b4125a7171d3a3def71d5ca6ac706f4e431391f1e",
            ),
            persistence_source: string(persistence_source),
            record_state: string(record_state),
            record_sequence,
            commissioning_required: config.commissioning_required,
            setup_reason: if config.commissioning_required {
                Some(string(if record_sequence == 0 {
                    "blank_persistence"
                } else {
                    "explicit_reset"
                }))
            } else {
                None
            },
            sensor_state: string(if sensor_ready { "ready" } else { "unavailable" }),
            heater_locked: config.commissioning_required || !sensor_ready || heater_fault_latched,
        }
    }
}

/// A transient code is intentionally available through the already-authorized
/// USB control channel so host tooling can complete a physical WiFi Info-page
/// pairing flow. It never persists and becomes inactive when that page exits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanPairingCode {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String<4>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String<ERROR_CODE_MAX_LEN>,
    pub message: String<ERROR_MESSAGE_MAX_LEN>,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(code: &str, message: &str, retryable: bool) -> Self {
        Self {
            code: string(code),
            message: string(message),
            retryable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbFrameError {
    MalformedJson,
    OutputTooSmall,
}

pub fn hello_frame(identity: Identity) -> UsbFrame {
    UsbFrame::Hello {
        protocol_version: string(USB_PROTOCOL_VERSION),
        framing: string(USB_FRAMING),
        capabilities: identity.capabilities.clone(),
        identity,
    }
}

pub fn log_frame(level: &str, message: &str) -> UsbFrame {
    UsbFrame::Log {
        level: string(level),
        message: string(message),
    }
}

pub fn parse_usb_frame(line: &str) -> Result<UsbFrame, UsbFrameError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let frame_type = parse_usb_wire::<UsbFrameTypeWire>(trimmed)?.frame_type;
    match frame_type.as_str() {
        "hello" => {
            let frame = parse_usb_wire::<UsbHelloInboundWire>(trimmed)?;
            Ok(UsbFrame::Hello {
                protocol_version: frame.protocol_version.ok_or(UsbFrameError::MalformedJson)?,
                framing: frame.framing.ok_or(UsbFrameError::MalformedJson)?,
                identity: frame.identity.ok_or(UsbFrameError::MalformedJson)?,
                capabilities: frame.capabilities.ok_or(UsbFrameError::MalformedJson)?,
            })
        }
        "request" => {
            let frame = parse_usb_wire::<UsbRequestInboundWire>(trimmed)?;
            Ok(UsbFrame::Request {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                op: parse_usb_request_op(frame.op.as_deref())?,
            })
        }
        "wifi_config" => {
            let header = parse_usb_wifi_config_header(trimmed)?;
            let request_id = bounded_string(header.request_id)?;
            let op = parse_wifi_config_op(header.op)?;
            if matches!(op, WifiConfigOp::Clear | WifiConfigOp::Cancel) {
                return Ok(UsbFrame::WifiConfig {
                    request_id,
                    config: WifiConfigCommand {
                        op,
                        ssid: None,
                        password: None,
                        static_ipv4: None,
                        telemetry_interval_ms: None,
                    },
                });
            }
            let frame = parse_usb_wifi_config_set(trimmed)?;
            Ok(UsbFrame::WifiConfig {
                request_id: bounded_string(frame.request_id)?,
                config: WifiConfigCommand {
                    op,
                    ssid: bounded_optional_string(frame.ssid)?,
                    password: bounded_optional_string(frame.password)?,
                    static_ipv4: frame.static_ipv4,
                    telemetry_interval_ms: frame.telemetry_interval_ms,
                },
            })
        }
        "runtime_config" => {
            let frame = parse_usb_wire::<UsbRuntimeConfigInboundWire>(trimmed)?;
            Ok(UsbFrame::RuntimeConfig {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: RuntimeConfigCommand {
                    target_temp_c: frame.target_temp_c,
                    selected_preset_slot: frame.selected_preset_slot,
                    presets_c: frame.presets_c,
                    active_cooling_enabled: frame.active_cooling_enabled,
                    heater_enabled: frame.heater_enabled,
                    manual_pps_enabled: frame.manual_pps_enabled,
                    manual_pps_mv: frame.manual_pps_mv,
                    manual_pps_ma: frame.manual_pps_ma,
                    fault_attention_acknowledged: frame.fault_attention_acknowledged,
                    calibration: frame.calibration,
                    thermal_profile_mode: frame.thermal_profile_mode,
                    thermal_control_profile: frame.thermal_control_profile,
                },
            })
        }
        "calibration_config" => {
            let frame = parse_usb_wire::<UsbCalibrationConfigInboundWire>(trimmed)?;
            Ok(UsbFrame::CalibrationConfig {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: CalibrationConfigCommand {
                    op: parse_calibration_config_op(frame.op.as_deref())?,
                    channel: frame.channel,
                    reference_temp_c: frame.reference_temp_c,
                    reference_vin_mv: frame.reference_vin_mv,
                    target_adc_mv: frame.target_adc_mv,
                    observed_mv: frame.observed_mv,
                    expected_mv: frame.expected_mv,
                    sample_index: frame.sample_index,
                    state: frame.state,
                    slot: frame.slot,
                    fit: frame.fit,
                },
            })
        }
        "calibration_job" => {
            let frame = parse_usb_wire::<UsbCalibrationJobInboundWire>(trimmed)?;
            Ok(UsbFrame::CalibrationJob {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                command: CalibrationJobCommandWire {
                    op: parse_calibration_job_op(frame.op.as_deref())?,
                    kind: frame.kind,
                },
            })
        }
        "thermal_plant_run" => {
            let frame = parse_usb_wire::<UsbThermalPlantRunInboundWire>(trimmed)?;
            Ok(UsbFrame::ThermalPlantRun {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                after_sample: frame.after_sample.unwrap_or(0),
            })
        }
        "thermal_tuning_run" => {
            let frame = parse_usb_wire::<UsbThermalTuningRunInboundWire>(trimmed)?;
            Ok(UsbFrame::ThermalTuningRun {
                command: ThermalTuningRunCommandWire {
                    op: frame.op.ok_or(UsbFrameError::MalformedJson)?,
                    request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                    run_id: frame.run_id,
                    power_class: frame.power_class,
                    after_sequence: frame.after_sequence,
                    limit: frame.limit,
                    through_sequence: frame.through_sequence,
                    trace_digest: frame.trace_digest,
                    candidate_id: frame.candidate_id,
                    candidate_hash: frame.candidate_hash,
                },
            })
        }
        "heater_curve_config" => {
            let frame = parse_usb_wire::<UsbHeaterCurveConfigInboundWire>(trimmed)?;
            Ok(UsbFrame::HeaterCurveConfig {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: HeaterCurveConfigCommand {
                    op: parse_heater_curve_config_op(frame.op.as_deref())?,
                    package: frame.heater_curve,
                },
            })
        }
        "heater_curve_save" => {
            let frame = parse_usb_wire::<UsbRequestIdInboundWire>(trimmed)?;
            Ok(UsbFrame::HeaterCurveSave {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
            })
        }
        "eeprom_maintenance" => {
            let frame = parse_usb_wire::<UsbEepromMaintenanceInboundWire>(trimmed)?;
            Ok(UsbFrame::EepromMaintenance {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                command: EepromMaintenanceCommand {
                    op: frame.op.ok_or(UsbFrameError::MalformedJson)?,
                    offset: frame.offset,
                    length: frame.length,
                    bytes: frame.bytes,
                },
            })
        }
        "response" => {
            let frame = parse_usb_wire::<UsbResponseInboundWire>(trimmed)?;
            Ok(UsbFrame::Response {
                request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
                ok: frame.ok.ok_or(UsbFrameError::MalformedJson)?,
                result: frame.result,
                error: frame.error,
            })
        }
        "status" => {
            let frame = parse_usb_wire::<UsbStatusInboundWire>(trimmed)?;
            Ok(UsbFrame::Status {
                status: Box::new(frame.status.ok_or(UsbFrameError::MalformedJson)?),
            })
        }
        "log" => {
            let frame = parse_usb_wire::<UsbLogInboundWire>(trimmed)?;
            Ok(UsbFrame::Log {
                level: frame.level.ok_or(UsbFrameError::MalformedJson)?,
                message: frame.message.ok_or(UsbFrameError::MalformedJson)?,
            })
        }
        "error" => {
            let frame = parse_usb_wire::<UsbErrorInboundWire>(trimmed)?;
            Ok(UsbFrame::Error {
                request_id: frame.request_id,
                error: frame.error.ok_or(UsbFrameError::MalformedJson)?,
            })
        }
        _ => Err(UsbFrameError::MalformedJson),
    }
}

/// Parses a thermal-tuning command without constructing a full `UsbFrame`.
///
/// The front-panel task handles this command through a dedicated PSRAM-backed
/// response path. `Ok(None)` means the line belongs to the ordinary control
/// plane and must be dispatched by `parse_usb_frame` instead.
pub fn parse_thermal_tuning_run_command(
    line: &str,
) -> Result<Option<ThermalTuningRunCommandWire>, UsbFrameError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let frame_type = parse_usb_wire::<UsbFrameTypeWire>(trimmed)?.frame_type;
    if frame_type != "thermal_tuning_run" {
        return Ok(None);
    }
    let frame = parse_usb_wire::<UsbThermalTuningRunInboundWire>(trimmed)?;
    Ok(Some(ThermalTuningRunCommandWire {
        request_id: frame.request_id.ok_or(UsbFrameError::MalformedJson)?,
        op: frame.op.ok_or(UsbFrameError::MalformedJson)?,
        run_id: frame.run_id,
        power_class: frame.power_class,
        after_sequence: frame.after_sequence,
        limit: frame.limit,
        through_sequence: frame.through_sequence,
        trace_digest: frame.trace_digest,
        candidate_id: frame.candidate_id,
        candidate_hash: frame.candidate_hash,
    }))
}

#[inline(never)]
fn parse_usb_wire<T>(line: &str) -> Result<T, UsbFrameError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json_core::from_str(line)
        .map(|(frame, _)| frame)
        .map_err(|_| UsbFrameError::MalformedJson)
}

#[inline(never)]
fn parse_usb_wifi_config_header(line: &str) -> Result<UsbWifiConfigHeaderWire<'_>, UsbFrameError> {
    serde_json_core::from_str(line)
        .map(|(frame, _)| frame)
        .map_err(|_| UsbFrameError::MalformedJson)
}

#[inline(never)]
fn parse_usb_wifi_config_set(line: &str) -> Result<UsbWifiConfigSetInboundWire<'_>, UsbFrameError> {
    serde_json_core::from_str(line)
        .map(|(frame, _)| frame)
        .map_err(|_| UsbFrameError::MalformedJson)
}

pub fn write_usb_frame<'a>(frame: &UsbFrame, out: &'a mut [u8]) -> Result<&'a str, UsbFrameError> {
    if let UsbFrame::Response {
        request_id,
        ok: true,
        result: Some(UsbResponsePayload::Status(status)),
        error: None,
    } = frame
    {
        return write_usb_status_response_frame(request_id, status.as_ref(), out);
    }

    write_usb_wire(&UsbFrameWire::from(frame), out)
}

#[inline(never)]
fn write_usb_status_response_frame<'a>(
    request_id: &String<REQUEST_ID_MAX_LEN>,
    status: &ControlPlaneStatus,
    out: &'a mut [u8],
) -> Result<&'a str, UsbFrameError> {
    write_usb_wire(
        &UsbStatusResponseWire {
            frame_type: "response",
            request_id,
            ok: true,
            result: UsbStatusPayloadWire { status },
        },
        out,
    )
}

#[inline(never)]
fn write_usb_wire<'a, T: Serialize>(wire: &T, out: &'a mut [u8]) -> Result<&'a str, UsbFrameError> {
    let written =
        serde_json_core::to_slice(wire, out).map_err(|_| UsbFrameError::OutputTooSmall)?;
    if written >= out.len() {
        return Err(UsbFrameError::OutputTooSmall);
    }
    out[written] = b'\n';
    core::str::from_utf8(&out[..written + 1]).map_err(|_| UsbFrameError::OutputTooSmall)
}

/// Writes a thermal-tuning response without constructing the large generic
/// response payload enum on the front-panel stack.
pub fn write_thermal_tuning_response<'a>(
    request_id: &String<REQUEST_ID_MAX_LEN>,
    snapshot: Option<&ThermalTuningRunSnapshotWire>,
    error: Option<&ApiError>,
    out: &'a mut [u8],
) -> Result<&'a str, UsbFrameError> {
    let wire = UsbThermalTuningResponseFrameWire {
        frame_type: "response",
        request_id,
        ok: snapshot.is_some(),
        result: snapshot.map(|snapshot| ThermalTuningResponsePayloadWire { snapshot }),
        error,
    };
    let written =
        serde_json_core::to_slice(&wire, out).map_err(|_| UsbFrameError::OutputTooSmall)?;
    if written >= out.len() {
        return Err(UsbFrameError::OutputTooSmall);
    }
    out[written] = b'\n';
    core::str::from_utf8(&out[..written + 1]).map_err(|_| UsbFrameError::OutputTooSmall)
}

pub fn network_from_memory(config: &MemoryConfig) -> NetworkSummary {
    let ssid = if config.wifi_ssid.is_empty() {
        None
    } else {
        Some(config.wifi_ssid.clone())
    };

    NetworkSummary {
        state: if ssid.is_some() {
            NetworkState::Connecting
        } else {
            NetworkState::Disabled
        },
        ssid,
        wifi_password_length: config.wifi_password.len() as u8,
        ..NetworkSummary::default()
    }
}

fn string<const N: usize>(value: &str) -> String<N> {
    let mut out = String::new();
    let _ = out.push_str(value);
    out
}

fn bounded_string<const N: usize>(value: Option<&str>) -> Result<String<N>, UsbFrameError> {
    let value = value.ok_or(UsbFrameError::MalformedJson)?;
    let mut out = String::new();
    out.push_str(value)
        .map_err(|_| UsbFrameError::MalformedJson)?;
    Ok(out)
}

fn bounded_optional_string<const N: usize>(
    value: Option<&str>,
) -> Result<Option<String<N>>, UsbFrameError> {
    value.map(|value| bounded_string(Some(value))).transpose()
}

fn push_str<const N: usize, const C: usize>(values: &mut Vec<String<N>, C>, value: &str) {
    let _ = values.push(string(value));
}

fn parse_usb_request_op(value: Option<&str>) -> Result<UsbRequestOp, UsbFrameError> {
    match value {
        Some("get_identity") => Ok(UsbRequestOp::GetIdentity),
        Some("get_install_status") => Ok(UsbRequestOp::GetInstallStatus),
        Some("complete_setup") => Ok(UsbRequestOp::CompleteSetup),
        Some("reset_persistence") => Ok(UsbRequestOp::ResetPersistence),
        Some("get_network") => Ok(UsbRequestOp::GetNetwork),
        Some("get_status") => Ok(UsbRequestOp::GetStatus),
        Some("get_lan_pairing_code") => Ok(UsbRequestOp::GetLanPairingCode),
        Some("open_lan_pairing_window") => Ok(UsbRequestOp::OpenLanPairingWindow),
        Some("close_lan_pairing_window") => Ok(UsbRequestOp::CloseLanPairingWindow),
        Some("get_calibration") => Ok(UsbRequestOp::GetCalibration),
        Some("get_calibration_job") => Ok(UsbRequestOp::GetCalibrationJob),
        Some("get_thermal_tuning_run") => Ok(UsbRequestOp::GetThermalTuningRun),
        Some("get_heater_curve") => Ok(UsbRequestOp::GetHeaterCurve),
        Some("set_log_level") => Ok(UsbRequestOp::SetLogLevel),
        Some("clear_lan_pairing_token") => Ok(UsbRequestOp::ClearLanPairingToken),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_heater_curve_config_op(value: Option<&str>) -> Result<HeaterCurveConfigOp, UsbFrameError> {
    match value {
        Some("preview") => Ok(HeaterCurveConfigOp::Preview),
        Some("clear_preview") => Ok(HeaterCurveConfigOp::ClearPreview),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_calibration_job_op(value: Option<&str>) -> Result<CalibrationJobOpWire, UsbFrameError> {
    match value {
        Some("start") => Ok(CalibrationJobOpWire::Start),
        Some("cancel") => Ok(CalibrationJobOpWire::Cancel),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_thermal_tuning_run_op(
    value: Option<&str>,
) -> Result<ThermalTuningRunOpWire, UsbFrameError> {
    match value {
        Some("get") => Ok(ThermalTuningRunOpWire::Get),
        Some("start") => Ok(ThermalTuningRunOpWire::Start),
        Some("cancel") => Ok(ThermalTuningRunOpWire::Cancel),
        Some("ack_trace") => Ok(ThermalTuningRunOpWire::AckTrace),
        Some("seal_review") => Ok(ThermalTuningRunOpWire::SealReview),
        Some("preview") => Ok(ThermalTuningRunOpWire::Preview),
        Some("discard_preview") => Ok(ThermalTuningRunOpWire::DiscardPreview),
        Some("save") => Ok(ThermalTuningRunOpWire::Save),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_wifi_config_op(value: Option<&str>) -> Result<WifiConfigOp, UsbFrameError> {
    match value {
        Some("set") => Ok(WifiConfigOp::Set),
        Some("clear") => Ok(WifiConfigOp::Clear),
        Some("cancel") => Ok(WifiConfigOp::Cancel),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_calibration_config_op(value: Option<&str>) -> Result<CalibrationConfigOp, UsbFrameError> {
    match value {
        Some("capture") => Ok(CalibrationConfigOp::Capture),
        Some("delete") => Ok(CalibrationConfigOp::Delete),
        Some("clear") => Ok(CalibrationConfigOp::Clear),
        Some("import") => Ok(CalibrationConfigOp::Import),
        Some("set_active_slot") => Ok(CalibrationConfigOp::SetActiveSlot),
        Some("set_slot_fit") => Ok(CalibrationConfigOp::SetSlotFit),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FanCommand, FanPhase, snapshot_at};
    use core::mem::size_of;
    use std::format;

    #[test]
    fn network_failure_states_remain_observable_before_any_reconnect_attempt() {
        assert_ne!(NetworkState::Error, NetworkState::Connecting);
        assert_ne!(NetworkState::Timeout, NetworkState::Connecting);
    }

    #[test]
    fn tuning_snapshot_stays_indirect_in_control_plane_enums() {
        let snapshot_size = size_of::<ThermalTuningRunSnapshotWire>();
        let payload_size = size_of::<UsbResponsePayload>();
        let frame_size = size_of::<UsbFrame>();
        assert!(
            payload_size < snapshot_size,
            "payload={payload_size} frame={frame_size} snapshot={snapshot_size}"
        );
        assert!(
            frame_size <= 8 * 1024,
            "payload={payload_size} frame={frame_size} snapshot={snapshot_size}"
        );
    }

    #[test]
    fn thermal_tuning_response_serializes_an_idle_snapshot_within_the_usb_budget() {
        let request_id = string("thermal-idle");
        let snapshot = ThermalTuningRunSnapshotWire::default();
        let mut out = [0u8; USB_LINE_MAX_LEN];

        let line = write_thermal_tuning_response(&request_id, Some(&snapshot), None, &mut out)
            .expect("idle thermal tuning response fits");

        assert!(line.contains(r#""thermal_tuning_run""#));
        assert!(line.contains(r#""runId":"idle""#));
        assert!(
            line.len() < 2 * 1024,
            "idle thermal tuning response was {} bytes",
            line.len()
        );
    }

    #[test]
    fn thermal_tuning_response_serializes_an_eight_event_trace_page_within_the_usb_budget() {
        let request_id = string("thermal-page");
        let mut snapshot = ThermalTuningRunSnapshotWire::default();
        let candidate_hash =
            string("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        for sequence in 0..8 {
            snapshot
                .page
                .events
                .push(ThermalTuningTraceEventWire {
                    sequence,
                    elapsed_ms: sequence as u32 * 1_000,
                    kind: ThermalTuningTraceKindWire::Sample,
                    phase: Some(ThermalTuningPhaseWire::Scout),
                    previous_phase: None,
                    target_c: Some(240),
                    trial_index: Some(0),
                    candidate_id: None,
                    canonical_candidate_point_hex: None,
                    temperature_centi_c: Some(23_950),
                    vin_mv: Some(20_000),
                    pps_contract_mv: Some(20_000),
                    pps_contract_ma: Some(5_000),
                    heater_output_permille: Some(1_000),
                    measurement_valid: Some(true),
                    disposition: Some(ThermalTuningTargetDispositionWire::Accepted),
                    score_tracking: Some(1_000),
                    score_energy: Some(2_000),
                    score_overshoot: Some(3_000),
                    score_stability: Some(4_000),
                    score_settle_ms: Some(5_000),
                    score_hold_mean_absolute_error_centi: Some(600),
                    score_output_switches: Some(7),
                    interval_lower_boundary_c: Some(60),
                    interval_upper_boundary_c: Some(240),
                    interval_pruned: Some(false),
                    candidate_frozen: Some(true),
                    gates: Some(u16::MAX),
                    candidate_hash: Some(candidate_hash.clone()),
                    event_reason: None,
                    trial_start_sequence: None,
                    trial_end_sequence: None,
                    trial_start_elapsed_ms: None,
                    trial_end_elapsed_ms: None,
                })
                .expect("eight events fit the wire page");
        }
        let mut out = [0u8; USB_LINE_MAX_LEN];

        let line = write_thermal_tuning_response(&request_id, Some(&snapshot), None, &mut out)
            .expect("eight-event thermal tuning response fits");

        assert!(line.contains(r#""events":["#));
        assert!(line.len() < USB_LINE_MAX_LEN);
    }

    #[test]
    fn identity_lists_feature_capabilities() {
        let identity = Identity::firmware_default();
        assert!(
            identity
                .capabilities
                .iter()
                .any(|value| value == "identity")
        );
        assert!(identity.capabilities.iter().any(|value| value == "status"));
        let tuning = identity
            .thermal_tuning
            .as_ref()
            .expect("thermal tuning capability detail");
        assert_eq!(tuning.id.as_str(), "thermal_tuning_run_v1");
        assert_eq!(tuning.supported_power_classes.len(), 2);
        assert_eq!(
            tuning.target_schedule_c,
            [60, 240, 140, 100, 80, 120, 180, 160, 220]
        );
        assert_eq!(tuning.trace.buffer_capacity, 96);
        assert!(tuning.candidate_promotion);
        #[cfg(feature = "web_serial")]
        {
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "usb_jsonl")
            );
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "wifi_config")
            );
        }
        #[cfg(feature = "net_http")]
        {
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "lan_http")
            );
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "lan_pairing")
            );
        }
    }

    #[test]
    fn identity_response_omits_unrelated_wire_fields_for_usb_transport() {
        let frame = UsbFrame::Response {
            request_id: string("compact-identity"),
            ok: true,
            result: Some(UsbResponsePayload::Identity(Box::new(
                Identity::firmware_default(),
            ))),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).expect("identity response fits");

        assert!(json.contains(r#""requestId":"compact-identity""#));
        assert!(json.contains(r#""identity""#));
        assert!(!json.contains(r#""status":null"#));
        assert!(!json.contains(r#""ssid":null"#));
        assert!(!json.contains(r#""heaterCurve":null"#));
        assert!(json.len() < 1_400);
        assert_eq!(
            parse_usb_frame(json).expect("compact response parses"),
            frame
        );
    }

    #[test]
    fn outbound_status_wire_stays_within_the_frontpanel_stack_budget() {
        assert!(
            core::mem::size_of::<UsbResponsePayload>() <= 768,
            "response payload is {} bytes",
            core::mem::size_of::<UsbResponsePayload>()
        );
        let wire_size = core::mem::size_of::<UsbFrameWire>();
        assert!(wire_size <= 1_024, "outbound wire is {wire_size} bytes");
    }

    #[test]
    fn hardware_identity_uses_the_mac_for_every_transport() {
        let identity = Identity::firmware_from_mac([0xa0, 0xf2, 0x62, 0xf2, 0x0d, 0x6c]);

        assert_eq!(identity.device_id.as_str(), "a0f262f20d6c");
        assert_eq!(identity.hostname.as_str(), "flux-purr-a0f262f20d6c");
    }

    #[test]
    fn status_adapter_uses_memory_and_runtime_state() {
        let mut memory = MemoryConfig {
            target_temp_c: 210,
            active_cooling_enabled: false,
            ..MemoryConfig::default()
        };
        memory.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        let status = ControlPlaneStatus::from_device_status(
            snapshot_at(10, 0),
            &memory,
            42,
            network_from_memory(&memory),
        );

        assert_eq!(status.mode, DeviceModeWire::Sampling);
        assert_eq!(status.target_temp_c, 210);
        assert!(!status.active_cooling_enabled);
        assert_eq!(status.network.state, NetworkState::Connecting);
        assert_eq!(status.network.ssid.as_deref(), Some("FluxPurr-Lab"));
        assert_eq!(status.frontpanel_key, Some(FrontPanelKeyWire::Center));
        assert_eq!(status.heater_lock_reason, None);
        assert_eq!(status.pd_controller.as_str(), "unknown");
        assert_eq!(status.pd_contract_kind.as_str(), "none");
        assert_eq!(status.pd_contract_current_ma, 0);
        assert_eq!(status.pd_contract_power_mw, 0);
        assert!(!status.pd_performance_guaranteed);
        assert_eq!(
            status.pd_degraded_reason.as_deref(),
            Some("pd_contract_unavailable")
        );
    }

    #[test]
    fn status_frame_serializes_pd_fallback_for_web_contract() {
        let mut status = ControlPlaneStatus::from_device_status(
            snapshot_at(17, 0),
            &MemoryConfig::default(),
            17,
            NetworkSummary::default(),
        );
        status.adc_diagnostics = Box::new(AdcDiagnosticsWire {
            calibration_source: AdcCalibrationSourceWire::Efuse,
            efuse_version: 1,
            attenuation_db: 6,
            init_code: Some(1850),
            reference_code: Some(1600),
            reference_mv: Some(850),
            rtd_raw_code_mean: 2100,
            rtd_raw_code_min: 2098,
            rtd_raw_code_max: 2102,
            rtd_raw_code_spread: 4,
            vin_raw_code_mean: 1800,
        });
        let frame = UsbFrame::Response {
            request_id: string("req-pd"),
            ok: true,
            result: Some(UsbResponsePayload::Status(Box::new(status))),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""pdState":"fallback_5v""#));
        assert!(json.contains(r#""manualPpsEnabled":false"#));
        assert!(json.contains(r#""heaterLockReason":null"#));
        assert!(json.len() < USB_LINE_MAX_LEN);
        assert!(json.contains(r#""ppsCapabilityMinMv":null"#));
        assert!(json.contains(r#""adcDiagnostics":{"calibrationSource":"efuse"#));
        assert!(json.contains(r#""rtdRawCodeSpread":4"#));
        assert!(json.len() <= USB_LINE_MAX_LEN);
        assert!(!json.contains("fallback5v"));
        assert_eq!(
            parse_usb_frame(json).expect("status response parses"),
            frame
        );
    }

    #[test]
    fn network_summary_exposes_password_length_without_password_content() {
        let mut config = MemoryConfig::default();
        config.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        config.wifi_password.push_str("secret-pass").unwrap();
        let summary = network_from_memory(&config);
        assert_eq!(summary.wifi_password_length, 11);

        let frame = UsbFrame::Response {
            request_id: string("req-network"),
            ok: true,
            result: Some(UsbResponsePayload::Network(summary)),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();
        assert!(json.contains(r#""wifiPasswordLength":11"#));
        assert!(!json.contains("secret-pass"));
    }

    #[test]
    fn parses_calibration_config_frame_type() {
        let frame = parse_usb_frame(
            r#"{"type":"calibration_config","requestId":"cal-001","op":"clear","channel":"vin_adc"}"#,
        )
        .expect("calibration_config frame parses");

        match frame {
            UsbFrame::CalibrationConfig { request_id, config } => {
                assert_eq!(request_id.as_str(), "cal-001");
                assert_eq!(config.op, CalibrationConfigOp::Clear);
                assert_eq!(config.channel, Some(CalibrationChannelWire::VinAdc));
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn full_calibration_state_response_fits_usb_line() {
        let mut memory = MemoryConfig::default();
        for index in 0..ADC_CALIBRATION_MAX_SAMPLES {
            let sample = AdcCalibrationSample {
                observed_mv: 400 + index as u16 * 170,
                expected_mv: 420 + index as u16 * 170,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: Some(420 + index as u16 * 170),
            };
            memory
                .adc_calibration
                .vin
                .insert(sample)
                .expect("sample slot exists");
        }
        memory.adc_calibration.vin.slots.a = AdcCalibrationSlotFit {
            gain: 0.97,
            offset_mv: 12.5,
        };
        memory.adc_calibration.vin.slots.b = AdcCalibrationSlotFit {
            gain: 1.03,
            offset_mv: -8.0,
        };
        memory.adc_calibration.vin.active_slot = AdcCalibrationSlotId::B;
        let frame = UsbFrame::Response {
            request_id: string("cal-full"),
            ok: true,
            result: Some(UsbResponsePayload::Calibration(
                calibration_state_from_memory(&memory),
            )),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).expect("full calibration state fits");

        assert!(json.contains(r#""requestId":"cal-full""#));
        assert!(json.contains(r#""fittedFit""#));
        assert!(json.contains(r#""activeSlot":"b""#));
        assert!(json.contains(
            r#""slots":{"a":{"gain":0.97,"offsetMv":12.5},"b":{"gain":1.03,"offsetMv":-8.0}}"#
        ));
    }

    #[test]
    fn full_thermal_control_status_response_fits_usb_line() {
        let memory = MemoryConfig::default();
        let mut status = ControlPlaneStatus::from_device_status(
            snapshot_at(180, 0),
            &memory,
            12,
            NetworkSummary::default(),
        );
        status.thermal_control_profile_preview = true;
        status.thermal_control = ThermalControlRuntimeWire {
            profile_active: true,
            profile_covers_target: true,
            profile_source: string("preview"),
            target_temp_c: 220,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 1_000,
            approach_power_permille: 900,
            approach_floor_power_permille: 700,
            approach_damping_exponent_permille: 1_100,
            approach_tail_window_centi_c: 150,
            hold_power_permille: 650,
            hold_reheat_power_permille: 800,
            hold_entry_centi_c: 15,
            hold_exit_centi_c: 70,
            hold_on_centi_c: 25,
            hold_off_centi_c: 90,
            overshoot_cutoff_centi_c: 150,
            hold_kp_permille_per_c: 20,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 8,
            approach_lead_ticks: 4,
            hold_lead_ticks: 0,
            temp_filter_alpha_permille: 260,
            warmup_reenter_centi_c: 1_000,
            approach_max_ticks: 250,
            approach_min_power_ratio_permille: 500,
            auto_adjustable_working_floor_mv: 6_100,
            heater_current_reserve_ma: 200,
        };
        let frame = UsbFrame::Response {
            request_id: string("thermal-full"),
            ok: true,
            result: Some(UsbResponsePayload::Status(Box::new(status))),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).expect("full thermal status fits");

        assert!(json.contains(r#""thermalControl":{"profileActive":true"#));
        assert!(json.contains(r#""autoAdjustableWorkingFloorMv":6100"#));
    }

    #[test]
    fn log_frame_serializes_lifecycle_message() {
        let frame = log_frame("info", "frontpanel runtime ready");
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""type":"log""#));
        assert!(json.contains(r#""level":"info""#));
        assert!(json.contains(r#""message":"frontpanel runtime ready""#));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn status_adapter_maps_fan_display_state() {
        let memory = MemoryConfig::default();
        let mut status = snapshot_at(10, 0);
        status.fan_enabled = false;
        status.fan_pwm_permille = FanCommand::from_phase(FanPhase::Stop).pwm_permille;
        let adapted =
            ControlPlaneStatus::from_device_status(status, &memory, 0, NetworkSummary::default());
        assert_eq!(adapted.fan_display_state, FanDisplayState::Auto);

        let mut running_fan = snapshot_at(120, 0);
        running_fan.fan_enabled = true;
        running_fan.fan_pwm_permille = crate::FAN_LOW_PWM_PERMILLE;
        let adapted = ControlPlaneStatus::from_device_status(
            running_fan,
            &memory,
            0,
            NetworkSummary::default(),
        );
        assert_eq!(adapted.fan_display_state, FanDisplayState::Run);

        let mut safety_fan = snapshot_at(120, 0);
        safety_fan.fan_enabled = true;
        safety_fan.fan_pwm_permille = crate::FAN_MID_PWM_PERMILLE;
        let cooling_disabled = MemoryConfig {
            active_cooling_enabled: false,
            ..MemoryConfig::default()
        };
        let adapted = ControlPlaneStatus::from_device_status(
            safety_fan,
            &cooling_disabled,
            0,
            NetworkSummary::default(),
        );
        assert_eq!(adapted.fan_display_state, FanDisplayState::Off);
    }

    #[test]
    fn wifi_command_applies_and_redacts_password() {
        let command = WifiConfigCommand {
            op: WifiConfigOp::Set,
            ssid: Some(string("FluxPurr-Lab")),
            password: Some(string("secret-pass")),
            static_ipv4: Some(Some(WifiStaticIpv4Wire {
                address: [192, 168, 31, 42],
                prefix_len: 24,
                gateway: [192, 168, 31, 1],
                dns: [1, 1, 1, 1],
            })),
            telemetry_interval_ms: Some(750),
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);
        assert_eq!(config.wifi_ssid.as_str(), "FluxPurr-Lab");
        assert_eq!(config.wifi_password.as_str(), "secret-pass");
        assert_eq!(config.wifi_static_ipv4.unwrap().address, [192, 168, 31, 42]);
        assert_eq!(
            command.redacted_summary().password.as_deref(),
            Some("<redacted>")
        );
    }

    #[test]
    fn wifi_command_preserves_static_ipv4_when_field_is_omitted() {
        let mut config = MemoryConfig {
            wifi_static_ipv4: Some(WifiStaticIpv4Config {
                address: [192, 168, 31, 42],
                prefix_len: 24,
                gateway: [192, 168, 31, 1],
                dns: [1, 1, 1, 1],
            }),
            ..MemoryConfig::default()
        };
        WifiConfigCommand {
            op: WifiConfigOp::Set,
            ssid: Some(string("FluxPurr-Lab")),
            password: None,
            static_ipv4: None,
            telemetry_interval_ms: None,
        }
        .apply_to(&mut config);

        assert_eq!(config.wifi_static_ipv4.unwrap().address, [192, 168, 31, 42]);
    }

    #[test]
    fn wifi_command_preserves_password_when_field_is_omitted() {
        let mut config = MemoryConfig::default();
        config.wifi_password.push_str("secret-pass").unwrap();

        WifiConfigCommand {
            op: WifiConfigOp::Set,
            ssid: Some(string("FluxPurr-Lab")),
            password: None,
            static_ipv4: None,
            telemetry_interval_ms: None,
        }
        .apply_to(&mut config);

        assert_eq!(config.wifi_password.as_str(), "secret-pass");
    }

    #[test]
    fn wifi_command_clears_password_when_empty_password_is_explicit() {
        let mut config = MemoryConfig::default();
        config.wifi_password.push_str("secret-pass").unwrap();

        WifiConfigCommand {
            op: WifiConfigOp::Set,
            ssid: Some(string("FluxPurr-Lab")),
            password: Some(string("")),
            static_ipv4: None,
            telemetry_interval_ms: None,
        }
        .apply_to(&mut config);

        assert!(config.wifi_password.is_empty());
    }

    #[test]
    fn wifi_command_clears_static_ipv4_when_field_is_explicit_null() {
        let mut config = MemoryConfig {
            wifi_static_ipv4: Some(WifiStaticIpv4Config {
                address: [192, 168, 31, 42],
                prefix_len: 24,
                gateway: [192, 168, 31, 1],
                dns: [1, 1, 1, 1],
            }),
            ..MemoryConfig::default()
        };
        let frame = parse_usb_frame(
            r#"{"type":"wifi_config","requestId":"req-dhcp","op":"set","staticIpv4":null}"#,
        )
        .unwrap();
        let UsbFrame::WifiConfig {
            config: command, ..
        } = frame
        else {
            panic!("expected WiFi config frame");
        };

        assert_eq!(command.static_ipv4, Some(None));
        command.apply_to(&mut config);
        assert!(config.wifi_static_ipv4.is_none());

        let direct =
            serde_json_core::from_str::<WifiConfigCommand>(r#"{"op":"set","staticIpv4":null}"#)
                .unwrap()
                .0;
        assert_eq!(direct.static_ipv4, Some(None));
    }

    #[test]
    fn usb_wifi_static_ipv4_patch_preserves_absent_and_serializes_dhcp_clear() {
        let clear = UsbFrame::WifiConfig {
            request_id: string("wifi-dhcp"),
            config: WifiConfigCommand {
                op: WifiConfigOp::Set,
                ssid: None,
                password: None,
                static_ipv4: Some(None),
                telemetry_interval_ms: None,
            },
        };
        let preserve = UsbFrame::WifiConfig {
            request_id: string("wifi-preserve"),
            config: WifiConfigCommand {
                op: WifiConfigOp::Set,
                ssid: None,
                password: None,
                static_ipv4: None,
                telemetry_interval_ms: None,
            },
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];

        let clear_json = write_usb_frame(&clear, &mut out).unwrap();
        assert!(clear_json.contains(r#""staticIpv4":null"#));
        assert_eq!(parse_usb_frame(clear_json).unwrap(), clear);

        let preserve_json = write_usb_frame(&preserve, &mut out).unwrap();
        assert!(!preserve_json.contains(r#""staticIpv4""#));
        assert_eq!(parse_usb_frame(preserve_json).unwrap(), preserve);
    }

    #[test]
    fn runtime_command_updates_memory_policy() {
        let command = RuntimeConfigCommand {
            target_temp_c: Some(250),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: Some(false),
            heater_enabled: Some(true),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);

        assert_eq!(config.target_temp_c, 250);
        assert!(!config.active_cooling_enabled);
        assert_eq!(command.heater_enabled, Some(true));
    }

    #[test]
    fn runtime_command_updates_memory_presets() {
        let command = RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: Some(3),
            presets_c: Some([
                Some(50),
                Some(100),
                None,
                Some(155),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ]),
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
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);

        assert_eq!(config.selected_preset_slot, 3);
        assert_eq!(config.presets_c[2], None);
        assert_eq!(config.presets_c[3], Some(155));
        assert_eq!(config.target_temp_c, 155);
    }

    #[test]
    fn runtime_command_saves_and_clears_thermal_profile() {
        let mut points = [None; FRONTPANEL_PRESET_COUNT];
        points[0] = Some(ThermalControlProfilePointWire {
            target_temp_c: 210,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 260,
            warmup_reenter_centi_c: 0,
            approach_power_permille: 260,
            approach_floor_power_permille: 180,
            approach_damping_exponent_permille:
                crate::memory::THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 180,
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
        let mut config = MemoryConfig::default();
        RuntimeConfigCommand {
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
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::Save,
                bank: None,
                profile: Some(ThermalControlProfileWire {
                    settings: None,
                    points,
                }),
            }),
        }
        .apply_to(&mut config);

        assert_eq!(
            config.active_thermal_control_profile.points[0],
            Some(ThermalControlProfilePointConfig {
                target_temp_c: 210,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 260,
                warmup_reenter_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
                approach_power_permille: 260,
                approach_floor_power_permille: 180,
                approach_damping_exponent_permille:
                    crate::memory::THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
                hold_reheat_power_permille: 0,
                hold_entry_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
                hold_exit_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
                hold_on_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
                hold_off_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
                overshoot_cutoff_centi_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
                hold_kp_permille_per_c:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
                hold_ki_permille_per_c_tick:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
                hold_blend_ticks:
                    crate::memory::THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            })
        );

        RuntimeConfigCommand {
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
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::ClearSaved,
                bank: None,
                profile: None,
            }),
        }
        .apply_to(&mut config);

        assert_eq!(
            config.active_thermal_control_profile,
            ThermalControlProfileConfig::default()
        );
    }

    #[test]
    fn runtime_command_save_keeps_all_thermal_profile_points() {
        let mut points = [None; FRONTPANEL_PRESET_COUNT];
        for (slot, target_temp_c) in [60, 80, 100, 120, 140, 160, 180, 200, 220, 240]
            .into_iter()
            .enumerate()
        {
            points[slot] = Some(ThermalControlProfilePointWire {
                target_temp_c,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 260,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 260,
                approach_floor_power_permille: 180,
                approach_damping_exponent_permille:
                    crate::memory::THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
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
        }

        let mut config = MemoryConfig::default();
        RuntimeConfigCommand {
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
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::Save,
                bank: None,
                profile: Some(ThermalControlProfileWire {
                    settings: None,
                    points,
                }),
            }),
        }
        .apply_to(&mut config);

        assert_eq!(
            config
                .active_thermal_control_profile
                .points
                .iter()
                .flatten()
                .count(),
            crate::memory::THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS
        );
        assert_eq!(
            config.active_thermal_control_profile.points[9]
                .expect("tenth point")
                .target_temp_c,
            240
        );
    }

    #[test]
    fn parse_usb_request_with_request_id() {
        let frame =
            parse_usb_frame(r#"{"type":"request","requestId":"req-001","op":"get_status"}"#)
                .unwrap();

        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("req-001"),
                op: UsbRequestOp::GetStatus,
            }
        );
    }

    #[test]
    fn parses_commissioning_persistence_requests() {
        for (op, expected) in [
            ("complete_setup", UsbRequestOp::CompleteSetup),
            ("reset_persistence", UsbRequestOp::ResetPersistence),
        ] {
            let line =
                std::format!(r#"{{"type":"request","requestId":"commissioning","op":"{op}"}}"#);
            assert_eq!(
                parse_usb_frame(&line).unwrap(),
                UsbFrame::Request {
                    request_id: string("commissioning"),
                    op: expected,
                }
            );
        }
    }

    #[test]
    fn parses_usb_only_lan_pairing_reset_request() {
        let frame = parse_usb_frame(
            r#"{"type":"request","requestId":"reset-lan","op":"clear_lan_pairing_token"}"#,
        )
        .unwrap();
        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("reset-lan"),
                op: UsbRequestOp::ClearLanPairingToken,
            }
        );
    }

    #[test]
    fn parses_usb_only_lan_pairing_window_requests() {
        let open = parse_usb_frame(
            r#"{"type":"request","requestId":"pairing-open","op":"open_lan_pairing_window"}"#,
        )
        .unwrap();
        let close = parse_usb_frame(
            r#"{"type":"request","requestId":"pairing-close","op":"close_lan_pairing_window"}"#,
        )
        .unwrap();

        assert_eq!(
            open,
            UsbFrame::Request {
                request_id: string("pairing-open"),
                op: UsbRequestOp::OpenLanPairingWindow,
            }
        );
        assert_eq!(
            close,
            UsbFrame::Request {
                request_id: string("pairing-close"),
                op: UsbRequestOp::CloseLanPairingWindow,
            }
        );
    }

    #[test]
    fn parses_and_writes_usb_lan_pairing_code_response() {
        let frame = parse_usb_frame(
            r#"{"type":"request","requestId":"pairing-code","op":"get_lan_pairing_code"}"#,
        )
        .unwrap();
        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("pairing-code"),
                op: UsbRequestOp::GetLanPairingCode,
            }
        );

        let response = UsbFrame::Response {
            request_id: string("pairing-code"),
            ok: true,
            result: Some(UsbResponsePayload::LanPairingCode(LanPairingCode {
                active: true,
                code: Some(string("4827")),
            })),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.contains(r#""lan_pairing_code":{"active":true,"code":"4827"}"#));
    }

    #[test]
    fn parse_wifi_frame_and_write_redacted_response() {
        let frame = parse_usb_frame(
            r#"{"type":"wifi_config","requestId":"req-002","op":"set","ssid":"FluxPurr-Lab","password":"secret-pass","autoReconnect":false,"telemetryIntervalMs":500}"#,
        )
        .unwrap();
        let UsbFrame::WifiConfig { request_id, config } = frame else {
            panic!("expected wifi frame");
        };
        assert_eq!(request_id.as_str(), "req-002");
        assert_eq!(config.password.as_deref(), Some("secret-pass"));
        let mut memory = MemoryConfig {
            wifi_auto_reconnect: false,
            ..MemoryConfig::default()
        };
        config.apply_to(&mut memory);
        assert!(memory.wifi_auto_reconnect);

        let response = UsbFrame::Response {
            request_id,
            ok: true,
            result: Some(UsbResponsePayload::Wifi(WifiConfigReceipt {
                wifi: config.redacted_summary(),
                network: NetworkSummary::default(),
            })),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.contains(r#""password":"<redacted>""#));
        assert!(!json.contains("secret-pass"));
        assert!(!json.contains("autoReconnect"));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn parse_wifi_clear_frame_without_materializing_set_fields() {
        let frame =
            parse_usb_frame(r#"{"type":"wifi_config","requestId":"wifi-clear","op":"clear"}"#)
                .unwrap();
        assert_eq!(
            frame,
            UsbFrame::WifiConfig {
                request_id: string("wifi-clear"),
                config: WifiConfigCommand {
                    op: WifiConfigOp::Clear,
                    ssid: None,
                    password: None,
                    static_ipv4: None,
                    telemetry_interval_ms: None,
                },
            }
        );
    }

    #[test]
    fn parse_wifi_cancel_frame_without_mutating_persisted_credentials() {
        let frame =
            parse_usb_frame(r#"{"type":"wifi_config","requestId":"wifi-cancel","op":"cancel"}"#)
                .unwrap();
        let UsbFrame::WifiConfig { config, .. } = frame else {
            panic!("expected WiFi config frame");
        };
        assert_eq!(config.op, WifiConfigOp::Cancel);
        assert_eq!(config.ssid, None);
        assert_eq!(config.password, None);

        let mut memory = MemoryConfig::default();
        memory.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        memory.wifi_password.push_str("secret-pass").unwrap();
        config.apply_to(&mut memory);

        assert_eq!(memory.wifi_ssid.as_str(), "FluxPurr-Lab");
        assert_eq!(memory.wifi_password.as_str(), "secret-pass");
    }

    #[test]
    fn parse_runtime_config_frame() {
        let frame = parse_usb_frame(
            r#"{"type":"runtime_config","requestId":"req-003","targetTempC":230,"activeCoolingEnabled":false,"heaterEnabled":true,"manualPpsEnabled":true,"manualPpsMv":10400,"manualPpsMa":2500}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::RuntimeConfig {
                request_id: string("req-003"),
                config: RuntimeConfigCommand {
                    target_temp_c: Some(230),
                    selected_preset_slot: None,
                    presets_c: None,
                    active_cooling_enabled: Some(false),
                    heater_enabled: Some(true),
                    manual_pps_enabled: Some(true),
                    manual_pps_mv: Some(10_400),
                    manual_pps_ma: Some(2_500),
                    fault_attention_acknowledged: None,
                    calibration: None,
                    thermal_profile_mode: None,
                    thermal_control_profile: None,
                },
            }
        );
    }

    #[test]
    fn runtime_config_frame_round_trips_thermal_profile_mode() {
        let frame = UsbFrame::RuntimeConfig {
            request_id: string("req-mode"),
            config: RuntimeConfigCommand {
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
                thermal_profile_mode: Some(ThermalProfileModeWire::W100),
                thermal_control_profile: None,
            },
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""thermalProfileMode":"100w""#));
        assert_eq!(parse_usb_frame(json).unwrap(), frame);
    }

    #[test]
    fn parse_runtime_config_frame_with_presets() {
        let frame = parse_usb_frame(
            r#"{"type":"runtime_config","requestId":"req-004","selectedPresetSlot":3,"presetsC":[50,100,null,155,180,200,210,220,250,300]}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::RuntimeConfig {
                request_id: string("req-004"),
                config: RuntimeConfigCommand {
                    target_temp_c: None,
                    selected_preset_slot: Some(3),
                    presets_c: Some([
                        Some(50),
                        Some(100),
                        None,
                        Some(155),
                        Some(180),
                        Some(200),
                        Some(210),
                        Some(220),
                        Some(250),
                        Some(300),
                    ]),
                    active_cooling_enabled: None,
                    heater_enabled: None,
                    manual_pps_enabled: None,
                    manual_pps_mv: None,
                    manual_pps_ma: None,
                    fault_attention_acknowledged: None,
                    calibration: None,
                    thermal_profile_mode: None,
                    thermal_control_profile: None,
                },
            }
        );
    }

    #[test]
    fn parse_runtime_config_frame_with_thermal_profile_preview() {
        let frame = parse_usb_frame(
            r#"{"type":"runtime_config","requestId":"req-thermal","thermalControlProfile":{"op":"preview","profile":{"points":[{"targetTempC":100,"brakeDistanceCentiC":700,"approachPowerPermille":420,"approachFloorPowerPermille":220,"holdPowerPermille":220},null,null,null,null,null,null,null,null,null]}}}"#,
        )
        .unwrap();

        let UsbFrame::RuntimeConfig { request_id, config } = frame else {
            panic!("expected runtime config frame");
        };
        assert_eq!(request_id.as_str(), "req-thermal");
        let command = config.thermal_control_profile.unwrap();
        assert_eq!(command.op, ThermalControlProfileOp::Preview);
        let profile = command.profile.unwrap();
        assert_eq!(
            profile.points[0],
            Some(ThermalControlProfilePointWire {
                target_temp_c: 100,
                brake_distance_centi_c: 700,
                warmup_power_permille: 0,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 220,
                approach_damping_exponent_permille:
                    crate::memory::THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
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
            })
        );
        assert!(profile.points[1].is_none());
    }

    #[test]
    fn parses_full_dev_host_thermal_profile_within_usb_line_limit() {
        let line = include_str!("../tests/fixtures/full-thermal-profile-preview.jsonl");
        assert!(line.len() > 3_000);
        assert!(line.len() <= USB_LINE_MAX_LEN);
        let UsbFrame::RuntimeConfig { request_id, config } = parse_usb_frame(line).unwrap() else {
            panic!("expected runtime config frame");
        };
        assert_eq!(request_id.as_str(), "full-profile");
        let profile = config.thermal_control_profile.unwrap().profile.unwrap();
        assert_eq!(profile.points.iter().flatten().count(), 6);
        assert_eq!(profile.points[0].unwrap().target_temp_c, 60);
        assert_eq!(profile.points[5].unwrap().target_temp_c, 250);
        assert_eq!(
            profile.settings.unwrap().auto_adjustable_working_floor_mv,
            6_100
        );
        assert_eq!(profile.settings.unwrap().heater_current_reserve_ma, 200);
    }

    #[test]
    fn parses_nine_point_thermal_profile_save_within_usb_line_limit() {
        let points = [60, 80, 100, 120, 140, 160, 180, 220, 240]
            .into_iter()
            .map(|target_temp_c| {
                format!(
                    r#"{{"targetTempC":{target_temp_c},"brakeDistanceCentiC":1000,"warmupPowerPermille":1000,"warmupReenterCentiC":1000,"approachPowerPermille":1000,"approachFloorPowerPermille":1000,"approachDampingExponentPermille":1000,"approachTailWindowCentiC":1000,"holdPowerPermille":1000,"holdReheatPowerPermille":1000,"holdEntryCentiC":1000,"holdExitCentiC":1000,"holdOnCentiC":1000,"holdOffCentiC":1000,"overshootCutoffCentiC":1000,"holdKpPermillePerC":1000,"holdKiPermillePerCTick":1000,"holdBlendTicks":1000,"approachLeadTicks":1000,"holdLeadTicks":1000}}"#
                )
            })
            .collect::<std::vec::Vec<_>>()
            .join(",");
        let line = format!(
            r#"{{"type":"runtime_config","requestId":"nine-point-save","thermalProfileMode":"100w","thermalControlProfile":{{"op":"save","bank":"pps5a","profile":{{"settings":{{"tempFilterAlphaPermille":1000,"warmupReenterCentiC":1000,"holdEntryCentiC":1000,"holdExitCentiC":1000,"holdOnCentiC":1000,"holdOffCentiC":1000,"overshootCutoffCentiC":1000,"approachMaxTicks":1000,"approachMinPowerRatioPermille":1000,"holdKpPermillePerC":1000,"holdKiPermillePerCTick":1000,"holdBlendTicks":1000,"holdReheatPowerPermille":1000,"approachLeadTicks":1000,"holdLeadTicks":1000,"autoAdjustableWorkingFloorMv":6100,"heaterCurrentReserveMa":1000}},"points":[{points},null]}}}}}}"#
        );

        assert!(line.len() > 4_096);
        assert!(line.len() <= USB_LINE_MAX_LEN);
        assert!(line.len() <= crate::net_http::LAN_HTTP_BODY_MAX_LEN);
        let UsbFrame::RuntimeConfig { request_id, config } = parse_usb_frame(&line).unwrap() else {
            panic!("expected runtime config frame");
        };
        assert_eq!(request_id.as_str(), "nine-point-save");
        let profile = config.thermal_control_profile.unwrap().profile.unwrap();
        assert_eq!(profile.points.iter().flatten().count(), 9);
        assert_eq!(profile.points[8].unwrap().target_temp_c, 240);
    }

    #[test]
    fn parse_calibration_job_frame_accepts_kind_alias() {
        let frame = parse_usb_frame(
            r#"{"type":"calibration_job","requestId":"req-005","op":"start","kind":"vin_adc_auto"}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::CalibrationJob {
                request_id: string("req-005"),
                command: CalibrationJobCommandWire {
                    op: CalibrationJobOpWire::Start,
                    kind: Some(CalibrationJobKindWire::VinAdcAuto),
                },
            }
        );
    }

    #[test]
    fn parse_calibration_job_frame_rejects_removed_heater_curve_auto() {
        assert!(parse_usb_frame(
            r#"{"type":"calibration_job","requestId":"req-removed","op":"start","kind":"heater_curve_auto"}"#,
        )
        .is_err());
    }

    #[test]
    fn parse_usb_request_accepts_long_calibration_job_op() {
        let frame = parse_usb_frame(
            r#"{"type":"request","requestId":"req-006","op":"get_calibration_job"}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("req-006"),
                op: UsbRequestOp::GetCalibrationJob,
            }
        );
    }

    #[test]
    fn parse_usb_request_accepts_thermal_tuning_snapshot_op() {
        let frame = parse_usb_frame(
            r#"{"type":"request","requestId":"req-thermal","op":"get_thermal_tuning_run"}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("req-thermal"),
                op: UsbRequestOp::GetThermalTuningRun,
            }
        );
    }

    #[test]
    fn write_calibration_job_frame_uses_kind_alias_for_host() {
        let frame = UsbFrame::CalibrationJob {
            request_id: string("req-007"),
            command: CalibrationJobCommandWire {
                op: CalibrationJobOpWire::Start,
                kind: Some(CalibrationJobKindWire::ThermalPlantAuto),
            },
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();
        assert!(json.contains(r#""type":"calibration_job""#));
        assert!(json.contains(r#""kind":"thermal_plant_auto""#));
    }

    #[test]
    fn thermal_plant_run_frame_round_trips_cursor_and_cooling_phase() {
        let request =
            parse_usb_frame(r#"{"type":"thermal_plant_run","requestId":"run-1","afterSample":32}"#)
                .unwrap();
        assert_eq!(
            request,
            UsbFrame::ThermalPlantRun {
                request_id: string("run-1"),
                after_sample: 32,
            }
        );

        let snapshot = ThermalPlantRunSnapshotWire {
            version: 1,
            attempt: None,
            trace_page: ThermalPlantTracePageWire {
                start_sample: 32,
                next_sample: None,
                total_samples: 33,
                points: heapless::Vec::from_slice(&[ThermalPlantTracePointWire {
                    sample_index: 32,
                    elapsed_ms: 1_600,
                    temperature_centi_c: 8_000,
                    heater_voltage_mv: 0,
                    duty_percent: 0,
                    phase: ThermalPlantRunPhaseWire::Cooling,
                }])
                .unwrap(),
            },
            provisional_curve: None,
            active_result: None,
        };
        let response = UsbFrame::Response {
            request_id: string("run-1"),
            ok: true,
            result: Some(UsbResponsePayload::ThermalPlantRun(snapshot.clone())),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.len() < USB_LINE_MAX_LEN);
        assert!(json.contains(r#""phase":"cooling""#));
        assert_eq!(parse_usb_frame(json).unwrap(), response);
    }

    #[test]
    fn eeprom_maintenance_frames_preserve_raw_chunk_bytes() {
        let frame = parse_usb_frame(
            r#"{"type":"eeprom_maintenance","requestId":"raw-1","op":"write","offset":8160,"bytes":[0,255,17,34]}"#,
        )
        .unwrap();
        assert_eq!(
            frame,
            UsbFrame::EepromMaintenance {
                request_id: string("raw-1"),
                command: EepromMaintenanceCommand {
                    op: EepromMaintenanceOp::Write,
                    offset: Some(8160),
                    length: None,
                    bytes: Some(heapless::Vec::from_slice(&[0, 255, 17, 34]).unwrap()),
                },
            }
        );

        let response = UsbFrame::Response {
            request_id: string("raw-1"),
            ok: true,
            result: Some(UsbResponsePayload::EepromBytes(
                heapless::Vec::from_slice(&[0, 255, 17, 34]).unwrap(),
            )),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.contains(r#""eeprom_bytes":[0,255,17,34]"#));
    }

    #[test]
    fn malformed_frame_returns_protocol_error() {
        assert_eq!(
            parse_usb_frame(r#"{"type":"request","op":"get_status"}"#),
            Err(UsbFrameError::MalformedJson)
        );
    }

    #[test]
    fn wifi_config_receipt_carries_a_redacted_versioned_network_snapshot() {
        let receipt = WifiConfigReceipt {
            wifi: RedactedWifiConfig {
                op: WifiConfigOp::Set,
                ssid: Some(string("FluxPurr-Lab")),
                password: Some(string("<redacted>")),
                static_ipv4: None,
                telemetry_interval_ms: None,
            },
            network: NetworkSummary {
                state: NetworkState::Connecting,
                configuration_generation: 7,
                transition_sequence: 19,
                failure_code: None,
                ..NetworkSummary::default()
            },
        };
        let response = UsbFrame::Response {
            request_id: string("wifi-v2"),
            ok: true,
            result: Some(UsbResponsePayload::Wifi(receipt)),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();

        assert!(json.contains(r#""configurationGeneration":7"#));
        assert!(json.contains(r#""transitionSequence":19"#));
        assert!(!json.contains("secret-pass"));
    }
}
