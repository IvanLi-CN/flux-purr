#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
use core::panic::PanicInfo;
#[cfg(target_arch = "xtensa")]
use defmt::{info, warn};
#[cfg(target_arch = "xtensa")]
use embassy_executor::Spawner;
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Instant, Timer as EmbassyTimer, with_timeout};
#[cfg(target_arch = "xtensa")]
use embedded_graphics::prelude::RgbColor;
#[cfg(target_arch = "xtensa")]
use embedded_hal::pwm::SetDutyCycle;
#[cfg(target_arch = "xtensa")]
use embedded_hal_bus::spi::ExclusiveDevice;
#[cfg(target_arch = "xtensa")]
use esp_hal::rtc_cntl::SocResetReason;
#[cfg(target_arch = "xtensa")]
use esp_hal::{
    Blocking,
    analog::adc::{Adc, AdcCalCurve, AdcConfig, Attenuation},
    clock::CpuClock,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    mcpwm::{
        McPwm, PeripheralClockConfig,
        operator::PwmPinConfig,
        timer::{CounterDirection, PwmWorkingMode},
    },
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
    usb_serial_jtag::UsbSerialJtag,
};
#[cfg(test)]
use flux_purr_firmware::DEFAULT_PD_VOLTAGE_REQUEST;
#[cfg(test)]
use flux_purr_firmware::adapters::ch224q;
#[cfg(test)]
use flux_purr_firmware::adapters::ch224q::Status;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::board::s3_frontpanel;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::buzzer::BuzzerOutput;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::buzzer::{BuzzerController, BuzzerCueId};
#[cfg(test)]
use flux_purr_firmware::control_plane::ThermalControlProfileCommand;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::control_plane::{
    ApiError, CalibrationControlCommand, CalibrationJobKindWire, CalibrationJobStateWire,
    CalibrationJobStatusWire, CalibrationModeWire, CalibrationRuntimeStateWire, ControlPlaneStatus,
    Identity, RuntimeConfigCommand, ThermalControlProfileOp, ThermalControlProfilePointWire,
    ThermalControlProfileSettingsWire, ThermalControlProfileWire, ThermalControlRuntimeWire,
    UsbFrame, UsbFrameError, UsbRequestOp, UsbResponsePayload, calibration_state_from_memory,
    heater_curve_state_from_memory, network_from_memory, parse_usb_frame, write_usb_frame,
};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::control_plane::{
    CalibrationChannelWire, CalibrationConfigCommand, CalibrationConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::{
    CalibrationJobCommandWire, CalibrationJobOpWire, HeaterCurveConfigCommand, HeaterCurveConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::{
    CalibrationSampleWire, CalibrationSlotFitWire, CalibrationSlotIdWire, CalibrationStateWire,
    samples_from_wire,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::{hello_frame, log_frame};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::frontpanel::{
    FRONTPANEL_PRESET_COUNT, FRONTPANEL_TARGET_TEMP_MAX_C, FRONTPANEL_TARGET_TEMP_MIN_C,
    FanDisplayState, FrontPanelKeyMap, FrontPanelRawState, FrontPanelRuntimeMode,
    FrontPanelUiState, HeaterLockReason,
};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::memory::{
    ADC_CALIBRATION_MAX_SAMPLES, AdcCalibrationSample, HEATER_CURVE_MAX_POINTS, HeaterCurvePoint,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::memory::{AdcCalibrationChannel, correct_adc_mv};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::memory::{
    HeaterCurveConfig, MemoryConfig,
    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
    THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
    THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
    THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
    THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX,
    THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS, ThermalControlProfileConfig,
    ThermalControlProfilePointConfig, ThermalControlProfileSettingsConfig, ThermalProfileBank,
    ThermalProfileMode, heater_resistance_ohms_from_curve,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::memory::{
    LEGACY_MEMORY_SLOT_A_OFFSET, LEGACY_MEMORY_SLOT_B_OFFSET, LEGACY_MEMORY_SLOT_SIZE,
    M24C64_PAGE_SIZE, M24c64, MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_B_OFFSET, MEMORY_SLOT_SIZE,
    MEMORY_WRITE_DEBOUNCE_MS, MemoryRecord, PREVIOUS_MEMORY_SLOT_A_OFFSET,
    PREVIOUS_MEMORY_SLOT_B_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE, decode_memory_record,
    encode_memory_record, select_latest_memory_record,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    DEFAULT_PD_VOLTAGE_REQUEST, FAN_PWM_FREQUENCY_HZ, pwm_percent_from_permille,
};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::{DeviceMode, DeviceStatus, PdState};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    adapters::ch224q::{self, Address, Status},
    display::{DISPLAY_PANEL_CONFIG, DisplayCanvas, SceneId, render_scene},
    frontpanel::{
        FRONTPANEL_DEBOUNCE_MS, FRONTPANEL_DOUBLE_CLICK_MS, FrontPanelInputController,
        FrontPanelInputTimings, FrontPanelRoute, KeyGesture, RawFrontPanelKey,
        render::render_frontpanel_ui,
    },
};
#[cfg(target_arch = "xtensa")]
use gc9d01::{GC9D01, Timer as Gc9d01Timer};
#[cfg(target_arch = "xtensa")]
use micromath::F32Ext;
#[cfg(target_arch = "xtensa")]
use static_cell::StaticCell;

#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(target_arch = "xtensa")]
fn reset_reason_log_line(reason: Option<SocResetReason>) -> &'static str {
    match reason {
        Some(SocResetReason::ChipPowerOn) => "reset_reason=chip_power_on\n",
        Some(SocResetReason::CoreSw) => "reset_reason=core_software\n",
        Some(SocResetReason::CoreDeepSleep) => "reset_reason=core_deep_sleep\n",
        Some(SocResetReason::CoreMwdt0) => "reset_reason=core_mwdt0\n",
        Some(SocResetReason::CoreMwdt1) => "reset_reason=core_mwdt1\n",
        Some(SocResetReason::CoreRtcWdt) => "reset_reason=core_rtc_wdt\n",
        Some(SocResetReason::CpuMwdt0) => "reset_reason=cpu_mwdt0\n",
        Some(SocResetReason::CpuSw) => "reset_reason=cpu_software\n",
        Some(SocResetReason::CpuRtcWdt) => "reset_reason=cpu_rtc_wdt\n",
        Some(SocResetReason::SysBrownOut) => "reset_reason=system_brownout\n",
        Some(SocResetReason::SysRtcWdt) => "reset_reason=system_rtc_wdt\n",
        Some(SocResetReason::CpuMwdt1) => "reset_reason=cpu_mwdt1\n",
        Some(SocResetReason::SysSuperWdt) => "reset_reason=system_super_wdt\n",
        Some(SocResetReason::SysClkGlitch) => "reset_reason=system_clock_glitch\n",
        Some(SocResetReason::CoreEfuseCrc) => "reset_reason=core_efuse_crc\n",
        Some(SocResetReason::CoreUsbUart) => "reset_reason=core_usb_uart\n",
        Some(SocResetReason::CoreUsbJtag) => "reset_reason=core_usb_jtag\n",
        Some(SocResetReason::CorePwrGlitch) => "reset_reason=core_power_glitch\n",
        None => "reset_reason=unknown\n",
    }
}

#[cfg(target_arch = "xtensa")]
#[defmt::global_logger]
struct UsbControlNoopLogger;

#[cfg(target_arch = "xtensa")]
unsafe impl defmt::Logger for UsbControlNoopLogger {
    fn acquire() {}

    unsafe fn flush() {}

    unsafe fn release() {}

    unsafe fn write(_bytes: &[u8]) {}
}

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {}
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
struct RawUsbSerialJtag {
    inner: UsbSerialJtag<'static, Blocking>,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
impl RawUsbSerialJtag {
    fn new(usb_device: esp_hal::peripherals::USB_DEVICE<'static>) -> Self {
        Self {
            inner: UsbSerialJtag::new(usb_device),
        }
    }

    fn read_byte(&mut self) -> nb::Result<u8, ()> {
        self.inner.read_byte().map_err(|err| match err {
            nb::Error::WouldBlock => nb::Error::WouldBlock,
            nb::Error::Other(_) => nb::Error::Other(()),
        })
    }
}

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
const AUTO_COOLING_FAN_MIN_TEMP_C: i16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const AUTO_COOLING_FAN_FULL_TEMP_C: i16 = 60;
#[cfg(any(target_arch = "xtensa", test))]
const AUTO_COOLING_FAN_COOLDOWN_MS: u64 = 30_000;
#[cfg(any(target_arch = "xtensa", test))]
const COOLING_DISABLED_PULSE_START_TEMP_C: i16 = 100;
#[cfg(any(target_arch = "xtensa", test))]
const COOLING_DISABLED_HEATER_LOCK_TEMP_C: i16 = 350;
#[cfg(any(target_arch = "xtensa", test))]
const COOLING_DISABLED_FAN_FULL_TEMP_C: i16 = 360;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HARD_CUTOFF_TEMP_C: i16 = 420;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
const HEATER_PROFILE_TICK_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
// Keep the per-cycle RTD aggregate unchanged while doubling control and RTD update cadence.
const HEATER_CONTROL_INTERVAL_MS: u64 = 50;
#[cfg(any(target_arch = "xtensa", test))]
const RUNTIME_INPUT_POLL_MAX_INTERVAL_MS: u64 = 20;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PHASE_HYSTERESIS_C: f32 = 0.10;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
const DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_ADJUSTABLE_MIN_MV: u16 = 12_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_ADJUSTABLE_MAX_MV: u16 = 28_000;
#[cfg(any(target_arch = "xtensa", test))]
const CH224Q_ADJUSTABLE_REQUEST_MIN_MV: u16 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_REQUEST_HYSTERESIS_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_REQUEST_STEP_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_SMALL_TRANSITION_MS: u64 = 25;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_LARGE_TRANSITION_MS: u64 = 275;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_PULSE_PERIOD_MS: u64 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATING_FAN_PULSE_MAX_DUTY_PERCENT: u8 = 50;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const DISPLAY_BRINGUP_TIMEOUT_MS: u64 = 1_500;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const USB_CONTROL_LINE_CAPACITY: usize = flux_purr_firmware::control_plane::USB_LINE_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_BUFFER_LEN: usize = flux_purr_firmware::control_plane::USB_LINE_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_PACKET_LEN: usize = 64;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_RETRY_LIMIT: usize = 4096;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_FULL_SPEED_PWM_PERMILLE: u16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_ACTIVE_COOLING_PWM_PERMILLE: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_HALF_SPEED_PWM_PERMILLE: u16 = 250;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE: u16 = 1_000;
#[cfg(test)]
const HEATER_APPROACH_DUTY_PERCENT: u8 = 32;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PROFILE_R20_OHMS: f32 = 3.2;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PROFILE_TEMP_COEFFICIENT_PER_C: f32 = 0.00393;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURRENT_LIMIT_FALLBACK_REQUEST: ch224q::VoltageRequest = ch224q::VoltageRequest::V9;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURRENT_LIMIT_RETURN_HYSTERESIS_MV: u16 = 200;
#[cfg(target_arch = "xtensa")]
const HEATER_PWM_FREQUENCY_HZ: u32 = 2_000;
#[cfg(target_arch = "xtensa")]
const FAN_PWM_PERIOD_TICKS: u16 = 99;
#[cfg(target_arch = "xtensa")]
const HEATER_PWM_PERIOD_TICKS: u16 = 99;
#[cfg(target_arch = "xtensa")]
const BUZZER_PWM_PERIOD_TICKS: u16 = 999;
#[cfg(target_arch = "xtensa")]
const BUZZER_IDLE_FREQUENCY_HZ: u32 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
const BUZZER_ATTENTION_REMINDER_INTERVAL_MS: u64 = 10_000;
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_ATTENUATION: Attenuation = Attenuation::_6dB;
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_COUNT: usize = 64;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_MIN_VALID_SAMPLE_COUNT: usize = 48;
#[cfg(target_arch = "xtensa")]
const RTD_LOG_INTERVAL_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
const PT1000_R0_OHMS: f32 = 1_000.0;
#[cfg(any(target_arch = "xtensa", test))]
const PT1000_A: f32 = 3.9083e-3;
#[cfg(any(target_arch = "xtensa", test))]
const PT1000_B: f32 = -5.775e-7;
#[cfg(any(target_arch = "xtensa", test))]
const PT1000_C: f32 = -4.183e-12;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_REFERENCE_RESISTOR_OHMS: f32 = 2_490.0;
#[cfg(any(target_arch = "xtensa", test))]
// Use the board's effective RTD divider rail instead of the ideal 3V3 nominal.
// Runtime samples on the current hardware land near ambient only when the divider
// is solved against ~3.0 V; hardcoding 3.3 V biases the PT1000 reading low.
const RTD_DIVIDER_SUPPLY_MV: u16 = 3_000;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_SHORT_FAULT_MAX_MV: u16 = 150;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_OPEN_FAULT_MIN_MV: u16 = 2_800;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_TEMP_MIN_C: f32 = -50.0;
#[cfg(any(target_arch = "xtensa", test))]
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
const EEPROM_WRITE_CYCLE_DELAY_MS: u64 = 5;

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
enum HeaterControlPhase {
    Warmup,
    Approach,
    Hold,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterControlPhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Approach => "approach",
            Self::Hold => "hold",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterPidSnapshot {
    duty_percent: u8,
    error_c: f32,
    control_error_c: f32,
    filtered_temp_c: f32,
    filtered_slope_c_per_s: f32,
    coast_active: bool,
    phase: HeaterControlPhase,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Default)]
struct HeaterControlTiming {
    interval_ms: u16,
    cycle_ms: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThermalControlProfilePoint {
    target_temp_c: i16,
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalControlProfile {
    settings: ThermalControlProfileSettings,
    points: [Option<ThermalControlProfilePoint>; FRONTPANEL_PRESET_COUNT],
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalControlTarget {
    brake_distance_c: f32,
    warmup_power_permille: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent: f32,
    approach_tail_window_c: f32,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    hold_entry_error_c: f32,
    hold_exit_error_c: f32,
    hold_on_error_c: f32,
    hold_off_error_c: f32,
    overshoot_cutoff_c: f32,
    hold_kp_permille_per_c: f32,
    hold_ki_permille_per_c_tick: f32,
    hold_blend_ticks: u8,
    approach_lead_ticks: u8,
    hold_lead_ticks: u8,
    settings: ThermalControlProfileSettings,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalControlProfileSettings {
    temp_filter_alpha: f32,
    warmup_reenter_error_c: f32,
    hold_entry_error_c: f32,
    hold_exit_error_c: f32,
    hold_on_error_c: f32,
    hold_off_error_c: f32,
    overshoot_cutoff_c: f32,
    approach_max_ticks: u8,
    approach_min_power_ratio: f32,
    hold_kp_permille_per_c: f32,
    hold_ki_permille_per_c_tick: f32,
    hold_blend_ticks: u8,
    hold_reheat_power_permille: u16,
    approach_lead_ticks: u8,
    hold_lead_ticks: u8,
    auto_adjustable_working_floor_mv: u16,
    heater_current_reserve_ma: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ThermalControlProfilePoint {
    fn sanitized(self) -> Self {
        let sanitize_inherited = |value: u16, max: u16| {
            if value == 0 { 0 } else { value.min(max) }
        };
        Self {
            target_temp_c: self.target_temp_c,
            brake_distance_centi_c: self.brake_distance_centi_c.clamp(100, 5_000),
            warmup_power_permille: self.warmup_power_permille.min(1_000),
            approach_power_permille: self.approach_power_permille.min(1_000),
            approach_floor_power_permille: self.approach_floor_power_permille.min(1_000),
            approach_damping_exponent_permille: if self.approach_damping_exponent_permille == 0 {
                THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT
            } else {
                self.approach_damping_exponent_permille.clamp(
                    100,
                    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
                )
            },
            approach_tail_window_centi_c: self
                .approach_tail_window_centi_c
                .min(THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX),
            hold_power_permille: self.hold_power_permille.min(1_000),
            hold_reheat_power_permille: self.hold_reheat_power_permille.min(1_000),
            hold_entry_centi_c: sanitize_inherited(self.hold_entry_centi_c, 5_000),
            hold_exit_centi_c: sanitize_inherited(self.hold_exit_centi_c, 5_000),
            hold_on_centi_c: sanitize_inherited(self.hold_on_centi_c, 5_000),
            hold_off_centi_c: sanitize_inherited(self.hold_off_centi_c, 5_000),
            overshoot_cutoff_centi_c: sanitize_inherited(self.overshoot_cutoff_centi_c, 5_000),
            hold_kp_permille_per_c: sanitize_inherited(self.hold_kp_permille_per_c, 10_000),
            hold_ki_permille_per_c_tick: sanitize_inherited(
                self.hold_ki_permille_per_c_tick,
                10_000,
            ),
            hold_blend_ticks: sanitize_inherited(self.hold_blend_ticks, u16::from(u8::MAX)),
            approach_lead_ticks: sanitize_inherited(self.approach_lead_ticks, u16::from(u8::MAX)),
            hold_lead_ticks: sanitize_inherited(self.hold_lead_ticks, u16::from(u8::MAX)),
        }
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
impl From<ThermalControlProfilePointWire> for ThermalControlProfilePoint {
    fn from(value: ThermalControlProfilePointWire) -> Self {
        Self {
            target_temp_c: value.target_temp_c,
            brake_distance_centi_c: value.brake_distance_centi_c,
            warmup_power_permille: value.warmup_power_permille,
            approach_power_permille: value.approach_power_permille,
            approach_floor_power_permille: value.approach_floor_power_permille,
            approach_damping_exponent_permille: value.approach_damping_exponent_permille,
            approach_tail_window_centi_c: value.approach_tail_window_centi_c,
            hold_power_permille: value.hold_power_permille,
            hold_reheat_power_permille: value.hold_reheat_power_permille,
            hold_entry_centi_c: value.hold_entry_centi_c,
            hold_exit_centi_c: value.hold_exit_centi_c,
            hold_on_centi_c: value.hold_on_centi_c,
            hold_off_centi_c: value.hold_off_centi_c,
            overshoot_cutoff_centi_c: value.overshoot_cutoff_centi_c,
            hold_kp_permille_per_c: value.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: value.hold_ki_permille_per_c_tick,
            hold_blend_ticks: value.hold_blend_ticks,
            approach_lead_ticks: value.approach_lead_ticks,
            hold_lead_ticks: value.hold_lead_ticks,
        }
        .sanitized()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<ThermalControlProfilePointConfig> for ThermalControlProfilePoint {
    fn from(value: ThermalControlProfilePointConfig) -> Self {
        Self {
            target_temp_c: value.target_temp_c,
            brake_distance_centi_c: value.brake_distance_centi_c,
            warmup_power_permille: value.warmup_power_permille,
            approach_power_permille: value.approach_power_permille,
            approach_floor_power_permille: value.approach_floor_power_permille,
            approach_damping_exponent_permille: value.approach_damping_exponent_permille,
            approach_tail_window_centi_c: value.approach_tail_window_centi_c,
            hold_power_permille: value.hold_power_permille,
            hold_reheat_power_permille: value.hold_reheat_power_permille,
            hold_entry_centi_c: value.hold_entry_centi_c,
            hold_exit_centi_c: value.hold_exit_centi_c,
            hold_on_centi_c: value.hold_on_centi_c,
            hold_off_centi_c: value.hold_off_centi_c,
            overshoot_cutoff_centi_c: value.overshoot_cutoff_centi_c,
            hold_kp_permille_per_c: value.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: value.hold_ki_permille_per_c_tick,
            hold_blend_ticks: value.hold_blend_ticks,
            approach_lead_ticks: value.approach_lead_ticks,
            hold_lead_ticks: value.hold_lead_ticks,
        }
        .sanitized()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<ThermalControlProfileSettingsConfig> for ThermalControlProfileSettings {
    fn from(value: ThermalControlProfileSettingsConfig) -> Self {
        Self {
            temp_filter_alpha: f32::from(value.temp_filter_alpha_permille.clamp(1, 1_000))
                / 1_000.0,
            warmup_reenter_error_c: f32::from(value.warmup_reenter_centi_c.clamp(50, 5_000))
                / 100.0,
            hold_entry_error_c: f32::from(value.hold_entry_centi_c.clamp(1, 5_000)) / 100.0,
            hold_exit_error_c: f32::from(value.hold_exit_centi_c.clamp(1, 5_000)) / 100.0,
            hold_on_error_c: f32::from(value.hold_on_centi_c.clamp(1, 5_000)) / 100.0,
            hold_off_error_c: f32::from(value.hold_off_centi_c.min(5_000)) / 100.0,
            overshoot_cutoff_c: f32::from(value.overshoot_cutoff_centi_c.clamp(1, 5_000)) / 100.0,
            approach_max_ticks: value.approach_max_ticks.clamp(1, u16::from(u8::MAX)) as u8,
            approach_min_power_ratio: f32::from(value.approach_min_power_ratio_permille.min(1_000))
                / 1_000.0,
            hold_kp_permille_per_c: f32::from(value.hold_kp_permille_per_c.min(10_000)),
            hold_ki_permille_per_c_tick: f32::from(value.hold_ki_permille_per_c_tick.min(10_000)),
            hold_blend_ticks: value.hold_blend_ticks.clamp(1, u16::from(u8::MAX)) as u8,
            hold_reheat_power_permille: value.hold_reheat_power_permille.min(1_000),
            approach_lead_ticks: value.approach_lead_ticks.min(u16::from(u8::MAX)) as u8,
            hold_lead_ticks: value.hold_lead_ticks.min(u16::from(u8::MAX)) as u8,
            auto_adjustable_working_floor_mv: value.auto_adjustable_working_floor_mv.clamp(
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
                THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
            ),
            heater_current_reserve_ma: value
                .heater_current_reserve_ma
                .min(THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX),
        }
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
impl From<ThermalControlProfileSettingsWire> for ThermalControlProfileSettings {
    fn from(value: ThermalControlProfileSettingsWire) -> Self {
        ThermalControlProfileSettingsConfig {
            temp_filter_alpha_permille: value.temp_filter_alpha_permille,
            warmup_reenter_centi_c: value.warmup_reenter_centi_c,
            hold_entry_centi_c: value.hold_entry_centi_c,
            hold_exit_centi_c: value.hold_exit_centi_c,
            hold_on_centi_c: value.hold_on_centi_c,
            hold_off_centi_c: value.hold_off_centi_c,
            overshoot_cutoff_centi_c: value.overshoot_cutoff_centi_c,
            approach_max_ticks: value.approach_max_ticks,
            approach_min_power_ratio_permille: value.approach_min_power_ratio_permille,
            hold_kp_permille_per_c: value.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: value.hold_ki_permille_per_c_tick,
            hold_blend_ticks: value.hold_blend_ticks,
            hold_reheat_power_permille: value.hold_reheat_power_permille,
            approach_lead_ticks: value.approach_lead_ticks,
            hold_lead_ticks: value.hold_lead_ticks,
            auto_adjustable_working_floor_mv: value.auto_adjustable_working_floor_mv,
            heater_current_reserve_ma: value.heater_current_reserve_ma,
        }
        .into()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for ThermalControlProfileSettings {
    fn default() -> Self {
        ThermalControlProfileSettingsConfig::default().into()
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
impl From<ThermalControlProfileWire> for ThermalControlProfile {
    fn from(value: ThermalControlProfileWire) -> Self {
        let mut points = [None; FRONTPANEL_PRESET_COUNT];
        for (index, point) in value.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        Self {
            settings: value.settings.map(Into::into).unwrap_or_default(),
            points,
        }
        .sanitized()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<ThermalControlProfileConfig> for ThermalControlProfile {
    fn from(value: ThermalControlProfileConfig) -> Self {
        let mut points = [None; FRONTPANEL_PRESET_COUNT];
        for (index, point) in value.points.into_iter().enumerate() {
            points[index] = point.map(Into::into);
        }
        Self {
            settings: value.settings.into(),
            points,
        }
        .sanitized()
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl ThermalControlProfile {
    fn from_saved_config(config: &ThermalControlProfileConfig) -> Option<Self> {
        (config.points.iter().any(Option::is_some)
            || config.settings != ThermalControlProfileSettingsConfig::default())
        .then_some(Self::from(*config))
    }

    fn sanitized(mut self) -> Self {
        self.settings = ThermalControlProfileSettingsConfig {
            temp_filter_alpha_permille: (self.settings.temp_filter_alpha * 1_000.0) as u16,
            warmup_reenter_centi_c: (self.settings.warmup_reenter_error_c * 100.0) as u16,
            hold_entry_centi_c: (self.settings.hold_entry_error_c * 100.0) as u16,
            hold_exit_centi_c: (self.settings.hold_exit_error_c * 100.0) as u16,
            hold_on_centi_c: (self.settings.hold_on_error_c * 100.0) as u16,
            hold_off_centi_c: (self.settings.hold_off_error_c * 100.0) as u16,
            overshoot_cutoff_centi_c: (self.settings.overshoot_cutoff_c * 100.0) as u16,
            approach_max_ticks: u16::from(self.settings.approach_max_ticks),
            approach_min_power_ratio_permille: (self.settings.approach_min_power_ratio * 1_000.0)
                as u16,
            hold_kp_permille_per_c: self.settings.hold_kp_permille_per_c as u16,
            hold_ki_permille_per_c_tick: self.settings.hold_ki_permille_per_c_tick as u16,
            hold_blend_ticks: u16::from(self.settings.hold_blend_ticks),
            hold_reheat_power_permille: self.settings.hold_reheat_power_permille,
            approach_lead_ticks: u16::from(self.settings.approach_lead_ticks),
            hold_lead_ticks: u16::from(self.settings.hold_lead_ticks),
            auto_adjustable_working_floor_mv: self.settings.auto_adjustable_working_floor_mv,
            heater_current_reserve_ma: self.settings.heater_current_reserve_ma,
        }
        .into();
        for point in self.points.iter_mut().flatten() {
            let mut sanitized = point.sanitized();
            if sanitized.hold_entry_centi_c == 0 {
                sanitized.hold_entry_centi_c = (self.settings.hold_entry_error_c * 100.0) as u16;
            }
            if sanitized.hold_exit_centi_c == 0 {
                sanitized.hold_exit_centi_c = (self.settings.hold_exit_error_c * 100.0) as u16;
            }
            if sanitized.hold_on_centi_c == 0 {
                sanitized.hold_on_centi_c = (self.settings.hold_on_error_c * 100.0) as u16;
            }
            if sanitized.hold_off_centi_c == 0 {
                sanitized.hold_off_centi_c = (self.settings.hold_off_error_c * 100.0) as u16;
            }
            if sanitized.overshoot_cutoff_centi_c == 0 {
                sanitized.overshoot_cutoff_centi_c =
                    (self.settings.overshoot_cutoff_c * 100.0) as u16;
            }
            if sanitized.hold_kp_permille_per_c == 0 {
                sanitized.hold_kp_permille_per_c = self.settings.hold_kp_permille_per_c as u16;
            }
            if sanitized.hold_ki_permille_per_c_tick == 0 {
                sanitized.hold_ki_permille_per_c_tick =
                    self.settings.hold_ki_permille_per_c_tick as u16;
            }
            if sanitized.hold_blend_ticks == 0 {
                sanitized.hold_blend_ticks = u16::from(self.settings.hold_blend_ticks);
            }
            if sanitized.hold_reheat_power_permille == 0 {
                sanitized.hold_reheat_power_permille = self.settings.hold_reheat_power_permille;
            }
            if sanitized.hold_reheat_power_permille == 0 {
                sanitized.hold_reheat_power_permille = sanitized.hold_power_permille;
            }
            if sanitized.warmup_power_permille == 0 {
                sanitized.warmup_power_permille = sanitized.approach_power_permille;
            }
            if sanitized.approach_lead_ticks == 0 {
                sanitized.approach_lead_ticks = u16::from(self.settings.approach_lead_ticks);
            }
            if sanitized.hold_lead_ticks == 0 {
                sanitized.hold_lead_ticks = u16::from(self.settings.hold_lead_ticks);
            }
            *point = sanitized.sanitized();
        }
        self
    }

    fn control_target(self, target_temp_c: i16) -> ThermalControlTarget {
        let profile = self.sanitized();
        let mut dense: heapless::Vec<ThermalControlProfilePoint, FRONTPANEL_PRESET_COUNT> =
            heapless::Vec::new();
        for point in profile.points.into_iter().flatten() {
            let _ = dense.push(point.sanitized());
        }
        dense.sort_unstable_by_key(|point| point.target_temp_c);
        if dense.is_empty() {
            return default_thermal_control_target_with_settings(target_temp_c, profile.settings);
        }

        let target = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
        if target < dense[0].target_temp_c || target > dense[dense.len() - 1].target_temp_c {
            return default_thermal_control_target_with_settings(target, profile.settings);
        }

        let mut lower = dense[0];
        let mut upper = dense[dense.len() - 1];
        for point in dense.iter().copied() {
            if point.target_temp_c <= target {
                lower = point;
            }
            if point.target_temp_c >= target {
                upper = point;
                break;
            }
        }
        interpolate_thermal_control_target(target, lower, upper, profile.settings)
    }

    fn covers_target(self, target_temp_c: i16) -> bool {
        let profile = self.sanitized();
        let target = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
        let mut minimum = None::<i16>;
        let mut maximum = None::<i16>;
        for point in profile.points.into_iter().flatten() {
            minimum =
                Some(minimum.map_or(point.target_temp_c, |value| value.min(point.target_temp_c)));
            maximum =
                Some(maximum.map_or(point.target_temp_c, |value| value.max(point.target_temp_c)));
        }
        matches!((minimum, maximum), (Some(minimum), Some(maximum)) if target >= minimum && target <= maximum)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn active_thermal_control_profile(
    memory_config: &MemoryConfig,
    preview: Option<ThermalControlProfile>,
    pps_capability_min_mv: Option<u16>,
    pps_capability_max_mv: Option<u16>,
    pps_capability_max_ma: Option<u16>,
) -> Option<ThermalControlProfile> {
    preview.or_else(|| {
        ThermalControlProfile::from_saved_config(memory_config.thermal_profile(
            resolve_thermal_profile_bank(
                memory_config.thermal_profile_mode,
                pps_capability_min_mv,
                pps_capability_max_mv,
                pps_capability_max_ma,
            ),
        ))
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn resolve_thermal_profile_bank(
    mode: ThermalProfileMode,
    pps_capability_min_mv: Option<u16>,
    pps_capability_max_mv: Option<u16>,
    pps_capability_max_ma: Option<u16>,
) -> ThermalProfileBank {
    if mode == ThermalProfileMode::Auto
        && pps_capability_min_mv.is_some_and(|value| value <= 20_000)
        && pps_capability_max_mv.is_some_and(|value| value >= 20_000)
        && pps_capability_max_ma.is_some_and(|value| value >= 5_000)
    {
        ThermalProfileBank::Pps5a
    } else {
        mode.default_bank()
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn thermal_control_runtime_wire(
    target_temp_c: i16,
    profile: Option<ThermalControlProfile>,
    preview_active: bool,
) -> ThermalControlRuntimeWire {
    let profile_active = profile.is_some();
    let profile_covers_target = profile.is_some_and(|value| value.covers_target(target_temp_c));
    let target = profile
        .map(|value| value.control_target(target_temp_c))
        .unwrap_or_else(|| default_thermal_control_target(target_temp_c));
    let mut profile_source = heapless::String::new();
    let _ = profile_source.push_str(if preview_active {
        "preview"
    } else if profile_active {
        "saved"
    } else {
        "default"
    });
    ThermalControlRuntimeWire {
        profile_active,
        profile_covers_target,
        profile_source,
        target_temp_c,
        brake_distance_centi_c: round_to_u16_nonnegative(target.brake_distance_c * 100.0),
        warmup_power_permille: target.warmup_power_permille,
        approach_power_permille: target.approach_power_permille,
        approach_floor_power_permille: target.approach_floor_power_permille,
        approach_damping_exponent_permille: round_to_u16_nonnegative(
            target.approach_damping_exponent * 1_000.0,
        ),
        approach_tail_window_centi_c: round_to_u16_nonnegative(
            target.approach_tail_window_c * 100.0,
        ),
        hold_power_permille: target.hold_power_permille,
        hold_reheat_power_permille: target.hold_reheat_power_permille,
        hold_entry_centi_c: round_to_u16_nonnegative(target.hold_entry_error_c * 100.0),
        hold_exit_centi_c: round_to_u16_nonnegative(target.hold_exit_error_c * 100.0),
        hold_on_centi_c: round_to_u16_nonnegative(target.hold_on_error_c * 100.0),
        hold_off_centi_c: round_to_u16_nonnegative(target.hold_off_error_c * 100.0),
        overshoot_cutoff_centi_c: round_to_u16_nonnegative(target.overshoot_cutoff_c * 100.0),
        hold_kp_permille_per_c: round_to_u16_nonnegative(target.hold_kp_permille_per_c),
        hold_ki_permille_per_c_tick: round_to_u16_nonnegative(target.hold_ki_permille_per_c_tick),
        hold_blend_ticks: u16::from(target.hold_blend_ticks),
        approach_lead_ticks: u16::from(target.approach_lead_ticks),
        hold_lead_ticks: u16::from(target.hold_lead_ticks),
        temp_filter_alpha_permille: round_to_u16_nonnegative(
            target.settings.temp_filter_alpha * 1_000.0,
        ),
        warmup_reenter_centi_c: round_to_u16_nonnegative(
            target.settings.warmup_reenter_error_c * 100.0,
        ),
        approach_max_ticks: u16::from(target.settings.approach_max_ticks),
        approach_min_power_ratio_permille: round_to_u16_nonnegative(
            target.settings.approach_min_power_ratio * 1_000.0,
        ),
        auto_adjustable_working_floor_mv: target.settings.auto_adjustable_working_floor_mv,
        heater_current_reserve_ma: target.settings.heater_current_reserve_ma,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn default_thermal_control_target(target_temp_c: i16) -> ThermalControlTarget {
    default_thermal_control_target_with_settings(
        target_temp_c,
        ThermalControlProfileSettings::default(),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn default_thermal_control_target_with_settings(
    target_temp_c: i16,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    let target = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
    let brake_distance_centi_c = if target <= 100 {
        450
    } else if target <= 180 {
        700
    } else if target <= 250 {
        1_000
    } else {
        1_400
    };
    let approach_power_permille = if target <= 100 {
        380
    } else if target <= 180 {
        320
    } else if target <= 250 {
        260
    } else {
        220
    };
    let warmup_power_permille = if target <= 60 {
        320
    } else if target <= 140 {
        approach_power_permille
    } else if target <= 220 {
        approach_power_permille.max(640)
    } else {
        approach_power_permille.max(920)
    };
    let approach_floor_power_permille = if target <= 100 {
        120
    } else if target <= 180 {
        200
    } else if target <= 250 {
        320
    } else {
        380
    };
    let hold_power_permille = if target <= 100 {
        180
    } else if target <= 180 {
        220
    } else if target <= 250 {
        260
    } else {
        300
    };
    let approach_damping_exponent_permille: u16 = if target <= 100 {
        1_400
    } else if target <= 140 {
        1_000
    } else if target <= 180 {
        800
    } else if target <= 220 {
        550
    } else {
        350
    };
    ThermalControlTarget {
        brake_distance_c: brake_distance_centi_c as f32 / 100.0,
        warmup_power_permille,
        approach_power_permille,
        approach_floor_power_permille,
        approach_damping_exponent: f32::from(approach_damping_exponent_permille) / 1_000.0,
        approach_tail_window_c: 0.0,
        hold_power_permille,
        hold_entry_error_c: if target <= 60 {
            0.35
        } else if target <= 100 {
            0.25
        } else if target <= 140 {
            0.20
        } else if target <= 180 {
            0.18
        } else if target <= 220 {
            0.15
        } else {
            0.12
        },
        hold_exit_error_c: if target <= 60 {
            1.6
        } else if target <= 100 {
            1.2
        } else if target <= 140 {
            1.0
        } else if target <= 180 {
            0.9
        } else if target <= 220 {
            0.8
        } else {
            0.7
        },
        hold_off_error_c: if target <= 60 {
            0.7
        } else if target <= 100 {
            0.8
        } else if target <= 140 {
            0.9
        } else if target <= 180 {
            1.0
        } else if target <= 220 {
            1.2
        } else {
            1.5
        },
        overshoot_cutoff_c: if target <= 60 {
            0.8
        } else if target <= 100 {
            0.9
        } else if target <= 140 {
            1.0
        } else if target <= 180 {
            1.2
        } else if target <= 220 {
            1.4
        } else {
            1.6
        },
        hold_kp_permille_per_c: if target <= 60 {
            70.0
        } else if target <= 100 {
            55.0
        } else if target <= 140 {
            42.0
        } else if target <= 180 {
            30.0
        } else if target <= 220 {
            22.0
        } else {
            18.0
        },
        hold_ki_permille_per_c_tick: if target <= 60 {
            3.0
        } else if target <= 100 {
            2.0
        } else if target <= 140 {
            1.5
        } else {
            1.0
        },
        hold_blend_ticks: if target <= 100 {
            16
        } else if target <= 180 {
            12
        } else {
            8
        },
        hold_reheat_power_permille: hold_power_permille,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
        hold_on_error_c: settings.hold_on_error_c,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn interpolate_thermal_control_target(
    target_temp_c: i16,
    lower: ThermalControlProfilePoint,
    upper: ThermalControlProfilePoint,
    settings: ThermalControlProfileSettings,
) -> ThermalControlTarget {
    if lower.target_temp_c >= upper.target_temp_c {
        return ThermalControlTarget {
            brake_distance_c: lower.brake_distance_centi_c as f32 / 100.0,
            warmup_power_permille: lower.warmup_power_permille,
            approach_power_permille: lower.approach_power_permille,
            approach_floor_power_permille: lower.approach_floor_power_permille,
            approach_damping_exponent: f32::from(lower.approach_damping_exponent_permille)
                / 1_000.0,
            approach_tail_window_c: f32::from(lower.approach_tail_window_centi_c) / 100.0,
            hold_power_permille: lower.hold_power_permille,
            hold_reheat_power_permille: lower.hold_reheat_power_permille,
            hold_entry_error_c: f32::from(lower.hold_entry_centi_c) / 100.0,
            hold_exit_error_c: f32::from(lower.hold_exit_centi_c) / 100.0,
            hold_on_error_c: f32::from(lower.hold_on_centi_c) / 100.0,
            hold_off_error_c: f32::from(lower.hold_off_centi_c) / 100.0,
            overshoot_cutoff_c: f32::from(lower.overshoot_cutoff_centi_c) / 100.0,
            hold_kp_permille_per_c: f32::from(lower.hold_kp_permille_per_c),
            hold_ki_permille_per_c_tick: f32::from(lower.hold_ki_permille_per_c_tick),
            hold_blend_ticks: lower.hold_blend_ticks.clamp(1, u16::from(u8::MAX)) as u8,
            approach_lead_ticks: lower.approach_lead_ticks.min(u16::from(u8::MAX)) as u8,
            hold_lead_ticks: lower.hold_lead_ticks.min(u16::from(u8::MAX)) as u8,
            settings,
        };
    }

    let span = f32::from(upper.target_temp_c - lower.target_temp_c);
    let ratio = (f32::from(target_temp_c - lower.target_temp_c) / span).clamp(0.0, 1.0);
    let lerp_u16 = |left: u16, right: u16, upper_bound: u16| -> u16 {
        (f32::from(left) + ((f32::from(right) - f32::from(left)) * ratio) + 0.5)
            .clamp(0.0, f32::from(upper_bound)) as u16
    };
    let linear_brake_distance = lerp_u16(
        lower.brake_distance_centi_c,
        upper.brake_distance_centi_c,
        5_000,
    );
    let midpoint_weight = 4.0 * ratio * (1.0 - ratio);
    let intermediate_brake_adjustment = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        -0.20
    } else if lower.target_temp_c >= 100 && upper.target_temp_c <= 180 {
        if upper.target_temp_c <= 140 {
            0.55
        } else {
            0.20
        }
    } else {
        0.0
    };
    let interpolated_brake_distance = (f32::from(linear_brake_distance)
        * (1.0 - intermediate_brake_adjustment * midpoint_weight)
        + 0.5) as u16;
    let low_temp_hold_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.20 * midpoint_weight)
    } else {
        1.0
    };
    let low_temp_reheat_scale = if lower.target_temp_c >= 60 && upper.target_temp_c <= 100 {
        1.0 - (0.10 * midpoint_weight)
    } else {
        1.0
    };
    let scale_low_temp_hold =
        |value: u16| (f32::from(value) * low_temp_hold_scale + 0.5).clamp(0.0, 1_000.0) as u16;
    ThermalControlTarget {
        brake_distance_c: f32::from(interpolated_brake_distance) / 100.0,
        warmup_power_permille: lerp_u16(
            lower.warmup_power_permille,
            upper.warmup_power_permille,
            1_000,
        ),
        approach_power_permille: lerp_u16(
            lower.approach_power_permille,
            upper.approach_power_permille,
            1_000,
        ),
        approach_floor_power_permille: lerp_u16(
            lower.approach_floor_power_permille,
            upper.approach_floor_power_permille,
            1_000,
        ),
        approach_damping_exponent: f32::from(lerp_u16(
            lower.approach_damping_exponent_permille,
            upper.approach_damping_exponent_permille,
            THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
        )) / 1_000.0,
        approach_tail_window_c: f32::from(lerp_u16(
            lower.approach_tail_window_centi_c,
            upper.approach_tail_window_centi_c,
            THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
        )) / 100.0,
        hold_power_permille: scale_low_temp_hold(lerp_u16(
            lower.hold_power_permille,
            upper.hold_power_permille,
            1_000,
        )),
        hold_reheat_power_permille: (f32::from(lerp_u16(
            lower.hold_reheat_power_permille,
            upper.hold_reheat_power_permille,
            1_000,
        )) * low_temp_reheat_scale
            + 0.5) as u16,
        hold_entry_error_c: f32::from(lerp_u16(
            lower.hold_entry_centi_c,
            upper.hold_entry_centi_c,
            5_000,
        )) / 100.0,
        hold_exit_error_c: f32::from(lerp_u16(
            lower.hold_exit_centi_c,
            upper.hold_exit_centi_c,
            5_000,
        )) / 100.0,
        hold_on_error_c: f32::from(lerp_u16(
            lower.hold_on_centi_c,
            upper.hold_on_centi_c,
            5_000,
        )) / 100.0,
        hold_off_error_c: f32::from(lerp_u16(
            lower.hold_off_centi_c,
            upper.hold_off_centi_c,
            5_000,
        )) / 100.0,
        overshoot_cutoff_c: f32::from(lerp_u16(
            lower.overshoot_cutoff_centi_c,
            upper.overshoot_cutoff_centi_c,
            5_000,
        )) / 100.0,
        hold_kp_permille_per_c: f32::from(lerp_u16(
            lower.hold_kp_permille_per_c,
            upper.hold_kp_permille_per_c,
            10_000,
        )),
        hold_ki_permille_per_c_tick: f32::from(lerp_u16(
            lower.hold_ki_permille_per_c_tick,
            upper.hold_ki_permille_per_c_tick,
            10_000,
        )),
        hold_blend_ticks: lerp_u16(
            lower.hold_blend_ticks,
            upper.hold_blend_ticks,
            u16::from(u8::MAX),
        )
        .clamp(1, u16::from(u8::MAX)) as u8,
        approach_lead_ticks: lerp_u16(
            lower.approach_lead_ticks,
            upper.approach_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
        hold_lead_ticks: lerp_u16(
            lower.hold_lead_ticks,
            upper.hold_lead_ticks,
            u16::from(u8::MAX),
        )
        .min(u16::from(u8::MAX)) as u8,
        settings,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn percent_from_permille(permille: u16) -> u8 {
    ((u32::from(permille.min(1_000)) + 5) / 10).min(100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
fn control_cycles_from_profile_ticks(profile_ticks: u16) -> u16 {
    if profile_ticks == 0 {
        return 0;
    }
    let numerator = u32::from(profile_ticks) * HEATER_PROFILE_TICK_MS as u32;
    let denominator = HEATER_CONTROL_INTERVAL_MS as u32;
    numerator.div_ceil(denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
fn scaled_filter_alpha_for_control_interval(alpha_per_profile_tick: f32) -> f32 {
    let alpha = alpha_per_profile_tick.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return 0.0;
    }
    if alpha >= 1.0 {
        return 1.0;
    }
    let profile_fraction = HEATER_CONTROL_INTERVAL_MS as f32 / HEATER_PROFILE_TICK_MS as f32;
    (1.0 - (1.0 - alpha).powf(profile_fraction)).clamp(0.0, 1.0)
}

#[cfg(any(target_arch = "xtensa", test))]
fn scaled_hold_ki_for_control_interval(ki_per_profile_tick: f32) -> f32 {
    ki_per_profile_tick.max(0.0)
        * (HEATER_CONTROL_INTERVAL_MS as f32 / HEATER_PROFILE_TICK_MS as f32)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_control_poll_wait_ms(elapsed_ms: u64, next_deadline_ms: u64) -> u64 {
    next_deadline_ms
        .saturating_sub(elapsed_ms)
        .clamp(1, RUNTIME_INPUT_POLL_MAX_INTERVAL_MS)
}

#[cfg(any(target_arch = "xtensa", test))]
fn next_heater_control_deadline_ms(deadline_ms: u64, control_started_ms: u64) -> u64 {
    let next_deadline_ms = deadline_ms.saturating_add(HEATER_CONTROL_INTERVAL_MS);
    if next_deadline_ms > control_started_ms {
        return next_deadline_ms;
    }

    let missed_intervals = control_started_ms
        .saturating_sub(next_deadline_ms)
        .saturating_div(HEATER_CONTROL_INTERVAL_MS)
        .saturating_add(1);
    next_deadline_ms.saturating_add(missed_intervals.saturating_mul(HEATER_CONTROL_INTERVAL_MS))
}

#[cfg(any(target_arch = "xtensa", test))]
fn warmup_handoff_error_c(
    brake_distance_c: f32,
    warmup_reenter_error_c: f32,
    filtered_slope_c_per_profile_tick: f32,
    approach_lead_ticks: u8,
) -> f32 {
    let predictive_distance_c =
        filtered_slope_c_per_profile_tick.max(0.0) * f32::from(approach_lead_ticks);
    let reentry_margin_c = (warmup_reenter_error_c - HEATER_HOLD_PHASE_HYSTERESIS_C).max(0.0);
    predictive_distance_c
        .max(brake_distance_c)
        .min(brake_distance_c + reentry_margin_c)
}

#[cfg(any(target_arch = "xtensa", test))]
fn warmup_handoff_ready(
    actual_error_c: f32,
    filtered_error_c: f32,
    brake_distance_c: f32,
    handoff_error_c: f32,
) -> bool {
    actual_error_c <= brake_distance_c || actual_error_c.max(filtered_error_c) <= handoff_error_c
}

#[cfg(any(target_arch = "xtensa", test))]
fn hold_effective_base_permille(
    hold_guard_error_c: f32,
    hold_reenter_error_c: f32,
    control_target: ThermalControlTarget,
) -> f32 {
    let hold_power_permille = f32::from(control_target.hold_power_permille.min(1_000));
    let hold_reheat_permille = f32::from(
        control_target
            .hold_reheat_power_permille
            .max(control_target.hold_power_permille)
            .min(1_000),
    );
    if hold_guard_error_c <= 0.0 || hold_reheat_permille <= hold_power_permille {
        return hold_power_permille;
    }

    let ratio = (hold_guard_error_c / hold_reenter_error_c.max(0.05)).clamp(0.0, 1.0);
    hold_power_permille + ((hold_reheat_permille - hold_power_permille) * ratio)
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct BuzzerHardwareState {
    frequency_hz: Option<u32>,
    duty_percent: u8,
    generation: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterController {
    fault_latched: Option<HeaterFaultReason>,
    last_target_temp_c: i16,
    filtered_temp_c: Option<f32>,
    previous_filtered_temp_c: Option<f32>,
    filtered_slope_c_per_profile_tick: f32,
    previous_measured_temp_c: Option<f32>,
    phase: HeaterControlPhase,
    phase_ticks: u16,
    recovering_from_hold: bool,
    duty_percent: u8,
    hold_entry_output_percent: u8,
    hold_integral_c: f32,
    hold_coast_active: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterController {
    const fn new() -> Self {
        Self {
            fault_latched: None,
            last_target_temp_c: 0,
            filtered_temp_c: None,
            previous_filtered_temp_c: None,
            filtered_slope_c_per_profile_tick: 0.0,
            previous_measured_temp_c: None,
            phase: HeaterControlPhase::Warmup,
            phase_ticks: 0,
            recovering_from_hold: false,
            duty_percent: 0,
            hold_entry_output_percent: 0,
            hold_integral_c: 0.0,
            hold_coast_active: false,
        }
    }

    const fn fault_latched(self) -> Option<HeaterFaultReason> {
        self.fault_latched
    }

    fn clear_fault_latch(&mut self) {
        self.fault_latched = None;
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
    }

    fn latch_fault(&mut self, reason: HeaterFaultReason) -> bool {
        let changed = self.fault_latched != Some(reason);
        self.fault_latched = Some(reason);
        self.filtered_temp_c = None;
        self.previous_filtered_temp_c = None;
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.recovering_from_hold = false;
        self.duty_percent = 0;
        self.hold_entry_output_percent = 0;
        self.hold_integral_c = 0.0;
        self.hold_coast_active = false;
        changed
    }

    fn update(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
        thermal_profile: Option<ThermalControlProfile>,
    ) -> HeaterPidSnapshot {
        let target_temp_c = target_temp_c.clamp(HEATER_PID_TARGET_MIN_C, HEATER_PID_TARGET_MAX_C);
        let last_target_temp_c = self.last_target_temp_c;
        self.last_target_temp_c = target_temp_c;
        let previous_measured_temp_c = self
            .previous_measured_temp_c
            .or(self.filtered_temp_c)
            .unwrap_or(measured_temp_c);
        self.previous_measured_temp_c = Some(measured_temp_c);

        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
        }

        if !heater_enabled || self.fault_latched.is_some() {
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
            self.filtered_slope_c_per_profile_tick = 0.0;
            self.previous_measured_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.recovering_from_hold = false;
            self.duty_percent = 0;
            self.hold_entry_output_percent = 0;
            self.hold_integral_c = 0.0;
            self.hold_coast_active = false;
            return HeaterPidSnapshot {
                duty_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                control_error_c: f32::from(target_temp_c) - measured_temp_c,
                filtered_temp_c: measured_temp_c,
                filtered_slope_c_per_s: 0.0,
                coast_active: false,
                phase: self.phase,
            };
        }

        if target_temp_c != last_target_temp_c {
            self.filtered_temp_c = Some(measured_temp_c);
            self.previous_filtered_temp_c = Some(measured_temp_c);
            self.filtered_slope_c_per_profile_tick = 0.0;
            self.previous_measured_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.recovering_from_hold = false;
            self.duty_percent = 0;
            self.hold_entry_output_percent = 0;
            self.hold_integral_c = 0.0;
            self.hold_coast_active = false;
        }

        let control_target = thermal_profile
            .map(|profile| profile.control_target(target_temp_c))
            .unwrap_or_else(|| default_thermal_control_target(target_temp_c));
        let settings = control_target.settings;
        let filter_alpha = scaled_filter_alpha_for_control_interval(settings.temp_filter_alpha);
        let approach_max_cycles =
            control_cycles_from_profile_ticks(u16::from(settings.approach_max_ticks)).max(1);
        let hold_blend_cycles =
            control_cycles_from_profile_ticks(u16::from(control_target.hold_blend_ticks)).max(1);
        let hold_ki =
            scaled_hold_ki_for_control_interval(control_target.hold_ki_permille_per_c_tick);
        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let previous_error_c = f32::from(target_temp_c) - previous_measured_temp_c;
        let last_filtered_temp_c = self.filtered_temp_c;
        let filtered_temp_c = if let Some(previous_filtered_temp_c) = last_filtered_temp_c {
            previous_filtered_temp_c + filter_alpha * (measured_temp_c - previous_filtered_temp_c)
        } else {
            measured_temp_c
        };
        let instantaneous_slope_c_per_profile_tick = last_filtered_temp_c
            .map(|last| {
                (filtered_temp_c - last)
                    * (HEATER_PROFILE_TICK_MS as f32 / HEATER_CONTROL_INTERVAL_MS as f32)
            })
            .unwrap_or(0.0);
        let slope_filter_alpha = filter_alpha.sqrt();
        self.filtered_slope_c_per_profile_tick += slope_filter_alpha
            * (instantaneous_slope_c_per_profile_tick - self.filtered_slope_c_per_profile_tick);
        let filtered_temp_slope_c_per_profile_tick = self.filtered_slope_c_per_profile_tick;
        self.previous_filtered_temp_c = last_filtered_temp_c;
        self.filtered_temp_c = Some(filtered_temp_c);
        let control_error_c = f32::from(target_temp_c) - filtered_temp_c;
        let approach_projected_temp_c = filtered_temp_c
            + (filtered_temp_slope_c_per_profile_tick
                * f32::from(control_target.approach_lead_ticks));
        let hold_projected_temp_c = filtered_temp_c
            + (filtered_temp_slope_c_per_profile_tick * f32::from(control_target.hold_lead_ticks));
        let approach_control_error_c = f32::from(target_temp_c) - approach_projected_temp_c;
        let hold_control_error_c = f32::from(target_temp_c) - hold_projected_temp_c;
        let hold_prediction_blocks_reheat = error_c > 0.0
            && filtered_temp_slope_c_per_profile_tick > 0.0
            && hold_control_error_c <= 0.0;
        let approach_guard_error_c = approach_control_error_c.min(error_c);
        let hold_entry_gate_c = control_target.hold_entry_error_c.max(0.05);
        let hold_entry_measurement_margin_c = 0.5;
        let hold_state_ready = control_error_c <= control_target.hold_exit_error_c;
        let actual_crossed_target_ready =
            error_c <= 0.0 && previous_error_c <= 0.0 && approach_control_error_c <= 0.0;
        let hold_guard_error_c = if error_c >= 0.0 {
            let filter_lag_allowance_c = control_target
                .hold_on_error_c
                .max(0.05)
                .min(hold_entry_gate_c);
            hold_control_error_c
                .max(0.0)
                .min(error_c + filter_lag_allowance_c)
        } else {
            // Above target, predictive lead was cutting hold power too aggressively and
            // creating wide bang-bang cycles. Keep actual-temperature guard on the
            // overshoot side and reserve predictive lead for under-target recovery.
            error_c
        };
        let hold_reenter_error_c = control_target
            .hold_exit_error_c
            .max(control_target.hold_on_error_c)
            .max(hold_entry_gate_c + HEATER_HOLD_PHASE_HYSTERESIS_C);
        // A projected crossing alone is not enough to coast. The physical sensor and the
        // filtered controller state must both be inside the profile's Hold exit gate first.
        // Otherwise a high rising slope can cut heating several degrees below target.
        let approach_predictive_coast_ready = approach_control_error_c <= 0.0
            && error_c <= control_target.hold_exit_error_c
            && control_error_c <= control_target.hold_exit_error_c;
        // Phase residency follows the actual plate temperature. Filtered and projected errors
        // shape power, but using their lag to leave Hold creates rapid Hold/Approach oscillation.
        let hold_exit_error_c = error_c;
        let brake_distance_c = control_target
            .brake_distance_c
            .max(control_target.hold_entry_error_c + 0.1);
        let warmup_handoff_error_c = warmup_handoff_error_c(
            brake_distance_c,
            settings.warmup_reenter_error_c,
            filtered_temp_slope_c_per_profile_tick,
            control_target.approach_lead_ticks,
        );

        let mut next_phase = self.phase;
        let previous_phase = self.phase;
        match self.phase {
            HeaterControlPhase::Warmup => {
                if warmup_handoff_ready(
                    error_c,
                    control_error_c,
                    brake_distance_c,
                    warmup_handoff_error_c,
                ) {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
            HeaterControlPhase::Approach => {
                let timeout_hold_ready = self.phase_ticks >= approach_max_cycles
                    && error_c <= hold_entry_gate_c
                    && hold_state_ready;
                // Re-enter warmup only when the actual plate reading is well below the brake
                // boundary. Filter lag after a rising sample must not undo a deliberate brake.
                if error_c >= brake_distance_c + settings.warmup_reenter_error_c {
                    next_phase = HeaterControlPhase::Warmup;
                } else if (approach_control_error_c <= hold_entry_gate_c
                    && error_c <= hold_entry_gate_c
                    && previous_error_c <= hold_entry_gate_c + hold_entry_measurement_margin_c)
                    || actual_crossed_target_ready
                    || timeout_hold_ready
                {
                    next_phase = HeaterControlPhase::Hold;
                }
            }
            HeaterControlPhase::Hold => {
                if hold_exit_error_c >= hold_reenter_error_c
                    && previous_error_c >= hold_reenter_error_c
                {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
        }

        if next_phase != self.phase {
            self.phase = next_phase;
            self.phase_ticks = 0;
            self.recovering_from_hold = previous_phase == HeaterControlPhase::Hold
                && self.phase == HeaterControlPhase::Approach;
        } else {
            self.phase_ticks = self.phase_ticks.saturating_add(1);
            if self.phase != HeaterControlPhase::Approach {
                self.recovering_from_hold = false;
            }
        }

        if previous_phase != self.phase {
            if self.phase == HeaterControlPhase::Hold {
                self.hold_coast_active = filtered_temp_slope_c_per_profile_tick > 0.0
                    && (self.duty_percent == 0
                        || approach_control_error_c <= 0.0
                        || hold_control_error_c <= 0.0);
                if actual_crossed_target_ready {
                    self.filtered_temp_c = Some(measured_temp_c);
                    self.previous_filtered_temp_c = Some(measured_temp_c);
                }
                let hold_entry_guard_error_c = if error_c > 0.0 {
                    hold_guard_error_c
                } else {
                    error_c
                };
                let hold_base_permille = hold_effective_base_permille(
                    hold_entry_guard_error_c,
                    hold_reenter_error_c,
                    control_target,
                );
                let positive_integral_limit = if hold_ki > 0.0 {
                    ((1_000.0 - hold_base_permille) / hold_ki).clamp(0.0, 255.0)
                } else {
                    0.0
                };
                let previous_output_permille = f32::from(self.duty_percent.min(100)) * 10.0;
                let hold_entry_base_permille = hold_base_permille
                    + (control_target.hold_kp_permille_per_c * hold_entry_guard_error_c);
                let actual_preload_ratio = if error_c > 0.0 {
                    (error_c / hold_entry_gate_c).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let projected_preload_ratio = if approach_guard_error_c > 0.0 {
                    (approach_guard_error_c / hold_entry_gate_c).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let preload_ratio = actual_preload_ratio.min(projected_preload_ratio);
                let carry_permille = (previous_output_permille - hold_entry_base_permille).max(0.0);
                let hold_entry_output_permille =
                    hold_entry_base_permille + (carry_permille * preload_ratio);
                self.hold_entry_output_percent =
                    percent_from_permille(hold_entry_output_permille.clamp(0.0, 1_000.0) as u16);
                self.hold_integral_c = if hold_ki > 0.0 {
                    ((carry_permille * preload_ratio) / hold_ki).clamp(0.0, positive_integral_limit)
                } else {
                    0.0
                };
            } else {
                self.hold_coast_active = false;
                self.hold_entry_output_percent = 0;
                if self.phase != HeaterControlPhase::Hold {
                    self.hold_integral_c = 0.0;
                }
            }
        }
        if self.hold_coast_active
            && filtered_temp_slope_c_per_profile_tick <= -0.02
            && measured_temp_c + 0.05 < previous_measured_temp_c
            && error_c >= control_target.hold_on_error_c.max(0.05)
            && control_error_c >= control_target.hold_on_error_c.max(0.05)
        {
            self.hold_coast_active = false;
            self.phase_ticks = 0;
            self.hold_entry_output_percent = 0;
        }
        let duty_percent = if self.hold_coast_active
            || hold_guard_error_c <= -control_target.overshoot_cutoff_c
        {
            self.hold_integral_c = 0.0;
            0
        } else {
            match self.phase {
                HeaterControlPhase::Warmup => {
                    percent_from_permille(control_target.warmup_power_permille.min(1_000))
                }
                HeaterControlPhase::Approach => {
                    if approach_predictive_coast_ready {
                        0
                    } else {
                        let span = (brake_distance_c - control_target.hold_entry_error_c).max(0.1);
                        let ratio = ((approach_guard_error_c - control_target.hold_entry_error_c)
                            / span)
                            .clamp(0.0, 1.0);
                        let shaped_ratio = ratio.powf(control_target.approach_damping_exponent);
                        let sustain_floor =
                            f32::from(approach_sustain_floor_permille(control_target, error_c));
                        let approach_ceiling =
                            f32::from(control_target.approach_power_permille.min(1_000))
                                .max(sustain_floor);
                        let requested_permille =
                            sustain_floor + ((approach_ceiling - sustain_floor) * shaped_ratio);
                        percent_from_permille(requested_permille.clamp(0.0, 1_000.0) as u16)
                    }
                }
                HeaterControlPhase::Hold => {
                    if hold_prediction_blocks_reheat {
                        self.hold_integral_c = 0.0;
                        0
                    } else {
                        let hold_base_permille = hold_effective_base_permille(
                            hold_guard_error_c,
                            hold_reenter_error_c,
                            control_target,
                        );
                        let positive_integral_limit = if hold_ki > 0.0 {
                            ((1_000.0 - hold_base_permille) / hold_ki).clamp(0.0, 255.0)
                        } else {
                            0.0
                        };
                        self.hold_integral_c = if hold_ki > 0.0 {
                            // The integral term only represents missing equilibrium heat. Letting it
                            // go negative turns a brief overshoot into a long zero-output valley.
                            (self.hold_integral_c + hold_guard_error_c)
                                .clamp(0.0, positive_integral_limit)
                        } else {
                            0.0
                        };
                        let mut requested_permille = hold_base_permille
                            + (control_target.hold_kp_permille_per_c * hold_guard_error_c)
                            + (hold_ki * self.hold_integral_c);
                        if hold_guard_error_c <= -control_target.hold_off_error_c {
                            self.hold_integral_c = 0.0;
                            let taper_span = (control_target.overshoot_cutoff_c
                                - control_target.hold_off_error_c)
                                .max(0.05);
                            let overshoot_c =
                                (-hold_guard_error_c).max(control_target.hold_off_error_c);
                            let taper_ratio = ((control_target.overshoot_cutoff_c - overshoot_c)
                                / taper_span)
                                .clamp(0.0, 1.0);
                            requested_permille =
                                requested_permille.clamp(0.0, 1_000.0) * taper_ratio;
                        }
                        let pi_percent =
                            percent_from_permille(requested_permille.clamp(0.0, 1_000.0) as u16);
                        if hold_guard_error_c > 0.0 && self.phase_ticks < hold_blend_cycles {
                            let blend_ratio = f32::from(self.phase_ticks.saturating_add(1))
                                / f32::from(hold_blend_cycles.max(1));
                            ((f32::from(self.hold_entry_output_percent)
                                + ((f32::from(pi_percent)
                                    - f32::from(self.hold_entry_output_percent))
                                    * blend_ratio))
                                .clamp(0.0, 100.0)
                                + 0.5) as u8
                        } else {
                            pi_percent
                        }
                    }
                }
            }
        };
        let predictive_coast_active = self.hold_coast_active
            || (self.phase == HeaterControlPhase::Approach && approach_predictive_coast_ready);
        let duty_percent = if predictive_coast_active {
            duty_percent
        } else {
            self.apply_under_target_reheat_floor(
                duty_percent,
                error_c,
                hold_control_error_c,
                control_target,
            )
        };

        self.duty_percent = duty_percent;

        HeaterPidSnapshot {
            duty_percent,
            error_c,
            control_error_c,
            filtered_temp_c,
            filtered_slope_c_per_s: filtered_temp_slope_c_per_profile_tick,
            coast_active: self.hold_coast_active,
            phase: self.phase,
        }
    }

    fn apply_under_target_reheat_floor(
        &self,
        duty_percent: u8,
        error_c: f32,
        hold_control_error_c: f32,
        control_target: ThermalControlTarget,
    ) -> u8 {
        if duty_percent != 0 || error_c <= 0.0 {
            return duty_percent;
        }

        let floor_permille = match self.phase {
            HeaterControlPhase::Warmup => return duty_percent,
            HeaterControlPhase::Approach => {
                approach_sustain_floor_permille(control_target, error_c)
            }
            HeaterControlPhase::Hold => {
                if hold_control_error_c <= 0.0 {
                    return duty_percent;
                }
                control_target
                    .hold_reheat_power_permille
                    .max(control_target.hold_power_permille)
            }
        };
        percent_from_permille(floor_permille.min(1_000))
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn approach_sustain_floor_permille(control_target: ThermalControlTarget, error_c: f32) -> u16 {
    let full_floor = control_target
        .approach_floor_power_permille
        .max(control_target.hold_reheat_power_permille)
        .max(control_target.hold_power_permille)
        .min(1_000);
    let tail_floor = control_target
        .hold_reheat_power_permille
        .max(control_target.hold_power_permille)
        .min(full_floor);
    let tail_window_c = control_target.approach_tail_window_c.max(0.0);
    if tail_window_c <= f32::EPSILON || full_floor == tail_floor {
        return full_floor;
    }

    let tail_progress =
        ((error_c - control_target.hold_entry_error_c) / tail_window_c).clamp(0.0, 1.0);
    (f32::from(tail_floor)
        + ((f32::from(full_floor) - f32::from(tail_floor)) * tail_progress)
        + 0.5) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FanVoltageProfile {
    Minimum,
    SafeHalf,
    Full,
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanVoltageProfile {
    const fn pwm_permille(self) -> u16 {
        match self {
            Self::Minimum => FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
            Self::SafeHalf => FAN_HALF_SPEED_PWM_PERMILLE,
            Self::Full => FAN_FULL_SPEED_PWM_PERMILLE,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FanHardwareCommand {
    enabled: bool,
    pwm_permille: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanHardwareCommand {
    const fn disabled() -> Self {
        Self {
            enabled: false,
            pwm_permille: FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
        }
    }

    const fn from_profile(profile: FanVoltageProfile) -> Self {
        Self {
            enabled: true,
            pwm_permille: profile.pwm_permille(),
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FanPolicyState {
    Disabled,
    ActiveCooling,
    SafeHalf,
    Full,
    ActiveCoolingCooldown { until_ms: u64 },
    CoolingDisabledPulse { duty_percent: u8 },
}

#[cfg(any(target_arch = "xtensa", test))]
impl FanPolicyState {
    const fn command(self, elapsed_ms: u64) -> FanHardwareCommand {
        match self {
            Self::Disabled => FanHardwareCommand::disabled(),
            Self::ActiveCooling => FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            },
            Self::SafeHalf => FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf),
            Self::Full => FanHardwareCommand::from_profile(FanVoltageProfile::Full),
            Self::ActiveCoolingCooldown { until_ms } => {
                if elapsed_ms < until_ms {
                    FanHardwareCommand::from_profile(FanVoltageProfile::Minimum)
                } else {
                    FanHardwareCommand::disabled()
                }
            }
            Self::CoolingDisabledPulse { duty_percent } => {
                if duty_percent == 0 {
                    return FanHardwareCommand::disabled();
                }

                let elapsed_in_period_ms = elapsed_ms % FAN_PULSE_PERIOD_MS;
                let on_window_ms = FAN_PULSE_PERIOD_MS.saturating_mul(duty_percent as u64) / 100;
                FanHardwareCommand {
                    enabled: elapsed_in_period_ms < on_window_ms,
                    pwm_permille: FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
                }
            }
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FanPolicyDecision {
    state: FanPolicyState,
    command: FanHardwareCommand,
    display_state: FanDisplayState,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn is_sensor_fault(reason: Option<HeaterFaultReason>) -> bool {
    matches!(
        reason,
        Some(
            HeaterFaultReason::SensorShort
                | HeaterFaultReason::SensorOpen
                | HeaterFaultReason::AdcReadFailed
        )
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn auto_cooling_command(
    current_temp_c: i16,
    elapsed_ms: u64,
    previous_state: FanPolicyState,
) -> FanPolicyState {
    if current_temp_c > AUTO_COOLING_FAN_FULL_TEMP_C {
        FanPolicyState::Full
    } else if current_temp_c >= AUTO_COOLING_FAN_MIN_TEMP_C {
        FanPolicyState::ActiveCooling
    } else {
        match previous_state {
            FanPolicyState::Full | FanPolicyState::ActiveCooling => {
                FanPolicyState::ActiveCoolingCooldown {
                    until_ms: elapsed_ms.saturating_add(AUTO_COOLING_FAN_COOLDOWN_MS),
                }
            }
            FanPolicyState::ActiveCoolingCooldown { until_ms } if elapsed_ms < until_ms => {
                FanPolicyState::ActiveCoolingCooldown { until_ms }
            }
            _ => FanPolicyState::Disabled,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn cooling_disabled_pulse_duty_percent(current_temp_c: i16) -> u8 {
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C {
        return 0;
    }

    (((current_temp_c - COOLING_DISABLED_PULSE_START_TEMP_C) / 10) as u8).min(25)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heating_fan_pulse_duty_percent(current_temp_c: i16) -> u8 {
    cooling_disabled_pulse_duty_percent(current_temp_c)
        .saturating_mul(2)
        .min(HEATING_FAN_PULSE_MAX_DUTY_PERCENT)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heating_fan_state(current_temp_c: i16, heater_output_percent: u8) -> FanPolicyState {
    if current_temp_c > COOLING_DISABLED_FAN_FULL_TEMP_C {
        return FanPolicyState::Full;
    }
    if current_temp_c > COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return FanPolicyState::SafeHalf;
    }
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C || heater_output_percent == 0 {
        return FanPolicyState::Disabled;
    }

    let duty_percent = heating_fan_pulse_duty_percent(current_temp_c);
    if duty_percent == 0 {
        return FanPolicyState::Disabled;
    }

    FanPolicyState::CoolingDisabledPulse { duty_percent }
}

#[cfg(any(target_arch = "xtensa", test))]
fn cooling_disabled_state(current_temp_c: i16) -> FanPolicyState {
    if current_temp_c > COOLING_DISABLED_FAN_FULL_TEMP_C {
        return FanPolicyState::Full;
    }
    if current_temp_c > COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return FanPolicyState::SafeHalf;
    }
    if current_temp_c <= COOLING_DISABLED_PULSE_START_TEMP_C {
        return FanPolicyState::Disabled;
    }

    let duty_percent = cooling_disabled_pulse_duty_percent(current_temp_c);
    if duty_percent == 0 {
        return FanPolicyState::Disabled;
    }

    FanPolicyState::CoolingDisabledPulse { duty_percent }
}

#[cfg(any(target_arch = "xtensa", test))]
fn fan_display_state_for_command(
    active_cooling_enabled: bool,
    command: FanHardwareCommand,
) -> FanDisplayState {
    if !active_cooling_enabled {
        FanDisplayState::Off
    } else if command.enabled {
        FanDisplayState::Run
    } else {
        FanDisplayState::Auto
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn fan_policy_decision(
    current_temp_c: i16,
    elapsed_ms: u64,
    heater_enabled: bool,
    heater_output_percent: u8,
    active_cooling_enabled: bool,
    previous_state: FanPolicyState,
    hold_previous_output: bool,
) -> FanPolicyDecision {
    let state = if hold_previous_output {
        previous_state
    } else if heater_enabled {
        heating_fan_state(current_temp_c, heater_output_percent)
    } else if active_cooling_enabled {
        auto_cooling_command(current_temp_c, elapsed_ms, previous_state)
    } else {
        cooling_disabled_state(current_temp_c)
    };
    let command = state.command(elapsed_ms);

    FanPolicyDecision {
        state,
        command,
        display_state: fan_display_state_for_command(active_cooling_enabled, command),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn next_heater_lock_reason(
    heater_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
) -> Option<HeaterLockReason> {
    if heater_fault == Some(HeaterFaultReason::OverTemp) {
        Some(HeaterLockReason::HardOvertemp)
    } else if cooling_disabled_lock_latched {
        Some(HeaterLockReason::CoolingDisabledOvertemp)
    } else {
        None
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn next_dashboard_warning_visible(
    elapsed_ms: u64,
    heater_lock_reason: Option<HeaterLockReason>,
) -> bool {
    heater_lock_reason.is_some()
        && (elapsed_ms / DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS).is_multiple_of(2)
}

#[cfg(any(target_arch = "xtensa", test))]
fn reconcile_cooling_disabled_lock(
    active_cooling_enabled: bool,
    current_temp_c: i16,
    has_sensor_fault: bool,
    latched: bool,
    armed: bool,
) -> (bool, bool, bool) {
    if active_cooling_enabled {
        return (false, true, latched);
    }
    if has_sensor_fault {
        return (latched, armed, false);
    }
    if current_temp_c <= COOLING_DISABLED_HEATER_LOCK_TEMP_C {
        return (latched, true, false);
    }
    if armed {
        return (true, false, !latched);
    }

    (latched, armed, false)
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

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RtdTemporalMedian {
    samples_c: [f32; 3],
    count: usize,
    next: usize,
}

#[cfg(any(target_arch = "xtensa", test))]
impl RtdTemporalMedian {
    fn push(&mut self, temp_c: f32) -> f32 {
        self.samples_c[self.next] = temp_c;
        self.next = (self.next + 1) % self.samples_c.len();
        self.count = self.count.saturating_add(1).min(self.samples_c.len());
        match self.count {
            1 => self.samples_c[0],
            2 => (self.samples_c[0] + self.samples_c[1]) * 0.5,
            _ => {
                let [a, b, c] = self.samples_c;
                a.max(b).min(a.min(b).max(c))
            }
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(target_arch = "xtensa")]
fn should_clear_runtime_fault_latch(
    heater_rearm_requested: bool,
    current_rtd_fault: Option<HeaterFaultReason>,
    latched_fault: Option<HeaterFaultReason>,
) -> bool {
    heater_rearm_requested && current_rtd_fault.is_none() && latched_fault.is_some()
}

#[cfg(any(target_arch = "xtensa", test))]
fn update_fault_attention_state(
    fault_present: bool,
    last_fault_present: &mut bool,
    attention_pending_after_fault_clear: &mut bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut BuzzerController,
    now_ms: u64,
) -> bool {
    let mut changed = false;

    if fault_present && !*last_fault_present {
        *attention_pending_after_fault_clear = false;
        *next_attention_reminder_ms = None;
        let _ = buzzer.play(BuzzerCueId::ProtectionAlarm, now_ms);
        changed = true;
    } else if !fault_present && *last_fault_present {
        *attention_pending_after_fault_clear = true;
        *next_attention_reminder_ms =
            Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        if buzzer.active_cue() == Some(BuzzerCueId::ProtectionAlarm) {
            let _ = buzzer.stop();
        }
        changed = true;
    }

    *last_fault_present = fault_present;
    changed
}

#[cfg(any(target_arch = "xtensa", test))]
fn consume_attention_input_if_pending(
    attention_pending_after_fault_clear: &mut bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut BuzzerController,
) -> bool {
    if !*attention_pending_after_fault_clear {
        return false;
    }

    *attention_pending_after_fault_clear = false;
    *next_attention_reminder_ms = None;
    let _ = buzzer.stop();
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_consume_attention_raw_input(
    attention_pending_after_fault_clear: bool,
    suppressing_current_input: bool,
    previous_raw_state: FrontPanelRawState,
    current_raw_state: FrontPanelRawState,
) -> bool {
    attention_pending_after_fault_clear
        && !suppressing_current_input
        && current_raw_state != previous_raw_state
        && current_raw_state.pressed_mask() != 0
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_clear_attention_ack_suppression(
    suppressing_current_input: bool,
    waits_for_delayed_event: bool,
    suppressed_event_seen: bool,
    current_raw_state: FrontPanelRawState,
    clear_after_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    suppressing_current_input
        && current_raw_state.pressed_mask() == 0
        && (!waits_for_delayed_event
            || suppressed_event_seen
            || clear_after_ms.is_some_and(|deadline| now_ms >= deadline))
}

#[cfg(any(target_arch = "xtensa", test))]
fn maybe_play_attention_reminder(
    attention_pending_after_fault_clear: bool,
    fault_present: bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut BuzzerController,
    now_ms: u64,
) -> bool {
    if !attention_pending_after_fault_clear || fault_present {
        return false;
    }

    if next_attention_reminder_ms.is_some_and(|next| now_ms >= next) {
        let _ = buzzer.play(BuzzerCueId::AttentionReminder, now_ms);
        *next_attention_reminder_ms =
            Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        return true;
    }

    false
}

#[cfg(any(target_arch = "xtensa", test))]
fn maybe_play_frontpanel_ui_input_feedback(
    interaction_handled: bool,
    specialized_feedback_played: bool,
    buzzer: &mut BuzzerController,
    now_ms: u64,
) -> bool {
    if !interaction_handled || specialized_feedback_played {
        return false;
    }

    let _ = buzzer.play(BuzzerCueId::UiInput, now_ms);
    true
}

#[cfg(target_arch = "xtensa")]
fn temp_c_to_deci_c(temp_c: f32) -> i16 {
    let scaled = temp_c * 10.0;
    let rounded = if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    };
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
fn temp_c_to_centi_c(temp_c: f32) -> i32 {
    let scaled = temp_c * 100.0;
    let rounded = if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    };
    rounded.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[cfg(target_arch = "xtensa")]
fn temp_c_to_whole_c(temp_c: f32) -> i16 {
    let rounded = if temp_c >= 0.0 {
        temp_c + 0.5
    } else {
        temp_c - 0.5
    };
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct RtdMeasurement {
    raw_adc_mv: u16,
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

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PdStatusObservation {
    status_raw: u8,
    status: Status,
    current_raw: u8,
    current_ma: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaterPowerBackendReason {
    PpsCovers20v,
    NoPps20vCapability,
    CapabilityReadFailed,
    AdjustableRequestFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackendReason {
    const fn label(self) -> &'static str {
        match self {
            Self::PpsCovers20v => "pps-covers-20v",
            Self::NoPps20vCapability => "no-pps-20v-capability",
            Self::CapabilityReadFailed => "capability-read-failed",
            Self::AdjustableRequestFailed => "adjustable-request-failed",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaterPowerBackend {
    PpsMos {
        pps_min_mv: u16,
        idle_request_mv: u16,
        pps_max_mv: u16,
        adjustable_max_mv: u16,
        capability_max_ma: u16,
        current_mode: Option<ch224q::AdjustableVoltageMode>,
        current_request_mv: u16,
        settle_until_ms: Option<u64>,
        next_request_at_ms: u64,
        current_limit_fixed_pwm_active: bool,
        current_limit_fixed_request_confirmed: bool,
    },
    FixedPdPwmFallback {
        reason: HeaterPowerBackendReason,
        fixed_request_confirmed: bool,
        fixed_request: ch224q::VoltageRequest,
    },
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualPpsError {
    NoPpsCapability,
    InvalidVoltage,
    PdNotReady,
    WriteFailed,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualPpsOwner {
    Debug,
    Calibration,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ManualPpsError {
    const fn code(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "manual_pps_no_capability",
            Self::InvalidVoltage => "manual_pps_invalid_voltage",
            Self::PdNotReady => "manual_pps_pd_not_ready",
            Self::WriteFailed => "manual_pps_write_failed",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::NoPpsCapability => "PPS capability is unavailable.",
            Self::InvalidVoltage => {
                "manualPpsMv/manualPpsMa must match PPS capability and APDO steps."
            }
            Self::PdNotReady => "PD contract is not ready for manual PPS.",
            Self::WriteFailed => "Manual PPS write failed.",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualPpsState {
    enabled: bool,
    owner: ManualPpsOwner,
    target_mv: Option<u16>,
    target_ma: Option<u16>,
    applied_mv: Option<u16>,
    capability_min_mv: Option<u16>,
    capability_max_mv: Option<u16>,
    capability_max_ma: Option<u16>,
    capability_apdos: [Option<ch224q::PpsApdo>; ch224q::MAX_PPS_APDOS],
    error: Option<ManualPpsError>,
    automatic_restore_pending: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for ManualPpsState {
    fn default() -> Self {
        Self {
            enabled: false,
            owner: ManualPpsOwner::Debug,
            target_mv: None,
            target_ma: None,
            applied_mv: None,
            capability_min_mv: None,
            capability_max_mv: None,
            capability_max_ma: None,
            capability_apdos: [None; ch224q::MAX_PPS_APDOS],
            error: None,
            automatic_restore_pending: false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationMode {
    Off,
    VinAdc,
    RtdAdc,
    HeaterCurve,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationMode {
    const fn to_wire(self) -> CalibrationModeWire {
        match self {
            Self::Off => CalibrationModeWire::Off,
            Self::VinAdc => CalibrationModeWire::VinAdc,
            Self::RtdAdc => CalibrationModeWire::RtdAdc,
            Self::HeaterCurve => CalibrationModeWire::HeaterCurve,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<CalibrationModeWire> for CalibrationMode {
    fn from(value: CalibrationModeWire) -> Self {
        match value {
            CalibrationModeWire::Off => Self::Off,
            CalibrationModeWire::VinAdc => Self::VinAdc,
            CalibrationModeWire::RtdAdc => Self::RtdAdc,
            CalibrationModeWire::HeaterCurve => Self::HeaterCurve,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationJobKind {
    VinAdcAuto,
    HeaterCurveAuto,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationJobKind {
    const fn to_wire(self) -> CalibrationJobKindWire {
        match self {
            Self::VinAdcAuto => CalibrationJobKindWire::VinAdcAuto,
            Self::HeaterCurveAuto => CalibrationJobKindWire::HeaterCurveAuto,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<CalibrationJobKindWire> for CalibrationJobKind {
    fn from(value: CalibrationJobKindWire) -> Self {
        match value {
            CalibrationJobKindWire::VinAdcAuto => Self::VinAdcAuto,
            CalibrationJobKindWire::HeaterCurveAuto => Self::HeaterCurveAuto,
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationJobStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationJobStatus {
    const fn to_wire(self) -> CalibrationJobStatusWire {
        match self {
            Self::Idle => CalibrationJobStatusWire::Idle,
            Self::Running => CalibrationJobStatusWire::Running,
            Self::Completed => CalibrationJobStatusWire::Completed,
            Self::Failed => CalibrationJobStatusWire::Failed,
            Self::Canceled => CalibrationJobStatusWire::Canceled,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CalibrationJobState {
    kind: Option<CalibrationJobKind>,
    status: CalibrationJobStatus,
    progress_percent: u8,
    samples_collected: u8,
    next_request_mv: Option<u16>,
    message: Option<ManualPpsError>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for CalibrationJobState {
    fn default() -> Self {
        Self {
            kind: None,
            status: CalibrationJobStatus::Idle,
            progress_percent: 0,
            samples_collected: 0,
            next_request_mv: None,
            message: None,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
const CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES: usize = 24;
#[cfg(any(target_arch = "xtensa", test))]
const CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV: u16 = 40;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalibrationVinAutoJob {
    start_request_mv: u16,
    next_request_mv: u16,
    max_request_mv: u16,
    target_ma: u16,
    settle_ticks: u8,
    stable_ticks: u8,
    last_observed_mv: Option<u16>,
    sample_count: u8,
    samples: [Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterCurveAutoBin {
    min_temp_c: f32,
    max_temp_c: f32,
    samples: u16,
    temp_sum_c: f32,
    resistance_sum_ohms: f32,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterCurveAutoBin {
    const fn new(min_temp_c: f32, max_temp_c: f32) -> Self {
        Self {
            min_temp_c,
            max_temp_c,
            samples: 0,
            temp_sum_c: 0.0,
            resistance_sum_ohms: 0.0,
        }
    }

    fn contains(self, temp_c: f32) -> bool {
        temp_c >= self.min_temp_c && temp_c < self.max_temp_c
    }

    fn observe(&mut self, temp_c: f32, resistance_ohms: f32) {
        self.samples = self.samples.saturating_add(1);
        self.temp_sum_c += temp_c;
        self.resistance_sum_ohms += resistance_ohms;
    }

    fn averaged_point(self) -> Option<(i16, u16)> {
        if self.samples == 0 {
            return None;
        }
        let temp_c = self.temp_sum_c / f32::from(self.samples);
        let resistance_ohms = self.resistance_sum_ohms / f32::from(self.samples);
        let temp_centi_c = round_to_i16(temp_c * 100.0);
        let resistance_milliohms = round_to_u16_nonnegative(resistance_ohms * 1000.0);
        Some((temp_centi_c, resistance_milliohms))
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn round_to_i16(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    let rounded = if value >= 0.0 {
        value + 0.5
    } else {
        value - 0.5
    };
    rounded.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
fn round_to_u16_nonnegative(value: f32) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    (value + 0.5).clamp(0.0, u16::MAX as f32) as u16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalibrationHeaterCurveAutoJob {
    stable_ticks: u8,
    started_ticks: u8,
    bins: [HeaterCurveAutoBin; 4],
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for CalibrationHeaterCurveAutoJob {
    fn default() -> Self {
        Self {
            stable_ticks: 0,
            started_ticks: 0,
            bins: [
                HeaterCurveAutoBin::new(120.0, 160.0),
                HeaterCurveAutoBin::new(160.0, 200.0),
                HeaterCurveAutoBin::new(200.0, 230.0),
                HeaterCurveAutoBin::new(230.0, 251.0),
            ],
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
enum CalibrationJobData {
    VinAdcAuto(CalibrationVinAutoJob),
    HeaterCurveAuto(CalibrationHeaterCurveAutoJob),
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalibrationRuntimeState {
    mode: CalibrationMode,
    pps_enabled: bool,
    pps_mv: Option<u16>,
    pps_ma: Option<u16>,
    heater_enabled: bool,
    target_adc_mv: Option<u16>,
    stable: bool,
    stability_error_mv: Option<i16>,
    error: Option<ManualPpsError>,
    job: CalibrationJobState,
    job_data: Option<CalibrationJobData>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for CalibrationRuntimeState {
    fn default() -> Self {
        Self {
            mode: CalibrationMode::Off,
            pps_enabled: false,
            pps_mv: None,
            pps_ma: None,
            heater_enabled: false,
            target_adc_mv: None,
            stable: false,
            stability_error_mv: None,
            error: None,
            job: CalibrationJobState::default(),
            job_data: None,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_runtime_state_to_wire(
    state: CalibrationRuntimeState,
) -> CalibrationRuntimeStateWire {
    CalibrationRuntimeStateWire {
        mode: state.mode.to_wire(),
        pps_enabled: state.pps_enabled,
        pps_mv: state.pps_mv,
        pps_ma: state.pps_ma,
        heater_enabled: state.heater_enabled,
        target_adc_mv: state.target_adc_mv,
        stable: state.stable,
        stability_error_mv: state.stability_error_mv,
        error: state.error.map(manual_pps_error_code),
        job: CalibrationJobStateWire {
            kind: state.job.kind.map(CalibrationJobKind::to_wire),
            status: state.job.status.to_wire(),
            progress_percent: state.job.progress_percent,
            samples_collected: state.job.samples_collected,
            next_request_mv: state.job.next_request_mv,
            message: state.job.message.or(state.error).map(|error| {
                let mut out = heapless::String::new();
                let _ = out.push_str(error.message());
                out
            }),
        },
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn reconcile_runtime_heater_enabled(
    current_heater_enabled: bool,
    calibration_runtime_state: CalibrationRuntimeState,
    current_rtd_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    heater_fault_latched: bool,
) -> bool {
    if calibration_runtime_state.mode == CalibrationMode::Off {
        return current_heater_enabled;
    }

    let calibration_heater_allowed = !is_sensor_fault(current_rtd_fault)
        && !cooling_disabled_lock_latched
        && !heater_fault_latched;
    calibration_runtime_state.heater_enabled && calibration_heater_allowed
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl ManualPpsState {
    fn from_capabilities(capabilities: Option<ch224q::AdjustablePowerCapabilities>) -> Self {
        let mut state = Self::default();
        if let Some(capabilities) = capabilities
            && let (Some(min_mv), Some(max_mv), Some(max_ma)) = (
                capabilities.pps_min_mv,
                capabilities.pps_max_mv,
                capabilities.pps_max_ma,
            )
        {
            let bounded_min_mv = min_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV);
            let bounded_max_mv = max_mv.min(ch224q::CH224Q_PPS_MAX_MV);
            if bounded_min_mv <= bounded_max_mv && max_ma > 0 {
                state.capability_min_mv = Some(bounded_min_mv);
                state.capability_max_mv = Some(bounded_max_mv);
                state.capability_max_ma = Some(max_ma);
                state.capability_apdos = capabilities.pps_apdos;
                if state.capability_apdos.iter().all(Option::is_none) {
                    state.capability_apdos[0] = Some(ch224q::PpsApdo {
                        min_mv: bounded_min_mv,
                        max_mv,
                        max_ma,
                    });
                }
            }
        }
        state
    }

    fn validate_target(&self, target_mv: u16, target_ma: u16) -> Result<(), ManualPpsError> {
        let (Some(min_mv), Some(max_mv), Some(_max_ma)) = (
            self.capability_min_mv,
            self.capability_max_mv,
            self.capability_max_ma,
        ) else {
            return Err(ManualPpsError::NoPpsCapability);
        };
        if target_mv < min_mv
            || target_mv > max_mv
            || target_mv > ch224q::CH224Q_PPS_MAX_MV
            || !target_mv.is_multiple_of(100)
            || target_ma == 0
            || !target_ma.is_multiple_of(50)
            || !self.has_matching_pps_apdo(target_mv, target_ma)
        {
            return Err(ManualPpsError::InvalidVoltage);
        }
        Ok(())
    }

    fn has_matching_pps_apdo(&self, target_mv: u16, target_ma: u16) -> bool {
        self.capability_apdos.iter().flatten().any(|apdo| {
            let min_mv = apdo.min_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV);
            let max_mv = apdo.max_mv.min(ch224q::CH224Q_PPS_MAX_MV);
            target_mv >= min_mv && target_mv <= max_mv && target_ma <= apdo.max_ma
        })
    }

    fn enable(
        &mut self,
        owner: ManualPpsOwner,
        target_mv: u16,
        target_ma: Option<u16>,
    ) -> Result<(), ManualPpsError> {
        let target_ma = target_ma
            .or(self.capability_max_ma)
            .ok_or(ManualPpsError::NoPpsCapability)?;
        self.validate_target(target_mv, target_ma)?;
        self.enabled = true;
        self.owner = owner;
        self.target_mv = Some(target_mv);
        self.target_ma = Some(target_ma);
        self.error = None;
        self.automatic_restore_pending = false;
        Ok(())
    }

    fn clear(&mut self) {
        let had_override = self.enabled
            || self.target_mv.is_some()
            || self.target_ma.is_some()
            || self.applied_mv.is_some();
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = None;
        self.automatic_restore_pending |= had_override;
    }

    fn fail(&mut self, error: ManualPpsError) {
        self.enabled = false;
        self.owner = ManualPpsOwner::Debug;
        self.target_mv = None;
        self.target_ma = None;
        self.applied_mv = None;
        self.error = Some(error);
        self.automatic_restore_pending = true;
    }

    fn consume_automatic_restore_pending(&mut self) -> bool {
        let pending = self.automatic_restore_pending;
        self.automatic_restore_pending = false;
        pending
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn manual_pps_error_code(
    error: ManualPpsError,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    let mut out = heapless::String::new();
    let _ = out.push_str(error.code());
    out
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
impl HeaterPowerBackend {
    const fn label(self) -> &'static str {
        match self {
            Self::PpsMos { .. } => "pps-mos",
            Self::FixedPdPwmFallback { .. } => "fixed-pd-pwm-fallback",
        }
    }

    const fn pd_request_mv(self) -> u16 {
        match self {
            Self::PpsMos {
                current_request_mv, ..
            } => current_request_mv,
            Self::FixedPdPwmFallback { fixed_request, .. } => fixed_request.millivolts(),
        }
    }

    const fn pd_contract_mv(self) -> u16 {
        self.pd_request_mv()
    }
}

#[cfg(target_arch = "xtensa")]
fn log_ui_state(state: &FrontPanelUiState) {
    info!(
        "ui route={=str} temp_c={=i16} target_c={=i16} heater_arm={=bool} heater_out={=u8}% fan_runtime={=bool} fan_display={=str} cooling_policy={=bool} heater_lock={=str} warn_visible={=bool}",
        route_label(state.route),
        state.current_temp_c,
        state.target_temp_c,
        state.heater_enabled,
        state.heater_output_percent,
        state.fan_enabled,
        state.fan_display_state.label(),
        state.active_cooling_enabled,
        state
            .heater_lock_reason
            .map(|reason| reason.label())
            .unwrap_or("none"),
        state.dashboard_warning_visible,
    );
}

#[cfg(any(target_arch = "xtensa", test))]
fn pt1000_resistance_ohms_at(temp_c: f32) -> f32 {
    let polynomial = 1.0 + PT1000_A * temp_c + PT1000_B * temp_c * temp_c;
    if temp_c >= 0.0 {
        PT1000_R0_OHMS * polynomial
    } else {
        PT1000_R0_OHMS * (polynomial + PT1000_C * (temp_c - 100.0) * temp_c * temp_c * temp_c)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
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

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_resistance_ohms_from_mv(adc_mv: u16) -> Result<f32, HeaterFaultReason> {
    rtd_resistance_ohms_from_fractional_mv(f32::from(adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_resistance_ohms_from_fractional_mv(adc_mv: f32) -> Result<f32, HeaterFaultReason> {
    if adc_mv <= f32::from(RTD_SHORT_FAULT_MAX_MV) {
        return Err(HeaterFaultReason::SensorShort);
    }
    if adc_mv >= f32::from(RTD_OPEN_FAULT_MIN_MV) {
        return Err(HeaterFaultReason::SensorOpen);
    }
    let supply_mv_f = RTD_DIVIDER_SUPPLY_MV as f32;
    if adc_mv >= supply_mv_f {
        return Err(HeaterFaultReason::SensorOpen);
    }

    Ok(RTD_REFERENCE_RESISTOR_OHMS * adc_mv / (supply_mv_f - adc_mv))
}

#[cfg(any(target_arch = "xtensa", test))]
fn correct_adc_fractional_mv(
    memory_config: &MemoryConfig,
    channel: AdcCalibrationChannel,
    raw_adc_mv: f32,
) -> f32 {
    let lower_mv = raw_adc_mv.floor().clamp(0.0, f32::from(u16::MAX)) as u16;
    let upper_mv = lower_mv.saturating_add(1);
    let lower_corrected = f32::from(correct_adc_mv(
        &memory_config.adc_calibration,
        channel,
        lower_mv,
    ));
    if upper_mv == lower_mv {
        return lower_corrected;
    }
    let upper_corrected = f32::from(correct_adc_mv(
        &memory_config.adc_calibration,
        channel,
        upper_mv,
    ));
    lower_corrected + ((upper_corrected - lower_corrected) * (raw_adc_mv - f32::from(lower_mv)))
}

#[cfg(any(target_arch = "xtensa", test))]
fn vin_adc_mv_for_input_mv(input_mv: u32) -> u16 {
    let numerator = input_mv.saturating_mul(s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS);
    let denominator =
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS;
    (numerator / denominator).min(u32::from(u16::MAX)) as u16
}

#[cfg(target_arch = "xtensa")]
fn vin_input_mv_from_adc_mv(adc_mv: u16) -> u32 {
    let numerator = u32::from(adc_mv).saturating_mul(
        s3_frontpanel::VIN_DIVIDER_R_HIGH_OHMS + s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS,
    );
    numerator / s3_frontpanel::VIN_DIVIDER_R_LOW_OHMS
}

#[cfg(any(target_arch = "xtensa", test))]
fn rtd_fractional_mean_mv(sum_mv: u32, valid_samples: usize) -> Option<f32> {
    if valid_samples < RTD_MIN_VALID_SAMPLE_COUNT {
        return None;
    }
    Some(sum_mv as f32 / valid_samples as f32)
}

#[cfg(target_arch = "xtensa")]
fn read_rtd_adc_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO2<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
    >,
) -> Option<(u16, f32)> {
    let mut sum_mv: u32 = 0;
    let mut valid_samples = 0_usize;
    for _ in 0..RTD_SAMPLE_COUNT {
        let sample_mv = loop {
            match adc.read_oneshot(pin) {
                Ok(value) => break Some(value),
                Err(nb::Error::WouldBlock) => continue,
                Err(_) => break None,
            }
        };
        let Some(sample_mv) = sample_mv else {
            continue;
        };
        sum_mv = sum_mv.saturating_add(sample_mv as u32);
        valid_samples = valid_samples.saturating_add(1);
    }

    let mean_mv = rtd_fractional_mean_mv(sum_mv, valid_samples)?;
    Some((mean_mv.round() as u16, mean_mv))
}

#[cfg(target_arch = "xtensa")]
fn read_vin_adc_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO1<'a>,
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
fn read_calibrated_vin_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO1<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
    >,
    memory_config: &MemoryConfig,
) -> Option<(u16, u16, u32)> {
    let raw_adc_mv = read_vin_adc_mv(adc, pin)?;
    let corrected_adc_mv = correct_adc_mv(
        &memory_config.adc_calibration,
        AdcCalibrationChannel::Vin,
        raw_adc_mv,
    );
    Some((
        raw_adc_mv,
        corrected_adc_mv,
        vin_input_mv_from_adc_mv(corrected_adc_mv),
    ))
}

#[cfg(target_arch = "xtensa")]
fn read_rtd_sample<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO2<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
    >,
    memory_config: &MemoryConfig,
) -> RtdSample {
    let Some((raw_adc_mv, raw_adc_fractional_mv)) = read_rtd_adc_mv(adc, pin) else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };

    if raw_adc_fractional_mv <= f32::from(RTD_SHORT_FAULT_MAX_MV) {
        return RtdSample::Fault {
            adc_mv: Some(raw_adc_mv),
            reason: HeaterFaultReason::SensorShort,
        };
    }
    if raw_adc_fractional_mv >= f32::from(RTD_OPEN_FAULT_MIN_MV) {
        return RtdSample::Fault {
            adc_mv: Some(raw_adc_mv),
            reason: HeaterFaultReason::SensorOpen,
        };
    }

    let adc_fractional_mv = correct_adc_fractional_mv(
        memory_config,
        AdcCalibrationChannel::Rtd,
        raw_adc_fractional_mv,
    );
    let adc_mv = adc_fractional_mv.round() as u16;

    match rtd_resistance_ohms_from_fractional_mv(adc_fractional_mv) {
        Ok(resistance_ohms) => {
            let temp_c = pt1000_temperature_c_from_resistance(resistance_ohms);
            RtdSample::Valid(RtdMeasurement {
                raw_adc_mv,
                adc_mv,
                resistance_ohms,
                temp_c,
                current_temp_c: temp_c_to_whole_c(temp_c),
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
fn load_memory_record(i2c: &mut I2c<'_, esp_hal::Blocking>) -> Option<MemoryRecord> {
    let mut eeprom = M24c64::new(i2c);
    let mut slot_a = [0u8; MEMORY_SLOT_SIZE];
    let mut slot_b = [0u8; MEMORY_SLOT_SIZE];
    let slot_a_read = eeprom
        .read_bytes(MEMORY_SLOT_A_OFFSET, &mut slot_a)
        .map(|_| decode_memory_record(&slot_a))
        .ok()
        .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
    let slot_b_read = eeprom
        .read_bytes(MEMORY_SLOT_B_OFFSET, &mut slot_b)
        .map(|_| decode_memory_record(&slot_b))
        .ok()
        .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
    let mut selected = select_latest_memory_record(slot_a_read, slot_b_read);
    if selected.is_none() {
        let mut previous_slot_a = [0u8; PREVIOUS_MEMORY_SLOT_SIZE];
        let mut previous_slot_b = [0u8; PREVIOUS_MEMORY_SLOT_SIZE];
        let previous_slot_a_read = eeprom
            .read_bytes(PREVIOUS_MEMORY_SLOT_A_OFFSET, &mut previous_slot_a)
            .map(|_| decode_memory_record(&previous_slot_a))
            .ok()
            .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
        let previous_slot_b_read = eeprom
            .read_bytes(PREVIOUS_MEMORY_SLOT_B_OFFSET, &mut previous_slot_b)
            .map(|_| decode_memory_record(&previous_slot_b))
            .ok()
            .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
        selected = select_latest_memory_record(previous_slot_a_read, previous_slot_b_read);
    }
    if selected.is_none() {
        let mut legacy_slot_a = [0u8; LEGACY_MEMORY_SLOT_SIZE];
        let mut legacy_slot_b = [0u8; LEGACY_MEMORY_SLOT_SIZE];
        let legacy_slot_a_read = eeprom
            .read_bytes(LEGACY_MEMORY_SLOT_A_OFFSET, &mut legacy_slot_a)
            .map(|_| decode_memory_record(&legacy_slot_a))
            .ok()
            .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
        let legacy_slot_b_read = eeprom
            .read_bytes(LEGACY_MEMORY_SLOT_B_OFFSET, &mut legacy_slot_b)
            .map(|_| decode_memory_record(&legacy_slot_b))
            .ok()
            .unwrap_or(Err(flux_purr_firmware::memory::MemoryDecodeError::BadMagic));
        selected = select_latest_memory_record(legacy_slot_a_read, legacy_slot_b_read);
    }

    if let Some(record) = &selected {
        info!(
            "memory restore ok seq={=u32} target_c={=i16} slot={=u8} active_cooling={=bool} wifi_ssid_len={=u8} telemetry_ms={=u32}",
            record.sequence,
            record.config.target_temp_c,
            record.config.selected_preset_slot as u8,
            record.config.active_cooling_enabled,
            record.config.wifi_ssid.len() as u8,
            record.config.telemetry_interval_ms,
        );
    } else {
        info!("memory restore unavailable -> using defaults");
    }

    selected
}

#[cfg(target_arch = "xtensa")]
async fn write_memory_record(i2c: &mut I2c<'_, esp_hal::Blocking>, record: &MemoryRecord) -> bool {
    let mut bytes = [0xffu8; MEMORY_SLOT_SIZE];
    let Ok(record_len) = encode_memory_record(record, &mut bytes) else {
        info!("memory commit encode failed");
        return false;
    };
    let base_offset = memory_slot_offset_for_sequence(record.sequence);
    let mut eeprom = M24c64::new(i2c);
    let mut written = 0usize;
    while written < record_len {
        let absolute_offset = usize::from(base_offset) + written;
        let page_room = M24C64_PAGE_SIZE - (absolute_offset % M24C64_PAGE_SIZE);
        let chunk_len = (record_len - written).min(page_room).min(M24C64_PAGE_SIZE);
        let Ok(page_offset) = u16::try_from(absolute_offset) else {
            info!("memory commit offset overflow");
            return false;
        };
        if eeprom
            .write_page(page_offset, &bytes[written..written + chunk_len])
            .is_err()
        {
            info!("memory commit write failed seq={=u32}", record.sequence);
            return false;
        }
        written += chunk_len;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
    }
    info!(
        "memory commit ok seq={=u32} bytes={=u16} slot=0x{=u16:04x}",
        record.sequence, record_len as u16, base_offset,
    );
    true
}

#[cfg(target_arch = "xtensa")]
async fn commit_memory_config_now(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    memory_sequence: &mut u32,
    memory_config: &MemoryConfig,
) -> bool {
    let next_sequence = memory_sequence.saturating_add(1);
    let record = MemoryRecord {
        sequence: next_sequence,
        config: memory_config.clone(),
    };
    if !write_memory_record(i2c, &record).await {
        return false;
    }
    let Some(verified) = load_memory_record(i2c) else {
        info!(
            "memory commit verify failed seq={=u32} reason=unreadable",
            next_sequence
        );
        return false;
    };
    if verified.sequence != next_sequence || verified.config != *memory_config {
        info!(
            "memory commit verify failed seq={=u32} read_seq={=u32} config_match={=bool}",
            next_sequence,
            verified.sequence,
            verified.config == *memory_config,
        );
        return false;
    }
    *memory_sequence = next_sequence;
    true
}

#[cfg(target_arch = "xtensa")]
const fn memory_slot_offset_for_sequence(sequence: u32) -> u16 {
    if sequence % 2 == 1 {
        MEMORY_SLOT_A_OFFSET
    } else {
        MEMORY_SLOT_B_OFFSET
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn apply_memory_config_to_ui(state: &mut FrontPanelUiState, config: &MemoryConfig) {
    state.set_target_temp_c(config.target_temp_c);
    state.selected_preset_slot = config.selected_preset_slot;
    state.ensure_selected_preset_slot();
    state.presets_c = config.presets_c;
    state.active_cooling_enabled = config.active_cooling_enabled;
}

#[cfg(any(target_arch = "xtensa", test))]
fn memory_config_from_ui(state: &FrontPanelUiState, previous: &MemoryConfig) -> MemoryConfig {
    MemoryConfig {
        target_temp_c: state.target_temp_c,
        selected_preset_slot: state.selected_preset_slot,
        presets_c: state.presets_c,
        active_cooling_enabled: state.active_cooling_enabled,
        wifi_ssid: previous.wifi_ssid.clone(),
        wifi_password: previous.wifi_password.clone(),
        wifi_auto_reconnect: previous.wifi_auto_reconnect,
        telemetry_interval_ms: previous.telemetry_interval_ms,
        adc_calibration: previous.adc_calibration,
        active_heater_curve: previous.active_heater_curve,
        active_thermal_control_profile: previous.active_thermal_control_profile,
        thermal_control_profile_pps5a: previous.thermal_control_profile_pps5a,
        thermal_profile_mode: previous.thermal_profile_mode,
    }
}

#[allow(dead_code)]
fn floor_mv_to_100mv(millivolts: u16) -> u16 {
    (millivolts / 100) * 100
}

#[cfg(any(target_arch = "xtensa", test))]
fn default_estimated_heater_resistance_ohms(current_temp_c: f32) -> f32 {
    HEATER_PROFILE_R20_OHMS
        * (1.0 + HEATER_PROFILE_TEMP_COEFFICIENT_PER_C * (current_temp_c - 20.0))
}

#[cfg(any(target_arch = "xtensa", test))]
fn estimated_heater_resistance_ohms(
    current_temp_c: f32,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> f32 {
    preview_heater_curve
        .and_then(|curve| heater_resistance_ohms_from_curve(curve, current_temp_c))
        .or_else(|| {
            heater_resistance_ohms_from_curve(&memory_config.active_heater_curve, current_temp_c)
        })
        .unwrap_or_else(|| default_estimated_heater_resistance_ohms(current_temp_c))
}

#[cfg(any(target_arch = "xtensa", test))]
fn effective_pps_current_limit_ma(
    capability_max_ma: u16,
    pd_observation: Option<PdStatusObservation>,
) -> u16 {
    if capability_max_ma == 0 {
        return 0;
    }

    let observed_current_ma = pd_observation
        .filter(|observation| observation.status.pd_active && observation.current_ma > 0)
        .map(|observation| observation.current_ma);
    observed_current_ma
        .map(|observed_current_ma| observed_current_ma.min(capability_max_ma))
        .unwrap_or(capability_max_ma)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_available_current_ma(current_limit_ma: u16, reserve_ma: u16) -> u16 {
    current_limit_ma.saturating_sub(reserve_ma.min(current_limit_ma))
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_safe_max_mv_for_temp(
    current_temp_c: f32,
    effective_current_limit_ma: u16,
    source_voltage_max_mv: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u16 {
    if effective_current_limit_ma == 0 {
        return 0;
    }

    let estimated_mv =
        (estimated_heater_resistance_ohms(current_temp_c, preview_heater_curve, memory_config)
            * f32::from(effective_current_limit_ma))
        .max(0.0)
        .min(f32::from(u16::MAX)) as u16;
    floor_mv_to_100mv(estimated_mv).min(source_voltage_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_use_current_limit_fixed_pwm_fallback(
    duty_percent: u8,
    was_active: bool,
    safe_max_mv: u16,
    control_floor_mv: u16,
) -> bool {
    if duty_percent == 0 {
        return false;
    }

    if was_active {
        safe_max_mv < control_floor_mv.saturating_add(HEATER_CURRENT_LIMIT_RETURN_HYSTERESIS_MV)
    } else {
        safe_max_mv < control_floor_mv
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_apply_current_limit_fixed_pwm_fallback(
    duty_percent: u8,
    manual_pps_active: bool,
    was_active: bool,
    safe_max_mv: u16,
    control_floor_mv: u16,
) -> bool {
    !manual_pps_active
        && should_use_current_limit_fixed_pwm_fallback(
            duty_percent,
            was_active,
            safe_max_mv,
            control_floor_mv,
        )
}

#[cfg(any(target_arch = "xtensa", test))]
fn effective_auto_adjustable_working_floor_mv(
    settings: ThermalControlProfileSettings,
    capability_floor_mv: u16,
    adjustable_max_mv: u16,
) -> u16 {
    settings
        .auto_adjustable_working_floor_mv
        .max(capability_floor_mv)
        .clamp(capability_floor_mv, adjustable_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
fn current_limit_fixed_pwm_duty_percent(
    duty_percent: u8,
    current_temp_c: f32,
    effective_current_limit_ma: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u8 {
    let fixed_mv = HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts();
    if duty_percent == 0 || fixed_mv == 0 {
        return 0;
    }

    let safe_mv = heater_safe_max_mv_for_temp(
        current_temp_c,
        effective_current_limit_ma,
        fixed_mv,
        preview_heater_curve,
        memory_config,
    );
    let capped_percent = (u32::from(safe_mv) * 100 / u32::from(fixed_mv)).min(100) as u8;
    duty_percent.min(capped_percent)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_request_mv_from_power_percent(duty_percent: u8, floor_mv: u16, ceiling_mv: u16) -> u16 {
    let bounded_min_mv = floor_mv.clamp(CH224Q_ADJUSTABLE_REQUEST_MIN_MV, HEATER_ADJUSTABLE_MAX_MV);
    let bounded_max_mv = ceiling_mv
        .max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV)
        .clamp(bounded_min_mv, HEATER_ADJUSTABLE_MAX_MV);
    if duty_percent == 0 {
        return bounded_min_mv;
    }

    let requested_mv = integer_sqrt_floor(
        u64::from(bounded_max_mv)
            .saturating_mul(u64::from(bounded_max_mv))
            .saturating_mul(u64::from(duty_percent.min(100)))
            / 100,
    ) as u16;

    floor_mv_to_100mv((requested_mv as u16).clamp(bounded_min_mv, bounded_max_mv))
        .clamp(bounded_min_mv, bounded_max_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
fn floor_gate_pulse_density_duty_percent(
    duty_percent: u8,
    active_request_mv: u16,
    ceiling_mv: u16,
    now_ms: u64,
) -> u8 {
    let active_power = u64::from(active_request_mv).saturating_mul(u64::from(active_request_mv));
    let target_percent = u64::from(duty_percent)
        .saturating_mul(u64::from(ceiling_mv).saturating_mul(u64::from(ceiling_mv)))
        / active_power.max(1);
    let target_percent = target_percent.clamp(1, 100);
    let tick = now_ms / HEATER_CONTROL_INTERVAL_MS;
    if (tick.saturating_mul(target_percent) % 100) < target_percent {
        100
    } else {
        0
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(clippy::too_many_arguments)]
fn adjustable_floor_gate_duty_percent(
    duty_percent: u8,
    request_mv: u16,
    floor_mv: u16,
    ceiling_mv: u16,
    allow_subfloor_modulation: bool,
    heater_error_c: f32,
    hold_on_error_c: f32,
    previous_physical_duty_percent: u8,
    now_ms: u64,
) -> u8 {
    if duty_percent == 0 {
        return 0;
    }

    // A 5V-capable profile may request sub-5V-equivalent power before the bounded PPS down-ramp
    // reaches its floor. Compensate against the active request voltage so that transition energy
    // does not exceed the controller's requested power. Each tick remains static 0% or 100%.
    if floor_mv == CH224Q_ADJUSTABLE_REQUEST_MIN_MV && !allow_subfloor_modulation {
        return floor_gate_pulse_density_duty_percent(duty_percent, request_mv, ceiling_mv, now_ms);
    }

    if request_mv > floor_mv || ceiling_mv <= floor_mv {
        return 100;
    }

    if !allow_subfloor_modulation {
        return 100;
    }

    if previous_physical_duty_percent == 0 {
        if heater_error_c >= hold_on_error_c.max(0.05) {
            100
        } else {
            0
        }
    } else if heater_error_c <= 0.0 {
        0
    } else {
        100
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn integer_sqrt_floor(value: u64) -> u32 {
    let mut low = 0_u64;
    let mut high = u64::from(u32::MAX);
    while low <= high {
        let mid = low + ((high - low) / 2);
        let square = mid.saturating_mul(mid);
        if square == value {
            return mid as u32;
        }
        if square < value {
            low = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    high as u32
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn adjustable_mode_for_request(request_mv: u16, pps_max_mv: u16) -> ch224q::AdjustableVoltageMode {
    if request_mv <= pps_max_mv.min(ch224q::CH224Q_PPS_MAX_MV) {
        ch224q::AdjustableVoltageMode::Pps
    } else {
        ch224q::AdjustableVoltageMode::Avs
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_blank_heater_for_adjustable_request(
    _current_request_mv: u16,
    _next_request_mv: u16,
    mode_changed: bool,
) -> bool {
    mode_changed
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_restore_gate_after_adjustable_request(blank_heater: bool, gate_duty_percent: u8) -> bool {
    !blank_heater && gate_duty_percent > 0
}

#[cfg(any(target_arch = "xtensa", test))]
fn pps_request_transition_ms(mode_changed: bool) -> u64 {
    if mode_changed {
        HEATER_PPS_LARGE_TRANSITION_MS
    } else {
        HEATER_PPS_SMALL_TRANSITION_MS
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn clamp_ch224q_adjustable_request_mv(request_mv: u16) -> u16 {
    request_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV)
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_adjustable_request_mv(
    duty_percent: u8,
    heater_enabled: bool,
    current_request_mv: u16,
    idle_request_mv: u16,
    control_floor_mv: u16,
    safe_max_mv: u16,
) -> u16 {
    if duty_percent == 0 {
        if heater_enabled {
            current_request_mv
        } else {
            idle_request_mv
        }
    } else {
        let desired_request_mv =
            heater_request_mv_from_power_percent(duty_percent, control_floor_mv, safe_max_mv);
        if desired_request_mv.abs_diff(current_request_mv) < HEATER_PPS_REQUEST_HYSTERESIS_MV {
            current_request_mv
        } else if desired_request_mv > current_request_mv {
            current_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(desired_request_mv)
        } else {
            current_request_mv
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(desired_request_mv)
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn select_heater_power_backend(
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    status: Option<Status>,
) -> HeaterPowerBackend {
    let Some(capabilities) = capabilities else {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::CapabilityReadFailed,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
        };
    };

    if !capabilities.pps_covers_20v {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
        };
    }

    let pps_min_mv = capabilities
        .pps_min_mv
        .unwrap_or(HEATER_ADJUSTABLE_MIN_MV)
        .clamp(CH224Q_ADJUSTABLE_REQUEST_MIN_MV, ch224q::CH224Q_PPS_MAX_MV);
    let pps_max_mv = capabilities
        .pps_max_mv
        .unwrap_or(ch224q::PPS_GATE_MV)
        .clamp(ch224q::PPS_GATE_MV, ch224q::CH224Q_PPS_MAX_MV);
    let idle_request_mv = HEATER_ADJUSTABLE_MIN_MV.clamp(pps_min_mv, pps_max_mv);
    let avs_max_mv = if status.is_some_and(|status| status.avs_exist) {
        capabilities
            .avs_min_mv
            .zip(capabilities.avs_max_mv)
            .and_then(|(avs_min_mv, avs_max_mv)| {
                let bounded_avs_max_mv =
                    avs_max_mv.min(HEATER_ADJUSTABLE_MAX_MV.min(ch224q::CH224Q_AVS_MAX_MV));
                let first_avs_request_mv = pps_max_mv.saturating_add(100);
                if avs_min_mv <= first_avs_request_mv && bounded_avs_max_mv > pps_max_mv {
                    Some(bounded_avs_max_mv)
                } else {
                    None
                }
            })
    } else {
        None
    };
    let adjustable_max_mv = avs_max_mv.unwrap_or_else(|| pps_max_mv.min(HEATER_ADJUSTABLE_MAX_MV));
    let capability_max_ma = capabilities.pps_max_ma.unwrap_or(0);

    HeaterPowerBackend::PpsMos {
        pps_min_mv,
        idle_request_mv,
        pps_max_mv,
        adjustable_max_mv,
        capability_max_ma,
        current_mode: None,
        current_request_mv: idle_request_mv,
        settle_until_ms: None,
        next_request_at_ms: 0,
        current_limit_fixed_pwm_active: false,
        current_limit_fixed_request_confirmed: false,
    }
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
async fn apply_heater_power_output<PWM>(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    ch224q_address: Address,
    heater_pwm: &mut PWM,
    backend: &mut HeaterPowerBackend,
    manual_pps: &mut ManualPpsState,
    pd_observation: Option<PdStatusObservation>,
    current_temp_c: f32,
    duty_percent: u8,
    heater_enabled: bool,
    heater_phase: HeaterControlPhase,
    heater_error_c: f32,
    hold_on_error_c: f32,
    last_physical_duty_percent: &mut u8,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
    active_thermal_settings: ThermalControlProfileSettings,
    now_ms: u64,
) -> bool
where
    PWM: SetDutyCycle,
{
    let manual_pps_active = manual_pps.enabled;
    if manual_pps_active {
        let target_mv = match manual_pps.target_mv {
            Some(target_mv) => target_mv,
            None => {
                manual_pps.fail(ManualPpsError::InvalidVoltage);
                return true;
            }
        };
        let target_ma = manual_pps.target_ma.unwrap_or(0);
        if !pd_observation.is_some_and(|observation| observation.status.pd_active) {
            manual_pps.fail(ManualPpsError::PdNotReady);
            let fixed_payload = ch224q::voltage_request_payload(DEFAULT_PD_VOLTAGE_REQUEST);
            let _ = write_ch224q_payload(i2c, ch224q_address, &fixed_payload).await;
            return true;
        }
        if manual_pps.applied_mv != Some(target_mv) {
            if request_ch224q_adjustable_voltage(
                i2c,
                ch224q_address,
                target_mv,
                ch224q::AdjustableVoltageMode::Pps,
                true,
            )
            .await
            {
                manual_pps.applied_mv = Some(target_mv);
                info!(
                    "manual pps override applied mv={=u16} ma={=u16}",
                    target_mv, target_ma
                );
            } else {
                manual_pps.fail(ManualPpsError::WriteFailed);
                let fixed_payload = ch224q::voltage_request_payload(DEFAULT_PD_VOLTAGE_REQUEST);
                let _ = write_ch224q_payload(i2c, ch224q_address, &fixed_payload).await;
                info!(
                    "manual pps override cleared reason={=str}",
                    ManualPpsError::WriteFailed.code()
                );
                return true;
            }
        }
    }

    if manual_pps.consume_automatic_restore_pending() {
        match *backend {
            HeaterPowerBackend::FixedPdPwmFallback {
                reason,
                fixed_request,
                ..
            } => {
                *backend = HeaterPowerBackend::FixedPdPwmFallback {
                    reason,
                    fixed_request_confirmed: false,
                    fixed_request,
                };
            }
            HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv,
                pps_max_mv,
                adjustable_max_mv,
                capability_max_ma,
                ..
            } => {
                *backend = HeaterPowerBackend::PpsMos {
                    pps_min_mv,
                    idle_request_mv,
                    pps_max_mv,
                    adjustable_max_mv,
                    capability_max_ma,
                    current_mode: None,
                    current_request_mv: idle_request_mv,
                    settle_until_ms: None,
                    next_request_at_ms: 0,
                    current_limit_fixed_pwm_active: false,
                    current_limit_fixed_request_confirmed: false,
                };
            }
        }
    }

    match *backend {
        HeaterPowerBackend::FixedPdPwmFallback {
            reason,
            fixed_request_confirmed,
            fixed_request,
        } => {
            if !fixed_request_confirmed && !manual_pps_active {
                let fixed_payload = ch224q::voltage_request_payload(fixed_request);
                if write_ch224q_payload(i2c, ch224q_address, &fixed_payload).await {
                    *backend = HeaterPowerBackend::FixedPdPwmFallback {
                        reason,
                        fixed_request_confirmed: true,
                        fixed_request,
                    };
                    info!("heater backend fallback fixed-pd request confirmed");
                } else {
                    apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                    info!(
                        "heater backend fallback waiting for fixed-pd request reason={=str}",
                        reason.label(),
                    );
                    return false;
                }
            }
            apply_heater_duty(heater_pwm, duty_percent, last_physical_duty_percent);
            false
        }
        HeaterPowerBackend::PpsMos {
            pps_min_mv,
            idle_request_mv,
            pps_max_mv,
            adjustable_max_mv,
            capability_max_ma,
            current_mode,
            current_request_mv,
            settle_until_ms,
            next_request_at_ms,
            current_limit_fixed_pwm_active,
            current_limit_fixed_request_confirmed,
        } => {
            let source_current_limit_ma =
                effective_pps_current_limit_ma(capability_max_ma, pd_observation);
            let effective_current_limit_ma = heater_available_current_ma(
                source_current_limit_ma,
                active_thermal_settings.heater_current_reserve_ma,
            );
            let safe_max_mv = heater_safe_max_mv_for_temp(
                current_temp_c,
                effective_current_limit_ma,
                adjustable_max_mv,
                preview_heater_curve,
                memory_config,
            );
            let control_floor_mv = effective_auto_adjustable_working_floor_mv(
                active_thermal_settings,
                pps_min_mv,
                adjustable_max_mv,
            );
            let current_limit_fixed_pwm_active = should_apply_current_limit_fixed_pwm_fallback(
                duty_percent,
                manual_pps_active,
                current_limit_fixed_pwm_active,
                safe_max_mv,
                control_floor_mv,
            );
            if current_limit_fixed_pwm_active {
                if !current_limit_fixed_request_confirmed {
                    apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                    if let Some(settle_until_ms) = settle_until_ms {
                        if now_ms < settle_until_ms {
                            return false;
                        }
                        *backend = HeaterPowerBackend::PpsMos {
                            pps_min_mv,
                            idle_request_mv,
                            pps_max_mv,
                            adjustable_max_mv,
                            capability_max_ma,
                            current_mode: None,
                            current_request_mv: HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                            settle_until_ms: None,
                            next_request_at_ms: 0,
                            current_limit_fixed_pwm_active: true,
                            current_limit_fixed_request_confirmed: true,
                        };
                    } else {
                        let fixed_payload =
                            ch224q::voltage_request_payload(HEATER_CURRENT_LIMIT_FALLBACK_REQUEST);
                        if !write_ch224q_payload(i2c, ch224q_address, &fixed_payload).await {
                            info!(
                                "heater current-limit fallback waiting fixed_mv={=u16} safe_max_mv={=u16} control_floor_mv={=u16} current_limit_ma={=u16}",
                                HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                                safe_max_mv,
                                control_floor_mv,
                                effective_current_limit_ma,
                            );
                            return false;
                        }
                        *backend = HeaterPowerBackend::PpsMos {
                            pps_min_mv,
                            idle_request_mv,
                            pps_max_mv,
                            adjustable_max_mv,
                            capability_max_ma,
                            current_mode: None,
                            current_request_mv: HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                            settle_until_ms: Some(
                                now_ms.saturating_add(HEATER_PPS_LARGE_TRANSITION_MS),
                            ),
                            next_request_at_ms: 0,
                            current_limit_fixed_pwm_active: true,
                            current_limit_fixed_request_confirmed: false,
                        };
                        return true;
                    }
                }
                let fallback_duty_percent = current_limit_fixed_pwm_duty_percent(
                    duty_percent,
                    current_temp_c,
                    effective_current_limit_ma,
                    preview_heater_curve,
                    memory_config,
                );
                apply_heater_duty(
                    heater_pwm,
                    fallback_duty_percent,
                    last_physical_duty_percent,
                );
                info!(
                    "heater current-limit fallback active temp_c={=f32} current_limit_ma={=u16} safe_max_mv={=u16} control_floor_mv={=u16} fixed_mv={=u16} duty={=u8}%",
                    current_temp_c,
                    effective_current_limit_ma,
                    safe_max_mv,
                    control_floor_mv,
                    HEATER_CURRENT_LIMIT_FALLBACK_REQUEST.millivolts(),
                    fallback_duty_percent,
                );
                return true;
            }

            if let Some(settle_until_ms) = settle_until_ms {
                apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                if now_ms < settle_until_ms {
                    return false;
                }
                *backend = HeaterPowerBackend::PpsMos {
                    pps_min_mv,
                    idle_request_mv,
                    pps_max_mv,
                    adjustable_max_mv,
                    capability_max_ma,
                    current_mode,
                    current_request_mv,
                    settle_until_ms: None,
                    next_request_at_ms,
                    current_limit_fixed_pwm_active: false,
                    current_limit_fixed_request_confirmed: false,
                };
                if current_request_mv <= safe_max_mv {
                    let settled_gate_duty_percent = if duty_percent == 0 {
                        0
                    } else {
                        adjustable_floor_gate_duty_percent(
                            duty_percent,
                            current_request_mv,
                            control_floor_mv,
                            safe_max_mv,
                            heater_phase == HeaterControlPhase::Hold,
                            heater_error_c,
                            hold_on_error_c,
                            *last_physical_duty_percent,
                            now_ms,
                        )
                    };
                    apply_heater_duty(
                        heater_pwm,
                        settled_gate_duty_percent,
                        last_physical_duty_percent,
                    );
                    return true;
                }
            }

            let request_mv = heater_adjustable_request_mv(
                duty_percent,
                heater_enabled,
                current_request_mv,
                idle_request_mv,
                control_floor_mv,
                safe_max_mv,
            );
            let request_mode = adjustable_mode_for_request(request_mv, pps_max_mv);
            let mode_changed = !manual_pps_active && current_mode != Some(request_mode);
            let voltage_changed = !manual_pps_active && current_request_mv != request_mv;
            let request_transition_pending = !manual_pps_active && now_ms < next_request_at_ms;
            let gate_duty_percent = if duty_percent == 0 {
                0
            } else {
                adjustable_floor_gate_duty_percent(
                    duty_percent,
                    current_request_mv,
                    control_floor_mv,
                    safe_max_mv,
                    heater_phase == HeaterControlPhase::Hold,
                    heater_error_c,
                    hold_on_error_c,
                    *last_physical_duty_percent,
                    now_ms,
                )
            };

            let blank_heater = should_blank_heater_for_adjustable_request(
                current_request_mv,
                request_mv,
                mode_changed,
            );
            if gate_duty_percent == 0 {
                apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
            } else if blank_heater {
                apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
            }

            if (voltage_changed || mode_changed) && !request_transition_pending {
                if !request_ch224q_adjustable_voltage(
                    i2c,
                    ch224q_address,
                    request_mv,
                    request_mode,
                    mode_changed,
                )
                .await
                {
                    apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                    let fixed_payload = ch224q::voltage_request_payload(DEFAULT_PD_VOLTAGE_REQUEST);
                    let fixed_request_confirmed =
                        write_ch224q_payload(i2c, ch224q_address, &fixed_payload).await;
                    *backend = HeaterPowerBackend::FixedPdPwmFallback {
                        reason: HeaterPowerBackendReason::AdjustableRequestFailed,
                        fixed_request_confirmed,
                        fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
                    };
                    if fixed_request_confirmed {
                        apply_heater_duty(heater_pwm, duty_percent, last_physical_duty_percent);
                    }
                    info!(
                        "heater backend fallback -> reason={=str} fixed_request_confirmed={=bool}",
                        HeaterPowerBackendReason::AdjustableRequestFailed.label(),
                        fixed_request_confirmed,
                    );
                    return true;
                }

                if should_restore_gate_after_adjustable_request(blank_heater, gate_duty_percent) {
                    apply_heater_duty(heater_pwm, gate_duty_percent, last_physical_duty_percent);
                }
                *backend = HeaterPowerBackend::PpsMos {
                    pps_min_mv,
                    idle_request_mv,
                    pps_max_mv,
                    adjustable_max_mv,
                    capability_max_ma,
                    current_mode: Some(request_mode),
                    current_request_mv: request_mv,
                    settle_until_ms: blank_heater
                        .then_some(now_ms.saturating_add(pps_request_transition_ms(mode_changed))),
                    next_request_at_ms: now_ms
                        .saturating_add(pps_request_transition_ms(mode_changed)),
                    current_limit_fixed_pwm_active: false,
                    current_limit_fixed_request_confirmed: false,
                };
                return true;
            }

            let active_request_gate_duty_percent = if request_transition_pending {
                adjustable_floor_gate_duty_percent(
                    duty_percent,
                    current_request_mv,
                    control_floor_mv,
                    safe_max_mv,
                    heater_phase == HeaterControlPhase::Hold,
                    heater_error_c,
                    hold_on_error_c,
                    *last_physical_duty_percent,
                    now_ms,
                )
            } else {
                gate_duty_percent
            };
            apply_heater_duty(
                heater_pwm,
                active_request_gate_duty_percent,
                last_physical_duty_percent,
            );
            if voltage_changed || mode_changed {
                info!(
                    "heater pps request temp_c={=f32} control={=u8}% current_limit_ma={=u16} safe_max_mv={=u16} control_floor_mv={=u16} request_mv={=u16}",
                    current_temp_c,
                    duty_percent,
                    effective_current_limit_ma,
                    safe_max_mv,
                    control_floor_mv,
                    request_mv,
                );
            }
            false
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_fan_output<PWM>(
    fan_enable: &mut Output<'_>,
    fan_pwm: &mut PWM,
    command: FanHardwareCommand,
    last_command: &mut Option<FanHardwareCommand>,
) where
    PWM: SetDutyCycle,
{
    if last_command.is_some_and(|last| last == command) {
        return;
    }

    let duty_percent = pwm_percent_from_permille(command.pwm_permille);
    let _ = fan_pwm.set_duty_cycle_percent(duty_percent);
    if command.enabled {
        fan_enable.set_high();
    } else {
        fan_enable.set_low();
    }
    info!(
        "fan runtime -> {=str} gpio35={=str} gpio36 duty={=u8}% pwm_permille={=u16} freq={=u32}Hz",
        if command.enabled { "run" } else { "off" },
        if command.enabled { "on" } else { "off" },
        duty_percent,
        command.pwm_permille,
        FAN_PWM_FREQUENCY_HZ,
    );
    *last_command = Some(command);
}

#[cfg(target_arch = "xtensa")]
fn apply_buzzer_output<'a, PWM>(
    buzzer_timer: &mut esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'a>>,
    buzzer_pwm: &mut PWM,
    peripheral_clock: &PeripheralClockConfig,
    output: BuzzerOutput,
    last_state: &mut BuzzerHardwareState,
) where
    PWM: SetDutyCycle,
{
    let next_state = BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent.min(100),
        generation: output.generation,
    };
    if *last_state == next_state {
        return;
    }

    let restart_needed = last_state.generation != next_state.generation
        || last_state.frequency_hz != next_state.frequency_hz;

    if restart_needed {
        let next_frequency_hz = next_state.frequency_hz.unwrap_or(BUZZER_IDLE_FREQUENCY_HZ);
        let timer_cfg = peripheral_clock
            .timer_clock_with_frequency(
                BUZZER_PWM_PERIOD_TICKS,
                PwmWorkingMode::Increase,
                Rate::from_hz(next_frequency_hz),
            )
            .expect("failed to derive buzzer PWM timer clock");
        buzzer_timer.stop();
        buzzer_timer.set_counter(0, CounterDirection::Increasing);
        buzzer_timer.start(timer_cfg);
    }

    let _ = buzzer_pwm.set_duty_cycle_percent(next_state.duty_percent);
    info!(
        "buzzer output -> freq_hz={=u32} duty={=u8}% gen={=u32}",
        next_state.frequency_hz.unwrap_or(0),
        next_state.duty_percent,
        next_state.generation,
    );
    *last_state = next_state;
}

#[cfg(target_arch = "xtensa")]
fn sync_frontpanel_runtime_state(
    ui_state: &mut FrontPanelUiState,
    fan_decision: FanPolicyDecision,
    heater_lock_reason: Option<HeaterLockReason>,
    elapsed_ms: u64,
) -> bool {
    let mut changed = false;

    if ui_state.fan_enabled != fan_decision.command.enabled {
        ui_state.fan_enabled = fan_decision.command.enabled;
        changed = true;
    }
    if ui_state.fan_display_state != fan_decision.display_state {
        ui_state.fan_display_state = fan_decision.display_state;
        changed = true;
    }
    if ui_state.heater_lock_reason != heater_lock_reason {
        ui_state.heater_lock_reason = heater_lock_reason;
        changed = true;
    }

    let dashboard_warning_visible = next_dashboard_warning_visible(elapsed_ms, heater_lock_reason);
    if ui_state.dashboard_warning_visible != dashboard_warning_visible {
        ui_state.dashboard_warning_visible = dashboard_warning_visible;
        changed = true;
    }

    changed
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[derive(Clone, Copy)]
struct UsbRuntimeStatusContext {
    elapsed_ms: u64,
    last_pd_observation: Option<PdStatusObservation>,
    heater_power_backend: HeaterPowerBackend,
    pid_snapshot: HeaterPidSnapshot,
    heater_control_timing: HeaterControlTiming,
    heater_physical_output_percent: u8,
    manual_pps: ManualPpsState,
    calibration: CalibrationRuntimeState,
    fan_command: FanHardwareCommand,
    current_rtd_fault: Option<HeaterFaultReason>,
    heater_fault_latched: Option<HeaterFaultReason>,
    thermal_control_profile_preview: bool,
    active_thermal_control_profile: Option<ThermalControlProfile>,
    last_raw_state: FrontPanelRawState,
    latest_temp_c: f32,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    vin_mv: u32,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_runtime_status(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    context: UsbRuntimeStatusContext,
) -> ControlPlaneStatus {
    let pd_contract_mv = context
        .manual_pps
        .target_mv
        .filter(|_| context.manual_pps.enabled)
        .unwrap_or_else(|| context.heater_power_backend.pd_contract_mv());
    let pd_state = if context.current_rtd_fault.is_some() {
        PdState::Fault
    } else if context
        .last_pd_observation
        .is_some_and(|observation| observation.status.pd_active)
    {
        PdState::Ready
    } else if pd_contract_mv <= 5_000 {
        PdState::Fallback5V
    } else {
        PdState::Negotiating
    };
    let frontpanel_key = context
        .last_raw_state
        .first_pressed()
        .map(|raw_key| FrontPanelKeyMap::default().logical_from_raw(raw_key));

    let mut status = ControlPlaneStatus::from_device_status(
        DeviceStatus {
            mode: if context.current_rtd_fault.is_some() {
                DeviceMode::Fault
            } else if ui_state.heater_enabled {
                DeviceMode::Sampling
            } else {
                DeviceMode::Idle
            },
            voltage_mv: context.vin_mv,
            current_ma: u32::from(
                context
                    .last_pd_observation
                    .map(|observation| observation.current_ma)
                    .unwrap_or(0),
            ),
            board_temp_centi: temp_c_to_centi_c(context.latest_temp_c),
            rtd_raw_adc_mv: 0,
            vin_raw_adc_mv: 0,
            pd_request_mv: context
                .manual_pps
                .target_mv
                .filter(|_| context.manual_pps.enabled)
                .unwrap_or_else(|| context.heater_power_backend.pd_request_mv()),
            pd_contract_mv,
            pd_state,
            heater_output_percent: ui_state.heater_output_percent,
            heater_physical_output_percent: context.heater_physical_output_percent,
            fan_enabled: ui_state.fan_enabled,
            fan_pwm_permille: context.fan_command.pwm_permille,
            frontpanel_key,
        },
        memory_config,
        (context.elapsed_ms / 1_000).min(u64::from(u32::MAX)) as u32,
        network_from_memory(memory_config),
    )
    .with_runtime_target_temp_c(ui_state.target_temp_c);
    status.rtd_raw_adc_mv = context.latest_rtd_raw_adc_mv;
    status.vin_raw_adc_mv = context.latest_vin_raw_adc_mv;
    status.manual_pps_enabled = context.manual_pps.enabled;
    status.manual_pps_mv = context.manual_pps.target_mv;
    status.manual_pps_ma = context.manual_pps.target_ma;
    status.pps_capability_min_mv = context.manual_pps.capability_min_mv;
    status.pps_capability_max_mv = context.manual_pps.capability_max_mv;
    status.pps_capability_max_ma = context.manual_pps.capability_max_ma;
    status.manual_pps_error = context.manual_pps.error.map(manual_pps_error_code);
    status.heater_fault_reason = context.heater_fault_latched.map(|reason| {
        let mut value = heapless::String::new();
        let _ = value.push_str(reason.label());
        value
    });
    status.heater_lock_reason = ui_state.heater_lock_reason.map(Into::into);
    let mut heater_control_phase = heapless::String::new();
    let _ = heater_control_phase.push_str(context.pid_snapshot.phase.label());
    status.heater_control_phase = Some(heater_control_phase);
    status.heater_error_c = Some(context.pid_snapshot.error_c);
    status.heater_control_error_c = Some(context.pid_snapshot.control_error_c);
    status.heater_filtered_temp_c = Some(context.pid_snapshot.filtered_temp_c);
    status.heater_filtered_slope_c_per_s = Some(context.pid_snapshot.filtered_slope_c_per_s);
    status.heater_coast_active = context.pid_snapshot.coast_active;
    status.heater_control_interval_ms = context.heater_control_timing.interval_ms;
    status.heater_control_cycle_ms = context.heater_control_timing.cycle_ms;
    status.calibration = calibration_runtime_state_to_wire(context.calibration);
    status.thermal_control_profile_preview = context.thermal_control_profile_preview;
    let resolved_bank = resolve_thermal_profile_bank(
        memory_config.thermal_profile_mode,
        context.manual_pps.capability_min_mv,
        context.manual_pps.capability_max_mv,
        context.manual_pps.capability_max_ma,
    );
    let mut thermal_profile_mode = heapless::String::new();
    let _ = thermal_profile_mode.push_str(memory_config.thermal_profile_mode.as_str());
    status.thermal_profile_mode = thermal_profile_mode;
    let mut thermal_profile_resolved_bank = heapless::String::new();
    let _ = thermal_profile_resolved_bank.push_str(resolved_bank.as_str());
    status.thermal_profile_resolved_bank = thermal_profile_resolved_bank;
    status.thermal_control = thermal_control_runtime_wire(
        ui_state.target_temp_c,
        context.active_thermal_control_profile,
        context.thermal_control_profile_preview,
    );
    status
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_runtime_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    mut context: UsbRuntimeStatusContext,
) -> (UsbFrame, CalibrationRuntimeState) {
    if let Some(command) = config.thermal_control_profile {
        match command.op {
            ThermalControlProfileOp::Preview | ThermalControlProfileOp::Save
                if command.profile.is_none() =>
            {
                return (
                    UsbFrame::Response {
                        request_id,
                        ok: false,
                        result: None,
                        error: Some(ApiError::new(
                            "thermal_profile_required",
                            "thermalControlProfile.profile is required for preview/save.",
                            false,
                        )),
                    },
                    context.calibration,
                );
            }
            ThermalControlProfileOp::Save
                if command.profile.is_some_and(|profile| {
                    profile.points.iter().flatten().count()
                        > THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS
                }) =>
            {
                return (
                    UsbFrame::Response {
                        request_id,
                        ok: false,
                        result: None,
                        error: Some(ApiError::new(
                            "thermal_profile_too_many_saved_points",
                            "saved thermal profiles support at most 6 populated points.",
                            false,
                        )),
                    },
                    context.calibration,
                );
            }
            ThermalControlProfileOp::ClearPreview | ThermalControlProfileOp::ClearSaved
                if command.profile.is_some() =>
            {
                return (
                    UsbFrame::Response {
                        request_id,
                        ok: false,
                        result: None,
                        error: Some(ApiError::new(
                            "thermal_profile_clear_payload",
                            "thermalControlProfile.profile must be omitted for clear operations.",
                            false,
                        )),
                    },
                    context.calibration,
                );
            }
            _ => {}
        }
    }

    if let Some(calibration) = config.calibration
        && let Err(error) =
            apply_calibration_control_config(&calibration, &mut context.calibration, manual_pps)
    {
        return (
            UsbFrame::Response {
                request_id,
                ok: false,
                result: None,
                error: Some(ApiError::new(error.code(), error.message(), false)),
            },
            context.calibration,
        );
    }
    if let Err(error) = apply_manual_pps_config(&config, manual_pps) {
        return (
            UsbFrame::Response {
                request_id,
                ok: false,
                result: None,
                error: Some(ApiError::new(error.code(), error.message(), false)),
            },
            context.calibration,
        );
    }
    if let Some(command) = config.thermal_control_profile {
        match command.op {
            ThermalControlProfileOp::Preview => {
                let Some(profile) = command.profile else {
                    return (
                        UsbFrame::Response {
                            request_id,
                            ok: false,
                            result: None,
                            error: Some(ApiError::new(
                                "thermal_profile_required",
                                "thermalControlProfile.profile is required for preview.",
                                false,
                            )),
                        },
                        context.calibration,
                    );
                };
                *thermal_control_profile_preview = Some(ThermalControlProfile::from(profile));
            }
            ThermalControlProfileOp::ClearPreview => {
                *thermal_control_profile_preview = None;
            }
            ThermalControlProfileOp::Save => {
                *thermal_control_profile_preview = None;
            }
            ThermalControlProfileOp::ClearSaved => {}
        }
    }
    config.apply_to(memory_config);
    apply_memory_config_to_ui(ui_state, memory_config);
    if let Some(heater_enabled) = config.heater_enabled {
        ui_state.heater_enabled = heater_enabled;
    }
    if context.calibration.mode != CalibrationMode::Off {
        if let Some(heater_enabled) = config
            .calibration
            .and_then(|calibration| calibration.heater_enabled)
        {
            ui_state.heater_enabled = heater_enabled;
        }
        if context.calibration.mode == CalibrationMode::RtdAdc
            && context.calibration.heater_enabled
            && let Some(target_adc_mv) = context.calibration.target_adc_mv
        {
            let hold_target_c = pt1000_temperature_c_from_resistance(
                rtd_resistance_ohms_from_mv(target_adc_mv)
                    .unwrap_or_else(|_| pt1000_resistance_ohms_at(0.0)),
            );
            let hold_target_c = if hold_target_c >= 0.0 {
                (hold_target_c + 0.5) as i16
            } else {
                (hold_target_c - 0.5) as i16
            };
            ui_state.target_temp_c =
                hold_target_c.clamp(FRONTPANEL_TARGET_TEMP_MIN_C, FRONTPANEL_TARGET_TEMP_MAX_C);
        }
    }
    ui_state.manual_pps_enabled = manual_pps.enabled;
    context.manual_pps = *manual_pps;
    context.thermal_control_profile_preview = thermal_control_profile_preview.is_some();
    context.active_thermal_control_profile = active_thermal_control_profile(
        memory_config,
        *thermal_control_profile_preview,
        manual_pps.capability_min_mv,
        manual_pps.capability_max_mv,
        manual_pps.capability_max_ma,
    );

    (
        usb_response(
            request_id,
            UsbResponsePayload::Status(usb_runtime_status(ui_state, memory_config, context)),
        ),
        context.calibration,
    )
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn apply_manual_pps_config(
    config: &RuntimeConfigCommand,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    if config.manual_pps_enabled == Some(false) {
        manual_pps.clear();
        return Ok(());
    }

    if config.manual_pps_enabled == Some(true)
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some()
    {
        let target_mv = config
            .manual_pps_mv
            .or(manual_pps.target_mv)
            .ok_or(ManualPpsError::InvalidVoltage)?;
        let target_ma = config.manual_pps_ma.or(manual_pps.target_ma);
        manual_pps.enable(ManualPpsOwner::Debug, target_mv, target_ma)?;
    }

    Ok(())
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn apply_calibration_control_config(
    config: &CalibrationControlCommand,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    if let Some(mode) = config.mode {
        calibration.mode = mode.into();
        if calibration.mode == CalibrationMode::Off {
            calibration.pps_enabled = false;
            calibration.heater_enabled = false;
            calibration.target_adc_mv = None;
            calibration.stable = false;
            calibration.stability_error_mv = None;
            calibration.job = CalibrationJobState::default();
            calibration.job_data = None;
        }
    }

    if let Some(target_adc_mv) = config.target_adc_mv {
        calibration.target_adc_mv = Some(target_adc_mv);
    }

    if let Some(heater_enabled) = config.heater_enabled {
        calibration.heater_enabled = heater_enabled;
    }

    if config.pps_enabled == Some(false) {
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
        if manual_pps.owner == ManualPpsOwner::Calibration {
            manual_pps.clear();
        }
        return Ok(());
    }

    if config.pps_enabled == Some(true) || config.pps_mv.is_some() {
        let target_mv = config
            .pps_mv
            .or(calibration.pps_mv)
            .ok_or(ManualPpsError::InvalidVoltage)?;
        let target_ma = calibration
            .pps_ma
            .or(manual_pps.target_ma)
            .or(manual_pps.capability_max_ma);
        manual_pps.enable(ManualPpsOwner::Calibration, target_mv, target_ma)?;
        calibration.pps_enabled = true;
        calibration.pps_mv = manual_pps.target_mv;
        calibration.pps_ma = manual_pps.target_ma;
        calibration.error = None;
    }

    Ok(())
}

#[cfg(target_arch = "xtensa")]
fn update_calibration_runtime_state(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &ManualPpsState,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) {
    calibration.pps_enabled = manual_pps.enabled && manual_pps.owner == ManualPpsOwner::Calibration;
    if calibration.pps_enabled {
        calibration.pps_mv = manual_pps.target_mv;
        calibration.pps_ma = manual_pps.target_ma;
        calibration.error = manual_pps.error;
    } else if manual_pps.owner != ManualPpsOwner::Calibration {
        calibration.pps_mv = None;
        calibration.pps_ma = None;
        calibration.error = None;
    }

    let observed_adc_mv = match calibration.mode {
        CalibrationMode::RtdAdc => Some(latest_rtd_raw_adc_mv),
        CalibrationMode::VinAdc => Some(latest_vin_raw_adc_mv),
        CalibrationMode::Off | CalibrationMode::HeaterCurve => None,
    };

    calibration.stability_error_mv = calibration
        .target_adc_mv
        .zip(observed_adc_mv)
        .map(|(target, observed)| (i32::from(observed) - i32::from(target)) as i16);
    calibration.stable = calibration
        .stability_error_mv
        .is_some_and(|error_mv| error_mv.abs() <= 8);
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_job_fail(
    calibration: &mut CalibrationRuntimeState,
    error: ManualPpsError,
    clear_manual_pps: bool,
    manual_pps: &mut ManualPpsState,
) {
    calibration.job.status = CalibrationJobStatus::Failed;
    calibration.job.message = Some(error);
    calibration.job_data = None;
    calibration.heater_enabled = false;
    if clear_manual_pps && manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_job_complete(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    samples_collected: u8,
    next_request_mv: Option<u16>,
) {
    calibration.job.kind = Some(kind);
    calibration.job.status = CalibrationJobStatus::Completed;
    calibration.job.progress_percent = 100;
    calibration.job.samples_collected = samples_collected;
    calibration.job.next_request_mv = next_request_mv;
    calibration.job.message = None;
    calibration.job_data = None;
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn calibration_job_canceled(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    calibration.job.status = CalibrationJobStatus::Canceled;
    calibration.job.message = None;
    calibration.job_data = None;
    calibration.heater_enabled = false;
    if manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_job_start(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    if calibration.job.status == CalibrationJobStatus::Running {
        return Err(ManualPpsError::WriteFailed);
    }
    match kind {
        CalibrationJobKind::VinAdcAuto => {
            if calibration.mode != CalibrationMode::VinAdc {
                return Err(ManualPpsError::InvalidVoltage);
            }
            let min_mv = manual_pps
                .capability_min_mv
                .ok_or(ManualPpsError::NoPpsCapability)?
                .max(5_000);
            let max_mv = manual_pps
                .capability_max_mv
                .ok_or(ManualPpsError::NoPpsCapability)?
                .min(28_000);
            let target_ma = calibration
                .pps_ma
                .or(manual_pps.capability_max_ma)
                .ok_or(ManualPpsError::NoPpsCapability)?;
            let next_request_mv = min_mv.div_ceil(100) * 100;
            manual_pps.enable(
                ManualPpsOwner::Calibration,
                next_request_mv,
                Some(target_ma),
            )?;
            memory_config.adc_calibration.vin.clear();
            memory_config.sanitize();
            calibration.pps_enabled = true;
            calibration.pps_mv = Some(next_request_mv);
            calibration.pps_ma = Some(target_ma);
            calibration.heater_enabled = false;
            calibration.job = CalibrationJobState {
                kind: Some(kind),
                status: CalibrationJobStatus::Running,
                progress_percent: 0,
                samples_collected: 0,
                next_request_mv: Some(next_request_mv),
                message: None,
            };
            calibration.job_data = Some(CalibrationJobData::VinAdcAuto(CalibrationVinAutoJob {
                start_request_mv: next_request_mv,
                next_request_mv,
                max_request_mv: max_mv,
                target_ma,
                settle_ticks: 0,
                stable_ticks: 0,
                last_observed_mv: None,
                sample_count: 0,
                samples: [None; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
            }));
            Ok(())
        }
        CalibrationJobKind::HeaterCurveAuto => {
            if calibration.mode != CalibrationMode::HeaterCurve {
                return Err(ManualPpsError::InvalidVoltage);
            }
            let target_mv = calibration.pps_mv.ok_or(ManualPpsError::InvalidVoltage)?;
            let target_ma = calibration
                .pps_ma
                .or(manual_pps.target_ma)
                .or(manual_pps.capability_max_ma)
                .ok_or(ManualPpsError::NoPpsCapability)?;
            manual_pps.enable(ManualPpsOwner::Calibration, target_mv, Some(target_ma))?;
            calibration.pps_enabled = true;
            calibration.pps_mv = manual_pps.target_mv;
            calibration.pps_ma = manual_pps.target_ma;
            calibration.heater_enabled = true;
            calibration.job = CalibrationJobState {
                kind: Some(kind),
                status: CalibrationJobStatus::Running,
                progress_percent: 0,
                samples_collected: 0,
                next_request_mv: Some(target_mv),
                message: None,
            };
            calibration.job_data = Some(CalibrationJobData::HeaterCurveAuto(
                CalibrationHeaterCurveAutoJob::default(),
            ));
            Ok(())
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn monotonic_smooth_heater_curve_points(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
) {
    if points.len() <= 1 {
        return;
    }

    for index in 1..points.len() {
        if points[index].resistance_milliohms < points[index - 1].resistance_milliohms {
            points[index].resistance_milliohms = points[index - 1].resistance_milliohms;
        }
        if points[index].temp_centi_c <= points[index - 1].temp_centi_c {
            points[index].temp_centi_c = points[index - 1].temp_centi_c.saturating_add(1);
        }
    }

    if points.len() >= 3 {
        let original = points.clone();
        for index in 1..(points.len() - 1) {
            let left = u32::from(original[index - 1].resistance_milliohms);
            let center = u32::from(original[index].resistance_milliohms);
            let right = u32::from(original[index + 1].resistance_milliohms);
            points[index].resistance_milliohms = ((left + center + right) / 3) as u16;
        }
        for index in 1..points.len() {
            if points[index].resistance_milliohms < points[index - 1].resistance_milliohms {
                points[index].resistance_milliohms = points[index - 1].resistance_milliohms;
            }
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn select_vin_auto_draft_samples(
    collected: &[Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
    sample_count: usize,
) -> heapless::Vec<AdcCalibrationSample, ADC_CALIBRATION_MAX_SAMPLES> {
    let mut dense =
        heapless::Vec::<AdcCalibrationSample, CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES>::new();
    for sample in collected.iter().take(sample_count).flatten() {
        let _ = dense.push(*sample);
    }

    let mut selected = heapless::Vec::<AdcCalibrationSample, ADC_CALIBRATION_MAX_SAMPLES>::new();
    if dense.is_empty() {
        return selected;
    }
    if dense.len() <= ADC_CALIBRATION_MAX_SAMPLES {
        for sample in dense {
            let _ = selected.push(sample);
        }
        return selected;
    }

    let last_index = dense.len() - 1;
    let bucket_count = ADC_CALIBRATION_MAX_SAMPLES - 1;
    let mut previous_index = None::<usize>;
    for slot in 0..ADC_CALIBRATION_MAX_SAMPLES {
        let index = if slot == 0 {
            0
        } else if slot == ADC_CALIBRATION_MAX_SAMPLES - 1 {
            last_index
        } else {
            ((slot * last_index) + (bucket_count / 2)) / bucket_count
        };
        if previous_index == Some(index) {
            continue;
        }
        previous_index = Some(index);
        let _ = selected.push(dense[index]);
    }
    selected
}

#[cfg(any(target_arch = "xtensa", test))]
fn commit_vin_auto_samples_to_draft(
    memory_config: &mut MemoryConfig,
    collected: &[Option<AdcCalibrationSample>; CALIBRATION_VIN_AUTO_MAX_SWEEP_SAMPLES],
    sample_count: usize,
) -> usize {
    let selected = select_vin_auto_draft_samples(collected, sample_count);
    memory_config.adc_calibration.vin.clear();
    for sample in selected {
        let _ = memory_config.adc_calibration.vin.insert(sample);
    }
    memory_config.sanitize();
    memory_config.adc_calibration.vin.sample_count()
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_curve_preview_from_auto_bins(
    bins: &[HeaterCurveAutoBin; 4],
) -> Option<HeaterCurveConfig> {
    let mut compacted = heapless::Vec::<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>::new();
    for bin in bins {
        let Some((temp_centi_c, resistance_milliohms)) = bin.averaged_point() else {
            continue;
        };
        let _ = compacted.push(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }
    if compacted.is_empty() {
        return None;
    }
    monotonic_smooth_heater_curve_points(&mut compacted);
    let mut points = [None; HEATER_CURVE_MAX_POINTS];
    for (index, point) in compacted.into_iter().enumerate() {
        points[index] = Some(point);
    }
    Some(HeaterCurveConfig { points })
}

#[allow(clippy::too_many_arguments)]
#[cfg(any(target_arch = "xtensa", test))]
fn update_calibration_job_state(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    preview_heater_curve: &mut Option<HeaterCurveConfig>,
    manual_pps: &mut ManualPpsState,
    _latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_temp_c: f32,
    pd_current_ma: u16,
    latest_vin_mv: u32,
) {
    if calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    let Some(job_data) = calibration.job_data else {
        return;
    };

    match job_data {
        CalibrationJobData::VinAdcAuto(mut job) => {
            if manual_pps.error.is_some() || !manual_pps.enabled {
                calibration_job_fail(
                    calibration,
                    manual_pps.error.unwrap_or(ManualPpsError::WriteFailed),
                    false,
                    manual_pps,
                );
                return;
            }

            if calibration.pps_mv != Some(job.next_request_mv) {
                match manual_pps.enable(
                    ManualPpsOwner::Calibration,
                    job.next_request_mv,
                    Some(job.target_ma),
                ) {
                    Ok(()) => {
                        calibration.pps_enabled = true;
                        calibration.pps_mv = Some(job.next_request_mv);
                        calibration.pps_ma = Some(job.target_ma);
                        job.settle_ticks = 0;
                        job.stable_ticks = 0;
                        job.last_observed_mv = None;
                    }
                    Err(error) => {
                        calibration_job_fail(calibration, error, false, manual_pps);
                        return;
                    }
                }
            }

            let requested_mv = manual_pps.target_mv.unwrap_or(job.next_request_mv);
            let request_locked =
                (i64::from(requested_mv) - i64::from(job.next_request_mv)).abs() <= 100;
            let raw_adc_stable = job.last_observed_mv.is_some_and(|previous_mv| {
                (i32::from(previous_mv) - i32::from(latest_vin_raw_adc_mv)).abs() <= 8
            });
            let moved_from_previous_sample = if job.sample_count == 0 {
                true
            } else {
                job.samples[usize::from(job.sample_count.saturating_sub(1))]
                    .map(|sample| {
                        (i32::from(sample.observed_mv) - i32::from(latest_vin_raw_adc_mv)).abs()
                            >= i32::from(CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV)
                    })
                    .unwrap_or(true)
            };

            if request_locked {
                job.settle_ticks = job.settle_ticks.saturating_add(1);
            } else {
                job.settle_ticks = 0;
            }

            if job.settle_ticks >= 3 && raw_adc_stable && moved_from_previous_sample {
                job.stable_ticks = job.stable_ticks.saturating_add(1);
            } else {
                job.stable_ticks = 0;
            }
            job.last_observed_mv = Some(latest_vin_raw_adc_mv);

            if job.stable_ticks >= 2 {
                if usize::from(job.sample_count) >= job.samples.len() {
                    calibration_job_fail(
                        calibration,
                        ManualPpsError::WriteFailed,
                        false,
                        manual_pps,
                    );
                    return;
                }
                job.samples[usize::from(job.sample_count)] = Some(AdcCalibrationSample {
                    observed_mv: latest_vin_raw_adc_mv,
                    expected_mv: vin_adc_mv_for_input_mv(u32::from(job.next_request_mv)),
                    reference_temp_deci_c: None,
                    target_adc_mv: None,
                    reference_vin_mv: Some(job.next_request_mv),
                });
                job.sample_count = job.sample_count.saturating_add(1);
                calibration.job.samples_collected =
                    calibration.job.samples_collected.saturating_add(1);
                let next_mv = job.next_request_mv.saturating_add(1_000);
                if next_mv > job.max_request_mv {
                    let stored_samples = commit_vin_auto_samples_to_draft(
                        memory_config,
                        &job.samples,
                        usize::from(job.sample_count),
                    );
                    if stored_samples == 0 {
                        calibration_job_fail(
                            calibration,
                            ManualPpsError::WriteFailed,
                            false,
                            manual_pps,
                        );
                        return;
                    }
                    calibration_job_complete(
                        calibration,
                        CalibrationJobKind::VinAdcAuto,
                        calibration.job.samples_collected,
                        None,
                    );
                    return;
                }
                job.next_request_mv = next_mv;
                job.settle_ticks = 0;
                job.stable_ticks = 0;
                job.last_observed_mv = None;
                calibration.job.next_request_mv = Some(job.next_request_mv);
            }

            let span_mv = job
                .max_request_mv
                .saturating_sub(job.start_request_mv)
                .max(1);
            let done_mv = job
                .next_request_mv
                .saturating_sub(job.start_request_mv)
                .min(span_mv);
            calibration.job.progress_percent =
                ((u32::from(done_mv) * 100) / u32::from(span_mv)).min(99) as u8;
            calibration.job_data = Some(CalibrationJobData::VinAdcAuto(job));
        }
        CalibrationJobData::HeaterCurveAuto(mut job) => {
            if manual_pps.error.is_some() || !manual_pps.enabled {
                calibration_job_fail(
                    calibration,
                    manual_pps.error.unwrap_or(ManualPpsError::WriteFailed),
                    true,
                    manual_pps,
                );
                return;
            }

            if !calibration.heater_enabled {
                if job.started_ticks > 0 {
                    calibration_job_fail(
                        calibration,
                        ManualPpsError::WriteFailed,
                        true,
                        manual_pps,
                    );
                    return;
                }
                calibration.heater_enabled = true;
            }

            if latest_temp_c >= 80.0 {
                job.started_ticks = job.started_ticks.saturating_add(1);
            }
            if latest_temp_c >= 120.0 {
                job.stable_ticks = job.stable_ticks.saturating_add(1);
            } else {
                job.stable_ticks = 0;
            }

            if job.started_ticks > 0 && latest_vin_mv > 0 && pd_current_ma > 0 {
                let resistance_ohms = latest_vin_mv as f32 / pd_current_ma as f32;
                for bin in &mut job.bins {
                    if (*bin).contains(latest_temp_c) {
                        bin.observe(latest_temp_c, resistance_ohms);
                    }
                }
                calibration.job.samples_collected =
                    calibration.job.samples_collected.saturating_add(1);
            }

            calibration.job.progress_percent = round_to_u16_nonnegative(
                (latest_temp_c.clamp(120.0, 250.0) - 120.0) / (250.0 - 120.0) * 100.0,
            )
            .min(99) as u8;

            if latest_temp_c >= 250.0 && job.stable_ticks >= 3 {
                let Some(preview) = heater_curve_preview_from_auto_bins(&job.bins) else {
                    calibration_job_fail(
                        calibration,
                        ManualPpsError::WriteFailed,
                        true,
                        manual_pps,
                    );
                    return;
                };
                *preview_heater_curve = Some(preview);
                calibration.heater_enabled = false;
                if manual_pps.owner == ManualPpsOwner::Calibration {
                    manual_pps.clear();
                }
                calibration.pps_enabled = false;
                calibration.pps_mv = None;
                calibration.pps_ma = None;
                calibration_job_complete(
                    calibration,
                    CalibrationJobKind::HeaterCurveAuto,
                    calibration.job.samples_collected,
                    None,
                );
                return;
            }

            calibration.job.next_request_mv = calibration.pps_mv;
            calibration.job_data = Some(CalibrationJobData::HeaterCurveAuto(job));
        }
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_calibration_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) -> UsbFrame {
    let result = match config.op {
        CalibrationConfigOp::Capture => {
            let Some(channel) = config.channel else {
                return usb_error_response(
                    request_id,
                    "calibration_channel_required",
                    "Calibration capture requires a channel.",
                );
            };
            let observed_mv = config.observed_mv.unwrap_or(match channel {
                CalibrationChannelWire::RtdAdc => latest_rtd_raw_adc_mv,
                CalibrationChannelWire::VinAdc => latest_vin_raw_adc_mv,
            });
            let expected_mv = match expected_calibration_adc_mv(&config, channel) {
                Some(expected_mv) => expected_mv,
                None => {
                    return usb_error_response(
                        request_id,
                        "calibration_reference_required",
                        "Calibration capture requires a valid physical reference.",
                    );
                }
            };
            memory_config
                .adc_calibration
                .channel_mut(channel.as_memory_channel())
                .insert(AdcCalibrationSample {
                    observed_mv,
                    expected_mv,
                    reference_temp_deci_c: config.reference_temp_c.map(|temp_c| {
                        let scaled = if temp_c >= 0.0 {
                            temp_c * 10.0 + 0.5
                        } else {
                            temp_c * 10.0 - 0.5
                        };
                        (scaled as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
                    }),
                    target_adc_mv: config
                        .target_adc_mv
                        .filter(|_| channel == CalibrationChannelWire::RtdAdc),
                    reference_vin_mv: config
                        .reference_vin_mv
                        .and_then(|millivolts| u16::try_from(millivolts).ok())
                        .filter(|_| channel == CalibrationChannelWire::VinAdc),
                })
                .ok_or(ApiError::new(
                    "calibration_samples_full",
                    "Calibration channel already has 8 samples.",
                    false,
                ))
        }
        CalibrationConfigOp::Delete => {
            let Some(channel) = config.channel else {
                return usb_error_response(
                    request_id,
                    "calibration_channel_required",
                    "Calibration delete requires a channel.",
                );
            };
            let Some(index) = config.sample_index else {
                return usb_error_response(
                    request_id,
                    "calibration_index_required",
                    "Calibration delete requires sampleIndex.",
                );
            };
            if memory_config
                .adc_calibration
                .channel_mut(channel.as_memory_channel())
                .delete(index)
            {
                Ok(index)
            } else {
                Err(ApiError::new(
                    "calibration_sample_not_found",
                    "Calibration sample index was not present.",
                    false,
                ))
            }
        }
        CalibrationConfigOp::Clear => {
            let Some(channel) = config.channel else {
                return usb_error_response(
                    request_id,
                    "calibration_channel_required",
                    "Calibration clear requires a channel.",
                );
            };
            memory_config
                .adc_calibration
                .channel_mut(channel.as_memory_channel())
                .clear();
            Ok(0)
        }
        CalibrationConfigOp::Import => {
            let Some(state) = config.state else {
                return usb_error_response(
                    request_id,
                    "calibration_state_required",
                    "Calibration import requires a state.",
                );
            };
            import_calibration_state(memory_config, state);
            Ok(0)
        }
        CalibrationConfigOp::SetActiveSlot => {
            let Some(channel) = config.channel else {
                return usb_error_response(
                    request_id,
                    "calibration_channel_required",
                    "Calibration slot switch requires a channel.",
                );
            };
            let Some(slot) = config.slot else {
                return usb_error_response(
                    request_id,
                    "calibration_slot_required",
                    "Calibration slot switch requires a slot.",
                );
            };
            memory_config
                .adc_calibration
                .channel_mut(channel.as_memory_channel())
                .active_slot = slot.into();
            Ok(0)
        }
        CalibrationConfigOp::SetSlotFit => {
            let Some(channel) = config.channel else {
                return usb_error_response(
                    request_id,
                    "calibration_channel_required",
                    "Calibration slot fit update requires a channel.",
                );
            };
            let Some(slot) = config.slot else {
                return usb_error_response(
                    request_id,
                    "calibration_slot_required",
                    "Calibration slot fit update requires a slot.",
                );
            };
            let Some(fit) = config.fit else {
                return usb_error_response(
                    request_id,
                    "calibration_fit_required",
                    "Calibration slot fit update requires gain and offset.",
                );
            };
            *memory_config
                .adc_calibration
                .channel_mut(channel.as_memory_channel())
                .slot_fit_mut(slot.into()) = fit.to_memory();
            Ok(0)
        }
    };

    if let Err(error) = result {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(error),
        };
    }
    memory_config.sanitize();
    usb_response(
        request_id,
        UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn import_calibration_state(memory_config: &mut MemoryConfig, state: CalibrationStateWire) {
    import_calibration_channel_state(
        &mut memory_config.adc_calibration.rtd,
        state.rtd_adc.samples,
        state.rtd_adc.slots.a,
        state.rtd_adc.slots.b,
        state.rtd_adc.active_slot,
    );
    import_calibration_channel_state(
        &mut memory_config.adc_calibration.vin,
        state.vin_adc.samples,
        state.vin_adc.slots.a,
        state.vin_adc.slots.b,
        state.vin_adc.active_slot,
    );
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn import_calibration_channel_state(
    channel: &mut flux_purr_firmware::memory::AdcCalibrationChannelConfig,
    samples: [Option<CalibrationSampleWire>; ADC_CALIBRATION_MAX_SAMPLES],
    slot_a: CalibrationSlotFitWire,
    slot_b: CalibrationSlotFitWire,
    active_slot: CalibrationSlotIdWire,
) {
    channel.samples = samples_from_wire(samples);
    channel.slots.a = slot_a.to_memory();
    channel.slots.b = slot_b.to_memory();
    channel.active_slot = active_slot.into();
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_heater_curve_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: HeaterCurveConfigCommand,
    memory_config: &MemoryConfig,
    preview_heater_curve: &mut Option<HeaterCurveConfig>,
) -> UsbFrame {
    match config.op {
        HeaterCurveConfigOp::Preview => {
            let Some(package) = config.package else {
                return usb_error_response(
                    request_id,
                    "heater_curve_package_required",
                    "Heater curve preview requires a package.",
                );
            };
            let mut curve = package.to_memory();
            curve.points.sort_unstable_by_key(|point| {
                point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX)
            });
            *preview_heater_curve = Some(curve);
        }
        HeaterCurveConfigOp::ClearPreview => {
            *preview_heater_curve = None;
        }
    }
    usb_response(
        request_id,
        UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
            memory_config,
            preview_heater_curve.as_ref(),
        )),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_calibration_job_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: CalibrationJobCommandWire,
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
) -> UsbFrame {
    match command.op {
        CalibrationJobOpWire::Cancel => {
            calibration_job_canceled(calibration, manual_pps);
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(*calibration).job,
                ),
            )
        }
        CalibrationJobOpWire::Start => {
            let Some(kind) = command.kind.map(CalibrationJobKind::from) else {
                return usb_error_response(
                    request_id,
                    "calibration_job_kind_required",
                    "Calibration auto job requires a job kind.",
                );
            };
            if let Err(error) = calibration_job_start(calibration, kind, memory_config, manual_pps)
            {
                return usb_error_response(request_id, error.code(), error.message());
            }
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(*calibration).job,
                ),
            )
        }
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn expected_calibration_adc_mv(
    config: &CalibrationConfigCommand,
    channel: CalibrationChannelWire,
) -> Option<u16> {
    if let Some(expected_mv) = config.expected_mv {
        return Some(expected_mv);
    }

    match channel {
        CalibrationChannelWire::RtdAdc => config.target_adc_mv,
        CalibrationChannelWire::VinAdc => config.reference_vin_mv.map(vin_adc_mv_for_input_mv),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_write_frame(
    usb: &mut RawUsbSerialJtag,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    usb_write_frame_to(usb, frame, tx_buf);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_write_response_frame(
    usb: &mut RawUsbSerialJtag,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    usb_write_response_frame_to(usb, frame, tx_buf);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_write_frame_to<T: UsbControlTx>(
    tx: &mut T,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    if let Ok(line) = write_usb_frame(frame, tx_buf) {
        let _ = usb_write_bytes_bounded(tx, line.as_bytes());
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_write_response_frame_to<T: UsbControlTx>(
    tx: &mut T,
    frame: &UsbFrame,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    if let Ok(line) = write_usb_frame(frame, tx_buf) {
        let _ = usb_write_bytes_bounded(tx, line.as_bytes());
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UsbTxError {
    WouldBlock,
    #[cfg_attr(all(target_arch = "xtensa", feature = "web_serial"), allow(dead_code))]
    Other,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
trait UsbControlTx {
    fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError>;
    fn flush_tx_nb(&mut self) -> Result<(), UsbTxError>;
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
impl UsbControlTx for RawUsbSerialJtag {
    fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
        self.inner.write_byte_nb(byte).map_err(|err| match err {
            nb::Error::WouldBlock => UsbTxError::WouldBlock,
            nb::Error::Other(_) => UsbTxError::Other,
        })
    }

    fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
        self.inner.flush_tx_nb().map_err(|err| match err {
            nb::Error::WouldBlock => UsbTxError::WouldBlock,
            nb::Error::Other(_) => UsbTxError::Other,
        })
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_write_bytes_bounded<T: UsbControlTx>(tx: &mut T, bytes: &[u8]) -> bool {
    let mut packet_len = 0;
    for byte in bytes {
        let mut retries = 0;
        loop {
            match tx.write_byte_nb(*byte) {
                Ok(()) => {
                    packet_len += 1;
                    if packet_len >= USB_CONTROL_TX_PACKET_LEN {
                        if !usb_flush_tx_bounded(tx) {
                            return false;
                        }
                        packet_len = 0;
                    }
                    break;
                }
                Err(UsbTxError::WouldBlock) if retries < USB_CONTROL_TX_RETRY_LIMIT => {
                    retries += 1;
                    if !usb_flush_tx_bounded(tx) {
                        return false;
                    }
                    packet_len = 0;
                }
                Err(_) => return false,
            }
        }
    }

    usb_flush_tx_bounded(tx)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_flush_tx_bounded<T: UsbControlTx>(tx: &mut T) -> bool {
    for _ in 0..USB_CONTROL_TX_RETRY_LIMIT {
        match tx.flush_tx_nb() {
            Ok(()) => return true,
            Err(UsbTxError::WouldBlock) => {}
            Err(_) => return false,
        }
    }
    false
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    result: UsbResponsePayload,
) -> UsbFrame {
    UsbFrame::Response {
        request_id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_error_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
    message: &'static str,
) -> UsbFrame {
    usb_error_response_with_retryable(request_id, code, message, false)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_error_response_with_retryable(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
    message: &'static str,
    retryable: bool,
) -> UsbFrame {
    UsbFrame::Response {
        request_id,
        ok: false,
        result: None,
        error: Some(ApiError::new(code, message, retryable)),
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_early_response(line: &str, memory_config: &MemoryConfig) -> UsbFrame {
    match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => match op {
            UsbRequestOp::GetIdentity => usb_response(
                request_id,
                UsbResponsePayload::Identity(Identity::firmware_default()),
            ),
            UsbRequestOp::GetNetwork => usb_response(
                request_id,
                UsbResponsePayload::Network(network_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibration => usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibrationJob => usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(CalibrationRuntimeStateWire::default().job),
            ),
            UsbRequestOp::GetHeaterCurve => usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    memory_config,
                    None,
                )),
            ),
            UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
            UsbRequestOp::GetStatus => usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "Runtime status is not available until hardware initialization completes.",
                true,
            ),
        },
        Ok(UsbFrame::WifiConfig { request_id, .. })
        | Ok(UsbFrame::RuntimeConfig { request_id, .. })
        | Ok(UsbFrame::CalibrationJob { request_id, .. })
        | Ok(UsbFrame::CalibrationConfig { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Configuration writes are not available until hardware initialization completes.",
            true,
        ),
        Ok(UsbFrame::Response { request_id, .. }) => usb_error_response(
            request_id,
            "unsupported_frame",
            "Host response frames are ignored.",
        ),
        Ok(_) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
        },
        Err(UsbFrameError::MalformedJson) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
        },
        Err(UsbFrameError::OutputTooSmall) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new(
                "output_too_small",
                "USB JSONL frame exceeded buffer.",
                false,
            ),
        },
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn poll_usb_early_control(
    usb: &mut RawUsbSerialJtag,
    rx_line: &mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    memory_config: &MemoryConfig,
) {
    loop {
        match usb.read_byte() {
            Ok(b'\n') => {
                let response = usb_early_response(rx_line.as_str(), memory_config);
                usb_write_response_frame(usb, &response, tx_buf);
                rx_line.clear();
            }
            Ok(b'\r') => {}
            Ok(byte) => {
                if rx_line.push(char::from(byte)).is_err() {
                    rx_line.clear();
                }
            }
            Err(nb::Error::WouldBlock) => break,
            Err(_) => break,
        }
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn run_usb_recovery_control_loop(
    usb: &mut RawUsbSerialJtag,
    rx_line: &mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    memory_config: &MemoryConfig,
) -> ! {
    let mut elapsed_ms = 0_u64;
    loop {
        loop {
            match usb.read_byte() {
                Ok(b'\n') => {
                    let response =
                        usb_recovery_response(rx_line.as_str(), memory_config, elapsed_ms);
                    usb_write_response_frame(usb, &response, tx_buf);
                    rx_line.clear();
                }
                Ok(b'\r') => {}
                Ok(byte) => {
                    if rx_line.push(char::from(byte)).is_err() {
                        rx_line.clear();
                    }
                }
                Err(nb::Error::WouldBlock) => break,
                Err(_) => break,
            }
        }
        EmbassyTimer::after_millis(20).await;
        elapsed_ms = elapsed_ms.saturating_add(20);
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_recovery_status(memory_config: &MemoryConfig, elapsed_ms: u64) -> ControlPlaneStatus {
    let mut status = ControlPlaneStatus::from_device_status(
        DeviceStatus {
            mode: DeviceMode::Fault,
            voltage_mv: 0,
            current_ma: 0,
            board_temp_centi: 0,
            rtd_raw_adc_mv: 0,
            vin_raw_adc_mv: 0,
            pd_request_mv: DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
            pd_contract_mv: 0,
            pd_state: PdState::Fault,
            heater_output_percent: 0,
            heater_physical_output_percent: 0,
            fan_enabled: false,
            fan_pwm_permille: FAN_FULL_SPEED_PWM_PERMILLE,
            frontpanel_key: None,
        },
        memory_config,
        (elapsed_ms / 1_000).min(u64::from(u32::MAX)) as u32,
        network_from_memory(memory_config),
    );
    status.calibration = CalibrationRuntimeStateWire::default();
    status
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_recovery_response(line: &str, memory_config: &MemoryConfig, elapsed_ms: u64) -> UsbFrame {
    match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => match op {
            UsbRequestOp::GetIdentity => usb_response(
                request_id,
                UsbResponsePayload::Identity(Identity::firmware_default()),
            ),
            UsbRequestOp::GetNetwork => usb_response(
                request_id,
                UsbResponsePayload::Network(network_from_memory(memory_config)),
            ),
            UsbRequestOp::GetStatus => usb_response(
                request_id,
                UsbResponsePayload::Status(usb_recovery_status(memory_config, elapsed_ms)),
            ),
            UsbRequestOp::GetCalibration => usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibrationJob => usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(CalibrationJobStateWire::default()),
            ),
            UsbRequestOp::GetHeaterCurve => usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    memory_config,
                    None,
                )),
            ),
            UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
        },
        Ok(UsbFrame::WifiConfig { request_id, .. })
        | Ok(UsbFrame::RuntimeConfig { request_id, .. })
        | Ok(UsbFrame::CalibrationConfig { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "Runtime writes are unavailable because hardware bring-up did not complete.",
            true,
        ),
        Ok(UsbFrame::Response { request_id, .. }) => usb_error_response(
            request_id,
            "unsupported_frame",
            "Host response frames are ignored.",
        ),
        Ok(_) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
        },
        Err(UsbFrameError::MalformedJson) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
        },
        Err(UsbFrameError::OutputTooSmall) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new(
                "output_too_small",
                "USB JSONL frame exceeded buffer.",
                false,
            ),
        },
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn handle_usb_control_line(
    line: &str,
    usb: &mut RawUsbSerialJtag,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    controller: &mut FrontPanelInputController,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    preview_heater_curve: &mut Option<HeaterCurveConfig>,
    memory_commit_due_ms: &mut Option<u64>,
    memory_sequence: &mut u32,
    pd_i2c: &mut I2c<'_, esp_hal::Blocking>,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    elapsed_ms: u64,
    last_pd_observation: Option<PdStatusObservation>,
    heater_power_backend: &mut HeaterPowerBackend,
    heater_controller: &mut HeaterController,
    pid_snapshot: HeaterPidSnapshot,
    manual_pps: &mut ManualPpsState,
    fan_command: FanHardwareCommand,
    current_rtd_fault: Option<HeaterFaultReason>,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    last_raw_state: FrontPanelRawState,
    latest_temp_c: f32,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_vin_mv: u32,
    last_heater_duty: u8,
    heater_control_timing: HeaterControlTiming,
) -> bool {
    let mut needs_redraw = false;
    let active_thermal_control_profile = active_thermal_control_profile(
        memory_config,
        *thermal_control_profile_preview,
        manual_pps.capability_min_mv,
        manual_pps.capability_max_mv,
        manual_pps.capability_max_ma,
    );
    let runtime_context = UsbRuntimeStatusContext {
        elapsed_ms,
        last_pd_observation,
        heater_power_backend: *heater_power_backend,
        pid_snapshot,
        heater_control_timing,
        heater_physical_output_percent: last_heater_duty,
        manual_pps: *manual_pps,
        calibration: *calibration_runtime_state,
        fan_command,
        current_rtd_fault,
        heater_fault_latched: heater_controller.fault_latched(),
        thermal_control_profile_preview: thermal_control_profile_preview.is_some(),
        active_thermal_control_profile,
        last_raw_state,
        latest_temp_c,
        latest_rtd_raw_adc_mv,
        latest_vin_raw_adc_mv,
        vin_mv: latest_vin_mv,
    };
    let response = match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => match op {
            UsbRequestOp::GetIdentity => usb_response(
                request_id,
                UsbResponsePayload::Identity(Identity::firmware_default()),
            ),
            UsbRequestOp::GetNetwork => usb_response(
                request_id,
                UsbResponsePayload::Network(network_from_memory(memory_config)),
            ),
            UsbRequestOp::GetStatus => usb_response(
                request_id,
                UsbResponsePayload::Status(usb_runtime_status(
                    ui_state,
                    memory_config,
                    runtime_context,
                )),
            ),
            UsbRequestOp::GetCalibration => usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibrationJob => usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(*calibration_runtime_state).job,
                ),
            ),
            UsbRequestOp::GetHeaterCurve => usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    memory_config,
                    preview_heater_curve.as_ref(),
                )),
            ),
            UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
        },
        Ok(UsbFrame::WifiConfig { request_id, config }) => {
            config.apply_to(memory_config);
            apply_memory_config_to_ui(ui_state, memory_config);
            *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            needs_redraw = true;
            usb_response(
                request_id,
                UsbResponsePayload::Wifi(config.redacted_summary()),
            )
        }
        Ok(UsbFrame::RuntimeConfig { request_id, config }) => {
            let previous_memory_config = memory_config.clone();
            let heater_toggle_requested = config.heater_enabled.is_some();
            let heater_rearm_requested = config.heater_enabled == Some(true);
            if should_clear_runtime_fault_latch(
                heater_rearm_requested,
                current_rtd_fault,
                heater_controller.fault_latched(),
            ) {
                heater_controller.clear_fault_latch();
                info!("heater runtime re-arm -> cleared latched fault");
            }
            let (response, next_calibration_runtime_state) = usb_runtime_config_response(
                request_id,
                config,
                ui_state,
                memory_config,
                manual_pps,
                thermal_control_profile_preview,
                runtime_context,
            );
            *calibration_runtime_state = next_calibration_runtime_state;
            if heater_toggle_requested {
                controller.clear_pending_short_press(RawFrontPanelKey::CenterBoot);
            }
            if *memory_config != previous_memory_config {
                *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            }
            needs_redraw = true;
            response
        }
        Ok(UsbFrame::CalibrationConfig { request_id, config }) => {
            let previous_memory_config = memory_config.clone();
            let response = usb_calibration_config_response(
                request_id.clone(),
                config,
                memory_config,
                latest_rtd_raw_adc_mv,
                latest_vin_raw_adc_mv,
            );
            if *memory_config != previous_memory_config {
                if commit_memory_config_now(pd_i2c, memory_sequence, memory_config).await {
                    *memory_commit_due_ms = None;
                } else {
                    *memory_config = previous_memory_config;
                    usb_write_response_frame(
                        usb,
                        &usb_error_response(
                            request_id,
                            "memory_commit_failed",
                            "Calibration draft could not be persisted.",
                        ),
                        tx_buf,
                    );
                    return needs_redraw;
                }
            }
            response
        }
        Ok(UsbFrame::CalibrationJob {
            request_id,
            command,
        }) => usb_calibration_job_response(
            request_id,
            command,
            calibration_runtime_state,
            memory_config,
            manual_pps,
        ),
        Ok(UsbFrame::HeaterCurveConfig { request_id, config }) => usb_heater_curve_config_response(
            request_id,
            config,
            memory_config,
            preview_heater_curve,
        ),
        Ok(UsbFrame::HeaterCurveSave { request_id }) => {
            if let Some(preview) = *preview_heater_curve {
                let previous_memory_config = memory_config.clone();
                memory_config.active_heater_curve = preview;
                memory_config.sanitize();
                if commit_memory_config_now(pd_i2c, memory_sequence, memory_config).await {
                    *memory_commit_due_ms = None;
                } else {
                    *memory_config = previous_memory_config;
                    usb_write_response_frame(
                        usb,
                        &usb_error_response(
                            request_id,
                            "memory_commit_failed",
                            "Heater curve could not be persisted.",
                        ),
                        tx_buf,
                    );
                    return needs_redraw;
                }
                usb_response(
                    request_id,
                    UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                        memory_config,
                        preview_heater_curve.as_ref(),
                    )),
                )
            } else {
                usb_error_response(
                    request_id,
                    "heater_curve_preview_required",
                    "Heater curve save requires an active preview package.",
                )
            }
        }
        Ok(UsbFrame::Response { request_id, .. }) => usb_error_response(
            request_id,
            "unsupported_frame",
            "Host response frames are ignored.",
        ),
        Ok(_) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("unsupported_frame", "Unsupported USB frame type.", false),
        },
        Err(UsbFrameError::MalformedJson) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new("malformed_json", "Malformed USB JSONL frame.", false),
        },
        Err(UsbFrameError::OutputTooSmall) => UsbFrame::Error {
            request_id: None,
            error: ApiError::new(
                "output_too_small",
                "USB JSONL frame exceeded buffer.",
                false,
            ),
        },
    };

    usb_write_response_frame(usb, &response, tx_buf);
    needs_redraw
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

    info!(
        "ch224q request failed after {=u8} attempts; continuing with safe status-only fallback",
        CH224Q_RETRY_ATTEMPTS,
    );
    Address::Primary
}

#[cfg(target_arch = "xtensa")]
async fn write_ch224q_payload(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
    payload: &[u8],
) -> bool {
    for attempt in 1..=CH224Q_RETRY_ATTEMPTS {
        if i2c.write(address.as_u8(), payload).is_ok() {
            return true;
        }

        info!(
            "ch224q write retry={=u8}/{=u8} addr=0x{=u8:02x} reg=0x{=u8:02x}",
            attempt,
            CH224Q_RETRY_ATTEMPTS,
            address.as_u8(),
            payload.first().copied().unwrap_or(0),
        );
        EmbassyTimer::after_millis(CH224Q_RETRY_DELAY_MS).await;
    }

    false
}

#[cfg(target_arch = "xtensa")]
async fn request_ch224q_adjustable_voltage(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
    request_mv: u16,
    mode: ch224q::AdjustableVoltageMode,
    mode_changed: bool,
) -> bool {
    let original_request_mv = request_mv;
    let request_mv = clamp_ch224q_adjustable_request_mv(request_mv);
    if request_mv != original_request_mv {
        warn!(
            "ch224q adjustable request below hardware minimum requested_mv={=u16} clamped_mv={=u16}",
            original_request_mv, request_mv,
        );
    }

    let voltage_written = match mode {
        ch224q::AdjustableVoltageMode::Pps => {
            let Some(payload) = ch224q::pps_voltage_payload(request_mv) else {
                info!("ch224q pps request invalid mv={=u16}", request_mv);
                return false;
            };
            write_ch224q_payload(i2c, address, &payload).await
        }
        ch224q::AdjustableVoltageMode::Avs => {
            let Some((high_payload, low_payload)) = ch224q::avs_voltage_payloads(request_mv) else {
                info!("ch224q avs request invalid mv={=u16}", request_mv);
                return false;
            };
            write_ch224q_payload(i2c, address, &high_payload).await
                && write_ch224q_payload(i2c, address, &low_payload).await
        }
    };
    if !voltage_written {
        return false;
    }

    if mode_changed {
        let payload = ch224q::voltage_request_payload(mode.control_request());
        if !write_ch224q_payload(i2c, address, &payload).await {
            return false;
        }
    }

    info!(
        "ch224q adjustable request ok mode={=str} mv={=u16}",
        match mode {
            ch224q::AdjustableVoltageMode::Pps => "pps",
            ch224q::AdjustableVoltageMode::Avs => "avs",
        },
        request_mv,
    );
    true
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
fn read_ch224q_power_data(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    address: Address,
) -> Option<[u8; ch224q::PD_POWER_DATA_REGISTER_COUNT]> {
    let mut bytes = [0u8; ch224q::PD_POWER_DATA_REGISTER_COUNT];
    i2c.write_read(
        address.as_u8(),
        &[ch224q::PD_POWER_DATA_START_REGISTER],
        &mut bytes,
    )
    .ok()
    .map(|_| bytes)
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
    let reset_reason = reset_reason_log_line(esp_hal::system::reset_reason());
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);
    let runtime_mode = FrontPanelRuntimeMode::compile_time_default();
    #[cfg(feature = "web_serial")]
    let mut usb_serial = RawUsbSerialJtag::new(peripherals.USB_DEVICE);
    #[cfg(feature = "web_serial")]
    let mut usb_rx_line: heapless::String<USB_CONTROL_LINE_CAPACITY> = heapless::String::new();
    #[cfg(feature = "web_serial")]
    let mut usb_tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];
    #[cfg(feature = "web_serial")]
    let usb_boot_memory_config = MemoryConfig::default();
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(Identity::firmware_default()),
        &mut usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, reset_reason.as_bytes());
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        &mut usb_tx_buf,
        &usb_boot_memory_config,
    );

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

    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        &mut usb_tx_buf,
        &usb_boot_memory_config,
    );

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
    let display_ready = with_timeout(
        Duration::from_millis(DISPLAY_BRINGUP_TIMEOUT_MS),
        display.init(),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if !display_ready {
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            &mut usb_tx_buf,
            &usb_boot_memory_config,
        )
        .await;

        #[cfg(not(feature = "web_serial"))]
        panic!("failed to initialize GC9D01 display");
    }

    render_scene(SceneId::StartupCalibration, canvas);
    display.write_area(
        0,
        0,
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        canvas.pixels(),
    );
    let startup_flush_ready = with_timeout(
        Duration::from_millis(DISPLAY_BRINGUP_TIMEOUT_MS),
        display.flush(),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if !startup_flush_ready {
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            &mut usb_tx_buf,
            &usb_boot_memory_config,
        )
        .await;

        #[cfg(not(feature = "web_serial"))]
        panic!("failed to draw startup calibration screen");
    }
    info!("scene={=str}", SceneId::StartupCalibration.label());
    for _ in 0..45 {
        #[cfg(feature = "web_serial")]
        poll_usb_early_control(
            &mut usb_serial,
            &mut usb_rx_line,
            &mut usb_tx_buf,
            &usb_boot_memory_config,
        );
        EmbassyTimer::after_millis(20).await;
    }
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
    for _ in 0..(CH224Q_PD_SETTLE_MS / 10) {
        #[cfg(feature = "web_serial")]
        poll_usb_early_control(
            &mut usb_serial,
            &mut usb_rx_line,
            &mut usb_tx_buf,
            &usb_boot_memory_config,
        );
        EmbassyTimer::after_millis(10).await;
    }
    let restored_memory_record = load_memory_record(&mut pd_i2c);
    let mut memory_config = restored_memory_record
        .as_ref()
        .map(|record| record.config.clone())
        .unwrap_or_default();
    let mut preview_heater_curve: Option<HeaterCurveConfig> = None;
    let mut memory_sequence = restored_memory_record
        .as_ref()
        .map(|record| record.sequence)
        .unwrap_or(0);
    let mut memory_commit_due_ms: Option<u64> = None;
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(Identity::firmware_default()),
        &mut usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        &mut usb_tx_buf,
        &memory_config,
    );

    let mut adc1_config = AdcConfig::new();
    let mut vin_adc_pin = adc1_config
        .enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO1, RTD_SAMPLE_ATTENUATION);
    let mut rtd_adc_pin = adc1_config
        .enable_pin_with_cal::<_, AdcCalCurve<_>>(peripherals.GPIO2, RTD_SAMPLE_ATTENUATION);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);
    info!(
        "adc monitor active: vin_gpio1 rtd_gpio2 atten={=str} samples={=u8} interval_ms={=u64}",
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
    let _ = fan_pwm.set_duty_cycle_percent(pwm_percent_from_permille(
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
    ));
    info!(
        "fan runtime armed: gpio35 default=off gpio36 min_output={=u16}permille active_pwm_40_60={=u16}permille safety_half={=u16}permille full={=u16}permille freq={=u32}Hz active_min>={=i16}C cooldown_ms={=u64} active_full>{=i16}C pulse>{=i16}C lock>{=i16}C full>{=i16}C",
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
        FAN_ACTIVE_COOLING_PWM_PERMILLE,
        FAN_HALF_SPEED_PWM_PERMILLE,
        FAN_FULL_SPEED_PWM_PERMILLE,
        FAN_PWM_FREQUENCY_HZ,
        AUTO_COOLING_FAN_MIN_TEMP_C,
        AUTO_COOLING_FAN_COOLDOWN_MS,
        AUTO_COOLING_FAN_FULL_TEMP_C,
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

    mcpwm.operator2.set_timer(&mcpwm.timer2);
    let mut buzzer_pwm = mcpwm
        .operator2
        .with_pin_a(peripherals.GPIO48, PwmPinConfig::UP_ACTIVE_HIGH);
    let buzzer_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            BUZZER_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(BUZZER_IDLE_FREQUENCY_HZ),
        )
        .expect("failed to derive buzzer PWM timer clock");
    mcpwm.timer2.start(buzzer_timer_cfg);
    let _ = buzzer_pwm.set_duty_cycle_percent(0);
    info!(
        "buzzer runtime armed: gpio48 default=silent period_ticks={=u16}",
        BUZZER_PWM_PERIOD_TICKS,
    );
    let mut last_pd_observation = if let Some((status_raw, status, current_raw, current_ma)) =
        await_ch224q_pd_ready(&mut pd_i2c, ch224q_address).await
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
    let active_thermal_settings =
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
    let power_data_capabilities = read_ch224q_power_data(&mut pd_i2c, ch224q_address)
        .map(|bytes| ch224q::AdjustablePowerCapabilities::from_pd_power_data(&bytes));
    match power_data_capabilities {
        Some(capabilities) => info!(
            "ch224q power data pps20={=bool} pps_min_mv={=u16} pps_max_mv={=u16} pps_max_ma={=u16}",
            capabilities.pps_covers_20v,
            capabilities.pps_min_mv.unwrap_or(0),
            capabilities.pps_max_mv.unwrap_or(0),
            capabilities.pps_max_ma.unwrap_or(0),
        ),
        None => info!("ch224q power data read failed"),
    }
    let mut manual_pps_state = ManualPpsState::from_capabilities(power_data_capabilities);
    let mut calibration_runtime_state = CalibrationRuntimeState::default();
    let mut thermal_control_profile_preview: Option<ThermalControlProfile> = None;
    let mut heater_power_backend = select_heater_power_backend(
        power_data_capabilities,
        last_pd_observation.map(|status| status.status),
    );
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

    let initial_rtd_sample = read_rtd_sample(&mut adc1, &mut rtd_adc_pin, &memory_config);
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new(runtime_mode);
    ui_state.pd_contract_mv = heater_power_backend.pd_contract_mv();
    apply_memory_config_to_ui(&mut ui_state, &memory_config);
    let mut heater_controller = HeaterController::new();
    let mut current_rtd_fault: Option<HeaterFaultReason> = None;
    let mut latest_temp_c = 0.0_f32;
    let mut latest_temp_i16 = 0_i16;
    let mut latest_rtd_raw_adc_mv = 0_u16;
    let mut latest_vin_raw_adc_mv = 0_u16;
    let mut latest_vin_mv = 0_u32;
    let mut rtd_temporal_median = RtdTemporalMedian::default();
    match initial_rtd_sample {
        RtdSample::Valid(measurement) => {
            let control_temp_c = rtd_temporal_median.push(measurement.temp_c);
            latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
            latest_temp_c = control_temp_c;
            latest_temp_i16 = temp_c_to_whole_c(control_temp_c);
            if is_overtemp_sample(measurement.temp_c) {
                current_rtd_fault = Some(HeaterFaultReason::OverTemp);
                let _ = heater_controller.latch_fault(HeaterFaultReason::OverTemp);
                info!(
                    "heater initial fault latched reason={=str}",
                    HeaterFaultReason::OverTemp.label()
                );
            }
            ui_state.current_temp_c = latest_temp_i16;
            ui_state.current_temp_deci_c = temp_c_to_deci_c(control_temp_c);
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
            let _ = heater_controller.latch_fault(reason);
            ui_state.current_temp_c = 0;
            ui_state.current_temp_deci_c = 0;
            info!(
                "rtd initial fault adc_mv={=u16} reason={=str}",
                adc_mv.unwrap_or(0),
                reason.label(),
            );
        }
    }
    if let Some((raw_adc_mv, corrected_adc_mv, vin_mv)) =
        read_calibrated_vin_mv(&mut adc1, &mut vin_adc_pin, &memory_config)
    {
        latest_vin_raw_adc_mv = raw_adc_mv;
        latest_vin_mv = vin_mv;
        info!(
            "vin initial raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
            raw_adc_mv, corrected_adc_mv, vin_mv,
        );
    }
    let mut last_heater_duty = 0_u8;
    let mut last_pid_snapshot = HeaterPidSnapshot {
        duty_percent: 0,
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
    let mut last_fan_command: Option<FanHardwareCommand> = None;
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    let initial_fan_decision = fan_policy_decision(
        latest_temp_i16,
        0,
        ui_state.heater_enabled,
        ui_state.heater_output_percent,
        ui_state.active_cooling_enabled,
        fan_policy_state,
        is_sensor_fault(current_rtd_fault),
    );
    fan_policy_state = initial_fan_decision.state;
    let mut fan_command = initial_fan_decision.command;
    let _ = sync_frontpanel_runtime_state(
        &mut ui_state,
        initial_fan_decision,
        next_heater_lock_reason(
            heater_controller.fault_latched(),
            cooling_disabled_lock_latched,
        ),
        0,
    );
    let _ = apply_heater_power_output(
        &mut pd_i2c,
        ch224q_address,
        &mut heater_pwm,
        &mut heater_power_backend,
        &mut manual_pps_state,
        last_pd_observation,
        latest_temp_c,
        0,
        false,
        HeaterControlPhase::Warmup,
        0.0,
        0.05,
        &mut last_heater_duty,
        preview_heater_curve.as_ref(),
        &memory_config,
        active_thermal_settings,
        0,
    )
    .await;
    ui_state.pd_contract_mv = heater_power_backend.pd_contract_mv();
    apply_fan_output(
        &mut fan_enable,
        &mut fan_pwm,
        fan_command,
        &mut last_fan_command,
    );
    let mut buzzer = BuzzerController::new();
    let mut last_fault_present = current_rtd_fault.is_some();
    let mut attention_pending_after_fault_clear = false;
    let mut suppress_attention_ack_input = false;
    let mut suppress_attention_ack_waits_for_event = false;
    let mut suppress_attention_ack_event_seen = false;
    let mut suppress_attention_ack_clear_delay_ms = FRONTPANEL_DEBOUNCE_MS;
    let mut suppress_attention_ack_clear_after_ms: Option<u64> = None;
    let mut next_attention_reminder_ms: Option<u64> = None;
    let mut buzzer_output_applied = BuzzerHardwareState::default();
    if last_fault_present {
        let _ = buzzer.play(BuzzerCueId::ProtectionAlarm, 0);
    }
    apply_buzzer_output(
        &mut mcpwm.timer2,
        &mut buzzer_pwm,
        &pwm_clock_cfg,
        buzzer.tick(0),
        &mut buzzer_output_applied,
    );
    let initial_frontpanel_ui_ready = with_timeout(
        Duration::from_millis(DISPLAY_BRINGUP_TIMEOUT_MS),
        flush_ui(&mut display, canvas, &ui_state),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if !initial_frontpanel_ui_ready {
        #[cfg(feature = "web_serial")]
        run_usb_recovery_control_loop(
            &mut usb_serial,
            &mut usb_rx_line,
            &mut usb_tx_buf,
            &memory_config,
        )
        .await;

        #[cfg(not(feature = "web_serial"))]
        panic!("failed to draw initial frontpanel UI");
    }
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &log_frame("info", "frontpanel runtime ready"),
        &mut usb_tx_buf,
    );
    log_ui_state(&ui_state);

    let runtime_started_ms = Instant::now().as_millis();
    let mut last_control_ms: u64 = 0;
    let mut next_control_deadline_ms = HEATER_CONTROL_INTERVAL_MS;
    let mut heater_control_timing = HeaterControlTiming::default();
    loop {
        let elapsed_before_wait_ms = Instant::now()
            .as_millis()
            .saturating_sub(runtime_started_ms);
        EmbassyTimer::after_millis(heater_control_poll_wait_ms(
            elapsed_before_wait_ms,
            next_control_deadline_ms,
        ))
        .await;
        let elapsed_ms = Instant::now()
            .as_millis()
            .saturating_sub(runtime_started_ms);

        let raw_state = inputs.sample();
        let sample = controller.sample_with_capabilities(
            elapsed_ms,
            raw_state,
            ui_state.gesture_capabilities(),
        );
        let mut needs_redraw = false;
        #[cfg(feature = "web_serial")]
        loop {
            match usb_serial.read_byte() {
                Ok(b'\n') => {
                    needs_redraw |= handle_usb_control_line(
                        usb_rx_line.as_str(),
                        &mut usb_serial,
                        &mut usb_tx_buf,
                        &mut controller,
                        &mut ui_state,
                        &mut memory_config,
                        &mut preview_heater_curve,
                        &mut memory_commit_due_ms,
                        &mut memory_sequence,
                        &mut pd_i2c,
                        &mut calibration_runtime_state,
                        elapsed_ms,
                        last_pd_observation,
                        &mut heater_power_backend,
                        &mut heater_controller,
                        last_pid_snapshot,
                        &mut manual_pps_state,
                        fan_command,
                        current_rtd_fault,
                        &mut thermal_control_profile_preview,
                        last_raw_state,
                        latest_temp_c,
                        latest_rtd_raw_adc_mv,
                        latest_vin_raw_adc_mv,
                        latest_vin_mv,
                        last_heater_duty,
                        heater_control_timing,
                    )
                    .await;
                    usb_rx_line.clear();
                }
                Ok(b'\r') => {}
                Ok(byte) => {
                    if usb_rx_line.push(char::from(byte)).is_err() {
                        usb_rx_line.clear();
                    }
                }
                Err(nb::Error::WouldBlock) => break,
                Err(_) => break,
            }
        }

        if sample.raw_state != last_raw_state {
            if should_consume_attention_raw_input(
                attention_pending_after_fault_clear,
                suppress_attention_ack_input,
                last_raw_state,
                sample.raw_state,
            ) && consume_attention_input_if_pending(
                &mut attention_pending_after_fault_clear,
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
            if consume_attention_input_if_pending(
                &mut attention_pending_after_fault_clear,
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
            if interaction_handled {
                needs_redraw = true;
            }
            let mut specialized_feedback_played = false;
            if ui_state.active_cooling_enabled != active_cooling_enabled_before {
                let _ = buzzer.play(
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
                            let _ = buzzer.play(BuzzerCueId::HeaterReject, elapsed_ms);
                            specialized_feedback_played = true;
                            needs_redraw = true;
                            info!("heater re-arm blocked reason={=str}", reason.label(),);
                        } else {
                            heater_controller.clear_fault_latch();
                            let _ = buzzer.play(BuzzerCueId::HeaterOn, elapsed_ms);
                            specialized_feedback_played = true;
                            info!("heater re-arm -> cleared latched fault");
                        }
                    } else {
                        let _ = buzzer.play(BuzzerCueId::HeaterOn, elapsed_ms);
                        specialized_feedback_played = true;
                        info!("heater arm -> on");
                    }
                } else {
                    let _ = buzzer.play(BuzzerCueId::HeaterOff, elapsed_ms);
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
                manual_pps_state.capability_min_mv,
                manual_pps_state.capability_max_mv,
                manual_pps_state.capability_max_ma,
            );
            let active_thermal_settings = active_thermal_control_profile
                .map(|profile| profile.settings)
                .unwrap_or_default();

            match read_rtd_sample(&mut adc1, &mut rtd_adc_pin, &memory_config) {
                RtdSample::Valid(measurement) => {
                    let control_temp_c = rtd_temporal_median.push(measurement.temp_c);
                    latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
                    current_rtd_fault = if is_overtemp_sample(measurement.temp_c) {
                        Some(HeaterFaultReason::OverTemp)
                    } else {
                        None
                    };
                    latest_temp_c = control_temp_c;
                    latest_temp_i16 = temp_c_to_whole_c(control_temp_c);
                    if ui_state.current_temp_c != latest_temp_i16 {
                        ui_state.current_temp_c = latest_temp_i16;
                        needs_redraw = true;
                    }
                    let current_temp_deci_c = temp_c_to_deci_c(control_temp_c);
                    if ui_state.current_temp_deci_c != current_temp_deci_c {
                        ui_state.current_temp_deci_c = current_temp_deci_c;
                        needs_redraw = true;
                    }
                    info!(
                        "rtd sample raw_adc_mv={=u16} adc_mv={=u16} divider_mv={=u16} resistance_ohms={=f32} temp_c={=f32} heater_arm={=bool}",
                        measurement.raw_adc_mv,
                        measurement.adc_mv,
                        RTD_DIVIDER_SUPPLY_MV,
                        measurement.resistance_ohms,
                        measurement.temp_c,
                        ui_state.heater_enabled,
                    );
                }
                RtdSample::Fault { adc_mv, reason } => {
                    latest_rtd_raw_adc_mv = adc_mv.unwrap_or(0);
                    current_rtd_fault = Some(reason);
                    rtd_temporal_median.clear();
                    clear_runtime_temperature(&mut latest_temp_c, &mut latest_temp_i16);
                    if ui_state.current_temp_c != 0 || ui_state.current_temp_deci_c != 0 {
                        ui_state.current_temp_c = 0;
                        ui_state.current_temp_deci_c = 0;
                        needs_redraw = true;
                    }
                    info!(
                        "rtd fault adc_mv={=u16} reason={=str} heater_arm={=bool}",
                        adc_mv.unwrap_or(0),
                        reason.label(),
                        ui_state.heater_enabled,
                    );
                }
            }

            if let Some((raw_adc_mv, corrected_adc_mv, vin_mv)) =
                read_calibrated_vin_mv(&mut adc1, &mut vin_adc_pin, &memory_config)
            {
                latest_vin_raw_adc_mv = raw_adc_mv;
                if latest_vin_mv != vin_mv {
                    latest_vin_mv = vin_mv;
                    needs_redraw = true;
                }
                info!(
                    "vin sample raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
                    raw_adc_mv, corrected_adc_mv, vin_mv,
                );
            }

            if let Some(reason) = current_rtd_fault
                && heater_controller.latch_fault(reason)
            {
                ui_state.heater_enabled = false;
                needs_redraw = true;
                info!("heater fault latched reason={=str}", reason.label());
            }

            let fault_present = current_rtd_fault.is_some();
            let attention_state_changed = update_fault_attention_state(
                fault_present,
                &mut last_fault_present,
                &mut attention_pending_after_fault_clear,
                &mut next_attention_reminder_ms,
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
            let pid_snapshot = heater_controller.update(
                ui_state.target_temp_c,
                latest_temp_c,
                ui_state.heater_enabled,
                active_thermal_control_profile,
            );
            last_pid_snapshot = pid_snapshot;
            let requested_duty_percent = pid_snapshot.duty_percent;
            if ui_state.heater_output_percent != requested_duty_percent {
                ui_state.heater_output_percent = requested_duty_percent;
                needs_redraw = true;
            }
            if apply_heater_power_output(
                &mut pd_i2c,
                ch224q_address,
                &mut heater_pwm,
                &mut heater_power_backend,
                &mut manual_pps_state,
                current_pd_observation,
                latest_temp_c,
                requested_duty_percent,
                ui_state.heater_enabled,
                pid_snapshot.phase,
                pid_snapshot.error_c,
                active_thermal_control_profile
                    .map(|profile| profile.control_target(ui_state.target_temp_c))
                    .unwrap_or_else(|| default_thermal_control_target(ui_state.target_temp_c))
                    .hold_on_error_c,
                &mut last_heater_duty,
                preview_heater_curve.as_ref(),
                &memory_config,
                active_thermal_settings,
                elapsed_ms,
            )
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
            update_calibration_runtime_state(
                &mut calibration_runtime_state,
                &manual_pps_state,
                latest_rtd_raw_adc_mv,
                latest_vin_raw_adc_mv,
            );
            update_calibration_job_state(
                &mut calibration_runtime_state,
                &mut memory_config,
                &mut preview_heater_curve,
                &mut manual_pps_state,
                latest_rtd_raw_adc_mv,
                latest_vin_raw_adc_mv,
                latest_temp_c,
                current_pd_observation
                    .map(|observation| observation.current_ma)
                    .unwrap_or(0),
                latest_vin_mv,
            );
            if calibration_runtime_state.mode != CalibrationMode::Off {
                if calibration_runtime_state.heater_enabled
                    && current_rtd_fault.is_none()
                    && heater_controller.fault_latched().is_some()
                {
                    heater_controller.clear_fault_latch();
                    info!("calibration heater re-arm -> cleared latched fault");
                }
            }
            let desired_heater_enabled = reconcile_runtime_heater_enabled(
                ui_state.heater_enabled,
                calibration_runtime_state,
                current_rtd_fault,
                cooling_disabled_lock_latched,
                heater_controller.fault_latched().is_some(),
            );
            if ui_state.heater_enabled != desired_heater_enabled {
                ui_state.heater_enabled = desired_heater_enabled;
                needs_redraw = true;
            }
            let next_pd_contract_mv = manual_pps_state
                .target_mv
                .filter(|_| manual_pps_state.enabled)
                .unwrap_or_else(|| heater_power_backend.pd_contract_mv());
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

        if memory_commit_due_ms.is_some_and(|due_ms| elapsed_ms >= due_ms) {
            memory_commit_due_ms = None;
            let next_sequence = memory_sequence.saturating_add(1);
            let record = MemoryRecord {
                sequence: next_sequence,
                config: memory_config.clone(),
            };
            if write_memory_record(&mut pd_i2c, &record).await {
                memory_sequence = next_sequence;
            } else {
                memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            }
        }

        let (
            next_cooling_disabled_lock_latched,
            next_cooling_disabled_lock_armed,
            lock_just_latched,
        ) = reconcile_cooling_disabled_lock(
            ui_state.active_cooling_enabled,
            latest_temp_i16,
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
                latest_temp_i16
            );
        }

        if !ui_state.heater_enabled
            && (last_heater_duty != 0 || ui_state.heater_output_percent != 0)
        {
            ui_state.heater_output_percent = 0;
            let _ = apply_heater_power_output(
                &mut pd_i2c,
                ch224q_address,
                &mut heater_pwm,
                &mut heater_power_backend,
                &mut manual_pps_state,
                last_pd_observation,
                latest_temp_c,
                0,
                false,
                HeaterControlPhase::Warmup,
                0.0,
                0.05,
                &mut last_heater_duty,
                preview_heater_curve.as_ref(),
                &memory_config,
                active_thermal_settings,
                elapsed_ms,
            )
            .await;
            let next_pd_contract_mv = heater_power_backend.pd_contract_mv();
            if ui_state.pd_contract_mv != next_pd_contract_mv {
                ui_state.pd_contract_mv = next_pd_contract_mv;
            }
            needs_redraw = true;
        }

        let fan_decision = fan_policy_decision(
            latest_temp_i16,
            elapsed_ms,
            ui_state.heater_enabled,
            ui_state.heater_output_percent,
            ui_state.active_cooling_enabled,
            fan_policy_state,
            is_sensor_fault(current_rtd_fault),
        );
        fan_policy_state = fan_decision.state;
        fan_command = fan_decision.command;
        apply_fan_output(
            &mut fan_enable,
            &mut fan_pwm,
            fan_command,
            &mut last_fan_command,
        );

        if sync_frontpanel_runtime_state(
            &mut ui_state,
            fan_decision,
            next_heater_lock_reason(
                heater_controller.fault_latched(),
                cooling_disabled_lock_latched,
            ),
            elapsed_ms,
        ) {
            needs_redraw = true;
        }

        if current_rtd_fault.is_some() && buzzer.active_cue() != Some(BuzzerCueId::ProtectionAlarm)
        {
            let _ = buzzer.play(BuzzerCueId::ProtectionAlarm, elapsed_ms);
        }

        if maybe_play_attention_reminder(
            attention_pending_after_fault_clear,
            current_rtd_fault.is_some(),
            &mut next_attention_reminder_ms,
            &mut buzzer,
            elapsed_ms,
        ) {
            info!("fault attention reminder -> chirp");
        }

        apply_buzzer_output(
            &mut mcpwm.timer2,
            &mut buzzer_pwm,
            &pwm_clock_cfg,
            buzzer.tick(elapsed_ms),
            &mut buzzer_output_applied,
        );

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
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3,web_serial[,frontpanel-key-test]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeUsbTx {
        capacity: usize,
        pending: std::vec::Vec<u8>,
        sent: std::vec::Vec<u8>,
        flush_count: usize,
    }

    impl FakeUsbTx {
        fn new(capacity: usize) -> Self {
            Self {
                capacity,
                pending: std::vec::Vec::new(),
                sent: std::vec::Vec::new(),
                flush_count: 0,
            }
        }
    }

    impl UsbControlTx for FakeUsbTx {
        fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
            if self.pending.len() >= self.capacity {
                return Err(UsbTxError::WouldBlock);
            }
            self.pending.push(byte);
            Ok(())
        }

        fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
            self.flush_count += 1;
            self.sent.extend_from_slice(&self.pending);
            self.pending.clear();
            Ok(())
        }
    }

    fn test_usb_runtime_status_context() -> UsbRuntimeStatusContext {
        UsbRuntimeStatusContext {
            elapsed_ms: 0,
            last_pd_observation: None,
            heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
                reason: HeaterPowerBackendReason::NoPps20vCapability,
                fixed_request_confirmed: true,
                fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            },
            pid_snapshot: HeaterPidSnapshot {
                duty_percent: 0,
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
            thermal_control_profile_preview: false,
            active_thermal_control_profile: None,
            last_raw_state: FrontPanelRawState::default(),
            latest_temp_c: 0.0,
            latest_rtd_raw_adc_mv: 0,
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
    fn usb_response_write_uses_bounded_chunks_for_host_requested_frames() {
        let payload = std::vec![b'x'; 180];
        let mut bounded_tx = FakeUsbTx::new(0);
        assert!(!usb_write_bytes_bounded(&mut bounded_tx, &payload));

        let mut response_tx = FakeUsbTx::new(64);
        let mut request_id = heapless::String::new();
        request_id.push_str("response-write").unwrap();
        let response = usb_response(
            request_id,
            UsbResponsePayload::Identity(Identity::firmware_default()),
        );
        let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

        usb_write_response_frame_to(&mut response_tx, &response, &mut tx_buf);

        let line = core::str::from_utf8(&response_tx.sent).expect("response frame is utf8");
        assert!(line.contains(r#""requestId":"response-write""#));
        assert!(line.ends_with('\n'));
        assert!(response_tx.flush_count > 1);
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
            }
            other => panic!("unexpected early identity response: {other:?}"),
        }
    }

    #[test]
    fn early_usb_control_reports_network_from_available_memory() {
        let mut config = MemoryConfig::default();
        config.wifi_ssid.push_str("bench-net").unwrap();
        let response = usb_early_response(
            r#"{"type":"request","requestId":"boot-net","op":"get_network"}"#,
            &config,
        );

        match response {
            UsbFrame::Response {
                request_id,
                ok: true,
                result: Some(UsbResponsePayload::Network(network)),
                error: None,
            } => {
                assert_eq!(request_id.as_str(), "boot-net");
                assert_eq!(
                    network.ssid.as_ref().map(|ssid| ssid.as_str()),
                    Some("bench-net")
                );
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
    fn recovery_usb_control_reports_fault_status_when_bringup_fails() {
        let mut memory_config = MemoryConfig {
            target_temp_c: 215,
            ..MemoryConfig::default()
        };
        memory_config.wifi_ssid.push_str("bench-net").unwrap();
        let response = usb_recovery_response(
            r#"{"type":"request","requestId":"recovery-status","op":"get_status"}"#,
            &memory_config,
            7_200,
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
                heater_enabled: Some(true),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
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
                assert!(status.heater_enabled);
                assert_eq!(status.heater_lock_reason, None);
                assert_eq!(status.uptime_seconds, 12);
                assert_eq!(memory_config.target_temp_c, 240);
                assert!(!memory_config.active_cooling_enabled);
            }
            other => panic!("unexpected runtime config response: {other:?}"),
        }
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
                heater_enabled: None,
                manual_pps_enabled: Some(true),
                manual_pps_mv: Some(10_400),
                manual_pps_ma: Some(2_500),
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
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
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
        let mut profile_points = [None; FRONTPANEL_PRESET_COUNT];
        profile_points[0] = Some(ThermalControlProfilePointWire {
            target_temp_c: 210,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 260,
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

        let (response, _) = usb_runtime_config_response(
            request_id,
            RuntimeConfigCommand {
                target_temp_c: Some(210),
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
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

        match response {
            UsbFrame::Response {
                ok: true,
                result: Some(UsbResponsePayload::Status(status)),
                error: None,
                ..
            } => {
                assert!(!status.thermal_control_profile_preview);
                assert!(thermal_profile_preview.is_none());
                assert!(status.thermal_control.profile_active);
                assert!(status.thermal_control.profile_covers_target);
                assert_eq!(status.thermal_control.profile_source.as_str(), "saved");
                assert_eq!(status.thermal_control.warmup_power_permille, 260);
                assert_eq!(
                    status.thermal_control.approach_damping_exponent_permille,
                    1_000
                );
                assert_eq!(
                    memory_config.active_thermal_control_profile.points[0],
                    Some(ThermalControlProfilePointConfig {
                        target_temp_c: 210,
                        brake_distance_centi_c: 1_000,
                        warmup_power_permille: 260,
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
                    })
                );
            }
            other => panic!("unexpected runtime config response: {other:?}"),
        }
    }

    #[test]
    fn saved_thermal_profile_converts_to_controller_profile() {
        let mut config = ThermalControlProfileConfig::default();
        config.points[0] = Some(ThermalControlProfilePointConfig {
            target_temp_c: 210,
            brake_distance_centi_c: 1_000,
            warmup_power_permille: 260,
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
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.heater_control_phase.as_deref(), Some("hold"));
        assert_eq!(status.heater_error_c, Some(-0.4));
        assert_eq!(status.heater_control_error_c, Some(-0.2));
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
                latest_temp_c: 140.237,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.board_temp_centi, 14_024);
        assert_eq!(status.current_temp_c, 140.24);
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

        let context_manual_pps = manual_pps;
        let mut thermal_profile_preview = None;
        let (response, _) = usb_runtime_config_response(
            request_id,
            RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(true),
                manual_pps_mv: Some(10_400),
                manual_pps_ma: Some(2_500),
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
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
                },
                manual_pps: context_manual_pps,
                vin_mv: 20_000,
                ..test_usb_runtime_status_context()
            },
        );

        match response {
            UsbFrame::Response {
                ok: true,
                result: Some(UsbResponsePayload::Status(status)),
                error: None,
                ..
            } => {
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
            other => panic!("unexpected manual PPS response: {other:?}"),
        }

        let error = apply_manual_pps_config(
            &RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(true),
                manual_pps_mv: Some(10_450),
                manual_pps_ma: Some(2_500),
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            &mut manual_pps,
        )
        .unwrap_err();
        assert_eq!(error, ManualPpsError::InvalidVoltage);

        apply_manual_pps_config(
            &RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(false),
                manual_pps_mv: None,
                manual_pps_ma: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            &mut manual_pps,
        )
        .unwrap();
        assert!(!manual_pps.enabled);
        assert_eq!(manual_pps.target_mv, None);
        assert_eq!(manual_pps.target_ma, None);
        assert!(manual_pps.consume_automatic_restore_pending());
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
        let mut preview_heater_curve = None;
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
            CalibrationJobKind::VinAdcAuto,
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
                    &mut preview_heater_curve,
                    &mut manual_pps,
                    0,
                    vin_raw_mv,
                    25.0,
                    3_000,
                    latest_vin_mv,
                );
            }
        }

        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert_eq!(calibration.job.kind, Some(CalibrationJobKind::VinAdcAuto));
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
    fn vin_auto_job_waits_for_measured_voltage_to_settle_before_sampling() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::VinAdc,
            pps_ma: Some(3_000),
            ..CalibrationRuntimeState::default()
        };
        let mut memory_config = MemoryConfig::default();
        let mut preview_heater_curve = None;
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
            CalibrationJobKind::VinAdcAuto,
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
                    &mut preview_heater_curve,
                    &mut manual_pps,
                    0,
                    settled_raw_mv.saturating_sub(80),
                    25.0,
                    3_000,
                    u32::from(request_mv),
                );
            }
            assert_eq!(calibration.job.samples_collected, step as u8);

            for _ in 0..5 {
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut preview_heater_curve,
                    &mut manual_pps,
                    0,
                    settled_raw_mv,
                    25.0,
                    3_000,
                    u32::from(request_mv),
                );
            }
            assert_eq!(calibration.job.samples_collected, step as u8 + 1);
        }

        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert_eq!(memory_config.adc_calibration.vin.sample_count(), 8);
    }

    #[test]
    fn heater_control_saturates_when_far_below_target() {
        let mut controller = HeaterController::new();
        let snapshot = controller.update(380, 25.0, true, None);

        assert_eq!(
            snapshot.duty_percent,
            percent_from_permille(default_thermal_control_target(380).warmup_power_permille)
        );
        assert!(snapshot.error_c > 300.0);
        assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn heater_control_poll_wait_lands_on_the_next_deadline() {
        assert_eq!(
            heater_control_poll_wait_ms(0, HEATER_CONTROL_INTERVAL_MS),
            RUNTIME_INPUT_POLL_MAX_INTERVAL_MS.min(HEATER_CONTROL_INTERVAL_MS)
        );
        assert_eq!(
            heater_control_poll_wait_ms(
                HEATER_CONTROL_INTERVAL_MS
                    .saturating_mul(2)
                    .saturating_sub(17),
                HEATER_CONTROL_INTERVAL_MS.saturating_mul(2),
            ),
            17
        );
        assert_eq!(
            heater_control_poll_wait_ms(HEATER_CONTROL_INTERVAL_MS, HEATER_CONTROL_INTERVAL_MS),
            1
        );
    }

    #[test]
    fn heater_control_deadline_preserves_cadence_without_catch_up_updates() {
        assert_eq!(
            next_heater_control_deadline_ms(
                HEATER_CONTROL_INTERVAL_MS,
                HEATER_CONTROL_INTERVAL_MS + 4
            ),
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
        assert!(!warmup_handoff_ready(14.2, 20.4, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_accepts_confirmed_temperature_momentum() {
        assert!(warmup_handoff_ready(14.2, 14.8, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_uses_actual_temperature_inside_static_brake() {
        assert!(warmup_handoff_ready(4.9, 20.4, 5.0, 14.9));
    }

    #[test]
    fn heater_warmup_hands_off_early_when_rise_rate_is_high() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.filtered_temp_c = Some(44.5);
        controller.previous_filtered_temp_c = Some(43.8);
        controller.previous_measured_temp_c = Some(44.5);
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

        let snapshot = controller.update(60, 45.2, true, Some(profile));

        assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
        assert!(snapshot.error_c > 14.0);
        assert!(snapshot.duty_percent < 100);
    }

    #[test]
    fn heater_control_reduces_output_as_temperature_rises() {
        let mut controller = HeaterController::new();
        let mut snapshots = Vec::new();
        for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2] {
            snapshots.push(controller.update(100, measured, true, None));
        }

        assert_eq!(
            snapshots[0].duty_percent,
            percent_from_permille(default_thermal_control_target(100).warmup_power_permille)
        );
        assert!(snapshots[3].duty_percent >= snapshots[4].duty_percent);
        assert!(snapshots[5].duty_percent < snapshots[0].duty_percent);
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
    fn heater_control_uses_one_permille_profile_warmup_without_rounding_up() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.phase = HeaterControlPhase::Warmup;
        controller.filtered_temp_c = Some(39.0);
        controller.previous_filtered_temp_c = Some(37.0);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 1.0,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 60,
                    brake_distance_centi_c: 1_000,
                    warmup_power_permille: 1,
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
        assert_eq!(snapshot.duty_percent, 0);
        assert!(snapshot.error_c > 10.0);
    }

    #[test]
    fn heater_control_warmup_uses_profile_approach_power_cap() {
        let mut controller = HeaterController::new();
        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings::default(),
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 140,
                    brake_distance_centi_c: 1_000,
                    warmup_power_permille: 420,
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
        assert_eq!(snapshot.duty_percent, 42);
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

        for measured in [25.0, 40.0, 55.0, 70.0, 82.0, 90.0, 96.0, 99.2, 100.4] {
            let _ = controller.update(100, measured, true, None);
        }

        let _ = controller.update(100, 99.6, true, None);
        let _ = controller.update(100, 98.4, true, None);
        let cooling = controller.update(100, 98.8, true, None);
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
        for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2, 99.8, 100.3] {
            let _ = controller.update(100, measured, true, None);
        }

        let near_target = controller.update(100, 99.95, true, None);
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
        assert!(snapshot.duty_percent >= 32);
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
    fn heater_approach_keeps_reheat_floor_while_still_under_target() {
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
        assert!(snapshot.duty_percent >= 52);
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
    fn heater_approach_under_target_zero_output_uses_approach_floor() {
        let controller = HeaterController {
            phase: HeaterControlPhase::Approach,
            ..HeaterController::new()
        };
        let target = ThermalControlTarget {
            brake_distance_c: 4.5,
            warmup_power_permille: 1_000,
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
        assert_eq!(approach_sustain_floor_permille(target, 1.5), 340);
        assert_eq!(approach_sustain_floor_permille(target, 0.5), 180);
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
            heater_request_mv_from_power_percent(
                0,
                HEATER_ADJUSTABLE_MIN_MV,
                HEATER_ADJUSTABLE_MAX_MV
            ),
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
    fn heater_armed_zero_output_keeps_current_pps_request() {
        assert_eq!(
            heater_adjustable_request_mv(0, true, 11_300, 12_000, 6_100, 20_000),
            11_300
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
    fn adjustable_floor_gate_duty_coasts_after_crossing_target() {
        assert_eq!(
            adjustable_floor_gate_duty_percent(17, 6_100, 6_100, 14_500, true, 0.1, 0.3, 100, 0),
            100
        );
        assert_eq!(
            adjustable_floor_gate_duty_percent(17, 6_100, 6_100, 14_500, true, 0.0, 0.3, 100, 0),
            0
        );
    }

    #[test]
    fn adjustable_floor_gate_duty_reheats_after_hold_on_error() {
        assert_eq!(
            adjustable_floor_gate_duty_percent(17, 6_100, 6_100, 14_500, true, 0.2, 0.3, 0, 0),
            0
        );
        assert_eq!(
            adjustable_floor_gate_duty_percent(17, 6_100, 6_100, 14_500, true, 0.3, 0.3, 0, 0),
            100
        );
    }

    #[test]
    fn adjustable_floor_gate_duty_stays_static_once_request_leaves_floor() {
        assert_eq!(
            adjustable_floor_gate_duty_percent(17, 6_200, 6_100, 14_500, true, -0.5, 0.3, 100, 0),
            100
        );
        assert_eq!(
            adjustable_floor_gate_duty_percent(0, 6_100, 6_100, 14_500, true, 1.0, 0.3, 100, 0),
            0
        );
    }

    #[test]
    fn adjustable_floor_gate_duty_stays_static_outside_hold_phase() {
        assert_eq!(
            adjustable_floor_gate_duty_percent(5, 6_100, 6_100, 14_500, false, -0.5, 0.3, 100, 0),
            100
        );
    }

    #[test]
    fn adjustable_5v_approach_floor_distributes_requested_power_across_ticks() {
        assert_eq!(
            floor_gate_pulse_density_duty_percent(10, 5_000, 7_500, 0),
            100
        );
        assert_eq!(
            floor_gate_pulse_density_duty_percent(10, 5_000, 7_500, HEATER_CONTROL_INTERVAL_MS),
            0
        );
        assert_eq!(
            adjustable_floor_gate_duty_percent(10, 5_000, 5_000, 7_500, false, 2.0, 0.3, 0, 0),
            100
        );
        assert_eq!(
            adjustable_floor_gate_duty_percent(
                10,
                5_000,
                5_000,
                7_500,
                false,
                2.0,
                0.3,
                0,
                HEATER_CONTROL_INTERVAL_MS,
            ),
            0
        );
    }

    #[test]
    fn adjustable_5v_approach_compensates_during_pps_down_ramp() {
        let physical_ticks = (0..100_u64)
            .filter(|tick| {
                adjustable_floor_gate_duty_percent(
                    11,
                    13_500,
                    5_000,
                    14_000,
                    false,
                    6.0,
                    0.3,
                    100,
                    tick * HEATER_CONTROL_INTERVAL_MS,
                ) == 100
            })
            .count();
        assert_eq!(physical_ticks, 11);
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
        assert_eq!(
            resolve_thermal_profile_bank(
                ThermalProfileMode::Auto,
                Some(5_000),
                Some(21_000),
                Some(5_000),
            ),
            ThermalProfileBank::Pps5a
        );
        assert_eq!(
            resolve_thermal_profile_bank(
                ThermalProfileMode::Auto,
                Some(5_000),
                Some(21_000),
                Some(3_250),
            ),
            ThermalProfileBank::Pps3a
        );
        assert_eq!(
            resolve_thermal_profile_bank(ThermalProfileMode::W100, None, None, Some(3_250)),
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
        assert_eq!(pps_request_transition_ms(false), 25);
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
    fn effective_pps_current_limit_prefers_lower_live_status_when_valid() {
        let status_limit = effective_pps_current_limit_ma(
            3_000,
            Some(PdStatusObservation {
                status_raw: 0,
                status: Status {
                    pd_active: true,
                    ..Status::default()
                },
                current_raw: 40,
                current_ma: 2_000,
            }),
        );
        assert_eq!(status_limit, 2_000);

        let status_zero_falls_back = effective_pps_current_limit_ma(
            3_000,
            Some(PdStatusObservation {
                status_raw: 0,
                status: Status {
                    pd_active: true,
                    ..Status::default()
                },
                current_raw: 0,
                current_ma: 0,
            }),
        );
        assert_eq!(status_zero_falls_back, 3_000);

        let missing_status_falls_back = effective_pps_current_limit_ma(3_000, None);
        assert_eq!(missing_status_falls_back, 3_000);
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
            }
        );
        assert_eq!(
            heater_request_mv_from_power_percent(0, 15_000, 21_000),
            15_000
        );
    }

    #[test]
    fn auto_cooling_policy_runs_a_30_second_low_voltage_cooldown_below_40c() {
        let stopped = fan_policy_decision(39, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(stopped.command, FanHardwareCommand::disabled());
        assert_eq!(stopped.display_state, FanDisplayState::Auto);

        let active = fan_policy_decision(40, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(
            active.command,
            FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            }
        );
        assert_eq!(active.state, FanPolicyState::ActiveCooling);
        assert_eq!(active.display_state, FanDisplayState::Run);

        let still_active =
            fan_policy_decision(60, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(
            still_active.command,
            FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            }
        );

        let cooldown = fan_policy_decision(
            39,
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
            39,
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
            39,
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

        let heating_on =
            fan_policy_decision(110, 99, true, 32, true, FanPolicyState::Disabled, false);
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
    fn rtd_temporal_median_rejects_single_batch_shift() {
        let mut median = RtdTemporalMedian::default();
        assert!((median.push(60.0) - 60.0).abs() < f32::EPSILON);
        assert!((median.push(60.2) - 60.1).abs() < f32::EPSILON);
        assert!((median.push(58.7) - 60.0).abs() < f32::EPSILON);
        assert!((median.push(60.3) - 60.2).abs() < f32::EPSILON);
    }

    #[test]
    fn rtd_temporal_median_tracks_sustained_change_and_clears_after_fault() {
        let mut median = RtdTemporalMedian::default();
        median.push(59.8);
        median.push(60.0);
        assert!((median.push(60.2) - 60.0).abs() < f32::EPSILON);
        assert!((median.push(60.4) - 60.2).abs() < f32::EPSILON);

        median.clear();
        assert!((median.push(42.0) - 42.0).abs() < f32::EPSILON);
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
    fn rtd_oversampling_accepts_partial_batch_with_enough_valid_conversions() {
        let valid_samples = RTD_MIN_VALID_SAMPLE_COUNT;
        let sum_mv = (900 * valid_samples) + ((valid_samples * 3) / 8);
        let mean_mv = rtd_fractional_mean_mv(sum_mv as u32, valid_samples).unwrap();

        assert!((mean_mv - 900.375).abs() < 0.001);
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

        let pulse_on =
            fan_policy_decision(110, 0, false, 0, false, FanPolicyState::Disabled, false);
        assert!(pulse_on.command.enabled);
        assert_eq!(pulse_on.display_state, FanDisplayState::Off);
        assert_eq!(
            pulse_on.command.pwm_permille,
            FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
        );

        let pulse_off =
            fan_policy_decision(110, 200, false, 0, false, FanPolicyState::Disabled, false);
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
            FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            }
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

        let (rearmed_latched, rearmed_armed, rearmed_just_latched) =
            reconcile_cooling_disabled_lock(
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
    fn fault_attention_transitions_alarm_to_pending_reminder() {
        let mut last_fault_present = false;
        let mut attention_pending = false;
        let mut next_reminder_ms = None;
        let mut buzzer = BuzzerController::new();

        assert!(update_fault_attention_state(
            true,
            &mut last_fault_present,
            &mut attention_pending,
            &mut next_reminder_ms,
            &mut buzzer,
            3_000,
        ));
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
        assert!(!attention_pending);
        assert_eq!(next_reminder_ms, None);

        assert!(update_fault_attention_state(
            false,
            &mut last_fault_present,
            &mut attention_pending,
            &mut next_reminder_ms,
            &mut buzzer,
            8_000,
        ));
        assert_eq!(buzzer.active_cue(), None);
        assert!(attention_pending);
        assert_eq!(
            next_reminder_ms,
            Some(8_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
        );
    }

    #[test]
    fn attention_pending_consumes_first_input_and_stops_reminders() {
        let mut attention_pending = true;
        let mut next_reminder_ms = Some(15_000);
        let mut buzzer = BuzzerController::new();
        let _ = buzzer.play(BuzzerCueId::AttentionReminder, 10_000);

        assert!(consume_attention_input_if_pending(
            &mut attention_pending,
            &mut next_reminder_ms,
            &mut buzzer,
        ));
        assert!(!attention_pending);
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
        let mut buzzer = BuzzerController::new();

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
        let mut buzzer = BuzzerController::new();

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
        let mut buzzer = BuzzerController::new();

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
        let config = MemoryConfig {
            target_temp_c: 180,
            active_cooling_enabled: false,
            ..MemoryConfig::default()
        };

        apply_memory_config_to_ui(&mut state, &config);

        assert!(!state.heater_enabled);
        let persisted = memory_config_from_ui(&state, &config);
        assert_eq!(persisted.target_temp_c, 180);
        assert!(!persisted.active_cooling_enabled);
    }

    #[test]
    fn runtime_heater_reconcile_preserves_dashboard_heater_when_calibration_is_off() {
        let desired = reconcile_runtime_heater_enabled(
            true,
            CalibrationRuntimeState::default(),
            None,
            false,
            false,
        );

        assert!(desired);
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
        );

        assert!(!desired);
    }
}
