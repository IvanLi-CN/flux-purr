use super::*;

fn query(temperature_centi_c: i16, current_ma: u16, wifi_started: bool) -> BoundaryQuery {
    BoundaryQuery {
        temperature_centi_c,
        current_rtd_mv: None,
        current_request_mv: None,
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
        [(2_000, 7_200, 1_000, 3_000), (2_100, 10_000, 1_200, 10_000)]
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
fn lookup_does_not_invent_warmer_or_lower_current_observations() {
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
    assert_eq!(lookup(10_100, 3_000, true), None);
    assert_eq!(lookup(3_100, 2_000, true), Some(7_200));
    assert_eq!(lookup(3_101, 2_000, true), Some(7_200));
    assert_eq!(lookup(4_101, 2_000, true), Some(7_200));
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
        current_ma: 2_100,
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
fn transient_three_amp_probe_carries_through_its_compact_temperature_bucket() {
    let mut value = boundary();
    value.point_count = 0;
    assert!(value.retain(BoundaryPoint {
        elapsed_ms: 10_000,
        rtd_code: 1_500,
        rtd_mv: 1_000,
        vin_code: 1_000,
        vin_mv: 800,
        request_current_ma: 3_000,
        tracking_request_mv: 6_400,
        flat_request_mv: 0,
        tracking_only: true,
    }));
    assert_eq!(
        value.safe_request_mv(query(3_000, 3_000, true), |_| Some(3_000)),
        Some(6_400)
    );
    assert_eq!(
        value.safe_request_mv(query(3_101, 3_000, true), |_| Some(3_000)),
        Some(6_400)
    );
    assert_eq!(
        value.safe_request_mv(query(4_801, 3_000, true), |_| Some(3_000)),
        None
    );
}

#[test]
fn tracking_point_covers_real_compacted_bucket_gap_without_theoretical_fallback() {
    let mut value = boundary();
    value.point_count = 0;
    assert!(value.retain(BoundaryPoint {
        elapsed_ms: 10_000,
        rtd_code: 1_500,
        rtd_mv: 1_199,
        vin_code: 1_000,
        vin_mv: 1_500,
        request_current_ma: 3_000,
        tracking_request_mv: 16_100,
        flat_request_mv: 0,
        tracking_only: true,
    }));
    // This reproduces the live recalibration rows: the latest colder point
    // projects to 104.6 C while the device is at 118.0 C.
    assert_eq!(
        value.safe_request_mv(query(11_800, 3_000, true), |raw| match raw {
            1_199 => Some(10_460),
            _ => None,
        }),
        Some(16_100)
    );
}

#[test]
fn confirmed_tracking_point_promotes_the_next_observed_voltage() {
    let mut value = boundary();
    value.point_count = 0;
    for (rtd_mv, tracking_request_mv) in [(1_168, 11_900), (1_189, 13_600)] {
        assert!(value.retain(BoundaryPoint {
            elapsed_ms: 10_000,
            rtd_code: 1_500,
            rtd_mv,
            vin_code: 1_000,
            vin_mv: 800,
            request_current_ma: 3_000,
            tracking_request_mv,
            flat_request_mv: 0,
            tracking_only: true,
        }));
    }
    let mut query = query(8_950, 3_000, true);
    query.current_request_mv = Some(11_700);
    assert_eq!(
        value.safe_request_mv(query, |raw| match raw {
            1_168 => Some(8_900),
            1_189 => Some(9_900),
            _ => None,
        }),
        Some(11_900)
    );
    query.current_request_mv = Some(11_900);
    assert_eq!(
        value.safe_request_mv(query, |raw| match raw {
            1_168 => Some(8_900),
            1_189 => Some(9_900),
            _ => None,
        }),
        Some(13_600)
    );
    query.current_request_mv = Some(12_500);
    assert_eq!(
        value.safe_request_mv(query, |raw| match raw {
            1_168 => Some(8_900),
            1_189 => Some(9_900),
            _ => None,
        }),
        Some(13_600)
    );
}

#[test]
fn adjacent_rtd_quantization_edge_allows_the_next_observed_voltage() {
    let mut value = boundary();
    value.point_count = 0;
    assert!(value.retain(BoundaryPoint {
        elapsed_ms: 10_000,
        rtd_code: 2_900,
        rtd_mv: 1_307,
        vin_code: 1_800,
        vin_mv: 1_523,
        request_current_ma: 3_000,
        tracking_request_mv: 18_800,
        flat_request_mv: 0,
        tracking_only: true,
    }));
    let mut query = query(15_964, 3_000, true);
    query.current_rtd_mv = Some(1_306);
    assert_eq!(
        value.safe_request_mv(query, |raw| (raw == 1_307).then_some(16_000)),
        Some(18_800)
    );

    query.current_rtd_mv = Some(1_305);
    assert_eq!(
        value.safe_request_mv(query, |raw| (raw == 1_307).then_some(16_000)),
        Some(18_800)
    );

    query.current_rtd_mv = Some(1_304);
    assert_eq!(
        value.safe_request_mv(query, |raw| (raw == 1_307).then_some(16_000)),
        None
    );
}

#[test]
fn observed_three_amp_tracking_point_authorizes_its_temperature_window() {
    let mut value = boundary();
    value.point_count = 0;
    assert!(value.retain(BoundaryPoint {
        elapsed_ms: 10_000,
        rtd_code: 1_500,
        rtd_mv: 1_000,
        vin_code: 1_000,
        vin_mv: 1_500,
        request_current_ma: 3_000,
        tracking_request_mv: 18_000,
        flat_request_mv: 0,
        tracking_only: true,
    }));
    assert_eq!(
        value.safe_request_mv(query(3_000, 3_000, true), |_| Some(3_000)),
        Some(18_000)
    );
}

#[test]
fn a_temperature_ramp_outside_one_degree_is_not_equilibrium() {
    let mut window = EquilibriumWindow::default();
    for tick in 0..=200 {
        assert!(!window.observe(tick * 50, 10_000 + tick as i16));
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

#[test]
fn in_band_temperature_drift_is_allowed_after_ten_seconds() {
    let mut window = EquilibriumWindow::default();
    for tick in 0..200 {
        let temperature = 10_000 + (tick / 7) as i16;
        assert!(!window.observe(tick * 50, temperature));
    }
    assert!(window.observe(10_050, 10_030));
}

#[test]
fn in_band_temperature_response_to_safe_voltage_does_not_starve_equilibrium() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut equilibrium_started = false;
    for now_ms in (0..120_000).step_by(50) {
        let output = job.output();
        // The measured temperature changes by only 0.3 C when returning
        // from a boundary probe to the safe voltage. Every observation
        // remains inside the required 1 C equilibrium band.
        let temperature = if output.request_mv <= 5_600 {
            10_030
        } else {
            10_000
        };
        job.update(sample(now_ms, output, temperature));
        if job.phase == scan::ScanPhase::Equilibrium {
            equilibrium_started = true;
        }
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            now_ms,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
        if equilibrium_started {
            return;
        }
    }
    panic!("an in-band response must reach the equilibrium phase");
}

#[test]
fn adc_jitter_is_filtered_for_equilibrium_without_relaxing_cutoff() {
    let mut value = boundary();
    value.point_count = 0;
    value.source_max_mv = 12_000;
    value.advertised_max_mv = 12_000;
    let mut job = scan::SupplyScan::new(value).unwrap();
    for now_ms in (0..2_000_000).step_by(50) {
        let output = job.output();
        let raw_temperature = if (now_ms / 50) % 2 == 0 {
            10_533
        } else {
            10_799
        };
        job.update(sample(now_ms, output, raw_temperature));
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            now_ms,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
        if job.phase == scan::ScanPhase::Complete {
            assert!(job.boundary.point_count > 0);
            return;
        }
    }
    panic!("ADC-coupled temperature jitter must not starve a stable scan");
}

#[test]
fn warming_during_electrical_sweep_does_not_starve_voltage_probing() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    for now_ms in (0..60_000).step_by(50) {
        let output = job.output();
        job.update(sample(now_ms, output, 10_000));
        if job.phase == scan::ScanPhase::Sweep
            && output.request_ma == MIN_HEATED_CURRENT_MA
            && output.request_mv > 5_500
        {
            let warmed = job.update(sample(now_ms + 1_000, job.output(), 10_500));
            assert!(warmed.request_mv > 5_500);
            assert_eq!(warmed.request_ma, MIN_HEATED_CURRENT_MA);
            return;
        }
    }
    panic!("the simulated electrical sweep did not advance while warming");
}

#[test]
fn thermal_ramp_does_not_turn_a_flat_sample_into_a_cc_boundary() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    for now_ms in (0..60_000).step_by(50) {
        let output = job.output();
        job.update(sample(now_ms, output, 10_000));
        if job.phase == scan::ScanPhase::Sweep
            && output.request_ma == MIN_HEATED_CURRENT_MA
            && output.request_mv > 5_500
        {
            let mut warming = sample(now_ms + 1_000, job.output(), 10_500);
            warming.vin_mv = 1;
            let next = job.update(warming);
            assert_eq!(job.phase, scan::ScanPhase::Sweep);
            assert!(next.request_mv > output.request_mv);
            assert_eq!(next.request_ma, MIN_HEATED_CURRENT_MA);
            return;
        }
    }
    panic!("the simulated electrical sweep did not reach a warming probe");
}

#[test]
fn ambiguous_voltage_response_is_retried_before_advancing() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut inject_at = None;
    let mut injected = false;
    for now_ms in (0..60_000).step_by(50) {
        let output = job.output();
        let mut input = sample(now_ms, output, 10_000);
        if let Some(request_mv) = inject_at
            && output.request_mv == request_mv
            && !injected
        {
            input.vin_mv = input.vin_mv.saturating_sub(4);
            injected = true;
        }
        let previous_request_mv = output.request_mv;
        let next = job.update(input);
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            now_ms,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
        if injected {
            assert_eq!(next.request_mv, previous_request_mv);
            let retried = job.update(sample(now_ms + 1_000, next, 10_000));
            assert_eq!(retried.request_ma, next.request_ma);
            assert!(retried.request_mv > previous_request_mv);
            return;
        }
        if next.request_mv > previous_request_mv && previous_request_mv >= 5_500 {
            inject_at = Some(next.request_mv);
        }
    }
    panic!("the simulated electrical sweep did not reach an ambiguous response");
}

#[test]
fn persistent_ambiguous_response_keeps_a_conservative_boundary() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut ambiguous_request = None;
    let mut ambiguous_vin_mv = 0;
    let mut ambiguous_samples = 0;
    for now_ms in (0..120_000).step_by(50) {
        let output = job.output();
        let mut input = sample(now_ms, output, 10_000);
        if let Some(request_mv) = ambiguous_request
            && output.request_mv == request_mv
        {
            if ambiguous_samples == 0 {
                ambiguous_vin_mv = input.vin_mv.saturating_sub(4);
            } else {
                ambiguous_vin_mv = ambiguous_vin_mv.saturating_add(4);
            }
            input.vin_mv = ambiguous_vin_mv;
            ambiguous_samples += 1;
        }
        let next = job.update(input);
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            now_ms,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
        if ambiguous_samples >= 5 {
            assert!(matches!(
                job.phase,
                scan::ScanPhase::Equilibrium
                    | scan::ScanPhase::Sweep
                    | scan::ScanPhase::TemperatureSettle
            ));
            assert!(next.request_mv <= ambiguous_request.unwrap());
            return;
        }
        if next.request_mv > output.request_mv && output.request_mv >= 5_500 {
            ambiguous_request = Some(next.request_mv);
        }
    }
    panic!("the simulated sweep did not reach persistent ambiguity");
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
fn preflight_initial_load_collapse_still_fails_closed() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let mut baseline = sample(1_000, job.output(), 3_000);
    baseline.vin_mv = 1_000;
    assert!(!job.update(baseline).heater_enabled);
    let mut loaded = sample(2_000, job.output(), 3_000);
    loaded.vin_mv = 200;
    assert!(!job.update(loaded).heater_enabled);
    assert_eq!(job.failure, Some(scan::ScanFailure::SupplyUnstable));
}

#[test]
fn high_voltage_preflight_accepts_a_stable_source_above_the_scaled_floor() {
    let mut job = scan::SupplyScan::new(boundary()).unwrap();
    let baseline = sample(1_000, job.output(), 3_000);
    assert!(!job.update(baseline).heater_enabled);

    let mut loaded = sample(2_000, job.output(), 3_000);
    loaded.vin_mv = 700;
    assert!(!job.update(loaded).heater_enabled);

    let next = job.update(scan::ScanInput {
        now_ms: 3_000,
        ..loaded
    });
    assert_eq!(job.phase, scan::ScanPhase::Sweep);
    assert_eq!(next.request_mv, 5_500);
    assert_eq!(next.request_ma, MIN_HEATED_CURRENT_MA);
    assert_eq!(job.failure, None);
}

#[test]
fn scan_preflights_at_three_amp_then_discovers_each_thermal_stage() {
    let mut value = boundary();
    value.point_count = 0;
    value.source_max_mv = 12_000;
    value.advertised_max_mv = 12_000;
    let mut job = scan::SupplyScan::new(value).unwrap();
    for time in (0..2_000_000).step_by(50) {
        let output = job.output();
        job.update(sample(time, output, 10_000));
        if job.phase == scan::ScanPhase::Complete {
            assert!(job.boundary.is_valid());
            assert!(job.boundary.point_count > 0);
            let has_minimum_current = job.boundary.points[..job.boundary.point_count as usize]
                .iter()
                .any(|point| point.request_current_ma == MIN_HEATED_CURRENT_MA);
            let has_three_amp = job.boundary.points[..job.boundary.point_count as usize]
                .iter()
                .any(|point| point.request_current_ma == 3_000);
            assert!(has_minimum_current);
            assert!(has_three_amp);
            assert!(!job.output().heater_enabled);
            return;
        }
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            time,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
    }
    panic!("fake source scan did not terminate");
}

#[test]
fn scan_retains_each_sustainable_current_stage_with_raw_adc_samples() {
    let mut value = boundary();
    value.point_count = 0;
    value.source_max_mv = 12_000;
    value.advertised_max_mv = 12_000;
    let mut job = scan::SupplyScan::new(value).unwrap();
    for time in (0..2_000_000).step_by(50) {
        let output = job.output();
        job.update(sample(time, output, 10_000));
        if job.phase == scan::ScanPhase::Complete {
            let points = &job.boundary.points[..usize::from(job.boundary.point_count)];
            let mut stages = [false; 21];
            for point in points.iter().filter(|point| !point.tracking_only) {
                assert!(point.rtd_code > 0);
                assert!(point.rtd_mv > 0);
                assert!(point.vin_code > 0);
                assert!(point.vin_mv > 0);
                let index = usize::from(point.request_current_ma - 1_000) / 100;
                stages[index] = true;
            }
            assert!(stages[20], "missing full 3 A boundary stage");
            assert!(stages[..20].iter().any(|stage| *stage));
            return;
        }
        assert_ne!(
            job.phase,
            scan::ScanPhase::Failed,
            "failure={:?} time={} request={} current={} points={}",
            job.failure,
            time,
            job.output().request_mv,
            job.output().request_ma,
            job.boundary.point_count
        );
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
            let safe_ceiling_reached = points.iter().any(|point| {
                point.tracking_request_mv.saturating_add(100) >= job.boundary.source_max_mv
            });
            assert!(job.cutoff_reached || temperature >= 220.0 || safe_ceiling_reached);
            assert!(points.iter().any(|p| p.tracking_only));
            assert!(
                points
                    .iter()
                    .any(|p| p.request_current_ma == MIN_HEATED_CURRENT_MA)
            );
            assert!(points.iter().any(|p| p.request_current_ma == 3_000));
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
