use core::{cmp::Ordering, fmt::Write};

use heapless::{String, Vec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapters::pd::{FUSB302B_PD_ABSOLUTE_MAX_MV, FUSB302B_PD_ABSOLUTE_MIN_MV};
use crate::memory::{ThermalControlProfileConfig, ThermalControlProfilePointConfig};

pub const THERMAL_TUNING_TARGETS_C: [i16; 9] = [60, 80, 100, 120, 140, 160, 180, 220, 240];
pub const THERMAL_TUNING_EXECUTION_ORDER_C: [i16; 9] = [60, 240, 140, 100, 80, 120, 180, 160, 220];
pub const THERMAL_TUNING_MAX_TARGETS: usize = 9;
pub const THERMAL_TUNING_EVENT_CAPACITY: usize = 256;
// Samples carry the full local PPS/control evidence and must fit one bounded
// JSONL response. Hosts page through the stream with afterSequence.
pub const THERMAL_TUNING_EVENT_PAGE_SIZE: usize = 4;
pub const THERMAL_TUNING_HOLD_CONFIRM_MS: u64 = 60_000;
pub const THERMAL_TUNING_SCOUT_HOLD_MS: u64 = 12_000;
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
    pub end_sequence: Option<u32>,
    pub end_elapsed_ms: Option<u64>,
    pub sample_start_sequence: Option<u32>,
    pub sample_end_sequence: Option<u32>,
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
pub struct ThermalTuningSample {
    pub sequence: u32,
    pub elapsed_ms: u64,
    pub target_c: i16,
    pub trial_index: u8,
    pub candidate_id: TuningId,
    pub candidate_hash: TuningHash,
    pub temperature_centi_c: i16,
    pub vin_mv: u32,
    pub pps_contract: ThermalTuningPpsContract,
    pub requested_mv: u16,
    pub pps_power_ceiling_mv: u32,
    pub heater_requested_output_permille: u16,
    pub heater_output_permille: u16,
    pub measurement_valid: bool,
    pub phase: ThermalTuningPhase,
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
    batch_predicted_profile: Option<ThermalControlProfileConfig>,
    batch_current_score: Option<[f64; 11]>,
    batch_current_confirmable: bool,
    pub seed_source: TuningReason,
    pub preview_update_pending: bool,
    pub trial_start_sequence: Option<u32>,
    pub trial_start_elapsed_ms: Option<u64>,
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
    approach_curve_samples: Vec<ThermalCurveSample, THERMAL_TUNING_APPROACH_TRACE_CAPACITY>,
    approach_curve_closed: bool,
    approach_curve_overflow: bool,
    approach_tail_abs_error_sum_c: f64,
    approach_tail_count: u32,
    approach_curve_last_elapsed_ms: Option<u64>,
    hold_output_histogram: [u16; 1001],
    hold_output_count: u32,
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
            batch_predicted_profile: None,
            batch_current_score: None,
            batch_current_confirmable: false,
            seed_source: TuningReason::new(),
            preview_update_pending: false,
            trial_start_sequence: None,
            trial_start_elapsed_ms: None,
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
            approach_curve_samples: Vec::new(),
            approach_curve_closed: false,
            approach_curve_overflow: false,
            approach_tail_abs_error_sum_c: 0.0,
            approach_tail_count: 0,
            approach_curve_last_elapsed_ms: None,
            hold_output_histogram: [0; 1001],
            hold_output_count: 0,
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
        if power_class != ThermalTuningPowerClass::Pps5a {
            self.fail("pps3a_not_supported");
            return Err(ThermalTuningError::InvalidPowerClass);
        }
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
        if !pps_ready {
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
        self.push_phase_transition_event("cooling before first trial");
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
        let full_speed_limit_ms: u32 = if target_c > 150 { 5_000 } else { 10_000 };
        let warmup_full = self.warmup_full_output && !self.warmup_full_output_invalid;
        let approach_to_hold = self.saw_approach && self.saw_hold;
        let scout_hold_complete = self.first_hold_elapsed_ms.is_some_and(|started| {
            elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_SCOUT_HOLD_MS
        });
        let hold_confirm = self.first_hold_elapsed_ms.is_some_and(|started| {
            elapsed_ms.saturating_sub(started) >= THERMAL_TUNING_HOLD_CONFIRM_MS
        });
        let budget_available = self
            .current_elapsed_ms
            .saturating_sub(self.target_started_elapsed_ms)
            < THERMAL_TUNING_TARGET_BUDGET_MS;
        let metric_passes = budget_available
            && warmup_full
            && !self.approach_curve_overflow
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
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            ),
            ThermalTuningTrialPurpose::BatchCurrent => {
                self.batch_current_score = Some(score);
                self.batch_current_confirmable = passes;
                if let Some(predicted) = self.batch_predicted_profile {
                    self.install_candidate_profile(predicted, "predicted_from_scout")?;
                    self.schedule_trial(
                        ThermalTuningTrialPurpose::BatchPredicted,
                        "cooling before predicted candidate",
                    )
                } else {
                    self.finish_candidate_batch(target_c, score, passes)
                }
            }
            ThermalTuningTrialPurpose::BatchPredicted => {
                self.finish_candidate_batch(target_c, score, passes)
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
            self.mutate_candidate_for_failure(
                target_c,
                max_overshoot_centi_c,
                hold_peak_to_peak_centi_c,
                settle_time_ms,
                full_speed_limit_ms,
            );
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
        profile: ThermalControlProfileConfig,
        seed_source: &str,
    ) -> Result<(), ThermalTuningError> {
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
        overshoot_centi_c: Option<u16>,
        hold_p2p_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) -> Result<(), ThermalTuningError> {
        let current = self
            .candidate_profile
            .ok_or(ThermalTuningError::CandidateProfileRequired)?;
        self.mutate_candidate_for_failure(
            target_c,
            overshoot_centi_c,
            hold_p2p_centi_c,
            settle_time_ms,
            full_speed_limit_ms,
        );
        let predicted = self.candidate_profile.filter(|profile| *profile != current);
        self.batch_current_profile = Some(current);
        self.batch_predicted_profile = predicted;
        self.batch_current_score = None;
        self.batch_current_confirmable = false;
        self.install_candidate_profile(current, "batch_current")?;
        self.schedule_trial(
            ThermalTuningTrialPurpose::BatchCurrent,
            "cooling before current candidate comparison",
        )
    }

    fn finish_candidate_batch(
        &mut self,
        target_c: i16,
        score: [f64; 11],
        confirmable: bool,
    ) -> Result<(), ThermalTuningError> {
        let current_score = self.batch_current_score.unwrap_or(score);
        let current_confirmable = self.batch_current_confirmable;
        let predicted_wins = self.trial_purpose == ThermalTuningTrialPurpose::BatchPredicted
            && (confirmable && !current_confirmable
                || confirmable == current_confirmable && score_precedes(&score, &current_score));
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
        let selected_confirmable = if predicted_wins {
            confirmable
        } else {
            current_confirmable
        };
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

    fn mutate_candidate_for_failure(
        &mut self,
        target_c: i16,
        max_overshoot_centi_c: Option<u16>,
        hold_peak_to_peak_centi_c: Option<u16>,
        settle_time_ms: Option<u64>,
        full_speed_limit_ms: u32,
    ) {
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
        if max_overshoot_centi_c.is_some_and(|value| value > 300) {
            point.brake_distance_centi_c =
                point.brake_distance_centi_c.saturating_add(12).min(5_000);
            point.approach_power_permille = point.approach_power_permille.saturating_sub(10);
            point.hold_power_permille = point.hold_power_permille.saturating_sub(8);
            point.hold_reheat_power_permille = point.hold_reheat_power_permille.saturating_sub(8);
            point.approach_lead_ticks = point.approach_lead_ticks.saturating_add(1).min(255);
        }
        if hold_peak_to_peak_centi_c.is_some_and(|value| value > 300) {
            point.hold_power_permille = point.hold_power_permille.saturating_sub(6);
            point.hold_reheat_power_permille = point.hold_reheat_power_permille.saturating_sub(6);
            point.hold_blend_ticks = point.hold_blend_ticks.saturating_add(1).min(255);
        }
        if settle_time_ms.is_none_or(|value| value > u64::from(full_speed_limit_ms)) {
            point.approach_power_permille =
                point.approach_power_permille.saturating_add(12).min(1_000);
            point.approach_floor_power_permille = point
                .approach_floor_power_permille
                .saturating_add(8)
                .min(1_000);
            point.approach_lead_ticks = point.approach_lead_ticks.saturating_add(1).min(255);
        }
        if let Some(hash) = hash_profile(profile) {
            self.candidate_hash = Some(hash);
            self.preview_update_pending = true;
            self.preview_receipt = None;
        }
    }

    fn reset_trial_metrics(&mut self) {
        self.trial_start_sequence = Some(self.next_sequence);
        self.trial_start_elapsed_ms = None;
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
        self.approach_curve_samples.clear();
        self.approach_curve_closed = false;
        self.approach_curve_overflow = false;
        self.approach_tail_abs_error_sum_c = 0.0;
        self.approach_tail_count = 0;
        self.approach_curve_last_elapsed_ms = None;
        self.hold_output_histogram = [0; 1001];
        self.hold_output_count = 0;
        self.first_band_elapsed_ms = None;
        self.first_band_exit_centi_c = None;
        self.deadline_nearest_distance_ms = u64::MAX;
        self.deadline_nearest_temperature_centi_c = None;
    }

    pub fn record_sample(
        &mut self,
        mut sample: ThermalTuningSample,
    ) -> Result<(), ThermalTuningError> {
        let origin_ms = self.sample_clock_origin_ms.unwrap_or(0);
        sample.elapsed_ms = sample.elapsed_ms.saturating_sub(origin_ms);
        let expected_target = self.validate_sample(&sample)?;
        if self.phase == ThermalTuningPhase::Cooldown {
            return self.record_cooldown_sample(sample, expected_target);
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
        {
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
        {
            self.finish_trial(sample_elapsed_ms)?;
        }
        Ok(())
    }

    fn full_speed_to_stable_timed_out(&self, elapsed_ms: u64, target_c: i16) -> bool {
        let Some(warmup_exit_ms) = self.warmup_exited_at_ms else {
            return false;
        };
        if self.stable_window_verified_at_ms.is_some() {
            return false;
        }
        let limit_ms = if target_c > 150 { 5_000 } else { 10_000 };
        let deadline_ms = warmup_exit_ms.saturating_add(limit_ms);
        elapsed_ms >= deadline_ms
            && self
                .stable_window_started_at_ms
                .is_none_or(|started_ms| started_ms > deadline_ms)
    }

    fn validate_sample(&mut self, sample: &ThermalTuningSample) -> Result<i16, ThermalTuningError> {
        if !self.armed {
            return Err(ThermalTuningError::NotRunning);
        }
        if sample.pps_contract.kind != ThermalTuningPowerClass::Pps5a
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
    ) -> Result<(), ThermalTuningError> {
        if sample.heater_output_permille != 0 || sample.heater_requested_output_permille != 0 {
            self.fail("cooldown_output_not_zero");
            return Err(ThermalTuningError::InvalidTransition);
        }
        self.current_elapsed_ms = sample.elapsed_ms;
        sample.sequence = self.next_sequence;
        sample.phase = ThermalTuningPhase::Cooldown;
        let cooled = i32::from(sample.temperature_centi_c)
            <= i32::from(cooldown_threshold_c(expected_target)) * 100;
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
        if cooled && self.armed {
            self.phase = ThermalTuningPhase::Warmup;
            self.reset_trial_metrics();
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
                self.saw_approach = true;
                self.warmup_exited_at_ms.get_or_insert(sample.elapsed_ms);
            }
            ThermalTuningPhase::Hold => {
                self.saw_hold = true;
                self.warmup_exited_at_ms.get_or_insert(sample.elapsed_ms);
                self.first_hold_elapsed_ms.get_or_insert(sample.elapsed_ms);
            }
            _ => {}
        }
        if sample.phase != ThermalTuningPhase::Warmup && !self.approach_curve_closed {
            self.record_approach_curve_sample(sample);
            self.approach_curve_closed = sample.phase == ThermalTuningPhase::Hold;
        }
        if self.first_hold_elapsed_ms.is_some() {
            let output = usize::from(sample.heater_output_permille).min(1_000);
            self.hold_output_histogram[output] =
                self.hold_output_histogram[output].saturating_add(1);
            self.hold_output_count = self.hold_output_count.saturating_add(1);
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
        self.update_stability_evidence(sample, expected_target);
        self.max_temperature_centi_c = Some(
            self.max_temperature_centi_c
                .map_or(sample.temperature_centi_c, |value| {
                    value.max(sample.temperature_centi_c)
                }),
        );
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
            let target_centi_c = i32::from(sample.target_c) * 100;
            self.approach_tail_abs_error_sum_c +=
                f64::from((i32::from(sample.temperature_centi_c) - target_centi_c).abs()) / 100.0;
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
        if distance > i32::from(THERMAL_TUNING_STABLE_BAND_CENTI_C) {
            if self.stable_window_verified_at_ms.is_none() {
                self.stable_window_started_at_ms = None;
            }
            return;
        }
        let stable_start = *self
            .stable_window_started_at_ms
            .get_or_insert(sample.elapsed_ms);
        let full_speed_limit_ms = if expected_target > 150 {
            5_000_u64
        } else {
            10_000_u64
        };
        if stable_start.saturating_sub(warmup_exit) <= full_speed_limit_ms
            && sample.elapsed_ms.saturating_sub(stable_start) >= THERMAL_TUNING_STABLE_WINDOW_MS
        {
            self.stable_window_verified_at_ms = Some(sample.elapsed_ms);
        }
    }

    fn update_stability_evidence(&mut self, sample: &ThermalTuningSample, target_c: i16) {
        let Some(warmup_exit) = self.warmup_exited_at_ms else {
            return;
        };
        if sample.elapsed_ms < warmup_exit {
            return;
        }
        let limit_ms = if target_c > 150 { 5_000 } else { 10_000 };
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
        let missing = 1.0e12;
        let limit_ms = if target_c > 150 { 5_000.0 } else { 10_000.0 };
        let margin_ms = if target_c > 150 { 500.0 } else { 1_000.0 };
        let overshoot_c = overshoot_centi_c.map(|value| f64::from(value) / 100.0);
        let hold_p2p_c = hold_p2p_centi_c.map(|value| f64::from(value) / 100.0);
        let settle_ms = settle_time_ms.map(|value| value as f64);
        let metric_gate = completed
            && overshoot_c.is_some_and(|value| value <= 3.0)
            && hold_p2p_c.is_some_and(|value| value <= 3.0)
            && settle_ms.is_some_and(|value| value <= limit_ms);
        let gate_penalty = if metric_gate {
            0.0
        } else {
            1.0 + overshoot_c.map_or(3.0, |value| (value - 3.0).max(0.0))
                + hold_p2p_c.map_or(3.0, |value| (value - 3.0).max(0.0))
                + settle_ms.map_or(2.0, |value| {
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
        [
            0.0,
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
        ]
    }

    pub fn advance_target(&mut self) -> Result<(), ThermalTuningError> {
        if !self.armed {
            return Err(ThermalTuningError::NotRunning);
        }
        while self.target_index + 1 < self.targets_c.len() {
            self.target_index += 1;
            self.trial_index = 0;
            self.trial_purpose = ThermalTuningTrialPurpose::Scout;
            self.batch_current_profile = None;
            self.batch_predicted_profile = None;
            self.batch_current_score = None;
            self.batch_current_confirmable = false;
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
            140 => Some((60, 240)),
            100 => Some((60, 140)),
            80 => Some((60, 100)),
            120 => Some((100, 140)),
            180 => Some((140, 240)),
            160 => Some((140, 180)),
            220 => Some((180, 240)),
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
            self.sealed = true;
            self.phase = ThermalTuningPhase::Sealed;
            self.terminal_disposition = ThermalTuningTerminalDisposition::CandidateReady;
        }
    }

    pub fn seal(&mut self) -> Result<(), ThermalTuningError> {
        if self.sealed
            && self.terminal_disposition == ThermalTuningTerminalDisposition::CandidateReady
        {
            return Ok(());
        }
        Err(ThermalTuningError::InvalidTransition)
    }

    pub fn cancel(&mut self, reason: &str) -> Result<(), ThermalTuningError> {
        if !self.armed && !self.sealed {
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
            bank: ThermalTuningPowerClass::Pps5a,
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
        let applied_hash = self.candidate_profile.as_ref().and_then(hash_profile);
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
        self.batch_current_profile = None;
        self.batch_predicted_profile = None;
        self.batch_current_score = None;
        self.batch_current_confirmable = false;
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
        let record = ThermalTuningTrialRecord {
            candidate: self.candidate_snapshot(target_c),
            trial_index,
            purpose: self.trial_purpose,
            start_sequence: self.next_sequence,
            start_elapsed_ms: self.current_elapsed_ms,
            end_sequence: None,
            end_elapsed_ms: None,
            sample_start_sequence: None,
            sample_end_sequence: None,
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
            end_sequence: Some(self.next_sequence),
            end_elapsed_ms: Some(elapsed_ms),
            sample_start_sequence: self.trial_sample_start_sequence,
            sample_end_sequence: self.trial_sample_end_sequence,
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
    if target_c < 80 { 35 } else { target_c - 40 }
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

fn score_precedes(left: &[f64; 11], right: &[f64; 11]) -> bool {
    left.iter()
        .zip(right)
        .map(|(left, right)| left.partial_cmp(right).unwrap_or(Ordering::Equal))
        .find(|order| *order != Ordering::Equal)
        == Some(Ordering::Less)
}

pub fn hash_profile(profile: &ThermalControlProfileConfig) -> Option<TuningHash> {
    let mut hasher = Sha256::new();
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

fn chained_event_digest(previous: &TuningHash, event: &ThermalTuningEvent) -> TuningHash {
    let mut hasher = Sha256::new();
    hasher.update(previous.as_bytes());
    let mut encoded = [0_u8; 4096];
    if let Ok(length) = serde_json_core::to_slice(event, &mut encoded) {
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
    use super::*;
    use crate::memory::{ThermalControlProfilePointConfig, ThermalControlProfileSettingsConfig};

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
                Some(point(240)),
                None,
            ],
        };
        assert_eq!(
            hash_profile(&profile).unwrap().as_str(),
            "sha256:841b20a8fec5278e37558f158496b234c89a2aca65f4393eb1a40fb4f6e40141"
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
            "6ce5cc6485ed1f4d4a7d9e35e2ce766386da295efec80563c2eaa1ad47408b75"
        );
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

    fn sample(
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
            phase,
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

    fn started_single_target_runtime() -> ThermalTuningRuntime {
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
                candidate_profile: Some(profile_with_targets(&[60])),
            })
            .unwrap();
        record_initial_cooldown(&mut runtime, 0);
        runtime
    }

    #[test]
    fn full_speed_to_stable_timeout_rejects_trial_and_cools_for_retry() {
        let mut runtime = started_single_target_runtime();
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
    fn eleven_component_score_uses_local_hold_and_approach_evidence() {
        let mut runtime = ThermalTuningRuntime::new();
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
            (10_700, ThermalTuningPhase::Hold, 6_000),
            (12_700, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::BatchCurrent
        );
        assert_eq!(runtime.trial_index, 1);
        assert!(runtime.batch_predicted_profile.is_some());

        record_current_trial_sample(&mut runtime, 13_000, ThermalTuningPhase::Cooldown, 3_400);
        for (elapsed, phase, temperature) in [
            (13_100, ThermalTuningPhase::Warmup, 3_000),
            (13_400, ThermalTuningPhase::Approach, 5_500),
            (13_700, ThermalTuningPhase::Hold, 6_000),
            (23_700, ThermalTuningPhase::Hold, 6_000),
            (25_700, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::BatchPredicted
        );
        assert_eq!(runtime.trial_index, 2);
        assert!(runtime.preview_update_pending);

        record_current_trial_sample(&mut runtime, 26_000, ThermalTuningPhase::Cooldown, 3_400);
        for (elapsed, phase, temperature) in [
            (26_100, ThermalTuningPhase::Warmup, 3_000),
            (26_400, ThermalTuningPhase::Approach, 5_500),
            (26_700, ThermalTuningPhase::Hold, 6_400),
            (26_800, ThermalTuningPhase::Hold, 6_000),
            (36_800, ThermalTuningPhase::Hold, 6_000),
            (38_800, ThermalTuningPhase::Hold, 6_000),
        ] {
            record_current_trial_sample(&mut runtime, elapsed, phase, temperature);
        }
        assert_eq!(
            runtime.trial_purpose,
            ThermalTuningTrialPurpose::HoldConfirm
        );
        assert_eq!(runtime.trial_index, 3);
        assert_eq!(runtime.seed_source.as_str(), "batch_selected_current");
        assert!(
            runtime.events.iter().any(|event| {
                event.reason.as_deref() == Some("candidate_batch_selected_current")
            })
        );
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Running
        );
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
            (11_100, ThermalTuningPhase::Hold, 6_000),
            (
                THERMAL_TUNING_TARGET_BUDGET_MS,
                ThermalTuningPhase::Hold,
                6_000,
            ),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
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
        assert_eq!(runtime.targets_c[runtime.target_index], 240);
        runtime.failed_target_mask = target_bit(240);
        runtime.advance_target().unwrap();
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
        assert!(!runtime.armed);
        assert_eq!(runtime.failed_target_mask.count_ones(), 8);
        assert_eq!(
            runtime
                .events
                .iter()
                .filter(|event| {
                    event.reason.as_deref() == Some("interval_pruned_without_accepted_bounds")
                })
                .count(),
            7
        );
    }

    #[test]
    fn full_scope_midpoint_uses_accepted_parent_bounds() {
        let mut runtime = ThermalTuningRuntime::new();
        let mut profile = profile_with_targets(&THERMAL_TUNING_TARGETS_C);
        profile.points[0].as_mut().unwrap().brake_distance_centi_c = 400;
        profile.points[8].as_mut().unwrap().brake_distance_centi_c = 800;
        profile.points[4].as_mut().unwrap().brake_distance_centi_c = 300;
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
        runtime.accepted_target_mask |= target_bit(240);
        runtime.advance_target().unwrap();
        assert_eq!(runtime.targets_c[runtime.target_index], 140);
        let snapshot = runtime.candidate_snapshot(Some(140));
        assert_eq!(
            (snapshot.lower_target_c, snapshot.upper_target_c),
            (Some(60), Some(240))
        );
        assert_eq!(snapshot.seed_source, "accepted_bounds_linear");
        assert_eq!(snapshot.point.unwrap().brake_distance_centi_c, 578);
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
        runtime
            .record_sample(sample(11_000, ThermalTuningPhase::Hold, 6_000))
            .unwrap();
        runtime
            .record_sample(sample(19_000, ThermalTuningPhase::Hold, 6_010))
            .unwrap();
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
        let profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
        let candidate_hash = hash_profile(&profile).unwrap();
        let mut runtime = ThermalTuningRuntime::new();
        runtime
            .start(ThermalTuningStart {
                run_id: id("run-save"),
                candidate_id: id("candidate-save"),
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
        let delivered = runtime.events_after(None).next_sequence;
        runtime.events_after(delivered);
        let receipt = runtime
            .confirm_save(ThermalTuningSaveConfirmation {
                run_id: "run-save",
                candidate_id: "candidate-save",
                candidate_hash: candidate_hash.as_str(),
                power_class: ThermalTuningPowerClass::Pps5a,
                readback_verified: true,
                bank_revision: 17,
                bank_hash: candidate_hash.clone(),
            })
            .unwrap();
        assert_eq!(receipt.bank_revision, 17);
        assert_eq!(receipt.bank_hash, candidate_hash);
        assert_eq!(receipt.bank, ThermalTuningPowerClass::Pps5a);
        assert!(receipt.atomic && receipt.readback_verified);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Passed
        );
    }

    #[test]
    fn save_rejects_delivered_but_unacknowledged_events() {
        let profile = profile_with_targets(&THERMAL_TUNING_EXECUTION_ORDER_C);
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
    fn rejects_pps3a_without_arming() {
        let mut runtime = ThermalTuningRuntime::new();
        let result = runtime.start(ThermalTuningStart {
            run_id: id("run-3a"),
            candidate_id: id("candidate-3a"),
            candidate_hash: hash("hash-3a"),
            power_class: ThermalTuningPowerClass::Pps3a,
            targets: &THERMAL_TUNING_TARGETS_C,
            pps_ready: true,
            thermal_model_available: true,
            calibration_conflict: false,
            candidate_profile: None,
        });
        assert_eq!(result, Err(ThermalTuningError::InvalidPowerClass));
        assert!(!runtime.armed);
        assert_eq!(runtime.phase, ThermalTuningPhase::Failed);
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
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
            phase: ThermalTuningPhase::Warmup,
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
            Err(ThermalTuningError::InvalidTargets)
        );
        assert_eq!(
            runtime.terminal_disposition,
            ThermalTuningTerminalDisposition::Failed
        );
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
            (10_600, ThermalTuningPhase::Hold, 6_000),
            (12_600, ThermalTuningPhase::Hold, 6_000),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
        record_initial_cooldown(&mut runtime, 12_900);
        for (elapsed_ms, phase, temperature) in [
            (13_000, ThermalTuningPhase::Warmup, 3_000),
            (13_300, ThermalTuningPhase::Approach, 5_500),
            (13_600, ThermalTuningPhase::Hold, 6_000),
            (23_600, ThermalTuningPhase::Hold, 6_000),
            (73_600, ThermalTuningPhase::Hold, 6_000),
        ] {
            let mut next = sample(elapsed_ms, phase, temperature);
            next.trial_index = 1;
            runtime.record_sample(next).unwrap();
        }
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
            (10_600, ThermalTuningPhase::Hold, 6_000),
            (11_000, ThermalTuningPhase::Hold, 6_400),
            (60_600, ThermalTuningPhase::Hold, 6_400),
        ] {
            runtime
                .record_sample(sample(elapsed_ms, phase, temperature))
                .unwrap();
        }
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
        for _ in 0..200 {
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
