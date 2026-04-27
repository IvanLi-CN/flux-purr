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
};
#[cfg(target_arch = "xtensa")]
use esp_println as _;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::buzzer::BuzzerOutput;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::buzzer::{BuzzerController, BuzzerCueId};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::frontpanel::{
    FanDisplayState, FrontPanelRawState, FrontPanelUiState, HeaterLockReason,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::memory::MemoryConfig;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::memory::{
    M24C64_PAGE_SIZE, M24c64, MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_B_OFFSET, MEMORY_SLOT_SIZE,
    MEMORY_WRITE_DEBOUNCE_MS, MemoryRecord, decode_memory_record, encode_memory_record,
    select_latest_memory_record,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    DEFAULT_PD_VOLTAGE_REQUEST, FAN_PWM_FREQUENCY_HZ, pwm_percent_from_permille,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    adapters::ch224q::{self, Address, Status},
    board::s3_frontpanel,
    display::{DISPLAY_PANEL_CONFIG, DisplayCanvas, SceneId, render_scene},
    frontpanel::{
        FRONTPANEL_DEBOUNCE_MS, FRONTPANEL_DOUBLE_CLICK_MS, FrontPanelInputController,
        FrontPanelInputTimings, FrontPanelKeyMap, FrontPanelRoute, FrontPanelRuntimeMode,
        KeyGesture, RawFrontPanelKey, render::render_frontpanel_ui,
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
const HEATER_CONTROL_INTERVAL_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
const DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_PULSE_PERIOD_MS: u64 = 10_000;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_FULL_SPEED_PWM_PERMILLE: u16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_ACTIVE_COOLING_PWM_PERMILLE: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_HALF_SPEED_PWM_PERMILLE: u16 = 250;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE: u16 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_WARMUP_EXIT_ERROR_C: f32 = 2.2;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_WARMUP_REENTER_ERROR_C: f32 = 4.0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_ENTRY_ERROR_C: f32 = 0.9;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_EXIT_ERROR_C: f32 = 2.0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_APPROACH_DUTY_PERCENT: u8 = 32;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_APPROACH_MAX_TICKS: u8 = 5;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_DUTY_PERCENT: u8 = 32;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_ON_ERROR_C: f32 = 0.3;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_OFF_ERROR_C: f32 = 0.05;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_OVERSHOOT_CUTOFF_C: f32 = 0.25;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_TEMP_FILTER_ALPHA: f32 = 0.45;
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
    phase: HeaterControlPhase,
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
    phase: HeaterControlPhase,
    phase_ticks: u8,
    duty_percent: u8,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HeaterController {
    const fn new() -> Self {
        Self {
            fault_latched: None,
            last_target_temp_c: 0,
            filtered_temp_c: None,
            phase: HeaterControlPhase::Warmup,
            phase_ticks: 0,
            duty_percent: 0,
        }
    }

    const fn fault_latched(self) -> Option<HeaterFaultReason> {
        self.fault_latched
    }

    fn clear_fault_latch(&mut self) {
        self.fault_latched = None;
        self.filtered_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
        self.duty_percent = 0;
    }

    fn latch_fault(&mut self, reason: HeaterFaultReason) -> bool {
        let changed = self.fault_latched != Some(reason);
        self.fault_latched = Some(reason);
        self.filtered_temp_c = None;
        self.phase = HeaterControlPhase::Warmup;
        self.phase_ticks = 0;
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
        let last_target_temp_c = self.last_target_temp_c;
        self.last_target_temp_c = target_temp_c;

        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
        }

        if !heater_enabled || self.fault_latched.is_some() {
            self.filtered_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.duty_percent = 0;
            return HeaterPidSnapshot {
                duty_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                control_error_c: f32::from(target_temp_c) - measured_temp_c,
                filtered_temp_c: measured_temp_c,
                phase: self.phase,
            };
        }

        if target_temp_c != last_target_temp_c {
            self.filtered_temp_c = Some(measured_temp_c);
            self.phase = HeaterControlPhase::Warmup;
            self.phase_ticks = 0;
            self.duty_percent = 0;
        }

        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let filtered_temp_c = if let Some(previous_filtered_temp_c) = self.filtered_temp_c {
            previous_filtered_temp_c
                + HEATER_TEMP_FILTER_ALPHA * (measured_temp_c - previous_filtered_temp_c)
        } else {
            measured_temp_c
        };
        self.filtered_temp_c = Some(filtered_temp_c);
        let control_error_c = f32::from(target_temp_c) - filtered_temp_c;

        let mut next_phase = self.phase;
        let previous_phase = self.phase;
        match self.phase {
            HeaterControlPhase::Warmup => {
                if control_error_c <= HEATER_WARMUP_EXIT_ERROR_C {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
            HeaterControlPhase::Approach => {
                if control_error_c >= HEATER_WARMUP_REENTER_ERROR_C {
                    next_phase = HeaterControlPhase::Warmup;
                } else if control_error_c <= HEATER_HOLD_ENTRY_ERROR_C
                    || self.phase_ticks >= HEATER_APPROACH_MAX_TICKS
                {
                    next_phase = HeaterControlPhase::Hold;
                }
            }
            HeaterControlPhase::Hold => {
                if control_error_c >= HEATER_HOLD_EXIT_ERROR_C {
                    next_phase = HeaterControlPhase::Approach;
                }
            }
        }

        if next_phase != self.phase {
            self.phase = next_phase;
            self.phase_ticks = 0;
        } else {
            self.phase_ticks = self.phase_ticks.saturating_add(1);
        }

        let previous_duty_percent =
            if previous_phase != self.phase && self.phase == HeaterControlPhase::Hold {
                0
            } else {
                self.duty_percent
            };
        let duty_percent =
            if measured_temp_c >= f32::from(target_temp_c) + HEATER_OVERSHOOT_CUTOFF_C {
                0
            } else {
                match self.phase {
                    HeaterControlPhase::Warmup => 100,
                    HeaterControlPhase::Approach => HEATER_APPROACH_DUTY_PERCENT,
                    HeaterControlPhase::Hold => {
                        if previous_duty_percent >= HEATER_HOLD_DUTY_PERCENT {
                            if control_error_c > HEATER_HOLD_OFF_ERROR_C {
                                HEATER_HOLD_DUTY_PERCENT
                            } else {
                                0
                            }
                        } else if control_error_c >= HEATER_HOLD_ON_ERROR_C {
                            HEATER_HOLD_DUTY_PERCENT
                        } else {
                            0
                        }
                    }
                }
            };

        self.duty_percent = duty_percent;

        HeaterPidSnapshot {
            duty_percent,
            error_c,
            control_error_c,
            filtered_temp_c,
            phase: self.phase,
        }
    }
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
    active_cooling_enabled: bool,
    previous_state: FanPolicyState,
    hold_previous_output: bool,
) -> FanPolicyDecision {
    let state = if hold_previous_output {
        previous_state
    } else if heater_enabled {
        cooling_disabled_state(current_temp_c)
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
    let selected = select_latest_memory_record(slot_a_read, slot_b_read);

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
    let restored_memory_record = load_memory_record(&mut pd_i2c);
    let mut memory_config = restored_memory_record
        .as_ref()
        .map(|record| record.config.clone())
        .unwrap_or_default();
    let mut memory_sequence = restored_memory_record
        .as_ref()
        .map(|record| record.sequence)
        .unwrap_or(0);
    let mut memory_commit_due_ms: Option<u64> = None;

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
    info!(
        "heater control policy mode=staged interval_ms={=u64} warmup_exit={=f32}C warmup_reenter={=f32}C hold_entry={=f32}C hold_exit={=f32}C approach_hi={=u8}% approach_lo={=u8}% approach_max_s={=u8} hold_hi={=u8}% hold_lo={=u8}%",
        HEATER_CONTROL_INTERVAL_MS,
        HEATER_WARMUP_EXIT_ERROR_C,
        HEATER_WARMUP_REENTER_ERROR_C,
        HEATER_HOLD_ENTRY_ERROR_C,
        HEATER_HOLD_EXIT_ERROR_C,
        HEATER_APPROACH_DUTY_PERCENT,
        HEATER_APPROACH_DUTY_PERCENT,
        HEATER_APPROACH_MAX_TICKS,
        HEATER_HOLD_DUTY_PERCENT,
        0_u8,
    );

    let initial_rtd_sample = read_rtd_sample(&mut adc1, &mut rtd_adc_pin);
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new(runtime_mode);
    ui_state.pd_contract_mv = DEFAULT_PD_VOLTAGE_REQUEST.millivolts();
    apply_memory_config_to_ui(&mut ui_state, &memory_config);
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
            ui_state.current_temp_deci_c = temp_c_to_deci_c(measurement.temp_c);
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
            ui_state.current_temp_deci_c = 0;
            info!(
                "rtd initial fault adc_mv={=u16} reason={=str}",
                adc_mv.unwrap_or(0),
                reason.label(),
            );
        }
    }
    let mut last_heater_duty = 0_u8;
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
    apply_heater_duty(&mut heater_pwm, 0, &mut last_heater_duty);
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
        let sample = controller.sample_with_capabilities(
            elapsed_ms,
            raw_state,
            ui_state.gesture_capabilities(),
        );
        let mut needs_redraw = false;

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
                    let current_temp_deci_c = temp_c_to_deci_c(measurement.temp_c);
                    if ui_state.current_temp_deci_c != current_temp_deci_c {
                        ui_state.current_temp_deci_c = current_temp_deci_c;
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
            let next_pd_contract_mv = DEFAULT_PD_VOLTAGE_REQUEST.millivolts();
            if ui_state.pd_contract_mv != next_pd_contract_mv {
                ui_state.pd_contract_mv = next_pd_contract_mv;
                needs_redraw = true;
            }

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
                "heater loop set_c={=i16} temp_c={=f32} duty={=u8}% error_c={=f32} control_error_c={=f32} temp_avg_c={=f32} phase={=str} arm={=bool} fault={=str}",
                ui_state.target_temp_c,
                latest_temp_c,
                pid_snapshot.duty_percent,
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
            apply_heater_duty(&mut heater_pwm, 0, &mut last_heater_duty);
            needs_redraw = true;
        }

        let fan_decision = fan_policy_decision(
            latest_temp_i16,
            elapsed_ms,
            ui_state.heater_enabled,
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
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3[,frontpanel-key-test]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heater_control_saturates_when_far_below_target() {
        let mut controller = HeaterController::new();
        let snapshot = controller.update(380, 25.0, true);

        assert_eq!(snapshot.duty_percent, 100);
        assert!(snapshot.error_c > 300.0);
        assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn heater_control_reduces_output_as_temperature_rises() {
        let mut controller = HeaterController::new();
        let mut snapshots = Vec::new();
        for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2] {
            snapshots.push(controller.update(100, measured, true));
        }

        assert_eq!(snapshots[0].duty_percent, 100);
        assert!(snapshots[3].duty_percent >= snapshots[4].duty_percent);
        assert!(snapshots[4].duty_percent >= snapshots[5].duty_percent);
    }

    #[test]
    fn heater_control_stays_aggressive_through_approach_band() {
        let mut controller = HeaterController::new();
        let mut snapshot = controller.update(100, 25.0, true);
        for measured in [40.0, 60.0, 80.0, 92.0, 96.0, 96.0, 96.0] {
            snapshot = controller.update(100, measured, true);
        }

        assert!(snapshot.duty_percent >= HEATER_APPROACH_DUTY_PERCENT);
    }

    #[test]
    fn heater_control_resets_when_disabled() {
        let mut controller = HeaterController::new();
        let enabled = controller.update(380, 25.0, true);
        let disabled = controller.update(380, 40.0, false);

        assert!(enabled.duty_percent > 0);
        assert_eq!(disabled.duty_percent, 0);
        assert_eq!(disabled.filtered_temp_c, 40.0);
        assert_eq!(disabled.phase, HeaterControlPhase::Warmup);
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
    fn heater_control_reapplies_power_when_temperature_falls_below_target() {
        let mut controller = HeaterController::new();

        for measured in [25.0, 40.0, 55.0, 70.0, 82.0, 90.0, 96.0, 99.2, 100.4] {
            let _ = controller.update(100, measured, true);
        }

        let _ = controller.update(100, 99.6, true);
        let _ = controller.update(100, 98.4, true);
        let cooling = controller.update(100, 98.8, true);
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
            let _ = controller.update(100, measured, true);
        }

        let overshoot = controller.update(100, 100.3, true);
        assert_eq!(overshoot.duty_percent, 0);
    }

    #[test]
    fn auto_cooling_policy_runs_a_30_second_low_voltage_cooldown_below_40c() {
        let stopped = fan_policy_decision(39, 0, false, true, FanPolicyState::Disabled, false);
        assert_eq!(stopped.command, FanHardwareCommand::disabled());
        assert_eq!(stopped.display_state, FanDisplayState::Auto);

        let active = fan_policy_decision(40, 0, false, true, FanPolicyState::Disabled, false);
        assert_eq!(
            active.command,
            FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            }
        );
        assert_eq!(active.state, FanPolicyState::ActiveCooling);
        assert_eq!(active.display_state, FanDisplayState::Run);

        let still_active = fan_policy_decision(60, 0, false, true, FanPolicyState::Disabled, false);
        assert_eq!(
            still_active.command,
            FanHardwareCommand {
                enabled: true,
                pwm_permille: FAN_ACTIVE_COOLING_PWM_PERMILLE,
            }
        );

        let cooldown =
            fan_policy_decision(39, 1_000, false, true, FanPolicyState::ActiveCooling, false);
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
            true,
            FanPolicyState::ActiveCoolingCooldown { until_ms: 31_000 },
            false,
        );
        assert_eq!(
            stopped_after_cooldown.command,
            FanHardwareCommand::disabled()
        );

        let full = fan_policy_decision(61, 0, false, true, FanPolicyState::Disabled, false);
        assert_eq!(
            full.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
        );
    }

    #[test]
    fn heater_enabled_does_not_use_idle_auto_cooling_thresholds() {
        let heating_below_100 =
            fan_policy_decision(41, 0, true, true, FanPolicyState::Disabled, false);
        assert_eq!(heating_below_100.command, FanHardwareCommand::disabled());
        assert_eq!(heating_below_100.display_state, FanDisplayState::Auto);

        let heating_with_policy_off =
            fan_policy_decision(41, 0, true, false, FanPolicyState::Disabled, false);
        assert_eq!(
            heating_with_policy_off.command,
            FanHardwareCommand::disabled()
        );
        assert_eq!(heating_with_policy_off.display_state, FanDisplayState::Off);

        let heating_over_100 =
            fan_policy_decision(110, 0, true, true, FanPolicyState::Disabled, false);
        assert!(heating_over_100.command.enabled);
        assert_eq!(
            heating_over_100.command.pwm_permille,
            FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
        );
        assert_eq!(heating_over_100.display_state, FanDisplayState::Run);
    }

    #[test]
    fn overtemp_threshold_uses_unrounded_temperature() {
        assert!(!is_overtemp_sample(419.9));
        assert!(is_overtemp_sample(420.0));
    }

    #[test]
    fn cooling_disabled_policy_uses_pulse_window_and_safety_steps() {
        assert_eq!(cooling_disabled_pulse_duty_percent(100), 0);
        assert_eq!(cooling_disabled_pulse_duty_percent(110), 1);
        assert_eq!(cooling_disabled_pulse_duty_percent(350), 25);

        let pulse_on = fan_policy_decision(110, 0, false, false, FanPolicyState::Disabled, false);
        assert!(pulse_on.command.enabled);
        assert_eq!(pulse_on.display_state, FanDisplayState::Off);
        assert_eq!(
            pulse_on.command.pwm_permille,
            FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE
        );

        let pulse_off =
            fan_policy_decision(110, 200, false, false, FanPolicyState::Disabled, false);
        assert!(!pulse_off.command.enabled);

        let half = fan_policy_decision(351, 0, false, false, FanPolicyState::Disabled, false);
        assert_eq!(
            half.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::SafeHalf)
        );
        assert_eq!(half.display_state, FanDisplayState::Off);

        let full = fan_policy_decision(361, 0, false, false, FanPolicyState::Disabled, false);
        assert_eq!(
            full.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
        );
        assert_eq!(full.display_state, FanDisplayState::Off);
    }

    #[test]
    fn rtd_sensor_fault_keeps_existing_policy_state() {
        let auto = fan_policy_decision(0, 0, false, true, FanPolicyState::ActiveCooling, true);
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
}
