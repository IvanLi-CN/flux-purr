#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
use core::mem::MaybeUninit;
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Timer};
#[cfg(target_arch = "xtensa")]
use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
#[cfg(target_arch = "xtensa")]
use embedded_hal::pwm::SetDutyCycle;
#[cfg(target_arch = "xtensa")]
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
#[cfg(target_arch = "xtensa")]
use esp_hal::{
    Blocking, Config,
    analog::adc::{Adc, AdcConfig, AdcPin, Attenuation},
    gpio::{AnalogPin, Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    mcpwm::{
        McPwm, PeripheralClockConfig,
        operator::{PwmPin, PwmPinConfig},
        timer::PwmWorkingMode,
    },
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
    usb_serial_jtag::UsbSerialJtag,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    FAN_PHASE_DURATION_SECS, FAN_PWM_FREQUENCY_HZ, FanCommand, FanPhase, pwm_percent_from_permille,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    control_plane::{FirmwareKind, Identity},
    display::{
        DISPLAY_PANEL_CONFIG, DISPLAY_PREVIEW_SEQUENCE, DisplayCanvas, DisplayThemeId,
        render_scene_with_theme,
    },
    frontpanel::{
        preview::SEQUENCE as FRONTPANEL_PREVIEW_SEQUENCE,
        render::{
            DashboardThemeId, TemperaturePaletteId, render_frontpanel_ui_with_theme,
            temperature_palette,
        },
    },
    ram_bringup::{RamBringupCommand, RamBringupTheme, supported_capabilities},
    status_light::{RgbChannels, STATUS_LIGHT_PREVIEW_SEQUENCE, status_light_output},
};
#[cfg(target_arch = "xtensa")]
use gc9d01::{GC9D01, Timer as Gc9d01Timer};
#[cfg(target_arch = "xtensa")]
use heapless::String;
#[cfg(target_arch = "xtensa")]
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "xtensa")]
const RESPONSE_CAPACITY: usize = 4096;
#[cfg(target_arch = "xtensa")]
const MCPWM_PERIPHERAL_CLOCK_HZ: u32 = 40_000_000;
#[cfg(target_arch = "xtensa")]
const FAN_PWM_PERIOD_TICKS: u16 = 99;

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // Reset immediately so the GPIO peripheral returns to its hardware-safe
    // defaults instead of holding a test output indefinitely.
    esp_hal::system::software_reset()
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomingFrame {
    #[serde(rename = "type")]
    frame_type: Option<String<16>>,
    request_id: Option<String<48>>,
    op: Option<String<24>>,
    command: Option<RamBringupCommand>,
    theme: Option<RamBringupTheme>,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Response<'a> {
    #[serde(rename = "type")]
    frame_type: &'static str,
    request_id: &'a String<48>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ResponseResult<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResponseResult<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<&'a Identity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<CommandResult>,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandResult {
    command: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_mask: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vin_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtd_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    i2c_device_id: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    i2c_revision: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fan_pwm_permille: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fan_duration_ms: Option<u32>,
}

#[cfg(target_arch = "xtensa")]
impl CommandResult {
    fn completed(command: RamBringupCommand) -> Self {
        Self {
            command: command.as_str(),
            status: "complete",
            ..Self::default()
        }
    }
}

#[cfg(target_arch = "xtensa")]
struct DisplayTimer;

#[cfg(target_arch = "xtensa")]
impl Gc9d01Timer for DisplayTimer {
    fn after_millis(milliseconds: u64) -> impl core::future::Future<Output = ()> {
        Timer::after_millis(milliseconds)
    }
}

#[cfg(target_arch = "xtensa")]
type DisplayBus = ExclusiveDevice<Spi<'static, esp_hal::Async>, Output<'static>, NoDelay>;
#[cfg(target_arch = "xtensa")]
type DisplayDriver = GC9D01<'static, DisplayBus, Output<'static>, Output<'static>, DisplayTimer>;
#[cfg(target_arch = "xtensa")]
type AdcDriver = Adc<'static, esp_hal::peripherals::ADC1<'static>, Blocking>;
#[cfg(target_arch = "xtensa")]
type VinAdcPin = AdcPin<esp_hal::peripherals::GPIO1<'static>, esp_hal::peripherals::ADC1<'static>>;
#[cfg(target_arch = "xtensa")]
type RtdAdcPin = AdcPin<esp_hal::peripherals::GPIO2<'static>, esp_hal::peripherals::ADC1<'static>>;
#[cfg(target_arch = "xtensa")]
type I2cBus = I2c<'static, Blocking>;
#[cfg(target_arch = "xtensa")]
type FanPwm = PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 0, true>;

#[cfg(target_arch = "xtensa")]
struct BringupState {
    identity: Identity,
    response: [u8; RESPONSE_CAPACITY],
    line: String<8192>,
    center: Input<'static>,
    right: Input<'static>,
    down: Input<'static>,
    left: Input<'static>,
    up: Input<'static>,
    heater_pwm: Output<'static>,
    fan_en: Output<'static>,
    fan_pwm: FanPwm,
    buzzer: Output<'static>,
    rgb_r: Output<'static>,
    rgb_g: Output<'static>,
    rgb_b: Output<'static>,
    backlight: Output<'static>,
    display: DisplayDriver,
    canvas: &'static mut DisplayCanvas,
    adc: AdcDriver,
    vin: VinAdcPin,
    rtd: RtdAdcPin,
    i2c: I2cBus,
}

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".dram2_uninit")]
static mut DISPLAY_CANVAS_STORAGE: MaybeUninit<DisplayCanvas> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".dram2_uninit")]
static mut DISPLAY_DRIVER_FRAMEBUFFER: MaybeUninit<
    [Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS],
> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
fn initialize_canvas() -> &'static mut DisplayCanvas {
    unsafe {
        let canvas = core::ptr::addr_of_mut!(DISPLAY_CANVAS_STORAGE).cast::<DisplayCanvas>();
        DisplayCanvas::initialize_black_in_place(canvas);
        &mut *canvas
    }
}

#[cfg(target_arch = "xtensa")]
fn initialize_driver_framebuffer()
-> &'static mut [Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS] {
    unsafe {
        (&mut *core::ptr::addr_of_mut!(DISPLAY_DRIVER_FRAMEBUFFER))
            .write([Rgb565::BLACK; flux_purr_firmware::display::DISPLAY_PIXELS])
    }
}

#[cfg(target_arch = "xtensa")]
#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    let peripherals = esp_hal::init(Config::default());
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let mut identity = Identity::firmware_from_mac(esp_hal::efuse::Efuse::mac_address());
    identity.firmware_kind = FirmwareKind::RamBringup;
    identity.capabilities.clear();
    let mut identity_capability = String::new();
    let _ = identity_capability.push_str("identity");
    let _ = identity.capabilities.push(identity_capability);
    for capability in supported_capabilities() {
        let _ = identity.capabilities.push(capability);
    }

    let input = InputConfig::default().with_pull(Pull::Up);
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
    let spi_device =
        ExclusiveDevice::new_no_delay(spi.into_async(), cs).expect("failed to wrap SPI2");
    let mut display = GC9D01::new(
        DISPLAY_PANEL_CONFIG,
        spi_device,
        dc,
        rst,
        initialize_driver_framebuffer(),
    );
    display.init().await.expect("failed to initialize GC9D01");

    let mut adc_config = AdcConfig::new();
    let vin = adc_config.enable_pin(peripherals.GPIO1, Attenuation::_11dB);
    let rtd = adc_config.enable_pin(peripherals.GPIO2, Attenuation::_11dB);
    let adc = Adc::new(peripherals.ADC1, adc_config);
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(100_000)),
    )
    .expect("failed to create I2C0")
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);

    let pwm_clock = PeripheralClockConfig::with_frequency(Rate::from_hz(MCPWM_PERIPHERAL_CLOCK_HZ))
        .expect("failed to derive MCPWM peripheral clock");
    let mut mcpwm = McPwm::new(peripherals.MCPWM0, pwm_clock);
    mcpwm.operator0.set_timer(&mcpwm.timer0);
    let fan_timer = pwm_clock
        .timer_clock_with_frequency(
            FAN_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive fan PWM timer clock");
    mcpwm.timer0.start(fan_timer);
    let mut fan_pwm = mcpwm
        .operator0
        .with_pin_a(peripherals.GPIO36, PwmPinConfig::UP_ACTIVE_HIGH);
    let _ = fan_pwm.set_duty_cycle_percent(0);

    let mut state = BringupState {
        identity,
        response: [0; RESPONSE_CAPACITY],
        line: String::new(),
        center: Input::new(peripherals.GPIO0, input),
        right: Input::new(peripherals.GPIO16, input),
        down: Input::new(peripherals.GPIO17, input),
        left: Input::new(peripherals.GPIO18, input),
        up: Input::new(peripherals.GPIO21, input),
        heater_pwm: Output::new(peripherals.GPIO47, Level::Low, OutputConfig::default()),
        fan_en: Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default()),
        fan_pwm,
        buzzer: Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default()),
        rgb_r: Output::new(peripherals.GPIO39, Level::High, OutputConfig::default()),
        rgb_g: Output::new(peripherals.GPIO38, Level::High, OutputConfig::default()),
        rgb_b: Output::new(peripherals.GPIO37, Level::High, OutputConfig::default()),
        backlight: Output::new(peripherals.GPIO13, Level::High, OutputConfig::default()),
        display,
        canvas: initialize_canvas(),
        adc,
        vin,
        rtd,
        i2c,
    };
    let mut usb = UsbSerialJtag::<Blocking>::new(peripherals.USB_DEVICE);

    loop {
        while let Ok(byte) = usb.read_byte() {
            if byte == b'\n' {
                handle_line(&mut state, &mut usb).await;
                state.line.clear();
            } else if state.line.len() < state.line.capacity() {
                let _ = state.line.push(byte as char);
            } else {
                state.line.clear();
            }
        }
        Timer::after(Duration::from_millis(1)).await;
    }
}

#[cfg(target_arch = "xtensa")]
async fn handle_line(state: &mut BringupState, usb: &mut UsbSerialJtag<'static, Blocking>) {
    let Ok((frame, _)) = serde_json_core::from_slice::<IncomingFrame>(state.line.as_bytes()) else {
        return;
    };
    let Some(request_id) = frame.request_id else {
        return;
    };

    if frame.frame_type.as_deref() == Some("request") && frame.op.as_deref() == Some("get_identity")
    {
        let identity = state.identity.clone();
        write_response(
            state,
            usb,
            Response {
                frame_type: "response",
                request_id: &request_id,
                ok: true,
                result: Some(ResponseResult {
                    identity: Some(&identity),
                    command: None,
                }),
                error: None,
            },
        );
        return;
    }

    let Some(command) = frame
        .command
        .filter(|_| frame.frame_type.as_deref() == Some("ram_bringup"))
    else {
        write_response(
            state,
            usb,
            Response {
                frame_type: "response",
                request_id: &request_id,
                ok: false,
                result: None,
                error: Some("unsupported_frame"),
            },
        );
        return;
    };
    if !state
        .identity
        .capabilities
        .iter()
        .any(|value| value == command.capability())
    {
        write_response(
            state,
            usb,
            Response {
                frame_type: "response",
                request_id: &request_id,
                ok: false,
                result: None,
                error: Some("unsupported_command"),
            },
        );
        return;
    }

    let outcome = execute_command(state, command, frame.theme).await;
    let (ok, result, error) = match outcome {
        Ok(command_result) => (true, Some(command_result), None),
        Err(error) => (false, None, Some(error)),
    };
    write_response(
        state,
        usb,
        Response {
            frame_type: "response",
            request_id: &request_id,
            ok,
            result: result.map(|command| ResponseResult {
                identity: None,
                command: Some(command),
            }),
            error,
        },
    );
    safe_outputs(state);
}

#[cfg(target_arch = "xtensa")]
fn write_response(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
    response: Response<'_>,
) {
    if let Ok(length) = serde_json_core::to_slice(&response, &mut state.response) {
        let _ = usb.write(&state.response[..length]);
        let _ = usb.write(b"\n");
    }
}

#[cfg(target_arch = "xtensa")]
fn safe_outputs(state: &mut BringupState) {
    state.heater_pwm.set_low();
    let stop = FanCommand::from_phase(FanPhase::Stop);
    let _ = state
        .fan_pwm
        .set_duty_cycle_percent(pwm_percent_from_permille(stop.pwm_permille));
    state.fan_en.set_low();
    state.buzzer.set_low();
    state.rgb_r.set_high();
    state.rgb_g.set_high();
    state.rgb_b.set_high();
    state.backlight.set_high();
}

#[cfg(target_arch = "xtensa")]
async fn execute_command(
    state: &mut BringupState,
    command: RamBringupCommand,
    theme: Option<RamBringupTheme>,
) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    let mut result = CommandResult::completed(command);
    let theme = theme.unwrap_or(RamBringupTheme::Light);
    match command {
        RamBringupCommand::PreviewDisplay => {
            for scene in DISPLAY_PREVIEW_SEQUENCE {
                render_scene_with_theme(scene, state.canvas, display_theme(theme));
                state.display.write_area(
                    0,
                    0,
                    DISPLAY_PANEL_CONFIG.width,
                    DISPLAY_PANEL_CONFIG.height,
                    state.canvas.pixels(),
                );
                state.display.flush().await.map_err(|_| "display_error")?;
                Timer::after_millis(scene.dwell_millis().max(250)).await;
            }
        }
        RamBringupCommand::PreviewFrontpanel => {
            for preview in FRONTPANEL_PREVIEW_SEQUENCE {
                let ui_state = preview.build();
                render_frontpanel_ui_with_theme(
                    state.canvas,
                    &ui_state,
                    dashboard_theme(theme),
                    temperature_palette(TemperaturePaletteId::Current),
                );
                state.display.write_area(
                    0,
                    0,
                    DISPLAY_PANEL_CONFIG.width,
                    DISPLAY_PANEL_CONFIG.height,
                    state.canvas.pixels(),
                );
                state.display.flush().await.map_err(|_| "display_error")?;
                Timer::after_millis(1_500).await;
            }
        }
        RamBringupCommand::PreviewStatusLight => {
            for state_id in STATUS_LIGHT_PREVIEW_SEQUENCE {
                for elapsed_ms in (0..1_400).step_by(140) {
                    apply_rgb(state, status_light_output(state_id, elapsed_ms));
                    Timer::after_millis(140).await;
                }
            }
        }
        RamBringupCommand::TestButtons => result.key_mask = Some(sample_buttons(state)),
        RamBringupCommand::TestAdc => {
            result.vin_mv = Some(read_adc_mv(&mut state.adc, &mut state.vin).ok_or("adc_error")?);
            result.rtd_mv = Some(read_adc_mv(&mut state.adc, &mut state.rtd).ok_or("adc_error")?);
        }
        RamBringupCommand::TestI2c => {
            let mut device_id = [0_u8; 1];
            let mut revision = [0_u8; 1];
            state
                .i2c
                .write_read(0x22, &[0x01], &mut device_id)
                .map_err(|_| "i2c_error")?;
            state
                .i2c
                .write_read(0x22, &[0x09], &mut revision)
                .map_err(|_| "i2c_error")?;
            result.i2c_device_id = Some(device_id[0]);
            result.i2c_revision = Some(revision[0]);
        }
        RamBringupCommand::TestRgb => {
            for color in [
                RgbChannels::RED,
                RgbChannels::new(false, true, false),
                RgbChannels::BLUE,
                RgbChannels::WHITE,
            ] {
                apply_rgb(state, color);
                Timer::after_millis(300).await;
            }
        }
        RamBringupCommand::TestBuzzer => {
            for _ in 0..23 {
                state.buzzer.set_high();
                Timer::after_millis(1).await;
                state.buzzer.set_low();
                Timer::after_millis(1).await;
            }
        }
        RamBringupCommand::TestFan => {
            let profile = FanCommand::from_phase(FanPhase::Mid);
            result.fan_pwm_permille = Some(profile.pwm_permille);
            result.fan_duration_ms = Some(FAN_PHASE_DURATION_SECS * 1_000);
            let _ = state
                .fan_pwm
                .set_duty_cycle_percent(pwm_percent_from_permille(profile.pwm_permille));
            state.fan_en.set_high();
            Timer::after(Duration::from_secs(FAN_PHASE_DURATION_SECS.into())).await;
            let stop = FanCommand::from_phase(FanPhase::Stop);
            if !stop.enabled {
                let _ = state
                    .fan_pwm
                    .set_duty_cycle_percent(pwm_percent_from_permille(stop.pwm_permille));
                state.fan_en.set_low();
            }
        }
    }
    Ok(result)
}

#[cfg(target_arch = "xtensa")]
const fn display_theme(theme: RamBringupTheme) -> DisplayThemeId {
    match theme {
        RamBringupTheme::Light => DisplayThemeId::Light,
        RamBringupTheme::Dark => DisplayThemeId::Dark,
    }
}

#[cfg(target_arch = "xtensa")]
const fn dashboard_theme(theme: RamBringupTheme) -> DashboardThemeId {
    match theme {
        RamBringupTheme::Light => DashboardThemeId::Light,
        RamBringupTheme::Dark => DashboardThemeId::Dark,
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_rgb(state: &mut BringupState, channels: RgbChannels) {
    if channels.red {
        state.rgb_r.set_low();
    } else {
        state.rgb_r.set_high();
    }
    if channels.green {
        state.rgb_g.set_low();
    } else {
        state.rgb_g.set_high();
    }
    if channels.blue {
        state.rgb_b.set_low();
    } else {
        state.rgb_b.set_high();
    }
}

#[cfg(target_arch = "xtensa")]
fn sample_buttons(state: &BringupState) -> u8 {
    (u8::from(state.center.is_low()) << 0)
        | (u8::from(state.right.is_low()) << 1)
        | (u8::from(state.down.is_low()) << 2)
        | (u8::from(state.left.is_low()) << 3)
        | (u8::from(state.up.is_low()) << 4)
}

#[cfg(target_arch = "xtensa")]
fn read_adc_mv<P>(
    adc: &mut AdcDriver,
    pin: &mut AdcPin<P, esp_hal::peripherals::ADC1<'static>>,
) -> Option<u16>
where
    P: esp_hal::analog::adc::AdcChannel + AnalogPin,
{
    loop {
        match adc.read_oneshot(pin) {
            Ok(raw) => return Some((u32::from(raw & 0x0fff) * 1_100 / 4_095) as u16),
            Err(nb::Error::WouldBlock) => continue,
            Err(nb::Error::Other(_)) => return None,
        }
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!("flux-purr-ram-bringup requires xtensa-esp32s3-none-elf");
}
