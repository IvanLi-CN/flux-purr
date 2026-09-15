#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) fn present_ui<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    state: &FrontPanelUiState,
) -> Result<(), gc9d01::Error<BUS::Error, DC::Error>>
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    render_frontpanel_ui(canvas, state);
    display.write_area(
        0,
        0,
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        canvas.pixels(),
    );
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn flush_ui<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    state: &FrontPanelUiState,
) -> Result<(), gc9d01::Error<BUS::Error, DC::Error>>
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    present_ui(display, canvas, state)?;
    display.flush().await?;
    info!(
        "frontpanel presentation committed route={=str}",
        route_label(state.route)
    );
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct InitialFrontpanelContext<'a, 'i, PWM> {
    pub(crate) state: &'a FrontPanelUiState,
    pub(crate) i2c: &'a mut I2c<'i, esp_hal::Blocking>,
    pub(crate) pd_port: &'a mut PdPort,
    pub(crate) last_pd_observation: &'a mut Option<PdStatusObservation>,
    pub(crate) heater_pwm: &'a mut PWM,
    pub(crate) last_heater_duty: &'a mut u8,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn present_initial_frontpanel_ui<'a, BUS, DC, RST, PWM>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    context: InitialFrontpanelContext<'_, '_, PWM>,
) -> bool
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
    PWM: SetDutyCycle,
{
    let InitialFrontpanelContext {
        state,
        i2c,
        pd_port,
        last_pd_observation,
        heater_pwm,
        last_heater_duty,
    } = context;
    if !matches!(
        run_display_operation_with_pd_and_heater(
            flush_ui(display, canvas, state),
            i2c,
            pd_port,
            last_pd_observation,
            heater_pwm,
            last_heater_duty,
        )
        .await,
        Some(Ok(()))
    ) {
        warn!("frontpanel runtime presentation failed");
        return false;
    }

    info!(
        "frontpanel startup presentation complete route={=str}",
        route_label(state.route)
    );
    true
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run_display_operation_with_pd<F>(
    operation: F,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    last_pd_observation: &mut Option<PdStatusObservation>,
) -> Option<F::Output>
where
    F: Future,
{
    let mut pinned_operation = core::pin::pin!(operation);
    let started_at = Instant::now();
    loop {
        match select(
            pinned_operation.as_mut(),
            EmbassyTimer::after_millis(PD_RUNTIME_SERVICE_INTERVAL_MS),
        )
        .await
        {
            Either::First(output) => return Some(output),
            Either::Second(_) => {
                *last_pd_observation = read_pd_status(i2c, pd_port, PdTimestamp::now()).await;
                if Instant::now().saturating_duration_since(started_at) >= DISPLAY_IO_TIMEOUT {
                    return None;
                }
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run_display_operation_with_pd_and_heater<F, PWM>(
    operation: F,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    last_pd_observation: &mut Option<PdStatusObservation>,
    heater_pwm: &mut PWM,
    last_heater_duty: &mut u8,
) -> Option<F::Output>
where
    F: Future,
    PWM: SetDutyCycle,
{
    let mut pinned_operation = core::pin::pin!(operation);
    let started_at = Instant::now();
    loop {
        match select(
            pinned_operation.as_mut(),
            EmbassyTimer::after_millis(PD_RUNTIME_SERVICE_INTERVAL_MS),
        )
        .await
        {
            Either::First(output) => return Some(output),
            Either::Second(_) => {
                let observation = read_pd_status(i2c, pd_port, PdTimestamp::now()).await;
                *last_pd_observation = observation;
                if !startup_pd_contract_ready(observation) {
                    // A failed status read is not proof of a detach, but it is
                    // never authorization to keep driving the heater while a
                    // display transfer is still in flight.
                    apply_heater_duty(heater_pwm, 0, last_heater_duty);
                }
                if Instant::now().saturating_duration_since(started_at) >= DISPLAY_IO_TIMEOUT {
                    return None;
                }
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) const DISPLAY_IO_TIMEOUT: Duration = Duration::from_secs(1);

#[cfg(target_arch = "xtensa")]
pub(crate) async fn request_pd_fixed_voltage(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
    request: ch224q::VoltageRequest,
) -> PdContractRequestState {
    match port {
        PdPort::Fusb302b(runtime) => {
            runtime
                .request_fixed_voltage(i2c, request.millivolts(), PdTimestamp::now())
                .await
        }
        PdPort::Unavailable => PdContractRequestState::Failed,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn request_pd_adjustable_voltage(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
    request_mv: u16,
    mode: ch224q::AdjustableVoltageMode,
    mode_changed: bool,
) -> PdContractRequestState {
    match port {
        PdPort::Fusb302b(runtime) => {
            let _ = mode_changed;
            if mode == ch224q::AdjustableVoltageMode::Pps {
                runtime
                    .request_pps_voltage(i2c, request_mv, PdTimestamp::now())
                    .await
            } else {
                PdContractRequestState::Failed
            }
        }
        PdPort::Unavailable => PdContractRequestState::Failed,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn read_pd_status(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
    now: PdTimestamp,
) -> Option<PdStatusObservation> {
    match port {
        PdPort::Fusb302b(runtime) => {
            if !runtime.poll(i2c, now).await {
                return None;
            }
            let contract = runtime.active_contract();
            let status_raw = if contract == Contract::none() {
                0
            } else {
                1 << 3
            };
            Some(PdStatusObservation {
                status_raw,
                status: Status::from_register(status_raw),
                current_raw: 0,
                current_ma: contract.current_ma,
                contract_voltage_mv: (contract != Contract::none()).then_some(contract.voltage_mv),
                contract,
            })
        }
        PdPort::Unavailable => None,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn read_pd_power_capabilities(
    _i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
) -> Option<ch224q::AdjustablePowerCapabilities> {
    match port {
        PdPort::Fusb302b(runtime) => runtime
            .source_capabilities()
            .and_then(fusb302b_adjustable_power_capabilities),
        PdPort::Unavailable => None,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run_key_test_runtime<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    inputs: FrontPanelInputs<'a>,
    status_light_started_ms: u64,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    last_pd_observation: &mut Option<PdStatusObservation>,
) -> Result<(), ()>
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::KeyTest);
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    let initial_flush_result = run_display_operation_with_pd(
        flush_ui(display, canvas, &ui_state),
        i2c,
        pd_port,
        last_pd_observation,
    )
    .await;
    if !matches!(initial_flush_result, Some(Ok(()))) {
        warn!("initial key-test UI failed; entering recovery");
        return Err(());
    }
    log_ui_state(&ui_state);

    let mut elapsed_ms: u64 = 0;
    loop {
        set_status_light_state(
            if Instant::now()
                .as_millis()
                .saturating_sub(status_light_started_ms)
                < STATUS_LIGHT_BOOT_DURATION_MS
            {
                StatusLightState::Booting
            } else {
                StatusLightState::Ready
            },
        );
        for _ in 0..(20 / PD_RUNTIME_SERVICE_INTERVAL_MS) {
            EmbassyTimer::after_millis(PD_RUNTIME_SERVICE_INTERVAL_MS).await;
            *last_pd_observation = read_pd_status(i2c, pd_port, PdTimestamp::now()).await;
        }
        elapsed_ms = elapsed_ms.saturating_add(20);

        let raw_state = inputs.sample();
        let sample = controller.sample_with_capabilities(
            elapsed_ms,
            raw_state,
            ui_state.gesture_capabilities(),
        );
        let mut needs_redraw = false;

        if sample.raw_state != last_raw_state {
            ui_state.set_raw_state(sample.raw_state);
            last_raw_state = sample.raw_state;
            info!("raw mask={=u8}", sample.raw_state.pressed_mask());
            needs_redraw = true;
        }

        for event in sample.events {
            info!(
                "key raw={=str} logical={=str} gesture={=str} at_ms={=u64}",
                event.raw_key.label(),
                event.key.label(),
                event.gesture.label(),
                event.at_ms,
            );
            if ui_state.handle_event(event) {
                needs_redraw = true;
            }
        }

        if needs_redraw {
            let flush_result = run_display_operation_with_pd(
                flush_ui(display, canvas, &ui_state),
                i2c,
                pd_port,
                last_pd_observation,
            )
            .await;
            if !matches!(flush_result, Some(Ok(()))) {
                warn!("key-test UI refresh failed; entering recovery");
                return Err(());
            }
            log_ui_state(&ui_state);
        }
    }
}
