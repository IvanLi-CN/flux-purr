async fn handle_thermal_command(
    client: &Client,
    default_devd: &str,
    command: ThermalCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        ThermalCommand::Model { command } => match command {
            ThermalModelCommand::Calibrate(target) => {
                request_with_lease(
                    client,
                    resolve_target(target, default_devd)?,
                    Method::POST,
                    "/calibration/job",
                    Some(json!({
                        "op": "start",
                        "kind": "thermal_plant_auto"
                    })),
                )
                .await
            }
        },
        ThermalCommand::Profile { command } => match command {
            ThermalProfileCommand::Preview(args) => {
                let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
                let profile = thermal_profile_package_from_value(imported);
                request_with_lease(
                    client,
                    resolve_target(args.target, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(thermal_profile_preview_runtime_body(
                        args.profile_mode,
                        profile,
                    )),
                )
                .await
            }
            ThermalProfileCommand::ClearPreview(selector) => {
                request_with_lease(
                    client,
                    resolve_target(selector, default_devd)?,
                    Method::PUT,
                    "/runtime",
                    Some(json!({
                        "thermalControlProfile": {
                            "op": "clear_preview"
                        }
                    })),
                )
                .await
            }
            ThermalProfileCommand::Save(args) => {
                let imported: Value = serde_json::from_slice(&fs::read(&args.file)?)?;
                let profile = thermal_profile_package_from_value(imported);
                request_thermal_profile_persist_with_resolved_bank(
                    client,
                    resolve_target(args.target, default_devd)?,
                    args.profile_mode,
                    "save",
                    Some(profile),
                )
                .await
            }
            ThermalProfileCommand::ClearSaved(args) => {
                request_thermal_profile_persist_with_resolved_bank(
                    client,
                    resolve_target(args.target, default_devd)?,
                    args.profile_mode,
                    "clear_saved",
                    None,
                )
                .await
            }
        },
        ThermalCommand::SelfTest(args) => {
            collect_thermal_self_test(client, default_devd, args).await
        }
        ThermalCommand::Tune(args) => {
            thermal_flagship::run_flagship_tuning(client, default_devd, args).await
        }
        ThermalCommand::Report { command } => match command {
            ThermalReportCommand::RenderSelfTest(args) => {
                thermal_report::render_self_test_evidence_bundle(
                    thermal_report::ThermalSelfTestReportInput {
                        run_dirs: args.run_dir,
                        output_dir: args.output_dir,
                    },
                )
            }
            ThermalReportCommand::RerenderLegacy(args) => {
                thermal_report::rerender_legacy_preliminary_review_bundle(
                    thermal_report::ThermalLegacyReportInput {
                        legacy_bundle_dir: args.legacy_bundle_dir,
                        output_dir: args.output_dir,
                    },
                )
            }
        },
        ThermalCommand::Retune(args) => {
            thermal_retune::run_thermal_retune(client, default_devd, args).await
        }
    }
}

#[derive(Debug, Clone)]
struct ThermalStageResult {
    target_temp_c: i16,
    rise_time_ms: u64,
    max_overshoot_c: f64,
    hold_peak_to_peak_c: f64,
    sample_count: usize,
    stop_reason: &'static str,
    terminal_runtime_drop_reason: Option<&'static str>,
    analysis: ThermalStageAnalysis,
    guard: ThermalApproachGuardAnalysis,
    full_speed_to_stable: ThermalFullSpeedStableAnalysis,
}

#[derive(Debug, Clone, Default)]
struct ThermalStageAnalysis {
    first_hold_temp_c: Option<f64>,
    first_hold_error_c: Option<f64>,
    residual_heat_after_hold_entry_c: Option<f64>,
    approach_median_output_permille: Option<u16>,
    approach_median_slope_c_per_s: Option<f64>,
    hold_median_output_permille: Option<u16>,
    hold_p90_output_permille: Option<u16>,
    hold_mean_error_c: Option<f64>,
    hold_max_above_target_c: Option<f64>,
    hold_max_below_target_c: Option<f64>,
    approach_curve_fit_basis: Option<&'static str>,
    approach_curve_start_temp_c: Option<f64>,
    approach_curve_fitted_ms: Option<u64>,
    approach_curve_preferred_ms: Option<u64>,
    approach_curve_limit_ms: Option<u64>,
    approach_curve_max_above_c: Option<f64>,
    approach_curve_max_below_c: Option<f64>,
    approach_curve_mean_abs_error_c: Option<f64>,
    approach_curve_oscillation_c: Option<f64>,
    approach_curve_deviation_class: Option<&'static str>,
    approach_curve_tail_uses_half_floor: Option<bool>,
    approach_sample_count: usize,
    hold_sample_count: usize,
}

#[derive(Debug, Clone, Default)]
struct ThermalApproachGuardAnalysis {
    hold_threshold_temp_c: f64,
    approach_started_at_ms: Option<u64>,
    hold_threshold_crossed_at_ms: Option<u64>,
    first_hold_at_ms: Option<u64>,
    warmup_reentered_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct ThermalFullSpeedStableAnalysis {
    warmup_exited_at_ms: Option<u64>,
    stable_window_started_at_ms: Option<u64>,
    stable_window_verified_at_ms: Option<u64>,
    settle_time_ms: Option<u64>,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct ThermalReplayStageSample {
    elapsed_ms: u64,
    current_temp_c: f64,
    heater_output_percent: u8,
    control_phase: Option<String>,
    control_phase_in_hold: bool,
    source_voltage_mv: Option<u64>,
    source_current_ma: Option<u64>,
    source_power_mw: Option<u64>,
}

#[derive(Debug, Clone)]
struct ThermalStageAnalyzer {
    target_temp_c: f64,
    first_hold_temp_c: Option<f64>,
    last_elapsed_ms: Option<u64>,
    last_temp_c: Option<f64>,
    approach_output_permille: Vec<u16>,
    approach_slope_c_per_s: Vec<f64>,
    hold_output_permille: Vec<u16>,
    hold_error_c: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
struct ThermalSourceWindowAnalysis {
    sample_count: usize,
    voltage_mv: Option<CalibrationSeriesStats>,
    current_ma: Option<CalibrationSeriesStats>,
    power_mw: Option<CalibrationSeriesStats>,
}

#[derive(Debug, Clone)]
struct ThermalStageSourceAnalysisBuilder {
    target_temp_c: f64,
    first_hold_temp_c: Option<f64>,
    approach: ThermalSourceWindowAnalysis,
    hold: ThermalSourceWindowAnalysis,
}

#[derive(Debug, Clone)]
struct ThermalApproachGuardTracker {
    hold_threshold_temp_c: f64,
    approach_started_at_ms: Option<u64>,
    hold_threshold_crossed_at_ms: Option<u64>,
    first_hold_at_ms: Option<u64>,
    warmup_reentered_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ThermalFullSpeedStableTracker {
    target_temp_c: f64,
    warmup_exited_at_ms: Option<u64>,
    stable_window_started_at_ms: Option<u64>,
    stable_window_verified_at_ms: Option<u64>,
    settle_time_ms: Option<u64>,
    failure_reason: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct ThermalSampleRateTracker {
    elapsed_ms: Vec<u64>,
    below_minimum_since_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ThermalSampleRateObservation {
    interval_ms: Option<u64>,
    rolling_rate_hz: Option<f64>,
    violation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalFullSpeedStableObservation {
    Pending,
    Verified,
    Failed(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThermalCandidateSettings {
    temp_filter_alpha_permille: u16,
    approach_max_ticks: u16,
    approach_min_power_ratio_permille: u16,
    auto_adjustable_working_floor_mv: u16,
    heater_current_reserve_ma: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThermalCandidatePoint {
    target_temp_c: i16,
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    warmup_reenter_centi_c: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThermalCandidateProfile {
    settings: ThermalCandidateSettings,
    points: Vec<ThermalCandidatePoint>,
}

#[derive(Debug, Clone)]
struct BenchSourceLiveTelemetry {
    voltage_mv: u64,
    current_ma: u64,
    power_mw: u64,
    sample_uptime_ms: u64,
    status: String,
}

const THERMAL_SOURCE_65W_POWER_WATTS: u64 = 65;
const THERMAL_SOURCE_100W_POWER_WATTS: u64 = 100;
const THERMAL_SOURCE_MIN_READY_VOLTAGE_MV: u64 = 5_000;
const THERMAL_STATUS_REQUEST_TIMEOUT_MS: u64 = 1_000;
const THERMAL_STATUS_REQUEST_RETRY_ATTEMPTS: usize = 8;
const THERMAL_STATUS_REQUEST_RETRY_BACKOFF_MS: u64 = 250;
const THERMAL_RUNTIME_READBACK_TIMEOUT_MS: u64 = 3_000;
const THERMAL_RUNTIME_READBACK_POLL_MS: u64 = 100;
const ISOLAPURR_LIVE_TELEMETRY_TIMEOUT: Duration = Duration::from_millis(1_500);
const ISOLAPURR_LIVE_TELEMETRY_ATTEMPTS: usize = 3;
const THERMAL_SOURCE_TELEMETRY_STALE_TIMEOUT: Duration = Duration::from_secs(6);
// IsolaPurr telemetry is already source-sampled; spawning its CLI at 10 Hz
// competes with the USB status poller without producing fresher measurements.
// A 500ms cache refresh remains well inside the six-second stale guard.
const THERMAL_SOURCE_TELEMETRY_POLL_INTERVAL: Duration = Duration::from_millis(500);

struct BenchSourceTelemetryCache {
    latest: BenchSourceLiveTelemetry,
    latest_sample_seen_at: tokio::time::Instant,
    terminal_error: Option<String>,
}

struct BenchSourceTelemetrySampler {
    source_kind: BenchSourceKind,
    source_url: String,
    cache: Arc<Mutex<BenchSourceTelemetryCache>>,
    poller: tokio::task::JoinHandle<()>,
}

impl BenchSourceTelemetrySampler {
    fn new(
        source_kind: BenchSourceKind,
        source_url: &str,
        initial: BenchSourceLiveTelemetry,
    ) -> Self {
        let cache = Arc::new(Mutex::new(BenchSourceTelemetryCache {
            latest: initial,
            latest_sample_seen_at: tokio::time::Instant::now(),
            terminal_error: None,
        }));
        let poller_cache = Arc::clone(&cache);
        let poller_source_url = source_url.to_string();
        let poller = tokio::spawn(run_bench_source_poller(
            source_kind,
            poller_source_url,
            poller_cache,
        ));
        Self {
            source_kind,
            source_url: source_url.to_string(),
            cache,
            poller,
        }
    }

    async fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let source_kind = self.source_kind;
        let source_url = self.source_url.clone();
        let telemetry = tokio::task::spawn_blocking(move || {
            read_bench_source_live_telemetry(source_kind, &source_url)
        })
        .await??;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("source telemetry cache lock poisoned"))?;
        cache.latest = telemetry;
        cache.latest_sample_seen_at = tokio::time::Instant::now();
        cache.terminal_error = None;
        Ok(())
    }

    fn snapshot(
        &self,
    ) -> Result<(BenchSourceLiveTelemetry, u64), Box<dyn std::error::Error + Send + Sync>> {
        let cache = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("source telemetry cache lock poisoned"))?;
        if let Some(error) = &cache.terminal_error {
            return Err(io::Error::other(error.clone()).into());
        }
        let stale_ms = cache
            .latest_sample_seen_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        if cache.latest_sample_seen_at.elapsed() > THERMAL_SOURCE_TELEMETRY_STALE_TIMEOUT {
            return Err(
                format!("isolapurr USB-C telemetry did not advance for {stale_ms}ms").into(),
            );
        }
        Ok((cache.latest.clone(), stale_ms))
    }

    fn latest_stale_ms(&self) -> u64 {
        let elapsed = self
            .cache
            .lock()
            .map(|cache| cache.latest_sample_seen_at.elapsed())
            .unwrap_or(THERMAL_SOURCE_TELEMETRY_STALE_TIMEOUT);
        elapsed.as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn latest(&self) -> BenchSourceLiveTelemetry {
        self.cache
            .lock()
            .map(|cache| cache.latest.clone())
            .unwrap_or_else(|_| BenchSourceLiveTelemetry {
                voltage_mv: 0,
                current_ma: 0,
                power_mw: 0,
                sample_uptime_ms: 0,
                status: "cache_lock_failed".to_string(),
            })
    }
}

async fn run_bench_source_poller(
    source_kind: BenchSourceKind,
    source_url: String,
    cache: Arc<Mutex<BenchSourceTelemetryCache>>,
) {
    loop {
        if !poll_bench_source_once(source_kind, &source_url, &cache).await {
            return;
        }
        tokio::time::sleep(THERMAL_SOURCE_TELEMETRY_POLL_INTERVAL).await;
    }
}

async fn poll_bench_source_once(
    source_kind: BenchSourceKind,
    source_url: &str,
    cache: &Arc<Mutex<BenchSourceTelemetryCache>>,
) -> bool {
    let source_url = source_url.to_string();
    let result = tokio::task::spawn_blocking(move || {
        read_bench_source_live_telemetry(source_kind, &source_url)
    })
    .await;
    let Ok(result) = result else { return true };
    let Ok(mut cache) = cache.lock() else { return false };
    match result {
        Ok(telemetry) => update_bench_source_cache(&mut cache, telemetry),
        Err(error) if thermal_source_probe_transient_error(error.as_ref()) => {}
        Err(error) => cache.terminal_error = Some(error.to_string()),
    }
    true
}

fn update_bench_source_cache(
    cache: &mut BenchSourceTelemetryCache,
    telemetry: BenchSourceLiveTelemetry,
) {
    if telemetry.sample_uptime_ms != cache.latest.sample_uptime_ms {
        cache.latest_sample_seen_at = tokio::time::Instant::now();
    }
    cache.latest = telemetry;
    cache.terminal_error = None;
}

impl Drop for BenchSourceTelemetrySampler {
    fn drop(&mut self) {
        self.poller.abort();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalHoldObservation {
    Warmup,
    Hold,
    Completed,
}

struct ThermalHoldTracker {
    target_temp_c: i16,
    hold_duration: Duration,
    entered_threshold_c: f64,
    stable_band_c: f64,
    first_hold_started_at: Option<tokio::time::Instant>,
    rise_time_ms: Option<u64>,
    min_c: f64,
    max_c: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThermalRuntimeDropReason {
    UptimeReset,
    LatchedFault,
    HeaterDisarmed,
    WrongMode,
    WrongTarget,
}

impl ThermalRuntimeDropReason {
    fn as_str(self) -> &'static str {
        match self {
            ThermalRuntimeDropReason::UptimeReset => "uptime_reset",
            ThermalRuntimeDropReason::LatchedFault => "latched_fault",
            ThermalRuntimeDropReason::HeaterDisarmed => "heater_disarmed",
            ThermalRuntimeDropReason::WrongMode => "wrong_mode",
            ThermalRuntimeDropReason::WrongTarget => "wrong_target",
        }
    }
}

impl ThermalStageAnalyzer {
    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            first_hold_temp_c: None,
            last_elapsed_ms: None,
            last_temp_c: None,
            approach_output_permille: Vec::new(),
            approach_slope_c_per_s: Vec::new(),
            hold_output_permille: Vec::new(),
            hold_error_c: Vec::new(),
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        heater_output_percent: u8,
        elapsed_ms: u64,
        control_phase_in_hold: bool,
    ) {
        let output_permille = u16::from(heater_output_percent).saturating_mul(10);
        let error_c = self.target_temp_c - current_temp_c;
        let slope_c_per_s = self.last_elapsed_ms.zip(self.last_temp_c).and_then(
            |(last_elapsed_ms, last_temp_c)| {
                let delta_ms = elapsed_ms.saturating_sub(last_elapsed_ms);
                (delta_ms > 0).then_some((current_temp_c - last_temp_c) / delta_ms as f64 * 1_000.0)
            },
        );
        if control_phase_in_hold {
            self.first_hold_temp_c.get_or_insert(current_temp_c);
        }
        let hold_window_started = self.first_hold_temp_c.is_some();
        let near_target_approach_window = !hold_window_started
            && !control_phase_in_hold
            && (0.0..=8.0).contains(&error_c)
            && current_temp_c <= self.target_temp_c + 0.5;

        if near_target_approach_window {
            self.approach_output_permille.push(output_permille);
            if let Some(slope_c_per_s) = slope_c_per_s
                && slope_c_per_s > 0.05
            {
                self.approach_slope_c_per_s.push(slope_c_per_s);
            }
        }
        if hold_window_started {
            self.hold_output_permille.push(output_permille);
            self.hold_error_c.push(error_c);
        }

        self.last_elapsed_ms = Some(elapsed_ms);
        self.last_temp_c = Some(current_temp_c);
    }

    fn finalize(&self, max_temp_c: f64) -> ThermalStageAnalysis {
        let first_hold_temp_c = self.first_hold_temp_c;
        let hold_mean_error_c = (!self.hold_error_c.is_empty())
            .then_some(self.hold_error_c.iter().sum::<f64>() / self.hold_error_c.len() as f64);
        let hold_max_above_target_c = self
            .hold_error_c
            .iter()
            .copied()
            .filter(|error_c| *error_c < 0.0)
            .map(f64::abs)
            .fold(0.0, f64::max);
        let hold_max_below_target_c = self
            .hold_error_c
            .iter()
            .copied()
            .filter(|error_c| *error_c > 0.0)
            .fold(0.0, f64::max);
        ThermalStageAnalysis {
            first_hold_temp_c,
            first_hold_error_c: first_hold_temp_c.map(|temp_c| self.target_temp_c - temp_c),
            residual_heat_after_hold_entry_c: first_hold_temp_c
                .map(|temp_c| (max_temp_c - temp_c).max(0.0)),
            approach_median_output_permille: percentile_u16(&self.approach_output_permille, 0.5),
            approach_median_slope_c_per_s: percentile_f64(&self.approach_slope_c_per_s, 0.5),
            hold_median_output_permille: percentile_u16(&self.hold_output_permille, 0.5),
            hold_p90_output_permille: percentile_u16(&self.hold_output_permille, 0.9),
            hold_mean_error_c,
            hold_max_above_target_c: (!self.hold_error_c.is_empty())
                .then_some(hold_max_above_target_c),
            hold_max_below_target_c: (!self.hold_error_c.is_empty())
                .then_some(hold_max_below_target_c),
            approach_sample_count: self.approach_output_permille.len(),
            hold_sample_count: self.hold_output_permille.len(),
            ..ThermalStageAnalysis::default()
        }
    }
}

impl ThermalApproachGuardTracker {
    fn new(target_temp_c: i16, hold_entry_centi_c: u16) -> Self {
        Self {
            hold_threshold_temp_c: f64::from(target_temp_c)
                - (f64::from(hold_entry_centi_c.max(1)) / 100.0),
            approach_started_at_ms: None,
            hold_threshold_crossed_at_ms: None,
            first_hold_at_ms: None,
            warmup_reentered_at_ms: None,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        control_phase: Option<&str>,
    ) -> Option<&'static str> {
        match control_phase {
            Some("approach") => self.observe_approach(current_temp_c, elapsed_ms),
            Some("hold") => {
                self.observe_hold(current_temp_c, elapsed_ms);
                None
            }
            Some("warmup") => self.observe_warmup(elapsed_ms),
            _ => None,
        }
    }

    fn observe_approach(&mut self, current_temp_c: f64, elapsed_ms: u64) -> Option<&'static str> {
        let approach_started_at_ms = *self.approach_started_at_ms.get_or_insert(elapsed_ms);
        if current_temp_c >= self.hold_threshold_temp_c {
            self.hold_threshold_crossed_at_ms.get_or_insert(elapsed_ms);
        }
        let approach_elapsed_ms = elapsed_ms.saturating_sub(approach_started_at_ms);
        if self.hold_threshold_crossed_at_ms.is_none() && approach_elapsed_ms > 10_000 {
            return Some("approach_threshold_timeout");
        }
        if self.first_hold_at_ms.is_none() && approach_elapsed_ms > 30_000 {
            return Some("approach_hold_timeout");
        }
        None
    }

    fn observe_hold(&mut self, current_temp_c: f64, elapsed_ms: u64) {
        if self.approach_started_at_ms.is_none() {
            return;
        }
        if current_temp_c >= self.hold_threshold_temp_c {
            self.hold_threshold_crossed_at_ms.get_or_insert(elapsed_ms);
        }
        self.first_hold_at_ms.get_or_insert(elapsed_ms);
    }

    fn observe_warmup(&mut self, elapsed_ms: u64) -> Option<&'static str> {
        if self.approach_started_at_ms.is_some() && self.first_hold_at_ms.is_none() {
            self.warmup_reentered_at_ms.get_or_insert(elapsed_ms);
            return Some("approach_reentered_warmup");
        }
        None
    }

    fn finalize(&self) -> ThermalApproachGuardAnalysis {
        ThermalApproachGuardAnalysis {
            hold_threshold_temp_c: self.hold_threshold_temp_c,
            approach_started_at_ms: self.approach_started_at_ms,
            hold_threshold_crossed_at_ms: self.hold_threshold_crossed_at_ms,
            first_hold_at_ms: self.first_hold_at_ms,
            warmup_reentered_at_ms: self.warmup_reentered_at_ms,
        }
    }
}

impl ThermalSourceWindowAnalysis {
    fn observe(&mut self, sample: &ThermalReplayStageSample) {
        self.sample_count = self.sample_count.saturating_add(1);
        if let Some(voltage_mv) = sample.source_voltage_mv {
            observe_series(&mut self.voltage_mv, voltage_mv as f64);
        }
        if let Some(current_ma) = sample.source_current_ma {
            observe_series(&mut self.current_ma, current_ma as f64);
        }
        if let Some(power_mw) = sample.source_power_mw {
            observe_series(&mut self.power_mw, power_mw as f64);
        }
    }

    fn to_value(&self) -> Option<Value> {
        if self.sample_count == 0 {
            return None;
        }
        let mut object = serde_json::Map::new();
        object.insert("sampleCount".into(), json!(self.sample_count));
        if let Some(voltage_mv) = self.voltage_mv.as_ref() {
            object.insert("voltageMv".into(), voltage_mv.to_value());
        }
        if let Some(current_ma) = self.current_ma.as_ref() {
            object.insert("currentMa".into(), current_ma.to_value());
        }
        if let Some(power_mw) = self.power_mw.as_ref() {
            object.insert("powerMw".into(), power_mw.to_value());
        }
        Some(Value::Object(object))
    }
}

impl ThermalStageSourceAnalysisBuilder {
    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            first_hold_temp_c: None,
            approach: ThermalSourceWindowAnalysis::default(),
            hold: ThermalSourceWindowAnalysis::default(),
        }
    }

    fn observe(&mut self, sample: &ThermalReplayStageSample) {
        let error_c = self.target_temp_c - sample.current_temp_c;
        if sample.control_phase_in_hold {
            self.first_hold_temp_c.get_or_insert(sample.current_temp_c);
        }
        let hold_window_started = self.first_hold_temp_c.is_some();
        let near_target_approach_window = !hold_window_started
            && !sample.control_phase_in_hold
            && (0.0..=8.0).contains(&error_c)
            && sample.current_temp_c <= self.target_temp_c + 0.5;

        if near_target_approach_window {
            self.approach.observe(sample);
        }
        if hold_window_started {
            self.hold.observe(sample);
        }
    }

    fn to_value(&self) -> Value {
        let mut object = serde_json::Map::new();
        if let Some(approach) = self.approach.to_value() {
            object.insert("approachSource".into(), approach);
        }
        if let Some(hold) = self.hold.to_value() {
            object.insert("holdSource".into(), hold);
        }
        Value::Object(object)
    }
}

impl ThermalFullSpeedStableTracker {
    const STABLE_BAND_C: f64 = 1.5;
    const STABLE_WINDOW_MS: u64 = 10_000;
    const LOW_TEMP_SETTLE_LIMIT_MS: u64 = 10_000;
    const HIGH_TEMP_SETTLE_LIMIT_MS: u64 = 5_000;
    const HIGH_TEMP_SETTLE_THRESHOLD_C: i16 = 150;

    fn settle_limit_ms_for_target(target_temp_c: i16) -> u64 {
        if target_temp_c > Self::HIGH_TEMP_SETTLE_THRESHOLD_C {
            Self::HIGH_TEMP_SETTLE_LIMIT_MS
        } else {
            Self::LOW_TEMP_SETTLE_LIMIT_MS
        }
    }

    fn settle_limit_ms(&self) -> u64 {
        Self::settle_limit_ms_for_target(self.target_temp_c.round() as i16)
    }

    fn new(target_temp_c: i16) -> Self {
        Self {
            target_temp_c: f64::from(target_temp_c),
            warmup_exited_at_ms: None,
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
            settle_time_ms: None,
            failure_reason: None,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        control_phase: Option<&str>,
    ) -> ThermalFullSpeedStableObservation {
        if self.warmup_exited_at_ms.is_none() {
            // The specification starts this budget at the first sample that
            // leaves the firmware's full-power warmup phase. Temperature
            // proximity is a result metric, not a valid timer origin.
            if !matches!(control_phase, Some("approach" | "hold")) {
                return ThermalFullSpeedStableObservation::Pending;
            }
            self.warmup_exited_at_ms = Some(elapsed_ms);
        }

        let warmup_exited_at_ms = self.warmup_exited_at_ms.unwrap_or(elapsed_ms);
        if self.stable_window_verified_at_ms.is_some() {
            return ThermalFullSpeedStableObservation::Verified;
        }

        // Stability is a physical temperature requirement. The controller may briefly use
        // Approach to recover heat loss while the plate remains inside the stable band; tying
        // this window to the phase label turns successful recovery into a false timeout.
        let inside_stable_window =
            (current_temp_c - self.target_temp_c).abs() <= Self::STABLE_BAND_C;
        if inside_stable_window {
            let stable_window_started_at_ms =
                *self.stable_window_started_at_ms.get_or_insert(elapsed_ms);
            let settle_time_ms = stable_window_started_at_ms.saturating_sub(warmup_exited_at_ms);
            self.settle_time_ms.get_or_insert(settle_time_ms);
            if settle_time_ms > self.settle_limit_ms() {
                self.failure_reason = Some("full_speed_to_stable_timeout");
                return ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout");
            }
            if elapsed_ms.saturating_sub(stable_window_started_at_ms) >= Self::STABLE_WINDOW_MS {
                self.stable_window_verified_at_ms = Some(elapsed_ms);
                return ThermalFullSpeedStableObservation::Verified;
            }
        } else if self.stable_window_started_at_ms.take().is_some() {
            self.settle_time_ms = None;
        }

        let latest_allowed_window_start_ms =
            warmup_exited_at_ms.saturating_add(self.settle_limit_ms());
        if self.stable_window_started_at_ms.is_none() && elapsed_ms > latest_allowed_window_start_ms
        {
            self.failure_reason = Some("full_speed_to_stable_timeout");
            return ThermalFullSpeedStableObservation::Failed("full_speed_to_stable_timeout");
        }
        ThermalFullSpeedStableObservation::Pending
    }

    fn finalize(&self) -> ThermalFullSpeedStableAnalysis {
        ThermalFullSpeedStableAnalysis {
            warmup_exited_at_ms: self.warmup_exited_at_ms,
            stable_window_started_at_ms: self.stable_window_started_at_ms,
            stable_window_verified_at_ms: self.stable_window_verified_at_ms,
            settle_time_ms: self.settle_time_ms,
            failure_reason: self.failure_reason,
        }
    }
}

impl ThermalHoldTracker {
    fn new(target_temp_c: i16, hold_duration: Duration) -> Self {
        Self {
            target_temp_c,
            hold_duration,
            entered_threshold_c: 0.5,
            stable_band_c: 3.0,
            first_hold_started_at: None,
            rise_time_ms: None,
            min_c: f64::INFINITY,
            max_c: f64::NEG_INFINITY,
        }
    }

    fn observe(
        &mut self,
        current_temp_c: f64,
        elapsed_ms: u64,
        now: tokio::time::Instant,
        control_phase_in_hold: bool,
    ) -> ThermalHoldObservation {
        let target_temp_c = f64::from(self.target_temp_c);
        if self.first_hold_started_at.is_none() {
            if !control_phase_in_hold {
                return ThermalHoldObservation::Warmup;
            }
            if current_temp_c < target_temp_c - self.entered_threshold_c {
                return ThermalHoldObservation::Warmup;
            }
            if (current_temp_c - target_temp_c).abs() > self.stable_band_c {
                return ThermalHoldObservation::Warmup;
            }
        }

        let first_hold_started_at = *self.first_hold_started_at.get_or_insert_with(|| {
            self.rise_time_ms.get_or_insert(elapsed_ms);
            now
        });
        self.min_c = self.min_c.min(current_temp_c);
        self.max_c = self.max_c.max(current_temp_c);
        if now.saturating_duration_since(first_hold_started_at) >= self.hold_duration {
            ThermalHoldObservation::Completed
        } else {
            ThermalHoldObservation::Hold
        }
    }

    fn rise_time_ms(&self) -> Option<u64> {
        self.rise_time_ms
    }

    fn peak_to_peak_c(&self) -> f64 {
        if self.min_c.is_finite() && self.max_c.is_finite() {
            self.max_c - self.min_c
        } else {
            f64::INFINITY
        }
    }
}

fn thermal_runtime_drop_reason(
    status: &Value,
    target_temp_c: i16,
    last_uptime_seconds: Option<u64>,
) -> Option<ThermalRuntimeDropReason> {
    let uptime_seconds = status.get("uptimeSeconds").and_then(Value::as_u64);
    if let Some((last, current)) = last_uptime_seconds.zip(uptime_seconds)
        && current < last
    {
        return Some(ThermalRuntimeDropReason::UptimeReset);
    }
    if status
        .get("heaterFaultReason")
        .is_some_and(|reason| !reason.is_null())
    {
        return Some(ThermalRuntimeDropReason::LatchedFault);
    }
    if status
        .get("targetTempC")
        .and_then(Value::as_i64)
        .is_some_and(|target| target != i64::from(target_temp_c))
    {
        return Some(ThermalRuntimeDropReason::WrongTarget);
    }
    if status
        .get("mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode != "sampling")
    {
        return Some(ThermalRuntimeDropReason::WrongMode);
    }
    if status
        .get("heaterEnabled")
        .and_then(Value::as_bool)
        .is_some_and(|heater_enabled| !heater_enabled)
    {
        return Some(ThermalRuntimeDropReason::HeaterDisarmed);
    }
    None
}

fn thermal_recoverable_sensor_fault(status: &Value) -> bool {
    matches!(
        status.get("heaterFaultReason").and_then(Value::as_str),
        Some("sensor-glitch" | "sensor-open" | "sensor-short" | "adc-read-failed")
    ) && status.get("mode").and_then(Value::as_str) != Some("fault")
}

fn thermal_stage_stop_reason_is_environment_fault(stop_reason: &str) -> bool {
    matches!(
        stop_reason,
        "runtime_lost"
            | "uptime_reset"
            | "latched_fault"
            | "heater_disarmed"
            | "wrong_mode"
            | "wrong_target"
            | "sample_rate_below_minimum"
            | "sample_rate_below_3hz"
            | "status_request_failed"
            | "source_telemetry_stale"
            | "source_fault"
            | "temperature_sample_glitch"
    )
}

fn thermal_stage_should_retry_after_environment_fault(result: &ThermalStageResult) -> bool {
    thermal_stage_stop_reason_is_environment_fault(result.stop_reason)
}

impl ThermalStageResult {
    fn to_value(&self) -> Value {
        json!({
            "targetTempC": self.target_temp_c,
            "riseTimeMs": self.rise_time_ms,
            "maxOvershootC": self.max_overshoot_c,
            "holdPeakToPeakC": self.hold_peak_to_peak_c,
            "sampleCount": self.sample_count,
            "stopReason": self.stop_reason,
            "terminalRuntimeDropReason": self.terminal_runtime_drop_reason,
            "guard": {
                "holdThresholdTempC": self.guard.hold_threshold_temp_c,
                "approachStartedAtMs": self.guard.approach_started_at_ms,
                "holdThresholdCrossedAtMs": self.guard.hold_threshold_crossed_at_ms,
                "firstHoldAtMs": self.guard.first_hold_at_ms,
                "warmupReenteredAtMs": self.guard.warmup_reentered_at_ms,
            },
            "fullSpeedToStable": {
                "limitMs": ThermalFullSpeedStableTracker::settle_limit_ms_for_target(self.target_temp_c),
                "stableBandC": ThermalFullSpeedStableTracker::STABLE_BAND_C,
                "stableWindowMs": ThermalFullSpeedStableTracker::STABLE_WINDOW_MS,
                "warmupExitedAtMs": self.full_speed_to_stable.warmup_exited_at_ms,
                "stableWindowStartedAtMs": self.full_speed_to_stable.stable_window_started_at_ms,
                "stableWindowVerifiedAtMs": self.full_speed_to_stable.stable_window_verified_at_ms,
                "settleTimeMs": self.full_speed_to_stable.settle_time_ms,
                "failureReason": self.full_speed_to_stable.failure_reason,
            },
            "analysis": {
                "firstHoldTempC": self.analysis.first_hold_temp_c,
                "firstHoldErrorC": self.analysis.first_hold_error_c,
                "residualHeatAfterHoldEntryC": self.analysis.residual_heat_after_hold_entry_c,
                "approachMedianOutputPermille": self.analysis.approach_median_output_permille,
                "approachMedianSlopeCPerS": self.analysis.approach_median_slope_c_per_s,
                "holdMedianOutputPermille": self.analysis.hold_median_output_permille,
                "holdP90OutputPermille": self.analysis.hold_p90_output_permille,
                "holdMeanErrorC": self.analysis.hold_mean_error_c,
                "holdMaxAboveTargetC": self.analysis.hold_max_above_target_c,
                "holdMaxBelowTargetC": self.analysis.hold_max_below_target_c,
                "approachCurveFitBasis": self.analysis.approach_curve_fit_basis,
                "approachCurveStartTempC": self.analysis.approach_curve_start_temp_c,
                "approachCurveFittedMs": self.analysis.approach_curve_fitted_ms,
                "approachCurvePreferredMs": self.analysis.approach_curve_preferred_ms,
                "approachCurveLimitMs": self.analysis.approach_curve_limit_ms,
                "approachCurveMaxAboveC": self.analysis.approach_curve_max_above_c,
                "approachCurveMaxBelowC": self.analysis.approach_curve_max_below_c,
                "approachCurveMeanAbsErrorC": self.analysis.approach_curve_mean_abs_error_c,
                "approachCurveOscillationC": self.analysis.approach_curve_oscillation_c,
                "approachCurveDeviationClass": self.analysis.approach_curve_deviation_class,
                "approachCurveTailUsesHalfFloor": self.analysis.approach_curve_tail_uses_half_floor,
                "approachSampleCount": self.analysis.approach_sample_count,
                "holdSampleCount": self.analysis.hold_sample_count,
            },
        })
    }
}

fn thermal_profile_package_from_value(imported: Value) -> Value {
    imported
        .get("profile")
        .cloned()
        .or_else(|| {
            imported
                .get("thermalControlProfile")
                .and_then(|thermal_control_profile| thermal_control_profile.get("profile"))
                .cloned()
        })
        .unwrap_or(imported)
}

fn parse_thermal_targets_from_summary(
    summary: &Value,
    key: &str,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let values = summary
        .get("parameters")
        .and_then(|parameters| parameters.get(key))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary missing parameters.{key}"),
            )
        })?;
    let mut targets = Vec::with_capacity(values.len());
    for value in values {
        let target = value.as_i64().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary parameters.{key} contains a non-integer target"),
            )
        })?;
        targets.push(i16::try_from(target).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("thermal summary parameters.{key} target out of range"),
            )
        })?);
    }
    Ok(targets)
}

fn thermal_self_test_evaluation_mode_from_summary(
    summary: &Value,
) -> ThermalSelfTestEvaluationMode {
    match summary
        .get("parameters")
        .and_then(|parameters| parameters.get("evaluationMode"))
        .and_then(Value::as_str)
    {
        Some("tuning-scout") => ThermalSelfTestEvaluationMode::TuningScout,
        _ => ThermalSelfTestEvaluationMode::HoldConfirm,
    }
}

fn require_value_u64(
    value: &Value,
    key: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing integer field: {key}"),
        )
        .into()
    })
}

fn require_value_i16(
    value: &Value,
    key: &str,
) -> Result<i16, Box<dyn std::error::Error + Send + Sync>> {
    let parsed = value.get(key).and_then(Value::as_i64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing integer field: {key}"),
        )
    })?;
    i16::try_from(parsed).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value field out of range: {key}"),
        )
        .into()
    })
}

fn require_value_f64(
    value: &Value,
    key: &str,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_f64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing numeric field: {key}"),
        )
        .into()
    })
}

fn require_value_str<'a>(
    value: &'a Value,
    key: &str,
) -> Result<&'a str, Box<dyn std::error::Error + Send + Sync>> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("value missing string field: {key}"),
        )
        .into()
    })
}

fn thermal_stage_result_from_value(
    value: &Value,
) -> Result<ThermalStageResult, Box<dyn std::error::Error + Send + Sync>> {
    let stop_reason = match require_value_str(value, "stopReason")? {
        "completed" => "completed",
        "timeout" => "timeout",
        "runtime_lost" => "runtime_lost",
        "uptime_reset" => "uptime_reset",
        "latched_fault" => "latched_fault",
        "heater_disarmed" => "heater_disarmed",
        "wrong_mode" => "wrong_mode",
        "wrong_target" => "wrong_target",
        "sample_rate_below_3hz" => "sample_rate_below_3hz",
        "status_request_failed" => "status_request_failed",
        "temperature_sample_glitch" => "temperature_sample_glitch",
        "heater_no_output" => "heater_no_output",
        "warmup_timeout" => "warmup_timeout",
        "approach_threshold_timeout" => "approach_threshold_timeout",
        "approach_hold_timeout" => "approach_hold_timeout",
        "approach_reentered_warmup" => "approach_reentered_warmup",
        "full_speed_to_stable_timeout" => "full_speed_to_stable_timeout",
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported thermal stop reason in replay: {other}"),
            )
            .into());
        }
    };
    let hold_peak_to_peak_c = match value.get("holdPeakToPeakC").and_then(Value::as_f64) {
        Some(value) => value,
        None if stop_reason != "completed" => f64::INFINITY,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "completed thermal stage missing numeric field: holdPeakToPeakC",
            )
            .into());
        }
    };
    Ok(ThermalStageResult {
        target_temp_c: require_value_i16(value, "targetTempC")?,
        rise_time_ms: require_value_u64(value, "riseTimeMs")?,
        max_overshoot_c: require_value_f64(value, "maxOvershootC")?,
        hold_peak_to_peak_c,
        sample_count: require_value_u64(value, "sampleCount")? as usize,
        stop_reason,
        terminal_runtime_drop_reason: value
            .get("terminalRuntimeDropReason")
            .and_then(Value::as_str)
            .and_then(|reason| match reason {
                "uptime_reset" => Some("uptime_reset"),
                "latched_fault" => Some("latched_fault"),
                "heater_disarmed" => Some("heater_disarmed"),
                "wrong_mode" => Some("wrong_mode"),
                "wrong_target" => Some("wrong_target"),
                "temperature_sample_glitch" => Some("temperature_sample_glitch"),
                _ => None,
            }),
        analysis: ThermalStageAnalysis::default(),
        guard: thermal_guard_analysis(value),
        full_speed_to_stable: thermal_full_speed_analysis(value),
    })
}

fn thermal_guard_analysis(value: &Value) -> ThermalApproachGuardAnalysis {
    let field = |key| value.get("guard").and_then(|guard| guard.get(key)).and_then(Value::as_u64);
    ThermalApproachGuardAnalysis {
        hold_threshold_temp_c: value.pointer("/guard/holdThresholdTempC").and_then(Value::as_f64).unwrap_or(0.0),
        approach_started_at_ms: field("approachStartedAtMs"),
        hold_threshold_crossed_at_ms: field("holdThresholdCrossedAtMs"),
        first_hold_at_ms: field("firstHoldAtMs"),
        warmup_reentered_at_ms: field("warmupReenteredAtMs"),
    }
}

fn thermal_full_speed_analysis(value: &Value) -> ThermalFullSpeedStableAnalysis {
    let field = |key| value.get("fullSpeedToStable").and_then(|item| item.get(key)).and_then(Value::as_u64);
    let failure_reason = value
        .pointer("/fullSpeedToStable/failureReason")
        .and_then(Value::as_str)
        .filter(|reason| *reason == "full_speed_to_stable_timeout")
        .map(|reason| match reason {
            "full_speed_to_stable_timeout" => "full_speed_to_stable_timeout",
            _ => unreachable!(),
        });
    ThermalFullSpeedStableAnalysis {
        warmup_exited_at_ms: field("warmupExitedAtMs"),
        stable_window_started_at_ms: field("stableWindowStartedAtMs"),
        stable_window_verified_at_ms: field("stableWindowVerifiedAtMs"),
        settle_time_ms: field("settleTimeMs"),
        failure_reason,
    }
}

fn read_ndjson_values(path: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
    let reader = BufReader::new(File::open(path)?);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line)?);
    }
    Ok(values)
}

fn thermal_control_temperature_c(
    status: &Value,
    heater: Option<&Value>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    status
        .get("heaterControlTempC")
        .and_then(Value::as_f64)
        .or_else(|| status.get("heaterFilteredTempC").and_then(Value::as_f64))
        .or_else(|| status.get("currentTempC").and_then(Value::as_f64))
        .or_else(|| {
            heater
                .and_then(|value| value.get("heaterControlTempC"))
                .and_then(Value::as_f64)
        })
        .or_else(|| {
            heater
                .and_then(|value| value.get("heaterFilteredTempC"))
                .and_then(Value::as_f64)
        })
        .or_else(|| {
            heater
                .and_then(|value| value.get("currentTempC"))
                .and_then(Value::as_f64)
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal sample missing control temperature",
            )
            .into()
        })
}

fn thermal_replay_stage_samples(
    samples: &[Value],
    target_temp_c: i16,
) -> Result<Vec<ThermalReplayStageSample>, Box<dyn std::error::Error + Send + Sync>> {
    let mut stage_samples = Vec::new();
    for sample in samples {
        if sample.get("testPhase").and_then(Value::as_str) != Some("applied") {
            continue;
        }
        if sample.get("targetTempC").and_then(Value::as_i64) != Some(i64::from(target_temp_c)) {
            continue;
        }
        if sample.get("phase").and_then(Value::as_str) == Some("runtime_rearm") {
            continue;
        }
        let heater = sample.get("heaterTelemetry").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal replay sample missing heaterTelemetry",
            )
        })?;
        let status = sample.get("status").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "thermal replay sample missing status",
            )
        })?;
        let elapsed_ms = require_value_u64(sample, "elapsedMs")?;
        let current_temp_c = thermal_control_temperature_c(status, Some(heater))?;
        let heater_output_percent =
            require_status_u64(heater, "heaterOutputPercent")?.min(u64::from(u8::MAX)) as u8;
        let control_phase = status
            .get("heaterControlPhase")
            .or_else(|| sample.get("phase"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let control_phase_in_hold = control_phase.as_deref() == Some("hold");
        let source_telemetry = sample.get("sourceTelemetry");
        stage_samples.push(ThermalReplayStageSample {
            elapsed_ms,
            current_temp_c,
            heater_output_percent,
            control_phase,
            control_phase_in_hold,
            source_voltage_mv: source_telemetry
                .and_then(|value| value.get("voltageMv"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualVoltageMv").and_then(Value::as_u64)),
            source_current_ma: source_telemetry
                .and_then(|value| value.get("currentMa"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualCurrentMa").and_then(Value::as_u64)),
            source_power_mw: source_telemetry
                .and_then(|value| value.get("powerMw"))
                .and_then(Value::as_u64)
                .or_else(|| sample.get("sourceActualPowerMw").and_then(Value::as_u64)),
        });
    }
    stage_samples.sort_by_key(|sample| sample.elapsed_ms);
    Ok(stage_samples)
}

fn thermal_replay_stage_analysis(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> ThermalStageAnalysis {
    let mut analyzer = ThermalStageAnalyzer::new(target_temp_c);
    let mut max_temp_c = f64::NEG_INFINITY;
    for sample in samples {
        max_temp_c = max_temp_c.max(sample.current_temp_c);
        analyzer.observe(
            sample.current_temp_c,
            sample.heater_output_percent,
            sample.elapsed_ms,
            sample.control_phase_in_hold,
        );
    }
    if max_temp_c.is_finite() {
        let mut analysis = analyzer.finalize(max_temp_c);
        thermal_stage_populate_approach_curve_analysis(&mut analysis, samples, target_temp_c);
        analysis
    } else {
        ThermalStageAnalysis::default()
    }
}

fn thermal_approach_curve_reference_temp_c(
    start_temp_c: f64,
    target_temp_c: f64,
    elapsed_from_start_ms: u64,
    fitted_ms: u64,
) -> f64 {
    let normalized = if fitted_ms == 0 {
        1.0
    } else {
        (elapsed_from_start_ms as f64 / fitted_ms as f64).clamp(0.0, 1.0)
    };
    let progress = 1.0 - (1.0 - normalized).powi(2);
    start_temp_c + (target_temp_c - start_temp_c) * progress
}

fn thermal_stage_populate_approach_curve_analysis(
    analysis: &mut ThermalStageAnalysis,
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) {
    let Some(start_index) = samples
        .iter()
        .position(|sample| sample.control_phase.as_deref() != Some("warmup"))
    else {
        return;
    };
    let Some(start_sample) = samples.get(start_index) else {
        return;
    };
    let end_index = samples
        .iter()
        .position(|sample| sample.control_phase_in_hold)
        .unwrap_or_else(|| samples.len().saturating_sub(1));
    let Some(end_sample) = samples.get(end_index) else {
        return;
    };

    analysis.approach_curve_fit_basis = Some("target_error_from_approach_start");
    analysis.approach_curve_start_temp_c = Some(start_sample.current_temp_c);
    analysis.approach_curve_preferred_ms = Some(THERMAL_APPROACH_CURVE_PREFERRED_MS);
    analysis.approach_curve_limit_ms = Some(THERMAL_APPROACH_CURVE_LIMIT_MS);
    analysis.approach_curve_tail_uses_half_floor = Some(true);

    let raw_duration_ms = end_sample
        .elapsed_ms
        .saturating_sub(start_sample.elapsed_ms);
    let fitted_ms = raw_duration_ms.clamp(
        THERMAL_APPROACH_CURVE_PREFERRED_MS,
        THERMAL_APPROACH_CURVE_LIMIT_MS,
    );
    analysis.approach_curve_fitted_ms = Some(fitted_ms);

    let target_temp_c = f64::from(target_temp_c);
    let target_delta_c = target_temp_c - start_sample.current_temp_c;
    if raw_duration_ms == 0 || target_delta_c <= 0.0 {
        analysis.approach_curve_deviation_class = Some("insufficient_evidence");
        return;
    }

    let mut max_above_c = 0.0f64;
    let mut max_below_c = 0.0f64;
    let mut mean_abs_error_sum_c = 0.0f64;
    let mut deviation_count = 0usize;
    let mut last_sign = 0i8;
    let mut sign_changes = 0usize;

    for sample in samples
        .iter()
        .take(end_index.saturating_add(1))
        .skip(start_index)
    {
        let elapsed_from_start_ms = sample.elapsed_ms.saturating_sub(start_sample.elapsed_ms);
        let reference_temp_c = thermal_approach_curve_reference_temp_c(
            start_sample.current_temp_c,
            target_temp_c,
            elapsed_from_start_ms,
            fitted_ms,
        );
        let deviation_c = sample.current_temp_c - reference_temp_c;
        max_above_c = max_above_c.max(deviation_c.max(0.0));
        max_below_c = max_below_c.max((-deviation_c).max(0.0));
        mean_abs_error_sum_c += deviation_c.abs();
        deviation_count = deviation_count.saturating_add(1);

        let sign = if deviation_c > THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C {
            1
        } else if deviation_c < -THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C {
            -1
        } else {
            0
        };
        if sign != 0 {
            if last_sign != 0 && sign != last_sign {
                sign_changes = sign_changes.saturating_add(1);
            }
            last_sign = sign;
        }
    }

    analysis.approach_curve_max_above_c = Some(max_above_c);
    analysis.approach_curve_max_below_c = Some(max_below_c);
    if deviation_count > 0 {
        analysis.approach_curve_mean_abs_error_c =
            Some(mean_abs_error_sum_c / deviation_count as f64);
    }
    let oscillation_c = max_above_c.min(max_below_c);
    analysis.approach_curve_oscillation_c = Some(oscillation_c);

    analysis.approach_curve_deviation_class = Some(
        if max_above_c >= 1.0 && max_above_c > max_below_c + THERMAL_APPROACH_CURVE_CLASS_MARGIN_C {
            "brake_late_or_residual"
        } else if max_below_c >= 1.0
            && max_below_c > max_above_c + THERMAL_APPROACH_CURVE_CLASS_MARGIN_C
        {
            "underpowered_or_early_coast"
        } else if sign_changes >= 2
            && max_above_c >= THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C
            && max_below_c >= THERMAL_APPROACH_CURVE_SIGNIFICANT_DEVIATION_C
        {
            "oscillatory_near_target"
        } else {
            "on_curve"
        },
    );
}

fn thermal_replay_stage_source_analysis(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> Value {
    let mut builder = ThermalStageSourceAnalysisBuilder::new(target_temp_c);
    for sample in samples {
        builder.observe(sample);
    }
    builder.to_value()
}

fn thermal_stage_value_attach_source_analysis(
    stage_value: &mut Value,
    stage_samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) {
    let Some(analysis) = stage_value
        .get_mut("analysis")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let source_analysis = thermal_replay_stage_source_analysis(stage_samples, target_temp_c);
    let Some(source_analysis) = source_analysis.as_object() else {
        return;
    };
    for (key, value) in source_analysis {
        analysis.insert(key.clone(), value.clone());
    }
}

fn thermal_summary_attach_replay_source_analysis(
    summary: &mut Value,
    samples: &[Value],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Some(applied) = summary.get_mut("applied").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for stage_value in applied.iter_mut() {
        let target_temp_c = stage_value
            .get("targetTempC")
            .and_then(Value::as_i64)
            .and_then(|value| i16::try_from(value).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "thermal summary applied result missing targetTempC",
                )
            })?;
        let stage_samples = thermal_replay_stage_samples(samples, target_temp_c)?;
        thermal_stage_value_attach_source_analysis(stage_value, &stage_samples, target_temp_c);
    }
    Ok(())
}

fn thermal_summary_attach_source_analysis_from_ndjson(
    summary: &mut Value,
    samples_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let samples = read_ndjson_values(samples_path)?;
    thermal_summary_attach_replay_source_analysis(summary, &samples)
}

fn thermal_replay_full_speed_to_stable(
    samples: &[ThermalReplayStageSample],
    target_temp_c: i16,
) -> ThermalFullSpeedStableAnalysis {
    let mut tracker = ThermalFullSpeedStableTracker::new(target_temp_c);
    for sample in samples {
        let _ = tracker.observe(
            sample.current_temp_c,
            sample.elapsed_ms,
            sample.control_phase.as_deref(),
        );
    }
    tracker.finalize()
}

fn thermal_candidate_point_from_heater_parameters(
    heater_parameters: &Value,
) -> Result<ThermalCandidatePoint, Box<dyn std::error::Error + Send + Sync>> {
    let target_temp_c = require_value_i16(heater_parameters, "targetTempC")?;
    let default_point = thermal_default_target_point(target_temp_c);
    Ok(ThermalCandidatePoint {
        target_temp_c,
        brake_distance_centi_c: heater_parameters
            .get("brakeDistanceCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.brake_distance_centi_c),
        warmup_power_permille: 1_000,
        approach_power_permille: heater_parameters
            .get("approachPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_power_permille),
        approach_floor_power_permille: heater_parameters
            .get("approachFloorPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_floor_power_permille),
        approach_damping_exponent_permille: heater_parameters
            .get("approachDampingExponentPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_damping_exponent_permille),
        approach_tail_window_centi_c: heater_parameters
            .get("approachTailWindowCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_tail_window_centi_c),
        hold_power_permille: heater_parameters
            .get("holdPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_power_permille),
        hold_reheat_power_permille: heater_parameters
            .get("holdReheatPowerPermille")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_reheat_power_permille),
        warmup_reenter_centi_c: heater_parameters
            .get("warmupReenterCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.warmup_reenter_centi_c),
        hold_entry_centi_c: heater_parameters
            .get("holdEntryCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_entry_centi_c),
        hold_exit_centi_c: heater_parameters
            .get("holdExitCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_exit_centi_c),
        hold_on_centi_c: heater_parameters
            .get("holdOnCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_on_centi_c),
        hold_off_centi_c: heater_parameters
            .get("holdOffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_off_centi_c),
        overshoot_cutoff_centi_c: heater_parameters
            .get("overshootCutoffCentiC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.overshoot_cutoff_centi_c),
        hold_kp_permille_per_c: heater_parameters
            .get("holdKpPermillePerC")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: heater_parameters
            .get("holdKiPermillePerCTick")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_ki_permille_per_c_tick),
        hold_blend_ticks: heater_parameters
            .get("holdBlendTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_blend_ticks),
        approach_lead_ticks: heater_parameters
            .get("approachLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.approach_lead_ticks),
        hold_lead_ticks: heater_parameters
            .get("holdLeadTicks")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
            .unwrap_or(default_point.hold_lead_ticks),
    })
}

fn thermal_replay_applied_profile(
    summary: &Value,
    samples: &[Value],
    target_temps_c: &[i16],
) -> Result<ThermalCandidateProfile, Box<dyn std::error::Error + Send + Sync>> {
    let fallback_bank = summary
        .get("source")
        .and_then(|source| source.get("resolvedBank"))
        .and_then(Value::as_str)
        .unwrap_or("pps3a");
    let seed_profile_path = summary
        .get("parameters")
        .and_then(|parameters| parameters.get("seedProfileFile"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut profile = if let Some(candidate_profile) = summary.get("candidateProfile") {
        thermal_candidate_profile_from_value(candidate_profile.clone())
    } else if let Some(seed_profile_path) = seed_profile_path
        && seed_profile_path.exists()
    {
        thermal_candidate_profile_from_value(serde_json::from_slice(&fs::read(seed_profile_path)?)?)
    } else {
        load_thermal_default_seed_candidate_profile(fallback_bank)?.0
    };
    if let Some(settings_value) = samples.iter().find_map(|sample| {
        sample
            .get("testPhase")
            .and_then(Value::as_str)
            .filter(|phase| *phase == "applied")
            .and_then(|_| sample.get("heaterParameters"))
            .and_then(|heater_parameters| heater_parameters.get("settings"))
            .cloned()
    }) {
        profile.settings = thermal_candidate_profile_from_value(json!({
            "settings": settings_value,
            "points": []
        }))
        .settings;
    }
    for &target_temp_c in target_temps_c {
        let Some(heater_parameters) = samples.iter().find_map(|sample| {
            (sample.get("testPhase").and_then(Value::as_str) == Some("applied")
                && sample.get("targetTempC").and_then(Value::as_i64)
                    == Some(i64::from(target_temp_c)))
            .then(|| sample.get("heaterParameters").cloned())
            .flatten()
        }) else {
            continue;
        };
        if let Some(point) = thermal_candidate_point_mut(&mut profile, target_temp_c) {
            *point = thermal_candidate_point_from_heater_parameters(&heater_parameters)?;
        }
    }
    Ok(profile)
}
