#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontPanelKey {
    Center,
    Right,
    Down,
    Left,
    Up,
}

pub const FRONT_PANEL_ADDRESS: u8 = 0x21;
pub const LCD_RES_PIN: u8 = 5;
pub const LCD_CS_PIN: u8 = 6;
pub const LCD_BLK_PIN: u8 = 7;

const KEY_MASK: u8 = 0b0001_1110;

pub fn decode_frontpanel_key(input_port: u8) -> Option<FrontPanelKey> {
    // TCA6408A inputs are active-low for this panel design.
    let pressed = (!input_port) & KEY_MASK;
    match pressed {
        0b0000_0010 => Some(FrontPanelKey::Right),
        0b0000_0100 => Some(FrontPanelKey::Down),
        0b0000_1000 => Some(FrontPanelKey::Left),
        0b0001_0000 => Some(FrontPanelKey::Up),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_each_four_way_key_without_ambiguity() {
        assert_eq!(
            decode_frontpanel_key(0b1111_1101),
            Some(FrontPanelKey::Right)
        );
        assert_eq!(
            decode_frontpanel_key(0b1111_1011),
            Some(FrontPanelKey::Down)
        );
        assert_eq!(
            decode_frontpanel_key(0b1111_0111),
            Some(FrontPanelKey::Left)
        );
        assert_eq!(decode_frontpanel_key(0b1110_1111), Some(FrontPanelKey::Up));
    }

    #[test]
    fn returns_none_for_no_key_or_multi_key_press() {
        assert_eq!(decode_frontpanel_key(0b1111_1111), None);
        assert_eq!(decode_frontpanel_key(0b1111_1110), None);
        assert_eq!(decode_frontpanel_key(0b1111_1001), None);
    }
}
