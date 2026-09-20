#[allow(unused_imports)]
use super::*;

#[cfg(all(
    target_arch = "xtensa",
    any(feature = "net_http", feature = "web_serial")
))]
pub(crate) use core::fmt::Write as _;
#[cfg(target_arch = "xtensa")]
extern crate alloc;
#[cfg(target_arch = "xtensa")]
pub(crate) use alloc::boxed::Box;
#[cfg(target_arch = "xtensa")]
pub(crate) use allocator_api2::boxed::Box as AllocBox;
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) use core::cell::RefCell;
#[cfg(target_arch = "xtensa")]
pub(crate) use core::future::Future;
#[cfg(target_arch = "xtensa")]
pub(crate) use core::sync::atomic::AtomicU32;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use core::sync::atomic::{AtomicU8, AtomicU16, Ordering};
#[cfg(target_arch = "xtensa")]
pub(crate) use core::{mem::MaybeUninit, panic::PanicInfo};
#[cfg(target_arch = "xtensa")]
pub(crate) use defmt::{info, warn};
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice as SharedI2cDevice;
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_executor::Spawner;
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_futures::select::{Either, Either3, select, select3};
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_sync::mutex::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
#[cfg(target_arch = "xtensa")]
pub(crate) use embassy_time::{Duration, Instant, Timer as EmbassyTimer};
#[cfg(target_arch = "xtensa")]
pub(crate) use embedded_graphics::pixelcolor::Rgb565;
#[cfg(target_arch = "xtensa")]
pub(crate) use embedded_graphics::prelude::RgbColor;
#[cfg(target_arch = "xtensa")]
pub(crate) use embedded_hal::pwm::SetDutyCycle;
#[cfg(target_arch = "xtensa")]
pub(crate) use embedded_hal_async::i2c::I2c as AsyncI2c;
#[cfg(target_arch = "xtensa")]
pub(crate) use embedded_hal_bus::spi::ExclusiveDevice;
#[cfg(target_arch = "xtensa")]
pub(crate) use esp_alloc::EspHeap;
#[cfg(target_arch = "xtensa")]
pub(crate) use esp_hal::rtc_cntl::SocResetReason;
#[cfg(target_arch = "xtensa")]
pub(crate) use esp_hal::{
    Async, Blocking,
    analog::adc::{
        Adc, AdcCalBasic, AdcCalCurve, AdcCalScheme, AdcChannel, AdcConfig, Attenuation,
    },
    efuse::{AdcCalibUnit, Efuse},
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c as HalI2c, SoftwareTimeout},
    interrupt::{Priority, software::SoftwareInterruptControl},
    mcpwm::{
        McPwm, PeripheralClockConfig,
        operator::{Operator, PwmActions, PwmPin, PwmPinConfig, PwmUpdateMethod},
        timer::{CounterDirection, PwmWorkingMode, Timer},
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
pub(crate) use esp_hal::{
    gpio::Pin,
    pcnt::{Pcnt, channel::EdgeMode, unit::Unit},
};
#[cfg(target_arch = "xtensa")]
pub(crate) use esp_rtos::embassy::InterruptExecutor;
#[cfg(test)]
pub(crate) use flux_purr_firmware::DEFAULT_PD_VOLTAGE_REQUEST;
#[cfg(test)]
pub(crate) use flux_purr_firmware::adapters::ch224q;
#[cfg(test)]
pub(crate) use flux_purr_firmware::adapters::ch224q::Status;
#[cfg(test)]
pub(crate) use flux_purr_firmware::adapters::fusb302b;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::adapters::fusb302b::SinkPhase;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::adapters::pd::SourceCapabilities;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::adapters::pd::{
    Contract, ContractKind, ControllerKind, FUSB302B_PPS_MAX_MV,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::adapters::pd::{
    FUSB302B_PPS_MIN_MV, GUARANTEED_HEATER_MIN_MV, MAX_HEATER_CONTRACT_MA, MIN_HEATER_CONTRACT_MA,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::board::s3_frontpanel;
#[cfg(target_arch = "xtensa")]
pub(crate) use flux_purr_firmware::buzzer::BuzzerDecision;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::buzzer::BuzzerOutput;
#[cfg(test)]
pub(crate) use flux_purr_firmware::buzzer::PROTECTION_ALARM_INTERVAL_MS;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::buzzer::{
    BuzzerArbiter, BuzzerCueId, BuzzerCueSource, ProtectionAlarmCadence,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-observe"))]
pub(crate) use flux_purr_firmware::buzzer_test::{
    BUZZER_TEST_OUTPUT_TRACE_CAPACITY, BuzzerTestOutputTraceEvent,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) use flux_purr_firmware::buzzer_test::{
    BuzzerTestSession, BuzzerTestSessionState, BuzzerTestStatus,
};
#[cfg(all(test, feature = "buzzer-test", not(target_arch = "xtensa")))]
pub(crate) use flux_purr_firmware::control_plane::BuzzerTestOp;
#[cfg(any(test, all(target_arch = "xtensa", feature = "web_serial")))]
pub(crate) use flux_purr_firmware::control_plane::EepromMaintenanceOp;
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) use flux_purr_firmware::control_plane::LanPairingCode;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) use flux_purr_firmware::control_plane::ThermalControlProfileCommand;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::WifiConfigCommand;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::WifiConfigOp;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::WifiConfigReceipt;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::hello_frame;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) use flux_purr_firmware::control_plane::{
    AdcCalibrationSourceWire, AdcDiagnosticsWire, ApiError, CalibrationControlCommand,
    CalibrationJobKindWire, CalibrationJobStateWire, CalibrationJobStatusWire, CalibrationModeWire,
    CalibrationRuntimeStateWire, ControlPlaneStatus, HeaterCurvePackageWire, Identity,
    InstallRuntimeSnapshot, InstallStatus, PersistenceFault, RuntimeConfigCommand,
    ThermalControlProfileOp, ThermalControlProfilePointWire, ThermalControlProfileSettingsWire,
    ThermalControlProfileWire, ThermalControlRuntimeWire, ThermalPlantActiveResultWire,
    ThermalPlantProvisionalCurveWire, ThermalPlantRunAttemptWire, ThermalPlantRunPhaseWire,
    ThermalPlantRunSnapshotWire, ThermalPlantRuntimeWire, ThermalPlantTracePageWire,
    ThermalPlantTracePointWire, UsbFrame, UsbFrameError, UsbRequestOp, UsbResponsePayload,
    calibration_state_from_memory, heater_curve_state_from_memory, network_from_memory,
    parse_usb_frame, write_usb_frame,
};
#[cfg(all(target_arch = "xtensa", feature = "buzzer-test"))]
pub(crate) use flux_purr_firmware::control_plane::{BuzzerTestCommand, BuzzerTestOp};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) use flux_purr_firmware::control_plane::{
    CalibrationChannelWire, CalibrationConfigCommand, CalibrationConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::{
    CalibrationJobCommandWire, CalibrationJobOpWire, EepromMaintenanceCommand,
    HeaterCurveConfigCommand, HeaterCurveConfigOp,
};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use flux_purr_firmware::control_plane::{
    CalibrationSampleWire, CalibrationSlotFitWire, CalibrationSlotIdWire, CalibrationStateWire,
    samples_from_wire,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::fan_policy::{
    FanOutputLevel, FanPolicySource, HeatingFanGuardMode, PostHeatCoolingMode,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::frontpanel::{
    FRONTPANEL_PRESET_COUNT, FRONTPANEL_TARGET_TEMP_MAX_C, FRONTPANEL_TARGET_TEMP_MIN_C,
    FanDisplayState, FrontPanelKeyMap, FrontPanelRawState, FrontPanelRoute, FrontPanelRuntimeMode,
    FrontPanelUiState, HeaterLockReason,
};
#[cfg(test)]
pub(crate) use flux_purr_firmware::frontpanel::{
    FrontPanelKey, KeyEvent, KeyGesture, RawFrontPanelKey,
};
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) use flux_purr_firmware::lan::LanEndpoint;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) use flux_purr_firmware::memory::{
    ADC_CALIBRATION_MAX_SAMPLES, AdcCalibrationSample, HEATER_CURVE_MAX_POINTS, HeaterCurvePoint,
    HeaterCurveRawObservation, HeaterCurveRawObservations,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::memory::{AdcCalibrationChannel, correct_adc_mv};
#[cfg(target_arch = "xtensa")]
pub(crate) use flux_purr_firmware::memory::{
    EepromError, FPR2_HEADER_LEN, FPR2_LAYOUT_A_OFFSET, FPR2_LAYOUT_B_OFFSET,
    FPR2_LAYOUT_SLOT_SIZE, FPR2_MAX_RECORD_SIZE, FPR2_NETWORK_OFFSET, FPR2_PREFERENCES_OFFSET,
    FPR2_SAFETY_A_OFFSET, FPR2_SAFETY_B_OFFSET, FPR2_SAFETY_SLOT_SIZE, FPR2_THERMAL_A_OFFSET,
    FPR2_THERMAL_B_OFFSET, FPR2_THERMAL_PLANT_OFFSET, FPR2_THERMAL_SLOT_SIZE,
    LEGACY_MEMORY_SLOT_A_OFFSET, LEGACY_MEMORY_SLOT_B_OFFSET, LEGACY_MEMORY_SLOT_SIZE,
    LayoutMarker, LayoutMarkerKind, LayoutMarkerStatus, M24C64_CAPACITY_BYTES, M24C64_I2C_ADDRESS,
    M24c64, MEMORY_RECORD_FORMAT_VERSION, MEMORY_RECORD_HEADER_LEN, MEMORY_SLOT_A_OFFSET,
    MEMORY_SLOT_B_OFFSET, MEMORY_SLOT_SIZE, MEMORY_WRITE_DEBOUNCE_MS, MemoryRecord,
    NetworkAndPairing, PREVIOUS_MEMORY_SLOT_A_OFFSET, PREVIOUS_MEMORY_SLOT_B_OFFSET,
    PREVIOUS_MEMORY_SLOT_SIZE, PersistDomain, PersistDomainData, PersistRecord, PersistSlot,
    SafetyCalibration, ThermalPlantPersistence, ThermalPolicy, UserPreferences,
    apply_legacy_config_tlv, decode_persist_record, encode_persist_record,
    fpr2_prepared_generation_is_complete, fpr2_snapshot_is_complete, persistence_crc32_update,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::memory::{
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
    ThermalProfileMode, heater_resistance_ohms_from_curve,
    quantize_thermal_plant_heater_voltage_mv, thermal_plant_heater_voltage_mv,
    thermal_plant_projection_from_transient,
};
#[cfg(test)]
pub(crate) use flux_purr_firmware::memory::{
    MEMORY_RECORD_FORMAT_VERSION, MEMORY_RECORD_HEADER_LEN, MEMORY_SLOT_SIZE, MemoryRecord,
    decode_memory_record, encode_memory_record,
};
#[cfg(test)]
pub(crate) use flux_purr_firmware::memory::{ThermalPlantRawAnchor, ThermalPlantRawTransaction};
#[cfg(all(target_arch = "xtensa", feature = "net_http"))]
pub(crate) use flux_purr_firmware::net_http::{
    ControlMailboxCommand, HttpMethod, LAN_HTTP_BODY_MAX_LEN,
};
#[cfg(target_arch = "xtensa")]
pub(crate) use flux_purr_firmware::status_light::{
    RgbChannels, StatusLightInputs, StatusLightState, select_status_light_state,
    status_light_output,
};
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) use flux_purr_firmware::thermal_plant::{
    ThermalPlantControlInput, ThermalPlantController,
};
#[cfg(target_arch = "xtensa")]
pub(crate) use flux_purr_firmware::{
    DEFAULT_PD_VOLTAGE_REQUEST, FAN_PWM_FREQUENCY_HZ, pwm_percent_from_permille,
};
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) use flux_purr_firmware::{DeviceMode, DeviceStatus, PdState};
#[cfg(target_arch = "xtensa")]
pub(crate) use flux_purr_firmware::{
    adapters::{
        ch224q::{self, Status},
        fusb302b,
    },
    display::{DISPLAY_PANEL_CONFIG, DisplayCanvas, SceneId, render_scene},
    frontpanel::{
        FRONTPANEL_DEBOUNCE_MS, FRONTPANEL_DOUBLE_CLICK_MS, FrontPanelInputController,
        FrontPanelInputTimings, KeyGesture, RawFrontPanelKey, render::render_frontpanel_ui,
    },
};
#[cfg(target_arch = "xtensa")]
pub(crate) use fusb302::{
    CcPin, CcPull, DataRole, Fusb302, InterruptMasks, PdPacket, PdRevision, PhyConfig, PowerRole,
    RetryCount, SopType, ToggleMode,
};
#[cfg(target_arch = "xtensa")]
pub(crate) use gc9d01::{GC9D01, Timer as Gc9d01Timer};
#[cfg(target_arch = "xtensa")]
pub(crate) use micromath::F32Ext;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use serde::{Deserialize, Serialize};
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) use sha2::{Digest, Sha256};

/// All I2C0 users share one async-arbitrated bus. The PD and EEPROM users run on
/// the normal executor, so the ESP-HAL async driver can await each interrupt-
/// driven transfer without crossing into the interrupt executor's `SendSpawner`.
#[cfg(target_arch = "xtensa")]
pub(crate) type I2c<'a> = SharedI2cDevice<'a, CriticalSectionRawMutex, HalI2c<'static, Async>>;

#[cfg(target_arch = "xtensa")]
pub(crate) type SharedI2cBus = AsyncMutex<CriticalSectionRawMutex, HalI2c<'static, Async>>;

#[cfg(target_arch = "xtensa")]
pub(crate) static mut I2C_BUS_STORAGE: MaybeUninit<SharedI2cBus> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdI2cError {
    BusBusy,
    I2c(esp_hal::i2c::master::Error),
}

#[cfg(target_arch = "xtensa")]
impl embedded_hal::i2c::Error for PdI2cError {
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        match self {
            Self::BusBusy => embedded_hal::i2c::ErrorKind::Other,
            Self::I2c(error) => embedded_hal::i2c::Error::kind(error),
        }
    }
}

/// PD's view of the shared I2C bus never waits for the EEPROM task. The
/// service task tries to acquire the bus at the start of a turn, holds it only
/// for that bounded PD turn, and retries on the next tick when EEPROM owns it.
#[cfg(target_arch = "xtensa")]
pub(crate) struct PdI2c<'a> {
    bus: &'a SharedI2cBus,
    guard: Option<AsyncMutexGuard<'a, CriticalSectionRawMutex, HalI2c<'static, Async>>>,
}

#[cfg(target_arch = "xtensa")]
impl<'a> PdI2c<'a> {
    pub(crate) fn new(bus: &'a SharedI2cBus) -> Self {
        Self { bus, guard: None }
    }

    pub(crate) fn try_acquire(&mut self) -> bool {
        if self.guard.is_some() {
            return true;
        }
        self.guard = self.bus.try_lock().ok();
        self.guard.is_some()
    }

    pub(crate) fn release(&mut self) {
        self.guard = None;
    }

    fn bus_mut(&mut self) -> Result<&mut HalI2c<'static, Async>, PdI2cError> {
        self.guard.as_deref_mut().ok_or(PdI2cError::BusBusy)
    }
}

#[cfg(target_arch = "xtensa")]
impl embedded_hal::i2c::ErrorType for PdI2c<'_> {
    type Error = PdI2cError;
}

#[cfg(target_arch = "xtensa")]
impl embedded_hal_async::i2c::I2c for PdI2c<'_> {
    async fn read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Self::Error> {
        AsyncI2c::read(self.bus_mut()?, address, read)
            .await
            .map_err(PdI2cError::I2c)
    }

    async fn write(&mut self, address: u8, write: &[u8]) -> Result<(), Self::Error> {
        AsyncI2c::write(self.bus_mut()?, address, write)
            .await
            .map_err(PdI2cError::I2c)
    }

    async fn write_read(
        &mut self,
        address: u8,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        AsyncI2c::write_read(self.bus_mut()?, address, write, read)
            .await
            .map_err(PdI2cError::I2c)
    }

    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal_async::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        AsyncI2c::transaction(self.bus_mut()?, address, operations)
            .await
            .map_err(PdI2cError::I2c)
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) type RawHeaterPwm = PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 1, true>;

#[cfg(target_arch = "xtensa")]
static HEATER_PWM_STORAGE: BlockingMutex<CriticalSectionRawMutex, RefCell<Option<RawHeaterPwm>>> =
    BlockingMutex::new(RefCell::new(None));

/// The PD task can revoke this permit and clear the physical PWM without
/// waiting for the front-panel executor to reach its next control iteration.
#[cfg(target_arch = "xtensa")]
pub(crate) static PD_HEATER_PERMIT: AtomicU8 = AtomicU8::new(0);

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_HEATER_PERMIT_EXPIRES_AT_MS: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Default)]
pub(crate) struct HeaterPwmGate;

#[cfg(target_arch = "xtensa")]
impl HeaterPwmGate {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) fn attach(pwm: RawHeaterPwm) {
        HEATER_PWM_STORAGE.lock(|slot| {
            *slot.borrow_mut() = Some(pwm);
        });
    }

    pub(crate) fn force_off() {
        PD_HEATER_PERMIT.store(0, Ordering::Release);
        PD_HEATER_PERMIT_EXPIRES_AT_MS.store(0, Ordering::Release);
        let mut gate = Self::new();
        let _ = gate.set_duty_cycle(0);
    }
}

#[cfg(target_arch = "xtensa")]
impl embedded_hal::pwm::ErrorType for HeaterPwmGate {
    type Error = core::convert::Infallible;
}

#[cfg(target_arch = "xtensa")]
impl SetDutyCycle for HeaterPwmGate {
    fn max_duty_cycle(&self) -> u16 {
        HEATER_PWM_STORAGE.lock(|slot| {
            slot.borrow()
                .as_ref()
                .map(SetDutyCycle::max_duty_cycle)
                .unwrap_or(100)
        })
    }

    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        HEATER_PWM_STORAGE.lock(|slot| {
            let effective_duty = if heater_permit_is_active(Instant::now().as_millis()) {
                duty
            } else {
                PD_HEATER_PERMIT.store(0, Ordering::Release);
                0
            };
            if let Some(pwm) = slot.borrow_mut().as_mut() {
                let _ = pwm.set_duty_cycle(effective_duty);
            }
        });
        Ok(())
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PD_SNAPSHOT_MAX_AGE_MS: u64 = 100;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn pd_snapshot_is_fresh(published_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(published_at_ms) <= PD_SNAPSHOT_MAX_AGE_MS
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn heater_permit_is_active(now_ms: u64) -> bool {
    let now = now_ms as u32;
    let expires_at = PD_HEATER_PERMIT_EXPIRES_AT_MS.load(Ordering::Acquire);
    PD_HEATER_PERMIT.load(Ordering::Acquire) != 0 && (now.wrapping_sub(expires_at) as i32) < 0
}

#[cfg(target_arch = "xtensa")]
esp_bootloader_esp_idf::esp_app_desc!();

// Boot handoffs allocate complete runtime states after display initialization.
// They need the same headroom regardless of whether the optional LAN task is
// linked: a no-network diagnostic image must remain a valid boot diagnostic.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RUNTIME_HEAP_SIZE: usize = 52 * 1024;

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".dram2_uninit")]
pub(crate) static mut RUNTIME_HEAP_STORAGE: MaybeUninit<[u8; RUNTIME_HEAP_SIZE]> =
    MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
pub(crate) fn init_runtime_heap() {
    // Wi-Fi heap and the USB response buffer share post-boot DRAM2. Keeping
    // the response buffer out of the Embassy task leaves enough primary DRAM
    // for the startup stack while the enlarged heap covers status snapshots.
    // This region is NOLOAD, so it retains arbitrary bytes across software
    // resets. Clear it before registration because the Wi-Fi binary embeds
    // ETS timers in heap objects and treats an initial non-null `priv_` field
    // as a live RTOS timer.
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

#[cfg(target_arch = "xtensa")]
pub(crate) const PSRAM_SIZE_BYTES: usize = 2 * 1024 * 1024;

#[cfg(target_arch = "xtensa")]
pub(crate) static DISPLAY_GRAPHICS_HEAP: EspHeap = EspHeap::empty();

#[cfg(target_arch = "xtensa")]
pub(crate) const DISPLAY_FRAMEBUFFER_BYTES: usize =
    flux_purr_firmware::display::DISPLAY_PIXELS * core::mem::size_of::<Rgb565>();

#[cfg(target_arch = "xtensa")]
pub(crate) const DISPLAY_SPI_FREQUENCY_HZ: u32 = 40_000_000;

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayGraphicsInitError {
    PsramUnavailable,
    FramebufferAllocationFailed,
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_display_graphics(
    psram: &esp_hal::peripherals::PSRAM<'static>,
) -> Result<
    &'static mut [Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS],
    DisplayGraphicsInitError,
> {
    let (start, size) = esp_hal::psram::psram_raw_parts(psram);
    if size != PSRAM_SIZE_BYTES {
        return Err(DisplayGraphicsInitError::PsramUnavailable);
    }

    // The graphics heap owns the mapped PSRAM region exclusively. It is
    // intentionally separate from esp_alloc::HEAP so control-plane objects
    // remain in the internal runtime heap.
    unsafe {
        DISPLAY_GRAPHICS_HEAP.add_region(esp_alloc::HeapRegion::new(
            start,
            size,
            esp_alloc::MemoryCapability::External.into(),
        ));
    }

    let mut framebuffer = AllocBox::try_new_uninit_in(&DISPLAY_GRAPHICS_HEAP)
        .map_err(|_| DisplayGraphicsInitError::FramebufferAllocationFailed)?;
    // Initialize the PSRAM allocation in place. Constructing the 16 KiB array
    // as a function argument would materialize it on the guarded boot stack.
    unsafe {
        let pixels = framebuffer.as_mut_ptr().cast::<Rgb565>();
        for index in 0..flux_purr_firmware::display::DISPLAY_PIXELS {
            pixels.add(index).write(Rgb565::BLACK);
        }
    }
    let framebuffer = unsafe { framebuffer.assume_init() };
    info!(
        "display graphics memory=psram bytes={=u32} framebuffer_bytes={=u32}",
        size as u32, DISPLAY_FRAMEBUFFER_BYTES as u32,
    );
    Ok(AllocBox::leak(framebuffer))
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[unsafe(link_section = ".uninit")]
pub(crate) static mut USB_CONTROL_RESPONSE_BUFFER: MaybeUninit<[u8; USB_CONTROL_TX_BUFFER_LEN]> =
    MaybeUninit::uninit();

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
static mut USB_CONTROL_RX_LINE: heapless::String<USB_CONTROL_LINE_CAPACITY> =
    heapless::String::new();

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".dram2_uninit")]
pub(crate) static mut DISPLAY_CANVAS_STORAGE: MaybeUninit<DisplayCanvas> = MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
#[unsafe(link_section = ".uninit")]
pub(crate) static mut EEPROM_RECORD_STAGING_STORAGE: MaybeUninit<
    [u8; EEPROM_RECORD_STAGING_BYTES],
> = MaybeUninit::uninit();

/// Overwrite retained runtime storage after an ESP software reset.
///
/// ESP application resets do not guarantee that the previous application's
/// runtime markers have been cleared. The caller must ensure that no task from
/// the previous application instance can still access the storage.
#[cfg(target_arch = "xtensa")]
pub(crate) unsafe fn initialize_after_software_reset<T>(
    storage: *mut MaybeUninit<T>,
    value: T,
) -> &'static mut T {
    // SAFETY: each storage slot is initialized once during the current boot,
    // before any current-boot task receives its reference.
    unsafe { (*storage).write(value) }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct MemoryIoScratch {
    pub(crate) bytes: [u8; EEPROM_WRITE_CHUNK_MAX_BYTES],
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct SensitiveEepromStaging<'a> {
    pub(crate) bytes: &'a mut [u8],
}

#[cfg(target_arch = "xtensa")]
impl<'a> SensitiveEepromStaging<'a> {
    pub(crate) fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
}

#[cfg(target_arch = "xtensa")]
impl Drop for SensitiveEepromStaging<'_> {
    fn drop(&mut self) {
        zeroize_bytes_volatile(self.bytes);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn zeroize_bytes_volatile(bytes: &mut [u8]) {
    for byte in bytes {
        // SAFETY: `byte` is an exclusive reference to initialized memory. A
        // volatile write keeps the scrub from being removed before deallocation.
        unsafe { core::ptr::write_volatile(byte, 0) };
    }
}

#[cfg(target_arch = "xtensa")]
impl Drop for MemoryIoScratch {
    fn drop(&mut self) {
        zeroize_bytes_volatile(&mut self.bytes);
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn new_memory_io_scratch() -> MemoryIoScratch {
    MemoryIoScratch {
        bytes: [0; EEPROM_WRITE_CHUNK_MAX_BYTES],
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn try_allocate_memory_io_scratch() -> Option<MemoryIoScratch> {
    Some(new_memory_io_scratch())
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn initialize_usb_control_response_buffer()
-> &'static mut [u8; USB_CONTROL_TX_BUFFER_LEN] {
    // SAFETY: this is called exactly once from the sole front-panel executor
    // before USB control handling starts. `MaybeUninit::write` initializes the
    // whole DRAM2 slot on every boot, so retained RAM contents are never read.
    unsafe {
        (&mut *core::ptr::addr_of_mut!(USB_CONTROL_RESPONSE_BUFFER))
            .write([0; USB_CONTROL_TX_BUFFER_LEN])
    }
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) fn initialize_usb_control_rx_line()
-> &'static mut heapless::String<USB_CONTROL_LINE_CAPACITY> {
    // Keep the 8 KiB transport buffer out of both the boot future stack and
    // the internal runtime heap reserved for Wi-Fi task stacks. The string has
    // no async state, so clearing it before ownership moves to this boot is
    // sufficient after a software reset.
    unsafe {
        let line = &mut *core::ptr::addr_of_mut!(USB_CONTROL_RX_LINE);
        line.clear();
        line
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_display_canvas() -> &'static mut DisplayCanvas {
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
pub(crate) fn initialize_eeprom_record_staging() -> &'static mut [u8; EEPROM_RECORD_STAGING_BYTES] {
    // The staging area holds transient EEPROM records, never I2C transaction
    // buffers. Reinitialize retained DRAM on each boot before using it.
    unsafe {
        (&mut *core::ptr::addr_of_mut!(EEPROM_RECORD_STAGING_STORAGE))
            .write([0; EEPROM_RECORD_STAGING_BYTES])
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn reset_reason_log_line(reason: Option<SocResetReason>) -> &'static str {
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
pub(crate) struct UsbControlNoopLogger;

#[cfg(target_arch = "xtensa")]
unsafe impl defmt::Logger for UsbControlNoopLogger {
    fn acquire() {}

    unsafe fn flush() {}

    unsafe fn release() {}

    unsafe fn write(_bytes: &[u8]) {}
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn rom_log_line(line: &[u8]) {
    unsafe extern "C" {
        fn esp_rom_output_tx_one_char(value: u8) -> i32;
    }
    for byte in line {
        // SAFETY: this ROM routine is available on ESP32-S3 and accepts one byte.
        unsafe { esp_rom_output_tx_one_char(*byte) };
    }
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) fn rom_boot_stage(stage: &[u8]) {
    rom_log_line(b"boot_rom=");
    rom_log_line(stage);
    rom_log_line(b"\n");
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct RomPanicWriter;

#[cfg(target_arch = "xtensa")]
impl core::fmt::Write for RomPanicWriter {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        rom_log_line(value.as_bytes());
        Ok(())
    }
}

#[cfg(target_arch = "xtensa")]
#[panic_handler]
pub(crate) fn panic(info: &PanicInfo<'_>) -> ! {
    rom_log_line(b"panic=firmware_fault\n");
    let mut writer = RomPanicWriter;
    let _ = core::fmt::Write::write_fmt(
        &mut writer,
        format_args!("panic_message={}\n", info.message()),
    );
    if let Some(location) = info.location() {
        let _ = core::fmt::Write::write_fmt(
            &mut writer,
            format_args!("panic_location={}:{}\n", location.file(), location.line()),
        );
    }
    esp_hal::rom::ets_delay_us(250_000);
    esp_hal::system::software_reset()
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) struct RawUsbSerialJtag {
    pub(crate) inner: UsbSerialJtag<'static, Blocking>,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
impl RawUsbSerialJtag {
    pub(crate) fn new(usb_device: esp_hal::peripherals::USB_DEVICE<'static>) -> Self {
        Self {
            inner: UsbSerialJtag::new(usb_device),
        }
    }

    pub(crate) fn read_byte(&mut self) -> nb::Result<u8, ()> {
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
pub(crate) const HEATER_PID_TARGET_MIN_C: i16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PID_TARGET_MAX_C: i16 = 400;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const ACTIVE_COOLING_FAN_MIN_TEMP_C: i16 = 35;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FORCED_COOLING_FAN_MIN_TEMP_C: i16 = 40;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FORCED_COOLING_FAN_FULL_TEMP_C: i16 = 60;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTO_COOLING_FAN_COOLDOWN_MS: u64 = 30_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const COOLING_DISABLED_PULSE_START_TEMP_C: i16 = 100;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const COOLING_DISABLED_HEATER_LOCK_TEMP_C: i16 = 350;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const COOLING_DISABLED_FAN_FULL_TEMP_C: i16 = 360;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HARD_CUTOFF_TEMP_C: i16 = 420;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) const HEATER_PROFILE_TICK_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
// Keep the per-cycle RTD aggregate unchanged while doubling control and RTD update cadence.
pub(crate) const HEATER_CONTROL_INTERVAL_MS: u64 = 50;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SNAPSHOT_REFRESH_INTERVAL_MS: u64 = 5;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) const PD_RUNTIME_USB_BYTE_BUDGET: u16 = 256;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn pd_runtime_elapsed_ms(started_at_ms: u64, now_ms: u64) -> u64 {
    now_ms.saturating_sub(started_at_ms)
}

/// Keep PD protocol deadlines in the monotonic clock domain. Thermal and UI
/// code intentionally uses a relative runtime clock, so the type boundary
/// prevents those values from reaching FUSB302B request timestamps.
#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PdTimestamp(u64);

#[cfg(any(target_arch = "xtensa", test))]
impl PdTimestamp {
    #[cfg(target_arch = "xtensa")]
    pub(crate) fn now() -> Self {
        Self(Instant::now().as_millis())
    }

    #[cfg(test)]
    pub(crate) const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    pub(crate) const fn as_millis(self) -> u64 {
        self.0
    }
}

// The plant delay is calibrated in seconds, so its predictor must not amplify
// sub-second RTD quantisation into a multi-degree correction.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const THERMAL_PLANT_SLOPE_FILTER_ALPHA: f32 = 0.025;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HOLD_PHASE_HYSTERESIS_C: f32 = 0.10;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) const DASHBOARD_WARNING_BLINK_HALF_PERIOD_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_ADJUSTABLE_MIN_MV: u16 = 12_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FUSB302B_INITIAL_PPS_REQUEST_MV: u16 = HEATER_ADJUSTABLE_MIN_MV;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_ADJUSTABLE_MAX_MV: u16 = 28_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const CH224Q_ADJUSTABLE_REQUEST_MIN_MV: u16 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PPS_REQUEST_HYSTERESIS_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PPS_REQUEST_STEP_MV: u16 = 500;
#[cfg(any(target_arch = "xtensa", test))]
// VIN is measured at the heater while PPS is requested at the source.
// Bound compensation so stale measurements cannot cause a large source jump.
pub(crate) const HEATER_PPS_PATH_DROP_COMPENSATION_MAX_MV: u16 = 2_500;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HOLD_PPS_INITIAL_SETTLE_MS: u64 = 10_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HOLD_PPS_STEADY_DWELL_MS: u64 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
// A current-limited high-temperature plate can be power-bound before the
// physical PWM reaches 98%.  Treat 80% as near saturation so PPS can recover
// the remaining voltage headroom without waiting for an unreachable duty.
pub(crate) const HEATER_HOLD_PPS_SATURATION_PWM_MIN_PERCENT: u8 = 80;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HOLD_PPS_RAISE_ERROR_MIN_C: f32 = 0.25;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_HOLD_PPS_RAISE_MAX_SLOPE_C_PER_S: f32 = 0.25;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PPS_SMALL_TRANSITION_MS: u64 = 500;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PPS_LARGE_TRANSITION_MS: u64 = 275;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg(test)]
pub(crate) const FAN_PULSE_PERIOD_MS: u64 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
#[cfg(test)]
pub(crate) const HEATING_FAN_PULSE_MAX_DUTY_PERCENT: u8 = 50;
#[cfg(target_arch = "xtensa")]
pub(crate) const DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS: u64 = 1_000;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) const USB_CONTROL_LINE_CAPACITY: usize =
    flux_purr_firmware::control_plane::USB_LINE_CONTENT_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) const USB_CONTROL_TX_BUFFER_LEN: usize =
    flux_purr_firmware::control_plane::USB_LINE_MAX_LEN;
#[cfg(any(all(target_arch = "xtensa", feature = "web_serial"), test))]
pub(crate) const USB_CONTROL_TX_PACKET_LEN: usize = 64;
#[cfg(target_arch = "xtensa")]
pub(crate) const USB_CONTROL_TX_PACKET_BUDGET: usize = 128;
#[cfg(target_arch = "xtensa")]
// Runtime responses are emitted cooperatively, one USB packet per loop turn.
// Keep the deadline bounded, but long enough for the largest response buffer
// to drain without aborting a valid JSONL frame.
pub(crate) const USB_CONTROL_RESPONSE_TIMEOUT_MS: u64 = 10_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FAN_FULL_SPEED_PWM_PERMILLE: u16 = 0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FAN_HALF_SPEED_PWM_PERMILLE: u16 = 250;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE: u16 = 1_000;
#[cfg(test)]
pub(crate) const HEATER_APPROACH_DUTY_PERCENT: u8 = 32;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PROFILE_R20_OHMS: f32 = 3.2;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_CURVE_COLD_ANCHOR_TEMP_C: f32 = 0.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_CURVE_R20_ANCHOR_TEMP_C: f32 = 20.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PROFILE_TEMP_COEFFICIENT_PER_C: f32 = 0.00393;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_CURRENT_LIMIT_FALLBACK_REQUEST: ch224q::VoltageRequest =
    ch224q::VoltageRequest::V9;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_CURRENT_LIMIT_RETURN_HYSTERESIS_MV: u16 = 200;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PWM_FREQUENCY_HZ: u32 = 100;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const MCPWM_PERIPHERAL_CLOCK_HZ: u32 = 40_000_000;
#[cfg(test)]
pub(crate) const MCPWM_TIMER_MAX_PRESCALER: u32 = 255;
#[cfg(target_arch = "xtensa")]
pub(crate) const FAN_PWM_PERIOD_TICKS: u16 = 99;
// MCPWM's timer prescaler is only eight bits. At 40 MHz, 100 Hz needs at
// least 1,563 timer counts; 1,600 counts gives an exact 100 Hz period.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_PWM_PERIOD_TICKS: u16 = 1_599;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const HEATER_WARMUP_SOFT_START_MS: u64 = 1_000;
// Timer2 keeps one clock divider for its lifetime. Cue pitch is selected with
// the period register because ESP32-S3 can report a new timer prescaler in
// CFG0 while the GPIO matrix continues emitting the previous carrier.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const BUZZER_TIMER_PRESCALER: u8 = 3;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const BUZZER_IDLE_FREQUENCY_HZ: u32 = 2_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const BUZZER_ATTENTION_REMINDER_INTERVAL_MS: u64 = 10_000;
#[cfg(target_arch = "xtensa")]
pub(crate) const STATUS_LIGHT_BOOT_DURATION_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RUNTIME_READY_BOOT_STAGE_LINE: &[u8] = b"boot_stage=runtime_ready\n";
#[cfg(target_arch = "xtensa")]
pub(crate) const RTD_SAMPLE_ATTENUATION: Attenuation = Attenuation::_6dB;
#[cfg(any(target_arch = "xtensa", test))]
// Sample each 1 ms phase across the full 100 Hz MOS period. A contiguous ADC
// burst can otherwise land on one PWM phase and report switching noise as a
// temperature change. This is acquisition timing, not a display filter.
pub(crate) const RTD_SAMPLE_COUNT: usize = 80;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_SAMPLE_PWM_PHASE_COUNT: usize = 10;
#[cfg(target_arch = "xtensa")]
pub(crate) const RTD_SAMPLE_PWM_PHASE_SPACING_US: u32 = 1_000;
#[cfg(target_arch = "xtensa")]
// ADC1 alternates between the low-impedance VIN divider and the filtered,
// high-impedance RTD node. Let the RTD node recover in real time before the
// conversion discard prefix; conversion count alone does not provide a stable
// RC settling interval across ADC clock conditions.
pub(crate) const RTD_CHANNEL_SWITCH_SETTLE_US: u32 = 5_000;
#[cfg(any(target_arch = "xtensa", test))]
// ADC1 is shared by the high-impedance RTD divider and the VIN divider. Keep
// a longer discard prefix after every channel switch so the retained batch is
// not biased by the preceding channel's sample-and-hold residue.
pub(crate) const RTD_SETTLE_DISCARD_SAMPLE_COUNT: usize = 96;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_MIN_VALID_SAMPLE_COUNT: usize = 60;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_RETRY_AFTER_VIN_STEP_RAW_ADC_DELTA_MV: u16 = 48;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_SAMPLE_STABLE_AFTER_REQUEST_MS: u64 = 300;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_MAX_SLEW_C_PER_S: f32 = 35.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_MAX_ACCEPTED_STEP_C: f32 = 6.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_MAX_UNPOWERED_RISE_C_PER_S: f32 = 4.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_MAX_UNPOWERED_RISE_STEP_C: f32 = 3.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_GUARD_RECOVERY_WINDOW_MS: u64 = 750;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_CONTROL_GUARD_RECOVERY_BAND_C: f32 = 3.0;
#[cfg(target_arch = "xtensa")]
pub(crate) const RTD_LOG_INTERVAL_MS: u64 = 1_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PT1000_R0_OHMS: f32 = 1_000.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PT1000_A: f32 = 3.9083e-3;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PT1000_B: f32 = -5.775e-7;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PT1000_C: f32 = -4.183e-12;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_REFERENCE_RESISTOR_OHMS: f32 = 2_490.0;
#[cfg(any(target_arch = "xtensa", test))]
// R17 = 31.6 kOhm and R16 = 10 kOhm set the TPS62933 3V3 rail to
// 0.8 V * (1 + 31.6 / 10) = 3.328 V nominal. This is the divider's circuit
// model, not a measured rail value or an ADC calibration parameter.
pub(crate) const RTD_DIVIDER_SUPPLY_MV: u16 = 3_328;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_SHORT_FAULT_MAX_MV: u16 = 150;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_OPEN_FAULT_MIN_MV: u16 = 2_800;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_TEMP_MIN_C: f32 = -50.0;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const RTD_TEMP_MAX_C: f32 = 500.0;
#[cfg(target_arch = "xtensa")]
pub(crate) const FUSB302B_I2C_FREQUENCY_HZ: u32 = 400_000;
#[cfg(target_arch = "xtensa")]
// FUSB302B and M24C64 both support fast-mode I2C. Keep each shared-bus
// transaction at the PD service interval so a stalled peripheral cannot
// monopolize the protocol service for the old 25ms timeout.
pub(crate) const I2C_TRANSACTION_TIMEOUT_MS: u64 = 5;
#[cfg(target_arch = "xtensa")]
pub(crate) const EEPROM_WRITE_CYCLE_DELAY_MS: u64 = 5;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const EEPROM_WRITE_CHUNK_MAX_BYTES: usize = 16;
#[cfg(target_arch = "xtensa")]
pub(crate) const EEPROM_READ_CHUNK_MAX_BYTES: usize = 16;
#[cfg(target_arch = "xtensa")]
pub(crate) const EEPROM_RECORD_STAGING_BYTES: usize = 1_024;
#[cfg(test)]
pub(crate) const EEPROM_UNUSED_GAP_LEN: usize = 0x0400;

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) const EEPROM_SNAPSHOT_SIZE: u16 = M24C64_CAPACITY_BYTES;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) const EEPROM_SNAPSHOT_CHUNK_MAX: u16 = 32;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) const EEPROM_SNAPSHOT_HASH_LEN: usize = 71;
#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
pub(crate) const EEPROM_SNAPSHOT_TIMEOUT_MS: u64 = 30_000;

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EepromSnapshotRequest {
    pub(crate) op: heapless::String<32>,
    pub(crate) request_id:
        heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    #[serde(default)]
    pub(crate) session_id:
        Option<heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>>,
    #[serde(default)]
    pub(crate) offset: Option<u16>,
    #[serde(default)]
    pub(crate) length: Option<u16>,
    #[serde(default)]
    pub(crate) sha256: Option<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>>,
}

#[cfg(all(target_arch = "xtensa", feature = "web_serial"))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EepromSnapshotResponse {
    pub(crate) ok: bool,
    pub(crate) request_id:
        heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_id:
        Option<heapless::String<{ flux_purr_firmware::control_plane::REQUEST_ID_MAX_LEN }>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) capacity: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chunk_max: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offset: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<heapless::Vec<u8, 32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sha256: Option<heapless::String<EEPROM_SNAPSHOT_HASH_LEN>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error:
        Option<heapless::String<{ flux_purr_firmware::control_plane::ERROR_CODE_MAX_LEN }>>,
}
