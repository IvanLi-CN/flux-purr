//! FUSB302B sink-policy helpers.
//!
//! The FUSB302B physical layer is provided by the `fusb302` crate. This
//! module deliberately keeps Flux Purr's product-specific contract policy,
//! RDO encoding, and timing independent of that transport.

use super::pd::{Contract, ContractKind, SourceCapabilities};

const PD_HEADER_REQUEST: u16 = 2;
const PD_HEADER_ACCEPT: u16 = 3;
const PD_HEADER_GET_SOURCE_CAP: u16 = 7;
// The Flux Purr target uses the FUSB302BMPX-compatible PD 3.0 path so PPS
// APDOs are advertised and accepted by the connected source. Keep all
// locally initiated headers on the same revision as automatic GoodCRC.
const PD_HEADER_SPEC_REV_30: u16 = 0b10 << 6;
const PPS_RDO_VOLTAGE_STEP_MV: u16 = 20;
const PPS_RDO_CURRENT_STEP_MA: u16 = 50;
const PPS_KEEPALIVE_INTERVAL_MS: u64 = 5_000;

pub const SOURCE_CAPS_INITIAL_WAIT_MS: u64 = 400;
pub const SOURCE_CAPS_RETRY_INTERVAL_MS: u64 = 5_000;

fn source_message_id_is_newer(last: u8, current: u8) -> bool {
    let distance = current.wrapping_sub(last) & 0x07;
    (1..=4).contains(&distance)
}

/// The only recovery actions available after a Source_Capabilities timeout.
///
/// A sink powered by the same VBUS must not initiate a PD reset merely because
/// a source-capability response is late: either reset can disturb the source
/// session that powers the sink itself. Keep the heater interlocked and retry
/// `Get_Source_Capabilities` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCapabilitiesRecovery {
    Wait,
    RetryGetSourceCapabilities,
}

/// A local transport failure that does not prove a CC detach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransientTransportFault {
    RetryFailed,
    PendingRequestTimeout,
    PartialReceiveTimeout,
    ReceiveIoError,
    TransmitIoError,
    ConfigurationIoError,
}

/// The recovery action for a local transport failure.
///
/// Reinitializing the PHY can withdraw Rd long enough for the source that
/// powers this sink to remove VBUS. These faults must instead preserve CC,
/// interlock heating, flush the incomplete transaction, and re-query source
/// capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransientTransportRecovery {
    FlushReceiveAndRequery,
}

pub const fn transient_transport_fault_recovery(
    _fault: TransientTransportFault,
) -> TransientTransportRecovery {
    TransientTransportRecovery::FlushReceiveAndRequery
}

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
    default_requested_mv: u16,
    requested_mv: u16,
    preferred_ma: u16,
    requested_mode: Option<crate::adapters::pd::PdContractRequestMode>,
    pending_contract: Contract,
    active_contract: Contract,
    source_capabilities: SourceCapabilities,
    source_capabilities_received: bool,
    source_message_id: Option<u8>,
}

impl SinkPolicy {
    pub const fn new(requested_mv: u16, preferred_ma: u16) -> Self {
        Self {
            phase: SinkPhase::WaitingForSourceCapabilities,
            default_requested_mv: requested_mv,
            requested_mv,
            preferred_ma,
            requested_mode: None,
            pending_contract: Contract::none(),
            active_contract: Contract::none(),
            source_capabilities: SourceCapabilities::empty(),
            source_capabilities_received: false,
            source_message_id: None,
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

    /// Build a request from the public power-domain contract. The adapter is
    /// the only layer allowed to resolve a private PDO/APDO object position.
    /// Unsupported mode, voltage, or current is rejected without substitution.
    pub fn request_contract(
        &mut self,
        request: crate::adapters::pd::PdContractRequest,
    ) -> Option<[u8; 4]> {
        if !self.source_capabilities_received {
            return None;
        }
        let contract = self.source_capabilities.select_exact_contract(request)?;
        let rdo = request_data_object(contract)?;
        self.requested_mv = request.voltage_mv;
        self.preferred_ma = request.operating_current_ma;
        self.requested_mode = Some(request.mode);
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    pub fn pending_contract_matches(
        &self,
        request: crate::adapters::pd::PdContractRequest,
    ) -> bool {
        self.source_capabilities
            .select_exact_contract(request)
            .is_some_and(|contract| contract == self.pending_contract)
    }

    pub fn pending_automatic_idle_contract_matches(&self) -> bool {
        self.source_capabilities_received
            && self
                .source_capabilities
                .select_fusb302b_contract(self.default_requested_mv, self.preferred_ma)
                .is_some_and(|contract| contract == self.pending_contract)
    }

    /// Retain an exact PPS request while a Fixed-to-PPS transition refreshes
    /// Source_Capabilities. The follow-up request is emitted by the service
    /// after the refreshed capabilities exchange completes.
    pub fn prepare_contract_refresh(
        &mut self,
        request: crate::adapters::pd::PdContractRequest,
    ) -> bool {
        self.source_capabilities_received
            && request.mode == crate::adapters::pd::PdContractRequestMode::Pps
            && {
                self.requested_mv = request.voltage_mv;
                self.preferred_ma = request.operating_current_ma;
                self.requested_mode = Some(request.mode);
                true
            }
    }

    pub fn confirmed_active_contract(self) -> Option<crate::adapters::pd::ConfirmedActiveContract> {
        crate::adapters::pd::ConfirmedActiveContract::from_private_contract(
            self.active_contract,
            self.source_capabilities,
        )
    }

    /// Select a PPS contract, with fixed PDO fallback, from source capabilities.
    pub fn on_source_capabilities(&mut self, pdos: &[u32]) -> Option<[u8; 4]> {
        self.on_source_capabilities_with_message_id(pdos, None)
    }

    /// Select a contract and retain the latest Source message ID. The runtime
    /// uses it to ignore duplicate responses from an abandoned exchange while
    /// allowing legitimate intervening Source messages to advance the ID by
    /// more than one.
    pub fn on_source_capabilities_with_message_id(
        &mut self,
        pdos: &[u32],
        source_message_id: Option<u8>,
    ) -> Option<[u8; 4]> {
        self.source_capabilities = SourceCapabilities::from_pdos(pdos);
        self.source_capabilities_received = true;
        self.source_message_id = source_message_id.map(|value| value & 0x07);
        self.begin_request(self.source_capabilities)
    }

    /// Refresh cached capabilities without needlessly renegotiating a still
    /// valid explicit contract. If the source no longer advertises that
    /// contract, clear it and begin a bounded replacement request instead.
    pub fn refresh_source_capabilities_with_message_id(
        &mut self,
        pdos: &[u32],
        source_message_id: Option<u8>,
    ) -> Option<[u8; 4]> {
        self.source_capabilities = SourceCapabilities::from_pdos(pdos);
        self.source_capabilities_received = true;
        self.source_message_id = source_message_id.map(|value| value & 0x07);

        if self.phase == SinkPhase::Ready && self.active_contract != Contract::none() {
            if self
                .source_capabilities
                .supports_contract(self.active_contract)
            {
                self.pending_contract = Contract::none();
                return None;
            }
            self.active_contract = Contract::none();
            self.pending_contract = Contract::none();
            self.phase = SinkPhase::WaitingForSourceCapabilities;
        }

        self.begin_request(self.source_capabilities)
    }

    fn begin_request(&mut self, capabilities: SourceCapabilities) -> Option<[u8; 4]> {
        let contract = match self.requested_mode {
            Some(crate::adapters::pd::PdContractRequestMode::Pps) => {
                let request = crate::adapters::pd::PdContractRequest::pps(
                    self.requested_mv,
                    self.preferred_ma,
                )
                .ok()?;
                capabilities.select_exact_contract(request)?
            }
            Some(crate::adapters::pd::PdContractRequestMode::Fixed) => {
                let request = crate::adapters::pd::PdContractRequest::fixed(
                    self.requested_mv,
                    self.preferred_ma,
                )
                .ok()?;
                capabilities.select_exact_contract(request)?
            }
            None => capabilities.select_fusb302b_contract(self.requested_mv, self.preferred_ma)?,
        };
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
        self.requested_mode = Some(crate::adapters::pd::PdContractRequestMode::Pps);
        true
    }

    pub fn request_pps_voltage(&mut self, requested_mv: u16) -> Option<[u8; 4]> {
        let contract = self.select_pps_contract(requested_mv)?;
        self.requested_mv = requested_mv;
        self.requested_mode = Some(crate::adapters::pd::PdContractRequestMode::Pps);
        let rdo = request_data_object(contract)?;
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    /// Restore the startup policy after a manual or thermal override ends.
    /// This prefers the configured PPS idle voltage and uses the bounded fixed
    /// PDO fallback only when the live capabilities provide no usable APDO.
    pub fn request_automatic_idle_contract(&mut self) -> Option<[u8; 4]> {
        if !self.source_capabilities_received {
            return None;
        }
        self.requested_mv = self.default_requested_mv;
        self.requested_mode = None;
        self.begin_request(self.source_capabilities)
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
        self.requested_mode = Some(crate::adapters::pd::PdContractRequestMode::Fixed);
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

    /// Make a local transport failure a heater-only interlock without
    /// withdrawing CC. The caller flushes the PHY receive FIFO and later
    /// re-queries the source before a new contract can authorize heat.
    pub fn interlock_after_transient_transport_fault(&mut self) {
        self.on_received_protocol_reset();
    }

    /// A received PD reset clears contract authorization but preserves the
    /// Type-C attachment. The physical CC relationship belongs to the PHY and
    /// must not be reconstructed by the policy engine.
    pub fn on_received_protocol_reset(&mut self) {
        self.pending_contract = Contract::none();
        self.active_contract = Contract::none();
        self.source_message_id = None;
        self.requested_mv = self.default_requested_mv;
        self.requested_mode = None;
        self.source_capabilities = SourceCapabilities::empty();
        self.source_capabilities_received = false;
        self.phase = SinkPhase::WaitingForSourceCapabilities;
    }

    /// `Accept` alone never arms heating; only `PS_RDY` installs a contract.
    pub fn on_control_message(&mut self, message_type: u8, now_ms: u64) {
        self.on_control_message_with_message_id(message_type, None, now_ms);
    }

    /// Process a control response and reject only a duplicate Source message.
    /// Source message IDs are monotonic modulo eight, but a Source may emit
    /// other valid messages between the capability advertisement and its
    /// response, so requiring an exact +1 ID loses valid negotiations.
    pub fn on_control_message_with_message_id(
        &mut self,
        message_type: u8,
        message_id: Option<u8>,
        now_ms: u64,
    ) {
        const ACCEPT: u8 = 3;
        const PS_RDY: u8 = 6;
        const REJECT: u8 = 4;
        const WAIT: u8 = 12;

        let message_id = message_id.map(|value| value & 0x07);
        let response_id_is_fresh = message_id.is_none_or(|current| {
            self.source_message_id
                .is_none_or(|last| source_message_id_is_newer(last, current))
        });

        if !response_id_is_fresh {
            return;
        }

        match (self.phase, message_type) {
            (SinkPhase::WaitingForAccept, ACCEPT) => {
                if let Some(message_id) = message_id {
                    self.source_message_id = Some(message_id);
                }
                self.phase = SinkPhase::WaitingForPsRdy;
            }
            (SinkPhase::WaitingForPsRdy, PS_RDY) => {
                self.active_contract = self.pending_contract;
                self.pending_contract = Contract::none();
                if let Some(message_id) = message_id {
                    self.source_message_id = Some(message_id);
                }
                self.phase = SinkPhase::Ready;
                let _ = now_ms;
            }
            // A source may reject or defer the first request while it is
            // still advertising a usable contract. Keep the attachment alive
            // and let the normal Source_Capabilities retry schedule recover.
            (_, REJECT | WAIT) => self.cancel_pending_request(),
            _ => {}
        }
    }

    pub fn on_detach_or_reset(&mut self) {
        self.pending_contract = Contract::none();
        self.active_contract = Contract::none();
        self.source_message_id = None;
        self.requested_mv = self.default_requested_mv;
        self.requested_mode = None;
        self.source_capabilities = SourceCapabilities::empty();
        self.source_capabilities_received = false;
        self.phase = SinkPhase::Detached;
    }

    /// Re-enter bounded Source_Capabilities discovery after the PHY reports a
    /// fresh Type-C attachment. The PHY owns CC detection; this transition
    /// only re-arms the policy's protocol-side discovery state.
    pub fn on_attachment_detected(&mut self) {
        if self.phase == SinkPhase::Detached {
            self.phase = SinkPhase::WaitingForSourceCapabilities;
        }
    }

    pub fn mark_fault(&mut self) {
        self.pending_contract = Contract::none();
        self.active_contract = Contract::none();
        self.source_message_id = None;
        self.phase = SinkPhase::Fault;
    }
}

pub const fn request_header(message_id: u8) -> u16 {
    PD_HEADER_REQUEST | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9) | (1 << 12)
}

pub const fn accept_header(message_id: u8) -> u16 {
    PD_HEADER_ACCEPT | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9)
}

pub const fn get_source_capabilities_header(message_id: u8) -> u16 {
    PD_HEADER_GET_SOURCE_CAP | PD_HEADER_SPEC_REV_30 | (((message_id & 0x07) as u16) << 9)
}

pub(crate) fn request_data_object(contract: Contract) -> Option<[u8; 4]> {
    if contract.object_position == 0 {
        return None;
    }
    let raw = match contract.kind {
        ContractKind::Pps => {
            if !contract.voltage_mv.is_multiple_of(PPS_RDO_VOLTAGE_STEP_MV)
                || !contract.current_ma.is_multiple_of(PPS_RDO_CURRENT_STEP_MA)
            {
                return None;
            }
            let voltage_units = u32::from(contract.voltage_mv / PPS_RDO_VOLTAGE_STEP_MV);
            let current_units = u32::from(contract.current_ma / PPS_RDO_CURRENT_STEP_MA);
            ((contract.object_position as u32) << 28)
                | (1 << 24)
                | ((voltage_units & 0x0fff) << 9)
                | (current_units & 0x7f)
        }
        ContractKind::Fixed => {
            if !contract.current_ma.is_multiple_of(10) {
                return None;
            }
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

/// Returns whether the sink should request Source_Capabilities now.
///
/// A newly attached source gets its normal advertisement window first. Once a
/// request has been sent, recovery remains a bounded re-query and never
/// escalates to a sink-initiated reset.
pub const fn source_capabilities_request_due(
    attached_at_ms: u64,
    last_request_at_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    match last_request_at_ms {
        Some(last_request_at_ms) => source_capabilities_retry_due(last_request_at_ms, now_ms),
        None => now_ms.saturating_sub(attached_at_ms) >= SOURCE_CAPS_INITIAL_WAIT_MS,
    }
}

pub const fn source_capabilities_recovery(
    last_request_at_ms: u64,
    now_ms: u64,
) -> SourceCapabilitiesRecovery {
    if source_capabilities_retry_due(last_request_at_ms, now_ms) {
        SourceCapabilitiesRecovery::RetryGetSourceCapabilities
    } else {
        SourceCapabilitiesRecovery::Wait
    }
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
    fn policy_arms_an_idle_twelve_volt_pps_contract_after_accept_then_ps_rdy() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);

        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
        assert_eq!(policy.active_contract().voltage_mv, 12_000);
    }

    #[test]
    fn fixed_only_discovery_allows_a_pps_capability_refresh() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        let fixed_only = [((5_000_u32 / 50) << 10) | (3_000_u32 / 10)];
        let _ = policy.on_source_capabilities(&fixed_only);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Fixed);

        let request = crate::adapters::pd::PdContractRequest::pps(17_500, 3_000).unwrap();
        assert!(policy.prepare_contract_refresh(request));
        assert_eq!(policy.requested_mv, 17_500);
        assert_eq!(policy.preferred_ma, 3_000);
    }

    #[test]
    fn explicit_pps_refresh_never_falls_back_to_fixed() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        let fixed_only = [((5_000_u32 / 50) << 10) | (3_000_u32 / 10)];
        let _ = policy.on_source_capabilities(&fixed_only);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        let request = crate::adapters::pd::PdContractRequest::pps(17_500, 3_000).unwrap();
        assert!(policy.prepare_contract_refresh(request));

        assert_eq!(policy.on_source_capabilities(&fixed_only), None);
        assert_eq!(policy.active_contract().kind, ContractKind::Fixed);
        assert_eq!(policy.phase(), SinkPhase::Ready);
    }

    #[test]
    fn automatic_idle_restore_returns_a_twenty_volt_override_to_twelve_volt_pps() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        let source = [
            ((20_000_u32 / 50) << 10) | (5_000_u32 / 10),
            PPS_APDO_5V_TO_21V_5A,
        ];
        let _ = policy.on_source_capabilities(&source);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().voltage_mv, 12_000);

        let _ = policy.request_pps_voltage(20_000);
        policy.on_control_message(3, 1);
        policy.on_control_message(6, 1);
        assert_eq!(policy.active_contract().voltage_mv, 20_000);

        assert!(policy.request_automatic_idle_contract().is_some());
        assert_eq!(policy.pending_contract.kind, ContractKind::Pps);
        assert_eq!(policy.pending_contract.voltage_mv, 12_000);
    }

    #[test]
    fn automatic_idle_replaces_a_superseded_pending_pps_contract() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        let _ = policy.request_pps_voltage(20_000);
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        assert_eq!(policy.pending_contract.voltage_mv, 20_000);
        assert!(!policy.pending_automatic_idle_contract_matches());

        assert!(policy.request_automatic_idle_contract().is_some());
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        assert_eq!(policy.pending_contract.voltage_mv, 12_000);
        assert!(policy.pending_automatic_idle_contract_matches());
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
    fn fresh_attachment_reenters_source_capability_discovery() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        policy.on_detach_or_reset();
        assert_eq!(policy.phase(), SinkPhase::Detached);

        policy.on_attachment_detected();

        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
    }

    #[test]
    fn received_protocol_reset_interlocks_without_detaching_cc() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);

        policy.on_received_protocol_reset();

        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
        assert_eq!(policy.active_contract(), Contract::none());
        assert_eq!(policy.source_capabilities(), None);
    }

    #[test]
    fn unusable_source_capabilities_keep_discovery_alive() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        assert_eq!(policy.on_source_capabilities(&[0x0000_0000]), None);
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
        assert_eq!(policy.active_contract(), Contract::none());
        assert!(policy.source_capabilities().is_some());
    }

    #[test]
    fn pps_keepalive_interval_is_five_seconds() {
        assert!(!pps_keepalive_due(1_000, 5_999));
        assert!(pps_keepalive_due(1_000, 6_000));
    }

    #[test]
    fn missing_source_capabilities_only_retry_without_resetting_the_source() {
        assert_eq!(
            source_capabilities_recovery(1_000, 1_999),
            SourceCapabilitiesRecovery::Wait
        );
        assert_eq!(
            source_capabilities_recovery(1_000, 6_000),
            SourceCapabilitiesRecovery::RetryGetSourceCapabilities
        );
    }

    #[test]
    fn source_capabilities_request_schedule_waits_then_retries() {
        assert!(!source_capabilities_request_due(1_000, None, 1_399));
        assert!(source_capabilities_request_due(1_000, None, 1_400));
        assert!(!source_capabilities_request_due(1_000, Some(1_400), 6_399));
        assert!(source_capabilities_request_due(1_000, Some(1_400), 6_400));
    }

    #[test]
    fn unanswered_startup_source_caps_never_resets_the_powering_source() {
        assert_eq!(
            source_capabilities_recovery(1_400, 2_399),
            SourceCapabilitiesRecovery::Wait
        );
        assert_eq!(
            source_capabilities_recovery(1_400, 2_400),
            SourceCapabilitiesRecovery::Wait
        );
        assert_eq!(
            source_capabilities_recovery(1_400, 6_400),
            SourceCapabilitiesRecovery::RetryGetSourceCapabilities
        );
    }

    #[test]
    fn missing_source_capabilities_only_waits_or_requeries_without_resetting_source() {
        assert!(
            matches!(
                source_capabilities_recovery(1_000, 2_000),
                SourceCapabilitiesRecovery::Wait
            ),
            "a missing Source_Capabilities response must not make the sink reset the source session"
        );
        assert!(matches!(
            source_capabilities_recovery(1_000, 6_000),
            SourceCapabilitiesRecovery::RetryGetSourceCapabilities
        ));
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
    fn rdo_encoding_rejects_unaligned_values_instead_of_rounding_up() {
        let contract = Contract {
            kind: ContractKind::Pps,
            object_position: 1,
            voltage_mv: 20_010,
            current_ma: 3_000,
        };
        assert_eq!(request_data_object(contract), None);
        let fixed = Contract {
            kind: ContractKind::Fixed,
            object_position: 1,
            voltage_mv: 12_000,
            current_ma: 3_005,
        };
        assert_eq!(request_data_object(fixed), None);
    }

    #[test]
    fn transient_transport_faults_interlock_without_requesting_a_phy_reset() {
        let mut policy = SinkPolicy::new(12_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.phase(), SinkPhase::Ready);

        for fault in [
            TransientTransportFault::RetryFailed,
            TransientTransportFault::PendingRequestTimeout,
            TransientTransportFault::PartialReceiveTimeout,
            TransientTransportFault::ReceiveIoError,
            TransientTransportFault::TransmitIoError,
            TransientTransportFault::ConfigurationIoError,
        ] {
            assert_eq!(
                transient_transport_fault_recovery(fault),
                TransientTransportRecovery::FlushReceiveAndRequery
            );
        }

        policy.interlock_after_transient_transport_fault();
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
        assert_eq!(policy.active_contract(), Contract::none());
        assert_eq!(policy.source_capabilities(), None);
        assert!(!policy.prepare_pps_request(12_000));
        assert_eq!(policy.request_pps_voltage(12_000), None);
        assert_eq!(policy.request_fixed_voltage(20_000), None);

        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
    }

    #[test]
    fn transient_transport_fault_discards_manual_pps_target_before_rediscovery() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert!(policy.request_pps_voltage(12_000).is_some());
        assert_eq!(policy.requested_mv, 12_000);

        policy.interlock_after_transient_transport_fault();

        assert_eq!(policy.requested_mv, 20_000);
        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
        assert_eq!(policy.requested_mv, 20_000);
    }

    #[test]
    fn rejected_request_cannot_install_a_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(4, 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
        assert_eq!(policy.active_contract(), Contract::none());
    }

    #[test]
    fn rejected_startup_request_can_requery_and_install_a_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);

        policy.on_control_message(4, 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);

        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
        policy.on_control_message(3, 1);
        policy.on_control_message(6, 2);

        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
    }

    #[test]
    fn wait_startup_request_can_requery_and_install_a_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);

        policy.on_control_message(12, 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);

        assert!(
            policy
                .on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A])
                .is_some()
        );
        policy.on_control_message(3, 1);
        policy.on_control_message(6, 2);

        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
    }

    #[test]
    fn rejected_renegotiation_preserves_the_active_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities(&[PPS_APDO_5V_TO_21V_5A]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        let active_contract = policy.active_contract();

        assert!(policy.request_pps_voltage(12_000).is_some());
        policy.on_control_message(4, 1);

        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract(), active_contract);
    }

    #[test]
    fn stale_control_responses_cannot_complete_a_new_source_capabilities_exchange() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities_with_message_id(&[PPS_APDO_5V_TO_21V_5A], Some(3));

        policy.on_control_message_with_message_id(3, Some(2), 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);

        // The Source may send another valid message before Accept; an exact
        // +1 requirement would incorrectly discard this response.
        policy.on_control_message_with_message_id(3, Some(4), 1);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);

        // A duplicate Accept ID is stale for the PS_RDY phase.
        policy.on_control_message_with_message_id(6, Some(4), 2);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);
        assert_eq!(policy.active_contract(), Contract::none());

        // PS_RDY may also skip an intervening Source message ID.
        policy.on_control_message_with_message_id(6, Some(6), 3);
        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
    }

    #[test]
    fn source_response_message_ids_accept_wraparound_and_reject_old_ids() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities_with_message_id(&[PPS_APDO_5V_TO_21V_5A], Some(7));

        policy.on_control_message_with_message_id(3, Some(0), 0);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);

        policy.on_control_message_with_message_id(6, Some(7), 1);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);
        policy.on_control_message_with_message_id(6, Some(2), 2);
        assert_eq!(policy.phase(), SinkPhase::Ready);
    }

    #[test]
    fn capability_refresh_preserves_a_still_advertised_contract() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities_with_message_id(&[PPS_APDO_5V_TO_21V_5A], Some(1));
        policy.on_control_message_with_message_id(3, Some(2), 0);
        policy.on_control_message_with_message_id(6, Some(3), 1);
        let active_contract = policy.active_contract();

        assert_eq!(
            policy.refresh_source_capabilities_with_message_id(&[PPS_APDO_5V_TO_21V_5A], Some(4)),
            None
        );
        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract(), active_contract);
    }

    #[test]
    fn capability_refresh_clears_an_invalid_contract_and_starts_discovery() {
        let mut policy = SinkPolicy::new(20_000, 5_000);
        let _ = policy.on_source_capabilities_with_message_id(&[PPS_APDO_5V_TO_21V_5A], Some(1));
        policy.on_control_message_with_message_id(3, Some(2), 0);
        policy.on_control_message_with_message_id(6, Some(3), 1);

        assert_eq!(
            policy.refresh_source_capabilities_with_message_id(&[0x0000_0000], Some(4)),
            None
        );
        assert_eq!(policy.phase(), SinkPhase::WaitingForSourceCapabilities);
        assert_eq!(policy.active_contract(), Contract::none());
    }

    #[test]
    fn startup_headers_use_the_fusb302b_pps_pd30_revision() {
        assert_eq!(request_header(5), 0x1a82);
        assert_eq!(get_source_capabilities_header(5), 0x0a87);
        assert_eq!(accept_header(0), 0x0083);
    }
}
