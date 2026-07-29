use crate::memory::ThermalPlantProjection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalPlantControlOutput {
    pub requested_power_mw: f32,
    pub loss_feedforward_mw: f32,
    pub approach_power_limit_mw: f32,
    pub predicted_error_c: f32,
    pub saturated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalPlantControlInput {
    pub model: ThermalPlantProjection,
    pub target_temp_c: f32,
    pub current_temp_c: f32,
    pub ambient_temp_c: f32,
    pub slope_c_per_s: f32,
    pub dt_s: f32,
    pub max_power_mw: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermalPlantController {
    integral_error_c_s: f32,
}

impl Default for ThermalPlantController {
    fn default() -> Self {
        Self {
            integral_error_c_s: 0.0,
        }
    }
}

pub fn control_target_temperature_c(target_temp_c: f32) -> f32 {
    target_temp_c
}

impl ThermalPlantController {
    pub const fn new() -> Self {
        Self {
            integral_error_c_s: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.integral_error_c_s = 0.0;
    }

    pub fn update(&mut self, input: ThermalPlantControlInput) -> ThermalPlantControlOutput {
        let ThermalPlantControlInput {
            model,
            target_temp_c,
            current_temp_c,
            ambient_temp_c,
            slope_c_per_s,
            dt_s,
            max_power_mw,
        } = input;
        let dt_s = dt_s.clamp(0.001, 1.0);
        let max_power_mw = max_power_mw.max(0.0);
        let control_target_temp_c = control_target_temperature_c(target_temp_c);
        let target_k = (control_target_temp_c + 273.15).max(1.0);
        let ambient_k = (ambient_temp_c + 273.15).max(1.0);
        let target_k2 = target_k * target_k;
        let ambient_k2 = ambient_k * ambient_k;
        let loss_feedforward_mw = (model.convection_mw_per_c
            * (control_target_temp_c - ambient_temp_c).max(0.0)
            + model.radiation_mw_per_k4
                * (target_k2 * target_k2 - ambient_k2 * ambient_k2).max(0.0))
        .max(0.0);
        let delay_s = model.transport_delay_ms as f32 / 1_000.0;
        let error_c = control_target_temp_c - current_temp_c;
        let predicted_error_c = error_c - delay_s * slope_c_per_s;
        // The calibration delay is the time that already-applied heat can keep
        // raising the RTD reading. Bound approach power by the heat capacity of
        // the remaining temperature error so the PI cannot bank more energy
        // than the plate can absorb before that delayed response arrives.
        // Only half of the fitted transport delay remains as unobserved heat
        // once the RTD has begun responding. Keeping that margin still brakes
        // before hold, while avoiding a low-power plateau below the target.
        let brake_horizon_s = if target_temp_c <= 60.0 {
            // The 60 C anchor retains more latent energy than its small
            // radiative-loss term predicts. Brake earlier so hold entry does
            // not carry several degrees of delayed heat into the band.
            (delay_s * 0.75).clamp(3.0, 8.0)
        } else if target_temp_c <= 100.0 {
            // 80 C retains enough delayed heat to use the full fitted delay.
            // Keep the 60 C setting separate: its compensated control target
            // already satisfies the hold gate with the shorter window.
            delay_s.clamp(4.0, 10.0)
        } else if target_temp_c >= 200.0 {
            // At the upper end of the validated range, radiative loss dominates
            // the final approach. A half-delay energy budget leaves the plate
            // below the hold band for too long; reserve only the portion that
            // remains unobserved after the RTD has crossed into the tail.
            (delay_s * 0.3).clamp(2.0, 4.0)
        } else {
            (delay_s * 0.5).clamp(2.0, 6.0)
        };
        let approach_rate_c_per_s = (error_c.max(0.0) / brake_horizon_s).min(1.8);
        let approach_power_limit_mw = (loss_feedforward_mw
            + model.thermal_capacity_mj_per_c * approach_rate_c_per_s)
            .clamp(0.0, max_power_mw);
        // Use the modelled transport delay for every target. Extending this
        // horizon at low temperatures leaves the PI unable to replace real
        // heat loss, which creates a persistent below-target equilibrium.
        let near_target_reheat_horizon_s = brake_horizon_s;
        let near_target_power_limit_mw = if error_c <= 2.0 {
            (loss_feedforward_mw
                + model.thermal_capacity_mj_per_c
                    * (error_c.max(0.0) / near_target_reheat_horizon_s))
                .clamp(0.0, max_power_mw)
        } else {
            max_power_mw
        };
        let approach_power_limit_mw = approach_power_limit_mw.min(near_target_power_limit_mw);

        let near_target = error_c.abs() <= 3.0;
        let response_time_s = if near_target && target_temp_c >= 200.0 {
            3.0
        } else if near_target && target_temp_c >= 160.0 {
            // The 3 A path has a long measured transport delay. At the
            // 160-180 C range, the middle-band gain leaves too much applied
            // heat after the plate first enters the stability band. Use a
            // shorter, still damped response time so the loss model removes
            // that tail energy before it becomes a hold-window overshoot.
            5.0
        } else if near_target {
            8.0
        } else {
            1.5
        };
        let integration_time_s = if near_target && target_temp_c >= 200.0 {
            12.0
        } else if near_target && target_temp_c >= 160.0 {
            18.0
        } else if near_target {
            30.0
        } else {
            4.0
        };
        let kp_mw_per_c =
            (model.thermal_capacity_mj_per_c / response_time_s).clamp(50.0, 100_000.0);
        let ki_mw_per_c_s = (kp_mw_per_c / integration_time_s).clamp(1.0, 25_000.0);
        let unsaturated = loss_feedforward_mw
            + kp_mw_per_c * predicted_error_c
            + ki_mw_per_c_s * self.integral_error_c_s;
        let requested_power_mw = unsaturated.clamp(0.0, approach_power_limit_mw);
        let saturated = (requested_power_mw - unsaturated).abs() > 0.5;
        let integration_pushes_into_saturation = (unsaturated > approach_power_limit_mw
            && error_c > 0.0)
            || (unsaturated < 0.0 && error_c < 0.0);
        if !saturated || !integration_pushes_into_saturation {
            self.integral_error_c_s =
                (self.integral_error_c_s + error_c * dt_s).clamp(-2_000.0, 2_000.0);
        }

        ThermalPlantControlOutput {
            requested_power_mw,
            loss_feedforward_mw,
            approach_power_limit_mw,
            predicted_error_c,
            saturated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_target_matches_the_requested_temperature() {
        assert_eq!(control_target_temperature_c(60.0), 60.0);
        assert_eq!(control_target_temperature_c(80.0), 80.0);
        assert_eq!(control_target_temperature_c(150.0), 150.0);
        assert_eq!(control_target_temperature_c(180.0), 180.0);
    }

    fn model() -> ThermalPlantProjection {
        ThermalPlantProjection {
            convection_mw_per_c: 100.0,
            radiation_mw_per_k4: 0.000_000_1,
            thermal_capacity_mj_per_c: 800.0,
            transport_delay_ms: 500,
        }
    }

    #[test]
    fn feedforward_is_positive_above_ambient() {
        let mut controller = ThermalPlantController::default();
        let output = controller.update(ThermalPlantControlInput {
            model: model(),
            target_temp_c: 140.0,
            current_temp_c: 140.0,
            ambient_temp_c: 25.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });
        assert!(output.loss_feedforward_mw > 0.0);
        assert!((output.requested_power_mw - output.loss_feedforward_mw).abs() < 1.0);
    }

    #[test]
    fn predictive_error_brakes_a_fast_rise() {
        let mut controller = ThermalPlantController::default();
        let rising = controller.update(ThermalPlantControlInput {
            model: model(),
            target_temp_c: 100.0,
            current_temp_c: 95.0,
            ambient_temp_c: 25.0,
            slope_c_per_s: 12.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });
        controller.reset();
        let flat = controller.update(ThermalPlantControlInput {
            model: model(),
            target_temp_c: 100.0,
            current_temp_c: 95.0,
            ambient_temp_c: 25.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });
        assert!(rising.predicted_error_c < 0.0);
        assert!(rising.requested_power_mw < flat.requested_power_mw);
    }

    #[test]
    fn anti_windup_recovers_after_upper_saturation() {
        let mut controller = ThermalPlantController::default();
        for _ in 0..200 {
            let output = controller.update(ThermalPlantControlInput {
                model: model(),
                target_temp_c: 240.0,
                current_temp_c: 25.0,
                ambient_temp_c: 25.0,
                slope_c_per_s: 0.0,
                dt_s: 0.05,
                max_power_mw: 1_000.0,
            });
            assert_eq!(output.requested_power_mw, 1_000.0);
        }
        let output = controller.update(ThermalPlantControlInput {
            model: model(),
            target_temp_c: 25.0,
            current_temp_c: 30.0,
            ambient_temp_c: 25.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });
        assert_eq!(output.requested_power_mw, 0.0);
    }

    #[test]
    fn delayed_candidate_limits_approach_energy_before_hold() {
        let mut controller = ThermalPlantController::default();
        let model = ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.000_001_461_697_4,
            thermal_capacity_mj_per_c: 42_576.72,
            transport_delay_ms: 10_000,
        };
        let output = controller.update(ThermalPlantControlInput {
            model,
            target_temp_c: 60.0,
            current_temp_c: 56.0,
            ambient_temp_c: 33.91,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 64_000.0,
        });

        // Replay point from the failing 60 C HIL trace. The old direct-target
        // PI saturated at 64 W here; the model-aware governor must start
        // braking before the 1.5 C hold threshold without stalling at 58 C.
        assert!(output.approach_power_limit_mw < 50_000.0);
        assert!(output.requested_power_mw <= output.approach_power_limit_mw);
        assert!(output.saturated);
    }

    #[test]
    fn candidate_bounds_reheat_budget_inside_hold_entry_band() {
        let mut controller = ThermalPlantController::default();
        let model = ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.000_001_461_697_4,
            thermal_capacity_mj_per_c: 42_576.72,
            transport_delay_ms: 10_000,
        };
        let output = controller.update(ThermalPlantControlInput {
            model,
            target_temp_c: 60.0,
            current_temp_c: 58.5,
            ambient_temp_c: 33.91,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 64_000.0,
        });

        assert!(output.approach_power_limit_mw < 30_000.0);
        assert!(output.requested_power_mw <= output.approach_power_limit_mw);
    }

    #[test]
    fn high_target_reheat_uses_full_brake_horizon() {
        let mut controller = ThermalPlantController::default();
        let model = ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.000_001_461_697_4,
            thermal_capacity_mj_per_c: 42_576.72,
            transport_delay_ms: 10_000,
        };
        let output = controller.update(ThermalPlantControlInput {
            model,
            target_temp_c: 140.0,
            current_temp_c: 138.5,
            ambient_temp_c: 33.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 64_000.0,
        });

        assert!(!output.saturated);
        assert!(output.requested_power_mw > output.loss_feedforward_mw + 7_000.0);
    }

    #[test]
    fn upper_range_tail_reserves_enough_energy_for_the_hold_band() {
        let mut controller = ThermalPlantController::default();
        let model = ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.000_001_461_697_4,
            thermal_capacity_mj_per_c: 42_576.72,
            transport_delay_ms: 10_000,
        };
        let output = controller.update(ThermalPlantControlInput {
            model,
            target_temp_c: 220.0,
            current_temp_c: 218.5,
            ambient_temp_c: 33.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });

        assert!(output.requested_power_mw > output.loss_feedforward_mw + 20_000.0);
    }

    #[test]
    fn high_intermediate_tail_brakes_before_the_stability_window() {
        let mut controller = ThermalPlantController::default();
        let model = ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.000_001_461_697_4,
            thermal_capacity_mj_per_c: 42_576.72,
            transport_delay_ms: 10_000,
        };
        let output = controller.update(ThermalPlantControlInput {
            model,
            target_temp_c: 180.0,
            current_temp_c: 178.83,
            ambient_temp_c: 33.91,
            slope_c_per_s: 0.387,
            dt_s: 0.05,
            max_power_mw: 64_000.0,
        });

        // This is the 180 C HIL tail state. Keeping the middle-band gain here
        // continued to inject roughly 65% output and broke the 10 s stable
        // window with a small late overshoot.
        assert!(output.requested_power_mw < 28_000.0);
        assert!(output.requested_power_mw < output.loss_feedforward_mw);
    }

    #[test]
    fn near_target_gain_schedule_limits_pi_reheat() {
        let mut controller = ThermalPlantController::default();
        let output = controller.update(ThermalPlantControlInput {
            model: model(),
            target_temp_c: 100.0,
            current_temp_c: 99.5,
            ambient_temp_c: 25.0,
            slope_c_per_s: 0.0,
            dt_s: 0.05,
            max_power_mw: 100_000.0,
        });

        assert!(output.requested_power_mw < output.loss_feedforward_mw + 5_000.0);
    }
}
