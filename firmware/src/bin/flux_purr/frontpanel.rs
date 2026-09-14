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
