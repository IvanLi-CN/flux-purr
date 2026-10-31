#[allow(unused_imports)]
use super::*;

/// Normal automatic heating is calibrated and bounded against the product's
/// 3 A operating envelope. Manual PPS, calibration, and debug paths keep the
/// broader adapter limit from `MAX_HEATER_CONTRACT_MA`.
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const MAX_AUTOMATIC_HEATER_CONTRACT_MA: u16 = 3_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTOMATIC_TERMINAL_VOLTAGE_MV: u16 = 21_000;
#[cfg(any(target_arch = "xtensa", test))]
pub(crate) const AUTOMATIC_TERMINAL_CURRENT_MA: u16 = 2_850;

#[cfg(any(target_arch = "xtensa", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutomaticHeaterResistanceSource {
    Theoretical,
}

#[cfg(any(target_arch = "xtensa", test))]
impl AutomaticHeaterResistanceSource {
    #[allow(dead_code)]
    pub(crate) const fn label(self) -> &'static str {
        match self {
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
    pub(crate) boundary_applied: bool,
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn encode_automatic_pps_current_ma(max_ma: u16) -> u16 {
    (max_ma.min(MAX_AUTOMATIC_HEATER_CONTRACT_MA) / 50) * 50
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn encode_automatic_fixed_current_ma(max_ma: u16) -> u16 {
    (max_ma.min(MAX_AUTOMATIC_HEATER_CONTRACT_MA) / 10) * 10
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
        || (observation.current_ma < MIN_HEATER_CONTRACT_MA
            && !(candidate_current_ma == AUTOMATIC_TERMINAL_CURRENT_MA
                && observation.contract.current_ma >= candidate_current_ma))
    {
        return 0;
    }
    if candidate_current_ma == AUTOMATIC_TERMINAL_CURRENT_MA
        && observation.contract.current_ma >= candidate_current_ma
    {
        // During the safety transition to the power-safe 21 V terminal, the
        // MOS gate is blanked before the new contract settles. The measured
        // source current can therefore be near zero even though the
        // confirmed contract still grants the terminal current budget.
        return candidate_current_ma;
    }
    candidate_current_ma.min(observation.current_ma)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_power_budget_mw(
    ceiling_mv: u16,
    resistance_ohms: f32,
    contract_current_ma: u16,
) -> u32 {
    if ceiling_mv == 0
        || contract_current_ma == 0
        || !resistance_ohms.is_finite()
        || resistance_ohms <= 0.0
    {
        return 0;
    }
    let model_power_mw = f32::from(ceiling_mv).powi(2) / resistance_ohms / 1_000.0;
    let contract_power_mw = f32::from(ceiling_mv) * f32::from(contract_current_ma) / 1_000.0;
    model_power_mw
        .min(contract_power_mw)
        .max(0.0)
        .min(u32::MAX as f32) as u32
}

#[cfg(test)]
pub(crate) fn automatic_heater_pps_candidate(
    apdo: ch224q::PpsApdo,
    request_min_mv: u16,
    request_max_mv: u16,
    hardware_max_mv: u16,
    resistance_ohms: f32,
    advertisement_index: usize,
) -> Option<AutomaticHeaterPpsCandidate> {
    automatic_heater_pps_candidate_with_limit(
        apdo,
        request_min_mv,
        request_max_mv,
        hardware_max_mv,
        resistance_ohms,
        advertisement_index,
        None,
    )
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_pps_candidate_with_limit(
    apdo: ch224q::PpsApdo,
    request_min_mv: u16,
    request_max_mv: u16,
    hardware_max_mv: u16,
    resistance_ohms: f32,
    advertisement_index: usize,
    observed_limit_mv: Option<u16>,
) -> Option<AutomaticHeaterPpsCandidate> {
    if !resistance_ohms.is_finite() || resistance_ohms <= 0.0 {
        return None;
    }
    let min_mv = apdo.min_mv.max(request_min_mv);
    let max_mv = apdo.max_mv.min(request_max_mv).min(hardware_max_mv);
    let current_ma = encode_automatic_pps_current_ma(apdo.max_ma);
    let terminal_current =
        observed_limit_mv.is_some() && current_ma == AUTOMATIC_TERMINAL_CURRENT_MA;
    if min_mv > max_mv || (current_ma < MIN_HEATER_CONTRACT_MA && !terminal_current) {
        return None;
    }
    let heater_budget_ma = current_ma;
    let electrical_limit_mv = (resistance_ohms * f32::from(heater_budget_ma))
        .floor()
        .clamp(0.0, f32::from(u16::MAX)) as u16;
    let allowed_mv =
        floor_mv_to_100mv(max_mv.min(observed_limit_mv.unwrap_or(electrical_limit_mv)));
    if allowed_mv < min_mv {
        return None;
    }
    let model_power_mw =
        (f32::from(allowed_mv) * f32::from(allowed_mv) / resistance_ohms) / 1_000.0;
    let contract_power_mw = f32::from(allowed_mv) * f32::from(current_ma) / 1_000.0;
    let power_mw = (if terminal_current {
        model_power_mw.min(contract_power_mw)
    } else {
        model_power_mw
    })
    .max(0.0)
    .min(u32::MAX as f32) as u32;
    Some(AutomaticHeaterPpsCandidate {
        min_mv,
        max_mv,
        current_ma,
        allowed_mv,
        power_mw,
        advertisement_index,
        boundary_applied: observed_limit_mv.is_some(),
    })
}

#[cfg(test)]
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

#[cfg(test)]
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
        && f32::from(voltage_mv) / resistance_ohms <= f32::from(current_ma)
}

#[cfg(test)]
pub(crate) fn automatic_heater_fixed_request(
    source_capabilities: SourceCapabilitiesView,
    resistance_ohms: f32,
) -> Option<(ch224q::VoltageRequest, u16)> {
    automatic_heater_fixed_request_with_safety(
        source_capabilities,
        resistance_ohms,
        |voltage_mv, request_ma| {
            automatic_heater_fixed_is_safe(
                voltage_mv,
                request_ma,
                HEATER_ADJUSTABLE_MAX_MV,
                resistance_ohms,
            )
        },
    )
}

#[cfg(any(target_arch = "xtensa", test))]
fn automatic_heater_fixed_request_with_safety(
    source_capabilities: SourceCapabilitiesView,
    resistance_ohms: f32,
    mut is_safe: impl FnMut(u16, u16) -> bool,
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
            if voltage_mv > FUSB302B_FIXED_MAX_MV {
                return None;
            }
            let request = match voltage_mv {
                5_000 => ch224q::VoltageRequest::V5,
                9_000 => ch224q::VoltageRequest::V9,
                12_000 => ch224q::VoltageRequest::V12,
                15_000 => ch224q::VoltageRequest::V15,
                20_000 => ch224q::VoltageRequest::V20,
                _ => return None,
            };
            let request_ma = encode_automatic_fixed_current_ma(fixed_max_ma);
            if !is_safe(voltage_mv, request_ma) {
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
pub(crate) fn automatic_heater_fixed_request_with_boundary(
    source_capabilities: SourceCapabilitiesView,
    resistance_ohms: f32,
    current_temp_c: f32,
    current_rtd_mv: Option<u16>,
    memory_config: &MemoryConfig,
) -> Option<(ch224q::VoltageRequest, u16)> {
    let boundary = memory_config.heater_supply_boundary.as_ref()?;
    if !boundary.is_valid() {
        return None;
    }
    let source_apdo_matches_boundary = source_capabilities.pps.iter().flatten().any(|range| {
        range.min_mv == boundary.advertised_min_mv
            && range.max_mv == boundary.advertised_max_mv
            && range.max_current_ma >= boundary.source_max_ma
    });
    if !source_apdo_matches_boundary {
        return None;
    }
    let source_power_limit_mw = source_capabilities
        .fixed
        .iter()
        .flatten()
        .filter(|(voltage_mv, _)| *voltage_mv >= GUARANTEED_HEATER_MIN_MV)
        .map(|(voltage_mv, current_ma)| u32::from(*voltage_mv) * u32::from(*current_ma) / 1_000)
        .max()
        .unwrap_or(0);
    if source_power_limit_mw != boundary.source_power_limit_mw {
        return None;
    }
    let wifi_started = flux_purr_firmware::heater_supply::wifi_hardware_started()?;
    let temperature_centi_c = round_to_i16(current_temp_c * 100.0);
    automatic_heater_fixed_request_with_safety(
        source_capabilities,
        resistance_ohms,
        |voltage_mv, request_ma| {
            let query = flux_purr_firmware::heater_supply::BoundaryQuery {
                temperature_centi_c,
                current_rtd_mv,
                current_request_mv: Some(voltage_mv),
                current_ma: request_ma,
                wifi_started,
                source_min_mv: boundary.advertised_min_mv,
                source_max_mv: boundary.advertised_max_mv,
                operational_source_min_mv: boundary.source_min_mv,
                operational_source_max_mv: boundary.source_max_mv,
                source_max_ma: boundary.source_max_ma,
                source_power_limit_mw: boundary.source_power_limit_mw,
            };
            let mut project_temperature = |raw| {
                projected_rtd_temperature_c(memory_config, raw)
                    .map(|temperature| round_to_i16(temperature * 100.0))
            };
            let Some(safe_voltage_mv) =
                boundary.safe_request_mv_or_cold_start(query, &mut project_temperature)
            else {
                return false;
            };
            request_ma <= boundary.source_max_ma
                && voltage_mv <= safe_voltage_mv
                && (boundary.source_power_limit_mw == 0
                    || u32::from(voltage_mv) * u32::from(request_ma) / 1_000
                        <= boundary.source_power_limit_mw)
        },
    )
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
    let ceiling_power_mw =
        (f32::from(ceiling_mv) * f32::from(ceiling_mv) / resistance_ohms / 1_000.0) as u32;
    // Saturation and the actuator share an integer-mW interface. Recognize
    // that exact endpoint before sqrt/floor, which would truncate it twice.
    if requested_power_mw >= ceiling_power_mw {
        return ceiling_mv;
    }
    let requested_mv = (requested_power_mw as f32 * 1_000.0 * resistance_ohms)
        .sqrt()
        .floor()
        .clamp(f32::from(floor_mv), f32::from(ceiling_mv)) as u16;
    floor_mv_to_100mv(requested_mv).clamp(floor_mv, ceiling_mv)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_warmup_request_mv(
    requested_power_mw: u32,
    floor_mv: u16,
    safe_ceiling_mv: u16,
) -> u16 {
    let floor_mv = floor_mv.clamp(CH224Q_ADJUSTABLE_REQUEST_MIN_MV, HEATER_ADJUSTABLE_MAX_MV);
    let safe_ceiling_mv = safe_ceiling_mv
        .max(floor_mv)
        .clamp(floor_mv, HEATER_ADJUSTABLE_MAX_MV);
    if requested_power_mw == 0 {
        floor_mv
    } else {
        safe_ceiling_mv
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_warmup_pwm_percent(
    requested_power_mw: u32,
    _warmup_soft_start_percent: u8,
) -> u8 {
    // PPS Warmup controls power with the requested voltage. Once the
    // contract is confirmed, the heater gate is either off for safety or
    // statically on for full conduction; warmup soft-start must not create a
    // PWM waveform on the PPS path.
    if requested_power_mw == 0 { 0 } else { 100 }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_step_request_mv(
    current_mv: u16,
    desired_mv: u16,
    floor_mv: u16,
    ceiling_mv: u16,
) -> u16 {
    let floor_mv = floor_mv.min(HEATER_ADJUSTABLE_MAX_MV);
    let ceiling_mv = ceiling_mv.max(floor_mv).min(HEATER_ADJUSTABLE_MAX_MV);
    let desired_mv = desired_mv.clamp(floor_mv, ceiling_mv);

    // A stale backend request can be outside the newly selected boundary. In
    // that case move directly into the safe interval; stepping from an
    // invalid request would create another invalid PD request.
    if current_mv < floor_mv || current_mv > ceiling_mv {
        return desired_mv;
    }
    if desired_mv > current_mv {
        current_mv
            .saturating_add(HEATER_PPS_REQUEST_STEP_MV)
            .min(desired_mv)
    } else {
        current_mv
            .saturating_sub(HEATER_PPS_REQUEST_STEP_MV)
            .max(desired_mv)
    }
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_heater_request_mv_for_phase(
    phase: HeaterControlPhase,
    current_mv: u16,
    desired_mv: u16,
    floor_mv: u16,
    ceiling_mv: u16,
) -> u16 {
    let floor_mv = floor_mv.min(HEATER_ADJUSTABLE_MAX_MV);
    let ceiling_mv = ceiling_mv.max(floor_mv).min(HEATER_ADJUSTABLE_MAX_MV);
    let desired_mv = desired_mv.clamp(floor_mv, ceiling_mv);
    if phase == HeaterControlPhase::Warmup {
        // PPS warm-up is the full-conduction measurement path. Request the
        // calibrated ceiling directly; stepping between adjacent voltages
        // would create needless contract churn and blank the heater.
        desired_mv
    } else if current_mv <= ceiling_mv && desired_mv < current_mv {
        // Once automatic heating has a safe confirmed PPS contract, lower
        // power belongs in PWM. Dropping the contract for every Approach/Hold
        // trim creates a needless settle window and can interrupt heating.
        current_mv
    } else {
        automatic_heater_step_request_mv(current_mv, desired_mv, floor_mv, ceiling_mv)
    }
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
    let duty = if requested_power_mw >= pon_mw as u32 {
        100
    } else {
        (requested_power_mw as f32 / pon_mw * 100.0)
            .floor()
            .clamp(0.0, 100.0) as u8
    };
    apply_warmup_soft_start(duty, warmup_soft_start_percent)
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_supply_response_fault_latched(
    previous: bool,
    normal_auto: bool,
    heater_enabled: bool,
    physical_duty_percent: u8,
    response_matches: bool,
) -> bool {
    normal_auto
        && heater_enabled
        && (previous || (physical_duty_percent == 100 && !response_matches))
}

#[cfg(any(target_arch = "xtensa", test))]
pub(crate) fn automatic_supply_response_sample_ready(
    observed_mv: u16,
    applied_mv: u16,
    next_request_at_ms: u64,
    transition_pending: bool,
    now_ms: u64,
) -> bool {
    observed_mv == applied_mv && !transition_pending && now_ms >= next_request_at_ms
}
