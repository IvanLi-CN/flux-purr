#![cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]

use heapless::String;
use serde::Deserialize;

pub const PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const FRAMING: &str = "jsonl";
pub const FIRMWARE_KIND: &str = "ram_bringup";
pub const CAPABILITY: &str = "test_pd_sink";
pub const REQUEST_ID_MAX: usize = 48;
pub const OP_MAX: usize = 32;
pub const CAPABILITY_MAX: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    TestPdSink,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(rename = "type")]
    pub frame_type: String<24>,
    #[serde(rename = "requestId")]
    pub request_id: String<REQUEST_ID_MAX>,
    pub op: String<OP_MAX>,
    pub capability: String<CAPABILITY_MAX>,
    #[serde(rename = "validateVin")]
    pub validate_vin: Option<bool>,
}

impl Request {
    pub fn command(&self) -> Option<Command> {
        if self.frame_type != FIRMWARE_KIND || !request_id_is_safe(self.request_id.as_bytes()) {
            return None;
        }
        if self.op.as_str() != CAPABILITY || self.capability.as_str() != CAPABILITY {
            return None;
        }
        Some(Command::TestPdSink)
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
        "{{\"type\":\"hello\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"framing\":\"{FRAMING}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"identity\":{{\"firmwareKind\":\"{FIRMWARE_KIND}\",\"deviceId\":\"flux-purr-ram-pd-hil-s3\",\"firmwareVersion\":\"ram-pd-hil/0.1\",\"buildId\":\"{}\",\"gitSha\":\"{}\",\"board\":\"esp32-s3\",\"apiVersion\":\"2026-09-28\",\"protocolVersion\":\"{PROTOCOL_VERSION}\",\"hostname\":\"flux-purr-ram-pd-hil-s3\",\"resetReason\":\"{reset_reason}\",\"capabilities\":[\"{CAPABILITY}\"]}},\"capabilities\":[\"{CAPABILITY}\"]}}",
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

pub fn write_summary_chunk_progress(
    out: &mut String<512>,
    request_id: &str,
    sequence: u32,
    index: usize,
    count: usize,
    data: &[u8],
) -> core::fmt::Result {
    out.push_str("{\"type\":\"progress\",\"requestId\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(request_id).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"firmwareKind\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(FIRMWARE_KIND).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"capability\":\"")
        .map_err(|_| core::fmt::Error)?;
    out.push_str(CAPABILITY).map_err(|_| core::fmt::Error)?;
    out.push_str("\",\"sequence\":")
        .map_err(|_| core::fmt::Error)?;
    push_u32(out, sequence)?;
    out.push_str(",\"result\":{\"kind\":\"summary_chunk\",\"encoding\":\"hex\",\"index\":")
        .map_err(|_| core::fmt::Error)?;
    push_usize(out, index)?;
    out.push_str(",\"count\":").map_err(|_| core::fmt::Error)?;
    push_usize(out, count)?;
    out.push_str(",\"data\":\"").map_err(|_| core::fmt::Error)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in data {
        out.push(char::from(HEX[usize::from(byte >> 4)]))
            .map_err(|_| core::fmt::Error)?;
        out.push(char::from(HEX[usize::from(byte & 0x0f)]))
            .map_err(|_| core::fmt::Error)?;
    }
    out.push_str("\",\"heater\":\"off\",\"pd\":\"owned\",\"eeprom\":\"untouched\"}}\n")
        .map_err(|_| core::fmt::Error)
}

fn push_usize<const N: usize>(out: &mut String<N>, value: usize) -> core::fmt::Result {
    let value = u32::try_from(value).map_err(|_| core::fmt::Error)?;
    push_u32(out, value)
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

pub fn write_summary_prefix(
    out: &mut String<32_768>,
    request_id: &str,
    overall: &str,
    pd_status: &str,
) -> core::fmt::Result {
    use core::fmt::Write;
    write!(
        out,
        "{{\"type\":\"pd_hil_summary\",\"requestId\":\"{request_id}\",\"firmwareKind\":\"{FIRMWARE_KIND}\",\"capability\":\"{CAPABILITY}\",\"ok\":true,\"result\":{{\"detail\":\"pd_hil_complete\",\"heater\":\"off\",\"pd\":\"{pd_status}\",\"eeprom\":\"untouched\",\"schemaVersion\":1,\"buildId\":\"{}\",\"sourceSha\":\"{}\",\"controller\":\"fusb302b\",\"overall\":\"{overall}\",",
        env!("FLUX_PURR_RAM_BUILD_ID"),
        env!("FLUX_PURR_RAM_SOURCE_SHA")
    )
}

pub fn write_summary_suffix(out: &mut String<32_768>) -> core::fmt::Result {
    out.push_str("}}\n").map_err(|_| core::fmt::Error)
}

fn request_id_is_safe(request_id: &[u8]) -> bool {
    !request_id.is_empty()
        && request_id
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pd_hil_requests_are_accepted() {
        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r1","op":"test_pd_sink","capability":"test_pd_sink"}"#,
        )
        .unwrap();
        assert_eq!(request.command(), Some(Command::TestPdSink));

        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r3","op":"test_pd_sink","capability":"test_pd_sink","validateVin":false}"#,
        )
        .unwrap();
        assert_eq!(request.validate_vin, Some(false));

        let (request, _) = parse_request(
            br#"{"type":"ram_bringup","requestId":"r2","op":"test_i2c","capability":"test_i2c"}"#,
        )
        .unwrap();
        assert_eq!(request.command(), None);
    }

    #[test]
    fn identity_and_progress_are_jsonl() {
        let mut identity = String::<1024>::new();
        write_identity(&mut identity, "CoreSw").unwrap();
        let value: serde_json::Value = serde_json::from_slice(identity.as_bytes()).unwrap();
        assert_eq!(value["capabilities"][0], CAPABILITY);

        let mut progress = String::<512>::new();
        write_progress(&mut progress, "r1", 4, "{\"kind\":\"session\"}").unwrap();
        let value: serde_json::Value = serde_json::from_slice(progress.as_bytes()).unwrap();
        assert_eq!(value["sequence"], 4);
        assert_eq!(value["result"]["kind"], "session");
    }

    #[test]
    fn summary_chunk_progress_is_jsonl() {
        let mut progress = String::<512>::new();
        write_summary_chunk_progress(&mut progress, "r1", 7, 1, 3, b"{}{}").unwrap();
        let value: serde_json::Value = serde_json::from_slice(progress.as_bytes()).unwrap();
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["result"]["kind"], "summary_chunk");
        assert_eq!(value["result"]["data"], "7b7d7b7d");
    }

    #[test]
    fn summary_prefix_and_suffix_close_both_json_objects() {
        let mut summary = String::<32_768>::new();
        write_summary_prefix(&mut summary, "r1", "unsupported", "default_verified").unwrap();
        summary.push_str("\"policy\":{}").unwrap();
        write_summary_suffix(&mut summary).unwrap();
        let value: serde_json::Value = serde_json::from_slice(summary.as_bytes()).unwrap();
        assert_eq!(value["result"]["overall"], "unsupported");
        assert_eq!(value["result"]["pd"], "default_verified");
    }
}
