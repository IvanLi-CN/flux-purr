use flux_purr_pd_sink_core::{Mode, SelectedContract, SourceCapabilities, SourceObject, Target};

pub const CURRENT_CEILING_MA: u16 = 5_000;
pub const POWER_GATE_MV: u16 = 13_000;
pub const PPS_MIN_MV: u16 = 5_000;
pub const PPS_MAX_MV: u16 = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FanSelection {
    pub contract: SelectedContract,
    pub priority: u8,
    pub reason: SelectionReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionReason {
    PreferredFixed,
    PreferredPps,
    HighestFallbackFixed,
    HighestFallbackPps,
}

impl SelectionReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreferredFixed => "preferred_fixed",
            Self::PreferredPps => "preferred_pps",
            Self::HighestFallbackFixed => "highest_fixed_5_to_12v",
            Self::HighestFallbackPps => "highest_pps_fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionError {
    NoUsableFixedOrPps,
}

impl SelectionError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoUsableFixedOrPps => "no_usable_fixed_or_pps_candidate",
        }
    }
}

pub fn select_fan_contract(
    capabilities: SourceCapabilities,
) -> Result<FanSelection, SelectionError> {
    let preferred = [
        (Mode::Fixed, 15_000, SelectionReason::PreferredFixed),
        (Mode::Pps, 15_000, SelectionReason::PreferredPps),
        (Mode::Fixed, 18_000, SelectionReason::PreferredFixed),
        (Mode::Pps, 18_000, SelectionReason::PreferredPps),
        (Mode::Fixed, 20_000, SelectionReason::PreferredFixed),
        (Mode::Pps, 20_000, SelectionReason::PreferredPps),
    ];
    for (priority, (mode, voltage_mv, reason)) in preferred.into_iter().enumerate() {
        if let Some(contract) = find_candidate(capabilities, mode, voltage_mv) {
            return Ok(FanSelection {
                contract,
                priority: priority as u8,
                reason,
            });
        }
    }

    let mut fallback_fixed = None;
    for voltage_mv in [12_000, 11_000, 10_000, 9_000, 8_000, 7_000, 6_000, 5_000] {
        if let Some(contract) = find_candidate(capabilities, Mode::Fixed, voltage_mv) {
            fallback_fixed = Some(contract);
            break;
        }
    }
    if let Some(contract) = fallback_fixed {
        return Ok(FanSelection {
            contract,
            priority: 6,
            reason: SelectionReason::HighestFallbackFixed,
        });
    }

    let mut best_pps: Option<SelectedContract> = None;
    for index in 0..usize::from(capabilities.count).min(capabilities.objects.len()) {
        let object = capabilities.objects[index];
        if object.mode != Mode::Pps || object.max_ma < 50 {
            continue;
        }
        let voltage_mv = object.max_mv.min(PPS_MAX_MV);
        let voltage_mv = voltage_mv - (voltage_mv % 20);
        if voltage_mv < object.min_mv.max(PPS_MIN_MV) || voltage_mv < PPS_MIN_MV {
            continue;
        }
        let Some(contract) = make_candidate(object, Mode::Pps, voltage_mv) else {
            continue;
        };
        if match best_pps {
            Some(best) => contract.target.voltage_mv > best.target.voltage_mv,
            None => true,
        } {
            best_pps = Some(contract);
        }
    }
    best_pps
        .map(|contract| FanSelection {
            contract,
            priority: 7,
            reason: SelectionReason::HighestFallbackPps,
        })
        .ok_or(SelectionError::NoUsableFixedOrPps)
}

fn find_candidate(
    capabilities: SourceCapabilities,
    mode: Mode,
    voltage_mv: u16,
) -> Option<SelectedContract> {
    let count = usize::from(capabilities.count).min(capabilities.objects.len());
    for object in capabilities.objects[..count].iter().copied() {
        let voltage_matches = match mode {
            Mode::Fixed => object.mode == Mode::Fixed && object.min_mv == voltage_mv,
            Mode::Pps => {
                object.mode == Mode::Pps
                    && object.min_mv <= voltage_mv
                    && voltage_mv <= object.max_mv
            }
        };
        if voltage_matches && let Some(contract) = make_candidate(object, mode, voltage_mv) {
            return Some(contract);
        }
    }
    None
}

fn make_candidate(object: SourceObject, mode: Mode, voltage_mv: u16) -> Option<SelectedContract> {
    let capped_ma = object.max_ma.min(CURRENT_CEILING_MA);
    let current_ma = match mode {
        Mode::Fixed => capped_ma / 10 * 10,
        Mode::Pps => capped_ma / 50 * 50,
    };
    if current_ma == 0 {
        return None;
    }
    let target = Target { mode, voltage_mv };
    Some(SelectedContract {
        target,
        object,
        contract_current_ma: current_ma,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_purr_pd_sink_core::SourceCapabilities;

    fn fixed(position: u8, voltage_mv: u16, max_ma: u16) -> SourceObject {
        SourceObject {
            raw: 0,
            position,
            mode: Mode::Fixed,
            min_mv: voltage_mv,
            max_mv: voltage_mv,
            max_ma,
        }
    }

    fn pps(position: u8, min_mv: u16, max_mv: u16, max_ma: u16) -> SourceObject {
        SourceObject {
            raw: 0,
            position,
            mode: Mode::Pps,
            min_mv,
            max_mv,
            max_ma,
        }
    }

    fn caps(objects: &[SourceObject]) -> SourceCapabilities {
        let mut capabilities = SourceCapabilities::empty();
        capabilities.count = objects.len() as u8;
        capabilities.objects[..objects.len()].copy_from_slice(objects);
        capabilities
    }

    #[test]
    fn fixed_15v_precedes_pps_and_higher_targets() {
        let selection = select_fan_contract(caps(&[
            pps(1, 5_000, 20_000, 5_000),
            fixed(2, 15_000, 5_000),
            fixed(3, 20_000, 5_000),
        ]))
        .unwrap();
        assert_eq!(
            selection.contract.target,
            Target {
                mode: Mode::Fixed,
                voltage_mv: 15_000
            }
        );
        assert_eq!(selection.reason, SelectionReason::PreferredFixed);
    }

    #[test]
    fn pps_15v_precedes_fixed_18v() {
        let selection = select_fan_contract(caps(&[
            fixed(1, 18_000, 5_000),
            pps(2, 5_000, 15_000, 5_000),
        ]))
        .unwrap();
        assert_eq!(
            selection.contract.target,
            Target {
                mode: Mode::Pps,
                voltage_mv: 15_000
            }
        );
    }

    #[test]
    fn fallback_fixed_uses_highest_5_to_12v() {
        let selection =
            select_fan_contract(caps(&[fixed(1, 9_000, 5_000), fixed(2, 12_000, 1_500)])).unwrap();
        assert_eq!(selection.contract.target.voltage_mv, 12_000);
        assert_eq!(selection.reason, SelectionReason::HighestFallbackFixed);
    }

    #[test]
    fn fallback_pps_uses_highest_aligned_voltage_only_when_fixed_is_absent() {
        let selection = select_fan_contract(caps(&[pps(1, 5_000, 12_345, 4_000)])).unwrap();
        assert_eq!(selection.contract.target.voltage_mv, 12_340);
        assert_eq!(selection.reason, SelectionReason::HighestFallbackPps);
    }

    #[test]
    fn zero_or_unencodable_current_is_skipped() {
        let selection = select_fan_contract(caps(&[
            fixed(1, 15_000, 0),
            pps(2, 5_000, 20_000, 25),
            fixed(3, 12_000, 500),
        ]))
        .unwrap();
        assert_eq!(selection.contract.target.voltage_mv, 12_000);
    }

    #[test]
    fn current_is_rounded_down_to_the_wire_granularity() {
        let selection = select_fan_contract(caps(&[fixed(1, 15_000, 15)])).unwrap();
        assert_eq!(selection.contract.contract_current_ma, 10);
    }

    #[test]
    fn no_candidate_is_a_negotiation_failure() {
        assert_eq!(
            select_fan_contract(caps(&[fixed(1, 20_000, 0)])),
            Err(SelectionError::NoUsableFixedOrPps)
        );
    }
}
