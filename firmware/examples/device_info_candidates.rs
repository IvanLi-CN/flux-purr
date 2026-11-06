//! Host-only proposals. Diagnostic fields are fixtures; FW comes from the build.
use flux_purr_firmware::{
    display::{DISPLAY_FRAMEBUFFER_BYTES, DisplayCanvas},
    frontpanel::render::{DashboardThemeId, device_info_design_preview::render},
};
use std::{env, fs, io, path::Path};
fn main() -> io::Result<()> {
    let output = env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("missing output directory"))?;
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    for (theme, palette) in [
        ("light", DashboardThemeId::Light),
        ("dark", DashboardThemeId::Dark),
    ] {
        for scene in ["c1-identity", "c2-calibration", "c3-runtime"] {
            let mut canvas = DisplayCanvas::new();
            render(&mut canvas, palette, scene);
            let mut frame = [0; DISPLAY_FRAMEBUFFER_BYTES];
            canvas.write_rgb565_le_bytes(&mut frame);
            fs::write(output.join(format!("{scene}-{theme}.bin")), frame)?;
        }
    }
    Ok(())
}
