#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn fusb302b_degraded_reason() -> &'static str {
    match FUSB302B_DIAGNOSTIC.load(Ordering::Relaxed) {
        FUSB302B_DIAG_WAITING_CC_ATTACH => "pd_fusb_cc_attach_pending",
        FUSB302B_DIAG_WAITING_SOURCE_CAPS => "pd_fusb_source_caps_waiting",
        FUSB302B_DIAG_SOURCE_CAPS_REQUESTED => "pd_fusb_source_caps_requested",
        FUSB302B_DIAG_WAITING_ACCEPT => "pd_fusb_accept_pending",
        FUSB302B_DIAG_WAITING_PS_RDY => "pd_fusb_ps_rdy_pending",
        FUSB302B_DIAG_RECOVERING => "pd_fusb_phy_recovering",
        FUSB302B_DIAG_FAULT => "pd_fusb_phy_fault",
        FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED => "pd_fusb_source_caps_tx_confirmed",
        FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN => "pd_fusb_source_caps_gcrc_seen",
        FUSB302B_DIAG_PROTECTION => "pd_fusb_phy_protection",
        FUSB302B_DIAG_MISSING_CRC => "pd_fusb_rx_crc_missing",
        FUSB302B_DIAG_MISSING_SOP => "pd_fusb_rx_sop_missing",
        FUSB302B_DIAG_UNSUPPORTED_SOP => "pd_fusb_rx_sop_unsupported",
        FUSB302B_DIAG_RX_I2C_ERROR => "pd_fusb_rx_i2c_error",
        FUSB302B_DIAG_TX_I2C_ERROR => "pd_fusb_tx_i2c_error",
        FUSB302B_DIAG_NO_USABLE_CONTRACT => "pd_fusb_no_usable_contract",
        FUSB302B_DIAG_RX_PARTIAL => "pd_fusb_rx_partial",
        FUSB302B_DIAG_REQUEST_TIMEOUT => "pd_fusb_contract_request_timeout",
        _ => "pd_contract_unavailable",
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn adc_diagnostics_wire() -> AdcDiagnosticsWire {
    let optional_code = |value: u16| (value != u16::MAX).then_some(value);
    let raw_min = RTD_RAW_CODE_MIN.load(Ordering::Relaxed);
    let raw_max = RTD_RAW_CODE_MAX.load(Ordering::Relaxed);
    AdcDiagnosticsWire {
        calibration_source: match ADC_CALIBRATION_SOURCE.load(Ordering::Relaxed) {
            0 => AdcCalibrationSourceWire::Efuse,
            1 => AdcCalibrationSourceWire::RuntimeFallback,
            _ => AdcCalibrationSourceWire::Unavailable,
        },
        efuse_version: ADC_EFUSE_VERSION.load(Ordering::Relaxed),
        attenuation_db: 6,
        init_code: optional_code(ADC_INIT_CODE.load(Ordering::Relaxed)),
        reference_code: optional_code(ADC_REFERENCE_CODE.load(Ordering::Relaxed)),
        reference_mv: optional_code(ADC_REFERENCE_MV.load(Ordering::Relaxed)),
        rtd_raw_code_mean: RTD_RAW_CODE_MEAN.load(Ordering::Relaxed),
        rtd_raw_code_min: raw_min,
        rtd_raw_code_max: raw_max,
        rtd_raw_code_spread: raw_max.saturating_sub(raw_min),
        vin_raw_code_mean: VIN_RAW_CODE_MEAN.load(Ordering::Relaxed),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum RtdSample {
    Valid(RtdMeasurement),
    Fault {
        adc_mv: Option<u16>,
        reason: HeaterFaultReason,
    },
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PdStatusObservation {
    status_raw: u8,
    status: Status,
    current_raw: u8,
    current_ma: u16,
    contract_voltage_mv: Option<u16>,
    contract: Contract,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PdStatusLogKey {
    status_raw: u8,
    pd_active: bool,
    epr_active: bool,
    epr_exist: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
fn pd_status_log_key(observation: Option<PdStatusObservation>) -> Option<PdStatusLogKey> {
    observation.map(|observation| PdStatusLogKey {
        status_raw: observation.status_raw,
        pd_active: observation.status.pd_active,
        epr_active: observation.status.epr_active,
        epr_exist: observation.status.epr_exist,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn pd_contract_allows_calibration(
    controller: ControllerKind,
    observation: Option<PdStatusObservation>,
) -> bool {
    match controller {
        ControllerKind::Fusb302b => observation.is_some_and(|observation| {
            observation.contract.kind == ContractKind::Pps
                && observation.contract.performance_guaranteed()
        }),
        // CH224Q retains its established PPS/AVS calibration policy.
        ControllerKind::Ch224q => true,
        ControllerKind::Unknown => false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaterPowerBackendReason {
    PpsCovers20v,
    NoPps20vCapability,
    CapabilityReadFailed,
    AdjustableRequestFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackendReason {
    const fn label(self) -> &'static str {
        match self {
            Self::PpsCovers20v => "pps-covers-20v",
            Self::NoPps20vCapability => "no-pps-20v-capability",
            Self::CapabilityReadFailed => "capability-read-failed",
            Self::AdjustableRequestFailed => "adjustable-request-failed",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaterPowerBackend {
    PpsMos {
        pps_min_mv: u16,
        idle_request_mv: u16,
        pps_max_mv: u16,
        adjustable_max_mv: u16,
        capability_max_ma: u16,
        current_mode: Option<ch224q::AdjustableVoltageMode>,
        current_request_mv: u16,
        settle_until_ms: Option<u64>,
        next_request_at_ms: u64,
        current_limit_fixed_pwm_active: bool,
        current_limit_fixed_request_confirmed: bool,
        // A terminal thermal-calibration transition has requested fixed PD. Keep
        // the regular PPS governor dormant until a later, explicit re-arm.
        terminal_fixed_pd_disarmed: bool,
    },
    FixedPdPwmFallback {
        reason: HeaterPowerBackendReason,
        fixed_request_confirmed: bool,
        fixed_request: ch224q::VoltageRequest,
        terminal_fixed_pd_disarmed: bool,
    },
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoldPpsGovernor {
    active: bool,
    next_adjust_at_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
struct HoldPpsRequestInput {
    phase: HeaterControlPhase,
    duty_percent: u8,
    actual_error_c: f32,
    filtered_slope_c_per_s: f32,
    current_request_mv: u16,
    control_floor_mv: u16,
    safe_max_mv: u16,
    now_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HoldPpsGovernor {
    const fn new() -> Self {
        Self {
            active: false,
            next_adjust_at_ms: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn step_request_into_bounds(
        current_request_mv: u16,
        control_floor_mv: u16,
        safe_max_mv: u16,
    ) -> u16 {
        let bounded_safe_max_mv = safe_max_mv.max(control_floor_mv);
        if current_request_mv < control_floor_mv {
            current_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(control_floor_mv)
        } else if current_request_mv > bounded_safe_max_mv {
            current_request_mv
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(bounded_safe_max_mv)
        } else {
            current_request_mv
        }
    }

    fn request_mv(&mut self, input: HoldPpsRequestInput) -> Option<u16> {
        let HoldPpsRequestInput {
            phase,
            duty_percent,
            actual_error_c,
            filtered_slope_c_per_s,
            current_request_mv,
            control_floor_mv,
            safe_max_mv,
            now_ms,
        } = input;
        // The raw measurement can briefly leave Hold at high temperature while
        // still inside the control loop's near-target band.  Keep PPS headroom
        // adaptation alive through that Approach phase; otherwise its settle
        // timer restarts before a saturated heater can recover to Hold.
        if !matches!(
            phase,
            HeaterControlPhase::Hold | HeaterControlPhase::Approach
        ) {
            self.reset();
            return None;
        }

        // A nonzero command below the working floor is handled by the fixed-PWM safety
        // fallback before this governor. Keep the idle path well-defined too.
        let bounded_safe_max_mv = safe_max_mv.max(control_floor_mv);
        let bounded_request_mv = Self::step_request_into_bounds(
            current_request_mv,
            control_floor_mv,
            bounded_safe_max_mv,
        );
        if !self.active {
            self.active = true;
            // Hold inherits the Approach voltage. PWM handles fast corrections; PPS only
            // rises later if that voltage cannot provide enough heating headroom.
            self.next_adjust_at_ms = now_ms.saturating_add(HEATER_HOLD_PPS_INITIAL_SETTLE_MS);
            return Some(bounded_request_mv);
        }
        if duty_percent == 0 || now_ms < self.next_adjust_at_ms {
            return Some(bounded_request_mv);
        }

        let physical_pwm_percent =
            heater_physical_pwm_percent(duty_percent, bounded_safe_max_mv, bounded_request_mv, 100);
        let raise_voltage = physical_pwm_percent >= HEATER_HOLD_PPS_SATURATION_PWM_MIN_PERCENT
            && actual_error_c >= HEATER_HOLD_PPS_RAISE_ERROR_MIN_C
            && filtered_slope_c_per_s <= HEATER_HOLD_PPS_RAISE_MAX_SLOPE_C_PER_S;
        let next_request_mv = if raise_voltage {
            bounded_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(bounded_safe_max_mv)
        } else {
            bounded_request_mv
        };

        self.next_adjust_at_ms = now_ms.saturating_add(HEATER_HOLD_PPS_STEADY_DWELL_MS);
        Some(next_request_mv)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualPpsError {
    NoPpsCapability,
    InvalidVoltage,
    CalibrationInProgress,
    TerminalDisarmPending,
    ThermalPlantManagedByJob,
    HeaterCurveCoverageInsufficient,
    ThermalPlantSourceUnsupported,
    ThermalPlantProjectionInvalid,
    PdNotReady,
    WriteFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualPpsOwner {
    Debug,
    Calibration,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ManualPpsError {
    const fn code(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "manual_pps_no_capability",
            Self::InvalidVoltage => "manual_pps_invalid_voltage",
            Self::CalibrationInProgress => "manual_pps_calibration_busy",
            Self::TerminalDisarmPending => "heater_disarm_pending",
            Self::ThermalPlantManagedByJob => "thermal_plant_managed_by_job",
            Self::HeaterCurveCoverageInsufficient => "heater_curve_coverage_insufficient",
            Self::ThermalPlantSourceUnsupported => "thermal_plant_source_unsupported",
            Self::ThermalPlantProjectionInvalid => "thermal_plant_projection_invalid",
            Self::PdNotReady => "manual_pps_pd_not_ready",
            Self::WriteFailed => "manual_pps_write_failed",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "PPS capability is unavailable.",
            Self::InvalidVoltage => {
                "manualPpsMv/manualPpsMa must match PPS capability and APDO steps."
            }
            Self::CalibrationInProgress => {
                "Manual PPS cannot override a running thermal-model calibration."
            }
            Self::TerminalDisarmPending => {
                "The previous heater session is still being physically disarmed."
            }
            Self::ThermalPlantManagedByJob => {
                "Automatic thermal-model calibration is managed by thermal_plant_auto."
            }
            Self::HeaterCurveCoverageInsufficient => {
                "The transient thermal-model run did not collect enough heater-curve samples."
            }
            Self::ThermalPlantSourceUnsupported => {
                "Thermal plant calibration requires a PPS APDO covering 20V at 3A or more."
            }
            Self::ThermalPlantProjectionInvalid => {
                "Thermal plant observations did not produce a physical model."
            }
            Self::PdNotReady => "PD contract is not ready for manual PPS.",
            Self::WriteFailed => "Manual PPS write failed.",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualPpsState {
    enabled: bool,
    owner: ManualPpsOwner,
    request_min_mv: u16,
    request_max_mv: u16,
    target_mv: Option<u16>,
    target_ma: Option<u16>,
    applied_mv: Option<u16>,
    capability_min_mv: Option<u16>,
    capability_max_mv: Option<u16>,
    capability_max_ma: Option<u16>,
    capability_apdos: [Option<ch224q::PpsApdo>; ch224q::MAX_PPS_APDOS],
    error: Option<ManualPpsError>,
    automatic_restore_pending: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for ManualPpsState {
    fn default() -> Self {
        Self {
            enabled: false,
            owner: ManualPpsOwner::Debug,
            request_min_mv: CH224Q_ADJUSTABLE_REQUEST_MIN_MV,
            request_max_mv: ch224q::CH224Q_PPS_MAX_MV,
            target_mv: None,
            target_ma: None,
            applied_mv: None,
            capability_min_mv: None,
            capability_max_mv: None,
            capability_max_ma: None,
            capability_apdos: [None; ch224q::MAX_PPS_APDOS],
            error: None,
            automatic_restore_pending: false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationMode {
    Off,
    VinAdc,
    RtdAdc,
    HeaterCurve,
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationMode {
    const fn to_wire(self) -> CalibrationModeWire {
        match self {
            Self::Off => CalibrationModeWire::Off,
            Self::VinAdc => CalibrationModeWire::VinAdc,
            Self::RtdAdc => CalibrationModeWire::RtdAdc,
            Self::HeaterCurve => CalibrationModeWire::HeaterCurve,
            Self::ThermalPlant => CalibrationModeWire::ThermalPlant,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<CalibrationModeWire> for CalibrationMode {
    fn from(value: CalibrationModeWire) -> Self {
        match value {
            CalibrationModeWire::Off => Self::Off,
            CalibrationModeWire::VinAdc => Self::VinAdc,
            CalibrationModeWire::RtdAdc => Self::RtdAdc,
            CalibrationModeWire::HeaterCurve => Self::HeaterCurve,
            CalibrationModeWire::ThermalPlant => Self::ThermalPlant,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationJobKind {
    VinAdc,
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationJobKind {
    const fn to_wire(self) -> CalibrationJobKindWire {
        match self {
            Self::VinAdc => CalibrationJobKindWire::VinAdcAuto,
            Self::ThermalPlant => CalibrationJobKindWire::ThermalPlantAuto,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<CalibrationJobKindWire> for CalibrationJobKind {
    fn from(value: CalibrationJobKindWire) -> Self {
        match value {
            CalibrationJobKindWire::VinAdcAuto => Self::VinAdc,
            CalibrationJobKindWire::ThermalPlantAuto => Self::ThermalPlant,
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationJobStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationJobStatus {
    const fn to_wire(self) -> CalibrationJobStatusWire {
        match self {
            Self::Idle => CalibrationJobStatusWire::Idle,
            Self::Running => CalibrationJobStatusWire::Running,
            Self::Completed => CalibrationJobStatusWire::Completed,
            Self::Failed => CalibrationJobStatusWire::Failed,
            Self::Canceled => CalibrationJobStatusWire::Canceled,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CalibrationJobState {
    kind: Option<CalibrationJobKind>,
    status: CalibrationJobStatus,
    progress_percent: u8,
    samples_collected: u8,
    next_request_mv: Option<u16>,
    message: Option<ManualPpsError>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for CalibrationJobState {
    fn default() -> Self {
        Self {
            kind: None,
            status: CalibrationJobStatus::Idle,
            progress_percent: 0,
            samples_collected: 0,
            next_request_mv: None,
            message: None,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
const CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES: usize = 24;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN: u16 = 20;
#[cfg(any(target_arch = "xtensa", test))]
const CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_AMBIENT_TICKS: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_HEAT_TIMEOUT_TICKS: u32 = 24_000;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_COOL_TIMEOUT_TICKS: u32 = 24_000;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_TARGET_TEMP_C: f32 = 220.0;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_COOL_COMPLETE_TEMP_C: f32 = 80.0;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C: f32 = 4.0;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalibrationVinAutoJob {
    start_request_mv: u16,
    next_request_mv: u16,
    max_request_mv: u16,
    target_ma: u16,
    settle_ticks: u8,
    stable_ticks: u8,
    last_observed_mv: Option<u16>,
    sample_count: u8,
    samples: [Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalPlantCurveBin {
    min_temp_c: f32,
    max_temp_c: f32,
    samples: u16,
    temp_sum_c: f32,
    resistance_sum_ohms: f32,
    raw_adc_sum_mv: u32,
    voltage_sum_mv: u64,
    current_sum_ma: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ThermalPlantCurveBin {
    const fn new(min_temp_c: f32, max_temp_c: f32) -> Self {
        Self {
            min_temp_c,
            max_temp_c,
            samples: 0,
            temp_sum_c: 0.0,
            resistance_sum_ohms: 0.0,
            raw_adc_sum_mv: 0,
            voltage_sum_mv: 0,
            current_sum_ma: 0,
        }
    }

    fn contains(self, temp_c: f32) -> bool {
        temp_c >= self.min_temp_c && temp_c < self.max_temp_c
    }

    fn observe(&mut self, temp_c: f32, resistance_ohms: f32) {
        self.samples = self.samples.saturating_add(1);
        self.temp_sum_c += temp_c;
        self.resistance_sum_ohms += resistance_ohms;
    }

    fn observe_electrical(
        &mut self,
        temp_c: f32,
        raw_rtd_adc_mv: u16,
        heater_voltage_mv: u32,
        heater_current_ma: u16,
    ) {
        self.observe(
            temp_c,
            heater_voltage_mv as f32 / f32::from(heater_current_ma),
        );
        self.raw_adc_sum_mv = self
            .raw_adc_sum_mv
            .saturating_add(u32::from(raw_rtd_adc_mv));
        self.voltage_sum_mv = self
            .voltage_sum_mv
            .saturating_add(u64::from(heater_voltage_mv));
        self.current_sum_ma = self
            .current_sum_ma
            .saturating_add(u64::from(heater_current_ma));
    }

    fn averaged_raw_observation(self) -> Option<HeaterCurveRawObservation> {
        if self.samples == 0 || self.raw_adc_sum_mv == 0 || self.current_sum_ma == 0 {
            return None;
        }
        let samples = u64::from(self.samples);
        let voltage_mv = (self.voltage_sum_mv / samples).min(u64::from(u16::MAX)) as u16;
        let current_ma = (self.current_sum_ma / samples).min(u64::from(u16::MAX)) as u16;
        Some(HeaterCurveRawObservation {
            raw_rtd_adc_mv: (u64::from(self.raw_adc_sum_mv) / samples).min(u64::from(u16::MAX))
                as u16,
            heater_voltage_mv: voltage_mv,
            heater_current_ma: current_ma,
            resistance_milliohms: ((u32::from(voltage_mv) * 1_000) / u32::from(current_ma))
                .min(u32::from(u16::MAX)) as u16,
        })
    }

    fn averaged_point(self) -> Option<(i16, u16)> {
        if self.samples == 0 {
            return None;
        }
        let temp_c = self.temp_sum_c / f32::from(self.samples);
        let measured_resistance_ohms = self.resistance_sum_ohms / f32::from(self.samples);
        let resistance_ohms =
            measured_resistance_ohms.max(default_estimated_heater_resistance_ohms(temp_c));
        let temp_centi_c = round_to_i16(temp_c * 100.0);
        let resistance_milliohms = round_to_u16_nonnegative(resistance_ohms * 1000.0);
        Some((temp_centi_c, resistance_milliohms))
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn round_to_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    let rounded = if value >= 0.0 {
        value + 0.5
    } else {
        value - 0.5
    };
    rounded.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
fn round_to_u16_nonnegative(value: f32) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    (value + 0.5).clamp(0.0, u16::MAX as f32) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalPlantCurveSampler {
    cold_bin: ThermalPlantCurveBin,
    bins: [ThermalPlantCurveBin; 4],
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum ThermalPlantAutoPhase {
    Ambient,
    Heating,
    Cooling,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug)]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
struct CalibrationThermalPlantAutoJob {
    run_id: u32,
    phase: ThermalPlantAutoPhase,
    source_max_mv: u16,
    source_current_ma: u16,
    ambient_raw_rtd_adc_mv: u16,
    idle_samples: u16,
    heater_curve: ThermalPlantCurveSampler,
    elapsed_ticks: u32,
    phase_started_tick: u32,
    sample_count: u8,
    last_saved_temp_c: f32,
    last_saved_tick: u16,
    samples: [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
}

// Keep the bounded transient trace out of the Embassy main task. The job is
// large enough that retaining it in CalibrationRuntimeState exhausts the
// application stack during Wi-Fi initialization.
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug, Default)]
struct CalibrationThermalPlantWorkspace {
    job: Option<CalibrationThermalPlantAutoJob>,
    next_run_id: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for ThermalPlantCurveSampler {
    fn default() -> Self {
        Self {
            cold_bin: ThermalPlantCurveBin::new(0.0, 80.0),
            bins: [
                ThermalPlantCurveBin::new(80.0, 120.0),
                ThermalPlantCurveBin::new(120.0, 160.0),
                ThermalPlantCurveBin::new(160.0, 190.0),
                ThermalPlantCurveBin::new(190.0, 221.0),
            ],
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
enum CalibrationJobData {
    VinAdc(CalibrationVinAutoJob),
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterCurvePreview {
    curve: HeaterCurveConfig,
    raw_observations: Option<HeaterCurveRawObservations>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn preview_heater_curve_config(preview: Option<&HeaterCurvePreview>) -> Option<&HeaterCurveConfig> {
    preview.map(|preview| &preview.curve)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalibrationRuntimeState {
    mode: CalibrationMode,
    pps_enabled: bool,
    pps_mv: Option<u16>,
    pps_ma: Option<u16>,
    heater_enabled: bool,
    target_adc_mv: Option<u16>,
    stable: bool,
    stability_error_mv: Option<i16>,
    error: Option<ManualPpsError>,
    job: CalibrationJobState,
    job_data: Option<CalibrationJobData>,
    model_target_temp_c: Option<i16>,
    thermal_plant_completion_disarm_pending: bool,
    immediate_heater_disarm_pending: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for CalibrationRuntimeState {
    fn default() -> Self {
        Self {
            mode: CalibrationMode::Off,
            pps_enabled: false,
            pps_mv: None,
            pps_ma: None,
            heater_enabled: false,
            target_adc_mv: None,
            stable: false,
            stability_error_mv: None,
            error: None,
            job: CalibrationJobState::default(),
            job_data: None,
            model_target_temp_c: None,
            thermal_plant_completion_disarm_pending: false,
            immediate_heater_disarm_pending: false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_runtime_state_to_wire(
    state: &CalibrationRuntimeState,
) -> CalibrationRuntimeStateWire {
    CalibrationRuntimeStateWire {
        mode: state.mode.to_wire(),
        pps_enabled: state.pps_enabled,
        pps_mv: state.pps_mv,
        pps_ma: state.pps_ma,
        heater_enabled: state.heater_enabled,
        target_adc_mv: state.target_adc_mv,
        stable: state.stable,
        stability_error_mv: state.stability_error_mv,
        error: state.error.map(manual_pps_error_code),
        job: CalibrationJobStateWire {
            kind: state.job.kind.map(CalibrationJobKind::to_wire),
            status: state.job.status.to_wire(),
            progress_percent: state.job.progress_percent,
            samples_collected: state.job.samples_collected,
            next_request_mv: state.job.next_request_mv,
            message: state.job.message.or(state.error).map(|error| {
                let mut out = heapless::String::new();
                let _ = out.push_str(error.message());
                out
            }),
        },
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn reconcile_runtime_heater_enabled(
    current_heater_enabled: bool,
    calibration_runtime_state: impl core::borrow::Borrow<CalibrationRuntimeState>,
    current_rtd_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    heater_fault_latched: bool,
    thermal_model_heater_allowed: bool,
    pd_contract_ready: bool,
) -> bool {
    let calibration_runtime_state = calibration_runtime_state.borrow();
    if calibration_runtime_state.mode == CalibrationMode::Off {
        return current_heater_enabled && thermal_model_heater_allowed && pd_contract_ready;
    }

    let calibration_heater_allowed = !is_sensor_fault(current_rtd_fault)
        && !cooling_disabled_lock_latched
        && !heater_fault_latched
        && pd_contract_ready;
    if calibration_runtime_state.mode == CalibrationMode::ThermalPlant
        && calibration_runtime_state.job.status != CalibrationJobStatus::Running
    {
        return false;
    }
    calibration_runtime_state.heater_enabled && calibration_heater_allowed
}

#[cfg(any(target_arch = "xtensa", test))]
fn disarm_stale_heater_arm_after_pd_transition(
    previous_pd_ready: bool,
    current_pd_ready: bool,
    ui_state: &mut FrontPanelUiState,
    calibration_runtime_state: &mut CalibrationRuntimeState,
) -> bool {
    if previous_pd_ready || !current_pd_ready {
        return false;
    }

    let was_armed = ui_state.heater_enabled || calibration_runtime_state.heater_enabled;
    ui_state.heater_enabled = false;
    calibration_runtime_state.heater_enabled = false;
    was_armed
}

#[cfg(target_arch = "xtensa")]
fn apply_pd_contract_observation<PWM>(
    observation: Option<PdStatusObservation>,
    pd_contract_ready: &mut bool,
    ui_state: &mut FrontPanelUiState,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    manual_pps_state: &mut ManualPpsState,
    heater_pwm: &mut PWM,
    last_heater_duty: &mut u8,
) -> bool
where
    PWM: SetDutyCycle,
{
    let current_pd_contract_ready = startup_pd_contract_ready(observation);
    let mut needs_redraw = false;
    if *pd_contract_ready != current_pd_contract_ready {
        let pd_was_ready = *pd_contract_ready;
        *pd_contract_ready = current_pd_contract_ready;
        needs_redraw = true;
        if current_pd_contract_ready {
            if disarm_stale_heater_arm_after_pd_transition(
                pd_was_ready,
                current_pd_contract_ready,
                ui_state,
                calibration_runtime_state,
            ) {
                info!(
                    "PD contract became ready; discarded pre-ready heater arm and require a new explicit arm"
                );
            }
            info!("PD contract became ready; released startup heater interlock");
        } else {
            info!("PD contract became unavailable; heater interlocked");
        }
    }

    if !current_pd_contract_ready
        && (ui_state.heater_enabled
            || calibration_runtime_state.heater_enabled
            || *last_heater_duty != 0
            || ui_state.heater_output_percent != 0)
    {
        // A stale observation must never leave GPIO47 powered while the
        // protocol state is unavailable. The next explicit arm is required
        // after a later contract recovery.
        ui_state.heater_enabled = false;
        ui_state.heater_output_percent = 0;
        calibration_runtime_state.heater_enabled = false;
        manual_pps_state.fail(ManualPpsError::PdNotReady);
        apply_heater_duty(heater_pwm, 0, last_heater_duty);
        needs_redraw = true;
        info!("PD contract interlock -> heater output zero");
    }

    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_STALE_CONTRACT_VIN_CONFIRM_MS: u64 = 100;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_STALE_CONTRACT_VIN_DEFICIT_MV: u32 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
const FUSB302B_STALE_CONTRACT_VIN_SETTLE_GRACE_MS: u64 = 500;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PdContractVinGuard {
    mismatch_since_ms: Option<u64>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl PdContractVinGuard {
    fn observe(
        &mut self,
        observation: Option<PdStatusObservation>,
        measured_vin_mv: Option<u32>,
        now_ms: u64,
        suspended: bool,
    ) -> bool {
        if suspended {
            self.mismatch_since_ms = None;
            return false;
        }
        let mismatch = match (observation, measured_vin_mv) {
            (Some(observation), Some(measured_vin_mv))
                if observation.contract != Contract::none() =>
            {
                u32::from(observation.contract.voltage_mv).saturating_sub(measured_vin_mv)
                    >= FUSB302B_STALE_CONTRACT_VIN_DEFICIT_MV
            }
            _ => false,
        };
        if !mismatch {
            self.mismatch_since_ms = None;
            return false;
        }

        let started = *self.mismatch_since_ms.get_or_insert(now_ms);
        now_ms.saturating_sub(started) >= FUSB302B_STALE_CONTRACT_VIN_CONFIRM_MS
    }
}

#[cfg(target_arch = "xtensa")]
struct PdContractVinContext<'a, PWM> {
    pd_port: &'a mut PdPort,
    last_pd_observation: &'a mut Option<PdStatusObservation>,
    pd_contract_ready: &'a mut bool,
    ui_state: &'a mut FrontPanelUiState,
    calibration_runtime_state: &'a mut CalibrationRuntimeState,
    manual_pps_state: &'a mut ManualPpsState,
    heater_pwm: &'a mut PWM,
    last_heater_duty: &'a mut u8,
}

#[cfg(target_arch = "xtensa")]
fn reconcile_pd_contract_with_vin<PWM>(
    guard: &mut PdContractVinGuard,
    observation: Option<PdStatusObservation>,
    measured_vin_mv: Option<u32>,
    now_ms: u64,
    context: PdContractVinContext<'_, PWM>,
) -> bool
where
    PWM: SetDutyCycle,
{
    let PdContractVinContext {
        pd_port,
        last_pd_observation,
        pd_contract_ready,
        ui_state,
        calibration_runtime_state,
        manual_pps_state,
        heater_pwm,
        last_heater_duty,
    } = context;
    if !guard.observe(
        observation,
        measured_vin_mv,
        now_ms,
        pd_port.stale_contract_vin_guard_suspended(now_ms),
    ) {
        return false;
    }

    // A measured VIN deficit invalidates only the cached contract. The
    // controller remains attached and will rediscover capabilities through the
    // existing bounded policy path; no VBUS state is inferred across power
    // loss, and no CC toggle is started here.
    pd_port.interlock_after_stale_contract(now_ms);
    *last_pd_observation = None;
    apply_pd_contract_observation(
        None,
        pd_contract_ready,
        ui_state,
        calibration_runtime_state,
        manual_pps_state,
        heater_pwm,
        last_heater_duty,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_calibration_snapshot(
    measured_temp_c: f32,
    heater_enabled: bool,
) -> HeaterPidSnapshot {
    HeaterPidSnapshot {
        duty_percent: u8::from(heater_enabled) * 100,
        warmup_soft_start_percent: 100,
        error_c: THERMAL_PLANT_TARGET_TEMP_C - measured_temp_c,
        control_error_c: THERMAL_PLANT_TARGET_TEMP_C - measured_temp_c,
        filtered_temp_c: measured_temp_c,
        filtered_slope_c_per_s: 0.0,
        coast_active: false,
        phase: HeaterControlPhase::Warmup,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_calibration_temperature_c(
    calibration: CalibrationRuntimeState,
    live_rtd_temp_c: Option<f32>,
    control_temp_c: f32,
) -> f32 {
    if thermal_plant_calibration_job_running(calibration) {
        live_rtd_temp_c.unwrap_or(control_temp_c)
    } else {
        control_temp_c
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_cooling_complete(live_temp_c: f32, recorded_temp_c: f32) -> bool {
    live_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
        && recorded_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_output_must_be_off(
    calibration: CalibrationRuntimeState,
    was_running: bool,
    measured_temp_c: f32,
) -> bool {
    if !was_running || calibration.job.kind != Some(CalibrationJobKind::ThermalPlant) {
        return false;
    }
    measured_temp_c >= THERMAL_PLANT_TARGET_TEMP_C
        || calibration.job.status != CalibrationJobStatus::Running
}

#[cfg(any(target_arch = "xtensa", test))]
fn consume_thermal_plant_completion_disarm(
    calibration_runtime_state: &mut CalibrationRuntimeState,
    desired_heater_enabled: bool,
) -> bool {
    if calibration_runtime_state.thermal_plant_completion_disarm_pending {
        calibration_runtime_state.thermal_plant_completion_disarm_pending = false;
        false
    } else {
        desired_heater_enabled
    }
}

#[cfg(test)]
fn take_immediate_heater_disarm(calibration_runtime_state: &mut CalibrationRuntimeState) -> bool {
    let pending = calibration_runtime_state.immediate_heater_disarm_pending;
    calibration_runtime_state.immediate_heater_disarm_pending = false;
    pending
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_model_heater_allowed(
    memory_config: &MemoryConfig,
    calibration: impl core::borrow::Borrow<CalibrationRuntimeState>,
    manual_pps: ManualPpsState,
) -> bool {
    let calibration = calibration.borrow();
    if calibration.mode == CalibrationMode::ThermalPlant {
        return calibration.job.status == CalibrationJobStatus::Running;
    }
    if calibration.mode != CalibrationMode::Off {
        return true;
    }
    if memory_config.commissioning_required {
        return false;
    }
    let plant_is_available =
        memory_config
            .thermal_plant_transient_active
            .is_some_and(|transaction| {
                thermal_plant_projection_from_transient(&transaction).is_some()
                    && thermal_plant_transient_trace_reaches_targets(&transaction, memory_config)
                    && thermal_plant_curve_is_bound(memory_config, transaction)
            });
    if !plant_is_available {
        return false;
    }

    manual_pps.has_matching_pps_apdo(20_000, 3_000)
        && has_persisted_heater_resistance_curve(memory_config)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_curve_is_bound(
    memory_config: &MemoryConfig,
    transaction: ThermalPlantTransientTransaction,
) -> bool {
    memory_config.heater_curve_transaction_id == Some(transaction.transaction_id)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_transient_trace_reaches_targets(
    transaction: &ThermalPlantTransientTransaction,
    memory_config: &MemoryConfig,
) -> bool {
    if !flux_purr_firmware::memory::thermal_plant_transient_transaction_is_complete(transaction) {
        return false;
    }
    let count = usize::from(transaction.sample_count);
    if !(24..=THERMAL_PLANT_TRANSIENT_MAX_SAMPLES).contains(&count) {
        return false;
    }

    let Some(ambient_temp_c) =
        projected_rtd_temperature_c(memory_config, transaction.ambient_raw_rtd_adc_mv)
    else {
        return false;
    };
    let samples = &transaction.samples[..count];
    let mut temperatures_c = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powered_max_temp_c = f32::MIN;
    let mut powered_peak_index = None;
    for (index, sample) in samples.iter().enumerate() {
        let Some(temperature_c) = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)
        else {
            return false;
        };
        if !temperature_c.is_finite() {
            return false;
        }
        if sample.duty_percent > 0 && temperature_c > powered_max_temp_c {
            powered_max_temp_c = temperature_c;
            powered_peak_index = Some(index);
        }
        temperatures_c[index] = temperature_c;
    }

    let Some(powered_peak_index) = powered_peak_index else {
        return false;
    };
    let final_sample = samples.last().copied();
    let final_temp_c = temperatures_c[count - 1];
    powered_max_temp_c >= THERMAL_PLANT_TARGET_TEMP_C
        && (temperatures_c[0] - ambient_temp_c).abs() <= 8.0
        && powered_peak_index + 1 < count
        && final_sample.is_some_and(|sample| sample.duty_percent == 0)
        && final_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl ManualPpsState {
    fn from_capabilities(capabilities: Option<ch224q::AdjustablePowerCapabilities>) -> Self {
        Self::from_capabilities_with_request_bounds(
            capabilities,
            CH224Q_ADJUSTABLE_REQUEST_MIN_MV,
            ch224q::CH224Q_PPS_MAX_MV,
        )
    }

    fn from_fusb302b_capabilities(
        capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    ) -> Self {
        Self::from_capabilities_with_request_bounds(
            capabilities,
            FUSB302B_PPS_MIN_MV,
            FUSB302B_PPS_MAX_MV,
        )
    }

    fn from_capabilities_with_request_bounds(
        capabilities: Option<ch224q::AdjustablePowerCapabilities>,
        request_min_mv: u16,
        request_max_mv: u16,
    ) -> Self {
        let mut state = Self {
            request_min_mv,
            request_max_mv,
            ..Self::default()
        };
        let Some(capabilities) = capabilities else {
            return state;
        };
        let Some((min_mv, max_mv, max_ma)) = capabilities
            .pps_min_mv
            .zip(capabilities.pps_max_mv)
            .zip(capabilities.pps_max_ma)
            .map(|((min_mv, max_mv), max_ma)| (min_mv, max_mv, max_ma))
        else {
            return state;
        };
        let bounded_min_mv = min_mv.max(request_min_mv);
        let bounded_max_mv = max_mv.min(request_max_mv);
        if bounded_min_mv > bounded_max_mv || max_ma == 0 {
            return state;
        }
        state.capability_min_mv = Some(bounded_min_mv);
        state.capability_max_mv = Some(bounded_max_mv);
        state.capability_max_ma = Some(max_ma);
        state.capability_apdos = capabilities.pps_apdos;
        if state.capability_apdos.iter().all(Option::is_none) {
            state.capability_apdos[0] = Some(ch224q::PpsApdo {
                min_mv: bounded_min_mv,
                max_mv,
                max_ma,
            });
        }
        state
    }

    fn validate_target(&self, target_mv: u16, target_ma: u16) -> Result<(), ManualPpsError> {
        let (Some(min_mv), Some(max_mv), Some(_max_ma)) = (
            self.capability_min_mv,
            self.capability_max_mv,
            self.capability_max_ma,
        ) else {
            return Err(ManualPpsError::NoPpsCapability);
        };
        if target_mv < min_mv
            || target_mv > max_mv
            || !target_mv.is_multiple_of(100)
            || target_ma == 0
            || !target_ma.is_multiple_of(50)
            || !self.has_matching_pps_apdo(target_mv, target_ma)
        {
            return Err(ManualPpsError::InvalidVoltage);
        }
        Ok(())
    }

    fn has_matching_pps_apdo(&self, target_mv: u16, target_ma: u16) -> bool {
        self.capability_apdos.iter().flatten().any(|apdo| {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            target_mv >= min_mv && target_mv <= max_mv && target_ma <= apdo.max_ma
        })
    }

    fn maximum_pps_current_for_target(&self, target_mv: u16) -> Option<u16> {
        self.capability_apdos
            .iter()
            .flatten()
            .filter_map(|apdo| {
                let min_mv = apdo.min_mv.max(self.request_min_mv);
                let max_mv = apdo.max_mv.min(self.request_max_mv);
                (target_mv >= min_mv && target_mv <= max_mv).then_some(apdo.max_ma)
            })
            .max()
    }

    fn thermal_plant_source_limits(&self) -> Option<(u16, u16, u16)> {
        self.single_pps_source_limits(20_000)
    }

    fn heater_source_limits(&self) -> Option<(u16, u16, u16)> {
        self.contiguous_pps_source_limits(HEATER_ADJUSTABLE_MIN_MV)
    }

    fn single_pps_source_limits(&self, anchor_mv: u16) -> Option<(u16, u16, u16)> {
        let mut best = None;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            if min_mv > anchor_mv || max_mv < anchor_mv || apdo.max_ma < MIN_HEATER_CONTRACT_MA {
                continue;
            }
            let candidate = (min_mv, max_mv, apdo.max_ma.min(MAX_HEATER_CONTRACT_MA));
            if best.is_none_or(|current: (u16, u16, u16)| {
                candidate.2 > current.2
                    || (candidate.2 == current.2
                        && (candidate.1 > current.1
                            || (candidate.1 == current.1 && candidate.0 < current.0)))
            }) {
                best = Some(candidate);
            }
        }
        best
    }

    fn contiguous_pps_source_limits(&self, anchor_mv: u16) -> Option<(u16, u16, u16)> {
        let mut minimum_mv = u16::MAX;
        let mut reachable_max_mv = 0;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            if min_mv > anchor_mv || max_mv < anchor_mv || apdo.max_ma < 3_000 {
                continue;
            }
            minimum_mv = minimum_mv.min(min_mv);
            reachable_max_mv = reachable_max_mv.max(max_mv);
        }
        if reachable_max_mv == 0 {
            return None;
        }

        // APDOs may offer a higher voltage at a lower current. Extend the
        // automatic request range only through overlapping APDOs, leaving the
        // negotiated PPS contract as the current-limit authority per request.
        while self.extend_reachable_max(&mut reachable_max_mv) {}
        while self.extend_minimum(&mut minimum_mv) {}

        // The runtime heater budget accepts a single current ceiling for the
        // whole automatic voltage range. Derive a ceiling that is valid at
        // every boundary of the continuous APDO component, rather than taking
        // the (potentially higher) current offered only at its top voltage.
        let conservative_current_ma = self.conservative_current_for_range(minimum_mv, reachable_max_mv)?;
        Some((minimum_mv, reachable_max_mv, conservative_current_ma))
    }

    fn extend_reachable_max(&self, reachable_max_mv: &mut u16) -> bool {
        let previous = *reachable_max_mv;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            if apdo.max_ma >= 3_000 && min_mv <= *reachable_max_mv && max_mv > *reachable_max_mv {
                *reachable_max_mv = max_mv;
            }
        }
        *reachable_max_mv != previous
    }

    fn extend_minimum(&self, minimum_mv: &mut u16) -> bool {
        let previous = *minimum_mv;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            if apdo.max_ma >= 3_000 && max_mv >= *minimum_mv && min_mv < *minimum_mv {
                *minimum_mv = min_mv;
            }
        }
        *minimum_mv != previous
    }

    fn conservative_current_for_range(
        &self,
        minimum_mv: u16,
        reachable_max_mv: u16,
    ) -> Option<u16> {
        let mut conservative_current_ma = u16::MAX;
        let mut current_observed = false;
        for apdo in self.capability_apdos.iter().flatten() {
            let apdo_min_mv = apdo.min_mv.max(self.request_min_mv);
            let apdo_max_mv = apdo.max_mv.min(self.request_max_mv);
            if apdo.max_ma < 3_000 || apdo_max_mv < minimum_mv || apdo_min_mv > reachable_max_mv {
                continue;
            }
            for checkpoint_mv in [
                apdo_min_mv.max(minimum_mv),
                apdo_max_mv.min(reachable_max_mv),
                apdo_max_mv.saturating_add(1),
            ]
            .into_iter()
            .filter(|checkpoint_mv| {
                *checkpoint_mv >= minimum_mv && *checkpoint_mv <= reachable_max_mv
            })
            {
                let current_ma = self.maximum_pps_current_for_target(checkpoint_mv)?;
                conservative_current_ma = conservative_current_ma.min(current_ma);
                current_observed = true;
            }
        }
        current_observed.then_some(conservative_current_ma)
    }

    fn enable(
        &mut self,
        owner: ManualPpsOwner,
        target_mv: u16,
        target_ma: Option<u16>,
    ) -> Result<(), ManualPpsError> {
        let target_ma = target_ma
            .or_else(|| self.maximum_pps_current_for_target(target_mv))
            .ok_or(ManualPpsError::NoPpsCapability)?;
        self.validate_target(target_mv, target_ma)?;
        self.enabled = true;
        self.owner = owner;
        self.target_mv = Some(target_mv);
        self.target_ma = Some(target_ma);
        self.error = None;
        self.automatic_restore_pending = false;
        Ok(())
    }

    fn clear(&mut self) {
        let had_override = self.enabled
            || self.target_mv.is_some()
            || self.target_ma.is_some()
            || self.applied_mv.is_some();
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = None;
        self.automatic_restore_pending |= had_override;
    }

    fn fail(&mut self, error: ManualPpsError) {
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = Some(error);
        self.automatic_restore_pending = true;
    }

    fn consume_automatic_restore_pending(&mut self) -> bool {
        let pending = self.automatic_restore_pending;
        self.automatic_restore_pending = false;
        pending
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn manual_pps_error_code(
    error: ManualPpsError,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    error_code_string(error.code())
}

#[cfg(any(target_arch = "xtensa", test))]
fn error_code_string(
    value: &str,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    let mut out = heapless::String::new();
    let _ = out.push_str(value);
    out
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackend {
    const fn label(self) -> &'static str {
        match self {
            Self::PpsMos { .. } => "pps-mos",
            Self::FixedPdPwmFallback { .. } => "fixed-pd-pwm-fallback",
        }
    }

    const fn pd_request_mv(self) -> u16 {
        match self {
            Self::PpsMos {
                current_request_mv, ..
            } => current_request_mv,
            Self::FixedPdPwmFallback { fixed_request, .. } => fixed_request.millivolts(),
        }
    }

    const fn pd_contract_mv(self) -> u16 {
        self.pd_request_mv()
    }

    const fn terminal_fixed_pd_disarmed(self) -> bool {
        match self {
            Self::PpsMos {
                terminal_fixed_pd_disarmed,
                ..
            }
            | Self::FixedPdPwmFallback {
                terminal_fixed_pd_disarmed,
                ..
            } => terminal_fixed_pd_disarmed,
        }
    }

    fn set_terminal_fixed_pd_disarmed(&mut self, disarmed: bool) {
        match self {
            Self::PpsMos {
                terminal_fixed_pd_disarmed,
                ..
            }
            | Self::FixedPdPwmFallback {
                terminal_fixed_pd_disarmed,
                ..
            } => *terminal_fixed_pd_disarmed = disarmed,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn effective_pd_contract_mv(
    manual_pps: &ManualPpsState,
    observation: Option<PdStatusObservation>,
    backend: HeaterPowerBackend,
) -> u16 {
    manual_pps
        .target_mv
        .filter(|_| manual_pps.enabled)
        .or_else(|| {
            observation
                .filter(|observation| observation.status.pd_active)
                .and_then(|observation| observation.contract_voltage_mv)
        })
        .unwrap_or_else(|| backend.pd_contract_mv())
}

#[cfg(target_arch = "xtensa")]
fn log_ui_state(state: &FrontPanelUiState) {
    info!(
        "ui route={=str} temp_c={=i16} target_c={=i16} heater_arm={=bool} heater_out={=u8}% fan_runtime={=bool} fan_display={=str} cooling_policy={=bool} heater_lock={=str} warn_visible={=bool}",
        route_label(state.route),
        state.current_temp_c,
        state.target_temp_c,
        state.heater_enabled,
        state.heater_output_percent,
        state.fan_enabled,
        state.fan_display_state.label(),
        state.active_cooling_enabled,
        state
            .heater_lock_reason
            .map(|reason| reason.label())
            .unwrap_or("none"),
        state.dashboard_warning_visible,
    );
}

#[cfg(any(target_arch = "xtensa", test))]
fn pt1000_resistance_ohms_at(temp_c: f32) -> f32 {
    let polynomial = 1.0 + PT1000_A * temp_c + PT1000_B * temp_c * temp_c;
    if temp_c >= 0.0 {
        PT1000_R0_OHMS * polynomial
    } else {
        PT1000_R0_OHMS * (polynomial + PT1000_C * (temp_c - 100.0) * temp_c * temp_c * temp_c)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn pt1000_temperature_c_from_resistance(resistance_ohms: f32) -> f32 {
    let mut low = RTD_TEMP_MIN_C;
    let mut high = RTD_TEMP_MAX_C;
    for _ in 0..32 {
        let mid = (low + high) * 0.5;
        if pt1000_resistance_ohms_at(mid) < resistance_ohms {
            low = mid;
        } else {
            high = mid;
        }
    }
    (low + high) * 0.5
}

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_resistance_ohms_from_mv(adc_mv: u16) -> Result<f32, HeaterFaultReason> {
    rtd_resistance_ohms_from_fractional_mv(f32::from(adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_resistance_ohms_from_fractional_mv(adc_mv: f32) -> Result<f32, HeaterFaultReason> {
    if adc_mv <= f32::from(RTD_SHORT_FAULT_MAX_MV) {
        return Err(HeaterFaultReason::SensorShort);
    }
    if adc_mv >= f32::from(RTD_OPEN_FAULT_MIN_MV) {
        return Err(HeaterFaultReason::SensorOpen);
    }
    let supply_mv_f = RTD_DIVIDER_SUPPLY_MV as f32;
    if adc_mv >= supply_mv_f {
        return Err(HeaterFaultReason::SensorOpen);
    }

    Ok(RTD_REFERENCE_RESISTOR_OHMS * adc_mv / (supply_mv_f - adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
fn correct_adc_fractional_mv(
    memory_config: &MemoryConfig,
    channel: AdcCalibrationChannel,
    raw_adc_mv: f32,
) -> f32 {
    let lower_mv = raw_adc_mv.floor().clamp(0.0, f32::from(u16::MAX)) as u16;
    let upper_mv = lower_mv.saturating_add(1);
    let lower_corrected = f32::from(correct_adc_mv(
        &memory_config.adc_calibration,
        channel,
        lower_mv,
    ));
    if upper_mv == lower_mv {
        return lower_corrected;
    }
    let upper_corrected = f32::from(correct_adc_mv(
        &memory_config.adc_calibration,
        channel,
        upper_mv,
    ));
    lower_corrected + ((upper_corrected - lower_corrected) * (raw_adc_mv - f32::from(lower_mv)))
}

#[cfg(any(target_arch = "xtensa", test))]
fn projected_rtd_temperature_c(memory_config: &MemoryConfig, raw_adc_mv: u16) -> Option<f32> {
    let corrected_mv = correct_adc_fractional_mv(
        memory_config,
        AdcCalibrationChannel::Rtd,
        f32::from(raw_adc_mv),
    );
    rtd_resistance_ohms_from_fractional_mv(corrected_mv)
        .ok()
        .map(pt1000_temperature_c_from_resistance)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_runtime_wire(memory_config: &MemoryConfig) -> ThermalPlantRuntimeWire {
    let active_projection = memory_config
        .thermal_plant_transient_active
        .and_then(|transaction| {
            (thermal_plant_curve_is_bound(memory_config, transaction)
                && thermal_plant_transient_trace_reaches_targets(&transaction, memory_config))
            .then(|| thermal_plant_projection_from_transient(&transaction))
            .flatten()
        });
    let state =
        if memory_config.thermal_plant_transient_active.is_some() && active_projection.is_none() {
            "invalid"
        } else if active_projection.is_some() {
            "active"
        } else {
            "missing"
        };
    let mut state_wire = heapless::String::new();
    let _ = state_wire.push_str(state);
    ThermalPlantRuntimeWire {
        state: state_wire,
        active_transaction_id: memory_config
            .thermal_plant_transient_active
            .map(|transaction| transaction.transaction_id),
        projection_valid: active_projection.is_some(),
        convection_mw_per_c: active_projection.map(|projection| projection.convection_mw_per_c),
        radiation_mw_per_k4: active_projection.map(|projection| projection.radiation_mw_per_k4),
        thermal_capacity_mj_per_c: active_projection
            .map(|projection| projection.thermal_capacity_mj_per_c),
        transport_delay_ms: active_projection.map(|projection| projection.transport_delay_ms),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_plant_run_snapshot_wire(
    calibration: &CalibrationRuntimeState,
    memory_config: &MemoryConfig,
    workspace: &CalibrationThermalPlantWorkspace,
    after_sample: u8,
    current_temp_c: f32,
    heater_voltage_mv: u32,
    duty_percent: u8,
) -> ThermalPlantRunSnapshotWire {
    let current_temp_centi_c = round_to_i16(current_temp_c * 100.0);
    let current_voltage_mv = heater_voltage_mv.min(u32::from(u16::MAX)) as u16;
    let job = workspace.job.as_ref();
    let persisted = memory_config.thermal_plant_transient_active.as_ref();
    let (samples, sample_count) = if let Some(job) = job {
        (&job.samples[..], job.sample_count)
    } else if let Some(transaction) = persisted {
        (&transaction.samples[..], transaction.sample_count)
    } else {
        (&[][..], 0)
    };
    let run_id = job
        .map(|value| value.run_id)
        .or_else(|| (workspace.next_run_id != 0).then_some(workspace.next_run_id))
        .or_else(|| persisted.map(|transaction| transaction.transaction_id))
        .unwrap_or(0);
    let trace_page = thermal_plant_trace_page(
        memory_config,
        samples,
        sample_count,
        after_sample,
        current_temp_centi_c,
    );
    let provisional_curve = job.and_then(thermal_plant_provisional_curve);
    let active_result =
        persisted.and_then(|transaction| thermal_plant_active_result(memory_config, transaction));
    let attempt = job.map(|job| {
        thermal_plant_run_attempt(
            calibration,
            job,
            run_id,
            current_temp_centi_c,
            current_voltage_mv,
            duty_percent,
        )
    });
    ThermalPlantRunSnapshotWire {
        version: 1,
        attempt,
        trace_page,
        provisional_curve,
        active_result,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_trace_page(
    memory_config: &MemoryConfig,
    samples: &[ThermalPlantTransientSample],
    sample_count: u8,
    after_sample: u8,
    fallback_temp_centi_c: i16,
) -> ThermalPlantTracePageWire {
    let start = after_sample.min(sample_count);
    let mut points = heapless::Vec::new();
    let mut saw_heating = samples
        .iter()
        .take(usize::from(start))
        .any(|sample| sample.duty_percent > 0);
    for (offset, sample) in samples
        .iter()
        .take(usize::from(sample_count))
        .enumerate()
        .skip(usize::from(start))
        .take(flux_purr_firmware::control_plane::THERMAL_PLANT_TRACE_PAGE_MAX)
    {
        let phase = thermal_plant_trace_phase(&mut saw_heating, sample.duty_percent);
        let temperature_centi_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)
            .map(|value| round_to_i16(value * 100.0))
            .unwrap_or(fallback_temp_centi_c);
        let _ = points.push(ThermalPlantTracePointWire {
            sample_index: offset as u8,
            elapsed_ms: u32::from(sample.elapsed_ticks)
                .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
            temperature_centi_c,
            heater_voltage_mv: thermal_plant_heater_voltage_mv(sample.heater_voltage_125mv),
            duty_percent: sample.duty_percent.min(100),
            phase,
        });
    }
    let next_sample = (start.saturating_add(points.len() as u8) < sample_count)
        .then_some(start.saturating_add(points.len() as u8));
    ThermalPlantTracePageWire {
        start_sample: start,
        next_sample,
        total_samples: sample_count,
        points,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_trace_phase(saw_heating: &mut bool, duty_percent: u8) -> ThermalPlantRunPhaseWire {
    if duty_percent > 0 {
        *saw_heating = true;
        ThermalPlantRunPhaseWire::Heating
    } else if *saw_heating {
        ThermalPlantRunPhaseWire::Cooling
    } else {
        ThermalPlantRunPhaseWire::Ambient
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_provisional_curve(
    job: &CalibrationThermalPlantAutoJob,
) -> Option<ThermalPlantProvisionalCurveWire> {
    let curve = heater_curve_from_transient_bins(&job.heater_curve.bins)?;
    let covered = job
        .heater_curve
        .bins
        .iter()
        .filter(|bin| bin.samples > 0)
        .count() as u8;
    let mut state = heapless::String::new();
    let _ = state.push_str("preview");
    Some(ThermalPlantProvisionalCurveWire {
        state,
        coverage_percent: covered.saturating_mul(25),
        curve: HeaterCurvePackageWire::from_memory(&curve, None),
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_active_result(
    memory_config: &MemoryConfig,
    transaction: &ThermalPlantTransientTransaction,
) -> Option<ThermalPlantActiveResultWire> {
    let (projection, _) = thermal_plant_projection_for_runtime(memory_config)?;
    Some(ThermalPlantActiveResultWire {
        transaction_id: transaction.transaction_id,
        curve: HeaterCurvePackageWire::from_memory(
            &memory_config.active_heater_curve,
            Some(&memory_config.heater_curve_raw_observations),
        ),
        convection_mw_per_c: Some(projection.convection_mw_per_c),
        radiation_mw_per_k4: Some(projection.radiation_mw_per_k4),
        thermal_capacity_mj_per_c: Some(projection.thermal_capacity_mj_per_c),
        transport_delay_ms: Some(projection.transport_delay_ms),
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_run_attempt(
    calibration: &CalibrationRuntimeState,
    job: &CalibrationThermalPlantAutoJob,
    run_id: u32,
    current_temp_centi_c: i16,
    current_voltage_mv: u16,
    duty_percent: u8,
) -> ThermalPlantRunAttemptWire {
    ThermalPlantRunAttemptWire {
        run_id,
        status: calibration.job.status.to_wire(),
        phase: Some(match job.phase {
            ThermalPlantAutoPhase::Ambient => ThermalPlantRunPhaseWire::Ambient,
            ThermalPlantAutoPhase::Heating => ThermalPlantRunPhaseWire::Heating,
            ThermalPlantAutoPhase::Cooling => ThermalPlantRunPhaseWire::Cooling,
        }),
        progress_percent: calibration.job.progress_percent,
        elapsed_ms: job
            .elapsed_ticks
            .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
        current_temp_centi_c,
        heater_voltage_mv: current_voltage_mv,
        duty_percent: duty_percent.min(100),
        sample_count: job.sample_count,
        restart_allowed: calibration.job.status != CalibrationJobStatus::Running
            && !calibration.immediate_heater_disarm_pending
            && !calibration.thermal_plant_completion_disarm_pending,
        error: calibration.job.message.map(manual_pps_error_code),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_plant_projection_for_runtime(
    memory_config: &MemoryConfig,
) -> Option<(flux_purr_firmware::memory::ThermalPlantProjection, f32)> {
    let transaction = memory_config.thermal_plant_transient_active?;
    if !thermal_plant_curve_is_bound(memory_config, transaction)
        || !thermal_plant_transient_trace_reaches_targets(&transaction, memory_config)
    {
        return None;
    }
    let projection = thermal_plant_projection_from_transient(&transaction)?;
    let ambient_temp_c =
        projected_rtd_temperature_c(memory_config, transaction.ambient_raw_rtd_adc_mv)?;
    Some((projection, ambient_temp_c))
}
