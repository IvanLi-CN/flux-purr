#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
use defmt::info;
#[cfg(target_arch = "xtensa")]
use embassy_executor::Spawner;
#[cfg(target_arch = "xtensa")]
use embassy_time::Timer as EmbassyTimer;
#[cfg(target_arch = "xtensa")]
use embedded_graphics::prelude::RgbColor;
#[cfg(target_arch = "xtensa")]
use embedded_hal::pwm::SetDutyCycle;
#[cfg(target_arch = "xtensa")]
use embedded_hal_bus::spi::ExclusiveDevice;
#[cfg(target_arch = "xtensa")]
use esp_backtrace as _;
#[cfg(target_arch = "xtensa")]
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    mcpwm::{McPwm, PeripheralClockConfig, operator::PwmPinConfig, timer::PwmWorkingMode},
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
#[cfg(target_arch = "xtensa")]
use esp_println as _;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    FAN_PWM_FREQUENCY_HZ,
    board::s3_frontpanel,
    display::{DISPLAY_PANEL_CONFIG, DisplayCanvas, SceneId, render_scene},
    frontpanel::{
        FrontPanelInputController, FrontPanelInputTimings, FrontPanelKeyMap, FrontPanelRawState,
        FrontPanelRoute, FrontPanelRuntimeMode, FrontPanelUiState, RawFrontPanelKey,
        render::render_frontpanel_ui,
    },
};
#[cfg(target_arch = "xtensa")]
use gc9d01::{GC9D01, Timer as Gc9d01Timer};
#[cfg(target_arch = "xtensa")]
use static_cell::StaticCell;

#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_DC as usize] = [(); 10];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_MOSI as usize] = [(); 11];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_SCLK as usize] = [(); 12];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_BLK as usize] = [(); 13];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_RES as usize] = [(); 14];
#[cfg(target_arch = "xtensa")]
const _: [(); s3_frontpanel::PIN_LCD_CS as usize] = [(); 15];
#[cfg(target_arch = "xtensa")]
const HEATER_TEST_PWM_FREQUENCY_HZ: u32 = 1_000;
#[cfg(target_arch = "xtensa")]
const HEATER_TEST_PWM_DUTY_PERCENT: u8 = 50;

#[cfg(target_arch = "xtensa")]
struct DisplayTimer;

#[cfg(target_arch = "xtensa")]
impl Gc9d01Timer for DisplayTimer {
    async fn after_millis(milliseconds: u64) {
        EmbassyTimer::after_millis(milliseconds).await;
    }
}

#[cfg(target_arch = "xtensa")]
struct FrontPanelInputs<'d> {
    center: Input<'d>,
    right: Input<'d>,
    down: Input<'d>,
    left: Input<'d>,
    up: Input<'d>,
}

#[cfg(target_arch = "xtensa")]
impl<'d> FrontPanelInputs<'d> {
    fn sample(&self) -> FrontPanelRawState {
        let mut state = FrontPanelRawState::default();
        state.set_pressed(RawFrontPanelKey::CenterBoot, self.center.is_low());
        state.set_pressed(RawFrontPanelKey::Right, self.right.is_low());
        state.set_pressed(RawFrontPanelKey::Down, self.down.is_low());
        state.set_pressed(RawFrontPanelKey::Left, self.left.is_low());
        state.set_pressed(RawFrontPanelKey::Up, self.up.is_low());
        state
    }
}

#[cfg(target_arch = "xtensa")]
fn runtime_mode_label(mode: FrontPanelRuntimeMode) -> &'static str {
    match mode {
        FrontPanelRuntimeMode::KeyTest => "key-test",
        FrontPanelRuntimeMode::App => "app",
    }
}

#[cfg(target_arch = "xtensa")]
fn route_label(route: FrontPanelRoute) -> &'static str {
    match route {
        FrontPanelRoute::KeyTest => "key-test",
        FrontPanelRoute::Dashboard => "dashboard",
        FrontPanelRoute::Menu => "menu",
        FrontPanelRoute::PresetTemp => "preset-temp",
        FrontPanelRoute::ActiveCooling => "active-cooling",
        FrontPanelRoute::WifiInfo => "wifi-info",
        FrontPanelRoute::DeviceInfo => "device-info",
    }
}

#[cfg(target_arch = "xtensa")]
fn log_ui_state(state: &FrontPanelUiState) {
    info!(
        "ui route={=str} target_c={=i16} heater={=bool} fan={=bool}",
        route_label(state.route),
        state.target_temp_c,
        state.heater_enabled,
        state.fan_enabled,
    );
}

#[cfg(target_arch = "xtensa")]
fn present_ui<'a, BUS, DC, RST>(
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
async fn flush_ui<'a, BUS, DC, RST>(
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
    display.flush().await
}

#[cfg(target_arch = "xtensa")]
#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

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

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let mut fan_enable = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
    fan_enable.set_low();
    let pwm_clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40))
        .expect("failed to derive MCPWM peripheral clock");
    let mut mcpwm = McPwm::new(peripherals.MCPWM0, pwm_clock_cfg);

    mcpwm.operator0.set_timer(&mcpwm.timer0);
    let mut fan_pwm = mcpwm
        .operator0
        .with_pin_a(peripherals.GPIO36, PwmPinConfig::UP_ACTIVE_HIGH);
    let fan_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            99,
            PwmWorkingMode::Increase,
            Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive fan PWM timer clock");
    mcpwm.timer0.start(fan_timer_cfg);
    let _ = fan_pwm.set_duty_cycle_percent(0);

    mcpwm.operator1.set_timer(&mcpwm.timer1);
    let mut heater_pwm = mcpwm
        .operator1
        .with_pin_a(peripherals.GPIO47, PwmPinConfig::UP_ACTIVE_HIGH);
    let heater_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            99,
            PwmWorkingMode::Increase,
            Rate::from_hz(HEATER_TEST_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive heater PWM timer clock");
    mcpwm.timer1.start(heater_timer_cfg);
    let _ = heater_pwm.set_duty_cycle_percent(HEATER_TEST_PWM_DUTY_PERCENT);
    info!(
        "heater PWM test active: gpio47 duty={=u8}% freq={=u32}Hz; fan remains safe-off",
        HEATER_TEST_PWM_DUTY_PERCENT, HEATER_TEST_PWM_FREQUENCY_HZ,
    );

    let input_cfg = InputConfig::default().with_pull(Pull::Up);
    let inputs = FrontPanelInputs {
        center: Input::new(peripherals.GPIO0, input_cfg),
        right: Input::new(peripherals.GPIO16, input_cfg),
        down: Input::new(peripherals.GPIO17, input_cfg),
        left: Input::new(peripherals.GPIO18, input_cfg),
        up: Input::new(peripherals.GPIO21, input_cfg),
    };

    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(10_000_000))
            .with_mode(SpiMode::_0),
    )
    .expect("failed to create SPI2")
    .with_sck(peripherals.GPIO12)
    .with_mosi(peripherals.GPIO11)
    .into_async();

    let cs = Output::new(peripherals.GPIO15, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    let mut backlight = Output::new(peripherals.GPIO13, Level::High, OutputConfig::default());
    backlight.set_low();
    info!("backlight active-low: gpio13 low -> on");

    let spi_device = ExclusiveDevice::new_no_delay(spi, cs)
        .expect("failed to wrap async SPI bus as ExclusiveDevice");

    static DRIVER_FB: StaticCell<
        [embedded_graphics::pixelcolor::Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS],
    > = StaticCell::new();
    static CANVAS: StaticCell<DisplayCanvas> = StaticCell::new();

    let driver_framebuffer = DRIVER_FB.init(
        [embedded_graphics::pixelcolor::Rgb565::BLACK; flux_purr_firmware::display::DISPLAY_PIXELS],
    );
    let canvas = CANVAS.init(DisplayCanvas::new());

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
    display
        .init()
        .await
        .expect("failed to initialize GC9D01 display");

    render_scene(SceneId::StartupCalibration, canvas);
    display.write_area(
        0,
        0,
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        canvas.pixels(),
    );
    display
        .flush()
        .await
        .expect("failed to draw startup calibration screen");
    info!("scene={=str}", SceneId::StartupCalibration.label());
    EmbassyTimer::after_millis(900).await;

    let runtime_mode = FrontPanelRuntimeMode::compile_time_default();
    info!(
        "frontpanel runtime mode={=str}",
        runtime_mode_label(runtime_mode)
    );

    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new(runtime_mode);
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    flush_ui(&mut display, canvas, &ui_state)
        .await
        .expect("failed to draw initial frontpanel UI");
    log_ui_state(&ui_state);

    let mut elapsed_ms: u64 = 0;
    loop {
        EmbassyTimer::after_millis(20).await;
        elapsed_ms = elapsed_ms.saturating_add(20);

        let raw_state = inputs.sample();
        let sample = controller.sample(elapsed_ms, raw_state);
        let mut needs_redraw = false;

        if sample.raw_state != last_raw_state {
            ui_state.set_raw_state(sample.raw_state);
            last_raw_state = sample.raw_state;
            info!("raw mask={=u8}", sample.raw_state.pressed_mask());
            if runtime_mode == FrontPanelRuntimeMode::KeyTest {
                needs_redraw = true;
            }
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
            flush_ui(&mut display, canvas, &ui_state)
                .await
                .expect("failed to refresh frontpanel UI");
            log_ui_state(&ui_state);
        }
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "esp32s3-fan-cycle now runs the interactive frontpanel mock runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3[,frontpanel-key-test]"
    );
}
