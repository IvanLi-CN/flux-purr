#![cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]

use heapless::String;
use serde::Deserialize;

pub const PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const FRAMING: &str = "jsonl";
pub const FIRMWARE_KIND: &str = "ram_bringup";
pub const REQUEST_ID_MAX: usize = 48;
pub const OP_MAX: usize = 32;
pub const CAPABILITY_MAX: usize = 32;
pub const DISPLAY_COLOR_MAX: usize = 16;
pub const CAPABILITIES: [&str; 10] = [
    "preview_display",
    "preview_frontpanel",
    "preview_status_light",
    "test_buttons",
    "test_adc",
    "test_i2c",
    "test_rgb",
    "test_buzzer",
    "test_fan",
    "exit",
];
pub const I2C_READ_ALLOWLIST: [(u8, u8); 1] = [(0x22, 0x09)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    PreviewDisplay,
    PreviewFrontpanel,
    PreviewStatusLight,
    TestButtons,
    TestAdc,
    TestI2c,
    TestRgb,
    TestBuzzer,
    TestFan,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPattern {
    Calibration,
    Solid([u8; 2]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseData {
    None,
    Effect(&'static str),
    Buttons {
        center: bool,
        right: bool,
        down: bool,
        left: bool,
        up: bool,
    },
    Adc {
        vin: u16,
        rtd: u16,
    },
    I2c {
        address: u8,
        register: u8,
        value: u8,
    },
    Fan {
        vin_raw: u16,
        vin_adc_mv: u16,
        input_mv: u32,
        minimum_mv: u16,
        measured: bool,
        voltage_ok: bool,
    },
}

impl DisplayPattern {
    pub fn parse(color: Option<&str>) -> Option<Self> {
        match color {
            None | Some("calibration") => Some(Self::Calibration),
            Some("red") => Some(Self::Solid([0xf8, 0x00])),
            Some("green") => Some(Self::Solid([0x07, 0xe0])),
            Some("blue") => Some(Self::Solid([0x00, 0x1f])),
            Some("white") => Some(Self::Solid([0xff, 0xff])),
            Some("black") => Some(Self::Solid([0x00, 0x00])),
            Some("yellow") => Some(Self::Solid([0xff, 0xe0])),
            Some("cyan") => Some(Self::Solid([0x07, 0xff])),
            Some("magenta") => Some(Self::Solid([0xf8, 0x1f])),
            Some(_) => None,
        }
    }
}

impl Command {
    pub fn parse(op: &str) -> Option<Self> {
        let op = op.as_bytes();
        if equal_literal(op, b"preview_display") {
            Some(Self::PreviewDisplay)
        } else if equal_literal(op, b"preview_frontpanel") {
            Some(Self::PreviewFrontpanel)
        } else if equal_literal(op, b"preview_status_light") {
            Some(Self::PreviewStatusLight)
        } else if equal_literal(op, b"test_buttons") {
            Some(Self::TestButtons)
        } else if equal_literal(op, b"test_adc") {
            Some(Self::TestAdc)
        } else if equal_literal(op, b"test_i2c") {
            Some(Self::TestI2c)
        } else if equal_literal(op, b"test_rgb") {
            Some(Self::TestRgb)
        } else if equal_literal(op, b"test_buzzer") {
            Some(Self::TestBuzzer)
        } else if equal_literal(op, b"test_fan") {
            Some(Self::TestFan)
        } else if equal_literal(op, b"exit") {
            Some(Self::Exit)
        } else {
            None
        }
    }
}

#[inline(never)]
pub fn equal_literal(value: &[u8], expected: &[u8]) -> bool {
    if value.len() != expected.len() {
        return false;
    }
    let mut index = 0;
    while index < expected.len() {
        if value[index] != expected[index] {
            return false;
        }
        index += 1;
    }
    true
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(rename = "type")]
    pub frame_type: String<24>,
    #[serde(rename = "requestId")]
    pub request_id: String<REQUEST_ID_MAX>,
    pub op: String<OP_MAX>,
    pub capability: String<CAPABILITY_MAX>,
    #[serde(default)]
    pub color: Option<String<DISPLAY_COLOR_MAX>>,
    #[serde(default)]
    pub address: Option<u8>,
    #[serde(default)]
    pub register: Option<u8>,
}

impl Request {
    pub fn command(&self) -> Option<Command> {
        if self.frame_type != FIRMWARE_KIND || !request_id_is_safe(self.request_id.as_bytes()) {
            return None;
        }
        let command = Command::parse(&self.op)?;
        if !equal_literal(self.capability.as_bytes(), self.op.as_bytes()) {
            return None;
        }
        if matches!(command, Command::PreviewDisplay)
            && DisplayPattern::parse(self.color.as_deref()).is_none()
        {
            return None;
        }
        if equal_literal(self.op.as_bytes(), b"test_i2c") {
            let address = self.address.unwrap_or(0x22);
            let register = self.register.unwrap_or(0x09);
            if !I2C_READ_ALLOWLIST.contains(&(address, register)) {
                return None;
            }
        }
        Some(command)
    }

    pub fn display_pattern(&self) -> Option<DisplayPattern> {
        if !matches!(Command::parse(&self.op), Some(Command::PreviewDisplay)) {
            return None;
        }
        DisplayPattern::parse(self.color.as_deref())
    }
}

fn request_id_is_safe(request_id: &[u8]) -> bool {
    !request_id.is_empty()
        && request_id
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b':'))
}

pub fn parse_request(line: &[u8]) -> Option<(Request, usize)> {
    let (request, consumed) = serde_json_core::de::from_slice(line).ok()?;
    (consumed == line.len()).then_some((request, consumed))
}

pub fn write_identity(out: &mut String<1024>) -> core::fmt::Result {
    use core::fmt::Write;
    write!(
        out,
        "{{\"type\":\"hello\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"framing\":\"{FRAMING}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"identity\":{{\"firmwareKind\":\"{FIRMWARE_KIND}\",\"deviceId\":\"flux-purr-ram-s3\",\"firmwareVersion\":\"ram-bringup/0.1\",\"buildId\":\"{}\",\"gitSha\":\"{}\",\"board\":\"esp32-s3\",\"apiVersion\":\"2026-05-29\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"hostname\":\"flux-purr-ram-s3\",\"capabilities\":[",
        env!("FLUX_PURR_RAM_BUILD_ID"),
        env!("FLUX_PURR_RAM_SOURCE_SHA")
    )?;
    for (index, capability) in CAPABILITIES.iter().enumerate() {
        if index != 0 {
            out.push(',').map_err(|_| core::fmt::Error)?;
        }
        out.push('"').map_err(|_| core::fmt::Error)?;
        out.push_str(capability).map_err(|_| core::fmt::Error)?;
        out.push('"').map_err(|_| core::fmt::Error)?;
    }
    out.push_str("]},\"capabilities\":[")
        .map_err(|_| core::fmt::Error)?;
    for (index, capability) in CAPABILITIES.iter().enumerate() {
        if index != 0 {
            out.push(',').map_err(|_| core::fmt::Error)?;
        }
        out.push('"').map_err(|_| core::fmt::Error)?;
        out.push_str(capability).map_err(|_| core::fmt::Error)?;
        out.push('"').map_err(|_| core::fmt::Error)?;
    }
    out.push_str("]}\n").map_err(|_| core::fmt::Error)
}

pub fn write_response(
    out: &mut String<512>,
    request_id: &str,
    capability: &str,
    ok: bool,
    detail: &str,
    data: ResponseData,
) -> core::fmt::Result {
    out.push_str("{\"type\":\"response\",\"requestId\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(request_id).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"ok\":").map_err(|_| core::fmt::Error)?;
    out.push_str(if ok { "true" } else { "false" })
        .map_err(|_| core::fmt::Error)?;
    out.push_str(",\"firmwareKind\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(FIRMWARE_KIND).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"capability\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(capability).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"result\":{\"detail\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(detail).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"heater\":\"off\",\"pd\":\"untouched\",\"eeprom\":\"untouched\"")
        .map_err(|_| core::fmt::Error)?;
    match data {
        ResponseData::None => {}
        ResponseData::Effect(effect) => {
            out.push_str(",\"effect\":\"")
                .map_err(|_| core::fmt::Error)?;
            out.push_str(effect).map_err(|_| core::fmt::Error)?;
            out.push('"').map_err(|_| core::fmt::Error)?;
        }
        ResponseData::Buttons {
            center,
            right,
            down,
            left,
            up,
        } => {
            out.push_str(",\"buttons\":{\"center\":")
                .map_err(|_| core::fmt::Error)?;
            push_bool(out, center)?;
            out.push_str(",\"right\":").map_err(|_| core::fmt::Error)?;
            push_bool(out, right)?;
            out.push_str(",\"down\":").map_err(|_| core::fmt::Error)?;
            push_bool(out, down)?;
            out.push_str(",\"left\":").map_err(|_| core::fmt::Error)?;
            push_bool(out, left)?;
            out.push_str(",\"up\":").map_err(|_| core::fmt::Error)?;
            push_bool(out, up)?;
            out.push_str("}").map_err(|_| core::fmt::Error)?;
        }
        ResponseData::Adc { vin, rtd } => {
            out.push_str(",\"adc\":{\"vin\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, vin)?;
            out.push_str(",\"rtd\":").map_err(|_| core::fmt::Error)?;
            push_u16(out, rtd)?;
            out.push_str("}").map_err(|_| core::fmt::Error)?;
        }
        ResponseData::I2c {
            address,
            register,
            value,
        } => {
            out.push_str(",\"i2c\":{\"address\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, address as u16)?;
            out.push_str(",\"register\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, register as u16)?;
            out.push_str(",\"value\":").map_err(|_| core::fmt::Error)?;
            push_u16(out, value as u16)?;
            out.push_str("}").map_err(|_| core::fmt::Error)?;
        }
        ResponseData::Fan {
            vin_raw,
            vin_adc_mv,
            input_mv,
            minimum_mv,
            measured,
            voltage_ok,
        } => {
            out.push_str(",\"effect\":\"fan_50_percent_5s_100_percent_5s_0_percent_5s\",\"power\":{\"vinRaw\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, vin_raw)?;
            out.push_str(",\"vinAdcMv\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, vin_adc_mv)?;
            out.push_str(",\"inputMv\":")
                .map_err(|_| core::fmt::Error)?;
            push_u32(out, input_mv)?;
            out.push_str(",\"minimumMv\":")
                .map_err(|_| core::fmt::Error)?;
            push_u16(out, minimum_mv)?;
            out.push_str(",\"measured\":")
                .map_err(|_| core::fmt::Error)?;
            push_bool(out, measured)?;
            out.push_str(",\"voltageOk\":")
                .map_err(|_| core::fmt::Error)?;
            push_bool(out, voltage_ok)?;
            out.push_str("}").map_err(|_| core::fmt::Error)?;
        }
    }
    out.push_str("}}\n").map_err(|_| core::fmt::Error)
}

pub fn write_fan_voltage_compact_progress(
    out: &mut String<256>,
    input_mv: u32,
    measured: bool,
    voltage_ok: bool,
) -> core::fmt::Result {
    out.push_str("{\"type\":\"fp\",\"v\":")
        .map_err(|_| core::fmt::Error)?;
    push_u32_small(out, input_mv)?;
    out.push_str(",\"m\":").map_err(|_| core::fmt::Error)?;
    push_bool_small(out, measured)?;
    out.push_str(",\"ok\":").map_err(|_| core::fmt::Error)?;
    push_bool_small(out, voltage_ok)?;
    out.push_str("}\n").map_err(|_| core::fmt::Error)
}

pub fn write_fan_stage_progress(
    out: &mut String<256>,
    _request_id: &str,
    duty_percent: u8,
    _duration_ms: u32,
) -> core::fmt::Result {
    let frame = match duty_percent {
        50 => "{\"type\":\"fs\",\"d\":50}\n",
        100 => "{\"type\":\"fs\",\"d\":100}\n",
        0 => "{\"type\":\"fs\",\"d\":0}\n",
        _ => return Err(core::fmt::Error),
    };
    out.push_str(frame).map_err(|_| core::fmt::Error)
}

fn push_bool(out: &mut String<512>, value: bool) -> core::fmt::Result {
    out.push_str(if value { "true" } else { "false" })
        .map_err(|_| core::fmt::Error)
}

fn push_bool_small(out: &mut String<256>, value: bool) -> core::fmt::Result {
    out.push_str(if value { "true" } else { "false" })
        .map_err(|_| core::fmt::Error)
}

fn push_u16(out: &mut String<512>, value: u16) -> core::fmt::Result {
    if value == 0 {
        out.push('0').map_err(|_| core::fmt::Error)?;
        return Ok(());
    }
    let mut digits = [0u8; 5];
    let mut length = 0;
    let mut remaining = value;
    while remaining != 0 {
        digits[length] = (remaining % 10) as u8;
        length += 1;
        remaining /= 10;
    }
    while length != 0 {
        length -= 1;
        out.push((b'0' + digits[length]) as char)
            .map_err(|_| core::fmt::Error)?;
    }
    Ok(())
}

fn push_u32(out: &mut String<512>, value: u32) -> core::fmt::Result {
    if value == 0 {
        out.push('0').map_err(|_| core::fmt::Error)?;
        return Ok(());
    }
    let mut digits = [0u8; 10];
    let mut length = 0;
    let mut remaining = value;
    while remaining != 0 {
        digits[length] = (remaining % 10) as u8;
        length += 1;
        remaining /= 10;
    }
    while length != 0 {
        length -= 1;
        out.push((b'0' + digits[length]) as char)
            .map_err(|_| core::fmt::Error)?;
    }
    Ok(())
}

fn push_u32_small(out: &mut String<256>, value: u32) -> core::fmt::Result {
    if value == 0 {
        out.push('0').map_err(|_| core::fmt::Error)?;
        return Ok(());
    }
    let mut digits = [0u8; 10];
    let mut length = 0;
    let mut remaining = value;
    while remaining != 0 {
        digits[length] = (remaining % 10) as u8;
        length += 1;
        remaining /= 10;
    }
    while length != 0 {
        length -= 1;
        out.push((b'0' + digits[length]) as char)
            .map_err(|_| core::fmt::Error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_capabilities_match_command_surface() {
        for capability in CAPABILITIES {
            assert!(
                Command::parse(capability).is_some(),
                "capability has no command implementation: {capability}"
            );
        }
    }

    #[test]
    fn only_typed_capability_bound_commands_are_accepted() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"test_i2c","capability":"test_i2c","address":34}"#,
        )
        .expect("request parses");
        assert_eq!(request.command(), Some(Command::TestI2c));
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r2","op":"test_fan","capability":"test_rgb"}"#,
        )
        .expect("request parses");
        assert_eq!(request.command(), None);
    }

    #[test]
    fn response_is_valid_jsonl() {
        let mut response = String::<512>::new();
        write_response(
            &mut response,
            "r1",
            "preview_status_light",
            true,
            "status_light_pwm_breath_ready",
            ResponseData::Effect("pwm_breathing_rainbow_8s"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["type"], "response");
        assert_eq!(value["requestId"], "r1");
        assert_eq!(value["capability"], "preview_status_light");
        assert_eq!(value["result"]["heater"], "off");
        assert_eq!(value["result"]["effect"], "pwm_breathing_rainbow_8s");
    }

    #[test]
    fn buzzer_response_describes_the_two_tone_sequence() {
        let mut response = String::<512>::new();
        write_response(
            &mut response,
            "r1",
            "test_buzzer",
            true,
            "buzzer_ready",
            ResponseData::Effect("1khz_1s_silence_1s_2khz_1s"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["result"]["detail"], "buzzer_ready");
        assert_eq!(value["result"]["effect"], "1khz_1s_silence_1s_2khz_1s");
    }

    #[test]
    fn rgb_response_describes_the_slow_five_cycle_sequence() {
        let mut response = String::<512>::new();
        write_response(
            &mut response,
            "r1",
            "test_rgb",
            true,
            "rgb_ready",
            ResponseData::Effect("rgb_red_green_blue_1s_x5"),
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["result"]["detail"], "rgb_ready");
        assert_eq!(value["result"]["effect"], "rgb_red_green_blue_1s_x5");
    }

    #[test]
    fn fan_response_describes_the_three_five_second_pwm_stages() {
        let mut response = String::<512>::new();
        write_response(
            &mut response,
            "r1",
            "test_fan",
            true,
            "fan_ready",
            ResponseData::Fan {
                vin_raw: 1800,
                vin_adc_mv: 1100,
                input_mv: 12_500,
                minimum_mv: 12_500,
                measured: true,
                voltage_ok: true,
            },
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["result"]["detail"], "fan_ready");
        assert_eq!(
            value["result"]["effect"],
            "fan_50_percent_5s_100_percent_5s_0_percent_5s"
        );
        assert_eq!(value["result"]["power"]["inputMv"], 12_500);
        assert_eq!(value["result"]["power"]["minimumMv"], 12_500);
        assert_eq!(value["result"]["power"]["voltageOk"], true);
    }

    #[test]
    fn fan_progress_frames_describe_power_and_pwm_stage_starts() {
        let mut compact_power = String::<256>::new();
        write_fan_voltage_compact_progress(&mut compact_power, 12_500, true, true).unwrap();
        let compact_power_value: serde_json::Value =
            serde_json::from_slice(compact_power.as_bytes()).unwrap();
        assert_eq!(compact_power_value["type"], "fp");
        assert_eq!(compact_power_value["v"], 12_500);
        assert_eq!(compact_power_value["m"], true);
        assert_eq!(compact_power_value["ok"], true);
        assert!(compact_power.as_bytes().len() < 64);

        let mut stage = String::<256>::new();
        write_fan_stage_progress(&mut stage, "r1", 50, 5_000).unwrap();
        let stage_value: serde_json::Value = serde_json::from_slice(stage.as_bytes()).unwrap();
        assert_eq!(stage_value["type"], "fs");
        assert_eq!(stage_value["d"], 50);
        assert!(stage.as_bytes().len() < 64);
    }

    #[test]
    fn response_includes_button_states() {
        let mut response = String::<512>::new();
        write_response(
            &mut response,
            "r1",
            "test_buttons",
            true,
            "buttons_read_only_ready",
            ResponseData::Buttons {
                center: true,
                right: false,
                down: false,
                left: true,
                up: false,
            },
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["result"]["buttons"]["center"], true);
        assert_eq!(value["result"]["buttons"]["left"], true);
        assert_eq!(value["result"]["buttons"]["up"], false);
    }

    #[test]
    fn i2c_rejects_non_allowlisted_addresses() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"test_i2c","capability":"test_i2c","address":80,"register":0}"#,
        )
        .expect("request parses");
        assert_eq!(request.command(), None);
    }

    #[test]
    fn request_id_rejects_json_injection_characters() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r\"1","op":"test_fan","capability":"test_fan"}"#,
        )
        .expect("request parses");
        assert_eq!(request.command(), None);
    }

    #[test]
    fn display_preview_defaults_to_calibration_and_accepts_named_colors() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"preview_display","capability":"preview_display"}"#,
        )
        .expect("request parses");
        assert_eq!(request.display_pattern(), Some(DisplayPattern::Calibration));

        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r2","op":"preview_display","capability":"preview_display","color":"red"}"#,
        )
        .expect("request parses");
        assert_eq!(
            request.display_pattern(),
            Some(DisplayPattern::Solid([0xf8, 0x00]))
        );
    }

    #[test]
    fn display_preview_rejects_unknown_colors() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"preview_display","capability":"preview_display","color":"orange"}"#,
        )
        .expect("request parses");
        assert_eq!(request.command(), None);
    }

    #[test]
    fn request_parser_rejects_trailing_bytes() {
        assert!(parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"exit","capability":"exit"}garbage"#,
        )
        .is_none());
    }
}
