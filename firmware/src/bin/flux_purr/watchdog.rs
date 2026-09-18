#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Default)]
pub(crate) struct WatchdogFeedGate {
    runtime_heartbeat: u32,
    pd_heartbeat: u32,
    armed: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
impl WatchdogFeedGate {
    pub(crate) fn observe(&mut self, runtime_heartbeat: u32, pd_heartbeat: u32) -> bool {
        if !self.armed {
            if runtime_heartbeat == 0 || pd_heartbeat == 0 {
                return false;
            }
            self.runtime_heartbeat = runtime_heartbeat;
            self.pd_heartbeat = pd_heartbeat;
            self.armed = true;
            return true;
        }

        if runtime_heartbeat == self.runtime_heartbeat || pd_heartbeat == self.pd_heartbeat {
            return false;
        }

        self.runtime_heartbeat = runtime_heartbeat;
        self.pd_heartbeat = pd_heartbeat;
        true
    }
}

#[cfg(target_arch = "xtensa")]
const WATCHDOG_SAMPLE_MS: u64 = 500;
#[cfg(target_arch = "xtensa")]
const WATCHDOG_TIMEOUT_MS: u64 = 5_000;

#[cfg(target_arch = "xtensa")]
static WATCHDOG_ARMED: AtomicU8 = AtomicU8::new(0);
#[cfg(target_arch = "xtensa")]
static RUNTIME_HEARTBEAT: AtomicU32 = AtomicU32::new(0);
#[cfg(target_arch = "xtensa")]
static PD_HEARTBEAT: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "xtensa")]
pub(crate) fn record_runtime_heartbeat() {
    RUNTIME_HEARTBEAT.fetch_add(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn record_pd_heartbeat() {
    PD_HEARTBEAT.fetch_add(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
pub(crate) fn arm_watchdog() {
    WATCHDOG_ARMED.store(1, Ordering::Release);
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
pub(crate) async fn watchdog_task(
    mut watchdog: esp_hal::timer::timg::Wdt<esp_hal::peripherals::TIMG0<'static>>,
) {
    watchdog.set_timeout(
        esp_hal::timer::timg::MwdtStage::Stage0,
        HalDuration::from_millis(WATCHDOG_TIMEOUT_MS),
    );

    let mut gate = WatchdogFeedGate::default();
    let mut watchdog_enabled = false;
    loop {
        EmbassyTimer::after_millis(WATCHDOG_SAMPLE_MS).await;
        if WATCHDOG_ARMED.load(Ordering::Acquire) == 0 {
            continue;
        }

        let runtime_heartbeat = RUNTIME_HEARTBEAT.load(Ordering::Acquire);
        let pd_heartbeat = PD_HEARTBEAT.load(Ordering::Acquire);
        if gate.observe(runtime_heartbeat, pd_heartbeat) {
            if !watchdog_enabled {
                watchdog.enable();
                watchdog_enabled = true;
            }
            watchdog.feed();
        }
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
