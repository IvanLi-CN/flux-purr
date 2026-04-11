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
    gpio::{Level, Output, OutputConfig},
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
    FAN_PWM_FREQUENCY_HZ, FanCommand, FanCycleController, FanPhase,
    board::s3_frontpanel,
    display::{
        DEMO_SEQUENCE, DEVICE_BOOT_FLOW, DISPLAY_PANEL_CONFIG, DeviceBootFlow, DisplayCanvas,
        SceneId, render_scene,
    },
    pwm_percent_from_permille,
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
struct DisplayTimer;

#[cfg(target_arch = "xtensa")]
impl Gc9d01Timer for DisplayTimer {
    async fn after_millis(milliseconds: u64) {
        EmbassyTimer::after_millis(milliseconds).await;
    }
}

#[cfg(target_arch = "xtensa")]
fn fan_phase_label(phase: FanPhase) -> &'static str {
    match phase {
        FanPhase::High => "high",
        FanPhase::Low => "low",
        FanPhase::Mid => "mid",
        FanPhase::Stop => "stop",
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_fan_command<PWM>(fan_enable: &mut Output<'_>, fan_pwm: &mut PWM, command: FanCommand)
where
    PWM: SetDutyCycle<Error = core::convert::Infallible>,
{
    let duty_percent = pwm_percent_from_permille(command.pwm_permille);
    let _ = fan_pwm.set_duty_cycle_percent(duty_percent);

    if command.enabled {
        fan_enable.set_high();
    } else {
        fan_enable.set_low();
    }
}

#[cfg(target_arch = "xtensa")]
fn log_fan_command(command: FanCommand, uptime_secs: u32) {
    let duty_percent = pwm_percent_from_permille(command.pwm_permille);
    info!(
        "fan phase={=str} enabled={=bool} pwm_permille={=u16} pwm_percent={=u8} uptime_s={=u32}",
        fan_phase_label(command.phase),
        command.enabled,
        command.pwm_permille,
        duty_percent,
        uptime_secs,
    );
}

#[cfg(target_arch = "xtensa")]
async fn wait_with_fan<PWM>(
    duration_ms: u64,
    elapsed_ms: &mut u64,
    fan_controller: &mut FanCycleController,
    active_command: &mut Option<FanCommand>,
    fan_enable: &mut Output<'_>,
    fan_pwm: &mut PWM,
) where
    PWM: SetDutyCycle<Error = core::convert::Infallible>,
{
    let mut remaining_ms = duration_ms;
    while remaining_ms > 0 {
        let step_ms = remaining_ms.min(200);
        EmbassyTimer::after_millis(step_ms).await;
        *elapsed_ms = elapsed_ms.saturating_add(step_ms);
        remaining_ms -= step_ms;

        let uptime_secs = ((*elapsed_ms) / 1_000).min(u32::MAX as u64) as u32;
        let next = fan_controller.command_at(uptime_secs);
        if active_command.is_none_or(|current| current != next) {
            apply_fan_command(fan_enable, fan_pwm, next);
            log_fan_command(next, uptime_secs);
            *active_command = Some(next);
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn present_scene<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    scene: SceneId,
) -> Result<(), gc9d01::Error<BUS::Error, DC::Error>>
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    render_scene(scene, canvas);
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
async fn flush_scene<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    scene: SceneId,
) -> Result<(), gc9d01::Error<BUS::Error, DC::Error>>
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    present_scene(display, canvas, scene)?;
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
        "boot fan_tach={=u8} fan_en={=u8} fan_pwm={=u8} pwm_hz={=u32}",
        s3_frontpanel::PIN_FAN_TACH,
        s3_frontpanel::PIN_FAN_EN,
        s3_frontpanel::PIN_FAN_PWM,
        FAN_PWM_FREQUENCY_HZ,
    );

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let mut fan_enable = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
    let fan_clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40))
        .expect("failed to derive MCPWM peripheral clock");
    let mut fan_mcpwm = McPwm::new(peripherals.MCPWM0, fan_clock_cfg);
    fan_mcpwm.operator0.set_timer(&fan_mcpwm.timer0);
    let mut fan_pwm = fan_mcpwm
        .operator0
        .with_pin_a(peripherals.GPIO36, PwmPinConfig::UP_ACTIVE_HIGH);
    let fan_timer_cfg = fan_clock_cfg
        .timer_clock_with_frequency(
            99,
            PwmWorkingMode::Increase,
            Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive fan PWM timer clock");
    fan_mcpwm.timer0.start(fan_timer_cfg);
    let mut fan_controller = FanCycleController::new();
    let mut elapsed_ms: u64 = 0;
    let initial_fan_command = fan_controller.command_at(0);
    apply_fan_command(&mut fan_enable, &mut fan_pwm, initial_fan_command);
    log_fan_command(initial_fan_command, 0);
    let mut fan_active_command = Some(initial_fan_command);

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

    flush_scene(&mut display, canvas, SceneId::StartupCalibration)
        .await
        .expect("failed to draw startup calibration screen");
    info!("scene={=str}", SceneId::StartupCalibration.label());

    match DEVICE_BOOT_FLOW {
        DeviceBootFlow::CalibrationOnly => {
            info!("device boot flow: calibration-only");
        }
        DeviceBootFlow::CalibrationThenDemoThenHold => {
            info!("device boot flow: calibration -> demo -> hold");
            wait_with_fan(
                1_200,
                &mut elapsed_ms,
                &mut fan_controller,
                &mut fan_active_command,
                &mut fan_enable,
                &mut fan_pwm,
            )
            .await;
            for scene in DEMO_SEQUENCE {
                flush_scene(&mut display, canvas, scene)
                    .await
                    .expect("failed to draw demo scene");
                info!("scene={=str}", scene.label());
                wait_with_fan(
                    scene.dwell_millis(),
                    &mut elapsed_ms,
                    &mut fan_controller,
                    &mut fan_active_command,
                    &mut fan_enable,
                    &mut fan_pwm,
                )
                .await;
            }
            flush_scene(&mut display, canvas, SceneId::StartupCalibration)
                .await
                .expect("failed to restore startup calibration screen");
            info!("scene={=str}", SceneId::StartupCalibration.label());
        }
    }

    let mut heartbeat_seconds: u32 = 0;
    loop {
        wait_with_fan(
            2_000,
            &mut elapsed_ms,
            &mut fan_controller,
            &mut fan_active_command,
            &mut fan_enable,
            &mut fan_pwm,
        )
        .await;
        heartbeat_seconds = heartbeat_seconds.wrapping_add(2);
        info!(
            "heartbeat startup-screen uptime_s={=u32}",
            heartbeat_seconds
        );
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "esp32s3-fan-cycle is now the GC9D01 async display bring-up binary; build with --target xtensa-esp32s3-none-elf --features esp32s3"
    );
}
