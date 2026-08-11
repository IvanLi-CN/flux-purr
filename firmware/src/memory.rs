use heapless::String;

use crate::frontpanel::{
    FRONTPANEL_PRESET_COUNT, FRONTPANEL_TARGET_TEMP_MAX_C, FRONTPANEL_TARGET_TEMP_MIN_C,
};

pub const M24C64_I2C_ADDRESS: u8 = 0x50;
pub const M24C64_CAPACITY_BYTES: u16 = 8 * 1024;
pub const M24C64_PAGE_SIZE: usize = 32;
pub const MEMORY_SLOT_SIZE: usize = 2048;
pub const MEMORY_SLOT_A_OFFSET: u16 = 0x1000;
pub const MEMORY_SLOT_B_OFFSET: u16 = 0x1800;
pub const PREVIOUS_MEMORY_SLOT_SIZE: usize = 1024;
pub const PREVIOUS_MEMORY_SLOT_A_OFFSET: u16 = 0x0400;
pub const PREVIOUS_MEMORY_SLOT_B_OFFSET: u16 = 0x0800;
pub const LEGACY_MEMORY_SLOT_SIZE: usize = 512;
pub const LEGACY_MEMORY_SLOT_A_OFFSET: u16 = 0x0000;
pub const LEGACY_MEMORY_SLOT_B_OFFSET: u16 = 0x0200;
pub const MEMORY_RECORD_FORMAT_VERSION: u8 = 5;
pub const MEMORY_RECORD_HEADER_LEN: usize = 16;
pub const MEMORY_RECORD_PAYLOAD_MAX: usize = MEMORY_SLOT_SIZE - MEMORY_RECORD_HEADER_LEN;
pub const MEMORY_WIFI_SSID_MAX_LEN: usize = 32;
pub const MEMORY_WIFI_PASSWORD_MAX_LEN: usize = 64;
pub const MEMORY_WRITE_DEBOUNCE_MS: u64 = 2_000;
pub const ADC_CALIBRATION_MAX_SAMPLES: usize = 8;
pub const HEATER_CURVE_MAX_POINTS: usize = 8;
pub const THERMAL_PLANT_ANCHOR_COUNT: usize = 2;
pub const THERMAL_PLANT_TRANSIENT_MAX_SAMPLES: usize = 128;
pub const THERMAL_CONTROL_PROFILE_MAX_POINTS: usize = FRONTPANEL_PRESET_COUNT;
pub const THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS: usize = THERMAL_CONTROL_PROFILE_MAX_POINTS;
pub const THERMAL_CONTROL_PROFILE_TEMP_FILTER_ALPHA_PERMILLE_DEFAULT: u16 = 750;
pub const THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT: u16 = 400;
pub const THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT: u16 = 90;
pub const THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT: u16 = 200;
pub const THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT: u16 = 30;
pub const THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT: u16 = 5;
pub const THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT: u16 = 25;
pub const THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_DEFAULT: u16 = 5;
pub const THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX: u16 = 255;
pub const THERMAL_CONTROL_PROFILE_APPROACH_MIN_POWER_RATIO_PERMILLE_DEFAULT: u16 = 0;
pub const THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT: u16 = 120;
pub const THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT: u16 = 12;
pub const THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT: u16 = 12;
pub const THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT: u16 = 0;
pub const THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT: u16 = 0;
pub const THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT: u16 = 0;
pub const THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT: u16 = 1_000;
pub const THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX: u16 = 4_000;
pub const THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX: u16 = 375;
pub const THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT: u16 = 5_000;
pub const THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN: u16 = 5_000;
pub const THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX: u16 = 28_000;
pub const THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_DEFAULT: u16 = 200;
pub const THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX: u16 = 1_000;
pub const ADC_CALIBRATION_RTD_DEFAULT_LOW_MV: u16 = 0;
pub const ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV: u16 = 2_800;
pub const ADC_CALIBRATION_VIN_DEFAULT_LOW_MV: u16 = 0;
pub const ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV: u16 = VIN_DEFAULT_ADC_HIGH_MV;
pub const CALIBRATION_REFERENCE_NONE_WIRE_VALUE: i16 = i16::MIN;
const ADC_CALIBRATION_SAMPLE_PAYLOAD_LEN: usize = ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2;
const ADC_CALIBRATION_REFERENCE_PAYLOAD_LEN: usize = ADC_CALIBRATION_MAX_SAMPLES * 2 * 2;
const ADC_CALIBRATION_TARGET_PAYLOAD_LEN: usize = ADC_CALIBRATION_MAX_SAMPLES * 2;
const ADC_CALIBRATION_SLOT_PAYLOAD_LEN: usize = 2 * 2 * (core::mem::size_of::<f32>() * 2);
const ADC_CALIBRATION_ACTIVE_SLOT_PAYLOAD_LEN: usize = 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY: usize = 10 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_APPROACH_MIN_RATIO: usize = 11 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS: usize = 14 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT: usize = 15 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_PREVIOUS_FIELD: usize = 16 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS: usize = 18 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN: usize = 17 * 2;
const THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY: usize = 7 * 2;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY: usize = 8;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS: usize = 28;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT: usize = 30;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP: usize = 32;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN: usize = 34;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON: usize = 36;
const THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER: usize = 38;
const THERMAL_CONTROL_PROFILE_LAYOUT_MARKER: [u8; 4] = *b"TCP2";
const THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN: usize =
    THERMAL_CONTROL_PROFILE_LAYOUT_MARKER.len();
const THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_VALUE_MASK: u16 = 0x0fff;
const THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_STEP_CENTI_C: u16 = 25;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_LEGACY: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_LEAD_TICKS: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_HOLD_REHEAT: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_WARMUP: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_HOLD_ON: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS
        * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON;
const THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER: usize =
    THERMAL_CONTROL_PROFILE_MAX_POINTS
        * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER;
const THERMAL_CONTROL_PROFILE_PAYLOAD_LEN: usize = THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
    + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
    + THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER;
const THERMAL_PLANT_TRANSIENT_HEADER_LEN: usize = 24;
const THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN: usize = 6;

const MEMORY_RECORD_MAGIC: [u8; 4] = *b"FPM1";
const PRESET_NONE_WIRE_VALUE: i16 = i16::MIN;
const CALIBRATION_NONE_WIRE_VALUE: u16 = u16::MAX;
const VIN_DEFAULT_ADC_HIGH_MV: u16 = 2_337;

const TLV_TARGET_TEMP_C: u8 = 0x01;
const TLV_SELECTED_PRESET_SLOT: u8 = 0x02;
const TLV_PRESETS_C: u8 = 0x03;
const TLV_ACTIVE_COOLING_ENABLED: u8 = 0x04;
const TLV_WIFI_SSID: u8 = 0x10;
const TLV_WIFI_PASSWORD: u8 = 0x11;
const TLV_WIFI_AUTO_RECONNECT: u8 = 0x12;
const TLV_TELEMETRY_INTERVAL_MS: u8 = 0x13;
const TLV_ADC_CALIBRATION_SAMPLES: u8 = 0x20;
const TLV_LEGACY_DRAFT_ADC_CALIBRATION: u8 = 0x21;
const TLV_ADC_CALIBRATION_REFERENCES: u8 = 0x22;
const TLV_LEGACY_DRAFT_ADC_CALIBRATION_REFERENCES: u8 = 0x23;
const TLV_ADC_CALIBRATION_TARGETS: u8 = 0x24;
const TLV_LEGACY_DRAFT_ADC_CALIBRATION_TARGETS: u8 = 0x25;
const TLV_ADC_CALIBRATION_SLOTS: u8 = 0x26;
const TLV_ADC_CALIBRATION_ACTIVE_SLOTS: u8 = 0x27;
const TLV_ACTIVE_HEATER_CURVE: u8 = 0x30;
const TLV_ACTIVE_THERMAL_CONTROL_PROFILE: u8 = 0x31;
const TLV_THERMAL_CONTROL_PROFILE_PPS3A: u8 = 0x32;
const TLV_THERMAL_CONTROL_PROFILE_PPS5A: u8 = 0x33;
const TLV_THERMAL_PROFILE_MODE: u8 = 0x34;
const TLV_HEATER_CURVE_RAW_OBSERVATIONS: u8 = 0x35;
const TLV_THERMAL_PLANT_CANDIDATE: u8 = 0x36;
const TLV_THERMAL_PLANT_ACTIVE: u8 = 0x37;
const TLV_LAN_PAIRING_TOKEN: u8 = 0x38;
const TLV_WIFI_STATIC_IPV4: u8 = 0x39;
const TLV_THERMAL_PLANT_TRANSIENT_ACTIVE: u8 = 0x3a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiStaticIpv4Config {
    pub address: [u8; 4],
    pub prefix_len: u8,
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
}

impl WifiStaticIpv4Config {
    pub fn is_valid(self) -> bool {
        self.prefix_len <= 32
            && is_unicast_ipv4(self.address)
            && is_unicast_ipv4(self.gateway)
            && is_unicast_ipv4(self.dns)
    }
}

fn is_unicast_ipv4(address: [u8; 4]) -> bool {
    let first = address[0];
    first != 0 && first != 127 && first < 224
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryConfig {
    pub target_temp_c: i16,
    pub selected_preset_slot: usize,
    pub presets_c: [Option<i16>; FRONTPANEL_PRESET_COUNT],
    pub active_cooling_enabled: bool,
    pub wifi_ssid: String<MEMORY_WIFI_SSID_MAX_LEN>,
    pub wifi_password: String<MEMORY_WIFI_PASSWORD_MAX_LEN>,
    pub wifi_auto_reconnect: bool,
    /// `None` selects DHCP. A value is explicitly configured through USB/devd
    /// and is never writable from the LAN HTTP surface.
    pub wifi_static_ipv4: Option<WifiStaticIpv4Config>,
    /// Stable LAN bearer token. This is never included in control-plane
    /// status, logs, traces, or user-facing EEPROM exports.
    pub lan_pairing_token: Option<[u8; crate::lan::LAN_TOKEN_BYTES]>,
    pub telemetry_interval_ms: u32,
    pub adc_calibration: AdcCalibrationConfig,
    pub active_heater_curve: HeaterCurveConfig,
    pub heater_curve_raw_observations: HeaterCurveRawObservations,
    /// Read-only two-platform records from the removed steady-state calibration.
    /// They remain decodable so later configuration writes do not reject a legacy
    /// record, but they never unlock production heating.
    pub thermal_plant_active: Option<ThermalPlantRawTransaction>,
    pub thermal_plant_transient_active: Option<ThermalPlantTransientTransaction>,
    /// The legacy field is the persisted 3 A / 65 W bank. Keeping its name makes
    /// v1 record migration explicit and avoids a silent behavior change for callers.
    pub active_thermal_control_profile: ThermalControlProfileConfig,
    pub thermal_control_profile_pps5a: ThermalControlProfileConfig,
    pub thermal_profile_mode: ThermalProfileMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalProfileMode {
    Auto,
    W65,
    W100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalProfileBank {
    Pps3a,
    Pps5a,
}

impl ThermalProfileMode {
    pub const fn default_bank(self) -> ThermalProfileBank {
        match self {
            Self::W100 => ThermalProfileBank::Pps5a,
            Self::Auto | Self::W65 => ThermalProfileBank::Pps3a,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::W65 => "65w",
            Self::W100 => "100w",
        }
    }
}

impl ThermalProfileBank {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pps3a => "pps3a",
            Self::Pps5a => "pps5a",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcCalibrationChannel {
    Rtd,
    Vin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcCalibrationSlotId {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcCalibrationSample {
    pub observed_mv: u16,
    pub expected_mv: u16,
    pub reference_temp_deci_c: Option<i16>,
    pub target_adc_mv: Option<u16>,
    pub reference_vin_mv: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdcCalibrationSlotFit {
    pub gain: f32,
    pub offset_mv: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AdcCalibrationSlots {
    pub a: AdcCalibrationSlotFit,
    pub b: AdcCalibrationSlotFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurvePoint {
    pub temp_centi_c: i16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurveConfig {
    pub points: [Option<HeaterCurvePoint>; HEATER_CURVE_MAX_POINTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurveRawObservation {
    pub raw_rtd_adc_mv: u16,
    pub heater_voltage_mv: u16,
    pub heater_current_ma: u16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurveRawObservations {
    pub points: [Option<HeaterCurveRawObservation>; HEATER_CURVE_MAX_POINTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalPlantRawAnchor {
    pub ambient_raw_rtd_adc_mv: u16,
    pub target_raw_rtd_adc_mv: u16,
    pub heater_voltage_mv: u16,
    pub heater_current_ma: u16,
    pub gate_off_idle_power_mw: u32,
    pub steady_hold_power_mw: u32,
    pub ramp_duration_ms: u32,
    pub ramp_energy_mj: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalPlantRawTransaction {
    pub transaction_id: u32,
    pub anchors: [ThermalPlantRawAnchor; THERMAL_PLANT_ANCHOR_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalPlantTransientSample {
    /// 50 ms ticks from the start of the automatic job.
    pub elapsed_ticks: u16,
    pub raw_rtd_adc_mv: u16,
    /// Measured heater voltage, quantized to 100 mV. The calibration source is
    /// bounded to 20 V, so this fits in a byte without losing useful precision.
    pub heater_voltage_100mv: u8,
    pub duty_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalPlantProjectionRecord {
    pub convection_mw_per_c_bits: u32,
    pub radiation_mw_per_k4_bits: u32,
    pub thermal_capacity_mj_per_c_bits: u32,
    pub transport_delay_ms: u32,
}

impl ThermalPlantProjectionRecord {
    pub fn from_projection(value: ThermalPlantProjection) -> Self {
        Self {
            convection_mw_per_c_bits: value.convection_mw_per_c.to_bits(),
            radiation_mw_per_k4_bits: value.radiation_mw_per_k4.to_bits(),
            thermal_capacity_mj_per_c_bits: value.thermal_capacity_mj_per_c.to_bits(),
            transport_delay_ms: value.transport_delay_ms,
        }
    }

    pub fn projection(self) -> ThermalPlantProjection {
        ThermalPlantProjection {
            convection_mw_per_c: f32::from_bits(self.convection_mw_per_c_bits),
            radiation_mw_per_k4: f32::from_bits(self.radiation_mw_per_k4_bits),
            thermal_capacity_mj_per_c: f32::from_bits(self.thermal_capacity_mj_per_c_bits),
            transport_delay_ms: self.transport_delay_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalPlantTransientTransaction {
    pub transaction_id: u32,
    pub ambient_raw_rtd_adc_mv: u16,
    pub sample_count: u8,
    pub projection: ThermalPlantProjectionRecord,
    pub samples: [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalPlantProjection {
    pub convection_mw_per_c: f32,
    pub radiation_mw_per_k4: f32,
    pub thermal_capacity_mj_per_c: f32,
    pub transport_delay_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalControlProfilePointConfig {
    pub target_temp_c: i16,
    pub brake_distance_centi_c: u16,
    pub warmup_power_permille: u16,
    pub warmup_reenter_centi_c: u16,
    pub approach_power_permille: u16,
    pub approach_floor_power_permille: u16,
    pub approach_damping_exponent_permille: u16,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalControlProfileConfig {
    pub settings: ThermalControlProfileSettingsConfig,
    pub points: [Option<ThermalControlProfilePointConfig>; THERMAL_CONTROL_PROFILE_MAX_POINTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalControlProfileSettingsConfig {
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
    pub hold_blend_ticks: u16,
    pub hold_reheat_power_permille: u16,
    pub approach_lead_ticks: u16,
    pub hold_lead_ticks: u16,
    pub auto_adjustable_working_floor_mv: u16,
    pub heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdcCalibrationChannelConfig {
    pub samples: [Option<AdcCalibrationSample>; ADC_CALIBRATION_MAX_SAMPLES],
    pub slots: AdcCalibrationSlots,
    pub active_slot: AdcCalibrationSlotId,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AdcCalibrationConfig {
    pub rtd: AdcCalibrationChannelConfig,
    pub vin: AdcCalibrationChannelConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdcCalibrationFit {
    pub gain: f32,
    pub offset_mv: f32,
    pub sample_count: usize,
}

impl Default for AdcCalibrationSlotFit {
    fn default() -> Self {
        Self {
            gain: 1.0,
            offset_mv: 0.0,
        }
    }
}

impl Default for AdcCalibrationChannelConfig {
    fn default() -> Self {
        Self {
            samples: [None; ADC_CALIBRATION_MAX_SAMPLES],
            slots: AdcCalibrationSlots::default(),
            active_slot: AdcCalibrationSlotId::A,
        }
    }
}

impl Default for HeaterCurveConfig {
    fn default() -> Self {
        Self {
            points: [None; HEATER_CURVE_MAX_POINTS],
        }
    }
}

impl Default for HeaterCurveRawObservations {
    fn default() -> Self {
        Self {
            points: [None; HEATER_CURVE_MAX_POINTS],
        }
    }
}

impl Default for ThermalControlProfileConfig {
    fn default() -> Self {
        Self {
            settings: ThermalControlProfileSettingsConfig::default(),
            points: [None; THERMAL_CONTROL_PROFILE_MAX_POINTS],
        }
    }
}

impl Default for ThermalControlProfileSettingsConfig {
    fn default() -> Self {
        Self {
            temp_filter_alpha_permille: THERMAL_CONTROL_PROFILE_TEMP_FILTER_ALPHA_PERMILLE_DEFAULT,
            warmup_reenter_centi_c: THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
            hold_entry_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
            hold_exit_centi_c: THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
            hold_on_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
            hold_off_centi_c: THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
            overshoot_cutoff_centi_c: THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
            approach_max_ticks: THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_DEFAULT,
            approach_min_power_ratio_permille:
                THERMAL_CONTROL_PROFILE_APPROACH_MIN_POWER_RATIO_PERMILLE_DEFAULT,
            hold_kp_permille_per_c: THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
            hold_ki_permille_per_c_tick:
                THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
            hold_blend_ticks: THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
            hold_reheat_power_permille: THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT,
            approach_lead_ticks: THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT,
            hold_lead_ticks: THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT,
            auto_adjustable_working_floor_mv:
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT,
            heater_current_reserve_ma: THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_DEFAULT,
        }
    }
}

impl AdcCalibrationConfig {
    pub fn channel(&self, channel: AdcCalibrationChannel) -> &AdcCalibrationChannelConfig {
        match channel {
            AdcCalibrationChannel::Rtd => &self.rtd,
            AdcCalibrationChannel::Vin => &self.vin,
        }
    }

    pub fn channel_mut(
        &mut self,
        channel: AdcCalibrationChannel,
    ) -> &mut AdcCalibrationChannelConfig {
        match channel {
            AdcCalibrationChannel::Rtd => &mut self.rtd,
            AdcCalibrationChannel::Vin => &mut self.vin,
        }
    }
}

impl AdcCalibrationChannelConfig {
    pub fn sample_count(&self) -> usize {
        self.samples.iter().flatten().count()
    }

    pub fn insert(&mut self, sample: AdcCalibrationSample) -> Option<usize> {
        for (index, slot) in self.samples.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(sample);
                return Some(index);
            }
        }
        None
    }

    pub fn delete(&mut self, index: usize) -> bool {
        let Some(slot) = self.samples.get_mut(index) else {
            return false;
        };
        let existed = slot.is_some();
        *slot = None;
        existed
    }

    pub fn clear(&mut self) {
        self.samples = [None; ADC_CALIBRATION_MAX_SAMPLES];
    }

    pub const fn slot_fit(&self, slot: AdcCalibrationSlotId) -> AdcCalibrationSlotFit {
        match slot {
            AdcCalibrationSlotId::A => self.slots.a,
            AdcCalibrationSlotId::B => self.slots.b,
        }
    }

    pub fn slot_fit_mut(&mut self, slot: AdcCalibrationSlotId) -> &mut AdcCalibrationSlotFit {
        match slot {
            AdcCalibrationSlotId::A => &mut self.slots.a,
            AdcCalibrationSlotId::B => &mut self.slots.b,
        }
    }

    pub const fn active_slot_fit(&self) -> AdcCalibrationSlotFit {
        self.slot_fit(self.active_slot)
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            target_temp_c: 100,
            selected_preset_slot: 1,
            presets_c: [
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
            ],
            active_cooling_enabled: true,
            wifi_ssid: String::new(),
            wifi_password: String::new(),
            wifi_auto_reconnect: true,
            wifi_static_ipv4: None,
            lan_pairing_token: None,
            telemetry_interval_ms: 500,
            adc_calibration: AdcCalibrationConfig::default(),
            active_heater_curve: HeaterCurveConfig::default(),
            heater_curve_raw_observations: HeaterCurveRawObservations::default(),
            thermal_plant_active: None,
            thermal_plant_transient_active: None,
            active_thermal_control_profile: ThermalControlProfileConfig::default(),
            thermal_control_profile_pps5a: ThermalControlProfileConfig::default(),
            thermal_profile_mode: ThermalProfileMode::W65,
        }
    }
}

impl MemoryConfig {
    pub fn sanitize(&mut self) {
        self.target_temp_c = clamp_temp_c(self.target_temp_c);
        // Automatic WiFi recovery is a device safety policy, not user configuration.
        self.wifi_auto_reconnect = true;
        if self.selected_preset_slot >= FRONTPANEL_PRESET_COUNT {
            self.selected_preset_slot = MemoryConfig::default().selected_preset_slot;
        }
        for temp in self.presets_c.iter_mut().flatten() {
            *temp = clamp_temp_c(*temp);
        }
        if self.telemetry_interval_ms == 0 {
            self.telemetry_interval_ms = MemoryConfig::default().telemetry_interval_ms;
        }
        self.wifi_static_ipv4 = self.wifi_static_ipv4.filter(|value| value.is_valid());
        sanitize_adc_calibration(&mut self.adc_calibration);
        sanitize_heater_curve(&mut self.active_heater_curve);
        sanitize_heater_curve_raw_observations(&mut self.heater_curve_raw_observations);
        self.thermal_plant_active = self
            .thermal_plant_active
            .filter(thermal_plant_raw_transaction_is_complete);
        self.thermal_plant_transient_active = self
            .thermal_plant_transient_active
            .filter(thermal_plant_transient_transaction_is_complete);
        sanitize_thermal_control_profile(&mut self.active_thermal_control_profile);
        sanitize_thermal_control_profile(&mut self.thermal_control_profile_pps5a);
    }

    pub const fn thermal_profile(&self, bank: ThermalProfileBank) -> &ThermalControlProfileConfig {
        match bank {
            ThermalProfileBank::Pps3a => &self.active_thermal_control_profile,
            ThermalProfileBank::Pps5a => &self.thermal_control_profile_pps5a,
        }
    }

    pub fn thermal_profile_mut(
        &mut self,
        bank: ThermalProfileBank,
    ) -> &mut ThermalControlProfileConfig {
        match bank {
            ThermalProfileBank::Pps3a => &mut self.active_thermal_control_profile,
            ThermalProfileBank::Pps5a => &mut self.thermal_control_profile_pps5a,
        }
    }
}

fn sanitize_heater_curve_raw_observations(config: &mut HeaterCurveRawObservations) {
    let mut compacted = [None; HEATER_CURVE_MAX_POINTS];
    let mut count = 0;
    for point in config.points.iter().flatten().copied() {
        if point.raw_rtd_adc_mv == 0
            || point.heater_voltage_mv == 0
            || point.heater_current_ma == 0
            || point.resistance_milliohms == 0
        {
            continue;
        }
        compacted[count] = Some(point);
        count += 1;
    }
    for index in 1..count {
        let point = compacted[index];
        let mut cursor = index;
        while cursor > 0
            && compacted[cursor - 1].map(|value| value.raw_rtd_adc_mv)
                > point.map(|value| value.raw_rtd_adc_mv)
        {
            compacted[cursor] = compacted[cursor - 1];
            cursor -= 1;
        }
        compacted[cursor] = point;
    }
    config.points = compacted;
}

pub fn thermal_plant_raw_transaction_is_complete(value: &ThermalPlantRawTransaction) -> bool {
    value.transaction_id != 0
        && value.anchors.iter().all(|anchor| {
            anchor.ambient_raw_rtd_adc_mv > 0
                && anchor.target_raw_rtd_adc_mv > anchor.ambient_raw_rtd_adc_mv
                && anchor.heater_voltage_mv > 0
                && anchor.heater_current_ma > 0
                && anchor.steady_hold_power_mw > anchor.gate_off_idle_power_mw
                && anchor.ramp_duration_ms > 0
                && anchor.ramp_energy_mj > 0
        })
}

pub fn thermal_plant_transient_transaction_is_complete(
    value: &ThermalPlantTransientTransaction,
) -> bool {
    if value.transaction_id == 0
        || value.ambient_raw_rtd_adc_mv == 0
        || !(24..=THERMAL_PLANT_TRANSIENT_MAX_SAMPLES as u8).contains(&value.sample_count)
    {
        return false;
    }
    let projection = value.projection.projection();
    if !projection.convection_mw_per_c.is_finite()
        || !projection.radiation_mw_per_k4.is_finite()
        || !projection.thermal_capacity_mj_per_c.is_finite()
        || projection.convection_mw_per_c < 0.0
        || projection.radiation_mw_per_k4 < 0.0
        || !(100.0..=2_000_000.0).contains(&projection.thermal_capacity_mj_per_c)
        || projection.transport_delay_ms > 10_000
    {
        return false;
    }
    if projection.convection_mw_per_c == 0.0 && projection.radiation_mw_per_k4 == 0.0 {
        return false;
    }

    let samples = &value.samples[..usize::from(value.sample_count)];
    let mut has_powered_sample = false;
    let mut has_cooling_sample = false;
    let mut cooling_started = false;
    let mut peak_raw_rtd_adc_mv = 0u16;
    for (index, sample) in samples.iter().enumerate() {
        if sample.raw_rtd_adc_mv == 0 || sample.duty_percent > 100 {
            return false;
        }
        if index > 0 && sample.elapsed_ticks <= samples[index - 1].elapsed_ticks {
            return false;
        }
        if sample.duty_percent == 0 {
            if has_powered_sample {
                cooling_started = true;
                has_cooling_sample |= sample.raw_rtd_adc_mv < peak_raw_rtd_adc_mv;
            }
        } else if sample.heater_voltage_100mv > 0 && !cooling_started {
            has_powered_sample = true;
            peak_raw_rtd_adc_mv = peak_raw_rtd_adc_mv.max(sample.raw_rtd_adc_mv);
        } else {
            return false;
        }
    }
    has_powered_sample
        && cooling_started
        && has_cooling_sample
        && peak_raw_rtd_adc_mv > value.ambient_raw_rtd_adc_mv
}

pub fn thermal_plant_projection_from_transient(
    value: &ThermalPlantTransientTransaction,
) -> Option<ThermalPlantProjection> {
    thermal_plant_transient_transaction_is_complete(value).then(|| value.projection.projection())
}

pub fn project_thermal_plant(
    value: &ThermalPlantRawTransaction,
    mut temperature_from_raw_adc: impl FnMut(u16) -> Option<f32>,
) -> Option<ThermalPlantProjection> {
    if !thermal_plant_raw_transaction_is_complete(value) {
        return None;
    }
    let mut delta_c = [0.0_f32; THERMAL_PLANT_ANCHOR_COUNT];
    let mut radiation = [0.0_f32; THERMAL_PLANT_ANCHOR_COUNT];
    let mut loss_mw = [0.0_f32; THERMAL_PLANT_ANCHOR_COUNT];
    let mut capacity_sum = 0.0_f32;
    let mut delay_sum = 0_u64;
    for (index, anchor) in value.anchors.iter().enumerate() {
        let ambient_c = temperature_from_raw_adc(anchor.ambient_raw_rtd_adc_mv)?;
        let target_c = temperature_from_raw_adc(anchor.target_raw_rtd_adc_mv)?;
        let span_c = target_c - ambient_c;
        if !(5.0..=350.0).contains(&span_c) {
            return None;
        }
        delta_c[index] = span_c;
        let target_k2 = (target_c + 273.15) * (target_c + 273.15);
        let ambient_k2 = (ambient_c + 273.15) * (ambient_c + 273.15);
        radiation[index] = target_k2 * target_k2 - ambient_k2 * ambient_k2;
        loss_mw[index] = (anchor.steady_hold_power_mw - anchor.gate_off_idle_power_mw) as f32;
        capacity_sum += anchor.ramp_energy_mj as f32 / span_c;
        delay_sum += u64::from(anchor.ramp_duration_ms);
    }
    let determinant = delta_c[0] * radiation[1] - delta_c[1] * radiation[0];
    if !determinant.is_finite() || determinant.abs() < 1.0 {
        return None;
    }
    let mut convection = (loss_mw[0] * radiation[1] - loss_mw[1] * radiation[0]) / determinant;
    let mut radiation_coefficient =
        (delta_c[0] * loss_mw[1] - delta_c[1] * loss_mw[0]) / determinant;
    if convection < 0.0 || radiation_coefficient < 0.0 {
        let convection_only = (delta_c[0] * loss_mw[0] + delta_c[1] * loss_mw[1])
            / (delta_c[0] * delta_c[0] + delta_c[1] * delta_c[1]);
        let radiation_only = (radiation[0] * loss_mw[0] + radiation[1] * loss_mw[1])
            / (radiation[0] * radiation[0] + radiation[1] * radiation[1]);
        let convection_residual_0 = loss_mw[0] - convection_only * delta_c[0];
        let convection_residual_1 = loss_mw[1] - convection_only * delta_c[1];
        let radiation_residual_0 = loss_mw[0] - radiation_only * radiation[0];
        let radiation_residual_1 = loss_mw[1] - radiation_only * radiation[1];
        let convection_error = convection_residual_0 * convection_residual_0
            + convection_residual_1 * convection_residual_1;
        let radiation_error = radiation_residual_0 * radiation_residual_0
            + radiation_residual_1 * radiation_residual_1;
        if convection_error <= radiation_error {
            convection = convection_only.max(0.0);
            radiation_coefficient = 0.0;
        } else {
            convection = 0.0;
            radiation_coefficient = radiation_only.max(0.0);
        }
    }
    let capacity = capacity_sum / THERMAL_PLANT_ANCHOR_COUNT as f32;
    if !convection.is_finite()
        || !radiation_coefficient.is_finite()
        || !capacity.is_finite()
        || convection < 0.0
        || radiation_coefficient < 0.0
        || !(100.0..=2_000_000.0).contains(&capacity)
    {
        return None;
    }
    Some(ThermalPlantProjection {
        convection_mw_per_c: convection,
        radiation_mw_per_k4: radiation_coefficient,
        thermal_capacity_mj_per_c: capacity,
        transport_delay_ms: (delay_sum / THERMAL_PLANT_ANCHOR_COUNT as u64).clamp(50, 10_000)
            as u32,
    })
}

pub fn heater_resistance_ohms_from_curve(
    curve: &HeaterCurveConfig,
    current_temp_c: f32,
) -> Option<f32> {
    let points = compacted_heater_points(curve);
    let first = points.first().copied()?;
    if points.len() == 1 {
        return Some(milliohms_to_ohms(first.resistance_milliohms));
    }

    let temp_centi_c = (current_temp_c * 100.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    if temp_centi_c <= first.temp_centi_c {
        return Some(milliohms_to_ohms(first.resistance_milliohms));
    }

    for pair in points.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if temp_centi_c <= right.temp_centi_c {
            let span = (right.temp_centi_c - left.temp_centi_c) as f32;
            if span <= 0.0 {
                return Some(milliohms_to_ohms(left.resistance_milliohms));
            }
            let t = (temp_centi_c - left.temp_centi_c) as f32 / span;
            let left_r = milliohms_to_ohms(left.resistance_milliohms);
            let right_r = milliohms_to_ohms(right.resistance_milliohms);
            return Some(left_r + ((right_r - left_r) * t));
        }
    }

    points
        .last()
        .map(|point| milliohms_to_ohms(point.resistance_milliohms))
}

fn milliohms_to_ohms(value: u16) -> f32 {
    value as f32 / 1_000.0
}

fn compacted_heater_points(
    curve: &HeaterCurveConfig,
) -> heapless::Vec<HeaterCurvePoint, HEATER_CURVE_MAX_POINTS> {
    let mut points = heapless::Vec::new();
    for point in curve.points.iter().flatten() {
        let _ = points.push(*point);
    }
    points.sort_unstable_by_key(|point| point.temp_centi_c);
    points
}

pub fn adc_calibration_fit(
    calibration: &AdcCalibrationConfig,
    channel: AdcCalibrationChannel,
) -> AdcCalibrationFit {
    fit_channel(calibration.channel(channel))
}

pub fn adc_calibration_active_slot_fit(
    calibration: &AdcCalibrationConfig,
    channel: AdcCalibrationChannel,
) -> AdcCalibrationSlotFit {
    calibration.channel(channel).active_slot_fit()
}

pub fn correct_adc_mv(
    calibration: &AdcCalibrationConfig,
    channel: AdcCalibrationChannel,
    observed_mv: u16,
) -> u16 {
    let fit = adc_calibration_active_slot_fit(calibration, channel);
    let corrected = (fit.gain * observed_mv as f32) + fit.offset_mv;
    let rounded = if corrected >= 0.0 {
        corrected + 0.5
    } else {
        corrected - 0.5
    };
    rounded.clamp(0.0, u16::MAX as f32) as u16
}

fn sanitize_adc_calibration(config: &mut AdcCalibrationConfig) {
    compact_channel(&mut config.rtd);
    compact_channel(&mut config.vin);
    sanitize_channel_slot_fit(&mut config.rtd.slots.a);
    sanitize_channel_slot_fit(&mut config.rtd.slots.b);
    sanitize_channel_slot_fit(&mut config.vin.slots.a);
    sanitize_channel_slot_fit(&mut config.vin.slots.b);
}

fn sanitize_heater_curve(config: &mut HeaterCurveConfig) {
    let points = compacted_heater_points(config);
    config.points = [None; HEATER_CURVE_MAX_POINTS];
    for (index, point) in points.into_iter().enumerate() {
        config.points[index] = Some(point);
    }
}

fn sanitize_thermal_control_profile(config: &mut ThermalControlProfileConfig) {
    config.settings.temp_filter_alpha_permille =
        config.settings.temp_filter_alpha_permille.clamp(1, 1_000);
    config.settings.warmup_reenter_centi_c =
        config.settings.warmup_reenter_centi_c.clamp(50, 5_000);
    config.settings.hold_entry_centi_c = config.settings.hold_entry_centi_c.clamp(1, 5_000);
    config.settings.hold_exit_centi_c = config.settings.hold_exit_centi_c.clamp(1, 5_000);
    config.settings.hold_on_centi_c = config.settings.hold_on_centi_c.clamp(1, 5_000);
    config.settings.hold_off_centi_c = config.settings.hold_off_centi_c.clamp(0, 5_000);
    config.settings.overshoot_cutoff_centi_c =
        config.settings.overshoot_cutoff_centi_c.clamp(1, 5_000);
    config.settings.approach_max_ticks = config
        .settings
        .approach_max_ticks
        .clamp(1, THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
    config.settings.approach_min_power_ratio_permille =
        config.settings.approach_min_power_ratio_permille.min(1_000);
    config.settings.hold_kp_permille_per_c = config.settings.hold_kp_permille_per_c.min(10_000);
    config.settings.hold_ki_permille_per_c_tick =
        config.settings.hold_ki_permille_per_c_tick.min(10_000);
    config.settings.hold_blend_ticks = config
        .settings
        .hold_blend_ticks
        .clamp(1, THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
    config.settings.hold_reheat_power_permille =
        config.settings.hold_reheat_power_permille.min(1_000);
    config.settings.approach_lead_ticks = config
        .settings
        .approach_lead_ticks
        .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
    config.settings.hold_lead_ticks = config
        .settings
        .hold_lead_ticks
        .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
    config.settings.auto_adjustable_working_floor_mv =
        config.settings.auto_adjustable_working_floor_mv.clamp(
            THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
            THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
        );
    config.settings.heater_current_reserve_ma = config
        .settings
        .heater_current_reserve_ma
        .min(THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX);

    let mut compacted = [None; THERMAL_CONTROL_PROFILE_MAX_POINTS];
    let mut points: heapless::Vec<
        ThermalControlProfilePointConfig,
        THERMAL_CONTROL_PROFILE_MAX_POINTS,
    > = heapless::Vec::new();
    for point in config.points.iter().flatten() {
        let sanitized = ThermalControlProfilePointConfig {
            target_temp_c: clamp_temp_c(point.target_temp_c),
            brake_distance_centi_c: point.brake_distance_centi_c.clamp(100, 5_000),
            warmup_power_permille: point.warmup_power_permille.min(1_000),
            warmup_reenter_centi_c: if point.warmup_reenter_centi_c == 0 {
                config.settings.warmup_reenter_centi_c
            } else {
                point.warmup_reenter_centi_c.clamp(50, 5_000)
            },
            approach_power_permille: point.approach_power_permille.min(1_000),
            approach_floor_power_permille: point.approach_floor_power_permille.min(1_000),
            approach_damping_exponent_permille: if point.approach_damping_exponent_permille == 0 {
                THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT
            } else {
                point.approach_damping_exponent_permille.clamp(
                    100,
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
                )
            },
            approach_tail_window_centi_c: point
                .approach_tail_window_centi_c
                .min(THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX),
            hold_power_permille: point.hold_power_permille.min(1_000),
            hold_reheat_power_permille: if point.hold_reheat_power_permille == 0 {
                config.settings.hold_reheat_power_permille
            } else {
                point.hold_reheat_power_permille.min(1_000)
            },
            hold_entry_centi_c: if point.hold_entry_centi_c == 0 {
                config.settings.hold_entry_centi_c
            } else {
                point.hold_entry_centi_c.min(5_000)
            },
            hold_exit_centi_c: if point.hold_exit_centi_c == 0 {
                config.settings.hold_exit_centi_c
            } else {
                point.hold_exit_centi_c.min(5_000)
            },
            hold_on_centi_c: if point.hold_on_centi_c == 0 {
                config.settings.hold_on_centi_c
            } else {
                point.hold_on_centi_c.min(5_000)
            },
            hold_off_centi_c: if point.hold_off_centi_c == 0 {
                config.settings.hold_off_centi_c
            } else {
                point.hold_off_centi_c.min(5_000)
            },
            overshoot_cutoff_centi_c: if point.overshoot_cutoff_centi_c == 0 {
                config.settings.overshoot_cutoff_centi_c
            } else {
                point.overshoot_cutoff_centi_c.min(5_000)
            },
            hold_kp_permille_per_c: if point.hold_kp_permille_per_c == 0 {
                config.settings.hold_kp_permille_per_c
            } else {
                point.hold_kp_permille_per_c.min(10_000)
            },
            hold_ki_permille_per_c_tick: if point.hold_ki_permille_per_c_tick == 0 {
                config.settings.hold_ki_permille_per_c_tick
            } else {
                point.hold_ki_permille_per_c_tick.min(10_000)
            },
            hold_blend_ticks: if point.hold_blend_ticks == 0 {
                config.settings.hold_blend_ticks
            } else {
                point
                    .hold_blend_ticks
                    .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
            },
            approach_lead_ticks: if point.approach_lead_ticks == 0 {
                config.settings.approach_lead_ticks
            } else {
                point
                    .approach_lead_ticks
                    .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
            },
            hold_lead_ticks: if point.hold_lead_ticks == 0 {
                config.settings.hold_lead_ticks
            } else {
                point
                    .hold_lead_ticks
                    .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
            },
        };
        let _ = points.push(sanitized);
    }
    points.sort_unstable_by_key(|point| point.target_temp_c);
    for (index, point) in points
        .into_iter()
        .take(THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS)
        .enumerate()
    {
        compacted[index] = Some(point);
    }
    config.points = compacted;
}

fn compact_channel(channel: &mut AdcCalibrationChannelConfig) {
    let mut compacted = [None; ADC_CALIBRATION_MAX_SAMPLES];
    let mut cursor = 0;
    for sample in channel.samples.iter().flatten() {
        if cursor < compacted.len() {
            compacted[cursor] = Some(*sample);
            cursor += 1;
        }
    }
    channel.samples = compacted;
}

fn legacy_default_points(channel: AdcCalibrationChannel) -> [AdcCalibrationSample; 2] {
    match channel {
        AdcCalibrationChannel::Rtd => [
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_RTD_DEFAULT_LOW_MV,
                expected_mv: ADC_CALIBRATION_RTD_DEFAULT_LOW_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV,
                expected_mv: ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
        ],
        AdcCalibrationChannel::Vin => [
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_VIN_DEFAULT_LOW_MV,
                expected_mv: ADC_CALIBRATION_VIN_DEFAULT_LOW_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV,
                expected_mv: ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
        ],
    }
}

fn fit_channel(channel: &AdcCalibrationChannelConfig) -> AdcCalibrationFit {
    let sample_count = channel.sample_count();
    if sample_count == 0 {
        return AdcCalibrationFit {
            gain: 1.0,
            offset_mv: 0.0,
            sample_count,
        };
    }

    if sample_count == 1 {
        let sample = channel.samples.iter().flatten().next().copied().unwrap();
        return AdcCalibrationFit {
            gain: 1.0,
            offset_mv: sample.expected_mv as f32 - sample.observed_mv as f32,
            sample_count,
        };
    }

    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    let mut sum_xx = 0.0_f32;
    let mut sum_xy = 0.0_f32;

    for sample in channel.samples.iter().flatten() {
        accumulate_fit_point(*sample, &mut sum_x, &mut sum_y, &mut sum_xx, &mut sum_xy);
    }

    let n = sample_count as f32;
    let denominator = (n * sum_xx) - (sum_x * sum_x);
    if denominator.abs() < f32::EPSILON {
        let offset_mv = (sum_y - sum_x) / n;
        return AdcCalibrationFit {
            gain: 1.0,
            offset_mv,
            sample_count,
        };
    }

    let gain = ((n * sum_xy) - (sum_x * sum_y)) / denominator;
    let offset_mv = (sum_y - (gain * sum_x)) / n;
    AdcCalibrationFit {
        gain,
        offset_mv,
        sample_count,
    }
}

fn legacy_fit_channel(
    channel: &AdcCalibrationChannelConfig,
    defaults: [AdcCalibrationSample; 2],
) -> AdcCalibrationSlotFit {
    let custom_count = channel.sample_count();
    let default_count = if custom_count < 2 { 2 } else { 0 };
    let total_count = custom_count + default_count;
    if total_count < 2 {
        return AdcCalibrationSlotFit::default();
    }

    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    let mut sum_xx = 0.0_f32;
    let mut sum_xy = 0.0_f32;

    if custom_count < 2 {
        for sample in defaults {
            accumulate_fit_point(sample, &mut sum_x, &mut sum_y, &mut sum_xx, &mut sum_xy);
        }
    }
    for sample in channel.samples.iter().flatten() {
        accumulate_fit_point(*sample, &mut sum_x, &mut sum_y, &mut sum_xx, &mut sum_xy);
    }

    let n = total_count as f32;
    let denominator = (n * sum_xx) - (sum_x * sum_x);
    if denominator.abs() < f32::EPSILON {
        return AdcCalibrationSlotFit {
            gain: 1.0,
            offset_mv: (sum_y - sum_x) / n,
        };
    }

    let gain = ((n * sum_xy) - (sum_x * sum_y)) / denominator;
    let offset_mv = (sum_y - (gain * sum_x)) / n;
    AdcCalibrationSlotFit { gain, offset_mv }
}

fn sanitize_channel_slot_fit(fit: &mut AdcCalibrationSlotFit) {
    if !fit.gain.is_finite() || fit.gain == 0.0 {
        fit.gain = 1.0;
    }
    if !fit.offset_mv.is_finite() {
        fit.offset_mv = 0.0;
    }
}

fn accumulate_fit_point(
    sample: AdcCalibrationSample,
    sum_x: &mut f32,
    sum_y: &mut f32,
    sum_xx: &mut f32,
    sum_xy: &mut f32,
) {
    let x = sample.observed_mv as f32;
    let y = sample.expected_mv as f32;
    *sum_x += x;
    *sum_y += y;
    *sum_xx += x * x;
    *sum_xy += x * y;
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRecord {
    pub sequence: u32,
    pub config: MemoryConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryDecodeError {
    TooShort,
    BadMagic,
    UnsupportedFormat(u8),
    BadHeaderLength(u8),
    PayloadOutOfBounds,
    CrcMismatch,
    MalformedTlv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEncodeError {
    BufferTooSmall,
    PayloadTooLarge,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EepromError<I2cError> {
    OutOfRange,
    PageWriteTooLong,
    PageBoundaryCrossed,
    I2c(I2cError),
}

#[cfg(target_arch = "xtensa")]
pub struct M24c64<I2C> {
    i2c: I2C,
    address: u8,
}

#[cfg(target_arch = "xtensa")]
impl<I2C> M24c64<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: M24C64_I2C_ADDRESS,
        }
    }

    pub const fn with_address(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

#[cfg(target_arch = "xtensa")]
impl<I2C> M24c64<I2C>
where
    I2C: embedded_hal::i2c::I2c,
{
    pub fn read_bytes(
        &mut self,
        offset: u16,
        bytes: &mut [u8],
    ) -> Result<(), EepromError<I2C::Error>> {
        if usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES) {
            return Err(EepromError::OutOfRange);
        }
        let address = offset.to_be_bytes();
        self.i2c
            .write_read(self.address, &address, bytes)
            .map_err(EepromError::I2c)
    }

    pub fn read_current_byte(&mut self) -> Result<u8, EepromError<I2C::Error>> {
        let mut byte = [0u8; 1];
        self.i2c
            .read(self.address, &mut byte)
            .map_err(EepromError::I2c)?;
        Ok(byte[0])
    }

    pub fn write_page(&mut self, offset: u16, bytes: &[u8]) -> Result<(), EepromError<I2C::Error>> {
        if bytes.len() > M24C64_PAGE_SIZE {
            return Err(EepromError::PageWriteTooLong);
        }
        if usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES) {
            return Err(EepromError::OutOfRange);
        }
        let page_offset = usize::from(offset) % M24C64_PAGE_SIZE;
        if page_offset + bytes.len() > M24C64_PAGE_SIZE {
            return Err(EepromError::PageBoundaryCrossed);
        }

        let mut payload = [0u8; M24C64_PAGE_SIZE + 2];
        payload[0..2].copy_from_slice(&offset.to_be_bytes());
        payload[2..2 + bytes.len()].copy_from_slice(bytes);
        self.i2c
            .write(self.address, &payload[..2 + bytes.len()])
            .map_err(EepromError::I2c)
    }
}

pub fn encode_memory_record(
    record: &MemoryRecord,
    out: &mut [u8],
) -> Result<usize, MemoryEncodeError> {
    if out.len() < MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryEncodeError::BufferTooSmall);
    }

    let payload_len = encode_config_payload(&record.config, &mut out[MEMORY_RECORD_HEADER_LEN..])?;
    if payload_len > MEMORY_RECORD_PAYLOAD_MAX {
        return Err(MemoryEncodeError::PayloadTooLarge);
    }

    out[0..4].copy_from_slice(&MEMORY_RECORD_MAGIC);
    out[4] = MEMORY_RECORD_FORMAT_VERSION;
    out[5] = MEMORY_RECORD_HEADER_LEN as u8;
    out[6..8].copy_from_slice(&(payload_len as u16).to_le_bytes());
    out[8..12].copy_from_slice(&record.sequence.to_le_bytes());
    let crc = crc32_update(
        crc32(&out[0..12]),
        &out[MEMORY_RECORD_HEADER_LEN..MEMORY_RECORD_HEADER_LEN + payload_len],
    ) ^ 0xffff_ffff;
    out[12..16].copy_from_slice(&crc.to_le_bytes());

    Ok(MEMORY_RECORD_HEADER_LEN + payload_len)
}

pub fn decode_memory_record(bytes: &[u8]) -> Result<MemoryRecord, MemoryDecodeError> {
    if bytes.len() < MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryDecodeError::TooShort);
    }
    if bytes[0..4] != MEMORY_RECORD_MAGIC {
        return Err(MemoryDecodeError::BadMagic);
    }
    if !matches!(bytes[4], 1 | 2 | 3 | 4 | MEMORY_RECORD_FORMAT_VERSION) {
        return Err(MemoryDecodeError::UnsupportedFormat(bytes[4]));
    }
    if bytes[5] as usize != MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryDecodeError::BadHeaderLength(bytes[5]));
    }

    let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    let payload_end = MEMORY_RECORD_HEADER_LEN
        .checked_add(payload_len)
        .ok_or(MemoryDecodeError::PayloadOutOfBounds)?;
    if payload_len > MEMORY_RECORD_PAYLOAD_MAX || payload_end > bytes.len() {
        return Err(MemoryDecodeError::PayloadOutOfBounds);
    }

    let expected_crc = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let actual_crc = crc32_update(
        crc32(&bytes[0..12]),
        &bytes[MEMORY_RECORD_HEADER_LEN..payload_end],
    ) ^ 0xffff_ffff;
    if expected_crc != actual_crc {
        return Err(MemoryDecodeError::CrcMismatch);
    }

    let sequence = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let mut config =
        decode_config_payload(&bytes[MEMORY_RECORD_HEADER_LEN..payload_end], bytes[4] >= 3)?;
    config.sanitize();

    Ok(MemoryRecord { sequence, config })
}

pub fn select_latest_memory_record(
    left: Result<MemoryRecord, MemoryDecodeError>,
    right: Result<MemoryRecord, MemoryDecodeError>,
) -> Option<MemoryRecord> {
    match (left, right) {
        (Ok(left), Ok(right)) if right.sequence > left.sequence => Some(right),
        (Ok(left), Ok(_)) => Some(left),
        (Ok(left), Err(_)) => Some(left),
        (Err(_), Ok(right)) => Some(right),
        (Err(_), Err(_)) => None,
    }
}

pub fn select_latest_optional_memory_record(
    left: Option<MemoryRecord>,
    right: Option<MemoryRecord>,
) -> Option<MemoryRecord> {
    select_latest_memory_record(
        left.ok_or(MemoryDecodeError::BadMagic),
        right.ok_or(MemoryDecodeError::BadMagic),
    )
}

fn encode_config_payload(
    config: &MemoryConfig,
    out: &mut [u8],
) -> Result<usize, MemoryEncodeError> {
    let mut cursor = 0;
    push_tlv(
        TLV_TARGET_TEMP_C,
        &config.target_temp_c.to_le_bytes(),
        out,
        &mut cursor,
    )?;
    push_tlv(
        TLV_SELECTED_PRESET_SLOT,
        &[config.selected_preset_slot as u8],
        out,
        &mut cursor,
    )?;

    let mut presets = [0u8; FRONTPANEL_PRESET_COUNT * 2];
    for (index, preset) in config.presets_c.iter().enumerate() {
        let wire_value = preset.map(clamp_temp_c).unwrap_or(PRESET_NONE_WIRE_VALUE);
        presets[index * 2..index * 2 + 2].copy_from_slice(&wire_value.to_le_bytes());
    }
    push_tlv(TLV_PRESETS_C, &presets, out, &mut cursor)?;
    push_tlv(
        TLV_ACTIVE_COOLING_ENABLED,
        &[u8::from(config.active_cooling_enabled)],
        out,
        &mut cursor,
    )?;
    push_tlv(TLV_WIFI_SSID, config.wifi_ssid.as_bytes(), out, &mut cursor)?;
    push_tlv(
        TLV_WIFI_PASSWORD,
        config.wifi_password.as_bytes(),
        out,
        &mut cursor,
    )?;
    push_tlv(
        TLV_WIFI_AUTO_RECONNECT,
        &[u8::from(config.wifi_auto_reconnect)],
        out,
        &mut cursor,
    )?;
    if let Some(token) = config.lan_pairing_token {
        push_tlv(TLV_LAN_PAIRING_TOKEN, &token, out, &mut cursor)?;
    }
    if let Some(static_ipv4) = config.wifi_static_ipv4 {
        let mut bytes = [0u8; 13];
        bytes[..4].copy_from_slice(&static_ipv4.address);
        bytes[4] = static_ipv4.prefix_len;
        bytes[5..9].copy_from_slice(&static_ipv4.gateway);
        bytes[9..13].copy_from_slice(&static_ipv4.dns);
        push_tlv(TLV_WIFI_STATIC_IPV4, &bytes, out, &mut cursor)?;
    }
    push_tlv(
        TLV_TELEMETRY_INTERVAL_MS,
        &config.telemetry_interval_ms.to_le_bytes(),
        out,
        &mut cursor,
    )?;
    let mut calibration_payload = [0u8; ADC_CALIBRATION_SAMPLE_PAYLOAD_LEN];
    encode_adc_calibration_samples(&config.adc_calibration, &mut calibration_payload);
    push_tlv(
        TLV_ADC_CALIBRATION_SAMPLES,
        &calibration_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_reference_payload = [0u8; ADC_CALIBRATION_REFERENCE_PAYLOAD_LEN];
    encode_adc_calibration_references(&config.adc_calibration, &mut calibration_reference_payload);
    push_tlv(
        TLV_ADC_CALIBRATION_REFERENCES,
        &calibration_reference_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_target_payload = [0u8; ADC_CALIBRATION_TARGET_PAYLOAD_LEN];
    encode_adc_calibration_targets(&config.adc_calibration, &mut calibration_target_payload);
    push_tlv(
        TLV_ADC_CALIBRATION_TARGETS,
        &calibration_target_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_slot_payload = [0u8; ADC_CALIBRATION_SLOT_PAYLOAD_LEN];
    encode_adc_calibration_slots(&config.adc_calibration, &mut calibration_slot_payload);
    push_tlv(
        TLV_ADC_CALIBRATION_SLOTS,
        &calibration_slot_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_active_slot_payload = [0u8; ADC_CALIBRATION_ACTIVE_SLOT_PAYLOAD_LEN];
    encode_adc_calibration_active_slots(
        &config.adc_calibration,
        &mut calibration_active_slot_payload,
    );
    push_tlv(
        TLV_ADC_CALIBRATION_ACTIVE_SLOTS,
        &calibration_active_slot_payload,
        out,
        &mut cursor,
    )?;
    let mut heater_curve_payload = [0u8; HEATER_CURVE_MAX_POINTS * 4];
    encode_heater_curve(&config.active_heater_curve, &mut heater_curve_payload);
    push_tlv(
        TLV_ACTIVE_HEATER_CURVE,
        &heater_curve_payload,
        out,
        &mut cursor,
    )?;
    let has_new_thermal_data = config
        .heater_curve_raw_observations
        .points
        .iter()
        .any(Option::is_some)
        || config.thermal_plant_transient_active.is_some();
    if !has_new_thermal_data {
        let mut thermal_profile_payload = [0u8; THERMAL_CONTROL_PROFILE_PAYLOAD_LEN];
        let thermal_profile_len = encode_thermal_control_profile(
            &config.active_thermal_control_profile,
            &mut thermal_profile_payload,
        );
        push_tlv(
            TLV_THERMAL_CONTROL_PROFILE_PPS3A,
            &thermal_profile_payload[..thermal_profile_len],
            out,
            &mut cursor,
        )?;
        let pps5a_profile_len = encode_thermal_control_profile(
            &config.thermal_control_profile_pps5a,
            &mut thermal_profile_payload,
        );
        push_tlv(
            TLV_THERMAL_CONTROL_PROFILE_PPS5A,
            &thermal_profile_payload[..pps5a_profile_len],
            out,
            &mut cursor,
        )?;
        let mode = match config.thermal_profile_mode {
            ThermalProfileMode::Auto => 0,
            ThermalProfileMode::W65 => 1,
            ThermalProfileMode::W100 => 2,
        };
        push_tlv(TLV_THERMAL_PROFILE_MODE, &[mode], out, &mut cursor)?;
    }
    let mut raw_curve_payload = [0u8; HEATER_CURVE_MAX_POINTS * 8];
    encode_heater_curve_raw_observations(
        &config.heater_curve_raw_observations,
        &mut raw_curve_payload,
    );
    push_tlv(
        TLV_HEATER_CURVE_RAW_OBSERVATIONS,
        &raw_curve_payload,
        out,
        &mut cursor,
    )?;
    if let Some(active) = config.thermal_plant_transient_active {
        let payload_len = THERMAL_PLANT_TRANSIENT_HEADER_LEN
            + usize::from(active.sample_count) * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN;
        let mut payload = [0u8; THERMAL_PLANT_TRANSIENT_HEADER_LEN
            + THERMAL_PLANT_TRANSIENT_MAX_SAMPLES * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN];
        encode_thermal_plant_transient_transaction(&active, &mut payload[..payload_len]);
        push_tlv(
            TLV_THERMAL_PLANT_TRANSIENT_ACTIVE,
            &payload[..payload_len],
            out,
            &mut cursor,
        )?;
    }
    Ok(cursor)
}

fn decode_config_payload(
    bytes: &[u8],
    wide_tlv_lengths: bool,
) -> Result<MemoryConfig, MemoryDecodeError> {
    let mut config = MemoryConfig::default();
    let mut legacy_active_adc_calibration = AdcCalibrationConfig::default();
    let mut legacy_draft_adc_calibration = AdcCalibrationConfig::default();
    let mut saw_new_adc_slots = false;
    let mut saw_new_adc_active_slots = false;
    let mut saw_legacy_active = false;
    let mut saw_legacy_draft = false;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let header_len = if wide_tlv_lengths { 3 } else { 2 };
        if bytes.len().saturating_sub(cursor) < header_len {
            return Err(MemoryDecodeError::MalformedTlv);
        }
        let tag = bytes[cursor];
        let len = if wide_tlv_lengths {
            u16::from_le_bytes([bytes[cursor + 1], bytes[cursor + 2]]) as usize
        } else {
            bytes[cursor + 1] as usize
        };
        cursor += header_len;
        if bytes.len().saturating_sub(cursor) < len {
            return Err(MemoryDecodeError::MalformedTlv);
        }
        let value = &bytes[cursor..cursor + len];
        cursor += len;

        match tag {
            TLV_TARGET_TEMP_C if len == 2 => {
                config.target_temp_c = i16::from_le_bytes([value[0], value[1]]);
            }
            TLV_SELECTED_PRESET_SLOT if len == 1 => {
                config.selected_preset_slot = value[0] as usize;
            }
            TLV_PRESETS_C if len == FRONTPANEL_PRESET_COUNT * 2 => {
                for index in 0..FRONTPANEL_PRESET_COUNT {
                    let wire_value = i16::from_le_bytes([value[index * 2], value[index * 2 + 1]]);
                    config.presets_c[index] = if wire_value == PRESET_NONE_WIRE_VALUE {
                        None
                    } else {
                        Some(wire_value)
                    };
                }
            }
            TLV_ACTIVE_COOLING_ENABLED if len == 1 => {
                config.active_cooling_enabled = value[0] != 0;
            }
            TLV_WIFI_SSID => {
                config.wifi_ssid.clear();
                let copy_len = value.len().min(MEMORY_WIFI_SSID_MAX_LEN);
                let _ = config
                    .wifi_ssid
                    .push_str(core::str::from_utf8(&value[..copy_len]).unwrap_or(""));
            }
            TLV_WIFI_PASSWORD => {
                config.wifi_password.clear();
                let copy_len = value.len().min(MEMORY_WIFI_PASSWORD_MAX_LEN);
                let _ = config
                    .wifi_password
                    .push_str(core::str::from_utf8(&value[..copy_len]).unwrap_or(""));
            }
            TLV_WIFI_AUTO_RECONNECT if len == 1 => {
                config.wifi_auto_reconnect = value[0] != 0;
            }
            TLV_LAN_PAIRING_TOKEN if len == crate::lan::LAN_TOKEN_BYTES => {
                let mut token = [0u8; crate::lan::LAN_TOKEN_BYTES];
                token.copy_from_slice(value);
                config.lan_pairing_token = Some(token);
            }
            TLV_WIFI_STATIC_IPV4 if len == 13 => {
                let mut address = [0u8; 4];
                let mut gateway = [0u8; 4];
                let mut dns = [0u8; 4];
                address.copy_from_slice(&value[..4]);
                gateway.copy_from_slice(&value[5..9]);
                dns.copy_from_slice(&value[9..13]);
                config.wifi_static_ipv4 = Some(WifiStaticIpv4Config {
                    address,
                    prefix_len: value[4],
                    gateway,
                    dns,
                });
            }
            TLV_TELEMETRY_INTERVAL_MS if len == 4 => {
                config.telemetry_interval_ms =
                    u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            }
            TLV_ADC_CALIBRATION_SAMPLES if len == ADC_CALIBRATION_SAMPLE_PAYLOAD_LEN => {
                let decoded = decode_adc_calibration_samples(value);
                legacy_active_adc_calibration = decoded;
                config.adc_calibration.rtd.samples = decoded.rtd.samples;
                config.adc_calibration.vin.samples = decoded.vin.samples;
                saw_legacy_active = true;
            }
            TLV_LEGACY_DRAFT_ADC_CALIBRATION if len == ADC_CALIBRATION_SAMPLE_PAYLOAD_LEN => {
                legacy_draft_adc_calibration = decode_adc_calibration_samples(value);
                saw_legacy_draft = true;
            }
            TLV_ADC_CALIBRATION_REFERENCES if len == ADC_CALIBRATION_REFERENCE_PAYLOAD_LEN => {
                decode_adc_calibration_references(value, &mut config.adc_calibration);
                decode_adc_calibration_references(value, &mut legacy_active_adc_calibration);
                saw_legacy_active = true;
            }
            TLV_LEGACY_DRAFT_ADC_CALIBRATION_REFERENCES
                if len == ADC_CALIBRATION_REFERENCE_PAYLOAD_LEN =>
            {
                decode_adc_calibration_references(value, &mut legacy_draft_adc_calibration);
                saw_legacy_draft = true;
            }
            TLV_ADC_CALIBRATION_TARGETS if len == ADC_CALIBRATION_TARGET_PAYLOAD_LEN => {
                decode_adc_calibration_targets(value, &mut config.adc_calibration);
                decode_adc_calibration_targets(value, &mut legacy_active_adc_calibration);
                saw_legacy_active = true;
            }
            TLV_LEGACY_DRAFT_ADC_CALIBRATION_TARGETS
                if len == ADC_CALIBRATION_TARGET_PAYLOAD_LEN =>
            {
                decode_adc_calibration_targets(value, &mut legacy_draft_adc_calibration);
                saw_legacy_draft = true;
            }
            TLV_ADC_CALIBRATION_SLOTS if len == ADC_CALIBRATION_SLOT_PAYLOAD_LEN => {
                decode_adc_calibration_slots(value, &mut config.adc_calibration);
                saw_new_adc_slots = true;
            }
            TLV_ADC_CALIBRATION_ACTIVE_SLOTS if len == ADC_CALIBRATION_ACTIVE_SLOT_PAYLOAD_LEN => {
                decode_adc_calibration_active_slots(value, &mut config.adc_calibration);
                saw_new_adc_active_slots = true;
            }
            TLV_ACTIVE_HEATER_CURVE if len == HEATER_CURVE_MAX_POINTS * 4 => {
                config.active_heater_curve = decode_heater_curve(value);
            }
            TLV_ACTIVE_THERMAL_CONTROL_PROFILE if is_supported_thermal_control_profile(value) => {
                config.active_thermal_control_profile = decode_thermal_control_profile(value);
            }
            TLV_THERMAL_CONTROL_PROFILE_PPS3A if is_supported_thermal_control_profile(value) => {
                config.active_thermal_control_profile = decode_thermal_control_profile(value);
            }
            TLV_THERMAL_CONTROL_PROFILE_PPS5A if is_supported_thermal_control_profile(value) => {
                config.thermal_control_profile_pps5a = decode_thermal_control_profile(value);
            }
            TLV_THERMAL_PROFILE_MODE if len == 1 => {
                config.thermal_profile_mode = match value[0] {
                    0 => ThermalProfileMode::Auto,
                    2 => ThermalProfileMode::W100,
                    _ => ThermalProfileMode::W65,
                };
            }
            TLV_HEATER_CURVE_RAW_OBSERVATIONS if len == HEATER_CURVE_MAX_POINTS * 8 => {
                config.heater_curve_raw_observations = decode_heater_curve_raw_observations(value);
            }
            TLV_THERMAL_PLANT_CANDIDATE if len == 52 => {
                // Retain historical records for inspection and lossless decoding only.
                // They are never promoted to the transient active model.
                if config.thermal_plant_active.is_none() {
                    config.thermal_plant_active = decode_thermal_plant_raw_transaction(value);
                }
            }
            TLV_THERMAL_PLANT_ACTIVE if len == 52 => {
                config.thermal_plant_active = decode_thermal_plant_raw_transaction(value);
            }
            TLV_THERMAL_PLANT_TRANSIENT_ACTIVE
                if (THERMAL_PLANT_TRANSIENT_HEADER_LEN
                    ..=THERMAL_PLANT_TRANSIENT_HEADER_LEN
                        + THERMAL_PLANT_TRANSIENT_MAX_SAMPLES
                            * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN)
                    .contains(&len) =>
            {
                config.thermal_plant_transient_active =
                    decode_thermal_plant_transient_transaction(value);
            }
            _ => {}
        }
    }
    if saw_legacy_active && !saw_new_adc_slots && !saw_new_adc_active_slots {
        migrate_legacy_adc_calibration(
            &mut config.adc_calibration,
            &legacy_active_adc_calibration,
            saw_legacy_draft.then_some(&legacy_draft_adc_calibration),
        );
    } else if saw_legacy_active && (!saw_new_adc_slots || !saw_new_adc_active_slots) {
        backfill_new_adc_calibration_defaults(&mut config.adc_calibration);
    }
    Ok(config)
}

fn encode_heater_curve_raw_observations(config: &HeaterCurveRawObservations, out: &mut [u8]) {
    for (index, point) in config.points.iter().enumerate() {
        let offset = index * 8;
        let values = point
            .map(|point| {
                [
                    point.raw_rtd_adc_mv,
                    point.heater_voltage_mv,
                    point.heater_current_ma,
                    point.resistance_milliohms,
                ]
            })
            .unwrap_or([u16::MAX; 4]);
        for (field, value) in values.iter().enumerate() {
            out[offset + field * 2..offset + field * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
}

fn decode_heater_curve_raw_observations(bytes: &[u8]) -> HeaterCurveRawObservations {
    let mut config = HeaterCurveRawObservations::default();
    for index in 0..HEATER_CURVE_MAX_POINTS {
        let offset = index * 8;
        let read = |field: usize| {
            u16::from_le_bytes([bytes[offset + field * 2], bytes[offset + field * 2 + 1]])
        };
        let raw_rtd_adc_mv = read(0);
        if raw_rtd_adc_mv != u16::MAX {
            config.points[index] = Some(HeaterCurveRawObservation {
                raw_rtd_adc_mv,
                heater_voltage_mv: read(1),
                heater_current_ma: read(2),
                resistance_milliohms: read(3),
            });
        }
    }
    config
}

#[cfg(test)]
fn encode_thermal_plant_raw_transaction(value: &ThermalPlantRawTransaction, out: &mut [u8]) {
    out[..4].copy_from_slice(&value.transaction_id.to_le_bytes());
    let mut cursor = 4;
    for anchor in value.anchors {
        for field in [
            anchor.ambient_raw_rtd_adc_mv,
            anchor.target_raw_rtd_adc_mv,
            anchor.heater_voltage_mv,
            anchor.heater_current_ma,
        ] {
            out[cursor..cursor + 2].copy_from_slice(&field.to_le_bytes());
            cursor += 2;
        }
        for field in [
            anchor.gate_off_idle_power_mw,
            anchor.steady_hold_power_mw,
            anchor.ramp_duration_ms,
            anchor.ramp_energy_mj,
        ] {
            out[cursor..cursor + 4].copy_from_slice(&field.to_le_bytes());
            cursor += 4;
        }
    }
}

fn decode_thermal_plant_raw_transaction(bytes: &[u8]) -> Option<ThermalPlantRawTransaction> {
    let transaction_id = u32::from_le_bytes(bytes[..4].try_into().ok()?);
    let mut cursor = 4;
    let mut anchors = [ThermalPlantRawAnchor {
        ambient_raw_rtd_adc_mv: 0,
        target_raw_rtd_adc_mv: 0,
        heater_voltage_mv: 0,
        heater_current_ma: 0,
        gate_off_idle_power_mw: 0,
        steady_hold_power_mw: 0,
        ramp_duration_ms: 0,
        ramp_energy_mj: 0,
    }; THERMAL_PLANT_ANCHOR_COUNT];
    for anchor in &mut anchors {
        let read_u16 = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        anchor.ambient_raw_rtd_adc_mv = read_u16(cursor);
        anchor.target_raw_rtd_adc_mv = read_u16(cursor + 2);
        anchor.heater_voltage_mv = read_u16(cursor + 4);
        anchor.heater_current_ma = read_u16(cursor + 6);
        cursor += 8;
        let read_u32 = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        anchor.gate_off_idle_power_mw = read_u32(cursor);
        anchor.steady_hold_power_mw = read_u32(cursor + 4);
        anchor.ramp_duration_ms = read_u32(cursor + 8);
        anchor.ramp_energy_mj = read_u32(cursor + 12);
        cursor += 16;
    }
    let value = ThermalPlantRawTransaction {
        transaction_id,
        anchors,
    };
    thermal_plant_raw_transaction_is_complete(&value).then_some(value)
}

fn encode_thermal_plant_transient_transaction(
    value: &ThermalPlantTransientTransaction,
    out: &mut [u8],
) {
    debug_assert_eq!(
        out.len(),
        THERMAL_PLANT_TRANSIENT_HEADER_LEN
            + usize::from(value.sample_count) * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN
    );
    out[..4].copy_from_slice(&value.transaction_id.to_le_bytes());
    out[4..6].copy_from_slice(&value.ambient_raw_rtd_adc_mv.to_le_bytes());
    out[6] = value.sample_count;
    out[7] = 0;
    let projection = value.projection;
    out[8..12].copy_from_slice(&projection.convection_mw_per_c_bits.to_le_bytes());
    out[12..16].copy_from_slice(&projection.radiation_mw_per_k4_bits.to_le_bytes());
    out[16..20].copy_from_slice(&projection.thermal_capacity_mj_per_c_bits.to_le_bytes());
    out[20..24].copy_from_slice(&projection.transport_delay_ms.to_le_bytes());
    for (index, sample) in value.samples[..usize::from(value.sample_count)]
        .iter()
        .enumerate()
    {
        let offset =
            THERMAL_PLANT_TRANSIENT_HEADER_LEN + index * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN;
        out[offset..offset + 2].copy_from_slice(&sample.elapsed_ticks.to_le_bytes());
        out[offset + 2..offset + 4].copy_from_slice(&sample.raw_rtd_adc_mv.to_le_bytes());
        out[offset + 4] = sample.heater_voltage_100mv;
        out[offset + 5] = sample.duty_percent;
    }
}

fn decode_thermal_plant_transient_transaction(
    bytes: &[u8],
) -> Option<ThermalPlantTransientTransaction> {
    if bytes.len() < THERMAL_PLANT_TRANSIENT_HEADER_LEN {
        return None;
    }
    let sample_count = bytes[6];
    let expected_len = THERMAL_PLANT_TRANSIENT_HEADER_LEN
        + usize::from(sample_count) * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN;
    if bytes.len() != expected_len
        || usize::from(sample_count) > THERMAL_PLANT_TRANSIENT_MAX_SAMPLES
    {
        return None;
    }
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_100mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, sample) in samples[..usize::from(sample_count)].iter_mut().enumerate() {
        let offset =
            THERMAL_PLANT_TRANSIENT_HEADER_LEN + index * THERMAL_PLANT_TRANSIENT_SAMPLE_PAYLOAD_LEN;
        *sample = ThermalPlantTransientSample {
            elapsed_ticks: u16::from_le_bytes([bytes[offset], bytes[offset + 1]]),
            raw_rtd_adc_mv: u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]),
            heater_voltage_100mv: bytes[offset + 4],
            duty_percent: bytes[offset + 5],
        };
    }
    let value = ThermalPlantTransientTransaction {
        transaction_id: u32::from_le_bytes(bytes[..4].try_into().ok()?),
        ambient_raw_rtd_adc_mv: u16::from_le_bytes([bytes[4], bytes[5]]),
        sample_count,
        projection: ThermalPlantProjectionRecord {
            convection_mw_per_c_bits: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            radiation_mw_per_k4_bits: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
            thermal_capacity_mj_per_c_bits: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
            transport_delay_ms: u32::from_le_bytes(bytes[20..24].try_into().ok()?),
        },
        samples,
    };
    thermal_plant_transient_transaction_is_complete(&value).then_some(value)
}

fn encode_adc_calibration_samples(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for channel in [&config.rtd, &config.vin] {
        for sample in channel.samples {
            let observed = sample
                .map(|sample| sample.observed_mv)
                .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
            let expected = sample
                .map(|sample| sample.expected_mv)
                .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
            out[cursor..cursor + 2].copy_from_slice(&observed.to_le_bytes());
            out[cursor + 2..cursor + 4].copy_from_slice(&expected.to_le_bytes());
            cursor += 4;
        }
    }
}

fn encode_adc_calibration_references(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for sample in config.rtd.samples {
        let reference = sample
            .and_then(|sample| sample.reference_temp_deci_c)
            .unwrap_or(CALIBRATION_REFERENCE_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&reference.to_le_bytes());
        cursor += 2;
    }
    for sample in config.vin.samples {
        let reference = sample
            .and_then(|sample| sample.reference_vin_mv)
            .and_then(|value| i16::try_from(value).ok())
            .unwrap_or(CALIBRATION_REFERENCE_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&reference.to_le_bytes());
        cursor += 2;
    }
}

fn encode_adc_calibration_targets(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for sample in config.rtd.samples {
        let target = sample
            .and_then(|sample| sample.target_adc_mv)
            .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&target.to_le_bytes());
        cursor += 2;
    }
}

fn decode_adc_calibration_samples(bytes: &[u8]) -> AdcCalibrationConfig {
    let mut config = AdcCalibrationConfig::default();
    let mut cursor = 0;
    for channel in [&mut config.rtd, &mut config.vin] {
        for slot in channel.samples.iter_mut() {
            let observed = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            let expected = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
            *slot = if observed == CALIBRATION_NONE_WIRE_VALUE
                || expected == CALIBRATION_NONE_WIRE_VALUE
            {
                None
            } else {
                Some(AdcCalibrationSample {
                    observed_mv: observed,
                    expected_mv: expected,
                    reference_temp_deci_c: None,
                    target_adc_mv: None,
                    reference_vin_mv: None,
                })
            };
            cursor += 4;
        }
    }
    config
}

fn encode_adc_calibration_slots(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for channel in [&config.rtd, &config.vin] {
        for fit in [channel.slots.a, channel.slots.b] {
            out[cursor..cursor + 4].copy_from_slice(&fit.gain.to_le_bytes());
            out[cursor + 4..cursor + 8].copy_from_slice(&fit.offset_mv.to_le_bytes());
            cursor += 8;
        }
    }
}

fn decode_adc_calibration_slots(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    let mut cursor = 0;
    for channel in [&mut config.rtd, &mut config.vin] {
        for slot in [AdcCalibrationSlotId::A, AdcCalibrationSlotId::B] {
            let gain = f32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
            let offset_mv = f32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
            *channel.slot_fit_mut(slot) = AdcCalibrationSlotFit { gain, offset_mv };
            cursor += 8;
        }
    }
}

fn encode_adc_calibration_active_slots(config: &AdcCalibrationConfig, out: &mut [u8]) {
    out[0] = encode_slot_id(config.rtd.active_slot);
    out[1] = encode_slot_id(config.vin.active_slot);
}

fn decode_adc_calibration_active_slots(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    config.rtd.active_slot = decode_slot_id(bytes[0]);
    config.vin.active_slot = decode_slot_id(bytes[1]);
}

const fn encode_slot_id(slot: AdcCalibrationSlotId) -> u8 {
    match slot {
        AdcCalibrationSlotId::A => 0,
        AdcCalibrationSlotId::B => 1,
    }
}

const fn decode_slot_id(value: u8) -> AdcCalibrationSlotId {
    match value {
        1 => AdcCalibrationSlotId::B,
        _ => AdcCalibrationSlotId::A,
    }
}

fn migrate_legacy_adc_calibration(
    calibration: &mut AdcCalibrationConfig,
    legacy_active: &AdcCalibrationConfig,
    legacy_draft: Option<&AdcCalibrationConfig>,
) {
    calibration.rtd.samples = legacy_active.rtd.samples;
    calibration.vin.samples = legacy_active.vin.samples;

    calibration.rtd.slots.a = legacy_fit_channel(
        &legacy_active.rtd,
        legacy_default_points(AdcCalibrationChannel::Rtd),
    );
    calibration.vin.slots.a = legacy_fit_channel(
        &legacy_active.vin,
        legacy_default_points(AdcCalibrationChannel::Vin),
    );

    if let Some(legacy_draft) = legacy_draft {
        calibration.rtd.slots.b = legacy_fit_channel(
            &legacy_draft.rtd,
            legacy_default_points(AdcCalibrationChannel::Rtd),
        );
        calibration.vin.slots.b = legacy_fit_channel(
            &legacy_draft.vin,
            legacy_default_points(AdcCalibrationChannel::Vin),
        );
    } else {
        calibration.rtd.slots.b = AdcCalibrationSlotFit::default();
        calibration.vin.slots.b = AdcCalibrationSlotFit::default();
    }

    calibration.rtd.active_slot = AdcCalibrationSlotId::A;
    calibration.vin.active_slot = AdcCalibrationSlotId::A;
}

fn backfill_new_adc_calibration_defaults(calibration: &mut AdcCalibrationConfig) {
    if calibration.rtd.slots.a.gain == 0.0 && calibration.rtd.slots.a.offset_mv == 0.0 {
        calibration.rtd.slots.a = AdcCalibrationSlotFit::default();
    }
    if calibration.rtd.slots.b.gain == 0.0 && calibration.rtd.slots.b.offset_mv == 0.0 {
        calibration.rtd.slots.b = AdcCalibrationSlotFit::default();
    }
    if calibration.vin.slots.a.gain == 0.0 && calibration.vin.slots.a.offset_mv == 0.0 {
        calibration.vin.slots.a = AdcCalibrationSlotFit::default();
    }
    if calibration.vin.slots.b.gain == 0.0 && calibration.vin.slots.b.offset_mv == 0.0 {
        calibration.vin.slots.b = AdcCalibrationSlotFit::default();
    }
}

fn decode_adc_calibration_references(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    let mut cursor = 0;
    for slot in config.rtd.samples.iter_mut() {
        let reference = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.reference_temp_deci_c =
                (reference != CALIBRATION_REFERENCE_NONE_WIRE_VALUE).then_some(reference);
            sample.target_adc_mv = None;
            sample.reference_vin_mv = None;
        }
        cursor += 2;
    }
    for slot in config.vin.samples.iter_mut() {
        let reference = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.reference_vin_mv =
                (reference != CALIBRATION_REFERENCE_NONE_WIRE_VALUE).then_some(reference as u16);
            sample.reference_temp_deci_c = None;
            sample.target_adc_mv = None;
        }
        cursor += 2;
    }
}

fn decode_adc_calibration_targets(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    let mut cursor = 0;
    for slot in config.rtd.samples.iter_mut() {
        let target = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.target_adc_mv = (target != CALIBRATION_NONE_WIRE_VALUE).then_some(target);
        }
        cursor += 2;
    }
}

fn encode_heater_curve(config: &HeaterCurveConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for point in config.points {
        let temp = point
            .map(|point| point.temp_centi_c)
            .unwrap_or(PRESET_NONE_WIRE_VALUE);
        let resistance = point
            .map(|point| point.resistance_milliohms)
            .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&temp.to_le_bytes());
        out[cursor + 2..cursor + 4].copy_from_slice(&resistance.to_le_bytes());
        cursor += 4;
    }
}

fn decode_heater_curve(bytes: &[u8]) -> HeaterCurveConfig {
    let mut config = HeaterCurveConfig::default();
    let mut cursor = 0;
    for slot in config.points.iter_mut() {
        let temp = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        let resistance = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
        *slot = if temp == PRESET_NONE_WIRE_VALUE || resistance == CALIBRATION_NONE_WIRE_VALUE {
            None
        } else {
            Some(HeaterCurvePoint {
                temp_centi_c: temp,
                resistance_milliohms: resistance,
            })
        };
        cursor += 4;
    }
    config
}

fn encode_thermal_control_profile(config: &ThermalControlProfileConfig, out: &mut [u8]) -> usize {
    out[..THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN]
        .copy_from_slice(&THERMAL_CONTROL_PROFILE_LAYOUT_MARKER);
    let settings_start = THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN;
    let settings_end =
        settings_start + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY;
    encode_thermal_control_profile_settings(
        &config.settings,
        &mut out[settings_start..settings_end],
    );
    let mut cursor = settings_end;
    for point in config
        .points
        .into_iter()
        .flatten()
        .take(THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS)
    {
        let target = clamp_temp_c(point.target_temp_c);
        let brake_distance = point.brake_distance_centi_c.clamp(100, 5_000);
        let approach_power = point.approach_power_permille.min(1_000);
        let warmup_power = point.warmup_power_permille.min(1_000);
        let warmup_reenter = point.warmup_reenter_centi_c.clamp(50, 5_000);
        let approach_floor_power = point.approach_floor_power_permille.min(1_000);
        let approach_damping_exponent = point.approach_damping_exponent_permille.clamp(
            100,
            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        );
        let approach_tail_window = point
            .approach_tail_window_centi_c
            .min(THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX);
        let packed_approach_damping = (approach_damping_exponent
            & THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_VALUE_MASK)
            | (((approach_tail_window
                + THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_STEP_CENTI_C / 2)
                / THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_STEP_CENTI_C)
                << 12);
        let hold_power = point.hold_power_permille.min(1_000);
        let hold_reheat_power = point.hold_reheat_power_permille.min(1_000);
        let hold_entry = point.hold_entry_centi_c;
        let hold_exit = point.hold_exit_centi_c;
        let hold_on = point.hold_on_centi_c;
        let hold_off = point.hold_off_centi_c;
        let overshoot_cutoff = point.overshoot_cutoff_centi_c;
        let hold_kp = point.hold_kp_permille_per_c.min(10_000);
        let hold_ki = point.hold_ki_permille_per_c_tick.min(10_000);
        let hold_blend = point
            .hold_blend_ticks
            .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
        let approach_lead = point
            .approach_lead_ticks
            .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
        let hold_lead = point
            .hold_lead_ticks
            .min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX);
        out[cursor..cursor + 2].copy_from_slice(&target.to_le_bytes());
        out[cursor + 2..cursor + 4].copy_from_slice(&brake_distance.to_le_bytes());
        out[cursor + 4..cursor + 6].copy_from_slice(&warmup_power.to_le_bytes());
        out[cursor + 6..cursor + 8].copy_from_slice(&approach_power.to_le_bytes());
        out[cursor + 8..cursor + 10].copy_from_slice(&approach_floor_power.to_le_bytes());
        out[cursor + 10..cursor + 12].copy_from_slice(&packed_approach_damping.to_le_bytes());
        out[cursor + 12..cursor + 14].copy_from_slice(&hold_power.to_le_bytes());
        out[cursor + 14..cursor + 16].copy_from_slice(&hold_reheat_power.to_le_bytes());
        out[cursor + 16..cursor + 18].copy_from_slice(&hold_entry.to_le_bytes());
        out[cursor + 18..cursor + 20].copy_from_slice(&hold_exit.to_le_bytes());
        out[cursor + 20..cursor + 22].copy_from_slice(&hold_on.to_le_bytes());
        out[cursor + 22..cursor + 24].copy_from_slice(&hold_off.to_le_bytes());
        out[cursor + 24..cursor + 26].copy_from_slice(&overshoot_cutoff.to_le_bytes());
        out[cursor + 26..cursor + 28].copy_from_slice(&hold_kp.to_le_bytes());
        out[cursor + 28..cursor + 30].copy_from_slice(&hold_ki.to_le_bytes());
        out[cursor + 30..cursor + 32].copy_from_slice(&hold_blend.to_le_bytes());
        out[cursor + 32..cursor + 34].copy_from_slice(&approach_lead.to_le_bytes());
        out[cursor + 34..cursor + 36].copy_from_slice(&hold_lead.to_le_bytes());
        out[cursor + 36..cursor + 38].copy_from_slice(&warmup_reenter.to_le_bytes());
        cursor += THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER;
    }
    cursor
}

#[allow(clippy::manual_is_multiple_of)]
fn decode_thermal_control_profile(bytes: &[u8]) -> ThermalControlProfileConfig {
    let mut config = ThermalControlProfileConfig::default();
    let mut cursor = 0;
    // Preserve the preceding on-device profile layout so an upgrade does not shift the
    // working-voltage floor or current reserve into the wrong fields.
    let point_payload_len = if bytes.starts_with(&THERMAL_CONTROL_PROFILE_LAYOUT_MARKER)
        && bytes.len()
            >= THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
                + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
        && (bytes.len()
            - THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
            - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            == 0
        && (bytes.len()
            - THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
            - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        let settings_start = THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN;
        let settings_end =
            settings_start + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY;
        config.settings =
            decode_thermal_control_profile_settings(&bytes[settings_start..settings_end]);
        cursor = settings_end;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
    } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            == 0
        && (bytes.len() - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS
    {
        // Compatibility for development builds that emitted the point-local layout before it
        // gained an explicit marker. Legacy layouts above win whenever their lengths collide.
        config.settings = decode_thermal_control_profile_settings(
            &bytes[..THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY],
        );
        cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY;
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_HOLD_ON {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_WARMUP {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_HOLD_REHEAT {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT
    } else if bytes.len() == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_LEAD_TICKS {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
    } else {
        THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
    };
    let available_point_count = bytes.len().saturating_sub(cursor) / point_payload_len;
    for slot in config.points.iter_mut().take(available_point_count) {
        let target = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        let brake_distance = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
        let (warmup_power_permille, approach_power) = if point_payload_len
            == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            || point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            || point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
        {
            (
                u16::from_le_bytes([bytes[cursor + 4], bytes[cursor + 5]]),
                u16::from_le_bytes([bytes[cursor + 6], bytes[cursor + 7]]),
            )
        } else {
            let approach_power = u16::from_le_bytes([bytes[cursor + 4], bytes[cursor + 5]]);
            (approach_power, approach_power)
        };
        let (approach_floor_power, approach_damping_exponent_permille, hold_power) =
            if point_payload_len
                == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
                || point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
                || point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            {
                let packed_approach_damping =
                    u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]);
                (
                    u16::from_le_bytes([bytes[cursor + 8], bytes[cursor + 9]]),
                    (packed_approach_damping & THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_VALUE_MASK)
                        .clamp(
                            100,
                            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
                        ),
                    u16::from_le_bytes([bytes[cursor + 12], bytes[cursor + 13]]),
                )
            } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP {
                (
                    u16::from_le_bytes([bytes[cursor + 8], bytes[cursor + 9]]),
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                    u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]),
                )
            } else if point_payload_len
                == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT
            {
                (
                    u16::from_le_bytes([bytes[cursor + 6], bytes[cursor + 7]]),
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                    u16::from_le_bytes([bytes[cursor + 8], bytes[cursor + 9]]),
                )
            } else {
                (
                    legacy_approach_floor_power(
                        approach_power,
                        hold_power_from_legacy_bytes(&bytes[cursor + 6..cursor + 8]),
                        config.settings.approach_min_power_ratio_permille,
                    ),
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                    hold_power_from_legacy_bytes(&bytes[cursor + 6..cursor + 8]),
                )
            };
        let (
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
            warmup_reenter_centi_c,
        ) = if point_payload_len
            == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
        {
            (
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]),
                u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]),
                u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]),
                u16::from_le_bytes([bytes[cursor + 34], bytes[cursor + 35]]),
                u16::from_le_bytes([bytes[cursor + 36], bytes[cursor + 37]]),
            )
        } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
        {
            (
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]),
                u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]),
                u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]),
                u16::from_le_bytes([bytes[cursor + 34], bytes[cursor + 35]]),
                config.settings.warmup_reenter_centi_c,
            )
        } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN {
            (
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                config.settings.hold_on_centi_c,
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]),
                u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]),
                u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]),
                config.settings.warmup_reenter_centi_c,
            )
        } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP {
            (
                u16::from_le_bytes([bytes[cursor + 12], bytes[cursor + 13]]),
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                config.settings.hold_on_centi_c,
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]),
                u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]),
                config.settings.warmup_reenter_centi_c,
            )
        } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT {
            (
                u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]),
                u16::from_le_bytes([bytes[cursor + 12], bytes[cursor + 13]]),
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                config.settings.hold_on_centi_c,
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]),
                config.settings.warmup_reenter_centi_c,
            )
        } else if point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS {
            (
                config.settings.hold_reheat_power_permille,
                u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]),
                u16::from_le_bytes([bytes[cursor + 12], bytes[cursor + 13]]),
                config.settings.hold_on_centi_c,
                u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]),
                u16::from_le_bytes([bytes[cursor + 16], bytes[cursor + 17]]),
                u16::from_le_bytes([bytes[cursor + 18], bytes[cursor + 19]]),
                u16::from_le_bytes([bytes[cursor + 20], bytes[cursor + 21]]),
                u16::from_le_bytes([bytes[cursor + 22], bytes[cursor + 23]]),
                u16::from_le_bytes([bytes[cursor + 24], bytes[cursor + 25]]),
                u16::from_le_bytes([bytes[cursor + 26], bytes[cursor + 27]]),
                config.settings.warmup_reenter_centi_c,
            )
        } else {
            (
                config.settings.hold_reheat_power_permille,
                config.settings.hold_entry_centi_c,
                config.settings.hold_exit_centi_c,
                config.settings.hold_on_centi_c,
                config.settings.hold_off_centi_c,
                config.settings.overshoot_cutoff_centi_c,
                config.settings.hold_kp_permille_per_c,
                config.settings.hold_ki_permille_per_c_tick,
                config.settings.hold_blend_ticks,
                config.settings.approach_lead_ticks,
                config.settings.hold_lead_ticks,
                config.settings.warmup_reenter_centi_c,
            )
        };
        let approach_tail_window_centi_c = if point_payload_len
            == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            || point_payload_len == THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
        {
            (u16::from_le_bytes([bytes[cursor + 10], bytes[cursor + 11]]) >> 12)
                * THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_STEP_CENTI_C
        } else {
            0
        };
        *slot = if target == PRESET_NONE_WIRE_VALUE
            || brake_distance == CALIBRATION_NONE_WIRE_VALUE
            || approach_power == CALIBRATION_NONE_WIRE_VALUE
            || approach_floor_power == CALIBRATION_NONE_WIRE_VALUE
            || hold_power == CALIBRATION_NONE_WIRE_VALUE
        {
            None
        } else {
            Some(ThermalControlProfilePointConfig {
                target_temp_c: target,
                brake_distance_centi_c: brake_distance,
                warmup_power_permille,
                warmup_reenter_centi_c,
                approach_power_permille: approach_power,
                approach_floor_power_permille: approach_floor_power,
                approach_damping_exponent_permille,
                approach_tail_window_centi_c,
                hold_power_permille: hold_power,
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
            })
        };
        cursor += point_payload_len;
    }
    config
}

#[allow(clippy::manual_is_multiple_of)]
fn is_supported_thermal_control_profile(bytes: &[u8]) -> bool {
    let len = bytes.len();
    let marked_current = bytes.starts_with(&THERMAL_CONTROL_PROFILE_LAYOUT_MARKER)
        && len
            >= THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
                + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
        && (len
            - THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
            - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            == 0
        && (len
            - THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
            - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let current_with_point_warmup_reenter = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let previous_settings_with_current_points = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let current_with_settings = len >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_HOLD_ON
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let previous_current_with_settings = len >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let previous_current_with_previous_field = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_PREVIOUS_FIELD
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_PREVIOUS_FIELD)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_PREVIOUS_FIELD)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let current_with_previous_points = len >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_WARMUP
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let hold_reheat_with_matching_points = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_HOLD_REHEAT
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let lead_ticks_with_matching_points = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_LEAD_TICKS
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let current_settings_legacy_points = len >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let hold_reheat_settings_legacy_points = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let lead_ticks_settings_legacy_points = len
        >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let legacy_with_legacy_points = len >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY)
            % THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            == 0
        && (len - THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_LEGACY)
            / THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_LEGACY
            <= THERMAL_CONTROL_PROFILE_MAX_POINTS;
    let legacy_points_only = len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_LEGACY;
    let lead_ticks_points_only = len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_LEAD_TICKS;
    let hold_reheat_points_only =
        len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_HOLD_REHEAT;
    let current_points_only =
        len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER;
    let previous_current_points_only =
        len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_POINT_HOLD_ON;
    let current_legacy_points_only = len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN;
    let current_previous_points_only =
        len == THERMAL_CONTROL_PROFILE_POINTS_PAYLOAD_LEN_WITH_WARMUP;
    marked_current
        || current_with_point_warmup_reenter
        || previous_settings_with_current_points
        || current_with_settings
        || previous_current_with_settings
        || previous_current_with_previous_field
        || current_with_previous_points
        || hold_reheat_with_matching_points
        || lead_ticks_with_matching_points
        || current_settings_legacy_points
        || hold_reheat_settings_legacy_points
        || lead_ticks_settings_legacy_points
        || legacy_with_legacy_points
        || legacy_points_only
        || lead_ticks_points_only
        || hold_reheat_points_only
        || previous_current_points_only
        || current_legacy_points_only
        || current_previous_points_only
        || current_points_only
}

fn hold_power_from_legacy_bytes(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn legacy_approach_floor_power(approach_power: u16, hold_power: u16, ratio_permille: u16) -> u16 {
    let scaled = ((u32::from(approach_power.min(1_000)) * u32::from(ratio_permille.min(1_000)))
        + 500)
        / 1_000;
    hold_power.min(1_000).max(scaled.min(1_000) as u16)
}

fn encode_thermal_control_profile_settings(
    settings: &ThermalControlProfileSettingsConfig,
    out: &mut [u8],
) {
    let values = [
        settings.temp_filter_alpha_permille,
        settings.approach_max_ticks,
        settings.approach_min_power_ratio_permille,
        settings.auto_adjustable_working_floor_mv,
        settings.heater_current_reserve_ma,
        0,
        0,
    ];
    for (index, value) in values.into_iter().enumerate() {
        let cursor = index * 2;
        out[cursor..cursor + 2].copy_from_slice(&value.to_le_bytes());
    }
}

fn decode_thermal_control_profile_settings(bytes: &[u8]) -> ThermalControlProfileSettingsConfig {
    let mut values = [0u16; 18];
    for (index, value) in values.iter_mut().enumerate() {
        let cursor = index * 2;
        if cursor + 1 >= bytes.len() {
            break;
        }
        *value = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
    }
    let has_approach_min_ratio =
        bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_APPROACH_MIN_RATIO;
    if bytes.len() == THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY {
        return ThermalControlProfileSettingsConfig {
            temp_filter_alpha_permille: values[0],
            warmup_reenter_centi_c: THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
            hold_entry_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
            hold_exit_centi_c: THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
            hold_on_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
            hold_off_centi_c: THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
            overshoot_cutoff_centi_c: THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
            approach_max_ticks: values[1],
            approach_min_power_ratio_permille: values[2],
            hold_kp_permille_per_c: THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
            hold_ki_permille_per_c_tick:
                THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
            hold_blend_ticks: THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
            hold_reheat_power_permille: THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT,
            approach_lead_ticks: THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT,
            hold_lead_ticks: THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT,
            auto_adjustable_working_floor_mv: values[3].clamp(
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
            ),
            heater_current_reserve_ma: values[4]
                .min(THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX),
        };
    }
    // The former 17-word layout stored a removed field before the working floor. Current
    // layouts have a 5V-or-higher floor followed by a <=1000mA reserve, which keeps 5.0-6.0V
    // current records distinguishable without changing the persisted record version.
    let previous_layout_with_extra_field = bytes.len()
        == THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
        && (values[15] < THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN
            || (values[15] < 6_100
                && values[16] > THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX));
    ThermalControlProfileSettingsConfig {
        temp_filter_alpha_permille: values[0],
        warmup_reenter_centi_c: values[1],
        hold_entry_centi_c: values[2],
        hold_exit_centi_c: values[3],
        hold_on_centi_c: values[4],
        hold_off_centi_c: values[5],
        overshoot_cutoff_centi_c: values[6],
        approach_max_ticks: values[7],
        approach_min_power_ratio_permille: if has_approach_min_ratio {
            values[8]
        } else {
            THERMAL_CONTROL_PROFILE_APPROACH_MIN_POWER_RATIO_PERMILLE_DEFAULT
        },
        hold_kp_permille_per_c: if has_approach_min_ratio {
            values[9]
        } else {
            values[8]
        },
        hold_ki_permille_per_c_tick: if has_approach_min_ratio {
            values[10]
        } else {
            values[9]
        },
        hold_blend_ticks: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS
        {
            values[11]
        } else {
            THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT
        },
        hold_reheat_power_permille: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        {
            values[12].min(1_000)
        } else {
            THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT
        },
        approach_lead_ticks: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        {
            values[13].min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
        } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS {
            values[12].min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
        } else {
            THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT
        },
        hold_lead_ticks: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_HOLD_REHEAT
        {
            values[14].min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
        } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_LEAD_TICKS {
            values[13].min(THERMAL_CONTROL_PROFILE_APPROACH_MAX_TICKS_MAX)
        } else {
            THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT
        },
        auto_adjustable_working_floor_mv: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS
            || previous_layout_with_extra_field
        {
            values[16].clamp(
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
            )
        } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN {
            values[15].clamp(
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
            )
        } else {
            THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_DEFAULT
        },
        heater_current_reserve_ma: if bytes.len()
            >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS
        {
            values[17].min(THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX)
        } else if bytes.len() >= THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
            && !previous_layout_with_extra_field
        {
            values[16].min(THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX)
        } else {
            THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_DEFAULT
        },
    }
}

fn push_tlv(
    tag: u8,
    value: &[u8],
    out: &mut [u8],
    cursor: &mut usize,
) -> Result<(), MemoryEncodeError> {
    if value.len() > u16::MAX as usize {
        return Err(MemoryEncodeError::PayloadTooLarge);
    }
    let next = cursor
        .checked_add(3)
        .and_then(|position| position.checked_add(value.len()))
        .ok_or(MemoryEncodeError::PayloadTooLarge)?;
    if next > out.len() {
        return Err(MemoryEncodeError::BufferTooSmall);
    }
    out[*cursor] = tag;
    out[*cursor + 1..*cursor + 3].copy_from_slice(&(value.len() as u16).to_le_bytes());
    out[*cursor + 3..next].copy_from_slice(value);
    *cursor = next;
    Ok(())
}

pub const fn clamp_temp_c(value: i16) -> i16 {
    if value < FRONTPANEL_TARGET_TEMP_MIN_C {
        FRONTPANEL_TARGET_TEMP_MIN_C
    } else if value > FRONTPANEL_TARGET_TEMP_MAX_C {
        FRONTPANEL_TARGET_TEMP_MAX_C
    } else {
        value
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> MemoryConfig {
        let mut config = MemoryConfig {
            target_temp_c: 222,
            selected_preset_slot: 4,
            active_cooling_enabled: false,
            wifi_auto_reconnect: true,
            telemetry_interval_ms: 1_250,
            ..MemoryConfig::default()
        };
        config.presets_c[2] = None;
        config.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        config.wifi_password.push_str("secret-pass").unwrap();
        config
            .adc_calibration
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_030,
                reference_temp_deci_c: Some(250),
                target_adc_mv: Some(970),
                reference_vin_mv: None,
            })
            .unwrap();
        config
            .adc_calibration
            .vin
            .insert(AdcCalibrationSample {
                observed_mv: 1_800,
                expected_mv: 1_760,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: Some(20_000),
            })
            .unwrap();
        config.adc_calibration.rtd.slots.a = AdcCalibrationSlotFit {
            gain: 1.0,
            offset_mv: 30.0,
        };
        config.adc_calibration.rtd.slots.b = AdcCalibrationSlotFit {
            gain: 0.99,
            offset_mv: -10.0,
        };
        config.adc_calibration.vin.slots.a = AdcCalibrationSlotFit {
            gain: 0.98,
            offset_mv: 15.0,
        };
        config.adc_calibration.vin.slots.b = AdcCalibrationSlotFit {
            gain: 1.01,
            offset_mv: -5.0,
        };
        config.adc_calibration.rtd.active_slot = AdcCalibrationSlotId::B;
        config.adc_calibration.vin.active_slot = AdcCalibrationSlotId::A;
        config.active_thermal_control_profile.points[0] = Some(ThermalControlProfilePointConfig {
            target_temp_c: 100,
            brake_distance_centi_c: 700,
            warmup_power_permille: 320,
            warmup_reenter_centi_c: THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
            approach_power_permille: 320,
            approach_floor_power_permille: 220,
            approach_damping_exponent_permille:
                THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 220,
            hold_reheat_power_permille: THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT,
            hold_entry_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
            hold_exit_centi_c: THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
            hold_on_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
            hold_off_centi_c: THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
            overshoot_cutoff_centi_c: THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
            hold_kp_permille_per_c: THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
            hold_ki_permille_per_c_tick:
                THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
            hold_blend_ticks: THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
            approach_lead_ticks: THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT,
            hold_lead_ticks: THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT,
        });
        config.active_thermal_control_profile.points[1] = Some(ThermalControlProfilePointConfig {
            target_temp_c: 210,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 260,
            warmup_reenter_centi_c: THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
            approach_power_permille: 260,
            approach_floor_power_permille: 180,
            approach_damping_exponent_permille:
                THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
            approach_tail_window_centi_c: 0,
            hold_power_permille: 180,
            hold_reheat_power_permille: THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT,
            hold_entry_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
            hold_exit_centi_c: THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
            hold_on_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
            hold_off_centi_c: THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
            overshoot_cutoff_centi_c: THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
            hold_kp_permille_per_c: THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
            hold_ki_permille_per_c_tick:
                THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
            hold_blend_ticks: THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
            approach_lead_ticks: THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT,
            hold_lead_ticks: THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT,
        });
        config.thermal_control_profile_pps5a = config.active_thermal_control_profile;
        config.thermal_control_profile_pps5a.points[1]
            .as_mut()
            .expect("5A profile point")
            .target_temp_c = 250;
        config.thermal_profile_mode = ThermalProfileMode::W100;
        config
    }

    #[test]
    fn default_config_matches_frontpanel_defaults() {
        let config = MemoryConfig::default();
        assert_eq!(config.target_temp_c, 100);
        assert_eq!(config.selected_preset_slot, 1);
        assert_eq!(config.presets_c[0], Some(50));
        assert_eq!(config.presets_c[9], Some(300));
        assert!(config.active_cooling_enabled);
        assert_eq!(config.adc_calibration.rtd.sample_count(), 0);
        assert_eq!(config.adc_calibration.vin.sample_count(), 0);
        assert_eq!(
            config.adc_calibration.rtd.active_slot,
            AdcCalibrationSlotId::A
        );
        assert_eq!(
            config.adc_calibration.vin.slots.a,
            AdcCalibrationSlotFit::default()
        );
    }

    #[test]
    fn sanitize_forces_wifi_auto_reconnect_policy_on() {
        let mut config = MemoryConfig {
            wifi_auto_reconnect: false,
            ..MemoryConfig::default()
        };

        config.sanitize();

        assert!(config.wifi_auto_reconnect);
    }

    #[test]
    fn static_ipv4_rejects_non_unicast_values() {
        let valid = WifiStaticIpv4Config {
            address: [192, 168, 31, 42],
            prefix_len: 24,
            gateway: [192, 168, 31, 1],
            dns: [1, 1, 1, 1],
        };
        assert!(valid.is_valid());
        assert!(
            !WifiStaticIpv4Config {
                address: [224, 0, 0, 1],
                ..valid
            }
            .is_valid()
        );
        assert!(
            !WifiStaticIpv4Config {
                gateway: [127, 0, 0, 1],
                ..valid
            }
            .is_valid()
        );
    }

    #[test]
    fn record_roundtrip_preserves_config() {
        let record = MemoryRecord {
            sequence: 42,
            config: {
                let mut config = sample_config();
                config.wifi_static_ipv4 = Some(WifiStaticIpv4Config {
                    address: [192, 168, 31, 42],
                    prefix_len: 24,
                    gateway: [192, 168, 31, 1],
                    dns: [1, 1, 1, 1],
                });
                config.lan_pairing_token = Some([0xa5; crate::lan::LAN_TOKEN_BYTES]);
                config
            },
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        let decoded = decode_memory_record(&bytes[..len]).unwrap();
        assert_eq!(decoded, record);
        assert_eq!(
            decoded.config.adc_calibration.rtd.samples[0]
                .and_then(|sample| sample.reference_temp_deci_c),
            Some(250)
        );
        assert_eq!(
            decoded.config.adc_calibration.rtd.samples[0].and_then(|sample| sample.target_adc_mv),
            Some(970)
        );
        assert_eq!(
            decoded.config.adc_calibration.vin.samples[0]
                .and_then(|sample| sample.reference_vin_mv),
            Some(20_000)
        );
        assert_eq!(decoded.config.adc_calibration.rtd.slots.b.offset_mv, -10.0);
        assert_eq!(
            decoded.config.adc_calibration.vin.active_slot,
            AdcCalibrationSlotId::A
        );
        assert_eq!(
            decoded.config.active_thermal_control_profile.points[1],
            Some(ThermalControlProfilePointConfig {
                target_temp_c: 210,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 260,
                warmup_reenter_centi_c: THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
                approach_power_permille: 260,
                approach_floor_power_permille: 180,
                approach_damping_exponent_permille:
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
                hold_reheat_power_permille:
                    THERMAL_CONTROL_PROFILE_HOLD_REHEAT_POWER_PERMILLE_DEFAULT,
                hold_entry_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
                hold_exit_centi_c: THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
                hold_on_centi_c: THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
                hold_off_centi_c: THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
                overshoot_cutoff_centi_c: THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
                hold_kp_permille_per_c: THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
                hold_ki_permille_per_c_tick:
                    THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
                hold_blend_ticks: THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
                approach_lead_ticks: THERMAL_CONTROL_PROFILE_APPROACH_LEAD_TICKS_DEFAULT,
                hold_lead_ticks: THERMAL_CONTROL_PROFILE_HOLD_LEAD_TICKS_DEFAULT,
            })
        );
        assert_eq!(
            decoded.config.thermal_profile_mode,
            ThermalProfileMode::W100
        );
        assert_eq!(
            decoded.config.thermal_control_profile_pps5a.points[1]
                .expect("5A profile point")
                .target_temp_c,
            250
        );
    }

    #[test]
    fn record_roundtrip_preserves_six_point_heater_curve() {
        let mut config = sample_config();
        config.active_heater_curve.points = [
            Some(HeaterCurvePoint {
                temp_centi_c: 0,
                resistance_milliohms: 2_948,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 2_000,
                resistance_milliohms: 3_200,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 14_098,
                resistance_milliohms: 3_904,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 18_221,
                resistance_milliohms: 3_912,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 21_752,
                resistance_milliohms: 3_919,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 24_142,
                resistance_milliohms: 3_925,
            }),
            None,
            None,
        ];
        let record = MemoryRecord {
            sequence: 44,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).unwrap();

        assert_eq!(decode_memory_record(&bytes[..len]).unwrap(), record);
    }

    #[test]
    fn record_roundtrip_preserves_full_profiles_and_heater_curve_at_max_credentials() {
        let mut config = sample_config();
        config.wifi_ssid.clear();
        config
            .wifi_ssid
            .push_str("12345678901234567890123456789012")
            .unwrap();
        config.wifi_password.clear();
        config
            .wifi_password
            .push_str("1234567890123456789012345678901234567890123456789012345678901234")
            .unwrap();
        let template = config.active_thermal_control_profile.points[0].unwrap();
        for (slot, target_temp_c) in [60, 80, 100, 120, 140, 160, 180, 200, 220, 240]
            .into_iter()
            .enumerate()
        {
            let point = Some(ThermalControlProfilePointConfig {
                target_temp_c,
                ..template
            });
            config.active_thermal_control_profile.points[slot] = point;
            config.thermal_control_profile_pps5a.points[slot] = point;
        }
        config.active_heater_curve.points = [
            Some(HeaterCurvePoint {
                temp_centi_c: 0,
                resistance_milliohms: 2_948,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 2_000,
                resistance_milliohms: 3_200,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 14_098,
                resistance_milliohms: 3_904,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 18_221,
                resistance_milliohms: 3_912,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 21_752,
                resistance_milliohms: 3_919,
            }),
            Some(HeaterCurvePoint {
                temp_centi_c: 24_142,
                resistance_milliohms: 3_925,
            }),
            None,
            None,
        ];

        let record = MemoryRecord {
            sequence: 45,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).unwrap();

        assert!(len <= MEMORY_SLOT_SIZE);
        assert_eq!(decode_memory_record(&bytes[..len]).unwrap(), record);
    }

    #[test]
    fn v1_header_decodes_the_legacy_profile_as_the_65w_bank() {
        let record = MemoryRecord {
            sequence: 7,
            config: sample_config(),
        };
        let mut current = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut current).unwrap();
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        bytes[..MEMORY_RECORD_HEADER_LEN].copy_from_slice(&current[..MEMORY_RECORD_HEADER_LEN]);
        bytes[4] = 1;
        let mut source = MEMORY_RECORD_HEADER_LEN;
        let mut destination = MEMORY_RECORD_HEADER_LEN;
        while source < len {
            let tag = current[source];
            let value_len = u16::from_le_bytes([current[source + 1], current[source + 2]]) as usize;
            let value_start = source + 3;
            let value_end = value_start + value_len;
            if tag != TLV_THERMAL_CONTROL_PROFILE_PPS5A && tag != TLV_THERMAL_PROFILE_MODE {
                bytes[destination] = if tag == TLV_THERMAL_CONTROL_PROFILE_PPS3A {
                    TLV_ACTIVE_THERMAL_CONTROL_PROFILE
                } else {
                    tag
                };
                bytes[destination + 1] = value_len as u8;
                bytes[destination + 2..destination + 2 + value_len]
                    .copy_from_slice(&current[value_start..value_end]);
                destination += 2 + value_len;
            }
            source = value_end;
        }
        let payload_len = destination - MEMORY_RECORD_HEADER_LEN;
        bytes[6..8].copy_from_slice(&(payload_len as u16).to_le_bytes());
        let crc = crc32_update(
            crc32(&bytes[0..12]),
            &bytes[MEMORY_RECORD_HEADER_LEN..destination],
        ) ^ 0xffff_ffff;
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());

        let decoded = decode_memory_record(&bytes[..destination]).unwrap();
        assert_eq!(decoded.sequence, record.sequence);
        assert_eq!(decoded.config.thermal_profile_mode, ThermalProfileMode::W65);
        assert_eq!(
            decoded
                .config
                .thermal_profile(ThermalProfileBank::Pps3a)
                .points[0]
                .expect("migrated 65W point")
                .target_temp_c,
            100
        );
        assert_eq!(
            decoded
                .config
                .thermal_profile(ThermalProfileBank::Pps5a)
                .points[0],
            None
        );
    }

    #[test]
    fn thermal_profile_tail_window_roundtrips_and_old_point_layout_defaults_to_zero() {
        let mut config = sample_config();
        config.active_thermal_control_profile.points[0]
            .as_mut()
            .expect("sample profile point")
            .approach_tail_window_centi_c = 175;

        let mut current = [0u8; THERMAL_CONTROL_PROFILE_PAYLOAD_LEN];
        let current_len =
            encode_thermal_control_profile(&config.active_thermal_control_profile, &mut current);
        let decoded_current = decode_thermal_control_profile(&current[..current_len]);
        assert_eq!(
            decoded_current.points[0]
                .expect("current point")
                .approach_tail_window_centi_c,
            175
        );

        let point = config.active_thermal_control_profile.points[0].expect("sample profile point");
        let settings = config.active_thermal_control_profile.settings;
        let mut legacy = [0u8; THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
            + THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN];
        let setting_values = [
            settings.temp_filter_alpha_permille,
            settings.warmup_reenter_centi_c,
            settings.hold_entry_centi_c,
            settings.hold_exit_centi_c,
            settings.hold_on_centi_c,
            settings.hold_off_centi_c,
            settings.overshoot_cutoff_centi_c,
            settings.approach_max_ticks,
            settings.approach_min_power_ratio_permille,
            settings.hold_kp_permille_per_c,
            settings.hold_ki_permille_per_c_tick,
            settings.hold_blend_ticks,
            settings.hold_reheat_power_permille,
            settings.approach_lead_ticks,
            settings.hold_lead_ticks,
            settings.auto_adjustable_working_floor_mv,
            settings.heater_current_reserve_ma,
        ];
        for (index, value) in setting_values.into_iter().enumerate() {
            let cursor = index * 2;
            legacy[cursor..cursor + 2].copy_from_slice(&value.to_le_bytes());
        }
        let point_offset = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN;
        let packed_damping = point.approach_damping_exponent_permille.clamp(
            100,
            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        ) & THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_VALUE_MASK;
        legacy[point_offset..point_offset + 2].copy_from_slice(&point.target_temp_c.to_le_bytes());
        legacy[point_offset + 2..point_offset + 4]
            .copy_from_slice(&point.brake_distance_centi_c.to_le_bytes());
        legacy[point_offset + 4..point_offset + 6]
            .copy_from_slice(&point.warmup_power_permille.to_le_bytes());
        legacy[point_offset + 6..point_offset + 8]
            .copy_from_slice(&point.approach_power_permille.to_le_bytes());
        legacy[point_offset + 8..point_offset + 10]
            .copy_from_slice(&point.approach_floor_power_permille.to_le_bytes());
        legacy[point_offset + 10..point_offset + 12].copy_from_slice(&packed_damping.to_le_bytes());
        legacy[point_offset + 12..point_offset + 14]
            .copy_from_slice(&point.hold_power_permille.to_le_bytes());
        legacy[point_offset + 14..point_offset + 16]
            .copy_from_slice(&point.hold_reheat_power_permille.to_le_bytes());
        legacy[point_offset + 16..point_offset + 18]
            .copy_from_slice(&point.hold_entry_centi_c.to_le_bytes());
        legacy[point_offset + 18..point_offset + 20]
            .copy_from_slice(&point.hold_exit_centi_c.to_le_bytes());
        legacy[point_offset + 20..point_offset + 22]
            .copy_from_slice(&point.hold_off_centi_c.to_le_bytes());
        legacy[point_offset + 22..point_offset + 24]
            .copy_from_slice(&point.overshoot_cutoff_centi_c.to_le_bytes());
        legacy[point_offset + 24..point_offset + 26]
            .copy_from_slice(&point.hold_kp_permille_per_c.to_le_bytes());
        legacy[point_offset + 26..point_offset + 28]
            .copy_from_slice(&point.hold_ki_permille_per_c_tick.to_le_bytes());
        legacy[point_offset + 28..point_offset + 30]
            .copy_from_slice(&point.hold_blend_ticks.to_le_bytes());
        legacy[point_offset + 30..point_offset + 32]
            .copy_from_slice(&point.approach_lead_ticks.to_le_bytes());
        legacy[point_offset + 32..point_offset + 34]
            .copy_from_slice(&point.hold_lead_ticks.to_le_bytes());
        let decoded_legacy = decode_thermal_control_profile(&legacy);
        assert_eq!(
            decoded_legacy.points[0]
                .expect("legacy point")
                .approach_tail_window_centi_c,
            0
        );
    }

    #[test]
    fn thermal_profile_previous_layout_keeps_floor_and_reserve() {
        let mut bytes = [0u8; THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_PREVIOUS];
        bytes[16 * 2..17 * 2].copy_from_slice(&7_200u16.to_le_bytes());
        bytes[17 * 2..18 * 2].copy_from_slice(&350u16.to_le_bytes());

        let decoded = decode_thermal_control_profile(&bytes);
        assert_eq!(decoded.settings.auto_adjustable_working_floor_mv, 7_200);
        assert_eq!(decoded.settings.heater_current_reserve_ma, 350);

        let mut encoded = [0u8; THERMAL_CONTROL_PROFILE_PAYLOAD_LEN];
        let encoded_len = encode_thermal_control_profile(&decoded, &mut encoded);
        assert_eq!(
            encoded_len,
            THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN
                + THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
        );
        assert_eq!(
            &encoded[..THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN],
            &THERMAL_CONTROL_PROFILE_LAYOUT_MARKER
        );
        let settings_start = THERMAL_CONTROL_PROFILE_LAYOUT_MARKER_LEN;
        assert_eq!(
            u16::from_le_bytes([
                encoded[settings_start + 3 * 2],
                encoded[settings_start + 3 * 2 + 1],
            ]),
            7_200
        );
        assert_eq!(
            u16::from_le_bytes([
                encoded[settings_start + 4 * 2],
                encoded[settings_start + 4 * 2 + 1],
            ]),
            350
        );
    }

    #[test]
    fn thermal_profile_layout_marker_prevents_legacy_five_point_collision() {
        let mut legacy = [0u8; THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
            + 5 * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN];
        let settings = sample_config().active_thermal_control_profile.settings;
        let setting_values = [
            settings.temp_filter_alpha_permille,
            settings.warmup_reenter_centi_c,
            settings.hold_entry_centi_c,
            settings.hold_exit_centi_c,
            settings.hold_on_centi_c,
            settings.hold_off_centi_c,
            settings.overshoot_cutoff_centi_c,
            settings.approach_max_ticks,
            settings.approach_min_power_ratio_permille,
            settings.hold_kp_permille_per_c,
            settings.hold_ki_permille_per_c_tick,
            settings.hold_blend_ticks,
            settings.hold_reheat_power_permille,
            settings.approach_lead_ticks,
            settings.hold_lead_ticks,
            settings.auto_adjustable_working_floor_mv,
            settings.heater_current_reserve_ma,
        ];
        for (index, value) in setting_values.into_iter().enumerate() {
            let offset = index * 2;
            legacy[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        for index in 0..5 {
            let cursor = THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN
                + index * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN;
            let values = [
                60 + index as u16 * 20,
                500,
                1_000,
                800,
                500,
                1_000,
                200,
                220,
                100,
                200,
                100,
                200,
                10,
                1,
                2,
                3,
                4,
            ];
            for (field_index, value) in values.into_iter().enumerate() {
                let offset = cursor + field_index * 2;
                legacy[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            }
        }

        assert_eq!(
            legacy.len(),
            THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN_WITH_GLOBALS_ONLY
                + 5 * THERMAL_CONTROL_PROFILE_POINT_PAYLOAD_LEN_WITH_POINT_WARMUP_REENTER
        );
        let decoded = decode_thermal_control_profile(&legacy);
        assert_eq!(decoded.settings, settings);
        assert_eq!(
            decoded.points[0].expect("first legacy point").target_temp_c,
            60
        );
        assert_eq!(
            decoded.points[4].expect("last legacy point").target_temp_c,
            140
        );
    }

    #[test]
    fn thermal_profile_previous_short_layout_keeps_working_floor() {
        let mut bytes = [0u8; THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN];
        bytes[15 * 2..16 * 2].copy_from_slice(&300u16.to_le_bytes());
        bytes[16 * 2..17 * 2].copy_from_slice(&7_400u16.to_le_bytes());

        let decoded = decode_thermal_control_profile(&bytes);
        assert_eq!(decoded.settings.auto_adjustable_working_floor_mv, 7_400);
        assert_eq!(
            decoded.settings.heater_current_reserve_ma,
            THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_DEFAULT
        );
    }

    #[test]
    fn thermal_profile_current_short_layout_preserves_5v_floor_and_reserve() {
        let mut bytes = [0u8; THERMAL_CONTROL_PROFILE_SETTINGS_PAYLOAD_LEN];
        bytes[15 * 2..16 * 2].copy_from_slice(&5_000u16.to_le_bytes());
        bytes[16 * 2..17 * 2].copy_from_slice(&350u16.to_le_bytes());

        let decoded = decode_thermal_control_profile(&bytes);
        assert_eq!(decoded.settings.auto_adjustable_working_floor_mv, 5_000);
        assert_eq!(decoded.settings.heater_current_reserve_ma, 350);
    }

    #[test]
    fn record_roundtrip_preserves_six_point_profile_at_max_credentials() {
        let mut config = sample_config();
        config.wifi_ssid.clear();
        config
            .wifi_ssid
            .push_str("12345678901234567890123456789012")
            .unwrap();
        config.wifi_password.clear();
        config
            .wifi_password
            .push_str("1234567890123456789012345678901234567890123456789012345678901234")
            .unwrap();
        let template = config.active_thermal_control_profile.points[0].unwrap();
        for (slot, target_temp_c) in [60, 100, 140, 180, 220, 250].into_iter().enumerate() {
            config.active_thermal_control_profile.points[slot] =
                Some(ThermalControlProfilePointConfig {
                    target_temp_c,
                    ..template
                });
        }

        let record = MemoryRecord {
            sequence: 43,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        assert!(len <= MEMORY_SLOT_SIZE);
        assert_eq!(decode_memory_record(&bytes[..len]).unwrap(), record);
    }

    #[test]
    fn thermal_profile_persistence_compacts_sparse_slots() {
        let mut config = sample_config();
        let point = config.active_thermal_control_profile.points[0].take();
        config.active_thermal_control_profile.points = [None; THERMAL_CONTROL_PROFILE_MAX_POINTS];
        config.active_thermal_control_profile.points[9] = point;
        let record = MemoryRecord {
            sequence: 43,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).expect("sparse profile encodes");
        let decoded = decode_memory_record(&bytes[..len]).expect("sparse profile decodes");

        assert!(decoded.config.active_thermal_control_profile.points[0].is_some());
        assert!(
            decoded.config.active_thermal_control_profile.points[1..]
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn thermal_profile_persistence_keeps_ten_dense_points() {
        let mut config = sample_config();
        let template = config.active_thermal_control_profile.points[0].unwrap();
        for (slot, target_temp_c) in [60, 80, 100, 120, 140, 160, 180, 200, 220, 240]
            .into_iter()
            .enumerate()
        {
            config.active_thermal_control_profile.points[slot] =
                Some(ThermalControlProfilePointConfig {
                    target_temp_c,
                    ..template
                });
        }

        let record = MemoryRecord {
            sequence: 44,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).expect("dense legacy profile encodes");
        let decoded = decode_memory_record(&bytes[..len]).expect("dense legacy profile decodes");

        assert_eq!(
            decoded
                .config
                .active_thermal_control_profile
                .points
                .iter()
                .flatten()
                .count(),
            THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS
        );
        assert_eq!(
            decoded.config.active_thermal_control_profile.points[0]
                .expect("first point")
                .target_temp_c,
            60
        );
        assert_eq!(
            decoded.config.active_thermal_control_profile.points[9]
                .expect("tenth point")
                .target_temp_c,
            240
        );
    }

    #[test]
    fn thermal_profile_persistence_materializes_legacy_inherited_point_fields() {
        let mut config = sample_config();
        let profile = &mut config.active_thermal_control_profile;
        profile.settings.warmup_reenter_centi_c = 620;
        profile.settings.hold_entry_centi_c = 31;
        profile.settings.hold_exit_centi_c = 142;
        profile.settings.hold_on_centi_c = 18;
        profile.settings.hold_off_centi_c = 77;
        profile.settings.overshoot_cutoff_centi_c = 205;
        profile.settings.hold_kp_permille_per_c = 27;
        profile.settings.hold_ki_permille_per_c_tick = 1;
        profile.settings.hold_blend_ticks = 4;
        profile.settings.hold_reheat_power_permille = 390;
        profile.settings.approach_lead_ticks = 3;
        profile.settings.hold_lead_ticks = 2;
        let point = profile.points[0].as_mut().expect("profile point");
        point.warmup_reenter_centi_c = 0;
        point.hold_entry_centi_c = 0;
        point.hold_exit_centi_c = 0;
        point.hold_on_centi_c = 0;
        point.hold_off_centi_c = 0;
        point.overshoot_cutoff_centi_c = 0;
        point.hold_kp_permille_per_c = 0;
        point.hold_ki_permille_per_c_tick = 0;
        point.hold_blend_ticks = 0;
        point.hold_reheat_power_permille = 0;
        point.approach_lead_ticks = 0;
        point.hold_lead_ticks = 0;

        config.sanitize();
        let expected = config.active_thermal_control_profile.points[0].expect("materialized point");
        let record = MemoryRecord {
            sequence: 45,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).expect("profile encodes");
        let decoded = decode_memory_record(&bytes[..len]).expect("profile decodes");

        assert_eq!(
            decoded.config.active_thermal_control_profile.points[0],
            Some(expected)
        );
        assert_eq!(expected.hold_ki_permille_per_c_tick, 1);
        assert_eq!(expected.hold_reheat_power_permille, 390);
    }

    #[test]
    fn decode_legacy_adc_calibration_without_reference_tlvs_keeps_samples() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 43,
            config: sample_config(),
        };
        let _len = encode_memory_record(&record, &mut bytes).unwrap();
        let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let payload_start = MEMORY_RECORD_HEADER_LEN;
        let payload_end = payload_start + payload_len;
        let payload = bytes[payload_start..payload_end].to_vec();
        let mut filtered_payload = [0u8; MEMORY_RECORD_PAYLOAD_MAX];
        let mut filtered_len = 0usize;
        let mut cursor = 0usize;
        while cursor < payload.len() {
            let tag = payload[cursor];
            let value_len = u16::from_le_bytes([payload[cursor + 1], payload[cursor + 2]]) as usize;
            let tlv_len = 3 + value_len;
            if tag != TLV_ADC_CALIBRATION_REFERENCES && tag != TLV_ADC_CALIBRATION_TARGETS {
                filtered_payload[filtered_len..filtered_len + tlv_len]
                    .copy_from_slice(&payload[cursor..cursor + tlv_len]);
                filtered_len += tlv_len;
            }
            cursor += tlv_len;
        }

        bytes[6..8].copy_from_slice(&(filtered_len as u16).to_le_bytes());
        bytes[payload_start..payload_start + filtered_len]
            .copy_from_slice(&filtered_payload[..filtered_len]);
        let crc = crc32_update(
            crc32(&bytes[0..12]),
            &bytes[payload_start..payload_start + filtered_len],
        ) ^ 0xffff_ffff;
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());

        let decoded = decode_memory_record(&bytes[..payload_start + filtered_len]).unwrap();
        let draft_rtd = decoded.config.adc_calibration.rtd.samples[0].unwrap();
        let active_vin = decoded.config.adc_calibration.vin.samples[0].unwrap();
        assert_eq!(draft_rtd.observed_mv, 1_000);
        assert_eq!(draft_rtd.expected_mv, 1_030);
        assert_eq!(draft_rtd.reference_temp_deci_c, None);
        assert_eq!(draft_rtd.target_adc_mv, None);
        assert_eq!(draft_rtd.reference_vin_mv, None);
        assert_eq!(active_vin.observed_mv, 1_800);
        assert_eq!(active_vin.expected_mv, 1_760);
        assert_eq!(active_vin.reference_temp_deci_c, None);
        assert_eq!(active_vin.target_adc_mv, None);
        assert_eq!(active_vin.reference_vin_mv, None);
    }

    #[test]
    fn adc_calibration_fit_uses_default_identity_without_custom_samples() {
        let config = AdcCalibrationConfig::default();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Vin);
        assert_eq!(fit.sample_count, 0);
        assert!((fit.gain - 1.0).abs() < 0.0001);
        assert!(fit.offset_mv.abs() < 0.0001);
        assert_eq!(
            correct_adc_mv(&config, AdcCalibrationChannel::Vin, 1_234),
            1_234
        );
    }

    #[test]
    fn adc_calibration_fit_mixes_default_points_for_single_custom_sample() {
        let mut config = AdcCalibrationConfig::default();
        config
            .vin
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_100,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: Some(12_000),
            })
            .unwrap();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Vin);
        assert_eq!(fit.sample_count, 1);
        assert!((fit.gain - 1.0).abs() < 0.0001);
        assert!((fit.offset_mv - 100.0).abs() < 0.0001);
    }

    #[test]
    fn adc_calibration_fit_ignores_default_points_after_two_custom_samples() {
        let mut config = AdcCalibrationConfig::default();
        config
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_100,
                reference_temp_deci_c: Some(250),
                target_adc_mv: Some(900),
                reference_vin_mv: None,
            })
            .unwrap();
        config
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 2_000,
                expected_mv: 2_200,
                reference_temp_deci_c: Some(500),
                target_adc_mv: Some(1_700),
                reference_vin_mv: None,
            })
            .unwrap();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Rtd);
        assert_eq!(fit.sample_count, 2);
        assert!((fit.gain - 1.1).abs() < 0.001);
        assert!(fit.offset_mv.abs() < 0.001);
    }

    #[test]
    fn adc_calibration_channel_caps_at_eight_samples_and_compacts_on_sanitize() {
        let mut config = AdcCalibrationConfig::default();
        for index in 0..ADC_CALIBRATION_MAX_SAMPLES {
            assert_eq!(
                config.rtd.insert(AdcCalibrationSample {
                    observed_mv: index as u16,
                    expected_mv: index as u16,
                    reference_temp_deci_c: Some(index as i16),
                    target_adc_mv: Some(index as u16),
                    reference_vin_mv: None,
                }),
                Some(index)
            );
        }
        assert_eq!(
            config.rtd.insert(AdcCalibrationSample {
                observed_mv: 9,
                expected_mv: 9,
                reference_temp_deci_c: Some(9),
                target_adc_mv: Some(9),
                reference_vin_mv: None,
            }),
            None
        );
        assert!(config.rtd.delete(3));
        sanitize_adc_calibration(&mut config);
        assert_eq!(config.rtd.samples[3].unwrap().observed_mv, 4);
        assert!(config.rtd.samples[ADC_CALIBRATION_MAX_SAMPLES - 1].is_none());
    }

    #[test]
    fn unknown_tlv_is_ignored() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 7,
            config: sample_config(),
        };
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let insert = MEMORY_RECORD_HEADER_LEN + payload_len;
        bytes[insert] = 0xee;
        bytes[insert + 1..insert + 3].copy_from_slice(&3u16.to_le_bytes());
        bytes[insert + 3..insert + 6].copy_from_slice(&[1, 2, 3]);
        let new_payload_len = payload_len + 6;
        bytes[6..8].copy_from_slice(&(new_payload_len as u16).to_le_bytes());
        let crc = crc32_update(
            crc32(&bytes[0..12]),
            &bytes[MEMORY_RECORD_HEADER_LEN..insert + 6],
        ) ^ 0xffff_ffff;
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());

        let decoded = decode_memory_record(&bytes[..len + 6]).unwrap();
        assert_eq!(decoded.config, record.config);
    }

    #[test]
    fn crc_rejects_corruption() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 1,
            config: sample_config(),
        };
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        bytes[len - 1] ^= 0x55;
        assert_eq!(
            decode_memory_record(&bytes[..len]),
            Err(MemoryDecodeError::CrcMismatch)
        );
    }

    #[test]
    fn latest_valid_slot_wins_and_corrupt_newer_falls_back() {
        let old = MemoryRecord {
            sequence: 3,
            config: MemoryConfig::default(),
        };
        let new = MemoryRecord {
            sequence: 4,
            config: sample_config(),
        };
        assert_eq!(
            select_latest_memory_record(Ok(old.clone()), Ok(new))
                .unwrap()
                .sequence,
            4
        );
        assert_eq!(
            select_latest_memory_record(Ok(old), Err(MemoryDecodeError::CrcMismatch))
                .unwrap()
                .sequence,
            3
        );
    }

    #[test]
    fn latest_sequence_wins_across_active_previous_and_legacy_slots() {
        let record = |sequence| MemoryRecord {
            sequence,
            config: MemoryConfig::default(),
        };

        let active_and_previous =
            select_latest_optional_memory_record(Some(record(7)), Some(record(11)));
        let selected = select_latest_optional_memory_record(active_and_previous, Some(record(5)));

        assert_eq!(selected.expect("latest record").sequence, 11);
    }

    #[test]
    fn sanitize_clamps_temperatures_and_bad_slot() {
        let mut config = MemoryConfig {
            target_temp_c: 450,
            selected_preset_slot: 99,
            ..MemoryConfig::default()
        };
        config.presets_c[0] = Some(-20);
        config.presets_c[1] = Some(480);
        config.telemetry_interval_ms = 0;
        config.sanitize();
        assert_eq!(config.target_temp_c, FRONTPANEL_TARGET_TEMP_MAX_C);
        assert_eq!(config.selected_preset_slot, 1);
        assert_eq!(config.presets_c[0], Some(FRONTPANEL_TARGET_TEMP_MIN_C));
        assert_eq!(config.presets_c[1], Some(FRONTPANEL_TARGET_TEMP_MAX_C));
        assert_eq!(config.telemetry_interval_ms, 500);
    }

    fn sample_thermal_plant_transaction() -> ThermalPlantRawTransaction {
        ThermalPlantRawTransaction {
            transaction_id: 0x515a_0001,
            anchors: [
                ThermalPlantRawAnchor {
                    ambient_raw_rtd_adc_mv: 250,
                    target_raw_rtd_adc_mv: 800,
                    heater_voltage_mv: 12_000,
                    heater_current_ma: 1_500,
                    gate_off_idle_power_mw: 100,
                    steady_hold_power_mw: 6_500,
                    ramp_duration_ms: 8_000,
                    ramp_energy_mj: 44_000,
                },
                ThermalPlantRawAnchor {
                    ambient_raw_rtd_adc_mv: 250,
                    target_raw_rtd_adc_mv: 2_200,
                    heater_voltage_mv: 18_000,
                    heater_current_ma: 3_000,
                    gate_off_idle_power_mw: 100,
                    steady_hold_power_mw: 27_000,
                    ramp_duration_ms: 4_000,
                    ramp_energy_mj: 156_000,
                },
            ],
        }
    }

    fn sample_transient_thermal_plant_transaction() -> ThermalPlantTransientTransaction {
        let mut samples = [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_100mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
        for (index, sample) in samples.iter_mut().take(24).enumerate() {
            *sample = ThermalPlantTransientSample {
                elapsed_ticks: (index as u16 + 1) * 10,
                raw_rtd_adc_mv: if index < 12 {
                    250 + index as u16 * 8
                } else {
                    330 - (index as u16 - 12) * 4
                },
                heater_voltage_100mv: if index < 12 { 200 } else { 0 },
                duty_percent: if index < 12 { 100 } else { 0 },
            };
        }
        ThermalPlantTransientTransaction {
            transaction_id: 0x5452_4e53,
            ambient_raw_rtd_adc_mv: 250,
            sample_count: 24,
            projection: ThermalPlantProjectionRecord::from_projection(ThermalPlantProjection {
                convection_mw_per_c: 120.0,
                radiation_mw_per_k4: 0.0000002,
                thermal_capacity_mj_per_c: 42_000.0,
                transport_delay_ms: 500,
            }),
            samples,
        }
    }

    #[test]
    fn raw_thermal_data_roundtrips_without_legacy_profiles() {
        let mut config = sample_config();
        config.heater_curve_raw_observations.points[0] = Some(HeaterCurveRawObservation {
            raw_rtd_adc_mv: 250,
            heater_voltage_mv: 10_000,
            heater_current_ma: 3_000,
            resistance_milliohms: 3_333,
        });
        config.thermal_plant_active = Some(sample_thermal_plant_transaction());
        config.thermal_plant_transient_active = Some(sample_transient_thermal_plant_transaction());
        let record = MemoryRecord {
            sequence: 51,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).expect("new model encodes");
        let decoded = decode_memory_record(&bytes[..len]).expect("new model decodes");

        assert_eq!(
            decoded.config.heater_curve_raw_observations,
            record.config.heater_curve_raw_observations
        );
        assert_eq!(
            decoded.config.thermal_plant_transient_active,
            record.config.thermal_plant_transient_active
        );
        assert_eq!(decoded.config.thermal_plant_active, None);
        assert!(
            decoded
                .config
                .active_thermal_control_profile
                .points
                .iter()
                .all(Option::is_none)
        );
        assert!(len <= MEMORY_SLOT_SIZE);
    }

    #[test]
    fn maximum_transient_trace_roundtrips_within_one_memory_record() {
        let mut config = sample_config();
        let mut transaction = sample_transient_thermal_plant_transaction();
        transaction.sample_count = THERMAL_PLANT_TRANSIENT_MAX_SAMPLES as u8;
        for (index, sample) in transaction.samples.iter_mut().enumerate().skip(24) {
            *sample = ThermalPlantTransientSample {
                elapsed_ticks: (index as u16 + 1) * 10,
                raw_rtd_adc_mv: if index < 64 {
                    250 + index as u16 * 4
                } else {
                    500 - (index as u16 - 64) * 2
                },
                heater_voltage_100mv: if index < 64 { 200 } else { 0 },
                duty_percent: if index < 64 { 100 } else { 0 },
            };
        }
        config.thermal_plant_transient_active = Some(transaction);
        let record = MemoryRecord {
            sequence: 52,
            config,
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).expect("maximum trace encodes");
        let decoded = decode_memory_record(&bytes[..len]).expect("maximum trace decodes");

        assert!(len <= MEMORY_SLOT_SIZE);
        assert_eq!(
            decoded.config.thermal_plant_transient_active,
            Some(transaction)
        );
    }

    #[test]
    fn transient_record_requires_cooling_after_heating() {
        let mut transaction = sample_transient_thermal_plant_transaction();
        transaction.samples[13].duty_percent = 100;
        transaction.samples[13].heater_voltage_100mv = 200;

        assert!(!thermal_plant_transient_transaction_is_complete(
            &transaction
        ));

        transaction = sample_transient_thermal_plant_transaction();
        for sample in transaction.samples.iter_mut().take(24).skip(12) {
            sample.raw_rtd_adc_mv = 338;
        }
        assert!(!thermal_plant_transient_transaction_is_complete(
            &transaction
        ));
    }

    #[test]
    fn legacy_candidate_tlv_remains_decode_only() {
        let transaction = sample_thermal_plant_transaction();
        let mut transaction_payload = [0u8; 52];
        encode_thermal_plant_raw_transaction(&transaction, &mut transaction_payload);
        let mut payload = [0u8; 64];
        let mut cursor = 0;
        push_tlv(
            TLV_THERMAL_PLANT_CANDIDATE,
            &transaction_payload,
            &mut payload,
            &mut cursor,
        )
        .expect("legacy candidate payload fits");

        let decoded = decode_config_payload(&payload[..cursor], true)
            .expect("legacy candidate payload decodes");

        assert_eq!(decoded.thermal_plant_active, Some(transaction));
        assert_eq!(decoded.thermal_plant_transient_active, None);
    }

    #[test]
    fn invalid_legacy_candidate_tlv_remains_decode_only() {
        let mut transaction = sample_thermal_plant_transaction();
        transaction.anchors[1].target_raw_rtd_adc_mv = transaction.anchors[0].target_raw_rtd_adc_mv;
        let mut transaction_payload = [0u8; 52];
        encode_thermal_plant_raw_transaction(&transaction, &mut transaction_payload);
        let mut payload = [0u8; 64];
        let mut cursor = 0;
        push_tlv(
            TLV_THERMAL_PLANT_CANDIDATE,
            &transaction_payload,
            &mut payload,
            &mut cursor,
        )
        .expect("legacy candidate payload fits");

        let decoded = decode_config_payload(&payload[..cursor], true)
            .expect("legacy candidate payload decodes");

        assert_eq!(decoded.thermal_plant_active, Some(transaction));
        assert!(project_thermal_plant(&transaction, |adc| Some(adc as f32 / 10.0)).is_none());
    }

    #[test]
    fn rtd_reprojection_changes_only_derived_thermal_model() {
        let raw = sample_thermal_plant_transaction();
        let original = project_thermal_plant(&raw, |adc| Some(adc as f32 / 10.0))
            .expect("original projection");
        let recalibrated = project_thermal_plant(&raw, |adc| Some(adc as f32 / 10.5))
            .expect("recalibrated projection");

        assert_eq!(raw, sample_thermal_plant_transaction());
        assert_ne!(
            original.convection_mw_per_c,
            recalibrated.convection_mw_per_c
        );
        assert_ne!(
            original.thermal_capacity_mj_per_c,
            recalibrated.thermal_capacity_mj_per_c
        );
    }
}
