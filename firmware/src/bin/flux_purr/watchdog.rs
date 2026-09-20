#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Default)]
pub(crate) struct WatchdogFeedGate {
    boot_heartbeat: u32,
    runtime_heartbeat: u32,
    pd_heartbeat: u32,
    armed: bool,
    runtime_started: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl WatchdogFeedGate {
    pub(crate) fn observe(
        &mut self,
        boot_heartbeat: u32,
        runtime_heartbeat: u32,
        pd_heartbeat: u32,
        pd_service_required: bool,
    ) -> bool {
        if runtime_heartbeat == 0 {
            if boot_heartbeat == 0 {
                return false;
            }
            if !self.armed {
                self.boot_heartbeat = boot_heartbeat;
                self.armed = true;
                return true;
            }
            if boot_heartbeat == self.boot_heartbeat {
                return false;
            }
            self.boot_heartbeat = boot_heartbeat;
            return true;
        }

        if pd_service_required && pd_heartbeat == 0 {
            return false;
        }
        if !self.armed {
            self.runtime_heartbeat = runtime_heartbeat;
            self.pd_heartbeat = pd_heartbeat;
            self.armed = true;
            self.runtime_started = true;
            return true;
        }

        if !self.runtime_started {
            self.runtime_heartbeat = runtime_heartbeat;
            self.pd_heartbeat = pd_heartbeat;
            self.runtime_started = true;
            return true;
        }

        if runtime_heartbeat == self.runtime_heartbeat
            || (pd_service_required && pd_heartbeat == self.pd_heartbeat)
        {
            return false;
        }

        self.runtime_heartbeat = runtime_heartbeat;
        self.pd_heartbeat = pd_heartbeat;
        true
    }
}

#[cfg(target_arch = "xtensa")]
const WATCHDOG_SAMPLE_MS: u64 = 50;
#[cfg(target_arch = "xtensa")]
const WATCHDOG_TIMEOUT_MS: u64 = 5_000;

#[cfg(target_arch = "xtensa")]
static WATCHDOG_ARMED: AtomicU8 = AtomicU8::new(0);
#[cfg(target_arch = "xtensa")]
static WATCHDOG_ENABLED: AtomicU8 = AtomicU8::new(0);
#[cfg(target_arch = "xtensa")]
static BOOT_HEARTBEAT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "xtensa")]
static RUNTIME_HEARTBEAT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "xtensa")]
static PD_HEARTBEAT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "xtensa")]
pub(crate) static PD_SERVICE_REQUIRED: AtomicU8 = AtomicU8::new(0);

#[cfg(target_arch = "xtensa")]
pub(crate) fn record_runtime_heartbeat() {
    RUNTIME_HEARTBEAT.fetch_add(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn record_boot_heartbeat() {
    BOOT_HEARTBEAT.fetch_add(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn record_pd_heartbeat() {
    PD_HEARTBEAT.fetch_add(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) async fn arm_watchdog() {
    WATCHDOG_ARMED.store(1, Ordering::Release);
    while WATCHDOG_ENABLED.load(Ordering::Acquire) == 0 {
        EmbassyTimer::after_millis(1).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn configure_watchdog_before_rtos(
    mut watchdog: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
) -> esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>> {
    // `set_timeout` reads esp-hal's global clock singleton. Configure the
    // hardware before esp-rtos transfers the bootstrap context to its timer
    // scheduler; the runtime supervisor only performs register-level feed
    // operations after that handoff.
    watchdog.set_timeout(
        esp_hal::timer::timg::MwdtStage::Stage0,
        HalDuration::from_millis(WATCHDOG_TIMEOUT_MS),
    );
    watchdog
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
pub(crate) async fn watchdog_task(
    mut watchdog: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
) {
    let mut gate = WatchdogFeedGate::default();
    let mut watchdog_enabled = false;
    loop {
        if WATCHDOG_ARMED.load(Ordering::Acquire) != 0 && !watchdog_enabled {
            watchdog.enable();
            watchdog_enabled = true;
            WATCHDOG_ENABLED.store(1, Ordering::Release);
            rom_boot_stage(b"watchdog_armed");
        }

        if watchdog_enabled {
            let boot_heartbeat = BOOT_HEARTBEAT.load(Ordering::Acquire);
            let runtime_heartbeat = RUNTIME_HEARTBEAT.load(Ordering::Acquire);
            let pd_heartbeat = PD_HEARTBEAT.load(Ordering::Acquire);
            let pd_service_required = PD_SERVICE_REQUIRED.load(Ordering::Acquire) != 0;
            if gate.observe(
                boot_heartbeat,
                runtime_heartbeat,
                pd_heartbeat,
                pd_service_required,
            ) {
                watchdog.feed();
            }
            if PD_HEATER_PERMIT.load(Ordering::Acquire) != 0
                && !heater_permit_is_active(Instant::now().as_millis())
            {
                HeaterPwmGate::force_off();
            }
        }

        EmbassyTimer::after_millis(WATCHDOG_SAMPLE_MS).await;
    }
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn spawn_watchdog(
    spawner: Spawner,
    watchdog: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
) {
    spawner
        .spawn(watchdog_task(watchdog))
        .expect("failed to spawn watchdog supervisor");
}
