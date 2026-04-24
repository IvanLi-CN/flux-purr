#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzerCueId {
    HeaterOn,
    HeaterOff,
    ActiveCoolingOn,
    ActiveCoolingOff,
    HeaterReject,
    ActiveCoolingReject,
    ProtectionAlarm,
    AttentionReminder,
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
pub struct BuzzerController {
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
    pub const fn new() -> Self {
        Self {
            active: None,
            output: BuzzerOutput::silent(),
            generation: 0,
        }
    }

    pub const fn active_cue(self) -> Option<BuzzerCueId> {
        match self.active {
            Some(active) => Some(active.cue),
            None => None,
        }
    }

    pub const fn is_active(self) -> bool {
        self.active.is_some()
    }

    pub const fn output(self) -> BuzzerOutput {
        self.output
    }

    pub fn play(&mut self, cue: BuzzerCueId, now_ms: u64) -> BuzzerOutput {
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

    pub fn stop(&mut self) -> BuzzerOutput {
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

    pub fn tick(&mut self, now_ms: u64) -> BuzzerOutput {
        let Some(mut active) = self.active else {
            self.output = BuzzerOutput::silent_with_generation(self.generation);
            return self.output;
        };

        loop {
            let pattern = pattern_for(active.cue);
            let step = pattern.steps[active.step_index];
            let elapsed = now_ms.saturating_sub(active.step_started_ms);
            if elapsed < u64::from(step.duration_ms) {
                self.output = output_for_step(step, self.generation);
                self.active = Some(active);
                return self.output;
            }

            active.step_started_ms = active
                .step_started_ms
                .saturating_add(u64::from(step.duration_ms));
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
        }
    }
}

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
    BuzzerStep::tone(1_850, 90),
    BuzzerStep::rest(80),
];
const ATTENTION_REMINDER_PATTERN: [BuzzerStep; 3] = [
    BuzzerStep::tone(1_650, 70),
    BuzzerStep::rest(30),
    BuzzerStep::tone(2_200, 110),
];

const fn pattern_for(cue: BuzzerCueId) -> BuzzerPattern {
    match cue {
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
            looping: true,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn looping_alarm_wraps_without_going_idle() {
        let mut controller = BuzzerController::new();
        controller.play(BuzzerCueId::ProtectionAlarm, 0);

        assert_eq!(controller.tick(0).frequency_hz, Some(2_300));
        assert_eq!(controller.tick(90).frequency_hz, None);
        assert_eq!(controller.tick(130).frequency_hz, Some(1_850));
        assert_eq!(controller.tick(220).frequency_hz, None);
        assert_eq!(controller.tick(300).frequency_hz, Some(2_300));
        assert_eq!(controller.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    }

    #[test]
    fn new_cue_preempts_previous_playback() {
        let mut controller = BuzzerController::new();
        controller.play(BuzzerCueId::ProtectionAlarm, 0);
        assert_eq!(controller.active_cue(), Some(BuzzerCueId::ProtectionAlarm));

        let output = controller.play(BuzzerCueId::ActiveCoolingReject, 35);
        assert_eq!(
            controller.active_cue(),
            Some(BuzzerCueId::ActiveCoolingReject)
        );
        assert_eq!(
            output,
            BuzzerOutput {
                frequency_hz: Some(480),
                duty_percent: 50,
                generation: 2,
            }
        );
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
    fn retriggering_same_cue_bumps_generation_for_hardware_restart() {
        let mut controller = BuzzerController::new();

        let first = controller.play(BuzzerCueId::HeaterOn, 0);
        let second = controller.play(BuzzerCueId::HeaterOn, 20);

        assert_eq!(first.frequency_hz, Some(1_240));
        assert_eq!(second.frequency_hz, Some(1_240));
        assert!(second.generation > first.generation);
    }
}
