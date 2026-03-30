pub const PIN_CENTER_KEY_BOOT: u8 = 0;
pub const PIN_VIN_ADC: u8 = 1;
pub const PIN_RTD_ADC: u8 = 2;
pub const PIN_TEMP_ADC: u8 = PIN_RTD_ADC;

pub const PIN_HEATER_PWM: u8 = 47;

pub const PIN_I2C_SDA: u8 = 8;
pub const PIN_I2C_SCL: u8 = 9;

pub const PIN_LCD_DC: u8 = 10;
pub const PIN_LCD_MOSI: u8 = 11;
pub const PIN_LCD_SCLK: u8 = 12;
pub const PIN_LCD_BLK: u8 = 13;
pub const PIN_LCD_RES: u8 = 14;
pub const PIN_LCD_CS: u8 = 15;
pub const PIN_KEY_RIGHT: u8 = 16;

pub const PIN_KEY_DOWN: u8 = 17;
pub const PIN_KEY_LEFT: u8 = 18;

pub const PIN_USB_D_MINUS: u8 = 19;
pub const PIN_USB_D_PLUS: u8 = 20;
pub const PIN_KEY_UP: u8 = 21;

pub const PIN_FAN_EN: u8 = 35;
pub const PIN_FAN_PWM: u8 = 36;

pub const ACTIVE_GPIO: [u8; 20] = [
    PIN_CENTER_KEY_BOOT,
    PIN_VIN_ADC,
    PIN_RTD_ADC,
    PIN_HEATER_PWM,
    PIN_I2C_SDA,
    PIN_I2C_SCL,
    PIN_LCD_DC,
    PIN_LCD_MOSI,
    PIN_LCD_SCLK,
    PIN_LCD_BLK,
    PIN_LCD_RES,
    PIN_LCD_CS,
    PIN_KEY_RIGHT,
    PIN_KEY_DOWN,
    PIN_KEY_LEFT,
    PIN_USB_D_MINUS,
    PIN_USB_D_PLUS,
    PIN_KEY_UP,
    PIN_FAN_EN,
    PIN_FAN_PWM,
];

pub const VIN_DIVIDER_R_HIGH_OHMS: u32 = 56_000;
pub const VIN_DIVIDER_R_LOW_OHMS: u32 = 5_100;
pub const VIN_DIVIDER_MAX_INPUT_MV: u32 = 28_000;
pub const VIN_DIVIDER_MAX_ADC_MV: u32 = (VIN_DIVIDER_MAX_INPUT_MV * VIN_DIVIDER_R_LOW_OHMS)
    / (VIN_DIVIDER_R_HIGH_OHMS + VIN_DIVIDER_R_LOW_OHMS);

pub fn gpio_map_is_valid() -> bool {
    if ACTIVE_GPIO.len() != 20 {
        return false;
    }

    let mut seen = [false; 49];
    for pin in ACTIVE_GPIO {
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
    fn gpio_map_accounts_for_s3_direct_frontpanel() {
        assert!(super::gpio_map_is_valid());
        assert_eq!(super::PIN_CENTER_KEY_BOOT, 0);
        assert_eq!(super::PIN_RTD_ADC, 2);
        assert_eq!(super::PIN_LCD_DC, 10);
        assert_eq!(super::PIN_LCD_BLK, 13);
        assert_eq!(super::PIN_FAN_EN, 35);
        assert_eq!(super::PIN_FAN_PWM, 36);
        assert_eq!(super::PIN_HEATER_PWM, 47);
        assert_eq!(super::VIN_DIVIDER_MAX_ADC_MV, 2_337);
    }
}
