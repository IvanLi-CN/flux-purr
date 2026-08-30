use alloc::string::ToString;
use heapless::{String, Vec};

use crate::control_plane::{
    THERMAL_TUNING_PROFILE_CANONICAL_HEX_LEN, ThermalTuningCandidateWire,
    ThermalTuningEligibilityWire, ThermalTuningJournalWire, ThermalTuningPhaseWire,
    ThermalTuningPromotionStateWire, ThermalTuningReviewStateWire, ThermalTuningReviewWire,
    ThermalTuningRunSnapshotWire, ThermalTuningRunStateWire, ThermalTuningRunWire,
    ThermalTuningTargetDispositionWire, ThermalTuningTargetProgressWire,
    ThermalTuningTerminalDispositionWire, ThermalTuningTraceEventWire, ThermalTuningTraceKindWire,
    ThermalTuningTracePageWire,
};
use flux_purr_thermal_tuning_core::{
    CANDIDATE_LADDER_WIDTH, CandidateEvaluation, CandidateGates, CandidateIdentity, CandidatePoint,
    CandidateProfile, CandidateScore, DecisionEvent, EXECUTION_ORDER_C, Eligibility,
    HOLD_CONFIRM_SECONDS, MAX_HOLD_PEAK_TO_PEAK_CENTI, MAX_OVERSHOOT_CENTI, Phase, PpsPowerClass,
    PromotionError, PromotionState, RunState, SampleEvent, TARGET_BUDGET_SECONDS,
    TargetDisposition, TerminalDisposition, ThermalTuningCore, TraceError, TraceRecord,
};

pub const THERMAL_TUNING_TRACE_CAPACITY: usize = 96;
pub const THERMAL_TUNING_RUN_ID_MAX_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceRunOwner {
    None,
    ManualHeating,
    Calibration,
    Installation,
    ThermalTuning,
}

impl MaintenanceRunOwner {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ManualHeating => "manual_heating",
            Self::Calibration => "calibration",
            Self::Installation => "installation",
            Self::ThermalTuning => "thermal_tuning",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaintenanceRunArbiter {
    owner: MaintenanceRunOwner,
}

impl Default for MaintenanceRunArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl MaintenanceRunArbiter {
    pub const fn new() -> Self {
        Self {
            owner: MaintenanceRunOwner::None,
        }
    }

    pub const fn owner(self) -> MaintenanceRunOwner {
        self.owner
    }

    pub fn try_acquire(
        &mut self,
        requested: MaintenanceRunOwner,
    ) -> Result<(), MaintenanceRunOwner> {
        if matches!(requested, MaintenanceRunOwner::None) {
            return Err(self.owner);
        }
        if self.owner == MaintenanceRunOwner::None || self.owner == requested {
            self.owner = requested;
            Ok(())
        } else {
            Err(self.owner)
        }
    }

    pub fn release(&mut self, owner: MaintenanceRunOwner) {
        if self.owner == owner {
            self.owner = MaintenanceRunOwner::None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalTuningEligibility {
    pub thermal_model_ready: bool,
    pub curve_covers_all_targets: bool,
    pub pps3a_available: bool,
    pub pps5a_available: bool,
    pub idle: bool,
    pub measurement_safe: bool,
}

impl ThermalTuningEligibility {
    pub const fn for_class(self, power_class: PpsPowerClass) -> Eligibility {
        Eligibility {
            thermal_model_ready: self.thermal_model_ready,
            curve_covers_all_targets: self.curve_covers_all_targets,
            pps_class_available: match power_class {
                PpsPowerClass::Pps3a => self.pps3a_available,
                PpsPowerClass::Pps5a => self.pps5a_available,
            },
            idle: self.idle,
            measurement_safe: self.measurement_safe,
        }
    }

    pub fn wire(
        self,
        owner: MaintenanceRunOwner,
        power_class: Option<PpsPowerClass>,
    ) -> ThermalTuningEligibilityWire {
        let mut reasons = Vec::new();
        let class_ready = power_class
            .map_or(self.pps3a_available || self.pps5a_available, |class| {
                self.for_class(class).pps_class_available
            });
        let ready = self.thermal_model_ready
            && self.curve_covers_all_targets
            && class_ready
            && self.idle
            && self.measurement_safe
            && owner == MaintenanceRunOwner::None;
        if !self.thermal_model_ready {
            push_reason(&mut reasons, "thermal_model_unavailable");
        }
        if !self.curve_covers_all_targets {
            push_reason(&mut reasons, "heater_curve_incomplete");
        }
        if power_class.is_some_and(|class| !self.for_class(class).pps_class_available)
            || (power_class.is_none() && !class_ready)
        {
            push_reason(&mut reasons, "pps_power_class_unavailable");
        }
        if !self.idle {
            push_reason(&mut reasons, "device_not_idle");
        }
        if !self.measurement_safe {
            push_reason(&mut reasons, "measurement_or_safety_fault");
        }
        if owner != MaintenanceRunOwner::None {
            push_reason(&mut reasons, "tuning_busy");
        }
        ThermalTuningEligibilityWire {
            ready,
            reasons,
            active_owner: (owner != MaintenanceRunOwner::None).then(|| string(owner.as_str())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalTuningRuntimeError {
    Busy,
    PowerClassUnavailable,
    Ineligible,
    NotActive,
    Trace(TraceError),
    Promotion(PromotionError),
}

impl From<TraceError> for ThermalTuningRuntimeError {
    fn from(value: TraceError) -> Self {
        Self::Trace(value)
    }
}

impl From<PromotionError> for ThermalTuningRuntimeError {
    fn from(value: PromotionError) -> Self {
        Self::Promotion(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalTuningSample {
    pub elapsed_ms: u32,
    pub temperature_centi_c: i16,
    pub vin_mv: u16,
    pub pps_contract_mv: u16,
    pub pps_contract_ma: u16,
    pub heater_output_permille: u16,
    pub measurement_valid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThermalTuningJournal {
    pub last_run_id: Option<u64>,
    pub last_disposition: Option<TerminalDisposition>,
}

pub struct ThermalTuningRuntime {
    core: ThermalTuningCore<THERMAL_TUNING_TRACE_CAPACITY>,
    arbiter: MaintenanceRunArbiter,
    eligibility: ThermalTuningEligibility,
    journal: ThermalTuningJournal,
    target_started_ms: u32,
    hold_started_ms: Option<u32>,
    target_min_temp_centi: i16,
    target_max_temp_centi: i16,
    last_temp_centi: i16,
    max_overshoot_centi: i16,
    warmup_complete: bool,
    hold_error_sum_centi: i64,
    hold_sample_count: u32,
    hold_confirm_within_gate: bool,
    output_switches: u16,
    last_output_permille: Option<u16>,
    preview_profile: Option<CandidateProfile>,
    candidate_ladder: Option<[CandidatePoint; CANDIDATE_LADDER_WIDTH]>,
    candidate_evaluations: [Option<CandidateEvaluation>; CANDIDATE_LADDER_WIDTH],
    candidate_trial_index: usize,
    candidate_trial_started_ms: u32,
}

impl Default for ThermalTuningRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ThermalTuningRuntime {
    pub fn new() -> Self {
        Self {
            core: ThermalTuningCore::new(),
            arbiter: MaintenanceRunArbiter::new(),
            eligibility: ThermalTuningEligibility {
                thermal_model_ready: false,
                curve_covers_all_targets: false,
                pps3a_available: false,
                pps5a_available: false,
                idle: true,
                measurement_safe: false,
            },
            journal: ThermalTuningJournal::default(),
            target_started_ms: 0,
            hold_started_ms: None,
            target_min_temp_centi: i16::MAX,
            target_max_temp_centi: i16::MIN,
            last_temp_centi: 0,
            max_overshoot_centi: 0,
            warmup_complete: false,
            hold_error_sum_centi: 0,
            hold_sample_count: 0,
            hold_confirm_within_gate: true,
            output_switches: 0,
            last_output_permille: None,
            preview_profile: None,
            candidate_ladder: None,
            candidate_evaluations: [None; CANDIDATE_LADDER_WIDTH],
            candidate_trial_index: 0,
            candidate_trial_started_ms: 0,
        }
    }

    pub const fn core(&self) -> &ThermalTuningCore<THERMAL_TUNING_TRACE_CAPACITY> {
        &self.core
    }

    pub const fn candidate_profile(&self) -> Option<CandidateProfile> {
        self.core.candidate()
    }

    pub const fn owner(&self) -> MaintenanceRunOwner {
        self.arbiter.owner()
    }

    pub const fn journal(&self) -> ThermalTuningJournal {
        self.journal
    }

    pub fn set_eligibility(&mut self, eligibility: ThermalTuningEligibility) {
        self.eligibility = eligibility;
    }

    pub fn restore_journal(&mut self, run_id: u64, disposition: Option<TerminalDisposition>) {
        self.journal = ThermalTuningJournal {
            last_run_id: (run_id != 0).then_some(run_id),
            last_disposition: disposition,
        };
    }

    pub fn start(
        &mut self,
        run_id: u64,
        power_class: PpsPowerClass,
        now_ms: u32,
    ) -> Result<(), ThermalTuningRuntimeError> {
        if self.core.state() == RunState::Running {
            return Err(ThermalTuningRuntimeError::Busy);
        }
        self.arbiter
            .try_acquire(MaintenanceRunOwner::ThermalTuning)
            .map_err(|_| ThermalTuningRuntimeError::Busy)?;
        if !self.eligibility.for_class(power_class).pps_class_available {
            self.arbiter.release(MaintenanceRunOwner::ThermalTuning);
            return Err(ThermalTuningRuntimeError::PowerClassUnavailable);
        }
        if let Err(error) =
            self.core
                .start(run_id, power_class, self.eligibility.for_class(power_class))
        {
            self.arbiter.release(MaintenanceRunOwner::ThermalTuning);
            return Err(match error {
                flux_purr_thermal_tuning_core::StartError::Busy => ThermalTuningRuntimeError::Busy,
                flux_purr_thermal_tuning_core::StartError::Ineligible
                | flux_purr_thermal_tuning_core::StartError::UnsupportedPowerClass
                | flux_purr_thermal_tuning_core::StartError::InvalidTraceCapacity => {
                    ThermalTuningRuntimeError::Ineligible
                }
            });
        }
        self.journal.last_run_id = Some(run_id);
        self.journal.last_disposition = None;
        self.target_started_ms = now_ms;
        self.hold_started_ms = None;
        self.target_min_temp_centi = i16::MAX;
        self.target_max_temp_centi = i16::MIN;
        self.max_overshoot_centi = 0;
        self.warmup_complete = false;
        self.hold_error_sum_centi = 0;
        self.hold_sample_count = 0;
        self.hold_confirm_within_gate = true;
        self.output_switches = 0;
        self.last_output_permille = None;
        self.preview_profile = None;
        self.begin_candidate_trial(now_ms)?;
        Ok(())
    }

    pub fn cancel(&mut self, now_ms: u32) -> Result<(), ThermalTuningRuntimeError> {
        if self.core.state() != RunState::Running {
            return Err(ThermalTuningRuntimeError::NotActive);
        }
        self.record_terminal_decision(now_ms, TargetDisposition::Skipped, 0x0000)?;
        self.core.finish(TerminalDisposition::Cancelled)?;
        self.finish_terminal(TerminalDisposition::Cancelled);
        Ok(())
    }

    pub fn reset_interrupted(&mut self) {
        if self.core.state() == RunState::Running {
            let _ = self.core.finish(TerminalDisposition::InterruptedReset);
            self.finish_terminal(TerminalDisposition::InterruptedReset);
        }
        self.arbiter.release(MaintenanceRunOwner::ThermalTuning);
        self.preview_profile = None;
        self.candidate_ladder = None;
        self.candidate_evaluations = [None; CANDIDATE_LADDER_WIDTH];
        self.candidate_trial_index = 0;
    }

    pub fn tick(&mut self, sample: ThermalTuningSample) -> Result<(), ThermalTuningRuntimeError> {
        if self.core.state() != RunState::Running {
            return Ok(());
        }
        let power_class = self
            .core
            .power_class()
            .ok_or(ThermalTuningRuntimeError::NotActive)?;
        let (_, expected_ma) = power_class.nominal_contract();
        if !sample.measurement_valid
            || sample.pps_contract_mv < 18_000
            || sample.pps_contract_ma < expected_ma
        {
            self.record_terminal_decision(sample.elapsed_ms, TargetDisposition::Failed, 0x0001)?;
            self.core.finish(TerminalDisposition::SafetyDisarmed)?;
            self.finish_terminal(TerminalDisposition::SafetyDisarmed);
            return Ok(());
        }
        let phase = self.core.phase();
        self.core.record_sample(SampleEvent {
            elapsed_ms: sample.elapsed_ms,
            temperature_centi_c: sample.temperature_centi_c,
            vin_mv: sample.vin_mv,
            pps_contract_mv: sample.pps_contract_mv,
            pps_contract_ma: sample.pps_contract_ma,
            heater_output_permille: sample.heater_output_permille,
            measurement_valid: sample.measurement_valid,
            phase,
        })?;
        self.last_temp_centi = sample.temperature_centi_c;
        self.warmup_complete |= sample.heater_output_permille >= 1_000;
        let target_c = self
            .core
            .current_target()
            .ok_or(ThermalTuningRuntimeError::NotActive)?;
        let target_centi = target_c.saturating_mul(100);
        if sample.elapsed_ms.saturating_sub(self.target_started_ms) >= TARGET_BUDGET_SECONDS * 1_000
        {
            self.record_terminal_decision(sample.elapsed_ms, TargetDisposition::Failed, 0x0000)?;
            self.core.finish(TerminalDisposition::BudgetExhausted)?;
            self.finish_terminal(TerminalDisposition::BudgetExhausted);
            return Ok(());
        }
        let error = target_centi
            .saturating_sub(sample.temperature_centi_c)
            .abs();
        self.max_overshoot_centi = self.max_overshoot_centi.max(
            sample
                .temperature_centi_c
                .saturating_sub(target_centi)
                .max(0),
        );
        if self
            .last_output_permille
            .is_some_and(|previous| previous != sample.heater_output_permille)
        {
            self.output_switches = self.output_switches.saturating_add(1);
        }
        self.last_output_permille = Some(sample.heater_output_permille);
        match phase {
            Phase::CooldownWait
                if sample.temperature_centi_c <= target_centi.saturating_sub(500) =>
            {
                self.core.set_phase(Phase::Scout)?;
            }
            Phase::Scout if sample.elapsed_ms.saturating_sub(self.target_started_ms) >= 5_000 => {
                self.core.set_phase(Phase::Retune)?;
            }
            Phase::Retune if error <= MAX_OVERSHOOT_CENTI => {
                self.core.set_phase(Phase::HoldConfirm)?;
                self.hold_started_ms = Some(sample.elapsed_ms);
                self.target_min_temp_centi = sample.temperature_centi_c;
                self.target_max_temp_centi = sample.temperature_centi_c;
                self.hold_confirm_within_gate = true;
            }
            Phase::HoldConfirm => {
                if error > MAX_HOLD_PEAK_TO_PEAK_CENTI {
                    self.hold_confirm_within_gate = false;
                }
                self.target_min_temp_centi =
                    self.target_min_temp_centi.min(sample.temperature_centi_c);
                self.target_max_temp_centi =
                    self.target_max_temp_centi.max(sample.temperature_centi_c);
                self.hold_error_sum_centi =
                    self.hold_error_sum_centi.saturating_add(i64::from(error));
                self.hold_sample_count = self.hold_sample_count.saturating_add(1);
                let peak_to_peak = self
                    .target_max_temp_centi
                    .saturating_sub(self.target_min_temp_centi);
                let hold_elapsed_ms = self
                    .hold_started_ms
                    .map_or(0, |started| sample.elapsed_ms.saturating_sub(started));
                if hold_elapsed_ms >= HOLD_CONFIRM_SECONDS * 1_000 {
                    let ladder = self
                        .candidate_ladder
                        .or_else(|| self.core.candidate_ladder_for_current_target())
                        .ok_or(ThermalTuningRuntimeError::NotActive)?;
                    let hold_mean_absolute_error_centi = if self.hold_sample_count == 0 {
                        0
                    } else {
                        (self.hold_error_sum_centi / i64::from(self.hold_sample_count)) as i32
                    };
                    let settle_ms = self.hold_started_ms.map_or(
                        sample
                            .elapsed_ms
                            .saturating_sub(self.candidate_trial_started_ms),
                        |started| started.saturating_sub(self.candidate_trial_started_ms),
                    );
                    let target_settle_limit_ms = if target_c > 150 { 5_000 } else { 10_000 };
                    let gates = CandidateGates {
                        warmup_complete: self.warmup_complete,
                        stage_complete: true,
                        dynamic_settle: settle_ms <= target_settle_limit_ms,
                        overshoot: self.max_overshoot_centi <= MAX_OVERSHOOT_CENTI,
                        hold_peak_to_peak: peak_to_peak <= MAX_HOLD_PEAK_TO_PEAK_CENTI,
                        hold_confirm: error <= MAX_HOLD_PEAK_TO_PEAK_CENTI
                            && self.hold_confirm_within_gate,
                    };
                    let current_index = self
                        .candidate_trial_index
                        .min(CANDIDATE_LADDER_WIDTH.saturating_sub(1));
                    self.candidate_evaluations[current_index] = Some(CandidateEvaluation {
                        point: ladder[current_index],
                        score: CandidateScore {
                            max_overshoot_centi: i32::from(self.max_overshoot_centi),
                            hold_peak_to_peak_centi: i32::from(peak_to_peak),
                            settle_ms,
                            hold_mean_absolute_error_centi,
                            output_switches: self.output_switches,
                        },
                        gates,
                    });
                    if current_index + 1 < CANDIDATE_LADDER_WIDTH {
                        self.candidate_trial_index = current_index + 1;
                        self.core
                            .set_current_target_candidate(ladder[self.candidate_trial_index])?
                            .ok_or(ThermalTuningRuntimeError::NotActive)?;
                        self.reset_candidate_window(sample.elapsed_ms);
                        self.core.set_phase(Phase::Retune)?;
                        return Ok(());
                    }
                    let evaluations = core::array::from_fn(|index| {
                        self.candidate_evaluations[index].unwrap_or(CandidateEvaluation {
                            point: ladder[index],
                            score: CandidateScore::default(),
                            gates: CandidateGates::default(),
                        })
                    });
                    if self.core.freeze_current_target(&evaluations)?.is_none() {
                        self.core.complete_current_target(
                            TargetDisposition::Failed,
                            sample.elapsed_ms,
                            error as i32,
                            sample.heater_output_permille as i32,
                            self.max_overshoot_centi as i32,
                            peak_to_peak as i32,
                            sample.elapsed_ms.saturating_sub(self.target_started_ms),
                            hold_mean_absolute_error_centi,
                            self.output_switches,
                            gates.mask(),
                        )?;
                        if self.core.state() == RunState::Terminal {
                            self.finish_terminal(
                                self.core.terminal().unwrap_or(TerminalDisposition::Failed),
                            );
                        } else {
                            self.reset_target_window(sample.elapsed_ms);
                            self.begin_candidate_trial(sample.elapsed_ms)?;
                        }
                        return Ok(());
                    }
                    self.core.complete_current_target(
                        TargetDisposition::Accepted,
                        sample.elapsed_ms,
                        error as i32,
                        sample.heater_output_permille as i32,
                        self.max_overshoot_centi as i32,
                        peak_to_peak as i32,
                        sample.elapsed_ms.saturating_sub(self.target_started_ms),
                        hold_mean_absolute_error_centi,
                        self.output_switches,
                        gates.mask(),
                    )?;
                    if self.core.state() == RunState::Running {
                        self.reset_target_window(sample.elapsed_ms);
                        self.begin_candidate_trial(sample.elapsed_ms)?;
                    } else {
                        self.finish_terminal(
                            self.core.terminal().unwrap_or(TerminalDisposition::Failed),
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn ack_trace(
        &mut self,
        through_sequence: u64,
        digest: [u8; 32],
    ) -> Result<(), ThermalTuningRuntimeError> {
        self.core.ack_trace(through_sequence, digest)?;
        Ok(())
    }

    pub fn seal_review(
        &mut self,
        through_sequence: u64,
        digest: [u8; 32],
    ) -> Result<(), ThermalTuningRuntimeError> {
        self.core.seal_review(through_sequence, digest)?;
        Ok(())
    }

    pub fn preview(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<CandidateProfile, ThermalTuningRuntimeError> {
        let profile = self.core.preview(run_id, identity, power_class)?;
        self.preview_profile = Some(profile);
        Ok(profile)
    }

    pub fn discard_preview(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<(), ThermalTuningRuntimeError> {
        self.core.discard_preview(run_id, identity, power_class)?;
        self.preview_profile = None;
        Ok(())
    }

    pub fn save(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<CandidateProfile, ThermalTuningRuntimeError> {
        let profile = self.core.save(run_id, identity, power_class)?;
        self.preview_profile = None;
        Ok(profile)
    }

    pub fn snapshot(
        &self,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> Result<ThermalTuningRunSnapshotWire, ThermalTuningRuntimeError> {
        let summary = self.core.summary();
        if self.core.trace_gap()
            || after_sequence.is_some_and(|after| {
                summary
                    .first_sequence
                    .is_some_and(|first| after.saturating_add(1) < first)
            })
        {
            return Err(ThermalTuningRuntimeError::Trace(TraceError::Gap));
        }
        let mut page_records = [TraceRecord {
            sequence: 0,
            event: flux_purr_thermal_tuning_core::TraceEvent::Decision(DecisionEvent {
                elapsed_ms: 0,
                target_c: 0,
                disposition: TargetDisposition::Pending,
                score_tracking: 0,
                score_energy: 0,
                score_overshoot: 0,
                score_stability: 0,
                score_settle_ms: 0,
                score_hold_mean_absolute_error_centi: 0,
                score_output_switches: 0,
                interval_lower_boundary_c: 0,
                interval_upper_boundary_c: 0,
                interval_pruned: false,
                candidate_frozen: false,
                gates: 0,
                candidate_hash: [0; 32],
            }),
            digest: [0; 32],
        }; crate::control_plane::THERMAL_TUNING_TRACE_PAGE_MAX];
        let count = self
            .core
            .trace_page(after_sequence, limit, &mut page_records);
        let mut events = Vec::new();
        for record in page_records.into_iter().take(count) {
            let _ = events.push(trace_event_wire(record));
        }
        let digest_through_page = (count > 0)
            .then(|| page_records[count - 1])
            .map(|record| string(&hex(&record.digest)));
        let mut accepted = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();
        for (index, target) in EXECUTION_ORDER_C.into_iter().enumerate() {
            if summary.accepted[index] {
                let _ = accepted.push(target);
            }
            if summary.failed[index] {
                let _ = failed.push(target);
            }
            if summary.skipped[index] {
                let _ = skipped.push(target);
            }
        }
        let candidate =
            summary
                .candidate
                .map_or_else(ThermalTuningCandidateWire::default, |identity| {
                    ThermalTuningCandidateWire {
                        candidate_id: Some(string(&hex(&identity.candidate_id))),
                        candidate_hash: Some(string(&hex(&identity.candidate_hash))),
                        canonical_profile_hex: self.core.candidate().map(|profile| {
                            profile_hex::<THERMAL_TUNING_PROFILE_CANONICAL_HEX_LEN>(profile)
                        }),
                        power_class: summary.power_class.map(Into::into),
                        promotion_state: promotion_wire(summary.promotion),
                    }
                });
        let run_id = if summary.run_id == 0 {
            string("idle")
        } else {
            string(&summary.run_id.to_string())
        };
        let trace_digest =
            (summary.last_sequence.is_some()).then(|| string(&hex(&summary.trace_digest)));
        let review_state = if summary.trace_gap {
            ThermalTuningReviewStateWire::Incomplete
        } else if summary.state == RunState::Terminal {
            if summary.review_complete {
                ThermalTuningReviewStateWire::Complete
            } else {
                ThermalTuningReviewStateWire::AwaitingSeal
            }
        } else if summary.state == RunState::Running {
            ThermalTuningReviewStateWire::Recording
        } else {
            ThermalTuningReviewStateWire::NotApplicable
        };
        let mut review = ThermalTuningReviewWire {
            state: review_state,
            reason: summary.trace_gap.then(|| string("trace_gap")),
            acknowledged_through: summary.acknowledged_through,
            terminal_sequence: summary
                .last_sequence
                .filter(|_| summary.state == RunState::Terminal),
            trace_digest,
        };
        if summary.state == RunState::Terminal
            && summary.terminal != Some(TerminalDisposition::Completed)
        {
            review.reason = summary.terminal.map(|value| string(value.as_str()));
        }
        Ok(ThermalTuningRunSnapshotWire {
            schema: string("thermal_tuning_run_v1"),
            run: ThermalTuningRunWire {
                run_id,
                state: run_state_wire(summary.state),
                power_class: summary.power_class.map(Into::into),
                phase: phase_wire(summary.phase),
                current_target_c: summary.current_target_c,
                target_progress: ThermalTuningTargetProgressWire {
                    accepted_c: accepted,
                    failed_c: failed,
                    skipped_c: skipped,
                },
                terminal_disposition: summary.terminal.map(terminal_wire),
                eligibility: self
                    .eligibility
                    .wire(self.arbiter.owner(), summary.power_class),
                review,
                candidate,
                journal: ThermalTuningJournalWire {
                    last_run_id: self
                        .journal
                        .last_run_id
                        .map(|value| string(&value.to_string())),
                    last_disposition: self.journal.last_disposition.map(terminal_wire),
                },
            },
            page: ThermalTuningTracePageWire {
                earliest_sequence: summary.first_sequence.unwrap_or(
                    summary
                        .last_sequence
                        .map_or(0, |last| last.saturating_add(1)),
                ),
                emitted_through: summary.last_sequence,
                next_after_sequence: summary
                    .last_sequence
                    .map_or(0, |last| last.saturating_add(1)),
                acknowledged_through: summary.acknowledged_through,
                digest_through_page,
                events,
            },
        })
    }

    fn record_terminal_decision(
        &mut self,
        elapsed_ms: u32,
        disposition: TargetDisposition,
        gates: u16,
    ) -> Result<(), ThermalTuningRuntimeError> {
        self.core
            .record_current_decision(disposition, elapsed_ms, 0, 0, 0, 0, 0, 0, 0, gates)?;
        Ok(())
    }

    fn finish_terminal(&mut self, disposition: TerminalDisposition) {
        self.journal.last_disposition = Some(disposition);
        self.arbiter.release(MaintenanceRunOwner::ThermalTuning);
        self.preview_profile = None;
        self.candidate_ladder = None;
        self.candidate_evaluations = [None; CANDIDATE_LADDER_WIDTH];
        self.candidate_trial_index = 0;
    }

    fn begin_candidate_trial(&mut self, now_ms: u32) -> Result<(), ThermalTuningRuntimeError> {
        let ladder = self
            .core
            .candidate_ladder_for_current_target()
            .ok_or(ThermalTuningRuntimeError::NotActive)?;
        self.candidate_ladder = Some(ladder);
        self.candidate_evaluations = [None; CANDIDATE_LADDER_WIDTH];
        self.candidate_trial_index = 0;
        self.candidate_trial_started_ms = now_ms;
        self.core
            .set_current_target_candidate(ladder[0])?
            .ok_or(ThermalTuningRuntimeError::NotActive)?;
        Ok(())
    }

    fn reset_candidate_window(&mut self, now_ms: u32) {
        self.candidate_trial_started_ms = now_ms;
        self.hold_started_ms = None;
        self.target_min_temp_centi = i16::MAX;
        self.target_max_temp_centi = i16::MIN;
        self.max_overshoot_centi = 0;
        self.hold_error_sum_centi = 0;
        self.hold_sample_count = 0;
        self.hold_confirm_within_gate = true;
        self.output_switches = 0;
        self.last_output_permille = None;
    }

    fn reset_target_window(&mut self, now_ms: u32) {
        self.target_started_ms = now_ms;
        self.warmup_complete = false;
        self.reset_candidate_window(now_ms);
    }
}

fn trace_event_wire(record: TraceRecord) -> ThermalTuningTraceEventWire {
    match record.event {
        flux_purr_thermal_tuning_core::TraceEvent::Sample(sample) => ThermalTuningTraceEventWire {
            sequence: record.sequence,
            elapsed_ms: sample.elapsed_ms,
            kind: ThermalTuningTraceKindWire::Sample,
            phase: Some(phase_wire(sample.phase)),
            target_c: None,
            temperature_centi_c: Some(sample.temperature_centi_c),
            vin_mv: Some(sample.vin_mv),
            pps_contract_mv: Some(sample.pps_contract_mv),
            pps_contract_ma: Some(sample.pps_contract_ma),
            heater_output_permille: Some(sample.heater_output_permille),
            measurement_valid: Some(sample.measurement_valid),
            disposition: None,
            score_tracking: None,
            score_energy: None,
            score_overshoot: None,
            score_stability: None,
            score_settle_ms: None,
            score_hold_mean_absolute_error_centi: None,
            score_output_switches: None,
            interval_lower_boundary_c: None,
            interval_upper_boundary_c: None,
            interval_pruned: None,
            candidate_frozen: None,
            gates: None,
            candidate_hash: None,
        },
        flux_purr_thermal_tuning_core::TraceEvent::Decision(decision) => {
            ThermalTuningTraceEventWire {
                sequence: record.sequence,
                elapsed_ms: decision.elapsed_ms,
                kind: ThermalTuningTraceKindWire::Decision,
                phase: None,
                target_c: Some(decision.target_c),
                temperature_centi_c: None,
                vin_mv: None,
                pps_contract_mv: None,
                pps_contract_ma: None,
                heater_output_permille: None,
                measurement_valid: None,
                disposition: Some(target_wire(decision.disposition)),
                score_tracking: Some(decision.score_tracking),
                score_energy: Some(decision.score_energy),
                score_overshoot: Some(decision.score_overshoot),
                score_stability: Some(decision.score_stability),
                score_settle_ms: Some(decision.score_settle_ms),
                score_hold_mean_absolute_error_centi: Some(
                    decision.score_hold_mean_absolute_error_centi,
                ),
                score_output_switches: Some(decision.score_output_switches),
                interval_lower_boundary_c: Some(decision.interval_lower_boundary_c),
                interval_upper_boundary_c: Some(decision.interval_upper_boundary_c),
                interval_pruned: Some(decision.interval_pruned),
                candidate_frozen: Some(decision.candidate_frozen),
                gates: Some(decision.gates),
                candidate_hash: Some(string(&hex(&decision.candidate_hash))),
            }
        }
    }
}

fn run_state_wire(value: RunState) -> ThermalTuningRunStateWire {
    match value {
        RunState::Idle => ThermalTuningRunStateWire::Idle,
        RunState::Running => ThermalTuningRunStateWire::Running,
        RunState::Terminal => ThermalTuningRunStateWire::Terminal,
    }
}

fn phase_wire(value: Phase) -> ThermalTuningPhaseWire {
    match value {
        Phase::Idle => ThermalTuningPhaseWire::Idle,
        Phase::CooldownWait => ThermalTuningPhaseWire::CooldownWait,
        Phase::Scout => ThermalTuningPhaseWire::Scout,
        Phase::Retune => ThermalTuningPhaseWire::Retune,
        Phase::HoldConfirm => ThermalTuningPhaseWire::HoldConfirm,
        Phase::Terminal => ThermalTuningPhaseWire::Terminal,
    }
}

fn terminal_wire(value: TerminalDisposition) -> ThermalTuningTerminalDispositionWire {
    match value {
        TerminalDisposition::Completed => ThermalTuningTerminalDispositionWire::Completed,
        TerminalDisposition::Failed => ThermalTuningTerminalDispositionWire::Failed,
        TerminalDisposition::Cancelled => ThermalTuningTerminalDispositionWire::Cancelled,
        TerminalDisposition::BudgetExhausted => {
            ThermalTuningTerminalDispositionWire::BudgetExhausted
        }
        TerminalDisposition::SafetyDisarmed => ThermalTuningTerminalDispositionWire::SafetyDisarmed,
        TerminalDisposition::ReviewIncomplete => {
            ThermalTuningTerminalDispositionWire::ReviewIncomplete
        }
        TerminalDisposition::InterruptedReset => {
            ThermalTuningTerminalDispositionWire::InterruptedReset
        }
    }
}

fn target_wire(value: TargetDisposition) -> ThermalTuningTargetDispositionWire {
    match value {
        TargetDisposition::Pending => ThermalTuningTargetDispositionWire::Pending,
        TargetDisposition::Accepted => ThermalTuningTargetDispositionWire::Accepted,
        TargetDisposition::Failed => ThermalTuningTargetDispositionWire::Failed,
        TargetDisposition::Skipped => ThermalTuningTargetDispositionWire::Skipped,
    }
}

fn promotion_wire(value: PromotionState) -> ThermalTuningPromotionStateWire {
    match value {
        PromotionState::Unavailable => ThermalTuningPromotionStateWire::Unavailable,
        PromotionState::AwaitingReview => ThermalTuningPromotionStateWire::AwaitingReview,
        PromotionState::Ready => ThermalTuningPromotionStateWire::Ready,
        PromotionState::Previewed => ThermalTuningPromotionStateWire::Previewed,
        PromotionState::Saved => ThermalTuningPromotionStateWire::Saved,
        PromotionState::Expired => ThermalTuningPromotionStateWire::Expired,
    }
}

fn push_reason(
    values: &mut Vec<String<{ crate::control_plane::ERROR_CODE_MAX_LEN }>, 8>,
    value: &str,
) {
    let _ = values.push(string(value));
}

fn string<const N: usize>(value: &str) -> String<N> {
    let mut output = String::new();
    let _ = output.push_str(value);
    output
}

fn hex(bytes: &[u8]) -> String<64> {
    hex_with_capacity(bytes)
}

fn profile_hex<const N: usize>(profile: CandidateProfile) -> String<N> {
    let mut bytes = [0u8; flux_purr_thermal_tuning_core::CANDIDATE_PROFILE_CANONICAL_BYTES];
    profile.canonical_bytes(&mut bytes);
    hex_with_capacity(&bytes)
}

fn hex_with_capacity<const N: usize>(bytes: &[u8]) -> String<N> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::new();
    for byte in bytes {
        let _ = output.push(HEX[usize::from(byte >> 4)] as char);
        let _ = output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_purr_thermal_tuning_core::TraceEvent;

    fn ready() -> ThermalTuningEligibility {
        ThermalTuningEligibility {
            thermal_model_ready: true,
            curve_covers_all_targets: true,
            pps3a_available: true,
            pps5a_available: true,
            idle: true,
            measurement_safe: true,
        }
    }

    fn sample(elapsed_ms: u32, temperature_centi_c: i16, current_ma: u16) -> ThermalTuningSample {
        sample_with_output(elapsed_ms, temperature_centi_c, current_ma, 500)
    }

    fn sample_with_output(
        elapsed_ms: u32,
        temperature_centi_c: i16,
        current_ma: u16,
        heater_output_permille: u16,
    ) -> ThermalTuningSample {
        ThermalTuningSample {
            elapsed_ms,
            temperature_centi_c,
            vin_mv: 20_000,
            pps_contract_mv: 20_000,
            pps_contract_ma: current_ma,
            heater_output_permille,
            measurement_valid: true,
        }
    }

    #[test]
    fn arbiter_keeps_maintenance_modes_mutually_exclusive() {
        let mut arbiter = MaintenanceRunArbiter::new();
        assert_eq!(
            arbiter.try_acquire(MaintenanceRunOwner::Calibration),
            Ok(())
        );
        assert_eq!(
            arbiter.try_acquire(MaintenanceRunOwner::ThermalTuning),
            Err(MaintenanceRunOwner::Calibration)
        );
        arbiter.release(MaintenanceRunOwner::Calibration);
        assert_eq!(
            arbiter.try_acquire(MaintenanceRunOwner::ThermalTuning),
            Ok(())
        );
    }

    #[test]
    fn idle_snapshot_is_ready_when_any_explicit_pps_class_is_available() {
        let eligibility = ready();
        let wire = eligibility.wire(MaintenanceRunOwner::None, None);
        assert!(wire.ready);
        assert!(wire.reasons.is_empty());

        let mut only_5a = ready();
        only_5a.pps3a_available = false;
        let wire = only_5a.wire(MaintenanceRunOwner::None, None);
        assert!(wire.ready);

        let mut unavailable = ready();
        unavailable.pps3a_available = false;
        unavailable.pps5a_available = false;
        let wire = unavailable.wire(MaintenanceRunOwner::None, None);
        assert!(!wire.ready);
        assert!(
            wire.reasons
                .iter()
                .any(|reason| reason == "pps_power_class_unavailable")
        );
    }

    #[test]
    fn only_explicit_pps_class_can_start() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        assert_eq!(runtime.start(1, PpsPowerClass::Pps3a, 0), Ok(()));
        assert_eq!(runtime.owner(), MaintenanceRunOwner::ThermalTuning);
    }

    #[test]
    fn repeated_start_keeps_the_active_tuning_owner() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        runtime.start(4, PpsPowerClass::Pps3a, 0).unwrap();

        assert_eq!(
            runtime.start(5, PpsPowerClass::Pps5a, 1),
            Err(ThermalTuningRuntimeError::Busy)
        );
        assert_eq!(runtime.owner(), MaintenanceRunOwner::ThermalTuning);
        assert_eq!(runtime.core().run_id(), 4);
    }

    #[test]
    fn invalid_pps_disarms_without_host_disconnect_dependency() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        runtime.start(2, PpsPowerClass::Pps5a, 0).unwrap();
        runtime.tick(sample(0, 5_000, 3_000)).unwrap();
        assert_eq!(
            runtime.core().terminal(),
            Some(TerminalDisposition::SafetyDisarmed)
        );
        let mut page = [TraceRecord {
            sequence: 0,
            event: TraceEvent::Sample(SampleEvent {
                elapsed_ms: 0,
                temperature_centi_c: 0,
                vin_mv: 0,
                pps_contract_mv: 0,
                pps_contract_ma: 0,
                heater_output_permille: 0,
                measurement_valid: false,
                phase: Phase::Idle,
            }),
            digest: [0; 32],
        }; 8];
        let count = runtime.core().trace_page(None, 8, &mut page);
        assert_eq!(count, 1);
        assert!(matches!(page[0].event, TraceEvent::Decision(_)));
    }

    #[test]
    fn hold_peak_to_peak_only_covers_the_confirm_window() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        runtime.start(6, PpsPowerClass::Pps3a, 0).unwrap();

        runtime
            .tick(sample_with_output(0, 2_500, 3_250, 1_000))
            .unwrap();
        runtime
            .tick(sample_with_output(1_000, 5_000, 3_250, 1_000))
            .unwrap();
        runtime
            .tick(sample_with_output(6_000, 5_000, 3_250, 500))
            .unwrap();
        runtime
            .tick(sample_with_output(7_000, 5_900, 3_250, 100))
            .unwrap();
        runtime
            .tick(sample_with_output(67_000, 6_000, 3_250, 0))
            .unwrap();
        runtime
            .tick(sample_with_output(68_000, 6_000, 3_250, 0))
            .unwrap();
        runtime
            .tick(sample_with_output(128_000, 6_000, 3_250, 0))
            .unwrap();
        runtime
            .tick(sample_with_output(129_000, 6_000, 3_250, 0))
            .unwrap();
        runtime
            .tick(sample_with_output(189_000, 6_000, 3_250, 0))
            .unwrap();

        assert!(runtime.core().summary().accepted[0]);
        assert_eq!(runtime.core().phase(), Phase::CooldownWait);
    }

    #[test]
    fn hold_confirm_rejects_an_out_of_band_sample_even_at_the_final_check() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        runtime.start(7, PpsPowerClass::Pps3a, 0).unwrap();

        runtime
            .tick(sample_with_output(0, 2_500, 3_250, 1_000))
            .unwrap();
        runtime
            .tick(sample_with_output(1_000, 5_000, 3_250, 1_000))
            .unwrap();
        runtime
            .tick(sample_with_output(6_000, 5_000, 3_250, 500))
            .unwrap();
        runtime
            .tick(sample_with_output(7_000, 5_900, 3_250, 100))
            .unwrap();
        runtime
            .tick(sample_with_output(8_000, 5_600, 3_250, 0))
            .unwrap();
        runtime
            .tick(sample_with_output(67_000, 5_900, 3_250, 0))
            .unwrap();

        assert!(!runtime.core().summary().accepted[0]);
        assert_eq!(runtime.core().phase(), Phase::Retune);
    }

    #[test]
    fn reset_journals_interrupted_run_and_releases_owner() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.set_eligibility(ready());
        runtime.start(3, PpsPowerClass::Pps3a, 0).unwrap();
        runtime.reset_interrupted();
        assert_eq!(
            runtime.journal().last_disposition,
            Some(TerminalDisposition::InterruptedReset)
        );
        assert_eq!(runtime.owner(), MaintenanceRunOwner::None);
    }

    #[test]
    fn restored_journal_does_not_resume_a_run() {
        let mut runtime = ThermalTuningRuntime::new();
        runtime.restore_journal(42, Some(TerminalDisposition::InterruptedReset));

        let snapshot = runtime.snapshot(None, 16).unwrap();
        assert_eq!(snapshot.run.run_id.as_str(), "idle");
        assert_eq!(snapshot.run.state, ThermalTuningRunStateWire::Idle);
        assert_eq!(snapshot.run.journal.last_run_id.as_deref(), Some("42"));
        assert_eq!(
            snapshot.run.journal.last_disposition,
            Some(ThermalTuningTerminalDispositionWire::InterruptedReset)
        );
        assert_eq!(runtime.owner(), MaintenanceRunOwner::None);
        assert_eq!(runtime.core().candidate(), None);
    }
}
