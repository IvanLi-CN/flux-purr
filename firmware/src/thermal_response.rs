//! Calibration-only identification of heat still arriving after cutoff.

#[allow(unused_imports)]
use micromath::F32Ext;

use crate::memory::ThermalPlantProjection;
use crate::thermal_plant::natural_cooling_loss_mw;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatResponse {
    pub delay_ms: u32,
    pub decay_ms: u32,
    pub rmse_c: f32,
    pub maximum_error_c: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResponseSample {
    pub at_ms: u32,
    pub temperature_c: f32,
    pub power_mw: f32,
    pub accepted: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct HeatStep {
    dt_s: f32,
    alpha: f32,
    tail_s: f32,
}

impl HeatStep {
    pub(crate) fn advance(self, received_mw: &mut f32, input_mw: f32) -> f32 {
        let energy_mj = input_mw * self.dt_s + (*received_mw - input_mw) * self.tail_s;
        *received_mw = input_mw + (*received_mw - input_mw) * self.alpha;
        energy_mj
    }
}

impl HeatResponse {
    /// Exact heat-state update and integral for a held input. Integrating the
    /// heat (rather than its endpoint) preserves energy at coarse forecast steps.
    pub fn step(self, received_mw: &mut f32, input_mw: f32, dt_s: f32) -> f32 {
        self.discretize(dt_s).advance(received_mw, input_mw)
    }

    pub(crate) fn discretize(self, dt_s: f32) -> HeatStep {
        let tau_s = self.decay_ms.max(100) as f32 / 1_000.0;
        let alpha = (-dt_s / tau_s).exp();
        HeatStep {
            dt_s,
            alpha,
            tail_s: tau_s * (1.0 - alpha),
        }
    }

    pub fn horizon_ms(self) -> u64 {
        u64::from(self.delay_ms) + 4 * u64::from(self.decay_ms)
    }
}

/// C and passive losses stay fixed. Only d and tau are identified from the
/// same raw calibration's shutdown energy; no tuning candidate is an input.
pub fn fit_shutdown_response(
    samples: &[ResponseSample],
    model: ThermalPlantProjection,
    ambient_c: f32,
) -> Option<HeatResponse> {
    let cutoff = samples
        .windows(2)
        .position(|pair| pair[0].power_mw > 1_000.0 && pair[1].power_mw == 0.0)?
        + 1;
    let origin = samples[cutoff];
    let input_mw = samples[cutoff - 1].power_mw;
    if !origin.accepted
        || !samples[cutoff - 1].accepted
        || !model.thermal_capacity_mj_per_c.is_finite()
        || model.thermal_capacity_mj_per_c <= 0.0
    {
        return None;
    }
    let mut times_s = [0.0_f32; 32];
    let mut received_mj = [0.0_f32; 32];
    let mut count = 0;
    let mut loss_mj = 0.0;
    let mut previous = origin;
    for sample in &samples[cutoff + 1..] {
        let elapsed_ms = sample.at_ms.checked_sub(origin.at_ms)?;
        if elapsed_ms > 15_000 || count == times_s.len() {
            break;
        }
        // Do not bridge an excluded temperature or an input edge.
        if !sample.accepted || sample.power_mw != 0.0 || sample.at_ms <= previous.at_ms {
            return None;
        }
        let dt_s = (sample.at_ms - previous.at_ms) as f32 / 1_000.0;
        loss_mj += 0.5
            * dt_s
            * (natural_cooling_loss_mw(model, previous.temperature_c, ambient_c)
                + natural_cooling_loss_mw(model, sample.temperature_c, ambient_c));
        times_s[count] = elapsed_ms as f32 / 1_000.0;
        received_mj[count] = model.thermal_capacity_mj_per_c
            * (sample.temperature_c - origin.temperature_c)
            + loss_mj;
        count += 1;
        previous = *sample;
    }
    if count < 12 || times_s[count - 1] < 5.0 {
        return None;
    }
    let cost = |delay_s: f32, tau_s: f32| -> (f32, f32) {
        let mut square_c = 0.0;
        let mut maximum_c = 0.0_f32;
        for i in 0..count {
            let time_s = times_s[i];
            let tail_s = (time_s - delay_s).max(0.0);
            let predicted_mj =
                input_mw * (time_s.min(delay_s) + tau_s * (1.0 - (-tail_s / tau_s).exp()));
            let error_c = (predicted_mj - received_mj[i]) / model.thermal_capacity_mj_per_c;
            square_c += error_c * error_c;
            maximum_c = maximum_c.max(error_c.abs());
        }
        (square_c / count as f32, maximum_c)
    };
    let mut best = (f32::INFINITY, 0.0, 0.1, 0.0);
    for delay_step in 0..=20 {
        for tau_step in 1..=48 {
            let delay_s = delay_step as f32 * 0.25;
            let tau_s = tau_step as f32 * 0.25;
            let (error, maximum) = cost(delay_s, tau_s);
            if error < best.0 {
                best = (error, delay_s, tau_s, maximum);
            }
        }
    }
    let coarse = best;
    for delay_step in -5..=5 {
        for tau_step in -5..=5 {
            let delay_s = (coarse.1 + delay_step as f32 * 0.05).clamp(0.0, 5.0);
            let tau_s = (coarse.2 + tau_step as f32 * 0.05).clamp(0.1, 12.0);
            let (error, maximum) = cost(delay_s, tau_s);
            if error < best.0 {
                best = (error, delay_s, tau_s, maximum);
            }
        }
    }
    let response = HeatResponse {
        delay_ms: (best.1 * 1_000.0).round() as u32,
        decay_ms: (best.2 * 1_000.0).round() as u32,
        rmse_c: best.0.sqrt(),
        maximum_error_c: best.3,
    };
    (response.rmse_c <= 1.0 && response.maximum_error_c <= 2.0).then_some(response)
}

/// Full-power equilibrium heat state gives a calibration-only reference.
/// Runtime uses its actual heat state instead of assuming equilibrium.
pub fn reference_brake_c(
    response: HeatResponse,
    model: ThermalPlantProjection,
    ambient_c: f32,
    maximum_power_mw: f32,
    target_c: f32,
) -> f32 {
    let step = response.discretize(0.1);
    let peak = |switch_c: f32| {
        let mut temperature_c = switch_c;
        let mut peak_c = switch_c;
        let mut received_mw = maximum_power_mw;
        let mut at_ms = 0;
        while at_ms < response.horizon_ms() {
            let input_mw = if at_ms < u64::from(response.delay_ms) {
                maximum_power_mw
            } else {
                maximum_power_mw * 0.05
            };
            let energy = step.advance(&mut received_mw, input_mw);
            temperature_c += (energy
                - natural_cooling_loss_mw(model, temperature_c, ambient_c) * 0.1)
                / model.thermal_capacity_mj_per_c;
            peak_c = peak_c.max(temperature_c);
            at_ms += 100;
        }
        peak_c
    };
    let mut lower = (target_c - 50.0).max(ambient_c);
    let mut upper = target_c;
    for _ in 0..12 {
        let mid = (lower + upper) * 0.5;
        if peak(mid) > target_c {
            upper = mid;
        } else {
            lower = mid;
        }
    }
    ((target_c - (lower + upper) * 0.5) * 100.0)
        .round()
        .clamp(100.0, 5_000.0)
        / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_fresh_calibration_shutdown_without_tuning_feedback() {
        // Rounded exported temperatures from calibration transaction 1348294812.
        // The input is the final heated sample's VIN²/R, not its 5A ceiling.
        let points = [
            (0, 220.24),
            (500, 220.24),
            (1_000, 220.86),
            (1_500, 220.86),
            (2_000, 220.86),
            (2_550, 220.24),
            (3_050, 220.24),
            (3_550, 219.62),
            (4_100, 219.62),
            (4_600, 219.62),
            (5_100, 218.39),
            (5_650, 217.78),
            (6_150, 217.78),
            (6_650, 216.55),
            (7_150, 215.94),
            (7_650, 215.94),
            (8_150, 214.72),
            (8_700, 214.11),
            (9_200, 213.50),
            (9_750, 212.28),
            (12_900, 208.05),
        ];
        let mut samples = [ResponseSample::default(); 22];
        samples[0] = ResponseSample {
            at_ms: 0,
            temperature_c: 220.24,
            power_mw: 76_150.0,
            accepted: true,
        };
        for (index, (time, temperature)) in points.into_iter().enumerate() {
            samples[index + 1] = ResponseSample {
                at_ms: time + 50,
                temperature_c: temperature,
                power_mw: 0.0,
                accepted: true,
            };
        }
        let model = ThermalPlantProjection {
            convection_mw_per_c: 198.67226,
            radiation_mw_per_k4: 2.2978014e-7,
            thermal_capacity_mj_per_c: 25_843.268,
            transport_delay_ms: 850,
        };
        let response = fit_shutdown_response(&samples, model, 29.83).unwrap();
        assert!(response.delay_ms <= 300, "{response:?}");
        assert!((3_500..=4_500).contains(&response.decay_ms), "{response:?}");
        assert!(response.rmse_c < 0.6, "{response:?}");
        let brake = reference_brake_c(response, model, 29.83, 105_000.0, 60.0);
        assert!(brake > 10.0 && brake < 18.0, "{brake}");
        let mut rejected = samples;
        rejected[5].accepted = false;
        assert!(fit_shutdown_response(&rejected, model, 29.83).is_none());
    }
}
