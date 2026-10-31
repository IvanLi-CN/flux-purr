#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PD_SERVICE_TICK_MS: u64 = 5;

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const PD_SERVICE_BUSY_RETRY_MS: u64 = 1;

#[cfg(target_arch = "xtensa")]
pub(crate) static mut PD_REALTIME_EXECUTOR_STORAGE: MaybeUninit<InterruptExecutor<2>> =
    MaybeUninit::uninit();

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pd_service_delay_ms(elapsed_ms: u64, acquired_bus: bool) -> u64 {
    if acquired_bus {
        PD_SERVICE_TICK_MS.saturating_sub(elapsed_ms).max(1)
    } else {
        PD_SERVICE_BUSY_RETRY_MS
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn continuous_pps_transition_keeps_heater_permit(
    owner_allows_continuity: bool,
    pps_request: bool,
    same_apdo_and_current: bool,
    source_capabilities_stable: bool,
    request_healthy: bool,
) -> bool {
    owner_allows_continuity
        && pps_request
        && same_apdo_and_current
        && source_capabilities_stable
        && request_healthy
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const fn previous_pps_transition_keeps_heater_permit(
    permit_active: bool,
    observation_missing: bool,
    previous_contract_present: bool,
    negotiation_phase: bool,
    capabilities_available: bool,
    capabilities_stable: bool,
    request_healthy: bool,
) -> bool {
    permit_active
        && observation_missing
        && previous_contract_present
        && negotiation_phase
        && capabilities_available
        && capabilities_stable
        && request_healthy
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) struct PdServiceSnapshot {
    pub(crate) observation: Option<PdStatusObservation>,
    pub(crate) active: Option<ConfirmedActiveContract>,
    pub(crate) source: SourceCapabilitiesView,
    pub(crate) transition_pending: bool,
    pub(crate) capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    pub(crate) controller: ControllerKind,
    pub(crate) service_available: bool,
    pub(crate) published_at_ms: u64,
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PdRequestState {
    Confirmed,
    Pending(PowerTicket),
    Failed,
}

#[cfg(any(target_arch = "xtensa", test))]
impl PdServiceSnapshot {
    pub(crate) const fn unavailable() -> Self {
        Self {
            observation: None,
            active: None,
            source: SourceCapabilitiesView {
                fixed: [None; MAX_SOURCE_PDOS],
                pps: [None; MAX_SOURCE_PDOS],
            },
            transition_pending: false,
            capabilities: None,
            controller: ControllerKind::Unknown,
            service_available: false,
            published_at_ms: 0,
        }
    }

    pub(crate) fn with_fresh_observation(self, now_ms: u64) -> Self {
        if self
            .observation
            .is_some_and(|_| !pd_snapshot_is_fresh(self.published_at_ms, now_ms))
        {
            Self {
                observation: None,
                active: None,
                ..self
            }
        } else {
            self
        }
    }

    pub(crate) fn settled_observation(self) -> Option<PdStatusObservation> {
        if self.transition_pending {
            None
        } else {
            self.observation
        }
    }

    /// The last confirmed contract remains observable during negotiation.
    /// Callers must validate it against the latest load/source envelope before
    /// permitting conduction; this never confirms the pending target.
    pub(crate) fn conducting_observation(self) -> Option<PdStatusObservation> {
        self.observation
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_SNAPSHOT: BlockingMutex<
    CriticalSectionRawMutex,
    RefCell<PdServiceSnapshot>,
> = BlockingMutex::new(RefCell::new(PdServiceSnapshot::unavailable()));

#[cfg(test)]
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
    _capabilities: Option<ch224q::AdjustablePowerCapabilities>,
) -> bool {
    observation.contract.kind == ContractKind::Fixed
        && observation.contract.voltage_mv >= FUSB302B_PD_ABSOLUTE_MIN_MV
        && observation.contract.voltage_mv <= FUSB302B_FIXED_MAX_MV
        && observation.contract.current_ma
            >= standby_current_for_voltage(observation.contract.voltage_mv)
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
        match PowerCoordinatorClient::new().idle_at_minimum(FUSB302B_PD_ABSOLUTE_MIN_MV) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    pub(crate) fn request_cooling_contract(&self) -> PdRequestState {
        match PowerCoordinatorClient::new().idle_at_minimum(FAN_WORKING_CONTRACT_MV) {
            Ok(ticket) => PdRequestState::Pending(ticket),
            Err(_) => PdRequestState::Failed,
        }
    }

    pub(crate) fn request_pps_contract_for(
        &self,
        owner: PowerIntentOwner,
        request: PdContractRequest,
    ) -> PdRequestState {
        if request.mode() != PdContractRequestMode::Pps
            || (request.is_calibration()
                && !matches!(
                    owner,
                    PowerIntentOwner::Calibration | PowerIntentOwner::ThermalPlantAuto
                ))
        {
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

    pub(crate) fn discard_ticket(&self, ticket: PowerTicket) {
        PowerCoordinatorClient::new().discard(ticket);
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_status_confirms_active_contract(
    phase: SinkPhase,
    contract: Contract,
    status0: u8,
) -> bool {
    matches!(
        phase,
        SinkPhase::Ready | SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
    ) && contract != Contract::none()
        && status0 & FUSB302B_STATUS0_VBUSOK != 0
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn fusb302b_contract_observation(
    phase: SinkPhase,
    contract: Contract,
    status_raw: u8,
    source: SourceCapabilities,
) -> Option<PdStatusObservation> {
    // Only the previously confirmed contract survives normal negotiation.
    // Requested power never raises this envelope, and changed capabilities
    // must still cover it before the service can authorize any output.
    if !fusb302b_status_confirms_active_contract(phase, contract, status_raw)
        || !source.supports_contract(contract)
    {
        return None;
    }
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
fn pd_status_observation(runtime: &Fusb302bRuntime) -> Option<PdStatusObservation> {
    if !runtime.service_available() || runtime.request_timed_out || !runtime.vbus_status_observed()
    {
        return None;
    }
    fusb302b_contract_observation(
        runtime.policy.phase(),
        runtime.active_contract(),
        runtime.vbus_status_raw(),
        runtime.source_capabilities()?,
    )
}

#[cfg(target_arch = "xtensa")]
fn publish_pd_snapshot(
    runtime: &Fusb302bRuntime,
    observation: Option<PdStatusObservation>,
    pending: Option<(PowerTicket, PendingPdOperation)>,
) {
    let now_ms = PdTimestamp::now().as_millis();
    let ready = startup_pd_contract_ready(observation);
    let previous_active = PD_SERVICE_SNAPSHOT.lock(|snapshot| snapshot.borrow().active);
    let request_healthy = !runtime.request_rejected
        && !runtime.request_timed_out
        && !runtime.vbus_low_interlocked
        && runtime.service_available();
    let continuous_pps_transition = pending.and_then(|(ticket, operation)| {
        let PendingPdOperation::Contract { request } = operation else {
            return None;
        };
        let same_apdo_and_safe_current = runtime
            .policy
            .confirmed_active_contract()
            .zip(runtime.policy.source_capabilities())
            .is_some_and(|(active, capabilities)| {
                if request.is_calibration() {
                    active.request_keeps_same_pps_apdo_or_reduces_current(
                        capabilities.view(),
                        request,
                    )
                } else {
                    active.request_keeps_same_pps_apdo(capabilities.view(), request)
                }
            });
        Some(continuous_pps_transition_keeps_heater_permit(
            matches!(
                ticket.owner(),
                Some(PowerIntentOwner::Calibration | PowerIntentOwner::ThermalPlantAuto)
            ),
            request.mode() == PdContractRequestMode::Pps,
            same_apdo_and_safe_current,
            !runtime.source_capabilities_refresh_pending,
            request_healthy,
        ))
    }) == Some(true);
    let preserve_previous_contract = previous_pps_transition_keeps_heater_permit(
        heater_permit_is_active(now_ms),
        observation.is_none(),
        previous_active.is_some(),
        matches!(
            runtime.policy.phase(),
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ),
        runtime.source_capabilities().is_some(),
        !runtime.source_capabilities_refresh_pending,
        request_healthy,
    );
    let active = observation
        .and_then(|observed| {
            ConfirmedActiveContract::from_private_contract(
                observed.contract,
                runtime.source_capabilities()?,
            )
        })
        .or_else(|| {
            preserve_previous_contract
                .then_some(previous_active)
                .flatten()
        });
    PD_SERVICE_SNAPSHOT.lock(|snapshot| {
        *snapshot.borrow_mut() = PdServiceSnapshot {
            observation,
            active,
            source: runtime
                .source_capabilities()
                .map_or_else(SourceCapabilitiesView::default, SourceCapabilities::view),
            transition_pending: runtime.source_capabilities_refresh_pending
                || matches!(
                    runtime.policy.phase(),
                    SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
                ),
            capabilities: runtime
                .source_capabilities()
                .and_then(fusb302b_adjustable_power_capabilities),
            controller: ControllerKind::Fusb302b,
            service_available: runtime.service_available(),
            published_at_ms: now_ms,
        };
    });
    if ready || continuous_pps_transition || preserve_previous_contract {
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
    if flash_preparation::is_active() && !matches!(command, PdServiceCommand::AutomaticIdle { .. })
    {
        let ticket = pd_service_command_ticket(command);
        return PdCommandProgress::Done(ticket, TicketOutcome::Rejected);
    }
    match command {
        PdServiceCommand::AutomaticIdle { ticket, minimum_mv } => {
            runtime.policy.set_standby_minimum_mv(minimum_mv);
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
            if flash_preparation::is_active()
                && !matches!(command, PdServiceCommand::AutomaticIdle { .. })
            {
                return Some((pd_service_command_ticket(command), TicketOutcome::Rejected));
            }
            if take_pd_service_command_cancelled(command) {
                let ticket = pd_service_command_ticket(command);
                if pending.is_some_and(|(pending_ticket, _)| pending_ticket == ticket) {
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
            if let Some((ticket, _)) = pending_to_retry
                && take_pd_service_ticket_cancelled(ticket)
            {
                runtime
                    .abort_pending_operation(i2c, PdTimestamp::now())
                    .await;
                *pending = None;
                return Some((ticket, TicketOutcome::Superseded));
            }
            if flash_preparation::is_active()
                && let Some((ticket, operation)) = pending_to_retry
                && !matches!(operation, PendingPdOperation::Idle)
            {
                runtime
                    .abort_pending_operation(i2c, PdTimestamp::now())
                    .await;
                *pending = None;
                return Some((ticket, TicketOutcome::Superseded));
            }
            if let Some((ticket, PendingPdOperation::Idle)) = pending_to_retry {
                match runtime
                    .advance_automatic_idle_transition(i2c, PdTimestamp::now())
                    .await
                {
                    PdContractRequestState::Failed => {
                        *pending = None;
                        return Some((ticket, TicketOutcome::Rejected));
                    }
                    PdContractRequestState::Confirmed | PdContractRequestState::Pending => {}
                }
            }
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
    publish_pd_snapshot(runtime, observation, pending);
    publish_pd_service_state(state);
    if record_heartbeat {
        record_pd_heartbeat();
    }
    if let Some((ticket, outcome)) = terminal {
        publish_pd_service_terminal(state, ticket, outcome).await;
        pending_after_pd_terminal(pending, ticket)
    } else {
        pending
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn pending_after_pd_terminal<T>(
    pending: Option<(PowerTicket, T)>,
    completed: PowerTicket,
) -> Option<(PowerTicket, T)> {
    pending.filter(|(ticket, _)| *ticket != completed)
}

#[cfg(target_arch = "xtensa")]
async fn run_pd_service_turn(
    i2c: &mut PdI2c<'static>,
    runtime: &mut Fusb302bRuntime,
    pending: &mut Option<(PowerTicket, PendingPdOperation)>,
) {
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
/// to its own cadence timer. The priority-3 executor preempts lower-priority
/// application work while the shared I2C bus remains non-blocking.
#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn pd_service_task(bus_address: usize, mut runtime: Box<Fusb302bRuntime>) {
    assert!(matches!(
        esp_hal::system::Cpu::current(),
        esp_hal::system::Cpu::ProCpu
    ));
    // The bus is initialized once on CPU0 and never moved. Both executors run
    // on CPU0; the async mutex serializes access across interrupt preemption.
    let bus = unsafe { &*(bus_address as *const SharedI2cBus) };
    let mut i2c = PdI2c::new(bus);
    let mut pending: Option<(PowerTicket, PendingPdOperation)> = None;
    loop {
        let turn_started_ms = Instant::now().as_millis();
        let acquired_bus = i2c.try_acquire();
        if acquired_bus {
            run_pd_service_turn(&mut i2c, &mut runtime, &mut pending).await;
        } else {
            // Clear authorization immediately, but do not count this skipped
            // turn as PD progress for the watchdog.
            publish_busy_pd_service_turn(&runtime, &mut pending).await;
        }
        let elapsed_ms = Instant::now().as_millis().saturating_sub(turn_started_ms);
        let delay_ms = pd_service_delay_ms(elapsed_ms, acquired_bus);
        EmbassyTimer::after_millis(delay_ms).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn spawn_pd_service(
    spawner: embassy_executor::SendSpawner,
    i2c: PdI2c<'static>,
    runtime: Box<Fusb302bRuntime>,
) {
    let bus_address = i2c.into_bus_address();
    spawn_power_coordinator(spawner);
    spawner
        .spawn(pd_service_task(bus_address, runtime))
        .expect("failed to spawn PD service task");
}
