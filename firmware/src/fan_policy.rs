use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostHeatCoolingMode {
    Off,
    #[default]
    Normal,
    Fast,
}

impl PostHeatCoolingMode {
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub const fn from_legacy(enabled: bool) -> Self {
        if enabled { Self::Normal } else { Self::Off }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Normal => "NORMAL",
            Self::Fast => "FAST",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatingFanGuardMode {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

impl HeatingFanGuardMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Low => "LOW",
            Self::Medium => "MED",
            Self::High => "HIGH",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanPolicySource {
    #[default]
    Idle,
    PostHeat,
    HeatingGuard,
    Safety,
}

impl FanPolicySource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::PostHeat => "POST",
            Self::HeatingGuard => "HEAT",
            Self::Safety => "SAFE",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanOutputLevel {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Limited,
}

impl FanOutputLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Low => "LOW",
            Self::Medium => "MED",
            Self::High => "HIGH",
            Self::Limited => "LIMIT",
        }
    }
}
