#[cfg(target_arch = "xtensa")]
#[expect(
    clippy::too_many_lines,
    clippy::excessive_nesting,
    reason = "legacy workflow preserves protocol ordering and safety checks"
)]
pub async fn run(_spawner: Spawner) {
    let reset_reason = reset_reason_log_line(esp_hal::system::reset_reason());
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mut startup_sequence = StartupSequence::new();
    // GPIO13 drives the panel's active-low backlight gate. Configure it
    // before any potentially blocking startup work so a visible panel does
    // not depend on later display or PD initialization completing.
    let mut backlight = Output::new(peripherals.GPIO13, Level::Low, OutputConfig::default());
    backlight.set_low();
    assert!(startup_sequence.advance(StartupSequenceStage::BacklightReady));
    init_runtime_heap();
    let eeprom_record_staging = initialize_eeprom_record_staging();
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);
    let software_interrupts = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let buzzer_realtime_spawner = BUZZER_REALTIME_EXECUTOR
        .init(InterruptExecutor::new(
            software_interrupts.software_interrupt1,
        ))
        .start(Priority::Priority2);
    let status_light_started_ms = Instant::now().as_millis();
    let status_light_red = Output::new(peripherals.GPIO39, Level::High, OutputConfig::default());
    let status_light_green = Output::new(peripherals.GPIO38, Level::High, OutputConfig::default());
    let status_light_blue = Output::new(peripherals.GPIO37, Level::High, OutputConfig::default());
    _spawner
        .spawn(run_status_light_task(
            status_light_red,
            status_light_green,
            status_light_blue,
            status_light_started_ms,
        ))
        .expect("failed to spawn status-light task");
    let runtime_mode = FrontPanelRuntimeMode::compile_time_default();
    #[cfg(feature = "web_serial")]
    let mut usb_serial = RawUsbSerialJtag::new(peripherals.USB_DEVICE);
    #[cfg(feature = "web_serial")]
    let mut usb_rx_line: heapless::String<USB_CONTROL_LINE_CAPACITY> = heapless::String::new();
    #[cfg(feature = "web_serial")]
    let usb_tx_buf = initialize_usb_control_response_buffer();
    #[cfg(all(target_arch = "xtensa", not(feature = "web_serial")))]
    let mut persistence_log_sink = NoopPersistenceLogSink;
    #[cfg(feature = "web_serial")]
    let usb_boot_memory_config = MemoryConfig::default();
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=backlight_on\n");
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(hardware_identity()),
        usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, reset_reason.as_bytes());
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_detect_start\n");
    let mut pd_i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default()
            .with_frequency(Rate::from_hz(FUSB302B_I2C_FREQUENCY_HZ))
            .with_software_timeout(SoftwareTimeout::Transaction(HalDuration::from_millis(
                I2C_TRANSACTION_TIMEOUT_MS,
            ))),
    )
    .expect("failed to create I2C0")
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);
    let detected_pd_controller = detect_pd_controller(&mut pd_i2c).await;
    let mut pd_port = match detected_pd_controller {
        DetectedPdController::Fusb302b(device_id) => {
            #[cfg(feature = "web_serial")]
            let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_fusb302b_detected\n");
            let mut runtime = Fusb302bRuntime::new();
            if !runtime.initialize(&mut pd_i2c).await {
                #[cfg(feature = "web_serial")]
                let _ =
                    usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_phy_init_failed\n");
                warn!(
                    "fusb302b identified device_id=0x{=u8:02x} but PHY initialization failed; holding heater interlocked",
                    device_id,
                );
                PdPort::Unavailable
            } else {
                #[cfg(feature = "web_serial")]
                let _ =
                    usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_phy_init_complete\n");
                info!(
                    "fusb302b selected device_id=0x{=u8:02x} policy=pps target_mv={=u16} max_current_ma={=u16}",
                    device_id,
                    DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
                    MAX_HEATER_CONTRACT_MA,
                );
                PdPort::Fusb302b(Box::new(runtime))
            }
        }
        DetectedPdController::Unknown => {
            #[cfg(feature = "web_serial")]
            let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_identity_unknown\n");
            warn!("PD controller identity is ambiguous or unreadable; holding heater interlocked");
            PdPort::Unavailable
        }
    };
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_contract_pending\n");
    // Complete one bounded startup service window before touching the display.
    // A missing contract remains a fail-closed heater interlock rather than an
    // unbounded boot delay.
    let pd_runtime_started_ms = Instant::now().as_millis();
    let fusb302b_present = matches!(&pd_port, PdPort::Fusb302b(_));
    let mut initial_pd_observation =
        read_pd_status(&mut pd_i2c, &mut pd_port, PdTimestamp::now()).await;
    while startup_pd_service_should_continue(
        fusb302b_present,
        startup_pd_contract_ready(initial_pd_observation),
        pd_port.service_available(),
        pd_runtime_elapsed_ms(pd_runtime_started_ms, Instant::now().as_millis()),
    ) {
        EmbassyTimer::after_millis(STARTUP_PD_SERVICE_INTERVAL_MS).await;
        initial_pd_observation =
            read_pd_status(&mut pd_i2c, &mut pd_port, PdTimestamp::now()).await;
    }
    let mut pd_contract_ready = startup_pd_contract_ready(initial_pd_observation);
    if !pd_contract_ready {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_contract_not_ready\n");
        warn!(
            "PD contract was not ready before display initialization; continuing with heater interlocked"
        );
    } else {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_contract_ready\n");
    }
    assert!(startup_sequence.advance(StartupSequenceStage::PdServiceComplete));

    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_setup_start\n");
    info!(
        "boot display_dc={=u8} mosi={=u8} sclk={=u8} blk={=u8} res={=u8} cs={=u8}",
        s3_frontpanel::PIN_LCD_DC,
        s3_frontpanel::PIN_LCD_MOSI,
        s3_frontpanel::PIN_LCD_SCLK,
        s3_frontpanel::PIN_LCD_BLK,
        s3_frontpanel::PIN_LCD_RES,
        s3_frontpanel::PIN_LCD_CS,
    );
    info!(
        "boot keys center={=u8} right={=u8} down={=u8} left={=u8} up={=u8}",
        s3_frontpanel::PIN_CENTER_KEY_BOOT,
        s3_frontpanel::PIN_KEY_RIGHT,
        s3_frontpanel::PIN_KEY_DOWN,
        s3_frontpanel::PIN_KEY_LEFT,
        s3_frontpanel::PIN_KEY_UP,
    );
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(10_000_000))
            .with_mode(SpiMode::_0),
    )
    .expect("failed to create SPI2")
    .with_sck(peripherals.GPIO12)
    .with_mosi(peripherals.GPIO11);
    let cs = Output::new(peripherals.GPIO15, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new_no_delay(spi.into_async(), cs)
        .expect("failed to wrap async SPI bus as ExclusiveDevice");
    static DRIVER_FB: StaticCell<
        [embedded_graphics::pixelcolor::Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS],
    > = StaticCell::new();
    let driver_framebuffer = DRIVER_FB.init_with(|| {
        [embedded_graphics::pixelcolor::Rgb565::BLACK; flux_purr_firmware::display::DISPLAY_PIXELS]
    });
    let canvas = initialize_display_canvas();
    let mut display: GC9D01<_, _, _, DisplayTimer> = GC9D01::new(
        DISPLAY_PANEL_CONFIG,
        spi_device,
        dc,
        rst,
        driver_framebuffer,
    );
    info!(
        "init panel width={=u16} height={=u16} dx={=u16} dy={=u16}",
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        DISPLAY_PANEL_CONFIG.dx,
        DISPLAY_PANEL_CONFIG.dy,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_init_start\n");
    let display_init_result = run_display_operation_with_pd(
        display.init(),
        &mut pd_i2c,
        &mut pd_port,
        &mut initial_pd_observation,
    )
    .await;
    let display_ready = matches!(display_init_result, Some(Ok(())));
    if !display_ready {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_init_failed\n");
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            usb_tx_buf,
            &usb_boot_memory_config,
            StatusLightState::Booting,
            UsbRecoveryPhase::BeforePersistentState,
        )
        .await;
        #[cfg(not(feature = "web_serial"))]
        panic!("failed to initialize GC9D01 display");
    }
    assert!(startup_sequence.advance(StartupSequenceStage::DisplayReady));
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_init_complete\n");
    match startup_frontpanel_presentation(runtime_mode) {
        StartupFrontPanelPresentation::Splash => render_scene(SceneId::StartupSplash, canvas),
        StartupFrontPanelPresentation::Calibration => {
            render_scene(SceneId::StartupCalibration, canvas)
        }
    }
    display.write_area(
        0,
        0,
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        canvas.pixels(),
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_flush_start\n");
    let startup_flush_result = run_display_operation_with_pd(
        display.flush(),
        &mut pd_i2c,
        &mut pd_port,
        &mut initial_pd_observation,
    )
    .await;
    let startup_flush_ready = match startup_flush_result {
        Some(Ok(())) => true,
        Some(Err(gc9d01::Error::Bus(_))) => {
            warn!("startup display flush failed: spi bus");
            false
        }
        Some(Err(gc9d01::Error::Pin(_))) => {
            warn!("startup display flush failed: display pin");
            false
        }
        None => {
            warn!("startup display flush timed out");
            false
        }
    };
    if !startup_flush_ready {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_flush_failed\n");
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            usb_tx_buf,
            &usb_boot_memory_config,
            StatusLightState::Booting,
            UsbRecoveryPhase::BeforePersistentState,
        )
        .await;
        #[cfg(not(feature = "web_serial"))]
        panic!("failed to draw startup calibration screen");
    }
    assert!(startup_sequence.advance(StartupSequenceStage::StartupFrameReady));
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_flush_complete\n");
    info!("backlight active-low: gpio13 low -> on");
    let input_cfg = InputConfig::default().with_pull(Pull::Up);
    let inputs = FrontPanelInputs {
        center: Input::new(peripherals.GPIO0, input_cfg),
        right: Input::new(peripherals.GPIO16, input_cfg),
        // The calibrated logical key map swaps raw DOWN/LEFT, so keep the raw
        // input binding on the verified GPIO order instead of the board labels.
        down: Input::new(peripherals.GPIO17, input_cfg),
        left: Input::new(peripherals.GPIO18, input_cfg),
        up: Input::new(peripherals.GPIO21, input_cfg),
    };
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
        &usb_boot_memory_config,
    );
    info!(
        "frontpanel runtime mode={=str}",
        runtime_mode_label(runtime_mode)
    );

    if runtime_mode == FrontPanelRuntimeMode::KeyTest {
        let mut key_test_last_pd_observation = initial_pd_observation;
        let mut _heater_safe = Output::new(peripherals.GPIO47, Level::Low, OutputConfig::default());
        _heater_safe.set_low();
        let mut _fan_enable_safe =
            Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
        _fan_enable_safe.set_low();
        let mut _fan_pwm_safe =
            Output::new(peripherals.GPIO36, Level::Low, OutputConfig::default());
        _fan_pwm_safe.set_low();
        info!("key-test runtime ready: gpio47/gpio35/gpio36 held safe-off without PD/RTD bring-up");
        match run_key_test_runtime(
            &mut display,
            canvas,
            inputs,
            status_light_started_ms,
            &mut pd_i2c,
            &mut pd_port,
            &mut key_test_last_pd_observation,
        )
        .await
        {
            Err(()) => {
                #[cfg(feature = "web_serial")]
                run_usb_recovery_control_loop(
                    &mut usb_serial,
                    &mut usb_rx_line,
                    usb_tx_buf,
                    &usb_boot_memory_config,
                    StatusLightState::HeaterInterlocked,
                    UsbRecoveryPhase::BeforePersistentState,
                )
                .await;

                #[cfg(not(feature = "web_serial"))]
                panic!("key-test display failed");
            }
            Ok(()) => unreachable!("key-test runtime only returns for a display fault"),
        }
    }
    // Put every power-related output into a known safe state before any I2C
    // probe or EEPROM access can take the boot path through a timeout.
    assert!(startup_sequence.advance(StartupSequenceStage::OtherInitialization));
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=outputs_init_start\n");
    let mut fan_enable = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
    let pwm_clock_cfg =
        PeripheralClockConfig::with_frequency(Rate::from_hz(MCPWM_PERIPHERAL_CLOCK_HZ))
            .expect("failed to derive MCPWM peripheral clock");
    let mut mcpwm = McPwm::new(peripherals.MCPWM0, pwm_clock_cfg);

    mcpwm.operator0.set_timer(&mcpwm.timer0);
    let mut fan_pwm = mcpwm
        .operator0
        .with_pin_a(peripherals.GPIO36, PwmPinConfig::UP_ACTIVE_HIGH);
    let fan_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            FAN_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive fan PWM timer clock");
    mcpwm.timer0.start(fan_timer_cfg);
    let _ = fan_pwm.set_duty_cycle_percent(pwm_percent_from_permille(
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
    ));
    info!(
        "fan runtime armed: gpio35 default=off gpio36 min_output={=u16}permille active_full={=u16}permille safety_half={=u16}permille full={=u16}permille freq={=u32}Hz active_min>={=i16}C cooldown_ms={=u64} forced_min>={=i16}C forced_full>{=i16}C pulse>{=i16}C lock>{=i16}C full>{=i16}C",
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
        FAN_FULL_SPEED_PWM_PERMILLE,
        FAN_HALF_SPEED_PWM_PERMILLE,
        FAN_FULL_SPEED_PWM_PERMILLE,
        FAN_PWM_FREQUENCY_HZ,
        ACTIVE_COOLING_FAN_MIN_TEMP_C,
        AUTO_COOLING_FAN_COOLDOWN_MS,
        FORCED_COOLING_FAN_MIN_TEMP_C,
        FORCED_COOLING_FAN_FULL_TEMP_C,
        COOLING_DISABLED_PULSE_START_TEMP_C,
        COOLING_DISABLED_HEATER_LOCK_TEMP_C,
        COOLING_DISABLED_FAN_FULL_TEMP_C,
    );

    mcpwm.operator1.set_timer(&mcpwm.timer1);
    let mut heater_pwm = mcpwm
        .operator1
        .with_pin_a(peripherals.GPIO47, PwmPinConfig::UP_ACTIVE_HIGH);
    let heater_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            HEATER_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(HEATER_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive heater PWM timer clock");
    mcpwm.timer1.start(heater_timer_cfg);
    let _ = heater_pwm.set_duty_cycle_percent(0);
    let mut last_heater_duty = 0_u8;

    mcpwm.operator2.set_timer(&mcpwm.timer2);
    #[cfg(feature = "buzzer-observe")]
    let mut buzzer_pin = peripherals.GPIO48.degrade();
    #[cfg(not(feature = "buzzer-observe"))]
    let buzzer_pin = peripherals.GPIO48;
    #[cfg(feature = "buzzer-observe")]
    let buzzer_edge_counter = {
        let pcnt = Pcnt::new(peripherals.PCNT);
        let unit = pcnt.unit0;
        unit.channel0.set_edge_signal(buzzer_pin.reborrow());
        unit.channel0
            .set_input_mode(EdgeMode::Hold, EdgeMode::Increment);
        unit.clear();
        unit.resume();
        unit
    };
    let mut buzzer_pwm = mcpwm.operator2.with_pin_a(
        buzzer_pin,
        PwmPinConfig::new(PwmActions::UP_ACTIVE_HIGH, PwmUpdateMethod::SYNC_IMMEDIATLY),
    );
    let buzzer_timer_cfg = pwm_clock_cfg.timer_clock_with_prescaler(
        buzzer_timer_period_ticks(BUZZER_IDLE_FREQUENCY_HZ)
            .expect("idle buzzer frequency is outside the Timer2 period range"),
        PwmWorkingMode::Increase,
        BUZZER_TIMER_PRESCALER,
    );
    mcpwm.timer2.start(buzzer_timer_cfg);
    let _ = buzzer_pwm.set_duty_cycle_percent(0);
    info!(
        "buzzer runtime armed: gpio48 default=silent fixed_prescaler={=u8}",
        BUZZER_TIMER_PRESCALER,
    );
    #[cfg(feature = "buzzer-observe")]
    buzzer_realtime_spawner
        .spawn(run_buzzer_task(
            mcpwm.timer2,
            buzzer_pwm,
            pwm_clock_cfg,
            buzzer_edge_counter,
        ))
        .expect("failed to spawn realtime buzzer task");
    #[cfg(not(feature = "buzzer-observe"))]
    buzzer_realtime_spawner
        .spawn(run_buzzer_task(mcpwm.timer2, buzzer_pwm, pwm_clock_cfg))
        .expect("failed to spawn realtime buzzer task");

    // Keep the fixed scratch alive through startup migration and network bring-up.
    let mut boot_memory_io_scratch = try_allocate_memory_io_scratch();
    let (
        mut eeprom_memory_record,
        mut eeprom_data_incompatible,
        mut eeprom_required,
        mut prepared_layout_recovery_pending,
    ) = {
        let mut eeprom_pd_service = EepromPdServiceContext::new(
            &mut initial_pd_observation,
            &mut heater_pwm,
            &mut last_heater_duty,
        );
        if let Some(scratch) = boot_memory_io_scratch.as_mut() {
            load_eeprom_memory_record(
                &mut pd_i2c,
                &mut pd_port,
                &mut eeprom_pd_service,
                scratch,
                eeprom_record_staging,
            )
            .await
        } else {
            (None, false, true, false)
        }
    };
    let mut eeprom_restore_pending = eeprom_data_incompatible || prepared_layout_recovery_pending;
    if !eeprom_required && !eeprom_data_incompatible && eeprom_memory_record.is_none() {
        let initialization_result = {
            #[cfg(feature = "web_serial")]
            let init_log_sink = &mut usb_serial as &mut dyn PersistenceLogSink;
            #[cfg(not(feature = "web_serial"))]
            let init_log_sink = &mut persistence_log_sink as &mut dyn PersistenceLogSink;
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut initial_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            initialize_fpr2_defaults(
                &mut pd_i2c,
                &mut pd_port,
                &mut eeprom_pd_service,
                &mut *init_log_sink,
                eeprom_record_staging,
            )
            .await
        };
        if let Some(sequence) = initialization_result {
            info!("blank EEPROM initialized and verified");
            eeprom_memory_record = Some(MemoryRecord {
                sequence,
                config: MemoryConfig::default(),
            });
        } else {
            eeprom_required = true;
        }
    } else if eeprom_data_incompatible && eeprom_memory_record.is_none() {
        // A non-blank EEPROM without a decodable record is not safe to
        // overwrite during boot. Keep the device in the explicit restore
        // state until maintenance provides a valid record.
        eeprom_required = true;
    }
    let mut persistence_source = if eeprom_required {
        "none"
    } else if eeprom_memory_record.is_some() {
        "eeprom"
    } else {
        "defaults"
    };
    let mut persistence_record_state = if eeprom_required && eeprom_data_incompatible {
        "incompatible"
    } else if eeprom_required {
        "unavailable"
    } else if eeprom_memory_record.is_some() {
        "valid"
    } else if eeprom_data_incompatible {
        "incompatible"
    } else {
        "blank"
    };
    let (mut memory_config, mut memory_sequence) = eeprom_memory_record
        .map(|record| (record.config, record.sequence))
        .unwrap_or_default();
    if prepared_layout_recovery_pending {
        let recovery_result = if boot_memory_io_scratch.is_some() {
            #[cfg(feature = "web_serial")]
            let recovery_log_sink = &mut usb_serial as &mut dyn PersistenceLogSink;
            #[cfg(not(feature = "web_serial"))]
            let recovery_log_sink = &mut persistence_log_sink as &mut dyn PersistenceLogSink;
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut initial_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            recover_prepared_fpr2_layout(
                &mut pd_i2c,
                &mut pd_port,
                &mut eeprom_pd_service,
                memory_sequence,
                &mut *recovery_log_sink,
                eeprom_record_staging,
            )
            .await
        } else {
            Err(MemoryCommitFailure {
                error: MemoryCommitError::VerifyUnreadable,
                phase: "active-recovery",
                attempt: 1,
                sequence: memory_sequence,
                domain: PersistDomain::LayoutMarker,
                slot: PersistSlot::A,
            })
        };
        if recovery_result.is_ok() {
            eeprom_required = false;
            prepared_layout_recovery_pending = false;
            eeprom_restore_pending = false;
            persistence_source = "eeprom";
            persistence_record_state = "valid";
            info!(
                "prepared FPR2 layout recovery complete seq={=u32}",
                memory_sequence
            );
        } else {
            eeprom_required = true;
            eeprom_restore_pending = false;
            warn!("prepared FPR2 layout recovery failed; keeping heater interlocked");
        }
    }
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
        &memory_config,
    );
    let mut preview_heater_curve: Option<HeaterCurvePreview> = None;
    let mut memory_commit_due_ms: Option<u64> = None;
    #[cfg(feature = "web_serial")]
    let mut eeprom_snapshot_session = EepromSnapshotSession::default();
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(hardware_identity()),
        usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
        &memory_config,
    );
    let mut last_pd_observation = initial_pd_observation;
    if let Some(PdStatusObservation {
        status_raw,
        status,
        current_raw,
        current_ma,
        ..
    }) = last_pd_observation
    {
        info!(
            "heater runtime ready: gpio47 freq={=u32}Hz target={=i16}~{=i16}C cooling_lock>{=i16}C hard_cutoff={=i16}C pd_status=0x{=u8:02x} pd={=bool} epr={=bool} epr_exist={=bool} current_raw=0x{=u8:02x} current_ma={=u16}",
            HEATER_PWM_FREQUENCY_HZ,
            HEATER_PID_TARGET_MIN_C,
            HEATER_PID_TARGET_MAX_C,
            COOLING_DISABLED_HEATER_LOCK_TEMP_C,
            HEATER_HARD_CUTOFF_TEMP_C,
            status_raw,
            status.pd_active,
            status.epr_active,
            status.epr_exist,
            current_raw,
            current_ma,
        );
    }
    let mut last_pd_status_log_key = pd_status_log_key(last_pd_observation);
    let mut pd_contract_vin_guard = PdContractVinGuard::default();
    let mut active_thermal_settings =
        ThermalControlProfileSettings::from(memory_config.active_thermal_control_profile.settings);
    info!(
        "heater control policy mode=hybrid interval_ms={=u64} warmup_reenter={=f32}C hold_entry={=f32}C hold_exit={=f32}C approach_max_s={=u8} hold_kp={=f32} hold_ki={=f32} auto_floor_mv={=u16} current_reserve_ma={=u16}",
        HEATER_CONTROL_INTERVAL_MS,
        active_thermal_settings.warmup_reenter_error_c,
        active_thermal_settings.hold_entry_error_c,
        active_thermal_settings.hold_exit_error_c,
        active_thermal_settings.approach_max_ticks,
        active_thermal_settings.hold_kp_permille_per_c,
        active_thermal_settings.hold_ki_permille_per_c_tick,
        active_thermal_settings.auto_adjustable_working_floor_mv,
        active_thermal_settings.heater_current_reserve_ma,
    );
    let power_data_capabilities = read_pd_power_capabilities(&mut pd_i2c, &mut pd_port);
    match power_data_capabilities {
        Some(capabilities) => info!(
            "pd power data pps20={=bool} pps_min_mv={=u16} pps_max_mv={=u16} pps_max_ma={=u16}",
            capabilities.pps_covers_20v,
            capabilities.pps_min_mv.unwrap_or(0),
            capabilities.pps_max_mv.unwrap_or(0),
            capabilities.pps_max_ma.unwrap_or(0),
        ),
        None => info!("pd power data read failed"),
    }
    let mut manual_pps_state = ManualPpsState::from_fusb302b_capabilities(power_data_capabilities);
    let mut last_fusb302b_power_capabilities = power_data_capabilities;
    let mut calibration_runtime_state = CalibrationRuntimeState::default();
    static THERMAL_PLANT_WORKSPACE: StaticCell<CalibrationThermalPlantWorkspace> =
        StaticCell::new();
    let thermal_plant_workspace =
        THERMAL_PLANT_WORKSPACE.init_with(CalibrationThermalPlantWorkspace::default);
    let mut thermal_control_profile_preview: Option<ThermalControlProfile> = None;
    let mut heater_power_backend = match pd_port.controller_kind() {
        ControllerKind::Fusb302b => select_fusb302b_heater_power_backend(power_data_capabilities),
        controller => constrain_heater_backend_to_controller(
            controller,
            select_heater_power_backend(
                power_data_capabilities,
                last_pd_observation.map(|status| status.status),
            ),
        ),
    };
    let mut hold_pps_governor = HoldPpsGovernor::new();
    match heater_power_backend {
        HeaterPowerBackend::PpsMos {
            pps_min_mv,
            pps_max_mv,
            adjustable_max_mv,
            ..
        } => info!(
            "heater backend selected mode={=str} reason={=str} pps_min_mv={=u16} idle_mv={=u16} pps_max_mv={=u16} adjustable_max_mv={=u16} gate_mv={=u16}",
            heater_power_backend.label(),
            HeaterPowerBackendReason::PpsCovers20v.label(),
            pps_min_mv,
            heater_power_backend.pd_contract_mv(),
            pps_max_mv,
            adjustable_max_mv,
            ch224q::PPS_GATE_MV,
        ),
        HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request,
            ..
        } => info!(
            "heater backend selected mode={=str} reason={=str} fixed_mv={=u16}",
            heater_power_backend.label(),
            reason.label(),
            fixed_request.millivolts(),
        ),
    }
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pre_adc_heater_sync_start\n");
    let _ = apply_heater_power_output(HeaterPowerOutputContext {
        i2c: &mut pd_i2c,
        pd_port: &mut pd_port,
        heater_pwm: &mut heater_pwm,
        backend: &mut heater_power_backend,
        hold_pps_governor: &mut hold_pps_governor,
        manual_pps: &mut manual_pps_state,
        pd_observation: last_pd_observation,
        measured_heater_mv: 0,
        current_temp_c: 0.0,
        duty_percent: 0,
        heater_enabled: false,
        control_phase: HeaterControlPhase::Warmup,
        control_error_c: 0.0,
        filtered_slope_c_per_s: 0.0,
        warmup_soft_start_percent: 0,
        last_physical_duty_percent: &mut last_heater_duty,
        preview_heater_curve: preview_heater_curve_config(preview_heater_curve.as_ref()),
        memory_config: &memory_config,
        active_thermal_settings,
        now_ms: 0,
    })
    .await;
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=heater_safe_output_ready\n");

    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=adc_init_start\n");
    let (mut adc1, mut vin_adc_pin, mut rtd_adc_pin, adc_curve) =
        initialize_adc1(peripherals.ADC1, peripherals.GPIO1, peripherals.GPIO2);
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=adc_init_complete\n");
    info!(
        "adc monitor active: vin_gpio1 rtd_gpio2 atten={=str} samples={=u8} interval_ms={=u64}",
        "6dB", RTD_SAMPLE_COUNT as u8, RTD_LOG_INTERVAL_MS,
    );

    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=initial_rtd_start\n");
    let initial_rtd_sample = read_rtd_sample_with_pd(
        &mut adc1,
        &mut rtd_adc_pin,
        adc_curve.as_ref(),
        &memory_config,
        &mut PdAdcService {
            i2c: &mut pd_i2c,
            pd_port: &mut pd_port,
            last_pd_observation: &mut last_pd_observation,
            heater_pwm: &mut heater_pwm,
            last_heater_duty: &mut last_heater_duty,
        },
    )
    .await;
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=initial_rtd_complete\n");
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new_startup(runtime_mode);
    ui_state.eeprom_data_incompatible = eeprom_data_incompatible;
    ui_state.eeprom_required = eeprom_required;
    ui_state.persistence_fault_attention_pending = eeprom_data_incompatible || eeprom_required;
    if ui_state.persistence_locked() {
        ui_state.heater_lock_reason = Some(HeaterLockReason::PersistenceRequired);
    }
    ui_state.pd_contract_mv =
        effective_pd_contract_mv(&manual_pps_state, last_pd_observation, heater_power_backend);
    apply_memory_config_to_ui(&mut ui_state, &memory_config);
    let mut heater_controller = HeaterController::new();
    let mut current_rtd_fault: Option<HeaterFaultReason> = None;
    let mut latest_temp_c = 0.0_f32;
    let mut latest_temp_i16 = 0_i16;
    let mut latest_display_temp_c = 0.0_f32;
    let mut latest_display_temp_i16 = 0_i16;
    let mut latest_rtd_raw_adc_mv = 0_u16;
    let mut latest_rtd_raw_adc_min_mv = 0_u16;
    let mut latest_rtd_raw_adc_max_mv = 0_u16;
    let mut latest_vin_raw_adc_mv = 0_u16;
    let mut latest_vin_mv = 0_u32;
    let mut rtd_pps_transition_guard =
        RtdPpsTransitionGuard::new(heater_power_backend.pd_request_mv());
    let mut rtd_control_measurement_guard = RtdControlMeasurementGuard::default();
    let mut control_measurement_guarded = false;
    let mut last_rtd_sample_request_mv = heater_power_backend.pd_request_mv();
    match initial_rtd_sample {
        RtdSample::Valid(measurement) => {
            latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
            latest_rtd_raw_adc_min_mv = measurement.raw_adc_min_mv;
            latest_rtd_raw_adc_max_mv = measurement.raw_adc_max_mv;
            latest_temp_c = measurement.temp_c;
            latest_temp_i16 = temp_c_to_whole_c(measurement.temp_c);
            rtd_control_measurement_guard.reseed(measurement.temp_c, 0);
            let _ = update_runtime_display_temperature(
                &mut ui_state,
                &mut latest_display_temp_c,
                &mut latest_display_temp_i16,
                measurement.temp_c,
            );
            if let Some(reason) = overtemp_fault_from_control_temperature(latest_temp_c) {
                current_rtd_fault = Some(reason);
                let _ = heater_controller.latch_fault(HeaterFaultReason::OverTemp);
                info!(
                    "heater initial fault latched reason={=str}",
                    HeaterFaultReason::OverTemp.label()
                );
            }
            ui_state.set_dashboard_presentation(if eeprom_restore_pending {
                flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore
            } else {
                flux_purr_firmware::frontpanel::DashboardPresentationState::Ready
            });
            info!(
                "rtd initial raw_adc_mv={=u16} adc_mv={=u16} divider_mv={=u16} resistance_ohms={=f32} temp_c={=f32}",
                measurement.raw_adc_mv,
                measurement.adc_mv,
                RTD_DIVIDER_SUPPLY_MV,
                measurement.resistance_ohms,
                measurement.temp_c,
            );
        }
        RtdSample::Fault { adc_mv, reason } => {
            current_rtd_fault = Some(reason);
            eeprom_restore_pending = false;
            ui_state.set_dashboard_presentation(
                flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault,
            );
            let _ = heater_controller.latch_fault(reason);
            let _ = retain_runtime_display_temperature(
                &mut ui_state,
                &mut latest_display_temp_c,
                &mut latest_display_temp_i16,
            );
            info!(
                "rtd initial fault adc_mv={=u16} reason={=str}",
                adc_mv.unwrap_or(0),
                reason.label(),
            );
        }
    }
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=initial_vin_start\n");
    if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) = read_calibrated_vin_mv_with_pd(
        &mut adc1,
        &mut vin_adc_pin,
        adc_curve.as_ref(),
        &memory_config,
        &mut PdAdcService {
            i2c: &mut pd_i2c,
            pd_port: &mut pd_port,
            last_pd_observation: &mut last_pd_observation,
            heater_pwm: &mut heater_pwm,
            last_heater_duty: &mut last_heater_duty,
        },
    )
    .await
    {
        latest_vin_raw_adc_mv = raw_adc_mv;
        latest_vin_mv = vin_mv;
        info!(
            "vin initial raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
            raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
        );
        let _ = reconcile_pd_contract_with_vin(
            &mut pd_contract_vin_guard,
            last_pd_observation,
            Some(vin_mv),
            PdTimestamp::now().as_millis(),
            PdContractVinContext {
                pd_port: &mut pd_port,
                last_pd_observation: &mut last_pd_observation,
                pd_contract_ready: &mut pd_contract_ready,
                ui_state: &mut ui_state,
                calibration_runtime_state: &mut calibration_runtime_state,
                manual_pps_state: &mut manual_pps_state,
                heater_pwm: &mut heater_pwm,
                last_heater_duty: &mut last_heater_duty,
            },
        );
    }
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=initial_vin_complete\n");
    let mut last_pid_snapshot = HeaterPidSnapshot {
        duty_percent: 0,
        warmup_soft_start_percent: 0,
        error_c: 0.0,
        control_error_c: 0.0,
        filtered_temp_c: 0.0,
        filtered_slope_c_per_s: 0.0,
        coast_active: false,
        phase: HeaterControlPhase::Warmup,
    };
    let mut cooling_disabled_lock_latched = false;
    let mut cooling_disabled_lock_armed = true;
    let mut fan_policy_state = FanPolicyState::Disabled;
    let mut heater_enabled_last_cycle = ui_state.heater_enabled;
    let mut last_fan_command: Option<FanHardwareCommand> = None;
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    let mut initial_fan_decision = fan_policy_decision_with_modes(
        latest_display_temp_i16,
        0,
        ui_state.heater_enabled,
        false,
        ui_state.post_heat_cooling_mode,
        ui_state.heating_fan_guard_mode,
        (fan_policy_state, is_sensor_fault(current_rtd_fault)),
    );
    if let Some(state) = overtemp_forced_fan_state(
        latest_display_temp_i16,
        is_overtemp_fault(current_rtd_fault),
    ) {
        let command = state.command(0);
        initial_fan_decision = FanPolicyDecision {
            state,
            command,
            display_state: fan_display_state_for_policy(
                FanPolicySource::Safety,
                ui_state.post_heat_cooling_mode,
                command,
            ),
            source: FanPolicySource::Safety,
            output_level: fan_output_level_for_command(command),
        };
    }
    fan_policy_state = initial_fan_decision.state;
    let mut fan_command = initial_fan_decision.command;
    let persistence_locked = ui_state.persistence_locked();
    let _ = sync_frontpanel_runtime_state(
        &mut ui_state,
        initial_fan_decision,
        next_heater_lock_reason_with_persistence(
            persistence_locked,
            heater_controller.fault_latched(),
            cooling_disabled_lock_latched,
            thermal_model_heater_allowed(
                &memory_config,
                calibration_runtime_state,
                manual_pps_state,
            ),
            pd_contract_ready,
        ),
        0,
    );
    ui_state.pd_contract_mv = heater_power_backend.pd_contract_mv();
    apply_fan_output(
        &mut fan_enable,
        &mut fan_pwm,
        fan_command,
        &mut last_fan_command,
    );
    let mut buzzer = BuzzerRuntime;
    let mut last_fault_present = is_overtemp_fault(current_rtd_fault);
    let mut overtemp_attention_acknowledged = false;
    let mut attention_pending_after_fault_clear = false;
    let mut overtemp_forced_fan_active = last_fault_present;
    let mut suppress_attention_ack_input = false;
    let mut suppress_attention_ack_waits_for_event = false;
    let mut suppress_attention_ack_event_seen = false;
    let mut suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
    let mut suppress_attention_ack_clear_after_ms: Option<u64> = None;
    let mut protection_alarm = ProtectionAlarmCadence::new();
    let mut next_attention_reminder_ms: Option<u64> = None;
    if last_fault_present {
        protection_alarm.arm(0);
        buzzer.activate_protection(BuzzerCueSource::Startup, 0);
    }
    let initial_status_light_elapsed_ms = Instant::now()
        .as_millis()
        .saturating_sub(status_light_started_ms);
    let initial_status_light_state = select_status_light_state(StatusLightInputs {
        booting: initial_status_light_elapsed_ms < STATUS_LIGHT_BOOT_DURATION_MS,
        thermal_runaway: is_overtemp_fault(current_rtd_fault),
        sensor_fault: is_sensor_fault(current_rtd_fault),
        heater_interlocked: matches!(
            ui_state.heater_lock_reason,
            Some(
                HeaterLockReason::PdContractUnavailable
                    | HeaterLockReason::ThermalModelMissingForSourceClass
            )
        ),
        heater_enabled: ui_state.heater_enabled,
        fan_enabled: fan_command.enabled,
        ..StatusLightInputs::default()
    });
    set_status_light_state(initial_status_light_state);
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(
        &mut usb_serial,
        b"boot_stage=display_runtime_presentation_start\n",
    );
    let initial_frontpanel_ui_ready = present_initial_frontpanel_ui(
        &mut display,
        canvas,
        InitialFrontpanelContext {
            state: &ui_state,
            i2c: &mut pd_i2c,
            pd_port: &mut pd_port,
            last_pd_observation: &mut last_pd_observation,
            heater_pwm: &mut heater_pwm,
            last_heater_duty: &mut last_heater_duty,
        },
    )
    .await;
    #[cfg(feature = "web_serial")]
    if initial_frontpanel_ui_ready {
        let _ = usb_write_bytes_bounded(
            &mut usb_serial,
            b"boot_stage=display_runtime_presentation_complete\n",
        );
    }
    if !initial_frontpanel_ui_ready {
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            usb_tx_buf,
            &memory_config,
            initial_status_light_state,
            UsbRecoveryPhase::RuntimeFault,
        )
        .await;

        #[cfg(not(feature = "web_serial"))]
        panic!("failed to draw initial frontpanel UI");
    }
    let restore_frame_was_shown = eeprom_restore_pending;
    if eeprom_data_incompatible {
        // Legacy EEPROM decoding is deliberately outside the pre-RTD path.
        // Yield between slots so USB early-control and the status-light task
        // remain serviceable while the explicit restore lock is visible.
        if let Some(scratch) = boot_memory_io_scratch.as_mut() {
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            let (legacy_record, read_failed) = load_legacy_eeprom_memory_record(
                &mut pd_i2c,
                &mut pd_port,
                &mut eeprom_pd_service,
                scratch,
                eeprom_record_staging,
            )
            .await;
            if let Some(record) = legacy_record {
                memory_sequence = record.sequence;
                memory_config = record.config;
                let migration_result = {
                    #[cfg(feature = "web_serial")]
                    let migration_log_sink = &mut usb_serial as &mut dyn PersistenceLogSink;
                    #[cfg(not(feature = "web_serial"))]
                    let migration_log_sink =
                        &mut persistence_log_sink as &mut dyn PersistenceLogSink;
                    let mut eeprom_pd_service = EepromPdServiceContext::new(
                        &mut last_pd_observation,
                        &mut heater_pwm,
                        &mut last_heater_duty,
                    );
                    migrate_legacy_memory_config(
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut eeprom_pd_service,
                        record.sequence,
                        &memory_config,
                        &mut *migration_log_sink,
                        eeprom_record_staging,
                    )
                    .await
                };
                if let Ok(sequence) = migration_result {
                    memory_sequence = sequence;
                    eeprom_required = false;
                    ui_state.eeprom_data_incompatible = false;
                    ui_state.eeprom_required = false;
                    ui_state.persistence_fault_attention_pending = false;
                    ui_state.set_dashboard_presentation(
                        flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                    );
                    apply_memory_config_to_ui(&mut ui_state, &memory_config);
                    active_thermal_settings = ThermalControlProfileSettings::from(
                        memory_config.active_thermal_control_profile.settings,
                    );
                    persistence_source = "eeprom";
                    persistence_record_state = "valid";
                    info!(
                        "legacy memory restore and FPR2 migration complete seq={=u32}",
                        sequence
                    );
                } else {
                    eeprom_required = true;
                    ui_state.eeprom_required = true;
                    eeprom_restore_pending = false;
                    ui_state.set_dashboard_presentation(
                        flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                    );
                    warn!("legacy memory migration failed; keeping heater interlocked");
                }
            } else if read_failed {
                eeprom_required = true;
                ui_state.eeprom_required = true;
                eeprom_restore_pending = false;
                ui_state.set_dashboard_presentation(
                    flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                );
                persistence_source = "none";
                persistence_record_state = "unavailable";
                warn!("legacy memory restore unreadable; keeping heater interlocked");
            }
        }
    }
    if eeprom_required && eeprom_restore_pending {
        // Restore work is complete at this point, even when it failed. Move
        // to the interactive error page so its center long-press can retry.
        if matches!(
            ui_state.dashboard_presentation,
            flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore
        ) {
            ui_state.set_dashboard_presentation(
                flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
            );
        }
    }
    let mut last_persisted_memory_config = memory_config.clone();
    // The first Dashboard frame is independent of Wi-Fi readiness. Start the
    // network control plane only after the trusted RTD presentation is on the
    // panel so radio retries cannot delay the owner-facing startup state.
    #[cfg(feature = "net_http")]
    {
        let mut eeprom_pd_service = EepromPdServiceContext::new(
            &mut last_pd_observation,
            &mut heater_pwm,
            &mut last_heater_duty,
        );
        let mut pd_network_service = PdNetworkServiceContext {
            eeprom: &mut eeprom_pd_service,
            pd_contract_ready: &mut pd_contract_ready,
            ui_state: &mut ui_state,
            calibration_runtime_state: &mut calibration_runtime_state,
            manual_pps: &mut manual_pps_state,
        };
        run_network_operation_with_pd(
            flux_purr_firmware::net::initialize_control_state(memory_config.lan_pairing_token),
            &mut pd_i2c,
            &mut pd_port,
            &mut pd_network_service,
        )
        .await;
    }
    #[cfg(all(feature = "net_http", feature = "web_serial"))]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=lan_control_state_ready\n");
    #[cfg(feature = "net_http")]
    {
        let mut eeprom_pd_service = EepromPdServiceContext::new(
            &mut last_pd_observation,
            &mut heater_pwm,
            &mut last_heater_duty,
        );
        let mut pd_network_service = PdNetworkServiceContext {
            eeprom: &mut eeprom_pd_service,
            pd_contract_ready: &mut pd_contract_ready,
            ui_state: &mut ui_state,
            calibration_runtime_state: &mut calibration_runtime_state,
            manual_pps: &mut manual_pps_state,
        };
        let spawn_result = run_network_operation_with_pd(
            flux_purr_firmware::net::spawn(&_spawner, peripherals.WIFI, &memory_config, |stage| {
                #[cfg(feature = "web_serial")]
                let _ = usb_write_bytes_bounded(&mut usb_serial, stage);
            }),
            &mut pd_i2c,
            &mut pd_port,
            &mut pd_network_service,
        )
        .await;
        if let Err(error) = spawn_result {
            warn!("LAN control plane startup failed: {=str}", error.message());
            run_network_operation_with_pd(
                flux_purr_firmware::net::report_startup_failure(error),
                &mut pd_i2c,
                &mut pd_port,
                &mut pd_network_service,
            )
            .await;
        }
    }
    drop(boot_memory_io_scratch);
    // Keep the explicit restore frame visible in the trace. A successful
    // legacy decode above already promoted the live state; the redraw below
    // replaces the locked frame without waiting for a full EEPROM rewrite.
    log_ui_state(&ui_state);
    #[cfg(feature = "web_serial")]
    {
        let _ = usb_write_bytes_bounded(&mut usb_serial, RUNTIME_READY_BOOT_STAGE_LINE);
        // The daemon may attach after early boot framing; repeat the latched
        // reset cause once JSONL control is ready for post-reset diagnosis.
        let _ = usb_write_bytes_bounded(&mut usb_serial, reset_reason.as_bytes());
    }

    let runtime_started_ms = pd_runtime_started_ms;
    let mut last_control_ms: u64 = 0;
    let mut next_control_deadline_ms = HEATER_CONTROL_INTERVAL_MS;
    // PD protocol work has a tighter deadline than thermal control. Keep it
    // ahead of USB/LAN commands and reuse its latest observation below.
    let mut next_pd_service_deadline_ms = 0;
    let mut heater_control_timing = HeaterControlTiming::default();
    let mut ui_refresh_pending = restore_frame_was_shown;
    let mut next_ui_refresh_ms = DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS;
    // USB automation can open WiFi Info after this loop has already sampled
    // the keys. Do not let an event from that older sample immediately close
    // the newly opened pairing window.
    let mut suppress_pairing_input_until_released = false;
    loop {
        // Yield cooperatively while using the monotonic clock for deadlines.
        #[cfg(feature = "web_serial")]
        embassy_futures::yield_now().await;
        #[cfg(feature = "web_serial")]
        let elapsed_ms = Instant::now()
            .as_millis()
            .saturating_sub(runtime_started_ms);
        let pd_now = PdTimestamp::now();
        let mut needs_redraw = false;

        if pd_runtime_service_due(pd_now.as_millis(), next_pd_service_deadline_ms) {
            next_pd_service_deadline_ms = next_pd_runtime_service_deadline_ms(
                next_pd_service_deadline_ms,
                pd_now.as_millis(),
            );
            let current_pd_observation = read_pd_status(&mut pd_i2c, &mut pd_port, pd_now).await;
            if pd_status_log_key(current_pd_observation) != last_pd_status_log_key {
                match current_pd_observation {
                    Some(observation) => info!(
                        "pd status update status=0x{=u8:02x} pd={=bool} epr={=bool} epr_exist={=bool} current_raw=0x{=u8:02x} current_ma={=u16}",
                        observation.status_raw,
                        observation.status.pd_active,
                        observation.status.epr_active,
                        observation.status.epr_exist,
                        observation.current_raw,
                        observation.current_ma,
                    ),
                    None => info!("pd status update read=failed"),
                }
                last_pd_status_log_key = pd_status_log_key(current_pd_observation);
            }
            last_pd_observation = current_pd_observation;
            needs_redraw |= apply_pd_contract_observation(
                current_pd_observation,
                &mut pd_contract_ready,
                &mut ui_state,
                &mut calibration_runtime_state,
                &mut manual_pps_state,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
        }

        let raw_state = inputs.sample();
        let sample = controller.sample_with_capabilities(
            elapsed_ms,
            raw_state,
            ui_state.gesture_capabilities(),
        );
        let route_before_usb_control = ui_state.route;
        #[cfg(feature = "net_http")]
        let mut control_command_processed = false;
        #[cfg(feature = "web_serial")]
        let mut usb_bytes_processed = 0_u16;
        #[cfg(feature = "web_serial")]
        loop {
            if usb_bytes_processed >= PD_RUNTIME_USB_BYTE_BUDGET {
                break;
            }
            match usb_serial.read_byte() {
                Ok(b'\n') => {
                    let mut eeprom_pd_service = EepromPdServiceContext::new(
                        &mut last_pd_observation,
                        &mut heater_pwm,
                        &mut last_heater_duty,
                    );
                    if let Some(response) = process_eeprom_snapshot_line(
                        usb_rx_line.as_str(),
                        &mut eeprom_snapshot_session,
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut eeprom_pd_service,
                        &mut memory_commit_due_ms,
                        elapsed_ms,
                    )
                    .await
                    {
                        if eeprom_snapshot_storage_failure(&response) {
                            mark_eeprom_required(
                                &mut ui_state,
                                &mut calibration_runtime_state,
                                &mut manual_pps_state,
                                &mut memory_commit_due_ms,
                                None,
                            );
                        }
                        write_eeprom_snapshot_response(&mut usb_serial, &response, usb_tx_buf);
                        usb_rx_line.clear();
                        #[cfg(feature = "net_http")]
                        {
                            control_command_processed = true;
                        }
                        break;
                    }
                    let pd_observation_for_control = last_pd_observation;
                    let heater_duty_for_control = last_heater_duty;
                    let mut eeprom_pd_service = EepromPdServiceContext::new(
                        &mut last_pd_observation,
                        &mut heater_pwm,
                        &mut last_heater_duty,
                    );
                    let (control_needs_redraw, response) = process_control_line(
                        usb_rx_line.as_str(),
                        ControlLineContext {
                            controller: &mut controller,
                            ui_state: &mut ui_state,
                            memory_config: &mut memory_config,
                            last_persisted_memory_config: &mut last_persisted_memory_config,
                            preview_heater_curve: &mut preview_heater_curve,
                            memory_commit_due_ms: &mut memory_commit_due_ms,
                            memory_sequence: &mut memory_sequence,
                            persistence_source,
                            persistence_record_state,
                            pd_i2c: &mut pd_i2c,
                            pd_controller: pd_port.controller_kind(),
                            pd_port: &mut pd_port,
                            eeprom_pd_service: &mut eeprom_pd_service,
                            calibration_runtime_state: &mut calibration_runtime_state,
                            thermal_plant_workspace,
                            elapsed_ms,
                            last_pd_observation: pd_observation_for_control,
                            pd_contract_ready: &mut pd_contract_ready,
                            heater_power_backend: &mut heater_power_backend,
                            heater_controller: &mut heater_controller,
                            pid_snapshot: last_pid_snapshot,
                            manual_pps: &mut manual_pps_state,
                            fan_command,
                            current_rtd_fault,
                            overtemp_attention_acknowledged: &mut overtemp_attention_acknowledged,
                            attention_pending_after_fault_clear:
                                &mut attention_pending_after_fault_clear,
                            overtemp_forced_fan_active: &mut overtemp_forced_fan_active,
                            next_attention_reminder_ms: &mut next_attention_reminder_ms,
                            buzzer: &mut buzzer,
                            thermal_control_profile_preview: &mut thermal_control_profile_preview,
                            last_raw_state,
                            latest_status_temp_c: latest_display_temp_c,
                            latest_control_temp_c: latest_temp_c,
                            control_measurement_guarded,
                            latest_rtd_raw_adc_mv,
                            latest_rtd_raw_adc_min_mv,
                            latest_rtd_raw_adc_max_mv,
                            latest_vin_raw_adc_mv,
                            latest_vin_mv,
                            last_heater_duty: heater_duty_for_control,
                            heater_control_timing,
                            persistence_log_sink: &mut usb_serial,
                            record_staging: eeprom_record_staging,
                        },
                    )
                    .await;
                    needs_redraw |= control_needs_redraw;
                    needs_redraw |=
                        disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
                            calibration_runtime_state: &mut calibration_runtime_state,
                            backend: &mut heater_power_backend,
                            manual_pps: &mut manual_pps_state,
                            i2c: &mut pd_i2c,
                            pd_port: &mut pd_port,
                            heater_pwm: &mut heater_pwm,
                            hold_pps_governor: &mut hold_pps_governor,
                            ui_state: &mut ui_state,
                            last_heater_duty: &mut last_heater_duty,
                            measured_vin_mv: latest_vin_mv,
                        })
                        .await;
                    usb_write_response_frame(&mut usb_serial, &response, usb_tx_buf);
                    usb_rx_line.clear();
                    #[cfg(feature = "net_http")]
                    {
                        control_command_processed = true;
                    }
                    break;
                }
                Ok(b'\r') => {
                    usb_bytes_processed = usb_bytes_processed.saturating_add(1);
                }
                Ok(byte) => {
                    usb_bytes_processed = usb_bytes_processed.saturating_add(1);
                    if usb_rx_line.push(char::from(byte)).is_err() {
                        usb_rx_line.clear();
                    }
                }
                Err(nb::Error::WouldBlock) => break,
                Err(_) => break,
            }
        }
        let pairing_opened_by_usb = route_before_usb_control != FrontPanelRoute::WifiInfo
            && ui_state.route == FrontPanelRoute::WifiInfo;
        if pairing_opened_by_usb {
            suppress_pairing_input_until_released = true;
        }

        #[cfg(feature = "net_http")]
        if !control_command_processed
            && let Some(command) = flux_purr_firmware::net::try_receive_command()
        {
            let request_id = command.request_id;
            let response_slot = command.response_slot;
            let is_mutation = matches!(
                command.method,
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Delete
            );
            let lease_active = {
                let mut eeprom_pd_service = EepromPdServiceContext::new(
                    &mut last_pd_observation,
                    &mut heater_pwm,
                    &mut last_heater_duty,
                );
                let mut pd_network_service = PdNetworkServiceContext {
                    eeprom: &mut eeprom_pd_service,
                    pd_contract_ready: &mut pd_contract_ready,
                    ui_state: &mut ui_state,
                    calibration_runtime_state: &mut calibration_runtime_state,
                    manual_pps: &mut manual_pps_state,
                };
                run_network_operation_with_pd(
                    flux_purr_firmware::net::command_lease_is_active(&command),
                    &mut pd_i2c,
                    &mut pd_port,
                    &mut pd_network_service,
                )
                .await
            };
            if is_mutation && !lease_active {
                flux_purr_firmware::net::respond_to_command(
                    response_slot,
                    request_id,
                    409,
                    lan_error_json(
                        "lease_expired",
                        "The LAN lease expired before this command reached the control loop.",
                    ),
                    false,
                );
                continue;
            }
            if is_mutation
                && flux_purr_firmware::net_http::validate_control_revision(
                    command.expected_revision,
                    flux_purr_firmware::net::current_control_revision(),
                )
                .is_err()
            {
                flux_purr_firmware::net::respond_to_command(
                    response_slot,
                    request_id,
                    409,
                    lan_error_json(
                        "stale_write",
                        "The control state changed after this client last read it.",
                    ),
                    false,
                );
                continue;
            }
            let direct_response = match (command.endpoint, command.method) {
                (LanEndpoint::Identity, HttpMethod::Get) => {
                    let identity = {
                        let mut eeprom_pd_service = EepromPdServiceContext::new(
                            &mut last_pd_observation,
                            &mut heater_pwm,
                            &mut last_heater_duty,
                        );
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut eeprom_pd_service,
                            pd_contract_ready: &mut pd_contract_ready,
                            ui_state: &mut ui_state,
                            calibration_runtime_state: &mut calibration_runtime_state,
                            manual_pps: &mut manual_pps_state,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::lan_identity(),
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut pd_network_service,
                        )
                        .await
                    };
                    Some(lan_json_response(&identity))
                }
                (LanEndpoint::Network, HttpMethod::Get) => {
                    let network = {
                        let mut eeprom_pd_service = EepromPdServiceContext::new(
                            &mut last_pd_observation,
                            &mut heater_pwm,
                            &mut last_heater_duty,
                        );
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut eeprom_pd_service,
                            pd_contract_ready: &mut pd_contract_ready,
                            ui_state: &mut ui_state,
                            calibration_runtime_state: &mut calibration_runtime_state,
                            manual_pps: &mut manual_pps_state,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::lan_network_summary(),
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut pd_network_service,
                        )
                        .await
                    };
                    Some(lan_json_response(&network))
                }
                _ => None,
            };
            if let Some((status, body)) = direct_response {
                flux_purr_firmware::net::respond_to_command(
                    response_slot,
                    request_id,
                    status,
                    body,
                    false,
                );
                continue;
            }
            let line = match lan_command_to_control_line(&command) {
                Ok(line) => line,
                Err(message) => {
                    flux_purr_firmware::net::respond_to_command(
                        response_slot,
                        request_id,
                        400,
                        lan_error_json("unsupported_lan_command", message),
                        false,
                    );
                    continue;
                }
            };
            let pd_observation_for_control = last_pd_observation;
            let heater_duty_for_control = last_heater_duty;
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            let (control_needs_redraw, response) = process_control_line(
                line.as_str(),
                ControlLineContext {
                    controller: &mut controller,
                    ui_state: &mut ui_state,
                    memory_config: &mut memory_config,
                    last_persisted_memory_config: &mut last_persisted_memory_config,
                    preview_heater_curve: &mut preview_heater_curve,
                    memory_commit_due_ms: &mut memory_commit_due_ms,
                    memory_sequence: &mut memory_sequence,
                    persistence_source,
                    persistence_record_state,
                    pd_i2c: &mut pd_i2c,
                    pd_controller: pd_port.controller_kind(),
                    pd_port: &mut pd_port,
                    eeprom_pd_service: &mut eeprom_pd_service,
                    calibration_runtime_state: &mut calibration_runtime_state,
                    thermal_plant_workspace,
                    elapsed_ms,
                    last_pd_observation: pd_observation_for_control,
                    pd_contract_ready: &mut pd_contract_ready,
                    heater_power_backend: &mut heater_power_backend,
                    heater_controller: &mut heater_controller,
                    pid_snapshot: last_pid_snapshot,
                    manual_pps: &mut manual_pps_state,
                    fan_command,
                    current_rtd_fault,
                    overtemp_attention_acknowledged: &mut overtemp_attention_acknowledged,
                    attention_pending_after_fault_clear: &mut attention_pending_after_fault_clear,
                    overtemp_forced_fan_active: &mut overtemp_forced_fan_active,
                    next_attention_reminder_ms: &mut next_attention_reminder_ms,
                    buzzer: &mut buzzer,
                    thermal_control_profile_preview: &mut thermal_control_profile_preview,
                    last_raw_state,
                    latest_status_temp_c: latest_display_temp_c,
                    latest_control_temp_c: latest_temp_c,
                    control_measurement_guarded,
                    latest_rtd_raw_adc_mv,
                    latest_rtd_raw_adc_min_mv,
                    latest_rtd_raw_adc_max_mv,
                    latest_vin_raw_adc_mv,
                    latest_vin_mv,
                    last_heater_duty: heater_duty_for_control,
                    heater_control_timing,
                    persistence_log_sink: &mut usb_serial,
                    record_staging: eeprom_record_staging,
                },
            )
            .await;
            needs_redraw |= control_needs_redraw;
            needs_redraw |= disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
                calibration_runtime_state: &mut calibration_runtime_state,
                backend: &mut heater_power_backend,
                manual_pps: &mut manual_pps_state,
                i2c: &mut pd_i2c,
                pd_port: &mut pd_port,
                heater_pwm: &mut heater_pwm,
                hold_pps_governor: &mut hold_pps_governor,
                ui_state: &mut ui_state,
                last_heater_duty: &mut last_heater_duty,
                measured_vin_mv: latest_vin_mv,
            })
            .await;
            let network_summary = {
                let mut eeprom_pd_service = EepromPdServiceContext::new(
                    &mut last_pd_observation,
                    &mut heater_pwm,
                    &mut last_heater_duty,
                );
                let mut pd_network_service = PdNetworkServiceContext {
                    eeprom: &mut eeprom_pd_service,
                    pd_contract_ready: &mut pd_contract_ready,
                    ui_state: &mut ui_state,
                    calibration_runtime_state: &mut calibration_runtime_state,
                    manual_pps: &mut manual_pps_state,
                };
                run_network_operation_with_pd(
                    flux_purr_firmware::net::lan_network_summary(),
                    &mut pd_i2c,
                    &mut pd_port,
                    &mut pd_network_service,
                )
                .await
            };
            let (status, body) = lan_frame_response(&response, network_summary);
            flux_purr_firmware::net::respond_to_command(
                response_slot,
                request_id,
                status,
                body,
                is_mutation,
            );
        }
        #[cfg(feature = "net_http")]
        let persisted_token_change = {
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            let mut pd_network_service = PdNetworkServiceContext {
                eeprom: &mut eeprom_pd_service,
                pd_contract_ready: &mut pd_contract_ready,
                ui_state: &mut ui_state,
                calibration_runtime_state: &mut calibration_runtime_state,
                manual_pps: &mut manual_pps_state,
            };
            run_network_operation_with_pd(
                flux_purr_firmware::net::take_persisted_token_change(),
                &mut pd_i2c,
                &mut pd_port,
                &mut pd_network_service,
            )
            .await
        };
        #[cfg(feature = "net_http")]
        if let Some(token) = persisted_token_change {
            memory_config.lan_pairing_token = token;
            memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
        }

        // EEPROM/display work may have serviced PD while the normal deadline
        // gate was suspended. Reconcile that observation before any thermal,
        // UI, or LAN state is consumed so a failed read cannot authorize stale
        // heater intent.
        needs_redraw |= apply_pd_contract_observation(
            last_pd_observation,
            &mut pd_contract_ready,
            &mut ui_state,
            &mut calibration_runtime_state,
            &mut manual_pps_state,
            &mut heater_pwm,
            &mut last_heater_duty,
        );

        needs_redraw |= disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
            calibration_runtime_state: &mut calibration_runtime_state,
            backend: &mut heater_power_backend,
            manual_pps: &mut manual_pps_state,
            i2c: &mut pd_i2c,
            pd_port: &mut pd_port,
            heater_pwm: &mut heater_pwm,
            hold_pps_governor: &mut hold_pps_governor,
            ui_state: &mut ui_state,
            last_heater_duty: &mut last_heater_duty,
            measured_vin_mv: latest_vin_mv,
        })
        .await;

        if sample.raw_state != last_raw_state {
            if should_consume_attention_raw_input(
                overtemp_attention_requires_ack(
                    is_overtemp_fault(current_rtd_fault),
                    overtemp_attention_acknowledged,
                    attention_pending_after_fault_clear,
                ),
                suppress_attention_ack_input,
                last_raw_state,
                sample.raw_state,
            ) && acknowledge_overtemp_attention(
                is_overtemp_fault(current_rtd_fault),
                &mut overtemp_attention_acknowledged,
                &mut attention_pending_after_fault_clear,
                &mut overtemp_forced_fan_active,
                &mut next_attention_reminder_ms,
                &mut buzzer,
            ) {
                suppress_attention_ack_input = true;
                suppress_attention_ack_event_seen = false;
                suppress_attention_ack_clear_after_ms = None;
                suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
                suppress_attention_ack_waits_for_event =
                    sample.raw_state.first_pressed().is_some_and(|raw_key| {
                        let key = FrontPanelKeyMap::default().logical_from_raw(raw_key);
                        let gestures = ui_state.gesture_capabilities().gestures_for(key);
                        if gestures.supports(KeyGesture::DoublePress) {
                            suppress_attention_ack_clear_delay_ms =
                                FRONTPANEL_DOUBLE_CLICK_MS.saturating_add(FRONTPANEL_DEBOUNCE_MS);
                        }
                        gestures.supports(KeyGesture::ShortPress)
                            || gestures.supports(KeyGesture::DoublePress)
                            || gestures.supports(KeyGesture::LongPress)
                    });
                info!(
                    "fault attention reminder acknowledged -> consume raw input mask={=u8}",
                    sample.raw_state.pressed_mask(),
                );
            }
            ui_state.set_raw_state(sample.raw_state);
            last_raw_state = sample.raw_state;
            info!("raw mask={=u8}", sample.raw_state.pressed_mask());
            if runtime_mode == FrontPanelRuntimeMode::KeyTest {
                needs_redraw = true;
            }
        }

        for event in sample.events {
            if suppress_pairing_input_until_released {
                continue;
            }
            let route_before = ui_state.route;
            let heater_enabled_before = ui_state.heater_enabled;
            let active_cooling_enabled_before = ui_state.active_cooling_enabled;
            info!(
                "key raw={=str} logical={=str} gesture={=str} at_ms={=u64}",
                event.raw_key.label(),
                event.key.label(),
                event.gesture.label(),
                event.at_ms,
            );
            if suppress_attention_ack_input {
                info!(
                    "fault attention acknowledgement suppresses event raw={=str} logical={=str} gesture={=str}",
                    event.raw_key.label(),
                    event.key.label(),
                    event.gesture.label(),
                );
                suppress_attention_ack_event_seen = true;
                continue;
            }
            if acknowledge_overtemp_attention(
                is_overtemp_fault(current_rtd_fault),
                &mut overtemp_attention_acknowledged,
                &mut attention_pending_after_fault_clear,
                &mut overtemp_forced_fan_active,
                &mut next_attention_reminder_ms,
                &mut buzzer,
            ) {
                info!(
                    "fault attention reminder acknowledged -> consume input raw={=str} logical={=str} gesture={=str}",
                    event.raw_key.label(),
                    event.key.label(),
                    event.gesture.label(),
                );
                continue;
            }
            let interaction_handled = ui_state.handle_event(event);
            if ui_state.take_persistence_retry_request() {
                let changed =
                    persist_domain_mask_between(&memory_config, &last_persisted_memory_config);
                let retry_blank_initialization = eeprom_required
                    && memory_sequence == 0
                    && !eeprom_data_incompatible
                    && !prepared_layout_recovery_pending;
                let retry_domains = if eeprom_required {
                    PersistDomainMask::ALL
                } else {
                    changed.union(PersistDomainMask::from_fault(
                        ui_state.persistence_fault.as_ref(),
                    ))
                };
                let retry_format_recovery = retry_blank_initialization
                    || prepared_layout_recovery_pending
                    || eeprom_data_incompatible;
                let mut eeprom_pd_service = EepromPdServiceContext::new(
                    &mut last_pd_observation,
                    &mut heater_pwm,
                    &mut last_heater_duty,
                );
                let retry_result = {
                    #[cfg(feature = "web_serial")]
                    let retry_log_sink = &mut usb_serial as &mut dyn PersistenceLogSink;
                    #[cfg(not(feature = "web_serial"))]
                    let retry_log_sink = &mut persistence_log_sink as &mut dyn PersistenceLogSink;
                    if prepared_layout_recovery_pending {
                        recover_prepared_fpr2_layout(
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut eeprom_pd_service,
                            memory_sequence,
                            retry_log_sink,
                            eeprom_record_staging,
                        )
                        .await
                    } else if eeprom_data_incompatible {
                        let mut scratch = new_memory_io_scratch();
                        let (legacy_record, read_failed) = load_legacy_eeprom_memory_record(
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut eeprom_pd_service,
                            &mut scratch,
                            eeprom_record_staging,
                        )
                        .await;
                        if let Some(record) = legacy_record {
                            memory_sequence = record.sequence;
                            memory_config = record.config;
                            migrate_legacy_memory_config(
                                &mut pd_i2c,
                                &mut pd_port,
                                &mut eeprom_pd_service,
                                memory_sequence,
                                &memory_config,
                                retry_log_sink,
                                eeprom_record_staging,
                            )
                            .await
                            .map(|sequence| {
                                memory_sequence = sequence;
                            })
                        } else {
                            Err(MemoryCommitFailure {
                                error: if read_failed {
                                    MemoryCommitError::VerifyUnreadable
                                } else {
                                    MemoryCommitError::VerifyMismatch
                                },
                                phase: "legacy-retry-read",
                                attempt: 1,
                                sequence: memory_sequence,
                                domain: PersistDomain::LayoutMarker,
                                slot: PersistSlot::Single,
                            })
                        }
                    } else if retry_blank_initialization {
                        match initialize_fpr2_defaults(
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut eeprom_pd_service,
                            retry_log_sink,
                            eeprom_record_staging,
                        )
                        .await
                        {
                            Some(sequence) => {
                                memory_sequence = sequence;
                                Ok(())
                            }
                            None => Err(MemoryCommitFailure {
                                error: MemoryCommitError::VerifyUnreadable,
                                phase: "active-init-retry",
                                attempt: 1,
                                sequence: memory_sequence.saturating_add(1),
                                domain: PersistDomain::LayoutMarker,
                                slot: PersistSlot::A,
                            }),
                        }
                    } else {
                        commit_memory_config_now(
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut eeprom_pd_service,
                            CommitMemoryConfigInput {
                                memory_sequence: &mut memory_sequence,
                                memory_config: &memory_config,
                                domains_to_write: retry_domains,
                                persistence_log_sink: retry_log_sink,
                                record_staging: eeprom_record_staging,
                            },
                        )
                        .await
                    }
                };
                match retry_result {
                    Ok(()) => {
                        if retry_format_recovery {
                            last_persisted_memory_config = memory_config.clone();
                            eeprom_required = false;
                            eeprom_data_incompatible = false;
                            prepared_layout_recovery_pending = false;
                            ui_state.eeprom_required = false;
                            ui_state.eeprom_data_incompatible = false;
                            ui_state.persistence_fault = None;
                            ui_state.persistence_fault_attention_pending = false;
                            let _ = ui_state.set_dashboard_presentation(
                                flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                            );
                        } else {
                            copy_persisted_domains(
                                &mut last_persisted_memory_config,
                                &memory_config,
                                retry_domains,
                            );
                            let retried_safety_domain = retry_domains
                                .includes(PersistDomain::SafetyCalibration)
                                || retry_domains.includes(PersistDomain::ThermalPolicy);
                            if retried_safety_domain {
                                eeprom_required = false;
                                ui_state.eeprom_required = false;
                                ui_state.eeprom_data_incompatible = false;
                                ui_state.persistence_fault = None;
                                ui_state.persistence_fault_attention_pending = false;
                            } else if !ui_state.persistence_locked() {
                                ui_state.persistence_fault = None;
                                ui_state.persistence_fault_attention_pending = false;
                            }
                        }
                        persistence_source = "eeprom";
                        persistence_record_state = "valid";
                        needs_redraw = true;
                    }
                    Err(error) => {
                        ui_state.persistence_fault = Some(persistence_fault_from_commit(error));
                        ui_state.persistence_fault_attention_pending = true;
                        needs_redraw = true;
                    }
                }
            }
            if route_before != FrontPanelRoute::WifiInfo
                && ui_state.route == FrontPanelRoute::WifiInfo
            {
                #[cfg(feature = "net_http")]
                {
                    let code = {
                        let mut eeprom_pd_service = EepromPdServiceContext::new(
                            &mut last_pd_observation,
                            &mut heater_pwm,
                            &mut last_heater_duty,
                        );
                        let mut pd_network_service = PdNetworkServiceContext {
                            eeprom: &mut eeprom_pd_service,
                            pd_contract_ready: &mut pd_contract_ready,
                            ui_state: &mut ui_state,
                            calibration_runtime_state: &mut calibration_runtime_state,
                            manual_pps: &mut manual_pps_state,
                        };
                        run_network_operation_with_pd(
                            flux_purr_firmware::net::enter_pairing(),
                            &mut pd_i2c,
                            &mut pd_port,
                            &mut pd_network_service,
                        )
                        .await
                    };
                    ui_state.enter_wifi_pairing(code);
                    info!("LAN pairing window opened from WiFi Info page");
                }
                #[cfg(not(feature = "net_http"))]
                ui_state.leave_wifi_pairing();
            } else if route_before == FrontPanelRoute::WifiInfo
                && ui_state.route != FrontPanelRoute::WifiInfo
            {
                #[cfg(feature = "net_http")]
                {
                    let mut eeprom_pd_service = EepromPdServiceContext::new(
                        &mut last_pd_observation,
                        &mut heater_pwm,
                        &mut last_heater_duty,
                    );
                    let mut pd_network_service = PdNetworkServiceContext {
                        eeprom: &mut eeprom_pd_service,
                        pd_contract_ready: &mut pd_contract_ready,
                        ui_state: &mut ui_state,
                        calibration_runtime_state: &mut calibration_runtime_state,
                        manual_pps: &mut manual_pps_state,
                    };
                    run_network_operation_with_pd(
                        flux_purr_firmware::net::leave_pairing(),
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut pd_network_service,
                    )
                    .await;
                    ui_state.leave_wifi_pairing();
                    info!("LAN pairing window closed after leaving WiFi Info page");
                }
                #[cfg(not(feature = "net_http"))]
                ui_state.leave_wifi_pairing();
            }
            if interaction_handled {
                needs_redraw = true;
            }
            let mut specialized_feedback_played = false;
            if ui_state.active_cooling_enabled != active_cooling_enabled_before {
                buzzer.request_feedback(
                    BuzzerCueSource::FrontPanel,
                    if ui_state.active_cooling_enabled {
                        BuzzerCueId::ActiveCoolingOn
                    } else {
                        BuzzerCueId::ActiveCoolingOff
                    },
                    elapsed_ms,
                );
                info!(
                    "active cooling policy -> {=str}",
                    if ui_state.active_cooling_enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                specialized_feedback_played = true;
                if ui_state.active_cooling_enabled {
                    cooling_disabled_lock_latched = false;
                    cooling_disabled_lock_armed = true;
                }
            }
            if ui_state.heater_enabled != heater_enabled_before {
                if ui_state.heater_enabled {
                    if cooling_disabled_lock_latched {
                        cooling_disabled_lock_latched = false;
                        cooling_disabled_lock_armed = false;
                        info!("heater re-arm -> cleared cooling-disabled lock");
                    }
                    if heater_controller.fault_latched().is_some() {
                        if let Some(reason) = current_rtd_fault {
                            ui_state.heater_enabled = false;
                            buzzer.request_feedback(
                                BuzzerCueSource::FrontPanel,
                                BuzzerCueId::HeaterReject,
                                elapsed_ms,
                            );
                            specialized_feedback_played = true;
                            needs_redraw = true;
                            info!("heater re-arm blocked reason={=str}", reason.label(),);
                        } else {
                            heater_controller.clear_fault_latch();
                            buzzer.request_feedback(
                                BuzzerCueSource::FrontPanel,
                                BuzzerCueId::HeaterOn,
                                elapsed_ms,
                            );
                            specialized_feedback_played = true;
                            info!("heater re-arm -> cleared latched fault");
                        }
                    } else {
                        buzzer.request_feedback(
                            BuzzerCueSource::FrontPanel,
                            BuzzerCueId::HeaterOn,
                            elapsed_ms,
                        );
                        specialized_feedback_played = true;
                        info!("heater arm -> on");
                    }
                } else {
                    buzzer.request_feedback(
                        BuzzerCueSource::FrontPanel,
                        BuzzerCueId::HeaterOff,
                        elapsed_ms,
                    );
                    specialized_feedback_played = true;
                    info!("heater arm -> off");
                }
            }
            if maybe_play_frontpanel_ui_input_feedback(
                interaction_handled,
                specialized_feedback_played,
                &mut buzzer,
                elapsed_ms,
            ) {
                info!(
                    "ui input feedback -> route={=str} key={=str} gesture={=str}",
                    route_label(ui_state.route),
                    event.key.label(),
                    event.gesture.label(),
                );
            }
            if interaction_handled {
                let next_memory_config = memory_config_from_ui(&ui_state, &memory_config);
                if next_memory_config != memory_config {
                    memory_config = next_memory_config;
                    memory_commit_due_ms =
                        Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                    info!(
                        "memory dirty -> debounce_until_ms={=u64} target_c={=i16} slot={=u8} active_cooling={=bool}",
                        memory_commit_due_ms.unwrap_or(0),
                        memory_config.target_temp_c,
                        memory_config.selected_preset_slot as u8,
                        memory_config.active_cooling_enabled,
                    );
                }
            }
        }
        if suppress_pairing_input_until_released
            && !pairing_opened_by_usb
            && sample.raw_state.first_pressed().is_none()
        {
            suppress_pairing_input_until_released = false;
        }
        if suppress_attention_ack_input
            && suppress_attention_ack_waits_for_event
            && sample.raw_state.pressed_mask() == 0
            && suppress_attention_ack_clear_after_ms.is_none()
        {
            suppress_attention_ack_clear_after_ms =
                Some(elapsed_ms.saturating_add(suppress_attention_ack_clear_delay_ms));
        }
        if should_clear_attention_ack_suppression(
            suppress_attention_ack_input,
            suppress_attention_ack_waits_for_event,
            suppress_attention_ack_event_seen,
            sample.raw_state,
            suppress_attention_ack_clear_after_ms,
            elapsed_ms,
        ) {
            suppress_attention_ack_input = false;
            suppress_attention_ack_waits_for_event = false;
            suppress_attention_ack_event_seen = false;
            suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
            suppress_attention_ack_clear_after_ms = None;
        }

        if elapsed_ms >= next_control_deadline_ms {
            let control_started_ms = elapsed_ms;
            heater_control_timing.interval_ms = control_started_ms
                .saturating_sub(last_control_ms)
                .min(u64::from(u16::MAX)) as u16;
            last_control_ms = elapsed_ms;
            next_control_deadline_ms =
                next_heater_control_deadline_ms(next_control_deadline_ms, control_started_ms);
            let active_thermal_control_profile = active_thermal_control_profile(
                &memory_config,
                thermal_control_profile_preview,
                &manual_pps_state,
            );
            let active_thermal_settings = active_thermal_control_profile
                .filter(|_| calibration_runtime_state.mode != CalibrationMode::Off)
                .map(|profile| profile.settings)
                .unwrap_or_default();

            let previous_vin_raw_adc_mv = latest_vin_raw_adc_mv;
            let current_request_mv = heater_power_backend.pd_request_mv();
            let mut rtd_sample = read_rtd_sample_with_pd(
                &mut adc1,
                &mut rtd_adc_pin,
                adc_curve.as_ref(),
                &memory_config,
                &mut PdAdcService {
                    i2c: &mut pd_i2c,
                    pd_port: &mut pd_port,
                    last_pd_observation: &mut last_pd_observation,
                    heater_pwm: &mut heater_pwm,
                    last_heater_duty: &mut last_heater_duty,
                },
            )
            .await;
            let first_rtd_snapshot = match &rtd_sample {
                RtdSample::Valid(measurement) => {
                    (measurement.raw_adc_mv, measurement.temp_c, "valid")
                }
                RtdSample::Fault { adc_mv, reason } => (adc_mv.unwrap_or(0), 0.0, reason.label()),
            };

            if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) =
                read_calibrated_vin_mv_with_pd(
                    &mut adc1,
                    &mut vin_adc_pin,
                    adc_curve.as_ref(),
                    &memory_config,
                    &mut PdAdcService {
                        i2c: &mut pd_i2c,
                        pd_port: &mut pd_port,
                        last_pd_observation: &mut last_pd_observation,
                        heater_pwm: &mut heater_pwm,
                        last_heater_duty: &mut last_heater_duty,
                    },
                )
                .await
            {
                let retry_rtd_after_power_step = should_retry_rtd_sample_after_power_step(
                    last_rtd_sample_request_mv,
                    current_request_mv,
                    previous_vin_raw_adc_mv,
                    raw_adc_mv,
                );
                latest_vin_raw_adc_mv = raw_adc_mv;
                if latest_vin_mv != vin_mv {
                    latest_vin_mv = vin_mv;
                    needs_redraw = true;
                }
                needs_redraw |= reconcile_pd_contract_with_vin(
                    &mut pd_contract_vin_guard,
                    last_pd_observation,
                    Some(vin_mv),
                    PdTimestamp::now().as_millis(),
                    PdContractVinContext {
                        pd_port: &mut pd_port,
                        last_pd_observation: &mut last_pd_observation,
                        pd_contract_ready: &mut pd_contract_ready,
                        ui_state: &mut ui_state,
                        calibration_runtime_state: &mut calibration_runtime_state,
                        manual_pps_state: &mut manual_pps_state,
                        heater_pwm: &mut heater_pwm,
                        last_heater_duty: &mut last_heater_duty,
                    },
                );
                info!(
                    "vin sample raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
                    raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
                );
                if retry_rtd_after_power_step {
                    rtd_sample = read_rtd_sample_with_pd(
                        &mut adc1,
                        &mut rtd_adc_pin,
                        adc_curve.as_ref(),
                        &memory_config,
                        &mut PdAdcService {
                            i2c: &mut pd_i2c,
                            pd_port: &mut pd_port,
                            last_pd_observation: &mut last_pd_observation,
                            heater_pwm: &mut heater_pwm,
                            last_heater_duty: &mut last_heater_duty,
                        },
                    )
                    .await;
                    let second_rtd_snapshot = match &rtd_sample {
                        RtdSample::Valid(measurement) => {
                            (measurement.raw_adc_mv, measurement.temp_c, "valid")
                        }
                        RtdSample::Fault { adc_mv, reason } => {
                            (adc_mv.unwrap_or(0), 0.0, reason.label())
                        }
                    };
                    let _ = (
                        last_rtd_sample_request_mv,
                        current_request_mv,
                        previous_vin_raw_adc_mv,
                        raw_adc_mv,
                        first_rtd_snapshot,
                        second_rtd_snapshot,
                    );
                }
            }

            preserve_rtd_control_guard_when_heater_disabled(
                ui_state.heater_enabled,
                &mut control_measurement_guarded,
            );

            let calibration_live_rtd_temp_c = match rtd_sample {
                RtdSample::Valid(measurement) => {
                    latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
                    latest_rtd_raw_adc_min_mv = measurement.raw_adc_min_mv;
                    latest_rtd_raw_adc_max_mv = measurement.raw_adc_max_mv;
                    needs_redraw |= apply_valid_rtd_measurement(
                        RuntimeDisplayTemperatureState {
                            ui_state: &mut ui_state,
                            latest_display_temp_c: &mut latest_display_temp_c,
                            latest_display_temp_i16: &mut latest_display_temp_i16,
                        },
                        RuntimeControlTemperatureState {
                            latest_control_temp_c: &mut latest_temp_c,
                            latest_control_temp_i16: &mut latest_temp_i16,
                            transition_guard: &mut rtd_pps_transition_guard,
                            measurement_guard: &mut rtd_control_measurement_guard,
                            control_measurement_guarded: &mut control_measurement_guarded,
                            heater_controller: &mut heater_controller,
                        },
                        current_request_mv,
                        elapsed_ms,
                        measurement.temp_c,
                    );
                    current_rtd_fault = overtemp_fault_from_control_temperature(measurement.temp_c);
                    Some(measurement.temp_c)
                }
                RtdSample::Fault { adc_mv, reason } => {
                    latest_rtd_raw_adc_mv = adc_mv.unwrap_or(0);
                    latest_rtd_raw_adc_min_mv = latest_rtd_raw_adc_mv;
                    latest_rtd_raw_adc_max_mv = latest_rtd_raw_adc_mv;
                    current_rtd_fault = Some(reason);
                    rtd_control_measurement_guard.clear();
                    control_measurement_guarded = false;
                    clear_runtime_temperature(&mut latest_temp_c, &mut latest_temp_i16);
                    needs_redraw |= retain_runtime_display_temperature(
                        &mut ui_state,
                        &mut latest_display_temp_c,
                        &mut latest_display_temp_i16,
                    );
                    info!(
                        "rtd fault adc_mv={=u16} reason={=str} heater_arm={=bool}",
                        adc_mv.unwrap_or(0),
                        reason.label(),
                        ui_state.heater_enabled,
                    );
                    None
                }
            };
            last_rtd_sample_request_mv = current_request_mv;

            if let Some(reason) = current_rtd_fault
                && heater_controller.latch_fault(reason)
            {
                ui_state.heater_enabled = false;
                needs_redraw = true;
                info!("heater fault latched reason={=str}", reason.label());
            }

            let fault_present = is_overtemp_fault(current_rtd_fault);
            let attention_state_changed = update_fault_attention_state(
                fault_present,
                FaultAttentionState {
                    last_fault_present: &mut last_fault_present,
                    attention_acknowledged: &mut overtemp_attention_acknowledged,
                    attention_pending_after_fault_clear: &mut attention_pending_after_fault_clear,
                    forced_fan_active: &mut overtemp_forced_fan_active,
                    protection_alarm: &mut protection_alarm,
                    next_attention_reminder_ms: &mut next_attention_reminder_ms,
                },
                latest_display_temp_i16,
                &mut buzzer,
                elapsed_ms,
            );
            if attention_state_changed && fault_present {
                info!("protection alarm -> active");
            } else if attention_state_changed && !fault_present {
                info!(
                    "protection cleared -> reminder pending interval_ms={=u64}",
                    BUZZER_ATTENTION_REMINDER_INTERVAL_MS,
                );
            }

            let current_pd_observation = last_pd_observation;
            let mut fusb302b_capabilities_changed = false;
            if pd_port.controller_kind() == ControllerKind::Fusb302b {
                let capabilities = read_pd_power_capabilities(&mut pd_i2c, &mut pd_port);
                if capabilities != last_fusb302b_power_capabilities {
                    fusb302b_capabilities_changed = true;
                    last_fusb302b_power_capabilities = capabilities;
                    heater_power_backend =
                        refresh_fusb302b_heater_power_backend(heater_power_backend, capabilities);
                    manual_pps_state = ManualPpsState::from_fusb302b_capabilities(capabilities);
                    hold_pps_governor = HoldPpsGovernor::new();
                    needs_redraw = true;
                    info!("fusb302b source capabilities changed; refreshed heater power bounds");
                }
            }
            update_calibration_runtime_state(
                &mut calibration_runtime_state,
                &manual_pps_state,
                latest_rtd_raw_adc_mv,
                latest_vin_raw_adc_mv,
            );
            let memory_before_calibration_job = memory_config.clone();
            let thermal_plant_was_running = calibration_runtime_state.mode
                == CalibrationMode::ThermalPlant
                && calibration_runtime_state.job.kind == Some(CalibrationJobKind::ThermalPlant)
                && calibration_runtime_state.job.status == CalibrationJobStatus::Running;
            if current_rtd_fault.is_some()
                && calibration_runtime_state.mode == CalibrationMode::ThermalPlant
                && calibration_runtime_state.job.status == CalibrationJobStatus::Running
            {
                calibration_job_fail(
                    &mut calibration_runtime_state,
                    ManualPpsError::WriteFailed,
                    true,
                    &mut manual_pps_state,
                );
            } else {
                let calibration_temp_c = thermal_plant_calibration_temperature_c(
                    calibration_runtime_state,
                    calibration_live_rtd_temp_c,
                    latest_temp_c,
                );
                update_calibration_job_state(
                    &mut calibration_runtime_state,
                    &mut memory_config,
                    &mut manual_pps_state,
                    thermal_plant_workspace,
                    CalibrationJobUpdateInput {
                        latest_rtd_raw_adc_mv,
                        latest_vin_raw_adc_mv,
                        latest_temp_c: calibration_temp_c,
                        pd_current_ma: current_pd_observation
                            .map(|observation| observation.current_ma)
                            .unwrap_or(0),
                        latest_vin_mv,
                        heater_duty_percent: last_heater_duty,
                    },
                );
            }
            if fusb302b_capabilities_changed {
                disarm_calibration_after_capability_refresh(
                    &mut calibration_runtime_state,
                    &mut manual_pps_state,
                );
            }
            let thermal_plant_completed = calibration_runtime_state.mode == CalibrationMode::Off
                && calibration_runtime_state.job.kind == Some(CalibrationJobKind::ThermalPlant)
                && calibration_runtime_state.job.status == CalibrationJobStatus::Completed;
            if memory_config != memory_before_calibration_job {
                if thermal_plant_completed {
                    let mut eeprom_pd_service = EepromPdServiceContext::new(
                        &mut last_pd_observation,
                        &mut heater_pwm,
                        &mut last_heater_duty,
                    );
                    if let Err(error) = commit_memory_config_now(
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut eeprom_pd_service,
                        CommitMemoryConfigInput {
                            memory_sequence: &mut memory_sequence,
                            memory_config: &memory_config,
                            domains_to_write: PersistDomainMask::SAFETY
                                .union(PersistDomainMask::THERMAL_PLANT),
                            #[cfg(feature = "web_serial")]
                            persistence_log_sink: &mut usb_serial,
                            #[cfg(not(feature = "web_serial"))]
                            persistence_log_sink: &mut persistence_log_sink,
                            record_staging: eeprom_record_staging,
                        },
                    )
                    .await
                    {
                        let code = error.code();
                        restore_persisted_memory_domains(
                            &mut memory_config,
                            &mut ui_state,
                            &last_persisted_memory_config,
                            PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
                        );
                        mark_eeprom_required(
                            &mut ui_state,
                            &mut calibration_runtime_state,
                            &mut manual_pps_state,
                            &mut memory_commit_due_ms,
                            Some(persistence_fault_from_commit(error)),
                        );
                        calibration_job_fail(
                            &mut calibration_runtime_state,
                            ManualPpsError::WriteFailed,
                            false,
                            &mut manual_pps_state,
                        );
                        info!("thermal plant activation commit failed reason={=str}", code);
                    } else {
                        copy_persisted_domains(
                            &mut last_persisted_memory_config,
                            &memory_config,
                            PersistDomainMask::SAFETY.union(PersistDomainMask::THERMAL_PLANT),
                        );
                        memory_commit_due_ms = None;
                    }
                } else {
                    memory_commit_due_ms =
                        Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                }
            }
            let calibration_output_temp_c = thermal_plant_calibration_temperature_c(
                calibration_runtime_state,
                calibration_live_rtd_temp_c,
                latest_temp_c,
            );
            let force_thermal_plant_output_off = !pd_contract_ready
                || thermal_plant_output_must_be_off(
                    calibration_runtime_state,
                    thermal_plant_was_running,
                    calibration_output_temp_c,
                );
            needs_redraw |= disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
                calibration_runtime_state: &mut calibration_runtime_state,
                backend: &mut heater_power_backend,
                manual_pps: &mut manual_pps_state,
                i2c: &mut pd_i2c,
                pd_port: &mut pd_port,
                heater_pwm: &mut heater_pwm,
                hold_pps_governor: &mut hold_pps_governor,
                ui_state: &mut ui_state,
                last_heater_duty: &mut last_heater_duty,
                measured_vin_mv: latest_vin_mv,
            })
            .await;
            if calibration_runtime_state.mode != CalibrationMode::Off
                && calibration_runtime_state.heater_enabled
                && current_rtd_fault.is_none()
                && heater_controller.fault_latched().is_some()
            {
                heater_controller.clear_fault_latch();
                info!("calibration heater re-arm -> cleared latched fault");
            }
            let mut desired_heater_enabled = reconcile_runtime_heater_enabled(
                ui_state.heater_enabled,
                calibration_runtime_state,
                current_rtd_fault,
                cooling_disabled_lock_latched,
                heater_controller.fault_latched().is_some(),
                thermal_model_heater_allowed(
                    &memory_config,
                    calibration_runtime_state,
                    manual_pps_state,
                ),
                pd_contract_ready,
            );
            desired_heater_enabled = consume_thermal_plant_completion_disarm(
                &mut calibration_runtime_state,
                desired_heater_enabled,
            );
            if ui_state.persistence_locked() {
                desired_heater_enabled = false;
            }
            if ui_state.heater_enabled != desired_heater_enabled {
                ui_state.heater_enabled = desired_heater_enabled;
                needs_redraw = true;
            }
            if force_thermal_plant_output_off {
                ui_state.heater_enabled = false;
            }
            let runtime_plant = thermal_plant_projection_for_runtime(&memory_config);
            // The controller works in heater watts, not source capability
            // watts. Bound achievable plate power by both V^2/R(T) and the
            // selected APDO's V*I contract without turning R(T) into a voltage
            // ceiling. The source contract owns its current boundary.
            let runtime_source_limits = manual_pps_state.heater_source_limits();
            let max_power_mw = heater_available_power_mw_for_temp(
                latest_temp_c,
                runtime_source_limits.map(|(_, max_mv, _)| max_mv),
                runtime_source_limits.map(|(_, _, max_ma)| max_ma),
                preview_heater_curve_config(preview_heater_curve.as_ref()),
                &memory_config,
            );
            let thermal_plant_calibration_running = calibration_runtime_state.mode
                == CalibrationMode::ThermalPlant
                && calibration_runtime_state.job.status == CalibrationJobStatus::Running;
            let pid_snapshot = if thermal_plant_calibration_running {
                thermal_plant_calibration_snapshot(
                    latest_temp_c,
                    calibration_runtime_state.heater_enabled,
                )
            } else if calibration_runtime_state.mode == CalibrationMode::Off {
                if let Some((model, ambient_temp_c)) = runtime_plant {
                    heater_controller.update_thermal_plant_at(ThermalPlantRuntimeInput {
                        target_temp_c: ui_state.target_temp_c,
                        measured_temp_c: latest_temp_c,
                        ambient_temp_c,
                        heater_enabled: ui_state.heater_enabled,
                        model,
                        max_power_mw: max_power_mw as f32,
                        now_ms: elapsed_ms,
                    })
                } else {
                    heater_controller.update_at(
                        ui_state.target_temp_c,
                        latest_temp_c,
                        false,
                        None,
                        elapsed_ms,
                    )
                }
            } else {
                heater_controller.update_at(
                    calibration_runtime_state
                        .model_target_temp_c
                        .unwrap_or(ui_state.target_temp_c),
                    latest_temp_c,
                    ui_state.heater_enabled,
                    None,
                    elapsed_ms,
                )
            };
            last_pid_snapshot = pid_snapshot;
            let requested_duty_percent = if force_thermal_plant_output_off {
                0
            } else {
                pid_snapshot.duty_percent
            };
            if ui_state.heater_output_percent != requested_duty_percent {
                ui_state.heater_output_percent = requested_duty_percent;
                needs_redraw = true;
            }
            if apply_heater_power_output(HeaterPowerOutputContext {
                i2c: &mut pd_i2c,
                pd_port: &mut pd_port,
                heater_pwm: &mut heater_pwm,
                backend: &mut heater_power_backend,
                hold_pps_governor: &mut hold_pps_governor,
                manual_pps: &mut manual_pps_state,
                pd_observation: current_pd_observation,
                measured_heater_mv: latest_vin_mv,
                current_temp_c: latest_temp_c,
                duty_percent: requested_duty_percent,
                heater_enabled: if force_thermal_plant_output_off {
                    false
                } else if thermal_plant_calibration_running {
                    calibration_runtime_state.heater_enabled
                } else {
                    ui_state.heater_enabled
                },
                control_phase: pid_snapshot.phase,
                control_error_c: pid_snapshot.error_c,
                filtered_slope_c_per_s: pid_snapshot.filtered_slope_c_per_s,
                warmup_soft_start_percent: pid_snapshot.warmup_soft_start_percent,
                last_physical_duty_percent: &mut last_heater_duty,
                preview_heater_curve: preview_heater_curve_config(preview_heater_curve.as_ref()),
                memory_config: &memory_config,
                active_thermal_settings,
                now_ms: elapsed_ms,
            })
            .await
            {
                needs_redraw = true;
            }
            heater_control_timing.cycle_ms = Instant::now()
                .as_millis()
                .saturating_sub(runtime_started_ms)
                .saturating_sub(control_started_ms)
                .min(u64::from(u16::MAX)) as u16;
            if ui_state.manual_pps_enabled != manual_pps_state.enabled {
                ui_state.manual_pps_enabled = manual_pps_state.enabled;
                needs_redraw = true;
            }
            let next_pd_contract_mv = effective_pd_contract_mv(
                &manual_pps_state,
                current_pd_observation,
                heater_power_backend,
            );
            if ui_state.pd_contract_mv != next_pd_contract_mv {
                ui_state.pd_contract_mv = next_pd_contract_mv;
                needs_redraw = true;
            }

            info!(
                "heater loop set_c={=i16} temp_c={=f32} control={=u8}% physical={=u8}% pd_mv={=u16} backend={=str} mos_gate={=u8}% error_c={=f32} control_error_c={=f32} temp_avg_c={=f32} phase={=str} arm={=bool} fault={=str}",
                ui_state.target_temp_c,
                latest_temp_c,
                requested_duty_percent,
                requested_duty_percent,
                next_pd_contract_mv,
                heater_power_backend.label(),
                last_heater_duty,
                pid_snapshot.error_c,
                pid_snapshot.control_error_c,
                pid_snapshot.filtered_temp_c,
                pid_snapshot.phase.label(),
                ui_state.heater_enabled,
                heater_controller
                    .fault_latched()
                    .map(|reason| reason.label())
                    .unwrap_or("none"),
            );
        }

        discard_deferred_memory_commit_for_incompatible_eeprom(
            ui_state.persistence_locked(),
            &mut memory_commit_due_ms,
        );
        #[cfg(feature = "web_serial")]
        let eeprom_snapshot_active = eeprom_snapshot_session.active;
        #[cfg(not(feature = "web_serial"))]
        let eeprom_snapshot_active = false;
        if !eeprom_snapshot_active
            && memory_commit_due_ms.is_some_and(|due_ms| elapsed_ms >= due_ms)
        {
            memory_commit_due_ms = None;
            let commit_domains =
                persist_domain_mask_between(&memory_config, &last_persisted_memory_config);
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            if let Err(error) = commit_memory_config_now(
                &mut pd_i2c,
                &mut pd_port,
                &mut eeprom_pd_service,
                CommitMemoryConfigInput {
                    memory_sequence: &mut memory_sequence,
                    memory_config: &memory_config,
                    domains_to_write: commit_domains,
                    #[cfg(feature = "web_serial")]
                    persistence_log_sink: &mut usb_serial,
                    #[cfg(not(feature = "web_serial"))]
                    persistence_log_sink: &mut persistence_log_sink,
                    record_staging: eeprom_record_staging,
                },
            )
            .await
            {
                let requires_heater_lock = memory_failure_requires_heater_lock(error);
                let fault = persistence_fault_from_commit(error);
                if requires_heater_lock {
                    restore_persisted_memory_domains(
                        &mut memory_config,
                        &mut ui_state,
                        &last_persisted_memory_config,
                        commit_domains,
                    );
                    mark_eeprom_required(
                        &mut ui_state,
                        &mut calibration_runtime_state,
                        &mut manual_pps_state,
                        &mut memory_commit_due_ms,
                        Some(fault),
                    );
                } else {
                    ui_state.persistence_fault = Some(fault);
                    ui_state.persistence_fault_attention_pending = true;
                }
            } else {
                copy_persisted_domains(
                    &mut last_persisted_memory_config,
                    &memory_config,
                    commit_domains,
                );
            }
        }

        if ui_state.eeprom_required && !eeprom_required {
            eeprom_required = true;
            persistence_source = "none";
            persistence_record_state = "unavailable";
        } else if ui_state.eeprom_data_incompatible && persistence_record_state == "valid" {
            persistence_record_state = "incompatible";
        }

        let (
            next_cooling_disabled_lock_latched,
            next_cooling_disabled_lock_armed,
            lock_just_latched,
        ) = reconcile_cooling_disabled_lock(
            ui_state.active_cooling_enabled,
            latest_display_temp_i16,
            is_sensor_fault(current_rtd_fault),
            cooling_disabled_lock_latched,
            cooling_disabled_lock_armed,
        );
        if cooling_disabled_lock_latched != next_cooling_disabled_lock_latched
            || cooling_disabled_lock_armed != next_cooling_disabled_lock_armed
        {
            cooling_disabled_lock_latched = next_cooling_disabled_lock_latched;
            cooling_disabled_lock_armed = next_cooling_disabled_lock_armed;
            needs_redraw = true;
        }
        if lock_just_latched {
            if ui_state.heater_enabled {
                ui_state.heater_enabled = false;
            }
            info!(
                "cooling-disabled safety lock latched temp_c={=i16}",
                latest_display_temp_i16
            );
        }

        if !ui_state.heater_enabled
            && (last_heater_duty != 0 || ui_state.heater_output_percent != 0)
        {
            ui_state.heater_output_percent = 0;
            let _ = apply_heater_power_output(HeaterPowerOutputContext {
                i2c: &mut pd_i2c,
                pd_port: &mut pd_port,
                heater_pwm: &mut heater_pwm,
                backend: &mut heater_power_backend,
                hold_pps_governor: &mut hold_pps_governor,
                manual_pps: &mut manual_pps_state,
                pd_observation: last_pd_observation,
                measured_heater_mv: latest_vin_mv,
                current_temp_c: latest_temp_c,
                duty_percent: 0,
                heater_enabled: false,
                control_phase: HeaterControlPhase::Warmup,
                control_error_c: -1.0,
                filtered_slope_c_per_s: -1.0,
                warmup_soft_start_percent: 0,
                last_physical_duty_percent: &mut last_heater_duty,
                preview_heater_curve: preview_heater_curve_config(preview_heater_curve.as_ref()),
                memory_config: &memory_config,
                active_thermal_settings,
                now_ms: elapsed_ms,
            })
            .await;
            let next_pd_contract_mv = effective_pd_contract_mv(
                &manual_pps_state,
                last_pd_observation,
                heater_power_backend,
            );
            if ui_state.pd_contract_mv != next_pd_contract_mv {
                ui_state.pd_contract_mv = next_pd_contract_mv;
            }
            needs_redraw = true;
        }

        let mut fan_decision = fan_policy_decision_with_modes(
            latest_display_temp_i16,
            elapsed_ms,
            ui_state.heater_enabled,
            heater_enabled_last_cycle && !ui_state.heater_enabled,
            ui_state.post_heat_cooling_mode,
            ui_state.heating_fan_guard_mode,
            (fan_policy_state, is_sensor_fault(current_rtd_fault)),
        );
        if calibration_runtime_state.mode == CalibrationMode::ThermalPlant
            && calibration_runtime_state.job.status == CalibrationJobStatus::Running
        {
            fan_decision = FanPolicyDecision {
                state: FanPolicyState::Disabled,
                command: FanHardwareCommand::disabled(),
                display_state: FanDisplayState::Off,
                source: FanPolicySource::Idle,
                output_level: FanOutputLevel::Off,
            };
        }
        if let Some(state) =
            overtemp_forced_fan_state(latest_display_temp_i16, overtemp_forced_fan_active)
        {
            let command = state.command(elapsed_ms);
            fan_decision = FanPolicyDecision {
                state,
                command,
                display_state: fan_display_state_for_policy(
                    FanPolicySource::Safety,
                    ui_state.post_heat_cooling_mode,
                    command,
                ),
                source: FanPolicySource::Safety,
                output_level: fan_output_level_for_command(command),
            };
        }
        fan_policy_state = fan_decision.state;
        fan_command = fan_decision.command;
        heater_enabled_last_cycle = ui_state.heater_enabled;
        apply_fan_output(
            &mut fan_enable,
            &mut fan_pwm,
            fan_command,
            &mut last_fan_command,
        );

        let persistence_locked = ui_state.persistence_locked();
        if sync_frontpanel_runtime_state(
            &mut ui_state,
            fan_decision,
            next_heater_lock_reason_with_persistence(
                persistence_locked,
                heater_controller.fault_latched(),
                cooling_disabled_lock_latched,
                thermal_model_heater_allowed(
                    &memory_config,
                    calibration_runtime_state,
                    manual_pps_state,
                ),
                pd_contract_ready,
            ),
            elapsed_ms,
        ) {
            needs_redraw = true;
        }

        #[cfg(feature = "net_http")]
        let runtime_network_summary = {
            let mut eeprom_pd_service = EepromPdServiceContext::new(
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            let mut pd_network_service = PdNetworkServiceContext {
                eeprom: &mut eeprom_pd_service,
                pd_contract_ready: &mut pd_contract_ready,
                ui_state: &mut ui_state,
                calibration_runtime_state: &mut calibration_runtime_state,
                manual_pps: &mut manual_pps_state,
            };
            run_network_operation_with_pd(
                flux_purr_firmware::net::lan_network_summary(),
                &mut pd_i2c,
                &mut pd_port,
                &mut pd_network_service,
            )
            .await
        };
        #[cfg(feature = "net_http")]
        if ui_state.apply_network_summary(runtime_network_summary) {
            needs_redraw = true;
        }
        if maybe_play_protection_alarm(
            is_overtemp_fault(current_rtd_fault),
            &mut protection_alarm,
            &mut buzzer,
            elapsed_ms,
        ) {
            info!("protection alarm -> replay");
        }

        if maybe_play_attention_reminder(
            attention_pending_after_fault_clear,
            is_overtemp_fault(current_rtd_fault),
            &mut next_attention_reminder_ms,
            &mut buzzer,
            elapsed_ms,
        ) {
            info!("fault attention reminder -> chirp");
        }

        let status_light_elapsed_ms = Instant::now()
            .as_millis()
            .saturating_sub(status_light_started_ms);
        let status_light_state = select_status_light_state(StatusLightInputs {
            booting: status_light_elapsed_ms < STATUS_LIGHT_BOOT_DURATION_MS,
            thermal_runaway: is_overtemp_fault(current_rtd_fault),
            thermal_runaway_attention_pending: attention_pending_after_fault_clear,
            sensor_fault: is_sensor_fault(current_rtd_fault),
            cooling_disabled_overtemp: cooling_disabled_lock_latched,
            heater_interlocked: matches!(
                ui_state.heater_lock_reason,
                Some(
                    HeaterLockReason::PdContractUnavailable
                        | HeaterLockReason::ThermalModelMissingForSourceClass
                )
            ),
            calibration_active: calibration_runtime_state.mode != CalibrationMode::Off,
            heater_enabled: ui_state.heater_enabled,
            fan_enabled: fan_command.enabled,
        });
        set_status_light_state(status_light_state);
        ui_refresh_pending |= needs_redraw;
        if ui_refresh_pending && elapsed_ms >= next_ui_refresh_ms {
            let display_flush_result = run_display_operation_with_pd_and_heater(
                flush_ui(&mut display, canvas, &ui_state),
                &mut pd_i2c,
                &mut pd_port,
                &mut last_pd_observation,
                &mut heater_pwm,
                &mut last_heater_duty,
            )
            .await;
            let pd_redraw_after_flush = apply_pd_contract_observation(
                last_pd_observation,
                &mut pd_contract_ready,
                &mut ui_state,
                &mut calibration_runtime_state,
                &mut manual_pps_state,
                &mut heater_pwm,
                &mut last_heater_duty,
            );
            ui_refresh_pending = pd_redraw_after_flush;
            match display_flush_result {
                Some(Ok(())) => log_ui_state(&ui_state),
                Some(Err(_)) | None => {
                    // A failed or timed-out async SPI transaction must not be
                    // reused: its chip-select and panel state can be incomplete
                    // after an interrupted transfer. Stop heat first, force the
                    // source back toward fixed PD, keep cooling active, then
                    // remain in the USB-readable terminal recovery path.
                    warn!("frontpanel UI refresh failed; entering recovery");
                    manual_pps_state.clear();
                    calibration_runtime_state.heater_enabled = false;
                    calibration_runtime_state.mode = CalibrationMode::Off;
                    calibration_runtime_state.immediate_heater_disarm_pending = true;
                    ui_state.heater_enabled = false;
                    ui_state.heater_output_percent = 0;
                    apply_heater_duty(&mut heater_pwm, 0, &mut last_heater_duty);
                    let _ = disarm_pending_thermal_plant_output(ThermalPlantDisarmContext {
                        calibration_runtime_state: &mut calibration_runtime_state,
                        backend: &mut heater_power_backend,
                        manual_pps: &mut manual_pps_state,
                        i2c: &mut pd_i2c,
                        pd_port: &mut pd_port,
                        heater_pwm: &mut heater_pwm,
                        hold_pps_governor: &mut hold_pps_governor,
                        ui_state: &mut ui_state,
                        last_heater_duty: &mut last_heater_duty,
                        measured_vin_mv: latest_vin_mv,
                    })
                    .await;
                    apply_fan_output(
                        &mut fan_enable,
                        &mut fan_pwm,
                        FanHardwareCommand::from_profile(FanVoltageProfile::Full),
                        &mut last_fan_command,
                    );
                    #[cfg(feature = "web_serial")]
                    run_usb_recovery_control_loop(
                        &mut usb_serial,
                        &mut usb_rx_line,
                        usb_tx_buf,
                        &memory_config,
                        StatusLightState::HeaterInterlocked,
                        UsbRecoveryPhase::RuntimeFault,
                    )
                    .await;

                    #[cfg(not(feature = "web_serial"))]
                    panic!("frontpanel UI refresh timed out");
                }
            }
            next_ui_refresh_ms = elapsed_ms.saturating_add(DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS);
        }
    }
}

#[cfg(not(target_arch = "xtensa"))]
pub fn host_main() {
    println!(
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3,web_serial,net_http"
    );
}
