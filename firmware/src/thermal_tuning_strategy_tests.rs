#[test]
fn saturated_trajectory_recovery_requires_observed_low_side_hold() {
    let previous = ThermalControlProfilePointConfig {
        hold_power_permille: 800,
        hold_reheat_power_permille: 800,
        hold_lead_ticks: 1,
        ..point(220)
    };
    let facts = TuneEvidence {
        hold_sample_count: 23,
        hold_mean_error_c: Some(0.8),
        hold_max_below_c: Some(1.62),
        hold_median_output_permille: Some(1_000),
        hold_p90_output_permille: Some(1_000),
        approach_median_output_permille: Some(1_000),
        approach_output_range: Some((50, 1_000)),
        first_hold_after_warmup_ms: Some(1_218),
        settle_limit_ms: 5_000,
        first_band_observed: true,
        first_band_exit_centi_c: Some(21_838),
        ..evidence(220)
    };
    let proposed = saturated_trajectory_hold_deficit_candidate(previous, facts).unwrap();
    assert_eq!(proposed.hold_power_permille, 900);
    assert_eq!(proposed.hold_reheat_power_permille, 900);
    assert_eq!(reseed_after_hold_confirm(previous, facts), proposed);
    for rejected in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 0,
            ..facts
        },
        TuneEvidence {
            hold_mean_error_c: None,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(5_001),
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 1_000)),
            ..facts
        },
        TuneEvidence {
            hold_median_output_permille: Some(100),
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(22_151),
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
    ] {
        assert!(saturated_trajectory_hold_deficit_candidate(previous, rejected).is_none());
    }
    let saturated = ThermalControlProfilePointConfig {
        hold_power_permille: 1_000,
        hold_reheat_power_permille: 1_000,
        hold_lead_ticks: 0,
        hold_blend_ticks: 1,
        ..previous
    };
    assert!(saturated_trajectory_hold_deficit_candidate(saturated, facts).is_none());
}

#[test]
fn model_floor_headroom_requires_complete_timely_low_side_hold_evidence() {
    let previous = point(240);
    let facts = TuneEvidence {
        overshoot_c: 0.16,
        hold_p2p_c: 1.86,
        hold_sample_count: 26,
        hold_off_sample_count: 3,
        hold_median_output_permille: Some(833),
        hold_p90_output_permille: Some(1000),
        approach_median_output_permille: Some(50),
        approach_floor_only: true,
        first_hold_after_warmup_ms: Some(713),
        first_band_observed: true,
        first_band_exit_centi_c: Some(23830),
        settle_limit_ms: 5000,
        ..evidence(240)
    };
    assert!(hold_reheat_headroom_candidate(previous, facts).is_some());
    for unavailable in [
        TuneEvidence {
            approach_floor_only: false,
            ..facts
        },
        TuneEvidence {
            approach_median_output_permille: None,
            ..facts
        },
        TuneEvidence {
            approach_median_output_permille: Some(100),
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(5001),
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: None,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(24160),
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: None,
            ..facts
        },
        TuneEvidence {
            first_band_observed: false,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 2,
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 2,
            ..facts
        },
        TuneEvidence {
            overshoot_c: 1.51,
            ..facts
        },
        TuneEvidence {
            hold_p2p_c: 3.01,
            ..facts
        },
        TuneEvidence {
            hold_median_output_permille: Some(950),
            ..facts
        },
        TuneEvidence {
            hold_p90_output_permille: None,
            ..facts
        },
    ] {
        assert!(hold_reheat_headroom_candidate(previous, unavailable).is_none());
    }
    assert_eq!(
        tune_point(
            previous,
            TuneEvidence {
                tunable_stop: false,
                ..facts
            }
        ),
        previous
    );
}

#[test]
fn saturated_pi_peak_does_not_hide_configured_reheat_or_timing_headroom() {
    // Captured 240C scout: physical Hold starts after 2343ms, then
    // coasting and delayed recovery break the band at 238.42C.
    let mut previous = point(240);
    previous.brake_distance_centi_c = 150;
    previous.warmup_reenter_centi_c = 1000;
    previous.approach_floor_power_permille = 1000;
    previous.approach_power_permille = 1000;
    previous.approach_damping_exponent_permille = 100;
    previous.approach_tail_window_centi_c = 0;
    previous.hold_power_permille = 900;
    previous.hold_reheat_power_permille = 980;
    previous.hold_on_centi_c = 5;
    previous.hold_off_centi_c = 80;
    previous.hold_kp_permille_per_c = 15;
    previous.hold_entry_centi_c = 150;
    previous.hold_exit_centi_c = 160;
    previous.hold_ki_permille_per_c_tick = 1;
    previous.hold_blend_ticks = 1;
    previous.overshoot_cutoff_centi_c = 180;
    previous.hold_lead_ticks = 4;
    let facts = TuneEvidence {
        overshoot_c: 0.06,
        hold_p2p_c: 1.94,
        residual_c: 0.5,
        under_c: 1.72,
        over_c: 0.06,
        hold_sample_count: 11,
        hold_off_sample_count: 4,
        first_hold_error_c: Some(0.44),
        hold_median_output_permille: Some(810),
        hold_p90_output_permille: Some(1000),
        approach_median_output_permille: Some(1000),
        first_hold_after_warmup_ms: Some(2343),
        first_band_observed: true,
        first_band_exit_centi_c: Some(23842),
        settle_limit_ms: 5000,
        curve_class: CurveClass::OnCurve,
        ..evidence(240)
    };
    let mut expected = previous;
    expected.hold_reheat_power_permille = 1000;
    expected.hold_lead_ticks = 3;
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    let mut earlier = expected;
    earlier.hold_lead_ticks = 2;
    assert_eq!(tune_point(expected, facts), earlier);
    expected.hold_lead_ticks = 0;
    assert!(hold_reheat_headroom_candidate(expected, facts).is_none());
    assert!(
        hold_reheat_headroom_candidate(
            previous,
            TuneEvidence {
                hold_p90_output_permille: None,
                ..facts
            }
        )
        .is_none()
    );
    assert_eq!(
        tune_point(
            previous,
            TuneEvidence {
                tunable_stop: false,
                ..facts
            }
        ),
        previous
    );
}

#[test]
fn low_band_exit_after_saturated_approach_restores_reheat_headroom() {
    // Captured 240C scout after the 1000 -> 900 Hold trim. Approach
    // remains full, but Hold loses its window at 238.46C after coasting.
    let mut previous = point(240);
    previous.brake_distance_centi_c = 150;
    previous.warmup_reenter_centi_c = 1000;
    previous.approach_floor_power_permille = 1000;
    previous.approach_power_permille = 1000;
    previous.approach_damping_exponent_permille = 100;
    previous.approach_tail_window_centi_c = 0;
    previous.hold_power_permille = 900;
    previous.hold_reheat_power_permille = 900;
    previous.hold_on_centi_c = 6;
    previous.hold_off_centi_c = 80;
    previous.hold_kp_permille_per_c = 15;
    previous.hold_entry_centi_c = 150;
    previous.hold_exit_centi_c = 160;
    previous.hold_ki_permille_per_c_tick = 1;
    previous.hold_blend_ticks = 1;
    previous.overshoot_cutoff_centi_c = 180;
    previous.hold_lead_ticks = 5;
    let facts = TuneEvidence {
        hold_p2p_c: 1.52,
        residual_c: 0.32,
        under_c: 1.54,
        hold_sample_count: 22,
        hold_off_sample_count: 4,
        first_hold_error_c: Some(0.34),
        hold_median_output_permille: Some(910),
        hold_p90_output_permille: Some(920),
        approach_median_output_permille: Some(1000),
        first_hold_after_warmup_ms: Some(1073),
        first_band_observed: true,
        first_band_exit_centi_c: Some(23846),
        settle_limit_ms: 5000,
        curve_class: CurveClass::OnCurve,
        ..evidence(240)
    };
    let next = tune_point(previous, facts);
    assert!(next.hold_reheat_power_permille > previous.hold_reheat_power_permille);
    assert_eq!(next.hold_power_permille, previous.hold_power_permille);
    assert_eq!(next.approach_floor_power_permille, 1000);
    assert_eq!(next.brake_distance_centi_c, 150);
    assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
    assert!(next.hold_lead_ticks < previous.hold_lead_ticks);
    assert_eq!(next.hold_reheat_power_permille, 980);
    assert_eq!(reseed_after_hold_confirm(previous, facts), next);
    assert_eq!(
        tune_point(
            previous,
            TuneEvidence {
                curve_class: CurveClass::Underpowered,
                ..facts
            }
        ),
        next
    );
    for unavailable in [
        TuneEvidence {
            first_band_observed: false,
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 0,
            ..facts
        },
        TuneEvidence {
            overshoot_c: 3.1,
            ..facts
        },
        TuneEvidence {
            hold_p90_output_permille: None,
            ..facts
        },
        TuneEvidence {
            first_hold_after_warmup_ms: Some(6000),
            ..facts
        },
    ] {
        assert!(hold_reheat_headroom_candidate(previous, unavailable).is_none());
    }
    assert_eq!(
        tune_point(
            previous,
            TuneEvidence {
                tunable_stop: false,
                ..facts
            }
        ),
        previous
    );
}

#[test]
fn saturated_hold_confirmation_uses_distinct_ripple_recovery() {
    // Captured 240C confirmation: the approach established a window in
    // 792ms; Hold alternated full output and cutoff, then broke low.
    let mut previous = point(240);
    previous.brake_distance_centi_c = 150;
    previous.warmup_reenter_centi_c = 1000;
    previous.approach_floor_power_permille = 1000;
    previous.approach_power_permille = 1000;
    previous.approach_damping_exponent_permille = 100;
    previous.approach_tail_window_centi_c = 0;
    previous.hold_power_permille = 1000;
    previous.hold_reheat_power_permille = 1000;
    previous.hold_on_centi_c = 11;
    previous.hold_off_centi_c = 80;
    previous.hold_kp_permille_per_c = 15;
    previous.hold_entry_centi_c = 150;
    previous.hold_exit_centi_c = 160;
    previous.hold_ki_permille_per_c_tick = 1;
    previous.hold_blend_ticks = 1;
    previous.overshoot_cutoff_centi_c = 180;
    previous.hold_lead_ticks = 6;
    let facts = TuneEvidence {
        overshoot_c: 1.26,
        hold_p2p_c: 3.16,
        under_c: 1.90,
        over_c: 1.26,
        hold_sample_count: 268,
        hold_off_sample_count: 51,
        hold_median_output_permille: Some(1000),
        hold_p90_output_permille: Some(1000),
        first_band_observed: true,
        first_band_exit_centi_c: Some(23808),
        stable_window_after_warmup_ms: Some(792),
        stable_window_verified: true,
        scout_hold_complete: true,
        full_speed_failed: false,
        ..evidence(240)
    };
    let next = reseed_after_hold_confirm(previous, facts);
    assert_ne!(next, previous);
    assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
    assert_eq!(
        next.approach_power_permille,
        previous.approach_power_permille
    );
    assert_eq!(next.approach_floor_power_permille, 1000);
    assert!(next.hold_power_permille < previous.hold_power_permille);
    assert!(next.hold_lead_ticks < previous.hold_lead_ticks);
    assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
    assert_eq!(tune_point(previous, facts), next);
    for unavailable in [
        TuneEvidence {
            full_speed_failed: true,
            ..facts
        },
        TuneEvidence {
            overshoot_c: 0.0,
            ..facts
        },
        TuneEvidence {
            hold_p90_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 0,
            ..facts
        },
    ] {
        assert!(saturated_hold_ripple_candidate(previous, unavailable).is_none());
    }
    assert_eq!(
        reseed_after_hold_confirm(
            previous,
            TuneEvidence {
                tunable_stop: false,
                ..facts
            }
        ),
        previous
    );
}

#[test]
fn saturated_240c_low_hold_exit_changes_recovery_not_approach() {
    let mut previous = point(240);
    previous.brake_distance_centi_c = 150;
    previous.approach_floor_power_permille = 1000;
    previous.approach_power_permille = 1000;
    previous.hold_power_permille = 1000;
    previous.hold_reheat_power_permille = 1000;
    previous.hold_on_centi_c = 11;
    previous.hold_kp_permille_per_c = 15;
    let facts = TuneEvidence {
        first_band_observed: true,
        first_band_exit_centi_c: Some(23827),
        hold_sample_count: 20,
        hold_median_output_permille: Some(0),
        overshoot_c: 0.73,
        under_c: 1.73,
        curve_class: CurveClass::Underpowered,
        ..evidence(240)
    };
    let next = tune_point(previous, facts);
    assert_ne!(next, previous);
    assert_eq!(
        next.approach_power_permille,
        previous.approach_power_permille
    );
    assert_eq!(next.brake_distance_centi_c, previous.brake_distance_centi_c);
    assert!(next.hold_on_centi_c < previous.hold_on_centi_c);
    assert!(next.hold_kp_permille_per_c > previous.hold_kp_permille_per_c);
}
use super::*;

fn point(target_c: i16) -> ThermalControlProfilePointConfig {
    ThermalControlProfilePointConfig {
        target_temp_c: target_c,
        brake_distance_centi_c: 300,
        warmup_power_permille: 1_000,
        warmup_reenter_centi_c: 400,
        approach_power_permille: 750,
        approach_floor_power_permille: 400,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 200,
        hold_power_permille: 350,
        hold_reheat_power_permille: 450,
        hold_entry_centi_c: 90,
        hold_exit_centi_c: 200,
        hold_on_centi_c: 30,
        hold_off_centi_c: 50,
        overshoot_cutoff_centi_c: 150,
        hold_kp_permille_per_c: 120,
        hold_ki_permille_per_c_tick: 12,
        hold_blend_ticks: 12,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    }
}

#[test]
fn missing_high_temperature_hold_does_not_invent_hold_feedback() {
    let mut previous = point(240);
    previous.brake_distance_centi_c = 150;
    previous.approach_damping_exponent_permille = 100;
    previous.hold_power_permille = 838;
    previous.hold_reheat_power_permille = 838;
    previous.hold_kp_permille_per_c = 30;
    previous.hold_ki_permille_per_c_tick = 1;
    let facts = TuneEvidence {
        approach_median_output_permille: Some(1_000),
        approach_median_slope_c_per_s: Some(0.2),
        curve_class: CurveClass::Underpowered,
        settle_limit_ms: 5_000,
        deadline_temperature_centi_c: Some(23_700),
        ..evidence(240)
    };
    for next in [
        tune_point(previous, facts),
        reseed_after_hold_confirm(previous, facts),
    ] {
        assert_eq!(next.hold_power_permille, previous.hold_power_permille);
        assert_eq!(
            next.hold_reheat_power_permille,
            previous.hold_reheat_power_permille
        );
        assert_eq!(next.hold_kp_permille_per_c, previous.hold_kp_permille_per_c);
        assert_eq!(next.hold_on_centi_c, previous.hold_on_centi_c);
        assert_eq!(next.hold_off_centi_c, previous.hold_off_centi_c);
        assert_eq!(next.hold_entry_centi_c, previous.hold_entry_centi_c);
        assert_eq!(next.hold_exit_centi_c, previous.hold_exit_centi_c);
        assert_eq!(
            next.overshoot_cutoff_centi_c,
            previous.overshoot_cutoff_centi_c
        );
    }
}

#[test]
fn coasting_failure_alternatives_change_damping_before_unused_hold_feedback() {
    let previous = point(60);
    let facts = TuneEvidence {
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_175),
        hold_sample_count: 14,
        hold_off_sample_count: 14,
        ..evidence(60)
    };
    let first = bounded_failure_alternative(previous, facts, 0).unwrap();
    assert_eq!(first.approach_damping_exponent_permille, 1_100);
    let expected = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 1_100,
        ..previous
    };
    assert_eq!(first, expected);
    for attempt in 0..32 {
        let next = bounded_failure_alternative(previous, facts, attempt).unwrap();
        assert_eq!(next.hold_power_permille, previous.hold_power_permille);
        assert_eq!(
            next.hold_reheat_power_permille,
            previous.hold_reheat_power_permille
        );
        assert_eq!(next.hold_kp_permille_per_c, previous.hold_kp_permille_per_c);
        assert!(next.brake_distance_centi_c >= previous.brake_distance_centi_c);
        assert!(
            next.approach_damping_exponent_permille >= previous.approach_damping_exponent_permille
        );
    }
}

#[test]
fn distinct_failure_alternatives_preserve_direction_and_missing_hold() {
    let previous = point(60);
    for high in [true, false] {
        let facts = TuneEvidence {
            first_band_observed: true,
            first_band_exit_centi_c: Some(if high { 6_160 } else { 5_840 }),
            hold_sample_count: 0,
            overshoot_c: 0.0,
            ..evidence(60)
        };
        for attempt in 0..32 {
            let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
            assert_eq!(
                alternative.hold_power_permille,
                previous.hold_power_permille
            );
            assert_eq!(
                alternative.hold_reheat_power_permille,
                previous.hold_reheat_power_permille
            );
            if high {
                assert!(alternative.brake_distance_centi_c >= previous.brake_distance_centi_c);
                assert!(
                    alternative.approach_damping_exponent_permille
                        >= previous.approach_damping_exponent_permille
                );
            } else {
                assert!(alternative.brake_distance_centi_c <= previous.brake_distance_centi_c);
                assert!(
                    alternative.approach_damping_exponent_permille
                        <= previous.approach_damping_exponent_permille
                );
            }
        }
        assert!(
            bounded_failure_alternative(
                previous,
                TuneEvidence {
                    tunable_stop: false,
                    ..facts
                },
                0
            )
            .is_none()
        );
        assert!(bounded_failure_alternative(previous, facts, 32).is_none());
    }
}

#[test]
fn saturated_failure_alternatives_do_not_reverse_or_fabricate_progress() {
    let mut previous = point(60);
    previous.brake_distance_centi_c = 5_000;
    previous.approach_damping_exponent_permille = 4_000;
    previous.approach_lead_ticks = 12;
    previous.hold_power_permille = 40;
    previous.hold_kp_permille_per_c = 4;
    let facts = TuneEvidence {
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_160),
        hold_sample_count: 3,
        overshoot_c: 0.0,
        ..evidence(60)
    };
    for attempt in 0..32 {
        assert!(bounded_failure_alternative(previous, facts, attempt).is_none());
    }
}

fn evidence(target_c: i16) -> TuneEvidence {
    TuneEvidence {
        target_c,
        tunable_stop: true,
        full_speed_failed: true,
        hold_pps_wait_interruptions: 0,
        guarded_hold_handoff: false,
        overshoot_c: 0.0,
        hold_p2p_c: 0.0,
        residual_c: 0.0,
        under_c: 0.0,
        over_c: 0.0,
        hold_sample_count: 0,
        hold_off_sample_count: 0,
        hold_entry_output_permille: None,
        hold_entry_pulse_only: false,
        hold_threshold_crossed: false,
        hold_mean_error_c: None,
        hold_max_below_c: None,
        first_hold_error_c: None,
        hold_median_output_permille: None,
        hold_p90_output_permille: None,
        hold_max_output_permille: None,
        approach_median_output_permille: None,
        approach_floor_only: false,
        approach_output_range: None,
        approach_median_slope_c_per_s: None,
        curve_class: CurveClass::Insufficient,
        first_hold_after_warmup_ms: None,
        stable_window_after_warmup_ms: None,
        settle_limit_ms: 10_000,
        scout_hold_complete: false,
        stable_window_verified: false,
        first_band_observed: false,
        first_band_exit_centi_c: None,
        deadline_temperature_centi_c: None,
    }
}

#[test]
fn unverified_timely_restart_keeps_the_broken_high_side_failure() {
    let mut facts = evidence(80);
    facts.scout_hold_complete = true;
    facts.stable_window_after_warmup_ms = Some(18_252);
    facts.settle_limit_ms = 30_000;
    facts.first_band_observed = true;
    facts.first_band_exit_centi_c = Some(8_155);
    assert!(stability_gate_evidence(facts) == (GateFailure::BrokeHigh, 0.05));
    facts.stable_window_verified = true;
    assert!(stability_gate_evidence(facts) == (GateFailure::Other, 0.0));
}

#[test]
fn severe_residual_matches_main_strategy_vector() {
    let mut previous = point(60);
    previous.brake_distance_centi_c = 1_210;
    previous.approach_damping_exponent_permille = 1_040;
    previous.approach_power_permille = 520;
    previous.approach_lead_ticks = 7;
    previous.hold_power_permille = 90;
    previous.hold_reheat_power_permille = 340;
    let mut facts = evidence(60);
    facts.overshoot_c = 7.7;
    facts.hold_p2p_c = 6.8;
    facts.residual_c = 6.8;
    facts.over_c = 7.7;
    facts.hold_sample_count = 19;
    facts.first_hold_error_c = Some(-0.9);
    facts.hold_median_output_permille = Some(0);
    facts.hold_mean_error_c = Some(-4.61);
    facts.hold_max_below_c = Some(0.0);
    let tuned = tune_point(previous, facts);
    assert_eq!(tuned.brake_distance_centi_c, 1_560);
    assert_eq!(tuned.approach_damping_exponent_permille, 1_240);
    assert_eq!(tuned.approach_lead_ticks, 9);
    assert_eq!(tuned.approach_floor_power_permille, 400);
    assert_eq!(tuned.hold_power_permille, 90);
    assert_eq!(tuned.hold_reheat_power_permille, 340);
}

#[test]
fn observed_60c_high_side_failure_must_not_accelerate_next_candidate() {
    let mut previous = point(60);
    previous.brake_distance_centi_c = 1_100;
    previous.approach_lead_ticks = 6;
    previous.approach_power_permille = 450;
    previous.approach_floor_power_permille = 160;
    previous.approach_damping_exponent_permille = 4_000;
    previous.hold_power_permille = 135;
    previous.hold_reheat_power_permille = 170;
    previous.hold_entry_centi_c = 220;
    previous.hold_exit_centi_c = 400;
    previous.hold_lead_ticks = 6;
    previous.hold_blend_ticks = 1;

    let mut facts = evidence(60);
    facts.curve_class = CurveClass::Underpowered;
    facts.overshoot_c = 3.19;
    facts.hold_p2p_c = 5.22;
    facts.residual_c = 5.22;
    facts.under_c = 2.03;
    facts.over_c = 3.19;
    facts.hold_sample_count = 20;
    facts.first_hold_error_c = Some(2.03);
    facts.hold_median_output_permille = Some(0);
    facts.hold_p90_output_permille = Some(0);
    facts.first_band_observed = true;
    facts.first_band_exit_centi_c = Some(6_154);

    let tuned = tune_point(previous, facts);
    assert_eq!(tuned.brake_distance_centi_c, 1_400);
    assert_eq!(tuned.approach_lead_ticks, 6);
    assert_eq!(tuned.approach_power_permille, 350);
    assert_eq!(tuned.approach_floor_power_permille, 100);
}

#[test]
fn observed_60c_lower_band_miss_accelerates_next_candidate() {
    let mut previous = point(60);
    previous.brake_distance_centi_c = 1_400;
    previous.approach_power_permille = 350;
    previous.approach_floor_power_permille = 100;
    previous.approach_damping_exponent_permille = 4_000;
    previous.hold_power_permille = 100;
    previous.hold_reheat_power_permille = 120;
    previous.hold_entry_centi_c = 320;
    previous.hold_exit_centi_c = 300;
    previous.approach_lead_ticks = 6;
    previous.hold_lead_ticks = 6;
    let mut facts = evidence(60);
    facts.curve_class = CurveClass::Underpowered;
    facts.overshoot_c = 1.05;
    facts.hold_p2p_c = 1.50;
    facts.residual_c = 3.58;
    facts.under_c = 3.17;
    facts.over_c = 0.84;
    facts.hold_sample_count = 40;
    facts.first_hold_error_c = Some(3.17);
    facts.first_band_observed = false;
    facts.deadline_temperature_centi_c = Some(5_792);

    let tuned = tune_point(previous, facts);

    assert!(tuned.brake_distance_centi_c < previous.brake_distance_centi_c);
    assert!(tuned.approach_power_permille > previous.approach_power_permille);
    assert!(tuned.approach_floor_power_permille > previous.approach_floor_power_permille);
}

fn high_temperature_point(target_c: i16) -> ThermalControlProfilePointConfig {
    ThermalControlProfilePointConfig {
        brake_distance_centi_c: 839,
        approach_power_permille: 667,
        approach_floor_power_permille: 600,
        approach_damping_exponent_permille: 1_834,
        approach_tail_window_centi_c: 167,
        approach_lead_ticks: 6,
        hold_power_permille: 600,
        hold_reheat_power_permille: 600,
        hold_entry_centi_c: 270,
        hold_exit_centi_c: 258,
        hold_on_centi_c: 20,
        hold_off_centi_c: 67,
        overshoot_cutoff_centi_c: 140,
        hold_kp_permille_per_c: 12,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 1,
        hold_lead_ticks: 6,
        ..point(target_c)
    }
}

fn high_temperature_failure(target_c: i16) -> TuneEvidence {
    TuneEvidence {
        // A slow Approach classification must not override measured high-side failure.
        curve_class: CurveClass::Underpowered,
        overshoot_c: 2.63,
        hold_p2p_c: 3.73,
        residual_c: 3.73,
        first_band_observed: true,
        first_band_exit_centi_c: Some(target_c * 100 + 151),
        settle_limit_ms: 15_000,
        ..evidence(target_c)
    }
}

#[test]
fn floor_only_coast_alternatives_use_remaining_braking_without_fake_progress() {
    let previous = ThermalControlProfilePointConfig {
        brake_distance_centi_c: 1_318,
        approach_damping_exponent_permille: 1_050,
        ..point(60)
    };
    let facts = TuneEvidence {
        overshoot_c: 1.7,
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_151),
        hold_sample_count: 15,
        hold_off_sample_count: 15,
        approach_floor_only: true,
        ..evidence(60)
    };
    for attempt in 0..32 {
        let alternative = bounded_failure_alternative(previous, facts, attempt).unwrap();
        assert_eq!(
            alternative,
            ThermalControlProfilePointConfig {
                brake_distance_centi_c: 1_318 + 40 * (attempt + 1),
                ..previous
            }
        );
    }
    let saturated = ThermalControlProfilePointConfig {
        brake_distance_centi_c: 5_000,
        ..previous
    };
    assert_eq!(tune_point(saturated, facts), saturated);
    assert_eq!(reseed_after_hold_confirm(saturated, facts), saturated);
    for attempt in 0..32 {
        assert!(bounded_failure_alternative(saturated, facts, attempt).is_none());
    }
}

#[test]
fn limited_hold_high_side_feedback_requires_complete_low_output_facts() {
    let previous = point(60);
    let facts = TuneEvidence {
        overshoot_c: 2.62,
        over_c: 2.62,
        hold_max_below_c: Some(0.49),
        first_band_observed: true,
        first_band_exit_centi_c: Some(5_848),
        hold_sample_count: 22,
        hold_off_sample_count: 20,
        hold_max_output_permille: Some(60),
        approach_output_range: Some((50, 990)),
        ..evidence(60)
    };
    let tuned = limited_hold_high_side_candidate(previous, facts).unwrap();
    assert_eq!(
        tuned,
        ThermalControlProfilePointConfig {
            approach_damping_exponent_permille: 2_000,
            ..previous
        }
    );
    for unavailable in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            target_c: 160,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            full_speed_failed: false,
            ..facts
        },
        TuneEvidence {
            first_band_observed: false,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 2,
            hold_off_sample_count: 2,
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: Some(101),
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 17,
            ..facts
        },
        TuneEvidence {
            hold_max_below_c: Some(1.51),
            ..facts
        },
        TuneEvidence {
            hold_max_below_c: None,
            ..facts
        },
        TuneEvidence {
            over_c: 1.5,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 990)),
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((50, 100)),
            ..facts
        },
        TuneEvidence {
            approach_output_range: None,
            ..facts
        },
    ] {
        assert!(limited_hold_high_side_candidate(previous, unavailable).is_none());
    }
}

fn unavailable_maintenance_supply(facts: TuneEvidence) -> [TuneEvidence; 18] {
    [
        TuneEvidence {
            hold_off_sample_count: 11,
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: Some(141),
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_max_below_c: Some(1.51),
            ..facts
        },
        TuneEvidence {
            hold_max_below_c: None,
            ..facts
        },
        TuneEvidence {
            approach_median_output_permille: Some(141),
            ..facts
        },
        TuneEvidence {
            approach_median_output_permille: Some(100),
            ..facts
        },
        TuneEvidence {
            approach_median_output_permille: None,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 200)),
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((50, 1_001)),
            ..facts
        },
        TuneEvidence {
            approach_output_range: None,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 2,
            ..facts
        },
        TuneEvidence {
            first_band_observed: false,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(9_849),
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            full_speed_failed: false,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            target_c: 160,
            ..facts
        },
    ]
}

#[test]
fn maintenance_scaled_braking_requires_complete_bounded_supply() {
    let previous = ThermalControlProfilePointConfig {
        hold_power_permille: 138,
        ..point(100)
    };
    let facts = TuneEvidence {
        overshoot_c: 1.54,
        over_c: 1.54,
        hold_max_below_c: Some(0.48),
        first_band_observed: true,
        first_band_exit_centi_c: Some(10_154),
        hold_sample_count: 24,
        hold_off_sample_count: 12,
        hold_max_output_permille: Some(140),
        approach_median_output_permille: Some(140),
        approach_output_range: Some((50, 200)),
        ..evidence(100)
    };
    assert!(low_power_coasting_evidence(previous, facts));
    assert_eq!(
        tune_point(previous, facts),
        ThermalControlProfilePointConfig {
            brake_distance_centi_c: previous.brake_distance_centi_c + 79,
            ..previous
        }
    );
    for unavailable in unavailable_maintenance_supply(facts) {
        assert!(
            !low_power_coasting_evidence(previous, unavailable),
            "{unavailable:?}"
        );
    }
    assert!(!low_power_coasting_evidence(
        ThermalControlProfilePointConfig {
            hold_power_permille: 100,
            ..previous
        },
        facts
    ));
}

#[test]
fn low_supply_hold_percentiles_do_not_hide_sustained_heat_or_large_pulses() {
    let previous = point(60);
    let facts = TuneEvidence {
        overshoot_c: 2.29,
        over_c: 2.29,
        hold_max_below_c: Some(0.46),
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_164),
        hold_sample_count: 12,
        hold_off_sample_count: 7,
        hold_median_output_permille: Some(0),
        hold_p90_output_permille: Some(40),
        hold_max_output_permille: Some(80),
        approach_output_range: Some((50, 760)),
        ..evidence(60)
    };
    assert!(limited_hold_high_side_candidate(previous, facts).is_some());
    for unavailable in [
        TuneEvidence {
            hold_max_output_permille: Some(101),
            ..facts
        },
        TuneEvidence {
            hold_max_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_p90_output_permille: Some(51),
            ..facts
        },
        TuneEvidence {
            hold_p90_output_permille: None,
            ..facts
        },
        TuneEvidence {
            target_c: 160,
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 2,
            hold_off_sample_count: 1,
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((0, 760)),
            ..facts
        },
        TuneEvidence {
            approach_output_range: Some((50, 1_001)),
            ..facts
        },
        TuneEvidence {
            over_c: 1.5,
            ..facts
        },
        TuneEvidence {
            hold_max_below_c: Some(1.51),
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
    ] {
        assert!(
            limited_hold_high_side_candidate(previous, unavailable).is_none(),
            "{unavailable:?}"
        );
    }
}

#[test]
fn saturated_coasting_horizon_requires_complete_normal_approach_evidence() {
    let previous = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 4_000,
        ..point(60)
    };
    let facts = TuneEvidence {
        overshoot_c: 1.57,
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_157),
        hold_sample_count: 14,
        hold_off_sample_count: 14,
        approach_output_range: Some((50, 790)),
        ..evidence(60)
    };
    let expected = ThermalControlProfilePointConfig {
        approach_lead_ticks: 2,
        ..previous
    };
    assert_eq!(
        coasting_high_side_candidate(previous, facts),
        Some(expected)
    );
    assert_eq!(
        bounded_failure_alternative(previous, facts, 0),
        Some(expected)
    );
    let brake = bounded_failure_alternative(previous, facts, 1).unwrap();
    assert_eq!(brake.approach_lead_ticks, previous.approach_lead_ticks);
    assert_eq!(
        brake.brake_distance_centi_c,
        previous.brake_distance_centi_c + 40
    );
    for range in [None, Some((0, 790)), Some((50, 50)), Some((50, 1_010))] {
        assert!(
            longer_trajectory_horizon(
                previous,
                TuneEvidence {
                    approach_output_range: range,
                    ..facts
                },
                2
            )
            .is_none()
        );
    }
    let saturated = ThermalControlProfilePointConfig {
        approach_lead_ticks: 12,
        ..previous
    };
    assert!(coasting_high_side_candidate(saturated, facts).is_none());
}

#[test]
fn captured_coasting_high_side_feedback_changes_only_effective_damping() {
    let previous = ThermalControlProfilePointConfig {
        brake_distance_centi_c: 1_553,
        hold_power_permille: 50,
        hold_reheat_power_permille: 50,
        hold_kp_permille_per_c: 21,
        ..point(60)
    };
    // Calibration-only 60C: all physical Hold samples request zero,
    // yet stored Approach heat carries the trace above the upper band.
    let facts = TuneEvidence {
        overshoot_c: 4.96,
        hold_p2p_c: 5.18,
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_161),
        hold_sample_count: 13,
        hold_off_sample_count: 13,
        ..evidence(60)
    };
    let expected = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 2_000,
        ..previous
    };
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    for unavailable in [
        TuneEvidence {
            tunable_stop: false,
            ..facts
        },
        TuneEvidence {
            target_c: 160,
            ..facts
        },
        TuneEvidence {
            full_speed_failed: false,
            ..facts
        },
        TuneEvidence {
            stable_window_verified: true,
            ..facts
        },
        TuneEvidence {
            first_band_observed: false,
            ..facts
        },
        TuneEvidence {
            first_band_exit_centi_c: Some(5_840),
            ..facts
        },
        TuneEvidence {
            hold_sample_count: 2,
            hold_off_sample_count: 2,
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 12,
            ..facts
        },
    ] {
        assert!(coasting_high_side_candidate(previous, unavailable).is_none());
    }
    let saturated = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 4_000,
        ..previous
    };
    assert!(coasting_high_side_candidate(saturated, facts).is_none());
    for (overshoot_c, step) in [(1.55, 500), (2.12, 620), (2.62, 1_000), (20.0, 1_000)] {
        let tuned = coasting_high_side_candidate(
            previous,
            TuneEvidence {
                overshoot_c,
                ..facts
            },
        )
        .unwrap();
        assert_eq!(tuned.approach_damping_exponent_permille, 1_000 + step);
    }
}

#[test]
fn captured_single_low_hold_entry_pulse_still_corrects_approach_heat() {
    let previous = ThermalControlProfilePointConfig {
        brake_distance_centi_c: 1_553,
        approach_damping_exponent_permille: 2_000,
        approach_power_permille: 1_000,
        approach_floor_power_permille: 50,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 50,
        hold_reheat_power_permille: 50,
        hold_kp_permille_per_c: 21,
        hold_ki_permille_per_c_tick: 1,
        hold_off_centi_c: 5,
        overshoot_cutoff_centi_c: 25,
        hold_blend_ticks: 4,
        hold_lead_ticks: 4,
        ..point(60)
    };
    // ac83f9d trial 3: one 50-permille entrance sample, then fifteen
    // zero requests while stored heat carries temperature above band.
    let facts = TuneEvidence {
        overshoot_c: 1.93,
        hold_p2p_c: 2.40,
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_153),
        hold_sample_count: 16,
        hold_off_sample_count: 15,
        hold_entry_output_permille: Some(50),
        ..evidence(60)
    };
    let expected = ThermalControlProfilePointConfig {
        approach_damping_exponent_permille: 2_500,
        ..previous
    };
    assert_eq!(tune_point(previous, facts), expected);
    assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
    let alternative = bounded_failure_alternative(previous, facts, 0).unwrap();
    assert_eq!(alternative.approach_damping_exponent_permille, 2_100);
    assert_eq!(
        alternative.hold_power_permille,
        previous.hold_power_permille
    );
    for unavailable in [
        TuneEvidence {
            hold_entry_output_permille: None,
            ..facts
        },
        TuneEvidence {
            hold_entry_output_permille: Some(0),
            ..facts
        },
        TuneEvidence {
            hold_entry_output_permille: Some(51),
            ..facts
        },
        TuneEvidence {
            hold_off_sample_count: 14,
            ..facts
        },
    ] {
        assert!(coasting_high_side_candidate(previous, unavailable).is_none());
    }
}

#[test]
fn high_side_trim_never_raises_calibration_only_hold_or_gain() {
    let facts = TuneEvidence {
        overshoot_c: 4.96,
        hold_p2p_c: 5.18,
        first_band_observed: true,
        first_band_exit_centi_c: Some(6_161),
        ..evidence(60)
    };
    for baseline in [40, 50, 92, 100, 150] {
        for gain in [4, 8, 21] {
            let previous = ThermalControlProfilePointConfig {
                hold_power_permille: baseline,
                hold_reheat_power_permille: baseline,
                hold_kp_permille_per_c: gain,
                ..point(60)
            };
            for next in [
                tune_point(previous, facts),
                reseed_after_hold_confirm(previous, facts),
            ] {
                assert!(next.hold_power_permille <= previous.hold_power_permille);
                assert!(next.hold_reheat_power_permille <= previous.hold_reheat_power_permille);
                assert!(next.hold_reheat_power_permille >= next.hold_power_permille);
                assert!(next.hold_kp_permille_per_c <= previous.hold_kp_permille_per_c);
                assert!(next.brake_distance_centi_c > previous.brake_distance_centi_c);
            }
        }
    }
}

#[test]
fn high_temperature_band_failure_uses_bounded_energy_trim_for_trial_and_confirmation() {
    for target_c in [160, 180, 220, 240] {
        let previous = high_temperature_point(target_c);
        let facts = high_temperature_failure(target_c);
        let expected = ThermalControlProfilePointConfig {
            brake_distance_centi_c: 1_139,
            approach_power_permille: 580,
            approach_floor_power_permille: 500,
            hold_power_permille: 550,
            hold_reheat_power_permille: 550,
            hold_entry_centi_c: 370,
            hold_exit_centi_c: 338,
            hold_kp_permille_per_c: 8,
            overshoot_cutoff_centi_c: 107,
            ..previous
        };
        assert_eq!(tune_point(previous, facts), expected);
        assert_eq!(reseed_after_hold_confirm(previous, facts), expected);
        // The effective floor is still constrained by the production Hold baseline.
        assert_eq!(
            expected
                .approach_floor_power_permille
                .max(expected.hold_power_permille)
                .max(100),
            550
        );
    }
}

#[test]
fn late_stable_window_never_narrows_hold_reentry_below_stable_band() {
    let mut previous = point(60);
    previous.hold_exit_centi_c = 50;
    let facts = TuneEvidence {
        hold_sample_count: 2,
        under_c: 0.3,
        over_c: 0.4,
        first_hold_after_warmup_ms: Some(9_000),
        stable_window_after_warmup_ms: Some(10_001),
        settle_limit_ms: 10_000,
        ..evidence(60)
    };
    let tuned = tune_base_point(previous, facts);
    assert_eq!(tuned.hold_exit_centi_c, SEARCH_HOLD_EXIT_FLOOR_CENTI_C);
}

#[test]
fn high_temperature_lower_miss_keeps_acceleration_and_does_not_trim_energy() {
    let previous = high_temperature_point(160);
    let facts = TuneEvidence {
        first_band_observed: false,
        first_band_exit_centi_c: None,
        deadline_temperature_centi_c: Some(15_700),
        ..high_temperature_failure(160)
    };
    assert!(conservative_high_side_candidate(previous, facts).is_none());
    let tuned = tune_point(previous, facts);
    assert!(tuned.approach_floor_power_permille >= previous.approach_floor_power_permille);
    assert!(tuned.approach_power_permille >= previous.approach_power_permille);
}

#[test]
fn high_temperature_trim_requires_high_side_and_a_failed_gate_within_scope() {
    let previous = high_temperature_point(160);
    let facts = high_temperature_failure(160);
    let no_high_side = TuneEvidence {
        first_band_exit_centi_c: None,
        deadline_temperature_centi_c: None,
        ..facts
    };
    assert!(conservative_high_side_candidate(previous, no_high_side).is_none());
    let no_thermal_failure = TuneEvidence {
        full_speed_failed: false,
        stable_window_verified: true,
        overshoot_c: 3.0,
        hold_p2p_c: 3.0,
        residual_c: 3.0,
        ..facts
    };
    assert!(conservative_high_side_candidate(previous, no_thermal_failure).is_none());
    let safety_failure = TuneEvidence {
        tunable_stop: false,
        ..facts
    };
    assert_eq!(tune_point(previous, safety_failure), previous);
    assert_eq!(
        reseed_after_hold_confirm(previous, safety_failure),
        previous
    );
    assert!(
        conservative_high_side_candidate(
            high_temperature_point(250),
            high_temperature_failure(250)
        )
        .is_none()
    );
}

#[test]
fn unverified_high_temperature_upper_exit_trims_even_below_thermal_hard_limits() {
    // Captured 160C scout 3: the window breaks at 161.71C while the
    // Underpowered classification otherwise raises the floor 700 -> 760.
    let previous = ThermalControlProfilePointConfig {
        brake_distance_centi_c: 801,
        approach_floor_power_permille: 700,
        approach_power_permille: 780,
        hold_power_permille: 449,
        hold_reheat_power_permille: 494,
        ..high_temperature_point(160)
    };
    for target_c in [160, 180, 220, 240] {
        let previous = ThermalControlProfilePointConfig {
            target_temp_c: target_c,
            ..previous
        };
        let facts = TuneEvidence {
            curve_class: CurveClass::Underpowered,
            overshoot_c: 1.60,
            hold_p2p_c: 1.67,
            residual_c: 1.67,
            first_band_observed: true,
            first_band_exit_centi_c: Some(target_c * 100 + 171),
            ..evidence(target_c)
        };
        for tuned in [
            tune_point(previous, facts),
            reseed_after_hold_confirm(previous, facts),
        ] {
            assert_eq!(tuned.approach_floor_power_permille, 600);
            assert_eq!(tuned.approach_power_permille, 680);
            assert_eq!(tuned.hold_power_permille, 399);
            assert_eq!(tuned.hold_reheat_power_permille, 444);
            assert_eq!(tuned.brake_distance_centi_c, 1101);
        }
        let missed_upper = TuneEvidence {
            first_band_observed: false,
            first_band_exit_centi_c: None,
            deadline_temperature_centi_c: Some(target_c * 100 + 171),
            ..facts
        };
        assert!(tune_point(previous, missed_upper).approach_floor_power_permille < 700);
        let verified = TuneEvidence {
            full_speed_failed: false,
            stable_window_verified: true,
            ..facts
        };
        assert!(conservative_high_side_candidate(previous, verified).is_none());
    }
    let low_target = TuneEvidence {
        first_band_observed: true,
        first_band_exit_centi_c: Some(10_171),
        overshoot_c: 1.71,
        hold_p2p_c: 1.71,
        ..evidence(100)
    };
    assert!(conservative_high_side_candidate(point(100), low_target).is_none());
}

#[test]
fn bounded_residual_matches_main_coast_gate_vector() {
    let mut previous = point(60);
    previous.brake_distance_centi_c = 1_910;
    let mut facts = evidence(60);
    facts.overshoot_c = 3.3;
    facts.hold_p2p_c = 2.4;
    facts.residual_c = 2.4;
    facts.over_c = 3.3;
    facts.hold_sample_count = 7;
    let tuned = tune_point(previous, facts);
    assert_eq!(tuned.brake_distance_centi_c, 1_910);
    assert_eq!(tuned.hold_exit_centi_c, 380);
}

#[test]
fn completed_small_hold_ripple_uses_trim_without_approach_acceleration() {
    let mut previous = point(60);
    previous.hold_kp_permille_per_c = 8;
    previous.hold_blend_ticks = 1;
    let mut facts = evidence(60);
    facts.full_speed_failed = false;
    facts.scout_hold_complete = true;
    facts.stable_window_verified = true;
    facts.hold_sample_count = 35;
    facts.overshoot_c = 1.41;
    facts.hold_p2p_c = 3.58;
    let tuned = tune_point(previous, facts);
    assert_eq!(tuned.hold_blend_ticks, 2);
    assert_eq!(tuned.hold_kp_permille_per_c, 8);
    assert_eq!(
        tuned.brake_distance_centi_c,
        previous.brake_distance_centi_c
    );
    assert_eq!(
        tuned.approach_power_permille,
        previous.approach_power_permille
    );

    facts.stable_window_verified = false;
    assert_ne!(tune_point(previous, facts), tuned);
}

#[test]
fn high_temperature_power_limited_branch_matches_main() {
    let previous = point(220);
    let mut facts = evidence(220);
    facts.curve_class = CurveClass::Underpowered;
    facts.approach_median_output_permille = Some(960);
    facts.approach_median_slope_c_per_s = Some(0.35);
    let tuned = tune_point(previous, facts);
    assert_eq!(tuned.warmup_power_permille, 1_000);
    assert_eq!(tuned.approach_power_permille, 1_000);
    assert_eq!(tuned.approach_floor_power_permille, 1_000);
    assert_eq!(tuned.hold_power_permille, 1_000);
    assert_eq!(tuned.hold_reheat_power_permille, 1_000);
    assert_eq!(tuned.approach_lead_ticks, 0);
}

#[test]
fn unsafe_or_incomplete_evidence_never_changes_candidate() {
    let previous = point(60);
    let mut facts = evidence(60);
    facts.tunable_stop = false;
    facts.overshoot_c = 9.0;
    assert_eq!(tune_point(previous, facts), previous);
}
