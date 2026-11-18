use super::*;

#[test]
fn tuning_keeps_a_bounded_pause_during_available_pd_rediscovery() {
    // HTTP run 3263 terminated at this transient state: service available,
    // discovering, no fault, no active contract and no host request ticket.
    let recovering = PowerState::from_observation(
        SinkPhase::WaitingForSourceCapabilities,
        None,
        None,
        true,
        None,
    );
    assert_eq!(recovering.protocol, PowerProtocol::Discovering);
    assert_eq!(recovering.failure, None);
    assert_eq!(
        tuning_pd_wait_decision(recovering, None, 818_154, None, false),
        TuningPdWaitDecision::Wait
    );
    assert_eq!(
        tuning_pd_wait_decision(recovering, Some(818_154), 848_154, None, false),
        TuningPdWaitDecision::Wait
    );
    assert_eq!(
        tuning_pd_wait_decision(recovering, Some(818_154), 848_155, None, false),
        TuningPdWaitDecision::Fail("pd_request_timeout")
    );
    // No active PPS contract means no continuing heater permit.
    assert_eq!(
        tuning_pps_transition_decision(TuningPdWaitDecision::Wait, recovering, None, false, false,),
        TuningPdWaitDecision::Wait
    );
    for failed in [
        PowerState {
            available: false,
            ..recovering
        },
        PowerState {
            failure: Some(PowerFailure::Detached),
            ..recovering
        },
    ] {
        assert_eq!(
            tuning_pd_wait_decision(failed, Some(818_154), 818_155, None, false),
            TuningPdWaitDecision::Fail("pd_contract_lost")
        );
    }
}
