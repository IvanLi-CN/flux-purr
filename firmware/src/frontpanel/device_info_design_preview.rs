//! Host-only deterministic scenarios using the production renderer.
use super::*;
pub fn render(canvas: &mut DisplayCanvas, theme: DashboardThemeId, scene: &str) {
    let mut state = FrontPanelUiState::new(super::super::FrontPanelRuntimeMode::App);
    state.route = FrontPanelRoute::DeviceInfo;
    state
        .device_info
        .set_identity(&crate::control_plane::Identity::firmware_from_mac([
            0xd0, 0xf1, 0x30, 0x8a, 0x14, 0x08,
        ]));
    state.device_info.thermal = "VALID";
    state.device_info.rtd = "SET";
    state.device_info.vin = "SET";
    state.device_info.config = "OK";
    state.device_info.uptime_seconds = 8323;
    state.device_info.reset = "POWER ON";
    state.device_info_page = match scene {
        "c2-calibration" => super::super::DeviceInfoPage::Calibration,
        "c3-runtime" => super::super::DeviceInfoPage::Runtime,
        _ => super::super::DeviceInfoPage::Identity,
    };
    render_frontpanel_ui_with_theme(canvas, &state, theme, &DEFAULT_TEMPERATURE_PALETTE);
}
