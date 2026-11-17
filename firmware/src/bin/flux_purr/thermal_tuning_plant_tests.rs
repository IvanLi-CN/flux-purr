use super::*;
use flux_purr_firmware::thermal_tuning as tuning;

#[test]
fn confirmed_safe_pps_full_scale_uses_the_controller_power_basis() {
    // Reproduce the production chain: controller fraction -> integer-mW
    // budget -> confirmed PPS gate. The voltage has already passed the
    // whole-device boundary/response checks at the caller.
    for current_ma in [3_250, 3_000, 5_000] {
        for resistance_ohms in [5.674_f32, 3.2, 7.5] {
            let full_power = automatic_heater_power_budget_mw(21_000, resistance_ohms, current_ma);
            for requested_percent in [100_u8, 50, 5, 0] {
                let requested_mw = full_power * u32::from(requested_percent) / 100;
                let actual = automatic_heater_pps_pwm_percent_for_power(
                    requested_mw,
                    resistance_ohms,
                    21_000,
                    current_ma,
                    100,
                );
                assert!(
                    actual.abs_diff(requested_percent) <= 1,
                    "controller requested {requested_percent}% but applied {actual}% at {current_ma}mA / {resistance_ohms}ohm"
                );
            }
        }
    }
}

#[test]
fn pps_power_allocation_rejects_missing_facts_and_keeps_output_limits() {
    for (voltage_mv, current_ma, resistance_ohms) in [
        (0, 3_250, 5.674),
        (21_000, 0, 5.674),
        (21_000, 3_250, 0.0),
        (21_000, 3_250, f32::NAN),
    ] {
        assert_eq!(
            automatic_heater_pps_pwm_percent_for_power(
                68_250,
                resistance_ohms,
                voltage_mv,
                current_ma,
                100,
            ),
            0
        );
    }
    assert_eq!(
        automatic_heater_pps_pwm_percent_for_power(u32::MAX, 5.674, 21_000, 3_250, 42),
        42
    );
    // Fixed PD does not carry the proven-safe PPS full-conduction contract.
    assert_eq!(
        automatic_heater_pwm_percent_for_power(68_250, 5.674, 21_000, 100),
        87
    );
}

#[test]
fn tuning_candidate_brake_reaches_the_production_controller_unchanged() {
    for power_class in [
        tuning::ThermalTuningPowerClass::Pps3a,
        tuning::ThermalTuningPowerClass::Pps5a,
    ] {
        for brake_centi_c in [60, 70, 100, 150] {
            let mut candidate = tuning::factory_default_profile(power_class);
            let point = candidate.points.iter_mut().flatten().last().unwrap();
            point.brake_distance_centi_c = brake_centi_c;
            let target_c = point.target_temp_c;
            // This is the preview/persisted-profile conversion used by the
            // production controller, not the tuning search's own data model.
            let profile = ThermalControlProfile::from_saved_config(&candidate).unwrap();
            let target = profile.control_target(target_c);
            assert_eq!(
                target.brake_distance_c,
                f32::from(brake_centi_c) / 100.0,
                "{power_class:?} candidate brake was silently changed"
            );
        }
    }
}

#[test]
fn factory_candidate_search_qualifies_a_calibrated_long_delay_plant() {
    for (max_power_mw, noise_c) in [
        (28_000.0, 0.0),
        (35_000.0, 0.0),
        (65_000.0, 0.0),
        (28_000.0, 0.75),
    ] {
        let mut replay = PlantReplay::new(max_power_mw);
        replay.noise_c = noise_c;
        for now_ms in (0..=tuning::THERMAL_TUNING_TARGET_BUDGET_MS).step_by(50) {
            replay.step(now_ms);
            if !replay.tuning.armed {
                break;
            }
        }
        assert_eq!(
            replay.tuning.accepted_target_mask,
            1,
            "terminal={:?}, failed={}, trials={}, decisions={:?}",
            replay.tuning.terminal_disposition,
            replay.tuning.failed_target_mask,
            replay.tuning.trial_index,
            replay.decisions
        );
        assert_eq!(replay.tuning.failed_target_mask, 0);
        assert!(!replay.tuning.armed);
        assert!(!replay.tuning.save_confirmed);
    }
}

#[test]
fn factory_candidate_search_handles_a_colder_environment_than_calibration() {
    let mut replay = PlantReplay::new(28_000.0);
    // The physical room can cool after calibration. Its stored reference
    // remains 30 C; accepted measurements still contain bounded disturbance.
    replay.actual_ambient_c = 25.0;
    replay.noise_c = 0.75;
    for now_ms in (0..=tuning::THERMAL_TUNING_TARGET_BUDGET_MS).step_by(50) {
        replay.step(now_ms);
        if !replay.tuning.armed {
            break;
        }
    }
    assert_eq!(
        replay.tuning.accepted_target_mask, 1,
        "terminal={:?}, trials={}, decisions={:?}",
        replay.tuning.terminal_disposition, replay.tuning.trial_index, replay.decisions
    );
    assert_eq!(replay.tuning.failed_target_mask, 0);
    assert!(!replay.tuning.armed);
    assert!(!replay.tuning.save_confirmed);
}

struct PlantReplay {
    tuning: tuning::ThermalTuningRuntime,
    controller: HeaterController,
    input: ThermalPlantRuntimeInput,
    plate_c: f32,
    actual_ambient_c: f32,
    noise_c: f32,
    peak_c: f32,
    delayed_power: std::collections::VecDeque<(u64, f32)>,
    cursor: Option<u32>,
    decisions: std::vec::Vec<(u8, std::string::String)>,
}

impl PlantReplay {
    fn new(max_power_mw: f32) -> Self {
        let mut tuning = tuning::ThermalTuningRuntime::new();
        tuning
            .start(tuning::ThermalTuningStart {
                run_id: "plant-replay".try_into().unwrap(),
                candidate_id: "plant-replay".try_into().unwrap(),
                candidate_hash: "unused-factory-hash".try_into().unwrap(),
                power_class: tuning::ThermalTuningPowerClass::Pps3a,
                targets: &[60],
                pps_ready: true,
                thermal_model_available: true,
                calibration_conflict: false,
                candidate_profile: None,
            })
            .unwrap();
        let mut input = production_test_input(34.9);
        input.ambient_temp_c = 30.0;
        input.max_power_mw = max_power_mw;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 211.08168,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 30_753.62,
            transport_delay_ms: 4_500,
        };
        Self {
            tuning,
            controller: HeaterController::new(),
            plate_c: input.measured_temp_c,
            actual_ambient_c: input.ambient_temp_c,
            noise_c: 0.0,
            peak_c: input.measured_temp_c,
            input,
            delayed_power: std::collections::VecDeque::from([(0, 0.0)]),
            cursor: None,
            decisions: std::vec::Vec::new(),
        }
    }

    fn step(&mut self, now_ms: u64) {
        let source_at = now_ms.saturating_sub(u64::from(self.input.model.transport_delay_ms));
        while self.delayed_power.len() > 1 && self.delayed_power[1].0 <= source_at {
            self.delayed_power.pop_front();
        }
        // Active fan cooling is a scenario assumption, not a fitted device
        // coefficient. Heating still uses the measured passive projection.
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            self.input.model,
            self.plate_c,
            self.actual_ambient_c,
        ) * if self.tuning.phase == tuning::ThermalTuningPhase::Cooldown {
            3.0
        } else {
            1.0
        };
        self.plate_c +=
            (self.delayed_power[0].1 - loss) / self.input.model.thermal_capacity_mj_per_c * 0.05;
        // Deterministic accepted-sample disturbance, not an ADC/guard replay.
        let noise = if (now_ms / 250).is_multiple_of(2) {
            self.noise_c
        } else {
            -self.noise_c
        };
        self.input.measured_temp_c = self.plate_c + noise;
        self.input.now_ms = now_ms;
        let heating = self.tuning.phase != tuning::ThermalTuningPhase::Cooldown;
        if heating && !self.input.heater_enabled {
            self.peak_c = self.input.measured_temp_c;
        }
        self.input.heater_enabled = heating;
        if heating {
            self.peak_c = self.peak_c.max(self.input.measured_temp_c);
        }
        let profile = ThermalControlProfile::from_saved_config(
            self.tuning.candidate_profile.as_ref().unwrap(),
        );
        let output = self
            .controller
            .update_production_at(
                ProductionThermalController::Profile(ThermalProfileBank::Pps3a),
                self.input,
                profile,
            )
            .0;
        let duty = if output.phase == HeaterControlPhase::Warmup {
            output.duty_percent.min(output.warmup_soft_start_percent)
        } else {
            output.duty_percent
        };
        let power = f32::from(duty) * self.input.max_power_mw / 100.0;
        self.delayed_power.push_back((now_ms, power));
        self.controller.applied_heat.observe(now_ms, power);
        self.record(now_ms, output.phase, duty);
    }

    fn record(&mut self, now_ms: u64, phase: HeaterControlPhase, duty: u8) {
        let phase = if self.tuning.phase == tuning::ThermalTuningPhase::Cooldown {
            tuning::ThermalTuningPhase::Cooldown
        } else {
            match phase {
                HeaterControlPhase::Warmup => tuning::ThermalTuningPhase::Warmup,
                HeaterControlPhase::Approach => tuning::ThermalTuningPhase::Approach,
                HeaterControlPhase::Hold => tuning::ThermalTuningPhase::Hold,
            }
        };
        self.tuning
            .record_live_sample(tuning::ThermalTuningSample {
                sequence: 0,
                elapsed_ms: now_ms,
                target_c: 60,
                trial_index: self.tuning.trial_index,
                candidate_id: self.tuning.candidate_id.clone().unwrap(),
                candidate_hash: self.tuning.candidate_hash.clone().unwrap(),
                temperature_centi_c: (self.input.measured_temp_c * 100.0).round() as i16,
                raw_temperature_centi_c: Some((self.input.measured_temp_c * 100.0).round() as i16),
                vin_mv: 20_900,
                pps_contract: tuning::ThermalTuningPpsContract {
                    voltage_mv: 21_000,
                    current_ma: 3_000,
                    kind: tuning::ThermalTuningPowerClass::Pps3a,
                    confirmed: true,
                },
                requested_mv: 21_000,
                pps_power_ceiling_mv: 21_100,
                heater_requested_output_permille: u16::from(duty) * 10,
                heater_output_permille: u16::from(duty) * 10,
                measurement_valid: true,
                measurement_accepted: Some(true),
                phase,
                fan: None,
            })
            .unwrap();
        let page = self.tuning.events_after(self.cursor);
        for event in &page.events {
            let Some(decision) = &event.decision else {
                continue;
            };
            if decision.disposition == "passed" {
                assert!(self.peak_c <= 63.0, "qualified trial peak={}", self.peak_c);
                assert!(decision.warmup_full);
                assert!(decision.approach_to_hold);
                assert!(
                    decision
                        .max_overshoot_centi_c
                        .is_some_and(|peak| peak <= 300)
                );
                assert!(
                    decision
                        .hold_peak_to_peak_centi_c
                        .is_some_and(|span| span <= 300)
                );
            }
            self.decisions.push((
                event.trial_index.unwrap_or_default(),
                event.reason.as_deref().unwrap_or_default().into(),
            ));
        }
        self.cursor = page.next_sequence.or(self.cursor);
    }
}
