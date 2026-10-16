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
fn exact_response_observations_do_not_bias_instantaneous_heat() {
    let mut plant = plant(850);
    plant.model.convection_mw_per_c = 0.0;
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure(850);
    history.configure_response(Some(response));
    let mut temperature_c = 35.0;
    let mut received_mw = 0.0;
    history.observe(0, plant.max_power_mw);
    history.observe_temperature(0, temperature_c, plant);
    for now_ms in (50..=12_000).step_by(50) {
        let input_mw = if now_ms <= 6_000 {
            plant.max_power_mw
        } else {
            0.05 * plant.max_power_mw
        };
        history.observe(now_ms - 50, input_mw);
        temperature_c +=
            response.step(&mut received_mw, input_mw, 0.05) / plant.model.thermal_capacity_mj_per_c;
        history.observe_temperature(now_ms, temperature_c, plant);
        history.observe(now_ms, input_mw);
        if now_ms % 1_000 == 0 {
            assert!(
                (history.received_mw - received_mw).abs() < 100.0,
                "at {now_ms}: model={}, actual={received_mw}",
                history.received_mw,
            );
        }
    }
}

#[test]
fn a_positive_transfer_discrepancy_cannot_supply_heat_after_its_tail() {
    let plant = plant(0);
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, 0.0);
    // An accepted rise can reveal heat not represented by the input
    // history. That is finite stored heat, not an independent heater.
    let mut actual_heat_mw = 30_000.0;
    let mut temperature_c = 50.0;
    history.observe_temperature(0, temperature_c, plant);
    for now_ms in (50..=2_000).step_by(50) {
        let energy = response.step(&mut actual_heat_mw, 0.0, 0.05);
        temperature_c +=
            (energy - plant.loss_mw(temperature_c) * 0.05) / plant.model.thermal_capacity_mj_per_c;
        history.observe_temperature(now_ms, temperature_c, plant);
        history.observe(now_ms, 0.0);
    }
    let (predicted, _) = history
        .predict_response(
            2_000,
            2_000,
            temperature_c,
            0.0,
            response.horizon_ms(),
            plant,
        )
        .unwrap();
    assert!(
        predicted.rise_c_per_s < 0.0,
        "zero input must cool after the finite heat tail: {predicted:?}",
    );
}

#[test]
fn tracking_cannot_hide_a_near_term_peak_behind_a_cooler_endpoint() {
    let mut plant = plant(0);
    plant.model.thermal_capacity_mj_per_c = 25_843.268;
    plant.max_power_mw = 105_000.0;
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, 0.0);
    history.received_mw = 20_000.0;
    let request = history
        .tracking_request_permille(
            0,
            0,
            59.0,
            TrackingTarget {
                temperature_c: 60.0,
                minimum_permille: 0,
                ahead_ms: response.horizon_ms(),
                command_ms: response.horizon_ms(),
            },
            plant,
        )
        .unwrap();
    let (_, peak) = history
        .predict_response(
            0,
            0,
            59.0,
            plant.max_power_mw * f32::from(request) / 1_000.0,
            response.horizon_ms(),
            plant,
        )
        .unwrap();
    assert!(peak <= 60.05, "request={request} predicted peak={peak}");
}

#[test]
fn peak_envelope_preserves_stationary_loss_compensation() {
    let plant = plant(0);
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, plant.loss_mw(60.0));
    history.received_mw = plant.loss_mw(60.0);
    let request = history
        .tracking_request_permille(
            0,
            0,
            60.0,
            TrackingTarget {
                temperature_c: 60.0,
                minimum_permille: 0,
                ahead_ms: response.horizon_ms(),
                command_ms: response.horizon_ms(),
            },
            plant,
        )
        .unwrap();
    assert!((95..=105).contains(&request), "{request}");
}

#[test]
fn hold_feedback_has_room_below_a_separate_complete_peak_limit() {
    let plant = plant(0);
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, plant.loss_mw(60.0));
    history.received_mw = plant.loss_mw(60.0);
    let (request, maximum) = history
        .tracking_request_bounds(
            0,
            0,
            60.0,
            TrackingTarget {
                temperature_c: 60.0,
                minimum_permille: 0,
                ahead_ms: response.horizon_ms(),
                command_ms: response.horizon_ms(),
            },
            plant,
            0.5,
        )
        .unwrap();
    assert!(maximum > request, "request={request}, maximum={maximum}");
    let (endpoint, peak) = history
        .predict_response(
            0,
            0,
            60.0,
            plant.max_power_mw * f32::from(maximum) / 1_000.0,
            response.horizon_ms(),
            plant,
        )
        .unwrap();
    assert!(endpoint.temperature_c > 60.1, "{endpoint:?}");
    assert!(peak <= 60.51, "{peak}");
}

#[test]
fn finite_hold_correction_checks_the_complete_post_command_tail() {
    let plant = plant(0);
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    let initial_heat = plant.loss_mw(59.0);
    history.observe(0, initial_heat);
    history.received_mw = initial_heat;
    let target = TrackingTarget {
        temperature_c: 60.0,
        minimum_permille: 0,
        ahead_ms: response.horizon_ms(),
        command_ms: u64::from(response.decay_ms),
    };
    let (request, ceiling) = history
        .tracking_request_bounds(0, 0, 59.0, target, plant, 0.5)
        .unwrap();
    let (_, constant_ceiling) = history
        .tracking_request_bounds(
            0,
            0,
            59.0,
            TrackingTarget {
                command_ms: target.ahead_ms,
                ..target
            },
            plant,
            0.5,
        )
        .unwrap();
    assert!(ceiling > constant_ceiling && request <= ceiling);
    let mut temperature = 59.0;
    let mut heat = initial_heat;
    let mut peak = temperature;
    for at_ms in (0..target.ahead_ms).step_by(50) {
        let input = if at_ms < target.command_ms {
            plant.max_power_mw * f32::from(ceiling) / 1_000.0
        } else {
            plant.loss_mw(target.temperature_c)
        };
        let energy = response.step(&mut heat, input, 0.05);
        temperature +=
            (energy - plant.loss_mw(temperature) * 0.05) / plant.model.thermal_capacity_mj_per_c;
        peak = peak.max(temperature);
    }
    assert!(peak <= 60.51, "{peak}");
}

#[test]
fn falling_hold_recovers_heat_before_crossing_the_target() {
    let mut plant = plant(0);
    plant.max_power_mw = 78_000.0;
    plant.model.thermal_capacity_mj_per_c = 25_843.268;
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_950,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, 0.0);
    history.received_mw = 20_000.0;
    let request = history
        .tracking_request_permille(
            0,
            0,
            220.5,
            TrackingTarget {
                temperature_c: 220.0,
                minimum_permille: 0,
                ahead_ms: response.horizon_ms(),
                command_ms: response.horizon_ms(),
            },
            plant,
        )
        .unwrap();
    let (endpoint, peak) = history
        .predict_response(
            0,
            0,
            220.5,
            plant.max_power_mw * f32::from(request) / 1_000.0,
            response.horizon_ms(),
            plant,
        )
        .unwrap();
    assert!(request > 400, "{request}");
    assert!(
        (endpoint.temperature_c - 220.0).abs() < 0.05,
        "{endpoint:?}"
    );
    assert!(peak <= 220.51, "{peak}");
}

#[test]
fn lower_future_source_capacity_cannot_erase_heat_already_in_flight() {
    let mut plant = plant(0);
    plant.max_power_mw = 100_000.0;
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 4_000,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, 100_000.0);
    history.observe(10_000, 100_000.0);
    let original = history
        .predict_response(10_000, 9_000, 50.0, 0.0, 0, plant)
        .unwrap();
    plant.max_power_mw = 40_000.0;
    let reduced = history
        .predict_response(10_000, 9_000, 50.0, 0.0, 0, plant)
        .unwrap();
    assert!((original.0.temperature_c - reduced.0.temperature_c).abs() < 0.001);
    assert!((original.0.rise_c_per_s - reduced.0.rise_c_per_s).abs() < 0.001);
}

#[test]
fn warmup_prediction_must_cover_the_shutdown_tail_beyond_pure_delay() {
    let plant = plant(850);
    let mut history = AppliedHeatHistory::new();
    history.configure(850);
    let response = HeatResponse {
        delay_ms: 850,
        decay_ms: 4_000,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    history.configure_response(Some(response));
    history.observe(0, 60_000.0);
    history.observe(10_000, 3_000.0);
    // A four-second transfer tail contains heat after the 850ms transport
    // interval. This is a model regression, not a historical tuning seed.
    let (_, peak_c) = history
        .predict_response(10_000, 10_000, 49.0, 3_000.0, response.horizon_ms(), plant)
        .unwrap();
    assert!(peak_c > 55.0, "{peak_c}");
    history.observe(11_000, 3_000.0);
    let before = history
        .predict_response(11_000, 11_000, 52.0, 3_000.0, response.horizon_ms(), plant)
        .unwrap();
    history.reanchor_temperature();
    let after = history
        .predict_response(11_000, 11_000, 52.0, 3_000.0, response.horizon_ms(), plant)
        .unwrap();
    assert_eq!(before, after);
    assert!(
        after.1 > 55.0,
        "tail must survive transport expiry and PPS reanchor"
    );
}

#[test]
fn forecast_advances_heat_to_a_newer_measurement_before_predicting() {
    let plant = plant(0);
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 4_000,
        rmse_c: 0.0,
        maximum_error_c: 0.0,
    };
    let mut history = AppliedHeatHistory::new();
    history.configure_response(Some(response));
    history.observe(0, 60_000.0);
    let before = history
        .predict_response(1_000, 1_000, 40.0, 3_000.0, 8_000, plant)
        .unwrap();
    history.observe(1_000, 60_000.0);
    let after = history
        .predict_response(1_000, 1_000, 40.0, 3_000.0, 8_000, plant)
        .unwrap();
    assert!((before.0.temperature_c - after.0.temperature_c).abs() < 0.001);
    assert!((before.1 - after.1).abs() < 0.001);
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
        let delivered_mw = if now_ms <= 2_700 { 60_000.0 } else { 3_000.0 };
        if now_ms > 0 {
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
