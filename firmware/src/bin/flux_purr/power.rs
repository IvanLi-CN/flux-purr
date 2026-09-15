#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) struct HeaterPowerOutputContext<'a, 'i, PWM> {
    pub(crate) i2c: &'a mut I2c<'i, esp_hal::Blocking>,
    pub(crate) pd_port: &'a mut PdPort,
    pub(crate) heater_pwm: &'a mut PWM,
    pub(crate) backend: &'a mut HeaterPowerBackend,
    pub(crate) hold_pps_governor: &'a mut HoldPpsGovernor,
    pub(crate) manual_pps: &'a mut ManualPpsState,
    pub(crate) pd_observation: Option<PdStatusObservation>,
    pub(crate) measured_heater_mv: u32,
    pub(crate) current_temp_c: f32,
    pub(crate) duty_percent: u8,
    pub(crate) heater_enabled: bool,
    pub(crate) control_phase: HeaterControlPhase,
    pub(crate) control_error_c: f32,
    pub(crate) filtered_slope_c_per_s: f32,
    pub(crate) warmup_soft_start_percent: u8,
    pub(crate) last_physical_duty_percent: &'a mut u8,
    pub(crate) preview_heater_curve: Option<&'a HeaterCurveConfig>,
    pub(crate) memory_config: &'a MemoryConfig,
    pub(crate) active_thermal_settings: ThermalControlProfileSettings,
    pub(crate) now_ms: u64,
}
