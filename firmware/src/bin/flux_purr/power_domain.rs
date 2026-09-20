#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PowerProtocol {
    #[default]
    Unknown,
    Discovering,
    WaitingForAccept,
    WaitingForPsRdy,
    Ready,
    Fault,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerFailure {
    Unavailable,
    Detached,
    Reset,
    RequestTimeout,
    TransportFault,
    CapabilityInvalidated,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerIntentOwner {
    ThermalPlantAuto,
    Calibration,
    ManualOperator,
    AutomaticThermal,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
impl PowerIntentOwner {
    const fn priority(self) -> u8 {
        match self {
            Self::ThermalPlantAuto => 4,
            Self::Calibration => 3,
            Self::ManualOperator => 2,
            Self::AutomaticThermal => 1,
        }
    }

    const fn slot(self) -> usize {
        match self {
            Self::ThermalPlantAuto => 0,
            Self::Calibration => 1,
            Self::ManualOperator => 2,
            Self::AutomaticThermal => 3,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PowerTicket {
    owner: Option<PowerIntentOwner>,
    sequence: u16,
    result_slot: usize,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TicketOutcome {
    Confirmed(ConfirmedActiveContract),
    CapabilitiesRefreshed,
    Rejected,
    TimedOut,
    TransportFault,
    Detached,
    Superseded,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PowerState {
    pub(crate) protocol: PowerProtocol,
    pub(crate) available: bool,
    pub(crate) requested: Option<PdContractRequest>,
    pub(crate) active: Option<ConfirmedActiveContract>,
    pub(crate) source_capabilities: SourceCapabilitiesView,
    pub(crate) failure: Option<PowerFailure>,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
impl PowerState {
    pub(crate) const fn unavailable() -> Self {
        Self {
            protocol: PowerProtocol::Unknown,
            available: false,
            requested: None,
            active: None,
            source_capabilities: SourceCapabilitiesView {
                fixed: [None; MAX_SOURCE_PDOS],
                pps: [None; MAX_SOURCE_PDOS],
            },
            failure: Some(PowerFailure::Unavailable),
        }
    }

    pub(crate) fn from_observation(
        observation: Option<PdStatusObservation>,
        capabilities: Option<SourceCapabilities>,
        service_available: bool,
        requested: Option<PdContractRequest>,
    ) -> Self {
        let protocol = match observation {
            Some(observation) => {
                if observation.status.pd_active {
                    PowerProtocol::Ready
                } else {
                    PowerProtocol::Discovering
                }
            }
            None if service_available => PowerProtocol::Discovering,
            None => PowerProtocol::Fault,
        };
        let active = observation.and_then(|observation| {
            ConfirmedActiveContract::from_private_contract(
                observation.contract,
                capabilities.unwrap_or_else(SourceCapabilities::empty),
            )
        });
        Self {
            protocol,
            available: service_available,
            requested,
            active,
            source_capabilities: capabilities
                .unwrap_or_else(SourceCapabilities::empty)
                .view(),
            failure: if service_available {
                active.is_none().then_some(PowerFailure::Detached)
            } else {
                Some(PowerFailure::Unavailable)
            },
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerCommand {
    Request {
        owner: PowerIntentOwner,
        request: PdContractRequest,
        ticket: PowerTicket,
    },
    RefreshCapabilities {
        ticket: PowerTicket,
    },
    Idle {
        ticket: PowerTicket,
    },
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PdServiceReport {
    pub(crate) state: PowerState,
    pub(crate) terminal: Option<(PowerTicket, TicketOutcome)>,
}

#[cfg(target_arch = "xtensa")]
static POWER_COMMANDS: Channel<CriticalSectionRawMutex, PowerCommand, 6> = Channel::new();

#[cfg(target_arch = "xtensa")]
static POWER_STATE: Watch<CriticalSectionRawMutex, PowerState, 5> =
    Watch::new_with(PowerState::unavailable());

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_RESULTS: [Signal<CriticalSectionRawMutex, (PowerTicket, TicketOutcome)>; 5] =
    [const { Signal::new() }; 5];

#[cfg(target_arch = "xtensa")]
static PD_SERVICE_REPORT: Signal<CriticalSectionRawMutex, PdServiceReport> = Signal::new();

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn request_matches_active(
    request: PdContractRequest,
    active: Option<ConfirmedActiveContract>,
) -> bool {
    active.is_some_and(|active| {
        active.mode == request.mode
            && active.voltage_mv == request.voltage_mv
            && active.operating_current_ma == request.operating_current_ma
    })
}

#[cfg(any(target_arch = "xtensa", test))]
#[cfg_attr(not(target_arch = "xtensa"), allow(dead_code))]
pub(crate) fn adjustable_capabilities_from_view(
    view: SourceCapabilitiesView,
) -> Option<ch224q::AdjustablePowerCapabilities> {
    let mut capabilities = ch224q::AdjustablePowerCapabilities::default();
    let mut has_pps = false;
    for range in view.pps.into_iter().flatten() {
        has_pps = true;
        capabilities.pps_min_mv = Some(
            capabilities
                .pps_min_mv
                .map_or(range.min_mv, |value| value.min(range.min_mv)),
        );
        capabilities.pps_max_mv = Some(
            capabilities
                .pps_max_mv
                .map_or(range.max_mv, |value| value.max(range.max_mv)),
        );
        capabilities.pps_max_ma = Some(
            capabilities
                .pps_max_ma
                .map_or(range.max_current_ma, |value| {
                    value.max(range.max_current_ma)
                }),
        );
        capabilities.pps_covers_20v |=
            range.min_mv <= GUARANTEED_HEATER_MIN_MV && range.max_mv >= GUARANTEED_HEATER_MIN_MV;
        if let Some(slot) = capabilities
            .pps_apdos
            .iter_mut()
            .find(|slot| slot.is_none())
        {
            *slot = Some(ch224q::PpsApdo {
                min_mv: range.min_mv,
                max_mv: range.max_mv,
                max_ma: range.max_current_ma,
            });
        }
    }
    has_pps.then_some(capabilities)
}

#[cfg(target_arch = "xtensa")]
#[allow(dead_code)]
pub(crate) struct PowerStateSubscription<'a> {
    receiver: embassy_sync::watch::Receiver<'a, CriticalSectionRawMutex, PowerState, 5>,
}

#[cfg(target_arch = "xtensa")]
#[allow(dead_code)]
impl PowerStateSubscription<'_> {
    pub(crate) fn try_get(&mut self) -> Option<PowerState> {
        self.receiver.try_get()
    }

    pub(crate) fn try_changed(&mut self) -> Option<PowerState> {
        self.receiver.try_changed()
    }

    pub(crate) async fn get(&mut self) -> PowerState {
        self.receiver.get().await
    }

    pub(crate) async fn changed(&mut self) -> PowerState {
        self.receiver.changed().await
    }
}

#[cfg(target_arch = "xtensa")]
#[allow(dead_code)]
#[derive(Clone, Copy, Default)]
pub(crate) struct PowerCoordinatorClient;

#[cfg(target_arch = "xtensa")]
#[allow(dead_code)]
impl PowerCoordinatorClient {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) fn subscribe(&self) -> Option<PowerStateSubscription<'static>> {
        POWER_STATE
            .receiver()
            .map(|receiver| PowerStateSubscription { receiver })
    }

    fn next_ticket(&self, owner: Option<PowerIntentOwner>) -> PowerTicket {
        static NEXT_TICKET: AtomicU16 = AtomicU16::new(1);
        PowerTicket {
            owner,
            sequence: NEXT_TICKET.fetch_add(1, Ordering::Relaxed),
            result_slot: owner.map_or(4, PowerIntentOwner::slot),
        }
    }

    pub(crate) fn request(
        &self,
        owner: PowerIntentOwner,
        request: PdContractRequest,
    ) -> Result<PowerTicket, ()> {
        let ticket = self.next_ticket(Some(owner));
        POWER_COMMANDS
            .try_send(PowerCommand::Request {
                owner,
                request,
                ticket,
            })
            .map(|()| ticket)
            .map_err(|_| ())
    }

    pub(crate) fn refresh_capabilities(&self) -> Result<PowerTicket, ()> {
        let ticket = self.next_ticket(None);
        POWER_COMMANDS
            .try_send(PowerCommand::RefreshCapabilities { ticket })
            .map(|()| ticket)
            .map_err(|_| ())
    }

    pub(crate) fn idle(&self) -> Result<PowerTicket, ()> {
        let ticket = self.next_ticket(None);
        POWER_COMMANDS
            .try_send(PowerCommand::Idle { ticket })
            .map(|()| ticket)
            .map_err(|_| ())
    }

    pub(crate) fn try_take(&self, ticket: PowerTicket) -> Option<TicketOutcome> {
        let (completed, outcome) = POWER_TICKET_RESULTS[ticket.result_slot].try_take()?;
        (completed == ticket).then_some(outcome)
    }

    pub(crate) async fn wait(&self, ticket: PowerTicket) -> TicketOutcome {
        let slot = ticket.result_slot;
        loop {
            let (completed, outcome) = POWER_TICKET_RESULTS[slot].wait().await;
            if completed == ticket {
                return outcome;
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn publish_pd_service_report(report: PdServiceReport) {
    PD_SERVICE_REPORT.signal(report);
}

#[cfg(target_arch = "xtensa")]
fn publish_power_state(state: PowerState) {
    POWER_STATE
        .sender()
        .send_if_modified(|current| match current {
            Some(current) if *current == state => false,
            current => {
                *current = Some(state);
                true
            }
        });
}

#[cfg(target_arch = "xtensa")]
fn signal_ticket(ticket: PowerTicket, outcome: TicketOutcome) {
    POWER_TICKET_RESULTS[ticket.result_slot].signal((ticket, outcome));
}

#[cfg(target_arch = "xtensa")]
fn power_command_details(
    command: PowerCommand,
) -> (
    PowerTicket,
    Option<PowerIntentOwner>,
    bool,
    Option<PdContractRequest>,
) {
    match command {
        PowerCommand::Request {
            owner,
            request,
            ticket,
        } => (ticket, Some(owner), false, Some(request)),
        PowerCommand::RefreshCapabilities { ticket } => (ticket, None, true, None),
        PowerCommand::Idle { ticket } => (ticket, None, false, None),
    }
}

#[cfg(target_arch = "xtensa")]
fn power_pd_command(
    command: PowerCommand,
) -> Option<(
    PowerTicket,
    Option<PowerIntentOwner>,
    bool,
    PdServiceCommand,
)> {
    let (ticket, owner, refresh, request) = power_command_details(command);
    let pd_command = match (command, request) {
        (PowerCommand::Request { request, .. }, Some(_)) => {
            PdServiceCommand::Contract { request, ticket }
        }
        (PowerCommand::RefreshCapabilities { .. }, None) => {
            PdServiceCommand::RefreshCapabilities { ticket }
        }
        (PowerCommand::Idle { .. }, None) => PdServiceCommand::AutomaticIdle { ticket },
        _ => return None,
    };
    Some((ticket, owner, refresh, pd_command))
}

#[cfg(target_arch = "xtensa")]
fn supersede_or_join_power_command(
    ticket: PowerTicket,
    owner: Option<PowerIntentOwner>,
    refresh: bool,
    inflight: &mut Option<(PowerIntentOwner, PowerTicket)>,
    inflight_refresh: &mut bool,
    joined_refresh: &mut Option<PowerTicket>,
) -> bool {
    if refresh && *inflight_refresh {
        *joined_refresh = Some(ticket);
        return true;
    }
    let Some((active_owner, old_ticket)) = *inflight else {
        return false;
    };
    match owner {
        Some(owner) if owner.priority() > active_owner.priority() => {
            signal_ticket(old_ticket, TicketOutcome::Superseded);
            *inflight = None;
            *inflight_refresh = false;
            false
        }
        _ => {
            signal_ticket(ticket, TicketOutcome::Superseded);
            true
        }
    }
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn power_coordinator_task() {
    let mut inflight: Option<(PowerIntentOwner, PowerTicket)> = None;
    let mut inflight_refresh = false;
    let mut joined_refresh: Option<PowerTicket> = None;
    loop {
        match select(POWER_COMMANDS.receive(), PD_SERVICE_REPORT.wait()).await {
            Either::First(command) => {
                let Some((ticket, owner, refresh, pd_command)) = power_pd_command(command) else {
                    let (ticket, _, _, _) = power_command_details(command);
                    signal_ticket(ticket, TicketOutcome::Rejected);
                    continue;
                };
                if supersede_or_join_power_command(
                    ticket,
                    owner,
                    refresh,
                    &mut inflight,
                    &mut inflight_refresh,
                    &mut joined_refresh,
                ) {
                    continue;
                }
                if try_send_pd_service_command(pd_command).is_err() {
                    signal_ticket(ticket, TicketOutcome::TransportFault);
                } else if let Some(owner) = owner {
                    inflight = Some((owner, ticket));
                    inflight_refresh = false;
                } else {
                    inflight = Some((PowerIntentOwner::AutomaticThermal, ticket));
                    inflight_refresh = refresh;
                }
            }
            Either::Second(report) => {
                publish_power_state(report.state);
                if let Some((ticket, outcome)) = report.terminal {
                    signal_ticket(ticket, outcome);
                    if let Some(joined) = joined_refresh.take() {
                        signal_ticket(joined, outcome);
                    }
                    if inflight.is_some_and(|(_, current)| current == ticket) {
                        inflight = None;
                        inflight_refresh = false;
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn spawn_power_coordinator(spawner: Spawner) {
    spawner
        .spawn(power_coordinator_task())
        .expect("failed to spawn power coordinator");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_contract_match_requires_mode_voltage_and_current() {
        let request = PdContractRequest::pps(20_000, 3_000).unwrap();
        let active = ConfirmedActiveContract {
            mode: PdContractRequestMode::Pps,
            voltage_mv: 20_000,
            operating_current_ma: 3_000,
            pps_range: None,
        };
        assert!(request_matches_active(request, Some(active)));
        assert!(!request_matches_active(
            PdContractRequest::fixed(20_000, 3_000).unwrap(),
            Some(active),
        ));
        assert!(!request_matches_active(
            PdContractRequest::pps(20_000, 5_000).unwrap(),
            Some(active),
        ));
    }
}
