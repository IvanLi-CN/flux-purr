pub const PIN_USB_D_MINUS: u8 = 18;
pub const PIN_USB_D_PLUS: u8 = 19;

pub const PIN_CH442E_IN: u8 = 8;
pub const PIN_CH442E_EN_N: u8 = 9;

pub const PIN_I2C_SDA: u8 = 4;
pub const PIN_I2C_SCL: u8 = 5;
pub const PIN_FRONT_PANEL_INT_N: u8 = 2;

pub const PIN_LCD_SCLK: u8 = 21;
pub const PIN_LCD_MOSI: u8 = 20;
pub const PIN_LCD_DC: u8 = 7;
pub const PIN_LCD_BLK: u8 = 6;

pub const PIN_FAN_PWM: u8 = 3;
pub const PIN_FAN_EN: u8 = 1;

pub const PIN_HEATER_PWM: u8 = 10;
pub const PIN_TEMP_ADC: u8 = 0;

pub const LOCKED_GPIO: [u8; 15] = [
    PIN_USB_D_MINUS,
    PIN_USB_D_PLUS,
    PIN_CH442E_IN,
    PIN_CH442E_EN_N,
    PIN_I2C_SDA,
    PIN_I2C_SCL,
    PIN_FRONT_PANEL_INT_N,
    PIN_LCD_SCLK,
    PIN_LCD_MOSI,
    PIN_LCD_DC,
    PIN_LCD_BLK,
    PIN_FAN_PWM,
    PIN_FAN_EN,
    PIN_HEATER_PWM,
    PIN_TEMP_ADC,
];

pub fn gpio_budget_is_exactly_15() -> bool {
    if LOCKED_GPIO.len() != 15 {
        return false;
    }

    let mut seen = [false; 22];
    for pin in LOCKED_GPIO {
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
    fn gpio_budget_is_exactly_15() {
        assert!(super::gpio_budget_is_exactly_15());
    }
}
