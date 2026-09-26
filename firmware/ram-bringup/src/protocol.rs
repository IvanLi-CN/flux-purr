#![cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]

use heapless::String;
use serde::Deserialize;

pub const PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const FRAMING: &str = "jsonl";
pub const FIRMWARE_KIND: &str = "ram_bringup";
pub const REQUEST_ID_MAX: usize = 48;
pub const OP_MAX: usize = 32;
pub const CAPABILITY_MAX: usize = 32;
pub const CAPABILITIES: [&str; 12] = [
    "identity",
    "status",
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

impl Command {
    pub fn parse(op: &str) -> Option<Self> {
        match op {
            "preview_display" => Some(Self::PreviewDisplay),
            "preview_frontpanel" => Some(Self::PreviewFrontpanel),
            "preview_status_light" => Some(Self::PreviewStatusLight),
            "test_buttons" => Some(Self::TestButtons),
            "test_adc" => Some(Self::TestAdc),
            "test_i2c" => Some(Self::TestI2c),
            "test_rgb" => Some(Self::TestRgb),
            "test_buzzer" => Some(Self::TestBuzzer),
            "test_fan" => Some(Self::TestFan),
            "exit" => Some(Self::Exit),
            _ => None,
        }
    }

    pub const fn capability(self) -> &'static str {
        match self {
            Self::PreviewDisplay => "preview_display",
            Self::PreviewFrontpanel => "preview_frontpanel",
            Self::PreviewStatusLight => "preview_status_light",
            Self::TestButtons => "test_buttons",
            Self::TestAdc => "test_adc",
            Self::TestI2c => "test_i2c",
            Self::TestRgb => "test_rgb",
            Self::TestBuzzer => "test_buzzer",
            Self::TestFan => "test_fan",
            Self::Exit => "exit",
        }
    }
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
        if self.capability != command.capability() {
            return None;
        }
        if matches!(command, Command::TestI2c) {
            let address = self.address.unwrap_or(0x22);
            let register = self.register.unwrap_or(0x09);
            if !I2C_READ_ALLOWLIST.contains(&(address, register)) {
                return None;
            }
        }
        Some(command)
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
        write!(out, "\"{capability}\"")?;
    }
    out.push_str("]},\"capabilities\":[")
        .map_err(|_| core::fmt::Error)?;
    for (index, capability) in CAPABILITIES.iter().enumerate() {
        if index != 0 {
            out.push(',').map_err(|_| core::fmt::Error)?;
        }
        write!(out, "\"{capability}\"")?;
    }
    out.push_str("]}\n").map_err(|_| core::fmt::Error)
}

pub fn write_response(
    out: &mut String<512>,
    request_id: &str,
    command: Command,
    ok: bool,
    detail: &str,
) -> core::fmt::Result {
    use core::fmt::Write;
    writeln!(
        out,
        "{{\"type\":\"response\",\"requestId\":\"{request_id}\",\"ok\":{ok},\"firmwareKind\":\"{FIRMWARE_KIND}\",\"capability\":\"{}\",\"result\":{{\"detail\":\"{detail}\",\"heater\":\"off\",\"pd\":\"untouched\",\"eeprom\":\"untouched\"}}}}",
        command.capability()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
