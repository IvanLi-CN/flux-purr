use super::{BoundaryPoint, EquilibriumWindow, SupplyBoundary};

const SETTLE_MS: u32 = 1_000;
const STAGE_TIMEOUT_MS: u32 = 15 * 60 * 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPhase {
    Baseline,
    Preflight,
    Sweep,
    TemperatureSettle,
    Equilibrium,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanFailure {
    WifiChanged,
    SupplyUnstable,
    AmbiguousResponse,
    StageTimeout,
    InvalidSample,
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
            self.fail(ScanFailure::StageTimeout);
        } else if self.normal_probe
            && input.now_ms.saturating_sub(self.normal_probe_started_ms) >= 10_000
        {
            self.finish_normal_probe(input);
        } else if !input.confirmed {
            // Negotiation time is not a settled electrical observation.
            self.changed_at_ms = input.now_ms;
        } else if input.now_ms.saturating_sub(self.changed_at_ms) >= SETTLE_MS {
            self.advance(input);
        }
        self.output()
    }

    fn advance(&mut self, input: ScanInput) {
        if self.phase != ScanPhase::Baseline && input.physical_percent != 100 {
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
        // Relative ADC response only: no conversion into a fictitious true VIN.
        if u32::from(input.vin_mv) * 100 < u32::from(self.baseline_vin_mv) * 90 {
            self.fail(ScanFailure::SupplyUnstable);
            return;
        }
        if let Some(previous) = self.previous
            && u32::from(input.vin_mv) * 100 < u32::from(previous.vin_mv) * 99
        {
            // Return to the last tracking current. Never force the 1 A stage.
            self.change(input, self.boundary.source_min_mv, self.request_ma + 100);
            self.phase = ScanPhase::Sweep;
            self.previous = None;
            self.stage_started_ms = input.now_ms;
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
        if (i32::from(input.temperature_centi_c) - i32::from(previous.temperature_centi_c)).abs()
            > 100
        {
            // Temperature movement can mimic a CC-loop voltage response.
            self.phase = ScanPhase::TemperatureSettle;
            self.equilibrium.reset();
            if let Some(point) = self.last_tracking {
                self.change(input, point.tracking_request_mv, self.request_ma);
            }
            self.previous = None;
            return;
        }
        let expected_delta = (u32::from(self.baseline_vin_mv) * 100)
            .div_ceil(u32::from(self.boundary.source_min_mv));
        let observed_delta = u32::from(input.vin_mv.saturating_sub(previous.vin_mv));
        if observed_delta * 100 >= expected_delta * 80 {
            self.last_tracking = Some(self.point(input));
            self.retain_normal_tracking(input);
            self.flat_count = 0;
            self.ambiguous_count = 0;
        } else if observed_delta * 100 <= expected_delta * 25 {
            self.flat_count += 1;
        } else {
            self.ambiguous_count = self.ambiguous_count.saturating_add(1);
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
        if self.ambiguous_count >= 5 {
            self.fail(ScanFailure::AmbiguousResponse);
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

    fn equilibrium(&mut self, input: ScanInput) {
        if self
            .equilibrium
            .observe(input.now_ms, input.temperature_centi_c)
        {
            // The stable point uses raw data captured at the safe voltage.
            self.last_tracking = Some(self.point(input));
            self.retain_tracking();
            if self.request_ma == 3_000 {
                self.phase = ScanPhase::Complete;
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
            i32::from(input.temperature_centi_c) - i32::from(previous.temperature_centi_c) > 20
        }) {
            // Warming can release more current headroom. Rescan instead of
            // holding the obsolete cold-stage voltage until apparent balance.
            self.phase = ScanPhase::Sweep;
            self.previous = Some(input);
            self.flat_count = 0;
            self.flat_request_mv = 0;
            self.equilibrium.reset();
            self.probe_next_voltage(input);
        }
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
