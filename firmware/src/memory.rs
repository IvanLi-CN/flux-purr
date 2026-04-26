use heapless::String;

use crate::frontpanel::{
    FRONTPANEL_PRESET_COUNT, FRONTPANEL_TARGET_TEMP_MAX_C, FRONTPANEL_TARGET_TEMP_MIN_C,
};

pub const M24C64_I2C_ADDRESS: u8 = 0x50;
pub const M24C64_CAPACITY_BYTES: u16 = 8 * 1024;
pub const M24C64_PAGE_SIZE: usize = 32;
pub const MEMORY_SLOT_SIZE: usize = 512;
pub const MEMORY_SLOT_A_OFFSET: u16 = 0x0000;
pub const MEMORY_SLOT_B_OFFSET: u16 = 0x0200;
pub const MEMORY_RECORD_FORMAT_VERSION: u8 = 1;
pub const MEMORY_RECORD_HEADER_LEN: usize = 16;
pub const MEMORY_RECORD_PAYLOAD_MAX: usize = MEMORY_SLOT_SIZE - MEMORY_RECORD_HEADER_LEN;
pub const MEMORY_WIFI_SSID_MAX_LEN: usize = 32;
pub const MEMORY_WIFI_PASSWORD_MAX_LEN: usize = 64;
pub const MEMORY_WRITE_DEBOUNCE_MS: u64 = 2_000;

const MEMORY_RECORD_MAGIC: [u8; 4] = *b"FPM1";
const PRESET_NONE_WIRE_VALUE: i16 = i16::MIN;

const TLV_TARGET_TEMP_C: u8 = 0x01;
const TLV_SELECTED_PRESET_SLOT: u8 = 0x02;
const TLV_PRESETS_C: u8 = 0x03;
const TLV_ACTIVE_COOLING_ENABLED: u8 = 0x04;
const TLV_WIFI_SSID: u8 = 0x10;
const TLV_WIFI_PASSWORD: u8 = 0x11;
const TLV_WIFI_AUTO_RECONNECT: u8 = 0x12;
const TLV_TELEMETRY_INTERVAL_MS: u8 = 0x13;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryConfig {
    pub target_temp_c: i16,
    pub selected_preset_slot: usize,
    pub presets_c: [Option<i16>; FRONTPANEL_PRESET_COUNT],
    pub active_cooling_enabled: bool,
    pub wifi_ssid: String<MEMORY_WIFI_SSID_MAX_LEN>,
    pub wifi_password: String<MEMORY_WIFI_PASSWORD_MAX_LEN>,
    pub wifi_auto_reconnect: bool,
    pub telemetry_interval_ms: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            target_temp_c: 100,
            selected_preset_slot: 1,
            presets_c: [
                Some(50),
                Some(100),
                Some(120),
                Some(150),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ],
            active_cooling_enabled: true,
            wifi_ssid: String::new(),
            wifi_password: String::new(),
            wifi_auto_reconnect: true,
            telemetry_interval_ms: 500,
        }
    }
}

impl MemoryConfig {
    pub fn sanitize(&mut self) {
        self.target_temp_c = clamp_temp_c(self.target_temp_c);
        if self.selected_preset_slot >= FRONTPANEL_PRESET_COUNT {
            self.selected_preset_slot = MemoryConfig::default().selected_preset_slot;
        }
        for temp in self.presets_c.iter_mut().flatten() {
            *temp = clamp_temp_c(*temp);
        }
        if self.telemetry_interval_ms == 0 {
            self.telemetry_interval_ms = MemoryConfig::default().telemetry_interval_ms;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    pub sequence: u32,
    pub config: MemoryConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryDecodeError {
    TooShort,
    BadMagic,
    UnsupportedFormat(u8),
    BadHeaderLength(u8),
    PayloadOutOfBounds,
    CrcMismatch,
    MalformedTlv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEncodeError {
    BufferTooSmall,
    PayloadTooLarge,
}

#[cfg(target_arch = "xtensa")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EepromError<I2cError> {
    OutOfRange,
    PageWriteTooLong,
    PageBoundaryCrossed,
    I2c(I2cError),
}

#[cfg(target_arch = "xtensa")]
pub struct M24c64<I2C> {
    i2c: I2C,
    address: u8,
}

#[cfg(target_arch = "xtensa")]
impl<I2C> M24c64<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: M24C64_I2C_ADDRESS,
        }
    }

    pub const fn with_address(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

#[cfg(target_arch = "xtensa")]
impl<I2C> M24c64<I2C>
where
    I2C: embedded_hal::i2c::I2c,
{
    pub fn read_bytes(
        &mut self,
        offset: u16,
        bytes: &mut [u8],
    ) -> Result<(), EepromError<I2C::Error>> {
        if usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES) {
            return Err(EepromError::OutOfRange);
        }
        let address = offset.to_be_bytes();
        self.i2c
            .write_read(self.address, &address, bytes)
            .map_err(EepromError::I2c)
    }

    pub fn write_page(&mut self, offset: u16, bytes: &[u8]) -> Result<(), EepromError<I2C::Error>> {
        if bytes.len() > M24C64_PAGE_SIZE {
            return Err(EepromError::PageWriteTooLong);
        }
        if usize::from(offset) + bytes.len() > usize::from(M24C64_CAPACITY_BYTES) {
            return Err(EepromError::OutOfRange);
        }
        let page_offset = usize::from(offset) % M24C64_PAGE_SIZE;
        if page_offset + bytes.len() > M24C64_PAGE_SIZE {
            return Err(EepromError::PageBoundaryCrossed);
        }

        let mut payload = [0u8; M24C64_PAGE_SIZE + 2];
        payload[0..2].copy_from_slice(&offset.to_be_bytes());
        payload[2..2 + bytes.len()].copy_from_slice(bytes);
        self.i2c
            .write(self.address, &payload[..2 + bytes.len()])
            .map_err(EepromError::I2c)
    }
}

pub fn encode_memory_record(
    record: &MemoryRecord,
    out: &mut [u8],
) -> Result<usize, MemoryEncodeError> {
    if out.len() < MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryEncodeError::BufferTooSmall);
    }

    let payload_len = encode_config_payload(&record.config, &mut out[MEMORY_RECORD_HEADER_LEN..])?;
    if payload_len > MEMORY_RECORD_PAYLOAD_MAX {
        return Err(MemoryEncodeError::PayloadTooLarge);
    }

    out[0..4].copy_from_slice(&MEMORY_RECORD_MAGIC);
    out[4] = MEMORY_RECORD_FORMAT_VERSION;
    out[5] = MEMORY_RECORD_HEADER_LEN as u8;
    out[6..8].copy_from_slice(&(payload_len as u16).to_le_bytes());
    out[8..12].copy_from_slice(&record.sequence.to_le_bytes());
    let crc = crc32_update(
        crc32(&out[0..12]),
        &out[MEMORY_RECORD_HEADER_LEN..MEMORY_RECORD_HEADER_LEN + payload_len],
    ) ^ 0xffff_ffff;
    out[12..16].copy_from_slice(&crc.to_le_bytes());

    Ok(MEMORY_RECORD_HEADER_LEN + payload_len)
}

pub fn decode_memory_record(bytes: &[u8]) -> Result<MemoryRecord, MemoryDecodeError> {
    if bytes.len() < MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryDecodeError::TooShort);
    }
    if bytes[0..4] != MEMORY_RECORD_MAGIC {
        return Err(MemoryDecodeError::BadMagic);
    }
    if bytes[4] != MEMORY_RECORD_FORMAT_VERSION {
        return Err(MemoryDecodeError::UnsupportedFormat(bytes[4]));
    }
    if bytes[5] as usize != MEMORY_RECORD_HEADER_LEN {
        return Err(MemoryDecodeError::BadHeaderLength(bytes[5]));
    }

    let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    let payload_end = MEMORY_RECORD_HEADER_LEN
        .checked_add(payload_len)
        .ok_or(MemoryDecodeError::PayloadOutOfBounds)?;
    if payload_len > MEMORY_RECORD_PAYLOAD_MAX || payload_end > bytes.len() {
        return Err(MemoryDecodeError::PayloadOutOfBounds);
    }

    let expected_crc = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let actual_crc = crc32_update(
        crc32(&bytes[0..12]),
        &bytes[MEMORY_RECORD_HEADER_LEN..payload_end],
    ) ^ 0xffff_ffff;
    if expected_crc != actual_crc {
        return Err(MemoryDecodeError::CrcMismatch);
    }

    let sequence = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let mut config = decode_config_payload(&bytes[MEMORY_RECORD_HEADER_LEN..payload_end])?;
    config.sanitize();

    Ok(MemoryRecord { sequence, config })
}

pub fn select_latest_memory_record(
    left: Result<MemoryRecord, MemoryDecodeError>,
    right: Result<MemoryRecord, MemoryDecodeError>,
) -> Option<MemoryRecord> {
    match (left, right) {
        (Ok(left), Ok(right)) if right.sequence > left.sequence => Some(right),
        (Ok(left), Ok(_)) => Some(left),
        (Ok(left), Err(_)) => Some(left),
        (Err(_), Ok(right)) => Some(right),
        (Err(_), Err(_)) => None,
    }
}

fn encode_config_payload(
    config: &MemoryConfig,
    out: &mut [u8],
) -> Result<usize, MemoryEncodeError> {
    let mut cursor = 0;
    push_tlv(
        TLV_TARGET_TEMP_C,
        &config.target_temp_c.to_le_bytes(),
        out,
        &mut cursor,
    )?;
    push_tlv(
        TLV_SELECTED_PRESET_SLOT,
        &[config.selected_preset_slot as u8],
        out,
        &mut cursor,
    )?;

    let mut presets = [0u8; FRONTPANEL_PRESET_COUNT * 2];
    for (index, preset) in config.presets_c.iter().enumerate() {
        let wire_value = preset.map(clamp_temp_c).unwrap_or(PRESET_NONE_WIRE_VALUE);
        presets[index * 2..index * 2 + 2].copy_from_slice(&wire_value.to_le_bytes());
    }
    push_tlv(TLV_PRESETS_C, &presets, out, &mut cursor)?;
    push_tlv(
        TLV_ACTIVE_COOLING_ENABLED,
        &[u8::from(config.active_cooling_enabled)],
        out,
        &mut cursor,
    )?;
    push_tlv(TLV_WIFI_SSID, config.wifi_ssid.as_bytes(), out, &mut cursor)?;
    push_tlv(
        TLV_WIFI_PASSWORD,
        config.wifi_password.as_bytes(),
        out,
        &mut cursor,
    )?;
    push_tlv(
        TLV_WIFI_AUTO_RECONNECT,
        &[u8::from(config.wifi_auto_reconnect)],
        out,
        &mut cursor,
    )?;
    push_tlv(
        TLV_TELEMETRY_INTERVAL_MS,
        &config.telemetry_interval_ms.to_le_bytes(),
        out,
        &mut cursor,
    )?;
    Ok(cursor)
}

fn decode_config_payload(bytes: &[u8]) -> Result<MemoryConfig, MemoryDecodeError> {
    let mut config = MemoryConfig::default();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes.len().saturating_sub(cursor) < 2 {
            return Err(MemoryDecodeError::MalformedTlv);
        }
        let tag = bytes[cursor];
        let len = bytes[cursor + 1] as usize;
        cursor += 2;
        if bytes.len().saturating_sub(cursor) < len {
            return Err(MemoryDecodeError::MalformedTlv);
        }
        let value = &bytes[cursor..cursor + len];
        cursor += len;

        match tag {
            TLV_TARGET_TEMP_C if len == 2 => {
                config.target_temp_c = i16::from_le_bytes([value[0], value[1]]);
            }
            TLV_SELECTED_PRESET_SLOT if len == 1 => {
                config.selected_preset_slot = value[0] as usize;
            }
            TLV_PRESETS_C if len == FRONTPANEL_PRESET_COUNT * 2 => {
                for index in 0..FRONTPANEL_PRESET_COUNT {
                    let wire_value = i16::from_le_bytes([value[index * 2], value[index * 2 + 1]]);
                    config.presets_c[index] = if wire_value == PRESET_NONE_WIRE_VALUE {
                        None
                    } else {
                        Some(wire_value)
                    };
                }
            }
            TLV_ACTIVE_COOLING_ENABLED if len == 1 => {
                config.active_cooling_enabled = value[0] != 0;
            }
            TLV_WIFI_SSID => {
                config.wifi_ssid.clear();
                let copy_len = value.len().min(MEMORY_WIFI_SSID_MAX_LEN);
                let _ = config
                    .wifi_ssid
                    .push_str(core::str::from_utf8(&value[..copy_len]).unwrap_or(""));
            }
            TLV_WIFI_PASSWORD => {
                config.wifi_password.clear();
                let copy_len = value.len().min(MEMORY_WIFI_PASSWORD_MAX_LEN);
                let _ = config
                    .wifi_password
                    .push_str(core::str::from_utf8(&value[..copy_len]).unwrap_or(""));
            }
            TLV_WIFI_AUTO_RECONNECT if len == 1 => {
                config.wifi_auto_reconnect = value[0] != 0;
            }
            TLV_TELEMETRY_INTERVAL_MS if len == 4 => {
                config.telemetry_interval_ms =
                    u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            }
            _ => {}
        }
    }
    Ok(config)
}

fn push_tlv(
    tag: u8,
    value: &[u8],
    out: &mut [u8],
    cursor: &mut usize,
) -> Result<(), MemoryEncodeError> {
    if value.len() > u8::MAX as usize {
        return Err(MemoryEncodeError::PayloadTooLarge);
    }
    let next = cursor
        .checked_add(2)
        .and_then(|position| position.checked_add(value.len()))
        .ok_or(MemoryEncodeError::PayloadTooLarge)?;
    if next > out.len() {
        return Err(MemoryEncodeError::BufferTooSmall);
    }
    out[*cursor] = tag;
    out[*cursor + 1] = value.len() as u8;
    out[*cursor + 2..next].copy_from_slice(value);
    *cursor = next;
    Ok(())
}

pub const fn clamp_temp_c(value: i16) -> i16 {
    if value < FRONTPANEL_TARGET_TEMP_MIN_C {
        FRONTPANEL_TARGET_TEMP_MIN_C
    } else if value > FRONTPANEL_TARGET_TEMP_MAX_C {
        FRONTPANEL_TARGET_TEMP_MAX_C
    } else {
        value
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    crc32_update(0xffff_ffff, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> MemoryConfig {
        let mut config = MemoryConfig {
            target_temp_c: 222,
            selected_preset_slot: 4,
            active_cooling_enabled: false,
            wifi_auto_reconnect: false,
            telemetry_interval_ms: 1_250,
            ..MemoryConfig::default()
        };
        config.presets_c[2] = None;
        config.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        config.wifi_password.push_str("secret-pass").unwrap();
        config
    }

    #[test]
    fn default_config_matches_frontpanel_defaults() {
        let config = MemoryConfig::default();
        assert_eq!(config.target_temp_c, 100);
        assert_eq!(config.selected_preset_slot, 1);
        assert_eq!(config.presets_c[0], Some(50));
        assert_eq!(config.presets_c[9], Some(300));
        assert!(config.active_cooling_enabled);
    }

    #[test]
    fn record_roundtrip_preserves_config() {
        let record = MemoryRecord {
            sequence: 42,
            config: sample_config(),
        };
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        let decoded = decode_memory_record(&bytes[..len]).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn unknown_tlv_is_ignored() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 7,
            config: sample_config(),
        };
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let insert = MEMORY_RECORD_HEADER_LEN + payload_len;
        bytes[insert] = 0xee;
        bytes[insert + 1] = 3;
        bytes[insert + 2..insert + 5].copy_from_slice(&[1, 2, 3]);
        let new_payload_len = payload_len + 5;
        bytes[6..8].copy_from_slice(&(new_payload_len as u16).to_le_bytes());
        let crc = crc32_update(
            crc32(&bytes[0..12]),
            &bytes[MEMORY_RECORD_HEADER_LEN..insert + 5],
        ) ^ 0xffff_ffff;
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());

        let decoded = decode_memory_record(&bytes[..len + 5]).unwrap();
        assert_eq!(decoded.config, record.config);
    }

    #[test]
    fn crc_rejects_corruption() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 1,
            config: sample_config(),
        };
        let len = encode_memory_record(&record, &mut bytes).unwrap();
        bytes[len - 1] ^= 0x55;
        assert_eq!(
            decode_memory_record(&bytes[..len]),
            Err(MemoryDecodeError::CrcMismatch)
        );
    }

    #[test]
    fn latest_valid_slot_wins_and_corrupt_newer_falls_back() {
        let old = MemoryRecord {
            sequence: 3,
            config: MemoryConfig::default(),
        };
        let new = MemoryRecord {
            sequence: 4,
            config: sample_config(),
        };
        assert_eq!(
            select_latest_memory_record(Ok(old.clone()), Ok(new))
                .unwrap()
                .sequence,
            4
        );
        assert_eq!(
            select_latest_memory_record(Ok(old), Err(MemoryDecodeError::CrcMismatch))
                .unwrap()
                .sequence,
            3
        );
    }

    #[test]
    fn sanitize_clamps_temperatures_and_bad_slot() {
        let mut config = MemoryConfig {
            target_temp_c: 450,
            selected_preset_slot: 99,
            ..MemoryConfig::default()
        };
        config.presets_c[0] = Some(-20);
        config.presets_c[1] = Some(480);
        config.telemetry_interval_ms = 0;
        config.sanitize();
        assert_eq!(config.target_temp_c, FRONTPANEL_TARGET_TEMP_MAX_C);
        assert_eq!(config.selected_preset_slot, 1);
        assert_eq!(config.presets_c[0], Some(FRONTPANEL_TARGET_TEMP_MIN_C));
        assert_eq!(config.presets_c[1], Some(FRONTPANEL_TARGET_TEMP_MAX_C));
        assert_eq!(config.telemetry_interval_ms, 500);
    }
}
