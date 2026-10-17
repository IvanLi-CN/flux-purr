use super::*;

fn query(temperature_centi_c: i16, current_ma: u16, wifi_started: bool) -> BoundaryQuery {
    BoundaryQuery {
        temperature_centi_c,
        current_ma,
        wifi_started,
        source_min_mv: 5_500,
        source_max_mv: 21_000,
        source_max_ma: 3_000,
    }
}

fn boundary() -> SupplyBoundary {
    let mut result = SupplyBoundary {
        adc: BoundaryAdcMetadata {
            divider_supply_mv: 3_328,
            reference_mv: 1_800,
            reference_code: 3_000,
            init_code: 500,
            efuse_version: 1,
            attenuation_db: 6,
        },
        wifi_started: true,
        source_min_mv: 5_500,
        source_max_mv: 21_000,
        source_max_ma: 3_000,
        advertised_min_mv: 5_500,
        advertised_max_mv: 21_000,
        ..SupplyBoundary::default()
    };
    for (request_current_ma, tracking_request_mv, rtd_mv, _) in
        [(1_800, 7_200, 1_000, 3_000), (2_000, 10_000, 1_200, 10_000)]
    {
        assert!(result.retain(BoundaryPoint {
            elapsed_ms: 10_000,
            rtd_code: 1_500,
            rtd_mv,
            vin_code: 1_000,
            vin_mv: 800,
            request_current_ma,
            tracking_request_mv,
            flat_request_mv: tracking_request_mv + 200,
            tracking_only: false,
        }));
    }
    result
}

#[test]
fn raw_points_round_trip_and_legacy_bytes_are_absent() {
    let value = boundary();
    let mut bytes = [0; BOUNDARY_MAX_BYTES];
    let len = value.encode(&mut bytes).unwrap();
    assert_eq!(SupplyBoundary::decode(&bytes[..len]), Some(value));
    bytes[0] = 0;
    assert!(SupplyBoundary::decode(&bytes[..len]).is_none());
    assert!(SupplyBoundary::decode(&[0; 64]).is_none());
}

#[test]
fn lookup_does_not_invent_colder_lower_current_or_other_wifi_observations() {
    let value = boundary();
    let lookup = |temp, current, wifi| {
        value.safe_request_mv(query(temp, current, wifi), |raw| match raw {
            1_000 => Some(3_000),
            1_200 => Some(10_000),
            _ => None,
        })
    };
    assert_eq!(lookup(2_500, 3_000, true), None);
    assert_eq!(lookup(5_000, 1_700, true), None);
    assert_eq!(lookup(5_000, 3_000, false), None);
    assert_eq!(lookup(5_000, 3_000, true), None);
    assert_eq!(lookup(10_100, 3_000, true), Some(10_000));
    assert_eq!(lookup(3_100, 1_900, true), Some(7_200));
    assert_eq!(lookup(3_101, 1_900, true), None);
    assert_eq!(lookup(10_101, 3_000, true), None);
}

#[test]
fn scan_floor_and_advertised_apdo_are_distinct_and_bound_the_lookup() {
    let mut value = boundary();
    value.advertised_min_mv = 3_300;
    value.advertised_max_mv = 22_000;
    let request = BoundaryQuery {
        source_min_mv: 3_300,
        source_max_mv: 22_000,
        ..query(10_100, 3_000, true)
    };
    assert_eq!(
        value.safe_request_mv(request, |_| Some(10_000)),
        Some(10_000)
    );
    assert!(
        value
            .safe_request_mv(query(10_100, 3_000, true), |_| Some(10_000))
            .is_none()
    );
    let mut encoded = [0; BOUNDARY_MAX_BYTES];
    let len = value.encode(&mut encoded).unwrap();
    assert_eq!(SupplyBoundary::decode(&encoded[..len]), Some(value));
}

#[test]
fn correcting_the_temperature_projection_changes_lookup_without_rewriting_raw_adc() {
    let value = boundary();
    assert_eq!(
        value.safe_request_mv(query(10_100, 3_000, true), |_| Some(11_000)),
        None,
    );
    assert_eq!(value, boundary());
}

#[test]
fn a_slow_ramp_through_one_degree_is_not_equilibrium() {
    let mut window = EquilibriumWindow::default();
    for tick in 0..=200 {
        assert!(!window.observe(tick * 50, 10_000 + (tick / 4) as i16));
    }
    for tick in 201..=402 {
        if window.observe(tick * 50, 10_050) {
            assert!(tick >= 401);
            return;
        }
    }
    panic!("a stationary ten-second window must qualify");
}

#[test]
fn noise_outside_the_full_band_restarts_the_equilibrium_window() {
    let mut window = EquilibriumWindow::default();
    assert!(!window.observe(0, 10_000));
    assert!(!window.observe(9_000, 10_101));
    assert!(!window.observe(10_000, 10_101));
    assert!(window.observe(19_000, 10_101));
}

fn sample(now_ms: u32, output: scan::ScanOutput, temperature: i16) -> scan::ScanInput {
    // Fake PPS source with an explicit current loop, no actual-current input.
    let delivered_mv = output.request_mv.min(output.request_ma * 4);
    scan::ScanInput {
        now_ms,
        temperature_centi_c: temperature,
        wifi_started: true,
        confirmed: true,
        physical_percent: if output.heater_enabled { 100 } else { 0 },
        rtd_code: 1_500,
        rtd_mv: 1_000,
        vin_code: delivered_mv / 4,
        vin_mv: delivered_mv / 12,
    }
}

#[test]
fn preflight_recovers_the_last_tracking_current_after_the_measured_cc_drop() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    for now_ms in (0..30_000).step_by(50) {
        let output = job.output();
        let mut input = sample(now_ms, output, 3_200);
        input.vin_mv = if !output.heater_enabled {
            456
        } else if output.request_ma >= 1_400 {
            438
        } else {
            402
        };
        let recovered = job.update(input);
        if output.request_ma == 1_300 {
            assert_ne!(job.phase, scan::ScanPhase::Failed, "{:?}", job.failure);
            if recovered.request_ma == 1_400 {
                assert!(!recovered.heater_enabled);
                let mut waiting = sample(now_ms + 1_000, recovered, 3_200);
                waiting.vin_mv = 456;
                waiting.confirmed = false;
                assert!(!job.update(waiting).heater_enabled);
                waiting.confirmed = true;
                waiting.now_ms += 999;
                assert!(!job.update(waiting).heater_enabled);
                waiting.now_ms += 1;
                assert!(job.update(waiting).heater_enabled);
                assert_eq!(job.phase, scan::ScanPhase::Sweep);
                return;
            }
        }
    }
    panic!("preflight did not recover the last confirmed tracking current");
}

#[test]
fn preflight_recovery_rejects_collapsed_unloaded_vin_or_remaining_conduction() {
    for (vin_mv, physical_percent) in [(402, 0), (456, 100), (0, 0)] {
        let mut job = scan::SupplyScan::new(boundary()).unwrap();
        let mut recovered = false;
        for now_ms in (0..30_000).step_by(50) {
            let output = job.output();
            let mut input = sample(now_ms, output, 3_200);
            input.vin_mv = if !output.heater_enabled {
                456
            } else if output.request_ma >= 1_400 {
                438
            } else {
                402
            };
            let next = job.update(input);
            if output.request_ma == 1_300 && next.request_ma == 1_400 {
                let mut recovery = sample(now_ms + 1_000, next, 3_200);
                recovery.vin_mv = vin_mv;
                recovery.physical_percent = physical_percent;
                assert!(!job.update(recovery).heater_enabled);
                assert_eq!(job.phase, scan::ScanPhase::Failed);
                recovered = true;
                break;
            }
        }
        assert!(recovered, "the captured current-loop knee was not reached");
    }
}

#[test]
fn preflight_initial_load_collapse_still_fails_closed() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut baseline = sample(1_000, job.output(), 3_000);
    baseline.vin_mv = 456;
    assert!(job.update(baseline).heater_enabled);
    let mut loaded = sample(2_000, job.output(), 3_000);
    loaded.vin_mv = 402;
    assert!(!job.update(loaded).heater_enabled);
    assert_eq!(job.failure, Some(scan::ScanFailure::SupplyUnstable));
}

#[test]
fn scan_discovers_the_sustainable_start_current_and_never_forces_one_amp() {
    let mut value = boundary();
    value.point_count = 0;
    let mut job = scan::SupplyScan::new(value).unwrap();
    for time in (0..2_000_000).step_by(50) {
        let output = job.output();
        job.update(sample(time, output, 3_000));
        if job.phase == scan::ScanPhase::Complete {
            assert!(job.boundary.is_valid());
            assert!(job.boundary.point_count > 5);
            assert_eq!(job.boundary.points[0].request_current_ma, 1_400);
            for point in &job.boundary.points[..job.boundary.point_count as usize] {
                assert!(point.tracking_request_mv <= point.request_current_ma * 4);
            }
            assert!(!job.output().heater_enabled);
            return;
        }
        assert_ne!(job.phase, scan::ScanPhase::Failed, "{:?}", job.failure);
    }
    panic!("fake source scan did not terminate");
}

#[test]
fn cutoff_and_wifi_change_turn_off_the_output_in_the_same_update() {
    for wifi_change in [false, true] {
        let mut job = scan::SupplyScan::new(boundary()).unwrap();
        let mut input = sample(1_000, job.output(), 3_000);
        if wifi_change {
            input.wifi_started = false;
        } else {
            input.temperature_centi_c = 22_000;
        }
        assert!(!job.update(input).heater_enabled);
        assert_eq!(
            job.phase,
            if wifi_change {
                scan::ScanPhase::Failed
            } else {
                scan::ScanPhase::Complete
            }
        );
    }
}

#[test]
fn unconfirmed_contracts_do_not_create_boundary_points() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    job.boundary.point_count = 0;
    for now_ms in (0..20_000).step_by(50) {
        let mut input = sample(now_ms, job.output(), 3_000);
        input.confirmed = false;
        job.update(input);
        assert_eq!(job.phase, scan::ScanPhase::Baseline);
        assert_eq!(job.boundary.point_count, 0);
    }
}

#[test]
fn dynamic_scan_keeps_three_amp_tracking_points_across_warmup() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut temperature = 25.0_f32;
    // Independent synthetic plant: temperature-dependent plate resistance,
    // constant auxiliary watts and an ideal source current loop. These values
    // are test inputs, not measurements or estimates reported by the product.
    for now_ms in (0..21_600_000).step_by(50) {
        let output = job.output();
        let resistance = 3.2 * (1.0 + 0.00393 * (temperature - 20.0));
        let current = f32::from(output.request_ma) / 1_000.0;
        let cc_voltage =
            resistance * (current + (current * current - 4.0 / resistance).sqrt()) / 2.0;
        let voltage = (f32::from(output.request_mv) / 1_000.0).min(if output.heater_enabled {
            cc_voltage
        } else {
            21.0
        });
        let raw_temperature = (1_000.0 + temperature * 4.0).round() as u16;
        job.update(scan::ScanInput {
            now_ms,
            temperature_centi_c: (temperature * 100.0).round() as i16,
            wifi_started: true,
            confirmed: true,
            physical_percent: u8::from(output.heater_enabled) * 100,
            rtd_code: raw_temperature * 2,
            rtd_mv: raw_temperature,
            vin_code: (voltage * 150.0).round() as u16,
            vin_mv: (voltage * 75.0).round() as u16,
        });
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "{:?}, T={temperature}, ms={now_ms}",
            job.failure
        );
        if job.phase == scan::ScanPhase::Complete {
            let points = &job.boundary.points[..usize::from(job.boundary.point_count)];
            assert!(job.boundary.is_valid());
            assert!(points.iter().any(|p| p.tracking_only && p.rtd_mv < 1_400));
            assert!(points.iter().any(|p| p.tracking_only && p.rtd_mv > 1_600));
            assert!(points.iter().all(|p| p.rtd_mv < 1_900));
            return;
        }
        let power = if output.heater_enabled {
            voltage * voltage / resistance
        } else {
            0.0
        };
        temperature += (power - 0.16 * (temperature - 25.0)) / 12.0 * 0.05;
    }
    panic!(
        "dynamic scan did not complete, phase={:?}, T={temperature}, points={:?}",
        job.phase, job.boundary
    );
}
