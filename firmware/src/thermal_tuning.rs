use core::{cmp::Ordering, fmt::Write};

use heapless::{String, Vec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::pd::{FUSB302B_PD_ABSOLUTE_MAX_MV, FUSB302B_PD_ABSOLUTE_MIN_MV};
use crate::heater_phase::{HOLD_ENTRY_BELOW_C, hold_entry_ready};
use crate::memory::{
    ThermalControlProfileConfig, ThermalControlProfilePointConfig, ThermalProfileBank,
};
use crate::thermal_tuning_limits::{
    TRIAL_TIMEOUT_MS, ThermalTuningTimingEvidence, original_settle_limit_ms,
};
use crate::thermal_tuning_measurement::{
    QualificationContinuity, QualificationMeasurement, QualificationObservation,
};
use crate::thermal_tuning_strategy::{
    CurveClass, TuneEvidence, bounded_failure_alternative, braking_boundary_damping_candidate,
    limited_hold_outputs, observed_low_side_hold_failure, reseed_after_hold_confirm, tune_point,
    unobserved_low_side_failure,
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
    floor_only: bool,
    braking_error_centi_c: Option<u16>,
}

#[derive(Clone, Copy)]
enum FailureBracketAxis {
    Damping,
    Braking,
    ResponseHorizon,
}

impl FailureBracketAxis {
    fn observes(self, failure: ObservedDampingFailure) -> bool {
        !failure.floor_only || matches!(self, Self::Braking)
    }

    fn value(self, point: ThermalControlProfilePointConfig) -> u16 {
        match self {
            Self::Damping => point.approach_damping_exponent_permille,
            Self::Braking => point.brake_distance_centi_c,
            Self::ResponseHorizon => point.approach_lead_ticks,
        }
    }

    fn set(self, point: &mut ThermalControlProfilePointConfig, value: u16) {
        match self {
            Self::Damping => point.approach_damping_exponent_permille = value,
            Self::Braking => point.brake_distance_centi_c = value,
            Self::ResponseHorizon => point.approach_lead_ticks = value,
        }
    }

    fn same_family(
        self,
        mut left: ThermalControlProfilePointConfig,
        mut right: ThermalControlProfilePointConfig,
    ) -> bool {
        self.set(&mut left, 0);
        self.set(&mut right, 0);
        same_effective_search_point(left, right)
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Damping => "same_run_damping_bracket",
            Self::Braking => "same_run_braking_bracket",
            Self::ResponseHorizon => "same_run_response_horizon_bracket",
        }
    }
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
    longest_qualified_hold_ms: u64,
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
    last_trial_request_mv: Option<u16>,
    hold_pps_wait_interruptions: u32,
    hold_pps_wait_pending: bool,
    qualification_continuity: QualificationContinuity,
    guarded_hold_handoff: bool,
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
    const EMPTY: Self = Self {
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
        longest_qualified_hold_ms: 0,
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
        last_trial_request_mv: None,
        hold_pps_wait_interruptions: 0,
        hold_pps_wait_pending: false,
        qualification_continuity: QualificationContinuity::new(),
        guarded_hold_handoff: false,
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
    };

    pub const fn new() -> Self {
        Self::EMPTY
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
        let scout_hold_complete = self.scout_hold_complete_at(elapsed_ms);
        let hold_confirm = self.trial_purpose == ThermalTuningTrialPurpose::HoldConfirm
            && self.first_hold_elapsed_ms.is_some_and(|started| {
                elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_HOLD_CONFIRM_MS
            });
        let budget_available = self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            < THERMAL_TUNING_TARGET_BUDGET_MS;
        // Only a window established within the unchanged target budget may finish its witness.
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

    fn scout_hold_complete_at(&self, elapsed_ms: u64) -> bool {
        self.first_hold_elapsed_ms.is_some_and(|started| {
            elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_SCOUT_HOLD_MS
        })
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
        if !passes
            && self.phase == ThermalTuningPhase::Warmup
            && self.warmup_exited_at_ms.is_none()
            && (self.trial_start_elapsed_ms.is_some_and(|started| {
                self.current_elapsed_ms.saturating_sub(started) >= TRIAL_TIMEOUT_MS
            }) || self
                .current_elapsed_ms
                .saturating_sub(self.target_started_elapsed_ms)
                >= THERMAL_TUNING_TARGET_BUDGET_MS)
        {
            return self.finish_warmup_timeout(target_c);
        }
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

    fn finish_warmup_timeout(&mut self, target_c: i16) -> Result<(), ThermalTuningError> {
        // A cold-start timeout never exercised Approach or Hold. Retry a
        // materially warmer physical start instead of inventing feedback
        // for their inactive parameters. Every attempt still stops at
        // 180 s and goes through the normal full-speed cooldown gate.
        let warmer_start_available = self.warmup_full_output
            && !self.warmup_full_output_invalid
            && self
                .trial_start_temperature_centi_c
                .zip(self.previous_sample_temperature_centi_c)
                .is_some_and(|(start, end)| {
                    let admitted_end =
                        i32::from(end).min(i32::from(cooldown_threshold_c(target_c)) * 100);
                    admitted_end - i32::from(start) >= 500
                })
            && self
                .previous_sample_at_ms
                .is_some_and(|last| self.current_elapsed_ms.saturating_sub(last) <= 1_000)
            && self
                .current_elapsed_ms
                .saturating_sub(self.target_started_elapsed_ms)
                < THERMAL_TUNING_TARGET_BUDGET_MS;
        if warmer_start_available {
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(target_c),
                Some(self.trial_index),
                None,
                Some("warmup_timeout_retry_from_warmer_start"),
            );
            return self
                .schedule_trial(self.trial_purpose, "cooling before unchanged warmup retry");
        }
        self.failed_target_mask |= target_bit(target_c);
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some("warmup_timeout_without_safe_warmer_retry"),
        );
        self.advance_target()
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
        let Some(high_side) = bracket_failure_side(evidence) else {
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
            floor_only: evidence.approach_floor_only,
            braking_error_centi_c: braking_failure_weight(evidence, high_side),
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
        self.prepare_failure_bracket(original, FailureBracketAxis::Damping)
    }

    fn prepare_failure_bracket(
        &mut self,
        original: ThermalControlProfileConfig,
        axis: FailureBracketAxis,
    ) -> bool {
        let Some(target) = self.targets_c.get(self.target_index).copied() else {
            return false;
        };
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
            if !high.high_side || !axis.observes(*high) || !axis.same_family(high.point, previous) {
                continue;
            }
            let low = self
                .observed_damping_failures
                .iter()
                .filter(|low| {
                    !low.high_side
                        && axis.observes(**low)
                        && axis.same_family(low.point, previous)
                        && axis.value(low.point).saturating_sub(axis.value(high.point)) >= 2
                })
                .min_by_key(|low| axis.value(low.point));
            let Some(low) = low else {
                continue;
            };
            let width = axis.value(low.point) - axis.value(high.point);
            if bracket.is_none_or(|(h, l)| width < axis.value(l.point) - axis.value(h.point)) {
                bracket = Some((*high, *low));
            }
        }
        let Some((high, low)) = bracket else {
            return false;
        };
        if matches!(axis, FailureBracketAxis::Damping)
            && axis.value(low.point) - axis.value(high.point) <= 50
            && previous.approach_lead_ticks == 0
            && let Some(result) = self.prepare_narrow_damping_braking(original, index, high, low)
        {
            return result;
        }
        let mut proposed = original;
        let (value, method) = self.failure_bracket_value(previous, axis, high, low);
        axis.set(proposed.points[index].as_mut().unwrap(), value);
        if matches!(axis, FailureBracketAxis::Damping)
            && axis.value(low.point) - axis.value(high.point) <= 50
        {
            // A narrow failed bracket has exhausted this one-dimensional
            // timing/overshoot tradeoff. Search the existing response horizon
            // instead of spending more experiments on sub-percent damping.
            let point = proposed.points[index].as_mut().unwrap();
            point.approach_lead_ticks = point.approach_lead_ticks.saturating_sub(1);
        }
        proposed.sanitize();
        self.candidate_profile = Some(proposed);
        if !self.record_fresh_proposal(Some(original)) {
            self.candidate_profile = Some(original);
            return false;
        }
        if self
            .install_candidate_profile(proposed, axis.reason())
            .is_err()
        {
            self.candidate_profile = Some(original);
            return false;
        }
        let mut reason = TuningReason::new();
        let _ = write!(
            reason,
            "{}_trials_{}_{}_{}_{}",
            axis.reason(),
            high.trial_index,
            low.trial_index,
            method,
            value
        );
        if proposed.points[index].unwrap().approach_lead_ticks != previous.approach_lead_ticks {
            let _ = write!(
                reason,
                "_horizon_{}",
                proposed.points[index].unwrap().approach_lead_ticks
            );
        }
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target),
            Some(self.trial_index),
            None,
            Some(reason.as_str()),
        );
        true
    }

    fn prepare_narrow_damping_braking(
        &mut self,
        original: ThermalControlProfileConfig,
        index: usize,
        high: ObservedDampingFailure,
        low: ObservedDampingFailure,
    ) -> Option<bool> {
        // A minimum response horizon cannot absorb the narrow failed tradeoff.
        // Explore braking from the actual high-side witness, retaining damping.
        // Try smaller distinct steps before treating the start-gap boundary
        // as exhausted; no rejected proposal starts a physical experiment.
        for step in [40, 20, 10, 5, 2, 1] {
            let mut proposed = original;
            proposed.points[index] = Some(ThermalControlProfilePointConfig {
                brake_distance_centi_c: high
                    .point
                    .brake_distance_centi_c
                    .saturating_add(step)
                    .min(5_000),
                ..high.point
            });
            proposed.sanitize();
            self.candidate_profile = Some(proposed);
            if !self.record_fresh_proposal(Some(original)) {
                continue;
            }
            if self
                .install_candidate_profile(proposed, "narrow_damping_bracket_braking")
                .is_err()
            {
                self.candidate_profile = Some(original);
                return Some(false);
            }
            let mut reason = TuningReason::new();
            let _ = write!(
                reason,
                "narrow_damping_trials_{}_{}_minimum_horizon_brake_{}",
                high.trial_index,
                low.trial_index,
                proposed.points[index].unwrap().brake_distance_centi_c
            );
            self.push_event(
                ThermalTuningEventKind::Decision,
                Some(high.point.target_temp_c),
                Some(self.trial_index),
                None,
                Some(reason.as_str()),
            );
            return Some(true);
        }
        self.candidate_profile = Some(original);
        None
    }

    fn failure_bracket_value(
        &self,
        previous: ThermalControlProfilePointConfig,
        axis: FailureBracketAxis,
        high: ObservedDampingFailure,
        low: ObservedDampingFailure,
    ) -> (u16, &'static str) {
        let midpoint =
            axis.value(high.point) + (axis.value(low.point) - axis.value(high.point)) / 2;
        if matches!(axis, FailureBracketAxis::Braking)
            && let Some(weighted) = weighted_braking_value(high, low)
        {
            let mut proposed = previous;
            axis.set(&mut proposed, weighted);
            if !same_effective_search_point(previous, proposed)
                && !self
                    .proposed_points
                    .iter()
                    .any(|point| same_effective_search_point(*point, proposed))
            {
                return (weighted, "weighted");
            }
        }
        (midpoint, "midpoint")
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
        if bracket_failure_side(evidence).is_some()
            && (self.prepare_damping_bracket(original)
                || self.prepare_failure_bracket(original, FailureBracketAxis::Braking)
                || self.prepare_failure_bracket(original, FailureBracketAxis::ResponseHorizon))
        {
            return true;
        }
        if self.prepare_short_approach_hold_handoff(original, target_c, evidence) {
            return true;
        }
        if self.prepare_in_band_hold_baseline(original, target_c, evidence) {
            return true;
        }
        if self.prepare_late_stability_proposal(original, target_c, evidence) {
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
        if let Some(point) = braking_boundary_damping_candidate(previous, evidence) {
            let mut alternative = original;
            alternative.points[index] = Some(point);
            alternative.sanitize();
            self.candidate_profile = Some(alternative);
            if self.record_fresh_proposal(Some(original)) {
                return self
                    .install_candidate_profile(alternative, "braking_boundary_approach_damping")
                    .is_ok();
            }
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

    fn prepare_late_stability_proposal(
        &mut self,
        original: ThermalControlProfileConfig,
        target_c: i16,
        evidence: TuneEvidence,
    ) -> bool {
        // A real Hold entry before the deadline is useful evidence, even when
        // rejected samples postpone or interrupt the continuous window while
        // the complete observed thermal prefix remains in band. Raise the handoff by
        // one bounded brake step; do not alter Hold or waive the 10 s gate.
        if !(late_stability_handoff_failure(evidence)
            || interrupted_stability_handoff_failure(evidence)
            || in_band_interrupted_stability_failure(evidence))
        {
            return false;
        }
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        let mut proposed_point = previous;
        proposed_point.brake_distance_centi_c =
            previous.brake_distance_centi_c.saturating_sub(40).max(1);
        if proposed_point.brake_distance_centi_c == previous.brake_distance_centi_c {
            return false;
        }
        let mut proposed = original;
        proposed.points[index] = Some(proposed_point);
        proposed.sanitize();
        self.candidate_profile = Some(proposed);
        if !self.record_fresh_proposal(Some(original)) {
            self.candidate_profile = Some(original);
            return false;
        }
        if self
            .install_candidate_profile(proposed, "late_stability_approach_handoff")
            .is_err()
        {
            self.candidate_profile = Some(original);
            return false;
        }
        self.push_event(
            ThermalTuningEventKind::Decision,
            Some(target_c),
            Some(self.trial_index),
            None,
            Some("late_stability_approach_handoff_brake_step"),
        );
        true
    }

    fn prepare_short_approach_hold_handoff(
        &mut self,
        original: ThermalControlProfileConfig,
        target_c: i16,
        evidence: TuneEvidence,
    ) -> bool {
        // One or two Approach samples do not prove a sustained floor for
        // conditional extension. They also cannot erase a long measured Hold.
        if !(1..=2).contains(&self.model_approach_sample_count)
            || !self
                .model_approach_output_range
                .is_some_and(|(min, max)| min >= 50 && max <= 1_000)
            || evidence.hold_pps_wait_interruptions == 0
            || evidence.hold_sample_count < 12
            || self
                .first_controller_hold_elapsed_ms
                .is_none_or(|started| self.current_elapsed_ms.saturating_sub(started) < 2_000)
            || !in_band_interrupted_stability_temperature(TuneEvidence {
                approach_output_range: self.model_approach_output_range,
                ..evidence
            })
            || evidence.under_c > evidence.over_c + 0.5
        {
            return false;
        }
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        for step in 1..=8 {
            let mut proposed = original;
            let point = proposed.points[index].as_mut().unwrap();
            point.brake_distance_centi_c = point
                .brake_distance_centi_c
                .saturating_add(40 * step)
                .min(5_000);
            proposed.sanitize();
            self.candidate_profile = Some(proposed);
            if self.record_fresh_proposal(Some(original)) {
                return self
                    .install_candidate_profile(proposed, "short_approach_timed_hold_handoff")
                    .is_ok();
            }
        }
        self.candidate_profile = Some(original);
        false
    }

    fn prepare_in_band_hold_baseline(
        &mut self,
        original: ThermalControlProfileConfig,
        target_c: i16,
        evidence: TuneEvidence,
    ) -> bool {
        if !evidence.tunable_stop
            || !evidence.full_speed_failed
            || evidence.stable_window_verified
            || evidence.hold_pps_wait_interruptions == 0
            || evidence.hold_sample_count < 30
            || evidence.overshoot_c > 1.5
            || evidence.hold_p2p_c > 3.0
            || evidence.under_c > 1.5
            || evidence.over_c > 1.5
            || evidence.first_band_exit_centi_c.is_some()
            || evidence
                .first_hold_after_warmup_ms
                .is_none_or(|at| at > evidence.settle_limit_ms)
        {
            return false;
        }
        let Some(index) = original
            .points
            .iter()
            .position(|point| point.is_some_and(|point| point.target_temp_c == target_c))
        else {
            return false;
        };
        let previous = original.points[index].unwrap();
        let Some(baseline) = evidence.hold_median_output_permille.filter(|value| {
            *value >= 50 && value.saturating_add(20) <= previous.hold_power_permille
        }) else {
            return false;
        };
        let mut proposed = original;
        proposed.points[index].as_mut().unwrap().hold_power_permille = baseline;
        proposed.sanitize();
        self.candidate_profile = Some(proposed);
        if !self.record_fresh_proposal(Some(original)) {
            self.candidate_profile = Some(original);
            return false;
        }
        self.install_candidate_profile(proposed, "in_band_hold_pps_wait_baseline")
            .is_ok()
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
            if !(explore_braking && self.prepare_failed_damping_braking(target_c, evidence)
                || self.prepare_fresh_failure_proposal_from_evidence(target_c, evidence))
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
            let reseeded = reseed_after_hold_confirm(previous, evidence);
            if same_effective_search_point(previous, reseeded) {
                // A no-op in the frozen parsed-stage reseed does not exhaust
                // the firmware strategy's complete same-run Hold evidence.
                tune_point(previous, evidence)
            } else {
                reseeded
            }
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
            hold_pps_wait_interruptions: self.hold_pps_wait_interruptions,
            guarded_hold_handoff: self.guarded_hold_handoff,
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
            // Search may use an already observed uninterrupted 12s segment;
            // trial completion still requires the current segment's clock.
            scout_hold_complete: self.longest_qualified_hold_ms >= THERMAL_TUNING_SCOUT_HOLD_MS,
            stable_window_verified: self.stable_window_verified_at_ms.is_some(),
            first_band_observed: self.first_band_elapsed_ms.is_some(),
            first_band_exit_centi_c: self.first_band_exit_centi_c,
            deadline_temperature_centi_c: self.deadline_temperature_evidence(),
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
        self.longest_qualified_hold_ms = 0;
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
        self.last_trial_request_mv = None;
        self.hold_pps_wait_interruptions = 0;
        self.hold_pps_wait_pending = false;
        self.qualification_continuity = QualificationContinuity::new();
        self.guarded_hold_handoff = false;
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
        let sample_is_fresh = sample.has_fresh_measurement();
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
        if sample_is_fresh
            && self
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
        let measurement = self
            .qualification_continuity
            .observe(QualificationObservation {
                elapsed_ms: sample.elapsed_ms,
                requested_mv: Some(sample.requested_mv),
                confirmed_contract_mv: sample
                    .pps_contract
                    .confirmed
                    .then_some(sample.pps_contract.voltage_mv),
                valid: sample.measurement_valid,
                accepted: sample.has_fresh_measurement(),
                temperature_centi_c: sample.temperature_centi_c,
                raw_temperature_centi_c: sample.raw_temperature_centi_c,
                target_c: expected_target,
            });
        if measurement == QualificationMeasurement::FreshAfterGap {
            self.interrupt_measurement_qualification();
        }
        let request_changed = self
            .last_trial_request_mv
            .is_some_and(|previous| previous != sample.requested_mv);
        self.last_trial_request_mv = Some(sample.requested_mv);
        let pps_wait_pending = self.hold_pps_wait_pending || request_changed;
        self.hold_pps_wait_pending = request_changed && sample.has_fresh_measurement();
        self.update_warmup_output_evidence(sample);
        self.record_trial_phase(sample, expected_target);
        if !sample.has_fresh_measurement() {
            let bounded_pps_pause = measurement == QualificationMeasurement::PpsPause
                && self.trial_timing.is_some_and(|timing| {
                    timing.policy_version
                        == crate::thermal_tuning_limits::PPS_CONTINUITY_TIMING_POLICY_VERSION
                });
            self.guarded_hold_handoff |= sample.measurement_valid
                && sample.phase == ThermalTuningPhase::Hold
                && self.hold_output_count > 0
                && self
                    .first_controller_hold_elapsed_ms
                    .is_some_and(|entry| sample.elapsed_ms.saturating_sub(entry) <= 1_000)
                && sample
                    .raw_temperature_centi_c
                    .is_some_and(|raw| raw.abs_diff(sample.temperature_centi_c) <= 50);
            if !bounded_pps_pause
                && pps_wait_pending
                && self.first_hold_elapsed_ms.is_some()
                && sample
                    .raw_temperature_centi_c
                    .is_some_and(|raw| raw.abs_diff(sample.temperature_centi_c) <= 50)
            {
                self.hold_pps_wait_interruptions =
                    self.hold_pps_wait_interruptions.saturating_add(1);
                self.hold_pps_wait_pending = false;
            }
            if !bounded_pps_pause {
                self.interrupt_measurement_qualification();
            }
            return;
        }
        self.update_strategy_evidence(sample, expected_target);
        if sample.phase != ThermalTuningPhase::Warmup && !self.approach_curve_closed {
            self.record_approach_curve_sample(sample);
            self.approach_curve_closed = sample.phase == ThermalTuningPhase::Hold;
        }
        self.record_controller_hold_output(sample, expected_target);
        if self.first_hold_elapsed_ms.is_some() || self.min_hold_temperature_centi_c.is_some() {
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

    fn record_trial_phase(&mut self, sample: &ThermalTuningSample, expected_target: i16) {
        // A recovery into Approach ends this Hold segment even when its
        // temperature is retained during a PPS pause. A later fresh physical
        // Hold entry starts a new qualification clock, never a sum of segments.
        if sample.phase != ThermalTuningPhase::Hold {
            self.first_hold_elapsed_ms = None;
        }
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
        if let Some(started) = self.first_hold_elapsed_ms {
            self.longest_qualified_hold_ms = self
                .longest_qualified_hold_ms
                .max(sample.elapsed_ms.saturating_sub(started));
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

    fn deadline_temperature_evidence(&self) -> Option<i16> {
        // Use the same maximum observation gap as the stable-window witness.
        // A formerly accepted value several seconds before the deadline is
        // not evidence of the temperature at that deadline, even when every
        // later held value is valid for conservative control.
        (self.deadline_nearest_distance_ms <= 1_000)
            .then_some(self.deadline_nearest_temperature_centi_c)
            .flatten()
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

    fn trial_progress(
        &self,
        target_c: i16,
        completed: bool,
        settle_ms: Option<f64>,
        limit_ms: f64,
        low_margin: bool,
    ) -> (f64, f64) {
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
            } else if let Some(deadline_temp) = self.deadline_temperature_evidence() {
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
        (progress_base, temperature_gap_c)
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
        let (progress_base, temperature_gap_c) =
            self.trial_progress(target_c, completed, settle_ms, limit_ms, low_margin);
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
            if completed {
                0.0
            } else if self.warmup_exited_at_ms.is_some() {
                1.0
            } else {
                // A Warmup timeout has no Approach deadline temperature.
                // Its missing-data sentinel cannot beat measured progress.
                2.0
            },
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
                self.activate_initial_target_seed(target, lower, upper)?;
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

    fn activate_initial_target_seed(
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
        point_at(lower).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        point_at(upper).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        point_at(target).ok_or(ThermalTuningError::CandidateProfileRequired)?;
        // Bounds authorize this interval; each untouched target already contains its
        // own calibration-derived seed. Interpolation would erase that model input.
        self.candidate_hash = hash_profile(profile);
        self.preview_update_pending = true;
        self.preview_receipt = None;
        self.seed_source = bounded_reason("initial_target_seed");
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
        self.proposed_points.clear();
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

fn braking_failure_weight(evidence: TuneEvidence, high_side: bool) -> Option<u16> {
    if high_side {
        return evidence
            .over_c
            .is_finite()
            .then_some((evidence.over_c * 100.0 + 0.5) as u16)
            .filter(|weight| *weight > 0);
    }
    if evidence.hold_sample_count != 0 || evidence.first_hold_after_warmup_ms.is_some() {
        return None;
    }
    let temperature = evidence.deadline_temperature_centi_c?;
    let minimum = i32::from(evidence.target_c) * 100 - (HOLD_ENTRY_BELOW_C * 100.0) as i32;
    u16::try_from(minimum - i32::from(temperature))
        .ok()
        .filter(|weight| *weight > 0)
}

fn weighted_braking_value(
    high: ObservedDampingFailure,
    low: ObservedDampingFailure,
) -> Option<u16> {
    let high_error = u32::from(high.braking_error_centi_c?);
    let low_error = u32::from(low.braking_error_centi_c?);
    let width = low
        .point
        .brake_distance_centi_c
        .checked_sub(high.point.brake_distance_centi_c)?;
    if width < 2 || high_error == 0 || low_error == 0 {
        return None;
    }
    let offset = (u32::from(width) * high_error / (high_error + low_error)) as u16;
    let upper = ((u32::from(width) * 3 / 4) as u16).min(width - 1).max(1);
    let offset = offset.clamp((width / 4).max(1), upper);
    Some(high.point.brake_distance_centi_c + offset)
}

fn late_stability_handoff_failure(evidence: TuneEvidence) -> bool {
    evidence.tunable_stop
        && evidence.full_speed_failed
        && !evidence.stable_window_verified
        && evidence.hold_sample_count >= 2
        && evidence.overshoot_c <= 1.5
        && evidence.hold_p2p_c <= 3.0
        && evidence.under_c <= 1.5
        && evidence.over_c <= 1.5
        && evidence.first_band_exit_centi_c.is_none()
        && evidence
            .first_hold_after_warmup_ms
            .is_some_and(|time| time <= evidence.settle_limit_ms)
        && evidence
            .stable_window_after_warmup_ms
            .is_some_and(|time| time > evidence.settle_limit_ms)
}

fn interrupted_stability_handoff_failure(evidence: TuneEvidence) -> bool {
    evidence.tunable_stop
        && evidence.target_c <= 150
        && evidence.full_speed_failed
        && !evidence.stable_window_verified
        && !evidence.approach_floor_only
        && evidence
            .approach_output_range
            .is_some_and(|(min, max)| min >= 50 && max <= 1_000)
        && evidence.stable_window_after_warmup_ms.is_none()
        && evidence.guarded_hold_handoff
        && (1..=2).contains(&evidence.hold_sample_count)
        && evidence.overshoot_c <= 1.5
        && evidence.hold_p2p_c <= 3.0
        && evidence.under_c <= 1.5
        && evidence.over_c <= 1.5
        && evidence.first_band_observed
        && evidence.first_band_exit_centi_c.is_none()
        && evidence
            .first_hold_after_warmup_ms
            .is_some_and(|time| time <= evidence.settle_limit_ms)
        && evidence
            .deadline_temperature_centi_c
            .is_some_and(|temperature| {
                let error = f64::from(evidence.target_c) - f64::from(temperature) / 100.0;
                (-1.5..=f64::from(HOLD_ENTRY_BELOW_C)).contains(&error)
            })
}

fn in_band_interrupted_stability_failure(evidence: TuneEvidence) -> bool {
    evidence.hold_sample_count >= 30 && in_band_interrupted_stability_temperature(evidence)
}

fn in_band_interrupted_stability_temperature(evidence: TuneEvidence) -> bool {
    evidence.tunable_stop
        && evidence.full_speed_failed
        && !evidence.stable_window_verified
        && evidence.stable_window_after_warmup_ms.is_none()
        && evidence.overshoot_c <= 1.5
        && evidence.hold_p2p_c <= 3.0
        && evidence.under_c <= 1.5
        && evidence.over_c <= 1.5
        && evidence.first_band_observed
        && evidence.first_band_exit_centi_c.is_none()
        && evidence
            .approach_output_range
            .is_some_and(|(min, max)| min >= 50 && max <= 1_000)
        && evidence
            .first_hold_after_warmup_ms
            .is_some_and(|time| time <= evidence.settle_limit_ms)
        && evidence
            .deadline_temperature_centi_c
            .is_some_and(|temperature| {
                (i32::from(temperature) - i32::from(evidence.target_c) * 100).abs() <= 150
            })
}

fn damping_failure_side(evidence: TuneEvidence) -> Option<bool> {
    if evidence.approach_floor_only {
        return None;
    }
    bracket_failure_side(evidence)
}

fn bracket_failure_side(evidence: TuneEvidence) -> Option<bool> {
    if evidence.target_c > 150
        || !evidence.tunable_stop
        || !evidence.full_speed_failed
        || evidence.stable_window_verified
        || (evidence.approach_floor_only && evidence.approach_output_range != Some((50, 50)))
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
    } else if late_stability_handoff_failure(evidence)
        || interrupted_stability_handoff_failure(evidence)
        || unobserved_low_side_failure(evidence)
        || (evidence.hold_sample_count < 3
            && evidence.first_band_exit_centi_c.is_none()
            && evidence.overshoot_c <= 0.5
            && evidence
                .first_hold_after_warmup_ms
                .is_some_and(|after| after > evidence.settle_limit_ms)
            && evidence
                .stable_window_after_warmup_ms
                .is_some_and(|after| after > evidence.settle_limit_ms))
        || (evidence.hold_sample_count == 0
            && evidence.first_hold_after_warmup_ms.is_none()
            && evidence.first_band_exit_centi_c.is_none()
            && evidence.overshoot_c <= 0.5
            && evidence
                .deadline_temperature_centi_c
                .is_some_and(|temperature| {
                    f32::from(evidence.target_c) - f32::from(temperature) / 100.0
                        > HOLD_ENTRY_BELOW_C
                }))
    {
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
                    >= current_facts.hold_sample_count.saturating_mul(9))
            // Use the same limited-supply evidence that selected damping.
            // Otherwise a losing comparison cannot escape that search axis.
            || limited_hold_outputs(current_facts))
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
#[path = "thermal_tuning_tests.rs"]
mod tests;
