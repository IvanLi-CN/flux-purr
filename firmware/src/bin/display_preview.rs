use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use flux_purr_firmware::display::{
    DISPLAY_FRAMEBUFFER_BYTES, DISPLAY_PANEL_CONFIG, DISPLAY_PHYSICAL_HEIGHT,
    DISPLAY_PHYSICAL_WIDTH, DisplayCanvas, STARTUP_SCENE_SLUG, SceneId, render_scene,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("firmware crate should live under the repo root")
        .to_path_buf()
}

fn default_output_path(scene: SceneId) -> PathBuf {
    repo_root().join(format!(
        "docs/specs/vmekj-s3-gc9d01-display-bringup/assets/{}.framebuffer.bin",
        scene.slug()
    ))
}

fn panel_output_path(logical_output_path: &Path, scene: SceneId) -> PathBuf {
    logical_output_path.with_file_name(format!("{}.panel.framebuffer.bin", scene.slug()))
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
    let panel_output_path = panel_output_path(&output_path, scene);

    let mut canvas = DisplayCanvas::new();
    render_scene(scene, &mut canvas);

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

    if let Err(error) = fs::write(&panel_output_path, panel_bytes) {
        eprintln!(
            "failed to write panel companion {}: {error}",
            panel_output_path.display()
        );
        return ExitCode::FAILURE;
    }

    println!(
        "wrote {} scene={} width=160 height=50 rgb565_endian=le; panel={} panel_width={} panel_height={} orientation=Landscape dx={} dy={} panel_rgb565_endian=be layout=gc9d01-panel-order",
        output_path.display(),
        scene.slug(),
        panel_output_path.display(),
        DISPLAY_PHYSICAL_WIDTH,
        DISPLAY_PHYSICAL_HEIGHT,
        DISPLAY_PANEL_CONFIG.dx,
        DISPLAY_PANEL_CONFIG.dy,
    );
    ExitCode::SUCCESS
}
