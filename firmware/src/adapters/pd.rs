//! Controller-neutral USB-C PD contracts used by the heater policy.

/// Fixed PDO fallback is bounded by the standard 20 V supply range.
pub const FUSB302B_FIXED_MAX_MV: u16 = 20_000;
pub const FUSB302B_PPS_MIN_MV: u16 = 5_000;
pub const FUSB302B_PPS_MAX_MV: u16 = 21_000;
pub const GUARANTEED_HEATER_MIN_MV: u16 = 20_000;
pub const MIN_HEATER_CONTRACT_MA: u16 = 3_000;
pub const MAX_HEATER_CONTRACT_MA: u16 = 5_000;
pub const MAX_SOURCE_PDOS: usize = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerKind {
    Ch224q,
    Fusb302b,
    #[default]
    Unknown,
}

impl ControllerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ch224q => "ch224q",
            Self::Fusb302b => "fusb302b",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContractKind {
    Fixed,
    Pps,
    #[default]
    None,
}

impl ContractKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Pps => "pps",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DegradedReason {
    BelowGuaranteedVoltage,
    NoUsableContract,
}

impl DegradedReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BelowGuaranteedVoltage => "pd_contract_below_20v",
            Self::NoUsableContract => "pd_contract_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contract {
    pub kind: ContractKind,
    pub object_position: u8,
    pub voltage_mv: u16,
    pub current_ma: u16,
}

impl Contract {
    pub const fn none() -> Self {
        Self {
            kind: ContractKind::None,
            object_position: 0,
            voltage_mv: 0,
            current_ma: 0,
        }
    }

    pub const fn power_mw(self) -> u32 {
        (self.voltage_mv as u32 * self.current_ma as u32) / 1_000
    }

    pub const fn performance_guaranteed(self) -> bool {
        self.voltage_mv >= GUARANTEED_HEATER_MIN_MV && self.current_ma >= MIN_HEATER_CONTRACT_MA
    }

    pub fn degraded_reason(self) -> Option<DegradedReason> {
        if matches!(self.kind, ContractKind::None) {
            Some(DegradedReason::NoUsableContract)
        } else if self.performance_guaranteed() {
            None
        } else {
            Some(DegradedReason::BelowGuaranteedVoltage)
        }
    }

    pub fn clamp_heater_power_mw(self, requested_mw: u32) -> u32 {
        requested_mw.min(self.power_mw())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedPdo {
    pub object_position: u8,
    pub voltage_mv: u16,
    pub max_ma: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpsApdo {
    pub object_position: u8,
    pub min_mv: u16,
    pub max_mv: u16,
    pub max_ma: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceCapabilities {
    pub fixed: [Option<FixedPdo>; MAX_SOURCE_PDOS],
    pub pps: [Option<PpsApdo>; MAX_SOURCE_PDOS],
}

impl SourceCapabilities {
    pub const fn empty() -> Self {
        Self {
            fixed: [None; MAX_SOURCE_PDOS],
            pps: [None; MAX_SOURCE_PDOS],
        }
    }

    pub fn from_pdos(pdos: &[u32]) -> Self {
        let mut capabilities = Self::empty();
        for (index, raw) in pdos.iter().copied().enumerate().take(MAX_SOURCE_PDOS) {
            let object_position = (index + 1) as u8;
            match (raw >> 30) & 0b11 {
                0b00 => capabilities.push_fixed(FixedPdo {
                    object_position,
                    voltage_mv: (((raw >> 10) & 0x3ff) as u16) * 50,
                    max_ma: ((raw & 0x3ff) as u16) * 10,
                }),
                0b11 if ((raw >> 28) & 0b11) == 0 => capabilities.push_pps(PpsApdo {
                    object_position,
                    min_mv: (((raw >> 8) & 0xff) as u16) * 100,
                    max_mv: (((raw >> 17) & 0xff) as u16) * 100,
                    max_ma: ((raw & 0x7f) as u16) * 50,
                }),
                _ => {}
            }
        }
        capabilities
    }

    /// Prefer a PPS APDO covering the requested 5-21 V operating window. A
    /// fixed PDO is used only when the source offers no usable PPS APDO.
    pub fn select_fusb302b_contract(
        self,
        requested_mv: u16,
        preferred_ma: u16,
    ) -> Option<Contract> {
        let requested_mv = requested_mv.clamp(FUSB302B_PPS_MIN_MV, FUSB302B_PPS_MAX_MV);
        let requested_ma = preferred_ma.clamp(MIN_HEATER_CONTRACT_MA, MAX_HEATER_CONTRACT_MA);

        let mut best_pps: Option<(PpsApdo, Contract)> = None;
        for apdo in self.pps.into_iter().flatten() {
            if apdo.max_ma < MIN_HEATER_CONTRACT_MA
                || apdo.min_mv > requested_mv
                || apdo.max_mv < requested_mv
            {
                continue;
            }
            let candidate = Contract {
                kind: ContractKind::Pps,
                object_position: apdo.object_position,
                voltage_mv: requested_mv,
                current_ma: apdo.max_ma.min(requested_ma).min(MAX_HEATER_CONTRACT_MA),
            };
            if best_pps.is_none_or(|(current_apdo, current)| {
                pps_candidate_is_better(
                    apdo,
                    candidate.current_ma,
                    current_apdo,
                    current.current_ma,
                )
            }) {
                best_pps = Some((apdo, candidate));
            }
        }
        if let Some((_, contract)) = best_pps {
            return Some(contract);
        }

        let mut best_fixed = None;
        for pdo in self.fixed.into_iter().flatten() {
            if pdo.voltage_mv > FUSB302B_FIXED_MAX_MV || pdo.max_ma < MIN_HEATER_CONTRACT_MA {
                continue;
            }
            let candidate = Contract {
                kind: ContractKind::Fixed,
                object_position: pdo.object_position,
                voltage_mv: pdo.voltage_mv,
                current_ma: pdo.max_ma.min(requested_ma).min(MAX_HEATER_CONTRACT_MA),
            };
            if best_fixed.is_none_or(|current: Contract| {
                candidate.voltage_mv > current.voltage_mv
                    || (candidate.voltage_mv == current.voltage_mv
                        && candidate.current_ma > current.current_ma)
            }) {
                best_fixed = Some(candidate);
            }
        }
        best_fixed
    }

    /// Select an exact fixed PDO for a terminal PPS-disarm transition. Unlike
    /// the normal operating selector, this never substitutes a PPS APDO or a
    /// nearby fixed voltage.
    pub fn select_fusb302b_fixed_contract(
        self,
        requested_mv: u16,
        preferred_ma: u16,
    ) -> Option<Contract> {
        if requested_mv > FUSB302B_FIXED_MAX_MV {
            return None;
        }
        let requested_ma = preferred_ma.clamp(MIN_HEATER_CONTRACT_MA, MAX_HEATER_CONTRACT_MA);

        self.fixed
            .into_iter()
            .flatten()
            .filter(|pdo| pdo.voltage_mv == requested_mv && pdo.max_ma >= MIN_HEATER_CONTRACT_MA)
            .map(|pdo| Contract {
                kind: ContractKind::Fixed,
                object_position: pdo.object_position,
                voltage_mv: pdo.voltage_mv,
                current_ma: pdo.max_ma.min(requested_ma).min(MAX_HEATER_CONTRACT_MA),
            })
            .max_by_key(|contract| {
                (
                    contract.current_ma,
                    core::cmp::Reverse(contract.object_position),
                )
            })
    }

    pub fn fusb302b_pps_capability(self) -> Option<PpsApdo> {
        let mut best = None;
        for apdo in self.pps.into_iter().flatten() {
            if apdo.max_ma < MIN_HEATER_CONTRACT_MA
                || apdo.min_mv > GUARANTEED_HEATER_MIN_MV
                || apdo.max_mv < GUARANTEED_HEATER_MIN_MV
            {
                continue;
            }
            if best.is_none_or(|current: PpsApdo| {
                pps_candidate_is_better(apdo, apdo.max_ma, current, current.max_ma)
            }) {
                best = Some(apdo);
            }
        }
        best
    }

    fn push_fixed(&mut self, pdo: FixedPdo) {
        if pdo.voltage_mv == 0 || pdo.max_ma == 0 {
            return;
        }
        if let Some(slot) = self.fixed.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(pdo);
        }
    }

    fn push_pps(&mut self, apdo: PpsApdo) {
        if apdo.min_mv == 0 || apdo.max_mv < apdo.min_mv || apdo.max_ma == 0 {
            return;
        }
        if let Some(slot) = self.pps.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(apdo);
        }
    }
}

fn pps_candidate_is_better(
    candidate: PpsApdo,
    candidate_ma: u16,
    current: PpsApdo,
    current_ma: u16,
) -> bool {
    let candidate_covers_full_window =
        candidate.min_mv <= FUSB302B_PPS_MIN_MV && candidate.max_mv >= FUSB302B_PPS_MAX_MV;
    let current_covers_full_window =
        current.min_mv <= FUSB302B_PPS_MIN_MV && current.max_mv >= FUSB302B_PPS_MAX_MV;
    if candidate_covers_full_window != current_covers_full_window {
        return candidate_covers_full_window;
    }
    if candidate_ma != current_ma {
        return candidate_ma > current_ma;
    }
    if candidate.max_mv != current.max_mv {
        return candidate.max_mv > current.max_mv;
    }
    candidate.min_mv < current.min_mv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pps_pdo(min_mv: u16, max_mv: u16, max_ma: u16) -> u32 {
        (0b11 << 30)
            | (((min_mv / 100) as u32) << 8)
            | (((max_mv / 100) as u32) << 17)
            | ((max_ma / 50) as u32)
    }

    fn fixed_pdo(mv: u16, ma: u16) -> u32 {
        ((mv / 50) as u32) << 10 | ((ma / 10) as u32)
    }

    #[test]
    fn prefers_a_full_range_pps_apdo() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(20_000, 3_000),
            pps_pdo(5_000, 21_000, 5_000),
        ]);
        let contract = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Pps);
        assert_eq!(contract.voltage_mv, 20_000);
        assert_eq!(contract.current_ma, 5_000);
        assert_eq!(contract.power_mw(), 100_000);
        assert!(contract.performance_guaranteed());
    }

    #[test]
    fn caps_a_pps_three_amp_contract_to_sixty_watts() {
        let capabilities = SourceCapabilities::from_pdos(&[pps_pdo(5_000, 21_000, 3_000)]);
        let contract = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Pps);
        assert_eq!(contract.voltage_mv, 20_000);
        assert_eq!(contract.current_ma, 3_000);
        assert_eq!(contract.power_mw(), 60_000);
    }

    #[test]
    fn uses_the_requested_pps_voltage_inside_the_apdo() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(20_000, 3_000),
            pps_pdo(5_000, 21_000, 5_000),
        ]);
        let contract = capabilities
            .select_fusb302b_contract(12_400, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Pps);
        assert_eq!(contract.voltage_mv, 12_400);
    }

    #[test]
    fn falls_back_to_fixed_when_no_pps_apdo_is_usable() {
        let capabilities = SourceCapabilities::from_pdos(&[fixed_pdo(20_000, 3_000)]);
        let contract = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Fixed);
        assert_eq!(contract.current_ma, 3_000);
        assert_eq!(contract.power_mw(), 60_000);
        assert_eq!(contract.clamp_heater_power_mw(100_000), 60_000);
    }

    #[test]
    fn selects_only_the_requested_fixed_pdo_for_a_terminal_transition() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(9_000, 3_000),
            fixed_pdo(20_000, 5_000),
            pps_pdo(5_000, 21_000, 5_000),
        ]);

        let contract = capabilities
            .select_fusb302b_fixed_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Fixed);
        assert_eq!(contract.object_position, 2);
        assert_eq!(contract.voltage_mv, 20_000);
        assert_eq!(contract.current_ma, 5_000);
        assert_eq!(
            capabilities.select_fusb302b_fixed_contract(15_000, 5_000),
            None
        );
    }

    #[test]
    fn falls_back_to_the_best_safe_fixed_pdo_below_twenty_volts() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(5_000, 3_000),
            fixed_pdo(9_000, 3_000),
            fixed_pdo(15_000, 3_000),
        ]);
        let contract = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Fixed);
        assert_eq!(contract.voltage_mv, 15_000);
        assert!(!contract.performance_guaranteed());
        assert_eq!(
            contract.degraded_reason(),
            Some(DegradedReason::BelowGuaranteedVoltage)
        );
    }

    #[test]
    fn rejects_contracts_below_three_amps() {
        let capabilities = SourceCapabilities::from_pdos(&[fixed_pdo(20_000, 2_500)]);
        assert_eq!(capabilities.select_fusb302b_contract(20_000, 5_000), None);
    }

    #[test]
    fn selects_the_lower_object_position_when_fixed_pdos_tie() {
        let capabilities =
            SourceCapabilities::from_pdos(&[fixed_pdo(20_000, 5_000), fixed_pdo(20_000, 5_000)]);
        let contract = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .unwrap();

        assert_eq!(contract.object_position, 1);
    }

    #[test]
    fn exposes_the_best_full_range_pps_capability() {
        let capabilities = SourceCapabilities::from_pdos(&[
            pps_pdo(5_000, 20_000, 5_000),
            pps_pdo(5_000, 21_000, 3_000),
        ]);
        let capability = capabilities.fusb302b_pps_capability().unwrap();

        assert_eq!(capability.object_position, 2);
        assert_eq!(capability.min_mv, 5_000);
        assert_eq!(capability.max_mv, 21_000);
        assert_eq!(capability.max_ma, 3_000);
    }
}
