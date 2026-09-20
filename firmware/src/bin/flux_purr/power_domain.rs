#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PowerProtocol {
    #[default]
    Unknown,
    Detached,
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
        phase: SinkPhase,
        observation: Option<PdStatusObservation>,
        capabilities: Option<SourceCapabilities>,
        service_available: bool,
        requested: Option<PdContractRequest>,
    ) -> Self {
        let protocol = if !service_available {
            PowerProtocol::Unknown
        } else {
            match phase {
                SinkPhase::Detached => PowerProtocol::Detached,
                SinkPhase::WaitingForSourceCapabilities => PowerProtocol::Discovering,
                SinkPhase::WaitingForAccept => PowerProtocol::WaitingForAccept,
                SinkPhase::WaitingForPsRdy => PowerProtocol::WaitingForPsRdy,
                SinkPhase::Ready if observation.is_some() => PowerProtocol::Ready,
                SinkPhase::Ready => PowerProtocol::Fault,
                SinkPhase::Fault => PowerProtocol::Fault,
            }
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
            failure: if !service_available {
                Some(PowerFailure::Unavailable)
            } else {
                match phase {
                    SinkPhase::Detached => Some(PowerFailure::Detached),
                    SinkPhase::Fault | SinkPhase::Ready if observation.is_none() => {
                        Some(PowerFailure::TransportFault)
                    }
                    _ => None,
                }
            },
        }
    }

    pub(crate) fn observation(self) -> Option<PdStatusObservation> {
        if !self.available {
            return None;
        }
        let active = self.active?;
        let kind = match active.mode {
            PdContractRequestMode::Fixed => ContractKind::Fixed,
            PdContractRequestMode::Pps => ContractKind::Pps,
        };
        Some(PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: active.operating_current_ma,
            contract_voltage_mv: Some(active.voltage_mv),
            contract: Contract {
                kind,
                object_position: 0,
                voltage_mv: active.voltage_mv,
                current_ma: active.operating_current_ma,
            },
        })
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
pub(crate) enum PdServiceCommand {
    AutomaticIdle {
        ticket: PowerTicket,
    },
    Contract {
        request: PdContractRequest,
        ticket: PowerTicket,
    },
    RefreshCapabilities {
        ticket: PowerTicket,
    },
}

#[cfg(target_arch = "xtensa")]
const POWER_COMMAND_CAPACITY: usize = 1;

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_COMMANDS: Channel<
    CriticalSectionRawMutex,
    PdServiceCommand,
    POWER_COMMAND_CAPACITY,
> = Channel::new();

#[cfg(target_arch = "xtensa")]
static PD_SERVICE_STATE: Watch<CriticalSectionRawMutex, PowerState, 1> =
    Watch::new_with(PowerState::unavailable());

#[cfg(target_arch = "xtensa")]
static PD_SERVICE_TERMINALS: Channel<
    CriticalSectionRawMutex,
    (PowerState, PowerTicket, TicketOutcome),
    POWER_COMMANDS_CAPACITY,
> = Channel::new();

#[cfg(target_arch = "xtensa")]
const POWER_COMMANDS_CAPACITY: usize = 6;

#[cfg(target_arch = "xtensa")]
static POWER_COMMANDS: Channel<CriticalSectionRawMutex, PowerCommand, POWER_COMMANDS_CAPACITY> =
    Channel::new();

#[cfg(target_arch = "xtensa")]
static POWER_STATE: Watch<CriticalSectionRawMutex, PowerState, 5> =
    Watch::new_with(PowerState::unavailable());

#[cfg(target_arch = "xtensa")]
const POWER_TICKET_SLOT_COUNT: usize = POWER_COMMANDS_CAPACITY;

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_RESULTS: [Signal<CriticalSectionRawMutex, (PowerTicket, TicketOutcome)>;
    POWER_TICKET_SLOT_COUNT] = [const { Signal::new() }; POWER_TICKET_SLOT_COUNT];

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_SLOTS: AtomicU8 = AtomicU8::new(0);

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

    fn next_ticket(&self, owner: Option<PowerIntentOwner>) -> Option<PowerTicket> {
        let mut slots = POWER_TICKET_SLOTS.load(Ordering::Acquire);
        let mask = (1u8 << POWER_TICKET_SLOT_COUNT) - 1;
        let slot = loop {
            let free = (!slots) & mask;
            let slot = free.trailing_zeros() as usize;
            if slot >= POWER_TICKET_SLOT_COUNT {
                return None;
            }
            let next = slots | (1u8 << slot);
            match POWER_TICKET_SLOTS.compare_exchange_weak(
                slots,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break slot,
                Err(current) => slots = current,
            }
        };
        static NEXT_TICKET: AtomicU16 = AtomicU16::new(1);
        Some(PowerTicket {
            owner,
            sequence: NEXT_TICKET.fetch_add(1, Ordering::Relaxed),
            result_slot: slot,
        })
    }

    fn release_ticket_slot(ticket: PowerTicket) {
        POWER_TICKET_SLOTS.fetch_and(!(1u8 << ticket.result_slot), Ordering::Release);
    }

    pub(crate) fn request(
        &self,
        owner: PowerIntentOwner,
        request: PdContractRequest,
    ) -> Result<PowerTicket, ()> {
        let Some(ticket) = self.next_ticket(Some(owner)) else {
            return Err(());
        };
        POWER_COMMANDS
            .try_send(PowerCommand::Request {
                owner,
                request,
                ticket,
            })
            .map(|()| ticket)
            .map_err(|_| {
                Self::release_ticket_slot(ticket);
            })
    }

    pub(crate) fn refresh_capabilities(&self) -> Result<PowerTicket, ()> {
        let Some(ticket) = self.next_ticket(None) else {
            return Err(());
        };
        POWER_COMMANDS
            .try_send(PowerCommand::RefreshCapabilities { ticket })
            .map(|()| ticket)
            .map_err(|_| {
                Self::release_ticket_slot(ticket);
            })
    }

    pub(crate) fn idle(&self) -> Result<PowerTicket, ()> {
        let Some(ticket) = self.next_ticket(None) else {
            return Err(());
        };
        POWER_COMMANDS
            .try_send(PowerCommand::Idle { ticket })
            .map(|()| ticket)
            .map_err(|_| {
                Self::release_ticket_slot(ticket);
            })
    }

    pub(crate) fn try_take(&self, ticket: PowerTicket) -> Option<TicketOutcome> {
        let result = POWER_TICKET_RESULTS[ticket.result_slot].try_take()?;
        if result.0 == ticket {
            Self::release_ticket_slot(ticket);
            Some(result.1)
        } else {
            POWER_TICKET_RESULTS[ticket.result_slot].signal(result);
            None
        }
    }

    pub(crate) async fn wait(&self, ticket: PowerTicket) -> TicketOutcome {
        let slot = ticket.result_slot;
        loop {
            let (completed, outcome) = POWER_TICKET_RESULTS[slot].wait().await;
            if completed == ticket {
                Self::release_ticket_slot(ticket);
                return outcome;
            }
            POWER_TICKET_RESULTS[slot].signal((completed, outcome));
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn publish_pd_service_state(state: PowerState) {
    PD_SERVICE_STATE.sender().send_if_modified(|current| {
        if current.as_ref().is_some_and(|current| *current == state) {
            false
        } else {
            *current = Some(state);
            true
        }
    });
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn publish_pd_service_terminal(
    state: PowerState,
    ticket: PowerTicket,
    outcome: TicketOutcome,
) {
    PD_SERVICE_TERMINALS
        .sender()
        .send((state, ticket, outcome))
        .await;
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn try_send_pd_service_command(command: PdServiceCommand) -> Result<(), ()> {
    PD_SERVICE_COMMANDS.try_send(command).map_err(|_| ())
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
    bool,
    Option<PdContractRequest>,
) {
    match command {
        PowerCommand::Request {
            owner,
            request,
            ticket,
        } => (ticket, Some(owner), false, false, Some(request)),
        PowerCommand::RefreshCapabilities { ticket } => (ticket, None, true, false, None),
        PowerCommand::Idle { ticket } => (ticket, None, false, true, None),
    }
}

#[cfg(target_arch = "xtensa")]
fn power_pd_command(
    command: PowerCommand,
) -> Option<(
    PowerTicket,
    Option<PowerIntentOwner>,
    bool,
    bool,
    PdServiceCommand,
)> {
    let (ticket, owner, refresh, idle, request) = power_command_details(command);
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
    Some((ticket, owner, refresh, idle, pd_command))
}

#[cfg(target_arch = "xtensa")]
fn settle_inflight(
    inflight: &mut Option<(PowerIntentOwner, PowerTicket)>,
    inflight_refresh: &mut bool,
    joined_refresh: &mut heapless::Vec<PowerTicket, POWER_COMMANDS_CAPACITY>,
    outcome: TicketOutcome,
) {
    if let Some((_, ticket)) = inflight.take() {
        signal_ticket(ticket, outcome);
    }
    *inflight_refresh = false;
    while let Some(ticket) = joined_refresh.pop() {
        signal_ticket(ticket, outcome);
    }
}

#[cfg(target_arch = "xtensa")]
fn supersede_or_join_power_command(
    ticket: PowerTicket,
    owner: Option<PowerIntentOwner>,
    refresh: bool,
    idle: bool,
    inflight: &mut Option<(PowerIntentOwner, PowerTicket)>,
    inflight_refresh: &mut bool,
    joined_refresh: &mut heapless::Vec<PowerTicket, POWER_COMMANDS_CAPACITY>,
) -> bool {
    if refresh && *inflight_refresh {
        if joined_refresh.push(ticket).is_err() {
            signal_ticket(ticket, TicketOutcome::Rejected);
        }
        return true;
    }
    let Some((active_owner, _old_ticket)) = *inflight else {
        return false;
    };
    match owner {
        _ if idle => {
            settle_inflight(
                inflight,
                inflight_refresh,
                joined_refresh,
                TicketOutcome::Superseded,
            );
            false
        }
        Some(owner) if owner.priority() > active_owner.priority() => {
            settle_inflight(
                inflight,
                inflight_refresh,
                joined_refresh,
                TicketOutcome::Superseded,
            );
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
    let mut joined_refresh = heapless::Vec::<PowerTicket, POWER_COMMANDS_CAPACITY>::new();
    let mut pd_state = PD_SERVICE_STATE
        .receiver()
        .expect("power coordinator state receiver capacity is reserved");
    loop {
        match select3(
            POWER_COMMANDS.receive(),
            PD_SERVICE_TERMINALS.receive(),
            pd_state.changed(),
        )
        .await
        {
            Either3::First(command) => {
                let Some((ticket, owner, refresh, idle, pd_command)) = power_pd_command(command)
                else {
                    let (ticket, _, _, _, _) = power_command_details(command);
                    signal_ticket(ticket, TicketOutcome::Rejected);
                    continue;
                };
                if supersede_or_join_power_command(
                    ticket,
                    owner,
                    refresh,
                    idle,
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
            Either3::Second((state, ticket, outcome)) => {
                publish_power_state(state);
                if inflight.is_some_and(|(_, current)| current == ticket) {
                    signal_ticket(ticket, outcome);
                    inflight = None;
                    inflight_refresh = false;
                    while let Some(joined) = joined_refresh.pop() {
                        signal_ticket(joined, outcome);
                    }
                }
            }
            Either3::Third(state) => publish_power_state(state),
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

    #[test]
    fn protocol_projection_preserves_negotiation_phase_without_fake_detach() {
        let state = PowerState::from_observation(
            SinkPhase::WaitingForAccept,
            None,
            Some(SourceCapabilities::empty()),
            true,
            None,
        );
        assert_eq!(state.protocol, PowerProtocol::WaitingForAccept);
        assert_eq!(state.failure, None);
    }
}
