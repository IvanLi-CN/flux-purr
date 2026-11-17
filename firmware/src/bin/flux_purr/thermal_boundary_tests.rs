use super::*;
use flux_purr_firmware::heater_supply::{BoundaryAdcMetadata, BoundaryPoint, SupplyBoundary};

#[test]
fn reserved_observed_contract_does_not_pause_at_a_live_rtd_bucket_edge() {
    // Minimized from HTTP run 3977adc5: the 13.6 V working contract was
    // blanked when its 13.7 V observation disappeared across an RTD edge.
    let config = observed_bucket_edge_config();
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

fn observed_bucket_edge_config() -> MemoryConfig {
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
        point_count: 3,
        ..Default::default()
    };
    for (slot, (rtd_mv, request_mv)) in [(1_049, 13_000), (1_065, 13_700), (1_309, 21_000)]
        .into_iter()
        .enumerate()
    {
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
