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
pub const ADC_CALIBRATION_MAX_SAMPLES: usize = 8;
pub const HEATER_CURVE_MAX_POINTS: usize = 8;
pub const ADC_CALIBRATION_RTD_DEFAULT_LOW_MV: u16 = 0;
pub const ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV: u16 = 2_800;
pub const ADC_CALIBRATION_VIN_DEFAULT_LOW_MV: u16 = 0;
pub const ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV: u16 = VIN_DEFAULT_ADC_HIGH_MV;
pub const CALIBRATION_REFERENCE_NONE_WIRE_VALUE: i16 = i16::MIN;

const MEMORY_RECORD_MAGIC: [u8; 4] = *b"FPM1";
const PRESET_NONE_WIRE_VALUE: i16 = i16::MIN;
const CALIBRATION_NONE_WIRE_VALUE: u16 = u16::MAX;
const VIN_DEFAULT_ADC_HIGH_MV: u16 = 2_337;

const TLV_TARGET_TEMP_C: u8 = 0x01;
const TLV_SELECTED_PRESET_SLOT: u8 = 0x02;
const TLV_PRESETS_C: u8 = 0x03;
const TLV_ACTIVE_COOLING_ENABLED: u8 = 0x04;
const TLV_WIFI_SSID: u8 = 0x10;
const TLV_WIFI_PASSWORD: u8 = 0x11;
const TLV_WIFI_AUTO_RECONNECT: u8 = 0x12;
const TLV_TELEMETRY_INTERVAL_MS: u8 = 0x13;
const TLV_ACTIVE_ADC_CALIBRATION: u8 = 0x20;
const TLV_DRAFT_ADC_CALIBRATION: u8 = 0x21;
const TLV_ACTIVE_ADC_CALIBRATION_REFERENCES: u8 = 0x22;
const TLV_DRAFT_ADC_CALIBRATION_REFERENCES: u8 = 0x23;
const TLV_ACTIVE_ADC_CALIBRATION_TARGETS: u8 = 0x24;
const TLV_DRAFT_ADC_CALIBRATION_TARGETS: u8 = 0x25;
const TLV_ACTIVE_HEATER_CURVE: u8 = 0x30;

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
    pub active_adc_calibration: AdcCalibrationConfig,
    pub draft_adc_calibration: AdcCalibrationConfig,
    pub active_heater_curve: HeaterCurveConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcCalibrationChannel {
    Rtd,
    Vin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcCalibrationSample {
    pub observed_mv: u16,
    pub expected_mv: u16,
    pub reference_temp_deci_c: Option<i16>,
    pub target_adc_mv: Option<u16>,
    pub reference_vin_mv: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurvePoint {
    pub temp_centi_c: i16,
    pub resistance_milliohms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaterCurveConfig {
    pub points: [Option<HeaterCurvePoint>; HEATER_CURVE_MAX_POINTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdcCalibrationChannelConfig {
    pub samples: [Option<AdcCalibrationSample>; ADC_CALIBRATION_MAX_SAMPLES],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdcCalibrationConfig {
    pub rtd: AdcCalibrationChannelConfig,
    pub vin: AdcCalibrationChannelConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdcCalibrationFit {
    pub gain: f32,
    pub offset_mv: f32,
    pub custom_sample_count: usize,
    pub default_sample_count: usize,
}

impl Default for AdcCalibrationChannelConfig {
    fn default() -> Self {
        Self {
            samples: [None; ADC_CALIBRATION_MAX_SAMPLES],
        }
    }
}

impl Default for HeaterCurveConfig {
    fn default() -> Self {
        Self {
            points: [None; HEATER_CURVE_MAX_POINTS],
        }
    }
}

impl AdcCalibrationConfig {
    pub fn channel(&self, channel: AdcCalibrationChannel) -> &AdcCalibrationChannelConfig {
        match channel {
            AdcCalibrationChannel::Rtd => &self.rtd,
            AdcCalibrationChannel::Vin => &self.vin,
        }
    }

    pub fn channel_mut(
        &mut self,
        channel: AdcCalibrationChannel,
    ) -> &mut AdcCalibrationChannelConfig {
        match channel {
            AdcCalibrationChannel::Rtd => &mut self.rtd,
            AdcCalibrationChannel::Vin => &mut self.vin,
        }
    }
}

impl AdcCalibrationChannelConfig {
    pub fn sample_count(&self) -> usize {
        self.samples.iter().flatten().count()
    }

    pub fn insert(&mut self, sample: AdcCalibrationSample) -> Option<usize> {
        for (index, slot) in self.samples.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(sample);
                return Some(index);
            }
        }
        None
    }

    pub fn delete(&mut self, index: usize) -> bool {
        let Some(slot) = self.samples.get_mut(index) else {
            return false;
        };
        let existed = slot.is_some();
        *slot = None;
        existed
    }

    pub fn clear(&mut self) {
        self.samples = [None; ADC_CALIBRATION_MAX_SAMPLES];
    }
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
            active_adc_calibration: AdcCalibrationConfig::default(),
            draft_adc_calibration: AdcCalibrationConfig::default(),
            active_heater_curve: HeaterCurveConfig::default(),
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
        sanitize_adc_calibration(&mut self.active_adc_calibration);
        sanitize_adc_calibration(&mut self.draft_adc_calibration);
        sanitize_heater_curve(&mut self.active_heater_curve);
    }
}

pub fn heater_resistance_ohms_from_curve(
    curve: &HeaterCurveConfig,
    current_temp_c: f32,
) -> Option<f32> {
    let points = compacted_heater_points(curve);
    let first = points.first().copied()?;
    if points.len() == 1 {
        return Some(milliohms_to_ohms(first.resistance_milliohms));
    }

    let temp_centi_c = (current_temp_c * 100.0).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
    if temp_centi_c <= first.temp_centi_c {
        return Some(milliohms_to_ohms(first.resistance_milliohms));
    }

    for pair in points.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if temp_centi_c <= right.temp_centi_c {
            let span = (right.temp_centi_c - left.temp_centi_c) as f32;
            if span <= 0.0 {
                return Some(milliohms_to_ohms(left.resistance_milliohms));
            }
            let t = (temp_centi_c - left.temp_centi_c) as f32 / span;
            let left_r = milliohms_to_ohms(left.resistance_milliohms);
            let right_r = milliohms_to_ohms(right.resistance_milliohms);
            return Some(left_r + ((right_r - left_r) * t));
        }
    }

    points
        .last()
        .map(|point| milliohms_to_ohms(point.resistance_milliohms))
}

fn milliohms_to_ohms(value: u16) -> f32 {
    value as f32 / 1_000.0
}

fn compacted_heater_points(
    curve: &HeaterCurveConfig,
) -> heapless::Vec<HeaterCurvePoint, HEATER_CURVE_MAX_POINTS> {
    let mut points = heapless::Vec::new();
    for point in curve.points.iter().flatten() {
        let _ = points.push(*point);
    }
    points.sort_unstable_by_key(|point| point.temp_centi_c);
    points
}

pub fn adc_calibration_fit(
    calibration: &AdcCalibrationConfig,
    channel: AdcCalibrationChannel,
) -> AdcCalibrationFit {
    fit_channel(calibration.channel(channel), default_points(channel))
}

pub fn correct_adc_mv(
    calibration: &AdcCalibrationConfig,
    channel: AdcCalibrationChannel,
    observed_mv: u16,
) -> u16 {
    let fit = adc_calibration_fit(calibration, channel);
    let corrected = (fit.gain * observed_mv as f32) + fit.offset_mv;
    let rounded = if corrected >= 0.0 {
        corrected + 0.5
    } else {
        corrected - 0.5
    };
    rounded.clamp(0.0, u16::MAX as f32) as u16
}

fn sanitize_adc_calibration(config: &mut AdcCalibrationConfig) {
    compact_channel(&mut config.rtd);
    compact_channel(&mut config.vin);
}

fn sanitize_heater_curve(config: &mut HeaterCurveConfig) {
    let points = compacted_heater_points(config);
    config.points = [None; HEATER_CURVE_MAX_POINTS];
    for (index, point) in points.into_iter().enumerate() {
        config.points[index] = Some(point);
    }
}

fn compact_channel(channel: &mut AdcCalibrationChannelConfig) {
    let mut compacted = [None; ADC_CALIBRATION_MAX_SAMPLES];
    let mut cursor = 0;
    for sample in channel.samples.iter().flatten() {
        if cursor < compacted.len() {
            compacted[cursor] = Some(*sample);
            cursor += 1;
        }
    }
    channel.samples = compacted;
}

fn default_points(channel: AdcCalibrationChannel) -> [AdcCalibrationSample; 2] {
    match channel {
        AdcCalibrationChannel::Rtd => [
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_RTD_DEFAULT_LOW_MV,
                expected_mv: ADC_CALIBRATION_RTD_DEFAULT_LOW_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV,
                expected_mv: ADC_CALIBRATION_RTD_DEFAULT_HIGH_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
        ],
        AdcCalibrationChannel::Vin => [
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_VIN_DEFAULT_LOW_MV,
                expected_mv: ADC_CALIBRATION_VIN_DEFAULT_LOW_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
            AdcCalibrationSample {
                observed_mv: ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV,
                expected_mv: ADC_CALIBRATION_VIN_DEFAULT_HIGH_MV,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: None,
            },
        ],
    }
}

fn fit_channel(
    channel: &AdcCalibrationChannelConfig,
    defaults: [AdcCalibrationSample; 2],
) -> AdcCalibrationFit {
    let custom_count = channel.sample_count();
    let default_count = if custom_count < 2 { 2 } else { 0 };
    let total_count = custom_count + default_count;
    if total_count < 2 {
        return AdcCalibrationFit {
            gain: 1.0,
            offset_mv: 0.0,
            custom_sample_count: custom_count,
            default_sample_count: default_count,
        };
    }

    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    let mut sum_xx = 0.0_f32;
    let mut sum_xy = 0.0_f32;

    if custom_count < 2 {
        for sample in defaults {
            accumulate_fit_point(sample, &mut sum_x, &mut sum_y, &mut sum_xx, &mut sum_xy);
        }
    }
    for sample in channel.samples.iter().flatten() {
        accumulate_fit_point(*sample, &mut sum_x, &mut sum_y, &mut sum_xx, &mut sum_xy);
    }

    let n = total_count as f32;
    let denominator = (n * sum_xx) - (sum_x * sum_x);
    if denominator.abs() < f32::EPSILON {
        let offset_mv = (sum_y - sum_x) / n;
        return AdcCalibrationFit {
            gain: 1.0,
            offset_mv,
            custom_sample_count: custom_count,
            default_sample_count: default_count,
        };
    }

    let gain = ((n * sum_xy) - (sum_x * sum_y)) / denominator;
    let offset_mv = (sum_y - (gain * sum_x)) / n;
    AdcCalibrationFit {
        gain,
        offset_mv,
        custom_sample_count: custom_count,
        default_sample_count: default_count,
    }
}

fn accumulate_fit_point(
    sample: AdcCalibrationSample,
    sum_x: &mut f32,
    sum_y: &mut f32,
    sum_xx: &mut f32,
    sum_xy: &mut f32,
) {
    let x = sample.observed_mv as f32;
    let y = sample.expected_mv as f32;
    *sum_x += x;
    *sum_y += y;
    *sum_xx += x * x;
    *sum_xy += x * y;
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
    let mut calibration_payload = [0u8; ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2];
    encode_adc_calibration(&config.active_adc_calibration, &mut calibration_payload);
    push_tlv(
        TLV_ACTIVE_ADC_CALIBRATION,
        &calibration_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_reference_payload = [0u8; ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2];
    encode_adc_calibration_references(
        &config.active_adc_calibration,
        &mut calibration_reference_payload,
    );
    push_tlv(
        TLV_ACTIVE_ADC_CALIBRATION_REFERENCES,
        &calibration_reference_payload,
        out,
        &mut cursor,
    )?;
    let mut calibration_target_payload = [0u8; ADC_CALIBRATION_MAX_SAMPLES * 2];
    encode_adc_calibration_targets(
        &config.active_adc_calibration,
        &mut calibration_target_payload,
    );
    push_tlv(
        TLV_ACTIVE_ADC_CALIBRATION_TARGETS,
        &calibration_target_payload,
        out,
        &mut cursor,
    )?;
    encode_adc_calibration(&config.draft_adc_calibration, &mut calibration_payload);
    push_tlv(
        TLV_DRAFT_ADC_CALIBRATION,
        &calibration_payload,
        out,
        &mut cursor,
    )?;
    encode_adc_calibration_references(
        &config.draft_adc_calibration,
        &mut calibration_reference_payload,
    );
    push_tlv(
        TLV_DRAFT_ADC_CALIBRATION_REFERENCES,
        &calibration_reference_payload,
        out,
        &mut cursor,
    )?;
    encode_adc_calibration_targets(
        &config.draft_adc_calibration,
        &mut calibration_target_payload,
    );
    push_tlv(
        TLV_DRAFT_ADC_CALIBRATION_TARGETS,
        &calibration_target_payload,
        out,
        &mut cursor,
    )?;
    let mut heater_curve_payload = [0u8; HEATER_CURVE_MAX_POINTS * 4];
    encode_heater_curve(&config.active_heater_curve, &mut heater_curve_payload);
    push_tlv(
        TLV_ACTIVE_HEATER_CURVE,
        &heater_curve_payload,
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
            TLV_ACTIVE_ADC_CALIBRATION if len == ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2 => {
                config.active_adc_calibration = decode_adc_calibration(value);
            }
            TLV_DRAFT_ADC_CALIBRATION if len == ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2 => {
                config.draft_adc_calibration = decode_adc_calibration(value);
            }
            TLV_ACTIVE_ADC_CALIBRATION_REFERENCES
                if len == ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2 =>
            {
                decode_adc_calibration_references(value, &mut config.active_adc_calibration);
            }
            TLV_DRAFT_ADC_CALIBRATION_REFERENCES
                if len == ADC_CALIBRATION_MAX_SAMPLES * 2 * 2 * 2 =>
            {
                decode_adc_calibration_references(value, &mut config.draft_adc_calibration);
            }
            TLV_ACTIVE_ADC_CALIBRATION_TARGETS if len == ADC_CALIBRATION_MAX_SAMPLES * 2 => {
                decode_adc_calibration_targets(value, &mut config.active_adc_calibration);
            }
            TLV_DRAFT_ADC_CALIBRATION_TARGETS if len == ADC_CALIBRATION_MAX_SAMPLES * 2 => {
                decode_adc_calibration_targets(value, &mut config.draft_adc_calibration);
            }
            TLV_ACTIVE_HEATER_CURVE if len == HEATER_CURVE_MAX_POINTS * 4 => {
                config.active_heater_curve = decode_heater_curve(value);
            }
            _ => {}
        }
    }
    Ok(config)
}

fn encode_adc_calibration(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for channel in [&config.rtd, &config.vin] {
        for sample in channel.samples {
            let observed = sample
                .map(|sample| sample.observed_mv)
                .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
            let expected = sample
                .map(|sample| sample.expected_mv)
                .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
            out[cursor..cursor + 2].copy_from_slice(&observed.to_le_bytes());
            out[cursor + 2..cursor + 4].copy_from_slice(&expected.to_le_bytes());
            cursor += 4;
        }
    }
}

fn encode_adc_calibration_references(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for sample in config.rtd.samples {
        let reference = sample
            .and_then(|sample| sample.reference_temp_deci_c)
            .unwrap_or(CALIBRATION_REFERENCE_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&reference.to_le_bytes());
        cursor += 2;
    }
    for sample in config.vin.samples {
        let reference = sample
            .and_then(|sample| sample.reference_vin_mv)
            .and_then(|value| i16::try_from(value).ok())
            .unwrap_or(CALIBRATION_REFERENCE_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&reference.to_le_bytes());
        cursor += 2;
    }
}

fn encode_adc_calibration_targets(config: &AdcCalibrationConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for sample in config.rtd.samples {
        let target = sample
            .and_then(|sample| sample.target_adc_mv)
            .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&target.to_le_bytes());
        cursor += 2;
    }
}

fn decode_adc_calibration(bytes: &[u8]) -> AdcCalibrationConfig {
    let mut config = AdcCalibrationConfig::default();
    let mut cursor = 0;
    for channel in [&mut config.rtd, &mut config.vin] {
        for slot in channel.samples.iter_mut() {
            let observed = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            let expected = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
            *slot = if observed == CALIBRATION_NONE_WIRE_VALUE
                || expected == CALIBRATION_NONE_WIRE_VALUE
            {
                None
            } else {
                Some(AdcCalibrationSample {
                    observed_mv: observed,
                    expected_mv: expected,
                    reference_temp_deci_c: None,
                    target_adc_mv: None,
                    reference_vin_mv: None,
                })
            };
            cursor += 4;
        }
    }
    config
}

fn decode_adc_calibration_references(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    let mut cursor = 0;
    for slot in config.rtd.samples.iter_mut() {
        let reference = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.reference_temp_deci_c =
                (reference != CALIBRATION_REFERENCE_NONE_WIRE_VALUE).then_some(reference);
            sample.target_adc_mv = None;
            sample.reference_vin_mv = None;
        }
        cursor += 2;
    }
    for slot in config.vin.samples.iter_mut() {
        let reference = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.reference_vin_mv =
                (reference != CALIBRATION_REFERENCE_NONE_WIRE_VALUE).then_some(reference as u16);
            sample.reference_temp_deci_c = None;
            sample.target_adc_mv = None;
        }
        cursor += 2;
    }
}

fn decode_adc_calibration_targets(bytes: &[u8], config: &mut AdcCalibrationConfig) {
    let mut cursor = 0;
    for slot in config.rtd.samples.iter_mut() {
        let target = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        if let Some(sample) = slot.as_mut() {
            sample.target_adc_mv = (target != CALIBRATION_NONE_WIRE_VALUE).then_some(target);
        }
        cursor += 2;
    }
}

fn encode_heater_curve(config: &HeaterCurveConfig, out: &mut [u8]) {
    let mut cursor = 0;
    for point in config.points {
        let temp = point
            .map(|point| point.temp_centi_c)
            .unwrap_or(PRESET_NONE_WIRE_VALUE);
        let resistance = point
            .map(|point| point.resistance_milliohms)
            .unwrap_or(CALIBRATION_NONE_WIRE_VALUE);
        out[cursor..cursor + 2].copy_from_slice(&temp.to_le_bytes());
        out[cursor + 2..cursor + 4].copy_from_slice(&resistance.to_le_bytes());
        cursor += 4;
    }
}

fn decode_heater_curve(bytes: &[u8]) -> HeaterCurveConfig {
    let mut config = HeaterCurveConfig::default();
    let mut cursor = 0;
    for slot in config.points.iter_mut() {
        let temp = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
        let resistance = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
        *slot = if temp == PRESET_NONE_WIRE_VALUE || resistance == CALIBRATION_NONE_WIRE_VALUE {
            None
        } else {
            Some(HeaterCurvePoint {
                temp_centi_c: temp,
                resistance_milliohms: resistance,
            })
        };
        cursor += 4;
    }
    config
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
            .draft_adc_calibration
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_030,
                reference_temp_deci_c: Some(250),
                target_adc_mv: Some(970),
                reference_vin_mv: None,
            })
            .unwrap();
        config
            .active_adc_calibration
            .vin
            .insert(AdcCalibrationSample {
                observed_mv: 1_800,
                expected_mv: 1_760,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: Some(20_000),
            })
            .unwrap();
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
        assert_eq!(config.active_adc_calibration.rtd.sample_count(), 0);
        assert_eq!(config.draft_adc_calibration.vin.sample_count(), 0);
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
        assert_eq!(
            decoded.config.draft_adc_calibration.rtd.samples[0]
                .and_then(|sample| sample.reference_temp_deci_c),
            Some(250)
        );
        assert_eq!(
            decoded.config.draft_adc_calibration.rtd.samples[0]
                .and_then(|sample| sample.target_adc_mv),
            Some(970)
        );
        assert_eq!(
            decoded.config.active_adc_calibration.vin.samples[0]
                .and_then(|sample| sample.reference_vin_mv),
            Some(20_000)
        );
    }

    #[test]
    fn decode_legacy_adc_calibration_without_reference_tlvs_keeps_samples() {
        let mut bytes = [0u8; MEMORY_SLOT_SIZE];
        let record = MemoryRecord {
            sequence: 43,
            config: sample_config(),
        };
        let _len = encode_memory_record(&record, &mut bytes).unwrap();
        let payload_len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let payload_start = MEMORY_RECORD_HEADER_LEN;
        let payload_end = payload_start + payload_len;
        let payload = bytes[payload_start..payload_end].to_vec();
        let mut filtered_payload = [0u8; MEMORY_RECORD_PAYLOAD_MAX];
        let mut filtered_len = 0usize;
        let mut cursor = 0usize;
        while cursor < payload.len() {
            let tag = payload[cursor];
            let value_len = payload[cursor + 1] as usize;
            let tlv_len = 2 + value_len;
            if tag != TLV_ACTIVE_ADC_CALIBRATION_REFERENCES
                && tag != TLV_DRAFT_ADC_CALIBRATION_REFERENCES
                && tag != TLV_ACTIVE_ADC_CALIBRATION_TARGETS
                && tag != TLV_DRAFT_ADC_CALIBRATION_TARGETS
            {
                filtered_payload[filtered_len..filtered_len + tlv_len]
                    .copy_from_slice(&payload[cursor..cursor + tlv_len]);
                filtered_len += tlv_len;
            }
            cursor += tlv_len;
        }

        bytes[6..8].copy_from_slice(&(filtered_len as u16).to_le_bytes());
        bytes[payload_start..payload_start + filtered_len]
            .copy_from_slice(&filtered_payload[..filtered_len]);
        let crc = crc32_update(
            crc32(&bytes[0..12]),
            &bytes[payload_start..payload_start + filtered_len],
        ) ^ 0xffff_ffff;
        bytes[12..16].copy_from_slice(&crc.to_le_bytes());

        let decoded = decode_memory_record(&bytes[..payload_start + filtered_len]).unwrap();
        let draft_rtd = decoded.config.draft_adc_calibration.rtd.samples[0].unwrap();
        let active_vin = decoded.config.active_adc_calibration.vin.samples[0].unwrap();
        assert_eq!(draft_rtd.observed_mv, 1_000);
        assert_eq!(draft_rtd.expected_mv, 1_030);
        assert_eq!(draft_rtd.reference_temp_deci_c, None);
        assert_eq!(draft_rtd.target_adc_mv, None);
        assert_eq!(draft_rtd.reference_vin_mv, None);
        assert_eq!(active_vin.observed_mv, 1_800);
        assert_eq!(active_vin.expected_mv, 1_760);
        assert_eq!(active_vin.reference_temp_deci_c, None);
        assert_eq!(active_vin.target_adc_mv, None);
        assert_eq!(active_vin.reference_vin_mv, None);
    }

    #[test]
    fn adc_calibration_fit_uses_default_identity_without_custom_samples() {
        let config = AdcCalibrationConfig::default();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Vin);
        assert_eq!(fit.custom_sample_count, 0);
        assert_eq!(fit.default_sample_count, 2);
        assert!((fit.gain - 1.0).abs() < 0.0001);
        assert!(fit.offset_mv.abs() < 0.0001);
        assert_eq!(
            correct_adc_mv(&config, AdcCalibrationChannel::Vin, 1_234),
            1_234
        );
    }

    #[test]
    fn adc_calibration_fit_mixes_default_points_for_single_custom_sample() {
        let mut config = AdcCalibrationConfig::default();
        config
            .vin
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_100,
                reference_temp_deci_c: None,
                target_adc_mv: None,
                reference_vin_mv: Some(12_000),
            })
            .unwrap();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Vin);
        assert_eq!(fit.custom_sample_count, 1);
        assert_eq!(fit.default_sample_count, 2);
        assert!(correct_adc_mv(&config, AdcCalibrationChannel::Vin, 1_000) > 1_000);
    }

    #[test]
    fn adc_calibration_fit_ignores_default_points_after_two_custom_samples() {
        let mut config = AdcCalibrationConfig::default();
        config
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 1_000,
                expected_mv: 1_100,
                reference_temp_deci_c: Some(250),
                target_adc_mv: Some(900),
                reference_vin_mv: None,
            })
            .unwrap();
        config
            .rtd
            .insert(AdcCalibrationSample {
                observed_mv: 2_000,
                expected_mv: 2_200,
                reference_temp_deci_c: Some(500),
                target_adc_mv: Some(1_700),
                reference_vin_mv: None,
            })
            .unwrap();
        let fit = adc_calibration_fit(&config, AdcCalibrationChannel::Rtd);
        assert_eq!(fit.custom_sample_count, 2);
        assert_eq!(fit.default_sample_count, 0);
        assert!((fit.gain - 1.1).abs() < 0.001);
        assert!(fit.offset_mv.abs() < 0.001);
    }

    #[test]
    fn adc_calibration_channel_caps_at_eight_samples_and_compacts_on_sanitize() {
        let mut config = AdcCalibrationConfig::default();
        for index in 0..ADC_CALIBRATION_MAX_SAMPLES {
            assert_eq!(
                config.rtd.insert(AdcCalibrationSample {
                    observed_mv: index as u16,
                    expected_mv: index as u16,
                    reference_temp_deci_c: Some(index as i16),
                    target_adc_mv: Some(index as u16),
                    reference_vin_mv: None,
                }),
                Some(index)
            );
        }
        assert_eq!(
            config.rtd.insert(AdcCalibrationSample {
                observed_mv: 9,
                expected_mv: 9,
                reference_temp_deci_c: Some(9),
                target_adc_mv: Some(9),
                reference_vin_mv: None,
            }),
            None
        );
        assert!(config.rtd.delete(3));
        sanitize_adc_calibration(&mut config);
        assert_eq!(config.rtd.samples[3].unwrap().observed_mv, 4);
        assert!(config.rtd.samples[ADC_CALIBRATION_MAX_SAMPLES - 1].is_none());
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
