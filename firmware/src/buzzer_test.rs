use crate::buzzer::{
    ATTENTION_REMINDER_INTERVAL_MS, BuzzerArbiter, BuzzerCueId, BuzzerCueSource, BuzzerDecision,
    ProtectionAlarmCadence,
};

pub const BUZZER_TEST_TRACE_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerTestScenario {
    FeedbackCoalesce,
    FeedbackReplace,
    ActiveCoolingRetrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuzzerTestSessionState {
    Idle,
    Running,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzerTestSessionError {
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestTraceEvent {
    pub elapsed_ms: u32,
    pub decision: BuzzerDecision,
}

#[cfg(feature = "buzzer-observe")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestOutputTraceEvent {
    pub elapsed_ms: u32,
    pub requested_frequency_hz: Option<u32>,
    pub applied_frequency_hz: u32,
    pub observed_frequency_hz: Option<u32>,
    pub observed_rising_edges: u16,
    pub observed_window_ms: u32,
    pub duty_percent: u8,
    pub generation: u32,
    pub timer_prescaler: u8,
    pub timer_period_ticks: u16,
}

#[cfg(feature = "buzzer-observe")]
pub const BUZZER_TEST_OUTPUT_TRACE_CAPACITY: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuzzerTestStatus {
    pub state: BuzzerTestSessionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<BuzzerTestScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue: Option<BuzzerCueId>,
    pub repeat: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_cue: Option<BuzzerCueId>,
    pub trace: heapless::Vec<BuzzerTestTraceEvent, BUZZER_TEST_TRACE_CAPACITY>,
    #[cfg(feature = "buzzer-observe")]
    pub output_trace: heapless::Vec<BuzzerTestOutputTraceEvent, BUZZER_TEST_OUTPUT_TRACE_CAPACITY>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuzzerTestPlayback {
    cue: BuzzerCueId,
    repeat: bool,
    attention_due_ms: Option<u64>,
    attention_started: bool,
}

pub struct BuzzerTestSession {
    state: BuzzerTestSessionState,
    scenario: Option<BuzzerTestScenario>,
    playback: Option<BuzzerTestPlayback>,
    started_at_ms: u64,
    next_action: u8,
    protection_cadence: ProtectionAlarmCadence,
    trace: heapless::Vec<BuzzerTestTraceEvent, BUZZER_TEST_TRACE_CAPACITY>,
}

impl Default for BuzzerTestSession {
    fn default() -> Self {
        Self::new()
    }
}

impl BuzzerTestSession {
    pub const fn new() -> Self {
        Self {
            state: BuzzerTestSessionState::Idle,
            scenario: None,
            playback: None,
            started_at_ms: 0,
            next_action: 0,
            protection_cadence: ProtectionAlarmCadence::new(),
            trace: heapless::Vec::new(),
        }
    }

    pub fn status(&self, active_cue: Option<BuzzerCueId>) -> BuzzerTestStatus {
        BuzzerTestStatus {
            state: self.state,
            scenario: self.scenario,
            cue: self.playback.map(|playback| playback.cue),
            repeat: self.playback.is_some_and(|playback| playback.repeat),
            active_cue,
            trace: self.trace.clone(),
            #[cfg(feature = "buzzer-observe")]
            output_trace: heapless::Vec::new(),
        }
    }

    pub fn next_deadline_ms(&self) -> Option<u64> {
        if self.state != BuzzerTestSessionState::Running {
            return None;
        }
        if let Some(scenario) = self.scenario {
            return scenario_action(scenario, self.next_action)
                .map(|(due_ms, _)| self.started_at_ms.saturating_add(due_ms))
                .or(Some(
                    self.started_at_ms
                        .saturating_add(scenario_duration_ms(scenario)),
                ));
        }

        let playback = self.playback?;
        match playback.cue {
            BuzzerCueId::ProtectionAlarm => self.protection_cadence.next_replay_ms(),
            BuzzerCueId::AttentionReminder => playback.attention_due_ms,
            _ => None,
        }
    }

    pub fn trigger_feedback(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        cue: BuzzerCueId,
        now_ms: u64,
    ) -> BuzzerDecision {
        if self.state != BuzzerTestSessionState::Running {
            self.state = BuzzerTestSessionState::Idle;
            self.scenario = None;
            self.playback = None;
            self.started_at_ms = now_ms;
            self.next_action = 0;
            self.protection_cadence.clear();
            self.trace.clear();
        }
        let decision = arbiter.request_feedback(BuzzerCueSource::BuzzerTest, cue, now_ms);
        self.record(now_ms, decision);
        decision
    }

    pub fn start_scenario(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        scenario: BuzzerTestScenario,
        now_ms: u64,
    ) -> Result<heapless::Vec<BuzzerDecision, 3>, BuzzerTestSessionError> {
        if self.state == BuzzerTestSessionState::Running {
            return Err(BuzzerTestSessionError::Busy);
        }
        self.state = BuzzerTestSessionState::Running;
        self.scenario = Some(scenario);
        self.playback = None;
        self.started_at_ms = now_ms;
        self.next_action = 0;
        self.trace.clear();
        Ok(self.advance(arbiter, now_ms))
    }

    pub fn start_playback(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        cue: BuzzerCueId,
        repeat: bool,
        now_ms: u64,
    ) -> Result<heapless::Vec<BuzzerDecision, 1>, BuzzerTestSessionError> {
        if self.state == BuzzerTestSessionState::Running {
            return Err(BuzzerTestSessionError::Busy);
        }

        self.state = BuzzerTestSessionState::Running;
        self.scenario = None;
        self.started_at_ms = now_ms;
        self.next_action = 0;
        self.protection_cadence.clear();
        self.trace.clear();
        self.playback = Some(BuzzerTestPlayback {
            cue,
            repeat,
            attention_due_ms: None,
            attention_started: false,
        });

        let mut decisions = heapless::Vec::new();
        let decision = match cue {
            BuzzerCueId::ProtectionAlarm => Some(self.protection_cadence.enter(arbiter, now_ms)),
            BuzzerCueId::AttentionReminder => {
                let _ = arbiter.enter_attention_pending();
                if let Some(playback) = self.playback.as_mut() {
                    playback.attention_due_ms =
                        Some(now_ms.saturating_add(ATTENTION_REMINDER_INTERVAL_MS));
                }
                None
            }
            _ => Some(arbiter.request_feedback(BuzzerCueSource::BuzzerTest, cue, now_ms)),
        };
        if let Some(decision) = decision {
            self.record(now_ms, decision);
            let _ = decisions.push(decision);
        }
        Ok(decisions)
    }

    pub fn stop_playback(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        now_ms: u64,
    ) -> Option<BuzzerDecision> {
        if self.playback.is_none() && self.scenario.is_none() {
            self.state = BuzzerTestSessionState::Idle;
            return None;
        }
        self.scenario = None;
        self.playback = None;
        self.protection_cadence.clear();
        self.state = BuzzerTestSessionState::Idle;
        let decision = arbiter.stop_test_playback()?;
        self.record(now_ms, decision);
        Some(decision)
    }

    pub fn cancel_for_safety(&mut self, arbiter: &mut BuzzerArbiter, now_ms: u64) {
        if self.state != BuzzerTestSessionState::Running {
            return;
        }
        self.scenario = None;
        self.playback = None;
        self.protection_cadence.clear();
        self.state = BuzzerTestSessionState::Idle;
        let _ = arbiter
            .stop_test_playback()
            .map(|decision| self.record(now_ms, decision));
    }

    pub fn advance(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        now_ms: u64,
    ) -> heapless::Vec<BuzzerDecision, 3> {
        let mut decisions = heapless::Vec::new();
        let Some(scenario) = self.scenario else {
            return self.advance_playback(arbiter, now_ms);
        };
        let elapsed_ms = now_ms.saturating_sub(self.started_at_ms);

        while let Some((due_ms, cue)) = scenario_action(scenario, self.next_action) {
            if elapsed_ms < due_ms {
                break;
            }
            self.next_action = self.next_action.saturating_add(1);
            let decision = self.trigger_feedback(arbiter, cue, now_ms);
            let _ = decisions.push(decision);
        }

        if elapsed_ms >= scenario_duration_ms(scenario) {
            self.state = BuzzerTestSessionState::Complete;
        }
        decisions
    }

    fn advance_playback(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        now_ms: u64,
    ) -> heapless::Vec<BuzzerDecision, 3> {
        let mut decisions = heapless::Vec::new();
        let Some(mut playback) = self.playback else {
            return decisions;
        };

        match playback.cue {
            BuzzerCueId::ProtectionAlarm => {
                if let Some(decision) = self.protection_cadence.tick(true, arbiter, now_ms) {
                    self.record(now_ms, decision);
                    let _ = decisions.push(decision);
                }
            }
            BuzzerCueId::AttentionReminder => {
                if playback
                    .attention_due_ms
                    .is_some_and(|due_ms| now_ms >= due_ms)
                {
                    let decision = arbiter
                        .request_attention_reminder(BuzzerCueSource::ThermalAttention, now_ms);
                    self.record(now_ms, decision);
                    let _ = decisions.push(decision);
                    playback.attention_started = true;
                    playback.attention_due_ms =
                        Some(now_ms.saturating_add(ATTENTION_REMINDER_INTERVAL_MS));
                }
            }
            _ if !arbiter.is_active() && playback.repeat => {
                let decision =
                    arbiter.request_feedback(BuzzerCueSource::BuzzerTest, playback.cue, now_ms);
                self.record(now_ms, decision);
                let _ = decisions.push(decision);
            }
            _ => {}
        }

        self.playback = Some(playback);
        self.complete_playback_if_quiet(arbiter);
        decisions
    }

    pub fn settle_after_tick(
        &mut self,
        arbiter: &mut BuzzerArbiter,
        now_ms: u64,
    ) -> heapless::Vec<BuzzerDecision, 3> {
        self.advance_playback(arbiter, now_ms)
    }

    fn complete_playback_if_quiet(&mut self, arbiter: &mut BuzzerArbiter) {
        let Some(playback) = self.playback else {
            return;
        };
        let finished = match playback.cue {
            BuzzerCueId::ProtectionAlarm => !playback.repeat && !arbiter.is_active(),
            BuzzerCueId::AttentionReminder => {
                !playback.repeat && playback.attention_started && !arbiter.is_active()
            }
            _ => !playback.repeat && !arbiter.is_active(),
        };
        if finished {
            self.protection_cadence.clear();
            let _ = arbiter.stop_test_playback();
            self.state = BuzzerTestSessionState::Complete;
        }
    }

    pub fn record_deferred_start(&mut self, now_ms: u64, decision: BuzzerDecision) {
        if decision.source == BuzzerCueSource::BuzzerTest {
            self.record(now_ms, decision);
        }
    }

    fn record(&mut self, now_ms: u64, decision: BuzzerDecision) {
        let elapsed_ms = now_ms
            .saturating_sub(self.started_at_ms)
            .min(u64::from(u32::MAX)) as u32;
        if self.trace.len() == BUZZER_TEST_TRACE_CAPACITY {
            let _ = self.trace.remove(0);
        }
        let _ = self.trace.push(BuzzerTestTraceEvent {
            elapsed_ms,
            decision,
        });
    }
}

const fn scenario_action(scenario: BuzzerTestScenario, action: u8) -> Option<(u64, BuzzerCueId)> {
    match (scenario, action) {
        (BuzzerTestScenario::FeedbackCoalesce, 0) => Some((0, BuzzerCueId::UiInput)),
        (BuzzerTestScenario::FeedbackCoalesce, 1) => Some((15, BuzzerCueId::UiInput)),
        (BuzzerTestScenario::FeedbackCoalesce, 2) => Some((30, BuzzerCueId::UiInput)),
        (BuzzerTestScenario::FeedbackReplace, 0) => Some((0, BuzzerCueId::UiInput)),
        (BuzzerTestScenario::FeedbackReplace, 1) => Some((15, BuzzerCueId::UiInput)),
        (BuzzerTestScenario::FeedbackReplace, 2) => Some((30, BuzzerCueId::HeaterOn)),
        (BuzzerTestScenario::ActiveCoolingRetrigger, 0) => Some((0, BuzzerCueId::ActiveCoolingOn)),
        (BuzzerTestScenario::ActiveCoolingRetrigger, 1) => Some((15, BuzzerCueId::ActiveCoolingOn)),
        (BuzzerTestScenario::ActiveCoolingRetrigger, 2) => Some((30, BuzzerCueId::ActiveCoolingOn)),
        _ => None,
    }
}

const fn scenario_duration_ms(scenario: BuzzerTestScenario) -> u64 {
    match scenario {
        BuzzerTestScenario::FeedbackCoalesce => 250,
        BuzzerTestScenario::FeedbackReplace => 350,
        BuzzerTestScenario::ActiveCoolingRetrigger => 500,
    }
}
