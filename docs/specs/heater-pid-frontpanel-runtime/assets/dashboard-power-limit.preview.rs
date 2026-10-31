use flux_purr_firmware::{
    display::{DISPLAY_FRAMEBUFFER_BYTES, DisplayCanvas},
    frontpanel::{
        FrontPanelRuntimeMode, FrontPanelUiState, HeaterLockReason, render::render_frontpanel_ui,
    },
};
use std::{env, fs, io, path::Path};

fn main() -> io::Result<()> {
    let output = env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("missing output directory"))?;
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    for (slug, reason) in [
        (
            "dashboard-power-wait",
            HeaterLockReason::PdContractUnavailable,
        ),
        ("dashboard-power-limit", HeaterLockReason::NoSafeHeaterPower),
    ] {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.current_temp_c = 25;
        state.current_temp_deci_c = 250;
        state.target_temp_c = 180;
        state.pd_contract_mv = 5000;
        state.heater_lock_reason = Some(reason);
        let mut canvas = DisplayCanvas::new();
        render_frontpanel_ui(&mut canvas, &state);
        let mut frame = [0u8; DISPLAY_FRAMEBUFFER_BYTES];
        canvas.write_rgb565_le_bytes(&mut frame);
        fs::write(output.join(format!("{slug}.framebuffer.bin")), frame)?;
    }
    Ok(())
}
