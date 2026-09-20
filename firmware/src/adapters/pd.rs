//! Controller-neutral USB-C PD contracts used by the heater policy.

/// Fixed PDO fallback is bounded by the standard 20 V supply range.
pub const FUSB302B_FIXED_MAX_MV: u16 = 20_000;
pub const FUSB302B_PD_ABSOLUTE_MIN_MV: u16 = 5_000;
pub const FUSB302B_PD_ABSOLUTE_MAX_MV: u16 = 28_000;
pub const FUSB302B_PPS_MIN_MV: u16 = 5_500;
pub const FUSB302B_PPS_MAX_MV: u16 = FUSB302B_PD_ABSOLUTE_MAX_MV;
pub const GUARANTEED_HEATER_MIN_MV: u16 = 20_000;
pub const MIN_HEATER_CONTRACT_MA: u16 = 3_000;
pub const MAX_HEATER_CONTRACT_MA: u16 = 5_000;
pub const MAX_SOURCE_PDOS: usize = 7;
pub const PD_FIXED_VOLTAGE_STEP_MV: u16 = 50;
pub const PD_FIXED_CURRENT_STEP_MA: u16 = 10;
pub const PD_PPS_VOLTAGE_STEP_MV: u16 = 100;
pub const PD_PPS_CURRENT_STEP_MA: u16 = 50;

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
    pub(crate) object_position: u8,
    pub voltage_mv: u16,
    pub current_ma: u16,
}

impl Contract {
    pub const fn observed(kind: ContractKind, voltage_mv: u16, current_ma: u16) -> Self {
        Self {
            kind,
            object_position: 0,
            voltage_mv,
            current_ma,
        }
    }

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
    pub(crate) object_position: u8,
    pub voltage_mv: u16,
    pub max_ma: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpsApdo {
    pub(crate) object_position: u8,
    pub min_mv: u16,
    pub max_mv: u16,
    pub max_ma: u16,
}

/// The semantic part of the APDO that callers need in order to decide
/// whether a later PPS adjustment is continuous. The protocol object position
/// remains private to the PD adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PpsAdjustmentRange {
    pub min_mv: u16,
    pub max_mv: u16,
    pub max_current_ma: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdContractRequestMode {
    Fixed,
    Pps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdContractRequest {
    pub mode: PdContractRequestMode,
    pub voltage_mv: u16,
    pub operating_current_ma: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdContractRequestError {
    ZeroVoltage,
    ZeroCurrent,
    FixedVoltageNotAligned,
    FixedCurrentNotAligned,
    PpsVoltageNotAligned,
    PpsCurrentNotAligned,
}

impl PdContractRequest {
    pub fn fixed(
        voltage_mv: u16,
        operating_current_ma: u16,
    ) -> Result<Self, PdContractRequestError> {
        if voltage_mv == 0 {
            return Err(PdContractRequestError::ZeroVoltage);
        }
        if operating_current_ma == 0 {
            return Err(PdContractRequestError::ZeroCurrent);
        }
        if !voltage_mv.is_multiple_of(PD_FIXED_VOLTAGE_STEP_MV) {
            return Err(PdContractRequestError::FixedVoltageNotAligned);
        }
        if !operating_current_ma.is_multiple_of(PD_FIXED_CURRENT_STEP_MA) {
            return Err(PdContractRequestError::FixedCurrentNotAligned);
        }
        Ok(Self {
            mode: PdContractRequestMode::Fixed,
            voltage_mv,
            operating_current_ma,
        })
    }

    pub fn pps(voltage_mv: u16, operating_current_ma: u16) -> Result<Self, PdContractRequestError> {
        if voltage_mv == 0 {
            return Err(PdContractRequestError::ZeroVoltage);
        }
        if operating_current_ma == 0 {
            return Err(PdContractRequestError::ZeroCurrent);
        }
        if !voltage_mv.is_multiple_of(PD_PPS_VOLTAGE_STEP_MV) {
            return Err(PdContractRequestError::PpsVoltageNotAligned);
        }
        if !operating_current_ma.is_multiple_of(PD_PPS_CURRENT_STEP_MA) {
            return Err(PdContractRequestError::PpsCurrentNotAligned);
        }
        Ok(Self {
            mode: PdContractRequestMode::Pps,
            voltage_mv,
            operating_current_ma,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfirmedActiveContract {
    pub mode: PdContractRequestMode,
    pub voltage_mv: u16,
    pub operating_current_ma: u16,
    pub pps_range: Option<PpsAdjustmentRange>,
}

impl ConfirmedActiveContract {
    pub fn from_private_contract(
        contract: Contract,
        capabilities: SourceCapabilities,
    ) -> Option<Self> {
        let mode = match contract.kind {
            ContractKind::Fixed => PdContractRequestMode::Fixed,
            ContractKind::Pps => PdContractRequestMode::Pps,
            ContractKind::None => return None,
        };
        let pps_range = if contract.kind == ContractKind::Pps {
            capabilities
                .pps
                .into_iter()
                .flatten()
                .find(|apdo| apdo.object_position == contract.object_position)
                .map(|apdo| PpsAdjustmentRange {
                    min_mv: apdo.min_mv,
                    max_mv: apdo.max_mv,
                    max_current_ma: apdo.max_ma,
                })
        } else {
            None
        };
        Some(Self {
            mode,
            voltage_mv: contract.voltage_mv,
            operating_current_ma: contract.current_ma,
            pps_range,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceCapabilitiesView {
    pub fixed: [Option<(u16, u16)>; MAX_SOURCE_PDOS],
    pub pps: [Option<PpsAdjustmentRange>; MAX_SOURCE_PDOS],
}

impl SourceCapabilities {
    pub fn view(self) -> SourceCapabilitiesView {
        let mut view = SourceCapabilitiesView::default();
        for (slot, pdo) in view.fixed.iter_mut().zip(self.fixed) {
            *slot = pdo.map(|pdo| (pdo.voltage_mv, pdo.max_ma));
        }
        for (slot, apdo) in view.pps.iter_mut().zip(self.pps) {
            *slot = apdo.map(|apdo| PpsAdjustmentRange {
                min_mv: apdo.min_mv,
                max_mv: apdo.max_mv,
                max_current_ma: apdo.max_ma,
            });
        }
        view
    }

    /// Select only an exact public request. No clamping, rounding, fallback,
    /// or mode substitution is allowed at this boundary.
    pub fn select_exact_contract(self, request: PdContractRequest) -> Option<Contract> {
        match request.mode {
            PdContractRequestMode::Fixed => self
                .fixed
                .into_iter()
                .flatten()
                .find(|pdo| {
                    pdo.voltage_mv == request.voltage_mv
                        && pdo.max_ma >= request.operating_current_ma
                })
                .map(|pdo| Contract {
                    kind: ContractKind::Fixed,
                    object_position: pdo.object_position,
                    voltage_mv: pdo.voltage_mv,
                    current_ma: request.operating_current_ma,
                }),
            PdContractRequestMode::Pps => self
                .pps
                .into_iter()
                .flatten()
                .find(|apdo| {
                    apdo.min_mv <= request.voltage_mv
                        && apdo.max_mv >= request.voltage_mv
                        && apdo.max_ma >= request.operating_current_ma
                })
                .map(|apdo| Contract {
                    kind: ContractKind::Pps,
                    object_position: apdo.object_position,
                    voltage_mv: request.voltage_mv,
                    current_ma: request.operating_current_ma,
                }),
        }
    }
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

    /// Prefer a PPS APDO covering the requested operating voltage. The
    /// request is constrained by the absolute PD guard, then by the live APDO.
    /// A fixed PDO is used only when the source offers no usable PPS APDO.
    /// Its voltage must not exceed the requested target, so automatic idle
    /// fallback cannot silently raise the VBUS above its configured floor.
    pub fn select_fusb302b_contract(
        self,
        requested_mv: u16,
        preferred_ma: u16,
    ) -> Option<Contract> {
        let requested_mv = requested_mv
            .clamp(FUSB302B_PD_ABSOLUTE_MIN_MV, FUSB302B_PD_ABSOLUTE_MAX_MV)
            .max(FUSB302B_PPS_MIN_MV);
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
            if pdo.voltage_mv < FUSB302B_PD_ABSOLUTE_MIN_MV
                || pdo.voltage_mv > FUSB302B_FIXED_MAX_MV
                || pdo.voltage_mv > requested_mv
                || pdo.max_ma < MIN_HEATER_CONTRACT_MA
            {
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
        if !(FUSB302B_PD_ABSOLUTE_MIN_MV..=FUSB302B_FIXED_MAX_MV).contains(&requested_mv) {
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

    /// Return whether a previously negotiated contract is still represented by
    /// the source capability object at the same position.
    pub fn supports_contract(self, contract: Contract) -> bool {
        match contract.kind {
            ContractKind::Fixed => self.fixed.into_iter().flatten().any(|pdo| {
                pdo.object_position == contract.object_position
                    && pdo.voltage_mv == contract.voltage_mv
                    && pdo.max_ma >= contract.current_ma
            }),
            ContractKind::Pps => self.pps.into_iter().flatten().any(|apdo| {
                apdo.object_position == contract.object_position
                    && apdo.min_mv <= contract.voltage_mv
                    && apdo.max_mv >= contract.voltage_mv
                    && apdo.max_ma >= contract.current_ma
            }),
            ContractKind::None => false,
        }
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
            if best.is_none_or(|current: PpsApdo| pps_capability_is_better(apdo, current)) {
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
    if candidate_ma != current_ma {
        return candidate_ma > current_ma;
    }
    if candidate.max_mv != current.max_mv {
        return candidate.max_mv > current.max_mv;
    }
    candidate.min_mv < current.min_mv
}

fn pps_capability_is_better(candidate: PpsApdo, current: PpsApdo) -> bool {
    if candidate.max_mv != current.max_mv {
        return candidate.max_mv > current.max_mv;
    }
    if candidate.max_ma != current.max_ma {
        return candidate.max_ma > current.max_ma;
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
    fn clamps_pps_requests_to_the_absolute_guard_then_the_live_apdo() {
        let mut capabilities = SourceCapabilities::empty();
        capabilities.pps[0] = Some(PpsApdo {
            object_position: 1,
            min_mv: 5_000,
            max_mv: 28_000,
            max_ma: 5_000,
        });

        let high = capabilities
            .select_fusb302b_contract(30_000, 5_000)
            .unwrap();
        let low = capabilities.select_fusb302b_contract(5_000, 5_000).unwrap();

        assert_eq!(high.kind, ContractKind::Pps);
        assert_eq!(high.voltage_mv, 28_000);
        assert_eq!(low.voltage_mv, 5_500);
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
    fn rejects_fixed_pdos_below_the_absolute_five_volt_floor() {
        let capabilities =
            SourceCapabilities::from_pdos(&[fixed_pdo(3_300, 5_000), fixed_pdo(5_000, 3_000)]);

        let normal = capabilities
            .select_fusb302b_contract(20_000, 5_000)
            .expect("the valid 5V fixed PDO should remain selectable");
        assert_eq!(normal.voltage_mv, 5_000);
        assert_eq!(
            SourceCapabilities::from_pdos(&[fixed_pdo(3_300, 5_000)])
                .select_fusb302b_contract(20_000, 5_000),
            None
        );
        assert_eq!(
            capabilities.select_fusb302b_fixed_contract(3_300, 5_000),
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
    fn twelve_volt_idle_fallback_never_selects_a_higher_fixed_pdo() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(5_000, 3_000),
            fixed_pdo(9_000, 3_000),
            fixed_pdo(12_000, 3_000),
            fixed_pdo(15_000, 3_000),
            fixed_pdo(20_000, 5_000),
        ]);

        let contract = capabilities
            .select_fusb302b_contract(12_000, 5_000)
            .unwrap();

        assert_eq!(contract.kind, ContractKind::Fixed);
        assert_eq!(contract.voltage_mv, 12_000);
        assert_eq!(contract.current_ma, 3_000);
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

    #[test]
    fn exact_requests_never_clamp_or_change_mode() {
        let capabilities = SourceCapabilities::from_pdos(&[
            fixed_pdo(20_000, 5_000),
            pps_pdo(5_000, 21_000, 5_000),
        ]);
        let pps = PdContractRequest::pps(20_000, 3_000).unwrap();
        let fixed = PdContractRequest::fixed(20_000, 3_000).unwrap();
        assert_eq!(
            capabilities.select_exact_contract(pps).unwrap().kind,
            ContractKind::Pps
        );
        assert_eq!(
            capabilities.select_exact_contract(fixed).unwrap().kind,
            ContractKind::Fixed
        );
        assert_eq!(
            capabilities.select_exact_contract(PdContractRequest::fixed(19_950, 3_000).unwrap()),
            None
        );
    }

    #[test]
    fn public_request_alignment_rejects_values_that_would_be_rounded() {
        assert_eq!(
            PdContractRequest::pps(20_020, 3_000),
            Err(PdContractRequestError::PpsVoltageNotAligned)
        );
        assert_eq!(
            PdContractRequest::pps(20_000, 3_025),
            Err(PdContractRequestError::PpsCurrentNotAligned)
        );
        assert_eq!(
            PdContractRequest::fixed(20_000, 3_005),
            Err(PdContractRequestError::FixedCurrentNotAligned)
        );
    }

    #[test]
    fn confirmed_pps_contract_exposes_range_without_object_position() {
        let capabilities = SourceCapabilities::from_pdos(&[pps_pdo(5_000, 21_000, 5_000)]);
        let private = capabilities
            .select_exact_contract(PdContractRequest::pps(20_000, 3_000).unwrap())
            .unwrap();
        let confirmed =
            ConfirmedActiveContract::from_private_contract(private, capabilities).unwrap();
        assert_eq!(
            confirmed.pps_range,
            Some(PpsAdjustmentRange {
                min_mv: 5_000,
                max_mv: 21_000,
                max_current_ma: 5_000,
            })
        );
    }
}
