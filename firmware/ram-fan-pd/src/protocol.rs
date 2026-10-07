#![cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]

use heapless::String;
use serde::Deserialize;

pub const PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const FRAMING: &str = "jsonl";
pub const FIRMWARE_KIND: &str = "ram_bringup";
pub const IMAGE_KIND: &str = "fan_pd";
pub const CAPABILITY: &str = "test_fan_pd";
pub const REQUEST_ID_MAX: usize = 48;
pub const OP_MAX: usize = 32;
pub const CAPABILITY_MAX: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    TestFanPd,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(rename = "type")]
    pub frame_type: String<24>,
    #[serde(rename = "requestId")]
    pub request_id: String<REQUEST_ID_MAX>,
    pub op: String<OP_MAX>,
    pub capability: String<CAPABILITY_MAX>,
}

impl Request {
    pub fn command(&self) -> Option<Command> {
        if self.frame_type != FIRMWARE_KIND || !request_id_is_safe(self.request_id.as_bytes()) {
            return None;
        }
        if self.op.as_str() != CAPABILITY || self.capability.as_str() != CAPABILITY {
            return None;
        }
        Some(Command::TestFanPd)
    }
}

pub fn parse_request(line: &[u8]) -> Option<(Request, usize)> {
    let (request, consumed) = serde_json_core::de::from_slice(line).ok()?;
    (consumed == line.len()).then_some((request, consumed))
}

pub fn write_identity(out: &mut String<1024>, reset_reason: &str) -> core::fmt::Result {
    use core::fmt::Write;
    writeln!(
        out,
        "{{\"type\":\"hello\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"framing\":\"{FRAMING}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"identity\":{{\"firmwareKind\":\"{FIRMWARE_KIND}\",\"imageKind\":\"{IMAGE_KIND}\",\"deviceId\":\"flux-purr-ram-fan-pd-s3\",\"firmwareVersion\":\"ram-fan-pd/0.1\",\"buildId\":\"{}\",\"gitSha\":\"{}\",\"board\":\"esp32-s3\",\"apiVersion\":\"2026-10-08\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"hostname\":\"flux-purr-ram-fan-pd-s3\",\"resetReason\":\"{reset_reason}\",\"capabilities\":[\"{CAPABILITY}\"]}},\"capabilities\":[\"{CAPABILITY}\"]}}",
        env!("FLUX_PURR_RAM_BUILD_ID"),
        env!("FLUX_PURR_RAM_SOURCE_SHA")
    )
}

pub fn write_progress(
    out: &mut String<512>,
    request_id: &str,
    sequence: u32,
    result_json: &str,
) -> core::fmt::Result {
    out.push_str("{\"type\":\"progress\",\"requestId\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(request_id).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"firmwareKind\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(FIRMWARE_KIND).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"imageKind\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(IMAGE_KIND).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"capability\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(CAPABILITY).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"sequence\":")
        .map_err(|_| core::fmt::Error)?;
    push_u32(out, sequence)?;
    out.push_str(",\"result\":").map_err(|_| core::fmt::Error)?;
    out.push_str(result_json).map_err(|_| core::fmt::Error)?;
    out.push_str("}\n").map_err(|_| core::fmt::Error)
}

pub fn write_summary_prefix(
    out: &mut String<32_768>,
    request_id: &str,
    overall: &str,
    pd_status: &str,
) -> core::fmt::Result {
    use core::fmt::Write;
    write!(
        out,
        "{{\"type\":\"fan_pd_summary\",\"requestId\":\"{request_id}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"imageKind\":\"{IMAGE_KIND}\",\"capability\":\"{CAPABILITY}\",\"ok\":true,\"result\":{{\"detail\":\"fan_pd_complete\",\"heater\":\"off\",\"pd\":\"{pd_status}\",\"eeprom\":\"untouched\",\"schemaVersion\":1,\"buildId\":\"{}\",\"sourceSha\":\"{}\",\"controller\":\"fusb302b\",\"overall\":\"{overall}\",",
        env!("FLUX_PURR_RAM_BUILD_ID"),
        env!("FLUX_PURR_RAM_SOURCE_SHA")
    )
}

pub fn write_summary_suffix(out: &mut String<32_768>) -> core::fmt::Result {
    out.push_str("}}\n").map_err(|_| core::fmt::Error)
}

pub fn write_summary_chunk_progress(
    out: &mut String<512>,
    request_id: &str,
    sequence: u32,
    index: usize,
    count: usize,
    chunk: &[u8],
) -> core::fmt::Result {
    use core::fmt::Write;
    write!(
        out,
        "{{\"type\":\"progress\",\"requestId\":\"{request_id}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"imageKind\":\"{IMAGE_KIND}\",\"capability\":\"{CAPABILITY}\",\"sequence\":{sequence},\"result\":{{\"kind\":\"summary_chunk\",\"index\":{index},\"count\":{count},\"encoding\":\"hex\",\"data\":\"",
    )?;
    for byte in chunk {
        write!(out, "{byte:02x}")?;
    }
    out.push_str("\"}}\n").map_err(|_| core::fmt::Error)
}

fn request_id_is_safe(request_id: &[u8]) -> bool {
    !request_id.is_empty()
        && request_id
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b':'))
}

fn push_u32<const N: usize>(out: &mut String<N>, mut value: u32) -> core::fmt::Result {
    let mut digits = [0u8; 10];
    let mut length = 0usize;
    if value == 0 {
        digits[0] = 0;
        length = 1;
    } else {
        while value != 0 {
            digits[length] = (value % 10) as u8;
            value /= 10;
            length += 1;
        }
    }
    for digit in digits[..length].iter().rev() {
        out.push(char::from(b'0' + *digit))
            .map_err(|_| core::fmt::Error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fan_pd_requests_are_accepted() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"test_fan_pd","capability":"test_fan_pd"}"#,
        )
        .unwrap();
        assert_eq!(request.command(), Some(Command::TestFanPd));
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r2","op":"test_fan","capability":"test_fan"}"#,
        )
        .unwrap();
        assert_eq!(request.command(), None);
    }

    #[test]
    fn identity_has_a_distinct_image_kind() {
        let mut identity = String::<1024>::new();
        write_identity(&mut identity, "ram_boot").unwrap();
        let value: serde_json::Value = serde_json::from_slice(identity.as_bytes()).unwrap();
        assert_eq!(value["identity"]["imageKind"], IMAGE_KIND);
        assert_eq!(value["capabilities"][0], CAPABILITY);
    }
}
