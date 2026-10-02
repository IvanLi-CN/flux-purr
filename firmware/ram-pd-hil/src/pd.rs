//! Allocation-free USB-PD policy and wire-codec helpers for the HIL image.

#![cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]

pub const FIXED_TARGETS_MV: [u16; 5] = [5_000, 9_000, 12_000, 15_000, 20_000];
pub const PPS_TARGETS_MV: [u16; 17] = [
    5_000, 6_000, 7_000, 8_000, 9_000, 10_000, 11_000, 12_000, 13_000, 14_000, 15_000, 16_000,
    17_000, 18_000, 19_000, 20_000, 21_000,
];
pub const TOTAL_TIERS: usize = FIXED_TARGETS_MV.len() + PPS_TARGETS_MV.len();
pub const MAX_SOURCE_PDOS: usize = 7;
pub const MIN_CURRENT_MA: u16 = 3_000;
pub const CURRENT_CEILING_MA: u16 = 5_000;
pub const HOLD_MS: u32 = 2_000;
pub const SAMPLE_INTERVAL_MS: u32 = 50;
pub const DEFAULT_CONTRACT_MV: u16 = 5_000;
pub const DEFAULT_VERIFY_MS: u32 = 500;
pub const DEFAULT_MIN_MV: u16 = 4_750;
pub const DEFAULT_MAX_MV: u16 = 5_250;
const PD_HEADER_SPEC_REV_30: u16 = 2 << 6;
const PD_HEADER_SPEC_REV_MASK: u16 = 0b11 << 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fixed,
    Pps,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Pps => "pps",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub mode: Mode,
    pub voltage_mv: u16,
}

pub const fn target_at(index: usize) -> Option<Target> {
    if index < FIXED_TARGETS_MV.len() {
        Some(Target {
            mode: Mode::Fixed,
            voltage_mv: FIXED_TARGETS_MV[index],
        })
    } else if index < TOTAL_TIERS {
        Some(Target {
            mode: Mode::Pps,
            voltage_mv: PPS_TARGETS_MV[index - FIXED_TARGETS_MV.len()],
        })
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceObject {
    pub raw: u32,
    pub position: u8,
    pub mode: Mode,
    pub min_mv: u16,
    pub max_mv: u16,
    pub max_ma: u16,
}

impl SourceObject {
    const EMPTY: Self = Self {
        raw: 0,
        position: 0,
        mode: Mode::Fixed,
        min_mv: 0,
        max_mv: 0,
        max_ma: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCapabilities {
    pub objects: [SourceObject; MAX_SOURCE_PDOS],
    pub count: u8,
}

impl SourceCapabilities {
    pub const fn empty() -> Self {
        Self {
            objects: [SourceObject::EMPTY; MAX_SOURCE_PDOS],
            count: 0,
        }
    }

    pub fn select(self, target: Target) -> Result<SelectedContract, UnsupportedReason> {
        if !target_in_hil_envelope(target) {
            return Err(UnsupportedReason::OutsideHilEnvelope);
        }

        let mut matched_voltage = false;
        for index in 0..usize::from(self.count) {
            let object = self.objects[index];
            let voltage_matches = match target.mode {
                Mode::Fixed => object.mode == Mode::Fixed && object.min_mv == target.voltage_mv,
                Mode::Pps => {
                    object.mode == Mode::Pps
                        && object.min_mv <= target.voltage_mv
                        && target.voltage_mv <= object.max_mv
                }
            };
            if !voltage_matches {
                continue;
            }
            matched_voltage = true;
            let current_ma = object.max_ma.min(CURRENT_CEILING_MA);
            if current_ma < MIN_CURRENT_MA {
                continue;
            }
            return Ok(SelectedContract {
                target,
                object,
                contract_current_ma: current_ma,
            });
        }

        if matched_voltage {
            Err(UnsupportedReason::SourceCurrentBelow3000Ma)
        } else if target.mode == Mode::Fixed {
            Err(UnsupportedReason::NoExactFixedPdo)
        } else {
            Err(UnsupportedReason::NoExactPpsApdo)
        }
    }

    pub fn select_default_5v(self) -> Option<SelectedContract> {
        for index in 0..usize::from(self.count) {
            let object = self.objects[index];
            if object.mode == Mode::Fixed
                && object.min_mv == DEFAULT_CONTRACT_MV
                && object.max_ma > 0
            {
                return Some(SelectedContract {
                    target: Target {
                        mode: Mode::Fixed,
                        voltage_mv: DEFAULT_CONTRACT_MV,
                    },
                    object,
                    contract_current_ma: object.max_ma.min(CURRENT_CEILING_MA),
                });
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedContract {
    pub target: Target,
    pub object: SourceObject,
    pub contract_current_ma: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    OutsideHilEnvelope,
    NoExactFixedPdo,
    NoExactPpsApdo,
    SourceCurrentBelow3000Ma,
}

impl UnsupportedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutsideHilEnvelope => "outside_hil_envelope",
            Self::NoExactFixedPdo => "no_exact_fixed_pdo",
            Self::NoExactPpsApdo => "no_exact_pps_apdo",
            Self::SourceCurrentBelow3000Ma => "source_current_below_3000ma",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    WrongMessageType,
    EmptyMessage,
    ObjectCountTooLarge,
    PayloadLengthMismatch,
}

pub fn decode_source_capabilities(
    header: u16,
    payload: &[u8],
) -> Result<SourceCapabilities, DecodeError> {
    if message_type(header) != 1 {
        return Err(DecodeError::WrongMessageType);
    }
    let count = usize::from(object_count(header));
    if count == 0 {
        return Err(DecodeError::EmptyMessage);
    }
    if count > MAX_SOURCE_PDOS {
        return Err(DecodeError::ObjectCountTooLarge);
    }
    if payload.len() != count * 4 {
        return Err(DecodeError::PayloadLengthMismatch);
    }

    let mut capabilities = SourceCapabilities::empty();
    capabilities.count = count as u8;
    for index in 0..count {
        let offset = index * 4;
        let raw = u32::from_le_bytes([
            payload[offset],
            payload[offset + 1],
            payload[offset + 2],
            payload[offset + 3],
        ]);
        if let Some(object) = decode_object(raw, (index + 1) as u8) {
            capabilities.objects[index] = object;
        }
    }
    Ok(capabilities)
}

fn decode_object(raw: u32, position: u8) -> Option<SourceObject> {
    match raw >> 30 {
        0 => {
            let voltage_mv = (((raw >> 10) & 0x03ff) as u16) * 50;
            let max_ma = ((raw & 0x03ff) as u16) * 10;
            Some(SourceObject {
                raw,
                position,
                mode: Mode::Fixed,
                min_mv: voltage_mv,
                max_mv: voltage_mv,
                max_ma,
            })
        }
        3 if ((raw >> 28) & 0x03) == 0 => {
            let min_mv = (((raw >> 8) & 0xff) as u16) * 100;
            let max_mv = (((raw >> 17) & 0xff) as u16) * 100;
            let max_ma = ((raw & 0x7f) as u16) * 50;
            Some(SourceObject {
                raw,
                position,
                mode: Mode::Pps,
                min_mv,
                max_mv,
                max_ma,
            })
        }
        _ => None,
    }
}

pub fn target_in_hil_envelope(target: Target) -> bool {
    match target.mode {
        Mode::Fixed => matches!(target.voltage_mv, 5_000 | 9_000 | 12_000 | 15_000 | 20_000),
        Mode::Pps => {
            target.voltage_mv >= 5_000
                && target.voltage_mv <= 21_000
                && target.voltage_mv.is_multiple_of(1_000)
        }
    }
}

pub fn tolerance_mv(target_mv: u16) -> u16 {
    let percent = (u32::from(target_mv) * 25).div_ceil(1_000) as u16;
    if percent > 250 { percent } else { 250 }
}

pub fn request_header(message_id: u8) -> u16 {
    2 | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9) | (1 << 12)
}

pub fn accept_header(message_id: u8) -> u16 {
    3 | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9)
}

pub fn soft_reset_header(message_id: u8) -> u16 {
    13 | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9)
}

pub fn get_source_capabilities_header(message_id: u8) -> u16 {
    get_source_capabilities_header_with_spec_revision(message_id, PD_HEADER_SPEC_REV_30)
}

pub const fn get_source_capabilities_header_with_spec_revision(
    message_id: u8,
    spec_revision: u16,
) -> u16 {
    7 | (spec_revision & PD_HEADER_SPEC_REV_MASK) | (((message_id & 0x07) as u16) << 9)
}

pub fn request_data_object(contract: SelectedContract) -> [u8; 4] {
    let raw = match contract.target.mode {
        Mode::Fixed => {
            let current_units = u32::from(contract.contract_current_ma / 10);
            (u32::from(contract.object.position) << 28)
                | (1 << 24)
                | (current_units << 10)
                | current_units
        }
        Mode::Pps => {
            let voltage_units = u32::from(contract.target.voltage_mv / 20);
            let current_units = u32::from(contract.contract_current_ma / 50);
            (u32::from(contract.object.position) << 28)
                | (1 << 24)
                | ((voltage_units & 0x0fff) << 9)
                | (current_units & 0x7f)
        }
    };
    raw.to_le_bytes()
}

pub const fn message_type(header: u16) -> u8 {
    (header & 0x1f) as u8
}

pub const fn object_count(header: u16) -> u8 {
    ((header >> 12) & 0x07) as u8
}

pub const fn message_id(header: u16) -> u8 {
    ((header >> 9) & 0x07) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_pdo(voltage_mv: u16, max_ma: u16) -> u32 {
        (u32::from(voltage_mv / 50) << 10) | u32::from(max_ma / 10)
    }

    fn pps_apdo(min_mv: u16, max_mv: u16, max_ma: u16) -> u32 {
        (3 << 30)
            | (u32::from(max_mv / 100) << 17)
            | (u32::from(min_mv / 100) << 8)
            | u32::from(max_ma / 50)
    }

    #[test]
    fn source_capabilities_decode_fixed_and_pps_objects() {
        let raw = [fixed_pdo(9_000, 3_000), pps_apdo(5_000, 21_000, 5_000)];
        let header = (2 << 12) | 1;
        let payload = [raw[0].to_le_bytes(), raw[1].to_le_bytes()];
        let bytes = [
            payload[0][0],
            payload[0][1],
            payload[0][2],
            payload[0][3],
            payload[1][0],
            payload[1][1],
            payload[1][2],
            payload[1][3],
        ];
        let caps = decode_source_capabilities(header, &bytes).unwrap();
        assert_eq!(caps.objects[0].max_mv, 9_000);
        assert_eq!(caps.objects[1].min_mv, 5_000);
        assert_eq!(caps.objects[1].max_mv, 21_000);
    }

    #[test]
    fn selection_is_exact_and_caps_current_at_five_amps() {
        let raw = [fixed_pdo(9_000, 6_000), pps_apdo(5_000, 21_000, 2_000)];
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&raw[0].to_le_bytes());
        bytes[4..].copy_from_slice(&raw[1].to_le_bytes());
        let caps = decode_source_capabilities((2 << 12) | 1, &bytes).unwrap();
        let fixed = caps
            .select(Target {
                mode: Mode::Fixed,
                voltage_mv: 9_000,
            })
            .unwrap();
        assert_eq!(fixed.contract_current_ma, 5_000);
        assert_eq!(
            caps.select(Target {
                mode: Mode::Pps,
                voltage_mv: 5_000,
            }),
            Err(UnsupportedReason::SourceCurrentBelow3000Ma)
        );
        assert_eq!(
            caps.select(Target {
                mode: Mode::Fixed,
                voltage_mv: 12_000,
            }),
            Err(UnsupportedReason::NoExactFixedPdo)
        );
    }

    #[test]
    fn default_5v_recovery_does_not_require_the_hil_three_amp_floor() {
        let raw = fixed_pdo(DEFAULT_CONTRACT_MV, 500);
        let bytes = raw.to_le_bytes();
        let caps = decode_source_capabilities((1 << 12) | 1, &bytes).unwrap();
        let selected = caps.select_default_5v().unwrap();
        assert_eq!(selected.contract_current_ma, 500);
        assert_eq!(selected.target.voltage_mv, DEFAULT_CONTRACT_MV);
    }

    #[test]
    fn request_encoding_matches_production_fusb302b_layout() {
        let contract = SelectedContract {
            target: Target {
                mode: Mode::Pps,
                voltage_mv: 12_000,
            },
            object: SourceObject {
                raw: 0,
                position: 2,
                mode: Mode::Pps,
                min_mv: 5_000,
                max_mv: 21_000,
                max_ma: 5_000,
            },
            contract_current_ma: 5_000,
        };
        let raw = u32::from_le_bytes(request_data_object(contract));
        assert_eq!(raw >> 28, 2);
        assert_eq!((raw >> 9) & 0x0fff, 600);
        assert_eq!(raw & 0x7f, 100);
        assert_eq!(request_header(5), 0x1a82);
        assert_eq!(soft_reset_header(1), 0x28d);
        assert_eq!(get_source_capabilities_header(1), 0x287);
        assert_eq!(
            get_source_capabilities_header_with_spec_revision(1, 0),
            0x207
        );
    }

    #[test]
    fn tolerance_uses_two_point_five_percent_with_a_250mv_floor() {
        assert_eq!(tolerance_mv(5_000), 250);
        assert_eq!(tolerance_mv(20_000), 500);
        assert!(target_in_hil_envelope(Target {
            mode: Mode::Pps,
            voltage_mv: 5_000,
        }));
        assert!(!target_in_hil_envelope(Target {
            mode: Mode::Pps,
            voltage_mv: 5_500,
        }));
    }
}
