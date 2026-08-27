//! FUSB302B sink-policy helpers.
//!
//! The FUSB302B physical layer is provided by the `fusb302` crate. This
//! module deliberately keeps Flux Purr's product-specific contract policy,
//! RDO encoding, and timing independent of that transport.

use super::pd::{Contract, ContractKind, SourceCapabilities};

const PD_HEADER_REQUEST: u16 = 2;
const PD_HEADER_GET_SOURCE_CAP: u16 = 7;
const PD_HEADER_SPEC_REV_30: u16 = 0b10 << 6;
const PPS_RDO_VOLTAGE_STEP_MV: u16 = 20;
const PPS_RDO_CURRENT_STEP_MA: u16 = 50;
const PPS_KEEPALIVE_INTERVAL_MS: u64 = 5_000;

pub const SOURCE_CAPS_INITIAL_WAIT_MS: u64 = 400;
pub const SOURCE_CAPS_RETRY_INTERVAL_MS: u64 = 5_000;
pub const SOURCE_CAPS_HARD_RESET_DELAY_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SinkPhase {
    #[default]
    Detached,
    WaitingForSourceCapabilities,
    WaitingForAccept,
    WaitingForPsRdy,
    Ready,
    Fault,
}

/// Sink-policy state held independently from every I2C transaction.
///
/// Callers exchange a single PHY frame, then run this policy without holding
/// the shared EEPROM/PD I2C bus across any PD timing interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SinkPolicy {
    phase: SinkPhase,
    requested_mv: u16,
    preferred_ma: u16,
    pending_contract: Contract,
    active_contract: Contract,
    source_capabilities: SourceCapabilities,
    source_capabilities_received: bool,
}

impl SinkPolicy {
    pub const fn new(requested_mv: u16, preferred_ma: u16) -> Self {
        Self {
            phase: SinkPhase::WaitingForSourceCapabilities,
            requested_mv,
            preferred_ma,
            pending_contract: Contract::none(),
            active_contract: Contract::none(),
            source_capabilities: SourceCapabilities::empty(),
            source_capabilities_received: false,
        }
    }

    pub const fn phase(self) -> SinkPhase {
        self.phase
    }

    pub const fn active_contract(self) -> Contract {
        self.active_contract
    }

    pub fn source_capabilities(self) -> Option<SourceCapabilities> {
        self.source_capabilities_received
            .then_some(self.source_capabilities)
    }

    /// Select a PPS contract, with fixed PDO fallback, from source capabilities.
    pub fn on_source_capabilities(&mut self, pdos: &[u32]) -> Option<[u8; 4]> {
        self.source_capabilities = SourceCapabilities::from_pdos(pdos);
        self.source_capabilities_received = true;
        self.begin_request(self.source_capabilities)
    }

    fn begin_request(&mut self, capabilities: SourceCapabilities) -> Option<[u8; 4]> {
        let contract =
            capabilities.select_fusb302b_contract(self.requested_mv, self.preferred_ma)?;
        let rdo = request_data_object(contract)?;
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    fn select_pps_contract(&self, requested_mv: u16) -> Option<Contract> {
        if !self.source_capabilities_received {
            return None;
        }
        let contract = self
            .source_capabilities
            .select_fusb302b_contract(requested_mv, self.preferred_ma)?;
        (contract.kind == ContractKind::Pps).then_some(contract)
    }

    /// Validate and retain a PPS target before the runtime refreshes Source
    /// Capabilities. Some sources require that refresh after a fixed-PDO
    /// contract before accepting a PPS request.
    pub fn prepare_pps_request(&mut self, requested_mv: u16) -> bool {
        if self.select_pps_contract(requested_mv).is_none() {
            return false;
        }
        self.requested_mv = requested_mv;
        true
    }

    pub fn request_pps_voltage(&mut self, requested_mv: u16) -> Option<[u8; 4]> {
        let contract = self.select_pps_contract(requested_mv)?;
        self.requested_mv = requested_mv;
        let rdo = request_data_object(contract)?;
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    /// Move a PPS session to an exact fixed PDO before releasing a terminal
    /// heater-disarm latch. This is deliberately separate from the normal
    /// selector, which prefers PPS whenever an APDO covers the target.
    pub fn request_fixed_voltage(&mut self, requested_mv: u16) -> Option<[u8; 4]> {
        if !self.source_capabilities_received {
            return None;
        }
        let contract = self
            .source_capabilities
            .select_fusb302b_fixed_contract(requested_mv, self.preferred_ma)?;
        let rdo = request_data_object(contract)?;
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    pub fn refresh_active_pps(&mut self) -> Option<[u8; 4]> {
        if self.active_contract.kind != ContractKind::Pps {
            return None;
        }
        self.pending_contract = self.active_contract;
        self.phase = SinkPhase::WaitingForAccept;
        request_data_object(self.pending_contract)
    }

    /// Abandon a request that did not reach `PS_RDY` without discarding a
    /// previously confirmed contract. The caller can use cached Source Caps to
    /// retry on a later PD service turn.
    pub fn cancel_pending_request(&mut self) {
        self.pending_contract = Contract::none();
        self.phase = if self.active_contract == Contract::none() {
            SinkPhase::WaitingForSourceCapabilities
        } else {
            SinkPhase::Ready
        };
    }

    /// Disarm after a request timeout. The runtime resets the PHY before it
    /// resumes negotiation, so delayed frames cannot install a contract after
    /// the timeout.
    pub fn timeout_pending_request(&mut self) {
        if matches!(
            self.phase,
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            self.mark_fault();
        }
    }

    /// `Accept` alone never arms heating; only `PS_RDY` installs a contract.
    pub fn on_control_message(&mut self, message_type: u8, now_ms: u64) {
        const ACCEPT: u8 = 3;
        const PS_RDY: u8 = 6;
        const REJECT: u8 = 4;
        const WAIT: u8 = 12;

        match (self.phase, message_type) {
            (SinkPhase::WaitingForAccept, ACCEPT) => self.phase = SinkPhase::WaitingForPsRdy,
            (SinkPhase::WaitingForPsRdy, PS_RDY) => {
                self.active_contract = self.pending_contract;
                self.pending_contract = Contract::none();
                self.phase = SinkPhase::Ready;
                let _ = now_ms;
            }
            (_, REJECT | WAIT) if self.active_contract != Contract::none() => {
                self.cancel_pending_request()
            }
            (_, REJECT | WAIT) => self.mark_fault(),
            _ => {}
        }
    }

    pub fn on_detach_or_reset(&mut self) {
        self.pending_contract = Contract::none();
        self.active_contract = Contract::none();
        self.source_capabilities_received = false;
        self.phase = SinkPhase::Detached;
    }

    pub fn mark_fault(&mut self) {
        self.pending_contract = Contract::none();
        self.active_contract = Contract::none();
        self.phase = SinkPhase::Fault;
    }
}

pub const fn request_header(message_id: u8) -> u16 {
    PD_HEADER_REQUEST | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9) | (1 << 12)
}

pub const fn get_source_capabilities_header(message_id: u8) -> u16 {
    PD_HEADER_GET_SOURCE_CAP | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9)
}

pub fn request_data_object(contract: Contract) -> Option<[u8; 4]> {
    if contract.object_position == 0 {
        return None;
    }
    let raw = match contract.kind {
        ContractKind::Pps => {
            let voltage_units = u32::from(contract.voltage_mv.div_ceil(PPS_RDO_VOLTAGE_STEP_MV));
            let current_units = u32::from(contract.current_ma.div_ceil(PPS_RDO_CURRENT_STEP_MA));
            ((contract.object_position as u32) << 28)
                | (1 << 24)
                | ((voltage_units & 0x0fff) << 9)
                | (current_units & 0x7f)
        }
        ContractKind::Fixed => {
            let current_units = (contract.current_ma / 10) as u32;
            ((contract.object_position as u32) << 28)
                | (1 << 24)
                | (current_units << 10)
                | current_units
        }
        ContractKind::None => return None,
    };
    Some(raw.to_le_bytes())
}

pub const fn pps_keepalive_due(last_request_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_request_at_ms) >= PPS_KEEPALIVE_INTERVAL_MS
}

pub const fn source_capabilities_retry_due(last_request_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_request_at_ms) >= SOURCE_CAPS_RETRY_INTERVAL_MS
}

pub const fn source_capabilities_hard_reset_due(last_request_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_request_at_ms) >= SOURCE_CAPS_HARD_RESET_DELAY_MS
}

/// Decode a complete Source_Capabilities data message from the public PHY packet view.
pub fn source_capabilities_from_message(header: u16, payload: &[u8]) -> Option<([u32; 7], usize)> {
    let message_type = (header & 0x1f) as u8;
    let count = usize::from((header >> 12) as u8 & 0x07);
    if message_type != 1 || count == 0 || payload.len() != count * 4 {
        return None;
    }

    let mut pdos = [0_u32; 7];
    for (index, pdo) in pdos.iter_mut().enumerate().take(count) {
        let offset = index * 4;
        *pdo = u32::from_le_bytes([
            payload[offset],
            payload[offset + 1],
            payload[offset + 2],
            payload[offset + 3],
        ]);
    }
    Some((pdos, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPS_APDO_5V_TO_21V_5A: u32 = 0xc1a4_3264;

    #[test]
    fn source_capabilities_packet_decodes_all_advertised_pdos() {
        let header = (2 << 12) | 1;
        let payload = [0x34, 0x12, 0, 0, 0x78, 0x56, 0, 0];
        assert_eq!(
            source_capabilities_from_message(header, &payload),
            Some(([0x1234, 0x5678, 0, 0, 0, 0, 0], 2))
        );
    }

    #[test]
    fn malformed_source_capabilities_packet_is_rejected() {
        assert_eq!(
            source_capabilities_from_message((2 << 12) | 1, &[0; 4]),
            None
        );
        assert_eq!(source_capabilities_from_message(1, &[]), None);
    }

    #[test]
    fn policy_arms_only_after_accept_then_ps_rdy() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        assert_eq!(policy.active_contract(), Contract::none());

        policy.on_control_message(3, 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);
        assert_eq!(policy.active_contract(), Contract::none());

        policy.on_control_message(6, 0);
        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
    }

    #[test]
    fn reset_clears_the_active_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        policy.on_detach_or_reset();
        assert_eq!(policy.phase(), SinkPhase::Detached);
        assert_eq!(policy.active_contract(), Contract::none());
        assert_eq!(policy.source_capabilities(), None);
    }

    #[test]
    fn pps_keepalive_interval_is_five_seconds() {
        assert!(!pps_keepalive_due(1_000, 5_999));
        assert!(pps_keepalive_due(1_000, 6_000));
    }

    #[test]
    fn source_capability_recovery_deadlines_are_preserved() {
        assert!(!source_capabilities_retry_due(1_000, 5_999));
        assert!(source_capabilities_retry_due(1_000, 6_000));
        assert!(!source_capabilities_hard_reset_due(1_000, 1_999));
        assert!(source_capabilities_hard_reset_due(1_000, 2_000));
    }

    #[test]
    fn pps_rdo_encodes_the_requested_contract() {
        let contract = Contract {
            kind: ContractKind::Pps,
            object_position: 2,
            voltage_mv: 20_000,
            current_ma: 5_000,
        };
        assert_eq!(
            request_data_object(contract),
            Some(0x2107_d064_u32.to_le_bytes())
        );
    }

    #[test]
    fn rejected_request_cannot_install_a_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(4, 0);
        assert_eq!(policy.phase(), SinkPhase::Fault);
        assert_eq!(policy.active_contract(), Contract::none());
    }

    #[test]
    fn pd30_headers_keep_message_id_and_object_count() {
        assert_eq!(request_header(5), 0x1a82);
        assert_eq!(get_source_capabilities_header(5), 0x0a87);
    }
}
