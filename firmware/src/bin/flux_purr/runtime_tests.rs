use super::*;
use flux_purr_firmware::thermal_tuning::ThermalTuningPhase;

include!("calibration_cutoff_fixture.rs");

#[test]
fn unchanged_heater_request_recovers_after_an_asynchronous_pd_disarm() {
    struct Pwm {
        applied: u16,
    }
    impl embedded_hal::pwm::ErrorType for Pwm {
        type Error = core::convert::Infallible;
    }
    impl embedded_hal::pwm::SetDutyCycle for Pwm {
        fn max_duty_cycle(&self) -> u16 {
            100
        }
        fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
            self.applied = duty;
            Ok(())
        }
    }

    // The PD task can clear the pin between front-panel control frames without
    // updating the front panel's cached last output.
    let mut pwm = Pwm { applied: 0 };
    let mut last_percent = 0;
    apply_permitted_heater_duty(&mut pwm, 100, &mut last_percent, true).unwrap();
    assert_eq!(pwm.applied, 100);
    pwm.applied = 0;
    assert_eq!(last_percent, 100);

    apply_permitted_heater_duty(&mut pwm, 100, &mut last_percent, true).unwrap();
    assert_eq!(pwm.applied, 100, "a fresh permit must restore full Warmup");
    apply_permitted_heater_duty(&mut pwm, 100, &mut last_percent, false).unwrap();
    assert_eq!(pwm.applied, 0, "a missing permit must still fail closed");
    assert_eq!(last_percent, 0);
    apply_permitted_heater_duty(&mut pwm, 100, &mut last_percent, true).unwrap();
    assert_eq!(pwm.applied, 100);
}

#[test]
fn calibration_trace_uses_confirmed_full_conduction_before_output_apply() {
    assert_eq!(calibration_trace_duty_percent(true, true, true, 0), 100);
    assert_eq!(calibration_trace_duty_percent(true, true, true, 37), 100);
    assert_eq!(calibration_trace_duty_percent(true, true, false, 0), 0);
    assert_eq!(calibration_trace_duty_percent(true, false, true, 0), 0);
    assert_eq!(calibration_trace_duty_percent(false, true, true, 0), 0);
    assert_eq!(calibration_trace_duty_percent(false, false, false, 43), 43);
}

#[test]
fn vin_adc_range_covers_the_board_and_confirmed_pps_input() {
    // Espressif specifies 1750mV at 6dB and 3100mV at 11dB. The actual
    // board divider, rather than the RTD range, defines VIN's requirement.
    let maximum_mv = match adc1_attenuation_db(AdcCalibrationChannel::Vin) {
        6 => 1_750,
        11 => 3_100,
        attenuation => panic!("unverified VIN attenuation {attenuation}"),
    };
    for input_mv in [5_000, 12_000, 20_000, 21_000, 21_300, 28_000] {
        let adc_mv = vin_adc_mv_for_input_mv(input_mv);
        assert!(
            adc_mv <= maximum_mv,
            "{input_mv}mV input puts {adc_mv}mV on VIN ADC, above its {maximum_mv}mV range"
        );
    }
}

#[test]
fn heater_coast_releases_on_smooth_cooling_at_fast_control_cadence() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    config.settings.temp_filter_alpha_permille = 750;
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 240;
    point.hold_on_centi_c = 11;
    point.hold_exit_centi_c = 160;
    point.hold_entry_centi_c = 150;
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    for interval_ms in [20, 50, 100] {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 240;
        controller.phase = HeaterControlPhase::Hold;
        controller.hold_coast_active = true;
        controller.filtered_temp_c = Some(240.5);
        controller.previous_measured_temp_c = Some(240.5);
        controller.filtered_slope_c_per_profile_tick = -0.5;
        let mut recovered = false;
        for now_ms in (interval_ms..=3_000).step_by(interval_ms as usize) {
            let snapshot = controller.update_at(
                240,
                240.5 - now_ms as f32 * 0.0005,
                true,
                Some(profile),
                now_ms,
            );
            recovered |= !snapshot.coast_active && snapshot.duty_percent > 0;
        }
        assert!(
            recovered,
            "coast never recovers at {interval_ms} ms cadence"
        );
    }
}

#[test]
fn heater_profile_uses_calibrated_cooling_for_braking_and_early_recovery() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 240;
    point.brake_distance_centi_c = 150;
    point.warmup_reenter_centi_c = 1000;
    point.hold_power_permille = 1000;
    point.overshoot_cutoff_centi_c = 300;
    point.hold_off_centi_c = 100;
    config.points[1..].fill(None);
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    let input = ThermalPlantRuntimeInput {
        allow_integral: true,
        target_temp_c: 240,
        measured_temp_c: 237.0,
        measurement_accepted: true,
        ambient_temp_c: 25.0,
        heater_enabled: true,
        // Observed rise exceeds the nominal calibrated rise, so live model
        // prediction must still brake early after candidate correction.
        max_power_mw: 25_000.0,
        now_ms: 50,
        model: flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 100.0,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 10_000.0,
            transport_delay_ms: 2000,
        },
    };
    let mut calibrated = HeaterController::new();
    calibrated.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    calibrated.last_target_temp_c = 240;
    calibrated.filtered_temp_c = Some(236.9);
    calibrated.previous_measured_temp_c = Some(236.9);
    calibrated.filtered_slope_c_per_profile_tick = 2.0;
    let mut legacy = calibrated;
    let plain = legacy.update_at(240, 237.0, true, Some(profile), 50);
    let (modelled, _) = calibrated.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        input,
        Some(profile),
    );
    assert_eq!(plain.phase, HeaterControlPhase::Warmup);
    assert_eq!(modelled.phase, HeaterControlPhase::Approach);

    calibrated.phase = HeaterControlPhase::Hold;
    calibrated.hold_coast_active = true;
    calibrated.filtered_temp_c = Some(240.7);
    calibrated.previous_measured_temp_c = Some(240.7);
    calibrated.filtered_slope_c_per_profile_tick = -0.5;
    let mut recovery = None;
    for tick in 1..=10 {
        let (snapshot, _) = calibrated.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            ThermalPlantRuntimeInput {
                measured_temp_c: 240.7 - tick as f32 * 0.025,
                now_ms: 50 + tick * 50,
                ..input
            },
            Some(profile),
        );
        if !snapshot.coast_active && snapshot.duty_percent > 0 {
            recovery = Some(snapshot);
            break;
        }
    }
    let recovery =
        recovery.expect("modelled loss must recover before the plate falls below target");
    assert!(recovery.error_c < 0.0);
    // The first positive recovery includes the existing entry blend; it does
    // not need to jump immediately to twenty percent.
    assert!(
        recovery.duty_percent > 0 && recovery.duty_percent < 100,
        "{recovery:?}"
    );
}

#[test]
fn heater_cooling_brake_does_not_double_count_loss_in_observed_net_slope() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 240;
    point.brake_distance_centi_c = 150;
    point.warmup_reenter_centi_c = 1000;
    point.hold_entry_centi_c = 150;
    point.approach_lead_ticks = 0;
    config.points[1..].fill(None);
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    let mut controller = HeaterController::new();
    controller.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    controller.last_target_temp_c = 240;
    controller.filtered_temp_c = Some(231.7);
    controller.previous_measured_temp_c = Some(231.7);
    controller.filtered_slope_c_per_profile_tick = 1.0;
    let input = ThermalPlantRuntimeInput {
        allow_integral: true,
        target_temp_c: 240,
        measured_temp_c: 231.725,
        measurement_accepted: true,
        ambient_temp_c: 25.0,
        heater_enabled: true,
        max_power_mw: 100_000.0,
        now_ms: 50,
        model: flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 139.1813,
            radiation_mw_per_k4: 4.397_830_7e-7,
            thermal_capacity_mj_per_c: 13_996.655,
            transport_delay_ms: 1950,
        },
    };
    let (snapshot, _) = controller.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        input,
        Some(profile),
    );
    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
}

fn pps_source_capability(min_mv: u16, max_mv: u16, max_ma: u16) -> u32 {
    (0b11 << 30)
        | (u32::from(max_mv / 100) << 17)
        | (u32::from(min_mv / 100) << 8)
        | u32::from(max_ma / 50)
}

fn fixed_source_capability(voltage_mv: u16, max_ma: u16) -> u32 {
    (u32::from(voltage_mv / 50) << 10) | u32::from(max_ma / 10)
}
const RUNTIME_IMPLEMENTATION: &str = concat!(
    include_str!("support.rs"),
    include_str!("eeprom_snapshot.rs"),
    include_str!("frontpanel.rs"),
    include_str!("thermal.rs"),
    include_str!("fan.rs"),
    include_str!("pd_control.rs"),
    include_str!("adc.rs"),
    include_str!("pd_protocol.rs"),
    include_str!("eeprom.rs"),
    include_str!("power.rs"),
    include_str!("tasks.rs"),
    include_str!("watchdog.rs"),
    include_str!("control_plane.rs"),
    include_str!("lan.rs"),
    include_str!("display_io.rs"),
    include_str!("pd_service.rs"),
    include_str!("boot.rs"),
    include_str!("runtime_loop.rs"),
);

#[test]
fn watchdog_feed_gate_requires_both_runtime_and_pd_progress() {
    let mut gate = WatchdogFeedGate::default();

    assert!(gate.observe(1, 0, 0, true));
    assert!(!gate.observe(1, 0, 0, true));
    assert!(gate.observe(1, 1, 1, true));
    assert!(!gate.observe(1, 2, 1, true));
    assert!(gate.observe(1, 2, 2, true));
    assert!(!gate.observe(1, 3, 2, true));
    assert!(!gate.observe(1, 4, 2, true));
}

#[test]
fn watchdog_feed_gate_uses_runtime_progress_when_pd_service_is_unavailable() {
    let mut gate = WatchdogFeedGate::default();

    assert!(gate.observe(1, 0, 0, false));
    assert!(gate.observe(1, 1, 0, false));
    assert!(gate.observe(1, 2, 0, false));
    assert!(!gate.observe(1, 2, 1, false));
}

#[test]
fn watchdog_feed_gate_uses_boot_progress_until_runtime_starts() {
    let mut gate = WatchdogFeedGate::default();

    assert!(!gate.observe(0, 0, 0, true));
    assert!(gate.observe(1, 0, 0, true));
    assert!(!gate.observe(1, 0, 0, true));
    assert!(gate.observe(2, 0, 0, true));
    assert!(!gate.observe(2, 1, 0, true));
    assert!(gate.observe(2, 1, 1, true));
}

#[test]
fn watchdog_configures_clock_derived_timeout_before_rtos_handoff() {
    let watchdog = include_str!("watchdog.rs");
    let boot = include_str!("boot.rs");
    let configure = watchdog
        .split("pub(crate) fn configure_watchdog_before_rtos")
        .nth(1)
        .expect("watchdog timeout configuration must remain explicit");
    let task = watchdog
        .split("pub(crate) async fn watchdog_task")
        .nth(1)
        .expect("watchdog supervisor task must remain present");
    let start_rtos = boot
        .split("pub(crate) fn start_rtos")
        .nth(1)
        .expect("RTOS startup must remain present");

    assert!(configure.contains("watchdog.set_timeout("));
    assert!(!task.contains("set_timeout("));
    assert!(
        start_rtos.find("configure_watchdog_before_rtos(timg0.wdt)")
            < start_rtos.find("esp_rtos::start(timg0.timer0)")
    );
}

#[test]
fn runtime_usb_transport_uses_yielding_nonblocking_response_packets() {
    let support = include_str!("support.rs");
    let control_plane = include_str!("control_plane.rs");
    let runtime_loop = include_str!("runtime_loop.rs");

    assert!(support.contains("UsbSerialJtag<'static, Blocking>"));
    assert!(control_plane.contains("async fn usb_write_response_bytes"));
    assert!(control_plane.contains("embassy_futures::yield_now().await"));
    assert!(control_plane.contains("USB_CONTROL_RESPONSE_TIMEOUT_MS"));
    assert!(support.contains("USB_CONTROL_TX_PACKET_BUDGET"));
    assert!(!support.contains("UsbSerialJtag::new(usb_device).into_async()"));
    assert!(!support.contains("self.inner.write(bytes)"));
    assert!(!control_plane.contains("USB_CONTROL_TX_RETRY_LIMIT"));
    assert!(!control_plane.contains("wait_for_tx_progress"));
    let writer = control_plane
        .split("struct UsbResponseWriter")
        .nth(1)
        .expect("response writer state machine must remain present");
    assert!(writer.contains("at most one non-blocking USB packet per call"));
    assert!(control_plane.contains("Ok(false) | Err(UsbTxError::WouldBlock)"));
    assert!(control_plane.contains("DeferredPersistenceLogSink"));
    assert!(control_plane.contains("USB_TRANSPORT_FAULT_MARKER"));
    assert!(control_plane.contains("usb_mutating_request_id"));
    assert!(runtime_loop.contains("response_pending"));
    assert!(runtime_loop.contains("if usb_input.response_pending"));
    assert!(runtime_loop.contains("USB_CONTROL_TX_PACKET_BUDGET"));
    assert!(runtime_loop.contains("persistence_log_pending"));
    assert!(runtime_loop.contains("!persistence_log_pending"));
    assert!(control_plane.contains("fn is_pending(&self) -> bool"));
}
const FIRMWARE_ENTRYPOINT: &str = include_str!("../flux_purr.rs");

#[test]
fn pd_startup_and_runtime_share_one_timestamp_epoch() {
    let started_at_ms = 42_000;
    assert_eq!(pd_runtime_elapsed_ms(started_at_ms, started_at_ms), 0);
    assert_eq!(
        pd_runtime_elapsed_ms(started_at_ms, started_at_ms + 400),
        400
    );
    assert_eq!(pd_runtime_elapsed_ms(started_at_ms, started_at_ms - 1), 0);

    let protocol_now = PdTimestamp::from_millis(42_400);
    assert_eq!(protocol_now.as_millis(), 42_400);
}

#[test]
fn manual_pps_missing_target_disarms_and_records_invalid_voltage() {
    let mut manual_pps = ManualPpsState {
        enabled: true,
        ..ManualPpsState::default()
    };

    assert_eq!(
        require_manual_pps_target(&mut manual_pps),
        Err(ManualPpsError::InvalidVoltage)
    );
    assert!(!manual_pps.enabled);
    assert_eq!(manual_pps.target_mv, None);
    assert_eq!(manual_pps.target_ma, None);
    assert_eq!(manual_pps.error, Some(ManualPpsError::InvalidVoltage));
    assert!(manual_pps.consume_automatic_restore_pending());
}

#[test]
fn pd_service_does_not_feed_control_elapsed_time_into_protocol_deadlines() {
    let source = RUNTIME_IMPLEMENTATION;
    let implementation = source
        .split("#[cfg(test)]\nmod tests")
        .next()
        .expect("implementation must precede tests");

    assert!(
        !implementation.contains("read_pd_snapshot(&mut pd_i2c, &mut pd_port, elapsed_ms)"),
        "PD protocol deadlines must not receive the relative control-loop clock"
    );
}

#[test]
fn runtime_pd_snapshot_refreshes_after_display_consumes_a_notification() {
    let runtime_loop = include_str!("runtime_loop.rs");
    let snapshot = runtime_loop
        .split("pub(crate) fn runtime_apply_pd_snapshot")
        .nth(1)
        .and_then(|source| {
            source
                .split("pub(crate) async fn runtime_process_usb_snapshot_line")
                .next()
        })
        .unwrap();
    let refresh = snapshot
        .find("state.power_state_subscription.try_get()")
        .unwrap();
    let observation = snapshot
        .find("power_state_observation(power_state)")
        .unwrap();
    assert!(refresh < observation);
    assert!(snapshot[refresh..observation].contains("state.power_state = latest"));
}

#[test]
fn pd_service_preempts_lower_priority_application_work() {
    let source = RUNTIME_IMPLEMENTATION;
    let support = include_str!("support.rs");
    let pd_service = include_str!("pd_service.rs");
    let runtime_loop = include_str!("runtime_loop.rs");
    let boot = include_str!("boot.rs");

    assert!(
        pd_service.contains("#[embassy_executor::task]\nasync fn pd_service_task"),
        "PD policy must have a dedicated Embassy task"
    );
    assert!(
        boot.contains("start_pd_realtime_executor(software_interrupts.software_interrupt2)")
            && boot.contains("InterruptExecutor::new(software_interrupt)")
            && boot.contains("executor.start(Priority::Priority3)")
            && boot.contains("buzzer_realtime_executor.start(Priority::Priority2)")
            && pd_service.contains("PD_REALTIME_EXECUTOR_STORAGE")
            && pd_service.contains("spawn_pd_service(\n    spawner: embassy_executor::SendSpawner"),
        "PD protocol must preempt ordinary work and the priority-2 buzzer"
    );
    assert!(
        pd_service
            .contains("select_pd_service_work(*pending, PD_SERVICE_COMMANDS.try_receive().ok())")
            && pd_service.contains("runtime.poll(i2c, PdTimestamp::now()).await"),
        "PD task must poll after a bounded command batch"
    );
    assert!(
        !pd_service.contains("PD_SERVICE_TICK_SIGNAL")
            && !pd_service.contains("async fn run_pd_service_tick")
            && pd_service.contains("pd_service_delay_ms(")
            && pd_service.contains("EmbassyTimer::after_millis(delay_ms).await"),
        "the PD task must own its bounded cadence timer"
    );
    let pd_task = pd_service
        .split("async fn pd_service_task")
        .nth(1)
        .and_then(|source| source.split("pub(crate) fn spawn_pd_service").next())
        .expect("PD task body must remain present");
    assert!(
        pd_task.contains("EmbassyTimer::after_millis(delay_ms).await"),
        "the PD task must yield between bounded service turns"
    );
    assert!(
        !source.contains("Spawner::for_current_executor")
            && include_str!("boot.rs").contains("initialize_boot_pd(")
            && include_str!("boot.rs").contains("boot system is available for PD initialization")
            && FIRMWARE_ENTRYPOINT.contains("runtime::run(spawner).await"),
        "the main task must start PD during direct boot"
    );
    assert!(
        boot.contains("pub(crate) async fn initialize_boot_pd(")
            && boot.contains("spawn_pd_service(system.pd_realtime_spawner")
            && pd_service.contains("let bus = unsafe { &*(bus_address as *const SharedI2cBus) }")
            && pd_service.contains("esp_hal::system::Cpu::current()")
            && support.contains("pub(crate) fn into_bus_address(mut self) -> usize"),
        "PD startup must keep the non-Send async bus on CPU0 while using its own executor"
    );
    assert!(
        !runtime_loop.contains("runtime_service_pd")
            && !runtime_loop.contains("runtime.poll(")
            && runtime_loop.contains("runtime_apply_pd_snapshot"),
        "the front-panel loop must not own PD polling"
    );
    assert!(
        support.contains("pub(crate) type I2c<'a> =")
            && support
                .contains("SharedI2cDevice<'a, CriticalSectionRawMutex, HalI2c<'static, Async>>")
            && support.contains("AsyncMutex<CriticalSectionRawMutex, HalI2c<'static, Async>>")
            && support.contains("AsyncMutex<CriticalSectionRawMutex")
            && source.contains(".with_scl(tokens.pd_scl)")
            && source.contains(".into_async()")
            && support.contains("pub(crate) struct PdI2c<'a>")
            && source.contains("let pd_task_i2c = PdI2c::new(i2c_bus)"),
        "EEPROM and PD must share the native async I2C driver"
    );
    assert!(
        !support.contains("BlockingAsync")
            && !support.contains("type I2c<'a, MODE = Blocking>")
            && !support.contains("BlockingMutex<CriticalSectionRawMutex, HalI2c"),
        "shared I2C access must not wrap the bus in a blocking async adapter"
    );
}

#[test]
fn pd_service_cadence_compensates_for_work_and_bus_contention() {
    assert_eq!(pd_service_delay_ms(0, true), PD_SERVICE_TICK_MS);
    assert_eq!(pd_service_delay_ms(3, true), 2);
    assert_eq!(pd_service_delay_ms(PD_SERVICE_TICK_MS, true), 1);
    assert_eq!(pd_service_delay_ms(20, true), 1);
    assert_eq!(pd_service_delay_ms(0, false), PD_SERVICE_BUSY_RETRY_MS);
}

#[test]
fn startup_pd_wait_uses_a_normal_executor_timer() {
    let boot = include_str!("boot.rs");
    let pd_service = include_str!("pd_service.rs");

    assert!(
        boot.contains("EmbassyTimer::after_millis(PD_SERVICE_TICK_MS)"),
        "boot must use a normal-executor timer for bounded startup progress"
    );
    assert!(
        !boot.contains("PD_STARTUP_TICK_SIGNAL") && !pd_service.contains("PD_STARTUP_TICK_SIGNAL"),
        "startup progress must not depend on a resettable Signal wait queue"
    );
    assert!(
        !pd_service.contains("PD_SERVICE_TICK_SIGNAL"),
        "the dedicated PD task must not depend on cross-executor signaling"
    );
}

#[test]
fn pd_service_never_waits_for_the_shared_i2c_bus() {
    let pd_service = include_str!("pd_service.rs");
    let support = include_str!("support.rs");

    assert!(
        support.contains("pub(crate) fn try_acquire(&mut self) -> bool"),
        "PD bus acquisition must be an immediate try-lock"
    );
    assert!(
        pd_service.contains("let acquired_bus = i2c.try_acquire()")
            && pd_service.contains("if acquired_bus")
            && pd_service.contains("i2c.release()"),
        "a busy EEPROM transaction must make PD skip this turn and retry later"
    );
    assert!(
        !pd_service.contains("i2c.lock().await") && !support.contains("self.bus.lock().await"),
        "the PD path must not await the shared I2C mutex"
    );
}

#[test]
fn pd_i2c_timeout_stays_short_while_transport_recovery_is_rate_limited() {
    assert_eq!(I2C_TRANSACTION_TIMEOUT_MS, 10);
    assert_eq!(FUSB302B_TRANSPORT_RECOVERY_BACKOFF_MS, 50);
    assert!(fusb302b_transport_retry_due(None, 1_000));
    assert!(!fusb302b_transport_retry_due(Some(1_050), 1_049));
    assert!(fusb302b_transport_retry_due(Some(1_050), 1_050));
}

#[test]
fn fusb302b_heater_observation_retains_confirmed_contract_during_negotiation() {
    let contract = Contract::observed(ContractKind::Pps, 20_000, 3_000);

    assert!(fusb302b_status_confirms_active_contract(
        SinkPhase::Ready,
        contract,
        FUSB302B_STATUS0_VBUSOK,
    ));
    assert!(fusb302b_status_confirms_active_contract(
        SinkPhase::WaitingForPsRdy,
        contract,
        FUSB302B_STATUS0_VBUSOK,
    ));
    assert!(fusb302b_status_confirms_active_contract(
        SinkPhase::WaitingForAccept,
        contract,
        FUSB302B_STATUS0_VBUSOK,
    ));
    assert!(!fusb302b_status_confirms_active_contract(
        SinkPhase::Ready,
        Contract::none(),
        FUSB302B_STATUS0_VBUSOK,
    ));
    assert!(!fusb302b_status_confirms_active_contract(
        SinkPhase::Ready,
        contract,
        0,
    ));
}

#[test]
fn fusb302b_observation_keeps_the_physical_status_byte_and_projects_pd_active() {
    let status0 = FUSB302B_STATUS0_VBUSOK;
    let projected = fusb302b_status_projection(status0);

    assert_eq!(status0, 1 << 7);
    assert!(!Status::from_register(status0).pd_active);
    assert!(projected.pd_active);
}

#[test]
fn pending_pd_terminal_model_covers_confirmation_failure_and_transition_waits() {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
    let request = PdContractRequest::pps(17_500, 3_000).unwrap();
    let contract = source.select_exact_contract(request).unwrap();
    let active = ConfirmedActiveContract::from_private_contract(contract, source).unwrap();
    let observation = PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: active.operating_current_ma,
        contract_voltage_mv: Some(active.voltage_mv),
        contract,
    };
    let context = |phase, observation, source_capabilities| PdServiceTerminalContext {
        phase,
        request_timed_out: false,
        request_rejected: false,
        diagnostic_request_timed_out: false,
        refresh_pending: false,
        observation,
        source_capabilities,
    };

    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::Ready, Some(observation), Some(source)),
            PendingPdOperation::Contract { request },
        ),
        Some(TicketOutcome::Confirmed(active)),
    );
    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::WaitingForPsRdy, Some(observation), Some(source)),
            PendingPdOperation::Contract { request },
        ),
        None,
    );
    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::Fault, None, Some(source)),
            PendingPdOperation::Contract { request },
        ),
        Some(TicketOutcome::TransportFault),
    );
    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::Ready, None, Some(SourceCapabilities::empty())),
            PendingPdOperation::Contract { request },
        ),
        Some(TicketOutcome::Rejected),
    );
    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::Ready, Some(observation), Some(source)),
            PendingPdOperation::Idle,
        ),
        None,
    );
    assert_eq!(
        pending_terminal_outcome(
            context(SinkPhase::Ready, None, Some(source)),
            PendingPdOperation::Refresh,
        ),
        Some(TicketOutcome::CapabilitiesRefreshed),
    );
}

#[test]
fn automatic_pd_ticket_confirmation_requires_the_requested_contract() {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
    let request = PdContractRequest::pps(17_500, 3_000).unwrap();
    let contract = source.select_exact_contract(request).unwrap();
    let active = ConfirmedActiveContract::from_private_contract(contract, source).unwrap();

    assert!(ticket_outcome_confirms_request(
        TicketOutcome::Confirmed(active),
        request,
    ));
    assert!(!ticket_outcome_confirms_request(
        TicketOutcome::Confirmed(active),
        PdContractRequest::pps(18_000, 3_000).unwrap(),
    ));
    assert!(!ticket_outcome_confirms_request(
        TicketOutcome::TimedOut,
        request,
    ));
}

#[test]
fn pd_snapshot_authorization_expires_after_the_service_heartbeat_window() {
    assert!(pd_snapshot_is_fresh(1_000, 1_000 + PD_SNAPSHOT_MAX_AGE_MS));
    assert!(!pd_snapshot_is_fresh(1_000, 1_001 + PD_SNAPSHOT_MAX_AGE_MS));
}

#[test]
fn pd_snapshot_and_pwm_paths_fail_closed_without_fresh_status() {
    let pd_service = include_str!("pd_service.rs");
    let support = include_str!("support.rs");
    let pd_task = pd_service
        .split("async fn pd_service_task")
        .nth(1)
        .and_then(|source| source.split("pub(crate) fn spawn_pd_service").next())
        .expect("PD task body must remain present");

    assert!(pd_service.contains("runtime.vbus_status_observed()"));
    assert!(pd_service.contains("let observation = pd_status_observation(runtime);"));
    assert!(
        !pd_service.contains("read_status().await.ok()?"),
        "the published observation must reuse the current poll's VBUS sample"
    );
    assert!(pd_service.contains("publish_pd_snapshot(runtime, observation)"));
    assert!(
        pd_service.contains("publish_pd_service_turn(runtime, None, *pending, None, false, false)")
    );
    assert!(pd_service.contains("if record_heartbeat"));
    assert!(!pd_task.contains("record_pd_heartbeat"));

    let permit_check = support
        .split("fn set_duty_cycle(&mut self, duty: u16)")
        .nth(1)
        .and_then(|source| source.split("fn max_duty_cycle").next())
        .unwrap_or_else(|| {
            support
                .split("fn set_duty_cycle(&mut self, duty: u16)")
                .nth(1)
                .expect("heater PWM gate must remain present")
        });
    assert!(
        permit_check
            .find("HEATER_PWM_STORAGE.lock")
            .is_some_and(|lock| { permit_check[lock..].contains("PD_HEATER_PERMIT.load") })
    );
}

#[test]
fn terminal_disarm_does_not_block_runtime_on_a_pd_ticket() {
    let eeprom = include_str!("eeprom.rs");
    let tasks = include_str!("tasks.rs");
    let disarm = eeprom
        .split("pub(crate) fn disarm_pending_thermal_plant_output")
        .nth(1)
        .and_then(|source| {
            source
                .split("pub(crate) fn terminal_idle_contract_confirmed")
                .next()
        })
        .expect("terminal disarm helper must remain present");

    assert!(
        !disarm.contains("wait_for_ticket"),
        "runtime-loop disarm must poll ticket completion instead of awaiting the PD actor"
    );
    assert!(
        eeprom.contains("pd_port.try_take_ticket(ticket)"),
        "runtime-loop disarm must use the non-blocking ticket facade"
    );
    assert!(
        !tasks.contains("wait_for_ticket"),
        "runtime heater control must never await the PD actor"
    );
}

#[test]
fn watchdog_is_enabled_before_frontpanel_runtime_starts() {
    let watchdog = include_str!("watchdog.rs");
    let boot = include_str!("boot.rs");
    let run = boot
        .split("pub(crate) async fn run(spawner: Spawner)")
        .nth(1)
        .expect("runtime entrypoint must remain present");

    assert!(watchdog.contains("WATCHDOG_ENABLED.store(1, Ordering::Release)"));
    assert!(run.find("spawn_watchdog(spawner, watchdog)") < run.find("arm_watchdog().await"));
    assert!(run.find("arm_watchdog().await") < run.find("init_runtime_heap()"));
    assert_eq!(boot.matches("arm_watchdog().await").count(), 1);
    assert!(boot.contains("record_boot_heartbeat();"));
}

#[test]
fn buzzer_cadence_stays_with_the_realtime_owner() {
    let tasks = include_str!("tasks.rs");
    let boot = include_str!("boot.rs");
    let buzzer_wait = tasks
        .split("pub(crate) async fn wait_for_buzzer_wake")
        .nth(1)
        .and_then(|source| {
            source
                .split("#[embassy_executor::task]\npub(crate) async fn run_buzzer_task")
                .next()
        })
        .expect("buzzer wake function must remain present");
    assert!(
        !tasks.contains("run_buzzer_tick_task")
            && !tasks.contains("BUZZER_TICK_SIGNAL")
            && buzzer_wait.contains("EmbassyTimer::after_millis(delay_ms)"),
        "buzzer cadence must not add a normal-executor timer task"
    );
    assert!(!boot.contains("run_buzzer_tick_task"));
}

#[test]
fn boot_initialization_does_not_block_the_normal_executor() {
    let boot = include_str!("boot.rs");
    assert!(
        !boot.contains("embassy_futures::block_on"),
        "boot must not block the normal executor while PD service tasks are running"
    );
    let normalized_entrypoint: String = FIRMWARE_ENTRYPOINT.split_whitespace().collect();
    assert!(
        normalized_entrypoint.contains("runtime::run(spawner).await;")
            && boot.contains("pub(crate) async fn run(spawner: Spawner)"),
        "the main task must own the direct boot sequence"
    );
}

#[test]
fn front_panel_cannot_service_pd_or_borrow_the_pd_bus() {
    let source = RUNTIME_IMPLEMENTATION;
    let runtime_loop = include_str!("runtime_loop.rs");
    let display_io = include_str!("display_io.rs");
    let eeprom = include_str!("eeprom.rs");

    for legacy_api in [
        "read_pd_snapshot(",
        "read_pd_capabilities_snapshot(",
        "submit_pd_fixed_voltage(",
        "submit_pd_adjustable_voltage(",
        "run_network_operation_with_snapshot(",
        "PdSnapshotAdcContext",
        "apply_pd_snapshot_during_",
    ] {
        assert!(
            !source.contains(legacy_api),
            "front-panel PD boundary must not retain legacy API: {legacy_api}"
        );
    }

    assert_eq!(
        source.matches("runtime.poll(").count(),
        1,
        "only the dedicated PD task may poll the FUSB302B runtime"
    );
    assert!(
        !runtime_loop.contains("I2c<'")
            && !runtime_loop.contains("Fusb302::new")
            && !display_io.contains("I2c<'")
            && !display_io.contains("Fusb302::new")
            && !eeprom.contains("Fusb302::new")
            && !eeprom.contains("pd_port: &mut PdPort"),
        "front-panel modules must not expose FUSB register access or mutable PD ownership"
    );
}

#[test]
fn runtime_display_cannot_consume_the_thermal_power_subscription() {
    let assembly = include_str!("runtime_assembly.rs");
    let runtime = include_str!("runtime_loop.rs");
    assert!(assembly.contains("let display_power_state_subscription = pd_port"));
    assert!(runtime.contains("&mut state.display_power_state_subscription"));
    assert!(runtime.contains("state.power_state_subscription.changed()"));
}

#[test]
fn fusb302b_received_resets_do_not_request_cc_reinitialization() {
    assert_eq!(
        fusb302b_received_reset_action(FUSB302B_INTERRUPTA_SOFT_RESET),
        Some(Fusb302bReceivedResetAction::AcceptAndWaitForSourceCapabilities)
    );
    assert_eq!(
        fusb302b_received_reset_action(FUSB302B_INTERRUPTA_HARD_RESET),
        Some(Fusb302bReceivedResetAction::WaitForSourceCapabilities)
    );
    assert_eq!(fusb302b_received_reset_action(0), None);
}

#[test]
fn fusb302b_pps_transport_uses_pd30_revision() {
    assert!(RUNTIME_IMPLEMENTATION.contains(
        "pub(crate) const fn fusb302b_phy_config(auto_goodcrc: bool) -> PhyConfig {\n    PhyConfig {"
    ));
    assert!(RUNTIME_IMPLEMENTATION.contains("pd_revision: PdRevision::Rev30"));
    assert!(!RUNTIME_IMPLEMENTATION.contains("PdHeaderRevision"));
}

#[test]
fn fusb302b_recovery_flushes_only_the_receive_fifo() {
    assert_eq!(fusb302b_receive_fifo_flush_value(0), 0b0000_0100);
    assert_eq!(fusb302b_receive_fifo_flush_value(0xff), 0b0111_0111);
    assert_eq!(fusb302b_transmit_fifo_flush_value(0), 0b0100_0000);
    assert_eq!(fusb302b_transmit_fifo_flush_value(0xff), 0b0110_1110);
}

#[test]
fn fusb302b_settled_sink_polarity_decodes_only_sink_states() {
    assert_eq!(
        fusb302b_settled_sink_polarity(FUSB302B_TOGSS_SNK_CC1),
        Some(1)
    );
    assert_eq!(
        fusb302b_settled_sink_polarity(FUSB302B_TOGSS_SNK_CC2),
        Some(2)
    );
    assert_eq!(fusb302b_settled_sink_polarity(0), None);
    assert_eq!(fusb302b_settled_sink_polarity(0b0001_1000), None);
}

#[test]
fn fusb302b_detach_requires_the_vbusok_transition_interrupt() {
    assert!(fusb302b_vbus_detach_was_reported(
        FUSB302B_INTERRUPT_VBUSOK,
        0,
    ));
    assert!(!fusb302b_vbus_detach_was_reported(0, 0));
    assert!(!fusb302b_vbus_detach_was_reported(
        FUSB302B_INTERRUPT_VBUSOK,
        FUSB302B_STATUS0_VBUSOK,
    ));
    assert!(!fusb302b_vbus_detach_was_reported(
        FUSB302B_INTERRUPTA_HARD_RESET,
        0,
    ));
}

#[test]
fn fusb302b_vbus_low_requires_a_bounded_confirmation_window() {
    assert!(!fusb302b_vbus_low_confirmation_expired(None, 1_000));
    assert!(!fusb302b_vbus_low_confirmation_expired(Some(1_000), 1_049,));
    assert!(fusb302b_vbus_low_confirmation_expired(Some(1_000), 1_050,));
    assert!(fusb302b_vbus_low_confirmation_expired(Some(1_000), 2_000,));
}

#[test]
fn fusb302b_vbus_restore_requires_a_bounded_confirmation_window() {
    assert!(!fusb302b_vbus_restore_confirmation_expired(None, 1_000));
    assert!(!fusb302b_vbus_restore_confirmation_expired(
        Some(1_000),
        1_049,
    ));
    assert!(fusb302b_vbus_restore_confirmation_expired(
        Some(1_000),
        1_050,
    ));
}

#[test]
fn vin_sampling_has_no_pd_authority_and_keeps_calibration_conducting() {
    let boot = include_str!("boot.rs");
    let runtime = include_str!("runtime_loop.rs");
    let initial_sample = boot
        .split("async fn initialize_initial_vin")
        .nth(1)
        .and_then(|source| source.split("fn initialize_safety").next())
        .expect("initial VIN sampler must remain present");
    let runtime_sample = runtime
        .split("async fn runtime_read_heater_sensors")
        .nth(1)
        .and_then(|source| {
            source
                .split("async fn runtime_process_frontpanel_input")
                .next()
        })
        .expect("runtime sensor sampler must remain present");

    for sampler in [initial_sample, runtime_sample] {
        for authority in ["pd_port", "pd_contract_ready", "last_pd_observation"] {
            assert!(
                !sampler.contains(authority),
                "VIN sampling must not access PD authority: {authority}"
            );
        }
    }
    assert!(!runtime_sample.contains("calibration_quiet_duty"));
    assert!(!runtime_sample.contains("apply_heater_duty"));
}

#[test]
fn fusb302b_any_low_vbus_observation_interlocks_before_detach_confirmation() {
    let source = RUNTIME_IMPLEMENTATION;
    let implementation = source
        .split("#[cfg(test)]\nmod tests")
        .next()
        .expect("implementation must precede tests");
    let low_vbus_branch = implementation
        .split("async fn handle_vbus_low_event")
        .nth(1)
        .and_then(|value| value.split("async fn handle_empty_event").next())
        .expect("VBUS-low handling must remain present");
    let transition_gate = low_vbus_branch
        .find("if transition && self.vbus_low_candidate_since_ms.is_none()")
        .expect("detach confirmation must remain transition-gated");
    let interlock = low_vbus_branch
        .find("self.interlock_after_vbus_low(now_ms)")
        .expect("every low-VBUS observation must interlock immediately");

    assert!(interlock < transition_gate);
    assert!(implementation.contains(
        "self.policy.on_received_protocol_reset();\n        self.attached_at_ms = Some(now_ms);"
    ));
    assert!(implementation.contains("vbus_low_interlocked: bool"));
    assert!(implementation.contains("self.clear_vbus_low_interlock();"));
}

#[test]
fn fusb302b_persistent_low_vbus_reuses_cc_session_after_restore() {
    let source = RUNTIME_IMPLEMENTATION;
    let implementation = source
        .split("#[cfg(test)]\nmod tests")
        .next()
        .expect("implementation must precede tests");
    let detach_recovery = implementation
        .split("async fn recover_after_detach")
        .nth(1)
        .and_then(|value| {
            value
                .split("    /// Re-synchronize only the PD protocol engine")
                .next()
        })
        .expect("detach recovery implementation must remain present");

    assert!(!detach_recovery.contains("on_detach_or_reset"));
    assert!(!detach_recovery.contains("self.polarity = None"));
    assert!(!detach_recovery.contains("self.next_message_id = 0"));
    assert!(detach_recovery.contains("awaiting_vbus_restore = true"));
    assert!(implementation.contains("fusb302b_vbus_restore_confirmation_expired"));

    let restore_path = implementation
        .split("async fn poll_vbus_restore")
        .nth(1)
        .and_then(|value| value.split("async fn poll_pending_request").next())
        .expect("VBUS restore path must remain present");
    assert!(!restore_path.contains("initialize(i2c)"));
    assert!(restore_path.contains("resynchronize_after_vbus_restore(i2c)"));
    assert!(restore_path.contains("self.awaiting_vbus_restore = false"));
    assert!(restore_path.contains("self.vbus_low_interlocked = false"));

    let resynchronization = implementation
        .split("async fn resynchronize_after_vbus_restore")
        .nth(1)
        .and_then(|value| {
            value
                .split("    /// Recover a local receive/transmit failure")
                .next()
        })
        .expect("VBUS restore resynchronization must remain present");
    let pd_reset = resynchronization
        .find("phy.pd_reset()")
        .expect("VBUS restore must reset the PD engine");
    let fifo_flush = resynchronization
        .find("phy.flush_fifos()")
        .expect("VBUS restore must flush both FIFOs");
    let phy_config = resynchronization
        .find("phy.configure_phy(fusb302b_phy_config(true))")
        .expect("VBUS restore must reapply the receiver PHY configuration");
    let masks = resynchronization
        .find("set_interrupt_masks(FUSB302B_RECEIVE_INTERRUPT_MASKS)")
        .expect("VBUS restore must reapply receiver interrupt masks");
    assert!(pd_reset < fifo_flush && fifo_flush < phy_config && phy_config < masks);
    assert!(!resynchronization.contains("set_cc_pull"));
    assert!(!resynchronization.contains("start_toggle"));
}

#[test]
fn fusb302b_status_only_signals_cannot_restart_the_physical_sink_session() {
    let source = RUNTIME_IMPLEMENTATION;
    let implementation = source
        .split("#[cfg(test)]\nmod tests")
        .next()
        .expect("implementation must precede tests");

    assert!(implementation.contains("Fusb302bReceivedResetAction"));
    assert!(implementation.contains("recover_after_received_reset"));
    assert!(implementation.contains("recover_transient_transport_fault"));
    assert!(implementation.contains("fusb302b_vbus_detach_was_reported"));
    assert!(implementation.contains("Fusb302bReceiveEvent::VbusLow"));
    assert!(implementation.contains("recover_after_detach"));
    assert!(!implementation.contains("fusb302b_runtime_vbus_loss_is_detach"));
    assert!(!implementation.contains("Fusb302bReceiveEvent::OppositeSettledCc"));
}

#[test]
fn fusb302b_retry_failure_is_consumed_until_the_bounded_requery() {
    assert!(fusb302b_retry_failure_requires_recovery(
        FUSB302B_STATUS0A_RETRY_FAIL,
        FUSB302B_STATUS1_RX_EMPTY,
        false,
    ));
    // RETRY_FAIL invalidates the exchange even when a stale frame remains
    // in the receive FIFO. Recovery must take precedence over that frame.
    assert!(fusb302b_retry_failure_requires_recovery(
        FUSB302B_STATUS0A_RETRY_FAIL,
        0,
        false,
    ));
    assert!(!fusb302b_retry_failure_requires_recovery(
        FUSB302B_STATUS0A_RETRY_FAIL,
        FUSB302B_STATUS1_RX_EMPTY,
        true,
    ));
    assert!(!fusb302b_retry_recovery_should_discard_frame(
        FUSB302B_STATUS1_RX_EMPTY,
        true,
    ));
    assert!(fusb302b_retry_recovery_should_discard_frame(0, true));
    assert!(!fusb302b_retry_recovery_should_discard_frame(0, false));
    assert!(!fusb302b::source_capabilities_request_due(
        1_000,
        Some(2_000),
        6_999,
    ));
    assert!(fusb302b::source_capabilities_request_due(
        1_000,
        Some(2_000),
        7_000,
    ));
}

#[test]
fn fusb302b_capability_bridge_preserves_each_usable_apdo() {
    let source = SourceCapabilities::from_pdos(&[
        pps_source_capability(5_000, 21_000, 5_000),
        pps_source_capability(5_000, 25_500, 3_000),
    ]);

    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let mut manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert!(manual.validate_target(20_000, 5_000).is_ok());
    assert!(manual.validate_target(24_000, 3_000).is_ok());
    assert!(manual.validate_target(5_000, 3_000).is_err());
    manual
        .enable(ManualPpsOwner::Debug, 24_000, None)
        .expect("an omitted current must use the APDO covering the requested voltage");
    assert_eq!(manual.target_ma, Some(3_000));
    assert_eq!(
        manual.thermal_plant_source_limits(),
        Some((5_500, 21_000, 3_000))
    );
    assert_eq!(
        adjustable_mode_for_request(24_000, 28_000),
        ch224q::AdjustableVoltageMode::Pps
    );
}

#[test]
fn calibration_request_preserves_the_advertised_apdo_through_the_capability_bridge() {
    let pdos = [
        (100_u32 << 10) | 300,
        pps_source_capability(3_300, 11_000, 3_000),
        pps_source_capability(3_300, 21_000, 3_000),
    ];
    let source = SourceCapabilities::from_pdos(&pdos);
    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let mut manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));
    manual.supply_scan_range = manual.thermal_plant_pps_range();
    manual
        .enable(ManualPpsOwner::Calibration, 5_500, Some(3_000))
        .unwrap();
    let request = manual_pps_contract_request(&manual, true).unwrap();
    let mut sink = fusb302b::SinkPolicy::new_standby(5_000, 1_000);
    assert!(sink.on_source_capabilities(&pdos).is_some());
    sink.on_control_message(3, 0);
    sink.on_control_message(6, 0);
    assert!(sink.prepare_contract_refresh(request));
    let rdo = sink.on_source_capabilities(&pdos).expect(
        "the real capability bridge must not replace an advertised APDO with a clipped identity",
    );
    assert_eq!((u32::from_le_bytes(rdo) >> 28) & 0x07, 3);
    assert_eq!(sink.pending_contract().voltage_mv, 5_500);
    assert_eq!(sink.pending_contract().current_ma, 3_000);
    sink.on_control_message(3, 50);
    sink.on_control_message(6, 100);
    assert!(request_matches_active(
        request,
        sink.confirmed_active_contract()
    ));
}

#[test]
fn thermal_plant_scan_respects_fixed_power_envelope_without_changing_apdo_identity() {
    let source = SourceCapabilities::from_pdos(&[
        fixed_source_capability(20_000, 3_000),
        pps_source_capability(5_000, 21_000, 3_000),
    ]);
    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    assert_eq!(capabilities.source_power_limit_mw, Some(60_000));
    assert_eq!(capabilities.source_power_limit_ma, Some(3_000));

    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));
    assert_eq!(
        manual.thermal_plant_pps_range(),
        Some(PpsAdjustmentRange {
            min_mv: 5_000,
            max_mv: 21_000,
            max_current_ma: 3_000,
        })
    );
    assert_eq!(
        manual.thermal_plant_source_limits(),
        Some((5_500, 20_000, 3_000))
    );
}

#[test]
fn thermal_plant_scan_caps_five_amp_apdo_to_fixed_three_amp_power_envelope() {
    let source = SourceCapabilities::from_pdos(&[
        fixed_source_capability(20_000, 3_000),
        pps_source_capability(5_000, 21_000, 5_000),
    ]);
    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert_eq!(
        manual.thermal_plant_source_limits(),
        Some((5_500, 20_000, 3_000))
    );
}

#[test]
fn advertised_apdo_identity_does_not_relax_the_product_request_bounds() {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(3_300, 21_000, 6_000)]);
    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let mut manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));
    assert_eq!(manual.maximum_pps_current_for_target(5_500), Some(5_000));
    assert!(manual.validate_target(3_300, 3_000).is_err());
    assert!(manual.validate_target(5_500, 5_500).is_err());
    manual.enable(ManualPpsOwner::Debug, 5_500, None).unwrap();
    assert_eq!(manual.target_ma, Some(5_000));
    let candidate = manual
        .automatic_heater_pps_candidate(25.0, None, &MemoryConfig::default(), None)
        .unwrap();
    assert_eq!(candidate.current_ma, 3_000);
    let request = manual
        .automatic_heater_request_contract(
            25.0,
            &MemoryConfig::default(),
            candidate.allowed_mv,
            candidate.current_ma,
            None,
        )
        .unwrap();
    assert_eq!(request.pps_range().unwrap().max_current_ma, 6_000);
    assert!(source.select_exact_contract(request).is_some());
}

#[test]
fn fusb302b_capability_refresh_preserves_manual_pps_intent() {
    let capabilities = ch224q::AdjustablePowerCapabilities {
        pps_min_mv: Some(5_500),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(5_000),
        pps_covers_20v: true,
        pps_apdos: [
            Some(ch224q::PpsApdo {
                min_mv: 5_500,
                max_mv: 21_000,
                max_ma: 5_000,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
        ],
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    let mut manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));
    manual
        .enable(ManualPpsOwner::Debug, 17_500, Some(3_000))
        .unwrap();
    manual.applied_mv = Some(17_500);

    manual.refresh_fusb302b_capabilities_preserving_intent(Some(capabilities));

    assert!(manual.enabled);
    assert_eq!(manual.target_mv, Some(17_500));
    assert_eq!(manual.target_ma, Some(3_000));
    assert_eq!(manual.applied_mv, None);
}

#[test]
fn automatic_heating_uses_a_current_ceiling_valid_across_the_pps_range() {
    let source = SourceCapabilities::from_pdos(&[
        pps_source_capability(5_000, 24_000, 3_000),
        pps_source_capability(24_000, 25_500, 5_000),
    ]);

    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert_eq!(manual.heater_source_limits(), Some((5_500, 25_500, 3_000)));
}

#[test]
fn thermal_plant_uses_one_apdo_covering_the_twenty_volt_anchor() {
    let source = SourceCapabilities::from_pdos(&[
        pps_source_capability(5_000, 24_000, 3_000),
        pps_source_capability(20_000, 25_500, 5_000),
    ]);

    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert_eq!(
        manual.thermal_plant_source_limits(),
        Some((20_000, 21_000, 3_000))
    );
    assert_eq!(manual.heater_source_limits(), Some((5_500, 25_500, 3_000)));
}

#[test]
fn fusb302b_capability_bridge_retains_degraded_pps_apdo() {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(5_000, 19_000, 3_000)]);

    let capabilities = fusb302b_adjustable_power_capabilities(source)
        .expect("a usable lower-voltage APDO remains visible to the PPS bridge");
    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert!(!capabilities.pps_covers_20v);
    assert!(manual.validate_target(12_000, 3_000).is_ok());
    assert_eq!(manual.thermal_plant_source_limits(), None);
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        pps_max_mv,
        capability_max_ma,
        ..
    } = select_fusb302b_heater_power_backend(Some(capabilities))
    else {
        panic!("a usable degraded PPS APDO must remain available to automatic heating");
    };
    assert_eq!(pps_min_mv, 5_500);
    assert_eq!(pps_max_mv, 19_000);
    assert_eq!(capability_max_ma, 3_000);
}

#[test]
fn fusb302b_missing_capabilities_never_restores_the_legacy_twenty_volt_default() {
    let backend = select_fusb302b_heater_power_backend(None);

    assert!(matches!(
        backend,
        HeaterPowerBackend::FixedPdPwmFallback {
            fixed_request: ch224q::VoltageRequest::V12,
            ..
        }
    ));
}

#[test]
fn fusb302b_idle_restore_requires_an_apdo_that_covers_twelve_volts() {
    let mut capabilities = ch224q::AdjustablePowerCapabilities::default();
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_000,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    assert!(source_supports_fusb302b_idle_pps(capabilities));

    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 15_000,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    assert!(!source_supports_fusb302b_idle_pps(capabilities));
}

#[test]
fn automatic_idle_restore_requires_a_confirmed_fixed_contract() {
    let observation = |voltage_mv| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: Status::from_register(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 3_000,
        contract_voltage_mv: Some(voltage_mv),
        contract: Contract::observed(ContractKind::Fixed, voltage_mv, 3_000),
    };

    assert!(automatic_idle_contract_is_confirmed(
        observation(12_000),
        None
    ));
    assert!(automatic_idle_contract_is_confirmed(
        observation(20_000),
        None
    ));
}

#[test]
fn automatic_idle_restore_rejects_a_low_current_twelve_volt_contract() {
    let observation = |kind| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: Status::from_register(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 50,
        contract_voltage_mv: Some(12_000),
        contract: Contract::observed(kind, 12_000, 50),
    };

    assert!(!automatic_idle_contract_is_confirmed(
        observation(ContractKind::Pps),
        None
    ));
    assert!(!automatic_idle_contract_is_confirmed(
        observation(ContractKind::Fixed),
        None
    ));
}

fn complete_idle_input() -> CompleteIdleInput {
    CompleteIdleInput {
        heater_enabled: false,
        thermal_tuning_active: false,
        physical_duty_percent: 0,
        manual_pps: ManualPpsState::default(),
        calibration: CalibrationRuntimeState::default(),
        fan_state: FanPolicyState::Disabled,
        fan_enabled: false,
        fan_power_pending: false,
    }
}

#[test]
fn complete_idle_excludes_armed_zero_pwm_pulse_gaps_and_pending_work() {
    assert!(complete_idle_power_allowed(complete_idle_input()));
    let mut tuning = complete_idle_input();
    tuning.thermal_tuning_active = true;
    assert!(!complete_idle_power_allowed(tuning));
    let mut armed = complete_idle_input();
    armed.heater_enabled = true;
    assert!(!complete_idle_power_allowed(armed));
    let mut physical = complete_idle_input();
    physical.physical_duty_percent = 1;
    assert!(!complete_idle_power_allowed(physical));
    let mut manual = complete_idle_input();
    manual.manual_pps.enabled = true;
    assert!(!complete_idle_power_allowed(manual));
    let mut calibration = complete_idle_input();
    calibration.calibration.mode = CalibrationMode::HeaterCurve;
    assert!(!complete_idle_power_allowed(calibration));
    let mut job = complete_idle_input();
    job.calibration.job.status = CalibrationJobStatus::Running;
    assert!(!complete_idle_power_allowed(job));
    let mut disarm = complete_idle_input();
    disarm.calibration.immediate_heater_disarm_pending = true;
    assert!(!complete_idle_power_allowed(disarm));
    let mut pulse = complete_idle_input();
    pulse.fan_state = FanPolicyState::HeatingGuardPulse {
        duty_percent: 20,
        pwm_permille: 500,
    };
    assert!(!complete_idle_power_allowed(pulse));
    let mut fan = complete_idle_input();
    fan.fan_enabled = true;
    assert!(!complete_idle_power_allowed(fan));
    let mut pending_fan = complete_idle_input();
    pending_fan.fan_power_pending = true;
    assert!(!complete_idle_power_allowed(pending_fan));
}

#[test]
fn ordinary_idle_replaces_cooling_fixed_with_lowest_adequate_fixed() {
    let observation = |mv, ma| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: ma,
        contract_voltage_mv: Some(mv),
        contract: Contract::observed(ContractKind::Fixed, mv, ma),
    };
    let mut source = SourceCapabilitiesView::default();
    source.fixed[0] = Some((5_000, 1_000));
    source.fixed[1] = Some((12_000, 3_000));
    assert!(!preferred_idle_contract_confirmed(
        Some(observation(12_000, 420)),
        source
    ));
    assert!(preferred_idle_contract_confirmed(
        Some(observation(5_000, 1_000)),
        source
    ));
    assert!(!preferred_idle_contract_confirmed(None, source));
    assert!(!working_fixed_contract_confirmed(
        Some(observation(12_000, 420)),
        12_000,
        3_000
    ));
    assert!(working_fixed_contract_confirmed(
        Some(observation(12_000, 3_000)),
        12_000,
        3_000
    ));
    source.fixed[0] = Some((5_000, 500));
    assert!(preferred_idle_contract_confirmed(
        Some(observation(12_000, 420)),
        source
    ));
    assert!(!preferred_idle_contract_confirmed(
        Some(observation(5_000, 1_000)),
        source
    ));
}

fn standby_policy_for_heater_start() -> (fusb302b::SinkPolicy, SourceCapabilities) {
    let pdos = [
        (100 << 10) | 100,
        (240 << 10) | 300,
        pps_source_capability(5_500, 21_000, 3_000),
    ];
    let source = SourceCapabilities::from_pdos(&pdos);
    let mut policy = fusb302b::SinkPolicy::new_standby(12_000, 3_000);
    assert!(policy.on_source_capabilities(&pdos).is_some());
    policy.on_control_message(3, 0);
    policy.on_control_message(6, 1);
    assert_eq!(policy.active_contract().voltage_mv, 5_000);
    (policy, source)
}

#[test]
fn ordinary_heater_start_survives_fixed_to_pps_accept_and_ps_rdy() {
    let (mut policy, source) = standby_policy_for_heater_start();
    let request = PdContractRequest::pps(12_000, 3_000).unwrap();
    let mut ui = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    ui.heater_enabled = true;
    let mut calibration = CalibrationRuntimeState::default();
    let mut manual = ManualPpsState::default();
    let mut ready = true;
    assert!(policy.request_contract(request).is_some());
    for phase in [
        SinkPhase::WaitingForAccept,
        SinkPhase::WaitingForPsRdy,
        SinkPhase::Ready,
    ] {
        assert_eq!(policy.phase(), phase);
        let observation = fusb302b_contract_observation(
            policy.phase(),
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            source,
        );
        let power = PowerState::from_observation(
            policy.phase(),
            observation,
            Some(source),
            true,
            Some(request),
        );
        reconcile_pd_contract_intent(
            power.observation(),
            &mut ready,
            &mut ui,
            &mut calibration,
            &mut manual,
            false,
        );
        ui.heater_enabled = reconcile_runtime_heater_enabled(
            ui.heater_enabled,
            calibration,
            None,
            false,
            false,
            true,
            ready,
        );
        assert!(
            ui.heater_enabled,
            "normal {phase:?} discarded the fresh startup intent"
        );
        let mut idle = complete_idle_input();
        idle.heater_enabled = ui.heater_enabled;
        assert!(
            !complete_idle_power_allowed(idle),
            "Idle competed with the working request"
        );
        let snapshot = PdServiceSnapshot {
            observation,
            transition_pending: phase != SinkPhase::Ready,
            service_available: true,
            ..PdServiceSnapshot::unavailable()
        };
        let working = working_pps_request_confirmed(snapshot.settled_observation(), request);
        assert_eq!(
            working,
            phase == SinkPhase::Ready,
            "PWM must wait for the new working contract"
        );
        let terminal = pending_terminal_outcome(
            PdServiceTerminalContext {
                phase,
                request_timed_out: false,
                request_rejected: false,
                diagnostic_request_timed_out: false,
                refresh_pending: false,
                observation,
                source_capabilities: Some(source),
            },
            PendingPdOperation::Contract { request },
        );
        assert_eq!(terminal.is_some(), phase == SinkPhase::Ready);
        if phase == SinkPhase::WaitingForAccept {
            assert_eq!(observation.unwrap().contract.voltage_mv, 5_000);
            policy.on_control_message(3, 2);
        } else if phase == SinkPhase::WaitingForPsRdy {
            assert_eq!(observation.unwrap().contract.voltage_mv, 5_000);
            policy.on_control_message(6, 3);
        }
    }
}

#[test]
fn fan_start_waits_for_heating_and_does_not_preempt_its_working_request() {
    let (mut policy, source) = standby_policy_for_heater_start();
    let request = PdContractRequest::pps(12_000, 3_000).unwrap();
    assert!(policy.request_contract(request).is_some());
    let pending = PdServiceSnapshot {
        observation: fusb302b_contract_observation(
            policy.phase(),
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            source,
        ),
        transition_pending: true,
        service_available: true,
        ..PdServiceSnapshot::unavailable()
    };
    assert_eq!(
        fan_working_power_admission(pending, false, true, false),
        FanWorkingPowerAdmission::WaitForHeating
    );
    assert_eq!(
        fan_working_power_admission(pending, false, false, true),
        FanWorkingPowerAdmission::WaitForHeating
    );
    policy.on_control_message(3, 2);
    policy.on_control_message(6, 3);
    let working = PdServiceSnapshot {
        observation: fusb302b_contract_observation(
            policy.phase(),
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            source,
        ),
        transition_pending: false,
        ..pending
    };
    assert_eq!(
        fan_working_power_admission(working, false, true, false),
        FanWorkingPowerAdmission::Confirmed
    );
    let adjusting = PdServiceSnapshot {
        transition_pending: true,
        ..working
    };
    assert!(adjusting.settled_observation().is_none());
    assert_eq!(
        adjusting.conducting_observation().unwrap().contract,
        working.observation.unwrap().contract
    );
    let observation = working.observation;
    assert!(calibration_contract_matches(
        observation,
        Some(12_000),
        Some(3_000),
        false
    ));
    assert!(!calibration_contract_matches(
        observation,
        Some(12_000),
        Some(3_000),
        true
    ));
    assert!(!calibration_contract_matches(
        observation,
        Some(11_900),
        Some(3_000),
        false
    ));
    assert!(!calibration_contract_matches(
        observation,
        Some(12_000),
        Some(2_900),
        false
    ));
    assert!(
        adjusting
            .with_fresh_observation(u64::MAX)
            .conducting_observation()
            .is_none()
    );
    assert_eq!(
        fan_working_power_admission(adjusting, true, true, false),
        FanWorkingPowerAdmission::Confirmed
    );
    assert_eq!(
        fan_working_power_admission(adjusting, false, true, false),
        FanWorkingPowerAdmission::Confirmed
    );
    assert_eq!(
        fan_working_power_admission(pending, false, false, false),
        FanWorkingPowerAdmission::RequestCooling
    );
}

#[test]
fn confirmed_manual_pps_preserves_post_heat_and_forced_fan_commands() {
    let (mut policy, source) = standby_policy_for_heater_start();
    assert!(
        policy
            .request_contract(PdContractRequest::pps(12_000, 3_000).unwrap())
            .is_some()
    );
    for phase in [
        SinkPhase::WaitingForAccept,
        SinkPhase::WaitingForPsRdy,
        SinkPhase::Ready,
    ] {
        assert_eq!(policy.phase(), phase);
        let snapshot = PdServiceSnapshot {
            observation: fusb302b_contract_observation(
                phase,
                policy.active_contract(),
                FUSB302B_STATUS0_VBUSOK,
                source,
            ),
            transition_pending: phase != SinkPhase::Ready,
            service_available: true,
            ..PdServiceSnapshot::unavailable()
        };
        for mut decision in manual_pps_cooling_decisions() {
            let wanted = decision.command;
            let admission =
                gate_fan_decision_on_working_power(&mut decision, snapshot, false, false, true);
            if phase == SinkPhase::Ready {
                assert_eq!(admission, FanWorkingPowerAdmission::Confirmed);
                assert_eq!(decision.command, wanted);
                assert!(decision.command.enabled);
            } else {
                assert_eq!(admission, FanWorkingPowerAdmission::WaitForHeating);
                assert!(!decision.command.enabled);
                assert_eq!(decision.output_level, FanOutputLevel::Off);
            }
        }
        if phase == SinkPhase::WaitingForAccept {
            policy.on_control_message(3, 2);
        } else if phase == SinkPhase::WaitingForPsRdy {
            policy.on_control_message(6, 3);
        }
    }
}

fn manual_pps_cooling_decisions() -> [FanPolicyDecision; 3] {
    let post_heat = fan_policy_decision_with_modes(
        55,
        1_000,
        false,
        true,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Off,
        (FanPolicyState::Disabled, false),
    );
    let forced = |temp| {
        let state = overtemp_forced_fan_state(temp, true).unwrap();
        let command = state.command(1_000);
        FanPolicyDecision {
            state,
            command,
            display_state: FanDisplayState::Safe,
            source: FanPolicySource::Safety,
            output_level: fan_output_level_for_command(command),
        }
    };
    [post_heat, forced(220), forced(50)]
}

#[test]
fn fan_only_retains_fresh_pps_during_transition_but_rejects_stale_or_standby_power() {
    let (mut policy, source) = standby_policy_for_heater_start();
    let standby = PdServiceSnapshot {
        observation: fusb302b_contract_observation(
            policy.phase(),
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            source,
        ),
        service_available: true,
        published_at_ms: 1_000,
        ..PdServiceSnapshot::unavailable()
    };
    let mut decision = manual_pps_cooling_decisions()[0];
    assert_eq!(
        gate_fan_decision_on_working_power(&mut decision, standby, false, false, false),
        FanWorkingPowerAdmission::RequestCooling
    );
    assert!(!decision.command.enabled);
    assert!(
        policy
            .request_contract(PdContractRequest::pps(12_000, 3_000).unwrap())
            .is_some()
    );
    policy.on_control_message(3, 2);
    policy.on_control_message(6, 3);
    let working = PdServiceSnapshot {
        observation: fusb302b_contract_observation(
            policy.phase(),
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            source,
        ),
        transition_pending: true,
        ..standby
    };
    let mut decision = manual_pps_cooling_decisions()[0];
    assert_eq!(
        gate_fan_decision_on_working_power(&mut decision, working, false, false, false),
        FanWorkingPowerAdmission::Confirmed
    );
    assert!(decision.command.enabled);
    let expired = working.with_fresh_observation(1_001 + PD_SNAPSHOT_MAX_AGE_MS);
    assert_eq!(
        gate_fan_decision_on_working_power(&mut decision, expired, true, false, false),
        FanWorkingPowerAdmission::RequestCooling
    );
    assert!(!decision.command.enabled);
}

#[test]
fn ordinary_heater_start_discards_intent_on_real_loss_and_never_replays_it() {
    let (mut policy, source) = standby_policy_for_heater_start();
    let request = PdContractRequest::pps(12_000, 3_000).unwrap();
    assert!(policy.request_contract(request).is_some());
    let mut ui = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    ui.heater_enabled = true;
    ui.heater_output_percent = 50;
    let mut calibration = CalibrationRuntimeState::default();
    let mut manual = ManualPpsState::default();
    let mut ready = true;
    let lost = fusb302b_contract_observation(policy.phase(), policy.active_contract(), 0, source);
    assert!(lost.is_none());
    assert!(reconcile_pd_contract_intent(
        lost,
        &mut ready,
        &mut ui,
        &mut calibration,
        &mut manual,
        true
    ));
    assert!(!ready && !ui.heater_enabled);
    assert_eq!(ui.heater_output_percent, 0);
    policy.on_control_message(3, 2);
    policy.on_control_message(6, 3);
    let recovered = fusb302b_contract_observation(
        policy.phase(),
        policy.active_contract(),
        FUSB302B_STATUS0_VBUSOK,
        source,
    );
    reconcile_pd_contract_intent(
        recovered,
        &mut ready,
        &mut ui,
        &mut calibration,
        &mut manual,
        false,
    );
    assert!(ready && !ui.heater_enabled);
    for phase in [
        SinkPhase::Detached,
        SinkPhase::Fault,
        SinkPhase::WaitingForSourceCapabilities,
    ] {
        assert!(
            fusb302b_contract_observation(
                phase,
                policy.active_contract(),
                FUSB302B_STATUS0_VBUSOK,
                source
            )
            .is_none()
        );
    }
    assert!(
        fusb302b_contract_observation(
            SinkPhase::Ready,
            policy.active_contract(),
            FUSB302B_STATUS0_VBUSOK,
            SourceCapabilities::empty()
        )
        .is_none()
    );
}

#[test]
fn flash_preparation_cannot_use_old_fixed_while_a_transition_is_pending() {
    let (policy, source) = standby_policy_for_heater_start();
    let observation = fusb302b_contract_observation(
        policy.phase(),
        policy.active_contract(),
        FUSB302B_STATUS0_VBUSOK,
        source,
    );
    let ui = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let calibration = CalibrationRuntimeState::default();
    let context = test_usb_runtime_status_context();
    let snapshot = PdServiceSnapshot {
        observation,
        transition_pending: true,
        service_available: true,
        ..PdServiceSnapshot::unavailable()
    };
    assert!(snapshot.observation.is_some());
    assert!(!flash_preparation_status(&ui, &calibration, &context, snapshot).pd_fixed_or_default);
    assert!(
        flash_preparation_status(
            &ui,
            &calibration,
            &context,
            PdServiceSnapshot {
                transition_pending: false,
                ..snapshot
            }
        )
        .pd_fixed_or_default
    );
}

#[test]
fn ordinary_heater_restart_requires_exact_live_working_pps_confirmation() {
    let request = PdContractRequest::pps(12_000, 3_000).unwrap();
    let observation = |kind, mv, ma| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: ma,
        contract_voltage_mv: Some(mv),
        contract: Contract::observed(kind, mv, ma),
    };
    assert!(!working_pps_request_confirmed(None, request));
    assert!(!working_pps_request_confirmed(
        Some(observation(ContractKind::Fixed, 12_000, 3_000)),
        request
    ));
    assert!(!working_pps_request_confirmed(
        Some(observation(ContractKind::Pps, 5_000, 3_000)),
        request
    ));
    assert!(!working_pps_request_confirmed(
        Some(observation(ContractKind::Pps, 12_000, 1_000)),
        request
    ));
    assert!(working_pps_request_confirmed(
        Some(observation(ContractKind::Pps, 12_000, 3_000)),
        request
    ));
}

#[test]
fn supply_scan_requires_its_pinned_apdo_before_sampling_or_conduction() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual
        .enable(ManualPpsOwner::Calibration, 6_000, Some(1_500))
        .unwrap();
    manual.supply_scan_range = manual.thermal_plant_pps_range();
    let request = manual_pps_contract_request(&manual, true).unwrap();
    assert_eq!(request.operating_current_ma(), 1_500);
    assert_eq!(request.pps_range(), manual.supply_scan_range);
    assert!(manual_pps_contract_request(&manual, false).is_none());
    let source = SourceCapabilities::from_pdos(&[
        pps_source_capability(5_000, 12_000, 3_000),
        pps_source_capability(5_000, 21_000, 3_000),
    ]);
    let contract = source.select_exact_contract(request).unwrap();
    let snapshot = PdServiceSnapshot {
        observation: Some(PdStatusObservation {
            status_raw: FUSB302B_STATUS0_VBUSOK,
            status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
            current_raw: 0,
            current_ma: 1_500,
            contract_voltage_mv: Some(6_000),
            contract,
        }),
        active: ConfirmedActiveContract::from_private_contract(contract, source),
        source: source.view(),
        published_at_ms: 100,
        ..PdServiceSnapshot::unavailable()
    };
    assert!(supply_scan_contract_matches(snapshot, &manual));
    let wrong = source
        .select_exact_contract(PdContractRequest::calibration_pps(6_000, 1_500).unwrap())
        .unwrap();
    assert!(!supply_scan_contract_matches(
        PdServiceSnapshot {
            active: ConfirmedActiveContract::from_private_contract(wrong, source),
            ..snapshot
        },
        &manual
    ));
    assert!(!supply_scan_contract_matches(
        PdServiceSnapshot {
            transition_pending: true,
            ..snapshot
        },
        &manual
    ));
    assert!(!supply_scan_contract_matches(
        snapshot.with_fresh_observation(u64::MAX),
        &manual
    ));
}

#[test]
fn automatic_confirmed_contract_is_rechecked_during_pending_transitions() {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
    let contract = source
        .select_exact_contract(PdContractRequest::pps(8_900, 3_000).unwrap())
        .unwrap();
    let observation = PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 3_000,
        contract_voltage_mv: Some(8_900),
        contract,
    };
    let snapshot = PdServiceSnapshot {
        observation: Some(observation),
        active: ConfirmedActiveContract::from_private_contract(contract, source),
        source: source.view(),
        published_at_ms: 100,
        ..PdServiceSnapshot::unavailable()
    };
    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual.capability_apdos[0].as_mut().unwrap().min_mv = 5_500;
    let config = MemoryConfig::default();
    let safe = |temp, state| {
        manual
            .automatic_heater_confirmed_pps_candidate(temp, &config, state)
            .is_some()
    };
    assert!(safe(20.0, snapshot));
    assert!(
        safe(
            20.0,
            PdServiceSnapshot {
                transition_pending: true,
                ..snapshot
            }
        ),
        "a safe old contract can conduct during an ordinary pending change"
    );
    assert!(
        !safe(-10.0, snapshot),
        "latest temperature can invalidate the old contract"
    );
    assert!(!safe(20.0, PdServiceSnapshot::unavailable()));
    assert!(!safe(20.0, snapshot.with_fresh_observation(10_000)));
    assert!(!safe(
        20.0,
        PdServiceSnapshot {
            observation: Some(PdStatusObservation {
                current_ma: 2_950,
                ..observation
            }),
            ..snapshot
        }
    ));
    assert!(!safe(
        20.0,
        PdServiceSnapshot {
            active: None,
            ..snapshot
        }
    ));
}

#[derive(Clone, Copy)]
enum FakeAutomaticPpsRequestResult {
    Pending,
    Confirmed,
    Failed,
}

fn fake_automatic_pps_transition_trace(
    voltage_changed: bool,
    mode_changed: bool,
    safety_recovery_required: bool,
    request_transition_pending: bool,
    gate_duty_percent: u8,
    blank_heater: bool,
    result: FakeAutomaticPpsRequestResult,
) -> Vec<&'static str> {
    let mut trace = Vec::new();
    if automatic_pps_transition_should_clear_pwm(
        voltage_changed,
        mode_changed,
        safety_recovery_required,
        request_transition_pending,
        gate_duty_percent,
        blank_heater,
    ) {
        trace.push("pwm-off");
    }
    if automatic_pps_transition_requires_request(
        voltage_changed,
        mode_changed,
        safety_recovery_required,
        request_transition_pending,
    ) {
        trace.push("pd-request");
        match result {
            FakeAutomaticPpsRequestResult::Pending => trace.push("wait"),
            FakeAutomaticPpsRequestResult::Failed => trace.push("pause"),
            FakeAutomaticPpsRequestResult::Confirmed => {
                if automatic_pps_transition_should_restore_pwm(
                    true,
                    safety_recovery_required,
                    blank_heater,
                    gate_duty_percent,
                ) {
                    trace.push("pwm-restore");
                }
            }
        }
    }
    trace
}

#[test]
fn fake_automatic_pps_transition_clears_pwm_before_safety_request() {
    assert_eq!(
        fake_automatic_pps_transition_trace(
            false,
            false,
            true,
            false,
            80,
            false,
            FakeAutomaticPpsRequestResult::Pending,
        ),
        vec!["pwm-off", "pd-request", "wait"]
    );
}

#[test]
fn fake_automatic_pps_transition_does_not_restore_after_stale_confirmation() {
    assert_eq!(
        fake_automatic_pps_transition_trace(
            true,
            false,
            false,
            false,
            80,
            true,
            FakeAutomaticPpsRequestResult::Pending,
        ),
        vec!["pwm-off", "pd-request", "wait"]
    );
    assert!(!automatic_pps_transition_should_restore_pwm(
        true, true, true, 80
    ));
}

#[test]
fn fake_automatic_pps_transition_pauses_after_failed_negotiation() {
    assert_eq!(
        fake_automatic_pps_transition_trace(
            true,
            false,
            true,
            false,
            80,
            true,
            FakeAutomaticPpsRequestResult::Failed,
        ),
        vec!["pwm-off", "pd-request", "pause"]
    );
}

#[test]
fn fake_automatic_pps_transition_restores_after_confirmed_stable_request() {
    assert_eq!(
        fake_automatic_pps_transition_trace(
            true,
            false,
            false,
            false,
            52,
            false,
            FakeAutomaticPpsRequestResult::Confirmed,
        ),
        vec!["pd-request", "pwm-restore"]
    );
}

#[test]
fn automatic_pps_safety_recovery_bypasses_the_stale_high_voltage_request() {
    assert_eq!(
        automatic_pps_safety_recovery_target_mv(5_500, 19_800),
        5_500
    );
    assert_eq!(
        automatic_pps_safety_recovery_target_mv(12_000, 9_700),
        9_700
    );
}

#[test]
fn ordinary_heater_off_drops_cached_pps_before_fresh_output_start() {
    let mut backend = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_500,
        idle_request_mv: 12_000,
        pps_max_mv: 21_000,
        adjustable_max_mv: 21_000,
        capability_max_ma: 3_000,
        current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
        current_request_mv: 21_000,
        settle_until_ms: Some(10_000),
        next_request_at_ms: 10_000,
        current_limit_fixed_pwm_active: true,
        current_limit_fixed_request_confirmed: true,
        terminal_fixed_pd_disarmed: false,
    };
    assert!(automatic_heater_power_is_idle(false, 0, false, false));
    assert!(!automatic_heater_power_is_idle(true, 0, false, false));
    assert!(!automatic_heater_power_is_idle(false, 0, true, false));
    assert!(!automatic_heater_power_is_idle(false, 1, false, false));
    assert!(!automatic_heater_power_is_idle(false, 0, false, true));
    reset_automatic_heater_backend(&mut backend);
    let HeaterPowerBackend::PpsMos {
        current_mode,
        settle_until_ms,
        next_request_at_ms,
        current_limit_fixed_request_confirmed,
        ..
    } = backend
    else {
        panic!("PPS capability must be retained for the next heating intent");
    };
    assert_eq!(current_mode, None);
    assert_eq!(settle_until_ms, None);
    assert_eq!(next_request_at_ms, 0);
    assert!(!current_limit_fixed_request_confirmed);
}

#[test]
fn automatic_heater_recovery_soft_start_ramps_after_a_safety_pause() {
    assert_eq!(automatic_heater_recovery_soft_start_percent(None, 0), 100);
    assert_eq!(
        automatic_heater_recovery_soft_start_percent(Some(1_000), 1_000),
        0
    );
    assert_eq!(
        automatic_heater_recovery_soft_start_percent(
            Some(1_000),
            1_000 + HEATER_WARMUP_SOFT_START_MS / 2
        ),
        50
    );
    assert_eq!(
        automatic_heater_recovery_soft_start_percent(
            Some(1_000),
            1_000 + HEATER_WARMUP_SOFT_START_MS
        ),
        100
    );
    assert_eq!(
        automatic_heater_recovery_soft_start_percent(Some(1_000), 4_000),
        100
    );
}

#[test]
fn automatic_heater_recovery_stays_armed_during_zero_power_safety_pause() {
    let started_at_ms = automatic_heater_recovery_started_at_ms(
        None,
        AutomaticHeaterRecoveryStateInput {
            normal_auto: true,
            heater_enabled: true,
            force_output_off: false,
            safety_paused: true,
            requested_power_mw: 0,
            automatic_output_was_active: true,
            physical_duty_percent: 0,
            now_ms: 1_000,
        },
    );
    assert_eq!(started_at_ms, Some(1_000));

    let started_at_ms = automatic_heater_recovery_started_at_ms(
        started_at_ms,
        AutomaticHeaterRecoveryStateInput {
            normal_auto: true,
            heater_enabled: true,
            force_output_off: false,
            safety_paused: true,
            requested_power_mw: 0,
            automatic_output_was_active: false,
            physical_duty_percent: 0,
            now_ms: 1_500,
        },
    );
    assert_eq!(started_at_ms, Some(1_000));

    assert_eq!(
        automatic_heater_recovery_started_at_ms(
            started_at_ms,
            AutomaticHeaterRecoveryStateInput {
                normal_auto: true,
                heater_enabled: true,
                force_output_off: false,
                safety_paused: false,
                requested_power_mw: 0,
                automatic_output_was_active: false,
                physical_duty_percent: 0,
                now_ms: 1_600,
            },
        ),
        None
    );
}

#[test]
fn flash_preparation_readiness_does_not_depend_on_vin_measurements() {
    let observation = PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    };

    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let calibration = CalibrationRuntimeState::default();
    let mut context = test_usb_runtime_status_context();
    context.last_pd_observation = Some(observation);
    context.elapsed_ms = 1_500;
    let snapshot = PdServiceSnapshot {
        observation: Some(observation),
        service_available: true,
        ..PdServiceSnapshot::unavailable()
    };
    for vin_mv in [0, 4_000, 5_000, 6_589, 21_121] {
        context.vin_mv = vin_mv;
        let readiness = flash_preparation_status(&ui_state, &calibration, &context, snapshot);
        assert!(!readiness.heating);
        assert!(!readiness.cooling);
        assert!(
            readiness.pd_fixed_or_default,
            "VIN {vin_mv} changed protocol readiness"
        );
    }
}

#[test]
fn flash_preparation_waits_for_pending_fan_power_before_reporting_fixed_ready() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let calibration = CalibrationRuntimeState::default();
    let mut context = test_usb_runtime_status_context();
    context.last_pd_observation = Some(PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    });
    let snapshot = PdServiceSnapshot {
        observation: context.last_pd_observation,
        service_available: true,
        ..PdServiceSnapshot::unavailable()
    };
    assert!(
        flash_preparation_status(&ui_state, &calibration, &context, snapshot).pd_fixed_or_default
    );

    context.fan_working_power_pending = true;
    let pending = flash_preparation_status(&ui_state, &calibration, &context, snapshot);
    assert!(!pending.heating);
    assert!(!pending.cooling);
    assert!(!pending.pd_fixed_or_default);

    context.fan_working_power_pending = false;
    assert!(
        flash_preparation_status(&ui_state, &calibration, &context, snapshot).pd_fixed_or_default
    );
}

#[test]
fn flash_preparation_rejects_expired_service_snapshot_even_with_cached_fixed_power() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let calibration = CalibrationRuntimeState::default();
    let mut context = test_usb_runtime_status_context();
    context.last_pd_observation = Some(PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    });
    let snapshot = PdServiceSnapshot {
        observation: context.last_pd_observation,
        service_available: true,
        published_at_ms: 1_000,
        ..PdServiceSnapshot::unavailable()
    };
    assert!(
        flash_preparation_status(
            &ui_state,
            &calibration,
            &context,
            snapshot.with_fresh_observation(1_000 + PD_SNAPSHOT_MAX_AGE_MS),
        )
        .pd_fixed_or_default
    );
    let expired = flash_preparation_status(
        &ui_state,
        &calibration,
        &context,
        snapshot.with_fresh_observation(1_001 + PD_SNAPSHOT_MAX_AGE_MS),
    );
    assert!(!expired.heating);
    assert!(!expired.cooling);
    assert!(!expired.pd_fixed_or_default);
    assert!(context.last_pd_observation.is_some());
}

#[test]
fn capability_refresh_ticket_always_settles_when_detached_or_faulted() {
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Detached, true, false, false),
        Some(TicketOutcome::Detached),
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Fault, true, false, false),
        Some(TicketOutcome::TransportFault),
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::WaitingForAccept, true, false, false),
        None,
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::WaitingForPsRdy, false, false, true),
        None,
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Ready, false, false, true),
        Some(TicketOutcome::CapabilitiesRefreshed),
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Ready, false, true, true),
        Some(TicketOutcome::TimedOut),
    );
}

#[test]
fn automatic_heating_does_not_join_disjoint_pps_apdos() {
    let source = SourceCapabilities::from_pdos(&[
        pps_source_capability(5_000, 11_000, 3_000),
        pps_source_capability(12_000, 19_000, 3_000),
    ]);

    let capabilities = fusb302b_adjustable_power_capabilities(source).unwrap();
    let manual = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    assert_eq!(manual.heater_source_limits(), Some((12_000, 19_000, 3_000)));
}

#[test]
fn fusb302b_identity_requires_stable_family_id_and_readable_status() {
    assert!(fusb302b_identity_is_stable(
        Some(0x91),
        Some(0x91),
        Some(0),
        Some(0)
    ));
    assert!(!fusb302b_identity_is_stable(
        Some(0x91),
        Some(0x92),
        Some(0),
        Some(0)
    ));
    assert!(fusb302b_identity_is_stable(
        Some(0x81),
        Some(0x81),
        Some(0),
        Some(0)
    ));
    assert!(!fusb302b_identity_is_stable(
        Some(0x71),
        Some(0x71),
        Some(0),
        Some(0)
    ));
    assert!(!fusb302b_identity_is_stable(
        Some(0x91),
        Some(0x91),
        Some(u8::MAX),
        Some(0),
    ));
    assert!(!fusb302b_identity_is_stable(
        Some(0x91),
        Some(0x91),
        Some(0),
        None
    ));
}

#[test]
fn v5_memory_header_bounds_payload_before_reading_slot_body() {
    let mut header = [0xff; MEMORY_RECORD_HEADER_LEN];
    header[0..4].copy_from_slice(b"FPM1");
    header[4] = MEMORY_RECORD_FORMAT_VERSION;
    header[5] = MEMORY_RECORD_HEADER_LEN as u8;
    header[6..8].copy_from_slice(&100u16.to_le_bytes());
    assert_eq!(
        memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
        Some(MEMORY_RECORD_HEADER_LEN + 100)
    );

    header[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
        None
    );
    header[4] = 0x7f;
    assert_eq!(
        memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
        None
    );
    header[4] = 1;
    assert_eq!(
        memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
        None
    );
}

#[test]
fn startup_sequence_requires_backlight_pd_display_frame_before_other_work() {
    let mut sequence = StartupSequence::new();
    assert!(!sequence.advance(StartupSequenceStage::PdServiceComplete));
    assert!(sequence.advance(StartupSequenceStage::BacklightReady));
    assert!(!sequence.advance(StartupSequenceStage::StartupFrameReady));
    assert!(sequence.advance(StartupSequenceStage::PdServiceComplete));
    assert!(sequence.advance(StartupSequenceStage::DisplayReady));
    assert!(sequence.advance(StartupSequenceStage::StartupFrameReady));
    assert!(sequence.advance(StartupSequenceStage::OtherInitialization));
    assert!(!sequence.advance(StartupSequenceStage::OtherInitialization));
}

#[test]
fn memory_commit_publishes_active_marker_after_every_domain_write() {
    let source = RUNTIME_IMPLEMENTATION;
    let commit = source
        .split("async fn commit_memory_config_now")
        .nth(1)
        .and_then(|value| {
            value
                .split("async fn commit_memory_config_domains_without_marker")
                .next()
        })
        .expect("memory commit implementation must remain present");
    let prepared = commit
        .find("LayoutMarkerStatus::Prepared")
        .expect("commit must prepare a marker");
    let double_slot_domains = commit
        .find("double_slot_domains: true,\n            phase: \"write\"")
        .expect("commit must write double-slot domains");
    let single_slot_domains = commit
        .find("double_slot_domains: false,\n            phase: \"write-single\"")
        .expect("commit must write single-slot domains");
    let active = commit
        .find("LayoutMarkerStatus::Active")
        .expect("commit must publish an active marker");

    assert!(prepared < double_slot_domains);
    assert!(double_slot_domains < single_slot_domains);
    assert!(single_slot_domains < active);
}

#[test]
fn fusb302b_initial_pps_request_matches_the_idle_voltage() {
    assert_eq!(FUSB302B_INITIAL_PPS_REQUEST_MV, HEATER_ADJUSTABLE_MIN_MV);
    assert_eq!(FUSB302B_INITIAL_PPS_REQUEST_MV, 12_000);
}
#[test]
fn zeroize_bytes_scrubs_reusable_heap_workspace() {
    let mut bytes = [0xFA, 0x00, 0xF4, 0x01, 0xA5, 0x5A];
    zeroize_bytes_volatile(&mut bytes);
    assert_eq!(bytes, [0; 6]);
}

#[test]
fn same_frequency_buzzer_retrigger_does_not_reconfigure_timer() {
    let configured_frequency_hz = 1_080;
    let active_state = BuzzerHardwareState {
        frequency_hz: Some(1_080),
        duty_percent: 50,
        generation: 7,
    };
    let retriggered_state = BuzzerHardwareState {
        generation: 8,
        ..active_state
    };

    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        retriggered_state
    ));
}

#[test]
fn buzzer_timer_readback_distinguishes_the_heater_on_tone_steps() {
    let low_period = buzzer_timer_period_ticks(1_240).unwrap();
    let high_period = buzzer_timer_period_ticks(1_680).unwrap();

    assert_ne!(low_period, high_period);
    assert_ne!(
        mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, low_period),
        mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, high_period)
    );
}

#[test]
fn buzzer_timer_keeps_one_prescaler_and_represents_every_production_frequency() {
    for frequency_hz in [
        320, 360, 420, 480, 900, 1_080, 1_200, 1_240, 1_550, 1_650, 1_680, 2_200, 2_300,
    ] {
        let period_ticks = buzzer_timer_period_ticks(frequency_hz)
            .expect("every production cue frequency must fit Timer2");
        let applied_frequency_hz = mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, period_ticks);
        assert!(
            applied_frequency_hz.abs_diff(frequency_hz) <= 1,
            "requested {frequency_hz} Hz, got {applied_frequency_hz} Hz"
        );
    }
}

#[test]
fn buzzer_pad_edge_observation_distinguishes_active_cooling_tones() {
    assert_eq!(buzzer_observed_frequency_hz(41, 45), Some(911));
    assert_eq!(buzzer_observed_frequency_hz(54, 45), Some(1_200));
    assert_eq!(buzzer_observed_frequency_hz(112, 70), Some(1_600));
    assert_eq!(buzzer_observed_frequency_hz(0, 0), None);
}

#[test]
fn active_cooling_tone_steps_quiet_and_stop_timer_before_each_retune() {
    let mut buzzer = BuzzerArbiter::new();
    let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
    let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent,
        generation: output.generation,
    };

    let _ = buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::ActiveCoolingOn, 0);
    let first_tone = hardware_state(buzzer.output());
    assert_eq!(
        buzzer_hardware_actions(configured_frequency_hz, first_tone).as_slice(),
        [
            BuzzerHardwareAction::SetDutyPercent(0),
            BuzzerHardwareAction::StopTimer,
            BuzzerHardwareAction::Retune(900),
            BuzzerHardwareAction::SetDutyPercent(50),
        ]
    );
    configured_frequency_hz = first_tone.frequency_hz.unwrap();

    let first_rest = hardware_state(buzzer.tick(45).output);
    assert_eq!(
        buzzer_hardware_actions(configured_frequency_hz, first_rest).as_slice(),
        [BuzzerHardwareAction::SetDutyPercent(0)]
    );

    let second_tone = hardware_state(buzzer.tick(70).output);
    assert_eq!(
        buzzer_hardware_actions(configured_frequency_hz, second_tone).as_slice(),
        [
            BuzzerHardwareAction::SetDutyPercent(0),
            BuzzerHardwareAction::StopTimer,
            BuzzerHardwareAction::Retune(1_200),
            BuzzerHardwareAction::SetDutyPercent(50),
        ]
    );
    configured_frequency_hz = second_tone.frequency_hz.unwrap();

    let second_rest = hardware_state(buzzer.tick(115).output);
    assert_eq!(
        buzzer_hardware_actions(configured_frequency_hz, second_rest).as_slice(),
        [BuzzerHardwareAction::SetDutyPercent(0)]
    );

    let third_tone = hardware_state(buzzer.tick(140).output);
    assert_eq!(
        buzzer_hardware_actions(configured_frequency_hz, third_tone).as_slice(),
        [
            BuzzerHardwareAction::SetDutyPercent(0),
            BuzzerHardwareAction::StopTimer,
            BuzzerHardwareAction::Retune(1_550),
            BuzzerHardwareAction::SetDutyPercent(50),
        ]
    );
}

#[test]
fn buzzer_timer_keeps_the_carrier_through_ui_input_silence() {
    let silent_state = BuzzerHardwareState {
        frequency_hz: None,
        duty_percent: 0,
        generation: 1,
    };
    let ui_input_state = BuzzerHardwareState {
        frequency_hz: Some(1_080),
        duty_percent: 50,
        generation: 2,
    };
    let heater_on_state = BuzzerHardwareState {
        frequency_hz: Some(1_240),
        duty_percent: 50,
        generation: 3,
    };

    assert!(buzzer_timer_reconfiguration_needed(
        BUZZER_IDLE_FREQUENCY_HZ,
        ui_input_state
    ));
    assert!(!buzzer_timer_reconfiguration_needed(
        ui_input_state.frequency_hz.unwrap(),
        silent_state
    ));
    assert!(!buzzer_timer_reconfiguration_needed(
        ui_input_state.frequency_hz.unwrap(),
        ui_input_state
    ));
    assert!(buzzer_timer_reconfiguration_needed(
        ui_input_state.frequency_hz.unwrap(),
        heater_on_state
    ));
}

#[test]
fn fast_ui_input_repeat_reuses_the_carrier_after_its_45ms_silence_gap() {
    let mut buzzer = BuzzerArbiter::new();
    let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
    let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent,
        generation: output.generation,
    };

    let _ = buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 0);
    let first_tone = hardware_state(buzzer.output());
    assert!(buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        first_tone
    ));
    configured_frequency_hz = first_tone.frequency_hz.unwrap();

    let silence = hardware_state(buzzer.tick(45).output);
    assert_eq!(silence.frequency_hz, None);
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        silence
    ));

    let _ = buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 60);
    let fast_repeat_tone = hardware_state(buzzer.output());
    assert_eq!(fast_repeat_tone.frequency_hz, Some(1_080));
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        fast_repeat_tone
    ));
}

#[test]
fn protection_alarm_reuses_one_carrier_through_its_pulses_and_replay() {
    let mut buzzer = BuzzerArbiter::new();
    let mut cadence = ProtectionAlarmCadence::new();
    let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
    let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent,
        generation: output.generation,
    };

    let _ = cadence.enter(&mut buzzer, 0);
    let first_pulse = hardware_state(buzzer.output());
    assert_eq!(first_pulse.frequency_hz, Some(2_300));
    assert!(buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        first_pulse
    ));
    configured_frequency_hz = first_pulse.frequency_hz.unwrap();

    let first_rest = hardware_state(buzzer.tick(90).output);
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        first_rest
    ));
    let second_pulse = hardware_state(buzzer.tick(130).output);
    assert_eq!(second_pulse.frequency_hz, Some(2_300));
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        second_pulse
    ));

    let second_rest = hardware_state(buzzer.tick(220).output);
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        second_rest
    ));
    let _ = buzzer.tick(300);

    let replay = cadence
        .tick(true, &mut buzzer, PROTECTION_ALARM_INTERVAL_MS)
        .expect("active protection replays at the production cadence");
    assert_eq!(replay.cue, BuzzerCueId::ProtectionAlarm);
    let replay_pulse = hardware_state(buzzer.output());
    assert_eq!(replay_pulse.frequency_hz, Some(2_300));
    assert!(!buzzer_timer_reconfiguration_needed(
        configured_frequency_hz,
        replay_pulse
    ));
}

struct FakeUsbTx {
    capacity: usize,
    auto_commits_full_packet: bool,
    rejects_empty_flush: bool,
    pending: std::vec::Vec<u8>,
    sent: std::vec::Vec<u8>,
    flush_count: usize,
    operation_count: usize,
}

impl FakeUsbTx {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            auto_commits_full_packet: true,
            rejects_empty_flush: false,
            pending: std::vec::Vec::new(),
            sent: std::vec::Vec::new(),
            flush_count: 0,
            operation_count: 0,
        }
    }

    fn auto_commit_rejecting_empty_flush(capacity: usize) -> Self {
        Self {
            rejects_empty_flush: true,
            ..Self::new(capacity)
        }
    }
}

impl UsbControlTx for FakeUsbTx {
    fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
        self.operation_count += 1;
        if self.pending.len() >= self.capacity {
            return Err(UsbTxError::WouldBlock);
        }
        self.pending.push(byte);
        if self.auto_commits_full_packet && self.pending.len() == self.capacity {
            self.sent.extend_from_slice(&self.pending);
            self.pending.clear();
        }
        Ok(())
    }

    fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
        self.operation_count += 1;
        self.flush_count += 1;
        if self.rejects_empty_flush && self.pending.is_empty() {
            return Err(UsbTxError::WouldBlock);
        }
        self.sent.extend_from_slice(&self.pending);
        self.pending.clear();
        Ok(())
    }

    fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
        Err(UsbTxError::WouldBlock)
    }
}

fn test_usb_runtime_status_context() -> UsbRuntimeStatusContext {
    UsbRuntimeStatusContext {
        elapsed_ms: 0,
        pd_controller: ControllerKind::Ch224q,
        last_pd_observation: None,
        heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            terminal_fixed_pd_disarmed: false,
        },
        pid_snapshot: HeaterPidSnapshot {
            duty_percent: 0,
            warmup_soft_start_percent: 0,
            error_c: 0.0,
            control_error_c: 0.0,
            filtered_temp_c: 0.0,
            filtered_slope_c_per_s: 0.0,
            coast_active: false,
            phase: HeaterControlPhase::Warmup,
        },
        heater_control_timing: HeaterControlTiming::default(),
        manual_pps: ManualPpsState::default(),
        calibration: CalibrationRuntimeState::default(),
        fan_command: FanHardwareCommand::disabled(),
        fan_working_power_pending: false,
        heater_physical_output_percent: 0,
        current_rtd_fault: None,
        heater_fault_latched: None,
        attention_pending_after_fault_clear: false,
        thermal_control_profile_preview: false,
        active_thermal_control_profile: None,
        last_raw_state: FrontPanelRawState::default(),
        latest_status_temp_c: 0.0,
        latest_control_temp_c: 0.0,
        control_measurement_guarded: false,
        latest_rtd_raw_adc_mv: 0,
        latest_rtd_raw_adc_min_mv: 0,
        latest_rtd_raw_adc_max_mv: 0,
        latest_vin_raw_adc_mv: 0,
        vin_mv: 12_000,
    }
}

#[test]
fn usb_write_bytes_flushes_full_fifo_without_truncating_large_frame() {
    let payload = std::vec![b'x'; 180];
    let mut tx = FakeUsbTx::new(64);

    assert!(usb_write_bytes_bounded(&mut tx, &payload));

    assert_eq!(tx.sent, payload);
    assert!(tx.flush_count >= 3);
    assert!(tx.pending.is_empty());
}

#[test]
fn usb_write_bytes_stops_on_hard_tx_error() {
    struct FailingUsbTx;

    impl UsbControlTx for FailingUsbTx {
        fn write_byte_nb(&mut self, _byte: u8) -> Result<(), UsbTxError> {
            Err(UsbTxError::Other)
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    assert!(!usb_write_bytes_bounded(&mut FailingUsbTx, b"x"));
}

#[test]
fn usb_write_bytes_returns_without_retrying_a_busy_endpoint() {
    struct DelayedUsbTx {
        write_attempts: usize,
    }

    impl UsbControlTx for DelayedUsbTx {
        fn write_byte_nb(&mut self, _byte: u8) -> Result<(), UsbTxError> {
            self.write_attempts += 1;
            Err(UsbTxError::WouldBlock)
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Err(UsbTxError::WouldBlock)
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Err(UsbTxError::WouldBlock)
        }
    }

    let mut tx = DelayedUsbTx { write_attempts: 0 };

    assert!(!usb_write_bytes_bounded(&mut tx, b"response\\n"));
    assert_eq!(tx.write_attempts, 1);
}

#[test]
fn usb_response_pump_promotes_hard_tx_failure_to_transport_fault() {
    struct FailingUsbTx;

    impl UsbControlTx for FailingUsbTx {
        fn write_byte_nb(&mut self, _byte: u8) -> Result<(), UsbTxError> {
            Err(UsbTxError::Other)
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    let payload = [b'x'; 1];
    let mut writer = UsbResponseWriter::new(&payload);
    let mut tx = FailingUsbTx;
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
    assert_eq!(
        usb_pump_response(&mut tx, &mut writer, &tx_buf, 0),
        UsbResponsePumpOutcome::Fault
    );
    assert!(writer.is_complete());
    assert!(USB_TRANSPORT_FAULT_MARKER.starts_with(b"\n{"));
    let _ = &mut tx_buf;
}

#[test]
fn usb_recovery_writer_resumes_after_a_partial_hard_failure() {
    struct PartialFailureTx {
        sent: std::vec::Vec<u8>,
        attempts: usize,
        fail_at: usize,
    }

    impl UsbControlTx for PartialFailureTx {
        fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
            if self.attempts == self.fail_at {
                self.attempts += 1;
                return Err(UsbTxError::Other);
            }
            self.attempts += 1;
            self.sent.push(byte);
            Ok(())
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    let payload = [b'r'; 96];
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
    tx_buf[..payload.len()].copy_from_slice(&payload);
    let mut writer = UsbResponseWriter::new(&payload);
    let mut tx = PartialFailureTx {
        sent: std::vec::Vec::new(),
        attempts: 0,
        fail_at: 7,
    };

    assert_eq!(
        usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, 0),
        UsbResponsePumpOutcome::Pending
    );
    for now_ms in [1, 2, 3, 4] {
        if usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, now_ms)
            == UsbResponsePumpOutcome::Idle
        {
            break;
        }
    }

    assert!(writer.is_complete());
    assert_eq!(tx.sent, payload);
}

#[test]
fn usb_short_packet_ack_is_polled_without_resubmitting_an_empty_packet() {
    #[derive(Default)]
    struct AsyncPacketTx {
        bytes: std::vec::Vec<u8>,
        acknowledged: bool,
        submissions: usize,
    }

    impl UsbControlTx for AsyncPacketTx {
        fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
            self.bytes.push(byte);
            Ok(())
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            // WR_DONE submits a packet, including a zero-length packet when
            // the previous packet was already consumed by the USB host.
            self.submissions += 1;
            self.acknowledged = false;
            Err(UsbTxError::WouldBlock)
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            if self.acknowledged {
                Ok(())
            } else {
                Err(UsbTxError::WouldBlock)
            }
        }
    }

    for recovery in [false, true] {
        let payload = b"short response\n";
        let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
        tx_buf[..payload.len()].copy_from_slice(payload);
        let mut writer = UsbResponseWriter::new(payload);
        let mut tx = AsyncPacketTx::default();
        let pump = if recovery {
            usb_pump_recovery_response::<AsyncPacketTx>
        } else {
            usb_pump_response::<AsyncPacketTx>
        };

        assert_eq!(
            pump(&mut tx, &mut writer, &tx_buf, 0),
            UsbResponsePumpOutcome::Pending
        );
        assert_eq!(
            pump(&mut tx, &mut writer, &tx_buf, 1),
            UsbResponsePumpOutcome::Pending
        );
        assert_eq!(
            pump(&mut tx, &mut writer, &tx_buf, 2),
            UsbResponsePumpOutcome::Pending
        );
        assert_eq!(
            tx.submissions, 1,
            "an absent host must not cause repeated submissions"
        );
        tx.acknowledged = true;
        assert_eq!(
            pump(&mut tx, &mut writer, &tx_buf, 3),
            UsbResponsePumpOutcome::Idle
        );
        assert_eq!(tx.submissions, 1);
        assert_eq!(tx.bytes, payload);
    }
}

#[test]
fn usb_recovery_writer_resumes_after_host_absence_exceeds_the_response_deadline() {
    let payload = USB_TRANSPORT_FAULT_MARKER;
    let mut writer = UsbResponseWriter::default();
    writer.start(payload.len(), 10);
    let mut tx = FakeUsbTx::new(64);
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
    tx_buf[..payload.len()].copy_from_slice(payload);

    assert_eq!(
        usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, 0),
        UsbResponsePumpOutcome::Pending
    );
    assert_eq!(tx.sent, payload[..64]);
    tx.capacity = 0;

    assert_eq!(
        usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, 10),
        UsbResponsePumpOutcome::Pending
    );
    assert!(!writer.is_complete());
    assert_eq!(tx.sent, payload[..64]);
    assert_eq!(
        usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, 20_000),
        UsbResponsePumpOutcome::Pending
    );
    tx.capacity = 64;
    for now_ms in 20_001..20_010 {
        if usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, now_ms)
            == UsbResponsePumpOutcome::Idle
        {
            break;
        }
    }
    assert!(writer.is_complete());
    assert_eq!(tx.sent, payload);
}

#[test]
fn deferred_persistence_log_enters_recovery_after_a_partial_hard_failure() {
    struct PartialFailureTx {
        sent: std::vec::Vec<u8>,
        attempts: usize,
        fail_at: usize,
    }

    impl UsbControlTx for PartialFailureTx {
        fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
            if self.attempts == self.fail_at {
                self.attempts += 1;
                return Err(UsbTxError::Other);
            }
            self.attempts += 1;
            self.sent.push(byte);
            Ok(())
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    let line = b"PERSISTENCE_COMMIT_FAILED code=test phase=runtime attempt=1\n";
    let mut sink = DeferredPersistenceLogSink::default();
    sink.write_line(line);
    let mut tx = PartialFailureTx {
        sent: std::vec::Vec::new(),
        attempts: 0,
        fail_at: 5,
    };

    assert!(!sink.flush_one(&mut tx));
    let sent_before_retry = tx.sent.clone();
    assert!(!sink.flush_one(&mut tx));
    assert_eq!(tx.sent, sent_before_retry);
    assert!(sink.take_transport_fault());
    assert!(!sink.is_pending());
}

#[test]
fn deferred_persistence_log_hard_failure_enters_transport_recovery() {
    struct FailingUsbTx;

    impl UsbControlTx for FailingUsbTx {
        fn write_byte_nb(&mut self, _byte: u8) -> Result<(), UsbTxError> {
            Err(UsbTxError::Other)
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    let mut sink = DeferredPersistenceLogSink::default();
    sink.write_line(b"PERSISTENCE_COMMIT_FAILED code=test\n");
    assert!(!sink.flush_one(&mut FailingUsbTx));
    assert!(sink.take_transport_fault());
    assert!(!sink.take_transport_fault());
}

#[test]
fn mutating_request_history_rejects_a_b_a_and_keeps_failed_mutations_out() {
    let mut history = heapless::Deque::new();
    let mut request_a = heapless::String::new();
    request_a.push_str("request-a").unwrap();
    let mut request_b = heapless::String::new();
    request_b.push_str("request-b").unwrap();

    assert!(!usb_mutating_request_id_is_recent(&history, &request_a));
    remember_mutating_request_id(&mut history, request_a.clone());
    remember_mutating_request_id(&mut history, request_b);
    assert!(usb_mutating_request_id_is_recent(&history, &request_a));

    let failed = usb_error_response(request_a.clone(), "memory_commit_failed", "failed");
    assert!(!usb_mutation_succeeded(&failed));
    let success = usb_response(request_a.clone(), UsbResponsePayload::Ack);
    assert!(usb_mutation_succeeded(&success));
}

#[test]
fn oversized_usb_line_discards_the_suffix_until_newline() {
    let mut line = heapless::String::<USB_CONTROL_LINE_CAPACITY>::new();
    let mut overflowed = false;
    for _ in 0..USB_CONTROL_LINE_CAPACITY {
        append_usb_control_byte(&mut line, &mut overflowed, b'x');
    }
    assert!(!overflowed);
    assert_eq!(line.len(), USB_CONTROL_LINE_CAPACITY);
    append_usb_control_byte(&mut line, &mut overflowed, b'{');
    append_usb_control_byte(&mut line, &mut overflowed, b'}');

    assert!(overflowed);
    assert!(line.is_empty());
    match usb_line_too_long_response() {
        UsbFrame::Error { error, .. } => assert_eq!(error.code.as_str(), "frame_too_large"),
        other => panic!("unexpected oversized frame response: {other:?}"),
    }
}

#[test]
fn mutating_usb_requests_are_deduplicated_but_reads_are_not() {
    assert_eq!(
        usb_mutating_request_id(
            r#"{"type":"runtime_config","requestId":"runtime-1","targetTempC":180}"#
        )
        .as_deref(),
        Some("runtime-1")
    );
    assert!(
        usb_mutating_request_id(r#"{"type":"request","requestId":"status-1","op":"get_status"}"#)
            .is_none()
    );
    assert_eq!(
        usb_mutating_request_id(
            r#"{"type":"eeprom_maintenance","requestId":"erase-1","op":"erase"}"#
        )
        .as_deref(),
        Some("erase-1")
    );
}

#[test]
fn usb_tx_buffer_matches_the_eight_kibibyte_jsonl_contract() {
    assert_eq!(
        USB_CONTROL_TX_BUFFER_LEN,
        flux_purr_firmware::control_plane::USB_LINE_MAX_LEN
    );
    assert_eq!(
        USB_CONTROL_LINE_CAPACITY,
        flux_purr_firmware::control_plane::USB_LINE_MAX_LEN - 1
    );
}

#[test]
fn usb_response_pump_reports_idle_after_the_last_packet_is_accepted() {
    let payload = [b'x'; 1];
    let mut tx = FakeUsbTx::new(64);
    let mut writer = UsbResponseWriter::new(&payload);
    let tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

    assert_eq!(
        usb_pump_response(&mut tx, &mut writer, &tx_buf, 0),
        UsbResponsePumpOutcome::Pending
    );
    assert_eq!(
        usb_pump_response(&mut tx, &mut writer, &tx_buf, 1),
        UsbResponsePumpOutcome::Idle
    );
    assert!(writer.is_complete());
}

#[test]
fn usb_response_write_defaults_to_bounded_chunks_for_host_requested_frames() {
    let payload = std::vec![b'x'; 180];
    let mut bounded_tx = FakeUsbTx::new(0);
    assert!(!usb_write_bytes_bounded(&mut bounded_tx, &payload));

    let mut response_tx = FakeUsbTx::new(64);
    let mut request_id = heapless::String::new();
    request_id.push_str("response-write").unwrap();
    let response = usb_response(
        request_id,
        UsbResponsePayload::Identity(Box::new(Identity::firmware_default())),
    );
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

    usb_write_response_frame_to(&mut response_tx, &response, &mut tx_buf);

    let line = core::str::from_utf8(&response_tx.sent).expect("response frame is utf8");
    assert!(line.contains(r#""requestId":"response-write""#));
    assert!(line.ends_with('\n'));
    assert!(response_tx.flush_count > 1);
}

#[test]
fn usb_response_write_uses_nonblocking_transport_calls() {
    struct NonblockingUsbTx {
        sent: std::vec::Vec<u8>,
    }

    impl UsbControlTx for NonblockingUsbTx {
        fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
            self.sent.push(byte);
            Ok(())
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }

        fn poll_flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            Ok(())
        }
    }

    let mut request_id = heapless::String::new();
    request_id.push_str("confirmed-response").unwrap();
    let response = usb_response(
        request_id,
        UsbResponsePayload::Identity(Box::new(Identity::firmware_default())),
    );
    let mut tx = NonblockingUsbTx {
        sent: std::vec::Vec::new(),
    };
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

    usb_write_response_frame_to(&mut tx, &response, &mut tx_buf);

    let line = core::str::from_utf8(&tx.sent).expect("response is utf8");
    assert!(line.contains(r#""requestId":"confirmed-response""#));
}

#[test]
fn usb_response_writer_limits_an_always_ready_endpoint_to_one_packet_per_step() {
    let payload = std::vec![b'x'; 180];
    let mut tx = FakeUsbTx::new(64);
    let mut writer = UsbResponseWriter::new(&payload);
    let mut operations_before = tx.operation_count;
    let mut steps = 0;

    while !writer.is_complete() {
        let complete = writer.step(&mut tx, &payload).unwrap_or(false);
        assert!(complete == writer.is_complete());
        assert!(
            tx.operation_count.saturating_sub(operations_before) <= USB_CONTROL_TX_PACKET_LEN + 1
        );
        operations_before = tx.operation_count;
        steps += 1;
        assert!(steps <= 10);
    }

    assert_eq!(tx.sent, payload);
    assert!(tx.pending.is_empty());
    assert_eq!(tx.flush_count, 1);

    let exact_packet = std::vec![b'y'; USB_CONTROL_TX_PACKET_LEN];
    let mut exact_packet_tx =
        FakeUsbTx::auto_commit_rejecting_empty_flush(USB_CONTROL_TX_PACKET_LEN);
    let mut exact_packet_writer = UsbResponseWriter::new(&exact_packet);
    assert!(
        exact_packet_writer
            .step(&mut exact_packet_tx, &exact_packet)
            .expect("an auto-committed full packet completes without a flush")
    );
    assert!(exact_packet_writer.is_complete());
    assert_eq!(exact_packet_tx.sent, exact_packet);
    assert_eq!(exact_packet_tx.flush_count, 0);

    writer.start(payload.len(), 10);
    assert!(writer.is_expired(10));
    writer.abort();
    assert!(writer.is_complete());
}

#[test]
fn rtd_capture_expected_mv_uses_target_adc_before_temperature_curve() {
    let config = CalibrationConfigCommand {
        op: CalibrationConfigOp::Capture,
        channel: Some(CalibrationChannelWire::RtdAdc),
        reference_temp_c: Some(49.0),
        reference_vin_mv: None,
        target_adc_mv: Some(1_000),
        observed_mv: None,
        expected_mv: None,
        sample_index: None,
        state: None,
        slot: None,
        fit: None,
    };

    assert_eq!(
        expected_calibration_adc_mv(&config, CalibrationChannelWire::RtdAdc),
        Some(1_000)
    );
}

#[test]
fn rtd_capture_expected_mv_requires_target_adc_without_explicit_expected() {
    let config = CalibrationConfigCommand {
        op: CalibrationConfigOp::Capture,
        channel: Some(CalibrationChannelWire::RtdAdc),
        reference_temp_c: Some(49.0),
        reference_vin_mv: None,
        target_adc_mv: None,
        observed_mv: None,
        expected_mv: None,
        sample_index: None,
        state: None,
        slot: None,
        fit: None,
    };

    assert_eq!(
        expected_calibration_adc_mv(&config, CalibrationChannelWire::RtdAdc),
        None
    );
}

#[test]
fn early_usb_control_answers_identity_before_runtime_ready() {
    let mut tx = FakeUsbTx::new(64);
    let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
    let response = usb_early_response(
        r#"{"type":"request","requestId":"boot-id","op":"get_identity"}"#,
        &MemoryConfig::default(),
    );

    usb_write_response_frame_to(&mut tx, &response, &mut tx_buf);
    let line = core::str::from_utf8(&tx.sent).expect("early identity response is utf8");
    let parsed = parse_usb_frame(line).expect("early identity response is valid jsonl");

    match parsed {
        UsbFrame::Response {
            request_id,
            ok,
            result: Some(UsbResponsePayload::Identity(identity)),
            error: None,
        } => {
            assert_eq!(request_id.as_str(), "boot-id");
            assert!(ok);
            assert_eq!(identity.protocol_version.as_str(), "flux-purr.usb.v1");
            assert_eq!(identity.device_id.as_str(), "a0f262f20d6c");
            assert_eq!(identity.hostname.as_str(), "flux-purr-a0f262f20d6c");
        }
        other => panic!("unexpected early identity response: {other:?}"),
    }
}

#[test]
fn early_usb_control_defers_network_until_main_loop() {
    let mut config = MemoryConfig::default();
    config.wifi_ssid.push_str("bench-net").unwrap();
    let response = usb_early_response(
        r#"{"type":"request","requestId":"boot-net","op":"get_network"}"#,
        &config,
    );

    match response {
        UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        } => {
            assert_eq!(request_id.as_str(), "boot-net");
            assert_eq!(error.code.as_str(), "startup_busy");
            assert!(error.retryable);
        }
        other => panic!("unexpected early network response: {other:?}"),
    }
}

#[test]
fn early_usb_control_defers_runtime_status_until_main_loop() {
    let response = usb_early_response(
        r#"{"type":"request","requestId":"boot-status","op":"get_status"}"#,
        &MemoryConfig::default(),
    );

    match response {
        UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        } => {
            assert_eq!(request_id.as_str(), "boot-status");
            assert_eq!(error.code.as_str(), "startup_busy");
            assert!(error.retryable);
        }
        other => panic!("unexpected early status response: {other:?}"),
    }
}

#[test]
fn thermal_tuning_is_unavailable_during_startup_and_recovery() {
    let memory_config = MemoryConfig::default();
    for request in [
        r#"{"type":"thermal_tuning","requestId":"tune-startup","op":"read"}"#,
        r#"{"type":"request","requestId":"tune-startup","op":"get_thermal_tuning"}"#,
    ] {
        for (response, expected_code) in [
            (usb_early_response(request, &memory_config), "startup_busy"),
            (
                usb_recovery_response(request, &memory_config, 0),
                "hardware_bringup_failed",
            ),
        ] {
            match response {
                UsbFrame::Response {
                    request_id,
                    ok: false,
                    result: None,
                    error: Some(error),
                } => {
                    assert_eq!(request_id.as_str(), "tune-startup");
                    assert_eq!(error.code.as_str(), expected_code);
                }
                other => panic!("unexpected thermal tuning response: {other:?}"),
            }
        }
    }
}

#[test]
fn startup_recovery_defers_network_and_status_until_persistent_state_is_ready() {
    let memory_config = MemoryConfig::default();
    for (request_id, request) in [
        (
            "recovery-net",
            r#"{"type":"request","requestId":"recovery-net","op":"get_network"}"#,
        ),
        (
            "recovery-status",
            r#"{"type":"request","requestId":"recovery-status","op":"get_status"}"#,
        ),
    ] {
        let response = usb_recovery_response_for_phase(
            request,
            &memory_config,
            0,
            UsbRecoveryPhase::BeforePersistentState,
        );

        match response {
            UsbFrame::Response {
                request_id: actual_request_id,
                ok: false,
                result: None,
                error: Some(error),
            } => {
                assert_eq!(actual_request_id.as_str(), request_id);
                assert_eq!(error.code.as_str(), "startup_busy");
                assert!(error.retryable);
            }
            other => panic!("unexpected startup recovery response: {other:?}"),
        }
    }
}

#[test]
fn recovery_usb_control_reports_fault_status_when_bringup_fails() {
    let mut memory_config = MemoryConfig {
        target_temp_c: 215,
        ..MemoryConfig::default()
    };
    memory_config.wifi_ssid.push_str("bench-net").unwrap();
    let response = usb_recovery_response_for_phase(
        r#"{"type":"request","requestId":"recovery-status","op":"get_status"}"#,
        &memory_config,
        7_200,
        UsbRecoveryPhase::RuntimeFault,
    );

    match response {
        UsbFrame::Response {
            request_id,
            ok: true,
            result: Some(UsbResponsePayload::Status(status)),
            error: None,
        } => {
            assert_eq!(request_id.as_str(), "recovery-status");
            assert_eq!(
                status.mode,
                flux_purr_firmware::control_plane::DeviceModeWire::Fault
            );
            assert_eq!(status.uptime_seconds, 7);
            assert_eq!(status.target_temp_c, 215);
            assert_eq!(
                status.network.ssid.as_ref().map(|ssid| ssid.as_str()),
                Some("bench-net")
            );
        }
        other => panic!("unexpected recovery status response: {other:?}"),
    }
}

#[test]
fn flash_preparation_queries_do_not_fabricate_outputs_before_runtime_is_available() {
    let memory_config = MemoryConfig::default();
    for (phase, expected_error) in [
        (UsbRecoveryPhase::BeforePersistentState, "startup_busy"),
        (UsbRecoveryPhase::RuntimeFault, "hardware_bringup_failed"),
    ] {
        for op in [
            "prepare_flash",
            "get_flash_preparation",
            "cancel_flash_preparation",
        ] {
            let request = format!(
                "{{\"type\":\"request\",\"requestId\":\"prep-unavailable\",\"op\":\"{op}\"}}"
            );
            let response = usb_recovery_response_for_phase(&request, &memory_config, 0, phase);
            match response {
                UsbFrame::Response {
                    request_id,
                    ok: false,
                    result: None,
                    error: Some(error),
                } => {
                    assert_eq!(request_id.as_str(), "prep-unavailable");
                    assert_eq!(error.code.as_str(), expected_error);
                    assert!(error.retryable);
                }
                other => panic!("unexpected unavailable preparation response: {other:?}"),
            }
        }
    }
}

#[test]
fn runtime_config_response_returns_updated_status_payload() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-1").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig {
        target_temp_c: 180,
        active_cooling_enabled: true,
        ..MemoryConfig::default()
    };
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = None;

    let (response, _) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: Some(240),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: Some(false),
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: Some(false),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        UsbRuntimeStatusContext {
            elapsed_ms: 12_000,
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    match response {
        UsbFrame::Response {
            request_id,
            ok: true,
            result: Some(UsbResponsePayload::Status(status)),
            error: None,
        } => {
            assert_eq!(request_id.as_str(), "runtime-1");
            assert_eq!(status.target_temp_c, 240);
            assert!(!status.active_cooling_enabled);
            assert!(!status.heater_enabled);
            assert_eq!(status.uptime_seconds, 12);
            assert_eq!(memory_config.target_temp_c, 240);
            assert!(!memory_config.active_cooling_enabled);
        }
        other => panic!("unexpected runtime config response: {other:?}"),
    }
}

#[test]
fn runtime_config_response_rejects_heater_arm_when_thermal_model_is_missing() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-heater-model-missing").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = None;

    let (response, _) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: Some(true),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        test_usb_runtime_status_context(),
    );

    match response {
        UsbFrame::Response {
            ok: true,
            result: Some(UsbResponsePayload::Status(status)),
            error: None,
            ..
        } => {
            assert!(!status.heater_enabled);
            assert_eq!(status.heater_output_percent, 0);
            assert_eq!(status.heater_physical_output_percent, 0);
            assert!(!ui_state.heater_enabled);
        }
        other => panic!("unexpected runtime config response: {other:?}"),
    }
}

#[test]
fn runtime_config_cannot_override_a_running_thermal_plant_job() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-thermal-job-busy").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = None;
    let mut context = test_usb_runtime_status_context();
    context.calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };

    let (response, returned_calibration) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: Some(true),
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        context,
    );

    match response {
        UsbFrame::Response {
            ok: false,
            error: Some(error),
            ..
        } => assert_eq!(error.code.as_str(), "manual_pps_calibration_busy"),
        other => panic!("unexpected runtime response: {other:?}"),
    }
    assert_eq!(
        returned_calibration.job.status,
        CalibrationJobStatus::Running
    );
    assert!(!ui_state.heater_enabled);
}

#[test]
fn runtime_config_does_not_preview_profile_when_later_validation_fails() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-profile-fail").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = None;
    let mut profile_points = [None; FRONTPANEL_PRESET_COUNT];
    profile_points[0] = Some(ThermalControlProfilePointWire {
        target_temp_c: 120,
        brake_distance_centi_c: 700,
        warmup_power_permille: 320,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 320,
        approach_floor_power_permille: 220,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 220,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: 0,
        hold_on_centi_c: 0,
        hold_off_centi_c: 0,
        overshoot_cutoff_centi_c: 0,
        hold_kp_permille_per_c: 0,
        hold_ki_permille_per_c_tick: 0,
        hold_blend_ticks: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    });

    let (response, _) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: Some(true),
            manual_pps_mv: Some(10_400),
            manual_pps_ma: Some(2_500),
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::Preview,
                bank: None,
                profile: Some(ThermalControlProfileWire {
                    settings: None,
                    points: profile_points,
                }),
            }),
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        UsbRuntimeStatusContext {
            elapsed_ms: 1_000,
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    match response {
        UsbFrame::Response {
            ok: false,
            error: Some(error),
            ..
        } => {
            assert_eq!(error.code.as_str(), "manual_pps_no_capability");
            assert!(thermal_profile_preview.is_none());
        }
        other => panic!("unexpected runtime config response: {other:?}"),
    }
}

#[test]
fn runtime_config_rejects_clear_preview_with_profile_payload() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-profile-clear").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = None;
    let mut profile_points = [None; FRONTPANEL_PRESET_COUNT];
    profile_points[0] = Some(ThermalControlProfilePointWire {
        target_temp_c: 120,
        brake_distance_centi_c: 700,
        warmup_power_permille: 320,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 320,
        approach_floor_power_permille: 220,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 220,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: 0,
        hold_on_centi_c: 0,
        hold_off_centi_c: 0,
        overshoot_cutoff_centi_c: 0,
        hold_kp_permille_per_c: 0,
        hold_ki_permille_per_c_tick: 0,
        hold_blend_ticks: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    });

    let (response, _) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::ClearPreview,
                bank: None,
                profile: Some(ThermalControlProfileWire {
                    settings: None,
                    points: profile_points,
                }),
            }),
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        UsbRuntimeStatusContext {
            elapsed_ms: 1_000,
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    match response {
        UsbFrame::Response {
            ok: false,
            error: Some(error),
            ..
        } => {
            assert_eq!(error.code.as_str(), "thermal_profile_clear_payload");
            assert!(thermal_profile_preview.is_none());
        }
        other => panic!("unexpected runtime config response: {other:?}"),
    }
}

#[test]
fn runtime_config_saves_thermal_profile_to_memory() {
    let mut request_id = heapless::String::new();
    request_id.push_str("runtime-profile-save").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps = ManualPpsState::default();
    let mut thermal_profile_preview = Some(ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [None; FRONTPANEL_PRESET_COUNT],
    });
    let profile_points = saved_thermal_profile_points();

    let (response, _) = usb_runtime_config_response(
        request_id,
        RuntimeConfigCommand {
            target_temp_c: Some(210),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: Some(ThermalControlProfileCommand {
                op: ThermalControlProfileOp::Save,
                bank: None,
                profile: Some(ThermalControlProfileWire {
                    settings: None,
                    points: profile_points,
                }),
            }),
        },
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        UsbRuntimeStatusContext {
            elapsed_ms: 1_000,
            thermal_control_profile_preview: true,
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_saved_thermal_profile_response(response, &memory_config, &thermal_profile_preview);
}

fn saved_thermal_profile_points()
-> [Option<ThermalControlProfilePointWire>; FRONTPANEL_PRESET_COUNT] {
    let mut points = [None; FRONTPANEL_PRESET_COUNT];
    points[0] = Some(ThermalControlProfilePointWire {
        target_temp_c: 210,
        brake_distance_centi_c: 1_000,
        warmup_power_permille: 260,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 260,
        approach_floor_power_permille: 180,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 180,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: 0,
        hold_on_centi_c: 0,
        hold_off_centi_c: 0,
        overshoot_cutoff_centi_c: 0,
        hold_kp_permille_per_c: 0,
        hold_ki_permille_per_c_tick: 0,
        hold_blend_ticks: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    });
    points
}

fn assert_saved_thermal_profile_response(
    response: UsbFrame,
    memory_config: &MemoryConfig,
    thermal_profile_preview: &Option<ThermalControlProfile>,
) {
    let UsbFrame::Response {
        ok: true,
        result: Some(UsbResponsePayload::Status(status)),
        error: None,
        ..
    } = response
    else {
        panic!("unexpected runtime config response: {response:?}");
    };
    assert!(!status.thermal_control_profile_preview);
    assert!(thermal_profile_preview.is_none());
    assert!(status.thermal_control.profile_active);
    assert!(status.thermal_control.profile_covers_target);
    assert_eq!(status.thermal_control.profile_source.as_str(), "saved");
    assert_eq!(status.thermal_control.warmup_power_permille, 1_000);
    assert_eq!(
        status.thermal_control.approach_damping_exponent_permille,
        1_000
    );
    assert_eq!(
        memory_config.active_thermal_control_profile.points[0],
        Some(saved_thermal_profile_point_config())
    );
}

fn saved_thermal_profile_point_config() -> ThermalControlProfilePointConfig {
    ThermalControlProfilePointConfig {
        target_temp_c: 210,
        brake_distance_centi_c: 1_000,
        warmup_power_permille: 260,
        warmup_reenter_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
        approach_power_permille: 260,
        approach_floor_power_permille: 180,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 180,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
        hold_exit_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
        hold_on_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
        hold_off_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
        overshoot_cutoff_centi_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
        hold_kp_permille_per_c:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
        hold_ki_permille_per_c_tick:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
        hold_blend_ticks:
            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    }
}

#[test]
fn saved_thermal_profile_converts_to_controller_profile() {
    let mut config = ThermalControlProfileConfig::default();
    config.points[0] = Some(ThermalControlProfilePointConfig {
        target_temp_c: 210,
        brake_distance_centi_c: 1_000,
        warmup_power_permille: 260,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 260,
        approach_floor_power_permille: 180,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 180,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: 0,
        hold_on_centi_c: 0,
        hold_off_centi_c: 0,
        overshoot_cutoff_centi_c: 0,
        hold_kp_permille_per_c: 0,
        hold_ki_permille_per_c_tick: 0,
        hold_blend_ticks: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    });

    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    let target = profile.control_target(210);

    assert_eq!(target.brake_distance_c, 10.0);
    assert_eq!(target.approach_power_permille, 260);
    assert_eq!(target.approach_floor_power_permille, 180);
    assert_eq!(target.hold_power_permille, 180);
    assert_eq!(target.hold_reheat_power_permille, 180);
}

#[test]
fn thermal_profile_settings_conversion_clamps_direct_preview_values() {
    let settings = ThermalControlProfileSettings::from(ThermalControlProfileSettingsConfig {
        temp_filter_alpha_permille: u16::MAX,
        warmup_reenter_centi_c: u16::MAX,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: u16::MAX,
        hold_on_centi_c: 0,
        hold_off_centi_c: u16::MAX,
        overshoot_cutoff_centi_c: 0,
        approach_max_ticks: u16::MAX,
        approach_min_power_ratio_permille: u16::MAX,
        hold_kp_permille_per_c: u16::MAX,
        hold_ki_permille_per_c_tick: u16::MAX,
        hold_blend_ticks: u16::MAX,
        hold_reheat_power_permille: u16::MAX,
        approach_lead_ticks: u16::MAX,
        hold_lead_ticks: u16::MAX,
        auto_adjustable_working_floor_mv: u16::MAX,
        heater_current_reserve_ma: u16::MAX,
    });

    assert_eq!(settings.temp_filter_alpha, 1.0);
    assert_eq!(settings.warmup_reenter_error_c, 50.0);
    assert_eq!(settings.hold_entry_error_c, 0.01);
    assert_eq!(settings.auto_adjustable_working_floor_mv, 28_000);
    assert_eq!(settings.heater_current_reserve_ma, 1_000);
}

#[test]
fn runtime_status_exposes_heater_lock_reason_when_present() {
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    ui_state.heater_lock_reason = Some(HeaterLockReason::CoolingDisabledOvertemp);

    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            elapsed_ms: 3_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(
        status.heater_lock_reason.as_deref(),
        Some("cooling-disabled-overtemp")
    );
}

#[test]
fn runtime_status_exposes_heater_control_snapshot() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            pid_snapshot: HeaterPidSnapshot {
                duty_percent: 37,
                warmup_soft_start_percent: 100,
                error_c: -0.4,
                control_error_c: -0.2,
                filtered_temp_c: 140.2,
                filtered_slope_c_per_s: 0.6,
                coast_active: true,
                phase: HeaterControlPhase::Hold,
            },
            heater_control_timing: HeaterControlTiming {
                interval_ms: 120,
                cycle_ms: 7,
            },
            latest_control_temp_c: 139.8,
            control_measurement_guarded: true,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.heater_control_phase.as_deref(), Some("hold"));
    assert_eq!(status.heater_error_c, Some(-0.4));
    assert_eq!(status.heater_control_error_c, Some(-0.2));
    assert_eq!(status.heater_control_temp_c, Some(139.8));
    assert!(status.heater_control_measurement_guarded);
    assert_eq!(status.heater_filtered_temp_c, Some(140.2));
    assert_eq!(status.heater_filtered_slope_c_per_s, Some(0.6));
    assert!(status.heater_coast_active);
    assert_eq!(status.heater_control_interval_ms, 120);
    assert_eq!(status.heater_control_cycle_ms, 7);
}

#[test]
fn runtime_status_preserves_centi_c_temperature_telemetry() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            latest_status_temp_c: 140.237,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.board_temp_centi, 14_024);
    assert_eq!(status.current_temp_c, 140.24);
}

#[test]
fn runtime_status_reports_rtd_batch_extrema() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            latest_rtd_raw_adc_mv: 900,
            latest_rtd_raw_adc_min_mv: 899,
            latest_rtd_raw_adc_max_mv: 902,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.rtd_raw_adc_mv, 900);
    assert_eq!(status.rtd_divider_supply_mv, Some(RTD_DIVIDER_SUPPLY_MV));
    assert_eq!(status.rtd_raw_adc_min_mv, 899);
    assert_eq!(status.rtd_raw_adc_max_mv, 902);
    assert_eq!(status.rtd_raw_adc_spread_mv, 3);
}

#[test]
fn runtime_status_uses_live_rtd_sample_for_owner_facing_temperature() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            latest_status_temp_c: 141.499,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.board_temp_centi, 14_150);
    assert_eq!(status.current_temp_c, 141.5);
}

#[test]
fn runtime_status_preserves_control_filter_telemetry_separately_from_display_temperature() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            latest_status_temp_c: 73.74,
            pid_snapshot: HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 100,
                error_c: 0.998,
                control_error_c: 0.998,
                filtered_temp_c: 59.004,
                filtered_slope_c_per_s: -0.4,
                coast_active: false,
                phase: HeaterControlPhase::Hold,
            },
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.current_temp_c, 73.74);
    assert_eq!(status.board_temp_centi, 7_374);
    assert_eq!(status.heater_filtered_temp_c, Some(59.004));
}

#[test]
fn runtime_status_reports_backend_request_when_manual_pps_is_disabled() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            heater_power_backend: HeaterPowerBackend::PpsMos {
                pps_min_mv: 5_000,
                idle_request_mv: 12_000,
                pps_max_mv: 21_000,
                adjustable_max_mv: 21_000,
                capability_max_ma: 3_000,
                current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
                current_request_mv: 12_000,
                settle_until_ms: None,
                next_request_at_ms: 0,
                current_limit_fixed_pwm_active: false,
                current_limit_fixed_request_confirmed: false,
                terminal_fixed_pd_disarmed: false,
            },
            vin_mv: 12_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.pd_request_mv, 12_000);
    assert_eq!(status.pd_contract_mv, 12_000);
}

#[test]
fn manual_pps_config_validates_capability_and_updates_status_payload() {
    let mut request_id = heapless::String::new();
    request_id.push_str("manual-pps").unwrap();
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: false,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(24_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    let mut thermal_profile_preview = None;
    let context_manual_pps = manual_pps;
    let (response, _) = usb_runtime_config_response(
        request_id,
        manual_pps_enable_command(),
        &mut ui_state,
        &mut memory_config,
        &mut manual_pps,
        &mut thermal_profile_preview,
        UsbRuntimeStatusContext {
            elapsed_ms: 1_000,
            heater_power_backend: HeaterPowerBackend::PpsMos {
                pps_min_mv: 5_000,
                idle_request_mv: 12_000,
                pps_max_mv: 21_000,
                adjustable_max_mv: 21_000,
                capability_max_ma: 3_000,
                current_mode: None,
                current_request_mv: 12_000,
                settle_until_ms: None,
                next_request_at_ms: 0,
                current_limit_fixed_pwm_active: false,
                current_limit_fixed_request_confirmed: false,
                terminal_fixed_pd_disarmed: false,
            },
            manual_pps: context_manual_pps,
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );
    assert_manual_pps_enabled_response(response, &manual_pps, &ui_state);

    let error = apply_manual_pps_config(
        &manual_pps_invalid_voltage_command(),
        CalibrationRuntimeState::default(),
        &mut manual_pps,
    )
    .unwrap_err();
    assert_eq!(error, ManualPpsError::InvalidVoltage);

    apply_manual_pps_config(
        &manual_pps_disable_command(),
        CalibrationRuntimeState::default(),
        &mut manual_pps,
    )
    .unwrap();
    assert!(!manual_pps.enabled);
    assert_eq!(manual_pps.target_mv, None);
    assert_eq!(manual_pps.target_ma, None);
    assert!(manual_pps.consume_automatic_restore_pending());
}

#[test]
fn clearing_manual_pps_requires_coordinator_idle_release() {
    let mut manual_pps = ManualPpsState {
        enabled: true,
        owner: ManualPpsOwner::Debug,
        target_mv: Some(20_000),
        target_ma: Some(3_000),
        applied_mv: Some(20_000),
        ..ManualPpsState::default()
    };

    manual_pps.clear();

    assert_eq!(
        manual_pps.automatic_restore_phase(),
        AutomaticRestorePhase::RequestIdle,
    );
}

fn manual_pps_enable_command() -> RuntimeConfigCommand {
    RuntimeConfigCommand {
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: Some(true),
        manual_pps_mv: Some(10_400),
        manual_pps_ma: Some(2_500),
        fault_attention_acknowledged: None,
        calibration: None,
        thermal_profile_mode: None,
        thermal_control_profile: None,
    }
}

fn manual_pps_invalid_voltage_command() -> RuntimeConfigCommand {
    RuntimeConfigCommand {
        manual_pps_enabled: Some(true),
        manual_pps_mv: Some(10_450),
        manual_pps_ma: Some(2_500),
        ..manual_pps_enable_command()
    }
}

fn manual_pps_disable_command() -> RuntimeConfigCommand {
    RuntimeConfigCommand {
        manual_pps_enabled: Some(false),
        manual_pps_mv: None,
        manual_pps_ma: None,
        ..manual_pps_enable_command()
    }
}

fn assert_manual_pps_enabled_response(
    response: UsbFrame,
    manual_pps: &ManualPpsState,
    ui_state: &FrontPanelUiState,
) {
    let UsbFrame::Response {
        ok: true,
        result: Some(UsbResponsePayload::Status(status)),
        error: None,
        ..
    } = response
    else {
        panic!("unexpected manual PPS response: {response:?}");
    };
    assert!(manual_pps.enabled);
    assert!(ui_state.manual_pps_enabled);
    assert!(status.manual_pps_enabled);
    assert_eq!(status.manual_pps_mv, Some(10_400));
    assert_eq!(status.manual_pps_ma, Some(2_500));
    assert_eq!(status.pps_capability_min_mv, Some(5_000));
    assert_eq!(status.pps_capability_max_mv, Some(21_000));
    assert_eq!(status.pps_capability_max_ma, Some(3_000));
    assert_eq!(status.pd_contract_mv, 10_400);
    assert_eq!(status.pd_request_mv, 10_400);
    assert_eq!(status.manual_pps_error, None);
}

#[test]
fn manual_pps_config_cannot_take_over_a_running_thermal_plant_job() {
    let error = apply_manual_pps_config(
        &RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: Some(false),
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: None,
        },
        CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        },
        &mut ManualPpsState::default(),
    )
    .unwrap_err();

    assert_eq!(error, ManualPpsError::CalibrationInProgress);
}

#[test]
fn thermal_plant_job_rejects_calibration_control_mutation_atomically() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        heater_enabled: true,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };
    let before = calibration;
    let mut manual_pps = ManualPpsState::default();

    let error = apply_calibration_control_config(
        &CalibrationControlCommand {
            mode: Some(CalibrationModeWire::HeaterCurve),
            pps_enabled: Some(false),
            pps_mv: None,
            heater_enabled: Some(false),
            target_adc_mv: Some(1_000),
        },
        &mut calibration,
        &mut manual_pps,
    )
    .unwrap_err();

    assert_eq!(error, ManualPpsError::CalibrationInProgress);
    assert_eq!(calibration, before);
    assert_eq!(manual_pps, ManualPpsState::default());
}

#[test]
fn manual_calibration_control_cannot_select_thermal_plant_mode() {
    let mut calibration = CalibrationRuntimeState::default();
    let mut manual_pps = ManualPpsState::default();

    let error = apply_calibration_control_config(
        &CalibrationControlCommand {
            mode: Some(CalibrationModeWire::ThermalPlant),
            pps_enabled: None,
            pps_mv: None,
            heater_enabled: None,
            target_adc_mv: None,
        },
        &mut calibration,
        &mut manual_pps,
    )
    .expect_err("thermal plant mode is job-only");

    assert_eq!(error, ManualPpsError::ThermalPlantManagedByJob);
    assert_eq!(calibration, CalibrationRuntimeState::default());
    assert_eq!(manual_pps, ManualPpsState::default());
}

#[test]
fn calibration_control_uses_the_target_apdo_current_when_unspecified() {
    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_500),
        pps_max_mv: Some(28_000),
        pps_max_ma: Some(5_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 21_000,
        max_ma: 5_000,
    });
    capabilities.pps_apdos[1] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 28_000,
        max_ma: 3_000,
    });
    let mut calibration = CalibrationRuntimeState::default();
    let mut manual_pps = ManualPpsState::from_fusb302b_capabilities(Some(capabilities));

    apply_calibration_control_config(
        &CalibrationControlCommand {
            mode: Some(CalibrationModeWire::HeaterCurve),
            pps_enabled: Some(true),
            pps_mv: Some(24_000),
            heater_enabled: None,
            target_adc_mv: None,
        },
        &mut calibration,
        &mut manual_pps,
    )
    .expect("calibration PPS selects the APDO that covers 24V");

    assert_eq!(calibration.pps_mv, Some(24_000));
    assert_eq!(calibration.pps_ma, Some(3_000));
}

#[test]
fn leaving_calibration_mode_disarms_its_pps_override() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::HeaterCurve,
        pps_enabled: true,
        heater_enabled: true,
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(20_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }));
    manual_pps
        .enable(ManualPpsOwner::Calibration, 20_000, Some(3_000))
        .expect("calibration PPS applies");

    apply_calibration_control_config(
        &CalibrationControlCommand {
            mode: Some(CalibrationModeWire::Off),
            pps_enabled: None,
            pps_mv: None,
            heater_enabled: None,
            target_adc_mv: None,
        },
        &mut calibration,
        &mut manual_pps,
    )
    .expect("calibration mode exits");

    assert_eq!(calibration.mode, CalibrationMode::Off);
    assert!(!calibration.heater_enabled);
    assert!(!manual_pps.enabled);
    assert!(calibration.immediate_heater_disarm_pending);
    assert!(manual_pps.consume_automatic_restore_pending());
}

#[test]
fn transient_input_change_disarms_calibration_before_fixed_pd_restore() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::HeaterCurve,
        pps_enabled: true,
        pps_mv: Some(20_000),
        pps_ma: Some(3_000),
        heater_enabled: true,
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState {
        enabled: true,
        owner: ManualPpsOwner::Calibration,
        target_mv: Some(20_000),
        target_ma: Some(3_000),
        applied_mv: Some(20_000),
        ..ManualPpsState::default()
    };

    disarm_calibration_after_transient_input_change(&mut calibration, &mut manual_pps);

    assert!(!calibration.heater_enabled);
    assert!(!calibration.pps_enabled);
    assert_eq!(calibration.pps_mv, None);
    assert_eq!(calibration.pps_ma, None);
    assert!(calibration.immediate_heater_disarm_pending);
    assert!(!manual_pps.enabled);
    assert_eq!(manual_pps.target_mv, None);
    assert_eq!(manual_pps.applied_mv, None);
}

#[test]
fn capability_refresh_disarms_active_calibration_before_output() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::HeaterCurve,
        pps_enabled: true,
        pps_mv: Some(20_000),
        pps_ma: Some(3_000),
        heater_enabled: true,
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState::from_fusb302b_capabilities(Some(
        ch224q::AdjustablePowerCapabilities::default(),
    ));
    disarm_calibration_after_capability_refresh(&mut calibration, &mut manual_pps);

    assert!(!calibration.heater_enabled);
    assert!(!calibration.pps_enabled);
    assert_eq!(calibration.pps_mv, None);
    assert_eq!(calibration.pps_ma, None);
    assert!(calibration.immediate_heater_disarm_pending);
    assert!(!manual_pps.enabled);
}

#[test]
fn capability_refresh_preserves_a_running_thermal_plant_job() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        pps_enabled: true,
        pps_mv: Some(5_500),
        pps_ma: Some(3_000),
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            progress_percent: 2,
            samples_collected: 1,
            next_request_mv: Some(5_500),
            message: None,
        },
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState {
        enabled: true,
        owner: ManualPpsOwner::Calibration,
        target_mv: Some(5_500),
        target_ma: Some(3_000),
        applied_mv: Some(5_500),
        ..ManualPpsState::default()
    };

    disarm_calibration_after_capability_refresh(&mut calibration, &mut manual_pps);

    assert_eq!(calibration.mode, CalibrationMode::ThermalPlant);
    assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
    assert!(calibration.pps_enabled);
    assert!(manual_pps.enabled);
    assert_eq!(manual_pps.owner, ManualPpsOwner::Calibration);
}

#[test]
fn capability_refresh_keeps_passive_adc_calibration_available() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::RtdAdc,
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState::from_fusb302b_capabilities(Some(
        ch224q::AdjustablePowerCapabilities::default(),
    ));

    disarm_calibration_after_capability_refresh(&mut calibration, &mut manual_pps);

    assert!(!calibration.heater_enabled);
    assert!(!calibration.pps_enabled);
    assert!(!calibration.immediate_heater_disarm_pending);
    assert!(!manual_pps.enabled);
}

#[test]
fn thermal_plant_job_waits_for_a_pending_terminal_disarm() {
    let mut calibration = CalibrationRuntimeState {
        immediate_heater_disarm_pending: true,
        ..CalibrationRuntimeState::default()
    };

    assert_eq!(
        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut MemoryConfig::default(),
            &mut ManualPpsState::default(),
        ),
        Err(ManualPpsError::TerminalDisarmPending)
    );
    assert_eq!(
        apply_calibration_control_config(
            &CalibrationControlCommand {
                mode: Some(CalibrationModeWire::HeaterCurve),
                pps_enabled: Some(true),
                pps_mv: Some(20_000),
                heater_enabled: Some(true),
                target_adc_mv: None,
            },
            &mut calibration,
            &mut ManualPpsState::default(),
        ),
        Err(ManualPpsError::TerminalDisarmPending)
    );
}

#[test]
fn thermal_plant_job_locks_persistent_calibration_inputs() {
    let running = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };
    assert!(thermal_plant_calibration_job_running(running));

    let completed = CalibrationRuntimeState {
        job: CalibrationJobState {
            status: CalibrationJobStatus::Completed,
            ..running.job
        },
        ..running
    };
    assert!(!thermal_plant_calibration_job_running(completed));
}

#[test]
fn manual_pps_failure_clears_requested_current() {
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    manual_pps
        .enable(ManualPpsOwner::Debug, 10_400, Some(2_500))
        .unwrap();
    manual_pps.applied_mv = Some(10_400);
    manual_pps.fail(ManualPpsError::WriteFailed);

    assert!(!manual_pps.enabled);
    assert_eq!(manual_pps.target_mv, None);
    assert_eq!(manual_pps.target_ma, None);
    assert_eq!(manual_pps.applied_mv, None);
    assert_eq!(manual_pps.error, Some(ManualPpsError::WriteFailed));
    assert!(manual_pps.consume_automatic_restore_pending());
}

#[test]
fn manual_pps_current_validation_uses_matching_apdo() {
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(1_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 21_000,
                    max_ma: 1_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    manual_pps
        .enable(ManualPpsOwner::Debug, 10_400, Some(2_500))
        .unwrap();
    assert_eq!(
        manual_pps
            .enable(ManualPpsOwner::Debug, 20_000, Some(2_500))
            .unwrap_err(),
        ManualPpsError::InvalidVoltage
    );
}

#[test]
fn vin_auto_draft_selection_preserves_sweep_endpoints() {
    let mut collected = [None; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES];
    for (index, request_mv) in (5_000..=21_000).step_by(1_000).enumerate() {
        collected[index] = Some(AdcCalibrationSample {
            observed_mv: 280 + (index as u16 * 40),
            expected_mv: request_mv,
            reference_temp_deci_c: None,
            target_adc_mv: None,
            reference_vin_mv: Some(request_mv),
        });
    }

    let selected = select_vin_auto_draft_samples(&collected, 17);
    assert_eq!(selected.len(), ADC_CALIBRATION_MAX_SAMPLES);
    assert_eq!(
        selected.first().map(|sample| sample.expected_mv),
        Some(5_000)
    );
    assert_eq!(
        selected.last().map(|sample| sample.expected_mv),
        Some(21_000)
    );
    assert!(
        selected
            .windows(2)
            .all(|pair| pair[1].expected_mv > pair[0].expected_mv)
    );
}

#[test]
fn vin_auto_job_finishes_full_sweep_without_storage_overflow() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::VinAdc,
        pps_ma: Some(3_000),
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = MemoryConfig::default();
    memory_config
        .adc_calibration
        .vin
        .insert(AdcCalibrationSample {
            observed_mv: 999,
            expected_mv: 9_999,
            reference_temp_deci_c: None,
            target_adc_mv: None,
            reference_vin_mv: Some(9_999),
        });
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 21_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::VinAdc,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();
    assert_eq!(memory_config.adc_calibration.vin.sample_count(), 0);

    for step in 0..17u16 {
        let vin_raw_mv = 280 + (step * 45);
        let latest_vin_mv = u32::from(5_000 + (step * 1_000));
        for _ in 0..4 {
            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                CalibrationJobUpdateInput {
                    now_ms: 0,
                    latest_rtd_raw_code: 1_500,
                    latest_vin_raw_code: 1_000,
                    wifi_started: Some(false),
                    pd_confirmed: true,
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: vin_raw_mv,
                    latest_temp_c: 25.0,
                    latest_vin_mv,
                    heater_duty_percent: 0,
                },
            );
        }
    }

    assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
    assert_eq!(calibration.job.kind, Some(CalibrationJobKind::VinAdc));
    assert_eq!(calibration.job.samples_collected, 17);
    assert_eq!(memory_config.adc_calibration.vin.sample_count(), 8);
    assert_eq!(
        memory_config.adc_calibration.vin.samples[0].map(|sample| sample.expected_mv),
        Some(vin_adc_mv_for_input_mv(5_000))
    );
    assert_eq!(
        memory_config.adc_calibration.vin.samples[7].map(|sample| sample.expected_mv),
        Some(vin_adc_mv_for_input_mv(21_000))
    );
}

#[test]
fn vin_auto_job_reselects_current_when_sweep_crosses_apdo_boundary() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::VinAdc,
        pps_ma: Some(3_000),
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps =
        ManualPpsState::from_fusb302b_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(28_000),
            pps_max_ma: Some(5_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 21_000,
                    max_ma: 5_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 21_000,
                    max_mv: 28_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::VinAdc,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();

    let mut crossed_apdo_boundary = false;
    for _ in 0..200 {
        let request_mv = manual_pps.target_mv.expect("VIN sweep keeps a PPS request");
        let expected_ma = if request_mv <= 21_000 { 5_000 } else { 3_000 };
        let step = (request_mv - 5_000) / 1_000;
        let vin_raw_mv = 280 + (step * 45);
        assert_eq!(manual_pps.target_ma, Some(expected_ma));
        crossed_apdo_boundary |= request_mv > 21_000;

        update_calibration_job_state(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            CalibrationJobUpdateInput {
                now_ms: 0,
                latest_rtd_raw_code: 1_500,
                latest_vin_raw_code: 1_000,
                wifi_started: Some(false),
                pd_confirmed: true,
                latest_rtd_raw_adc_mv: 0,
                latest_vin_raw_adc_mv: vin_raw_mv,
                latest_temp_c: 25.0,
                latest_vin_mv: u32::from(request_mv),
                heater_duty_percent: 0,
            },
        );
        if calibration.job.status == CalibrationJobStatus::Completed {
            break;
        }
    }

    assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
    assert_eq!(calibration.job.samples_collected, 23);
    assert!(crossed_apdo_boundary);
}

#[test]
fn vin_auto_job_waits_for_measured_voltage_to_settle_before_sampling() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::VinAdc,
        pps_ma: Some(3_000),
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 21_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::VinAdc,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();

    for step in 0..17u16 {
        let request_mv = 5_000 + (step * 1_000);
        let settled_raw_mv = 280 + (step * 45);

        for _ in 0..3 {
            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                CalibrationJobUpdateInput {
                    now_ms: 0,
                    latest_rtd_raw_code: 1_500,
                    latest_vin_raw_code: 1_000,
                    wifi_started: Some(false),
                    pd_confirmed: true,
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: settled_raw_mv.saturating_sub(80),
                    latest_temp_c: 25.0,
                    latest_vin_mv: u32::from(request_mv),
                    heater_duty_percent: 0,
                },
            );
        }
        assert_eq!(calibration.job.samples_collected, step as u8);

        for _ in 0..5 {
            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                CalibrationJobUpdateInput {
                    now_ms: 0,
                    latest_rtd_raw_code: 1_500,
                    latest_vin_raw_code: 1_000,
                    wifi_started: Some(false),
                    pd_confirmed: true,
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: settled_raw_mv,
                    latest_temp_c: 25.0,
                    latest_vin_mv: u32::from(request_mv),
                    heater_duty_percent: 0,
                },
            );
        }
        assert_eq!(calibration.job.samples_collected, step as u8 + 1);
    }

    assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
    assert_eq!(memory_config.adc_calibration.vin.sample_count(), 8);
}

#[test]
fn thermal_plant_auto_job_starts_at_the_power_safe_ceiling_and_caps_current_at_3a() {
    for (max_mv, max_ma) in [(20_000, 3_000), (21_000, 3_000), (21_000, 5_000)] {
        let mut calibration = CalibrationRuntimeState::default();
        let mut memory_config = MemoryConfig::default();
        for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
            memory_config.heater_curve_raw_observations.points[index] =
                Some(HeaterCurveRawObservation {
                    raw_rtd_adc_mv,
                    heater_voltage_mv: 20_000,
                    heater_current_ma: max_ma,
                    resistance_milliohms: 4_000,
                });
        }
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(max_mv),
                pps_max_ma: Some(max_ma),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv,
                        max_ma,
                    }),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
                avs_min_mv: None,
                avs_max_mv: None,
                ..Default::default()
            }));

        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut manual_pps,
        )
        .unwrap();

        assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
        assert_eq!(calibration.mode, CalibrationMode::ThermalPlant);
        let expected_mv = max_mv;
        assert_eq!(calibration.pps_mv, Some(expected_mv));
        assert_eq!(calibration.job.next_request_mv, Some(expected_mv));
        assert_eq!(calibration.pps_ma, Some(3_000));
    }
}

#[test]
fn thermal_plant_auto_job_uses_the_apdo_that_covers_20v_not_another_range() {
    let mut calibration = CalibrationRuntimeState::default();
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(5_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 21_000,
                    max_ma: 3_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 5_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));

    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();

    assert_eq!(calibration.pps_mv, Some(21_000));
    assert_eq!(calibration.pps_ma, Some(3_000));
    assert_eq!(calibration.job.next_request_mv, Some(21_000));
}

#[test]
fn thermal_plant_source_selects_lowest_floor_after_current_and_ceiling_tie() {
    let manual_pps = ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_000),
        pps_apdos: [
            Some(ch224q::PpsApdo {
                min_mv: 10_000,
                max_mv: 21_000,
                max_ma: 3_000,
            }),
            Some(ch224q::PpsApdo {
                min_mv: 5_000,
                max_mv: 21_000,
                max_ma: 3_000,
            }),
            None,
            None,
            None,
            None,
            None,
        ],
        avs_min_mv: None,
        avs_max_mv: None,
        ..Default::default()
    }));

    assert_eq!(
        manual_pps.thermal_plant_source_limits(),
        Some((5_000, 21_000, 3_000))
    );
}

#[test]
fn thermal_plant_auto_job_requires_a_pps_range_that_covers_20v() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = MemoryConfig::default();
    for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
        memory_config.heater_curve_raw_observations.points[index] =
            Some(HeaterCurveRawObservation {
                raw_rtd_adc_mv,
                heater_voltage_mv: 20_000,
                heater_current_ma: 3_000,
                resistance_milliohms: 4_000,
            });
    }
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: false,
            pps_min_mv: Some(20_100),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }));

    assert_eq!(
        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut manual_pps,
        ),
        Err(ManualPpsError::ThermalPlantSourceUnsupported)
    );
    assert_eq!(calibration.job.status, CalibrationJobStatus::Idle);

    let mut below_current_floor =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(20_000),
            pps_max_ma: Some(2_999),
            ..Default::default()
        }));
    assert_eq!(
        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut below_current_floor,
        ),
        Err(ManualPpsError::ThermalPlantSourceUnsupported)
    );
}

#[test]
fn thermal_plant_auto_job_starts_one_transient_run_for_3a_and_5a_pps() {
    for (max_mv, max_ma) in [(20_000, 3_000), (21_000, 5_000)] {
        assert_auto_thermal_plant_job_start(max_mv, max_ma);
    }
}

fn assert_auto_thermal_plant_job_start(max_mv: u16, max_ma: u16) {
    let mut calibration = CalibrationRuntimeState::default();
    let mut config = thermal_plant_job_memory_config(max_mv, max_ma);
    let mut manual = thermal_plant_job_manual_pps(max_mv, max_ma);
    let mut workspace = CalibrationThermalPlantWorkspace::default();
    calibration_job_start_with_workspace(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut config,
        &mut manual,
        &mut workspace,
    )
    .unwrap();
    let mut plant = SupplyCalibrationPlant {
        temperature_c: 25.0,
        now_ms: 0,
        saw_board_voltage_drop: false,
    };
    for _ in 0..THERMAL_PLANT_AMBIENT_TICKS {
        plant.tick(&mut calibration, &mut config, &mut manual, &mut workspace);
        assert!(
            !calibration.heater_enabled,
            "ambient sampling must remain unpowered"
        );
    }
    assert_eq!(
        workspace.job.as_ref().unwrap().phase,
        ThermalPlantAutoPhase::Heating
    );
    while plant.now_ms < 4_000
        && !workspace.job.as_ref().is_some_and(|job| {
            job.supply_scan.as_ref().is_some_and(|scan| {
                scan.phase == flux_purr_firmware::heater_supply::scan::ScanPhase::Preflight
            })
        })
    {
        plant.tick(&mut calibration, &mut config, &mut manual, &mut workspace);
    }
    assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
    assert!(
        !calibration.heater_enabled,
        "high-voltage preflight is unloaded"
    );
    assert_eq!(manual.target_mv, Some(max_mv));
    assert_eq!(manual.target_ma, Some(3_000));
    assert_eq!(
        workspace
            .job
            .as_ref()
            .unwrap()
            .supply_scan
            .as_ref()
            .unwrap()
            .phase,
        flux_purr_firmware::heater_supply::scan::ScanPhase::Preflight
    );
}

fn thermal_plant_job_memory_config(max_mv: u16, max_ma: u16) -> MemoryConfig {
    let mut memory_config = MemoryConfig::default();
    for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
        memory_config.heater_curve_raw_observations.points[index] =
            Some(HeaterCurveRawObservation {
                raw_rtd_adc_mv,
                heater_voltage_mv: max_mv,
                heater_current_ma: max_ma,
                resistance_milliohms: 4_000,
            });
    }
    memory_config
}

fn thermal_plant_job_manual_pps(max_mv: u16, max_ma: u16) -> ManualPpsState {
    ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(max_mv),
        pps_max_ma: Some(max_ma),
        ..Default::default()
    }))
}

const SYNTHETIC_TARGET_MARGIN_C: f32 = 2.0;

fn runtime_test_raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
    let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
    (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
        / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms)
        - RTD_ASSUMED_ADC_OFFSET_UV as f32 / 1_000.0)
        .round() as u16
}

fn synthetic_supply_boundary() -> flux_purr_firmware::heater_supply::SupplyBoundary {
    use flux_purr_firmware::heater_supply::{BoundaryAdcMetadata, BoundaryPoint, SupplyBoundary};
    let mut boundary = SupplyBoundary {
        adc: BoundaryAdcMetadata {
            divider_supply_mv: RTD_DIVIDER_SUPPLY_MV,
            ..Default::default()
        },
        wifi_started: false,
        source_min_mv: 5_500,
        source_max_mv: 20_000,
        source_max_ma: 3_000,
        advertised_min_mv: 5_000,
        advertised_max_mv: 20_000,
        point_count: 1,
        ..Default::default()
    };
    boundary.points[0] = BoundaryPoint {
        elapsed_ms: 1_000,
        rtd_code: 1_500,
        rtd_mv: runtime_test_raw_rtd_adc_mv_for_temp(100.0),
        vin_code: 1_500,
        vin_mv: 750,
        request_current_ma: 3_000,
        tracking_request_mv: 10_000,
        flat_request_mv: 10_100,
        tracking_only: true,
    };
    assert!(boundary.is_valid());
    boundary
}

#[test]
fn synthetic_raw_rtd_temperatures_invert_the_active_assumed_offset() {
    let memory = MemoryConfig::default();
    for temperature in [25.0, 30.0, 80.0, 100.0, 160.0, 220.0] {
        let raw_mv = runtime_test_raw_rtd_adc_mv_for_temp(temperature);
        let projected = projected_rtd_temperature_c(&memory, raw_mv).unwrap();
        assert!(
            (projected - temperature).abs() < 0.3,
            "{temperature}: {projected}"
        );
    }
}

fn synthetic_memory_config() -> MemoryConfig {
    let mut config = MemoryConfig {
        commissioning_required: false,
        ..MemoryConfig::default()
    };
    config.active_heater_curve.points[0] = Some(HeaterCurvePoint {
        temp_centi_c: 2_500,
        resistance_milliohms: 4_000,
    });
    config.active_heater_curve.points[1] = Some(HeaterCurvePoint {
        temp_centi_c: 22_000,
        resistance_milliohms: 6_000,
    });
    for (index, (temp_c, resistance_milliohms)) in
        [(100.0, 4_800), (200.0, 5_800)].into_iter().enumerate()
    {
        config.heater_curve_raw_observations.points[index] = Some(HeaterCurveRawObservation {
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temp_c),
            heater_voltage_mv: 20_000,
            heater_current_ma: 3_000,
            resistance_milliohms,
        });
    }
    config.heater_supply_boundary = Some(synthetic_supply_boundary());
    config
}

fn synthetic_trace(
    memory_config: &MemoryConfig,
    ambient_temp_c: f32,
) -> (
    [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    usize,
) {
    synthetic_trace_with_delay(memory_config, ambient_temp_c, 0)
}

fn synthetic_trace_with_delay(
    _memory_config: &MemoryConfig,
    ambient_temp_c: f32,
    delay_ticks: usize,
) -> (
    [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    usize,
) {
    let mut input_history = [0.0; 41];
    assert!(delay_ticks < input_history.len());
    let capacity_mj_per_c = 100_000.0_f32;
    let convection_mw_per_c = 100.0_f32;
    let radiation_mw_per_k4 = 0.0000005_f32;
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    samples[0] = ThermalPlantTransientSample {
        elapsed_ticks: 1,
        raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(ambient_temp_c),
        heater_voltage_125mv: 0,
        duty_percent: 0,
    };
    let mut sample_count = 1usize;
    let mut temperature_c = ambient_temp_c;
    let mut heating = true;
    let mut last_saved_temp_c = ambient_temp_c;
    for tick in 2..=60_000_u16 {
        let reached_cutoff =
            heating && temperature_c >= THERMAL_PLANT_TARGET_TEMP_C + SYNTHETIC_TARGET_MARGIN_C;
        let duty_percent = u8::from(heating) * 100;
        let should_save = sample_count < 24
            || (temperature_c - last_saved_temp_c).abs() >= THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C
            || reached_cutoff
            || (!heating
                && temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C - SYNTHETIC_TARGET_MARGIN_C);
        if should_save {
            assert!(sample_count < THERMAL_PLANT_TRANSIENT_MAX_SAMPLES);
            samples[sample_count] = ThermalPlantTransientSample {
                elapsed_ticks: u32::from(tick),
                raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
                heater_voltage_125mv: if heating {
                    160
                } else {
                    flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
                },
                duty_percent,
            };
            sample_count += 1;
            last_saved_temp_c = temperature_c;
        }
        if reached_cutoff {
            heating = false;
            input_history[usize::from(tick) % input_history.len()] = 0.0;
            last_saved_temp_c = f32::MIN;
            continue;
        }
        if !heating
            && temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C - SYNTHETIC_TARGET_MARGIN_C
        {
            break;
        }
        let power_mw = if heating { 20.0 * 3_000.0 } else { 0.0 };
        input_history[usize::from(tick) % input_history.len()] = power_mw;
        let delayed_power_mw = input_history
            [(usize::from(tick) + input_history.len() - delay_ticks) % input_history.len()];
        let temperature_k = temperature_c + 273.15;
        let ambient_k = ambient_temp_c + 273.15;
        let losses_mw = convection_mw_per_c * (temperature_c - ambient_temp_c)
            + radiation_mw_per_k4 * (temperature_k.powi(4) - ambient_k.powi(4));
        temperature_c += (delayed_power_mw - losses_mw) / capacity_mj_per_c * 0.05;
    }
    (samples, sample_count)
}

fn assert_synthetic_fit(
    memory_config: &MemoryConfig,
    samples: &[ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    sample_count: usize,
    ambient_temp_c: f32,
) -> ThermalPlantTransientTransaction {
    let (transaction, residual) = fit_thermal_plant_transient(
        0x5452_4e53,
        runtime_test_raw_rtd_adc_mv_for_temp(ambient_temp_c),
        samples,
        sample_count as u8,
        None,
        memory_config,
    )
    .expect("synthetic trace fits");
    let projection = thermal_plant_projection_from_transient(&transaction).unwrap();
    assert!(sample_count >= 24);
    assert!(residual <= 0.20);
    assert!((projection.thermal_capacity_mj_per_c - 100_000.0).abs() < 40_000.0);
    assert!((projection.convection_mw_per_c - 100.0).abs() < 80.0);
    assert!(projection.radiation_mw_per_k4 >= 0.0);
    assert_eq!(transaction.samples[0].duty_percent, 0);
    assert_eq!(transaction.samples[1].duty_percent, 100);
    assert!(
        transaction.samples[..usize::from(transaction.sample_count)]
            .iter()
            .any(|sample| sample.duty_percent == 0)
    );
    assert!(thermal_plant_transient_trace_reaches_targets(
        &transaction,
        memory_config
    ));

    let mut quantized_trace = *samples;
    for (index, sample) in quantized_trace[..sample_count].iter_mut().enumerate() {
        if index > 0 && index + 1 < sample_count {
            sample.raw_rtd_adc_mv = if index % 2 == 0 {
                sample.raw_rtd_adc_mv.saturating_add(1)
            } else {
                sample.raw_rtd_adc_mv.saturating_sub(1)
            };
        }
    }
    let (_, quantized_residual) = fit_thermal_plant_transient(
        0x5155_414e,
        runtime_test_raw_rtd_adc_mv_for_temp(ambient_temp_c),
        &quantized_trace,
        sample_count as u8,
        None,
        memory_config,
    )
    .expect("bounded ADC quantization must still fit");
    assert!(quantized_residual <= 0.20);
    transaction
}

fn synthetic_manual_pps() -> ManualPpsState {
    ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(20_000),
        pps_max_ma: Some(3_000),
        ..Default::default()
    }))
}

fn assert_synthetic_model_guards(
    memory_config: &mut MemoryConfig,
    transaction: ThermalPlantTransientTransaction,
    ambient_temp_c: f32,
) {
    memory_config.thermal_plant_transient_active = Some(transaction);
    memory_config.heater_curve_transaction_id = Some(transaction.transaction_id);
    let manual_pps = synthetic_manual_pps();
    assert!(thermal_model_heater_allowed(
        memory_config,
        CalibrationRuntimeState::default(),
        manual_pps
    ));
    let valid_snapshot = thermal_plant_run_snapshot_wire(
        &CalibrationRuntimeState::default(),
        memory_config,
        &CalibrationThermalPlantWorkspace::default(),
        0,
        ambient_temp_c,
        0,
        0,
    );
    assert_eq!(
        valid_snapshot
            .active_result
            .as_ref()
            .map(|result| result.transaction_id),
        Some(transaction.transaction_id)
    );
    let mut unrelated_curve = memory_config.clone();
    unrelated_curve.heater_curve_transaction_id = Some(transaction.transaction_id + 1);
    assert!(thermal_model_heater_allowed(
        &unrelated_curve,
        CalibrationRuntimeState::default(),
        manual_pps
    ));
    let unrelated_wire = thermal_plant_runtime_wire(&unrelated_curve);
    assert_eq!(unrelated_wire.state.as_str(), "active");
    assert!(unrelated_wire.projection_valid);
    assert!(thermal_plant_projection_for_runtime(&unrelated_curve).is_some());
    let mut missing_raw_curve = memory_config.clone();
    missing_raw_curve.heater_curve_raw_observations = HeaterCurveRawObservations::default();
    assert!(thermal_model_heater_allowed(
        &missing_raw_curve,
        CalibrationRuntimeState::default(),
        manual_pps
    ));
}

fn assert_synthetic_apdo_and_persistence(
    memory_config: &mut MemoryConfig,
    transaction: ThermalPlantTransientTransaction,
) {
    let split_apdo_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(5_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 5_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 20_000,
                    max_mv: 21_000,
                    max_ma: 1_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));
    assert!(!thermal_model_heater_allowed(
        memory_config,
        CalibrationRuntimeState::default(),
        split_apdo_pps
    ));

    let mut stale_transaction = transaction;
    stale_transaction.projection.thermal_capacity_mj_per_c_bits = 200_000.0_f32.to_bits();
    memory_config.thermal_plant_transient_active = Some(stale_transaction);
    memory_config.heater_curve_raw_observations.points[1]
        .as_mut()
        .expect("second raw curve observation")
        .resistance_milliohms += 20;
    assert!(rebuild_transient_thermal_plant_for_current_inputs(
        memory_config
    ));
    let rebuilt_transaction = memory_config
        .thermal_plant_transient_active
        .expect("raw trace refits");
    assert_eq!(rebuilt_transaction.samples, transaction.samples);
    assert_ne!(
        rebuilt_transaction
            .projection
            .thermal_capacity_mj_per_c_bits,
        stale_transaction.projection.thermal_capacity_mj_per_c_bits
    );

    let persisted_record = MemoryRecord {
        sequence: 1,
        config: memory_config.clone(),
    };
    let mut persisted_bytes = [0_u8; MEMORY_SLOT_SIZE];
    let persisted_len = encode_memory_record(&persisted_record, &mut persisted_bytes)
        .expect("transient record encodes");
    let decoded = decode_memory_record(&persisted_bytes[..persisted_len])
        .expect("legacy snapshot still decodes unrelated configuration");
    assert!(
        decoded.config.thermal_plant_transient_active.is_none(),
        "the legacy snapshot format must not restore heater calibration"
    );
}

fn assert_synthetic_invalid_trace_guards(
    memory_config: &mut MemoryConfig,
    transaction: ThermalPlantTransientTransaction,
    ambient_temp_c: f32,
) {
    let manual_pps = synthetic_manual_pps();
    let mut nonterminal_cooldown = transaction;
    let append_index = usize::from(nonterminal_cooldown.sample_count);
    assert!(append_index < THERMAL_PLANT_TRANSIENT_MAX_SAMPLES);
    let previous = nonterminal_cooldown.samples[append_index - 1];
    nonterminal_cooldown.samples[append_index] = ThermalPlantTransientSample {
        elapsed_ticks: previous.elapsed_ticks.saturating_add(1),
        raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(100.0),
        heater_voltage_125mv: flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL,
        duty_percent: 0,
    };
    nonterminal_cooldown.sample_count = nonterminal_cooldown.sample_count.saturating_add(1);
    assert!(thermal_plant_projection_from_transient(&nonterminal_cooldown).is_some());
    assert!(!thermal_plant_transient_trace_reaches_targets(
        &nonterminal_cooldown,
        memory_config
    ));

    let mut incomplete_cooldown = transaction;
    for sample in incomplete_cooldown.samples[..usize::from(incomplete_cooldown.sample_count)]
        .iter_mut()
        .filter(|sample| sample.duty_percent == 0)
    {
        sample.raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(90.0);
    }
    assert!(thermal_plant_projection_from_transient(&incomplete_cooldown).is_some());
    assert!(!thermal_plant_transient_trace_reaches_targets(
        &incomplete_cooldown,
        memory_config
    ));
    memory_config.thermal_plant_transient_active = Some(incomplete_cooldown);
    assert!(!thermal_model_heater_allowed(
        memory_config,
        CalibrationRuntimeState::default(),
        manual_pps
    ));
    let invalid_snapshot = thermal_plant_run_snapshot_wire(
        &CalibrationRuntimeState::default(),
        memory_config,
        &CalibrationThermalPlantWorkspace::default(),
        0,
        ambient_temp_c,
        0,
        0,
    );
    assert!(invalid_snapshot.active_result.is_none());

    let mut below_cutoff = transaction;
    for sample in &mut below_cutoff.samples[..usize::from(below_cutoff.sample_count)] {
        if sample.duty_percent > 0 {
            sample.raw_rtd_adc_mv = sample
                .raw_rtd_adc_mv
                .min(runtime_test_raw_rtd_adc_mv_for_temp(215.0));
        }
    }
    assert!(thermal_plant_transient_trace_reaches_targets(
        &below_cutoff,
        memory_config
    ));

    let mut below_target = transaction;
    let mut saw_powered_sample = false;
    for sample in below_target.samples[..usize::from(below_target.sample_count)].iter_mut() {
        if sample.duty_percent > 0 {
            if saw_powered_sample {
                sample.raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(95.0);
            }
            saw_powered_sample = true;
        } else {
            sample.raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(80.0);
        }
    }
    assert!(thermal_plant_projection_from_transient(&below_target).is_some());
    assert!(!thermal_plant_transient_trace_reaches_targets(
        &below_target,
        memory_config
    ));
    memory_config.thermal_plant_transient_active = Some(below_target);
    assert!(!thermal_model_heater_allowed(
        memory_config,
        CalibrationRuntimeState::default(),
        manual_pps
    ));

    let mut cold_baseline = transaction;
    cold_baseline.samples[0].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(-50.0);
    assert!(thermal_plant_projection_from_transient(&cold_baseline).is_some());
    assert!(!thermal_plant_transient_trace_reaches_targets(
        &cold_baseline,
        memory_config
    ));
    assert!(
        fit_thermal_plant_transient(
            cold_baseline.transaction_id,
            cold_baseline.ambient_raw_rtd_adc_mv,
            &cold_baseline.samples,
            cold_baseline.sample_count,
            None,
            memory_config,
        )
        .is_none()
    );
}

#[test]
fn transient_thermal_fit_excludes_captured_instantaneous_spike_windows() {
    let rows: std::vec::Vec<[u32; 4]> = serde_json::from_str(include_str!(
        "../../../testdata/thermal-plant-a-spike-trace.json"
    ))
    .unwrap();
    let mut memory = MemoryConfig::default();
    for (index, (temp_centi_c, resistance_milliohms)) in [
        (0, 2948),
        (2000, 3200),
        (10046, 4212),
        (14079, 4719),
        (17585, 5160),
        (20665, 5547),
    ]
    .into_iter()
    .enumerate()
    {
        memory.active_heater_curve.points[index] = Some(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut cooling = false;
    let mut saw_power = false;
    for (sample, row) in samples.iter_mut().zip(&rows) {
        if row[3] > 0 {
            saw_power = true;
        } else if saw_power {
            cooling = true;
        }
        *sample = ThermalPlantTransientSample {
            elapsed_ticks: row[0],
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(row[1] as f32 / 100.0),
            heater_voltage_125mv: if cooling {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                row[2] as u8
            },
            duty_percent: row[3] as u8,
        };
    }
    let data = prepare_transient_fit(
        &samples,
        rows.len() as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .unwrap();
    let peak_index = rows.iter().position(|row| row[1] == 8014).unwrap();
    assert!(data.row(peak_index, 0).is_none());
    assert!(data.row(peak_index + 1, 0).is_none());
    assert!(data.row(rows.len() - 1, 0).is_some());
}

#[test]
fn transient_thermal_fit_recovers_with_an_instantaneous_contact_spike() {
    let memory = synthetic_memory_config();
    let (mut samples, count) = synthetic_trace(&memory, 25.0);
    samples[5].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(280.0);
    let (transaction, residual) = fit_thermal_plant_transient(
        123,
        samples[0].raw_rtd_adc_mv,
        &samples,
        count as u8,
        None,
        &memory,
    )
    .expect("an isolated contact spike must not corrupt a physical trace");
    assert_eq!(transaction.samples, samples);
    assert!(residual <= 0.20);
}

#[test]
fn transient_thermal_fit_rejects_late_cooling_contact_excursions() {
    let memory = synthetic_memory_config();
    let (mut samples, count) = synthetic_trace(&memory, 25.0);
    let cooling_start = (1..count)
        .find(|index| samples[*index].duty_percent == 0)
        .expect("synthetic trace has a cooling phase");
    let spike_index = (cooling_start..count)
        .find(|index| {
            samples[*index]
                .elapsed_ticks
                .saturating_sub(samples[cooling_start].elapsed_ticks)
                >= 400
        })
        .expect("synthetic trace has a late cooling sample");
    let base_temp_c = projected_rtd_temperature_c(&memory, samples[spike_index - 1].raw_rtd_adc_mv)
        .expect("synthetic temperature projects");
    samples[spike_index].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(base_temp_c + 5.0);
    let data = prepare_transient_fit(
        &samples,
        count as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .expect("a late cooling contact excursion must be excluded, not abort the fit");
    assert!(data.row(spike_index, 0).is_none());
    let (_, residual) = fit_thermal_plant_transient(
        321,
        samples[0].raw_rtd_adc_mv,
        &samples,
        count as u8,
        None,
        &memory,
    )
    .expect("the physical trace remains fit-capable after excluding the excursion");
    assert!(residual <= 0.20);
}

#[test]
fn transient_temperature_acceptance_rejects_a_cutoff_boundary_offset() {
    let temperatures = [
        25.0, 45.0, 75.0, 110.0, 150.0, 190.0, 220.0, 289.0, 287.0, 270.0, 240.0, 200.0, 160.0,
        120.0, 79.0,
    ];
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, temperature_c) in temperatures.into_iter().enumerate() {
        let cooling = index >= 9;
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: (index as u32) * 100,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
            heater_voltage_125mv: if cooling {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                160
            },
            duty_percent: u8::from(index <= 7) * 100,
        };
    }
    let accepted = transient_temperature_acceptance(&samples[..temperatures.len()], &temperatures);
    assert!(!accepted[7], "the cutoff jump must be rejected");
    assert!(
        accepted[8..temperatures.len()]
            .iter()
            .all(|accepted| !accepted),
        "a later cooling sample must not re-anchor an invalid trace"
    );
}

#[test]
fn transient_thermal_fit_excludes_sub_slew_heating_contact_peaks() {
    let memory = synthetic_memory_config();
    let (mut samples, count) = synthetic_trace(&memory, 25.0);
    let spike_index = (24..count - 1)
        .find(|index| {
            samples[*index + 1].duty_percent > 0
                && samples[*index].elapsed_ticks - samples[*index - 1].elapsed_ticks >= 10
                && samples[*index + 1].elapsed_ticks - samples[*index].elapsed_ticks >= 10
        })
        .expect("trace has a sparse powered interval");
    let previous =
        projected_rtd_temperature_c(&memory, samples[spike_index - 1].raw_rtd_adc_mv).unwrap();
    let next =
        projected_rtd_temperature_c(&memory, samples[spike_index + 1].raw_rtd_adc_mv).unwrap();
    let peak = previous.max(next) + 5.0;
    samples[spike_index].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(peak);
    let elapsed_s =
        (samples[spike_index].elapsed_ticks - samples[spike_index - 1].elapsed_ticks) as f32 * 0.05;
    assert!((peak - previous) / elapsed_s < RTD_CONTROL_MAX_SLEW_C_PER_S);
    let data = prepare_transient_fit(
        &samples,
        count as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .expect("an isolated sub-slew contact peak must not invalidate the physical trace");
    assert!(data.row(spike_index, 0).is_none());
    let (transaction, residual) = fit_thermal_plant_transient(
        1,
        samples[0].raw_rtd_adc_mv,
        &samples,
        count as u8,
        None,
        &memory,
    )
    .expect("the accepted physical observations remain fit-capable");
    assert_eq!(transaction.samples, samples);
    assert!(residual <= 0.20);
}

#[test]
fn transient_thermal_fit_cannot_use_a_spike_for_heating_coverage() {
    let memory = synthetic_memory_config();
    let (mut samples, count) = synthetic_trace(&memory, 25.0);
    for sample in &mut samples[..count] {
        sample.raw_rtd_adc_mv = sample
            .raw_rtd_adc_mv
            .min(runtime_test_raw_rtd_adc_mv_for_temp(95.0));
    }
    samples[5].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(280.0);
    assert!(
        prepare_transient_fit(
            &samples,
            count as u8,
            None,
            &memory,
            samples[0].raw_rtd_adc_mv
        )
        .is_none()
    );
}

#[test]
fn transient_thermal_fit_recovers_a_physical_model_from_heat_and_cool_trace() {
    let ambient_temp_c = 25.0_f32;
    let mut memory_config = synthetic_memory_config();
    let (samples, sample_count) = synthetic_trace(&memory_config, ambient_temp_c);
    let transaction = assert_synthetic_fit(&memory_config, &samples, sample_count, ambient_temp_c);
    assert_synthetic_model_guards(&mut memory_config, transaction, ambient_temp_c);
    assert_synthetic_apdo_and_persistence(&mut memory_config, transaction);
    assert_synthetic_invalid_trace_guards(&mut memory_config, transaction, ambient_temp_c);
}

#[test]
fn thermal_plant_export_keeps_legacy_resistance_absent() {
    let mut memory = synthetic_memory_config();
    let (samples, sample_count) = synthetic_trace(&memory, 25.0);
    let transaction = assert_synthetic_fit(&memory, &samples, sample_count, 25.0);
    memory.thermal_plant_transient_active = Some(transaction);
    let exported = thermal_plant_active_result(&memory, &transaction).unwrap();
    assert!(exported.curve.points.iter().all(Option::is_none));
    for temperature in [60.0, 120.0, 220.0] {
        assert_eq!(
            estimated_heater_resistance_ohms(temperature, None, &memory),
            default_estimated_heater_resistance_ohms(temperature)
        );
    }
}

#[test]
fn transient_heat_coverage_preserves_the_integer_adc_minimum_bin() {
    let memory = synthetic_memory_config();
    let raw_offset_mv =
        -rtd_assumed_adc_offset_uv(&memory, RTD_ASSUMED_ADC_OFFSET_UV) as f32 / 1_000.0;
    let (mut samples, count) = synthetic_trace(&memory, 30.0);
    let resistance = pt1000_resistance_ohms_at(100.0);
    let cutoff_raw_mv = (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance
        / (RTD_REFERENCE_RESISTOR_OHMS + resistance)
        + raw_offset_mv)
        .round() as u16;
    let cutoff_c = projected_rtd_temperature_c(&memory, cutoff_raw_mv).unwrap();
    if RTD_ASSUMED_ADC_OFFSET_UV == -13_500 {
        assert!(cutoff_c < 100.0);
    }
    for sample in samples[..count]
        .iter_mut()
        .filter(|sample| sample.duty_percent > 0)
    {
        sample.raw_rtd_adc_mv = sample.raw_rtd_adc_mv.min(cutoff_raw_mv);
    }
    samples[count - 1].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(79.0);
    let ambient_raw_mv = samples[0].raw_rtd_adc_mv;
    let mut transaction = ThermalPlantTransientTransaction {
        transaction_id: 1,
        ambient_raw_rtd_adc_mv: ambient_raw_mv,
        sample_count: count as u8,
        projection: ThermalPlantProjectionRecord::from_projection(ThermalPlantProjection {
            convection_mw_per_c: 100.0,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 100_000.0,
            transport_delay_ms: 50,
        }),
        samples,
    };
    assert!(thermal_plant_transient_trace_reaches_targets(
        &transaction,
        &memory
    ));
    assert!(prepare_transient_fit(&samples, count as u8, None, &memory, ambient_raw_mv).is_some());

    for sample in transaction.samples[..count]
        .iter_mut()
        .filter(|sample| sample.duty_percent > 0)
    {
        sample.raw_rtd_adc_mv = sample.raw_rtd_adc_mv.min(cutoff_raw_mv - 1);
    }
    assert!(!thermal_plant_transient_trace_reaches_targets(
        &transaction,
        &memory
    ));
    assert!(
        prepare_transient_fit(
            &transaction.samples,
            count as u8,
            None,
            &memory,
            ambient_raw_mv,
        )
        .is_none()
    );
}

#[test]
fn transient_zero_delay_model_does_not_invent_a_multi_second_delay() {
    let memory = synthetic_memory_config();
    let (samples, count) = synthetic_trace(&memory, 25.0);
    let data = prepare_transient_fit(
        &samples,
        count as u8,
        None,
        &memory,
        runtime_test_raw_rtd_adc_mv_for_temp(25.0),
    )
    .unwrap();
    let (model, residual) = best_transient_projection(&data).unwrap();
    assert!(
        model.transport_delay_ms <= 500,
        "zero-delay physical trace fitted {}ms of transport delay, residual={residual}",
        model.transport_delay_ms
    );
}

#[test]
fn transient_fit_preserves_a_real_transport_delay() {
    let memory = synthetic_memory_config();
    let (samples, count) = synthetic_trace_with_delay(&memory, 25.0, 20);
    let data = prepare_transient_fit(
        &samples,
        count as u8,
        None,
        &memory,
        runtime_test_raw_rtd_adc_mv_for_temp(25.0),
    )
    .unwrap();
    let (model, residual) = best_transient_projection(&data).unwrap();
    assert!(residual <= 0.2);
    assert!(
        (700..=1_300).contains(&model.transport_delay_ms),
        "1000ms physical transport delay fitted {}ms, residual={residual}",
        model.transport_delay_ms
    );
}

fn captured_thermal_trace() -> &'static [(u32, i16, u16, u8, bool)] {
    &[
        (3_250, 2_740, 5_375, 0, false),
        (3_450, 2_740, 5_250, 100, false),
        (52_150, 3_677, 5_500, 0, false),
        (74_800, 4_091, 5_625, 0, false),
        (157_800, 5_060, 5_750, 0, false),
        (206_400, 5_531, 5_250, 100, false),
        (286_300, 6_269, 6_250, 100, false),
        (374_050, 7_021, 7_250, 0, false),
        (992_700, 7_877, 8_500, 0, false),
        (1_025_100, 8_658, 10_125, 0, false),
        (1_057_250, 9_595, 10_625, 0, false),
        (1_078_000, 10_119, 11_500, 0, false),
        (1_102_000, 10_940, 12_625, 0, false),
        (1_129_950, 11_977, 12_875, 0, false),
        (1_148_750, 12_682, 13_625, 0, false),
        (1_174_450, 13_656, 14_625, 100, false),
        (1_192_850, 14_282, 16_250, 0, false),
        (1_206_000, 15_183, 17_375, 0, false),
        (1_236_150, 16_652, 18_000, 0, false),
        (1_244_750, 17_040, 17_250, 100, false),
        (1_277_350, 18_339, 19_000, 0, false),
        (1_295_600, 19_147, 18_875, 100, false),
        (1_326_700, 20_206, 19_375, 100, false),
        (1_360_450, 21_168, 19_875, 0, false),
        (1_397_950, 21_962, 20_375, 0, false),
        (1_398_150, 22_024, 0, 0, true),
        (1_399_700, 21_901, 0, 0, true),
        (1_401_800, 21_716, 0, 0, true),
        (1_402_800, 21_594, 0, 0, true),
        (1_404_400, 21_411, 0, 0, true),
        (1_405_950, 21_168, 0, 0, true),
        (1_408_050, 20_986, 0, 0, true),
        (1_414_400, 20_147, 0, 0, true),
        (1_425_300, 18_915, 0, 0, true),
        (1_440_700, 17_208, 0, 0, true),
        (1_451_050, 16_322, 0, 0, true),
        (1_460_500, 15_452, 0, 0, true),
        (1_471_950, 14_598, 0, 0, true),
        (1_491_050, 13_346, 0, 0, true),
        (1_504_650, 12_530, 0, 0, true),
        (1_522_300, 11_679, 0, 0, true),
        (1_542_100, 10_843, 0, 0, true),
        (1_572_200, 9_737, 0, 0, true),
        (1_592_250, 9_312, 0, 0, true),
        (1_612_300, 8_751, 0, 0, true),
        (1_639_600, 7_968, 0, 0, true),
    ]
}

#[test]
fn captured_thermal_trace_requires_confirmed_samples_for_transient_fit() {
    let trace = captured_thermal_trace();
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, (elapsed_ms, temp_centi_c, heater_mv, duty, cooling)) in
        trace.iter().copied().enumerate()
    {
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: elapsed_ms / 50,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(f32::from(temp_centi_c) / 100.0),
            heater_voltage_125mv: if cooling {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                (heater_mv / 125) as u8
            },
            duty_percent: duty,
        };
    }
    let memory = MemoryConfig::default();
    let data = prepare_transient_fit(
        &samples,
        trace.len() as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .expect("the captured trace should pass basic acceptance");
    assert!(best_transient_projection(&data).is_none());
    let mut filtered = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut filtered_count = 0usize;
    for sample in samples[..trace.len()].iter().copied() {
        if filtered_count == 0
            || sample.duty_percent > 0
            || flux_purr_firmware::memory::thermal_plant_sample_is_cooling(sample)
        {
            filtered[filtered_count] = sample;
            filtered_count += 1;
        }
    }
    let filtered_data = prepare_transient_fit(
        &filtered,
        filtered_count as u8,
        None,
        &memory,
        filtered[0].raw_rtd_adc_mv,
    );
    let (_, residual) = best_transient_projection(
        &filtered_data.expect("confirmed samples should produce a physical model"),
    )
    .expect("confirmed samples should produce a projection");
    assert!(residual <= 0.20);
}

#[test]
fn cached_transient_balances_match_direct_integrals_at_every_delay_edge() {
    let memory = synthetic_memory_config();
    let (samples, count) = synthetic_trace_with_delay(&memory, 25.0, 20);
    let data = prepare_transient_fit(
        &samples,
        count as u8,
        None,
        &memory,
        runtime_test_raw_rtd_adc_mv_for_temp(25.0),
    )
    .unwrap();
    let temperatures: Vec<_> = samples[..count]
        .iter()
        .map(|sample| projected_rtd_temperature_c(&memory, sample.raw_rtd_adc_mv).unwrap())
        .collect();
    let powers: Vec<_> = samples[..count]
        .iter()
        .zip(&temperatures)
        .map(|(sample, temperature)| {
            transient_sample_power_mw(*sample, *temperature, None, &memory).unwrap()
        })
        .collect();
    let ambient =
        projected_rtd_temperature_c(&memory, runtime_test_raw_rtd_adc_mv_for_temp(25.0)).unwrap();
    for delay in [0, 1, 20, 99, 200] {
        for index in 1..count {
            let cached = data.row(index, delay);
            if let Some((cached_values, cached_energy)) = cached {
                let (values, energy) = transient_fit_row(
                    &samples[..count],
                    &temperatures,
                    &powers,
                    index,
                    delay,
                    ambient,
                )
                .expect("an accepted cached row has a direct balance");
                assert_eq!(values, cached_values);
                assert!(
                    (energy - cached_energy).abs() <= 1.0 + energy.abs() * 0.0001,
                    "energy mismatch at index={index} delay={delay}: direct={energy} cached={cached_energy}"
                );
            }
        }
    }
}

#[test]
fn transient_power_matches_the_temperature_derivative_interval() {
    let samples = [0, 5, 10, 20].map(|elapsed_ticks| ThermalPlantTransientSample {
        elapsed_ticks,
        raw_rtd_adc_mv: 1000,
        heater_voltage_125mv: 160,
        duty_percent: if elapsed_ticks == 0 { 100 } else { 0 },
    });
    // A 250ms powered interval followed by 250ms off must contribute its
    // energy to the 500ms temperature derivative, even though its endpoint
    // power is zero. A delayed window includes only its actual overlap.
    let powers = [1000.0, 0.0, 0.0, 0.0];
    let (_, energy_mj) =
        transient_fit_row(&samples, &[25.0, 26.0, 26.0, 26.0], &powers, 3, 0, 25.0).unwrap();
    assert_eq!(energy_mj, 125.0);
    assert_eq!(
        transient_mean_delayed_power_mw(&samples, &powers, 0.0, 10.0, 0),
        Some(500.0)
    );
    assert_eq!(
        transient_mean_delayed_power_mw(&samples, &powers, 0.0, 10.0, 8),
        Some(200.0)
    );
    let continuous_vin_samples = [0, 10].map(|elapsed_ticks| ThermalPlantTransientSample {
        elapsed_ticks,
        raw_rtd_adc_mv: 1000,
        heater_voltage_125mv: if elapsed_ticks == 0 { 96 } else { 160 },
        duty_percent: 100,
    });
    assert_eq!(
        transient_mean_delayed_power_mw(&continuous_vin_samples, &[1000.0, 2000.0], 0.0, 10.0, 0),
        Some(1500.0),
        "measured VIN drift under fixed PPS is not a discrete duty edge"
    );
    let rising_edge_samples = [0, 10].map(|elapsed_ticks| ThermalPlantTransientSample {
        elapsed_ticks,
        raw_rtd_adc_mv: 1000,
        heater_voltage_125mv: 160,
        duty_percent: if elapsed_ticks == 0 { 0 } else { 100 },
    });
    assert_eq!(
        transient_mean_delayed_power_mw(&rising_edge_samples, &[0.0, 2000.0], 0.0, 10.0, 0),
        Some(2000.0),
        "a sparse trace must attribute a confirmed rising edge to its new power"
    );
}

const LIVE_DEVICE_TRACE: &[(u16, f32, u8)] = &[
    (40, 32.67, 0),
    (41, 32.67, 100),
    (42, 32.67, 100),
    (43, 32.67, 100),
    (44, 32.67, 100),
    (45, 32.67, 100),
    (46, 32.67, 100),
    (47, 32.67, 100),
    (48, 33.08, 100),
    (49, 32.67, 100),
    (50, 32.67, 100),
    (51, 32.67, 100),
    (52, 33.08, 100),
    (53, 33.08, 100),
    (54, 33.08, 100),
    (55, 33.08, 100),
    (56, 33.08, 100),
    (57, 33.49, 100),
    (58, 33.08, 100),
    (59, 33.49, 100),
    (60, 33.49, 100),
    (61, 33.90, 100),
    (62, 33.90, 100),
    (63, 33.90, 100),
    (104, 38.01, 100),
    (126, 41.75, 100),
    (149, 45.94, 100),
    (175, 50.17, 100),
    (192, 54.02, 100),
    (213, 58.33, 100),
    (236, 62.26, 100),
    (255, 66.66, 100),
    (274, 70.66, 100),
    (296, 74.70, 100),
    (315, 78.77, 100),
    (335, 82.43, 100),
    (356, 86.58, 100),
    (375, 90.77, 100),
    (397, 95.00, 100),
    (420, 99.27, 100),
    (442, 103.59, 100),
    (465, 107.46, 100),
    (489, 111.85, 100),
    (513, 115.79, 100),
    (534, 119.77, 100),
    (558, 123.78, 100),
    (585, 128.34, 100),
    (605, 131.92, 100),
    (633, 136.04, 100),
    (660, 140.20, 100),
    (688, 144.40, 100),
    (711, 148.63, 100),
    (741, 152.37, 100),
    (771, 156.68, 100),
    (799, 161.03, 100),
    (830, 165.42, 100),
    (865, 169.29, 100),
    (900, 173.76, 100),
    (928, 177.70, 100),
    (971, 181.68, 100),
    (1004, 186.26, 100),
    (1042, 189.73, 100),
    (1082, 194.39, 100),
    (1122, 198.51, 100),
    (1165, 202.66, 100),
    (1220, 206.85, 100),
    (1251, 210.46, 100),
    (1304, 214.72, 100),
    (1358, 219.01, 100),
    (1379, 220.24, 100),
    (1380, 224.58, 0),
    (1559, 220.24, 0),
    (1643, 215.94, 0),
    (1713, 211.68, 0),
    (1754, 208.05, 0),
    (1811, 203.85, 0),
    (1871, 199.69, 0),
    (1930, 195.56, 0),
    (1984, 191.47, 0),
    (2030, 187.42, 0),
    (2082, 183.39, 0),
    (2146, 179.40, 0),
    (2193, 175.44, 0),
    (2252, 170.96, 0),
    (2329, 167.07, 0),
    (2414, 163.22, 0),
    (2474, 158.85, 0),
    (2562, 155.06, 0),
    (2672, 150.76, 0),
    (2765, 147.04, 0),
    (2852, 142.82, 0),
    (2976, 138.63, 0),
    (3093, 134.49, 0),
    (3206, 130.38, 0),
    (3313, 126.31, 0),
    (3448, 122.28, 0),
    (3608, 118.28, 0),
    (3733, 114.31, 0),
    (3939, 109.89, 0),
    (4131, 106.00, 0),
    (4306, 102.14, 0),
    (4472, 97.84, 0),
    (4692, 93.59, 0),
    (4926, 89.84, 0),
    (5131, 85.65, 0),
    (5488, 81.51, 0),
    (5647, 80.14, 0),
];

#[test]
fn captured_calibration_rejects_impossible_startup_temperature_spikes() {
    let trace = CALIBRATION_CUTOFF_TRACE;
    let memory = MemoryConfig::default();
    let mut curve = HeaterCurveConfig::default();
    for (index, (temp_centi_c, resistance_milliohms)) in [
        (0, 2948),
        (2000, 3200),
        (10096, 4218),
        (14113, 4723),
        (17632, 5166),
        (20623, 5542),
    ]
    .into_iter()
    .enumerate()
    {
        curve.points[index] = Some(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, (elapsed_ticks, centi_c, heater_voltage_125mv, duty_percent)) in
        trace.iter().copied().enumerate()
    {
        let resistance = pt1000_resistance_ohms_at(f32::from(centi_c) / 100.0);
        let raw = f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance
            / (RTD_REFERENCE_RESISTOR_OHMS + resistance)
            - RTD_ASSUMED_ADC_OFFSET_UV as f32 / 1000.0;
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: u32::from(elapsed_ticks),
            raw_rtd_adc_mv: raw.round() as u16,
            heater_voltage_125mv,
            duty_percent,
        };
    }
    let data = prepare_transient_fit(
        &samples,
        trace.len() as u8,
        Some(&curve),
        &memory,
        samples[0].raw_rtd_adc_mv,
    );
    if RTD_ASSUMED_ADC_OFFSET_UV == -13_500 {
        assert!(
            data.is_some(),
            "the actual mapping covers both cutoff temperatures"
        );
    }
    if let Some(data) = data {
        let spike_index = trace
            .iter()
            .position(|(_, centi_c, _, _)| *centi_c == 5123)
            .unwrap();
        assert!(data.row(spike_index, 0).is_none());
        assert!(data.row(spike_index + 1, 0).is_none());
    }
}

#[test]
fn transient_thermal_fit_accepts_the_live_device_trace_shape() {
    let memory_config = MemoryConfig {
        commissioning_required: false,
        ..MemoryConfig::default()
    };
    let mut preview_curve = HeaterCurveConfig::default();
    for (index, (temp_centi_c, resistance_milliohms)) in [
        (0, 2_948),
        (2_000, 3_200),
        (10_051, 4_213),
        (14_075, 4_719),
        (17_570, 5_158),
        (20_615, 5_541),
    ]
    .into_iter()
    .enumerate()
    {
        preview_curve.points[index] = Some(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }

    // This is the public temperature trace from the physical device run:
    // 50 ms startup samples, a 220C cutoff, and passive cooling to 80C.
    let trace = LIVE_DEVICE_TRACE;
    assert!(trace.len() <= THERMAL_PLANT_TRANSIENT_MAX_SAMPLES);

    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut cooling = false;
    let mut saw_power = false;
    for (index, (elapsed_ticks, temp_c, duty_percent)) in trace.iter().copied().enumerate() {
        if duty_percent > 0 {
            saw_power = true;
        } else if saw_power {
            cooling = true;
        }
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: u32::from(elapsed_ticks),
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temp_c),
            heater_voltage_125mv: if cooling {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                160
            },
            duty_percent,
        };
    }

    let recorded_terminal_temp_c =
        projected_rtd_temperature_c(&memory_config, samples[trace.len() - 1].raw_rtd_adc_mv)
            .expect("fixture terminal temperature projects");
    assert!(recorded_terminal_temp_c > THERMAL_PLANT_COOL_COMPLETE_TEMP_C);
    assert!(!thermal_plant_cooling_complete(
        79.99,
        recorded_terminal_temp_c
    ));
    assert!(
        fit_thermal_plant_transient(
            0x4c49_5645,
            runtime_test_raw_rtd_adc_mv_for_temp(32.67),
            &samples,
            trace.len() as u8,
            Some(&preview_curve),
            &memory_config,
        )
        .is_none(),
        "a nonterminal recorded trace must not fit"
    );

    let mut terminal_samples = samples;
    terminal_samples[trace.len() - 1].raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(79.73);
    let terminal_temp_c = projected_rtd_temperature_c(
        &memory_config,
        terminal_samples[trace.len() - 1].raw_rtd_adc_mv,
    )
    .expect("terminal fixture temperature projects");
    assert!(thermal_plant_cooling_complete(79.73, terminal_temp_c));
    let (_, residual) = fit_thermal_plant_transient(
        0x4c49_5645,
        runtime_test_raw_rtd_adc_mv_for_temp(32.67),
        &terminal_samples,
        trace.len() as u8,
        Some(&preview_curve),
        &memory_config,
    )
    .expect("live device-shaped trace must produce a physical model");
    assert!(residual <= 0.20);
}

#[test]
fn transient_thermal_fit_accepts_physical_3a_stage_transitions() {
    let memory = MemoryConfig::default();
    let trace = [
        (3450, 27.00, 5375, 0),
        (3550, 27.00, 5375, 100),
        (21800, 34.31, 5125, 100),
        (67850, 39.67, 5000, 100),
        (119900, 46.78, 5000, 100),
        (160100, 50.60, 5125, 100),
        (306600, 59.64, 5750, 100),
        (581200, 63.57, 6375, 100),
        (732550, 72.90, 7375, 100),
        (807500, 81.05, 8000, 100),
        (1243250, 89.37, 10625, 100),
        (1255100, 94.06, 10875, 100),
        (2496450, 198.51, 6750, 100),
        (3164550, 131.92, 14000, 100),
        (3195450, 145.98, 16625, 100),
        (3206750, 157.22, 17625, 100),
        (3233150, 172.08, 19000, 100),
        (3234650, 173.76, 19125, 100),
        (3240650, 179.40, 19875, 100),
        (3247300, 182.82, 19875, 100),
        (3253050, 190.31, 19375, 100),
        (3258100, 192.64, 19375, 100),
        (3270650, 200.88, 19875, 100),
        (3283500, 209.86, 19875, 100),
        (3299550, 220.24, 19375, 100),
        (3299650, 220.24, 0, 0),
        (3302850, 219.01, 0, 0),
        (3306000, 217.16, 0, 0),
        (3309150, 215.33, 0, 0),
        (3341350, 177.70, 0, 0),
        (3360950, 163.22, 0, 0),
        (3415950, 144.40, 0, 0),
        (3423900, 144.92, 0, 0),
        (3437550, 107.94, 0, 0),
        (3444250, 116.29, 0, 0),
        (3446300, 131.41, 0, 0),
        (3454500, 98.32, 0, 0),
        (3469250, 126.82, 0, 0),
        (3471550, 140.72, 0, 0),
        (3473200, 108.92, 0, 0),
        (3476050, 128.85, 0, 0),
        (3477650, 92.65, 0, 0),
        (3493850, 96.89, 0, 0),
        (3495550, 135.52, 0, 0),
        (3500450, 119.77, 0, 0),
        (3503950, 112.34, 0, 0),
        (3515100, 143.87, 0, 0),
        (3529300, 79.68, 0, 0),
    ];
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, (elapsed_ms, temp_c, voltage_mv, duty_percent)) in trace.into_iter().enumerate() {
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: (elapsed_ms / HEATER_CONTROL_INTERVAL_MS) as u32,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temp_c),
            heater_voltage_125mv: if duty_percent == 0 && index > 0 {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                (voltage_mv / 125) as u8
            },
            duty_percent,
        };
    }
    let temperatures = trace.map(|(_, temp_c, _, _)| temp_c);
    let accepted = transient_temperature_acceptance(&samples[..trace.len()], &temperatures);
    assert!(accepted[12] && accepted[13]);
    let (_, residual) = fit_thermal_plant_transient(
        0x335f_3341,
        samples[0].raw_rtd_adc_mv,
        &samples,
        trace.len() as u8,
        None,
        &memory,
    )
    .expect("physical 3A trace should produce a model");
    assert!(residual <= 0.20);
}

#[test]
fn transient_fit_excludes_fan_assisted_cooling_from_energy_balance() {
    let memory = synthetic_memory_config();
    let (mut samples, count) = synthetic_trace(&memory, 25.0);
    for sample in &mut samples[..count] {
        if flux_purr_firmware::memory::thermal_plant_sample_is_cooling(*sample) {
            sample.heater_voltage_125mv =
                flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL;
        }
    }
    let (_, residual) = fit_thermal_plant_transient(
        0x4641_4e53,
        samples[0].raw_rtd_adc_mv,
        &samples,
        count as u8,
        None,
        &memory,
    )
    .expect("fan-assisted cooling remains terminal evidence, not heater input");
    assert!(residual <= 0.20);
}

#[test]
fn transient_fit_accepts_a_zero_duty_cutoff_sample_in_heating_phase() {
    let memory = synthetic_memory_config();
    let temperatures = [
        25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 90.0, 95.0,
        95.0, 100.5, 96.0, 91.0, 86.0, 82.0, 79.0, 77.0, 75.0,
    ];
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, temperature_c) in temperatures.into_iter().enumerate() {
        let cooling = index >= 17;
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: (index as u32) * 100,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
            heater_voltage_125mv: if cooling {
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
            } else {
                160
            },
            duty_percent: u8::from(!cooling && index != 16) * 100,
        };
    }
    assert!(
        prepare_transient_fit(
            &samples,
            temperatures.len() as u8,
            None,
            &memory,
            samples[0].raw_rtd_adc_mv,
        )
        .is_some()
    );
}

#[test]
fn runtime_ready_boot_stage_matches_post_flash_contract() {
    assert_eq!(RUNTIME_READY_BOOT_STAGE_LINE, b"boot_stage=runtime_ready\n");
}

#[test]
fn runtime_ready_boot_stage_is_emitted_after_the_first_ui_and_before_runtime_loop() {
    let source = RUNTIME_IMPLEMENTATION;
    let normalized_source: String = source.split_whitespace().collect();
    let runtime_state_flow = normalized_source
        .split(
            "pub(crate)asyncfninitialize_runtime_state_from_ready(spawner:Spawner,ready:Box<BootMemoryReady>,)->Box<BootRuntimeState>",
        )
        .nth(1)
        .expect("runtime-state initialization must remain present");
    let first_ui = runtime_state_flow
        .find("runtime.present_initial_ui().await;")
        .expect("boot must render the first UI before becoming ready");
    let ready_marker = runtime_state_flow
        .find(
            "let_=usb_write_bytes_bounded(&mutruntime.system.usb_serial,RUNTIME_READY_BOOT_STAGE_LINE,",
        )
        .expect("boot must emit the runtime-ready marker on USB");
    let runtime_entry = runtime_state_flow
        .rfind("runtime}")
        .expect("boot must retain the prepared runtime state after becoming ready");
    assert!(first_ui < ready_marker);
    assert!(ready_marker < runtime_entry);

    let boot = include_str!("boot.rs");
    let runtime_loop = include_str!("runtime_loop.rs");
    let normalized_entrypoint: String = FIRMWARE_ENTRYPOINT.split_whitespace().collect();
    assert!(
        normalized_entrypoint.contains("runtime::run(spawner).await;")
            && boot.contains("initialize_runtime_state_from_ready(spawner, ready).await")
            && boot.contains(".spawn(run_boot_runtime_finalize_task(spawner, state))")
            && boot.contains(
                "async fn run_boot_runtime_finalize_task(spawner: Spawner, state: Box<BootRuntimeState>)"
            )
            && boot.contains(".spawn(run_frontpanel_runtime_task(state))")
            && runtime_loop.contains("async fn run_runtime_loop(mut state: Box<RuntimeLoopState>)")
    );
}

#[test]
fn runtime_loop_storage_is_reserved_before_network_startup() {
    let source = RUNTIME_IMPLEMENTATION;
    let state_initialization = source
        .split("pub(crate) async fn initialize_runtime_state_from_ready")
        .nth(1)
        .expect("runtime-state initialization must remain present");
    let reserve = state_initialization
        .find("Box::<BootRuntimeState>::new_uninit()")
        .expect("the boot runtime state must be reserved before startup work");
    let network = state_initialization
        .find("runtime.start_network(&spawner).await;")
        .expect("network startup must remain in the runtime initialization path");
    let handoff = state_initialization
        .rfind("runtime\n}")
        .expect("the prepared boot state must remain after startup work");

    assert!(reserve < network);
    assert!(network < handoff);
}

#[test]
fn adc_boot_tokens_are_consumed_before_runtime_assembly() {
    let boot = include_str!("boot.rs");
    let runtime_assembly = include_str!("runtime_assembly.rs");
    let initialize_adc = boot
        .split("async fn initialize_adc(&mut self)")
        .nth(1)
        .and_then(|source| source.split("async fn initialize_initial_rtd").next())
        .expect("ADC initialization must remain present");

    assert!(
        initialize_adc.contains("self\n            .tokens\n            .take()"),
        "ADC initialization must consume its one-time boot tokens"
    );
    assert!(
        runtime_assembly.contains("tokens.is_none()")
            && !runtime_assembly.contains("tokens.is_some()"),
        "runtime assembly must receive consumed boot tokens after ADC initialization"
    );
}

#[test]
fn usb_control_rx_line_uses_bss_instead_of_the_runtime_heap() {
    let boot = include_str!("boot.rs");
    let support = include_str!("support.rs");

    assert!(
        boot.contains("usb_rx_line: &'static mut heapless::String<USB_CONTROL_LINE_CAPACITY>")
            && boot.contains("let usb_rx_line = initialize_usb_control_rx_line();"),
        "boot must borrow the static USB receive line"
    );
    assert!(
        support.contains(
            "static mut USB_CONTROL_RX_LINE: heapless::String<USB_CONTROL_LINE_CAPACITY>"
        ) && support.contains("pub(crate) fn initialize_usb_control_rx_line()")
            && support.contains("line.clear();"),
        "the USB receive line must be reset in BSS before each boot"
    );
    assert!(
        !support.contains("Box::<heapless::String<USB_CONTROL_LINE_CAPACITY>>::new_uninit()"),
        "the USB receive line must not consume the internal runtime heap"
    );
}

#[test]
fn runtime_heap_keeps_boot_handoff_capacity_without_networking() {
    assert_eq!(
        RUNTIME_HEAP_SIZE,
        52 * 1024,
        "the diagnostic build must retain enough heap for complete boot states"
    );
}

#[test]
fn boot_stages_handoff_the_heap_pipeline_between_normal_executor_tasks() {
    let boot = include_str!("boot.rs");
    let support = include_str!("support.rs");

    assert!(
        boot.contains("pub(crate) async fn run(spawner: Spawner)")
            && boot.contains("init_runtime_heap();")
            && boot.contains("let pipeline_storage = Box::<BootPipeline>::new_uninit();")
            && boot.contains("BootPipeline::new(system_tokens, device_tokens)")
            && boot.contains(".spawn(run_boot_system_stage_task(spawner, pipeline))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_system_stage_task")
            && boot.contains("let system_storage = Box::<BootSystem>::new_uninit();")
            && boot.contains(
                "pipeline.system = Some(initialize_boot_system(\n        spawner,\n        system_tokens,\n        system_storage,\n    ));"
            )
            && boot.contains(".spawn(run_boot_pd_stage_task(spawner, pipeline))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_pd_stage_task")
            && boot.contains(".spawn(run_boot_display_stage_task(spawner, pipeline))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_display_stage_task")
            && boot.contains(".spawn(run_boot_memory_stage_task(spawner, pipeline))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_memory_stage_task")
            && boot.contains(".spawn(run_boot_runtime_stage_task(spawner, pipeline))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_runtime_stage_task")
            && boot.contains(".spawn(run_boot_runtime_finalize_task(spawner, state))")
            && boot.contains("#[embassy_executor::task]\nasync fn run_boot_runtime_finalize_task")
            && boot.contains(".spawn(run_frontpanel_runtime_task(state))"),
        "each ordered boot stage must hand the heap pipeline to the next task"
    );
    assert!(
        !boot.contains("BootHandoff")
            && !boot.contains("BootStageStorage")
            && !boot.contains("run_after_boot_system")
            && !support.contains("BOOT_PIPELINE_FUTURE_HEAP_STORAGE")
            && !support.contains("init_boot_pipeline_future_heap()"),
        "the boot path must not retain an aggregate coordinator future"
    );
    assert!(
        boot.contains("system: Option<Box<BootSystem>>")
            && boot.contains("pub(crate) struct BootDisplay {\n    system: Box<BootSystem>,")
            && boot.contains("display: Option<Box<BootDisplay>>")
            && boot.contains("memory_ready: Option<Box<BootMemoryReady>>")
            && boot.contains("Box::<BootSystem>::new_uninit()")
            && boot.contains("Box::<BootDisplay>::new_uninit()")
            && boot.contains("Box::<BootMemoryReady>::new_uninit()")
            && boot.contains("Box::<BootRuntimeState>::new_uninit()"),
        "full boot states must cross stage boundaries only through heap allocations"
    );
}

#[test]
fn boot_output_initialization_stays_in_one_boot_container() {
    let boot = include_str!("boot.rs");

    assert!(
        boot.contains("pub(crate) fn initialize_boot_outputs_stage(boot: &mut BootDisplay)")
            && boot.contains(".output_tokens\n        .take()")
            && boot.contains("boot.output_state = Some(BootOutputState")
            && boot.contains("let display_storage = Box::<BootDisplay>::new_uninit();")
            && boot.contains(
                "initialize_boot_display_from_parts(system, device_tokens, display_storage).await"
            )
            && boot.contains("initialize_boot_outputs_stage(")
            && !boot.contains("pub(crate) struct BootOutput {"),
        "boot output initialization must not return another full boot container through the startup future"
    );
}

#[test]
fn boot_backlight_is_enabled_before_display_initialization() {
    let boot = include_str!("boot.rs");
    let boot_system = boot
        .split("pub(crate) fn initialize_boot_system(")
        .nth(1)
        .expect("boot system initialization is present");
    let backlight = boot_system
        .find("let mut backlight = Output::new(tokens.gpio13, Level::Low")
        .expect("backlight is configured active-low");
    let backlight_enable = boot_system
        .find("backlight.set_low();")
        .expect("backlight is enabled during boot system initialization");
    assert!(backlight < backlight_enable);
    let display_stage = boot
        .split("pub(crate) async fn initialize_boot_display(")
        .nth(1)
        .expect("display boot stage is present");
    assert!(!display_stage.contains("runtime.backlight.set_low();"));
}

#[test]
fn boot_memory_future_is_heap_pinned_before_eeprom_initialization() {
    let boot = include_str!("boot.rs");
    let normalized_boot: String = boot.split_whitespace().collect();

    assert!(
        normalized_boot.contains(
            "Box::pin(initialize_boot_memory(memory_context,eeprom_record_staging,)).await"
        ),
        "the EEPROM startup future must not be nested on the guarded boot stack"
    );
}

#[test]
fn runtime_synchronization_uses_normal_static_storage() {
    let support = include_str!("support.rs");
    let pd_service = include_str!("pd_service.rs");
    let tasks = include_str!("tasks.rs");
    let network = include_str!("../../net.rs");

    assert!(
        !support.contains("ResettableStatic")
            && !pd_service.contains("reset_pd_service_state")
            && !tasks.contains("reset_buzzer_runtime_state"),
        "runtime synchronization must not overwrite storage that the executor can retain"
    );
    assert!(
        network.contains("use static_cell::StaticCell;")
            && network.contains("static NET_RESOURCES: StaticCell<StackResources<8>>")
            && network.contains("static WIFI_CONTROLLER: StaticCell<WifiController<'static>>")
            && !network.contains("ResettableStatic")
            && !network.contains("initialize_after_software_reset")
            && !network.contains("reset_runtime_sync_state"),
        "network runtime storage must remain one-time initialized because spawned WiFi tasks retain those references"
    );
}

#[test]
fn boot_emits_rom_markers_around_hal_initialization() {
    let boot = include_str!("boot.rs");
    let run = boot
        .split("pub(crate) async fn run(spawner: Spawner)")
        .nth(1)
        .expect("boot run entrypoint must remain present");
    let init_enter = run
        .find("rom_boot_stage(b\"hal_init_enter\")")
        .expect("boot must mark entry before HAL initialization");
    let hal_init = run
        .find("let peripherals = esp_hal::init(config);")
        .expect("boot must initialize the HAL");
    let init_complete = run
        .find("rom_boot_stage(b\"hal_init_complete\")")
        .expect("boot must mark successful HAL initialization");
    assert!(init_enter < hal_init);
    assert!(hal_init < init_complete);
    for marker in [
        "pd_stage_enter",
        "pd_i2c_taken",
        "pd_i2c_locked",
        "pd_detect_done",
        "pd_phy_init_enter",
        "pd_phy_init_done",
        "pd_service_spawned",
        "pd_startup_window_done",
    ] {
        assert!(
            boot.contains(&format!("rom_boot_stage(b\"{marker}\")")),
            "PD boot diagnostics must retain the {marker} stage marker"
        );
    }
}

#[test]
fn entrypoint_awaits_the_direct_boot_path_without_fault_probes() {
    assert!(
        FIRMWARE_ENTRYPOINT.contains("runtime::run(spawner).await")
            && !RUNTIME_IMPLEMENTATION.contains("initialize_boot_memory_ready")
            && !RUNTIME_IMPLEMENTATION.contains("[DEBUG-")
            && !RUNTIME_IMPLEMENTATION.contains("if false"),
        "the Embassy entry task must await direct boot without diagnostic bypasses"
    );
}

#[test]
fn post_system_boot_handoff_starts_after_the_allocator_is_ready() {
    let boot = include_str!("boot.rs");
    let normalized_boot: String = boot.split_whitespace().collect();

    assert!(
        normalized_boot.contains("init_runtime_heap();")
            && normalized_boot.contains("letpipeline_storage=Box::<BootPipeline>::new_uninit();")
            && normalized_boot.contains("BootPipeline::new(system_tokens,device_tokens)")
            && normalized_boot.contains("asyncfnrun_boot_system_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)")
            && normalized_boot.contains("letsystem_storage=Box::<BootSystem>::new_uninit();")
            && normalized_boot
                .contains("pipeline.system=Some(initialize_boot_system(spawner,system_tokens,system_storage,));"),
        "the root task must heap-pin the system stage before asynchronous boot work"
    );
    assert!(
        normalized_boot.contains(".spawn(run_boot_system_stage_task(spawner,pipeline))")
            && normalized_boot.contains(".spawn(run_boot_pd_stage_task(spawner,pipeline))"),
        "the post-system boot pipeline must move to its first task after heap initialization"
    );
    assert!(
        normalized_boot.contains(
            "asyncfnrun_boot_system_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)"
        ) && normalized_boot.contains(
            "asyncfnrun_boot_pd_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)"
        ) && normalized_boot.contains(
            "asyncfnrun_boot_display_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)"
        ) && normalized_boot.contains(
            "asyncfnrun_boot_memory_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)"
        ) && normalized_boot.contains(
            "asyncfnrun_boot_runtime_stage_task(spawner:Spawner,mutpipeline:Box<BootPipeline>)"
        ) && normalized_boot.contains(
            "asyncfnrun_boot_runtime_finalize_task(spawner:Spawner,state:Box<BootRuntimeState>)"
        ) && !normalized_boot.contains("dynFuture<Output=()>")
            && !normalized_boot.contains("Future<Output=BootDisplay>")
            && !normalized_boot.contains("Future<Output=BootMemoryReady>")
            && !normalized_boot.contains("Future<Output=Box<RuntimeLoopState>>"),
        "each stage must retain full state in the heap pipeline without a nested coordinator future"
    );
}

#[test]
fn display_timeout_path_uses_the_native_async_spi_device() {
    let source = RUNTIME_IMPLEMENTATION;
    let display_bus = source
        .split("pub(crate) type RuntimeDisplayBus =")
        .nth(1)
        .and_then(|value| value.split(';').next())
        .expect("runtime display bus alias must remain present");
    assert!(display_bus.contains("ExclusiveDevice"));
    assert!(display_bus.contains("Spi<'static, esp_hal::Async>"));
    assert!(!display_bus.contains("BlockingAsync"));

    let frontpanel = include_str!("frontpanel.rs");
    assert!(frontpanel.contains("const DISPLAY_DELAY_YIELD_QUANTUM_US: u32 = 1_000"));
    assert!(frontpanel.contains("embassy_futures::yield_now().await"));
    assert!(!frontpanel.contains("EmbassyTimer::after_millis"));

    let display_setup = source
        .split("pub(crate) fn initialize_display_driver(")
        .nth(1)
        .expect("display initialization must remain present");
    assert!(display_setup.contains("ExclusiveDevice::new_no_delay(spi.into_async(), cs)"));
    assert!(!source.contains("pub(crate) struct CancellationSafeSpiDevice"));
}

#[test]
fn display_framebuffer_boot_initialization_never_materializes_a_stack_sized_array() {
    let boot = include_str!("boot.rs");
    let display_setup = boot
        .split("pub(crate) fn initialize_display_driver(")
        .nth(1)
        .expect("display initialization must remain present");

    assert!(display_setup.contains("initialize_display_graphics(&psram)"));
    let support = include_str!("support.rs");
    assert!(
        support.contains("try_new_uninit_in") || support.contains("new_uninit_in"),
        "the PSRAM framebuffer must be initialized in place"
    );
    assert!(
        support.contains("DISPLAY_GRAPHICS_HEAP_GUARD_BYTES")
            && support.contains("esp_alloc::HEAP.add_region")
            && support.contains("runtime_heap_start"),
        "unused PSRAM must remain available as a global runtime allocation fallback"
    );
    assert!(
        !display_setup.contains("initialize_after_software_reset("),
        "the display framebuffer must not be passed by value through the boot task stack"
    );
}

#[test]
fn dirty_dashboard_refresh_is_capped_at_thirty_frames_per_second() {
    assert_eq!(DISPLAY_RUNTIME_MAX_FPS, 30);
    assert_eq!(DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS, 33);
}

#[test]
fn transient_trace_zero_duty_samples_do_not_rearm_from_source_voltage() {
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Ambient,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv: 250,
        idle_samples: 1,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: 1,
        phase_started_tick: 0,
        sample_count: 0,
        last_saved_temp_c: f32::MIN,
        last_saved_tick: 0,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_125mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    };

    assert!(record_thermal_plant_transient_sample(
        &mut job, 250, 25.0, 20_000, 0, true
    ));
    assert_eq!(job.samples[0].heater_voltage_125mv, 160);
    assert_eq!(job.samples[0].duty_percent, 0);

    job.phase = ThermalPlantAutoPhase::Heating;
    job.elapsed_ticks = 2;
    assert!(record_thermal_plant_transient_sample(
        &mut job, 260, 26.0, 20_000, 100, true
    ));
    assert_eq!(job.samples[1].heater_voltage_125mv, 160);
    assert_eq!(job.samples[1].duty_percent, 100);

    // Past the dense startup window, cutoff must still be recorded even
    // when there is no temperature change. Otherwise the fit invents input
    // energy until a later 4C cooling sample.
    job.sample_count = 24;
    job.samples[23] = job.samples[1];
    job.elapsed_ticks = 3;
    assert!(record_thermal_plant_transient_sample(
        &mut job, 260, 26.0, 20_000, 0, false
    ));
    assert_eq!(job.sample_count, 25);
    assert_eq!(job.samples[24].elapsed_ticks, 3);
    assert_eq!(job.samples[24].duty_percent, 0);
    assert_ne!(
        job.samples[24].heater_voltage_125mv,
        flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL
    );

    job.phase = ThermalPlantAutoPhase::Cooling;
    job.elapsed_ticks = 4;
    assert!(record_thermal_plant_transient_sample(
        &mut job, 259, 25.9, 20_000, 0, true
    ));
    assert_eq!(
        job.samples[25].heater_voltage_125mv,
        flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL
    );
}

#[test]
fn transient_temperature_steps_are_rate_limited_after_dense_startup() {
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Heating,
        source_current_ma: 5_000,
        ambient_raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(25.0),
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: 16,
        phase_started_tick: 0,
        sample_count: 16,
        last_saved_temp_c: 25.0,
        last_saved_tick: 16,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_125mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    };
    job.samples[15] = ThermalPlantTransientSample {
        elapsed_ticks: 16,
        raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(25.0),
        heater_voltage_125mv: 168,
        duty_percent: 100,
    };

    job.elapsed_ticks = 17;
    assert!(record_thermal_plant_transient_sample(
        &mut job,
        runtime_test_raw_rtd_adc_mv_for_temp(30.0),
        30.0,
        21_000,
        100,
        false,
    ));
    assert_eq!(job.sample_count, 16);

    job.elapsed_ticks = 26;
    assert!(record_thermal_plant_transient_sample(
        &mut job,
        runtime_test_raw_rtd_adc_mv_for_temp(35.0),
        35.0,
        21_000,
        100,
        false,
    ));
    assert_eq!(job.sample_count, 17);
    assert_eq!(job.samples[16].elapsed_ticks, 26);
}

#[test]
fn transient_trace_compaction_keeps_the_full_heating_span_for_fitting() {
    let memory = synthetic_memory_config();
    let (samples, sample_count) = synthetic_trace(&memory, 25.0);
    assert!(sample_count > 48);
    let ambient_raw_rtd_adc_mv = samples[0].raw_rtd_adc_mv;
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Cooling,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv,
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: samples[sample_count - 1].elapsed_ticks,
        phase_started_tick: samples
            .iter()
            .take(sample_count)
            .position(|sample| sample.duty_percent == 0)
            .map_or(0, |index| samples[index].elapsed_ticks),
        sample_count: sample_count as u8,
        last_saved_temp_c: 79.0,
        last_saved_tick: samples[sample_count - 1].elapsed_ticks,
        samples,
    };

    compact_supply_calibration_trace(&mut job);

    let compacted = &job.samples[..usize::from(job.sample_count)];
    let first_powered = compacted
        .iter()
        .find(|sample| sample.duty_percent != 0)
        .expect("compacted trace retains powered samples");
    let peak_powered = compacted
        .iter()
        .filter(|sample| sample.duty_percent != 0)
        .map(|sample| projected_rtd_temperature_c(&memory, sample.raw_rtd_adc_mv).unwrap())
        .fold(f32::MIN, f32::max);
    assert!(projected_rtd_temperature_c(&memory, first_powered.raw_rtd_adc_mv).unwrap() < 100.0);
    assert!(peak_powered >= THERMAL_PLANT_TARGET_TEMP_C);
    assert!(
        compacted
            .last()
            .is_some_and(|sample| sample.duty_percent == 0)
    );
    assert!(
        fit_thermal_plant_transient(
            0x434f_4d50,
            ambient_raw_rtd_adc_mv,
            &job.samples,
            job.sample_count,
            None,
            &memory,
        )
        .is_some()
    );
}

#[test]
fn transient_trace_compaction_handles_non_monotonic_rtd_samples() {
    let ambient_raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(25.0);
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Cooling,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv,
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: 127 * 10,
        phase_started_tick: 101 * 10,
        sample_count: 128,
        last_saved_temp_c: 80.0,
        last_saved_tick: 127 * 10,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_125mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    };
    for index in 0..=100 {
        let temperature = if index % 2 == 0 { 220.0 } else { 80.0 };
        job.samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: index as u32 * 10,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature),
            heater_voltage_125mv: 160,
            duty_percent: 100,
        };
    }
    for index in 101..128 {
        job.samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: index as u32 * 10,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(80.0),
            heater_voltage_125mv:
                flux_purr_firmware::memory::THERMAL_PLANT_COOLING_VOLTAGE_SENTINEL,
            duty_percent: 0,
        };
    }

    compact_supply_calibration_trace(&mut job);

    assert!(job.sample_count >= 24);
    assert!(job.sample_count <= 48);
    let compacted = &job.samples[..usize::from(job.sample_count)];
    assert!(compacted.iter().any(|sample| sample.duty_percent > 0));
    assert!(
        compacted
            .iter()
            .any(|sample| { flux_purr_firmware::memory::thermal_plant_sample_is_cooling(*sample) })
    );
}

#[test]
fn transient_cutoff_keeps_time_samples_when_temperature_has_not_moved_four_degrees() {
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Cooling,
        source_current_ma: 5_000,
        ambient_raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(30.0),
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: 1_001,
        phase_started_tick: 1_000,
        sample_count: 68,
        last_saved_temp_c: 220.0,
        last_saved_tick: 1_000,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_125mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    };
    job.samples[67] = ThermalPlantTransientSample {
        elapsed_ticks: 1_000,
        raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(220.0),
        heater_voltage_125mv: 168,
        duty_percent: 100,
    };
    for tick in 1_001..=1_220 {
        job.elapsed_ticks = tick;
        // A slow post-cut peak remains invisible to the old 4C recorder.
        let temperature = 220.0 + ((tick - 1_001) as f32 * 0.005).min(0.8);
        assert!(record_thermal_plant_transient_sample(
            &mut job,
            runtime_test_raw_rtd_adc_mv_for_temp(temperature),
            temperature,
            21_000,
            0,
            false,
        ));
    }
    let cutoff = &job.samples[68..usize::from(job.sample_count)];
    assert_eq!(cutoff.first().unwrap().elapsed_ticks, 1_001);
    assert!(cutoff.len() >= 20, "post-cut samples={}", cutoff.len());
    assert!(
        cutoff
            .windows(2)
            .all(|pair| pair[1].elapsed_ticks - pair[0].elapsed_ticks <= 10)
    );
    assert!(cutoff.last().unwrap().elapsed_ticks <= 1_200);
    assert!(job.sample_count < THERMAL_PLANT_TRANSIENT_MAX_SAMPLES as u8);
}

#[test]
fn transient_trace_periodically_samples_slow_cooling() {
    let mut job = CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Cooling,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(30.0),
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: None,
        started_at_ms: None,
        elapsed_ticks: 1_000 + THERMAL_PLANT_TRACE_INTERVAL_TICKS,
        phase_started_tick: 1_000,
        sample_count: 1,
        last_saved_temp_c: 120.0,
        last_saved_tick: 1_000,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 1_000,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(120.0),
            heater_voltage_125mv: 160,
            duty_percent: 100,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    };

    assert!(record_thermal_plant_transient_sample(
        &mut job,
        runtime_test_raw_rtd_adc_mv_for_temp(119.5),
        119.5,
        20_000,
        0,
        false,
    ));
    assert_eq!(job.sample_count, 2);
    assert_eq!(
        job.samples[1].elapsed_ticks,
        1_000 + THERMAL_PLANT_TRACE_INTERVAL_TICKS
    );
    assert_eq!(job.samples[1].duty_percent, 0);
}

struct SupplyCalibrationPlant {
    temperature_c: f32,
    now_ms: u64,
    saw_board_voltage_drop: bool,
}

impl SupplyCalibrationPlant {
    fn tick(
        &mut self,
        calibration: &mut CalibrationRuntimeState,
        config: &mut MemoryConfig,
        manual_pps: &mut ManualPpsState,
        workspace: &mut CalibrationThermalPlantWorkspace,
    ) {
        let duty = u8::from(calibration.heater_enabled) * 100;
        let source_mv = manual_pps.target_mv.unwrap_or(5_500);
        let current_a = f32::from(manual_pps.target_ma.unwrap_or(3_000)) / 1_000.0;
        let resistance = 3.2 * (1.0 + 0.00393 * (self.temperature_c - 20.0));
        // An independent synthetic board load and source CC loop. No such
        // current or watt measurement is supplied to the product algorithm.
        // Below the synthetic source's CC operating point there is no positive
        // quadratic root; keep the plant bounded instead of panicking in debug.
        let cc_discriminant = (current_a.powi(2) - 3.2 / resistance).max(0.0);
        let cc_voltage = resistance * (current_a + cc_discriminant.sqrt()) / 2.0;
        let delivered_v =
            (f32::from(source_mv) / 1_000.0).min(if duty > 0 { cc_voltage } else { 21.0 });
        let delivered_mv = (delivered_v * 1_000.0).round() as u32;
        self.saw_board_voltage_drop |= delivered_mv + 100 < u32::from(source_mv);
        let raw_rtd = runtime_test_raw_rtd_adc_mv_for_temp(self.temperature_c);
        let projected = projected_rtd_temperature_c(config, raw_rtd).unwrap();
        let previous_sample_count = workspace
            .job
            .as_ref()
            .map_or(0, |job| usize::from(job.sample_count));
        update_calibration_job_state_with_workspace(
            calibration,
            config,
            manual_pps,
            workspace,
            CalibrationJobUpdateInput {
                now_ms: self.now_ms,
                latest_rtd_raw_code: raw_rtd * 2,
                latest_vin_raw_code: (delivered_v * 150.0).round() as u16,
                wifi_started: Some(false),
                pd_confirmed: true,
                latest_rtd_raw_adc_mv: raw_rtd,
                latest_vin_raw_adc_mv: (delivered_v * 75.0).round() as u16,
                latest_temp_c: projected,
                latest_vin_mv: delivered_mv,
                heater_duty_percent: duty,
            },
        );
        let recorded_sample = workspace.job.as_ref().and_then(|job| {
            let current_sample_count = usize::from(job.sample_count);
            (current_sample_count > 0 && current_sample_count != previous_sample_count)
                .then(|| job.samples[current_sample_count - 1])
        });
        if let Some(sample) = recorded_sample
            .filter(|sample| !flux_purr_firmware::memory::thermal_plant_sample_is_cooling(*sample))
        {
            assert_eq!(
                sample.heater_voltage_125mv,
                quantize_thermal_plant_heater_voltage_mv(delivered_mv)
            );
        }
        let power_w = if duty > 0 {
            delivered_v.powi(2) / resistance
        } else {
            0.0
        };
        self.temperature_c += (power_w - 0.16 * (self.temperature_c - 25.0)) / 12.0 * 0.05;
        self.now_ms += 50;
    }
}

fn assert_boundary_warmup_reaches_cutoff(config: &MemoryConfig, manual: &ManualPpsState) {
    let mut temperature = 25.0_f32;
    for _ in 0..36_000 {
        let raw = runtime_test_raw_rtd_adc_mv_for_temp(temperature);
        let projected = projected_rtd_temperature_c(config, raw).unwrap();
        let candidate = manual
            .automatic_heater_pps_candidate_with_rtd(projected, None, config, None, Some(raw))
            .unwrap();
        let resistance = default_estimated_heater_resistance_ohms(temperature);
        let current = f32::from(candidate.current_ma) / 1_000.0;
        let cc_discriminant = (current.powi(2) - 3.2 / resistance).max(0.0);
        let cc_voltage = resistance * (current + cc_discriminant.sqrt()) / 2.0;
        let voltage = (f32::from(candidate.allowed_mv) / 1_000.0).min(cc_voltage);
        let power_w = voltage.powi(2) / resistance;
        temperature += (power_w - 0.16 * (temperature - 25.0)) / 12.0 * 0.05;
        if temperature >= 220.0 {
            return;
        }
    }
    panic!("calibrated boundary stalled automatic warmup at {temperature} C");
}

#[test]
fn supply_scan_equilibrium_preserves_fractional_adc_temperature_precision() {
    use flux_purr_firmware::heater_supply::scan::ScanPhase;

    let mut calibration = CalibrationRuntimeState::default();
    let mut config = MemoryConfig::default();
    let mut manual = thermal_plant_job_manual_pps(20_000, 3_000);
    let mut workspace = CalibrationThermalPlantWorkspace::default();
    calibration_job_start_with_workspace(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut config,
        &mut manual,
        &mut workspace,
    )
    .unwrap();
    let job = workspace.job.as_mut().unwrap();
    job.phase = ThermalPlantAutoPhase::Heating;
    job.supply_scan.as_mut().unwrap().phase = ScanPhase::TemperatureSettle;
    let temperature = |mv| projected_rtd_fractional_temperature_c(&config, mv).unwrap();
    assert!(temperature(1_101.55) - temperature(1_099.45) < 1.0);
    assert!(temperature(1_102.0) - temperature(1_099.0) > 1.0);

    for tick in 0..=240 {
        let fractional_mv = if tick % 4 < 2 { 1_099.45 } else { 1_101.55 };
        let live_temp = projected_rtd_fractional_temperature_c(&config, fractional_mv).unwrap();
        update_calibration_job_state_with_workspace(
            &mut calibration,
            &mut config,
            &mut manual,
            &mut workspace,
            CalibrationJobUpdateInput {
                now_ms: tick * 50,
                latest_rtd_raw_code: (fractional_mv * 2.0) as u16,
                latest_vin_raw_code: 800,
                wifi_started: Some(false),
                pd_confirmed: true,
                latest_rtd_raw_adc_mv: fractional_mv.round() as u16,
                latest_vin_raw_adc_mv: 400,
                latest_temp_c: live_temp,
                latest_vin_mv: 5_500,
                heater_duty_percent: 100,
            },
        );
        let phase = workspace
            .job
            .as_ref()
            .unwrap()
            .supply_scan
            .as_ref()
            .unwrap()
            .phase;
        if phase == ScanPhase::Sweep {
            assert!(tick >= 220, "a full ten-second window is still required");
            return;
        }
        assert_eq!(phase, ScanPhase::TemperatureSettle);
    }
    panic!("rounding raw mV must not reset a valid fractional-temperature window");
}

#[test]
fn thermal_plant_waits_through_a_transient_missing_capability_snapshot() {
    let mut calibration = CalibrationRuntimeState::default();
    let mut config = thermal_plant_job_memory_config(20_000, 3_000);
    let mut manual_pps = thermal_plant_job_manual_pps(20_000, 3_000);
    let mut workspace = CalibrationThermalPlantWorkspace::default();
    calibration_job_start_with_workspace(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut config,
        &mut manual_pps,
        &mut workspace,
    )
    .unwrap();

    // A FUSB302B source-capability refresh briefly publishes no APDO while
    // the source is still attached. The job must hold output off and resume
    // from its existing intent when the next snapshot arrives.
    manual_pps.refresh_fusb302b_capabilities_preserving_intent(None);
    update_calibration_job_state_with_workspace(
        &mut calibration,
        &mut config,
        &mut manual_pps,
        &mut workspace,
        CalibrationJobUpdateInput {
            now_ms: 1_000,
            latest_rtd_raw_code: 500,
            latest_vin_raw_code: 1_000,
            wifi_started: Some(false),
            pd_confirmed: false,
            latest_rtd_raw_adc_mv: 250,
            latest_vin_raw_adc_mv: 500,
            latest_temp_c: 25.0,
            latest_vin_mv: 10_000,
            heater_duty_percent: 0,
        },
    );

    assert_eq!(calibration.mode, CalibrationMode::ThermalPlant);
    assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
    assert_eq!(calibration.job.message, None);
    assert!(!calibration.heater_enabled);
    assert!(manual_pps.enabled);

    update_thermal_plant_calibration_job(
        &mut calibration,
        &mut config,
        &mut manual_pps,
        &mut workspace,
        CalibrationJobUpdateInput {
            now_ms: u64::from(THERMAL_PLANT_TOTAL_TIMEOUT_TICKS + 1) * HEATER_CONTROL_INTERVAL_MS
                + 1_000,
            latest_rtd_raw_code: 500,
            latest_vin_raw_code: 1_000,
            wifi_started: Some(false),
            pd_confirmed: false,
            latest_rtd_raw_adc_mv: 250,
            latest_vin_raw_adc_mv: 500,
            latest_temp_c: 25.0,
            latest_vin_mv: 10_000,
            heater_duty_percent: 0,
        },
    );

    assert_eq!(
        calibration.job.message,
        Some(ManualPpsError::ThermalPlantTimeout)
    );
    assert!(!manual_pps.enabled);
}

#[test]
fn thermal_plant_auto_completes_supply_scan_and_cooling_on_a_three_amp_source() {
    let mut calibration = CalibrationRuntimeState::default();
    let mut config = MemoryConfig::default();
    let mut manual_pps = thermal_plant_job_manual_pps(20_000, 3_000);
    let mut workspace = CalibrationThermalPlantWorkspace::default();
    calibration_job_start_with_workspace(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut config,
        &mut manual_pps,
        &mut workspace,
    )
    .unwrap();
    let mut plant = SupplyCalibrationPlant {
        temperature_c: 25.0,
        now_ms: 0,
        saw_board_voltage_drop: false,
    };
    while plant.now_ms < 22_000_000 && calibration.job.status == CalibrationJobStatus::Running {
        plant.tick(
            &mut calibration,
            &mut config,
            &mut manual_pps,
            &mut workspace,
        );
    }
    if calibration.job.status != CalibrationJobStatus::Completed {
        let job = workspace.job.as_ref().unwrap();
        panic!(
            "thermal plant calibration did not complete: error={:?}, samples={}, T={}, ms={}",
            calibration.job.message, job.sample_count, plant.temperature_c, plant.now_ms
        );
    }
    assert_eq!(
        calibration.job.status,
        CalibrationJobStatus::Completed,
        "error={:?}, T={}, ms={}",
        calibration.job.message,
        plant.temperature_c,
        plant.now_ms
    );
    assert!(
        plant.now_ms <= u64::from(THERMAL_PLANT_TOTAL_TIMEOUT_TICKS) * HEATER_CONTROL_INTERVAL_MS,
        "rapid thermal plant calibration exceeded five-minute budget: {} ms",
        plant.now_ms
    );
    assert_eq!(calibration.mode, CalibrationMode::Off);
    assert!(plant.saw_board_voltage_drop);
    assert!(!calibration.heater_enabled);
    assert!(!manual_pps.enabled);
    assert!(config.thermal_plant_transient_active.is_some());
    assert_boundary_warmup_reaches_cutoff(&config, &manual_pps);
    let transaction = config.thermal_plant_transient_active.unwrap();
    let projection = thermal_plant_projection_from_transient(&transaction).unwrap();
    assert!(
        (6_000.0..=18_000.0).contains(&projection.thermal_capacity_mj_per_c),
        "{projection:?}"
    );
    assert!(transaction.sample_count <= 48);
    let raw_samples = transaction.samples;
    assert!(rebuild_transient_thermal_plant_for_current_inputs(
        &mut config
    ));
    assert_eq!(
        config.thermal_plant_transient_active.unwrap().samples,
        raw_samples
    );
    let rebuilt_projection =
        thermal_plant_projection_from_transient(&config.thermal_plant_transient_active.unwrap())
            .expect("compressed trace can rebuild a valid projection");
    assert!(rebuilt_projection.convection_mw_per_c.is_finite());
    assert!(rebuilt_projection.convection_mw_per_c > 0.0);
    assert!(rebuilt_projection.thermal_capacity_mj_per_c.is_finite());
    assert!(rebuilt_projection.thermal_capacity_mj_per_c > 0.0);
    assert!(
        config
            .heater_supply_boundary
            .is_some_and(|boundary| boundary.is_valid())
    );
    assert!(
        config
            .active_heater_curve
            .points
            .iter()
            .all(Option::is_none)
    );
    assert!(
        config
            .heater_curve_raw_observations
            .points
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn thermal_plant_elapsed_clock_retains_fractional_intervals() {
    let mut clock = CalibrationElapsedClock::default();
    let mut actual_ms = 0;
    let mut ticks = 0;
    for interval in [9, 47, 61, 75, 50, 115, 29, 2, 500] {
        actual_ms += interval;
        ticks += clock.advance(interval);
        assert_eq!(u64::from(ticks), actual_ms / HEATER_CONTROL_INTERVAL_MS);
    }
    clock = CalibrationElapsedClock::default();
    assert_eq!(clock.advance(49), 0);
    assert_eq!(clock.advance(1), 1);
    assert_eq!(clock.advance_for_run(Some(1), 49), 0);
    assert_eq!(clock.advance_for_run(Some(2), 1), 0);
    assert_eq!(clock.advance_for_run(Some(2), 49), 1);
    assert_eq!(clock.advance_for_run(None, 49), 0);
    assert_eq!(clock.advance_for_run(Some(3), 1), 0);
}

#[test]
fn thermal_plant_transient_cuts_heat_at_220_before_the_next_output_cycle() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        heater_enabled: true,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        job_data: Some(CalibrationJobData::ThermalPlant),
        ..CalibrationRuntimeState::default()
    };
    test_install_thermal_plant_job(CalibrationThermalPlantAutoJob {
        run_id: 1,
        phase: ThermalPlantAutoPhase::Heating,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv: 250,
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        supply_scan: flux_purr_firmware::heater_supply::scan::SupplyScan::new(
            synthetic_supply_boundary(),
        ),
        started_at_ms: Some(0),
        elapsed_ticks: 100,
        phase_started_tick: THERMAL_PLANT_AMBIENT_TICKS.into(),
        sample_count: 1,
        last_saved_temp_c: 215.0,
        last_saved_tick: 100,
        samples: [ThermalPlantTransientSample {
            elapsed_ticks: 100,
            raw_rtd_adc_mv: 250,
            heater_voltage_125mv: 160,
            duty_percent: 100,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    });
    let mut memory_config = MemoryConfig::default();
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(20_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }));
    manual_pps
        .enable(ManualPpsOwner::Calibration, 5_500, Some(3_000))
        .unwrap();

    update_calibration_job_state(
        &mut calibration,
        &mut memory_config,
        &mut manual_pps,
        CalibrationJobUpdateInput {
            now_ms: 5_050,
            latest_rtd_raw_code: 1_500,
            latest_vin_raw_code: 1_000,
            wifi_started: Some(false),
            pd_confirmed: true,
            latest_rtd_raw_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(221.0),
            latest_vin_raw_adc_mv: 750,
            latest_temp_c: THERMAL_PLANT_TARGET_TEMP_C,
            latest_vin_mv: 20_000,
            heater_duty_percent: 100,
        },
    );

    assert!(!calibration.heater_enabled);
    assert!(manual_pps.enabled);
    assert_eq!(manual_pps.target_mv, Some(20_000));
    assert!(!take_immediate_heater_disarm(&mut calibration));
    assert_eq!(calibration.job_data, Some(CalibrationJobData::ThermalPlant));
    assert_eq!(
        test_thermal_plant_phase(),
        Some(ThermalPlantAutoPhase::Cooling)
    );
    assert_eq!(
        thermal_plant_calibration_snapshot(220.0, false).duty_percent,
        0
    );
    assert_eq!(
        thermal_plant_calibration_snapshot(215.0, true).duty_percent,
        100
    );
    assert!(thermal_plant_output_must_be_off(calibration, true, 220.0));
}

#[test]
fn thermal_plant_live_rtd_cutoff_ignores_a_lagging_guarded_temperature() {
    let calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        heater_enabled: true,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };

    let calibration_temp = thermal_plant_calibration_temperature_c(calibration, Some(229.5), 158.4);
    assert_eq!(calibration_temp, 229.5);
    assert!(thermal_plant_output_must_be_off(
        calibration,
        true,
        calibration_temp
    ));
    assert_eq!(
        thermal_plant_calibration_temperature_c(
            CalibrationRuntimeState::default(),
            Some(229.5),
            158.4
        ),
        158.4
    );
}

#[test]
fn thermal_plant_job_fails_before_sampling_after_a_manual_pps_override() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = MemoryConfig::default();
    for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
        memory_config.heater_curve_raw_observations.points[index] =
            Some(HeaterCurveRawObservation {
                raw_rtd_adc_mv,
                heater_voltage_mv: 20_000,
                heater_current_ma: 3_000,
                resistance_milliohms: 4_000,
            });
    }
    let mut manual_pps =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(20_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }));
    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();
    manual_pps
        .enable(ManualPpsOwner::Debug, 20_000, Some(3_000))
        .unwrap();

    update_calibration_job_state(
        &mut calibration,
        &mut memory_config,
        &mut manual_pps,
        CalibrationJobUpdateInput {
            now_ms: 0,
            latest_rtd_raw_code: 1_500,
            latest_vin_raw_code: 1_000,
            wifi_started: Some(false),
            pd_confirmed: true,
            latest_rtd_raw_adc_mv: 0,
            latest_vin_raw_adc_mv: 0,
            latest_temp_c: 20.0,
            latest_vin_mv: 20_000,
            heater_duty_percent: 0,
        },
    );

    assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
    assert_eq!(calibration.job.samples_collected, 0);
    assert_eq!(memory_config.thermal_plant_active, None);
}

#[test]
fn thermal_plant_job_rejects_invalid_adc_and_waits_for_an_unconfirmed_contract() {
    for invalid_adc in [false, true] {
        let mut calibration = CalibrationRuntimeState::default();
        let mut config = MemoryConfig::default();
        let mut manual = thermal_plant_job_manual_pps(20_000, 3_000);
        let mut workspace = CalibrationThermalPlantWorkspace::default();
        calibration_job_start_with_workspace(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut config,
            &mut manual,
            &mut workspace,
        )
        .unwrap();
        let mut plant = SupplyCalibrationPlant {
            temperature_c: 25.0,
            now_ms: 0,
            saw_board_voltage_drop: false,
        };
        while plant.now_ms < 10_000 && !calibration.heater_enabled {
            plant.tick(&mut calibration, &mut config, &mut manual, &mut workspace);
        }
        assert!(calibration.heater_enabled);
        let samples_before_unconfirmed = workspace.job.as_ref().map_or(0, |job| job.sample_count);
        update_calibration_job_state_with_workspace(
            &mut calibration,
            &mut config,
            &mut manual,
            &mut workspace,
            CalibrationJobUpdateInput {
                now_ms: plant.now_ms,
                latest_rtd_raw_code: 1_500,
                latest_vin_raw_code: 1_000,
                wifi_started: Some(false),
                pd_confirmed: invalid_adc,
                latest_rtd_raw_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(26.0),
                latest_vin_raw_adc_mv: if invalid_adc { 0 } else { 400 },
                latest_temp_c: 26.0,
                latest_vin_mv: if invalid_adc { 0 } else { 5_500 },
                heater_duty_percent: 100,
            },
        );
        assert!(!calibration.heater_enabled);
        assert_eq!(
            calibration.job.status,
            if invalid_adc {
                CalibrationJobStatus::Failed
            } else {
                CalibrationJobStatus::Running
            }
        );
        if !invalid_adc {
            assert_eq!(
                workspace.job.as_ref().map_or(0, |job| job.sample_count),
                samples_before_unconfirmed,
                "unconfirmed PPS frames must not enter the transient fit",
            );
        }
        assert_eq!(manual.enabled, !invalid_adc);
        assert_eq!(take_immediate_heater_disarm(&mut calibration), invalid_adc);
        assert_eq!(config.thermal_plant_transient_active, None);
    }
}

#[test]
fn memory_record_write_chunk_len_keeps_i2c_frames_small_and_page_aligned() {
    assert_eq!(memory_record_write_chunk_len(0x0400, 128), 16);
    assert_eq!(memory_record_write_chunk_len(0x0418, 128), 8);
    assert_eq!(memory_record_write_chunk_len(0x041f, 128), 1);
    assert_eq!(memory_record_write_chunk_len(0x0420, 7), 7);
}

#[test]
fn raw_eeprom_writes_split_at_page_boundaries_from_any_offset() {
    assert_eq!(eeprom_maintenance_write_chunk_len(0x001f, 2), 1);
    assert_eq!(eeprom_maintenance_write_chunk_len(0x0020, 17), 16);
    assert_eq!(eeprom_maintenance_write_chunk_len(0x003f, 16), 1);
}

#[test]
fn raw_eeprom_maintenance_yields_between_bounded_transactions() {
    let source = include_str!("eeprom.rs");

    assert!(!source.contains("service_pd_during_eeprom_operation"));
    assert!(!source.contains("EepromMaintenancePdServiceSchedule"));
    assert!(source.contains("EmbassyTimer::after_millis(0).await;"));
}

#[test]
fn deferred_memory_commit_waits_until_all_heater_outputs_are_off() {
    assert!(!runtime_memory_commit_permitted(true, false, false, 0, 0));
    assert!(!runtime_memory_commit_permitted(false, true, false, 0, 0));
    assert!(!runtime_memory_commit_permitted(false, false, true, 0, 0));
    assert!(!runtime_memory_commit_permitted(false, false, false, 1, 0));
    assert!(!runtime_memory_commit_permitted(false, false, false, 0, 1));
    assert!(runtime_memory_commit_permitted(false, false, false, 0, 0));
}

#[test]
fn non_blank_eeprom_without_a_valid_record_is_incompatible() {
    let blank = [0xff; EEPROM_UNUSED_GAP_LEN];
    let mut incompatible_gap = blank;
    incompatible_gap[0] = 0x7e;

    assert!(!eeprom_data_is_incompatible(
        false,
        eeprom_bytes_contain_data(&blank)
    ));
    assert!(eeprom_data_is_incompatible(
        false,
        eeprom_bytes_contain_data(&incompatible_gap)
    ));
    assert!(!eeprom_data_is_incompatible(
        true,
        eeprom_bytes_contain_data(&incompatible_gap)
    ));
}

#[test]
fn eeprom_maintenance_waits_for_delayed_power_confirmation() {
    assert_eq!(eeprom_power_disarm_result(None, 0), None);
    assert_eq!(eeprom_power_disarm_result(None, 200), None);
    assert_eq!(eeprom_power_disarm_result(Some(true), 250), Some(true));
}

#[test]
fn eeprom_maintenance_rejects_failed_or_expired_power_confirmation() {
    assert_eq!(eeprom_power_disarm_result(Some(false), 250), Some(false));
    assert_eq!(
        eeprom_power_disarm_result(None, EEPROM_POWER_DISARM_TIMEOUT_MS - 1),
        None
    );
    assert_eq!(
        eeprom_power_disarm_result(None, EEPROM_POWER_DISARM_TIMEOUT_MS),
        Some(false)
    );
    assert_eq!(
        eeprom_power_disarm_result(Some(true), EEPROM_POWER_DISARM_TIMEOUT_MS + 1),
        Some(false)
    );
}

#[test]
fn raw_eeprom_maintenance_locks_writes_and_clears_erased_runtime_state() {
    let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut config = MemoryConfig {
        target_temp_c: 180,
        ..MemoryConfig::default()
    };
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::HeaterCurve,
        pps_enabled: true,
        pps_mv: Some(20_000),
        pps_ma: Some(3_000),
        heater_enabled: true,
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState {
        enabled: true,
        owner: ManualPpsOwner::Debug,
        target_mv: Some(20_000),
        target_ma: Some(3_000),
        applied_mv: Some(20_000),
        ..ManualPpsState::default()
    };
    let sequence = 9;
    let mut commit_due_ms = Some(20);

    begin_mutating_eeprom_maintenance(
        &mut state,
        &mut calibration,
        &mut manual_pps,
        &mut commit_due_ms,
    );
    assert!(raw_eeprom_operation_mutates(EepromMaintenanceOp::Write));
    assert!(raw_eeprom_operation_mutates(EepromMaintenanceOp::Erase));
    assert!(!raw_eeprom_operation_mutates(EepromMaintenanceOp::Read));
    assert!(state.eeprom_data_incompatible);
    state.eeprom_data_incompatible = false;
    state.eeprom_required = false;
    commit_due_ms = Some(42);
    mark_eeprom_required(
        &mut state,
        &mut calibration,
        &mut manual_pps,
        &mut commit_due_ms,
        None,
    );
    assert!(state.eeprom_required);
    assert!(state.persistence_locked());
    assert_eq!(commit_due_ms, None);
    assert!(!manual_pps.enabled);
    assert!(manual_pps.automatic_restore_pending);
    assert_eq!(calibration.mode, CalibrationMode::Off);
    assert!(!calibration.heater_enabled);
    assert!(calibration.immediate_heater_disarm_pending);
    assert_eq!(commit_due_ms, None);

    apply_successful_eeprom_maintenance_operation(
        EepromMaintenanceOp::Write,
        &mut state,
        &mut config,
        &mut commit_due_ms,
    );
    assert!(state.eeprom_data_incompatible);
    assert_eq!(config.target_temp_c, 180);

    state.eeprom_data_incompatible = false;
    state.eeprom_required = false;
    apply_successful_eeprom_maintenance_operation(
        EepromMaintenanceOp::Erase,
        &mut state,
        &mut config,
        &mut commit_due_ms,
    );
    assert_eq!(config, MemoryConfig::default());
    assert_eq!(sequence, 9);
    assert_eq!(commit_due_ms, None);
    assert_eq!(state.target_temp_c, MemoryConfig::default().target_temp_c);
    assert!(!state.eeprom_data_incompatible);

    commit_due_ms = Some(42);
    discard_deferred_memory_commit_for_incompatible_eeprom(true, &mut commit_due_ms);
    assert_eq!(commit_due_ms, None);
}

#[test]
fn heater_control_saturates_when_far_below_target() {
    let mut controller = HeaterController::new();
    let snapshot = controller.update(380, 25.0, true, None);

    assert_eq!(snapshot.duty_percent, 100);
    assert!(snapshot.error_c > 300.0);
    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
    assert_eq!(controller.fault_latched(), None);
}

#[test]
fn warmup_soft_start_completes_in_100ms_per_arm_and_target_change() {
    let mut controller = HeaterController::new();
    let armed = controller.update_at(140, 25.0, true, None, 1_000);
    assert_eq!(armed.warmup_soft_start_percent, 0);

    let mid_ramp = controller.update_at(140, 25.0, true, None, 1_050);
    assert_eq!(mid_ramp.warmup_soft_start_percent, 50);

    let completed = controller.update_at(140, 25.0, true, None, 1_100);
    assert_eq!(completed.warmup_soft_start_percent, 100);

    let target_changed = controller.update_at(180, 25.0, true, None, 3_000);
    assert_eq!(target_changed.warmup_soft_start_percent, 0);

    let disabled = controller.update_at(180, 25.0, false, None, 4_000);
    assert_eq!(disabled.warmup_soft_start_percent, 0);

    let rearmed = controller.update_at(180, 25.0, true, None, 5_000);
    assert_eq!(rearmed.warmup_soft_start_percent, 0);
}

#[test]
fn warmup_soft_start_restarts_after_approach_reentry() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.heater_was_enabled = true;
    controller.phase = HeaterControlPhase::Approach;
    controller.filtered_temp_c = Some(25.0);
    controller.previous_filtered_temp_c = Some(25.0);
    controller.previous_measured_temp_c = Some(25.0);
    controller.warmup_started_at_ms = Some(0);

    let snapshot = controller.update_at(140, 25.0, true, None, 4_000);

    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
    assert_eq!(snapshot.warmup_soft_start_percent, 0);
}

#[test]
fn heater_control_deadline_preserves_cadence_without_catch_up_updates() {
    assert_eq!(
        next_heater_control_deadline_ms(HEATER_CONTROL_INTERVAL_MS, HEATER_CONTROL_INTERVAL_MS + 4),
        HEATER_CONTROL_INTERVAL_MS * 2
    );
    assert_eq!(
        next_heater_control_deadline_ms(
            HEATER_CONTROL_INTERVAL_MS * 2,
            HEATER_CONTROL_INTERVAL_MS * 2 + 1,
        ),
        HEATER_CONTROL_INTERVAL_MS * 3
    );
    assert_eq!(
        next_heater_control_deadline_ms(
            HEATER_CONTROL_INTERVAL_MS * 3,
            HEATER_CONTROL_INTERVAL_MS * 4 + 17,
        ),
        HEATER_CONTROL_INTERVAL_MS * 5
    );
}

#[test]
fn warmup_handoff_expands_with_measured_thermal_momentum() {
    let handoff_error = warmup_handoff_error_c(5.0, 10.0, 6.6, 5);

    assert!((handoff_error - 14.9).abs() < 0.01);
}

#[test]
fn warmup_handoff_keeps_static_brake_for_slow_rise() {
    let handoff_error = warmup_handoff_error_c(5.0, 10.0, 0.8, 5);

    assert!((handoff_error - 5.0).abs() < 0.01);
}

#[test]
fn warmup_handoff_rejects_raw_temperature_jump_while_filter_lags() {
    assert!(!warmup_handoff_ready(14.2, 20.8, 20.4, 5.0, 14.9));
}

#[test]
fn warmup_handoff_accepts_confirmed_temperature_momentum() {
    assert!(warmup_handoff_ready(5.8, 6.4, 14.8, 5.0, 14.9));
}

#[test]
fn warmup_handoff_rejects_predictive_ready_when_actual_is_still_far_from_brake() {
    assert!(!warmup_handoff_ready(14.2, 14.8, 14.8, 5.0, 14.9));
}

#[test]
fn warmup_handoff_accepts_actual_temperature_inside_static_brake_with_bounded_filter_lag() {
    assert!(warmup_handoff_ready(4.9, 5.2, 7.8, 5.0, 14.9));
}

#[test]
fn warmup_handoff_rejects_actual_temperature_inside_static_brake_when_filter_is_far_behind() {
    assert!(!warmup_handoff_ready(4.9, 5.2, 20.4, 5.0, 14.9));
}

#[test]
fn warmup_handoff_requires_previous_actual_confirmation_inside_static_brake() {
    assert!(!warmup_handoff_ready(4.9, 20.4, 20.4, 5.0, 14.9));
}

#[test]
fn warmup_handoff_requires_previous_actual_confirmation_for_predictive_ready() {
    assert!(!warmup_handoff_ready(14.2, 16.1, 14.8, 5.0, 14.9));
}

#[test]
fn warmup_handoff_rejects_single_sample_actual_overshoot_when_filter_lags() {
    assert!(!warmup_handoff_ready(-7.6, 11.4, 14.8, 11.0, 14.9));
}

#[test]
fn heater_warmup_hands_off_early_when_rise_rate_is_high() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.filtered_temp_c = Some(53.8);
    controller.previous_filtered_temp_c = Some(53.2);
    controller.previous_measured_temp_c = Some(53.6);
    controller.phase = HeaterControlPhase::Warmup;

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            warmup_reenter_error_c: 10.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 500,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 590,
                approach_floor_power_permille: 510,
                approach_damping_exponent_permille: 1_320,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
                hold_reheat_power_permille: 270,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 540,
                hold_on_centi_c: 30,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 150,
                hold_kp_permille_per_c: 55,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 1,
                approach_lead_ticks: 5,
                hold_lead_ticks: 2,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 54.2, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.error_c > 5.0);
    assert!(snapshot.duty_percent < 100);
}

#[test]
fn heater_control_reduces_output_as_temperature_rises() {
    let mut controller = HeaterController::new();
    let mut snapshots = Vec::new();
    let mut now_ms = 0;
    for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2] {
        let mut snapshot = controller.update_at(100, measured, true, None, now_ms);
        for _ in 1..20 {
            now_ms += HEATER_CONTROL_INTERVAL_MS;
            snapshot = controller.update_at(100, measured, true, None, now_ms);
        }
        now_ms += HEATER_CONTROL_INTERVAL_MS;
        snapshots.push(snapshot);
    }

    assert_eq!(snapshots[0].duty_percent, 100);
    assert!(snapshots[3].duty_percent >= snapshots[4].duty_percent);
    assert!(
        snapshots[5].duty_percent < snapshots[0].duty_percent,
        "snapshots={snapshots:?}"
    );
}

#[test]
fn heater_control_stays_aggressive_through_approach_band() {
    let mut controller = HeaterController::new();
    let mut snapshot = controller.update(100, 25.0, true, None);
    for measured in [40.0, 60.0, 80.0, 92.0, 96.0, 96.0, 96.0] {
        snapshot = controller.update(100, measured, true, None);
    }

    assert!(snapshot.duty_percent >= HEATER_APPROACH_DUTY_PERCENT);
}

#[test]
fn heater_control_keeps_full_power_warmup_during_warmup() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Warmup;
    controller.filtered_temp_c = Some(39.0);
    controller.previous_filtered_temp_c = Some(37.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.7,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 1,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 100,
                approach_floor_power_permille: 25,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 45,
                hold_reheat_power_permille: 60,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 150,
                hold_kp_permille_per_c: 32,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 8,
                approach_lead_ticks: 10,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 40.5, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
    assert_eq!(snapshot.duty_percent, 100);
    assert!(snapshot.error_c > 10.0);
}

#[test]
fn heater_control_warmup_ignores_profile_power_caps() {
    let mut controller = HeaterController::new();
    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 420,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 280,
                hold_reheat_power_permille: 340,
                hold_entry_centi_c: 10,
                hold_exit_centi_c: 55,
                hold_on_centi_c: 0,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 22,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 4,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 30.0, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
    assert_eq!(snapshot.duty_percent, 100);
}

#[test]
fn heater_control_requires_actual_margin_before_entering_hold() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 180;
    controller.phase = HeaterControlPhase::Approach;
    controller.filtered_temp_c = Some(178.8);
    controller.previous_filtered_temp_c = Some(178.0);
    controller.duty_percent = 53;

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 180,
                brake_distance_centi_c: 650,
                warmup_power_permille: 760,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 760,
                approach_floor_power_permille: 460,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 450,
                hold_reheat_power_permille: 620,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 70,
                hold_on_centi_c: 0,
                hold_off_centi_c: 240,
                overshoot_cutoff_centi_c: 300,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 3,
                approach_lead_ticks: 2,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(180, 179.4, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.error_c > 0.5);
}

#[test]
fn heater_control_resets_when_disabled() {
    let mut controller = HeaterController::new();
    let enabled = controller.update(380, 25.0, true, None);
    let disabled = controller.update(380, 40.0, false, None);

    assert!(enabled.duty_percent > 0);
    assert_eq!(disabled.duty_percent, 0);
    assert_eq!(disabled.filtered_temp_c, 40.0);
    assert_eq!(disabled.phase, HeaterControlPhase::Warmup);
}

#[test]
fn heater_fault_latch_requires_manual_clear() {
    let mut controller = HeaterController::new();
    let overtemp = controller.update(380, 421.0, true, None);
    assert_eq!(overtemp.duty_percent, 0);
    assert_eq!(
        controller.fault_latched(),
        Some(HeaterFaultReason::OverTemp)
    );

    let still_latched = controller.update(380, 200.0, true, None);
    assert_eq!(still_latched.duty_percent, 0);
    assert_eq!(
        controller.fault_latched(),
        Some(HeaterFaultReason::OverTemp)
    );

    controller.clear_fault_latch();
    let rearmed = controller.update(380, 200.0, true, None);
    assert!(rearmed.duty_percent > 0);
    assert_eq!(controller.fault_latched(), None);
}

#[test]
fn heater_control_reapplies_power_when_temperature_falls_below_target() {
    let mut controller = HeaterController::new();
    let mut now_ms = 0;

    for measured in [25.0, 40.0, 55.0, 70.0, 82.0, 90.0, 96.0, 99.2, 100.4] {
        for _ in 0..20 {
            let _ = controller.update_at(100, measured, true, None, now_ms);
            now_ms += HEATER_CONTROL_INTERVAL_MS;
        }
    }

    let mut cooling = controller.update_at(100, 99.6, true, None, now_ms);
    for step in 1..=12 {
        now_ms += HEATER_CONTROL_INTERVAL_MS;
        let measured = 99.6 - (step as f32 * 0.06);
        cooling = controller.update_at(100, measured, true, None, now_ms);
    }
    assert!(cooling.duty_percent > 0);
    assert!(matches!(
        cooling.phase,
        HeaterControlPhase::Approach | HeaterControlPhase::Hold
    ));
}

#[test]
fn heater_control_cuts_power_on_overshoot() {
    let mut controller = HeaterController::new();
    for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2, 99.8] {
        let _ = controller.update(100, measured, true, None);
    }

    let overshoot = controller.update(100, 101.0, true, None);
    assert_eq!(overshoot.duty_percent, 0);
}

#[test]
fn heater_control_hold_reapplies_small_power_near_target_without_waiting_for_large_drop() {
    let mut controller = HeaterController::new();
    let mut now_ms = 0;
    for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2, 99.8, 100.3] {
        for _ in 0..20 {
            let _ = controller.update_at(100, measured, true, None, now_ms);
            now_ms += HEATER_CONTROL_INTERVAL_MS;
        }
    }

    let mut near_target = controller.update_at(100, 99.95, true, None, now_ms);
    for step in 1..=12 {
        now_ms += HEATER_CONTROL_INTERVAL_MS;
        let measured = 99.95 - (step as f32 * 0.06);
        near_target = controller.update_at(100, measured, true, None, now_ms);
    }
    assert!(near_target.coast_active);
    assert_eq!(near_target.duty_percent, 0);
    // Continue real cooling observations until the filtered low-error gate
    // catches up, without simulating a large temperature drop.
    for step in 13..=24 {
        now_ms += HEATER_CONTROL_INTERVAL_MS;
        let measured = 99.95 - (step as f32 * 0.06);
        near_target = controller.update_at(100, measured, true, None, now_ms);
        if near_target.duty_percent > 0 {
            break;
        }
    }
    assert!(near_target.error_c < 1.5);
    assert!(matches!(
        near_target.phase,
        HeaterControlPhase::Approach | HeaterControlPhase::Hold
    ));
    assert!(near_target.duty_percent > 0);
}

#[test]
fn heater_approach_timeout_does_not_force_hold_on_raw_temp_spike() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = control_cycles_from_profile_ticks(u16::from(
        ThermalControlProfileSettings::default().approach_max_ticks,
    ));
    controller.filtered_temp_c = Some(58.32);
    controller.previous_filtered_temp_c = Some(58.33);
    controller.duty_percent = 0;

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 320,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 100,
                approach_floor_power_permille: 25,
                approach_damping_exponent_permille: 1_500,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 60,
                hold_reheat_power_permille: 100,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 150,
                hold_kp_permille_per_c: 40,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 6,
                approach_lead_ticks: 10,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 59.9, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.control_error_c > 1.0);
    assert!(snapshot.duty_percent <= 10);
}

#[test]
fn heater_hold_lead_does_not_force_zero_for_small_actual_overshoot() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 100;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 8;
    controller.duty_percent = 40;
    controller.filtered_temp_c = Some(100.0);
    controller.previous_filtered_temp_c = Some(99.7);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 900,
                warmup_power_permille: 300,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 300,
                approach_floor_power_permille: 150,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
                hold_reheat_power_permille: 260,
                hold_entry_centi_c: 12,
                hold_exit_centi_c: 60,
                hold_on_centi_c: 0,
                hold_off_centi_c: 30,
                overshoot_cutoff_centi_c: 50,
                hold_kp_permille_per_c: 65,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 1,
                approach_lead_ticks: 0,
                hold_lead_ticks: 8,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(100, 100.2, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.duty_percent > 0);
}

#[test]
fn heater_hold_filter_lag_does_not_reheat_while_actual_temp_is_above_target_and_rising() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 100;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 8;
    controller.duty_percent = 22;
    controller.filtered_temp_c = Some(99.6);
    controller.previous_filtered_temp_c = Some(99.3);
    controller.previous_measured_temp_c = Some(99.9);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.4,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 300,
                approach_damping_exponent_permille: 1_220,
                approach_tail_window_centi_c: 375,
                hold_power_permille: 220,
                hold_reheat_power_permille: 220,
                hold_entry_centi_c: 150,
                hold_exit_centi_c: 120,
                hold_on_centi_c: 10,
                hold_off_centi_c: 180,
                overshoot_cutoff_centi_c: 90,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 2,
                approach_lead_ticks: 7,
                hold_lead_ticks: 8,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(100, 100.21, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c < 0.0);
    assert!(snapshot.control_error_c > 0.0);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert_eq!(snapshot.duty_percent, 0);
}

#[test]
fn heater_hold_does_not_reheat_while_actual_and_filtered_are_above_target_and_rising() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 100;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 8;
    controller.duty_percent = 22;
    controller.filtered_temp_c = Some(99.9);
    controller.previous_filtered_temp_c = Some(99.68);
    controller.previous_measured_temp_c = Some(100.71);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.99,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 1_300,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 340,
                approach_floor_power_permille: 220,
                approach_damping_exponent_permille: 1_500,
                approach_tail_window_centi_c: 375,
                hold_power_permille: 220,
                hold_reheat_power_permille: 220,
                hold_entry_centi_c: 150,
                hold_exit_centi_c: 120,
                hold_on_centi_c: 10,
                hold_off_centi_c: 50,
                overshoot_cutoff_centi_c: 50,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 2,
                approach_lead_ticks: 9,
                hold_lead_ticks: 8,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(100, 100.25, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c < 0.0);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert_eq!(snapshot.duty_percent, 0);
}

#[test]
fn heater_hold_overshoot_does_not_leave_negative_integral_dead_zone() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 12;
    controller.duty_percent = 0;
    controller.filtered_temp_c = Some(219.8);
    controller.previous_filtered_temp_c = Some(219.8);
    controller.hold_integral_c = -40.0;

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 320,
                warmup_power_permille: 920,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 920,
                approach_floor_power_permille: 780,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 800,
                hold_reheat_power_permille: 900,
                hold_entry_centi_c: 8,
                hold_exit_centi_c: 50,
                hold_on_centi_c: 0,
                hold_off_centi_c: 100,
                overshoot_cutoff_centi_c: 120,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 219.6, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.duty_percent > 0);
}

#[test]
fn heater_hold_softens_mild_overshoot_instead_of_hard_cutoff() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 6;
    controller.duty_percent = 36;
    controller.filtered_temp_c = Some(140.0);
    controller.previous_filtered_temp_c = Some(140.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 440,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 440,
                approach_floor_power_permille: 240,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 360,
                hold_reheat_power_permille: 420,
                hold_entry_centi_c: 15,
                hold_exit_centi_c: 65,
                hold_on_centi_c: 0,
                hold_off_centi_c: 80,
                overshoot_cutoff_centi_c: 120,
                hold_kp_permille_per_c: 30,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 4,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 140.9, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.duty_percent > 0);
    assert!(snapshot.duty_percent < 36);
}

#[test]
fn heater_hold_ignores_single_under_target_dip_until_filtered_error_grows() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 8;
    controller.duty_percent = 7;
    controller.filtered_temp_c = Some(59.8);
    controller.previous_filtered_temp_c = Some(59.8);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 320,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 100,
                approach_floor_power_permille: 25,
                approach_damping_exponent_permille: 1_500,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 60,
                hold_reheat_power_permille: 100,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 150,
                hold_kp_permille_per_c: 40,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 6,
                approach_lead_ticks: 10,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 58.9, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.control_error_c < 0.9);
}

#[test]
fn heater_hold_entry_band_does_not_amplify_filtered_lag_into_pi_power() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 2;
    controller.filtered_temp_c = Some(55.0);
    controller.previous_filtered_temp_c = Some(54.6);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_310,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 590,
                approach_floor_power_permille: 510,
                approach_damping_exponent_permille: 1_370,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 60,
                hold_reheat_power_permille: 60,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 540,
                hold_on_centi_c: 30,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 80,
                hold_kp_permille_per_c: 8,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 1,
                approach_lead_ticks: 3,
                hold_lead_ticks: 2,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 59.4, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.control_error_c > 4.0);
    assert!(snapshot.duty_percent <= 7);
}

#[test]
fn heater_hold_coasts_after_predictive_cut_until_temperature_is_falling() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Approach;
    controller.duty_percent = 0;
    controller.filtered_temp_c = Some(58.0);
    controller.previous_filtered_temp_c = Some(57.5);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_310,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 590,
                approach_floor_power_permille: 510,
                approach_damping_exponent_permille: 1_370,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 60,
                hold_reheat_power_permille: 60,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 540,
                hold_on_centi_c: 30,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 80,
                hold_kp_permille_per_c: 8,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 1,
                approach_lead_ticks: 3,
                hold_lead_ticks: 2,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let entered_hold = controller.update(60, 59.4, true, Some(profile));
    assert_eq!(entered_hold.phase, HeaterControlPhase::Approach);
    let entered_hold = controller.update(60, 59.5, true, Some(profile));
    assert_eq!(entered_hold.phase, HeaterControlPhase::Hold);
    assert!(controller.hold_coast_active);
    assert_eq!(entered_hold.duty_percent, 0);

    let rising = controller.update(60, 61.0, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(rising.duty_percent, 0);

    let falling_above_target = controller.update(60, 60.8, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(falling_above_target.duty_percent, 0);

    controller.filtered_temp_c = Some(59.9);
    controller.previous_filtered_temp_c = Some(60.0);
    controller.filtered_slope_c_per_profile_tick = -0.5;
    controller.previous_measured_temp_c = Some(60.0);
    let raw_dip = controller.update(60, 59.5, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(raw_dip.duty_percent, 0);

    controller.filtered_temp_c = Some(59.3);
    controller.previous_filtered_temp_c = Some(59.4);
    controller.filtered_slope_c_per_profile_tick = -0.5;
    controller.previous_measured_temp_c = Some(59.6);
    let falling_under_target = controller.update(60, 59.4, true, Some(profile));
    assert!(falling_under_target.coast_active);
    assert_eq!(falling_under_target.duty_percent, 0);
    let falling_under_target = controller.update(60, 59.3, true, Some(profile));
    assert!(!controller.hold_coast_active);
    assert!(falling_under_target.duty_percent > 0);
    assert_eq!(controller.phase_ticks, 0);
    assert_eq!(controller.hold_entry_output_percent, 0);
}

#[test]
fn heater_hold_coasts_when_projection_crosses_target_with_nonzero_approach_power() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 20;
    controller.duty_percent = 42;
    controller.filtered_temp_c = Some(132.57);
    controller.previous_filtered_temp_c = Some(131.97);
    controller.filtered_slope_c_per_profile_tick = 2.4;
    controller.previous_measured_temp_c = Some(139.2);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.26,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 340,
                hold_reheat_power_permille: 420,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 160,
                hold_on_centi_c: 30,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 40,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 5,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 139.5, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(controller.hold_coast_active);
    assert_eq!(snapshot.duty_percent, 0);

    let rising_above_target = controller.update(140, 141.0, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(rising_above_target.duty_percent, 0);

    let falling_above_target = controller.update(140, 140.8, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(falling_above_target.duty_percent, 0);

    controller.filtered_temp_c = Some(139.9);
    controller.previous_filtered_temp_c = Some(140.0);
    controller.filtered_slope_c_per_profile_tick = -0.5;
    controller.previous_measured_temp_c = Some(140.0);
    let raw_dip = controller.update(140, 139.5, true, Some(profile));
    assert!(controller.hold_coast_active);
    assert_eq!(raw_dip.duty_percent, 0);

    controller.filtered_temp_c = Some(139.3);
    controller.previous_filtered_temp_c = Some(139.4);
    controller.filtered_slope_c_per_profile_tick = -0.5;
    controller.previous_measured_temp_c = Some(139.6);
    let falling_under_target = controller.update(140, 139.4, true, Some(profile));
    assert!(falling_under_target.coast_active);
    assert_eq!(falling_under_target.duty_percent, 0);
    let falling_under_target = controller.update(140, 139.3, true, Some(profile));
    assert!(!controller.hold_coast_active);
    assert!(falling_under_target.duty_percent > 0);
}

#[test]
fn heater_approach_under_temperature_cannot_enter_hold_after_zero_output() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 180;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 20;
    controller.duty_percent = 0;
    controller.filtered_temp_c = Some(177.8);
    controller.previous_filtered_temp_c = Some(177.6);
    controller.filtered_slope_c_per_profile_tick = 0.2;
    controller.previous_measured_temp_c = Some(178.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.26,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            None,
            None,
            None,
            Some(ThermalControlProfilePoint {
                target_temp_c: 180,
                brake_distance_centi_c: 875,
                warmup_power_permille: 950,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 710,
                approach_floor_power_permille: 410,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 420,
                hold_reheat_power_permille: 560,
                hold_entry_centi_c: 180,
                hold_exit_centi_c: 70,
                hold_on_centi_c: 25,
                hold_off_centi_c: 225,
                overshoot_cutoff_centi_c: 250,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 3,
                approach_lead_ticks: 4,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(180, 178.3, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(!controller.hold_coast_active);
    assert!(snapshot.duty_percent > 0);
}

#[test]
fn heater_approach_projection_cannot_coast_outside_hold_exit_gate() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Approach;
    controller.filtered_temp_c = Some(136.0);
    controller.previous_filtered_temp_c = Some(135.0);
    controller.filtered_slope_c_per_profile_tick = 1.0;
    controller.previous_measured_temp_c = Some(136.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 600,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 700,
                approach_floor_power_permille: 260,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 180,
                hold_reheat_power_permille: 320,
                hold_entry_centi_c: 50,
                hold_exit_centi_c: 100,
                hold_on_centi_c: 30,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 24,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 5,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 137.0, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.control_error_c > 1.0);
    assert!(snapshot.duty_percent >= 26);
    assert!(snapshot.duty_percent < 32);
}

#[test]
fn heater_hold_reheats_without_reentering_approach() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 16;
    controller.duty_percent = 72;
    controller.filtered_temp_c = Some(219.84);
    controller.previous_filtered_temp_c = Some(220.02);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            hold_on_error_c: 2.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            None,
            None,
            None,
            None,
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 500,
                warmup_power_permille: 980,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 920,
                approach_floor_power_permille: 730,
                approach_damping_exponent_permille: 250,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 720,
                hold_reheat_power_permille: 790,
                hold_entry_centi_c: 28,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 210,
                overshoot_cutoff_centi_c: 275,
                hold_kp_permille_per_c: 26,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 10,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 218.6, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.control_error_c > 0.9);

    let follow_up = controller.update(220, 217.6, true, Some(profile));
    assert_eq!(follow_up.phase, HeaterControlPhase::Hold);
    assert!(follow_up.control_error_c > 2.0);

    let confirmed_drop = controller.update(220, 217.5, true, Some(profile));
    assert_eq!(confirmed_drop.phase, HeaterControlPhase::Hold);
    assert!(confirmed_drop.duty_percent >= snapshot.duty_percent);
}

#[test]
fn heater_hold_keeps_phase_when_actual_under_target_error_outpaces_filter() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 20;
    controller.duty_percent = 78;
    controller.filtered_temp_c = Some(219.0);
    controller.previous_filtered_temp_c = Some(219.2);
    controller.previous_measured_temp_c = Some(217.5);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.25,
            hold_on_error_c: 2.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            None,
            None,
            None,
            None,
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 500,
                warmup_power_permille: 980,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 920,
                approach_floor_power_permille: 730,
                approach_damping_exponent_permille: 250,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 720,
                hold_reheat_power_permille: 790,
                hold_entry_centi_c: 28,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 210,
                overshoot_cutoff_centi_c: 275,
                hold_kp_permille_per_c: 26,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 10,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 217.6, true, Some(profile));
    assert!(snapshot.error_c > 2.0);
    assert!(snapshot.control_error_c < 2.0);
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
}

#[test]
fn heater_hold_blend_does_not_keep_approach_output_after_target_cross() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 0;
    controller.duty_percent = 100;
    controller.hold_entry_output_percent = 100;
    controller.filtered_temp_c = Some(219.8);
    controller.previous_filtered_temp_c = Some(219.8);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 450,
                warmup_power_permille: 900,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 900,
                approach_floor_power_permille: 740,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 820,
                hold_reheat_power_permille: 900,
                hold_entry_centi_c: 8,
                hold_exit_centi_c: 50,
                hold_on_centi_c: 0,
                hold_off_centi_c: 180,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 16,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 8,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 220.3, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.duty_percent < 100);
}

#[test]
fn heater_hold_entry_does_not_preload_integral_when_residual_heat_is_already_spent() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 1;
    controller.duty_percent = 76;
    controller.filtered_temp_c = Some(219.0);
    controller.previous_filtered_temp_c = Some(218.0);
    controller.previous_measured_temp_c = Some(220.3);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 520,
                warmup_power_permille: 760,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 760,
                approach_floor_power_permille: 600,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 620,
                hold_reheat_power_permille: 700,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 50,
                hold_on_centi_c: 0,
                hold_off_centi_c: 240,
                overshoot_cutoff_centi_c: 320,
                hold_kp_permille_per_c: 22,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 5,
                approach_lead_ticks: 2,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 221.7, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert_eq!(controller.hold_integral_c, 0.0);
    assert!(controller.hold_entry_output_percent < 76);
    assert!(snapshot.duty_percent < 76);
}

#[test]
fn heater_approach_uses_hold_base_without_inheriting_reheat_floor() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 8;
    controller.duty_percent = 41;
    controller.filtered_temp_c = Some(139.0);
    controller.previous_filtered_temp_c = Some(139.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 780,
                warmup_power_permille: 640,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 600,
                approach_floor_power_permille: 360,
                approach_damping_exponent_permille: 700,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 380,
                hold_reheat_power_permille: 520,
                hold_entry_centi_c: 10,
                hold_exit_centi_c: 45,
                hold_on_centi_c: 0,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 34,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 4,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 139.0, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.duty_percent >= 38);
    assert!(snapshot.duty_percent < 52);
}

#[test]
fn heater_coast_release_requires_filtered_low_error() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    config.settings.temp_filter_alpha_permille = 1;
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 60;
    point.hold_on_centi_c = 30;
    point.hold_exit_centi_c = 500;
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Hold;
    controller.hold_coast_active = true;
    controller.filtered_temp_c = Some(60.5);
    controller.previous_measured_temp_c = Some(59.8);
    controller.filtered_slope_c_per_profile_tick = -0.5;
    controller.update_at(60, 59.6, true, Some(profile), 1_000);
    let snapshot = controller.update_at(60, 59.4, true, Some(profile), 2_000);
    assert!(snapshot.filtered_temp_c > 59.7);
    assert!(
        snapshot.coast_active,
        "filtered low-error gate has not been met"
    );
    assert_eq!(snapshot.duty_percent, 0);
}

#[test]
fn heater_approach_coast_requires_actual_hold_exit_error() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    config.settings.temp_filter_alpha_permille = 1;
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 140;
    point.hold_entry_centi_c = 50;
    point.hold_exit_centi_c = 100;
    point.approach_lead_ticks = 5;
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Approach;
    controller.filtered_temp_c = Some(139.3);
    controller.previous_measured_temp_c = Some(139.3);
    controller.filtered_slope_c_per_profile_tick = 1.0;
    let snapshot = controller.update_at(140, 138.5, true, Some(profile), 1_000);
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.control_error_c <= 1.0);
    assert!(snapshot.error_c > 1.0);
    assert!(
        snapshot.duty_percent > 0,
        "actual holdExit gate has not been met"
    );
}

#[test]
fn heater_approach_predictive_coast_waits_for_actual_error_to_shrink() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 100;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 4;
    controller.duty_percent = 25;
    controller.filtered_temp_c = Some(97.6);
    controller.previous_filtered_temp_c = Some(96.8);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 860,
                warmup_power_permille: 361,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 361,
                approach_floor_power_permille: 249,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 158,
                hold_reheat_power_permille: 296,
                hold_entry_centi_c: 25,
                hold_exit_centi_c: 73,
                hold_on_centi_c: 0,
                hold_off_centi_c: 140,
                overshoot_cutoff_centi_c: 185,
                hold_kp_permille_per_c: 25,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 5,
                approach_lead_ticks: 5,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(100, 98.8, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.duty_percent > 0);
}

#[test]
fn heater_approach_predictive_coast_cuts_power_once_actual_error_is_hold_ready() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 100;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 4;
    controller.duty_percent = 25;
    controller.filtered_temp_c = Some(98.2);
    controller.previous_filtered_temp_c = Some(97.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 860,
                warmup_power_permille: 361,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 361,
                approach_floor_power_permille: 249,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 158,
                hold_reheat_power_permille: 296,
                hold_entry_centi_c: 25,
                hold_exit_centi_c: 73,
                hold_on_centi_c: 0,
                hold_off_centi_c: 140,
                overshoot_cutoff_centi_c: 185,
                hold_kp_permille_per_c: 25,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 5,
                approach_lead_ticks: 5,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(100, 99.4, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.duty_percent >= 10);
}

#[test]
fn heater_approach_projection_keeps_reheat_when_filter_lags_actual_plate() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 20;
    controller.duty_percent = 20;
    controller.filtered_temp_c = Some(51.64);
    controller.previous_filtered_temp_c = Some(50.94);
    controller.filtered_slope_c_per_profile_tick = 2.8;

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.26,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_910,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 520,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_540,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 90,
                hold_reheat_power_permille: 140,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 200,
                hold_on_centi_c: 30,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 150,
                hold_kp_permille_per_c: 16,
                hold_ki_permille_per_c_tick: 2,
                hold_blend_ticks: 1,
                approach_lead_ticks: 12,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 59.0, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.control_error_c > 2.0);
    assert!(snapshot.duty_percent >= 14);
}

#[test]
fn heater_approach_accepts_previous_sample_within_measurement_margin() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 32;
    controller.duty_percent = 34;
    controller.filtered_temp_c = Some(135.05222);
    controller.previous_filtered_temp_c = Some(134.72865);
    controller.filtered_slope_c_per_profile_tick = 1.29428;
    controller.previous_measured_temp_c = Some(139.0);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.26,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 280,
                hold_reheat_power_permille: 340,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 160,
                hold_on_centi_c: 30,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 22,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 4,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 139.56143, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
}

#[test]
fn heater_hold_residency_is_not_narrower_than_hold_entry_band() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 140;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 1;
    controller.duty_percent = 34;
    controller.filtered_temp_c = Some(138.56);
    controller.previous_filtered_temp_c = Some(138.3);
    controller.previous_measured_temp_c = Some(138.56);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 140,
                brake_distance_centi_c: 1_000,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 420,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 280,
                hold_reheat_power_permille: 340,
                hold_entry_centi_c: 200,
                hold_exit_centi_c: 160,
                hold_on_centi_c: 30,
                hold_off_centi_c: 160,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 22,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 4,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(140, 138.3, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
}

#[test]
fn heater_approach_crossing_target_enters_hold_before_filtered_error_catches_up() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = 12;
    controller.duty_percent = 72;
    controller.filtered_temp_c = Some(218.8);
    controller.previous_filtered_temp_c = Some(218.18);
    controller.filtered_slope_c_per_profile_tick = 2.48;
    controller.previous_measured_temp_c = Some(220.5);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.7,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            None,
            None,
            None,
            None,
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 442,
                warmup_power_permille: 980,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 940,
                approach_floor_power_permille: 760,
                approach_damping_exponent_permille: 250,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 720,
                hold_reheat_power_permille: 880,
                hold_entry_centi_c: 8,
                hold_exit_centi_c: 45,
                hold_on_centi_c: 0,
                hold_off_centi_c: 205,
                overshoot_cutoff_centi_c: 320,
                hold_kp_permille_per_c: 34,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 4,
                approach_lead_ticks: 1,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 221.0, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.control_error_c > 0.45);
    assert!(controller.hold_coast_active);
    assert_eq!(snapshot.duty_percent, 0);

    let follow_up = controller.update(220, 219.6, true, Some(profile));
    assert_eq!(follow_up.phase, HeaterControlPhase::Hold);
    assert!(follow_up.control_error_c < 0.6);
}

#[test]
fn heater_hold_under_target_zero_output_reheats_from_profile_floor() {
    let controller = HeaterController {
        phase: HeaterControlPhase::Hold,
        ..HeaterController::new()
    };
    let target = ThermalControlTarget {
        brake_distance_c: 4.5,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: 4.0,
        approach_power_permille: 900,
        approach_floor_power_permille: 740,
        approach_damping_exponent: 1.0,
        approach_tail_window_c: 0.0,
        hold_power_permille: 820,
        hold_reheat_power_permille: 900,
        hold_entry_error_c: 0.08,
        hold_exit_error_c: 0.5,
        hold_on_error_c: 0.0,
        hold_off_error_c: 3.5,
        overshoot_cutoff_c: 4.5,
        hold_kp_permille_per_c: 12.0,
        hold_ki_permille_per_c_tick: 1.0,
        hold_blend_ticks: 2,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        settings: ThermalControlProfileSettings::default(),
    };

    assert_eq!(
        controller.apply_under_target_reheat_floor(0, 0.4, 0.4, target),
        90
    );
}

#[test]
fn hold_effective_base_blends_toward_reheat_power_under_target() {
    let target = ThermalControlTarget {
        brake_distance_c: 4.5,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: 4.0,
        approach_power_permille: 900,
        approach_floor_power_permille: 740,
        approach_damping_exponent: 1.0,
        approach_tail_window_c: 0.0,
        hold_power_permille: 740,
        hold_reheat_power_permille: 900,
        hold_entry_error_c: 0.08,
        hold_exit_error_c: 1.2,
        hold_on_error_c: 0.0,
        hold_off_error_c: 3.5,
        overshoot_cutoff_c: 4.5,
        hold_kp_permille_per_c: 12.0,
        hold_ki_permille_per_c_tick: 1.0,
        hold_blend_ticks: 2,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        settings: ThermalControlProfileSettings::default(),
    };

    assert_eq!(hold_effective_base_permille(-0.2, 1.2, target), 740.0);
    assert_eq!(hold_effective_base_permille(0.0, 1.2, target), 740.0);
    assert!((hold_effective_base_permille(0.6, 1.2, target) - 820.0).abs() < 0.01);
    assert_eq!(hold_effective_base_permille(1.2, 1.2, target), 900.0);
}

#[test]
fn heater_hold_under_target_biases_output_above_equilibrium_hold_power() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 220;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = control_cycles_from_profile_ticks(40);
    controller.filtered_temp_c = Some(219.4);
    controller.previous_filtered_temp_c = Some(219.45);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            hold_on_error_c: 1.2,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 220,
                brake_distance_centi_c: 450,
                warmup_power_permille: 900,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 900,
                approach_floor_power_permille: 740,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 740,
                hold_reheat_power_permille: 900,
                hold_entry_centi_c: 8,
                hold_exit_centi_c: 50,
                hold_on_centi_c: 0,
                hold_off_centi_c: 180,
                overshoot_cutoff_centi_c: 220,
                hold_kp_permille_per_c: 12,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 8,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(220, 219.4, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c > 0.5);
    assert!(snapshot.duty_percent >= 82);
}

#[test]
fn heater_hold_does_not_reheat_into_predicted_overshoot() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = control_cycles_from_profile_ticks(4);
    controller.filtered_temp_c = Some(59.5);
    controller.previous_filtered_temp_c = Some(59.4);
    controller.previous_measured_temp_c = Some(59.5);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 1.0,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_050,
                warmup_power_permille: 1_000,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 600,
                approach_floor_power_permille: 300,
                approach_damping_exponent_permille: 4_000,
                approach_tail_window_centi_c: 375,
                hold_power_permille: 170,
                hold_reheat_power_permille: 275,
                hold_entry_centi_c: 70,
                hold_exit_centi_c: 300,
                hold_on_centi_c: 30,
                hold_off_centi_c: 70,
                overshoot_cutoff_centi_c: 100,
                hold_kp_permille_per_c: 32,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 3,
                approach_lead_ticks: 5,
                hold_lead_ticks: 4,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    assert_eq!(profile.control_target(60).hold_lead_ticks, 4);
    let snapshot = controller.update(60, 59.8, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c > 0.0);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert_eq!(snapshot.duty_percent, 0);
}

#[test]
fn heater_hold_prediction_does_not_zero_output_while_plate_is_still_well_below_target() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = control_cycles_from_profile_ticks(4);
    controller.duty_percent = 0;
    controller.filtered_temp_c = Some(57.235535);
    controller.previous_filtered_temp_c = Some(57.195503);
    controller.previous_measured_temp_c = Some(57.66);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_400,
                warmup_power_permille: 740,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 450,
                approach_floor_power_permille: 240,
                approach_damping_exponent_permille: 4_000,
                approach_tail_window_centi_c: 375,
                hold_power_permille: 135,
                hold_reheat_power_permille: 170,
                hold_entry_centi_c: 220,
                hold_exit_centi_c: 400,
                hold_on_centi_c: 30,
                hold_off_centi_c: 50,
                overshoot_cutoff_centi_c: 50,
                hold_kp_permille_per_c: 8,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 10,
                hold_lead_ticks: 6,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 57.81, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c > 2.0);
    assert!(snapshot.control_error_c > 2.0);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert!(snapshot.duty_percent > 0);
}

#[test]
fn heater_approach_does_not_enter_hold_far_below_target_after_zero_output() {
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.phase = HeaterControlPhase::Approach;
    controller.phase_ticks = control_cycles_from_profile_ticks(4);
    controller.duty_percent = 0;
    controller.filtered_temp_c = Some(57.190575);
    controller.previous_filtered_temp_c = Some(57.15819);
    controller.filtered_slope_c_per_profile_tick = 0.402053;
    controller.previous_measured_temp_c = Some(57.45);

    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings {
            temp_filter_alpha: 0.7,
            ..ThermalControlProfileSettings::default()
        },
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 60,
                brake_distance_centi_c: 1_400,
                warmup_power_permille: 740,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 450,
                approach_floor_power_permille: 240,
                approach_damping_exponent_permille: 4_000,
                approach_tail_window_centi_c: 375,
                hold_power_permille: 135,
                hold_reheat_power_permille: 170,
                hold_entry_centi_c: 220,
                hold_exit_centi_c: 400,
                hold_on_centi_c: 30,
                hold_off_centi_c: 50,
                overshoot_cutoff_centi_c: 50,
                hold_kp_permille_per_c: 8,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 1,
                approach_lead_ticks: 10,
                hold_lead_ticks: 6,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let snapshot = controller.update(60, 57.99, true, Some(profile));

    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.error_c > 2.0);
    assert!(snapshot.control_error_c > 2.5);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert!(!controller.hold_coast_active);
    // Predictive coast must not bypass the normal Approach positive-power floor.
    assert!(snapshot.duty_percent >= 10);
}

#[test]
fn heater_approach_under_target_zero_output_uses_approach_floor() {
    let controller = HeaterController {
        phase: HeaterControlPhase::Approach,
        ..HeaterController::new()
    };
    let target = ThermalControlTarget {
        brake_distance_c: 4.5,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: 4.0,
        approach_power_permille: 900,
        approach_floor_power_permille: 740,
        approach_damping_exponent: 1.0,
        approach_tail_window_c: 0.0,
        hold_power_permille: 620,
        hold_reheat_power_permille: 680,
        hold_entry_error_c: 0.08,
        hold_exit_error_c: 0.5,
        hold_on_error_c: 0.0,
        hold_off_error_c: 3.0,
        overshoot_cutoff_c: 4.0,
        hold_kp_permille_per_c: 12.0,
        hold_ki_permille_per_c_tick: 1.0,
        hold_blend_ticks: 2,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        settings: ThermalControlProfileSettings::default(),
    };

    assert_eq!(
        controller.apply_under_target_reheat_floor(0, 0.2, 0.0, target),
        74
    );
}

#[test]
fn approach_tail_window_tapers_only_the_near_target_floor() {
    let target = ThermalControlTarget {
        brake_distance_c: 4.5,
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: 4.0,
        approach_power_permille: 900,
        approach_floor_power_permille: 500,
        approach_damping_exponent: 1.0,
        approach_tail_window_c: 2.0,
        hold_power_permille: 140,
        hold_reheat_power_permille: 180,
        hold_entry_error_c: 0.5,
        hold_exit_error_c: 0.8,
        hold_on_error_c: 0.3,
        hold_off_error_c: 1.0,
        overshoot_cutoff_c: 1.0,
        hold_kp_permille_per_c: 28.0,
        hold_ki_permille_per_c_tick: 1.0,
        hold_blend_ticks: 1,
        approach_lead_ticks: 4,
        hold_lead_ticks: 8,
        settings: ThermalControlProfileSettings::default(),
    };

    assert_eq!(approach_sustain_floor_permille(target, 3.0), 500);
    assert_eq!(approach_sustain_floor_permille(target, 1.5), 320);
    assert_eq!(approach_sustain_floor_permille(target, 0.5), 140);
    assert_eq!(
        approach_sustain_floor_permille(
            ThermalControlTarget {
                approach_tail_window_c: 0.0,
                ..target
            },
            0.5,
        ),
        500
    );
}

#[test]
fn heater_adjustable_voltage_maps_power_percent_to_requested_range() {
    assert_eq!(
        heater_request_mv_from_power_percent(0, HEATER_ADJUSTABLE_MIN_MV, HEATER_ADJUSTABLE_MAX_MV),
        12_000
    );
    assert_eq!(
        heater_request_mv_from_power_percent(
            50,
            HEATER_ADJUSTABLE_MIN_MV,
            HEATER_ADJUSTABLE_MAX_MV
        ),
        19_700
    );
    assert_eq!(
        heater_request_mv_from_power_percent(
            100,
            HEATER_ADJUSTABLE_MIN_MV,
            HEATER_ADJUSTABLE_MAX_MV
        ),
        28_000
    );
}

#[test]
fn heater_armed_zero_output_steps_toward_working_floor() {
    assert_eq!(
        heater_adjustable_request_mv(0, true, 11_300, 12_000, 6_100, 20_000),
        10_800
    );
    assert_eq!(
        heater_adjustable_request_mv(0, true, 6_300, 12_000, 6_100, 20_000),
        6_100
    );
    assert_eq!(
        heater_adjustable_request_mv(0, false, 11_300, 12_000, 6_100, 20_000),
        12_000
    );
    assert_eq!(
        heater_adjustable_request_mv(34, true, 10_000, 12_000, 6_100, 20_000),
        10_500
    );
}

#[test]
fn tuning_cooldown_stages_full_pps_voltage_without_heater_output() {
    assert_eq!(
        automatic_pps_request_mv(0, false, true, 12_000, 12_000, 6_100, 21_000),
        21_000
    );
    assert_eq!(
        automatic_pps_request_mv(0, false, true, 21_000, 12_000, 6_100, 21_000),
        21_000
    );
    assert_eq!(
        automatic_pps_request_mv(0, false, false, 21_000, 12_000, 6_100, 21_000),
        12_000
    );
    assert_eq!(heater_physical_pwm_percent(0, 21_000, 12_000, 100), 0);
}

#[test]
fn heater_pwm_frequency_is_100hz_for_all_heater_backends() {
    assert_eq!(HEATER_PWM_FREQUENCY_HZ, 100);
}

#[test]
fn heater_pwm_timer_is_representable_at_100hz() {
    let timer_counts = u32::from(HEATER_PWM_PERIOD_TICKS) + 1;
    let required_prescaler =
        MCPWM_PERIPHERAL_CLOCK_HZ / (HEATER_PWM_FREQUENCY_HZ * timer_counts) - 1;

    assert!(required_prescaler <= MCPWM_TIMER_MAX_PRESCALER);
    assert_eq!(
        MCPWM_PERIPHERAL_CLOCK_HZ / (required_prescaler + 1) / timer_counts,
        HEATER_PWM_FREQUENCY_HZ
    );
}

#[test]
fn heater_physical_pwm_uses_full_duty_when_pps_matches_requested_power() {
    assert_eq!(heater_physical_pwm_percent(100, 14_000, 14_000, 100), 100);
    assert_eq!(heater_physical_pwm_percent(25, 14_000, 7_000, 100), 100);
}

#[test]
fn heater_physical_pwm_reduces_power_at_pps_floor_and_during_down_ramp() {
    assert_eq!(heater_physical_pwm_percent(0, 14_000, 5_000, 100), 0);
    assert_eq!(heater_physical_pwm_percent(10, 14_000, 5_000, 100), 78);
    assert_eq!(heater_physical_pwm_percent(10, 14_000, 13_500, 100), 10);
    assert!(heater_physical_pwm_percent(10, 14_000, 13_500, 100) <= 10);
}

#[test]
fn heater_physical_pwm_reduces_power_below_6_5v_working_floor() {
    assert_eq!(
        heater_adjustable_request_mv(0, true, 12_000, 12_000, 6_100, 20_000),
        11_500
    );
    assert_eq!(
        heater_adjustable_request_mv(0, true, 6_100, 12_000, 6_100, 20_000),
        6_100
    );
    assert_eq!(heater_physical_pwm_percent(10, 14_000, 6_100, 100), 52);
    assert_eq!(heater_physical_pwm_percent(1, 14_000, 6_100, 100), 5);
    assert_eq!(heater_physical_pwm_percent(0, 14_000, 6_100, 100), 0);
}

#[test]
fn hold_pps_restores_observed_high_temperature_headroom_before_the_window_deadline() {
    // The 220 C HIL held 20.5 V at 81% logical output and about 1 C
    // undershoot. A ten-second initial dwell changed voltage just before the
    // first continuous ten-second witness could finish, invalidating it.
    let mut governor = HoldPpsGovernor::new();
    let input = HoldPpsRequestInput {
        phase: HeaterControlPhase::Hold,
        duty_percent: 81,
        actual_error_c: 1.0,
        filtered_slope_c_per_s: 0.0,
        current_request_mv: 20_500,
        control_floor_mv: 12_000,
        safe_max_mv: 21_000,
        now_ms: 0,
    };
    assert_eq!(
        heater_phase_pps_request_mv(&mut governor, input, false),
        Some(20_500)
    );
    assert_eq!(
        heater_phase_pps_request_mv(
            &mut governor,
            HoldPpsRequestInput {
                now_ms: 1_000,
                ..input
            },
            false
        ),
        Some(21_000)
    );
}

#[test]
fn hold_pps_governor_keeps_the_approach_voltage_when_headroom_is_sufficient() {
    let mut governor = HoldPpsGovernor::new();
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: 0,
        }),
        Some(18_000)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(18_000)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(18_000)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 0,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS + HEATER_HOLD_PPS_STEADY_DWELL_MS,
        }),
        Some(18_000)
    );

    let mut nominal = HoldPpsGovernor::new();
    assert_eq!(
        nominal.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: 0.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 14_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: 0,
        }),
        Some(14_000)
    );
    assert_eq!(
        nominal.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: 0.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 14_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(14_000)
    );
}

#[test]
fn profile_approach_downramps_pps_but_hold_keeps_the_handoff() {
    let mut controller = HeaterController::new();
    controller.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    controller.phase = HeaterControlPhase::Approach;
    assert!(controller.initial_profile_approach());
    controller.phase = HeaterControlPhase::Hold;
    assert!(!controller.initial_profile_approach());
    controller.phase = HeaterControlPhase::Approach;
    controller.production_controller = Some(ProductionThermalController::Unavailable);
    assert!(!controller.initial_profile_approach());
    let mut governor = HoldPpsGovernor::new();
    let input = HoldPpsRequestInput {
        phase: HeaterControlPhase::Approach,
        duty_percent: 10,
        actual_error_c: 16.18,
        filtered_slope_c_per_s: 2.4,
        current_request_mv: 21_000,
        control_floor_mv: 6_100,
        safe_max_mv: 21_000,
        now_ms: 4_000,
    };
    let automatic = automatic_pps_request_mv(10, true, false, 21_000, 12_000, 6_100, 21_000);
    assert_eq!(automatic, 20_500);
    assert_eq!(
        heater_phase_pps_request_mv(&mut governor, input, true).unwrap_or(automatic),
        20_500,
    );
    let hold = HoldPpsRequestInput {
        phase: HeaterControlPhase::Hold,
        current_request_mv: 10_000,
        now_ms: 8_000,
        ..input
    };
    assert_eq!(
        heater_phase_pps_request_mv(&mut governor, hold, false),
        Some(10_000)
    );
    assert_eq!(
        heater_phase_pps_request_mv(
            &mut governor,
            HoldPpsRequestInput {
                phase: HeaterControlPhase::Approach,
                ..hold
            },
            false,
        ),
        Some(10_000),
    );
    // Re-entering Warmup/first Approach starts a new down-ramp, even after Hold.
    assert_eq!(
        heater_phase_pps_request_mv(&mut governor, input, true),
        None
    );
    assert_eq!(governor, HoldPpsGovernor::new());
    // The established 3A plant path does not receive the profile-only guard.
    assert_eq!(
        heater_phase_pps_request_mv(&mut governor, input, false),
        Some(21_000)
    );
}

#[test]
fn tuning_continuous_pps_downramp_does_not_restart_the_trial_or_hide_failure() {
    let (power, _) = production_power_fixture(5_000);
    let request = PdContractRequest::pps(19_500, 5_000).unwrap();
    for protocol in [
        PowerProtocol::Ready,
        PowerProtocol::WaitingForAccept,
        PowerProtocol::WaitingForPsRdy,
    ] {
        let state = PowerState {
            protocol,
            requested: Some(request),
            ..power
        };
        let pending = tuning_pd_wait_decision(state, None, 200, Some(100), false);
        assert_eq!(pending, TuningPdWaitDecision::Wait);
        assert_eq!(
            tuning_pps_transition_decision(pending, state, Some(request), false, false),
            TuningPdWaitDecision::Continue,
        );
        assert_eq!(
            tuning_pps_transition_decision(pending, state, Some(request), true, false),
            TuningPdWaitDecision::Wait,
        );
        assert_eq!(
            tuning_pps_transition_decision(pending, state, Some(request), false, true),
            TuningPdWaitDecision::Wait,
        );
        let timeout = tuning_pd_wait_decision(
            state,
            None,
            AUTOMATIC_PPS_TICKET_WAIT_MS + 101,
            Some(100),
            false,
        );
        assert_eq!(
            tuning_pps_transition_decision(timeout, state, Some(request), false, false),
            TuningPdWaitDecision::Fail("pd_request_timeout"),
        );
    }
    for state in [PowerState::unavailable(), production_power_fixture(3_250).0] {
        assert_eq!(
            tuning_pps_transition_decision(
                TuningPdWaitDecision::Wait,
                state,
                Some(request),
                false,
                false
            ),
            TuningPdWaitDecision::Wait,
        );
    }
    let failure = TuningPdWaitDecision::Fail("pd_request_failed");
    assert_eq!(
        tuning_pps_transition_decision(failure, power, Some(request), false, false),
        failure
    );
}

#[test]
fn hold_pps_governor_clamps_immediately_to_a_lower_safe_max() {
    let mut governor = HoldPpsGovernor::new();
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 19_000,
            control_floor_mv: 6_100,
            safe_max_mv: 6_100,
            now_ms: 0,
        }),
        Some(6_100)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 6_100,
            control_floor_mv: 6_100,
            safe_max_mv: 6_100,
            now_ms: HEATER_PPS_SMALL_TRANSITION_MS,
        }),
        Some(6_100)
    );
}

#[test]
fn hold_pps_governor_raises_only_for_flat_below_target_saturation() {
    let mut governor = HoldPpsGovernor::new();
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.1,
            current_request_mv: 12_000,
            control_floor_mv: 6_100,
            safe_max_mv: 14_000,
            now_ms: 0,
        }),
        Some(12_000)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.1,
            current_request_mv: 12_000,
            control_floor_mv: 6_100,
            safe_max_mv: 14_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(12_500)
    );

    let mut current_limited = HoldPpsGovernor::new();
    assert_eq!(
        current_limited.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Approach,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.1,
            current_request_mv: 18_500,
            control_floor_mv: 6_100,
            safe_max_mv: 18_500,
            now_ms: 0,
        }),
        Some(18_500)
    );
    assert_eq!(
        current_limited.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Approach,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.1,
            current_request_mv: 18_500,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(19_000)
    );

    let mut rising = HoldPpsGovernor::new();
    assert_eq!(
        rising.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.5,
            current_request_mv: 12_000,
            control_floor_mv: 6_100,
            safe_max_mv: 14_000,
            now_ms: 0,
        }),
        Some(12_000)
    );
    assert_eq!(
        rising.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 80,
            actual_error_c: 0.6,
            filtered_slope_c_per_s: 0.5,
            current_request_mv: 12_000,
            control_floor_mv: 6_100,
            safe_max_mv: 14_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(12_000)
    );
}

#[test]
fn hold_pps_governor_keeps_adaptation_through_near_target_approach() {
    let mut governor = HoldPpsGovernor::new();
    let _ = governor.request_mv(HoldPpsRequestInput {
        phase: HeaterControlPhase::Hold,
        duty_percent: 100,
        actual_error_c: 1.0,
        filtered_slope_c_per_s: 0.0,
        current_request_mv: 18_000,
        control_floor_mv: 6_100,
        safe_max_mv: 21_000,
        now_ms: 0,
    });
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Approach,
            duty_percent: 100,
            actual_error_c: 1.0,
            filtered_slope_c_per_s: 0.0,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
        }),
        Some(18_500)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Warmup,
            duty_percent: 100,
            actual_error_c: 1.0,
            filtered_slope_c_per_s: 0.0,
            current_request_mv: 18_000,
            control_floor_mv: 6_100,
            safe_max_mv: 21_000,
            now_ms: HEATER_HOLD_PPS_INITIAL_SETTLE_MS + 1_000,
        }),
        None
    );
}

#[test]
fn warmup_soft_start_scales_physical_pwm_without_changing_control_request() {
    assert_eq!(apply_warmup_soft_start(80, 0), 0);
    assert_eq!(apply_warmup_soft_start(80, 50), 40);
    assert_eq!(apply_warmup_soft_start(80, 100), 80);
}

#[test]
fn partition_table_binary_matches_eeprom_only_layout() {
    let partition_table = include_str!("../../../partitions.csv");
    assert!(!partition_table.contains("flux_cfg"));
    let expected = esp_idf_part::PartitionTable::try_from(partition_table.as_bytes().to_vec())
        .unwrap()
        .to_bin()
        .unwrap();
    assert_eq!(
        expected.as_slice(),
        include_bytes!("../../../partitions.bin")
    );
}

#[test]
fn thermal_control_profile_interpolates_between_target_points() {
    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 500,
                warmup_power_permille: 400,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 400,
                approach_floor_power_permille: 220,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 200,
                hold_reheat_power_permille: 240,
                hold_entry_centi_c: 20,
                hold_exit_centi_c: 100,
                hold_on_centi_c: 0,
                hold_off_centi_c: 40,
                overshoot_cutoff_centi_c: 60,
                hold_kp_permille_per_c: 70,
                hold_ki_permille_per_c_tick: 3,
                hold_blend_ticks: 18,
                approach_lead_ticks: 10,
                hold_lead_ticks: 12,
            }),
            Some(ThermalControlProfilePoint {
                target_temp_c: 200,
                brake_distance_centi_c: 900,
                warmup_power_permille: 300,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 300,
                approach_floor_power_permille: 260,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 260,
                hold_reheat_power_permille: 420,
                hold_entry_centi_c: 10,
                hold_exit_centi_c: 60,
                hold_on_centi_c: 0,
                hold_off_centi_c: 120,
                overshoot_cutoff_centi_c: 140,
                hold_kp_permille_per_c: 20,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 8,
                approach_lead_ticks: 4,
                hold_lead_ticks: 6,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let target = profile.control_target(150);
    assert_eq!(target.brake_distance_c, 7.0);
    assert_eq!(target.approach_power_permille, 350);
    assert_eq!(target.approach_floor_power_permille, 240);
    assert_eq!(target.hold_power_permille, 230);
    assert_eq!(target.hold_reheat_power_permille, 330);
    assert_eq!(target.hold_entry_error_c, 0.15);
    assert_eq!(target.hold_exit_error_c, 0.8);
    assert_eq!(target.hold_off_error_c, 0.8);
    assert_eq!(target.overshoot_cutoff_c, 1.0);
    assert_eq!(target.hold_kp_permille_per_c, 45.0);
    assert_eq!(target.hold_ki_permille_per_c_tick, 2.0);
    assert_eq!(target.hold_blend_ticks, 13);
    assert_eq!(target.approach_lead_ticks, 7);
    assert_eq!(target.hold_lead_ticks, 9);
}

#[test]
fn thermal_control_profile_preserves_large_brake_distance_interpolation() {
    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 180,
                brake_distance_centi_c: 1_200,
                warmup_power_permille: 300,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 300,
                approach_floor_power_permille: 240,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 240,
                hold_reheat_power_permille: 360,
                hold_entry_centi_c: 18,
                hold_exit_centi_c: 90,
                hold_on_centi_c: 0,
                hold_off_centi_c: 100,
                overshoot_cutoff_centi_c: 120,
                hold_kp_permille_per_c: 30,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 12,
                approach_lead_ticks: 6,
                hold_lead_ticks: 8,
            }),
            Some(ThermalControlProfilePoint {
                target_temp_c: 250,
                brake_distance_centi_c: 2_000,
                warmup_power_permille: 260,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 260,
                approach_floor_power_permille: 260,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 260,
                hold_reheat_power_permille: 520,
                hold_entry_centi_c: 10,
                hold_exit_centi_c: 55,
                hold_on_centi_c: 0,
                hold_off_centi_c: 150,
                overshoot_cutoff_centi_c: 160,
                hold_kp_permille_per_c: 14,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 6,
                approach_lead_ticks: 2,
                hold_lead_ticks: 4,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    let target = profile.control_target(215);
    assert_eq!(target.brake_distance_c, 16.0);
    assert_eq!(target.approach_power_permille, 280);
    assert_eq!(target.approach_floor_power_permille, 250);
    assert_eq!(target.hold_power_permille, 250);
    assert_eq!(target.hold_reheat_power_permille, 440);
    assert_eq!(target.hold_entry_error_c, 0.14);
    assert_eq!(target.hold_exit_error_c, 0.73);
    assert_eq!(target.hold_off_error_c, 1.25);
    assert_eq!(target.overshoot_cutoff_c, 1.4);
    assert_eq!(target.hold_kp_permille_per_c, 22.0);
    assert_eq!(target.hold_ki_permille_per_c_tick, 1.0);
    assert_eq!(target.hold_blend_ticks, 9);
    assert_eq!(target.approach_lead_ticks, 4);
    assert_eq!(target.hold_lead_ticks, 6);
}

#[test]
fn thermal_control_profile_falls_back_outside_profile_range() {
    let profile = ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points: [
            Some(ThermalControlProfilePoint {
                target_temp_c: 50,
                brake_distance_centi_c: 500,
                warmup_power_permille: 400,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 400,
                approach_floor_power_permille: 200,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 200,
                hold_reheat_power_permille: 220,
                hold_entry_centi_c: 25,
                hold_exit_centi_c: 120,
                hold_on_centi_c: 0,
                hold_off_centi_c: 40,
                overshoot_cutoff_centi_c: 50,
                hold_kp_permille_per_c: 80,
                hold_ki_permille_per_c_tick: 3,
                hold_blend_ticks: 16,
                approach_lead_ticks: 12,
                hold_lead_ticks: 14,
            }),
            Some(ThermalControlProfilePoint {
                target_temp_c: 250,
                brake_distance_centi_c: 2_000,
                warmup_power_permille: 260,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 260,
                approach_floor_power_permille: 260,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 260,
                hold_reheat_power_permille: 520,
                hold_entry_centi_c: 10,
                hold_exit_centi_c: 55,
                hold_on_centi_c: 0,
                hold_off_centi_c: 150,
                overshoot_cutoff_centi_c: 160,
                hold_kp_permille_per_c: 14,
                hold_ki_permille_per_c_tick: 1,
                hold_blend_ticks: 6,
                approach_lead_ticks: 2,
                hold_lead_ticks: 3,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    };

    assert_eq!(
        profile.control_target(300),
        default_thermal_control_target(300)
    );
}

#[test]
fn thermal_profile_auto_resolution_uses_advertised_20v_5a_capability() {
    let five_amp = ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(5_000),
        pps_apdos: [
            Some(ch224q::PpsApdo {
                min_mv: 5_000,
                max_mv: 21_000,
                max_ma: 5_000,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
        ],
        avs_min_mv: None,
        avs_max_mv: None,
        ..Default::default()
    }));
    assert_eq!(
        resolve_thermal_profile_bank(ThermalProfileMode::Auto, &five_amp),
        ThermalProfileBank::Pps5a
    );
    let three_amp = ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_250),
        pps_apdos: [
            Some(ch224q::PpsApdo {
                min_mv: 5_000,
                max_mv: 21_000,
                max_ma: 3_250,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
        ],
        avs_min_mv: None,
        avs_max_mv: None,
        ..Default::default()
    }));
    assert_eq!(
        resolve_thermal_profile_bank(ThermalProfileMode::Auto, &three_amp),
        ThermalProfileBank::Pps3a
    );
    let split_apdos =
        ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(5_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 5_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 20_000,
                    max_mv: 21_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }));
    assert_eq!(
        resolve_thermal_profile_bank(ThermalProfileMode::Auto, &split_apdos),
        ThermalProfileBank::Pps3a
    );
    assert_eq!(
        resolve_thermal_profile_bank(ThermalProfileMode::W100, &ManualPpsState::default()),
        ThermalProfileBank::Pps5a
    );
}

fn production_power_fixture(max_ma: u16) -> (PowerState, ManualPpsState) {
    let source = SourceCapabilities::from_pdos(&[pps_source_capability(5_000, 21_000, max_ma)]);
    let request = PdContractRequest::pps(20_000, max_ma).unwrap();
    let contract = source.select_exact_contract(request).unwrap();
    let active = ConfirmedActiveContract::from_private_contract(contract, source).unwrap();
    let power_state = PowerState {
        protocol: PowerProtocol::Ready,
        available: true,
        requested: None,
        active: Some(active),
        source_capabilities: source.view(),
        failure: None,
    };
    let manual_pps = ManualPpsState::from_capabilities(adjustable_capabilities_from_view(
        power_state.source_capabilities,
    ));
    (power_state, manual_pps)
}

#[test]
fn valid_fixed_idle_contract_still_requires_automatic_pps_bootstrap() {
    let (mut power, manual_pps) = production_power_fixture(5_000);
    let active = power.active.as_mut().unwrap();
    active.mode = PdContractRequestMode::Fixed;
    active.voltage_mv = 5_000;
    active.operating_current_ma = 1_000;
    assert!(power.available);
    assert_eq!(power.protocol, PowerProtocol::Ready);
    assert!(automatic_pps_working_contract_pending(
        power,
        ThermalProfileMode::W100,
        &manual_pps,
    ));
    assert_eq!(
        automatic_pps_request_mv(0, true, true, 12_000, 12_000, 5_500, 21_000),
        21_000
    );

    let (ready, _) = production_power_fixture(5_000);
    assert!(!automatic_pps_working_contract_pending(
        ready,
        ThermalProfileMode::W100,
        &manual_pps,
    ));
    let (lower_current, _) = production_power_fixture(3_000);
    assert!(automatic_pps_working_contract_pending(
        lower_current,
        ThermalProfileMode::W100,
        &manual_pps,
    ));
}

fn production_test_profile(approach_power_permille: u16) -> ThermalControlProfile {
    let mut points = [None; FRONTPANEL_PRESET_COUNT];
    points[0] = Some(ThermalControlProfilePoint {
        target_temp_c: 60,
        brake_distance_centi_c: 1_400,
        warmup_power_permille: 1_000,
        warmup_reenter_centi_c: 400,
        approach_power_permille,
        approach_floor_power_permille: 100,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 100,
        hold_power_permille: 250,
        hold_reheat_power_permille: 300,
        hold_entry_centi_c: 150,
        hold_exit_centi_c: 250,
        hold_on_centi_c: 30,
        hold_off_centi_c: 50,
        overshoot_cutoff_centi_c: 300,
        hold_kp_permille_per_c: 100,
        hold_ki_permille_per_c_tick: 10,
        hold_blend_ticks: 4,
        approach_lead_ticks: 8,
        hold_lead_ticks: 6,
    });
    ThermalControlProfile {
        settings: ThermalControlProfileSettings::default(),
        points,
    }
}

fn production_test_input(measured_temp_c: f32) -> ThermalPlantRuntimeInput {
    ThermalPlantRuntimeInput {
        allow_integral: true,
        target_temp_c: 60,
        measured_temp_c,
        ambient_temp_c: 25.0,
        measurement_accepted: true,
        heater_enabled: true,
        model: flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 100.0,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 15_000.0,
            transport_delay_ms: 200,
        },
        max_power_mw: 65_000.0,
        now_ms: 3_000,
    }
}

fn calibrated_tail_test_setup() -> (
    flux_purr_firmware::thermal_response::HeatResponse,
    ThermalPlantRuntimeInput,
    ThermalControlProfile,
) {
    use flux_purr_firmware::thermal_response::{HeatResponse, reference_brake_c};
    let response = HeatResponse {
        delay_ms: 0,
        decay_ms: 3_900,
        rmse_c: 0.4,
        maximum_error_c: 0.8,
    };
    let mut input = production_test_input(35.0);
    input.model = flux_purr_firmware::memory::ThermalPlantProjection {
        convection_mw_per_c: 198.67226,
        radiation_mw_per_k4: 2.2978014e-7,
        thermal_capacity_mj_per_c: 25_843.268,
        transport_delay_ms: 850,
    };
    input.max_power_mw = 105_000.0;
    input.ambient_temp_c = 29.83;
    let brake = reference_brake_c(
        response,
        input.model,
        input.ambient_temp_c,
        input.max_power_mw,
        60.0,
    );
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 60;
    point.brake_distance_centi_c = (brake * 100.0).round() as u16;
    point.approach_lead_ticks = 0;
    point.hold_lead_ticks = 0;
    point.approach_damping_exponent_permille = 1_000;
    point.hold_power_permille = 66;
    point.hold_reheat_power_permille = 66;
    point.hold_kp_permille_per_c = 16;
    config.points[1..].fill(None);
    let profile = ThermalControlProfile::from_saved_config(&config).unwrap();
    (response, input, profile)
}

#[test]
fn calibrated_tail_drives_production_handoff_and_bounded_reheat() {
    let (response, mut input, profile) = calibrated_tail_test_setup();
    let mut controller = HeaterController::new();
    controller.heat_response = Some(response);
    let mut temperature = 35.0;
    let mut received_mw = 0.0;
    let mut handoff = None;
    let mut handoff_ms = None;
    let mut hold = None;
    let mut stable_since = None;
    let mut verified_since = None;
    let mut peak = temperature;
    for now_ms in (0..90_000).step_by(50) {
        input.now_ms = now_ms;
        input.measured_temp_c = temperature;
        let (output, _) = controller.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            input,
            Some(profile),
        );
        if output.phase == HeaterControlPhase::Approach {
            if handoff.is_none() {
                handoff = Some(temperature);
                handoff_ms = Some(now_ms);
                assert_eq!(output.duty_percent, 5);
            }
            assert!((5..=100).contains(&output.duty_percent), "{output:?}");
        }
        if output.phase == HeaterControlPhase::Hold {
            assert!(temperature >= 59.5, "{temperature}");
            hold.get_or_insert(now_ms);
        }
        if output.phase == HeaterControlPhase::Hold && (58.5..=61.5).contains(&temperature) {
            let start = *stable_since.get_or_insert(now_ms);
            if now_ms - start >= 10_000 {
                verified_since.get_or_insert(start);
            }
        } else {
            stable_since = None;
        }
        let applied_mw = input.max_power_mw * f32::from(output.duty_percent) / 100.0
            * f32::from(output.warmup_soft_start_percent)
            / 100.0;
        controller.applied_heat.observe(now_ms, applied_mw);
        let energy = response.step(&mut received_mw, applied_mw, 0.05);
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            input.model,
            temperature,
            input.ambient_temp_c,
        );
        temperature += (energy - loss * 0.05) / input.model.thermal_capacity_mj_per_c;
        peak = peak.max(temperature);
    }
    let handoff = handoff.unwrap();
    assert!((46.0..=51.0).contains(&handoff), "{handoff}");
    assert!(
        hold.is_some(),
        "handoff={handoff}, final={temperature}, peak={peak}"
    );
    assert!(peak <= 61.5, "{peak}");
    let settled_ms = verified_since.unwrap() - handoff_ms.unwrap();
    assert!(
        settled_ms <= 10_000,
        "settled={settled_ms} handoff={handoff} hold={hold:?} peak={peak}",
    );
    assert!((58.5..=61.5).contains(&temperature), "{temperature}");
}

#[test]
fn complete_heat_model_does_not_double_count_approach_lead_in_warmup() {
    let (response, mut input, profile) = calibrated_tail_test_setup();
    let mut advanced = profile;
    advanced.points[0].as_mut().unwrap().approach_lead_ticks = 7;
    let mut controllers = [HeaterController::new(), HeaterController::new()];
    for controller in &mut controllers {
        controller.heat_response = Some(response);
    }
    let mut received_mw = 0.0;
    for now_ms in (0..20_000).step_by(50) {
        input.now_ms = now_ms;
        let outputs: [_; 2] = core::array::from_fn(|index| {
            controllers[index]
                .update_production_at(
                    ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
                    input,
                    Some([profile, advanced][index]),
                )
                .0
        });
        assert_eq!(outputs[0].phase, outputs[1].phase);
        if outputs[0].phase == HeaterControlPhase::Approach {
            return;
        }
        let power = input.max_power_mw * f32::from(outputs[0].duty_percent) / 100.0
            * f32::from(outputs[0].warmup_soft_start_percent)
            / 100.0;
        for controller in &mut controllers {
            controller.applied_heat.observe(now_ms, power);
        }
        let energy = response.step(&mut received_mw, power, 0.05);
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            input.model,
            input.measured_temp_c,
            input.ambient_temp_c,
        );
        input.measured_temp_c += (energy - loss * 0.05) / input.model.thermal_capacity_mj_per_c;
    }
    panic!("expected a calibrated handoff");
}

#[test]
fn calibrated_high_temperature_hold_does_not_wait_for_a_low_side_exit() {
    for extra_loss_mw in [5_000.0, 10_000.0] {
        check_calibrated_high_temperature_hold(extra_loss_mw);
    }
}

#[test]
fn calibrated_hold_feedback_can_correct_the_model_endpoint_request() {
    let (response, mut input, profile) = calibrated_tail_test_setup();
    input.target_temp_c = 220;
    input.measured_temp_c = 219.0;
    input.max_power_mw = 78_000.0;
    input.now_ms = 20_000;
    let kind = ProductionThermalController::Profile(ThermalProfileBank::Pps5a);
    let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
        input.model,
        219.0,
        input.ambient_temp_c,
    );
    let outputs = [1, 21].map(|kp| {
        let mut profile = profile;
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 220;
        point.hold_kp_permille_per_c = kp;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_power_permille = 641;
        point.hold_reheat_power_permille = 641;
        point.hold_blend_ticks = 1;
        point.hold_off_centi_c = 120;
        point.overshoot_cutoff_centi_c = 150;
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.heat_response = Some(response);
        controller.last_target_temp_c = 220;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 1_000;
        controller.filtered_temp_c = Some(219.0);
        controller.previous_filtered_temp_c = Some(219.0);
        controller.previous_measured_temp_c = Some(219.0);
        controller.last_accepted_measurement_at_ms = Some(19_950);
        controller.profile_last_control_at_ms = Some(19_950);
        controller.applied_heat.configure(850);
        controller.applied_heat.configure_response(Some(response));
        for now_ms in (0..=20_000).step_by(50) {
            controller.applied_heat.observe(now_ms, loss);
        }
        controller
            .update_production_at(kind, input, Some(profile))
            .0
    });
    assert_eq!(outputs[0].phase, HeaterControlPhase::Hold);
    assert_eq!(outputs[1].phase, HeaterControlPhase::Hold);
    assert!(
        outputs[1].duty_percent > outputs[0].duty_percent,
        "{outputs:?}"
    );
}

#[test]
fn calibrated_hold_recovers_a_one_degree_deficit_within_the_response_time() {
    let (response, mut input, mut profile) = calibrated_tail_test_setup();
    input.target_temp_c = 220;
    input.max_power_mw = 78_000.0;
    let point = profile.points[0].as_mut().unwrap();
    point.target_temp_c = 220;
    point.hold_power_permille = 641;
    point.hold_reheat_power_permille = 641;
    point.hold_kp_permille_per_c = 21;
    point.hold_ki_permille_per_c_tick = 1;
    point.hold_blend_ticks = 1;
    point.hold_off_centi_c = 120;
    point.overshoot_cutoff_centi_c = 150;
    let kind = ProductionThermalController::Profile(ThermalProfileBank::Pps5a);
    let mut controller = HeaterController::new();
    controller.production_controller = Some(kind);
    controller.heat_response = Some(response);
    controller.last_target_temp_c = 220;
    controller.heater_was_enabled = true;
    controller.phase = HeaterControlPhase::Hold;
    controller.phase_ticks = 1_000;
    controller.filtered_temp_c = Some(219.0);
    controller.previous_filtered_temp_c = Some(219.0);
    controller.previous_measured_temp_c = Some(219.0);
    controller.last_accepted_measurement_at_ms = Some(19_950);
    controller.profile_last_control_at_ms = Some(19_950);
    controller.applied_heat.configure(850);
    controller.applied_heat.configure_response(Some(response));
    let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
        input.model,
        219.0,
        input.ambient_temp_c,
    );
    for now_ms in (0..=20_000).step_by(50) {
        controller.applied_heat.observe(now_ms, loss);
    }
    let mut temperature = 219.0;
    let mut heat = loss;
    let mut recovered_at_ms = None;
    for now_ms in (20_000..80_000).step_by(50) {
        input.now_ms = now_ms;
        input.measured_temp_c = temperature;
        let output = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(output.phase, HeaterControlPhase::Hold);
        assert!(
            (218.5..=221.5).contains(&temperature),
            "{now_ms}: {temperature}"
        );
        if temperature >= 219.75 {
            recovered_at_ms.get_or_insert(now_ms - 20_000);
        }
        let power = input.max_power_mw * f32::from(output.duty_percent) / 100.0;
        controller.applied_heat.observe(now_ms, power);
        let energy = response.step(&mut heat, power, 0.05);
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            input.model,
            temperature,
            input.ambient_temp_c,
        );
        temperature += (energy - loss * 0.05) / input.model.thermal_capacity_mj_per_c;
    }
    assert!(
        recovered_at_ms.is_some_and(|at| at <= u64::from(response.decay_ms) * 3),
        "{recovered_at_ms:?}"
    );
}

fn check_calibrated_high_temperature_hold(fan_loss_mw: f32) {
    let (response, mut input, mut profile) = calibrated_tail_test_setup();
    input.target_temp_c = 220;
    input.max_power_mw = 78_000.0;
    let point = profile.points[0].as_mut().unwrap();
    point.target_temp_c = 220;
    point.brake_distance_centi_c = (flux_purr_firmware::thermal_response::reference_brake_c(
        response,
        input.model,
        input.ambient_temp_c,
        input.max_power_mw,
        220.0,
    ) * 100.0)
        .round() as u16;
    point.hold_power_permille = 641;
    point.hold_reheat_power_permille = 641;
    point.hold_kp_permille_per_c = 21;
    point.hold_off_centi_c = 120;
    point.overshoot_cutoff_centi_c = 150;
    let mut controller = HeaterController::new();
    controller.heat_response = Some(response);
    let mut temperature = 200.0;
    let mut received_mw = 0.0;
    let mut entered_hold = false;
    let mut minimum_hold_c = f32::MAX;
    let mut peak_hold_c = f32::MIN;
    for now_ms in (0..100_000).step_by(50) {
        input.now_ms = now_ms;
        input.measured_temp_c = temperature;
        let (output, _) = controller.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            input,
            Some(profile),
        );
        entered_hold |= output.phase == HeaterControlPhase::Hold;
        if entered_hold {
            minimum_hold_c = minimum_hold_c.min(temperature);
            peak_hold_c = peak_hold_c.max(temperature);
            assert_eq!(
                output.phase,
                HeaterControlPhase::Hold,
                "at {now_ms}: {temperature}"
            );
        }
        let applied_mw = input.max_power_mw * f32::from(output.duty_percent) / 100.0
            * f32::from(output.warmup_soft_start_percent)
            / 100.0;
        controller.applied_heat.observe(now_ms, applied_mw);
        let energy = response.step(&mut received_mw, applied_mw, 0.05);
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            input.model,
            temperature,
            input.ambient_temp_c,
        );
        // A bounded periodic loss represents the heating fan/environment.
        // The accepted-temperature observer must recover its missing heat;
        // a static PI baseline must not cap that recovery indefinitely.
        let extra_loss_mw = if now_ms % 10_000 < 5_000 {
            fan_loss_mw
        } else {
            0.0
        };
        temperature +=
            (energy - (loss + extra_loss_mw) * 0.05) / input.model.thermal_capacity_mj_per_c;
    }
    assert!(entered_hold);
    assert!(minimum_hold_c >= 218.5, "{minimum_hold_c}");
    assert!(peak_hold_c <= 221.5, "{peak_hold_c}");
}

#[test]
fn production_point_local_zero_leads_survive_profile_materialization() {
    let mut config: ThermalControlProfileConfig = ThermalControlProfileWire {
        settings: None,
        points: saved_thermal_profile_points(),
    }
    .into();
    config.settings.approach_lead_ticks = 7;
    config.settings.hold_lead_ticks = 6;
    let point = config.points[0].as_mut().unwrap();
    point.target_temp_c = 60;
    point.approach_lead_ticks = 0;
    point.hold_lead_ticks = 0;
    config.sanitize();
    let preview = ThermalControlProfile::from_saved_config(&config).unwrap();
    let effective = preview.control_target(60);
    assert_eq!(effective.approach_lead_ticks, 0);
    assert_eq!(effective.hold_lead_ticks, 0);
}

#[test]
fn production_hold_entry_cannot_be_widened_by_candidate_or_timeout() {
    for target in [60, 160] {
        for curve_entry in [20, 270, 420] {
            for phase_ticks in [0, u16::MAX] {
                let mut profile = production_test_profile(667);
                profile.settings.temp_filter_alpha = 1.0;
                let point = profile.points[0].as_mut().unwrap();
                point.target_temp_c = target;
                point.hold_entry_centi_c = curve_entry;
                point.approach_lead_ticks = 0;
                point.hold_lead_ticks = 0;
                let measured = f32::from(target) - 2.51;
                let mut controller = HeaterController::new();
                controller.last_target_temp_c = target;
                controller.heater_was_enabled = true;
                controller.phase = HeaterControlPhase::Approach;
                controller.phase_ticks = phase_ticks;
                controller.filtered_temp_c = Some(measured);
                controller.previous_filtered_temp_c = Some(measured);
                controller.previous_measured_temp_c = Some(measured);
                let under = controller.update_at(target, measured, true, Some(profile), 3_000);
                assert_eq!(under.phase, HeaterControlPhase::Approach);

                // Neither a narrow nor a wide curve entry changes this gate.
                let admitted_temp = f32::from(target) - 0.5;
                controller.filtered_temp_c = Some(admitted_temp);
                controller.previous_filtered_temp_c = Some(admitted_temp);
                controller.previous_measured_temp_c = Some(admitted_temp);
                let admitted =
                    controller.update_at(target, admitted_temp, true, Some(profile), 3_050);
                assert_eq!(admitted.phase, HeaterControlPhase::Hold);
            }
        }
    }
}

#[test]
fn hold_entry_curve_parameter_still_shapes_approach_output() {
    let mut outputs = [0; 2];
    for (index, curve_entry) in [100, 420].into_iter().enumerate() {
        let mut profile = production_test_profile(667);
        profile.settings.temp_filter_alpha = 1.0;
        let point = profile.points[0].as_mut().unwrap();
        point.hold_entry_centi_c = curve_entry;
        point.hold_power_permille = 100;
        point.approach_lead_ticks = 0;
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(54.0);
        controller.previous_filtered_temp_c = Some(54.0);
        controller.previous_measured_temp_c = Some(54.0);
        let output = controller.update_at(60, 54.0, true, Some(profile), 3_000);
        assert_eq!(output.phase, HeaterControlPhase::Approach);
        outputs[index] = output.duty_percent;
    }
    assert_ne!(outputs[0], outputs[1]);
}

#[test]
fn profile_temperature_filter_preserves_one_second_semantics_at_real_cadence() {
    let mut profile = production_test_profile(300);
    profile.settings.temp_filter_alpha = 0.75;
    for interval_ms in [50, 60, 100] {
        let mut controller = HeaterController::new();
        let mut now_ms = 0;
        let mut snapshot = controller.update_at(240, 25.0, true, Some(profile), 0);
        while now_ms < 1_000 {
            now_ms = (now_ms + interval_ms).min(1_000);
            snapshot = controller.update_at(240, 35.0, true, Some(profile), now_ms);
        }
        assert!(
            (snapshot.filtered_temp_c - 32.5).abs() < 0.2,
            "interval={interval_ms}ms, filtered={}",
            snapshot.filtered_temp_c,
        );
    }
}

#[test]
fn profile_slope_uses_elapsed_time_instead_of_assumed_fifty_ms() {
    let mut profile = production_test_profile(300);
    profile.settings.temp_filter_alpha = 1.0;
    for interval_ms in [50, 60, 100] {
        let mut controller = HeaterController::new();
        controller.update_at(240, 25.0, true, Some(profile), 0);
        let next_temp_c = 25.0 + interval_ms as f32 / 1_000.0;
        let snapshot = controller.update_at(240, next_temp_c, true, Some(profile), interval_ms);
        assert!(
            (snapshot.filtered_slope_c_per_s - 1.0).abs() < 0.001,
            "interval={interval_ms}ms, slope={}",
            snapshot.filtered_slope_c_per_s,
        );
    }
}

#[test]
fn profile_hold_integral_and_blend_time_follow_elapsed_seconds() {
    let profile = production_test_profile(300);
    for interval_ms in [50, 60, 100] {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.reseed_measurement(59.8);
        controller.update_at(60, 59.8, true, Some(profile), 0);
        let mut now_ms = 0;
        while now_ms < 1_000 {
            now_ms = (now_ms + interval_ms).min(1_000);
            controller.update_at(60, 59.8, true, Some(profile), now_ms);
        }
        assert!(
            (controller.hold_integral_c - 4.2).abs() < 0.02,
            "interval={interval_ms}ms, integral={}",
            controller.hold_integral_c,
        );
        assert_eq!(controller.phase_ticks, 21, "interval={interval_ms}ms");
    }
}

#[test]
fn hold_actuator_feedback_prevents_positive_integral_windup() {
    let profile = production_test_profile(300);
    let mut controller = HeaterController::new();
    controller.last_target_temp_c = 60;
    controller.heater_was_enabled = true;
    controller.phase = HeaterControlPhase::Hold;
    controller.reseed_measurement(59.8);
    let mut input = production_test_input(59.8);
    input.max_power_mw = 1_000.0;
    input.allow_integral = false;
    for now_ms in (0..=1_000).step_by(50) {
        input.now_ms = now_ms;
        controller.natural_cooling_input = Some(input);
        controller.update_at(60, 59.8, true, Some(profile), now_ms);
    }
    assert_eq!(controller.phase, HeaterControlPhase::Hold);
    assert_eq!(controller.hold_integral_c, 0.0);
    input.allow_integral = true;
    input.now_ms = 1_050;
    controller.natural_cooling_input = Some(input);
    controller.update_at(60, 59.8, true, Some(profile), 1_050);
    assert!(controller.hold_integral_c > 0.0);
}

#[test]
fn profile_disarm_does_not_integrate_disabled_time_on_rearm() {
    let mut controller = HeaterController::new();
    let profile = production_test_profile(300);
    controller.update_at(240, 25.0, true, Some(profile), 0);
    controller.update_at(240, 35.0, true, Some(profile), 100);
    controller.update_at(240, 35.0, false, Some(profile), 150);
    let snapshot = controller.update_at(240, 36.0, true, Some(profile), 20_000);
    let nominal_alpha = scaled_filter_alpha_for_control_interval(0.75);
    assert!((snapshot.filtered_temp_c - (35.0 + nominal_alpha)).abs() < 0.01);
    assert_eq!(snapshot.warmup_soft_start_percent, 0);
    assert_eq!(controller.phase_ticks, 1);
}

#[test]
fn production_controller_requires_the_selected_bank_current() {
    let (five_amp, five_amp_pps) = production_power_fixture(5_000);
    assert_eq!(
        production_thermal_controller(five_amp, ThermalProfileMode::Auto, &five_amp_pps),
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a)
    );
    let (three_amp, three_amp_pps) = production_power_fixture(3_250);
    assert_eq!(
        production_thermal_controller(three_amp, ThermalProfileMode::W65, &three_amp_pps),
        ProductionThermalController::Profile(ThermalProfileBank::Pps3a)
    );
    assert_eq!(
        production_thermal_controller(three_amp, ThermalProfileMode::W100, &three_amp_pps),
        ProductionThermalController::Unavailable
    );
    assert_eq!(
        production_thermal_controller(
            PowerState::unavailable(),
            ThermalProfileMode::W100,
            &five_amp_pps
        ),
        ProductionThermalController::Unavailable
    );
    let negotiating = PowerState {
        protocol: PowerProtocol::WaitingForAccept,
        ..five_amp
    };
    assert_eq!(
        production_thermal_controller(negotiating, ThermalProfileMode::Auto, &five_amp_pps),
        ProductionThermalController::Unavailable
    );
    let continuous = PowerState {
        requested: Some(PdContractRequest::pps(20_100, 5_000).unwrap()),
        ..negotiating
    };
    assert_eq!(
        production_thermal_controller(continuous, ThermalProfileMode::Auto, &five_amp_pps),
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a)
    );
    assert_eq!(
        production_thermal_controller(
            PowerState {
                requested: Some(PdContractRequest::pps(20_100, 3_000).unwrap()),
                ..continuous
            },
            ThermalProfileMode::Auto,
            &five_amp_pps
        ),
        ProductionThermalController::Unavailable
    );
}

#[test]
fn five_amp_model_approach_responds_to_damping_without_a_power_plateau() {
    let mut outputs = [0; 2];
    for (index, damping) in [1_000, 4_000].into_iter().enumerate() {
        let mut profile = production_test_profile(900);
        profile.settings.temp_filter_alpha = 1.0;
        let point = profile.points[0].as_mut().unwrap();
        point.approach_damping_exponent_permille = damping;
        point.approach_lead_ticks = 0;
        let mut controller = HeaterController::new();
        controller.production_controller = Some(ProductionThermalController::Profile(
            ThermalProfileBank::Pps5a,
        ));
        controller.phase = HeaterControlPhase::Approach;
        controller.heater_was_enabled = true;
        controller.last_target_temp_c = 60;
        controller.filtered_temp_c = Some(52.0);
        controller.previous_filtered_temp_c = Some(52.0);
        controller.previous_measured_temp_c = Some(52.0);
        let mut input = production_test_input(52.0);
        input.model.transport_delay_ms = 1_950;
        input.max_power_mw = 100_000.0;
        controller.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            input,
            Some(profile),
        );
        input.now_ms += 1_000;
        outputs[index] = controller
            .update_production_at(
                ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
                input,
                Some(profile),
            )
            .0
            .duty_percent;
    }
    assert!(
        outputs[0] > outputs[1],
        "damping must shape live Approach: {outputs:?}"
    );
    assert!(outputs[1] >= 5);
}

#[test]
fn five_amp_braking_approach_entry_immediately_requests_five_percent() {
    for target in [60, 160, 240] {
        for interval_ms in [20, 50, 100] {
            let temperature = f32::from(target) - 8.0;
            let mut profile = production_test_profile(900);
            profile.points[0].as_mut().unwrap().target_temp_c = target;
            let mut controller = HeaterController::new();
            controller.production_controller = Some(ProductionThermalController::Profile(
                ThermalProfileBank::Pps5a,
            ));
            controller.last_target_temp_c = target;
            controller.heater_was_enabled = true;
            controller.filtered_temp_c = Some(temperature);
            controller.previous_measured_temp_c = Some(temperature);
            controller.profile_last_control_at_ms = Some(3_000 - interval_ms);
            let mut input = production_test_input(temperature);
            input.target_temp_c = target;
            let (output, disarmed) = controller.update_production_at(
                ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
                input,
                Some(profile),
            );
            assert!(!disarmed);
            assert_eq!(output.phase, HeaterControlPhase::Approach);
            assert_eq!(output.duty_percent, 5, "target={target}, dt={interval_ms}");
        }
    }
}

#[test]
fn five_amp_braking_approach_can_request_less_than_passive_loss_and_reheat() {
    let mut outputs = [0; 2];
    for (index, slope) in [2.0, -0.3].into_iter().enumerate() {
        let mut controller = HeaterController::new();
        controller.production_controller = Some(ProductionThermalController::Profile(
            ThermalProfileBank::Pps5a,
        ));
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(58.0);
        controller.previous_measured_temp_c = Some(58.0);
        controller.filtered_slope_c_per_profile_tick = slope;
        let mut profile = production_test_profile(900);
        profile.points[0].as_mut().unwrap().approach_lead_ticks = 0;
        let mut input = production_test_input(58.0);
        input.model.convection_mw_per_c = 300.0;
        input.model.transport_delay_ms = 2_000;
        let (output, _) = controller.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            input,
            Some(profile),
        );
        assert_eq!(output.phase, HeaterControlPhase::Approach);
        outputs[index] = output.duty_percent;
    }
    assert_eq!(outputs[0], 5, "rising plate must brake below passive loss");
    assert!(
        outputs[1] > outputs[0],
        "falling plate must regain heat: {outputs:?}"
    );
}

#[test]
fn trajectory_approach_restores_high_temperature_loss_before_transport_delay() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 220;
        controller.heater_was_enabled = true;
        controller.filtered_temp_c = Some(219.0);
        controller.previous_measured_temp_c = Some(219.0);
        controller.filtered_slope_c_per_profile_tick = 0.4;
        controller.profile_last_control_at_ms = Some(2_950);
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 220;
        point.brake_distance_centi_c = 100;
        point.approach_lead_ticks = 0;
        let mut input = production_test_input(219.02);
        input.target_temp_c = 220;
        input.ambient_temp_c = 30.0;
        input.max_power_mw = 60_000.0;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 21.32596,
            radiation_mw_per_k4: 9.014564e-7,
            thermal_capacity_mj_per_c: 31_067.016,
            transport_delay_ms: 700,
        };
        let entry = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(entry.phase, HeaterControlPhase::Approach);
        assert_eq!(entry.duty_percent, 5);
        input.now_ms += 100;
        input.measured_temp_c = 219.06;
        let tracked = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(tracked.phase, HeaterControlPhase::Approach);
        assert!(
            tracked.duty_percent > 5,
            "{bank:?}: replace impending heat loss before it appears"
        );
    }
}

#[test]
fn trajectory_approach_keeps_reference_when_temperature_deficit_increases() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let (mut controller, profile, mut input) = pps_cooling_test_fixture(bank);
        let trajectory = controller.approach_trajectory.unwrap();
        let warmup_started_at_ms = controller.warmup_started_at_ms;
        controller.reseed_measurement_after_pps_step(232.0);
        input.measured_temp_c = 232.0;
        input.now_ms += 100;
        let (output, disarmed) = controller.update_production_at(kind, input, Some(profile));
        assert!(!disarmed);
        assert_eq!(output.phase, HeaterControlPhase::Approach, "{bank:?}");
        assert!(
            controller
                .approach_trajectory
                .unwrap()
                .reference
                .temperature_c
                >= trajectory.reference.temperature_c,
            "{bank:?}: reference must continue from its pre-deficit origin"
        );
        assert_eq!(controller.warmup_started_at_ms, warmup_started_at_ms);
        assert_eq!(output.warmup_soft_start_percent, 100);
        assert!(output.duty_percent > 5, "{bank:?}: model must recover heat");
    }
}

#[test]
fn trajectory_hold_recovers_and_limits_arriving_heat_without_phase_regression() {
    // A 9c3882aa 220C trial 2: Hold 219.49C/57% -> Approach
    // 219.22C/11%, followed by 218.39C. Recovery has no new Warmup energy.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut profile = production_test_profile(1_000);
        profile.settings.temp_filter_alpha = 1.0;
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 220;
        point.approach_lead_ticks = 0;
        point.approach_damping_exponent_permille = 100;
        point.hold_entry_centi_c = 50;
        point.hold_exit_centi_c = 70;
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 220;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 40;
        controller.duty_percent = 57;
        controller.hold_entry_output_percent = 57;
        controller.filtered_temp_c = Some(219.29);
        controller.previous_filtered_temp_c = Some(219.29);
        controller.previous_measured_temp_c = Some(219.29);
        controller.filtered_slope_c_per_profile_tick = -0.5;
        controller.profile_last_control_at_ms = Some(2_900);
        let mut input = production_test_input(219.22);
        input.target_temp_c = 220;
        input.max_power_mw = 100_000.0;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 205.21394,
            radiation_mw_per_k4: 1.8367949e-7,
            thermal_capacity_mj_per_c: 25_915.566,
            transport_delay_ms: 1_950,
        };
        controller
            .applied_heat
            .configure(input.model.transport_delay_ms);
        controller.applied_heat.observe(1_000, 35_000.0);
        controller.applied_heat.observe(input.now_ms, 35_000.0);
        let mut high_arrival = controller;
        let (output, disarmed) = controller.update_production_at(kind, input, Some(profile));
        assert!(!disarmed);
        assert_eq!(output.phase, HeaterControlPhase::Hold);
        assert!(controller.approach_trajectory.is_none());
        assert!(
            output.duty_percent >= 40,
            "{bank:?}: falling plate must retain recovery authority, {output:?}"
        );
        assert!(!controller.initial_profile_approach());

        high_arrival.applied_heat =
            flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        high_arrival
            .applied_heat
            .configure(input.model.transport_delay_ms);
        high_arrival.applied_heat.observe(1_000, 90_000.0);
        high_arrival.applied_heat.observe(input.now_ms, 90_000.0);
        let reduced = high_arrival
            .update_production_at(kind, input, Some(profile))
            .0;
        assert!(
            reduced.duty_percent < 57,
            "{bank:?}: recovery origin must not become a hard power floor"
        );
    }
}

#[test]
fn legacy_approach_can_reenter_warmup_when_temperature_deficit_increases() {
    let (mut controller, profile, _) = pps_cooling_test_fixture(ThermalProfileBank::Pps5a);
    controller.reseed_measurement_after_pps_step(232.0);
    controller.natural_cooling_input = None;
    let output = controller.update_at(240, 232.0, true, Some(profile), 3_100);
    assert_eq!(output.phase, HeaterControlPhase::Warmup);
    assert!(controller.approach_trajectory.is_none());
    assert_eq!(controller.warmup_started_at_ms, Some(3_100));
}

#[test]
fn trajectory_reference_and_heat_history_survive_pps_origins_but_not_disarm() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let (mut controller, profile, input) = pps_cooling_test_fixture(bank);
        controller
            .applied_heat
            .configure(input.model.transport_delay_ms);
        controller.applied_heat.observe(2_000, 60_000.0);
        controller.applied_heat.observe(input.now_ms, 3_000.0);
        let trajectory = controller.approach_trajectory;
        let mut history = controller.applied_heat;
        assert!(trajectory.is_some());
        controller.reseed_measurement_after_pps_step(input.measured_temp_c - 0.3);
        history.reanchor_temperature();
        assert_eq!(controller.approach_trajectory, trajectory);
        assert_eq!(controller.applied_heat, history);
        controller.update_production_at(
            kind,
            ThermalPlantRuntimeInput {
                heater_enabled: false,
                ..input
            },
            Some(profile),
        );
        assert!(controller.approach_trajectory.is_none());
        assert_eq!(
            controller.applied_heat,
            flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new()
        );
    }
}

#[test]
fn held_approach_temperature_does_not_erase_heat_already_arriving() {
    // PPS settling keeps the last accepted temperature while old Warmup
    // energy continues arriving. A held reading cannot become a new origin.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        for pps_step in [false, true] {
            let kind = ProductionThermalController::Profile(bank);
            let mut controller = HeaterController::new();
            controller.production_controller = Some(kind);
            controller.last_target_temp_c = 60;
            controller.heater_was_enabled = true;
            controller.phase = HeaterControlPhase::Approach;
            controller.filtered_temp_c = Some(50.0);
            controller.previous_measured_temp_c = Some(50.0);
            controller.profile_last_control_at_ms = Some(4_950);
            let mut profile = production_test_profile(1_000);
            profile.settings.temp_filter_alpha = 1.0;
            profile.points[0].as_mut().unwrap().approach_lead_ticks = 0;
            let mut input = production_test_input(50.0);
            input.now_ms = 5_000;
            input.ambient_temp_c = 30.0;
            input.max_power_mw = 100_000.0;
            input.model.convection_mw_per_c = 200.0;
            input.model.thermal_capacity_mj_per_c = 25_000.0;
            input.model.transport_delay_ms = 2_100;
            controller.applied_heat.configure(2_100);
            controller.applied_heat.observe(2_000, 100_000.0);
            controller.applied_heat.observe(5_000, 5_000.0);
            controller.update_production_at(kind, input, Some(profile));
            let mut fresh = controller;
            let mut held = controller;
            if pps_step {
                held.reseed_measurement_after_pps_step(50.0);
            }
            assert_eq!(held.last_accepted_measurement_at_ms, Some(5_000));
            // The just-issued braking command has not reached the plate.
            // Integrate the known full-power input over this blind interval.
            let mut actual_c = 50.0;
            for _ in 0..900 {
                actual_c += (100_000.0 - 200.0 * (actual_c - 30.0)) / 25_000.0 * 0.001;
            }
            input.now_ms = 5_900;
            let fresh_output = fresh
                .update_production_at(
                    kind,
                    ThermalPlantRuntimeInput {
                        measured_temp_c: actual_c,
                        ..input
                    },
                    Some(profile),
                )
                .0;
            let held_output = held
                .update_production_at(
                    kind,
                    ThermalPlantRuntimeInput {
                        measurement_accepted: false,
                        ..input
                    },
                    Some(profile),
                )
                .0;
            assert_eq!(held_output.phase, HeaterControlPhase::Approach);
            assert_eq!(fresh_output.phase, HeaterControlPhase::Approach);
            assert_eq!(held.last_accepted_measurement_at_ms, Some(5_000));
            assert_eq!(fresh.last_accepted_measurement_at_ms, Some(5_900));
            assert!(
                held_output.duty_percent <= fresh_output.duty_percent + 1,
                "{bank:?}, pps_step={pps_step}: held={held_output:?}, fresh={fresh_output:?}"
            );
            held.update_production_at(
                kind,
                ThermalPlantRuntimeInput {
                    heater_enabled: false,
                    ..input
                },
                Some(profile),
            );
            assert_eq!(held.last_accepted_measurement_at_ms, None);
        }
    }
}

#[test]
fn rejected_approach_measurements_cannot_raise_the_last_braking_request() {
    // A 60C HIL held 54.18C for 16s while the raw channel was already above
    // target. Advancing the reference without a new observation must not
    // manufacture a reason to raise heating during the blind interval.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Warmup;
        controller.filtered_temp_c = Some(54.18);
        controller.previous_measured_temp_c = Some(54.18);
        controller.profile_last_control_at_ms = Some(4_950);
        let mut profile = production_test_profile(1_000);
        profile.settings.temp_filter_alpha = 1.0;
        profile.points[0].as_mut().unwrap().approach_lead_ticks = 0;
        let mut input = production_test_input(54.18);
        input.now_ms = 5_000;
        input.ambient_temp_c = 28.61;
        input.max_power_mw = 100_000.0;
        input.model.convection_mw_per_c = 168.47858;
        input.model.radiation_mw_per_k4 = 3.3575284e-7;
        input.model.thermal_capacity_mj_per_c = 25_746.225;
        input.model.transport_delay_ms = 1_800;
        controller.applied_heat.configure(1_800);
        controller.applied_heat.observe(2_000, 100_000.0);
        controller.applied_heat.observe(input.now_ms, 5_000.0);
        let first = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(first.duty_percent, 5);
        for step in 1..=160 {
            input.now_ms = 5_000 + step * 100;
            input.measurement_accepted = false;
            if step % 5 == 0 {
                controller.reseed_measurement_after_pps_step(input.measured_temp_c);
            }
            let output = controller
                .update_production_at(kind, input, Some(profile))
                .0;
            assert_eq!(output.phase, HeaterControlPhase::Approach);
            assert_eq!(
                output.duty_percent, 5,
                "{bank:?}: a rejected reading must not increase braking power at {step}: {output:?}"
            );
            controller.applied_heat.observe(input.now_ms, 5_000.0);
        }
        input.now_ms += 100;
        input.measurement_accepted = true;
        input.measured_temp_c = 55.0;
        controller.update_production_at(kind, input, Some(profile));
        for step in 1..=20 {
            input.now_ms += 100;
            let output = controller
                .update_production_at(kind, input, Some(profile))
                .0;
            if step == 20 {
                assert!(
                    output.duty_percent > 5,
                    "fresh low readings must restore recovery authority"
                );
            }
            controller
                .applied_heat
                .observe(input.now_ms, f32::from(output.duty_percent) * 1_000.0);
        }
    }
}

#[test]
fn accepted_approach_deficit_can_apply_the_model_full_power_request() {
    // At the B high-temperature endpoint the delayed-energy prediction and
    // trajectory already account for transport delay. A second actuator ramp
    // must not discard a fresh, model-bounded recovery request.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 220;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(218.0);
        controller.previous_measured_temp_c = Some(218.0);
        controller.last_accepted_measurement_at_ms = Some(5_000);
        controller.profile_last_control_at_ms = Some(5_000);
        controller.duty_percent = 5;
        controller.approach_last_request_permille = Some(50);
        controller.approach_trajectory =
            Some(flux_purr_firmware::thermal_trajectory::TemperatureTrajectory::new(5_000, 219.0));
        let mut profile = production_test_profile(1_000);
        profile.points[0].as_mut().unwrap().target_temp_c = 220;
        profile.points[0].as_mut().unwrap().approach_lead_ticks = 0;
        let mut input = production_test_input(218.0);
        input.target_temp_c = 220;
        input.ambient_temp_c = 29.94;
        input.max_power_mw = 63_000.0;
        input.now_ms = 5_100;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 81.21304,
            radiation_mw_per_k4: 6.7999764e-7,
            thermal_capacity_mj_per_c: 30_629.809,
            transport_delay_ms: 1_400,
        };
        controller.applied_heat.configure(1_400);
        controller.applied_heat.observe(2_000, 3_150.0);
        controller.applied_heat.observe(5_000, 3_150.0);
        let mut rejected = controller;
        let held = rejected
            .update_production_at(
                kind,
                ThermalPlantRuntimeInput {
                    measurement_accepted: false,
                    ..input
                },
                Some(profile),
            )
            .0;
        assert_eq!(held.phase, HeaterControlPhase::Approach);
        assert_eq!(held.duty_percent, 5);
        let output = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(output.phase, HeaterControlPhase::Approach);
        assert_eq!(
            output.duty_percent, 100,
            "{bank:?}: fresh low temperature and bounded delayed heat must retain full recovery authority: {output:?}"
        );
    }
}

#[test]
fn fresh_approach_recovery_matches_across_rejected_callbacks() {
    // The same physical hold followed by the same fresh reading must not
    // acquire a slower recovery merely because PPS wait callbacks ran.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut sparse = HeaterController::new();
        sparse.production_controller = Some(kind);
        sparse.last_target_temp_c = 60;
        sparse.heater_was_enabled = true;
        sparse.phase = HeaterControlPhase::Approach;
        sparse.filtered_temp_c = Some(54.0);
        sparse.previous_measured_temp_c = Some(54.0);
        sparse.last_accepted_measurement_at_ms = Some(5_000);
        sparse.profile_last_control_at_ms = Some(5_000);
        sparse.duty_percent = 5;
        sparse.approach_last_request_permille = Some(50);
        sparse.approach_trajectory =
            Some(flux_purr_firmware::thermal_trajectory::TemperatureTrajectory::new(5_000, 54.0));
        let mut profile = production_test_profile(1_000);
        profile.settings.temp_filter_alpha = 1.0;
        profile.points[0].as_mut().unwrap().approach_lead_ticks = 0;
        let mut input = production_test_input(54.0);
        input.model.transport_delay_ms = 1_400;
        sparse.applied_heat.configure(1_400);
        sparse.applied_heat.observe(2_000, 5_000.0);
        sparse.applied_heat.observe(5_000, 5_000.0);
        let mut callbacks = sparse;
        for step in 1..=9 {
            input.now_ms = 5_000 + step * 100;
            input.measurement_accepted = false;
            let held = callbacks.update_production_at(kind, input, Some(profile)).0;
            assert_eq!(held.phase, HeaterControlPhase::Approach);
            assert_eq!(held.duty_percent, 5);
            callbacks.applied_heat.observe(input.now_ms, 5_000.0);
        }
        input.now_ms = 6_000;
        input.measurement_accepted = true;
        input.measured_temp_c = 53.9;
        let sparse_output = sparse.update_production_at(kind, input, Some(profile)).0;
        let callbacks_output = callbacks.update_production_at(kind, input, Some(profile)).0;
        assert!(sparse_output.duty_percent >= 25);
        assert!(
            callbacks_output
                .duty_percent
                .abs_diff(sparse_output.duty_percent)
                <= 1,
            "{bank:?}: callback cadence cannot change the same fresh recovery: sparse={sparse_output:?}, callbacks={callbacks_output:?}"
        );
    }
}

#[test]
fn rejected_hold_measurements_do_not_resume_heat_or_wind_up_the_integral() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.filtered_temp_c = Some(59.6);
        controller.previous_measured_temp_c = Some(59.6);
        controller.profile_last_control_at_ms = Some(4_950);
        let mut profile = production_test_profile(1_000);
        profile.settings.temp_filter_alpha = 1.0;
        let mut input = production_test_input(59.6);
        input.now_ms = 5_000;
        input.model.transport_delay_ms = 1_800;
        input.max_power_mw = 100_000.0;
        controller.applied_heat.configure(1_800);
        controller.applied_heat.observe(2_000, 100_000.0);
        controller.applied_heat.observe(input.now_ms, 0.0);
        let first = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(first.phase, HeaterControlPhase::Hold);
        assert_eq!(first.duty_percent, 0);
        let integral = controller.hold_integral_c;
        for step in 1..=160 {
            input.now_ms = 5_000 + step * 100;
            input.measurement_accepted = false;
            let output = controller
                .update_production_at(kind, input, Some(profile))
                .0;
            assert_eq!(output.phase, HeaterControlPhase::Hold);
            assert_eq!(output.duty_percent, 0);
            assert_eq!(controller.hold_integral_c, integral);
            controller.applied_heat.observe(input.now_ms, 0.0);
        }
        input.now_ms += 100;
        input.measurement_accepted = true;
        let resumed = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert!(
            resumed.duty_percent > 0,
            "fresh low Hold readings must restore heat"
        );
    }
}

#[test]
fn trajectory_heat_observation_rejects_held_samples_and_resets_on_recovery() {
    let (mut controller, profile, input) = pps_cooling_test_fixture(ThermalProfileBank::Pps5a);
    let kind = ProductionThermalController::Profile(ThermalProfileBank::Pps5a);
    controller.update_production_at(kind, input, Some(profile));
    controller
        .applied_heat
        .configure(input.model.transport_delay_ms);
    controller.applied_heat.observe(input.now_ms, 3_000.0);
    let history = controller.applied_heat;
    controller.update_production_at(
        kind,
        ThermalPlantRuntimeInput {
            now_ms: input.now_ms + 100,
            measurement_accepted: false,
            ..input
        },
        Some(profile),
    );
    assert_eq!(controller.applied_heat, history);
    let mut recovered = history;
    recovered.reset_temperature_observation();
    controller.reseed_measurement(input.measured_temp_c);
    assert_eq!(controller.applied_heat, recovered);
}

#[test]
fn trajectory_controller_reaches_physical_hold_with_delayed_calibrated_plants() {
    for (bank, model, max_power_mw) in [
        (
            ThermalProfileBank::Pps5a,
            flux_purr_firmware::memory::ThermalPlantProjection {
                convection_mw_per_c: 195.67747,
                radiation_mw_per_k4: 2.1190255e-7,
                thermal_capacity_mj_per_c: 25_815.854,
                transport_delay_ms: 1_900,
            },
            100_000.0,
        ),
        (
            ThermalProfileBank::Pps3a,
            flux_purr_firmware::memory::ThermalPlantProjection {
                convection_mw_per_c: 21.32596,
                radiation_mw_per_k4: 9.014564e-7,
                thermal_capacity_mj_per_c: 31_067.016,
                transport_delay_ms: 700,
            },
            60_000.0,
        ),
        (
            ThermalProfileBank::Pps3a,
            flux_purr_firmware::memory::ThermalPlantProjection {
                convection_mw_per_c: 81.21304,
                radiation_mw_per_k4: 6.7999764e-7,
                thermal_capacity_mj_per_c: 30_629.809,
                transport_delay_ms: 1_400,
            },
            63_000.0,
        ),
    ] {
        for target in [60, 220] {
            assert_trajectory_plant_handoff(bank, model, max_power_mw, target);
        }
    }
}

#[test]
fn calibrated_hold_uses_arriving_energy_instead_of_a_stale_positive_slope() {
    // Repaired A trial 15 repeatedly cuts Hold at 220.00..220.36C, then
    // leaves the stable band at 218.31C. A positive filtered derivative is
    // not evidence that the current command will still arrive above target.
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 220;
        point.hold_power_permille = 700;
        point.hold_reheat_power_permille = 1_000;
        point.hold_kp_permille_per_c = 62;
        point.hold_ki_permille_per_c_tick = 1;
        point.hold_on_centi_c = 5;
        point.hold_off_centi_c = 240;
        point.hold_blend_ticks = 1;
        point.hold_lead_ticks = 0;
        let mut input = production_test_input(220.12);
        input.target_temp_c = 220;
        input.ambient_temp_c = 30.0;
        input.max_power_mw = 100_000.0;
        input.now_ms = 5_000;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 201.96992,
            radiation_mw_per_k4: 2.0132205e-7,
            thermal_capacity_mj_per_c: 26_466.248,
            transport_delay_ms: 2_000,
        };
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 220;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 100;
        controller.hold_coast_active = true;
        controller.profile_last_control_at_ms = Some(4_950);
        controller.filtered_temp_c = Some(220.02);
        controller.previous_measured_temp_c = Some(220.09);
        controller.filtered_slope_c_per_profile_tick = 0.2;
        controller.applied_heat.configure(2_000);
        controller.applied_heat.observe(2_000, 0.0);
        controller.applied_heat.observe(input.now_ms, 0.0);
        let mut high_arrival = controller;
        let recovering = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(recovering.phase, HeaterControlPhase::Hold);
        assert!(recovering.duty_percent > 0, "{bank:?}: {recovering:?}");

        // Identical plate temperature and derivative with actual heat in
        // flight must still cut power. The forecast, not the raw slope,
        // distinguishes the two states through the production call site.
        high_arrival.applied_heat =
            flux_purr_firmware::thermal_trajectory::AppliedHeatHistory::new();
        high_arrival.applied_heat.configure(2_000);
        high_arrival.applied_heat.observe(2_000, 100_000.0);
        high_arrival.applied_heat.observe(input.now_ms, 100_000.0);
        let braking = high_arrival
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(braking.duty_percent, 0, "{bank:?}: {braking:?}");

        input.now_ms += 50;
        input.measured_temp_c = 224.0;
        let cutoff = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(cutoff.duty_percent, 0, "{bank:?}: actual cutoff survives");
    }
}

#[test]
fn calibrated_hold_does_not_reheat_into_delayed_high_side_energy() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        let mut input = production_test_input(61.03);
        input.ambient_temp_c = 30.0;
        input.now_ms = 4_000;
        input.max_power_mw = 100_000.0;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 205.21394,
            radiation_mw_per_k4: 1.8367949e-7,
            thermal_capacity_mj_per_c: 25_915.566,
            transport_delay_ms: 1_950,
        };
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.hold_power_permille = 60;
        point.hold_reheat_power_permille = 60;
        point.hold_kp_permille_per_c = 8;
        point.hold_ki_permille_per_c_tick = 2;
        point.hold_off_centi_c = 120;
        point.overshoot_cutoff_centi_c = 150;
        point.hold_lead_ticks = 2;
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 100;
        controller.profile_last_control_at_ms = Some(3_950);
        controller.filtered_temp_c = Some(input.measured_temp_c);
        controller.previous_measured_temp_c = Some(input.measured_temp_c);
        controller.filtered_slope_c_per_profile_tick = -0.01;
        controller
            .applied_heat
            .configure(input.model.transport_delay_ms);
        controller.applied_heat.observe(2_000, 10_000.0);
        controller.applied_heat.observe(input.now_ms, 0.0);

        let output = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(output.phase, HeaterControlPhase::Hold);
        assert_eq!(
            output.duty_percent, 0,
            "{bank:?}: short falling slope is not proof that heat in flight has dissipated"
        );

        // The final under-target fallback must not bypass the arrival guard.
        controller.applied_heat.observe(4_000, 90_000.0);
        controller.applied_heat.observe(4_400, 0.0);
        input.now_ms = 4_400;
        input.measured_temp_c = 59.8;
        controller.filtered_temp_c = Some(59.8);
        controller.previous_measured_temp_c = Some(59.8);
        controller.filtered_slope_c_per_profile_tick = 0.0;
        controller.profile_last_control_at_ms = Some(4_350);
        let below_target = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(
            below_target.duty_percent, 0,
            "{bank:?}: fallback reheating bypasses the delayed-energy limit"
        );

        // Genuine cooling must still restore heat before undershoot. This is
        // the same production call site, with known zero-power history.
        input.now_ms = 8_000;
        input.measured_temp_c = 59.8;
        controller.filtered_temp_c = Some(59.8);
        controller.previous_measured_temp_c = Some(59.82);
        controller.filtered_slope_c_per_profile_tick = -0.15;
        controller.profile_last_control_at_ms = Some(7_950);
        controller.applied_heat.observe(input.now_ms, 0.0);
        let cooling = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert!(
            cooling.duty_percent > 0,
            "{bank:?}: recovery must remain available"
        );
    }
}

fn assert_trajectory_plant_handoff(
    bank: ThermalProfileBank,
    model: flux_purr_firmware::memory::ThermalPlantProjection,
    max_power_mw: f32,
    target: i16,
) {
    let kind = ProductionThermalController::Profile(bank);
    let mut controller = HeaterController::new();
    let mut profile = production_test_profile(1_000);
    let point = profile.points[0].as_mut().unwrap();
    point.target_temp_c = target;
    let loss =
        flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(model, f32::from(target), 30.0);
    point.brake_distance_centi_c = (((max_power_mw - loss) / model.thermal_capacity_mj_per_c
        * model.transport_delay_ms as f32
        / 1_000.0
        + 0.5)
        * 100.0)
        .max(100.0) as u16;
    point.approach_lead_ticks = 0;
    let mut input = production_test_input(if target == 60 { 34.9 } else { 199.9 });
    input.target_temp_c = target;
    input.model = model;
    input.ambient_temp_c = 30.0;
    input.max_power_mw = max_power_mw;
    let mut delayed = std::collections::VecDeque::from([(0_u64, 0.0_f32)]);
    let mut applied = 0.0;
    let mut entered_approach = false;
    let mut entered_hold = false;
    let mut approach_at = 0;
    let mut handoff_ms = 0;
    let mut peak = input.measured_temp_c;
    for now_ms in (0_u64..=100_000).step_by(50) {
        let source_at = now_ms.saturating_sub(u64::from(model.transport_delay_ms));
        while delayed.len() > 1 && delayed[1].0 <= source_at {
            delayed.pop_front();
        }
        let loss = flux_purr_firmware::thermal_plant::natural_cooling_loss_mw(
            model,
            input.measured_temp_c,
            30.0,
        );
        input.measured_temp_c += (delayed[0].1 - loss) / model.thermal_capacity_mj_per_c * 0.05;
        input.now_ms = now_ms;
        let output = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        if output.phase == HeaterControlPhase::Approach {
            if !entered_approach {
                entered_approach = true;
                approach_at = now_ms;
                assert_eq!(output.duty_percent, 5);
            }
            assert!((5..=100).contains(&output.duty_percent));
        }
        if output.phase == HeaterControlPhase::Hold && !entered_hold {
            entered_hold = true;
            handoff_ms = now_ms - approach_at;
            assert!(input.measured_temp_c >= f32::from(target) - 0.5);
        }
        let physical_percent = if output.phase == HeaterControlPhase::Warmup {
            output.duty_percent.min(output.warmup_soft_start_percent)
        } else {
            output.duty_percent
        };
        let power = f32::from(physical_percent) * max_power_mw / 100.0;
        if power != applied {
            delayed.push_back((now_ms, power));
            applied = power;
        }
        controller.applied_heat.observe(now_ms, power);
        peak = peak.max(input.measured_temp_c);
    }
    assert!(
        entered_hold,
        "{bank:?}/{target}: no physical Hold, final={}",
        input.measured_temp_c
    );
    let handoff_limit_ms = if target <= 150 { 10_000 } else { 5_000 };
    assert!(
        handoff_ms <= handoff_limit_ms,
        "{bank:?}/{target}: late handoff {handoff_ms}ms (limit {handoff_limit_ms}ms)"
    );
    assert!(
        peak <= f32::from(target) + 3.0,
        "{bank:?}/{target}: peak={peak}"
    );
}

#[test]
fn calibrated_brake_feedback_moves_the_shared_high_temperature_handoff() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 240;
        controller.heater_was_enabled = true;
        controller.filtered_temp_c = Some(237.75);
        controller.previous_measured_temp_c = Some(237.75);
        controller.filtered_slope_c_per_profile_tick = 0.9;
        controller.profile_last_control_at_ms = Some(2_950);
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 240;
        point.brake_distance_centi_c = 150;
        point.hold_entry_centi_c = 90;
        point.approach_lead_ticks = 0;
        let mut input = production_test_input(237.8);
        input.target_temp_c = 240;
        input.ambient_temp_c = 33.9;
        input.max_power_mw = 73_909.95;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 119.58163,
            radiation_mw_per_k4: 6.174697e-7,
            thermal_capacity_mj_per_c: 22_564.195,
            transport_delay_ms: 3_400,
        };
        // The fresh calibrated distance is 2.30C. Same-run feedback reduced
        // it to 1.50C, but the uncorrected dynamic model masks that change.
        let mut baseline = controller;
        let mut initial_profile = profile;
        initial_profile.points[0]
            .as_mut()
            .unwrap()
            .brake_distance_centi_c = 230;
        assert_eq!(
            baseline
                .update_production_at(kind, input, Some(initial_profile))
                .0
                .phase,
            HeaterControlPhase::Approach,
            "the fresh calibrated point retains model-guided braking"
        );
        let first = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(first.phase, HeaterControlPhase::Warmup, "bank={bank:?}");
        assert_eq!(first.duty_percent, 100);
        input.now_ms += 500;
        input.measured_temp_c = 238.4;
        controller.update_production_at(kind, input, Some(profile));
        input.now_ms += 500;
        input.measured_temp_c = 238.7;
        let entry = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(entry.phase, HeaterControlPhase::Approach);
        assert_eq!(entry.duty_percent, 5);
    }
}

#[test]
fn pps_segments_keep_trajectory_recovery_available() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut initial = HeaterController::new();
        initial.production_controller = Some(kind);
        initial.last_target_temp_c = 240;
        initial.heater_was_enabled = true;
        initial.filtered_temp_c = Some(238.3);
        initial.previous_measured_temp_c = Some(238.3);
        initial.filtered_slope_c_per_profile_tick = 0.4;
        initial.profile_last_control_at_ms = Some(2_950);
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 240;
        point.brake_distance_centi_c = 197;
        point.approach_lead_ticks = 0;
        let mut input = production_test_input(238.3);
        input.target_temp_c = 240;
        input.model.transport_delay_ms = 2_650;
        assert_eq!(
            initial
                .update_production_at(kind, input, Some(profile))
                .0
                .duty_percent,
            5
        );
        let mut steady = initial;
        let mut stepped = initial;
        let mut transition = RtdPpsTransitionGuard::new(21_000);
        let mut measurement = RtdControlMeasurementGuard::default();
        measurement.reseed(238.3, 3_000);
        let mut control_temp = 238.3;
        let mut steady_reheat_at = None;
        let mut stepped_reheat_at = None;
        for offset in (50..=1_500).step_by(50) {
            let physical_temp = 238.3 - offset as f32 * 0.0006;
            input.now_ms = 3_000 + offset;
            input.measured_temp_c = physical_temp;
            input.measurement_accepted = true;
            let steady_output = steady.update_production_at(kind, input, Some(profile)).0;
            if steady_output.duty_percent > 5 {
                steady_reheat_at.get_or_insert(offset);
            }
            // Only PPS cadence differs: no ADC offset is injected. Both twins
            // receive the same physical cooling through the real guard chain.
            let request_mv = 20_500 - ((offset - 50) / 500) as u16 * 500;
            let accepted = accept_rtd_control_sample_after_pps_transition(
                &mut transition,
                &mut stepped,
                &mut measurement,
                control_temp,
                request_mv,
                input.now_ms,
                physical_temp,
            );
            if let Some(value) = accepted {
                control_temp = value;
            }
            input.measured_temp_c = control_temp;
            input.measurement_accepted = accepted.is_some();
            let stepped_output = stepped.update_production_at(kind, input, Some(profile)).0;
            if stepped_output.duty_percent > 5 {
                stepped_reheat_at.get_or_insert(offset);
            }
        }
        assert!(
            steady_reheat_at.is_some(),
            "steady request must recover on real cooling"
        );
        assert!(
            stepped_reheat_at.is_some(),
            "bank={bank:?}: tracking lost at PPS reset: steady={steady_reheat_at:?}, stepped={stepped_reheat_at:?}, request={}%",
            stepped.duty_percent
        );
    }
}

#[test]
fn pps_epoch_anchor_does_not_turn_voltage_offset_into_thermal_velocity() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        for offset_c in [-0.3, 0.3] {
            assert_pps_epoch_anchor(bank, offset_c);
        }
    }
}

fn assert_pps_epoch_anchor(bank: ThermalProfileBank, offset_c: f32) {
    let kind = ProductionThermalController::Profile(bank);
    let (mut controller, profile, mut input) = pps_cooling_test_fixture(bank);
    controller.filtered_slope_c_per_profile_tick = 0.4;
    let mut transition = RtdPpsTransitionGuard::new(21_000);
    let mut measurement = RtdControlMeasurementGuard::default();
    measurement.reseed(238.3, 3_000);
    let mut control_temp = 238.3;
    let mut accepted_count = 0;
    for elapsed in (50..=350).step_by(50) {
        input.now_ms = 3_000 + elapsed;
        let accepted = accept_rtd_control_sample_after_pps_transition(
            &mut transition,
            &mut controller,
            &mut measurement,
            control_temp,
            20_500,
            input.now_ms,
            238.3 + offset_c,
        );
        if let Some(temp) = accepted {
            control_temp = temp;
        }
        input.measured_temp_c = control_temp;
        input.measurement_accepted = accepted.is_some();
        let snapshot = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        if accepted.is_some() {
            accepted_count += 1;
            assert_eq!(elapsed, 350, "the production settling wait must complete");
            assert!(
                (snapshot.filtered_slope_c_per_s - 0.4).abs() < 0.001,
                "bank={bank:?}, offset={offset_c}: new PPS origin fabricated velocity {}",
                snapshot.filtered_slope_c_per_s
            );
            assert_eq!(snapshot.filtered_temp_c, control_temp);
        }
    }
    assert_eq!(accepted_count, 1);
    // Compare the next local interval with an independently seeded
    // controller: neither the old origin nor its 300ms blind wait
    // may affect the derivative in the new voltage epoch.
    let mut local_reference = controller;
    local_reference.reseed_measurement(control_temp);
    local_reference.filtered_slope_c_per_profile_tick = 0.4;
    input.now_ms = 3_400;
    let local_temp = control_temp - 0.05;
    let accepted = accept_rtd_control_sample_after_pps_transition(
        &mut transition,
        &mut controller,
        &mut measurement,
        control_temp,
        20_500,
        input.now_ms,
        local_temp,
    );
    assert_eq!(accepted, Some(local_temp));
    input.measured_temp_c = local_temp;
    input.measurement_accepted = true;
    let actual = controller
        .update_production_at(kind, input, Some(profile))
        .0;
    let expected = local_reference
        .update_production_at(kind, input, Some(profile))
        .0;
    assert_eq!(actual.filtered_temp_c, expected.filtered_temp_c);
    assert_eq!(
        actual.filtered_slope_c_per_s,
        expected.filtered_slope_c_per_s
    );
    assert_eq!(actual.duty_percent, expected.duty_percent);
}

#[test]
fn pending_pps_measurement_origin_is_cleared_by_controller_resets() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let (mut base, profile, input) = pps_cooling_test_fixture(bank);
        base.reseed_measurement_after_pps_step(input.measured_temp_c);
        assert!(base.pps_measurement_anchor_pending);
        for reason in 0..5 {
            let mut reset = base;
            let mut next = input;
            next.now_ms += 50;
            let kind = ProductionThermalController::Profile(bank);
            match reason {
                0 => reset.reseed_measurement(next.measured_temp_c),
                1 => {
                    next.heater_enabled = false;
                    reset.update_production_at(kind, next, Some(profile));
                }
                2 => {
                    next.target_temp_c = 250;
                    reset.update_production_at(kind, next, Some(profile));
                }
                3 => {
                    reset.latch_fault(HeaterFaultReason::OverTemp);
                }
                _ => reset.clear_fault_latch(),
            }
            assert!(
                !reset.pps_measurement_anchor_pending,
                "bank={bank:?}, reset={reason}"
            );
        }
    }
}

fn pps_cooling_test_fixture(
    bank: ThermalProfileBank,
) -> (
    HeaterController,
    ThermalControlProfile,
    ThermalPlantRuntimeInput,
) {
    let kind = ProductionThermalController::Profile(bank);
    let mut controller = HeaterController::new();
    controller.production_controller = Some(kind);
    controller.last_target_temp_c = 240;
    controller.heater_was_enabled = true;
    controller.filtered_temp_c = Some(238.3);
    controller.previous_measured_temp_c = Some(238.3);
    controller.filtered_slope_c_per_profile_tick = 0.4;
    controller.profile_last_control_at_ms = Some(2_950);
    let mut profile = production_test_profile(1_000);
    let point = profile.points[0].as_mut().unwrap();
    point.target_temp_c = 240;
    point.brake_distance_centi_c = 197;
    point.approach_lead_ticks = 0;
    let mut input = production_test_input(238.3);
    input.target_temp_c = 240;
    input.model.transport_delay_ms = 2_650;
    assert_eq!(
        controller
            .update_production_at(kind, input, Some(profile))
            .0
            .duty_percent,
        5
    );
    (controller, profile, input)
}

#[test]
fn pps_sample_wait_does_not_invent_approach_deceleration() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 240;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(238.3);
        controller.previous_measured_temp_c = Some(238.3);
        controller.filtered_slope_c_per_profile_tick = 0.8;
        controller.profile_last_control_at_ms = Some(2_950);
        let mut profile = production_test_profile(1_000);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 240;
        point.brake_distance_centi_c = 197;
        point.approach_lead_ticks = 0;
        let mut input = production_test_input(238.3);
        input.target_temp_c = 240;
        input.model.transport_delay_ms = 2_650;
        controller.update_production_at(kind, input, Some(profile));
        let trusted_slope = controller.filtered_slope_c_per_profile_tick;
        let mut transition = RtdPpsTransitionGuard::new(21_000);
        let mut measurement = RtdControlMeasurementGuard::default();
        measurement.reseed(238.3, 3_000);
        for offset in (50..350).step_by(50) {
            input.now_ms = 3_000 + offset;
            let accepted = accept_rtd_control_sample_after_pps_transition(
                &mut transition,
                &mut controller,
                &mut measurement,
                238.3,
                20_500,
                input.now_ms,
                237.8,
            );
            assert_eq!(accepted, None);
            input.measurement_accepted = accepted.is_some();
            let snapshot = controller
                .update_production_at(kind, input, Some(profile))
                .0;
            assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
            assert!((5..=100).contains(&snapshot.duty_percent));
            assert_eq!(
                controller.filtered_slope_c_per_profile_tick, trusted_slope,
                "bank={bank:?}: a retained temperature is not a new observation"
            );
        }
    }
}

#[test]
fn resumed_production_measurement_uses_the_complete_sample_gap() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let profile = production_test_profile(1_000);
        let mut input = production_test_input(45.0);
        let mut coarse = HeaterController::new();
        coarse.update_production_at(kind, input, Some(profile));
        let mut blocked = coarse;
        input.measurement_accepted = false;
        for offset in (50..300).step_by(50) {
            input.now_ms = 3_000 + offset;
            blocked.update_production_at(kind, input, Some(profile));
        }
        input.now_ms = 3_300;
        input.measured_temp_c = 45.3;
        input.measurement_accepted = true;
        let resumed = blocked.update_production_at(kind, input, Some(profile)).0;
        let reference = coarse.update_production_at(kind, input, Some(profile)).0;
        assert!((resumed.filtered_temp_c - reference.filtered_temp_c).abs() < 0.0001);
        assert!(
            (resumed.filtered_slope_c_per_s - reference.filtered_slope_c_per_s).abs() < 0.0001,
            "bank={bank:?}: derivative must use 300ms, not the last 50ms control tick"
        );
    }
}

#[test]
fn five_amp_model_approach_ignores_profile_floor_and_ceiling() {
    let mut normal = HeaterController::new();
    normal.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    normal.phase = HeaterControlPhase::Approach;
    normal.heater_was_enabled = true;
    normal.last_target_temp_c = 60;
    normal.filtered_temp_c = Some(54.0);
    normal.previous_filtered_temp_c = Some(53.9);
    normal.previous_measured_temp_c = Some(54.0);
    let mut tuning = normal;
    let profile = production_test_profile(300);
    let input = production_test_input(54.5);
    let (normal_output, normal_disarmed) = normal.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        input,
        Some(profile),
    );
    let (tuning_output, tuning_disarmed) = tuning.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        input,
        Some(profile),
    );
    assert!(!normal_disarmed && !tuning_disarmed);
    assert_eq!(normal_output, tuning_output);
    assert!(normal_output.duty_percent >= 5);

    let mut changed_candidate = HeaterController::new();
    changed_candidate.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    changed_candidate.phase = HeaterControlPhase::Approach;
    changed_candidate.heater_was_enabled = true;
    changed_candidate.last_target_temp_c = 60;
    changed_candidate.filtered_temp_c = Some(54.0);
    changed_candidate.previous_filtered_temp_c = Some(53.9);
    changed_candidate.previous_measured_temp_c = Some(54.0);
    let mut changed_profile = production_test_profile(900);
    changed_profile.points[0]
        .as_mut()
        .unwrap()
        .approach_floor_power_permille = 800;
    let (changed_output, _) = changed_candidate.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        input,
        Some(changed_profile),
    );
    assert_eq!(normal_output.duty_percent, changed_output.duty_percent);
    let normal_pwm = heater_physical_pwm_percent(normal_output.duty_percent, 21_000, 21_000, 100);
    let changed_pwm = heater_physical_pwm_percent(changed_output.duty_percent, 21_000, 21_000, 100);
    assert_eq!(normal_pwm, changed_pwm);
}

#[test]
fn production_output_uses_candidate_settings_for_both_power_classes() {
    let mut profile = production_test_profile(300);
    profile.settings.auto_adjustable_working_floor_mv = 6_100;
    profile.settings.heater_current_reserve_ma = 300;
    for max_ma in [5_000, 3_250] {
        let (power, manual_pps) = production_power_fixture(max_ma);
        let kind = production_thermal_controller(power, ThermalProfileMode::Auto, &manual_pps);
        let output = thermal_output_settings(Some(profile), kind, CalibrationMode::Off);
        assert_eq!(output, profile.settings, "source current {max_ma}");
    }
    assert_eq!(
        thermal_output_settings(
            Some(profile),
            ProductionThermalController::Unavailable,
            CalibrationMode::ThermalPlant,
        ),
        profile.settings,
    );
    assert_eq!(
        thermal_output_settings(
            None,
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            CalibrationMode::Off
        ),
        ThermalControlProfileSettings::default(),
    );
}

#[test]
fn adc_conversion_ready_in_microseconds_does_not_require_a_timer_wakeup() {
    let polls = core::cell::Cell::new(0_u64);
    let result = poll_adc_conversion(
        || {
            polls.set(polls.get() + 1);
            if polls.get() >= 6 {
                Ok(1_234_u16)
            } else {
                Err(nb::Error::WouldBlock)
            }
        },
        || polls.get(),
    );
    assert_eq!(result, Ok(1_234));
}

#[test]
fn adc_conversion_polling_yields_when_slow_and_preserves_errors() {
    let polls = core::cell::Cell::new(0_u64);
    let result: nb::Result<u16, ()> = poll_adc_conversion(
        || {
            polls.set(polls.get() + 1);
            Err(nb::Error::WouldBlock)
        },
        || polls.get(),
    );
    assert_eq!(result, Err(nb::Error::WouldBlock));
    assert!(polls.get() <= 50);
    assert_eq!(
        poll_adc_conversion::<u16>(|| Err(nb::Error::Other(())), || 0),
        Err(nb::Error::Other(())),
    );
}

#[test]
fn adc_new_conversion_cannot_consume_the_previous_channel_while_busy() {
    let idle_polls = core::cell::Cell::new(0_u64);
    let reads = core::cell::Cell::new(0_u64);
    let idle = wait_for_adc_idle(
        || {
            idle_polls.set(idle_polls.get() + 1);
            idle_polls.get() >= 4
        },
        || idle_polls.get(),
    );
    let result = idle.then(|| {
        poll_adc_conversion(
            || {
                reads.set(reads.get() + 1);
                // A premature trigger still exposes the preceding VIN result.
                Ok(if idle_polls.get() < 4 {
                    3_980_u16
                } else {
                    2_300
                })
            },
            || 0,
        )
    });
    assert_eq!(result, Some(Ok(2_300)));
    assert_eq!(reads.get(), 1);
    assert!(idle);
}

#[test]
fn adc_idle_reset_boundary_retains_a_conversion_turnaround_interval() {
    let elapsed = core::cell::Cell::new(0_u64);
    let ready = wait_for_adc_idle(
        || true,
        || {
            elapsed.set(elapsed.get() + 1);
            elapsed.get()
        },
    );
    let sample = ready.then(|| {
        // Idle alone does not provide an interval after the previous reset.
        if elapsed.get() < 50 { 3_980_u16 } else { 2_300 }
    });
    assert_eq!(sample, Some(2_300));
    assert_eq!(elapsed.get(), 50);
}

#[test]
fn adc_idle_gate_times_out_without_triggering_and_preserves_inflight_conversion() {
    let polls = core::cell::Cell::new(0_u64);
    let idle = wait_for_adc_idle(
        || {
            polls.set(polls.get() + 1);
            false
        },
        || polls.get(),
    );
    assert!(!idle);
    assert_eq!(polls.get(), 1_000);
    let read = idle.then(|| panic!("busy ADC must not start a new conversion"));
    assert_eq!(read, None::<u16>);
    assert_eq!(poll_adc_conversion(|| Ok(2_300_u16), || 0,), Ok(2_300),);
}

#[test]
fn production_source_class_change_disarms_before_switching_controller() {
    let mut controller = HeaterController::new();
    let (first, disarmed) = controller.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps3a),
        production_test_input(30.0),
        None,
    );
    assert!(!disarmed);
    assert!(first.duty_percent > 0);
    let (switched, disarmed) = controller.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        production_test_input(31.0),
        Some(production_test_profile(900)),
    );
    assert!(disarmed);
    assert_eq!(switched.duty_percent, 0);
    assert!(!controller.heater_was_enabled);
}

#[test]
fn both_power_classes_use_the_same_calibrated_profile_control() {
    let mut direct = HeaterController::new();
    let mut selected = HeaterController::new();
    for (index, temperature_c) in [30.0, 40.0, 56.0, 59.7, 60.2].into_iter().enumerate() {
        let input = ThermalPlantRuntimeInput {
            now_ms: index as u64 * 50,
            ..production_test_input(temperature_c)
        };
        let (baseline, _) = direct.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
            input,
            Some(production_test_profile(900)),
        );
        let (actual, disarmed) = selected.update_production_at(
            ProductionThermalController::Profile(ThermalProfileBank::Pps3a),
            input,
            Some(production_test_profile(900)),
        );
        assert!(!disarmed);
        assert_eq!(actual, baseline);
    }
}

#[test]
fn heater_adjustable_voltage_clamps_to_requested_ceiling() {
    assert_eq!(
        heater_request_mv_from_power_percent(100, HEATER_ADJUSTABLE_MIN_MV, 21_000),
        21_000
    );
    assert_eq!(
        heater_request_mv_from_power_percent(100, HEATER_ADJUSTABLE_MIN_MV, 19_000),
        19_000
    );
    assert_eq!(
        heater_request_mv_from_power_percent(0, 15_000, 21_000),
        15_000
    );
}

#[test]
fn heater_adjustable_voltage_never_requests_below_ch224q_hardware_floor() {
    assert_eq!(clamp_ch224q_adjustable_request_mv(3_300), 5_000);
    assert_eq!(
        heater_request_mv_from_power_percent(0, 3_300, 21_000),
        5_000
    );
    assert_eq!(
        heater_request_mv_from_power_percent(100, 3_300, 4_800),
        5_000
    );
}

#[test]
fn adjustable_pps_same_mode_voltage_changes_keep_heater_gate_active() {
    assert!(!should_blank_heater_for_adjustable_request(
        14_000, 14_100, false
    ));
    assert!(!should_blank_heater_for_adjustable_request(
        14_100, 14_000, false
    ));
    assert!(should_blank_heater_for_adjustable_request(
        14_000, 14_100, true
    ));
}

#[test]
fn adjustable_pps_request_suppresses_sub_500mv_churn() {
    assert_eq!(
        heater_adjustable_request_mv(50, true, 10_000, 12_000, 6_100, 14_000),
        10_000
    );
    let material_request_mv = heater_request_mv_from_power_percent(60, 6_100, 14_000);
    assert!(material_request_mv.abs_diff(10_000) >= HEATER_PPS_REQUEST_HYSTERESIS_MV);
    assert_eq!(
        heater_adjustable_request_mv(60, true, 10_000, 12_000, 6_100, 14_000),
        10_500
    );
}

#[test]
fn adjustable_pps_request_transition_distinguishes_same_mode_and_path_changes() {
    assert_eq!(pps_request_transition_ms(false), 500);
    assert_eq!(pps_request_transition_ms(true), 275);
}

#[test]
fn adjustable_pps_request_ramps_in_500mv_steps() {
    assert_eq!(
        heater_adjustable_request_mv(100, true, 10_000, 12_000, 6_100, 20_000),
        10_500
    );
    assert_eq!(
        heater_adjustable_request_mv(1, true, 12_000, 12_000, 6_100, 20_000),
        11_500
    );
}

#[test]
fn heater_safe_max_matches_65w_temperature_limits() {
    assert_eq!(
        heater_safe_max_mv_for_temp(0.0, 3_250, 21_000, None, &MemoryConfig::default()),
        9_500
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(20.0, 3_250, 21_000, None, &MemoryConfig::default()),
        10_400
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(60.0, 3_250, 21_000, None, &MemoryConfig::default()),
        12_000
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(85.0, 3_250, 21_000, None, &MemoryConfig::default()),
        13_000
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(165.0, 3_250, 21_000, None, &MemoryConfig::default()),
        16_300
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(296.0, 3_250, 21_000, None, &MemoryConfig::default()),
        21_000
    );
}

#[test]
fn heater_safe_max_preserves_higher_power_sources() {
    assert_eq!(
        heater_safe_max_mv_for_temp(20.0, 5_000, 24_000, None, &MemoryConfig::default()),
        16_000
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(165.0, 5_000, 24_000, None, &MemoryConfig::default()),
        24_000
    );
}

#[test]
fn obsolete_resistance_curve_does_not_set_the_power_ceiling() {
    let mut config = MemoryConfig::default();
    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 2_000,
        resistance_milliohms: 3_936,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 22_000,
        resistance_milliohms: 5_674,
    });

    let pps3a_power_mw =
        heater_available_power_mw_for_temp(160.0, Some(20_000), Some(3_250), None, &config);
    let pps5a_power_mw =
        heater_available_power_mw_for_temp(160.0, Some(21_000), Some(5_000), None, &config);

    assert_eq!(
        pps3a_power_mw,
        heater_available_power_mw_for_temp(
            160.0,
            Some(20_000),
            Some(3_250),
            None,
            &MemoryConfig::default()
        )
    );
    assert!(pps5a_power_mw > 70_000);
}

#[test]
fn production_pps_ceiling_ignores_obsolete_resistance_and_current_reserve() {
    let mut config = MemoryConfig::default();
    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 2_000,
        resistance_milliohms: 3_200,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 21_500,
        resistance_milliohms: 6_680,
    });

    assert_eq!(
        production_pps_request_ceiling_mv(215.0, 3_000, 200, 21_000, None, &config),
        16_900
    );
}

#[test]
fn pps_request_path_drop_compensation_cannot_relax_the_plate_limit() {
    assert_eq!(
        heater_source_request_ceiling_mv(17_300, 17_300, 15_900, 21_000),
        17_300
    );
    assert_eq!(
        heater_source_request_ceiling_mv(17_300, 17_300, 0, 21_000),
        17_300
    );
    assert_eq!(
        heater_source_request_ceiling_mv(20_000, 21_000, 17_000, 21_000),
        20_000
    );
}

#[test]
fn obsolete_resistance_curve_is_absent_from_product_wire_state() {
    let mut config = MemoryConfig::default();

    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 2_000,
        resistance_milliohms: 4_000,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 20_000,
        resistance_milliohms: 5_600,
    });

    let wire = flux_purr_firmware::control_plane::heater_curve_state_from_memory(&config, None);
    assert!(wire.active.points.iter().all(Option::is_none));
    assert!(wire.preview.is_none());
}

#[test]
fn obsolete_raw_observations_cannot_reenter_the_automatic_resistance_model() {
    let mut config = MemoryConfig::default();
    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 0,
        resistance_milliohms: 2_948,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 21_670,
        resistance_milliohms: 5_674,
    });
    config.heater_curve_raw_observations.points[0] =
        Some(flux_purr_firmware::memory::HeaterCurveRawObservation {
            raw_rtd_adc_mv: 1_269,
            heater_voltage_mv: 18_500,
            heater_current_ma: 4_700,
            resistance_milliohms: 3_936,
        });
    config.heater_curve_raw_observations.points[1] =
        Some(flux_purr_firmware::memory::HeaterCurveRawObservation {
            raw_rtd_adc_mv: 1_300,
            heater_voltage_mv: 18_500,
            heater_current_ma: 4_700,
            resistance_milliohms: 3_936,
        });

    let resistance = automatic_heater_resistance_ohms(216.7, None, &config);
    assert_eq!(resistance, default_estimated_heater_resistance_ohms(216.7));
    assert_eq!(
        estimated_heater_resistance_with_source(216.7, None, &config).1,
        AutomaticHeaterResistanceSource::Theoretical
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(216.7, 4_700, 21_000, None, &config),
        21_000
    );
}

#[test]
fn effective_pps_current_limit_uses_contract_not_instantaneous_draw() {
    let status_limit = effective_pps_current_limit_ma(
        5_000,
        Some(PdStatusObservation {
            status_raw: 0,
            status: Status {
                pd_active: true,
                ..Status::default()
            },
            current_raw: 40,
            current_ma: 2_000,
            contract_voltage_mv: None,
            contract: Contract::none(),
        }),
    );
    assert_eq!(status_limit, 5_000);

    let zero_draw_keeps_contract = effective_pps_current_limit_ma(
        5_000,
        Some(PdStatusObservation {
            status_raw: 0,
            status: Status {
                pd_active: true,
                ..Status::default()
            },
            current_raw: 0,
            current_ma: 0,
            contract_voltage_mv: None,
            contract: Contract::none(),
        }),
    );
    assert_eq!(zero_draw_keeps_contract, 5_000);

    let refreshed_pps_contract_limits_the_budget = effective_pps_current_limit_ma(
        5_000,
        Some(PdStatusObservation {
            status_raw: 0,
            status: Status {
                pd_active: true,
                ..Status::default()
            },
            current_raw: 0,
            current_ma: 3_000,
            contract_voltage_mv: Some(24_000),
            contract: Contract::observed(ContractKind::Pps, 24_000, 3_000),
        }),
    );
    assert_eq!(refreshed_pps_contract_limits_the_budget, 3_000);

    assert_eq!(effective_pps_current_limit_ma(5_000, None), 5_000);
}

#[test]
fn automatic_controller_power_budget_tracks_the_confirmed_contract_current() {
    let observation = |current_ma, voltage_mv| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma,
        contract_voltage_mv: Some(voltage_mv),
        contract: Contract::observed(ContractKind::Pps, voltage_mv, current_ma),
    };
    assert_eq!(
        automatic_heater_controller_current_ma(
            3_250,
            5_000,
            9_700,
            Some(observation(3_000, 8_900)),
        ),
        3_000
    );
    assert_eq!(
        automatic_heater_controller_current_ma(
            3_250,
            5_000,
            9_700,
            Some(observation(2_950, 8_900)),
        ),
        0
    );
    assert_eq!(
        automatic_heater_controller_current_ma(3_250, 5_000, 9_700, None),
        3_250
    );
}

#[test]
fn automatic_full_power_reaches_the_quantized_voltage_ceiling_and_full_conduction() {
    // The watt interface carries integer mW. Truncating the same physical
    // ceiling must not lose another 100 mV or turn full conduction into 99%.
    for resistance_ohms in [2.8_f32, 3.2, 3.74, 5.0, 7.5] {
        for ceiling_mv in [5_500_u16, 8_500, 9_700, 10_400, 15_200, 21_000] {
            let maximum_mw = (f32::from(ceiling_mv).powi(2) / resistance_ohms / 1_000.0) as u32;
            assert_eq!(
                automatic_heater_request_mv_from_power(
                    maximum_mw,
                    resistance_ohms,
                    5_500,
                    ceiling_mv,
                ),
                ceiling_mv,
            );
            assert_eq!(
                automatic_heater_pwm_percent_for_power(
                    maximum_mw,
                    resistance_ohms,
                    ceiling_mv,
                    100
                ),
                100,
            );
            assert_eq!(
                automatic_heater_pwm_percent_for_power(maximum_mw, resistance_ohms, ceiling_mv, 42),
                42,
            );
            assert!(
                automatic_heater_pwm_percent_for_power(
                    maximum_mw.saturating_sub(1),
                    resistance_ohms,
                    ceiling_mv,
                    100,
                ) < 100,
            );
        }
    }
}

#[test]
fn automatic_heater_policy_caps_the_apdo_budget_at_three_amps() {
    let apdos = [
        Some(ch224q::PpsApdo {
            min_mv: 5_000,
            max_mv: 20_000,
            max_ma: 3_250,
        }),
        None,
        None,
        None,
        None,
        None,
        None,
    ];
    let candidate = select_automatic_heater_pps_candidate(
        &apdos,
        5_000,
        20_000,
        HEATER_ADJUSTABLE_MAX_MV,
        3.2,
        None,
    )
    .expect("3.25 A APDO leaves a safe automatic range");
    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 9_600);
    assert_eq!(candidate.power_mw, 28_800);
    assert_eq!(
        heater_available_power_mw_for_temp(
            20.0,
            Some(20_000),
            Some(3_250),
            None,
            &MemoryConfig::default(),
        ),
        33_800
    );
    assert_eq!(
        automatic_heater_request_mv_from_power(15_000, 3.2, 5_000, 9_700),
        6_900
    );
    assert_eq!(
        automatic_heater_pwm_percent_for_power(5_000, 3.2, 5_500, 100),
        52
    );
    assert!(automatic_heater_fixed_is_safe(9_000, 3_000, 28_000, 3.2));
    assert!(!automatic_heater_fixed_is_safe(9_000, 3_000, 28_000, 2.8));
    assert!(automatic_heater_fixed_is_safe(9_000, 3_250, 28_000, 3.2));
}

#[test]
fn automatic_heater_policy_accepts_exact_3a_apdo_budget() {
    let apdos = [
        Some(ch224q::PpsApdo {
            min_mv: 5_000,
            max_mv: 20_000,
            max_ma: 3_000,
        }),
        None,
        None,
        None,
        None,
        None,
        None,
    ];
    let candidate = select_automatic_heater_pps_candidate(
        &apdos,
        5_000,
        20_000,
        HEATER_ADJUSTABLE_MAX_MV,
        3.2,
        None,
    )
    .expect("3 A APDO leaves a safe automatic range");
    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 9_600);
    assert_eq!(candidate.power_mw, 28_800);
}

#[test]
fn automatic_heater_pps_request_ramps_large_voltage_changes() {
    assert_eq!(
        automatic_heater_step_request_mv(8_400, 10_300, 5_500, 10_300),
        8_900
    );
    assert_eq!(
        automatic_heater_step_request_mv(8_900, 7_700, 5_500, 10_300),
        8_400
    );
    assert_eq!(
        automatic_heater_step_request_mv(12_000, 7_700, 5_500, 7_700),
        7_700
    );
}

#[test]
fn automatic_heater_observed_three_amp_boundary_can_raise_the_preset_ceiling() {
    let candidate = automatic_heater_pps_candidate_with_limit(
        ch224q::PpsApdo {
            min_mv: 5_500,
            max_mv: 21_000,
            max_ma: 3_000,
        },
        5_500,
        21_000,
        HEATER_ADJUSTABLE_MAX_MV,
        3.2,
        0,
        Some(18_000),
    )
    .expect("the observed 3 A tracking boundary is usable");
    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 18_000);
    assert_eq!(candidate.power_mw, 101_250);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_boundary_uses_encoded_budget_for_five_amp_apdo() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 5_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_000,
        max_mv: 21_000,
        max_ma: 5_000,
    });
    let mut boundary = synthetic_supply_boundary();
    boundary.source_max_mv = 21_000;
    boundary.advertised_max_mv = 21_000;
    assert!(boundary.is_valid());
    let projected_boundary_temperature =
        projected_rtd_temperature_c(&MemoryConfig::default(), boundary.points[0].rtd_mv)
            .expect("synthetic boundary point projects to a temperature");
    assert!((projected_boundary_temperature - 100.0).abs() < 0.3);
    let observed_limit = boundary.safe_request_mv(
        flux_purr_firmware::heater_supply::BoundaryQuery {
            temperature_centi_c: 10_100,
            current_rtd_mv: None,
            current_request_mv: None,
            current_ma: 3_000,
            wifi_started: false,
            source_min_mv: 5_000,
            source_max_mv: 21_000,
            source_max_ma: 3_000,
            source_power_limit_mw: 0,
        },
        |raw| {
            projected_rtd_temperature_c(&MemoryConfig::default(), raw)
                .map(|temperature| round_to_i16(temperature * 100.0))
        },
    );
    assert_eq!(observed_limit, Some(10_000));
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let candidate = manual
        .automatic_heater_pps_candidate(101.0, None, &config, None)
        .expect("encoded 3 A budget keeps the calibrated boundary usable");

    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 10_000);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_boundary_matches_a_raw_apdo_below_the_fusb_floor() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 5_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 5_000,
    });
    let mut boundary = synthetic_supply_boundary();
    boundary.source_max_mv = 20_000;
    boundary.advertised_min_mv = 3_300;
    boundary.advertised_max_mv = 21_000;
    boundary.points[0].rtd_mv = runtime_test_raw_rtd_adc_mv_for_temp(60.0);
    boundary.points[0].tracking_request_mv = 9_200;
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let candidate = manual
        .automatic_heater_pps_candidate(68.0, None, &config, None)
        .expect("the persisted 3 A boundary should cover the next colder bucket");

    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 9_200);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_boundary_uses_coldest_observed_floor_before_first_observation() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 5_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 5_000,
    });
    let mut boundary = synthetic_supply_boundary();
    boundary.advertised_min_mv = 3_300;
    boundary.advertised_max_mv = 21_000;
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let candidate = manual
        .automatic_heater_pps_candidate(25.0, None, &config, None)
        .expect("a valid boundary must keep cold startup inside the observed safe floor");

    assert_eq!(candidate.allowed_mv, 10_000);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_boundary_carries_the_colder_point_across_a_rounded_bin_edge() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 5_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 5_000,
    });
    let mut boundary = synthetic_supply_boundary();
    boundary.source_max_mv = 20_000;
    boundary.advertised_min_mv = 3_300;
    boundary.advertised_max_mv = 21_000;
    boundary.points[0].rtd_mv = runtime_test_raw_rtd_adc_mv_for_temp(137.6);
    boundary.points[0].tracking_request_mv = 18_300;
    boundary.points[0].flat_request_mv = 0;
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let candidate = manual
        .automatic_heater_pps_candidate(148.3, None, &config, None)
        .expect("the colder observed point covers the rounded bin edge");

    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 18_300);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_boundary_uses_the_adjacent_live_rtd_quantization_edge() {
    let manual = thermal_plant_job_manual_pps(20_000, 3_000);
    let mut boundary = synthetic_supply_boundary();
    boundary.points[0].rtd_mv = 1_307;
    boundary.points[0].tracking_request_mv = 18_800;
    boundary.points[0].flat_request_mv = 0;
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let candidate = manual
        .automatic_heater_pps_candidate_with_rtd(159.64, None, &config, None, Some(1_306))
        .expect("the adjacent persisted RTD code should keep the next safe voltage");

    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 18_800);
    assert!(candidate.boundary_applied);
}

#[test]
fn automatic_heater_pps_selection_keeps_the_current_apdo_on_a_power_tie() {
    let apdos = [
        Some(ch224q::PpsApdo {
            min_mv: 10_000,
            max_mv: 20_000,
            max_ma: 3_250,
        }),
        Some(ch224q::PpsApdo {
            min_mv: 5_000,
            max_mv: 20_000,
            max_ma: 3_250,
        }),
        None,
        None,
        None,
        None,
        None,
    ];
    let candidate = select_automatic_heater_pps_candidate(
        &apdos,
        5_000,
        20_000,
        HEATER_ADJUSTABLE_MAX_MV,
        3.2,
        Some(7_000),
    )
    .expect("both APDOs are safe");
    assert_eq!(candidate.advertisement_index, 1);
}

#[test]
fn automatic_heater_fixed_fallback_uses_only_advertised_safe_contracts() {
    let mut source = SourceCapabilitiesView::default();
    source.fixed[0] = Some((5_000, 3_000));
    source.fixed[1] = Some((9_000, 3_250));
    source.fixed[2] = Some((15_000, 4_000));

    let (request, request_ma) = automatic_heater_fixed_request(source, 3.2)
        .expect("the advertised 9 V fixed PDO is safe under the preset load model");
    assert_eq!(request, ch224q::VoltageRequest::V9);
    assert_eq!(request_ma, 3_000);

    source.fixed[1] = Some((9_000, 3_000));
    let (request, request_ma) = automatic_heater_fixed_request(source, 2.8)
        .expect("the advertised 5 V fixed PDO remains safe");
    assert_eq!(request, ch224q::VoltageRequest::V5);
    assert_eq!(request_ma, 3_000);

    source.fixed[0] = Some((5_000, 1_500));
    assert!(automatic_heater_fixed_request(source, 2.8).is_none());

    source.fixed = [Some((28_000, 5_000)), None, None, None, None, None, None];
    assert!(automatic_heater_fixed_request(source, 2.8).is_none());
}

#[test]
fn heater_current_reserve_leaves_board_power_headroom() {
    assert_eq!(heater_available_current_ma(3_250, 200), 3_050);
    assert_eq!(heater_available_current_ma(3_200, 200), 3_000);
    assert_eq!(heater_available_current_ma(150, 200), 0);
}

#[test]
fn current_limit_fixed_pwm_fallback_uses_hysteresis() {
    assert_eq!(HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(), 9_000);
    assert!(should_apply_current_limit_fixed_pwm_fallback(
        100, false, false, 10_400, 12_000
    ));
    assert!(should_apply_current_limit_fixed_pwm_fallback(
        100, false, true, 12_100, 12_000
    ));
    assert!(!should_apply_current_limit_fixed_pwm_fallback(
        100, false, true, 12_200, 12_000
    ));
    assert!(!should_apply_current_limit_fixed_pwm_fallback(
        0, false, true, 9_500, 12_000
    ));
    assert!(!should_apply_current_limit_fixed_pwm_fallback(
        100, true, false, 10_400, 12_000
    ));
}

#[test]
fn current_limit_fallback_waits_for_a_confirmed_pps_contract() {
    assert!(!current_limit_fallback_requires_confirmed_pps(None));

    let fixed = PdStatusObservation {
        status_raw: 0,
        status: Status {
            pd_active: true,
            ..Status::default()
        },
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    };
    assert!(!current_limit_fallback_requires_confirmed_pps(Some(fixed)));

    let pps = PdStatusObservation {
        contract: Contract::observed(ContractKind::Pps, 20_000, 3_000),
        current_ma: 3_000,
        contract_voltage_mv: Some(20_000),
        ..fixed
    };
    assert!(current_limit_fallback_requires_confirmed_pps(Some(pps)));
}

#[test]
fn automatic_pps_start_failure_retries_while_fixed_contract_is_confirmed() {
    let fixed = PdStatusObservation {
        status_raw: 0,
        status: Status {
            pd_active: true,
            ..Status::default()
        },
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    };
    assert!(retry_pps_startup_after_fixed_contract_failure(Some(fixed)));

    let pps = PdStatusObservation {
        contract: Contract::observed(ContractKind::Pps, 20_000, 3_000),
        current_ma: 3_000,
        contract_voltage_mv: Some(20_000),
        ..fixed
    };
    assert!(!retry_pps_startup_after_fixed_contract_failure(Some(pps)));
    assert!(!retry_pps_startup_after_fixed_contract_failure(None));
}

#[test]
fn automatic_pps_start_preserves_heater_intent_from_fixed_standby() {
    let backend = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_500,
        idle_request_mv: 5_500,
        pps_max_mv: 21_000,
        adjustable_max_mv: 21_000,
        capability_max_ma: 3_000,
        current_mode: None,
        current_request_mv: 5_500,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: false,
    };

    assert!(automatic_pps_transition_can_preserve_heater_intent(
        backend, true, false, true
    ));
    assert!(!automatic_pps_transition_can_preserve_heater_intent(
        backend, true, true, true
    ));
    assert!(!automatic_pps_transition_can_preserve_heater_intent(
        backend, true, false, false
    ));
}

#[test]
fn tuning_power_preparation_requests_the_source_ceiling_directly() {
    assert_eq!(
        automatic_pps_request_mv(0, false, true, 12_000, 12_000, 6_100, 21_000),
        21_000
    );
}

#[test]
fn adjustable_working_floor_respects_capability_and_maximum() {
    let settings = ThermalControlProfileSettings {
        auto_adjustable_working_floor_mv: 6_100,
        ..ThermalControlProfileSettings::default()
    };
    assert_eq!(
        effective_auto_adjustable_working_floor_mv(settings, 5_000, 20_000),
        6_100
    );
    assert_eq!(
        effective_auto_adjustable_working_floor_mv(settings, 9_000, 20_000),
        9_000
    );
    assert_eq!(
        effective_auto_adjustable_working_floor_mv(settings, 5_000, 6_000),
        6_000
    );

    let minimum_settings = ThermalControlProfileSettings {
        auto_adjustable_working_floor_mv: 5_000,
        ..ThermalControlProfileSettings::default()
    };
    assert_eq!(
        effective_auto_adjustable_working_floor_mv(minimum_settings, 5_000, 20_000),
        5_000
    );
    assert_eq!(
        effective_auto_adjustable_working_floor_mv(minimum_settings, 9_000, 20_000),
        9_000
    );
}

#[test]
fn current_limit_fixed_pwm_fallback_caps_low_current_duty() {
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(100, 20.0, 1_000, None, &MemoryConfig::default()),
        35
    );
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(50, 20.0, 1_000, None, &MemoryConfig::default()),
        35
    );
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(20, 20.0, 1_000, None, &MemoryConfig::default()),
        20
    );
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(100, 20.0, 0, None, &MemoryConfig::default()),
        0
    );
}

#[test]
fn current_limit_fixed_pwm_fallback_preserves_65w_duty() {
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(100, 0.0, 3_250, None, &MemoryConfig::default()),
        100
    );
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(100, 20.0, 3_250, None, &MemoryConfig::default()),
        100
    );
    assert_eq!(
        current_limit_fixed_pwm_duty_percent(42, 20.0, 3_250, None, &MemoryConfig::default()),
        42
    );
}

#[test]
fn fixed_pd_runtime_caps_cold_plate_duty_to_the_negotiated_current_budget() {
    let config = MemoryConfig::default();

    assert_eq!(
        fixed_pd_pwm_duty_percent(100, 20.0, 12_000, 3_250, 200, None, &config,),
        80
    );
    assert_eq!(
        fixed_pd_pwm_duty_percent(50, 20.0, 12_000, 3_250, 200, None, &config,),
        50
    );
    assert_eq!(
        fixed_pd_pwm_duty_percent(100, 20.0, 12_000, 0, 200, None, &config),
        0
    );
}

#[test]
fn heater_backend_uses_pps_mos_only_when_pps_covers_20v() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(3_300),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: Some(15_000),
            avs_max_mv: Some(28_000),
            ..Default::default()
        }),
        Some(Status {
            avs_exist: true,
            ..Status::default()
        }),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 28_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 12_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );

    let fallback = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: false,
            pps_min_mv: Some(3_300),
            pps_max_mv: Some(15_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }),
        Some(Status::default()),
    );
    assert_eq!(
        fallback,
        HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            terminal_fixed_pd_disarmed: false,
        }
    );
}

#[test]
fn heater_backend_uses_the_20v_apdo_current_not_another_range() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(5_000),
            pps_apdos: [
                Some(ch224q::PpsApdo {
                    min_mv: 5_000,
                    max_mv: 11_000,
                    max_ma: 5_000,
                }),
                Some(ch224q::PpsApdo {
                    min_mv: 20_000,
                    max_mv: 21_000,
                    max_ma: 3_000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }),
        Some(Status::default()),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 20_000,
            idle_request_mv: 20_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 21_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 20_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );
}

#[test]
fn heater_backend_limits_to_pps_when_avs_is_unavailable() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(3_300),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: Some(15_000),
            avs_max_mv: Some(28_000),
            ..Default::default()
        }),
        Some(Status::default()),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 21_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 12_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );
    assert_eq!(
        heater_request_mv_from_power_percent(100, HEATER_ADJUSTABLE_MIN_MV, 21_000),
        21_000
    );
}

#[test]
fn heater_backend_clamps_avs_to_advertised_capability() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(3_300),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: Some(15_000),
            avs_max_mv: Some(24_000),
            ..Default::default()
        }),
        Some(Status {
            avs_exist: true,
            ..Status::default()
        }),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 24_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 12_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );
    assert_eq!(
        heater_request_mv_from_power_percent(100, HEATER_ADJUSTABLE_MIN_MV, 24_000),
        24_000
    );
}

#[test]
fn heater_backend_ignores_avs_without_advertised_range() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(3_300),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }),
        Some(Status {
            avs_exist: true,
            ..Status::default()
        }),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 21_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 12_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );
}

#[test]
fn heater_backend_clamps_low_end_to_advertised_pps_minimum() {
    let backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(15_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            avs_min_mv: None,
            avs_max_mv: None,
            ..Default::default()
        }),
        Some(Status::default()),
    );

    assert_eq!(
        backend,
        HeaterPowerBackend::PpsMos {
            pps_min_mv: 15_000,
            idle_request_mv: 15_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 21_000,
            capability_max_ma: 3_000,
            current_mode: None,
            current_request_mv: 15_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        }
    );
    assert_eq!(
        heater_request_mv_from_power_percent(0, 15_000, 21_000),
        15_000
    );
}

#[test]
fn auto_cooling_policy_runs_full_speed_then_cooldown_below_35c() {
    let stopped = fan_policy_decision(34, 0, false, 0, true, FanPolicyState::Disabled, false);
    assert_eq!(stopped.command, FanHardwareCommand::disabled());
    assert_eq!(stopped.display_state, FanDisplayState::Auto);

    let active = fan_policy_decision(35, 0, false, 0, true, FanPolicyState::Disabled, false);
    assert_eq!(
        active.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
    assert_eq!(active.state, FanPolicyState::ActiveCooling);
    assert_eq!(active.display_state, FanDisplayState::Run);

    let still_active = fan_policy_decision(60, 0, false, 0, true, FanPolicyState::Disabled, false);
    assert_eq!(
        still_active.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );

    let cooldown = fan_policy_decision(
        34,
        1_000,
        false,
        0,
        true,
        FanPolicyState::ActiveCooling,
        false,
    );
    assert_eq!(
        cooldown.state,
        FanPolicyState::ActiveCoolingCooldown { until_ms: 31_000 }
    );
    assert_eq!(
        cooldown.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Minimum)
    );

    let still_cooling = fan_policy_decision(
        34,
        30_500,
        false,
        0,
        true,
        FanPolicyState::ActiveCoolingCooldown { until_ms: 31_000 },
        false,
    );
    assert_eq!(
        still_cooling.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Minimum)
    );

    let stopped_after_cooldown = fan_policy_decision(
        34,
        31_000,
        false,
        0,
        true,
        FanPolicyState::ActiveCoolingCooldown { until_ms: 31_000 },
        false,
    );
    assert_eq!(
        stopped_after_cooldown.command,
        FanHardwareCommand::disabled()
    );

    let full = fan_policy_decision(61, 0, false, 0, true, FanPolicyState::Disabled, false);
    assert_eq!(
        full.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
}

#[test]
fn multi_level_post_heat_cooling_follows_temperature_bands() {
    let normal_hot = fan_policy_decision_with_modes(
        41,
        0,
        false,
        true,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::Disabled, false),
    );
    assert_eq!(normal_hot.source, FanPolicySource::PostHeat);
    assert_eq!(normal_hot.output_level, FanOutputLevel::Medium);
    assert_eq!(
        normal_hot.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf)
    );

    let normal_cool = fan_policy_decision_with_modes(
        40,
        1_000,
        false,
        false,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::PostHeatMedium, false),
    );
    assert_eq!(
        normal_cool.state,
        FanPolicyState::PostHeatCooldown {
            until_ms: 31_000,
            profile: FanVoltageProfile::Minimum,
        }
    );
    assert_eq!(normal_cool.output_level, FanOutputLevel::Low);
    assert!(normal_cool.command.enabled);

    let normal_done = fan_policy_decision_with_modes(
        40,
        31_000,
        false,
        false,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Medium,
        (
            FanPolicyState::PostHeatCooldown {
                until_ms: 31_000,
                profile: FanVoltageProfile::Minimum,
            },
            false,
        ),
    );
    assert!(!normal_done.command.enabled);

    let fast_high = fan_policy_decision_with_modes(
        61,
        0,
        false,
        true,
        PostHeatCoolingMode::Fast,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::Disabled, false),
    );
    assert_eq!(
        fast_high.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
    let fast_medium = fan_policy_decision_with_modes(
        50,
        0,
        false,
        false,
        PostHeatCoolingMode::Fast,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::ActiveCooling, false),
    );
    assert_eq!(
        fast_medium.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf)
    );
    let fast_tail = fan_policy_decision_with_modes(
        40,
        0,
        false,
        false,
        PostHeatCoolingMode::Fast,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::PostHeatMedium, false),
    );
    assert_eq!(
        fast_tail.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf)
    );
}

#[test]
fn tuning_cooldown_forces_full_fan_without_changing_normal_post_heat_policy() {
    for mode in [
        PostHeatCoolingMode::Off,
        PostHeatCoolingMode::Normal,
        PostHeatCoolingMode::Fast,
    ] {
        let ordinary = fan_policy_decision_with_modes(
            55,
            1_000,
            false,
            true,
            mode,
            HeatingFanGuardMode::Medium,
            (FanPolicyState::Disabled, false),
        );
        let cooling = tuning_cooldown_fan_decision(ordinary, true);
        assert_eq!(cooling.state, FanPolicyState::Full);
        assert_eq!(
            cooling.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
        );
        assert_eq!(cooling.display_state, FanDisplayState::Run);
        assert_eq!(cooling.output_level, FanOutputLevel::High);
        assert_eq!(cooling.source, FanPolicySource::PostHeat);
        assert_eq!(tuning_cooldown_fan_decision(ordinary, false), ordinary);
    }
}

#[test]
fn tuning_cooldown_preserves_safety_fan_provenance() {
    let safety = FanPolicyDecision {
        state: FanPolicyState::SafeHalf,
        command: FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf),
        display_state: FanDisplayState::Safe,
        source: FanPolicySource::Safety,
        output_level: FanOutputLevel::Medium,
    };
    let cooling = tuning_cooldown_fan_decision(safety, true);
    assert_eq!(
        cooling.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
    assert_eq!(cooling.source, FanPolicySource::Safety);
    assert_eq!(cooling.display_state, FanDisplayState::Safe);
}

#[test]
fn tuning_cooling_continues_after_terminal_until_strict_35c_for_every_target() {
    let terminal_request = |latched, was_armed, heater_enabled, temperature, target| {
        tuning_cooling_requested(
            latched,
            was_armed,
            false,
            ThermalTuningPhase::Failed,
            heater_enabled,
            temperature,
            target,
        )
    };
    assert!(terminal_request(false, true, false, Some(55.0), Some(60)));
    assert!(terminal_request(true, false, false, Some(41.0), Some(60)));
    assert!(terminal_request(true, false, false, Some(35.01), Some(60)));
    assert!(terminal_request(true, false, false, Some(35.0), Some(60)));
    assert!(!terminal_request(true, false, false, Some(34.99), Some(60)));
    assert!(terminal_request(true, false, false, None, Some(60)));
    assert!(!terminal_request(true, false, true, Some(55.0), Some(60)));
    assert!(!terminal_request(false, false, false, Some(55.0), Some(60)));
    assert!(terminal_request(true, false, false, Some(40.01), Some(240)));
    for target in [Some(60), Some(220), Some(240), None] {
        for temperature in [40.0, 37.25, 35.01, 35.0] {
            assert!(terminal_request(
                true,
                false,
                false,
                Some(temperature),
                target
            ));
        }
        assert!(!terminal_request(true, false, false, Some(34.99), target));
        assert!(terminal_request(true, false, false, None, target));
    }
    for temperature in [55.0, 40.0, 35.01, 35.0, 34.99] {
        assert!(tuning_cooling_requested(
            false,
            false,
            true,
            ThermalTuningPhase::Cooldown,
            false,
            Some(temperature),
            Some(60),
        ));
    }
}

#[test]
fn multi_level_heating_guard_interpolates_pulse_and_output() {
    assert_eq!(guard_pulse_percent(100, HeatingFanGuardMode::Low), 0);
    assert_eq!(guard_pulse_percent(200, HeatingFanGuardMode::Low), 50);
    assert_eq!(guard_pulse_percent(80, HeatingFanGuardMode::Medium), 0);
    assert_eq!(guard_pulse_percent(150, HeatingFanGuardMode::Medium), 50);
    assert_eq!(
        interpolate_limited_fan_pwm(150),
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
    );
    assert_eq!(interpolate_limited_fan_pwm(240), 700);

    let low = fan_policy_decision_with_modes(
        150,
        0,
        true,
        false,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Low,
        (FanPolicyState::Disabled, false),
    );
    assert_eq!(low.source, FanPolicySource::HeatingGuard);
    assert!(low.command.enabled);
    assert_eq!(
        low.command.pwm_permille,
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
    );

    let medium_limited = fan_policy_decision_with_modes(
        200,
        0,
        true,
        false,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::Medium,
        (FanPolicyState::Disabled, false),
    );
    assert_eq!(
        medium_limited.command.pwm_permille,
        interpolate_limited_fan_pwm(200)
    );
    assert!(medium_limited.command.enabled);

    let high = fan_policy_decision_with_modes(
        81,
        5_000,
        true,
        false,
        PostHeatCoolingMode::Normal,
        HeatingFanGuardMode::High,
        (FanPolicyState::Disabled, false),
    );
    assert_eq!(high.state, FanPolicyState::HeatingGuardContinuous);
    assert_eq!(high.output_level, FanOutputLevel::Low);
    assert!(high.command.enabled);
}

#[test]
fn heater_enabled_uses_actual_output_for_heating_pulses() {
    let heating_below_100 =
        fan_policy_decision(41, 0, true, 32, true, FanPolicyState::Disabled, false);
    assert_eq!(heating_below_100.command, FanHardwareCommand::disabled());
    assert_eq!(heating_below_100.display_state, FanDisplayState::Auto);

    let heating_with_policy_off =
        fan_policy_decision(41, 0, true, 32, false, FanPolicyState::Disabled, false);
    assert_eq!(
        heating_with_policy_off.command,
        FanHardwareCommand::disabled()
    );
    assert_eq!(heating_with_policy_off.display_state, FanDisplayState::Off);

    let armed_but_not_outputting =
        fan_policy_decision(110, 0, true, 0, true, FanPolicyState::Disabled, false);
    assert_eq!(
        armed_but_not_outputting.command,
        FanHardwareCommand::disabled()
    );
    assert_eq!(
        armed_but_not_outputting.display_state,
        FanDisplayState::Auto
    );

    let heating_over_100 =
        fan_policy_decision(110, 0, true, 32, true, FanPolicyState::Disabled, false);
    assert!(heating_over_100.command.enabled);
    assert_eq!(
        heating_over_100.command.pwm_permille,
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
    );
    assert_eq!(heating_over_100.display_state, FanDisplayState::Run);
}

#[test]
fn heating_fan_pulses_double_the_cooling_disabled_window() {
    assert_eq!(cooling_disabled_pulse_duty_percent(110), 1);
    assert_eq!(heating_fan_pulse_duty_percent(110), 2);
    assert_eq!(cooling_disabled_pulse_duty_percent(350), 25);
    assert_eq!(heating_fan_pulse_duty_percent(350), 50);

    let heating_on = fan_policy_decision(110, 99, true, 32, true, FanPolicyState::Disabled, false);
    assert!(heating_on.command.enabled);

    let heating_off =
        fan_policy_decision(110, 100, true, 32, true, FanPolicyState::Disabled, false);
    assert!(!heating_off.command.enabled);

    let capped_on =
        fan_policy_decision(350, 2_499, true, 32, true, FanPolicyState::Disabled, false);
    assert!(capped_on.command.enabled);

    let capped_off =
        fan_policy_decision(350, 2_500, true, 32, true, FanPolicyState::Disabled, false);
    assert!(!capped_off.command.enabled);
}

#[test]
fn overtemp_threshold_uses_unrounded_temperature() {
    assert!(!is_overtemp_sample(419.9));
    assert!(is_overtemp_sample(420.0));
}

#[test]
fn tuning_temperature_retains_live_rtd_when_control_is_guarded() {
    // The captured run retained 34.54C in control while current RTD was 43.07C.
    let reading = tuning_temperature_evidence(43.07, false, true, Some(641_814), 641_920);
    assert_eq!(reading.temperature_centi_c, 4_307);
    assert!(reading.measurement_valid);
    assert!(!reading.cooldown_admission_ready);

    let unaccepted_cold = tuning_temperature_evidence(34.54, false, false, Some(641_814), 641_920);
    assert!(!unaccepted_cold.cooldown_admission_ready);
    let accepted_cold = tuning_temperature_evidence(34.54, false, false, Some(641_920), 641_920);
    assert_eq!(accepted_cold.temperature_centi_c, 3_454);
    assert!(accepted_cold.cooldown_admission_ready);
}

#[test]
fn tuning_temperature_uses_retained_control_for_guarded_raw_jump() {
    let reading =
        tuning_temperature_evidence_with_control(51.54, 43.13, false, true, Some(278_552), 278_661);
    assert_eq!(reading.temperature_centi_c, 4_313);
    assert!(reading.measurement_valid);
    assert!(!reading.measurement_accepted);
    assert!(!reading.cooldown_admission_ready);
    let stale = tuning_temperature_evidence_with_control(
        43.13,
        43.13,
        false,
        false,
        Some(278_552),
        278_661,
    );
    assert!(!stale.measurement_accepted);
    let recovered = tuning_temperature_evidence_with_control(
        43.14,
        43.14,
        false,
        false,
        Some(278_661),
        278_661,
    );
    assert!(recovered.measurement_accepted);
}

#[test]
fn tuning_temperature_faults_do_not_reuse_the_retained_display_reading() {
    for (temp_c, fault) in [(34.54, true), (f32::NAN, false), (f32::INFINITY, false)] {
        let reading = tuning_temperature_evidence(temp_c, fault, false, Some(10), 10);
        assert!(!reading.measurement_valid);
        assert!(!reading.cooldown_admission_ready);
    }
}

#[test]
fn tuning_temperature_guard_cannot_release_terminal_cooling() {
    for (live_temp_c, guarded, accepted_at_ms) in [
        (43.07, true, Some(641_814)),
        (34.54, true, Some(641_814)),
        (34.54, false, Some(641_814)),
    ] {
        let reading =
            tuning_temperature_evidence(live_temp_c, false, guarded, accepted_at_ms, 641_920);
        assert!(tuning_cooling_requested(
            true,
            false,
            false,
            ThermalTuningPhase::Canceled,
            false,
            reading.cooldown_admission_ready.then_some(live_temp_c),
            Some(60),
        ));
    }
    let reading = tuning_temperature_evidence(34.54, false, false, Some(641_920), 641_920);
    assert!(!tuning_cooling_requested(
        true,
        false,
        false,
        ThermalTuningPhase::Canceled,
        false,
        reading.cooldown_admission_ready.then_some(34.54),
        Some(60),
    ));
}

#[test]
fn raw_rtd_overtemp_bypasses_the_control_slew_guard() {
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(140.0, 0);

    assert_eq!(measurement_guard.observe(430.0, 1_000), None);
    assert!(measurement_guard.guarded);
    assert_eq!(overtemp_fault_from_control_temperature(140.0), None);
    assert_eq!(
        overtemp_fault_from_control_temperature(430.0),
        Some(HeaterFaultReason::OverTemp)
    );
}

#[test]
fn rtd_control_guard_recovers_after_a_stable_persistent_temperature_shift() {
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(140.0, 0);

    assert_eq!(measurement_guard.observe(260.0, 1_000), None);
    assert!(measurement_guard.guarded);
    assert_eq!(measurement_guard.observe(260.0, 1_700), None);
    assert!(measurement_guard.guarded);
    assert_eq!(measurement_guard.observe(260.0, 1_800), Some(260.0));
    assert!(!measurement_guard.guarded);
    assert_eq!(measurement_guard.observe(145.0, 5_000), None);
    assert!(measurement_guard.guarded);
    assert_eq!(measurement_guard.observe(145.0, 5_800), Some(145.0));
    assert!(!measurement_guard.guarded);
}

#[test]
fn rtd_control_guard_accepts_small_in_band_unpowered_noise() {
    let mut guard = RtdControlMeasurementGuard::default();
    // Fixed 13.5V A trial 4 on 86353a39: a roughly 0.5C raw step
    // inside the stable band was rejected at the 100ms control cadence.
    guard.reseed(60.86, 690_489);
    assert_eq!(
        guard.observe_with_heater_duty(61.36, 690_589, 0),
        Some(61.36)
    );
    assert!(!guard.guarded);
    assert_eq!(
        guard.observe_with_heater_duty(60.92, 690_689, 0),
        Some(60.92)
    );
    assert_eq!(guard.observe_with_heater_duty(61.62, 690_789, 0), None);
    assert!(guard.guarded);
    assert_eq!(guard.observe_with_heater_duty(64.0, 690_789, 0), None);
    assert!(guard.guarded);
    assert_eq!(guard.last_accepted_temp_c, Some(60.92));
    assert_eq!(guard.observe_with_heater_duty(64.0, 694_789, 0), None);
    assert!(guard.guarded);
}

#[test]
fn rtd_control_guard_does_not_reseed_from_fast_unpowered_rise() {
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(80.0, 0);

    assert_eq!(
        measurement_guard.observe_with_heater_duty(85.0, 300, 0),
        None
    );
    assert!(measurement_guard.guarded);
    assert_eq!(measurement_guard.last_accepted_temp_c, Some(80.0));

    assert_eq!(
        measurement_guard.observe_with_heater_duty(85.0, 1_500, 0),
        None
    );
    assert!(measurement_guard.guarded);
    assert_eq!(measurement_guard.last_accepted_temp_c, Some(80.0));

    assert_eq!(
        measurement_guard.observe_with_heater_duty(80.5, 1_800, 0),
        Some(80.5)
    );
    assert!(!measurement_guard.guarded);
}

#[test]
fn rtd_control_guard_remains_active_after_heater_disabled_interval() {
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(143.7, 0);
    let mut measurement_guarded = true;

    preserve_rtd_control_guard_when_heater_disabled(false, &mut measurement_guarded);

    assert_eq!(measurement_guard.last_accepted_temp_c, Some(143.7));
    assert!(!measurement_guarded);
    assert_eq!(measurement_guard.observe(430.0, 1_000), None);
    assert!(measurement_guard.guarded);
}

#[test]
fn rtd_pps_transition_guard_accepts_immediately_and_reseeds_on_request_change() {
    let mut guard = RtdPpsTransitionGuard::new(21_000);
    assert_eq!(guard.observe(21_000, 0), (true, false));
    assert_eq!(guard.observe(20_500, 40), (false, true));
    assert_eq!(guard.observe(20_000, 240), (false, true));
    assert_eq!(guard.observe(20_000, 520), (false, false));
    assert_eq!(guard.observe(20_000, 540), (true, true));
    assert_eq!(guard.observe(20_000, 580), (true, false));
}

#[test]
fn pps_reseed_preserves_the_real_age_of_the_last_temperature_for_slew_checks() {
    for stable_temperature_c in [54.47, 73.28] {
        let mut transition = RtdPpsTransitionGuard::new(21_000);
        let mut controller = HeaterController::new();
        controller.reseed_measurement(52.30);
        controller.duty_percent = 5;
        let mut measurement = RtdControlMeasurementGuard::default();
        measurement.reseed(52.30, 7_321);
        assert_eq!(
            accept_rtd_control_sample_after_pps_transition(
                &mut transition,
                &mut controller,
                &mut measurement,
                52.30,
                20_500,
                8_797,
                stable_temperature_c,
            ),
            None
        );
        assert_eq!(measurement.last_accepted_at_ms, Some(7_321));
        let accepted = accept_rtd_control_sample_after_pps_transition(
            &mut transition,
            &mut controller,
            &mut measurement,
            52.30,
            20_500,
            9_097,
            stable_temperature_c,
        );
        if stable_temperature_c < 60.0 {
            assert_eq!(accepted, Some(stable_temperature_c));
            assert_eq!(measurement.last_accepted_at_ms, Some(9_097));
        } else {
            assert_eq!(accepted, None);
            assert!(measurement.guarded);
            assert_eq!(measurement.last_accepted_at_ms, Some(7_321));
        }
    }
}

#[test]
fn rtd_pps_transition_rechecks_first_stable_sample_from_last_trusted_temperature() {
    let mut guard = RtdPpsTransitionGuard::new(18_000);
    let mut controller = HeaterController::new();
    controller.reseed_measurement(140.0);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(140.0, 0);

    assert_eq!(
        accept_rtd_control_sample_after_pps_transition(
            &mut guard,
            &mut controller,
            &mut measurement_guard,
            140.0,
            17_500,
            50,
            140.0,
        ),
        None
    );
    assert_eq!(
        accept_rtd_control_sample_after_pps_transition(
            &mut guard,
            &mut controller,
            &mut measurement_guard,
            140.0,
            17_500,
            350,
            143.0,
        ),
        None
    );
    assert!(measurement_guard.guarded);
    assert_eq!(controller.filtered_temp_c, Some(140.0));

    assert_eq!(
        accept_rtd_control_sample_after_pps_transition(
            &mut guard,
            &mut controller,
            &mut measurement_guard,
            140.0,
            17_500,
            1_100,
            143.0,
        ),
        Some(143.0)
    );
    assert!(!measurement_guard.guarded);
    assert_eq!(controller.filtered_temp_c, Some(143.0));
}

#[test]
fn pps_transition_reseed_keeps_last_trusted_control_temperature() {
    let mut guard = RtdPpsTransitionGuard::new(18_000);
    let mut controller = HeaterController::new();
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    let last_control_temp_c = 41.39;

    let control_temp_c = accept_rtd_control_sample_after_pps_transition(
        &mut guard,
        &mut controller,
        &mut measurement_guard,
        last_control_temp_c,
        14_000,
        0,
        73.74,
    );

    assert_eq!(control_temp_c, None);
    assert_eq!(controller.filtered_temp_c, Some(last_control_temp_c));
    assert_eq!(
        controller.previous_filtered_temp_c,
        Some(last_control_temp_c)
    );
    assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
    assert_eq!(
        controller.previous_measured_temp_c,
        Some(last_control_temp_c)
    );
    assert!(!measurement_guard.guarded);
}

#[test]
fn rtd_guard_accepts_calibration_heating_with_idle_production_controller() {
    let mut ui = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut control_temp = 30.0;
    let mut control_whole = 30;
    let mut display_temp = 30.0;
    let mut display_whole = 30;
    let mut transition = RtdPpsTransitionGuard::new(21_000);
    let mut measurement = RtdControlMeasurementGuard::default();
    measurement.reseed(30.0, 0);
    let mut guarded = false;
    let mut controller = HeaterController::new();
    // Calibration owns the applied 100% MOS output; production remains idle.
    assert_eq!(controller.duty_percent, 0);
    apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui,
            latest_display_temp_c: &mut display_temp,
            latest_display_temp_i16: &mut display_whole,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut control_temp,
            latest_control_temp_i16: &mut control_whole,
            transition_guard: &mut transition,
            measurement_guard: &mut measurement,
            control_measurement_guarded: &mut guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 100,
        },
        21_000,
        100,
        32.0,
    );
    assert_eq!(control_temp, 32.0);
    assert!(!guarded);
    assert_eq!(controller.duty_percent, 0);
}

#[test]
fn rtd_control_guard_rejects_impossible_jump_without_hiding_raw_temperature() {
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut latest_control_temp_c = 140.0;
    let mut latest_control_temp_i16 = 140;
    let mut latest_display_temp_c = 140.0;
    let mut latest_display_temp_i16 = 140;
    let mut transition_guard = RtdPpsTransitionGuard::new(12_000);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    measurement_guard.reseed(140.0, 0);
    let mut control_measurement_guarded = false;
    let mut controller = HeaterController::new();

    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_control_temp_c,
            latest_control_temp_i16: &mut latest_control_temp_i16,
            transition_guard: &mut transition_guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        12_000,
        50,
        310.0,
    ));

    assert_eq!(latest_display_temp_c, 310.0);
    assert_eq!(ui_state.current_temp_c, 310);
    assert_eq!(latest_control_temp_c, 140.0);
    assert_eq!(latest_control_temp_i16, 140);
    assert!(control_measurement_guarded);

    measurement_guard.clear();
    assert_eq!(measurement_guard.observe(31.0, 100), Some(31.0));
    assert!(!measurement_guard.guarded);
}

#[test]
fn rtd_retry_triggers_on_request_change_even_before_vin_moves() {
    assert!(should_retry_rtd_sample_after_power_step(
        17_500, 18_000, 1_170, 1_170
    ));
}

#[test]
fn rtd_retry_triggers_on_large_vin_step_without_request_change() {
    assert!(should_retry_rtd_sample_after_power_step(
        18_000, 18_000, 1_170, 1_230
    ));
    assert!(!should_retry_rtd_sample_after_power_step(
        18_000, 18_000, 1_170, 1_200
    ));
}

#[test]
fn heater_controller_reseed_clears_measurement_slope_without_changing_phase() {
    let mut controller = HeaterController::new();
    controller.phase = HeaterControlPhase::Approach;
    controller.filtered_temp_c = Some(40.0);
    controller.previous_filtered_temp_c = Some(39.0);
    controller.filtered_slope_c_per_profile_tick = 8.0;
    controller.previous_measured_temp_c = Some(41.0);

    controller.reseed_measurement(52.0);

    assert_eq!(controller.phase, HeaterControlPhase::Approach);
    assert_eq!(controller.filtered_temp_c, Some(52.0));
    assert_eq!(controller.previous_filtered_temp_c, Some(52.0));
    assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
    assert_eq!(controller.previous_measured_temp_c, Some(52.0));
}

#[test]
fn pps_measurement_reseed_does_not_erase_observed_approach_heat_in_flight() {
    for bank in [ThermalProfileBank::Pps3a, ThermalProfileBank::Pps5a] {
        let kind = ProductionThermalController::Profile(bank);
        let mut controller = HeaterController::new();
        controller.production_controller = Some(kind);
        controller.last_target_temp_c = 60;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(54.32);
        controller.previous_measured_temp_c = Some(54.32);
        controller.filtered_slope_c_per_profile_tick = 2.0;
        controller.profile_last_control_at_ms = Some(3_000);
        let mut profile = production_test_profile(54);
        let point = profile.points[0].as_mut().unwrap();
        point.target_temp_c = 60;
        point.brake_distance_centi_c = 1_238;
        point.approach_lead_ticks = 0;
        let mut input = production_test_input(54.32);
        input.target_temp_c = 60;
        input.now_ms = 3_050;
        input.ambient_temp_c = 33.08;
        input.max_power_mw = 100_000.0;
        input.model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 137.1277,
            radiation_mw_per_k4: 5.4832304e-7,
            thermal_capacity_mj_per_c: 22_173.453,
            transport_delay_ms: 2_650,
        };
        controller.natural_cooling_input = Some(input);
        let mut uninterrupted = controller;
        assert_eq!(
            uninterrupted
                .update_production_at(kind, input, Some(profile))
                .0
                .duty_percent,
            5
        );
        let mut transition = RtdPpsTransitionGuard::new(19_000);
        let mut measurement = RtdControlMeasurementGuard::default();
        measurement.reseed(54.32, 3_000);
        assert_eq!(
            accept_rtd_control_sample_after_pps_transition(
                &mut transition,
                &mut controller,
                &mut measurement,
                54.32,
                18_500,
                3_050,
                53.0,
            ),
            None,
            "a power-step ADC offset remains rejected"
        );
        let snapshot = controller
            .update_production_at(kind, input, Some(profile))
            .0;
        assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
        assert!(snapshot.filtered_slope_c_per_s > 0.0);
        assert_eq!(
            snapshot.duty_percent, 5,
            "bank={bank:?}: a PPS step is not physical deceleration"
        );
    }
}

#[test]
fn pps_reseed_preserves_only_valid_rising_model_approach_velocity() {
    let mut controller = HeaterController::new();
    controller.heater_was_enabled = true;
    controller.phase = HeaterControlPhase::Approach;
    controller.natural_cooling_input = Some(production_test_input(54.32));
    for slope in [-1.0, 0.0, f32::NAN, f32::INFINITY] {
        controller.filtered_slope_c_per_profile_tick = slope;
        controller.reseed_measurement_after_pps_step(54.32);
        assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
    }
    for phase in [HeaterControlPhase::Warmup, HeaterControlPhase::Hold] {
        controller.phase = phase;
        controller.filtered_slope_c_per_profile_tick = 2.0;
        controller.reseed_measurement_after_pps_step(54.32);
        assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
    }
    controller.phase = HeaterControlPhase::Approach;
    controller.natural_cooling_input = None;
    controller.filtered_slope_c_per_profile_tick = 2.0;
    controller.reseed_measurement_after_pps_step(54.32);
    assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
}

#[test]
fn rtd_fractional_millivolts_preserve_oversampled_temperature_resolution() {
    let lower_temp = pt1000_temperature_c_from_resistance(
        rtd_resistance_ohms_from_fractional_mv(900.0).unwrap(),
    );
    let midpoint_temp = pt1000_temperature_c_from_resistance(
        rtd_resistance_ohms_from_fractional_mv(900.5).unwrap(),
    );
    let upper_temp = pt1000_temperature_c_from_resistance(
        rtd_resistance_ohms_from_fractional_mv(901.0).unwrap(),
    );

    assert!(lower_temp < midpoint_temp);
    assert!(midpoint_temp < upper_temp);
    assert!((midpoint_temp - ((lower_temp + upper_temp) * 0.5)).abs() < 0.01);
}

fn assumed_offset_test_batch(temp_c: f32) -> RtdAdcBatch {
    let resistance = pt1000_resistance_ohms_at(temp_c);
    let mv =
        f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance / (RTD_REFERENCE_RESISTOR_OHMS + resistance);
    RtdAdcBatch {
        mean_mv: mv,
        min_mv: mv.floor() as u16,
        max_mv: mv.ceil() as u16,
        mean_raw_code: 2_280,
        min_raw_code: 2_275,
        max_raw_code: 2_285,
    }
}

#[test]
fn projected_rtd_temperature_matches_live_adc_assumption() {
    let memory = MemoryConfig::default();
    for raw_mv in [1_044, 1_210, 1_420] {
        let live = rtd_sample_from_batch(
            &memory,
            RtdAdcBatch {
                mean_mv: f32::from(raw_mv),
                min_mv: raw_mv,
                max_mv: raw_mv,
                ..assumed_offset_test_batch(30.0)
            },
            RTD_ASSUMED_ADC_OFFSET_UV,
        );
        let RtdSample::Valid(live) = live else {
            panic!("valid RTD voltage required");
        };
        let projected = projected_rtd_temperature_c(&memory, raw_mv).unwrap();
        assert!(
            (projected - live.temp_c).abs() < 0.001,
            "raw={raw_mv}: live={} projected={projected}",
            live.temp_c
        );
        let sample = ThermalPlantTransientSample {
            elapsed_ticks: 1,
            raw_rtd_adc_mv: raw_mv,
            heater_voltage_125mv: 168,
            duty_percent: 100,
        };
        let page = thermal_plant_trace_page(&memory, &[sample], 1, 0, 0);
        assert_eq!(
            page.points[0].temperature_centi_c,
            round_to_i16(live.temp_c * 100.0)
        );
    }
}

#[test]
fn assumed_rtd_offset_is_separate_from_raw_diagnostics_and_user_calibration() {
    let config = MemoryConfig::default();
    let original = config.clone();
    let batch = assumed_offset_test_batch(35.5);
    let RtdSample::Valid(base) = rtd_sample_from_batch(&config, batch, 0) else {
        panic!("default sample must be valid");
    };
    let RtdSample::Valid(adjusted) = rtd_sample_from_batch(&config, batch, -13_500) else {
        panic!("assumed-offset sample must be valid");
    };
    assert!((base.temp_c - 35.5).abs() < 0.02);
    assert!((adjusted.temp_c - 30.0).abs() < 0.03);
    assert_eq!(adjusted.raw_adc_mv, base.raw_adc_mv);
    assert_eq!(adjusted.raw_adc_min_mv, base.raw_adc_min_mv);
    assert_eq!(adjusted.raw_adc_max_mv, base.raw_adc_max_mv);
    assert_eq!(config, original);
    assert_eq!(
        correct_adc_mv(&config.adc_calibration, AdcCalibrationChannel::Vin, 1_234),
        1_234
    );
}

#[test]
fn assumed_rtd_offset_yields_to_user_points_or_an_active_fit() {
    let mut config = MemoryConfig::default();
    config.adc_calibration.rtd.slots.a.offset_mv = 2.0;
    assert_eq!(rtd_assumed_adc_offset_uv(&config, -13_500), 0);
    let batch = assumed_offset_test_batch(30.0);
    assert_eq!(
        rtd_sample_from_batch(&config, batch, -13_500),
        rtd_sample_from_batch(&config, batch, 0)
    );
    config.adc_calibration.rtd.slots.a.offset_mv = 0.0;
    config.adc_calibration.rtd.samples[0] = Some(AdcCalibrationSample {
        observed_mv: 1_030,
        expected_mv: 1_031,
        reference_temp_deci_c: None,
        target_adc_mv: None,
        reference_vin_mv: None,
    });
    assert_eq!(rtd_assumed_adc_offset_uv(&config, -13_500), 0);
}

#[test]
fn assumed_rtd_offset_cannot_hide_original_sensor_or_overtemp_faults() {
    let config = MemoryConfig::default();
    for (mv, reason) in [
        (
            f32::from(RTD_SHORT_FAULT_MAX_MV),
            HeaterFaultReason::SensorShort,
        ),
        (
            f32::from(RTD_OPEN_FAULT_MIN_MV),
            HeaterFaultReason::SensorOpen,
        ),
    ] {
        let batch = RtdAdcBatch {
            mean_mv: mv,
            ..assumed_offset_test_batch(30.0)
        };
        for offset in [-20_000, 20_000] {
            assert_eq!(
                rtd_sample_from_batch(&config, batch, offset),
                RtdSample::Fault {
                    adc_mv: Some(mv.round() as u16),
                    reason
                }
            );
        }
    }
    let batch = assumed_offset_test_batch(421.0);
    assert_eq!(
        rtd_sample_from_batch(&config, batch, -20_000),
        RtdSample::Fault {
            adc_mv: Some(batch.mean_mv.round() as u16),
            reason: HeaterFaultReason::OverTemp,
        }
    );
}

#[test]
fn rtd_uses_nominal_regulator_feedback_divider_supply() {
    let expected_temperature_c = 31.0;
    let resistance_ohms = pt1000_resistance_ohms_at(expected_temperature_c);
    let sense_mv = 3_328.0 * resistance_ohms / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms);
    let reported_temperature_c = pt1000_temperature_c_from_resistance(
        rtd_resistance_ohms_from_fractional_mv(sense_mv).unwrap(),
    );

    assert!(
        (reported_temperature_c - expected_temperature_c).abs() < 0.05,
        "expected {expected_temperature_c:.2}C, got {reported_temperature_c:.2}C"
    );
}

#[test]
fn rtd_oversampling_accepts_partial_batch_with_enough_valid_conversions() {
    let valid_samples = RTD_MIN_VALID_SAMPLE_COUNT;
    let sum_mv = (900 * valid_samples) + (valid_samples / 2);
    let mean_mv = rtd_fractional_mean_mv(sum_mv as u32, valid_samples).unwrap();

    assert!((mean_mv - 900.5).abs() < 0.001);
}

#[test]
fn rtd_oversampling_rejects_batch_below_valid_conversion_threshold() {
    assert_eq!(
        rtd_fractional_mean_mv(
            900 * RTD_MIN_VALID_SAMPLE_COUNT as u32,
            RTD_MIN_VALID_SAMPLE_COUNT - 1
        ),
        None
    );
}

#[test]
fn rtd_oversampling_discards_settle_prefix_before_meaningful_average() {
    let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
    samples.extend(std::iter::repeat_n(Some(900_u16), RTD_SAMPLE_COUNT));
    let mut iter = samples.into_iter();
    let mean_mv = oversampled_fractional_mean_mv_with_discard(
        RTD_SAMPLE_COUNT + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || iter.next().flatten(),
    )
    .unwrap();

    assert!((mean_mv - 900.0).abs() < 0.001);
}

#[test]
fn rtd_oversampling_reports_kept_batch_extrema() {
    let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
    let mut kept = vec![Some(900_u16); RTD_SAMPLE_COUNT];
    kept[3] = Some(899);
    kept[17] = Some(902);
    samples.extend(kept);
    let mut iter = samples.into_iter();
    let batch = oversampled_rtd_batch_with_discard(
        RTD_SAMPLE_COUNT + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || iter.next().flatten(),
    )
    .expect("RTD batch has enough valid conversions");

    assert_eq!(batch.min_mv, 899);
    assert_eq!(batch.max_mv, 902);
    assert_eq!(batch.max_mv.saturating_sub(batch.min_mv), 3);
}

#[test]
fn rtd_phase_sampling_covers_the_entire_pwm_period_without_hiding_extrema() {
    let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
    for phase in 0..RTD_SAMPLE_PWM_PHASE_COUNT {
        let phase_mv = if phase % 2 == 0 { 900 } else { 920 };
        samples.extend(std::iter::repeat_n(
            Some(phase_mv),
            RTD_SAMPLE_COUNT / RTD_SAMPLE_PWM_PHASE_COUNT,
        ));
    }
    let mut iter = samples.into_iter();
    let mut phase_waits = 0_usize;
    let batch = phase_averaged_rtd_batch_with_discard(
        RTD_SAMPLE_COUNT,
        RTD_SAMPLE_PWM_PHASE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || {
            iter.next().flatten().map(|value| AdcConvertedSample {
                raw_code: value.saturating_mul(2),
                calibrated_mv: value,
            })
        },
        || phase_waits = phase_waits.saturating_add(1),
    )
    .expect("RTD phase batch has enough valid conversions");

    assert!((batch.mean_mv - 910.0).abs() < 0.001);
    assert_eq!(batch.min_mv, 900);
    assert_eq!(batch.max_mv, 920);
    assert_eq!(batch.mean_raw_code, 1_820);
    assert_eq!(batch.min_raw_code, 1_800);
    assert_eq!(batch.max_raw_code, 1_840);
    assert_eq!(phase_waits, RTD_SAMPLE_PWM_PHASE_COUNT - 1);
}

#[test]
fn adc_samples_always_mask_status_bits_to_twelve_bit_code() {
    assert_eq!(mask_adc1_raw_code(0xfabc), 0x0abc);
    assert_eq!(mask_adc1_raw_code(0x0fff), 0x0fff);
}

#[test]
fn rtd_phase_sampling_rejects_an_invalid_phase_plan() {
    assert_eq!(
        phase_averaged_rtd_batch_with_discard(
            79,
            10,
            0,
            || {
                Some(AdcConvertedSample {
                    raw_code: 1_800,
                    calibrated_mv: 900,
                })
            },
            || {}
        ),
        None
    );
}

#[test]
fn rtd_oversampling_ignores_faulty_prefix_only_after_valid_tail_threshold() {
    let kept_valid_samples = RTD_MIN_VALID_SAMPLE_COUNT;
    let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
    samples.extend(std::iter::repeat_n(Some(900_u16), kept_valid_samples));
    let mut iter = samples.into_iter();
    let mean_mv = oversampled_fractional_mean_mv_with_discard(
        kept_valid_samples + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || iter.next().flatten(),
    )
    .unwrap();

    assert!((mean_mv - 900.0).abs() < 0.001);
}

#[test]
fn default_adc_calibration_keeps_fractional_mean() {
    let config = MemoryConfig::default();
    let corrected = correct_adc_fractional_mv(&config, AdcCalibrationChannel::Rtd, 900.375);

    assert!((corrected - 900.375).abs() < 0.001);
}

#[test]
fn cooling_disabled_policy_uses_pulse_window_and_safety_steps() {
    assert_eq!(cooling_disabled_pulse_duty_percent(100), 0);
    assert_eq!(cooling_disabled_pulse_duty_percent(110), 1);
    assert_eq!(cooling_disabled_pulse_duty_percent(350), 25);

    let pulse_on = fan_policy_decision(110, 0, false, 0, false, FanPolicyState::Disabled, false);
    assert!(pulse_on.command.enabled);
    assert_eq!(pulse_on.display_state, FanDisplayState::Off);
    assert_eq!(
        pulse_on.command.pwm_permille,
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
    );

    let pulse_off = fan_policy_decision(110, 200, false, 0, false, FanPolicyState::Disabled, false);
    assert!(!pulse_off.command.enabled);

    let half = fan_policy_decision(351, 0, false, 0, false, FanPolicyState::Disabled, false);
    assert_eq!(
        half.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf)
    );
    assert_eq!(half.display_state, FanDisplayState::Off);

    let full = fan_policy_decision(361, 0, false, 0, false, FanPolicyState::Disabled, false);
    assert_eq!(
        full.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
    assert_eq!(full.display_state, FanDisplayState::Off);
}

#[test]
fn rtd_sensor_fault_keeps_existing_policy_state() {
    let auto = fan_policy_decision(0, 0, false, 0, true, FanPolicyState::ActiveCooling, true);
    assert_eq!(
        auto.command,
        FanHardwareCommand::from_profile(FanVoltageProfile::Full)
    );
    assert_eq!(auto.display_state, FanDisplayState::Run);

    let pulse_on = fan_policy_decision(
        0,
        0,
        false,
        0,
        false,
        FanPolicyState::CoolingDisabledPulse { duty_percent: 10 },
        true,
    );
    assert!(pulse_on.command.enabled);
    assert_eq!(pulse_on.display_state, FanDisplayState::Off);

    let pulse_off = fan_policy_decision(
        0,
        1_500,
        false,
        0,
        false,
        FanPolicyState::CoolingDisabledPulse { duty_percent: 10 },
        true,
    );
    assert!(!pulse_off.command.enabled);
    assert_eq!(pulse_off.display_state, FanDisplayState::Off);
}

#[test]
fn cooling_disabled_lock_requires_cooldown_after_manual_rearm() {
    let (latched, armed, just_latched) =
        reconcile_cooling_disabled_lock(false, 351, false, false, true);
    assert_eq!((latched, armed, just_latched), (true, false, true));

    let (manual_override_latched, manual_override_armed, manual_override_just_latched) =
        reconcile_cooling_disabled_lock(false, 351, false, false, false);
    assert_eq!(
        (
            manual_override_latched,
            manual_override_armed,
            manual_override_just_latched
        ),
        (false, false, false)
    );

    let (rearmed_latched, rearmed_armed, rearmed_just_latched) = reconcile_cooling_disabled_lock(
        false,
        350,
        false,
        manual_override_latched,
        manual_override_armed,
    );
    assert_eq!(
        (rearmed_latched, rearmed_armed, rearmed_just_latched),
        (false, true, false)
    );

    let (latched_again, armed_again, just_latched_again) =
        reconcile_cooling_disabled_lock(false, 351, false, rearmed_latched, rearmed_armed);
    assert_eq!(
        (latched_again, armed_again, just_latched_again),
        (true, false, true)
    );
}

#[test]
fn rtd_fault_clears_cached_runtime_temperature() {
    let mut latest_temp_c = 378.4;
    let mut latest_temp_i16 = 378;

    clear_runtime_temperature(&mut latest_temp_c, &mut latest_temp_i16);
    assert_eq!(latest_temp_c, 0.0);
    assert_eq!(latest_temp_i16, 0);
}

#[test]
fn valid_rtd_measurement_promotes_startup_dashboard_to_ready() {
    let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
    let mut latest_temp_c = 0.0;
    let mut latest_temp_i16 = 0;
    let mut latest_display_temp_c = 0.0;
    let mut latest_display_temp_i16 = 0;
    let mut guard = RtdPpsTransitionGuard::new(12_000);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    let mut control_measurement_guarded = false;
    let mut controller = HeaterController::new();

    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        12_000,
        100,
        41.39,
    ));

    assert_eq!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::Ready
    );
    assert_eq!(ui_state.current_temp_deci_c, 414);
}

#[test]
fn valid_rtd_measurement_does_not_bypass_eeprom_restore_lock() {
    let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
    ui_state.set_dashboard_presentation(
        flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore,
    );
    ui_state.eeprom_data_incompatible = true;
    let mut latest_display_temp_c = 0.0;
    let mut latest_display_temp_i16 = 0;

    assert!(update_runtime_display_temperature(
        &mut ui_state,
        &mut latest_display_temp_c,
        &mut latest_display_temp_i16,
        41.39,
    ));
    assert_eq!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore
    );
    assert!(ui_state.persistence_locked());
}

#[test]
fn runtime_sensor_fault_retains_last_valid_dashboard_temperature() {
    let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
    let mut latest_display_temp_c = 0.0;
    let mut latest_display_temp_i16 = 0;

    assert!(update_runtime_display_temperature(
        &mut ui_state,
        &mut latest_display_temp_c,
        &mut latest_display_temp_i16,
        85.4,
    ));
    assert_eq!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::Ready
    );
    assert_eq!(ui_state.current_temp_deci_c, 854);

    ui_state.heater_lock_reason = Some(HeaterLockReason::SensorFault);
    ui_state.dashboard_warning_visible = true;
    assert!(!retain_runtime_display_temperature(
        &mut ui_state,
        &mut latest_display_temp_c,
        &mut latest_display_temp_i16,
    ));
    assert_eq!(ui_state.current_temp_deci_c, 854);
    assert_eq!(latest_display_temp_i16, 85);
}

#[test]
fn valid_rtd_measurement_updates_display_and_control_on_request_change() {
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut latest_temp_c = 41.39;
    let mut latest_temp_i16 = 41;
    let mut latest_display_temp_c = latest_temp_c;
    let mut latest_display_temp_i16 = latest_temp_i16;
    let mut guard = RtdPpsTransitionGuard::new(12_000);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    let mut control_measurement_guarded = false;
    let mut controller = HeaterController::new();

    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        15_000,
        100,
        73.74,
    ));

    assert_eq!(latest_temp_c, 41.39);
    assert_eq!(latest_temp_i16, 41);
    assert_eq!(latest_display_temp_c, 73.74);
    assert_eq!(latest_display_temp_i16, 74);
    assert_eq!(ui_state.current_temp_c, 74);
    assert_eq!(ui_state.current_temp_deci_c, 737);
}

#[test]
fn five_amp_power_step_retry_continues_through_runtime_sampling_pipeline() {
    let retry_sample = RtdSample::Valid(RtdMeasurement {
        raw_adc_code: 1_500,
        raw_adc_mv: 983,
        raw_adc_min_mv: 982,
        raw_adc_max_mv: 984,
        adc_mv: 983,
        resistance_ohms: 1_118.0,
        temp_c: 30.73,
        current_temp_c: 31,
    });
    let RtdSample::Valid(measurement) = retry_sample else {
        panic!("5A warmup retry must remain a valid RTD sample");
    };
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut latest_temp_c = 25.37;
    let mut latest_temp_i16 = 25;
    let mut latest_display_temp_c = latest_temp_c;
    let mut latest_display_temp_i16 = latest_temp_i16;
    let mut guard = RtdPpsTransitionGuard::new(19_500);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    let mut control_measurement_guarded = false;
    let mut controller = HeaterController::new();

    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        20_000,
        100,
        measurement.temp_c,
    ));

    assert_eq!(latest_display_temp_c, 30.73);
    assert_eq!(latest_display_temp_i16, 31);
    assert_eq!(ui_state.current_temp_c, 31);
    assert_eq!(ui_state.current_temp_deci_c, 307);
    assert_eq!(latest_temp_c, 25.37);
    assert_eq!(latest_temp_i16, 25);
    assert!(
        control_measurement_guarded,
        "PPS sample wait must be observable"
    );
    assert_eq!(controller.fault_latched(), None);
}

#[test]
fn measurement_fault_samples_remain_explicit_hard_faults() {
    for reason in [
        HeaterFaultReason::SensorOpen,
        HeaterFaultReason::SensorShort,
        HeaterFaultReason::AdcReadFailed,
    ] {
        let sample = RtdSample::Fault {
            adc_mv: None,
            reason,
        };
        assert!(matches!(
            sample,
            RtdSample::Fault {
                reason: observed,
                ..
            } if observed == reason
        ));
    }
}

#[test]
fn valid_rtd_measurement_updates_control_after_request_stabilizes() {
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let mut latest_temp_c = 41.39;
    let mut latest_temp_i16 = 41;
    let mut latest_display_temp_c = latest_temp_c;
    let mut latest_display_temp_i16 = latest_temp_i16;
    let mut guard = RtdPpsTransitionGuard::new(12_000);
    let mut measurement_guard = RtdControlMeasurementGuard::default();
    let mut control_measurement_guarded = false;
    let mut controller = HeaterController::new();

    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        15_000,
        100,
        73.74,
    ));
    assert!(apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        15_000,
        700,
        42.0,
    ));

    // The first accepted sample after the PPS transition is still
    // observed at zero physical duty. Repeat after its unpowered-slew
    // guard interval before requiring the control temperature to move.
    let _ = apply_valid_rtd_measurement(
        RuntimeDisplayTemperatureState {
            ui_state: &mut ui_state,
            latest_display_temp_c: &mut latest_display_temp_c,
            latest_display_temp_i16: &mut latest_display_temp_i16,
        },
        RuntimeControlTemperatureState {
            latest_control_temp_c: &mut latest_temp_c,
            latest_control_temp_i16: &mut latest_temp_i16,
            transition_guard: &mut guard,
            measurement_guard: &mut measurement_guard,
            control_measurement_guarded: &mut control_measurement_guarded,
            heater_controller: &mut controller,
            applied_heater_duty_percent: 0,
        },
        15_000,
        1_000,
        42.0,
    );
    assert_eq!(latest_temp_c, 42.0);
    assert_eq!(latest_temp_i16, 42);
    assert_eq!(latest_display_temp_c, 42.0);
    assert_eq!(latest_display_temp_i16, 42);
    assert_eq!(ui_state.current_temp_c, 42);
    assert_eq!(ui_state.current_temp_deci_c, 420);
}

#[test]
fn pd_status_log_key_ignores_current_limit_churn() {
    let first = PdStatusObservation {
        status_raw: 0x81,
        status: Status {
            bc_active: false,
            qc2_active: false,
            qc3_active: false,
            pd_active: true,
            epr_active: false,
            epr_exist: false,
            avs_exist: false,
        },
        current_raw: 0x10,
        current_ma: 800,
        contract_voltage_mv: None,
        contract: Contract::none(),
    };
    let second = PdStatusObservation {
        current_raw: 0x2a,
        current_ma: 2_100,
        ..first
    };

    assert_eq!(
        pd_status_log_key(Some(first)),
        pd_status_log_key(Some(second))
    );
}

#[test]
fn fusb302b_fixed_contract_status_is_explicit_and_blocks_calibration() {
    let contract = Contract::observed(ContractKind::Fixed, 20_000, 5_000);
    let observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    };
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            pd_controller: ControllerKind::Fusb302b,
            last_pd_observation: Some(observation),
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.pd_controller.as_str(), "fusb302b");
    assert_eq!(status.pd_contract_kind.as_str(), "fixed");
    assert_eq!(status.pd_contract_current_ma, 5_000);
    assert_eq!(status.pd_contract_power_mw, 100_000);
    assert!(status.pd_performance_guaranteed);
    assert_eq!(status.pd_degraded_reason, None);
    assert!(!pd_contract_allows_calibration(
        ControllerKind::Fusb302b,
        Some(observation)
    ));

    let low_voltage = PdStatusObservation {
        contract: Contract::observed(ContractKind::Fixed, 15_000, 3_000),
        contract_voltage_mv: Some(15_000),
        current_ma: 3_000,
        ..observation
    };
    assert!(!pd_contract_allows_calibration(
        ControllerKind::Fusb302b,
        Some(low_voltage)
    ));

    let fallback = HeaterPowerBackend::FixedPdPwmFallback {
        reason: HeaterPowerBackendReason::CapabilityReadFailed,
        fixed_request_confirmed: true,
        fixed_request: ch224q::VoltageRequest::V20,
        terminal_fixed_pd_disarmed: false,
    };
    assert_eq!(
        effective_pd_contract_mv(&ManualPpsState::default(), Some(low_voltage), fallback),
        15_000
    );
}

#[test]
fn supply_scan_admission_uses_capability_instead_of_a_prior_twenty_volt_contract() {
    let capable =
        ManualPpsState::from_fusb302b_capabilities(Some(ch224q::AdjustablePowerCapabilities {
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }));
    assert!(pd_allows_calibration_job_start(
        ControllerKind::Fusb302b,
        None,
        Some(CalibrationJobKindWire::ThermalPlantAuto),
        &capable,
    ));
    assert!(!pd_allows_calibration_job_start(
        ControllerKind::Fusb302b,
        None,
        Some(CalibrationJobKindWire::VinAdcAuto),
        &capable,
    ));
    assert!(!pd_allows_calibration_job_start(
        ControllerKind::Fusb302b,
        None,
        Some(CalibrationJobKindWire::ThermalPlantAuto),
        &ManualPpsState::default(),
    ));
    assert!(!pd_allows_calibration_job_start(
        ControllerKind::Ch224q,
        None,
        Some(CalibrationJobKindWire::ThermalPlantAuto),
        &capable,
    ));
}

#[test]
fn fusb302b_pps_contract_enables_calibration_and_keeps_the_absolute_guard() {
    let contract = Contract::observed(ContractKind::Pps, 20_000, 5_000);
    let observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    };
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            pd_controller: ControllerKind::Fusb302b,
            last_pd_observation: Some(observation),
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.pd_contract_kind.as_str(), "pps");
    assert_eq!(status.pd_contract_power_mw, 100_000);
    assert!(status.pd_performance_guaranteed);
    assert!(pd_contract_allows_calibration(
        ControllerKind::Fusb302b,
        Some(observation)
    ));

    let backend = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_000,
        idle_request_mv: 12_000,
        pps_max_mv: 28_000,
        adjustable_max_mv: 28_000,
        capability_max_ma: 5_000,
        current_mode: Some(ch224q::AdjustableVoltageMode::Avs),
        current_request_mv: 24_000,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: false,
    };
    let constrained = constrain_heater_backend_to_controller(ControllerKind::Fusb302b, backend);
    let HeaterPowerBackend::PpsMos {
        pps_max_mv,
        adjustable_max_mv,
        current_mode,
        current_request_mv,
        ..
    } = constrained
    else {
        panic!("FUSB302B must retain a PPS backend when a PPS APDO is present");
    };
    assert_eq!(pps_max_mv, 28_000);
    assert_eq!(adjustable_max_mv, 28_000);
    assert_eq!(current_mode, Some(ch224q::AdjustableVoltageMode::Pps));
    assert_eq!(current_request_mv, 24_000);
}

#[test]
fn fusb302b_deferred_source_capabilities_promote_the_pps_backend() {
    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_000,
        max_mv: 21_000,
        max_ma: 3_000,
    });

    let backend = select_fusb302b_heater_power_backend(Some(capabilities));
    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        pps_max_mv,
        capability_max_ma,
        current_mode,
        ..
    } = backend
    else {
        panic!("FUSB302B must promote from fixed fallback to PPS");
    };
    assert_eq!(pps_min_mv, FUSB302B_PPS_MIN_MV);
    assert_eq!(pps_max_mv, 21_000);
    assert_eq!(capability_max_ma, 3_000);
    assert_eq!(current_mode, Some(ch224q::AdjustableVoltageMode::Pps));
}

#[test]
fn fusb302b_capability_refresh_preserves_terminal_fixed_pd_disarm() {
    let previous = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_500,
        idle_request_mv: 12_000,
        pps_max_mv: 21_000,
        adjustable_max_mv: 21_000,
        capability_max_ma: 3_000,
        current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
        current_request_mv: 20_000,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: true,
    };
    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_500),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 21_000,
        max_ma: 3_000,
    });

    assert!(matches!(
        refresh_fusb302b_heater_power_backend(previous, Some(capabilities)),
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed: true,
            ..
        }
    ));
}

#[test]
fn complete_idle_clears_failed_pps_adapter_only_with_observed_capability() {
    for current_ma in [3_000, 5_000] {
        let (power, _) = production_power_fixture(current_ma);
        let capabilities = adjustable_capabilities_from_view(power.source_capabilities);
        let failed = HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::AdjustableRequestFailed,
            fixed_request_confirmed: false,
            fixed_request: ch224q::VoltageRequest::V12,
            terminal_fixed_pd_disarmed: true,
        };
        let mut backend = failed;
        assert!(!reset_idle_failed_pps_backend(&mut backend, None));
        assert_eq!(backend, failed);
        assert!(reset_idle_failed_pps_backend(&mut backend, capabilities));
        assert!(matches!(
            backend,
            HeaterPowerBackend::PpsMos {
                capability_max_ma,
                terminal_fixed_pd_disarmed: true,
                settle_until_ms: None,
                next_request_at_ms: 0,
                ..
            } if capability_max_ma == current_ma
        ));
        assert!(!reset_idle_failed_pps_backend(&mut backend, capabilities));
    }
}

#[test]
fn fusb302b_rediscovery_preserves_terminal_fixed_pd_disarm() {
    let previous = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_500,
        idle_request_mv: 12_000,
        pps_max_mv: 21_000,
        adjustable_max_mv: 21_000,
        capability_max_ma: 3_000,
        current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
        current_request_mv: 20_000,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
        terminal_fixed_pd_disarmed: true,
    };
    let fallback = refresh_fusb302b_heater_power_backend(previous, None);
    assert!(matches!(
        fallback,
        HeaterPowerBackend::FixedPdPwmFallback {
            terminal_fixed_pd_disarmed: true,
            ..
        }
    ));

    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_500),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 21_000,
        max_ma: 3_000,
    });

    assert!(matches!(
        refresh_fusb302b_heater_power_backend(fallback, Some(capabilities)),
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed: true,
            ..
        }
    ));
}

#[test]
fn fusb302b_initial_backend_uses_fusb302b_request_bounds() {
    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_500),
        pps_max_mv: Some(28_000),
        pps_max_ma: Some(3_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 28_000,
        max_ma: 3_000,
    });

    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        pps_max_mv,
        ..
    } = select_fusb302b_heater_power_backend(Some(capabilities))
    else {
        panic!("FUSB302B should retain a usable PPS backend");
    };

    assert_eq!(pps_min_mv, FUSB302B_PPS_MIN_MV);
    assert_eq!(pps_max_mv, FUSB302B_PPS_MAX_MV);
}

#[test]
fn fusb302b_accepts_high_voltage_only_three_amp_pps_apdo() {
    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(12_000),
        pps_max_mv: Some(21_000),
        pps_max_ma: Some(3_000),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 12_000,
        max_mv: 21_000,
        max_ma: 3_000,
    });

    let HeaterPowerBackend::PpsMos {
        pps_min_mv,
        pps_max_mv,
        capability_max_ma,
        ..
    } = select_fusb302b_heater_power_backend(Some(capabilities))
    else {
        panic!("a 12-21V/3A APDO is a valid automatic heater source");
    };
    assert_eq!(pps_min_mv, 12_000);
    assert_eq!(pps_max_mv, 21_000);
    assert_eq!(capability_max_ma, 3_000);
}

#[test]
fn fusb302b_pending_contract_is_not_reported_as_ready() {
    let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    let status = usb_runtime_status(
        &ui_state,
        &MemoryConfig::default(),
        UsbRuntimeStatusContext {
            pd_controller: ControllerKind::Fusb302b,
            heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
                reason: HeaterPowerBackendReason::NoPps20vCapability,
                fixed_request_confirmed: false,
                fixed_request: ch224q::VoltageRequest::V20,
                terminal_fixed_pd_disarmed: false,
            },
            vin_mv: 20_000,
            ..test_usb_runtime_status_context()
        },
    );

    assert_eq!(status.pd_contract_kind.as_str(), "none");
    assert_eq!(status.pd_contract_current_ma, 0);
    assert_eq!(status.pd_contract_power_mw, 0);
    assert!(!status.pd_performance_guaranteed);
    assert_eq!(
        status.pd_degraded_reason.as_deref(),
        Some("pd_contract_unavailable")
    );
}

#[test]
fn pps_keepalive_without_a_response_expires_only_the_probe_window() {
    assert!(!pps_keepalive_response_timeout_due(None, 10_000));
    assert!(!pps_keepalive_response_timeout_due(Some(10_000), 11_499));
    assert!(pps_keepalive_response_timeout_due(Some(10_000), 11_500));
    assert!(pps_keepalive_response_timeout_due(Some(0), u64::MAX));
}

#[test]
fn pps_keepalive_deadline_requires_a_fresh_expected_control_response() {
    assert!(pps_keepalive_response_is_valid(
        SinkPhase::Ready,
        true,
        3,
        true
    ));
    assert!(pps_keepalive_response_is_valid(
        SinkPhase::Ready,
        true,
        6,
        true
    ));
    assert!(!pps_keepalive_response_is_valid(
        SinkPhase::Ready,
        true,
        1,
        true
    ));
    assert!(!pps_keepalive_response_is_valid(
        SinkPhase::Ready,
        true,
        3,
        false
    ));
    assert!(!pps_keepalive_response_is_valid(
        SinkPhase::WaitingForPsRdy,
        true,
        6,
        true
    ));
}

#[test]
fn only_a_fresh_reject_or_wait_settles_a_pending_request() {
    let mut policy = fusb302b::SinkPolicy::new(12_000, 3_000);
    assert!(
        policy
            .on_source_capabilities_with_message_id(
                &[pps_source_capability(5_500, 21_000, 3_000)],
                Some(3),
            )
            .is_some()
    );
    let phase = policy.phase();
    assert_eq!(phase, SinkPhase::WaitingForAccept);

    for message_type in [4, 12] {
        assert!(!response_rejects_pending_request(
            phase,
            message_type,
            policy.source_capabilities_message_is_fresh(Some(2)),
        ));
        policy.on_control_message_with_message_id(message_type, Some(2), 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        assert!(response_rejects_pending_request(
            phase,
            message_type,
            policy.source_capabilities_message_is_fresh(Some(4)),
        ));
    }
    assert!(!response_rejects_pending_request(phase, 3, true));
    assert!(!response_rejects_pending_request(SinkPhase::Ready, 4, true));
}

#[test]
fn successful_contract_confirmation_clears_a_previous_timeout_marker() {
    assert!(successful_contract_confirmation_clears_timeout(
        true,
        SinkPhase::Ready
    ));
    assert!(!successful_contract_confirmation_clears_timeout(
        true,
        SinkPhase::WaitingForPsRdy
    ));
    assert!(!successful_contract_confirmation_clears_timeout(
        false,
        SinkPhase::Ready
    ));
}

#[test]
fn contract_request_response_timeout_uses_the_slow_transition_window() {
    assert!(!contract_request_response_timeout_due(None, 10_000));
    assert!(!contract_request_response_timeout_due(Some(10_000), 14_999));
    assert!(contract_request_response_timeout_due(Some(10_000), 15_000));
    assert!(contract_request_response_timeout_due(Some(0), u64::MAX));
}

#[test]
fn fusb302b_protocol_fault_snapshot_codes_map_faults() {
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_RETRY_FAILED),
        Some("pd_fusb_retry_failed")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_PENDING_REQUEST_TIMEOUT),
        Some("pd_fusb_pending_request_timeout")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_READ_INTERRUPTS_IO),
        Some("pd_fusb_read_interrupts_io_error")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_READ_STATUS_IO),
        Some("pd_fusb_read_status_io_error")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_RX_FIFO_FLUSH_IO),
        Some("pd_fusb_rx_fifo_flush_io_error")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_RECEIVE_PACKET_IO),
        Some("pd_fusb_receive_packet_io_error")
    );
    assert_eq!(
        fusb302b_protocol_fault_code(FUSB302B_PROTOCOL_FAULT_NONE),
        None
    );
}

#[test]
fn fusb302b_i2c_error_snapshot_codes_map_errors() {
    assert_eq!(
        fusb302b_i2c_error_code(FUSB302B_I2C_ERROR_ACK_ADDRESS),
        Some("pd_i2c_ack_address")
    );
    assert_eq!(
        fusb302b_i2c_error_code(FUSB302B_I2C_ERROR_TIMEOUT),
        Some("pd_i2c_timeout")
    );
    assert_eq!(
        fusb302b_i2c_error_code(FUSB302B_I2C_ERROR_EXECUTION_INCOMPLETE),
        Some("pd_i2c_execution_incomplete")
    );
    assert_eq!(fusb302b_i2c_error_code(FUSB302B_I2C_ERROR_NONE), None);
}

#[test]
fn fixed_pd_settle_requires_an_observed_fixed_contract() {
    let fixed_observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: 3_000,
        contract_voltage_mv: Some(20_000),
        contract: Contract::observed(ContractKind::Fixed, 20_000, 3_000),
    };

    assert!(pd_observation_confirms_fixed_contract(
        Some(fixed_observation),
        20_000
    ));
    assert!(!pd_observation_confirms_fixed_contract(
        Some(PdStatusObservation {
            contract: Contract::observed(ContractKind::Pps, 20_000, 3_000),
            ..fixed_observation
        }),
        20_000
    ));
    assert!(!pd_observation_confirms_fixed_contract(None, 20_000));
}

#[test]
fn fusb302b_backend_never_inherits_a_ch224q_fixed_voltage_default() {
    let legacy = HeaterPowerBackend::FixedPdPwmFallback {
        reason: HeaterPowerBackendReason::CapabilityReadFailed,
        fixed_request_confirmed: true,
        fixed_request: ch224q::VoltageRequest::V28,
        terminal_fixed_pd_disarmed: false,
    };

    let fusb = constrain_heater_backend_to_controller(ControllerKind::Fusb302b, legacy);
    assert_eq!(fusb.pd_request_mv(), 12_000);
    let HeaterPowerBackend::FixedPdPwmFallback {
        fixed_request_confirmed,
        ..
    } = fusb
    else {
        panic!("FUSB302B must use fixed-PDO PWM fallback");
    };
    assert!(!fixed_request_confirmed);

    let ch224q = constrain_heater_backend_to_controller(ControllerKind::Ch224q, legacy);
    assert_eq!(ch224q.pd_request_mv(), 28_000);
}

#[test]
fn fault_attention_transitions_alarm_to_pending_reminder() {
    let mut last_fault_present = false;
    let mut attention_acknowledged = false;
    let mut attention_pending = false;
    let mut forced_fan_active = false;
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut next_reminder_ms = None;
    let mut buzzer = BuzzerArbiter::new();

    assert!(update_fault_attention_state(
        true,
        FaultAttentionState {
            last_fault_present: &mut last_fault_present,
            attention_acknowledged: &mut attention_acknowledged,
            attention_pending_after_fault_clear: &mut attention_pending,
            forced_fan_active: &mut forced_fan_active,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        100,
        &mut buzzer,
        3_000,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    assert!(!attention_pending);
    assert_eq!(protection_alarm.next_replay_ms(), Some(4_000));
    assert_eq!(next_reminder_ms, None);
    assert!(forced_fan_active);

    assert!(update_fault_attention_state(
        false,
        FaultAttentionState {
            last_fault_present: &mut last_fault_present,
            attention_acknowledged: &mut attention_acknowledged,
            attention_pending_after_fault_clear: &mut attention_pending,
            forced_fan_active: &mut forced_fan_active,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        100,
        &mut buzzer,
        8_000,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::AttentionReminder));
    assert!(attention_pending);
    assert_eq!(protection_alarm.next_replay_ms(), None);
    assert_eq!(
        next_reminder_ms,
        Some(8_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
    );
    assert!(forced_fan_active);
}

#[test]
fn attention_reminder_starts_immediately_after_fault_clear_then_rearms() {
    let mut last_fault_present = true;
    let mut attention_acknowledged = false;
    let mut attention_pending = false;
    let mut forced_fan_active = true;
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut next_reminder_ms = None;
    let mut buzzer = BuzzerArbiter::new();
    buzzer.activate_protection(BuzzerCueSource::ThermalProtection, 0);

    assert!(update_fault_attention_state(
        false,
        FaultAttentionState {
            last_fault_present: &mut last_fault_present,
            attention_acknowledged: &mut attention_acknowledged,
            attention_pending_after_fault_clear: &mut attention_pending,
            forced_fan_active: &mut forced_fan_active,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        39,
        &mut buzzer,
        2_000,
    ));

    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::AttentionReminder));
    assert_eq!(buzzer.output().frequency_hz, Some(1_650));
    assert_eq!(
        next_reminder_ms,
        Some(2_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
    );
}

#[test]
fn measurement_faults_do_not_require_overtemp_attention() {
    for reason in [
        HeaterFaultReason::SensorOpen,
        HeaterFaultReason::SensorShort,
        HeaterFaultReason::AdcReadFailed,
    ] {
        assert!(!is_overtemp_fault(Some(reason)));
    }
    assert!(is_overtemp_fault(Some(HeaterFaultReason::OverTemp)));
    assert!(!is_overtemp_fault(None));
}

#[test]
fn acknowledging_active_overtemp_keeps_alarm_but_prevents_pending_reminder() {
    let mut last_overtemp_present = false;
    let mut acknowledged = false;
    let mut pending = false;
    let mut forced_fan = false;
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut next_reminder_ms = None;
    let mut buzzer = BuzzerArbiter::new();

    assert!(update_fault_attention_state(
        true,
        FaultAttentionState {
            last_fault_present: &mut last_overtemp_present,
            attention_acknowledged: &mut acknowledged,
            attention_pending_after_fault_clear: &mut pending,
            forced_fan_active: &mut forced_fan,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        420,
        &mut buzzer,
        0,
    ));
    assert!(acknowledge_overtemp_attention(
        true,
        &mut acknowledged,
        &mut pending,
        &mut forced_fan,
        &mut next_reminder_ms,
        &mut buzzer,
    ));
    assert!(acknowledged);
    assert!(!pending);
    assert!(!forced_fan);
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));

    assert!(update_fault_attention_state(
        false,
        FaultAttentionState {
            last_fault_present: &mut last_overtemp_present,
            attention_acknowledged: &mut acknowledged,
            attention_pending_after_fault_clear: &mut pending,
            forced_fan_active: &mut forced_fan,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        100,
        &mut buzzer,
        2_000,
    ));
    assert!(!pending);
    assert_eq!(next_reminder_ms, None);
}

#[test]
fn cooling_below_40_releases_forced_fan_but_not_attention_requirement() {
    let mut last_overtemp_present = true;
    let mut acknowledged = false;
    let mut pending = false;
    let mut forced_fan = true;
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut next_reminder_ms = None;
    let mut buzzer = BuzzerArbiter::new();

    assert!(update_fault_attention_state(
        false,
        FaultAttentionState {
            last_fault_present: &mut last_overtemp_present,
            attention_acknowledged: &mut acknowledged,
            attention_pending_after_fault_clear: &mut pending,
            forced_fan_active: &mut forced_fan,
            protection_alarm: &mut protection_alarm,
            next_attention_reminder_ms: &mut next_reminder_ms,
        },
        39,
        &mut buzzer,
        2_000,
    ));
    assert!(pending);
    assert!(!forced_fan);
    assert!(overtemp_attention_requires_ack(
        false,
        acknowledged,
        pending
    ));
}

#[test]
fn overtemp_forced_fan_follows_locked_temperature_bands() {
    assert_eq!(
        overtemp_forced_fan_state(61, true),
        Some(FanPolicyState::Full)
    );
    assert_eq!(
        overtemp_forced_fan_state(60, true),
        Some(FanPolicyState::SafeHalf)
    );
    assert_eq!(
        overtemp_forced_fan_state(40, true),
        Some(FanPolicyState::SafeHalf)
    );
    assert_eq!(overtemp_forced_fan_state(39, true), None);
    assert_eq!(overtemp_forced_fan_state(220, false), None);
}

#[test]
fn protection_alarm_replays_at_one_second_cadence_as_one_shots() {
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut buzzer = BuzzerArbiter::new();
    let _ = protection_alarm.enter(&mut buzzer, 0);

    let _ = buzzer.tick(90);
    let _ = buzzer.tick(130);
    let _ = buzzer.tick(220);
    assert_eq!(buzzer.tick(300).output.frequency_hz, None);

    assert!(!maybe_play_protection_alarm(
        true,
        &mut protection_alarm,
        &mut buzzer,
        999,
    ));
    assert_eq!(buzzer.active_cue(), None);

    assert!(maybe_play_protection_alarm(
        true,
        &mut protection_alarm,
        &mut buzzer,
        1_000,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    assert_eq!(protection_alarm.next_replay_ms(), Some(2_000));
    assert_eq!(buzzer.output().frequency_hz, Some(2_300));
}

#[test]
fn protection_alarm_replays_after_a_late_tick() {
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut buzzer = BuzzerArbiter::new();
    let _ = protection_alarm.enter(&mut buzzer, 0);

    // A late executor wake preserves the silent step instead of skipping
    // directly to the next audible pulse.
    assert_eq!(buzzer.tick(1_000).output.frequency_hz, None);
    assert_eq!(buzzer.output().duty_percent, 0);
    assert!(maybe_play_protection_alarm(
        true,
        &mut protection_alarm,
        &mut buzzer,
        1_000,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
    assert_eq!(buzzer.output().frequency_hz, None);
    assert_eq!(protection_alarm.next_replay_ms(), Some(2_000));
}

#[test]
fn attention_pending_consumes_first_input_and_stops_reminders() {
    let mut attention_acknowledged = false;
    let mut attention_pending = true;
    let mut forced_fan_active = true;
    let mut next_reminder_ms = Some(15_000);
    let mut buzzer = BuzzerArbiter::new();
    assert_eq!(buzzer.enter_attention_pending(), None);
    let _ = buzzer.request_attention_reminder(BuzzerCueSource::ThermalAttention, 10_000);

    assert!(acknowledge_overtemp_attention(
        false,
        &mut attention_acknowledged,
        &mut attention_pending,
        &mut forced_fan_active,
        &mut next_reminder_ms,
        &mut buzzer,
    ));
    assert!(attention_acknowledged);
    assert!(!attention_pending);
    assert!(!forced_fan_active);
    assert_eq!(next_reminder_ms, None);
    assert_eq!(buzzer.active_cue(), None);
}

#[test]
fn attention_pending_can_be_acknowledged_by_raw_unsupported_input() {
    let idle = flux_purr_firmware::frontpanel::FrontPanelRawState::default();
    let mut unsupported_press = idle;
    unsupported_press.set_pressed(flux_purr_firmware::frontpanel::RawFrontPanelKey::Up, true);

    assert!(should_consume_attention_raw_input(
        true,
        false,
        idle,
        unsupported_press,
    ));
    assert!(!should_consume_attention_raw_input(
        true,
        true,
        idle,
        unsupported_press,
    ));
    assert!(!should_consume_attention_raw_input(
        false,
        false,
        idle,
        unsupported_press,
    ));
    assert!(!should_consume_attention_raw_input(
        true,
        false,
        unsupported_press,
        idle,
    ));
}

#[test]
fn attention_ack_suppression_waits_for_delayed_supported_events() {
    let idle = flux_purr_firmware::frontpanel::FrontPanelRawState::default();

    assert!(should_clear_attention_ack_suppression(
        true, false, false, idle, None, 1_000,
    ));
    assert!(!should_clear_attention_ack_suppression(
        true,
        true,
        false,
        idle,
        Some(1_020),
        1_019,
    ));
    assert!(should_clear_attention_ack_suppression(
        true,
        true,
        false,
        idle,
        Some(1_020),
        1_020,
    ));
    assert!(!should_clear_attention_ack_suppression(
        true,
        true,
        false,
        idle,
        Some(1_250),
        1_200,
    ));
    assert!(should_clear_attention_ack_suppression(
        true,
        true,
        true,
        idle,
        Some(1_250),
        1_200,
    ));
    assert!(should_clear_attention_ack_suppression(
        true,
        true,
        false,
        idle,
        Some(1_250),
        1_250,
    ));
}

#[test]
fn attention_reminder_rearms_every_10_seconds_until_acknowledged() {
    let mut next_reminder_ms = Some(10_000);
    let mut buzzer = BuzzerArbiter::new();
    assert_eq!(buzzer.enter_attention_pending(), None);

    assert!(!maybe_play_attention_reminder(
        true,
        false,
        &mut next_reminder_ms,
        &mut buzzer,
        9_999,
    ));
    assert_eq!(buzzer.active_cue(), None);

    assert!(maybe_play_attention_reminder(
        true,
        false,
        &mut next_reminder_ms,
        &mut buzzer,
        10_000,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::AttentionReminder));
    assert_eq!(
        next_reminder_ms,
        Some(10_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
    );
}

#[test]
fn generic_ui_feedback_plays_for_handled_non_specialized_actions() {
    let mut buzzer = BuzzerArbiter::new();

    assert!(maybe_play_frontpanel_ui_input_feedback(
        true,
        false,
        &mut buzzer,
        2_500,
    ));
    assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::UiInput));
    assert_eq!(buzzer.output().frequency_hz, Some(1_080));
}

#[test]
fn generic_ui_feedback_skips_specialized_actions() {
    let mut buzzer = BuzzerArbiter::new();

    assert!(!maybe_play_frontpanel_ui_input_feedback(
        true,
        true,
        &mut buzzer,
        2_500,
    ));
    assert_eq!(buzzer.active_cue(), None);
    assert_eq!(buzzer.output().frequency_hz, None);
}

#[test]
fn memory_restore_does_not_restore_heater_arm() {
    let mut state = flux_purr_firmware::frontpanel::FrontPanelUiState::new(
        flux_purr_firmware::frontpanel::FrontPanelRuntimeMode::App,
    );
    let persisted = MemoryConfig {
        target_temp_c: 180,
        active_cooling_enabled: false,
        post_heat_cooling_mode: PostHeatCoolingMode::Off,
        ..MemoryConfig::default()
    };
    let mut pending = MemoryConfig {
        target_temp_c: 251,
        active_cooling_enabled: true,
        ..persisted.clone()
    };

    apply_memory_config_to_ui(&mut state, &pending);
    restore_last_persisted_memory_config(&mut pending, &mut state, &persisted);

    assert!(!state.heater_enabled);
    let restored = memory_config_from_ui(&state, &pending);
    assert_eq!(restored.target_temp_c, 180);
    assert!(!restored.active_cooling_enabled);
}

#[test]
fn runtime_heater_reconcile_preserves_dashboard_heater_when_calibration_is_off() {
    let desired = reconcile_runtime_heater_enabled(
        true,
        CalibrationRuntimeState::default(),
        None,
        false,
        false,
        true,
        true,
    );

    assert!(desired);
}

#[test]
fn pending_automatic_pps_preserves_heater_intent_during_contract_transition() {
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    ui_state.heater_enabled = true;
    let mut calibration_runtime_state = CalibrationRuntimeState::default();
    let mut manual_pps_state = ManualPpsState::default();
    let mut pd_contract_ready = true;
    let transition_observation = PdStatusObservation {
        status_raw: 0,
        status: Status::from_register(0),
        current_raw: 0,
        current_ma: 0,
        contract_voltage_mv: None,
        contract: Contract::none(),
    };

    assert!(reconcile_pd_contract_intent_with_pending_automatic_pps(
        Some(transition_observation),
        &mut pd_contract_ready,
        &mut ui_state,
        &mut calibration_runtime_state,
        &mut manual_pps_state,
        false,
        true,
    ));
    assert!(!pd_contract_ready);
    assert!(ui_state.heater_enabled);
    assert!(!calibration_runtime_state.heater_enabled);
    assert!(reconcile_runtime_heater_enabled(
        ui_state.heater_enabled,
        calibration_runtime_state,
        None,
        false,
        false,
        true,
        true,
    ));
}

#[test]
fn pd_unavailable_startup_enters_dashboard_with_heater_locked() {
    let pd_contract_ready = startup_pd_contract_ready(None);
    let state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);

    assert_eq!(state.route, FrontPanelRoute::Dashboard);
    assert!(!pd_contract_ready);
    assert_eq!(
        startup_frontpanel_presentation(FrontPanelRuntimeMode::App),
        StartupFrontPanelPresentation::Splash
    );
    assert_eq!(
        startup_frontpanel_presentation(FrontPanelRuntimeMode::KeyTest),
        StartupFrontPanelPresentation::Calibration
    );
    assert_eq!(
        next_heater_lock_reason(None, false, true, pd_contract_ready),
        Some(HeaterLockReason::PdContractUnavailable)
    );
    assert!(!reconcile_runtime_heater_enabled(
        true,
        CalibrationRuntimeState::default(),
        None,
        false,
        false,
        true,
        pd_contract_ready,
    ));
}

#[test]
fn sensor_fault_lock_reason_is_exposed_without_relaxing_fail_closed_behavior() {
    assert_eq!(
        next_heater_lock_reason(Some(HeaterFaultReason::SensorOpen), false, true, true,),
        Some(HeaterLockReason::SensorFault)
    );
    assert!(!reconcile_runtime_heater_enabled(
        true,
        CalibrationRuntimeState {
            mode: CalibrationMode::RtdAdc,
            heater_enabled: true,
            ..CalibrationRuntimeState::default()
        },
        Some(HeaterFaultReason::SensorOpen),
        false,
        true,
        true,
        true,
    ));
}

#[test]
fn persistence_lock_reason_is_exposed_before_other_runtime_gates() {
    assert_eq!(
        next_heater_lock_reason_with_persistence(true, None, false, true, true),
        Some(HeaterLockReason::PersistenceRequired)
    );
    assert_eq!(
        next_heater_lock_reason_with_persistence(false, None, false, true, true),
        None
    );
    assert_eq!(
        next_heater_lock_reason_with_automatic_power(false, None, false, true, true, false),
        Some(HeaterLockReason::NoSafeHeaterPower)
    );
    assert_eq!(
        next_heater_lock_reason_with_automatic_power(true, None, false, true, true, false),
        Some(HeaterLockReason::PersistenceRequired)
    );
}

#[test]
fn initial_rtd_fault_latches_dashboard_and_heater_lock() {
    let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
    let mut heater_controller = HeaterController::new();

    ui_state.set_dashboard_presentation(
        flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault,
    );
    assert!(heater_controller.latch_fault(HeaterFaultReason::SensorOpen));
    assert_eq!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault
    );
    assert_eq!(
        next_heater_lock_reason(heater_controller.fault_latched(), false, true, true),
        Some(HeaterLockReason::SensorFault)
    );
    assert!(!reconcile_runtime_heater_enabled(
        true,
        CalibrationRuntimeState {
            mode: CalibrationMode::RtdAdc,
            heater_enabled: true,
            ..CalibrationRuntimeState::default()
        },
        heater_controller.fault_latched(),
        false,
        true,
        true,
        true,
    ));
}

#[test]
fn startup_pd_service_prioritizes_negotiation_before_low_priority_startup_work() {
    assert!(startup_pd_service_should_continue(true, false, true, 0));
    assert!(startup_pd_service_should_continue(true, false, true, 749));
    assert!(!startup_pd_service_should_continue(true, false, true, 750));
    assert!(!startup_pd_service_should_continue(true, true, true, 0));
    assert!(!startup_pd_service_should_continue(true, false, false, 0));
    assert!(!startup_pd_service_should_continue(false, false, true, 0));
}

#[test]
fn runtime_loop_services_pd_before_control_plane_work() {
    let source = RUNTIME_IMPLEMENTATION;
    let runtime_loop = source
        .split("async fn run_runtime_loop")
        .nth(1)
        .expect("runtime loop marker must remain present");
    let pd_service = runtime_loop
        .find("runtime_apply_pd_snapshot")
        .expect("runtime loop must apply the independent PD snapshot");
    let control_plane = runtime_loop
        .find("runtime_process_input")
        .expect("runtime loop must retain control-plane handling");

    assert!(pd_service < control_plane);
}

#[test]
fn runtime_control_input_is_bounded_before_the_next_pd_service() {
    let source = RUNTIME_IMPLEMENTATION;
    let usb_input = source;
    let normalized_source: String = source.split_whitespace().collect();

    assert!(usb_input.contains("while usb_bytes_processed < PD_RUNTIME_USB_BYTE_BUDGET"));
    let control_frame = normalized_source
        .find("usb_start_response_frame(&mutstate.transport.usb_response_writer,&response,state.transport.usb_tx_buf,")
        .expect("USB control path must use the response writer");
    let control_frame_tail = &normalized_source[control_frame..];
    assert!(control_frame_tail.contains("usb_rx_line.clear();"));
    assert!(normalized_source.contains("returnruntime_process_usb_line(state,elapsed_ms).await;"));
    assert!(
        !normalized_source.contains("usb_response_tx"),
        "runtime must not retain an undelivered USB response queue"
    );
    assert!(source.contains("let Some(command) = flux_purr_firmware::net::try_receive_command()"));
    assert!(
        !source
            .contains("while let Some(command) = flux_purr_firmware::net::try_receive_command()")
    );
    assert!(
        normalized_source
            .contains("letusb_response_state=runtime_pump_usb_response_budget(state).await;")
    );
    assert!(normalized_source.contains("UsbResponsePumpOutcome::Fault"));
    assert!(normalized_source.contains("UsbResponsePumpOutcome::Idle"));
    assert!(normalized_source.contains("usb_transport_faulted=true"));
    assert!(normalized_source.contains("usb_start_transport_recovery"));
    assert!(normalized_source.contains("usb_pump_recovery_response"));
    assert!(!normalized_source.contains("usb_recovery_marker_failed"));
    assert!(
        !normalized_source.contains("ifusb_pump_response(&mutstate.transport.usb_serial,return")
    );
}

#[test]
fn skipped_runtime_iterations_still_dispatch_the_sampled_frontpanel_input() {
    let runtime_loop = RUNTIME_IMPLEMENTATION;
    let skip_branch = runtime_loop
        .split("if input.skip_iteration")
        .nth(1)
        .expect("runtime loop must retain the bounded skip path");

    assert!(skip_branch.contains("runtime_process_frontpanel_input"));
    assert!(skip_branch.contains("input.sample"));
    assert!(skip_branch.contains("input.pairing_opened_by_usb"));
    assert!(skip_branch.contains("runtime_control_heater"));
    assert!(skip_branch.contains("runtime_process_pending_safety"));
}

#[test]
fn early_usb_write_failures_enter_framing_recovery() {
    let control_plane = include_str!("control_plane.rs");
    let early_control = control_plane
        .split("pub(crate) async fn poll_usb_early_control")
        .nth(1)
        .expect("early USB control poll must remain present");

    assert!(early_control.contains("if !usb_write_response_frame"));
    assert!(early_control.contains("USB_TRANSPORT_FAULT_MARKER"));
    assert!(early_control.contains("run_usb_recovery_control_loop"));
}

#[test]
fn network_awaits_do_not_own_or_wrap_pd_service() {
    let source = RUNTIME_IMPLEMENTATION;
    let lan = include_str!("lan.rs");
    let runtime_loop = include_str!("runtime_loop.rs");
    assert!(!source.contains("run_network_operation_with_snapshot("));
    assert!(!source.contains("run_network_operation_with_pd("));
    for call in [
        "flux_purr_firmware::net::lan_network_summary().await",
        "flux_purr_firmware::net::enter_pairing().await",
        "flux_purr_firmware::net::leave_pairing().await",
        "flux_purr_firmware::net::clear_token_from_usb().await",
        "flux_purr_firmware::net::cancel_wifi_connection().await",
        "flux_purr_firmware::net::apply_wifi_config(context.memory_config).await",
    ] {
        assert!(lan.contains(call), "expected direct network await: {call}");
    }
    for call in [
        "flux_purr_firmware::net::command_lease_is_active(&command).await",
        "flux_purr_firmware::net::lan_identity().await",
        "flux_purr_firmware::net::lan_network_summary().await",
        "flux_purr_firmware::net::take_persisted_token_change().await",
    ] {
        assert!(
            runtime_loop.contains(call),
            "expected direct runtime network await: {call}"
        );
    }
    assert!(source.contains("async fn initialize_network_control_state"));
    assert!(source.contains("async fn spawn_network"));
    assert!(source.contains("flux_purr_firmware::net::spawn("));
}

#[test]
fn lan_runtime_helpers_are_excluded_without_net_http() {
    let runtime_loop = include_str!("runtime_loop.rs").replace("#[inline(never)]\n", "");
    for helper in [
        "impl RuntimeLanInputOutcome",
        "async fn runtime_lan_direct_response",
        "fn runtime_process_lan_control",
        "fn runtime_box_lan_response",
        "fn runtime_execute_lan_command",
        "fn runtime_reject_lan_command",
    ] {
        let feature_gate = format!(
            "#[cfg(all(target_arch = \"xtensa\", feature = \"net_http\"))]\npub(crate) {helper}"
        );
        let impl_feature_gate =
            format!("#[cfg(all(target_arch = \"xtensa\", feature = \"net_http\"))]\n{helper}");
        assert!(
            runtime_loop.contains(&feature_gate) || runtime_loop.contains(&impl_feature_gate),
            "{helper} must stay out of the no-network firmware image"
        );
    }
}

#[test]
fn fixed_contract_does_not_emit_liveness_probes() {
    let adc = include_str!("adc.rs");
    let poll = adc
        .split("pub(crate) async fn poll")
        .nth(1)
        .and_then(|source| source.split("async fn poll_receive_messages").next())
        .expect("PD poll must remain adjacent to message processing");

    assert!(
        !adc.contains("async fn poll_liveness_probe")
            && !poll.contains("poll_liveness_probe")
            && !adc.contains("FixedContractLiveness"),
        "fixed PDOs must not send unsolicited Get_Source_Capabilities probes"
    );
}

#[test]
fn runtime_pd_service_interlocks_stale_heater_output_in_the_high_priority_path() {
    let source = RUNTIME_IMPLEMENTATION;
    let pd_service = source
        .find("async fn pd_service_task")
        .expect("PD task must own protocol polling");
    let control_tick = source
        .find("async fn runtime_control_heater")
        .expect("runtime loop must retain thermal control scheduling");

    let pd_service_source = include_str!("pd_service.rs");
    assert!(pd_service_source.contains("HeaterPwmGate::force_off()"));
    assert!(pd_service_source.contains("pd_snapshot_is_fresh"));
    assert!(pd_service < control_tick);
    assert!(source.contains("PD_HEATER_PERMIT"));
}

#[test]
fn pd_contract_loss_relocks_an_armed_heater() {
    let calibration = CalibrationRuntimeState::default();

    assert!(reconcile_runtime_heater_enabled(
        true,
        calibration,
        None,
        false,
        false,
        true,
        true,
    ));
    assert!(!reconcile_runtime_heater_enabled(
        true,
        calibration,
        None,
        false,
        false,
        true,
        false,
    ));
    assert_eq!(
        next_heater_lock_reason(None, false, true, false),
        Some(HeaterLockReason::PdContractUnavailable)
    );
}

#[test]
fn tuning_failure_identifies_the_interlock_that_disarmed_heating() {
    assert_eq!(
        tuning_interlock_reason(false, true, false, false, false, false),
        "pd_contract_lost"
    );
    assert_eq!(
        tuning_interlock_reason(true, true, true, false, false, false),
        "persistence_interlock_disarmed"
    );
    assert_eq!(
        tuning_interlock_reason(true, false, false, false, false, false),
        "thermal_model_interlock_disarmed"
    );
    assert_eq!(
        tuning_interlock_reason(true, true, false, true, false, false),
        "cooling_interlock_disarmed"
    );
    assert_eq!(
        tuning_interlock_reason(true, true, false, false, true, false),
        "controller_fault_disarmed"
    );
    assert_eq!(
        tuning_interlock_reason(true, true, false, false, false, true),
        "sensor_fault_disarmed"
    );
    assert_eq!(
        tuning_interlock_reason(true, true, false, false, false, false),
        "heater_arm_cleared_before_trial"
    );
}

#[test]
fn tuning_pd_failure_context_retains_actual_failure_and_contracts() {
    let (mut power, _) = production_power_fixture(5_000);
    power.protocol = PowerProtocol::WaitingForPsRdy;
    power.failure = Some(PowerFailure::CapabilityInvalidated);
    power.requested = Some(PdContractRequest::pps(18_500, 5_000).unwrap());
    let context = tuning_pd_failure_context(power, Some(10), Some(100), 250, "power");
    assert_eq!(
        context[0].as_str(),
        "pd:power p=wait_ps_rdy f=capability_invalidated available=1 age_ms=150"
    );
    let active = power.active.unwrap();
    assert_eq!(
        context[1].as_str(),
        format!(
            "pd:power a={}/5000 r=18500/5000 ticket=1 at_ms=250",
            active.voltage_mv
        )
    );

    let absent = tuning_pd_failure_context(PowerState::unavailable(), None, None, 250, "sample");
    assert_eq!(
        absent[0].as_str(),
        "pd:sample p=unknown f=unavailable available=0"
    );
    assert_eq!(
        absent[1].as_str(),
        "pd:sample a=none r=none ticket=0 at_ms=250"
    );
}

#[test]
fn tuning_pd_failure_context_preserves_full_width_wait_age() {
    let power = PowerState {
        protocol: PowerProtocol::WaitingForAccept,
        available: true,
        failure: Some(PowerFailure::CapabilityInvalidated),
        ..PowerState::unavailable()
    };
    let context = tuning_pd_failure_context(power, Some(0), None, u64::MAX, "interlock");
    assert_eq!(
        context[0].as_str(),
        "pd:interlock p=wait_accept f=capability_invalidated available=1 age_ms=18446744073709551615"
    );
    assert!(context[0].len() <= flux_purr_firmware::thermal_tuning::THERMAL_TUNING_REASON_MAX_LEN);
    assert!(context[1].ends_with("at_ms=18446744073709551615"));
}

#[test]
fn tuning_waits_for_an_admitted_pd_request_instead_of_failing_at_first_missing_snapshot() {
    let request = PdContractRequest::pps(12_500, 5_000).unwrap();
    let pending = PowerState {
        protocol: PowerProtocol::WaitingForAccept,
        available: true,
        requested: Some(request),
        active: None,
        failure: None,
        ..PowerState::unavailable()
    };
    assert_eq!(
        tuning_pd_wait_decision(pending, Some(100), 129, Some(100), false),
        TuningPdWaitDecision::Wait
    );
    assert_eq!(
        tuning_pd_wait_decision(pending, Some(100), 5_601, Some(100), false),
        TuningPdWaitDecision::Fail("pd_request_timeout")
    );

    let failed = PowerState {
        failure: Some(PowerFailure::Detached),
        ..pending
    };
    assert_eq!(
        tuning_pd_wait_decision(failed, Some(100), 129, Some(100), false),
        TuningPdWaitDecision::Fail("pd_contract_lost")
    );

    let (ready, _) = production_power_fixture(5_000);
    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 129, Some(100), false),
        TuningPdWaitDecision::Wait
    );
    assert_eq!(
        tuning_pd_wait_decision(ready, None, 129, Some(100), false),
        TuningPdWaitDecision::Wait,
        "a pending ticket must block the previous ready contract even before the wait marker is set"
    );
    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 129, None, false),
        TuningPdWaitDecision::Ready
    );
    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 129, None, true),
        TuningPdWaitDecision::Fail("pd_request_failed")
    );

    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 10_642, Some(9_920), false),
        TuningPdWaitDecision::Wait,
        "a newly admitted 500 mV ticket must not inherit the first step's deadline"
    );
    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 15_421, Some(9_920), false),
        TuningPdWaitDecision::Fail("pd_request_timeout")
    );
}

#[test]
fn tuning_waits_for_working_pps_after_a_ready_fixed_idle_contract() {
    let (mut idle, _) = production_power_fixture(5_000);
    let active = idle.active.as_mut().unwrap();
    active.mode = PdContractRequestMode::Fixed;
    active.voltage_mv = 5_000;
    active.operating_current_ma = 1_000;
    assert_eq!(
        tuning_pd_wait_decision(idle, None, 100, None, false),
        TuningPdWaitDecision::Wait,
        "Fixed idle power does not qualify a PPS5A trial"
    );
    assert_eq!(
        tuning_pd_wait_decision(idle, Some(100), 5_601, None, false),
        TuningPdWaitDecision::Fail("pd_request_timeout")
    );
    let (ready, _) = production_power_fixture(5_000);
    assert_eq!(
        tuning_pd_wait_decision(ready, Some(100), 300, None, false),
        TuningPdWaitDecision::Ready
    );
}

#[test]
fn automatic_pps_progress_requires_current_heating_or_tuning_intent() {
    for current_ma in [3_000, 5_000] {
        let (mut power, _) = production_power_fixture(current_ma);
        let active = power.active.as_mut().unwrap();
        active.mode = PdContractRequestMode::Fixed;
        active.voltage_mv = 5_000;
        active.operating_current_ma = 1_000;
        for protocol in [
            PowerProtocol::Ready,
            PowerProtocol::WaitingForAccept,
            PowerProtocol::WaitingForPsRdy,
        ] {
            power.protocol = protocol;
            for ticket_pending in [false, true] {
                assert!(
                    !automatic_pps_progress_allowed(power, ticket_pending, false, false, false),
                    "standby must not initiate or preserve a thermal request: {current_ma}mA {protocol:?} ticket={ticket_pending}"
                );
                assert!(automatic_pps_progress_allowed(
                    power,
                    ticket_pending,
                    true,
                    true,
                    false
                ));
                assert!(automatic_pps_progress_allowed(
                    power,
                    ticket_pending,
                    false,
                    false,
                    true
                ));
            }
        }
    }
}

#[test]
fn newly_armed_tuning_reconciles_power_before_its_first_control_sample() {
    // The start command runs after the loop's initial PD snapshot. This wiring
    // guard covers the device-only call site that host state fixtures cannot
    // construct (hardware/watch receivers live inside RuntimeLoopState).
    let source = include_str!("runtime_loop.rs");
    let control = source
        .split("pub(crate) async fn runtime_control_heater")
        .nth(1)
        .and_then(|source| source.split("fn runtime_heater_output_settings").next())
        .unwrap();
    let readiness = control
        .find("runtime_apply_pd_snapshot(state, elapsed_ms)")
        .unwrap();
    assert!(
        readiness
            < control
                .find("runtime_apply_tuning_cooling_boundary")
                .unwrap()
    );
    assert!(readiness < control.find("runtime_update_heater_controller").unwrap());
    assert!(
        readiness
            < control
                .find("runtime_record_thermal_tuning_sample")
                .unwrap()
    );
}

#[test]
fn automatic_pps_ticket_requires_the_exact_confirmed_contract() {
    let (ready, _) = production_power_fixture(5_000);
    let active = ready.active.unwrap();
    assert_eq!(
        automatic_pps_ticket_decision(None, 100, 129, 20_000, 5_000),
        AutomaticPpsTicketDecision::Wait
    );
    assert_eq!(
        automatic_pps_ticket_decision(
            Some(TicketOutcome::Confirmed(active)),
            100,
            129,
            20_000,
            5_000,
        ),
        AutomaticPpsTicketDecision::Confirmed
    );
    assert_eq!(
        automatic_pps_ticket_decision(
            Some(TicketOutcome::Confirmed(active)),
            100,
            129,
            19_500,
            5_000,
        ),
        AutomaticPpsTicketDecision::Failed
    );
    assert_eq!(
        automatic_pps_ticket_decision(Some(TicketOutcome::Rejected), 100, 129, 20_000, 5_000),
        AutomaticPpsTicketDecision::Failed
    );
    assert_eq!(
        automatic_pps_ticket_decision(None, 100, 5_601, 20_000, 5_000),
        AutomaticPpsTicketDecision::Failed
    );
}

#[test]
fn automatic_pps_transition_never_freezes_or_replays_cached_pwm() {
    let source = include_str!("tasks.rs");
    let pending = source
        .split("fn poll_pending_automatic_power_ticket")
        .nth(1)
        .unwrap()
        .split("fn pps_request_mv_for_context")
        .next()
        .unwrap();
    assert!(pending.contains("pending_automatic_pps_duty(context, pending.pause_heater)"));
    assert!(pending.contains("context.pd_port.snapshot().settled_observation()"));
    let fields = source
        .split("struct PendingAutomaticPowerRequest {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!fields.contains("gate_duty_percent"));
    assert!(!fields.contains("duty_percent"));
}

#[test]
fn automatic_pps_live_gate_tracks_control_during_wait_and_confirmation() {
    let (ready, _) = production_power_fixture(5_000);
    let mut confirmed = ready.active.unwrap();
    confirmed.voltage_mv = 12_500;
    // Real HIL: the controller requests zero while a 13V -> 12.5V ticket
    // is pending. Both the waiting and confirmed output must be zero.
    for (outcome, voltage_bound_mv) in [
        (None, 13_000),
        (Some(TicketOutcome::Confirmed(confirmed)), 12_500),
    ] {
        assert_ne!(
            automatic_pps_ticket_decision(outcome, 100, 129, 12_500, 5_000),
            AutomaticPpsTicketDecision::Failed
        );
        assert_eq!(
            live_pps_gate_duty_percent(0, true, false, 21_000, voltage_bound_mv, 100),
            0
        );
    }
    assert_eq!(
        live_pps_gate_duty_percent(12, true, false, 21_000, 21_000, 100),
        12
    );
    assert_eq!(
        live_pps_gate_duty_percent(12, true, false, 21_000, 20_500, 100),
        12
    );
    assert_eq!(
        live_pps_gate_duty_percent(19, true, false, 21_000, 12_500, 100),
        53
    );
    assert_eq!(
        live_pps_gate_duty_percent(100, true, false, 21_000, 21_000, 50),
        50
    );
    assert_eq!(
        live_pps_gate_duty_percent(100, false, false, 21_000, 21_000, 100),
        0
    );
    assert_eq!(
        live_pps_gate_duty_percent(100, true, true, 21_000, 21_000, 100),
        0
    );
}

#[test]
fn tuning_power_wait_resumes_profile_without_a_false_source_change() {
    let mut controller = HeaterController::new();
    controller.production_controller = Some(ProductionThermalController::Profile(
        ThermalProfileBank::Pps5a,
    ));
    let paused = pause_tuning_profile_controller(&mut controller, 60, 34.0, 129);
    assert_eq!(paused.duty_percent, 0);
    assert_eq!(controller.production_controller, None);
    let (_, disarm) = controller.update_production_at(
        ProductionThermalController::Profile(ThermalProfileBank::Pps5a),
        production_test_input(34.0),
        Some(production_test_profile(300)),
    );
    assert!(!disarm);
}

#[test]
fn pd_ready_transition_discards_power_wait_heater_arm_until_rearmed() {
    let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
    ui_state.set_dashboard_presentation(
        flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
    );
    let mut calibration_runtime_state = CalibrationRuntimeState::default();

    assert!(ui_state.handle_event(KeyEvent {
        raw_key: RawFrontPanelKey::CenterBoot,
        key: FrontPanelKey::Center,
        gesture: KeyGesture::ShortPress,
        at_ms: 0,
    }));
    assert!(ui_state.heater_enabled);

    assert!(disarm_stale_heater_arm_after_pd_transition(
        false,
        true,
        &mut ui_state,
        &mut calibration_runtime_state,
    ));
    assert!(!ui_state.heater_enabled);
    assert!(!calibration_runtime_state.heater_enabled);
    assert!(!reconcile_runtime_heater_enabled(
        ui_state.heater_enabled,
        calibration_runtime_state,
        None,
        false,
        false,
        true,
        true,
    ));

    assert!(ui_state.handle_event(KeyEvent {
        raw_key: RawFrontPanelKey::CenterBoot,
        key: FrontPanelKey::Center,
        gesture: KeyGesture::ShortPress,
        at_ms: 1,
    }));
    assert!(ui_state.heater_enabled);
    assert!(reconcile_runtime_heater_enabled(
        ui_state.heater_enabled,
        calibration_runtime_state,
        None,
        false,
        false,
        true,
        true,
    ));
}

#[test]
fn runtime_heater_reconcile_applies_calibration_gate_when_mode_is_active() {
    let desired = reconcile_runtime_heater_enabled(
        true,
        CalibrationRuntimeState {
            mode: CalibrationMode::RtdAdc,
            heater_enabled: false,
            ..CalibrationRuntimeState::default()
        },
        None,
        false,
        false,
        true,
        true,
    );

    assert!(!desired);
}

#[test]
fn failed_thermal_plant_calibration_keeps_heating_locked() {
    let calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Failed,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };

    assert!(!thermal_model_heater_allowed(
        &MemoryConfig::default(),
        calibration,
        ManualPpsState::default(),
    ));
    assert!(!reconcile_runtime_heater_enabled(
        true,
        calibration,
        None,
        false,
        false,
        true,
        true,
    ));
}

#[test]
fn thermal_plant_timeout_disarms_without_committing_partial_model() {
    let mut calibration = CalibrationRuntimeState::default();
    let mut memory_config = thermal_plant_job_memory_config(20_000, 3_000);
    let mut manual_pps = thermal_plant_job_manual_pps(20_000, 3_000);
    let mut workspace = CalibrationThermalPlantWorkspace::default();
    calibration_job_start_with_workspace(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut memory_config,
        &mut manual_pps,
        &mut workspace,
    )
    .unwrap();
    workspace.job.as_mut().unwrap().started_at_ms = Some(0);

    update_thermal_plant_calibration_job(
        &mut calibration,
        &mut memory_config,
        &mut manual_pps,
        &mut workspace,
        CalibrationJobUpdateInput {
            now_ms: (u64::from(THERMAL_PLANT_TOTAL_TIMEOUT_TICKS) + 1) * HEATER_CONTROL_INTERVAL_MS,
            latest_rtd_raw_code: 500,
            latest_vin_raw_code: 1_000,
            wifi_started: Some(false),
            pd_confirmed: true,
            latest_rtd_raw_adc_mv: 250,
            latest_vin_raw_adc_mv: 500,
            latest_temp_c: 25.0,
            latest_vin_mv: 10_000,
            heater_duty_percent: 100,
        },
    );

    assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
    assert_eq!(
        calibration.job.message,
        Some(ManualPpsError::ThermalPlantTimeout)
    );
    assert_eq!(calibration.mode, CalibrationMode::Off);
    assert!(!calibration.heater_enabled);
    assert!(calibration.immediate_heater_disarm_pending);
    assert!(!manual_pps.enabled);
    assert!(memory_config.thermal_plant_transient_active.is_none());
}

#[test]
fn legacy_steady_state_record_never_unlocks_heating() {
    let memory_config = MemoryConfig {
        thermal_plant_active: Some(ThermalPlantRawTransaction {
            transaction_id: 7,
            anchors: [
                ThermalPlantRawAnchor {
                    ambient_raw_rtd_adc_mv: 250,
                    target_raw_rtd_adc_mv: 700,
                    heater_voltage_mv: 20_000,
                    heater_current_ma: 3_000,
                    gate_off_idle_power_mw: 0,
                    steady_hold_power_mw: 1_000,
                    ramp_duration_ms: 1_000,
                    ramp_energy_mj: 1_000,
                },
                ThermalPlantRawAnchor {
                    ambient_raw_rtd_adc_mv: 250,
                    target_raw_rtd_adc_mv: 2_000,
                    heater_voltage_mv: 20_000,
                    heater_current_ma: 3_000,
                    gate_off_idle_power_mw: 0,
                    steady_hold_power_mw: 2_000,
                    ramp_duration_ms: 2_000,
                    ramp_energy_mj: 2_000,
                },
            ],
        }),
        ..MemoryConfig::default()
    };
    let manual_pps = ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(5_000),
        pps_max_mv: Some(20_000),
        pps_max_ma: Some(5_000),
        ..Default::default()
    }));

    assert!(!thermal_model_heater_allowed(
        &memory_config,
        CalibrationRuntimeState::default(),
        manual_pps,
    ));
}

#[test]
fn thermal_plant_completion_disarm_is_consumed_once() {
    let mut calibration = CalibrationRuntimeState {
        thermal_plant_completion_disarm_pending: true,
        ..CalibrationRuntimeState::default()
    };
    let mut desired_heater_enabled =
        reconcile_runtime_heater_enabled(true, calibration, None, false, false, true, true);
    desired_heater_enabled =
        consume_thermal_plant_completion_disarm(&mut calibration, desired_heater_enabled);
    assert!(!desired_heater_enabled);
    assert!(reconcile_runtime_heater_enabled(
        true,
        calibration,
        None,
        false,
        false,
        true,
        true,
    ));
}

#[test]
fn immediate_heater_disarm_is_consumed_once() {
    let mut calibration = CalibrationRuntimeState {
        immediate_heater_disarm_pending: true,
        ..CalibrationRuntimeState::default()
    };

    assert!(take_immediate_heater_disarm(&mut calibration));
    assert!(!take_immediate_heater_disarm(&mut calibration));
}

#[test]
fn terminal_disarm_locks_pps_until_fixed_pd_write_completes() {
    let calibration = CalibrationRuntimeState {
        immediate_heater_disarm_pending: true,
        ..CalibrationRuntimeState::default()
    };
    let mut backend = select_heater_power_backend(
        Some(ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            ..Default::default()
        }),
        Some(Status::default()),
    );

    assert!(latch_terminal_fixed_pd_disarm(&calibration, &mut backend));
    assert!(calibration.immediate_heater_disarm_pending);
    assert!(matches!(
        backend,
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed: true,
            ..
        }
    ));
}

#[test]
fn tuning_cooldown_releases_completed_calibration_power_lock_without_heating() {
    for capability_ma in [3_000, 5_000] {
        let mut backend = select_heater_power_backend(
            Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(capability_ma),
                ..Default::default()
            }),
            Some(Status::default()),
        );
        assert!(latch_terminal_fixed_pd_disarm(
            &CalibrationRuntimeState {
                immediate_heater_disarm_pending: true,
                ..Default::default()
            },
            &mut backend,
        ));
        assert!(!release_terminal_fixed_pd_disarm_for_new_work(
            &mut backend,
            false,
            false,
            false,
        ));
        assert!(backend.terminal_fixed_pd_disarmed());
        assert!(release_terminal_fixed_pd_disarm_for_new_work(
            &mut backend,
            false,
            false,
            true,
        ));
        let HeaterPowerBackend::PpsMos {
            current_mode,
            current_request_mv,
            idle_request_mv,
            capability_max_ma,
            terminal_fixed_pd_disarmed,
            ..
        } = backend
        else {
            panic!("both source classes must retain the shared PPS backend");
        };
        assert!(!terminal_fixed_pd_disarmed);
        assert_eq!(current_mode, None);
        assert_eq!(capability_max_ma, capability_ma);
        assert!(!automatic_heater_power_is_idle(false, 0, false, true));
        let request_mv = automatic_pps_request_mv(
            0,
            false,
            true,
            current_request_mv,
            idle_request_mv,
            5_500,
            21_000,
        );
        assert!(request_mv > idle_request_mv);
        assert_eq!(
            live_pps_gate_duty_percent(0, false, true, 21_000, request_mv, 0),
            0
        );
        let (mut fixed, _) = production_power_fixture(capability_ma);
        let active = fixed.active.as_mut().unwrap();
        active.mode = PdContractRequestMode::Fixed;
        active.voltage_mv = 5_000;
        active.operating_current_ma = 1_000;
        assert_eq!(
            tuning_pd_wait_decision(fixed, None, 0, None, false),
            TuningPdWaitDecision::Wait
        );
        let (ready, _) = production_power_fixture(capability_ma);
        assert_eq!(
            tuning_pd_wait_decision(ready, Some(0), 100, None, false),
            TuningPdWaitDecision::Ready
        );
    }
}

#[test]
fn manual_pps_calibration_releases_terminal_disarm_without_enabling_the_heater() {
    let mut backend = HeaterPowerBackend::PpsMos {
        pps_min_mv: 5_000,
        idle_request_mv: 12_000,
        pps_max_mv: 21_000,
        adjustable_max_mv: 21_000,
        capability_max_ma: 5_000,
        current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
        current_request_mv: 20_000,
        settle_until_ms: Some(100),
        next_request_at_ms: 100,
        current_limit_fixed_pwm_active: true,
        current_limit_fixed_request_confirmed: true,
        terminal_fixed_pd_disarmed: true,
    };

    assert!(release_terminal_fixed_pd_disarm_for_new_work(
        &mut backend,
        false,
        true,
        false
    ));
    assert!(matches!(
        backend,
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed: false,
            current_mode: None,
            current_request_mv: 12_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            ..
        }
    ));
}

#[test]
fn manual_pps_releases_terminal_disarm_after_fixed_pd_fallback() {
    let mut backend = HeaterPowerBackend::FixedPdPwmFallback {
        reason: HeaterPowerBackendReason::NoPps20vCapability,
        fixed_request_confirmed: true,
        fixed_request: ch224q::VoltageRequest::V20,
        terminal_fixed_pd_disarmed: true,
    };

    assert!(release_terminal_fixed_pd_disarm_for_new_work(
        &mut backend,
        false,
        true,
        false
    ));
    assert_eq!(
        backend,
        HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: false,
            fixed_request: ch224q::VoltageRequest::V20,
            terminal_fixed_pd_disarmed: false,
        }
    );
}

#[test]
fn fusb302b_retries_manual_pps_when_the_active_contract_is_fixed() {
    let manual_pps = ManualPpsState {
        enabled: true,
        owner: ManualPpsOwner::Calibration,
        target_mv: Some(20_000),
        target_ma: Some(5_000),
        applied_mv: Some(20_000),
        ..ManualPpsState::default()
    };
    let fixed = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: 5_000,
        contract_voltage_mv: Some(20_000),
        contract: Contract::observed(ContractKind::Fixed, 20_000, 5_000),
    };
    let pps = PdStatusObservation {
        contract: Contract::observed(ContractKind::Pps, 20_000, 5_000),
        ..fixed
    };

    assert!(manual_pps_request_required(
        manual_pps,
        ControllerKind::Fusb302b,
        Some(fixed)
    ));
    assert!(!manual_pps_request_required(
        manual_pps,
        ControllerKind::Fusb302b,
        Some(pps)
    ));
    assert!(!manual_pps_request_required(
        manual_pps,
        ControllerKind::Ch224q,
        Some(fixed)
    ));
}

#[test]
fn terminal_disarm_requires_protocol_confirmed_fixed_power() {
    let observation = PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: 1_000,
        contract_voltage_mv: Some(5_000),
        contract: Contract::observed(ContractKind::Fixed, 5_000, 1_000),
    };
    assert!(terminal_idle_contract_confirmed(Some(observation)));
    assert!(!terminal_idle_contract_confirmed(None));
    assert!(!terminal_idle_contract_confirmed(Some(
        PdStatusObservation {
            contract: Contract::observed(ContractKind::Pps, 5_000, 1_000),
            ..observation
        }
    )));
    assert!(!terminal_idle_contract_confirmed(Some(
        PdStatusObservation {
            contract: Contract::observed(ContractKind::Fixed, 5_000, 900),
            ..observation
        }
    )));
    assert!(terminal_idle_contract_confirmed(Some(
        PdStatusObservation {
            contract: Contract::observed(ContractKind::Fixed, 9_000, 560),
            contract_voltage_mv: Some(9_000),
            current_ma: 560,
            ..observation
        }
    )));
}

#[test]
fn cooling_waits_for_working_power_and_retains_confirmed_pps() {
    let observation = |kind, mv, ma| PdStatusObservation {
        status_raw: FUSB302B_STATUS0_VBUSOK,
        status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
        current_raw: 0,
        current_ma: ma,
        contract_voltage_mv: Some(mv),
        contract: Contract::observed(kind, mv, ma),
    };
    assert!(!fan_working_contract_confirmed(None, false));
    assert!(!fan_working_contract_confirmed(
        Some(observation(ContractKind::Fixed, 5_000, 1_000)),
        false
    ));
    assert!(fan_working_contract_confirmed(
        Some(observation(ContractKind::Pps, 12_000, 3_000)),
        false
    ));
    assert!(!fan_working_contract_confirmed(
        Some(observation(ContractKind::Fixed, 12_000, 410)),
        false
    ));
    assert!(fan_working_contract_confirmed(
        Some(observation(ContractKind::Fixed, 12_000, 420)),
        false
    ));
    assert!(fan_working_contract_confirmed(
        Some(observation(ContractKind::Pps, 21_000, 3_000)),
        true
    ));
    assert!(!fan_working_contract_confirmed(
        Some(observation(ContractKind::Fixed, 5_000, 1_000)),
        true
    ));
}

#[test]
fn flash_preparation_cancels_manual_calibration_without_replaying_it() {
    for mode in [
        CalibrationMode::VinAdc,
        CalibrationMode::RtdAdc,
        CalibrationMode::HeaterCurve,
    ] {
        let mut calibration = CalibrationRuntimeState {
            mode,
            heater_enabled: true,
            pps_enabled: true,
            pps_mv: Some(8_000),
            pps_ma: Some(3_000),
            target_adc_mv: Some(1_000),
            ..CalibrationRuntimeState::default()
        };
        let mut manual_pps = ManualPpsState::default();
        cancel_calibration_for_flash_preparation(&mut calibration, &mut manual_pps);
        assert_eq!(calibration.mode, CalibrationMode::Off);
        assert!(!calibration.heater_enabled);
        assert!(!calibration.pps_enabled);
        assert!(calibration.target_adc_mv.is_none());
        assert!(calibration.immediate_heater_disarm_pending);
    }
}

#[test]
fn canceling_a_running_job_latches_immediate_disarm_and_preserves_terminal_state() {
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        heater_enabled: true,
        job: CalibrationJobState {
            kind: Some(CalibrationJobKind::ThermalPlant),
            status: CalibrationJobStatus::Running,
            ..CalibrationJobState::default()
        },
        ..CalibrationRuntimeState::default()
    };
    let mut manual_pps = ManualPpsState::default();

    calibration_job_canceled(&mut calibration, &mut manual_pps);

    assert_eq!(calibration.job.status, CalibrationJobStatus::Canceled);
    assert_eq!(calibration.mode, CalibrationMode::Off);
    assert!(!calibration.heater_enabled);
    assert!(take_immediate_heater_disarm(&mut calibration));

    calibration.job.status = CalibrationJobStatus::Completed;
    calibration_job_canceled(&mut calibration, &mut manual_pps);
    assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
    assert!(!take_immediate_heater_disarm(&mut calibration));
}

#[test]
fn automatic_supply_response_fault_cannot_retry_on_unloaded_voltage_recovery() {
    assert!(automatic_supply_response_fault_latched(
        false, true, true, 100, false
    ));
    assert!(automatic_supply_response_fault_latched(
        true, true, true, 0, true
    ));
    assert!(!automatic_supply_response_fault_latched(
        true, true, false, 0, true
    ));
    assert!(!automatic_supply_response_fault_latched(
        true, false, true, 100, false
    ));
    assert!(!automatic_supply_response_fault_latched(
        false, true, true, 0, false
    ));
}

#[test]
fn automatic_pps_contract_safety_uses_its_own_apdo_and_current_budget() {
    let mut manual = thermal_plant_job_manual_pps(21_000, 5_000);
    manual.capability_apdos[1] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 11_000,
        max_ma: 3_000,
    });
    let config = MemoryConfig::default();
    let actual_range = PpsAdjustmentRange {
        min_mv: 5_500,
        max_mv: 11_000,
        max_current_ma: 3_000,
    };
    let candidate = manual
        .automatic_heater_pps_for_contract(20.0, &config, 9_000, 3_000, Some(actual_range))
        .unwrap();
    assert_eq!(candidate.advertisement_index, 1);
    assert_eq!(candidate.current_ma, 3_000);
    assert_eq!(candidate.allowed_mv, 9_600);
    assert!(
        manual
            .automatic_heater_pps_for_contract(20.0, &config, 9_000, 2_950, Some(actual_range))
            .is_none()
    );
    assert!(
        manual
            .automatic_heater_pps_for_contract(20.0, &config, 12_000, 3_000, Some(actual_range))
            .is_none()
    );
}

fn automatic_boundary_request_mv(
    boundary: &flux_purr_firmware::heater_supply::SupplyBoundary,
    config: &MemoryConfig,
    current_rtd_mv: Option<u16>,
    current_request_mv: Option<u16>,
) -> Option<u16> {
    boundary
        .safe_point(
            flux_purr_firmware::heater_supply::BoundaryQuery {
                temperature_centi_c: 8_200,
                current_rtd_mv,
                current_request_mv,
                current_ma: 3_000,
                wifi_started: false,
                source_min_mv: 3_300,
                source_max_mv: 21_000,
                source_max_ma: 3_000,
                source_power_limit_mw: 0,
            },
            |raw| {
                projected_rtd_temperature_c(config, raw)
                    .map(|temperature| round_to_i16(temperature * 100.0))
            },
        )
        .map(|point| point.tracking_request_mv)
}

#[test]
fn automatic_voltage_response_uses_the_confirmed_warm_boundary_point() {
    use flux_purr_firmware::heater_supply::{BoundaryAdcMetadata, BoundaryPoint, SupplyBoundary};

    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 3_300,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    let mut boundary = SupplyBoundary {
        adc: BoundaryAdcMetadata {
            divider_supply_mv: RTD_DIVIDER_SUPPLY_MV,
            ..Default::default()
        },
        wifi_started: false,
        source_min_mv: 5_500,
        source_max_mv: 20_000,
        source_max_ma: 3_000,
        advertised_min_mv: 3_300,
        advertised_max_mv: 21_000,
        point_count: 2,
        ..Default::default()
    };
    boundary.points[0] = BoundaryPoint {
        elapsed_ms: 1_000,
        rtd_code: 1_500,
        rtd_mv: runtime_test_raw_rtd_adc_mv_for_temp(70.0),
        vin_code: 1_100,
        vin_mv: 1_113,
        request_current_ma: 3_000,
        tracking_request_mv: 13_900,
        tracking_only: true,
        ..Default::default()
    };
    boundary.points[1] = BoundaryPoint {
        elapsed_ms: 2_000,
        rtd_code: 1_600,
        rtd_mv: runtime_test_raw_rtd_adc_mv_for_temp(85.0),
        vin_code: 1_250,
        vin_mv: 1_250,
        request_current_ma: 3_000,
        tracking_request_mv: 16_500,
        tracking_only: true,
        ..Default::default()
    };
    assert!(boundary.is_valid());
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));

    let confirmed_range = Some(PpsAdjustmentRange {
        min_mv: 3_300,
        max_mv: 21_000,
        max_current_ma: 3_000,
    });
    let current_rtd_mv = Some(runtime_test_raw_rtd_adc_mv_for_temp(82.0));
    assert_eq!(
        automatic_boundary_request_mv(&boundary, &config, current_rtd_mv, None),
        Some(13_900)
    );
    assert_eq!(
        automatic_boundary_request_mv(&boundary, &config, current_rtd_mv, Some(16_500)),
        Some(16_500)
    );
    assert!(manual.automatic_voltage_response_matches_with_rtd(
        &config,
        82.0,
        16_500,
        3_000,
        1_190,
        AutomaticVoltageResponseContext {
            confirmed_range,
            current_rtd_mv,
        },
    ));
}

#[test]
fn live_boundary_points_keep_a_confirmed_high_voltage_request_authorized() {
    use flux_purr_firmware::heater_supply::{BoundaryAdcMetadata, BoundaryPoint, SupplyBoundary};

    let mut boundary = SupplyBoundary {
        adc: BoundaryAdcMetadata {
            divider_supply_mv: RTD_DIVIDER_SUPPLY_MV,
            ..Default::default()
        },
        wifi_started: false,
        source_min_mv: 5_500,
        source_max_mv: 20_000,
        source_max_ma: 3_000,
        advertised_min_mv: 3_300,
        advertised_max_mv: 21_000,
        point_count: 9,
        ..Default::default()
    };
    for (slot, (rtd_mv, vin_mv, request_mv)) in [
        (1_030, 997, 10_300),
        (1_141, 1_136, 13_900),
        (1_089, 1_089, 13_900),
        (1_111, 1_111, 13_900),
        (1_111, 1_111, 13_900),
        (1_112, 1_112, 13_900),
        (1_113, 1_113, 13_900),
        (1_361, 1_361, 16_500),
        (1_330, 1_330, 16_500),
    ]
    .into_iter()
    .enumerate()
    {
        boundary.points[slot] = BoundaryPoint {
            elapsed_ms: (slot as u32 + 1) * 50,
            rtd_code: 1_000,
            rtd_mv,
            vin_code: 1_000,
            vin_mv,
            request_current_ma: 3_000,
            tracking_request_mv: request_mv,
            tracking_only: true,
            ..Default::default()
        };
    }
    assert!(boundary.is_valid());
    let config = MemoryConfig {
        heater_supply_boundary: Some(boundary),
        ..MemoryConfig::default()
    };
    flux_purr_firmware::heater_supply::publish_wifi_hardware_state(Some(false));
    let mut manual = thermal_plant_job_manual_pps(21_000, 3_000);
    manual.capability_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: 5_500,
        max_mv: 21_000,
        max_ma: 3_000,
    });
    println!(
        "live candidate 79.88C/req13.9: {:?}",
        manual.automatic_heater_pps_candidate_with_rtd(
            79.88,
            None,
            &config,
            Some(13_900),
            Some(1_146),
        )
    );
    println!(
        "live candidate 79.88C/req15.4: {:?}",
        manual.automatic_heater_pps_candidate_with_rtd(
            79.88,
            None,
            &config,
            Some(15_400),
            Some(1_146),
        )
    );
    let query = |temperature_centi_c, current_rtd_mv, current_request_mv| {
        boundary
            .safe_point(
                flux_purr_firmware::heater_supply::BoundaryQuery {
                    temperature_centi_c,
                    current_rtd_mv: Some(current_rtd_mv),
                    current_request_mv: Some(current_request_mv),
                    current_ma: 3_000,
                    wifi_started: false,
                    source_min_mv: 3_300,
                    source_max_mv: 21_000,
                    source_max_ma: 3_000,
                    source_power_limit_mw: 0,
                },
                |raw| {
                    projected_rtd_temperature_c(&config, raw)
                        .map(|temperature| round_to_i16(temperature * 100.0))
                },
            )
            .map(|point| point.tracking_request_mv)
    };
    assert_eq!(query(7_722, 1_141, 13_900), Some(13_900));
    assert_eq!(query(7_988, 1_146, 15_400), None);
    assert_eq!(query(8_200, 1_150, 15_400), None);
}

#[test]
fn diagnose_real_failed_thermal_plant_trace_fit() {
    let trace = [
        (3500, 25.39, 19_750, 0),
        (4950, 26.20, 5_375, 100),
        (8000, 24.19, 5_750, 100),
        (13_100, 26.60, 6_250, 100),
        (19_800, 27.00, 7_250, 100),
        (25_000, 27.81, 7_875, 100),
        (30_550, 28.21, 8_500, 100),
        (33_600, 28.61, 8_625, 100),
        (35_500, 28.61, 9_125, 100),
        (39_000, 28.61, 9_250, 100),
        (43_300, 29.02, 10_125, 100),
        (56_100, 31.04, 11_500, 100),
        (69_400, 33.49, 12_875, 100),
        (74_150, 38.42, 13_000, 100),
        (74_400, 38.84, 12_875, 100),
        (77_200, 43.00, 13_000, 100),
        (80_600, 47.20, 13_000, 100),
        (84_300, 51.45, 13_000, 100),
        (87_400, 55.74, 13_000, 100),
        (91_450, 60.07, 13_000, 100),
        (95_500, 64.45, 13_000, 100),
        (99_600, 68.88, 13_000, 100),
        (103_700, 72.90, 13_000, 100),
        (107_300, 76.96, 13_000, 100),
        (112_200, 81.05, 13_000, 100),
        (116_600, 85.19, 13_000, 100),
        (121_800, 89.37, 13_000, 100),
        (127_700, 93.59, 13_000, 100),
        (134_000, 97.84, 13_000, 100),
        (171_950, 104.07, 16_000, 100),
        (173_400, 105.52, 16_000, 0),
        (173_500, 105.52, 0, 0),
        (174_050, 105.52, 0, 0),
        (174_550, 105.52, 0, 0),
        (175_100, 105.52, 0, 0),
        (175_650, 105.52, 0, 0),
        (176_200, 104.55, 0, 0),
        (176_750, 104.07, 0, 0),
        (177_300, 103.59, 0, 0),
        (177_800, 103.11, 0, 0),
        (178_300, 102.63, 0, 0),
        (178_850, 101.66, 0, 0),
        (179_400, 101.19, 0, 0),
        (179_950, 100.71, 0, 0),
        (180_450, 99.75, 0, 0),
        (180_950, 99.27, 0, 0),
        (181_450, 98.80, 0, 0),
        (181_950, 97.84, 0, 0),
        (182_500, 97.37, 0, 0),
        (183_000, 96.89, 0, 0),
        (186_450, 92.65, 0, 0),
        (189_800, 88.44, 0, 0),
        (193_750, 84.27, 0, 0),
        (197_750, 80.14, 0, 0),
        (198_300, 79.68, 0, 0),
    ];
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, (elapsed_ms, temperature_c, voltage_mv, duty_percent)) in
        trace.into_iter().enumerate()
    {
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: elapsed_ms / HEATER_CONTROL_INTERVAL_MS as u32,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
            heater_voltage_125mv: if index >= 31 {
                flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL
            } else {
                (voltage_mv / 125) as u8
            },
            duty_percent,
        };
    }
    let memory = MemoryConfig::default();
    let prepared = prepare_transient_fit(
        &samples,
        trace.len() as u8,
        None,
        &memory,
        runtime_test_raw_rtd_adc_mv_for_temp(25.39),
    );
    let data = prepared.expect("the captured trace must pass structural preparation");
    let (_, full_residual) =
        best_transient_projection(&data).expect("stable powered windows must fit the trace");
    let (_, convection_residual) = best_transient_projection_without_radiation(&data)
        .expect("stable powered windows must fit without a radiation term");
    assert!(full_residual <= 0.20, "full residual={full_residual}");
    assert!(
        convection_residual <= 0.20,
        "convection-only residual={convection_residual}"
    );
}

#[test]
fn transient_fit_replays_a_real_failed_hil_trace_after_quantized_voltage_filter() {
    let points: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../testdata/thermal-plant-a-repaired-hil-trace.json"
    ))
    .expect("HIL fixture is JSON");
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, point) in points.iter().enumerate() {
        let phase = point["phase"].as_str().unwrap_or_default();
        let voltage_mv = point["heaterVoltageMv"].as_u64().unwrap_or_default() as u16;
        let temperature_c = point["temperatureCentiC"].as_f64().unwrap_or_default() as f32 / 100.0;
        let raw_rtd_adc_mv = point["rawRtdAdcMv"]
            .as_u64()
            .map(|value| value as u16)
            .unwrap_or_else(|| runtime_test_raw_rtd_adc_mv_for_temp(temperature_c));
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: point["elapsedMs"].as_u64().unwrap_or_default() as u32
                / HEATER_CONTROL_INTERVAL_MS as u32,
            raw_rtd_adc_mv,
            heater_voltage_125mv: if phase == "cooling" {
                flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL
            } else {
                (voltage_mv / 125) as u8
            },
            duty_percent: point["dutyPercent"].as_u64().unwrap_or_default() as u8,
        };
    }
    let memory = MemoryConfig::default();
    let data = prepare_transient_fit(
        &samples,
        points.len() as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .expect("HIL fixture passes structural preparation");
    let (_, full_residual) =
        best_transient_projection(&data).expect("HIL fixture produces a physical model");
    let (_, convection_residual) = best_transient_projection_without_radiation(&data)
        .expect("HIL fixture produces a convection-only model");
    assert!(full_residual <= 0.20, "full residual={full_residual}");
    assert!(
        convection_residual <= 0.20,
        "convection-only residual={convection_residual}"
    );
}

#[test]
fn transient_fit_replays_the_latest_failed_hil_trace() {
    let points: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../testdata/thermal-plant-latest-hil-trace.json"
    ))
    .expect("HIL fixture is JSON");
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, point) in points.iter().enumerate() {
        let phase = point["phase"].as_str().unwrap_or_default();
        let voltage_mv = point["heaterVoltageMv"].as_u64().unwrap_or_default() as u16;
        let temperature_c = point["temperatureCentiC"].as_f64().unwrap_or_default() as f32 / 100.0;
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: point["elapsedMs"].as_u64().unwrap_or_default() as u32
                / HEATER_CONTROL_INTERVAL_MS as u32,
            raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
            heater_voltage_125mv: if phase == "cooling" {
                flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL
            } else {
                (voltage_mv / 125) as u8
            },
            duty_percent: point["dutyPercent"].as_u64().unwrap_or_default() as u8,
        };
    }
    let memory = MemoryConfig::default();
    let data = prepare_transient_fit(
        &samples,
        points.len() as u8,
        None,
        &memory,
        samples[0].raw_rtd_adc_mv,
    )
    .expect("latest HIL fixture passes structural preparation");
    let full = best_transient_projection(&data);
    let convection_only = best_transient_projection_without_radiation(&data);
    assert!(full.is_some(), "latest HIL fixture must fit physically");
    assert!(
        convection_only.is_some(),
        "latest HIL fixture must fit without radiation"
    );
    assert!(full.is_some_and(|(_, residual)| residual <= 0.20));
    assert!(convection_only.is_some_and(|(_, residual)| residual <= 0.20));
}

#[test]
fn transient_fit_replays_the_completed_final_hil_trace() {
    let snapshot: serde_json::Value = serde_json::from_str(include_str!(
        "../../../testdata/thermal-plant-final-hil-replay.json"
    ))
    .expect("HIL fixture is JSON");
    let points = snapshot["tracePage"]["points"]
        .as_array()
        .expect("HIL fixture contains trace points");
    let ambient_raw_rtd_adc_mv = snapshot["attempt"]["ambientRawRtdAdcMv"]
        .as_u64()
        .expect("HIL fixture contains the ambient raw ADC baseline")
        as u16;
    let mut samples = [ThermalPlantTransientSample {
        elapsed_ticks: 0,
        raw_rtd_adc_mv: 0,
        heater_voltage_125mv: 0,
        duty_percent: 0,
    }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    for (index, point) in points.iter().enumerate() {
        let phase = point["phase"].as_str().unwrap_or_default();
        let voltage_mv = point["heaterVoltageMv"].as_u64().unwrap_or_default() as u16;
        let temperature_c = point["temperatureCentiC"].as_f64().unwrap_or_default() as f32 / 100.0;
        let raw_rtd_adc_mv = point["rawRtdAdcMv"]
            .as_u64()
            .map(|value| value as u16)
            .unwrap_or_else(|| runtime_test_raw_rtd_adc_mv_for_temp(temperature_c));
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks: point["elapsedMs"].as_u64().unwrap_or_default() as u32
                / HEATER_CONTROL_INTERVAL_MS as u32,
            raw_rtd_adc_mv,
            heater_voltage_125mv: if phase == "cooling" {
                flux_purr_firmware::memory::THERMAL_PLANT_FAN_COOLING_VOLTAGE_SENTINEL
            } else {
                (voltage_mv / 125) as u8
            },
            duty_percent: point["dutyPercent"].as_u64().unwrap_or_default() as u8,
        };
    }
    let memory = MemoryConfig::default();
    let data = prepare_transient_fit(
        &samples,
        points.len() as u8,
        None,
        &memory,
        ambient_raw_rtd_adc_mv,
    )
    .expect("completed HIL fixture passes structural preparation");
    let (projection, residual) = best_transient_projection_without_radiation(&data)
        .or_else(|| best_transient_projection(&data))
        .expect("completed HIL fixture produces a model");
    eprintln!(
        "completed HIL replay candidate: residual={residual:.6}, capacity={:.3}, convection={:.3}, radiation={:.3}, delay_ms={}",
        projection.thermal_capacity_mj_per_c,
        projection.convection_mw_per_c,
        projection.radiation_mw_per_k4,
        projection.transport_delay_ms
    );
    assert!(residual <= 0.20, "completed HIL residual={residual}");
    assert!((500..=2_000).contains(&projection.transport_delay_ms));
    assert!((20_000.0..=40_000.0).contains(&projection.thermal_capacity_mj_per_c));
    assert!((0.0..=1_000.0).contains(&projection.convection_mw_per_c));
    let device_projection = &snapshot["activeResult"];
    assert_eq!(device_projection["transportDelayMs"].as_u64(), Some(1_750));
    assert!(
        (device_projection["thermalCapacityMjPerC"]
            .as_f64()
            .unwrap_or_default()
            - 34_750.246)
            .abs()
            < 2_000.0
    );
    assert!(
        (device_projection["convectionMwPerC"]
            .as_f64()
            .unwrap_or_default()
            - 79.986046)
            .abs()
            < 100.0
    );
    eprintln!(
        "completed HIL replay: residual={residual:.6}, capacity={:.3}, convection={:.3}, radiation={:.3}, delay_ms={}",
        projection.thermal_capacity_mj_per_c,
        projection.convection_mw_per_c,
        projection.radiation_mw_per_k4,
        projection.transport_delay_ms
    );
}

#[test]
fn automatic_relative_supply_response_waits_for_matching_settled_voltage() {
    assert!(!automatic_supply_response_sample_ready(
        9_600, 9_700, 1_000, false, 2_000
    ));
    assert!(!automatic_supply_response_sample_ready(
        9_700, 9_700, 1_000, true, 2_000
    ));
    assert!(!automatic_supply_response_sample_ready(
        9_700, 9_700, 2_000, false, 1_950
    ));
    assert!(automatic_supply_response_sample_ready(
        9_700, 9_700, 2_000, false, 2_000
    ));
    assert!(!automatic_supply_response_fault_latched(
        false, true, true, 52, false
    ));
}
