#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceMode {
    Idle,
    Sampling,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceStatus {
    pub mode: DeviceMode,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
}

static SAMPLE_TICK: AtomicU32 = AtomicU32::new(0);

pub fn snapshot() -> DeviceStatus {
    let tick = SAMPLE_TICK.fetch_add(1, Ordering::Relaxed);
    DeviceStatus {
        mode: DeviceMode::Sampling,
        voltage_mv: 12_000 + (tick % 50),
        current_ma: 800 + (tick % 40),
        board_temp_centi: 3_200 + ((tick % 30) as i32),
    }
}

pub async fn poll_once() -> DeviceStatus {
    embassy_futures::yield_now().await;
    snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_starts_in_sampling_mode() {
        let value = snapshot();
        assert_eq!(value.mode, DeviceMode::Sampling);
        assert!(value.voltage_mv >= 12_000);
        assert!(value.current_ma >= 800);
    }
}
