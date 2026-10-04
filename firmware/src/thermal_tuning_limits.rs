//! Trial-local, bounded timing exceptions. This module never drives an actuator.
use serde::{Deserialize, Serialize};

use crate::memory::ThermalControlProfilePointConfig;

#[cfg(test)]
#[path = "thermal_tuning_limits_tests.rs"]
mod tests;

pub const TRIAL_TIMEOUT_MS: u64 = 180_000;
pub const TIMING_POLICY_VERSION: u8 = 1;

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
    pub floor_observed_at_ms: Option<u64>,
    pub extended_at_ms: Option<u64>,
    pub warmup_exited_at_ms: Option<u64>,
    pub stable_window_started_at_ms: Option<u64>,
    pub stable_window_verified_at_ms: Option<u64>,
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
            allowed_approach_floor_permille: point.hold_power_permille.max(100),
            effective_approach_floor_permille: point
                .approach_floor_power_permille
                .max(point.hold_power_permille),
            floor_observed_at_ms: None,
            extended_at_ms: None,
            warmup_exited_at_ms: None,
            stable_window_started_at_ms: None,
            stable_window_verified_at_ms: None,
        }
    }

    pub fn observe_approach_request(&mut self, elapsed_ms: u64, requested_permille: u16) {
        if self.extended_at_ms.is_some() {
            return;
        }
        let rounded_floor = ((self.effective_approach_floor_permille + 5) / 10) * 10;
        if self.effective_approach_floor_permille == self.allowed_approach_floor_permille
            && requested_permille == rounded_floor
        {
            self.floor_observed_at_ms.get_or_insert(elapsed_ms);
        } else if requested_permille != 0 {
            // A protective coast cannot create floor evidence, but neither
            // does it raise this unchanged candidate above its allowed floor.
            // Preserve a real witness; nonzero requests away from the floor
            // still invalidate it before the original deadline.
            self.floor_observed_at_ms = None;
        }
    }

    pub fn try_extend(&mut self, elapsed_ms: u64, original_timed_out: bool) -> bool {
        if self.extended_at_ms.is_some() {
            return true;
        }
        if !original_timed_out || self.floor_observed_at_ms.is_none() {
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
        self.policy_version == TIMING_POLICY_VERSION
            && matches!(self.original_limit_ms, 5_000 | 10_000)
            && Some(self.extension_limit_ms) == self.original_limit_ms.checked_mul(3)
            && self.effective_approach_floor_permille == self.allowed_approach_floor_permille
            && self.allowed_approach_floor_permille >= 100
            && matches!(
                (self.floor_observed_at_ms, self.extended_at_ms, self.warmup_exited_at_ms),
                (Some(floor), Some(extended), Some(exit))
                    if floor >= exit && floor <= extended
                        && extended.checked_sub(exit).is_some_and(|age| age >= u64::from(self.original_limit_ms))
            )
    }
}
