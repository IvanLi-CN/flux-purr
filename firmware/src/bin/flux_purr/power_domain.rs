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
    sequence: u32,
    result_slot: usize,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerAdmissionError {
    Busy,
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
        let active = observation.and_then(|observation| {
            ConfirmedActiveContract::from_private_contract(
                observation.contract,
                capabilities.unwrap_or_else(SourceCapabilities::empty),
            )
        });
        let protocol = if !service_available {
            PowerProtocol::Unknown
        } else {
            match phase {
                SinkPhase::Detached => PowerProtocol::Detached,
                SinkPhase::WaitingForSourceCapabilities => PowerProtocol::Discovering,
                SinkPhase::WaitingForAccept => PowerProtocol::WaitingForAccept,
                SinkPhase::WaitingForPsRdy => PowerProtocol::WaitingForPsRdy,
                SinkPhase::Ready if active.is_some() => PowerProtocol::Ready,
                SinkPhase::Ready => PowerProtocol::Fault,
                SinkPhase::Fault => PowerProtocol::Fault,
            }
        };
        let failure = power_state_failure(phase, observation, active, service_available);
        let source_capabilities = if failure.is_some() {
            SourceCapabilities::empty().view()
        } else {
            capabilities
                .unwrap_or_else(SourceCapabilities::empty)
                .view()
        };
        Self {
            protocol,
            available: service_available,
            requested,
            active,
            source_capabilities,
            failure,
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
        let status_raw = FUSB302B_STATUS0_VBUSOK;
        Some(PdStatusObservation {
            status_raw,
            status: fusb302b_status_projection(status_raw),
            current_raw: 0,
            current_ma: active.operating_current_ma,
            contract_voltage_mv: Some(active.voltage_mv),
            contract: Contract::observed(kind, active.voltage_mv, active.operating_current_ma),
        })
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn power_state_failure(
    phase: SinkPhase,
    observation: Option<PdStatusObservation>,
    active: Option<ConfirmedActiveContract>,
    service_available: bool,
) -> Option<PowerFailure> {
    if !service_available {
        Some(PowerFailure::Unavailable)
    } else {
        match phase {
            SinkPhase::Detached => Some(PowerFailure::Detached),
            SinkPhase::Fault => Some(PowerFailure::TransportFault),
            SinkPhase::Ready if observation.is_none() => Some(PowerFailure::TransportFault),
            _ if observation.is_some() && active.is_none() => {
                Some(PowerFailure::CapabilityInvalidated)
            }
            _ => None,
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
static PD_SERVICE_CANCELLED_TICKET: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "xtensa")]
static PD_SERVICE_STATE: Watch<CriticalSectionRawMutex, PowerState, 1> =
    Watch::new_with(PowerState::unavailable());

#[cfg(target_arch = "xtensa")]
static PD_SERVICE_TERMINALS: Channel<
    CriticalSectionRawMutex,
    (PowerState, PowerTicket, TicketOutcome),
    POWER_COMMANDS_CAPACITY,
> = Channel::new();

#[cfg(any(target_arch = "xtensa", test))]
const POWER_COMMANDS_CAPACITY: usize = 6;

#[cfg(target_arch = "xtensa")]
static POWER_COMMANDS: Channel<CriticalSectionRawMutex, PowerCommand, POWER_COMMANDS_CAPACITY> =
    Channel::new();

#[cfg(target_arch = "xtensa")]
static POWER_STATE: Watch<CriticalSectionRawMutex, PowerState, 5> =
    Watch::new_with(PowerState::unavailable());

#[cfg(target_arch = "xtensa")]
static POWER_STATE_LATEST: BlockingMutex<CriticalSectionRawMutex, RefCell<PowerState>> =
    BlockingMutex::new(RefCell::new(PowerState::unavailable()));

#[cfg(any(target_arch = "xtensa", test))]
const POWER_TICKET_SLOT_COUNT: usize = POWER_COMMANDS_CAPACITY;

#[cfg(any(target_arch = "xtensa", test))]
fn reserve_ticket_slot(slots: &AtomicU8) -> Option<usize> {
    let mask = (1u8 << POWER_TICKET_SLOT_COUNT) - 1;
    let mut current = slots.load(Ordering::Acquire);
    loop {
        let free = (!current) & mask;
        let slot = free.trailing_zeros() as usize;
        if slot >= POWER_TICKET_SLOT_COUNT {
            return None;
        }
        match slots.compare_exchange_weak(
            current,
            current | (1u8 << slot),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(slot),
            Err(next) => current = next,
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn release_ticket_slot(slots: &AtomicU8, slot: usize) {
    slots.fetch_and(!(1u8 << slot), Ordering::Release);
}

#[cfg(any(target_arch = "xtensa", test))]
fn release_discarded_ticket_slot(discarded: &AtomicU8, slots: &AtomicU8, slot: usize) -> bool {
    let mask = 1u8 << slot;
    if discarded.fetch_and(!mask, Ordering::AcqRel) & mask == 0 {
        return false;
    }
    release_ticket_slot(slots, slot);
    true
}

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_RESULTS: [Signal<CriticalSectionRawMutex, (PowerTicket, TicketOutcome)>;
    POWER_TICKET_SLOT_COUNT] = [const { Signal::new() }; POWER_TICKET_SLOT_COUNT];

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_SLOTS: AtomicU8 = AtomicU8::new(0);

#[cfg(target_arch = "xtensa")]
static POWER_TICKET_DISCARDED: AtomicU8 = AtomicU8::new(0);

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn request_matches_active(
    request: PdContractRequest,
    active: Option<ConfirmedActiveContract>,
) -> bool {
    active.is_some_and(|active| {
        active.mode == request.mode()
            && active.voltage_mv == request.voltage_mv()
            && active.operating_current_ma == request.operating_current_ma()
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pps_adjustment_is_continuous(state: PowerState, request: PdContractRequest) -> bool {
    if !state.available || state.failure.is_some() || request.mode() != PdContractRequestMode::Pps {
        return false;
    }
    state.active.is_some_and(|active| {
        active.request_keeps_same_pps_apdo(state.source_capabilities, request)
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn should_join_refresh(refresh: bool, inflight_refresh: bool) -> bool {
    refresh && inflight_refresh
}

#[cfg(any(target_arch = "xtensa", test))]
fn terminal_matches_inflight(
    inflight: Option<(PowerIntentOwner, PowerTicket)>,
    ticket: PowerTicket,
) -> bool {
    inflight.is_some_and(|(_, current)| current == ticket)
}

#[cfg(any(target_arch = "xtensa", test))]
fn active_owner_after_terminal(
    current: Option<PowerIntentOwner>,
    completed: Option<PowerIntentOwner>,
    was_refresh: bool,
    was_idle: bool,
    outcome: TicketOutcome,
) -> Option<PowerIntentOwner> {
    if was_refresh
        && !matches!(
            outcome,
            TicketOutcome::TimedOut | TicketOutcome::TransportFault | TicketOutcome::Detached
        )
    {
        current
    } else if matches!(outcome, TicketOutcome::Confirmed(_)) && !was_idle {
        completed
    } else {
        None
    }
}

#[cfg(any(target_arch = "xtensa", test))]
fn failed_contract_owner_for_terminal(
    inflight: Option<(PowerIntentOwner, PowerTicket)>,
    ticket: PowerTicket,
    was_refresh: bool,
    was_idle: bool,
    outcome: TicketOutcome,
) -> Option<PowerIntentOwner> {
    if was_refresh
        || was_idle
        || matches!(
            outcome,
            TicketOutcome::Confirmed(_) | TicketOutcome::CapabilitiesRefreshed
        )
        || !terminal_matches_inflight(inflight, ticket)
    {
        None
    } else {
        inflight.map(|(owner, _)| owner)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_pd_service_work<T: Copy, U>(
    pending: Option<T>,
    queued_command: Option<U>,
) -> (Option<T>, Option<U>) {
    if queued_command.is_some() {
        (None, queued_command)
    } else {
        (pending, None)
    }
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

    pub(crate) fn latest(&self) -> PowerState {
        POWER_STATE_LATEST.lock(|state| *state.borrow())
    }

    fn next_ticket(&self, owner: Option<PowerIntentOwner>) -> Option<PowerTicket> {
        let slot = reserve_ticket_slot(&POWER_TICKET_SLOTS)?;
        static NEXT_TICKET: AtomicU32 = AtomicU32::new(1);
        Some(PowerTicket {
            owner,
            sequence: NEXT_TICKET.fetch_add(1, Ordering::Relaxed),
            result_slot: slot,
        })
    }

    fn release_ticket_slot(ticket: PowerTicket) {
        release_ticket_slot(&POWER_TICKET_SLOTS, ticket.result_slot);
    }

    pub(crate) fn discard(&self, ticket: PowerTicket) {
        let mask = 1u8 << ticket.result_slot;
        if let Some(result) = POWER_TICKET_RESULTS[ticket.result_slot].try_take() {
            if result.0 == ticket {
                Self::release_ticket_slot(ticket);
                return;
            }
            POWER_TICKET_RESULTS[ticket.result_slot].signal(result);
        }
        POWER_TICKET_DISCARDED.fetch_or(mask, Ordering::AcqRel);
        if let Some(result) = POWER_TICKET_RESULTS[ticket.result_slot].try_take() {
            POWER_TICKET_DISCARDED.fetch_and(!mask, Ordering::Release);
            if result.0 == ticket {
                Self::release_ticket_slot(ticket);
            } else {
                POWER_TICKET_RESULTS[ticket.result_slot].signal(result);
            }
        }
    }

    pub(crate) fn request(
        &self,
        owner: PowerIntentOwner,
        request: PdContractRequest,
    ) -> Result<PowerTicket, PowerAdmissionError> {
        let Some(ticket) = self.next_ticket(Some(owner)) else {
            return Err(PowerAdmissionError::Busy);
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
                PowerAdmissionError::Busy
            })
    }

    pub(crate) fn refresh_capabilities(&self) -> Result<PowerTicket, PowerAdmissionError> {
        let Some(ticket) = self.next_ticket(None) else {
            return Err(PowerAdmissionError::Busy);
        };
        POWER_COMMANDS
            .try_send(PowerCommand::RefreshCapabilities { ticket })
            .map(|()| ticket)
            .map_err(|_| {
                Self::release_ticket_slot(ticket);
                PowerAdmissionError::Busy
            })
    }

    pub(crate) fn idle(&self) -> Result<PowerTicket, PowerAdmissionError> {
        let Some(ticket) = self.next_ticket(None) else {
            return Err(PowerAdmissionError::Busy);
        };
        POWER_COMMANDS
            .try_send(PowerCommand::Idle { ticket })
            .map(|()| ticket)
            .map_err(|_| {
                Self::release_ticket_slot(ticket);
                PowerAdmissionError::Busy
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
    POWER_STATE_LATEST.lock(|current| *current.borrow_mut() = state);
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
    if !release_discarded_ticket_slot(
        &POWER_TICKET_DISCARDED,
        &POWER_TICKET_SLOTS,
        ticket.result_slot,
    ) {
        POWER_TICKET_RESULTS[ticket.result_slot].signal((ticket, outcome));
    }
}

#[cfg(target_arch = "xtensa")]
fn ticket_key(ticket: PowerTicket) -> u32 {
    ticket.sequence
}

#[cfg(target_arch = "xtensa")]
fn cancel_pd_service_ticket(ticket: PowerTicket) {
    PD_SERVICE_CANCELLED_TICKET.store(ticket_key(ticket), Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn take_pd_service_command_cancelled(command: PdServiceCommand) -> bool {
    PD_SERVICE_CANCELLED_TICKET
        .compare_exchange(
            ticket_key(pd_service_command_ticket(command)),
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn pd_service_command_ticket(command: PdServiceCommand) -> PowerTicket {
    match command {
        PdServiceCommand::AutomaticIdle { ticket }
        | PdServiceCommand::Contract { ticket, .. }
        | PdServiceCommand::RefreshCapabilities { ticket } => ticket,
    }
}

#[cfg(any(target_arch = "xtensa", test))]
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
        cancel_pd_service_ticket(ticket);
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
    if should_join_refresh(refresh, *inflight_refresh) {
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

#[cfg(any(target_arch = "xtensa", test))]
fn owner_is_superseded(
    owner: Option<PowerIntentOwner>,
    active_owner: Option<PowerIntentOwner>,
) -> bool {
    owner.is_some_and(|owner| {
        active_owner.is_some_and(|active| owner.priority() < active.priority())
    })
}

#[cfg(any(target_arch = "xtensa", test))]
fn queued_command_is_preempted_by_failed_owner(
    command: PowerCommand,
    failed_owner: PowerIntentOwner,
) -> bool {
    let (_, owner, refresh, idle, _) = power_command_details(command);
    if idle {
        return false;
    }
    if refresh {
        return true;
    }
    owner.is_some_and(|owner| owner.priority() <= failed_owner.priority())
}

#[cfg(target_arch = "xtensa")]
struct PowerCoordinatorState {
    inflight: Option<(PowerIntentOwner, PowerTicket)>,
    inflight_refresh: bool,
    inflight_idle: bool,
    active_owner: Option<PowerIntentOwner>,
    joined_refresh: heapless::Vec<PowerTicket, POWER_COMMANDS_CAPACITY>,
    deferred: Option<PowerCommand>,
}

#[cfg(target_arch = "xtensa")]
impl PowerCoordinatorState {
    fn new() -> Self {
        Self {
            inflight: None,
            inflight_refresh: false,
            inflight_idle: false,
            active_owner: None,
            joined_refresh: heapless::Vec::new(),
            deferred: None,
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn dispatch_power_command(
    command: PowerCommand,
    coordinator: &mut PowerCoordinatorState,
) -> Option<PowerIntentOwner> {
    let (_, new_owner, new_refresh, new_idle, _) = power_command_details(command);
    if let Some(deferred) = coordinator.deferred {
        let (deferred_ticket, deferred_owner, _, _, _) = power_command_details(deferred);
        let new_priority = new_owner.map_or(0, PowerIntentOwner::priority);
        let deferred_priority = deferred_owner.map_or(0, PowerIntentOwner::priority);
        if new_idle || new_refresh || new_priority >= deferred_priority {
            coordinator.deferred = None;
            signal_ticket(deferred_ticket, TicketOutcome::Superseded);
        } else {
            let (ticket, _, _, _, _) = power_command_details(command);
            signal_ticket(ticket, TicketOutcome::Superseded);
            return None;
        }
    }
    let Some((ticket, owner, refresh, idle, pd_command)) = power_pd_command(command) else {
        let (ticket, _, _, _, _) = power_command_details(command);
        signal_ticket(ticket, TicketOutcome::Rejected);
        return None;
    };
    if owner_is_superseded(owner, coordinator.active_owner) {
        signal_ticket(ticket, TicketOutcome::Superseded);
        return None;
    }
    if supersede_or_join_power_command(
        ticket,
        owner,
        refresh,
        idle,
        &mut coordinator.inflight,
        &mut coordinator.inflight_refresh,
        &mut coordinator.joined_refresh,
    ) {
        return None;
    }
    if try_send_pd_service_command(pd_command).is_err() {
        if coordinator.deferred.is_some() {
            signal_ticket(ticket, TicketOutcome::TransportFault);
        } else {
            coordinator.deferred = Some(command);
        }
        owner
    } else if let Some(owner) = owner {
        coordinator.inflight = Some((owner, ticket));
        coordinator.inflight_refresh = false;
        coordinator.inflight_idle = false;
        None
    } else {
        coordinator.inflight = Some((PowerIntentOwner::AutomaticThermal, ticket));
        coordinator.inflight_refresh = refresh;
        coordinator.inflight_idle = idle;
        None
    }
}

#[cfg(target_arch = "xtensa")]
type FailedOwnerBacklog = (
    heapless::Vec<PowerTicket, POWER_COMMANDS_CAPACITY>,
    heapless::Vec<PowerCommand, POWER_COMMANDS_CAPACITY>,
);

#[cfg(target_arch = "xtensa")]
fn collect_failed_owner_backlog(failed_owner: PowerIntentOwner) -> FailedOwnerBacklog {
    let mut superseded = heapless::Vec::new();
    let mut dispatchable = heapless::Vec::new();
    for _ in 0..POWER_COMMANDS_CAPACITY {
        let Ok(command) = POWER_COMMANDS.try_receive() else {
            break;
        };
        let (ticket, _, _, _, _) = power_command_details(command);
        if queued_command_is_preempted_by_failed_owner(command, failed_owner) {
            if let Err(ticket) = superseded.push(ticket) {
                signal_ticket(ticket, TicketOutcome::Superseded);
            }
        } else if let Err(command) = dispatchable.push(command) {
            let (ticket, _, _, _, _) = power_command_details(command);
            signal_ticket(ticket, TicketOutcome::Rejected);
        }
    }
    (superseded, dispatchable)
}

#[cfg(target_arch = "xtensa")]
fn settle_failed_owner_backlog(
    failed_owner: PowerIntentOwner,
    backlog: FailedOwnerBacklog,
    coordinator: &mut PowerCoordinatorState,
) {
    let (superseded, dispatchable) = backlog;
    for ticket in superseded {
        signal_ticket(ticket, TicketOutcome::Superseded);
    }

    let mut priority_floor = failed_owner;
    for command in dispatchable {
        if queued_command_is_preempted_by_failed_owner(command, priority_floor) {
            let (ticket, _, _, _, _) = power_command_details(command);
            signal_ticket(ticket, TicketOutcome::Superseded);
            continue;
        }
        if let Some(next_floor) = dispatch_power_command(command, coordinator)
            && next_floor.priority() > priority_floor.priority()
        {
            priority_floor = next_floor;
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn retry_deferred_power_command(coordinator: &mut PowerCoordinatorState) {
    let Some(command) = coordinator.deferred.take() else {
        return;
    };
    if let Some(failed_owner) = dispatch_power_command(command, coordinator) {
        let backlog = collect_failed_owner_backlog(failed_owner);
        settle_failed_owner_backlog(failed_owner, backlog, coordinator);
    }
}

#[cfg(target_arch = "xtensa")]
fn dispatch_power_terminal(
    ticket: PowerTicket,
    outcome: TicketOutcome,
    inflight: &mut Option<(PowerIntentOwner, PowerTicket)>,
    inflight_refresh: &mut bool,
    inflight_idle: &mut bool,
    active_owner: &mut Option<PowerIntentOwner>,
    joined_refresh: &mut heapless::Vec<PowerTicket, POWER_COMMANDS_CAPACITY>,
) -> bool {
    if !terminal_matches_inflight(*inflight, ticket) {
        return false;
    }
    let completed_owner = inflight.map(|(owner, _)| owner);
    *active_owner = active_owner_after_terminal(
        *active_owner,
        completed_owner,
        *inflight_refresh,
        *inflight_idle,
        outcome,
    );
    signal_ticket(ticket, outcome);
    *inflight = None;
    *inflight_refresh = false;
    *inflight_idle = false;
    while let Some(joined) = joined_refresh.pop() {
        signal_ticket(joined, outcome);
    }
    true
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn power_coordinator_task() {
    let mut coordinator = PowerCoordinatorState::new();
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
                if let Some(failed_owner) = dispatch_power_command(command, &mut coordinator) {
                    let backlog = collect_failed_owner_backlog(failed_owner);
                    settle_failed_owner_backlog(failed_owner, backlog, &mut coordinator);
                }
            }
            Either3::Second((state, ticket, outcome)) => {
                let failed_owner = failed_contract_owner_for_terminal(
                    coordinator.inflight,
                    ticket,
                    coordinator.inflight_refresh,
                    coordinator.inflight_idle,
                    outcome,
                );
                let failed_owner_backlog = failed_owner.map(collect_failed_owner_backlog);
                let terminal_accepted = dispatch_power_terminal(
                    ticket,
                    outcome,
                    &mut coordinator.inflight,
                    &mut coordinator.inflight_refresh,
                    &mut coordinator.inflight_idle,
                    &mut coordinator.active_owner,
                    &mut coordinator.joined_refresh,
                );
                if terminal_accepted {
                    publish_power_state(state);
                }
                if let (Some(failed_owner), Some(backlog)) = (failed_owner, failed_owner_backlog) {
                    settle_failed_owner_backlog(failed_owner, backlog, &mut coordinator);
                }
                retry_deferred_power_command(&mut coordinator);
            }
            Either3::Third(state) => {
                publish_power_state(state);
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
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let active_contract = capabilities.select_exact_contract(request).unwrap();
        let active =
            ConfirmedActiveContract::from_private_contract(active_contract, capabilities).unwrap();
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

    #[test]
    fn fault_projection_withholds_source_capabilities() {
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let state =
            PowerState::from_observation(SinkPhase::Fault, None, Some(capabilities), true, None);

        assert_eq!(state.failure, Some(PowerFailure::TransportFault));
        assert!(state.source_capabilities.pps.iter().all(Option::is_none));
        assert!(state.source_capabilities.fixed.iter().all(Option::is_none));
    }

    #[test]
    fn power_state_observation_keeps_fusb_status_raw_and_pd_projection() {
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let request = PdContractRequest::pps(17_500, 3_000).unwrap();
        let contract = capabilities.select_exact_contract(request).unwrap();
        let observation = PdStatusObservation {
            status_raw: FUSB302B_STATUS0_VBUSOK,
            status: fusb302b_status_projection(FUSB302B_STATUS0_VBUSOK),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        };
        let state = PowerState::from_observation(
            SinkPhase::Ready,
            Some(observation),
            Some(capabilities),
            true,
            None,
        );

        let projected = state.observation().expect("active contract is observable");
        assert_eq!(projected.status_raw, FUSB302B_STATUS0_VBUSOK);
        assert!(projected.status.pd_active);
    }

    #[test]
    fn invalidated_capabilities_project_as_a_fault_not_ready() {
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let contract = capabilities
            .select_exact_contract(PdContractRequest::pps(17_500, 3_000).unwrap())
            .unwrap();
        let observation = PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        };
        let state = PowerState::from_observation(
            SinkPhase::Ready,
            Some(observation),
            Some(SourceCapabilities::empty()),
            true,
            None,
        );

        assert_eq!(state.protocol, PowerProtocol::Fault);
        assert_eq!(state.active, None);
        assert_eq!(state.failure, Some(PowerFailure::CapabilityInvalidated));
    }

    #[test]
    fn confirmed_active_contract_is_not_republished_as_requested() {
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let contract = capabilities
            .select_exact_contract(PdContractRequest::pps(17_500, 3_000).unwrap())
            .unwrap();
        let observation = PdStatusObservation {
            status_raw: 1 << 3,
            status: Status::from_register(1 << 3),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        };

        let state = PowerState::from_observation(
            SinkPhase::Ready,
            Some(observation),
            Some(capabilities),
            true,
            None,
        );

        assert!(state.active.is_some());
        assert_eq!(state.requested, None);
    }

    #[test]
    fn six_ticket_slots_report_busy_until_a_terminal_is_consumed() {
        let slots = AtomicU8::new(0);
        let reserved = (0..POWER_TICKET_SLOT_COUNT)
            .map(|_| reserve_ticket_slot(&slots).expect("slot available"))
            .collect::<heapless::Vec<_, POWER_TICKET_SLOT_COUNT>>();

        assert_eq!(reserved.len(), 6);
        assert_eq!(reserve_ticket_slot(&slots), None);
        release_ticket_slot(&slots, reserved[2]);
        assert_eq!(reserve_ticket_slot(&slots), Some(reserved[2]));
    }

    #[test]
    fn discarded_ticket_releases_its_slot_when_the_terminal_arrives() {
        let slots = AtomicU8::new(0);
        let discarded = AtomicU8::new(0);
        let slot = reserve_ticket_slot(&slots).expect("slot available");
        discarded.fetch_or(1u8 << slot, Ordering::Release);

        assert!(release_discarded_ticket_slot(&discarded, &slots, slot));
        assert_eq!(reserve_ticket_slot(&slots), Some(slot));
        assert!(!release_discarded_ticket_slot(&discarded, &slots, slot));
    }

    #[test]
    fn ticket_terminal_is_accepted_once_only_for_the_inflight_ticket() {
        let owner = PowerIntentOwner::Calibration;
        let ticket = PowerTicket {
            owner: Some(owner),
            sequence: 1,
            result_slot: 0,
        };
        let other = PowerTicket {
            sequence: 2,
            ..ticket
        };
        let inflight = Some((owner, ticket));

        assert!(terminal_matches_inflight(inflight, ticket));
        assert!(!terminal_matches_inflight(inflight, other));
        assert!(!terminal_matches_inflight(None, ticket));
    }

    #[test]
    fn refresh_joins_and_retains_the_confirmed_intent_owner() {
        let owner = PowerIntentOwner::Calibration;
        assert!(should_join_refresh(true, true));
        assert!(!should_join_refresh(false, true));
        assert!(!should_join_refresh(true, false));
        assert!(!owner_is_superseded(Some(owner), Some(owner)));
        assert!(owner_is_superseded(
            Some(PowerIntentOwner::AutomaticThermal),
            Some(owner),
        ));
        assert_eq!(
            active_owner_after_terminal(
                Some(owner),
                Some(PowerIntentOwner::AutomaticThermal),
                true,
                false,
                TicketOutcome::CapabilitiesRefreshed,
            ),
            Some(owner),
        );
        assert_eq!(
            active_owner_after_terminal(
                Some(owner),
                Some(PowerIntentOwner::AutomaticThermal),
                true,
                false,
                TicketOutcome::Rejected,
            ),
            Some(owner),
        );
    }

    #[test]
    fn failed_owner_terminal_supersedes_queued_same_or_lower_priority_work() {
        let owner = PowerIntentOwner::Calibration;
        let ticket = PowerTicket {
            owner: Some(owner),
            sequence: 10,
            result_slot: 0,
        };
        let low_priority_command = PowerCommand::Request {
            owner: PowerIntentOwner::AutomaticThermal,
            request: PdContractRequest::fixed(5_000, 3_000).unwrap(),
            ticket: PowerTicket {
                owner: Some(PowerIntentOwner::AutomaticThermal),
                sequence: 11,
                result_slot: 1,
            },
        };
        let same_priority_command = PowerCommand::Request {
            owner,
            request: PdContractRequest::fixed(5_000, 3_000).unwrap(),
            ticket: PowerTicket {
                owner: Some(owner),
                sequence: 15,
                result_slot: 5,
            },
        };
        let higher_priority_command = PowerCommand::Request {
            owner: PowerIntentOwner::ThermalPlantAuto,
            request: PdContractRequest::fixed(5_000, 3_000).unwrap(),
            ticket: PowerTicket {
                owner: Some(PowerIntentOwner::ThermalPlantAuto),
                sequence: 12,
                result_slot: 2,
            },
        };
        let refresh = PowerCommand::RefreshCapabilities {
            ticket: PowerTicket {
                owner: None,
                sequence: 13,
                result_slot: 3,
            },
        };
        let idle = PowerCommand::Idle {
            ticket: PowerTicket {
                owner: None,
                sequence: 14,
                result_slot: 4,
            },
        };
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let active_request = PdContractRequest::pps(17_500, 3_000).unwrap();
        let active_contract = capabilities.select_exact_contract(active_request).unwrap();
        let active =
            ConfirmedActiveContract::from_private_contract(active_contract, capabilities).unwrap();

        assert_eq!(
            failed_contract_owner_for_terminal(
                Some((owner, ticket)),
                ticket,
                false,
                false,
                TicketOutcome::Rejected,
            ),
            Some(owner),
        );
        assert!(queued_command_is_preempted_by_failed_owner(
            low_priority_command,
            owner,
        ));
        assert!(queued_command_is_preempted_by_failed_owner(
            same_priority_command,
            owner,
        ));
        assert!(!queued_command_is_preempted_by_failed_owner(
            higher_priority_command,
            owner,
        ));
        assert!(queued_command_is_preempted_by_failed_owner(refresh, owner));
        assert!(!queued_command_is_preempted_by_failed_owner(idle, owner));
        assert_eq!(
            failed_contract_owner_for_terminal(
                Some((owner, ticket)),
                ticket,
                false,
                false,
                TicketOutcome::Confirmed(active),
            ),
            None,
        );
    }

    #[test]
    fn pending_terminal_prevents_dequeueing_a_new_pd_command() {
        assert_eq!(
            select_pd_service_work(Some((1, 10)), Some((2, 20))),
            (None, Some((2, 20))),
        );
        assert_eq!(
            select_pd_service_work(Some((1, 10)), None::<(i32, i32)>),
            (Some((1, 10)), None),
        );
    }

    #[test]
    fn continuous_pps_adjustment_requires_the_confirmed_range_and_current() {
        let capabilities =
            SourceCapabilities::from_pdos(&[pps_source_capability(5_500, 21_000, 3_000)]);
        let active_request = PdContractRequest::pps(17_500, 3_000).unwrap();
        let active_contract = capabilities.select_exact_contract(active_request).unwrap();
        let active =
            ConfirmedActiveContract::from_private_contract(active_contract, capabilities).unwrap();
        let state = PowerState {
            protocol: PowerProtocol::Ready,
            available: true,
            requested: None,
            active: Some(active),
            source_capabilities: capabilities.view(),
            failure: None,
        };

        assert!(pps_adjustment_is_continuous(
            state,
            PdContractRequest::pps(18_000, 3_000).unwrap(),
        ));
        assert!(!pps_adjustment_is_continuous(
            state,
            PdContractRequest::pps(21_100, 3_000).unwrap(),
        ));
        assert!(!pps_adjustment_is_continuous(
            state,
            PdContractRequest::pps(18_000, 3_050).unwrap(),
        ));
        assert!(!pps_adjustment_is_continuous(
            PowerState {
                failure: Some(PowerFailure::TransportFault),
                ..state
            },
            PdContractRequest::pps(18_000, 3_000).unwrap(),
        ));
    }

    #[test]
    fn overlapping_apdo_selection_is_not_mistaken_for_same_contract() {
        let capabilities = SourceCapabilities::from_pdos(&[
            pps_source_capability(5_500, 20_000, 3_000),
            pps_source_capability(17_000, 21_000, 3_000),
        ]);
        let active_request = PdContractRequest::pps(20_500, 3_000).unwrap();
        let active_contract = capabilities.select_exact_contract(active_request).unwrap();
        let active =
            ConfirmedActiveContract::from_private_contract(active_contract, capabilities).unwrap();
        let state = PowerState {
            protocol: PowerProtocol::Ready,
            available: true,
            requested: None,
            active: Some(active),
            source_capabilities: capabilities.view(),
            failure: None,
        };

        assert!(pps_adjustment_is_continuous(
            state,
            PdContractRequest::pps(20_600, 3_000).unwrap(),
        ));
        assert!(!pps_adjustment_is_continuous(
            state,
            PdContractRequest::pps(17_500, 3_000).unwrap(),
        ));
    }

    fn pps_source_capability(min_mv: u16, max_mv: u16, max_ma: u16) -> u32 {
        (0b11 << 30)
            | (u32::from(max_mv / 100) << 17)
            | (u32::from(min_mv / 100) << 8)
            | u32::from(max_ma / 50)
    }
}
