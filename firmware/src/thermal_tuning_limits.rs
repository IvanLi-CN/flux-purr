//! Trial-local, bounded timing exceptions. This module never drives an actuator.
use serde::{Deserialize, Serialize};

use crate::memory::ThermalControlProfilePointConfig;
use crate::thermal_tuning::ThermalTuningPhase;

#[cfg(test)]
#[path = "thermal_tuning_limits_tests.rs"]
mod tests;

pub const TRIAL_TIMEOUT_MS: u64 = 180_000;
pub const TIMING_POLICY_VERSION: u8 = 2;
pub const MODEL_APPROACH_TIMING_POLICY_VERSION: u8 = 3;
pub const BRAKING_APPROACH_TIMING_POLICY_VERSION: u8 = 4;
pub const EXTENSION_FLOOR_PERMILLE: u16 = 100;

pub const fn original_settle_limit_ms(target_c: i16) -> u32 {
    if target_c > 150 { 5_000 } else { 10_000 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalTuningTimingEvidence {
    pub policy_version: u8,
    pub original_limit_ms: u32,
    pub extension_limit_ms: u32,
    pub allowed_approach_floor_permille: u16,
    pub effective_approach_floor_permille: u16,
    #[serde(default, skip_serializing_if = "floor_not_recorded")]
    pub configured_approach_floor_permille: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_approach_floor_permille: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_deadline_observed_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_deadline_phase: Option<ThermalTuningPhase>,
    pub floor_observed_at_ms: Option<u64>,
    pub extended_at_ms: Option<u64>,
    pub warmup_exited_at_ms: Option<u64>,
    pub stable_window_started_at_ms: Option<u64>,
    pub stable_window_verified_at_ms: Option<u64>,
}

fn floor_not_recorded(value: &u16) -> bool {
    *value == 0
}

impl ThermalTuningTimingEvidence {
    pub fn for_point(point: ThermalControlProfilePointConfig) -> Self {
        let original_limit_ms = original_settle_limit_ms(point.target_temp_c);
        // Captured 60C floor trials broke their first stable window 13.9–15.8s
        // after Warmup. Allow one bounded recovery interval rather than reset
        // the clock or erase that failure: 30s at <=150C, 15s above it.
        Self {
            policy_version: TIMING_POLICY_VERSION,
            original_limit_ms,
            extension_limit_ms: original_limit_ms * 3,
            allowed_approach_floor_permille: EXTENSION_FLOOR_PERMILLE,
            effective_approach_floor_permille: point
                .approach_floor_power_permille
                .max(point.hold_power_permille),
            configured_approach_floor_permille: point.approach_floor_power_permille,
            model_approach_floor_permille: None,
            original_deadline_observed_at_ms: None,
            original_deadline_phase: None,
            floor_observed_at_ms: None,
            extended_at_ms: None,
            warmup_exited_at_ms: None,
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
        }
    }

    pub fn for_model_point(point: ThermalControlProfilePointConfig) -> Self {
        Self {
            policy_version: BRAKING_APPROACH_TIMING_POLICY_VERSION,
            allowed_approach_floor_permille: crate::thermal_plant::FIVE_AMP_APPROACH_MIN_PERMILLE,
            effective_approach_floor_permille: crate::thermal_plant::FIVE_AMP_APPROACH_MIN_PERMILLE,
            model_approach_floor_permille: Some(
                crate::thermal_plant::FIVE_AMP_APPROACH_MIN_PERMILLE,
            ),
            ..Self::for_point(point)
        }
    }

    pub fn required_floor_permille(&self) -> Option<u16> {
        match self.policy_version {
            TIMING_POLICY_VERSION | MODEL_APPROACH_TIMING_POLICY_VERSION => {
                Some(EXTENSION_FLOOR_PERMILLE)
            }
            BRAKING_APPROACH_TIMING_POLICY_VERSION => {
                Some(crate::thermal_plant::FIVE_AMP_APPROACH_MIN_PERMILLE)
            }
            _ => None,
        }
    }

    pub fn supported_policy(&self) -> bool {
        if self.required_floor_permille() != Some(self.allowed_approach_floor_permille) {
            return false;
        }
        match self.policy_version {
            TIMING_POLICY_VERSION => self.model_approach_floor_permille.is_none(),
            MODEL_APPROACH_TIMING_POLICY_VERSION | BRAKING_APPROACH_TIMING_POLICY_VERSION => {
                self.model_approach_floor_permille == Some(self.allowed_approach_floor_permille)
                    && self.effective_approach_floor_permille
                        == self.allowed_approach_floor_permille
            }
            _ => false,
        }
    }

    fn floor_eligible(&self) -> bool {
        self.supported_policy()
            && self.effective_approach_floor_permille == self.allowed_approach_floor_permille
            && (self.model_approach_floor_permille.is_some()
                || self.configured_approach_floor_permille == self.allowed_approach_floor_permille)
    }

    pub fn observe_approach_request(&mut self, elapsed_ms: u64, requested_permille: u16) {
        if self.extended_at_ms.is_some() {
            return;
        }
        if self.floor_eligible() && requested_permille == self.allowed_approach_floor_permille {
            self.floor_observed_at_ms.get_or_insert(elapsed_ms);
        } else if requested_permille != 0 {
            // A protective coast cannot create floor evidence, but neither
            // does it raise this unchanged candidate above its allowed floor.
            // Preserve a real witness; nonzero requests away from the floor
            // still invalidate it before the original deadline.
            self.floor_observed_at_ms = None;
        }
    }

    pub fn try_extend(
        &mut self,
        elapsed_ms: u64,
        original_timed_out: bool,
        phase: ThermalTuningPhase,
    ) -> bool {
        if self.extended_at_ms.is_some() {
            return true;
        }
        let Some(deadline_ms) = self
            .warmup_exited_at_ms
            .and_then(|exit| exit.checked_add(u64::from(self.original_limit_ms)))
        else {
            return false;
        };
        if elapsed_ms < deadline_ms || self.original_deadline_observed_at_ms.is_some() {
            return false;
        }
        // Bind even a rejected decision or an in-flight ordinary window. A
        // later Hold break or floor touch cannot reopen this original check.
        self.original_deadline_observed_at_ms = Some(elapsed_ms);
        self.original_deadline_phase = Some(phase);
        if !original_timed_out
            || phase != ThermalTuningPhase::Approach
            || !self.floor_eligible()
            || self
                .floor_observed_at_ms
                .is_none_or(|floor| floor >= deadline_ms)
        {
            return false;
        }
        self.extended_at_ms = Some(elapsed_ms);
        true
    }

    pub fn active_limit_ms(&self) -> u32 {
        if self.extended_at_ms.is_some() {
            self.extension_limit_ms
        } else {
            self.original_limit_ms
        }
    }

    pub fn valid_extension(&self) -> bool {
        self.floor_eligible()
            && matches!(self.original_limit_ms, 5_000 | 10_000)
            && Some(self.extension_limit_ms) == self.original_limit_ms.checked_mul(3)
            && self.original_deadline_phase == Some(ThermalTuningPhase::Approach)
            && self.original_deadline_observed_at_ms == self.extended_at_ms
            && matches!(
                (self.floor_observed_at_ms, self.extended_at_ms, self.warmup_exited_at_ms),
                (Some(floor), Some(extended), Some(exit))
                    if floor >= exit
                        && exit.checked_add(u64::from(self.original_limit_ms)).is_some_and(|deadline| floor < deadline)
                        && extended.checked_sub(exit).is_some_and(|age| age >= u64::from(self.original_limit_ms))
            )
    }
}
