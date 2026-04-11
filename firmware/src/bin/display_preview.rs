use std::{env, fs, path::PathBuf, process::ExitCode};

use flux_purr_firmware::display::{
    DISPLAY_FRAMEBUFFER_BYTES, DISPLAY_PANEL_CONFIG, DISPLAY_PHYSICAL_HEIGHT,
    DISPLAY_PHYSICAL_WIDTH, DisplayCanvas, STARTUP_SCENE_SLUG, SceneId, render_scene,
};

fn default_output_path(scene: SceneId) -> PathBuf {
    PathBuf::from(format!(
        "docs/specs/vmekj-s3-gc9d01-display-bringup/assets/{}.framebuffer.bin",
        scene.slug()
    ))
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let scene_slug = args
        .next()
        .unwrap_or_else(|| STARTUP_SCENE_SLUG.to_string());
    let Some(scene) = SceneId::from_slug(&scene_slug) else {
        eprintln!("unknown scene '{scene_slug}'");
        eprintln!(
            "known scenes: startup, solid-red, solid-green, solid-blue, checker-wide, checker-fine, shapes, lines, text, triangles, grid"
        );
        return ExitCode::FAILURE;
    };
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(scene));

    let mut canvas = DisplayCanvas::new();
    render_scene(scene, &mut canvas);

    let mut bytes = [0_u8; DISPLAY_FRAMEBUFFER_BYTES];
    canvas.write_panel_rgb565_be_bytes(&mut bytes);

    if let Some(parent) = output_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!(
            "failed to create output directory {}: {error}",
            parent.display()
        );
        return ExitCode::FAILURE;
    }

    if let Err(error) = fs::write(&output_path, bytes) {
        eprintln!("failed to write {}: {error}", output_path.display());
        return ExitCode::FAILURE;
    }

    println!(
        "wrote {} scene={} width={} height={} orientation=Landscape dx={} dy={} rgb565_endian=be layout=gc9d01-panel-order",
        output_path.display(),
        scene.slug(),
        DISPLAY_PHYSICAL_WIDTH,
        DISPLAY_PHYSICAL_HEIGHT,
        DISPLAY_PANEL_CONFIG.dx,
        DISPLAY_PANEL_CONFIG.dy,
    );
    ExitCode::SUCCESS
}
