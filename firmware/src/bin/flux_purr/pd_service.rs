#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_TICK_MS: u64 = 5;

#[cfg(target_arch = "xtensa")]
static PD_INTERLOCK_PENDING: AtomicU8 = AtomicU8::new(0);

#[cfg(target_arch = "xtensa")]
static PD_INTERLOCK_LATCHED: AtomicU8 = AtomicU8::new(0);

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PdServiceSnapshot {
    pub(crate) observation: Option<PdStatusObservation>,
    pub(crate) capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    pub(crate) controller: ControllerKind,
    pub(crate) service_available: bool,
    pub(crate) stale_contract_vin_guard_suspended: bool,
    pub(crate) published_at_ms: u64,
}

#[cfg(target_arch = "xtensa")]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdRequestState {
    Confirmed,
    Pending(PowerTicket),
    Failed,
}

#[cfg(target_arch = "xtensa")]
impl PdServiceSnapshot {
    const fn unavailable() -> Self {
        Self {
            observation: None,
            capabilities: None,
            controller: ControllerKind::Unknown,
            service_available: false,
            stale_contract_vin_guard_suspended: false,
            published_at_ms: 0,
        }
    }

    fn with_fresh_observation(self, now_ms: u64) -> Self {
        if self
            .observation
            .is_some_and(|_| !pd_snapshot_is_fresh(self.published_at_ms, now_ms))
        {
            Self {
                observation: None,
                ..self
            }
        } else {
            self
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_SNAPSHOT: BlockingMutex<
    CriticalSectionRawMutex,
    RefCell<PdServiceSnapshot>,
> = BlockingMutex::new(RefCell::new(PdServiceSnapshot::unavailable()));

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn source_supports_fusb302b_idle_pps(
    capabilities: ch224q::AdjustablePowerCapabilities,
) -> bool {
    capabilities.pps_apdos.into_iter().flatten().any(|apdo| {
        apdo.min_mv <= FUSB302B_INITIAL_PPS_REQUEST_MV
            && apdo.max_mv >= FUSB302B_INITIAL_PPS_REQUEST_MV
            && apdo.max_ma >= MIN_HEATER_CONTRACT_MA
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_idle_contract_is_confirmed(
    observation: PdStatusObservation,
    capabilities: Option<ch224q::AdjustablePowerCapabilities>,
) -> bool {
    (observation.contract.kind == ContractKind::Pps
        && observation.contract.voltage_mv == FUSB302B_INITIAL_PPS_REQUEST_MV
        && observation.contract.current_ma >= MIN_HEATER_CONTRACT_MA)
        || (observation.contract.kind == ContractKind::Fixed
            && observation.contract.voltage_mv <= FUSB302B_INITIAL_PPS_REQUEST_MV
            && observation.contract.current_ma >= MIN_HEATER_CONTRACT_MA
            && !capabilities.is_some_and(source_supports_fusb302b_idle_pps))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn refresh_terminal_outcome(
    phase: SinkPhase,
    refresh_pending: bool,
    refresh_timed_out: bool,
    has_source_capabilities: bool,
) -> Option<TicketOutcome> {
    match phase {
        SinkPhase::Detached => Some(TicketOutcome::Detached),
        SinkPhase::Fault => Some(TicketOutcome::TransportFault),
        _ if refresh_timed_out => Some(TicketOutcome::TimedOut),
        _ if !refresh_pending
            && has_source_capabilities
            && !matches!(
                phase,
                SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
            ) =>
        {
            Some(TicketOutcome::CapabilitiesRefreshed)
        }
        _ => None,
    }
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Default)]
pub(crate) struct PdServiceClient;

#[cfg(target_arch = "xtensa")]
impl PdServiceClient {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) fn mark_starting_fusb302b() -> Self {
        PD_INTERLOCK_PENDING.store(0, Ordering::Release);
        PD_INTERLOCK_LATCHED.store(0, Ordering::Release);
        PD_SERVICE_REQUIRED.store(1, Ordering::Release);
        PD_SERVICE_SNAPSHOT.lock(|snapshot| {
            *snapshot.borrow_mut() = PdServiceSnapshot {
                controller: ControllerKind::Fusb302b,
                service_available: true,
                ..PdServiceSnapshot::unavailable()
            };
        });
        PD_HEATER_PERMIT.store(0, Ordering::Release);
        Self::new()
    }

    pub(crate) fn mark_unavailable() -> Self {
        PD_INTERLOCK_PENDING.store(0, Ordering::Release);
        PD_INTERLOCK_LATCHED.store(0, Ordering::Release);
        PD_SERVICE_REQUIRED.store(0, Ordering::Release);
        PD_SERVICE_SNAPSHOT.lock(|snapshot| {
            *snapshot.borrow_mut() = PdServiceSnapshot::unavailable();
        });
        HeaterPwmGate::force_off();
        publish_pd_service_state(PowerState::unavailable());
        Self::new()
    }

    pub(crate) fn snapshot(&self) -> PdServiceSnapshot {
        PD_SERVICE_SNAPSHOT
            .lock(|snapshot| *snapshot.borrow())
            .with_fresh_observation(PdTimestamp::now().as_millis())
    }

    pub(crate) fn capabilities(&self) -> Option<ch224q::AdjustablePowerCapabilities> {
        self.snapshot().capabilities
    }

    pub(crate) fn controller_kind(&self) -> ControllerKind {
        self.snapshot().controller
    }

    pub(crate) fn stale_contract_vin_guard_suspended(&self, _now_ms: u64) -> bool {
        self.snapshot().stale_contract_vin_guard_suspended
    }

    pub(crate) fn interlock_after_stale_contract(&self, _now_ms: u64) {
        let snapshot = self.snapshot();
        if snapshot.service_available {
            PD_INTERLOCK_LATCHED.store(1, Ordering::Release);
            PD_INTERLOCK_PENDING.store(1, Ordering::Release);
        }
        PD_SERVICE_SNAPSHOT.lock(|current| {
            let mut current = current.borrow_mut();
            current.observation = None;
            current.stale_contract_vin_guard_suspended = false;
        });
        HeaterPwmGate::force_off();
    }

    fn submit_request(&self, request: PdContractRequest) -> PdRequestState {
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdRequestState::Failed;
        }
        match PowerCoordinatorClient::new().request(PowerIntentOwner::AutomaticThermal, request) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    pub(crate) fn request_fixed_contract(&self, request: PdContractRequest) -> PdRequestState {
        if request.mode() != PdContractRequestMode::Fixed {
            return PdRequestState::Failed;
        }
        self.submit_request(request)
    }

    pub(crate) fn restore_automatic_idle_contract(&self) -> PdRequestState {
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdRequestState::Failed;
        }
        match PowerCoordinatorClient::new().idle() {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    pub(crate) fn request_pps_contract_for(
        &self,
        owner: PowerIntentOwner,
        request: PdContractRequest,
    ) -> PdRequestState {
        if request.mode() != PdContractRequestMode::Pps {
            return PdRequestState::Failed;
        }
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdRequestState::Failed;
        }
        match PowerCoordinatorClient::new().request(owner, request) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn refresh_capabilities(&self) -> Result<PowerTicket, PowerAdmissionError> {
        PowerCoordinatorClient::new().refresh_capabilities()
    }

    pub(crate) fn pps_request_requires_heater_pause(&self, request: PdContractRequest) -> bool {
        !pps_adjustment_is_continuous(PowerCoordinatorClient::new().latest(), request)
    }

    pub(crate) fn subscribe_power_state(&self) -> Option<PowerStateSubscription<'static>> {
        PowerCoordinatorClient::new().subscribe()
    }

    #[allow(dead_code)]
    pub(crate) async fn wait_for_ticket(&self, ticket: PowerTicket) -> TicketOutcome {
        PowerCoordinatorClient::new().wait(ticket).await
    }

    pub(crate) fn try_take_ticket(&self, ticket: PowerTicket) -> Option<TicketOutcome> {
        PowerCoordinatorClient::new().try_take(ticket)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_status_confirms_active_contract(
    phase: SinkPhase,
    contract: Contract,
    status0: u8,
) -> bool {
    phase == SinkPhase::Ready
        && contract != Contract::none()
        && status0 & FUSB302B_STATUS0_VBUSOK != 0
}

#[cfg(target_arch = "xtensa")]
fn pd_status_observation(runtime: &Fusb302bRuntime) -> Option<PdStatusObservation> {
    if !runtime.service_available() || runtime.request_timed_out || !runtime.vbus_status_observed()
    {
        return None;
    }
    let contract = runtime.active_contract();
    if !fusb302b_status_confirms_active_contract(
        runtime.policy.phase(),
        contract,
        runtime.vbus_status_raw(),
    ) {
        return None;
    }

    let status_raw = runtime.vbus_status_raw();
    Some(PdStatusObservation {
        status_raw,
        status: fusb302b_status_projection(status_raw),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    })
}

#[cfg(target_arch = "xtensa")]
fn publish_pd_snapshot(runtime: &Fusb302bRuntime, observation: Option<PdStatusObservation>) {
    let now_ms = PdTimestamp::now().as_millis();
    let interlock_latched = PD_INTERLOCK_LATCHED.load(Ordering::Acquire) != 0;
    let observation = (!interlock_latched).then_some(observation).flatten();
    let ready = startup_pd_contract_ready(observation);
    PD_SERVICE_SNAPSHOT.lock(|snapshot| {
        *snapshot.borrow_mut() = PdServiceSnapshot {
            observation,
            capabilities: runtime
                .source_capabilities()
                .and_then(fusb302b_adjustable_power_capabilities),
            controller: ControllerKind::Fusb302b,
            service_available: runtime.service_available(),
            stale_contract_vin_guard_suspended: runtime.stale_contract_vin_guard_suspended(now_ms),
            published_at_ms: now_ms,
        };
    });
    if ready {
        PD_HEATER_PERMIT_EXPIRES_AT_MS.store(
            now_ms.saturating_add(PD_SNAPSHOT_MAX_AGE_MS) as u32,
            Ordering::Release,
        );
        PD_HEATER_PERMIT.store(1, Ordering::Release);
    } else {
        HeaterPwmGate::force_off();
    }
}

#[cfg(target_arch = "xtensa")]
async fn process_pd_command(
    runtime: &mut Fusb302bRuntime,
    i2c: &mut PdI2c<'static>,
    command: PdServiceCommand,
    replacing_pending: bool,
) -> PdCommandProgress {
    match command {
        PdServiceCommand::AutomaticIdle { ticket } => {
            match runtime
                .request_automatic_idle_contract(i2c, PdTimestamp::now(), replacing_pending)
                .await
            {
                PdContractRequestState::Confirmed => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Idle)
                }
                PdContractRequestState::Pending => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Idle)
                }
                PdContractRequestState::Failed => {
                    PdCommandProgress::Done(ticket, TicketOutcome::Rejected)
                }
            }
        }
        PdServiceCommand::Contract { request, ticket } => {
            match runtime
                .request_contract(i2c, request, PdTimestamp::now(), replacing_pending)
                .await
            {
                PdContractRequestState::Confirmed => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Contract { request })
                }
                PdContractRequestState::Pending => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Contract { request })
                }
                PdContractRequestState::Failed => {
                    PdCommandProgress::Done(ticket, TicketOutcome::Rejected)
                }
            }
        }
        PdServiceCommand::RefreshCapabilities { ticket } => {
            match runtime
                .refresh_source_capabilities(i2c, PdTimestamp::now(), replacing_pending, false)
                .await
            {
                PdContractRequestState::Confirmed => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Refresh)
                }
                PdContractRequestState::Pending => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Refresh)
                }
                PdContractRequestState::Failed => {
                    PdCommandProgress::Done(ticket, TicketOutcome::Rejected)
                }
            }
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
pub(crate) enum PendingPdOperation {
    Contract { request: PdContractRequest },
    Idle,
    Refresh,
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy)]
pub(crate) struct PdServiceTerminalContext {
    pub(crate) phase: SinkPhase,
    pub(crate) request_timed_out: bool,
    pub(crate) request_rejected: bool,
    pub(crate) diagnostic_request_timed_out: bool,
    pub(crate) refresh_pending: bool,
    pub(crate) observation: Option<PdStatusObservation>,
    pub(crate) source_capabilities: Option<SourceCapabilities>,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
enum PdCommandProgress {
    Pending(PowerTicket, PendingPdOperation),
    Done(PowerTicket, TicketOutcome),
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pending_terminal_outcome(
    context: PdServiceTerminalContext,
    pending: PendingPdOperation,
) -> Option<TicketOutcome> {
    let active = context.observation.and_then(|observation| {
        ConfirmedActiveContract::from_private_contract(
            observation.contract,
            context
                .source_capabilities
                .unwrap_or_else(SourceCapabilities::empty),
        )
    });
    if context.request_timed_out {
        return Some(TicketOutcome::TimedOut);
    }
    if context.request_rejected {
        return Some(TicketOutcome::Rejected);
    }
    if context.phase == SinkPhase::Fault {
        return Some(TicketOutcome::TransportFault);
    }
    if context.diagnostic_request_timed_out {
        return Some(TicketOutcome::TimedOut);
    }
    match pending {
        PendingPdOperation::Contract { request }
            if context.phase == SinkPhase::Ready && request_matches_active(request, active) =>
        {
            active.map(TicketOutcome::Confirmed)
        }
        PendingPdOperation::Contract { request }
            if context.source_capabilities.is_some_and(|capabilities| {
                request.mode() == PdContractRequestMode::Pps
                    && capabilities.select_exact_contract(request).is_none()
            }) =>
        {
            Some(TicketOutcome::Rejected)
        }
        PendingPdOperation::Idle
            if context.phase == SinkPhase::Ready
                && context.observation.is_some_and(|observation| {
                    automatic_idle_contract_is_confirmed(
                        observation,
                        context
                            .source_capabilities
                            .and_then(fusb302b_adjustable_power_capabilities),
                    )
                }) =>
        {
            active.map(TicketOutcome::Confirmed)
        }
        PendingPdOperation::Refresh => refresh_terminal_outcome(
            context.phase,
            context.refresh_pending,
            context.request_timed_out,
            context.source_capabilities.is_some(),
        ),
        PendingPdOperation::Contract { .. } | PendingPdOperation::Idle
            if active.is_none()
                && !matches!(
                    context.phase,
                    SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
                ) =>
        {
            Some(TicketOutcome::Detached)
        }
        _ => None,
    }
}

#[cfg(target_arch = "xtensa")]
fn pending_terminal(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: PendingPdOperation,
) -> Option<TicketOutcome> {
    pending_terminal_outcome(
        PdServiceTerminalContext {
            phase: runtime.policy.phase(),
            request_timed_out: runtime.request_timed_out,
            request_rejected: runtime.request_rejected,
            diagnostic_request_timed_out: FUSB302B_DIAGNOSTIC.load(Ordering::Acquire)
                == FUSB302B_DIAG_REQUEST_TIMEOUT,
            refresh_pending: runtime.source_capabilities_refresh_pending,
            observation,
            source_capabilities: runtime.source_capabilities(),
        },
        pending,
    )
}

#[cfg(target_arch = "xtensa")]
fn publish_power_report(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
    terminal_override: Option<(PowerTicket, TicketOutcome)>,
    settle_pending: bool,
) -> (PowerState, Option<(PowerTicket, TicketOutcome)>) {
    let terminal = if terminal_override.is_some() {
        terminal_override
    } else if settle_pending {
        pending.and_then(|(ticket, operation)| {
            pending_terminal(runtime, observation, operation).map(|outcome| (ticket, outcome))
        })
    } else {
        None
    };
    let requested = if terminal.is_some() {
        None
    } else {
        match pending.map(|(_, operation)| operation) {
            Some(PendingPdOperation::Contract { request, .. }) => Some(request),
            _ => None,
        }
    };
    let state = PowerState::from_observation(
        runtime.policy.phase(),
        observation,
        runtime.source_capabilities(),
        runtime.service_available(),
        requested,
    );
    (state, terminal)
}

#[cfg(target_arch = "xtensa")]
async fn retry_deferred_contract_request(
    runtime: &mut Fusb302bRuntime,
    i2c: &mut PdI2c<'static>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
) -> Option<(PowerTicket, TicketOutcome)> {
    let Some((ticket, PendingPdOperation::Contract { request })) = pending else {
        return None;
    };
    if runtime.policy.phase() != SinkPhase::Ready
        || runtime.source_capabilities_refresh_pending
        || runtime
            .confirmed_active_contract()
            .is_some_and(|active| request_matches_active(request, Some(active)))
    {
        return None;
    }
    if runtime.source_capabilities().is_some_and(|capabilities| {
        request.mode() == PdContractRequestMode::Pps
            && capabilities.select_exact_contract(request).is_none()
    }) {
        return Some((ticket, TicketOutcome::Rejected));
    }
    match runtime
        .request_contract(i2c, request, PdTimestamp::now(), false)
        .await
    {
        PdContractRequestState::Failed => Some((ticket, TicketOutcome::Rejected)),
        PdContractRequestState::Confirmed | PdContractRequestState::Pending => None,
    }
}

#[cfg(target_arch = "xtensa")]
async fn process_pd_service_work(
    runtime: &mut Fusb302bRuntime,
    i2c: &mut PdI2c<'static>,
    pending: &mut Option<(PowerTicket, PendingPdOperation)>,
) -> Option<(PowerTicket, TicketOutcome)> {
    let replacing_pending = pending.is_some();
    let (pending_to_retry, command) =
        select_pd_service_work(*pending, PD_SERVICE_COMMANDS.try_receive().ok());
    match command {
        Some(command) => {
            if take_pd_service_command_cancelled(command) {
                let ticket = pd_service_command_ticket(command);
                if pending.is_some() {
                    runtime
                        .abort_pending_operation(i2c, PdTimestamp::now())
                        .await;
                    *pending = None;
                }
                return Some((ticket, TicketOutcome::Superseded));
            }
            *pending = None;
            match process_pd_command(runtime, i2c, command, replacing_pending).await {
                PdCommandProgress::Pending(ticket, operation) => {
                    *pending = Some((ticket, operation));
                    None
                }
                PdCommandProgress::Done(ticket, outcome) => Some((ticket, outcome)),
            }
        }
        None => {
            if let Some(completion) =
                retry_deferred_contract_request(runtime, i2c, pending_to_retry).await
            {
                *pending = None;
                Some(completion)
            } else {
                None
            }
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn consume_pd_interlock(runtime: &mut Fusb302bRuntime) {
    if PD_INTERLOCK_PENDING.swap(0, Ordering::Acquire) != 0 {
        runtime.interlock_after_stale_contract(PdTimestamp::now().as_millis());
        PD_INTERLOCK_LATCHED.store(0, Ordering::Release);
        HeaterPwmGate::force_off();
    }
}

#[cfg(target_arch = "xtensa")]
async fn publish_pd_service_turn(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
    terminal_override: Option<(PowerTicket, TicketOutcome)>,
    settle_pending: bool,
    record_heartbeat: bool,
) -> Option<(PowerTicket, PendingPdOperation)> {
    let (state, terminal) = publish_power_report(
        runtime,
        observation,
        pending,
        terminal_override,
        settle_pending,
    );
    publish_pd_snapshot(runtime, observation);
    publish_pd_service_state(state);
    if record_heartbeat {
        record_pd_heartbeat();
    }
    if let Some((ticket, outcome)) = terminal {
        publish_pd_service_terminal(state, ticket, outcome).await;
        None
    } else {
        pending
    }
}

#[cfg(target_arch = "xtensa")]
async fn run_pd_service_turn(
    i2c: &mut PdI2c<'static>,
    runtime: &mut Fusb302bRuntime,
    pending: &mut Option<(PowerTicket, PendingPdOperation)>,
) {
    consume_pd_interlock(runtime);
    let terminal_override = process_pd_service_work(runtime, i2c, pending).await;
    // A request flood must never turn the command mailbox into a second
    // unbounded work queue. Poll once on every service turn regardless of
    // how many commands are waiting.
    let _ = runtime.poll(i2c, PdTimestamp::now()).await;
    // `runtime.poll` has just sampled the FUSB302B status bank. Reuse only
    // that same-turn VBUSOK result so authorization remains fresh without
    // duplicating the four-register I2C status read.
    let observation = pd_status_observation(runtime);
    i2c.release();
    *pending = publish_pd_service_turn(
        runtime,
        observation,
        *pending,
        terminal_override,
        true,
        true,
    )
    .await;
}

#[cfg(target_arch = "xtensa")]
async fn publish_busy_pd_service_turn(
    runtime: &Fusb302bRuntime,
    pending: &mut Option<(PowerTicket, PendingPdOperation)>,
) {
    *pending = publish_pd_service_turn(runtime, None, *pending, None, false, false).await;
}

/// The sole owner of FUSB302B policy state and physical PD I2C transactions.
/// Every loop turn does bounded work, releases the shared bus, and then yields
/// to its own cadence timer. This task deliberately stays out of interrupt
/// context because FUSB302B protocol recovery has a deep call stack.
#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn pd_service_task(mut i2c: PdI2c<'static>, mut runtime: Box<Fusb302bRuntime>) {
    let mut pending: Option<(PowerTicket, PendingPdOperation)> = None;
    loop {
        if i2c.try_acquire() {
            run_pd_service_turn(&mut i2c, &mut runtime, &mut pending).await;
        } else {
            // Clear authorization immediately, but do not count this skipped
            // turn as PD progress for the watchdog.
            publish_busy_pd_service_turn(&runtime, &mut pending).await;
        }
        EmbassyTimer::after_millis(PD_SERVICE_TICK_MS).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn spawn_pd_service(
    spawner: Spawner,
    mut i2c: PdI2c<'static>,
    runtime: Box<Fusb302bRuntime>,
) {
    i2c.release();
    spawn_power_coordinator(spawner);
    spawner
        .spawn(pd_service_task(i2c, runtime))
        .expect("failed to spawn PD service task");
}
