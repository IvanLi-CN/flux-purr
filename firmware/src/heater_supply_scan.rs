use super::{
    BoundaryPoint, EquilibriumWindow, MIN_BOUNDARY_COVERAGE_CENTI_C, MIN_HEATED_CURRENT_MA,
    SupplyBoundary,
};

const SETTLE_MS: u32 = 1_000;
const RAPID_SETTLE_MS: u32 = 500;
const STAGE_TIMEOUT_MS: u32 = 15 * 60 * 1_000;
const MIN_THERMAL_COVERAGE_CENTI_C: i16 = 22_000;
const RAPID_MIN_COVERAGE_CENTI_C: i16 = MIN_BOUNDARY_COVERAGE_CENTI_C;
const RAPID_REPROBE_STEP_CENTI_C: i16 = 1_000;
const RAPID_COARSE_STEP_MV: u16 = 500;
const RAPID_FINE_STEP_MV: u16 = 100;
const RAPID_SOURCE_CEILING_CONFIRMATIONS: u8 = 2;
const PROBE_RECOVERY_STEP_MV: u16 = 500;
const NORMAL_PROBE_FLAT_CONFIRMATIONS: u8 = 5;
const NORMAL_PROBE_AMBIGUOUS_CONFIRMATIONS: u8 = 8;
const NORMAL_PROBE_TIMEOUT_MS: u32 = 10_000;
const NORMAL_PROBE_MIN_TEMPERATURE_STEP_CENTI_C: i16 = 1_000;
const RAPID_FINE_RETEST_WINDOW_MV: u16 = 500;
const TERMINAL_PROBE_CURRENT_STEP_MA: u16 = 50;
// A 100 mV PPS step is within the combined ADC quantization, cable drop, and
// converter regulation error at the low end of the scan. Do not classify one
// such step as a source current boundary unless the relative sag guard also
// fails. This keeps a cold, low-power load from being locked at the PPS floor.
const RAPID_MIN_BOUNDARY_SPAN_MV: u16 = 500;
// Start measured 3 A coverage before the lower-current warm-up boundary can
// become a thermal dead end. Subsequent probes are spaced by ten degrees.
const THREE_AMP_COVERAGE_START_CENTI_C: i16 = 6_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPhase {
    Baseline,
    Preflight,
    PreflightRecovery,
    Sweep,
    SourceBoundaryRecovery,
    RapidWarmup,
    TerminalProbe,
    TemperatureSettle,
    Equilibrium,
    ProbeRecovery,
    Complete,
    Failed,
}

impl ScanPhase {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Preflight => "preflight",
            Self::PreflightRecovery => "preflight_recovery",
            Self::Sweep => "sweep",
            Self::SourceBoundaryRecovery => "source_boundary_recovery",
            Self::RapidWarmup => "rapid_warmup",
            Self::TerminalProbe => "terminal_probe",
            Self::TemperatureSettle => "temperature_settle",
            Self::Equilibrium => "equilibrium",
            Self::ProbeRecovery => "probe_recovery",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanMode {
    Detailed,
    Rapid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanFailure {
    WifiChanged,
    SupplyUnstable,
    AmbiguousResponse,
    StageTimeout,
    InvalidSample,
}

impl ScanFailure {
    pub const fn code(self) -> &'static str {
        match self {
            Self::WifiChanged => "supply_scan_wifi_changed",
            Self::SupplyUnstable => "supply_scan_supply_unstable",
            Self::AmbiguousResponse => "supply_scan_ambiguous_response",
            Self::StageTimeout => "supply_scan_stage_timeout",
            Self::InvalidSample => "supply_scan_invalid_sample",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScanInput {
    pub now_ms: u32,
    pub temperature_centi_c: i16,
    pub wifi_started: bool,
    pub confirmed: bool,
    pub physical_percent: u8,
    pub rtd_code: u16,
    pub rtd_mv: u16,
    pub vin_code: u16,
    pub vin_mv: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanOutput {
    pub request_mv: u16,
    pub request_ma: u16,
    pub heater_enabled: bool,
}

#[derive(Debug)]
pub struct SupplyScan {
    pub boundary: SupplyBoundary,
    pub phase: ScanPhase,
    pub failure: Option<ScanFailure>,
    pub cutoff_reached: bool,
    request_mv: u16,
    request_ma: u16,
    changed_at_ms: u32,
    stage_started_ms: u32,
    baseline_vin_mv: u16,
    baseline_request_mv: u16,
    previous: Option<ScanInput>,
    last_tracking: Option<BoundaryPoint>,
    flat_request_mv: u16,
    flat_count: u8,
    ambiguous_count: u8,
    equilibrium: EquilibriumWindow,
    stability_temperature_centi_c: Option<i32>,
    normal_probe: bool,
    normal_probe_started_ms: u32,
    last_normal_probe_temperature_centi_c: Option<i16>,
    return_current_ma: u16,
    return_request_mv: u16,
    normal_bins: [Option<BoundaryPoint>; 20],
    rapid_tracking_confirmations: u8,
    last_rapid_probe_temperature_centi_c: Option<i16>,
    rapid_boundary_reached: bool,
    rapid_probe_since_boundary: bool,
    rapid_failed_probe_mv: Option<u16>,
    rapid_failed_probe_count: u8,
    terminal_probe_confirmations: u8,
    hot_boundary_reached: bool,
    mode: ScanMode,
}

impl SupplyScan {
    pub fn new(boundary: SupplyBoundary) -> Option<Self> {
        Self::new_with_mode(boundary, ScanMode::Detailed)
    }

    /// Build the product calibration scanner. It uses one actual 3 A PPS
    /// contract and keeps the voltage/temperature scan bounded enough for the
    /// user-facing calibration budget. The detailed multi-current scanner is
    /// retained by `new` for host-side regression coverage.
    pub fn new_rapid(boundary: SupplyBoundary) -> Option<Self> {
        Self::new_with_mode(boundary, ScanMode::Rapid)
    }

    fn new_with_mode(mut boundary: SupplyBoundary, mode: ScanMode) -> Option<Self> {
        if boundary.source_min_mv < 5_500
            || boundary.source_min_mv >= boundary.source_max_mv
            || boundary.source_max_mv > 21_000
            || boundary.source_max_ma < 3_000
        {
            return None;
        }
        boundary.point_count = 0;
        boundary.cutoff_reached = false;
        boundary.points.fill(BoundaryPoint::default());
        // Prove the 3 A contract at the highest power-safe voltage first.
        // Starting at the PPS floor can leave the board in a low-voltage
        // current loop before the scan has established a usable contract.
        Some(Self {
            request_mv: boundary.source_max_mv,
            request_ma: 3_000,
            boundary,
            phase: ScanPhase::Baseline,
            failure: None,
            cutoff_reached: false,
            changed_at_ms: 0,
            stage_started_ms: 0,
            baseline_vin_mv: 0,
            baseline_request_mv: boundary.source_max_mv,
            previous: None,
            last_tracking: None,
            flat_request_mv: 0,
            flat_count: 0,
            ambiguous_count: 0,
            equilibrium: EquilibriumWindow::default(),
            stability_temperature_centi_c: None,
            normal_probe: false,
            normal_probe_started_ms: 0,
            last_normal_probe_temperature_centi_c: None,
            return_current_ma: 3_000,
            return_request_mv: boundary.source_min_mv,
            normal_bins: [None; 20],
            rapid_tracking_confirmations: 0,
            last_rapid_probe_temperature_centi_c: None,
            rapid_boundary_reached: false,
            rapid_probe_since_boundary: false,
            rapid_failed_probe_mv: None,
            rapid_failed_probe_count: 0,
            terminal_probe_confirmations: 0,
            hot_boundary_reached: false,
            mode,
        })
    }

    fn settle_ms(&self) -> u32 {
        match (self.mode, self.phase) {
            (ScanMode::Rapid, ScanPhase::TemperatureSettle) => SETTLE_MS,
            (ScanMode::Detailed, _) => SETTLE_MS,
            (ScanMode::Rapid, _) => RAPID_SETTLE_MS,
        }
    }

    fn is_rapid(&self) -> bool {
        self.mode == ScanMode::Rapid
    }

    fn rapid_same_current_recovery(&self) -> bool {
        self.is_rapid()
            && matches!(
                self.phase,
                ScanPhase::ProbeRecovery | ScanPhase::SourceBoundaryRecovery
            )
    }

    pub fn output(&self) -> ScanOutput {
        let continuous_recovery = self.rapid_same_current_recovery();
        ScanOutput {
            request_mv: self.request_mv,
            request_ma: self.request_ma,
            heater_enabled: matches!(
                self.phase,
                ScanPhase::Sweep
                    | ScanPhase::RapidWarmup
                    | ScanPhase::TerminalProbe
                    | ScanPhase::TemperatureSettle
                    | ScanPhase::Equilibrium
            ) || continuous_recovery,
        }
    }

    fn fail(&mut self, reason: ScanFailure) {
        self.phase = ScanPhase::Failed;
        self.failure = Some(reason);
    }

    fn change(&mut self, input: ScanInput, mv: u16, ma: u16) {
        self.request_mv = mv;
        self.request_ma = ma;
        self.changed_at_ms = input.now_ms;
    }

    fn stability_temperature_centi_c(&mut self, raw_temperature_centi_c: i16) -> i16 {
        // The raw sample remains authoritative for the 220 C cutoff and is
        // persisted with each boundary point. A short fixed-point low-pass
        // is only used for thermal stability and response decisions because
        // the powered ADC batch can contain repeatable PWM-coupled spikes.
        let raw = i32::from(raw_temperature_centi_c);
        let filtered = self
            .stability_temperature_centi_c
            .map_or(raw, |previous| previous + (raw - previous) / 4);
        self.stability_temperature_centi_c = Some(filtered);
        filtered.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }

    fn point(&self, input: ScanInput) -> BoundaryPoint {
        BoundaryPoint {
            elapsed_ms: input.now_ms,
            rtd_code: input.rtd_code,
            rtd_mv: input.rtd_mv,
            vin_code: input.vin_code,
            vin_mv: input.vin_mv,
            request_current_ma: self.request_ma,
            tracking_request_mv: self.request_mv,
            flat_request_mv: self.flat_request_mv,
            tracking_only: self.normal_probe,
        }
    }

    pub fn update(&mut self, input: ScanInput) -> ScanOutput {
        if matches!(self.phase, ScanPhase::Complete | ScanPhase::Failed) {
            return self.output();
        }
        let stability_temperature_centi_c =
            self.stability_temperature_centi_c(input.temperature_centi_c);
        if input.wifi_started != self.boundary.wifi_started {
            self.fail(ScanFailure::WifiChanged);
        } else if input.rtd_mv == 0
            || input.vin_mv == 0
            || input.rtd_code > 4_095
            || input.vin_code > 4_095
        {
            self.fail(ScanFailure::InvalidSample);
        } else if input.temperature_centi_c >= 22_000 {
            self.cutoff_reached = true;
            self.boundary.cutoff_reached = true;
            self.retain_tracking();
            self.phase = ScanPhase::Complete;
        } else if self.normal_probe
            && input.now_ms.saturating_sub(self.normal_probe_started_ms) >= NORMAL_PROBE_TIMEOUT_MS
        {
            // A changing thermal load can keep the electrical response in the
            // ambiguous branch indefinitely. Do not keep a 3 A/full-power
            // probe loaded while waiting for convergence; the last tracking
            // point is already retained and the lower-current recovery path
            // is the safe next observation.
            self.finish_normal_probe(input);
        } else if input.now_ms.saturating_sub(self.stage_started_ms) > STAGE_TIMEOUT_MS {
            self.handle_stage_timeout(input);
        } else if input.temperature_centi_c >= THREE_AMP_COVERAGE_START_CENTI_C
            && self.request_ma < 3_000
            && !self.normal_probe
            && matches!(self.phase, ScanPhase::Sweep | ScanPhase::TemperatureSettle)
            && self
                .last_normal_probe_temperature_centi_c
                .is_none_or(|previous| {
                    input.temperature_centi_c.saturating_sub(previous)
                        >= NORMAL_PROBE_MIN_TEMPERATURE_STEP_CENTI_C
                })
        {
            // Lower-current stages describe the thermal response, but the
            // automatic safety boundary must also cover the hot end at the
            // actual 3 A contract. Move to that contract before the cutoff
            // can terminate the lower-current stage. An equilibrium phase is
            // allowed to finish and retain its stage before this probe.
            self.begin_three_amp_probe(input);
        } else if !input.confirmed {
            // Negotiation time is not a settled electrical observation.
            self.changed_at_ms = input.now_ms;
        } else if input.now_ms.saturating_sub(self.changed_at_ms) >= self.settle_ms() {
            self.advance(ScanInput {
                temperature_centi_c: stability_temperature_centi_c,
                ..input
            });
        }
        self.output()
    }

    fn advance(&mut self, input: ScanInput) {
        if !matches!(
            self.phase,
            ScanPhase::Baseline
                | ScanPhase::Preflight
                | ScanPhase::PreflightRecovery
                | ScanPhase::ProbeRecovery
                | ScanPhase::SourceBoundaryRecovery
        ) && input.physical_percent != 100
        {
            self.changed_at_ms = input.now_ms;
            return;
        }
        match self.phase {
            ScanPhase::Baseline => {
                if input.physical_percent != 0 {
                    self.fail(ScanFailure::SupplyUnstable);
                    return;
                }
                self.baseline_vin_mv = input.vin_mv;
                self.baseline_request_mv = self.request_mv;
                self.phase = ScanPhase::Preflight;
                self.changed_at_ms = input.now_ms;
            }
            ScanPhase::Preflight => self.preflight(input),
            ScanPhase::PreflightRecovery => {
                if input.physical_percent != 0 || input.vin_mv < self.preflight_min_vin_mv() {
                    self.fail(ScanFailure::SupplyUnstable);
                } else {
                    self.phase = ScanPhase::Sweep;
                    self.changed_at_ms = input.now_ms;
                }
            }
            ScanPhase::ProbeRecovery => {
                let continuous = self.rapid_same_current_recovery();
                if !input.confirmed
                    || (continuous && input.physical_percent != 100)
                    || (!continuous && input.physical_percent != 0)
                {
                    self.changed_at_ms = input.now_ms;
                    return;
                }
                let return_request_mv = self.return_request_mv.max(self.boundary.source_min_mv);
                if self.request_mv == return_request_mv {
                    self.phase = ScanPhase::Sweep;
                    self.changed_at_ms = input.now_ms;
                } else {
                    self.change(
                        input,
                        self.request_mv
                            .saturating_sub(PROBE_RECOVERY_STEP_MV)
                            .max(return_request_mv),
                        self.return_current_ma,
                    );
                }
            }
            ScanPhase::SourceBoundaryRecovery => self.source_boundary_recovery(input),
            ScanPhase::Sweep => self.sweep(input),
            ScanPhase::RapidWarmup => self.rapid_warmup(input),
            ScanPhase::TerminalProbe => self.terminal_probe(input),
            ScanPhase::TemperatureSettle => {
                if self
                    .equilibrium
                    .observe(input.now_ms, input.temperature_centi_c)
                {
                    self.phase = ScanPhase::Sweep;
                    self.previous = None;
                }
            }
            ScanPhase::Equilibrium => self.equilibrium(input),
            ScanPhase::Complete | ScanPhase::Failed => {}
        }
    }

    fn preflight(&mut self, input: ScanInput) {
        // The high-voltage 3 A contract is established with the heater off;
        // full-conduction load validation happens in the bounded 3 A probes.
        // Keep the scaled floor guard so an unloaded source collapse still
        // fails closed before the first heated stage.
        if input.vin_mv < self.preflight_min_vin_mv() {
            self.fail(ScanFailure::SupplyUnstable);
            return;
        }
        if let Some(previous) = self.previous {
            if u32::from(input.vin_mv) * 100 < u32::from(previous.vin_mv) * 99 {
                // A falling unloaded VIN during preflight has no safe
                // lower-current interpretation. Fail closed before any
                // heated equilibrium is recorded.
                self.fail(ScanFailure::SupplyUnstable);
                return;
            }
            // The 3 A preflight proves that this APDO can carry the product's
            // automatic-heater contract. Lower-current equilibrium stages are
            // thermal-model evidence only; automatic limits continue to use
            // the separate 3 A tracking observations.
            self.request_ma = if self.is_rapid() {
                3_000
            } else {
                MIN_HEATED_CURRENT_MA
            };
            self.phase = ScanPhase::Sweep;
            self.previous = None;
            self.normal_probe = false;
            self.return_request_mv = self.boundary.source_min_mv;
            self.last_tracking = None;
            self.flat_request_mv = 0;
            self.flat_count = 0;
            self.ambiguous_count = 0;
            self.equilibrium.reset();
            self.stage_started_ms = input.now_ms;
            self.change(input, self.boundary.source_min_mv, self.request_ma);
        } else {
            // Hold the confirmed 3 A floor for one more settled sample before
            // beginning the 2.0 A minimum heated thermal-model stage.
            self.previous = Some(input);
            self.changed_at_ms = input.now_ms;
        }
    }

    fn terminal_probe_target(&self) -> Option<(u16, u16)> {
        let target_mv = self.boundary.advertised_max_mv.min(21_000) / 100 * 100;
        if target_mv <= self.boundary.source_max_mv || self.boundary.source_power_limit_mw == 0 {
            return None;
        }
        let target_ma = (self.boundary.source_power_limit_mw * 1_000 / u32::from(target_mv)) as u16
            / TERMINAL_PROBE_CURRENT_STEP_MA
            * TERMINAL_PROBE_CURRENT_STEP_MA;
        (target_ma >= 1_000).then_some((target_mv, target_ma))
    }

    fn begin_terminal_probe(&mut self, input: ScanInput) -> bool {
        let Some((target_mv, target_ma)) = self.terminal_probe_target() else {
            return false;
        };
        self.request_mv = target_mv;
        self.request_ma = target_ma;
        self.terminal_probe_confirmations = 0;
        self.phase = ScanPhase::TerminalProbe;
        self.previous = None;
        self.changed_at_ms = input.now_ms;
        self.stage_started_ms = input.now_ms;
        true
    }

    fn begin_terminal_probe_or_complete(&mut self, input: ScanInput) {
        if !self.begin_terminal_probe(input) {
            self.phase = ScanPhase::Complete;
        }
    }

    fn terminal_probe(&mut self, input: ScanInput) {
        if !input.confirmed || input.physical_percent != 100 {
            self.changed_at_ms = input.now_ms;
            return;
        }
        if !self.voltage_tracks_request(input) {
            // A full-conduction flag alone is not evidence that the terminal
            // PPS request is tracking. The source may already be in CC while
            // the protocol still reports 100% conduction. Return to the last
            // power-safe 3 A ceiling and keep the scan alive instead of
            // persisting a false terminal observation.
            self.terminal_probe_confirmations = 0;
            self.phase = ScanPhase::SourceBoundaryRecovery;
            self.previous = None;
            self.return_request_mv = self.boundary.source_max_mv;
            self.return_current_ma = 3_000;
            self.rapid_tracking_confirmations = 0;
            self.rapid_probe_since_boundary = true;
            self.last_rapid_probe_temperature_centi_c = Some(input.temperature_centi_c);
            self.change(input, self.boundary.source_max_mv, 3_000);
            return;
        }
        self.terminal_probe_confirmations = self.terminal_probe_confirmations.saturating_add(1);
        if self.terminal_probe_confirmations >= 2 {
            let mut point = self.point(input);
            // Preserve the actual power-safe terminal contract. The runtime
            // may use this point only with the encoded current derived from
            // the source power envelope; it must never turn 21 V/2.85 A into
            // an invalid 21 V/3 A request.
            point.flat_request_mv = 0;
            point.tracking_only = true;
            if !self.boundary.retain(point) {
                self.fail(ScanFailure::SupplyUnstable);
            } else {
                self.phase = ScanPhase::Complete;
            }
        } else {
            self.changed_at_ms = input.now_ms;
        }
    }

    fn preflight_min_vin_mv(&self) -> u16 {
        let request_mv = self.baseline_request_mv.max(1);
        let scaled_floor = u32::from(self.baseline_vin_mv)
            .saturating_mul(u32::from(self.boundary.source_min_mv))
            / u32::from(request_mv);
        scaled_floor
            .saturating_mul(90)
            .div_ceil(100)
            .min(u32::from(u16::MAX)) as u16
    }

    fn begin_three_amp_probe(&mut self, input: ScanInput) {
        if self
            .last_normal_probe_temperature_centi_c
            .is_some_and(|previous| {
                input.temperature_centi_c.saturating_sub(previous)
                    < NORMAL_PROBE_MIN_TEMPERATURE_STEP_CENTI_C
            })
        {
            // Do not stack another full-conduction probe on the same thermal
            // band. Advance the lower-current sweep and let the next ten-
            // degree observation establish another bounded 3 A point.
            self.normal_probe = false;
            self.phase = ScanPhase::Sweep;
            self.previous = None;
            self.last_tracking = None;
            self.flat_count = 0;
            self.ambiguous_count = 0;
            self.flat_request_mv = 0;
            self.equilibrium.reset();
            self.stage_started_ms = input.now_ms;
            self.change(
                input,
                self.boundary.source_min_mv,
                self.request_ma.saturating_add(100).min(3_000),
            );
            return;
        }
        // Continue the next current stage from the last confirmed safe
        // voltage. Returning to the PPS floor would cool the plate and make
        // later stages measure a recovery transient instead of a boundary.
        self.return_request_mv = self.request_mv.max(self.boundary.source_min_mv);
        self.normal_probe = true;
        self.last_normal_probe_temperature_centi_c = Some(input.temperature_centi_c);
        self.return_current_ma = if self.phase == ScanPhase::Equilibrium {
            (self.request_ma + 100).min(3_000)
        } else {
            // A coverage probe can begin during the first lower-current
            // warm-up stage. Resume that same stage after unloading so the
            // required equilibrium records are still collected.
            self.request_ma
        };
        self.phase = ScanPhase::Sweep;
        self.previous = None;
        self.last_tracking = None;
        self.flat_count = 0;
        self.ambiguous_count = 0;
        self.flat_request_mv = 0;
        self.stage_started_ms = input.now_ms;
        // Start the full-conduction 3 A probe at the last lower-current
        // voltage that was already tracking. Sweep upward from there so the
        // source CC knee is observed instead of forcing an unsafe 20 V/3 A
        // load before the boundary is known.
        self.normal_probe_started_ms = input.now_ms;
        self.change(input, self.return_request_mv, 3_000);
    }

    fn sweep(&mut self, input: ScanInput) {
        let Some(previous) = self.previous else {
            self.last_tracking = Some(self.point(input));
            self.retain_normal_tracking(input);
            self.previous = Some(input);
            self.probe_next_voltage(input);
            return;
        };
        if self.is_rapid() && !self.voltage_tracks_request(input) {
            // The source current loop can begin before the advertised ceiling
            // and leave an intermediate PPS request with a large VIN sag. It
            // is already outside the measured tracking envelope; do not wait
            // for a flat-response counter or retain this request as a thermal
            // boundary point.
            self.begin_source_boundary_recovery(input);
            return;
        }
        // The rapid product scan must not treat the first sample at the
        // advertised ceiling as proof that the source can sustain the load.
        // A 3 A source may briefly report the requested voltage before its
        // current loop pulls VBUS down. Defer that ceiling sample to the
        // warm-up phase, where two consecutive tracking observations are
        // required and a sag immediately returns to the last safe point.
        if self.is_rapid() && self.request_mv >= self.boundary.source_max_mv {
            if self.voltage_tracks_request(input) {
                self.phase = ScanPhase::RapidWarmup;
                self.previous = Some(input);
                self.rapid_tracking_confirmations = 0;
                self.last_rapid_probe_temperature_centi_c = None;
                self.changed_at_ms = input.now_ms;
            } else {
                self.begin_source_boundary_recovery(input);
            }
            return;
        }
        // Keep the electrical sweep moving while the plate warms. A low
        // voltage/current pair can take minutes to approach equilibrium, and
        // waiting here would prevent the scan from reaching a higher PPS
        // voltage that provides a useful thermal observation. Temperature
        // stability is checked only after the electrical boundary is bracketed.
        let expected_delta = (u32::from(self.baseline_vin_mv) * 100)
            .div_ceil(u32::from(self.baseline_request_mv.max(1)));
        let observed_delta = u32::from(input.vin_mv.saturating_sub(previous.vin_mv));
        let thermal_ramp =
            (i32::from(input.temperature_centi_c) - i32::from(previous.temperature_centi_c)).abs()
                > 100;
        if observed_delta * 100 >= expected_delta * 80 {
            self.last_tracking = Some(self.point(input));
            self.retain_normal_tracking(input);
            self.flat_count = 0;
            self.ambiguous_count = 0;
        } else if thermal_ramp {
            // A changing thermal load can reduce the VIN delta without
            // proving that the source entered CC. Keep the electrical sweep
            // moving; only a response observed after the temperature settles
            // may contribute to the flat/ambiguous bracket.
            self.flat_count = 0;
            self.ambiguous_count = 0;
            self.previous = Some(input);
            self.probe_next_voltage(input);
            return;
        } else if observed_delta * 100 <= expected_delta * 25 {
            self.flat_count += 1;
        } else {
            self.ambiguous_count = self.ambiguous_count.saturating_add(1);
            // A PPS contract can be confirmed before the ADC has caught up
            // with the new VBUS level. Hold this request for one more settled
            // sample instead of advancing through an unclassified response.
            // No boundary point is retained until a later tracking or flat
            // observation proves which side of the bracket it belongs to.
            self.previous = Some(input);
            self.changed_at_ms = input.now_ms;
            let ambiguous_confirmations = if self.normal_probe {
                NORMAL_PROBE_AMBIGUOUS_CONFIRMATIONS
            } else {
                5
            };
            if self.ambiguous_count >= ambiguous_confirmations {
                // Treat a persistent intermediate response as the
                // conservative side of the boundary. It may be a source
                // current-limit knee or an ADC transition that never settles
                // exactly on either threshold; retaining it as a flat point
                // cannot raise the usable voltage, while aborting the whole
                // calibration would discard all earlier safe observations.
                self.handle_ambiguous_confirmation(input);
            }
            return;
        }
        self.previous = Some(input);
        let flat_confirmations = if self.normal_probe {
            NORMAL_PROBE_FLAT_CONFIRMATIONS
        } else {
            2
        };
        if self.flat_count >= flat_confirmations {
            self.handle_flat_confirmation(input);
        } else {
            self.probe_next_voltage(input);
        }
    }

    fn handle_ambiguous_confirmation(&mut self, input: ScanInput) {
        if self.rapid_flat_boundary_ready(input) {
            self.flat_request_mv = self.request_mv;
            self.begin_equilibrium(input);
            return;
        }
        // At the low end, a flat 100 mV response can be ordinary quantization
        // or cable/converter drop. Continue the bounded voltage sweep until
        // the relative sag guard or a meaningful request span confirms a
        // boundary.
        self.ambiguous_count = 0;
        self.probe_next_voltage(input);
    }

    fn handle_flat_confirmation(&mut self, input: ScanInput) {
        if !self.rapid_flat_boundary_ready(input) {
            self.flat_count = 0;
            self.probe_next_voltage(input);
            return;
        }
        self.flat_request_mv = self.request_mv;
        if let Some(point) = self.last_tracking.as_mut() {
            point.flat_request_mv = self.flat_request_mv;
        }
        self.begin_equilibrium(input);
    }

    fn probe_next_voltage(&mut self, input: ScanInput) {
        if self.request_mv >= self.boundary.source_max_mv {
            if self.is_rapid() {
                self.phase = ScanPhase::RapidWarmup;
                self.previous = None;
                self.last_rapid_probe_temperature_centi_c = None;
                self.changed_at_ms = input.now_ms;
            } else {
                self.begin_equilibrium(input);
            }
        } else {
            let step_mv = if self.is_rapid() {
                self.rapid_probe_step_mv()
            } else {
                100
            };
            self.change(
                input,
                self.request_mv
                    .saturating_add(step_mv)
                    .min(self.boundary.source_max_mv),
                self.request_ma,
            );
        }
    }

    fn rapid_probe_step_mv(&self) -> u16 {
        if !self.rapid_boundary_reached {
            return RAPID_COARSE_STEP_MV;
        }
        // Re-test the failed request at 100 mV resolution, then use coarse
        // steps as the plate warms. Only the final half-volt before the
        // advertised source ceiling needs a fine bracket; keeping every
        // warmer re-probe at 100 mV can consume the five-minute budget.
        match self.rapid_failed_probe_mv {
            None => RAPID_FINE_STEP_MV,
            Some(failed)
                if self.request_mv.saturating_add(RAPID_FINE_RETEST_WINDOW_MV) < failed =>
            {
                RAPID_COARSE_STEP_MV
            }
            Some(failed) if self.request_mv < failed => RAPID_FINE_STEP_MV,
            Some(_)
                if self.request_mv.saturating_add(RAPID_COARSE_STEP_MV)
                    >= self.boundary.source_max_mv =>
            {
                RAPID_FINE_STEP_MV
            }
            Some(_) => RAPID_COARSE_STEP_MV,
        }
    }

    fn begin_equilibrium(&mut self, input: ScanInput) {
        let Some(point) = self.last_tracking else {
            self.fail(ScanFailure::AmbiguousResponse);
            return;
        };
        if self.normal_probe {
            // A flat or ambiguous response is the 3 A boundary observation.
            // Stop loading immediately and recover the lower-current stage;
            // holding the source ceiling would turn the probe into an
            // avoidable over-power dwell on a 60 W/3 A source.
            self.finish_normal_probe(input);
            return;
        }
        if self.is_rapid() {
            self.phase = ScanPhase::RapidWarmup;
            self.previous = None;
            self.last_rapid_probe_temperature_centi_c = None;
            self.changed_at_ms = input.now_ms;
            self.request_mv = point.tracking_request_mv;
            return;
        }
        self.phase = ScanPhase::Equilibrium;
        self.equilibrium.reset();
        self.previous = Some(input);
        // One voltage step inside the observed tracking region, not a made-up
        // auxiliary-current allowance.
        self.change(
            input,
            point
                .tracking_request_mv
                .saturating_sub(100)
                .max(self.boundary.source_min_mv),
            self.request_ma,
        );
    }

    fn handle_rapid_hot_boundary(&mut self, input: ScanInput) -> bool {
        if !self.hot_boundary_reached || input.temperature_centi_c < RAPID_MIN_COVERAGE_CENTI_C {
            return false;
        }
        if self.rapid_failed_probe_count >= RAPID_SOURCE_CEILING_CONFIRMATIONS {
            // The rapid product contract is a minimum 100 C powered
            // coverage. Before cooling, a 60 W/3 A source with a 21 V APDO
            // gets one safe-current terminal probe. A successful, tracking
            // probe records the operational source ceiling for automatic
            // heating; it never authorizes the advertised 21 V endpoint.
            self.begin_terminal_probe_or_complete(input);
            return true;
        }
        if self
            .rapid_failed_probe_mv
            .is_some_and(|failed| self.request_mv < failed)
        {
            // Re-test the same hot boundary at 100 mV resolution before
            // accepting the lower recovery point.
            self.begin_rapid_voltage_reprobe(input);
            return true;
        }
        false
    }

    fn rapid_warmup(&mut self, input: ScanInput) {
        if !input.confirmed || input.physical_percent != 100 {
            self.changed_at_ms = input.now_ms;
            return;
        }
        if !self.voltage_tracks_request(input) {
            self.begin_source_boundary_recovery(input);
            return;
        }
        self.last_rapid_probe_temperature_centi_c
            .get_or_insert(input.temperature_centi_c);
        self.rapid_tracking_confirmations = self.rapid_tracking_confirmations.saturating_add(1);
        if self.rapid_tracking_confirmations >= 2 {
            self.retain_normal_tracking(input);
            if self.handle_rapid_hot_boundary(input) {
                return;
            }
            let rapid_hot_hold_ready =
                self.last_rapid_probe_temperature_centi_c
                    .is_some_and(|previous| {
                        input.temperature_centi_c.saturating_sub(previous)
                            >= RAPID_REPROBE_STEP_CENTI_C
                    });
            let rapid_reprobe_due = if self.rapid_boundary_reached {
                let failed_request_retested = self
                    .rapid_failed_probe_mv
                    .is_some_and(|failed| self.request_mv >= failed);
                // Once a failed request has been re-confirmed safe, continue
                // the bounded upward sweep immediately. A failed request
                // itself still needs the ten-degree thermal rise before it
                // is retried, so a cold source boundary cannot be chased in
                // a tight loop.
                !self.rapid_probe_since_boundary || rapid_hot_hold_ready || failed_request_retested
            } else {
                rapid_hot_hold_ready
            };
            if rapid_reprobe_due && self.request_mv < self.boundary.source_max_mv {
                // A cold CC boundary is not the hot-end boundary: the plate
                // resistance rises while warming, so the same voltage can
                // become safe again. Repeat the bounded hot re-probe at every
                // ten-degree interval from the first powered rise, until the
                // source ceiling is reached. Waiting for the minimum retained
                // coverage temperature would leave the cold-to-midrange
                // lookup on a stale low voltage and let load current fall
                // well below the 3 A contract as the plate warms.
                self.begin_rapid_voltage_reprobe(input);
            } else if input.temperature_centi_c >= RAPID_MIN_COVERAGE_CENTI_C
                && self.request_mv >= self.boundary.source_max_mv
                && rapid_hot_hold_ready
            {
                // The operational 3 A ceiling may be 20 V on a 60 W source,
                // while the APDO advertises 21 V. Capture that endpoint with
                // the power-safe current before cooling.
                self.begin_terminal_probe_or_complete(input);
            } else if !self.rapid_boundary_reached
                && self.request_mv < self.boundary.source_max_mv
                && self
                    .last_rapid_probe_temperature_centi_c
                    .is_some_and(|previous| {
                        input.temperature_centi_c.saturating_sub(previous)
                            >= RAPID_REPROBE_STEP_CENTI_C
                    })
            {
                self.begin_rapid_voltage_reprobe(input);
            } else {
                self.changed_at_ms = input.now_ms;
            }
        } else {
            self.changed_at_ms = input.now_ms;
        }
    }

    fn begin_rapid_voltage_reprobe(&mut self, input: ScanInput) {
        let next_request_mv = self
            .rapid_failed_probe_mv
            .filter(|failed| {
                *failed > self.request_mv
                    && failed.saturating_sub(self.request_mv) <= RAPID_FINE_RETEST_WINDOW_MV
            })
            .unwrap_or_else(|| self.request_mv.saturating_add(self.rapid_probe_step_mv()))
            .min(self.boundary.source_max_mv);
        if next_request_mv <= self.request_mv {
            self.phase = ScanPhase::Complete;
            return;
        }
        // Preserve the last confirmed 3 A point as the comparison baseline,
        // then advance by one PPS step. A later CC response can therefore
        // return to this exact request without lowering the measured envelope.
        self.phase = ScanPhase::Sweep;
        self.normal_probe = false;
        self.previous = Some(input);
        self.last_tracking = Some(self.point(input));
        self.flat_request_mv = 0;
        self.flat_count = 0;
        self.ambiguous_count = 0;
        self.equilibrium.reset();
        self.stage_started_ms = input.now_ms;
        self.rapid_tracking_confirmations = 0;
        // Start the next ten-degree interval at the temperature where this
        // probe begins. If the source rejects the request, recovery must wait
        // for a fresh thermal rise before trying the next 100 mV step.
        self.last_rapid_probe_temperature_centi_c = Some(input.temperature_centi_c);
        self.rapid_probe_since_boundary = true;
        self.change(input, next_request_mv, 3_000);
    }

    fn handle_stage_timeout(&mut self, input: ScanInput) {
        if self.request_ma >= 3_000 {
            self.fail(ScanFailure::StageTimeout);
            return;
        }

        // A low-current stage that never settles is not a steady-state
        // observation. It may be in Sweep after the equilibrium window was
        // restarted by continued warming, so the phase must not turn this
        // recoverable stage timeout into a calibration-wide failure. Skip it
        // without retaining a fabricated boundary point and continue the
        // bounded 100 mA sweep. The 3 A stage remains a hard timeout because
        // it is the required warm-up coverage path.
        self.phase = ScanPhase::Sweep;
        self.previous = None;
        self.last_tracking = None;
        self.flat_request_mv = 0;
        self.flat_count = 0;
        self.ambiguous_count = 0;
        self.normal_probe = false;
        self.equilibrium.reset();
        self.stage_started_ms = input.now_ms;
        self.change(
            input,
            self.boundary.source_min_mv,
            self.request_ma.saturating_add(100).min(3_000),
        );
    }

    fn equilibrium(&mut self, input: ScanInput) {
        if self
            .equilibrium
            .observe(input.now_ms, input.temperature_centi_c)
        {
            // The stable point uses raw data captured at the safe voltage.
            self.last_tracking = Some(self.point(input));
            self.retain_tracking();
            if self.request_ma == 3_000 {
                self.finish_three_amp_equilibrium(input);
            } else {
                self.begin_three_amp_probe(input);
            }
        } else if self.previous.is_some_and(|previous| {
            i32::from(input.temperature_centi_c) - i32::from(previous.temperature_centi_c) > 100
        }) {
            // Warming can release more current headroom. Rescan instead of
            // holding the obsolete cold-stage voltage until apparent balance.
            // Movement inside the accepted 1 C band must not preempt the
            // ten-second window or its independent end-to-end drift check.
            self.phase = ScanPhase::Sweep;
            self.previous = Some(input);
            self.flat_count = 0;
            self.flat_request_mv = 0;
            self.equilibrium.reset();
            self.probe_next_voltage(input);
        }
    }

    fn finish_three_amp_equilibrium(&mut self, input: ScanInput) {
        let scan_ceiling_reached = self.last_tracking.is_some_and(|point| {
            point.tracking_request_mv.saturating_add(100) >= self.boundary.source_max_mv
        });
        if input.temperature_centi_c >= MIN_THERMAL_COVERAGE_CENTI_C || scan_ceiling_reached {
            self.phase = ScanPhase::Complete;
            return;
        }

        // A low-temperature 3 A equilibrium is only a safe pause while the
        // plate resistance is still rising. Keep the same bounded stage alive
        // and resume the voltage sweep one 100 mV step above the last proven
        // tracking point; the source response continues to enforce the
        // current-loop boundary.
        self.phase = ScanPhase::Sweep;
        self.previous = None;
        self.last_tracking = None;
        self.flat_count = 0;
        self.ambiguous_count = 0;
        self.flat_request_mv = 0;
        self.equilibrium.reset();
        self.probe_next_voltage(input);
    }

    fn finish_normal_probe(&mut self, input: ScanInput) {
        self.normal_probe = false;
        self.phase = ScanPhase::ProbeRecovery;
        self.previous = None;
        self.last_tracking = None;
        self.flat_count = 0;
        self.flat_request_mv = 0;
        self.stage_started_ms = input.now_ms;
        // Change current at the observed boundary voltage while the heater is
        // already off. Only then step the unloaded contract down toward the
        // prior safe voltage; never request a 20 V/3 A loaded dwell.
        self.change(input, self.request_mv, self.return_current_ma);
    }

    fn voltage_tracks_request(&self, input: ScanInput) -> bool {
        let relative_match = self.last_tracking.is_none_or(|point| {
            SupplyBoundary::voltage_response_matches(point, self.request_mv, input.vin_mv)
        });
        let absolute_match = if self.baseline_vin_mv == 0 || self.baseline_request_mv == 0 {
            true
        } else {
            let expected_vin_mv = u32::from(self.baseline_vin_mv)
                .saturating_mul(u32::from(self.request_mv))
                .div_ceil(u32::from(self.baseline_request_mv));
            u32::from(input.vin_mv) * 100 >= expected_vin_mv * 90
        };
        relative_match && absolute_match
    }

    fn rapid_flat_boundary_ready(&self, input: ScanInput) -> bool {
        if !self.is_rapid() {
            return true;
        }
        let Some(last_tracking) = self.last_tracking else {
            return false;
        };
        self.request_mv
            .saturating_sub(last_tracking.tracking_request_mv)
            >= RAPID_MIN_BOUNDARY_SPAN_MV
            || !SupplyBoundary::voltage_response_matches(
                last_tracking,
                self.request_mv,
                input.vin_mv,
            )
    }

    fn begin_source_boundary_recovery(&mut self, input: ScanInput) {
        let failed_request_mv = self.request_mv;
        let last_safe = self
            .last_tracking
            .filter(|point| point.tracking_request_mv < failed_request_mv)
            .or_else(|| {
                self.boundary.points[..usize::from(self.boundary.point_count)]
                    .iter()
                    .copied()
                    .filter(|point| {
                        point.request_current_ma == self.request_ma
                            && point.tracking_request_mv < failed_request_mv
                    })
                    .max_by_key(|point| point.tracking_request_mv)
            });
        let recovery_step_mv = if self.is_rapid() {
            RAPID_COARSE_STEP_MV
        } else {
            PROBE_RECOVERY_STEP_MV
        };
        let coarse_recovery_mv = failed_request_mv
            .saturating_sub(recovery_step_mv)
            .max(self.boundary.source_min_mv);
        self.return_request_mv = last_safe.map_or(coarse_recovery_mv, |point| {
            point
                .tracking_request_mv
                .min(coarse_recovery_mv)
                .max(self.boundary.source_min_mv)
        });
        if self.return_request_mv >= failed_request_mv {
            self.fail(ScanFailure::SupplyUnstable);
            return;
        }
        self.return_current_ma = self.request_ma;
        self.normal_probe = false;
        // A point at the failed request may have been safe only at a colder
        // temperature. Clear it before recovery so the lower contract is
        // checked against the unloaded absolute VIN baseline instead of
        // comparing against that now-invalid point.
        self.last_tracking = None;
        if self.is_rapid() {
            // A loaded request that no longer tracks has bracketed the
            // source CC boundary. Keep the last safe point and retry the next
            // fine step after the plate warms by ten degrees. Two hot CC
            // failures establish the source ceiling; the failed voltage may
            // change as the plate resistance rises, so count the observations
            // rather than requiring the same request to fail twice.
            self.rapid_boundary_reached = true;
            if input.temperature_centi_c >= RAPID_MIN_COVERAGE_CENTI_C {
                self.rapid_failed_probe_count = self.rapid_failed_probe_count.saturating_add(1);
            } else {
                self.rapid_failed_probe_count = 0;
            }
            self.rapid_failed_probe_mv = Some(self.request_mv);
            self.hot_boundary_reached = input.temperature_centi_c >= RAPID_MIN_COVERAGE_CENTI_C;
        }
        self.rapid_tracking_confirmations = 0;
        self.phase = ScanPhase::SourceBoundaryRecovery;
        self.previous = None;
        self.flat_count = 0;
        self.ambiguous_count = 0;
        self.flat_request_mv = self.request_mv;
        // Rapid 3 A recovery keeps the same current envelope while requesting
        // the last confirmed safe voltage. The next phase waits for a settled
        // contract before resuming the 3 A warm-up. Detailed scans retain the
        // unload-before-recovery behavior through the zero-percent condition.
        self.change(input, self.return_request_mv, self.return_current_ma);
    }

    fn source_boundary_recovery(&mut self, input: ScanInput) {
        let continuous = self.rapid_same_current_recovery();
        if !input.confirmed
            || (continuous && input.physical_percent != 100)
            || (!continuous && input.physical_percent != 0)
        {
            self.changed_at_ms = input.now_ms;
            return;
        }
        if input.now_ms.saturating_sub(self.changed_at_ms) < self.settle_ms() {
            return;
        }
        self.phase = ScanPhase::RapidWarmup;
        self.previous = None;
        self.flat_request_mv = 0;
        self.rapid_tracking_confirmations = 0;
        if !self.rapid_probe_since_boundary {
            self.last_rapid_probe_temperature_centi_c = None;
        }
        self.changed_at_ms = input.now_ms;
    }

    fn retain_normal_tracking(&mut self, input: ScanInput) {
        if self.request_ma != 3_000 {
            return;
        }
        let bin = ((i32::from(input.temperature_centi_c) - 2_000).max(0) / 1_000).min(19) as usize;
        let mut point = self.point(input);
        point.tracking_only = true;
        // Store the strongest observed point in each bounded temperature bin;
        // lookup still checks its actual raw-ADC-derived temperature.
        if self.normal_bins[bin]
            .is_none_or(|previous| point.tracking_request_mv > previous.tracking_request_mv)
        {
            self.normal_bins[bin] = Some(point);
            self.rebuild_normal_points();
        }
    }

    fn rebuild_normal_points(&mut self) {
        let count = usize::from(self.boundary.point_count);
        let mut keep = 0;
        for index in 0..count {
            if !self.boundary.points[index].tracking_only {
                self.boundary.points[keep] = self.boundary.points[index];
                keep += 1;
            }
        }
        self.boundary.point_count = keep as u8;
        for point in self.normal_bins.iter().flatten() {
            self.boundary.retain(*point);
        }
    }

    fn retain_tracking(&mut self) {
        if let Some(point) = self.last_tracking
            && !self.normal_probe
        {
            self.boundary.retain(point);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heater_supply::BoundaryAdcMetadata;

    fn scan_for_timeout(current_ma: u16) -> SupplyScan {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 21_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::Equilibrium;
        scan.request_mv = 5_600;
        scan.request_ma = current_ma;
        scan.stage_started_ms = 0;
        scan.changed_at_ms = 0;
        scan.previous = Some(ScanInput {
            now_ms: 0,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 400,
            vin_mv: 450,
        });
        scan.equilibrium.reset();
        scan
    }

    fn timeout_input(now_ms: u32, output: ScanOutput, high: bool) -> ScanInput {
        ScanInput {
            now_ms,
            temperature_centi_c: if high { 10_090 } else { 9_910 },
            wifi_started: false,
            confirmed: true,
            physical_percent: u8::from(output.heater_enabled) * 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 400,
            vin_mv: 450,
        }
    }

    fn probe_input(now_ms: u32, vin_mv: u16, physical_percent: u8) -> ScanInput {
        ScanInput {
            now_ms,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_500,
            vin_mv,
        }
    }

    fn rapid_input(now_ms: u32, output: ScanOutput, temperature_centi_c: i16) -> ScanInput {
        let delivered_mv = output.request_mv.min(output.request_ma.saturating_mul(4));
        ScanInput {
            now_ms,
            temperature_centi_c,
            wifi_started: false,
            confirmed: true,
            physical_percent: u8::from(output.heater_enabled) * 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: delivered_mv / 4,
            vin_mv: delivered_mv / 12,
        }
    }

    #[test]
    fn rapid_scan_uses_only_three_amp_and_finishes_after_coverage() {
        let boundary = SupplyBoundary {
            adc: BoundaryAdcMetadata {
                divider_supply_mv: 3_328,
                ..BoundaryAdcMetadata::default()
            },
            source_min_mv: 5_500,
            source_max_mv: 12_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 12_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        for now_ms in (0..180_000).step_by(50) {
            let output = scan.output();
            assert!(output.request_ma == 3_000 || scan.phase == ScanPhase::Baseline);
            scan.update(rapid_input(
                now_ms,
                output,
                3_000 + ((now_ms / 50) as i16 * 5),
            ));
            assert_ne!(scan.phase, ScanPhase::Failed);
            if scan.phase == ScanPhase::Complete {
                assert!(scan.boundary.is_valid());
                assert!(scan.boundary.point_count > 0);
                assert!(
                    scan.boundary.points[..usize::from(scan.boundary.point_count)]
                        .iter()
                        .all(|point| point.request_current_ma == 3_000)
                );
                assert!(!scan.output().heater_enabled);
                return;
            }
        }
        panic!("rapid scan did not terminate within its bounded warm-up");
    }

    #[test]
    fn rapid_scan_does_not_hold_a_low_voltage_tracking_point() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        let mut saw_request_above_13v = false;

        for now_ms in (0..120_000).step_by(500) {
            let output = scan.output();
            if output.request_mv > 13_000 && output.heater_enabled {
                saw_request_above_13v = true;
            }
            let vin_mv = if output.heater_enabled {
                output.request_mv
            } else {
                boundary.source_max_mv
            };
            scan.update(ScanInput {
                now_ms,
                temperature_centi_c: 3_000 + (now_ms / 20) as i16,
                wifi_started: false,
                confirmed: true,
                physical_percent: u8::from(output.heater_enabled) * 100,
                rtd_code: 1_000,
                rtd_mv: 1_000,
                vin_code: (vin_mv / 5).min(4_095),
                vin_mv,
            });
            assert_ne!(scan.phase, ScanPhase::Failed);
            if scan.phase == ScanPhase::Complete {
                break;
            }
        }

        assert!(saw_request_above_13v);
        assert!(scan.boundary.point_count > 0);
        assert!(
            scan.boundary.points[..usize::from(scan.boundary.point_count)]
                .iter()
                .all(|point| point.request_current_ma == 3_000)
        );
    }

    #[test]
    fn rapid_scan_finishes_at_the_source_ceiling_after_two_real_observations() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 20_000;
        scan.request_ma = 3_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 19_800,
            tracking_request_mv: 20_000,
            request_current_ma: 3_000,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let sample = |now_ms, temperature_centi_c| ScanInput {
            now_ms,
            temperature_centi_c,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 19_800,
        };
        scan.update(sample(500, 10_000));
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        scan.update(sample(1_000, 14_000));
        assert_eq!(scan.phase, ScanPhase::Complete);
        assert!(!scan.output().heater_enabled);
    }

    #[test]
    fn rapid_scan_reprobes_the_boundary_after_temperature_rises() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_900;
        scan.request_ma = 3_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_700,
            tracking_request_mv: 13_900,
            request_current_ma: 3_000,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let first = scan.update(ScanInput {
            now_ms: 500,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert_eq!(first.request_mv, 13_900);

        let second = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 10_050,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert_eq!(second.request_mv, 13_900);

        let reprobe = scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 20_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(reprobe.request_mv, 14_400);
        assert_eq!(reprobe.request_ma, 3_000);
        assert!(reprobe.heater_enabled);
    }

    #[test]
    fn rapid_scan_coarsens_after_retesting_a_failed_probe() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.rapid_boundary_reached = true;
        scan.rapid_failed_probe_mv = Some(13_500);
        scan.request_mv = 13_000;
        assert_eq!(scan.rapid_probe_step_mv(), RAPID_FINE_STEP_MV);
        scan.request_mv = 13_500;
        assert_eq!(scan.rapid_probe_step_mv(), RAPID_COARSE_STEP_MV);
        scan.request_mv = 19_500;
        assert_eq!(scan.rapid_probe_step_mv(), RAPID_FINE_STEP_MV);

        // The initial cold probe fails at the source ceiling. It is far from
        // that failed point after recovery, so the warm-up must use coarse
        // steps until it reaches the final half-volt retest window.
        scan.rapid_failed_probe_mv = Some(20_000);
        scan.request_mv = 13_000;
        assert_eq!(scan.rapid_probe_step_mv(), RAPID_COARSE_STEP_MV);
    }

    #[test]
    fn rapid_scan_retries_nearby_failed_voltage_then_resumes_coarse_sweep() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_000;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_probe_since_boundary = true;
        scan.rapid_failed_probe_mv = Some(13_500);
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(10_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 12_900,
            request_current_ma: 3_000,
            tracking_request_mv: 13_000,
            ..BoundaryPoint::default()
        });

        let retry = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 20_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 12_900,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(retry.request_mv, 13_500);

        // Once the failed request tracks at the warmer temperature, the
        // scanner may continue with the bounded coarse step immediately.
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_500;
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(20_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_400,
            request_current_ma: 3_000,
            tracking_request_mv: 13_500,
            ..BoundaryPoint::default()
        });
        let next = scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 20_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_400,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(next.request_mv, 14_000);
    }

    #[test]
    fn rapid_scan_does_not_call_a_single_low_voltage_flat_step_a_boundary() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::Sweep;
        scan.request_mv = 5_700;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.baseline_vin_mv = 19_800;
        scan.baseline_request_mv = 20_000;
        scan.changed_at_ms = 0;
        scan.previous = Some(probe_input(0, 5_594, 100));
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 5_594,
            tracking_request_mv: 5_600,
            request_current_ma: 3_000,
            ..BoundaryPoint::default()
        });

        let first = scan.update(probe_input(1_000, 5_594, 100));
        assert!(first.heater_enabled);
        let second = scan.update(probe_input(2_000, 5_594, 100));

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(second.request_mv, 5_900);
        assert!(second.heater_enabled);
        assert_eq!(scan.boundary.point_count, 0);
    }

    #[test]
    fn rapid_scan_returns_to_last_safe_voltage_when_source_ceiling_sags() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 20_000;
        scan.request_ma = 3_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_160,
            request_current_ma: 3_000,
            tracking_request_mv: 13_400,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let recovery = scan.update(ScanInput {
            now_ms: 500,
            temperature_centi_c: 6_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_000,
        });
        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(recovery.request_mv, 13_400);
        assert!(recovery.heater_enabled);
        assert_eq!(scan.boundary.point_count, 0);

        let resumed = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 6_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_400,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert_eq!(resumed.request_mv, 13_400);
        assert!(resumed.heater_enabled);

        let first_safe = scan.update(ScanInput {
            now_ms: 1_500,
            temperature_centi_c: 6_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_200,
        });
        assert_eq!(scan.boundary.point_count, 0);
        assert!(first_safe.heater_enabled);

        scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 6_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_200,
        });
        assert_eq!(scan.boundary.point_count, 1);
        assert_eq!(scan.boundary.points[0].tracking_request_mv, 13_400);
    }

    #[test]
    fn rapid_scan_rejects_absolute_vin_sag_against_unloaded_baseline() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 19_500;
        scan.request_ma = 3_000;
        scan.baseline_request_mv = 20_000;
        scan.baseline_vin_mv = 20_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 16_000,
            request_current_ma: 3_000,
            tracking_request_mv: 19_500,
            ..BoundaryPoint::default()
        });
        scan.boundary.points[0] = BoundaryPoint {
            vin_mv: 18_900,
            request_current_ma: 3_000,
            tracking_request_mv: 19_000,
            ..BoundaryPoint::default()
        };
        scan.boundary.point_count = 1;

        let output = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 12_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 16_000,
        });
        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(output.request_mv, 19_000);
        assert!(output.heater_enabled);
    }

    #[test]
    fn rapid_scan_recovery_steps_down_when_failed_point_is_the_only_last_tracking() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 19_500;
        scan.request_ma = 3_000;
        scan.baseline_request_mv = 20_000;
        scan.baseline_vin_mv = 20_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 19_400,
            request_current_ma: 3_000,
            tracking_request_mv: 19_500,
            ..BoundaryPoint::default()
        });

        let output = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 12_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 16_000,
        });

        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(output.request_mv, 19_000);
        assert!(output.heater_enabled);
        assert!(scan.last_tracking.is_none());
    }

    #[test]
    fn rapid_scan_finishes_after_recovering_from_source_boundary_with_hot_coverage() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 20_000;
        scan.request_ma = 3_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_160,
            request_current_ma: 3_000,
            tracking_request_mv: 13_400,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let recovery = scan.update(ScanInput {
            now_ms: 500,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_000,
        });
        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(recovery.request_mv, 13_400);
        assert!(recovery.heater_enabled);

        let resumed = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_400,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert_eq!(resumed.request_mv, 13_400);
        assert!(resumed.heater_enabled);

        scan.update(ScanInput {
            now_ms: 1_500,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_200,
        });
        let reprobe = scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 10_050,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_200,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(reprobe.request_mv, 13_900);
        assert_eq!(reprobe.request_ma, 3_000);
        assert!(reprobe.heater_enabled);
        assert_eq!(scan.boundary.point_count, 1);
        assert_eq!(scan.boundary.points[0].tracking_request_mv, 13_400);
    }

    #[test]
    fn rapid_scan_reprobes_hot_voltage_after_cold_boundary() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_900;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_700,
            request_current_ma: 3_000,
            tracking_request_mv: 13_900,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        scan.update(ScanInput {
            now_ms: 500,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });
        let reprobe = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 10_050,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(reprobe.request_mv, 14_000);
        assert_eq!(reprobe.request_ma, 3_000);
        assert!(reprobe.heater_enabled);
    }

    #[test]
    fn rapid_scan_reprobes_after_each_hot_interval_until_source_ceiling() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_900;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(10_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_700,
            request_current_ma: 3_000,
            tracking_request_mv: 13_900,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let first_reprobe = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 11_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_700,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(first_reprobe.request_mv, 14_000);
        assert_eq!(scan.last_rapid_probe_temperature_centi_c, Some(11_000));

        // A second ten-degree interval must create another measured probe.
        // The previous implementation completed here after a single hot
        // re-probe and left this higher-temperature region unmeasured.
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 14_000;
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(11_000);
        scan.stability_temperature_centi_c = Some(12_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_800,
            request_current_ma: 3_000,
            tracking_request_mv: 14_000,
            ..BoundaryPoint::default()
        });
        let second_reprobe = scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 12_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_800,
        });
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(second_reprobe.request_mv, 14_100);
        assert_eq!(second_reprobe.request_ma, 3_000);
        assert!(second_reprobe.heater_enabled);
    }

    #[test]
    fn rapid_scan_keeps_hot_probe_spacing_across_boundary_recovery() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 14_000;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_probe_since_boundary = true;
        scan.last_rapid_probe_temperature_centi_c = Some(12_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_800,
            request_current_ma: 3_000,
            tracking_request_mv: 14_000,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let recovery = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 12_500,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 12_000,
        });

        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(recovery.request_mv, 13_500);
        assert!(scan.rapid_probe_since_boundary);

        let resumed = scan.update(ScanInput {
            now_ms: 2_000,
            temperature_centi_c: 12_500,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 14_000,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert!(resumed.heater_enabled);
        assert_eq!(scan.last_rapid_probe_temperature_centi_c, Some(12_000));
    }

    #[test]
    fn rapid_scan_reprobes_before_minimum_coverage_after_cold_boundary() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 13_000;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_probe_since_boundary = true;
        scan.rapid_failed_probe_mv = Some(13_500);
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(3_400);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 12_900,
            request_current_ma: 3_000,
            tracking_request_mv: 13_000,
            ..BoundaryPoint::default()
        });

        let reprobe = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 4_400,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 12_900,
        });

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(reprobe.request_mv, 13_500);
        assert_eq!(reprobe.request_ma, 3_000);
        assert!(reprobe.heater_enabled);
    }

    #[test]
    fn rapid_scan_does_not_end_after_two_hot_failures_below_source_ceiling() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 18_900;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_probe_since_boundary = true;
        scan.last_rapid_probe_temperature_centi_c = Some(10_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 18_800,
            request_current_ma: 3_000,
            tracking_request_mv: 18_800,
            ..BoundaryPoint::default()
        });

        let first = ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 10_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 17_600,
        };
        scan.begin_source_boundary_recovery(first);
        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(scan.rapid_failed_probe_count, 1);

        let recovered = ScanInput {
            now_ms: 2_000,
            physical_percent: 100,
            vin_mv: 18_800,
            ..first
        };
        scan.source_boundary_recovery(recovered);
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);

        scan.request_mv = 19_000;
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 18_800,
            request_current_ma: 3_000,
            tracking_request_mv: 18_800,
            ..BoundaryPoint::default()
        });
        let second = ScanInput {
            now_ms: 3_000,
            temperature_centi_c: 11_000,
            ..first
        };
        scan.begin_source_boundary_recovery(second);
        assert_eq!(scan.rapid_failed_probe_count, 2);
        assert!(scan.output().heater_enabled);

        scan.source_boundary_recovery(ScanInput {
            now_ms: 4_000,
            physical_percent: 100,
            vin_mv: 18_800,
            ..second
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert!(scan.output().heater_enabled);
        assert!(scan.output().request_mv < scan.boundary.source_max_mv);
    }

    #[test]
    fn rapid_scan_keeps_a_hot_hold_after_the_final_boundary() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid rapid scan boundary");
        scan.phase = ScanPhase::RapidWarmup;
        scan.request_mv = 20_000;
        scan.request_ma = 3_000;
        scan.rapid_boundary_reached = true;
        scan.rapid_tracking_confirmations = 1;
        scan.last_rapid_probe_temperature_centi_c = Some(10_000);
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 13_800,
            request_current_ma: 3_000,
            tracking_request_mv: 20_000,
            ..BoundaryPoint::default()
        });
        scan.changed_at_ms = 0;

        let first = scan.update(ScanInput {
            now_ms: 500,
            temperature_centi_c: 10_050,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_800,
        });
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert!(first.heater_enabled);

        let held = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 14_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_000,
            vin_mv: 13_800,
        });
        assert_eq!(scan.phase, ScanPhase::Complete);
        assert!(!held.heater_enabled);
    }

    #[test]
    fn final_three_amp_low_equilibrium_resumes_voltage_scan() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::Equilibrium;
        scan.request_mv = 6_900;
        scan.request_ma = 3_000;
        scan.last_tracking = Some(BoundaryPoint {
            tracking_request_mv: 7_000,
            request_current_ma: 3_000,
            ..BoundaryPoint::default()
        });
        scan.equilibrium.started_ms = Some(0);
        scan.equilibrium.min_centi_c = 8_000;
        scan.equilibrium.max_centi_c = 8_000;

        let output = scan.update(ScanInput {
            now_ms: 10_000,
            temperature_centi_c: 8_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 500,
            vin_mv: 500,
        });

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(output.request_mv, 7_000);
        assert_eq!(output.request_ma, 3_000);
        assert!(output.heater_enabled);
    }

    #[test]
    fn scan_starts_three_amp_preflight_at_power_safe_ceiling() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let scan = SupplyScan::new(boundary).expect("valid scan boundary");

        assert_eq!(scan.output().request_mv, 20_000);
        assert_eq!(scan.output().request_ma, 3_000);
        assert!(!scan.output().heater_enabled);
    }

    #[test]
    fn early_three_amp_probe_preserves_lower_current_stage() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::Sweep;
        scan.request_mv = 9_000;
        scan.request_ma = 2_000;
        scan.changed_at_ms = 0;
        scan.stage_started_ms = 0;

        let output = scan.update(ScanInput {
            now_ms: 1_000,
            temperature_centi_c: 6_000,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_350,
            vin_mv: 9_000,
        });

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert!(scan.normal_probe);
        assert_eq!(scan.return_current_ma, 2_000);
        assert_eq!(output.request_mv, 9_000);
        assert_eq!(output.request_ma, 3_000);
        assert!(output.heater_enabled);
    }

    #[test]
    fn three_amp_probe_sweeps_from_last_safe_voltage_and_unloads_at_cc_knee() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::Sweep;
        scan.request_mv = 12_300;
        scan.request_ma = 3_000;
        scan.normal_probe = true;
        scan.return_current_ma = 2_400;
        scan.return_request_mv = 12_300;
        scan.baseline_vin_mv = 12_000;
        scan.baseline_request_mv = 12_300;
        scan.changed_at_ms = 0;
        scan.stage_started_ms = 0;

        let first = scan.update(probe_input(1_000, 12_300, 100));

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert!(scan.normal_probe);
        assert_eq!(first.request_mv, 12_400);
        assert_eq!(first.request_ma, 3_000);
        assert!(first.heater_enabled);

        let second = scan.update(probe_input(2_000, 12_400, 100));

        assert_eq!(second.request_mv, 12_500);
        assert_eq!(second.request_ma, 3_000);
        assert!(second.heater_enabled);

        let tracking = scan.update(probe_input(3_000, 12_400, 100));

        assert_eq!(tracking.request_mv, 12_600);
        assert_eq!(tracking.request_ma, 3_000);
        assert!(tracking.heater_enabled);

        for now_ms in [4_000, 5_000, 6_000] {
            let output = scan.update(probe_input(now_ms, 12_400, 100));

            assert_eq!(scan.phase, ScanPhase::Sweep);
            assert_eq!(output.request_ma, 3_000);
            assert!(output.heater_enabled);
        }

        let finished = scan.update(probe_input(7_000, 12_400, 100));

        assert_eq!(scan.phase, ScanPhase::ProbeRecovery);
        assert!(!scan.normal_probe);
        assert_eq!(finished.request_mv, 12_900);
        assert_eq!(finished.request_ma, 2_400);
        assert!(!finished.heater_enabled);

        let recovered = scan.update(probe_input(8_000, 12_400, 0));

        assert_eq!(scan.phase, ScanPhase::ProbeRecovery);
        assert_eq!(recovered.request_mv, 12_400);
        assert_eq!(recovered.request_ma, 2_400);
        assert!(!recovered.heater_enabled);

        let resumed = scan.update(probe_input(9_000, 12_400, 0));

        assert_eq!(scan.phase, ScanPhase::ProbeRecovery);
        assert_eq!(resumed.request_mv, 12_300);
        assert_eq!(resumed.request_ma, 2_400);
        assert!(!resumed.heater_enabled);

        let resumed = scan.update(probe_input(10_000, 12_400, 0));

        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(resumed.request_mv, 12_300);
        assert_eq!(resumed.request_ma, 2_400);
        assert!(resumed.heater_enabled);
    }

    #[test]
    fn three_amp_probe_timeout_unloads_when_temperature_keeps_rising() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new(boundary).expect("valid scan boundary");
        scan.phase = ScanPhase::Sweep;
        scan.request_mv = 19_900;
        scan.request_ma = 3_000;
        scan.normal_probe = true;
        scan.normal_probe_started_ms = 1_000;
        scan.return_current_ma = 2_900;
        scan.return_request_mv = 19_900;
        scan.stage_started_ms = 1_000;

        let output = scan.update(ScanInput {
            now_ms: 11_001,
            temperature_centi_c: 17_500,
            wifi_started: false,
            confirmed: true,
            physical_percent: 100,
            rtd_code: 1_000,
            rtd_mv: 1_000,
            vin_code: 1_900,
            vin_mv: 19_800,
        });

        assert_eq!(scan.phase, ScanPhase::ProbeRecovery);
        assert!(!scan.normal_probe);
        assert_eq!(output.request_mv, 19_900);
        assert_eq!(output.request_ma, 2_900);
        assert!(!output.heater_enabled);
    }

    #[test]
    fn low_current_stage_timeout_skips_without_retaining_a_steady_point() {
        let mut scan = scan_for_timeout(1_300);
        for tick in 1..=18_002_u32 {
            let now_ms = tick * 50;
            let output = scan.output();
            scan.update(timeout_input(
                now_ms,
                output,
                (tick / 200).is_multiple_of(2),
            ));
            if scan.phase == ScanPhase::Sweep && scan.request_ma == 1_400 {
                assert_eq!(scan.boundary.point_count, 0);
                assert_eq!(scan.failure, None);
                return;
            }
        }
        panic!(
            "low-current equilibrium timeout did not advance: phase={:?}, current={}mA",
            scan.phase, scan.request_ma
        );
    }

    #[test]
    fn low_current_sweep_timeout_skips_after_equilibrium_restarts() {
        let mut scan = scan_for_timeout(2_300);
        scan.phase = ScanPhase::Sweep;
        scan.previous = None;

        let now_ms = 15 * 60 * 1_000 + 1;
        let input = timeout_input(now_ms, scan.output(), false);
        let output = scan.update(input);

        assert_eq!(scan.failure, None);
        assert_eq!(scan.phase, ScanPhase::Sweep);
        assert_eq!(output.request_ma, 2_400);
        assert!(output.heater_enabled);
    }

    #[test]
    fn three_amp_stage_timeout_still_fails_closed() {
        let mut scan = scan_for_timeout(3_000);
        for tick in 1..=18_002_u32 {
            let now_ms = tick * 50;
            let output = scan.output();
            scan.update(timeout_input(
                now_ms,
                output,
                (tick / 200).is_multiple_of(2),
            ));
            if scan.phase == ScanPhase::Failed {
                assert_eq!(scan.failure, Some(ScanFailure::StageTimeout));
                return;
            }
        }
        panic!("3 A equilibrium timeout did not fail closed");
    }

    #[test]
    fn sixty_watt_scan_exposes_a_safe_21v_terminal_probe() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            source_power_limit_mw: 60_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let scan = SupplyScan::new_rapid(boundary).expect("valid 60 W scan boundary");

        assert_eq!(scan.terminal_probe_target(), Some((21_000, 2_850)));
    }

    #[test]
    fn terminal_probe_requires_voltage_tracking_and_records_power_safe_endpoint() {
        let boundary = SupplyBoundary {
            source_min_mv: 5_500,
            source_max_mv: 20_000,
            source_max_ma: 3_000,
            source_power_limit_mw: 60_000,
            advertised_min_mv: 5_500,
            advertised_max_mv: 21_000,
            ..SupplyBoundary::default()
        };
        let mut scan = SupplyScan::new_rapid(boundary).expect("valid 60 W scan boundary");
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 19_500,
            request_current_ma: 3_000,
            tracking_request_mv: 20_000,
            ..BoundaryPoint::default()
        });
        assert!(scan.begin_terminal_probe(probe_input(1_000, 19_000, 0)));
        assert_eq!(scan.output().request_mv, 21_000);
        assert_eq!(scan.output().request_ma, 2_850);
        assert!(scan.output().heater_enabled);

        let first = scan.update(probe_input(1_500, 15_000, 100));
        assert_eq!(scan.phase, ScanPhase::SourceBoundaryRecovery);
        assert_eq!(first.request_mv, 20_000);
        assert_eq!(first.request_ma, 3_000);
        assert!(first.heater_enabled);

        let recovery = scan.update(probe_input(2_000, 19_500, 100));
        assert_eq!(scan.phase, ScanPhase::RapidWarmup);
        assert_eq!(recovery.request_mv, 20_000);
        assert_eq!(recovery.request_ma, 3_000);

        let mut scan = SupplyScan::new_rapid(boundary).expect("valid 60 W scan boundary");
        scan.last_tracking = Some(BoundaryPoint {
            vin_mv: 19_500,
            request_current_ma: 3_000,
            tracking_request_mv: 20_000,
            ..BoundaryPoint::default()
        });
        assert!(scan.begin_terminal_probe(probe_input(3_000, 19_000, 0)));
        let first = scan.update(probe_input(3_500, 20_500, 100));
        assert_eq!(scan.phase, ScanPhase::TerminalProbe);
        assert!(first.heater_enabled);
        let second = scan.update(probe_input(4_000, 20_500, 100));
        assert_eq!(scan.phase, ScanPhase::Complete);
        assert!(!second.heater_enabled);
        assert_eq!(scan.boundary.point_count, 1);
        let point = scan.boundary.points[0];
        assert_eq!(point.tracking_request_mv, 21_000);
        assert_eq!(point.request_current_ma, 2_850);
        assert!(point.tracking_only);
    }
}
