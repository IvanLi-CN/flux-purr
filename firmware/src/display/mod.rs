use core::convert::Infallible;

use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_4X6, FONT_5X8},
    },
    pixelcolor::{Rgb565, raw::RawU16},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle, Triangle},
    text::{Alignment, Text},
};
use gc9d01::{Config as PanelConfig, Orientation};

pub const DISPLAY_WIDTH: u16 = 160;
pub const DISPLAY_HEIGHT: u16 = 50;
pub const DISPLAY_WIDTH_USIZE: usize = DISPLAY_WIDTH as usize;
pub const DISPLAY_HEIGHT_USIZE: usize = DISPLAY_HEIGHT as usize;
pub const DISPLAY_PIXELS: usize = DISPLAY_WIDTH_USIZE * DISPLAY_HEIGHT_USIZE;
pub const DISPLAY_FRAMEBUFFER_BYTES: usize = DISPLAY_PIXELS * 2;

pub const DISPLAY_PANEL_CONFIG: PanelConfig = PanelConfig {
    width: DISPLAY_WIDTH,
    height: DISPLAY_HEIGHT,
    orientation: Orientation::Landscape,
    rgb: false,
    inverted: false,
    dx: 15,
    dy: 0,
};

pub const DEVICE_BOOT_FLOW: DeviceBootFlow = DeviceBootFlow::CalibrationThenDemoThenHold;
pub const STARTUP_SCENE_SLUG: &str = "startup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceBootFlow {
    CalibrationOnly,
    CalibrationThenDemoThenHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneId {
    StartupCalibration,
    DemoSolidRed,
    DemoSolidGreen,
    DemoSolidBlue,
    DemoCheckerWide,
    DemoCheckerFine,
    DemoShapes,
    DemoLines,
    DemoText,
    DemoTriangles,
    DemoGrid,
}

impl SceneId {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::StartupCalibration => "startup",
            Self::DemoSolidRed => "demo-solid-red",
            Self::DemoSolidGreen => "demo-solid-green",
            Self::DemoSolidBlue => "demo-solid-blue",
            Self::DemoCheckerWide => "demo-checker-wide",
            Self::DemoCheckerFine => "demo-checker-fine",
            Self::DemoShapes => "demo-shapes",
            Self::DemoLines => "demo-lines",
            Self::DemoText => "demo-text",
            Self::DemoTriangles => "demo-triangles",
            Self::DemoGrid => "demo-grid",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::StartupCalibration => "startup calibration",
            Self::DemoSolidRed => "solid red",
            Self::DemoSolidGreen => "solid green",
            Self::DemoSolidBlue => "solid blue",
            Self::DemoCheckerWide => "wide checker",
            Self::DemoCheckerFine => "fine checker",
            Self::DemoShapes => "shapes",
            Self::DemoLines => "lines",
            Self::DemoText => "text",
            Self::DemoTriangles => "triangles",
            Self::DemoGrid => "grid",
        }
    }

    pub const fn dwell_millis(self) -> u64 {
        match self {
            Self::StartupCalibration => 0,
            Self::DemoSolidRed | Self::DemoSolidGreen | Self::DemoSolidBlue => 450,
            Self::DemoCheckerWide | Self::DemoCheckerFine => 550,
            Self::DemoShapes
            | Self::DemoLines
            | Self::DemoText
            | Self::DemoTriangles
            | Self::DemoGrid => 700,
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "startup" | "startup-calibration" => Some(Self::StartupCalibration),
            "demo-solid-red" | "solid-red" => Some(Self::DemoSolidRed),
            "demo-solid-green" | "solid-green" => Some(Self::DemoSolidGreen),
            "demo-solid-blue" | "solid-blue" => Some(Self::DemoSolidBlue),
            "demo-checker-wide" | "checker-wide" => Some(Self::DemoCheckerWide),
            "demo-checker-fine" | "checker-fine" => Some(Self::DemoCheckerFine),
            "demo-shapes" | "shapes" => Some(Self::DemoShapes),
            "demo-lines" | "lines" => Some(Self::DemoLines),
            "demo-text" | "text" => Some(Self::DemoText),
            "demo-triangles" | "triangles" => Some(Self::DemoTriangles),
            "demo-grid" | "grid" => Some(Self::DemoGrid),
            _ => None,
        }
    }
}

pub const DEMO_SEQUENCE: [SceneId; 10] = [
    SceneId::DemoSolidRed,
    SceneId::DemoSolidGreen,
    SceneId::DemoSolidBlue,
    SceneId::DemoCheckerWide,
    SceneId::DemoCheckerFine,
    SceneId::DemoShapes,
    SceneId::DemoLines,
    SceneId::DemoText,
    SceneId::DemoTriangles,
    SceneId::DemoGrid,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayCanvas {
    pixels: [Rgb565; DISPLAY_PIXELS],
}

impl Default for DisplayCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayCanvas {
    pub const fn new() -> Self {
        Self {
            pixels: [Rgb565::BLACK; DISPLAY_PIXELS],
        }
    }

    pub fn pixels(&self) -> &[Rgb565] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [Rgb565] {
        &mut self.pixels
    }

    pub fn write_rgb565_le_bytes(&self, out: &mut [u8; DISPLAY_FRAMEBUFFER_BYTES]) {
        for (index, pixel) in self.pixels.iter().copied().enumerate() {
            let raw: RawU16 = pixel.into();
            let bytes = raw.into_inner().to_le_bytes();
            let base = index * 2;
            out[base] = bytes[0];
            out[base + 1] = bytes[1];
        }
    }
}

impl OriginDimensions for DisplayCanvas {
    fn size(&self) -> Size {
        Size::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
    }
}

impl DrawTarget for DisplayCanvas {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }
            let x = point.x as usize;
            let y = point.y as usize;
            if x >= DISPLAY_WIDTH_USIZE || y >= DISPLAY_HEIGHT_USIZE {
                continue;
            }
            self.pixels[y * DISPLAY_WIDTH_USIZE + x] = color;
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.pixels.fill(color);
        Ok(())
    }
}

pub fn render_scene(scene: SceneId, canvas: &mut DisplayCanvas) {
    canvas.clear(Rgb565::BLACK).ok();

    match scene {
        SceneId::StartupCalibration => render_startup_calibration(canvas),
        SceneId::DemoSolidRed => {
            canvas.clear(Rgb565::RED).ok();
        }
        SceneId::DemoSolidGreen => {
            canvas.clear(Rgb565::GREEN).ok();
        }
        SceneId::DemoSolidBlue => {
            canvas.clear(Rgb565::BLUE).ok();
        }
        SceneId::DemoCheckerWide => render_checker(canvas, 20, 16),
        SceneId::DemoCheckerFine => render_checker(canvas, 10, 10),
        SceneId::DemoShapes => render_shapes(canvas),
        SceneId::DemoLines => render_lines(canvas),
        SceneId::DemoText => render_text(canvas),
        SceneId::DemoTriangles => render_triangles(canvas),
        SceneId::DemoGrid => render_grid(canvas),
    }
}

fn render_startup_calibration(canvas: &mut DisplayCanvas) {
    let border = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
    let text_small = MonoTextStyle::new(&FONT_4X6, Rgb565::WHITE);
    let text_small_black = MonoTextStyle::new(&FONT_4X6, Rgb565::BLACK);
    let text_mid = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);

    Rectangle::new(
        Point::new(0, 0),
        Size::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32),
    )
    .into_styled(border)
    .draw(canvas)
    .ok();

    Rectangle::new(Point::new(2, 2), Size::new(14, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
        .draw(canvas)
        .ok();
    Rectangle::new(Point::new(144, 2), Size::new(14, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        .draw(canvas)
        .ok();
    Rectangle::new(Point::new(2, 40), Size::new(14, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE))
        .draw(canvas)
        .ok();
    Rectangle::new(Point::new(144, 40), Size::new(14, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(canvas)
        .ok();

    Text::with_alignment("TL", Point::new(9, 8), text_small_black, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment(
        "TR",
        Point::new(151, 8),
        text_small_black,
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
    Text::with_alignment("BL", Point::new(9, 46), text_small, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment(
        "BR",
        Point::new(151, 46),
        text_small_black,
        Alignment::Center,
    )
    .draw(canvas)
    .ok();

    Triangle::new(Point::new(80, 2), Point::new(72, 11), Point::new(88, 11))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::MAGENTA))
        .draw(canvas)
        .ok();
    Text::with_alignment("UP", Point::new(80, 18), text_small, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment("L", Point::new(22, 18), text_small, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment("R", Point::new(138, 18), text_small, Alignment::Center)
        .draw(canvas)
        .ok();

    draw_palette_row(
        canvas,
        0,
        20,
        12,
        &[
            Rgb565::RED,
            Rgb565::GREEN,
            Rgb565::BLUE,
            Rgb565::YELLOW,
            Rgb565::CYAN,
            Rgb565::MAGENTA,
            Rgb565::WHITE,
            Rgb565::BLACK,
        ],
    );
    draw_palette_row(
        canvas,
        0,
        32,
        7,
        &[
            Rgb565::new(0x00, 0x00, 0x00),
            Rgb565::new(0x04, 0x08, 0x04),
            Rgb565::new(0x08, 0x10, 0x08),
            Rgb565::new(0x0c, 0x18, 0x0c),
            Rgb565::new(0x10, 0x20, 0x10),
            Rgb565::new(0x14, 0x28, 0x14),
            Rgb565::new(0x18, 0x30, 0x18),
            Rgb565::WHITE,
        ],
    );

    Text::with_alignment(
        "GC9D01 160x50 DX15",
        Point::new(80, 49),
        text_mid,
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
}

fn draw_palette_row(canvas: &mut DisplayCanvas, x: i32, y: i32, height: u32, colors: &[Rgb565; 8]) {
    for (index, color) in colors.iter().copied().enumerate() {
        let block_x = x + (index as i32 * 20);
        Rectangle::new(Point::new(block_x, y), Size::new(20, height))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(canvas)
            .ok();
        Rectangle::new(Point::new(block_x, y), Size::new(20, height))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(canvas)
            .ok();
    }
}

fn render_checker(canvas: &mut DisplayCanvas, block_w: u16, block_h: u16) {
    const COLORS: [Rgb565; 8] = [
        Rgb565::RED,
        Rgb565::GREEN,
        Rgb565::BLUE,
        Rgb565::YELLOW,
        Rgb565::MAGENTA,
        Rgb565::CYAN,
        Rgb565::WHITE,
        Rgb565::BLACK,
    ];

    let step_y = block_h as usize;
    let step_x = block_w as usize;
    for row in (0..DISPLAY_HEIGHT_USIZE).step_by(step_y) {
        for col in (0..DISPLAY_WIDTH_USIZE).step_by(step_x) {
            let color = COLORS[((row / step_y) + (col / step_x)) % COLORS.len()];
            let width = (DISPLAY_WIDTH_USIZE - col).min(step_x) as u32;
            let height = (DISPLAY_HEIGHT_USIZE - row).min(step_y) as u32;
            Rectangle::new(Point::new(col as i32, row as i32), Size::new(width, height))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(canvas)
                .ok();
        }
    }
}

fn render_shapes(canvas: &mut DisplayCanvas) {
    Rectangle::new(Point::new(8, 8), Size::new(28, 16))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::RED, 1))
        .draw(canvas)
        .ok();
    Rectangle::new(Point::new(44, 10), Size::new(24, 14))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        .draw(canvas)
        .ok();
    Circle::new(Point::new(82, 8), 18)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::BLUE, 2))
        .draw(canvas)
        .ok();
    Circle::new(Point::new(118, 10), 12)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(canvas)
        .ok();
    Text::with_alignment(
        "SHAPES",
        Point::new(80, 44),
        MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE),
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
}

fn render_lines(canvas: &mut DisplayCanvas) {
    for index in 0..8 {
        let color = match index % 3 {
            0 => Rgb565::RED,
            1 => Rgb565::GREEN,
            _ => Rgb565::BLUE,
        };
        Line::new(
            Point::new(index * 20, 0),
            Point::new(index * 20 + 20, DISPLAY_HEIGHT as i32 - 1),
        )
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(canvas)
        .ok();
    }
}

fn render_text(canvas: &mut DisplayCanvas) {
    let title = MonoTextStyle::new(&FONT_5X8, Rgb565::WHITE);
    let detail = MonoTextStyle::new(&FONT_4X6, Rgb565::CYAN);

    Text::with_alignment("GC9D01", Point::new(80, 10), title, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment("async spi", Point::new(80, 22), detail, Alignment::Center)
        .draw(canvas)
        .ok();
    Text::with_alignment(
        "embassy + eg",
        Point::new(80, 32),
        detail,
        Alignment::Center,
    )
    .draw(canvas)
    .ok();
    Text::with_alignment("flux-purr", Point::new(80, 42), detail, Alignment::Center)
        .draw(canvas)
        .ok();
}

fn render_triangles(canvas: &mut DisplayCanvas) {
    Triangle::new(Point::new(22, 6), Point::new(8, 38), Point::new(36, 38))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::MAGENTA))
        .draw(canvas)
        .ok();
    Triangle::new(Point::new(80, 8), Point::new(58, 40), Point::new(102, 40))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(canvas)
        .ok();
    Triangle::new(
        Point::new(132, 10),
        Point::new(114, 40),
        Point::new(150, 40),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
    .draw(canvas)
    .ok();
}

fn render_grid(canvas: &mut DisplayCanvas) {
    for row in (0..DISPLAY_HEIGHT_USIZE).step_by(10) {
        for col in (0..DISPLAY_WIDTH_USIZE).step_by(10) {
            let color = if ((row / 10) + (col / 10)) % 2 == 0 {
                Rgb565::WHITE
            } else {
                Rgb565::BLACK
            };
            Rectangle::new(Point::new(col as i32, row as i32), Size::new(10, 10))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(canvas)
                .ok();
            Rectangle::new(Point::new(col as i32, row as i32), Size::new(10, 10))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::RED, 1))
                .draw(canvas)
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_scene_renders_corner_markers_and_text() {
        let mut canvas = DisplayCanvas::new();
        render_scene(SceneId::StartupCalibration, &mut canvas);

        assert_eq!(canvas.pixels()[2 * DISPLAY_WIDTH_USIZE + 2], Rgb565::RED);
        assert_eq!(
            canvas.pixels()[2 * DISPLAY_WIDTH_USIZE + 144],
            Rgb565::GREEN
        );
        assert_eq!(canvas.pixels()[42 * DISPLAY_WIDTH_USIZE + 2], Rgb565::BLUE);
        assert_eq!(
            canvas.pixels()[42 * DISPLAY_WIDTH_USIZE + 144],
            Rgb565::YELLOW
        );
    }

    #[test]
    fn startup_scene_can_be_serialized_as_rgb565_le() {
        let mut canvas = DisplayCanvas::new();
        render_scene(SceneId::StartupCalibration, &mut canvas);
        let mut bytes = [0_u8; DISPLAY_FRAMEBUFFER_BYTES];
        canvas.write_rgb565_le_bytes(&mut bytes);
        assert_eq!(bytes.len(), DISPLAY_FRAMEBUFFER_BYTES);
        assert!(bytes.iter().any(|byte| *byte != 0));
    }

    #[test]
    fn demo_sequence_does_not_include_startup_scene() {
        assert!(!DEMO_SEQUENCE.contains(&SceneId::StartupCalibration));
        assert_eq!(DEMO_SEQUENCE[0], SceneId::DemoSolidRed);
        assert_eq!(DEMO_SEQUENCE[DEMO_SEQUENCE.len() - 1], SceneId::DemoGrid);
    }

    #[test]
    fn scene_slug_lookup_handles_aliases() {
        assert_eq!(
            SceneId::from_slug("startup"),
            Some(SceneId::StartupCalibration)
        );
        assert_eq!(SceneId::from_slug("grid"), Some(SceneId::DemoGrid));
        assert_eq!(SceneId::from_slug("missing"), None);
    }
}
