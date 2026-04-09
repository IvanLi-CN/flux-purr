#![no_std]

pub mod adapters;
pub mod board;

use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(not(target_os = "none"))]
extern crate std;

pub const FAN_PHASE_DURATION_SECS: u32 = 10;
pub const FAN_PWM_FREQUENCY_HZ: u32 = 25_000;
pub const FAN_HIGH_PWM_PERMILLE: u16 = 30;
pub const FAN_MID_PWM_PERMILLE: u16 = 300;
pub const FAN_LOW_PWM_PERMILLE: u16 = 500;
pub const FAN_STOP_SAFE_PWM_PERMILLE: u16 = FAN_LOW_PWM_PERMILLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanPcbVariant {
    FiveVolt,
    TwelveVolt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanRailProfile {
    pub variant: FanPcbVariant,
    pub approx_max_output_mv: u16,
    pub approx_output_span_mv: u16,
    pub startup_boost_mv: u16,
    pub startup_boost_ms: u16,
}

impl FanRailProfile {
    pub const fn min_output_mv(self) -> u16 {
        self.approx_max_output_mv - self.approx_output_span_mv
    }
}

pub const FAN_RAIL_PROFILE_5V: FanRailProfile = FanRailProfile {
    variant: FanPcbVariant::FiveVolt,
    approx_max_output_mv: 5_061,
    approx_output_span_mv: 2_068,
    startup_boost_mv: 5_000,
    startup_boost_ms: 200,
};

pub const FAN_RAIL_PROFILE_12V: FanRailProfile = FanRailProfile {
    variant: FanPcbVariant::TwelveVolt,
    approx_max_output_mv: 12_043,
    approx_output_span_mv: 5_456,
    startup_boost_mv: 12_000,
    startup_boost_ms: 200,
};

pub const FAN_DEFAULT_RAIL_PROFILE: FanRailProfile = FAN_RAIL_PROFILE_5V;
pub const FAN_APPROX_MAX_OUTPUT_MV: u16 = FAN_DEFAULT_RAIL_PROFILE.approx_max_output_mv;
pub const FAN_APPROX_OUTPUT_SPAN_MV: u16 = FAN_DEFAULT_RAIL_PROFILE.approx_output_span_mv;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMode {
    Idle,
    Sampling,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontPanelKey {
    Center,
    Right,
    Down,
    Left,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdState {
    Negotiating,
    Ready,
    Fallback5V,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceStatus {
    pub mode: DeviceMode,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: PdState,
    pub fan_enabled: bool,
    pub fan_pwm_permille: u16,
    pub frontpanel_key: Option<FrontPanelKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanPhase {
    High,
    Low,
    Mid,
    Stop,
}

impl FanPhase {
    pub const fn next(self) -> Self {
        match self {
            Self::High => Self::Low,
            Self::Low => Self::Mid,
            Self::Mid => Self::Stop,
            Self::Stop => Self::High,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanCommand {
    pub phase: FanPhase,
    pub enabled: bool,
    pub pwm_permille: u16,
}

impl FanCommand {
    pub const fn from_phase(phase: FanPhase) -> Self {
        match phase {
            FanPhase::High => Self {
                phase,
                enabled: true,
                pwm_permille: FAN_HIGH_PWM_PERMILLE,
            },
            FanPhase::Low => Self {
                phase,
                enabled: true,
                pwm_permille: FAN_LOW_PWM_PERMILLE,
            },
            FanPhase::Mid => Self {
                phase,
                enabled: true,
                pwm_permille: FAN_MID_PWM_PERMILLE,
            },
            FanPhase::Stop => Self {
                phase,
                enabled: false,
                pwm_permille: FAN_STOP_SAFE_PWM_PERMILLE,
            },
        }
    }

    pub const fn approx_output_mv(self) -> u16 {
        self.approx_output_mv_for_profile(FAN_DEFAULT_RAIL_PROFILE)
    }

    pub const fn approx_output_mv_for_profile(self, profile: FanRailProfile) -> u16 {
        if self.enabled {
            fan_output_voltage_mv_from_pwm_permille_for_profile(profile, self.pwm_permille)
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanCycleController {
    phase: FanPhase,
    phase_started_at_secs: u32,
}

impl Default for FanCycleController {
    fn default() -> Self {
        Self::new()
    }
}

impl FanCycleController {
    pub const fn new() -> Self {
        Self {
            phase: FanPhase::High,
            phase_started_at_secs: 0,
        }
    }

    pub const fn phase(self) -> FanPhase {
        self.phase
    }

    pub fn command_at(&mut self, uptime_secs: u32) -> FanCommand {
        while uptime_secs.saturating_sub(self.phase_started_at_secs) >= FAN_PHASE_DURATION_SECS {
            self.phase = self.phase.next();
            self.phase_started_at_secs = self
                .phase_started_at_secs
                .saturating_add(FAN_PHASE_DURATION_SECS);
        }

        FanCommand::from_phase(self.phase)
    }
}

pub const fn fan_output_voltage_mv_from_pwm_permille_for_profile(
    profile: FanRailProfile,
    pwm_permille: u16,
) -> u16 {
    let duty = if pwm_permille > 1_000 {
        1_000
    } else {
        pwm_permille as u32
    };
    let drop_mv = ((profile.approx_output_span_mv as u32 * duty) + 500) / 1_000;
    (profile.approx_max_output_mv as u32 - drop_mv) as u16
}

pub const fn fan_output_voltage_mv_from_pwm_permille(pwm_permille: u16) -> u16 {
    fan_output_voltage_mv_from_pwm_permille_for_profile(FAN_DEFAULT_RAIL_PROFILE, pwm_permille)
}

pub const fn pwm_percent_from_permille(pwm_permille: u16) -> u8 {
    let bounded = if pwm_permille > 1_000 {
        1_000
    } else {
        pwm_permille
    };
    ((bounded + 5) / 10) as u8
}

static SAMPLE_TICK: AtomicU32 = AtomicU32::new(0);

fn snapshot_at(tick: u32, uptime_secs: u32) -> DeviceStatus {
    let request = adapters::ch224q::VoltageRequest::V28;
    let fallback = tick.is_multiple_of(17);
    let pd_contract_mv = if fallback {
        adapters::ch224q::VoltageRequest::V5.millivolts()
    } else {
        request.millivolts()
    };
    let mut fan = FanCycleController::new();
    let fan_command = fan.command_at(uptime_secs);

    DeviceStatus {
        mode: DeviceMode::Sampling,
        voltage_mv: 12_000 + (tick % 50),
        current_ma: 800 + (tick % 40),
        board_temp_centi: 3_200 + ((tick % 30) as i32),
        pd_request_mv: request.millivolts(),
        pd_contract_mv,
        pd_state: if fallback {
            PdState::Fallback5V
        } else {
            PdState::Ready
        },
        fan_enabled: fan_command.enabled,
        fan_pwm_permille: fan_command.pwm_permille,
        frontpanel_key: if tick.is_multiple_of(10) {
            Some(FrontPanelKey::Center)
        } else {
            None
        },
    }
}

#[cfg(not(target_os = "none"))]
fn accumulate_host_elapsed(
    fractional_elapsed: core::time::Duration,
    elapsed: core::time::Duration,
    uptime_secs: u32,
) -> (u32, core::time::Duration) {
    let accumulated = fractional_elapsed.saturating_add(elapsed);
    let elapsed_secs = accumulated.as_secs().min(u32::MAX as u64) as u32;
    let remainder =
        accumulated.saturating_sub(core::time::Duration::from_secs(elapsed_secs as u64));

    (uptime_secs.saturating_add(elapsed_secs), remainder)
}

#[cfg(all(not(test), not(target_os = "none")))]
fn mock_uptime_secs() -> u32 {
    use std::{
        sync::{Mutex, OnceLock},
        time::Instant,
    };

    struct HostMockClock {
        last_sample_at: Instant,
        fractional_elapsed: core::time::Duration,
        uptime_secs: u32,
    }

    static MOCK_CLOCK: OnceLock<Mutex<HostMockClock>> = OnceLock::new();
    let clock = MOCK_CLOCK.get_or_init(|| {
        Mutex::new(HostMockClock {
            last_sample_at: Instant::now(),
            fractional_elapsed: core::time::Duration::ZERO,
            uptime_secs: 0,
        })
    });
    let mut clock = clock.lock().expect("host mock clock poisoned");
    let now = Instant::now();
    let elapsed = now.duration_since(clock.last_sample_at);
    clock.last_sample_at = now;
    let (uptime_secs, fractional_elapsed) =
        accumulate_host_elapsed(clock.fractional_elapsed, elapsed, clock.uptime_secs);
    clock.uptime_secs = uptime_secs;
    clock.fractional_elapsed = fractional_elapsed;

    clock.uptime_secs
}

#[cfg(all(test, not(target_os = "none")))]
fn mock_uptime_secs() -> u32 {
    0
}

#[cfg(target_os = "none")]
fn mock_uptime_secs() -> u32 {
    SAMPLE_TICK.load(Ordering::Relaxed)
}

pub fn snapshot() -> DeviceStatus {
    let tick = SAMPLE_TICK.fetch_add(1, Ordering::Relaxed);
    snapshot_at(tick, mock_uptime_secs())
}

pub async fn poll_once() -> DeviceStatus {
    embassy_futures::yield_now().await;
    snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_within(actual: u16, expected: u16, tolerance: u16) {
        let delta = actual.abs_diff(expected);
        assert!(
            delta <= tolerance,
            "expected {actual} to be within {tolerance} of {expected}, delta={delta}"
        );
    }

    #[test]
    fn snapshot_starts_in_sampling_mode() {
        let value = snapshot_at(10, 0);
        assert_eq!(value.mode, DeviceMode::Sampling);
        assert!(value.voltage_mv >= 12_000);
        assert!(value.current_ma >= 800);
        assert_eq!(value.pd_request_mv, 28_000);
        assert_eq!(value.pd_contract_mv, 28_000);
        assert_eq!(value.pd_state, PdState::Ready);
        assert_eq!(value.frontpanel_key, Some(FrontPanelKey::Center));
        assert!(value.fan_enabled);
        assert_eq!(value.fan_pwm_permille, FAN_HIGH_PWM_PERMILLE);
    }

    #[test]
    fn snapshot_fan_fields_follow_uptime_not_poll_count() {
        let value = snapshot_at(123, 20);
        assert!(value.fan_enabled);
        assert_eq!(value.fan_pwm_permille, FAN_MID_PWM_PERMILLE);
        assert_eq!(value.pd_contract_mv, 28_000);

        let stopped = snapshot_at(999, 30);
        assert!(!stopped.fan_enabled);
        assert_eq!(stopped.fan_pwm_permille, FAN_STOP_SAFE_PWM_PERMILLE);
    }

    #[test]
    fn snapshot_preserves_pd_fallback_and_frontpanel_key_logic() {
        let fallback = snapshot_at(17, 0);
        assert_eq!(fallback.pd_request_mv, 28_000);
        assert_eq!(fallback.pd_contract_mv, 5_000);
        assert_eq!(fallback.pd_state, PdState::Fallback5V);
        assert_eq!(fallback.frontpanel_key, None);
    }

    #[test]
    fn fan_cycle_controller_advances_on_ten_second_boundaries() {
        let mut controller = FanCycleController::new();

        assert_eq!(controller.command_at(0).phase, FanPhase::High);
        assert_eq!(controller.command_at(9).phase, FanPhase::High);
        assert_eq!(controller.command_at(10).phase, FanPhase::Low);
        assert_eq!(controller.command_at(19).phase, FanPhase::Low);
        assert_eq!(controller.command_at(20).phase, FanPhase::Mid);
        assert_eq!(controller.command_at(29).phase, FanPhase::Mid);
        assert_eq!(controller.command_at(30).phase, FanPhase::Stop);
        assert_eq!(controller.command_at(39).phase, FanPhase::Stop);
        assert_eq!(controller.command_at(40).phase, FanPhase::High);
    }

    #[test]
    fn fan_cycle_controller_handles_large_time_jumps() {
        let mut controller = FanCycleController::new();
        assert_eq!(controller.command_at(85).phase, FanPhase::High);
        assert_eq!(controller.command_at(95).phase, FanPhase::Low);
    }

    #[test]
    fn pwm_mapping_matches_5v_bringup_points() {
        assert_within(
            fan_output_voltage_mv_from_pwm_permille(FAN_HIGH_PWM_PERMILLE),
            5_000,
            80,
        );
        assert_within(
            fan_output_voltage_mv_from_pwm_permille(FAN_MID_PWM_PERMILLE),
            4_400,
            80,
        );
        assert_within(
            fan_output_voltage_mv_from_pwm_permille(FAN_LOW_PWM_PERMILLE),
            4_000,
            80,
        );
        assert_eq!(fan_output_voltage_mv_from_pwm_permille(1_000), 2_993);
    }

    #[test]
    fn pwm_mapping_matches_12v_variant_points() {
        assert_eq!(
            fan_output_voltage_mv_from_pwm_permille_for_profile(
                FAN_RAIL_PROFILE_12V,
                FAN_HIGH_PWM_PERMILLE,
            ),
            11_879,
        );
        assert_eq!(
            fan_output_voltage_mv_from_pwm_permille_for_profile(
                FAN_RAIL_PROFILE_12V,
                FAN_MID_PWM_PERMILLE,
            ),
            10_406,
        );
        assert_eq!(
            fan_output_voltage_mv_from_pwm_permille_for_profile(
                FAN_RAIL_PROFILE_12V,
                FAN_LOW_PWM_PERMILLE,
            ),
            9_315,
        );
        assert_eq!(
            fan_output_voltage_mv_from_pwm_permille_for_profile(FAN_RAIL_PROFILE_12V, 1_000),
            6_587,
        );
    }

    #[test]
    fn rail_profiles_freeze_variant_startup_requirements() {
        assert_eq!(FAN_DEFAULT_RAIL_PROFILE, FAN_RAIL_PROFILE_5V);
        assert_eq!(FAN_RAIL_PROFILE_5V.startup_boost_mv, 5_000);
        assert_eq!(FAN_RAIL_PROFILE_5V.startup_boost_ms, 200);
        assert_eq!(FAN_RAIL_PROFILE_5V.min_output_mv(), 2_993);
        assert_eq!(FAN_RAIL_PROFILE_12V.startup_boost_mv, 12_000);
        assert_eq!(FAN_RAIL_PROFILE_12V.startup_boost_ms, 200);
        assert_eq!(FAN_RAIL_PROFILE_12V.min_output_mv(), 6_587);
    }

    #[test]
    fn stop_command_disables_output_without_relabeling_it_as_low_speed() {
        let stop = FanCommand::from_phase(FanPhase::Stop);
        let low = FanCommand::from_phase(FanPhase::Low);

        assert!(!stop.enabled);
        assert_eq!(stop.phase, FanPhase::Stop);
        assert_eq!(stop.approx_output_mv(), 0);
        assert_eq!(stop.approx_output_mv_for_profile(FAN_RAIL_PROFILE_12V), 0);
        assert!(low.enabled);
        assert_eq!(low.phase, FanPhase::Low);
        assert_eq!(stop.pwm_permille, FAN_STOP_SAFE_PWM_PERMILLE);
        assert_eq!(low.pwm_permille, FAN_LOW_PWM_PERMILLE);
    }

    #[test]
    fn fan_command_reports_variant_specific_output_estimates() {
        let high = FanCommand::from_phase(FanPhase::High);

        assert_eq!(high.approx_output_mv(), 4_999);
        assert_eq!(
            high.approx_output_mv_for_profile(FAN_RAIL_PROFILE_12V),
            11_879,
        );
        assert!(high.approx_output_mv_for_profile(FAN_RAIL_PROFILE_12V) > high.approx_output_mv());
    }

    #[test]
    fn permille_maps_to_rounded_percent_for_ledc_api() {
        assert_eq!(pwm_percent_from_permille(FAN_HIGH_PWM_PERMILLE), 3);
        assert_eq!(pwm_percent_from_permille(FAN_MID_PWM_PERMILLE), 30);
        assert_eq!(pwm_percent_from_permille(FAN_LOW_PWM_PERMILLE), 50);
        assert_eq!(pwm_percent_from_permille(1_500), 100);
    }

    #[cfg(not(target_os = "none"))]
    #[test]
    fn host_mock_elapsed_accumulates_fractional_seconds_between_polls() {
        use core::time::Duration;

        let (uptime_secs, fractional_elapsed) =
            accumulate_host_elapsed(Duration::ZERO, Duration::from_millis(1_500), 0);
        assert_eq!(uptime_secs, 1);
        assert_eq!(fractional_elapsed, Duration::from_millis(500));

        let (uptime_secs, fractional_elapsed) = accumulate_host_elapsed(
            fractional_elapsed,
            Duration::from_millis(1_500),
            uptime_secs,
        );
        assert_eq!(uptime_secs, 3);
        assert_eq!(fractional_elapsed, Duration::ZERO);

        let (uptime_secs, remainder) =
            accumulate_host_elapsed(Duration::from_millis(400), Duration::from_millis(700), 9);
        assert_eq!(uptime_secs, 10);
        assert_eq!(remainder, Duration::from_millis(100));
    }
}
