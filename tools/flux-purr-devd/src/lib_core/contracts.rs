pub(crate) use super::*;

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
    pub(crate) fn channel_mut(
        &mut self,
        channel: CalibrationChannel,
    ) -> &mut CalibrationChannelState {
        match channel {
            CalibrationChannel::RtdAdc => &mut self.rtd_adc,
            CalibrationChannel::VinAdc => &mut self.vin_adc,
        }
    }

    pub(crate) fn refresh_fits(&mut self) {
        self.rtd_adc.refresh(CalibrationChannel::RtdAdc);
        self.vin_adc.refresh(CalibrationChannel::VinAdc);
    }
}

impl CalibrationChannelState {
    pub(crate) fn refresh(&mut self, channel: CalibrationChannel) {
        if channel == CalibrationChannel::RtdAdc {
            sanitize_web_facing_rtd_samples(&mut self.samples);
        } else {
            compact_calibration_samples(&mut self.samples);
        }
        self.fitted_fit = fit_calibration_channel(&self.samples, channel);
    }

    pub(crate) fn sanitize_slot_fits(&mut self) {
        sanitize_calibration_slot_fit(&mut self.slots.a);
        sanitize_calibration_slot_fit(&mut self.slots.b);
    }

    pub(crate) fn slot_fit_mut(&mut self, slot: CalibrationSlotId) -> &mut CalibrationSlotFit {
        match slot {
            CalibrationSlotId::A => &mut self.slots.a,
            CalibrationSlotId::B => &mut self.slots.b,
        }
    }
}

pub(crate) fn sanitize_calibration_slot_fit(fit: &mut CalibrationSlotFit) {
    if !fit.gain.is_finite() || fit.gain == 0.0 {
        fit.gain = 1.0;
    }
    if !fit.offset_mv.is_finite() {
        fit.offset_mv = 0.0;
    }
}

pub(crate) fn fit_calibration_channel(
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

pub(crate) fn is_web_facing_calibration_sample(
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

pub(crate) fn sanitize_web_facing_rtd_samples(samples: &mut Vec<Option<CalibrationSample>>) {
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

pub(crate) fn vin_adc_mv_for_input_mv(input_mv: u32) -> u16 {
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
pub(crate) fn deserialize_static_ipv4_patch<'de, D>(
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
    #[serde(default)]
    pub post_heat_cooling_mode: Option<String>,
    #[serde(default)]
    pub heating_fan_guard_mode: Option<String>,
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
pub enum BuzzerTestOp {
    Trigger,
    Run,
    Stop,
    Status,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerTestCue {
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
pub enum BuzzerTestScenario {
    FeedbackCoalesce,
    FeedbackReplace,
    ActiveCoolingRetrigger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestRequest {
    pub lease_id: String,
    pub op: BuzzerTestOp,
    pub cue: Option<BuzzerTestCue>,
    pub scenario: Option<BuzzerTestScenario>,
    #[serde(default)]
    pub repeat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerTestSessionState {
    Idle,
    Running,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestDecision {
    pub source: String,
    pub cue: String,
    pub disposition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestTraceEvent {
    pub elapsed_ms: u32,
    pub decision: BuzzerTestDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestOutputTraceEvent {
    pub elapsed_ms: u32,
    pub requested_frequency_hz: Option<u32>,
    pub applied_frequency_hz: u32,
    #[serde(default)]
    pub observed_frequency_hz: Option<u32>,
    #[serde(default)]
    pub observed_rising_edges: u16,
    #[serde(default)]
    pub observed_window_ms: u32,
    pub duty_percent: u8,
    pub generation: u32,
    pub timer_prescaler: u8,
    pub timer_period_ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestStatus {
    pub state: BuzzerTestSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<BuzzerTestScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue: Option<BuzzerTestCue>,
    #[serde(default)]
    pub repeat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_cue: Option<String>,
    pub trace: Vec<BuzzerTestTraceEvent>,
    #[serde(default)]
    pub output_trace: Vec<BuzzerTestOutputTraceEvent>,
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
    pub(crate) const fn usb_op(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbRequestWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbThermalPlantRunWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) after_sample: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbWifiConfigWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ssid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) static_ipv4: Option<Option<WifiStaticIpv4Request>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbRuntimeConfigWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_temp_c: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selected_preset_slot: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) presets_c: Option<&'a Vec<Option<i16>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active_cooling_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) post_heat_cooling_mode: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) heating_fan_guard_mode: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) heater_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manual_pps_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manual_pps_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) manual_pps_ma: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fault_attention_acknowledged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) calibration: Option<&'a CalibrationControlRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thermal_profile_mode: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thermal_control_profile: Option<&'a ThermalControlProfileRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbBuzzerTestWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: BuzzerTestOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) buzzer_cue: Option<BuzzerTestCue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) buzzer_scenario: Option<BuzzerTestScenario>,
    pub(crate) repeat: bool,
}

#[cfg(test)]
pub(crate) fn encode_usb_runtime_mode_for_test(mode: &String) -> String {
    serde_json::to_string(&UsbRuntimeConfigWire {
        frame_type: "runtime_config",
        request_id: "mode-test",
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
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
pub(crate) struct UsbCalibrationConfigWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: CalibrationConfigOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) channel: Option<CalibrationChannel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reference_temp_c: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reference_vin_mv: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_adc_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) observed_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sample_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) state: Option<&'a CalibrationState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) slot: Option<CalibrationSlotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fit: Option<&'a CalibrationSlotFit>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbHeaterCurveConfigWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: HeaterCurveConfigOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) heater_curve: Option<&'a HeaterCurvePackage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbHeaterCurveSaveWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbCalibrationJobWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: CalibrationJobOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<CalibrationJobKind>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbEepromMaintenanceWire<'a> {
    #[serde(rename = "type")]
    pub(crate) frame_type: &'static str,
    pub(crate) request_id: &'a str,
    pub(crate) op: EepromMaintenanceOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offset: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) length: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<&'a Vec<u8>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsbResponseWire {
    #[serde(rename = "type")]
    pub(crate) frame_type: String,
    pub(crate) request_id: Option<String>,
    pub(crate) ok: Option<bool>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<ApiError>,
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
pub(crate) struct BootObservation {
    pub(crate) reset_count: u8,
    saw_boot_progress: bool,
    pub(crate) last_stage: Option<String>,
}

impl BootObservation {
    pub(crate) fn observe_line(&mut self, line: &str) -> Result<bool, HttpError> {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalFirmwareUpdateRequest {
    pub(crate) port: String,
    pub(crate) artifact_id: String,
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
pub(crate) enum FirmwareOperationPhase {
    Preflight,
    Execution,
}

pub(crate) struct FirmwareOperationProgress {
    state: AppState,
    device_id: String,
    operation_id: String,
    phase: FirmwareOperationPhase,
    operation: FirmwareOperation,
    pub(crate) artifact_id: String,
    sequence: u64,
    active_stage: Option<String>,
}

impl FirmwareOperationProgress {
    pub(crate) fn new(
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

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn emit(&mut self, event_name: &str, stage: Option<&str>, details: Value) {
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

    pub(crate) fn operation_started(&mut self) {
        self.emit("operation_started", None, json!({}));
    }

    pub(crate) fn stage_started(&mut self, stage: &str, details: Value) {
        self.active_stage = Some(stage.to_string());
        self.emit("stage_started", Some(stage), details);
    }

    pub(crate) fn stage_progress(&mut self, stage: &str, details: Value) {
        self.emit("stage_progress", Some(stage), details);
    }

    pub(crate) fn stage_completed(&mut self, stage: &str, details: Value) {
        self.emit("stage_completed", Some(stage), details);
        self.active_stage = None;
    }

    pub(crate) fn stage_failed(&mut self, stage: &str, code: &str) {
        self.emit("stage_failed", Some(stage), json!({ "code": code }));
        self.active_stage = None;
    }

    pub(crate) fn operation_completed(&mut self, outcome: &str) {
        self.emit("operation_completed", None, json!({ "outcome": outcome }));
    }

    pub(crate) fn fail(&mut self, error: HttpError) -> HttpError {
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

    pub(crate) fn require<T>(&mut self, result: Result<T, HttpError>) -> Result<T, HttpError> {
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
    pub(crate) fn validate_for_flash(&self) -> Result<(), HttpError> {
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
    pub(crate) status: StatusCode,
    pub(crate) error: ApiError,
}

impl HttpError {
    pub(crate) fn internal(message: &str) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            message,
            true,
        )
    }

    pub(crate) fn internal_with_details(code: &str, message: &str, details: Value) -> Self {
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

    pub(crate) fn not_found(code: &str, message: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message, false)
    }

    pub(crate) fn bad_request(code: &str, message: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message, false)
    }

    pub(crate) fn forbidden(code: &str, message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message, true)
    }

    pub(crate) fn conflict(code: &str, message: &str, details: Value) -> Self {
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

    pub(crate) fn new(status: StatusCode, code: &str, message: &str, retryable: bool) -> Self {
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
