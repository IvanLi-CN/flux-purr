use super::{BoundaryPoint, EquilibriumWindow, SupplyBoundary};

const SETTLE_MS: u32 = 1_000;
const STAGE_TIMEOUT_MS: u32 = 15 * 60 * 1_000;
const MIN_THERMAL_COVERAGE_CENTI_C: i16 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPhase {
    Baseline,
    Preflight,
    PreflightRecovery,
    Sweep,
    TemperatureSettle,
    Equilibrium,
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
    previous: Option<ScanInput>,
    last_tracking: Option<BoundaryPoint>,
    flat_request_mv: u16,
    flat_count: u8,
    ambiguous_count: u8,
    equilibrium: EquilibriumWindow,
    stability_temperature_centi_c: Option<i32>,
    normal_probe: bool,
    return_current_ma: u16,
    normal_probe_started_ms: u32,
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
        Some(Self {
            request_mv: boundary.source_min_mv,
            request_ma: 3_000,
            boundary,
            phase: ScanPhase::Baseline,
            failure: None,
            cutoff_reached: false,
            changed_at_ms: 0,
            stage_started_ms: 0,
            baseline_vin_mv: 0,
            previous: None,
            last_tracking: None,
            flat_request_mv: 0,
            flat_count: 0,
            ambiguous_count: 0,
            equilibrium: EquilibriumWindow::default(),
            stability_temperature_centi_c: None,
            normal_probe: false,
            return_current_ma: 3_000,
            normal_probe_started_ms: 0,
            normal_bins: [None; 20],
        })
    }

    pub fn output(&self) -> ScanOutput {
        ScanOutput {
            request_mv: self.request_mv,
            request_ma: self.request_ma,
            heater_enabled: matches!(
                self.phase,
                ScanPhase::Preflight
                    | ScanPhase::Sweep
                    | ScanPhase::TemperatureSettle
                    | ScanPhase::Equilibrium
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
        } else if input.temperature_centi_c >= 22_000 {
            self.cutoff_reached = true;
            self.boundary.cutoff_reached = true;
            self.retain_tracking();
            self.phase = ScanPhase::Complete;
        } else if input.rtd_mv == 0
            || input.vin_mv == 0
            || input.rtd_code > 4_095
            || input.vin_code > 4_095
        {
            self.fail(ScanFailure::InvalidSample);
        } else if input.now_ms.saturating_sub(self.stage_started_ms) > STAGE_TIMEOUT_MS {
            self.handle_stage_timeout(input);
        } else if self.normal_probe
            && input.now_ms.saturating_sub(self.normal_probe_started_ms) >= 10_000
        {
            self.finish_normal_probe(input);
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
            ScanPhase::Baseline | ScanPhase::PreflightRecovery
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
                self.phase = ScanPhase::Preflight;
                self.changed_at_ms = input.now_ms;
            }
            ScanPhase::Preflight => self.preflight(input),
            ScanPhase::PreflightRecovery => {
                if input.physical_percent != 0
                    || u32::from(input.vin_mv) * 100 < u32::from(self.baseline_vin_mv) * 90
                {
                    self.fail(ScanFailure::SupplyUnstable);
                } else {
                    self.phase = ScanPhase::Sweep;
                    self.changed_at_ms = input.now_ms;
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
        if let Some(previous) = self.previous
            && u32::from(input.vin_mv) * 100 < u32::from(previous.vin_mv) * 99
        {
            // A current-loop knee can cross the baseline floor in one step.
            // Unload before restoring the last proven tracking current.
            self.change(input, self.boundary.source_min_mv, self.request_ma + 100);
            self.phase = ScanPhase::PreflightRecovery;
            self.previous = None;
            self.stage_started_ms = input.now_ms;
            return;
        }
        // Without a proven tracking current there is no safe recovery target.
        // Relative ADC response only: no conversion into a fictitious true VIN.
        if u32::from(input.vin_mv) * 100 < u32::from(self.baseline_vin_mv) * 90 {
            self.fail(ScanFailure::SupplyUnstable);
            return;
        }
        if self.request_ma == 1_000 {
            self.phase = ScanPhase::Sweep;
            self.previous = None;
        } else {
            self.previous = Some(input);
            self.change(input, self.request_mv, self.request_ma - 100);
        }
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
            .div_ceil(u32::from(self.boundary.source_min_mv));
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
            if self.ambiguous_count >= 5 {
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
        if self.flat_count >= 2 {
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
        if self.phase != ScanPhase::Equilibrium || self.request_ma >= 3_000 {
            self.fail(ScanFailure::StageTimeout);
            return;
        }

        // A low-current stage that never settles is not a steady-state
        // observation. Skip it without retaining a fabricated boundary point
        // and continue the bounded 100 mA sweep. The 3 A stage remains a hard
        // timeout because it is the required warm-up coverage path.
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
                self.normal_probe = true;
                self.normal_probe_started_ms = input.now_ms;
                self.return_current_ma = self.request_ma + 100;
                self.phase = ScanPhase::Sweep;
                self.previous = None;
                self.last_tracking = None;
                self.flat_count = 0;
                self.flat_request_mv = 0;
                self.stage_started_ms = input.now_ms;
                self.change(input, self.request_mv, 3_000);
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
        self.phase = ScanPhase::Sweep;
        self.previous = None;
        self.last_tracking = None;
        self.flat_count = 0;
        self.flat_request_mv = 0;
        self.stage_started_ms = input.now_ms;
        self.change(input, self.boundary.source_min_mv, self.return_current_ma);
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
