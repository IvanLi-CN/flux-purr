use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

use crate::{
    DeviceMode, DeviceStatus, PdState,
    frontpanel::{FRONTPANEL_PRESET_COUNT, FrontPanelKey, HeaterLockReason},
    memory::{
        ADC_CALIBRATION_MAX_SAMPLES, AdcCalibrationChannel, AdcCalibrationFit,
        AdcCalibrationSample, AdcCalibrationSlotFit, AdcCalibrationSlotId, HEATER_CURVE_MAX_POINTS,
        HeaterCurveConfig, HeaterCurvePoint, MEMORY_WIFI_PASSWORD_MAX_LEN,
        MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig, ThermalControlProfileConfig,
        ThermalControlProfilePointConfig, ThermalControlProfileSettingsConfig,
        ThermalPlantRawAnchor, ThermalPlantRawTransaction, ThermalProfileBank, ThermalProfileMode,
        adc_calibration_fit, thermal_plant_raw_transaction_is_complete,
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
pub const CAPABILITY_COUNT_MAX: usize = 10;
// A fully materialized 9-point thermal profile save request is about 5 KiB.
// Keep one shared bound for firmware and devd JSONL frames so it can persist.
pub const USB_LINE_MAX_LEN: usize = 8 * 1024;
pub const REQUEST_ID_MAX_LEN: usize = 48;
pub const ERROR_CODE_MAX_LEN: usize = 48;
pub const ERROR_MESSAGE_MAX_LEN: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSummary {
    pub state: NetworkState,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
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
            ssid: None,
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
}

impl Identity {
    pub fn firmware_default() -> Self {
        let mut capabilities = Vec::new();
        push_str(&mut capabilities, "identity");
        push_str(&mut capabilities, "status");
        push_str(&mut capabilities, "network");
        push_str(&mut capabilities, "calibration");
        #[cfg(feature = "web_serial")]
        {
            push_str(&mut capabilities, "usb_jsonl");
            push_str(&mut capabilities, "wifi_config");
            push_str(&mut capabilities, "monitor");
        }
        Self {
            device_id: string("flux-purr-s3-001"),
            firmware_version: string(env!("CARGO_PKG_VERSION")),
            build_id: string(option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("host-build")),
            git_sha: string(option_env!("GIT_SHA").unwrap_or("unknown")),
            board: string("esp32-s3"),
            api_version: string(CONTROL_PLANE_API_VERSION),
            protocol_version: string(USB_PROTOCOL_VERSION),
            hostname: string("flux-purr-s3-001"),
            capabilities,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FanDisplayState {
    Off,
    Auto,
    Run,
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
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: PdStateWire,
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
    pub candidate_transaction_id: Option<u32>,
    pub active_transaction_id: Option<u32>,
    pub projection_valid: bool,
    pub convection_mw_per_c: Option<f32>,
    pub radiation_mw_per_k4: Option<f32>,
    pub thermal_capacity_mj_per_c: Option<f32>,
    pub transport_delay_ms: Option<u32>,
}

impl ControlPlaneStatus {
    pub fn from_device_status(
        status: DeviceStatus,
        memory: &MemoryConfig,
        uptime_seconds: u32,
        network: NetworkSummary,
    ) -> Self {
        let heater_output_percent = status.heater_output_percent.min(100);
        let fan_display_state = if !memory.active_cooling_enabled {
            FanDisplayState::Off
        } else if status.fan_enabled {
            FanDisplayState::Run
        } else {
            FanDisplayState::Auto
        };

        Self {
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
            pd_request_mv: status.pd_request_mv,
            pd_contract_mv: status.pd_contract_mv,
            pd_state: status.pd_state.into(),
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
        }
    }

    pub fn with_runtime_target_temp_c(mut self, target_temp_c: i16) -> Self {
        self.target_temp_c = target_temp_c;
        self
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
    HeaterCurveAuto,
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
    pub password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    pub auto_reconnect: Option<bool>,
    pub telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WifiConfigOp {
    Set,
    Clear,
}

impl WifiConfigCommand {
    pub fn apply_to(&self, config: &mut MemoryConfig) {
        match self.op {
            WifiConfigOp::Clear => {
                config.wifi_ssid.clear();
                config.wifi_password.clear();
                config.wifi_auto_reconnect = false;
            }
            WifiConfigOp::Set => {
                config.wifi_ssid.clear();
                if let Some(ssid) = &self.ssid {
                    let _ = config.wifi_ssid.push_str(ssid);
                }
                config.wifi_password.clear();
                if let Some(password) = &self.password {
                    let _ = config.wifi_password.push_str(password);
                }
                if let Some(auto_reconnect) = self.auto_reconnect {
                    config.wifi_auto_reconnect = auto_reconnect;
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
            auto_reconnect: self.auto_reconnect,
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
    pub auto_reconnect: Option<bool>,
    pub telemetry_interval_ms: Option<u32>,
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
    pub thermal_plant_model: Option<ThermalPlantModelCommandWire>,
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
        if let Some(command) = self.thermal_plant_model {
            match command.op {
                ThermalPlantModelOpWire::SaveCandidate => {
                    if let Some(transaction) = command.transaction.map(Into::into)
                        && thermal_plant_raw_transaction_is_complete(&transaction)
                    {
                        config.thermal_plant_candidate = Some(transaction);
                    }
                }
                ThermalPlantModelOpWire::PromoteCandidate => {
                    if config.thermal_plant_candidate.is_some_and(|candidate| {
                        Some(candidate.transaction_id) == command.transaction_id
                    }) {
                        config.thermal_plant_active = config.thermal_plant_candidate;
                    }
                }
                ThermalPlantModelOpWire::ClearCandidate => {
                    config.thermal_plant_candidate = None;
                }
            }
        }
        config.sanitize();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalPlantModelOpWire {
    SaveCandidate,
    PromoteCandidate,
    ClearCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRawAnchorWire {
    pub ambient_raw_rtd_adc_mv: u16,
    pub target_raw_rtd_adc_mv: u16,
    pub heater_voltage_mv: u16,
    pub heater_current_ma: u16,
    pub gate_off_idle_power_mw: u32,
    pub steady_hold_power_mw: u32,
    pub ramp_duration_ms: u32,
    pub ramp_energy_mj: u32,
}

impl From<ThermalPlantRawAnchorWire> for ThermalPlantRawAnchor {
    fn from(value: ThermalPlantRawAnchorWire) -> Self {
        Self {
            ambient_raw_rtd_adc_mv: value.ambient_raw_rtd_adc_mv,
            target_raw_rtd_adc_mv: value.target_raw_rtd_adc_mv,
            heater_voltage_mv: value.heater_voltage_mv,
            heater_current_ma: value.heater_current_ma,
            gate_off_idle_power_mw: value.gate_off_idle_power_mw,
            steady_hold_power_mw: value.steady_hold_power_mw,
            ramp_duration_ms: value.ramp_duration_ms,
            ramp_energy_mj: value.ramp_energy_mj,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantRawTransactionWire {
    pub transaction_id: u32,
    pub anchors: [ThermalPlantRawAnchorWire; 2],
}

impl From<ThermalPlantRawTransactionWire> for ThermalPlantRawTransaction {
    fn from(value: ThermalPlantRawTransactionWire) -> Self {
        Self {
            transaction_id: value.transaction_id,
            anchors: value.anchors.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalPlantModelCommandWire {
    pub op: ThermalPlantModelOpWire,
    pub transaction_id: Option<u32>,
    pub transaction: Option<ThermalPlantRawTransactionWire>,
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
    pub fn from_memory(config: &HeaterCurveConfig) -> Self {
        let mut points = [None; HEATER_CURVE_MAX_POINTS];
        for (index, point) in config.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        Self { points }
    }

    pub fn to_memory(self) -> HeaterCurveConfig {
        let mut points = [None; HEATER_CURVE_MAX_POINTS];
        for (index, point) in self.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        HeaterCurveConfig { points }
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
    preview: Option<&HeaterCurveConfig>,
) -> HeaterCurveStateWire {
    HeaterCurveStateWire {
        active: HeaterCurvePackageWire::from_memory(&config.active_heater_curve),
        preview: preview.map(HeaterCurvePackageWire::from_memory),
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
    HeaterCurveConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: HeaterCurveConfigCommand,
    },
    HeaterCurveSave {
        request_id: String<REQUEST_ID_MAX_LEN>,
    },
    Response {
        request_id: String<REQUEST_ID_MAX_LEN>,
        ok: bool,
        result: Option<UsbResponsePayload>,
        error: Option<ApiError>,
    },
    Status {
        status: ControlPlaneStatus,
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
    #[serde(rename = "protocolVersion")]
    protocol_version: Option<String<24>>,
    framing: Option<String<8>>,
    identity: Option<Identity>,
    capabilities: Option<Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>>,
    #[serde(rename = "requestId")]
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<24>>,
    ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    auto_reconnect: Option<bool>,
    telemetry_interval_ms: Option<u32>,
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
    thermal_plant_model: Option<ThermalPlantModelCommandWire>,
    channel: Option<CalibrationChannelWire>,
    reference_temp_c: Option<f32>,
    reference_vin_mv: Option<u32>,
    target_adc_mv: Option<u16>,
    observed_mv: Option<u16>,
    expected_mv: Option<u16>,
    sample_index: Option<usize>,
    #[serde(rename = "kind", alias = "jobKind")]
    job_kind: Option<CalibrationJobKindWire>,
    state: Option<CalibrationStateWire>,
    slot: Option<CalibrationSlotIdWire>,
    fit: Option<CalibrationSlotFitWire>,
    heater_curve: Option<HeaterCurvePackageWire>,
    ok: Option<bool>,
    result: Option<UsbResponsePayload>,
    error: Option<ApiError>,
    status: Option<ControlPlaneStatus>,
    level: Option<String<8>>,
    message: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

impl TryFrom<UsbFrameWire> for UsbFrame {
    type Error = UsbFrameError;

    fn try_from(value: UsbFrameWire) -> Result<Self, <UsbFrame as TryFrom<UsbFrameWire>>::Error> {
        match value.frame_type.as_str() {
            "hello" => Ok(UsbFrame::Hello {
                protocol_version: value.protocol_version.ok_or(UsbFrameError::MalformedJson)?,
                framing: value.framing.ok_or(UsbFrameError::MalformedJson)?,
                identity: value.identity.ok_or(UsbFrameError::MalformedJson)?,
                capabilities: value.capabilities.ok_or(UsbFrameError::MalformedJson)?,
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
                    auto_reconnect: value.auto_reconnect,
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
                    calibration: value.calibration,
                    thermal_profile_mode: value.thermal_profile_mode,
                    thermal_control_profile: value.thermal_control_profile,
                    thermal_plant_model: value.thermal_plant_model,
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
                    state: value.state,
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
            "heater_curve_config" => Ok(UsbFrame::HeaterCurveConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: HeaterCurveConfigCommand {
                    op: parse_heater_curve_config_op(value.op.as_deref())?,
                    package: value.heater_curve,
                },
            }),
            "heater_curve_save" => Ok(UsbFrame::HeaterCurveSave {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "response" => Ok(UsbFrame::Response {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                ok: value.ok.ok_or(UsbFrameError::MalformedJson)?,
                result: value.result,
                error: value.error,
            }),
            "status" => Ok(UsbFrame::Status {
                status: value.status.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "log" => Ok(UsbFrame::Log {
                level: value.level.ok_or(UsbFrameError::MalformedJson)?,
                message: value.message.ok_or(UsbFrameError::MalformedJson)?,
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
            auto_reconnect: None,
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
            thermal_plant_model: None,
            channel: None,
            reference_temp_c: None,
            reference_vin_mv: None,
            target_adc_mv: None,
            observed_mv: None,
            expected_mv: None,
            sample_index: None,
            job_kind: None,
            state: None,
            slot: None,
            fit: None,
            heater_curve: None,
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
                wire.identity = Some(identity.clone());
                wire.capabilities = Some(capabilities.clone());
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
                wire.auto_reconnect = config.auto_reconnect;
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
                wire.calibration = config.calibration;
                wire.thermal_profile_mode = config.thermal_profile_mode;
                wire.thermal_control_profile = config.thermal_control_profile;
                wire.thermal_plant_model = config.thermal_plant_model;
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
                wire.state = config.state;
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
            UsbFrame::HeaterCurveConfig { request_id, config } => {
                wire.frame_type = string("heater_curve_config");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(config.op.as_str()));
                wire.heater_curve = config.package;
            }
            UsbFrame::HeaterCurveSave { request_id } => {
                wire.frame_type = string("heater_curve_save");
                wire.request_id = Some(request_id.clone());
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
                wire.result = result.clone();
                wire.error = error.clone();
            }
            UsbFrame::Status { status } => {
                wire.frame_type = string("status");
                wire.status = Some(status.clone());
            }
            UsbFrame::Log { level, message } => {
                wire.frame_type = string("log");
                wire.level = Some(level.clone());
                wire.message = Some(message.clone());
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
    GetNetwork,
    GetStatus,
    GetCalibration,
    GetCalibrationJob,
    GetHeaterCurve,
    SetLogLevel,
}

impl UsbRequestOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::GetIdentity => "get_identity",
            Self::GetNetwork => "get_network",
            Self::GetStatus => "get_status",
            Self::GetCalibration => "get_calibration",
            Self::GetCalibrationJob => "get_calibration_job",
            Self::GetHeaterCurve => "get_heater_curve",
            Self::SetLogLevel => "set_log_level",
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
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbResponsePayload {
    Identity(Identity),
    Network(NetworkSummary),
    Status(ControlPlaneStatus),
    Wifi(RedactedWifiConfig),
    Calibration(CalibrationStateWire),
    CalibrationJob(CalibrationJobStateWire),
    HeaterCurve(HeaterCurveStateWire),
    Ack,
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
    serde_json_core::from_str::<UsbFrameWire>(trimmed)
        .map(|(frame, _)| frame)
        .map_err(|_| UsbFrameError::MalformedJson)?
        .try_into()
        .map_err(|_| UsbFrameError::MalformedJson)
}

pub fn write_usb_frame<'a>(frame: &UsbFrame, out: &'a mut [u8]) -> Result<&'a str, UsbFrameError> {
    let wire = UsbFrameWire::from(frame);
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
            NetworkState::Idle
        } else {
            NetworkState::Disabled
        },
        ssid,
        ..NetworkSummary::default()
    }
}

fn string<const N: usize>(value: &str) -> String<N> {
    let mut out = String::new();
    let _ = out.push_str(value);
    out
}

fn push_str<const N: usize, const C: usize>(values: &mut Vec<String<N>, C>, value: &str) {
    let _ = values.push(string(value));
}

fn parse_usb_request_op(value: Option<&str>) -> Result<UsbRequestOp, UsbFrameError> {
    match value {
        Some("get_identity") => Ok(UsbRequestOp::GetIdentity),
        Some("get_network") => Ok(UsbRequestOp::GetNetwork),
        Some("get_status") => Ok(UsbRequestOp::GetStatus),
        Some("get_calibration") => Ok(UsbRequestOp::GetCalibration),
        Some("get_calibration_job") => Ok(UsbRequestOp::GetCalibrationJob),
        Some("get_heater_curve") => Ok(UsbRequestOp::GetHeaterCurve),
        Some("set_log_level") => Ok(UsbRequestOp::SetLogLevel),
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

fn parse_wifi_config_op(value: Option<&str>) -> Result<WifiConfigOp, UsbFrameError> {
    match value {
        Some("set") => Ok(WifiConfigOp::Set),
        Some("clear") => Ok(WifiConfigOp::Clear),
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
    use std::format;

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
        assert_eq!(status.network.state, NetworkState::Idle);
        assert_eq!(status.network.ssid.as_deref(), Some("FluxPurr-Lab"));
        assert_eq!(status.frontpanel_key, Some(FrontPanelKeyWire::Center));
        assert_eq!(status.heater_lock_reason, None);
    }

    #[test]
    fn status_frame_serializes_pd_fallback_for_web_contract() {
        let status = ControlPlaneStatus::from_device_status(
            snapshot_at(17, 0),
            &MemoryConfig::default(),
            17,
            NetworkSummary::default(),
        );
        let frame = UsbFrame::Response {
            request_id: string("req-pd"),
            ok: true,
            result: Some(UsbResponsePayload::Status(status)),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""pdState":"fallback_5v""#));
        assert!(json.contains(r#""manualPpsEnabled":false"#));
        assert!(json.contains(r#""heaterLockReason":null"#));
        assert!(json.contains(r#""ppsCapabilityMinMv":null"#));
        assert!(!json.contains("fallback5v"));
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
            result: Some(UsbResponsePayload::Status(status)),
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
            auto_reconnect: Some(true),
            telemetry_interval_ms: Some(750),
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);
        assert_eq!(config.wifi_ssid.as_str(), "FluxPurr-Lab");
        assert_eq!(config.wifi_password.as_str(), "secret-pass");
        assert_eq!(
            command.redacted_summary().password.as_deref(),
            Some("<redacted>")
        );
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
            thermal_plant_model: None,
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
            thermal_plant_model: None,
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
            thermal_plant_model: None,
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
            thermal_plant_model: None,
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
            thermal_plant_model: None,
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
    fn parse_wifi_frame_and_write_redacted_response() {
        let frame = parse_usb_frame(
            r#"{"type":"wifi_config","requestId":"req-002","op":"set","ssid":"FluxPurr-Lab","password":"secret-pass","autoReconnect":true,"telemetryIntervalMs":500}"#,
        )
        .unwrap();
        let UsbFrame::WifiConfig { request_id, config } = frame else {
            panic!("expected wifi frame");
        };
        assert_eq!(request_id.as_str(), "req-002");
        assert_eq!(config.password.as_deref(), Some("secret-pass"));

        let response = UsbFrame::Response {
            request_id,
            ok: true,
            result: Some(UsbResponsePayload::Wifi(config.redacted_summary())),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.contains(r#""password":"<redacted>""#));
        assert!(!json.contains("secret-pass"));
        assert!(json.ends_with('\n'));
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
                    thermal_plant_model: None,
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
                thermal_plant_model: None,
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
                    thermal_plant_model: None,
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
    fn write_calibration_job_frame_uses_kind_alias_for_host() {
        let frame = UsbFrame::CalibrationJob {
            request_id: string("req-007"),
            command: CalibrationJobCommandWire {
                op: CalibrationJobOpWire::Start,
                kind: Some(CalibrationJobKindWire::HeaterCurveAuto),
            },
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();
        assert!(json.contains(r#""type":"calibration_job""#));
        assert!(json.contains(r#""kind":"heater_curve_auto""#));
    }

    #[test]
    fn malformed_frame_returns_protocol_error() {
        assert_eq!(
            parse_usb_frame(r#"{"type":"request","op":"get_status"}"#),
            Err(UsbFrameError::MalformedJson)
        );
    }
}
