#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]

#[cfg(target_arch = "xtensa")]
extern crate alloc;

#[cfg(target_arch = "xtensa")]
use alloc::boxed::Box;
#[cfg(target_arch = "xtensa")]
use core::mem::MaybeUninit;
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Instant, Timer};
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
    FAN_PHASE_DURATION_SECS, FAN_PWM_FREQUENCY_HZ, pwm_percent_from_permille,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{FanCommand, FanPhase};
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

// The product runtime keeps the adjustable fan rail at its minimum-output
// duty while disabled. RAM bring-up must use the same electrical safe state;
// the legacy FanPhase::Stop value is a status-model value, not this hardware
// shutdown duty.
const PRODUCT_SAFE_FAN_PWM_PERMILLE: u16 = 1_000;

#[cfg(any(target_arch = "xtensa", test))]
const fn fan_stop_pwm_permille() -> u16 {
    PRODUCT_SAFE_FAN_PWM_PERMILLE
}

#[cfg(target_arch = "xtensa")]
unsafe extern "C" {
    static _stack_start_cpu0: u32;
    static _init_start: u32;
    static _bss_start: u32;
    static _bss_end: u32;
    static _data_start: u32;
    static _data_end: u32;
    static _sidata: u32;
    fn __zero_bss() -> bool;
    fn __init_data() -> bool;
    fn __post_init();
    #[link_name = "main"]
    fn ram_main() -> !;
    fn _xtensa_lx_rt_zero_fill(start: *mut u32, end: *mut u32);
    fn _xtensa_lx_rt_copy(src: *const u32, start: *mut u32, end: *mut u32);
}

#[cfg(target_arch = "xtensa")]
core::arch::global_asm!(
    r#"
    .section .text
    .global ram_entry
    .type ram_entry, @function
ram_entry:
    entry a1, 0
    movi a0, 0
    wsr.intenable a0
    l32r a5, ram_sym_stack_start_cpu0
    mov a1, a5

    l32r a5, ram_sym_zero_bss
    callx8 a5
    beqz a10, .Lram_init_data
    l32r a10, ram_sym_bss_start
    l32r a11, ram_sym_bss_end
    l32r a5, ram_sym_zero_fill
    callx8 a5

.Lram_init_data:
    l32r a5, ram_sym_init_data
    callx8 a5
    beqz a10, .Lram_init_data_done
    l32r a10, ram_sym_sidata
    l32r a11, ram_sym_data_start
    l32r a12, ram_sym_data_end
    l32r a5, ram_sym_copy
    callx8 a5

.Lram_init_data_done:
    memw
    wsr.ccompare0 a0
    wsr.ccompare1 a0
    wsr.ccompare2 a0
    isync
    l32r a5, ram_sym_init_start
    wsr.vecbase a5
    l32r a5, ram_sym_post_init
    callx8 a5
    l32r a5, ram_sym_main
    callx8 a5
    j ram_entry

    .literal ram_sym_stack_start_cpu0, {_stack_start_cpu0}
    .literal ram_sym_init_start, {_init_start}
    .literal ram_sym_bss_start, {_bss_start}
    .literal ram_sym_bss_end, {_bss_end}
    .literal ram_sym_data_start, {_data_start}
    .literal ram_sym_data_end, {_data_end}
    .literal ram_sym_sidata, {_sidata}
    .literal ram_sym_zero_bss, {__zero_bss}
    .literal ram_sym_init_data, {__init_data}
    .literal ram_sym_post_init, {__post_init}
    .literal ram_sym_main, {ram_main}
    .literal ram_sym_zero_fill, {_xtensa_lx_rt_zero_fill}
    .literal ram_sym_copy, {_xtensa_lx_rt_copy}
    "#,
    _stack_start_cpu0 = sym _stack_start_cpu0,
    _init_start = sym _init_start,
    _bss_start = sym _bss_start,
    _bss_end = sym _bss_end,
    _data_start = sym _data_start,
    _data_end = sym _data_end,
    _sidata = sym _sidata,
    __zero_bss = sym __zero_bss,
    __init_data = sym __init_data,
    __post_init = sym __post_init,
    ram_main = sym ram_main,
    _xtensa_lx_rt_zero_fill = sym _xtensa_lx_rt_zero_fill,
    _xtensa_lx_rt_copy = sym _xtensa_lx_rt_copy,
);

#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(target_arch = "xtensa")]
const RESPONSE_CAPACITY: usize = 4096;
#[cfg(target_arch = "xtensa")]
const MCPWM_PERIPHERAL_CLOCK_HZ: u32 = 40_000_000;
#[cfg(target_arch = "xtensa")]
const FAN_PWM_PERIOD_TICKS: u16 = 99;
#[cfg(target_arch = "xtensa")]
const RUNTIME_HEAP_SIZE: usize = 32 * 1024;

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
    last_command_request_id: String<48>,
    last_command_response: [u8; RESPONSE_CAPACITY],
    last_command_response_len: usize,
    pending_fan: Option<PendingFanTest>,
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
struct PendingFanTest {
    request_id: String<48>,
    result: CommandResult,
    deadline: Instant,
    next_heartbeat: Instant,
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
#[unsafe(link_section = ".dram2_uninit")]
static mut RUNTIME_HEAP_STORAGE: MaybeUninit<[u8; RUNTIME_HEAP_SIZE]> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
fn initialize_runtime_heap() {
    let heap_ptr = core::ptr::addr_of_mut!(RUNTIME_HEAP_STORAGE).cast::<u8>();
    unsafe {
        heap_ptr.write_bytes(0, RUNTIME_HEAP_SIZE);
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            heap_ptr,
            RUNTIME_HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}

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
    initialize_runtime_heap();
    let mut identity = Identity::firmware_from_mac(esp_hal::efuse::Efuse::mac_address());
    identity.firmware_kind = FirmwareKind::RamBringup;
    identity.capabilities.clear();
    let mut identity_capability = String::new();
    let _ = identity_capability.push_str("identity");
    let _ = identity.capabilities.push(identity_capability);
    for capability in supported_capabilities() {
        let _ = identity.capabilities.push(capability);
    }

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // Bring up the USB endpoint before constructing board drivers. This keeps
    // the RAM identity probe useful even if a later peripheral constructor
    // rejects an unexpected board revision.
    let usb = Box::new(UsbSerialJtag::<Blocking>::new(peripherals.USB_DEVICE));
    let usb = Box::leak(usb);
    service_initial_identity(usb, &identity).await;

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
    let display = GC9D01::new(
        DISPLAY_PANEL_CONFIG,
        spi_device,
        dc,
        rst,
        initialize_driver_framebuffer(),
    );
    // LCD initialization stays command-scoped so a disconnected panel cannot
    // take down the USB session used by electrical bring-up tests.

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
    let _ = fan_pwm.set_duty_cycle_percent(pwm_percent_from_permille(fan_stop_pwm_permille()));

    let state = Box::new(BringupState {
        identity,
        response: [0; RESPONSE_CAPACITY],
        last_command_request_id: String::new(),
        last_command_response: [0; RESPONSE_CAPACITY],
        last_command_response_len: 0,
        pending_fan: None,
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
    });
    let state = Box::leak(state);
    command_loop(state, usb).await;
}

#[cfg(target_arch = "xtensa")]
async fn command_loop(
    state: &'static mut BringupState,
    usb: &'static mut UsbSerialJtag<'static, Blocking>,
) -> ! {
    loop {
        service_pending_fan(state, usb);
        while let Ok(byte) = usb.read_byte() {
            if state.pending_fan.is_some() {
                if byte == b'\n' {
                    state.line.clear();
                }
                continue;
            }
            if byte == b'\n' {
                if !handle_identity_line(state, usb) {
                    if !handle_test_buttons_line(state, usb) {
                        run_command(state, usb).await;
                    }
                }
                state.line.clear();
            } else if state.line.len() < state.line.capacity() {
                let _ = state.line.push(byte as char);
            } else {
                state.line.clear();
            }
        }
        Timer::after_millis(1).await;
    }
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
async fn run_command(state: &mut BringupState, usb: &mut UsbSerialJtag<'static, Blocking>) {
    let Ok((frame, _)) = serde_json_core::from_slice::<IncomingFrame>(state.line.as_bytes()) else {
        return;
    };
    let Some(request_id) = frame.request_id else {
        return;
    };
    if state.last_command_response_len != 0
        && state.last_command_request_id.as_str() == request_id.as_str()
    {
        let length = state.last_command_response_len;
        let _ = usb.write(&state.last_command_response[..length]);
        let _ = usb.write(b"\n");
        return;
    }
    let Some(command) = frame
        .command
        .filter(|_| frame.frame_type.as_deref() == Some("ram_bringup"))
    else {
        return;
    };
    if !state
        .identity
        .capabilities
        .iter()
        .any(|value| value == command.capability())
    {
        write_command_error(state, usb, &request_id, "unsupported_command");
        return;
    }
    if command == RamBringupCommand::TestFan {
        start_fan_test(state, &request_id);
        return;
    }
    if frame.theme.is_some()
        && !matches!(
            command,
            RamBringupCommand::PreviewDisplay
                | RamBringupCommand::PreviewFrontpanel
                | RamBringupCommand::PreviewStatusLight
        )
    {
        write_command_error(state, usb, &request_id, "theme_requires_preview");
        return;
    }
    let theme = frame.theme.unwrap_or(RamBringupTheme::Light);
    let outcome = match command {
        RamBringupCommand::TestAdc => execute_test_adc(state).await,
        RamBringupCommand::TestI2c => execute_test_i2c(state),
        RamBringupCommand::TestRgb => execute_test_rgb(state).await,
        RamBringupCommand::TestBuzzer => execute_test_buzzer(state).await,
        RamBringupCommand::TestFan => unreachable!(),
        RamBringupCommand::PreviewStatusLight => execute_preview_status_light(state).await,
        RamBringupCommand::PreviewDisplay => execute_preview_display(state, theme).await,
        RamBringupCommand::PreviewFrontpanel => execute_preview_frontpanel(state, theme).await,
        RamBringupCommand::TestButtons => Ok(CommandResult::completed(command)),
    };
    write_command_outcome(state, usb, &request_id, outcome);
    safe_outputs(state);
}

#[cfg(target_arch = "xtensa")]
fn start_fan_test(state: &mut BringupState, request_id: &String<48>) {
    safe_outputs(state);
    let profile = FanCommand::from_phase(FanPhase::Mid);
    let mut result = CommandResult::completed(RamBringupCommand::TestFan);
    result.fan_pwm_permille = Some(profile.pwm_permille);
    result.fan_duration_ms = Some(FAN_PHASE_DURATION_SECS * 1_000);
    let _ = state
        .fan_pwm
        .set_duty_cycle_percent(pwm_percent_from_permille(profile.pwm_permille));
    state.fan_en.set_high();
    let mut request = String::new();
    let _ = request.push_str(request_id.as_str());
    state.pending_fan = Some(PendingFanTest {
        request_id: request,
        result,
        deadline: Instant::now() + Duration::from_secs(FAN_PHASE_DURATION_SECS.into()),
        next_heartbeat: Instant::now() + Duration::from_secs(1),
    });
}

#[cfg(target_arch = "xtensa")]
fn service_pending_fan(state: &mut BringupState, usb: &mut UsbSerialJtag<'static, Blocking>) {
    let Some(mut pending) = state.pending_fan.take() else {
        return;
    };
    let now = Instant::now();
    if now < pending.deadline {
        if now >= pending.next_heartbeat {
            let _ = usb.write(b"{\"type\":\"ram_fan_active\"}\n");
            pending.next_heartbeat = now + Duration::from_secs(1);
        }
        state.pending_fan = Some(pending);
        return;
    }
    let _ = state
        .fan_pwm
        .set_duty_cycle_percent(pwm_percent_from_permille(fan_stop_pwm_permille()));
    state.fan_en.set_low();
    write_command_outcome(state, usb, &pending.request_id, Ok(pending.result));
    safe_outputs(state);
}

#[cfg(target_arch = "xtensa")]
async fn execute_test_adc(state: &mut BringupState) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    let mut result = CommandResult::completed(RamBringupCommand::TestAdc);
    result.vin_mv = Some(read_adc_mv(&mut state.adc, &mut state.vin).ok_or("adc_error")?);
    result.rtd_mv = Some(read_adc_mv(&mut state.adc, &mut state.rtd).ok_or("adc_error")?);
    Ok(result)
}

#[cfg(target_arch = "xtensa")]
fn execute_test_i2c(state: &mut BringupState) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
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
    let mut result = CommandResult::completed(RamBringupCommand::TestI2c);
    result.i2c_device_id = Some(device_id[0]);
    result.i2c_revision = Some(revision[0]);
    Ok(result)
}

#[cfg(target_arch = "xtensa")]
async fn execute_test_rgb(state: &mut BringupState) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    for color in [
        RgbChannels::RED,
        RgbChannels::new(false, true, false),
        RgbChannels::BLUE,
        RgbChannels::WHITE,
    ] {
        apply_rgb(state, color);
        Timer::after_millis(300).await;
    }
    Ok(CommandResult::completed(RamBringupCommand::TestRgb))
}

#[cfg(target_arch = "xtensa")]
async fn execute_test_buzzer(state: &mut BringupState) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    for _ in 0..23 {
        state.buzzer.set_high();
        Timer::after_millis(1).await;
        state.buzzer.set_low();
        Timer::after_millis(1).await;
    }
    Ok(CommandResult::completed(RamBringupCommand::TestBuzzer))
}

#[cfg(target_arch = "xtensa")]
async fn execute_preview_status_light(
    state: &mut BringupState,
) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    for state_id in STATUS_LIGHT_PREVIEW_SEQUENCE {
        for elapsed_ms in (0..1_400).step_by(140) {
            apply_rgb(state, status_light_output(state_id, elapsed_ms));
            Timer::after_millis(140).await;
        }
    }
    Ok(CommandResult::completed(
        RamBringupCommand::PreviewStatusLight,
    ))
}

#[cfg(target_arch = "xtensa")]
async fn execute_preview_display(
    state: &mut BringupState,
    theme: RamBringupTheme,
) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
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
    Ok(CommandResult::completed(RamBringupCommand::PreviewDisplay))
}

#[cfg(target_arch = "xtensa")]
async fn execute_preview_frontpanel(
    state: &mut BringupState,
    theme: RamBringupTheme,
) -> Result<CommandResult, &'static str> {
    safe_outputs(state);
    for preview in FRONTPANEL_PREVIEW_SEQUENCE {
        let ui_state = Box::new(preview.build());
        render_frontpanel_ui_with_theme(
            state.canvas,
            ui_state.as_ref(),
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
    Ok(CommandResult::completed(
        RamBringupCommand::PreviewFrontpanel,
    ))
}

#[cfg(target_arch = "xtensa")]
fn write_command_outcome(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
    request_id: &String<48>,
    outcome: Result<CommandResult, &'static str>,
) {
    let (ok, result, error) = match outcome {
        Ok(result) => (true, Some(result), None),
        Err(error) => (false, None, Some(error)),
    };
    let length = write_response(
        state,
        usb,
        Response {
            frame_type: "response",
            request_id,
            ok,
            result: result.map(|command| ResponseResult {
                identity: None,
                command: Some(command),
            }),
            error,
        },
    );
    cache_command_response(state, request_id, length);
}

#[cfg(target_arch = "xtensa")]
fn write_command_error(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
    request_id: &String<48>,
    error: &'static str,
) {
    let length = write_response(
        state,
        usb,
        Response {
            frame_type: "response",
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        },
    );
    cache_command_response(state, request_id, length);
}

#[cfg(target_arch = "xtensa")]
fn cache_command_response(state: &mut BringupState, request_id: &String<48>, length: usize) {
    state.last_command_request_id.clear();
    let _ = state.last_command_request_id.push_str(request_id.as_str());
    state.last_command_response[..length].copy_from_slice(&state.response[..length]);
    state.last_command_response_len = length;
}

#[cfg(target_arch = "xtensa")]
fn handle_test_buttons_line(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
) -> bool {
    let Ok((frame, _)) = serde_json_core::from_slice::<IncomingFrame>(state.line.as_bytes()) else {
        return false;
    };
    let Some(request_id) = frame.request_id else {
        return true;
    };
    if frame.frame_type.as_deref() != Some("ram_bringup") {
        return false;
    }
    if frame.command != Some(RamBringupCommand::TestButtons) {
        return false;
    }
    let length = write_response(
        state,
        usb,
        Response {
            frame_type: "response",
            request_id: &request_id,
            ok: true,
            result: Some(ResponseResult {
                identity: None,
                command: Some(CommandResult {
                    key_mask: Some(sample_buttons(state)),
                    ..CommandResult::completed(RamBringupCommand::TestButtons)
                }),
            }),
            error: None,
        },
    );
    cache_command_response(state, &request_id, length);
    true
}

#[cfg(target_arch = "xtensa")]
async fn service_initial_identity(usb: &mut UsbSerialJtag<'static, Blocking>, identity: &Identity) {
    // The ROM USB handoff can take several seconds to re-enumerate. Keep the
    // initial probe alive while the host closes the ROM transport and reopens
    // this same USB port.
    let deadline = embassy_time::Instant::now() + Duration::from_secs(60);
    let mut line = String::<8192>::new();
    let mut response = [0_u8; RESPONSE_CAPACITY];
    while embassy_time::Instant::now() < deadline {
        while let Ok(byte) = usb.read_byte() {
            if byte == b'\n' {
                if let Ok((frame, _)) =
                    serde_json_core::from_slice::<IncomingFrame>(line.as_bytes())
                    && frame.frame_type.as_deref() == Some("request")
                    && frame.op.as_deref() == Some("get_identity")
                    && let Some(request_id) = frame.request_id.as_ref()
                    && let Ok(length) = serde_json_core::to_slice(
                        &Response {
                            frame_type: "response",
                            request_id,
                            ok: true,
                            result: Some(ResponseResult {
                                identity: Some(identity),
                                command: None,
                            }),
                            error: None,
                        },
                        &mut response,
                    )
                {
                    let _ = usb.write(&response[..length]);
                    let _ = usb.write(b"\n");
                    // Let the USB Serial/JTAG TX FIFO drain before moving on
                    // to board-peripheral initialization after ROM handoff.
                    Timer::after_millis(100).await;
                    return;
                }
                line.clear();
            } else if line.len() < line.capacity() {
                let _ = line.push(byte as char);
            } else {
                line.clear();
            }
        }
        Timer::after(Duration::from_millis(1)).await;
    }
}

#[cfg(target_arch = "xtensa")]
fn handle_identity_line(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
) -> bool {
    let Ok((frame, _)) = serde_json_core::from_slice::<IncomingFrame>(state.line.as_bytes()) else {
        return false;
    };
    if frame.frame_type.as_deref() != Some("request") || frame.op.as_deref() != Some("get_identity")
    {
        return false;
    }
    let Some(request_id) = frame.request_id else {
        return true;
    };
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
    true
}

#[cfg(target_arch = "xtensa")]
fn write_response(
    state: &mut BringupState,
    usb: &mut UsbSerialJtag<'static, Blocking>,
    response: Response<'_>,
) -> usize {
    if let Ok(length) = serde_json_core::to_slice(&response, &mut state.response) {
        let _ = usb.write(&state.response[..length]);
        let _ = usb.write(b"\n");
        length
    } else {
        0
    }
}

#[cfg(target_arch = "xtensa")]
fn safe_outputs(state: &mut BringupState) {
    state.heater_pwm.set_low();
    let _ = state
        .fan_pwm
        .set_duty_cycle_percent(pwm_percent_from_permille(fan_stop_pwm_permille()));
    state.fan_en.set_low();
    state.buzzer.set_low();
    state.rgb_r.set_high();
    state.rgb_g.set_high();
    state.rgb_b.set_high();
    state.backlight.set_high();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_fan_shutdown_matches_product_safe_output() {
        assert_eq!(fan_stop_pwm_permille(), PRODUCT_SAFE_FAN_PWM_PERMILLE);
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!("flux-purr-ram-bringup requires xtensa-esp32s3-none-elf");
}
