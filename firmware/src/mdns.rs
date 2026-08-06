//! Pure, host-testable DNS-SD response encoding.

use core::fmt::Write as _;

use heapless::String;

use crate::net_http::{DeviceNames, HTTP_SERVICE_PORT, HTTP_SERVICE_TXT, HTTP_SERVICE_TYPE};

const MDNS_TTL_SECS: u32 = 120;
const DNS_CLASS_IN: u16 = 0x0001;
const DNS_CLASS_IN_CACHE_FLUSH: u16 = 0x8001;

pub fn build_http_announcement(
    buffer: &mut [u8],
    names: &DeviceNames,
    ipv4: [u8; 4],
) -> Option<usize> {
    let hostname = names.hostname.as_str();
    let mut host_fqdn = String::<48>::new();
    host_fqdn.push_str(hostname).ok()?;
    host_fqdn.push_str(".local").ok()?;
    let mut instance = String::<80>::new();
    instance.push_str(hostname).ok()?;
    instance.push_str(".").ok()?;
    instance.push_str(HTTP_SERVICE_TYPE).ok()?;
    buffer
        .get_mut(..12)?
        .copy_from_slice(&[0, 0, 0x84, 0, 0, 0, 0, 4, 0, 0, 0, 0]);
    let mut at = 12;
    at = dns_record_ptr(buffer, at, HTTP_SERVICE_TYPE, instance.as_str())?;
    at = dns_record_srv(
        buffer,
        at,
        instance.as_str(),
        HTTP_SERVICE_PORT,
        host_fqdn.as_str(),
    )?;
    at = dns_record_txt(buffer, at, instance.as_str(), names)?;
    dns_record_a(buffer, at, host_fqdn.as_str(), ipv4)
}

fn dns_name(buffer: &mut [u8], mut at: usize, name: &str) -> Option<usize> {
    for label in name.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > 63 || at + bytes.len() + 1 > buffer.len() {
            return None;
        }
        buffer[at] = bytes.len() as u8;
        at += 1;
        buffer[at..at + bytes.len()].copy_from_slice(bytes);
        at += bytes.len();
    }
    *buffer.get_mut(at)? = 0;
    Some(at + 1)
}

fn dns_header(buffer: &mut [u8], at: usize, ty: u16, class: u16, data_len: u16) -> Option<usize> {
    if at + 10 > buffer.len() {
        return None;
    }
    buffer[at..at + 2].copy_from_slice(&ty.to_be_bytes());
    buffer[at + 2..at + 4].copy_from_slice(&class.to_be_bytes());
    buffer[at + 4..at + 8].copy_from_slice(&MDNS_TTL_SECS.to_be_bytes());
    buffer[at + 8..at + 10].copy_from_slice(&data_len.to_be_bytes());
    Some(at + 10)
}

fn dns_record_ptr(buffer: &mut [u8], at: usize, name: &str, target: &str) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 12, DNS_CLASS_IN, 0)?;
    let end = dns_name(buffer, data_at, target)?;
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_record_srv(
    buffer: &mut [u8],
    at: usize,
    name: &str,
    port: u16,
    target: &str,
) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 33, DNS_CLASS_IN_CACHE_FLUSH, 0)?;
    if data_at + 6 > buffer.len() {
        return None;
    }
    buffer[data_at..data_at + 6].fill(0);
    buffer[data_at + 4..data_at + 6].copy_from_slice(&port.to_be_bytes());
    let end = dns_name(buffer, data_at + 6, target)?;
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_record_txt(buffer: &mut [u8], at: usize, name: &str, names: &DeviceNames) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 16, DNS_CLASS_IN_CACHE_FLUSH, 0)?;
    let mut end = data_at;
    for entry in HTTP_SERVICE_TXT {
        end = dns_txt(buffer, end, entry)?;
    }
    let mut device = String::<20>::new();
    write!(
        device,
        "device={}",
        core::str::from_utf8(&names.device_id).ok()?
    )
    .ok()?;
    end = dns_txt(buffer, end, device.as_str())?;
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_txt(buffer: &mut [u8], at: usize, value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    if bytes.len() > 255 || at + bytes.len() + 1 > buffer.len() {
        return None;
    }
    buffer[at] = bytes.len() as u8;
    buffer[at + 1..at + 1 + bytes.len()].copy_from_slice(bytes);
    Some(at + 1 + bytes.len())
}

fn dns_record_a(buffer: &mut [u8], at: usize, name: &str, ip: [u8; 4]) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 1, DNS_CLASS_IN_CACHE_FLUSH, 4)?;
    buffer.get_mut(data_at..data_at + 4)?.copy_from_slice(&ip);
    Some(data_at + 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net_http::device_names_from_mac;

    #[test]
    fn announcement_uses_shared_ptr_and_cache_flush_for_unique_records() {
        let names = device_names_from_mac([0xa0, 0xf2, 0x62, 0xf2, 0x0d, 0x6c]);
        let mut packet = [0u8; 512];
        let length = build_http_announcement(&mut packet, &names, [192, 168, 31, 189]).unwrap();

        assert_eq!(&packet[..12], &[0, 0, 0x84, 0, 0, 0, 0, 4, 0, 0, 0, 0]);
        let ptr_name_end = dns_name_end(&packet, 12);
        assert_eq!(
            u16::from_be_bytes([packet[ptr_name_end], packet[ptr_name_end + 1]]),
            12
        );
        assert_eq!(
            u16::from_be_bytes([packet[ptr_name_end + 2], packet[ptr_name_end + 3]]),
            DNS_CLASS_IN
        );
        assert!(
            packet[..length]
                .windows(4)
                .any(|value| value == [192, 168, 31, 189])
        );
        assert!(
            packet[..length]
                .windows(b"device=a0f262f20d6c".len())
                .any(|value| value == b"device=a0f262f20d6c")
        );
    }

    fn dns_name_end(packet: &[u8], mut at: usize) -> usize {
        while packet[at] != 0 {
            at += usize::from(packet[at]) + 1;
        }
        at + 1
    }
}
