use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

use crate::{
    DeviceMode, DeviceStatus, PdState,
    frontpanel::{FRONTPANEL_PRESET_COUNT, FrontPanelKey},
    memory::{MEMORY_WIFI_PASSWORD_MAX_LEN, MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig},
};

pub const CONTROL_PLANE_API_VERSION: &str = "2026-05-29";
pub const USB_PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
pub const USB_FRAMING: &str = "jsonl";
pub const DEVICE_ID_MAX_LEN: usize = 48;
pub const BUILD_ID_MAX_LEN: usize = 48;
pub const GIT_SHA_MAX_LEN: usize = 40;
pub const HOSTNAME_MAX_LEN: usize = 64;
pub const CAPABILITY_MAX_LEN: usize = 24;
pub const CAPABILITY_COUNT_MAX: usize = 10;
pub const USB_LINE_MAX_LEN: usize = 1024;
pub const REQUEST_ID_MAX_LEN: usize = 48;
pub const ERROR_CODE_MAX_LEN: usize = 48;
pub const ERROR_MESSAGE_MAX_LEN: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    Disabled,
    Idle,
    Saving,
    Connecting,
    Connected,
    Error,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSummary {
    pub state: NetworkState,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    pub ip: Option<String<48>>,
    pub gateway: Option<String<48>>,
    pub dns: Vec<String<48>, 2>,
    pub wifi_rssi: Option<i16>,
    pub last_error: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

impl Default for NetworkSummary {
    fn default() -> Self {
        Self {
            state: NetworkState::Disabled,
            ssid: None,
            ip: None,
            gateway: None,
            dns: Vec::new(),
            wifi_rssi: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String<DEVICE_ID_MAX_LEN>,
    pub firmware_version: String<32>,
    pub build_id: String<BUILD_ID_MAX_LEN>,
    pub git_sha: String<GIT_SHA_MAX_LEN>,
    pub board: String<24>,
    pub api_version: String<16>,
    pub protocol_version: String<24>,
    pub hostname: String<HOSTNAME_MAX_LEN>,
    pub capabilities: Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>,
}

impl Identity {
    pub fn firmware_default() -> Self {
        let mut capabilities = Vec::new();
        push_str(&mut capabilities, "identity");
        push_str(&mut capabilities, "status");
        push_str(&mut capabilities, "network");
        #[cfg(feature = "web_serial")]
        {
            push_str(&mut capabilities, "usb_jsonl");
            push_str(&mut capabilities, "wifi_config");
            push_str(&mut capabilities, "monitor");
        }
        Self {
            device_id: string("flux-purr-s3-001"),
            firmware_version: string(env!("CARGO_PKG_VERSION")),
            build_id: string(option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("host-build")),
            git_sha: string(option_env!("GIT_SHA").unwrap_or("unknown")),
            board: string("esp32-s3"),
            api_version: string(CONTROL_PLANE_API_VERSION),
            protocol_version: string(USB_PROTOCOL_VERSION),
            hostname: string("flux-purr-s3-001"),
            capabilities,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FanDisplayState {
    Off,
    Auto,
    Run,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPlaneStatus {
    pub mode: DeviceModeWire,
    pub uptime_seconds: u32,
    pub current_temp_c: f32,
    pub target_temp_c: i16,
    pub selected_preset_slot: usize,
    pub presets_c: [Option<i16>; FRONTPANEL_PRESET_COUNT],
    pub heater_enabled: bool,
    pub heater_output_percent: u8,
    pub active_cooling_enabled: bool,
    pub fan_display_state: FanDisplayState,
    pub fan_enabled: bool,
    pub fan_pwm_permille: u16,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: PdStateWire,
    pub frontpanel_key: Option<FrontPanelKeyWire>,
    pub network: NetworkSummary,
}

impl ControlPlaneStatus {
    pub fn from_device_status(
        status: DeviceStatus,
        memory: &MemoryConfig,
        uptime_seconds: u32,
        network: NetworkSummary,
    ) -> Self {
        let heater_output_percent = status.heater_output_percent.min(100);
        let fan_display_state = if !memory.active_cooling_enabled {
            FanDisplayState::Off
        } else if status.fan_enabled {
            FanDisplayState::Run
        } else {
            FanDisplayState::Auto
        };

        Self {
            mode: status.mode.into(),
            uptime_seconds,
            current_temp_c: status.board_temp_centi as f32 / 100.0,
            target_temp_c: memory.target_temp_c,
            selected_preset_slot: memory.selected_preset_slot,
            presets_c: memory.presets_c,
            heater_enabled: matches!(status.mode, DeviceMode::Sampling),
            heater_output_percent,
            active_cooling_enabled: memory.active_cooling_enabled,
            fan_display_state,
            fan_enabled: status.fan_enabled,
            fan_pwm_permille: status.fan_pwm_permille,
            voltage_mv: status.voltage_mv,
            current_ma: status.current_ma,
            board_temp_centi: status.board_temp_centi,
            pd_request_mv: status.pd_request_mv,
            pd_contract_mv: status.pd_contract_mv,
            pd_state: status.pd_state.into(),
            frontpanel_key: status.frontpanel_key.map(Into::into),
            network,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceModeWire {
    Idle,
    Sampling,
    Fault,
}

impl From<DeviceMode> for DeviceModeWire {
    fn from(value: DeviceMode) -> Self {
        match value {
            DeviceMode::Idle => Self::Idle,
            DeviceMode::Sampling => Self::Sampling,
            DeviceMode::Fault => Self::Fault,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdStateWire {
    Negotiating,
    Ready,
    #[serde(rename = "fallback_5v")]
    Fallback5v,
    Fault,
}

impl From<PdState> for PdStateWire {
    fn from(value: PdState) -> Self {
        match value {
            PdState::Negotiating => Self::Negotiating,
            PdState::Ready => Self::Ready,
            PdState::Fallback5V => Self::Fallback5v,
            PdState::Fault => Self::Fault,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontPanelKeyWire {
    Center,
    Right,
    Down,
    Left,
    Up,
}

impl From<FrontPanelKey> for FrontPanelKeyWire {
    fn from(value: FrontPanelKey) -> Self {
        match value {
            FrontPanelKey::Center => Self::Center,
            FrontPanelKey::Right => Self::Right,
            FrontPanelKey::Down => Self::Down,
            FrontPanelKey::Left => Self::Left,
            FrontPanelKey::Up => Self::Up,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiConfigCommand {
    pub op: WifiConfigOp,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    pub password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    pub auto_reconnect: Option<bool>,
    pub telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WifiConfigOp {
    Set,
    Clear,
}

impl WifiConfigCommand {
    pub fn apply_to(&self, config: &mut MemoryConfig) {
        match self.op {
            WifiConfigOp::Clear => {
                config.wifi_ssid.clear();
                config.wifi_password.clear();
                config.wifi_auto_reconnect = false;
            }
            WifiConfigOp::Set => {
                config.wifi_ssid.clear();
                if let Some(ssid) = &self.ssid {
                    let _ = config.wifi_ssid.push_str(ssid);
                }
                config.wifi_password.clear();
                if let Some(password) = &self.password {
                    let _ = config.wifi_password.push_str(password);
                }
                if let Some(auto_reconnect) = self.auto_reconnect {
                    config.wifi_auto_reconnect = auto_reconnect;
                }
                if let Some(interval) = self.telemetry_interval_ms {
                    config.telemetry_interval_ms = interval.max(1);
                }
            }
        }
        config.sanitize();
    }

    pub fn redacted_summary(&self) -> RedactedWifiConfig {
        RedactedWifiConfig {
            op: self.op,
            ssid: self.ssid.clone(),
            password: self.password.as_ref().map(|_| string("<redacted>")),
            auto_reconnect: self.auto_reconnect,
            telemetry_interval_ms: self.telemetry_interval_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedWifiConfig {
    pub op: WifiConfigOp,
    pub ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    pub password: Option<String<16>>,
    pub auto_reconnect: Option<bool>,
    pub telemetry_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigCommand {
    pub target_temp_c: Option<i16>,
    pub selected_preset_slot: Option<usize>,
    pub presets_c: Option<[Option<i16>; FRONTPANEL_PRESET_COUNT]>,
    pub active_cooling_enabled: Option<bool>,
    pub heater_enabled: Option<bool>,
}

impl RuntimeConfigCommand {
    pub fn apply_to(&self, config: &mut MemoryConfig) {
        if let Some(target_temp_c) = self.target_temp_c {
            config.target_temp_c = target_temp_c;
        }
        if let Some(selected_preset_slot) = self.selected_preset_slot {
            config.selected_preset_slot = selected_preset_slot;
        }
        if let Some(presets_c) = self.presets_c {
            config.presets_c = presets_c;
            if self.target_temp_c.is_none()
                && let Some(target_temp_c) = config
                    .presets_c
                    .get(config.selected_preset_slot)
                    .and_then(|preset| *preset)
            {
                config.target_temp_c = target_temp_c;
            }
        }
        if let Some(active_cooling_enabled) = self.active_cooling_enabled {
            config.active_cooling_enabled = active_cooling_enabled;
        }
        config.sanitize();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UsbFrame {
    Hello {
        protocol_version: String<24>,
        framing: String<8>,
        identity: Identity,
        capabilities: Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>,
    },
    Request {
        request_id: String<REQUEST_ID_MAX_LEN>,
        op: UsbRequestOp,
    },
    WifiConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: WifiConfigCommand,
    },
    RuntimeConfig {
        request_id: String<REQUEST_ID_MAX_LEN>,
        config: RuntimeConfigCommand,
    },
    Response {
        request_id: String<REQUEST_ID_MAX_LEN>,
        ok: bool,
        result: Option<UsbResponsePayload>,
        error: Option<ApiError>,
    },
    Status {
        status: ControlPlaneStatus,
    },
    Log {
        level: String<8>,
        message: String<ERROR_MESSAGE_MAX_LEN>,
    },
    Error {
        request_id: Option<String<REQUEST_ID_MAX_LEN>>,
        error: ApiError,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsbFrameWire {
    #[serde(rename = "type")]
    frame_type: String<16>,
    #[serde(rename = "protocolVersion")]
    protocol_version: Option<String<24>>,
    framing: Option<String<8>>,
    identity: Option<Identity>,
    capabilities: Option<Vec<String<CAPABILITY_MAX_LEN>, CAPABILITY_COUNT_MAX>>,
    #[serde(rename = "requestId")]
    request_id: Option<String<REQUEST_ID_MAX_LEN>>,
    op: Option<String<16>>,
    ssid: Option<String<MEMORY_WIFI_SSID_MAX_LEN>>,
    password: Option<String<MEMORY_WIFI_PASSWORD_MAX_LEN>>,
    auto_reconnect: Option<bool>,
    telemetry_interval_ms: Option<u32>,
    target_temp_c: Option<i16>,
    selected_preset_slot: Option<usize>,
    presets_c: Option<[Option<i16>; FRONTPANEL_PRESET_COUNT]>,
    active_cooling_enabled: Option<bool>,
    heater_enabled: Option<bool>,
    ok: Option<bool>,
    result: Option<UsbResponsePayload>,
    error: Option<ApiError>,
    status: Option<ControlPlaneStatus>,
    level: Option<String<8>>,
    message: Option<String<ERROR_MESSAGE_MAX_LEN>>,
}

impl TryFrom<UsbFrameWire> for UsbFrame {
    type Error = UsbFrameError;

    fn try_from(value: UsbFrameWire) -> Result<Self, <UsbFrame as TryFrom<UsbFrameWire>>::Error> {
        match value.frame_type.as_str() {
            "hello" => Ok(UsbFrame::Hello {
                protocol_version: value.protocol_version.ok_or(UsbFrameError::MalformedJson)?,
                framing: value.framing.ok_or(UsbFrameError::MalformedJson)?,
                identity: value.identity.ok_or(UsbFrameError::MalformedJson)?,
                capabilities: value.capabilities.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "request" => Ok(UsbFrame::Request {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                op: parse_usb_request_op(value.op.as_deref())?,
            }),
            "wifi_config" => Ok(UsbFrame::WifiConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: WifiConfigCommand {
                    op: parse_wifi_config_op(value.op.as_deref())?,
                    ssid: value.ssid,
                    password: value.password,
                    auto_reconnect: value.auto_reconnect,
                    telemetry_interval_ms: value.telemetry_interval_ms,
                },
            }),
            "runtime_config" => Ok(UsbFrame::RuntimeConfig {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                config: RuntimeConfigCommand {
                    target_temp_c: value.target_temp_c,
                    selected_preset_slot: value.selected_preset_slot,
                    presets_c: value.presets_c,
                    active_cooling_enabled: value.active_cooling_enabled,
                    heater_enabled: value.heater_enabled,
                },
            }),
            "response" => Ok(UsbFrame::Response {
                request_id: value.request_id.ok_or(UsbFrameError::MalformedJson)?,
                ok: value.ok.ok_or(UsbFrameError::MalformedJson)?,
                result: value.result,
                error: value.error,
            }),
            "status" => Ok(UsbFrame::Status {
                status: value.status.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "log" => Ok(UsbFrame::Log {
                level: value.level.ok_or(UsbFrameError::MalformedJson)?,
                message: value.message.ok_or(UsbFrameError::MalformedJson)?,
            }),
            "error" => Ok(UsbFrame::Error {
                request_id: value.request_id,
                error: value.error.ok_or(UsbFrameError::MalformedJson)?,
            }),
            _ => Err(UsbFrameError::MalformedJson),
        }
    }
}

impl From<&UsbFrame> for UsbFrameWire {
    fn from(value: &UsbFrame) -> Self {
        let mut wire = UsbFrameWire {
            frame_type: String::new(),
            protocol_version: None,
            framing: None,
            identity: None,
            capabilities: None,
            request_id: None,
            op: None,
            ssid: None,
            password: None,
            auto_reconnect: None,
            telemetry_interval_ms: None,
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            heater_enabled: None,
            ok: None,
            result: None,
            error: None,
            status: None,
            level: None,
            message: None,
        };

        match value {
            UsbFrame::Hello {
                protocol_version,
                framing,
                identity,
                capabilities,
            } => {
                wire.frame_type = string("hello");
                wire.protocol_version = Some(protocol_version.clone());
                wire.framing = Some(framing.clone());
                wire.identity = Some(identity.clone());
                wire.capabilities = Some(capabilities.clone());
            }
            UsbFrame::Request { request_id, op } => {
                wire.frame_type = string("request");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(op.as_str()));
            }
            UsbFrame::WifiConfig { request_id, config } => {
                wire.frame_type = string("wifi_config");
                wire.request_id = Some(request_id.clone());
                wire.op = Some(string(config.op.as_str()));
                wire.ssid = config.ssid.clone();
                wire.password = config.password.clone();
                wire.auto_reconnect = config.auto_reconnect;
                wire.telemetry_interval_ms = config.telemetry_interval_ms;
            }
            UsbFrame::RuntimeConfig { request_id, config } => {
                wire.frame_type = string("runtime_config");
                wire.request_id = Some(request_id.clone());
                wire.target_temp_c = config.target_temp_c;
                wire.selected_preset_slot = config.selected_preset_slot;
                wire.presets_c = config.presets_c;
                wire.active_cooling_enabled = config.active_cooling_enabled;
                wire.heater_enabled = config.heater_enabled;
            }
            UsbFrame::Response {
                request_id,
                ok,
                result,
                error,
            } => {
                wire.frame_type = string("response");
                wire.request_id = Some(request_id.clone());
                wire.ok = Some(*ok);
                wire.result = result.clone();
                wire.error = error.clone();
            }
            UsbFrame::Status { status } => {
                wire.frame_type = string("status");
                wire.status = Some(status.clone());
            }
            UsbFrame::Log { level, message } => {
                wire.frame_type = string("log");
                wire.level = Some(level.clone());
                wire.message = Some(message.clone());
            }
            UsbFrame::Error { request_id, error } => {
                wire.frame_type = string("error");
                wire.request_id = request_id.clone();
                wire.error = Some(error.clone());
            }
        }

        wire
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbRequestOp {
    GetIdentity,
    GetNetwork,
    GetStatus,
    SetLogLevel,
}

impl UsbRequestOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::GetIdentity => "get_identity",
            Self::GetNetwork => "get_network",
            Self::GetStatus => "get_status",
            Self::SetLogLevel => "set_log_level",
        }
    }
}

impl WifiConfigOp {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbResponsePayload {
    Identity(Identity),
    Network(NetworkSummary),
    Status(ControlPlaneStatus),
    Wifi(RedactedWifiConfig),
    Ack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String<ERROR_CODE_MAX_LEN>,
    pub message: String<ERROR_MESSAGE_MAX_LEN>,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(code: &str, message: &str, retryable: bool) -> Self {
        Self {
            code: string(code),
            message: string(message),
            retryable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbFrameError {
    MalformedJson,
    OutputTooSmall,
}

pub fn hello_frame(identity: Identity) -> UsbFrame {
    UsbFrame::Hello {
        protocol_version: string(USB_PROTOCOL_VERSION),
        framing: string(USB_FRAMING),
        capabilities: identity.capabilities.clone(),
        identity,
    }
}

pub fn log_frame(level: &str, message: &str) -> UsbFrame {
    UsbFrame::Log {
        level: string(level),
        message: string(message),
    }
}

pub fn parse_usb_frame(line: &str) -> Result<UsbFrame, UsbFrameError> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    serde_json_core::from_str::<UsbFrameWire>(trimmed)
        .map(|(frame, _)| frame)
        .map_err(|_| UsbFrameError::MalformedJson)?
        .try_into()
        .map_err(|_| UsbFrameError::MalformedJson)
}

pub fn write_usb_frame<'a>(frame: &UsbFrame, out: &'a mut [u8]) -> Result<&'a str, UsbFrameError> {
    let wire = UsbFrameWire::from(frame);
    let written =
        serde_json_core::to_slice(&wire, out).map_err(|_| UsbFrameError::OutputTooSmall)?;
    if written >= out.len() {
        return Err(UsbFrameError::OutputTooSmall);
    }
    out[written] = b'\n';
    core::str::from_utf8(&out[..written + 1]).map_err(|_| UsbFrameError::OutputTooSmall)
}

pub fn network_from_memory(config: &MemoryConfig) -> NetworkSummary {
    let ssid = if config.wifi_ssid.is_empty() {
        None
    } else {
        Some(config.wifi_ssid.clone())
    };

    NetworkSummary {
        state: if ssid.is_some() {
            NetworkState::Idle
        } else {
            NetworkState::Disabled
        },
        ssid,
        ..NetworkSummary::default()
    }
}

fn string<const N: usize>(value: &str) -> String<N> {
    let mut out = String::new();
    let _ = out.push_str(value);
    out
}

fn push_str<const N: usize, const C: usize>(values: &mut Vec<String<N>, C>, value: &str) {
    let _ = values.push(string(value));
}

fn parse_usb_request_op(value: Option<&str>) -> Result<UsbRequestOp, UsbFrameError> {
    match value {
        Some("get_identity") => Ok(UsbRequestOp::GetIdentity),
        Some("get_network") => Ok(UsbRequestOp::GetNetwork),
        Some("get_status") => Ok(UsbRequestOp::GetStatus),
        Some("set_log_level") => Ok(UsbRequestOp::SetLogLevel),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

fn parse_wifi_config_op(value: Option<&str>) -> Result<WifiConfigOp, UsbFrameError> {
    match value {
        Some("set") => Ok(WifiConfigOp::Set),
        Some("clear") => Ok(WifiConfigOp::Clear),
        _ => Err(UsbFrameError::MalformedJson),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FanCommand, FanPhase, snapshot_at};

    #[test]
    fn identity_lists_feature_capabilities() {
        let identity = Identity::firmware_default();
        assert!(
            identity
                .capabilities
                .iter()
                .any(|value| value == "identity")
        );
        assert!(identity.capabilities.iter().any(|value| value == "status"));
        #[cfg(feature = "web_serial")]
        {
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "usb_jsonl")
            );
            assert!(
                identity
                    .capabilities
                    .iter()
                    .any(|value| value == "wifi_config")
            );
        }
    }

    #[test]
    fn status_adapter_uses_memory_and_runtime_state() {
        let mut memory = MemoryConfig {
            target_temp_c: 210,
            active_cooling_enabled: false,
            ..MemoryConfig::default()
        };
        memory.wifi_ssid.push_str("FluxPurr-Lab").unwrap();
        let status = ControlPlaneStatus::from_device_status(
            snapshot_at(10, 0),
            &memory,
            42,
            network_from_memory(&memory),
        );

        assert_eq!(status.mode, DeviceModeWire::Sampling);
        assert_eq!(status.target_temp_c, 210);
        assert!(!status.active_cooling_enabled);
        assert_eq!(status.network.state, NetworkState::Idle);
        assert_eq!(status.network.ssid.as_deref(), Some("FluxPurr-Lab"));
        assert_eq!(status.frontpanel_key, Some(FrontPanelKeyWire::Center));
    }

    #[test]
    fn status_frame_serializes_pd_fallback_for_web_contract() {
        let status = ControlPlaneStatus::from_device_status(
            snapshot_at(17, 0),
            &MemoryConfig::default(),
            17,
            NetworkSummary::default(),
        );
        let frame = UsbFrame::Response {
            request_id: string("req-pd"),
            ok: true,
            result: Some(UsbResponsePayload::Status(status)),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""pdState":"fallback_5v""#));
        assert!(!json.contains("fallback5v"));
    }

    #[test]
    fn log_frame_serializes_lifecycle_message() {
        let frame = log_frame("info", "frontpanel runtime ready");
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&frame, &mut out).unwrap();

        assert!(json.contains(r#""type":"log""#));
        assert!(json.contains(r#""level":"info""#));
        assert!(json.contains(r#""message":"frontpanel runtime ready""#));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn status_adapter_maps_fan_display_state() {
        let memory = MemoryConfig::default();
        let mut status = snapshot_at(10, 0);
        status.fan_enabled = false;
        status.fan_pwm_permille = FanCommand::from_phase(FanPhase::Stop).pwm_permille;
        let adapted =
            ControlPlaneStatus::from_device_status(status, &memory, 0, NetworkSummary::default());
        assert_eq!(adapted.fan_display_state, FanDisplayState::Auto);

        let mut running_fan = snapshot_at(120, 0);
        running_fan.fan_enabled = true;
        running_fan.fan_pwm_permille = crate::FAN_LOW_PWM_PERMILLE;
        let adapted = ControlPlaneStatus::from_device_status(
            running_fan,
            &memory,
            0,
            NetworkSummary::default(),
        );
        assert_eq!(adapted.fan_display_state, FanDisplayState::Run);

        let mut safety_fan = snapshot_at(120, 0);
        safety_fan.fan_enabled = true;
        safety_fan.fan_pwm_permille = crate::FAN_MID_PWM_PERMILLE;
        let cooling_disabled = MemoryConfig {
            active_cooling_enabled: false,
            ..MemoryConfig::default()
        };
        let adapted = ControlPlaneStatus::from_device_status(
            safety_fan,
            &cooling_disabled,
            0,
            NetworkSummary::default(),
        );
        assert_eq!(adapted.fan_display_state, FanDisplayState::Off);
    }

    #[test]
    fn wifi_command_applies_and_redacts_password() {
        let command = WifiConfigCommand {
            op: WifiConfigOp::Set,
            ssid: Some(string("FluxPurr-Lab")),
            password: Some(string("secret-pass")),
            auto_reconnect: Some(true),
            telemetry_interval_ms: Some(750),
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);
        assert_eq!(config.wifi_ssid.as_str(), "FluxPurr-Lab");
        assert_eq!(config.wifi_password.as_str(), "secret-pass");
        assert_eq!(
            command.redacted_summary().password.as_deref(),
            Some("<redacted>")
        );
    }

    #[test]
    fn runtime_command_updates_memory_policy() {
        let command = RuntimeConfigCommand {
            target_temp_c: Some(250),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: Some(false),
            heater_enabled: Some(true),
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);

        assert_eq!(config.target_temp_c, 250);
        assert!(!config.active_cooling_enabled);
        assert_eq!(command.heater_enabled, Some(true));
    }

    #[test]
    fn runtime_command_updates_memory_presets() {
        let command = RuntimeConfigCommand {
            target_temp_c: None,
            selected_preset_slot: Some(3),
            presets_c: Some([
                Some(50),
                Some(100),
                None,
                Some(155),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ]),
            active_cooling_enabled: None,
            heater_enabled: None,
        };
        let mut config = MemoryConfig::default();
        command.apply_to(&mut config);

        assert_eq!(config.selected_preset_slot, 3);
        assert_eq!(config.presets_c[2], None);
        assert_eq!(config.presets_c[3], Some(155));
        assert_eq!(config.target_temp_c, 155);
    }

    #[test]
    fn parse_usb_request_with_request_id() {
        let frame =
            parse_usb_frame(r#"{"type":"request","requestId":"req-001","op":"get_status"}"#)
                .unwrap();

        assert_eq!(
            frame,
            UsbFrame::Request {
                request_id: string("req-001"),
                op: UsbRequestOp::GetStatus,
            }
        );
    }

    #[test]
    fn parse_wifi_frame_and_write_redacted_response() {
        let frame = parse_usb_frame(
            r#"{"type":"wifi_config","requestId":"req-002","op":"set","ssid":"FluxPurr-Lab","password":"secret-pass","autoReconnect":true,"telemetryIntervalMs":500}"#,
        )
        .unwrap();
        let UsbFrame::WifiConfig { request_id, config } = frame else {
            panic!("expected wifi frame");
        };
        assert_eq!(request_id.as_str(), "req-002");
        assert_eq!(config.password.as_deref(), Some("secret-pass"));

        let response = UsbFrame::Response {
            request_id,
            ok: true,
            result: Some(UsbResponsePayload::Wifi(config.redacted_summary())),
            error: None,
        };
        let mut out = [0u8; USB_LINE_MAX_LEN];
        let json = write_usb_frame(&response, &mut out).unwrap();
        assert!(json.contains(r#""password":"<redacted>""#));
        assert!(!json.contains("secret-pass"));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn parse_runtime_config_frame() {
        let frame = parse_usb_frame(
            r#"{"type":"runtime_config","requestId":"req-003","targetTempC":230,"activeCoolingEnabled":false,"heaterEnabled":true}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::RuntimeConfig {
                request_id: string("req-003"),
                config: RuntimeConfigCommand {
                    target_temp_c: Some(230),
                    selected_preset_slot: None,
                    presets_c: None,
                    active_cooling_enabled: Some(false),
                    heater_enabled: Some(true),
                },
            }
        );
    }

    #[test]
    fn parse_runtime_config_frame_with_presets() {
        let frame = parse_usb_frame(
            r#"{"type":"runtime_config","requestId":"req-004","selectedPresetSlot":3,"presetsC":[50,100,null,155,180,200,210,220,250,300]}"#,
        )
        .unwrap();

        assert_eq!(
            frame,
            UsbFrame::RuntimeConfig {
                request_id: string("req-004"),
                config: RuntimeConfigCommand {
                    target_temp_c: None,
                    selected_preset_slot: Some(3),
                    presets_c: Some([
                        Some(50),
                        Some(100),
                        None,
                        Some(155),
                        Some(180),
                        Some(200),
                        Some(210),
                        Some(220),
                        Some(250),
                        Some(300),
                    ]),
                    active_cooling_enabled: None,
                    heater_enabled: None,
                },
            }
        );
    }

    #[test]
    fn malformed_frame_returns_protocol_error() {
        assert_eq!(
            parse_usb_frame(r#"{"type":"request","op":"get_status"}"#),
            Err(UsbFrameError::MalformedJson)
        );
    }
}
