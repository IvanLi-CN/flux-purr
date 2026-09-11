use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use flux_purr_firmware::display::{
    DISPLAY_FRAMEBUFFER_BYTES, DISPLAY_PANEL_CONFIG, DISPLAY_PHYSICAL_HEIGHT,
    DISPLAY_PHYSICAL_WIDTH, DisplayCanvas, DisplayThemeId, STARTUP_SCENE_SLUG, SceneId,
    render_scene_with_theme,
};

#[derive(Debug, PartialEq, Eq)]
struct PreviewArgs {
    scene_slug: String,
    output: Option<PathBuf>,
    theme: DisplayThemeId,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("firmware crate should live under the repo root")
        .to_path_buf()
}

fn default_output_path(scene: SceneId) -> PathBuf {
    repo_root()
        .join("docs/specs/s3-gc9d01-display-bringup/assets")
        .join(format!("{}.framebuffer.bin", scene.slug()))
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

fn parse_cli_args<I>(args: I) -> Result<PreviewArgs, String>
where
    I: IntoIterator<Item = String>,
{
    let mut positional = Vec::new();
    let mut theme = DisplayThemeId::default();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        if argument == "--theme" {
            let value = args
                .next()
                .ok_or_else(|| String::from("missing value for --theme"))?;
            theme = DisplayThemeId::from_slug(&value)
                .ok_or_else(|| format!("unknown theme '{value}' (known: dark, light)"))?;
        } else if let Some(value) = argument.strip_prefix("--theme=") {
            theme = DisplayThemeId::from_slug(value)
                .ok_or_else(|| format!("unknown theme '{value}' (known: dark, light)"))?;
        } else if argument.starts_with('-') {
            return Err(format!("unknown argument '{argument}'"));
        } else {
            positional.push(argument);
        }
    }

    Ok(PreviewArgs {
        scene_slug: positional
            .first()
            .cloned()
            .unwrap_or_else(|| STARTUP_SCENE_SLUG.to_string()),
        output: positional.get(1).map(PathBuf::from),
        theme,
    })
}

fn main() -> ExitCode {
    let parsed = match parse_cli_args(env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let scene_slug = parsed.scene_slug;
    let Some(scene) = SceneId::from_slug(&scene_slug) else {
        eprintln!("unknown scene '{scene_slug}'");
        eprintln!(
            "known scenes: startup, startup-splash, startup-calibration, solid-red, solid-green, solid-blue, checker-wide, checker-fine, shapes, lines, text, triangles, grid, frontpanel-home, frontpanel-preferences-preset-temp, frontpanel-preferences-active-cooling, frontpanel-preferences-wifi-info, frontpanel-preferences-device-info, frontpanel-preset-temp, frontpanel-preset-temp-disabled, frontpanel-active-cooling, frontpanel-wifi-info, frontpanel-device-info"
        );
        return ExitCode::FAILURE;
    };
    let output_path = parsed.output.unwrap_or_else(|| default_output_path(scene));
    let panel_output_path = panel_output_path(&output_path);

    let mut canvas = DisplayCanvas::new();
    render_scene_with_theme(scene, &mut canvas, parsed.theme);

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
        "wrote {} scene={} theme={} width=160 height=50 rgb565_endian=le; panel={} panel_width={} panel_height={} orientation=Landscape dx={} dy={} panel_rgb565_endian=be layout=gc9d01-panel-order",
        output_path.display(),
        scene.slug(),
        parsed.theme.slug(),
        panel_output_path.display(),
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
    fn parse_cli_args_defaults_to_light_theme() {
        let parsed = parse_cli_args([String::from("startup-splash")]).expect("args should parse");

        assert_eq!(parsed.scene_slug, "startup-splash");
        assert_eq!(parsed.theme, DisplayThemeId::Light);
        assert_eq!(parsed.output, None);
    }

    #[test]
    fn parse_cli_args_accepts_dark_theme_after_output() {
        let parsed = parse_cli_args([
            String::from("startup-splash"),
            String::from("/tmp/startup-dark.framebuffer.bin"),
            String::from("--theme"),
            String::from("dark"),
        ])
        .expect("args should parse");

        assert_eq!(parsed.theme, DisplayThemeId::Dark);
        assert_eq!(
            parsed.output,
            Some(PathBuf::from("/tmp/startup-dark.framebuffer.bin"))
        );
    }

    #[test]
    fn panel_output_path_tracks_the_requested_filename() {
        assert_eq!(
            panel_output_path(Path::new("/tmp/startup.framebuffer.bin")),
            PathBuf::from("/tmp/startup.panel.framebuffer.bin")
        );
        assert_eq!(
            panel_output_path(Path::new("/tmp/custom.bin")),
            PathBuf::from("/tmp/custom.panel.bin")
        );
        assert_eq!(
            panel_output_path(Path::new("/tmp/custom")),
            PathBuf::from("/tmp/custom.panel")
        );
    }
}
