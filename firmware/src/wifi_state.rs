//! Hardware-independent WiFi provisioning domain model.
//!
//! The public Interface accepts domain events and returns a state/effect pair.
//! ESP networking is an Adapter that executes the effect and feeds the next
//! event back; this module has no timers, drivers, USB, or allocator.

use crate::control_plane::{NetworkFailureCode, NetworkState};

pub const SAVING_TIMEOUT_MS: u32 = 3_000;
pub const PROVISIONING_TIMEOUT_MS: u32 = 30_000;
pub const MAX_PROVISIONING_ATTEMPTS: u8 = 3;

pub trait WifiClock {
    fn now_ms(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiEvent {
    ApplyConfig,
    ClearConfig,
    CancelProvisioning,
    DisconnectCompleted,
    DisconnectTimedOut,
    DriverConfigured,
    DriverConfigurationFailed,
    AssociationSucceeded,
    AssociationFailed,
    AssociationTimedOut,
    Ipv4Configured,
    Ipv4TimedOut,
    ProvisioningTimedOut,
    RetryDelayElapsed,
    StationDisconnected { auto_reconnect: bool },
    LanStartupFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiEffect {
    None,
    Disconnect,
    ConfigureDriver,
    Associate,
    AwaitIpv4,
    RetryAfterDelay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiTransition {
    pub accepted: bool,
    pub state: NetworkState,
    pub failure_code: Option<NetworkFailureCode>,
    pub configuration_generation: u32,
    pub transition_sequence: u32,
    pub effect: WifiEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiProvisioningMachine {
    state: NetworkState,
    failure_code: Option<NetworkFailureCode>,
    configuration_generation: u32,
    transition_sequence: u32,
    attempts: u8,
    provisioning_started_at_ms: Option<u64>,
}

impl Default for WifiProvisioningMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiProvisioningMachine {
    pub const fn new() -> Self {
        Self {
            state: NetworkState::Disabled,
            failure_code: None,
            configuration_generation: 0,
            transition_sequence: 0,
            attempts: 0,
            provisioning_started_at_ms: None,
        }
    }

    pub fn apply(&mut self, event: WifiEvent) -> WifiTransition {
        self.apply_at(event, 0)
    }

    pub fn apply_with_clock<C: WifiClock>(
        &mut self,
        event: WifiEvent,
        clock: &C,
    ) -> WifiTransition {
        self.apply_at(event, clock.now_ms())
    }

    pub fn apply_at(&mut self, event: WifiEvent, now_ms: u64) -> WifiTransition {
        let event = self.expired_event(event, now_ms);
        if !self.accepts(event) {
            return self.transition(false, WifiEffect::None);
        }
        let (state, failure_code, effect) = match event {
            WifiEvent::ClearConfig => {
                self.configuration_generation = self.configuration_generation.wrapping_add(1);
                self.attempts = 0;
                self.provisioning_started_at_ms = None;
                (NetworkState::Disabled, None, WifiEffect::None)
            }
            WifiEvent::CancelProvisioning => {
                self.attempts = 0;
                self.provisioning_started_at_ms = None;
                (NetworkState::Idle, None, WifiEffect::Disconnect)
            }
            WifiEvent::ApplyConfig => {
                self.configuration_generation = self.configuration_generation.wrapping_add(1);
                self.attempts = 0;
                self.provisioning_started_at_ms = Some(now_ms);
                (NetworkState::Saving, None, WifiEffect::Disconnect)
            }
            WifiEvent::DisconnectCompleted => {
                (NetworkState::Connecting, None, WifiEffect::ConfigureDriver)
            }
            WifiEvent::DisconnectTimedOut => (
                NetworkState::Error,
                Some(NetworkFailureCode::DisconnectTimedOut),
                WifiEffect::None,
            ),
            WifiEvent::DriverConfigured => (NetworkState::Connecting, None, WifiEffect::Associate),
            WifiEvent::DriverConfigurationFailed => {
                self.retry_or_terminal(NetworkFailureCode::ConfigurationFailed, NetworkState::Error)
            }
            WifiEvent::AssociationSucceeded => {
                (NetworkState::Connecting, None, WifiEffect::AwaitIpv4)
            }
            WifiEvent::AssociationFailed => {
                self.retry_or_terminal(NetworkFailureCode::AssociationRejected, NetworkState::Error)
            }
            WifiEvent::AssociationTimedOut => {
                self.retry_or_terminal(NetworkFailureCode::AssociationTimedOut, NetworkState::Error)
            }
            WifiEvent::Ipv4Configured => {
                self.provisioning_started_at_ms = None;
                (NetworkState::Connected, None, WifiEffect::None)
            }
            WifiEvent::Ipv4TimedOut => {
                self.retry_or_terminal(NetworkFailureCode::Ipv4TimedOut, NetworkState::Error)
            }
            WifiEvent::ProvisioningTimedOut => (
                NetworkState::Error,
                Some(NetworkFailureCode::Ipv4TimedOut),
                WifiEffect::None,
            ),
            WifiEvent::RetryDelayElapsed => {
                (NetworkState::Connecting, None, WifiEffect::ConfigureDriver)
            }
            WifiEvent::StationDisconnected {
                auto_reconnect: true,
            } => {
                self.attempts = 0;
                self.provisioning_started_at_ms = Some(now_ms);
                (NetworkState::Connecting, None, WifiEffect::RetryAfterDelay)
            }
            WifiEvent::StationDisconnected {
                auto_reconnect: false,
            } => (
                NetworkState::Error,
                Some(NetworkFailureCode::StationDisconnected),
                WifiEffect::None,
            ),
            WifiEvent::LanStartupFailed => (
                NetworkState::Error,
                Some(NetworkFailureCode::LanStartupFailed),
                WifiEffect::None,
            ),
        };
        self.state = state;
        self.failure_code = failure_code;
        self.transition_sequence = self.transition_sequence.wrapping_add(1);
        self.transition(true, effect)
    }

    pub const fn state(&self) -> NetworkState {
        self.state
    }

    fn transition(&self, accepted: bool, effect: WifiEffect) -> WifiTransition {
        WifiTransition {
            accepted,
            state: self.state,
            failure_code: self.failure_code,
            configuration_generation: self.configuration_generation,
            transition_sequence: self.transition_sequence,
            effect,
        }
    }

    const fn accepts(&self, event: WifiEvent) -> bool {
        match event {
            WifiEvent::ApplyConfig | WifiEvent::ClearConfig | WifiEvent::LanStartupFailed => true,
            WifiEvent::CancelProvisioning => {
                !matches!(self.state, NetworkState::Disabled | NetworkState::Idle)
            }
            WifiEvent::DisconnectCompleted | WifiEvent::DisconnectTimedOut => {
                matches!(self.state, NetworkState::Saving)
            }
            WifiEvent::DriverConfigured
            | WifiEvent::DriverConfigurationFailed
            | WifiEvent::AssociationSucceeded
            | WifiEvent::AssociationFailed
            | WifiEvent::AssociationTimedOut
            | WifiEvent::Ipv4Configured
            | WifiEvent::Ipv4TimedOut
            | WifiEvent::ProvisioningTimedOut => matches!(self.state, NetworkState::Connecting),
            WifiEvent::RetryDelayElapsed => matches!(self.state, NetworkState::Connecting),
            WifiEvent::StationDisconnected { .. } => matches!(self.state, NetworkState::Connected),
        }
    }

    fn expired_event(&self, event: WifiEvent, now_ms: u64) -> WifiEvent {
        let Some(started_at_ms) = self.provisioning_started_at_ms else {
            return event;
        };
        let elapsed_ms = now_ms.saturating_sub(started_at_ms);
        if matches!(self.state, NetworkState::Saving) && elapsed_ms > u64::from(SAVING_TIMEOUT_MS) {
            return WifiEvent::DisconnectTimedOut;
        }
        if matches!(self.state, NetworkState::Connecting)
            && elapsed_ms > u64::from(PROVISIONING_TIMEOUT_MS)
        {
            return WifiEvent::ProvisioningTimedOut;
        }
        event
    }

    fn retry_or_terminal(
        &mut self,
        failure_code: NetworkFailureCode,
        terminal: NetworkState,
    ) -> (NetworkState, Option<NetworkFailureCode>, WifiEffect) {
        self.attempts = self.attempts.saturating_add(1);
        if self.attempts < MAX_PROVISIONING_ATTEMPTS {
            (NetworkState::Connecting, None, WifiEffect::RetryAfterDelay)
        } else {
            (terminal, Some(failure_code), WifiEffect::None)
        }
    }
}

#[cfg(test)]
#[path = "wifi_state_tests.rs"]
mod tests;
