//! Feasible temperature references and delayed applied-heat prediction.
//! Electrical input is estimated by the runtime, not measured by this model.

use crate::memory::ThermalPlantProjection;
use crate::thermal_plant::natural_cooling_loss_mw;

const HEAT_BUCKETS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrajectoryPlant {
    pub model: ThermalPlantProjection,
    pub ambient_c: f32,
    pub max_power_mw: f32,
}

impl TrajectoryPlant {
    pub fn loss_mw(self, temperature_c: f32) -> f32 {
        natural_cooling_loss_mw(self.model, temperature_c, self.ambient_c)
    }

    fn rise(self, temperature_c: f32, power_mw: f32) -> f32 {
        (power_mw - self.loss_mw(temperature_c)) / self.model.thermal_capacity_mj_per_c
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReferenceState {
    pub temperature_c: f32,
    pub rise_c_per_s: f32,
}

/// A bounded history covers the entire calibrated delay, even at 60 seconds.
/// Buckets store time-weighted power; repeated callbacks cannot invent energy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedHeatHistory {
    powers_mw: [f32; HEAT_BUCKETS],
    width_ms: u64,
    first_bucket: u64,
    last_at_ms: Option<u64>,
    current_power_mw: f32,
    current_energy_mj: f32,
    current_span_ms: u64,
}

impl Default for AppliedHeatHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl AppliedHeatHistory {
    pub const fn new() -> Self {
        Self {
            powers_mw: [0.0; HEAT_BUCKETS],
            width_ms: 50,
            first_bucket: 0,
            last_at_ms: None,
            current_power_mw: 0.0,
            current_energy_mj: 0.0,
            current_span_ms: 0,
        }
    }

    pub fn configure(&mut self, delay_ms: u32) {
        let width_ms = u64::from(delay_ms)
            .div_ceil(HEAT_BUCKETS as u64 - 2)
            .max(50);
        if self.width_ms != width_ms {
            *self = Self {
                width_ms,
                ..Self::new()
            };
        }
    }

    pub fn observe(&mut self, now_ms: u64, power_mw: f32) {
        if !power_mw.is_finite() || power_mw < 0.0 {
            return;
        }
        let Some(mut at_ms) = self.last_at_ms else {
            self.first_bucket = now_ms / self.width_ms;
            self.last_at_ms = Some(now_ms);
            self.current_power_mw = power_mw;
            return;
        };
        if now_ms < at_ms {
            *self = Self {
                width_ms: self.width_ms,
                ..Self::new()
            };
            self.observe(now_ms, power_mw);
            return;
        }
        // A long pause is a held output, not thousands of callback iterations.
        if now_ms - at_ms > self.width_ms * HEAT_BUCKETS as u64 {
            at_ms = now_ms - self.width_ms * (HEAT_BUCKETS as u64 - 1);
            self.first_bucket = at_ms / self.width_ms;
            self.current_energy_mj = 0.0;
            self.current_span_ms = 0;
        }
        while at_ms < now_ms {
            let bucket = at_ms / self.width_ms;
            let end_ms = ((bucket + 1) * self.width_ms).min(now_ms);
            let span_ms = end_ms - at_ms;
            self.current_energy_mj += self.current_power_mw * span_ms as f32 / 1_000.0;
            self.current_span_ms += span_ms;
            if end_ms.is_multiple_of(self.width_ms) {
                self.powers_mw[bucket as usize % HEAT_BUCKETS] =
                    self.current_energy_mj * 1_000.0 / self.current_span_ms as f32;
                self.current_energy_mj = 0.0;
                self.current_span_ms = 0;
            }
            at_ms = end_ms;
        }
        self.last_at_ms = Some(now_ms);
        self.current_power_mw = power_mw;
    }

    fn power_at(&self, at_ms: u64) -> Option<f32> {
        let last_at_ms = self.last_at_ms?;
        let bucket = at_ms / self.width_ms;
        let last_bucket = last_at_ms / self.width_ms;
        if bucket
            < self
                .first_bucket
                .max(last_bucket.saturating_sub(HEAT_BUCKETS as u64 - 1))
        {
            return None;
        }
        if bucket >= last_bucket {
            return Some(if self.current_span_ms > 0 && at_ms < last_at_ms {
                self.current_energy_mj * 1_000.0 / self.current_span_ms as f32
            } else {
                self.current_power_mw
            });
        }
        Some(self.powers_mw[bucket as usize % HEAT_BUCKETS])
    }

    pub fn forecast(
        &self,
        now_ms: u64,
        measured_c: f32,
        observed_rise_c_per_s: f32,
        plant: TrajectoryPlant,
    ) -> ReferenceState {
        let delay_ms = u64::from(plant.model.transport_delay_ms);
        let inferred_mw = (plant.loss_mw(measured_c)
            + plant.model.thermal_capacity_mj_per_c * observed_rise_c_per_s)
            .clamp(0.0, plant.max_power_mw);
        let mut temperature_c = measured_c;
        let start_ms = now_ms.saturating_sub(delay_ms);
        let mut at_ms = start_ms;
        while at_ms < now_ms {
            let end_ms = ((at_ms / self.width_ms + 1) * self.width_ms).min(now_ms);
            let power_mw = self.power_at(at_ms).unwrap_or(inferred_mw);
            temperature_c +=
                plant.rise(temperature_c, power_mw) * (end_ms - at_ms) as f32 / 1_000.0;
            at_ms = end_ms;
        }
        // Startup has no negative timestamps. Account for the uncovered prefix
        // once using observed velocity rather than counting it as stored power.
        if now_ms < delay_ms {
            temperature_c += observed_rise_c_per_s * (delay_ms - now_ms) as f32 / 1_000.0;
        }
        ReferenceState {
            temperature_c,
            rise_c_per_s: plant.rise(temperature_c, self.power_at(now_ms).unwrap_or(inferred_mw)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemperatureTrajectory {
    pub reference: ReferenceState,
    last_at_ms: u64,
}

impl TemperatureTrajectory {
    pub fn new(now_ms: u64, command_arrival_c: f32) -> Self {
        Self {
            reference: ReferenceState {
                temperature_c: command_arrival_c,
                rise_c_per_s: 0.0,
            },
            last_at_ms: now_ms,
        }
    }

    pub fn advance(
        &mut self,
        now_ms: u64,
        target_c: f32,
        horizon_s: f32,
        damping: f32,
        plant: TrajectoryPlant,
    ) -> ReferenceState {
        let mut remaining_ms = now_ms.saturating_sub(self.last_at_ms);
        // Above this gap the ordinary sensor guard must disarm. Bound model work
        // without silently shifting the reference clock or replaying old samples.
        remaining_ms = remaining_ms.min(5_000);
        self.last_at_ms = now_ms;
        let omega = 2.0 / horizon_s.max(0.4);
        while remaining_ms > 0 {
            let step_ms = remaining_ms.min(50);
            let dt_s = step_ms as f32 / 1_000.0;
            let state = self.reference;
            let acceleration = omega * omega * (target_c - state.temperature_c)
                - 2.0 * damping.clamp(0.1, 4.0) * omega * state.rise_c_per_s;
            let desired_rise = state.rise_c_per_s + acceleration * dt_s;
            let loss_mw = plant.loss_mw(state.temperature_c);
            let power_mw = (loss_mw + plant.model.thermal_capacity_mj_per_c * desired_rise)
                .clamp(0.05 * plant.max_power_mw, plant.max_power_mw);
            let feasible_rise = plant.rise(state.temperature_c, power_mw);
            self.reference = ReferenceState {
                temperature_c: state.temperature_c + feasible_rise * dt_s,
                rise_c_per_s: feasible_rise,
            };
            remaining_ms -= step_ms;
        }
        self.reference
    }

    pub fn request_permille(
        reference: ReferenceState,
        predicted: ReferenceState,
        horizon_s: f32,
        plant: TrajectoryPlant,
    ) -> u16 {
        let rise = reference.rise_c_per_s
            + (reference.temperature_c - predicted.temperature_c) / horizon_s.max(0.4)
            + 0.5 * (reference.rise_c_per_s - predicted.rise_c_per_s);
        let power_mw =
            plant.loss_mw(reference.temperature_c) + plant.model.thermal_capacity_mj_per_c * rise;
        (power_mw * 1_000.0 / plant.max_power_mw).clamp(50.0, 1_000.0) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plant(delay_ms: u32) -> TrajectoryPlant {
        TrajectoryPlant {
            model: ThermalPlantProjection {
                convection_mw_per_c: 200.0,
                radiation_mw_per_k4: 0.0,
                thermal_capacity_mj_per_c: 20_000.0,
                transport_delay_ms: delay_ms,
            },
            ambient_c: 30.0,
            max_power_mw: 60_000.0,
        }
    }

    #[test]
    fn forecast_keeps_old_full_power_then_expires_it_at_the_real_delay() {
        let mut history = AppliedHeatHistory::new();
        let plant = plant(700);
        history.configure(700);
        history.observe(0, 60_000.0);
        history.observe(1_000, 3_000.0);
        let pending = history.forecast(1_000, 60.0, 0.0, plant);
        assert!(pending.temperature_c > 61.8);
        history.observe(1_700, 3_000.0);
        let expired = history.forecast(1_700, 60.0, 3.0, plant);
        assert!(
            expired.temperature_c < 60.0,
            "stored input must supersede stale velocity"
        );
    }

    #[test]
    fn power_buckets_preserve_energy_and_cover_a_sixty_second_delay() {
        let mut history = AppliedHeatHistory::new();
        history.configure(60_000);
        history.observe(1_000, 60_000.0);
        history.observe(81_000, 60_000.0);
        assert_eq!(history.power_at(21_000), Some(60_000.0));
        history.observe(81_025, 0.0);
        history.observe(81_050, 60_000.0);
        assert!(history.power_at(81_025).unwrap() < 60_000.0);
        history.observe(81_050, 60_000.0);
        assert!(history.power_at(81_025).unwrap() < 60_000.0);
    }

    #[test]
    fn reference_converges_and_every_rate_has_feasible_positive_power() {
        for delay_ms in [700, 1_900] {
            let plant = plant(delay_ms);
            let mut trajectory = TemperatureTrajectory::new(0, 55.0);
            for now_ms in (50..=20_000).step_by(50) {
                let previous = trajectory.reference;
                let reference = trajectory.advance(now_ms, 60.0, 1.9, 1.0, plant);
                let implied_mw = plant.loss_mw(previous.temperature_c)
                    + reference.rise_c_per_s * plant.model.thermal_capacity_mj_per_c;
                assert!((2_999.0..=60_001.0).contains(&implied_mw));
                assert!(reference.temperature_c <= 60.01);
            }
            assert!((trajectory.reference.temperature_c - 60.0).abs() < 0.01);
            assert!(trajectory.reference.rise_c_per_s.abs() < 0.01);
        }
    }
}
