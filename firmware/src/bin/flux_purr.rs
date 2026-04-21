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
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
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
    DEFAULT_PD_VOLTAGE_REQUEST, FAN_PWM_FREQUENCY_HZ,
    adapters::ch224q::{self, Address, Status},
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
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_TARGET_MIN_C: i16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_TARGET_MAX_C: i16 = 400;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_FAN_ON_TEMP_C: i16 = 360;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_FAN_OFF_TEMP_C: i16 = 340;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HARD_CUTOFF_TEMP_C: i16 = 420;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CONTROL_INTERVAL_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_KP: f32 = 1.9;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_KI: f32 = 0.02;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_KD: f32 = 0.8;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_INTEGRAL_MIN: f32 = -500.0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PID_INTEGRAL_MAX: f32 = 500.0;
#[cfg(target_arch = "xtensa")]
const HEATER_PWM_FREQUENCY_HZ: u32 = 2_000;
#[cfg(target_arch = "xtensa")]
const FAN_TEST_DUTY_PERCENT: u8 = 0;
#[cfg(target_arch = "xtensa")]
const FAN_PWM_PERIOD_TICKS: u16 = 99;
#[cfg(target_arch = "xtensa")]
const HEATER_PWM_PERIOD_TICKS: u16 = 99;
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_ATTENUATION: Attenuation = Attenuation::_6dB;
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_COUNT: usize = 8;
#[cfg(target_arch = "xtensa")]
const RTD_LOG_INTERVAL_MS: u64 = 1_000;
#[cfg(target_arch = "xtensa")]
const PT1000_R0_OHMS: f32 = 1_000.0;
#[cfg(target_arch = "xtensa")]
const PT1000_A: f32 = 3.9083e-3;
#[cfg(target_arch = "xtensa")]
const PT1000_B: f32 = -5.775e-7;
#[cfg(target_arch = "xtensa")]
const PT1000_C: f32 = -4.183e-12;
#[cfg(target_arch = "xtensa")]
const RTD_REFERENCE_RESISTOR_OHMS: f32 = 2_490.0;
#[cfg(target_arch = "xtensa")]
// Use the board's effective RTD divider rail instead of the ideal 3V3 nominal.
// Runtime samples on the current hardware land near ambient only when the divider
// is solved against ~3.0 V; hardcoding 3.3 V biases the PT1000 reading low.
const RTD_DIVIDER_SUPPLY_MV: u16 = 3_000;
#[cfg(target_arch = "xtensa")]
const RTD_SHORT_FAULT_MAX_MV: u16 = 150;
#[cfg(target_arch = "xtensa")]
const RTD_OPEN_FAULT_MIN_MV: u16 = 2_800;
#[cfg(target_arch = "xtensa")]
const RTD_TEMP_MIN_C: f32 = -50.0;
#[cfg(target_arch = "xtensa")]
const RTD_TEMP_MAX_C: f32 = 500.0;
#[cfg(target_arch = "xtensa")]
const CH224Q_I2C_FREQUENCY_HZ: u32 = 100_000;
#[cfg(target_arch = "xtensa")]
const CH224Q_RETRY_ATTEMPTS: u8 = 3;
#[cfg(target_arch = "xtensa")]
const CH224Q_RETRY_DELAY_MS: u64 = 50;
#[cfg(target_arch = "xtensa")]
const CH224Q_PD_SETTLE_MS: u64 = 150;
#[cfg(target_arch = "xtensa")]
const CH224Q_STATUS_POLL_ATTEMPTS: u8 = 40;
#[cfg(target_arch = "xtensa")]
const CH224Q_STATUS_POLL_DELAY_MS: u64 = 100;

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

#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaterFaultReason {
    SensorShort,
    SensorOpen,
    AdcReadFailed,
    OverTemp,
}

#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterFaultReason {
    const fn label(self) -> &'static str {
        match self {
            Self::SensorShort => "sensor-short",
            Self::SensorOpen => "sensor-open",
            Self::AdcReadFailed => "adc-read-failed",
            Self::OverTemp => "over-temp",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterPidSnapshot {
    duty_percent: u8,
    error_c: f32,
    integral: f32,
    derivative_c_per_s: f32,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterController {
    fault_latched: Option<HeaterFaultReason>,
    last_target_temp_c: i16,
    last_measured_temp_c: Option<f32>,
    integral: f32,
    duty_percent: u8,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterController {
    const fn new() -> Self {
        Self {
            fault_latched: None,
            last_target_temp_c: 0,
            last_measured_temp_c: None,
            integral: 0.0,
            duty_percent: 0,
        }
    }

    const fn fault_latched(self) -> Option<HeaterFaultReason> {
        self.fault_latched
    }

    fn clear_fault_latch(&mut self) {
        self.fault_latched = None;
        self.integral = 0.0;
        self.last_measured_temp_c = None;
        self.duty_percent = 0;
    }

    fn latch_fault(&mut self, reason: HeaterFaultReason) -> bool {
        let changed = self.fault_latched != Some(reason);
        self.fault_latched = Some(reason);
        self.integral = 0.0;
        self.last_measured_temp_c = None;
        self.duty_percent = 0;
        changed
    }

    fn update(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
    ) -> HeaterPidSnapshot {
        let target_temp_c = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
        let previous_temp = self.last_measured_temp_c;
        let last_target_temp_c = self.last_target_temp_c;
        self.last_target_temp_c = target_temp_c;

        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
        }

        if !heater_enabled || self.fault_latched.is_some() {
            self.integral = 0.0;
            self.last_measured_temp_c = Some(measured_temp_c);
            self.duty_percent = 0;
            return HeaterPidSnapshot {
                duty_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                integral: 0.0,
                derivative_c_per_s: 0.0,
            };
        }

        if target_temp_c != last_target_temp_c {
            self.integral = 0.0;
            self.last_measured_temp_c = previous_temp.map(|_| measured_temp_c);
        }

        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let dt_s = HEATER_CONTROL_INTERVAL_MS as f32 / 1_000.0;
        let derivative_c_per_s = previous_temp
            .map(|previous| (measured_temp_c - previous) / dt_s)
            .unwrap_or(0.0);
        let proportional_derivative = HEATER_PID_KP * error_c - HEATER_PID_KD * derivative_c_per_s;
        let integral_candidate = (self.integral + error_c * dt_s)
            .clamp(HEATER_PID_INTEGRAL_MIN, HEATER_PID_INTEGRAL_MAX);
        let unsaturated_output = proportional_derivative + HEATER_PID_KI * self.integral;
        let saturating_high = unsaturated_output >= 100.0 && error_c > 0.0;
        let saturating_low = unsaturated_output <= 0.0 && error_c < 0.0;
        if !(saturating_high || saturating_low) {
            self.integral = integral_candidate;
        }
        let duty = (proportional_derivative + HEATER_PID_KI * self.integral).clamp(0.0, 100.0);

        self.last_measured_temp_c = Some(measured_temp_c);
        self.duty_percent = (duty + 0.5) as u8;

        HeaterPidSnapshot {
            duty_percent: self.duty_percent,
            error_c,
            integral: self.integral,
            derivative_c_per_s,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_run_fan(current_temp_c: i16, was_running: bool) -> bool {
    current_temp_c >= HEATER_FAN_ON_TEMP_C
        || (was_running && current_temp_c >= HEATER_FAN_OFF_TEMP_C)
}

#[cfg(any(target_arch = "xtensa", test))]
fn next_fan_runtime_enabled(current_temp_c: i16, was_running: bool, has_rtd_fault: bool) -> bool {
    if has_rtd_fault {
        was_running
    } else {
        should_run_fan(current_temp_c, was_running)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn is_overtemp_sample(temp_c: f32) -> bool {
    temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C)
}

#[cfg(any(target_arch = "xtensa", test))]
fn clear_runtime_temperature(latest_temp_c: &mut f32, latest_temp_i16: &mut i16) {
    *latest_temp_c = 0.0;
    *latest_temp_i16 = 0;
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct RtdMeasurement {
    adc_mv: u16,
    resistance_ohms: f32,
    temp_c: f32,
    current_temp_c: i16,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq)]
enum RtdSample {
    Valid(RtdMeasurement),
    Fault {
        adc_mv: Option<u16>,
        reason: HeaterFaultReason,
    },
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PdStatusObservation {
    status_raw: u8,
    status: Status,
    current_raw: u8,
    current_ma: u16,
}

#[cfg(target_arch = "xtensa")]
fn log_ui_state(state: &FrontPanelUiState) {
    info!(
        "ui route={=str} temp_c={=i16} target_c={=i16} heater_arm={=bool} heater_out={=u8}% fan={=bool}",
        route_label(state.route),
        state.current_temp_c,
        state.target_temp_c,
        state.heater_enabled,
        state.heater_output_percent,
        state.fan_enabled,
    );
}

#[cfg(target_arch = "xtensa")]
fn pt1000_resistance_ohms_at(temp_c: f32) -> f32 {
    let polynomial = 1.0 + PT1000_A * temp_c + PT1000_B * temp_c * temp_c;
    if temp_c >= 0.0 {
        PT1000_R0_OHMS * polynomial
    } else {
        PT1000_R0_OHMS * (polynomial + PT1000_C * (temp_c - 100.0) * temp_c * temp_c * temp_c)
    }
}

#[cfg(target_arch = "xtensa")]
fn pt1000_temperature_c_from_resistance(resistance_ohms: f32) -> f32 {
    let mut low = RTD_TEMP_MIN_C;
    let mut high = RTD_TEMP_MAX_C;
    for _ in 0..32 {
        let mid = (low + high) * 0.5;
        if pt1000_resistance_ohms_at(mid) < resistance_ohms {
            low = mid;
        } else {
            high = mid;
        }
    }
    (low + high) * 0.5
}

#[cfg(target_arch = "xtensa")]
fn rtd_resistance_ohms_from_mv(adc_mv: u16) -> Result<f32, HeaterFaultReason> {
    if adc_mv <= RTD_SHORT_FAULT_MAX_MV {
        return Err(HeaterFaultReason::SensorShort);
    }
    if adc_mv >= RTD_OPEN_FAULT_MIN_MV {
        return Err(HeaterFaultReason::SensorOpen);
    }
    let adc_mv_f = adc_mv as f32;
    let supply_mv_f = RTD_DIVIDER_SUPPLY_MV as f32;
    if adc_mv_f >= supply_mv_f {
        return Err(HeaterFaultReason::SensorOpen);
    }

    Ok(RTD_REFERENCE_RESISTOR_OHMS * adc_mv_f / (supply_mv_f - adc_mv_f))
}

#[cfg(target_arch = "xtensa")]
fn read_rtd_adc_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO2<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
    >,
) -> Option<u16> {
    let mut sum_mv: u32 = 0;
    for _ in 0..RTD_SAMPLE_COUNT {
        let sample_mv = loop {
            match adc.read_oneshot(pin) {
                Ok(value) => break value,
                Err(nb::Error::WouldBlock) => continue,
                Err(_) => return None,
            }
        };
        sum_mv = sum_mv.saturating_add(sample_mv as u32);
    }

    Some((sum_mv / RTD_SAMPLE_COUNT as u32) as u16)
}

#[cfg(target_arch = "xtensa")]
fn read_rtd_sample<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO2<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
    >,
) -> RtdSample {
    let Some(adc_mv) = read_rtd_adc_mv(adc, pin) else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };

    match rtd_resistance_ohms_from_mv(adc_mv) {
        Ok(resistance_ohms) => {
            let temp_c = pt1000_temperature_c_from_resistance(resistance_ohms);
            let current_temp_c = if temp_c >= 0.0 {
                (temp_c + 0.5) as i16
            } else {
                (temp_c - 0.5) as i16
            };
            RtdSample::Valid(RtdMeasurement {
                adc_mv,
                resistance_ohms,
                temp_c,
                current_temp_c,
            })
        }
        Err(reason) => RtdSample::Fault {
            adc_mv: Some(adc_mv),
            reason,
        },
    }
}

#[cfg(target_arch = "xtensa")]
fn read_ch224q_status(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
) -> Option<PdStatusObservation> {
    let status_raw = read_ch224q_register(i2c, address, ch224q::STATUS_REGISTER)?;
    let current_raw =
        read_ch224q_register(i2c, address, ch224q::CURRENT_DATA_REGISTER).unwrap_or(0);
    Some(PdStatusObservation {
        status_raw,
        status: Status::from_register(status_raw),
        current_raw,
        current_ma: ch224q::current_ma_from_register(current_raw),
    })
}

#[cfg(target_arch = "xtensa")]
fn apply_heater_duty<PWM>(heater_pwm: &mut PWM, duty_percent: u8, last_duty_percent: &mut u8)
where
    PWM: SetDutyCycle,
{
    if duty_percent == *last_duty_percent {
        return;
    }

    let _ = heater_pwm.set_duty_cycle_percent(duty_percent);
    info!(
        "heater output -> duty={=u8}% prev={=u8}%",
        duty_percent, *last_duty_percent,
    );
    *last_duty_percent = duty_percent;
}

#[cfg(target_arch = "xtensa")]
fn apply_fan_output<PWM>(
    fan_enable: &mut Output<'_>,
    fan_pwm: &mut PWM,
    fan_running: bool,
    last_fan_running: &mut bool,
) where
    PWM: SetDutyCycle,
{
    if fan_running == *last_fan_running {
        return;
    }

    let _ = fan_pwm.set_duty_cycle_percent(FAN_TEST_DUTY_PERCENT);
    if fan_running {
        fan_enable.set_high();
    } else {
        fan_enable.set_low();
    }
    info!(
        "fan runtime -> {=str} gpio35={=str} gpio36 duty={=u8}% freq={=u32}Hz",
        if fan_running { "run" } else { "off" },
        if fan_running { "on" } else { "off" },
        FAN_TEST_DUTY_PERCENT,
        FAN_PWM_FREQUENCY_HZ,
    );
    *last_fan_running = fan_running;
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
async fn request_ch224q_voltage(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    request: ch224q::VoltageRequest,
) -> Address {
    let payload = ch224q::voltage_request_payload(request);

    for attempt in 1..=CH224Q_RETRY_ATTEMPTS {
        for address in [Address::Primary, Address::Secondary] {
            if i2c.write(address.as_u8(), &payload).is_ok() {
                info!(
                    "ch224q request ok addr=0x{=u8:02x} reg=0x{=u8:02x} code={=u8} mv={=u16}",
                    address.as_u8(),
                    ch224q::VOLTAGE_CONTROL_REGISTER,
                    request.control_register_value(),
                    request.millivolts(),
                );
                return address;
            }
        }

        info!(
            "ch224q request retry={=u8}/{=u8} mv={=u16}",
            attempt,
            CH224Q_RETRY_ATTEMPTS,
            request.millivolts(),
        );
        EmbassyTimer::after_millis(CH224Q_RETRY_DELAY_MS).await;
    }

    panic!("failed to program CH224Q voltage request");
}

#[cfg(target_arch = "xtensa")]
fn read_ch224q_register(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
    register: u8,
) -> Option<u8> {
    let mut value = [0u8; 1];
    i2c.write_read(address.as_u8(), &[register], &mut value)
        .ok()
        .map(|_| value[0])
}

#[cfg(target_arch = "xtensa")]
async fn await_ch224q_pd_ready(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
) -> Option<(u8, Status, u8, u16)> {
    for attempt in 1..=CH224Q_STATUS_POLL_ATTEMPTS {
        let Some(status_raw) = read_ch224q_register(i2c, address, ch224q::STATUS_REGISTER) else {
            info!(
                "ch224q status read failed addr=0x{=u8:02x} attempt={=u8}/{=u8}",
                address.as_u8(),
                attempt,
                CH224Q_STATUS_POLL_ATTEMPTS,
            );
            EmbassyTimer::after_millis(CH224Q_STATUS_POLL_DELAY_MS).await;
            continue;
        };
        let current_raw =
            read_ch224q_register(i2c, address, ch224q::CURRENT_DATA_REGISTER).unwrap_or(0);
        let status = Status::from_register(status_raw);
        let current_ma = ch224q::current_ma_from_register(current_raw);
        info!(
            "ch224q status addr=0x{=u8:02x} attempt={=u8}/{=u8} status=0x{=u8:02x} current_raw=0x{=u8:02x} current_ma={=u16}",
            address.as_u8(),
            attempt,
            CH224Q_STATUS_POLL_ATTEMPTS,
            status_raw,
            current_raw,
            current_ma,
        );
        if status.pd_active && !status.epr_active {
            return Some((status_raw, status, current_raw, current_ma));
        }
        EmbassyTimer::after_millis(CH224Q_STATUS_POLL_DELAY_MS).await;
    }

    None
}

#[cfg(target_arch = "xtensa")]
async fn run_key_test_runtime<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    inputs: FrontPanelInputs<'a>,
) -> !
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
    flush_ui(display, canvas, &ui_state)
        .await
        .expect("failed to draw initial key-test UI");
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
            flush_ui(display, canvas, &ui_state)
                .await
                .expect("failed to refresh key-test UI");
            log_ui_state(&ui_state);
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let runtime_mode = FrontPanelRuntimeMode::compile_time_default();

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
    info!(
        "frontpanel runtime mode={=str}",
        runtime_mode_label(runtime_mode)
    );

    if runtime_mode == FrontPanelRuntimeMode::KeyTest {
        let mut _heater_safe = Output::new(peripherals.GPIO47, Level::Low, OutputConfig::default());
        _heater_safe.set_low();
        let mut _fan_enable_safe =
            Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
        _fan_enable_safe.set_low();
        let mut _fan_pwm_safe =
            Output::new(peripherals.GPIO36, Level::Low, OutputConfig::default());
        _fan_pwm_safe.set_low();
        info!("key-test runtime ready: gpio47/gpio35/gpio36 held safe-off without PD/RTD bring-up");
        run_key_test_runtime(&mut display, canvas, inputs).await;
    }

    let mut pd_i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(CH224Q_I2C_FREQUENCY_HZ)),
    )
    .expect("failed to create I2C0")
    .with_sda(peripherals.GPIO8)
    .with_scl(peripherals.GPIO9);
    let ch224q_address = request_ch224q_voltage(&mut pd_i2c, DEFAULT_PD_VOLTAGE_REQUEST).await;
    info!(
        "pd request locked addr=0x{=u8:02x} target_mv={=u16} settle_ms={=u64}",
        ch224q_address.as_u8(),
        DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
        CH224Q_PD_SETTLE_MS,
    );
    EmbassyTimer::after_millis(CH224Q_PD_SETTLE_MS).await;

    let mut adc1_config = AdcConfig::new();
    let mut rtd_adc_pin = adc1_config
        .enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO2, RTD_SAMPLE_ATTENUATION);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    info!(
        "rtd monitor active: gpio2 atten={=str} samples={=u8} interval_ms={=u64}",
        "6dB", RTD_SAMPLE_COUNT as u8, RTD_LOG_INTERVAL_MS,
    );

    let mut fan_enable = Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
    let pwm_clock_cfg = PeripheralClockConfig::with_frequency(Rate::from_mhz(40))
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
    let _ = fan_pwm.set_duty_cycle_percent(FAN_TEST_DUTY_PERCENT);
    info!(
        "fan runtime armed: gpio35 default=off gpio36 duty={=u8}% freq={=u32}Hz on>= {=i16}C off< {=i16}C",
        FAN_TEST_DUTY_PERCENT, FAN_PWM_FREQUENCY_HZ, HEATER_FAN_ON_TEMP_C, HEATER_FAN_OFF_TEMP_C,
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
    let mut last_pd_observation = if let Some((status_raw, status, current_raw, current_ma)) =
        await_ch224q_pd_ready(&mut pd_i2c, ch224q_address).await
    {
        info!(
            "heater runtime ready: gpio47 freq={=u32}Hz target={=i16}~{=i16}C fan_on={=i16}C cutoff={=i16}C pd_status=0x{=u8:02x} pd={=bool} epr={=bool} epr_exist={=bool} current_raw=0x{=u8:02x} current_ma={=u16}",
            HEATER_PWM_FREQUENCY_HZ,
            HEATER_PID_TARGET_MIN_C,
            HEATER_PID_TARGET_MAX_C,
            HEATER_FAN_ON_TEMP_C,
            HEATER_HARD_CUTOFF_TEMP_C,
            status_raw,
            status.pd_active,
            status.epr_active,
            status.epr_exist,
            current_raw,
            current_ma,
        );
        Some(PdStatusObservation {
            status_raw,
            status,
            current_raw,
            current_ma,
        })
    } else {
        info!(
            "heater runtime continuing: CH224Q PD status not ready after request_mv={=u16}; status will be observed only",
            DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
        );
        read_ch224q_status(&mut pd_i2c, ch224q_address)
    };
    info!(
        "heater pid params kp={=f32} ki={=f32} kd={=f32} interval_ms={=u64}",
        HEATER_PID_KP, HEATER_PID_KI, HEATER_PID_KD, HEATER_CONTROL_INTERVAL_MS,
    );

    let initial_rtd_sample = read_rtd_sample(&mut adc1, &mut rtd_adc_pin);
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new(runtime_mode);
    let mut heater_controller = HeaterController::new();
    let mut current_rtd_fault: Option<HeaterFaultReason> = None;
    let mut latest_temp_c = 0.0_f32;
    let mut latest_temp_i16 = 0_i16;
    match initial_rtd_sample {
        RtdSample::Valid(measurement) => {
            latest_temp_c = measurement.temp_c;
            latest_temp_i16 = measurement.current_temp_c;
            if is_overtemp_sample(measurement.temp_c) {
                current_rtd_fault = Some(HeaterFaultReason::OverTemp);
                let _ = heater_controller.latch_fault(HeaterFaultReason::OverTemp);
                info!(
                    "heater initial fault latched reason={=str}",
                    HeaterFaultReason::OverTemp.label()
                );
            }
            ui_state.current_temp_c = measurement.current_temp_c;
            info!(
                "rtd initial adc_mv={=u16} divider_mv={=u16} resistance_ohms={=f32} temp_c={=f32}",
                measurement.adc_mv,
                RTD_DIVIDER_SUPPLY_MV,
                measurement.resistance_ohms,
                measurement.temp_c,
            );
        }
        RtdSample::Fault { adc_mv, reason } => {
            current_rtd_fault = Some(reason);
            let _ = heater_controller.latch_fault(reason);
            ui_state.current_temp_c = 0;
            info!(
                "rtd initial fault adc_mv={=u16} reason={=str}",
                adc_mv.unwrap_or(0),
                reason.label(),
            );
        }
    }
    let mut last_heater_duty = 0_u8;
    let mut fan_runtime_enabled = should_run_fan(latest_temp_i16, false);
    let mut fan_output_applied = false;
    ui_state.fan_enabled = fan_runtime_enabled;
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    apply_heater_duty(&mut heater_pwm, 0, &mut last_heater_duty);
    apply_fan_output(
        &mut fan_enable,
        &mut fan_pwm,
        fan_runtime_enabled,
        &mut fan_output_applied,
    );
    flush_ui(&mut display, canvas, &ui_state)
        .await
        .expect("failed to draw initial frontpanel UI");
    log_ui_state(&ui_state);

    let mut elapsed_ms: u64 = 0;
    let mut last_control_ms: u64 = 0;
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
            let heater_enabled_before = ui_state.heater_enabled;
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
            if ui_state.heater_enabled != heater_enabled_before {
                if ui_state.heater_enabled {
                    if heater_controller.fault_latched().is_some() {
                        if let Some(reason) = current_rtd_fault {
                            ui_state.heater_enabled = false;
                            info!("heater re-arm blocked reason={=str}", reason.label(),);
                        } else {
                            heater_controller.clear_fault_latch();
                            info!("heater re-arm -> cleared latched fault");
                        }
                    } else {
                        info!("heater arm -> on");
                    }
                } else {
                    info!("heater arm -> off");
                }
            }
        }

        if elapsed_ms.saturating_sub(last_control_ms) >= HEATER_CONTROL_INTERVAL_MS {
            last_control_ms = elapsed_ms;

            match read_rtd_sample(&mut adc1, &mut rtd_adc_pin) {
                RtdSample::Valid(measurement) => {
                    current_rtd_fault = if is_overtemp_sample(measurement.temp_c) {
                        Some(HeaterFaultReason::OverTemp)
                    } else {
                        None
                    };
                    latest_temp_c = measurement.temp_c;
                    latest_temp_i16 = measurement.current_temp_c;
                    if ui_state.current_temp_c != measurement.current_temp_c {
                        ui_state.current_temp_c = measurement.current_temp_c;
                        needs_redraw = true;
                    }
                    info!(
                        "rtd sample adc_mv={=u16} divider_mv={=u16} resistance_ohms={=f32} temp_c={=f32} heater_arm={=bool}",
                        measurement.adc_mv,
                        RTD_DIVIDER_SUPPLY_MV,
                        measurement.resistance_ohms,
                        measurement.temp_c,
                        ui_state.heater_enabled,
                    );
                }
                RtdSample::Fault { adc_mv, reason } => {
                    current_rtd_fault = Some(reason);
                    clear_runtime_temperature(&mut latest_temp_c, &mut latest_temp_i16);
                    info!(
                        "rtd fault adc_mv={=u16} reason={=str} heater_arm={=bool}",
                        adc_mv.unwrap_or(0),
                        reason.label(),
                        ui_state.heater_enabled,
                    );
                }
            }

            if let Some(reason) = current_rtd_fault
                && heater_controller.latch_fault(reason)
            {
                ui_state.heater_enabled = false;
                needs_redraw = true;
                info!("heater fault latched reason={=str}", reason.label());
            }

            let current_pd_observation = read_ch224q_status(&mut pd_i2c, ch224q_address);
            if current_pd_observation != last_pd_observation {
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
                last_pd_observation = current_pd_observation;
            }

            let next_fan_runtime_enabled = next_fan_runtime_enabled(
                latest_temp_i16,
                fan_runtime_enabled,
                current_rtd_fault.is_some(),
            );
            if ui_state.fan_enabled != next_fan_runtime_enabled {
                ui_state.fan_enabled = next_fan_runtime_enabled;
                needs_redraw = true;
            }
            fan_runtime_enabled = next_fan_runtime_enabled;
            apply_fan_output(
                &mut fan_enable,
                &mut fan_pwm,
                fan_runtime_enabled,
                &mut fan_output_applied,
            );

            let pid_snapshot = heater_controller.update(
                ui_state.target_temp_c,
                latest_temp_c,
                ui_state.heater_enabled,
            );
            if ui_state.heater_output_percent != pid_snapshot.duty_percent {
                ui_state.heater_output_percent = pid_snapshot.duty_percent;
                needs_redraw = true;
            }
            apply_heater_duty(
                &mut heater_pwm,
                pid_snapshot.duty_percent,
                &mut last_heater_duty,
            );

            info!(
                "heater loop set_c={=i16} temp_c={=f32} duty={=u8}% error_c={=f32} integral={=f32} deriv_cps={=f32} arm={=bool} fault={=str}",
                ui_state.target_temp_c,
                latest_temp_c,
                pid_snapshot.duty_percent,
                pid_snapshot.error_c,
                pid_snapshot.integral,
                pid_snapshot.derivative_c_per_s,
                ui_state.heater_enabled,
                heater_controller
                    .fault_latched()
                    .map(|reason| reason.label())
                    .unwrap_or("none"),
            );
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
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3[,frontpanel-key-test]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heater_pid_saturates_when_far_below_target() {
        let mut controller = HeaterController::new();
        let snapshot = controller.update(380, 25.0, true);

        assert_eq!(snapshot.duty_percent, 100);
        assert!(snapshot.error_c > 300.0);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn heater_pid_reduces_output_as_temperature_rises() {
        let mut controller = HeaterController::new();
        let cold = controller.update(380, 25.0, true);
        let warm = controller.update(380, 300.0, true);
        let near_setpoint = controller.update(380, 378.0, true);

        assert!(cold.duty_percent > warm.duty_percent);
        assert!(warm.duty_percent >= near_setpoint.duty_percent);
    }

    #[test]
    fn heater_pid_unwinds_after_sustained_saturation() {
        let mut controller = HeaterController::new();

        let mut snapshot = controller.update(380, 25.0, true);
        assert_eq!(snapshot.duty_percent, 100);

        for measured in [50.0, 80.0, 120.0, 160.0, 200.0, 240.0, 280.0, 320.0] {
            snapshot = controller.update(380, measured, true);
        }
        assert!(snapshot.duty_percent < 100);

        let near_setpoint = controller.update(380, 370.0, true);
        assert!(near_setpoint.duty_percent < 20);

        let at_target = controller.update(380, 380.0, true);
        assert_eq!(at_target.duty_percent, 0);
    }

    #[test]
    fn heater_pid_resets_when_disabled() {
        let mut controller = HeaterController::new();
        let enabled = controller.update(380, 25.0, true);
        let disabled = controller.update(380, 40.0, false);

        assert!(enabled.duty_percent > 0);
        assert_eq!(disabled.duty_percent, 0);
        assert_eq!(disabled.integral, 0.0);
    }

    #[test]
    fn heater_fault_latch_requires_manual_clear() {
        let mut controller = HeaterController::new();
        let overtemp = controller.update(380, 421.0, true);
        assert_eq!(overtemp.duty_percent, 0);
        assert_eq!(
            controller.fault_latched(),
            Some(HeaterFaultReason::OverTemp)
        );

        let still_latched = controller.update(380, 200.0, true);
        assert_eq!(still_latched.duty_percent, 0);
        assert_eq!(
            controller.fault_latched(),
            Some(HeaterFaultReason::OverTemp)
        );

        controller.clear_fault_latch();
        let rearmed = controller.update(380, 200.0, true);
        assert!(rearmed.duty_percent > 0);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn fan_runtime_uses_overtemp_hysteresis() {
        assert!(!should_run_fan(359, false));
        assert!(should_run_fan(360, false));
        assert!(should_run_fan(350, true));
        assert!(!should_run_fan(339, true));
    }

    #[test]
    fn overtemp_threshold_uses_unrounded_temperature() {
        assert!(!is_overtemp_sample(419.9));
        assert!(is_overtemp_sample(420.0));
    }

    #[test]
    fn rtd_fault_keeps_existing_fan_cooling_state() {
        assert!(next_fan_runtime_enabled(0, true, true));
        assert!(!next_fan_runtime_enabled(0, false, true));
        assert!(next_fan_runtime_enabled(360, false, false));
    }

    #[test]
    fn rtd_fault_clears_cached_runtime_temperature() {
        let mut latest_temp_c = 378.4;
        let mut latest_temp_i16 = 378;

        clear_runtime_temperature(&mut latest_temp_c, &mut latest_temp_i16);
        assert_eq!(latest_temp_c, 0.0);
        assert_eq!(latest_temp_i16, 0);
    }
}
