use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle, Triangle},
};

use crate::display::DisplayCanvas;

use super::{FrontPanelKey, FrontPanelMenuItem, FrontPanelRoute, FrontPanelUiState, KeyGesture};

const COLOR_BG: Rgb565 = Rgb565::new(1, 4, 3);
const COLOR_PANEL: Rgb565 = Rgb565::new(2, 8, 6);
const COLOR_PANEL_STRONG: Rgb565 = Rgb565::new(3, 10, 8);
const COLOR_BORDER: Rgb565 = Rgb565::new(5, 15, 11);
const COLOR_TEXT: Rgb565 = Rgb565::new(30, 62, 31);
const COLOR_MUTED: Rgb565 = Rgb565::new(17, 40, 25);
const COLOR_DISABLED: Rgb565 = Rgb565::new(11, 27, 17);
const COLOR_ACCENT: Rgb565 = Rgb565::new(31, 38, 7);
const COLOR_SUCCESS: Rgb565 = Rgb565::new(8, 54, 20);
const COLOR_WARNING: Rgb565 = Rgb565::new(31, 52, 11);
const COLOR_CYAN: Rgb565 = Rgb565::new(12, 54, 31);
const COLOR_TEMP_MINT: Rgb565 = Rgb565::new(10, 56, 24);
const COLOR_TEMP_LIME: Rgb565 = Rgb565::new(19, 55, 12);
const TEMPERATURE_THRESHOLDS_C: [i16; 6] = [0, 80, 150, 220, 300, 420];
const TEMPERATURE_COLORS: [Rgb565; 5] = [
    COLOR_CYAN,
    COLOR_TEMP_MINT,
    COLOR_TEMP_LIME,
    COLOR_WARNING,
    COLOR_ACCENT,
];

pub fn render_frontpanel_ui(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    canvas.clear(COLOR_BG).ok();

    match state.route {
        FrontPanelRoute::KeyTest => draw_key_test(canvas, state),
        FrontPanelRoute::Dashboard => draw_dashboard(canvas, state),
        FrontPanelRoute::Menu => draw_menu(canvas, state),
        FrontPanelRoute::PresetTemp => draw_preset_temp(canvas, state),
        FrontPanelRoute::ActiveCooling => draw_active_cooling(canvas, state),
        FrontPanelRoute::WifiInfo => draw_wifi_info(canvas),
        FrontPanelRoute::DeviceInfo => draw_device_info(canvas),
    }
}

fn fill_rect(canvas: &mut DisplayCanvas, x: i32, y: i32, width: u32, height: u32, color: Rgb565) {
    Rectangle::new(Point::new(x, y), Size::new(width, height))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(canvas)
        .ok();
}

#[derive(Clone, Copy)]
enum BitmapAlign {
    Left,
    Center,
    Right,
}

const BITMAP_FONT_WIDTH: i32 = 3;
const BITMAP_FONT_FALLBACK: [&str; 5] = ["111", "001", "011", "000", "010"];

fn bitmap_glyph(ch: char) -> &'static [&'static str; 5] {
    match ch.to_ascii_uppercase() {
        ' ' => &["000", "000", "000", "000", "000"],
        '.' => &["000", "000", "000", "000", "010"],
        '-' => &["000", "000", "111", "000", "000"],
        ':' => &["000", "010", "000", "010", "000"],
        '/' => &["001", "001", "010", "100", "100"],
        '%' => &["101", "001", "010", "100", "101"],
        '+' => &["000", "010", "111", "010", "000"],
        '=' => &["000", "111", "000", "111", "000"],
        '°' => &["010", "101", "010", "000", "000"],
        '0' => &["111", "101", "101", "101", "111"],
        '1' => &["010", "110", "010", "010", "111"],
        '2' => &["111", "001", "111", "100", "111"],
        '3' => &["111", "001", "111", "001", "111"],
        '4' => &["101", "101", "111", "001", "001"],
        '5' => &["111", "100", "111", "001", "111"],
        '6' => &["111", "100", "111", "101", "111"],
        '7' => &["111", "001", "001", "001", "001"],
        '8' => &["111", "101", "111", "101", "111"],
        '9' => &["111", "101", "111", "001", "111"],
        'A' => &["111", "101", "111", "101", "101"],
        'B' => &["110", "101", "110", "101", "110"],
        'C' => &["111", "100", "100", "100", "111"],
        'D' => &["110", "101", "101", "101", "110"],
        'E' => &["111", "100", "110", "100", "111"],
        'F' => &["111", "100", "110", "100", "100"],
        'G' => &["111", "100", "101", "101", "111"],
        'H' => &["101", "101", "111", "101", "101"],
        'I' => &["111", "010", "010", "010", "111"],
        'J' => &["001", "001", "001", "101", "111"],
        'K' => &["101", "101", "110", "101", "101"],
        'L' => &["100", "100", "100", "100", "111"],
        'M' => &["101", "111", "111", "101", "101"],
        'N' => &["101", "111", "111", "111", "101"],
        'O' => &["111", "101", "101", "101", "111"],
        'P' => &["110", "101", "110", "100", "100"],
        'Q' => &["111", "101", "101", "111", "001"],
        'R' => &["110", "101", "110", "101", "101"],
        'S' => &["111", "100", "111", "001", "111"],
        'T' => &["111", "010", "010", "010", "010"],
        'U' => &["101", "101", "101", "101", "111"],
        'V' => &["101", "101", "101", "101", "010"],
        'W' => &["101", "101", "111", "111", "101"],
        'X' => &["101", "101", "010", "101", "101"],
        'Y' => &["101", "101", "010", "010", "010"],
        'Z' => &["111", "001", "010", "100", "111"],
        _ => &BITMAP_FONT_FALLBACK,
    }
}

fn measure_bitmap_text(text: &str, scale: u32, letter_spacing: u32) -> i32 {
    let count = text.chars().count() as i32;
    if count == 0 {
        return 0;
    }
    let scale = scale as i32;
    let spacing = letter_spacing as i32;
    (count * BITMAP_FONT_WIDTH + (count - 1) * spacing) * scale
}

fn temperature_color(value_c: i16) -> Rgb565 {
    for index in 0..(TEMPERATURE_THRESHOLDS_C.len() - 1) {
        if value_c < TEMPERATURE_THRESHOLDS_C[index + 1] {
            return TEMPERATURE_COLORS[index];
        }
    }

    TEMPERATURE_COLORS[TEMPERATURE_COLORS.len() - 1]
}

#[allow(clippy::too_many_arguments)]
fn draw_bitmap_text(
    canvas: &mut DisplayCanvas,
    text: &str,
    x: i32,
    y: i32,
    color: Rgb565,
    scale: u32,
    letter_spacing: u32,
    align: BitmapAlign,
) {
    let width = measure_bitmap_text(text, scale, letter_spacing);
    let mut cursor_x = match align {
        BitmapAlign::Left => x,
        BitmapAlign::Center => x - (width / 2),
        BitmapAlign::Right => x - width,
    };
    let scale_i32 = scale as i32;
    let spacing = letter_spacing as i32;

    for ch in text.chars() {
        let glyph = bitmap_glyph(ch);
        for (row_index, row) in glyph.iter().enumerate() {
            for (column_index, pixel) in row.chars().enumerate() {
                if pixel != '1' {
                    continue;
                }
                fill_rect(
                    canvas,
                    cursor_x + column_index as i32 * scale_i32,
                    y + row_index as i32 * scale_i32,
                    scale,
                    scale,
                    color,
                );
            }
        }
        cursor_x += (BITMAP_FONT_WIDTH + spacing) * scale_i32;
    }
}

fn draw_text_small(canvas: &mut DisplayCanvas, text: &str, x: i32, y: i32, color: Rgb565) {
    draw_bitmap_text(canvas, text, x, y, color, 1, 1, BitmapAlign::Left);
}

fn draw_text_mid(canvas: &mut DisplayCanvas, text: &str, x: i32, y: i32, color: Rgb565) {
    draw_bitmap_text(canvas, text, x, y, color, 2, 1, BitmapAlign::Left);
}

fn draw_text_mid_center(canvas: &mut DisplayCanvas, text: &str, x: i32, y: i32, color: Rgb565) {
    draw_bitmap_text(canvas, text, x, y, color, 2, 1, BitmapAlign::Center);
}

fn draw_text_mid_right(canvas: &mut DisplayCanvas, text: &str, x: i32, y: i32, color: Rgb565) {
    draw_bitmap_text(canvas, text, x, y, color, 2, 1, BitmapAlign::Right);
}

fn draw_segment(
    canvas: &mut DisplayCanvas,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color: Rgb565,
) {
    fill_rect(canvas, x, y, width, height, color);
}

fn draw_seven_segment_digit(
    canvas: &mut DisplayCanvas,
    digit: char,
    x: i32,
    y: i32,
    color: Rgb565,
) {
    let thickness = 3;
    let width = 15;
    let height = 26;
    let mid_y = y + ((height - thickness) / 2);

    let segments = match digit {
        '-' => [false, false, false, false, false, false, true],
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => return,
    };

    let shapes = [
        (x + 2, y, width - 4, thickness),
        (x + width - thickness, y + 2, thickness, 9),
        (x + width - thickness, y + 15, thickness, 9),
        (x + 2, y + height - thickness, width - 4, thickness),
        (x, y + 15, thickness, 9),
        (x, y + 2, thickness, 9),
        (x + 2, mid_y, width - 4, thickness),
    ];

    for (active, (sx, sy, sw, sh)) in segments.iter().zip(shapes) {
        if *active {
            draw_segment(canvas, sx, sy, sw as u32, sh as u32, color);
        }
    }
}

fn draw_seven_segment_text(canvas: &mut DisplayCanvas, text: &str, x: i32, y: i32, color: Rgb565) {
    let mut cursor_x = x;
    for digit in text.chars() {
        draw_seven_segment_digit(canvas, digit, cursor_x, y, color);
        cursor_x += 17;
    }
}

fn measure_seven_segment_text(text: &str) -> i32 {
    let digits = text.chars().count() as i32;
    if digits == 0 { 0 } else { digits * 17 - 2 }
}

fn draw_status_line(canvas: &mut DisplayCanvas, y: i32, label: &str, value: &str, color: Rgb565) {
    draw_text_mid(canvas, label, 80, y, color);
    draw_text_mid_right(canvas, value, 154, y, color);
}

fn draw_bitmap_rows(canvas: &mut DisplayCanvas, rows: &[&str], x: i32, y: i32, color: Rgb565) {
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, pixel) in row.chars().enumerate() {
            if pixel != '1' {
                continue;
            }
            fill_rect(
                canvas,
                x + column_index as i32,
                y + row_index as i32,
                1,
                1,
                color,
            );
        }
    }
}

fn i16_to_text(value: i16) -> heapless::String<8> {
    use core::fmt::Write;

    let mut out = heapless::String::<8>::new();
    let _ = write!(&mut out, "{}", value);
    out
}

fn deci_c_to_parts(value_deci_c: i16) -> (heapless::String<8>, char) {
    use core::fmt::Write;

    let mut integer = heapless::String::<8>::new();
    let sign = if value_deci_c < 0 { "-" } else { "" };
    let magnitude = i32::from(value_deci_c).abs();
    let whole = magnitude / 10;
    let tenth = magnitude % 10;
    let _ = write!(&mut integer, "{}{}", sign, whole);
    let fractional = char::from(b'0' + u8::try_from(tenth).unwrap_or(0));
    (integer, fractional)
}

fn digit_char_text(ch: char) -> &'static str {
    match ch {
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        _ => "?",
    }
}

fn pd_voltage_content_text(contract_mv: u16) -> heapless::String<8> {
    use core::fmt::Write;

    let whole = contract_mv / 1000;
    let fractional = (contract_mv % 1000) / 10;

    let mut out = heapless::String::<8>::new();
    let _ = write!(&mut out, "{}.{:02}", whole, fractional);
    out
}

fn gesture_color(gesture: Option<KeyGesture>) -> Rgb565 {
    match gesture {
        Some(KeyGesture::ShortPress) => COLOR_SUCCESS,
        Some(KeyGesture::DoublePress) => COLOR_ACCENT,
        Some(KeyGesture::LongPress) => COLOR_CYAN,
        None => COLOR_TEXT,
    }
}

const MENU_ICON_PRESET_TEMP: [&str; 16] = [
    "0000000111000000",
    "0000001001000000",
    "0000001011000000",
    "0000001001000000",
    "0000001001000000",
    "0000001111000000",
    "0000001111000000",
    "0000001111000000",
    "0000001111000000",
    "0000110110110000",
    "0000101111010000",
    "0000101111010000",
    "0000101111010000",
    "0000100111010000",
    "0000010000100000",
    "0000001111000000",
];
const MENU_ICON_ACTIVE_COOLING: [&str; 16] = [
    "0000001111100000",
    "0000011111100000",
    "0000011111100000",
    "0000001111000000",
    "0000001110000000",
    "0110001000000110",
    "1111000110011111",
    "1111111111111111",
    "1111111111111111",
    "1111110110000111",
    "0110000001000110",
    "0000000111000000",
    "0000000111100000",
    "0000011111100000",
    "0000011111100000",
    "0000011111100000",
];
const MENU_ICON_WIFI: [&str; 16] = [
    "0000000000000000",
    "0000111111110000",
    "0001111111111000",
    "0111000000001110",
    "1110000000000111",
    "1100011111110011",
    "0001110000111000",
    "0011000000001100",
    "0000001111000000",
    "0000011111100000",
    "0000010000100000",
    "0000000000000000",
    "0000000110000000",
    "0000000110000000",
    "0000000000000000",
    "0000000000000000",
];
const MENU_ICON_DEVICE: [&str; 16] = [
    "0000000000000000",
    "0001001001001000",
    "0001001001001000",
    "0110000000000110",
    "0000111111110000",
    "0000111111110000",
    "0110110000110110",
    "0000110000110000",
    "0000110000110000",
    "0110110000110110",
    "0000111111110000",
    "0000111111110000",
    "0110000000000110",
    "0001001001001000",
    "0001001001001000",
    "0000000000000000",
];
const CELSIUS_UNIT_BITMAP: [&str; 11] = [
    "011000000000",
    "100100111111",
    "100101110000",
    "011011000000",
    "000011000000",
    "000011000000",
    "000011000000",
    "000011000000",
    "000011000000",
    "000001111000",
    "000000111111",
];

fn menu_icon_rows(item: FrontPanelMenuItem) -> &'static [&'static str] {
    match item {
        FrontPanelMenuItem::PresetTemp => &MENU_ICON_PRESET_TEMP,
        FrontPanelMenuItem::ActiveCooling => &MENU_ICON_ACTIVE_COOLING,
        FrontPanelMenuItem::WifiInfo => &MENU_ICON_WIFI,
        FrontPanelMenuItem::DeviceInfo => &MENU_ICON_DEVICE,
    }
}

fn menu_footer_title(item: FrontPanelMenuItem) -> &'static str {
    match item {
        FrontPanelMenuItem::PresetTemp => "TEMP SET",
        FrontPanelMenuItem::ActiveCooling => "A-COOL",
        FrontPanelMenuItem::WifiInfo => "WIFI",
        FrontPanelMenuItem::DeviceInfo => "DEVICE",
    }
}

fn shape_color_for(state: &FrontPanelUiState, key: FrontPanelKey) -> Rgb565 {
    if state.key_test.last_key == Some(key) {
        gesture_color(state.key_test.last_gesture)
    } else {
        COLOR_TEXT
    }
}

fn draw_key_test(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    fill_rect(canvas, 4, 4, 152, 42, COLOR_PANEL_STRONG);
    draw_text_mid(canvas, "KEY TEST", 8, 7, COLOR_TEXT);
    draw_text_small(canvas, "SHORT=SUCCESS", 84, 8, COLOR_SUCCESS);
    draw_text_small(canvas, "DOUBLE=ACCENT", 84, 15, COLOR_ACCENT);
    draw_text_small(canvas, "LONG=INFO", 84, 22, COLOR_CYAN);

    Triangle::new(Point::new(40, 8), Point::new(30, 20), Point::new(50, 20))
        .into_styled(PrimitiveStyle::with_fill(shape_color_for(
            state,
            FrontPanelKey::Up,
        )))
        .draw(canvas)
        .ok();
    Triangle::new(Point::new(40, 40), Point::new(30, 28), Point::new(50, 28))
        .into_styled(PrimitiveStyle::with_fill(shape_color_for(
            state,
            FrontPanelKey::Down,
        )))
        .draw(canvas)
        .ok();
    Triangle::new(Point::new(12, 24), Point::new(24, 14), Point::new(24, 34))
        .into_styled(PrimitiveStyle::with_fill(shape_color_for(
            state,
            FrontPanelKey::Left,
        )))
        .draw(canvas)
        .ok();
    Triangle::new(Point::new(68, 24), Point::new(56, 14), Point::new(56, 34))
        .into_styled(PrimitiveStyle::with_fill(shape_color_for(
            state,
            FrontPanelKey::Right,
        )))
        .draw(canvas)
        .ok();
    Circle::new(Point::new(30, 14), 20)
        .into_styled(PrimitiveStyle::with_fill(shape_color_for(
            state,
            FrontPanelKey::Center,
        )))
        .draw(canvas)
        .ok();
    draw_text_small(canvas, "U", 39, 15, COLOR_BG);
    draw_text_small(canvas, "D", 39, 34, COLOR_BG);
    draw_text_small(canvas, "L", 17, 24, COLOR_BG);
    draw_text_small(canvas, "R", 61, 24, COLOR_BG);
    draw_text_small(canvas, "OK", 34, 24, COLOR_BG);

    draw_text_small(
        canvas,
        state
            .key_test
            .last_raw_key
            .map(|key| key.short_label())
            .unwrap_or("---"),
        84,
        32,
        COLOR_WARNING,
    );
    draw_text_small(
        canvas,
        state
            .key_test
            .last_key
            .map(|key| key.short_label())
            .unwrap_or("---"),
        112,
        32,
        COLOR_TEXT,
    );
    draw_text_small(
        canvas,
        state
            .key_test
            .last_gesture
            .map(|gesture| gesture.label())
            .unwrap_or("IDLE"),
        134,
        32,
        gesture_color(state.key_test.last_gesture),
    );

    draw_text_small(
        canvas,
        match state.key_test.raw_state.pressed_mask() {
            0 => "MASK 00",
            mask => match mask {
                1 => "MASK 01",
                2 => "MASK 02",
                4 => "MASK 04",
                8 => "MASK 08",
                16 => "MASK 10",
                _ => "MASK ++",
            },
        },
        84,
        41,
        COLOR_MUTED,
    );
}

fn draw_dashboard(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    let (display_text, fractional_digit) = deci_c_to_parts(state.current_temp_deci_c);
    let value_color = temperature_color(state.current_temp_c);
    let set_text = i16_to_text(state.target_temp_c);
    let digits_width = measure_seven_segment_text(&display_text);
    let digits_right_edge = 57;
    let digits_x = digits_right_edge - digits_width;

    fill_rect(canvas, 4, 4, 72, 36, COLOR_PANEL_STRONG);
    draw_seven_segment_text(canvas, &display_text, digits_x, 8, value_color);
    draw_text_mid(canvas, digit_char_text(fractional_digit), 66, 8, COLOR_TEXT);
    fill_rect(canvas, 60, 16, 2, 2, COLOR_TEXT);
    draw_bitmap_rows(canvas, &CELSIUS_UNIT_BITMAP, 60, 24, COLOR_TEXT);

    fill_rect(canvas, 78, 4, 78, 36, COLOR_PANEL);
    if state.heater_lock_reason.is_some() && state.dashboard_warning_visible {
        draw_status_line(canvas, 7, "WARN", "OTEMP", COLOR_WARNING);
    } else {
        draw_status_line(canvas, 7, "SET", &set_text, COLOR_WARNING);
    }
    draw_text_mid(canvas, "PPS", 80, 18, COLOR_CYAN);
    let pps_numeric = pd_voltage_content_text(state.pd_contract_mv);
    draw_text_mid_right(canvas, &pps_numeric, 147, 18, COLOR_CYAN);
    draw_text_mid_right(canvas, "V", 154, 18, COLOR_CYAN);
    draw_status_line(
        canvas,
        29,
        "FAN",
        state.fan_display_state.label(),
        match state.fan_display_state {
            super::FanDisplayState::Off => COLOR_DISABLED,
            super::FanDisplayState::Auto => COLOR_CYAN,
            super::FanDisplayState::Run => COLOR_SUCCESS,
        },
    );

    fill_rect(canvas, 4, 42, 152, 5, COLOR_PANEL);
    let heater_fill_width = heater_bar_fill_width(state.heater_output_percent);
    if heater_fill_width > 0 {
        fill_rect(canvas, 6, 43, heater_fill_width, 3, COLOR_ACCENT);
    }
}

fn heater_bar_fill_width(output_percent: u8) -> u32 {
    let output_percent = u32::from(output_percent.min(100));
    if output_percent == 0 {
        return 0;
    }

    (13 + ((output_percent * 148) / 100)).min(148)
}

fn draw_menu(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    fill_rect(canvas, 4, 4, 152, 24, COLOR_PANEL_STRONG);
    fill_rect(canvas, 4, 30, 152, 16, COLOR_PANEL);

    for (index, item) in FrontPanelMenuItem::ALL.iter().enumerate() {
        let x = 6 + index as i32 * 38;
        if index > 0 {
            fill_rect(canvas, x - 2, 8, 1, 16, COLOR_BORDER);
        }
        if *item == state.selected_menu_item {
            fill_rect(canvas, x + 4, 6, 26, 20, COLOR_ACCENT);
        }
        draw_bitmap_rows(
            canvas,
            menu_icon_rows(*item),
            x + 9,
            8,
            if *item == state.selected_menu_item {
                COLOR_BG
            } else {
                COLOR_TEXT
            },
        );
    }
    draw_bitmap_rows(
        canvas,
        menu_icon_rows(state.selected_menu_item),
        8,
        30,
        COLOR_WARNING,
    );
    draw_text_mid_center(
        canvas,
        menu_footer_title(state.selected_menu_item),
        80,
        34,
        COLOR_TEXT,
    );
}

fn draw_preset_temp(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    const SLOT_LABELS: [&str; 10] = ["M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9", "M10"];

    for (index, label) in SLOT_LABELS.iter().enumerate().take(state.presets_c.len()) {
        let color = if index == state.selected_preset_slot {
            COLOR_ACCENT
        } else if state.presets_c[index].is_some() {
            COLOR_TEXT
        } else {
            COLOR_DISABLED
        };
        let x = 2 + index as i32 * 16;
        draw_text_small(canvas, label, x, 2, color);
    }

    let value = state.selected_preset().map(i16_to_text);
    let display = value.as_deref().unwrap_or("---");
    let digit_color = state
        .selected_preset()
        .map(temperature_color)
        .unwrap_or(COLOR_DISABLED);
    let unit_color = if state.selected_preset().is_some() {
        COLOR_TEXT
    } else {
        COLOR_DISABLED
    };
    let digits_width = if display.is_empty() {
        0
    } else {
        display.chars().count() as i32 * 17 - 2
    };
    let digits_x = ((160 - (digits_width + 3 + CELSIUS_UNIT_BITMAP[0].len() as i32)) / 2).max(0);
    draw_seven_segment_text(canvas, display, digits_x, 18, digit_color);
    draw_bitmap_rows(
        canvas,
        &CELSIUS_UNIT_BITMAP,
        digits_x + digits_width + 3,
        33,
        unit_color,
    );
}

fn draw_active_cooling(canvas: &mut DisplayCanvas, state: &FrontPanelUiState) {
    draw_text_mid(canvas, "A-COOL", 8, 6, COLOR_TEXT);
    draw_text_mid_right(
        canvas,
        if state.active_cooling_enabled {
            "ON"
        } else {
            "OFF"
        },
        152,
        6,
        if state.active_cooling_enabled {
            COLOR_SUCCESS
        } else {
            COLOR_WARNING
        },
    );
    draw_text_small(canvas, "PD 12V | AUTO <35 OFF >40 MIN", 8, 22, COLOR_CYAN);
    draw_text_small(
        canvas,
        "SAFE >100 PLS >350 50% >360 MAX",
        8,
        34,
        COLOR_WARNING,
    );
}

fn draw_wifi_info(canvas: &mut DisplayCanvas) {
    draw_text_mid(canvas, "SSID FLUXLAB", 8, 6, COLOR_TEXT);
    draw_text_mid(canvas, "RSSI -58DBM", 8, 19, COLOR_CYAN);
    draw_text_mid(canvas, "IP 192.168.4.1", 8, 32, COLOR_WARNING);
}

fn draw_device_info(canvas: &mut DisplayCanvas) {
    draw_text_mid(canvas, "BOARD FP-S3", 8, 6, COLOR_TEXT);
    draw_text_mid(canvas, "FW V0.3.0", 8, 19, COLOR_WARNING);
    draw_text_mid(canvas, "ID S3-001", 8, 32, COLOR_CYAN);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_color_follows_threshold_bands() {
        assert_eq!(temperature_color(-5), COLOR_CYAN);
        assert_eq!(temperature_color(79), COLOR_CYAN);
        assert_eq!(temperature_color(80), COLOR_TEMP_MINT);
        assert_eq!(temperature_color(149), COLOR_TEMP_MINT);
        assert_eq!(temperature_color(150), COLOR_TEMP_LIME);
        assert_eq!(temperature_color(219), COLOR_TEMP_LIME);
        assert_eq!(temperature_color(220), COLOR_WARNING);
        assert_eq!(temperature_color(299), COLOR_WARNING);
        assert_eq!(temperature_color(300), COLOR_ACCENT);
        assert_eq!(temperature_color(450), COLOR_ACCENT);
    }

    #[test]
    fn heater_bar_fill_width_tracks_output_percent() {
        assert_eq!(heater_bar_fill_width(0), 0);
        assert_eq!(heater_bar_fill_width(25), 50);
        assert_eq!(heater_bar_fill_width(50), 87);
        assert_eq!(heater_bar_fill_width(64), 107);
        assert_eq!(heater_bar_fill_width(100), 148);
        assert_eq!(heater_bar_fill_width(255), 148);
    }

    #[test]
    fn deci_temperature_parts_keep_one_decimal() {
        assert_eq!(deci_c_to_parts(263), ("26".try_into().unwrap(), '3'));
        assert_eq!(deci_c_to_parts(3000), ("300".try_into().unwrap(), '0'));
        assert_eq!(deci_c_to_parts(-52), ("-5".try_into().unwrap(), '2'));
    }

    #[test]
    fn seven_segment_measurement_matches_glyph_advance() {
        assert_eq!(measure_seven_segment_text(""), 0);
        assert_eq!(measure_seven_segment_text("8"), 15);
        assert_eq!(measure_seven_segment_text("26"), 32);
        assert_eq!(measure_seven_segment_text("300"), 49);
    }

    #[test]
    fn pd_voltage_content_text_omits_unit_for_split_layout() {
        assert_eq!(pd_voltage_content_text(12_000), "12.00");
        assert_eq!(pd_voltage_content_text(20_000), "20.00");
        assert_eq!(pd_voltage_content_text(20_080), "20.08");
    }
}
