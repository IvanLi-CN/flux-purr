use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use flux_purr_firmware::{
    DEFAULT_PD_VOLTAGE_REQUEST,
    display::{
        DISPLAY_FRAMEBUFFER_BYTES, DISPLAY_PANEL_CONFIG, DISPLAY_PHYSICAL_HEIGHT,
        DISPLAY_PHYSICAL_WIDTH, DisplayCanvas,
    },
    frontpanel::{
        FanDisplayState, FrontPanelKeyMap, FrontPanelMenuItem, FrontPanelRawState, FrontPanelRoute,
        FrontPanelRuntimeMode, FrontPanelUiState, HeaterLockReason, KeyEvent, KeyGesture,
        RawFrontPanelKey, render::render_frontpanel_ui,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewPreset {
    KeyTestIdle,
    KeyTestShort,
    KeyTestDouble,
    KeyTestLong,
    Dashboard,
    DashboardManual,
    DashboardFanOff,
    DashboardFanAuto,
    DashboardFanRun,
    DashboardOvertempA,
    DashboardOvertempB,
    Menu,
    PresetTemp,
    ActiveCooling,
    WifiInfo,
    DeviceInfo,
}

fn build_key_test_state(raw_key: RawFrontPanelKey, gesture: KeyGesture) -> FrontPanelUiState {
    let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::KeyTest);
    let mut raw_state = FrontPanelRawState::default();
    raw_state.set_pressed(raw_key, true);
    state.set_raw_state(raw_state);

    let _ = state.handle_event(KeyEvent {
        raw_key,
        key: FrontPanelKeyMap::default().logical_from_raw(raw_key),
        gesture,
        at_ms: 0,
    });

    state
}

fn base_dashboard_state() -> FrontPanelUiState {
    let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    state.pd_contract_mv = DEFAULT_PD_VOLTAGE_REQUEST.millivolts();
    state.target_temp_c = 180;
    state.current_temp_c = 32;
    state.current_temp_deci_c = 321;
    state.heater_enabled = true;
    state.heater_output_percent = 18;
    state
}

impl PreviewPreset {
    const fn slug(self) -> &'static str {
        match self {
            Self::KeyTestIdle => "key-test-idle",
            Self::KeyTestShort => "key-test-short",
            Self::KeyTestDouble => "key-test-double",
            Self::KeyTestLong => "key-test-long",
            Self::Dashboard => "dashboard",
            Self::DashboardManual => "dashboard-manual",
            Self::DashboardFanOff => "dashboard-fan-off",
            Self::DashboardFanAuto => "dashboard-fan-auto",
            Self::DashboardFanRun => "dashboard-fan-run",
            Self::DashboardOvertempA => "dashboard-overtemp-a",
            Self::DashboardOvertempB => "dashboard-overtemp-b",
            Self::Menu => "menu",
            Self::PresetTemp => "preset-temp",
            Self::ActiveCooling => "active-cooling",
            Self::WifiInfo => "wifi-info",
            Self::DeviceInfo => "device-info",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "key-test-idle" | "keytest-idle" => Some(Self::KeyTestIdle),
            "key-test-short" | "keytest-short" => Some(Self::KeyTestShort),
            "key-test-double" | "keytest-double" => Some(Self::KeyTestDouble),
            "key-test-long" | "keytest-long" => Some(Self::KeyTestLong),
            "dashboard" => Some(Self::Dashboard),
            "dashboard-manual" => Some(Self::DashboardManual),
            "dashboard-fan-off" => Some(Self::DashboardFanOff),
            "dashboard-fan-auto" => Some(Self::DashboardFanAuto),
            "dashboard-fan-run" => Some(Self::DashboardFanRun),
            "dashboard-overtemp-a" => Some(Self::DashboardOvertempA),
            "dashboard-overtemp-b" => Some(Self::DashboardOvertempB),
            "menu" => Some(Self::Menu),
            "preset-temp" => Some(Self::PresetTemp),
            "active-cooling" => Some(Self::ActiveCooling),
            "wifi-info" => Some(Self::WifiInfo),
            "device-info" => Some(Self::DeviceInfo),
            _ => None,
        }
    }

    fn build_state(self) -> FrontPanelUiState {
        match self {
            Self::KeyTestIdle => FrontPanelUiState::new(FrontPanelRuntimeMode::KeyTest),
            Self::KeyTestShort => {
                build_key_test_state(RawFrontPanelKey::Up, KeyGesture::ShortPress)
            }
            Self::KeyTestDouble => {
                build_key_test_state(RawFrontPanelKey::CenterBoot, KeyGesture::DoublePress)
            }
            Self::KeyTestLong => {
                build_key_test_state(RawFrontPanelKey::Down, KeyGesture::LongPress)
            }
            Self::Dashboard => FrontPanelUiState::new(FrontPanelRuntimeMode::App),
            Self::DashboardManual => {
                let mut state = base_dashboard_state();
                state.current_temp_c = 365;
                state.current_temp_deci_c = 3654;
                state.target_temp_c = 380;
                state.heater_output_percent = 64;
                state.fan_enabled = true;
                state.fan_display_state = FanDisplayState::Run;
                state
            }
            Self::DashboardFanOff => {
                let mut state = base_dashboard_state();
                state.current_temp_c = 96;
                state.current_temp_deci_c = 962;
                state.heater_enabled = false;
                state.heater_output_percent = 0;
                state.active_cooling_enabled = false;
                state.fan_enabled = false;
                state.fan_display_state = FanDisplayState::Off;
                state
            }
            Self::DashboardFanAuto => {
                let mut state = base_dashboard_state();
                state.current_temp_c = 34;
                state.current_temp_deci_c = 341;
                state.fan_enabled = false;
                state.fan_display_state = FanDisplayState::Auto;
                state
            }
            Self::DashboardFanRun => {
                let mut state = base_dashboard_state();
                state.current_temp_c = 58;
                state.current_temp_deci_c = 583;
                state.heater_output_percent = 72;
                state.fan_enabled = true;
                state.fan_display_state = FanDisplayState::Run;
                state
            }
            Self::DashboardOvertempA => {
                let mut state = base_dashboard_state();
                state.current_temp_c = 351;
                state.current_temp_deci_c = 3512;
                state.heater_enabled = false;
                state.heater_output_percent = 0;
                state.active_cooling_enabled = false;
                state.fan_enabled = true;
                state.fan_display_state = FanDisplayState::Off;
                state.heater_lock_reason = Some(HeaterLockReason::CoolingDisabledOvertemp);
                state.dashboard_warning_visible = true;
                state
            }
            Self::DashboardOvertempB => {
                let mut state = Self::DashboardOvertempA.build_state();
                state.dashboard_warning_visible = false;
                state
            }
            Self::Menu => {
                let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
                state.route = FrontPanelRoute::Menu;
                state.selected_menu_item = FrontPanelMenuItem::ActiveCooling;
                state
            }
            Self::PresetTemp => {
                let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
                state.route = FrontPanelRoute::PresetTemp;
                state.selected_preset_slot = 4;
                state
            }
            Self::ActiveCooling => {
                let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
                state.route = FrontPanelRoute::ActiveCooling;
                state.active_cooling_enabled = true;
                state.pd_contract_mv = DEFAULT_PD_VOLTAGE_REQUEST.millivolts();
                state
            }
            Self::WifiInfo => {
                let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
                state.route = FrontPanelRoute::WifiInfo;
                state
            }
            Self::DeviceInfo => {
                let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
                state.route = FrontPanelRoute::DeviceInfo;
                state
            }
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("firmware crate should live under the repo root")
        .to_path_buf()
}

fn default_output_path(preset: PreviewPreset) -> PathBuf {
    repo_root().join(format!(
        "docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/{}.framebuffer.bin",
        preset.slug()
    ))
}

fn panel_output_path(logical_output_path: &Path) -> PathBuf {
    let file_name = logical_output_path
        .file_name()
        .expect("logical framebuffer output path should include a file name")
        .to_string_lossy();

    let companion_name = if let Some(prefix) = file_name.strip_suffix(".framebuffer.bin") {
        format!("{prefix}.panel.framebuffer.bin")
    } else if let Some((stem, ext)) = file_name.rsplit_once('.') {
        format!("{stem}.panel.{ext}")
    } else {
        format!("{file_name}.panel")
    };

    logical_output_path.with_file_name(companion_name)
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let preset_slug = args.next().unwrap_or_else(|| String::from("dashboard"));
    let Some(preset) = PreviewPreset::from_slug(&preset_slug) else {
        eprintln!(
            "unknown frontpanel preset '{}' (known: key-test-idle, key-test-short, key-test-double, key-test-long, dashboard, dashboard-manual, dashboard-fan-off, dashboard-fan-auto, dashboard-fan-run, dashboard-overtemp-a, dashboard-overtemp-b, menu, preset-temp, active-cooling, wifi-info, device-info)",
            preset_slug
        );
        return ExitCode::FAILURE;
    };
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(preset));
    let panel_path = panel_output_path(&output_path);

    let mut canvas = DisplayCanvas::new();
    let state = preset.build_state();
    render_frontpanel_ui(&mut canvas, &state);

    let mut logical_bytes = [0_u8; DISPLAY_FRAMEBUFFER_BYTES];
    canvas.write_rgb565_le_bytes(&mut logical_bytes);
    let mut panel_bytes = [0_u8; DISPLAY_FRAMEBUFFER_BYTES];
    canvas.write_panel_rgb565_be_bytes(&mut panel_bytes);

    if let Some(parent) = output_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!(
            "failed to create output directory {}: {error}",
            parent.display()
        );
        return ExitCode::FAILURE;
    }

    if let Err(error) = fs::write(&output_path, logical_bytes) {
        eprintln!("failed to write {}: {error}", output_path.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&panel_path, panel_bytes) {
        eprintln!("failed to write {}: {error}", panel_path.display());
        return ExitCode::FAILURE;
    }

    println!(
        "wrote {} preset={} width=160 height=50 rgb565_endian=le; panel={} panel_width={} panel_height={} orientation=Landscape dx={} dy={} panel_rgb565_endian=be layout=gc9d01-panel-order",
        output_path.display(),
        preset.slug(),
        panel_path.display(),
        DISPLAY_PHYSICAL_WIDTH,
        DISPLAY_PHYSICAL_HEIGHT,
        DISPLAY_PANEL_CONFIG.dx,
        DISPLAY_PANEL_CONFIG.dy,
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_output_path_tracks_requested_filename() {
        assert_eq!(
            panel_output_path(Path::new("/tmp/dashboard.framebuffer.bin")),
            PathBuf::from("/tmp/dashboard.panel.framebuffer.bin")
        );
    }
}
