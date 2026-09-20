#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_TICK_MS: u64 = 5;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_MAX_COMMANDS_PER_TICK: usize = 1;

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
    pub(crate) pending_request: Option<PdContractRequest>,
    pub(crate) pending_ticket: Option<PowerTicket>,
    pub(crate) pending_idle: bool,
}

#[cfg(target_arch = "xtensa")]
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
            pending_request: None,
            pending_ticket: None,
            pending_idle: false,
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
        && observation.contract.voltage_mv == FUSB302B_INITIAL_PPS_REQUEST_MV)
        || (observation.contract.kind == ContractKind::Fixed
            && observation.contract.voltage_mv <= FUSB302B_INITIAL_PPS_REQUEST_MV
            && !capabilities.is_some_and(source_supports_fusb302b_idle_pps))
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
        let active = snapshot.observation.and_then(|observation| {
            ConfirmedActiveContract::from_private_contract(
                observation.contract,
                snapshot
                    .capabilities
                    .map(|_| SourceCapabilities::empty())
                    .unwrap_or_else(SourceCapabilities::empty),
            )
        });
        if request_matches_active(request, active) {
            return PdRequestState::Confirmed;
        }
        if snapshot.pending_request == Some(request)
            && let Some(ticket) = snapshot.pending_ticket
        {
            return PdRequestState::Pending(ticket);
        }
        match PowerCoordinatorClient::new().request(PowerIntentOwner::AutomaticThermal, request) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    pub(crate) fn request_fixed_contract(&self, request: PdContractRequest) -> PdRequestState {
        if request.mode != PdContractRequestMode::Fixed {
            return PdRequestState::Failed;
        }
        self.submit_request(request)
    }

    pub(crate) fn restore_automatic_idle_contract(&self) -> PdRequestState {
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdRequestState::Failed;
        }
        if snapshot.observation.is_some_and(|observation| {
            automatic_idle_contract_is_confirmed(observation, snapshot.capabilities)
        }) {
            return PdRequestState::Confirmed;
        }
        if snapshot.pending_idle
            && let Some(ticket) = snapshot.pending_ticket
        {
            return PdRequestState::Pending(ticket);
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
        if request.mode != PdContractRequestMode::Pps {
            return PdRequestState::Failed;
        }
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdRequestState::Failed;
        }
        if snapshot.observation.is_some_and(|observation| {
            observation.contract.kind == ContractKind::Pps
                && observation.contract.voltage_mv == request.voltage_mv
                && observation.contract.current_ma == request.operating_current_ma
        }) {
            return PdRequestState::Confirmed;
        }
        if snapshot.pending_request == Some(request)
            && let Some(ticket) = snapshot.pending_ticket
        {
            return PdRequestState::Pending(ticket);
        }
        match PowerCoordinatorClient::new().request(owner, request) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn refresh_capabilities(&self) -> Result<PowerTicket, ()> {
        PowerCoordinatorClient::new().refresh_capabilities()
    }

    pub(crate) fn subscribe_power_state(&self) -> Option<PowerStateSubscription<'static>> {
        PowerCoordinatorClient::new().subscribe()
    }

    #[allow(dead_code)]
    pub(crate) async fn wait_for_ticket(&self, ticket: PowerTicket) -> TicketOutcome {
        PowerCoordinatorClient::new().wait(ticket).await
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_status_confirms_ready_contract(
    phase: SinkPhase,
    contract: Contract,
    status0: u8,
) -> bool {
    phase == SinkPhase::Ready
        && contract != Contract::none()
        && status0 & FUSB302B_STATUS0_VBUSOK != 0
}

#[cfg(target_arch = "xtensa")]
async fn pd_status_observation(
    runtime: &Fusb302bRuntime,
    i2c: &mut PdI2c<'_>,
) -> Option<PdStatusObservation> {
    let contract = runtime.active_contract();
    let status = Fusb302::new(&mut *i2c).read_status().await.ok()?;
    if !fusb302b_status_confirms_ready_contract(runtime.policy.phase(), contract, status.status0) {
        return None;
    }

    let status_raw = 1 << 3;
    Some(PdStatusObservation {
        status_raw,
        status: Status::from_register(status_raw),
        current_raw: 0,
        current_ma: contract.current_ma,
        contract_voltage_mv: Some(contract.voltage_mv),
        contract,
    })
}

#[cfg(target_arch = "xtensa")]
fn publish_pd_snapshot(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
) {
    let now_ms = PdTimestamp::now().as_millis();
    let interlock_latched = PD_INTERLOCK_LATCHED.load(Ordering::Acquire) != 0;
    let observation = (!interlock_latched).then_some(observation).flatten();
    let ready = startup_pd_contract_ready(observation);
    let (pending_request, pending_idle, pending_ticket) = match pending {
        Some((ticket, PendingPdOperation::Contract { request })) => {
            (Some(request), false, Some(ticket))
        }
        Some((ticket, PendingPdOperation::Idle { .. })) => (None, true, Some(ticket)),
        Some((ticket, PendingPdOperation::Refresh)) => (None, false, Some(ticket)),
        None => (None, false, None),
    };
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
            pending_request,
            pending_ticket,
            pending_idle,
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
) -> PdCommandProgress {
    match command {
        PdServiceCommand::AutomaticIdle { ticket } => {
            let previous = runtime.confirmed_active_contract();
            match runtime
                .request_automatic_idle_contract(i2c, PdTimestamp::now())
                .await
            {
                PdContractRequestState::Confirmed => PdCommandProgress::Done(
                    ticket,
                    TicketOutcome::Confirmed(
                        runtime
                            .confirmed_active_contract()
                            .or(previous)
                            .expect("confirmed idle contract must be present"),
                    ),
                ),
                PdContractRequestState::Pending => {
                    PdCommandProgress::Pending(ticket, PendingPdOperation::Idle { previous })
                }
                PdContractRequestState::Failed => {
                    PdCommandProgress::Done(ticket, TicketOutcome::Rejected)
                }
            }
        }
        PdServiceCommand::Contract { request, ticket } => {
            let previous = runtime.confirmed_active_contract();
            match runtime
                .request_contract(i2c, request, PdTimestamp::now())
                .await
            {
                PdContractRequestState::Confirmed => PdCommandProgress::Done(
                    ticket,
                    TicketOutcome::Confirmed(
                        runtime
                            .confirmed_active_contract()
                            .or(previous)
                            .expect("confirmed contract must be present"),
                    ),
                ),
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
                .refresh_source_capabilities(i2c, PdTimestamp::now())
                .await
            {
                PdContractRequestState::Confirmed => {
                    PdCommandProgress::Done(ticket, TicketOutcome::CapabilitiesRefreshed)
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

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
enum PendingPdOperation {
    Contract {
        request: PdContractRequest,
    },
    Idle {
        previous: Option<ConfirmedActiveContract>,
    },
    Refresh,
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
enum PdCommandProgress {
    Pending(PowerTicket, PendingPdOperation),
    Done(PowerTicket, TicketOutcome),
}

#[cfg(target_arch = "xtensa")]
fn pending_terminal(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: PendingPdOperation,
) -> Option<TicketOutcome> {
    let active = observation.and_then(|observation| {
        ConfirmedActiveContract::from_private_contract(
            observation.contract,
            runtime
                .source_capabilities()
                .unwrap_or_else(SourceCapabilities::empty),
        )
    });
    if runtime.policy.phase() == SinkPhase::Fault {
        return Some(TicketOutcome::TransportFault);
    }
    if FUSB302B_DIAGNOSTIC.load(Ordering::Acquire) == FUSB302B_DIAG_REQUEST_TIMEOUT {
        return Some(TicketOutcome::TimedOut);
    }
    match pending {
        PendingPdOperation::Contract { request, .. } if request_matches_active(request, active) => {
            active.map(TicketOutcome::Confirmed)
        }
        PendingPdOperation::Contract { request, .. }
            if runtime.source_capabilities().is_some_and(|capabilities| {
                request.mode == PdContractRequestMode::Pps
                    && capabilities.select_exact_contract(request).is_none()
            }) =>
        {
            Some(TicketOutcome::Rejected)
        }
        PendingPdOperation::Idle { previous } if active.is_some() && active != previous => {
            active.map(TicketOutcome::Confirmed)
        }
        PendingPdOperation::Refresh if !runtime.source_capabilities_refresh_pending => {
            Some(TicketOutcome::CapabilitiesRefreshed)
        }
        PendingPdOperation::Contract { .. } | PendingPdOperation::Idle { .. }
            if active.is_none()
                && !matches!(
                    runtime.policy.phase(),
                    SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
                ) =>
        {
            Some(TicketOutcome::Detached)
        }
        _ => None,
    }
}

#[cfg(target_arch = "xtensa")]
fn publish_power_report(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
    terminal_override: Option<(PowerTicket, TicketOutcome)>,
) -> (PowerState, Option<(PowerTicket, TicketOutcome)>) {
    let requested = match pending.map(|(_, operation)| operation) {
        Some(PendingPdOperation::Contract { request, .. }) => Some(request),
        _ => None,
    };
    let state = PowerState::from_observation(
        runtime.policy.phase(),
        observation,
        runtime.source_capabilities(),
        runtime.service_available(),
        requested,
    );
    let terminal = terminal_override.or_else(|| {
        pending.and_then(|(ticket, operation)| {
            pending_terminal(runtime, observation, operation).map(|outcome| (ticket, outcome))
        })
    });
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
        request.mode == PdContractRequestMode::Pps
            && capabilities.select_exact_contract(request).is_none()
    }) {
        return Some((ticket, TicketOutcome::Rejected));
    }
    match runtime
        .request_contract(i2c, request, PdTimestamp::now())
        .await
    {
        PdContractRequestState::Failed => Some((ticket, TicketOutcome::Rejected)),
        PdContractRequestState::Confirmed | PdContractRequestState::Pending => None,
    }
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
            if PD_INTERLOCK_PENDING.swap(0, Ordering::Acquire) != 0 {
                runtime.interlock_after_stale_contract(PdTimestamp::now().as_millis());
                PD_INTERLOCK_LATCHED.store(0, Ordering::Release);
                HeaterPwmGate::force_off();
            }
            let mut terminal_override = None;
            if let Some(completion) =
                retry_deferred_contract_request(&mut runtime, &mut i2c, pending).await
            {
                terminal_override = Some(completion);
            }
            for _ in 0..PD_SERVICE_MAX_COMMANDS_PER_TICK {
                let Ok(command) = PD_SERVICE_COMMANDS.try_receive() else {
                    break;
                };
                match process_pd_command(&mut runtime, &mut i2c, command).await {
                    PdCommandProgress::Pending(ticket, operation) => {
                        pending = Some((ticket, operation));
                    }
                    PdCommandProgress::Done(ticket, outcome) => {
                        terminal_override = Some((ticket, outcome));
                    }
                }
            }
            // A request flood must never turn the command mailbox into a second
            // unbounded work queue. Poll once on every service turn regardless of
            // how many commands are waiting.
            let _ = runtime.poll(&mut i2c, PdTimestamp::now()).await;
            // The policy contract is only heater-authorizing after a fresh
            // hardware status read. Keep this read inside the same bus lease so
            // an EEPROM turn cannot leave a stale contract looking active.
            let observation = pd_status_observation(&runtime, &mut i2c).await;
            i2c.release();
            let (state, terminal) =
                publish_power_report(&runtime, observation, pending, terminal_override);
            publish_pd_snapshot(
                &runtime,
                observation,
                terminal.is_none().then_some(pending).flatten(),
            );
            publish_pd_service_state(state);
            // A heartbeat represents a complete bus-acquired poll-and-publish
            // turn. A skipped try-lock turn must not look like PD progress.
            record_pd_heartbeat();
            if let Some((ticket, outcome)) = terminal {
                publish_pd_service_terminal(state, ticket, outcome).await;
                pending = None;
            }
        } else {
            // Clear authorization immediately, but do not count this skipped
            // turn as PD progress for the watchdog.
            let (state, terminal) = publish_power_report(&runtime, None, pending, None);
            publish_pd_snapshot(
                &runtime,
                None,
                terminal.is_none().then_some(pending).flatten(),
            );
            publish_pd_service_state(state);
            if let Some((ticket, outcome)) = terminal {
                publish_pd_service_terminal(state, ticket, outcome).await;
                pending = None;
            }
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
