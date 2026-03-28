pub const PIN_USB_D_MINUS: u8 = 18;
pub const PIN_USB_D_PLUS: u8 = 19;

pub const PIN_I2C_SDA: u8 = 4;
pub const PIN_I2C_SCL: u8 = 5;
pub const PIN_FRONT_PANEL_INT_N: u8 = 2;

pub const PIN_LCD_SCLK: u8 = 21;
pub const PIN_LCD_MOSI: u8 = 20;
pub const PIN_LCD_DC: u8 = 7;

pub const PIN_FAN_PWM: u8 = 3;
pub const PIN_FAN_EN: u8 = 6;
pub const PIN_CENTER_KEY_BOOT: u8 = 9;

pub const PIN_HEATER_PWM: u8 = 10;
pub const PIN_VIN_ADC: u8 = 1;
pub const PIN_TEMP_ADC: u8 = 0;

pub const ACTIVE_GPIO: [u8; 14] = [
    PIN_USB_D_MINUS,
    PIN_USB_D_PLUS,
    PIN_I2C_SDA,
    PIN_I2C_SCL,
    PIN_FRONT_PANEL_INT_N,
    PIN_LCD_SCLK,
    PIN_LCD_MOSI,
    PIN_LCD_DC,
    PIN_FAN_PWM,
    PIN_FAN_EN,
    PIN_CENTER_KEY_BOOT,
    PIN_HEATER_PWM,
    PIN_VIN_ADC,
    PIN_TEMP_ADC,
];

pub const RESERVED_STRAPPING_GPIO: [u8; 1] = [8];

pub const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
pub const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
pub const VIN_DIVIDER_MAX_INPUT_MV: u32 = 28_000;
pub const VIN_DIVIDER_MAX_ADC_MV: u32 = (VIN_DIVIDER_MAX_INPUT_MV * VIN_DIVIDER_R_LOW_OHMS)
    / (VIN_DIVIDER_R_HIGH_OHMS + VIN_DIVIDER_R_LOW_OHMS);

pub fn gpio_map_is_valid() -> bool {
    if ACTIVE_GPIO.len() + RESERVED_STRAPPING_GPIO.len() != 15 {
        return false;
    }

    let mut seen = [false; 22];
    for pin in ACTIVE_GPIO {
        let idx = pin as usize;
        if idx >= seen.len() || seen[idx] {
            return false;
        }
        seen[idx] = true;
    }

    for pin in RESERVED_STRAPPING_GPIO {
        let idx = pin as usize;
        if idx >= seen.len() || seen[idx] {
            return false;
        }
        seen[idx] = true;
    }

    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpio_map_accounts_for_active_and_reserved_lines() {
        assert!(super::gpio_map_is_valid());
        assert_eq!(super::PIN_FAN_EN, 6);
        assert_eq!(super::PIN_CENTER_KEY_BOOT, 9);
        assert_eq!(super::VIN_DIVIDER_MAX_ADC_MV, 2_337);
    }
}
