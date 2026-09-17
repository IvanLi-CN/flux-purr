#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) type RuntimeDisplayBus =
    ExclusiveDevice<Spi<'static, esp_hal::Async>, Output<'static>, embedded_hal_bus::spi::NoDelay>;
#[cfg(target_arch = "xtensa")]
pub(crate) type RuntimeDisplay =
    GC9D01<'static, RuntimeDisplayBus, Output<'static>, Output<'static>, DisplayTimer>;

#[cfg(target_arch = "xtensa")]
pub(crate) type RuntimePwm0 = PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 0, true>;

#[cfg(target_arch = "xtensa")]
pub(crate) type RuntimePwm1 = HeaterPwmGate;

#[cfg(target_arch = "xtensa")]
pub(crate) type RuntimeMcpwm = McPwm<'static, esp_hal::peripherals::MCPWM0<'static>>;

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeTransportState {
    #[cfg(feature = "web_serial")]
    pub(crate) usb_serial: RawUsbSerialJtag,
    #[cfg(feature = "web_serial")]
    pub(crate) usb_rx_line: &'static mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    #[cfg(feature = "web_serial")]
    pub(crate) usb_tx_buf: &'static mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    #[cfg(feature = "web_serial")]
    pub(crate) eeprom_snapshot_session: EepromSnapshotSession,
    #[cfg(not(feature = "web_serial"))]
    pub(crate) persistence_log_sink: NoopPersistenceLogSink,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct RuntimeLoopState {
    pub(crate) runtime_mode: FrontPanelRuntimeMode,
    pub(crate) display: RuntimeDisplay,
    pub(crate) canvas: &'static mut DisplayCanvas,
    pub(crate) inputs: FrontPanelInputs<'static>,
    pub(crate) controller: FrontPanelInputController,
    pub(crate) eeprom_i2c: I2c<'static>,
    pub(crate) pd_port: PdPort,
    pub(crate) fan_enable: Output<'static>,
    pub(crate) fan_pwm: RuntimePwm0,
    pub(crate) heater_pwm: RuntimePwm1,
    pub(crate) adc1: Adc1Driver,
    pub(crate) vin_adc_pin: VinAdcPin,
    pub(crate) rtd_adc_pin: RtdAdcPin,
    pub(crate) adc_curve: Option<Adc1Curve>,
    pub(crate) transport: RuntimeTransportState,
    pub(crate) eeprom_record_staging: &'static mut [u8; EEPROM_RECORD_STAGING_BYTES],
    pub(crate) memory_config: MemoryConfig,
    pub(crate) last_persisted_memory_config: MemoryConfig,
    pub(crate) memory_commit_due_ms: Option<u64>,
    pub(crate) memory_sequence: u32,
    pub(crate) eeprom_required: bool,
    pub(crate) eeprom_data_incompatible: bool,
    pub(crate) prepared_layout_recovery_pending: bool,
    pub(crate) persistence_source: &'static str,
    pub(crate) persistence_record_state: &'static str,
    pub(crate) preview_heater_curve: Option<HeaterCurvePreview>,
    pub(crate) pd_contract_ready: bool,
    pub(crate) manual_pps_state: ManualPpsState,
    pub(crate) calibration_runtime_state: CalibrationRuntimeState,
    pub(crate) thermal_plant_workspace: &'static mut CalibrationThermalPlantWorkspace,
    pub(crate) thermal_control_profile_preview: Option<ThermalControlProfile>,
    pub(crate) heater_power_backend: HeaterPowerBackend,
    pub(crate) hold_pps_governor: HoldPpsGovernor,
    pub(crate) heater_controller: HeaterController,
    pub(crate) ui_state: FrontPanelUiState,
    pub(crate) current_rtd_fault: Option<HeaterFaultReason>,
    pub(crate) latest_temp_c: f32,
    pub(crate) latest_temp_i16: i16,
    pub(crate) latest_display_temp_c: f32,
    pub(crate) latest_display_temp_i16: i16,
    pub(crate) latest_rtd_raw_adc_mv: u16,
    pub(crate) latest_rtd_raw_adc_min_mv: u16,
    pub(crate) latest_rtd_raw_adc_max_mv: u16,
    pub(crate) latest_vin_raw_adc_mv: u16,
    pub(crate) latest_vin_mv: u32,
    pub(crate) pd_contract_vin_guard: PdContractVinGuard,
    pub(crate) rtd_pps_transition_guard: RtdPpsTransitionGuard,
    pub(crate) rtd_control_measurement_guard: RtdControlMeasurementGuard,
    pub(crate) control_measurement_guarded: bool,
    pub(crate) last_rtd_sample_request_mv: u16,
    pub(crate) last_pd_observation: Option<PdStatusObservation>,
    pub(crate) last_pd_status_log_key: Option<PdStatusLogKey>,
    pub(crate) last_fusb302b_power_capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    pub(crate) active_thermal_settings: ThermalControlProfileSettings,
    pub(crate) last_pid_snapshot: HeaterPidSnapshot,
    pub(crate) last_heater_duty: u8,
    pub(crate) cooling_disabled_lock_latched: bool,
    pub(crate) cooling_disabled_lock_armed: bool,
    pub(crate) fan_policy_state: FanPolicyState,
    pub(crate) heater_enabled_last_cycle: bool,
    pub(crate) last_fan_command: Option<FanHardwareCommand>,
    pub(crate) last_raw_state: FrontPanelRawState,
    pub(crate) fan_command: FanHardwareCommand,
    pub(crate) buzzer: BuzzerRuntime,
    pub(crate) last_fault_present: bool,
    pub(crate) overtemp_attention_acknowledged: bool,
    pub(crate) attention_pending_after_fault_clear: bool,
    pub(crate) overtemp_forced_fan_active: bool,
    pub(crate) suppress_attention_ack_input: bool,
    pub(crate) suppress_attention_ack_waits_for_event: bool,
    pub(crate) suppress_attention_ack_event_seen: bool,
    pub(crate) suppress_attention_ack_clear_delay_ms: u64,
    pub(crate) suppress_attention_ack_clear_after_ms: Option<u64>,
    pub(crate) protection_alarm: ProtectionAlarmCadence,
    pub(crate) next_attention_reminder_ms: Option<u64>,
    pub(crate) status_light_started_ms: u64,
    pub(crate) runtime_started_ms: u64,
    pub(crate) last_control_ms: u64,
    pub(crate) next_control_deadline_ms: u64,
    pub(crate) heater_control_timing: HeaterControlTiming,
    pub(crate) ui_refresh_pending: bool,
    pub(crate) next_ui_refresh_ms: u64,
    pub(crate) suppress_pairing_input_until_released: bool,
}

#[cfg(target_arch = "xtensa")]
macro_rules! assemble_runtime_loop {
    ($($field:ident),+ $(,)?) => {
        RuntimeLoopState { $($field),+ }
    };
}

#[path = "runtime_assembly.rs"]
pub(crate) mod runtime_assembly;

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootSystemTokens {
    gpio13: esp_hal::peripherals::GPIO13<'static>,
    software_interrupt: esp_hal::peripherals::SW_INTERRUPT<'static>,
    status_light_red: esp_hal::peripherals::GPIO39<'static>,
    status_light_green: esp_hal::peripherals::GPIO38<'static>,
    status_light_blue: esp_hal::peripherals::GPIO37<'static>,
    #[cfg(feature = "web_serial")]
    usb_device: esp_hal::peripherals::USB_DEVICE<'static>,
    i2c0: esp_hal::peripherals::I2C0<'static>,
    pd_sda: esp_hal::peripherals::GPIO8<'static>,
    pd_scl: esp_hal::peripherals::GPIO9<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootDeviceTokens {
    spi2: esp_hal::peripherals::SPI2<'static>,
    display_sck: esp_hal::peripherals::GPIO12<'static>,
    display_mosi: esp_hal::peripherals::GPIO11<'static>,
    display_cs: esp_hal::peripherals::GPIO15<'static>,
    display_dc: esp_hal::peripherals::GPIO10<'static>,
    display_rst: esp_hal::peripherals::GPIO14<'static>,
    center: esp_hal::peripherals::GPIO0<'static>,
    right: esp_hal::peripherals::GPIO16<'static>,
    down: esp_hal::peripherals::GPIO17<'static>,
    left: esp_hal::peripherals::GPIO18<'static>,
    up: esp_hal::peripherals::GPIO21<'static>,
    fan_enable: esp_hal::peripherals::GPIO35<'static>,
    fan_pwm: esp_hal::peripherals::GPIO36<'static>,
    heater_pwm: esp_hal::peripherals::GPIO47<'static>,
    buzzer: esp_hal::peripherals::GPIO48<'static>,
    mcpwm0: esp_hal::peripherals::MCPWM0<'static>,
    #[cfg(feature = "buzzer-observe")]
    pcnt: esp_hal::peripherals::PCNT<'static>,
    adc1: esp_hal::peripherals::ADC1<'static>,
    vin_adc: esp_hal::peripherals::GPIO1<'static>,
    rtd_adc: esp_hal::peripherals::GPIO2<'static>,
    #[cfg(feature = "net_http")]
    wifi: esp_hal::peripherals::WIFI<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootSystem {
    reset_reason: &'static str,
    startup_sequence: StartupSequence,
    runtime_mode: FrontPanelRuntimeMode,
    status_light_started_ms: u64,
    pd_runtime_started_ms: u64,
    pd_contract_ready: bool,
    buzzer_realtime_spawner: embassy_executor::SendSpawner,
    eeprom_i2c: I2c<'static>,
    pd_task_i2c: Option<PdI2c<'static>>,
    pd_port: PdPort,
    initial_pd_observation: Option<PdStatusObservation>,
    eeprom_record_staging: &'static mut [u8; EEPROM_RECORD_STAGING_BYTES],
    #[cfg(feature = "web_serial")]
    usb_serial: RawUsbSerialJtag,
    #[cfg(feature = "web_serial")]
    usb_rx_line: &'static mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    #[cfg(feature = "web_serial")]
    usb_tx_buf: &'static mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    #[cfg(feature = "web_serial")]
    usb_boot_memory_config: MemoryConfig,
    #[cfg(not(feature = "web_serial"))]
    persistence_log_sink: NoopPersistenceLogSink,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootOutputTokens {
    fan_enable: esp_hal::peripherals::GPIO35<'static>,
    fan_pwm: esp_hal::peripherals::GPIO36<'static>,
    heater_pwm: esp_hal::peripherals::GPIO47<'static>,
    buzzer: esp_hal::peripherals::GPIO48<'static>,
    mcpwm0: esp_hal::peripherals::MCPWM0<'static>,
    #[cfg(feature = "buzzer-observe")]
    pcnt: esp_hal::peripherals::PCNT<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootAdcTokens {
    adc1: esp_hal::peripherals::ADC1<'static>,
    vin_adc: esp_hal::peripherals::GPIO1<'static>,
    rtd_adc: esp_hal::peripherals::GPIO2<'static>,
    #[cfg(feature = "net_http")]
    wifi: esp_hal::peripherals::WIFI<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootOutputState {
    fan_enable: Output<'static>,
    fan_pwm: RuntimePwm0,
    heater_pwm: RuntimePwm1,
    last_heater_duty: u8,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootDisplay {
    system: Box<BootSystem>,
    tokens: BootAdcTokens,
    output_tokens: Option<BootOutputTokens>,
    output_state: Option<BootOutputState>,
    boot_memory_io_scratch: Option<MemoryIoScratch>,
    display: RuntimeDisplay,
    canvas: &'static mut DisplayCanvas,
    inputs: FrontPanelInputs<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootOutputContext<'a> {
    startup_sequence: &'a mut StartupSequence,
    fan_enable_pin: esp_hal::peripherals::GPIO35<'static>,
    fan_pwm_pin: esp_hal::peripherals::GPIO36<'static>,
    heater_pwm_pin: esp_hal::peripherals::GPIO47<'static>,
    buzzer_pin: esp_hal::peripherals::GPIO48<'static>,
    mcpwm0: esp_hal::peripherals::MCPWM0<'static>,
    #[cfg(feature = "buzzer-observe")]
    pcnt: esp_hal::peripherals::PCNT<'static>,
    buzzer_realtime_spawner: embassy_executor::SendSpawner,
    #[cfg(feature = "web_serial")]
    usb_serial: &'a mut RawUsbSerialJtag,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootKeyTestContext<'a> {
    display: &'a mut RuntimeDisplay,
    canvas: &'a mut DisplayCanvas,
    inputs: FrontPanelInputs<'static>,
    status_light_started_ms: u64,
    pd_port: &'a PdPort,
    initial_pd_observation: &'a mut Option<PdStatusObservation>,
    #[cfg(feature = "web_serial")]
    usb_serial: &'a mut RawUsbSerialJtag,
    #[cfg(feature = "web_serial")]
    usb_rx_line: &'a mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    #[cfg(feature = "web_serial")]
    usb_tx_buf: &'a mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    #[cfg(feature = "web_serial")]
    usb_boot_memory_config: &'a MemoryConfig,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootMemoryReady {
    system: BootSystem,
    tokens: BootAdcTokens,
    display: RuntimeDisplay,
    canvas: &'static mut DisplayCanvas,
    inputs: FrontPanelInputs<'static>,
    fan_enable: Output<'static>,
    fan_pwm: RuntimePwm0,
    heater_pwm: RuntimePwm1,
    last_heater_duty: u8,
    boot_memory_io_scratch: Option<MemoryIoScratch>,
    memory: BootMemoryState,
}

/// Owns the linear boot resources after the allocator is available.
///
/// Keeping every full-stage value here means the boot coordinator only ever
/// passes pointers across async boundaries. In particular, it prevents a
/// `BootDisplay` or `BootMemoryReady` return value from being materialized in
/// the guarded ProCPU task stack while another stage is being polled.
#[cfg(target_arch = "xtensa")]
pub(crate) struct BootPipeline {
    system_tokens: Option<BootSystemTokens>,
    system: Option<Box<BootSystem>>,
    device_tokens: Option<BootDeviceTokens>,
    display: Option<Box<BootDisplay>>,
    memory_ready: Option<Box<BootMemoryReady>>,
}

#[cfg(target_arch = "xtensa")]
impl BootPipeline {
    fn new(system_tokens: BootSystemTokens, device_tokens: BootDeviceTokens) -> Self {
        Self {
            system_tokens: Some(system_tokens),
            system: None,
            device_tokens: Some(device_tokens),
            display: None,
            memory_ready: None,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootRuntimeState {
    system: BootSystem,
    tokens: Option<BootAdcTokens>,
    #[cfg(feature = "net_http")]
    wifi: Option<esp_hal::peripherals::WIFI<'static>>,
    display: RuntimeDisplay,
    canvas: &'static mut DisplayCanvas,
    inputs: FrontPanelInputs<'static>,
    fan_enable: Output<'static>,
    fan_pwm: RuntimePwm0,
    heater_pwm: RuntimePwm1,
    last_heater_duty: u8,
    boot_memory_io_scratch: Option<MemoryIoScratch>,
    #[cfg(feature = "web_serial")]
    eeprom_snapshot_session: EepromSnapshotSession,
    memory: BootMemoryState,
    adc1: Option<Adc1Driver>,
    vin_adc_pin: Option<VinAdcPin>,
    rtd_adc_pin: Option<RtdAdcPin>,
    adc_curve: Option<Adc1Curve>,
    controller: FrontPanelInputController,
    preview_heater_curve: Option<HeaterCurvePreview>,
    memory_commit_due_ms: Option<u64>,
    last_persisted_memory_config: MemoryConfig,
    last_pd_observation: Option<PdStatusObservation>,
    last_pd_status_log_key: Option<PdStatusLogKey>,
    pd_contract_vin_guard: PdContractVinGuard,
    active_thermal_settings: ThermalControlProfileSettings,
    manual_pps_state: ManualPpsState,
    last_fusb302b_power_capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    calibration_runtime_state: CalibrationRuntimeState,
    thermal_plant_workspace: &'static mut CalibrationThermalPlantWorkspace,
    thermal_control_profile_preview: Option<ThermalControlProfile>,
    heater_power_backend: HeaterPowerBackend,
    hold_pps_governor: HoldPpsGovernor,
    heater_controller: HeaterController,
    ui_state: FrontPanelUiState,
    current_rtd_fault: Option<HeaterFaultReason>,
    latest_temp_c: f32,
    latest_temp_i16: i16,
    latest_display_temp_c: f32,
    latest_display_temp_i16: i16,
    latest_rtd_raw_adc_mv: u16,
    latest_rtd_raw_adc_min_mv: u16,
    latest_rtd_raw_adc_max_mv: u16,
    latest_vin_raw_adc_mv: u16,
    latest_vin_mv: u32,
    rtd_pps_transition_guard: RtdPpsTransitionGuard,
    rtd_control_measurement_guard: RtdControlMeasurementGuard,
    control_measurement_guarded: bool,
    last_rtd_sample_request_mv: u16,
    last_pid_snapshot: HeaterPidSnapshot,
    cooling_disabled_lock_latched: bool,
    cooling_disabled_lock_armed: bool,
    fan_policy_state: FanPolicyState,
    heater_enabled_last_cycle: bool,
    last_fan_command: Option<FanHardwareCommand>,
    last_raw_state: FrontPanelRawState,
    fan_command: FanHardwareCommand,
    buzzer: BuzzerRuntime,
    last_fault_present: bool,
    overtemp_attention_acknowledged: bool,
    attention_pending_after_fault_clear: bool,
    overtemp_forced_fan_active: bool,
    suppress_attention_ack_input: bool,
    suppress_attention_ack_waits_for_event: bool,
    suppress_attention_ack_event_seen: bool,
    suppress_attention_ack_clear_delay_ms: u64,
    suppress_attention_ack_clear_after_ms: Option<u64>,
    protection_alarm: ProtectionAlarmCadence,
    next_attention_reminder_ms: Option<u64>,
    ui_refresh_pending: bool,
    next_ui_refresh_ms: u64,
    suppress_pairing_input_until_released: bool,
}

#[cfg(target_arch = "xtensa")]
static mut THERMAL_PLANT_WORKSPACE_STORAGE: MaybeUninit<CalibrationThermalPlantWorkspace> =
    MaybeUninit::uninit();

#[cfg(target_arch = "xtensa")]
fn new_thermal_plant_workspace() -> &'static mut CalibrationThermalPlantWorkspace {
    unsafe {
        initialize_after_software_reset(
            core::ptr::addr_of_mut!(THERMAL_PLANT_WORKSPACE_STORAGE),
            CalibrationThermalPlantWorkspace::default(),
        )
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootSystemRuntimeParts {
    reset_reason: &'static str,
    runtime_mode: FrontPanelRuntimeMode,
    status_light_started_ms: u64,
    pd_runtime_started_ms: u64,
    pd_contract_ready: bool,
    eeprom_i2c: I2c<'static>,
    pd_port: PdPort,
    eeprom_record_staging: &'static mut [u8; EEPROM_RECORD_STAGING_BYTES],
    #[cfg(feature = "web_serial")]
    usb_serial: RawUsbSerialJtag,
    #[cfg(feature = "web_serial")]
    usb_rx_line: &'static mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    #[cfg(feature = "web_serial")]
    usb_tx_buf: &'static mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    #[cfg(not(feature = "web_serial"))]
    persistence_log_sink: NoopPersistenceLogSink,
}

#[cfg(target_arch = "xtensa")]
impl From<BootSystem> for BootSystemRuntimeParts {
    fn from(system: BootSystem) -> Self {
        let BootSystem {
            reset_reason,
            runtime_mode,
            status_light_started_ms,
            pd_runtime_started_ms,
            pd_contract_ready,
            eeprom_i2c,
            pd_port,
            eeprom_record_staging,
            #[cfg(feature = "web_serial")]
            usb_serial,
            #[cfg(feature = "web_serial")]
            usb_rx_line,
            #[cfg(feature = "web_serial")]
            usb_tx_buf,
            #[cfg(not(feature = "web_serial"))]
            persistence_log_sink,
            ..
        } = system;
        Self {
            reset_reason,
            runtime_mode,
            status_light_started_ms,
            pd_runtime_started_ms,
            pd_contract_ready,
            eeprom_i2c,
            pd_port,
            eeprom_record_staging,
            #[cfg(feature = "web_serial")]
            usb_serial,
            #[cfg(feature = "web_serial")]
            usb_rx_line,
            #[cfg(feature = "web_serial")]
            usb_tx_buf,
            #[cfg(not(feature = "web_serial"))]
            persistence_log_sink,
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootMemoryRuntimeParts {
    memory_config: MemoryConfig,
    memory_sequence: u32,
    eeprom_required: bool,
    eeprom_data_incompatible: bool,
    prepared_layout_recovery_pending: bool,
    persistence_source: &'static str,
    persistence_record_state: &'static str,
}

#[cfg(target_arch = "xtensa")]
impl From<BootMemoryState> for BootMemoryRuntimeParts {
    fn from(memory: BootMemoryState) -> Self {
        let BootMemoryState {
            memory_config,
            memory_sequence,
            eeprom_required,
            eeprom_data_incompatible,
            prepared_layout_recovery_pending,
            persistence_source,
            persistence_record_state,
            ..
        } = memory;
        Self {
            memory_config,
            memory_sequence,
            eeprom_required,
            eeprom_data_incompatible,
            prepared_layout_recovery_pending,
            persistence_source,
            persistence_record_state,
        }
    }
}

#[cfg(target_arch = "xtensa")]
impl BootRuntimeState {
    fn from_ready(ready: Box<BootMemoryReady>, storage: Box<MaybeUninit<Self>>) -> Box<Self> {
        let runtime_mode = ready.system.runtime_mode;
        Box::write(
            storage,
            Self {
                system: ready.system,
                tokens: Some(ready.tokens),
                #[cfg(feature = "net_http")]
                wifi: None,
                display: ready.display,
                canvas: ready.canvas,
                inputs: ready.inputs,
                fan_enable: ready.fan_enable,
                fan_pwm: ready.fan_pwm,
                heater_pwm: ready.heater_pwm,
                last_heater_duty: ready.last_heater_duty,
                boot_memory_io_scratch: ready.boot_memory_io_scratch,
                #[cfg(feature = "web_serial")]
                eeprom_snapshot_session: EepromSnapshotSession::default(),
                last_persisted_memory_config: ready.memory.memory_config.clone(),
                memory: ready.memory,
                adc1: None,
                vin_adc_pin: None,
                rtd_adc_pin: None,
                adc_curve: None,
                controller: FrontPanelInputController::new(
                    FrontPanelKeyMap::default(),
                    FrontPanelInputTimings::default(),
                ),
                preview_heater_curve: None,
                memory_commit_due_ms: None,
                last_pd_observation: None,
                last_pd_status_log_key: None,
                pd_contract_vin_guard: PdContractVinGuard::default(),
                active_thermal_settings: ThermalControlProfileSettings::default(),
                manual_pps_state: ManualPpsState::default(),
                last_fusb302b_power_capabilities: None,
                calibration_runtime_state: CalibrationRuntimeState::default(),
                thermal_plant_workspace: new_thermal_plant_workspace(),
                thermal_control_profile_preview: None,
                heater_power_backend: HeaterPowerBackend::FixedPdPwmFallback {
                    reason: HeaterPowerBackendReason::CapabilityReadFailed,
                    fixed_request: DEFAULT_PD_VOLTAGE_REQUEST,
                    fixed_request_confirmed: false,
                    terminal_fixed_pd_disarmed: false,
                },
                hold_pps_governor: HoldPpsGovernor::new(),
                heater_controller: HeaterController::new(),
                ui_state: FrontPanelUiState::new_startup(runtime_mode),
                current_rtd_fault: None,
                latest_temp_c: 0.0,
                latest_temp_i16: 0,
                latest_display_temp_c: 0.0,
                latest_display_temp_i16: 0,
                latest_rtd_raw_adc_mv: 0,
                latest_rtd_raw_adc_min_mv: 0,
                latest_rtd_raw_adc_max_mv: 0,
                latest_vin_raw_adc_mv: 0,
                latest_vin_mv: 0,
                rtd_pps_transition_guard: RtdPpsTransitionGuard::new(0),
                rtd_control_measurement_guard: RtdControlMeasurementGuard::default(),
                control_measurement_guarded: false,
                last_rtd_sample_request_mv: 0,
                last_pid_snapshot: HeaterPidSnapshot {
                    duty_percent: 0,
                    warmup_soft_start_percent: 0,
                    error_c: 0.0,
                    control_error_c: 0.0,
                    filtered_temp_c: 0.0,
                    filtered_slope_c_per_s: 0.0,
                    coast_active: false,
                    phase: HeaterControlPhase::Warmup,
                },
                cooling_disabled_lock_latched: false,
                cooling_disabled_lock_armed: true,
                fan_policy_state: FanPolicyState::Disabled,
                heater_enabled_last_cycle: false,
                last_fan_command: None,
                last_raw_state: FrontPanelRawState::default(),
                fan_command: FanHardwareCommand::disabled(),
                buzzer: BuzzerRuntime,
                last_fault_present: false,
                overtemp_attention_acknowledged: false,
                attention_pending_after_fault_clear: false,
                overtemp_forced_fan_active: false,
                suppress_attention_ack_input: false,
                suppress_attention_ack_waits_for_event: false,
                suppress_attention_ack_event_seen: false,
                suppress_attention_ack_clear_delay_ms: FRONTPANEL_DEBOUNCE_MS,
                suppress_attention_ack_clear_after_ms: None,
                protection_alarm: ProtectionAlarmCadence::new(),
                next_attention_reminder_ms: None,
                ui_refresh_pending: false,
                next_ui_refresh_ms: DISPLAY_RUNTIME_MIN_REFRESH_INTERVAL_MS,
                suppress_pairing_input_until_released: false,
            },
        )
    }

    async fn initialize_power(&mut self) {
        #[cfg(feature = "web_serial")]
        usb_write_frame(
            &mut self.system.usb_serial,
            &hello_frame(hardware_identity()),
            self.system.usb_tx_buf,
        );
        #[cfg(feature = "web_serial")]
        poll_usb_early_control(
            &mut self.system.usb_serial,
            &mut *self.system.usb_rx_line,
            self.system.usb_tx_buf,
            &self.memory.memory_config,
        );
        self.last_pd_observation = self.system.initial_pd_observation;
        log_initial_pd_observation(self.last_pd_observation);
        self.active_thermal_settings = ThermalControlProfileSettings::from(
            self.memory
                .memory_config
                .active_thermal_control_profile
                .settings,
        );
        log_thermal_control_policy(self.active_thermal_settings);
        let capabilities = self.system.pd_port.capabilities();
        log_pd_power_capabilities(capabilities);
        self.manual_pps_state = ManualPpsState::from_fusb302b_capabilities(capabilities);
        self.last_fusb302b_power_capabilities = capabilities;
        self.heater_power_backend = select_boot_heater_backend(
            &self.system.pd_port,
            capabilities,
            self.last_pd_observation,
        );
        log_heater_backend(self.heater_power_backend);
        self.apply_safe_heater_output().await;
    }

    async fn apply_safe_heater_output(&mut self) {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=pre_adc_heater_sync_start\n",
        );
        let _ = apply_heater_power_output(HeaterPowerOutputContext {
            pd_port: &self.system.pd_port,
            heater_pwm: &mut self.heater_pwm,
            backend: &mut self.heater_power_backend,
            hold_pps_governor: &mut self.hold_pps_governor,
            manual_pps: &mut self.manual_pps_state,
            pd_observation: self.last_pd_observation,
            measured_heater_mv: 0,
            current_temp_c: 0.0,
            duty_percent: 0,
            heater_enabled: false,
            control_phase: HeaterControlPhase::Warmup,
            control_error_c: 0.0,
            filtered_slope_c_per_s: 0.0,
            warmup_soft_start_percent: 0,
            last_physical_duty_percent: &mut self.last_heater_duty,
            preview_heater_curve: preview_heater_curve_config(self.preview_heater_curve.as_ref()),
            memory_config: &self.memory.memory_config,
            active_thermal_settings: self.active_thermal_settings,
            now_ms: 0,
        })
        .await;
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=heater_safe_output_ready\n",
        );
    }

    async fn initialize_adc(&mut self) {
        let tokens = self
            .tokens
            .take()
            .expect("boot ADC tokens are available once");
        #[cfg(feature = "net_http")]
        let BootAdcTokens {
            adc1: adc1_peripheral,
            vin_adc: vin_adc_pin_peripheral,
            rtd_adc: rtd_adc_pin_peripheral,
            wifi,
        } = tokens;
        #[cfg(not(feature = "net_http"))]
        let BootAdcTokens {
            adc1: adc1_peripheral,
            vin_adc: vin_adc_pin_peripheral,
            rtd_adc: rtd_adc_pin_peripheral,
        } = tokens;
        #[cfg(feature = "net_http")]
        {
            self.wifi = Some(wifi);
        }
        #[cfg(feature = "web_serial")]
        let _ =
            usb_write_bytes_bounded(&mut self.system.usb_serial, b"boot_stage=adc_init_start\n");
        let (adc1, vin_adc_pin, rtd_adc_pin, adc_curve) = initialize_adc1(
            adc1_peripheral,
            vin_adc_pin_peripheral,
            rtd_adc_pin_peripheral,
        );
        self.adc1 = Some(adc1);
        self.vin_adc_pin = Some(vin_adc_pin);
        self.rtd_adc_pin = Some(rtd_adc_pin);
        self.adc_curve = adc_curve;
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=adc_init_complete\n",
        );
        info!(
            "adc monitor active: vin_gpio1 rtd_gpio2 atten={=str} samples={=u8} interval_ms={=u64}",
            "6dB", RTD_SAMPLE_COUNT as u8, RTD_LOG_INTERVAL_MS,
        );
        self.initialize_initial_rtd().await;
        self.initialize_initial_vin().await;
    }

    async fn initialize_initial_rtd(&mut self) {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=initial_rtd_start\n",
        );
        let mut adc1 = self.adc1.take().expect("ADC initialized before RTD sample");
        let mut rtd_adc_pin = self
            .rtd_adc_pin
            .take()
            .expect("RTD ADC initialized before sample");
        let sample = read_rtd_sample(
            &mut adc1,
            &mut rtd_adc_pin,
            self.adc_curve.as_ref(),
            &self.memory.memory_config,
        )
        .await;
        self.adc1 = Some(adc1);
        self.rtd_adc_pin = Some(rtd_adc_pin);
        self.apply_initial_rtd_sample(sample);
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=initial_rtd_complete\n",
        );
    }

    fn apply_initial_rtd_sample(&mut self, sample: RtdSample) {
        match sample {
            RtdSample::Valid(measurement) => {
                self.latest_rtd_raw_adc_mv = measurement.raw_adc_mv;
                self.latest_rtd_raw_adc_min_mv = measurement.raw_adc_min_mv;
                self.latest_rtd_raw_adc_max_mv = measurement.raw_adc_max_mv;
                self.latest_temp_c = measurement.temp_c;
                self.latest_temp_i16 = temp_c_to_whole_c(measurement.temp_c);
                self.rtd_control_measurement_guard
                    .reseed(measurement.temp_c, 0);
                let _ = update_runtime_display_temperature(
                    &mut self.ui_state,
                    &mut self.latest_display_temp_c,
                    &mut self.latest_display_temp_i16,
                    measurement.temp_c,
                );
                if let Some(reason) = overtemp_fault_from_control_temperature(self.latest_temp_c) {
                    self.current_rtd_fault = Some(reason);
                    let _ = self
                        .heater_controller
                        .latch_fault(HeaterFaultReason::OverTemp);
                    info!(
                        "heater initial fault latched reason={=str}",
                        HeaterFaultReason::OverTemp.label()
                    );
                }
                self.ui_state
                    .set_dashboard_presentation(if self.memory.eeprom_restore_pending {
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
                self.current_rtd_fault = Some(reason);
                self.memory.eeprom_restore_pending = false;
                self.ui_state.set_dashboard_presentation(
                    flux_purr_firmware::frontpanel::DashboardPresentationState::InitialRtdFault,
                );
                let _ = self.heater_controller.latch_fault(reason);
                let _ = retain_runtime_display_temperature(
                    &mut self.ui_state,
                    &mut self.latest_display_temp_c,
                    &mut self.latest_display_temp_i16,
                );
                info!(
                    "rtd initial fault adc_mv={=u16} reason={=str}",
                    adc_mv.unwrap_or(0),
                    reason.label(),
                );
            }
        }
    }

    async fn initialize_initial_vin(&mut self) {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=initial_vin_start\n",
        );
        let mut adc1 = self.adc1.take().expect("ADC initialized before VIN sample");
        let mut vin_adc_pin = self
            .vin_adc_pin
            .take()
            .expect("VIN ADC initialized before sample");
        if let Some((raw_code, raw_adc_mv, corrected_adc_mv, vin_mv)) = read_calibrated_vin_mv(
            &mut adc1,
            &mut vin_adc_pin,
            self.adc_curve.as_ref(),
            &self.memory.memory_config,
        )
        .await
        {
            self.latest_vin_raw_adc_mv = raw_adc_mv;
            self.latest_vin_mv = vin_mv;
            info!(
                "vin initial raw_code={=u16} raw_adc_mv={=u16} adc_mv={=u16} input_mv={=u32}",
                raw_code, raw_adc_mv, corrected_adc_mv, vin_mv,
            );
            let _ = reconcile_pd_contract_with_vin(
                &mut self.pd_contract_vin_guard,
                self.last_pd_observation,
                Some(vin_mv),
                PdTimestamp::now().as_millis(),
                PdContractVinContext {
                    pd_port: &self.system.pd_port,
                    last_pd_observation: &mut self.last_pd_observation,
                    pd_contract_ready: &mut self.system.pd_contract_ready,
                    ui_state: &mut self.ui_state,
                    calibration_runtime_state: &mut self.calibration_runtime_state,
                    manual_pps_state: &mut self.manual_pps_state,
                    heater_pwm: &mut self.heater_pwm,
                    last_heater_duty: &mut self.last_heater_duty,
                },
            );
        }
        self.adc1 = Some(adc1);
        self.vin_adc_pin = Some(vin_adc_pin);
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=initial_vin_complete\n",
        );
    }

    fn initialize_safety(&mut self) {
        self.initialize_ui_state();
        self.initialize_fan_state();
        self.initialize_fault_state();
    }

    fn initialize_ui_state(&mut self) {
        self.ui_state.eeprom_data_incompatible = self.memory.eeprom_data_incompatible;
        self.ui_state.eeprom_required = self.memory.eeprom_required;
        self.ui_state.persistence_fault_attention_pending =
            self.memory.eeprom_data_incompatible || self.memory.eeprom_required;
        if self.ui_state.persistence_locked() {
            self.ui_state.heater_lock_reason = Some(HeaterLockReason::PersistenceRequired);
        }
        self.ui_state.pd_contract_mv = effective_pd_contract_mv(
            &self.manual_pps_state,
            self.last_pd_observation,
            self.heater_power_backend,
        );
        apply_memory_config_to_ui(&mut self.ui_state, &self.memory.memory_config);
    }

    fn initialize_fan_state(&mut self) {
        self.ui_state.set_raw_state(self.last_raw_state);
        let initial_fan_decision = initial_boot_fan_decision(
            self.latest_display_temp_i16,
            self.ui_state.heater_enabled,
            self.ui_state.post_heat_cooling_mode,
            self.ui_state.heating_fan_guard_mode,
            self.current_rtd_fault,
        );
        self.fan_policy_state = initial_fan_decision.state;
        self.fan_command = initial_fan_decision.command;
        let persistence_locked = self.ui_state.persistence_locked();
        let _ = sync_frontpanel_runtime_state(
            &mut self.ui_state,
            initial_fan_decision,
            next_heater_lock_reason_with_persistence(
                persistence_locked,
                self.heater_controller.fault_latched(),
                self.cooling_disabled_lock_latched,
                thermal_model_heater_allowed(
                    &self.memory.memory_config,
                    self.calibration_runtime_state,
                    self.manual_pps_state,
                ),
                self.system.pd_contract_ready,
            ),
            0,
        );
        self.ui_state.pd_contract_mv = self.heater_power_backend.pd_contract_mv();
        apply_fan_output(
            &mut self.fan_enable,
            &mut self.fan_pwm,
            self.fan_command,
            &mut self.last_fan_command,
        );
    }

    fn initialize_fault_state(&mut self) {
        self.last_fault_present = is_overtemp_fault(self.current_rtd_fault);
        self.overtemp_forced_fan_active = self.last_fault_present;
        if self.last_fault_present {
            self.protection_alarm.arm(0);
            self.buzzer.activate_protection(BuzzerCueSource::Startup, 0);
        }
    }

    async fn present_initial_ui(&mut self) {
        let initial_status_light_state = select_status_light_state(StatusLightInputs {
            booting: Instant::now()
                .as_millis()
                .saturating_sub(self.system.status_light_started_ms)
                < STATUS_LIGHT_BOOT_DURATION_MS,
            thermal_runaway: is_overtemp_fault(self.current_rtd_fault),
            sensor_fault: is_sensor_fault(self.current_rtd_fault),
            heater_interlocked: matches!(
                self.ui_state.heater_lock_reason,
                Some(
                    HeaterLockReason::PdContractUnavailable
                        | HeaterLockReason::ThermalModelMissingForSourceClass
                )
            ),
            heater_enabled: self.ui_state.heater_enabled,
            fan_enabled: self.fan_command.enabled,
            ..StatusLightInputs::default()
        });
        set_status_light_state(initial_status_light_state);
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut self.system.usb_serial,
            b"boot_stage=display_runtime_presentation_start\n",
        );
        let ready = present_initial_frontpanel_ui(
            &mut self.display,
            self.canvas,
            InitialFrontpanelContext {
                state: &self.ui_state,
                pd_port: &self.system.pd_port,
                last_pd_observation: &mut self.last_pd_observation,
                heater_pwm: &mut self.heater_pwm,
                last_heater_duty: &mut self.last_heater_duty,
            },
        )
        .await;
        #[cfg(feature = "web_serial")]
        if ready {
            let _ = usb_write_bytes_bounded(
                &mut self.system.usb_serial,
                b"boot_stage=display_runtime_presentation_complete\n",
            );
        }
        if !ready {
            #[cfg(feature = "web_serial")]
            run_usb_recovery_control_loop(
                &mut self.system.usb_serial,
                &mut *self.system.usb_rx_line,
                self.system.usb_tx_buf,
                &self.memory.memory_config,
                initial_status_light_state,
                UsbRecoveryPhase::RuntimeFault,
            )
            .await;
            #[cfg(not(feature = "web_serial"))]
            panic!("failed to draw initial frontpanel UI");
        }
    }

    async fn restore_legacy_memory(&mut self) {
        let restore_frame_was_shown = self.memory.eeprom_restore_pending;
        if self.memory.eeprom_data_incompatible {
            self.restore_incompatible_memory().await;
        }
        if self.memory.eeprom_required && self.memory.eeprom_restore_pending {
            self.ui_state.set_dashboard_presentation(
                flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
            );
        }
        self.last_persisted_memory_config = self.memory.memory_config.clone();
        self.ui_refresh_pending = restore_frame_was_shown;
    }

    async fn restore_incompatible_memory(&mut self) {
        let Some(scratch) = self.boot_memory_io_scratch.as_mut() else {
            return;
        };
        let (legacy_record, read_failed) = load_legacy_eeprom_memory_record(
            &mut self.system.eeprom_i2c,
            scratch,
            self.system.eeprom_record_staging,
        )
        .await;
        if let Some(record) = legacy_record {
            self.memory.memory_sequence = record.sequence;
            self.memory.memory_config = record.config;
            self.migrate_legacy_memory(record.sequence).await;
        } else if read_failed {
            self.mark_legacy_memory_unavailable();
        }
    }

    async fn migrate_legacy_memory(&mut self, sequence: u32) {
        let migration_result = {
            #[cfg(feature = "web_serial")]
            let migration_log_sink = &mut self.system.usb_serial as &mut dyn PersistenceLogSink;
            #[cfg(not(feature = "web_serial"))]
            let migration_log_sink =
                &mut self.system.persistence_log_sink as &mut dyn PersistenceLogSink;
            migrate_legacy_memory_config(
                &mut self.system.eeprom_i2c,
                sequence,
                &self.memory.memory_config,
                &mut *migration_log_sink,
                self.system.eeprom_record_staging,
            )
            .await
        };
        match migration_result {
            Ok(sequence) => self.mark_legacy_memory_restored(sequence),
            Err(_) => self.mark_legacy_memory_failed(),
        }
    }

    fn mark_legacy_memory_restored(&mut self, sequence: u32) {
        self.memory.memory_sequence = sequence;
        self.memory.eeprom_required = false;
        self.ui_state.eeprom_data_incompatible = false;
        self.ui_state.eeprom_required = false;
        self.ui_state.persistence_fault_attention_pending = false;
        self.ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
        );
        apply_memory_config_to_ui(&mut self.ui_state, &self.memory.memory_config);
        self.active_thermal_settings = ThermalControlProfileSettings::from(
            self.memory
                .memory_config
                .active_thermal_control_profile
                .settings,
        );
        self.memory.persistence_source = "eeprom";
        self.memory.persistence_record_state = "valid";
        info!(
            "legacy memory restore and FPR2 migration complete seq={=u32}",
            sequence
        );
    }

    fn mark_legacy_memory_failed(&mut self) {
        self.memory.eeprom_required = true;
        self.ui_state.eeprom_required = true;
        self.memory.eeprom_restore_pending = false;
        self.ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
        );
        warn!("legacy memory migration failed; keeping heater interlocked");
    }

    fn mark_legacy_memory_unavailable(&mut self) {
        self.memory.eeprom_required = true;
        self.ui_state.eeprom_required = true;
        self.memory.eeprom_restore_pending = false;
        self.ui_state.set_dashboard_presentation(
            flux_purr_firmware::frontpanel::DashboardPresentationState::Ready,
        );
        self.memory.persistence_source = "none";
        self.memory.persistence_record_state = "unavailable";
        warn!("legacy memory restore unreadable; keeping heater interlocked");
    }

    async fn start_network(&mut self, spawner: &Spawner) {
        #[cfg(feature = "net_http")]
        {
            self.initialize_network_control_state().await;
            let result = self.spawn_network(spawner).await;
            if let Err(error) = result {
                warn!("LAN control plane startup failed: {=str}", error.message());
                self.report_network_startup_failure(error).await;
            }
        }
    }

    #[cfg(feature = "net_http")]
    async fn initialize_network_control_state(&mut self) {
        flux_purr_firmware::net::initialize_control_state(
            self.memory.memory_config.lan_pairing_token,
        )
        .await;
    }

    #[cfg(feature = "net_http")]
    async fn spawn_network(
        &mut self,
        spawner: &Spawner,
    ) -> Result<(), flux_purr_firmware::net::LanStartupError> {
        #[cfg(feature = "web_serial")]
        let usb_serial = &mut self.system.usb_serial;
        let wifi = self.wifi.take().expect("Wi-Fi token initialized with ADC");
        #[cfg(feature = "web_serial")]
        let result =
            flux_purr_firmware::net::spawn(spawner, wifi, &self.memory.memory_config, |stage| {
                let _ = usb_write_bytes_bounded(usb_serial, stage);
            })
            .await;
        #[cfg(not(feature = "web_serial"))]
        let result =
            flux_purr_firmware::net::spawn(spawner, wifi, &self.memory.memory_config, |_| {}).await;
        result
    }

    #[cfg(feature = "net_http")]
    async fn report_network_startup_failure(
        &mut self,
        error: flux_purr_firmware::net::LanStartupError,
    ) {
        flux_purr_firmware::net::report_startup_failure(error).await;
    }

    fn into_runtime_loop(
        self: Box<Self>,
        storage: Box<MaybeUninit<RuntimeLoopState>>,
    ) -> Box<RuntimeLoopState> {
        Box::write(storage, crate::build_runtime_loop_state!(*self))
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_initial_pd_observation(observation: Option<PdStatusObservation>) {
    if let Some(PdStatusObservation {
        status_raw,
        status,
        current_raw,
        current_ma,
        ..
    }) = observation
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
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_thermal_control_policy(settings: ThermalControlProfileSettings) {
    info!(
        "heater control policy mode=hybrid interval_ms={=u64} warmup_reenter={=f32}C hold_entry={=f32}C hold_exit={=f32}C approach_max_s={=u8} hold_kp={=f32} hold_ki={=f32} auto_floor_mv={=u16} current_reserve_ma={=u16}",
        HEATER_CONTROL_INTERVAL_MS,
        settings.warmup_reenter_error_c,
        settings.hold_entry_error_c,
        settings.hold_exit_error_c,
        settings.approach_max_ticks,
        settings.hold_kp_permille_per_c,
        settings.hold_ki_permille_per_c_tick,
        settings.auto_adjustable_working_floor_mv,
        settings.heater_current_reserve_ma,
    );
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_pd_power_capabilities(capabilities: Option<ch224q::AdjustablePowerCapabilities>) {
    match capabilities {
        Some(capabilities) => info!(
            "pd power data pps20={=bool} pps_min_mv={=u16} pps_max_mv={=u16} pps_max_ma={=u16}",
            capabilities.pps_covers_20v,
            capabilities.pps_min_mv.unwrap_or(0),
            capabilities.pps_max_mv.unwrap_or(0),
            capabilities.pps_max_ma.unwrap_or(0),
        ),
        None => info!("pd power data read failed"),
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn select_boot_heater_backend(
    pd_port: &PdPort,
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    observation: Option<PdStatusObservation>,
) -> HeaterPowerBackend {
    match pd_port.controller_kind() {
        ControllerKind::Fusb302b => select_fusb302b_heater_power_backend(capabilities),
        controller => constrain_heater_backend_to_controller(
            controller,
            select_heater_power_backend(capabilities, observation.map(|status| status.status)),
        ),
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn log_heater_backend(backend: HeaterPowerBackend) {
    match backend {
        HeaterPowerBackend::PpsMos {
            pps_min_mv,
            pps_max_mv,
            adjustable_max_mv,
            ..
        } => info!(
            "heater backend selected mode={=str} reason={=str} pps_min_mv={=u16} idle_mv={=u16} pps_max_mv={=u16} adjustable_max_mv={=u16} gate_mv={=u16}",
            backend.label(),
            HeaterPowerBackendReason::PpsCovers20v.label(),
            pps_min_mv,
            backend.pd_contract_mv(),
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
            backend.label(),
            reason.label(),
            fixed_request.millivolts(),
        ),
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initial_boot_fan_decision(
    display_temp_i16: i16,
    heater_enabled: bool,
    post_heat_cooling_mode: PostHeatCoolingMode,
    heating_fan_guard_mode: HeatingFanGuardMode,
    current_rtd_fault: Option<HeaterFaultReason>,
) -> FanPolicyDecision {
    let mut decision = fan_policy_decision_with_modes(
        display_temp_i16,
        0,
        heater_enabled,
        false,
        post_heat_cooling_mode,
        heating_fan_guard_mode,
        (FanPolicyState::Disabled, is_sensor_fault(current_rtd_fault)),
    );
    if let Some(state) =
        overtemp_forced_fan_state(display_temp_i16, is_overtemp_fault(current_rtd_fault))
    {
        let command = state.command(0);
        decision = FanPolicyDecision {
            state,
            command,
            display_state: fan_display_state_for_policy(
                FanPolicySource::Safety,
                post_heat_cooling_mode,
                command,
            ),
            source: FanPolicySource::Safety,
            output_level: fan_output_level_for_command(command),
        };
    }
    decision
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn split_boot_tokens(
    peripherals: esp_hal::peripherals::Peripherals,
) -> (
    esp_hal::peripherals::TIMG0<'static>,
    BootSystemTokens,
    BootDeviceTokens,
) {
    (
        peripherals.TIMG0,
        BootSystemTokens {
            gpio13: peripherals.GPIO13,
            software_interrupt: peripherals.SW_INTERRUPT,
            status_light_red: peripherals.GPIO39,
            status_light_green: peripherals.GPIO38,
            status_light_blue: peripherals.GPIO37,
            #[cfg(feature = "web_serial")]
            usb_device: peripherals.USB_DEVICE,
            i2c0: peripherals.I2C0,
            pd_sda: peripherals.GPIO8,
            pd_scl: peripherals.GPIO9,
        },
        BootDeviceTokens {
            spi2: peripherals.SPI2,
            display_sck: peripherals.GPIO12,
            display_mosi: peripherals.GPIO11,
            display_cs: peripherals.GPIO15,
            display_dc: peripherals.GPIO10,
            display_rst: peripherals.GPIO14,
            center: peripherals.GPIO0,
            right: peripherals.GPIO16,
            down: peripherals.GPIO17,
            left: peripherals.GPIO18,
            up: peripherals.GPIO21,
            fan_enable: peripherals.GPIO35,
            fan_pwm: peripherals.GPIO36,
            heater_pwm: peripherals.GPIO47,
            buzzer: peripherals.GPIO48,
            mcpwm0: peripherals.MCPWM0,
            #[cfg(feature = "buzzer-observe")]
            pcnt: peripherals.PCNT,
            adc1: peripherals.ADC1,
            vin_adc: peripherals.GPIO1,
            rtd_adc: peripherals.GPIO2,
            #[cfg(feature = "net_http")]
            wifi: peripherals.WIFI,
        },
    )
}

#[cfg(target_arch = "xtensa")]
fn spawn_status_light_task(
    spawner: Spawner,
    red_pin: esp_hal::peripherals::GPIO39<'static>,
    green_pin: esp_hal::peripherals::GPIO38<'static>,
    blue_pin: esp_hal::peripherals::GPIO37<'static>,
) -> u64 {
    let started_ms = Instant::now().as_millis();
    let red = Output::new(red_pin, Level::High, OutputConfig::default());
    let green = Output::new(green_pin, Level::High, OutputConfig::default());
    let blue = Output::new(blue_pin, Level::High, OutputConfig::default());
    spawner
        .spawn(run_status_light_task(red, green, blue, started_ms))
        .expect("failed to spawn status-light task");
    started_ms
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) fn initialize_boot_system(
    spawner: Spawner,
    tokens: BootSystemTokens,
    storage: Box<MaybeUninit<BootSystem>>,
) -> Box<BootSystem> {
    rom_boot_stage(b"system_enter");
    let reset_reason = reset_reason_log_line(esp_hal::system::reset_reason());
    let mut startup_sequence = StartupSequence::new();
    let mut backlight = Output::new(tokens.gpio13, Level::Low, OutputConfig::default());
    backlight.set_low();
    assert!(startup_sequence.advance(StartupSequenceStage::BacklightReady));
    let eeprom_record_staging = initialize_eeprom_record_staging();
    let software_interrupts = SoftwareInterruptControl::new(tokens.software_interrupt);
    let buzzer_realtime_executor = unsafe {
        initialize_after_software_reset(
            core::ptr::addr_of_mut!(BUZZER_REALTIME_EXECUTOR_STORAGE),
            InterruptExecutor::new(software_interrupts.software_interrupt1),
        )
    };
    let buzzer_realtime_spawner = buzzer_realtime_executor.start(Priority::Priority2);
    let status_light_started_ms = spawn_status_light_task(
        spawner,
        tokens.status_light_red,
        tokens.status_light_green,
        tokens.status_light_blue,
    );
    let runtime_mode = FrontPanelRuntimeMode::compile_time_default();
    #[cfg(feature = "web_serial")]
    let mut usb_serial = RawUsbSerialJtag::new(tokens.usb_device);
    #[cfg(feature = "web_serial")]
    let usb_rx_line = initialize_usb_control_rx_line();
    #[cfg(feature = "web_serial")]
    let usb_tx_buf = initialize_usb_control_response_buffer();
    #[cfg(feature = "web_serial")]
    let usb_boot_memory_config = MemoryConfig::default();
    rom_boot_stage(b"usb_ready");
    #[cfg(not(feature = "web_serial"))]
    let persistence_log_sink = NoopPersistenceLogSink;
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=backlight_on\n");
    #[cfg(feature = "web_serial")]
    usb_write_frame(
        &mut usb_serial,
        &hello_frame(hardware_identity()),
        usb_tx_buf,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, reset_reason.as_bytes());
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut usb_serial, b"boot_stage=pd_detect_start\n");
    let raw_i2c = HalI2c::new(
        tokens.i2c0,
        I2cConfig::default()
            .with_frequency(Rate::from_hz(FUSB302B_I2C_FREQUENCY_HZ))
            .with_software_timeout(SoftwareTimeout::Transaction(HalDuration::from_millis(
                I2C_TRANSACTION_TIMEOUT_MS,
            ))),
    )
    .expect("failed to create I2C0")
    .with_sda(tokens.pd_sda)
    .with_scl(tokens.pd_scl);
    let i2c_bus = unsafe {
        initialize_after_software_reset(
            core::ptr::addr_of_mut!(I2C_BUS_STORAGE),
            AsyncMutex::new(BlockingAsync::new(raw_i2c)),
        )
    };
    let eeprom_i2c = SharedI2cDevice::new(i2c_bus);
    let pd_task_i2c = PdI2c::new(i2c_bus);
    rom_boot_stage(b"i2c_ready");
    Box::write(
        storage,
        BootSystem {
            reset_reason,
            startup_sequence,
            runtime_mode,
            status_light_started_ms,
            pd_runtime_started_ms: 0,
            pd_contract_ready: false,
            buzzer_realtime_spawner,
            eeprom_i2c,
            pd_task_i2c: Some(pd_task_i2c),
            pd_port: PdServiceClient::new(),
            initial_pd_observation: None,
            eeprom_record_staging,
            #[cfg(feature = "web_serial")]
            usb_serial,
            #[cfg(feature = "web_serial")]
            usb_rx_line,
            #[cfg(feature = "web_serial")]
            usb_tx_buf,
            #[cfg(feature = "web_serial")]
            usb_boot_memory_config,
            #[cfg(not(feature = "web_serial"))]
            persistence_log_sink,
        },
    )
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_pd(spawner: Spawner, system: &mut BootSystem) {
    rom_boot_stage(b"pd_stage_enter");
    let mut pd_task_i2c = system
        .pd_task_i2c
        .take()
        .expect("PD task I2C device is available during boot");
    rom_boot_stage(b"pd_i2c_taken");
    assert!(
        pd_task_i2c.try_acquire(),
        "PD bus must be available during boot initialization"
    );
    rom_boot_stage(b"pd_i2c_locked");
    let detected_pd_controller = detect_pd_controller(&mut pd_task_i2c).await;
    rom_boot_stage(b"pd_detect_done");
    system.pd_port = match detected_pd_controller {
        DetectedPdController::Fusb302b(device_id) => {
            #[cfg(feature = "web_serial")]
            let _ = usb_write_bytes_bounded(
                &mut system.usb_serial,
                b"boot_stage=pd_fusb302b_detected\n",
            );
            let mut runtime = Fusb302bRuntime::new();
            rom_boot_stage(b"pd_phy_init_enter");
            if !runtime.initialize(&mut pd_task_i2c).await {
                pd_task_i2c.release();
                #[cfg(feature = "web_serial")]
                let _ = usb_write_bytes_bounded(
                    &mut system.usb_serial,
                    b"boot_stage=pd_phy_init_failed\n",
                );
                warn!(
                    "fusb302b identified device_id=0x{=u8:02x} but PHY initialization failed; holding heater interlocked",
                    device_id,
                );
                PdServiceClient::mark_unavailable()
            } else {
                rom_boot_stage(b"pd_phy_init_done");
                #[cfg(feature = "web_serial")]
                let _ = usb_write_bytes_bounded(
                    &mut system.usb_serial,
                    b"boot_stage=pd_phy_init_complete\n",
                );
                info!(
                    "fusb302b selected device_id=0x{=u8:02x} policy=pps target_mv={=u16} max_current_ma={=u16}",
                    device_id, FUSB302B_INITIAL_PPS_REQUEST_MV, MAX_HEATER_CONTRACT_MA,
                );
                let client = PdServiceClient::mark_starting_fusb302b();
                spawn_pd_service(spawner, pd_task_i2c, Box::new(runtime));
                rom_boot_stage(b"pd_service_spawned");
                client
            }
        }
        DetectedPdController::Unknown => {
            pd_task_i2c.release();
            #[cfg(feature = "web_serial")]
            let _ = usb_write_bytes_bounded(
                &mut system.usb_serial,
                b"boot_stage=pd_identity_unknown\n",
            );
            warn!("PD controller identity is ambiguous or unreadable; holding heater interlocked");
            PdServiceClient::mark_unavailable()
        }
    };
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(&mut system.usb_serial, b"boot_stage=pd_contract_pending\n");
    let pd_runtime_started_ms = Instant::now().as_millis();
    let fusb302b_present = system.pd_port.controller_kind() == ControllerKind::Fusb302b;
    let initial_pd_observation =
        wait_for_initial_pd_contract(system, pd_runtime_started_ms, fusb302b_present).await;
    let pd_contract_ready = startup_pd_contract_ready(initial_pd_observation);
    if !pd_contract_ready {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(
            &mut system.usb_serial,
            b"boot_stage=pd_contract_not_ready\n",
        );
        warn!(
            "PD contract was not ready before display initialization; continuing with heater interlocked"
        );
    } else {
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(&mut system.usb_serial, b"boot_stage=pd_contract_ready\n");
    }
    system.initial_pd_observation = initial_pd_observation;
    system.pd_runtime_started_ms = pd_runtime_started_ms;
    system.pd_contract_ready = pd_contract_ready;
    rom_boot_stage(b"pd_startup_window_done");
    assert!(
        system
            .startup_sequence
            .advance(StartupSequenceStage::PdServiceComplete)
    );
}

#[cfg(target_arch = "xtensa")]
async fn wait_for_initial_pd_contract(
    system: &BootSystem,
    pd_runtime_started_ms: u64,
    fusb302b_present: bool,
) -> Option<PdStatusObservation> {
    let mut observation = system.pd_port.observation();
    while startup_pd_service_should_continue(
        fusb302b_present,
        startup_pd_contract_ready(observation),
        system.pd_port.service_available(),
        pd_runtime_elapsed_ms(pd_runtime_started_ms, Instant::now().as_millis()),
    ) {
        EmbassyTimer::after_millis(PD_SERVICE_TICK_MS).await;
        observation = system.pd_port.observation();
    }
    observation
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootDisplayContext<'a> {
    runtime_mode: FrontPanelRuntimeMode,
    runtime: BootDisplayRuntimeContext<'a>,
    spi2: esp_hal::peripherals::SPI2<'static>,
    display_sck: esp_hal::peripherals::GPIO12<'static>,
    display_mosi: esp_hal::peripherals::GPIO11<'static>,
    display_cs: esp_hal::peripherals::GPIO15<'static>,
    display_dc: esp_hal::peripherals::GPIO10<'static>,
    display_rst: esp_hal::peripherals::GPIO14<'static>,
    center: esp_hal::peripherals::GPIO0<'static>,
    right: esp_hal::peripherals::GPIO16<'static>,
    down: esp_hal::peripherals::GPIO17<'static>,
    left: esp_hal::peripherals::GPIO18<'static>,
    up: esp_hal::peripherals::GPIO21<'static>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootDisplayRuntimeContext<'a> {
    startup_sequence: &'a mut StartupSequence,
    pd_port: &'a PdPort,
    initial_pd_observation: &'a mut Option<PdStatusObservation>,
    status_light: StatusLightState,
    #[cfg(feature = "web_serial")]
    usb_serial: &'a mut RawUsbSerialJtag,
    #[cfg(feature = "web_serial")]
    usb_rx_line: &'a mut heapless::String<USB_CONTROL_LINE_CAPACITY>,
    #[cfg(feature = "web_serial")]
    usb_tx_buf: &'a mut [u8; USB_CONTROL_TX_BUFFER_LEN],
    #[cfg(feature = "web_serial")]
    usb_boot_memory_config: &'a MemoryConfig,
}

#[cfg(target_arch = "xtensa")]
fn initialize_display_framebuffer()
-> &'static mut [embedded_graphics::pixelcolor::Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS]
{
    static mut DRIVER_FB_STORAGE: MaybeUninit<
        [embedded_graphics::pixelcolor::Rgb565; flux_purr_firmware::display::DISPLAY_PIXELS],
    > = MaybeUninit::uninit();

    // Rebuild retained storage in place. Passing this 16 KiB array by value
    // would materialize it on the guarded startup task stack.
    unsafe {
        let pixels = core::ptr::addr_of_mut!(DRIVER_FB_STORAGE)
            .cast::<embedded_graphics::pixelcolor::Rgb565>();
        for index in 0..flux_purr_firmware::display::DISPLAY_PIXELS {
            core::ptr::write(
                pixels.add(index),
                embedded_graphics::pixelcolor::Rgb565::BLACK,
            );
        }
        &mut *core::ptr::addr_of_mut!(DRIVER_FB_STORAGE).cast::<[
            embedded_graphics::pixelcolor::Rgb565;
            flux_purr_firmware::display::DISPLAY_PIXELS
        ]>()
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_display_driver(
    spi2: esp_hal::peripherals::SPI2<'static>,
    display_sck: esp_hal::peripherals::GPIO12<'static>,
    display_mosi: esp_hal::peripherals::GPIO11<'static>,
    display_cs: esp_hal::peripherals::GPIO15<'static>,
    display_dc: esp_hal::peripherals::GPIO10<'static>,
    display_rst: esp_hal::peripherals::GPIO14<'static>,
) -> (RuntimeDisplay, &'static mut DisplayCanvas) {
    let spi = Spi::new(
        spi2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(10_000_000))
            .with_mode(SpiMode::_0),
    )
    .expect("failed to create SPI2")
    .with_sck(display_sck)
    .with_mosi(display_mosi);
    let cs = Output::new(display_cs, Level::High, OutputConfig::default());
    let dc = Output::new(display_dc, Level::Low, OutputConfig::default());
    let rst = Output::new(display_rst, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new_no_delay(spi.into_async(), cs)
        .expect("failed to wrap async SPI bus as ExclusiveDevice");
    let driver_framebuffer = initialize_display_framebuffer();
    let canvas = initialize_display_canvas();
    let display = GC9D01::new(
        DISPLAY_PANEL_CONFIG,
        spi_device,
        dc,
        rst,
        driver_framebuffer,
    );
    (display, canvas)
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_display_panel(
    context: &mut BootDisplayRuntimeContext<'_>,
    display: &mut RuntimeDisplay,
) {
    rom_boot_stage(b"display_init_enter");
    info!(
        "init panel width={=u16} height={=u16} dx={=u16} dy={=u16}",
        DISPLAY_PANEL_CONFIG.width,
        DISPLAY_PANEL_CONFIG.height,
        DISPLAY_PANEL_CONFIG.dx,
        DISPLAY_PANEL_CONFIG.dy,
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_init_start\n");
    let result = run_display_operation_with_snapshot(
        display.init(),
        context.pd_port,
        context.initial_pd_observation,
    )
    .await;
    if matches!(result, Some(Ok(()))) {
        rom_boot_stage(b"display_init_done");
        assert!(
            context
                .startup_sequence
                .advance(StartupSequenceStage::DisplayReady)
        );
        #[cfg(feature = "web_serial")]
        let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_init_complete\n");
        return;
    }
    #[cfg(feature = "web_serial")]
    {
        rom_boot_stage(b"display_init_recovery");
        let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_init_failed\n");
        run_usb_recovery_control_loop(
            context.usb_serial,
            context.usb_rx_line,
            context.usb_tx_buf,
            context.usb_boot_memory_config,
            context.status_light,
            UsbRecoveryPhase::BeforePersistentState,
        )
        .await;
    }
    #[cfg(not(feature = "web_serial"))]
    panic!("failed to initialize GC9D01 display");
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn present_startup_display(
    context: &mut BootDisplayRuntimeContext<'_>,
    display: &mut RuntimeDisplay,
    canvas: &'static mut DisplayCanvas,
    runtime_mode: FrontPanelRuntimeMode,
) -> &'static mut DisplayCanvas {
    rom_boot_stage(b"display_flush_enter");
    match startup_frontpanel_presentation(runtime_mode) {
        StartupFrontPanelPresentation::Splash => render_scene(SceneId::StartupSplash, canvas),
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
    let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_flush_start\n");
    let ready = match run_display_operation_with_snapshot(
        display.flush(),
        context.pd_port,
        context.initial_pd_observation,
    )
    .await
    {
        Some(Ok(())) => true,
        Some(Err(gc9d01::Error::Bus(_))) => {
            warn!("startup display flush failed: spi bus");
            false
        }
        Some(Err(gc9d01::Error::Pin(_))) => {
            warn!("startup display flush failed: display pin");
            false
        }
        None => {
            warn!("startup display flush timed out");
            false
        }
    };
    if !ready {
        #[cfg(feature = "web_serial")]
        {
            rom_boot_stage(b"display_flush_recovery");
            let _ =
                usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_flush_failed\n");
            run_usb_recovery_control_loop(
                context.usb_serial,
                context.usb_rx_line,
                context.usb_tx_buf,
                context.usb_boot_memory_config,
                context.status_light,
                UsbRecoveryPhase::BeforePersistentState,
            )
            .await;
        }
        #[cfg(not(feature = "web_serial"))]
        panic!("failed to draw startup calibration screen");
    }
    assert!(
        context
            .startup_sequence
            .advance(StartupSequenceStage::StartupFrameReady)
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=display_flush_complete\n");
    rom_boot_stage(b"display_flush_done");
    canvas
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_frontpanel_inputs(
    center: esp_hal::peripherals::GPIO0<'static>,
    right: esp_hal::peripherals::GPIO16<'static>,
    down: esp_hal::peripherals::GPIO17<'static>,
    left: esp_hal::peripherals::GPIO18<'static>,
    up: esp_hal::peripherals::GPIO21<'static>,
) -> FrontPanelInputs<'static> {
    let input_cfg = InputConfig::default().with_pull(Pull::Up);
    FrontPanelInputs {
        center: Input::new(center, input_cfg),
        right: Input::new(right, input_cfg),
        down: Input::new(down, input_cfg),
        left: Input::new(left, input_cfg),
        up: Input::new(up, input_cfg),
    }
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_display(
    context: BootDisplayContext<'_>,
) -> (
    RuntimeDisplay,
    &'static mut DisplayCanvas,
    FrontPanelInputs<'static>,
) {
    rom_boot_stage(b"display_setup_enter");
    let BootDisplayContext {
        runtime_mode,
        mut runtime,
        spi2,
        display_sck,
        display_mosi,
        display_cs,
        display_dc,
        display_rst,
        center,
        right,
        down,
        left,
        up,
    } = context;
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(runtime.usb_serial, b"boot_stage=display_setup_start\n");
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
    let (mut display, canvas) = initialize_display_driver(
        spi2,
        display_sck,
        display_mosi,
        display_cs,
        display_dc,
        display_rst,
    );
    rom_boot_stage(b"display_driver_ready");
    initialize_display_panel(&mut runtime, &mut display).await;
    let canvas = present_startup_display(&mut runtime, &mut display, canvas, runtime_mode).await;
    info!("backlight active-low: gpio13 low -> on");
    let inputs = initialize_frontpanel_inputs(center, right, down, left, up);
    #[cfg(feature = "web_serial")]
    poll_usb_early_control(
        runtime.usb_serial,
        runtime.usb_rx_line,
        runtime.usb_tx_buf,
        runtime.usb_boot_memory_config,
    );
    info!(
        "frontpanel runtime mode={=str}",
        runtime_mode_label(runtime_mode)
    );
    (display, canvas, inputs)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BuzzerPwmParts {
    timer: esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'static>>,
    pwm: PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 2, true>,
    #[cfg(feature = "buzzer-observe")]
    edge_counter: Unit<'static, 0>,
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_buzzer_pwm(
    mut buzzer_timer: Timer<2, esp_hal::peripherals::MCPWM0<'static>>,
    mut buzzer_operator: Operator<'static, 2, esp_hal::peripherals::MCPWM0<'static>>,
    buzzer_pin: esp_hal::peripherals::GPIO48<'static>,
    pwm_clock_cfg: PeripheralClockConfig,
    #[cfg(feature = "buzzer-observe")] pcnt: esp_hal::peripherals::PCNT<'static>,
) -> BuzzerPwmParts {
    buzzer_operator.set_timer(&buzzer_timer);
    #[cfg(feature = "buzzer-observe")]
    let mut buzzer_pin = buzzer_pin.degrade();
    #[cfg(feature = "buzzer-observe")]
    let buzzer_edge_counter = {
        let pcnt = Pcnt::new(pcnt);
        let unit = pcnt.unit0;
        unit.channel0.set_edge_signal(buzzer_pin.reborrow());
        unit.channel0
            .set_input_mode(EdgeMode::Hold, EdgeMode::Increment);
        unit.clear();
        unit.resume();
        unit
    };
    let mut buzzer_pwm = buzzer_operator.with_pin_a(
        buzzer_pin,
        PwmPinConfig::new(PwmActions::UP_ACTIVE_HIGH, PwmUpdateMethod::SYNC_IMMEDIATLY),
    );
    let buzzer_timer_cfg = pwm_clock_cfg.timer_clock_with_prescaler(
        buzzer_timer_period_ticks(BUZZER_IDLE_FREQUENCY_HZ)
            .expect("idle buzzer frequency is outside the Timer2 period range"),
        PwmWorkingMode::Increase,
        BUZZER_TIMER_PRESCALER,
    );
    buzzer_timer.start(buzzer_timer_cfg);
    let _ = buzzer_pwm.set_duty_cycle_percent(0);
    BuzzerPwmParts {
        timer: buzzer_timer,
        pwm: buzzer_pwm,
        #[cfg(feature = "buzzer-observe")]
        edge_counter: buzzer_edge_counter,
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn spawn_buzzer_runtime(
    buzzer_timer: esp_hal::mcpwm::timer::Timer<2, esp_hal::peripherals::MCPWM0<'static>>,
    buzzer_pwm: PwmPin<'static, esp_hal::peripherals::MCPWM0<'static>, 2, true>,
    peripheral_clock: PeripheralClockConfig,
    buzzer_realtime_spawner: embassy_executor::SendSpawner,
    #[cfg(feature = "buzzer-observe")] buzzer_edge_counter: Unit<'static, 0>,
) {
    info!(
        "buzzer runtime armed: gpio48 default=silent fixed_prescaler={=u8}",
        BUZZER_TIMER_PRESCALER,
    );
    #[cfg(feature = "buzzer-observe")]
    buzzer_realtime_spawner
        .spawn(run_buzzer_task(
            buzzer_timer,
            buzzer_pwm,
            peripheral_clock,
            buzzer_edge_counter,
        ))
        .expect("failed to spawn realtime buzzer task");
    #[cfg(not(feature = "buzzer-observe"))]
    buzzer_realtime_spawner
        .spawn(run_buzzer_task(buzzer_timer, buzzer_pwm, peripheral_clock))
        .expect("failed to spawn realtime buzzer task");
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn initialize_boot_outputs(
    context: BootOutputContext<'_>,
) -> (Output<'static>, RuntimePwm0, RuntimePwm1, u8) {
    assert!(
        context
            .startup_sequence
            .advance(StartupSequenceStage::OtherInitialization)
    );
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(context.usb_serial, b"boot_stage=outputs_init_start\n");
    let mut fan_enable = Output::new(context.fan_enable_pin, Level::Low, OutputConfig::default());
    let pwm_clock_cfg =
        PeripheralClockConfig::with_frequency(Rate::from_hz(MCPWM_PERIPHERAL_CLOCK_HZ))
            .expect("failed to derive MCPWM peripheral clock");
    let mcpwm = McPwm::new(context.mcpwm0, pwm_clock_cfg);
    let RuntimeMcpwm {
        mut timer0,
        mut timer1,
        timer2,
        mut operator0,
        mut operator1,
        operator2,
        ..
    } = mcpwm;
    operator0.set_timer(&timer0);
    let mut fan_pwm = operator0.with_pin_a(context.fan_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let fan_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            FAN_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(FAN_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive fan PWM timer clock");
    timer0.start(fan_timer_cfg);
    let _ = fan_pwm.set_duty_cycle_percent(pwm_percent_from_permille(
        FAN_MINIMUM_OUTPUT_VOLTAGE_PWM_PERMILLE,
    ));
    operator1.set_timer(&timer1);
    let mut heater_pwm = operator1.with_pin_a(context.heater_pwm_pin, PwmPinConfig::UP_ACTIVE_HIGH);
    let heater_timer_cfg = pwm_clock_cfg
        .timer_clock_with_frequency(
            HEATER_PWM_PERIOD_TICKS,
            PwmWorkingMode::Increase,
            Rate::from_hz(HEATER_PWM_FREQUENCY_HZ),
        )
        .expect("failed to derive heater PWM timer clock");
    timer1.start(heater_timer_cfg);
    let _ = heater_pwm.set_duty_cycle_percent(0);
    HeaterPwmGate::attach(heater_pwm);
    let heater_pwm = HeaterPwmGate::new();
    let buzzer = initialize_buzzer_pwm(
        timer2,
        operator2,
        context.buzzer_pin,
        pwm_clock_cfg,
        #[cfg(feature = "buzzer-observe")]
        context.pcnt,
    );
    spawn_buzzer_runtime(
        buzzer.timer,
        buzzer.pwm,
        pwm_clock_cfg,
        context.buzzer_realtime_spawner,
        #[cfg(feature = "buzzer-observe")]
        buzzer.edge_counter,
    );
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
    fan_enable.set_low();
    (fan_enable, fan_pwm, heater_pwm, 0)
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootMemoryContext<'a> {
    eeprom_i2c: &'a mut I2c<'static>,
    scratch: Option<MemoryIoScratch>,
    #[cfg(feature = "web_serial")]
    usb_serial: &'a mut RawUsbSerialJtag,
    #[cfg(not(feature = "web_serial"))]
    persistence_log_sink: &'a mut NoopPersistenceLogSink,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootMemoryState {
    memory_config: MemoryConfig,
    memory_sequence: u32,
    eeprom_required: bool,
    eeprom_data_incompatible: bool,
    prepared_layout_recovery_pending: bool,
    eeprom_restore_pending: bool,
    persistence_source: &'static str,
    persistence_record_state: &'static str,
}

#[cfg(target_arch = "xtensa")]
pub(crate) struct BootMemoryRecoveryState<'a> {
    recovery_pending: &'a mut bool,
    eeprom_required: &'a mut bool,
    eeprom_restore_pending: &'a mut bool,
    persistence_source: &'a mut &'static str,
    persistence_record_state: &'a mut &'static str,
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn load_boot_memory_record(
    context: &mut BootMemoryContext<'_>,
    eeprom_record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> (Option<MemoryRecord>, bool, bool, bool) {
    let Some(scratch) = context.scratch.as_mut() else {
        return (None, false, true, false);
    };
    load_eeprom_memory_record(context.eeprom_i2c, scratch, eeprom_record_staging).await
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn initialize_blank_boot_memory(
    context: &mut BootMemoryContext<'_>,
    eeprom_record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
    record: &mut Option<MemoryRecord>,
    eeprom_required: &mut bool,
    eeprom_data_incompatible: bool,
) {
    if *eeprom_required || eeprom_data_incompatible || record.is_some() {
        return;
    }
    let initialization_result = {
        #[cfg(feature = "web_serial")]
        let init_log_sink = context.usb_serial as &mut dyn PersistenceLogSink;
        #[cfg(not(feature = "web_serial"))]
        let init_log_sink = context.persistence_log_sink as &mut dyn PersistenceLogSink;
        initialize_fpr2_defaults(context.eeprom_i2c, init_log_sink, eeprom_record_staging).await
    };
    if let Some(sequence) = initialization_result {
        info!("blank EEPROM initialized and verified");
        *record = Some(MemoryRecord {
            sequence,
            config: MemoryConfig::default(),
        });
    } else {
        *eeprom_required = true;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn recover_boot_memory_layout(
    context: &mut BootMemoryContext<'_>,
    eeprom_record_staging: &mut [u8; EEPROM_RECORD_STAGING_BYTES],
    memory_sequence: u32,
    state: BootMemoryRecoveryState<'_>,
) {
    if !*state.recovery_pending {
        return;
    }
    let recovery_result = if context.scratch.is_some() {
        #[cfg(feature = "web_serial")]
        let recovery_log_sink = context.usb_serial as &mut dyn PersistenceLogSink;
        #[cfg(not(feature = "web_serial"))]
        let recovery_log_sink = context.persistence_log_sink as &mut dyn PersistenceLogSink;
        recover_prepared_fpr2_layout(
            context.eeprom_i2c,
            memory_sequence,
            recovery_log_sink,
            eeprom_record_staging,
        )
        .await
    } else {
        Err(MemoryCommitFailure {
            error: MemoryCommitError::VerifyUnreadable,
            phase: "active-recovery",
            attempt: 1,
            sequence: memory_sequence,
            domain: PersistDomain::LayoutMarker,
            slot: PersistSlot::A,
        })
    };
    if recovery_result.is_ok() {
        *state.eeprom_required = false;
        *state.recovery_pending = false;
        *state.eeprom_restore_pending = false;
        *state.persistence_source = "eeprom";
        *state.persistence_record_state = "valid";
        info!(
            "prepared FPR2 layout recovery complete seq={=u32}",
            memory_sequence
        );
    } else {
        *state.eeprom_required = true;
        *state.eeprom_restore_pending = false;
        warn!("prepared FPR2 layout recovery failed; keeping heater interlocked");
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn initialize_boot_memory<'a>(
    mut context: BootMemoryContext<'a>,
    eeprom_record_staging: &'static mut [u8; EEPROM_RECORD_STAGING_BYTES],
) -> (
    BootMemoryState,
    Option<MemoryIoScratch>,
    &'static mut [u8; EEPROM_RECORD_STAGING_BYTES],
) {
    let (mut record, eeprom_data_incompatible, mut eeprom_required, recovery_pending) =
        load_boot_memory_record(&mut context, &mut *eeprom_record_staging).await;
    if eeprom_data_incompatible && record.is_none() {
        eeprom_required = true;
    }
    initialize_blank_boot_memory(
        &mut context,
        &mut *eeprom_record_staging,
        &mut record,
        &mut eeprom_required,
        eeprom_data_incompatible,
    )
    .await;
    let mut persistence_source = if eeprom_required {
        "none"
    } else if record.is_some() {
        "eeprom"
    } else {
        "defaults"
    };
    let mut persistence_record_state = if eeprom_required && eeprom_data_incompatible {
        "incompatible"
    } else if eeprom_required {
        "unavailable"
    } else if record.is_some() {
        "valid"
    } else if eeprom_data_incompatible {
        "incompatible"
    } else {
        "blank"
    };
    let (memory_config, memory_sequence) = record
        .map(|record| (record.config, record.sequence))
        .unwrap_or_default();
    let mut eeprom_restore_pending = eeprom_data_incompatible || recovery_pending;
    let mut prepared_layout_recovery_pending = recovery_pending;
    recover_boot_memory_layout(
        &mut context,
        &mut *eeprom_record_staging,
        memory_sequence,
        BootMemoryRecoveryState {
            recovery_pending: &mut prepared_layout_recovery_pending,
            eeprom_required: &mut eeprom_required,
            eeprom_restore_pending: &mut eeprom_restore_pending,
            persistence_source: &mut persistence_source,
            persistence_record_state: &mut persistence_record_state,
        },
    )
    .await;
    let scratch = context.scratch;
    (
        BootMemoryState {
            memory_config,
            memory_sequence,
            eeprom_required,
            eeprom_data_incompatible,
            prepared_layout_recovery_pending,
            eeprom_restore_pending,
            persistence_source,
            persistence_record_state,
        },
        scratch,
        eeprom_record_staging,
    )
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run_key_test_boot(context: BootKeyTestContext<'_>) -> ! {
    let BootKeyTestContext {
        display,
        canvas,
        inputs,
        status_light_started_ms,
        pd_port,
        initial_pd_observation,
        #[cfg(feature = "web_serial")]
        usb_serial,
        #[cfg(feature = "web_serial")]
        usb_rx_line,
        #[cfg(feature = "web_serial")]
        usb_tx_buf,
        #[cfg(feature = "web_serial")]
        usb_boot_memory_config,
    } = context;
    match run_key_test_runtime(
        display,
        canvas,
        inputs,
        status_light_started_ms,
        pd_port,
        initial_pd_observation,
    )
    .await
    {
        Err(()) => {
            #[cfg(feature = "web_serial")]
            run_usb_recovery_control_loop(
                usb_serial,
                usb_rx_line,
                usb_tx_buf,
                usb_boot_memory_config,
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

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_display_from_parts(
    mut system: Box<BootSystem>,
    tokens: BootDeviceTokens,
    storage: Box<MaybeUninit<BootDisplay>>,
) -> Box<BootDisplay> {
    let BootDeviceTokens {
        spi2,
        display_sck,
        display_mosi,
        display_cs,
        display_dc,
        display_rst,
        center,
        right,
        down,
        left,
        up,
        fan_enable,
        fan_pwm,
        heater_pwm,
        buzzer,
        mcpwm0,
        #[cfg(feature = "buzzer-observe")]
        pcnt,
        adc1,
        vin_adc,
        rtd_adc,
        #[cfg(feature = "net_http")]
        wifi,
    } = tokens;
    let (mut display, canvas, inputs) = initialize_boot_display(BootDisplayContext {
        runtime_mode: system.runtime_mode,
        runtime: BootDisplayRuntimeContext {
            startup_sequence: &mut system.startup_sequence,
            pd_port: &system.pd_port,
            initial_pd_observation: &mut system.initial_pd_observation,
            status_light: StatusLightState::Booting,
            #[cfg(feature = "web_serial")]
            usb_serial: &mut system.usb_serial,
            #[cfg(feature = "web_serial")]
            usb_rx_line: &mut *system.usb_rx_line,
            #[cfg(feature = "web_serial")]
            usb_tx_buf: &mut *system.usb_tx_buf,
            #[cfg(feature = "web_serial")]
            usb_boot_memory_config: &system.usb_boot_memory_config,
        },
        spi2,
        display_sck,
        display_mosi,
        display_cs,
        display_dc,
        display_rst,
        center,
        right,
        down,
        left,
        up,
    })
    .await;
    if system.runtime_mode == FrontPanelRuntimeMode::KeyTest {
        run_key_test_boot(BootKeyTestContext {
            display: &mut display,
            canvas,
            inputs,
            status_light_started_ms: system.status_light_started_ms,
            pd_port: &system.pd_port,
            initial_pd_observation: &mut system.initial_pd_observation,
            #[cfg(feature = "web_serial")]
            usb_serial: &mut system.usb_serial,
            #[cfg(feature = "web_serial")]
            usb_rx_line: &mut *system.usb_rx_line,
            #[cfg(feature = "web_serial")]
            usb_tx_buf: &mut *system.usb_tx_buf,
            #[cfg(feature = "web_serial")]
            usb_boot_memory_config: &system.usb_boot_memory_config,
        })
        .await;
    }
    finish_boot_display(
        storage,
        system,
        BootAdcTokens {
            adc1,
            vin_adc,
            rtd_adc,
            #[cfg(feature = "net_http")]
            wifi,
        },
        BootOutputTokens {
            fan_enable,
            fan_pwm,
            heater_pwm,
            buzzer,
            mcpwm0,
            #[cfg(feature = "buzzer-observe")]
            pcnt,
        },
        display,
        canvas,
        inputs,
    )
}

#[cfg(target_arch = "xtensa")]
fn finish_boot_display(
    storage: Box<MaybeUninit<BootDisplay>>,
    system: Box<BootSystem>,
    tokens: BootAdcTokens,
    output_tokens: BootOutputTokens,
    display: RuntimeDisplay,
    canvas: &'static mut DisplayCanvas,
    inputs: FrontPanelInputs<'static>,
) -> Box<BootDisplay> {
    Box::write(
        storage,
        BootDisplay {
            system,
            tokens,
            output_tokens: Some(output_tokens),
            output_state: None,
            boot_memory_io_scratch: None,
            display,
            canvas,
            inputs,
        },
    )
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_display_stage(pipeline: &mut BootPipeline) {
    let system = pipeline
        .system
        .take()
        .expect("boot system is available for display initialization");
    let device_tokens = pipeline
        .device_tokens
        .take()
        .expect("device tokens are available for display initialization");
    let display_storage = Box::<BootDisplay>::new_uninit();
    pipeline.display =
        Some(initialize_boot_display_from_parts(system, device_tokens, display_storage).await);
    initialize_boot_outputs_stage(
        pipeline
            .display
            .as_mut()
            .expect("display stage completes before output initialization"),
    );
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) fn initialize_boot_outputs_stage(boot: &mut BootDisplay) {
    let BootOutputTokens {
        fan_enable: fan_enable_pin,
        fan_pwm: fan_pwm_pin,
        heater_pwm: heater_pwm_pin,
        buzzer: buzzer_pin,
        mcpwm0,
        #[cfg(feature = "buzzer-observe")]
        pcnt,
    } = boot
        .output_tokens
        .take()
        .expect("output tokens initialized exactly once");
    let (fan_enable, fan_pwm, heater_pwm, last_heater_duty) =
        initialize_boot_outputs(BootOutputContext {
            startup_sequence: &mut boot.system.startup_sequence,
            fan_enable_pin,
            fan_pwm_pin,
            heater_pwm_pin,
            buzzer_pin,
            mcpwm0,
            #[cfg(feature = "buzzer-observe")]
            pcnt,
            buzzer_realtime_spawner: boot.system.buzzer_realtime_spawner,
            #[cfg(feature = "web_serial")]
            usb_serial: &mut boot.system.usb_serial,
        });
    boot.output_state = Some(BootOutputState {
        fan_enable,
        fan_pwm,
        heater_pwm,
        last_heater_duty,
    });
    boot.boot_memory_io_scratch = try_allocate_memory_io_scratch();
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_memory_from_display(
    boot: Box<BootDisplay>,
    storage: Box<MaybeUninit<BootMemoryReady>>,
) -> Box<BootMemoryReady> {
    let BootDisplay {
        system,
        tokens,
        display,
        canvas,
        inputs,
        output_tokens,
        output_state,
        boot_memory_io_scratch,
    } = *boot;
    let mut system = *system;
    debug_assert!(output_tokens.is_none());
    let BootOutputState {
        fan_enable,
        fan_pwm,
        heater_pwm,
        last_heater_duty,
    } = output_state.expect("boot outputs initialized before memory stage");
    let eeprom_record_staging = system.eeprom_record_staging;
    let memory_context = BootMemoryContext {
        eeprom_i2c: &mut system.eeprom_i2c,
        scratch: boot_memory_io_scratch,
        #[cfg(feature = "web_serial")]
        usb_serial: &mut system.usb_serial,
        #[cfg(not(feature = "web_serial"))]
        persistence_log_sink: &mut system.persistence_log_sink,
    };
    let (memory, boot_memory_io_scratch, eeprom_record_staging) = Box::pin(initialize_boot_memory(
        memory_context,
        eeprom_record_staging,
    ))
    .await;
    system.eeprom_record_staging = eeprom_record_staging;
    Box::write(
        storage,
        BootMemoryReady {
            system,
            tokens,
            display,
            canvas,
            inputs,
            fan_enable,
            fan_pwm,
            heater_pwm,
            last_heater_duty,
            boot_memory_io_scratch,
            memory,
        },
    )
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_boot_memory_stage(pipeline: &mut BootPipeline) {
    let display = pipeline
        .display
        .take()
        .expect("display stage completes before memory initialization");
    let memory_ready_storage = Box::<BootMemoryReady>::new_uninit();
    pipeline.memory_ready =
        Some(initialize_boot_memory_from_display(display, memory_ready_storage).await);
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) fn start_rtos(timg0: esp_hal::peripherals::TIMG0<'static>) {
    let timg0 = TimerGroup::new(timg0);
    esp_rtos::start(timg0.timer0);
    rom_boot_stage(b"rtos_started");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_frontpanel_runtime_task(state: Box<RuntimeLoopState>) {
    run_runtime_loop(state).await;
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_system_stage_task(spawner: Spawner, mut pipeline: Box<BootPipeline>) {
    let system_tokens = pipeline
        .system_tokens
        .take()
        .expect("system tokens are available for boot initialization");
    let system_storage = Box::<BootSystem>::new_uninit();
    pipeline.system = Some(initialize_boot_system(
        spawner,
        system_tokens,
        system_storage,
    ));
    spawner
        .spawn(run_boot_pd_stage_task(spawner, pipeline))
        .expect("failed to spawn PD boot stage");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_pd_stage_task(spawner: Spawner, mut pipeline: Box<BootPipeline>) {
    initialize_boot_pd(
        spawner,
        pipeline
            .system
            .as_mut()
            .expect("boot system is available for PD initialization"),
    )
    .await;
    spawner
        .spawn(run_boot_display_stage_task(spawner, pipeline))
        .expect("failed to spawn display boot stage");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_display_stage_task(spawner: Spawner, mut pipeline: Box<BootPipeline>) {
    initialize_boot_display_stage(&mut pipeline).await;
    spawner
        .spawn(run_boot_memory_stage_task(spawner, pipeline))
        .expect("failed to spawn memory boot stage");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_memory_stage_task(spawner: Spawner, mut pipeline: Box<BootPipeline>) {
    initialize_boot_memory_stage(&mut pipeline).await;
    spawner
        .spawn(run_boot_runtime_stage_task(spawner, pipeline))
        .expect("failed to spawn runtime boot stage");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_runtime_stage_task(spawner: Spawner, mut pipeline: Box<BootPipeline>) {
    let state = initialize_runtime_state_stage(spawner, &mut pipeline).await;
    spawner
        .spawn(run_boot_runtime_finalize_task(spawner, state))
        .expect("failed to spawn runtime finalize stage");
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn run_boot_runtime_finalize_task(spawner: Spawner, state: Box<BootRuntimeState>) {
    rom_boot_stage(b"runtime_ready");
    let runtime_loop_storage = Box::<RuntimeLoopState>::new_uninit();
    let state = state.into_runtime_loop(runtime_loop_storage);
    spawner
        .spawn(run_frontpanel_runtime_task(state))
        .expect("failed to spawn front-panel runtime task");
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn run(spawner: Spawner) {
    // These ROM-only markers remain available before the HAL clock singleton
    // exists, so a startup panic can be placed on either side of `init`.
    rom_boot_stage(b"hal_init_enter");
    let config = esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max());
    rom_boot_stage(b"hal_init_configured");
    let peripherals = esp_hal::init(config);
    rom_boot_stage(b"hal_init_complete");
    let (timg0, system_tokens, device_tokens) = split_boot_tokens(peripherals);
    start_rtos(timg0);
    init_runtime_heap();
    // The root task transfers both token groups into heap storage before any
    // asynchronous boot work. Every complete boot state then stays out of the
    // root task's guarded stack.
    let pipeline_storage = Box::<BootPipeline>::new_uninit();
    let pipeline = Box::write(
        pipeline_storage,
        BootPipeline::new(system_tokens, device_tokens),
    );
    spawner
        .spawn(run_boot_system_stage_task(spawner, pipeline))
        .expect("failed to spawn system boot stage");
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_runtime_state_from_ready(
    spawner: Spawner,
    ready: Box<BootMemoryReady>,
) -> Box<BootRuntimeState> {
    // Reserve both large permanent state objects before startup work can
    // consume the internal heap. Their construction is written directly to
    // these allocations, avoiding a full runtime-state temporary on CPU0.
    let runtime_state_storage = Box::<BootRuntimeState>::new_uninit();
    let mut runtime = BootRuntimeState::from_ready(ready, runtime_state_storage);
    runtime.initialize_power().await;
    runtime.initialize_adc().await;
    runtime.initialize_safety();
    runtime.present_initial_ui().await;
    runtime.restore_legacy_memory().await;
    runtime.start_network(&spawner).await;
    #[cfg(feature = "web_serial")]
    let _ = usb_write_bytes_bounded(
        &mut runtime.system.usb_serial,
        RUNTIME_READY_BOOT_STAGE_LINE,
    );
    runtime
}

#[cfg(target_arch = "xtensa")]
#[inline(never)]
pub(crate) async fn initialize_runtime_state_stage(
    spawner: Spawner,
    pipeline: &mut BootPipeline,
) -> Box<BootRuntimeState> {
    let ready = pipeline
        .memory_ready
        .take()
        .expect("memory stage completes before runtime initialization");
    initialize_runtime_state_from_ready(spawner, ready).await
}
