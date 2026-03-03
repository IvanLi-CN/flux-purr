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
        }
    }
}

pub const STATUS_REGISTER: u8 = 0x09;
pub const VOLTAGE_CONTROL_REGISTER: u8 = 0x0A;

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
    }

    #[test]
    fn reports_requested_voltage_in_millivolts() {
        assert_eq!(VoltageRequest::V28.millivolts(), 28_000);
    }
}
