#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    Primary,
    Secondary,
}

impl Address {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Primary => 0x22,
            Self::Secondary => 0x23,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressError {
    Unsupported(u8),
}

impl core::convert::TryFrom<u8> for Address {
    type Error = AddressError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x22 => Ok(Self::Primary),
            0x23 => Ok(Self::Secondary),
            _ => Err(AddressError::Unsupported(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoltageRequest {
    V5,
    V9,
    V12,
    V15,
    V20,
    V28,
    Pps,
    Avs,
}

impl VoltageRequest {
    pub const fn millivolts(self) -> u16 {
        match self {
            Self::V5 => 5_000,
            Self::V9 => 9_000,
            Self::V12 => 12_000,
            Self::V15 => 15_000,
            Self::V20 => 20_000,
            Self::V28 => 28_000,
            Self::Pps => 0,
            Self::Avs => 0,
        }
    }

    pub const fn control_register_value(self) -> u8 {
        match self {
            Self::V5 => 0,
            Self::V9 => 1,
            Self::V12 => 2,
            Self::V15 => 3,
            Self::V20 => 4,
            Self::V28 => 5,
            Self::Pps => 6,
            Self::Avs => 7,
        }
    }
}

pub const STATUS_REGISTER: u8 = 0x09;
pub const VOLTAGE_CONTROL_REGISTER: u8 = 0x0A;
pub const CURRENT_DATA_REGISTER: u8 = 0x50;
pub const AVS_VOLTAGE_CONFIG_HIGH_REGISTER: u8 = 0x51;
pub const AVS_VOLTAGE_CONFIG_LOW_REGISTER: u8 = 0x52;
pub const PPS_VOLTAGE_CONFIG_REGISTER: u8 = 0x53;
pub const PD_POWER_DATA_START_REGISTER: u8 = 0x60;
pub const PD_POWER_DATA_REGISTER_COUNT: usize = 0x30;
pub const PPS_GATE_MV: u16 = 20_000;
pub const CH224Q_PPS_MAX_MV: u16 = 21_000;
pub const CH224Q_AVS_MAX_MV: u16 = 28_000;

pub const fn voltage_request_payload(request: VoltageRequest) -> [u8; 2] {
    [VOLTAGE_CONTROL_REGISTER, request.control_register_value()]
}

pub const fn pps_voltage_payload(millivolts: u16) -> Option<[u8; 2]> {
    let units = millivolts / 100;
    if !millivolts.is_multiple_of(100) || units > u8::MAX as u16 {
        return None;
    }

    Some([PPS_VOLTAGE_CONFIG_REGISTER, units as u8])
}

pub const fn avs_voltage_payloads(millivolts: u16) -> Option<([u8; 2], [u8; 2])> {
    let units = millivolts / 25;
    if !millivolts.is_multiple_of(25) {
        return None;
    }

    Some((
        [AVS_VOLTAGE_CONFIG_HIGH_REGISTER, (units >> 8) as u8],
        [AVS_VOLTAGE_CONFIG_LOW_REGISTER, (units & 0xff) as u8],
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustableVoltageMode {
    Pps,
    Avs,
}

impl AdjustableVoltageMode {
    pub const fn control_request(self) -> VoltageRequest {
        match self {
            Self::Pps => VoltageRequest::Pps,
            Self::Avs => VoltageRequest::Avs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub bc_active: bool,
    pub qc2_active: bool,
    pub qc3_active: bool,
    pub pd_active: bool,
    pub epr_active: bool,
    pub epr_exist: bool,
    pub avs_exist: bool,
}

impl Status {
    pub const fn from_register(value: u8) -> Self {
        Self {
            bc_active: (value & (1 << 0)) != 0,
            qc2_active: (value & (1 << 1)) != 0,
            qc3_active: (value & (1 << 2)) != 0,
            pd_active: (value & (1 << 3)) != 0,
            epr_active: (value & (1 << 4)) != 0,
            epr_exist: (value & (1 << 5)) != 0,
            avs_exist: (value & (1 << 6)) != 0,
        }
    }
}

pub const fn current_ma_from_register(value: u8) -> u16 {
    (value as u16) * 50
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdjustablePowerCapabilities {
    pub fixed_12v: bool,
    pub fixed_20v: bool,
    pub pps_covers_20v: bool,
    pub pps_min_mv: Option<u16>,
    pub pps_max_mv: Option<u16>,
    pub avs_min_mv: Option<u16>,
    pub avs_max_mv: Option<u16>,
}

impl AdjustablePowerCapabilities {
    pub fn from_pd_power_data(bytes: &[u8]) -> Self {
        let mut capabilities = Self::default();
        let mut parsed_source_cap = false;

        if let Some((pdo_offset, pdo_count)) = source_cap_pdo_window(bytes) {
            parsed_source_cap = true;
            for pdo_index in 0..pdo_count {
                let offset = pdo_offset + pdo_index * 4;
                Self::merge_power_data_object(
                    &mut capabilities,
                    u32::from_le_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]),
                );
            }
        }

        if !parsed_source_cap {
            for chunk in bytes.chunks_exact(4) {
                Self::merge_power_data_object(
                    &mut capabilities,
                    u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
                );
            }
        }

        capabilities
    }

    fn merge_power_data_object(capabilities: &mut Self, raw: u32) {
        if raw == 0 {
            return;
        }

        match pd_object_type(raw) {
            0b00 => match fixed_pdo_mv(raw) {
                12_000 => capabilities.fixed_12v = true,
                20_000 => capabilities.fixed_20v = true,
                _ => {}
            },
            0b11 => match augmented_pd_object_type(raw) {
                0 => {
                    let min_mv = pps_apdo_min_mv(raw);
                    let max_mv = pps_apdo_max_mv(raw);
                    if min_mv == 0 || max_mv == 0 || max_mv < min_mv {
                        return;
                    }

                    capabilities.pps_min_mv = Some(match capabilities.pps_min_mv {
                        Some(previous) => previous.min(min_mv),
                        None => min_mv,
                    });
                    capabilities.pps_max_mv = Some(match capabilities.pps_max_mv {
                        Some(previous) => previous.max(max_mv),
                        None => max_mv,
                    });
                    capabilities.pps_covers_20v |= min_mv <= PPS_GATE_MV && max_mv >= PPS_GATE_MV;
                }
                1 => {
                    let min_mv = epr_avs_apdo_min_mv(raw);
                    let max_mv = epr_avs_apdo_max_mv(raw);
                    if min_mv == 0 || max_mv == 0 || max_mv < min_mv {
                        return;
                    }

                    capabilities.avs_min_mv = Some(match capabilities.avs_min_mv {
                        Some(previous) => previous.min(min_mv),
                        None => min_mv,
                    });
                    capabilities.avs_max_mv = Some(match capabilities.avs_max_mv {
                        Some(previous) => previous.max(max_mv),
                        None => max_mv,
                    });
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn source_cap_pdo_window(bytes: &[u8]) -> Option<(usize, usize)> {
    if bytes.len() < 2 {
        return None;
    }

    let header = u16::from_le_bytes([bytes[0], bytes[1]]);
    let pdo_count = source_cap_pdo_count(header);
    let pdo_offset = if source_cap_is_extended(header) { 4 } else { 2 };
    if pdo_count == 0 || bytes.len() < pdo_offset + pdo_count * 4 {
        return None;
    }

    Some((pdo_offset, pdo_count))
}

const fn source_cap_pdo_count(header: u16) -> usize {
    ((header >> 12) & 0b111) as usize
}

const fn source_cap_is_extended(header: u16) -> bool {
    (header & (1 << 15)) != 0
}

const fn pd_object_type(raw: u32) -> u32 {
    (raw >> 30) & 0b11
}

const fn augmented_pd_object_type(raw: u32) -> u32 {
    (raw >> 28) & 0b11
}

const fn fixed_pdo_mv(raw: u32) -> u16 {
    (((raw >> 10) & 0x3ff) as u16) * 50
}

const fn pps_apdo_min_mv(raw: u32) -> u16 {
    (((raw >> 8) & 0xff) as u16) * 100
}

const fn pps_apdo_max_mv(raw: u32) -> u16 {
    (((raw >> 17) & 0xff) as u16) * 100
}

const fn epr_avs_apdo_min_mv(raw: u32) -> u16 {
    (((raw >> 8) & 0xff) as u16) * 100
}

const fn epr_avs_apdo_max_mv(raw: u32) -> u16 {
    (((raw >> 17) & 0x1ff) as u16) * 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_primary_and_secondary_i2c_address() {
        assert_eq!(Address::try_from(0x22), Ok(Address::Primary));
        assert_eq!(Address::try_from(0x23), Ok(Address::Secondary));
        assert_eq!(Address::Primary.as_u8(), 0x22);
        assert_eq!(Address::Secondary.as_u8(), 0x23);
    }

    #[test]
    fn rejects_unknown_i2c_address() {
        assert_eq!(
            Address::try_from(0x24),
            Err(AddressError::Unsupported(0x24))
        );
    }

    #[test]
    fn encodes_voltage_requests_for_control_register() {
        assert_eq!(VoltageRequest::V5.control_register_value(), 0);
        assert_eq!(VoltageRequest::V9.control_register_value(), 1);
        assert_eq!(VoltageRequest::V12.control_register_value(), 2);
        assert_eq!(VoltageRequest::V15.control_register_value(), 3);
        assert_eq!(VoltageRequest::V20.control_register_value(), 4);
        assert_eq!(VoltageRequest::V28.control_register_value(), 5);
        assert_eq!(VoltageRequest::Pps.control_register_value(), 6);
        assert_eq!(VoltageRequest::Avs.control_register_value(), 7);
    }

    #[test]
    fn reports_requested_voltage_in_millivolts() {
        assert_eq!(VoltageRequest::V28.millivolts(), 28_000);
    }

    #[test]
    fn encodes_voltage_request_payload() {
        assert_eq!(voltage_request_payload(VoltageRequest::V12), [0x0A, 2]);
        assert_eq!(voltage_request_payload(VoltageRequest::Pps), [0x0A, 6]);
        assert_eq!(voltage_request_payload(VoltageRequest::Avs), [0x0A, 7]);
    }

    #[test]
    fn encodes_pps_and_avs_voltage_payloads() {
        assert_eq!(pps_voltage_payload(20_000), Some([0x53, 200]));
        assert_eq!(pps_voltage_payload(28_000), None);
        assert_eq!(
            avs_voltage_payloads(28_000),
            Some(([0x51, 0x04], [0x52, 0x60]))
        );
        assert_eq!(avs_voltage_payloads(28_010), None);
    }

    #[test]
    fn decodes_status_register_bits() {
        let status = Status::from_register(0b0011_1001);
        assert!(status.bc_active);
        assert!(!status.qc2_active);
        assert!(!status.qc3_active);
        assert!(status.pd_active);
        assert!(status.epr_active);
        assert!(status.epr_exist);
        assert!(!status.avs_exist);
    }

    #[test]
    fn converts_current_register_to_ma() {
        assert_eq!(current_ma_from_register(0), 0);
        assert_eq!(current_ma_from_register(10), 500);
        assert_eq!(current_ma_from_register(65), 3_250);
    }

    #[test]
    fn detects_pps_apdo_that_covers_20v() {
        let pps_apdo = (0b11_u32 << 30) | (210_u32 << 17) | (33_u32 << 8) | 60_u32;
        let bytes = pps_apdo.to_le_bytes();
        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(!capabilities.fixed_12v);
        assert!(!capabilities.fixed_20v);
        assert!(capabilities.pps_covers_20v);
        assert_eq!(capabilities.pps_min_mv, Some(3_300));
        assert_eq!(capabilities.pps_max_mv, Some(21_000));
        assert_eq!(capabilities.avs_min_mv, None);
        assert_eq!(capabilities.avs_max_mv, None);
    }

    #[test]
    fn detects_pps_apdo_after_source_cap_header() {
        let fixed_5v_3a = (100_u32 << 10) | 300_u32;
        let pps_apdo = (0b11_u32 << 30) | (210_u32 << 17) | (33_u32 << 8) | 60_u32;
        let header = (2_u16 << 12) | 1;
        let mut bytes = [0_u8; 10];
        bytes[0..2].copy_from_slice(&header.to_le_bytes());
        bytes[2..6].copy_from_slice(&fixed_5v_3a.to_le_bytes());
        bytes[6..10].copy_from_slice(&pps_apdo.to_le_bytes());

        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(capabilities.pps_covers_20v);
        assert_eq!(capabilities.pps_min_mv, Some(3_300));
        assert_eq!(capabilities.pps_max_mv, Some(21_000));
    }

    #[test]
    fn ignores_bytes_after_declared_source_cap_objects() {
        let fixed_5v_3a = (100_u32 << 10) | 300_u32;
        let pps_apdo = (0b11_u32 << 30) | (210_u32 << 17) | (33_u32 << 8) | 60_u32;
        let header = (1_u16 << 12) | 1;
        let mut bytes = [0_u8; 10];
        bytes[0..2].copy_from_slice(&header.to_le_bytes());
        bytes[2..6].copy_from_slice(&fixed_5v_3a.to_le_bytes());
        bytes[6..10].copy_from_slice(&pps_apdo.to_le_bytes());

        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(!capabilities.pps_covers_20v);
        assert_eq!(capabilities.pps_min_mv, None);
        assert_eq!(capabilities.pps_max_mv, None);
    }

    #[test]
    fn detects_epr_avs_apdo_voltage_range() {
        let epr_avs_apdo =
            (0b11_u32 << 30) | (0b01_u32 << 28) | (280_u32 << 17) | (150_u32 << 8) | 80_u32;
        let bytes = epr_avs_apdo.to_le_bytes();
        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(!capabilities.pps_covers_20v);
        assert_eq!(capabilities.avs_min_mv, Some(15_000));
        assert_eq!(capabilities.avs_max_mv, Some(28_000));
    }

    #[test]
    fn rejects_fixed_pd_as_pps_capability() {
        let fixed_20v_pdo = (400_u32 << 10) | 150_u32;
        let bytes = fixed_20v_pdo.to_le_bytes();
        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(!capabilities.fixed_12v);
        assert!(capabilities.fixed_20v);
        assert!(!capabilities.pps_covers_20v);
        assert_eq!(capabilities.pps_min_mv, None);
        assert_eq!(capabilities.pps_max_mv, None);
        assert_eq!(capabilities.avs_min_mv, None);
        assert_eq!(capabilities.avs_max_mv, None);
    }

    #[test]
    fn rejects_pps_apdo_that_does_not_cover_20v() {
        let pps_apdo = (0b11_u32 << 30) | (150_u32 << 17) | (33_u32 << 8) | 60_u32;
        let bytes = pps_apdo.to_le_bytes();
        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(!capabilities.pps_covers_20v);
        assert_eq!(capabilities.pps_min_mv, Some(3_300));
        assert_eq!(capabilities.pps_max_mv, Some(15_000));
    }

    #[test]
    fn detects_fixed_12v_and_20v_pdos_after_source_cap_header() {
        let fixed_5v_3a = (100_u32 << 10) | 300_u32;
        let fixed_12v_3a = (240_u32 << 10) | 300_u32;
        let fixed_20v_2a = (400_u32 << 10) | 200_u32;
        let header = (3_u16 << 12) | 1;
        let mut bytes = [0_u8; 14];
        bytes[0..2].copy_from_slice(&header.to_le_bytes());
        bytes[2..6].copy_from_slice(&fixed_5v_3a.to_le_bytes());
        bytes[6..10].copy_from_slice(&fixed_12v_3a.to_le_bytes());
        bytes[10..14].copy_from_slice(&fixed_20v_2a.to_le_bytes());

        let capabilities = AdjustablePowerCapabilities::from_pd_power_data(&bytes);

        assert!(capabilities.fixed_12v);
        assert!(capabilities.fixed_20v);
        assert!(!capabilities.pps_covers_20v);
    }
}
