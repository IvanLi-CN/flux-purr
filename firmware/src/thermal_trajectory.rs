//! Feasible temperature references and delayed applied-heat prediction.
//! Electrical input is estimated by the runtime, not measured by this model.

use crate::memory::ThermalPlantProjection;
use crate::thermal_plant::natural_cooling_loss_mw;
use crate::thermal_response::{HeatResponse, HeatStep};

const HEAT_BUCKETS: usize = 64;
const MEASUREMENT_GAP_MS: u64 = 1_000;

struct ForecastBias {
    extra_loss_mw: f32,
    stored_heat_mw: f32,
}

impl ForecastBias {
    fn new(net_mw: f32) -> Self {
        Self {
            extra_loss_mw: net_mw.min(0.0),
            stored_heat_mw: net_mw.max(0.0),
        }
    }

    fn energy(&mut self, step: HeatStep, dt_s: f32) -> f32 {
        self.extra_loss_mw * dt_s + step.advance(&mut self.stored_heat_mw, 0.0)
    }

    fn net_mw(&self) -> f32 {
        self.extra_loss_mw + self.stored_heat_mw
    }
}

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
    received_energy_mj: f32,
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

#[derive(Clone, Copy, Debug)]
pub struct TrackingTarget {
    pub temperature_c: f32,
    pub minimum_permille: u16,
    pub ahead_ms: u64,
    /// Time for this correction before resuming the target's maintenance heat.
    pub command_ms: u64,
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
    response: Option<HeatResponse>,
    received_mw: f32,
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
            response: None,
            received_mw: 0.0,
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
                received_energy_mj: 0.0,
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
        if let Some(response) = self.response {
            let mut delivered_at_ms = at_ms;
            while delivered_at_ms < now_ms {
                let end_ms = (delivered_at_ms + 50).min(now_ms);
                let input_mw = delivered_at_ms
                    .checked_sub(u64::from(response.delay_ms))
                    .and_then(|source_ms| self.power_at(source_ms))
                    .unwrap_or(0.0);
                response.step(
                    &mut self.received_mw,
                    input_mw,
                    (end_ms - delivered_at_ms) as f32 / 1_000.0,
                );
                delivered_at_ms = end_ms;
            }
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

    pub fn configure_response(&mut self, response: Option<HeatResponse>) {
        if self.response != response {
            self.response = response;
            self.received_mw = 0.0;
        }
    }

    /// Same-state reachability bounds for trajectory inversion. Electrical
    /// history is fixed; only commands issued from now onward are varied.
    pub fn predict_response(
        &self,
        now_ms: u64,
        measured_at_ms: u64,
        measured_c: f32,
        future_power_mw: f32,
        horizon_ms: u64,
        plant: TrajectoryPlant,
    ) -> Option<(ReferenceState, f32)> {
        let response = self.response?;
        let start_ms = measured_at_ms.min(now_ms);
        let mut received_mw = self.response_heat_at(start_ms, now_ms)?;
        let mut temperature_c = measured_c;
        let mut peak_c = measured_c;
        let mut at_ms = start_ms;
        let end_ms = now_ms + horizon_ms;
        let mut bias = ForecastBias::new(self.response_bias_mw(now_ms));
        let step = response.discretize(0.1);
        while at_ms < end_ms {
            let next_ms = (at_ms + 100).min(end_ms);
            let input_mw = self.forecast_input_mw(at_ms, now_ms, future_power_mw, response);
            let dt_s = (next_ms - at_ms) as f32 / 1_000.0;
            let interval_step = if next_ms - at_ms == 100 {
                step
            } else {
                response.discretize(dt_s)
            };
            let energy = interval_step.advance(&mut received_mw, input_mw);
            let bias_energy = bias.energy(interval_step, dt_s);
            temperature_c += (energy + bias_energy - plant.loss_mw(temperature_c) * dt_s)
                / plant.model.thermal_capacity_mj_per_c;
            peak_c = peak_c.max(temperature_c);
            at_ms = next_ms;
        }
        Some((
            ReferenceState {
                temperature_c,
                rise_c_per_s: plant.rise(temperature_c, received_mw + bias.net_mw()),
            },
            peak_c,
        ))
    }

    fn response_heat_at(&self, start_ms: u64, now_ms: u64) -> Option<f32> {
        let response = self.response?;
        let mut received_mw = self.received_mw;
        // Reconstruct the heat state at the accepted reading. This is a
        // short, bounded inverse integration, not a fabricated measurement.
        let mut at_ms = self.last_at_ms.unwrap_or(start_ms).min(now_ms);
        while at_ms > start_ms {
            let before_ms = at_ms.saturating_sub(50).max(start_ms);
            let input_mw = before_ms
                .checked_sub(u64::from(response.delay_ms))
                .and_then(|source_ms| self.power_at(source_ms))
                .unwrap_or(0.0);
            let mut unit = 1.0;
            response.step(&mut unit, 0.0, (at_ms - before_ms) as f32 / 1_000.0);
            // A reduced PPS setting bounds future input, not heat stored
            // under an earlier, stronger contract.
            received_mw = (input_mw + (received_mw - input_mw) / unit).max(0.0);
            at_ms = before_ms;
        }
        while at_ms < start_ms {
            let next_ms = (at_ms + 50).min(start_ms);
            let input_mw = self.forecast_input_mw(at_ms, now_ms, 0.0, response);
            response.step(
                &mut received_mw,
                input_mw,
                (next_ms - at_ms) as f32 / 1_000.0,
            );
            at_ms = next_ms;
        }
        Some(received_mw)
    }

    fn forecast_input_mw(
        &self,
        at_ms: u64,
        now_ms: u64,
        future_mw: f32,
        response: HeatResponse,
    ) -> f32 {
        let Some(source_ms) = at_ms.checked_sub(u64::from(response.delay_ms)) else {
            return 0.0;
        };
        if source_ms < now_ms {
            self.power_at(source_ms).unwrap_or(self.current_power_mw)
        } else {
            future_mw
        }
    }

    pub fn tracking_request_permille(
        &self,
        now_ms: u64,
        measured_at_ms: u64,
        measured_c: f32,
        target: TrackingTarget,
        plant: TrajectoryPlant,
    ) -> Option<u16> {
        self.tracking_request_bounds(now_ms, measured_at_ms, measured_c, target, plant, 0.0)
            .map(|(request, _)| request)
    }

    /// Return endpoint feedforward separately from the complete peak limit,
    /// so feedback can correct a model error without defeating that limit.
    pub fn tracking_request_bounds(
        &self,
        now_ms: u64,
        measured_at_ms: u64,
        measured_c: f32,
        target: TrackingTarget,
        plant: TrajectoryPlant,
        peak_headroom_c: f32,
    ) -> Option<(u16, u16)> {
        let response = self.response?;
        let mut at_ms = measured_at_ms.min(now_ms);
        let heat = self.response_heat_at(at_ms, now_ms)?;
        let mut heats = [heat; 2];
        let mut temperatures = [measured_c; 2];
        // A falling plate can already be above its reference. Restoring heat
        // toward that reference must not be blocked by an unavoidable initial
        // temperature; prevent a new peak above the existing temperature.
        let peak_ceiling_c = (target.temperature_c + peak_headroom_c.max(0.0)).max(measured_c);
        let mut peak_request = 1_000.0_f32;
        let end_ms = now_ms + target.ahead_ms;
        let mut bias = ForecastBias::new(self.response_bias_mw(now_ms));
        let command_end_ms = now_ms
            .saturating_add(target.command_ms)
            .saturating_add(u64::from(response.delay_ms));
        let step = response.discretize(0.1);
        while at_ms < end_ms {
            let next_ms = (at_ms + 100).min(end_ms);
            let dt_s = (next_ms - at_ms) as f32 / 1_000.0;
            let interval_step = if next_ms - at_ms == 100 {
                step
            } else {
                response.discretize(dt_s)
            };
            let maintenance_mw = (plant.loss_mw(target.temperature_c) - bias.net_mw())
                .clamp(0.0, plant.max_power_mw);
            let future_powers = if at_ms < command_end_ms {
                [0.0, plant.max_power_mw]
            } else {
                [maintenance_mw; 2]
            };
            let bias_energy = bias.energy(interval_step, dt_s);
            for (index, future_mw) in future_powers.into_iter().enumerate() {
                let input_mw = self.forecast_input_mw(at_ms, now_ms, future_mw, response);
                let energy = interval_step.advance(&mut heats[index], input_mw);
                temperatures[index] += (energy + bias_energy
                    - plant.loss_mw(temperatures[index]) * dt_s)
                    / plant.model.thermal_capacity_mj_per_c;
            }
            let difference_c = temperatures[1] - temperatures[0];
            if next_ms > now_ms
                && (difference_c > 0.000_01 || temperatures[0] > peak_ceiling_c + 0.001)
            {
                peak_request = peak_request
                    .min((peak_ceiling_c - temperatures[0]) / difference_c.max(0.000_01) * 1_000.0);
            }
            at_ms = next_ms;
        }
        let endpoint_request = (target.temperature_c - temperatures[0])
            / (temperatures[1] - temperatures[0]).max(0.001)
            * 1_000.0;
        Some((
            endpoint_request
                .min(peak_request)
                .clamp(f32::from(target.minimum_permille), 1_000.0) as u16,
            peak_request.clamp(f32::from(target.minimum_permille), 1_000.0) as u16,
        ))
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
        let delay_ms = u64::from(
            self.response
                .map_or(plant.model.transport_delay_ms, |response| response.delay_ms),
        );
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
        let window_ms = if self.response.is_some() {
            delay_ms.max(1_000) * 2
        } else {
            delay_ms.max(100) * 2
        };
        if self
            .transfer
            .window_start_ms
            .is_some_and(|start_ms| now_ms.saturating_sub(start_ms) > window_ms)
        {
            self.clear_transfer_energy_window();
        }
        self.transfer.window_start_ms.get_or_insert(origin_ms);
        self.transfer.valid_ms += elapsed_ms;
        self.transfer.last_interval_ms = Some(now_ms);
        self.transfer.temperature_delta_c += temperature_c - origin_c;
        self.transfer.loss_energy_mj += plant.loss_mw((origin_c + temperature_c) * 0.5) * elapsed_s;
        self.transfer.delayed_energy_mj += delayed_energy_mj;
        if self.response.is_some() {
            self.correct_response_heat(origin_ms, now_ms, plant);
            return;
        }
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

    fn response_interval_energy(&self, start_ms: u64, end_ms: u64) -> Option<f32> {
        let response = self.response?;
        let mut at_ms = self.last_at_ms?;
        let mut received_mw = self.received_mw;
        while at_ms < end_ms {
            let next_ms = (at_ms + 50).min(end_ms);
            let input_mw = self.forecast_input_mw(at_ms, end_ms, self.current_power_mw, response);
            response.step(
                &mut received_mw,
                input_mw,
                (next_ms - at_ms) as f32 / 1_000.0,
            );
            at_ms = next_ms;
        }
        if at_ms != end_ms {
            return None;
        }
        let mut energy_mj = 0.0;
        while at_ms > start_ms {
            let previous_ms = at_ms.saturating_sub(50).max(start_ms);
            let dt_s = (at_ms - previous_ms) as f32 / 1_000.0;
            let input_mw = previous_ms
                .checked_sub(u64::from(response.delay_ms))
                .and_then(|source_ms| self.power_at(source_ms))?;
            let mut alpha = 1.0;
            response.step(&mut alpha, 0.0, dt_s);
            received_mw = input_mw + (received_mw - input_mw) / alpha;
            let mut before = received_mw;
            energy_mj += response.step(&mut before, input_mw, dt_s);
            at_ms = previous_ms;
        }
        Some(energy_mj)
    }

    fn correct_response_heat(&mut self, start_ms: u64, now_ms: u64, plant: TrajectoryPlant) {
        let Some(energy_mj) = self.response_interval_energy(start_ms, now_ms) else {
            self.clear_transfer_energy_window();
            return;
        };
        self.transfer.received_energy_mj += energy_mj;
        if self.transfer.valid_ms < 1_000 {
            return;
        }
        // Keep calibrated electrical heat separate from the observed net heat
        // discrepancy. Correcting q alone makes a persistent extra loss decay
        // away in every forecast, producing a permanent tracking offset.
        let innovation_mj = self.transfer.loss_energy_mj
            + plant.model.thermal_capacity_mj_per_c * self.transfer.temperature_delta_c
            - self.transfer.received_energy_mj;
        let discrepancy_mw = innovation_mj / (self.transfer.valid_ms as f32 / 1_000.0);
        let previous_mw = self.response_bias_mw(now_ms);
        let bias_mw = (0.75 * previous_mw + 0.25 * discrepancy_mw)
            .clamp(-plant.max_power_mw, plant.max_power_mw);
        self.transfer.residual = Some((now_ms, bias_mw));
        self.clear_transfer_energy_window();
    }

    fn response_bias_mw(&self, now_ms: u64) -> f32 {
        let Some((observed_ms, bias_mw)) = self.transfer.residual else {
            return 0.0;
        };
        let lifetime_ms = self
            .response
            .map_or(0, |response| u64::from(response.decay_ms).max(1_000) * 2);
        if now_ms < observed_ms || now_ms - observed_ms > lifetime_ms {
            0.0
        } else {
            bias_mw
        }
    }

    fn clear_transfer_energy_window(&mut self) {
        self.transfer.window_start_ms = None;
        self.transfer.valid_ms = 0;
        self.transfer.temperature_delta_c = 0.0;
        self.transfer.loss_energy_mj = 0.0;
        self.transfer.delayed_energy_mj = 0.0;
        self.transfer.received_energy_mj = 0.0;
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
        if let Some(response) = self.response {
            return self
                .predict_response(
                    now_ms,
                    measured_at_ms,
                    measured_c,
                    self.current_power_mw,
                    u64::from(response.delay_ms),
                    plant,
                )
                .unwrap()
                .0;
        }
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

    /// Forecasting a known reference is different from accepting a long gap
    /// in live measurements. Advance bounded chunks without truncating time.
    pub fn projected(
        self,
        ahead_ms: u64,
        target_c: f32,
        horizon_s: f32,
        damping: f32,
        plant: TrajectoryPlant,
    ) -> ReferenceState {
        let mut future = self;
        let end_ms = self.last_at_ms + ahead_ms;
        while future.last_at_ms < end_ms {
            future.advance(
                (future.last_at_ms + 5_000).min(end_ms),
                target_c,
                horizon_s,
                damping,
                plant,
            );
        }
        future.reference
    }
}

#[cfg(test)]
#[path = "thermal_trajectory_tests.rs"]
mod tests;
