use super::{BoundaryPoint, EquilibriumWindow, MIN_HEATED_CURRENT_MA, SupplyBoundary};

const SETTLE_MS: u32 = 1_000;
const STAGE_TIMEOUT_MS: u32 = 15 * 60 * 1_000;
const MIN_THERMAL_COVERAGE_CENTI_C: i16 = 22_000;
const PROBE_RECOVERY_STEP_MV: u16 = 500;
const NORMAL_PROBE_FLAT_CONFIRMATIONS: u8 = 5;
const NORMAL_PROBE_AMBIGUOUS_CONFIRMATIONS: u8 = 8;
const NORMAL_PROBE_TIMEOUT_MS: u32 = 10_000;
// Finish lower-current thermal stages early enough to establish the required
// 3 A safety boundary before the hard 220 C cutoff can end powered sampling.
const LOWER_CURRENT_STAGE_EXIT_CENTI_C: i16 = 18_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPhase {
    Baseline,
    Preflight,
    PreflightRecovery,
    Sweep,
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
            Self::TemperatureSettle => "temperature_settle",
            Self::Equilibrium => "equilibrium",
            Self::ProbeRecovery => "probe_recovery",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
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
    return_current_ma: u16,
    return_request_mv: u16,
    normal_bins: [Option<BoundaryPoint>; 20],
}

impl SupplyScan {
    pub fn new(mut boundary: SupplyBoundary) -> Option<Self> {
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
            return_current_ma: 3_000,
            return_request_mv: boundary.source_min_mv,
            normal_bins: [None; 20],
        })
    }

    pub fn output(&self) -> ScanOutput {
        ScanOutput {
            request_mv: self.request_mv,
            request_ma: self.request_ma,
            heater_enabled: matches!(
                self.phase,
                ScanPhase::Sweep | ScanPhase::TemperatureSettle | ScanPhase::Equilibrium
            ),
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
        } else if input.temperature_centi_c >= LOWER_CURRENT_STAGE_EXIT_CENTI_C
            && self.request_ma < 3_000
            && !self.normal_probe
        {
            // Lower-current stages describe the thermal response, but the
            // automatic safety boundary must also cover the hot end at the
            // actual 3 A contract. Move to that contract before the cutoff
            // can terminate the lower-current stage.
            self.begin_three_amp_probe(input);
        } else if self.normal_probe
            && input.now_ms.saturating_sub(self.normal_probe_started_ms) >= NORMAL_PROBE_TIMEOUT_MS
        {
            // A changing thermal load can keep the electrical response in the
            // ambiguous branch indefinitely. Do not keep a 3 A/full-power
            // probe loaded while waiting for convergence; the last tracking
            // point is already retained and the lower-current recovery path is
            // the safe next observation.
            self.finish_normal_probe(input);
        } else if input.now_ms.saturating_sub(self.stage_started_ms) > STAGE_TIMEOUT_MS {
            self.handle_stage_timeout(input);
        } else if !input.confirmed {
            // Negotiation time is not a settled electrical observation.
            self.changed_at_ms = input.now_ms;
        } else if input.now_ms.saturating_sub(self.changed_at_ms) >= SETTLE_MS {
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
                if input.physical_percent != 0 || !input.confirmed {
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
            ScanPhase::Sweep => self.sweep(input),
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
            self.request_ma = MIN_HEATED_CURRENT_MA;
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
        // Continue the next current stage from the last confirmed safe
        // voltage. Returning to the PPS floor would cool the plate and make
        // later stages measure a recovery transient instead of a boundary.
        self.return_request_mv = self.request_mv.max(self.boundary.source_min_mv);
        self.normal_probe = true;
        self.return_current_ma = (self.request_ma + 100).min(3_000);
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
                self.flat_request_mv = self.request_mv;
                self.begin_equilibrium(input);
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
            self.flat_request_mv = self.request_mv;
            if let Some(point) = self.last_tracking.as_mut() {
                point.flat_request_mv = self.flat_request_mv;
            }
            self.begin_equilibrium(input);
        } else {
            self.probe_next_voltage(input);
        }
    }

    fn probe_next_voltage(&mut self, input: ScanInput) {
        if self.request_mv >= self.boundary.source_max_mv {
            self.begin_equilibrium(input);
        } else {
            self.change(input, self.request_mv + 100, self.request_ma);
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
}
