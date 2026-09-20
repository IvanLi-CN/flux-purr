use super::*;

fn pps_source_capability(min_mv: u16, max_mv: u16, max_ma: u16) -> u32 {
    (0b11 << 30)
        | (u32::from(max_mv / 100) << 17)
        | (u32::from(min_mv / 100) << 8)
        | u32::from(max_ma / 50)
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
fn pd_service_is_owned_by_an_independent_normal_task() {
    let source = RUNTIME_IMPLEMENTATION;
    let support = include_str!("support.rs");
    let pd_service = include_str!("pd_service.rs");
    let runtime_loop = include_str!("runtime_loop.rs");

    assert!(
        pd_service.contains("#[embassy_executor::task]\nasync fn pd_service_task"),
        "PD policy must have a dedicated Embassy task"
    );
    assert!(
        !source.contains("PD_REALTIME_EXECUTOR")
            && !source.contains("software_interrupt2")
            && !source.contains("Priority::Priority3"),
        "the full PD protocol must not run in an interrupt executor"
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
            && pd_service.contains("EmbassyTimer::after_millis(PD_SERVICE_TICK_MS).await"),
        "the PD task must own its normal-executor cadence timer"
    );
    let pd_task = pd_service
        .split("async fn pd_service_task")
        .nth(1)
        .and_then(|source| source.split("pub(crate) fn spawn_pd_service").next())
        .expect("PD task body must remain present");
    assert!(
        pd_task.contains("EmbassyTimer::after_millis(PD_SERVICE_TICK_MS).await"),
        "the normal PD task must yield between bounded service turns"
    );
    assert!(
        !source.contains("Spawner::for_current_executor")
            && include_str!("boot.rs").contains("initialize_boot_pd(")
            && include_str!("boot.rs").contains("boot system is available for PD initialization")
            && FIRMWARE_ENTRYPOINT.contains("runtime::run(spawner).await"),
        "the main task must start PD during direct boot"
    );
    let boot = include_str!("boot.rs");
    let pd_service = include_str!("pd_service.rs");
    assert!(
        !boot.contains("PD_REALTIME_EXECUTOR_STORAGE")
            && !boot.contains("software_interrupt2")
            && boot.contains("pub(crate) async fn initialize_boot_pd(")
            && boot.contains("spawner: Spawner,")
            && !pd_service.contains("PD_REALTIME_EXECUTOR_REF")
            && !pd_service.contains("pd_realtime_spawner()"),
        "PD service startup must use the normal executor spawner"
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
        pd_service.contains("if i2c.try_acquire()") && pd_service.contains("i2c.release()"),
        "a busy EEPROM transaction must make PD skip this turn and retry later"
    );
    assert!(
        !pd_service.contains("i2c.lock().await") && !support.contains("self.bus.lock().await"),
        "the PD path must not await the shared I2C mutex"
    );
}

#[test]
fn fusb302b_heater_observation_requires_ready_contract_and_vbus() {
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

    assert!(pd_service.contains("read_status().await.ok()?"));
    assert!(pd_service.contains("let observation = pd_status_observation(runtime, i2c).await"));
    assert!(pd_service.contains("publish_pd_snapshot(\n        runtime,\n        observation,"));
    assert!(pd_service.contains("publish_pd_service_turn(runtime, None, *pending, None, false)"));
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
        "pub(crate) const fn fusb302b_phy_config(auto_goodcrc: bool) -> PhyConfig {\n    PhyConfig {\n        pd_revision: PdRevision::Rev30,"
    ));
    assert!(!RUNTIME_IMPLEMENTATION.contains("pd_revision: PdRevision::Rev20"));
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
fn stale_pd_contract_requires_continuous_measured_vin_deficit() {
    let contract = Contract::observed(ContractKind::Pps, 12_000, 5_000);
    let observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    };
    let mut guard = PdContractVinGuard::default();

    assert!(!guard.observe(Some(observation), Some(5_000), 1_000, false));
    assert!(!guard.observe(Some(observation), Some(5_000), 1_099, false));
    assert!(guard.observe(Some(observation), Some(5_000), 1_100, false));
    assert!(!guard.observe(Some(observation), Some(11_000), 1_101, false));
    assert!(!guard.observe(Some(observation), Some(5_000), 1_200, false));
    assert!(guard.observe(Some(observation), Some(5_000), 1_300, false));
}

#[test]
fn stale_pd_contract_does_not_infer_loss_without_a_vin_sample() {
    let contract = Contract::observed(ContractKind::Fixed, 20_000, 3_000);
    let observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    };
    let mut guard = PdContractVinGuard::default();

    assert!(!guard.observe(Some(observation), None, 2_000, false));
    assert!(!guard.observe(Some(observation), None, 2_500, false));
    assert!(!guard.observe(None, Some(5_000), 3_000, false));
}

#[test]
fn stale_pd_contract_guard_waits_out_pd_request_settling() {
    let contract = Contract::observed(ContractKind::Pps, 20_000, 3_000);
    let observation = PdStatusObservation {
        status_raw: 1 << 3,
        status: Status::from_register(1 << 3),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    };
    let mut guard = PdContractVinGuard::default();

    assert!(!guard.observe(Some(observation), Some(5_000), 4_000, true));
    assert!(!guard.observe(Some(observation), Some(5_000), 4_500, true));
    assert!(!guard.observe(Some(observation), Some(5_000), 4_599, false));
    assert!(guard.observe(Some(observation), Some(5_000), 4_699, false));
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
        Some((5_500, 21_000, 5_000))
    );
    assert_eq!(
        adjustable_mode_for_request(24_000, 28_000),
        ch224q::AdjustableVoltageMode::Pps
    );
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
        Some((20_000, 25_500, 5_000))
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
fn automatic_idle_restore_never_confirms_a_fixed_twenty_volt_contract() {
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
    assert!(!automatic_idle_contract_is_confirmed(
        observation(20_000),
        None
    ));
}

#[test]
fn capability_refresh_ticket_always_settles_when_detached_or_faulted() {
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Detached, true, false),
        Some(TicketOutcome::Detached),
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Fault, true, false),
        Some(TicketOutcome::TransportFault),
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::WaitingForAccept, true, false),
        None,
    );
    assert_eq!(
        refresh_terminal_outcome(SinkPhase::Ready, false, true),
        Some(TicketOutcome::CapabilitiesRefreshed),
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
    assert!(!fusb302b_identity_is_stable(
        Some(0x81),
        Some(0x81),
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
fn usb_recovery_writer_times_out_instead_of_renewing_forever() {
    let payload = [b'r'; 96];
    let mut writer = UsbResponseWriter::default();
    writer.start(payload.len(), 10);
    let mut tx = FakeUsbTx::new(64);
    let tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

    assert_eq!(
        usb_pump_recovery_response(&mut tx, &mut writer, &tx_buf, 10),
        UsbResponsePumpOutcome::Fault
    );
    assert!(writer.is_complete());
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
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: vin_raw_mv,
                    latest_temp_c: 25.0,
                    pd_current_ma: 3_000,
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
                latest_rtd_raw_adc_mv: 0,
                latest_vin_raw_adc_mv: vin_raw_mv,
                latest_temp_c: 25.0,
                pd_current_ma: 3_000,
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
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: settled_raw_mv.saturating_sub(80),
                    latest_temp_c: 25.0,
                    pd_current_ma: 3_000,
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
                    latest_rtd_raw_adc_mv: 0,
                    latest_vin_raw_adc_mv: settled_raw_mv,
                    latest_temp_c: 25.0,
                    pd_current_ma: 3_000,
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
fn thermal_plant_auto_job_requests_the_selected_apdo_ceiling_for_3a_and_5a() {
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
        assert_eq!(calibration.pps_mv, Some(max_mv));
        assert_eq!(calibration.job.next_request_mv, Some(max_mv));
        assert_eq!(calibration.pps_ma, Some(max_ma));
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
    let mut calibration = CalibrationRuntimeState {
        mode: CalibrationMode::ThermalPlant,
        ..CalibrationRuntimeState::default()
    };
    let mut memory_config = thermal_plant_job_memory_config(max_mv, max_ma);
    let mut manual_pps = thermal_plant_job_manual_pps(max_mv, max_ma);
    calibration_job_start(
        &mut calibration,
        CalibrationJobKind::ThermalPlant,
        &mut memory_config,
        &mut manual_pps,
    )
    .unwrap();

    for _ in 0..THERMAL_PLANT_AMBIENT_TICKS {
        update_thermal_plant_job(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            ThermalPlantJobUpdate {
                raw_rtd_adc_mv: 250,
                temp_c: 25.0,
                max_mv,
                max_ma,
                heater_duty_percent: 0,
            },
        );
    }
    assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
    assert!(calibration.heater_enabled);
    assert_eq!(calibration.model_target_temp_c, None);
    assert_eq!(calibration.job_data, Some(CalibrationJobData::ThermalPlant));
    assert_eq!(
        test_thermal_plant_phase(),
        Some(ThermalPlantAutoPhase::Heating)
    );

    for (raw_rtd_adc_mv, temp_c) in [(400, 60.0), (700, 140.0), (1_100, 215.0)] {
        update_thermal_plant_job(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            ThermalPlantJobUpdate {
                raw_rtd_adc_mv,
                temp_c,
                max_mv,
                max_ma,
                heater_duty_percent: 100,
            },
        );
        assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
        assert_eq!(calibration.job.next_request_mv, Some(max_mv));
        assert_eq!(calibration.pps_mv, Some(max_mv));
        assert_eq!(
            thermal_plant_calibration_snapshot(temp_c, calibration.heater_enabled).duty_percent,
            100
        );
        assert_eq!(
            test_thermal_plant_phase(),
            Some(ThermalPlantAutoPhase::Heating)
        );
    }
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

struct ThermalPlantJobUpdate {
    raw_rtd_adc_mv: u16,
    temp_c: f32,
    max_mv: u16,
    max_ma: u16,
    heater_duty_percent: u8,
}

fn update_thermal_plant_job(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    update: ThermalPlantJobUpdate,
) {
    update_calibration_job_state(
        calibration,
        memory_config,
        manual_pps,
        CalibrationJobUpdateInput {
            latest_rtd_raw_adc_mv: update.raw_rtd_adc_mv,
            latest_vin_raw_adc_mv: 0,
            latest_temp_c: update.temp_c,
            pd_current_ma: update.max_ma,
            latest_vin_mv: update.max_mv.into(),
            heater_duty_percent: update.heater_duty_percent,
        },
    );
}
const SYNTHETIC_TARGET_MARGIN_C: f32 = 2.0;

fn runtime_test_raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
    let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
    (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
        / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
        .round() as u16
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
    config
}

fn synthetic_trace(
    memory_config: &MemoryConfig,
    ambient_temp_c: f32,
) -> (
    [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    usize,
) {
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
                elapsed_ticks: tick,
                raw_rtd_adc_mv: runtime_test_raw_rtd_adc_mv_for_temp(temperature_c),
                heater_voltage_125mv: if heating { 160 } else { 0 },
                duty_percent,
            };
            sample_count += 1;
            last_saved_temp_c = temperature_c;
        }
        if reached_cutoff {
            heating = false;
            last_saved_temp_c = f32::MIN;
            continue;
        }
        if !heating
            && temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C - SYNTHETIC_TARGET_MARGIN_C
        {
            break;
        }
        let resistance_ohms = estimated_heater_resistance_ohms(temperature_c, None, memory_config);
        let power_mw = if heating {
            20.0 * 20.0 / resistance_ohms * 1_000.0
        } else {
            0.0
        };
        let temperature_k = temperature_c + 273.15;
        let ambient_k = ambient_temp_c + 273.15;
        let losses_mw = convection_mw_per_c * (temperature_c - ambient_temp_c)
            + radiation_mw_per_k4 * (temperature_k.powi(4) - ambient_k.powi(4));
        temperature_c += (power_mw - losses_mw) / capacity_mj_per_c * 0.05;
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
    assert!(!thermal_model_heater_allowed(
        &unrelated_curve,
        CalibrationRuntimeState::default(),
        manual_pps
    ));
    let unrelated_wire = thermal_plant_runtime_wire(&unrelated_curve);
    assert_eq!(unrelated_wire.state.as_str(), "invalid");
    assert!(!unrelated_wire.projection_valid);
    assert!(thermal_plant_projection_for_runtime(&unrelated_curve).is_none());
    let mut missing_raw_curve = memory_config.clone();
    missing_raw_curve.heater_curve_raw_observations = HeaterCurveRawObservations::default();
    assert!(!thermal_model_heater_allowed(
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
    let persisted_transaction = decode_memory_record(&persisted_bytes[..persisted_len])
        .expect("transient record decodes")
        .config
        .thermal_plant_transient_active
        .expect("transient model persists");
    let persisted_last =
        persisted_transaction.samples[usize::from(persisted_transaction.sample_count) - 1];
    assert_eq!(persisted_last.duty_percent, 0);
    assert!(
        projected_rtd_temperature_c(memory_config, persisted_last.raw_rtd_adc_mv)
            .is_some_and(|temperature_c| temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C)
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
        heater_voltage_125mv: 0,
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

    let mut below_target = transaction;
    let mut saw_powered_sample = false;
    for sample in below_target.samples[..usize::from(below_target.sample_count)].iter_mut() {
        if sample.duty_percent > 0 {
            if saw_powered_sample {
                sample.raw_rtd_adc_mv = runtime_test_raw_rtd_adc_mv_for_temp(215.0);
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
fn transient_thermal_fit_recovers_a_physical_model_from_heat_and_cool_trace() {
    let ambient_temp_c = 25.0_f32;
    let mut memory_config = synthetic_memory_config();
    let (samples, sample_count) = synthetic_trace(&memory_config, ambient_temp_c);
    let transaction = assert_synthetic_fit(&memory_config, &samples, sample_count, ambient_temp_c);
    assert_synthetic_model_guards(&mut memory_config, transaction, ambient_temp_c);
    assert_synthetic_apdo_and_persistence(&mut memory_config, transaction);
    assert_synthetic_invalid_trace_guards(&mut memory_config, transaction, ambient_temp_c);
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
fn transient_thermal_fit_accepts_the_live_device_trace_shape() {
    fn raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
        let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
        (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
            / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
            .round() as u16
    }

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
    for (index, (elapsed_ticks, temp_c, duty_percent)) in trace.iter().copied().enumerate() {
        samples[index] = ThermalPlantTransientSample {
            elapsed_ticks,
            raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(temp_c),
            heater_voltage_125mv: 160,
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
            raw_rtd_adc_mv_for_temp(32.67),
            &samples,
            trace.len() as u8,
            Some(&preview_curve),
            &memory_config,
        )
        .is_none(),
        "a nonterminal recorded trace must not fit"
    );

    let mut terminal_samples = samples;
    terminal_samples[trace.len() - 1].raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(79.73);
    let terminal_temp_c = projected_rtd_temperature_c(
        &memory_config,
        terminal_samples[trace.len() - 1].raw_rtd_adc_mv,
    )
    .expect("terminal fixture temperature projects");
    assert!(thermal_plant_cooling_complete(79.73, terminal_temp_c));
    let (_, residual) = fit_thermal_plant_transient(
        0x4c49_5645,
        raw_rtd_adc_mv_for_temp(32.67),
        &terminal_samples,
        trace.len() as u8,
        Some(&preview_curve),
        &memory_config,
    )
    .expect("live device-shaped trace must produce a physical model");
    assert!(residual <= 0.20);
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
        source_max_mv: 20_000,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv: 250,
        idle_samples: 1,
        heater_curve: ThermalPlantCurveSampler::default(),
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

    job.elapsed_ticks = 2;
    assert!(record_thermal_plant_transient_sample(
        &mut job, 260, 26.0, 20_000, 100, true
    ));
    assert_eq!(job.samples[1].heater_voltage_125mv, 160);
    assert_eq!(job.samples[1].duty_percent, 100);
}

#[test]
fn thermal_plant_auto_completes_one_transient_cycle_for_3a_and_5a_pps() {
    fn raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
        let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
        (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
            / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
            .round() as u16
    }

    for source_current_ma in [3_000, 5_000] {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            ..CalibrationRuntimeState::default()
        };
        let mut memory_config = MemoryConfig::default();
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(source_current_ma),
                ..Default::default()
            }));
        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut manual_pps,
        )
        .unwrap();

        let ambient_temp_c = 25.0_f32;
        let capacity_mj_per_c = 30_000.0_f32;
        let convection_mw_per_c = 100.0_f32;
        let radiation_mw_per_k4 = 0.00000005_f32;
        let mut temperature_c = ambient_temp_c;
        for _ in 0..(THERMAL_PLANT_AMBIENT_TICKS as usize
            + THERMAL_PLANT_HEAT_TIMEOUT_TICKS as usize
            + THERMAL_PLANT_COOL_TIMEOUT_TICKS as usize)
        {
            let heater_duty_percent = u8::from(calibration.heater_enabled) * 100;
            match test_thermal_plant_phase() {
                Some(ThermalPlantAutoPhase::Heating) => assert_eq!(heater_duty_percent, 100),
                Some(ThermalPlantAutoPhase::Cooling) => assert_eq!(heater_duty_percent, 0),
                Some(ThermalPlantAutoPhase::Ambient) | None => {}
            }
            let source_mv = manual_pps.target_mv.unwrap_or(0);
            let resistance_ohms =
                estimated_heater_resistance_ohms(temperature_c, None, &memory_config);
            let pd_current_ma = if heater_duty_percent > 0 {
                ((f32::from(source_mv) / resistance_ohms).round() as u16).min(source_current_ma)
            } else {
                0
            };
            let measured_heater_mv = if heater_duty_percent > 0 {
                u32::from(source_mv)
                    .min((f32::from(pd_current_ma) * resistance_ohms).round() as u32)
            } else {
                0
            };
            let raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(temperature_c);
            let reported_temp_c =
                projected_rtd_temperature_c(&memory_config, raw_rtd_adc_mv).unwrap();
            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                CalibrationJobUpdateInput {
                    latest_rtd_raw_adc_mv: raw_rtd_adc_mv,
                    latest_vin_raw_adc_mv: 0,
                    latest_temp_c: reported_temp_c,
                    pd_current_ma,
                    latest_vin_mv: measured_heater_mv,
                    heater_duty_percent,
                },
            );
            if calibration.job.status == CalibrationJobStatus::Completed {
                break;
            }
            if calibration.job.status == CalibrationJobStatus::Failed {
                panic!(
                    "thermal plant job failed: {:?}, physical_temp={temperature_c}, reported_temp={reported_temp_c}, samples={}",
                    calibration.job.message, calibration.job.samples_collected
                );
            }

            let power_mw = if heater_duty_percent > 0 {
                ((f32::from(source_mv) / 1_000.0).powi(2) / resistance_ohms * 1_000.0)
                    .min(f32::from(source_mv) * f32::from(source_current_ma) / 1_000.0)
            } else {
                0.0
            };
            let temperature_k = temperature_c + 273.15;
            let ambient_k = ambient_temp_c + 273.15;
            let losses_mw = convection_mw_per_c * (temperature_c - ambient_temp_c)
                + radiation_mw_per_k4 * (temperature_k.powi(4) - ambient_k.powi(4));
            temperature_c += (power_mw - losses_mw) / capacity_mj_per_c * 0.05;
        }

        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert_eq!(calibration.mode, CalibrationMode::Off);
        assert!(!calibration.heater_enabled);
        assert!(!manual_pps.enabled);
        assert!(memory_config.thermal_plant_transient_active.is_some());
        assert!(has_calibrated_heater_resistance_curve(&memory_config));
    }
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
        source_max_mv: 20_000,
        source_current_ma: 3_000,
        ambient_raw_rtd_adc_mv: 250,
        idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
        heater_curve: ThermalPlantCurveSampler::default(),
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
        .enable(ManualPpsOwner::Calibration, 20_000, Some(3_000))
        .unwrap();

    update_calibration_job_state(
        &mut calibration,
        &mut memory_config,
        &mut manual_pps,
        CalibrationJobUpdateInput {
            latest_rtd_raw_adc_mv: 1_200,
            latest_vin_raw_adc_mv: 0,
            latest_temp_c: THERMAL_PLANT_TARGET_TEMP_C,
            pd_current_ma: 3_000,
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
            latest_rtd_raw_adc_mv: 0,
            latest_vin_raw_adc_mv: 0,
            latest_temp_c: 20.0,
            pd_current_ma: 0,
            latest_vin_mv: 20_000,
            heater_duty_percent: 0,
        },
    );

    assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
    assert_eq!(calibration.job.samples_collected, 0);
    assert_eq!(memory_config.thermal_plant_active, None);
}

#[test]
fn thermal_plant_job_disarms_on_missing_powered_electrical_observation() {
    for (latest_vin_mv, pd_current_ma) in [(0, 3_000), (20_000, 0)] {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            ..CalibrationRuntimeState::default()
        };
        let mut memory_config = MemoryConfig::default();
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

        for _ in 0..THERMAL_PLANT_AMBIENT_TICKS {
            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                CalibrationJobUpdateInput {
                    latest_rtd_raw_adc_mv: 250,
                    latest_vin_raw_adc_mv: 0,
                    latest_temp_c: 25.0,
                    pd_current_ma: 3_000,
                    latest_vin_mv: 20_000,
                    heater_duty_percent: 0,
                },
            );
        }
        assert!(calibration.heater_enabled);

        update_calibration_job_state(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            CalibrationJobUpdateInput {
                latest_rtd_raw_adc_mv: 260,
                latest_vin_raw_adc_mv: 0,
                latest_temp_c: 26.0,
                pd_current_ma,
                latest_vin_mv,
                heater_duty_percent: 100,
            },
        );

        assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
        assert!(!calibration.heater_enabled);
        assert!(!manual_pps.enabled);
        assert!(take_immediate_heater_disarm(&mut calibration));
        assert_eq!(memory_config.thermal_plant_transient_active, None);
    }
}

#[test]
fn transient_curve_projection_includes_low_temperature_anchors() {
    let mut bins = ThermalPlantCurveSampler::default().bins;
    bins[0].observe(100.0, 3.911);
    bins[1].observe(140.0, 3.918);
    bins[2].observe(175.0, 3.924);
    bins[3].observe(210.0, 3.929);

    let preview = heater_curve_from_transient_bins(&bins).unwrap();

    assert_eq!(
        preview.points[0],
        Some(default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C))
    );
    assert_eq!(
        preview.points[1],
        Some(default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C))
    );
    assert_eq!(
        preview.points[2].map(|point| point.temp_centi_c),
        Some(10_000)
    );
    assert!(preview.points[5].is_some());
    assert!(preview.points[6].is_none());
}

#[test]
fn transient_curve_sampling_requires_each_temperature_band() {
    let mut job = ThermalPlantCurveSampler::default();
    for _ in 0..THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN {
        job.cold_bin.observe(100.0, 3.8);
        for bin in &mut job.bins {
            bin.observe((bin.min_temp_c + bin.max_temp_c) / 2.0, 3.9);
        }
    }

    assert!(thermal_plant_curve_samples_ready(&job));

    job.bins[3].samples = THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN - 1;
    assert!(!thermal_plant_curve_samples_ready(&job));
}

#[test]
fn transient_curve_projection_never_underestimates_nominal_heater_model() {
    let mut bins = ThermalPlantCurveSampler::default().bins;
    bins[0].observe(100.0, 3.911);
    bins[1].observe(140.0, 3.918);
    bins[2].observe(175.0, 3.924);
    bins[3].observe(210.0, 3.929);

    let preview = heater_curve_from_transient_bins(&bins).unwrap();

    for point in preview.points.into_iter().flatten() {
        let temp_c = f32::from(point.temp_centi_c) / 100.0;
        let expected_floor =
            round_to_u16_nonnegative(default_estimated_heater_resistance_ohms(temp_c) * 1000.0);
        assert!(point.resistance_milliohms >= expected_floor);
    }
}

#[test]
fn transient_curve_projection_does_not_clamp_low_temp_voltage_to_first_hot_bin() {
    let mut bins = ThermalPlantCurveSampler::default().bins;
    bins[0].observe(100.0, 3.911);
    bins[1].observe(140.0, 3.918);
    bins[2].observe(175.0, 3.924);
    bins[3].observe(210.0, 3.929);

    let preview = heater_curve_from_transient_bins(&bins).unwrap();
    let memory_config = MemoryConfig::default();

    assert_eq!(
        heater_safe_max_mv_for_temp(20.0, 5_000, 24_000, Some(&preview), &memory_config),
        16_000
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(60.0, 5_000, 24_000, Some(&preview), &memory_config),
        18_500
    );
    assert_eq!(
        heater_safe_max_mv_for_temp(220.0, 4_800, 21_000, Some(&preview), &memory_config),
        21_000
    );
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
fn warmup_soft_start_runs_once_per_arm_and_target_change() {
    let mut controller = HeaterController::new();
    let armed = controller.update_at(140, 25.0, true, None, 1_000);
    assert_eq!(armed.warmup_soft_start_percent, 0);

    let mid_ramp = controller.update_at(140, 25.0, true, None, 1_500);
    assert_eq!(mid_ramp.warmup_soft_start_percent, 50);

    let completed = controller.update_at(140, 25.0, true, None, 2_000);
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

    let snapshot = controller.update(180, 179.6, true, Some(profile));
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
    assert!(snapshot.error_c > 0.2);
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
    controller.previous_measured_temp_c = Some(138.0);

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

    let snapshot = controller.update(140, 138.6, true, Some(profile));

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
    assert!(!controller.hold_coast_active);
    assert!(falling_under_target.duty_percent > 0);
}

#[test]
fn heater_hold_does_not_coast_below_target_just_because_previous_output_was_zero() {
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

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
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
fn heater_hold_on_error_delays_reentry_to_approach() {
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
    assert_eq!(confirmed_drop.phase, HeaterControlPhase::Approach);
}

#[test]
fn heater_hold_reentry_uses_actual_under_target_error_when_filter_lags() {
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
    assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
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
    assert_eq!(snapshot.duty_percent, 0);
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
    controller.previous_measured_temp_c = Some(137.5);

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

    let snapshot = controller.update(140, 138.56143, true, Some(profile));

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
fn heater_hold_entry_does_not_coast_far_below_target_after_zero_output_approach_sample() {
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

    assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
    assert!(snapshot.error_c > 2.0);
    assert!(snapshot.control_error_c > 2.5);
    assert!(snapshot.filtered_slope_c_per_s > 0.0);
    assert!(!controller.hold_coast_active);
    assert!(snapshot.duty_percent > 0);
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
fn hold_pps_governor_steps_toward_a_lower_safe_max_without_clamping() {
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
        Some(18_500)
    );
    assert_eq!(
        governor.request_mv(HoldPpsRequestInput {
            phase: HeaterControlPhase::Hold,
            duty_percent: 40,
            actual_error_c: -1.0,
            filtered_slope_c_per_s: 0.05,
            current_request_mv: 18_500,
            control_floor_mv: 6_100,
            safe_max_mv: 6_100,
            now_ms: HEATER_PPS_SMALL_TRANSITION_MS,
        }),
        Some(18_000)
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
fn adjustable_pps_same_mode_request_restores_an_active_gate_before_returning() {
    assert!(should_restore_gate_after_adjustable_request(false, 100));
    assert!(!should_restore_gate_after_adjustable_request(false, 0));
    assert!(!should_restore_gate_after_adjustable_request(true, 100));
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
fn calibrated_resistance_curve_sets_3a_plant_power_ceiling() {
    let mut config = MemoryConfig::default();
    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 2_000,
        resistance_milliohms: 3_936,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 22_000,
        resistance_milliohms: 5_674,
    });
    assert!(has_calibrated_heater_resistance_curve(&config));
    let pps3a_power_mw =
        heater_available_power_mw_for_temp(160.0, Some(20_000), Some(3_250), None, &config);
    let pps5a_power_mw =
        heater_available_power_mw_for_temp(160.0, Some(21_000), Some(5_000), None, &config);

    // The 20 V / 3.25 A APDO contributes its complete 65 W contract;
    // R(T) remains part of the heater-watt estimate but does not lower the
    // production voltage request or invent a board-current reserve.
    assert!((64_000..=65_000).contains(&pps3a_power_mw));
    assert!(pps5a_power_mw > 70_000);
}

#[test]
fn production_pps_ceiling_is_the_selected_apdo_maximum_not_r_times_i() {
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
        21_000
    );
}

#[test]
fn pps_request_compensates_measured_path_drop_without_relaxing_plate_limit() {
    assert_eq!(
        heater_source_request_ceiling_mv(17_300, 17_300, 15_900, 21_000),
        18_700
    );
    assert_eq!(
        heater_source_request_ceiling_mv(17_300, 17_300, 0, 21_000),
        17_300
    );
    assert_eq!(
        heater_source_request_ceiling_mv(20_000, 21_000, 17_000, 21_000),
        21_000
    );
}

#[test]
fn pps3a_heater_lock_requires_a_saved_resistance_curve() {
    let mut config = MemoryConfig::default();
    assert!(!has_calibrated_heater_resistance_curve(&config));

    config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 2_000,
        resistance_milliohms: 4_000,
    });
    config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
        temp_centi_c: 20_000,
        resistance_milliohms: 5_600,
    });

    assert!(has_calibrated_heater_resistance_curve(&config));
}

#[test]
fn raw_heater_observations_reproject_without_underestimating_the_model_floor() {
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

    let resistance = estimated_heater_resistance_ohms(216.7, None, &config);
    assert!(resistance >= default_estimated_heater_resistance_ohms(216.7));
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
fn thermal_plant_filters_single_sample_slope_before_delay_prediction() {
    let mut controller = HeaterController::new();
    let model = flux_purr_firmware::memory::ThermalPlantProjection {
        convection_mw_per_c: 0.0,
        radiation_mw_per_k4: 0.0,
        thermal_capacity_mj_per_c: 42_000.0,
        transport_delay_ms: 10_000,
    };
    let input = |measured_temp_c, heater_enabled, now_ms| ThermalPlantRuntimeInput {
        target_temp_c: 60,
        measured_temp_c,
        ambient_temp_c: 30.0,
        heater_enabled,
        model,
        max_power_mw: 100_000.0,
        now_ms,
    };

    controller.update_thermal_plant_at(input(30.0, true, 0));
    let snapshot = controller.update_thermal_plant_at(input(31.0, true, 50));

    assert!((snapshot.filtered_slope_c_per_s - 0.375).abs() < 0.001);
    assert!(snapshot.control_error_c > 0.0);
    assert!(snapshot.duty_percent >= 70);
    assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
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
    assert!(normalized_source.contains("usb_recovery_marker_failed=true"));
    assert!(normalized_source.contains("retainingterminaltransportfault"));
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
    let runtime_loop = include_str!("runtime_loop.rs");
    for helper in [
        "impl RuntimeLanInputOutcome",
        "async fn runtime_lan_direct_response",
        "async fn runtime_process_lan_control",
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

    assert!(
        source.contains("PD_INTERLOCK_LATCHED")
            && source.contains("PD_INTERLOCK_PENDING")
            && source.contains("PD_INTERLOCK_PENDING.swap")
    );
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

    assert!(release_terminal_fixed_pd_disarm_for_manual_pps(
        &mut backend,
        true
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

    assert!(release_terminal_fixed_pd_disarm_for_manual_pps(
        &mut backend,
        true
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
fn terminal_disarm_waits_for_measured_idle_voltage() {
    let fixed_mv = u32::from(FUSB302B_INITIAL_PPS_REQUEST_MV);
    assert!(!terminal_idle_voltage_confirmed(
        fixed_mv.saturating_add(9_000)
    ));
    assert!(!terminal_idle_voltage_confirmed(
        fixed_mv.saturating_add(3_000)
    ));
    assert!(terminal_idle_voltage_confirmed(
        fixed_mv.saturating_add(450)
    ));
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
