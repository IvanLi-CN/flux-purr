//! Deterministic RGB status-light language for the physical front-panel LED.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbChannels {
    pub red: bool,
    pub green: bool,
    pub blue: bool,
}

impl RgbChannels {
    pub const OFF: Self = Self::new(false, false, false);
    pub const RED: Self = Self::new(true, false, false);
    pub const GREEN: Self = Self::new(false, true, false);
    pub const BLUE: Self = Self::new(false, false, true);
    pub const CYAN: Self = Self::new(false, true, true);
    pub const MAGENTA: Self = Self::new(true, false, true);
    pub const AMBER: Self = Self::new(true, true, false);
    pub const WHITE: Self = Self::new(true, true, true);

    pub const fn new(red: bool, green: bool, blue: bool) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLightState {
    Booting,
    Ready,
    Heating,
    Cooling,
    Calibration,
    HeaterInterlocked,
    CoolingDisabledOvertemp,
    SensorFault,
    ThermalRunawayAttentionPending,
    ThermalRunaway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusLightInputs {
    pub booting: bool,
    pub thermal_runaway: bool,
    pub thermal_runaway_attention_pending: bool,
    pub sensor_fault: bool,
    pub cooling_disabled_overtemp: bool,
    pub heater_interlocked: bool,
    pub calibration_active: bool,
    pub heater_enabled: bool,
    pub fan_enabled: bool,
}

pub const fn select_status_light_state(inputs: StatusLightInputs) -> StatusLightState {
    if inputs.thermal_runaway {
        StatusLightState::ThermalRunaway
    } else if inputs.thermal_runaway_attention_pending {
        StatusLightState::ThermalRunawayAttentionPending
    } else if inputs.sensor_fault {
        StatusLightState::SensorFault
    } else if inputs.cooling_disabled_overtemp {
        StatusLightState::CoolingDisabledOvertemp
    } else if inputs.heater_interlocked {
        StatusLightState::HeaterInterlocked
    } else if inputs.calibration_active {
        StatusLightState::Calibration
    } else if inputs.booting {
        StatusLightState::Booting
    } else if inputs.heater_enabled {
        StatusLightState::Heating
    } else if inputs.fan_enabled {
        StatusLightState::Cooling
    } else {
        StatusLightState::Ready
    }
}

pub const fn status_light_output(state: StatusLightState, elapsed_ms: u64) -> RgbChannels {
    match state {
        StatusLightState::Booting => {
            if periodic_on(elapsed_ms, 700, 350) {
                RgbChannels::WHITE
            } else {
                RgbChannels::OFF
            }
        }
        StatusLightState::Ready => RgbChannels::GREEN,
        StatusLightState::Heating => RgbChannels::AMBER,
        StatusLightState::Cooling => {
            if periodic_on(elapsed_ms, 1_400, 350) {
                RgbChannels::BLUE
            } else {
                RgbChannels::OFF
            }
        }
        StatusLightState::Calibration => {
            if periodic_on(elapsed_ms, 1_000, 500) {
                RgbChannels::CYAN
            } else {
                RgbChannels::OFF
            }
        }
        StatusLightState::HeaterInterlocked => {
            if periodic_on(elapsed_ms, 1_000, 400) {
                RgbChannels::AMBER
            } else {
                RgbChannels::OFF
            }
        }
        StatusLightState::CoolingDisabledOvertemp => {
            burst_output(elapsed_ms, 1_400, 3, 160, 120, RgbChannels::AMBER)
        }
        StatusLightState::SensorFault => {
            burst_output(elapsed_ms, 1_200, 2, 180, 120, RgbChannels::MAGENTA)
        }
        StatusLightState::ThermalRunawayAttentionPending => {
            if periodic_on(elapsed_ms, 1_000, 160) {
                RgbChannels::RED
            } else {
                RgbChannels::OFF
            }
        }
        StatusLightState::ThermalRunaway => {
            if periodic_on(elapsed_ms, 250, 125) {
                RgbChannels::RED
            } else {
                RgbChannels::OFF
            }
        }
    }
}

const fn periodic_on(elapsed_ms: u64, period_ms: u64, on_ms: u64) -> bool {
    elapsed_ms % period_ms < on_ms
}

const fn burst_output(
    elapsed_ms: u64,
    period_ms: u64,
    flashes: u64,
    on_ms: u64,
    gap_ms: u64,
    color: RgbChannels,
) -> RgbChannels {
    let phase_ms = elapsed_ms % period_ms;
    let flash_window_ms = on_ms + gap_ms;
    if phase_ms < flashes * flash_window_ms && phase_ms % flash_window_ms < on_ms {
        color
    } else {
        RgbChannels::OFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_states_override_every_normal_indicator() {
        let state = select_status_light_state(StatusLightInputs {
            booting: true,
            thermal_runaway: true,
            thermal_runaway_attention_pending: true,
            sensor_fault: true,
            cooling_disabled_overtemp: true,
            heater_interlocked: true,
            calibration_active: true,
            heater_enabled: true,
            fan_enabled: true,
        });

        assert_eq!(state, StatusLightState::ThermalRunaway);
    }

    #[test]
    fn safety_states_follow_the_documented_priority_order() {
        let normal_state = StatusLightInputs {
            booting: true,
            heater_enabled: true,
            fan_enabled: true,
            ..StatusLightInputs::default()
        };

        assert_eq!(
            select_status_light_state(StatusLightInputs {
                thermal_runaway_attention_pending: true,
                ..normal_state
            }),
            StatusLightState::ThermalRunawayAttentionPending
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                sensor_fault: true,
                ..normal_state
            }),
            StatusLightState::SensorFault
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                cooling_disabled_overtemp: true,
                ..normal_state
            }),
            StatusLightState::CoolingDisabledOvertemp
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                heater_interlocked: true,
                ..normal_state
            }),
            StatusLightState::HeaterInterlocked
        );
    }

    #[test]
    fn selects_every_non_fault_runtime_state() {
        assert_eq!(
            select_status_light_state(StatusLightInputs::default()),
            StatusLightState::Ready
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                fan_enabled: true,
                ..StatusLightInputs::default()
            }),
            StatusLightState::Cooling
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                heater_enabled: true,
                fan_enabled: true,
                ..StatusLightInputs::default()
            }),
            StatusLightState::Heating
        );
        assert_eq!(
            select_status_light_state(StatusLightInputs {
                calibration_active: true,
                heater_enabled: true,
                ..StatusLightInputs::default()
            }),
            StatusLightState::Calibration
        );
    }

    #[test]
    fn fault_patterns_have_distinct_cadence() {
        assert_eq!(
            status_light_output(StatusLightState::SensorFault, 0),
            RgbChannels::MAGENTA
        );
        assert_eq!(
            status_light_output(StatusLightState::SensorFault, 180),
            RgbChannels::OFF
        );
        assert_eq!(
            status_light_output(StatusLightState::SensorFault, 300),
            RgbChannels::MAGENTA
        );
        assert_eq!(
            status_light_output(StatusLightState::CoolingDisabledOvertemp, 560),
            RgbChannels::AMBER
        );
        assert_eq!(
            status_light_output(StatusLightState::CoolingDisabledOvertemp, 720),
            RgbChannels::OFF
        );
        assert_eq!(
            status_light_output(StatusLightState::ThermalRunaway, 125),
            RgbChannels::OFF
        );
        assert_eq!(
            status_light_output(StatusLightState::ThermalRunawayAttentionPending, 160),
            RgbChannels::OFF
        );
    }
}
