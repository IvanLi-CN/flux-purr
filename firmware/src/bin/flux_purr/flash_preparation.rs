//! RAM-only admission hold used while a host prepares the device for flashing.
//!
//! The hold deliberately has no persistence or timeout. A reset clears the
//! process image; an explicit cancel clears the hold while leaving heating
//! disarmed until a new user intent is submitted.

#[allow(unused_imports)]
use super::*;

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
static FLASH_PREPARATION_HOLD: AtomicU8 = AtomicU8::new(0);

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
pub(crate) fn is_active() -> bool {
    FLASH_PREPARATION_HOLD.load(Ordering::Acquire) != 0
}

#[cfg(not(any(target_arch = "xtensa", test)))]
#[allow(dead_code)]
pub(crate) const fn is_active() -> bool {
    false
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
pub(crate) fn start_hold() {
    FLASH_PREPARATION_HOLD.store(1, Ordering::Release);
    #[cfg(target_arch = "xtensa")]
    HeaterPwmGate::force_off();
}

#[cfg(any(target_arch = "xtensa", test))]
#[allow(dead_code)]
pub(crate) fn clear_hold() {
    FLASH_PREPARATION_HOLD.store(0, Ordering::Release);
    #[cfg(target_arch = "xtensa")]
    HeaterPwmGate::force_off();
}
