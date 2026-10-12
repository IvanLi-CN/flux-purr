use core::{cmp::Ordering, fmt::Write};

use heapless::{String, Vec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::pd::{FUSB302B_PD_ABSOLUTE_MAX_MV, FUSB302B_PD_ABSOLUTE_MIN_MV};
use crate::heater_phase::hold_entry_ready;
use crate::memory::{
    ThermalControlProfileConfig, ThermalControlProfilePointConfig, ThermalProfileBank,
};
use crate::thermal_tuning_limits::{
    TRIAL_TIMEOUT_MS, ThermalTuningTimingEvidence, original_settle_limit_ms,
};
use crate::thermal_tuning_strategy::{
    CurveClass, TuneEvidence, bounded_failure_alternative, observed_low_side_hold_failure,
    reseed_after_hold_confirm, tune_point, unobserved_low_side_failure,
};

#[cfg(test)]
#[path = "thermal_tuning_unverified_window_tests.rs"]
mod unverified_window_tests;

pub const THERMAL_TUNING_TARGETS_C: [i16; 8] = [60, 80, 100, 120, 140, 160, 180, 220];
pub const THERMAL_TUNING_EXECUTION_ORDER_C: [i16; 8] = [60, 220, 120, 80, 100, 160, 140, 180];
// Keep the wire/runtime vector capacity at the historical envelope. The
// canonical scope above is eight targets; this capacity is transport storage,
// not permission to add another target (membership and order are validated
// against the canonical execution order).
pub const THERMAL_TUNING_MAX_TARGETS: usize = 9;
pub const THERMAL_TUNING_EVENT_CAPACITY: usize = 256;
// Samples carry the full local PPS/control evidence and must fit one bounded
// JSONL response. Six events keep the largest sample-only page below the
// shared LAN 6 KiB body envelope while letting USB hosts drain the live stream
// faster than the runtime produces samples at its 200 ms cadence.
pub const THERMAL_TUNING_EVENT_PAGE_SIZE: usize = 6;
pub const THERMAL_TUNING_HOLD_CONFIRM_MS: u64 = 60_000;
pub const THERMAL_TUNING_SCOUT_HOLD_MS: u64 = 12_000;
// Leave control-cycle scheduling margin below the required 333ms sample span.
pub const THERMAL_TUNING_LIVE_SAMPLE_INTERVAL_MS: u64 = 200;
pub const THERMAL_TUNING_STABLE_WINDOW_MS: u64 = 10_000;
pub const THERMAL_TUNING_STABLE_BAND_CENTI_C: i16 = 150;
pub const THERMAL_TUNING_TARGET_BUDGET_MS: u64 = 20 * 60 * 1_000;
const THERMAL_TUNING_APPROACH_TRACE_CAPACITY: usize = 256;
pub const THERMAL_TUNING_ID_MAX_LEN: usize = 48;
pub const THERMAL_TUNING_REASON_MAX_LEN: usize = 96;
// `sha256:` plus 64 hexadecimal characters.
pub const THERMAL_TUNING_HASH_LEN: usize = 71;

pub type TuningId = String<THERMAL_TUNING_ID_MAX_LEN>;
pub type TuningHash = String<THERMAL_TUNING_HASH_LEN>;
pub type TuningReason = String<THERMAL_TUNING_REASON_MAX_LEN>;

pub struct ThermalTuningStart<'a> {
    pub run_id: TuningId,
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub power_class: ThermalTuningPowerClass,
    pub targets: &'a [i16],
    pub pps_ready: bool,
    pub thermal_model_available: bool,
    pub calibration_conflict: bool,
    pub candidate_profile: Option<ThermalControlProfileConfig>,
}

pub struct ThermalTuningSaveConfirmation<'a> {
    pub run_id: &'a str,
    pub candidate_id: &'a str,
    pub candidate_hash: &'a str,
    pub power_class: ThermalTuningPowerClass,
    pub readback_verified: bool,
    pub bank_revision: u32,
    pub bank_hash: TuningHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningPowerClass {
    Pps3a,
    Pps5a,
}

impl ThermalTuningPowerClass {
    pub const fn required_current_ma(self) -> u16 {
        match self {
            Self::Pps3a => 3_000,
            Self::Pps5a => 5_000,
        }
    }

    pub const fn profile_bank(self) -> ThermalProfileBank {
        match self {
            Self::Pps3a => ThermalProfileBank::Pps3a,
            Self::Pps5a => ThermalProfileBank::Pps5a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningPhase {
    Idle,
    Warmup,
    Approach,
    Hold,
    Cooldown,
    Sealed,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningTrialPurpose {
    Scout,
    BatchCurrent,
    BatchPredicted,
    HoldConfirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningEventKind {
    Sample,
    PhaseTransition,
    CandidateTrial,
    Decision,
    Safety,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningTerminalDisposition {
    Running,
    CandidateReady,
    Passed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningReviewDisposition {
    Pending,
    Complete,
    ReviewIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalTuningCommandOp {
    Start,
    Cancel,
    Seal,
    ConfirmSave,
    Read,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningCandidatePoint {
    pub target_c: i16,
    pub warmup_power_permille: u16,
    pub warmup_reenter_centi_c: u16,
    pub brake_distance_centi_c: u16,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningCandidateSnapshot {
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub power_class: ThermalTuningPowerClass,
    pub point: Option<ThermalTuningCandidatePoint>,
    pub seed_source: TuningReason,
    pub lower_target_c: Option<i16>,
    pub upper_target_c: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTrialRecord {
    pub candidate: ThermalTuningCandidateSnapshot,
    pub trial_index: u8,
    pub purpose: ThermalTuningTrialPurpose,
    pub start_sequence: u32,
    pub start_elapsed_ms: u64,
    #[serde(default)]
    pub start_temperature_centi_c: Option<i16>,
    #[serde(default)]
    pub cooldown_threshold_centi_c: Option<i16>,
    pub end_sequence: Option<u32>,
    pub end_elapsed_ms: Option<u64>,
    pub sample_start_sequence: Option<u32>,
    pub sample_end_sequence: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ThermalTuningTimingEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningDecisionRecord {
    pub disposition: TuningReason,
    pub purpose: ThermalTuningTrialPurpose,
    pub max_overshoot_centi_c: Option<u16>,
    pub hold_peak_to_peak_centi_c: Option<u16>,
    pub settle_time_ms: Option<u32>,
    pub score: [f64; 11],
    pub warmup_full: bool,
    pub approach_to_hold: bool,
    pub hold_confirm: bool,
    pub hard_gate_overshoot_centi_c: u16,
    pub hard_gate_hold_peak_to_peak_centi_c: u16,
    pub hard_gate_full_speed_to_stable_ms: u32,
    pub freeze: bool,
    pub prune: bool,
    pub stop_reason: TuningReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<ThermalTuningTimingEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningPpsContract {
    pub voltage_mv: u16,
    pub current_ma: u16,
    pub kind: ThermalTuningPowerClass,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningFanSample {
    pub enabled: bool,
    pub pwm_permille: u16,
    pub output_level: crate::fan_policy::FanOutputLevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningSample {
    pub sequence: u32,
    pub elapsed_ms: u64,
    pub target_c: i16,
    pub trial_index: u8,
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub temperature_centi_c: i16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_temperature_centi_c: Option<i16>,
    pub vin_mv: u32,
    pub pps_contract: ThermalTuningPpsContract,
    pub requested_mv: u16,
    pub pps_power_ceiling_mv: u32,
    pub heater_requested_output_permille: u16,
    pub heater_output_permille: u16,
    pub measurement_valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_accepted: Option<bool>,
    pub phase: ThermalTuningPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan: Option<ThermalTuningFanSample>,
}

impl ThermalTuningSample {
    fn has_fresh_measurement(&self) -> bool {
        self.measurement_valid && self.measurement_accepted.unwrap_or(true)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningEvent {
    pub sequence: u32,
    pub elapsed_ms: u64,
    pub kind: ThermalTuningEventKind,
    pub phase: ThermalTuningPhase,
    pub target_c: Option<i16>,
    pub trial_index: Option<u8>,
    pub sample: Option<ThermalTuningSample>,
    pub reason: Option<TuningReason>,
    pub trial: Option<ThermalTuningTrialRecord>,
    pub decision: Option<ThermalTuningDecisionRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTrace {
    pub first_sequence: Option<u32>,
    pub last_sequence: Option<u32>,
    pub complete: bool,
    pub digest: TuningHash,
    pub gap: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningPreviewReceipt {
    pub run_id: TuningId,
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub power_class: ThermalTuningPowerClass,
    pub ram_applied_hash: TuningHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningSaveReceipt {
    pub run_id: TuningId,
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub power_class: ThermalTuningPowerClass,
    pub bank: ThermalTuningPowerClass,
    pub bank_revision: u32,
    pub bank_hash: TuningHash,
    pub atomic: bool,
    pub readback_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningEventPage {
    pub events: Vec<ThermalTuningEvent, THERMAL_TUNING_EVENT_PAGE_SIZE>,
    pub next_sequence: Option<u32>,
    pub trace: ThermalTuningTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThermalCurveSample {
    elapsed_ms: u64,
    temperature_centi_c: i16,
}

struct TrialEvaluation {
    score: [f64; 11],
    passes: bool,
    warmup_full: bool,
    max_overshoot_centi_c: Option<u16>,
    hold_peak_to_peak_centi_c: Option<u16>,
    settle_time_ms: Option<u64>,
    full_speed_limit_ms: u32,
}

struct TrialStopReasonInput {
    passes: bool,
    budget_available: bool,
    warmup_full: bool,
    settle_time_ms: Option<u64>,
    full_speed_limit_ms: u32,
    approach_to_hold: bool,
    max_overshoot_centi_c: Option<u16>,
    hold_peak_to_peak_centi_c: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalTuningError {
    InvalidPowerClass,
    InvalidTargets,
    AlreadyRunning,
    NotRunning,
    NotSealed,
    IdentityMismatch,
    EventCapacity,
    InvalidTransition,
    CandidateProfileRequired,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ObservedHoldFailure {
    point: ThermalControlProfilePointConfig,
    trial_index: u8,
    evidence: TuneEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObservedDampingFailure {
    point: ThermalControlProfilePointConfig,
    trial_index: u8,
    high_side: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThermalTuningRuntime {
    pub phase: ThermalTuningPhase,
    pub terminal_disposition: ThermalTuningTerminalDisposition,
    pub review_disposition: ThermalTuningReviewDisposition,
    pub power_class: Option<ThermalTuningPowerClass>,
    pub run_id: Option<TuningId>,
    pub candidate_id: Option<TuningId>,
    pub candidate_hash: Option<TuningHash>,
    pub targets_c: Vec<i16, THERMAL_TUNING_MAX_TARGETS>,
    pub target_index: usize,
    pub trial_index: u8,
    pub trial_purpose: ThermalTuningTrialPurpose,
    pub target_started_elapsed_ms: u64,
    pub accepted_target_mask: u16,
    pub failed_target_mask: u16,
    pub armed: bool,
    pub sealed: bool,
    pub save_confirmed: bool,
    pub preview_receipt: Option<ThermalTuningPreviewReceipt>,
    pub save_receipt: Option<ThermalTuningSaveReceipt>,
    pub trace_gap: bool,
    pub events: Vec<ThermalTuningEvent, THERMAL_TUNING_EVENT_CAPACITY>,
    pub candidate_profile: Option<ThermalControlProfileConfig>,
    batch_current_profile: Option<ThermalControlProfileConfig>,
    proposed_points: Vec<ThermalControlProfilePointConfig, 32>,
    observed_hold_failures: Vec<ObservedHoldFailure, 2>,
    observed_damping_failures: Vec<ObservedDampingFailure, 8>,
    batch_predicted_profile: Option<ThermalControlProfileConfig>,
    batch_current_score: Option<[f64; 11]>,
    batch_current_confirmable: bool,
    batch_current_failure_evidence: Option<TuneEvidence>,
    pub seed_source: TuningReason,
    pub preview_update_pending: bool,
    pub trial_start_sequence: Option<u32>,
    pub trial_start_elapsed_ms: Option<u64>,
    pub trial_start_temperature_centi_c: Option<i16>,
    pub trial_sample_start_sequence: Option<u32>,
    pub trial_sample_end_sequence: Option<u32>,
    pub warmup_full_output: bool,
    pub warmup_full_output_invalid: bool,
    pub warmup_physical_transient_started_at_ms: Option<u64>,
    pub saw_approach: bool,
    pub saw_hold: bool,
    pub first_hold_elapsed_ms: Option<u64>,
    pub warmup_exited_at_ms: Option<u64>,
    pub stable_window_started_at_ms: Option<u64>,
    pub stable_window_verified_at_ms: Option<u64>,
    pub max_temperature_centi_c: Option<i16>,
    pub min_hold_temperature_centi_c: Option<i16>,
    pub max_hold_temperature_centi_c: Option<i16>,
    trial_timing: Option<ThermalTuningTimingEvidence>,
    stable_window_sample_count: u32,
    stable_window_last_sample_ms: Option<u64>,
    longest_stable_span_ms: u64,
    max_band_excursion_centi_c: u16,
    approach_curve_samples: Vec<ThermalCurveSample, THERMAL_TUNING_APPROACH_TRACE_CAPACITY>,
    approach_curve_closed: bool,
    approach_curve_overflow: bool,
    approach_tail_abs_error_sum_c: f64,
    approach_tail_count: u32,
    approach_tail_max_above_c: f64,
    approach_tail_max_below_c: f64,
    approach_tail_first_sign: i8,
    approach_tail_last_sign: i8,
    approach_tail_sign_changes: u16,
    approach_curve_last_elapsed_ms: Option<u64>,
    hold_output_histogram: [u16; 1001],
    hold_output_count: u32,
    hold_entry_output_permille: Option<u16>,
    hold_entry_pulse_only: bool,
    approach_output_histogram: [u16; 1001],
    approach_output_count: u32,
    model_approach_sample_count: u32,
    model_approach_floor_only: bool,
    model_approach_output_range: Option<(u16, u16)>,
    approach_slopes_c_per_s: Vec<f64, THERMAL_TUNING_APPROACH_TRACE_CAPACITY>,
    near_target_evidence_overflow: bool,
    previous_sample_at_ms: Option<u64>,
    previous_sample_temperature_centi_c: Option<i16>,
    first_controller_hold_elapsed_ms: Option<u64>,
    first_hold_temperature_centi_c: Option<i16>,
    min_controller_hold_temperature_centi_c: Option<i16>,
    max_controller_hold_temperature_centi_c: Option<i16>,
    hold_error_sum_centi_c: i64,
    hold_threshold_crossed: bool,
    first_band_elapsed_ms: Option<u64>,
    first_band_exit_centi_c: Option<i16>,
    deadline_nearest_distance_ms: u64,
    deadline_nearest_temperature_centi_c: Option<i16>,
    trace_digest: TuningHash,
    retained_trace_digest: TuningHash,
    first_sequence: Option<u32>,
    last_sequence: Option<u32>,
    last_delivered_sequence: Option<u32>,
    last_acknowledged_sequence: Option<u32>,
    next_sequence: u32,
    current_elapsed_ms: u64,
    sample_clock_origin_ms: Option<u64>,
    last_live_sample_elapsed_ms: Option<u64>,
}

impl Default for ThermalTuningRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ThermalTuningRuntime {
    pub const fn new() -> Self {
        Self {
            phase: ThermalTuningPhase::Idle,
            terminal_disposition: ThermalTuningTerminalDisposition::Running,
            review_disposition: ThermalTuningReviewDisposition::Pending,
            power_class: None,
            run_id: None,
            candidate_id: None,
            candidate_hash: None,
            targets_c: Vec::new(),
            target_index: 0,
            trial_index: 0,
            trial_purpose: ThermalTuningTrialPurpose::Scout,
            target_started_elapsed_ms: 0,
            accepted_target_mask: 0,
            failed_target_mask: 0,
            armed: false,
            sealed: false,
            save_confirmed: false,
            preview_receipt: None,
            save_receipt: None,
            trace_gap: false,
            events: Vec::new(),
            candidate_profile: None,
            batch_current_profile: None,
            proposed_points: Vec::new(),
            observed_hold_failures: Vec::new(),
            observed_damping_failures: Vec::new(),
            batch_predicted_profile: None,
            batch_current_score: None,
            batch_current_confirmable: false,
            batch_current_failure_evidence: None,
            seed_source: TuningReason::new(),
            preview_update_pending: false,
            trial_start_sequence: None,
            trial_start_elapsed_ms: None,
            trial_start_temperature_centi_c: None,
            trial_sample_start_sequence: None,
            trial_sample_end_sequence: None,
            warmup_full_output: false,
            warmup_full_output_invalid: false,
            warmup_physical_transient_started_at_ms: None,
            saw_approach: false,
            saw_hold: false,
            first_hold_elapsed_ms: None,
            warmup_exited_at_ms: None,
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
            max_temperature_centi_c: None,
            min_hold_temperature_centi_c: None,
            max_hold_temperature_centi_c: None,
            trial_timing: None,
            stable_window_sample_count: 0,
            stable_window_last_sample_ms: None,
            longest_stable_span_ms: 0,
            max_band_excursion_centi_c: 0,
            approach_curve_samples: Vec::new(),
            approach_curve_closed: false,
            approach_curve_overflow: false,
            approach_tail_abs_error_sum_c: 0.0,
            approach_tail_count: 0,
            approach_tail_max_above_c: 0.0,
            approach_tail_max_below_c: 0.0,
            approach_tail_first_sign: 0,
            approach_tail_last_sign: 0,
            approach_tail_sign_changes: 0,
            approach_curve_last_elapsed_ms: None,
            hold_output_histogram: [0; 1001],
            hold_output_count: 0,
            hold_entry_output_permille: None,
            hold_entry_pulse_only: true,
            approach_output_histogram: [0; 1001],
            approach_output_count: 0,
            model_approach_sample_count: 0,
            model_approach_floor_only: true,
            model_approach_output_range: None,
            approach_slopes_c_per_s: Vec::new(),
            near_target_evidence_overflow: false,
            previous_sample_at_ms: None,
            previous_sample_temperature_centi_c: None,
            first_controller_hold_elapsed_ms: None,
            first_hold_temperature_centi_c: None,
            min_controller_hold_temperature_centi_c: None,
            max_controller_hold_temperature_centi_c: None,
            hold_error_sum_centi_c: 0,
            hold_threshold_crossed: false,
            first_band_elapsed_ms: None,
            first_band_exit_centi_c: None,
            deadline_nearest_distance_ms: u64::MAX,
            deadline_nearest_temperature_centi_c: None,
            trace_digest: TuningHash::new(),
            retained_trace_digest: TuningHash::new(),
            first_sequence: None,
            last_sequence: None,
            last_delivered_sequence: None,
            last_acknowledged_sequence: None,
            next_sequence: 0,
            current_elapsed_ms: 0,
            sample_clock_origin_ms: None,
            last_live_sample_elapsed_ms: None,
        }
    }

    pub fn start(&mut self, request: ThermalTuningStart<'_>) -> Result<(), ThermalTuningError> {
        self.start_at(request, 0)
    }

    pub fn start_at(
        &mut self,
        request: ThermalTuningStart<'_>,
        started_at_ms: u64,
    ) -> Result<(), ThermalTuningError> {
        self.start_inner(request, started_at_ms, false)
    }

    /// Begin only the disarmed cooling/power preparation stage. The caller
    /// must verify the matching 5A APDO and bound the PPS confirmation wait;
    /// unconfirmed contracts still cannot be recorded or admit Warmup.
    pub fn start_preparing_power_at(
        &mut self,
        request: ThermalTuningStart<'_>,
        started_at_ms: u64,
    ) -> Result<(), ThermalTuningError> {
        self.start_inner(request, started_at_ms, true)
    }

    fn start_inner(
        &mut self,
        request: ThermalTuningStart<'_>,
        started_at_ms: u64,
        prepare_power: bool,
    ) -> Result<(), ThermalTuningError> {
        let ThermalTuningStart {
            run_id,
            candidate_id,
            candidate_hash,
            power_class,
            targets,
            pps_ready,
            thermal_model_available,
            calibration_conflict,
            candidate_profile,
        } = request;
        if self.armed {
            return Err(ThermalTuningError::AlreadyRunning);
        }
        self.reset_for_run();
        self.sample_clock_origin_ms = Some(started_at_ms);
        self.power_class = Some(power_class);
        self.run_id = Some(run_id);
        self.candidate_id = Some(candidate_id);
        self.candidate_hash = Some(candidate_hash);
        self.candidate_profile = candidate_profile;
        self.seed_source = bounded_reason("initial_candidate");
        if targets.is_empty() || targets.len() > THERMAL_TUNING_MAX_TARGETS {
            self.fail("invalid_target_scope");
            return Err(ThermalTuningError::InvalidTargets);
        }
        let mut previous_position = None;
        for target in targets {
            let Some(position) = THERMAL_TUNING_EXECUTION_ORDER_C
                .iter()
                .position(|configured| configured == target)
            else {
                self.fail("invalid_target_scope");
                return Err(ThermalTuningError::InvalidTargets);
            };
            if previous_position.is_some_and(|previous| position <= previous) {
                self.fail("invalid_target_order");
                return Err(ThermalTuningError::InvalidTargets);
            }
            previous_position = Some(position);
            if self.targets_c.push(*target).is_err() {
                self.fail("invalid_target_scope");
                return Err(ThermalTuningError::InvalidTargets);
            }
        }
        if !pps_ready && !prepare_power {
            self.fail("pps_contract_unavailable");
            return Err(ThermalTuningError::InvalidTransition);
        }
        if !thermal_model_available {
            self.fail("thermal_model_unavailable");
            return Err(ThermalTuningError::InvalidTransition);
        }
        if calibration_conflict {
            self.fail("calibration_conflict");
            return Err(ThermalTuningError::InvalidTransition);
        }
        if self.candidate_profile.as_ref().is_none_or(|profile| {
            self.targets_c
                .iter()
                .any(|target| candidate_point(profile, *target).is_none())
        }) {
            self.fail("candidate_profile_required");
            return Err(ThermalTuningError::CandidateProfileRequired);
        }
        self.phase = ThermalTuningPhase::Cooldown;
        self.terminal_disposition = ThermalTuningTerminalDisposition::Running;
        self.review_disposition = ThermalTuningReviewDisposition::Pending;
        self.armed = true;
        self.preview_update_pending = true;
        self.reset_trial_metrics();
        self.push_phase_transition_event(if pps_ready {
            "cooling before first trial"
        } else {
            "preparing PPS5A working power before first trial"
        });
        Ok(())
    }

    fn finish_trial(&mut self, elapsed_ms: u64) -> Result<(), ThermalTuningError> {
        let target_c = self
            .targets_c
            .get(self.target_index)
            .copied()
            .ok_or(ThermalTuningError::InvalidTransition)?;
        let max_overshoot_centi_c = self
            .max_temperature_centi_c
            .map(|value| value.saturating_sub(target_c.saturating_mul(100)).max(0) as u16);
        let hold_peak_to_peak_centi_c = self
            .min_hold_temperature_centi_c
            .zip(self.max_hold_temperature_centi_c)
            .map(|(minimum, maximum)| maximum.saturating_sub(minimum).max(0) as u16);
        let settle_time_ms = self
            .warmup_exited_at_ms
            .zip(self.stable_window_started_at_ms)
            .map(|(warmup_exit, stable_start)| stable_start.saturating_sub(warmup_exit));
        let full_speed_limit_ms = self.active_settle_limit_ms(target_c);
        let warmup_full = self.warmup_full_output && !self.warmup_full_output_invalid;
        let approach_to_hold = self.saw_approach && self.saw_hold;
        let scout_hold_complete = self.first_hold_elapsed_ms.is_some_and(|started| {
            elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_SCOUT_HOLD_MS
        });
        let hold_confirm = self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm
            && self.first_hold_elapsed_ms.is_some_and(|started| {
                elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_HOLD_CONFIRM_MS
            });
        let budget_available = self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            < THERMAL_TUNING_TARGET_BUDGET_MS;
        // A continuous stable window that was already established before the
        // target budget expires is allowed to finish its bounded 10 s witness.
        // The target clock is never reset and a window that starts after the
        // budget remains a hard failure.
        let budget_window_continuation = self.target_budget_window_is_verified(budget_available);
        let metric_passes = (budget_available || budget_window_continuation)
            && self
                .trial_start_elapsed_ms
                .is_some_and(|started| elapsed_ms.saturating_sub(started) < TRIAL_TIMEOUT_MS)
            && warmup_full
            && !self.approach_curve_overflow
            && !self.near_target_evidence_overflow
            && approach_to_hold
            && scout_hold_complete
            && self.stable_window_verified_at_ms.is_some()
            && max_overshoot_centi_c.is_some_and(|value| value <= 300)
            && hold_peak_to_peak_centi_c.is_some_and(|value| value <= 300)
            && settle_time_ms.is_some_and(|value| value <= u64::from(full_speed_limit_ms));
        let passes = metric_passes
            && (self.trial_purpose != ThermalTuningTrialPurpose::HoldConfirm || hold_confirm);
        let stop_reason = self.trial_stop_reason(TrialStopReasonInput {
            passes,
            budget_available,
            warmup_full,
            settle_time_ms,
            full_speed_limit_ms,
            approach_to_hold,
            max_overshoot_centi_c,
            hold_peak_to_peak_centi_c,
        });
        let decision = ThermalTuningDecisionRecord {
            disposition: bounded_reason(if passes && hold_confirm {
                "adopted"
            } else if passes {
                "passed"
            } else {
                "rejected"
            }),
            purpose: self.trial_purpose,
            max_overshoot_centi_c,
            hold_peak_to_peak_centi_c,
            settle_time_ms: settle_time_ms.map(|value| value.min(u64::from(u32::MAX)) as u32),
            score: self.trial_score(
                target_c,
                scout_hold_complete && budget_available,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
            ),
            warmup_full,
            approach_to_hold,
            hold_confirm,
            hard_gate_overshoot_centi_c: 300,
            hard_gate_hold_peak_to_peak_centi_c: 300,
            hard_gate_full_speed_to_stable_ms: full_speed_limit_ms,
            freeze: passes && hold_confirm,
            prune: !passes,
            stop_reason: bounded_reason(stop_reason),
            timing: self.trial_timing,
        };
        self.push_closed_trial_event(elapsed_ms);
        let score = decision.score;
        self.push_decision_event(decision);
        self.advance_after_trial(
            target_c,
            TrialEvaluation {
                score,
                passes,
                warmup_full,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            },
        )
    }

    fn trial_stop_reason(&self, input: TrialStopReasonInput) -> &'static str {
        if input.passes && self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm {
            "candidate passed hard gates and 60s hold confirm"
        } else if input.passes {
            "candidate passed 12s scout hard gates"
        } else if !input.budget_available {
            "target monotonic tuning budget exhausted"
        } else if !input.warmup_full {
            "warmup logical full-output hard gate failed"
        } else if self.approach_curve_overflow {
            "approach_curve_evidence_overflow"
        } else if self.near_target_evidence_overflow {
            "near_target_evidence_overflow"
        } else if self.trial_start_elapsed_ms.is_some_and(|started| {
            self.current_elapsed_ms.saturating_sub(started) >= TRIAL_TIMEOUT_MS
        }) {
            "trial stage timeout"
        } else if self.extension_thermal_gate_failed() {
            "conditional extension thermal gate failed"
        } else if input
            .settle_time_ms
            .is_none_or(|value| value > u64::from(input.full_speed_limit_ms))
        {
            "full-speed-to-stable budget exhausted"
        } else if !input.approach_to_hold {
            "approach-to-hold hard gate failed"
        } else if input.max_overshoot_centi_c.is_some_and(|value| value > 300) {
            "overshoot hard gate exceeded"
        } else if input
            .hold_peak_to_peak_centi_c
            .is_some_and(|value| value > 300)
        {
            "hold peak-to-peak hard gate exceeded"
        } else {
            "hold confirmation failed"
        }
    }

    fn advance_after_trial(
        &mut self,
        target_c: i16,
        evaluation: TrialEvaluation,
    ) -> Result<(), ThermalTuningError> {
        let TrialEvaluation {
            score,
            passes,
            warmup_full,
            max_overshoot_centi_c,
            hold_peak_to_peak_centi_c,
            settle_time_ms,
            full_speed_limit_ms,
        } = evaluation;
        let failure_evidence = if !passes && warmup_full {
            let evidence = self.candidate_failure_evidence(
                target_c,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            );
            self.remember_observed_hold_failure(target_c, evidence);
            self.remember_damping_failure(target_c, evidence);
            Some(evidence)
        } else {
            None
        };
        match self.trial_purpose {
            ThermalTuningTrialPurpose::Scout if passes => self.schedule_trial(
                ThermalTuningTrialPurpose::HoldConfirm,
                "cooling before hold confirm",
            ),
            ThermalTuningTrialPurpose::Scout if !warmup_full => {
                self.failed_target_mask |= target_bit(target_c);
                self.advance_target()
            }
            ThermalTuningTrialPurpose::Scout => self.prepare_candidate_batch(
                target_c,
                score,
                failure_evidence.ok_or(ThermalTuningError::InvalidTransition)?,
            ),
            ThermalTuningTrialPurpose::BatchCurrent => {
                self.batch_current_score = Some(score);
                self.batch_current_confirmable = passes;
                self.batch_current_failure_evidence = failure_evidence;
                if let Some(predicted) = self.batch_predicted_profile {
                    self.install_candidate_profile(predicted, "predicted_from_scout")?;
                    self.schedule_trial(
                        ThermalTuningTrialPurpose::BatchPredicted,
                        "cooling before predicted candidate",
                    )
                } else {
                    self.finish_candidate_batch(target_c, score, passes, failure_evidence)
                }
            }
            ThermalTuningTrialPurpose::BatchPredicted => {
                self.finish_candidate_batch(target_c, score, passes, failure_evidence)
            }
            ThermalTuningTrialPurpose::HoldConfirm if passes => {
                self.accepted_target_mask |= target_bit(target_c);
                self.advance_target()
            }
            ThermalTuningTrialPurpose::HoldConfirm => self.retry_or_fail_trial(
                target_c,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            ),
        }
    }

    fn retry_or_fail_trial(
        &mut self,
        target_c: i16,
        max_overshoot_centi_c: Option<u16>,
        hold_peak_to_peak_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) -> Result<(), ThermalTuningError> {
        if self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            < THERMAL_TUNING_TARGET_BUDGET_MS
            && self.trial_index < u8::MAX
        {
            if !self.prepare_fresh_failure_proposal(
                target_c,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            ) {
                return self.exhaust_candidate_search(target_c);
            }
            self.seed_source = bounded_reason("evidence_adjustment");
            return self.schedule_trial(ThermalTuningTrialPurpose::Scout, "cooling before retry");
        }
        self.failed_target_mask |= target_bit(target_c);
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some("target exhausted monotonic tuning budget"),
        );
        self.advance_target()
    }

    fn schedule_trial(
        &mut self,
        purpose: ThermalTuningTrialPurpose,
        reason: &str,
    ) -> Result<(), ThermalTuningError> {
        let target_c = self.targets_c[self.target_index];
        if self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            >= THERMAL_TUNING_TARGET_BUDGET_MS
            || self.trial_index == u8::MAX
        {
            self.failed_target_mask |= target_bit(target_c);
            return self.advance_target();
        }
        self.trial_index += 1;
        self.trial_purpose = purpose;
        self.phase = ThermalTuningPhase::Cooldown;
        self.reset_trial_metrics();
        self.push_phase_transition_event(reason);
        Ok(())
    }

    fn install_candidate_profile(
        &mut self,
        mut profile: ThermalControlProfileConfig,
        seed_source: &str,
    ) -> Result<(), ThermalTuningError> {
        profile.sanitize();
        let hash = hash_profile(&profile).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        self.candidate_profile = Some(profile);
        self.candidate_hash = Some(hash);
        self.seed_source = bounded_reason(seed_source);
        self.preview_update_pending = true;
        self.preview_receipt = None;
        Ok(())
    }

    fn prepare_candidate_batch(
        &mut self,
        target_c: i16,
        current_score: [f64; 11],
        current_evidence: TuneEvidence,
    ) -> Result<(), ThermalTuningError> {
        let current = self
            .candidate_profile
            .ok_or(ThermalTuningError::CandidateProfileRequired)?;
        if !self.prepare_fresh_failure_proposal_from_evidence(target_c, current_evidence) {
            return self.exhaust_candidate_search(target_c);
        }
        let predicted = self.candidate_profile.filter(|profile| *profile != current);
        if predicted.is_none() {
            return self.exhaust_candidate_search(target_c);
        }
        self.batch_current_profile = Some(current);
        self.batch_predicted_profile = predicted;
        self.batch_current_score = Some(current_score);
        self.batch_current_confirmable = false;
        self.batch_current_failure_evidence = Some(current_evidence);
        self.install_candidate_profile(predicted.unwrap(), "predicted_from_completed_scout")?;
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some("batch_current_reuses_completed_scout"),
        );
        self.schedule_trial(
            ThermalTuningTrialPurpose::BatchPredicted,
            "cooling before predicted candidate comparison",
        )
    }

    fn record_fresh_proposal(&mut self, previous: Option<ThermalControlProfileConfig>) -> bool {
        let target = self.targets_c[self.target_index];
        let point = self.candidate_profile.and_then(|profile| {
            profile
                .points
                .into_iter()
                .flatten()
                .find(|point| point.target_temp_c == target)
        });
        let previous_point = previous.and_then(|profile| {
            profile
                .points
                .into_iter()
                .flatten()
                .find(|point| point.target_temp_c == target)
        });
        let Some(point) = point else {
            return false;
        };
        // A candidate whose static brake already covers the entire allowed
        // start gap can skip Warmup before its full-output witness exists.
        let start_gap_centi_c = (target - cooldown_threshold_c(target)) as u16 * 100;
        if point.brake_distance_centi_c >= start_gap_centi_c {
            return false;
        }
        if previous_point.is_some_and(|previous| same_effective_search_point(previous, point))
            || self
                .proposed_points
                .iter()
                .any(|previous| same_effective_search_point(*previous, point))
        {
            return false;
        }
        if let Some(previous) = previous_point
            && !self.proposed_points.contains(&previous)
            && self.proposed_points.push(previous).is_err()
        {
            return false;
        }
        self.proposed_points.push(point).is_ok()
    }

    fn remember_observed_hold_failure(&mut self, target_c: i16, evidence: TuneEvidence) {
        if evidence.target_c != target_c || !observed_low_side_hold_failure(evidence) {
            return;
        }
        let Some(point) = self
            .candidate_profile
            .as_ref()
            .and_then(|profile| {
                profile
                    .points
                    .iter()
                    .flatten()
                    .find(|point| point.target_temp_c == target_c)
            })
            .copied()
        else {
            return;
        };
        let observed = ObservedHoldFailure {
            point,
            trial_index: self.trial_index,
            evidence,
        };
        if let Some(previous) = self
            .observed_hold_failures
            .iter_mut()
            .find(|previous| same_effective_search_point(previous.point, point))
        {
            *previous = observed;
        } else {
            if self.observed_hold_failures.is_full() {
                self.observed_hold_failures.remove(0);
            }
            let _ = self.observed_hold_failures.push(observed);
        }
    }

    fn prepare_fresh_failure_proposal(
        &mut self,
        target_c: i16,
        overshoot: Option<u16>,
        hold_p2p: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) -> bool {
        let evidence = self.candidate_failure_evidence(
            target_c,
            overshoot,
            hold_p2p,
            settle_time_ms,
            full_speed_limit_ms,
        );
        self.prepare_fresh_failure_proposal_from_evidence(target_c, evidence)
    }

    fn remember_damping_failure(&mut self, target_c: i16, evidence: TuneEvidence) {
        if evidence.target_c != target_c {
            return;
        }
        let Some(high_side) = damping_failure_side(evidence) else {
            return;
        };
        let Some(point) = self.candidate_profile.and_then(|profile| {
            profile
                .points
                .into_iter()
                .flatten()
                .find(|p| p.target_temp_c == target_c)
        }) else {
            return;
        };
        let observed = ObservedDampingFailure {
            point,
            trial_index: self.trial_index,
            high_side,
        };
        if let Some(previous) = self
            .observed_damping_failures
            .iter_mut()
            .find(|previous| same_effective_search_point(previous.point, point))
        {
            // Selected-current feedback can be proposed while the runtime's
            // index still names the predicted trial. Retain the actual witness.
            if previous.high_side != high_side {
                *previous = observed;
            }
        } else {
            if self.observed_damping_failures.is_full() {
                self.observed_damping_failures.remove(0);
            }
            let _ = self.observed_damping_failures.push(observed);
        }
    }

    fn prepare_damping_bracket(&mut self, original: ThermalControlProfileConfig) -> bool {
        let target = self.targets_c[self.target_index];
        let Some(index) = original
            .points
            .iter()
            .position(|p| p.is_some_and(|p| p.target_temp_c == target))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        let mut bracket: Option<(ObservedDampingFailure, ObservedDampingFailure)> = None;
        for high in &self.observed_damping_failures {
            if !high.high_side || !same_point_except_damping(high.point, previous) {
                continue;
            }
            let low = self
                .observed_damping_failures
                .iter()
                .filter(|low| {
                    !low.high_side
                        && same_point_except_damping(low.point, previous)
                        && low
                            .point
                            .approach_damping_exponent_permille
                            .saturating_sub(high.point.approach_damping_exponent_permille)
                            >= 2
                })
                .min_by_key(|low| low.point.approach_damping_exponent_permille);
            let Some(low) = low else {
                continue;
            };
            let width = low.point.approach_damping_exponent_permille
                - high.point.approach_damping_exponent_permille;
            if bracket.is_none_or(|(h, l)| {
                width
                    < l.point.approach_damping_exponent_permille
                        - h.point.approach_damping_exponent_permille
            }) {
                bracket = Some((*high, *low));
            }
        }
        let Some((high, low)) = bracket else {
            return false;
        };
        let mut proposed = original;
        let midpoint = high.point.approach_damping_exponent_permille
            + (low.point.approach_damping_exponent_permille
                - high.point.approach_damping_exponent_permille)
                / 2;
        proposed.points[index]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille = midpoint;
        proposed.sanitize();
        self.candidate_profile = Some(proposed);
        if !self.record_fresh_proposal(Some(original)) {
            self.candidate_profile = Some(original);
            return false;
        }
        if self
            .install_candidate_profile(proposed, "same_run_damping_bracket")
            .is_err()
        {
            self.candidate_profile = Some(original);
            return false;
        }
        let mut reason = TuningReason::new();
        let _ = write!(
            reason,
            "same_run_damping_bracket_trials_{}_{}_midpoint_{}",
            high.trial_index, low.trial_index, midpoint
        );
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target),
            Some(self.trial_index),
            None,
            Some(reason.as_str()),
        );
        true
    }

    fn prepare_fresh_failure_proposal_from_evidence(
        &mut self,
        target_c: i16,
        evidence: TuneEvidence,
    ) -> bool {
        if evidence.target_c != target_c {
            return false;
        }
        let Some(original) = self.candidate_profile else {
            return false;
        };
        self.remember_damping_failure(target_c, evidence);
        if damping_failure_side(evidence).is_some() && self.prepare_damping_bracket(original) {
            return true;
        }
        self.mutate_candidate_from_evidence(target_c, evidence);
        if self.record_fresh_proposal(Some(original)) {
            return true;
        }
        if self.prepare_smaller_braking_proposal(original, target_c) {
            return true;
        }
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        for attempt in 0..32 {
            let Some(point) = bounded_failure_alternative(previous, evidence, attempt) else {
                continue;
            };
            let mut alternative = original;
            alternative.points[index] = Some(point);
            alternative.sanitize();
            self.candidate_profile = Some(alternative);
            if !self.record_fresh_proposal(Some(original)) {
                continue;
            }
            if self
                .install_candidate_profile(alternative, "distinct_same_run_failure_proposal")
                .is_err()
            {
                return false;
            }
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(target_c),
                Some(self.trial_index),
                None,
                Some("repeated_proposal_replaced_by_distinct_effective_point"),
            );
            return true;
        }
        self.candidate_profile = Some(original);
        self.candidate_hash = hash_profile(&original);
        if self.prepare_observed_hold_failure_proposal(original, index, evidence) {
            return true;
        }
        self.candidate_profile = Some(original);
        self.candidate_hash = hash_profile(&original);
        false
    }

    fn prepare_smaller_braking_proposal(
        &mut self,
        original: ThermalControlProfileConfig,
        target_c: i16,
    ) -> bool {
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        let Some(mut proposed_point) = self
            .candidate_profile
            .and_then(|profile| profile.points[index])
        else {
            return false;
        };
        let requested_brake = proposed_point.brake_distance_centi_c;
        proposed_point.brake_distance_centi_c = previous.brake_distance_centi_c;
        let start_gap = (target_c - cooldown_threshold_c(target_c)) as u16 * 100;
        if requested_brake < start_gap
            || requested_brake <= previous.brake_distance_centi_c
            || !same_effective_search_point(previous, proposed_point)
        {
            return false;
        }
        // Preserve the measured coarse direction without discarding it for a
        // tiny fallback when the full step would consume the Warmup gap.
        let mut step = (requested_brake - previous.brake_distance_centi_c) / 2;
        while step >= 40 {
            let mut proposed = original;
            proposed.points[index]
                .as_mut()
                .unwrap()
                .brake_distance_centi_c = previous.brake_distance_centi_c.saturating_add(step);
            proposed.sanitize();
            self.candidate_profile = Some(proposed);
            if !self.record_fresh_proposal(Some(original)) {
                step /= 2;
                continue;
            }
            if self
                .install_candidate_profile(proposed, "bounded_coarse_braking")
                .is_err()
            {
                return false;
            }
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(target_c),
                Some(self.trial_index),
                None,
                Some("coarse_braking_step_bisected_to_preserve_warmup"),
            );
            return true;
        }
        self.candidate_profile = Some(original);
        false
    }

    fn prepare_observed_hold_failure_proposal(
        &mut self,
        original: ThermalControlProfileConfig,
        index: usize,
        evidence: TuneEvidence,
    ) -> bool {
        let previous = original.points[index].unwrap();
        let target_c = previous.target_temp_c;
        // A later pre-Hold failure does not erase an earlier measured Hold
        // deficit for this exact effective point. Use it only after the live
        // Approach alternatives are exhausted, never as qualification data.
        if evidence.target_c != target_c || !unobserved_low_side_failure(evidence) {
            return false;
        }
        let Some(observed) = self
            .observed_hold_failures
            .iter()
            .find(|observed| {
                observed.evidence.target_c == target_c
                    && same_effective_search_point(observed.point, previous)
            })
            .copied()
        else {
            return false;
        };
        for attempt in 0..32 {
            let Some(point) = bounded_failure_alternative(previous, observed.evidence, attempt)
            else {
                continue;
            };
            let mut alternative = original;
            alternative.points[index] = Some(point);
            alternative.sanitize();
            self.candidate_profile = Some(alternative);
            if !self.record_fresh_proposal(Some(original)) {
                continue;
            }
            if self
                .install_candidate_profile(alternative, "same_run_observed_hold_failure")
                .is_err()
            {
                return false;
            }
            let mut reason = TuningReason::new();
            let _ = write!(
                reason,
                "same_run_hold_failure_trial_{}_proposed_distinct_point",
                observed.trial_index
            );
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(target_c),
                Some(self.trial_index),
                None,
                Some(reason.as_str()),
            );
            return true;
        }
        false
    }

    fn exhaust_candidate_search(&mut self, target_c: i16) -> Result<(), ThermalTuningError> {
        self.failed_target_mask |= target_bit(target_c);
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some("candidate_search_exhausted_no_distinct_safe_proposal"),
        );
        self.advance_target()
    }

    fn finish_candidate_batch(
        &mut self,
        target_c: i16,
        score: [f64; 11],
        confirmable: bool,
        predicted_failure_evidence: Option<TuneEvidence>,
    ) -> Result<(), ThermalTuningError> {
        let current_score = self.batch_current_score.unwrap_or(score);
        let current_confirmable = self.batch_current_confirmable;
        let predicted_wins = self.trial_purpose == ThermalTuningTrialPurpose::BatchPredicted
            && (confirmable && !current_confirmable
                || confirmable == current_confirmable && score_precedes(&score, &current_score));
        // A measured stronger-damping loss should not starve the existing
        // stored-heat channel by retrying finer damping around the winner.
        let explore_braking = !predicted_wins
            && !confirmable
            && !current_confirmable
            && failed_damping_comparison_needs_braking(
                self.batch_current_profile,
                self.batch_predicted_profile,
                self.batch_current_failure_evidence,
                predicted_failure_evidence,
                target_c,
            );
        let selected_failure_evidence = if predicted_wins {
            predicted_failure_evidence
        } else {
            self.batch_current_failure_evidence
        };
        if !predicted_wins {
            let current = self
                .batch_current_profile
                .ok_or(ThermalTuningError::CandidateProfileRequired)?;
            self.install_candidate_profile(current, "batch_selected_current")?;
        } else {
            self.seed_source = bounded_reason("batch_selected_predicted");
        }
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some(if predicted_wins {
                "candidate_batch_selected_predicted"
            } else {
                "candidate_batch_selected_current"
            }),
        );
        self.batch_current_profile = None;
        self.batch_predicted_profile = None;
        self.batch_current_score = None;
        self.batch_current_confirmable = false;
        self.batch_current_failure_evidence = None;
        let selected_confirmable = if predicted_wins {
            confirmable
        } else {
            current_confirmable
        };
        if !selected_confirmable {
            // Reuse only the selected profile's measured failure. Repeating
            // this failed profile as a Scout spends heat without a new proposal.
            let Some(evidence) = selected_failure_evidence else {
                return self.exhaust_candidate_search(target_c);
            };
            if !(self.prepare_fresh_failure_proposal_from_evidence(target_c, evidence)
                || explore_braking && self.prepare_failed_damping_braking(target_c, evidence))
            {
                return self.exhaust_candidate_search(target_c);
            }
            self.seed_source = bounded_reason("selected_failure_adjustment");
        }
        self.schedule_trial(
            if selected_confirmable {
                ThermalTuningTrialPurpose::HoldConfirm
            } else {
                ThermalTuningTrialPurpose::Scout
            },
            if selected_confirmable {
                "cooling before selected candidate hold confirm"
            } else {
                "cooling before next tuning round"
            },
        )
    }

    fn prepare_failed_damping_braking(&mut self, target_c: i16, evidence: TuneEvidence) -> bool {
        let Some(original) = self.candidate_profile else {
            return false;
        };
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        for attempt in (1..32).step_by(2) {
            let Some(point) = bounded_failure_alternative(previous, evidence, attempt) else {
                continue;
            };
            if point.brake_distance_centi_c <= previous.brake_distance_centi_c {
                continue;
            }
            let mut proposed = original;
            proposed.points[index] = Some(point);
            proposed.sanitize();
            self.candidate_profile = Some(proposed);
            if !self.record_fresh_proposal(Some(original)) {
                continue;
            }
            if self
                .install_candidate_profile(proposed, "failed_damping_braking")
                .is_err()
            {
                return false;
            }
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(target_c),
                Some(self.trial_index),
                None,
                Some("failed_damping_comparison_explores_braking"),
            );
            return true;
        }
        self.candidate_profile = Some(original);
        false
    }

    #[cfg(test)]
    fn mutate_candidate_for_failure(
        &mut self,
        target_c: i16,
        max_overshoot_centi_c: Option<u16>,
        hold_peak_to_peak_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) {
        let evidence = self.candidate_failure_evidence(
            target_c,
            max_overshoot_centi_c,
            hold_peak_to_peak_centi_c,
            settle_time_ms,
            full_speed_limit_ms,
        );
        self.mutate_candidate_from_evidence(target_c, evidence);
    }

    fn mutate_candidate_from_evidence(&mut self, target_c: i16, evidence: TuneEvidence) {
        let Some(previous) = self
            .candidate_profile
            .as_ref()
            .and_then(|profile| {
                profile
                    .points
                    .iter()
                    .flatten()
                    .find(|point| point.target_temp_c == target_c)
            })
            .copied()
        else {
            return;
        };
        let tuned = if self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm {
            reseed_after_hold_confirm(previous, evidence)
        } else {
            tune_point(previous, evidence)
        };
        if tuned == previous {
            return;
        }
        let Some(profile) = self.candidate_profile.as_mut() else {
            return;
        };
        let Some(point) = profile
            .points
            .iter_mut()
            .flatten()
            .find(|point| point.target_temp_c == target_c)
        else {
            return;
        };
        *point = tuned;
        profile.sanitize();
        if let Some(hash) = hash_profile(profile) {
            self.candidate_hash = Some(hash);
            self.preview_update_pending = true;
            self.preview_receipt = None;
        }
    }

    fn candidate_failure_evidence(
        &self,
        target_c: i16,
        max_overshoot_centi_c: Option<u16>,
        hold_peak_to_peak_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) -> TuneEvidence {
        let first_hold_error_c = self.first_hold_temperature_centi_c.map(|temperature| {
            f64::from(i32::from(target_c) * 100 - i32::from(temperature)) / 100.0
        });
        let hold_mean_error_c = (self.hold_output_count > 0)
            .then(|| self.hold_error_sum_centi_c as f64 / (self.hold_output_count as f64 * 100.0));
        let mut slopes = self.approach_slopes_c_per_s.clone();
        slopes.sort_unstable_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
        let slope_index = slopes.len() / 2;
        let mut evidence = TuneEvidence {
            target_c,
            tunable_stop: !self.near_target_evidence_overflow && !self.approach_curve_overflow,
            full_speed_failed: self.stable_window_verified_at_ms.is_none()
                || settle_time_ms.is_none_or(|value| value > u64::from(full_speed_limit_ms)),
            overshoot_c: f64::from(max_overshoot_centi_c.unwrap_or_default()) / 100.0,
            // No qualified-Hold samples cannot establish a low ripple. Match
            // the external strategy's missing-evidence sentinel so a failed
            // lower-band trial cannot claim the accelerated energy step.
            hold_p2p_c: hold_peak_to_peak_centi_c
                .map_or(f64::INFINITY, |value| f64::from(value) / 100.0),
            residual_c: self
                .max_temperature_centi_c
                .zip(self.first_hold_temperature_centi_c)
                .map_or(
                    f64::from(max_overshoot_centi_c.unwrap_or_default()) / 100.0,
                    |(maximum, first)| {
                        f64::from(i32::from(maximum) - i32::from(first)).max(0.0) / 100.0
                    },
                ),
            under_c: self
                .min_controller_hold_temperature_centi_c
                .map_or(0.0, |minimum| {
                    f64::from(i32::from(target_c) * 100 - i32::from(minimum)).max(0.0) / 100.0
                }),
            over_c: self
                .max_controller_hold_temperature_centi_c
                .map_or(0.0, |maximum| {
                    f64::from(i32::from(maximum) - i32::from(target_c) * 100).max(0.0) / 100.0
                }),
            hold_sample_count: self.hold_output_count,
            hold_off_sample_count: u32::from(self.hold_output_histogram[0]),
            hold_entry_output_permille: self.hold_entry_output_permille,
            hold_entry_pulse_only: self.hold_entry_pulse_only,
            hold_threshold_crossed: self.hold_threshold_crossed,
            hold_mean_error_c,
            hold_max_below_c: self.min_controller_hold_temperature_centi_c.map(|minimum| {
                f64::from(i32::from(target_c) * 100 - i32::from(minimum)).max(0.0) / 100.0
            }),
            first_hold_error_c,
            hold_median_output_permille: self.hold_output_percentile(0.5).map(|value| value as u16),
            hold_p90_output_permille: self.hold_output_percentile(0.9).map(|value| value as u16),
            hold_max_output_permille: self
                .hold_output_histogram
                .iter()
                .rposition(|count| *count > 0)
                .map(|output| output as u16),
            approach_median_output_permille: histogram_percentile(
                &self.approach_output_histogram,
                self.approach_output_count,
                0.5,
            ),
            approach_floor_only: self.model_approach_sample_count >= 3
                && self.model_approach_floor_only,
            approach_output_range: (self.model_approach_sample_count >= 3)
                .then_some(self.model_approach_output_range)
                .flatten(),
            approach_median_slope_c_per_s: slopes.get(slope_index).copied(),
            curve_class: self.strategy_curve_class(target_c),
            first_hold_after_warmup_ms: self
                .first_controller_hold_elapsed_ms
                .zip(self.warmup_exited_at_ms)
                .map(|(hold, warmup)| hold.saturating_sub(warmup)),
            stable_window_after_warmup_ms: self
                .stable_window_started_at_ms
                .zip(self.warmup_exited_at_ms)
                .map(|(stable, warmup)| stable.saturating_sub(warmup)),
            settle_limit_ms: u64::from(full_speed_limit_ms),
            scout_hold_complete: self.first_hold_elapsed_ms.is_some_and(|started| {
                self.current_elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_SCOUT_HOLD_MS
            }),
            stable_window_verified: self.stable_window_verified_at_ms.is_some(),
            first_band_observed: self.first_band_elapsed_ms.is_some(),
            first_band_exit_centi_c: self.first_band_exit_centi_c,
            deadline_temperature_centi_c: self.deadline_nearest_temperature_centi_c,
        };
        if self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm {
            // Reconstruct the frozen host stage's Celsius differences before
            // its rounded reseed steps, including exact half-step boundaries.
            evidence.overshoot_c = self.max_temperature_centi_c.map_or(0.0, |maximum| {
                (f64::from(maximum) / 100.0 - f64::from(target_c)).max(0.0)
            });
            evidence.hold_p2p_c = self
                .min_hold_temperature_centi_c
                .zip(self.max_hold_temperature_centi_c)
                .map_or(0.0, |(minimum, maximum)| {
                    f64::from(maximum) / 100.0 - f64::from(minimum) / 100.0
                });
        }
        evidence
    }

    fn reset_trial_metrics(&mut self) {
        self.trial_start_sequence = Some(self.next_sequence);
        self.trial_start_elapsed_ms = None;
        self.trial_start_temperature_centi_c = None;
        self.trial_sample_start_sequence = None;
        self.trial_sample_end_sequence = None;
        self.warmup_full_output = false;
        self.warmup_full_output_invalid = false;
        self.warmup_physical_transient_started_at_ms = None;
        self.saw_approach = false;
        self.saw_hold = false;
        self.first_hold_elapsed_ms = None;
        self.warmup_exited_at_ms = None;
        self.stable_window_started_at_ms = None;
        self.stable_window_verified_at_ms = None;
        self.max_temperature_centi_c = None;
        self.min_hold_temperature_centi_c = None;
        self.max_hold_temperature_centi_c = None;
        self.trial_timing = None;
        self.stable_window_sample_count = 0;
        self.stable_window_last_sample_ms = None;
        self.longest_stable_span_ms = 0;
        self.max_band_excursion_centi_c = 0;
        self.approach_curve_samples.clear();
        self.approach_curve_closed = false;
        self.approach_curve_overflow = false;
        self.approach_tail_abs_error_sum_c = 0.0;
        self.approach_tail_count = 0;
        self.approach_tail_max_above_c = 0.0;
        self.approach_tail_max_below_c = 0.0;
        self.approach_tail_first_sign = 0;
        self.approach_tail_last_sign = 0;
        self.approach_tail_sign_changes = 0;
        self.approach_curve_last_elapsed_ms = None;
        self.hold_output_histogram = [0; 1001];
        self.hold_output_count = 0;
        self.hold_entry_output_permille = None;
        self.hold_entry_pulse_only = true;
        self.approach_output_histogram = [0; 1001];
        self.approach_output_count = 0;
        self.model_approach_sample_count = 0;
        self.model_approach_floor_only = true;
        self.model_approach_output_range = None;
        self.approach_slopes_c_per_s.clear();
        self.near_target_evidence_overflow = false;
        self.previous_sample_at_ms = None;
        self.previous_sample_temperature_centi_c = None;
        self.first_controller_hold_elapsed_ms = None;
        self.first_hold_temperature_centi_c = None;
        self.min_controller_hold_temperature_centi_c = None;
        self.max_controller_hold_temperature_centi_c = None;
        self.hold_error_sum_centi_c = 0;
        self.hold_threshold_crossed = false;
        self.first_band_elapsed_ms = None;
        self.first_band_exit_centi_c = None;
        self.deadline_nearest_distance_ms = u64::MAX;
        self.deadline_nearest_temperature_centi_c = None;
    }

    pub fn record_sample(&mut self, sample: ThermalTuningSample) -> Result<(), ThermalTuningError> {
        self.record_sample_with_cooldown_admission(sample, true)
    }

    fn record_sample_with_cooldown_admission(
        &mut self,
        mut sample: ThermalTuningSample,
        cooldown_admission_ready: bool,
    ) -> Result<(), ThermalTuningError> {
        let origin_ms = self.sample_clock_origin_ms.unwrap_or(0);
        sample.elapsed_ms = sample.elapsed_ms.saturating_sub(origin_ms);
        let expected_target = self.validate_sample(&sample)?;
        if self.phase == ThermalTuningPhase::Cooldown {
            return self.record_cooldown_sample(sample, expected_target, cooldown_admission_ready);
        }
        self.update_trial_metrics(&sample, expected_target);
        let mut sample = sample;
        let previous_phase = self.phase;
        if previous_phase != sample.phase {
            self.phase = sample.phase;
            self.push_phase_transition_event(phase_transition_reason(sample.phase));
        }
        sample.sequence = self.next_sequence;
        self.phase = sample.phase;
        if self.trial_sample_start_sequence.is_none() {
            self.trial_sample_start_sequence = Some(sample.sequence);
        }
        self.trial_sample_end_sequence = Some(sample.sequence);
        let sample_elapsed_ms = sample.elapsed_ms;
        self.push_event(
            ThermalTuningEventKind::Sample,
            Some(sample.target_c),
            Some(sample.trial_index),
            Some(sample),
            None,
        );
        if self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            >= THERMAL_TUNING_TARGET_BUDGET_MS
            && !self.target_budget_window_is_in_progress()
        {
            return self.finish_trial(sample_elapsed_ms);
        }
        if self
            .trial_start_elapsed_ms
            .is_some_and(|started| sample_elapsed_ms.saturating_sub(started) >= TRIAL_TIMEOUT_MS)
        {
            return self.finish_trial(sample_elapsed_ms);
        }
        // Preserve the ordinary deadline. Only observed logical floor output
        // plus an actual original timeout can activate the bounded exception.
        self.maybe_extend_trial(sample_elapsed_ms, expected_target);
        if self.extension_thermal_gate_failed() {
            return self.finish_trial(sample_elapsed_ms);
        }
        if self.full_speed_to_stable_timed_out(sample_elapsed_ms, expected_target) {
            return self.finish_trial(sample_elapsed_ms);
        }
        let required_hold_ms = if self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm {
            THERMAL_TUNING_HOLD_CONFIRM_MS
        } else {
            THERMAL_TUNING_SCOUT_HOLD_MS
        };
        if self
            .first_hold_elapsed_ms
            .is_some_and(|started| sample_elapsed_ms.saturating_sub(started) >= required_hold_ms)
            // Hold duration is a minimum, not a stable-window cutoff. A
            // timely window may finish its ten seconds after the deadline.
            // Invalid thermal facts still reject at the original Hold bound.
            && (self.stable_window_verified_at_ms.is_some()
                || !self.warmup_full_output
                || self.warmup_full_output_invalid
                || self.thermal_gate_failed())
        {
            self.finish_trial(sample_elapsed_ms)?;
        }
        Ok(())
    }

    fn target_budget_window_is_in_progress(&self) -> bool {
        self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm
            && self.stable_window_verified_at_ms.is_none()
            && self.target_budget_window_started_before_deadline()
    }

    fn target_budget_window_is_verified(&self, budget_available: bool) -> bool {
        !budget_available
            && self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm
            && self.stable_window_verified_at_ms.is_some()
            && self.target_budget_window_started_before_deadline()
    }

    fn target_budget_window_started_before_deadline(&self) -> bool {
        self.stable_window_started_at_ms.is_some_and(|started| {
            started
                < self
                    .target_started_elapsed_ms
                    .saturating_add(THERMAL_TUNING_TARGET_BUDGET_MS)
        })
    }

    pub fn record_live_sample(
        &mut self,
        sample: ThermalTuningSample,
    ) -> Result<(), ThermalTuningError> {
        self.record_live_sample_with_cooldown_admission(sample, true)
    }

    /// Retain rejected RTD observations for audit, without treating the held
    /// control value as fresh qualification evidence. Wall-clock budgets run.
    pub fn record_live_sample_with_cooldown_admission(
        &mut self,
        sample: ThermalTuningSample,
        cooldown_admission_ready: bool,
    ) -> Result<(), ThermalTuningError> {
        let mut checked = sample.clone();
        checked.elapsed_ms = checked
            .elapsed_ms
            .saturating_sub(self.sample_clock_origin_ms.unwrap_or(0));
        self.validate_sample(&checked)?;
        if self.phase == ThermalTuningPhase::Cooldown
            && (checked.heater_output_permille != 0
                || checked.heater_requested_output_permille != 0)
        {
            self.fail("cooldown_output_not_zero");
            return Err(ThermalTuningError::InvalidTransition);
        }
        let phase_changed =
            self.phase != ThermalTuningPhase::Cooldown && self.phase != checked.phase;
        // Keep every startup control observation through the first full PWM
        // sample. The normal 200ms recorder would hide a 100ms soft start.
        let startup_evidence_due = self.phase != ThermalTuningPhase::Cooldown
            && checked.phase == ThermalTuningPhase::Warmup
            && !self.warmup_full_output;
        let measurement_gap_due = !checked.has_fresh_measurement()
            && (self.first_hold_elapsed_ms.is_some()
                || self.stable_window_started_at_ms.is_some()
                || self.previous_sample_at_ms.is_some());
        // Recording cadence must not postpone a hard deadline or a broken
        // in-flight stable window beyond the current control cycle.
        let deadline_check_due = self.warmup_exited_at_ms.is_some_and(|warmup_exit_ms| {
            let limit_ms = u64::from(self.active_settle_limit_ms(checked.target_c));
            let first_original_check_due = self.trial_timing.as_ref().is_some_and(|timing| {
                timing.original_deadline_observed_at_ms.is_none()
                    && checked.elapsed_ms
                        >= warmup_exit_ms.saturating_add(u64::from(timing.original_limit_ms))
            });
            first_original_check_due
                || (checked.elapsed_ms >= warmup_exit_ms.saturating_add(limit_ms)
                    && self.stable_window_verified_at_ms.is_none()
                    && (self.stable_window_started_at_ms.is_none()
                        || (i32::from(checked.temperature_centi_c)
                            - i32::from(checked.target_c) * 100)
                            .abs()
                            > i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C)))
        });
        if !phase_changed
            && !startup_evidence_due
            && !measurement_gap_due
            && !deadline_check_due
            && self.last_live_sample_elapsed_ms.is_some_and(|last| {
                checked.elapsed_ms.saturating_sub(last) < THERMAL_TUNING_LIVE_SAMPLE_INTERVAL_MS
            })
        {
            return Ok(());
        }
        let previous_phase = self.phase;
        self.record_sample_with_cooldown_admission(sample, cooldown_admission_ready)?;
        self.last_live_sample_elapsed_ms = if self.phase == previous_phase {
            Some(checked.elapsed_ms)
        } else {
            None
        };
        Ok(())
    }

    fn full_speed_to_stable_timed_out(&self, elapsed_ms: u64, target_c: i16) -> bool {
        let Some(warmup_exit_ms) = self.warmup_exited_at_ms else {
            return false;
        };
        if self.stable_window_verified_at_ms.is_some() {
            return false;
        }
        let deadline_ms =
            warmup_exit_ms.saturating_add(u64::from(self.active_settle_limit_ms(target_c)));
        elapsed_ms >= deadline_ms
            && self
                .stable_window_started_at_ms
                .is_none_or(|started_ms| started_ms > deadline_ms)
    }

    fn active_settle_limit_ms(&self, target_c: i16) -> u32 {
        self.trial_timing.as_ref().map_or_else(
            || original_settle_limit_ms(target_c),
            ThermalTuningTimingEvidence::active_limit_ms,
        )
    }

    /// Bound policy and observed trigger facts, without evaluating or mutating a trial.
    pub fn current_timing_evidence(&self) -> Option<ThermalTuningTimingEvidence> {
        self.trial_timing
    }

    fn maybe_extend_trial(&mut self, elapsed_ms: u64, target_c: i16) {
        let timed_out = self.full_speed_to_stable_timed_out(elapsed_ms, target_c);
        let gates_valid = self.warmup_full_output
            && !self.warmup_full_output_invalid
            && self
                .max_temperature_centi_c
                .is_none_or(|value| value <= target_c * 100 + 300)
            && self
                .min_hold_temperature_centi_c
                .zip(self.max_hold_temperature_centi_c)
                .is_none_or(|(min, max)| i32::from(max) - i32::from(min) <= 300);
        if let Some(timing) = &mut self.trial_timing {
            let already_extended = timing.extended_at_ms.is_some();
            if timing.try_extend(elapsed_ms, timed_out && gates_valid, self.phase)
                && !already_extended
            {
                // The failure classifier now observes the bound active deadline.
                // Keep earlier band-entry/break facts and the original timing audit.
                self.deadline_nearest_distance_ms = u64::MAX;
                self.deadline_nearest_temperature_centi_c = None;
            }
        }
    }

    fn extension_thermal_gate_failed(&self) -> bool {
        self.trial_timing
            .as_ref()
            .is_some_and(|timing| timing.extended_at_ms.is_some())
            && self.thermal_gate_failed()
    }

    fn thermal_gate_failed(&self) -> bool {
        self.max_temperature_centi_c
            .is_some_and(|value| value > self.targets_c[self.target_index] * 100 + 300)
            || self
                .min_hold_temperature_centi_c
                .zip(self.max_hold_temperature_centi_c)
                .is_some_and(|(min, max)| i32::from(max) - i32::from(min) > 300)
    }

    fn validate_sample(&mut self, sample: &ThermalTuningSample) -> Result<i16, ThermalTuningError> {
        if !self.armed {
            return Err(ThermalTuningError::NotRunning);
        }
        if Some(sample.pps_contract.kind) != self.power_class
            || sample.pps_contract.current_ma < sample.pps_contract.kind.required_current_ma()
            || !sample.pps_contract.confirmed
            || !sample.measurement_valid
        {
            self.fail("unsafe_sample");
            return Err(ThermalTuningError::InvalidTransition);
        }
        if !(u32::from(FUSB302B_PD_ABSOLUTE_MIN_MV)..=u32::from(FUSB302B_PD_ABSOLUTE_MAX_MV))
            .contains(&sample.pps_power_ceiling_mv)
            || u32::from(sample.requested_mv) > sample.pps_power_ceiling_mv
        {
            self.fail("invalid_pps_voltage_ceiling");
            return Err(ThermalTuningError::InvalidTransition);
        }
        let expected_target = self
            .targets_c
            .get(self.target_index)
            .copied()
            .ok_or(ThermalTuningError::InvalidTransition)?;
        if sample.target_c != expected_target
            || sample.trial_index != self.trial_index
            || self.candidate_id.as_ref() != Some(&sample.candidate_id)
            || self.candidate_hash.as_ref() != Some(&sample.candidate_hash)
        {
            self.fail("sample_identity_mismatch");
            return Err(ThermalTuningError::IdentityMismatch);
        }
        if sample.elapsed_ms < self.current_elapsed_ms {
            self.fail("sample_time_regressed");
            return Err(ThermalTuningError::InvalidTransition);
        }
        Ok(expected_target)
    }

    fn record_cooldown_sample(
        &mut self,
        mut sample: ThermalTuningSample,
        expected_target: i16,
        cooldown_admission_ready: bool,
    ) -> Result<(), ThermalTuningError> {
        if sample.heater_output_permille != 0 || sample.heater_requested_output_permille != 0 {
            self.fail("cooldown_output_not_zero");
            return Err(ThermalTuningError::InvalidTransition);
        }
        self.current_elapsed_ms = sample.elapsed_ms;
        sample.sequence = self.next_sequence;
        sample.phase = ThermalTuningPhase::Cooldown;
        let start_temperature_centi_c = sample.temperature_centi_c;
        let cooled = i32::from(sample.temperature_centi_c)
            <= i32::from(cooldown_threshold_c(expected_target)) * 100;
        // Cooldown intentionally restores the request to the idle voltage. The
        // settled contract, rather than the pending request, is the safety
        // evidence that the next warmup can raise power to the ceiling.
        let trial_power_ready = sample.pps_contract.confirmed
            && u32::from(sample.pps_contract.voltage_mv) == sample.pps_power_ceiling_mv;
        let measurement_accepted = sample.has_fresh_measurement();
        self.push_event(
            ThermalTuningEventKind::Sample,
            Some(expected_target),
            Some(self.trial_index),
            Some(sample),
            None,
        );
        if self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            >= THERMAL_TUNING_TARGET_BUDGET_MS
        {
            self.failed_target_mask |= target_bit(expected_target);
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(expected_target),
                Some(self.trial_index),
                None,
                Some("target_budget_exhausted_during_cooldown"),
            );
            return self.advance_target();
        }
        if cooled
            && trial_power_ready
            && cooldown_admission_ready
            && measurement_accepted
            && self.armed
        {
            self.phase = ThermalTuningPhase::Warmup;
            self.reset_trial_metrics();
            self.trial_start_temperature_centi_c = Some(start_temperature_centi_c);
            self.push_candidate_trial_event(self.trial_index);
            self.push_phase_transition_event("cooldown complete; entered warmup phase");
        }
        Ok(())
    }

    fn update_trial_metrics(&mut self, sample: &ThermalTuningSample, expected_target: i16) {
        if self.trial_start_elapsed_ms.is_none() {
            self.trial_start_elapsed_ms = Some(sample.elapsed_ms);
        }
        self.current_elapsed_ms = sample.elapsed_ms;
        self.update_warmup_output_evidence(sample);
        match sample.phase {
            ThermalTuningPhase::Approach => {
                self.model_approach_sample_count =
                    self.model_approach_sample_count.saturating_add(1);
                self.model_approach_floor_only &= sample.heater_requested_output_permille
                    == crate::thermal_plant::APPROACH_MIN_PERMILLE;
                let requested = sample.heater_requested_output_permille;
                self.model_approach_output_range = Some(
                    self.model_approach_output_range
                        .map_or((requested, requested), |(min, max)| {
                            (min.min(requested), max.max(requested))
                        }),
                );
                self.saw_approach = true;
                self.warmup_exited_at_ms.get_or_insert(sample.elapsed_ms);
            }
            ThermalTuningPhase::Hold => {
                self.saw_hold = true;
                self.warmup_exited_at_ms.get_or_insert(sample.elapsed_ms);
                // Strategy analysis starts at the controller's first Hold
                // sample, including entry residual heat. Qualification timing
                // starts independently when the physical hold gate is met.
                self.first_controller_hold_elapsed_ms
                    .get_or_insert(sample.elapsed_ms);
                self.record_fresh_hold_entry(sample, expected_target);
            }
            _ => {}
        }
        if !sample.has_fresh_measurement() {
            self.interrupt_measurement_qualification();
            return;
        }
        self.update_strategy_evidence(sample, expected_target);
        if sample.phase != ThermalTuningPhase::Warmup && !self.approach_curve_closed {
            self.record_approach_curve_sample(sample);
            self.approach_curve_closed = sample.phase == ThermalTuningPhase::Hold;
        }
        self.record_controller_hold_output(sample, expected_target);
        if self.first_hold_elapsed_ms.is_some() {
            self.min_hold_temperature_centi_c = Some(
                self.min_hold_temperature_centi_c
                    .map_or(sample.temperature_centi_c, |value| {
                        value.min(sample.temperature_centi_c)
                    }),
            );
            self.max_hold_temperature_centi_c = Some(
                self.max_hold_temperature_centi_c
                    .map_or(sample.temperature_centi_c, |value| {
                        value.max(sample.temperature_centi_c)
                    }),
            );
        }
        self.update_stable_window(sample, expected_target);
        if let Some(timing) = &mut self.trial_timing {
            if sample.phase == ThermalTuningPhase::Approach {
                timing.observe_approach_request(
                    sample.elapsed_ms,
                    sample.heater_requested_output_permille,
                );
            }
            timing.warmup_exited_at_ms = self.warmup_exited_at_ms;
            timing.stable_window_started_at_ms = self.stable_window_started_at_ms;
            timing.stable_window_verified_at_ms = self.stable_window_verified_at_ms;
        }
        self.update_stability_evidence(sample, expected_target);
        self.max_temperature_centi_c = Some(
            self.max_temperature_centi_c
                .map_or(sample.temperature_centi_c, |value| {
                    value.max(sample.temperature_centi_c)
                }),
        );
    }

    fn record_fresh_hold_entry(&mut self, sample: &ThermalTuningSample, target_c: i16) {
        if !sample.has_fresh_measurement() {
            return;
        }
        self.first_hold_temperature_centi_c
            .get_or_insert(sample.temperature_centi_c);
        let error_centi_c = i32::from(target_c) * 100 - i32::from(sample.temperature_centi_c);
        if hold_entry_ready(error_centi_c as f32 / 100.0) {
            self.first_hold_elapsed_ms.get_or_insert(sample.elapsed_ms);
        }
    }

    fn interrupt_measurement_qualification(&mut self) {
        // A retained value is not a new observation. Preserve a completed
        // witness, but never bridge a gap in continuous Hold confirmation.
        self.first_hold_elapsed_ms = None;
        if self.stable_window_verified_at_ms.is_none() {
            self.stable_window_started_at_ms = None;
            self.stable_window_sample_count = 0;
        }
        self.stable_window_last_sample_ms = None;
        self.previous_sample_at_ms = None;
        self.previous_sample_temperature_centi_c = None;
        if let Some(timing) = &mut self.trial_timing {
            timing.warmup_exited_at_ms = self.warmup_exited_at_ms;
            timing.stable_window_started_at_ms = self.stable_window_started_at_ms;
        }
    }

    fn record_controller_hold_output(
        &mut self,
        sample: &ThermalTuningSample,
        expected_target: i16,
    ) {
        if self.first_hold_temperature_centi_c.is_none() {
            return;
        }
        self.hold_error_sum_centi_c = self.hold_error_sum_centi_c.saturating_add(
            i64::from(expected_target) * 100 - i64::from(sample.temperature_centi_c),
        );
        // Candidate powers and the external analyzer use the PID logical
        // request. PPS voltage compensation must not alter that domain.
        let output = usize::from(sample.heater_requested_output_permille).min(1_000);
        self.hold_entry_output_permille.get_or_insert(output as u16);
        if output > 0 {
            self.hold_entry_pulse_only &= output <= 100
                && sample.phase == ThermalTuningPhase::Hold
                && self
                    .first_controller_hold_elapsed_ms
                    .is_some_and(|entry| sample.elapsed_ms.saturating_sub(entry) <= 250);
        }
        self.hold_output_histogram[output] = self.hold_output_histogram[output].saturating_add(1);
        self.hold_output_count = self.hold_output_count.saturating_add(1);
        self.min_controller_hold_temperature_centi_c = Some(
            self.min_controller_hold_temperature_centi_c
                .map_or(sample.temperature_centi_c, |value| {
                    value.min(sample.temperature_centi_c)
                }),
        );
        self.max_controller_hold_temperature_centi_c = Some(
            self.max_controller_hold_temperature_centi_c
                .map_or(sample.temperature_centi_c, |value| {
                    value.max(sample.temperature_centi_c)
                }),
        );
    }

    fn update_strategy_evidence(&mut self, sample: &ThermalTuningSample, target_c: i16) {
        let target_centi_c = i32::from(target_c) * 100;
        let error_centi_c = target_centi_c - i32::from(sample.temperature_centi_c);
        if sample.phase == ThermalTuningPhase::Approach
            || (sample.phase == ThermalTuningPhase::Hold && self.saw_approach)
        {
            self.hold_threshold_crossed |= hold_entry_ready(error_centi_c as f32 / 100.0);
        }
        if self.first_hold_temperature_centi_c.is_none()
            && sample.phase != ThermalTuningPhase::Hold
            && (0..=800).contains(&error_centi_c)
        {
            self.record_near_target_evidence(sample);
        }
        self.previous_sample_at_ms = Some(sample.elapsed_ms);
        self.previous_sample_temperature_centi_c = Some(sample.temperature_centi_c);
    }

    fn record_near_target_evidence(&mut self, sample: &ThermalTuningSample) {
        let output = usize::from(sample.heater_requested_output_permille).min(1_000);
        if self.approach_output_histogram[output] == u16::MAX {
            self.near_target_evidence_overflow = true;
        } else {
            self.approach_output_histogram[output] += 1;
            self.approach_output_count = self.approach_output_count.saturating_add(1);
        }
        let (Some(previous_ms), Some(previous_temp)) = (
            self.previous_sample_at_ms,
            self.previous_sample_temperature_centi_c,
        ) else {
            return;
        };
        let delta_ms = sample.elapsed_ms.saturating_sub(previous_ms);
        if delta_ms == 0 {
            return;
        }
        let slope = f64::from(i32::from(sample.temperature_centi_c) - i32::from(previous_temp))
            * 10.0
            / delta_ms as f64;
        if slope > 0.05 && self.approach_slopes_c_per_s.push(slope).is_err() {
            self.near_target_evidence_overflow = true;
        }
    }

    fn strategy_curve_class(&self, target_c: i16) -> CurveClass {
        if self.approach_curve_overflow {
            return CurveClass::Insufficient;
        }
        let Some(first) = self.approach_curve_samples.first() else {
            return CurveClass::Insufficient;
        };
        let Some(last) = self.approach_curve_samples.last() else {
            return CurveClass::Insufficient;
        };
        let raw_duration = self
            .approach_curve_last_elapsed_ms
            .unwrap_or(last.elapsed_ms)
            .saturating_sub(first.elapsed_ms);
        let target_delta =
            f64::from(i32::from(target_c) * 100 - i32::from(first.temperature_centi_c)) / 100.0;
        if raw_duration == 0 || target_delta <= 0.0 {
            return CurveClass::Insufficient;
        }
        let fitted_ms = raw_duration.clamp(5_000, 10_000);
        let mut max_above = 0.0_f64;
        let mut max_below = 0.0_f64;
        let mut previous_sign = 0_i8;
        let mut sign_changes = 0_u16;
        for sample in &self.approach_curve_samples {
            let normalized = (sample.elapsed_ms.saturating_sub(first.elapsed_ms) as f64
                / fitted_ms as f64)
                .clamp(0.0, 1.0);
            let progress = 1.0 - (1.0 - normalized) * (1.0 - normalized);
            let reference = f64::from(first.temperature_centi_c) / 100.0 + target_delta * progress;
            let deviation = f64::from(sample.temperature_centi_c) / 100.0 - reference;
            max_above = max_above.max(deviation.max(0.0));
            max_below = max_below.max((-deviation).max(0.0));
            let sign = if deviation > 0.5 {
                1
            } else if deviation < -0.5 {
                -1
            } else {
                0
            };
            if sign != 0 && previous_sign != 0 && sign != previous_sign {
                sign_changes += 1;
            }
            if sign != 0 {
                previous_sign = sign;
            }
        }
        max_above = max_above.max(self.approach_tail_max_above_c);
        max_below = max_below.max(self.approach_tail_max_below_c);
        if self.approach_tail_first_sign != 0 {
            if previous_sign != 0 && previous_sign != self.approach_tail_first_sign {
                sign_changes += 1;
            }
            sign_changes += self.approach_tail_sign_changes;
        }
        if max_above >= 1.0 && max_above > max_below + 0.4 {
            CurveClass::BrakeLate
        } else if max_below >= 1.0 && max_below > max_above + 0.4 {
            CurveClass::Underpowered
        } else if sign_changes >= 2 && max_above >= 0.5 && max_below >= 0.5 {
            CurveClass::Oscillatory
        } else {
            CurveClass::OnCurve
        }
    }

    fn update_warmup_output_evidence(&mut self, sample: &ThermalTuningSample) {
        if sample.phase != ThermalTuningPhase::Warmup {
            return;
        }
        let logical_full = sample.heater_requested_output_permille >= 995;
        let physical_full = sample.heater_output_permille >= 990;
        if !self.warmup_full_output {
            self.warmup_full_output = logical_full && physical_full;
            return;
        }
        if !logical_full || sample.heater_output_permille < 950 {
            self.warmup_full_output_invalid = true;
            return;
        }
        if physical_full {
            self.warmup_physical_transient_started_at_ms = None;
            return;
        }
        let started = self
            .warmup_physical_transient_started_at_ms
            .get_or_insert(sample.elapsed_ms);
        self.warmup_full_output_invalid |= sample.elapsed_ms.saturating_sub(*started) > 2_000;
    }

    fn record_approach_curve_sample(&mut self, sample: &ThermalTuningSample) {
        let first_elapsed_ms = self
            .approach_curve_samples
            .first()
            .map_or(sample.elapsed_ms, |first| first.elapsed_ms);
        self.approach_curve_last_elapsed_ms = Some(sample.elapsed_ms);
        if sample.elapsed_ms.saturating_sub(first_elapsed_ms) > 10_000 {
            let Some(first) = self.approach_curve_samples.first() else {
                return;
            };
            let raw_duration_ms = sample.elapsed_ms.saturating_sub(first.elapsed_ms);
            let fitted_ms = raw_duration_ms.clamp(5_000, 10_000) as f64;
            let start_c = f64::from(first.temperature_centi_c) / 100.0;
            let target_c = f64::from(sample.target_c);
            let normalized = (sample.elapsed_ms.saturating_sub(first.elapsed_ms) as f64
                / fitted_ms)
                .clamp(0.0, 1.0);
            let progress = 1.0 - (1.0 - normalized) * (1.0 - normalized);
            let reference_c = start_c + (target_c - start_c) * progress;
            let deviation_c = f64::from(sample.temperature_centi_c) / 100.0 - reference_c;
            self.approach_tail_abs_error_sum_c += deviation_c.abs();
            self.approach_tail_max_above_c =
                self.approach_tail_max_above_c.max(deviation_c.max(0.0));
            self.approach_tail_max_below_c =
                self.approach_tail_max_below_c.max((-deviation_c).max(0.0));
            let sign = if deviation_c > 0.5 {
                1
            } else if deviation_c < -0.5 {
                -1
            } else {
                0
            };
            if sign != 0 && self.approach_tail_first_sign == 0 {
                self.approach_tail_first_sign = sign;
            } else if sign != 0
                && self.approach_tail_last_sign != 0
                && self.approach_tail_last_sign != sign
            {
                self.approach_tail_sign_changes = self.approach_tail_sign_changes.saturating_add(1);
            }
            if sign != 0 {
                self.approach_tail_last_sign = sign;
            }
            self.approach_tail_count = self.approach_tail_count.saturating_add(1);
        } else if self
            .approach_curve_samples
            .push(ThermalCurveSample {
                elapsed_ms: sample.elapsed_ms,
                temperature_centi_c: sample.temperature_centi_c,
            })
            .is_err()
        {
            self.approach_curve_overflow = true;
        }
    }

    fn update_stable_window(&mut self, sample: &ThermalTuningSample, expected_target: i16) {
        let Some(warmup_exit) = self.warmup_exited_at_ms else {
            return;
        };
        let distance =
            (i32::from(sample.temperature_centi_c) - i32::from(expected_target) * 100).abs();
        let gap = self
            .stable_window_last_sample_ms
            .is_some_and(|last| sample.elapsed_ms.saturating_sub(last) > 1_000);
        let new_timestamp = self.stable_window_last_sample_ms != Some(sample.elapsed_ms);
        self.stable_window_last_sample_ms = Some(sample.elapsed_ms);
        if gap && self.stable_window_verified_at_ms.is_none() {
            self.stable_window_started_at_ms = None;
            self.stable_window_sample_count = 0;
        }
        if distance > i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C) {
            if self.stable_window_verified_at_ms.is_none() {
                self.stable_window_started_at_ms = None;
                self.stable_window_sample_count = 0;
            }
            return;
        }
        let stable_start = *self
            .stable_window_started_at_ms
            .get_or_insert(sample.elapsed_ms);
        if new_timestamp {
            self.stable_window_sample_count = self.stable_window_sample_count.saturating_add(1);
        }
        let window_duration = sample.elapsed_ms.saturating_sub(stable_start);
        let sampling_sufficient = u64::from(self.stable_window_sample_count.saturating_sub(1))
            .saturating_mul(1_000)
            >= window_duration.saturating_mul(3);
        if sampling_sufficient {
            self.longest_stable_span_ms = self.longest_stable_span_ms.max(window_duration);
        }
        let full_speed_limit_ms = u64::from(self.active_settle_limit_ms(expected_target));
        if stable_start.saturating_sub(warmup_exit) <= full_speed_limit_ms
            && sample.elapsed_ms.saturating_sub(stable_start) >= THERMAL_TUNING_STABLE_WINDOW_MS
            && sampling_sufficient
        {
            // Keep the first complete witness. Later Hold samples may leave
            // the absolute band; they belong to the separate hold p2p gate.
            self.stable_window_verified_at_ms
                .get_or_insert(sample.elapsed_ms);
        }
    }

    fn update_stability_evidence(&mut self, sample: &ThermalTuningSample, target_c: i16) {
        let Some(warmup_exit) = self.warmup_exited_at_ms else {
            return;
        };
        if sample.elapsed_ms < warmup_exit {
            return;
        }
        let limit_ms = u64::from(self.active_settle_limit_ms(target_c));
        let deadline = warmup_exit.saturating_add(limit_ms);
        let distance = sample.elapsed_ms.abs_diff(deadline);
        if distance < self.deadline_nearest_distance_ms {
            self.deadline_nearest_distance_ms = distance;
            self.deadline_nearest_temperature_centi_c = Some(sample.temperature_centi_c);
        }
        let lower = i32::from(target_c) * 100 - i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C);
        let upper = i32::from(target_c) * 100 + i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C);
        let temperature = i32::from(sample.temperature_centi_c);
        if self.first_band_elapsed_ms.is_none() {
            if sample.elapsed_ms <= deadline && (lower..=upper).contains(&temperature) {
                self.first_band_elapsed_ms = Some(sample.elapsed_ms);
            }
        } else if self.first_band_exit_centi_c.is_none() && !(lower..=upper).contains(&temperature)
        {
            self.first_band_exit_centi_c = Some(sample.temperature_centi_c);
        }
        if self.first_band_elapsed_ms.is_some() {
            self.max_band_excursion_centi_c = self.max_band_excursion_centi_c.max(
                (lower - temperature)
                    .max(temperature - upper)
                    .max(0)
                    .min(i32::from(u16::MAX)) as u16,
            );
        }
    }

    fn hold_output_percentile(&self, percentile: f64) -> Option<f64> {
        if self.hold_output_count == 0 {
            return None;
        }
        let rank = ((f64::from(self.hold_output_count - 1) * percentile) + 0.5) as u32;
        let mut seen = 0_u32;
        for (percent, count) in self.hold_output_histogram.iter().enumerate() {
            seen = seen.saturating_add(u32::from(*count));
            if seen > rank {
                return Some(percent as f64);
            }
        }
        None
    }

    fn approach_curve_mean_abs_error_c(&self, target_c: i16) -> Option<f64> {
        if self.approach_curve_overflow {
            return None;
        }
        let first = self.approach_curve_samples.first()?;
        let duration = self
            .approach_curve_last_elapsed_ms?
            .saturating_sub(first.elapsed_ms);
        let start_c = f64::from(first.temperature_centi_c) / 100.0;
        let target_c = f64::from(target_c);
        if duration == 0 || target_c <= start_c {
            return None;
        }
        let fitted_ms = duration.clamp(5_000, 10_000) as f64;
        let total_error = self.approach_tail_abs_error_sum_c
            + self
                .approach_curve_samples
                .iter()
                .map(|sample| {
                    let elapsed = sample.elapsed_ms.saturating_sub(first.elapsed_ms) as f64;
                    let progress = (elapsed / fitted_ms).clamp(0.0, 1.0);
                    let remaining = 1.0 - progress;
                    let reference = start_c + (target_c - start_c) * (1.0 - remaining * remaining);
                    (f64::from(sample.temperature_centi_c) / 100.0 - reference).abs()
                })
                .sum::<f64>();
        Some(
            total_error
                / (self.approach_curve_samples.len() as f64 + f64::from(self.approach_tail_count)),
        )
    }

    fn trial_score(
        &self,
        target_c: i16,
        completed: bool,
        overshoot_centi_c: Option<u16>,
        hold_p2p_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
    ) -> [f64; 11] {
        let completed = completed && self.stable_window_verified_at_ms.is_some();
        let missing = 1.0e12;
        let limit_ms = f64::from(self.active_settle_limit_ms(target_c));
        let margin_ms = if target_c > 150 { 500.0 } else { 1_000.0 };
        let overshoot_c = overshoot_centi_c.map(|value| f64::from(value) / 100.0);
        let hold_p2p_c = hold_p2p_centi_c.map(|value| f64::from(value) / 100.0);
        let settle_ms = settle_time_ms.map(|value| value as f64);
        let verified_settle_ms = self.stable_window_verified_at_ms.and(settle_ms);
        let metric_gate = completed
            && overshoot_c.is_some_and(|value| value <= 3.0)
            && hold_p2p_c.is_some_and(|value| value <= 3.0)
            && settle_ms.is_some_and(|value| value <= limit_ms);
        let gate_penalty = if metric_gate {
            0.0
        } else {
            1.0 + overshoot_c.map_or(3.0, |value| (value - 3.0).max(0.0))
                + hold_p2p_c.map_or(3.0, |value| (value - 3.0).max(0.0))
                + verified_settle_ms.map_or(2.0, |value| {
                    ((value - limit_ms).max(0.0) / 1_000.0).min(5.0)
                })
        };
        let low_margin = completed
            && settle_ms.is_some_and(|value| value <= limit_ms && limit_ms - value < margin_ms);
        let target_centi = i32::from(target_c) * 100;
        let lower = target_centi - i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C);
        let upper = target_centi + i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C);
        let (progress_base, temperature_gap_c) =
            if completed && settle_ms.is_some_and(|value| value <= limit_ms) {
                (if low_margin { 0.25 } else { 0.0 }, 0.0)
            } else if let Some(exit) = self.first_band_exit_centi_c {
                let value = i32::from(exit);
                (
                    1.0,
                    f64::from((lower - value).max(value - upper).max(0)) / 100.0,
                )
            } else if self.first_band_elapsed_ms.is_some() {
                (0.0, 0.0)
            } else if let Some(deadline_temp) = self.deadline_nearest_temperature_centi_c {
                let value = i32::from(deadline_temp);
                if value < lower || value > upper {
                    (
                        3.0,
                        f64::from((lower - value).max(value - upper).max(0)) / 100.0,
                    )
                } else {
                    (2.0, 0.0)
                }
            } else {
                (4.0, 0.0)
            };
        let first_band_penalty = self.first_band_elapsed_ms.map_or(0.0, |value| {
            (value.saturating_sub(self.trial_start_elapsed_ms.unwrap_or(0)) as f64 / 1_000_000.0)
                .min(0.5)
        });
        let hold_spread = self
            .hold_output_percentile(0.5)
            .zip(self.hold_output_percentile(0.9))
            .map_or(missing, |(median, p90)| (p90 - median).abs());
        let mut score = [
            if self.warmup_full_output && !self.warmup_full_output_invalid {
                0.0
            } else {
                1.0
            },
            if completed { 0.0 } else { 1.0 },
            gate_penalty,
            if low_margin { 1.0 } else { 0.0 },
            progress_base + first_band_penalty + temperature_gap_c.min(5.0),
            settle_ms.map_or(missing, |value| (value - (limit_ms - margin_ms)).max(0.0)),
            settle_ms.unwrap_or(missing),
            overshoot_c.unwrap_or(missing),
            hold_p2p_c.unwrap_or(missing),
            hold_spread,
            self.approach_curve_mean_abs_error_c(target_c)
                .unwrap_or(missing),
        ];
        if !completed {
            // First reduce the complete prefix's violation. A faster entry
            // cannot make a longer truncated, unstable span more feasible.
            score[4] = if self.first_band_elapsed_ms.is_some() {
                f64::from(self.max_band_excursion_centi_c) / 100.0
            } else {
                progress_base + temperature_gap_c.min(5.0)
            };
            score[5] = if self.first_band_elapsed_ms.is_some() {
                let ranked_span_ms = self.longest_stable_span_ms
                    / THERMAL_TUNING_LIVE_SAMPLE_INTERVAL_MS
                    * THERMAL_TUNING_LIVE_SAMPLE_INTERVAL_MS;
                1.0 - (ranked_span_ms as f64 / THERMAL_TUNING_STABLE_WINDOW_MS as f64).min(1.0)
            } else {
                1.0
            };
            score[6] = overshoot_c.unwrap_or(missing);
            score[7] = hold_p2p_c.unwrap_or(missing);
            score[8] = settle_ms.unwrap_or(missing);
        }
        score
    }

    pub fn advance_target(&mut self) -> Result<(), ThermalTuningError> {
        if !self.armed {
            return Err(ThermalTuningError::NotRunning);
        }
        while self.target_index + 1 < self.targets_c.len() {
            self.target_index += 1;
            self.proposed_points.clear();
            self.observed_hold_failures.clear();
            self.observed_damping_failures.clear();
            self.trial_index = 0;
            self.trial_purpose = ThermalTuningTrialPurpose::Scout;
            self.batch_current_profile = None;
            self.batch_predicted_profile = None;
            self.batch_current_score = None;
            self.batch_current_confirmable = false;
            self.batch_current_failure_evidence = None;
            self.target_started_elapsed_ms = self.current_elapsed_ms;
            let target = self.targets_c[self.target_index];
            if self
                .required_accepted_bounds(target)
                .is_some_and(|(lower, upper)| {
                    self.accepted_target_mask & target_bit(lower) == 0
                        || self.accepted_target_mask & target_bit(upper) == 0
                })
            {
                self.failed_target_mask |= target_bit(target);
                self.push_event(
                    ThermalTuningEventKind::Decision,
                    Some(target),
                    None,
                    None,
                    Some("interval_pruned_without_accepted_bounds"),
                );
                continue;
            }
            if let Some((lower, upper)) = self.required_accepted_bounds(target) {
                self.seed_target_from_bounds(target, lower, upper)?;
            }
            self.phase = ThermalTuningPhase::Cooldown;
            self.reset_trial_metrics();
            self.push_phase_transition_event("cooling before next target");
            return Ok(());
        }
        self.complete_target_scope();
        Ok(())
    }

    fn required_accepted_bounds(&self, target: i16) -> Option<(i16, i16)> {
        if self.targets_c.as_slice() != THERMAL_TUNING_EXECUTION_ORDER_C {
            return None;
        }
        match target {
            120 => Some((60, 220)),
            80 => Some((60, 120)),
            100 => Some((80, 120)),
            160 => Some((120, 220)),
            140 => Some((120, 160)),
            180 => Some((160, 220)),
            _ => None,
        }
    }

    fn seed_target_from_bounds(
        &mut self,
        target: i16,
        lower: i16,
        upper: i16,
    ) -> Result<(), ThermalTuningError> {
        let profile = self
            .candidate_profile
            .as_mut()
            .ok_or(ThermalTuningError::CandidateProfileRequired)?;
        let point_at = |target_c| {
            profile
                .points
                .iter()
                .flatten()
                .find(|point| point.target_temp_c == target_c)
                .copied()
        };
        let lower_point = point_at(lower).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        let upper_point = point_at(upper).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        let seeded = interpolate_candidate_point(target, lower_point, upper_point);
        let point = profile
            .points
            .iter_mut()
            .flatten()
            .find(|point| point.target_temp_c == target)
            .ok_or(ThermalTuningError::CandidateProfileRequired)?;
        *point = seeded;
        profile.sanitize();
        self.candidate_hash = hash_profile(profile);
        self.preview_update_pending = true;
        self.preview_receipt = None;
        self.seed_source = bounded_reason("accepted_bounds_linear");
        Ok(())
    }

    fn complete_target_scope(&mut self) {
        self.armed = false;
        self.review_disposition = ThermalTuningReviewDisposition::Complete;
        if self.failed_target_mask != 0 {
            self.phase = ThermalTuningPhase::Failed;
            self.terminal_disposition = ThermalTuningTerminalDisposition::Failed;
        } else {
            self.sealed = false;
            self.phase = ThermalTuningPhase::Completed;
            self.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        }
    }

    pub fn seal(&mut self) -> Result<(), ThermalTuningError> {
        if self.terminal_disposition != ThermalTuningTerminalDisposition::CandidateReady {
            return Err(ThermalTuningError::InvalidTransition);
        }
        if self.sealed {
            return Ok(());
        }
        if self.last_acknowledged_sequence != self.last_sequence
            || self.preview_update_pending
            || self.preview_receipt.is_none()
        {
            return Err(ThermalTuningError::InvalidTransition);
        }
        self.sealed = true;
        self.phase = ThermalTuningPhase::Sealed;
        self.push_phase_transition_event("candidate sealed for explicit save confirmation");
        Ok(())
    }

    pub fn cancel(&mut self, reason: &str) -> Result<(), ThermalTuningError> {
        if !self.armed
            && !self.sealed
            && self.terminal_disposition != ThermalTuningTerminalDisposition::CandidateReady
        {
            return Err(ThermalTuningError::NotRunning);
        }
        self.armed = false;
        self.sealed = false;
        self.phase = ThermalTuningPhase::Canceled;
        self.terminal_disposition = ThermalTuningTerminalDisposition::Canceled;
        self.review_disposition = ThermalTuningReviewDisposition::Complete;
        self.push_event(
            ThermalTuningEventKind::Safety,
            None,
            None,
            None,
            Some(reason),
        );
        Ok(())
    }

    pub fn confirm_save(
        &mut self,
        confirmation: ThermalTuningSaveConfirmation<'_>,
    ) -> Result<ThermalTuningSaveReceipt, ThermalTuningError> {
        let ThermalTuningSaveConfirmation {
            run_id,
            candidate_id,
            candidate_hash,
            power_class,
            readback_verified,
            bank_revision,
            bank_hash,
        } = confirmation;
        self.validate_save_identity(run_id, candidate_id, candidate_hash, power_class)?;
        if !readback_verified || bank_hash.as_str() != candidate_hash {
            self.fail("candidate_readback_not_verified");
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
            return Err(ThermalTuningError::IdentityMismatch);
        }
        self.save_confirmed = true;
        self.phase = ThermalTuningPhase::Completed;
        self.terminal_disposition = ThermalTuningTerminalDisposition::Passed;
        self.push_event(ThermalTuningEventKind::Decision, None, None, None, None);
        let receipt = ThermalTuningSaveReceipt {
            run_id: self.run_id.clone().unwrap_or_default(),
            candidate_id: self.candidate_id.clone().unwrap_or_default(),
            candidate_hash: self.candidate_hash.clone().unwrap_or_default(),
            power_class,
            bank: power_class,
            bank_revision,
            bank_hash,
            atomic: true,
            readback_verified,
        };
        self.save_receipt = Some(receipt.clone());
        Ok(receipt)
    }

    pub fn validate_save_identity(
        &mut self,
        run_id: &str,
        candidate_id: &str,
        candidate_hash: &str,
        power_class: ThermalTuningPowerClass,
    ) -> Result<(), ThermalTuningError> {
        if !self.sealed {
            return Err(ThermalTuningError::NotSealed);
        }
        if self.last_acknowledged_sequence != self.last_sequence {
            self.fail("event_ack_incomplete");
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
            return Err(ThermalTuningError::InvalidTransition);
        }
        if self.targets_c.as_slice() != THERMAL_TUNING_EXECUTION_ORDER_C {
            self.fail("non_qualification_scope");
            return Err(ThermalTuningError::InvalidTargets);
        }
        if self.run_id.as_deref() != Some(run_id)
            || self.candidate_id.as_deref() != Some(candidate_id)
            || self.candidate_hash.as_deref() != Some(candidate_hash)
            || self.power_class != Some(power_class)
            || self
                .candidate_profile
                .as_ref()
                .and_then(hash_profile)
                .as_deref()
                != Some(candidate_hash)
        {
            self.fail("candidate_identity_mismatch");
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
            return Err(ThermalTuningError::IdentityMismatch);
        }
        if self.preview_receipt.as_ref().is_none_or(|receipt| {
            receipt.run_id.as_str() != run_id
                || receipt.candidate_id.as_str() != candidate_id
                || receipt.candidate_hash.as_str() != candidate_hash
                || receipt.power_class != power_class
                || receipt.ram_applied_hash.as_str() != candidate_hash
        }) {
            self.fail("candidate_preview_not_verified");
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
            return Err(ThermalTuningError::IdentityMismatch);
        }
        Ok(())
    }

    pub fn record_preview_applied(&mut self) -> Result<(), ThermalTuningError> {
        let applied_hash = self.candidate_profile.as_ref().and_then(|profile| {
            let mut effective = *profile;
            effective.sanitize();
            hash_profile(&effective)
        });
        if !self.armed || applied_hash.as_ref() != self.candidate_hash.as_ref() {
            self.fail("candidate_preview_hash_mismatch");
            return Err(ThermalTuningError::IdentityMismatch);
        }
        self.preview_receipt = Some(ThermalTuningPreviewReceipt {
            run_id: self.run_id.clone().unwrap_or_default(),
            candidate_id: self.candidate_id.clone().unwrap_or_default(),
            candidate_hash: self.candidate_hash.clone().unwrap_or_default(),
            power_class: self.power_class.unwrap_or(ThermalTuningPowerClass::Pps5a),
            ram_applied_hash: applied_hash.unwrap_or_default(),
        });
        self.preview_update_pending = false;
        Ok(())
    }

    pub fn events_after(&mut self, after_sequence: Option<u32>) -> ThermalTuningEventPage {
        let invalid_ack = after_sequence.is_some_and(|after| {
            self.last_delivered_sequence
                .is_none_or(|delivered| after > delivered)
        });
        if invalid_ack {
            self.fail("event_ack_skipped_unconfirmed");
            self.trace_gap = true;
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
        }
        let after_sequence = if invalid_ack { None } else { after_sequence };
        if let Some(acknowledged) = after_sequence {
            self.last_acknowledged_sequence = Some(
                self.last_acknowledged_sequence
                    .map_or(acknowledged, |previous| previous.max(acknowledged)),
            );
        }
        let first = self.events.first().map(|event| event.sequence);
        if after_sequence
            .is_some_and(|after| first.is_some_and(|sequence| after.saturating_add(1) < sequence))
        {
            self.trace_gap = true;
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
        }
        if let Some(after_sequence) = after_sequence {
            while self
                .events
                .first()
                .is_some_and(|event| event.sequence <= after_sequence)
            {
                let event = self.events.remove(0);
                self.retained_trace_digest =
                    chained_event_digest(&self.retained_trace_digest, &event);
            }
        }
        let mut events = Vec::new();
        for event in self.events.iter() {
            if after_sequence.is_none_or(|after| event.sequence > after)
                && events.push(event.clone()).is_err()
            {
                break;
            }
        }
        let next_sequence = events.last().map(|event| event.sequence);
        if let Some(sequence) = next_sequence {
            self.last_delivered_sequence = Some(
                self.last_delivered_sequence
                    .map_or(sequence, |delivered| delivered.max(sequence)),
            );
        }
        ThermalTuningEventPage {
            events,
            next_sequence,
            trace: self.trace(),
        }
    }

    pub fn trace(&self) -> ThermalTuningTrace {
        ThermalTuningTrace {
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            complete: !self.armed && !self.trace_gap,
            digest: self.trace_digest.clone(),
            gap: self.trace_gap,
        }
    }

    pub fn fail(&mut self, reason: &str) {
        self.armed = false;
        self.sealed = false;
        self.phase = ThermalTuningPhase::Failed;
        self.terminal_disposition = ThermalTuningTerminalDisposition::Failed;
        self.review_disposition = ThermalTuningReviewDisposition::Complete;
        self.push_event(
            ThermalTuningEventKind::Safety,
            None,
            None,
            None,
            Some(reason),
        );
    }

    /// Retain caller facts before the canonical failure without changing its meaning.
    pub fn fail_with_context(&mut self, reason: &str, context: &[&str]) {
        for fact in context {
            self.push_event(
                ThermalTuningEventKind::Safety,
                self.targets_c.get(self.target_index).copied(),
                Some(self.trial_index),
                None,
                Some(fact),
            );
        }
        self.fail(reason);
        if self.trace_gap {
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
        }
    }

    fn reset_for_run(&mut self) {
        self.phase = ThermalTuningPhase::Idle;
        self.terminal_disposition = ThermalTuningTerminalDisposition::Running;
        self.review_disposition = ThermalTuningReviewDisposition::Pending;
        self.power_class = None;
        self.run_id = None;
        self.candidate_id = None;
        self.candidate_hash = None;
        self.targets_c.clear();
        self.target_index = 0;
        self.trial_index = 0;
        self.trial_purpose = ThermalTuningTrialPurpose::Scout;
        self.target_started_elapsed_ms = 0;
        self.accepted_target_mask = 0;
        self.failed_target_mask = 0;
        self.armed = false;
        self.sealed = false;
        self.save_confirmed = false;
        self.preview_receipt = None;
        self.save_receipt = None;
        self.candidate_profile = None;
        self.observed_hold_failures.clear();
        self.observed_damping_failures.clear();
        self.batch_current_profile = None;
        self.batch_predicted_profile = None;
        self.batch_current_score = None;
        self.batch_current_confirmable = false;
        self.batch_current_failure_evidence = None;
        self.seed_source.clear();
        self.preview_update_pending = false;
        self.trace_gap = false;
        self.events.clear();
        self.trace_digest.clear();
        self.retained_trace_digest.clear();
        self.first_sequence = None;
        self.last_sequence = None;
        self.last_delivered_sequence = None;
        self.last_acknowledged_sequence = None;
        self.next_sequence = 0;
        self.current_elapsed_ms = 0;
        self.sample_clock_origin_ms = None;
        self.last_live_sample_elapsed_ms = None;
    }

    fn push_phase_transition_event(&mut self, reason: &str) {
        self.push_event(
            ThermalTuningEventKind::PhaseTransition,
            self.targets_c.get(self.target_index).copied(),
            Some(self.trial_index),
            None,
            Some(reason),
        );
    }

    fn push_candidate_trial_event(&mut self, trial_index: u8) {
        let target_c = self.targets_c.get(self.target_index).copied();
        self.trial_timing = target_c.and_then(|target| {
            self.candidate_profile
                .as_ref()?
                .points
                .iter()
                .flatten()
                .find(|point| point.target_temp_c == target)
                .copied()
                .map(ThermalTuningTimingEvidence::for_model_point)
        });
        let record = ThermalTuningTrialRecord {
            candidate: self.candidate_snapshot(target_c),
            trial_index,
            purpose: self.trial_purpose,
            start_sequence: self.next_sequence,
            start_elapsed_ms: self.current_elapsed_ms,
            start_temperature_centi_c: self.trial_start_temperature_centi_c,
            cooldown_threshold_centi_c: target_c.map(|target| cooldown_threshold_c(target) * 100),
            end_sequence: None,
            end_elapsed_ms: None,
            sample_start_sequence: None,
            sample_end_sequence: None,
            timing: self.trial_timing,
        };
        self.push_event(
            ThermalTuningEventKind::CandidateTrial,
            target_c,
            Some(trial_index),
            None,
            Some("candidate trial materialized from device preview"),
        );
        if let Some(event) = self
            .events
            .last_mut()
            .filter(|event| event.sequence == self.next_sequence.saturating_sub(1))
        {
            event.trial = Some(record);
            self.recompute_trace_digest();
        }
    }

    fn push_closed_trial_event(&mut self, elapsed_ms: u64) {
        let target_c = self.targets_c.get(self.target_index).copied();
        let record = ThermalTuningTrialRecord {
            candidate: self.candidate_snapshot(target_c),
            trial_index: self.trial_index,
            purpose: self.trial_purpose,
            start_sequence: self.trial_start_sequence.unwrap_or(self.next_sequence),
            start_elapsed_ms: self.trial_start_elapsed_ms.unwrap_or(elapsed_ms),
            start_temperature_centi_c: self.trial_start_temperature_centi_c,
            cooldown_threshold_centi_c: target_c.map(|target| cooldown_threshold_c(target) * 100),
            end_sequence: Some(self.next_sequence),
            end_elapsed_ms: Some(elapsed_ms),
            sample_start_sequence: self.trial_sample_start_sequence,
            sample_end_sequence: self.trial_sample_end_sequence,
            timing: self.trial_timing,
        };
        self.push_event(
            ThermalTuningEventKind::CandidateTrial,
            target_c,
            Some(self.trial_index),
            None,
            Some("candidate trial closed"),
        );
        if let Some(event) = self
            .events
            .last_mut()
            .filter(|event| event.sequence == self.next_sequence.saturating_sub(1))
        {
            event.trial = Some(record);
            self.recompute_trace_digest();
        }
    }

    fn candidate_snapshot(&self, target_c: Option<i16>) -> ThermalTuningCandidateSnapshot {
        ThermalTuningCandidateSnapshot {
            candidate_id: self.candidate_id.clone().unwrap_or_default(),
            candidate_hash: self.candidate_hash.clone().unwrap_or_default(),
            power_class: self.power_class.unwrap_or(ThermalTuningPowerClass::Pps5a),
            point: target_c.and_then(|target| {
                self.candidate_profile
                    .as_ref()
                    .and_then(|profile| candidate_point(profile, target))
            }),
            seed_source: self.seed_source.clone(),
            lower_target_c: target_c
                .and_then(|target| self.required_accepted_bounds(target))
                .map(|(lower, _)| lower),
            upper_target_c: target_c
                .and_then(|target| self.required_accepted_bounds(target))
                .map(|(_, upper)| upper),
        }
    }

    fn push_decision_event(&mut self, decision: ThermalTuningDecisionRecord) {
        let target_c = self.targets_c.get(self.target_index).copied();
        self.push_event(
            ThermalTuningEventKind::Decision,
            target_c,
            Some(self.trial_index),
            None,
            Some(decision.stop_reason.as_str()),
        );
        if let Some(event) = self
            .events
            .last_mut()
            .filter(|event| event.sequence == self.next_sequence.saturating_sub(1))
        {
            event.decision = Some(decision);
            self.recompute_trace_digest();
        }
    }

    fn push_event(
        &mut self,
        kind: ThermalTuningEventKind,
        target_c: Option<i16>,
        trial_index: Option<u8>,
        sample: Option<ThermalTuningSample>,
        reason: Option<&str>,
    ) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        let reason = reason.map(bounded_reason);
        let event = ThermalTuningEvent {
            sequence,
            elapsed_ms: self.current_elapsed_ms,
            kind,
            phase: self.phase,
            target_c,
            trial_index,
            sample,
            reason,
            trial: None,
            decision: None,
        };
        if self.first_sequence.is_none() {
            self.first_sequence = Some(sequence);
        }
        self.last_sequence = Some(sequence);
        if self.events.push(event).is_err() {
            self.trace_gap = true;
            self.review_disposition = ThermalTuningReviewDisposition::ReviewIncomplete;
            self.armed = false;
            self.phase = ThermalTuningPhase::Failed;
            self.terminal_disposition = ThermalTuningTerminalDisposition::Failed;
        } else if let Some(event) = self.events.last() {
            self.trace_digest = chained_event_digest(&self.trace_digest, event);
        }
    }

    fn recompute_trace_digest(&mut self) {
        let mut digest = self.retained_trace_digest.clone();
        for event in &self.events {
            digest = chained_event_digest(&digest, event);
        }
        self.trace_digest = digest;
    }
}

fn bounded_reason(value: &str) -> TuningReason {
    let mut reason = TuningReason::new();
    let _ = reason.push_str(value);
    reason
}

fn phase_transition_reason(phase: ThermalTuningPhase) -> &'static str {
    match phase {
        ThermalTuningPhase::Warmup => "entered warmup phase",
        ThermalTuningPhase::Approach => "entered approach phase",
        ThermalTuningPhase::Hold => "entered hold phase",
        ThermalTuningPhase::Cooldown => "entered cooldown phase",
        ThermalTuningPhase::Sealed => "candidate sealed",
        ThermalTuningPhase::Completed => "candidate saved",
        ThermalTuningPhase::Failed => "tuning failed",
        ThermalTuningPhase::Canceled => "tuning canceled",
        ThermalTuningPhase::Idle => "returned to idle",
    }
}

fn cooldown_threshold_c(target_c: i16) -> i16 {
    if target_c < 80 {
        35
    } else if target_c > 200 {
        target_c - 20
    } else if target_c > 160 {
        target_c - 30
    } else {
        target_c - 40
    }
}

fn histogram_percentile(histogram: &[u16; 1001], count: u32, percentile: f64) -> Option<u16> {
    if count == 0 {
        return None;
    }
    let rank = ((f64::from(count - 1) * percentile.clamp(0.0, 1.0)) + 0.5) as u32;
    let mut seen = 0_u32;
    for (output, occurrences) in histogram.iter().enumerate() {
        seen = seen.saturating_add(u32::from(*occurrences));
        if seen > rank {
            return Some(output as u16);
        }
    }
    None
}

fn target_bit(target_c: i16) -> u16 {
    THERMAL_TUNING_TARGETS_C
        .iter()
        .position(|target| *target == target_c)
        .map_or(0, |index| 1 << index)
}

fn interpolate_candidate_point(
    target_c: i16,
    lower: ThermalControlProfilePointConfig,
    upper: ThermalControlProfilePointConfig,
) -> ThermalControlProfilePointConfig {
    if lower.target_temp_c >= upper.target_temp_c {
        return ThermalControlProfilePointConfig {
            target_temp_c: target_c,
            ..lower
        };
    }
    let numerator = f32::from(target_c - lower.target_temp_c);
    let denominator = f32::from(upper.target_temp_c - lower.target_temp_c);
    let ratio = numerator / denominator;
    let interpolate = |left: u16, right: u16, ceiling: u16| {
        (f32::from(left) + (f32::from(right) - f32::from(left)) * ratio + 0.5)
            .clamp(0.0, f32::from(ceiling)) as u16
    };
    let mut seeded = lower;
    seeded.target_temp_c = target_c;
    seeded.warmup_power_permille = 1_000;
    macro_rules! field {
        ($name:ident, $ceiling:expr) => {
            seeded.$name = interpolate(lower.$name, upper.$name, $ceiling);
        };
    }
    field!(brake_distance_centi_c, 5_000);
    field!(approach_power_permille, 1_000);
    field!(approach_floor_power_permille, 1_000);
    field!(approach_damping_exponent_permille, 4_000);
    field!(approach_tail_window_centi_c, 5_000);
    field!(hold_power_permille, 1_000);
    field!(hold_reheat_power_permille, 1_000);
    field!(warmup_reenter_centi_c, 5_000);
    field!(hold_entry_centi_c, 5_000);
    field!(hold_exit_centi_c, 5_000);
    field!(hold_on_centi_c, 5_000);
    field!(hold_off_centi_c, 5_000);
    field!(overshoot_cutoff_centi_c, 5_000);
    field!(hold_kp_permille_per_c, 10_000);
    field!(hold_ki_permille_per_c_tick, 10_000);
    seeded.hold_ki_permille_per_c_tick = seeded.hold_ki_permille_per_c_tick.max(1);
    field!(hold_blend_ticks, 255);
    seeded.hold_blend_ticks = seeded.hold_blend_ticks.max(1);
    field!(approach_lead_ticks, 255);
    field!(hold_lead_ticks, 255);
    seeded
}

fn candidate_point(
    profile: &ThermalControlProfileConfig,
    target_c: i16,
) -> Option<ThermalTuningCandidatePoint> {
    let point = profile
        .points
        .iter()
        .flatten()
        .find(|point| point.target_temp_c == target_c)?;
    Some(ThermalTuningCandidatePoint {
        target_c: point.target_temp_c,
        warmup_power_permille: point.warmup_power_permille,
        warmup_reenter_centi_c: point.warmup_reenter_centi_c,
        brake_distance_centi_c: point.brake_distance_centi_c,
        approach_power_permille: point.approach_power_permille,
        approach_floor_power_permille: point.approach_floor_power_permille,
        approach_damping_exponent_permille: point.approach_damping_exponent_permille,
        approach_tail_window_centi_c: point.approach_tail_window_centi_c,
        hold_power_permille: point.hold_power_permille,
        hold_reheat_power_permille: point.hold_reheat_power_permille,
        hold_entry_centi_c: point.hold_entry_centi_c,
        hold_exit_centi_c: point.hold_exit_centi_c,
        hold_on_centi_c: point.hold_on_centi_c,
        hold_off_centi_c: point.hold_off_centi_c,
        overshoot_cutoff_centi_c: point.overshoot_cutoff_centi_c,
        hold_kp_permille_per_c: point.hold_kp_permille_per_c,
        hold_ki_permille_per_c_tick: point.hold_ki_permille_per_c_tick,
        hold_blend_ticks: point.hold_blend_ticks,
        approach_lead_ticks: point.approach_lead_ticks,
        hold_lead_ticks: point.hold_lead_ticks,
    })
}

fn same_effective_search_point(
    mut left: ThermalControlProfilePointConfig,
    mut right: ThermalControlProfilePointConfig,
) -> bool {
    // The calibrated 5A model ignores these legacy profile clamps. Changing
    // them alone must not consume another physical experiment.
    left.approach_floor_power_permille = 0;
    left.approach_power_permille = 0;
    right.approach_floor_power_permille = 0;
    right.approach_power_permille = 0;
    left == right
}

fn same_point_except_damping(
    mut left: ThermalControlProfilePointConfig,
    mut right: ThermalControlProfilePointConfig,
) -> bool {
    left.approach_damping_exponent_permille = 0;
    right.approach_damping_exponent_permille = 0;
    same_effective_search_point(left, right)
}

fn damping_failure_side(evidence: TuneEvidence) -> Option<bool> {
    if evidence.target_c > 150
        || !evidence.tunable_stop
        || !evidence.full_speed_failed
        || evidence.stable_window_verified
        || evidence.approach_floor_only
        || !evidence
            .approach_output_range
            .is_some_and(|(lo, hi)| lo >= 50 && hi <= 1_000)
    {
        return None;
    }
    if evidence.first_band_observed
        && evidence.hold_sample_count >= 3
        && evidence.hold_max_below_c.is_some_and(|below| below <= 1.5)
        && evidence.over_c > 1.5
    {
        Some(true)
    } else if unobserved_low_side_failure(evidence) {
        Some(false)
    } else {
        None
    }
}

fn failed_damping_comparison_needs_braking(
    current: Option<ThermalControlProfileConfig>,
    predicted: Option<ThermalControlProfileConfig>,
    current_evidence: Option<TuneEvidence>,
    predicted_evidence: Option<TuneEvidence>,
    target_c: i16,
) -> bool {
    let point = |profile: ThermalControlProfileConfig| {
        profile
            .points
            .into_iter()
            .flatten()
            .find(|point| point.target_temp_c == target_c)
    };
    let Some(((current, predicted), (current_facts, predicted_facts))) = current
        .and_then(point)
        .zip(predicted.and_then(point))
        .zip(current_evidence.zip(predicted_evidence))
    else {
        return false;
    };
    current_facts.target_c == target_c
        && predicted_facts.target_c == target_c
        && damping_failure_side(current_facts) == Some(true)
        && damping_failure_side(predicted_facts) == Some(true)
        && (current_facts.hold_off_sample_count == current_facts.hold_sample_count
            || (current_facts.hold_entry_pulse_only
                && current_facts.hold_off_sample_count.saturating_mul(10)
                    >= current_facts.hold_sample_count.saturating_mul(9)))
        && same_point_except_damping(current, predicted)
        && predicted.approach_damping_exponent_permille > current.approach_damping_exponent_permille
        && bounded_failure_alternative(current, current_facts, 1).is_some_and(|alternative| {
            alternative.brake_distance_centi_c > current.brake_distance_centi_c
        })
}

fn score_precedes(left: &[f64; 11], right: &[f64; 11]) -> bool {
    left.iter()
        .zip(right)
        .map(|(left, right)| left.partial_cmp(right).unwrap_or(Ordering::Equal))
        .find(|order| *order != Ordering::Equal)
        == Some(Ordering::Less)
}

pub fn hash_profile(profile: &ThermalControlProfileConfig) -> Option<TuningHash> {
    let mut hasher = Sha256::new();
    // Retain the wire hash encoding for historical evidence. This hashes only
    // profile content; run identity, power class and bank are checked separately.
    hasher.update(b"thermal-tuning-v2/candidate/pps5a");
    for value in [
        profile.settings.temp_filter_alpha_permille,
        profile.settings.warmup_reenter_centi_c,
        profile.settings.hold_entry_centi_c,
        profile.settings.hold_exit_centi_c,
        profile.settings.hold_on_centi_c,
        profile.settings.hold_off_centi_c,
        profile.settings.overshoot_cutoff_centi_c,
        profile.settings.approach_max_ticks,
        profile.settings.approach_min_power_ratio_permille,
        profile.settings.hold_kp_permille_per_c,
        profile.settings.hold_ki_permille_per_c_tick,
        profile.settings.hold_blend_ticks,
        profile.settings.hold_reheat_power_permille,
        profile.settings.approach_lead_ticks,
        profile.settings.hold_lead_ticks,
        profile.settings.auto_adjustable_working_floor_mv,
        profile.settings.heater_current_reserve_ma,
    ] {
        hasher.update(value.to_le_bytes());
    }
    for point in profile.points.iter().flatten() {
        for value in [
            point.target_temp_c as u16,
            point.brake_distance_centi_c,
            point.warmup_power_permille,
            point.warmup_reenter_centi_c,
            point.approach_power_permille,
            point.approach_floor_power_permille,
            point.approach_damping_exponent_permille,
            point.approach_tail_window_centi_c,
            point.hold_power_permille,
            point.hold_reheat_power_permille,
            point.hold_entry_centi_c,
            point.hold_exit_centi_c,
            point.hold_on_centi_c,
            point.hold_off_centi_c,
            point.overshoot_cutoff_centi_c,
            point.hold_kp_permille_per_c,
            point.hold_ki_permille_per_c_tick,
            point.hold_blend_ticks,
            point.approach_lead_ticks,
            point.hold_lead_ticks,
        ] {
            hasher.update(value.to_le_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut output = TuningHash::new();
    output.push_str("sha256:").ok()?;
    for byte in digest {
        write!(output, "{byte:02x}").ok()?;
    }
    Some(output)
}

const TRACE_DIGEST_DOMAIN: &[u8] = b"thermal-tuning-event-digest-v2";
const TRACE_SCORE_SCALE: f64 = 1_000_000.0;

fn quantized_score(value: f64) -> i64 {
    if value.is_nan() {
        return i64::MIN;
    }
    let scaled = value * TRACE_SCORE_SCALE;
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else if scaled <= i64::MIN as f64 {
        i64::MIN
    } else if scaled >= 0.0 {
        (scaled + 0.5) as i64
    } else {
        (scaled - 0.5) as i64
    }
}

fn chained_event_digest(previous: &TuningHash, event: &ThermalTuningEvent) -> TuningHash {
    let mut hasher = Sha256::new();
    hasher.update(previous.as_bytes());
    hasher.update(TRACE_DIGEST_DOMAIN);
    let mut canonical = event.clone();
    let decision_score = canonical.decision.as_mut().map(|decision| {
        let score = decision.score;
        decision.score = [0.0; 11];
        score
    });
    let mut encoded = [0_u8; 4096];
    if let Ok(length) = serde_json_core::to_slice(&canonical, &mut encoded) {
        hasher.update(&encoded[..length]);
    } else {
        hasher.update(event.sequence.to_le_bytes());
        hasher.update(event.elapsed_ms.to_le_bytes());
        hasher.update([event.kind as u8, event.phase as u8]);
        hasher.update(event.target_c.unwrap_or_default().to_le_bytes());
        hasher.update([event.trial_index.unwrap_or_default()]);
        if let Some(reason) = &event.reason {
            hasher.update(reason.as_bytes());
        }
    }
    if let Some(score) = decision_score {
        hasher.update(b"score-micros");
        for value in score {
            hasher.update(quantized_score(value).to_le_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut output = TuningHash::new();
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn digest_event_chain(events: &[ThermalTuningEvent]) -> TuningHash {
    events.iter().fold(TuningHash::new(), |digest, event| {
        chained_event_digest(&digest, event)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_op_and_cyclic_candidate_proposals_are_not_reheated() {
        let mut runtime = started_single_target_runtime();
        let original = runtime.candidate_profile.unwrap();
        assert!(!runtime.record_fresh_proposal(Some(original)));
        let mut changed = original;
        changed.points[0].as_mut().unwrap().hold_kp_permille_per_c += 1;
        runtime.candidate_profile = Some(changed);
        assert!(runtime.record_fresh_proposal(Some(original)));
        runtime.candidate_profile = Some(original);
        assert!(!runtime.record_fresh_proposal(Some(changed)));
        runtime.exhaust_candidate_search(60).unwrap();
        assert!(!runtime.armed);
        assert!(runtime.events.iter().any(|event| event.reason.as_deref()
            == Some("candidate_search_exhausted_no_distinct_safe_proposal")));
    }
    use super::*;
    use crate::memory::{ThermalControlProfilePointConfig, ThermalControlProfileSettingsConfig};

    fn replay_damping_bracket_trial(
        runtime: &mut ThermalTuningRuntime,
        trial: &serde_json::Value,
    ) -> TrialEvaluation {
        runtime.reset_trial_metrics();
        let wire: ThermalTuningCandidatePoint =
            serde_json::from_value(trial["point"].clone()).unwrap();
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        macro_rules! copy_fields {
            ($($field:ident),+ $(,)?) => { $(point.$field = wire.$field;)+ };
        }
        copy_fields!(
            brake_distance_centi_c,
            warmup_power_permille,
            warmup_reenter_centi_c,
            approach_power_permille,
            approach_floor_power_permille,
            approach_damping_exponent_permille,
            approach_tail_window_centi_c,
            hold_power_permille,
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
            hold_lead_ticks
        );
        runtime
            .install_candidate_profile(profile, "fixture_replay")
            .unwrap();
        runtime.trial_index = trial["trialIndex"].as_u64().unwrap() as u8;
        for row in trial["samples"].as_array().unwrap() {
            let mut observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
            runtime.current_elapsed_ms = observed.elapsed_ms;
            runtime.update_trial_metrics(&observed, 60);
        }
        let decision = &trial["decision"];
        TrialEvaluation {
            score: serde_json::from_value(decision["score"].clone()).unwrap(),
            passes: false,
            warmup_full: decision["warmupFull"].as_bool().unwrap(),
            max_overshoot_centi_c: decision["maxOvershootCentiC"].as_u64().map(|v| v as u16),
            hold_peak_to_peak_centi_c: decision["holdPeakToPeakCentiC"].as_u64().map(|v| v as u16),
            settle_time_ms: None,
            full_speed_limit_ms: 10_000,
        }
    }

    #[test]
    fn captured_opposite_damping_failures_bisect_the_same_run_interval() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        for trial in fixture["trials"].as_array().unwrap() {
            let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
            runtime.advance_after_trial(60, evaluation).unwrap();
        }
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(next.brake_distance_centi_c, 2_328);
        assert_eq!(next.approach_damping_exponent_permille, 2_500);
        assert_eq!(next.hold_power_permille, 56);
        assert_eq!(runtime.accepted_target_mask, 0);
        assert!(runtime.events.iter().any(|event| event.reason.as_deref()
            == Some("same_run_damping_bracket_trials_5_6_midpoint_2500")));
    }

    #[test]
    fn captured_saturated_damping_coast_search_uses_remaining_trajectory_horizon() {
        let trial: serde_json::Value = serde_json::from_str(include_str!(
            "thermal_tuning_saturated_damping.fixture.json"
        ))
        .unwrap();
        let mut runtime = started_single_target_runtime();
        let evaluation = replay_damping_bracket_trial(&mut runtime, &trial);
        let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
        let facts = runtime.candidate_failure_evidence(
            60,
            evaluation.max_overshoot_centi_c,
            evaluation.hold_peak_to_peak_centi_c,
            None,
            evaluation.full_speed_limit_ms,
        );
        assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
        assert_eq!(facts.approach_output_range, Some((50, 790)));
        assert_eq!(facts.overshoot_c, 1.57);
        runtime.advance_after_trial(60, evaluation).unwrap();
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(next.approach_lead_ticks, 2);
        assert_eq!(
            next,
            ThermalControlProfilePointConfig {
                approach_lead_ticks: 2,
                ..previous
            }
        );
        assert_eq!(runtime.accepted_target_mask, 0);
    }

    #[test]
    fn captured_coarse_brake_preserves_progress_inside_the_warmup_start_gap() {
        let trial: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_brake_backoff.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        let evaluation = replay_damping_bracket_trial(&mut runtime, &trial);
        let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(previous.brake_distance_centi_c, 1_654);
        runtime.advance_after_trial(60, evaluation).unwrap();
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(next.brake_distance_centi_c, 2_144);
        assert_eq!(
            next,
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 2_144,
                ..previous
            }
        );
        assert!(runtime.armed);
        assert_eq!(runtime.accepted_target_mask, 0);
    }

    #[test]
    fn captured_failed_damping_comparison_explores_the_existing_braking_channel() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_failed_damping.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        for trial in fixture["trials"].as_array().unwrap() {
            let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
            runtime.advance_after_trial(60, evaluation).unwrap();
        }
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(next.brake_distance_centi_c, 1_820);
        assert_eq!(next.approach_damping_exponent_permille, 2_110);
        assert_eq!(runtime.accepted_target_mask, 0);
        assert!(
            runtime.events.iter().any(|event| event.reason.as_deref()
                == Some("failed_damping_comparison_explores_braking"))
        );
    }

    #[test]
    fn failed_damping_braking_requires_matching_measured_high_side_failures() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_failed_damping.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        let mut pairs = std::vec::Vec::new();
        for trial in fixture["trials"].as_array().unwrap() {
            let evaluation = replay_damping_bracket_trial(&mut runtime, trial);
            pairs.push((
                runtime.candidate_profile,
                Some(runtime.candidate_failure_evidence(
                    60,
                    evaluation.max_overshoot_centi_c,
                    evaluation.hold_peak_to_peak_centi_c,
                    None,
                    10_000,
                )),
            ));
        }
        let ((current, facts), (predicted, predicted_facts)) = (pairs[0], pairs[1]);
        assert!(failed_damping_comparison_needs_braking(
            current,
            predicted,
            facts,
            predicted_facts,
            60
        ));
        let facts = facts.unwrap();
        for rejected in [
            None,
            Some(TuneEvidence {
                target_c: 80,
                ..facts
            }),
            Some(TuneEvidence {
                stable_window_verified: true,
                ..facts
            }),
            Some(TuneEvidence {
                hold_off_sample_count: 0,
                hold_entry_pulse_only: false,
                hold_median_output_permille: Some(500),
                hold_p90_output_permille: Some(600),
                hold_max_output_permille: Some(800),
                ..facts
            }),
        ] {
            assert!(!failed_damping_comparison_needs_braking(
                current,
                predicted,
                rejected,
                predicted_facts,
                60
            ));
        }
        let mut different = predicted.unwrap();
        different.points[0].as_mut().unwrap().brake_distance_centi_c += 1;
        assert!(!failed_damping_comparison_needs_braking(
            current,
            Some(different),
            Some(facts),
            predicted_facts,
            60
        ));
    }

    #[test]
    fn completed_scout_supplies_measured_current_without_reheating_it() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
        let score = evaluation.score;
        let current = runtime.candidate_profile;
        runtime.trial_purpose = ThermalTuningTrialPurpose::Scout;
        runtime.advance_after_trial(60, evaluation).unwrap();
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::BatchPredicted
        );
        assert_eq!(runtime.trial_index, 6);
        assert_eq!(runtime.batch_current_profile, current);
        assert_eq!(runtime.batch_current_score, Some(score));
        assert!(!runtime.batch_current_confirmable);
        assert!(runtime.batch_current_failure_evidence.is_some());
        assert_ne!(runtime.candidate_profile, current);
        assert_eq!(runtime.accepted_target_mask, 0);
    }

    #[test]
    fn damping_bracket_rejects_changed_parameters_and_cross_run_evidence() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_damping_bracket.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        let high = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
        let facts = runtime.candidate_failure_evidence(
            60,
            high.max_overshoot_centi_c,
            high.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        assert_eq!(damping_failure_side(facts), Some(true));
        for invalid in [
            TuneEvidence {
                tunable_stop: false,
                ..facts
            },
            TuneEvidence {
                stable_window_verified: true,
                ..facts
            },
            TuneEvidence {
                approach_floor_only: true,
                ..facts
            },
            TuneEvidence {
                approach_output_range: Some((0, 300)),
                ..facts
            },
        ] {
            assert_eq!(damping_failure_side(invalid), None);
        }
        runtime.remember_damping_failure(60, facts);
        let low = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
        let low_facts = runtime.candidate_failure_evidence(
            60,
            low.max_overshoot_centi_c,
            low.hold_peak_to_peak_centi_c,
            None,
            10_000,
        );
        assert_eq!(damping_failure_side(low_facts), Some(false));
        runtime.candidate_profile.as_mut().unwrap().points[0]
            .as_mut()
            .unwrap()
            .brake_distance_centi_c += 1;
        runtime.remember_damping_failure(60, low_facts);
        assert!(!runtime.prepare_damping_bracket(runtime.candidate_profile.unwrap()));
        runtime.reset_for_run();
        assert!(runtime.observed_damping_failures.is_empty());
    }

    #[test]
    fn fan_sample_digest_matches_web_and_binds_applied_command() {
        let (event, _) = serde_json_core::from_slice::<ThermalTuningEvent>(include_bytes!(
            "../../web/src/features/control-plane-demo/thermal-tuning-fan.fixture.json"
        ))
        .unwrap();
        let expected = digest_event_chain(&[event.clone()]);
        assert_eq!(
            expected.as_str(),
            "966de02dc5498cccd6d899bb359104a1e1e83a0ec4d7dca65da7fd9dc6546ea6"
        );
        let mut altered = event;
        altered
            .sample
            .as_mut()
            .unwrap()
            .fan
            .as_mut()
            .unwrap()
            .enabled = false;
        assert_ne!(expected, digest_event_chain(&[altered]));
    }

    #[test]
    fn canonical_candidate_hash_matches_web_wire_fixture() {
        let point = |target_temp_c| ThermalControlProfilePointConfig {
            target_temp_c,
            brake_distance_centi_c: 1,
            warmup_power_permille: 1,
            warmup_reenter_centi_c: 1,
            approach_power_permille: 1,
            approach_floor_power_permille: 1,
            approach_damping_exponent_permille: 1,
            approach_tail_window_centi_c: 1,
            hold_power_permille: 1,
            hold_reheat_power_permille: 1,
            hold_entry_centi_c: 1,
            hold_exit_centi_c: 1,
            hold_on_centi_c: 1,
            hold_off_centi_c: 1,
            overshoot_cutoff_centi_c: 1,
            hold_kp_permille_per_c: 1,
            hold_ki_permille_per_c_tick: 1,
            hold_blend_ticks: 1,
            approach_lead_ticks: 1,
            hold_lead_ticks: 1,
        };
        let profile = ThermalControlProfileConfig {
            settings: ThermalControlProfileSettingsConfig {
                temp_filter_alpha_permille: 1,
                warmup_reenter_centi_c: 1,
                hold_entry_centi_c: 1,
                hold_exit_centi_c: 1,
                hold_on_centi_c: 1,
                hold_off_centi_c: 1,
                overshoot_cutoff_centi_c: 1,
                approach_max_ticks: 1,
                approach_min_power_ratio_permille: 1,
                hold_kp_permille_per_c: 1,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                hold_reheat_power_permille: 1,
                approach_lead_ticks: 1,
                hold_lead_ticks: 1,
                auto_adjustable_working_floor_mv: 1,
                heater_current_reserve_ma: 1,
            },
            points: [
                Some(point(60)),
                Some(point(80)),
                Some(point(100)),
                Some(point(120)),
                Some(point(140)),
                Some(point(160)),
                Some(point(180)),
                Some(point(220)),
                None,
                None,
            ],
        };
        assert_eq!(
            hash_profile(&profile).unwrap().as_str(),
            "sha256:896e40c0087904f10eee7db9ddacad5e391cfaf5915923aacf27f83f3ddefe53"
        );
    }

    #[test]
    fn decision_event_digest_fixture_for_web_export() {
        let event: ThermalTuningEvent = serde_json::from_value(serde_json::json!({
            "sequence": 0,
            "elapsedMs": 4500,
            "kind": "decision",
            "phase": "hold",
            "targetC": 60,
            "trialIndex": 1,
            "sample": null,
            "reason": "candidate_adopted",
            "trial": null,
            "decision": {
                "disposition": "adopted",
                "purpose": "hold_confirm",
                "maxOvershootCentiC": 125,
                "holdPeakToPeakCentiC": 155,
                "settleTimeMs": 7800,
                "score": [0.0, 1.0, 1.25, 2.0, 3.5, 4.0, 5.25, 6.0, 7.5, 8.0, 9.25],
                "warmupFull": true,
                "approachToHold": true,
                "holdConfirm": true,
                "hardGateOvershootCentiC": 300,
                "hardGateHoldPeakToPeakCentiC": 300,
                "hardGateFullSpeedToStableMs": 12000,
                "freeze": true,
                "prune": false,
                "stopReason": "accepted"
            }
        }))
        .unwrap();
        let mut encoded = [0u8; 4096];
        let length = serde_json_core::to_slice(&event, &mut encoded).unwrap();
        assert!(
            core::str::from_utf8(&encoded[..length])
                .unwrap()
                .contains("\"score\":[0.0,1.0,1.25")
        );
        assert_eq!(
            digest_event_chain(&[event]).as_str(),
            "97d020d2c3f902b33a1d48b53e684e2c5e93d7bd76eb6138f249ccbc3e51e893"
        );
    }

    #[test]
    fn decision_digest_quantizes_float_score_for_cross_target_stability() {
        let mut event = serde_json::from_value::<ThermalTuningEvent>(serde_json::json!({
            "sequence": 0,
            "elapsedMs": 0,
            "kind": "decision",
            "phase": "hold",
            "targetC": 60,
            "trialIndex": 0,
            "sample": null,
            "reason": "accepted",
            "trial": null,
            "decision": {
                "disposition": "accepted",
                "purpose": "hold_confirm",
                "maxOvershootCentiC": 100,
                "holdPeakToPeakCentiC": 100,
                "settleTimeMs": 1000,
                "score": [1.2345670, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                "warmupFull": true,
                "approachToHold": true,
                "holdConfirm": true,
                "hardGateOvershootCentiC": 300,
                "hardGateHoldPeakToPeakCentiC": 300,
                "hardGateFullSpeedToStableMs": 10000,
                "freeze": true,
                "prune": false,
                "stopReason": "accepted"
            }
        }))
        .unwrap();
        let baseline = digest_event_chain(&[event.clone()]);
        event.decision.as_mut().unwrap().score[0] += 0.0000001;
        assert_eq!(baseline, digest_event_chain(&[event]));
    }

    fn id(value: &str) -> TuningId {
        let mut result = TuningId::new();
        result.push_str(value).unwrap();
        result
    }

    fn hash(value: &str) -> TuningHash {
        let mut result = TuningHash::new();
        result.push_str(value).unwrap();
        result
    }

    fn profile_with_targets(targets: &[i16]) -> ThermalControlProfileConfig {
        let mut profile = ThermalControlProfileConfig::default();
        for (index, target_c) in targets.iter().copied().enumerate() {
            profile.points[index] = Some(ThermalControlProfilePointConfig {
                target_temp_c: target_c,
                brake_distance_centi_c: 300,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 400,
                approach_power_permille: 750,
                approach_floor_power_permille: 400,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 200,
                hold_power_permille: 350,
                hold_reheat_power_permille: 450,
                hold_entry_centi_c: 90,
                hold_exit_centi_c: 200,
                hold_on_centi_c: 30,
                hold_off_centi_c: 5,
                overshoot_cutoff_centi_c: 25,
                hold_kp_permille_per_c: 120,
                hold_ki_permille_per_c_tick: 12,
                hold_blend_ticks: 12,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            });
        }
        profile
    }

    pub(super) fn sample(
        elapsed_ms: u64,
        phase: ThermalTuningPhase,
        temperature_centi_c: i16,
    ) -> ThermalTuningSample {
        ThermalTuningSample {
            sequence: 999,
            elapsed_ms,
            target_c: 60,
            trial_index: 0,
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            temperature_centi_c,
            raw_temperature_centi_c: None,
            vin_mv: 12_000,
            pps_contract: ThermalTuningPpsContract {
                voltage_mv: 21_000,
                current_ma: 5_000,
                kind: ThermalTuningPowerClass::Pps5a,
                confirmed: true,
            },
            requested_mv: 21_000,
            pps_power_ceiling_mv: 21_000,
            heater_requested_output_permille: if phase == ThermalTuningPhase::Warmup {
                1_000
            } else {
                400
            },
            heater_output_permille: if phase == ThermalTuningPhase::Warmup {
                1_000
            } else {
                380
            },
            measurement_valid: true,
            measurement_accepted: None,
            phase,
            fan: None,
        }
    }

    fn record_initial_cooldown(runtime: &mut ThermalTuningRuntime, elapsed_ms: u64) {
        let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 3_400);
        cooling.trial_index = runtime.trial_index;
        cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    }

    #[test]
    fn cooldown_accepts_idle_request_when_trial_contract_is_confirmed() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-power-staging"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();

        let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_400);
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        cooling.requested_mv = 12_000;
        cooling.pps_contract.voltage_mv = 12_000;
        runtime.record_sample(cooling.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);

        cooling.elapsed_ms = 300;
        cooling.pps_contract.voltage_mv = 12_000;
        runtime.record_sample(cooling.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);

        cooling.elapsed_ms = 600;
        cooling.pps_contract.voltage_mv = 21_000;
        runtime.record_sample(cooling).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    }

    #[test]
    fn cooldown_requires_current_accepted_temperature_without_pausing_budget() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-current-temperature"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        for (elapsed_ms, temperature_centi_c) in [(0, 4_307), (300, 3_454)] {
            let mut observed = sample(
                elapsed_ms,
                ThermalTuningPhase::Cooldown,
                temperature_centi_c,
            );
            observed.heater_requested_output_permille = 0;
            observed.heater_output_permille = 0;
            runtime
                .record_live_sample_with_cooldown_admission(observed, false)
                .unwrap();
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        }
        assert!(!runtime.events.iter().any(|event| event.trial.is_some()));
        assert_eq!(runtime.current_elapsed_ms, 300);
        let temperatures: std::vec::Vec<_> = runtime
            .events
            .iter()
            .filter_map(|event| {
                event
                    .sample
                    .as_ref()
                    .map(|sample| sample.temperature_centi_c)
            })
            .collect();
        assert_eq!(temperatures, [4_307, 3_454]);

        let mut accepted = sample(600, ThermalTuningPhase::Cooldown, 3_454);
        accepted.heater_requested_output_permille = 0;
        accepted.heater_output_permille = 0;
        let mut exhausted = runtime.clone();
        let mut pending = accepted.clone();
        pending.elapsed_ms = THERMAL_TUNING_TARGET_BUDGET_MS;
        exhausted
            .record_live_sample_with_cooldown_admission(pending, false)
            .unwrap();
        assert!(!exhausted.armed);
        assert!(exhausted.events.iter().any(|event| {
            event.reason.as_deref() == Some("target_budget_exhausted_during_cooldown")
        }));
        assert!(!exhausted.events.iter().any(|event| event.trial.is_some()));

        runtime
            .record_live_sample_with_cooldown_admission(accepted, true)
            .unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
        assert_eq!(runtime.trial_start_temperature_centi_c, Some(3_454));
    }

    #[test]
    fn firmware_60c_initial_and_retry_trials_start_at_35c() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-35c-cooldown"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();

        let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_501);
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        cooling.elapsed_ms = 300;
        cooling.temperature_centi_c = 3_500;
        runtime.record_sample(cooling).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
        let first = runtime
            .events
            .iter()
            .find_map(|event| event.trial.as_ref())
            .unwrap();
        assert_eq!(first.start_temperature_centi_c, Some(3_500));
        assert_eq!(first.cooldown_threshold_centi_c, Some(3_500));

        for (elapsed_ms, phase, temperature_centi_c) in [
            (300, ThermalTuningPhase::Warmup, 3_500),
            (1_300, ThermalTuningPhase::Approach, 4_500),
            (11_300, ThermalTuningPhase::Approach, 5_000),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature_centi_c))
                .unwrap();
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.trial_index, 1);
        let closed_first = runtime
            .events
            .iter()
            .filter_map(|event| event.trial.as_ref())
            .find(|trial| trial.trial_index == 0 && trial.end_sequence.is_some())
            .unwrap();
        assert_eq!(closed_first.start_temperature_centi_c, Some(3_500));
        assert_eq!(closed_first.cooldown_threshold_centi_c, Some(3_500));

        let mut retry = sample(11_600, ThermalTuningPhase::Cooldown, 3_501);
        retry.trial_index = runtime.trial_index;
        retry.candidate_hash = runtime.candidate_hash.clone().unwrap();
        retry.heater_requested_output_permille = 0;
        retry.heater_output_permille = 0;
        runtime.record_sample(retry.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        retry.elapsed_ms = 11_900;
        retry.temperature_centi_c = 3_500;
        runtime.record_sample(retry).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
        let retry_trial = runtime
            .events
            .iter()
            .filter_map(|event| event.trial.as_ref())
            .find(|trial| trial.trial_index == 1)
            .unwrap();
        assert_eq!(retry_trial.start_temperature_centi_c, Some(3_500));
        assert_eq!(retry_trial.cooldown_threshold_centi_c, Some(3_500));
        assert_eq!(cooldown_threshold_c(80), 40);
        assert_eq!(cooldown_threshold_c(240), 220);
    }

    #[test]
    fn high_temperature_cooldown_keeps_strict_160_and_200_boundaries() {
        for (target, threshold) in [
            (160, 120),
            (161, 131),
            (180, 150),
            (200, 170),
            (201, 181),
            (220, 200),
            (240, 220),
        ] {
            assert_eq!(cooldown_threshold_c(target), threshold);
        }
    }

    #[test]
    fn every_high_temperature_trial_waits_for_and_records_its_cooldown_boundary() {
        for (target, threshold) in [(160, 12_000), (180, 15_000), (220, 20_000)] {
            let mut runtime = ThermalTuningRuntime::new();
            runtime
                .start(ThermalTuningStart {
                    run_id: id("run-high-temperature-cooldown"),
                    candidate_id: id("candidate-hold"),
                    candidate_hash: hash("hash-hold"),
                    power_class: ThermalTuningPowerClass::Pps5a,
                    targets: &[target],
                    pps_ready: true,
                    thermal_model_available: true,
                    calibration_conflict: false,
                    candidate_profile: Some(profile_with_targets(&[target])),
                })
                .unwrap();
            for purpose in [
                ThermalTuningTrialPurpose::Scout,
                ThermalTuningTrialPurpose::BatchCurrent,
                ThermalTuningTrialPurpose::BatchPredicted,
                ThermalTuningTrialPurpose::HoldConfirm,
            ] {
                if purpose != ThermalTuningTrialPurpose::Scout {
                    runtime
                        .schedule_trial(purpose, "cooling before test trial")
                        .unwrap();
                }
                let mut cooling = sample(
                    runtime.current_elapsed_ms + 300,
                    ThermalTuningPhase::Cooldown,
                    threshold + 1,
                );
                cooling.target_c = target;
                cooling.trial_index = runtime.trial_index;
                cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
                cooling.heater_requested_output_permille = 0;
                cooling.heater_output_permille = 0;
                runtime.record_sample(cooling.clone()).unwrap();
                assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
                cooling.elapsed_ms += 300;
                cooling.temperature_centi_c = threshold;
                runtime.record_sample(cooling).unwrap();
                assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
                let trial = runtime
                    .events
                    .iter()
                    .rev()
                    .find_map(|event| event.trial.as_ref())
                    .unwrap();
                assert_eq!(trial.purpose, purpose);
                assert_eq!(trial.start_temperature_centi_c, Some(threshold));
                assert_eq!(trial.cooldown_threshold_centi_c, Some(threshold));
            }
        }
    }

    #[test]
    fn high_side_trace_stops_when_the_stable_window_breaks_before_deadline() {
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 1_100;
        point.approach_lead_ticks = 6;
        point.approach_power_permille = 450;
        point.approach_floor_power_permille = 160;
        point.approach_damping_exponent_permille = 4_000;
        point.hold_power_permille = 135;
        point.hold_reheat_power_permille = 170;
        point.hold_entry_centi_c = 220;
        point.hold_exit_centi_c = 400;
        point.hold_lead_ticks = 6;
        point.hold_blend_ticks = 1;
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("high-side-trace"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        let mut cooldown = sample(0, ThermalTuningPhase::Cooldown, 3_470);
        cooldown.heater_requested_output_permille = 0;
        cooldown.heater_output_permille = 0;
        runtime.record_sample(cooldown).unwrap();
        for (elapsed_ms, phase, temperature_centi_c) in [
            (1_000, ThermalTuningPhase::Warmup, 3_470),
            (17_000, ThermalTuningPhase::Warmup, 4_800),
            (17_587, ThermalTuningPhase::Approach, 4_823),
            (21_131, ThermalTuningPhase::Approach, 5_725),
            (21_488, ThermalTuningPhase::Hold, 5_797),
            (21_932, ThermalTuningPhase::Hold, 5_855),
            (24_800, ThermalTuningPhase::Hold, 6_148),
            (25_147, ThermalTuningPhase::Hold, 6_154),
            (27_573, ThermalTuningPhase::Hold, 6_319),
            (27_916, ThermalTuningPhase::Hold, 6_307),
        ] {
            let mut observed = sample(elapsed_ms, phase, temperature_centi_c);
            if phase == ThermalTuningPhase::Hold {
                observed.heater_requested_output_permille = 0;
                observed.heater_output_permille = 0;
            }
            runtime.record_sample(observed).unwrap();
        }
        assert_eq!(runtime.trial_index, 1);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        let decision_event = runtime
            .events
            .iter()
            .find(|event| event.decision.is_some())
            .unwrap();
        assert_eq!(decision_event.elapsed_ms, 27_916);
        assert_eq!(
            decision_event.decision.as_ref().unwrap().stop_reason,
            "full-speed-to-stable budget exhausted"
        );
        let selected = runtime.batch_predicted_profile.unwrap();
        let selected_point = selected.points[0].unwrap();
        assert_eq!(selected_point.brake_distance_centi_c, 1_400);
        assert_eq!(selected_point.approach_power_permille, 350);
    }

    #[test]
    fn live_samples_retain_100ms_soft_start_before_full_output() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-soft-start"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        for (elapsed_ms, output) in [(50, 0), (100, 500), (150, 1_000), (200, 1_000)] {
            let mut warmup = sample(elapsed_ms, ThermalTuningPhase::Warmup, 3_400);
            warmup.heater_output_permille = output;
            runtime.record_live_sample(warmup).unwrap();
        }
        let outputs: std::vec::Vec<_> = runtime
            .events
            .iter()
            .filter_map(|event| {
                event
                    .sample
                    .as_ref()
                    .filter(|sample| sample.phase == ThermalTuningPhase::Warmup)
                    .map(|sample| (sample.elapsed_ms, sample.heater_output_permille))
            })
            .collect();
        assert_eq!(outputs, [(50, 0), (100, 500), (150, 1_000)]);
        assert!(runtime.warmup_full_output);
    }

    #[test]
    fn live_samples_follow_host_cadence_without_skipping_safety() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-live-cadence"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        for elapsed_ms in (0..=1_000).step_by(100) {
            let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 7_000);
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_live_sample(cooling).unwrap();
        }
        assert_eq!(
            runtime
                .events
                .iter()
                .filter(|event| event.kind == ThermalTuningEventKind::Sample)
                .count(),
            6
        );

        let mut invalid = sample(1_050, ThermalTuningPhase::Warmup, 7_000);
        invalid.heater_requested_output_permille = 0;
        invalid.heater_output_permille = 0;
        invalid.pps_contract.confirmed = false;
        assert_eq!(
            runtime.record_live_sample(invalid),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!runtime.armed);
    }

    #[test]
    fn live_sample_pages_remain_contiguous_beyond_event_capacity() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-live-pages"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        let mut cursor = None;
        let mut received = Vec::<u32, 512>::new();
        for elapsed_ms in (0..=90_000).step_by(100) {
            let mut cooling = sample(elapsed_ms, ThermalTuningPhase::Warmup, 7_000);
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_live_sample(cooling).unwrap();
            if elapsed_ms % 1_200 == 0 {
                let page = runtime.events_after(cursor);
                for event in page.events {
                    received.push(event.sequence).unwrap();
                }
                cursor = page.next_sequence.or(cursor);
            }
        }
        loop {
            let page = runtime.events_after(cursor);
            if page.events.is_empty() {
                break;
            }
            for event in page.events {
                received.push(event.sequence).unwrap();
            }
            cursor = page.next_sequence;
        }
        assert!(!runtime.trace().gap);
        assert!(received.len() > THERMAL_TUNING_EVENT_CAPACITY);
        assert!(
            received
                .iter()
                .enumerate()
                .all(|(index, sequence)| *sequence == index as u32)
        );
    }

    fn record_current_trial_sample(
        runtime: &mut ThermalTuningRuntime,
        elapsed_ms: u64,
        phase: ThermalTuningPhase,
        temperature_centi_c: i16,
    ) {
        let mut observed = sample(elapsed_ms, phase, temperature_centi_c);
        observed.trial_index = runtime.trial_index;
        observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
        if phase == ThermalTuningPhase::Cooldown {
            observed.heater_requested_output_permille = 0;
            observed.heater_output_permille = 0;
        }
        runtime.record_sample(observed).unwrap();
    }

    fn record_dense_hold(
        runtime: &mut ThermalTuningRuntime,
        first: u64,
        last: u64,
        temperature: i16,
    ) {
        for elapsed in (first..=last).step_by(300) {
            record_current_trial_sample(runtime, elapsed, ThermalTuningPhase::Hold, temperature);
            if runtime.events.len() > 128 {
                let mut cursor = runtime.last_acknowledged_sequence;
                loop {
                    let page = runtime.events_after(cursor);
                    if page.events.is_empty() {
                        break;
                    }
                    cursor = page.next_sequence;
                }
                runtime.events_after(cursor);
            }
        }
    }

    fn started_single_target_runtime() -> ThermalTuningRuntime {
        started_runtime_with_profile(profile_with_targets(&[60]))
    }

    #[test]
    fn fixed_idle_preparation_never_admits_an_unconfirmed_trial() {
        let request = || ThermalTuningStart {
            run_id: id("run-preparing-power"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            targets: &[60],
            pps_ready: false,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile_with_targets(&[60])),
        };
        let mut strict = ThermalTuningRuntime::new();
        assert!(strict.start(request()).is_err());
        assert!(!strict.armed);

        let mut preparing = ThermalTuningRuntime::new();
        preparing.start_preparing_power_at(request(), 0).unwrap();
        assert_eq!(preparing.phase, ThermalTuningPhase::Cooldown);
        let mut unconfirmed = sample(0, ThermalTuningPhase::Cooldown, 3_400);
        unconfirmed.heater_requested_output_permille = 0;
        unconfirmed.heater_output_permille = 0;
        unconfirmed.pps_contract.confirmed = false;
        assert!(preparing.record_sample(unconfirmed).is_err());
        assert!(!preparing.armed);

        let mut confirmed = ThermalTuningRuntime::new();
        confirmed.start_preparing_power_at(request(), 0).unwrap();
        record_initial_cooldown(&mut confirmed, 0);
        assert_eq!(confirmed.phase, ThermalTuningPhase::Warmup);
    }

    fn started_runtime_with_profile(profile: ThermalControlProfileConfig) -> ThermalTuningRuntime {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-speed-gate"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        runtime
    }

    #[test]
    fn approach_at_allowed_floor_can_continue_after_original_deadline() {
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        point.approach_floor_power_permille = 100;
        point.approach_power_permille = 180;
        point.hold_power_permille = 60;
        let mut runtime = started_runtime_with_profile(profile);
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        for elapsed_ms in [1_000, 5_000, 11_000] {
            let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
            observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
            observed.heater_requested_output_permille = 50;
            observed.heater_output_permille = 170;
            runtime.record_sample(observed).unwrap();
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
        assert_eq!(runtime.trial_index, 0);
        assert_eq!(runtime.trial_timing.unwrap().policy_version, 4);
        assert!(runtime.events.iter().all(|event| event.decision.is_none()));
    }

    #[test]
    fn a_hold_window_break_cannot_obtain_a_retrospective_approach_extension() {
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        point.approach_floor_power_permille = 100;
        point.hold_power_permille = 60;
        let mut runtime = started_runtime_with_profile(profile);
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        for elapsed in [1_000, 2_000] {
            let mut observed = sample(elapsed, ThermalTuningPhase::Approach, 5_500);
            observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
            observed.heater_requested_output_permille = 100;
            runtime.record_sample(observed).unwrap();
        }
        record_dense_hold(&mut runtime, 3_000, 10_800, 6_000);
        record_current_trial_sample(&mut runtime, 11_000, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        record_current_trial_sample(&mut runtime, 12_000, ThermalTuningPhase::Hold, 6_170);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert!(decision.timing.unwrap().extended_at_ms.is_none());
    }

    #[test]
    fn deadline_coast_preserves_bound_floor_evidence_in_the_live_trial() {
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        point.approach_floor_power_permille = 100;
        point.hold_power_permille = 60;
        let mut runtime = started_runtime_with_profile(profile);
        let candidate_hash = runtime.candidate_hash.clone().unwrap();
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        for (elapsed_ms, request) in [(1_000, 50), (5_000, 50), (11_000, 0)] {
            let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
            observed.candidate_hash = candidate_hash.clone();
            observed.heater_requested_output_permille = request;
            observed.heater_output_permille = request;
            runtime.record_sample(observed).unwrap();
        }
        assert!(runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
        assert_eq!(runtime.trial_index, 0);
        assert_eq!(runtime.candidate_hash.as_ref(), Some(&candidate_hash));
        assert!(runtime.trial_timing.unwrap().valid_extension());
        assert!(runtime.events.iter().all(|event| event.decision.is_none()));
    }

    pub(super) fn extended_floor_runtime() -> ThermalTuningRuntime {
        let mut profile = profile_with_targets(&[60]);
        let point = profile.points[0].as_mut().unwrap();
        point.approach_floor_power_permille = 100;
        point.hold_power_permille = 60;
        let mut runtime = started_runtime_with_profile(profile);
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        for elapsed_ms in (1_000..=12_000).step_by(300) {
            let mut observed = sample(elapsed_ms, ThermalTuningPhase::Approach, 5_500);
            observed.candidate_hash = runtime.candidate_hash.clone().unwrap();
            observed.heater_requested_output_permille = 50;
            runtime.record_sample(observed).unwrap();
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Approach);
        assert!(runtime.trial_timing.unwrap().valid_extension());
        runtime
    }

    #[test]
    fn extended_scout_still_requires_a_full_stable_window_and_qualified_hold() {
        let mut runtime = extended_floor_runtime();
        for elapsed_ms in (13_000..=25_000).step_by(300) {
            record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "passed");
        assert_eq!(decision.settle_time_ms, Some(12_000));
        assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
        assert!(!decision.hold_confirm);
        assert!(!decision.freeze);
        assert!(decision.timing.unwrap().valid_extension());
        assert!(!runtime.save_confirmed);
    }

    #[test]
    fn ordinary_scout_waits_for_a_timely_restarted_stable_window() {
        let mut runtime = started_single_target_runtime();
        record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
        for elapsed_ms in (1_000..=8_800).step_by(300) {
            let temperature = if elapsed_ms == 1_000 { 6_000 } else { 6_170 };
            let phase = if elapsed_ms == 1_000 {
                ThermalTuningPhase::Approach
            } else {
                ThermalTuningPhase::Hold
            };
            record_current_trial_sample(&mut runtime, elapsed_ms, phase, temperature);
        }
        record_dense_hold(&mut runtime, 9_100, 13_300, 6_000);
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert_eq!(runtime.trial_index, 0);
        assert!(runtime.events.iter().all(|event| event.decision.is_none()));
        assert_eq!(runtime.stable_window_started_at_ms, Some(9_100));
        assert_eq!(runtime.stable_window_verified_at_ms, None);
        assert_eq!(runtime.trial_timing.unwrap().extended_at_ms, None);

        record_dense_hold(&mut runtime, 13_600, 19_000, 6_000);
        record_current_trial_sample(&mut runtime, 19_100, ThermalTuningPhase::Hold, 6_000);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "passed");
        assert_eq!(decision.settle_time_ms, Some(8_100));
        assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 10_000);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert!(!runtime.save_confirmed);
    }

    #[test]
    fn live_recording_verifies_stability_with_observed_control_intervals() {
        for control_interval_ms in [68, 83, 96] {
            let mut runtime = started_single_target_runtime();
            record_current_trial_sample(&mut runtime, 0, ThermalTuningPhase::Warmup, 3_400);
            runtime
                .record_live_sample(sample(1_000, ThermalTuningPhase::Approach, 5_900))
                .unwrap();
            for elapsed_ms in (1_068..=20_000).step_by(control_interval_ms) {
                let temperature = if elapsed_ms < 9_000 { 6_170 } else { 6_000 };
                runtime
                    .record_live_sample(sample(elapsed_ms, ThermalTuningPhase::Hold, temperature))
                    .unwrap();
                if runtime.phase == ThermalTuningPhase::Cooldown {
                    break;
                }
            }
            assert_eq!(
                runtime.phase,
                ThermalTuningPhase::Cooldown,
                "control interval {control_interval_ms}ms must not starve the stable recorder"
            );
            let decision = runtime
                .events
                .iter()
                .find_map(|event| event.decision.as_ref())
                .unwrap();
            assert_eq!(decision.disposition.as_str(), "passed");
            let timing = decision.timing.unwrap();
            let window_ms = timing.stable_window_verified_at_ms.unwrap()
                - timing.stable_window_started_at_ms.unwrap();
            assert!((10_000..=11_000).contains(&window_ms));
        }
    }

    #[test]
    fn long_scout_cannot_claim_independent_confirmation() {
        let mut runtime = extended_floor_runtime();
        for elapsed_ms in (13_000..=75_000).step_by(340) {
            record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert!(runtime.stable_window_verified_at_ms.is_none());
        for elapsed_ms in (75_040..=85_000).step_by(200) {
            record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
            if runtime.phase == ThermalTuningPhase::Cooldown {
                break;
            }
        }
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.purpose, ThermalTuningTrialPurpose::Scout);
        assert_eq!(decision.disposition.as_str(), "passed");
        assert!(!decision.hold_confirm);
        assert!(!decision.freeze);
        assert_eq!(runtime.accepted_target_mask, 0);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert!(!runtime.save_confirmed);
    }

    fn extended_scout_after_minimum_hold(window_start_ms: u64) -> ThermalTuningRuntime {
        let mut runtime = extended_floor_runtime();
        for elapsed_ms in (13_000..window_start_ms).step_by(300) {
            let temperature = if elapsed_ms == 13_000 { 6_000 } else { 6_170 };
            record_current_trial_sample(
                &mut runtime,
                elapsed_ms,
                ThermalTuningPhase::Hold,
                temperature,
            );
        }
        record_current_trial_sample(
            &mut runtime,
            window_start_ms,
            ThermalTuningPhase::Hold,
            6_000,
        );
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert_eq!(runtime.trial_index, 0);
        assert_eq!(runtime.stable_window_started_at_ms, Some(window_start_ms));
        assert_eq!(runtime.stable_window_verified_at_ms, None);
        runtime
    }

    #[test]
    fn extended_scout_waits_for_the_same_window_beyond_minimum_hold() {
        let mut runtime = extended_scout_after_minimum_hold(20_200);
        let candidate_hash = runtime.candidate_hash.clone();
        record_dense_hold(&mut runtime, 20_500, 25_000, 6_000);
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert_eq!(runtime.trial_index, 0);
        assert!(runtime.events.iter().all(|event| event.decision.is_none()));

        record_dense_hold(&mut runtime, 25_300, 30_100, 6_000);
        record_current_trial_sample(&mut runtime, 30_200, ThermalTuningPhase::Hold, 6_000);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "passed");
        assert_eq!(decision.settle_time_ms, Some(19_200));
        assert_eq!(
            decision.timing.unwrap().stable_window_verified_at_ms,
            Some(30_200)
        );
        assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
        assert_eq!(runtime.candidate_hash, candidate_hash);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert!(!runtime.save_confirmed);
    }

    #[test]
    fn waiting_scout_still_stops_when_its_window_breaks_after_the_cap() {
        let mut runtime = extended_scout_after_minimum_hold(30_500);
        record_current_trial_sample(&mut runtime, 30_800, ThermalTuningPhase::Hold, 6_000);
        record_current_trial_sample(&mut runtime, 31_100, ThermalTuningPhase::Hold, 6_000);
        record_current_trial_sample(&mut runtime, 31_200, ThermalTuningPhase::Hold, 6_170);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert_eq!(
            decision.stop_reason,
            "full-speed-to-stable budget exhausted"
        );
        assert_eq!(decision.timing.unwrap().stable_window_verified_at_ms, None);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.target_started_elapsed_ms, 0);
    }

    #[test]
    fn waiting_scout_still_disarms_on_invalid_measurements() {
        let mut runtime = extended_scout_after_minimum_hold(20_200);
        record_dense_hold(&mut runtime, 20_500, 25_000, 6_000);
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert_eq!(runtime.trial_index, 0);
        let mut invalid = sample(25_300, ThermalTuningPhase::Hold, 6_000);
        invalid.candidate_hash = runtime.candidate_hash.clone().unwrap();
        invalid.measurement_valid = false;
        assert_eq!(
            runtime.record_sample(invalid),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
    }

    #[test]
    fn extension_stops_at_its_absolute_deadline_without_resetting_the_budget() {
        let mut runtime = extended_floor_runtime();
        record_current_trial_sample(&mut runtime, 31_000, ThermalTuningPhase::Approach, 5_500);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.target_started_elapsed_ms, 0);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert_eq!(decision.hard_gate_full_speed_to_stable_ms, 30_000);
        assert_eq!(decision.timing.unwrap().original_limit_ms, 10_000);
    }

    #[test]
    fn extended_confirmation_keeps_all_sixty_seconds_and_never_automatically_saves() {
        let mut runtime = extended_floor_runtime();
        runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
        for elapsed_ms in (13_000..=72_700).step_by(300) {
            record_current_trial_sample(&mut runtime, elapsed_ms, ThermalTuningPhase::Hold, 6_000);
        }
        assert!(runtime.armed);
        assert_eq!(runtime.accepted_target_mask, 0);
        record_current_trial_sample(&mut runtime, 73_000, ThermalTuningPhase::Hold, 6_000);
        assert!(!runtime.armed);
        assert_eq!(runtime.accepted_target_mask, target_bit(60));
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "adopted");
        assert!(decision.hold_confirm);
        assert!(decision.freeze);
        assert!(decision.timing.unwrap().valid_extension());
        assert!(!runtime.save_confirmed);
        assert!(runtime.save_receipt.is_none());
    }

    #[test]
    fn extended_trial_cannot_ignore_overshoot_or_an_invalid_sensor() {
        let mut overshoot = extended_floor_runtime();
        record_current_trial_sample(&mut overshoot, 13_000, ThermalTuningPhase::Hold, 6_301);
        assert_eq!(overshoot.phase, ThermalTuningPhase::Cooldown);
        let decision = overshoot
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert_eq!(
            decision.stop_reason.as_str(),
            "conditional extension thermal gate failed"
        );
        let mut invalid = extended_floor_runtime();
        let mut observed = sample(13_000, ThermalTuningPhase::Approach, 5_500);
        observed.candidate_hash = invalid.candidate_hash.clone().unwrap();
        observed.measurement_valid = false;
        assert_eq!(
            invalid.record_sample(observed),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!invalid.armed);
        assert_eq!(
            invalid.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
    }

    #[test]
    fn scout_and_batch_stop_at_full_speed_deadline_without_stable_window() {
        for purpose in [
            ThermalTuningTrialPurpose::Scout,
            ThermalTuningTrialPurpose::BatchCurrent,
            ThermalTuningTrialPurpose::BatchPredicted,
        ] {
            let mut runtime = started_single_target_runtime();
            runtime.trial_purpose = purpose;
            if purpose != ThermalTuningTrialPurpose::Scout {
                runtime.batch_current_profile = runtime.candidate_profile;
            }
            for (elapsed_ms, phase, temperature) in [
                (0, ThermalTuningPhase::Warmup, 3_900),
                (1_000, ThermalTuningPhase::Approach, 5_000),
                (5_000, ThermalTuningPhase::Hold, 5_500),
                (11_000, ThermalTuningPhase::Hold, 5_500),
            ] {
                record_current_trial_sample(&mut runtime, elapsed_ms, phase, temperature);
            }
            if purpose == ThermalTuningTrialPurpose::BatchPredicted {
                // This isolated deadline harness has no preceding current
                // trial. Never substitute the losing trial's facts for it.
                assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
                assert!(!runtime.armed);
            } else {
                assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown, "{purpose:?}");
                assert_eq!(runtime.trial_index, 1, "{purpose:?}");
            }
            let decision_event = runtime
                .events
                .iter()
                .find(|event| event.decision.is_some())
                .unwrap();
            let decision = decision_event.decision.as_ref().unwrap();
            assert_eq!(decision_event.elapsed_ms, 11_000);
            assert_eq!(decision.purpose, purpose);
            assert_eq!(decision.disposition.as_str(), "rejected");
            assert_eq!(decision.max_overshoot_centi_c, Some(0));
            assert_eq!(decision.hold_peak_to_peak_centi_c, None);
            assert!(!decision.hold_confirm);
        }
    }

    #[test]
    fn live_trial_five_replay_stops_before_qualified_hold_despite_controller_hold() {
        let mut runtime = started_single_target_runtime();
        // Actual phase/temperature boundary observations from run
        // firmware-pps5a-1790966844116, trial 5, relative to its trial start.
        for (elapsed_ms, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_495),
            (4_061, ThermalTuningPhase::Approach, 4_314),
            (13_840, ThermalTuningPhase::Hold, 5_582),
        ] {
            runtime
                .record_live_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
        assert_eq!(runtime.phase, ThermalTuningPhase::Hold);
        assert!(runtime.first_hold_elapsed_ms.is_none());
        // Controller Hold cannot extend the full-speed-to-stable deadline.
        runtime
            .record_live_sample(sample(14_061, ThermalTuningPhase::Hold, 5_593))
            .unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.trial_index, 1);
        let event = runtime
            .events
            .iter()
            .find(|event| event.decision.is_some())
            .unwrap();
        let decision = event.decision.as_ref().unwrap();
        assert_eq!(event.elapsed_ms, 14_061);
        assert_eq!(
            decision.stop_reason,
            "full-speed-to-stable budget exhausted"
        );
        assert!(decision.settle_time_ms.is_none());
        assert!(decision.hold_peak_to_peak_centi_c.is_none());
        assert_eq!(runtime.accepted_target_mask, 0);
    }

    #[test]
    fn live_deadline_checks_high_temperature_and_broken_timely_window_each_cycle() {
        for (target, window_start, elapsed_ms, temperature) in [
            (180, None, 6_000, 17_000),
            (60, Some(10_900), 11_010, 5_840),
        ] {
            let mut runtime = ThermalTuningRuntime::new();
            runtime
                .start(ThermalTuningStart {
                    run_id: id("run-deadline"),
                    candidate_id: id("candidate-hold"),
                    candidate_hash: hash("hash-hold"),
                    power_class: ThermalTuningPowerClass::Pps5a,
                    targets: &[target],
                    pps_ready: true,
                    thermal_model_available: true,
                    calibration_conflict: false,
                    candidate_profile: Some(profile_with_targets(&[target])),
                })
                .unwrap();
            let mut cooling = sample(0, ThermalTuningPhase::Cooldown, 3_400);
            cooling.target_c = target;
            cooling.heater_requested_output_permille = 0;
            cooling.heater_output_permille = 0;
            runtime.record_sample(cooling).unwrap();
            for (at_ms, phase, temp) in [
                (0, ThermalTuningPhase::Warmup, target * 100 - 4_000),
                (1_000, ThermalTuningPhase::Approach, target * 100 - 2_000),
                (
                    elapsed_ms - 110,
                    ThermalTuningPhase::Hold,
                    if window_start.is_some() {
                        target * 100 - 100
                    } else {
                        target * 100 - 2_000
                    },
                ),
            ] {
                let mut observed = sample(at_ms, phase, temp);
                observed.target_c = target;
                runtime.record_live_sample(observed).unwrap();
            }
            assert_eq!(runtime.stable_window_started_at_ms, window_start);
            // A second sample in the same phase establishes normal recording
            // cadence. The failure only 50ms later must bypass that cadence.
            let recent_temp = if window_start.is_some() {
                target * 100 - 100
            } else {
                temperature
            };
            let mut recent = sample(elapsed_ms - 50, ThermalTuningPhase::Hold, recent_temp);
            recent.target_c = target;
            runtime.record_live_sample(recent).unwrap();
            let mut observed = sample(elapsed_ms, ThermalTuningPhase::Hold, temperature);
            observed.target_c = target;
            runtime.record_live_sample(observed).unwrap();
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown, "{target}C");
            let event = runtime
                .events
                .iter()
                .find(|event| event.decision.is_some())
                .unwrap();
            assert_eq!(event.elapsed_ms, elapsed_ms);
            assert_eq!(
                event.decision.as_ref().unwrap().stop_reason,
                "full-speed-to-stable budget exhausted"
            );
        }
    }

    #[test]
    fn hold_confirm_settle_timeout_rejects_trial_and_cools_for_retry() {
        let mut runtime = started_single_target_runtime();
        runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Approach, 4_000))
            .unwrap();
        runtime
            .record_sample(sample(11_000, ThermalTuningPhase::Approach, 4_500))
            .unwrap();
        assert!(runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.trial_index, 1);
        assert!(runtime.events.iter().any(|event| {
            event.kind == ThermalTuningEventKind::Sample && event.elapsed_ms == 11_000
        }));
        assert!(runtime.events.iter().any(|event| {
            event.kind == ThermalTuningEventKind::Decision
                && event.decision.as_ref().is_some_and(|decision| {
                    decision.stop_reason == "full-speed-to-stable budget exhausted"
                })
        }));
    }

    #[test]
    fn hold_observation_starts_near_target_after_early_hold_phase() {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_600))
            .unwrap();
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_000))
            .unwrap();
        runtime
            .record_sample(sample(2_000, ThermalTuningPhase::Hold, 5_696))
            .unwrap();
        assert_eq!(runtime.first_hold_elapsed_ms, None);
        assert_eq!(runtime.hold_output_count, 1);
        assert_eq!(runtime.first_hold_temperature_centi_c, Some(5_696));
        assert_eq!(runtime.min_hold_temperature_centi_c, None);

        runtime
            .record_sample(sample(6_000, ThermalTuningPhase::Hold, 5_970))
            .unwrap();
        assert_eq!(runtime.first_hold_elapsed_ms, Some(6_000));
        record_dense_hold(&mut runtime, 6_100, 9_900, 5_970);
        record_dense_hold(&mut runtime, 10_000, 15_900, 6_131);
        record_current_trial_sample(&mut runtime, 16_000, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(runtime.trial_index, 0);
        record_dense_hold(&mut runtime, 16_100, 17_900, 6_000);
        record_current_trial_sample(&mut runtime, 18_000, ThermalTuningPhase::Hold, 6_000);
        assert!(runtime.events.iter().any(|event| {
            event.kind == ThermalTuningEventKind::Decision
                && event.decision.as_ref().is_some_and(|decision| {
                    decision.disposition.as_str() == "passed"
                        && decision.hold_peak_to_peak_centi_c == Some(161)
                })
        }));
    }

    #[test]
    fn hold_entry_request_is_retained_without_hiding_later_supply() {
        for (entry, later) in [(50, 0), (0, 50), (51, 0), (50, 50)] {
            let mut runtime = started_single_target_runtime();
            runtime
                .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_500))
                .unwrap();
            runtime
                .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_000))
                .unwrap();
            for (elapsed, temperature, output) in [
                (2_000, 5_953, entry),
                (2_200, 5_990, later),
                (2_400, 6_160, 0),
            ] {
                let mut observation = sample(elapsed, ThermalTuningPhase::Hold, temperature);
                observation.heater_requested_output_permille = output;
                runtime.record_sample(observation).unwrap();
            }
            let facts = runtime.candidate_failure_evidence(60, Some(160), Some(207), None, 10_000);
            assert_eq!(facts.hold_entry_output_permille, Some(entry));
            assert_eq!(facts.hold_sample_count, 3);
            assert_eq!(
                facts.hold_off_sample_count,
                u32::from(entry == 0) + u32::from(later == 0) + 1
            );
        }
    }

    #[test]
    fn warmup_requires_sustained_logical_and_physical_full_output() {
        let mut runtime = started_single_target_runtime();
        let mut soft_start = sample(0, ThermalTuningPhase::Warmup, 3_000);
        soft_start.heater_output_permille = 800;
        runtime.record_sample(soft_start).unwrap();
        assert!(!runtime.warmup_full_output);
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Warmup, 3_100))
            .unwrap();
        assert!(runtime.warmup_full_output);
        let mut bounded = sample(1_500, ThermalTuningPhase::Warmup, 3_200);
        bounded.heater_output_permille = 970;
        runtime.record_sample(bounded.clone()).unwrap();
        bounded.elapsed_ms = 3_500;
        runtime.record_sample(bounded.clone()).unwrap();
        assert!(!runtime.warmup_full_output_invalid);
        bounded.elapsed_ms = 3_551;
        runtime.record_sample(bounded).unwrap();
        assert!(runtime.warmup_full_output_invalid);
    }

    #[test]
    fn warmup_logical_drop_cannot_recover_after_full_output() {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        let mut reduced = sample(100, ThermalTuningPhase::Warmup, 3_100);
        reduced.heater_requested_output_permille = 994;
        runtime.record_sample(reduced).unwrap();
        runtime
            .record_sample(sample(200, ThermalTuningPhase::Warmup, 3_200))
            .unwrap();
        assert!(runtime.warmup_full_output_invalid);
    }

    #[test]
    fn strategy_statistics_ignore_pps_pwm_for_the_same_logical_requests() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[60]));
        for (elapsed_ms, phase, temp, requested, pwm) in [
            (1_000, ThermalTuningPhase::Approach, 5_300, 680, 710),
            (1_250, ThermalTuningPhase::Approach, 5_400, 760, 950),
            (1_500, ThermalTuningPhase::Hold, 6_000, 300, 600),
            (1_750, ThermalTuningPhase::Hold, 6_010, 400, 900),
        ] {
            let mut measured = sample(elapsed_ms, phase, temp);
            measured.heater_requested_output_permille = requested;
            measured.heater_output_permille = pwm;
            runtime.update_trial_metrics(&measured, 60);
        }
        let evidence = runtime.candidate_failure_evidence(60, Some(10), Some(10), None, 10_000);
        assert_eq!(evidence.approach_median_output_permille, Some(760));
        assert_eq!(evidence.hold_median_output_permille, Some(400));
        assert_eq!(evidence.hold_p90_output_permille, Some(400));
        assert_eq!(
            runtime.trial_score(60, false, Some(10), Some(10), None)[9],
            0.0
        );
    }

    #[test]
    fn eleven_component_score_uses_local_hold_and_approach_evidence() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.warmup_full_output = true;
        runtime.stable_window_verified_at_ms = Some(14_000);
        runtime.trial_start_elapsed_ms = Some(0);
        runtime.first_band_elapsed_ms = Some(5_200);
        runtime.hold_output_histogram[300] = 3;
        runtime.hold_output_histogram[400] = 1;
        runtime.hold_output_count = 4;
        runtime
            .approach_curve_samples
            .push(ThermalCurveSample {
                elapsed_ms: 1_000,
                temperature_centi_c: 5_000,
            })
            .unwrap();
        runtime.approach_curve_last_elapsed_ms = Some(6_000);
        runtime
            .approach_curve_samples
            .push(ThermalCurveSample {
                elapsed_ms: 6_000,
                temperature_centi_c: 6_000,
            })
            .unwrap();
        assert_eq!(
            runtime.trial_score(60, true, Some(200), Some(150), Some(4_000)),
            [
                0.0, 0.0, 0.0, 0.0, 0.0052, 0.0, 4_000.0, 2.0, 1.5, 100.0, 0.0
            ]
        );
        let low_margin = runtime.trial_score(60, true, Some(200), Some(150), Some(9_500));
        assert_eq!(low_margin[3], 1.0);
        assert_eq!(low_margin[4], 0.2552);
        assert_eq!(low_margin[5], 500.0);
    }

    #[test]
    fn failed_scout_compares_current_and_predicted_before_separate_hold_confirm() {
        let mut runtime = started_single_target_runtime();
        for (elapsed, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_000),
            (300, ThermalTuningPhase::Approach, 5_500),
            (600, ThermalTuningPhase::Hold, 6_400),
            (700, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        record_dense_hold(&mut runtime, 800, 12_500, 6_000);
        record_current_trial_sample(&mut runtime, 12_700, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::BatchPredicted
        );
        assert_eq!(runtime.trial_index, 1);
        assert!(runtime.batch_predicted_profile.is_some());

        record_current_trial_sample(&mut runtime, 13_000, ThermalTuningPhase::Cooldown, 3_400);
        for (elapsed, phase, temperature) in [
            (13_100, ThermalTuningPhase::Warmup, 3_000),
            (13_400, ThermalTuningPhase::Approach, 5_500),
            (13_700, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        record_dense_hold(&mut runtime, 13_800, 25_700, 6_000);
        record_current_trial_sample(&mut runtime, 25_700, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert_eq!(runtime.trial_index, 2);
        assert!(runtime.preview_update_pending);
        assert!(runtime.events.iter().any(|event| {
            event.reason.as_deref() == Some("candidate_batch_selected_predicted")
        }));

        record_current_trial_sample(&mut runtime, 26_000, ThermalTuningPhase::Cooldown, 3_400);
        for (elapsed, phase, temperature) in [
            (26_100, ThermalTuningPhase::Warmup, 3_000),
            (26_400, ThermalTuningPhase::Approach, 5_500),
            (26_700, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        record_dense_hold(&mut runtime, 26_800, 38_800, 6_000);
        record_current_trial_sample(&mut runtime, 38_800, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert_eq!(runtime.trial_index, 2);
        assert_eq!(runtime.accepted_target_mask, 0);
        assert_eq!(runtime.seed_source.as_str(), "batch_selected_predicted");
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Running
        );
        record_dense_hold(&mut runtime, 38_900, 86_700, 6_000);
        record_current_trial_sample(&mut runtime, 86_700, ThermalTuningPhase::Hold, 6_000);
        assert_ne!(runtime.accepted_target_mask, 0);
    }

    #[test]
    fn captured_failed_batch_winner_must_change_before_another_scout() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_failed_batch.fixture.json")).unwrap();
        for predicted_wins in [true, false] {
            let mut runtime = started_single_target_runtime();
            let mut current = runtime.candidate_profile.unwrap();
            let point = current.points[0].as_mut().unwrap();
            point.brake_distance_centi_c = 1_259;
            point.approach_damping_exponent_permille = 1_000;
            point.approach_lead_ticks = 0;
            point.approach_power_permille = 1_000;
            point.approach_floor_power_permille = 50;
            point.approach_tail_window_centi_c = 0;
            point.hold_power_permille = 56;
            point.hold_reheat_power_permille = 56;
            point.hold_kp_permille_per_c = 43;
            point.hold_ki_permille_per_c_tick = 1;
            point.hold_blend_ticks = 2;
            point.hold_lead_ticks = 2;
            let mut predicted = current;
            predicted.points[0]
                .as_mut()
                .unwrap()
                .approach_damping_exponent_permille = 1_500;
            runtime.candidate_profile = Some(current);
            runtime.batch_current_profile = Some(current);
            runtime.batch_predicted_profile = Some(predicted);
            runtime.trial_purpose = ThermalTuningTrialPurpose::BatchCurrent;
            runtime.trial_index = 4;
            for (index, trial) in fixture["trials"].as_array().unwrap().iter().enumerate() {
                for row in trial["samples"].as_array().unwrap() {
                    let mut observed = sample(
                        row[0].as_u64().unwrap(),
                        serde_json::from_value(row[2].clone()).unwrap(),
                        row[1].as_i64().unwrap() as i16,
                    );
                    observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
                    observed.heater_output_permille = observed.heater_requested_output_permille;
                    runtime.update_trial_metrics(&observed, 60);
                }
                let decision = &trial["decision"];
                let mut score: [f64; 11] =
                    serde_json::from_value(decision["score"].clone()).unwrap();
                // Exercise the other winner's cache without changing physical
                // observations, failure facts or either qualification result.
                if !predicted_wins && index == 0 {
                    score = [0.0; 11];
                }
                runtime
                    .advance_after_trial(
                        60,
                        TrialEvaluation {
                            score,
                            passes: false,
                            warmup_full: true,
                            max_overshoot_centi_c: Some(
                                decision["maxOvershootCentiC"].as_u64().unwrap() as u16,
                            ),
                            hold_peak_to_peak_centi_c: Some(
                                decision["holdPeakToPeakCentiC"].as_u64().unwrap() as u16,
                            ),
                            settle_time_ms: None,
                            full_speed_limit_ms: 10_000,
                        },
                    )
                    .unwrap();
            }
            let next = runtime.candidate_profile.unwrap().points[0].unwrap();
            assert_eq!(runtime.trial_purpose, ThermalTuningTrialPurpose::Scout);
            assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
            assert_eq!(runtime.accepted_target_mask, 0);
            // The predicted prefix is unpowered; the current prefix has only
            // 17/19 zero Hold samples and cannot use the 90% entrance exception.
            // Each winner must retain its own braking or damping evidence.
            assert_eq!(
                next.approach_damping_exponent_permille,
                if predicted_wins { 1_500 } else { 2_500 }
            );
            assert_eq!(
                next.brake_distance_centi_c,
                if predicted_wins { 1_371 } else { 1_259 }
            );
            assert_eq!(next.hold_power_permille, 56);
            assert_eq!(next.hold_reheat_power_permille, 56);
            assert!(runtime.preview_update_pending);
            assert!(runtime.preview_receipt.is_none());
            assert_eq!(runtime.trial_index, 6);
            assert!(runtime.batch_current_failure_evidence.is_none());
            assert!(runtime.batch_current_profile.is_none());
        }
    }

    #[test]
    fn candidate_scores_rank_lexicographically() {
        let mut current = [0.0; 11];
        let mut predicted = [0.0; 11];
        current[2] = 1.0;
        predicted[2] = 0.0;
        predicted[10] = 1.0e12;
        assert!(score_precedes(&predicted, &current));
        assert!(!score_precedes(&current, &predicted));
    }

    #[test]
    fn failed_rank_does_not_trade_lower_excursion_for_six_ms_sample_jitter() {
        // Real 39d62c7 trial 4/5: the better trace loses six milliseconds
        // at sampled band endpoints, well below the 200ms recording cadence.
        let mut scores = std::vec::Vec::new();
        for (span, excursion, overshoot, p2p) in [(3_644, 25, 175, 215), (3_638, 10, 160, 203)] {
            let mut runtime = started_single_target_runtime();
            runtime.first_band_elapsed_ms = Some(1_000);
            runtime.first_band_exit_centi_c = Some(6_150 + excursion as i16);
            runtime.longest_stable_span_ms = span;
            runtime.max_band_excursion_centi_c = excursion;
            scores.push(runtime.trial_score(60, false, Some(overshoot), Some(p2p), None));
            assert!(runtime.stable_window_verified_at_ms.is_none());
        }
        assert_eq!(scores[0][5], scores[1][5]);
        assert!(score_precedes(&scores[1], &scores[0]));
    }

    #[test]
    fn failed_rank_prefers_lower_excursion_over_fast_unstable_band_entry() {
        // 8f32f34 trial 4/5: stronger damping reaches the band later,
        // so a truncated partial span must not reward the hotter point.
        let mut scores = std::vec::Vec::new();
        for (span, excursion, overshoot, p2p) in [(4_175, 15, 165, 211), (3_658, 6, 156, 202)] {
            let mut runtime = started_single_target_runtime();
            runtime.first_band_elapsed_ms = Some(1_000);
            runtime.first_band_exit_centi_c = Some(6_150 + excursion as i16);
            runtime.longest_stable_span_ms = span;
            runtime.max_band_excursion_centi_c = excursion;
            scores.push(runtime.trial_score(60, false, Some(overshoot), Some(p2p), None));
            assert!(runtime.stable_window_verified_at_ms.is_none());
        }
        assert!(score_precedes(&scores[1], &scores[0]));
        assert!(!score_precedes(&scores[0], &scores[1]));
    }

    #[test]
    fn captured_failed_rank_preserves_full_warmup_before_thermal_progress() {
        // 9485eb63 A60 trials 2/3: a 0.01C smaller peak cannot outweigh
        // the predicted trial's missing full-output Warmup witness.
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_warmup_search.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
        assert!(runtime.warmup_full_output && !runtime.warmup_full_output_invalid);
        let current_score = runtime.trial_score(60, false, Some(158), Some(161), None);
        replay_damping_bracket_trial(&mut runtime, &fixture["trials"][1]);
        assert!(!runtime.warmup_full_output);
        let predicted_score = runtime.trial_score(60, false, Some(157), Some(179), None);
        assert!(score_precedes(&current_score, &predicted_score));
        runtime.warmup_full_output = true;
        runtime.warmup_full_output_invalid = true;
        assert!(score_precedes(
            &current_score,
            &runtime.trial_score(60, false, Some(157), Some(179), None)
        ));
    }

    #[test]
    fn captured_start_gap_failure_uses_another_effective_search_channel() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_warmup_search.fixture.json"))
                .unwrap();
        let mut runtime = started_single_target_runtime();
        let evaluation = replay_damping_bracket_trial(&mut runtime, &fixture["trials"][0]);
        let original = runtime.candidate_profile.unwrap();
        runtime.advance_after_trial(60, evaluation).unwrap();
        let next = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(next.brake_distance_centi_c < 2_500);
        assert!(!same_effective_search_point(
            original.points[0].unwrap(),
            next
        ));
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert!(runtime.armed);
        assert!(!runtime.events.iter().any(|event| event.reason.as_deref()
            == Some("candidate_search_exhausted_no_distinct_safe_proposal")));
    }

    #[test]
    fn search_rejects_a_brake_that_covers_the_allowed_start_gap() {
        let mut runtime = started_single_target_runtime();
        let mut original = runtime.candidate_profile.unwrap();
        original.points[0].as_mut().unwrap().brake_distance_centi_c = 2_452;
        let mut proposed = original;
        proposed.points[0].as_mut().unwrap().brake_distance_centi_c = 2_532;
        runtime.candidate_profile = Some(proposed);
        assert!(!runtime.record_fresh_proposal(Some(original)));
        assert!(runtime.proposed_points.is_empty());
        proposed = original;
        proposed.points[0]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille += 100;
        runtime.candidate_profile = Some(proposed);
        assert!(runtime.record_fresh_proposal(Some(original)));
    }

    #[test]
    fn unverified_band_restart_cannot_improve_the_timing_gate_score() {
        let mut runtime = started_single_target_runtime();
        runtime.first_band_elapsed_ms = Some(1_000);
        runtime.max_band_excursion_centi_c = 2;
        runtime.longest_stable_span_ms = 3_738;
        let broken = runtime.trial_score(60, false, Some(152), Some(210), None);
        let just_restarted = runtime.trial_score(60, false, Some(152), Some(210), Some(10_010));
        assert_eq!(broken[2], just_restarted[2]);
        assert!(runtime.stable_window_verified_at_ms.is_none());
    }

    #[test]
    fn captured_failed_rank_prefers_the_longer_calibration_only_window() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_failed_rank.fixture.json")).unwrap();
        let mut scores = std::vec::Vec::new();
        for trial in fixture["trials"].as_array().unwrap() {
            let mut runtime = started_single_target_runtime();
            runtime.warmup_exited_at_ms = trial["warmupExitMs"].as_u64();
            runtime.trial_start_elapsed_ms = runtime.warmup_exited_at_ms;
            for row in trial["samples"].as_array().unwrap() {
                let observed = sample(
                    row[0].as_u64().unwrap(),
                    serde_json::from_value(row[2].clone()).unwrap(),
                    row[1].as_i64().unwrap() as i16,
                );
                runtime.update_stable_window(&observed, 60);
                runtime.update_stability_evidence(&observed, 60);
            }
            assert!(runtime.stable_window_verified_at_ms.is_none());
            let score = runtime.trial_score(
                60,
                false,
                Some(trial["overshootCentiC"].as_u64().unwrap() as u16),
                Some(trial["holdP2pCentiC"].as_u64().unwrap() as u16),
                None,
            );
            assert!(score[1] > 0.0);
            scores.push(score);
        }
        assert!(score_precedes(&scores[1], &scores[0]));
    }

    #[test]
    fn captured_eight_target_a60_coast_corrects_warmup_energy_first() {
        // firmware-pps5a-1791463860752, trial 0: Approach requests only
        // 50/70 permille, followed by a completely unpowered Hold rising
        // from 60.21 to 66.05 C. Reducing Approach damping alone cannot
        // remove the heat already deposited during Warmup.
        let mut runtime = started_single_target_runtime();
        let mut profile = runtime.candidate_profile.unwrap();
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 780;
        point.approach_damping_exponent_permille = 1_000;
        point.approach_lead_ticks = 0;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 50;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 56;
        point.hold_reheat_power_permille = 56;
        point.hold_kp_permille_per_c = 43;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 2;
        point.hold_lead_ticks = 2;
        let previous = *point;
        runtime.candidate_profile = Some(profile);
        for (elapsed, temperature, phase, requested) in [
            (17_504, 5_251, ThermalTuningPhase::Approach, 50),
            (19_281, 5_750, ThermalTuningPhase::Approach, 50),
            (19_517, 5_813, ThermalTuningPhase::Approach, 70),
            (19_719, 5_834, ThermalTuningPhase::Approach, 70),
            (19_946, 5_851, ThermalTuningPhase::Approach, 50),
            (20_870, 5_992, ThermalTuningPhase::Approach, 50),
            (20_931, 6_021, ThermalTuningPhase::Hold, 0),
            (21_267, 6_080, ThermalTuningPhase::Hold, 0),
            (22_419, 6_172, ThermalTuningPhase::Hold, 0),
            (27_049, 6_605, ThermalTuningPhase::Hold, 0),
        ] {
            let mut observed = sample(elapsed, phase, temperature);
            observed.heater_requested_output_permille = requested;
            observed.heater_output_permille = requested;
            runtime.update_trial_metrics(&observed, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(605), Some(585), None, 10_000);
        assert!(!facts.approach_floor_only);
        assert_eq!(facts.approach_output_range, Some((50, 70)));
        assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
        let proposed = tune_point(previous, facts);
        assert!(
            proposed.brake_distance_centi_c > previous.brake_distance_centi_c,
            "coasting after low Approach power must reduce stored Warmup energy: {proposed:?}"
        );
        assert_eq!(proposed.brake_distance_centi_c, 1_780);
        assert_eq!(
            proposed,
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 1_780,
                ..previous
            }
        );
        assert_eq!(reseed_after_hold_confirm(previous, facts), proposed);
        let mut heated = facts;
        heated.approach_output_range = Some((50, 1_000));
        heated.approach_median_output_permille = Some(500);
        assert_eq!(tune_point(previous, heated), proposed);
        heated.overshoot_c = 2.0;
        assert_ne!(tune_point(previous, heated).brake_distance_centi_c, 1_780);
        let mut protected = facts;
        protected.approach_output_range = Some((0, 70));
        assert_ne!(tune_point(previous, protected), proposed);
        runtime.reset_trial_metrics();
        assert!(runtime.model_approach_output_range.is_none());
    }

    #[test]
    fn captured_a60_small_hold_pulses_select_effective_approach_damping() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_limited_hold.fixture.json")).unwrap();
        let mut runtime = started_single_target_runtime();
        let mut profile = runtime.candidate_profile.unwrap();
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 1_243;
        point.approach_damping_exponent_permille = 1_000;
        point.approach_lead_ticks = 0;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 50;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 56;
        point.hold_reheat_power_permille = 56;
        point.hold_kp_permille_per_c = 43;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 2;
        point.hold_lead_ticks = 2;
        let previous = *point;
        runtime.candidate_profile = Some(profile);
        for row in fixture["samples"].as_array().unwrap() {
            let mut observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = observed.heater_requested_output_permille;
            runtime.update_trial_metrics(&observed, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(262), Some(311), None, 10_000);
        assert!(!facts.hold_entry_pulse_only);
        assert_eq!(facts.hold_sample_count, 22);
        assert_eq!(facts.hold_off_sample_count, 20);
        assert_eq!(facts.hold_max_output_permille, Some(60));
        assert_eq!(facts.hold_max_below_c, Some(0.49));
        let expected = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_120,
            ..previous
        };
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    }

    #[test]
    fn captured_a60_brief_hold_entry_power_still_corrects_warmup_energy() {
        // firmware-pps5a-1791468629135 trial 0: two 6% readings 60 ms
        // apart at Hold entry, then no power throughout the residual rise.
        let mut runtime = started_single_target_runtime();
        let mut profile = runtime.candidate_profile.unwrap();
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 780;
        point.approach_damping_exponent_permille = 1_000;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 50;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 56;
        point.hold_reheat_power_permille = 56;
        point.hold_kp_permille_per_c = 43;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 2;
        point.hold_lead_ticks = 2;
        let previous = *point;
        runtime.candidate_profile = Some(profile);
        for (elapsed, temperature, phase, requested) in [
            (17_742, 5_223, ThermalTuningPhase::Approach, 50),
            (19_430, 5_733, ThermalTuningPhase::Approach, 50),
            (20_735, 5_945, ThermalTuningPhase::Approach, 50),
            (20_798, 5_963, ThermalTuningPhase::Hold, 60),
            (20_858, 5_960, ThermalTuningPhase::Hold, 60),
            (21_123, 6_009, ThermalTuningPhase::Hold, 0),
            (21_323, 6_071, ThermalTuningPhase::Hold, 0),
            (21_576, 6_106, ThermalTuningPhase::Hold, 0),
            (21_782, 6_124, ThermalTuningPhase::Hold, 0),
            (22_031, 6_144, ThermalTuningPhase::Hold, 0),
            (22_286, 6_181, ThermalTuningPhase::Hold, 0),
            (22_521, 6_192, ThermalTuningPhase::Hold, 0),
            (22_774, 6_206, ThermalTuningPhase::Hold, 0),
            (22_982, 6_224, ThermalTuningPhase::Hold, 0),
            (23_234, 6_245, ThermalTuningPhase::Hold, 0),
            (23_446, 6_283, ThermalTuningPhase::Hold, 0),
            (23_698, 6_321, ThermalTuningPhase::Hold, 0),
            (23_915, 6_341, ThermalTuningPhase::Hold, 0),
            (24_153, 6_331, ThermalTuningPhase::Hold, 0),
            (24_365, 6_409, ThermalTuningPhase::Hold, 0),
            (24_617, 6_429, ThermalTuningPhase::Hold, 0),
            (24_871, 6_442, ThermalTuningPhase::Hold, 0),
            (25_133, 6_461, ThermalTuningPhase::Hold, 0),
            (25_395, 6_475, ThermalTuningPhase::Hold, 0),
            (25_647, 6_506, ThermalTuningPhase::Hold, 0),
            (25_913, 6_496, ThermalTuningPhase::Hold, 0),
            (26_114, 6_533, ThermalTuningPhase::Hold, 0),
            (26_378, 6_560, ThermalTuningPhase::Hold, 0),
            (26_631, 6_546, ThermalTuningPhase::Hold, 0),
            (26_844, 6_553, ThermalTuningPhase::Hold, 0),
            (27_098, 6_572, ThermalTuningPhase::Hold, 0),
            (27_307, 6_564, ThermalTuningPhase::Hold, 0),
            (27_507, 6_552, ThermalTuningPhase::Hold, 0),
            (27_771, 6_576, ThermalTuningPhase::Hold, 0),
        ] {
            let mut observed = sample(elapsed, phase, temperature);
            observed.heater_requested_output_permille = requested;
            observed.heater_output_permille = requested;
            runtime.update_trial_metrics(&observed, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(576), Some(616), None, 10_000);
        assert!(facts.hold_entry_pulse_only);
        let higher_approach = TuneEvidence {
            approach_floor_only: false,
            approach_output_range: Some((50, 700)),
            approach_median_output_permille: Some(700),
            ..facts
        };
        assert_eq!(
            tune_point(previous, higher_approach),
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 1_780,
                ..previous
            }
        );
        let proposed = tune_point(previous, facts);
        assert_eq!(
            proposed,
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 1_780,
                ..previous
            }
        );
        let mut later_reheat = sample(28_000, ThermalTuningPhase::Hold, 6_570);
        later_reheat.heater_requested_output_permille = 60;
        later_reheat.heater_output_permille = 60;
        runtime.update_trial_metrics(&later_reheat, 60);
        let reheated = runtime.candidate_failure_evidence(60, Some(576), Some(616), None, 10_000);
        assert!(!reheated.hold_entry_pulse_only);
        assert_ne!(tune_point(previous, reheated), proposed);
    }

    #[test]
    fn captured_a60_approach_band_jitter_does_not_invent_a_hold_deficit() {
        // firmware-pps5a-1791470799796 trial 3: the first band exit is
        // 58.48 C in Approach, but every physical Hold request is zero
        // while temperature rises to 62.61 C. More reheat is the wrong sign.
        let mut runtime = started_single_target_runtime();
        let mut profile = runtime.candidate_profile.unwrap();
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 1_280;
        point.approach_damping_exponent_permille = 1_000;
        point.approach_lead_ticks = 0;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 50;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 56;
        point.hold_reheat_power_permille = 56;
        point.hold_kp_permille_per_c = 43;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_on_centi_c = 30;
        point.hold_blend_ticks = 2;
        point.hold_lead_ticks = 2;
        let previous = *point;
        runtime.candidate_profile = Some(profile);
        for (elapsed, temperature, phase, requested) in [
            (413_103, 4_726, ThermalTuningPhase::Approach, 50),
            (413_164, 4_734, ThermalTuningPhase::Approach, 50),
            (413_379, 4_801, ThermalTuningPhase::Approach, 50),
            (415_026, 5_223, ThermalTuningPhase::Approach, 940),
            (415_250, 5_292, ThermalTuningPhase::Approach, 940),
            (415_452, 5_324, ThermalTuningPhase::Approach, 940),
            (415_700, 5_412, ThermalTuningPhase::Approach, 490),
            (415_933, 5_453, ThermalTuningPhase::Approach, 490),
            (416_137, 5_490, ThermalTuningPhase::Approach, 280),
            (416_343, 5_537, ThermalTuningPhase::Approach, 200),
            (416_578, 5_579, ThermalTuningPhase::Approach, 110),
            (416_819, 5_626, ThermalTuningPhase::Approach, 60),
            (417_087, 5_668, ThermalTuningPhase::Approach, 60),
            (417_329, 5_731, ThermalTuningPhase::Approach, 140),
            (417_592, 5_765, ThermalTuningPhase::Approach, 50),
            (417_833, 5_825, ThermalTuningPhase::Approach, 50),
            (418_043, 5_857, ThermalTuningPhase::Approach, 90),
            (418_265, 5_848, ThermalTuningPhase::Approach, 90),
            (418_520, 5_892, ThermalTuningPhase::Approach, 50),
            (418_779, 5_898, ThermalTuningPhase::Approach, 50),
            (418_987, 5_930, ThermalTuningPhase::Approach, 50),
            (419_117, 5_966, ThermalTuningPhase::Hold, 0),
            (419_194, 5_964, ThermalTuningPhase::Hold, 0),
            (419_413, 5_968, ThermalTuningPhase::Hold, 0),
            (419_656, 6_012, ThermalTuningPhase::Hold, 0),
            (419_868, 6_039, ThermalTuningPhase::Hold, 0),
            (420_113, 6_075, ThermalTuningPhase::Hold, 0),
            (420_345, 6_094, ThermalTuningPhase::Hold, 0),
            (420_590, 6_114, ThermalTuningPhase::Hold, 0),
            (420_802, 6_126, ThermalTuningPhase::Hold, 0),
            (421_043, 6_122, ThermalTuningPhase::Hold, 0),
            (421_289, 6_142, ThermalTuningPhase::Hold, 0),
            (421_501, 6_160, ThermalTuningPhase::Hold, 0),
            (421_747, 6_158, ThermalTuningPhase::Hold, 0),
            (421_947, 6_164, ThermalTuningPhase::Hold, 0),
            (422_182, 6_186, ThermalTuningPhase::Hold, 0),
            (422_422, 6_179, ThermalTuningPhase::Hold, 0),
            (422_701, 6_193, ThermalTuningPhase::Hold, 0),
            (422_941, 6_197, ThermalTuningPhase::Hold, 0),
            (423_174, 6_261, ThermalTuningPhase::Hold, 0),
        ] {
            let mut observed = sample(elapsed, phase, temperature);
            observed.heater_requested_output_permille = requested;
            observed.heater_output_permille = requested;
            runtime.update_trial_metrics(&observed, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(261), Some(297), None, 10_000);
        assert_eq!(facts.first_band_exit_centi_c, Some(5_848));
        assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
        assert_eq!(facts.hold_max_below_c, Some(0.36));
        assert_eq!(facts.over_c, 2.61);
        let expected = ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_110,
            ..previous
        };
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
        for attempt in 0..8 {
            let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
            assert_eq!(alternative.hold_reheat_power_permille, 56);
            assert_eq!(alternative.hold_kp_permille_per_c, 43);
            assert!(
                alternative.approach_damping_exponent_permille > 1_000
                    || alternative.brake_distance_centi_c > 1_280
            );
        }
        // A genuinely cold physical Hold must retain the opposite feedback.
        let cold_hold = TuneEvidence {
            hold_max_below_c: Some(2.0),
            under_c: 2.0,
            over_c: 0.0,
            overshoot_c: 0.0,
            ..facts
        };
        let recovery = tune_point(previous, cold_hold);
        assert_eq!(recovery.hold_reheat_power_permille, 136);
        assert_eq!(recovery.approach_damping_exponent_permille, 1_000);
    }

    #[test]
    fn captured_floor_only_coast_search_changes_braking_not_unused_damping() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_floor_coast.fixture.json")).unwrap();
        let mut runtime = started_single_target_runtime();
        let mut profile = runtime.candidate_profile.unwrap();
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 1_318;
        point.approach_damping_exponent_permille = 1_050;
        point.approach_lead_ticks = 1;
        point.approach_floor_power_permille = 100;
        point.approach_power_permille = 920;
        point.hold_power_permille = 54;
        point.hold_reheat_power_permille = 54;
        point.hold_kp_permille_per_c = 23;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 3;
        point.hold_lead_ticks = 3;
        point.hold_on_centi_c = 30;
        point.hold_off_centi_c = 5;
        point.hold_entry_centi_c = 90;
        point.hold_exit_centi_c = 200;
        point.overshoot_cutoff_centi_c = 80;
        point.approach_tail_window_centi_c = 0;
        point.warmup_power_permille = 1_000;
        point.warmup_reenter_centi_c = 400;
        let previous = *point;
        runtime.candidate_profile = Some(profile);
        for row in fixture["samples"].as_array().unwrap() {
            let mut observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
            runtime.update_trial_metrics(&observed, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(170), Some(212), None, 10_000);
        assert_eq!(facts.hold_sample_count, 15);
        assert_eq!(facts.hold_off_sample_count, 15);
        assert!(facts.first_band_exit_centi_c.is_some_and(|t| t > 6_150));
        assert!(!facts.stable_window_verified);
        assert!(facts.approach_floor_only);
        let proposed = tune_point(previous, facts);
        assert!(proposed.brake_distance_centi_c > previous.brake_distance_centi_c);
        assert_eq!(
            proposed,
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: proposed.brake_distance_centi_c,
                ..previous
            }
        );
        assert_eq!(reseed_after_hold_confirm(previous, facts), proposed);
    }

    #[test]
    fn floor_only_evidence_requires_actual_approach_and_resets_per_trial() {
        for nonfloor in [0, 51, 1_000] {
            let mut runtime = started_single_target_runtime();
            let mut warmup = sample(0, ThermalTuningPhase::Warmup, 3_500);
            warmup.heater_requested_output_permille = 1_000;
            runtime.update_trial_metrics(&warmup, 60);
            for elapsed in [200, 400, 600] {
                let mut observed = sample(elapsed, ThermalTuningPhase::Approach, 5_000);
                observed.heater_requested_output_permille = 50;
                runtime.update_trial_metrics(&observed, 60);
                let facts = runtime.candidate_failure_evidence(60, None, None, None, 10_000);
                assert_eq!(facts.approach_floor_only, elapsed == 600);
            }
            let mut different = sample(800, ThermalTuningPhase::Approach, 5_050);
            different.heater_requested_output_permille = nonfloor;
            runtime.update_trial_metrics(&different, 60);
            assert!(
                !runtime
                    .candidate_failure_evidence(60, None, None, None, 10_000)
                    .approach_floor_only
            );
            runtime.reset_trial_metrics();
            assert!(
                !runtime
                    .candidate_failure_evidence(60, None, None, None, 10_000)
                    .approach_floor_only
            );
        }
    }

    #[test]
    fn repeated_high_side_proposal_searches_remaining_effective_parameters() {
        let mut runtime = started_single_target_runtime();
        runtime.first_band_elapsed_ms = Some(5_000);
        runtime.first_band_exit_centi_c = Some(6_159);
        runtime.min_controller_hold_temperature_centi_c = Some(5_950);
        runtime.max_controller_hold_temperature_centi_c = Some(6_159);
        runtime.first_hold_temperature_centi_c = Some(5_950);
        runtime.max_temperature_centi_c = Some(6_159);
        runtime.hold_output_count = 10;
        let original = runtime.candidate_profile;
        runtime.mutate_candidate_for_failure(60, Some(159), Some(209), None, 10_000);
        assert!(runtime.record_fresh_proposal(original));
        let already_proposed = runtime.candidate_profile;
        runtime.candidate_profile = original;
        let score = runtime.trial_score(60, false, Some(159), Some(209), None);
        let evidence = runtime.candidate_failure_evidence(60, Some(159), Some(209), None, 10_000);
        runtime
            .prepare_candidate_batch(60, score, evidence)
            .unwrap();
        assert!(runtime.armed);
        assert_ne!(runtime.batch_predicted_profile, already_proposed);
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::BatchPredicted
        );
    }

    #[test]
    fn failed_window_progress_rejects_sparse_or_interrupted_samples() {
        for interval in [500, 1_100] {
            let mut runtime = started_single_target_runtime();
            runtime.warmup_exited_at_ms = Some(0);
            for elapsed in (0..=9_900).step_by(interval) {
                runtime.update_stable_window(&sample(elapsed, ThermalTuningPhase::Hold, 6_000), 60);
            }
            assert_eq!(runtime.longest_stable_span_ms, 0);
            assert!(runtime.stable_window_verified_at_ms.is_none());
        }
        let mut runtime = started_single_target_runtime();
        runtime.warmup_exited_at_ms = Some(0);
        for elapsed in [0, 300, 600, 900, 2_001, 2_301, 2_601] {
            runtime.update_stable_window(&sample(elapsed, ThermalTuningPhase::Hold, 6_000), 60);
        }
        assert_eq!(runtime.longest_stable_span_ms, 900);
    }

    #[test]
    fn missing_240c_hold_searches_braking_instead_of_unused_hold_settings() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.targets_c.push(240).unwrap();
        let mut original = profile_with_targets(&[240]);
        let point = original.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 150;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 1_000;
        point.approach_damping_exponent_permille = 100;
        point.hold_power_permille = 838;
        point.hold_reheat_power_permille = 838;
        point.overshoot_cutoff_centi_c = 300;
        original.sanitize();
        runtime.candidate_profile = Some(original);
        runtime.candidate_hash = hash_profile(&original);
        runtime.deadline_nearest_temperature_centi_c = Some(23_700);
        runtime.approach_output_count = 8;
        runtime.approach_output_histogram[1_000] = 8;
        runtime.approach_slopes_c_per_s.push(0.2).unwrap();
        assert!(runtime.prepare_fresh_failure_proposal(240, Some(0), None, None, 5_000));
        let before = original.points[0].unwrap();
        let after = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(after.brake_distance_centi_c < before.brake_distance_centi_c);
        assert_eq!(after.hold_power_permille, before.hold_power_permille);
        assert_eq!(
            after.hold_reheat_power_permille,
            before.hold_reheat_power_permille
        );
        assert_eq!(after.hold_kp_permille_per_c, before.hold_kp_permille_per_c);
        assert!(runtime.events.iter().any(|event| event.reason.as_deref()
            == Some("repeated_proposal_replaced_by_distinct_effective_point")));
    }

    #[test]
    fn captured_model_floor_handoff_recovers_hold_without_legacy_clamps() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_model_hold.fixture.json")).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[240]);
        let wire: ThermalTuningCandidatePoint =
            serde_json::from_value(fixture["point"].clone()).unwrap();
        let previous = ThermalControlProfilePointConfig {
            target_temp_c: wire.target_c,
            brake_distance_centi_c: wire.brake_distance_centi_c,
            warmup_power_permille: wire.warmup_power_permille,
            approach_power_permille: wire.approach_power_permille,
            approach_floor_power_permille: wire.approach_floor_power_permille,
            approach_damping_exponent_permille: wire.approach_damping_exponent_permille,
            approach_tail_window_centi_c: wire.approach_tail_window_centi_c,
            hold_power_permille: wire.hold_power_permille,
            hold_on_centi_c: wire.hold_on_centi_c,
            hold_off_centi_c: wire.hold_off_centi_c,
            hold_kp_permille_per_c: wire.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: wire.hold_ki_permille_per_c_tick,
            hold_reheat_power_permille: wire.hold_reheat_power_permille,
            hold_entry_centi_c: wire.hold_entry_centi_c,
            hold_exit_centi_c: wire.hold_exit_centi_c,
            warmup_reenter_centi_c: wire.warmup_reenter_centi_c,
            approach_lead_ticks: wire.approach_lead_ticks,
            hold_lead_ticks: wire.hold_lead_ticks,
            hold_blend_ticks: wire.hold_blend_ticks,
            overshoot_cutoff_centi_c: wire.overshoot_cutoff_centi_c,
        };
        profile.points[0] = Some(previous);
        profile.sanitize();
        runtime.candidate_profile = Some(profile);
        runtime.trial_start_elapsed_ms = fixture["trialStartedAtMs"].as_u64();
        runtime.warmup_exited_at_ms = fixture["warmupExitedAtMs"].as_u64();
        runtime.warmup_full_output = true;
        for row in fixture["samples"].as_array().unwrap() {
            let mut observed = sample(
                row[0].as_u64().unwrap(),
                serde_json::from_value(row[2].clone()).unwrap(),
                row[1].as_i64().unwrap() as i16,
            );
            observed.target_c = 240;
            observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
            observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
            runtime.current_elapsed_ms = observed.elapsed_ms;
            runtime.update_trial_metrics(&observed, 240);
        }
        let facts = runtime.candidate_failure_evidence(240, Some(16), Some(186), None, 5_000);
        assert!(facts.approach_floor_only);
        assert_eq!(facts.approach_median_output_permille, Some(50));
        assert!(facts.first_hold_after_warmup_ms.unwrap() <= 5_000);
        assert_eq!(facts.first_band_exit_centi_c, Some(23_830));
        assert!(runtime.stable_window_verified_at_ms.is_none());
        assert_eq!(runtime.accepted_target_mask, 0);
        let mut expected = previous;
        expected.hold_reheat_power_permille = 913;
        expected.hold_on_centi_c = 25;
        expected.hold_lead_ticks = 2;
        expected.hold_blend_ticks = 1;
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    }

    #[test]
    fn captured_same_run_hold_failure_survives_a_pre_hold_retry() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_same_run_hold.fixture.json"))
                .unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime.targets_c.push(240).unwrap();
        runtime.armed = true;
        let mut original = profile_with_targets(&[240]);
        let point = original.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 110;
        point.approach_power_permille = 1_000;
        point.approach_floor_power_permille = 833;
        point.approach_damping_exponent_permille = 100;
        point.approach_tail_window_centi_c = 0;
        point.hold_power_permille = 833;
        point.hold_reheat_power_permille = 833;
        point.hold_kp_permille_per_c = 38;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_blend_ticks = 3;
        point.hold_lead_ticks = 3;
        original.sanitize();
        let previous = original.points[0].unwrap();
        for trial in fixture["trials"].as_array().unwrap() {
            runtime.reset_trial_metrics();
            runtime.candidate_profile = Some(original);
            runtime.candidate_hash = hash_profile(&original);
            runtime.trial_index = trial["trialIndex"].as_u64().unwrap() as u8;
            runtime.trial_purpose = ThermalTuningTrialPurpose::Scout;
            runtime.trial_start_elapsed_ms = trial["trialStartedAtMs"].as_u64();
            runtime.warmup_exited_at_ms = trial["warmupExitedAtMs"].as_u64();
            runtime.target_started_elapsed_ms = runtime.trial_start_elapsed_ms.unwrap();
            runtime.warmup_full_output = true;
            for row in trial["samples"].as_array().unwrap() {
                let mut observed = sample(
                    row[0].as_u64().unwrap(),
                    serde_json::from_value(row[2].clone()).unwrap(),
                    row[1].as_i64().unwrap() as i16,
                );
                observed.target_c = 240;
                observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
                observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
                runtime.current_elapsed_ms = observed.elapsed_ms;
                runtime.update_trial_metrics(&observed, 240);
            }
            assert!(runtime.stable_window_verified_at_ms.is_none());
            if runtime.trial_index == 5 {
                assert!(runtime.hold_output_count >= 3);
                assert_eq!(runtime.first_band_exit_centi_c, Some(23_842));
                runtime.finish_trial(runtime.current_elapsed_ms).unwrap();
                assert_eq!(runtime.accepted_target_mask, 0);
            } else {
                assert_eq!(runtime.hold_output_count, 0);
                runtime.proposed_points.clear();
                runtime.proposed_points.push(previous).unwrap();
                let mut tried = previous;
                tried.brake_distance_centi_c = 100;
                runtime.proposed_points.push(tried).unwrap();
                assert!(runtime.prepare_fresh_failure_proposal(240, Some(0), None, None, 5_000));
                let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
                assert_eq!(
                    proposed.brake_distance_centi_c,
                    previous.brake_distance_centi_c
                );
                assert_eq!(proposed.approach_damping_exponent_permille, 100);
                assert!(proposed.hold_reheat_power_permille > previous.hold_reheat_power_permille);
                assert_eq!(runtime.hold_output_count, 0);
                assert!(runtime.stable_window_verified_at_ms.is_none());
                assert_eq!(runtime.accepted_target_mask, 0);
                assert!(runtime.events.iter().any(|event| event.reason.as_deref()
                    == Some("same_run_hold_failure_trial_5_proposed_distinct_point")));
            }
        }
    }

    #[test]
    fn retained_hold_feedback_rejects_another_point_or_live_failure_direction() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.targets_c.push(240).unwrap();
        let mut original = profile_with_targets(&[240]);
        original.points[0].as_mut().unwrap().brake_distance_centi_c = 100;
        runtime.candidate_profile = Some(original);
        runtime.first_band_elapsed_ms = Some(1_000);
        runtime.first_band_exit_centi_c = Some(23_840);
        runtime.hold_output_count = 8;
        runtime.hold_output_histogram[900] = 8;
        let prior = runtime.candidate_failure_evidence(240, Some(0), Some(150), None, 5_000);
        runtime.remember_observed_hold_failure(240, prior);
        assert_eq!(runtime.observed_hold_failures.len(), 1);
        runtime.reset_trial_metrics();
        runtime.deadline_nearest_temperature_centi_c = Some(23_700);
        let live = runtime.candidate_failure_evidence(240, Some(0), None, None, 5_000);
        for incompatible in [
            TuneEvidence {
                tunable_stop: false,
                ..live
            },
            TuneEvidence {
                deadline_temperature_centi_c: Some(24_200),
                ..live
            },
            TuneEvidence {
                target_c: 220,
                deadline_temperature_centi_c: Some(21_700),
                ..live
            },
        ] {
            assert!(!runtime.prepare_observed_hold_failure_proposal(original, 0, incompatible));
        }
        let mut another_point = original;
        another_point.points[0]
            .as_mut()
            .unwrap()
            .hold_power_permille += 1;
        assert!(!runtime.prepare_observed_hold_failure_proposal(another_point, 0, live));
        assert_eq!(runtime.hold_output_count, 0);
        assert_eq!(runtime.accepted_target_mask, 0);
        runtime.reset_for_run();
        assert!(runtime.observed_hold_failures.is_empty());
    }

    #[test]
    fn retained_hold_feedback_requires_measured_low_failure_and_clears_on_target_change() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.targets_c.extend_from_slice(&[240, 140]).unwrap();
        runtime.candidate_profile = Some(profile_with_targets(&[240, 140]));
        runtime.armed = true;
        runtime.first_band_elapsed_ms = Some(1_000);
        runtime.first_band_exit_centi_c = Some(23_840);
        runtime.hold_output_count = 8;
        let prior = runtime.candidate_failure_evidence(240, Some(0), Some(150), None, 5_000);
        for invalid in [
            TuneEvidence {
                tunable_stop: false,
                ..prior
            },
            TuneEvidence {
                hold_sample_count: 0,
                ..prior
            },
            TuneEvidence {
                overshoot_c: 3.01,
                ..prior
            },
            TuneEvidence {
                first_band_exit_centi_c: Some(24_200),
                ..prior
            },
        ] {
            runtime.remember_observed_hold_failure(240, invalid);
            assert!(runtime.observed_hold_failures.is_empty());
        }
        runtime.remember_observed_hold_failure(240, prior);
        assert_eq!(runtime.observed_hold_failures.len(), 1);
        runtime.advance_target().unwrap();
        assert!(runtime.observed_hold_failures.is_empty());
    }

    #[test]
    fn captured_severe_trajectory_coast_corrects_handoff_above_low_supply_threshold() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("thermal_tuning_trajectory_coast.fixture.json"))
                .unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[60]);
        let previous = profile.points[0].as_mut().unwrap();
        previous.brake_distance_centi_c = 780;
        previous.approach_damping_exponent_permille = 1_000;
        previous.hold_power_permille = 56;
        previous.hold_reheat_power_permille = 56;
        runtime.candidate_profile = Some(profile);
        for recorded in fixture["samples"].as_array().unwrap() {
            let phase = serde_json::from_value(recorded["phase"].clone()).unwrap();
            let mut observation = sample(
                recorded["elapsedMs"].as_u64().unwrap(),
                phase,
                recorded["temperatureCentiC"].as_i64().unwrap() as i16,
            );
            observation.heater_requested_output_permille =
                recorded["requestPermille"].as_u64().unwrap() as u16;
            observation.heater_output_permille = observation.heater_requested_output_permille;
            runtime.update_trial_metrics(&observation, 60);
        }
        let facts = runtime.candidate_failure_evidence(60, Some(685), Some(683), None, 10_000);
        assert_eq!(facts.approach_median_output_permille, Some(130));
        assert_eq!(facts.hold_off_sample_count, facts.hold_sample_count);
        runtime.mutate_candidate_for_failure(60, Some(685), Some(683), None, 10_000);
        let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(proposed.brake_distance_centi_c >= 1_780);
        assert_eq!(proposed.approach_damping_exponent_permille, 1_000);
        assert_eq!(proposed.hold_power_permille, 56);
    }

    #[test]
    fn captured_trajectory_hold_deficit_recovers_hold_before_approach() {
        // B-group 36aa5d68, 220C batch-predicted trial 2. These are the
        // complete recorded Approach/Hold requests, including the one stop.
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[220]);
        let point = profile.points[0].as_mut().unwrap();
        point.brake_distance_centi_c = 100;
        point.approach_damping_exponent_permille = 910;
        point.approach_floor_power_permille = 800;
        point.hold_power_permille = 800;
        point.hold_reheat_power_permille = 800;
        point.hold_lead_ticks = 1;
        point.hold_blend_ticks = 1;
        point.hold_kp_permille_per_c = 235;
        point.hold_ki_permille_per_c_tick = 3;
        runtime.candidate_profile = Some(profile);
        for (elapsed, phase, temperature, output) in [
            (712_878, ThermalTuningPhase::Approach, 21_909, 50),
            (712_959, ThermalTuningPhase::Approach, 21_946, 1_000),
            (713_212, ThermalTuningPhase::Approach, 21_928, 980),
            (713_441, ThermalTuningPhase::Approach, 21_901, 1_000),
            (713_651, ThermalTuningPhase::Approach, 21_916, 1_000),
            (713_881, ThermalTuningPhase::Approach, 21_946, 1_000),
            (714_096, ThermalTuningPhase::Hold, 21_984, 870),
            (714_158, ThermalTuningPhase::Hold, 21_957, 870),
            (714_362, ThermalTuningPhase::Hold, 21_929, 890),
            (714_574, ThermalTuningPhase::Hold, 21_973, 880),
            (714_780, ThermalTuningPhase::Hold, 21_980, 850),
            (715_039, ThermalTuningPhase::Hold, 22_012, 0),
            (715_272, ThermalTuningPhase::Hold, 21_972, 810),
            (715_513, ThermalTuningPhase::Hold, 21_913, 1_000),
            (715_729, ThermalTuningPhase::Hold, 21_965, 900),
            (715_930, ThermalTuningPhase::Hold, 21_935, 900),
            (716_130, ThermalTuningPhase::Hold, 21_927, 990),
            (716_339, ThermalTuningPhase::Hold, 21_902, 1_000),
            (716_593, ThermalTuningPhase::Hold, 21_930, 980),
            (716_808, ThermalTuningPhase::Hold, 21_953, 980),
            (717_023, ThermalTuningPhase::Hold, 21_925, 1_000),
            (717_249, ThermalTuningPhase::Hold, 21_933, 1_000),
            (717_463, ThermalTuningPhase::Hold, 21_899, 1_000),
            (717_681, ThermalTuningPhase::Hold, 21_908, 1_000),
            (717_901, ThermalTuningPhase::Hold, 21_874, 1_000),
            (718_141, ThermalTuningPhase::Hold, 21_874, 1_000),
            (718_354, ThermalTuningPhase::Hold, 21_873, 1_000),
            (718_627, ThermalTuningPhase::Hold, 21_851, 1_000),
            (718_778, ThermalTuningPhase::Hold, 21_838, 1_000),
        ] {
            let mut observation = sample(elapsed, phase, temperature);
            observation.target_c = 220;
            observation.heater_requested_output_permille = output;
            observation.heater_output_permille = output;
            runtime.update_trial_metrics(&observation, 220);
        }
        let facts = runtime.candidate_failure_evidence(220, Some(12), Some(174), None, 5_000);
        assert_eq!(facts.first_hold_after_warmup_ms, Some(1_218));
        assert_eq!(facts.first_band_exit_centi_c, Some(21_838));
        assert_eq!(facts.hold_off_sample_count, 1);
        assert!(facts.hold_median_output_permille.unwrap() >= 950);
        runtime.mutate_candidate_for_failure(220, Some(12), Some(174), None, 5_000);
        let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(proposed.hold_power_permille >= 900);
        assert!(proposed.hold_reheat_power_permille >= 900);
        assert_eq!(proposed.hold_lead_ticks, 0);
        assert_eq!(proposed.brake_distance_centi_c, 100);
        assert_eq!(proposed.approach_damping_exponent_permille, 910);
    }

    #[test]
    fn trajectory_coasting_pulse_corrects_warmup_handoff_before_damping() {
        // A-group 36aa5d68 scout: a brief 20% Approach pulse,
        // mostly 5-10%, then entirely unpowered Hold still rises to 66.71C.
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[60]);
        let initial = profile.points[0].as_mut().unwrap();
        initial.brake_distance_centi_c = 780;
        initial.approach_damping_exponent_permille = 1_000;
        runtime.candidate_profile = Some(profile);
        for (elapsed, phase, temperature, output) in [
            (10_641, ThermalTuningPhase::Warmup, 3_425, 1_000),
            (17_259, ThermalTuningPhase::Approach, 5_247, 50),
            (17_338, ThermalTuningPhase::Approach, 5_257, 50),
            (17_575, ThermalTuningPhase::Approach, 5_341, 140),
            (17_820, ThermalTuningPhase::Approach, 5_439, 50),
            (18_061, ThermalTuningPhase::Approach, 5_521, 110),
            (18_323, ThermalTuningPhase::Approach, 5_558, 180),
            (18_561, ThermalTuningPhase::Approach, 5_631, 110),
            (18_816, ThermalTuningPhase::Approach, 5_698, 200),
            (19_056, ThermalTuningPhase::Approach, 5_767, 100),
            (19_315, ThermalTuningPhase::Approach, 5_825, 170),
            (19_546, ThermalTuningPhase::Approach, 5_856, 80),
            (19_789, ThermalTuningPhase::Approach, 5_901, 70),
            (20_035, ThermalTuningPhase::Approach, 5_925, 80),
            (20_499, ThermalTuningPhase::Hold, 6_009, 0),
            (21_836, ThermalTuningPhase::Hold, 6_183, 0),
            (22_532, ThermalTuningPhase::Hold, 6_264, 0),
            (27_172, ThermalTuningPhase::Hold, 6_671, 0),
        ] {
            let mut observation = sample(elapsed, phase, temperature);
            observation.heater_requested_output_permille = output;
            observation.heater_output_permille = output;
            runtime.update_trial_metrics(&observation, 60);
        }
        runtime.mutate_candidate_for_failure(60, Some(671), Some(705), None, 10_000);
        let proposed = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert!(proposed.brake_distance_centi_c > 780);
        assert_eq!(proposed.approach_damping_exponent_permille, 1_000);
    }

    #[test]
    fn legacy_approach_clamps_alone_are_not_distinct_model_proposals() {
        let mut runtime = started_single_target_runtime();
        let original = runtime.candidate_profile.unwrap();
        let point = runtime.candidate_profile.as_mut().unwrap().points[0]
            .as_mut()
            .unwrap();
        point.approach_power_permille += 10;
        point.approach_floor_power_permille += 10;
        assert!(!runtime.record_fresh_proposal(Some(original)));
    }

    #[test]
    fn candidate_preview_identity_uses_the_bounded_production_profile() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[60]);
        profile.points[0]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille = 4_050;
        runtime
            .install_candidate_profile(profile, "predicted_from_scout")
            .unwrap();
        let mut effective = runtime.candidate_profile.unwrap();
        effective.sanitize();
        assert_eq!(runtime.candidate_profile, Some(effective));
        assert_eq!(runtime.candidate_hash, hash_profile(&effective));
    }

    #[test]
    fn preview_receipt_rejects_an_unbounded_profile_identity() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[60]);
        profile.points[0]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille = 4_050;
        runtime.candidate_profile = Some(profile);
        runtime.candidate_hash = hash_profile(&profile);
        runtime.armed = true;
        assert_eq!(
            runtime.record_preview_applied(),
            Err(ThermalTuningError::IdentityMismatch)
        );
        assert!(!runtime.armed);
        assert!(runtime.preview_receipt.is_none());
    }

    #[test]
    fn high_side_candidate_mutation_bounds_damping_before_hashing() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&[60]);
        profile.points[0]
            .as_mut()
            .unwrap()
            .approach_damping_exponent_permille = 4_000;
        runtime
            .install_candidate_profile(profile, "initial")
            .unwrap();
        for (elapsed, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_500),
            (1_000, ThermalTuningPhase::Approach, 5_850),
            (2_000, ThermalTuningPhase::Hold, 5_950),
            (3_000, ThermalTuningPhase::Hold, 6_153),
        ] {
            runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
        }
        runtime.mutate_candidate_for_failure(60, Some(153), Some(203), None, 10_000);
        let effective = runtime.candidate_profile.unwrap();
        assert_eq!(
            effective.points[0]
                .unwrap()
                .approach_damping_exponent_permille,
            4_000
        );
        assert_eq!(runtime.candidate_hash, hash_profile(&effective));
    }

    #[test]
    fn local_trial_samples_select_conservative_high_side_candidate() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[60]));
        for (elapsed, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_000),
            (1_000, ThermalTuningPhase::Approach, 5_910),
            (2_000, ThermalTuningPhase::Hold, 6_090),
            (3_000, ThermalTuningPhase::Hold, 6_770),
        ] {
            runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
        }
        assert_eq!(runtime.first_hold_temperature_centi_c, Some(6_090));
        assert_eq!(runtime.hold_output_count, 2);
        assert_eq!(runtime.hold_error_sum_centi_c, -860);
        runtime.mutate_candidate_for_failure(60, Some(770), Some(680), None, 10_000);
        let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(tuned.brake_distance_centi_c, 600);
        assert_eq!(tuned.approach_power_permille, 650);
        assert_eq!(tuned.approach_floor_power_permille, 300);
        assert_eq!(tuned.hold_power_permille, 300);
    }

    #[test]
    fn high_temperature_local_band_exit_trims_the_next_preview_candidate() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[160]));
        let previous = runtime.candidate_profile.unwrap().points[0].unwrap();
        for (elapsed, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 12_000),
            (1_000, ThermalTuningPhase::Approach, 15_900),
            (2_000, ThermalTuningPhase::Hold, 15_990),
            (3_000, ThermalTuningPhase::Hold, 16_263),
        ] {
            runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 160);
        }
        runtime.mutate_candidate_for_failure(160, Some(263), Some(373), None, 15_000);
        let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(
            tuned.brake_distance_centi_c,
            previous.brake_distance_centi_c + 300
        );
        assert_eq!(
            tuned.approach_floor_power_permille,
            previous.approach_floor_power_permille - 100
        );
        assert_eq!(tuned.hold_power_permille, previous.hold_power_permille - 50);
        assert!(tuned.approach_power_permille < previous.approach_power_permille);
        assert_eq!(tuned.warmup_power_permille, 1_000);
        assert!(runtime.preview_update_pending);
        assert!(runtime.preview_receipt.is_none());
        assert_eq!(
            runtime.candidate_hash,
            hash_profile(&runtime.candidate_profile.unwrap())
        );
    }

    #[test]
    fn captured_160c_high_and_low_prefixes_select_different_candidate_directions() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/thermal-160c-high-low-replays.json"
        ))
        .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let wire: ThermalTuningCandidatePoint =
                serde_json::from_value(fixture["point"].clone()).unwrap();
            let mut profile = profile_with_targets(&[160]);
            let previous = profile.points[0].as_mut().unwrap();
            macro_rules! copy_fields {
                ($($field:ident),+ $(,)?) => { $(previous.$field = wire.$field;)+ };
            }
            copy_fields!(
                brake_distance_centi_c,
                warmup_power_permille,
                warmup_reenter_centi_c,
                approach_power_permille,
                approach_floor_power_permille,
                approach_damping_exponent_permille,
                approach_tail_window_centi_c,
                hold_power_permille,
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
                hold_lead_ticks
            );
            let previous = *previous;
            let decision = &fixture["decision"];
            let captured: ThermalTuningTimingEvidence =
                serde_json::from_value(decision["timing"].clone()).unwrap();
            let mut runtime = ThermalTuningRuntime::new();
            runtime.candidate_profile = Some(profile);
            runtime.trial_timing = Some(ThermalTuningTimingEvidence::for_point(previous));
            runtime.trial_purpose = serde_json::from_value(decision["purpose"].clone()).unwrap();
            for row in fixture["samples"].as_array().unwrap() {
                let elapsed = row[0].as_u64().unwrap();
                if captured.extended_at_ms.is_some_and(|at| elapsed >= at) {
                    runtime.trial_timing.as_mut().unwrap().extended_at_ms = captured.extended_at_ms;
                }
                let phase = serde_json::from_value(row[1].clone()).unwrap();
                let mut observed = sample(elapsed, phase, row[2].as_i64().unwrap() as i16);
                observed.heater_requested_output_permille = row[3].as_u64().unwrap() as u16;
                observed.heater_output_permille = row[4].as_u64().unwrap() as u16;
                runtime.update_trial_metrics(&observed, 160);
            }
            runtime.mutate_candidate_for_failure(
                160,
                decision["maxOvershootCentiC"]
                    .as_u64()
                    .map(|value| value as u16),
                decision["holdPeakToPeakCentiC"]
                    .as_u64()
                    .map(|value| value as u16),
                decision["settleTimeMs"].as_u64(),
                decision["hardGateFullSpeedToStableMs"].as_u64().unwrap() as u32,
            );
            let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
            if fixture["side"] == "high" {
                assert_eq!(tuned.approach_floor_power_permille, 500);
                assert_eq!(tuned.hold_power_permille, 550);
                assert_eq!(tuned.brake_distance_centi_c, 1_139);
            } else {
                assert!(
                    tuned.approach_floor_power_permille >= previous.approach_floor_power_permille
                );
                assert!(tuned.hold_power_permille >= previous.hold_power_permille);
            }
        }
    }

    #[test]
    fn controller_hold_analysis_keeps_entry_before_qualification_timer() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[60]));
        for (elapsed, phase, temperature) in [
            (1_000, ThermalTuningPhase::Approach, 5_000),
            (2_000, ThermalTuningPhase::Hold, 5_687),
            (3_000, ThermalTuningPhase::Approach, 5_800),
        ] {
            runtime.update_trial_metrics(&sample(elapsed, phase, temperature), 60);
        }
        assert_eq!(runtime.first_controller_hold_elapsed_ms, Some(2_000));
        assert_eq!(runtime.first_hold_temperature_centi_c, Some(5_687));
        assert_eq!(runtime.hold_output_count, 2);
        assert_eq!(runtime.approach_output_count, 0);
        assert!(runtime.first_hold_elapsed_ms.is_none());
        assert!(runtime.min_hold_temperature_centi_c.is_none());
        runtime.update_trial_metrics(&sample(4_000, ThermalTuningPhase::Hold, 5_950), 60);
        runtime.update_trial_metrics(&sample(5_000, ThermalTuningPhase::Hold, 6_060), 60);
        assert_eq!(runtime.first_hold_elapsed_ms, Some(4_000));
        assert_eq!(runtime.hold_output_count, 4);
        assert_eq!(runtime.hold_error_sum_centi_c, 503);
        assert_eq!(runtime.min_controller_hold_temperature_centi_c, Some(5_687));
        assert_eq!(runtime.max_controller_hold_temperature_centi_c, Some(6_060));
        assert_eq!(runtime.min_hold_temperature_centi_c, Some(5_950));
        assert_eq!(runtime.max_hold_temperature_centi_c, Some(6_060));
        runtime.reset_trial_metrics();
        assert!(runtime.first_controller_hold_elapsed_ms.is_none());
        assert!(runtime.min_controller_hold_temperature_centi_c.is_none());
        assert!(runtime.max_controller_hold_temperature_centi_c.is_none());
    }

    #[test]
    fn next_trial_drops_previous_near_target_statistics() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[60]));
        runtime.update_trial_metrics(&sample(1_000, ThermalTuningPhase::Approach, 5_950), 60);
        runtime.update_trial_metrics(&sample(2_000, ThermalTuningPhase::Hold, 6_010), 60);
        assert_eq!(runtime.approach_output_count, 1);
        assert_eq!(runtime.hold_output_count, 1);
        runtime.reset_trial_metrics();
        assert_eq!(runtime.approach_output_count, 0);
        assert_eq!(runtime.hold_output_count, 0);
        assert!(runtime.first_hold_temperature_centi_c.is_none());
        assert!(runtime.previous_sample_at_ms.is_none());
    }

    #[test]
    fn even_approach_slope_samples_use_main_strategy_upper_median() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.candidate_profile = Some(profile_with_targets(&[220]));
        runtime.approach_output_histogram[960] = 1;
        runtime.approach_output_count = 1;
        runtime.approach_slopes_c_per_s.push(0.4).unwrap();
        runtime.approach_slopes_c_per_s.push(1.2).unwrap();
        runtime.mutate_candidate_for_failure(220, Some(0), None, None, 5_000);
        let tuned = runtime.candidate_profile.unwrap().points[0].unwrap();
        assert_eq!(tuned.approach_floor_power_permille, 520);
        assert_eq!(tuned.hold_power_permille, 350);
    }

    #[test]
    fn late_approach_curve_keeps_real_hil_class() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../tools/flux-purr-devd/tests/fixtures/thermal-60c-late-window-replays.json"
        ))
        .unwrap();
        let fixture = &fixtures[1];
        assert_eq!(fixture["sourceTrialIndex"], 6);
        let mut runtime = ThermalTuningRuntime::new();
        for row in fixture["samples"].as_array().unwrap() {
            let phase: ThermalTuningPhase = serde_json::from_value(row[4].clone()).unwrap();
            if phase == ThermalTuningPhase::Warmup {
                continue;
            }
            let measured = sample(
                row[0].as_u64().unwrap(),
                phase,
                row[1].as_i64().unwrap() as i16,
            );
            runtime.record_approach_curve_sample(&measured);
            if phase == ThermalTuningPhase::Hold {
                break;
            }
        }
        assert!(runtime.approach_tail_count > 0);
        assert!(matches!(
            runtime.strategy_curve_class(60),
            CurveClass::Underpowered
        ));
    }

    #[test]
    fn approach_tail_keeps_exact_error_without_unbounded_sample_storage() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut sample = sample(1_000, ThermalTuningPhase::Approach, 5_000);
        runtime.record_approach_curve_sample(&sample);
        sample.elapsed_ms = 6_000;
        sample.temperature_centi_c = 6_000;
        runtime.record_approach_curve_sample(&sample);
        sample.elapsed_ms = 12_000;
        sample.temperature_centi_c = 5_900;
        runtime.record_approach_curve_sample(&sample);
        assert_eq!(runtime.approach_curve_samples.len(), 2);
        assert_eq!(runtime.approach_tail_count, 1);
        let mean_error = runtime.approach_curve_mean_abs_error_c(60).unwrap();
        assert!((mean_error - 3.5 / 3.0).abs() < 1.0e-9);
    }

    #[test]
    fn target_budget_replaces_the_two_trial_cap() {
        let mut runtime = started_single_target_runtime();
        let mut timestamp = 0;
        for expected_trial in 0..3 {
            if expected_trial > 0 {
                let mut cooling = sample(timestamp, ThermalTuningPhase::Cooldown, 3_400);
                cooling.trial_index = expected_trial;
                cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
                cooling.heater_requested_output_permille = 0;
                cooling.heater_output_permille = 0;
                runtime.record_sample(cooling).unwrap();
            }
            let mut warmup = sample(timestamp, ThermalTuningPhase::Warmup, 3_000);
            warmup.trial_index = expected_trial;
            warmup.candidate_hash = runtime.candidate_hash.clone().unwrap();
            runtime.record_sample(warmup).unwrap();
            let mut approach = sample(timestamp + 1_000, ThermalTuningPhase::Approach, 4_000);
            approach.trial_index = expected_trial;
            approach.candidate_hash = runtime.candidate_hash.clone().unwrap();
            runtime.record_sample(approach.clone()).unwrap();
            approach.elapsed_ms = timestamp + 11_000;
            approach.temperature_centi_c = 4_500;
            runtime.record_sample(approach).unwrap();
            assert_eq!(runtime.trial_index, expected_trial + 1);
            assert!(runtime.armed);
            timestamp += 12_000;
        }
        let mut budget = sample(
            THERMAL_TUNING_TARGET_BUDGET_MS,
            ThermalTuningPhase::Cooldown,
            3_400,
        );
        budget.trial_index = runtime.trial_index;
        budget.candidate_hash = runtime.candidate_hash.clone().unwrap();
        budget.heater_requested_output_permille = 0;
        budget.heater_output_permille = 0;
        runtime.record_sample(budget).unwrap();
        assert!(!runtime.armed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
    }

    #[test]
    fn hold_completion_at_target_budget_cannot_qualify() {
        let mut runtime = started_single_target_runtime();
        for (elapsed_ms, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_000),
            (1_000, ThermalTuningPhase::Approach, 5_500),
            (1_100, ThermalTuningPhase::Hold, 6_000),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
        record_dense_hold(&mut runtime, 1_200, 11_100, 6_000);
        record_current_trial_sample(
            &mut runtime,
            THERMAL_TUNING_TARGET_BUDGET_MS,
            ThermalTuningPhase::Hold,
            6_000,
        );
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
        assert!(runtime.save_receipt.is_none());
        assert!(runtime.events.iter().any(|event| {
            event.kind == ThermalTuningEventKind::Decision
                && event.decision.as_ref().is_some_and(|decision| {
                    decision.stop_reason == "target monotonic tuning budget exhausted"
                })
        }));
    }

    #[test]
    fn removed_240c_target_is_rejected_before_heating() {
        for power_class in [
            ThermalTuningPowerClass::Pps3a,
            ThermalTuningPowerClass::Pps5a,
        ] {
            let mut runtime = ThermalTuningRuntime::new();
            let result = runtime.start(ThermalTuningStart {
                run_id: id("run-removed-target"),
                candidate_id: id("candidate-removed-target"),
                candidate_hash: hash("hash-removed-target"),
                power_class,
                targets: &[240],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[240])),
            });
            assert_eq!(result, Err(ThermalTuningError::InvalidTargets));
            assert!(!runtime.armed);
            assert_eq!(
                runtime.terminal_disposition,
                ThermalTuningTerminalDisposition::Failed
            );
        }
    }

    #[test]
    fn full_scope_only_seeds_from_already_confirmed_recursive_bounds() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .targets_c
            .extend_from_slice(&THERMAL_TUNING_EXECUTION_ORDER_C)
            .unwrap();
        let mut visited = 0;
        for target in THERMAL_TUNING_EXECUTION_ORDER_C {
            if let Some((lower, upper)) = runtime.required_accepted_bounds(target) {
                assert!(lower < target && target < upper);
                assert_ne!(visited & target_bit(lower), 0);
                assert_ne!(visited & target_bit(upper), 0);
            } else {
                assert!(target == 60 || target == 220);
            }
            visited |= target_bit(target);
        }
        assert_eq!(
            visited.count_ones() as usize,
            THERMAL_TUNING_TARGETS_C.len()
        );
    }

    #[test]
    fn full_scope_prunes_midpoints_without_both_accepted_endpoints() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-pruned"),
                candidate_id: id("candidate-pruned"),
                candidate_hash: hash("hash-pruned"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&THERMAL_TUNING_TARGETS_C)),
            })
            .unwrap();
        runtime.accepted_target_mask = target_bit(60);
        runtime.advance_target().unwrap();
        assert_eq!(runtime.targets_c[runtime.target_index], 220);
        runtime.failed_target_mask = target_bit(220);
        runtime.advance_target().unwrap();
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.failed_target_mask.count_ones(), 7);
        assert_eq!(
            runtime
                .events
                .iter()
                .filter(|event| {
                    event.reason.as_deref() == Some("interval_pruned_without_accepted_bounds")
                })
                .count(),
            6
        );
    }

    #[test]
    fn full_scope_midpoint_uses_accepted_parent_bounds() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&THERMAL_TUNING_TARGETS_C);
        profile.points[0].as_mut().unwrap().brake_distance_centi_c = 400;
        profile.points[7].as_mut().unwrap().brake_distance_centi_c = 800;
        profile.points[3].as_mut().unwrap().brake_distance_centi_c = 300;
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-bounds"),
                candidate_id: id("candidate-bounds"),
                candidate_hash: hash("hash-bounds"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        runtime.accepted_target_mask = target_bit(60);
        runtime.advance_target().unwrap();
        runtime.accepted_target_mask |= target_bit(220);
        runtime.advance_target().unwrap();
        assert_eq!(runtime.targets_c[runtime.target_index], 120);
        let snapshot = runtime.candidate_snapshot(Some(120));
        assert_eq!(
            (snapshot.lower_target_c, snapshot.upper_target_c),
            (Some(60), Some(220))
        );
        assert_eq!(snapshot.seed_source, "accepted_bounds_linear");
        assert_eq!(snapshot.point.unwrap().brake_distance_centi_c, 550);
        assert_ne!(runtime.candidate_hash.as_deref(), Some("hash-bounds"));
        assert!(runtime.preview_update_pending);
        assert!(runtime.armed);
    }

    #[test]
    fn sample_elapsed_time_is_relative_to_the_run_not_firmware_uptime() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start_at(
                ThermalTuningStart {
                    run_id: id("run-speed-gate"),
                    candidate_id: id("candidate-hold"),
                    candidate_hash: hash("hash-hold"),
                    power_class: ThermalTuningPowerClass::Pps5a,
                    targets: &[60],
                    pps_ready: true,
                    thermal_model_available: true,
                    calibration_conflict: false,
                    candidate_profile: Some(profile_with_targets(&[60])),
                },
                99_950,
            )
            .unwrap();
        record_initial_cooldown(&mut runtime, 99_950);
        runtime
            .record_sample(sample(100_000, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(100_050, ThermalTuningPhase::Approach, 3_010))
            .unwrap();
        let elapsed: std::vec::Vec<u64> = runtime
            .events
            .iter()
            .filter_map(|event| event.sample.as_ref().map(|sample| sample.elapsed_ms))
            .collect();
        assert_eq!(elapsed, [0, 50, 100]);
        assert_eq!(runtime.current_elapsed_ms, 100);
    }

    #[test]
    fn verified_window_keeps_its_first_witness_during_later_hold_excursions() {
        // The 240C confirmation's early 10s window is valid, but later Hold
        // leaves the absolute band while staying inside its separate p2p gate.
        let mut runtime = ThermalTuningRuntime::new();
        runtime.warmup_exited_at_ms = Some(1_000);
        for elapsed_ms in (1_000..=11_000).step_by(250) {
            runtime
                .update_stable_window(&sample(elapsed_ms, ThermalTuningPhase::Hold, 23_950), 240);
        }
        assert_eq!(runtime.stable_window_started_at_ms, Some(1_000));
        assert_eq!(runtime.stable_window_verified_at_ms, Some(11_000));
        for (elapsed_ms, temperature) in [(11_250, 24_000), (11_500, 23_830), (11_750, 23_950)] {
            runtime.update_stable_window(
                &sample(elapsed_ms, ThermalTuningPhase::Hold, temperature),
                240,
            );
        }
        assert_eq!(runtime.stable_window_started_at_ms, Some(1_000));
        assert_eq!(runtime.stable_window_verified_at_ms, Some(11_000));
    }

    #[test]
    fn rejected_temperature_sample_breaks_qualification_without_disarming() {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
            .unwrap();
        runtime
            .record_live_sample(sample(600, ThermalTuningPhase::Hold, 6_000))
            .unwrap();
        assert_eq!(runtime.first_hold_elapsed_ms, Some(600));
        let mut wire = serde_json::to_value(sample(650, ThermalTuningPhase::Hold, 6_000)).unwrap();
        wire["measurementAccepted"] = serde_json::json!(false);
        wire["rawTemperatureCentiC"] = serde_json::json!(8_249);
        runtime
            .record_live_sample(serde_json::from_value(wire).unwrap())
            .unwrap();
        assert!(runtime.armed);
        assert_eq!(runtime.current_elapsed_ms, 650);
        assert_eq!(runtime.first_hold_elapsed_ms, None);
        assert_eq!(runtime.stable_window_started_at_ms, None);
        assert_eq!(runtime.max_temperature_centi_c, Some(6_000));
        assert_eq!(runtime.previous_sample_at_ms, None);
        runtime
            .record_sample(sample(900, ThermalTuningPhase::Hold, 6_010))
            .unwrap();
        assert_eq!(runtime.first_hold_elapsed_ms, Some(900));
        assert_eq!(runtime.stable_window_started_at_ms, Some(900));
    }

    #[test]
    fn rejected_temperature_keeps_wall_deadline_and_sensor_fault_gate() {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Approach, 5_500))
            .unwrap();
        let mut rejected = sample(11_000, ThermalTuningPhase::Approach, 6_000);
        rejected.measurement_accepted = Some(false);
        runtime.record_sample(rejected.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        let decision = runtime
            .events
            .iter()
            .find_map(|event| event.decision.as_ref())
            .unwrap();
        assert_eq!(decision.disposition.as_str(), "rejected");
        assert_eq!(
            decision.stop_reason,
            "full-speed-to-stable budget exhausted"
        );

        let mut runtime = started_single_target_runtime();
        rejected.measurement_valid = false;
        assert_eq!(
            runtime.record_sample(rejected),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!runtime.armed);
    }

    #[test]
    fn rejected_temperature_restarts_independent_confirmation() {
        let mut runtime = started_single_target_runtime();
        runtime.trial_purpose = ThermalTuningTrialPurpose::HoldConfirm;
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
            .unwrap();
        record_dense_hold(&mut runtime, 600, 20_000, 6_000);
        let verified = runtime.stable_window_verified_at_ms;
        assert!(verified.is_some());
        let mut rejected = sample(20_100, ThermalTuningPhase::Hold, 6_000);
        rejected.measurement_accepted = Some(false);
        runtime.record_live_sample(rejected).unwrap();
        assert_eq!(runtime.first_hold_elapsed_ms, None);
        assert_eq!(runtime.stable_window_verified_at_ms, verified);
        record_dense_hold(&mut runtime, 20_400, 61_000, 6_000);
        assert_eq!(runtime.first_hold_elapsed_ms, Some(20_400));
        assert_eq!(runtime.accepted_target_mask, 0);
        assert!(runtime.armed);
        record_dense_hold(&mut runtime, 61_300, 80_200, 6_000);
        record_current_trial_sample(&mut runtime, 80_400, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(runtime.accepted_target_mask, target_bit(60));
    }

    #[test]
    fn stable_window_started_before_deadline_can_finish_after_it() {
        let mut runtime = started_single_target_runtime();
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(1_000, ThermalTuningPhase::Approach, 4_000))
            .unwrap();
        runtime
            .record_sample(sample(9_000, ThermalTuningPhase::Approach, 5_900))
            .unwrap();
        record_dense_hold(&mut runtime, 9_100, 18_700, 6_010);
        record_current_trial_sample(&mut runtime, 19_000, ThermalTuningPhase::Hold, 6_010);
        assert!(runtime.armed);
        assert_eq!(runtime.stable_window_started_at_ms, Some(9_000));
        assert_eq!(runtime.stable_window_verified_at_ms, Some(19_000));
    }

    #[test]
    fn preview_receipt_requires_the_applied_canonical_candidate_hash() {
        let profile = profile_with_targets(&[60]);
        let candidate_hash = hash_profile(&profile).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-preview"),
                candidate_id: id("candidate-preview"),
                candidate_hash: candidate_hash.clone(),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        assert!(runtime.preview_update_pending);
        runtime.record_preview_applied().unwrap();
        assert_eq!(
            runtime.preview_receipt.unwrap().ram_applied_hash,
            candidate_hash
        );

        let mut mismatch = ThermalTuningRuntime::new();
        mismatch
            .start(ThermalTuningStart {
                run_id: id("run-mismatch"),
                candidate_id: id("candidate-mismatch"),
                candidate_hash: hash("wrong-hash"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        assert_eq!(
            mismatch.record_preview_applied(),
            Err(ThermalTuningError::IdentityMismatch)
        );
        assert!(!mismatch.armed);
        assert!(mismatch.preview_receipt.is_none());
    }

    #[test]
    fn save_receipt_binds_eeprom_generation_and_verified_bank_hash() {
        for power_class in [
            ThermalTuningPowerClass::Pps3a,
            ThermalTuningPowerClass::Pps5a,
        ] {
            let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
            profile.sanitize();
            let candidate_hash = hash_profile(&profile).unwrap();
            let mut runtime = ThermalTuningRuntime::new();
            runtime
                .start(ThermalTuningStart {
                    run_id: id("run-save"),
                    candidate_id: id("candidate-save"),
                    candidate_hash: candidate_hash.clone(),
                    power_class,
                    targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
                    pps_ready: true,
                    thermal_model_available: true,
                    calibration_conflict: false,
                    candidate_profile: Some(profile),
                })
                .unwrap();
            runtime.record_preview_applied().unwrap();
            runtime.sealed = true;
            runtime.armed = false;
            runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
            let delivered = runtime.events_after(None).next_sequence;
            runtime.events_after(delivered);
            let receipt = runtime
                .confirm_save(ThermalTuningSaveConfirmation {
                    run_id: "run-save",
                    candidate_id: "candidate-save",
                    candidate_hash: candidate_hash.as_str(),
                    power_class,
                    readback_verified: true,
                    bank_revision: 17,
                    bank_hash: candidate_hash.clone(),
                })
                .unwrap();
            assert_eq!(receipt.bank_revision, 17);
            assert_eq!(receipt.bank_hash, candidate_hash);
            assert_eq!(receipt.bank, power_class);
            assert!(receipt.atomic && receipt.readback_verified);
            assert_eq!(
                runtime.terminal_disposition,
                ThermalTuningTerminalDisposition::Passed
            );
        }
    }

    #[test]
    fn save_rejects_delivered_but_unacknowledged_events() {
        let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
        profile.sanitize();
        let candidate_hash = hash_profile(&profile).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-unacked"),
                candidate_id: id("candidate-unacked"),
                candidate_hash: candidate_hash.clone(),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile),
            })
            .unwrap();
        runtime.record_preview_applied().unwrap();
        runtime.sealed = true;
        runtime.armed = false;
        runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        runtime.events_after(None);

        assert_eq!(
            runtime.validate_save_identity(
                "run-unacked",
                "candidate-unacked",
                candidate_hash.as_str(),
                ThermalTuningPowerClass::Pps5a,
            ),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert_eq!(
            runtime.review_disposition,
            ThermalTuningReviewDisposition::ReviewIncomplete
        );
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
        assert!(runtime.save_receipt.is_none());
        assert!(
            runtime
                .events
                .iter()
                .any(|event| { event.reason.as_deref() == Some("event_ack_incomplete") })
        );
    }

    #[test]
    fn admits_pps3a_with_the_same_disarmed_preparation() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
        profile.sanitize();
        let result = runtime.start(ThermalTuningStart {
            run_id: id("run-3a"),
            candidate_id: id("candidate-3a"),
            candidate_hash: hash_profile(&profile).unwrap(),
            power_class: ThermalTuningPowerClass::Pps3a,
            targets: &THERMAL_TUNING_EXECUTION_ORDER_C,
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: Some(profile),
        });
        assert_eq!(result, Ok(()));
        assert!(runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Running
        );
    }

    #[test]
    fn three_amp_sampling_requires_its_confirmed_current_and_run_identity() {
        for current_ma in [3_000, 2_999] {
            let mut profile = profile_with_targets(&[60]);
            profile.sanitize();
            let candidate_hash = hash_profile(&profile).unwrap();
            let mut runtime = ThermalTuningRuntime::new();
            runtime
                .start(ThermalTuningStart {
                    run_id: id("run-3a"),
                    candidate_id: id("candidate-hold"),
                    candidate_hash: candidate_hash.clone(),
                    power_class: ThermalTuningPowerClass::Pps3a,
                    targets: &[60],
                    pps_ready: true,
                    thermal_model_available: true,
                    calibration_conflict: false,
                    candidate_profile: Some(profile),
                })
                .unwrap();
            let mut observed = sample(300, ThermalTuningPhase::Cooldown, 3_400);
            observed.candidate_hash = candidate_hash;
            observed.pps_contract.kind = ThermalTuningPowerClass::Pps3a;
            observed.pps_contract.current_ma = current_ma;
            observed.heater_requested_output_permille = 0;
            observed.heater_output_permille = 0;
            let result = runtime.record_sample(observed);
            assert_eq!(result.is_ok(), current_ma == 3_000);
            assert_eq!(runtime.armed, current_ma == 3_000);
        }
    }

    #[test]
    fn seal_cannot_promote_an_unfinished_trial() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-5a"),
                candidate_id: id("candidate-5a"),
                candidate_hash: hash("hash-5a"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        assert_eq!(runtime.seal(), Err(ThermalTuningError::InvalidTransition));
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Running
        );
        assert_eq!(
            runtime.confirm_save(ThermalTuningSaveConfirmation {
                run_id: "run-5a",
                candidate_id: "candidate-5a",
                candidate_hash: "hash-5a",
                power_class: ThermalTuningPowerClass::Pps5a,
                readback_verified: true,
                bank_revision: 1,
                bank_hash: hash("hash-5a"),
            }),
            Err(ThermalTuningError::NotSealed)
        );
    }

    #[test]
    fn candidate_seal_requires_prior_ack_and_creates_a_new_ack_boundary() {
        let mut runtime = ThermalTuningRuntime::new();
        let profile = profile_with_targets(&[60]);
        let candidate_hash = hash_profile(&profile).unwrap();
        runtime.run_id = Some(id("run-seal"));
        runtime.candidate_id = Some(id("candidate-seal"));
        runtime.candidate_hash = Some(candidate_hash.clone());
        runtime.candidate_profile = Some(profile);
        runtime.power_class = Some(ThermalTuningPowerClass::Pps5a);
        runtime.phase = ThermalTuningPhase::Completed;
        runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        runtime.preview_receipt = Some(ThermalTuningPreviewReceipt {
            run_id: id("run-seal"),
            candidate_id: id("candidate-seal"),
            candidate_hash: candidate_hash.clone(),
            power_class: ThermalTuningPowerClass::Pps5a,
            ram_applied_hash: candidate_hash,
        });
        runtime.push_event(
            ThermalTuningEventKind::Decision,
            Some(60),
            Some(0),
            None,
            Some("target completed"),
        );
        let first_page = runtime.events_after(None);
        assert_eq!(runtime.seal(), Err(ThermalTuningError::InvalidTransition));
        assert!(!runtime.sealed);
        runtime.events_after(first_page.next_sequence);
        runtime.seal().unwrap();
        assert!(runtime.sealed);
        assert_eq!(runtime.trace().last_sequence, Some(1));
        runtime.seal().unwrap();
        assert_eq!(runtime.trace().last_sequence, Some(1));
        assert_eq!(runtime.cancel("owner_cancel"), Ok(()));
        assert!(!runtime.sealed);
    }

    #[test]
    fn unsealed_ready_candidate_can_be_canceled() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        assert_eq!(runtime.cancel("owner_cancel"), Ok(()));
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Canceled
        );
    }

    #[test]
    fn unsafe_sample_disarms_runtime() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run"),
                candidate_id: id("candidate"),
                candidate_hash: hash("hash"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        let sample = ThermalTuningSample {
            sequence: 1,
            elapsed_ms: 300,
            target_c: 60,
            trial_index: 0,
            candidate_id: id("candidate"),
            candidate_hash: hash("hash"),
            temperature_centi_c: 2500,
            raw_temperature_centi_c: None,
            vin_mv: 12000,
            pps_contract: ThermalTuningPpsContract {
                voltage_mv: 12000,
                current_ma: 5000,
                kind: ThermalTuningPowerClass::Pps3a,
                confirmed: true,
            },
            requested_mv: 12000,
            pps_power_ceiling_mv: 12_000,
            heater_requested_output_permille: 1000,
            heater_output_permille: 0,
            measurement_valid: true,
            measurement_accepted: None,
            phase: ThermalTuningPhase::Warmup,
            fan: None,
        };
        assert_eq!(
            runtime.record_sample(sample),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
    }

    #[test]
    fn milliwatts_in_voltage_ceiling_disarms_runtime() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        let mut invalid = sample(300, ThermalTuningPhase::Warmup, 2500);
        invalid.pps_power_ceiling_mv = 105_000;

        assert_eq!(
            runtime.record_sample(invalid),
            Err(ThermalTuningError::InvalidTransition)
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
    }

    #[test]
    fn completes_sixty_second_hold_and_keeps_zero_based_paged_trace() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-hold"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();

        record_initial_cooldown(&mut runtime, 0);
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
            .unwrap();
        let mut after_sequence = None;
        let mut trial_open_sequence = None;
        let mut closed_trial = None;
        for elapsed_ms in (600..=12_600).step_by(300) {
            runtime
                .record_sample(sample(elapsed_ms, ThermalTuningPhase::Hold, 6_000))
                .unwrap();
        }
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        record_initial_cooldown(&mut runtime, 12_900);
        let mut warmup = sample(13_000, ThermalTuningPhase::Warmup, 3_000);
        warmup.trial_index = 1;
        runtime.record_sample(warmup).unwrap();
        let mut approach = sample(13_300, ThermalTuningPhase::Approach, 5_500);
        approach.trial_index = 1;
        runtime.record_sample(approach).unwrap();
        for elapsed_ms in (13_600..=73_600).step_by(300) {
            let mut hold = sample(elapsed_ms, ThermalTuningPhase::Hold, 6_000);
            hold.trial_index = 1;
            runtime.record_sample(hold).unwrap();
            loop {
                let page = runtime.events_after(after_sequence);
                if page.events.is_empty() {
                    break;
                }
                trial_open_sequence = trial_open_sequence.or_else(|| {
                    page.events
                        .iter()
                        .find(|event| {
                            event.kind == ThermalTuningEventKind::CandidateTrial
                                && event
                                    .trial
                                    .as_ref()
                                    .is_some_and(|trial| trial.end_sequence.is_none())
                        })
                        .map(|event| event.sequence)
                });
                closed_trial = closed_trial.or_else(|| {
                    page.events
                        .iter()
                        .filter_map(|event| event.trial.as_ref())
                        .find(|trial| {
                            trial.end_sequence.is_some()
                                && trial.purpose == ThermalTuningTrialPurpose::HoldConfirm
                        })
                        .cloned()
                });
                after_sequence = page.next_sequence;
            }
        }

        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::CandidateReady
        );
        assert!(!runtime.sealed);
        assert_eq!(runtime.trace().first_sequence, Some(0));
        assert!(runtime.trace().last_sequence.unwrap_or_default() > 200);
        assert_eq!(trial_open_sequence, Some(2));
        assert!(closed_trial.is_some_and(|trial: ThermalTuningTrialRecord| {
            trial.end_sequence.is_some()
                && trial.start_elapsed_ms == 13_000
                && trial.end_elapsed_ms == Some(73_600)
        }));
        let digest_before_ack = runtime.trace().digest;
        runtime.events_after(runtime.trace().last_sequence);
        assert_eq!(runtime.trace().digest, digest_before_ack);
        assert!(!runtime.trace().gap);
        assert_eq!(
            runtime.confirm_save(ThermalTuningSaveConfirmation {
                run_id: "run-hold",
                candidate_id: "candidate-hold",
                candidate_hash: "hash-hold",
                power_class: ThermalTuningPowerClass::Pps5a,
                readback_verified: true,
                bank_revision: 1,
                bank_hash: hash("hash-hold"),
            }),
            Err(ThermalTuningError::NotSealed)
        );
        runtime.preview_receipt = Some(ThermalTuningPreviewReceipt {
            run_id: id("run-hold"),
            candidate_id: id("candidate-hold"),
            candidate_hash: hash("hash-hold"),
            power_class: ThermalTuningPowerClass::Pps5a,
            ram_applied_hash: hash("hash-hold"),
        });
        runtime.preview_update_pending = false;
        runtime.seal().unwrap();
        assert!(runtime.sealed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Sealed);
        assert_ne!(runtime.trace().digest, digest_before_ack);
        let seal_page = runtime.events_after(None);
        assert!(seal_page.events.iter().any(|event| {
            event.reason.as_deref() == Some("candidate sealed for explicit save confirmation")
        }));
        runtime.events_after(seal_page.next_sequence);
        assert_eq!(
            runtime.confirm_save(ThermalTuningSaveConfirmation {
                run_id: "run-hold",
                candidate_id: "candidate-hold",
                candidate_hash: "hash-hold",
                power_class: ThermalTuningPowerClass::Pps5a,
                readback_verified: true,
                bank_revision: 1,
                bank_hash: hash("hash-hold"),
            }),
            Err(ThermalTuningError::InvalidTargets)
        );
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
    }

    #[test]
    fn caller_failure_context_precedes_canonical_disarmed_failure() {
        let mut runtime = started_single_target_runtime();
        let before = runtime.trace().last_sequence.unwrap();
        let digest_before = runtime.trace().digest;
        runtime.fail_with_context(
            "pd_contract_lost",
            &[
                "pd:power p=fault f=reset",
                "pd:power a=none r=none ticket=0",
            ],
        );
        let tail = &runtime.events[runtime.events.len() - 3..];
        assert_eq!(tail[0].sequence, before + 1);
        assert_eq!(tail[0].reason.as_deref(), Some("pd:power p=fault f=reset"));
        assert_eq!(
            tail[1].reason.as_deref(),
            Some("pd:power a=none r=none ticket=0")
        );
        assert_eq!(tail[2].reason.as_deref(), Some("pd_contract_lost"));
        assert!(
            tail.iter()
                .all(|event| event.kind == ThermalTuningEventKind::Safety)
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
        assert_eq!(
            runtime.review_disposition,
            ThermalTuningReviewDisposition::Complete
        );
        assert!(runtime.trace().complete);
        assert_ne!(runtime.trace().digest, digest_before);
        let digest = runtime.trace().digest;
        runtime.recompute_trace_digest();
        assert_eq!(runtime.trace().digest, digest);
    }

    #[test]
    fn caller_failure_context_overflow_stays_review_incomplete() {
        let mut runtime = started_single_target_runtime();
        while runtime.events.len() < THERMAL_TUNING_EVENT_CAPACITY - 1 {
            runtime.push_phase_transition_event("retained unacknowledged event");
        }
        runtime.fail_with_context(
            "pd_contract_lost",
            &[
                "pd:power p=fault f=reset",
                "pd:power a=none r=none ticket=0",
            ],
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
        assert_eq!(
            runtime.review_disposition,
            ThermalTuningReviewDisposition::ReviewIncomplete
        );
        assert!(runtime.trace().gap);
        assert!(!runtime.trace().complete);
    }

    #[test]
    fn event_ack_cannot_skip_undelivered_evidence() {
        let mut runtime = started_single_target_runtime();
        let first_page = runtime.events_after(None);
        let delivered = first_page.next_sequence.unwrap();
        assert!(runtime.trace().last_sequence.unwrap() >= delivered);

        let skipped = runtime.events_after(Some(delivered + 1));
        assert!(!runtime.armed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
        assert_eq!(
            runtime.review_disposition,
            ThermalTuningReviewDisposition::ReviewIncomplete
        );
        assert!(skipped.trace.gap);
        assert!(runtime.events.iter().any(|event| {
            event.kind == ThermalTuningEventKind::Safety
                && event.reason.as_deref() == Some("event_ack_skipped_unconfirmed")
        }));
    }

    #[test]
    fn accepted_target_cools_before_the_next_target_arms() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-hold"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60, 80],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60, 80])),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        for (elapsed_ms, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_000),
            (300, ThermalTuningPhase::Approach, 5_500),
            (600, ThermalTuningPhase::Hold, 6_000),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
        record_dense_hold(&mut runtime, 700, 12_400, 6_000);
        record_current_trial_sample(&mut runtime, 12_600, ThermalTuningPhase::Hold, 6_000);
        record_initial_cooldown(&mut runtime, 12_900);
        for (elapsed_ms, phase, temperature) in [
            (13_000, ThermalTuningPhase::Warmup, 3_000),
            (13_300, ThermalTuningPhase::Approach, 5_500),
            (13_600, ThermalTuningPhase::Hold, 6_000),
        ] {
            let mut next = sample(elapsed_ms, phase, temperature);
            next.trial_index = 1;
            runtime.record_sample(next).unwrap();
        }
        record_dense_hold(&mut runtime, 13_700, 73_400, 6_000);
        record_current_trial_sample(&mut runtime, 73_600, ThermalTuningPhase::Hold, 6_000);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.target_index, 1);
        let mut cooling = sample(73_900, ThermalTuningPhase::Warmup, 5_000);
        cooling.target_c = 80;
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling.clone()).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        cooling.elapsed_ms = 74_200;
        cooling.temperature_centi_c = 3_900;
        runtime.record_sample(cooling).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
        assert_eq!(runtime.trial_index, 0);
    }

    #[test]
    fn failed_trial_changes_candidate_only_at_cooldown_boundary() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-hold"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        for (elapsed_ms, phase, temperature) in [
            (0, ThermalTuningPhase::Warmup, 3_000),
            (300, ThermalTuningPhase::Approach, 5_500),
            (600, ThermalTuningPhase::Hold, 6_000),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
        record_dense_hold(&mut runtime, 700, 10_600, 6_000);
        record_dense_hold(&mut runtime, 10_700, 12_500, 6_400);
        record_current_trial_sample(&mut runtime, 12_600, ThermalTuningPhase::Hold, 6_400);
        assert_eq!(runtime.phase, ThermalTuningPhase::Cooldown);
        assert_eq!(runtime.trial_index, 1);
        assert!(runtime.preview_update_pending);
        assert_ne!(runtime.candidate_hash.as_deref(), Some("hash-hold"));
        let mut cooling = sample(60_900, ThermalTuningPhase::Warmup, 3_400);
        cooling.trial_index = 1;
        cooling.candidate_hash = runtime.candidate_hash.clone().unwrap();
        cooling.heater_requested_output_permille = 0;
        cooling.heater_output_permille = 0;
        runtime.record_sample(cooling).unwrap();
        assert_eq!(runtime.phase, ThermalTuningPhase::Warmup);
    }

    #[test]
    fn hold_confirmation_uses_elapsed_time_not_sample_count() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-hold"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        runtime
            .record_sample(sample(0, ThermalTuningPhase::Warmup, 3_000))
            .unwrap();
        runtime
            .record_sample(sample(300, ThermalTuningPhase::Approach, 5_500))
            .unwrap();
        runtime
            .record_sample(sample(600, ThermalTuningPhase::Hold, 6_000))
            .unwrap();
        record_dense_hold(&mut runtime, 700, 10_600, 6_000);
        for _ in 0..100 {
            runtime
                .record_sample(sample(10_900, ThermalTuningPhase::Hold, 6_000))
                .unwrap();
        }
        assert!(runtime.armed);
        assert!(!runtime.sealed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Running
        );
    }

    #[test]
    fn sample_candidate_identity_mismatch_disarms() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-hold"),
                candidate_id: id("candidate-hold"),
                candidate_hash: hash("hash-hold"),
                power_class: ThermalTuningPowerClass::Pps5a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        let mut wrong = sample(300, ThermalTuningPhase::Warmup, 3_000);
        wrong.candidate_hash = hash("another-candidate");
        assert_eq!(
            runtime.record_sample(wrong),
            Err(ThermalTuningError::IdentityMismatch)
        );
        assert!(!runtime.armed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
    }
}
