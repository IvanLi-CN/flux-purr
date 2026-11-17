use super::*;
use flux_purr_firmware::heater_supply::{BoundaryAdcMetadata, BoundaryPoint, SupplyBoundary};

#[test]
fn warmup_observed_voltage_steps_keep_the_confirmed_gate() {
    // HTTP run 69fe6e16: 12.9 -> 13.6 V and 14.4 -> 15.2 V were safe
    // boundary increases, but the unbounded Warmup request tripped the
    // production large-step blanking guard (diagnostic flags=4).
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(3_300, 21_000, 3_000)]);
    for (mut current_mv, ceiling_mv) in [(12_900, 13_700), (14_400, 15_300), (17_800, 18_700)] {
        let target_mv = automatic_heater_working_ceiling_mv(5_500, ceiling_mv);
        while current_mv < target_mv {
            let next_mv = automatic_heater_request_mv_for_phase(
                HeaterControlPhase::Warmup,
                current_mv,
                ceiling_mv,
                5_500,
                ceiling_mv,
            );
            let active = ConfirmedActiveContract::from_private_contract(
                source
                    .select_exact_contract(
                        PdContractRequest::automatic_pps(current_mv, 3_000).unwrap(),
                    )
                    .unwrap(),
                source,
            );
            assert!(
                !automatic_pps_transition_should_blank_heater(
                    false,
                    automatic_pps_reconciliation_requires_pause(active, ceiling_mv),
                    should_blank_heater_for_pps_range_change(
                        current_mv, next_mv, false, 5_500, 21_000
                    ),
                ),
                "safe Warmup step {current_mv} -> {next_mv} must preserve conduction"
            );
            assert!(next_mv > current_mv);
            assert!(next_mv <= target_mv);
            current_mv = next_mv;
        }
    }
}

#[test]
fn reserved_observed_contract_does_not_pause_at_a_live_rtd_bucket_edge() {
    // Minimized from HTTP run 3977adc5: the 13.6 V working contract was
    // blanked when its 13.7 V observation disappeared across an RTD edge.
    let config = observed_boundary_config(&[(1_049, 13_000), (1_065, 13_700), (1_309, 21_000)]);
    let edge_c = projected_rtd_temperature_c(&config, 1_065).unwrap();
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));
    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(3_300, 21_000, 3_000)]);
    let active = ConfirmedActiveContract::from_private_contract(
        source
            .select_exact_contract(PdContractRequest::automatic_pps(13_600, 3_000).unwrap())
            .unwrap(),
        source,
    );
    for (rtd_mv, temp_c) in [(1_065, edge_c), (1_064, edge_c - 1.0), (1_065, edge_c)] {
        let candidate = manual
            .automatic_heater_pps_candidate_with_rtd(
                temp_c,
                None,
                &config,
                Some(13_600),
                Some(rtd_mv),
            )
            .unwrap();
        assert!(candidate.boundary_applied);
        assert_eq!(candidate.allowed_mv, 13_700);
        assert_eq!(
            automatic_heater_request_mv_for_phase(
                HeaterControlPhase::Warmup,
                13_600,
                candidate.allowed_mv,
                candidate.min_mv,
                candidate.allowed_mv,
            ),
            13_600
        );
        assert!(!automatic_pps_reconciliation_requires_pause(
            active,
            candidate.allowed_mv
        ));
    }
    for previous_mv in [None, Some(12_900)] {
        let candidate = manual
            .automatic_heater_pps_candidate_with_rtd(
                edge_c - 1.0,
                None,
                &config,
                previous_mv,
                Some(1_064),
            )
            .unwrap();
        assert_eq!(candidate.allowed_mv, 13_000, "no early promotion");
    }
    let colder = manual
        .automatic_heater_pps_candidate_with_rtd(
            edge_c - 4.0,
            None,
            &config,
            Some(13_600),
            Some(1_056),
        )
        .unwrap();
    assert_eq!(colder.allowed_mv, 13_000);
    assert!(automatic_pps_reconciliation_requires_pause(
        active,
        colder.allowed_mv
    ));
}

#[test]
fn rising_temperature_does_not_cross_a_later_lower_observed_ceiling() {
    // The same HTTP calibration contains 16.1 V at RTD 1134 mV followed
    // by 15.9 V at 1145 mV. A safe warm-up envelope must anticipate the
    // lower observation without promoting any unobserved higher voltage.
    let points = [
        (1_049, 13_000),
        (1_134, 16_100),
        (1_145, 15_900),
        (1_309, 21_000),
    ];
    let config = observed_boundary_config(&points);
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));
    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(3_300, 21_000, 3_000)]);
    let mut current_mv = 12_900;
    let edge_c = projected_rtd_temperature_c(&config, 1_134).unwrap();
    for (previous_mv, expected_mv) in [(None, 13_000), (Some(15_800), 15_900)] {
        let candidate = manual
            .automatic_heater_pps_candidate_with_rtd(
                edge_c - 1.0,
                None,
                &config,
                previous_mv,
                Some(1_133),
            )
            .unwrap();
        assert_eq!(candidate.allowed_mv, expected_mv);
    }
    for (rtd_mv, measured_ceiling_mv) in points {
        let temp_c = projected_rtd_temperature_c(&config, rtd_mv).unwrap();
        let candidate = manual
            .automatic_heater_pps_candidate_with_rtd(
                temp_c,
                None,
                &config,
                Some(current_mv),
                Some(rtd_mv),
            )
            .unwrap();
        assert!(candidate.boundary_applied);
        assert!(candidate.allowed_mv <= measured_ceiling_mv);
        let active = ConfirmedActiveContract::from_private_contract(
            source
                .select_exact_contract(PdContractRequest::automatic_pps(current_mv, 3_000).unwrap())
                .unwrap(),
            source,
        );
        assert!(
            !automatic_pps_reconciliation_requires_pause(active, candidate.allowed_mv),
            "RTD={rtd_mv}: active={current_mv}, ceiling={}",
            candidate.allowed_mv
        );
        current_mv = automatic_heater_request_mv_for_phase(
            HeaterControlPhase::Warmup,
            current_mv,
            candidate.allowed_mv,
            candidate.min_mv,
            candidate.allowed_mv,
        );
    }
    assert_eq!(
        config.heater_supply_boundary.unwrap().points[1].tracking_request_mv,
        16_100
    );
}

fn observed_boundary_config(points: &[(u16, u16)]) -> MemoryConfig {
    let mut boundary = SupplyBoundary {
        adc: BoundaryAdcMetadata {
            divider_supply_mv: RTD_DIVIDER_SUPPLY_MV,
            ..Default::default()
        },
        wifi_started: false,
        source_min_mv: 5_500,
        source_max_mv: 21_000,
        source_max_ma: 3_000,
        advertised_min_mv: 3_300,
        advertised_max_mv: 21_000,
        point_count: points.len() as u8,
        ..Default::default()
    };
    for (slot, (rtd_mv, request_mv)) in points.iter().copied().enumerate() {
        boundary.points[slot] = BoundaryPoint {
            elapsed_ms: (slot as u32 + 1) * 50,
            rtd_code: 2_000,
            rtd_mv,
            vin_code: 1_000,
            vin_mv: 1_000,
            request_current_ma: 3_000,
            tracking_request_mv: request_mv,
            tracking_only: true,
            ..Default::default()
        };
    }
    assert!(boundary.is_valid());
    MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..Default::default()
    }
}
