#[allow(unused_imports)]
use super::*;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_TICK_MS: u64 = 5;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_COMMAND_CAPACITY: usize = 8;

#[cfg(target_arch = "xtensa")]
pub(crate) const PD_SERVICE_MAX_COMMANDS_PER_TICK: usize = 1;

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) enum PdServiceCommand {
    FixedVoltage(u16),
    PpsVoltage(u16),
    Interlock { now_ms: u64 },
}

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy)]
pub(crate) struct PdServiceSnapshot {
    pub(crate) observation: Option<PdStatusObservation>,
    pub(crate) capabilities: Option<ch224q::AdjustablePowerCapabilities>,
    pub(crate) controller: ControllerKind,
    pub(crate) service_available: bool,
    pub(crate) stale_contract_vin_guard_suspended: bool,
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
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_COMMANDS: Channel<
    CriticalSectionRawMutex,
    PdServiceCommand,
    PD_SERVICE_COMMAND_CAPACITY,
> = Channel::new();

#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_SNAPSHOT: BlockingMutex<
    CriticalSectionRawMutex,
    RefCell<PdServiceSnapshot>,
> = BlockingMutex::new(RefCell::new(PdServiceSnapshot::unavailable()));

#[cfg(target_arch = "xtensa")]
#[derive(Clone, Copy, Default)]
pub(crate) struct PdServiceClient;

#[cfg(target_arch = "xtensa")]
impl PdServiceClient {
    pub(crate) const fn new() -> Self {
        Self
    }

    pub(crate) fn mark_starting_fusb302b() -> Self {
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
        PD_SERVICE_SNAPSHOT.lock(|snapshot| {
            *snapshot.borrow_mut() = PdServiceSnapshot::unavailable();
        });
        HeaterPwmGate::force_off();
        Self::new()
    }

    pub(crate) fn snapshot(&self) -> PdServiceSnapshot {
        PD_SERVICE_SNAPSHOT.lock(|snapshot| *snapshot.borrow())
    }

    pub(crate) fn observation(&self) -> Option<PdStatusObservation> {
        self.snapshot().observation
    }

    pub(crate) fn capabilities(&self) -> Option<ch224q::AdjustablePowerCapabilities> {
        self.snapshot().capabilities
    }

    pub(crate) fn controller_kind(&self) -> ControllerKind {
        self.snapshot().controller
    }

    pub(crate) fn service_available(&self) -> bool {
        self.snapshot().service_available
    }

    pub(crate) fn stale_contract_vin_guard_suspended(&self, _now_ms: u64) -> bool {
        self.snapshot().stale_contract_vin_guard_suspended
    }

    pub(crate) fn interlock_after_stale_contract(&self, now_ms: u64) {
        let snapshot = self.snapshot();
        PD_SERVICE_SNAPSHOT.lock(|current| {
            let mut current = current.borrow_mut();
            current.observation = None;
            current.stale_contract_vin_guard_suspended = false;
        });
        HeaterPwmGate::force_off();
        if snapshot.service_available {
            let _ = PD_SERVICE_COMMANDS.try_send(PdServiceCommand::Interlock { now_ms });
        }
    }

    fn submit_request(
        &self,
        command: PdServiceCommand,
        requested_mv: u16,
        kind: ContractKind,
    ) -> PdContractRequestState {
        let snapshot = self.snapshot();
        if !snapshot.service_available || snapshot.controller != ControllerKind::Fusb302b {
            return PdContractRequestState::Failed;
        }
        if snapshot.observation.is_some_and(|observation| {
            observation.contract.kind == kind && observation.contract.voltage_mv == requested_mv
        }) {
            return PdContractRequestState::Confirmed;
        }
        // Requests are deliberately non-blocking. The service task owns the
        // request and the next snapshot establishes whether it was accepted.
        // A full queue is a real failure; reporting Pending here would allow
        // the front-panel loop to wait forever for a command that was dropped.
        match PD_SERVICE_COMMANDS.try_send(command) {
            Ok(()) => PdContractRequestState::Pending,
            Err(_) => PdContractRequestState::Failed,
        }
    }

    pub(crate) fn request_fixed_voltage(
        &self,
        request: ch224q::VoltageRequest,
    ) -> PdContractRequestState {
        self.submit_request(
            PdServiceCommand::FixedVoltage(request.millivolts()),
            request.millivolts(),
            ContractKind::Fixed,
        )
    }

    pub(crate) fn request_pps_voltage(&self, request_mv: u16) -> PdContractRequestState {
        self.submit_request(
            PdServiceCommand::PpsVoltage(request_mv),
            request_mv,
            ContractKind::Pps,
        )
    }
}

#[cfg(target_arch = "xtensa")]
fn pd_status_observation(runtime: &Fusb302bRuntime) -> Option<PdStatusObservation> {
    let contract = runtime.active_contract();
    (contract != Contract::none()).then(|| {
        let status_raw = 1 << 3;
        PdStatusObservation {
            status_raw,
            status: Status::from_register(status_raw),
            current_raw: 0,
            current_ma: contract.current_ma,
            contract_voltage_mv: Some(contract.voltage_mv),
            contract,
        }
    })
}

#[cfg(target_arch = "xtensa")]
fn publish_pd_snapshot(runtime: &Fusb302bRuntime) {
    let observation = pd_status_observation(runtime);
    let ready = startup_pd_contract_ready(observation);
    PD_SERVICE_SNAPSHOT.lock(|snapshot| {
        *snapshot.borrow_mut() = PdServiceSnapshot {
            observation,
            capabilities: runtime
                .source_capabilities()
                .and_then(fusb302b_adjustable_power_capabilities),
            controller: ControllerKind::Fusb302b,
            service_available: runtime.service_available(),
            stale_contract_vin_guard_suspended: runtime
                .stale_contract_vin_guard_suspended(PdTimestamp::now().as_millis()),
        };
    });
    let previously_permitted = PD_HEATER_PERMIT.swap(u8::from(ready), Ordering::AcqRel) != 0;
    if previously_permitted && !ready {
        HeaterPwmGate::force_off();
    }
}

#[cfg(target_arch = "xtensa")]
async fn process_pd_command(
    runtime: &mut Fusb302bRuntime,
    i2c: &mut PdI2c<'static>,
    command: PdServiceCommand,
) {
    match command {
        PdServiceCommand::FixedVoltage(request_mv) => {
            let _ = runtime
                .request_fixed_voltage(i2c, request_mv, PdTimestamp::now())
                .await;
        }
        PdServiceCommand::PpsVoltage(request_mv) => {
            let _ = runtime
                .request_pps_voltage(i2c, request_mv, PdTimestamp::now())
                .await;
        }
        PdServiceCommand::Interlock { now_ms } => {
            runtime.interlock_after_stale_contract(now_ms);
            HeaterPwmGate::force_off();
        }
    }
}

/// The sole owner of FUSB302B policy state and physical PD I2C transactions.
/// Every loop turn does bounded work, releases the shared bus, and then yields
/// to its own cadence timer. This task deliberately stays out of interrupt
/// context because FUSB302B protocol recovery has a deep call stack.
#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
async fn pd_service_task(mut i2c: PdI2c<'static>, mut runtime: Box<Fusb302bRuntime>) {
    loop {
        if i2c.try_acquire() {
            for _ in 0..PD_SERVICE_MAX_COMMANDS_PER_TICK {
                let Ok(command) = PD_SERVICE_COMMANDS.try_receive() else {
                    break;
                };
                process_pd_command(&mut runtime, &mut i2c, command).await;
            }
            // A request flood must never turn the command mailbox into a second
            // unbounded work queue. Poll once on every service turn regardless of
            // how many commands are waiting.
            let _ = runtime.poll(&mut i2c, PdTimestamp::now()).await;
            i2c.release();
        }
        publish_pd_snapshot(&runtime);
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
    spawner
        .spawn(pd_service_task(i2c, runtime))
        .expect("failed to spawn PD service task");
}
