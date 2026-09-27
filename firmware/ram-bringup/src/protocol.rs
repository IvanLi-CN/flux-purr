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
    serde_json_core::de::from_slice(line).ok()
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
    out.push_str("\",\"heater\":\"off\",\"pd\":\"untouched\",\"eeprom\":\"untouched\"}}\n")
        .map_err(|_| core::fmt::Error)
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
            "status_light_preview_ready",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(value["type"], "response");
        assert_eq!(value["requestId"], "r1");
        assert_eq!(value["capability"], "preview_status_light");
        assert_eq!(value["result"]["heater"], "off");
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
}
