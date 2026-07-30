use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
};

use flux_purr_firmware::status_light::{RgbChannels, StatusLightState, status_light_output};

const WIDTH: usize = 960;
const HEIGHT: usize = 760;
const BACKGROUND: [u8; 3] = [15, 18, 20];
const PANEL: [u8; 3] = [28, 33, 37];
const BORDER: [u8; 3] = [62, 71, 78];
const TEXT: [u8; 3] = [220, 228, 232];
const OFF: [u8; 3] = [43, 49, 54];

struct PreviewState {
    label: &'static str,
    state: StatusLightState,
}

const PREVIEW_STATES: [PreviewState; 10] = [
    PreviewState {
        label: "BOOT",
        state: StatusLightState::Booting,
    },
    PreviewState {
        label: "READY",
        state: StatusLightState::Ready,
    },
    PreviewState {
        label: "HEAT",
        state: StatusLightState::Heating,
    },
    PreviewState {
        label: "COOL",
        state: StatusLightState::Cooling,
    },
    PreviewState {
        label: "CAL",
        state: StatusLightState::Calibration,
    },
    PreviewState {
        label: "LOCK",
        state: StatusLightState::HeaterInterlocked,
    },
    PreviewState {
        label: "TRIP",
        state: StatusLightState::CoolingDisabledOvertemp,
    },
    PreviewState {
        label: "SENSOR",
        state: StatusLightState::SensorFault,
    },
    PreviewState {
        label: "PEND",
        state: StatusLightState::ThermalRunawayAttentionPending,
    },
    PreviewState {
        label: "RUN",
        state: StatusLightState::ThermalRunaway,
    },
];

fn main() -> io::Result<()> {
    let output_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("status-light-language.ppm"));
    if let Some(parent) = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let mut canvas = Canvas::new(WIDTH, HEIGHT, BACKGROUND);
    for (index, preview) in PREVIEW_STATES.iter().enumerate() {
        draw_state_row(&mut canvas, index, preview);
    }

    let mut file = File::create(&output_path)?;
    write!(file, "P6\n{} {}\n255\n", WIDTH, HEIGHT)?;
    file.write_all(&canvas.pixels)?;
    println!("wrote {}", output_path.display());
    Ok(())
}

fn draw_state_row(canvas: &mut Canvas, index: usize, preview: &PreviewState) {
    const ROW_HEIGHT: i32 = 66;
    const ROW_START_Y: i32 = 40;
    const SAMPLE_STEP_MS: u64 = 140;

    let y = ROW_START_Y + (index as i32 * ROW_HEIGHT);
    canvas.fill_rect(18, y, 924, 52, PANEL);
    canvas.stroke_rect(18, y, 924, 52, BORDER);
    canvas.draw_text(preview.label, 34, y + 18, 3, TEXT);

    for sample in 0..10 {
        let elapsed_ms = sample as u64 * SAMPLE_STEP_MS;
        let color = channels_to_rgb(status_light_output(preview.state, elapsed_ms));
        let x = 235 + (sample * 68);
        canvas.circle(x, y + 26, 15, color);
        canvas.circle_outline(x, y + 26, 17, BORDER);
    }
}

fn channels_to_rgb(channels: RgbChannels) -> [u8; 3] {
    if !channels.red && !channels.green && !channels.blue {
        return OFF;
    }

    [
        if channels.red { 244 } else { 0 },
        if channels.green { 220 } else { 0 },
        if channels.blue { 255 } else { 0 },
    ]
}

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize, color: [u8; 3]) -> Self {
        let mut pixels = vec![0; width * height * 3];
        for pixel in pixels.chunks_exact_mut(3) {
            pixel.copy_from_slice(&color);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let offset = (y as usize * self.width + x as usize) * 3;
        self.pixels[offset..offset + 3].copy_from_slice(&color);
    }

    fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: [u8; 3]) {
        for row in y..y + height {
            for column in x..x + width {
                self.set_pixel(column, row, color);
            }
        }
    }

    fn stroke_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: [u8; 3]) {
        self.fill_rect(x, y, width, 1, color);
        self.fill_rect(x, y + height - 1, width, 1, color);
        self.fill_rect(x, y, 1, height, color);
        self.fill_rect(x + width - 1, y, 1, height, color);
    }

    fn circle(&mut self, center_x: i32, center_y: i32, radius: i32, color: [u8; 3]) {
        let radius_squared = radius * radius;
        for y in center_y - radius..=center_y + radius {
            for x in center_x - radius..=center_x + radius {
                let dx = x - center_x;
                let dy = y - center_y;
                if dx * dx + dy * dy <= radius_squared {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    fn circle_outline(&mut self, center_x: i32, center_y: i32, radius: i32, color: [u8; 3]) {
        let outer = radius * radius;
        let inner = (radius - 2) * (radius - 2);
        for y in center_y - radius..=center_y + radius {
            for x in center_x - radius..=center_x + radius {
                let dx = x - center_x;
                let dy = y - center_y;
                let distance_squared = dx * dx + dy * dy;
                if distance_squared <= outer && distance_squared >= inner {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    fn draw_text(&mut self, text: &str, x: i32, y: i32, scale: i32, color: [u8; 3]) {
        let mut cursor_x = x;
        for byte in text.bytes() {
            let glyph = glyph(byte);
            for (row, bits) in glyph.iter().enumerate() {
                for column in 0..5 {
                    if bits & (1 << (4 - column)) != 0 {
                        self.fill_rect(
                            cursor_x + (column * scale),
                            y + (row as i32 * scale),
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
            cursor_x += 6 * scale;
        }
    }
}

fn glyph(byte: u8) -> [u8; 7] {
    match byte {
        b'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        b'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        b'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        b'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        b'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        b'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        b'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        b'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        b'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        b'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        b'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        b'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        b'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        b'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        b'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        _ => [0; 7],
    }
}
