//! Read-only presentation data; rendering never queries hardware.
use core::fmt::Write;
use heapless::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub version: String<32>,
    pub sha: String<40>,
    pub device_id: String<48>,
    pub board: String<24>,
    pub reset: &'static str,
    pub thermal: &'static str,
    pub rtd: &'static str,
    pub vin: &'static str,
    pub config: &'static str,
    pub uptime_seconds: u64,
    pub heater_percent: u8,
    pub fan_on: bool,
}
impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            version: String::new(),
            sha: String::new(),
            device_id: String::new(),
            board: String::new(),
            reset: "UNKNOWN",
            thermal: "MISSING",
            rtd: "DEFAULT",
            vin: "DEFAULT",
            config: "UNKNOWN",
            uptime_seconds: 0,
            heater_percent: 0,
            fan_on: false,
        }
    }
}
impl DeviceInfo {
    pub fn set_identity(&mut self, identity: &crate::control_plane::Identity) {
        self.version.clear();
        let _ = self.version.push_str(&identity.firmware_version);
        self.sha.clear();
        let _ = self.sha.push_str(&identity.git_sha);
        self.device_id.clear();
        let _ = self.device_id.push_str(&identity.device_id);
        self.board.clear();
        let _ = self.board.push_str(&identity.board);
        self.reset = match identity.reset_reason.as_deref().unwrap_or("unknown") {
            "chip_power_on" => "POWER ON",
            "core_software" | "cpu_software" => "SW RESET",
            "system_brownout" => "BROWNOUT",
            reason if reason.contains("wdt") => "WATCHDOG",
            _ => "UNKNOWN",
        };
    }
    pub fn uptime(&self) -> String<24> {
        let mut result = String::new();
        let seconds = self.uptime_seconds;
        if seconds / 3600 > 99 {
            let _ = write!(
                result,
                "{}D {:02}:{:02}",
                seconds / 86400,
                seconds / 3600 % 24,
                seconds / 60 % 60
            );
        } else {
            let _ = write!(
                result,
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uptime_switches_to_days_without_wrapping() {
        let mut info = DeviceInfo {
            uptime_seconds: 359999,
            ..DeviceInfo::default()
        };
        assert_eq!(info.uptime().as_str(), "99:59:59");
        info.uptime_seconds = 360000;
        assert_eq!(info.uptime().as_str(), "4D 04:00");
    }
    #[test]
    fn identity_uses_wire_identity_without_placeholder_values() {
        let mut info = DeviceInfo::default();
        assert!(info.device_id.is_empty());
        let mut identity = crate::control_plane::Identity::firmware_from_mac([1, 2, 3, 4, 5, 6]);
        identity.reset_reason = Some("core_mwdt0".try_into().unwrap());
        info.set_identity(&identity);
        assert_eq!(info.device_id, identity.device_id.as_str());
        assert_eq!(info.version, identity.firmware_version);
        assert_eq!(info.reset, "WATCHDOG");
    }
}
