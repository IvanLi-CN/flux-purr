#![no_std]

pub mod adapters;
pub mod board;

use core::sync::atomic::{AtomicU32, Ordering};

pub use adapters::tca6408a::FrontPanelKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMode {
    Idle,
    Sampling,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbRoute {
    Mcu,
    Sink,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdState {
    Negotiating,
    Ready,
    Fallback5V,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceStatus {
    pub mode: DeviceMode,
    pub voltage_mv: u32,
    pub current_ma: u32,
    pub board_temp_centi: i32,
    pub pd_request_mv: u16,
    pub pd_contract_mv: u16,
    pub pd_state: PdState,
    pub usb_route: UsbRoute,
    pub fan_enabled: bool,
    pub fan_pwm_permille: u16,
    pub frontpanel_key: Option<FrontPanelKey>,
}

static SAMPLE_TICK: AtomicU32 = AtomicU32::new(0);

fn to_usb_route(route: adapters::ch442e::Route) -> UsbRoute {
    match route {
        adapters::ch442e::Route::Mcu => UsbRoute::Mcu,
        adapters::ch442e::Route::Sink => UsbRoute::Sink,
        adapters::ch442e::Route::Disabled => UsbRoute::Disabled,
    }
}

pub fn snapshot() -> DeviceStatus {
    let tick = SAMPLE_TICK.fetch_add(1, Ordering::Relaxed);
    let request = adapters::ch224q::VoltageRequest::V28;
    let route = adapters::ch442e::Pins::default_mcu().route();

    let fallback = tick % 17 == 0;
    let pd_contract_mv = if fallback {
        adapters::ch224q::VoltageRequest::V5.millivolts()
    } else {
        request.millivolts()
    };

    let fan_pwm = 600 + (tick % 200) as u16;

    DeviceStatus {
        mode: DeviceMode::Sampling,
        voltage_mv: 12_000 + (tick % 50),
        current_ma: 800 + (tick % 40),
        board_temp_centi: 3_200 + ((tick % 30) as i32),
        pd_request_mv: request.millivolts(),
        pd_contract_mv,
        pd_state: if fallback {
            PdState::Fallback5V
        } else {
            PdState::Ready
        },
        usb_route: to_usb_route(route),
        fan_enabled: true,
        fan_pwm_permille: fan_pwm,
        frontpanel_key: if tick % 10 == 0 {
            Some(FrontPanelKey::Center)
        } else {
            None
        },
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

    #[test]
    fn snapshot_includes_pd_and_routing_fields() {
        let value = snapshot();
        assert_eq!(value.pd_request_mv, 28_000);
        assert!(value.pd_contract_mv == 28_000 || value.pd_contract_mv == 5_000);
        assert_eq!(value.usb_route, UsbRoute::Mcu);
        assert!(value.fan_enabled);
        assert!(value.fan_pwm_permille >= 600);
    }
}
