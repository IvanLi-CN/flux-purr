use heapless::String;
use serde::{Deserialize, Serialize};

use crate::control_plane::Identity;

pub const RAM_BRINGUP_FRAME_TYPE: &str = "ram_bringup";
pub const RAM_BRINGUP_COMMAND_MAX_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RamBringupTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RamBringupCommand {
    PreviewDisplay,
    PreviewFrontpanel,
    PreviewStatusLight,
    TestButtons,
    TestAdc,
    TestI2c,
    TestRgb,
    TestBuzzer,
    TestFan,
}

impl RamBringupCommand {
    pub const ALL: [Self; 9] = [
        Self::PreviewDisplay,
        Self::PreviewFrontpanel,
        Self::PreviewStatusLight,
        Self::TestButtons,
        Self::TestAdc,
        Self::TestI2c,
        Self::TestRgb,
        Self::TestBuzzer,
        Self::TestFan,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreviewDisplay => "preview_display",
            Self::PreviewFrontpanel => "preview_frontpanel",
            Self::PreviewStatusLight => "preview_status_light",
            Self::TestButtons => "test_buttons",
            Self::TestAdc => "test_adc",
            Self::TestI2c => "test_i2c",
            Self::TestRgb => "test_rgb",
            Self::TestBuzzer => "test_buzzer",
            Self::TestFan => "test_fan",
        }
    }

    pub const fn capability(self) -> &'static str {
        self.as_str()
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|command| command.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamBringupRequest {
    #[serde(rename = "type")]
    pub frame_type: String<16>,
    pub request_id: String<48>,
    pub command: RamBringupCommand,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<RamBringupTheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamBringupResponse {
    #[serde(rename = "type")]
    pub frame_type: String<8>,
    pub request_id: String<48>,
    pub ok: bool,
    pub result: Option<RamBringupResult>,
    pub error: Option<String<64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamBringupResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<RamBringupCommandResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamBringupCommandResult {
    pub command: String<24>,
    pub status: String<16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_mask: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vin_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtd_mv: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i2c_device_id: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i2c_revision: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_pwm_permille: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_duration_ms: Option<u32>,
}

pub fn supported_capabilities() -> heapless::Vec<String<24>, 9> {
    let mut capabilities = heapless::Vec::new();
    for command in RamBringupCommand::ALL {
        let mut value = String::new();
        let _ = value.push_str(command.capability());
        let _ = capabilities.push(value);
    }
    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_set_is_closed_and_serializes_as_snake_case() {
        assert_eq!(RamBringupCommand::TestAdc.as_str(), "test_adc");
        assert_eq!(
            RamBringupCommand::parse("test_fan"),
            Some(RamBringupCommand::TestFan)
        );
        assert_eq!(RamBringupCommand::parse("write_eeprom"), None);
    }

    #[test]
    fn capabilities_are_exactly_the_safe_bringup_surface() {
        let capabilities = supported_capabilities();
        assert_eq!(capabilities.len(), 9);
        assert!(!capabilities.iter().any(|value| value.contains("heater")));
        assert!(!capabilities.iter().any(|value| value.contains("eeprom")));
        assert!(!capabilities.iter().any(|value| value.contains("pd")));
    }

    #[test]
    fn response_wire_round_trips_command_result_object() {
        let response = RamBringupResponse {
            frame_type: "response".try_into().unwrap(),
            request_id: "ram-1".try_into().unwrap(),
            ok: true,
            result: Some(RamBringupResult {
                identity: None,
                command: Some(RamBringupCommandResult {
                    command: "test_adc".try_into().unwrap(),
                    status: "complete".try_into().unwrap(),
                    key_mask: None,
                    vin_mv: Some(5000),
                    rtd_mv: Some(1200),
                    i2c_device_id: None,
                    i2c_revision: None,
                    fan_pwm_permille: None,
                    fan_duration_ms: None,
                }),
            }),
            error: None,
        };
        let encoded = serde_json::to_string(&response).unwrap();
        let wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(wire["result"]["command"]["command"], "test_adc");
        let decoded: RamBringupResponse = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, response);
    }
}
