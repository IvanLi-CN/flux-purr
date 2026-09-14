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
fn link_and_ipv4_completion_publish_one_connected_snapshot() {
    let mut machine = WifiProvisioningMachine::new();
    machine.apply_at(WifiEvent::ApplyConfig, 0);
    machine.apply_at(WifiEvent::DisconnectCompleted, 1);
    machine.apply_at(WifiEvent::DriverConfigured, 2);

    let linked = machine.apply_at(WifiEvent::AssociationSucceeded, 3);
    assert!(linked.accepted);
    assert_eq!(linked.state, NetworkState::Connecting);

    let connected = machine.apply_at(WifiEvent::Ipv4Configured, 4);
    assert!(connected.accepted);
    assert_eq!(connected.state, NetworkState::Connected);
    assert_eq!(connected.failure_code, None);
    assert!(connected.transition_sequence > linked.transition_sequence);
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
fn cancel_provisioning_stops_the_active_connection_without_clearing_credentials() {
    let mut machine = WifiProvisioningMachine::new();
    let accepted = machine.apply(WifiEvent::ApplyConfig);
    let connecting = machine.apply(WifiEvent::DisconnectCompleted);

    let cancelled = machine.apply(WifiEvent::CancelProvisioning);

    assert!(cancelled.accepted);
    assert_eq!(cancelled.state, NetworkState::Idle);
    assert_eq!(cancelled.failure_code, None);
    assert_eq!(
        cancelled.configuration_generation,
        accepted.configuration_generation
    );
    assert!(cancelled.transition_sequence > connecting.transition_sequence);
    assert_eq!(cancelled.effect, WifiEffect::Disconnect);

    let stale_association = machine.apply(WifiEvent::AssociationSucceeded);
    assert!(!stale_association.accepted);
    assert_eq!(stale_association.state, NetworkState::Idle);

    let retry = machine.apply(WifiEvent::ApplyConfig);
    assert!(retry.accepted);
    assert_eq!(retry.state, NetworkState::Saving);
    assert!(retry.configuration_generation > cancelled.configuration_generation);
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
        WifiEvent::CancelProvisioning,
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
