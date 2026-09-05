#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(all(
    target_arch = "xtensa",
    any(feature = "net_http", feature = "web_serial")
))]
use core::fmt::Write as _;
#[cfg(target_arch = "xtensa")]
extern crate alloc;
#[cfg(target_arch = "xtensa")]
use alloc::{
    alloc::{Layout, alloc},
    boxed::Box,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
use core::cell::RefCell;
#[cfg(any(target_arch = "xtensa", test))]
use core::sync::atomic::{AtomicU8, AtomicU16, Ordering};
#[cfg(target_arch = "xtensa")]
use core::{mem::MaybeUninit, panic::PanicInfo};
#[cfg(target_arch = "xtensa")]
use defmt::{info, warn};
#[cfg(target_arch = "xtensa")]
use embassy_embedded_hal::adapter::BlockingAsync;
#[cfg(target_arch = "xtensa")]
use embassy_executor::Spawner;
#[cfg(target_arch = "xtensa")]
use embassy_futures::select::{Either, Either3, select, select3};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
#[cfg(target_arch = "xtensa")]
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
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
    analog::adc::{Adc, AdcCalBasic, AdcCalCurve, AdcCalScheme, AdcConfig, Attenuation},
    clock::CpuClock,
    delay::Delay,
    efuse::{AdcCalibUnit, Efuse},
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout},
    interrupt::{Priority, software::SoftwareInterruptControl},
    mcpwm::{
        McPwm, PeripheralClockConfig,
        operator::{PwmActions, PwmPin, PwmPinConfig, PwmUpdateMethod},
        timer::{CounterDirection, PwmWorkingMode},
    },
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::{Duration as HalDuration, Rate},
    timer::timg::TimerGroup,
    usb_serial_jtag::UsbSerialJtag,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
use esp_hal::{
    gpio::Pin,
    pcnt::{Pcnt, channel::EdgeMode, unit::Unit},
};
#[cfg(target_arch = "xtensa")]
use esp_rtos::embassy::InterruptExecutor;
#[cfg(test)]
use flux_purr_firmware::DEFAULT_PD_VOLTAGE_REQUEST;
#[cfg(test)]
use flux_purr_firmware::adapters::ch224q;
#[cfg(test)]
use flux_purr_firmware::adapters::ch224q::Status;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::adapters::pd::SourceCapabilities;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::adapters::pd::{
    Contract, ContractKind, ControllerKind, FUSB302B_PPS_MAX_MV,
};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::adapters::pd::{
    FUSB302B_PPS_MIN_MV, GUARANTEED_HEATER_MIN_MV, MAX_HEATER_CONTRACT_MA, MIN_HEATER_CONTRACT_MA,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::board::s3_frontpanel;
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::buzzer::BuzzerDecision;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::buzzer::BuzzerOutput;
#[cfg(test)]
use flux_purr_firmware::buzzer::PROTECTION_ALARM_INTERVAL_MS;
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::buzzer::{
    BuzzerArbiter, BuzzerCueId, BuzzerCueSource, ProtectionAlarmCadence,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
use flux_purr_firmware::buzzer_test::{
    BUZZER_TEST_OUTPUT_TRACE_CAPACITY, BuzzerTestOutputTraceEvent,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
use flux_purr_firmware::buzzer_test::{
    BuzzerTestSession, BuzzerTestSessionState, BuzzerTestStatus,
};
#[cfg(any(test, all(target_arch = "xtensa", feature = "web_serial")))]
use flux_purr_firmware::control_plane::EepromMaintenanceOp;
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
use flux_purr_firmware::control_plane::LanPairingCode;
#[cfg(test)]
use flux_purr_firmware::control_plane::ThermalControlProfileCommand;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::WifiConfigOp;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::WifiConfigReceipt;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::hello_frame;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::control_plane::{
    AdcCalibrationSourceWire, AdcDiagnosticsWire, ApiError, CalibrationControlCommand,
    CalibrationJobKindWire, CalibrationJobStateWire, CalibrationJobStatusWire, CalibrationModeWire,
    CalibrationRuntimeStateWire, ControlPlaneStatus, HeaterCurvePackageWire, Identity,
    InstallStatus, RuntimeConfigCommand, ThermalControlProfileOp, ThermalControlProfilePointWire,
    ThermalControlProfileSettingsWire, ThermalControlProfileWire, ThermalControlRuntimeWire,
    ThermalPlantActiveResultWire, ThermalPlantProvisionalCurveWire, ThermalPlantRunAttemptWire,
    ThermalPlantRunPhaseWire, ThermalPlantRunSnapshotWire, ThermalPlantRuntimeWire,
    ThermalPlantTracePageWire, ThermalPlantTracePointWire, UsbFrame, UsbFrameError, UsbRequestOp,
    UsbResponsePayload, calibration_state_from_memory, heater_curve_state_from_memory,
    network_from_memory, parse_usb_frame, write_usb_frame,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
use flux_purr_firmware::control_plane::{BuzzerTestCommand, BuzzerTestOp};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::control_plane::{
    CalibrationChannelWire, CalibrationConfigCommand, CalibrationConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::{
    CalibrationJobCommandWire, CalibrationJobOpWire, EepromMaintenanceCommand,
    HeaterCurveConfigCommand, HeaterCurveConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use flux_purr_firmware::control_plane::{
    CalibrationSampleWire, CalibrationSlotFitWire, CalibrationSlotIdWire, CalibrationStateWire,
    samples_from_wire,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::frontpanel::{
    FRONTPANEL_PRESET_COUNT, FRONTPANEL_TARGET_TEMP_MAX_C, FRONTPANEL_TARGET_TEMP_MIN_C,
    FanDisplayState, FrontPanelKeyMap, FrontPanelRawState, FrontPanelRoute, FrontPanelRuntimeMode,
    FrontPanelUiState, HeaterLockReason,
};
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
use flux_purr_firmware::lan::LanEndpoint;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::memory::{
    ADC_CALIBRATION_MAX_SAMPLES, AdcCalibrationSample, HEATER_CURVE_MAX_POINTS, HeaterCurvePoint,
    HeaterCurveRawObservation, HeaterCurveRawObservations,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::memory::{AdcCalibrationChannel, correct_adc_mv};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::memory::{
    EepromError, LEGACY_MEMORY_SLOT_A_OFFSET, LEGACY_MEMORY_SLOT_B_OFFSET, LEGACY_MEMORY_SLOT_SIZE,
    M24C64_CAPACITY_BYTES, M24C64_I2C_ADDRESS, M24c64, MEMORY_RECORD_FORMAT_VERSION,
    MEMORY_RECORD_HEADER_LEN, MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_B_OFFSET, MEMORY_SLOT_SIZE,
    MEMORY_WRITE_DEBOUNCE_MS, MemoryRecord, PREVIOUS_MEMORY_SLOT_A_OFFSET,
    PREVIOUS_MEMORY_SLOT_B_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE, decode_memory_record,
    encode_memory_record,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::memory::{
    HeaterCurveConfig, MemoryConfig,
    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_DEFAULT,
    THERMAL_CONTROL_PROFILE_APPROACH_DAMPING_EXPONENT_PERMILLE_MAX,
    THERMAL_CONTROL_PROFILE_APPROACH_TAIL_WINDOW_CENTI_C_MAX,
    THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MAX,
    THERMAL_CONTROL_PROFILE_AUTO_ADJUSTABLE_WORKING_FLOOR_MV_MIN,
    THERMAL_CONTROL_PROFILE_HEATER_CURRENT_RESERVE_MA_MAX,
    THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS, THERMAL_PLANT_TRANSIENT_MAX_CONVECTION_MW_PER_C,
    THERMAL_PLANT_TRANSIENT_MAX_RADIATION_MW_PER_K4, THERMAL_PLANT_TRANSIENT_MAX_SAMPLES,
    ThermalControlProfileConfig, ThermalControlProfilePointConfig,
    ThermalControlProfileSettingsConfig, ThermalPlantProjection, ThermalPlantProjectionRecord,
    ThermalPlantTransientSample, ThermalPlantTransientTransaction, ThermalProfileBank,
    ThermalProfileMode, heater_resistance_ohms_from_curve, thermal_plant_projection_from_transient,
};
#[cfg(test)]
use flux_purr_firmware::memory::{
    MEMORY_RECORD_FORMAT_VERSION, MEMORY_RECORD_HEADER_LEN, MEMORY_SLOT_SIZE, MemoryRecord,
    decode_memory_record, encode_memory_record,
};
#[cfg(test)]
use flux_purr_firmware::memory::{ThermalPlantRawAnchor, ThermalPlantRawTransaction};
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
use flux_purr_firmware::net_http::{ControlMailboxCommand, HttpMethod, LAN_HTTP_BODY_MAX_LEN};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::status_light::{
    RgbChannels, StatusLightInputs, StatusLightState, select_status_light_state,
    status_light_output,
};
#[cfg(any(target_arch = "xtensa", test))]
use flux_purr_firmware::thermal_plant::{ThermalPlantControlInput, ThermalPlantController};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    DEFAULT_PD_VOLTAGE_REQUEST, FAN_PWM_FREQUENCY_HZ, pwm_percent_from_permille,
};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
use flux_purr_firmware::{DeviceMode, DeviceStatus, PdState};
#[cfg(target_arch = "xtensa")]
use flux_purr_firmware::{
    adapters::{
        ch224q::{self, Status},
        fusb302b::{self, SinkPhase},
    },
    display::{DISPLAY_PANEL_CONFIG, DisplayCanvas, SceneId, render_scene},
    frontpanel::{
        FRONTPANEL_DEBOUNCE_MS, FRONTPANEL_DOUBLE_CLICK_MS, FrontPanelInputController,
        FrontPanelInputTimings, KeyGesture, RawFrontPanelKey, render::render_frontpanel_ui,
    },
};
#[cfg(target_arch = "xtensa")]
use fusb302::{
    CcPin, CcPull, DataRole, Fusb302, InterruptMasks, PdPacket, PdRevision, PhyConfig, PowerRole,
    RetryCount, SopType, ToggleMode,
};
#[cfg(target_arch = "xtensa")]
use gc9d01::{GC9D01, Timer as Gc9d01Timer};
#[cfg(target_arch = "xtensa")]
use micromath::F32Ext;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use serde::{Deserialize, Serialize};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
use sha2::{Digest, Sha256};
#[cfg(target_arch = "xtensa")]
use static_cell::StaticCell;

#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
#[unsafe(link_section = ".dram2_uninit")]
static mut RUNTIME_HEAP_STORAGE: MaybeUninit<[u8; RUNTIME_HEAP_SIZE]> = MaybeUninit::uninit();

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
// The display canvas is reinitialized before every use and lives in DRAM2 so
// the ProCPU control task retains enough guarded stack for USB requests.
const RUNTIME_HEAP_SIZE: usize = 48 * 1024;

#[cfg(all(target_arch = "xtensa", not(feature = "net_http")))]
#[unsafe(link_section = ".dram2_uninit")]
static mut RUNTIME_HEAP_STORAGE: MaybeUninit<[u8; RUNTIME_HEAP_SIZE]> = MaybeUninit::uninit();

#[cfg(all(target_arch = "xtensa", not(feature = "net_http")))]
const RUNTIME_HEAP_SIZE: usize = 8 * 1024;

#[cfg(target_arch = "xtensa")]
fn init_runtime_heap() {
    // Wi-Fi heap and the USB response buffer share post-boot DRAM2. Keeping
    // the 8 KiB response buffer out of the Embassy task leaves enough primary
    // DRAM for the startup stack during radio initialization. This region is
    // NOLOAD, so it retains arbitrary bytes across software resets. Clear it
    // before registration because the Wi-Fi binary embeds ETS timers in heap
    // objects and treats an initial non-null `priv_` field as a live RTOS timer.
    let heap_ptr = core::ptr::addr_of_mut!(RUNTIME_HEAP_STORAGE).cast::<u8>();
    // SAFETY: this runs once before the storage is registered with the global
    // allocator, and the static region remains exclusively owned by that
    // allocator for the rest of the program.
    unsafe {
        heap_ptr.write_bytes(0, RUNTIME_HEAP_SIZE);
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            heap_ptr,
            RUNTIME_HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[unsafe(link_section = ".dram2_uninit")]
static mut USB_CONTROL_RESPONSE_BUFFER: MaybeUninit<[u8; USB_CONTROL_TX_BUFFER_LEN]> =
    MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".dram2_uninit")]
static mut DISPLAY_CANVAS_STORAGE: MaybeUninit<DisplayCanvas> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
struct MemoryIoScratch {
    record_bytes: [u8; MEMORY_SLOT_SIZE],
}

#[cfg(any(target_arch = "xtensa", test))]
fn zeroize_bytes_volatile(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: `byte` is an exclusive reference to initialized memory. A
        // volatile write keeps the scrub from being removed before deallocation.
        unsafe { core::ptr::write_volatile(byte, 0) };
    }
}

#[cfg(target_arch = "xtensa")]
impl Drop for MemoryIoScratch {
    fn drop(&mut self) {
        // Wi-Fi reuses this allocator region during radio startup. Do not leave
        // EEPROM/config bytes where a C timer object can observe stale pointers.
        zeroize_bytes_volatile(&mut self.record_bytes);
    }
}

#[cfg(target_arch = "xtensa")]
fn try_allocate_memory_io_scratch() -> Option<Box<MemoryIoScratch>> {
    let layout = Layout::new::<MemoryIoScratch>();
    // SAFETY: the global allocator is initialized before any configuration I/O.
    // A null allocation is handled as a regular unavailable-workspace result.
    let allocation = unsafe { alloc(layout) };
    if allocation.is_null() {
        return None;
    }
    // SAFETY: `write_bytes` initializes every byte of the allocation before it
    // is exposed as `MemoryIoScratch`. This keeps the workspace out of the
    // ProCPU task stack and out of permanent application BSS.
    unsafe {
        allocation.write_bytes(0, layout.size());
        Some(Box::from_raw(allocation.cast::<MemoryIoScratch>()))
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn initialize_usb_control_response_buffer() -> &'static mut [u8; USB_CONTROL_TX_BUFFER_LEN] {
    // SAFETY: this is called exactly once from the sole front-panel executor
    // before USB control handling starts. `MaybeUninit::write` initializes the
    // whole DRAM2 slot on every boot, so retained RAM contents are never read.
    unsafe {
        (&mut *core::ptr::addr_of_mut!(USB_CONTROL_RESPONSE_BUFFER))
            .write([0; USB_CONTROL_TX_BUFFER_LEN])
    }
}

#[cfg(target_arch = "xtensa")]
fn initialize_display_canvas() -> &'static mut DisplayCanvas {
    // DRAM2 is retained across software resets, so overwrite the canvas rather
    // than using StaticCell's one-time initialization marker. Initialize in
    // place so the 16 KiB canvas never occupies the guarded task stack.
    unsafe {
        let canvas = core::ptr::addr_of_mut!(DISPLAY_CANVAS_STORAGE).cast::<DisplayCanvas>();
        DisplayCanvas::initialize_black_in_place(canvas);
        &mut *canvas
    }
}

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
fn rom_log_line(line: &[u8]) {
    unsafe extern "C" {
        fn esp_rom_output_tx_one_char(value: u8) -> i32;
    }
    for byte in line {
        // SAFETY: this ROM routine is available on ESP32-S3 and accepts one byte.
        unsafe { esp_rom_output_tx_one_char(*byte) };
    }
}

#[cfg(target_arch = "xtensa")]
struct RomPanicWriter;

#[cfg(target_arch = "xtensa")]
impl core::fmt::Write for RomPanicWriter {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        rom_log_line(value.as_bytes());
        Ok(())
    }
}

#[cfg(target_arch = "xtensa")]
#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    rom_log_line(b"panic=firmware_fault\n");
    if let Some(location) = info.location() {
        let mut writer = RomPanicWriter;
        let _ = core::fmt::Write::write_fmt(
            &mut writer,
            format_args!("panic_location={}:{}\n", location.file(), location.line()),
        );
    }
    esp_hal::rom::ets_delay_us(250_000);
    esp_hal::system::software_reset()
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

    fn write_response_bytes(&mut self, bytes: &[u8]) -> bool {
        // A control response is only sent after the host has delivered a full
        // request line, so the USB endpoint is known to have an active reader.
        // The HAL's blocking writer waits for each 64-byte packet to complete;
        // this avoids silently losing a larger JSON response when a nonblocking
        // flush observes a temporarily busy endpoint.
        self.inner.write(bytes).is_ok()
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
const ACTIVE_COOLING_FAN_MIN_TEMP_C: i16 = 35;
#[cfg(any(target_arch = "xtensa", test))]
const FORCED_COOLING_FAN_MIN_TEMP_C: i16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const FORCED_COOLING_FAN_FULL_TEMP_C: i16 = 60;
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
// The plant delay is calibrated in seconds, so its predictor must not amplify
// sub-second RTD quantisation into a multi-degree correction.
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_SLOPE_FILTER_ALPHA: f32 = 0.025;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PHASE_HYSTERESIS_C: f32 = 0.10;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
const DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_ADJUSTABLE_MIN_MV: u16 = 12_000;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_INITIAL_PPS_REQUEST_MV: u16 = HEATER_ADJUSTABLE_MIN_MV;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_ADJUSTABLE_MAX_MV: u16 = 28_000;
#[cfg(any(target_arch = "xtensa", test))]
const CH224Q_ADJUSTABLE_REQUEST_MIN_MV: u16 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_REQUEST_HYSTERESIS_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_REQUEST_STEP_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
// VIN is measured at the heater while PPS is requested at the source.
// Bound compensation so stale measurements cannot cause a large source jump.
const HEATER_PPS_PATH_DROP_COMPENSATION_MAX_MV: u16 = 2_500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PPS_INITIAL_SETTLE_MS: u64 = 10_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PPS_STEADY_DWELL_MS: u64 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
// A current-limited high-temperature plate can be power-bound before the
// physical PWM reaches 98%.  Treat 80% as near saturation so PPS can recover
// the remaining voltage headroom without waiting for an unreachable duty.
const HEATER_HOLD_PPS_SATURATION_PWM_MIN_PERCENT: u8 = 80;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PPS_RAISE_ERROR_MIN_C: f32 = 0.25;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_HOLD_PPS_RAISE_MAX_SLOPE_C_PER_S: f32 = 0.25;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_SMALL_TRANSITION_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PPS_LARGE_TRANSITION_MS: u64 = 275;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_PULSE_PERIOD_MS: u64 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
const HEATING_FAN_PULSE_MAX_DUTY_PERCENT: u8 = 50;
#[cfg(target_arch = "xtensa")]
const DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS: u64 = 1_000;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const USB_CONTROL_LINE_CAPACITY: usize = flux_purr_firmware::control_plane::USB_LINE_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_BUFFER_LEN: usize = flux_purr_firmware::control_plane::USB_LINE_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_PACKET_LEN: usize = 64;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
const USB_CONTROL_TX_RETRY_LIMIT: usize = 4096;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const USB_CONTROL_TX_BACKOFF_US: u32 = 25;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_FULL_SPEED_PWM_PERMILLE: u16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_HALF_SPEED_PWM_PERMILLE: u16 = 250;
#[cfg(any(target_arch = "xtensa", test))]
const FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE: u16 = 1_000;
#[cfg(test)]
const HEATER_APPROACH_DUTY_PERCENT: u8 = 32;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PROFILE_R20_OHMS: f32 = 3.2;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURVE_COLD_ANCHOR_TEMP_C: f32 = 0.0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURVE_R20_ANCHOR_TEMP_C: f32 = 20.0;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PROFILE_TEMP_COEFFICIENT_PER_C: f32 = 0.00393;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURRENT_LIMIT_FALLBACK_REQUEST: ch224q::VoltageRequest = ch224q::VoltageRequest::V9;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_CURRENT_LIMIT_RETURN_HYSTERESIS_MV: u16 = 200;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PWM_FREQUENCY_HZ: u32 = 100;
#[cfg(any(target_arch = "xtensa", test))]
const MCPWM_PERIPHERAL_CLOCK_HZ: u32 = 40_000_000;
#[cfg(test)]
const MCPWM_TIMER_MAX_PRESCALER: u32 = 255;
#[cfg(target_arch = "xtensa")]
const FAN_PWM_PERIOD_TICKS: u16 = 99;
// MCPWM's timer prescaler is only eight bits. At 40 MHz, 100 Hz needs at
// least 1,563 timer counts; 1,600 counts gives an exact 100 Hz period.
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_PWM_PERIOD_TICKS: u16 = 1_599;
#[cfg(any(target_arch = "xtensa", test))]
const HEATER_WARMUP_SOFT_START_MS: u64 = 1_000;
// Timer2 keeps one clock divider for its lifetime. Cue pitch is selected with
// the period register because ESP32-S3 can report a new timer prescaler in
// CFG0 while the GPIO matrix continues emitting the previous carrier.
#[cfg(any(target_arch = "xtensa", test))]
const BUZZER_TIMER_PRESCALER: u8 = 3;
#[cfg(any(target_arch = "xtensa", test))]
const BUZZER_IDLE_FREQUENCY_HZ: u32 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
const BUZZER_ATTENTION_REMINDER_INTERVAL_MS: u64 = 10_000;
#[cfg(target_arch = "xtensa")]
const STATUS_LIGHT_BOOT_DURATION_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
const RUNTIME_READY_BOOT_STAGE_LINE: &[u8] = b"boot_stage=runtime_ready\n";
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_ATTENUATION: Attenuation = Attenuation::_6dB;
#[cfg(any(target_arch = "xtensa", test))]
// Sample each 1 ms phase across the full 100 Hz MOS period. A contiguous ADC
// burst can otherwise land on one PWM phase and report switching noise as a
// temperature change. This is acquisition timing, not a display filter.
const RTD_SAMPLE_COUNT: usize = 80;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_SAMPLE_PWM_PHASE_COUNT: usize = 10;
#[cfg(target_arch = "xtensa")]
const RTD_SAMPLE_PWM_PHASE_SPACING_US: u32 = 1_000;
#[cfg(target_arch = "xtensa")]
// ADC1 alternates between the low-impedance VIN divider and the filtered,
// high-impedance RTD node. Let the RTD node recover in real time before the
// conversion discard prefix; conversion count alone does not provide a stable
// RC settling interval across ADC clock conditions.
const RTD_CHANNEL_SWITCH_SETTLE_US: u32 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
// ADC1 is shared by the high-impedance RTD divider and the VIN divider. Keep
// a longer discard prefix after every channel switch so the retained batch is
// not biased by the preceding channel's sample-and-hold residue.
const RTD_SETTLE_DISCARD_SAMPLE_COUNT: usize = 96;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_MIN_VALID_SAMPLE_COUNT: usize = 60;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_RETRY_AFTER_VIN_STEP_RAW_ADC_DELTA_MV: u16 = 48;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_SAMPLE_STABLE_AFTER_REQUEST_MS: u64 = 300;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_MAX_SLEW_C_PER_S: f32 = 35.0;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_MAX_ACCEPTED_STEP_C: f32 = 6.0;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_MAX_UNPOWERED_RISE_C_PER_S: f32 = 4.0;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_MAX_UNPOWERED_RISE_STEP_C: f32 = 3.0;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_GUARD_RECOVERY_WINDOW_MS: u64 = 750;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_CONTROL_GUARD_RECOVERY_BAND_C: f32 = 3.0;
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
// R17 = 31.6 kOhm and R16 = 10 kOhm set the TPS62933 3V3 rail to
// 0.8 V * (1 + 31.6 / 10) = 3.328 V nominal. This is the divider's circuit
// model, not a measured rail value or an ADC calibration parameter.
const RTD_DIVIDER_SUPPLY_MV: u16 = 3_328;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_SHORT_FAULT_MAX_MV: u16 = 150;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_OPEN_FAULT_MIN_MV: u16 = 2_800;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_TEMP_MIN_C: f32 = -50.0;
#[cfg(any(target_arch = "xtensa", test))]
const RTD_TEMP_MAX_C: f32 = 500.0;
#[cfg(target_arch = "xtensa")]
const FUSB302B_I2C_FREQUENCY_HZ: u32 = 100_000;
#[cfg(target_arch = "xtensa")]
// Keep identity probing fail-closed without turning an absent optional
// controller into a multi-second startup stall. EEPROM reads are chunked
// below this budget so the shared bus retains the same bounded transaction.
const I2C_TRANSACTION_TIMEOUT_MS: u64 = 25;
#[cfg(target_arch = "xtensa")]
const EEPROM_WRITE_CYCLE_DELAY_MS: u64 = 5;
#[cfg(any(target_arch = "xtensa", test))]
const EEPROM_WRITE_CHUNK_MAX_BYTES: usize = 16;
#[cfg(target_arch = "xtensa")]
const EEPROM_READ_CHUNK_MAX_BYTES: usize = 64;
#[cfg(target_arch = "xtensa")]
const EEPROM_UNUSED_GAP_OFFSET: u16 = 0x0c00;
#[cfg(test)]
const EEPROM_UNUSED_GAP_LEN: usize = 0x0400;

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const EEPROM_SNAPSHOT_SIZE: u16 = M24C64_CAPACITY_BYTES;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const EEPROM_SNAPSHOT_CHUNK_MAX: u16 = 32;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const EEPROM_SNAPSHOT_HASH_LEN: usize = 71;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
const EEPROM_SNAPSHOT_TIMEOUT_MS: u64 = 30_000;

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EepromSnapshotRequest {
    op: heapless::String<32>,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    #[serde(default)]
    session_id: Option<heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>>,
    #[serde(default)]
    offset: Option<u16>,
    #[serde(default)]
    length: Option<u16>,
    #[serde(default)]
    sha256: Option<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>>,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EepromSnapshotResponse {
    ok: bool,
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capacity: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_max: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<heapless::Vec<u8, 32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }>>,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Default)]
struct EepromSnapshotSession {
    active: bool,
    session_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    next_offset: u16,
    last_activity_ms: u64,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn snapshot_string<const N: usize>(value: &str) -> heapless::String<N> {
    let mut output = heapless::String::new();
    let _ = output.push_str(value);
    output
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_snapshot_error(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    code: &'static str,
) -> EepromSnapshotResponse {
    EepromSnapshotResponse {
        ok: false,
        request_id,
        session_id: None,
        capacity: None,
        chunk_max: None,
        offset: None,
        bytes: None,
        sha256: None,
        error: Some(snapshot_string(code)),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_snapshot_storage_failure(response: &EepromSnapshotResponse) -> bool {
    matches!(
        response.error.as_ref().map(|error| error.as_str()),
        Some("eeprom_unavailable" | "eeprom_read_failed" | "snapshot_hash_mismatch")
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_snapshot_digest(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
) -> Result<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>, &'static str> {
    let Some(address) = probe_eeprom_address(i2c) else {
        return Err("eeprom_unavailable");
    };
    let mut eeprom = M24c64::with_address(i2c, address);
    let mut hasher = Sha256::new();
    let mut offset = 0_u16;
    let mut bytes = [0_u8; EEPROM_SNAPSHOT_CHUNK_MAX as usize];
    while offset < EEPROM_SNAPSHOT_SIZE {
        let length = usize::from((EEPROM_SNAPSHOT_SIZE - offset).min(EEPROM_SNAPSHOT_CHUNK_MAX));
        read_eeprom_bytes_chunked(&mut eeprom, offset, &mut bytes[..length])
            .map_err(|_| "eeprom_read_failed")?;
        hasher.update(&bytes[..length]);
        offset = offset.saturating_add(length as u16);
    }
    let digest = hasher.finalize();
    let mut rendered = heapless::String::new();
    rendered
        .push_str("sha256:")
        .map_err(|_| "digest_format_failed")?;
    for byte in digest {
        write!(rendered, "{byte:02x}").map_err(|_| "digest_format_failed")?;
    }
    Ok(rendered)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn write_eeprom_snapshot_response(
    usb: &mut RawUsbSerialJtag,
    response: &EepromSnapshotResponse,
    tx_buf: &mut [u8; USB_CONTROL_TX_BUFFER_LEN],
) {
    let Ok(written) = serde_json_core::to_slice(response, tx_buf) else {
        let _ = usb_write_bytes_bounded(usb, b"{\"ok\":false,\"error\":\"output_too_small\"}\n");
        return;
    };
    if written >= tx_buf.len() {
        let _ = usb_write_bytes_bounded(usb, b"{\"ok\":false,\"error\":\"output_too_small\"}\n");
        return;
    }
    tx_buf[written] = b'\n';
    let _ = usb_write_bytes_bounded(usb, &tx_buf[..=written]);
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn process_eeprom_snapshot_line(
    line: &str,
    session: &mut EepromSnapshotSession,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    last_heater_duty: u8,
    memory_commit_due_ms: &mut Option<u64>,
    elapsed_ms: u64,
) -> Option<EepromSnapshotResponse> {
    let parsed = serde_json_core::from_slice::<EepromSnapshotRequest>(line.as_bytes());
    let request = match parsed {
        Ok((request, _)) if request.op.as_str().starts_with("eeprom_snapshot_") => request,
        Ok(_) => return None,
        Err(_) if line.contains("eeprom_snapshot_") => {
            return Some(eeprom_snapshot_error(
                heapless::String::new(),
                "malformed_snapshot",
            ));
        }
        Err(_) => return None,
    };
    if session.active
        && elapsed_ms.saturating_sub(session.last_activity_ms) > EEPROM_SNAPSHOT_TIMEOUT_MS
    {
        session.active = false;
        session.session_id.clear();
        session.next_offset = 0;
        *memory_commit_due_ms = None;
    }
    let request_id = request.request_id.clone();
    let requested_session = request
        .session_id
        .as_ref()
        .unwrap_or(&request.request_id)
        .clone();
    match request.op.as_str() {
        "eeprom_snapshot_open" => {
            if last_heater_duty != 0 {
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            let session_id = request.session_id.unwrap_or(request.request_id.clone());
            if session_id.is_empty() {
                return Some(eeprom_snapshot_error(request_id, "session_required"));
            }
            session.active = true;
            session.session_id = session_id.clone();
            session.next_offset = 0;
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session_id),
                capacity: Some(EEPROM_SNAPSHOT_SIZE),
                chunk_max: Some(EEPROM_SNAPSHOT_CHUNK_MAX),
                offset: None,
                bytes: None,
                sha256: None,
                error: None,
            })
        }
        "eeprom_snapshot_read" => {
            if !session.active || requested_session != session.session_id {
                return Some(eeprom_snapshot_error(
                    request_id,
                    "snapshot_session_invalid",
                ));
            }
            if last_heater_duty != 0 {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            let (Some(offset), Some(length)) = (request.offset, request.length) else {
                return Some(eeprom_snapshot_error(request_id, "snapshot_range_required"));
            };
            if length == 0
                || length > EEPROM_SNAPSHOT_CHUNK_MAX
                || offset != session.next_offset
                || offset.saturating_add(length) > EEPROM_SNAPSHOT_SIZE
            {
                return Some(eeprom_snapshot_error(request_id, "snapshot_range_invalid"));
            }
            let Some(address) = probe_eeprom_address(i2c) else {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "eeprom_unavailable"));
            };
            let mut eeprom = M24c64::with_address(i2c, address);
            let mut bytes = heapless::Vec::<u8, 32>::new();
            let _ = bytes.resize_default(usize::from(length));
            if read_eeprom_bytes_chunked(&mut eeprom, offset, bytes.as_mut_slice()).is_err() {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "eeprom_read_failed"));
            }
            session.next_offset = session.next_offset.saturating_add(length);
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session.session_id.clone()),
                capacity: None,
                chunk_max: None,
                offset: Some(offset),
                bytes: Some(bytes),
                sha256: None,
                error: None,
            })
        }
        "eeprom_snapshot_close" => {
            if !session.active || requested_session != session.session_id {
                return Some(eeprom_snapshot_error(
                    request_id,
                    "snapshot_session_invalid",
                ));
            }
            if last_heater_duty != 0 {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "heater_active"));
            }
            if session.next_offset != EEPROM_SNAPSHOT_SIZE {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "snapshot_incomplete"));
            }
            let digest = match eeprom_snapshot_digest(i2c) {
                Ok(digest) => digest,
                Err(code) => {
                    session.active = false;
                    return Some(eeprom_snapshot_error(request_id, code));
                }
            };
            if request.sha256.as_ref() != Some(&digest) {
                session.active = false;
                return Some(eeprom_snapshot_error(request_id, "snapshot_hash_mismatch"));
            }
            let session_id = session.session_id.clone();
            session.active = false;
            session.session_id.clear();
            session.next_offset = 0;
            session.last_activity_ms = elapsed_ms;
            *memory_commit_due_ms = None;
            Some(EepromSnapshotResponse {
                ok: true,
                request_id,
                session_id: Some(session_id),
                capacity: None,
                chunk_max: None,
                offset: None,
                bytes: None,
                sha256: Some(digest),
                error: None,
            })
        }
        _ => Some(eeprom_snapshot_error(request_id, "snapshot_op_unsupported")),
    }
}

#[cfg(target_arch = "xtensa")]
struct DisplayTimer;

#[cfg(target_arch = "xtensa")]
impl Gc9d01Timer for DisplayTimer {
    fn after_millis(milliseconds: u64) -> impl core::future::Future<Output = ()> {
        EmbassyTimer::after_millis(milliseconds)
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
    warmup_soft_start_percent: u8,
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
    warmup_reenter_centi_c: u16,
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
    warmup_reenter_error_c: f32,
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
            warmup_reenter_centi_c: sanitize_inherited(self.warmup_reenter_centi_c, 5_000),
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
            warmup_reenter_centi_c: value.warmup_reenter_centi_c,
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
            warmup_reenter_centi_c: value.warmup_reenter_centi_c,
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
            if sanitized.warmup_reenter_centi_c == 0 {
                sanitized.warmup_reenter_centi_c =
                    (self.settings.warmup_reenter_error_c * 100.0) as u16;
            }
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
            sanitized.warmup_power_permille = sanitized
                .warmup_power_permille
                .max(sanitized.approach_power_permille)
                .min(1_000);
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
    manual_pps: &ManualPpsState,
) -> Option<ThermalControlProfile> {
    preview.or_else(|| {
        ThermalControlProfile::from_saved_config(memory_config.thermal_profile(
            resolve_thermal_profile_bank(memory_config.thermal_profile_mode, manual_pps),
        ))
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn resolve_thermal_profile_bank(
    mode: ThermalProfileMode,
    manual_pps: &ManualPpsState,
) -> ThermalProfileBank {
    if mode == ThermalProfileMode::Auto && manual_pps.has_matching_pps_apdo(20_000, 5_000) {
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
        warmup_reenter_centi_c: round_to_u16_nonnegative(target.warmup_reenter_error_c * 100.0),
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
    let warmup_power_permille = 1_000;
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
        warmup_reenter_error_c: settings.warmup_reenter_error_c,
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
            warmup_power_permille: 1_000,
            warmup_reenter_error_c: f32::from(lower.warmup_reenter_centi_c) / 100.0,
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
        warmup_power_permille: 1_000,
        warmup_reenter_error_c: f32::from(lerp_u16(
            lower.warmup_reenter_centi_c,
            upper.warmup_reenter_centi_c,
            5_000,
        )) / 100.0,
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
    previous_actual_error_c: f32,
    filtered_error_c: f32,
    brake_distance_c: f32,
    handoff_error_c: f32,
) -> bool {
    let predictive_actual_margin_c = 1.0;
    let max_filtered_lag_for_static_brake_c = 3.0;
    let actual_handoff_delta_c = previous_actual_error_c - actual_error_c;
    let actual_step_confirmed = actual_handoff_delta_c <= handoff_error_c + 1.0;
    let previous_handoff_confirmed = previous_actual_error_c <= handoff_error_c + 1.0;
    let actual_brake_confirmed = actual_error_c <= brake_distance_c
        && previous_actual_error_c <= brake_distance_c + 0.5
        && filtered_error_c <= brake_distance_c + max_filtered_lag_for_static_brake_c
        && actual_step_confirmed;
    let predictive_actual_confirmed = actual_error_c
        <= brake_distance_c + predictive_actual_margin_c
        && previous_actual_error_c <= brake_distance_c + predictive_actual_margin_c + 0.5;
    let predictive_handoff_confirmed = actual_error_c.max(filtered_error_c) <= handoff_error_c
        && previous_handoff_confirmed
        && actual_step_confirmed
        && predictive_actual_confirmed;
    actual_brake_confirmed || predictive_handoff_confirmed
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

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct BuzzerHardwareState {
    frequency_hz: Option<u32>,
    duty_percent: u8,
    generation: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_timer_reconfiguration_needed(
    configured_frequency_hz: u32,
    next_state: BuzzerHardwareState,
) -> bool {
    next_state
        .frequency_hz
        .is_some_and(|frequency_hz| frequency_hz != configured_frequency_hz)
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuzzerHardwareAction {
    StopTimer,
    Retune(u32),
    SetDutyPercent(u8),
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_hardware_actions(
    configured_frequency_hz: u32,
    next_state: BuzzerHardwareState,
) -> heapless::Vec<BuzzerHardwareAction, 4> {
    let mut actions = heapless::Vec::new();
    if buzzer_timer_reconfiguration_needed(configured_frequency_hz, next_state) {
        let frequency_hz = next_state
            .frequency_hz
            .expect("timer reconfiguration requires an audible buzzer frequency");
        let _ = actions.push(BuzzerHardwareAction::SetDutyPercent(0));
        let _ = actions.push(BuzzerHardwareAction::StopTimer);
        let _ = actions.push(BuzzerHardwareAction::Retune(frequency_hz));
    }
    let _ = actions.push(BuzzerHardwareAction::SetDutyPercent(
        next_state.duty_percent,
    ));
    actions
}

#[cfg(any(test, all(target_arch = "xtensa", feature = "buzzer-observe")))]
fn mcpwm_timer_frequency_hz(prescaler: u8, period_ticks: u16) -> u32 {
    MCPWM_PERIPHERAL_CLOCK_HZ / (u32::from(prescaler) + 1) / (u32::from(period_ticks) + 1)
}

#[cfg(any(target_arch = "xtensa", test))]
fn buzzer_timer_period_ticks(frequency_hz: u32) -> Option<u16> {
    if frequency_hz == 0 {
        return None;
    }
    let timer_clock_hz = MCPWM_PERIPHERAL_CLOCK_HZ / (u32::from(BUZZER_TIMER_PRESCALER) + 1);
    let period_counts = timer_clock_hz
        .saturating_add(frequency_hz / 2)
        .checked_div(frequency_hz)?;
    if period_counts == 0 || period_counts > u32::from(u16::MAX) + 1 {
        return None;
    }
    Some((period_counts - 1) as u16)
}

#[cfg(any(test, all(target_arch = "xtensa", feature = "buzzer-observe")))]
fn buzzer_observed_frequency_hz(rising_edges: u16, window_ms: u32) -> Option<u32> {
    if window_ms == 0 {
        return None;
    }
    Some(u32::from(rising_edges).saturating_mul(1_000) / window_ms)
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
    hold_coast_cooling_samples: u8,
    heater_was_enabled: bool,
    warmup_started_at_ms: Option<u64>,
    thermal_plant_controller: ThermalPlantController,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThermalPlantRuntimeInput {
    target_temp_c: i16,
    measured_temp_c: f32,
    ambient_temp_c: f32,
    heater_enabled: bool,
    model: flux_purr_firmware::memory::ThermalPlantProjection,
    max_power_mw: f32,
    now_ms: u64,
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
            hold_coast_cooling_samples: 0,
            heater_was_enabled: false,
            warmup_started_at_ms: None,
            thermal_plant_controller: ThermalPlantController::new(),
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
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        self.thermal_plant_controller.reset();
    }

    fn reseed_measurement(&mut self, measured_temp_c: f32) {
        self.filtered_temp_c = Some(measured_temp_c);
        self.previous_filtered_temp_c = Some(measured_temp_c);
        self.filtered_slope_c_per_profile_tick = 0.0;
        self.previous_measured_temp_c = Some(measured_temp_c);
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
        self.hold_coast_cooling_samples = 0;
        self.heater_was_enabled = false;
        self.warmup_started_at_ms = None;
        self.thermal_plant_controller.reset();
        changed
    }

    #[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
    fn update_thermal_plant_at(&mut self, input: ThermalPlantRuntimeInput) -> HeaterPidSnapshot {
        let ThermalPlantRuntimeInput {
            target_temp_c,
            measured_temp_c,
            ambient_temp_c,
            heater_enabled,
            model,
            max_power_mw,
            now_ms,
        } = input;
        if !heater_enabled || self.fault_latched.is_some() {
            self.thermal_plant_controller.reset();
            self.reseed_measurement(measured_temp_c);
            self.duty_percent = 0;
            self.heater_was_enabled = false;
            return HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 0,
                error_c: f32::from(target_temp_c) - measured_temp_c,
                control_error_c: f32::from(target_temp_c) - measured_temp_c,
                filtered_temp_c: measured_temp_c,
                filtered_slope_c_per_s: 0.0,
                coast_active: false,
                phase: HeaterControlPhase::Warmup,
            };
        }
        if measured_temp_c >= f32::from(HEATER_HARD_CUTOFF_TEMP_C) {
            self.latch_fault(HeaterFaultReason::OverTemp);
            return self.update_thermal_plant_at(ThermalPlantRuntimeInput {
                target_temp_c,
                measured_temp_c,
                ambient_temp_c,
                heater_enabled: false,
                model,
                max_power_mw,
                now_ms,
            });
        }
        if !self.heater_was_enabled || self.last_target_temp_c != target_temp_c {
            self.thermal_plant_controller.reset();
            self.reseed_measurement(measured_temp_c);
            self.warmup_started_at_ms = Some(now_ms);
        }
        self.heater_was_enabled = true;
        self.last_target_temp_c = target_temp_c;
        let previous = self.filtered_temp_c.unwrap_or(measured_temp_c);
        let filtered_temp_c = previous + 0.75 * (measured_temp_c - previous);
        let raw_slope_c_per_s =
            (filtered_temp_c - previous) * (1_000.0 / HEATER_CONTROL_INTERVAL_MS as f32);
        let slope_c_per_s = self.filtered_slope_c_per_profile_tick
            + THERMAL_PLANT_SLOPE_FILTER_ALPHA
                * (raw_slope_c_per_s - self.filtered_slope_c_per_profile_tick);
        self.filtered_slope_c_per_profile_tick = slope_c_per_s;
        self.previous_filtered_temp_c = self.filtered_temp_c;
        self.filtered_temp_c = Some(filtered_temp_c);
        let output = self
            .thermal_plant_controller
            .update(ThermalPlantControlInput {
                model,
                target_temp_c: f32::from(target_temp_c),
                current_temp_c: filtered_temp_c,
                ambient_temp_c,
                slope_c_per_s,
                dt_s: HEATER_CONTROL_INTERVAL_MS as f32 / 1_000.0,
                max_power_mw,
            });
        let duty_percent = if max_power_mw <= 0.0 {
            0
        } else {
            round_to_u16_nonnegative(output.requested_power_mw * 100.0 / max_power_mw).min(100)
                as u8
        };
        self.duty_percent = duty_percent;
        let error_c = f32::from(target_temp_c) - measured_temp_c;
        let phase = if error_c.abs() <= 1.5 {
            HeaterControlPhase::Hold
        } else if error_c <= 3.0 {
            HeaterControlPhase::Approach
        } else {
            HeaterControlPhase::Warmup
        };
        self.phase = phase;
        HeaterPidSnapshot {
            duty_percent,
            warmup_soft_start_percent: if phase == HeaterControlPhase::Warmup {
                self.warmup_started_at_ms
                    .map(|started_at_ms| {
                        (now_ms.saturating_sub(started_at_ms).saturating_mul(100)
                            / HEATER_WARMUP_SOFT_START_MS)
                            .min(100) as u8
                    })
                    .unwrap_or(100)
            } else {
                100
            },
            error_c,
            control_error_c: output.predicted_error_c,
            filtered_temp_c,
            filtered_slope_c_per_s: slope_c_per_s,
            coast_active: duty_percent == 0 && slope_c_per_s > 0.0,
            phase,
        }
    }

    #[cfg(test)]
    fn update(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
        thermal_profile: Option<ThermalControlProfile>,
    ) -> HeaterPidSnapshot {
        self.update_at(
            target_temp_c,
            measured_temp_c,
            heater_enabled,
            thermal_profile,
            0,
        )
    }

    fn update_at(
        &mut self,
        target_temp_c: i16,
        measured_temp_c: f32,
        heater_enabled: bool,
        thermal_profile: Option<ThermalControlProfile>,
        now_ms: u64,
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
            self.hold_coast_cooling_samples = 0;
            self.heater_was_enabled = false;
            self.warmup_started_at_ms = None;
            return HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 0,
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
            self.hold_coast_cooling_samples = 0;
            self.warmup_started_at_ms = Some(now_ms);
        }

        if !self.heater_was_enabled {
            self.warmup_started_at_ms = Some(now_ms);
        }
        self.heater_was_enabled = true;

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
        let hold_prediction_guard_c = control_target.hold_on_error_c.max(0.05) * 2.0;
        let hold_prediction_blocks_reheat = error_c > 0.0
            && error_c <= hold_prediction_guard_c
            && filtered_temp_slope_c_per_profile_tick > 0.0
            && hold_control_error_c <= 0.0;
        let hold_filter_lag_blocks_reheat =
            error_c <= 0.0 && control_error_c > 0.0 && filtered_temp_slope_c_per_profile_tick > 0.0;
        let hold_actual_overshoot_blocks_reheat = error_c <= 0.0
            && control_error_c <= 0.0
            && (control_error_c - error_c) >= 0.05
            && filtered_temp_slope_c_per_profile_tick > 0.0;
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
        let approach_coast_gate_c = hold_entry_gate_c.max(control_target.hold_exit_error_c)
            + (hold_entry_measurement_margin_c * 2.0);
        let approach_predictive_coast_ready = approach_control_error_c <= 0.0
            && error_c <= approach_coast_gate_c
            && control_error_c <= control_target.hold_exit_error_c;
        // Phase residency follows the actual plate temperature. Filtered and projected errors
        // shape power, but using their lag to leave Hold creates rapid Hold/Approach oscillation.
        let hold_exit_error_c = error_c;
        let brake_distance_c = control_target
            .brake_distance_c
            .max(control_target.hold_entry_error_c + 0.1);
        let warmup_handoff_error_c = warmup_handoff_error_c(
            brake_distance_c,
            control_target.warmup_reenter_error_c,
            filtered_temp_slope_c_per_profile_tick,
            control_target.approach_lead_ticks,
        );

        let mut next_phase = self.phase;
        let previous_phase = self.phase;
        match self.phase {
            HeaterControlPhase::Warmup => {
                if warmup_handoff_ready(
                    error_c,
                    previous_error_c,
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
                if error_c >= brake_distance_c + control_target.warmup_reenter_error_c {
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
            if self.phase == HeaterControlPhase::Warmup {
                self.warmup_started_at_ms = Some(now_ms);
            }
        } else {
            self.phase_ticks = self.phase_ticks.saturating_add(1);
            if self.phase != HeaterControlPhase::Approach {
                self.recovering_from_hold = false;
            }
        }

        if previous_phase != self.phase {
            if self.phase == HeaterControlPhase::Hold {
                let hold_entry_coast_guard_c = control_target
                    .hold_exit_error_c
                    .max(control_target.hold_on_error_c.max(0.05) * 2.0);
                let hold_entry_zero_output_ready = self.duty_percent == 0
                    && error_c <= control_target.hold_on_error_c.max(0.05) * 2.0;
                let hold_entry_projection_ready = self.duty_percent > 0
                    && error_c <= hold_entry_coast_guard_c
                    && (approach_control_error_c <= 0.0 || hold_control_error_c <= 0.0);
                self.hold_coast_active = filtered_temp_slope_c_per_profile_tick > 0.0
                    && (actual_crossed_target_ready
                        || error_c <= 0.0
                        || control_error_c <= 0.0
                        || hold_entry_zero_output_ready
                        || hold_entry_projection_ready);
                self.hold_coast_cooling_samples = 0;
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
                self.hold_coast_cooling_samples = 0;
                self.hold_entry_output_percent = 0;
                if self.phase != HeaterControlPhase::Hold {
                    self.hold_integral_c = 0.0;
                }
            }
        }
        let coast_raw_cooling = measured_temp_c + 0.05 < previous_measured_temp_c
            && error_c >= control_target.hold_on_error_c.max(0.05);
        if self.hold_coast_active && coast_raw_cooling {
            self.hold_coast_cooling_samples = self.hold_coast_cooling_samples.saturating_add(1);
        } else {
            self.hold_coast_cooling_samples = 0;
        }
        let coast_plate_is_cooling =
            filtered_temp_slope_c_per_profile_tick <= -0.02 && self.hold_coast_cooling_samples >= 2;
        if self.hold_coast_active && coast_plate_is_cooling {
            self.hold_coast_active = false;
            self.hold_coast_cooling_samples = 0;
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
                    percent_from_permille(control_target.warmup_power_permille)
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
                    if hold_prediction_blocks_reheat
                        || hold_filter_lag_blocks_reheat
                        || hold_actual_overshoot_blocks_reheat
                    {
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
        let warmup_soft_start_percent = if self.phase == HeaterControlPhase::Warmup {
            self.warmup_started_at_ms
                .map(|started_at_ms| {
                    let elapsed_ms = now_ms.saturating_sub(started_at_ms);
                    (elapsed_ms.saturating_mul(100) / HEATER_WARMUP_SOFT_START_MS).min(100) as u8
                })
                .unwrap_or(100)
        } else {
            100
        };

        HeaterPidSnapshot {
            duty_percent,
            warmup_soft_start_percent,
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
        .max(control_target.hold_power_permille)
        .min(1_000);
    let tail_floor = control_target.hold_power_permille.min(full_floor);
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
            Self::ActiveCooling => FanHardwareCommand::from_profile(FanVoltageProfile::Full),
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
fn is_overtemp_fault(reason: Option<HeaterFaultReason>) -> bool {
    reason == Some(HeaterFaultReason::OverTemp)
}

#[cfg(any(target_arch = "xtensa", test))]
fn auto_cooling_command(
    current_temp_c: i16,
    elapsed_ms: u64,
    previous_state: FanPolicyState,
) -> FanPolicyState {
    if current_temp_c >= ACTIVE_COOLING_FAN_MIN_TEMP_C {
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
fn overtemp_forced_fan_state(
    current_temp_c: i16,
    forced_fan_active: bool,
) -> Option<FanPolicyState> {
    if !forced_fan_active || current_temp_c < FORCED_COOLING_FAN_MIN_TEMP_C {
        None
    } else if current_temp_c > FORCED_COOLING_FAN_FULL_TEMP_C {
        Some(FanPolicyState::Full)
    } else {
        Some(FanPolicyState::SafeHalf)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn startup_pd_contract_ready(observation: Option<PdStatusObservation>) -> bool {
    observation.is_some_and(|observation| observation.status.pd_active)
}

#[cfg(test)]
const STARTUP_PD_WAIT_BUDGET_MS: u64 = 0;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartupFrontPanelPresentation {
    Dashboard,
    Calibration,
}

#[cfg(any(target_arch = "xtensa", test))]
const fn startup_frontpanel_presentation(
    runtime_mode: FrontPanelRuntimeMode,
) -> StartupFrontPanelPresentation {
    match runtime_mode {
        FrontPanelRuntimeMode::App => StartupFrontPanelPresentation::Dashboard,
        FrontPanelRuntimeMode::KeyTest => StartupFrontPanelPresentation::Calibration,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn next_heater_lock_reason(
    heater_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    thermal_model_heater_allowed: bool,
    pd_contract_ready: bool,
) -> Option<HeaterLockReason> {
    if is_sensor_fault(heater_fault) {
        Some(HeaterLockReason::SensorFault)
    } else if heater_fault == Some(HeaterFaultReason::OverTemp) {
        Some(HeaterLockReason::HardOvertemp)
    } else if cooling_disabled_lock_latched {
        Some(HeaterLockReason::CoolingDisabledOvertemp)
    } else if !pd_contract_ready {
        Some(HeaterLockReason::PdContractUnavailable)
    } else if !thermal_model_heater_allowed {
        Some(HeaterLockReason::ThermalModelMissingForSourceClass)
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
fn overtemp_fault_from_control_temperature(temp_c: f32) -> Option<HeaterFaultReason> {
    is_overtemp_sample(temp_c).then_some(HeaterFaultReason::OverTemp)
}

#[cfg(any(target_arch = "xtensa", test))]
fn clear_runtime_temperature(latest_temp_c: &mut f32, latest_temp_i16: &mut i16) {
    *latest_temp_c = 0.0;
    *latest_temp_i16 = 0;
}

#[cfg(any(target_arch = "xtensa", test))]
fn sync_runtime_temperature_ui(
    ui_state: &mut FrontPanelUiState,
    current_temp_c: i16,
    current_temp_deci_c: i16,
) -> bool {
    let mut needs_redraw = false;
    if ui_state.current_temp_c != current_temp_c {
        ui_state.current_temp_c = current_temp_c;
        needs_redraw = true;
    }
    if ui_state.current_temp_deci_c != current_temp_deci_c {
        ui_state.current_temp_deci_c = current_temp_deci_c;
        needs_redraw = true;
    }
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
fn update_runtime_display_temperature(
    ui_state: &mut FrontPanelUiState,
    latest_display_temp_c: &mut f32,
    latest_display_temp_i16: &mut i16,
    temp_c: f32,
) -> bool {
    *latest_display_temp_c = temp_c;
    *latest_display_temp_i16 = temp_c_to_whole_c(temp_c);
    let mut needs_redraw = false;
    if matches!(
        ui_state.dashboard_presentation,
        flux_purr_firmware::frontpanel::DashboardPresentationState::Initializing
            | flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault
    ) {
        needs_redraw = ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
        );
    }
    needs_redraw |=
        sync_runtime_temperature_ui(ui_state, *latest_display_temp_i16, temp_c_to_deci_c(temp_c));
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
struct RuntimeDisplayTemperatureState<'a> {
    ui_state: &'a mut FrontPanelUiState,
    latest_display_temp_c: &'a mut f32,
    latest_display_temp_i16: &'a mut i16,
}

#[cfg(any(target_arch = "xtensa", test))]
struct RuntimeControlTemperatureState<'a> {
    latest_control_temp_c: &'a mut f32,
    latest_control_temp_i16: &'a mut i16,
    transition_guard: &'a mut RtdPpsTransitionGuard,
    measurement_guard: &'a mut RtdControlMeasurementGuard,
    control_measurement_guarded: &'a mut bool,
    heater_controller: &'a mut HeaterController,
}

#[cfg(any(target_arch = "xtensa", test))]
fn apply_valid_rtd_measurement(
    display: RuntimeDisplayTemperatureState<'_>,
    control: RuntimeControlTemperatureState<'_>,
    request_mv: u16,
    now_ms: u64,
    measurement_temp_c: f32,
) -> bool {
    let needs_redraw = update_runtime_display_temperature(
        display.ui_state,
        display.latest_display_temp_c,
        display.latest_display_temp_i16,
        measurement_temp_c,
    );
    if let Some(control_temp_c) = accept_rtd_control_sample_after_pps_transition(
        control.transition_guard,
        control.heater_controller,
        control.measurement_guard,
        *control.latest_control_temp_c,
        request_mv,
        now_ms,
        measurement_temp_c,
    ) {
        *control.latest_control_temp_c = control_temp_c;
        *control.latest_control_temp_i16 = temp_c_to_whole_c(control_temp_c);
    }
    *control.control_measurement_guarded = control.measurement_guard.guarded;
    needs_redraw
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn retain_runtime_display_temperature(
    ui_state: &mut FrontPanelUiState,
    latest_display_temp_c: &mut f32,
    latest_display_temp_i16: &mut i16,
) -> bool {
    sync_runtime_temperature_ui(
        ui_state,
        *latest_display_temp_i16,
        temp_c_to_deci_c(*latest_display_temp_c),
    )
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RtdPpsTransitionGuard {
    request_mv: Option<u16>,
    blocked_until_ms: Option<u64>,
}

#[cfg(any(target_arch = "xtensa", test))]
impl RtdPpsTransitionGuard {
    fn new(request_mv: u16) -> Self {
        Self {
            request_mv: Some(request_mv),
            ..Self::default()
        }
    }

    fn observe(&mut self, request_mv: u16, now_ms: u64) -> (bool, bool) {
        let request_changed = self.request_mv != Some(request_mv);
        self.request_mv = Some(request_mv);
        if request_changed {
            self.blocked_until_ms =
                Some(now_ms.saturating_add(RTD_CONTROL_SAMPLE_STABLE_AFTER_REQUEST_MS));
            return (false, true);
        }

        let accept_control_sample = match self.blocked_until_ms {
            Some(blocked_until_ms) if now_ms < blocked_until_ms => false,
            Some(_) => {
                self.blocked_until_ms = None;
                return (true, true);
            }
            None => true,
        };

        (accept_control_sample, false)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RtdControlMeasurementGuard {
    last_accepted_temp_c: Option<f32>,
    last_accepted_at_ms: Option<u64>,
    guarded_candidate_temp_c: Option<f32>,
    guarded_candidate_since_ms: Option<u64>,
    guarded: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl RtdControlMeasurementGuard {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn reseed(&mut self, temp_c: f32, now_ms: u64) {
        self.last_accepted_temp_c = Some(temp_c);
        self.last_accepted_at_ms = Some(now_ms);
        self.guarded_candidate_temp_c = None;
        self.guarded_candidate_since_ms = None;
        self.guarded = false;
    }

    fn observe(&mut self, measurement_temp_c: f32, now_ms: u64) -> Option<f32> {
        self.guarded = false;
        let Some(last_temp_c) = self.last_accepted_temp_c else {
            self.reseed(measurement_temp_c, now_ms);
            return Some(measurement_temp_c);
        };
        let elapsed_ms = now_ms
            .saturating_sub(self.last_accepted_at_ms.unwrap_or(now_ms))
            .max(25);
        let max_delta_c = (RTD_CONTROL_MAX_SLEW_C_PER_S * (elapsed_ms as f32 / 1_000.0))
            .min(RTD_CONTROL_MAX_ACCEPTED_STEP_C);
        if (measurement_temp_c - last_temp_c).abs() > max_delta_c {
            let candidate_is_consistent = self.guarded_candidate_temp_c.is_some_and(|candidate| {
                (measurement_temp_c - candidate).abs() <= RTD_CONTROL_GUARD_RECOVERY_BAND_C
            });
            if !candidate_is_consistent {
                self.guarded_candidate_temp_c = Some(measurement_temp_c);
                self.guarded_candidate_since_ms = Some(now_ms);
            }
            if candidate_is_consistent
                && now_ms.saturating_sub(self.guarded_candidate_since_ms.unwrap_or(now_ms))
                    >= RTD_CONTROL_GUARD_RECOVERY_WINDOW_MS
            {
                self.reseed(measurement_temp_c, now_ms);
                return Some(measurement_temp_c);
            }
            self.guarded = true;
            return None;
        }

        self.reseed(measurement_temp_c, now_ms);
        Some(measurement_temp_c)
    }

    fn observe_with_heater_duty(
        &mut self,
        measurement_temp_c: f32,
        now_ms: u64,
        heater_duty_percent: u8,
    ) -> Option<f32> {
        if let (Some(last_temp_c), Some(last_at_ms)) =
            (self.last_accepted_temp_c, self.last_accepted_at_ms)
        {
            let elapsed_ms = now_ms.saturating_sub(last_at_ms).max(25);
            let max_unpowered_rise_c = (RTD_CONTROL_MAX_UNPOWERED_RISE_C_PER_S
                * (elapsed_ms as f32 / 1_000.0))
                .min(RTD_CONTROL_MAX_UNPOWERED_RISE_STEP_C);
            if heater_duty_percent == 0
                && measurement_temp_c > last_temp_c
                && measurement_temp_c - last_temp_c > max_unpowered_rise_c
            {
                // A plate can retain heat after coasting, but it cannot make a
                // multi-degree step while the heater output is physically off.
                // Keep the raw reading visible and preserve the last trusted
                // control temperature instead of allowing a persistent ADC
                // artifact to reseed the control loop.
                self.guarded_candidate_temp_c = None;
                self.guarded_candidate_since_ms = None;
                self.guarded = true;
                return None;
            }
        }

        self.observe(measurement_temp_c, now_ms)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn preserve_rtd_control_guard_when_heater_disabled(
    heater_enabled: bool,
    measurement_guarded: &mut bool,
) {
    if !heater_enabled {
        *measurement_guarded = false;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn accept_rtd_control_sample_after_pps_transition(
    transition_guard: &mut RtdPpsTransitionGuard,
    heater_controller: &mut HeaterController,
    measurement_guard: &mut RtdControlMeasurementGuard,
    last_control_temp_c: f32,
    request_mv: u16,
    now_ms: u64,
    measurement_temp_c: f32,
) -> Option<f32> {
    let (accept_control_sample, reseed_filter) = transition_guard.observe(request_mv, now_ms);
    if reseed_filter {
        heater_controller.reseed_measurement(last_control_temp_c);
        measurement_guard.reseed(last_control_temp_c, now_ms);
    }
    let was_guarded = measurement_guard.guarded;
    let accepted = accept_control_sample
        .then(|| {
            measurement_guard.observe_with_heater_duty(
                measurement_temp_c,
                now_ms,
                heater_controller.duty_percent,
            )
        })
        .flatten();
    if was_guarded && accepted.is_some() {
        heater_controller.reseed_measurement(measurement_temp_c);
    }
    accepted
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_retry_rtd_sample_after_power_step(
    previous_request_mv: u16,
    current_request_mv: u16,
    previous_vin_raw_adc_mv: u16,
    current_vin_raw_adc_mv: u16,
) -> bool {
    previous_request_mv != current_request_mv
        || previous_vin_raw_adc_mv.abs_diff(current_vin_raw_adc_mv)
            >= RTD_RETRY_AFTER_VIN_STEP_RAW_ADC_DELTA_MV
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
trait BuzzerCueSink {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64);
    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64);
    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64);
    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    );
    fn clear_attention(&mut self);
    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64);
}

#[cfg(test)]
impl BuzzerCueSink for BuzzerArbiter {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64) {
        let _ = BuzzerArbiter::request_feedback(self, source, cue, now_ms);
    }

    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::activate_protection(self, source, now_ms);
    }

    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::request_protection_replay(self, source, now_ms);
    }

    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) {
        let _ = BuzzerArbiter::enter_attention_pending(self);
        let _ = BuzzerArbiter::request_attention_reminder(self, source, now_ms);
    }

    fn clear_attention(&mut self) {
        let _ = BuzzerArbiter::clear_attention(self);
    }

    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64) {
        let _ = BuzzerArbiter::request_attention_reminder(self, source, now_ms);
    }
}

#[cfg(target_arch = "xtensa")]
impl BuzzerCueSink for BuzzerRuntime {
    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, now_ms: u64) {
        BuzzerRuntime::request_feedback(self, source, cue, now_ms);
    }

    fn activate_protection(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::activate_protection(self, source, now_ms);
    }

    fn request_protection_replay(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::request_protection_replay(self, source, now_ms);
    }

    fn enter_attention_pending_and_request_reminder(
        &mut self,
        source: BuzzerCueSource,
        now_ms: u64,
    ) {
        BuzzerRuntime::enter_attention_pending_and_request_reminder(self, source, now_ms);
    }

    fn clear_attention(&mut self) {
        self.clear_attention();
    }

    fn request_attention_reminder(&mut self, source: BuzzerCueSource, now_ms: u64) {
        BuzzerRuntime::request_attention_reminder(self, source, now_ms);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
struct FaultAttentionState<'a> {
    last_fault_present: &'a mut bool,
    attention_acknowledged: &'a mut bool,
    attention_pending_after_fault_clear: &'a mut bool,
    forced_fan_active: &'a mut bool,
    protection_alarm: &'a mut ProtectionAlarmCadence,
    next_attention_reminder_ms: &'a mut Option<u64>,
}

#[cfg(any(target_arch = "xtensa", test))]
fn update_fault_attention_state<B: BuzzerCueSink>(
    fault_present: bool,
    state: FaultAttentionState<'_>,
    current_temp_c: i16,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    let FaultAttentionState {
        last_fault_present,
        attention_acknowledged,
        attention_pending_after_fault_clear,
        forced_fan_active,
        protection_alarm,
        next_attention_reminder_ms,
    } = state;
    let mut changed = false;

    if fault_present && !*last_fault_present {
        *attention_acknowledged = false;
        *attention_pending_after_fault_clear = false;
        *forced_fan_active = true;
        *next_attention_reminder_ms = None;
        protection_alarm.arm(now_ms);
        buzzer.activate_protection(BuzzerCueSource::ThermalProtection, now_ms);
        changed = true;
    } else if !fault_present && *last_fault_present {
        *attention_pending_after_fault_clear = !*attention_acknowledged;
        protection_alarm.clear();
        if *attention_pending_after_fault_clear {
            // The protection alarm has just stopped. Give the operator an
            // immediate reminder, then keep the existing ten-second cadence.
            buzzer.enter_attention_pending_and_request_reminder(
                BuzzerCueSource::ThermalAttention,
                now_ms,
            );
            *next_attention_reminder_ms =
                Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        } else {
            buzzer.clear_attention();
            *next_attention_reminder_ms = None;
        }
        changed = true;
    }

    if *forced_fan_active
        && (*attention_acknowledged || current_temp_c < FORCED_COOLING_FAN_MIN_TEMP_C)
    {
        *forced_fan_active = false;
        changed = true;
    }

    *last_fault_present = fault_present;
    changed
}

#[cfg(any(target_arch = "xtensa", test))]
fn overtemp_attention_requires_ack(
    overtemp_active: bool,
    attention_acknowledged: bool,
    attention_pending_after_fault_clear: bool,
) -> bool {
    (overtemp_active && !attention_acknowledged) || attention_pending_after_fault_clear
}

#[cfg(any(target_arch = "xtensa", test))]
fn acknowledge_overtemp_attention<B: BuzzerCueSink>(
    overtemp_active: bool,
    attention_acknowledged: &mut bool,
    attention_pending_after_fault_clear: &mut bool,
    forced_fan_active: &mut bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut B,
) -> bool {
    if !overtemp_attention_requires_ack(
        overtemp_active,
        *attention_acknowledged,
        *attention_pending_after_fault_clear,
    ) {
        return false;
    }

    *attention_acknowledged = true;
    *attention_pending_after_fault_clear = false;
    *forced_fan_active = false;
    *next_attention_reminder_ms = None;
    if !overtemp_active {
        buzzer.clear_attention();
    }
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn maybe_play_protection_alarm<B: BuzzerCueSink>(
    fault_present: bool,
    protection_alarm: &mut ProtectionAlarmCadence,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !protection_alarm.replay_due(fault_present, now_ms) {
        return false;
    }
    buzzer.request_protection_replay(BuzzerCueSource::ThermalProtection, now_ms);
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
fn maybe_play_attention_reminder<B: BuzzerCueSink>(
    attention_pending_after_fault_clear: bool,
    fault_present: bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !attention_pending_after_fault_clear || fault_present {
        return false;
    }

    if next_attention_reminder_ms.is_some_and(|next| now_ms >= next) {
        buzzer.request_attention_reminder(BuzzerCueSource::ThermalAttention, now_ms);
        *next_attention_reminder_ms =
            Some(now_ms.saturating_add(BUZZER_ATTENTION_REMINDER_INTERVAL_MS));
        return true;
    }

    false
}

#[cfg(any(target_arch = "xtensa", test))]
fn maybe_play_frontpanel_ui_input_feedback<B: BuzzerCueSink>(
    interaction_handled: bool,
    specialized_feedback_played: bool,
    buzzer: &mut B,
    now_ms: u64,
) -> bool {
    if !interaction_handled || specialized_feedback_played {
        return false;
    }

    buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, now_ms);
    true
}

#[cfg(target_arch = "xtensa")]
fn log_buzzer_decision(decision: BuzzerDecision) {
    info!(
        "buzzer arbitration source={=str} cue={=str} disposition={=str}",
        decision.source.label(),
        decision.cue.label(),
        decision.disposition.label(),
    );
}

#[cfg(any(target_arch = "xtensa", test))]
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

#[cfg(any(target_arch = "xtensa", test))]
fn temp_c_to_whole_c(temp_c: f32) -> i16 {
    let rounded = if temp_c >= 0.0 {
        temp_c + 0.5
    } else {
        temp_c - 0.5
    };
    rounded.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct RtdMeasurement {
    raw_adc_mv: u16,
    raw_adc_min_mv: u16,
    raw_adc_max_mv: u16,
    adc_mv: u16,
    resistance_ohms: f32,
    temp_c: f32,
    current_temp_c: i16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct RtdAdcBatch {
    mean_mv: f32,
    min_mv: u16,
    max_mv: u16,
    mean_raw_code: u16,
    min_raw_code: u16,
    max_raw_code: u16,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AdcConvertedSample {
    raw_code: u16,
    calibrated_mv: u16,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static ADC_CALIBRATION_SOURCE: AtomicU8 = AtomicU8::new(2);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static ADC_EFUSE_VERSION: AtomicU8 = AtomicU8::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static ADC_INIT_CODE: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static ADC_REFERENCE_CODE: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static ADC_REFERENCE_MV: AtomicU16 = AtomicU16::new(u16::MAX);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static RTD_RAW_CODE_MEAN: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static RTD_RAW_CODE_MIN: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static RTD_RAW_CODE_MAX: AtomicU16 = AtomicU16::new(0);
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
static VIN_RAW_CODE_MEAN: AtomicU16 = AtomicU16::new(0);

#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_IDLE: u8 = 0;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_WAITING_CC_ATTACH: u8 = 1;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_WAITING_SOURCE_CAPS: u8 = 2;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_SOURCE_CAPS_REQUESTED: u8 = 3;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_WAITING_ACCEPT: u8 = 4;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_WAITING_PS_RDY: u8 = 5;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_RECOVERING: u8 = 6;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_FAULT: u8 = 7;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED: u8 = 8;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN: u8 = 9;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_PROTECTION: u8 = 10;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_MISSING_CRC: u8 = 11;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_MISSING_SOP: u8 = 12;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_UNSUPPORTED_SOP: u8 = 13;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_RX_I2C_ERROR: u8 = 14;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_TX_I2C_ERROR: u8 = 15;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_NO_USABLE_CONTRACT: u8 = 16;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_RX_PARTIAL: u8 = 17;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_SOURCE_CAPS_HARD_RESET_SENT: u8 = 18;
#[cfg(any(target_arch = "xtensa", test))]
const FUSB302B_DIAG_REQUEST_TIMEOUT: u8 = 19;
#[cfg(target_arch = "xtensa")]
const FUSB302B_MAX_RX_MESSAGES_PER_POLL: u8 = 4;
#[cfg(target_arch = "xtensa")]
const FUSB302B_PARTIAL_RX_TIMEOUT_MS: u64 = 250;
#[cfg(target_arch = "xtensa")]
const FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS: u64 = 1_500;
#[cfg(any(target_arch = "xtensa", test))]
static FUSB302B_DIAGNOSTIC: AtomicU8 = AtomicU8::new(FUSB302B_DIAG_IDLE);

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn fusb302b_degraded_reason() -> &'static str {
    match FUSB302B_DIAGNOSTIC.load(Ordering::Relaxed) {
        FUSB302B_DIAG_WAITING_CC_ATTACH => "pd_fusb_cc_attach_pending",
        FUSB302B_DIAG_WAITING_SOURCE_CAPS => "pd_fusb_source_caps_waiting",
        FUSB302B_DIAG_SOURCE_CAPS_REQUESTED => "pd_fusb_source_caps_requested",
        FUSB302B_DIAG_WAITING_ACCEPT => "pd_fusb_accept_pending",
        FUSB302B_DIAG_WAITING_PS_RDY => "pd_fusb_ps_rdy_pending",
        FUSB302B_DIAG_RECOVERING => "pd_fusb_phy_recovering",
        FUSB302B_DIAG_FAULT => "pd_fusb_phy_fault",
        FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED => "pd_fusb_source_caps_tx_confirmed",
        FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN => "pd_fusb_source_caps_gcrc_seen",
        FUSB302B_DIAG_PROTECTION => "pd_fusb_phy_protection",
        FUSB302B_DIAG_MISSING_CRC => "pd_fusb_rx_crc_missing",
        FUSB302B_DIAG_MISSING_SOP => "pd_fusb_rx_sop_missing",
        FUSB302B_DIAG_UNSUPPORTED_SOP => "pd_fusb_rx_sop_unsupported",
        FUSB302B_DIAG_RX_I2C_ERROR => "pd_fusb_rx_i2c_error",
        FUSB302B_DIAG_TX_I2C_ERROR => "pd_fusb_tx_i2c_error",
        FUSB302B_DIAG_NO_USABLE_CONTRACT => "pd_fusb_no_usable_contract",
        FUSB302B_DIAG_RX_PARTIAL => "pd_fusb_rx_partial",
        FUSB302B_DIAG_SOURCE_CAPS_HARD_RESET_SENT => "pd_fusb_source_caps_hard_reset_sent",
        FUSB302B_DIAG_REQUEST_TIMEOUT => "pd_fusb_contract_request_timeout",
        _ => "pd_contract_unavailable",
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn adc_diagnostics_wire() -> AdcDiagnosticsWire {
    let optional_code = |value: u16| (value != u16::MAX).then_some(value);
    let raw_min = RTD_RAW_CODE_MIN.load(Ordering::Relaxed);
    let raw_max = RTD_RAW_CODE_MAX.load(Ordering::Relaxed);
    AdcDiagnosticsWire {
        calibration_source: match ADC_CALIBRATION_SOURCE.load(Ordering::Relaxed) {
            0 => AdcCalibrationSourceWire::Efuse,
            1 => AdcCalibrationSourceWire::RuntimeFallback,
            _ => AdcCalibrationSourceWire::Unavailable,
        },
        efuse_version: ADC_EFUSE_VERSION.load(Ordering::Relaxed),
        attenuation_db: 6,
        init_code: optional_code(ADC_INIT_CODE.load(Ordering::Relaxed)),
        reference_code: optional_code(ADC_REFERENCE_CODE.load(Ordering::Relaxed)),
        reference_mv: optional_code(ADC_REFERENCE_MV.load(Ordering::Relaxed)),
        rtd_raw_code_mean: RTD_RAW_CODE_MEAN.load(Ordering::Relaxed),
        rtd_raw_code_min: raw_min,
        rtd_raw_code_max: raw_max,
        rtd_raw_code_spread: raw_max.saturating_sub(raw_min),
        vin_raw_code_mean: VIN_RAW_CODE_MEAN.load(Ordering::Relaxed),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
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
    contract_voltage_mv: Option<u16>,
    contract: Contract,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PdStatusLogKey {
    status_raw: u8,
    pd_active: bool,
    epr_active: bool,
    epr_exist: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
fn pd_status_log_key(observation: Option<PdStatusObservation>) -> Option<PdStatusLogKey> {
    observation.map(|observation| PdStatusLogKey {
        status_raw: observation.status_raw,
        pd_active: observation.status.pd_active,
        epr_active: observation.status.epr_active,
        epr_exist: observation.status.epr_exist,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn pd_contract_allows_calibration(
    controller: ControllerKind,
    observation: Option<PdStatusObservation>,
) -> bool {
    match controller {
        ControllerKind::Fusb302b => observation.is_some_and(|observation| {
            observation.contract.kind == ContractKind::Pps
                && observation.contract.performance_guaranteed()
        }),
        // CH224Q retains its established PPS/AVS calibration policy.
        ControllerKind::Ch224q => true,
        ControllerKind::Unknown => false,
    }
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
        // A terminal thermal-calibration transition has requested fixed PD. Keep
        // the regular PPS governor dormant until a later, explicit re-arm.
        terminal_fixed_pd_disarmed: bool,
    },
    FixedPdPwmFallback {
        reason: HeaterPowerBackendReason,
        fixed_request_confirmed: bool,
        fixed_request: ch224q::VoltageRequest,
    },
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoldPpsGovernor {
    active: bool,
    next_adjust_at_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl HoldPpsGovernor {
    const fn new() -> Self {
        Self {
            active: false,
            next_adjust_at_ms: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn step_request_into_bounds(
        current_request_mv: u16,
        control_floor_mv: u16,
        safe_max_mv: u16,
    ) -> u16 {
        let bounded_safe_max_mv = safe_max_mv.max(control_floor_mv);
        if current_request_mv < control_floor_mv {
            current_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(control_floor_mv)
        } else if current_request_mv > bounded_safe_max_mv {
            current_request_mv
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(bounded_safe_max_mv)
        } else {
            current_request_mv
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn request_mv(
        &mut self,
        phase: HeaterControlPhase,
        duty_percent: u8,
        actual_error_c: f32,
        filtered_slope_c_per_s: f32,
        current_request_mv: u16,
        control_floor_mv: u16,
        safe_max_mv: u16,
        now_ms: u64,
    ) -> Option<u16> {
        // The raw measurement can briefly leave Hold at high temperature while
        // still inside the control loop's near-target band.  Keep PPS headroom
        // adaptation alive through that Approach phase; otherwise its settle
        // timer restarts before a saturated heater can recover to Hold.
        if !matches!(
            phase,
            HeaterControlPhase::Hold | HeaterControlPhase::Approach
        ) {
            self.reset();
            return None;
        }

        // A nonzero command below the working floor is handled by the fixed-PWM safety
        // fallback before this governor. Keep the idle path well-defined too.
        let bounded_safe_max_mv = safe_max_mv.max(control_floor_mv);
        let bounded_request_mv = Self::step_request_into_bounds(
            current_request_mv,
            control_floor_mv,
            bounded_safe_max_mv,
        );
        if !self.active {
            self.active = true;
            // Hold inherits the Approach voltage. PWM handles fast corrections; PPS only
            // rises later if that voltage cannot provide enough heating headroom.
            self.next_adjust_at_ms = now_ms.saturating_add(HEATER_HOLD_PPS_INITIAL_SETTLE_MS);
            return Some(bounded_request_mv);
        }
        if duty_percent == 0 || now_ms < self.next_adjust_at_ms {
            return Some(bounded_request_mv);
        }

        let physical_pwm_percent =
            heater_physical_pwm_percent(duty_percent, bounded_safe_max_mv, bounded_request_mv, 100);
        let raise_voltage = physical_pwm_percent >= HEATER_HOLD_PPS_SATURATION_PWM_MIN_PERCENT
            && actual_error_c >= HEATER_HOLD_PPS_RAISE_ERROR_MIN_C
            && filtered_slope_c_per_s <= HEATER_HOLD_PPS_RAISE_MAX_SLOPE_C_PER_S;
        let next_request_mv = if raise_voltage {
            bounded_request_mv
                .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
                .min(bounded_safe_max_mv)
        } else {
            bounded_request_mv
        };

        self.next_adjust_at_ms = now_ms.saturating_add(HEATER_HOLD_PPS_STEADY_DWELL_MS);
        Some(next_request_mv)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualPpsError {
    NoPpsCapability,
    InvalidVoltage,
    CalibrationInProgress,
    TerminalDisarmPending,
    ThermalPlantManagedByJob,
    HeaterCurveCoverageInsufficient,
    ThermalPlantSourceUnsupported,
    ThermalPlantProjectionInvalid,
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
            Self::CalibrationInProgress => "manual_pps_calibration_busy",
            Self::TerminalDisarmPending => "heater_disarm_pending",
            Self::ThermalPlantManagedByJob => "thermal_plant_managed_by_job",
            Self::HeaterCurveCoverageInsufficient => "heater_curve_coverage_insufficient",
            Self::ThermalPlantSourceUnsupported => "thermal_plant_source_unsupported",
            Self::ThermalPlantProjectionInvalid => "thermal_plant_projection_invalid",
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
            Self::CalibrationInProgress => {
                "Manual PPS cannot override a running thermal-model calibration."
            }
            Self::TerminalDisarmPending => {
                "The previous heater session is still being physically disarmed."
            }
            Self::ThermalPlantManagedByJob => {
                "Automatic thermal-model calibration is managed by thermal_plant_auto."
            }
            Self::HeaterCurveCoverageInsufficient => {
                "The transient thermal-model run did not collect enough heater-curve samples."
            }
            Self::ThermalPlantSourceUnsupported => {
                "Thermal plant calibration requires a PPS APDO covering 20V at 3A or more."
            }
            Self::ThermalPlantProjectionInvalid => {
                "Thermal plant observations did not produce a physical model."
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
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationMode {
    const fn to_wire(self) -> CalibrationModeWire {
        match self {
            Self::Off => CalibrationModeWire::Off,
            Self::VinAdc => CalibrationModeWire::VinAdc,
            Self::RtdAdc => CalibrationModeWire::RtdAdc,
            Self::HeaterCurve => CalibrationModeWire::HeaterCurve,
            Self::ThermalPlant => CalibrationModeWire::ThermalPlant,
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
            CalibrationModeWire::ThermalPlant => Self::ThermalPlant,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationJobKind {
    VinAdc,
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
impl CalibrationJobKind {
    const fn to_wire(self) -> CalibrationJobKindWire {
        match self {
            Self::VinAdc => CalibrationJobKindWire::VinAdcAuto,
            Self::ThermalPlant => CalibrationJobKindWire::ThermalPlantAuto,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
impl From<CalibrationJobKindWire> for CalibrationJobKind {
    fn from(value: CalibrationJobKindWire) -> Self {
        match value {
            CalibrationJobKindWire::VinAdcAuto => Self::VinAdc,
            CalibrationJobKindWire::ThermalPlantAuto => Self::ThermalPlant,
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
const THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN: u16 = 20;
#[cfg(any(target_arch = "xtensa", test))]
const CALIBRATION_VIN_AUTO_MIN_MOVED_ADC_MV: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_AMBIENT_TICKS: u16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_HEAT_TIMEOUT_TICKS: u32 = 24_000;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_COOL_TIMEOUT_TICKS: u32 = 24_000;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_TARGET_TEMP_C: f32 = 220.0;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_COOL_COMPLETE_TEMP_C: f32 = 80.0;
#[cfg(any(target_arch = "xtensa", test))]
const THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C: f32 = 4.0;

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
struct ThermalPlantCurveBin {
    min_temp_c: f32,
    max_temp_c: f32,
    samples: u16,
    temp_sum_c: f32,
    resistance_sum_ohms: f32,
    raw_adc_sum_mv: u32,
    voltage_sum_mv: u64,
    current_sum_ma: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
impl ThermalPlantCurveBin {
    const fn new(min_temp_c: f32, max_temp_c: f32) -> Self {
        Self {
            min_temp_c,
            max_temp_c,
            samples: 0,
            temp_sum_c: 0.0,
            resistance_sum_ohms: 0.0,
            raw_adc_sum_mv: 0,
            voltage_sum_mv: 0,
            current_sum_ma: 0,
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

    fn observe_electrical(
        &mut self,
        temp_c: f32,
        raw_rtd_adc_mv: u16,
        heater_voltage_mv: u32,
        heater_current_ma: u16,
    ) {
        self.observe(
            temp_c,
            heater_voltage_mv as f32 / f32::from(heater_current_ma),
        );
        self.raw_adc_sum_mv = self
            .raw_adc_sum_mv
            .saturating_add(u32::from(raw_rtd_adc_mv));
        self.voltage_sum_mv = self
            .voltage_sum_mv
            .saturating_add(u64::from(heater_voltage_mv));
        self.current_sum_ma = self
            .current_sum_ma
            .saturating_add(u64::from(heater_current_ma));
    }

    fn averaged_raw_observation(self) -> Option<HeaterCurveRawObservation> {
        if self.samples == 0 || self.raw_adc_sum_mv == 0 || self.current_sum_ma == 0 {
            return None;
        }
        let samples = u64::from(self.samples);
        let voltage_mv = (self.voltage_sum_mv / samples).min(u64::from(u16::MAX)) as u16;
        let current_ma = (self.current_sum_ma / samples).min(u64::from(u16::MAX)) as u16;
        Some(HeaterCurveRawObservation {
            raw_rtd_adc_mv: (u64::from(self.raw_adc_sum_mv) / samples).min(u64::from(u16::MAX))
                as u16,
            heater_voltage_mv: voltage_mv,
            heater_current_ma: current_ma,
            resistance_milliohms: ((u32::from(voltage_mv) * 1_000) / u32::from(current_ma))
                .min(u32::from(u16::MAX)) as u16,
        })
    }

    fn averaged_point(self) -> Option<(i16, u16)> {
        if self.samples == 0 {
            return None;
        }
        let temp_c = self.temp_sum_c / f32::from(self.samples);
        let measured_resistance_ohms = self.resistance_sum_ohms / f32::from(self.samples);
        let resistance_ohms =
            measured_resistance_ohms.max(default_estimated_heater_resistance_ohms(temp_c));
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
struct ThermalPlantCurveSampler {
    cold_bin: ThermalPlantCurveBin,
    bins: [ThermalPlantCurveBin; 4],
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum ThermalPlantAutoPhase {
    Ambient,
    Heating,
    Cooling,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug)]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
struct CalibrationThermalPlantAutoJob {
    run_id: u32,
    phase: ThermalPlantAutoPhase,
    source_max_mv: u16,
    source_current_ma: u16,
    ambient_raw_rtd_adc_mv: u16,
    idle_samples: u16,
    heater_curve: ThermalPlantCurveSampler,
    elapsed_ticks: u32,
    phase_started_tick: u32,
    sample_count: u8,
    last_saved_temp_c: f32,
    last_saved_tick: u16,
    samples: [ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
}

// Keep the bounded transient trace out of the Embassy main task. The job is
// large enough that retaining it in CalibrationRuntimeState exhausts the
// application stack during Wi-Fi initialization.
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Debug, Default)]
struct CalibrationThermalPlantWorkspace {
    job: Option<CalibrationThermalPlantAutoJob>,
    next_run_id: u32,
}

#[cfg(any(target_arch = "xtensa", test))]
impl Default for ThermalPlantCurveSampler {
    fn default() -> Self {
        Self {
            cold_bin: ThermalPlantCurveBin::new(0.0, 80.0),
            bins: [
                ThermalPlantCurveBin::new(80.0, 120.0),
                ThermalPlantCurveBin::new(120.0, 160.0),
                ThermalPlantCurveBin::new(160.0, 190.0),
                ThermalPlantCurveBin::new(190.0, 221.0),
            ],
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
enum CalibrationJobData {
    VinAdc(CalibrationVinAutoJob),
    ThermalPlant,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeaterCurvePreview {
    curve: HeaterCurveConfig,
    raw_observations: Option<HeaterCurveRawObservations>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn preview_heater_curve_config(preview: Option<&HeaterCurvePreview>) -> Option<&HeaterCurveConfig> {
    preview.map(|preview| &preview.curve)
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
    model_target_temp_c: Option<i16>,
    thermal_plant_completion_disarm_pending: bool,
    immediate_heater_disarm_pending: bool,
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
            model_target_temp_c: None,
            thermal_plant_completion_disarm_pending: false,
            immediate_heater_disarm_pending: false,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_runtime_state_to_wire(
    state: &CalibrationRuntimeState,
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
    calibration_runtime_state: impl core::borrow::Borrow<CalibrationRuntimeState>,
    current_rtd_fault: Option<HeaterFaultReason>,
    cooling_disabled_lock_latched: bool,
    heater_fault_latched: bool,
    thermal_model_heater_allowed: bool,
    pd_contract_ready: bool,
) -> bool {
    let calibration_runtime_state = calibration_runtime_state.borrow();
    if calibration_runtime_state.mode == CalibrationMode::Off {
        return current_heater_enabled && thermal_model_heater_allowed && pd_contract_ready;
    }

    let calibration_heater_allowed = !is_sensor_fault(current_rtd_fault)
        && !cooling_disabled_lock_latched
        && !heater_fault_latched
        && pd_contract_ready;
    if calibration_runtime_state.mode == CalibrationMode::ThermalPlant
        && calibration_runtime_state.job.status != CalibrationJobStatus::Running
    {
        return false;
    }
    calibration_runtime_state.heater_enabled && calibration_heater_allowed
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_calibration_snapshot(
    measured_temp_c: f32,
    heater_enabled: bool,
) -> HeaterPidSnapshot {
    HeaterPidSnapshot {
        duty_percent: u8::from(heater_enabled) * 100,
        warmup_soft_start_percent: 100,
        error_c: THERMAL_PLANT_TARGET_TEMP_C - measured_temp_c,
        control_error_c: THERMAL_PLANT_TARGET_TEMP_C - measured_temp_c,
        filtered_temp_c: measured_temp_c,
        filtered_slope_c_per_s: 0.0,
        coast_active: false,
        phase: HeaterControlPhase::Warmup,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_calibration_temperature_c(
    calibration: CalibrationRuntimeState,
    live_rtd_temp_c: Option<f32>,
    control_temp_c: f32,
) -> f32 {
    if thermal_plant_calibration_job_running(calibration) {
        live_rtd_temp_c.unwrap_or(control_temp_c)
    } else {
        control_temp_c
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_cooling_complete(live_temp_c: f32, recorded_temp_c: f32) -> bool {
    live_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
        && recorded_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_output_must_be_off(
    calibration: CalibrationRuntimeState,
    was_running: bool,
    measured_temp_c: f32,
) -> bool {
    if !was_running || calibration.job.kind != Some(CalibrationJobKind::ThermalPlant) {
        return false;
    }
    measured_temp_c >= THERMAL_PLANT_TARGET_TEMP_C
        || calibration.job.status != CalibrationJobStatus::Running
}

#[cfg(any(target_arch = "xtensa", test))]
fn consume_thermal_plant_completion_disarm(
    calibration_runtime_state: &mut CalibrationRuntimeState,
    desired_heater_enabled: bool,
) -> bool {
    if calibration_runtime_state.thermal_plant_completion_disarm_pending {
        calibration_runtime_state.thermal_plant_completion_disarm_pending = false;
        false
    } else {
        desired_heater_enabled
    }
}

#[cfg(test)]
fn take_immediate_heater_disarm(calibration_runtime_state: &mut CalibrationRuntimeState) -> bool {
    let pending = calibration_runtime_state.immediate_heater_disarm_pending;
    calibration_runtime_state.immediate_heater_disarm_pending = false;
    pending
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_model_heater_allowed(
    memory_config: &MemoryConfig,
    calibration: impl core::borrow::Borrow<CalibrationRuntimeState>,
    manual_pps: ManualPpsState,
) -> bool {
    let calibration = calibration.borrow();
    if calibration.mode == CalibrationMode::ThermalPlant {
        return calibration.job.status == CalibrationJobStatus::Running;
    }
    if calibration.mode != CalibrationMode::Off {
        return true;
    }
    if memory_config.commissioning_required {
        return false;
    }
    let plant_is_available =
        memory_config
            .thermal_plant_transient_active
            .is_some_and(|transaction| {
                thermal_plant_projection_from_transient(&transaction).is_some()
                    && thermal_plant_transient_trace_reaches_targets(&transaction, memory_config)
                    && thermal_plant_curve_is_bound(memory_config, transaction)
            });
    if !plant_is_available {
        return false;
    }

    manual_pps.has_matching_pps_apdo(20_000, 3_000)
        && has_persisted_heater_resistance_curve(memory_config)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_curve_is_bound(
    memory_config: &MemoryConfig,
    transaction: ThermalPlantTransientTransaction,
) -> bool {
    memory_config.heater_curve_transaction_id == Some(transaction.transaction_id)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_transient_trace_reaches_targets(
    transaction: &ThermalPlantTransientTransaction,
    memory_config: &MemoryConfig,
) -> bool {
    if !flux_purr_firmware::memory::thermal_plant_transient_transaction_is_complete(transaction) {
        return false;
    }
    let count = usize::from(transaction.sample_count);
    if !(24..=THERMAL_PLANT_TRANSIENT_MAX_SAMPLES).contains(&count) {
        return false;
    }

    let Some(ambient_temp_c) =
        projected_rtd_temperature_c(memory_config, transaction.ambient_raw_rtd_adc_mv)
    else {
        return false;
    };
    let samples = &transaction.samples[..count];
    let mut temperatures_c = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powered_max_temp_c = f32::MIN;
    let mut powered_peak_index = None;
    for (index, sample) in samples.iter().enumerate() {
        let Some(temperature_c) = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)
        else {
            return false;
        };
        if !temperature_c.is_finite() {
            return false;
        }
        if sample.duty_percent > 0 && temperature_c > powered_max_temp_c {
            powered_max_temp_c = temperature_c;
            powered_peak_index = Some(index);
        }
        temperatures_c[index] = temperature_c;
    }

    let Some(powered_peak_index) = powered_peak_index else {
        return false;
    };
    let final_sample = samples.last().copied();
    let final_temp_c = temperatures_c[count - 1];
    powered_max_temp_c >= THERMAL_PLANT_TARGET_TEMP_C
        && (temperatures_c[0] - ambient_temp_c).abs() <= 8.0
        && powered_peak_index + 1 < count
        && final_sample.is_some_and(|sample| sample.duty_percent == 0)
        && final_temp_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C
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

    fn thermal_plant_source_limits(&self) -> Option<(u16, u16, u16)> {
        let mut selected = None;
        for apdo in self.capability_apdos.iter().flatten() {
            let min_mv = apdo.min_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV);
            let max_mv = apdo.max_mv.min(HEATER_ADJUSTABLE_MAX_MV);
            if min_mv > 20_000 || max_mv < 20_000 || apdo.max_ma < 3_000 {
                continue;
            }
            let candidate = (min_mv, max_mv, apdo.max_ma);
            if selected.is_none_or(|(selected_min_mv, selected_max_mv, selected_ma)| {
                candidate.2 > selected_ma
                    || (candidate.2 == selected_ma
                        && (candidate.1 > selected_max_mv
                            || (candidate.1 == selected_max_mv && candidate.0 < selected_min_mv)))
            }) {
                selected = Some(candidate);
            }
        }
        selected
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
    error_code_string(error.code())
}

#[cfg(any(target_arch = "xtensa", test))]
fn error_code_string(
    value: &str,
) -> heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }> {
    let mut out = heapless::String::new();
    let _ = out.push_str(value);
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

#[cfg(any(target_arch = "xtensa", test))]
fn effective_pd_contract_mv(
    manual_pps: &ManualPpsState,
    observation: Option<PdStatusObservation>,
    backend: HeaterPowerBackend,
) -> u16 {
    manual_pps
        .target_mv
        .filter(|_| manual_pps.enabled)
        .or_else(|| {
            observation
                .filter(|observation| observation.status.pd_active)
                .and_then(|observation| observation.contract_voltage_mv)
        })
        .unwrap_or_else(|| backend.pd_contract_mv())
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
fn projected_rtd_temperature_c(memory_config: &MemoryConfig, raw_adc_mv: u16) -> Option<f32> {
    let corrected_mv = correct_adc_fractional_mv(
        memory_config,
        AdcCalibrationChannel::Rtd,
        f32::from(raw_adc_mv),
    );
    rtd_resistance_ohms_from_fractional_mv(corrected_mv)
        .ok()
        .map(pt1000_temperature_c_from_resistance)
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_runtime_wire(memory_config: &MemoryConfig) -> ThermalPlantRuntimeWire {
    let active_projection = memory_config
        .thermal_plant_transient_active
        .and_then(|transaction| {
            (thermal_plant_curve_is_bound(memory_config, transaction)
                && thermal_plant_transient_trace_reaches_targets(&transaction, memory_config))
            .then(|| thermal_plant_projection_from_transient(&transaction))
            .flatten()
        });
    let state =
        if memory_config.thermal_plant_transient_active.is_some() && active_projection.is_none() {
            "invalid"
        } else if active_projection.is_some() {
            "active"
        } else {
            "missing"
        };
    let mut state_wire = heapless::String::new();
    let _ = state_wire.push_str(state);
    ThermalPlantRuntimeWire {
        state: state_wire,
        active_transaction_id: memory_config
            .thermal_plant_transient_active
            .map(|transaction| transaction.transaction_id),
        projection_valid: active_projection.is_some(),
        convection_mw_per_c: active_projection.map(|projection| projection.convection_mw_per_c),
        radiation_mw_per_k4: active_projection.map(|projection| projection.radiation_mw_per_k4),
        thermal_capacity_mj_per_c: active_projection
            .map(|projection| projection.thermal_capacity_mj_per_c),
        transport_delay_ms: active_projection.map(|projection| projection.transport_delay_ms),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_plant_run_snapshot_wire(
    calibration: &CalibrationRuntimeState,
    memory_config: &MemoryConfig,
    workspace: &CalibrationThermalPlantWorkspace,
    after_sample: u8,
    current_temp_c: f32,
    heater_voltage_mv: u32,
    duty_percent: u8,
) -> ThermalPlantRunSnapshotWire {
    let current_temp_centi_c = round_to_i16(current_temp_c * 100.0);
    let current_voltage_mv = heater_voltage_mv.min(u32::from(u16::MAX)) as u16;
    let job = workspace.job.as_ref();
    let persisted = memory_config.thermal_plant_transient_active.as_ref();
    let (samples, sample_count) = if let Some(job) = job {
        (&job.samples[..], job.sample_count)
    } else if let Some(transaction) = persisted {
        (&transaction.samples[..], transaction.sample_count)
    } else {
        (&[][..], 0)
    };
    let run_id = job
        .map(|value| value.run_id)
        .or_else(|| (workspace.next_run_id != 0).then_some(workspace.next_run_id))
        .or_else(|| persisted.map(|transaction| transaction.transaction_id))
        .unwrap_or(0);

    let start = after_sample.min(sample_count);
    let mut points = heapless::Vec::new();
    let mut saw_heating = samples
        .iter()
        .take(usize::from(start))
        .any(|sample| sample.duty_percent > 0);
    for (offset, sample) in samples
        .iter()
        .take(usize::from(sample_count))
        .enumerate()
        .skip(usize::from(start))
        .take(flux_purr_firmware::control_plane::THERMAL_PLANT_TRACE_PAGE_MAX)
    {
        let phase = if sample.duty_percent > 0 {
            saw_heating = true;
            ThermalPlantRunPhaseWire::Heating
        } else if saw_heating {
            ThermalPlantRunPhaseWire::Cooling
        } else {
            ThermalPlantRunPhaseWire::Ambient
        };
        let temperature_centi_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)
            .map(|value| round_to_i16(value * 100.0))
            .unwrap_or(current_temp_centi_c);
        let _ = points.push(ThermalPlantTracePointWire {
            sample_index: offset as u8,
            elapsed_ms: u32::from(sample.elapsed_ticks)
                .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
            temperature_centi_c,
            heater_voltage_mv: u16::from(sample.heater_voltage_100mv).saturating_mul(100),
            duty_percent: sample.duty_percent.min(100),
            phase,
        });
    }
    let next_sample = if start.saturating_add(points.len() as u8) < sample_count {
        Some(start.saturating_add(points.len() as u8))
    } else {
        None
    };

    let trace_page = ThermalPlantTracePageWire {
        start_sample: start,
        next_sample,
        total_samples: sample_count,
        points,
    };
    let provisional_curve = job.and_then(|job| {
        heater_curve_from_transient_bins(&job.heater_curve.bins).map(|curve| {
            let covered = job
                .heater_curve
                .bins
                .iter()
                .filter(|bin| bin.samples > 0)
                .count() as u8;
            ThermalPlantProvisionalCurveWire {
                state: {
                    let mut state = heapless::String::new();
                    let _ = state.push_str("preview");
                    state
                },
                coverage_percent: covered.saturating_mul(25),
                curve: HeaterCurvePackageWire::from_memory(&curve, None),
            }
        })
    });
    let active_result = persisted.and_then(|transaction| {
        thermal_plant_projection_for_runtime(memory_config).map(|(projection, _)| {
            ThermalPlantActiveResultWire {
                transaction_id: transaction.transaction_id,
                curve: HeaterCurvePackageWire::from_memory(
                    &memory_config.active_heater_curve,
                    Some(&memory_config.heater_curve_raw_observations),
                ),
                convection_mw_per_c: Some(projection.convection_mw_per_c),
                radiation_mw_per_k4: Some(projection.radiation_mw_per_k4),
                thermal_capacity_mj_per_c: Some(projection.thermal_capacity_mj_per_c),
                transport_delay_ms: Some(projection.transport_delay_ms),
            }
        })
    });
    let attempt = job.map(|job| ThermalPlantRunAttemptWire {
        run_id,
        status: calibration.job.status.to_wire(),
        phase: Some(match job.phase {
            ThermalPlantAutoPhase::Ambient => ThermalPlantRunPhaseWire::Ambient,
            ThermalPlantAutoPhase::Heating => ThermalPlantRunPhaseWire::Heating,
            ThermalPlantAutoPhase::Cooling => ThermalPlantRunPhaseWire::Cooling,
        }),
        progress_percent: calibration.job.progress_percent,
        elapsed_ms: job
            .elapsed_ticks
            .saturating_mul(HEATER_CONTROL_INTERVAL_MS as u32),
        current_temp_centi_c,
        heater_voltage_mv: current_voltage_mv,
        duty_percent: duty_percent.min(100),
        sample_count: job.sample_count,
        restart_allowed: calibration.job.status != CalibrationJobStatus::Running
            && !calibration.immediate_heater_disarm_pending
            && !calibration.thermal_plant_completion_disarm_pending,
        error: calibration.job.message.map(manual_pps_error_code),
    });

    ThermalPlantRunSnapshotWire {
        version: 1,
        attempt,
        trace_page,
        provisional_curve,
        active_result,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
fn thermal_plant_projection_for_runtime(
    memory_config: &MemoryConfig,
) -> Option<(flux_purr_firmware::memory::ThermalPlantProjection, f32)> {
    let transaction = memory_config.thermal_plant_transient_active?;
    if !thermal_plant_curve_is_bound(memory_config, transaction)
        || !thermal_plant_transient_trace_reaches_targets(&transaction, memory_config)
    {
        return None;
    }
    let projection = thermal_plant_projection_from_transient(&transaction)?;
    let ambient_temp_c =
        projected_rtd_temperature_c(memory_config, transaction.ambient_raw_rtd_adc_mv)?;
    Some((projection, ambient_temp_c))
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

#[cfg(test)]
fn oversampled_fractional_mean_mv_with_discard<F>(
    total_samples: usize,
    discard_valid_prefix_samples: usize,
    read_sample: F,
) -> Option<f32>
where
    F: FnMut() -> Option<u16>,
{
    oversampled_rtd_batch_with_discard(total_samples, discard_valid_prefix_samples, read_sample)
        .map(|batch| batch.mean_mv)
}

#[cfg(test)]
fn oversampled_rtd_batch_with_discard<F>(
    total_samples: usize,
    discard_valid_prefix_samples: usize,
    mut read_sample: F,
) -> Option<RtdAdcBatch>
where
    F: FnMut() -> Option<u16>,
{
    let mut sum_mv: u32 = 0;
    let mut min_mv = u16::MAX;
    let mut max_mv = 0_u16;
    let mut valid_samples = 0_usize;
    let mut discarded_valid_samples = 0_usize;

    for _ in 0..total_samples {
        let Some(sample_mv) = read_sample() else {
            continue;
        };
        if discarded_valid_samples < discard_valid_prefix_samples {
            discarded_valid_samples = discarded_valid_samples.saturating_add(1);
            continue;
        }
        sum_mv = sum_mv.saturating_add(sample_mv as u32);
        min_mv = min_mv.min(sample_mv);
        max_mv = max_mv.max(sample_mv);
        valid_samples = valid_samples.saturating_add(1);
    }

    rtd_fractional_mean_mv(sum_mv, valid_samples).map(|mean_mv| RtdAdcBatch {
        mean_mv,
        min_mv,
        max_mv,
        mean_raw_code: mean_mv.round() as u16,
        min_raw_code: min_mv,
        max_raw_code: max_mv,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn phase_averaged_rtd_batch_with_discard<F, W>(
    retained_samples: usize,
    phase_count: usize,
    discard_valid_prefix_samples: usize,
    mut read_sample: F,
    mut wait_for_next_phase: W,
) -> Option<RtdAdcBatch>
where
    F: FnMut() -> Option<AdcConvertedSample>,
    W: FnMut(),
{
    if retained_samples == 0 || phase_count == 0 || !retained_samples.is_multiple_of(phase_count) {
        return None;
    }

    let samples_per_phase = retained_samples / phase_count;
    let mut sum_mv: u32 = 0;
    let mut sum_raw_code: u32 = 0;
    let mut min_mv = u16::MAX;
    let mut max_mv = 0_u16;
    let mut min_raw_code = u16::MAX;
    let mut max_raw_code = 0_u16;
    let mut valid_samples = 0_usize;
    let mut discarded_valid_samples = 0_usize;

    for _ in 0..retained_samples.saturating_add(discard_valid_prefix_samples) {
        let Some(sample) = read_sample() else {
            continue;
        };
        if discarded_valid_samples < discard_valid_prefix_samples {
            discarded_valid_samples = discarded_valid_samples.saturating_add(1);
            continue;
        }

        sum_mv = sum_mv.saturating_add(sample.calibrated_mv as u32);
        sum_raw_code = sum_raw_code.saturating_add(sample.raw_code as u32);
        min_mv = min_mv.min(sample.calibrated_mv);
        max_mv = max_mv.max(sample.calibrated_mv);
        min_raw_code = min_raw_code.min(sample.raw_code);
        max_raw_code = max_raw_code.max(sample.raw_code);
        valid_samples = valid_samples.saturating_add(1);

        if valid_samples.is_multiple_of(samples_per_phase) && valid_samples < retained_samples {
            wait_for_next_phase();
        }
    }

    rtd_fractional_mean_mv(sum_mv, valid_samples).map(|mean_mv| RtdAdcBatch {
        mean_mv,
        min_mv,
        max_mv,
        mean_raw_code: (sum_raw_code / valid_samples as u32) as u16,
        min_raw_code,
        max_raw_code,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
const fn mask_adc1_raw_code(value: u16) -> u16 {
    value & 0x0fff
}

#[cfg(target_arch = "xtensa")]
type Adc1Driver = Adc<'static, esp_hal::peripherals::ADC1<'static>, esp_hal::Blocking>;
#[cfg(target_arch = "xtensa")]
type VinAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO1<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
type RtdAdcPin = esp_hal::analog::adc::AdcPin<
    esp_hal::peripherals::GPIO2<'static>,
    esp_hal::peripherals::ADC1<'static>,
    AdcCalBasic<esp_hal::peripherals::ADC1<'static>>,
>;
#[cfg(target_arch = "xtensa")]
type Adc1Curve = AdcCalCurve<esp_hal::peripherals::ADC1<'static>>;

#[cfg(target_arch = "xtensa")]
fn initialize_adc1(
    adc: esp_hal::peripherals::ADC1<'static>,
    vin_gpio: esp_hal::peripherals::GPIO1<'static>,
    rtd_gpio: esp_hal::peripherals::GPIO2<'static>,
) -> (Adc1Driver, VinAdcPin, RtdAdcPin, Option<Adc1Curve>) {
    let efuse_version = Efuse::rtc_calib_version();
    let init_code = Efuse::rtc_calib_init_code(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION);
    let reference_code = Efuse::rtc_calib_cal_code(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION);
    let reference_mv = (efuse_version == 1)
        .then(|| Efuse::rtc_calib_cal_mv(AdcCalibUnit::ADC1, RTD_SAMPLE_ATTENUATION));
    let efuse_ready = efuse_version == 1
        && init_code.is_some()
        && reference_code.is_some()
        && reference_mv.is_some();
    #[cfg(feature = "web_serial")]
    {
        ADC_CALIBRATION_SOURCE.store(if efuse_ready { 0 } else { 1 }, Ordering::Relaxed);
        ADC_EFUSE_VERSION.store(efuse_version, Ordering::Relaxed);
        ADC_INIT_CODE.store(init_code.unwrap_or(u16::MAX), Ordering::Relaxed);
        ADC_REFERENCE_CODE.store(reference_code.unwrap_or(u16::MAX), Ordering::Relaxed);
        ADC_REFERENCE_MV.store(reference_mv.unwrap_or(u16::MAX), Ordering::Relaxed);
    }

    let mut config = AdcConfig::new();
    let vin_pin = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(vin_gpio, RTD_SAMPLE_ATTENUATION);
    let rtd_pin = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(rtd_gpio, RTD_SAMPLE_ATTENUATION);
    let adc = Adc::new(adc, config);
    let curve = efuse_ready.then(|| Adc1Curve::new_cal(RTD_SAMPLE_ATTENUATION));
    (adc, vin_pin, rtd_pin, curve)
}

#[cfg(target_arch = "xtensa")]
fn read_rtd_adc_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO2<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'a>>,
    >,
    curve: &AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
) -> Option<RtdAdcBatch> {
    let delay = Delay::new();
    delay.delay_micros(RTD_CHANNEL_SWITCH_SETTLE_US);
    let batch = phase_averaged_rtd_batch_with_discard(
        RTD_SAMPLE_COUNT,
        RTD_SAMPLE_PWM_PHASE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || {
            let raw_code = loop {
                match adc.read_oneshot(pin) {
                    Ok(value) => break value,
                    Err(nb::Error::WouldBlock) => continue,
                    Err(_) => return None,
                }
            };
            let raw_code = mask_adc1_raw_code(raw_code);
            Some(AdcConvertedSample {
                raw_code,
                calibrated_mv: curve.adc_val(raw_code),
            })
        },
        || delay.delay_micros(RTD_SAMPLE_PWM_PHASE_SPACING_US),
    )?;
    Some(batch)
}

#[cfg(target_arch = "xtensa")]
fn read_vin_adc_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO1<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'a>>,
    >,
    curve: &AdcCalCurve<esp_hal::peripherals::ADC1<'a>>,
) -> Option<RtdAdcBatch> {
    let delay = Delay::new();
    delay.delay_micros(RTD_CHANNEL_SWITCH_SETTLE_US);
    phase_averaged_rtd_batch_with_discard(
        RTD_SAMPLE_COUNT,
        RTD_SAMPLE_PWM_PHASE_COUNT,
        RTD_SETTLE_DISCARD_SAMPLE_COUNT,
        || {
            let raw_code = loop {
                match adc.read_oneshot(pin) {
                    Ok(value) => break value,
                    Err(nb::Error::WouldBlock) => continue,
                    Err(_) => return None,
                }
            };
            let raw_code = mask_adc1_raw_code(raw_code);
            Some(AdcConvertedSample {
                raw_code,
                calibrated_mv: curve.adc_val(raw_code),
            })
        },
        || delay.delay_micros(RTD_SAMPLE_PWM_PHASE_SPACING_US),
    )
}

#[cfg(target_arch = "xtensa")]
fn read_calibrated_vin_mv<'a>(
    adc: &mut Adc<'a, esp_hal::peripherals::ADC1<'a>, esp_hal::Blocking>,
    pin: &mut esp_hal::analog::adc::AdcPin<
        esp_hal::peripherals::GPIO1<'a>,
        esp_hal::peripherals::ADC1<'a>,
        AdcCalBasic<esp_hal::peripherals::ADC1<'a>>,
    >,
    curve: Option<&AdcCalCurve<esp_hal::peripherals::ADC1<'a>>>,
    memory_config: &MemoryConfig,
) -> Option<(u16, u16, u16, u32)> {
    let curve = curve?;
    let batch = read_vin_adc_mv(adc, pin, curve)?;
    let raw_code = batch.mean_raw_code;
    let raw_adc_mv = batch.mean_mv.round() as u16;
    #[cfg(feature = "web_serial")]
    VIN_RAW_CODE_MEAN.store(raw_code, Ordering::Relaxed);
    let corrected_adc_mv = correct_adc_mv(
        &memory_config.adc_calibration,
        AdcCalibrationChannel::Vin,
        raw_adc_mv,
    );
    Some((
        raw_code,
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
        AdcCalBasic<esp_hal::peripherals::ADC1<'a>>,
    >,
    curve: Option<&AdcCalCurve<esp_hal::peripherals::ADC1<'a>>>,
    memory_config: &MemoryConfig,
) -> RtdSample {
    let Some(curve) = curve else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };
    let Some(batch) = read_rtd_adc_mv(adc, pin, curve) else {
        return RtdSample::Fault {
            adc_mv: None,
            reason: HeaterFaultReason::AdcReadFailed,
        };
    };
    #[cfg(feature = "web_serial")]
    {
        RTD_RAW_CODE_MEAN.store(batch.mean_raw_code, Ordering::Relaxed);
        RTD_RAW_CODE_MIN.store(batch.min_raw_code, Ordering::Relaxed);
        RTD_RAW_CODE_MAX.store(batch.max_raw_code, Ordering::Relaxed);
    }
    let raw_adc_mv = batch.mean_mv.round() as u16;
    let raw_adc_fractional_mv = batch.mean_mv;

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
                raw_adc_min_mv: batch.min_mv,
                raw_adc_max_mv: batch.max_mv,
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
const FUSB302B_STATUS0_VBUS_OK: u8 = 1 << 7;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS0_CRC_CHECK: u8 = 1 << 4;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS0A_RETRY_FAIL: u8 = 1 << 4;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1_RX_EMPTY: u8 = 1 << 5;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1_OVERTEMP: u8 = 1 << 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1_VCONN_OCP: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_STATUS1A_RXSOP: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_TOGSS_MASK: u8 = 0b0011_1000;
#[cfg(target_arch = "xtensa")]
const FUSB302B_TOGSS_SNK_CC1: u8 = 0b0010_1000;
#[cfg(target_arch = "xtensa")]
const FUSB302B_TOGSS_SNK_CC2: u8 = 0b0011_0000;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTA_TX_SENT: u8 = 1 << 2;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTA_SOFT_RESET: u8 = 1 << 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTA_HARD_RESET: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_INTERRUPTB_GCRC_SENT: u8 = 1;
#[cfg(target_arch = "xtensa")]
const FUSB302B_TOGGLE_INTERRUPT_MASKS: InterruptMasks = InterruptMasks::new(0x7f, 0xbf, 0xff);
#[cfg(target_arch = "xtensa")]
const FUSB302B_RECEIVE_INTERRUPT_MASKS: InterruptMasks = InterruptMasks::new(0x7d, 0xe0, 0x00);

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fusb302bReceiveEvent {
    Empty { tx_sent: bool, gcrc_sent: bool },
    Partial { tx_sent: bool, gcrc_sent: bool },
    Message(PdPacket),
    Reset,
    RetryFailed,
    Protection,
    UnsupportedSop,
}

#[cfg(target_arch = "xtensa")]
const fn fusb302b_phy_config(auto_goodcrc: bool) -> PhyConfig {
    PhyConfig {
        pd_revision: PdRevision::Rev30,
        power_role: PowerRole::Sink,
        data_role: DataRole::Ufp,
        auto_goodcrc,
        retry_count: RetryCount::Three,
        auto_soft_reset: false,
        auto_hard_reset: false,
        receive_sop: fusb302::ReceiveSopMask::NONE,
    }
}

#[cfg(target_arch = "xtensa")]
struct Fusb302bRuntime {
    policy: fusb302b::SinkPolicy,
    polarity: Option<CcPin>,
    next_message_id: u8,
    attached_at_ms: Option<u64>,
    last_source_capabilities_request_at_ms: Option<u64>,
    source_capabilities_refresh_pending: bool,
    source_capabilities_refresh_requested_at_ms: Option<u64>,
    last_request_at_ms: Option<u64>,
    source_capabilities_tx_confirmed: bool,
    source_capabilities_gcrc_seen: bool,
    partial_rx_started_at_ms: Option<u64>,
    source_caps_hard_reset_sent: bool,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PdContractRequestState {
    Confirmed,
    Pending,
    Failed,
}

#[cfg(target_arch = "xtensa")]
impl Fusb302bRuntime {
    const fn new() -> Self {
        Self {
            policy: fusb302b::SinkPolicy::new(
                FUSB302B_INITIAL_PPS_REQUEST_MV,
                MAX_HEATER_CONTRACT_MA,
            ),
            polarity: None,
            next_message_id: 0,
            attached_at_ms: None,
            last_source_capabilities_request_at_ms: None,
            source_capabilities_refresh_pending: false,
            source_capabilities_refresh_requested_at_ms: None,
            last_request_at_ms: None,
            source_capabilities_tx_confirmed: false,
            source_capabilities_gcrc_seen: false,
            partial_rx_started_at_ms: None,
            source_caps_hard_reset_sent: false,
        }
    }

    async fn initialize(&mut self, i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
        let mut phy = Fusb302::new(BlockingAsync::new(i2c));
        let initialized = phy.init().await.is_ok()
            && phy.pd_reset().await.is_ok()
            && phy.set_host_current_default().await.is_ok()
            && phy.configure_phy(fusb302b_phy_config(false)).await.is_ok()
            && phy.set_cc_pull(CcPin::Cc1, CcPull::Down).await.is_ok()
            && phy.set_cc_pull(CcPin::Cc2, CcPull::Down).await.is_ok()
            && phy.set_measure_cc(None).await.is_ok()
            && phy
                .set_interrupt_masks(FUSB302B_TOGGLE_INTERRUPT_MASKS)
                .await
                .is_ok()
            && phy.read_interrupts().await.is_ok()
            && phy.start_toggle(ToggleMode::Sink).await.is_ok();
        FUSB302B_DIAGNOSTIC.store(
            if initialized {
                FUSB302B_DIAG_WAITING_CC_ATTACH
            } else {
                FUSB302B_DIAG_FAULT
            },
            Ordering::Relaxed,
        );
        initialized
    }

    async fn restart_after_reset(&mut self, i2c: &mut I2c<'_, esp_hal::Blocking>) -> bool {
        self.policy.on_detach_or_reset();
        self.polarity = None;
        self.next_message_id = 0;
        self.attached_at_ms = None;
        self.last_source_capabilities_request_at_ms = None;
        self.source_capabilities_refresh_pending = false;
        self.source_capabilities_refresh_requested_at_ms = None;
        self.last_request_at_ms = None;
        self.source_capabilities_tx_confirmed = false;
        self.source_capabilities_gcrc_seen = false;
        self.partial_rx_started_at_ms = None;
        self.source_caps_hard_reset_sent = false;
        if self.initialize(i2c).await {
            self.policy =
                fusb302b::SinkPolicy::new(FUSB302B_INITIAL_PPS_REQUEST_MV, MAX_HEATER_CONTRACT_MA);
            true
        } else {
            self.policy.mark_fault();
            false
        }
    }

    fn active_contract(&self) -> Contract {
        self.policy.active_contract()
    }

    fn source_capabilities(&self) -> Option<SourceCapabilities> {
        self.policy.source_capabilities()
    }

    async fn request_pps_voltage(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        requested_mv: u16,
        now_ms: u64,
    ) -> PdContractRequestState {
        let active = self.policy.active_contract();
        if active.kind == ContractKind::Pps && active.voltage_mv == requested_mv {
            return PdContractRequestState::Confirmed;
        }
        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            return PdContractRequestState::Pending;
        }
        if active.kind == ContractKind::Fixed {
            if self.source_capabilities_refresh_pending {
                return PdContractRequestState::Pending;
            }
            if !self.policy.prepare_pps_request(requested_mv) {
                return PdContractRequestState::Failed;
            }
            let header = fusb302b::get_source_capabilities_header(self.next_message_id);
            if !self.transmit(i2c, header, &[]).await {
                return PdContractRequestState::Failed;
            }
            self.source_capabilities_refresh_pending = true;
            self.source_capabilities_refresh_requested_at_ms = Some(now_ms);
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
            return PdContractRequestState::Pending;
        }
        let Some(rdo) = self.policy.request_pps_voltage(requested_mv) else {
            return PdContractRequestState::Failed;
        };
        let header = fusb302b::request_header(self.next_message_id);
        if !self.transmit(i2c, header, &rdo).await {
            return PdContractRequestState::Failed;
        }
        self.last_request_at_ms = Some(now_ms);
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
        PdContractRequestState::Pending
    }

    async fn request_fixed_voltage(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        requested_mv: u16,
        now_ms: u64,
    ) -> bool {
        let active = self.policy.active_contract();
        if active.kind == ContractKind::Fixed && active.voltage_mv == requested_mv {
            return true;
        }
        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            return true;
        }
        let Some(rdo) = self.policy.request_fixed_voltage(requested_mv) else {
            return false;
        };
        let header = fusb302b::request_header(self.next_message_id);
        if !self.transmit(i2c, header, &rdo).await {
            return false;
        }
        self.last_request_at_ms = Some(now_ms);
        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
        true
    }

    async fn transmit(
        &mut self,
        i2c: &mut I2c<'_, esp_hal::Blocking>,
        header: u16,
        data: &[u8],
    ) -> bool {
        let Ok(packet) = PdPacket::new(SopType::Sop, header, data) else {
            self.policy.mark_fault();
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return false;
        };
        let mut phy = Fusb302::new(BlockingAsync::new(i2c));
        if phy.transmit(&packet).await.is_err() {
            self.policy.mark_fault();
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
            return false;
        }
        self.next_message_id = (self.next_message_id + 1) & 0x07;
        true
    }

    /// Drain a bounded number of completed PD frames in one service turn. No
    /// call awaits while I2C is borrowed, so EEPROM traffic remains independent
    /// of the controller's PD timing.
    async fn poll(&mut self, i2c: &mut I2c<'_, esp_hal::Blocking>, now_ms: u64) -> bool {
        if self.policy.phase() == SinkPhase::Fault {
            return false;
        }

        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy | SinkPhase::Ready
        ) {
            let vbus_present = {
                let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
                phy.read_status()
                    .await
                    .map(|status| status.status0 & FUSB302B_STATUS0_VBUS_OK != 0)
            };
            match vbus_present {
                Ok(true) => {}
                Ok(false) => return self.restart_after_reset(i2c).await,
                Err(_) => {
                    self.policy.mark_fault();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_I2C_ERROR, Ordering::Relaxed);
                    return false;
                }
            }
        }

        if matches!(
            self.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) && self
            .last_request_at_ms
            .is_some_and(|last| now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS)
        {
            // Reset the PHY and discard every contract before retrying. This
            // flushes delayed Accept/PS_RDY frames so an expired transaction
            // can never install a newer pending contract.
            self.policy.timeout_pending_request();
            self.last_request_at_ms = None;
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
            return self.restart_after_reset(i2c).await;
        }

        if self.source_capabilities_refresh_pending
            && self
                .source_capabilities_refresh_requested_at_ms
                .is_some_and(|last| {
                    now_ms.saturating_sub(last) >= FUSB302B_CONTRACT_REQUEST_TIMEOUT_MS
                })
        {
            // Preserve the active fixed contract and retry the Source Caps
            // refresh on a later control turn.
            self.source_capabilities_refresh_pending = false;
            self.source_capabilities_refresh_requested_at_ms = None;
            FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_REQUEST_TIMEOUT, Ordering::Relaxed);
        }

        if self.polarity.is_none() {
            let polarity = {
                let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
                match phy.read_status().await {
                    Ok(status) => match status.status1a & FUSB302B_TOGSS_MASK {
                        FUSB302B_TOGSS_SNK_CC1 => Some(CcPin::Cc1),
                        FUSB302B_TOGSS_SNK_CC2 => Some(CcPin::Cc2),
                        _ => None,
                    },
                    Err(_) => {
                        self.policy.mark_fault();
                        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_FAULT, Ordering::Relaxed);
                        return false;
                    }
                }
            };
            if let Some(polarity) = polarity {
                let selected = {
                    let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
                    phy.flush_fifos().await.is_ok()
                        && phy.stop_toggle().await.is_ok()
                        && phy.set_cc_pull(CcPin::Cc1, CcPull::Down).await.is_ok()
                        && phy.set_cc_pull(CcPin::Cc2, CcPull::Down).await.is_ok()
                        && phy.set_measure_cc(Some(polarity)).await.is_ok()
                        && phy.set_tx_cc(polarity).await.is_ok()
                        && phy.configure_phy(fusb302b_phy_config(true)).await.is_ok()
                        && phy
                            .set_interrupt_masks(FUSB302B_RECEIVE_INTERRUPT_MASKS)
                            .await
                            .is_ok()
                };
                if !selected {
                    self.policy.mark_fault();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_FAULT, Ordering::Relaxed);
                    return false;
                }
                self.polarity = Some(polarity);
                self.attached_at_ms = Some(now_ms);
                self.partial_rx_started_at_ms = None;
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_SOURCE_CAPS, Ordering::Relaxed);
            } else {
                FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_WAITING_CC_ATTACH, Ordering::Relaxed);
                return true;
            }
        }

        for _ in 0..FUSB302B_MAX_RX_MESSAGES_PER_POLL {
            let event = match fusb302b_receive_event(i2c).await {
                Ok(event) => event,
                Err(()) => {
                    self.policy.mark_fault();
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_FAULT, Ordering::Relaxed);
                    return false;
                }
            };
            match event {
                Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent } => {
                    self.partial_rx_started_at_ms = None;
                    if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
                        self.source_capabilities_tx_confirmed |= tx_sent;
                        self.source_capabilities_gcrc_seen |= gcrc_sent;
                        let query_due = match self.last_source_capabilities_request_at_ms {
                            Some(last) => fusb302b::source_capabilities_retry_due(last, now_ms),
                            None => self.attached_at_ms.is_some_and(|attached_at_ms| {
                                now_ms.saturating_sub(attached_at_ms)
                                    >= fusb302b::SOURCE_CAPS_INITIAL_WAIT_MS
                            }),
                        };
                        let hard_reset_due = !self.source_caps_hard_reset_sent
                            && self
                                .last_source_capabilities_request_at_ms
                                .is_some_and(|last| {
                                    fusb302b::source_capabilities_hard_reset_due(last, now_ms)
                                });
                        if hard_reset_due {
                            let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
                            if phy.transmit_hard_reset().await.is_err() {
                                self.policy.mark_fault();
                                FUSB302B_DIAGNOSTIC
                                    .store(FUSB302B_DIAG_TX_I2C_ERROR, Ordering::Relaxed);
                                return false;
                            }
                            self.source_caps_hard_reset_sent = true;
                            FUSB302B_DIAGNOSTIC.store(
                                FUSB302B_DIAG_SOURCE_CAPS_HARD_RESET_SENT,
                                Ordering::Relaxed,
                            );
                            return true;
                        }
                        if !query_due {
                            let diagnostic = if self.source_capabilities_gcrc_seen {
                                FUSB302B_DIAG_SOURCE_CAPS_GCRC_SEEN
                            } else if self.source_capabilities_tx_confirmed {
                                FUSB302B_DIAG_SOURCE_CAPS_TX_CONFIRMED
                            } else if self.last_source_capabilities_request_at_ms.is_some() {
                                FUSB302B_DIAG_SOURCE_CAPS_REQUESTED
                            } else {
                                FUSB302B_DIAG_WAITING_SOURCE_CAPS
                            };
                            FUSB302B_DIAGNOSTIC.store(diagnostic, Ordering::Relaxed);
                            return true;
                        }
                        let header = fusb302b::get_source_capabilities_header(self.next_message_id);
                        if !self.transmit(i2c, header, &[]).await {
                            return false;
                        }
                        self.source_capabilities_tx_confirmed = false;
                        self.source_capabilities_gcrc_seen = false;
                        self.last_source_capabilities_request_at_ms = Some(now_ms);
                        FUSB302B_DIAGNOSTIC
                            .store(FUSB302B_DIAG_SOURCE_CAPS_REQUESTED, Ordering::Relaxed);
                    } else if self.policy.phase() == SinkPhase::Ready
                        && self.active_contract().kind == ContractKind::Pps
                        && self
                            .last_request_at_ms
                            .is_some_and(|last| fusb302b::pps_keepalive_due(last, now_ms))
                    {
                        let Some(rdo) = self.policy.refresh_active_pps() else {
                            self.policy.mark_fault();
                            return false;
                        };
                        let header = fusb302b::request_header(self.next_message_id);
                        if !self.transmit(i2c, header, &rdo).await {
                            return false;
                        }
                        self.last_request_at_ms = Some(now_ms);
                    }
                    return true;
                }
                Fusb302bReceiveEvent::Partial { tx_sent, gcrc_sent } => {
                    if self.policy.phase() == SinkPhase::WaitingForSourceCapabilities {
                        self.source_capabilities_tx_confirmed |= tx_sent;
                        self.source_capabilities_gcrc_seen |= gcrc_sent;
                    }
                    let partial_started_at_ms = self.partial_rx_started_at_ms.get_or_insert(now_ms);
                    if now_ms.saturating_sub(*partial_started_at_ms)
                        >= FUSB302B_PARTIAL_RX_TIMEOUT_MS
                    {
                        FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                        return self.restart_after_reset(i2c).await;
                    }
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RX_PARTIAL, Ordering::Relaxed);
                    return true;
                }
                Fusb302bReceiveEvent::Message(message) => {
                    self.partial_rx_started_at_ms = None;
                    if let Some((pdos, count)) = fusb302b::source_capabilities_from_message(
                        message.header(),
                        message.payload(),
                    ) {
                        self.last_source_capabilities_request_at_ms = None;
                        self.source_capabilities_refresh_pending = false;
                        self.source_capabilities_refresh_requested_at_ms = None;
                        self.source_capabilities_tx_confirmed = false;
                        self.source_capabilities_gcrc_seen = false;
                        if let Some(rdo) = self.policy.on_source_capabilities(&pdos[..count]) {
                            let header = fusb302b::request_header(self.next_message_id);
                            if !self.transmit(i2c, header, &rdo).await {
                                return false;
                            }
                            self.last_request_at_ms = Some(now_ms);
                            FUSB302B_DIAGNOSTIC
                                .store(FUSB302B_DIAG_WAITING_ACCEPT, Ordering::Relaxed);
                        } else {
                            self.policy.mark_fault();
                            FUSB302B_DIAGNOSTIC
                                .store(FUSB302B_DIAG_NO_USABLE_CONTRACT, Ordering::Relaxed);
                            return false;
                        }
                    } else if message.payload().is_empty() {
                        self.policy
                            .on_control_message((message.header() & 0x1f) as u8, now_ms);
                        FUSB302B_DIAGNOSTIC.store(
                            if self.policy.phase() == SinkPhase::Ready {
                                FUSB302B_DIAG_IDLE
                            } else {
                                FUSB302B_DIAG_WAITING_PS_RDY
                            },
                            Ordering::Relaxed,
                        );
                    }
                }
                Fusb302bReceiveEvent::Reset => {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                    return self.restart_after_reset(i2c).await;
                }
                Fusb302bReceiveEvent::RetryFailed => {
                    FUSB302B_DIAGNOSTIC.store(FUSB302B_DIAG_RECOVERING, Ordering::Relaxed);
                    return self.restart_after_reset(i2c).await;
                }
                Fusb302bReceiveEvent::Protection | Fusb302bReceiveEvent::UnsupportedSop => {
                    self.policy.mark_fault();
                    FUSB302B_DIAGNOSTIC.store(
                        match event {
                            Fusb302bReceiveEvent::Protection => FUSB302B_DIAG_PROTECTION,
                            Fusb302bReceiveEvent::UnsupportedSop => FUSB302B_DIAG_UNSUPPORTED_SOP,
                            _ => unreachable!(),
                        },
                        Ordering::Relaxed,
                    );
                    return false;
                }
            }
        }

        self.policy.phase() != SinkPhase::Fault
    }
}

#[cfg(target_arch = "xtensa")]
async fn fusb302b_receive_event(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
) -> Result<Fusb302bReceiveEvent, ()> {
    let mut phy = Fusb302::new(BlockingAsync::new(i2c));
    // Preserve the receiver's CRC/SOP state before clearing its interrupt latches.
    let status = phy.read_status().await.map_err(|_| ())?;
    let interrupts = phy.read_interrupts().await.map_err(|_| ())?;
    let tx_sent = interrupts.interrupt_a & FUSB302B_INTERRUPTA_TX_SENT != 0;
    let gcrc_sent = interrupts.interrupt_b & FUSB302B_INTERRUPTB_GCRC_SENT != 0;

    if interrupts.interrupt_a & (FUSB302B_INTERRUPTA_SOFT_RESET | FUSB302B_INTERRUPTA_HARD_RESET)
        != 0
    {
        return Ok(Fusb302bReceiveEvent::Reset);
    }
    if status.status0a & FUSB302B_STATUS0A_RETRY_FAIL != 0
        && status.status1 & FUSB302B_STATUS1_RX_EMPTY != 0
    {
        return Ok(Fusb302bReceiveEvent::RetryFailed);
    }
    if status.status1 & (FUSB302B_STATUS1_OVERTEMP | FUSB302B_STATUS1_VCONN_OCP) != 0 {
        return Ok(Fusb302bReceiveEvent::Protection);
    }
    if status.status1 & FUSB302B_STATUS1_RX_EMPTY != 0 {
        return Ok(Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent });
    }
    if status.status0 & FUSB302B_STATUS0_CRC_CHECK == 0
        || status.status1a & FUSB302B_STATUS1A_RXSOP == 0
    {
        return Ok(Fusb302bReceiveEvent::Partial { tx_sent, gcrc_sent });
    }

    match phy.receive().await.map_err(|_| ())? {
        None => Ok(Fusb302bReceiveEvent::Empty { tx_sent, gcrc_sent }),
        Some(packet) if packet.sop() == SopType::Sop => Ok(Fusb302bReceiveEvent::Message(packet)),
        Some(_) => Ok(Fusb302bReceiveEvent::UnsupportedSop),
    }
}

#[cfg(target_arch = "xtensa")]
fn fusb302b_adjustable_power_capabilities(
    source_capabilities: SourceCapabilities,
) -> Option<ch224q::AdjustablePowerCapabilities> {
    let apdo = source_capabilities.fusb302b_pps_capability()?;
    let pps_min_mv = apdo.min_mv.max(FUSB302B_PPS_MIN_MV);
    let pps_max_mv = apdo.max_mv.min(FUSB302B_PPS_MAX_MV);
    let pps_max_ma = apdo.max_ma.min(MAX_HEATER_CONTRACT_MA);
    if pps_min_mv > GUARANTEED_HEATER_MIN_MV
        || pps_max_mv < GUARANTEED_HEATER_MIN_MV
        || pps_max_ma < MIN_HEATER_CONTRACT_MA
    {
        return None;
    }

    let mut capabilities = ch224q::AdjustablePowerCapabilities {
        pps_covers_20v: true,
        pps_min_mv: Some(pps_min_mv),
        pps_max_mv: Some(pps_max_mv),
        pps_max_ma: Some(pps_max_ma),
        ..ch224q::AdjustablePowerCapabilities::default()
    };
    capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
        min_mv: pps_min_mv,
        max_mv: pps_max_mv,
        max_ma: pps_max_ma,
    });
    Some(capabilities)
}

#[cfg(target_arch = "xtensa")]
enum PdPort {
    Fusb302b(Fusb302bRuntime),
    Unavailable,
}

#[cfg(target_arch = "xtensa")]
impl PdPort {
    const fn controller_kind(&self) -> ControllerKind {
        match self {
            Self::Fusb302b(_) => ControllerKind::Fusb302b,
            Self::Unavailable => ControllerKind::Unknown,
        }
    }
}

#[cfg(target_arch = "xtensa")]
enum DetectedPdController {
    Fusb302b(u8),
    Unknown,
}

#[cfg(any(target_arch = "xtensa", test))]
fn fusb302b_identity_is_stable(
    first_id: Option<u8>,
    second_id: Option<u8>,
    status0: Option<u8>,
    status1: Option<u8>,
) -> bool {
    matches!((first_id, second_id, status0, status1), (Some(first), Some(second), Some(status0), Some(status1))
        if first == second
            && first & 0xf0 == 0x90
            && status0 != u8::MAX
            && status1 != u8::MAX)
}

#[cfg(target_arch = "xtensa")]
async fn detect_pd_controller(i2c: &mut I2c<'_, esp_hal::Blocking>) -> DetectedPdController {
    let first = {
        let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
        phy.device_id().await.ok()
    };
    let second = {
        let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
        phy.device_id().await.ok()
    };
    let (Some(first), Some(second)) = (first, second) else {
        return DetectedPdController::Unknown;
    };
    if first != second || !first.is_fusb302b_family() {
        return DetectedPdController::Unknown;
    }
    let status = {
        let mut phy = Fusb302::new(BlockingAsync::new(&mut *i2c));
        phy.read_status().await.ok()
    };
    let (status0, status1) = status
        .map(|status| (Some(status.status0), Some(status.status1)))
        .unwrap_or((None, None));
    if fusb302b_identity_is_stable(Some(first.bits()), Some(second.bits()), status0, status1) {
        DetectedPdController::Fusb302b(first.bits())
    } else {
        DetectedPdController::Unknown
    }
}

#[cfg(target_arch = "xtensa")]
fn memory_commit_error_from_eeprom<I2cError>(error: EepromError<I2cError>) -> MemoryCommitError
where
    I2cError: embedded_hal::i2c::Error,
{
    match error {
        EepromError::OutOfRange
        | EepromError::PageWriteTooLong
        | EepromError::PageBoundaryCrossed => MemoryCommitError::WriteFailed,
        EepromError::I2c(error) => match error.kind() {
            embedded_hal::i2c::ErrorKind::NoAcknowledge(source) => match source {
                embedded_hal::i2c::NoAcknowledgeSource::Address => {
                    MemoryCommitError::WriteAddressNoAck
                }
                embedded_hal::i2c::NoAcknowledgeSource::Data => MemoryCommitError::WriteDataNoAck,
                _ => MemoryCommitError::WriteUnknownNoAck,
            },
            embedded_hal::i2c::ErrorKind::Bus => MemoryCommitError::WriteBus,
            embedded_hal::i2c::ErrorKind::ArbitrationLoss => MemoryCommitError::WriteArbitration,
            _ => MemoryCommitError::WriteOther,
        },
    }
}

#[cfg(target_arch = "xtensa")]
fn probe_eeprom_address(i2c: &mut I2c<'_, esp_hal::Blocking>) -> Option<u8> {
    let mut eeprom = M24c64::with_address(i2c, M24C64_I2C_ADDRESS);
    let mut byte = [0u8; 1];
    eeprom
        .read_bytes(0, &mut byte)
        .ok()
        .map(|()| M24C64_I2C_ADDRESS)
}

#[cfg(target_arch = "xtensa")]
fn read_eeprom_bytes_chunked<I2C>(
    eeprom: &mut M24c64<I2C>,
    offset: u16,
    bytes: &mut [u8],
) -> Result<(), EepromError<I2C::Error>>
where
    I2C: embedded_hal::i2c::I2c,
{
    let mut read = 0usize;
    while read < bytes.len() {
        let chunk_len = (bytes.len() - read).min(EEPROM_READ_CHUNK_MAX_BYTES);
        let chunk_offset = offset
            .checked_add(read as u16)
            .ok_or(EepromError::OutOfRange)?;
        eeprom.read_bytes(chunk_offset, &mut bytes[read..read + chunk_len])?;
        read += chunk_len;
    }
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
fn eeprom_bytes_contain_data(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte != 0xff)
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn write_eeprom_bytes_verified(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    offset: u16,
    bytes: &[u8],
) -> Result<(), MemoryCommitError> {
    let Some(address) = probe_eeprom_address(i2c) else {
        return Err(MemoryCommitError::WriteAddressNoAck);
    };
    let mut eeprom = M24c64::with_address(&mut *i2c, address);
    let mut written = 0usize;
    while written < bytes.len() {
        let absolute_offset = usize::from(offset) + written;
        let chunk_len = eeprom_maintenance_write_chunk_len(absolute_offset, bytes.len() - written);
        let chunk_offset =
            u16::try_from(absolute_offset).map_err(|_| MemoryCommitError::WriteFailed)?;
        eeprom
            .write_page(chunk_offset, &bytes[written..written + chunk_len])
            .map_err(memory_commit_error_from_eeprom)?;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
        written += chunk_len;
    }
    let mut verify = [0u8; flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX];
    read_eeprom_bytes_chunked(&mut eeprom, offset, &mut verify[..bytes.len()])
        .map_err(|_| MemoryCommitError::VerifyUnreadable)?;
    if verify[..bytes.len()] != *bytes {
        return Err(MemoryCommitError::VerifyMismatch);
    }
    Ok(())
}

#[cfg(any(target_arch = "xtensa", test))]
fn eeprom_maintenance_write_chunk_len(absolute_offset: usize, remaining: usize) -> usize {
    let page_size = flux_purr_firmware::memory::M24C64_PAGE_SIZE;
    let page_room = page_size - (absolute_offset % page_size);
    remaining.min(page_room).min(EEPROM_WRITE_CHUNK_MAX_BYTES)
}

#[cfg(any(target_arch = "xtensa", test))]
fn eeprom_data_is_incompatible(has_valid_record: bool, contains_data: bool) -> bool {
    !has_valid_record && contains_data
}

#[cfg(any(target_arch = "xtensa", test))]
const fn raw_eeprom_operation_mutates(op: EepromMaintenanceOp) -> bool {
    matches!(op, EepromMaintenanceOp::Write | EepromMaintenanceOp::Erase)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn begin_mutating_eeprom_maintenance(
    ui_state: &mut FrontPanelUiState,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    memory_commit_due_ms: &mut Option<u64>,
) {
    // The EEPROM may already be partially changed when the first I2C failure
    // is reported. Lock power and suppress normal record writes before sending
    // the first raw byte.
    ui_state.heater_enabled = false;
    ui_state.heater_output_percent = 0;
    ui_state.eeprom_data_incompatible = true;
    calibration_job_canceled(calibration, manual_pps);
    calibration.mode = CalibrationMode::Off;
    calibration.pps_enabled = false;
    calibration.pps_mv = None;
    calibration.pps_ma = None;
    calibration.heater_enabled = false;
    calibration.job_data = None;
    calibration.model_target_temp_c = None;
    calibration.thermal_plant_completion_disarm_pending = false;
    calibration.immediate_heater_disarm_pending = true;
    manual_pps.clear();
    *memory_commit_due_ms = None;
}

#[cfg(any(target_arch = "xtensa", test))]
fn mark_eeprom_required(
    ui_state: &mut FrontPanelUiState,
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
    memory_commit_due_ms: &mut Option<u64>,
) {
    begin_mutating_eeprom_maintenance(ui_state, calibration, manual_pps, memory_commit_due_ms);
    ui_state.eeprom_required = true;
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn eeprom_storage_failure_response(response: &UsbFrame) -> bool {
    let UsbFrame::Response {
        ok: false,
        error: Some(error),
        ..
    } = response
    else {
        return false;
    };
    matches!(
        error.code.as_str(),
        "eeprom_unavailable"
            | "eeprom_read_failed"
            | "memory_commit_write_failed"
            | "memory_commit_write_address_nack"
            | "memory_commit_write_data_nack"
            | "memory_commit_write_unknown_nack"
            | "memory_commit_write_bus_error"
            | "memory_commit_write_arbitration_lost"
            | "memory_commit_write_other_error"
            | "memory_commit_verify_unreadable"
            | "memory_commit_verify_mismatch"
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn apply_successful_eeprom_maintenance_operation(
    op: EepromMaintenanceOp,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    memory_commit_due_ms: &mut Option<u64>,
) {
    if matches!(op, EepromMaintenanceOp::Erase) {
        *memory_config = MemoryConfig::default();
        *memory_commit_due_ms = None;
        apply_memory_config_to_ui(ui_state, memory_config);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn discard_deferred_memory_commit_for_incompatible_eeprom(
    eeprom_data_incompatible: bool,
    memory_commit_due_ms: &mut Option<u64>,
) {
    if eeprom_data_incompatible {
        *memory_commit_due_ms = None;
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
async fn usb_eeprom_maintenance_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    command: EepromMaintenanceCommand,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
) -> UsbFrame {
    match command.op {
        EepromMaintenanceOp::Read => {
            let (Some(offset), Some(length)) = (command.offset, command.length) else {
                return usb_error_response(
                    request_id,
                    "eeprom_range_required",
                    "EEPROM read requires offset and length.",
                );
            };
            let length = usize::from(length);
            if length == 0
                || length > flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX
                || usize::from(offset) + length > usize::from(M24C64_CAPACITY_BYTES)
            {
                return usb_error_response(
                    request_id,
                    "eeprom_range_invalid",
                    "EEPROM read range is invalid.",
                );
            }
            let Some(address) = probe_eeprom_address(i2c) else {
                return usb_error_response(
                    request_id,
                    "eeprom_unavailable",
                    "EEPROM is unavailable.",
                );
            };
            let mut bytes = heapless::Vec::new();
            let _ = bytes.resize_default(length);
            let mut eeprom = M24c64::with_address(i2c, address);
            if read_eeprom_bytes_chunked(&mut eeprom, offset, bytes.as_mut_slice()).is_err() {
                return usb_error_response(request_id, "eeprom_read_failed", "EEPROM read failed.");
            }
            usb_response(request_id, UsbResponsePayload::EepromBytes(bytes))
        }
        EepromMaintenanceOp::Write => {
            let (Some(offset), Some(bytes)) = (command.offset, command.bytes) else {
                return usb_error_response(
                    request_id,
                    "eeprom_write_required",
                    "EEPROM write requires offset and bytes.",
                );
            };
            if bytes.is_empty()
                || usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES)
            {
                return usb_error_response(
                    request_id,
                    "eeprom_range_invalid",
                    "EEPROM write range is invalid.",
                );
            }
            match write_eeprom_bytes_verified(i2c, offset, bytes.as_slice()).await {
                Ok(()) => usb_response(request_id, UsbResponsePayload::Ack),
                Err(error) => usb_error_response(request_id, error.code(), error.message()),
            }
        }
        EepromMaintenanceOp::Erase => {
            let erased = [0xff; flux_purr_firmware::control_plane::EEPROM_MAINTENANCE_CHUNK_MAX];
            let mut offset = 0u16;
            while offset < M24C64_CAPACITY_BYTES {
                if let Err(error) = write_eeprom_bytes_verified(i2c, offset, &erased).await {
                    return usb_error_response(request_id, error.code(), error.message());
                }
                offset = offset.saturating_add(erased.len() as u16);
            }
            usb_response(request_id, UsbResponsePayload::Ack)
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[inline(never)]
fn memory_record_length_from_header(header: &[u8], slot_size: usize) -> Option<usize> {
    if header.len() < MEMORY_RECORD_HEADER_LEN
        || header[0..4] != *b"FPM1"
        || header[4] != MEMORY_RECORD_FORMAT_VERSION
        || usize::from(header[5]) != MEMORY_RECORD_HEADER_LEN
    {
        return None;
    }
    let payload_len = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let record_len = MEMORY_RECORD_HEADER_LEN.checked_add(payload_len)?;
    (record_len <= slot_size).then_some(record_len)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
fn load_eeprom_memory_record(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    scratch: &mut MemoryIoScratch,
) -> (Option<MemoryRecord>, bool, bool) {
    let Some(address) = probe_eeprom_address(i2c) else {
        info!("memory restore skipped: eeprom unavailable");
        return (None, false, true);
    };

    let mut eeprom = M24c64::with_address(i2c, address);
    let mut contains_data = false;
    let mut read_failed = false;
    let mut selected: Option<MemoryRecord> = None;
    let current_format_valid;
    // Only the current v5 slots are read on the critical path. Older layouts
    // are scanned after the first Dashboard frame so EEPROM migration cannot
    // delay RTD sampling or the owner-facing startup state.
    for offset in [MEMORY_SLOT_A_OFFSET, MEMORY_SLOT_B_OFFSET] {
        let header = &mut scratch.record_bytes[..MEMORY_RECORD_HEADER_LEN];
        let candidate = match read_eeprom_bytes_chunked(&mut eeprom, offset, header) {
            Ok(()) => {
                contains_data |= eeprom_bytes_contain_data(header);
                if let Some(record_len) = memory_record_length_from_header(header, MEMORY_SLOT_SIZE)
                {
                    let bytes = &mut scratch.record_bytes[..record_len];
                    match read_eeprom_bytes_chunked(
                        &mut eeprom,
                        offset.saturating_add(MEMORY_RECORD_HEADER_LEN as u16),
                        &mut bytes[MEMORY_RECORD_HEADER_LEN..],
                    ) {
                        Ok(()) => decode_memory_record(bytes).ok(),
                        Err(_) => {
                            read_failed = true;
                            None
                        }
                    }
                } else {
                    None
                }
            }
            Err(_) => {
                read_failed = true;
                None
            }
        };
        selected = match (selected, candidate) {
            (Some(current), Some(candidate)) if candidate.sequence > current.sequence => {
                Some(candidate)
            }
            (Some(current), _) => Some(current),
            (None, candidate) => candidate,
        };
    }

    current_format_valid = selected.is_some();
    // Probe one byte in each archived slot to distinguish a blank EEPROM from
    // legacy data without reading the full old records during boot.
    for offset in [
        PREVIOUS_MEMORY_SLOT_A_OFFSET,
        PREVIOUS_MEMORY_SLOT_B_OFFSET,
        LEGACY_MEMORY_SLOT_A_OFFSET,
        LEGACY_MEMORY_SLOT_B_OFFSET,
    ] {
        let probe = &mut scratch.record_bytes[..1];
        match read_eeprom_bytes_chunked(&mut eeprom, offset, probe) {
            Ok(()) => contains_data |= eeprom_bytes_contain_data(probe),
            Err(_) => read_failed = true,
        }
    }

    let gap_probe = &mut scratch.record_bytes[..1];
    match read_eeprom_bytes_chunked(&mut eeprom, EEPROM_UNUSED_GAP_OFFSET, gap_probe) {
        Ok(()) => contains_data |= eeprom_bytes_contain_data(gap_probe),
        Err(_) => read_failed = true,
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

    let incompatible = eeprom_data_is_incompatible(current_format_valid, contains_data);
    let required = read_failed && selected.is_none();
    (selected, incompatible, required)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
async fn load_legacy_eeprom_memory_record(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    scratch: &mut MemoryIoScratch,
) -> (Option<MemoryRecord>, bool) {
    let Some(address) = probe_eeprom_address(i2c) else {
        return (None, true);
    };
    let mut eeprom = M24c64::with_address(i2c, address);
    let mut selected: Option<MemoryRecord> = None;
    let mut read_failed = false;
    for (offset, length) in [
        (PREVIOUS_MEMORY_SLOT_A_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE),
        (PREVIOUS_MEMORY_SLOT_B_OFFSET, PREVIOUS_MEMORY_SLOT_SIZE),
        (LEGACY_MEMORY_SLOT_A_OFFSET, LEGACY_MEMORY_SLOT_SIZE),
        (LEGACY_MEMORY_SLOT_B_OFFSET, LEGACY_MEMORY_SLOT_SIZE),
    ] {
        let bytes = &mut scratch.record_bytes[..length];
        let candidate = match read_eeprom_bytes_chunked(&mut eeprom, offset, bytes) {
            Ok(()) => decode_memory_record(bytes).ok(),
            Err(_) => {
                read_failed = true;
                None
            }
        };
        selected = match (selected, candidate) {
            (Some(current), Some(candidate)) if candidate.sequence > current.sequence => {
                Some(candidate)
            }
            (Some(current), _) => Some(current),
            (None, candidate) => candidate,
        };
        EmbassyTimer::after_millis(0).await;
    }
    (selected, read_failed)
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryCommitError {
    EncodeFailed,
    WriteFailed,
    WriteAddressNoAck,
    WriteDataNoAck,
    WriteUnknownNoAck,
    WriteBus,
    WriteArbitration,
    WriteOther,
    VerifyUnreadable,
    VerifyMismatch,
}

#[cfg(target_arch = "xtensa")]
impl MemoryCommitError {
    const fn code(self) -> &'static str {
        match self {
            Self::EncodeFailed => "memory_commit_encode_failed",
            Self::WriteFailed => "memory_commit_write_failed",
            Self::WriteAddressNoAck => "memory_commit_write_address_nack",
            Self::WriteDataNoAck => "memory_commit_write_data_nack",
            Self::WriteUnknownNoAck => "memory_commit_write_unknown_nack",
            Self::WriteBus => "memory_commit_write_bus_error",
            Self::WriteArbitration => "memory_commit_write_arbitration_lost",
            Self::WriteOther => "memory_commit_write_other_error",
            Self::VerifyUnreadable => "memory_commit_verify_unreadable",
            Self::VerifyMismatch => "memory_commit_verify_mismatch",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::EncodeFailed => "Memory record could not be encoded.",
            Self::WriteFailed => "Memory record could not be written to EEPROM.",
            Self::WriteAddressNoAck => "EEPROM did not acknowledge its I2C address.",
            Self::WriteDataNoAck => "EEPROM rejected the I2C write payload.",
            Self::WriteUnknownNoAck => "EEPROM write failed with an I2C NACK.",
            Self::WriteBus => "EEPROM write failed with an I2C bus error.",
            Self::WriteArbitration => "EEPROM write lost I2C bus arbitration.",
            Self::WriteOther => "EEPROM write failed with an uncategorized I2C error.",
            Self::VerifyUnreadable => "Memory record could not be read back after write.",
            Self::VerifyMismatch => "Memory record readback did not match the requested config.",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn memory_record_write_chunk_len(absolute_offset: usize, remaining: usize) -> usize {
    eeprom_maintenance_write_chunk_len(absolute_offset, remaining)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
async fn write_eeprom_memory_record(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    elapsed_ms: u64,
    commit_started_at: Instant,
    record: &MemoryRecord,
    scratch: &mut MemoryIoScratch,
) -> Result<(), MemoryCommitError> {
    scratch.record_bytes.fill(0xff);
    let Ok(record_len) = encode_memory_record(record, &mut scratch.record_bytes) else {
        info!("memory commit encode failed");
        return Err(MemoryCommitError::EncodeFailed);
    };
    let base_offset = memory_slot_offset_for_sequence(record.sequence);
    service_pd_during_memory_commit(i2c, pd_port, elapsed_ms, commit_started_at).await;
    let Some(address) = probe_eeprom_address(i2c) else {
        return Err(MemoryCommitError::WriteAddressNoAck);
    };
    let mut written = 0usize;
    while written < record_len {
        let absolute_offset = usize::from(base_offset) + written;
        let chunk_len = memory_record_write_chunk_len(absolute_offset, record_len - written);
        let Ok(page_offset) = u16::try_from(absolute_offset) else {
            info!("memory commit offset overflow");
            return Err(MemoryCommitError::WriteFailed);
        };
        let write_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom.write_page(
                page_offset,
                &scratch.record_bytes[written..written + chunk_len],
            )
        };
        if let Err(error) = write_result {
            let error = memory_commit_error_from_eeprom(error);
            info!("memory commit write failed seq={=u32}", record.sequence);
            return Err(error);
        }
        written += chunk_len;
        EmbassyTimer::after_millis(EEPROM_WRITE_CYCLE_DELAY_MS).await;
        // FUSB302B shares this bus. Dropping the EEPROM adapter before every
        // PD poll keeps its receive and contract deadlines serviced throughout
        // a long record write.
        service_pd_during_memory_commit(i2c, pd_port, elapsed_ms, commit_started_at).await;
    }
    info!(
        "memory commit ok seq={=u32} bytes={=u16} slot=0x{=u16:04x}",
        record.sequence, record_len as u16, base_offset,
    );
    Ok(())
}

#[cfg(target_arch = "xtensa")]
async fn service_pd_during_memory_commit(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    elapsed_ms: u64,
    commit_started_at: Instant,
) {
    let now_ms = elapsed_ms.saturating_add(
        Instant::now()
            .as_millis()
            .saturating_sub(commit_started_at.as_millis()),
    );
    let _ = read_pd_status(i2c, pd_port, now_ms).await;
}

#[cfg(target_arch = "xtensa")]
async fn verify_eeprom_memory_record(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    elapsed_ms: u64,
    commit_started_at: Instant,
    record: &MemoryRecord,
    scratch: &mut MemoryIoScratch,
) -> Result<MemoryRecord, MemoryCommitError> {
    service_pd_during_memory_commit(i2c, pd_port, elapsed_ms, commit_started_at).await;
    let Some(address) = probe_eeprom_address(i2c) else {
        return Err(MemoryCommitError::VerifyUnreadable);
    };
    let base_offset = memory_slot_offset_for_sequence(record.sequence);
    let header_read = {
        let mut eeprom = M24c64::with_address(&mut *i2c, address);
        eeprom.read_bytes(
            base_offset,
            &mut scratch.record_bytes[..MEMORY_RECORD_HEADER_LEN],
        )
    };
    if header_read.is_err() {
        return Err(MemoryCommitError::VerifyUnreadable);
    }
    service_pd_during_memory_commit(i2c, pd_port, elapsed_ms, commit_started_at).await;

    let payload_len = usize::from(u16::from_le_bytes([
        scratch.record_bytes[6],
        scratch.record_bytes[7],
    ]));
    let Some(record_len) = MEMORY_RECORD_HEADER_LEN.checked_add(payload_len) else {
        return Err(MemoryCommitError::VerifyUnreadable);
    };
    if record_len > MEMORY_SLOT_SIZE {
        return Err(MemoryCommitError::VerifyUnreadable);
    }

    let mut read = MEMORY_RECORD_HEADER_LEN;
    while read < record_len {
        let chunk_len = (record_len - read).min(EEPROM_WRITE_CHUNK_MAX_BYTES);
        let chunk_offset = base_offset
            .checked_add(read as u16)
            .ok_or(MemoryCommitError::VerifyUnreadable)?;
        let read_result = {
            let mut eeprom = M24c64::with_address(&mut *i2c, address);
            eeprom.read_bytes(
                chunk_offset,
                &mut scratch.record_bytes[read..read + chunk_len],
            )
        };
        if read_result.is_err() {
            return Err(MemoryCommitError::VerifyUnreadable);
        }
        read += chunk_len;
        service_pd_during_memory_commit(i2c, pd_port, elapsed_ms, commit_started_at).await;
    }

    decode_memory_record(&scratch.record_bytes[..record_len])
        .map_err(|_| MemoryCommitError::VerifyUnreadable)
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryCommitBackend {
    Eeprom,
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
async fn write_memory_record(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    elapsed_ms: u64,
    commit_started_at: Instant,
    record: &MemoryRecord,
    scratch: &mut MemoryIoScratch,
) -> Result<MemoryCommitBackend, MemoryCommitError> {
    write_eeprom_memory_record(i2c, pd_port, elapsed_ms, commit_started_at, record, scratch)
        .await
        .map(|()| MemoryCommitBackend::Eeprom)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
async fn commit_memory_config_now(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    elapsed_ms: u64,
    memory_sequence: &mut u32,
    memory_config: &MemoryConfig,
) -> Result<(), MemoryCommitError> {
    let Some(mut scratch) = try_allocate_memory_io_scratch() else {
        return Err(MemoryCommitError::EncodeFailed);
    };
    let mut expected_config = memory_config.clone();
    expected_config.sanitize();
    let mut last_error = MemoryCommitError::WriteFailed;
    let commit_started_at = Instant::now();

    for attempt in 0..2 {
        let next_sequence = memory_sequence.saturating_add(1 + attempt);
        let record = MemoryRecord {
            sequence: next_sequence,
            config: expected_config.clone(),
        };
        let backend = match write_memory_record(
            i2c,
            pd_port,
            elapsed_ms,
            commit_started_at,
            &record,
            &mut scratch,
        )
        .await
        {
            Ok(backend) => backend,
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        let verified = match backend {
            MemoryCommitBackend::Eeprom => {
                verify_eeprom_memory_record(
                    i2c,
                    pd_port,
                    elapsed_ms,
                    commit_started_at,
                    &record,
                    &mut scratch,
                )
                .await
            }
        };
        let verified = match verified {
            Ok(verified) => verified,
            Err(error) => {
                info!(
                    "memory commit verify failed seq={=u32} reason={=str}",
                    next_sequence,
                    error.code(),
                );
                last_error = error;
                continue;
            }
        };
        if verified.sequence != next_sequence || verified.config != expected_config {
            info!(
                "memory commit verify failed seq={=u32} read_seq={=u32} config_match={=bool}",
                next_sequence,
                verified.sequence,
                verified.config == expected_config,
            );
            last_error = MemoryCommitError::VerifyMismatch;
            continue;
        }
        *memory_sequence = next_sequence;
        return Ok(());
    }

    Err(last_error)
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
        commissioning_required: previous.commissioning_required,
        target_temp_c: state.target_temp_c,
        selected_preset_slot: state.selected_preset_slot,
        presets_c: state.presets_c,
        active_cooling_enabled: state.active_cooling_enabled,
        wifi_ssid: previous.wifi_ssid.clone(),
        wifi_password: previous.wifi_password.clone(),
        wifi_auto_reconnect: previous.wifi_auto_reconnect,
        wifi_static_ipv4: previous.wifi_static_ipv4,
        telemetry_interval_ms: previous.telemetry_interval_ms,
        adc_calibration: previous.adc_calibration,
        active_heater_curve: previous.active_heater_curve,
        heater_curve_raw_observations: previous.heater_curve_raw_observations,
        heater_curve_transaction_id: previous.heater_curve_transaction_id,
        thermal_plant_active: previous.thermal_plant_active,
        thermal_plant_transient_active: previous.thermal_plant_transient_active,
        active_thermal_control_profile: previous.active_thermal_control_profile,
        thermal_control_profile_pps5a: previous.thermal_control_profile_pps5a,
        thermal_profile_mode: previous.thermal_profile_mode,
        lan_pairing_token: previous.lan_pairing_token,
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
fn projected_heater_curve(memory_config: &MemoryConfig) -> Option<HeaterCurveConfig> {
    let mut curve = HeaterCurveConfig::default();
    curve.points[0] = Some(default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C));
    curve.points[1] = Some(default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C));
    let mut count = 2;
    let mut observed_count = 0;
    let mut last_resistance_milliohms = curve.points[1]
        .map(|point| point.resistance_milliohms)
        .unwrap_or_default();
    for observation in memory_config
        .heater_curve_raw_observations
        .points
        .iter()
        .flatten()
    {
        let temp_c = projected_rtd_temperature_c(memory_config, observation.raw_rtd_adc_mv)?;
        if !(-50.0..=450.0).contains(&temp_c) {
            return None;
        }
        let resistance_milliohms = observation
            .resistance_milliohms
            .max(last_resistance_milliohms)
            .max(heater_curve_model_floor_milliohms(temp_c));
        last_resistance_milliohms = resistance_milliohms;
        curve.points[count] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: round_to_i16(temp_c * 100.0),
            resistance_milliohms,
        });
        count += 1;
        observed_count += 1;
    }
    (observed_count >= 2).then_some(curve)
}

#[cfg(any(target_arch = "xtensa", test))]
fn estimated_heater_resistance_ohms(
    current_temp_c: f32,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> f32 {
    let estimated = preview_heater_curve
        .and_then(|curve| heater_resistance_ohms_from_curve(curve, current_temp_c))
        .or_else(|| {
            projected_heater_curve(memory_config)
                .and_then(|curve| heater_resistance_ohms_from_curve(&curve, current_temp_c))
        })
        .or_else(|| {
            heater_resistance_ohms_from_curve(&memory_config.active_heater_curve, current_temp_c)
        })
        .unwrap_or_else(|| default_estimated_heater_resistance_ohms(current_temp_c));
    estimated.max(default_estimated_heater_resistance_ohms(current_temp_c))
}

#[cfg(any(target_arch = "xtensa", test))]
fn effective_pps_current_limit_ma(
    capability_max_ma: u16,
    _pd_observation: Option<PdStatusObservation>,
) -> u16 {
    // CURRENT_DATA_REGISTER is the instantaneous draw. It is useful telemetry,
    // but it is not the APDO contract limit: treating a partially loaded 5 A
    // source as, for example, a 2.5 A contract feeds the draw back into the
    // voltage ceiling and prevents a high-temperature plate from ever using
    // its negotiated PPS headroom. The resistance-derived ceiling below keeps
    // actual heater current within this contractual budget.
    capability_max_ma
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
fn production_pps_request_ceiling_mv(
    _current_temp_c: f32,
    _source_current_limit_ma: u16,
    _reserve_ma: u16,
    source_voltage_max_mv: u16,
    _preview_heater_curve: Option<&HeaterCurveConfig>,
    _memory_config: &MemoryConfig,
) -> u16 {
    source_voltage_max_mv
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_available_power_mw_for_temp(
    current_temp_c: f32,
    capability_max_mv: Option<u16>,
    capability_max_ma: Option<u16>,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u32 {
    let source_voltage_max_mv = capability_max_mv.unwrap_or(0).min(HEATER_ADJUSTABLE_MAX_MV);
    let available_current_ma = capability_max_ma.unwrap_or(0);
    if source_voltage_max_mv == 0 || available_current_ma == 0 {
        return 0;
    }

    let resistance_ohms =
        estimated_heater_resistance_ohms(current_temp_c, preview_heater_curve, memory_config);
    if !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return 0;
    }
    let resistance_limited_power_mw = f32::from(source_voltage_max_mv)
        * f32::from(source_voltage_max_mv)
        / resistance_ohms
        / 1_000.0;
    let contract_power_mw =
        f32::from(source_voltage_max_mv) * f32::from(available_current_ma) / 1_000.0;
    resistance_limited_power_mw
        .min(contract_power_mw)
        .max(0.0)
        .min(u32::MAX as f32) as u32
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_source_request_ceiling_mv(
    safe_heater_mv: u16,
    current_request_mv: u16,
    measured_heater_mv: u32,
    source_voltage_max_mv: u16,
) -> u16 {
    let measured_heater_mv = measured_heater_mv.min(u32::from(u16::MAX)) as u16;
    if measured_heater_mv == 0 || current_request_mv <= measured_heater_mv {
        return safe_heater_mv.min(source_voltage_max_mv);
    }
    let path_drop_mv = current_request_mv
        .saturating_sub(measured_heater_mv)
        .min(HEATER_PPS_PATH_DROP_COMPENSATION_MAX_MV);
    safe_heater_mv
        .saturating_add(path_drop_mv)
        .min(source_voltage_max_mv)
}

#[cfg(test)]
fn has_calibrated_heater_resistance_curve(memory_config: &MemoryConfig) -> bool {
    memory_config
        .active_heater_curve
        .points
        .iter()
        .flatten()
        .count()
        >= 2
        || projected_heater_curve(memory_config).is_some()
}

#[cfg(any(target_arch = "xtensa", test))]
fn has_persisted_heater_resistance_curve(memory_config: &MemoryConfig) -> bool {
    projected_heater_curve(memory_config).is_some()
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
fn fixed_pd_pwm_duty_percent(
    duty_percent: u8,
    current_temp_c: f32,
    fixed_mv: u16,
    negotiated_current_ma: u16,
    reserve_ma: u16,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> u8 {
    if duty_percent == 0 || fixed_mv == 0 {
        return 0;
    }

    let available_current_ma = heater_available_current_ma(negotiated_current_ma, reserve_ma);
    let safe_mv = heater_safe_max_mv_for_temp(
        current_temp_c,
        available_current_ma,
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
fn heater_physical_pwm_percent(
    duty_percent: u8,
    ceiling_mv: u16,
    active_request_mv: u16,
    warmup_soft_start_percent: u8,
) -> u8 {
    if duty_percent == 0 {
        return 0;
    }
    let requested_power = u64::from(duty_percent.min(100))
        .saturating_mul(u64::from(ceiling_mv).saturating_mul(u64::from(ceiling_mv)));
    let active_request_mv = active_request_mv.max(CH224Q_ADJUSTABLE_REQUEST_MIN_MV);
    let active_power = u64::from(active_request_mv).saturating_mul(u64::from(active_request_mv));
    let power_matched_percent = (requested_power / active_power.max(1)).min(100) as u8;
    let soft_started_percent = u16::from(power_matched_percent)
        .saturating_mul(u16::from(warmup_soft_start_percent.min(100)))
        / 100;
    soft_started_percent.min(100) as u8
}

#[cfg(any(target_arch = "xtensa", test))]
fn apply_warmup_soft_start(duty_percent: u8, warmup_soft_start_percent: u8) -> u8 {
    (u16::from(duty_percent.min(100)).saturating_mul(u16::from(warmup_soft_start_percent.min(100)))
        / 100) as u8
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

#[cfg(test)]
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
                .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
                .max(control_floor_mv)
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

    let capability_state = ManualPpsState::from_capabilities(Some(capabilities));
    let Some((pps_min_mv, pps_max_mv, capability_max_ma)) =
        capability_state.thermal_plant_source_limits()
    else {
        return HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::NoPps20vCapability,
            fixed_request_confirmed: true,
            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
        };
    };
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
        terminal_fixed_pd_disarmed: false,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn constrain_heater_backend_to_controller(
    controller: ControllerKind,
    backend: HeaterPowerBackend,
) -> HeaterPowerBackend {
    match (controller, backend) {
        (
            ControllerKind::Fusb302b,
            HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv,
                pps_max_mv,
                adjustable_max_mv,
                capability_max_ma,
                current_request_mv,
                settle_until_ms,
                next_request_at_ms,
                current_limit_fixed_pwm_active,
                current_limit_fixed_request_confirmed,
                terminal_fixed_pd_disarmed,
                ..
            },
        ) if pps_min_mv <= FUSB302B_PPS_MAX_MV => {
            let pps_max_mv = pps_max_mv.min(FUSB302B_PPS_MAX_MV);
            HeaterPowerBackend::PpsMos {
                pps_min_mv,
                idle_request_mv: idle_request_mv.clamp(pps_min_mv, pps_max_mv),
                pps_max_mv,
                adjustable_max_mv: adjustable_max_mv.min(pps_max_mv),
                capability_max_ma,
                current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
                current_request_mv: current_request_mv.clamp(pps_min_mv, pps_max_mv),
                settle_until_ms,
                next_request_at_ms,
                current_limit_fixed_pwm_active,
                current_limit_fixed_request_confirmed,
                terminal_fixed_pd_disarmed,
            }
        }
        (ControllerKind::Fusb302b, HeaterPowerBackend::PpsMos { .. }) => {
            HeaterPowerBackend::FixedPdPwmFallback {
                reason: HeaterPowerBackendReason::NoPps20vCapability,
                fixed_request_confirmed: false,
                fixed_request: ch224q::VoltageRequest::V20,
            }
        }
        (ControllerKind::Fusb302b, HeaterPowerBackend::FixedPdPwmFallback { reason, .. }) => {
            HeaterPowerBackend::FixedPdPwmFallback {
                reason,
                fixed_request_confirmed: false,
                fixed_request: ch224q::VoltageRequest::V20,
            }
        }
        (_, backend) => backend,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn fusb302b_pps_backend_from_capabilities(
    capabilities: ch224q::AdjustablePowerCapabilities,
) -> Option<HeaterPowerBackend> {
    let backend = constrain_heater_backend_to_controller(
        ControllerKind::Fusb302b,
        select_heater_power_backend(Some(capabilities), None),
    );
    matches!(backend, HeaterPowerBackend::PpsMos { .. }).then_some(backend)
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
async fn disarm_pending_thermal_plant_output<PWM>(
    calibration_runtime_state: &mut CalibrationRuntimeState,
    backend: &mut HeaterPowerBackend,
    manual_pps: &mut ManualPpsState,
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    heater_pwm: &mut PWM,
    hold_pps_governor: &mut HoldPpsGovernor,
    ui_state: &mut FrontPanelUiState,
    last_heater_duty: &mut u8,
    measured_vin_mv: u32,
) -> bool
where
    PWM: SetDutyCycle,
{
    if !latch_terminal_fixed_pd_disarm(calibration_runtime_state, backend) {
        return false;
    }

    apply_heater_duty(heater_pwm, 0, last_heater_duty);
    hold_pps_governor.reset();
    ui_state.heater_enabled = false;
    ui_state.heater_output_percent = 0;

    if !request_pd_fixed_voltage(i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST).await {
        // Keep both the disarm latch and the PPS backend lock so the next
        // control period retries fixed PD without re-applying a PPS request.
        return true;
    }

    if !terminal_fixed_pd_voltage_confirmed(measured_vin_mv) {
        // The CH224Q accepts the register write before the source has actually
        // left PPS. Keep the terminal lock active until VIN proves fixed PD.
        return true;
    }

    calibration_runtime_state.immediate_heater_disarm_pending = false;
    let _ = manual_pps.consume_automatic_restore_pending();
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn terminal_fixed_pd_voltage_confirmed(measured_vin_mv: u32) -> bool {
    measured_vin_mv.abs_diff(u32::from(DEFAULT_PD_VOLTAGE_REQUEST.millivolts())) <= 1_000
}

#[cfg(any(target_arch = "xtensa", test))]
fn latch_terminal_fixed_pd_disarm(
    calibration_runtime_state: &CalibrationRuntimeState,
    backend: &mut HeaterPowerBackend,
) -> bool {
    if !calibration_runtime_state.immediate_heater_disarm_pending {
        return false;
    }
    if let HeaterPowerBackend::PpsMos {
        terminal_fixed_pd_disarmed,
        ..
    } = backend
    {
        *terminal_fixed_pd_disarmed = true;
    }
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn release_terminal_fixed_pd_disarm_for_manual_pps(
    backend: &mut HeaterPowerBackend,
    manual_pps_active: bool,
) -> bool {
    let HeaterPowerBackend::PpsMos {
        terminal_fixed_pd_disarmed,
        current_mode,
        current_request_mv,
        settle_until_ms,
        next_request_at_ms,
        current_limit_fixed_pwm_active,
        current_limit_fixed_request_confirmed,
        idle_request_mv,
        ..
    } = backend
    else {
        return false;
    };
    if !manual_pps_active || !*terminal_fixed_pd_disarmed {
        return false;
    }

    // A new manual PPS request is an explicit, non-heating re-arm. It may
    // renegotiate the source while the heater output remains at zero.
    *terminal_fixed_pd_disarmed = false;
    *current_mode = None;
    *current_request_mv = *idle_request_mv;
    *settle_until_ms = None;
    *next_request_at_ms = 0;
    *current_limit_fixed_pwm_active = false;
    *current_limit_fixed_request_confirmed = false;
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn manual_pps_request_required(
    manual_pps: ManualPpsState,
    controller: ControllerKind,
    observation: Option<PdStatusObservation>,
) -> bool {
    let Some(target_mv) = manual_pps.target_mv else {
        return false;
    };
    if manual_pps.applied_mv != Some(target_mv) {
        return true;
    }
    if controller != ControllerKind::Fusb302b {
        return false;
    }
    let target_ma = manual_pps.target_ma.unwrap_or(0);
    !observation.is_some_and(|observation| {
        observation.contract.kind == ContractKind::Pps
            && observation.contract.voltage_mv == target_mv
            && observation.contract.current_ma >= target_ma
    })
}

#[cfg(target_arch = "xtensa")]
async fn apply_heater_power_output<PWM>(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_port: &mut PdPort,
    heater_pwm: &mut PWM,
    backend: &mut HeaterPowerBackend,
    hold_pps_governor: &mut HoldPpsGovernor,
    manual_pps: &mut ManualPpsState,
    pd_observation: Option<PdStatusObservation>,
    measured_heater_mv: u32,
    current_temp_c: f32,
    duty_percent: u8,
    heater_enabled: bool,
    control_phase: HeaterControlPhase,
    control_error_c: f32,
    filtered_slope_c_per_s: f32,
    warmup_soft_start_percent: u8,
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
    let _ = release_terminal_fixed_pd_disarm_for_manual_pps(backend, manual_pps_active);
    let terminal_fixed_pd_disarmed = match backend {
        HeaterPowerBackend::PpsMos {
            terminal_fixed_pd_disarmed,
            ..
        } => *terminal_fixed_pd_disarmed,
        HeaterPowerBackend::FixedPdPwmFallback { .. } => false,
    };
    if terminal_fixed_pd_disarmed {
        apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
        return false;
    }

    let mut manual_pps_request_changed = false;
    if manual_pps_active {
        hold_pps_governor.reset();
    }
    if manual_pps_active {
        let target_mv = match manual_pps.target_mv {
            Some(target_mv) => target_mv,
            None => {
                manual_pps.fail(ManualPpsError::InvalidVoltage);
                apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                return true;
            }
        };
        let target_ma = manual_pps.target_ma.unwrap_or(0);
        if !pd_observation.is_some_and(|observation| observation.status.pd_active) {
            manual_pps.fail(ManualPpsError::PdNotReady);
            apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
            let _ = request_pd_fixed_voltage(i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST).await;
            return true;
        }
        if manual_pps_request_required(*manual_pps, pd_port.controller_kind(), pd_observation) {
            match request_pd_adjustable_voltage(
                i2c,
                pd_port,
                target_mv,
                ch224q::AdjustableVoltageMode::Pps,
                true,
            )
            .await
            {
                PdContractRequestState::Confirmed => {
                    manual_pps.applied_mv = Some(target_mv);
                    manual_pps_request_changed = true;
                    info!(
                        "manual pps override applied mv={=u16} ma={=u16}",
                        target_mv, target_ma
                    );
                }
                PdContractRequestState::Pending => {
                    // An RDO on FUSB302B is not a completed contract. Keep the
                    // source transition and heater output separate until PS_RDY.
                    apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                    return false;
                }
                PdContractRequestState::Failed => {
                    manual_pps.fail(ManualPpsError::WriteFailed);
                    apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                    let _ =
                        request_pd_fixed_voltage(i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST).await;
                    info!(
                        "manual pps override cleared reason={=str}",
                        ManualPpsError::WriteFailed.code()
                    );
                    return true;
                }
            }
        }
    }

    if manual_pps.enabled && manual_pps.owner == ManualPpsOwner::Calibration {
        // Calibration owns a source voltage already limited by the same
        // Fixed-PD fallback cannot change source voltage, so PWM remains the
        // only way to stay within the negotiated current contract.
        // Do not route it through the generic profile governor or PWM-based
        // current fallback: the transient needs a measured, full-duty step.
        let previous_duty_percent = *last_physical_duty_percent;
        apply_heater_duty(
            heater_pwm,
            if heater_enabled { duty_percent } else { 0 },
            last_physical_duty_percent,
        );
        return manual_pps_request_changed || *last_physical_duty_percent != previous_duty_percent;
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
                    terminal_fixed_pd_disarmed: false,
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
                if request_pd_fixed_voltage(i2c, pd_port, fixed_request).await {
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
            let negotiated_current_ma = pd_observation
                .filter(|observation| observation.status.pd_active)
                .map(|observation| observation.current_ma)
                .unwrap_or(0);
            let safe_duty_percent = fixed_pd_pwm_duty_percent(
                duty_percent,
                current_temp_c,
                pd_observation
                    .and_then(|observation| observation.contract_voltage_mv)
                    .unwrap_or_else(|| fixed_request.millivolts()),
                negotiated_current_ma,
                active_thermal_settings.heater_current_reserve_ma,
                preview_heater_curve,
                memory_config,
            );
            apply_heater_duty(
                heater_pwm,
                apply_warmup_soft_start(safe_duty_percent, warmup_soft_start_percent),
                last_physical_duty_percent,
            );
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
            ..
        } => {
            let source_current_limit_ma =
                effective_pps_current_limit_ma(capability_max_ma, pd_observation);
            let effective_current_limit_ma = source_current_limit_ma;
            let persisted_safe_max_mv = production_pps_request_ceiling_mv(
                current_temp_c,
                effective_current_limit_ma,
                active_thermal_settings.heater_current_reserve_ma,
                adjustable_max_mv,
                preview_heater_curve,
                memory_config,
            );
            let safe_max_mv = persisted_safe_max_mv;
            let source_request_ceiling_mv = heater_source_request_ceiling_mv(
                safe_max_mv,
                current_request_mv,
                measured_heater_mv,
                adjustable_max_mv,
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
                hold_pps_governor.reset();
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
                            terminal_fixed_pd_disarmed: false,
                        };
                    } else {
                        if !request_pd_fixed_voltage(
                            i2c,
                            pd_port,
                            HEATER_CURRENT_LIMIT_FALLBACK_REQUEST,
                        )
                        .await
                        {
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
                            terminal_fixed_pd_disarmed: false,
                        };
                        return true;
                    }
                }
                let fallback_duty_percent = apply_warmup_soft_start(
                    current_limit_fixed_pwm_duty_percent(
                        duty_percent,
                        current_temp_c,
                        effective_current_limit_ma,
                        preview_heater_curve,
                        memory_config,
                    ),
                    warmup_soft_start_percent,
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
                    terminal_fixed_pd_disarmed: false,
                };
                if current_request_mv <= source_request_ceiling_mv {
                    let settled_gate_duty_percent = heater_physical_pwm_percent(
                        duty_percent,
                        source_request_ceiling_mv,
                        current_request_mv,
                        warmup_soft_start_percent,
                    );
                    apply_heater_duty(
                        heater_pwm,
                        settled_gate_duty_percent,
                        last_physical_duty_percent,
                    );
                    return true;
                }
            }

            let automatic_request_mv = heater_adjustable_request_mv(
                duty_percent,
                heater_enabled,
                current_request_mv,
                idle_request_mv,
                control_floor_mv,
                source_request_ceiling_mv,
            );
            let request_mv = if manual_pps_active {
                automatic_request_mv
            } else {
                hold_pps_governor
                    .request_mv(
                        control_phase,
                        duty_percent,
                        control_error_c,
                        filtered_slope_c_per_s,
                        current_request_mv,
                        control_floor_mv,
                        source_request_ceiling_mv,
                        now_ms,
                    )
                    .unwrap_or(automatic_request_mv)
            };
            let request_mode = adjustable_mode_for_request(request_mv, pps_max_mv);
            let mode_changed = !manual_pps_active && current_mode != Some(request_mode);
            let voltage_changed = !manual_pps_active && current_request_mv != request_mv;
            let request_transition_pending = !manual_pps_active && now_ms < next_request_at_ms;
            let gate_duty_percent = heater_physical_pwm_percent(
                duty_percent,
                source_request_ceiling_mv,
                current_request_mv,
                warmup_soft_start_percent,
            );

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
                match request_pd_adjustable_voltage(
                    i2c,
                    pd_port,
                    request_mv,
                    request_mode,
                    mode_changed,
                )
                .await
                {
                    PdContractRequestState::Confirmed => {}
                    PdContractRequestState::Pending => {
                        apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                        return false;
                    }
                    PdContractRequestState::Failed => {
                        apply_heater_duty(heater_pwm, 0, last_physical_duty_percent);
                        let fixed_request_confirmed =
                            request_pd_fixed_voltage(i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST)
                                .await;
                        *backend = HeaterPowerBackend::FixedPdPwmFallback {
                            reason: HeaterPowerBackendReason::AdjustableRequestFailed,
                            fixed_request_confirmed,
                            fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
                        };
                        if fixed_request_confirmed {
                            let negotiated_current_ma = pd_observation
                                .filter(|observation| observation.status.pd_active)
                                .map(|observation| observation.current_ma)
                                .unwrap_or(0);
                            let safe_duty_percent = fixed_pd_pwm_duty_percent(
                                duty_percent,
                                current_temp_c,
                                pd_observation
                                    .and_then(|observation| observation.contract_voltage_mv)
                                    .unwrap_or_else(|| DEFAULT_PD_VOLTAGE_REQUEST.millivolts()),
                                negotiated_current_ma,
                                active_thermal_settings.heater_current_reserve_ma,
                                preview_heater_curve,
                                memory_config,
                            );
                            apply_heater_duty(
                                heater_pwm,
                                apply_warmup_soft_start(
                                    safe_duty_percent,
                                    warmup_soft_start_percent,
                                ),
                                last_physical_duty_percent,
                            );
                        }
                        info!(
                            "heater backend fallback -> reason={=str} fixed_request_confirmed={=bool}",
                            HeaterPowerBackendReason::AdjustableRequestFailed.label(),
                            fixed_request_confirmed,
                        );
                        return true;
                    }
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
                    terminal_fixed_pd_disarmed: false,
                };
                return true;
            }

            let active_request_gate_duty_percent = if request_transition_pending {
                heater_physical_pwm_percent(
                    duty_percent,
                    source_request_ceiling_mv,
                    current_request_mv,
                    warmup_soft_start_percent,
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
                    "heater pps request temp_c={=f32} control={=u8}% current_limit_ma={=u16} safe_heater_mv={=u16} source_ceiling_mv={=u16} control_floor_mv={=u16} request_mv={=u16}",
                    current_temp_c,
                    duty_percent,
                    effective_current_limit_ma,
                    safe_max_mv,
                    source_request_ceiling_mv,
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
    configured_frequency_hz: &mut u32,
) -> bool
where
    PWM: SetDutyCycle,
{
    let next_state = BuzzerHardwareState {
        frequency_hz: output.frequency_hz,
        duty_percent: output.duty_percent.min(100),
        generation: output.generation,
    };
    if *last_state == next_state {
        return false;
    }

    for action in buzzer_hardware_actions(*configured_frequency_hz, next_state) {
        match action {
            // Duty updates on GPIO48 are immediate. Quiesce the output before
            // touching Timer2 so a retune cannot preserve a partial old cycle.
            BuzzerHardwareAction::SetDutyPercent(duty_percent) => {
                let _ = buzzer_pwm.set_duty_cycle_percent(duty_percent);
            }
            BuzzerHardwareAction::StopTimer => buzzer_timer.stop(),
            BuzzerHardwareAction::Retune(next_frequency_hz) => {
                let period_ticks = buzzer_timer_period_ticks(next_frequency_hz)
                    .expect("buzzer frequency is outside the Timer2 period range");
                let timer_cfg = peripheral_clock.timer_clock_with_prescaler(
                    period_ticks,
                    PwmWorkingMode::Increase,
                    BUZZER_TIMER_PRESCALER,
                );
                buzzer_timer.set_counter(0, CounterDirection::Increasing);
                buzzer_timer.start(timer_cfg);
                *configured_frequency_hz = next_frequency_hz;
            }
        }
    }
    info!(
        "buzzer output -> freq_hz={=u32} duty={=u8}% gen={=u32}",
        next_state.frequency_hz.unwrap_or(0),
        next_state.duty_percent,
        next_state.generation,
    );
    *last_state = next_state;
    true
}

#[cfg(target_arch = "xtensa")]
const BUZZER_COMMAND_CAPACITY: usize = 32;

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone)]
enum BuzzerRuntimeCommand {
    Feedback {
        source: BuzzerCueSource,
        cue: BuzzerCueId,
    },
    ProtectionReplay {
        source: BuzzerCueSource,
    },
    RequestAttentionReminder {
        source: BuzzerCueSource,
    },
    #[cfg(feature = "buzzer-test")]
    Test(BuzzerTestCommand),
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy)]
enum BuzzerSafetyCommand {
    ActivateProtection { source: BuzzerCueSource },
    EnterAttentionPendingAndRequestReminder { source: BuzzerCueSource },
    ClearAttention,
}

#[cfg(target_arch = "xtensa")]
static BUZZER_COMMANDS: Channel<
    CriticalSectionRawMutex,
    BuzzerRuntimeCommand,
    BUZZER_COMMAND_CAPACITY,
> = Channel::new();

#[cfg(target_arch = "xtensa")]
static BUZZER_SAFETY_COMMAND: Signal<CriticalSectionRawMutex, BuzzerSafetyCommand> = Signal::new();

// GPIO48 transitions have 25-80 ms deadlines. Keep their sole owner out of the
// thread-mode executor, where display or control work can otherwise defer a cue
// transition until the next cooperative poll.
#[cfg(target_arch = "xtensa")]
static BUZZER_REALTIME_EXECUTOR: StaticCell<InterruptExecutor<1>> = StaticCell::new();

/// Runtime callers submit cue requests. The dedicated task owns arbitration,
/// cue progression, and every GPIO48 PWM write.
#[cfg(target_arch = "xtensa")]
#[derive(Default)]
struct BuzzerRuntime;

#[cfg(target_arch = "xtensa")]
impl BuzzerRuntime {
    fn submit(command: BuzzerRuntimeCommand) {
        if BUZZER_COMMANDS.try_send(command).is_err() {
            // Feedback is best effort, but this capacity is intentionally far
            // larger than one front-panel/control-loop iteration can produce.
            warn!("buzzer command mailbox full");
        }
    }

    fn request_feedback(&mut self, source: BuzzerCueSource, cue: BuzzerCueId, _: u64) {
        Self::submit(BuzzerRuntimeCommand::Feedback { source, cue });
    }

    fn activate_protection(&mut self, source: BuzzerCueSource, _: u64) {
        BUZZER_SAFETY_COMMAND.signal(BuzzerSafetyCommand::ActivateProtection { source });
    }

    fn request_protection_replay(&mut self, source: BuzzerCueSource, _: u64) {
        Self::submit(BuzzerRuntimeCommand::ProtectionReplay { source });
    }

    fn enter_attention_pending_and_request_reminder(&mut self, source: BuzzerCueSource, _: u64) {
        BUZZER_SAFETY_COMMAND
            .signal(BuzzerSafetyCommand::EnterAttentionPendingAndRequestReminder { source });
    }

    fn clear_attention(&mut self) {
        BUZZER_SAFETY_COMMAND.signal(BuzzerSafetyCommand::ClearAttention);
    }

    fn request_attention_reminder(&mut self, source: BuzzerCueSource, _: u64) {
        Self::submit(BuzzerRuntimeCommand::RequestAttentionReminder { source });
    }

    #[cfg(feature = "buzzer-test")]
    fn submit_test(command: BuzzerTestCommand) {
        Self::submit(BuzzerRuntimeCommand::Test(command));
    }
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
type BuzzerTestStatusMutex = BlockingMutex<CriticalSectionRawMutex, RefCell<BuzzerTestStatus>>;

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
static BUZZER_TEST_STATUS: BuzzerTestStatusMutex =
    BlockingMutex::new(RefCell::new(BuzzerTestStatus {
        state: BuzzerTestSessionState::Idle,
        scenario: None,
        cue: None,
        repeat: false,
        active_cue: None,
        trace: heapless::Vec::new(),
        #[cfg(feature = "buzzer-observe")]
        output_trace: heapless::Vec::new(),
    }));

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
fn buzzer_test_status() -> BuzzerTestStatus {
    BUZZER_TEST_STATUS.lock(|status| status.borrow().clone())
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
#[cfg(feature = "buzzer-observe")]
fn publish_buzzer_test_status(
    session: &BuzzerTestSession,
    arbiter: &BuzzerArbiter,
    output_trace: &BuzzerTestOutputTrace,
) {
    let mut status = session.status(arbiter.active_cue());
    status.output_trace = output_trace.events.clone();
    BUZZER_TEST_STATUS.lock(|published| *published.borrow_mut() = status);
}

#[cfg(all(
    target_arch = "xtensa",
    feature = "buzzer-test",
    not(feature = "buzzer-observe")
))]
fn publish_buzzer_test_status(session: &BuzzerTestSession, arbiter: &BuzzerArbiter) {
    let status = session.status(arbiter.active_cue());
    BUZZER_TEST_STATUS.lock(|published| *published.borrow_mut() = status);
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
struct BuzzerTestOutputTrace {
    started_at_ms: u64,
    last_recorded_ms: u64,
    last_pad_rising_edges: u16,
    events: heapless::Vec<BuzzerTestOutputTraceEvent, BUZZER_TEST_OUTPUT_TRACE_CAPACITY>,
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
impl BuzzerTestOutputTrace {
    const fn new() -> Self {
        Self {
            started_at_ms: 0,
            last_recorded_ms: 0,
            last_pad_rising_edges: 0,
            events: heapless::Vec::new(),
        }
    }

    fn reset(&mut self, now_ms: u64, pad_rising_edges: u16) {
        self.started_at_ms = now_ms;
        self.last_recorded_ms = now_ms;
        self.last_pad_rising_edges = pad_rising_edges;
        self.events.clear();
    }

    fn record(&mut self, now_ms: u64, output: BuzzerOutput, pad_rising_edges: u16) {
        // The buzzer task exclusively owns timer2 writes. This direct PAC
        // access reads CFG0 only, after `apply_buzzer_output` has completed.
        let cfg0 = unsafe { (&*esp_hal::peripherals::MCPWM0::PTR).timer(2).cfg0().read() };
        let timer_prescaler = cfg0.prescale().bits();
        let timer_period_ticks = cfg0.period().bits();
        let observed_window_ms = now_ms
            .saturating_sub(self.last_recorded_ms)
            .min(u64::from(u32::MAX)) as u32;
        let observed_rising_edges = pad_rising_edges.wrapping_sub(self.last_pad_rising_edges);
        if let Some(previous) = self.events.last_mut() {
            previous.observed_window_ms = observed_window_ms;
            previous.observed_rising_edges = observed_rising_edges;
            previous.observed_frequency_hz = (previous.duty_percent > 0)
                .then(|| buzzer_observed_frequency_hz(observed_rising_edges, observed_window_ms))
                .flatten();
        }
        self.last_recorded_ms = now_ms;
        self.last_pad_rising_edges = pad_rising_edges;
        if self.events.len() == BUZZER_TEST_OUTPUT_TRACE_CAPACITY {
            let _ = self.events.remove(0);
        }
        let _ = self.events.push(BuzzerTestOutputTraceEvent {
            elapsed_ms: now_ms
                .saturating_sub(self.started_at_ms)
                .min(u64::from(u32::MAX)) as u32,
            requested_frequency_hz: output.frequency_hz,
            applied_frequency_hz: mcpwm_timer_frequency_hz(timer_prescaler, timer_period_ticks),
            observed_frequency_hz: None,
            observed_rising_edges: 0,
            observed_window_ms: 0,
            duty_percent: output.duty_percent.min(100),
            generation: output.generation,
            timer_prescaler,
            timer_period_ticks,
        });
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_buzzer_command(
    command: BuzzerRuntimeCommand,
    arbiter: &mut BuzzerArbiter,
    now_ms: u64,
    #[cfg(feature = "buzzer-test")] test_session: &mut BuzzerTestSession,
) {
    let decision = match command {
        BuzzerRuntimeCommand::Feedback { source, cue } => {
            Some(arbiter.request_feedback(source, cue, now_ms))
        }
        BuzzerRuntimeCommand::ProtectionReplay { source } => {
            Some(arbiter.request_protection_replay(source, now_ms))
        }
        BuzzerRuntimeCommand::RequestAttentionReminder { source } => {
            Some(arbiter.request_attention_reminder(source, now_ms))
        }
        #[cfg(feature = "buzzer-test")]
        BuzzerRuntimeCommand::Test(command) => {
            apply_buzzer_test_command(test_session, arbiter, command, now_ms);
            None
        }
    };
    if let Some(decision) = decision {
        log_buzzer_decision(decision);
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_buzzer_safety_command(
    command: BuzzerSafetyCommand,
    arbiter: &mut BuzzerArbiter,
    now_ms: u64,
    #[cfg(feature = "buzzer-test")] test_session: &mut BuzzerTestSession,
) {
    #[cfg(feature = "buzzer-test")]
    test_session.cancel_for_safety(arbiter, now_ms);
    let decision = match command {
        BuzzerSafetyCommand::ActivateProtection { source } => {
            Some(arbiter.activate_protection(source, now_ms))
        }
        BuzzerSafetyCommand::EnterAttentionPendingAndRequestReminder { source } => {
            if let Some(decision) = arbiter.enter_attention_pending() {
                log_buzzer_decision(decision);
            }
            Some(arbiter.request_attention_reminder(source, now_ms))
        }
        BuzzerSafetyCommand::ClearAttention => arbiter.clear_attention(),
    };
    if let Some(decision) = decision {
        log_buzzer_decision(decision);
    }
}

#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
fn apply_buzzer_test_command(
    test_session: &mut BuzzerTestSession,
    arbiter: &mut BuzzerArbiter,
    command: BuzzerTestCommand,
    now_ms: u64,
) {
    match command.op {
        BuzzerTestOp::Status => return,
        BuzzerTestOp::Trigger => {
            let cue = command.cue.expect("validated buzzer test trigger cue");
            if command.repeat
                || matches!(
                    cue,
                    BuzzerCueId::ProtectionAlarm | BuzzerCueId::AttentionReminder
                )
            {
                match test_session.start_playback(arbiter, cue, command.repeat, now_ms) {
                    Ok(decisions) => {
                        for decision in decisions {
                            log_buzzer_decision(decision);
                        }
                    }
                    Err(_) => warn!("buzzer test command ignored while a session is active"),
                }
            } else {
                let decision = test_session.trigger_feedback(arbiter, cue, now_ms);
                log_buzzer_decision(decision);
            }
        }
        BuzzerTestOp::Run => match test_session.start_scenario(
            arbiter,
            command.scenario.expect("validated buzzer test scenario"),
            now_ms,
        ) {
            Ok(decisions) => {
                for decision in decisions {
                    log_buzzer_decision(decision);
                }
            }
            Err(_) => warn!("buzzer test command ignored while a session is active"),
        },
        BuzzerTestOp::Stop => {
            if let Some(decision) = test_session.stop_playback(arbiter, now_ms) {
                log_buzzer_decision(decision);
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_buzzer_task(
    mut buzzer_timer: esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'static>>,
    mut buzzer_pwm: PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 2, true>,
    peripheral_clock: PeripheralClockConfig,
    #[cfg(feature = "buzzer-observe")] buzzer_edge_counter: Unit<'static, 0>,
) -> ! {
    let mut arbiter = BuzzerArbiter::new();
    let mut applied = BuzzerHardwareState::default();
    let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
    #[cfg(feature = "buzzer-test")]
    let mut test_session = BuzzerTestSession::new();
    #[cfg(feature = "buzzer-observe")]
    let mut output_trace = BuzzerTestOutputTrace::new();

    loop {
        let now_ms = Instant::now().as_millis();
        #[cfg(feature = "buzzer-test")]
        for decision in test_session.advance(&mut arbiter, now_ms) {
            log_buzzer_decision(decision);
        }

        let tick = arbiter.tick(now_ms);
        if let Some(decision) = tick.deferred_start {
            log_buzzer_decision(decision);
            #[cfg(feature = "buzzer-test")]
            test_session.record_deferred_start(now_ms, decision);
        }
        #[cfg(feature = "buzzer-test")]
        for decision in test_session.settle_after_tick(&mut arbiter, now_ms) {
            log_buzzer_decision(decision);
        }
        let output = arbiter.output();
        let _output_changed = apply_buzzer_output(
            &mut buzzer_timer,
            &mut buzzer_pwm,
            &peripheral_clock,
            output,
            &mut applied,
            &mut configured_frequency_hz,
        );
        #[cfg(feature = "buzzer-observe")]
        if _output_changed {
            output_trace.record(now_ms, output, buzzer_edge_counter.value() as u16);
        }
        #[cfg(all(feature = "buzzer-test", feature = "buzzer-observe"))]
        publish_buzzer_test_status(&test_session, &arbiter, &output_trace);
        #[cfg(all(feature = "buzzer-test", not(feature = "buzzer-observe")))]
        publish_buzzer_test_status(&test_session, &arbiter);

        let next_deadline_ms = arbiter.next_transition_ms();
        #[cfg(feature = "buzzer-test")]
        let next_deadline_ms = match test_session.next_deadline_ms() {
            Some(test_deadline_ms) => Some(match next_deadline_ms {
                Some(deadline_ms) => deadline_ms.min(test_deadline_ms),
                None => test_deadline_ms,
            }),
            None => next_deadline_ms,
        };

        enum BuzzerTaskWake {
            Command(BuzzerRuntimeCommand),
            Safety(BuzzerSafetyCommand),
        }

        let wake = if let Some(deadline_ms) = next_deadline_ms {
            let delay_ms = deadline_ms
                .saturating_sub(Instant::now().as_millis())
                .max(1);
            match select3(
                BUZZER_COMMANDS.receive(),
                BUZZER_SAFETY_COMMAND.wait(),
                EmbassyTimer::after_millis(delay_ms),
            )
            .await
            {
                Either3::First(command) => Some(BuzzerTaskWake::Command(command)),
                Either3::Second(command) => Some(BuzzerTaskWake::Safety(command)),
                Either3::Third(_) => None,
            }
        } else {
            match select(BUZZER_COMMANDS.receive(), BUZZER_SAFETY_COMMAND.wait()).await {
                Either::First(command) => Some(BuzzerTaskWake::Command(command)),
                Either::Second(command) => Some(BuzzerTaskWake::Safety(command)),
            }
        };

        if let Some(wake) = wake {
            let now_ms = Instant::now().as_millis();
            match wake {
                BuzzerTaskWake::Command(command) => {
                    #[cfg(feature = "buzzer-test")]
                    if matches!(
                        &command,
                        BuzzerRuntimeCommand::Test(BuzzerTestCommand {
                            op: BuzzerTestOp::Trigger | BuzzerTestOp::Run,
                            ..
                        })
                    ) {
                        #[cfg(feature = "buzzer-observe")]
                        output_trace.reset(now_ms, buzzer_edge_counter.value() as u16);
                    }
                    #[cfg(feature = "buzzer-test")]
                    apply_buzzer_command(command, &mut arbiter, now_ms, &mut test_session);
                    #[cfg(not(feature = "buzzer-test"))]
                    apply_buzzer_command(command, &mut arbiter, now_ms);
                }
                BuzzerTaskWake::Safety(command) => {
                    #[cfg(feature = "buzzer-test")]
                    apply_buzzer_safety_command(command, &mut arbiter, now_ms, &mut test_session);
                    #[cfg(not(feature = "buzzer-test"))]
                    apply_buzzer_safety_command(command, &mut arbiter, now_ms);
                }
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn apply_status_light_output(
    red: &mut Output<'_>,
    green: &mut Output<'_>,
    blue: &mut Output<'_>,
    output: RgbChannels,
    last_output: &mut Option<RgbChannels>,
) {
    if last_output.is_some_and(|last| last == output) {
        return;
    }

    // LED1 is common-anode: low GPIO output sinks the selected color channel.
    if output.red {
        red.set_low();
    } else {
        red.set_high();
    }
    if output.green {
        green.set_low();
    } else {
        green.set_high();
    }
    if output.blue {
        blue.set_low();
    } else {
        blue.set_high();
    }
    *last_output = Some(output);
}

#[cfg(target_arch = "xtensa")]
static STATUS_LIGHT_STATE: AtomicU8 = AtomicU8::new(StatusLightState::Booting as u8);

#[cfg(target_arch = "xtensa")]
fn set_status_light_state(state: StatusLightState) {
    STATUS_LIGHT_STATE.store(state as u8, Ordering::Relaxed);
}

#[cfg(target_arch = "xtensa")]
fn status_light_state_from_code(code: u8) -> StatusLightState {
    match code {
        0 => StatusLightState::Booting,
        1 => StatusLightState::Ready,
        2 => StatusLightState::Heating,
        3 => StatusLightState::Cooling,
        4 => StatusLightState::Calibration,
        5 => StatusLightState::HeaterInterlocked,
        6 => StatusLightState::CoolingDisabledOvertemp,
        7 => StatusLightState::SensorFault,
        8 => StatusLightState::ThermalRunawayAttentionPending,
        9 => StatusLightState::ThermalRunaway,
        _ => StatusLightState::Booting,
    }
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_status_light_task(
    mut red: Output<'static>,
    mut green: Output<'static>,
    mut blue: Output<'static>,
    started_ms: u64,
) -> ! {
    let mut last_output = None;
    loop {
        refresh_status_light(
            &mut red,
            &mut green,
            &mut blue,
            started_ms,
            status_light_state_from_code(STATUS_LIGHT_STATE.load(Ordering::Relaxed)),
            &mut last_output,
        );
        EmbassyTimer::after_millis(20).await;
    }
}

#[cfg(target_arch = "xtensa")]
fn refresh_status_light(
    red: &mut Output<'_>,
    green: &mut Output<'_>,
    blue: &mut Output<'_>,
    started_ms: u64,
    state: StatusLightState,
    last_output: &mut Option<RgbChannels>,
) {
    let elapsed_ms = Instant::now().as_millis().saturating_sub(started_ms);
    apply_status_light_output(
        red,
        green,
        blue,
        status_light_output(state, elapsed_ms),
        last_output,
    );
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
    pd_controller: ControllerKind,
    last_pd_observation: Option<PdStatusObservation>,
    heater_power_backend: HeaterPowerBackend,
    pid_snapshot: HeaterPidSnapshot,
    heater_control_timing: HeaterControlTiming,
    heater_physical_output_percent: u8,
    manual_pps: ManualPpsState,
    #[cfg(test)]
    calibration: CalibrationRuntimeState,
    fan_command: FanHardwareCommand,
    current_rtd_fault: Option<HeaterFaultReason>,
    heater_fault_latched: Option<HeaterFaultReason>,
    attention_pending_after_fault_clear: bool,
    thermal_control_profile_preview: bool,
    active_thermal_control_profile: Option<ThermalControlProfile>,
    last_raw_state: FrontPanelRawState,
    latest_status_temp_c: f32,
    latest_control_temp_c: f32,
    control_measurement_guarded: bool,
    latest_rtd_raw_adc_mv: u16,
    latest_rtd_raw_adc_min_mv: u16,
    latest_rtd_raw_adc_max_mv: u16,
    latest_vin_raw_adc_mv: u16,
    vin_mv: u32,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_runtime_status_with_calibration(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    calibration: &CalibrationRuntimeState,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    let pd_contract_mv = effective_pd_contract_mv(
        &context.manual_pps,
        context.last_pd_observation,
        context.heater_power_backend,
    );
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

    let mut status = ControlPlaneStatus::boxed_from_device_status(
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
            board_temp_centi: temp_c_to_centi_c(context.latest_status_temp_c),
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
        ui_state.network.clone(),
    );
    status.target_temp_c = ui_state.target_temp_c;
    status.rtd_raw_adc_mv = context.latest_rtd_raw_adc_mv;
    status.rtd_raw_adc_min_mv = context.latest_rtd_raw_adc_min_mv;
    status.rtd_raw_adc_max_mv = context.latest_rtd_raw_adc_max_mv;
    status.rtd_raw_adc_spread_mv = context
        .latest_rtd_raw_adc_max_mv
        .saturating_sub(context.latest_rtd_raw_adc_min_mv);
    status.vin_raw_adc_mv = context.latest_vin_raw_adc_mv;
    status.adc_diagnostics = Box::new(adc_diagnostics_wire());
    status.manual_pps_enabled = context.manual_pps.enabled;
    status.manual_pps_mv = context.manual_pps.target_mv;
    status.manual_pps_ma = context.manual_pps.target_ma;
    status.pps_capability_min_mv = context.manual_pps.capability_min_mv;
    status.pps_capability_max_mv = context.manual_pps.capability_max_mv;
    status.pps_capability_max_ma = context.manual_pps.capability_max_ma;
    status.manual_pps_error = context.manual_pps.error.map(manual_pps_error_code);
    // This is contract metadata, not a VBUS current measurement. The legacy
    // `currentMa` field remains CH224Q telemetry. FUSB302B reports only its
    // negotiated PDO current through the explicit contract field below.
    status.pd_controller = error_code_string(context.pd_controller.as_str());
    let observed_contract = context
        .last_pd_observation
        .map(|observation| observation.contract)
        .filter(|contract| *contract != Contract::none());
    // The FUSB302B policy only gains a contract at PS_RDY. Its cached source
    // capabilities and a pending RDO must never appear as an active contract.
    let fusb_contract_pending =
        context.pd_controller == ControllerKind::Fusb302b && observed_contract.is_none();
    let fallback_contract_kind = if matches!(
        context.heater_power_backend,
        HeaterPowerBackend::PpsMos { .. }
    ) {
        ContractKind::Pps
    } else {
        ContractKind::Fixed
    };
    let contract_kind =
        observed_contract
            .map(|contract| contract.kind)
            .unwrap_or(if fusb_contract_pending {
                ContractKind::None
            } else {
                fallback_contract_kind
            });
    status.pd_contract_kind = error_code_string(contract_kind.as_str());
    status.pd_contract_current_ma = if fusb_contract_pending {
        0
    } else {
        observed_contract
            .map(|contract| contract.current_ma)
            .or(context.manual_pps.target_ma)
            .or(context.manual_pps.capability_max_ma)
            .unwrap_or(0)
    };
    let contract_voltage_mv = observed_contract
        .map(|contract| contract.voltage_mv)
        .unwrap_or(if fusb_contract_pending {
            0
        } else {
            pd_contract_mv
        });
    status.pd_contract_power_mw =
        (u32::from(contract_voltage_mv) * u32::from(status.pd_contract_current_ma)) / 1_000;
    status.pd_performance_guaranteed = if fusb_contract_pending {
        false
    } else {
        observed_contract
            .map(Contract::performance_guaranteed)
            .unwrap_or(
                matches!(pd_state, PdState::Ready)
                    && pd_contract_mv >= 20_000
                    && status.pd_contract_current_ma >= 3_000,
            )
    };
    status.pd_degraded_reason =
        if matches!(pd_state, PdState::Ready) && !status.pd_performance_guaranteed {
            Some(error_code_string("pd_contract_below_20v"))
        } else if !matches!(pd_state, PdState::Ready) {
            Some(error_code_string(
                if context.pd_controller == ControllerKind::Fusb302b {
                    fusb302b_degraded_reason()
                } else {
                    "pd_contract_unavailable"
                },
            ))
        } else {
            None
        };
    status.heater_fault_reason = context.heater_fault_latched.map(|reason| {
        let mut value = heapless::String::new();
        let _ = value.push_str(reason.label());
        value
    });
    status.fault_attention_pending = context.attention_pending_after_fault_clear;
    status.heater_lock_reason = ui_state.heater_lock_reason.map(Into::into);
    let mut heater_control_phase = heapless::String::new();
    let _ = heater_control_phase.push_str(context.pid_snapshot.phase.label());
    status.heater_control_phase = Some(heater_control_phase);
    status.heater_error_c = Some(context.pid_snapshot.error_c);
    status.heater_control_error_c = Some(context.pid_snapshot.control_error_c);
    status.heater_control_temp_c = Some(context.latest_control_temp_c);
    status.heater_control_measurement_guarded = context.control_measurement_guarded;
    status.heater_filtered_temp_c = Some(context.pid_snapshot.filtered_temp_c);
    status.heater_filtered_slope_c_per_s = Some(context.pid_snapshot.filtered_slope_c_per_s);
    status.heater_coast_active = context.pid_snapshot.coast_active;
    status.heater_control_interval_ms = context.heater_control_timing.interval_ms;
    status.heater_control_cycle_ms = context.heater_control_timing.cycle_ms;
    status.calibration = calibration_runtime_state_to_wire(calibration);
    status.thermal_control_profile_preview = context.thermal_control_profile_preview;
    let resolved_bank =
        resolve_thermal_profile_bank(memory_config.thermal_profile_mode, &context.manual_pps);
    let mut thermal_profile_mode = heapless::String::new();
    let _ = thermal_profile_mode.push_str(memory_config.thermal_profile_mode.as_str());
    status.thermal_profile_mode = thermal_profile_mode;
    let mut thermal_profile_resolved_bank = heapless::String::new();
    let _ = thermal_profile_resolved_bank.push_str(resolved_bank.as_str());
    status.thermal_profile_resolved_bank = thermal_profile_resolved_bank;
    status.thermal_control = if calibration.mode == CalibrationMode::ThermalPlant {
        flux_purr_firmware::control_plane::ThermalControlRuntimeWire::default()
    } else {
        thermal_control_runtime_wire(
            ui_state.target_temp_c,
            context.active_thermal_control_profile,
            context.thermal_control_profile_preview,
        )
    };
    status.thermal_plant_model = thermal_plant_runtime_wire(memory_config);
    status
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_runtime_status(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    calibration: &CalibrationRuntimeState,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    usb_runtime_status_with_calibration(ui_state, memory_config, calibration, context)
}

#[cfg(test)]
fn usb_runtime_status(
    ui_state: &FrontPanelUiState,
    memory_config: &MemoryConfig,
    context: UsbRuntimeStatusContext,
) -> Box<ControlPlaneStatus> {
    let calibration = context.calibration;
    usb_runtime_status_with_calibration(ui_state, memory_config, &calibration, context)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
#[allow(clippy::too_many_arguments)]
fn usb_runtime_config_response_with_calibration(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    mut config: RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    calibration: &mut CalibrationRuntimeState,
    mut context: UsbRuntimeStatusContext,
) -> UsbFrame {
    let manual_pps_requested = config.manual_pps_enabled.is_some()
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some();
    if thermal_plant_calibration_job_running(*calibration)
        && (manual_pps_requested
            || config.calibration.is_some()
            || config.heater_enabled == Some(true))
    {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(ApiError::new(
                ManualPpsError::CalibrationInProgress.code(),
                "Manual PPS and heater controls cannot override a running thermal-model calibration.",
                false,
            )),
        };
    }
    if let Some(command) = config.thermal_control_profile {
        match command.op {
            ThermalControlProfileOp::Preview | ThermalControlProfileOp::Save
                if command.profile.is_none() =>
            {
                return UsbFrame::Response {
                    request_id,
                    ok: false,
                    result: None,
                    error: Some(ApiError::new(
                        "thermal_profile_required",
                        "thermalControlProfile.profile is required for preview/save.",
                        false,
                    )),
                };
            }
            ThermalControlProfileOp::Save
                if command.profile.is_some_and(|profile| {
                    profile.points.iter().flatten().count()
                        > THERMAL_CONTROL_PROFILE_PERSISTED_MAX_POINTS
                }) =>
            {
                return UsbFrame::Response {
                    request_id,
                    ok: false,
                    result: None,
                    error: Some(ApiError::new(
                        "thermal_profile_too_many_saved_points",
                        "saved thermal profiles support at most 10 populated points.",
                        false,
                    )),
                };
            }
            ThermalControlProfileOp::ClearPreview | ThermalControlProfileOp::ClearSaved
                if command.profile.is_some() =>
            {
                return UsbFrame::Response {
                    request_id,
                    ok: false,
                    result: None,
                    error: Some(ApiError::new(
                        "thermal_profile_clear_payload",
                        "thermalControlProfile.profile must be omitted for clear operations.",
                        false,
                    )),
                };
            }
            _ => {}
        }
    }

    if let Some(calibration_command) = config.calibration
        && let Err(error) =
            apply_calibration_control_config(&calibration_command, calibration, manual_pps)
    {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(ApiError::new(error.code(), error.message(), false)),
        };
    }
    if let Err(error) = apply_manual_pps_config(&config, *calibration, manual_pps) {
        return UsbFrame::Response {
            request_id,
            ok: false,
            result: None,
            error: Some(ApiError::new(error.code(), error.message(), false)),
        };
    }
    if let Some(command) = config.thermal_control_profile {
        match command.op {
            ThermalControlProfileOp::Preview => {
                let Some(profile) = command.profile else {
                    return UsbFrame::Response {
                        request_id,
                        ok: false,
                        result: None,
                        error: Some(ApiError::new(
                            "thermal_profile_required",
                            "thermalControlProfile.profile is required for preview.",
                            false,
                        )),
                    };
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
    if config.heater_enabled == Some(true)
        && !thermal_model_heater_allowed(memory_config, *calibration, *manual_pps)
    {
        config.heater_enabled = Some(false);
    }
    apply_memory_config_to_ui(ui_state, memory_config);
    if let Some(heater_enabled) = config.heater_enabled {
        ui_state.heater_enabled = heater_enabled;
    }
    if calibration.mode != CalibrationMode::Off {
        if let Some(heater_enabled) = config
            .calibration
            .and_then(|calibration| calibration.heater_enabled)
        {
            ui_state.heater_enabled = heater_enabled;
        }
        if calibration.mode == CalibrationMode::RtdAdc
            && calibration.heater_enabled
            && let Some(target_adc_mv) = calibration.target_adc_mv
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
    context.active_thermal_control_profile =
        active_thermal_control_profile(memory_config, *thermal_control_profile_preview, manual_pps);

    usb_response(
        request_id,
        UsbResponsePayload::Status(usb_runtime_status_with_calibration(
            ui_state,
            memory_config,
            calibration,
            context,
        )),
    )
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[allow(clippy::too_many_arguments)]
fn usb_runtime_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    calibration: &mut CalibrationRuntimeState,
    context: UsbRuntimeStatusContext,
) -> UsbFrame {
    usb_runtime_config_response_with_calibration(
        request_id,
        config,
        ui_state,
        memory_config,
        manual_pps,
        thermal_control_profile_preview,
        calibration,
        context,
    )
}

#[cfg(test)]
fn usb_runtime_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: RuntimeConfigCommand,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    context: UsbRuntimeStatusContext,
) -> (UsbFrame, CalibrationRuntimeState) {
    let mut calibration = context.calibration;
    let response = usb_runtime_config_response_with_calibration(
        request_id,
        config,
        ui_state,
        memory_config,
        manual_pps,
        thermal_control_profile_preview,
        &mut calibration,
        context,
    );
    (response, calibration)
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn apply_manual_pps_config(
    config: &RuntimeConfigCommand,
    calibration: CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    let manual_pps_requested = config.manual_pps_enabled.is_some()
        || config.manual_pps_mv.is_some()
        || config.manual_pps_ma.is_some();
    if calibration.immediate_heater_disarm_pending
        && manual_pps_requested
        && config.manual_pps_enabled != Some(false)
    {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    if calibration.mode == CalibrationMode::ThermalPlant
        && calibration.job.status == CalibrationJobStatus::Running
        && manual_pps_requested
    {
        return Err(ManualPpsError::CalibrationInProgress);
    }
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
    let requests_mutation = config.mode.is_some()
        || config.pps_enabled.is_some()
        || config.pps_mv.is_some()
        || config.heater_enabled.is_some()
        || config.target_adc_mv.is_some();
    if thermal_plant_calibration_job_running(*calibration) && requests_mutation {
        return Err(ManualPpsError::CalibrationInProgress);
    }
    if calibration.immediate_heater_disarm_pending && requests_mutation {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    if config.mode == Some(CalibrationModeWire::ThermalPlant) {
        return Err(ManualPpsError::ThermalPlantManagedByJob);
    }
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
            if manual_pps.owner == ManualPpsOwner::Calibration {
                manual_pps.clear();
                calibration.immediate_heater_disarm_pending = true;
            }
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

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn thermal_plant_calibration_job_running(calibration: CalibrationRuntimeState) -> bool {
    calibration.mode == CalibrationMode::ThermalPlant
        && calibration.job.status == CalibrationJobStatus::Running
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
        CalibrationMode::Off | CalibrationMode::HeaterCurve | CalibrationMode::ThermalPlant => None,
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
    let thermal_plant_job = calibration.job.kind == Some(CalibrationJobKind::ThermalPlant);
    calibration.job.status = CalibrationJobStatus::Failed;
    calibration.job.message = Some(error);
    calibration.job_data = None;
    calibration.model_target_temp_c = None;
    calibration.heater_enabled = false;
    if thermal_plant_job {
        calibration.mode = CalibrationMode::Off;
        calibration.immediate_heater_disarm_pending = true;
    }
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
    calibration.model_target_temp_c = None;
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn calibration_job_canceled(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    if calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    calibration.job.status = CalibrationJobStatus::Canceled;
    calibration.job.message = None;
    calibration.job_data = None;
    calibration.heater_enabled = false;
    calibration.mode = CalibrationMode::Off;
    calibration.immediate_heater_disarm_pending = true;
    if manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
        calibration.pps_enabled = false;
        calibration.pps_mv = None;
        calibration.pps_ma = None;
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn disarm_calibration_after_transient_input_change(
    calibration: &mut CalibrationRuntimeState,
    manual_pps: &mut ManualPpsState,
) {
    calibration.heater_enabled = false;
    calibration.pps_enabled = false;
    calibration.pps_mv = None;
    calibration.pps_ma = None;
    if manual_pps.owner == ManualPpsOwner::Calibration {
        manual_pps.clear();
    }
    calibration.immediate_heater_disarm_pending = true;
}

#[cfg(any(target_arch = "xtensa", test))]
fn calibration_job_start_with_workspace(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> Result<(), ManualPpsError> {
    if calibration.job.status == CalibrationJobStatus::Running
        || calibration.immediate_heater_disarm_pending
    {
        return Err(ManualPpsError::TerminalDisarmPending);
    }
    match kind {
        CalibrationJobKind::VinAdc => {
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
            calibration.job_data = Some(CalibrationJobData::VinAdc(CalibrationVinAutoJob {
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
        CalibrationJobKind::ThermalPlant => {
            let (_, source_max_mv, source_current_ma) = manual_pps
                .thermal_plant_source_limits()
                .ok_or(ManualPpsError::ThermalPlantSourceUnsupported)?;
            let request_mv = source_max_mv;
            manual_pps.enable(
                ManualPpsOwner::Calibration,
                request_mv,
                Some(source_current_ma),
            )?;
            calibration.mode = CalibrationMode::ThermalPlant;
            calibration.pps_enabled = true;
            calibration.pps_mv = manual_pps.target_mv;
            calibration.pps_ma = manual_pps.target_ma;
            calibration.heater_enabled = false;
            calibration.model_target_temp_c = None;
            calibration.job = CalibrationJobState {
                kind: Some(kind),
                status: CalibrationJobStatus::Running,
                progress_percent: 0,
                samples_collected: 0,
                next_request_mv: Some(request_mv),
                message: None,
            };
            thermal_plant_workspace.next_run_id =
                thermal_plant_workspace.next_run_id.wrapping_add(1).max(1);
            thermal_plant_workspace.job = Some(CalibrationThermalPlantAutoJob {
                run_id: thermal_plant_workspace.next_run_id,
                phase: ThermalPlantAutoPhase::Ambient,
                source_max_mv,
                source_current_ma,
                ambient_raw_rtd_adc_mv: 0,
                idle_samples: 0,
                heater_curve: ThermalPlantCurveSampler::default(),
                elapsed_ticks: 0,
                phase_started_tick: 0,
                sample_count: 0,
                last_saved_temp_c: 0.0,
                last_saved_tick: 0,
                samples: [ThermalPlantTransientSample {
                    elapsed_ticks: 0,
                    raw_rtd_adc_mv: 0,
                    heater_voltage_100mv: 0,
                    duty_percent: 0,
                }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
            });
            calibration.job_data = Some(CalibrationJobData::ThermalPlant);
            Ok(())
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn calibration_job_start(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> Result<(), ManualPpsError> {
    calibration_job_start_with_workspace(
        calibration,
        kind,
        memory_config,
        manual_pps,
        thermal_plant_workspace,
    )
}

#[cfg(test)]
std::thread_local! {
    static TEST_THERMAL_PLANT_WORKSPACE: core::cell::RefCell<CalibrationThermalPlantWorkspace> =
        core::cell::RefCell::new(CalibrationThermalPlantWorkspace::default());
}

#[cfg(test)]
fn calibration_job_start(
    calibration: &mut CalibrationRuntimeState,
    kind: CalibrationJobKind,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
) -> Result<(), ManualPpsError> {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        calibration_job_start_with_workspace(
            calibration,
            kind,
            memory_config,
            manual_pps,
            &mut workspace.borrow_mut(),
        )
    })
}

#[cfg(test)]
fn test_thermal_plant_phase() -> Option<ThermalPlantAutoPhase> {
    TEST_THERMAL_PLANT_WORKSPACE
        .with(|workspace| workspace.borrow().job.as_ref().map(|job| job.phase))
}

#[cfg(test)]
fn test_install_thermal_plant_job(job: CalibrationThermalPlantAutoJob) {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        workspace.borrow_mut().job = Some(job);
    });
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
fn enforce_heater_curve_model_floor(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
) {
    for point in points {
        let temp_c = f32::from(point.temp_centi_c) / 100.0;
        point.resistance_milliohms = point
            .resistance_milliohms
            .max(heater_curve_model_floor_milliohms(temp_c));
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_curve_model_floor_milliohms(temp_c: f32) -> u16 {
    round_to_u16_nonnegative(default_estimated_heater_resistance_ohms(temp_c) * 1000.0)
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
fn heater_curve_from_transient_bins(bins: &[ThermalPlantCurveBin; 4]) -> Option<HeaterCurveConfig> {
    let mut measured = heapless::Vec::<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>::new();
    for bin in bins {
        let Some((temp_centi_c, resistance_milliohms)) = bin.averaged_point() else {
            continue;
        };
        let _ = measured.push(HeaterCurvePoint {
            temp_centi_c,
            resistance_milliohms,
        });
    }
    if measured.is_empty() {
        return None;
    }
    monotonic_smooth_heater_curve_points(&mut measured);
    enforce_heater_curve_model_floor(&mut measured);

    let mut compacted = heapless::Vec::<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>::new();
    push_heater_curve_point_monotonic(
        &mut compacted,
        default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C),
    );
    push_heater_curve_point_monotonic(
        &mut compacted,
        default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C),
    );
    for point in measured {
        push_heater_curve_point_monotonic(&mut compacted, point);
    }

    let mut points = [None; HEATER_CURVE_MAX_POINTS];
    for (index, point) in compacted.into_iter().enumerate() {
        points[index] = Some(point);
    }
    Some(HeaterCurveConfig { points })
}

#[cfg(any(target_arch = "xtensa", test))]
fn heater_curve_raw_observations_from_transient_bins(
    cold_bin: ThermalPlantCurveBin,
    bins: &[ThermalPlantCurveBin; 4],
) -> Option<HeaterCurveRawObservations> {
    let mut points = [None; HEATER_CURVE_MAX_POINTS];
    let mut count = 0;
    if let Some(point) = cold_bin.averaged_raw_observation() {
        points[count] = Some(point);
        count += 1;
    }
    for bin in bins {
        if let Some(point) = bin.averaged_raw_observation() {
            points[count] = Some(point);
            count += 1;
        }
    }
    (count >= 2).then_some(HeaterCurveRawObservations { points })
}

#[cfg(any(target_arch = "xtensa", test))]
fn thermal_plant_curve_samples_ready(job: &ThermalPlantCurveSampler) -> bool {
    job.cold_bin.samples >= THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN
        && job
            .bins
            .iter()
            .all(|bin| bin.samples >= THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN)
}

#[cfg(any(target_arch = "xtensa", test))]
fn default_heater_curve_point(temp_c: f32) -> HeaterCurvePoint {
    HeaterCurvePoint {
        temp_centi_c: round_to_i16(temp_c * 100.0),
        resistance_milliohms: round_to_u16_nonnegative(
            default_estimated_heater_resistance_ohms(temp_c) * 1_000.0,
        ),
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn push_heater_curve_point_monotonic(
    points: &mut heapless::Vec<HeaterCurvePoint, { HEATER_CURVE_MAX_POINTS }>,
    mut point: HeaterCurvePoint,
) {
    if let Some(previous) = points.last().copied() {
        if point.temp_centi_c <= previous.temp_centi_c {
            point.temp_centi_c = previous.temp_centi_c.saturating_add(1);
        }
        if point.resistance_milliohms < previous.resistance_milliohms {
            point.resistance_milliohms = previous.resistance_milliohms;
        }
    }
    let _ = points.push(point);
}

#[cfg(any(target_arch = "xtensa", test))]
fn record_thermal_plant_transient_sample(
    job: &mut CalibrationThermalPlantAutoJob,
    raw_rtd_adc_mv: u16,
    latest_temp_c: f32,
    latest_vin_mv: u32,
    duty_percent: u8,
    force: bool,
) -> bool {
    if raw_rtd_adc_mv == 0 || !latest_temp_c.is_finite() || duty_percent > 100 {
        return false;
    }
    let elapsed_ticks = job.elapsed_ticks.min(u32::from(u16::MAX)) as u16;
    let should_record = force
        || job.sample_count < 24
        || (latest_temp_c - job.last_saved_temp_c).abs() >= THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C;
    if !should_record {
        return true;
    }
    let index = usize::from(job.sample_count);
    if index >= THERMAL_PLANT_TRANSIENT_MAX_SAMPLES
        || (index > 0 && elapsed_ticks <= job.last_saved_tick)
    {
        return false;
    }
    job.samples[index] = ThermalPlantTransientSample {
        elapsed_ticks,
        raw_rtd_adc_mv,
        heater_voltage_100mv: (latest_vin_mv / 100).min(u32::from(u8::MAX)) as u8,
        duty_percent,
    };
    job.sample_count = job.sample_count.saturating_add(1);
    job.last_saved_temp_c = latest_temp_c;
    job.last_saved_tick = elapsed_ticks;
    true
}

#[cfg(any(target_arch = "xtensa", test))]
fn transient_sample_power_mw(
    sample: ThermalPlantTransientSample,
    temp_c: f32,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> Option<f32> {
    if sample.duty_percent == 0 {
        return Some(0.0);
    }
    let voltage_v = f32::from(sample.heater_voltage_100mv) / 10.0;
    let resistance_ohms =
        estimated_heater_resistance_ohms(temp_c, preview_heater_curve, memory_config);
    (voltage_v > 0.0 && resistance_ohms.is_finite() && resistance_ohms > 0.1).then_some(
        voltage_v * voltage_v / resistance_ohms * 1_000.0 * f32::from(sample.duty_percent) / 100.0,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn solve_transient_normal_equations(
    normal: [[f32; 3]; 3],
    rhs: [f32; 3],
    mask: u8,
) -> Option<[f32; 3]> {
    let mut selected = [0usize; 3];
    let mut count = 0usize;
    for column in 0..3 {
        if mask & (1 << column) != 0 {
            selected[count] = column;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mut matrix = [[0.0_f32; 3]; 3];
    let mut target = [0.0_f32; 3];
    for row in 0..count {
        target[row] = rhs[selected[row]];
        for column in 0..count {
            matrix[row][column] = normal[selected[row]][selected[column]];
        }
    }
    for pivot in 0..count {
        let mut best = pivot;
        for row in pivot + 1..count {
            if matrix[row][pivot].abs() > matrix[best][pivot].abs() {
                best = row;
            }
        }
        if matrix[best][pivot].abs() < 1.0e-6 {
            return None;
        }
        if best != pivot {
            matrix.swap(best, pivot);
            target.swap(best, pivot);
        }
        let divisor = matrix[pivot][pivot];
        for value in matrix[pivot][pivot..count].iter_mut() {
            *value /= divisor;
        }
        target[pivot] /= divisor;
        for row in 0..count {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            let pivot_row = matrix[pivot];
            for (offset, value) in matrix[row][pivot..count].iter_mut().enumerate() {
                *value -= factor * pivot_row[pivot + offset];
            }
            target[row] -= factor * target[pivot];
        }
    }
    let mut output = [0.0_f32; 3];
    for index in 0..count {
        output[selected[index]] = target[index];
    }
    Some(output)
}

#[cfg(any(target_arch = "xtensa", test))]
fn determinant_3x3(matrix: [[f32; 3]; 3]) -> f32 {
    matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
}

#[cfg(any(target_arch = "xtensa", test))]
fn transient_fit_row(
    samples: &[ThermalPlantTransientSample],
    temperatures_c: &[f32],
    powers_mw: &[f32],
    index: usize,
    delay_ticks: u16,
    ambient_temp_c: f32,
) -> Option<([f32; 3], f32)> {
    const MIN_DERIVATIVE_WINDOW_TICKS: u16 = 10;
    let previous_index = (0..index).rev().find(|candidate| {
        samples[index]
            .elapsed_ticks
            .saturating_sub(samples[*candidate].elapsed_ticks)
            >= MIN_DERIVATIVE_WINDOW_TICKS
    })?;
    let dt_ticks = samples[index]
        .elapsed_ticks
        .saturating_sub(samples[previous_index].elapsed_ticks);
    let delayed_tick = samples[index].elapsed_ticks.saturating_sub(delay_ticks);
    let delayed_index = (0..index)
        .rev()
        .find(|candidate| samples[*candidate].elapsed_ticks <= delayed_tick)?;
    let mid_temp_c = (temperatures_c[index] + temperatures_c[previous_index]) * 0.5;
    let ambient_kelvin = ambient_temp_c + 273.15;
    let kelvin = mid_temp_c + 273.15;
    Some((
        [
            (temperatures_c[index] - temperatures_c[previous_index])
                / (f32::from(dt_ticks) * HEATER_CONTROL_INTERVAL_MS as f32 / 1_000.0),
            (mid_temp_c - ambient_temp_c).max(0.0),
            kelvin.powi(4) - ambient_kelvin.powi(4),
        ],
        powers_mw[delayed_index],
    ))
}

#[cfg(any(target_arch = "xtensa", test))]
fn fit_thermal_plant_transient(
    transaction_id: u32,
    ambient_raw_rtd_adc_mv: u16,
    samples: &[ThermalPlantTransientSample; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
    sample_count: u8,
    preview_heater_curve: Option<&HeaterCurveConfig>,
    memory_config: &MemoryConfig,
) -> Option<(ThermalPlantTransientTransaction, f32)> {
    if usize::from(sample_count) < 24 {
        return None;
    }
    let count = usize::from(sample_count);
    let samples = &samples[..count];
    let ambient_temp_c = projected_rtd_temperature_c(memory_config, ambient_raw_rtd_adc_mv)?;
    let mut temperatures_c = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powers_mw = [0.0_f32; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
    let mut powered_max_temp_c = f32::MIN;
    let mut powered_peak_index = None;
    for (index, sample) in samples.iter().enumerate() {
        let temperature_c = projected_rtd_temperature_c(memory_config, sample.raw_rtd_adc_mv)?;
        let power_mw =
            transient_sample_power_mw(*sample, temperature_c, preview_heater_curve, memory_config)?;
        if !temperature_c.is_finite() || !power_mw.is_finite() || power_mw < 0.0 {
            return None;
        }
        if sample.duty_percent == 100 && temperature_c > powered_max_temp_c {
            powered_max_temp_c = temperature_c;
            powered_peak_index = Some(index);
        }
        temperatures_c[index] = temperature_c;
        powers_mw[index] = power_mw;
    }
    let powered_peak_index = powered_peak_index?;
    if powered_max_temp_c < THERMAL_PLANT_TARGET_TEMP_C
        || (temperatures_c[0] - ambient_temp_c).abs() > 8.0
        || powered_peak_index + 1 >= count
        || samples.last().is_none_or(|sample| sample.duty_percent != 0)
        || temperatures_c[count - 1] > THERMAL_PLANT_COOL_COMPLETE_TEMP_C
    {
        return None;
    }

    let mut best: Option<(ThermalPlantProjection, f32)> = None;
    for delay_ticks in 0..=200_u16 {
        let mut scale_sums = [0.0_f32; 3];
        let mut row_count = 0usize;
        for index in 1..count {
            if let Some((values, _)) = transient_fit_row(
                samples,
                &temperatures_c[..count],
                &powers_mw[..count],
                index,
                delay_ticks,
                ambient_temp_c,
            ) {
                for column in 0..3 {
                    scale_sums[column] += values[column] * values[column];
                }
                row_count += 1;
            }
        }
        if row_count < 12 {
            continue;
        }
        let scales = scale_sums.map(|sum| (sum / row_count as f32).sqrt());
        if scales
            .iter()
            .any(|scale| !scale.is_finite() || *scale < 1.0e-6)
        {
            continue;
        }
        let mut normal = [[0.0_f32; 3]; 3];
        let mut rhs = [0.0_f32; 3];
        let mut power_sum_sq = 0.0_f32;
        for index in 1..count {
            let Some((values, target)) = transient_fit_row(
                samples,
                &temperatures_c[..count],
                &powers_mw[..count],
                index,
                delay_ticks,
                ambient_temp_c,
            ) else {
                continue;
            };
            let normalized = [
                values[0] / scales[0],
                values[1] / scales[1],
                values[2] / scales[2],
            ];
            for row in 0..3 {
                rhs[row] += normalized[row] * target;
                for column in 0..3 {
                    normal[row][column] += normalized[row] * normalized[column];
                }
            }
            power_sum_sq += target * target;
        }
        let gram = normal.map(|row| row.map(|value| value / row_count as f32));
        if determinant_3x3(gram).abs() < 1.0e-5 || power_sum_sq <= 1.0 {
            continue;
        }
        for mask in 1..8_u8 {
            let Some(solution) = solve_transient_normal_equations(normal, rhs, mask) else {
                continue;
            };
            if solution.iter().any(|value| *value < -1.0e-4) {
                continue;
            }
            let coefficients = [
                (solution[0] / scales[0]).max(0.0),
                (solution[1] / scales[1]).max(0.0),
                (solution[2] / scales[2]).max(0.0),
            ];
            let mut residual_sum_sq = 0.0_f32;
            for index in 1..count {
                let Some((values, target)) = transient_fit_row(
                    samples,
                    &temperatures_c[..count],
                    &powers_mw[..count],
                    index,
                    delay_ticks,
                    ambient_temp_c,
                ) else {
                    continue;
                };
                let predicted = coefficients[0] * values[0]
                    + coefficients[1] * values[1]
                    + coefficients[2] * values[2];
                let residual = target - predicted;
                residual_sum_sq += residual * residual;
            }
            let residual = (residual_sum_sq / power_sum_sq).sqrt();
            let projection = ThermalPlantProjection {
                thermal_capacity_mj_per_c: coefficients[0],
                convection_mw_per_c: coefficients[1],
                radiation_mw_per_k4: coefficients[2],
                transport_delay_ms: u32::from(delay_ticks) * HEATER_CONTROL_INTERVAL_MS as u32,
            };
            if !projection.thermal_capacity_mj_per_c.is_finite()
                || !projection.convection_mw_per_c.is_finite()
                || !projection.radiation_mw_per_k4.is_finite()
                || !(100.0..=2_000_000.0).contains(&projection.thermal_capacity_mj_per_c)
                || !(0.0..=THERMAL_PLANT_TRANSIENT_MAX_CONVECTION_MW_PER_C)
                    .contains(&projection.convection_mw_per_c)
                || !(0.0..=THERMAL_PLANT_TRANSIENT_MAX_RADIATION_MW_PER_K4)
                    .contains(&projection.radiation_mw_per_k4)
                || residual > 0.20
            {
                continue;
            }
            if best.is_none_or(|(_, current)| residual < current) {
                best = Some((projection, residual));
            }
        }
    }
    let (projection, residual) = best?;
    let transaction = ThermalPlantTransientTransaction {
        transaction_id: transaction_id.max(1),
        ambient_raw_rtd_adc_mv,
        sample_count,
        projection: ThermalPlantProjectionRecord::from_projection(projection),
        samples: {
            let mut copied = [ThermalPlantTransientSample {
                elapsed_ticks: 0,
                raw_rtd_adc_mv: 0,
                heater_voltage_100mv: 0,
                duty_percent: 0,
            }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
            copied[..count].copy_from_slice(samples);
            copied
        },
    };
    flux_purr_firmware::memory::thermal_plant_transient_transaction_is_complete(&transaction)
        .then_some((transaction, residual))
}

#[cfg(any(target_arch = "xtensa", test))]
fn rebuild_transient_thermal_plant_for_current_inputs(memory_config: &mut MemoryConfig) -> bool {
    let Some(previous) = memory_config.thermal_plant_transient_active else {
        return false;
    };

    let rebuilt = has_persisted_heater_resistance_curve(memory_config)
        .then(|| {
            fit_thermal_plant_transient(
                previous.transaction_id,
                previous.ambient_raw_rtd_adc_mv,
                &previous.samples,
                previous.sample_count,
                None,
                memory_config,
            )
            .map(|(transaction, _)| transaction)
        })
        .flatten();
    let rebuilt = if let Some(rebuilt) = rebuilt {
        rebuilt
    } else {
        let mut invalid = previous;
        invalid.projection = ThermalPlantProjectionRecord {
            convection_mw_per_c_bits: 0,
            radiation_mw_per_k4_bits: 0,
            thermal_capacity_mj_per_c_bits: 0,
            transport_delay_ms: 0,
        };
        invalid
    };
    memory_config.thermal_plant_transient_active = Some(rebuilt);
    true
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
fn invalidate_transient_thermal_plant(memory_config: &mut MemoryConfig) {
    if let Some(transaction) = memory_config.thermal_plant_transient_active.as_mut() {
        transaction.projection = ThermalPlantProjectionRecord {
            convection_mw_per_c_bits: 0,
            radiation_mw_per_k4_bits: 0,
            thermal_capacity_mj_per_c_bits: 0,
            transport_delay_ms: 0,
        };
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(any(target_arch = "xtensa", test))]
fn update_calibration_job_state_with_workspace(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_temp_c: f32,
    pd_current_ma: u16,
    latest_vin_mv: u32,
    heater_duty_percent: u8,
) {
    if calibration.job.status != CalibrationJobStatus::Running {
        return;
    }
    let Some(job_data) = calibration.job_data else {
        return;
    };

    match job_data {
        CalibrationJobData::VinAdc(mut job) => {
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
                        CalibrationJobKind::VinAdc,
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
            calibration.job_data = Some(CalibrationJobData::VinAdc(job));
        }
        CalibrationJobData::ThermalPlant => {
            let Some(job) = thermal_plant_workspace.job.as_mut() else {
                calibration_job_fail(
                    calibration,
                    ManualPpsError::ThermalPlantProjectionInvalid,
                    true,
                    manual_pps,
                );
                return;
            };
            let source_lost = manual_pps.error.is_some()
                || !manual_pps.enabled
                || manual_pps.owner != ManualPpsOwner::Calibration
                || !manual_pps.has_matching_pps_apdo(20_000, job.source_current_ma);
            if source_lost {
                calibration_job_fail(
                    calibration,
                    manual_pps
                        .error
                        .unwrap_or(ManualPpsError::ThermalPlantSourceUnsupported),
                    true,
                    manual_pps,
                );
                return;
            }
            let Some(recorded_temp_c) =
                projected_rtd_temperature_c(memory_config, latest_rtd_raw_adc_mv)
            else {
                calibration_job_fail(
                    calibration,
                    ManualPpsError::ThermalPlantProjectionInvalid,
                    true,
                    manual_pps,
                );
                return;
            };
            job.elapsed_ticks = job.elapsed_ticks.saturating_add(1);
            match job.phase {
                ThermalPlantAutoPhase::Ambient => {
                    calibration.heater_enabled = false;
                    job.ambient_raw_rtd_adc_mv = if job.idle_samples == 0 {
                        latest_rtd_raw_adc_mv
                    } else {
                        ((u32::from(job.ambient_raw_rtd_adc_mv) * u32::from(job.idle_samples)
                            + u32::from(latest_rtd_raw_adc_mv))
                            / (u32::from(job.idle_samples) + 1)) as u16
                    };
                    job.idle_samples = job.idle_samples.saturating_add(1);
                    calibration.job.progress_percent = 1;
                    if job.idle_samples >= THERMAL_PLANT_AMBIENT_TICKS {
                        if !record_thermal_plant_transient_sample(
                            job,
                            latest_rtd_raw_adc_mv,
                            recorded_temp_c,
                            latest_vin_mv,
                            0,
                            true,
                        ) {
                            calibration_job_fail(
                                calibration,
                                ManualPpsError::ThermalPlantProjectionInvalid,
                                true,
                                manual_pps,
                            );
                            return;
                        }
                        job.phase = ThermalPlantAutoPhase::Heating;
                        job.phase_started_tick = job.elapsed_ticks;
                        calibration.heater_enabled = true;
                    }
                }
                ThermalPlantAutoPhase::Heating => {
                    let request_mv = job.source_max_mv;
                    if manual_pps.target_mv != Some(request_mv)
                        && let Err(error) = manual_pps.enable(
                            ManualPpsOwner::Calibration,
                            request_mv,
                            Some(job.source_current_ma),
                        )
                    {
                        calibration_job_fail(calibration, error, true, manual_pps);
                        return;
                    }
                    calibration.pps_enabled = true;
                    calibration.pps_mv = Some(request_mv);
                    calibration.pps_ma = Some(job.source_current_ma);
                    calibration.job.next_request_mv = Some(request_mv);
                    if heater_duty_percent > 0 && (latest_vin_mv == 0 || pd_current_ma == 0) {
                        calibration_job_fail(
                            calibration,
                            ManualPpsError::ThermalPlantProjectionInvalid,
                            true,
                            manual_pps,
                        );
                        return;
                    }
                    if heater_duty_percent > 0 {
                        if job.heater_curve.cold_bin.contains(latest_temp_c) {
                            job.heater_curve.cold_bin.observe_electrical(
                                latest_temp_c,
                                latest_rtd_raw_adc_mv,
                                latest_vin_mv,
                                pd_current_ma,
                            );
                        }
                        for bin in &mut job.heater_curve.bins {
                            if (*bin).contains(latest_temp_c) {
                                bin.observe_electrical(
                                    latest_temp_c,
                                    latest_rtd_raw_adc_mv,
                                    latest_vin_mv,
                                    pd_current_ma,
                                );
                            }
                        }
                    }
                    if job.elapsed_ticks.saturating_sub(job.phase_started_tick)
                        > THERMAL_PLANT_HEAT_TIMEOUT_TICKS
                        || !record_thermal_plant_transient_sample(
                            job,
                            latest_rtd_raw_adc_mv,
                            recorded_temp_c,
                            latest_vin_mv,
                            heater_duty_percent,
                            latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C,
                        )
                    {
                        calibration_job_fail(
                            calibration,
                            ManualPpsError::ThermalPlantProjectionInvalid,
                            true,
                            manual_pps,
                        );
                        return;
                    }
                    calibration.job.progress_percent = (2.0
                        + (latest_temp_c / THERMAL_PLANT_TARGET_TEMP_C).clamp(0.0, 1.0) * 58.0)
                        as u8;
                    if latest_temp_c >= THERMAL_PLANT_TARGET_TEMP_C {
                        calibration.heater_enabled = false;
                        job.phase = ThermalPlantAutoPhase::Cooling;
                        job.phase_started_tick = job.elapsed_ticks;
                        calibration.job.progress_percent = 60;
                    } else {
                        calibration.heater_enabled = true;
                    }
                }
                ThermalPlantAutoPhase::Cooling => {
                    calibration.heater_enabled = false;
                    if job.elapsed_ticks.saturating_sub(job.phase_started_tick)
                        > THERMAL_PLANT_COOL_TIMEOUT_TICKS
                        || !record_thermal_plant_transient_sample(
                            job,
                            latest_rtd_raw_adc_mv,
                            recorded_temp_c,
                            latest_vin_mv,
                            heater_duty_percent,
                            thermal_plant_cooling_complete(latest_temp_c, recorded_temp_c),
                        )
                    {
                        calibration_job_fail(
                            calibration,
                            ManualPpsError::ThermalPlantProjectionInvalid,
                            true,
                            manual_pps,
                        );
                        return;
                    }
                    calibration.job.progress_percent = (60.0
                        + ((THERMAL_PLANT_TARGET_TEMP_C - latest_temp_c)
                            / (THERMAL_PLANT_TARGET_TEMP_C - THERMAL_PLANT_COOL_COMPLETE_TEMP_C))
                            .clamp(0.0, 1.0)
                            * 39.0) as u8;
                    if thermal_plant_cooling_complete(latest_temp_c, recorded_temp_c) {
                        if !thermal_plant_curve_samples_ready(&job.heater_curve) {
                            calibration_job_fail(
                                calibration,
                                ManualPpsError::HeaterCurveCoverageInsufficient,
                                true,
                                manual_pps,
                            );
                            return;
                        }
                        let Some(curve) = heater_curve_from_transient_bins(&job.heater_curve.bins)
                        else {
                            calibration_job_fail(
                                calibration,
                                ManualPpsError::HeaterCurveCoverageInsufficient,
                                true,
                                manual_pps,
                            );
                            return;
                        };
                        let Some(raw_observations) =
                            heater_curve_raw_observations_from_transient_bins(
                                job.heater_curve.cold_bin,
                                &job.heater_curve.bins,
                            )
                        else {
                            calibration_job_fail(
                                calibration,
                                ManualPpsError::HeaterCurveCoverageInsufficient,
                                true,
                                manual_pps,
                            );
                            return;
                        };
                        let transaction_id = (u32::from(job.ambient_raw_rtd_adc_mv) << 16)
                            ^ u32::from(latest_rtd_raw_adc_mv)
                            ^ job.elapsed_ticks
                            ^ 0x5452_4e53;
                        let fit_result = fit_thermal_plant_transient(
                            transaction_id,
                            job.ambient_raw_rtd_adc_mv,
                            &job.samples,
                            job.sample_count,
                            Some(&curve),
                            memory_config,
                        );
                        let Some((transaction, _residual)) = fit_result else {
                            calibration_job_fail(
                                calibration,
                                ManualPpsError::ThermalPlantProjectionInvalid,
                                true,
                                manual_pps,
                            );
                            return;
                        };
                        memory_config.active_heater_curve = curve;
                        memory_config.heater_curve_raw_observations = raw_observations;
                        memory_config.heater_curve_transaction_id =
                            Some(transaction.transaction_id);
                        memory_config.thermal_plant_transient_active = Some(transaction);
                        memory_config.thermal_plant_active = None;
                        memory_config.sanitize();
                        manual_pps.clear();
                        calibration.pps_enabled = false;
                        calibration.pps_mv = None;
                        calibration.pps_ma = None;
                        calibration.immediate_heater_disarm_pending = true;
                        calibration.mode = CalibrationMode::Off;
                        calibration.thermal_plant_completion_disarm_pending = true;
                        calibration.job.samples_collected = job.sample_count;
                        calibration_job_complete(
                            calibration,
                            CalibrationJobKind::ThermalPlant,
                            job.sample_count,
                            None,
                        );
                        return;
                    }
                }
            }
            calibration.job.samples_collected = job.sample_count;
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[allow(clippy::too_many_arguments)]
fn update_calibration_job_state(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_temp_c: f32,
    pd_current_ma: u16,
    latest_vin_mv: u32,
    heater_duty_percent: u8,
) {
    update_calibration_job_state_with_workspace(
        calibration,
        memory_config,
        manual_pps,
        thermal_plant_workspace,
        latest_rtd_raw_adc_mv,
        latest_vin_raw_adc_mv,
        latest_temp_c,
        pd_current_ma,
        latest_vin_mv,
        heater_duty_percent,
    );
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn update_calibration_job_state(
    calibration: &mut CalibrationRuntimeState,
    memory_config: &mut MemoryConfig,
    manual_pps: &mut ManualPpsState,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_temp_c: f32,
    pd_current_ma: u16,
    latest_vin_mv: u32,
    heater_duty_percent: u8,
) {
    TEST_THERMAL_PLANT_WORKSPACE.with(|workspace| {
        update_calibration_job_state_with_workspace(
            calibration,
            memory_config,
            manual_pps,
            &mut workspace.borrow_mut(),
            latest_rtd_raw_adc_mv,
            latest_vin_raw_adc_mv,
            latest_temp_c,
            pd_current_ma,
            latest_vin_mv,
            heater_duty_percent,
        );
    });
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
fn usb_calibration_config_response(
    request_id: heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    config: CalibrationConfigCommand,
    memory_config: &mut MemoryConfig,
    latest_rtd_raw_adc_mv: u16,
    latest_vin_raw_adc_mv: u16,
) -> UsbFrame {
    let previous_adc_calibration = memory_config.adc_calibration.clone();
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
    if memory_config.adc_calibration != previous_adc_calibration {
        rebuild_transient_thermal_plant_for_current_inputs(memory_config);
    }
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
    preview_heater_curve: &mut Option<HeaterCurvePreview>,
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
            let raw_observations = package.raw_observations_to_memory();
            let mut curve = package.to_memory();
            curve.points.sort_unstable_by_key(|point| {
                point.map(|point| point.temp_centi_c).unwrap_or(i16::MAX)
            });
            *preview_heater_curve = Some(HeaterCurvePreview {
                curve,
                raw_observations,
            });
        }
        HeaterCurveConfigOp::ClearPreview => {
            *preview_heater_curve = None;
        }
    }
    usb_response(
        request_id,
        UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
            memory_config,
            preview_heater_curve
                .as_ref()
                .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
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
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
) -> UsbFrame {
    match command.op {
        CalibrationJobOpWire::Cancel => {
            calibration_job_canceled(calibration, manual_pps);
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration).job,
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
            if let Err(error) = calibration_job_start(
                calibration,
                kind,
                memory_config,
                manual_pps,
                thermal_plant_workspace,
            ) {
                return usb_error_response(request_id, error.code(), error.message());
            }
            usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration).job,
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
        let _ = tx.write_response_bytes(line.as_bytes());
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

    fn wait_for_tx_progress(&mut self) {}

    fn write_response_bytes(&mut self, bytes: &[u8]) -> bool
    where
        Self: Sized,
    {
        usb_write_bytes_bounded(self, bytes)
    }
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

    fn wait_for_tx_progress(&mut self) {
        // USB Serial/JTAG advances independently of this polling loop. A tight
        // retry can exhaust its budget before the endpoint observes WR_DONE,
        // which would silently drop host-requested JSONL responses.
        esp_hal::rom::ets_delay_us(USB_CONTROL_TX_BACKOFF_US);
    }

    fn write_response_bytes(&mut self, bytes: &[u8]) -> bool {
        self.write_response_bytes(bytes)
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
            Err(UsbTxError::WouldBlock) => tx.wait_for_tx_progress(),
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
fn hardware_identity() -> Identity {
    #[cfg(target_arch = "xtensa")]
    {
        Identity::firmware_from_mac(esp_hal::efuse::Efuse::mac_address())
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        Identity::firmware_from_mac([0xa0, 0xf2, 0x62, 0xf2, 0x0d, 0x6c])
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
                UsbResponsePayload::Identity(Box::new(hardware_identity())),
            ),
            UsbRequestOp::GetInstallStatus => usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "Install status is unavailable until EEPROM restoration completes.",
                true,
            ),
            UsbRequestOp::CompleteSetup | UsbRequestOp::ResetPersistence => {
                usb_error_response_with_retryable(
                    request_id,
                    "startup_busy",
                    "Persistence changes are unavailable until EEPROM restoration completes.",
                    true,
                )
            }
            // The boot-time memory argument is still the zero-value placeholder
            // until the main loop has completed EEPROM restoration. Never
            // expose it as a network snapshot: a configured device would appear
            // transiently disabled or connected with no address and the host
            // could persist that false state. The devd read path retries this
            // explicit startup boundary until the runtime owns the snapshot.
            UsbRequestOp::GetNetwork => usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "Network status is not available until EEPROM and WiFi initialization completes.",
                true,
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
            UsbRequestOp::GetLanPairingCode => usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "LAN pairing code is not available until runtime initialization completes.",
                true,
            ),
            UsbRequestOp::OpenLanPairingWindow | UsbRequestOp::CloseLanPairingWindow => {
                usb_error_response_with_retryable(
                    request_id,
                    "startup_busy",
                    "LAN pairing window is not available until runtime initialization completes.",
                    true,
                )
            }
            UsbRequestOp::ClearLanPairingToken => usb_error_response_with_retryable(
                request_id,
                "startup_busy",
                "LAN pairing reset is not available until runtime initialization completes.",
                true,
            ),
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
        Ok(UsbFrame::ThermalPlantRun { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "startup_busy",
            "Thermal-model run snapshots are not available until hardware initialization completes.",
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
    status_light_state: StatusLightState,
    phase: UsbRecoveryPhase,
) -> ! {
    set_status_light_state(status_light_state);
    let mut elapsed_ms = 0_u64;
    loop {
        loop {
            match usb.read_byte() {
                Ok(b'\n') => {
                    let response = usb_recovery_response_for_phase(
                        rx_line.as_str(),
                        memory_config,
                        elapsed_ms,
                        phase,
                    );
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UsbRecoveryPhase {
    BeforePersistentState,
    RuntimeFault,
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_recovery_response_for_phase(
    line: &str,
    memory_config: &MemoryConfig,
    elapsed_ms: u64,
    phase: UsbRecoveryPhase,
) -> UsbFrame {
    match phase {
        // Persistence restoration and Wi-Fi initialization have not happened,
        // so only the early USB contract may answer. In particular, it keeps
        // network and runtime status behind the retryable startup boundary.
        UsbRecoveryPhase::BeforePersistentState => usb_early_response(line, memory_config),
        UsbRecoveryPhase::RuntimeFault => usb_recovery_response(line, memory_config, elapsed_ms),
    }
}

#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
fn usb_recovery_status(memory_config: &MemoryConfig, elapsed_ms: u64) -> Box<ControlPlaneStatus> {
    let mut status = ControlPlaneStatus::boxed_from_device_status(
        DeviceStatus {
            mode: DeviceMode::Fault,
            voltage_mv: 0,
            current_ma: 0,
            board_temp_centi: -100,
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
                UsbResponsePayload::Identity(Box::new(hardware_identity())),
            ),
            UsbRequestOp::GetInstallStatus => usb_response(
                request_id,
                UsbResponsePayload::InstallStatus(InstallStatus::from_runtime(
                    memory_config,
                    "defaults",
                    "incompatible",
                    0,
                    false,
                    true,
                )),
            ),
            UsbRequestOp::CompleteSetup | UsbRequestOp::ResetPersistence => usb_error_response(
                request_id,
                "hardware_bringup_failed",
                "Persistence changes are unavailable because hardware bring-up failed.",
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
            UsbRequestOp::GetLanPairingCode => usb_error_response_with_retryable(
                request_id,
                "hardware_bringup_failed",
                "LAN pairing code is unavailable because hardware bring-up did not complete.",
                true,
            ),
            UsbRequestOp::OpenLanPairingWindow | UsbRequestOp::CloseLanPairingWindow => {
                usb_error_response_with_retryable(
                    request_id,
                    "hardware_bringup_failed",
                    "LAN pairing window is unavailable because hardware bring-up did not complete.",
                    true,
                )
            }
            UsbRequestOp::ClearLanPairingToken => usb_error_response_with_retryable(
                request_id,
                "hardware_bringup_failed",
                "LAN pairing reset is unavailable because hardware bring-up did not complete.",
                true,
            ),
        },
        Ok(UsbFrame::WifiConfig { request_id, .. })
        | Ok(UsbFrame::RuntimeConfig { request_id, .. })
        | Ok(UsbFrame::CalibrationConfig { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "Runtime writes are unavailable because hardware bring-up did not complete.",
            true,
        ),
        Ok(UsbFrame::ThermalPlantRun { request_id, .. }) => usb_error_response_with_retryable(
            request_id,
            "hardware_bringup_failed",
            "Thermal-model run snapshots are unavailable because hardware bring-up did not complete.",
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
async fn process_control_line(
    line: &str,
    controller: &mut FrontPanelInputController,
    ui_state: &mut FrontPanelUiState,
    memory_config: &mut MemoryConfig,
    preview_heater_curve: &mut Option<HeaterCurvePreview>,
    memory_commit_due_ms: &mut Option<u64>,
    memory_sequence: &mut u32,
    persistence_source: &'static str,
    persistence_record_state: &'static str,
    pd_i2c: &mut I2c<'_, esp_hal::Blocking>,
    pd_controller: ControllerKind,
    pd_port: &mut PdPort,
    calibration_runtime_state: &mut CalibrationRuntimeState,
    thermal_plant_workspace: &mut CalibrationThermalPlantWorkspace,
    elapsed_ms: u64,
    last_pd_observation: Option<PdStatusObservation>,
    heater_power_backend: &mut HeaterPowerBackend,
    heater_controller: &mut HeaterController,
    pid_snapshot: HeaterPidSnapshot,
    manual_pps: &mut ManualPpsState,
    fan_command: FanHardwareCommand,
    current_rtd_fault: Option<HeaterFaultReason>,
    overtemp_attention_acknowledged: &mut bool,
    attention_pending_after_fault_clear: &mut bool,
    overtemp_forced_fan_active: &mut bool,
    next_attention_reminder_ms: &mut Option<u64>,
    buzzer: &mut BuzzerRuntime,
    thermal_control_profile_preview: &mut Option<ThermalControlProfile>,
    last_raw_state: FrontPanelRawState,
    latest_status_temp_c: f32,
    latest_control_temp_c: f32,
    control_measurement_guarded: bool,
    latest_rtd_raw_adc_mv: u16,
    latest_rtd_raw_adc_min_mv: u16,
    latest_rtd_raw_adc_max_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_vin_mv: u32,
    last_heater_duty: u8,
    heater_control_timing: HeaterControlTiming,
) -> (bool, UsbFrame) {
    let mut needs_redraw = false;
    let active_thermal_control_profile =
        active_thermal_control_profile(memory_config, *thermal_control_profile_preview, manual_pps);
    let runtime_context =
        |heater_fault_latched: Option<HeaterFaultReason>,
         attention_pending_after_fault_clear_value: bool| UsbRuntimeStatusContext {
            elapsed_ms,
            pd_controller,
            last_pd_observation,
            heater_power_backend: *heater_power_backend,
            pid_snapshot,
            heater_control_timing,
            heater_physical_output_percent: last_heater_duty,
            manual_pps: *manual_pps,
            fan_command,
            current_rtd_fault,
            heater_fault_latched,
            attention_pending_after_fault_clear: attention_pending_after_fault_clear_value,
            thermal_control_profile_preview: thermal_control_profile_preview.is_some(),
            active_thermal_control_profile,
            last_raw_state,
            latest_status_temp_c,
            latest_control_temp_c,
            control_measurement_guarded,
            latest_rtd_raw_adc_mv,
            latest_rtd_raw_adc_min_mv,
            latest_rtd_raw_adc_max_mv,
            latest_vin_raw_adc_mv,
            vin_mv: latest_vin_mv,
        };
    #[cfg(feature = "net_http")]
    if ui_state.apply_network_summary(flux_purr_firmware::net::lan_network_summary().await) {
        // Status and network requests must observe the same device-owned
        // snapshot even during the first control-loop ticks after boot.
        needs_redraw = true;
    }
    let response = match parse_usb_frame(line) {
        Ok(UsbFrame::Request { request_id, op }) => match op {
            UsbRequestOp::GetIdentity => usb_response(
                request_id,
                UsbResponsePayload::Identity(Box::new(hardware_identity())),
            ),
            UsbRequestOp::GetInstallStatus => usb_response(
                request_id,
                UsbResponsePayload::InstallStatus(InstallStatus::from_runtime(
                    memory_config,
                    persistence_source,
                    persistence_record_state,
                    *memory_sequence,
                    current_rtd_fault.is_none() && latest_status_temp_c.is_finite(),
                    heater_controller.fault_latched().is_some(),
                )),
            ),
            UsbRequestOp::CompleteSetup => {
                if ui_state.persistence_locked() {
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; setup cannot be completed.",
                    )
                } else {
                    let sensor_ready =
                        current_rtd_fault.is_none() && latest_status_temp_c.is_finite();
                    let calibration_ready = flux_purr_firmware::memory::adc_calibration_fit(
                        &memory_config.adc_calibration,
                        flux_purr_firmware::memory::AdcCalibrationChannel::Rtd,
                    )
                    .sample_count
                        >= 2
                        && flux_purr_firmware::memory::adc_calibration_fit(
                            &memory_config.adc_calibration,
                            flux_purr_firmware::memory::AdcCalibrationChannel::Vin,
                        )
                        .sample_count
                            >= 2
                        && memory_config
                            .active_heater_curve
                            .points
                            .iter()
                            .flatten()
                            .count()
                            >= 2;
                    match memory_config.complete_setup(sensor_ready, calibration_ready) {
                        Ok(()) => {
                            *memory_commit_due_ms =
                                Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                            usb_response(request_id, UsbResponsePayload::Ack)
                        }
                        Err(flux_purr_firmware::memory::SetupCompletionError::SensorNotReady) => {
                            usb_error_response(
                                request_id,
                                "sensor_unready",
                                "Sensor readiness is required before setup completion.",
                            )
                        }
                        Err(
                            flux_purr_firmware::memory::SetupCompletionError::CalibrationRequired,
                        ) => usb_error_response(
                            request_id,
                            "calibration_required",
                            "Calibration is required before setup completion.",
                        ),
                    }
                }
            }
            UsbRequestOp::ResetPersistence => {
                if ui_state.persistence_locked() {
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; persistence cannot be reset.",
                    )
                } else {
                    memory_config.reset_for_commissioning();
                    apply_memory_config_to_ui(ui_state, memory_config);
                    *memory_commit_due_ms =
                        Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                    needs_redraw = true;
                    usb_response(request_id, UsbResponsePayload::Ack)
                }
            }
            UsbRequestOp::GetNetwork => {
                #[cfg(feature = "net_http")]
                let network = flux_purr_firmware::net::lan_network_summary().await;
                #[cfg(not(feature = "net_http"))]
                let network = network_from_memory(memory_config);
                usb_response(request_id, UsbResponsePayload::Network(network))
            }
            UsbRequestOp::GetStatus => usb_response(
                request_id,
                UsbResponsePayload::Status(usb_runtime_status(
                    ui_state,
                    memory_config,
                    calibration_runtime_state,
                    runtime_context(
                        heater_controller.fault_latched(),
                        *attention_pending_after_fault_clear,
                    ),
                )),
            ),
            UsbRequestOp::GetCalibration => usb_response(
                request_id,
                UsbResponsePayload::Calibration(calibration_state_from_memory(memory_config)),
            ),
            UsbRequestOp::GetCalibrationJob => usb_response(
                request_id,
                UsbResponsePayload::CalibrationJob(
                    calibration_runtime_state_to_wire(calibration_runtime_state).job,
                ),
            ),
            UsbRequestOp::GetHeaterCurve => usb_response(
                request_id,
                UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                    memory_config,
                    preview_heater_curve
                        .as_ref()
                        .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
                )),
            ),
            UsbRequestOp::SetLogLevel => usb_response(request_id, UsbResponsePayload::Ack),
            UsbRequestOp::GetLanPairingCode => {
                #[cfg(feature = "net_http")]
                {
                    let code = flux_purr_firmware::net::pairing_code();
                    usb_response(
                        request_id,
                        UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
                    )
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::OpenLanPairingWindow => {
                #[cfg(feature = "net_http")]
                {
                    let code = flux_purr_firmware::net::enter_pairing().await;
                    ui_state.enter_wifi_pairing(code);
                    needs_redraw = true;
                    usb_response(
                        request_id,
                        UsbResponsePayload::LanPairingCode(lan_pairing_code_payload(code)),
                    )
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::CloseLanPairingWindow => {
                #[cfg(feature = "net_http")]
                {
                    flux_purr_firmware::net::leave_pairing().await;
                    ui_state.leave_wifi_pairing();
                    ui_state.route = FrontPanelRoute::Dashboard;
                    needs_redraw = true;
                    usb_response(request_id, UsbResponsePayload::Ack)
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
            UsbRequestOp::ClearLanPairingToken => {
                #[cfg(feature = "net_http")]
                {
                    if ui_state.persistence_locked() {
                        usb_error_response(
                            request_id,
                            "eeprom_required",
                            "EEPROM_REQUIRED: persistent configuration is unavailable; LAN pairing token cannot be cleared.",
                        )
                    } else {
                        flux_purr_firmware::net::clear_token_from_usb().await;
                        memory_config.lan_pairing_token = None;
                        *memory_commit_due_ms =
                            Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                        info!("LAN pairing token cleared by USB control request");
                        usb_response(request_id, UsbResponsePayload::Ack)
                    }
                }
                #[cfg(not(feature = "net_http"))]
                {
                    usb_error_response(
                        request_id,
                        "lan_unavailable",
                        "LAN pairing is disabled in this firmware build.",
                    )
                }
            }
        },
        Ok(UsbFrame::WifiConfig { request_id, config }) => {
            if ui_state.persistence_locked() && !matches!(config.op, WifiConfigOp::Cancel) {
                return (
                    needs_redraw,
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; Wi-Fi persistence is locked.",
                    ),
                );
            }
            #[cfg(feature = "net_http")]
            let network = match config.op {
                WifiConfigOp::Cancel => {
                    match flux_purr_firmware::net::cancel_wifi_connection().await {
                        Ok(network) => network,
                        Err(error) => {
                            return (
                                needs_redraw,
                                usb_error_response(request_id, error.code(), error.message()),
                            );
                        }
                    }
                }
                WifiConfigOp::Set | WifiConfigOp::Clear => {
                    config.apply_to(memory_config);
                    flux_purr_firmware::net::apply_wifi_config(memory_config).await
                }
            };
            #[cfg(not(feature = "net_http"))]
            let network = match config.op {
                WifiConfigOp::Cancel => {
                    return (
                        needs_redraw,
                        usb_error_response(
                            request_id,
                            "wifi_cancel_unavailable",
                            "WiFi cancellation is unavailable in this firmware build.",
                        ),
                    );
                }
                WifiConfigOp::Set | WifiConfigOp::Clear => {
                    config.apply_to(memory_config);
                    network_from_memory(memory_config)
                }
            };
            if !matches!(config.op, WifiConfigOp::Cancel) {
                apply_memory_config_to_ui(ui_state, memory_config);
                *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
                needs_redraw = true;
            }
            usb_response(
                request_id,
                UsbResponsePayload::Wifi(WifiConfigReceipt {
                    wifi: config.redacted_summary(),
                    network,
                }),
            )
        }
        Ok(UsbFrame::RuntimeConfig {
            request_id,
            mut config,
        }) => {
            if ui_state.persistence_locked()
                && (config.heater_enabled == Some(true)
                    || config.manual_pps_enabled == Some(true)
                    || config.calibration.is_some())
            {
                return (
                    needs_redraw,
                    usb_error_response(
                        request_id,
                        "eeprom_required",
                        "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                    ),
                );
            }
            let previous_memory_config = memory_config.clone();
            let heater_toggle_requested = config.heater_enabled.is_some();
            let heater_rearm_requested = config.heater_enabled == Some(true);
            let overtemp_active = is_overtemp_fault(current_rtd_fault);
            if config.fault_attention_acknowledged == Some(true)
                && acknowledge_overtemp_attention(
                    overtemp_active,
                    overtemp_attention_acknowledged,
                    attention_pending_after_fault_clear,
                    overtemp_forced_fan_active,
                    next_attention_reminder_ms,
                    buzzer,
                )
            {
                info!("overtemp attention acknowledged");
            }
            if heater_rearm_requested && (overtemp_active || *attention_pending_after_fault_clear) {
                config.heater_enabled = Some(false);
                buzzer.request_feedback(
                    BuzzerCueSource::RuntimeControl,
                    BuzzerCueId::HeaterReject,
                    elapsed_ms,
                );
                info!("heater runtime arm rejected by overtemp attention state");
            }
            if should_clear_runtime_fault_latch(
                heater_rearm_requested,
                current_rtd_fault,
                heater_controller.fault_latched(),
            ) {
                heater_controller.clear_fault_latch();
                info!("heater runtime re-arm -> cleared latched fault");
            }
            let response = usb_runtime_config_response(
                request_id,
                config,
                ui_state,
                memory_config,
                manual_pps,
                thermal_control_profile_preview,
                calibration_runtime_state,
                runtime_context(
                    heater_controller.fault_latched(),
                    *attention_pending_after_fault_clear,
                ),
            );
            if heater_toggle_requested {
                controller.clear_pending_short_press(RawFrontPanelKey::CenterBoot);
            }
            if *memory_config != previous_memory_config {
                *memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
            }
            needs_redraw = true;
            response
        }
        #[cfg(feature = "buzzer-test")]
        Ok(UsbFrame::BuzzerTest {
            request_id,
            command,
        }) => {
            if !command.is_valid() {
                usb_error_response(
                    request_id,
                    "invalid_buzzer_test_command",
                    "buzzer_test requires exactly the fields for its operation.",
                )
            } else if command.op != BuzzerTestOp::Status
                && (ui_state.heater_enabled
                    || current_rtd_fault.is_some()
                    || heater_controller.fault_latched().is_some()
                    || *attention_pending_after_fault_clear)
            {
                usb_error_response(
                    request_id,
                    "buzzer_test_interlocked",
                    "Buzzer test requires heater-off with no active or pending thermal fault.",
                )
            } else {
                match command.op {
                    BuzzerTestOp::Status => UsbFrame::BuzzerTestResponse {
                        request_id,
                        status: Box::new(buzzer_test_status()),
                    },
                    BuzzerTestOp::Trigger => {
                        let status = buzzer_test_status();
                        if status.state == BuzzerTestSessionState::Running {
                            usb_error_response(
                                request_id,
                                "buzzer_test_busy",
                                "A buzzer test scenario is already running.",
                            )
                        } else {
                            BuzzerRuntime::submit_test(command);
                            UsbFrame::BuzzerTestResponse {
                                request_id,
                                status: Box::new(status),
                            }
                        }
                    }
                    BuzzerTestOp::Run => {
                        let status = buzzer_test_status();
                        if status.state != BuzzerTestSessionState::Running {
                            BuzzerRuntime::submit_test(command);
                            UsbFrame::BuzzerTestResponse {
                                request_id,
                                status: Box::new(status),
                            }
                        } else {
                            usb_error_response(
                                request_id,
                                "buzzer_test_busy",
                                "A buzzer test scenario is already running.",
                            )
                        }
                    }
                    BuzzerTestOp::Stop => {
                        BuzzerRuntime::submit_test(command);
                        UsbFrame::BuzzerTestResponse {
                            request_id,
                            status: Box::new(buzzer_test_status()),
                        }
                    }
                }
            }
        }
        Ok(UsbFrame::CalibrationConfig { request_id, config }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if thermal_plant_calibration_job_running(*calibration_runtime_state) {
                usb_error_response(
                    request_id,
                    ManualPpsError::CalibrationInProgress.code(),
                    "Automatic thermal-model calibration is running; calibration inputs are locked.",
                )
            } else {
                let previous_memory_config = memory_config.clone();
                let response = usb_calibration_config_response(
                    request_id.clone(),
                    config,
                    memory_config,
                    latest_rtd_raw_adc_mv,
                    latest_vin_raw_adc_mv,
                );
                if previous_memory_config
                    .thermal_plant_transient_active
                    .is_some()
                    && previous_memory_config.adc_calibration != memory_config.adc_calibration
                {
                    disarm_calibration_after_transient_input_change(
                        calibration_runtime_state,
                        manual_pps,
                    );
                }
                if *memory_config != previous_memory_config {
                    if commit_memory_config_now(
                        pd_i2c,
                        pd_port,
                        elapsed_ms,
                        memory_sequence,
                        memory_config,
                    )
                    .await
                    .is_ok()
                    {
                        *memory_commit_due_ms = None;
                    } else {
                        *memory_config = previous_memory_config;
                        mark_eeprom_required(
                            ui_state,
                            calibration_runtime_state,
                            manual_pps,
                            memory_commit_due_ms,
                        );
                        return (
                            needs_redraw,
                            usb_error_response(
                                request_id,
                                "memory_commit_failed",
                                "Calibration draft could not be persisted.",
                            ),
                        );
                    }
                }
                response
            }
        }
        Ok(UsbFrame::CalibrationJob {
            request_id,
            command,
        }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if matches!(command.op, CalibrationJobOpWire::Start)
                && !pd_contract_allows_calibration(pd_controller, last_pd_observation)
            {
                let (code, message) = (
                    "pd_performance_not_guaranteed",
                    "Calibration requires a performance-guaranteed PPS contract.",
                );
                usb_error_response(request_id, code, message)
            } else {
                usb_calibration_job_response(
                    request_id,
                    command,
                    calibration_runtime_state,
                    memory_config,
                    manual_pps,
                    thermal_plant_workspace,
                )
            }
        }
        Ok(UsbFrame::ThermalPlantRun {
            request_id,
            after_sample,
        }) => usb_response(
            request_id,
            UsbResponsePayload::ThermalPlantRun(thermal_plant_run_snapshot_wire(
                calibration_runtime_state,
                memory_config,
                thermal_plant_workspace,
                after_sample,
                latest_status_temp_c,
                latest_vin_mv,
                last_heater_duty,
            )),
        ),
        Ok(UsbFrame::HeaterCurveConfig { request_id, config }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else {
                usb_heater_curve_config_response(
                    request_id,
                    config,
                    memory_config,
                    preview_heater_curve,
                )
            }
        }
        Ok(UsbFrame::HeaterCurveSave { request_id }) => {
            if ui_state.persistence_locked() {
                usb_error_response(
                    request_id,
                    "eeprom_required",
                    "EEPROM_REQUIRED: persistent configuration is unavailable; heating and calibration are locked.",
                )
            } else if thermal_plant_calibration_job_running(*calibration_runtime_state) {
                usb_error_response(
                    request_id,
                    ManualPpsError::CalibrationInProgress.code(),
                    "Automatic thermal-model calibration is running; calibration inputs are locked.",
                )
            } else if let Some(preview) = *preview_heater_curve {
                let previous_memory_config = memory_config.clone();
                memory_config.active_heater_curve = preview.curve;
                if let Some(raw_observations) = preview.raw_observations {
                    memory_config.heater_curve_raw_observations = raw_observations;
                }
                memory_config.sanitize();
                let raw_observations_changed = previous_memory_config.heater_curve_raw_observations
                    != memory_config.heater_curve_raw_observations;
                if raw_observations_changed
                    && previous_memory_config
                        .thermal_plant_transient_active
                        .is_some()
                {
                    memory_config.heater_curve_transaction_id = None;
                    invalidate_transient_thermal_plant(memory_config);
                    disarm_calibration_after_transient_input_change(
                        calibration_runtime_state,
                        manual_pps,
                    );
                }
                if let Err(error) = commit_memory_config_now(
                    pd_i2c,
                    pd_port,
                    elapsed_ms,
                    memory_sequence,
                    memory_config,
                )
                .await
                {
                    *memory_config = previous_memory_config;
                    mark_eeprom_required(
                        ui_state,
                        calibration_runtime_state,
                        manual_pps,
                        memory_commit_due_ms,
                    );
                    return (
                        needs_redraw,
                        usb_error_response(request_id, error.code(), error.message()),
                    );
                }
                *memory_commit_due_ms = None;
                usb_response(
                    request_id,
                    UsbResponsePayload::HeaterCurve(heater_curve_state_from_memory(
                        memory_config,
                        preview_heater_curve
                            .as_ref()
                            .map(|preview| (&preview.curve, preview.raw_observations.as_ref())),
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
        Ok(UsbFrame::EepromMaintenance {
            request_id,
            command,
        }) => {
            if last_heater_duty != 0 {
                return (
                    false,
                    usb_error_response(
                        request_id,
                        "heater_output_active",
                        "EEPROM maintenance requires physical heater output to be off.",
                    ),
                );
            }
            let op = command.op;
            if raw_eeprom_operation_mutates(op) {
                begin_mutating_eeprom_maintenance(
                    ui_state,
                    calibration_runtime_state,
                    manual_pps,
                    memory_commit_due_ms,
                );
                if !request_pd_fixed_voltage(pd_i2c, pd_port, DEFAULT_PD_VOLTAGE_REQUEST).await {
                    return (
                        needs_redraw,
                        usb_error_response(
                            request_id,
                            "eeprom_power_disarm_failed",
                            "EEPROM maintenance could not restore fixed PD.",
                        ),
                    );
                }
            } else {
                ui_state.heater_enabled = false;
                calibration_job_canceled(calibration_runtime_state, manual_pps);
            }
            needs_redraw = true;
            let response = usb_eeprom_maintenance_response(request_id, command, pd_i2c).await;
            if matches!(&response, UsbFrame::Response { ok: true, .. }) {
                apply_successful_eeprom_maintenance_operation(
                    op,
                    ui_state,
                    memory_config,
                    memory_commit_due_ms,
                );
                if matches!(op, EepromMaintenanceOp::Erase) {
                    *preview_heater_curve = None;
                    *thermal_control_profile_preview = None;
                }
            } else if eeprom_storage_failure_response(&response) {
                ui_state.eeprom_required = true;
            }
            response
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

    (needs_redraw, response)
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_pairing_code_payload(code: Option<[u8; 4]>) -> LanPairingCode {
    let code = code.map(|digits| {
        let mut rendered = heapless::String::new();
        for digit in digits {
            let _ = rendered.push(digit as char);
        }
        rendered
    });
    LanPairingCode {
        active: code.is_some(),
        code,
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_command_to_control_line(
    command: &ControlMailboxCommand,
) -> Result<heapless::String<USB_CONTROL_LINE_CAPACITY>, &'static str> {
    let request_op = match (command.endpoint, command.method) {
        (LanEndpoint::Identity, HttpMethod::Get) => Some("get_identity"),
        (LanEndpoint::Network, HttpMethod::Get) => Some("get_network"),
        (LanEndpoint::Status | LanEndpoint::Events, HttpMethod::Get) => Some("get_status"),
        (LanEndpoint::Calibration, HttpMethod::Get) => Some("get_calibration"),
        (LanEndpoint::CalibrationJob, HttpMethod::Get) => Some("get_calibration_job"),
        (LanEndpoint::HeaterCurve, HttpMethod::Get) => Some("get_heater_curve"),
        _ => None,
    };
    let mut line = heapless::String::new();
    if let Some(op) = request_op {
        write!(
            line,
            r#"{{"type":"request","requestId":"lan","op":"{op}"}}"#
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    if command.endpoint == LanEndpoint::ThermalPlantRun && command.method == HttpMethod::Get {
        write!(
            line,
            r#"{{"type":"thermal_plant_run","requestId":"lan","afterSample":{}}}"#,
            command.after_sample.unwrap_or(0)
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    // Saving the current heater-curve preview has no payload in the shared
    // USB JSONL contract. Do not force it through the JSON-object adapter.
    if command.endpoint == LanEndpoint::HeaterCurveSave {
        write!(line, r#"{{"type":"heater_curve_save","requestId":"lan"}}"#)
            .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    // The LAN API keeps a thermal-profile payload focused on the profile
    // operation itself. USB JSONL carries the same operation under runtime
    // config's `thermalControlProfile` field.
    if command.endpoint == LanEndpoint::ThermalProfile {
        let body = command.body.as_str().trim();
        if !body.starts_with('{') || !body.ends_with('}') {
            return Err("LAN command body must be a JSON object");
        }
        write!(
            line,
            r#"{{"type":"runtime_config","requestId":"lan","thermalControlProfile":{body}}}"#
        )
        .map_err(|_| "LAN request is too large")?;
        return Ok(line);
    }

    let frame_type = match command.endpoint {
        LanEndpoint::Runtime => "runtime_config",
        LanEndpoint::Calibration => "calibration_config",
        LanEndpoint::CalibrationJob => "calibration_job",
        LanEndpoint::HeaterCurve => "heater_curve_config",
        _ => return Err("LAN endpoint does not accept this method"),
    };
    let fields = command
        .body
        .as_str()
        .trim()
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .ok_or("LAN command body must be a JSON object")?
        .trim();
    write!(line, r#"{{"type":"{frame_type}","requestId":"lan""#)
        .map_err(|_| "LAN request is too large")?;
    if !fields.is_empty() {
        line.push(',').map_err(|_| "LAN request is too large")?;
        line.push_str(fields)
            .map_err(|_| "LAN request is too large")?;
    }
    line.push('}').map_err(|_| "LAN request is too large")?;
    Ok(line)
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_error_json(code: &str, message: &str) -> heapless::String<LAN_HTTP_BODY_MAX_LEN> {
    let mut body = heapless::String::new();
    let _ = write!(
        body,
        r#"{{"error":{{"code":"{code}","message":"{message}"}}}}"#
    );
    body
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_frame_response(
    frame: &UsbFrame,
    network: flux_purr_firmware::control_plane::NetworkSummary,
) -> (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>) {
    match frame {
        UsbFrame::Response {
            ok: true,
            result: Some(result),
            ..
        } => match result {
            UsbResponsePayload::Identity(value) => lan_json_response(value),
            UsbResponsePayload::InstallStatus(_) => (
                404,
                lan_error_json(
                    "unsupported_operation",
                    "Install status is available only through USB/devd.",
                ),
            ),
            UsbResponsePayload::Network(value) => lan_json_response(value),
            UsbResponsePayload::Status(value) => {
                let mut status = value.clone();
                status.network = network;
                lan_json_response(&status)
            }
            UsbResponsePayload::LanPairingCode(value) => lan_json_response(value),
            UsbResponsePayload::Wifi(value) => lan_json_response(value),
            UsbResponsePayload::Calibration(value) => lan_json_response(value),
            UsbResponsePayload::CalibrationJob(value) => lan_json_response(value),
            UsbResponsePayload::ThermalPlantRun(value) => lan_json_response(value),
            UsbResponsePayload::HeaterCurve(value) => lan_json_response(value),
            UsbResponsePayload::EepromBytes(_) => (
                404,
                lan_error_json(
                    "unsupported_operation",
                    "EEPROM maintenance is available only through USB/devd.",
                ),
            ),
            UsbResponsePayload::Ack => {
                let mut body = heapless::String::new();
                let _ = body.push_str(r#"{"accepted":true}"#);
                (200, body)
            }
        },
        UsbFrame::Response {
            error: Some(error), ..
        }
        | UsbFrame::Error { error, .. } => (
            lan_error_status(error.code.as_str()),
            lan_error_json(error.code.as_str(), error.message.as_str()),
        ),
        _ => (
            500,
            lan_error_json(
                "invalid_control_response",
                "Control loop returned an invalid LAN response.",
            ),
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_json_response<T: Serialize>(value: &T) -> (u16, heapless::String<LAN_HTTP_BODY_MAX_LEN>) {
    let mut buffer = [0u8; LAN_HTTP_BODY_MAX_LEN];
    match serde_json_core::to_slice(value, &mut buffer) {
        Ok(written) => match core::str::from_utf8(&buffer[..written]) {
            Ok(json) => {
                let mut body = heapless::String::new();
                let _ = body.push_str(json);
                (200, body)
            }
            Err(_) => (
                500,
                lan_error_json(
                    "invalid_control_response",
                    "Control response was not valid UTF-8.",
                ),
            ),
        },
        Err(_) => (
            500,
            lan_error_json(
                "response_too_large",
                "Control response exceeded the LAN envelope.",
            ),
        ),
    }
}

#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
fn lan_error_status(code: &str) -> u16 {
    if code.starts_with("invalid_")
        || code.ends_with("_required")
        || matches!(
            code,
            "malformed_json" | "unsupported_frame" | "unsupported_lan_command"
        )
    {
        400
    } else {
        409
    }
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
    display.flush().await?;
    info!(
        "frontpanel presentation committed route={=str}",
        route_label(state.route)
    );
    Ok(())
}

#[cfg(target_arch = "xtensa")]
async fn present_initial_frontpanel_ui<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    state: &FrontPanelUiState,
) -> bool
where
    BUS: embedded_hal_async::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin<Error = DC::Error>,
    BUS::Error: core::fmt::Debug + embedded_hal::spi::Error,
    DC::Error: core::fmt::Debug,
{
    if !matches!(
        with_timeout(DISPLAY_IO_TIMEOUT, flush_ui(display, canvas, state)).await,
        Ok(Ok(()))
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
const DISPLAY_IO_TIMEOUT: Duration = Duration::from_secs(1);

#[cfg(target_arch = "xtensa")]
async fn request_pd_fixed_voltage(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
    request: ch224q::VoltageRequest,
) -> bool {
    match port {
        PdPort::Fusb302b(runtime) => {
            runtime
                .request_fixed_voltage(i2c, request.millivolts(), Instant::now().as_millis())
                .await
        }
        PdPort::Unavailable => false,
    }
}

#[cfg(target_arch = "xtensa")]
async fn request_pd_adjustable_voltage(
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
                    .request_pps_voltage(i2c, request_mv, Instant::now().as_millis())
                    .await
            } else {
                PdContractRequestState::Failed
            }
        }
        PdPort::Unavailable => PdContractRequestState::Failed,
    }
}

#[cfg(target_arch = "xtensa")]
async fn read_pd_status(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    port: &mut PdPort,
    now_ms: u64,
) -> Option<PdStatusObservation> {
    match port {
        PdPort::Fusb302b(runtime) => {
            if !runtime.poll(i2c, now_ms).await {
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
fn read_pd_power_capabilities(
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
async fn run_key_test_runtime<'a, BUS, DC, RST>(
    display: &mut GC9D01<'a, BUS, DC, RST, DisplayTimer>,
    canvas: &mut DisplayCanvas,
    inputs: FrontPanelInputs<'a>,
    status_light_started_ms: u64,
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
    if !matches!(
        with_timeout(DISPLAY_IO_TIMEOUT, flush_ui(display, canvas, &ui_state)).await,
        Ok(Ok(()))
    ) {
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
            if !matches!(
                with_timeout(DISPLAY_IO_TIMEOUT, flush_ui(display, canvas, &ui_state)).await,
                Ok(Ok(()))
            ) {
                warn!("key-test UI refresh failed; entering recovery");
                return Err(());
            }
            log_ui_state(&ui_state);
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    let reset_reason = reset_reason_log_line(esp_hal::system::reset_reason());
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    init_runtime_heap();
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
    let startup_ui_state = FrontPanelUiState::new_startup(runtime_mode);
    #[cfg(feature = "web_serial")]
    let mut usb_serial = RawUsbSerialJtag::new(peripherals.USB_DEVICE);
    #[cfg(feature = "web_serial")]
    let mut usb_rx_line: heapless::String<USB_CONTROL_LINE_CAPACITY> = heapless::String::new();
    #[cfg(feature = "web_serial")]
    let usb_tx_buf = initialize_usb_control_response_buffer();
    #[cfg(feature = "web_serial")]
    let usb_boot_memory_config = MemoryConfig::default();
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(hardware_identity()),
        usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, reset_reason.as_bytes());
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
        &usb_boot_memory_config,
    );
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

    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
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
    .with_mosi(peripherals.GPIO11);

    let cs = Output::new(peripherals.GPIO15, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO10, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    let mut backlight = Output::new(peripherals.GPIO13, Level::High, OutputConfig::default());
    backlight.set_low();
    info!("backlight active-low: gpio13 low -> on");

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
    let display_ready = matches!(
        with_timeout(DISPLAY_IO_TIMEOUT, display.init()).await,
        Ok(Ok(()))
    );
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
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_init_complete\n");
    match startup_frontpanel_presentation(runtime_mode) {
        StartupFrontPanelPresentation::Dashboard => render_frontpanel_ui(canvas, &startup_ui_state),
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
    let startup_flush_ready = match with_timeout(DISPLAY_IO_TIMEOUT, display.flush()).await {
        Ok(Ok(())) => true,
        Ok(Err(gc9d01::Error::Bus(_))) => {
            warn!("startup display flush failed: spi bus");
            false
        }
        Ok(Err(gc9d01::Error::Pin(_))) => {
            warn!("startup display flush failed: display pin");
            false
        }
        Err(_) => {
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
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=display_flush_complete\n");
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
        let mut _heater_safe = Output::new(peripherals.GPIO47, Level::Low, OutputConfig::default());
        _heater_safe.set_low();
        let mut _fan_enable_safe =
            Output::new(peripherals.GPIO35, Level::Low, OutputConfig::default());
        _fan_enable_safe.set_low();
        let mut _fan_pwm_safe =
            Output::new(peripherals.GPIO36, Level::Low, OutputConfig::default());
        _fan_pwm_safe.set_low();
        info!("key-test runtime ready: gpio47/gpio35/gpio36 held safe-off without PD/RTD bring-up");
        match run_key_test_runtime(&mut display, canvas, inputs, status_light_started_ms).await {
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
                PdPort::Fusb302b(runtime)
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
    // PD negotiation is serviced by the runtime loop. Do one bounded service
    // turn for already-attached sources, but never hold the Dashboard behind
    // a contract wait.
    let initial_pd_observation =
        read_pd_status(&mut pd_i2c, &mut pd_port, Instant::now().as_millis()).await;
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        &mut usb_serial,
        &mut usb_rx_line,
        usb_tx_buf,
        &usb_boot_memory_config,
    );
    let mut pd_contract_ready = startup_pd_contract_ready(initial_pd_observation);
    if !pd_contract_ready {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_contract_not_ready\n");
        warn!(
            "PD contract was not ready before outputs initialize; continuing with heater interlocked"
        );
    } else {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_contract_ready\n");
    }
    // Keep this allocation owned until Wi-Fi has created its timer objects.
    // Releasing it earlier lets the C driver reinterpret allocator free-list
    // bytes as an uninitialized timer `priv_` pointer.
    let mut boot_memory_io_scratch = try_allocate_memory_io_scratch();
    let (mut eeprom_memory_record, eeprom_data_incompatible, mut eeprom_required) =
        if let Some(scratch) = boot_memory_io_scratch.as_deref_mut() {
            load_eeprom_memory_record(&mut pd_i2c, scratch)
        } else {
            (None, false, true)
        };
    let mut eeprom_restore_pending = eeprom_data_incompatible;
    if !eeprom_required && !eeprom_data_incompatible && eeprom_memory_record.is_none() {
        let initialization_result = if let Some(scratch) = boot_memory_io_scratch.as_deref_mut() {
            let record = MemoryRecord {
                sequence: 1,
                config: MemoryConfig::default(),
            };
            let commit_started_at = Instant::now();
            match write_eeprom_memory_record(
                &mut pd_i2c,
                &mut pd_port,
                0,
                commit_started_at,
                &record,
                scratch,
            )
            .await
            {
                Ok(()) => verify_eeprom_memory_record(
                    &mut pd_i2c,
                    &mut pd_port,
                    0,
                    commit_started_at,
                    &record,
                    scratch,
                )
                .await
                .ok(),
                Err(error) => {
                    warn!(
                        "blank EEPROM initialization failed: {=str}",
                        error.message()
                    );
                    None
                }
            }
        } else {
            None
        };
        if let Some(record) = initialization_result {
            info!("blank EEPROM initialized and verified");
            eeprom_memory_record = Some(record);
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
        "eeprom_required"
    } else if eeprom_memory_record.is_some() {
        "eeprom"
    } else {
        "defaults"
    };
    let mut persistence_record_state = if eeprom_required {
        "eeprom_required"
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
    let mut manual_pps_state = ManualPpsState::from_capabilities(power_data_capabilities);
    let mut calibration_runtime_state = CalibrationRuntimeState::default();
    static THERMAL_PLANT_WORKSPACE: StaticCell<CalibrationThermalPlantWorkspace> =
        StaticCell::new();
    let thermal_plant_workspace =
        THERMAL_PLANT_WORKSPACE.init_with(CalibrationThermalPlantWorkspace::default);
    let mut thermal_control_profile_preview: Option<ThermalControlProfile> = None;
    let mut heater_power_backend = constrain_heater_backend_to_controller(
        pd_port.controller_kind(),
        select_heater_power_backend(
            power_data_capabilities,
            last_pd_observation.map(|status| status.status),
        ),
    );
    let mut hold_pps_governor = HoldPpsGovernor::new();
    let mut last_heater_duty = 0_u8;
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
    let _ = apply_heater_power_output(
        &mut pd_i2c,
        &mut pd_port,
        &mut heater_pwm,
        &mut heater_power_backend,
        &mut hold_pps_governor,
        &mut manual_pps_state,
        last_pd_observation,
        0,
        0.0,
        0,
        false,
        HeaterControlPhase::Warmup,
        0.0,
        0.0,
        0,
        &mut last_heater_duty,
        preview_heater_curve_config(preview_heater_curve.as_ref()),
        &memory_config,
        active_thermal_settings,
        0,
    )
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
    let initial_rtd_sample = read_rtd_sample(
        &mut adc1,
        &mut rtd_adc_pin,
        adc_curve.as_ref(),
        &memory_config,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=initial_rtd_complete\n");
    let mut controller = FrontPanelInputController::new(
        FrontPanelKeyMap::default(),
        FrontPanelInputTimings::default(),
    );
    let mut ui_state = FrontPanelUiState::new_startup(runtime_mode);
    ui_state.eeprom_data_incompatible = eeprom_data_incompatible;
    ui_state.eeprom_required = eeprom_required;
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
    if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) = read_calibrated_vin_mv(
        &mut adc1,
        &mut vin_adc_pin,
        adc_curve.as_ref(),
        &memory_config,
    ) {
        latest_vin_raw_adc_mv = raw_adc_mv;
        latest_vin_mv = vin_mv;
        info!(
            "vin initial raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
            raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
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
    let mut last_fan_command: Option<FanHardwareCommand> = None;
    let mut last_raw_state = FrontPanelRawState::default();
    ui_state.set_raw_state(last_raw_state);
    let mut initial_fan_decision = fan_policy_decision(
        latest_display_temp_i16,
        0,
        ui_state.heater_enabled,
        ui_state.heater_output_percent,
        ui_state.active_cooling_enabled,
        fan_policy_state,
        is_sensor_fault(current_rtd_fault),
    );
    if let Some(state) = overtemp_forced_fan_state(
        latest_display_temp_i16,
        is_overtemp_fault(current_rtd_fault),
    ) {
        let command = state.command(0);
        initial_fan_decision = FanPolicyDecision {
            state,
            command,
            display_state: fan_display_state_for_command(ui_state.active_cooling_enabled, command),
        };
    }
    fan_policy_state = initial_fan_decision.state;
    let mut fan_command = initial_fan_decision.command;
    let _ = sync_frontpanel_runtime_state(
        &mut ui_state,
        initial_fan_decision,
        next_heater_lock_reason(
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
    let initial_frontpanel_ui_ready =
        present_initial_frontpanel_ui(&mut display, canvas, &ui_state).await;
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
    if eeprom_restore_pending {
        // Legacy EEPROM decoding is deliberately outside the pre-RTD path.
        // Yield between slots so USB early-control and the status-light task
        // remain serviceable while the explicit restore lock is visible.
        if let Some(scratch) = boot_memory_io_scratch.as_deref_mut() {
            let (legacy_record, read_failed) =
                load_legacy_eeprom_memory_record(&mut pd_i2c, scratch).await;
            if let Some(record) = legacy_record {
                memory_sequence = record.sequence;
                memory_config = record.config;
                ui_state.eeprom_data_incompatible = false;
                ui_state.eeprom_required = false;
                ui_state.set_dashboard_presentation(
                    flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
                );
                apply_memory_config_to_ui(&mut ui_state, &memory_config);
                active_thermal_settings = ThermalControlProfileSettings::from(
                    memory_config.active_thermal_control_profile.settings,
                );
                persistence_source = "eeprom";
                persistence_record_state = "legacy_restored";
                info!(
                    "legacy memory restore complete seq={=u32}; v5 migration remains deferred",
                    record.sequence,
                );
            } else if read_failed {
                eeprom_required = true;
                ui_state.eeprom_required = true;
                persistence_source = "eeprom_required";
                persistence_record_state = "eeprom_required";
                warn!("legacy memory restore unreadable; keeping heater interlocked");
            }
        }
    }
    // The first Dashboard frame is independent of Wi-Fi readiness. Start the
    // network control plane only after the trusted RTD presentation is on the
    // panel so radio retries cannot delay the owner-facing startup state.
    #[cfg(feature = "net_http")]
    flux_purr_firmware::net::initialize_control_state(memory_config.lan_pairing_token).await;
    #[cfg(all(feature = "net_http", feature = "web_serial"))]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=lan_control_state_ready\n");
    #[cfg(feature = "net_http")]
    {
        if let Err(error) =
            flux_purr_firmware::net::spawn(&_spawner, peripherals.WIFI, &memory_config, |stage| {
                #[cfg(feature = "web_serial")]
                let _ = usb_write_bytes_bounded(&mut usb_serial, stage);
            })
            .await
        {
            warn!("LAN control plane startup failed: {=str}", error.message());
            flux_purr_firmware::net::report_startup_failure(error).await;
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

    let runtime_started_ms = Instant::now().as_millis();
    let mut last_control_ms: u64 = 0;
    let mut next_control_deadline_ms = HEATER_CONTROL_INTERVAL_MS;
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

        let raw_state = inputs.sample();
        let sample = controller.sample_with_capabilities(
            elapsed_ms,
            raw_state,
            ui_state.gesture_capabilities(),
        );
        let mut needs_redraw = false;
        let route_before_usb_control = ui_state.route;
        #[cfg(feature = "web_serial")]
        loop {
            match usb_serial.read_byte() {
                Ok(b'\n') => {
                    if let Some(response) = process_eeprom_snapshot_line(
                        usb_rx_line.as_str(),
                        &mut eeprom_snapshot_session,
                        &mut pd_i2c,
                        last_heater_duty,
                        &mut memory_commit_due_ms,
                        elapsed_ms,
                    ) {
                        if eeprom_snapshot_storage_failure(&response) {
                            mark_eeprom_required(
                                &mut ui_state,
                                &mut calibration_runtime_state,
                                &mut manual_pps_state,
                                &mut memory_commit_due_ms,
                            );
                        }
                        write_eeprom_snapshot_response(&mut usb_serial, &response, usb_tx_buf);
                        usb_rx_line.clear();
                        continue;
                    }
                    let (control_needs_redraw, response) = process_control_line(
                        usb_rx_line.as_str(),
                        &mut controller,
                        &mut ui_state,
                        &mut memory_config,
                        &mut preview_heater_curve,
                        &mut memory_commit_due_ms,
                        &mut memory_sequence,
                        persistence_source,
                        persistence_record_state,
                        &mut pd_i2c,
                        pd_port.controller_kind(),
                        &mut pd_port,
                        &mut calibration_runtime_state,
                        thermal_plant_workspace,
                        elapsed_ms,
                        last_pd_observation,
                        &mut heater_power_backend,
                        &mut heater_controller,
                        last_pid_snapshot,
                        &mut manual_pps_state,
                        fan_command,
                        current_rtd_fault,
                        &mut overtemp_attention_acknowledged,
                        &mut attention_pending_after_fault_clear,
                        &mut overtemp_forced_fan_active,
                        &mut next_attention_reminder_ms,
                        &mut buzzer,
                        &mut thermal_control_profile_preview,
                        last_raw_state,
                        latest_display_temp_c,
                        latest_temp_c,
                        control_measurement_guarded,
                        latest_rtd_raw_adc_mv,
                        latest_rtd_raw_adc_min_mv,
                        latest_rtd_raw_adc_max_mv,
                        latest_vin_raw_adc_mv,
                        latest_vin_mv,
                        last_heater_duty,
                        heater_control_timing,
                    )
                    .await;
                    needs_redraw |= control_needs_redraw;
                    needs_redraw |= disarm_pending_thermal_plant_output(
                        &mut calibration_runtime_state,
                        &mut heater_power_backend,
                        &mut manual_pps_state,
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut heater_pwm,
                        &mut hold_pps_governor,
                        &mut ui_state,
                        &mut last_heater_duty,
                        latest_vin_mv,
                    )
                    .await;
                    usb_write_response_frame(&mut usb_serial, &response, usb_tx_buf);
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
        let pairing_opened_by_usb = route_before_usb_control != FrontPanelRoute::WifiInfo
            && ui_state.route == FrontPanelRoute::WifiInfo;
        if pairing_opened_by_usb {
            suppress_pairing_input_until_released = true;
        }

        #[cfg(feature = "net_http")]
        while let Some(command) = flux_purr_firmware::net::try_receive_command() {
            let request_id = command.request_id;
            let response_slot = command.response_slot;
            let is_mutation = matches!(
                command.method,
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Delete
            );
            if is_mutation && !flux_purr_firmware::net::command_lease_is_active(&command).await {
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
                (LanEndpoint::Identity, HttpMethod::Get) => Some(lan_json_response(
                    &flux_purr_firmware::net::lan_identity().await,
                )),
                (LanEndpoint::Network, HttpMethod::Get) => Some(lan_json_response(
                    &flux_purr_firmware::net::lan_network_summary().await,
                )),
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
            let (control_needs_redraw, response) = process_control_line(
                line.as_str(),
                &mut controller,
                &mut ui_state,
                &mut memory_config,
                &mut preview_heater_curve,
                &mut memory_commit_due_ms,
                &mut memory_sequence,
                persistence_source,
                persistence_record_state,
                &mut pd_i2c,
                pd_port.controller_kind(),
                &mut pd_port,
                &mut calibration_runtime_state,
                thermal_plant_workspace,
                elapsed_ms,
                last_pd_observation,
                &mut heater_power_backend,
                &mut heater_controller,
                last_pid_snapshot,
                &mut manual_pps_state,
                fan_command,
                current_rtd_fault,
                &mut overtemp_attention_acknowledged,
                &mut attention_pending_after_fault_clear,
                &mut overtemp_forced_fan_active,
                &mut next_attention_reminder_ms,
                &mut buzzer,
                &mut thermal_control_profile_preview,
                last_raw_state,
                latest_display_temp_c,
                latest_temp_c,
                control_measurement_guarded,
                latest_rtd_raw_adc_mv,
                latest_rtd_raw_adc_min_mv,
                latest_rtd_raw_adc_max_mv,
                latest_vin_raw_adc_mv,
                latest_vin_mv,
                last_heater_duty,
                heater_control_timing,
            )
            .await;
            needs_redraw |= control_needs_redraw;
            needs_redraw |= disarm_pending_thermal_plant_output(
                &mut calibration_runtime_state,
                &mut heater_power_backend,
                &mut manual_pps_state,
                &mut pd_i2c,
                &mut pd_port,
                &mut heater_pwm,
                &mut hold_pps_governor,
                &mut ui_state,
                &mut last_heater_duty,
                latest_vin_mv,
            )
            .await;
            let (status, body) = lan_frame_response(
                &response,
                flux_purr_firmware::net::lan_network_summary().await,
            );
            flux_purr_firmware::net::respond_to_command(
                response_slot,
                request_id,
                status,
                body,
                is_mutation,
            );
        }
        #[cfg(feature = "net_http")]
        if let Some(token) = flux_purr_firmware::net::take_persisted_token_change().await {
            memory_config.lan_pairing_token = token;
            memory_commit_due_ms = Some(elapsed_ms.saturating_add(MEMORY_WRITE_DEBOUNCE_MS));
        }

        needs_redraw |= disarm_pending_thermal_plant_output(
            &mut calibration_runtime_state,
            &mut heater_power_backend,
            &mut manual_pps_state,
            &mut pd_i2c,
            &mut pd_port,
            &mut heater_pwm,
            &mut hold_pps_governor,
            &mut ui_state,
            &mut last_heater_duty,
            latest_vin_mv,
        )
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
            if route_before != FrontPanelRoute::WifiInfo
                && ui_state.route == FrontPanelRoute::WifiInfo
            {
                #[cfg(feature = "net_http")]
                {
                    let code = flux_purr_firmware::net::enter_pairing().await;
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
                    flux_purr_firmware::net::leave_pairing().await;
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
            let mut rtd_sample = read_rtd_sample(
                &mut adc1,
                &mut rtd_adc_pin,
                adc_curve.as_ref(),
                &memory_config,
            );
            let first_rtd_snapshot = match &rtd_sample {
                RtdSample::Valid(measurement) => {
                    (measurement.raw_adc_mv, measurement.temp_c, "valid")
                }
                RtdSample::Fault { adc_mv, reason } => (adc_mv.unwrap_or(0), 0.0, reason.label()),
            };

            if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) = read_calibrated_vin_mv(
                &mut adc1,
                &mut vin_adc_pin,
                adc_curve.as_ref(),
                &memory_config,
            ) {
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
                info!(
                    "vin sample raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
                    raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
                );
                if retry_rtd_after_power_step {
                    rtd_sample = read_rtd_sample(
                        &mut adc1,
                        &mut rtd_adc_pin,
                        adc_curve.as_ref(),
                        &memory_config,
                    );
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

            let current_pd_observation =
                read_pd_status(&mut pd_i2c, &mut pd_port, elapsed_ms).await;
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
            let current_pd_contract_ready = startup_pd_contract_ready(current_pd_observation);
            if pd_contract_ready != current_pd_contract_ready {
                pd_contract_ready = current_pd_contract_ready;
                needs_redraw = true;
                if pd_contract_ready {
                    info!("PD contract became ready; released startup heater interlock");
                } else {
                    info!("PD contract became unavailable; heater interlocked");
                }
            }
            if pd_port.controller_kind() == ControllerKind::Fusb302b
                && matches!(
                    heater_power_backend,
                    HeaterPowerBackend::FixedPdPwmFallback { .. }
                )
                && let Some(capabilities) = read_pd_power_capabilities(&mut pd_i2c, &mut pd_port)
                && let Some(next_backend) = fusb302b_pps_backend_from_capabilities(capabilities)
            {
                heater_power_backend = next_backend;
                manual_pps_state = ManualPpsState::from_capabilities(Some(capabilities));
                hold_pps_governor = HoldPpsGovernor::new();
                needs_redraw = true;
                info!(
                    "fusb302b source capabilities became available; promoted heater backend to pps-mos"
                );
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
                    latest_rtd_raw_adc_mv,
                    latest_vin_raw_adc_mv,
                    calibration_temp_c,
                    current_pd_observation
                        .map(|observation| observation.current_ma)
                        .unwrap_or(0),
                    latest_vin_mv,
                    last_heater_duty,
                );
            }
            let thermal_plant_completed = calibration_runtime_state.mode == CalibrationMode::Off
                && calibration_runtime_state.job.kind == Some(CalibrationJobKind::ThermalPlant)
                && calibration_runtime_state.job.status == CalibrationJobStatus::Completed;
            if memory_config != memory_before_calibration_job {
                if thermal_plant_completed {
                    if let Err(error) = commit_memory_config_now(
                        &mut pd_i2c,
                        &mut pd_port,
                        elapsed_ms,
                        &mut memory_sequence,
                        &memory_config,
                    )
                    .await
                    {
                        memory_config = memory_before_calibration_job;
                        mark_eeprom_required(
                            &mut ui_state,
                            &mut calibration_runtime_state,
                            &mut manual_pps_state,
                            &mut memory_commit_due_ms,
                        );
                        calibration_job_fail(
                            &mut calibration_runtime_state,
                            ManualPpsError::WriteFailed,
                            false,
                            &mut manual_pps_state,
                        );
                        info!(
                            "thermal plant activation commit failed reason={=str}",
                            error.code()
                        );
                    } else {
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
            needs_redraw |= disarm_pending_thermal_plant_output(
                &mut calibration_runtime_state,
                &mut heater_power_backend,
                &mut manual_pps_state,
                &mut pd_i2c,
                &mut pd_port,
                &mut heater_pwm,
                &mut hold_pps_governor,
                &mut ui_state,
                &mut last_heater_duty,
                latest_vin_mv,
            )
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
            let runtime_source_limits = manual_pps_state.thermal_plant_source_limits();
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
            if apply_heater_power_output(
                &mut pd_i2c,
                &mut pd_port,
                &mut heater_pwm,
                &mut heater_power_backend,
                &mut hold_pps_governor,
                &mut manual_pps_state,
                current_pd_observation,
                latest_vin_mv,
                latest_temp_c,
                requested_duty_percent,
                if force_thermal_plant_output_off {
                    false
                } else if thermal_plant_calibration_running {
                    calibration_runtime_state.heater_enabled
                } else {
                    ui_state.heater_enabled
                },
                pid_snapshot.phase,
                pid_snapshot.error_c,
                pid_snapshot.filtered_slope_c_per_s,
                pid_snapshot.warmup_soft_start_percent,
                &mut last_heater_duty,
                preview_heater_curve_config(preview_heater_curve.as_ref()),
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
            if commit_memory_config_now(
                &mut pd_i2c,
                &mut pd_port,
                elapsed_ms,
                &mut memory_sequence,
                &memory_config,
            )
            .await
            .is_err()
            {
                mark_eeprom_required(
                    &mut ui_state,
                    &mut calibration_runtime_state,
                    &mut manual_pps_state,
                    &mut memory_commit_due_ms,
                );
            }
        }

        if ui_state.eeprom_required && !eeprom_required {
            eeprom_required = true;
            persistence_source = "eeprom_required";
            persistence_record_state = "eeprom_required";
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
            let _ = apply_heater_power_output(
                &mut pd_i2c,
                &mut pd_port,
                &mut heater_pwm,
                &mut heater_power_backend,
                &mut hold_pps_governor,
                &mut manual_pps_state,
                last_pd_observation,
                latest_vin_mv,
                latest_temp_c,
                0,
                false,
                HeaterControlPhase::Warmup,
                -1.0,
                -1.0,
                0,
                &mut last_heater_duty,
                preview_heater_curve_config(preview_heater_curve.as_ref()),
                &memory_config,
                active_thermal_settings,
                elapsed_ms,
            )
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

        let mut fan_decision = fan_policy_decision(
            latest_display_temp_i16,
            elapsed_ms,
            ui_state.heater_enabled,
            ui_state.heater_output_percent,
            ui_state.active_cooling_enabled,
            fan_policy_state,
            is_sensor_fault(current_rtd_fault),
        );
        if calibration_runtime_state.mode == CalibrationMode::ThermalPlant
            && calibration_runtime_state.job.status == CalibrationJobStatus::Running
        {
            fan_decision = FanPolicyDecision {
                state: FanPolicyState::Disabled,
                command: FanHardwareCommand::disabled(),
                display_state: FanDisplayState::Off,
            };
        }
        if let Some(state) =
            overtemp_forced_fan_state(latest_display_temp_i16, overtemp_forced_fan_active)
        {
            let command = state.command(elapsed_ms);
            fan_decision = FanPolicyDecision {
                state,
                command,
                display_state: fan_display_state_for_command(
                    ui_state.active_cooling_enabled,
                    command,
                ),
            };
        }
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
        if ui_state.apply_network_summary(flux_purr_firmware::net::lan_network_summary().await) {
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
            match with_timeout(
                DISPLAY_IO_TIMEOUT,
                flush_ui(&mut display, canvas, &ui_state),
            )
            .await
            {
                Ok(Ok(())) => log_ui_state(&ui_state),
                Ok(Err(_)) | Err(_) => {
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
                    let _ = disarm_pending_thermal_plant_output(
                        &mut calibration_runtime_state,
                        &mut heater_power_backend,
                        &mut manual_pps_state,
                        &mut pd_i2c,
                        &mut pd_port,
                        &mut heater_pwm,
                        &mut hold_pps_governor,
                        &mut ui_state,
                        &mut last_heater_duty,
                        latest_vin_mv,
                    )
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
            ui_refresh_pending = false;
            next_ui_refresh_ms = elapsed_ms.saturating_add(DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS);
        }
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!(
        "flux-purr now runs the interactive frontpanel runtime; build with --target xtensa-esp32s3-none-elf --features esp32s3,web_serial,net_http"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusb302b_identity_requires_stable_family_id_and_readable_status() {
        assert!(fusb302b_identity_is_stable(
            Some(0x91),
            Some(0x91),
            Some(0),
            Some(0)
        ));
        assert!(!fusb302b_identity_is_stable(
            Some(0x91),
            Some(0x92),
            Some(0),
            Some(0)
        ));
        assert!(!fusb302b_identity_is_stable(
            Some(0x81),
            Some(0x81),
            Some(0),
            Some(0)
        ));
        assert!(!fusb302b_identity_is_stable(
            Some(0x91),
            Some(0x91),
            Some(u8::MAX),
            Some(0),
        ));
        assert!(!fusb302b_identity_is_stable(
            Some(0x91),
            Some(0x91),
            Some(0),
            None
        ));
    }

    #[test]
    fn v5_memory_header_bounds_payload_before_reading_slot_body() {
        let mut header = [0xff; MEMORY_RECORD_HEADER_LEN];
        header[0..4].copy_from_slice(b"FPM1");
        header[4] = MEMORY_RECORD_FORMAT_VERSION;
        header[5] = MEMORY_RECORD_HEADER_LEN as u8;
        header[6..8].copy_from_slice(&100u16.to_le_bytes());
        assert_eq!(
            memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
            Some(MEMORY_RECORD_HEADER_LEN + 100)
        );

        header[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(
            memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
            None
        );
        header[4] = 0x7f;
        assert_eq!(
            memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
            None
        );
        header[4] = 1;
        assert_eq!(
            memory_record_length_from_header(&header, MEMORY_SLOT_SIZE),
            None
        );
    }

    #[test]
    fn startup_safe_outputs_precede_pd_and_legacy_eeprom_work() {
        let source = include_str!("flux_purr.rs");
        let outputs = source
            .find("boot_stage=outputs_init_start")
            .expect("safe output stage marker");
        let pd = source
            .find("boot_stage=pd_detect_start")
            .expect("PD stage marker");
        let legacy = source
            .find("load_legacy_eeprom_memory_record(&mut pd_i2c, scratch)")
            .expect("legacy restore call");
        let first_frame = source
            .find("present_initial_frontpanel_ui(&mut display, canvas, &ui_state)")
            .expect("initial Dashboard frame");
        assert!(outputs < pd);
        assert!(first_frame < legacy);
    }

    #[test]
    fn fusb302b_initial_pps_request_matches_the_idle_voltage() {
        assert_eq!(FUSB302B_INITIAL_PPS_REQUEST_MV, HEATER_ADJUSTABLE_MIN_MV);
        assert_eq!(FUSB302B_INITIAL_PPS_REQUEST_MV, 12_000);
    }

    #[test]
    fn zeroize_bytes_scrubs_reusable_heap_workspace() {
        let mut bytes = [0xFA, 0x00, 0xF4, 0x01, 0xA5, 0x5A];
        zeroize_bytes_volatile(&mut bytes);
        assert_eq!(bytes, [0; 6]);
    }

    #[test]
    fn same_frequency_buzzer_retrigger_does_not_reconfigure_timer() {
        let configured_frequency_hz = 1_080;
        let active_state = BuzzerHardwareState {
            frequency_hz: Some(1_080),
            duty_percent: 50,
            generation: 7,
        };
        let retriggered_state = BuzzerHardwareState {
            generation: 8,
            ..active_state
        };

        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            retriggered_state
        ));
    }

    #[test]
    fn buzzer_timer_readback_distinguishes_the_heater_on_tone_steps() {
        let low_period = buzzer_timer_period_ticks(1_240).unwrap();
        let high_period = buzzer_timer_period_ticks(1_680).unwrap();

        assert_ne!(low_period, high_period);
        assert_ne!(
            mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, low_period),
            mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, high_period)
        );
    }

    #[test]
    fn buzzer_timer_keeps_one_prescaler_and_represents_every_production_frequency() {
        for frequency_hz in [
            320, 360, 420, 480, 900, 1_080, 1_200, 1_240, 1_550, 1_650, 1_680, 2_200, 2_300,
        ] {
            let period_ticks = buzzer_timer_period_ticks(frequency_hz)
                .expect("every production cue frequency must fit Timer2");
            let applied_frequency_hz =
                mcpwm_timer_frequency_hz(BUZZER_TIMER_PRESCALER, period_ticks);
            assert!(
                applied_frequency_hz.abs_diff(frequency_hz) <= 1,
                "requested {frequency_hz} Hz, got {applied_frequency_hz} Hz"
            );
        }
    }

    #[test]
    fn buzzer_pad_edge_observation_distinguishes_active_cooling_tones() {
        assert_eq!(buzzer_observed_frequency_hz(41, 45), Some(911));
        assert_eq!(buzzer_observed_frequency_hz(54, 45), Some(1_200));
        assert_eq!(buzzer_observed_frequency_hz(112, 70), Some(1_600));
        assert_eq!(buzzer_observed_frequency_hz(0, 0), None);
    }

    #[test]
    fn active_cooling_tone_steps_quiet_and_stop_timer_before_each_retune() {
        let mut buzzer = BuzzerArbiter::new();
        let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
        let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
            frequency_hz: output.frequency_hz,
            duty_percent: output.duty_percent,
            generation: output.generation,
        };

        let _ =
            buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::ActiveCoolingOn, 0);
        let first_tone = hardware_state(buzzer.output());
        assert_eq!(
            buzzer_hardware_actions(configured_frequency_hz, first_tone).as_slice(),
            [
                BuzzerHardwareAction::SetDutyPercent(0),
                BuzzerHardwareAction::StopTimer,
                BuzzerHardwareAction::Retune(900),
                BuzzerHardwareAction::SetDutyPercent(50),
            ]
        );
        configured_frequency_hz = first_tone.frequency_hz.unwrap();

        let first_rest = hardware_state(buzzer.tick(45).output);
        assert_eq!(
            buzzer_hardware_actions(configured_frequency_hz, first_rest).as_slice(),
            [BuzzerHardwareAction::SetDutyPercent(0)]
        );

        let second_tone = hardware_state(buzzer.tick(70).output);
        assert_eq!(
            buzzer_hardware_actions(configured_frequency_hz, second_tone).as_slice(),
            [
                BuzzerHardwareAction::SetDutyPercent(0),
                BuzzerHardwareAction::StopTimer,
                BuzzerHardwareAction::Retune(1_200),
                BuzzerHardwareAction::SetDutyPercent(50),
            ]
        );
        configured_frequency_hz = second_tone.frequency_hz.unwrap();

        let second_rest = hardware_state(buzzer.tick(115).output);
        assert_eq!(
            buzzer_hardware_actions(configured_frequency_hz, second_rest).as_slice(),
            [BuzzerHardwareAction::SetDutyPercent(0)]
        );

        let third_tone = hardware_state(buzzer.tick(140).output);
        assert_eq!(
            buzzer_hardware_actions(configured_frequency_hz, third_tone).as_slice(),
            [
                BuzzerHardwareAction::SetDutyPercent(0),
                BuzzerHardwareAction::StopTimer,
                BuzzerHardwareAction::Retune(1_550),
                BuzzerHardwareAction::SetDutyPercent(50),
            ]
        );
    }

    #[test]
    fn buzzer_timer_keeps_the_carrier_through_ui_input_silence() {
        let silent_state = BuzzerHardwareState {
            frequency_hz: None,
            duty_percent: 0,
            generation: 1,
        };
        let ui_input_state = BuzzerHardwareState {
            frequency_hz: Some(1_080),
            duty_percent: 50,
            generation: 2,
        };
        let heater_on_state = BuzzerHardwareState {
            frequency_hz: Some(1_240),
            duty_percent: 50,
            generation: 3,
        };

        assert!(buzzer_timer_reconfiguration_needed(
            BUZZER_IDLE_FREQUENCY_HZ,
            ui_input_state
        ));
        assert!(!buzzer_timer_reconfiguration_needed(
            ui_input_state.frequency_hz.unwrap(),
            silent_state
        ));
        assert!(!buzzer_timer_reconfiguration_needed(
            ui_input_state.frequency_hz.unwrap(),
            ui_input_state
        ));
        assert!(buzzer_timer_reconfiguration_needed(
            ui_input_state.frequency_hz.unwrap(),
            heater_on_state
        ));
    }

    #[test]
    fn fast_ui_input_repeat_reuses_the_carrier_after_its_45ms_silence_gap() {
        let mut buzzer = BuzzerArbiter::new();
        let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
        let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
            frequency_hz: output.frequency_hz,
            duty_percent: output.duty_percent,
            generation: output.generation,
        };

        let _ = buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 0);
        let first_tone = hardware_state(buzzer.output());
        assert!(buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            first_tone
        ));
        configured_frequency_hz = first_tone.frequency_hz.unwrap();

        let silence = hardware_state(buzzer.tick(45).output);
        assert_eq!(silence.frequency_hz, None);
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            silence
        ));

        let _ = buzzer.request_feedback(BuzzerCueSource::FrontPanel, BuzzerCueId::UiInput, 60);
        let fast_repeat_tone = hardware_state(buzzer.output());
        assert_eq!(fast_repeat_tone.frequency_hz, Some(1_080));
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            fast_repeat_tone
        ));
    }

    #[test]
    fn protection_alarm_reuses_one_carrier_through_its_pulses_and_replay() {
        let mut buzzer = BuzzerArbiter::new();
        let mut cadence = ProtectionAlarmCadence::new();
        let mut configured_frequency_hz = BUZZER_IDLE_FREQUENCY_HZ;
        let hardware_state = |output: BuzzerOutput| BuzzerHardwareState {
            frequency_hz: output.frequency_hz,
            duty_percent: output.duty_percent,
            generation: output.generation,
        };

        let _ = cadence.enter(&mut buzzer, 0);
        let first_pulse = hardware_state(buzzer.output());
        assert_eq!(first_pulse.frequency_hz, Some(2_300));
        assert!(buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            first_pulse
        ));
        configured_frequency_hz = first_pulse.frequency_hz.unwrap();

        let first_rest = hardware_state(buzzer.tick(90).output);
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            first_rest
        ));
        let second_pulse = hardware_state(buzzer.tick(130).output);
        assert_eq!(second_pulse.frequency_hz, Some(2_300));
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            second_pulse
        ));

        let second_rest = hardware_state(buzzer.tick(220).output);
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            second_rest
        ));
        let _ = buzzer.tick(300);

        let replay = cadence
            .tick(true, &mut buzzer, PROTECTION_ALARM_INTERVAL_MS)
            .expect("active protection replays at the production cadence");
        assert_eq!(replay.cue, BuzzerCueId::ProtectionAlarm);
        let replay_pulse = hardware_state(buzzer.output());
        assert_eq!(replay_pulse.frequency_hz, Some(2_300));
        assert!(!buzzer_timer_reconfiguration_needed(
            configured_frequency_hz,
            replay_pulse
        ));
    }

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
            pd_controller: ControllerKind::Ch224q,
            last_pd_observation: None,
            heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
                reason: HeaterPowerBackendReason::NoPps20vCapability,
                fixed_request_confirmed: true,
                fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
            },
            pid_snapshot: HeaterPidSnapshot {
                duty_percent: 0,
                warmup_soft_start_percent: 0,
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
            attention_pending_after_fault_clear: false,
            thermal_control_profile_preview: false,
            active_thermal_control_profile: None,
            last_raw_state: FrontPanelRawState::default(),
            latest_status_temp_c: 0.0,
            latest_control_temp_c: 0.0,
            control_measurement_guarded: false,
            latest_rtd_raw_adc_mv: 0,
            latest_rtd_raw_adc_min_mv: 0,
            latest_rtd_raw_adc_max_mv: 0,
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
    fn usb_write_bytes_waits_for_a_busy_endpoint_before_replying() {
        struct DelayedUsbTx {
            waits_before_ready: usize,
            waits: usize,
            pending: std::vec::Vec<u8>,
            sent: std::vec::Vec<u8>,
        }

        impl UsbControlTx for DelayedUsbTx {
            fn write_byte_nb(&mut self, byte: u8) -> Result<(), UsbTxError> {
                if self.waits < self.waits_before_ready {
                    return Err(UsbTxError::WouldBlock);
                }
                self.pending.push(byte);
                Ok(())
            }

            fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
                if self.waits < self.waits_before_ready {
                    return Err(UsbTxError::WouldBlock);
                }
                self.sent.extend_from_slice(&self.pending);
                self.pending.clear();
                Ok(())
            }

            fn wait_for_tx_progress(&mut self) {
                self.waits += 1;
            }
        }

        let mut tx = DelayedUsbTx {
            waits_before_ready: 3,
            waits: 0,
            pending: std::vec::Vec::new(),
            sent: std::vec::Vec::new(),
        };

        assert!(usb_write_bytes_bounded(&mut tx, b"response\\n"));
        assert_eq!(tx.waits, 3);
        assert_eq!(tx.sent, b"response\\n");
    }

    #[test]
    fn usb_response_write_uses_default_bounded_chunks_for_host_requested_frames() {
        let payload = std::vec![b'x'; 180];
        let mut bounded_tx = FakeUsbTx::new(0);
        assert!(!usb_write_bytes_bounded(&mut bounded_tx, &payload));

        let mut response_tx = FakeUsbTx::new(64);
        let mut request_id = heapless::String::new();
        request_id.push_str("response-write").unwrap();
        let response = usb_response(
            request_id,
            UsbResponsePayload::Identity(Box::new(Identity::firmware_default())),
        );
        let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

        usb_write_response_frame_to(&mut response_tx, &response, &mut tx_buf);

        let line = core::str::from_utf8(&response_tx.sent).expect("response frame is utf8");
        assert!(line.contains(r#""requestId":"response-write""#));
        assert!(line.ends_with('\n'));
        assert!(response_tx.flush_count > 1);
    }

    #[test]
    fn usb_response_write_allows_the_runtime_transport_to_confirm_delivery() {
        struct ConfirmingUsbTx {
            response: std::vec::Vec<u8>,
            fallback_write_attempts: usize,
        }

        impl UsbControlTx for ConfirmingUsbTx {
            fn write_byte_nb(&mut self, _byte: u8) -> Result<(), UsbTxError> {
                self.fallback_write_attempts += 1;
                Err(UsbTxError::Other)
            }

            fn flush_tx_nb(&mut self) -> Result<(), UsbTxError> {
                Ok(())
            }

            fn write_response_bytes(&mut self, bytes: &[u8]) -> bool {
                self.response.extend_from_slice(bytes);
                true
            }
        }

        let mut request_id = heapless::String::new();
        request_id.push_str("confirmed-response").unwrap();
        let response = usb_response(
            request_id,
            UsbResponsePayload::Identity(Box::new(Identity::firmware_default())),
        );
        let mut tx = ConfirmingUsbTx {
            response: std::vec::Vec::new(),
            fallback_write_attempts: 0,
        };
        let mut tx_buf = [0_u8; USB_CONTROL_TX_BUFFER_LEN];

        usb_write_response_frame_to(&mut tx, &response, &mut tx_buf);

        let line = core::str::from_utf8(&tx.response).expect("response is utf8");
        assert!(line.contains(r#""requestId":"confirmed-response""#));
        assert_eq!(tx.fallback_write_attempts, 0);
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
                assert_eq!(identity.device_id.as_str(), "a0f262f20d6c");
                assert_eq!(identity.hostname.as_str(), "flux-purr-a0f262f20d6c");
            }
            other => panic!("unexpected early identity response: {other:?}"),
        }
    }

    #[test]
    fn early_usb_control_defers_network_until_main_loop() {
        let mut config = MemoryConfig::default();
        config.wifi_ssid.push_str("bench-net").unwrap();
        let response = usb_early_response(
            r#"{"type":"request","requestId":"boot-net","op":"get_network"}"#,
            &config,
        );

        match response {
            UsbFrame::Response {
                request_id,
                ok: false,
                result: None,
                error: Some(error),
            } => {
                assert_eq!(request_id.as_str(), "boot-net");
                assert_eq!(error.code.as_str(), "startup_busy");
                assert!(error.retryable);
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
    fn startup_recovery_defers_network_and_status_until_persistent_state_is_ready() {
        let memory_config = MemoryConfig::default();
        for (request_id, request) in [
            (
                "recovery-net",
                r#"{"type":"request","requestId":"recovery-net","op":"get_network"}"#,
            ),
            (
                "recovery-status",
                r#"{"type":"request","requestId":"recovery-status","op":"get_status"}"#,
            ),
        ] {
            let response = usb_recovery_response_for_phase(
                request,
                &memory_config,
                0,
                UsbRecoveryPhase::BeforePersistentState,
            );

            match response {
                UsbFrame::Response {
                    request_id: actual_request_id,
                    ok: false,
                    result: None,
                    error: Some(error),
                } => {
                    assert_eq!(actual_request_id.as_str(), request_id);
                    assert_eq!(error.code.as_str(), "startup_busy");
                    assert!(error.retryable);
                }
                other => panic!("unexpected startup recovery response: {other:?}"),
            }
        }
    }

    #[test]
    fn recovery_usb_control_reports_fault_status_when_bringup_fails() {
        let mut memory_config = MemoryConfig {
            target_temp_c: 215,
            ..MemoryConfig::default()
        };
        memory_config.wifi_ssid.push_str("bench-net").unwrap();
        let response = usb_recovery_response_for_phase(
            r#"{"type":"request","requestId":"recovery-status","op":"get_status"}"#,
            &memory_config,
            7_200,
            UsbRecoveryPhase::RuntimeFault,
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
                heater_enabled: Some(false),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                fault_attention_acknowledged: None,
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
                assert!(!status.heater_enabled);
                assert_eq!(status.uptime_seconds, 12);
                assert_eq!(memory_config.target_temp_c, 240);
                assert!(!memory_config.active_cooling_enabled);
            }
            other => panic!("unexpected runtime config response: {other:?}"),
        }
    }

    #[test]
    fn runtime_config_response_rejects_heater_arm_when_thermal_model_is_missing() {
        let mut request_id = heapless::String::new();
        request_id.push_str("runtime-heater-model-missing").unwrap();
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut memory_config = MemoryConfig::default();
        let mut manual_pps = ManualPpsState::default();
        let mut thermal_profile_preview = None;

        let (response, _) = usb_runtime_config_response(
            request_id,
            RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: Some(true),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            &mut ui_state,
            &mut memory_config,
            &mut manual_pps,
            &mut thermal_profile_preview,
            test_usb_runtime_status_context(),
        );

        match response {
            UsbFrame::Response {
                ok: true,
                result: Some(UsbResponsePayload::Status(status)),
                error: None,
                ..
            } => {
                assert!(!status.heater_enabled);
                assert_eq!(status.heater_output_percent, 0);
                assert_eq!(status.heater_physical_output_percent, 0);
                assert!(!ui_state.heater_enabled);
            }
            other => panic!("unexpected runtime config response: {other:?}"),
        }
    }

    #[test]
    fn runtime_config_cannot_override_a_running_thermal_plant_job() {
        let mut request_id = heapless::String::new();
        request_id.push_str("runtime-thermal-job-busy").unwrap();
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut memory_config = MemoryConfig::default();
        let mut manual_pps = ManualPpsState::default();
        let mut thermal_profile_preview = None;
        let mut context = test_usb_runtime_status_context();
        context.calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };

        let (response, returned_calibration) = usb_runtime_config_response(
            request_id,
            RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: Some(true),
                manual_pps_enabled: None,
                manual_pps_mv: None,
                manual_pps_ma: None,
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            &mut ui_state,
            &mut memory_config,
            &mut manual_pps,
            &mut thermal_profile_preview,
            context,
        );

        match response {
            UsbFrame::Response {
                ok: false,
                error: Some(error),
                ..
            } => assert_eq!(error.code.as_str(), "manual_pps_calibration_busy"),
            other => panic!("unexpected runtime response: {other:?}"),
        }
        assert_eq!(
            returned_calibration.job.status,
            CalibrationJobStatus::Running
        );
        assert!(!ui_state.heater_enabled);
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
            warmup_reenter_centi_c: 0,
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
                fault_attention_acknowledged: None,
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
            warmup_reenter_centi_c: 0,
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
                fault_attention_acknowledged: None,
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
            warmup_reenter_centi_c: 0,
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
                fault_attention_acknowledged: None,
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
                assert_eq!(status.thermal_control.warmup_power_permille, 1_000);
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
                        warmup_reenter_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_WARMUP_REENTER_CENTI_C_DEFAULT,
                        approach_power_permille: 260,
                        approach_floor_power_permille: 180,
                        approach_damping_exponent_permille: 1_000,
                        approach_tail_window_centi_c: 0,
                        hold_power_permille: 180,
                        hold_reheat_power_permille: 0,
                        hold_entry_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_ENTRY_CENTI_C_DEFAULT,
                        hold_exit_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_EXIT_CENTI_C_DEFAULT,
                        hold_on_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_ON_CENTI_C_DEFAULT,
                        hold_off_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_OFF_CENTI_C_DEFAULT,
                        overshoot_cutoff_centi_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_OVERSHOOT_CUTOFF_CENTI_C_DEFAULT,
                        hold_kp_permille_per_c:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_KP_PERMILLE_PER_C_DEFAULT,
                        hold_ki_permille_per_c_tick:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_KI_PERMILLE_PER_C_TICK_DEFAULT,
                        hold_blend_ticks:
                            flux_purr_firmware::memory::THERMAL_CONTROL_PROFILE_HOLD_BLEND_TICKS_DEFAULT,
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
            warmup_reenter_centi_c: 0,
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
                    warmup_soft_start_percent: 100,
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
                latest_control_temp_c: 139.8,
                control_measurement_guarded: true,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.heater_control_phase.as_deref(), Some("hold"));
        assert_eq!(status.heater_error_c, Some(-0.4));
        assert_eq!(status.heater_control_error_c, Some(-0.2));
        assert_eq!(status.heater_control_temp_c, Some(139.8));
        assert!(status.heater_control_measurement_guarded);
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
                latest_status_temp_c: 140.237,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.board_temp_centi, 14_024);
        assert_eq!(status.current_temp_c, 140.24);
    }

    #[test]
    fn runtime_status_reports_rtd_batch_extrema() {
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                latest_rtd_raw_adc_mv: 900,
                latest_rtd_raw_adc_min_mv: 899,
                latest_rtd_raw_adc_max_mv: 902,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.rtd_raw_adc_mv, 900);
        assert_eq!(status.rtd_raw_adc_min_mv, 899);
        assert_eq!(status.rtd_raw_adc_max_mv, 902);
        assert_eq!(status.rtd_raw_adc_spread_mv, 3);
    }

    #[test]
    fn runtime_status_uses_live_rtd_sample_for_owner_facing_temperature() {
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                latest_status_temp_c: 141.499,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.board_temp_centi, 14_150);
        assert_eq!(status.current_temp_c, 141.5);
    }

    #[test]
    fn runtime_status_preserves_control_filter_telemetry_separately_from_display_temperature() {
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                latest_status_temp_c: 73.74,
                pid_snapshot: HeaterPidSnapshot {
                    duty_percent: 0,
                    warmup_soft_start_percent: 100,
                    error_c: 0.998,
                    control_error_c: 0.998,
                    filtered_temp_c: 59.004,
                    filtered_slope_c_per_s: -0.4,
                    coast_active: false,
                    phase: HeaterControlPhase::Hold,
                },
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.current_temp_c, 73.74);
        assert_eq!(status.board_temp_centi, 7_374);
        assert_eq!(status.heater_filtered_temp_c, Some(59.004));
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
                    terminal_fixed_pd_disarmed: false,
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
                fault_attention_acknowledged: None,
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
                    terminal_fixed_pd_disarmed: false,
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
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            CalibrationRuntimeState::default(),
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
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            CalibrationRuntimeState::default(),
            &mut manual_pps,
        )
        .unwrap();
        assert!(!manual_pps.enabled);
        assert_eq!(manual_pps.target_mv, None);
        assert_eq!(manual_pps.target_ma, None);
        assert!(manual_pps.consume_automatic_restore_pending());
    }

    #[test]
    fn manual_pps_config_cannot_take_over_a_running_thermal_plant_job() {
        let error = apply_manual_pps_config(
            &RuntimeConfigCommand {
                target_temp_c: None,
                selected_preset_slot: None,
                presets_c: None,
                active_cooling_enabled: None,
                heater_enabled: None,
                manual_pps_enabled: Some(false),
                manual_pps_mv: None,
                manual_pps_ma: None,
                fault_attention_acknowledged: None,
                calibration: None,
                thermal_profile_mode: None,
                thermal_control_profile: None,
            },
            CalibrationRuntimeState {
                mode: CalibrationMode::ThermalPlant,
                job: CalibrationJobState {
                    kind: Some(CalibrationJobKind::ThermalPlant),
                    status: CalibrationJobStatus::Running,
                    ..CalibrationJobState::default()
                },
                ..CalibrationRuntimeState::default()
            },
            &mut ManualPpsState::default(),
        )
        .unwrap_err();

        assert_eq!(error, ManualPpsError::CalibrationInProgress);
    }

    #[test]
    fn thermal_plant_job_rejects_calibration_control_mutation_atomically() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            heater_enabled: true,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };
        let before = calibration;
        let mut manual_pps = ManualPpsState::default();

        let error = apply_calibration_control_config(
            &CalibrationControlCommand {
                mode: Some(CalibrationModeWire::HeaterCurve),
                pps_enabled: Some(false),
                pps_mv: None,
                heater_enabled: Some(false),
                target_adc_mv: Some(1_000),
            },
            &mut calibration,
            &mut manual_pps,
        )
        .unwrap_err();

        assert_eq!(error, ManualPpsError::CalibrationInProgress);
        assert_eq!(calibration, before);
        assert_eq!(manual_pps, ManualPpsState::default());
    }

    #[test]
    fn manual_calibration_control_cannot_select_thermal_plant_mode() {
        let mut calibration = CalibrationRuntimeState::default();
        let mut manual_pps = ManualPpsState::default();

        let error = apply_calibration_control_config(
            &CalibrationControlCommand {
                mode: Some(CalibrationModeWire::ThermalPlant),
                pps_enabled: None,
                pps_mv: None,
                heater_enabled: None,
                target_adc_mv: None,
            },
            &mut calibration,
            &mut manual_pps,
        )
        .expect_err("thermal plant mode is job-only");

        assert_eq!(error, ManualPpsError::ThermalPlantManagedByJob);
        assert_eq!(calibration, CalibrationRuntimeState::default());
        assert_eq!(manual_pps, ManualPpsState::default());
    }

    #[test]
    fn leaving_calibration_mode_disarms_its_pps_override() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::HeaterCurve,
            pps_enabled: true,
            heater_enabled: true,
            ..CalibrationRuntimeState::default()
        };
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }));
        manual_pps
            .enable(ManualPpsOwner::Calibration, 20_000, Some(3_000))
            .expect("calibration PPS applies");

        apply_calibration_control_config(
            &CalibrationControlCommand {
                mode: Some(CalibrationModeWire::Off),
                pps_enabled: None,
                pps_mv: None,
                heater_enabled: None,
                target_adc_mv: None,
            },
            &mut calibration,
            &mut manual_pps,
        )
        .expect("calibration mode exits");

        assert_eq!(calibration.mode, CalibrationMode::Off);
        assert!(!calibration.heater_enabled);
        assert!(!manual_pps.enabled);
        assert!(calibration.immediate_heater_disarm_pending);
        assert!(manual_pps.consume_automatic_restore_pending());
    }

    #[test]
    fn transient_input_change_disarms_calibration_before_fixed_pd_restore() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::HeaterCurve,
            pps_enabled: true,
            pps_mv: Some(20_000),
            pps_ma: Some(3_000),
            heater_enabled: true,
            ..CalibrationRuntimeState::default()
        };
        let mut manual_pps = ManualPpsState {
            enabled: true,
            owner: ManualPpsOwner::Calibration,
            target_mv: Some(20_000),
            target_ma: Some(3_000),
            applied_mv: Some(20_000),
            ..ManualPpsState::default()
        };

        disarm_calibration_after_transient_input_change(&mut calibration, &mut manual_pps);

        assert!(!calibration.heater_enabled);
        assert!(!calibration.pps_enabled);
        assert_eq!(calibration.pps_mv, None);
        assert_eq!(calibration.pps_ma, None);
        assert!(calibration.immediate_heater_disarm_pending);
        assert!(!manual_pps.enabled);
        assert_eq!(manual_pps.target_mv, None);
        assert_eq!(manual_pps.applied_mv, None);
    }

    #[test]
    fn thermal_plant_job_waits_for_a_pending_terminal_disarm() {
        let mut calibration = CalibrationRuntimeState {
            immediate_heater_disarm_pending: true,
            ..CalibrationRuntimeState::default()
        };

        assert_eq!(
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut MemoryConfig::default(),
                &mut ManualPpsState::default(),
            ),
            Err(ManualPpsError::TerminalDisarmPending)
        );
        assert_eq!(
            apply_calibration_control_config(
                &CalibrationControlCommand {
                    mode: Some(CalibrationModeWire::HeaterCurve),
                    pps_enabled: Some(true),
                    pps_mv: Some(20_000),
                    heater_enabled: Some(true),
                    target_adc_mv: None,
                },
                &mut calibration,
                &mut ManualPpsState::default(),
            ),
            Err(ManualPpsError::TerminalDisarmPending)
        );
    }

    #[test]
    fn thermal_plant_job_locks_persistent_calibration_inputs() {
        let running = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };
        assert!(thermal_plant_calibration_job_running(running));

        let completed = CalibrationRuntimeState {
            job: CalibrationJobState {
                status: CalibrationJobStatus::Completed,
                ..running.job
            },
            ..running
        };
        assert!(!thermal_plant_calibration_job_running(completed));
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
            CalibrationJobKind::VinAdc,
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
                    &mut manual_pps,
                    0,
                    vin_raw_mv,
                    25.0,
                    3_000,
                    latest_vin_mv,
                    0,
                );
            }
        }

        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert_eq!(calibration.job.kind, Some(CalibrationJobKind::VinAdc));
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
            CalibrationJobKind::VinAdc,
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
                    &mut manual_pps,
                    0,
                    settled_raw_mv.saturating_sub(80),
                    25.0,
                    3_000,
                    u32::from(request_mv),
                    0,
                );
            }
            assert_eq!(calibration.job.samples_collected, step as u8);

            for _ in 0..5 {
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut manual_pps,
                    0,
                    settled_raw_mv,
                    25.0,
                    3_000,
                    u32::from(request_mv),
                    0,
                );
            }
            assert_eq!(calibration.job.samples_collected, step as u8 + 1);
        }

        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert_eq!(memory_config.adc_calibration.vin.sample_count(), 8);
    }

    #[test]
    fn thermal_plant_auto_job_requests_the_selected_apdo_ceiling_for_3a_and_5a() {
        for (max_mv, max_ma) in [(20_000, 3_000), (21_000, 3_000), (21_000, 5_000)] {
            let mut calibration = CalibrationRuntimeState::default();
            let mut memory_config = MemoryConfig::default();
            for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
                memory_config.heater_curve_raw_observations.points[index] =
                    Some(HeaterCurveRawObservation {
                        raw_rtd_adc_mv,
                        heater_voltage_mv: 20_000,
                        heater_current_ma: max_ma,
                        resistance_milliohms: 4_000,
                    });
            }
            let mut manual_pps =
                ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                    pps_covers_20v: true,
                    pps_min_mv: Some(5_000),
                    pps_max_mv: Some(max_mv),
                    pps_max_ma: Some(max_ma),
                    pps_apdos: [
                        Some(ch224q::PpsApdo {
                            min_mv: 5_000,
                            max_mv,
                            max_ma,
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
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut manual_pps,
            )
            .unwrap();

            assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
            assert_eq!(calibration.mode, CalibrationMode::ThermalPlant);
            assert_eq!(calibration.pps_mv, Some(max_mv));
            assert_eq!(calibration.job.next_request_mv, Some(max_mv));
            assert_eq!(calibration.pps_ma, Some(max_ma));
        }
    }

    #[test]
    fn thermal_plant_auto_job_uses_the_apdo_that_covers_20v_not_another_range() {
        let mut calibration = CalibrationRuntimeState::default();
        let mut memory_config = MemoryConfig::default();
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(5_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 21_000,
                        max_ma: 3_000,
                    }),
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 11_000,
                        max_ma: 5_000,
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

        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut manual_pps,
        )
        .unwrap();

        assert_eq!(calibration.pps_mv, Some(21_000));
        assert_eq!(calibration.pps_ma, Some(3_000));
        assert_eq!(calibration.job.next_request_mv, Some(21_000));
    }

    #[test]
    fn thermal_plant_source_selects_lowest_floor_after_current_and_ceiling_tie() {
        let manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(3_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 10_000,
                        max_mv: 21_000,
                        max_ma: 3_000,
                    }),
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
                ],
                avs_min_mv: None,
                avs_max_mv: None,
            }));

        assert_eq!(
            manual_pps.thermal_plant_source_limits(),
            Some((5_000, 21_000, 3_000))
        );
    }

    #[test]
    fn thermal_plant_auto_job_requires_a_pps_range_that_covers_20v() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            ..CalibrationRuntimeState::default()
        };
        let mut memory_config = MemoryConfig::default();
        for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
            memory_config.heater_curve_raw_observations.points[index] =
                Some(HeaterCurveRawObservation {
                    raw_rtd_adc_mv,
                    heater_voltage_mv: 20_000,
                    heater_current_ma: 3_000,
                    resistance_milliohms: 4_000,
                });
        }
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: false,
                pps_min_mv: Some(20_100),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }));

        assert_eq!(
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut manual_pps,
            ),
            Err(ManualPpsError::ThermalPlantSourceUnsupported)
        );
        assert_eq!(calibration.job.status, CalibrationJobStatus::Idle);

        let mut below_current_floor =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(2_999),
                ..Default::default()
            }));
        assert_eq!(
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut below_current_floor,
            ),
            Err(ManualPpsError::ThermalPlantSourceUnsupported)
        );
    }

    #[test]
    fn thermal_plant_auto_job_starts_one_transient_run_for_3a_and_5a_pps() {
        for (max_mv, max_ma) in [(20_000, 3_000), (21_000, 5_000)] {
            let mut calibration = CalibrationRuntimeState {
                mode: CalibrationMode::ThermalPlant,
                ..CalibrationRuntimeState::default()
            };
            let mut memory_config = MemoryConfig::default();
            for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
                memory_config.heater_curve_raw_observations.points[index] =
                    Some(HeaterCurveRawObservation {
                        raw_rtd_adc_mv,
                        heater_voltage_mv: max_mv,
                        heater_current_ma: max_ma,
                        resistance_milliohms: 4_000,
                    });
            }
            let mut manual_pps =
                ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                    pps_covers_20v: true,
                    pps_min_mv: Some(5_000),
                    pps_max_mv: Some(max_mv),
                    pps_max_ma: Some(max_ma),
                    ..Default::default()
                }));
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut manual_pps,
            )
            .unwrap();

            for _ in 0..THERMAL_PLANT_AMBIENT_TICKS {
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut manual_pps,
                    250,
                    0,
                    25.0,
                    max_ma,
                    max_mv.into(),
                    0,
                );
            }
            assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
            assert!(calibration.heater_enabled);
            assert_eq!(calibration.model_target_temp_c, None);
            assert_eq!(calibration.job_data, Some(CalibrationJobData::ThermalPlant));
            assert_eq!(
                test_thermal_plant_phase(),
                Some(ThermalPlantAutoPhase::Heating)
            );
            for (raw_rtd_adc_mv, temp_c) in [(400, 60.0), (700, 140.0), (1_100, 215.0)] {
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut manual_pps,
                    raw_rtd_adc_mv,
                    0,
                    temp_c,
                    max_ma,
                    max_mv.into(),
                    100,
                );

                assert_eq!(calibration.job.status, CalibrationJobStatus::Running);
                assert_eq!(calibration.job.next_request_mv, Some(max_mv));
                assert_eq!(calibration.pps_mv, Some(max_mv));
                assert_eq!(
                    thermal_plant_calibration_snapshot(temp_c, calibration.heater_enabled)
                        .duty_percent,
                    100
                );
                assert_eq!(
                    test_thermal_plant_phase(),
                    Some(ThermalPlantAutoPhase::Heating)
                );
            }
        }
    }

    #[test]
    fn transient_thermal_fit_recovers_a_physical_model_from_heat_and_cool_trace() {
        const SYNTHETIC_TARGET_MARGIN_C: f32 = 2.0;

        fn raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
            let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
            (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
                / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
                .round() as u16
        }

        let mut memory_config = MemoryConfig {
            commissioning_required: false,
            ..MemoryConfig::default()
        };
        memory_config.active_heater_curve.points[0] = Some(HeaterCurvePoint {
            temp_centi_c: 2_500,
            resistance_milliohms: 4_000,
        });
        memory_config.active_heater_curve.points[1] = Some(HeaterCurvePoint {
            temp_centi_c: 22_000,
            resistance_milliohms: 6_000,
        });
        for (index, (temp_c, resistance_milliohms)) in
            [(100.0, 4_800), (200.0, 5_800)].into_iter().enumerate()
        {
            memory_config.heater_curve_raw_observations.points[index] =
                Some(HeaterCurveRawObservation {
                    raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(temp_c),
                    heater_voltage_mv: 20_000,
                    heater_current_ma: 3_000,
                    resistance_milliohms,
                });
        }

        let ambient_temp_c = 25.0_f32;
        let capacity_mj_per_c = 100_000.0_f32;
        let convection_mw_per_c = 100.0_f32;
        let radiation_mw_per_k4 = 0.0000005_f32;
        let mut samples = [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_100mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
        samples[0] = ThermalPlantTransientSample {
            elapsed_ticks: 1,
            raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(ambient_temp_c),
            heater_voltage_100mv: 0,
            duty_percent: 0,
        };
        let mut sample_count = 1usize;
        let mut temperature_c = ambient_temp_c;
        let mut heating = true;
        let mut last_saved_temp_c = ambient_temp_c;
        for tick in 2..=60_000_u16 {
            let reached_cutoff =
                heating && temperature_c >= THERMAL_PLANT_TARGET_TEMP_C + SYNTHETIC_TARGET_MARGIN_C;
            let duty_percent = u8::from(heating) * 100;
            if sample_count < 24
                || (temperature_c - last_saved_temp_c).abs() >= THERMAL_PLANT_TRACE_MIN_TEMP_STEP_C
                || reached_cutoff
                || (!heating
                    && temperature_c
                        <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C - SYNTHETIC_TARGET_MARGIN_C)
            {
                if sample_count >= THERMAL_PLANT_TRANSIENT_MAX_SAMPLES {
                    panic!("synthetic trace exceeded fixed capacity");
                }
                samples[sample_count] = ThermalPlantTransientSample {
                    elapsed_ticks: tick,
                    raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(temperature_c),
                    heater_voltage_100mv: if heating { 200 } else { 0 },
                    duty_percent,
                };
                sample_count += 1;
                last_saved_temp_c = temperature_c;
            }
            if reached_cutoff {
                heating = false;
                last_saved_temp_c = f32::MIN;
                continue;
            }
            if !heating
                && temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C - SYNTHETIC_TARGET_MARGIN_C
            {
                break;
            }
            let resistance_ohms =
                estimated_heater_resistance_ohms(temperature_c, None, &memory_config);
            let power_mw = if heating {
                20.0 * 20.0 / resistance_ohms * 1_000.0
            } else {
                0.0
            };
            let temperature_k = temperature_c + 273.15;
            let ambient_k = ambient_temp_c + 273.15;
            let losses_mw = convection_mw_per_c * (temperature_c - ambient_temp_c)
                + radiation_mw_per_k4 * (temperature_k.powi(4) - ambient_k.powi(4));
            temperature_c += (power_mw - losses_mw) / capacity_mj_per_c * 0.05;
        }
        assert!(sample_count >= 24);
        let (transaction, residual) = fit_thermal_plant_transient(
            0x5452_4e53,
            raw_rtd_adc_mv_for_temp(ambient_temp_c),
            &samples,
            sample_count as u8,
            None,
            &memory_config,
        )
        .expect("synthetic trace fits");
        let projection = thermal_plant_projection_from_transient(&transaction).unwrap();

        assert!(residual <= 0.20);
        assert!((projection.thermal_capacity_mj_per_c - capacity_mj_per_c).abs() < 40_000.0);
        assert!((projection.convection_mw_per_c - convection_mw_per_c).abs() < 80.0);
        assert!(projection.radiation_mw_per_k4 >= 0.0);
        assert_eq!(transaction.samples[0].duty_percent, 0);
        assert_eq!(transaction.samples[1].duty_percent, 100);
        assert!(
            transaction.samples[..usize::from(transaction.sample_count)]
                .iter()
                .any(|sample| sample.duty_percent == 0)
        );
        assert!(thermal_plant_transient_trace_reaches_targets(
            &transaction,
            &memory_config
        ));

        let mut quantized_trace = samples;
        for (index, sample) in quantized_trace[..sample_count].iter_mut().enumerate() {
            if index > 0 && index + 1 < sample_count {
                sample.raw_rtd_adc_mv = if index % 2 == 0 {
                    sample.raw_rtd_adc_mv.saturating_add(1)
                } else {
                    sample.raw_rtd_adc_mv.saturating_sub(1)
                };
            }
        }
        let (_, quantized_residual) = fit_thermal_plant_transient(
            0x5155_414e,
            raw_rtd_adc_mv_for_temp(ambient_temp_c),
            &quantized_trace,
            sample_count as u8,
            None,
            &memory_config,
        )
        .expect("bounded ADC quantization must still fit");
        assert!(quantized_residual <= 0.20);

        memory_config.thermal_plant_transient_active = Some(transaction);
        memory_config.heater_curve_transaction_id = Some(transaction.transaction_id);
        let manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }));
        assert!(thermal_model_heater_allowed(
            &memory_config,
            CalibrationRuntimeState::default(),
            manual_pps
        ));
        let valid_snapshot = thermal_plant_run_snapshot_wire(
            &CalibrationRuntimeState::default(),
            &memory_config,
            &CalibrationThermalPlantWorkspace::default(),
            0,
            ambient_temp_c,
            0,
            0,
        );
        assert_eq!(
            valid_snapshot
                .active_result
                .as_ref()
                .map(|result| result.transaction_id),
            Some(transaction.transaction_id)
        );
        let mut unrelated_curve = memory_config.clone();
        unrelated_curve.heater_curve_transaction_id = Some(transaction.transaction_id + 1);
        assert!(!thermal_model_heater_allowed(
            &unrelated_curve,
            CalibrationRuntimeState::default(),
            manual_pps
        ));
        let unrelated_wire = thermal_plant_runtime_wire(&unrelated_curve);
        assert_eq!(unrelated_wire.state.as_str(), "invalid");
        assert!(!unrelated_wire.projection_valid);
        assert!(thermal_plant_projection_for_runtime(&unrelated_curve).is_none());
        let mut missing_raw_curve = memory_config.clone();
        missing_raw_curve.heater_curve_raw_observations = HeaterCurveRawObservations::default();
        assert!(!thermal_model_heater_allowed(
            &missing_raw_curve,
            CalibrationRuntimeState::default(),
            manual_pps
        ));
        let split_apdo_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(5_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 11_000,
                        max_ma: 5_000,
                    }),
                    Some(ch224q::PpsApdo {
                        min_mv: 20_000,
                        max_mv: 21_000,
                        max_ma: 1_000,
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
        assert!(!thermal_model_heater_allowed(
            &memory_config,
            CalibrationRuntimeState::default(),
            split_apdo_pps
        ));
        let mut stale_transaction = transaction;
        stale_transaction.projection.thermal_capacity_mj_per_c_bits = 200_000.0_f32.to_bits();
        memory_config.thermal_plant_transient_active = Some(stale_transaction);
        memory_config.heater_curve_raw_observations.points[1]
            .as_mut()
            .expect("second raw curve observation")
            .resistance_milliohms += 20;
        assert!(rebuild_transient_thermal_plant_for_current_inputs(
            &mut memory_config
        ));
        let rebuilt_transaction = memory_config
            .thermal_plant_transient_active
            .expect("raw trace refits");
        assert_eq!(rebuilt_transaction.samples, transaction.samples);
        assert_ne!(
            rebuilt_transaction
                .projection
                .thermal_capacity_mj_per_c_bits,
            stale_transaction.projection.thermal_capacity_mj_per_c_bits
        );

        let persisted_record = MemoryRecord {
            sequence: 1,
            config: memory_config.clone(),
        };
        let mut persisted_bytes = [0_u8; MEMORY_SLOT_SIZE];
        let persisted_len = encode_memory_record(&persisted_record, &mut persisted_bytes)
            .expect("transient record encodes");
        let persisted_transaction = decode_memory_record(&persisted_bytes[..persisted_len])
            .expect("transient record decodes")
            .config
            .thermal_plant_transient_active
            .expect("transient model persists");
        let persisted_last =
            persisted_transaction.samples[usize::from(persisted_transaction.sample_count) - 1];
        assert_eq!(persisted_last.duty_percent, 0);
        assert!(
            projected_rtd_temperature_c(&memory_config, persisted_last.raw_rtd_adc_mv)
                .is_some_and(|temperature_c| temperature_c <= THERMAL_PLANT_COOL_COMPLETE_TEMP_C)
        );

        let mut nonterminal_cooldown = transaction;
        let append_index = usize::from(nonterminal_cooldown.sample_count);
        assert!(append_index < THERMAL_PLANT_TRANSIENT_MAX_SAMPLES);
        let previous = nonterminal_cooldown.samples[append_index - 1];
        nonterminal_cooldown.samples[append_index] = ThermalPlantTransientSample {
            elapsed_ticks: previous.elapsed_ticks.saturating_add(1),
            raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(100.0),
            heater_voltage_100mv: 0,
            duty_percent: 0,
        };
        nonterminal_cooldown.sample_count = nonterminal_cooldown.sample_count.saturating_add(1);
        assert!(thermal_plant_projection_from_transient(&nonterminal_cooldown).is_some());
        assert!(!thermal_plant_transient_trace_reaches_targets(
            &nonterminal_cooldown,
            &memory_config
        ));

        let mut incomplete_cooldown = transaction;
        for sample in incomplete_cooldown.samples[..usize::from(incomplete_cooldown.sample_count)]
            .iter_mut()
            .filter(|sample| sample.duty_percent == 0)
        {
            sample.raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(90.0);
        }
        assert!(thermal_plant_projection_from_transient(&incomplete_cooldown).is_some());
        assert!(!thermal_plant_transient_trace_reaches_targets(
            &incomplete_cooldown,
            &memory_config
        ));
        memory_config.thermal_plant_transient_active = Some(incomplete_cooldown);
        assert!(!thermal_model_heater_allowed(
            &memory_config,
            CalibrationRuntimeState::default(),
            manual_pps
        ));
        let invalid_snapshot = thermal_plant_run_snapshot_wire(
            &CalibrationRuntimeState::default(),
            &memory_config,
            &CalibrationThermalPlantWorkspace::default(),
            0,
            ambient_temp_c,
            0,
            0,
        );
        assert!(invalid_snapshot.active_result.is_none());

        let mut below_target = transaction;
        let mut saw_powered_sample = false;
        for sample in below_target.samples[..usize::from(below_target.sample_count)].iter_mut() {
            if sample.duty_percent > 0 {
                if saw_powered_sample {
                    sample.raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(215.0);
                }
                saw_powered_sample = true;
            } else {
                sample.raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(80.0);
            }
        }
        assert!(thermal_plant_projection_from_transient(&below_target).is_some());
        assert!(!thermal_plant_transient_trace_reaches_targets(
            &below_target,
            &memory_config
        ));
        memory_config.thermal_plant_transient_active = Some(below_target);
        assert!(!thermal_model_heater_allowed(
            &memory_config,
            CalibrationRuntimeState::default(),
            manual_pps
        ));

        let mut cold_baseline = transaction;
        cold_baseline.samples[0].raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(-50.0);
        assert!(thermal_plant_projection_from_transient(&cold_baseline).is_some());
        assert!(!thermal_plant_transient_trace_reaches_targets(
            &cold_baseline,
            &memory_config
        ));
        assert!(
            fit_thermal_plant_transient(
                cold_baseline.transaction_id,
                cold_baseline.ambient_raw_rtd_adc_mv,
                &cold_baseline.samples,
                cold_baseline.sample_count,
                None,
                &memory_config,
            )
            .is_none()
        );
    }

    #[test]
    fn transient_thermal_fit_accepts_the_live_device_trace_shape() {
        fn raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
            let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
            (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
                / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
                .round() as u16
        }

        let memory_config = MemoryConfig {
            commissioning_required: false,
            ..MemoryConfig::default()
        };
        let mut preview_curve = HeaterCurveConfig::default();
        for (index, (temp_centi_c, resistance_milliohms)) in [
            (0, 2_948),
            (2_000, 3_200),
            (10_051, 4_213),
            (14_075, 4_719),
            (17_570, 5_158),
            (20_615, 5_541),
        ]
        .into_iter()
        .enumerate()
        {
            preview_curve.points[index] = Some(HeaterCurvePoint {
                temp_centi_c,
                resistance_milliohms,
            });
        }

        // This is the public temperature trace from the physical device run:
        // 50 ms startup samples, a 220C cutoff, and passive cooling to 80C.
        let trace = [
            (40, 32.67, 0),
            (41, 32.67, 100),
            (42, 32.67, 100),
            (43, 32.67, 100),
            (44, 32.67, 100),
            (45, 32.67, 100),
            (46, 32.67, 100),
            (47, 32.67, 100),
            (48, 33.08, 100),
            (49, 32.67, 100),
            (50, 32.67, 100),
            (51, 32.67, 100),
            (52, 33.08, 100),
            (53, 33.08, 100),
            (54, 33.08, 100),
            (55, 33.08, 100),
            (56, 33.08, 100),
            (57, 33.49, 100),
            (58, 33.08, 100),
            (59, 33.49, 100),
            (60, 33.49, 100),
            (61, 33.90, 100),
            (62, 33.90, 100),
            (63, 33.90, 100),
            (104, 38.01, 100),
            (126, 41.75, 100),
            (149, 45.94, 100),
            (175, 50.17, 100),
            (192, 54.02, 100),
            (213, 58.33, 100),
            (236, 62.26, 100),
            (255, 66.66, 100),
            (274, 70.66, 100),
            (296, 74.70, 100),
            (315, 78.77, 100),
            (335, 82.43, 100),
            (356, 86.58, 100),
            (375, 90.77, 100),
            (397, 95.00, 100),
            (420, 99.27, 100),
            (442, 103.59, 100),
            (465, 107.46, 100),
            (489, 111.85, 100),
            (513, 115.79, 100),
            (534, 119.77, 100),
            (558, 123.78, 100),
            (585, 128.34, 100),
            (605, 131.92, 100),
            (633, 136.04, 100),
            (660, 140.20, 100),
            (688, 144.40, 100),
            (711, 148.63, 100),
            (741, 152.37, 100),
            (771, 156.68, 100),
            (799, 161.03, 100),
            (830, 165.42, 100),
            (865, 169.29, 100),
            (900, 173.76, 100),
            (928, 177.70, 100),
            (971, 181.68, 100),
            (1004, 186.26, 100),
            (1042, 189.73, 100),
            (1082, 194.39, 100),
            (1122, 198.51, 100),
            (1165, 202.66, 100),
            (1220, 206.85, 100),
            (1251, 210.46, 100),
            (1304, 214.72, 100),
            (1358, 219.01, 100),
            (1379, 220.24, 100),
            (1380, 224.58, 0),
            (1559, 220.24, 0),
            (1643, 215.94, 0),
            (1713, 211.68, 0),
            (1754, 208.05, 0),
            (1811, 203.85, 0),
            (1871, 199.69, 0),
            (1930, 195.56, 0),
            (1984, 191.47, 0),
            (2030, 187.42, 0),
            (2082, 183.39, 0),
            (2146, 179.40, 0),
            (2193, 175.44, 0),
            (2252, 170.96, 0),
            (2329, 167.07, 0),
            (2414, 163.22, 0),
            (2474, 158.85, 0),
            (2562, 155.06, 0),
            (2672, 150.76, 0),
            (2765, 147.04, 0),
            (2852, 142.82, 0),
            (2976, 138.63, 0),
            (3093, 134.49, 0),
            (3206, 130.38, 0),
            (3313, 126.31, 0),
            (3448, 122.28, 0),
            (3608, 118.28, 0),
            (3733, 114.31, 0),
            (3939, 109.89, 0),
            (4131, 106.00, 0),
            (4306, 102.14, 0),
            (4472, 97.84, 0),
            (4692, 93.59, 0),
            (4926, 89.84, 0),
            (5131, 85.65, 0),
            (5488, 81.51, 0),
            (5647, 80.14, 0),
        ];
        assert!(trace.len() <= THERMAL_PLANT_TRANSIENT_MAX_SAMPLES);

        let mut samples = [ThermalPlantTransientSample {
            elapsed_ticks: 0,
            raw_rtd_adc_mv: 0,
            heater_voltage_100mv: 0,
            duty_percent: 0,
        }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES];
        for (index, (elapsed_ticks, temp_c, duty_percent)) in trace.into_iter().enumerate() {
            samples[index] = ThermalPlantTransientSample {
                elapsed_ticks,
                raw_rtd_adc_mv: raw_rtd_adc_mv_for_temp(temp_c),
                heater_voltage_100mv: 200,
                duty_percent,
            };
        }

        let recorded_terminal_temp_c =
            projected_rtd_temperature_c(&memory_config, samples[trace.len() - 1].raw_rtd_adc_mv)
                .expect("fixture terminal temperature projects");
        assert!(recorded_terminal_temp_c > THERMAL_PLANT_COOL_COMPLETE_TEMP_C);
        assert!(!thermal_plant_cooling_complete(
            79.99,
            recorded_terminal_temp_c
        ));
        assert!(
            fit_thermal_plant_transient(
                0x4c49_5645,
                raw_rtd_adc_mv_for_temp(32.67),
                &samples,
                trace.len() as u8,
                Some(&preview_curve),
                &memory_config,
            )
            .is_none(),
            "a nonterminal recorded trace must not fit"
        );

        let mut terminal_samples = samples;
        terminal_samples[trace.len() - 1].raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(79.73);
        let terminal_temp_c = projected_rtd_temperature_c(
            &memory_config,
            terminal_samples[trace.len() - 1].raw_rtd_adc_mv,
        )
        .expect("terminal fixture temperature projects");
        assert!(thermal_plant_cooling_complete(79.73, terminal_temp_c));
        let (_, residual) = fit_thermal_plant_transient(
            0x4c49_5645,
            raw_rtd_adc_mv_for_temp(32.67),
            &terminal_samples,
            trace.len() as u8,
            Some(&preview_curve),
            &memory_config,
        )
        .expect("live device-shaped trace must produce a physical model");
        assert!(residual <= 0.20);
    }

    #[test]
    fn runtime_ready_boot_stage_matches_post_flash_contract() {
        assert_eq!(RUNTIME_READY_BOOT_STAGE_LINE, b"boot_stage=runtime_ready\n");
    }

    #[test]
    fn thermal_plant_auto_completes_one_transient_cycle_for_3a_and_5a_pps() {
        fn raw_rtd_adc_mv_for_temp(temp_c: f32) -> u16 {
            let resistance_ohms = pt1000_resistance_ohms_at(temp_c);
            (f32::from(RTD_DIVIDER_SUPPLY_MV) * resistance_ohms
                / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms))
                .round() as u16
        }

        for source_current_ma in [3_000, 5_000] {
            let mut calibration = CalibrationRuntimeState {
                mode: CalibrationMode::ThermalPlant,
                ..CalibrationRuntimeState::default()
            };
            let mut memory_config = MemoryConfig::default();
            let mut manual_pps =
                ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                    pps_covers_20v: true,
                    pps_min_mv: Some(5_000),
                    pps_max_mv: Some(20_000),
                    pps_max_ma: Some(source_current_ma),
                    ..Default::default()
                }));
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut manual_pps,
            )
            .unwrap();

            let ambient_temp_c = 25.0_f32;
            let capacity_mj_per_c = 30_000.0_f32;
            let convection_mw_per_c = 100.0_f32;
            let radiation_mw_per_k4 = 0.00000005_f32;
            let mut temperature_c = ambient_temp_c;
            for _ in 0..(THERMAL_PLANT_AMBIENT_TICKS as usize
                + THERMAL_PLANT_HEAT_TIMEOUT_TICKS as usize
                + THERMAL_PLANT_COOL_TIMEOUT_TICKS as usize)
            {
                let heater_duty_percent = u8::from(calibration.heater_enabled) * 100;
                match test_thermal_plant_phase() {
                    Some(ThermalPlantAutoPhase::Heating) => assert_eq!(heater_duty_percent, 100),
                    Some(ThermalPlantAutoPhase::Cooling) => assert_eq!(heater_duty_percent, 0),
                    Some(ThermalPlantAutoPhase::Ambient) | None => {}
                }
                let source_mv = manual_pps.target_mv.unwrap_or(0);
                let resistance_ohms =
                    estimated_heater_resistance_ohms(temperature_c, None, &memory_config);
                let pd_current_ma = if heater_duty_percent > 0 {
                    ((f32::from(source_mv) / resistance_ohms).round() as u16).min(source_current_ma)
                } else {
                    0
                };
                let measured_heater_mv = if heater_duty_percent > 0 {
                    u32::from(source_mv)
                        .min((f32::from(pd_current_ma) * resistance_ohms).round() as u32)
                } else {
                    0
                };
                let raw_rtd_adc_mv = raw_rtd_adc_mv_for_temp(temperature_c);
                let reported_temp_c =
                    projected_rtd_temperature_c(&memory_config, raw_rtd_adc_mv).unwrap();
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut manual_pps,
                    raw_rtd_adc_mv,
                    0,
                    reported_temp_c,
                    pd_current_ma,
                    measured_heater_mv,
                    heater_duty_percent,
                );
                if calibration.job.status == CalibrationJobStatus::Completed {
                    break;
                }
                if calibration.job.status == CalibrationJobStatus::Failed {
                    panic!(
                        "thermal plant job failed: {:?}, physical_temp={temperature_c}, reported_temp={reported_temp_c}, samples={}",
                        calibration.job.message, calibration.job.samples_collected
                    );
                }

                let power_mw = if heater_duty_percent > 0 {
                    ((f32::from(source_mv) / 1_000.0).powi(2) / resistance_ohms * 1_000.0)
                        .min(f32::from(source_mv) * f32::from(source_current_ma) / 1_000.0)
                } else {
                    0.0
                };
                let temperature_k = temperature_c + 273.15;
                let ambient_k = ambient_temp_c + 273.15;
                let losses_mw = convection_mw_per_c * (temperature_c - ambient_temp_c)
                    + radiation_mw_per_k4 * (temperature_k.powi(4) - ambient_k.powi(4));
                temperature_c += (power_mw - losses_mw) / capacity_mj_per_c * 0.05;
            }

            assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
            assert_eq!(calibration.mode, CalibrationMode::Off);
            assert!(!calibration.heater_enabled);
            assert!(!manual_pps.enabled);
            assert!(memory_config.thermal_plant_transient_active.is_some());
            assert!(has_calibrated_heater_resistance_curve(&memory_config));
        }
    }

    #[test]
    fn thermal_plant_transient_cuts_heat_at_220_before_the_next_output_cycle() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            heater_enabled: true,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            job_data: Some(CalibrationJobData::ThermalPlant),
            ..CalibrationRuntimeState::default()
        };
        test_install_thermal_plant_job(CalibrationThermalPlantAutoJob {
            run_id: 1,
            phase: ThermalPlantAutoPhase::Heating,
            source_max_mv: 20_000,
            source_current_ma: 3_000,
            ambient_raw_rtd_adc_mv: 250,
            idle_samples: THERMAL_PLANT_AMBIENT_TICKS,
            heater_curve: ThermalPlantCurveSampler::default(),
            elapsed_ticks: 100,
            phase_started_tick: THERMAL_PLANT_AMBIENT_TICKS.into(),
            sample_count: 1,
            last_saved_temp_c: 215.0,
            last_saved_tick: 100,
            samples: [ThermalPlantTransientSample {
                elapsed_ticks: 100,
                raw_rtd_adc_mv: 250,
                heater_voltage_100mv: 200,
                duty_percent: 100,
            }; THERMAL_PLANT_TRANSIENT_MAX_SAMPLES],
        });
        let mut memory_config = MemoryConfig::default();
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }));
        manual_pps
            .enable(ManualPpsOwner::Calibration, 20_000, Some(3_000))
            .unwrap();

        update_calibration_job_state(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            1_200,
            0,
            THERMAL_PLANT_TARGET_TEMP_C,
            3_000,
            20_000,
            100,
        );

        assert!(!calibration.heater_enabled);
        assert!(manual_pps.enabled);
        assert_eq!(manual_pps.target_mv, Some(20_000));
        assert!(!take_immediate_heater_disarm(&mut calibration));
        assert_eq!(calibration.job_data, Some(CalibrationJobData::ThermalPlant));
        assert_eq!(
            test_thermal_plant_phase(),
            Some(ThermalPlantAutoPhase::Cooling)
        );
        assert_eq!(
            thermal_plant_calibration_snapshot(220.0, false).duty_percent,
            0
        );
        assert_eq!(
            thermal_plant_calibration_snapshot(215.0, true).duty_percent,
            100
        );
        assert!(thermal_plant_output_must_be_off(calibration, true, 220.0));
    }

    #[test]
    fn thermal_plant_live_rtd_cutoff_ignores_a_lagging_guarded_temperature() {
        let calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            heater_enabled: true,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };

        let calibration_temp =
            thermal_plant_calibration_temperature_c(calibration, Some(229.5), 158.4);
        assert_eq!(calibration_temp, 229.5);
        assert!(thermal_plant_output_must_be_off(
            calibration,
            true,
            calibration_temp
        ));
        assert_eq!(
            thermal_plant_calibration_temperature_c(
                CalibrationRuntimeState::default(),
                Some(229.5),
                158.4
            ),
            158.4
        );
    }

    #[test]
    fn thermal_plant_job_fails_before_sampling_after_a_manual_pps_override() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            ..CalibrationRuntimeState::default()
        };
        let mut memory_config = MemoryConfig::default();
        for (index, raw_rtd_adc_mv) in [240, 460].into_iter().enumerate() {
            memory_config.heater_curve_raw_observations.points[index] =
                Some(HeaterCurveRawObservation {
                    raw_rtd_adc_mv,
                    heater_voltage_mv: 20_000,
                    heater_current_ma: 3_000,
                    resistance_milliohms: 4_000,
                });
        }
        let mut manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }));
        calibration_job_start(
            &mut calibration,
            CalibrationJobKind::ThermalPlant,
            &mut memory_config,
            &mut manual_pps,
        )
        .unwrap();
        manual_pps
            .enable(ManualPpsOwner::Debug, 20_000, Some(3_000))
            .unwrap();

        update_calibration_job_state(
            &mut calibration,
            &mut memory_config,
            &mut manual_pps,
            0,
            0,
            20.0,
            0,
            20_000,
            0,
        );

        assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
        assert_eq!(calibration.job.samples_collected, 0);
        assert_eq!(memory_config.thermal_plant_active, None);
    }

    #[test]
    fn thermal_plant_job_disarms_on_missing_powered_electrical_observation() {
        for (latest_vin_mv, pd_current_ma) in [(0, 3_000), (20_000, 0)] {
            let mut calibration = CalibrationRuntimeState {
                mode: CalibrationMode::ThermalPlant,
                ..CalibrationRuntimeState::default()
            };
            let mut memory_config = MemoryConfig::default();
            let mut manual_pps =
                ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                    pps_covers_20v: true,
                    pps_min_mv: Some(5_000),
                    pps_max_mv: Some(20_000),
                    pps_max_ma: Some(3_000),
                    ..Default::default()
                }));
            calibration_job_start(
                &mut calibration,
                CalibrationJobKind::ThermalPlant,
                &mut memory_config,
                &mut manual_pps,
            )
            .unwrap();

            for _ in 0..THERMAL_PLANT_AMBIENT_TICKS {
                update_calibration_job_state(
                    &mut calibration,
                    &mut memory_config,
                    &mut manual_pps,
                    250,
                    0,
                    25.0,
                    3_000,
                    20_000,
                    0,
                );
            }
            assert!(calibration.heater_enabled);

            update_calibration_job_state(
                &mut calibration,
                &mut memory_config,
                &mut manual_pps,
                260,
                0,
                26.0,
                pd_current_ma,
                latest_vin_mv,
                100,
            );

            assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
            assert!(!calibration.heater_enabled);
            assert!(!manual_pps.enabled);
            assert!(take_immediate_heater_disarm(&mut calibration));
            assert_eq!(memory_config.thermal_plant_transient_active, None);
        }
    }

    #[test]
    fn transient_curve_projection_includes_low_temperature_anchors() {
        let mut bins = ThermalPlantCurveSampler::default().bins;
        bins[0].observe(100.0, 3.911);
        bins[1].observe(140.0, 3.918);
        bins[2].observe(175.0, 3.924);
        bins[3].observe(210.0, 3.929);

        let preview = heater_curve_from_transient_bins(&bins).unwrap();

        assert_eq!(
            preview.points[0],
            Some(default_heater_curve_point(HEATER_CURVE_COLD_ANCHOR_TEMP_C))
        );
        assert_eq!(
            preview.points[1],
            Some(default_heater_curve_point(HEATER_CURVE_R20_ANCHOR_TEMP_C))
        );
        assert_eq!(
            preview.points[2].map(|point| point.temp_centi_c),
            Some(10_000)
        );
        assert!(preview.points[5].is_some());
        assert!(preview.points[6].is_none());
    }

    #[test]
    fn transient_curve_sampling_requires_each_temperature_band() {
        let mut job = ThermalPlantCurveSampler::default();
        for _ in 0..THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN {
            job.cold_bin.observe(100.0, 3.8);
            for bin in &mut job.bins {
                bin.observe((bin.min_temp_c + bin.max_temp_c) / 2.0, 3.9);
            }
        }

        assert!(thermal_plant_curve_samples_ready(&job));

        job.bins[3].samples = THERMAL_PLANT_CURVE_MIN_SAMPLES_PER_BIN - 1;
        assert!(!thermal_plant_curve_samples_ready(&job));
    }

    #[test]
    fn transient_curve_projection_never_underestimates_nominal_heater_model() {
        let mut bins = ThermalPlantCurveSampler::default().bins;
        bins[0].observe(100.0, 3.911);
        bins[1].observe(140.0, 3.918);
        bins[2].observe(175.0, 3.924);
        bins[3].observe(210.0, 3.929);

        let preview = heater_curve_from_transient_bins(&bins).unwrap();

        for point in preview.points.into_iter().flatten() {
            let temp_c = f32::from(point.temp_centi_c) / 100.0;
            let expected_floor =
                round_to_u16_nonnegative(default_estimated_heater_resistance_ohms(temp_c) * 1000.0);
            assert!(point.resistance_milliohms >= expected_floor);
        }
    }

    #[test]
    fn transient_curve_projection_does_not_clamp_low_temp_voltage_to_first_hot_bin() {
        let mut bins = ThermalPlantCurveSampler::default().bins;
        bins[0].observe(100.0, 3.911);
        bins[1].observe(140.0, 3.918);
        bins[2].observe(175.0, 3.924);
        bins[3].observe(210.0, 3.929);

        let preview = heater_curve_from_transient_bins(&bins).unwrap();
        let memory_config = MemoryConfig::default();

        assert_eq!(
            heater_safe_max_mv_for_temp(20.0, 5_000, 24_000, Some(&preview), &memory_config),
            16_000
        );
        assert_eq!(
            heater_safe_max_mv_for_temp(60.0, 5_000, 24_000, Some(&preview), &memory_config),
            18_500
        );
        assert_eq!(
            heater_safe_max_mv_for_temp(220.0, 4_800, 21_000, Some(&preview), &memory_config),
            21_000
        );
    }

    #[test]
    fn memory_record_write_chunk_len_keeps_i2c_frames_small_and_page_aligned() {
        assert_eq!(memory_record_write_chunk_len(0x0400, 128), 16);
        assert_eq!(memory_record_write_chunk_len(0x0418, 128), 8);
        assert_eq!(memory_record_write_chunk_len(0x041f, 128), 1);
        assert_eq!(memory_record_write_chunk_len(0x0420, 7), 7);
    }

    #[test]
    fn raw_eeprom_writes_split_at_page_boundaries_from_any_offset() {
        assert_eq!(eeprom_maintenance_write_chunk_len(0x001f, 2), 1);
        assert_eq!(eeprom_maintenance_write_chunk_len(0x0020, 17), 16);
        assert_eq!(eeprom_maintenance_write_chunk_len(0x003f, 16), 1);
    }

    #[test]
    fn non_blank_eeprom_without_a_valid_record_is_incompatible() {
        let blank = [0xff; EEPROM_UNUSED_GAP_LEN];
        let mut incompatible_gap = blank;
        incompatible_gap[0] = 0x7e;

        assert!(!eeprom_data_is_incompatible(
            false,
            eeprom_bytes_contain_data(&blank)
        ));
        assert!(eeprom_data_is_incompatible(
            false,
            eeprom_bytes_contain_data(&incompatible_gap)
        ));
        assert!(!eeprom_data_is_incompatible(
            true,
            eeprom_bytes_contain_data(&incompatible_gap)
        ));
    }

    #[test]
    fn raw_eeprom_maintenance_locks_writes_and_clears_erased_runtime_state() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut config = MemoryConfig {
            target_temp_c: 180,
            ..MemoryConfig::default()
        };
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::HeaterCurve,
            pps_enabled: true,
            pps_mv: Some(20_000),
            pps_ma: Some(3_000),
            heater_enabled: true,
            ..CalibrationRuntimeState::default()
        };
        let mut manual_pps = ManualPpsState {
            enabled: true,
            owner: ManualPpsOwner::Debug,
            target_mv: Some(20_000),
            target_ma: Some(3_000),
            applied_mv: Some(20_000),
            ..ManualPpsState::default()
        };
        let sequence = 9;
        let mut commit_due_ms = Some(20);

        begin_mutating_eeprom_maintenance(
            &mut state,
            &mut calibration,
            &mut manual_pps,
            &mut commit_due_ms,
        );
        assert!(raw_eeprom_operation_mutates(EepromMaintenanceOp::Write));
        assert!(raw_eeprom_operation_mutates(EepromMaintenanceOp::Erase));
        assert!(!raw_eeprom_operation_mutates(EepromMaintenanceOp::Read));
        assert!(state.eeprom_data_incompatible);
        state.eeprom_data_incompatible = false;
        state.eeprom_required = false;
        commit_due_ms = Some(42);
        mark_eeprom_required(
            &mut state,
            &mut calibration,
            &mut manual_pps,
            &mut commit_due_ms,
        );
        assert!(state.eeprom_required);
        assert!(state.persistence_locked());
        assert_eq!(commit_due_ms, None);
        assert!(!manual_pps.enabled);
        assert!(manual_pps.automatic_restore_pending);
        assert_eq!(calibration.mode, CalibrationMode::Off);
        assert!(!calibration.heater_enabled);
        assert!(calibration.immediate_heater_disarm_pending);
        assert_eq!(commit_due_ms, None);

        apply_successful_eeprom_maintenance_operation(
            EepromMaintenanceOp::Write,
            &mut state,
            &mut config,
            &mut commit_due_ms,
        );
        assert!(state.eeprom_data_incompatible);
        assert_eq!(config.target_temp_c, 180);

        state.eeprom_data_incompatible = false;
        state.eeprom_required = false;
        apply_successful_eeprom_maintenance_operation(
            EepromMaintenanceOp::Erase,
            &mut state,
            &mut config,
            &mut commit_due_ms,
        );
        assert_eq!(config, MemoryConfig::default());
        assert_eq!(sequence, 9);
        assert_eq!(commit_due_ms, None);
        assert_eq!(state.target_temp_c, MemoryConfig::default().target_temp_c);
        assert!(!state.eeprom_data_incompatible);

        commit_due_ms = Some(42);
        discard_deferred_memory_commit_for_incompatible_eeprom(true, &mut commit_due_ms);
        assert_eq!(commit_due_ms, None);
    }

    #[test]
    fn heater_control_saturates_when_far_below_target() {
        let mut controller = HeaterController::new();
        let snapshot = controller.update(380, 25.0, true, None);

        assert_eq!(snapshot.duty_percent, 100);
        assert!(snapshot.error_c > 300.0);
        assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn warmup_soft_start_runs_once_per_arm_and_target_change() {
        let mut controller = HeaterController::new();
        let armed = controller.update_at(140, 25.0, true, None, 1_000);
        assert_eq!(armed.warmup_soft_start_percent, 0);

        let mid_ramp = controller.update_at(140, 25.0, true, None, 1_500);
        assert_eq!(mid_ramp.warmup_soft_start_percent, 50);

        let completed = controller.update_at(140, 25.0, true, None, 2_000);
        assert_eq!(completed.warmup_soft_start_percent, 100);

        let target_changed = controller.update_at(180, 25.0, true, None, 3_000);
        assert_eq!(target_changed.warmup_soft_start_percent, 0);

        let disabled = controller.update_at(180, 25.0, false, None, 4_000);
        assert_eq!(disabled.warmup_soft_start_percent, 0);

        let rearmed = controller.update_at(180, 25.0, true, None, 5_000);
        assert_eq!(rearmed.warmup_soft_start_percent, 0);
    }

    #[test]
    fn warmup_soft_start_restarts_after_approach_reentry() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 140;
        controller.heater_was_enabled = true;
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(25.0);
        controller.previous_filtered_temp_c = Some(25.0);
        controller.previous_measured_temp_c = Some(25.0);
        controller.warmup_started_at_ms = Some(0);

        let snapshot = controller.update_at(140, 25.0, true, None, 4_000);

        assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
        assert_eq!(snapshot.warmup_soft_start_percent, 0);
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
        assert!(!warmup_handoff_ready(14.2, 20.8, 20.4, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_accepts_confirmed_temperature_momentum() {
        assert!(warmup_handoff_ready(5.8, 6.4, 14.8, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_rejects_predictive_ready_when_actual_is_still_far_from_brake() {
        assert!(!warmup_handoff_ready(14.2, 14.8, 14.8, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_accepts_actual_temperature_inside_static_brake_with_bounded_filter_lag() {
        assert!(warmup_handoff_ready(4.9, 5.2, 7.8, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_rejects_actual_temperature_inside_static_brake_when_filter_is_far_behind() {
        assert!(!warmup_handoff_ready(4.9, 5.2, 20.4, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_requires_previous_actual_confirmation_inside_static_brake() {
        assert!(!warmup_handoff_ready(4.9, 20.4, 20.4, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_requires_previous_actual_confirmation_for_predictive_ready() {
        assert!(!warmup_handoff_ready(14.2, 16.1, 14.8, 5.0, 14.9));
    }

    #[test]
    fn warmup_handoff_rejects_single_sample_actual_overshoot_when_filter_lags() {
        assert!(!warmup_handoff_ready(-7.6, 11.4, 14.8, 11.0, 14.9));
    }

    #[test]
    fn heater_warmup_hands_off_early_when_rise_rate_is_high() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.filtered_temp_c = Some(53.8);
        controller.previous_filtered_temp_c = Some(53.2);
        controller.previous_measured_temp_c = Some(53.6);
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
                    warmup_reenter_centi_c: 0,
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

        let snapshot = controller.update(60, 54.2, true, Some(profile));

        assert_eq!(snapshot.phase, HeaterControlPhase::Approach);
        assert!(snapshot.error_c > 5.0);
        assert!(snapshot.duty_percent < 100);
    }

    #[test]
    fn heater_control_reduces_output_as_temperature_rises() {
        let mut controller = HeaterController::new();
        let mut snapshots = Vec::new();
        let mut now_ms = 0;
        for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2] {
            let mut snapshot = controller.update_at(100, measured, true, None, now_ms);
            for _ in 1..20 {
                now_ms += HEATER_CONTROL_INTERVAL_MS;
                snapshot = controller.update_at(100, measured, true, None, now_ms);
            }
            now_ms += HEATER_CONTROL_INTERVAL_MS;
            snapshots.push(snapshot);
        }

        assert_eq!(snapshots[0].duty_percent, 100);
        assert!(snapshots[3].duty_percent >= snapshots[4].duty_percent);
        assert!(
            snapshots[5].duty_percent < snapshots[0].duty_percent,
            "snapshots={snapshots:?}"
        );
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
    fn heater_control_keeps_full_power_warmup_during_warmup() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.phase = HeaterControlPhase::Warmup;
        controller.filtered_temp_c = Some(39.0);
        controller.previous_filtered_temp_c = Some(37.0);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 0.7,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 60,
                    brake_distance_centi_c: 1_000,
                    warmup_power_permille: 1,
                    warmup_reenter_centi_c: 0,
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
        assert_eq!(snapshot.duty_percent, 100);
        assert!(snapshot.error_c > 10.0);
    }

    #[test]
    fn heater_control_warmup_ignores_profile_power_caps() {
        let mut controller = HeaterController::new();
        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings::default(),
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 140,
                    brake_distance_centi_c: 1_000,
                    warmup_power_permille: 420,
                    warmup_reenter_centi_c: 0,
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
        assert_eq!(snapshot.duty_percent, 100);
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
                    warmup_reenter_centi_c: 0,
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
        let mut now_ms = 0;

        for measured in [25.0, 40.0, 55.0, 70.0, 82.0, 90.0, 96.0, 99.2, 100.4] {
            for _ in 0..20 {
                let _ = controller.update_at(100, measured, true, None, now_ms);
                now_ms += HEATER_CONTROL_INTERVAL_MS;
            }
        }

        let mut cooling = controller.update_at(100, 99.6, true, None, now_ms);
        for step in 1..=12 {
            now_ms += HEATER_CONTROL_INTERVAL_MS;
            let measured = 99.6 - (step as f32 * 0.06);
            cooling = controller.update_at(100, measured, true, None, now_ms);
        }
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
        let mut now_ms = 0;
        for measured in [25.0, 60.0, 80.0, 92.0, 96.0, 99.2, 99.8, 100.3] {
            for _ in 0..20 {
                let _ = controller.update_at(100, measured, true, None, now_ms);
                now_ms += HEATER_CONTROL_INTERVAL_MS;
            }
        }

        let mut near_target = controller.update_at(100, 99.95, true, None, now_ms);
        for step in 1..=12 {
            now_ms += HEATER_CONTROL_INTERVAL_MS;
            let measured = 99.95 - (step as f32 * 0.06);
            near_target = controller.update_at(100, measured, true, None, now_ms);
        }
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
    fn heater_hold_filter_lag_does_not_reheat_while_actual_temp_is_above_target_and_rising() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 100;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 8;
        controller.duty_percent = 22;
        controller.filtered_temp_c = Some(99.6);
        controller.previous_filtered_temp_c = Some(99.3);
        controller.previous_measured_temp_c = Some(99.9);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 0.4,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 100,
                    brake_distance_centi_c: 1_000,
                    warmup_power_permille: 1_000,
                    warmup_reenter_centi_c: 0,
                    approach_power_permille: 420,
                    approach_floor_power_permille: 300,
                    approach_damping_exponent_permille: 1_220,
                    approach_tail_window_centi_c: 375,
                    hold_power_permille: 220,
                    hold_reheat_power_permille: 220,
                    hold_entry_centi_c: 150,
                    hold_exit_centi_c: 120,
                    hold_on_centi_c: 10,
                    hold_off_centi_c: 180,
                    overshoot_cutoff_centi_c: 90,
                    hold_kp_permille_per_c: 20,
                    hold_ki_permille_per_c_tick: 1,
                    hold_blend_ticks: 2,
                    approach_lead_ticks: 7,
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

        let snapshot = controller.update(100, 100.21, true, Some(profile));
        assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
        assert!(snapshot.error_c < 0.0);
        assert!(snapshot.control_error_c > 0.0);
        assert!(snapshot.filtered_slope_c_per_s > 0.0);
        assert_eq!(snapshot.duty_percent, 0);
    }

    #[test]
    fn heater_hold_does_not_reheat_while_actual_and_filtered_are_above_target_and_rising() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 100;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = 8;
        controller.duty_percent = 22;
        controller.filtered_temp_c = Some(99.9);
        controller.previous_filtered_temp_c = Some(99.68);
        controller.previous_measured_temp_c = Some(100.71);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 0.99,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 100,
                    brake_distance_centi_c: 1_300,
                    warmup_power_permille: 1_000,
                    warmup_reenter_centi_c: 0,
                    approach_power_permille: 340,
                    approach_floor_power_permille: 220,
                    approach_damping_exponent_permille: 1_500,
                    approach_tail_window_centi_c: 375,
                    hold_power_permille: 220,
                    hold_reheat_power_permille: 220,
                    hold_entry_centi_c: 150,
                    hold_exit_centi_c: 120,
                    hold_on_centi_c: 10,
                    hold_off_centi_c: 50,
                    overshoot_cutoff_centi_c: 50,
                    hold_kp_permille_per_c: 20,
                    hold_ki_permille_per_c_tick: 1,
                    hold_blend_ticks: 2,
                    approach_lead_ticks: 9,
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

        let snapshot = controller.update(100, 100.25, true, Some(profile));
        assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
        assert!(snapshot.error_c < 0.0);
        assert!(snapshot.filtered_slope_c_per_s > 0.0);
        assert_eq!(snapshot.duty_percent, 0);
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
    fn heater_hold_does_not_coast_below_target_just_because_previous_output_was_zero() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 180;
        controller.phase = HeaterControlPhase::Approach;
        controller.phase_ticks = 20;
        controller.duty_percent = 0;
        controller.filtered_temp_c = Some(177.8);
        controller.previous_filtered_temp_c = Some(177.6);
        controller.filtered_slope_c_per_profile_tick = 0.2;
        controller.previous_measured_temp_c = Some(178.0);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 0.26,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                None,
                None,
                None,
                Some(ThermalControlProfilePoint {
                    target_temp_c: 180,
                    brake_distance_centi_c: 875,
                    warmup_power_permille: 950,
                    warmup_reenter_centi_c: 0,
                    approach_power_permille: 710,
                    approach_floor_power_permille: 410,
                    approach_damping_exponent_permille: 1_000,
                    approach_tail_window_centi_c: 0,
                    hold_power_permille: 420,
                    hold_reheat_power_permille: 560,
                    hold_entry_centi_c: 180,
                    hold_exit_centi_c: 70,
                    hold_on_centi_c: 25,
                    hold_off_centi_c: 225,
                    overshoot_cutoff_centi_c: 250,
                    hold_kp_permille_per_c: 20,
                    hold_ki_permille_per_c_tick: 1,
                    hold_blend_ticks: 3,
                    approach_lead_ticks: 4,
                    hold_lead_ticks: 0,
                }),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        };

        let snapshot = controller.update(180, 178.3, true, Some(profile));

        assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
        assert!(!controller.hold_coast_active);
        assert!(snapshot.duty_percent > 0);
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
                    warmup_reenter_centi_c: 0,
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
        assert!(snapshot.duty_percent >= 26);
        assert!(snapshot.duty_percent < 32);
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
    fn heater_approach_uses_hold_base_without_inheriting_reheat_floor() {
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
                    warmup_reenter_centi_c: 0,
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
        assert!(snapshot.duty_percent >= 38);
        assert!(snapshot.duty_percent < 52);
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
            warmup_reenter_error_c: 4.0,
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
            warmup_reenter_error_c: 4.0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
    fn heater_hold_prediction_does_not_zero_output_while_plate_is_still_well_below_target() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.phase = HeaterControlPhase::Hold;
        controller.phase_ticks = control_cycles_from_profile_ticks(4);
        controller.duty_percent = 0;
        controller.filtered_temp_c = Some(57.235535);
        controller.previous_filtered_temp_c = Some(57.195503);
        controller.previous_measured_temp_c = Some(57.66);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings::default(),
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 60,
                    brake_distance_centi_c: 1_400,
                    warmup_power_permille: 740,
                    warmup_reenter_centi_c: 0,
                    approach_power_permille: 450,
                    approach_floor_power_permille: 240,
                    approach_damping_exponent_permille: 4_000,
                    approach_tail_window_centi_c: 375,
                    hold_power_permille: 135,
                    hold_reheat_power_permille: 170,
                    hold_entry_centi_c: 220,
                    hold_exit_centi_c: 400,
                    hold_on_centi_c: 30,
                    hold_off_centi_c: 50,
                    overshoot_cutoff_centi_c: 50,
                    hold_kp_permille_per_c: 8,
                    hold_ki_permille_per_c_tick: 1,
                    hold_blend_ticks: 1,
                    approach_lead_ticks: 10,
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
                None,
            ],
        };

        let snapshot = controller.update(60, 57.81, true, Some(profile));

        assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
        assert!(snapshot.error_c > 2.0);
        assert!(snapshot.control_error_c > 2.0);
        assert!(snapshot.filtered_slope_c_per_s > 0.0);
        assert!(snapshot.duty_percent > 0);
    }

    #[test]
    fn heater_hold_entry_does_not_coast_far_below_target_after_zero_output_approach_sample() {
        let mut controller = HeaterController::new();
        controller.last_target_temp_c = 60;
        controller.phase = HeaterControlPhase::Approach;
        controller.phase_ticks = control_cycles_from_profile_ticks(4);
        controller.duty_percent = 0;
        controller.filtered_temp_c = Some(57.190575);
        controller.previous_filtered_temp_c = Some(57.15819);
        controller.filtered_slope_c_per_profile_tick = 0.402053;
        controller.previous_measured_temp_c = Some(57.45);

        let profile = ThermalControlProfile {
            settings: ThermalControlProfileSettings {
                temp_filter_alpha: 0.7,
                ..ThermalControlProfileSettings::default()
            },
            points: [
                Some(ThermalControlProfilePoint {
                    target_temp_c: 60,
                    brake_distance_centi_c: 1_400,
                    warmup_power_permille: 740,
                    warmup_reenter_centi_c: 0,
                    approach_power_permille: 450,
                    approach_floor_power_permille: 240,
                    approach_damping_exponent_permille: 4_000,
                    approach_tail_window_centi_c: 375,
                    hold_power_permille: 135,
                    hold_reheat_power_permille: 170,
                    hold_entry_centi_c: 220,
                    hold_exit_centi_c: 400,
                    hold_on_centi_c: 30,
                    hold_off_centi_c: 50,
                    overshoot_cutoff_centi_c: 50,
                    hold_kp_permille_per_c: 8,
                    hold_ki_permille_per_c_tick: 1,
                    hold_blend_ticks: 1,
                    approach_lead_ticks: 10,
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
                None,
            ],
        };

        let snapshot = controller.update(60, 57.99, true, Some(profile));

        assert_eq!(snapshot.phase, HeaterControlPhase::Hold);
        assert!(snapshot.error_c > 2.0);
        assert!(snapshot.control_error_c > 2.5);
        assert!(snapshot.filtered_slope_c_per_s > 0.0);
        assert!(!controller.hold_coast_active);
        assert!(snapshot.duty_percent > 0);
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
            warmup_reenter_error_c: 4.0,
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
            warmup_reenter_error_c: 4.0,
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
        assert_eq!(approach_sustain_floor_permille(target, 1.5), 320);
        assert_eq!(approach_sustain_floor_permille(target, 0.5), 140);
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
    fn heater_armed_zero_output_steps_toward_working_floor() {
        assert_eq!(
            heater_adjustable_request_mv(0, true, 11_300, 12_000, 6_100, 20_000),
            10_800
        );
        assert_eq!(
            heater_adjustable_request_mv(0, true, 6_300, 12_000, 6_100, 20_000),
            6_100
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
    fn heater_pwm_frequency_is_100hz_for_all_heater_backends() {
        assert_eq!(HEATER_PWM_FREQUENCY_HZ, 100);
    }

    #[test]
    fn heater_pwm_timer_is_representable_at_100hz() {
        let timer_counts = u32::from(HEATER_PWM_PERIOD_TICKS) + 1;
        let required_prescaler =
            MCPWM_PERIPHERAL_CLOCK_HZ / (HEATER_PWM_FREQUENCY_HZ * timer_counts) - 1;

        assert!(required_prescaler <= MCPWM_TIMER_MAX_PRESCALER);
        assert_eq!(
            MCPWM_PERIPHERAL_CLOCK_HZ / (required_prescaler + 1) / timer_counts,
            HEATER_PWM_FREQUENCY_HZ
        );
    }

    #[test]
    fn heater_physical_pwm_uses_full_duty_when_pps_matches_requested_power() {
        assert_eq!(heater_physical_pwm_percent(100, 14_000, 14_000, 100), 100);
        assert_eq!(heater_physical_pwm_percent(25, 14_000, 7_000, 100), 100);
    }

    #[test]
    fn heater_physical_pwm_reduces_power_at_pps_floor_and_during_down_ramp() {
        assert_eq!(heater_physical_pwm_percent(0, 14_000, 5_000, 100), 0);
        assert_eq!(heater_physical_pwm_percent(10, 14_000, 5_000, 100), 78);
        assert_eq!(heater_physical_pwm_percent(10, 14_000, 13_500, 100), 10);
        assert!(heater_physical_pwm_percent(10, 14_000, 13_500, 100) <= 10);
    }

    #[test]
    fn heater_physical_pwm_reduces_power_below_6_5v_working_floor() {
        assert_eq!(
            heater_adjustable_request_mv(0, true, 12_000, 12_000, 6_100, 20_000),
            11_500
        );
        assert_eq!(
            heater_adjustable_request_mv(0, true, 6_100, 12_000, 6_100, 20_000),
            6_100
        );
        assert_eq!(heater_physical_pwm_percent(10, 14_000, 6_100, 100), 52);
        assert_eq!(heater_physical_pwm_percent(1, 14_000, 6_100, 100), 5);
        assert_eq!(heater_physical_pwm_percent(0, 14_000, 6_100, 100), 0);
    }

    #[test]
    fn hold_pps_governor_keeps_the_approach_voltage_when_headroom_is_sufficient() {
        let mut governor = HoldPpsGovernor::new();
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                40,
                -1.0,
                0.05,
                18_000,
                6_100,
                21_000,
                0,
            ),
            Some(18_000)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                40,
                -1.0,
                0.05,
                18_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(18_000)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                40,
                -1.0,
                0.05,
                18_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(18_000)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                0,
                -1.0,
                0.05,
                18_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS + HEATER_HOLD_PPS_STEADY_DWELL_MS,
            ),
            Some(18_000)
        );

        let mut nominal = HoldPpsGovernor::new();
        assert_eq!(
            nominal.request_mv(
                HeaterControlPhase::Hold,
                40,
                0.0,
                0.05,
                14_000,
                6_100,
                21_000,
                0,
            ),
            Some(14_000)
        );
        assert_eq!(
            nominal.request_mv(
                HeaterControlPhase::Hold,
                40,
                0.0,
                0.05,
                14_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(14_000)
        );
    }

    #[test]
    fn hold_pps_governor_steps_toward_a_lower_safe_max_without_clamping() {
        let mut governor = HoldPpsGovernor::new();
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                40,
                -1.0,
                0.05,
                19_000,
                6_100,
                6_100,
                0,
            ),
            Some(18_500)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                40,
                -1.0,
                0.05,
                18_500,
                6_100,
                6_100,
                HEATER_PPS_SMALL_TRANSITION_MS,
            ),
            Some(18_000)
        );
    }

    #[test]
    fn hold_pps_governor_raises_only_for_flat_below_target_saturation() {
        let mut governor = HoldPpsGovernor::new();
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                80,
                0.6,
                0.1,
                12_000,
                6_100,
                14_000,
                0,
            ),
            Some(12_000)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Hold,
                80,
                0.6,
                0.1,
                12_000,
                6_100,
                14_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(12_500)
        );

        let mut current_limited = HoldPpsGovernor::new();
        assert_eq!(
            current_limited.request_mv(
                HeaterControlPhase::Approach,
                80,
                0.6,
                0.1,
                18_500,
                6_100,
                18_500,
                0,
            ),
            Some(18_500)
        );
        assert_eq!(
            current_limited.request_mv(
                HeaterControlPhase::Approach,
                80,
                0.6,
                0.1,
                18_500,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(19_000)
        );

        let mut rising = HoldPpsGovernor::new();
        assert_eq!(
            rising.request_mv(
                HeaterControlPhase::Hold,
                80,
                0.6,
                0.5,
                12_000,
                6_100,
                14_000,
                0,
            ),
            Some(12_000)
        );
        assert_eq!(
            rising.request_mv(
                HeaterControlPhase::Hold,
                80,
                0.6,
                0.5,
                12_000,
                6_100,
                14_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(12_000)
        );
    }

    #[test]
    fn hold_pps_governor_keeps_adaptation_through_near_target_approach() {
        let mut governor = HoldPpsGovernor::new();
        let _ = governor.request_mv(
            HeaterControlPhase::Hold,
            100,
            1.0,
            0.0,
            18_000,
            6_100,
            21_000,
            0,
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Approach,
                100,
                1.0,
                0.0,
                18_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS,
            ),
            Some(18_500)
        );
        assert_eq!(
            governor.request_mv(
                HeaterControlPhase::Warmup,
                100,
                1.0,
                0.0,
                18_000,
                6_100,
                21_000,
                HEATER_HOLD_PPS_INITIAL_SETTLE_MS + 1_000,
            ),
            None
        );
    }

    #[test]
    fn warmup_soft_start_scales_physical_pwm_without_changing_control_request() {
        assert_eq!(apply_warmup_soft_start(80, 0), 0);
        assert_eq!(apply_warmup_soft_start(80, 50), 40);
        assert_eq!(apply_warmup_soft_start(80, 100), 80);
    }

    #[test]
    fn partition_table_binary_matches_eeprom_only_layout() {
        let partition_table = include_str!("../../partitions.csv");
        assert!(!partition_table.contains("flux_cfg"));
        let expected = esp_idf_part::PartitionTable::try_from(partition_table.as_bytes().to_vec())
            .unwrap()
            .to_bin()
            .unwrap();
        assert_eq!(expected.as_slice(), include_bytes!("../../partitions.bin"));
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
                    warmup_reenter_centi_c: 0,
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
        let five_amp =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(5_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 21_000,
                        max_ma: 5_000,
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
        assert_eq!(
            resolve_thermal_profile_bank(ThermalProfileMode::Auto, &five_amp),
            ThermalProfileBank::Pps5a
        );
        let three_amp =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(3_250),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 21_000,
                        max_ma: 3_250,
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
        assert_eq!(
            resolve_thermal_profile_bank(ThermalProfileMode::Auto, &three_amp),
            ThermalProfileBank::Pps3a
        );
        let split_apdos =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(5_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 11_000,
                        max_ma: 5_000,
                    }),
                    Some(ch224q::PpsApdo {
                        min_mv: 20_000,
                        max_mv: 21_000,
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
        assert_eq!(
            resolve_thermal_profile_bank(ThermalProfileMode::Auto, &split_apdos),
            ThermalProfileBank::Pps3a
        );
        assert_eq!(
            resolve_thermal_profile_bank(ThermalProfileMode::W100, &ManualPpsState::default()),
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
        assert_eq!(pps_request_transition_ms(false), 500);
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
    fn calibrated_resistance_curve_sets_3a_plant_power_ceiling() {
        let mut config = MemoryConfig::default();
        config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 2_000,
            resistance_milliohms: 3_936,
        });
        config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 22_000,
            resistance_milliohms: 5_674,
        });
        assert!(has_calibrated_heater_resistance_curve(&config));
        let pps3a_power_mw =
            heater_available_power_mw_for_temp(160.0, Some(20_000), Some(3_250), None, &config);
        let pps5a_power_mw =
            heater_available_power_mw_for_temp(160.0, Some(21_000), Some(5_000), None, &config);

        // The 20 V / 3.25 A APDO contributes its complete 65 W contract;
        // R(T) remains part of the heater-watt estimate but does not lower the
        // production voltage request or invent a board-current reserve.
        assert!((64_000..=65_000).contains(&pps3a_power_mw));
        assert!(pps5a_power_mw > 70_000);
    }

    #[test]
    fn production_pps_ceiling_is_the_selected_apdo_maximum_not_r_times_i() {
        let mut config = MemoryConfig::default();
        config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 2_000,
            resistance_milliohms: 3_200,
        });
        config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 21_500,
            resistance_milliohms: 6_680,
        });

        assert_eq!(
            production_pps_request_ceiling_mv(215.0, 3_000, 200, 21_000, None, &config),
            21_000
        );
    }

    #[test]
    fn pps_request_compensates_measured_path_drop_without_relaxing_plate_limit() {
        assert_eq!(
            heater_source_request_ceiling_mv(17_300, 17_300, 15_900, 21_000),
            18_700
        );
        assert_eq!(
            heater_source_request_ceiling_mv(17_300, 17_300, 0, 21_000),
            17_300
        );
        assert_eq!(
            heater_source_request_ceiling_mv(20_000, 21_000, 17_000, 21_000),
            21_000
        );
    }

    #[test]
    fn pps3a_heater_lock_requires_a_saved_resistance_curve() {
        let mut config = MemoryConfig::default();
        assert!(!has_calibrated_heater_resistance_curve(&config));

        config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 2_000,
            resistance_milliohms: 4_000,
        });
        config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 20_000,
            resistance_milliohms: 5_600,
        });

        assert!(has_calibrated_heater_resistance_curve(&config));
    }

    #[test]
    fn raw_heater_observations_reproject_without_underestimating_the_model_floor() {
        let mut config = MemoryConfig::default();
        config.active_heater_curve.points[0] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 0,
            resistance_milliohms: 2_948,
        });
        config.active_heater_curve.points[1] = Some(flux_purr_firmware::memory::HeaterCurvePoint {
            temp_centi_c: 21_670,
            resistance_milliohms: 5_674,
        });
        config.heater_curve_raw_observations.points[0] =
            Some(flux_purr_firmware::memory::HeaterCurveRawObservation {
                raw_rtd_adc_mv: 1_269,
                heater_voltage_mv: 18_500,
                heater_current_ma: 4_700,
                resistance_milliohms: 3_936,
            });
        config.heater_curve_raw_observations.points[1] =
            Some(flux_purr_firmware::memory::HeaterCurveRawObservation {
                raw_rtd_adc_mv: 1_300,
                heater_voltage_mv: 18_500,
                heater_current_ma: 4_700,
                resistance_milliohms: 3_936,
            });

        let resistance = estimated_heater_resistance_ohms(216.7, None, &config);
        assert!(resistance >= default_estimated_heater_resistance_ohms(216.7));
        assert_eq!(
            heater_safe_max_mv_for_temp(216.7, 4_700, 21_000, None, &config),
            21_000
        );
    }

    #[test]
    fn effective_pps_current_limit_uses_contract_not_instantaneous_draw() {
        let status_limit = effective_pps_current_limit_ma(
            5_000,
            Some(PdStatusObservation {
                status_raw: 0,
                status: Status {
                    pd_active: true,
                    ..Status::default()
                },
                current_raw: 40,
                current_ma: 2_000,
                contract_voltage_mv: None,
                contract: Contract::none(),
            }),
        );
        assert_eq!(status_limit, 5_000);

        let zero_draw_keeps_contract = effective_pps_current_limit_ma(
            5_000,
            Some(PdStatusObservation {
                status_raw: 0,
                status: Status {
                    pd_active: true,
                    ..Status::default()
                },
                current_raw: 0,
                current_ma: 0,
                contract_voltage_mv: None,
                contract: Contract::none(),
            }),
        );
        assert_eq!(zero_draw_keeps_contract, 5_000);

        assert_eq!(effective_pps_current_limit_ma(5_000, None), 5_000);
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
    fn fixed_pd_runtime_caps_cold_plate_duty_to_the_negotiated_current_budget() {
        let config = MemoryConfig::default();

        assert_eq!(
            fixed_pd_pwm_duty_percent(100, 20.0, 12_000, 3_250, 200, None, &config,),
            80
        );
        assert_eq!(
            fixed_pd_pwm_duty_percent(50, 20.0, 12_000, 3_250, 200, None, &config,),
            50
        );
        assert_eq!(
            fixed_pd_pwm_duty_percent(100, 20.0, 12_000, 0, 200, None, &config),
            0
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
                terminal_fixed_pd_disarmed: false,
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
    fn heater_backend_uses_the_20v_apdo_current_not_another_range() {
        let backend = select_heater_power_backend(
            Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(5_000),
                pps_apdos: [
                    Some(ch224q::PpsApdo {
                        min_mv: 5_000,
                        max_mv: 11_000,
                        max_ma: 5_000,
                    }),
                    Some(ch224q::PpsApdo {
                        min_mv: 20_000,
                        max_mv: 21_000,
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
            }),
            Some(Status::default()),
        );

        assert_eq!(
            backend,
            HeaterPowerBackend::PpsMos {
                pps_min_mv: 20_000,
                idle_request_mv: 20_000,
                pps_max_mv: 21_000,
                adjustable_max_mv: 21_000,
                capability_max_ma: 3_000,
                current_mode: None,
                current_request_mv: 20_000,
                settle_until_ms: None,
                next_request_at_ms: 0,
                current_limit_fixed_pwm_active: false,
                current_limit_fixed_request_confirmed: false,
                terminal_fixed_pd_disarmed: false,
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
                terminal_fixed_pd_disarmed: false,
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
                terminal_fixed_pd_disarmed: false,
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
                terminal_fixed_pd_disarmed: false,
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
                terminal_fixed_pd_disarmed: false,
            }
        );
        assert_eq!(
            heater_request_mv_from_power_percent(0, 15_000, 21_000),
            15_000
        );
    }

    #[test]
    fn auto_cooling_policy_runs_full_speed_then_cooldown_below_35c() {
        let stopped = fan_policy_decision(34, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(stopped.command, FanHardwareCommand::disabled());
        assert_eq!(stopped.display_state, FanDisplayState::Auto);

        let active = fan_policy_decision(35, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(
            active.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
        );
        assert_eq!(active.state, FanPolicyState::ActiveCooling);
        assert_eq!(active.display_state, FanDisplayState::Run);

        let still_active =
            fan_policy_decision(60, 0, false, 0, true, FanPolicyState::Disabled, false);
        assert_eq!(
            still_active.command,
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
        );

        let cooldown = fan_policy_decision(
            34,
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
            34,
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
            34,
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
    fn raw_rtd_overtemp_bypasses_the_control_slew_guard() {
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(140.0, 0);

        assert_eq!(measurement_guard.observe(430.0, 1_000), None);
        assert!(measurement_guard.guarded);
        assert_eq!(overtemp_fault_from_control_temperature(140.0), None);
        assert_eq!(
            overtemp_fault_from_control_temperature(430.0),
            Some(HeaterFaultReason::OverTemp)
        );
    }

    #[test]
    fn rtd_control_guard_recovers_after_a_stable_persistent_temperature_shift() {
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(140.0, 0);

        assert_eq!(measurement_guard.observe(260.0, 1_000), None);
        assert!(measurement_guard.guarded);
        assert_eq!(measurement_guard.observe(260.0, 1_700), None);
        assert!(measurement_guard.guarded);
        assert_eq!(measurement_guard.observe(260.0, 1_800), Some(260.0));
        assert!(!measurement_guard.guarded);
        assert_eq!(measurement_guard.observe(145.0, 5_000), None);
        assert!(measurement_guard.guarded);
        assert_eq!(measurement_guard.observe(145.0, 5_800), Some(145.0));
        assert!(!measurement_guard.guarded);
    }

    #[test]
    fn rtd_control_guard_does_not_reseed_from_fast_unpowered_rise() {
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(80.0, 0);

        assert_eq!(
            measurement_guard.observe_with_heater_duty(85.0, 300, 0),
            None
        );
        assert!(measurement_guard.guarded);
        assert_eq!(measurement_guard.last_accepted_temp_c, Some(80.0));

        assert_eq!(
            measurement_guard.observe_with_heater_duty(85.0, 1_500, 0),
            None
        );
        assert!(measurement_guard.guarded);
        assert_eq!(measurement_guard.last_accepted_temp_c, Some(80.0));

        assert_eq!(
            measurement_guard.observe_with_heater_duty(80.5, 1_800, 0),
            Some(80.5)
        );
        assert!(!measurement_guard.guarded);
    }

    #[test]
    fn rtd_control_guard_remains_active_after_heater_disabled_interval() {
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(143.7, 0);
        let mut measurement_guarded = true;

        preserve_rtd_control_guard_when_heater_disabled(false, &mut measurement_guarded);

        assert_eq!(measurement_guard.last_accepted_temp_c, Some(143.7));
        assert!(!measurement_guarded);
        assert_eq!(measurement_guard.observe(430.0, 1_000), None);
        assert!(measurement_guard.guarded);
    }

    #[test]
    fn rtd_pps_transition_guard_accepts_immediately_and_reseeds_on_request_change() {
        let mut guard = RtdPpsTransitionGuard::new(21_000);
        assert_eq!(guard.observe(21_000, 0), (true, false));
        assert_eq!(guard.observe(20_500, 40), (false, true));
        assert_eq!(guard.observe(20_000, 240), (false, true));
        assert_eq!(guard.observe(20_000, 520), (false, false));
        assert_eq!(guard.observe(20_000, 540), (true, true));
        assert_eq!(guard.observe(20_000, 580), (true, false));
    }

    #[test]
    fn rtd_pps_transition_rechecks_first_stable_sample_from_last_trusted_temperature() {
        let mut guard = RtdPpsTransitionGuard::new(18_000);
        let mut controller = HeaterController::new();
        controller.reseed_measurement(140.0);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(140.0, 0);

        assert_eq!(
            accept_rtd_control_sample_after_pps_transition(
                &mut guard,
                &mut controller,
                &mut measurement_guard,
                140.0,
                17_500,
                50,
                140.0,
            ),
            None
        );
        assert_eq!(
            accept_rtd_control_sample_after_pps_transition(
                &mut guard,
                &mut controller,
                &mut measurement_guard,
                140.0,
                17_500,
                350,
                143.0,
            ),
            None
        );
        assert!(measurement_guard.guarded);
        assert_eq!(controller.filtered_temp_c, Some(140.0));

        assert_eq!(
            accept_rtd_control_sample_after_pps_transition(
                &mut guard,
                &mut controller,
                &mut measurement_guard,
                140.0,
                17_500,
                1_100,
                143.0,
            ),
            Some(143.0)
        );
        assert!(!measurement_guard.guarded);
        assert_eq!(controller.filtered_temp_c, Some(143.0));
    }

    #[test]
    fn pps_transition_reseed_keeps_last_trusted_control_temperature() {
        let mut guard = RtdPpsTransitionGuard::new(18_000);
        let mut controller = HeaterController::new();
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        let last_control_temp_c = 41.39;

        let control_temp_c = accept_rtd_control_sample_after_pps_transition(
            &mut guard,
            &mut controller,
            &mut measurement_guard,
            last_control_temp_c,
            14_000,
            0,
            73.74,
        );

        assert_eq!(control_temp_c, None);
        assert_eq!(controller.filtered_temp_c, Some(last_control_temp_c));
        assert_eq!(
            controller.previous_filtered_temp_c,
            Some(last_control_temp_c)
        );
        assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
        assert_eq!(
            controller.previous_measured_temp_c,
            Some(last_control_temp_c)
        );
        assert!(!measurement_guard.guarded);
    }

    #[test]
    fn rtd_control_guard_rejects_impossible_jump_without_hiding_raw_temperature() {
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut latest_control_temp_c = 140.0;
        let mut latest_control_temp_i16 = 140;
        let mut latest_display_temp_c = 140.0;
        let mut latest_display_temp_i16 = 140;
        let mut transition_guard = RtdPpsTransitionGuard::new(12_000);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        measurement_guard.reseed(140.0, 0);
        let mut control_measurement_guarded = false;
        let mut controller = HeaterController::new();

        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_control_temp_c,
                latest_control_temp_i16: &mut latest_control_temp_i16,
                transition_guard: &mut transition_guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            12_000,
            50,
            310.0,
        ));

        assert_eq!(latest_display_temp_c, 310.0);
        assert_eq!(ui_state.current_temp_c, 310);
        assert_eq!(latest_control_temp_c, 140.0);
        assert_eq!(latest_control_temp_i16, 140);
        assert!(control_measurement_guarded);

        measurement_guard.clear();
        assert_eq!(measurement_guard.observe(31.0, 100), Some(31.0));
        assert!(!measurement_guard.guarded);
    }

    #[test]
    fn rtd_retry_triggers_on_request_change_even_before_vin_moves() {
        assert!(should_retry_rtd_sample_after_power_step(
            17_500, 18_000, 1_170, 1_170
        ));
    }

    #[test]
    fn rtd_retry_triggers_on_large_vin_step_without_request_change() {
        assert!(should_retry_rtd_sample_after_power_step(
            18_000, 18_000, 1_170, 1_230
        ));
        assert!(!should_retry_rtd_sample_after_power_step(
            18_000, 18_000, 1_170, 1_200
        ));
    }

    #[test]
    fn heater_controller_reseed_clears_measurement_slope_without_changing_phase() {
        let mut controller = HeaterController::new();
        controller.phase = HeaterControlPhase::Approach;
        controller.filtered_temp_c = Some(40.0);
        controller.previous_filtered_temp_c = Some(39.0);
        controller.filtered_slope_c_per_profile_tick = 8.0;
        controller.previous_measured_temp_c = Some(41.0);

        controller.reseed_measurement(52.0);

        assert_eq!(controller.phase, HeaterControlPhase::Approach);
        assert_eq!(controller.filtered_temp_c, Some(52.0));
        assert_eq!(controller.previous_filtered_temp_c, Some(52.0));
        assert_eq!(controller.filtered_slope_c_per_profile_tick, 0.0);
        assert_eq!(controller.previous_measured_temp_c, Some(52.0));
    }

    #[test]
    fn thermal_plant_filters_single_sample_slope_before_delay_prediction() {
        let mut controller = HeaterController::new();
        let model = flux_purr_firmware::memory::ThermalPlantProjection {
            convection_mw_per_c: 0.0,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 42_000.0,
            transport_delay_ms: 10_000,
        };
        let input = |measured_temp_c, heater_enabled, now_ms| ThermalPlantRuntimeInput {
            target_temp_c: 60,
            measured_temp_c,
            ambient_temp_c: 30.0,
            heater_enabled,
            model,
            max_power_mw: 100_000.0,
            now_ms,
        };

        controller.update_thermal_plant_at(input(30.0, true, 0));
        let snapshot = controller.update_thermal_plant_at(input(31.0, true, 50));

        assert!((snapshot.filtered_slope_c_per_s - 0.375).abs() < 0.001);
        assert!(snapshot.control_error_c > 0.0);
        assert!(snapshot.duty_percent >= 70);
        assert_eq!(snapshot.phase, HeaterControlPhase::Warmup);
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
    fn rtd_uses_nominal_regulator_feedback_divider_supply() {
        let expected_temperature_c = 31.0;
        let resistance_ohms = pt1000_resistance_ohms_at(expected_temperature_c);
        let sense_mv = 3_328.0 * resistance_ohms / (RTD_REFERENCE_RESISTOR_OHMS + resistance_ohms);
        let reported_temperature_c = pt1000_temperature_c_from_resistance(
            rtd_resistance_ohms_from_fractional_mv(sense_mv).unwrap(),
        );

        assert!(
            (reported_temperature_c - expected_temperature_c).abs() < 0.05,
            "expected {expected_temperature_c:.2}C, got {reported_temperature_c:.2}C"
        );
    }

    #[test]
    fn rtd_oversampling_accepts_partial_batch_with_enough_valid_conversions() {
        let valid_samples = RTD_MIN_VALID_SAMPLE_COUNT;
        let sum_mv = (900 * valid_samples) + (valid_samples / 2);
        let mean_mv = rtd_fractional_mean_mv(sum_mv as u32, valid_samples).unwrap();

        assert!((mean_mv - 900.5).abs() < 0.001);
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
    fn rtd_oversampling_discards_settle_prefix_before_meaningful_average() {
        let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
        samples.extend(std::iter::repeat_n(Some(900_u16), RTD_SAMPLE_COUNT));
        let mut iter = samples.into_iter();
        let mean_mv = oversampled_fractional_mean_mv_with_discard(
            RTD_SAMPLE_COUNT + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            || iter.next().flatten(),
        )
        .unwrap();

        assert!((mean_mv - 900.0).abs() < 0.001);
    }

    #[test]
    fn rtd_oversampling_reports_kept_batch_extrema() {
        let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
        let mut kept = vec![Some(900_u16); RTD_SAMPLE_COUNT];
        kept[3] = Some(899);
        kept[17] = Some(902);
        samples.extend(kept);
        let mut iter = samples.into_iter();
        let batch = oversampled_rtd_batch_with_discard(
            RTD_SAMPLE_COUNT + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            || iter.next().flatten(),
        )
        .expect("RTD batch has enough valid conversions");

        assert_eq!(batch.min_mv, 899);
        assert_eq!(batch.max_mv, 902);
        assert_eq!(batch.max_mv.saturating_sub(batch.min_mv), 3);
    }

    #[test]
    fn rtd_phase_sampling_covers_the_entire_pwm_period_without_hiding_extrema() {
        let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
        for phase in 0..RTD_SAMPLE_PWM_PHASE_COUNT {
            let phase_mv = if phase % 2 == 0 { 900 } else { 920 };
            samples.extend(std::iter::repeat_n(
                Some(phase_mv),
                RTD_SAMPLE_COUNT / RTD_SAMPLE_PWM_PHASE_COUNT,
            ));
        }
        let mut iter = samples.into_iter();
        let mut phase_waits = 0_usize;
        let batch = phase_averaged_rtd_batch_with_discard(
            RTD_SAMPLE_COUNT,
            RTD_SAMPLE_PWM_PHASE_COUNT,
            RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            || {
                iter.next().flatten().map(|value| AdcConvertedSample {
                    raw_code: value.saturating_mul(2),
                    calibrated_mv: value,
                })
            },
            || phase_waits = phase_waits.saturating_add(1),
        )
        .expect("RTD phase batch has enough valid conversions");

        assert!((batch.mean_mv - 910.0).abs() < 0.001);
        assert_eq!(batch.min_mv, 900);
        assert_eq!(batch.max_mv, 920);
        assert_eq!(batch.mean_raw_code, 1_820);
        assert_eq!(batch.min_raw_code, 1_800);
        assert_eq!(batch.max_raw_code, 1_840);
        assert_eq!(phase_waits, RTD_SAMPLE_PWM_PHASE_COUNT - 1);
    }

    #[test]
    fn adc_samples_always_mask_status_bits_to_twelve_bit_code() {
        assert_eq!(mask_adc1_raw_code(0xfabc), 0x0abc);
        assert_eq!(mask_adc1_raw_code(0x0fff), 0x0fff);
    }

    #[test]
    fn rtd_phase_sampling_rejects_an_invalid_phase_plan() {
        assert_eq!(
            phase_averaged_rtd_batch_with_discard(
                79,
                10,
                0,
                || {
                    Some(AdcConvertedSample {
                        raw_code: 1_800,
                        calibrated_mv: 900,
                    })
                },
                || {}
            ),
            None
        );
    }

    #[test]
    fn rtd_oversampling_ignores_faulty_prefix_only_after_valid_tail_threshold() {
        let kept_valid_samples = RTD_MIN_VALID_SAMPLE_COUNT;
        let mut samples = vec![Some(1_240_u16); RTD_SETTLE_DISCARD_SAMPLE_COUNT];
        samples.extend(std::iter::repeat_n(Some(900_u16), kept_valid_samples));
        let mut iter = samples.into_iter();
        let mean_mv = oversampled_fractional_mean_mv_with_discard(
            kept_valid_samples + RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            RTD_SETTLE_DISCARD_SAMPLE_COUNT,
            || iter.next().flatten(),
        )
        .unwrap();

        assert!((mean_mv - 900.0).abs() < 0.001);
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
            FanHardwareCommand::from_profile(FanVoltageProfile::Full)
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
    fn valid_rtd_measurement_promotes_startup_dashboard_to_ready() {
        let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
        let mut latest_temp_c = 0.0;
        let mut latest_temp_i16 = 0;
        let mut latest_display_temp_c = 0.0;
        let mut latest_display_temp_i16 = 0;
        let mut guard = RtdPpsTransitionGuard::new(12_000);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        let mut control_measurement_guarded = false;
        let mut controller = HeaterController::new();

        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            12_000,
            100,
            41.39,
        ));

        assert_eq!(
            ui_state.dashboard_presentation,
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready
        );
        assert_eq!(ui_state.current_temp_deci_c, 414);
    }

    #[test]
    fn valid_rtd_measurement_does_not_bypass_eeprom_restore_lock() {
        let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
        ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore,
        );
        ui_state.eeprom_data_incompatible = true;
        let mut latest_display_temp_c = 0.0;
        let mut latest_display_temp_i16 = 0;

        assert!(update_runtime_display_temperature(
            &mut ui_state,
            &mut latest_display_temp_c,
            &mut latest_display_temp_i16,
            41.39,
        ));
        assert_eq!(
            ui_state.dashboard_presentation,
            flux_purr_firmware::frontpanel::DashboardPresentationState::EepromRestore
        );
        assert!(ui_state.persistence_locked());
    }

    #[test]
    fn runtime_sensor_fault_retains_last_valid_dashboard_temperature() {
        let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
        let mut latest_display_temp_c = 0.0;
        let mut latest_display_temp_i16 = 0;

        assert!(update_runtime_display_temperature(
            &mut ui_state,
            &mut latest_display_temp_c,
            &mut latest_display_temp_i16,
            85.4,
        ));
        assert_eq!(
            ui_state.dashboard_presentation,
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready
        );
        assert_eq!(ui_state.current_temp_deci_c, 854);

        ui_state.heater_lock_reason = Some(HeaterLockReason::SensorFault);
        ui_state.dashboard_warning_visible = true;
        assert!(!retain_runtime_display_temperature(
            &mut ui_state,
            &mut latest_display_temp_c,
            &mut latest_display_temp_i16,
        ));
        assert_eq!(ui_state.current_temp_deci_c, 854);
        assert_eq!(latest_display_temp_i16, 85);
    }

    #[test]
    fn valid_rtd_measurement_updates_display_and_control_on_request_change() {
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut latest_temp_c = 41.39;
        let mut latest_temp_i16 = 41;
        let mut latest_display_temp_c = latest_temp_c;
        let mut latest_display_temp_i16 = latest_temp_i16;
        let mut guard = RtdPpsTransitionGuard::new(12_000);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        let mut control_measurement_guarded = false;
        let mut controller = HeaterController::new();

        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            15_000,
            100,
            73.74,
        ));

        assert_eq!(latest_temp_c, 41.39);
        assert_eq!(latest_temp_i16, 41);
        assert_eq!(latest_display_temp_c, 73.74);
        assert_eq!(latest_display_temp_i16, 74);
        assert_eq!(ui_state.current_temp_c, 74);
        assert_eq!(ui_state.current_temp_deci_c, 737);
    }

    #[test]
    fn five_amp_power_step_retry_continues_through_runtime_sampling_pipeline() {
        let retry_sample = RtdSample::Valid(RtdMeasurement {
            raw_adc_mv: 983,
            raw_adc_min_mv: 982,
            raw_adc_max_mv: 984,
            adc_mv: 983,
            resistance_ohms: 1_118.0,
            temp_c: 30.73,
            current_temp_c: 31,
        });
        let RtdSample::Valid(measurement) = retry_sample else {
            panic!("5A warmup retry must remain a valid RTD sample");
        };
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut latest_temp_c = 25.37;
        let mut latest_temp_i16 = 25;
        let mut latest_display_temp_c = latest_temp_c;
        let mut latest_display_temp_i16 = latest_temp_i16;
        let mut guard = RtdPpsTransitionGuard::new(19_500);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        let mut control_measurement_guarded = false;
        let mut controller = HeaterController::new();

        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            20_000,
            100,
            measurement.temp_c,
        ));

        assert_eq!(latest_display_temp_c, 30.73);
        assert_eq!(latest_display_temp_i16, 31);
        assert_eq!(ui_state.current_temp_c, 31);
        assert_eq!(ui_state.current_temp_deci_c, 307);
        assert_eq!(latest_temp_c, 25.37);
        assert_eq!(latest_temp_i16, 25);
        assert_eq!(controller.fault_latched(), None);
    }

    #[test]
    fn measurement_fault_samples_remain_explicit_hard_faults() {
        for reason in [
            HeaterFaultReason::SensorOpen,
            HeaterFaultReason::SensorShort,
            HeaterFaultReason::AdcReadFailed,
        ] {
            let sample = RtdSample::Fault {
                adc_mv: None,
                reason,
            };
            assert!(matches!(
                sample,
                RtdSample::Fault {
                    reason: observed,
                    ..
                } if observed == reason
            ));
        }
    }

    #[test]
    fn valid_rtd_measurement_updates_control_after_request_stabilizes() {
        let mut ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let mut latest_temp_c = 41.39;
        let mut latest_temp_i16 = 41;
        let mut latest_display_temp_c = latest_temp_c;
        let mut latest_display_temp_i16 = latest_temp_i16;
        let mut guard = RtdPpsTransitionGuard::new(12_000);
        let mut measurement_guard = RtdControlMeasurementGuard::default();
        let mut control_measurement_guarded = false;
        let mut controller = HeaterController::new();

        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            15_000,
            100,
            73.74,
        ));
        assert!(apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            15_000,
            700,
            42.0,
        ));

        // The first accepted sample after the PPS transition is still
        // observed at zero physical duty. Repeat after its unpowered-slew
        // guard interval before requiring the control temperature to move.
        let _ = apply_valid_rtd_measurement(
            RuntimeDisplayTemperatureState {
                ui_state: &mut ui_state,
                latest_display_temp_c: &mut latest_display_temp_c,
                latest_display_temp_i16: &mut latest_display_temp_i16,
            },
            RuntimeControlTemperatureState {
                latest_control_temp_c: &mut latest_temp_c,
                latest_control_temp_i16: &mut latest_temp_i16,
                transition_guard: &mut guard,
                measurement_guard: &mut measurement_guard,
                control_measurement_guarded: &mut control_measurement_guarded,
                heater_controller: &mut controller,
            },
            15_000,
            1_000,
            42.0,
        );
        assert_eq!(latest_temp_c, 42.0);
        assert_eq!(latest_temp_i16, 42);
        assert_eq!(latest_display_temp_c, 42.0);
        assert_eq!(latest_display_temp_i16, 42);
        assert_eq!(ui_state.current_temp_c, 42);
        assert_eq!(ui_state.current_temp_deci_c, 420);
    }

    #[test]
    fn pd_status_log_key_ignores_current_limit_churn() {
        let first = PdStatusObservation {
            status_raw: 0x81,
            status: Status {
                bc_active: false,
                qc2_active: false,
                qc3_active: false,
                pd_active: true,
                epr_active: false,
                epr_exist: false,
                avs_exist: false,
            },
            current_raw: 0x10,
            current_ma: 800,
            contract_voltage_mv: None,
            contract: Contract::none(),
        };
        let second = PdStatusObservation {
            current_raw: 0x2a,
            current_ma: 2_100,
            ..first
        };

        assert_eq!(
            pd_status_log_key(Some(first)),
            pd_status_log_key(Some(second))
        );
    }

    #[test]
    fn fusb302b_fixed_contract_status_is_explicit_and_blocks_calibration() {
        let contract = Contract {
            kind: ContractKind::Fixed,
            object_position: 3,
            voltage_mv: 20_000,
            current_ma: 5_000,
        };
        let observation = PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        };
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                pd_controller: ControllerKind::Fusb302b,
                last_pd_observation: Some(observation),
                vin_mv: 20_000,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.pd_controller.as_str(), "fusb302b");
        assert_eq!(status.pd_contract_kind.as_str(), "fixed");
        assert_eq!(status.pd_contract_current_ma, 5_000);
        assert_eq!(status.pd_contract_power_mw, 100_000);
        assert!(status.pd_performance_guaranteed);
        assert_eq!(status.pd_degraded_reason, None);
        assert!(!pd_contract_allows_calibration(
            ControllerKind::Fusb302b,
            Some(observation)
        ));

        let low_voltage = PdStatusObservation {
            contract: Contract {
                voltage_mv: 15_000,
                current_ma: 3_000,
                ..contract
            },
            contract_voltage_mv: Some(15_000),
            current_ma: 3_000,
            ..observation
        };
        assert!(!pd_contract_allows_calibration(
            ControllerKind::Fusb302b,
            Some(low_voltage)
        ));

        let fallback = HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::CapabilityReadFailed,
            fixed_request_confirmed: true,
            fixed_request: ch224q::VoltageRequest::V20,
        };
        assert_eq!(
            effective_pd_contract_mv(&ManualPpsState::default(), Some(low_voltage), fallback),
            15_000
        );
    }

    #[test]
    fn fusb302b_pps_contract_enables_calibration_and_caps_backend_to_21v() {
        let contract = Contract {
            kind: ContractKind::Pps,
            object_position: 2,
            voltage_mv: 20_000,
            current_ma: 5_000,
        };
        let observation = PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        };
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                pd_controller: ControllerKind::Fusb302b,
                last_pd_observation: Some(observation),
                vin_mv: 20_000,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.pd_contract_kind.as_str(), "pps");
        assert_eq!(status.pd_contract_power_mw, 100_000);
        assert!(status.pd_performance_guaranteed);
        assert!(pd_contract_allows_calibration(
            ControllerKind::Fusb302b,
            Some(observation)
        ));

        let backend = HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 28_000,
            adjustable_max_mv: 28_000,
            capability_max_ma: 5_000,
            current_mode: Some(ch224q::AdjustableVoltageMode::Avs),
            current_request_mv: 24_000,
            settle_until_ms: None,
            next_request_at_ms: 0,
            current_limit_fixed_pwm_active: false,
            current_limit_fixed_request_confirmed: false,
            terminal_fixed_pd_disarmed: false,
        };
        let constrained = constrain_heater_backend_to_controller(ControllerKind::Fusb302b, backend);
        let HeaterPowerBackend::PpsMos {
            pps_max_mv,
            adjustable_max_mv,
            current_mode,
            current_request_mv,
            ..
        } = constrained
        else {
            panic!("FUSB302B must retain a PPS backend when a PPS APDO is present");
        };
        assert_eq!(pps_max_mv, 21_000);
        assert_eq!(adjustable_max_mv, 21_000);
        assert_eq!(current_mode, Some(ch224q::AdjustableVoltageMode::Pps));
        assert_eq!(current_request_mv, 21_000);
    }

    #[test]
    fn fusb302b_deferred_source_capabilities_promote_the_pps_backend() {
        let mut capabilities = ch224q::AdjustablePowerCapabilities {
            pps_covers_20v: true,
            pps_min_mv: Some(5_000),
            pps_max_mv: Some(21_000),
            pps_max_ma: Some(3_000),
            ..ch224q::AdjustablePowerCapabilities::default()
        };
        capabilities.pps_apdos[0] = Some(ch224q::PpsApdo {
            min_mv: 5_000,
            max_mv: 21_000,
            max_ma: 3_000,
        });

        let backend = fusb302b_pps_backend_from_capabilities(capabilities)
            .expect("FUSB302B PPS source capabilities must enable the PPS backend");
        let HeaterPowerBackend::PpsMos {
            pps_min_mv,
            pps_max_mv,
            capability_max_ma,
            current_mode,
            ..
        } = backend
        else {
            panic!("FUSB302B must promote from fixed fallback to PPS");
        };
        assert_eq!(pps_min_mv, 5_000);
        assert_eq!(pps_max_mv, 21_000);
        assert_eq!(capability_max_ma, 3_000);
        assert_eq!(current_mode, Some(ch224q::AdjustableVoltageMode::Pps));
    }

    #[test]
    fn fusb302b_pending_contract_is_not_reported_as_ready() {
        let ui_state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        let status = usb_runtime_status(
            &ui_state,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                pd_controller: ControllerKind::Fusb302b,
                heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
                    reason: HeaterPowerBackendReason::NoPps20vCapability,
                    fixed_request_confirmed: false,
                    fixed_request: ch224q::VoltageRequest::V20,
                },
                vin_mv: 20_000,
                ..test_usb_runtime_status_context()
            },
        );

        assert_eq!(status.pd_contract_kind.as_str(), "none");
        assert_eq!(status.pd_contract_current_ma, 0);
        assert_eq!(status.pd_contract_power_mw, 0);
        assert!(!status.pd_performance_guaranteed);
        assert_eq!(
            status.pd_degraded_reason.as_deref(),
            Some("pd_contract_unavailable")
        );
    }

    #[test]
    fn fusb302b_backend_never_inherits_a_ch224q_28v_default() {
        let legacy = HeaterPowerBackend::FixedPdPwmFallback {
            reason: HeaterPowerBackendReason::CapabilityReadFailed,
            fixed_request_confirmed: true,
            fixed_request: ch224q::VoltageRequest::V28,
        };

        let fusb = constrain_heater_backend_to_controller(ControllerKind::Fusb302b, legacy);
        assert_eq!(fusb.pd_request_mv(), 20_000);
        let HeaterPowerBackend::FixedPdPwmFallback {
            fixed_request_confirmed,
            ..
        } = fusb
        else {
            panic!("FUSB302B must use fixed-PDO PWM fallback");
        };
        assert!(!fixed_request_confirmed);

        let ch224q = constrain_heater_backend_to_controller(ControllerKind::Ch224q, legacy);
        assert_eq!(ch224q.pd_request_mv(), 28_000);
    }

    #[test]
    fn fault_attention_transitions_alarm_to_pending_reminder() {
        let mut last_fault_present = false;
        let mut attention_acknowledged = false;
        let mut attention_pending = false;
        let mut forced_fan_active = false;
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut next_reminder_ms = None;
        let mut buzzer = BuzzerArbiter::new();

        assert!(update_fault_attention_state(
            true,
            FaultAttentionState {
                last_fault_present: &mut last_fault_present,
                attention_acknowledged: &mut attention_acknowledged,
                attention_pending_after_fault_clear: &mut attention_pending,
                forced_fan_active: &mut forced_fan_active,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            100,
            &mut buzzer,
            3_000,
        ));
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
        assert!(!attention_pending);
        assert_eq!(protection_alarm.next_replay_ms(), Some(4_000));
        assert_eq!(next_reminder_ms, None);
        assert!(forced_fan_active);

        assert!(update_fault_attention_state(
            false,
            FaultAttentionState {
                last_fault_present: &mut last_fault_present,
                attention_acknowledged: &mut attention_acknowledged,
                attention_pending_after_fault_clear: &mut attention_pending,
                forced_fan_active: &mut forced_fan_active,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            100,
            &mut buzzer,
            8_000,
        ));
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::AttentionReminder));
        assert!(attention_pending);
        assert_eq!(protection_alarm.next_replay_ms(), None);
        assert_eq!(
            next_reminder_ms,
            Some(8_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
        );
        assert!(forced_fan_active);
    }

    #[test]
    fn attention_reminder_starts_immediately_after_fault_clear_then_rearms() {
        let mut last_fault_present = true;
        let mut attention_acknowledged = false;
        let mut attention_pending = false;
        let mut forced_fan_active = true;
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut next_reminder_ms = None;
        let mut buzzer = BuzzerArbiter::new();
        buzzer.activate_protection(BuzzerCueSource::ThermalProtection, 0);

        assert!(update_fault_attention_state(
            false,
            FaultAttentionState {
                last_fault_present: &mut last_fault_present,
                attention_acknowledged: &mut attention_acknowledged,
                attention_pending_after_fault_clear: &mut attention_pending,
                forced_fan_active: &mut forced_fan_active,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            39,
            &mut buzzer,
            2_000,
        ));

        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::AttentionReminder));
        assert_eq!(buzzer.output().frequency_hz, Some(1_650));
        assert_eq!(
            next_reminder_ms,
            Some(2_000 + BUZZER_ATTENTION_REMINDER_INTERVAL_MS)
        );
    }

    #[test]
    fn measurement_faults_do_not_require_overtemp_attention() {
        for reason in [
            HeaterFaultReason::SensorOpen,
            HeaterFaultReason::SensorShort,
            HeaterFaultReason::AdcReadFailed,
        ] {
            assert!(!is_overtemp_fault(Some(reason)));
        }
        assert!(is_overtemp_fault(Some(HeaterFaultReason::OverTemp)));
        assert!(!is_overtemp_fault(None));
    }

    #[test]
    fn acknowledging_active_overtemp_keeps_alarm_but_prevents_pending_reminder() {
        let mut last_overtemp_present = false;
        let mut acknowledged = false;
        let mut pending = false;
        let mut forced_fan = false;
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut next_reminder_ms = None;
        let mut buzzer = BuzzerArbiter::new();

        assert!(update_fault_attention_state(
            true,
            FaultAttentionState {
                last_fault_present: &mut last_overtemp_present,
                attention_acknowledged: &mut acknowledged,
                attention_pending_after_fault_clear: &mut pending,
                forced_fan_active: &mut forced_fan,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            420,
            &mut buzzer,
            0,
        ));
        assert!(acknowledge_overtemp_attention(
            true,
            &mut acknowledged,
            &mut pending,
            &mut forced_fan,
            &mut next_reminder_ms,
            &mut buzzer,
        ));
        assert!(acknowledged);
        assert!(!pending);
        assert!(!forced_fan);
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));

        assert!(update_fault_attention_state(
            false,
            FaultAttentionState {
                last_fault_present: &mut last_overtemp_present,
                attention_acknowledged: &mut acknowledged,
                attention_pending_after_fault_clear: &mut pending,
                forced_fan_active: &mut forced_fan,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            100,
            &mut buzzer,
            2_000,
        ));
        assert!(!pending);
        assert_eq!(next_reminder_ms, None);
    }

    #[test]
    fn cooling_below_40_releases_forced_fan_but_not_attention_requirement() {
        let mut last_overtemp_present = true;
        let mut acknowledged = false;
        let mut pending = false;
        let mut forced_fan = true;
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut next_reminder_ms = None;
        let mut buzzer = BuzzerArbiter::new();

        assert!(update_fault_attention_state(
            false,
            FaultAttentionState {
                last_fault_present: &mut last_overtemp_present,
                attention_acknowledged: &mut acknowledged,
                attention_pending_after_fault_clear: &mut pending,
                forced_fan_active: &mut forced_fan,
                protection_alarm: &mut protection_alarm,
                next_attention_reminder_ms: &mut next_reminder_ms,
            },
            39,
            &mut buzzer,
            2_000,
        ));
        assert!(pending);
        assert!(!forced_fan);
        assert!(overtemp_attention_requires_ack(
            false,
            acknowledged,
            pending
        ));
    }

    #[test]
    fn overtemp_forced_fan_follows_locked_temperature_bands() {
        assert_eq!(
            overtemp_forced_fan_state(61, true),
            Some(FanPolicyState::Full)
        );
        assert_eq!(
            overtemp_forced_fan_state(60, true),
            Some(FanPolicyState::SafeHalf)
        );
        assert_eq!(
            overtemp_forced_fan_state(40, true),
            Some(FanPolicyState::SafeHalf)
        );
        assert_eq!(overtemp_forced_fan_state(39, true), None);
        assert_eq!(overtemp_forced_fan_state(220, false), None);
    }

    #[test]
    fn protection_alarm_replays_at_one_second_cadence_as_one_shots() {
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut buzzer = BuzzerArbiter::new();
        let _ = protection_alarm.enter(&mut buzzer, 0);

        let _ = buzzer.tick(90);
        let _ = buzzer.tick(130);
        let _ = buzzer.tick(220);
        assert_eq!(buzzer.tick(300).output.frequency_hz, None);

        assert!(!maybe_play_protection_alarm(
            true,
            &mut protection_alarm,
            &mut buzzer,
            999,
        ));
        assert_eq!(buzzer.active_cue(), None);

        assert!(maybe_play_protection_alarm(
            true,
            &mut protection_alarm,
            &mut buzzer,
            1_000,
        ));
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
        assert_eq!(protection_alarm.next_replay_ms(), Some(2_000));
        assert_eq!(buzzer.output().frequency_hz, Some(2_300));
    }

    #[test]
    fn protection_alarm_replays_after_a_late_tick() {
        let mut protection_alarm = ProtectionAlarmCadence::new();
        let mut buzzer = BuzzerArbiter::new();
        let _ = protection_alarm.enter(&mut buzzer, 0);

        // A late executor wake preserves the silent step instead of skipping
        // directly to the next audible pulse.
        assert_eq!(buzzer.tick(1_000).output.frequency_hz, None);
        assert_eq!(buzzer.output().duty_percent, 0);
        assert!(maybe_play_protection_alarm(
            true,
            &mut protection_alarm,
            &mut buzzer,
            1_000,
        ));
        assert_eq!(buzzer.active_cue(), Some(BuzzerCueId::ProtectionAlarm));
        assert_eq!(buzzer.output().frequency_hz, None);
        assert_eq!(protection_alarm.next_replay_ms(), Some(2_000));
    }

    #[test]
    fn attention_pending_consumes_first_input_and_stops_reminders() {
        let mut attention_acknowledged = false;
        let mut attention_pending = true;
        let mut forced_fan_active = true;
        let mut next_reminder_ms = Some(15_000);
        let mut buzzer = BuzzerArbiter::new();
        assert_eq!(buzzer.enter_attention_pending(), None);
        let _ = buzzer.request_attention_reminder(BuzzerCueSource::ThermalAttention, 10_000);

        assert!(acknowledge_overtemp_attention(
            false,
            &mut attention_acknowledged,
            &mut attention_pending,
            &mut forced_fan_active,
            &mut next_reminder_ms,
            &mut buzzer,
        ));
        assert!(attention_acknowledged);
        assert!(!attention_pending);
        assert!(!forced_fan_active);
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
        let mut buzzer = BuzzerArbiter::new();
        assert_eq!(buzzer.enter_attention_pending(), None);

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
        let mut buzzer = BuzzerArbiter::new();

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
        let mut buzzer = BuzzerArbiter::new();

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
            true,
            true,
        );

        assert!(desired);
    }

    #[test]
    fn pd_unavailable_startup_enters_dashboard_with_heater_locked() {
        let pd_contract_ready = startup_pd_contract_ready(None);
        let state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);

        assert_eq!(state.route, FrontPanelRoute::Dashboard);
        assert!(!pd_contract_ready);
        assert_eq!(
            startup_frontpanel_presentation(FrontPanelRuntimeMode::App),
            StartupFrontPanelPresentation::Dashboard
        );
        assert_eq!(
            startup_frontpanel_presentation(FrontPanelRuntimeMode::KeyTest),
            StartupFrontPanelPresentation::Calibration
        );
        assert_eq!(
            next_heater_lock_reason(None, false, true, pd_contract_ready),
            Some(HeaterLockReason::PdContractUnavailable)
        );
        assert!(!reconcile_runtime_heater_enabled(
            true,
            CalibrationRuntimeState::default(),
            None,
            false,
            false,
            true,
            pd_contract_ready,
        ));
    }

    #[test]
    fn sensor_fault_lock_reason_is_exposed_without_relaxing_fail_closed_behavior() {
        assert_eq!(
            next_heater_lock_reason(Some(HeaterFaultReason::SensorOpen), false, true, true,),
            Some(HeaterLockReason::SensorFault)
        );
        assert!(!reconcile_runtime_heater_enabled(
            true,
            CalibrationRuntimeState {
                mode: CalibrationMode::RtdAdc,
                heater_enabled: true,
                ..CalibrationRuntimeState::default()
            },
            Some(HeaterFaultReason::SensorOpen),
            false,
            true,
            true,
            true,
        ));
    }

    #[test]
    fn initial_rtd_fault_latches_dashboard_and_heater_lock() {
        let mut ui_state = FrontPanelUiState::new_startup(FrontPanelRuntimeMode::App);
        let mut heater_controller = HeaterController::new();

        ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault,
        );
        assert!(heater_controller.latch_fault(HeaterFaultReason::SensorOpen));
        assert_eq!(
            ui_state.dashboard_presentation,
            flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault
        );
        assert_eq!(
            next_heater_lock_reason(heater_controller.fault_latched(), false, true, true),
            Some(HeaterLockReason::SensorFault)
        );
        assert!(!reconcile_runtime_heater_enabled(
            true,
            CalibrationRuntimeState {
                mode: CalibrationMode::RtdAdc,
                heater_enabled: true,
                ..CalibrationRuntimeState::default()
            },
            heater_controller.fault_latched(),
            false,
            true,
            true,
            true,
        ));
    }

    #[test]
    fn pd_runtime_service_does_not_block_startup() {
        assert_eq!(STARTUP_PD_WAIT_BUDGET_MS, 0);
    }

    #[test]
    fn pd_contract_loss_relocks_an_armed_heater() {
        let calibration = CalibrationRuntimeState::default();

        assert!(reconcile_runtime_heater_enabled(
            true,
            calibration,
            None,
            false,
            false,
            true,
            true,
        ));
        assert!(!reconcile_runtime_heater_enabled(
            true,
            calibration,
            None,
            false,
            false,
            true,
            false,
        ));
        assert_eq!(
            next_heater_lock_reason(None, false, true, false),
            Some(HeaterLockReason::PdContractUnavailable)
        );
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
            true,
            true,
        );

        assert!(!desired);
    }

    #[test]
    fn failed_thermal_plant_calibration_keeps_heating_locked() {
        let calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Failed,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };

        assert!(!thermal_model_heater_allowed(
            &MemoryConfig::default(),
            calibration,
            ManualPpsState::default(),
        ));
        assert!(!reconcile_runtime_heater_enabled(
            true,
            calibration,
            None,
            false,
            false,
            true,
            true,
        ));
    }

    #[test]
    fn legacy_steady_state_record_never_unlocks_heating() {
        let memory_config = MemoryConfig {
            thermal_plant_active: Some(ThermalPlantRawTransaction {
                transaction_id: 7,
                anchors: [
                    ThermalPlantRawAnchor {
                        ambient_raw_rtd_adc_mv: 250,
                        target_raw_rtd_adc_mv: 700,
                        heater_voltage_mv: 20_000,
                        heater_current_ma: 3_000,
                        gate_off_idle_power_mw: 0,
                        steady_hold_power_mw: 1_000,
                        ramp_duration_ms: 1_000,
                        ramp_energy_mj: 1_000,
                    },
                    ThermalPlantRawAnchor {
                        ambient_raw_rtd_adc_mv: 250,
                        target_raw_rtd_adc_mv: 2_000,
                        heater_voltage_mv: 20_000,
                        heater_current_ma: 3_000,
                        gate_off_idle_power_mw: 0,
                        steady_hold_power_mw: 2_000,
                        ramp_duration_ms: 2_000,
                        ramp_energy_mj: 2_000,
                    },
                ],
            }),
            ..MemoryConfig::default()
        };
        let manual_pps =
            ManualPpsState::from_capabilities(Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(20_000),
                pps_max_ma: Some(5_000),
                ..Default::default()
            }));

        assert!(!thermal_model_heater_allowed(
            &memory_config,
            CalibrationRuntimeState::default(),
            manual_pps,
        ));
    }

    #[test]
    fn thermal_plant_completion_disarm_is_consumed_once() {
        let mut calibration = CalibrationRuntimeState {
            thermal_plant_completion_disarm_pending: true,
            ..CalibrationRuntimeState::default()
        };
        let mut desired_heater_enabled =
            reconcile_runtime_heater_enabled(true, calibration, None, false, false, true, true);
        desired_heater_enabled =
            consume_thermal_plant_completion_disarm(&mut calibration, desired_heater_enabled);
        assert!(!desired_heater_enabled);
        assert!(reconcile_runtime_heater_enabled(
            true,
            calibration,
            None,
            false,
            false,
            true,
            true,
        ));
    }

    #[test]
    fn immediate_heater_disarm_is_consumed_once() {
        let mut calibration = CalibrationRuntimeState {
            immediate_heater_disarm_pending: true,
            ..CalibrationRuntimeState::default()
        };

        assert!(take_immediate_heater_disarm(&mut calibration));
        assert!(!take_immediate_heater_disarm(&mut calibration));
    }

    #[test]
    fn terminal_disarm_locks_pps_until_fixed_pd_write_completes() {
        let calibration = CalibrationRuntimeState {
            immediate_heater_disarm_pending: true,
            ..CalibrationRuntimeState::default()
        };
        let mut backend = select_heater_power_backend(
            Some(ch224q::AdjustablePowerCapabilities {
                pps_covers_20v: true,
                pps_min_mv: Some(5_000),
                pps_max_mv: Some(21_000),
                pps_max_ma: Some(3_000),
                ..Default::default()
            }),
            Some(Status::default()),
        );

        assert!(latch_terminal_fixed_pd_disarm(&calibration, &mut backend));
        assert!(calibration.immediate_heater_disarm_pending);
        assert!(matches!(
            backend,
            HeaterPowerBackend::PpsMos {
                terminal_fixed_pd_disarmed: true,
                ..
            }
        ));
    }

    #[test]
    fn manual_pps_calibration_releases_terminal_disarm_without_enabling_the_heater() {
        let mut backend = HeaterPowerBackend::PpsMos {
            pps_min_mv: 5_000,
            idle_request_mv: 12_000,
            pps_max_mv: 21_000,
            adjustable_max_mv: 21_000,
            capability_max_ma: 5_000,
            current_mode: Some(ch224q::AdjustableVoltageMode::Pps),
            current_request_mv: 20_000,
            settle_until_ms: Some(100),
            next_request_at_ms: 100,
            current_limit_fixed_pwm_active: true,
            current_limit_fixed_request_confirmed: true,
            terminal_fixed_pd_disarmed: true,
        };

        assert!(release_terminal_fixed_pd_disarm_for_manual_pps(
            &mut backend,
            true
        ));
        assert!(matches!(
            backend,
            HeaterPowerBackend::PpsMos {
                terminal_fixed_pd_disarmed: false,
                current_mode: None,
                current_request_mv: 12_000,
                settle_until_ms: None,
                next_request_at_ms: 0,
                current_limit_fixed_pwm_active: false,
                current_limit_fixed_request_confirmed: false,
                ..
            }
        ));
    }

    #[test]
    fn fusb302b_retries_manual_pps_when_the_active_contract_is_fixed() {
        let manual_pps = ManualPpsState {
            enabled: true,
            owner: ManualPpsOwner::Calibration,
            target_mv: Some(20_000),
            target_ma: Some(5_000),
            applied_mv: Some(20_000),
            ..ManualPpsState::default()
        };
        let fixed = PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: 5_000,
            contract_voltage_mv: Some(20_000),
            contract: Contract {
                kind: ContractKind::Fixed,
                object_position: 1,
                voltage_mv: 20_000,
                current_ma: 5_000,
            },
        };
        let pps = PdStatusObservation {
            contract: Contract {
                kind: ContractKind::Pps,
                ..fixed.contract
            },
            ..fixed
        };

        assert!(manual_pps_request_required(
            manual_pps,
            ControllerKind::Fusb302b,
            Some(fixed)
        ));
        assert!(!manual_pps_request_required(
            manual_pps,
            ControllerKind::Fusb302b,
            Some(pps)
        ));
        assert!(!manual_pps_request_required(
            manual_pps,
            ControllerKind::Ch224q,
            Some(fixed)
        ));
    }

    #[test]
    fn terminal_disarm_waits_for_measured_fixed_pd_voltage() {
        let fixed_mv = u32::from(DEFAULT_PD_VOLTAGE_REQUEST.millivolts());
        assert!(!terminal_fixed_pd_voltage_confirmed(
            fixed_mv.saturating_add(9_000)
        ));
        assert!(!terminal_fixed_pd_voltage_confirmed(
            fixed_mv.saturating_add(3_000)
        ));
        assert!(terminal_fixed_pd_voltage_confirmed(
            fixed_mv.saturating_add(450)
        ));
    }

    #[test]
    fn canceling_a_running_job_latches_immediate_disarm_and_preserves_terminal_state() {
        let mut calibration = CalibrationRuntimeState {
            mode: CalibrationMode::ThermalPlant,
            heater_enabled: true,
            job: CalibrationJobState {
                kind: Some(CalibrationJobKind::ThermalPlant),
                status: CalibrationJobStatus::Running,
                ..CalibrationJobState::default()
            },
            ..CalibrationRuntimeState::default()
        };
        let mut manual_pps = ManualPpsState::default();

        calibration_job_canceled(&mut calibration, &mut manual_pps);

        assert_eq!(calibration.job.status, CalibrationJobStatus::Canceled);
        assert_eq!(calibration.mode, CalibrationMode::Off);
        assert!(!calibration.heater_enabled);
        assert!(take_immediate_heater_disarm(&mut calibration));

        calibration.job.status = CalibrationJobStatus::Completed;
        calibration_job_canceled(&mut calibration, &mut manual_pps);
        assert_eq!(calibration.job.status, CalibrationJobStatus::Completed);
        assert!(!take_immediate_heater_disarm(&mut calibration));
    }
}
