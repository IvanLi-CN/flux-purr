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
use embedded_hal_bus::spi::ExclusiveDevice;
#[cfg(target_arch = "xtensa")]
use esp_backtrace as _;
#[cfg(target_arch = "xtensa")]
use esp_hal::{
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
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
    board::s3_frontpanel,
    display::{
        DEMO_SEQUENCE, DEVICE_BOOT_FLOW, DISPLAY_PANEL_CONFIG, DeviceBootFlow, DisplayCanvas,
        SceneId, render_scene,
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
struct DisplayTimer;

#[cfg(target_arch = "xtensa")]
impl Gc9d01Timer for DisplayTimer {
    async fn after_millis(milliseconds: u64) {
        EmbassyTimer::after_millis(milliseconds).await;
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

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

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
    let mut backlight = Output::new(peripherals.GPIO13, Level::Low, OutputConfig::default());
    backlight.set_high();

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
            EmbassyTimer::after_millis(1200).await;
            for scene in DEMO_SEQUENCE {
                flush_scene(&mut display, canvas, scene)
                    .await
                    .expect("failed to draw demo scene");
                info!("scene={=str}", scene.label());
                EmbassyTimer::after_millis(scene.dwell_millis()).await;
            }
            flush_scene(&mut display, canvas, SceneId::StartupCalibration)
                .await
                .expect("failed to restore startup calibration screen");
            info!("scene={=str}", SceneId::StartupCalibration.label());
        }
    }

    loop {
        EmbassyTimer::after_millis(1_000).await;
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "esp32s3-fan-cycle is now the GC9D01 async display bring-up binary; build with --target xtensa-esp32s3-none-elf --features esp32s3"
    );
}
