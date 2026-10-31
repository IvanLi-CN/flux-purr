#[allow(unused_imports)]
use super::*;

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use core::fmt::Write as _;

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn fusb302b_degraded_reason() -> &'static str {
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
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn fusb302b_capability_refresh_is_transient() -> bool {
    matches!(
        fusb302b_degraded_reason(),
        "pd_fusb_source_caps_gcrc_seen"
            | "pd_fusb_source_caps_requested"
            | "pd_fusb_phy_recovering"
    )
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn fusb302b_identity_degraded_reason()
-> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    let diagnostic = FUSB302B_IDENTITY_DIAGNOSTIC.load(Ordering::Acquire);
    let mut reason = error_code_string(match diagnostic {
        FUSB302B_IDENTITY_DIAG_I2C_ERROR => "pd_fusb_identity_i2c_error",
        FUSB302B_IDENTITY_DIAG_ID_MISMATCH => "pd_fusb_identity_mismatch",
        FUSB302B_IDENTITY_DIAG_UNSUPPORTED_ID => "pd_fusb_identity_unsupported_id",
        FUSB302B_IDENTITY_DIAG_STATUS_INVALID => "pd_fusb_identity_status_invalid",
        _ => "pd_contract_unavailable",
    });
    if diagnostic == FUSB302B_IDENTITY_DIAG_ID_MISMATCH {
        let _ = write!(
            reason,
            "_{:02x}_{:02x}",
            FUSB302B_IDENTITY_FIRST_ID.load(Ordering::Acquire),
            FUSB302B_IDENTITY_SECOND_ID.load(Ordering::Acquire),
        );
    } else if diagnostic == FUSB302B_IDENTITY_DIAG_UNSUPPORTED_ID {
        let _ = write!(
            reason,
            "_{:02x}",
            FUSB302B_IDENTITY_SECOND_ID.load(Ordering::Acquire),
        );
    }
    reason
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) const fn fusb302b_protocol_fault_code(fault: u8) -> Option<&'static str> {
    match fault {
        FUSB302B_PROTOCOL_FAULT_VBUS_LOW => Some("pd_fusb_vbus_low"),
        FUSB302B_PROTOCOL_FAULT_RECEIVED_RESET => Some("pd_fusb_received_reset"),
        FUSB302B_PROTOCOL_FAULT_RETRY_FAILED => Some("pd_fusb_retry_failed"),
        FUSB302B_PROTOCOL_FAULT_PENDING_REQUEST_TIMEOUT => Some("pd_fusb_pending_request_timeout"),
        FUSB302B_PROTOCOL_FAULT_SOURCE_CAPABILITIES_TIMEOUT => {
            Some("pd_fusb_source_capabilities_timeout")
        }
        FUSB302B_PROTOCOL_FAULT_PARTIAL_RECEIVE_TIMEOUT => Some("pd_fusb_partial_receive_timeout"),
        FUSB302B_PROTOCOL_FAULT_RECEIVE_IO => Some("pd_fusb_receive_io_error"),
        FUSB302B_PROTOCOL_FAULT_TRANSMIT_IO => Some("pd_fusb_transmit_io_error"),
        FUSB302B_PROTOCOL_FAULT_CONFIGURATION_IO => Some("pd_fusb_configuration_io_error"),
        FUSB302B_PROTOCOL_FAULT_PROTECTION => Some("pd_fusb_protection"),
        FUSB302B_PROTOCOL_FAULT_READ_INTERRUPTS_IO => Some("pd_fusb_read_interrupts_io_error"),
        FUSB302B_PROTOCOL_FAULT_READ_STATUS_IO => Some("pd_fusb_read_status_io_error"),
        FUSB302B_PROTOCOL_FAULT_RX_FIFO_FLUSH_IO => Some("pd_fusb_rx_fifo_flush_io_error"),
        FUSB302B_PROTOCOL_FAULT_RECEIVE_PACKET_IO => Some("pd_fusb_receive_packet_io_error"),
        FUSB302B_PROTOCOL_FAULT_UNSUPPORTED_SOP => Some("pd_fusb_unsupported_sop"),
        _ => None,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) const fn fusb302b_i2c_error_code(error: u8) -> Option<&'static str> {
    match error {
        FUSB302B_I2C_ERROR_BUS_BUSY => Some("pd_i2c_bus_busy"),
        FUSB302B_I2C_ERROR_ACK_ADDRESS => Some("pd_i2c_ack_address"),
        FUSB302B_I2C_ERROR_ACK_DATA => Some("pd_i2c_ack_data"),
        FUSB302B_I2C_ERROR_ACK_UNKNOWN => Some("pd_i2c_ack_unknown"),
        FUSB302B_I2C_ERROR_TIMEOUT => Some("pd_i2c_timeout"),
        FUSB302B_I2C_ERROR_ARBITRATION_LOST => Some("pd_i2c_arbitration_lost"),
        FUSB302B_I2C_ERROR_EXECUTION_INCOMPLETE => Some("pd_i2c_execution_incomplete"),
        FUSB302B_I2C_ERROR_OTHER => Some("pd_i2c_other"),
        _ => None,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn fusb302b_last_protocol_fault()
-> Option<heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }>> {
    fusb302b_protocol_fault_code(FUSB302B_LAST_PROTOCOL_FAULT.load(Ordering::Acquire))
        .map(error_code_string)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn fusb302b_last_i2c_error()
-> Option<heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }>> {
    fusb302b_i2c_error_code(FUSB302B_LAST_I2C_ERROR.load(Ordering::Acquire)).map(error_code_string)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) fn adc_diagnostics_wire() -> AdcDiagnosticsWire {
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
        rtd_assumed_offset_uv: 0,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RtdSample {
    Valid(RtdMeasurement),
    Fault {
        adc_mv: Option<u16>,
        reason: HeaterFaultReason,
    },
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PdStatusObservation {
    pub(crate) status_raw: u8,
    pub(crate) status: Status,
    pub(crate) current_raw: u8,
    pub(crate) current_ma: u16,
    pub(crate) contract_voltage_mv: Option<u16>,
    pub(crate) contract: Contract,
}

/// Convert the FUSB302B STATUS0 sample into the legacy status projection.
///
/// STATUS0 is a FUSB302B register and its VBUSOK bit is not the same bit used
/// by the CH224Q status register for `pd_active`. Keep the sampled byte intact
/// for diagnostics, and derive the cross-controller semantic field explicitly.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn fusb302b_status_projection(status0: u8) -> Status {
    Status {
        bc_active: false,
        qc2_active: false,
        qc3_active: false,
        pd_active: status0 & FUSB302B_STATUS0_VBUSOK != 0,
        epr_active: false,
        epr_exist: false,
        avs_exist: false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PdStatusLogKey {
    status_raw: u8,
    pd_active: bool,
    epr_active: bool,
    epr_exist: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pd_status_log_key(
    observation: Option<PdStatusObservation>,
) -> Option<PdStatusLogKey> {
    observation.map(|observation| PdStatusLogKey {
        status_raw: observation.status_raw,
        pd_active: observation.status.pd_active,
        epr_active: observation.status.epr_active,
        epr_exist: observation.status.epr_exist,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pd_contract_allows_calibration(
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
pub(crate) fn pd_allows_calibration_job_start(
    controller: ControllerKind,
    observation: Option<PdStatusObservation>,
    kind: Option<CalibrationJobKindWire>,
    manual_pps: &ManualPpsState,
) -> bool {
    if kind == Some(CalibrationJobKindWire::ThermalPlantAuto) {
        // This scan programs both PPS voltage and current. A prior high-voltage
        // contract is unnecessary; acquisition stays off until its own confirms.
        return controller == ControllerKind::Fusb302b
            && manual_pps.thermal_plant_source_limits().is_some();
    }
    pd_contract_allows_calibration(controller, observation)
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaterPowerBackendReason {
    PpsCovers20v,
    NoPps20vCapability,
    CapabilityReadFailed,
    AdjustableRequestFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackendReason {
    pub(crate) const fn label(self) -> &'static str {
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
pub(crate) enum HeaterPowerBackend {
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
pub(crate) struct HoldPpsGovernor {
    active: bool,
    next_adjust_at_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
pub(crate) struct HoldPpsRequestInput {
    pub(crate) phase: HeaterControlPhase,
    pub(crate) duty_percent: u8,
    pub(crate) actual_error_c: f32,
    pub(crate) filtered_slope_c_per_s: f32,
    pub(crate) current_request_mv: u16,
    pub(crate) control_floor_mv: u16,
    pub(crate) safe_max_mv: u16,
    pub(crate) now_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HoldPpsGovernor {
    pub(crate) const fn new() -> Self {
        Self {
            active: false,
            next_adjust_at_ms: 0,
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    fn step_request_into_bounds(
        current_request_mv: u16,
        control_floor_mv: u16,
        safe_max_mv: u16,
    ) -> u16 {
        if safe_max_mv < control_floor_mv {
            return safe_max_mv;
        }
        let bounded_safe_max_mv = safe_max_mv;
        if current_request_mv < control_floor_mv {
            current_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(control_floor_mv)
        } else if current_request_mv > bounded_safe_max_mv {
            bounded_safe_max_mv
        } else {
            current_request_mv
        }
    }

    pub(crate) fn request_mv(&mut self, input: HoldPpsRequestInput) -> Option<u16> {
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
        if safe_max_mv < control_floor_mv {
            return Some(safe_max_mv);
        }
        let bounded_safe_max_mv = safe_max_mv;
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
pub(crate) enum ManualPpsError {
    NoPpsCapability,
    InvalidVoltage,
    InvalidCurrent,
    CalibrationInProgress,
    TerminalDisarmPending,
    ThermalPlantManagedByJob,
    HeaterCurveCoverageInsufficient,
    ThermalPlantSourceUnsupported,
    ThermalPlantProjectionInvalid,
    ThermalPlantTimeout,
    SupplyScan(flux_purr_firmware::heater_supply::scan::ScanFailure),
    PdNotReady,
    WriteFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPpsOwner {
    Debug,
    Calibration,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ManualPpsError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "manual_pps_no_capability",
            Self::InvalidVoltage => "manual_pps_invalid_voltage",
            Self::InvalidCurrent => "manual_pps_invalid_current",
            Self::CalibrationInProgress => "manual_pps_calibration_busy",
            Self::TerminalDisarmPending => "heater_disarm_pending",
            Self::ThermalPlantManagedByJob => "thermal_plant_managed_by_job",
            Self::HeaterCurveCoverageInsufficient => "heater_curve_coverage_insufficient",
            Self::ThermalPlantSourceUnsupported => "thermal_plant_source_unsupported",
            Self::ThermalPlantProjectionInvalid => "thermal_plant_projection_invalid",
            Self::ThermalPlantTimeout => "thermal_plant_timeout",
            Self::SupplyScan(reason) => match reason {
                flux_purr_firmware::heater_supply::scan::ScanFailure::WifiChanged => {
                    "supply_scan_wifi_changed"
                }
                flux_purr_firmware::heater_supply::scan::ScanFailure::SupplyUnstable => {
                    "supply_scan_supply_unstable"
                }
                flux_purr_firmware::heater_supply::scan::ScanFailure::AmbiguousResponse => {
                    "supply_scan_ambiguous_response"
                }
                flux_purr_firmware::heater_supply::scan::ScanFailure::StageTimeout => {
                    "supply_scan_stage_timeout"
                }
                flux_purr_firmware::heater_supply::scan::ScanFailure::InvalidSample => {
                    "supply_scan_invalid_sample"
                }
            },
            Self::PdNotReady => "manual_pps_pd_not_ready",
            Self::WriteFailed => "manual_pps_write_failed",
        }
    }

    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "PPS capability is unavailable.",
            Self::InvalidVoltage => {
                "manualPpsMv/manualPpsMa must match PPS capability and APDO steps."
            }
            Self::InvalidCurrent => {
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
                "The calibration did not collect a usable whole-device supply boundary."
            }
            Self::ThermalPlantSourceUnsupported => {
                "Thermal plant calibration requires a PPS APDO covering 20V at 3A or more."
            }
            Self::ThermalPlantProjectionInvalid => {
                "Thermal plant observations did not produce a physical model."
            }
            Self::ThermalPlantTimeout => {
                "Thermal plant calibration exceeded its five-minute time budget."
            }
            Self::SupplyScan(_) => {
                "Whole-device supply scanning stopped; inspect the scan error code."
            }
            Self::PdNotReady => "PD contract is not ready for manual PPS.",
            Self::WriteFailed => "Manual PPS write failed.",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPpsState {
    pub(crate) enabled: bool,
    pub(crate) owner: ManualPpsOwner,
    pub(crate) request_min_mv: u16,
    pub(crate) request_max_mv: u16,
    pub(crate) target_mv: Option<u16>,
    pub(crate) target_ma: Option<u16>,
    pub(crate) applied_mv: Option<u16>,
    pub(crate) capability_min_mv: Option<u16>,
    pub(crate) capability_max_mv: Option<u16>,
    pub(crate) capability_max_ma: Option<u16>,
    pub(crate) capability_apdos: [Option<ch224q::PpsApdo>; ch224q::MAX_PPS_APDOS],
    pub(crate) source_power_limit_mw: Option<u32>,
    pub(crate) source_power_limit_ma: Option<u16>,
    pub(crate) error: Option<ManualPpsError>,
    pub(crate) automatic_restore_pending: bool,
    pub(crate) automatic_restore_ticket: Option<PowerTicket>,
    pub(crate) automatic_restore_confirmed: bool,
    pub(crate) allow_pending_refresh: bool,
    pub(crate) pending_power_ticket: Option<PowerTicket>,
    pub(crate) pending_power_request_mv: Option<u16>,
    pub(crate) supply_scan_range: Option<PpsAdjustmentRange>,
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
            source_power_limit_mw: None,
            source_power_limit_ma: None,
            error: None,
            automatic_restore_pending: false,
            automatic_restore_ticket: None,
            automatic_restore_confirmed: false,
            allow_pending_refresh: false,
            pending_power_ticket: None,
            pending_power_request_mv: None,
            supply_scan_range: None,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomaticRestorePhase {
    NotRequired,
    RequestIdle,
    AwaitIdle(PowerTicket),
    ResetBackend,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalibrationMode {
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
pub(crate) enum CalibrationJobKind {
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
pub(crate) enum CalibrationJobStatus {
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
pub(crate) struct CalibrationJobState {
    pub(crate) kind: Option<CalibrationJobKind>,
    pub(crate) status: CalibrationJobStatus,
    pub(crate) progress_percent: u8,
    pub(crate) samples_collected: u8,
    pub(crate) next_request_mv: Option<u16>,
    pub(crate) message: Option<ManualPpsError>,
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
pub(crate) const CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES: usize = 24;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_AMBIENT_TICKS: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_TOTAL_TIMEOUT_TICKS: u32 = 6_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_HEAT_TIMEOUT_TICKS: u32 = 432_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_COOL_TIMEOUT_TICKS: u32 = 24_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_TARGET_TEMP_C: f32 = 220.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_COOL_COMPLETE_TEMP_C: f32 = 80.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C: f32 = 4.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_TRACE_INTERVAL_TICKS: u32 = 200;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CalibrationVinAutoJob {
    pub(crate) start_request_mv: u16,
    pub(crate) next_request_mv: u16,
    pub(crate) max_request_mv: u16,
    pub(crate) target_ma: u16,
    pub(crate) settle_ticks: u8,
    pub(crate) stable_ticks: u8,
    pub(crate) last_observed_mv: Option<u16>,
    pub(crate) sample_count: u8,
    pub(crate) samples: [Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn round_to_i16(value: f32) -> i16 {
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
pub(crate) fn round_to_u16_nonnegative(value: f32) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    (value + 0.5).clamp(0.0, u16::MAX as f32) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ThermalPlantAutoPhase {
    Ambient,
    Heating,
    Cooling,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug)]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) struct CalibrationThermalPlantAutoJob {
    pub(crate) run_id: u32,
    pub(crate) phase: ThermalPlantAutoPhase,
    pub(crate) source_current_ma: u16,
    pub(crate) ambient_raw_rtd_adc_mv: u16,
    pub(crate) idle_samples: u16,
    pub(crate) supply_scan: Option<flux_purr_firmware::heater_supply::scan::SupplyScan>,
    pub(crate) started_at_ms: Option<u64>,
    pub(crate) elapsed_ticks: u32,
    pub(crate) phase_started_tick: u32,
    pub(crate) sample_count: u8,
    pub(crate) last_saved_temp_c: f32,
    pub(crate) last_saved_tick: u32,
    pub(crate) samples: [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
}

// Keep the bounded transient trace out of the Embassy main task. The job is
// large enough that retaining it in CalibrationRuntimeState exhausts the
// application stack during Wi-Fi initialization.
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug, Default)]
pub(crate) struct CalibrationThermalPlantWorkspace {
    pub(crate) job: Option<CalibrationThermalPlantAutoJob>,
    pub(crate) next_run_id: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CalibrationJobData {
    VinAdc(CalibrationVinAutoJob),
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeaterCurvePreview {
    pub(crate) curve: HeaterCurveConfig,
    pub(crate) raw_observations: Option<HeaterCurveRawObservations>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn preview_heater_curve_config(
    preview: Option<&HeaterCurvePreview>,
) -> Option<&HeaterCurveConfig> {
    preview.map(|preview| &preview.curve)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CalibrationRuntimeState {
    pub(crate) mode: CalibrationMode,
    pub(crate) pps_enabled: bool,
    pub(crate) pps_mv: Option<u16>,
    pub(crate) pps_ma: Option<u16>,
    pub(crate) heater_enabled: bool,
    pub(crate) target_adc_mv: Option<u16>,
    pub(crate) stable: bool,
    pub(crate) stability_error_mv: Option<i16>,
    pub(crate) error: Option<ManualPpsError>,
    pub(crate) scan_phase: Option<flux_purr_firmware::heater_supply::scan::ScanPhase>,
    pub(crate) scan_failure: Option<flux_purr_firmware::heater_supply::scan::ScanFailure>,
    pub(crate) scan_contract_confirmed: bool,
    pub(crate) job: CalibrationJobState,
    pub(crate) job_data: Option<CalibrationJobData>,
    pub(crate) model_target_temp_c: Option<i16>,
    pub(crate) thermal_plant_completion_disarm_pending: bool,
    pub(crate) immediate_heater_disarm_pending: bool,
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
            scan_phase: None,
            scan_failure: None,
            scan_contract_confirmed: false,
            job: CalibrationJobState::default(),
            job_data: None,
            model_target_temp_c: None,
            thermal_plant_completion_disarm_pending: false,
            immediate_heater_disarm_pending: false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn calibration_runtime_state_to_wire(
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
        scan_phase: state.scan_phase.map(|phase| {
            let mut value = heapless::String::new();
            let _ = value.push_str(phase.label());
            value
        }),
        scan_failure: state.scan_failure.map(|failure| {
            let mut value = heapless::String::new();
            let _ = value.push_str(failure.code());
            value
        }),
        scan_contract_confirmed: state.scan_phase.map(|_| state.scan_contract_confirmed),
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
pub(crate) fn reconcile_runtime_heater_enabled(
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
pub(crate) const fn calibration_heater_contract_ready(
    pd_contract_ready: bool,
    calibration_pps_transition_pending: bool,
) -> bool {
    pd_contract_ready || calibration_pps_transition_pending
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn disarm_stale_heater_arm_after_pd_transition(
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

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
pub(crate) fn reconcile_pd_contract_intent(
    observation: Option<PdStatusObservation>,
    pd_contract_ready: &mut bool,
    ui_state: &mut FrontPanelUiState,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    manual_pps_state: &mut ManualPpsState,
    physical_output_active: bool,
) -> bool {
    reconcile_pd_contract_intent_with_pending_automatic_pps(
        observation,
        pd_contract_ready,
        ui_state,
        calibration_runtime_state,
        manual_pps_state,
        physical_output_active,
        false,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn reconcile_pd_contract_intent_with_pending_automatic_pps(
    observation: Option<PdStatusObservation>,
    pd_contract_ready: &mut bool,
    ui_state: &mut FrontPanelUiState,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    manual_pps_state: &mut ManualPpsState,
    physical_output_active: bool,
    automatic_pps_transition_pending: bool,
) -> bool {
    let current_pd_contract_ready = startup_pd_contract_ready(observation);
    let mut needs_redraw = *pd_contract_ready != current_pd_contract_ready;
    if needs_redraw {
        let pd_was_ready = *pd_contract_ready;
        *pd_contract_ready = current_pd_contract_ready;
        if current_pd_contract_ready && !automatic_pps_transition_pending {
            needs_redraw |= disarm_stale_heater_arm_after_pd_transition(
                pd_was_ready,
                current_pd_contract_ready,
                ui_state,
                calibration_runtime_state,
            );
        }
    }

    if !current_pd_contract_ready
        && !automatic_pps_transition_pending
        && (ui_state.heater_enabled
            || calibration_runtime_state.heater_enabled
            || physical_output_active
            || ui_state.heater_output_percent != 0)
    {
        ui_state.heater_enabled = false;
        ui_state.heater_output_percent = 0;
        calibration_runtime_state.heater_enabled = false;
        manual_pps_state.fail(ManualPpsError::PdNotReady);
        needs_redraw = true;
    }
    needs_redraw
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn apply_pd_contract_observation<PWM>(
    observation: Option<PdStatusObservation>,
    pd_contract_ready: &mut bool,
    ui_state: &mut FrontPanelUiState,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    manual_pps_state: &mut ManualPpsState,
    heater_output: (&mut PWM, &mut u8),
    automatic_pps_transition_pending: bool,
) -> bool
where
    PWM: SetDutyCycle,
{
    let (heater_pwm, last_heater_duty) = heater_output;
    let previous_pd_ready = *pd_contract_ready;
    let needs_redraw = reconcile_pd_contract_intent_with_pending_automatic_pps(
        observation,
        pd_contract_ready,
        ui_state,
        calibration_runtime_state,
        manual_pps_state,
        *last_heater_duty != 0,
        automatic_pps_transition_pending,
    );
    if previous_pd_ready != *pd_contract_ready {
        info!("PD contract readiness -> {=bool}", *pd_contract_ready);
    }
    if !*pd_contract_ready && !automatic_pps_transition_pending {
        // Genuine unavailable or stale power still drops both intent and PWM;
        // normal negotiation carries the previous confirmed supply instead.
        apply_heater_duty(heater_pwm, 0, last_heater_duty);
    }
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_calibration_snapshot(
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
pub(crate) fn thermal_plant_calibration_temperature_c(
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
pub(crate) fn thermal_plant_cooling_complete(live_temp_c: f32, recorded_temp_c: f32) -> bool {
    live_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
        && recorded_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_output_must_be_off(
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
pub(crate) fn consume_thermal_plant_completion_disarm(
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
pub(crate) fn take_immediate_heater_disarm(
    calibration_runtime_state: &mut CalibrationRuntimeState,
) -> bool {
    let pending = calibration_runtime_state.immediate_heater_disarm_pending;
    calibration_runtime_state.immediate_heater_disarm_pending = false;
    pending
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn thermal_model_heater_allowed(
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
            });
    if !plant_is_available {
        return false;
    }

    manual_pps.has_matching_pps_apdo(20_000, 3_000)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_transient_trace_reaches_targets(
    transaction: &ThermalPlantTransientTransaction,
    memory_config: &MemoryConfig,
) -> bool {
    if !memory_config
        .heater_supply_boundary
        .as_ref()
        .is_some_and(flux_purr_firmware::heater_supply::SupplyBoundary::is_valid)
        || !flux_purr_firmware::memory::thermal_plant_transient_transaction_is_complete(transaction)
    {
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
        if !flux_purr_firmware::memory::thermal_plant_sample_is_cooling(*sample)
            && temperature_c > powered_max_temp_c
        {
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
    thermal_plant_heating_sample_covers_minimum(
        memory_config,
        samples[powered_peak_index].raw_rtd_adc_mv,
    ) && (temperatures_c[0] - ambient_temp_c).abs() <= 8.0
        && powered_peak_index + 1 < count
        && final_sample.is_some_and(flux_purr_firmware::memory::thermal_plant_sample_is_cooling)
        && final_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutomaticHeaterLookupContext {
    current_request_mv: Option<u16>,
    current_rtd_mv: Option<u16>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutomaticVoltageResponseContext {
    pub(crate) confirmed_range: Option<PpsAdjustmentRange>,
    pub(crate) current_rtd_mv: Option<u16>,
}

// A loaded 3 A source can regulate below the calibration snapshot because of
// cable and converter drop while still delivering the requested current. The
// absolute floor rejects a real source collapse without turning that normal
// current-limit knee into a false safety recovery.
#[cfg(any(target_arch = "xtensa", test))]
const AUTOMATIC_VIN_ABSOLUTE_FLOOR_PERCENT: u64 = 90;

#[cfg(any(target_arch = "xtensa", test))]
fn automatic_voltage_response_meets_absolute_floor(request_mv: u16, vin_raw_adc_mv: u16) -> bool {
    let expected_adc_mv = vin_adc_mv_for_input_mv(u32::from(request_mv));
    expected_adc_mv > 0
        && u64::from(vin_raw_adc_mv) * 100
            >= u64::from(expected_adc_mv) * AUTOMATIC_VIN_ABSOLUTE_FLOOR_PERCENT
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl ManualPpsState {
    pub(crate) fn from_capabilities(
        capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    ) -> Self {
        Self::from_capabilities_with_request_bounds(
            capabilities,
            CH224Q_ADJUSTABLE_REQUEST_MIN_MV,
            ch224q::CH224Q_PPS_MAX_MV,
        )
    }

    pub(crate) fn from_fusb302b_capabilities(
        capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    ) -> Self {
        let mut state = Self::from_capabilities_with_request_bounds(
            capabilities,
            FUSB302B_PPS_MIN_MV,
            FUSB302B_PPS_MAX_MV,
        );
        state.allow_pending_refresh = true;
        state
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
        state.source_power_limit_mw = capabilities.source_power_limit_mw;
        state.source_power_limit_ma = capabilities.source_power_limit_ma;
        if state.capability_apdos.iter().all(Option::is_none) {
            state.capability_apdos[0] = Some(ch224q::PpsApdo {
                min_mv: bounded_min_mv,
                max_mv,
                max_ma,
            });
        }
        state
    }

    pub(crate) fn validate_target(
        &self,
        target_mv: u16,
        target_ma: u16,
    ) -> Result<(), ManualPpsError> {
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

    pub(crate) fn has_matching_pps_apdo(&self, target_mv: u16, target_ma: u16) -> bool {
        self.capability_apdos.iter().flatten().any(|apdo| {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            target_mv >= min_mv
                && target_mv <= max_mv
                && target_ma <= apdo.max_ma.min(MAX_HEATER_CONTRACT_MA)
        })
    }

    pub(crate) fn maximum_pps_current_for_target(&self, target_mv: u16) -> Option<u16> {
        self.capability_apdos
            .iter()
            .flatten()
            .filter_map(|apdo| {
                let min_mv = apdo.min_mv.max(self.request_min_mv);
                let max_mv = apdo.max_mv.min(self.request_max_mv);
                (target_mv >= min_mv && target_mv <= max_mv)
                    .then_some(apdo.max_ma.min(MAX_HEATER_CONTRACT_MA))
            })
            .max()
    }

    pub(crate) fn thermal_plant_source_limits(&self) -> Option<(u16, u16, u16)> {
        let range = self.thermal_plant_pps_range()?;
        let scan_current_ma = 3_000;
        // The FUSB302B path clamps every exposed/requested PPS contract to
        // its 5.5 V floor. Keep the persisted boundary keyed to that same
        // effective range even when the source advertises a lower raw APDO
        // minimum.
        let bounded_min_mv = range
            .min_mv
            .max(self.request_min_mv)
            .max(FUSB302B_PPS_MIN_MV);
        let bounded_max_mv = range.max_mv.min(self.request_max_mv).min(21_000);
        let operational_max_mv = match (self.source_power_limit_mw, self.source_power_limit_ma) {
            (Some(power_mw), Some(envelope_ma)) if envelope_ma >= scan_current_ma => {
                let fixed_power_max_mv =
                    (power_mw * 1_000 / u32::from(scan_current_ma)) as u16 / 100 * 100;
                fixed_power_max_mv.min(bounded_max_mv)
            }
            (Some(_), Some(_)) => return None,
            (None, None) => bounded_max_mv,
            _ => return None,
        };
        if operational_max_mv < bounded_min_mv {
            return None;
        }
        Some((bounded_min_mv, operational_max_mv, scan_current_ma))
    }

    pub(crate) fn thermal_plant_source_power_limit_mw(&self) -> u32 {
        self.source_power_limit_mw
            .zip(self.source_power_limit_ma)
            .filter(|(_, current_ma)| *current_ma >= 3_000)
            .map_or(0, |(power_mw, _)| power_mw)
    }

    /// Return the operational APDO range used by the persisted whole-device
    /// boundary. The boundary records the lower of the advertised APDO range
    /// and the source power envelope at 3 A; matching it against the raw
    /// advertised maximum would reject a valid 60 W boundary on a 21 V APDO.
    fn automatic_boundary_source_limits(&self, apdo: ch224q::PpsApdo) -> (u16, u16) {
        let bounded_min_mv = apdo
            .min_mv
            .max(self.request_min_mv)
            .max(FUSB302B_PPS_MIN_MV);
        let bounded_max_mv = apdo
            .max_mv
            .min(self.request_max_mv)
            .min(HEATER_ADJUSTABLE_MAX_MV);
        self.thermal_plant_source_limits()
            .map(|(source_min_mv, source_max_mv, _)| {
                (
                    source_min_mv.max(bounded_min_mv),
                    source_max_mv.min(bounded_max_mv),
                )
            })
            .unwrap_or((bounded_min_mv, bounded_max_mv))
    }

    pub(crate) fn heater_source_limits(&self) -> Option<(u16, u16, u16)> {
        self.contiguous_pps_source_limits(HEATER_ADJUSTABLE_MIN_MV)
    }

    #[cfg(test)]
    pub(crate) fn automatic_heater_pps_candidate(
        &self,
        current_temp_c: f32,
        preview_heater_curve: Option<&HeaterCurveConfig>,
        memory_config: &MemoryConfig,
        current_request_mv: Option<u16>,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        self.automatic_heater_pps_candidate_with_rtd(
            current_temp_c,
            preview_heater_curve,
            memory_config,
            current_request_mv,
            None,
        )
    }

    pub(crate) fn automatic_heater_pps_candidate_with_rtd(
        &self,
        current_temp_c: f32,
        preview_heater_curve: Option<&HeaterCurveConfig>,
        memory_config: &MemoryConfig,
        current_request_mv: Option<u16>,
        current_rtd_mv: Option<u16>,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        let _ = preview_heater_curve;
        self.capability_apdos
            .iter()
            .enumerate()
            .filter_map(|(index, apdo)| {
                let apdo = (*apdo)?;
                self.automatic_heater_pps_for_apdo(
                    apdo,
                    index,
                    current_temp_c,
                    memory_config,
                    apdo.max_ma,
                    AutomaticHeaterLookupContext {
                        current_request_mv,
                        current_rtd_mv,
                    },
                )
            })
            .max_by_key(|candidate| {
                (
                    candidate.power_mw,
                    current_request_mv
                        .is_some_and(|mv| (candidate.min_mv..=candidate.max_mv).contains(&mv)),
                    core::cmp::Reverse(candidate.allowed_mv),
                    core::cmp::Reverse(candidate.advertisement_index),
                )
            })
    }

    fn automatic_heater_pps_for_apdo(
        &self,
        apdo: ch224q::PpsApdo,
        index: usize,
        temp_c: f32,
        memory_config: &MemoryConfig,
        budget_ma: u16,
        lookup: AutomaticHeaterLookupContext,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        let wifi_started = flux_purr_firmware::heater_supply::wifi_hardware_started()?;
        let current_ma = encode_automatic_pps_current_ma(budget_ma.min(apdo.max_ma));
        // Boundary records are keyed to the product's encoded automatic
        // current budget. The raw APDO may advertise 5 A even when this
        // product intentionally caps automatic heating at 3 A; comparing the
        // raw advertisement with a 3 A calibration would discard the valid
        // whole-device boundary and fall back to the theoretical model.
        // The persisted boundary is keyed by the source capability used for
        // the 3 A scan. The terminal point deliberately executes at 2.85 A
        // to stay below the fixed-PDO power envelope, but that encoded
        // current must still validate against the same 3 A boundary while a
        // 20.5 V or 21 V terminal request is being confirmed.
        let boundary_source_max_ma = if current_ma == AUTOMATIC_TERMINAL_CURRENT_MA
            && apdo.max_ma >= MAX_AUTOMATIC_HEATER_CONTRACT_MA
        {
            MAX_AUTOMATIC_HEATER_CONTRACT_MA
        } else {
            current_ma
        };
        let (boundary_source_min_mv, boundary_source_max_mv) =
            self.automatic_boundary_source_limits(apdo);
        let mut selected_current_ma = current_ma;
        let observed_limit_mv =
            memory_config
                .heater_supply_boundary
                .as_ref()
                .and_then(|boundary| {
                    let query = flux_purr_firmware::heater_supply::BoundaryQuery {
                        temperature_centi_c: round_to_i16(temp_c * 100.0),
                        current_rtd_mv: lookup.current_rtd_mv,
                        current_request_mv: lookup.current_request_mv,
                        current_ma,
                        wifi_started,
                        source_min_mv: apdo.min_mv,
                        source_max_mv: apdo.max_mv,
                        operational_source_min_mv: boundary_source_min_mv,
                        operational_source_max_mv: boundary_source_max_mv,
                        source_max_ma: boundary_source_max_ma,
                        source_power_limit_mw: self.thermal_plant_source_power_limit_mw(),
                    };
                    let mut project_temperature = |raw| {
                        projected_rtd_temperature_c(memory_config, raw)
                            .map(|temp| round_to_i16(temp * 100.0))
                    };
                    if let Some(point) = boundary.safe_point(query, &mut project_temperature) {
                        selected_current_ma = point.request_current_ma;
                        Some(point.tracking_request_mv)
                    } else {
                        boundary.safe_request_mv_or_cold_start(query, &mut project_temperature)
                    }
                });
        let current_ma = selected_current_ma;
        automatic_heater_pps_candidate_with_limit(
            ch224q::PpsApdo {
                max_ma: current_ma,
                ..apdo
            },
            self.request_min_mv,
            self.request_max_mv,
            HEATER_ADJUSTABLE_MAX_MV,
            default_estimated_heater_resistance_ohms(temp_c),
            index,
            observed_limit_mv,
        )
    }

    #[cfg(test)]
    pub(crate) fn automatic_heater_request_contract(
        &self,
        temp_c: f32,
        config: &MemoryConfig,
        voltage_mv: u16,
        current_ma: u16,
        previous_mv: Option<u16>,
    ) -> Option<PdContractRequest> {
        self.automatic_heater_request_contract_with_rtd(
            temp_c,
            config,
            voltage_mv,
            current_ma,
            previous_mv,
            None,
        )
    }

    pub(crate) fn automatic_heater_request_contract_with_rtd(
        &self,
        temp_c: f32,
        config: &MemoryConfig,
        voltage_mv: u16,
        current_ma: u16,
        previous_mv: Option<u16>,
        current_rtd_mv: Option<u16>,
    ) -> Option<PdContractRequest> {
        let candidate = self.automatic_heater_pps_candidate_with_rtd(
            temp_c,
            None,
            config,
            previous_mv,
            current_rtd_mv,
        )?;
        if candidate.current_ma != current_ma
            || !(candidate.min_mv..=candidate.allowed_mv).contains(&voltage_mv)
        {
            return None;
        }
        let apdo = self.capability_apdos[candidate.advertisement_index]?;
        PdContractRequest::automatic_pps(voltage_mv, current_ma)
            .ok()?
            .within_pps_range(PpsAdjustmentRange {
                min_mv: apdo.min_mv,
                max_mv: apdo.max_mv,
                max_current_ma: apdo.max_ma,
            })
    }

    #[cfg(test)]
    pub(crate) fn automatic_heater_confirmed_pps_candidate(
        &self,
        temp_c: f32,
        config: &MemoryConfig,
        snapshot: PdServiceSnapshot,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        self.automatic_heater_confirmed_pps_candidate_with_rtd(temp_c, config, snapshot, None)
    }

    pub(crate) fn automatic_heater_confirmed_pps_candidate_with_rtd(
        &self,
        temp_c: f32,
        config: &MemoryConfig,
        snapshot: PdServiceSnapshot,
        current_rtd_mv: Option<u16>,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        let observed = snapshot.conducting_observation()?;
        let active = snapshot.active?;
        if !observed.status.pd_active
            || observed.contract.kind != ContractKind::Pps
            || active.mode != PdContractRequestMode::Pps
            || active.voltage_mv != observed.contract.voltage_mv
            || active.operating_current_ma != observed.contract.current_ma
        {
            return None;
        }
        let range = active.pps_range?;
        // During a safety transition the heater is deliberately blanked before
        // the new contract settles, so the measured source current can be near
        // zero even though the PD contract still carries the requested current.
        // Boundary validation must use that confirmed contract current; using
        // the transient load reading would reject the terminal contract and
        // leave the heater disabled until it falls back to the old voltage.
        let candidate = self
            .automatic_heater_pps_for_contract_with_rtd(
                temp_c,
                config,
                active.voltage_mv,
                active.operating_current_ma,
                Some(range),
                current_rtd_mv,
            )
            .or_else(|| {
                // Once the terminal contract is confirmed, the heater may have
                // been blanked long enough for the RTD lookup temperature to
                // fall below the compact boundary's terminal point. The contract
                // itself is still independently safe when it is the encoded
                // 21 V/2.85 A endpoint of the advertised 60 W/3 A envelope. Keep
                // the recovery path alive without accepting any other contract
                // or exceeding the source power limit.
                let terminal_power_mw =
                    u32::from(active.voltage_mv) * u32::from(active.operating_current_ma) / 1_000;
                if active.voltage_mv != AUTOMATIC_TERMINAL_VOLTAGE_MV
                    || active.operating_current_ma != AUTOMATIC_TERMINAL_CURRENT_MA
                    || self
                        .source_power_limit_mw
                        .is_none_or(|limit| terminal_power_mw > limit)
                    || self
                        .source_power_limit_ma
                        .is_none_or(|limit| limit < MAX_AUTOMATIC_HEATER_CONTRACT_MA)
                {
                    return None;
                }
                self.capability_apdos
                    .iter()
                    .enumerate()
                    .find_map(|(index, apdo)| {
                        let apdo = (*apdo)?;
                        let min_mv = apdo.min_mv.max(self.request_min_mv);
                        let max_mv = apdo.max_mv.min(self.request_max_mv);
                        (range.min_mv == apdo.min_mv
                            && range.max_mv == apdo.max_mv
                            && range.max_current_ma == apdo.max_ma
                            && (min_mv..=max_mv).contains(&active.voltage_mv)
                            && active.operating_current_ma <= apdo.max_ma)
                            .then_some(AutomaticHeaterPpsCandidate {
                                min_mv,
                                max_mv,
                                current_ma: active.operating_current_ma,
                                allowed_mv: active.voltage_mv,
                                power_mw: terminal_power_mw,
                                advertisement_index: index,
                                boundary_applied: true,
                            })
                    })
            })?;
        (candidate.min_mv..=candidate.allowed_mv)
            .contains(&active.voltage_mv)
            .then_some(candidate)
    }

    #[cfg(test)]
    pub(crate) fn automatic_heater_pps_for_contract(
        &self,
        temp_c: f32,
        memory_config: &MemoryConfig,
        voltage_mv: u16,
        current_ma: u16,
        confirmed_range: Option<PpsAdjustmentRange>,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        self.automatic_heater_pps_for_contract_with_rtd(
            temp_c,
            memory_config,
            voltage_mv,
            current_ma,
            confirmed_range,
            None,
        )
    }

    pub(crate) fn automatic_heater_pps_for_contract_with_rtd(
        &self,
        temp_c: f32,
        memory_config: &MemoryConfig,
        voltage_mv: u16,
        current_ma: u16,
        confirmed_range: Option<PpsAdjustmentRange>,
        current_rtd_mv: Option<u16>,
    ) -> Option<AutomaticHeaterPpsCandidate> {
        self.capability_apdos
            .iter()
            .enumerate()
            .find_map(|(index, apdo)| {
                let apdo = (*apdo)?;
                if confirmed_range.is_some_and(|range| {
                    range.min_mv != apdo.min_mv
                        || range.max_mv != apdo.max_mv
                        || range.max_current_ma != apdo.max_ma
                }) || !(apdo.min_mv..=apdo.max_mv).contains(&voltage_mv)
                    || current_ma > apdo.max_ma
                {
                    return None;
                }
                self.automatic_heater_pps_for_apdo(
                    apdo,
                    index,
                    temp_c,
                    memory_config,
                    current_ma,
                    AutomaticHeaterLookupContext {
                        current_request_mv: Some(voltage_mv),
                        current_rtd_mv,
                    },
                )
            })
    }

    #[cfg(test)]
    pub(crate) fn automatic_voltage_response_matches(
        &self,
        memory_config: &MemoryConfig,
        temp_c: f32,
        contract_mv: u16,
        contract_ma: u16,
        vin_raw_adc_mv: u16,
        confirmed_range: Option<PpsAdjustmentRange>,
    ) -> bool {
        self.automatic_voltage_response_matches_with_rtd(
            memory_config,
            temp_c,
            contract_mv,
            contract_ma,
            vin_raw_adc_mv,
            AutomaticVoltageResponseContext {
                confirmed_range,
                current_rtd_mv: None,
            },
        )
    }

    pub(crate) fn automatic_voltage_response_matches_with_rtd(
        &self,
        memory_config: &MemoryConfig,
        temp_c: f32,
        contract_mv: u16,
        contract_ma: u16,
        vin_raw_adc_mv: u16,
        response: AutomaticVoltageResponseContext,
    ) -> bool {
        let Some(wifi_started) = flux_purr_firmware::heater_supply::wifi_hardware_started() else {
            return false;
        };
        let Some(boundary) = memory_config.heater_supply_boundary.as_ref() else {
            return automatic_voltage_response_meets_absolute_floor(contract_mv, vin_raw_adc_mv);
        };
        let point = self.capability_apdos.iter().flatten().find_map(|apdo| {
            if response.confirmed_range.is_some_and(|range| {
                range.min_mv != apdo.min_mv
                    || range.max_mv != apdo.max_mv
                    || range.max_current_ma != apdo.max_ma
            }) {
                return None;
            }
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo
                .max_mv
                .min(self.request_max_mv)
                .min(HEATER_ADJUSTABLE_MAX_MV);
            let (source_min_mv, source_max_mv) = self.automatic_boundary_source_limits(*apdo);
            if !(min_mv..=max_mv).contains(&contract_mv) || contract_ma > apdo.max_ma {
                return None;
            }
            boundary.safe_point_or_cold_start(
                flux_purr_firmware::heater_supply::BoundaryQuery {
                    temperature_centi_c: round_to_i16(temp_c * 100.0),
                    current_rtd_mv: response.current_rtd_mv,
                    // The confirmed contract may have been authorized by the
                    // measured next point in a compacted warm-up staircase.
                    // Select that point here as well; validating a higher
                    // contract against the colder point's VIN ratio can turn
                    // ADC quantization into a false safety recovery.
                    current_request_mv: Some(contract_mv),
                    current_ma: contract_ma,
                    wifi_started,
                    source_min_mv: apdo.min_mv,
                    source_max_mv: apdo.max_mv,
                    operational_source_min_mv: source_min_mv,
                    operational_source_max_mv: source_max_mv,
                    source_max_ma: apdo.max_ma.min(MAX_AUTOMATIC_HEATER_CONTRACT_MA),
                    source_power_limit_mw: self.thermal_plant_source_power_limit_mw(),
                },
                |raw| {
                    projected_rtd_temperature_c(memory_config, raw)
                        .map(|temp| round_to_i16(temp * 100.0))
                },
            )
        });
        point.is_some_and(|point| {
            // Cold-start fallback authorizes only the coldest measured
            // tracking request. A stale contract above that point must be
            // blanked and renegotiated before physical output resumes.
            contract_mv <= point.tracking_request_mv
                && (flux_purr_firmware::heater_supply::SupplyBoundary::voltage_response_matches(
                    point,
                    contract_mv,
                    vin_raw_adc_mv,
                ) || automatic_voltage_response_meets_absolute_floor(
                    contract_mv,
                    vin_raw_adc_mv,
                ))
        })
    }

    pub(crate) fn thermal_plant_pps_range(&self) -> Option<PpsAdjustmentRange> {
        let anchor_mv = 20_000;
        let mut best = None;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(self.request_min_mv);
            let max_mv = apdo.max_mv.min(self.request_max_mv);
            if min_mv > anchor_mv || max_mv < anchor_mv || apdo.max_ma < MIN_HEATER_CONTRACT_MA {
                continue;
            }
            let candidate = PpsAdjustmentRange {
                min_mv: apdo.min_mv,
                max_mv: apdo.max_mv,
                max_current_ma: apdo.max_ma,
            };
            if best.is_none_or(|current: PpsAdjustmentRange| {
                (
                    candidate.max_current_ma,
                    candidate.max_mv,
                    core::cmp::Reverse(candidate.min_mv),
                ) > (
                    current.max_current_ma,
                    current.max_mv,
                    core::cmp::Reverse(current.min_mv),
                )
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
        let conservative_current_ma =
            self.conservative_current_for_range(minimum_mv, reachable_max_mv)?;
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
            }) {
                let current_ma = self.maximum_pps_current_for_target(checkpoint_mv)?;
                conservative_current_ma = conservative_current_ma.min(current_ma);
                current_observed = true;
            }
        }
        current_observed.then_some(conservative_current_ma)
    }

    pub(crate) fn enable(
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
        self.automatic_restore_confirmed = false;
        Ok(())
    }

    pub(crate) fn stage_pending_request(
        &mut self,
        owner: ManualPpsOwner,
        target_mv: u16,
        target_ma: u16,
    ) -> Result<(), ManualPpsError> {
        match PdContractRequest::pps(target_mv, target_ma) {
            Ok(_) => {}
            Err(
                PdContractRequestError::ZeroCurrent | PdContractRequestError::PpsCurrentNotAligned,
            ) => {
                return Err(ManualPpsError::InvalidCurrent);
            }
            Err(_) => return Err(ManualPpsError::InvalidVoltage),
        }
        self.enabled = true;
        self.owner = owner;
        self.target_mv = Some(target_mv);
        self.target_ma = Some(target_ma);
        self.applied_mv = None;
        self.error = None;
        self.automatic_restore_pending = false;
        self.automatic_restore_confirmed = false;
        Ok(())
    }

    pub(crate) fn refresh_fusb302b_capabilities_preserving_intent(
        &mut self,
        capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    ) {
        let previous = *self;
        let mut refreshed = Self::from_fusb302b_capabilities(capabilities);
        refreshed.pending_power_ticket = previous.pending_power_ticket;
        refreshed.pending_power_request_mv = previous.pending_power_request_mv;
        refreshed.supply_scan_range = previous.supply_scan_range;
        refreshed.automatic_restore_ticket = previous.automatic_restore_ticket;
        refreshed.automatic_restore_confirmed = previous.automatic_restore_confirmed;
        refreshed.automatic_restore_pending = previous.automatic_restore_pending;
        if previous.enabled {
            refreshed.enabled = true;
            refreshed.owner = previous.owner;
            refreshed.target_mv = previous.target_mv;
            refreshed.target_ma = previous.target_ma;
            refreshed.applied_mv = None;
        }
        *self = refreshed;
    }

    pub(crate) fn hold_calibration_intent_for_capability_refresh(&mut self) {
        self.pending_power_ticket = None;
        self.pending_power_request_mv = None;
        self.applied_mv = None;
        self.error = None;
        self.automatic_restore_pending = false;
        self.automatic_restore_confirmed = false;
    }

    pub(crate) fn clear(&mut self) {
        let had_override = self.enabled
            || self.target_mv.is_some()
            || self.target_ma.is_some()
            || self.applied_mv.is_some();
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.supply_scan_range = None;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = None;
        self.automatic_restore_pending |= had_override;
        if had_override {
            self.automatic_restore_confirmed = false;
        }
    }

    pub(crate) fn fail(&mut self, error: ManualPpsError) {
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.supply_scan_range = None;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = Some(error);
        self.automatic_restore_pending = true;
        self.automatic_restore_confirmed = false;
    }

    pub(crate) fn automatic_restore_phase(&self) -> AutomaticRestorePhase {
        if !self.automatic_restore_pending {
            AutomaticRestorePhase::NotRequired
        } else if self.automatic_restore_confirmed {
            AutomaticRestorePhase::ResetBackend
        } else if let Some(ticket) = self.automatic_restore_ticket {
            AutomaticRestorePhase::AwaitIdle(ticket)
        } else {
            AutomaticRestorePhase::RequestIdle
        }
    }

    pub(crate) fn complete_automatic_restore(&mut self, outcome: TicketOutcome) {
        self.automatic_restore_ticket = None;
        self.automatic_restore_confirmed = matches!(outcome, TicketOutcome::Confirmed(_));
    }

    pub(crate) fn consume_automatic_restore_pending(&mut self) -> bool {
        let pending = self.automatic_restore_pending;
        self.automatic_restore_pending = false;
        self.automatic_restore_confirmed = false;
        pending
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn manual_pps_error_code(
    error: ManualPpsError,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    error_code_string(error.code())
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn error_code_string(
    value: &str,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    let mut out = heapless::String::new();
    let _ = out.push_str(value);
    out
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackend {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::PpsMos { .. } => "pps-mos",
            Self::FixedPdPwmFallback { .. } => "fixed-pd-pwm-fallback",
        }
    }

    pub(crate) const fn pd_request_mv(self) -> u16 {
        match self {
            Self::PpsMos {
                current_request_mv, ..
            } => current_request_mv,
            Self::FixedPdPwmFallback { fixed_request, .. } => fixed_request.millivolts(),
        }
    }

    pub(crate) const fn pd_contract_mv(self) -> u16 {
        self.pd_request_mv()
    }

    pub(crate) const fn terminal_fixed_pd_disarmed(self) -> bool {
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

    pub(crate) fn set_terminal_fixed_pd_disarmed(&mut self, disarmed: bool) {
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
pub(crate) fn automatic_pps_transition_can_preserve_heater_intent(
    backend: HeaterPowerBackend,
    heater_enabled: bool,
    manual_pps_enabled: bool,
    thermal_model_allowed: bool,
) -> bool {
    heater_enabled
        && !manual_pps_enabled
        && thermal_model_allowed
        && matches!(backend, HeaterPowerBackend::PpsMos { .. })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn effective_pd_contract_mv(
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
pub(crate) fn log_ui_state(state: &FrontPanelUiState) {
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
pub(crate) fn pt1000_resistance_ohms_at(temp_c: f32) -> f32 {
    let polynomial = 1.0 + PT1000_A * temp_c + PT1000_B * temp_c * temp_c;
    if temp_c >= 0.0 {
        PT1000_R0_OHMS * polynomial
    } else {
        PT1000_R0_OHMS * (polynomial + PT1000_C * (temp_c - 100.0) * temp_c * temp_c * temp_c)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pt1000_temperature_c_from_resistance(resistance_ohms: f32) -> f32 {
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
pub(crate) fn rtd_resistance_ohms_from_mv(adc_mv: u16) -> Result<f32, HeaterFaultReason> {
    rtd_resistance_ohms_from_fractional_mv(f32::from(adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn rtd_resistance_ohms_from_fractional_mv(
    adc_mv: f32,
) -> Result<f32, HeaterFaultReason> {
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
pub(crate) fn correct_adc_fractional_mv(
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
pub(crate) fn projected_rtd_temperature_c(
    memory_config: &MemoryConfig,
    raw_adc_mv: u16,
) -> Option<f32> {
    projected_rtd_fractional_temperature_c(memory_config, f32::from(raw_adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
#[inline(never)]
pub(crate) fn projected_rtd_fractional_temperature_c(
    memory_config: &MemoryConfig,
    raw_adc_mv: f32,
) -> Option<f32> {
    let corrected_mv =
        corrected_rtd_adc_fractional_mv(memory_config, raw_adc_mv, RTD_ASSUMED_ADC_OFFSET_UV);
    rtd_resistance_ohms_from_fractional_mv(corrected_mv)
        .ok()
        .map(pt1000_temperature_c_from_resistance)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_heating_sample_covers_minimum(
    memory_config: &MemoryConfig,
    raw_adc_mv: u16,
) -> bool {
    // Persisted raw millivolts are rounded. Minimum powered coverage uses
    // the upper edge of that half-mV bin; fitting and reporting still use
    // the unchanged sample centre. The independent live cutoff stays 220C.
    projected_rtd_fractional_temperature_c(memory_config, f32::from(raw_adc_mv) + 0.5)
        .is_some_and(|temperature_c| temperature_c >= 100.0)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_runtime_wire(memory_config: &MemoryConfig) -> ThermalPlantRuntimeWire {
    let active_projection = memory_config
        .thermal_plant_transient_active
        .and_then(|transaction| {
            (thermal_plant_transient_trace_reaches_targets(&transaction, memory_config))
                .then(|| thermal_plant_projection_from_transient(&transaction))
                .flatten()
        });
    let boundary_present = memory_config
        .heater_supply_boundary
        .is_some_and(|boundary| boundary.is_valid());
    let state = if boundary_present
        && memory_config.thermal_plant_transient_active.is_some()
        && active_projection.is_none()
    {
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
            .filter(|_| boundary_present)
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
pub(crate) fn thermal_plant_run_snapshot_wire(
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
    let provisional_curve = None;
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
        supply_boundary: job
            .and_then(|job| job.supply_scan.as_ref().map(|scan| scan.boundary))
            .or(memory_config.heater_supply_boundary)
            .map(|value| Box::new(value.into())),
        version: 2,
        attempt,
        trace_page,
        provisional_curve,
        active_result,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_trace_page(
    memory_config: &MemoryConfig,
    samples: &[ThermalPlantTransientSample],
    sample_count: u8,
    after_sample: u8,
    fallback_temp_centi_c: i16,
) -> ThermalPlantTracePageWire {
    let start = after_sample.min(sample_count);
    let mut points = heapless::Vec::new();
    let mut saw_heating = samples.iter().take(usize::from(start)).any(|sample| {
        sample.duty_percent > 0
            && !flux_purr_firmware::memory::thermal_plant_sample_is_cooling(*sample)
    });
    for (offset, sample) in samples
        .iter()
        .take(usize::from(sample_count))
        .enumerate()
        .skip(usize::from(start))
        .take(flux_purr_firmware::control_plane::THERMAL_PLANT_TRACE_PAGE_MAX)
    {
        let phase = thermal_plant_trace_phase(&mut saw_heating, *sample);
        let temperature_centi_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)
            .map(|value| round_to_i16(value * 100.0))
            .unwrap_or(fallback_temp_centi_c);
        let _ = points.push(ThermalPlantTracePointWire {
            sample_index: offset as u8,
            elapsed_ms: sample
                .elapsed_ticks
                .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
            raw_rtd_adc_mv: sample.raw_rtd_adc_mv,
            temperature_centi_c,
            heater_voltage_mv: thermal_plant_heater_voltage_mv(
                flux_purr_firmware::memory::thermal_plant_sample_voltage_125mv(*sample),
            ),
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
pub(crate) fn thermal_plant_trace_phase(
    saw_heating: &mut bool,
    sample: ThermalPlantTransientSample,
) -> ThermalPlantRunPhaseWire {
    if flux_purr_firmware::memory::thermal_plant_sample_is_cooling(sample) {
        ThermalPlantRunPhaseWire::Cooling
    } else if sample.duty_percent > 0 || *saw_heating {
        *saw_heating = true;
        ThermalPlantRunPhaseWire::Heating
    } else {
        ThermalPlantRunPhaseWire::Ambient
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_active_result(
    memory_config: &MemoryConfig,
    transaction: &ThermalPlantTransientTransaction,
) -> Option<ThermalPlantActiveResultWire> {
    let (projection, _) = thermal_plant_projection_for_runtime(memory_config)?;
    Some(ThermalPlantActiveResultWire {
        transaction_id: transaction.transaction_id,
        curve: HeaterCurvePackageWire::from_memory(&HeaterCurveConfig::default(), None),
        convection_mw_per_c: Some(projection.convection_mw_per_c),
        radiation_mw_per_k4: Some(projection.radiation_mw_per_k4),
        thermal_capacity_mj_per_c: Some(projection.thermal_capacity_mj_per_c),
        transport_delay_ms: Some(projection.transport_delay_ms),
        heat_response: thermal_plant_heat_response(memory_config),
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_run_attempt(
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
        ambient_raw_rtd_adc_mv: job.ambient_raw_rtd_adc_mv,
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
pub(crate) fn thermal_plant_projection_for_runtime(
    memory_config: &MemoryConfig,
) -> Option<(flux_purr_firmware::memory::ThermalPlantProjection, f32)> {
    let transaction = memory_config.thermal_plant_transient_active?;
    if !thermal_plant_transient_trace_reaches_targets(&transaction, memory_config) {
        return None;
    }
    let projection = thermal_plant_projection_from_transient(&transaction)?;
    let ambient_temp_c =
        projected_rtd_temperature_c(memory_config, transaction.ambient_raw_rtd_adc_mv)?;
    Some((projection, ambient_temp_c))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn thermal_plant_heat_response(
    memory_config: &MemoryConfig,
) -> Option<flux_purr_firmware::thermal_response::HeatResponse> {
    use flux_purr_firmware::thermal_response::{ResponseSample, fit_shutdown_response};
    let (model, ambient_c) = thermal_plant_projection_for_runtime(memory_config)?;
    let transaction = memory_config.thermal_plant_transient_active.as_ref()?;
    let count = usize::from(transaction.sample_count);
    let mut samples = [ResponseSample::default(); THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut temperatures = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, sample) in transaction.samples[..count].iter().enumerate() {
        let temperature_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)?;
        temperatures[index] = temperature_c;
        samples[index] = ResponseSample {
            at_ms: sample.elapsed_ticks * HEATER_CONTROL_INTERVAL_MS as u32,
            temperature_c,
            power_mw: transient_sample_power_mw(*sample, temperature_c, None, memory_config)?,
            accepted: true,
        };
    }
    let accepted =
        transient_temperature_acceptance(&transaction.samples[..count], &temperatures[..count]);
    for index in 0..count {
        samples[index].accepted = accepted[index];
    }
    fit_shutdown_response(&samples[..count], model, ambient_c)
}
