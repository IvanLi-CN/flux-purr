#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "buzzer-test", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "buzzer-test", serde(rename_all = "snake_case"))]
pub enum BuzzerCueId {
    UiInput,
    HeaterOn,
    HeaterOff,
    ActiveCoolingOn,
    ActiveCoolingOff,
    HeaterReject,
    ActiveCoolingReject,
    ProtectionAlarm,
    AttentionReminder,
}

impl BuzzerCueId {
    pub const ALL: [Self; 9] = [
        Self::UiInput,
        Self::HeaterOn,
        Self::HeaterOff,
        Self::ActiveCoolingOn,
        Self::ActiveCoolingOff,
        Self::HeaterReject,
        Self::ActiveCoolingReject,
        Self::ProtectionAlarm,
        Self::AttentionReminder,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::UiInput => "ui_input",
            Self::HeaterOn => "heater_on",
            Self::HeaterOff => "heater_off",
            Self::ActiveCoolingOn => "active_cooling_on",
            Self::ActiveCoolingOff => "active_cooling_off",
            Self::HeaterReject => "heater_reject",
            Self::ActiveCoolingReject => "active_cooling_reject",
            Self::ProtectionAlarm => "protection_alarm",
            Self::AttentionReminder => "attention_reminder",
        }
    }

    const fn is_feedback(self) -> bool {
        !matches!(self, Self::ProtectionAlarm | Self::AttentionReminder)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "buzzer-test", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "buzzer-test", serde(rename_all = "snake_case"))]
pub enum BuzzerCueSource {
    Startup,
    FrontPanel,
    RuntimeControl,
    ThermalProtection,
    ThermalAttention,
    #[cfg(feature = "buzzer-test")]
    BuzzerTest,
}

impl BuzzerCueSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::FrontPanel => "frontpanel",
            Self::RuntimeControl => "runtime_control",
            Self::ThermalProtection => "thermal_protection",
            Self::ThermalAttention => "thermal_attention",
            #[cfg(feature = "buzzer-test")]
            Self::BuzzerTest => "buzzer_test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "buzzer-test", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "buzzer-test", serde(rename_all = "snake_case"))]
pub enum BuzzerDecisionDisposition {
    Started,
    Preempted,
    Queued,
    Coalesced,
    Replaced,
    Dropped,
    Stopped,
}

impl BuzzerDecisionDisposition {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Preempted => "preempted",
            Self::Queued => "queued",
            Self::Coalesced => "coalesced",
            Self::Replaced => "replaced",
            Self::Dropped => "dropped",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "buzzer-test", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "buzzer-test", serde(rename_all = "camelCase"))]
pub struct BuzzerDecision {
    pub source: BuzzerCueSource,
    pub cue: BuzzerCueId,
    pub disposition: BuzzerDecisionDisposition,
}

impl BuzzerDecision {
    const fn new(
        source: BuzzerCueSource,
        cue: BuzzerCueId,
        disposition: BuzzerDecisionDisposition,
    ) -> Self {
        Self {
            source,
            cue,
            disposition,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuzzerTick {
    pub output: BuzzerOutput,
    pub deferred_start: Option<BuzzerDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuzzerStep {
    pub frequency_hz: Option<u32>,
    pub duty_percent: u8,
    pub duration_ms: u32,
}

impl BuzzerStep {
    pub const fn tone(frequency_hz: u32, duration_ms: u32) -> Self {
        Self {
            frequency_hz: Some(frequency_hz),
            duty_percent: 50,
            duration_ms,
        }
    }

    pub const fn rest(duration_ms: u32) -> Self {
        Self {
            frequency_hz: None,
            duty_percent: 0,
            duration_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuzzerPattern {
    pub steps: &'static [BuzzerStep],
    pub looping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuzzerOutput {
    pub frequency_hz: Option<u32>,
    pub duty_percent: u8,
    pub generation: u32,
}

impl BuzzerOutput {
    pub const fn silent() -> Self {
        Self::silent_with_generation(0)
    }

    pub const fn silent_with_generation(generation: u32) -> Self {
        Self {
            frequency_hz: None,
            duty_percent: 0,
            generation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveCue {
    cue: BuzzerCueId,
    step_index: usize,
    step_started_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuzzerController {
    active: Option<ActiveCue>,
    output: BuzzerOutput,
    generation: u32,
}

impl Default for BuzzerController {
    fn default() -> Self {
        Self::new()
    }
}

impl BuzzerController {
    const fn new() -> Self {
        Self {
            active: None,
            output: BuzzerOutput::silent(),
            generation: 0,
        }
    }

    const fn active_cue(self) -> Option<BuzzerCueId> {
        match self.active {
            Some(active) => Some(active.cue),
            None => None,
        }
    }

    const fn is_active(self) -> bool {
        self.active.is_some()
    }

    const fn output(self) -> BuzzerOutput {
        self.output
    }

    fn play(&mut self, cue: BuzzerCueId, now_ms: u64) -> BuzzerOutput {
        let pattern = pattern_for(cue);
        let first_step = pattern.steps[0];
        self.generation = self.generation.wrapping_add(1);
        self.active = Some(ActiveCue {
            cue,
            step_index: 0,
            step_started_ms: now_ms,
        });
        self.output = output_for_step(first_step, self.generation);
        self.output
    }

    fn stop(&mut self) -> BuzzerOutput {
        if self.active.is_some()
            || self.output.frequency_hz.is_some()
            || self.output.duty_percent != 0
        {
            self.generation = self.generation.wrapping_add(1);
        }
        self.active = None;
        self.output = BuzzerOutput::silent_with_generation(self.generation);
        self.output
    }

    fn tick(&mut self, now_ms: u64) -> BuzzerOutput {
        let Some(mut active) = self.active else {
            self.output = BuzzerOutput::silent_with_generation(self.generation);
            return self.output;
        };

        let pattern = pattern_for(active.cue);
        let step = pattern.steps[active.step_index];
        let scheduled_end_ms = active
            .step_started_ms
            .saturating_add(u64::from(step.duration_ms));
        if now_ms < scheduled_end_ms {
            self.output = output_for_step(step, self.generation);
            self.active = Some(active);
            return self.output;
        }

        // A late tick must still expose the next step to GPIO48. Advancing
        // through multiple expired steps would erase a short rest entirely.
        active.step_started_ms = scheduled_end_ms.max(now_ms);
        active.step_index += 1;
        if active.step_index >= pattern.steps.len() {
            if pattern.looping {
                active.step_index = 0;
            } else {
                self.active = None;
                self.generation = self.generation.wrapping_add(1);
                self.output = BuzzerOutput::silent_with_generation(self.generation);
                return self.output;
            }
        }

        let next_step = pattern.steps[active.step_index];
        self.output = output_for_step(next_step, self.generation);
        self.active = Some(active);
        self.output
    }

    fn next_transition_ms(&self) -> Option<u64> {
        self.active.map(|active| {
            active.step_started_ms.saturating_add(u64::from(
                pattern_for(active.cue).steps[active.step_index].duration_ms,
            ))
        })
    }
}

const UI_INPUT_PATTERN: [BuzzerStep; 1] = [BuzzerStep::tone(1_080, 45)];
const HEATER_ON_PATTERN: [BuzzerStep; 3] = [
    BuzzerStep::tone(1_240, 60),
    BuzzerStep::rest(30),
    BuzzerStep::tone(1_680, 80),
];
const HEATER_OFF_PATTERN: [BuzzerStep; 3] = [
    BuzzerStep::tone(1_680, 60),
    BuzzerStep::rest(30),
    BuzzerStep::tone(1_240, 80),
];
const ACTIVE_COOLING_ON_PATTERN: [BuzzerStep; 5] = [
    BuzzerStep::tone(900, 45),
    BuzzerStep::rest(25),
    BuzzerStep::tone(1_200, 45),
    BuzzerStep::rest(25),
    BuzzerStep::tone(1_550, 70),
];
const ACTIVE_COOLING_OFF_PATTERN: [BuzzerStep; 5] = [
    BuzzerStep::tone(1_550, 45),
    BuzzerStep::rest(25),
    BuzzerStep::tone(1_200, 45),
    BuzzerStep::rest(25),
    BuzzerStep::tone(900, 70),
];
const HEATER_REJECT_PATTERN: [BuzzerStep; 3] = [
    BuzzerStep::tone(420, 120),
    BuzzerStep::rest(35),
    BuzzerStep::tone(360, 150),
];
const ACTIVE_COOLING_REJECT_PATTERN: [BuzzerStep; 5] = [
    BuzzerStep::tone(480, 75),
    BuzzerStep::rest(20),
    BuzzerStep::tone(480, 75),
    BuzzerStep::rest(20),
    BuzzerStep::tone(320, 120),
];
const PROTECTION_ALARM_PATTERN: [BuzzerStep; 4] = [
    BuzzerStep::tone(2_300, 90),
    BuzzerStep::rest(40),
    BuzzerStep::tone(2_300, 90),
    BuzzerStep::rest(80),
];
const ATTENTION_REMINDER_PATTERN: [BuzzerStep; 3] = [
    BuzzerStep::tone(1_650, 70),
    BuzzerStep::rest(30),
    BuzzerStep::tone(2_200, 110),
];

/// The fault condition remains active between one-shot protection patterns.
/// Both normal runtime and feature-gated diagnostics use this cadence.
pub const PROTECTION_ALARM_INTERVAL_MS: u64 = 1_000;
pub const ATTENTION_REMINDER_INTERVAL_MS: u64 = 10_000;

const fn pattern_for(cue: BuzzerCueId) -> BuzzerPattern {
    match cue {
        BuzzerCueId::UiInput => BuzzerPattern {
            steps: &UI_INPUT_PATTERN,
            looping: false,
        },
        BuzzerCueId::HeaterOn => BuzzerPattern {
            steps: &HEATER_ON_PATTERN,
            looping: false,
        },
        BuzzerCueId::HeaterOff => BuzzerPattern {
            steps: &HEATER_OFF_PATTERN,
            looping: false,
        },
        BuzzerCueId::ActiveCoolingOn => BuzzerPattern {
            steps: &ACTIVE_COOLING_ON_PATTERN,
            looping: false,
        },
        BuzzerCueId::ActiveCoolingOff => BuzzerPattern {
            steps: &ACTIVE_COOLING_OFF_PATTERN,
            looping: false,
        },
        BuzzerCueId::HeaterReject => BuzzerPattern {
            steps: &HEATER_REJECT_PATTERN,
            looping: false,
        },
        BuzzerCueId::ActiveCoolingReject => BuzzerPattern {
            steps: &ACTIVE_COOLING_REJECT_PATTERN,
            looping: false,
        },
        BuzzerCueId::ProtectionAlarm => BuzzerPattern {
            steps: &PROTECTION_ALARM_PATTERN,
            looping: false,
        },
        BuzzerCueId::AttentionReminder => BuzzerPattern {
            steps: &ATTENTION_REMINDER_PATTERN,
            looping: false,
        },
    }
}

const fn output_for_step(step: BuzzerStep, generation: u32) -> BuzzerOutput {
    BuzzerOutput {
        frequency_hz: step.frequency_hz,
        duty_percent: step.duty_percent,
        generation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudibleSafetyState {
    Normal,
    ProtectionActive,
    AttentionPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingCue {
    source: BuzzerCueSource,
    cue: BuzzerCueId,
}

impl PendingCue {
    const fn decision(self, disposition: BuzzerDecisionDisposition) -> BuzzerDecision {
        BuzzerDecision::new(self.source, self.cue, disposition)
    }
}

pub struct BuzzerArbiter {
    controller: BuzzerController,
    safety_state: AudibleSafetyState,
    pending_feedback: Option<PendingCue>,
    pending_attention: Option<BuzzerCueSource>,
}

impl Default for BuzzerArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl BuzzerArbiter {
    pub const fn new() -> Self {
        Self {
            controller: BuzzerController::new(),
            safety_state: AudibleSafetyState::Normal,
            pending_feedback: None,
            pending_attention: None,
        }
    }

    pub const fn active_cue(&self) -> Option<BuzzerCueId> {
        self.controller.active_cue()
    }

    pub const fn is_active(&self) -> bool {
        self.controller.is_active()
    }

    pub const fn output(&self) -> BuzzerOutput {
        self.controller.output()
    }

    /// Returns the next PWM output deadline for the active cue.
    pub fn next_transition_ms(&self) -> Option<u64> {
        self.controller.next_transition_ms()
    }

    pub fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64) -> BuzzerDecision {
        let disposition = if self.controller.is_active() {
            BuzzerDecisionDisposition::Preempted
        } else {
            BuzzerDecisionDisposition::Started
        };
        self.safety_state = AudibleSafetyState::ProtectionActive;
        self.pending_feedback = None;
        self.pending_attention = None;
        self.controller.play(BuzzerCueId::ProtectionAlarm, now_ms);
        BuzzerDecision::new(source, BuzzerCueId::ProtectionAlarm, disposition)
    }

    pub fn request_protection_replay(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) -> BuzzerDecision {
        if self.safety_state != AudibleSafetyState::ProtectionActive {
            return BuzzerDecision::new(
                source,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Dropped,
            );
        }
        if self.controller.active_cue() == Some(BuzzerCueId::ProtectionAlarm) {
            return BuzzerDecision::new(
                source,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Coalesced,
            );
        }

        let disposition = if self.controller.is_active() {
            BuzzerDecisionDisposition::Preempted
        } else {
            BuzzerDecisionDisposition::Started
        };
        self.controller.play(BuzzerCueId::ProtectionAlarm, now_ms);
        BuzzerDecision::new(source, BuzzerCueId::ProtectionAlarm, disposition)
    }

    pub fn enter_attention_pending(&mut self) -> Option<BuzzerDecision> {
        self.safety_state = AudibleSafetyState::AttentionPending;
        self.pending_feedback = None;
        self.pending_attention = None;
        if self.controller.active_cue() == Some(BuzzerCueId::ProtectionAlarm) {
            self.controller.stop();
            return Some(BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Stopped,
            ));
        }
        None
    }

    pub fn clear_attention(&mut self) -> Option<BuzzerDecision> {
        self.safety_state = AudibleSafetyState::Normal;
        self.pending_feedback = None;
        self.pending_attention = None;
        let cue = self.controller.active_cue()?;
        let source = match cue {
            BuzzerCueId::ProtectionAlarm => BuzzerCueSource::ThermalProtection,
            BuzzerCueId::AttentionReminder => BuzzerCueSource::ThermalAttention,
            _ => return None,
        };
        self.controller.stop();
        Some(BuzzerDecision::new(
            source,
            cue,
            BuzzerDecisionDisposition::Stopped,
        ))
    }

    pub fn request_attention_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) -> BuzzerDecision {
        if self.safety_state != AudibleSafetyState::AttentionPending {
            return BuzzerDecision::new(
                source,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Dropped,
            );
        }

        match self.controller.active_cue() {
            None => {
                self.controller.play(BuzzerCueId::AttentionReminder, now_ms);
                BuzzerDecision::new(
                    source,
                    BuzzerCueId::AttentionReminder,
                    BuzzerDecisionDisposition::Started,
                )
            }
            Some(BuzzerCueId::AttentionReminder) | Some(BuzzerCueId::ProtectionAlarm) => {
                BuzzerDecision::new(
                    source,
                    BuzzerCueId::AttentionReminder,
                    BuzzerDecisionDisposition::Coalesced,
                )
            }
            Some(_) => {
                let disposition = if self.pending_attention.is_some() {
                    BuzzerDecisionDisposition::Coalesced
                } else {
                    self.pending_attention = Some(source);
                    BuzzerDecisionDisposition::Queued
                };
                BuzzerDecision::new(source, BuzzerCueId::AttentionReminder, disposition)
            }
        }
    }

    pub fn request_feedback(
        &mut self,
        source: BuzzerCueSource,
        cue: BuzzerCueId,
        now_ms: u64,
    ) -> BuzzerDecision {
        if !cue.is_feedback() {
            return BuzzerDecision::new(source, cue, BuzzerDecisionDisposition::Dropped);
        }
        if self.safety_state != AudibleSafetyState::Normal {
            return BuzzerDecision::new(source, cue, BuzzerDecisionDisposition::Dropped);
        }
        if !self.controller.is_active() {
            self.controller.play(cue, now_ms);
            return BuzzerDecision::new(source, cue, BuzzerDecisionDisposition::Started);
        }

        let pending = PendingCue { source, cue };
        let disposition = match self.pending_feedback {
            None => {
                self.pending_feedback = Some(pending);
                BuzzerDecisionDisposition::Queued
            }
            Some(_) if cue == BuzzerCueId::UiInput => BuzzerDecisionDisposition::Coalesced,
            Some(_) => {
                self.pending_feedback = Some(pending);
                BuzzerDecisionDisposition::Replaced
            }
        };
        pending.decision(disposition)
    }

    #[cfg(feature = "buzzer-test")]
    pub fn stop_test_playback(&mut self) -> Option<BuzzerDecision> {
        self.pending_feedback = None;
        self.pending_attention = None;
        self.safety_state = AudibleSafetyState::Normal;
        let cue = self.controller.active_cue()?;
        self.controller.stop();
        Some(BuzzerDecision::new(
            BuzzerCueSource::BuzzerTest,
            cue,
            BuzzerDecisionDisposition::Stopped,
        ))
    }

    pub fn tick(&mut self, now_ms: u64) -> BuzzerTick {
        let mut output = self.controller.tick(now_ms);
        let deferred_start = if self.controller.is_active() {
            None
        } else if self.safety_state == AudibleSafetyState::AttentionPending {
            self.pending_attention.take().map(|source| {
                self.controller.play(BuzzerCueId::AttentionReminder, now_ms);
                output = self.controller.output();
                BuzzerDecision::new(
                    source,
                    BuzzerCueId::AttentionReminder,
                    BuzzerDecisionDisposition::Started,
                )
            })
        } else if self.safety_state == AudibleSafetyState::Normal {
            self.pending_feedback.take().map(|pending| {
                self.controller.play(pending.cue, now_ms);
                output = self.controller.output();
                pending.decision(BuzzerDecisionDisposition::Started)
            })
        } else {
            None
        };
        BuzzerTick {
            output,
            deferred_start,
        }
    }
}

/// Drives the production protection alarm through the arbiter's public safety
/// API. The runtime and the buzzer test feature share this exact scheduling object.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProtectionAlarmCadence {
    next_replay_ms: Option<u64>,
}

impl ProtectionAlarmCadence {
    pub const fn new() -> Self {
        Self {
            next_replay_ms: None,
        }
    }

    pub fn enter(&mut self, buzzer: &mut BuzzerArbiter, now_ms: u64) -> BuzzerDecision {
        self.enter_with_source(buzzer, BuzzerCueSource::ThermalProtection, now_ms)
    }

    pub fn enter_with_source(
        &mut self,
        buzzer: &mut BuzzerArbiter,
        source: BuzzerCueSource,
        now_ms: u64,
    ) -> BuzzerDecision {
        self.arm(now_ms);
        buzzer.activate_protection(source, now_ms)
    }

    pub fn arm(&mut self, now_ms: u64) {
        self.next_replay_ms = Some(now_ms.saturating_add(PROTECTION_ALARM_INTERVAL_MS));
    }

    pub fn clear(&mut self) {
        self.next_replay_ms = None;
    }

    pub const fn next_replay_ms(&self) -> Option<u64> {
        self.next_replay_ms
    }

    pub fn replay_due(&mut self, fault_present: bool, now_ms: u64) -> bool {
        if !fault_present {
            self.clear();
            return false;
        }
        if self.next_replay_ms.is_none_or(|next| now_ms < next) {
            return false;
        }

        self.arm(now_ms);
        true
    }

    pub fn tick(
        &mut self,
        fault_present: bool,
        buzzer: &mut BuzzerArbiter,
        now_ms: u64,
    ) -> Option<BuzzerDecision> {
        if !self.replay_due(fault_present, now_ms) {
            return None;
        }
        Some(buzzer.request_protection_replay(BuzzerCueSource::ThermalProtection, now_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "buzzer-test")]
    use crate::buzzer_test::{BuzzerTestScenario, BuzzerTestSession, BuzzerTestSessionState};

    #[test]
    fn one_shot_pattern_returns_to_silence() {
        let mut controller = BuzzerController::new();
        assert_eq!(
            controller.play(BuzzerCueId::HeaterOn, 0),
            BuzzerOutput {
                frequency_hz: Some(1_240),
                duty_percent: 50,
                generation: 1,
            }
        );
        assert_eq!(controller.active_cue(), Some(BuzzerCueId::HeaterOn));

        assert_eq!(controller.tick(60).frequency_hz, None);
        assert_eq!(controller.tick(90).frequency_hz, Some(1_680));
        assert_eq!(controller.tick(169).frequency_hz, Some(1_680));
        let finished = controller.tick(170);
        assert_eq!(finished.frequency_hz, None);
        assert_eq!(finished.duty_percent, 0);
        assert!(finished.generation > 1);
        assert_eq!(controller.active_cue(), None);
    }

    #[test]
    fn late_tick_never_skips_a_protection_silence_step() {
        let mut controller = BuzzerController::new();
        controller.play(BuzzerCueId::ProtectionAlarm, 0);

        // The task arrived after the nominal rest boundary. It still applies
        // the rest before it emits the second audible step.
        assert_eq!(controller.tick(145).duty_percent, 0);
        assert_eq!(controller.tick(184).duty_percent, 0);
        assert_eq!(controller.tick(185).frequency_hz, Some(2_300));
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_coalesce_scenario_records_the_single_pending_feedback_path() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        let started = session
            .start_scenario(&mut arbiter, BuzzerTestScenario::FeedbackCoalesce, 0)
            .expect("the idle buzzer test session starts");
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].disposition, BuzzerDecisionDisposition::Started);
        assert_eq!(
            session.status(arbiter.active_cue()).trace[0]
                .decision
                .disposition,
            BuzzerDecisionDisposition::Started
        );

        let queued = session.advance(&mut arbiter, 15);
        assert_eq!(queued[0].disposition, BuzzerDecisionDisposition::Queued);
        let coalesced = session.advance(&mut arbiter, 30);
        assert_eq!(
            coalesced[0].disposition,
            BuzzerDecisionDisposition::Coalesced
        );

        let deferred = arbiter
            .tick(45)
            .deferred_start
            .expect("pending feedback starts");
        session.record_deferred_start(45, deferred);
        let status = session.status(arbiter.active_cue());
        assert_eq!(status.trace.len(), 4);
        assert_eq!(
            status.trace[3].decision.disposition,
            BuzzerDecisionDisposition::Started
        );
        assert_eq!(status.trace[3].decision.cue, BuzzerCueId::UiInput);
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_replace_scenario_records_the_latest_specialized_feedback() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        session
            .start_scenario(&mut arbiter, BuzzerTestScenario::FeedbackReplace, 0)
            .expect("the idle buzzer test session starts");
        assert_eq!(
            session.advance(&mut arbiter, 15)[0].disposition,
            BuzzerDecisionDisposition::Queued
        );
        assert_eq!(
            session.advance(&mut arbiter, 30)[0].disposition,
            BuzzerDecisionDisposition::Replaced
        );

        let deferred = arbiter.tick(45).deferred_start.expect("replacement starts");
        session.record_deferred_start(45, deferred);
        let status = session.status(arbiter.active_cue());
        assert_eq!(status.trace[2].decision.cue, BuzzerCueId::HeaterOn);
        assert_eq!(
            status.trace[2].decision.disposition,
            BuzzerDecisionDisposition::Replaced
        );
        assert_eq!(status.trace[3].decision.cue, BuzzerCueId::HeaterOn);
        assert_eq!(
            status.trace[3].decision.disposition,
            BuzzerDecisionDisposition::Started
        );
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_active_cooling_retrigger_preserves_the_three_tone_production_pattern() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        session
            .start_scenario(&mut arbiter, BuzzerTestScenario::ActiveCoolingRetrigger, 0)
            .expect("the idle buzzer test session starts");
        assert_eq!(arbiter.output().frequency_hz, Some(900));
        assert_eq!(
            session.advance(&mut arbiter, 15)[0].disposition,
            BuzzerDecisionDisposition::Queued
        );
        assert_eq!(
            session.advance(&mut arbiter, 30)[0].disposition,
            BuzzerDecisionDisposition::Replaced
        );

        assert_eq!(arbiter.tick(45).output.duty_percent, 0);
        assert_eq!(arbiter.tick(70).output.frequency_hz, Some(1_200));
        assert_eq!(arbiter.tick(115).output.duty_percent, 0);
        assert_eq!(arbiter.tick(140).output.frequency_hz, Some(1_550));
        assert_eq!(arbiter.tick(210).output.frequency_hz, Some(900));
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_feedback_is_dropped_while_real_protection_is_active() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();
        let _ = arbiter.activate_protection(BuzzerCueSource::ThermalProtection, 0);

        let decision = session.trigger_feedback(&mut arbiter, BuzzerCueId::UiInput, 10);
        assert_eq!(decision.disposition, BuzzerDecisionDisposition::Dropped);
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_repeat_protection_uses_the_production_safety_cadence() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        let started = session
            .start_playback(&mut arbiter, BuzzerCueId::ProtectionAlarm, true, 0)
            .expect("the idle buzzer test session starts a module playback");
        assert_eq!(started.len(), 1);
        assert_eq!(
            started[0],
            BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Started,
            )
        );
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::ProtectionAlarm));

        let _ = arbiter.tick(90);
        let _ = arbiter.tick(130);
        let _ = arbiter.tick(220);
        assert_eq!(
            arbiter.tick(300).output,
            BuzzerOutput::silent_with_generation(2)
        );
        assert!(session.advance(&mut arbiter, 999).is_empty());

        let replay = session.advance(&mut arbiter, PROTECTION_ALARM_INTERVAL_MS);
        assert_eq!(replay.len(), 1);
        assert_eq!(
            replay[0],
            BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Started,
            )
        );
        let status = session.status(arbiter.active_cue());
        assert_eq!(status.state, BuzzerTestSessionState::Running);
        assert_eq!(status.cue, Some(BuzzerCueId::ProtectionAlarm));
        assert!(status.repeat);
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_repeat_feedback_restarts_after_each_production_pattern() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        session
            .start_playback(&mut arbiter, BuzzerCueId::UiInput, true, 0)
            .expect("the idle buzzer test session starts a repeating feedback cue");
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::UiInput));

        assert_eq!(arbiter.tick(45).output.duty_percent, 0);
        assert!(!arbiter.is_active());
        let replay = session.settle_after_tick(&mut arbiter, 45);

        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::UiInput));
        assert_eq!(replay.len(), 1);
        let status = session.status(arbiter.active_cue());
        assert_eq!(status.trace.len(), 2);
        assert_eq!(status.trace[1].elapsed_ms, 45);
        assert_eq!(
            status.trace[1].decision,
            BuzzerDecision::new(
                BuzzerCueSource::BuzzerTest,
                BuzzerCueId::UiInput,
                BuzzerDecisionDisposition::Started,
            )
        );
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_attention_reminder_starts_immediately_then_rearms_at_ten_seconds() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        let started = session
            .start_playback(&mut arbiter, BuzzerCueId::AttentionReminder, true, 0)
            .expect("the idle buzzer test session starts an immediate attention reminder");
        assert_eq!(
            started.as_slice(),
            &[BuzzerDecision::new(
                BuzzerCueSource::ThermalAttention,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Started,
            )]
        );
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::AttentionReminder));
        assert_eq!(arbiter.output().frequency_hz, Some(1_650));

        let _ = arbiter.tick(70);
        let _ = arbiter.tick(100);
        let _ = arbiter.tick(211);
        assert!(!arbiter.is_active());
        assert!(session.advance(&mut arbiter, 9_999).is_empty());
        let replay = session.advance(&mut arbiter, 10_000);
        assert_eq!(replay.len(), 1);
        assert_eq!(
            replay[0],
            BuzzerDecision::new(
                BuzzerCueSource::ThermalAttention,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Started,
            )
        );
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::AttentionReminder));
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_one_shot_feedback_starts_a_fresh_decision_trace() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        let decision = session.trigger_feedback(&mut arbiter, BuzzerCueId::UiInput, 29_138);
        let status = session.status(arbiter.active_cue());

        assert_eq!(status.state, BuzzerTestSessionState::Idle);
        assert_eq!(status.trace.len(), 1);
        assert_eq!(status.trace[0].elapsed_ms, 0);
        assert_eq!(status.trace[0].decision, decision);
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_one_shot_protection_completes_after_its_last_gpio_step() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        session
            .start_playback(&mut arbiter, BuzzerCueId::ProtectionAlarm, false, 0)
            .expect("the idle buzzer test session starts a one-shot protection cue");

        let _ = arbiter.tick(90);
        let _ = arbiter.tick(130);
        let _ = arbiter.tick(220);
        assert_eq!(arbiter.tick(300).output.duty_percent, 0);
        assert!(!arbiter.is_active());

        let _ = session.settle_after_tick(&mut arbiter, 300);
        assert_eq!(
            session.status(arbiter.active_cue()).state,
            BuzzerTestSessionState::Complete
        );
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_stop_returns_the_arbiter_to_normal_without_changing_the_cue_pattern() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        session
            .start_playback(&mut arbiter, BuzzerCueId::ProtectionAlarm, true, 0)
            .expect("the idle buzzer test session starts a module playback");
        assert_eq!(
            session.stop_playback(&mut arbiter, 50),
            Some(BuzzerDecision::new(
                BuzzerCueSource::BuzzerTest,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Stopped,
            ))
        );
        assert_eq!(arbiter.active_cue(), None);
        assert_eq!(
            session.status(arbiter.active_cue()).state,
            BuzzerTestSessionState::Idle
        );
    }

    #[cfg(feature = "buzzer-test")]
    #[test]
    fn buzzer_test_stop_does_not_stop_unowned_production_feedback() {
        let mut arbiter = BuzzerArbiter::new();
        let mut session = BuzzerTestSession::new();

        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterOn, 0),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::HeaterOn,
                BuzzerDecisionDisposition::Started,
            )
        );
        assert_eq!(session.stop_playback(&mut arbiter, 1), None);
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::HeaterOn));
    }

    #[test]
    fn protection_alarm_is_a_one_shot() {
        let mut arbiter = BuzzerArbiter::new();
        assert_eq!(
            arbiter.activate_protection(BuzzerCueSource::ThermalProtection, 0),
            BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Started,
            )
        );

        assert_eq!(arbiter.output().frequency_hz, Some(2_300));
        assert_eq!(arbiter.tick(90).output.frequency_hz, None);
        assert_eq!(arbiter.tick(130).output.frequency_hz, Some(2_300));
        assert_eq!(arbiter.tick(220).output.frequency_hz, None);
        assert_eq!(arbiter.tick(300).output.frequency_hz, None);
        assert_eq!(arbiter.active_cue(), None);
    }

    #[test]
    fn protection_alarm_keeps_a_constant_carrier_across_its_audible_steps() {
        let pattern = pattern_for(BuzzerCueId::ProtectionAlarm);
        let mut audible_frequencies = pattern.steps.iter().filter_map(|step| step.frequency_hz);
        assert_eq!(audible_frequencies.next(), Some(2_300));
        assert_eq!(audible_frequencies.next(), Some(2_300));
        assert_eq!(audible_frequencies.next(), None);
    }

    #[test]
    fn every_production_cue_has_a_pattern() {
        for cue in BuzzerCueId::ALL {
            let pattern = pattern_for(cue);
            assert!(!pattern.steps.is_empty(), "{} has no steps", cue.label());
        }
    }

    #[test]
    fn protection_preempts_and_clears_feedback() {
        let mut arbiter = BuzzerArbiter::new();
        arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterOn, 0);
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 10),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::UiInput,
                BuzzerDecisionDisposition::Queued,
            )
        );
        assert_eq!(
            arbiter.activate_protection(BuzzerCueSource::ThermalProtection, 20),
            BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Preempted,
            )
        );
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterOff, 30),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::HeaterOff,
                BuzzerDecisionDisposition::Dropped,
            )
        );
        assert_eq!(arbiter.tick(320).output.frequency_hz, None);
        assert_eq!(arbiter.tick(500).deferred_start, None);
    }

    #[test]
    fn stop_clears_the_active_cue_immediately() {
        let mut controller = BuzzerController::new();
        controller.play(BuzzerCueId::AttentionReminder, 0);
        assert!(controller.is_active());

        let stopped = controller.stop();
        assert_eq!(stopped.frequency_hz, None);
        assert_eq!(stopped.duty_percent, 0);
        assert!(stopped.generation > 0);
        assert!(!controller.is_active());
        let idle = controller.tick(500);
        assert_eq!(idle.frequency_hz, None);
        assert_eq!(idle.duty_percent, 0);
    }

    #[test]
    fn feedback_coalesces_and_keeps_only_the_latest_specialized_cue() {
        let mut arbiter = BuzzerArbiter::new();
        arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterOn, 0);
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 10),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::UiInput,
                BuzzerDecisionDisposition::Queued,
            )
        );
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 20),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::UiInput,
                BuzzerDecisionDisposition::Coalesced,
            )
        );
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterReject, 30,),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::HeaterReject,
                BuzzerDecisionDisposition::Replaced,
            )
        );
        assert_eq!(
            arbiter.request_feedback(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::ActiveCoolingReject,
                40,
            ),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::ActiveCoolingReject,
                BuzzerDecisionDisposition::Replaced,
            )
        );

        let _ = arbiter.tick(60);
        let _ = arbiter.tick(90);
        let tick = arbiter.tick(170);
        assert_eq!(
            tick.deferred_start,
            Some(BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::ActiveCoolingReject,
                BuzzerDecisionDisposition::Started,
            ))
        );
        assert_eq!(tick.output.frequency_hz, Some(480));
    }

    #[test]
    fn feedback_requests_cannot_start_a_safety_cue() {
        let mut arbiter = BuzzerArbiter::new();

        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::ProtectionAlarm, 0),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Dropped,
            )
        );
        assert_eq!(arbiter.active_cue(), None);
    }

    #[test]
    fn attention_waits_for_active_feedback_and_drops_new_feedback() {
        let mut arbiter = BuzzerArbiter::new();
        arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::HeaterOn, 0);
        assert_eq!(arbiter.enter_attention_pending(), None);
        assert_eq!(
            arbiter.request_attention_reminder(BuzzerCueSource::ThermalAttention, 10),
            BuzzerDecision::new(
                BuzzerCueSource::ThermalAttention,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Queued,
            )
        );
        assert_eq!(
            arbiter.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 20),
            BuzzerDecision::new(
                BuzzerCueSource::FrontPanel,
                BuzzerCueId::UiInput,
                BuzzerDecisionDisposition::Dropped,
            )
        );

        let _ = arbiter.tick(60);
        let _ = arbiter.tick(90);
        let tick = arbiter.tick(170);
        assert_eq!(
            tick.deferred_start,
            Some(BuzzerDecision::new(
                BuzzerCueSource::ThermalAttention,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Started,
            ))
        );
        assert_eq!(tick.output.frequency_hz, Some(1_650));
    }

    #[test]
    fn protection_preempts_an_active_attention_reminder() {
        let mut arbiter = BuzzerArbiter::new();
        assert_eq!(arbiter.enter_attention_pending(), None);
        arbiter.request_attention_reminder(BuzzerCueSource::ThermalAttention, 0);

        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::AttentionReminder));
        assert_eq!(
            arbiter.activate_protection(BuzzerCueSource::ThermalProtection, 10),
            BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Preempted,
            )
        );
        assert_eq!(arbiter.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    }

    #[test]
    fn clearing_attention_stops_safety_cue_without_replaying_feedback() {
        let mut arbiter = BuzzerArbiter::new();
        arbiter.activate_protection(BuzzerCueSource::ThermalProtection, 0);
        assert_eq!(
            arbiter.enter_attention_pending(),
            Some(BuzzerDecision::new(
                BuzzerCueSource::ThermalProtection,
                BuzzerCueId::ProtectionAlarm,
                BuzzerDecisionDisposition::Stopped,
            ))
        );
        arbiter.request_attention_reminder(BuzzerCueSource::ThermalAttention, 10);
        assert_eq!(
            arbiter.clear_attention(),
            Some(BuzzerDecision::new(
                BuzzerCueSource::ThermalAttention,
                BuzzerCueId::AttentionReminder,
                BuzzerDecisionDisposition::Stopped,
            ))
        );
        assert_eq!(arbiter.tick(500).deferred_start, None);
    }
}
