use flux_purr_firmware::display::{
    DISPLAY_FRAMEBUFFER_BYTES, DisplayCanvas, SceneId, render_scene,
};
use std::{env, fs, io, path::Path};

fn main() -> io::Result<()> {
    let mut canvas = DisplayCanvas::new();
    render_scene(SceneId::StartupCalibration, &mut canvas);

    let mut panel_frame = [0u8; DISPLAY_FRAMEBUFFER_BYTES];
    canvas.write_panel_rgb565_be_bytes(&mut panel_frame);
    let asset = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("ram-bringup/assets/calibration.panel.rgb565be.bin");

    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--write") => fs::write(asset, panel_frame),
        Some("--check") => {
            if fs::read(asset)? != panel_frame {
                return Err(io::Error::other("RAM calibration frame is out of date"));
            }
            Ok(())
        }
        Some("--preview-frame") => {
            let path = args.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "missing preview frame path")
            })?;
            let mut logical_frame = [0u8; DISPLAY_FRAMEBUFFER_BYTES];
            canvas.write_rgb565_le_bytes(&mut logical_frame);
            fs::write(path, logical_frame)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: ram_calibration_asset --write|--check|--preview-frame <path>",
        )),
    }
}
