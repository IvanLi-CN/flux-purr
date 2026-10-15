#[allow(unused_imports)]
use super::*;

/// Current reserved for the controller, display and fan while automatic
/// heating owns the remainder of the negotiated source contract.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTOMATIC_HEATER_CURRENT_RESERVE_MA: u16 = 200;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomaticHeaterResistanceSource {
    RamPreview,
    ProjectedRaw,
    SavedCurve,
    Theoretical,
}

#[cfg(any(target_arch = "xtensa", test))]
impl AutomaticHeaterResistanceSource {
    #[allow(dead_code)]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::RamPreview => "ram-preview",
            Self::ProjectedRaw => "projected-raw",
            Self::SavedCurve => "saved-curve",
            Self::Theoretical => "theoretical",
        }
    }
}

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AutomaticHeaterPpsCandidate {
    pub(crate) min_mv: u16,
    pub(crate) max_mv: u16,
    pub(crate) current_ma: u16,
    pub(crate) allowed_mv: u16,
    pub(crate) power_mw: u32,
    pub(crate) advertisement_index: usize,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn encode_automatic_pps_current_ma(max_ma: u16) -> u16 {
    (max_ma.min(MAX_HEATER_CONTRACT_MA) / 50) * 50
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn encode_automatic_fixed_current_ma(max_ma: u16) -> u16 {
    (max_ma.min(MAX_HEATER_CONTRACT_MA) / 10) * 10
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_controller_current_ma(
    candidate_current_ma: u16,
    contract_min_mv: u16,
    contract_max_mv: u16,
    observation: Option<PdStatusObservation>,
) -> u16 {
    let Some(observation) = observation else {
        return candidate_current_ma;
    };
    let Some(contract_mv) = observation.contract_voltage_mv else {
        return 0;
    };
    if !observation.status.pd_active
        || !(contract_min_mv..=contract_max_mv).contains(&contract_mv)
        || observation.current_ma < MIN_HEATER_CONTRACT_MA
    {
        return 0;
    }
    candidate_current_ma.min(observation.current_ma)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_pps_candidate(
    apdo: ch224q::PpsApdo,
    request_min_mv: u16,
    request_max_mv: u16,
    hardware_max_mv: u16,
    resistance_ohms: f32,
    advertisement_index: usize,
) -> Option<AutomaticHeaterPpsCandidate> {
    if !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return None;
    }
    let min_mv = apdo.min_mv.max(request_min_mv);
    let max_mv = apdo.max_mv.min(request_max_mv).min(hardware_max_mv);
    let current_ma = encode_automatic_pps_current_ma(apdo.max_ma);
    if min_mv > max_mv || current_ma < MIN_HEATER_CONTRACT_MA {
        return None;
    }
    let heater_budget_ma = current_ma.saturating_sub(AUTOMATIC_HEATER_CURRENT_RESERVE_MA);
    let electrical_limit_mv = (resistance_ohms * f32::from(heater_budget_ma))
        .floor()
        .clamp(0.0, f32::from(u16::MAX)) as u16;
    let allowed_mv = floor_mv_to_100mv(max_mv.min(electrical_limit_mv));
    if allowed_mv < min_mv {
        return None;
    }
    let power_mw = ((f32::from(allowed_mv) * f32::from(allowed_mv) / resistance_ohms) / 1_000.0)
        .max(0.0)
        .min(u32::MAX as f32) as u32;
    Some(AutomaticHeaterPpsCandidate {
        min_mv,
        max_mv,
        current_ma,
        allowed_mv,
        power_mw,
        advertisement_index,
    })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn select_automatic_heater_pps_candidate(
    apdos: &[Option<ch224q::PpsApdo>; ch224q::MAX_PPS_APDOS],
    request_min_mv: u16,
    request_max_mv: u16,
    hardware_max_mv: u16,
    resistance_ohms: f32,
    current_request_mv: Option<u16>,
) -> Option<AutomaticHeaterPpsCandidate> {
    apdos
        .iter()
        .enumerate()
        .filter_map(|(index, apdo)| {
            automatic_heater_pps_candidate(
                (*apdo)?,
                request_min_mv,
                request_max_mv,
                hardware_max_mv,
                resistance_ohms,
                index,
            )
        })
        .max_by(|left, right| {
            left.power_mw
                .cmp(&right.power_mw)
                .then_with(|| {
                    let left_keeps_current = current_request_mv.is_some_and(|voltage_mv| {
                        (left.min_mv..=left.max_mv).contains(&voltage_mv)
                    });
                    let right_keeps_current = current_request_mv.is_some_and(|voltage_mv| {
                        (right.min_mv..=right.max_mv).contains(&voltage_mv)
                    });
                    left_keeps_current.cmp(&right_keeps_current)
                })
                .then_with(|| right.allowed_mv.cmp(&left.allowed_mv))
                .then_with(|| right.advertisement_index.cmp(&left.advertisement_index))
        })
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_fixed_is_safe(
    voltage_mv: u16,
    source_max_ma: u16,
    hardware_max_mv: u16,
    resistance_ohms: f32,
) -> bool {
    if voltage_mv == 0 || !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return false;
    }
    let current_ma = encode_automatic_fixed_current_ma(source_max_ma);
    current_ma >= MIN_HEATER_CONTRACT_MA
        && voltage_mv <= hardware_max_mv
        && f32::from(voltage_mv) / resistance_ohms
            <= f32::from(current_ma.saturating_sub(AUTOMATIC_HEATER_CURRENT_RESERVE_MA))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_fixed_request(
    source_capabilities: SourceCapabilitiesView,
    resistance_ohms: f32,
) -> Option<(ch224q::VoltageRequest, u16)> {
    if !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return None;
    }
    source_capabilities
        .fixed
        .iter()
        .enumerate()
        .filter_map(|(advertisement_index, fixed)| {
            let (voltage_mv, fixed_max_ma) = (*fixed)?;
            let request = match voltage_mv {
                5_000 => ch224q::VoltageRequest::V5,
                9_000 => ch224q::VoltageRequest::V9,
                12_000 => ch224q::VoltageRequest::V12,
                15_000 => ch224q::VoltageRequest::V15,
                20_000 => ch224q::VoltageRequest::V20,
                28_000 => ch224q::VoltageRequest::V28,
                _ => return None,
            };
            let request_ma = encode_automatic_fixed_current_ma(fixed_max_ma);
            if !automatic_heater_fixed_is_safe(
                voltage_mv,
                request_ma,
                HEATER_ADJUSTABLE_MAX_MV,
                resistance_ohms,
            ) {
                return None;
            }
            let power_mw =
                (f32::from(voltage_mv) * f32::from(voltage_mv) / resistance_ohms / 1_000.0) as u32;
            Some((request, request_ma, power_mw, advertisement_index))
        })
        .max_by(|left, right| {
            left.2
                .cmp(&right.2)
                .then_with(|| right.0.millivolts().cmp(&left.0.millivolts()))
                .then_with(|| right.3.cmp(&left.3))
        })
        .map(|(request, request_ma, _, _)| (request, request_ma))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_request_mv_from_power(
    requested_power_mw: u32,
    resistance_ohms: f32,
    floor_mv: u16,
    ceiling_mv: u16,
) -> u16 {
    let floor_mv = floor_mv.clamp(CH224Q_ADJUSTABLE_REQUEST_MIN_MV, HEATER_ADJUSTABLE_MAX_MV);
    let ceiling_mv = ceiling_mv
        .max(floor_mv)
        .clamp(floor_mv, HEATER_ADJUSTABLE_MAX_MV);
    if requested_power_mw == 0 || !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return floor_mv;
    }
    let requested_mv = (requested_power_mw as f32 * 1_000.0 * resistance_ohms)
        .sqrt()
        .floor()
        .clamp(f32::from(floor_mv), f32::from(ceiling_mv)) as u16;
    floor_mv_to_100mv(requested_mv).clamp(floor_mv, ceiling_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_pwm_percent_for_power(
    requested_power_mw: u32,
    resistance_ohms: f32,
    confirmed_mv: u16,
    warmup_soft_start_percent: u8,
) -> u8 {
    if requested_power_mw == 0
        || confirmed_mv == 0
        || !resistance_ohms.is_finite()
        || resistance_ohms <= 0.0
    {
        return 0;
    }
    let pon_mw = f32::from(confirmed_mv) * f32::from(confirmed_mv) / resistance_ohms / 1_000.0;
    if !pon_mw.is_finite() || pon_mw <= 0.0 {
        return 0;
    }
    let duty = (requested_power_mw as f32 / pon_mw * 100.0)
        .floor()
        .clamp(0.0, 100.0) as u8;
    apply_warmup_soft_start(duty, warmup_soft_start_percent)
}
