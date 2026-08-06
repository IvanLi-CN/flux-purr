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
mod tests {
    use super::*;

    struct FixedClock(u64);

    impl WifiClock for FixedClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    #[test]
    fn transient_failures_stay_connecting_until_the_third_attempt() {
        let mut machine = WifiProvisioningMachine::new();
        machine.apply(WifiEvent::ApplyConfig);
        machine.apply(WifiEvent::DisconnectCompleted);
        machine.apply(WifiEvent::DriverConfigured);
        for _ in 0..2 {
            let transition = machine.apply(WifiEvent::AssociationFailed);
            assert_eq!(transition.state, NetworkState::Connecting);
            assert_eq!(transition.failure_code, None);
            assert_eq!(transition.effect, WifiEffect::RetryAfterDelay);
            machine.apply(WifiEvent::RetryDelayElapsed);
            machine.apply(WifiEvent::DriverConfigured);
        }
        let terminal = machine.apply(WifiEvent::AssociationFailed);
        assert_eq!(terminal.state, NetworkState::Error);
        assert_eq!(
            terminal.failure_code,
            Some(NetworkFailureCode::AssociationRejected)
        );
    }

    #[test]
    fn each_configuration_has_a_monotonic_receipt_identity() {
        let mut machine = WifiProvisioningMachine::new();
        let first = machine.apply(WifiEvent::ApplyConfig);
        let second = machine.apply(WifiEvent::ClearConfig);
        assert_eq!(first.configuration_generation, 1);
        assert_eq!(second.configuration_generation, 2);
        assert!(second.transition_sequence > first.transition_sequence);
    }

    #[test]
    fn invalid_events_are_rejected_without_publishing_a_new_transition() {
        let mut machine = WifiProvisioningMachine::new();
        let rejected = machine.apply(WifiEvent::AssociationFailed);
        assert!(!rejected.accepted);
        assert_eq!(rejected.state, NetworkState::Disabled);
        assert_eq!(rejected.transition_sequence, 0);
        assert_eq!(rejected.effect, WifiEffect::None);
    }

    #[test]
    fn ordinary_retry_cannot_reopen_a_settled_configuration_failure() {
        let mut machine = WifiProvisioningMachine::new();
        machine.apply(WifiEvent::ApplyConfig);
        machine.apply(WifiEvent::DisconnectCompleted);
        machine.apply(WifiEvent::DriverConfigured);
        machine.apply(WifiEvent::AssociationFailed);
        machine.apply(WifiEvent::RetryDelayElapsed);
        machine.apply(WifiEvent::DriverConfigured);
        machine.apply(WifiEvent::AssociationFailed);
        machine.apply(WifiEvent::RetryDelayElapsed);
        machine.apply(WifiEvent::DriverConfigured);
        let terminal = machine.apply(WifiEvent::AssociationFailed);
        assert_eq!(terminal.state, NetworkState::Error);
        assert_eq!(
            terminal.failure_code,
            Some(NetworkFailureCode::AssociationRejected)
        );

        let retry = machine.apply(WifiEvent::RetryDelayElapsed);
        assert!(!retry.accepted);
        assert_eq!(retry.state, NetworkState::Error);
        assert_eq!(
            retry.failure_code,
            Some(NetworkFailureCode::AssociationRejected)
        );

        let recovery = machine.apply(WifiEvent::RetryDelayElapsed);
        assert!(!recovery.accepted);
        assert_eq!(recovery.state, NetworkState::Error);
        assert_eq!(
            recovery.failure_code,
            Some(NetworkFailureCode::AssociationRejected)
        );

        let new_configuration = machine.apply(WifiEvent::ApplyConfig);
        assert!(new_configuration.accepted);
        assert_eq!(new_configuration.state, NetworkState::Saving);
        assert!(new_configuration.configuration_generation > terminal.configuration_generation);
    }

    #[test]
    fn golden_fixture_enumerates_only_public_wifi_state_v2_values() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/wifi-provisioning-v2.json")).unwrap();
        let states = fixture["states"].as_array().unwrap();
        assert_eq!(states.len(), 4);
        assert!(states.iter().any(|state| state == "connected"));
        assert!(fixture["traces"].as_array().unwrap().iter().all(|trace| {
            trace["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .all(|snapshot| snapshot["transitionSequence"].as_u64().is_some())
        }));
    }

    #[test]
    fn disconnect_stage_is_bounded_by_the_same_injected_clock() {
        let mut machine = WifiProvisioningMachine::new();
        machine.apply_with_clock(WifiEvent::ApplyConfig, &FixedClock(100));

        let transition = machine.apply_at(
            WifiEvent::DisconnectCompleted,
            100 + u64::from(SAVING_TIMEOUT_MS) + 1,
        );

        assert_eq!(transition.state, NetworkState::Error);
        assert_eq!(
            transition.failure_code,
            Some(NetworkFailureCode::DisconnectTimedOut)
        );
    }

    #[test]
    fn retry_delay_does_not_restart_the_thirty_second_configuration_transaction() {
        let mut machine = WifiProvisioningMachine::new();
        machine.apply_at(WifiEvent::ApplyConfig, 0);
        machine.apply_at(WifiEvent::DisconnectCompleted, 1);
        machine.apply_at(WifiEvent::DriverConfigured, 1);
        machine.apply_at(WifiEvent::AssociationFailed, 10_000);
        machine.apply_at(WifiEvent::RetryDelayElapsed, 12_000);
        machine.apply_at(WifiEvent::DriverConfigured, 12_000);
        machine.apply_at(WifiEvent::AssociationFailed, 20_000);
        machine.apply_at(WifiEvent::RetryDelayElapsed, 22_000);
        machine.apply_at(WifiEvent::DriverConfigured, 22_000);

        let transition = machine.apply_at(WifiEvent::Ipv4Configured, 30_001);

        assert_eq!(transition.state, NetworkState::Error);
        assert_eq!(
            transition.failure_code,
            Some(NetworkFailureCode::Ipv4TimedOut)
        );
    }

    #[test]
    fn state_event_matrix_is_deterministic_and_rejects_illegal_events() {
        let states = [
            NetworkState::Disabled,
            NetworkState::Idle,
            NetworkState::Saving,
            NetworkState::Connecting,
            NetworkState::Connected,
            NetworkState::Error,
            NetworkState::Timeout,
        ];
        let events = [
            WifiEvent::ApplyConfig,
            WifiEvent::ClearConfig,
            WifiEvent::DisconnectCompleted,
            WifiEvent::DisconnectTimedOut,
            WifiEvent::DriverConfigured,
            WifiEvent::DriverConfigurationFailed,
            WifiEvent::AssociationSucceeded,
            WifiEvent::AssociationFailed,
            WifiEvent::AssociationTimedOut,
            WifiEvent::Ipv4Configured,
            WifiEvent::Ipv4TimedOut,
            WifiEvent::ProvisioningTimedOut,
            WifiEvent::RetryDelayElapsed,
            WifiEvent::StationDisconnected {
                auto_reconnect: true,
            },
            WifiEvent::StationDisconnected {
                auto_reconnect: false,
            },
            WifiEvent::LanStartupFailed,
        ];

        for state in states {
            for event in events {
                let mut machine = WifiProvisioningMachine {
                    state,
                    failure_code: None,
                    configuration_generation: 1,
                    transition_sequence: 1,
                    attempts: 0,
                    provisioning_started_at_ms: Some(100),
                };
                let expected_acceptance = machine.accepts(event);
                let first = machine.clone().apply_at(event, 100);
                let second = machine.apply_at(event, 100);

                assert_eq!(first, second, "state={state:?}, event={event:?}");
                assert_eq!(
                    first.accepted, expected_acceptance,
                    "state={state:?}, event={event:?}"
                );
                if !expected_acceptance {
                    assert_eq!(first.state, state, "state={state:?}, event={event:?}");
                    assert_eq!(
                        first.transition_sequence, 1,
                        "state={state:?}, event={event:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn driver_configuration_failures_share_the_same_three_attempt_budget() {
        let mut machine = WifiProvisioningMachine::new();
        machine.apply_at(WifiEvent::ApplyConfig, 0);
        machine.apply_at(WifiEvent::DisconnectCompleted, 1);

        for now_ms in [2, 4] {
            let transition = machine.apply_at(WifiEvent::DriverConfigurationFailed, now_ms);
            assert_eq!(transition.state, NetworkState::Connecting);
            assert_eq!(transition.failure_code, None);
            machine.apply_at(WifiEvent::RetryDelayElapsed, now_ms + 1);
        }

        let terminal = machine.apply_at(WifiEvent::DriverConfigurationFailed, 6);
        assert_eq!(terminal.state, NetworkState::Error);
        assert_eq!(
            terminal.failure_code,
            Some(NetworkFailureCode::ConfigurationFailed)
        );
    }
}
