//! Feasible temperature references and delayed applied-heat prediction.
//! Electrical input is estimated by the runtime, not measured by this model.

use crate::memory::ThermalPlantProjection;
use crate::thermal_plant::natural_cooling_loss_mw;

const HEAT_BUCKETS: usize = 64;
const MEASUREMENT_GAP_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HeatTransferObservation {
    origin: Option<(u64, f32)>,
    last_measurement_ms: Option<u64>,
    last_interval_ms: Option<u64>,
    residual: Option<(u64, f32)>,
    window_start_ms: Option<u64>,
    valid_ms: u64,
    temperature_delta_c: f32,
    loss_energy_mj: f32,
    delayed_energy_mj: f32,
}

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
    transfer: HeatTransferObservation,
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
            transfer: HeatTransferObservation {
                origin: None,
                last_measurement_ms: None,
                last_interval_ms: None,
                residual: None,
                window_start_ms: None,
                valid_ms: 0,
                temperature_delta_c: 0.0,
                loss_energy_mj: 0.0,
                delayed_energy_mj: 0.0,
            },
        }
    }

    pub fn configure(&mut self, delay_ms: u32) {
        // The additional delay covers the accepted measurement interval used
        // to compare received energy with the same delayed input interval.
        let width_ms = (u64::from(delay_ms) * 2 + MEASUREMENT_GAP_MS)
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

    /// Clear sensor evidence on recovery; electrical heat in flight survives.
    pub fn reset_temperature_observation(&mut self) {
        self.transfer = HeatTransferObservation::default();
    }

    /// PPS changes the ADC origin, not previously observed physical transfer.
    /// Retained residuals still age and expire; the first new reading anchors.
    pub fn reanchor_temperature(&mut self) {
        self.transfer.origin = None;
        self.transfer.last_measurement_ms = None;
    }

    /// Call only for accepted measurements within one voltage epoch. Held
    /// readings must not be supplied as new observations.
    pub fn observe_temperature(&mut self, now_ms: u64, temperature_c: f32, plant: TrajectoryPlant) {
        if !temperature_c.is_finite() {
            self.reset_temperature_observation();
            return;
        }
        if self.transfer.last_interval_ms.is_some_and(|last_ms| {
            now_ms < last_ms
                || now_ms - last_ms > u64::from(plant.model.transport_delay_ms).max(100) * 2
        }) {
            self.reset_temperature_observation();
        }
        if let Some(previous_ms) = self.transfer.last_measurement_ms {
            if now_ms == previous_ms {
                return;
            }
            if now_ms < previous_ms || now_ms - previous_ms > MEASUREMENT_GAP_MS {
                self.reset_temperature_observation();
            }
        }
        self.transfer.last_measurement_ms = Some(now_ms);
        let Some((origin_ms, origin_c)) = self.transfer.origin else {
            self.transfer.origin = Some((now_ms, temperature_c));
            return;
        };
        let delay_ms = u64::from(plant.model.transport_delay_ms);
        let elapsed_ms = now_ms - origin_ms;
        if elapsed_ms < 100 {
            return;
        }
        self.transfer.origin = Some((now_ms, temperature_c));
        let Some(mut at_ms) = origin_ms.checked_sub(delay_ms) else {
            return;
        };
        let end_ms = now_ms - delay_ms;
        let mut delayed_energy_mj = 0.0;
        while at_ms < end_ms {
            let next_ms = ((at_ms / self.width_ms + 1) * self.width_ms).min(end_ms);
            let Some(power_mw) = self.power_at(at_ms) else {
                // Unknown history already has a velocity fallback. Do not
                // invent an independent correction for the same energy.
                return;
            };
            delayed_energy_mj += power_mw * (next_ms - at_ms) as f32 / 1_000.0;
            at_ms = next_ms;
        }
        let elapsed_s = elapsed_ms as f32 / 1_000.0;
        if self
            .transfer
            .window_start_ms
            .is_some_and(|start_ms| now_ms.saturating_sub(start_ms) > delay_ms.max(100) * 2)
        {
            self.clear_transfer_energy_window();
        }
        self.transfer.window_start_ms.get_or_insert(origin_ms);
        self.transfer.valid_ms += elapsed_ms;
        self.transfer.last_interval_ms = Some(now_ms);
        self.transfer.temperature_delta_c += temperature_c - origin_c;
        self.transfer.loss_energy_mj += plant.loss_mw((origin_c + temperature_c) * 0.5) * elapsed_s;
        self.transfer.delayed_energy_mj += delayed_energy_mj;
        // Accumulate only intervals with their own accepted temperature
        // origin. PPS gaps and ADC shifts contribute no time or temperature
        // difference; valid evidence on either side may share this average.
        if self.transfer.valid_ms < delay_ms.max(100) {
            return;
        }
        let valid_s = self.transfer.valid_ms as f32 / 1_000.0;
        let residual_mw = ((self.transfer.loss_energy_mj
            + plant.model.thermal_capacity_mj_per_c * self.transfer.temperature_delta_c
            - self.transfer.delayed_energy_mj)
            / valid_s)
            .clamp(-plant.max_power_mw, plant.max_power_mw);
        self.transfer.residual = Some((now_ms, residual_mw));
        self.clear_transfer_energy_window();
    }

    fn clear_transfer_energy_window(&mut self) {
        self.transfer.window_start_ms = None;
        self.transfer.valid_ms = 0;
        self.transfer.temperature_delta_c = 0.0;
        self.transfer.loss_energy_mj = 0.0;
        self.transfer.delayed_energy_mj = 0.0;
    }

    fn residual_at(&self, now_ms: u64, future_ms: u64, plant: TrajectoryPlant) -> f32 {
        let Some((observed_ms, residual_mw)) = self.transfer.residual else {
            return 0.0;
        };
        let horizon_ms = u64::from(plant.model.transport_delay_ms).max(100);
        let age_ms = now_ms.saturating_sub(observed_ms);
        if now_ms < observed_ms || age_ms > horizon_ms * 2 {
            return 0.0;
        }
        // A bounded two-delay decay expresses model uncertainty, not a newly
        // fitted thermal constant. No persistent or cross-run learned state.
        let decay = 1.0 + (age_ms + future_ms) as f32 / (horizon_ms * 2) as f32;
        residual_mw / (decay * decay)
    }

    pub fn forecast(
        &self,
        now_ms: u64,
        measured_c: f32,
        observed_rise_c_per_s: f32,
        plant: TrajectoryPlant,
    ) -> ReferenceState {
        self.forecast_from_measurement(now_ms, now_ms, measured_c, observed_rise_c_per_s, plant)
    }

    /// A held reading retains its accepted timestamp. Integrate heat arriving
    /// since that observation as well as heat in flight to command arrival.
    /// This prediction is not another accepted sensor measurement.
    pub fn forecast_from_measurement(
        &self,
        now_ms: u64,
        measured_at_ms: u64,
        measured_c: f32,
        observed_rise_c_per_s: f32,
        plant: TrajectoryPlant,
    ) -> ReferenceState {
        let delay_ms = u64::from(plant.model.transport_delay_ms);
        let measured_at_ms = measured_at_ms.min(now_ms);
        let inferred_mw = (plant.loss_mw(measured_c)
            + plant.model.thermal_capacity_mj_per_c * observed_rise_c_per_s)
            .clamp(0.0, plant.max_power_mw);
        let mut temperature_c = measured_c;
        let start_ms = measured_at_ms.saturating_sub(delay_ms);
        let mut at_ms = start_ms;
        while at_ms < now_ms {
            let end_ms = ((at_ms / self.width_ms + 1) * self.width_ms).min(now_ms);
            let arrival_ms = at_ms.saturating_add(delay_ms);
            let power_mw = self.power_at(at_ms).map_or(inferred_mw, |power_mw| {
                (power_mw
                    + self.residual_at(
                        arrival_ms.min(now_ms),
                        arrival_ms.saturating_sub(now_ms),
                        plant,
                    ))
                .clamp(0.0, plant.max_power_mw)
            });
            temperature_c +=
                plant.rise(temperature_c, power_mw) * (end_ms - at_ms) as f32 / 1_000.0;
            at_ms = end_ms;
        }
        // Startup has no negative timestamps. Account for the uncovered prefix
        // once using observed velocity rather than counting it as stored power.
        if measured_at_ms < delay_ms {
            temperature_c += observed_rise_c_per_s * (delay_ms - measured_at_ms) as f32 / 1_000.0;
        }
        ReferenceState {
            temperature_c,
            rise_c_per_s: plant.rise(
                temperature_c,
                self.power_at(now_ms).map_or(inferred_mw, |power_mw| {
                    (power_mw + self.residual_at(now_ms, delay_ms, plant))
                        .clamp(0.0, plant.max_power_mw)
                }),
            ),
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

/// Limit only upward Approach requests using the calibrated transport delay.
///
/// Downward requests remain immediate so braking can still remove heat as soon
/// as the model calls for it. An upward request gains at most one delay-scaled
/// fraction of full scale per elapsed control interval. This bounds a model
/// correction without introducing a second profile floor or ceiling.
pub fn slew_up_permille(
    previous_permille: u16,
    requested_permille: u16,
    elapsed_ms: u64,
    transport_delay_ms: u32,
) -> u16 {
    if requested_permille <= previous_permille {
        return requested_permille;
    }
    let delay_ms = u64::from(transport_delay_ms.max(1));
    let elapsed_ms = elapsed_ms.max(1);
    let step = elapsed_ms
        .saturating_mul(1_000)
        .saturating_add(delay_ms.saturating_sub(1))
        .saturating_div(delay_ms)
        .clamp(50, 1_000) as u16;
    previous_permille
        .saturating_add(step)
        .min(requested_permille)
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
    fn held_measurement_forecast_matches_heat_integrated_since_observation() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(0, 60_000.0);
        history.observe(1_000, 3_000.0);
        let mut actual_c = 60.0;
        for _ in 0..400 {
            actual_c += (60_000.0 - 200.0 * (actual_c - 30.0)) / 20_000.0 * 0.001;
        }
        let fresh = history.forecast(1_400, actual_c, 0.0, plant);
        let held = history.forecast_from_measurement(1_400, 1_000, 60.0, 0.0, plant);
        let incorrectly_fresh = history.forecast(1_400, 60.0, 0.0, plant);
        assert!((held.temperature_c - fresh.temperature_c).abs() < 0.03);
        assert!(held.temperature_c - incorrectly_fresh.temperature_c > 1.0);
        assert!((held.rise_c_per_s - fresh.rise_c_per_s).abs() < 0.01);
    }

    #[test]
    fn held_temperature_forecast_also_accounts_for_unpowered_cooling() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(0, 0.0);
        let held = history.forecast_from_measurement(1_900, 1_000, 220.0, 0.0, plant);
        let incorrectly_fresh = history.forecast(1_900, 220.0, 0.0, plant);
        assert!(held.temperature_c < incorrectly_fresh.temperature_c - 1.5);
        // Forecasting never creates sensor observations or transfer evidence.
        assert_eq!(history.transfer, HeatTransferObservation::default());
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
    fn fresh_calibration_retains_observed_heat_after_input_delay_expires() {
        let plant = TrajectoryPlant {
            model: ThermalPlantProjection {
                convection_mw_per_c: 195.67747,
                radiation_mw_per_k4: 2.1190255e-7,
                thermal_capacity_mj_per_c: 25_815.854,
                transport_delay_ms: 1_900,
            },
            ambient_c: 33.90,
            max_power_mw: 105_000.0,
        };
        let mut history = AppliedHeatHistory::new();
        history.configure(1_900);
        // Fresh passive calibration, not a tuned candidate. The recorded
        // plate stays hot after all delayed electrical input has expired.
        for (now_ms, temperature_c) in [
            (78_400, 220.24),
            (78_950, 220.24),
            (79_450, 220.86),
            (79_950, 220.86),
            (80_450, 220.86),
            (80_950, 220.86),
            (81_450, 221.48),
            (81_950, 220.86),
            (82_500, 220.86),
        ] {
            history.observe(now_ms, 0.0);
            history.observe_temperature(now_ms, temperature_c, plant);
        }
        let forecast = history.forecast(82_500, 220.86, 0.0, plant);
        let mut uncorrected = history;
        uncorrected.reset_temperature_observation();
        assert!(
            (uncorrected
                .forecast(82_500, 220.86, 0.0, plant)
                .temperature_c
                - 220.24)
                .abs()
                > 1.5
        );
        assert!(
            (forecast.temperature_c - 220.24).abs() <= 1.5,
            "fresh calibration future observation: 220.24C; forecast: {forecast:?}"
        );
    }

    #[test]
    fn received_energy_is_not_added_twice_to_a_matched_delayed_plant() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        let mut temperature_c = 60.0;
        for now_ms in (0..=5_000).step_by(50) {
            let applied_mw = if now_ms < 2_000 { 60_000.0 } else { 3_000.0 };
            if now_ms > 0 {
                let delivered_mw = if now_ms <= 2_700 { 60_000.0 } else { 3_000.0 };
                temperature_c += plant.rise(temperature_c, delivered_mw) * 0.05;
            }
            history.observe(now_ms, applied_mw);
            history.observe_temperature(now_ms, temperature_c, plant);
        }
        let (_, residual_mw) = history.transfer.residual.unwrap();
        assert!(residual_mw.abs() < 100.0, "{residual_mw}");
        let mut without_observation = history;
        without_observation.reset_temperature_observation();
        assert!(
            (history
                .forecast(5_000, temperature_c, 0.0, plant)
                .temperature_c
                - without_observation
                    .forecast(5_000, temperature_c, 0.0, plant)
                    .temperature_c)
                .abs()
                < 0.01
        );
    }

    #[test]
    fn duplicate_epoch_shift_and_expired_observations_cannot_invent_heat() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(0, 0.0);
        for now_ms in (1_000..=1_700).step_by(100) {
            history.observe(now_ms, 0.0);
            history.observe_temperature(now_ms, 60.0, plant);
        }
        assert!(history.transfer.residual.is_some());
        let before_duplicate = history;
        history.observe_temperature(1_700, 90.0, plant);
        assert_eq!(history, before_duplicate);
        let residual = history.transfer.residual;
        history.reanchor_temperature();
        history.observe(1_800, 0.0);
        history.observe_temperature(1_800, 90.0, plant);
        assert_eq!(history.transfer.residual, residual);
        assert_eq!(history.transfer.valid_ms, 0);
        // Blind intervals do not refresh residual age, even if forecast is
        // called repeatedly. Sensor recovery clears it immediately.
        assert_eq!(history.residual_at(3_101, 0, plant), 0.0);
        history.observe_temperature(3_101, 90.0, plant);
        assert!(history.transfer.residual.is_none());
        assert_eq!(history.transfer.valid_ms, 0);
        history.reset_temperature_observation();
        assert!(history.transfer.origin.is_none());
        assert!(history.power_at(1_800).is_some());
    }

    #[test]
    fn small_quantization_does_not_create_a_full_power_correction() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(0, 0.0);
        for (index, now_ms) in (1_000..=5_900).step_by(100).enumerate() {
            let temperature_c = 60.0 + if index % 2 == 0 { 0.05 } else { -0.05 };
            history.observe(now_ms, 0.0);
            history.observe_temperature(now_ms, temperature_c, plant);
            let forecast = history.forecast(now_ms, temperature_c, 0.0, plant);
            assert!(forecast.temperature_c <= temperature_c + 0.15);
            assert!(forecast.temperature_c >= temperature_c - 0.25);
        }
    }

    #[test]
    fn pps_segments_accumulate_only_their_own_valid_energy_intervals() {
        let plant = plant(700);
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(0, 0.0);
        for segment in 0..7 {
            let now_ms = 1_000 + segment * 150;
            let origin_c = if segment % 2 == 0 { 60.0 } else { 80.0 };
            history.observe(now_ms, 0.0);
            history.reanchor_temperature();
            history.observe_temperature(now_ms, origin_c, plant);
            history.observe(now_ms + 100, 0.0);
            history.observe_temperature(now_ms + 100, origin_c, plant);
            if segment < 6 {
                assert!(history.transfer.residual.is_none());
                assert_eq!(history.transfer.valid_ms, (segment + 1) * 100);
            }
        }
        let (_, residual_mw) = history.transfer.residual.unwrap();
        assert!((plant.loss_mw(60.0)..=plant.loss_mw(80.0)).contains(&residual_mw));
        assert_eq!(history.transfer.valid_ms, 0);
    }

    #[test]
    fn passive_b_calibration_does_not_gain_a_false_positive_heat_tail() {
        let plant = TrajectoryPlant {
            model: ThermalPlantProjection {
                convection_mw_per_c: 21.32596,
                radiation_mw_per_k4: 9.014564e-7,
                thermal_capacity_mj_per_c: 31_067.016,
                transport_delay_ms: 700,
            },
            ambient_c: 24.73,
            max_power_mw: 63_000.0,
        };
        let mut history = AppliedHeatHistory::new();
        history.configure(700);
        history.observe(202_700, 0.0);
        for (now_ms, temperature_c) in [
            (204_750, 218.35),
            (205_250, 217.12),
            (205_750, 217.12),
            (206_250, 216.51),
            (206_750, 215.90),
            (207_250, 215.29),
            (207_750, 215.29),
        ] {
            history.observe(now_ms, 0.0);
            history.observe_temperature(now_ms, temperature_c, plant);
        }
        let predicted = history.forecast(207_750, 215.29, 0.0, plant);
        assert!(
            (predicted.temperature_c - 214.08).abs() <= 1.5,
            "{predicted:?}"
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

    #[test]
    fn approach_request_ramps_up_from_the_brake_request() {
        assert_eq!(slew_up_permille(50, 900, 100, 1_900), 103);
        assert_eq!(slew_up_permille(103, 900, 600, 1_900), 419);
    }

    #[test]
    fn approach_request_brakes_immediately_and_reaches_full_request_after_delay() {
        assert_eq!(slew_up_permille(900, 50, 50, 1_900), 50);
        assert_eq!(slew_up_permille(50, 1_000, 1_900, 1_900), 1_000);
    }
}
