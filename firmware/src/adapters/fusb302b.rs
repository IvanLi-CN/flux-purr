//! FUSB302B register-level sink support.
//!
//! The controller is deliberately probed with reads only.  It shares `0x22`
//! with CH224Q, so an I2C ACK must never select a driver or permit a write.

use super::pd::{Contract, ContractKind, ControllerKind, SourceCapabilities};

pub const I2C_ADDRESS: u8 = 0x22;
pub const DEVICE_ID_REGISTER: u8 = 0x01;
pub const SWITCHES0_REGISTER: u8 = 0x02;
pub const SWITCHES1_REGISTER: u8 = 0x03;
pub const MEASURE_REGISTER: u8 = 0x04;
pub const CONTROL0_REGISTER: u8 = 0x06;
pub const CONTROL1_REGISTER: u8 = 0x07;
pub const CONTROL2_REGISTER: u8 = 0x08;
pub const CONTROL3_REGISTER: u8 = 0x09;
pub const MASK_REGISTER: u8 = 0x0a;
pub const POWER_REGISTER: u8 = 0x0b;
pub const RESET_REGISTER: u8 = 0x0c;
pub const MASKA_REGISTER: u8 = 0x0e;
pub const MASKB_REGISTER: u8 = 0x0f;
pub const STATUS0A_REGISTER: u8 = 0x3c;
pub const STATUS1A_REGISTER: u8 = 0x3d;
pub const INTERRUPTA_REGISTER: u8 = 0x3e;
pub const STATUS0_REGISTER: u8 = 0x40;
pub const STATUS1_REGISTER: u8 = 0x41;
pub const INTERRUPT_REGISTER: u8 = 0x42;
pub const FIFO_REGISTER: u8 = 0x43;

const DEVICE_ID_SIGNATURE_MASK: u8 = 0b1111_0000;
const DEVICE_ID_SIGNATURE: u8 = 0b1001_0000;
const SWITCHES0_PDWN1: u8 = 1;
const SWITCHES0_PDWN2: u8 = 1 << 1;
const SWITCHES0_SINK_PULL_DOWNS: u8 = SWITCHES0_PDWN1 | SWITCHES0_PDWN2;
const SWITCHES0_MEAS_CC1: u8 = 1 << 2;
const SWITCHES0_MEAS_CC2: u8 = 1 << 3;
const SWITCHES1_SPEC_REV_30: u8 = 0b10 << 5;
const SWITCHES1_AUTO_CRC: u8 = 1 << 2;
const SWITCHES1_TXCC1: u8 = 1;
const SWITCHES1_TXCC2: u8 = 1 << 1;
const CONTROL2_MODE_SNK_TOGGLE: u8 = (0b10 << 1) | 1;
const CONTROL2_MODE_SNK: u8 = 0b10 << 1;
const CONTROL3_AUTO_RETRY_3: u8 = 0b111;
const CONTROL0_HOST_CURRENT_DEFAULT: u8 = 0b01 << 2;
const CONTROL0_TX_FLUSH: u8 = 1 << 6;
const CONTROL1_RX_FLUSH: u8 = 1 << 2;
const POWER_ALL: u8 = 0x0f;
const RESET_SW: u8 = 1;
const RESET_PD: u8 = 1 << 1;
const MASK_TOGGLE: u8 = 0x7f;
const MASKA_TOGGLE: u8 = 0xbf;
const MASK_RECEIVE: u8 = 0x7d;
const MASKA_RECEIVE: u8 = 0xe0;
const MASKB_RECEIVE: u8 = 0;
const TXON: u8 = 0xa1;
const SYNC1: u8 = 0x12;
const SYNC2: u8 = 0x13;
const RESET1: u8 = 0x15;
const RESET2: u8 = 0x16;
const PACKSYM: u8 = 0x80;
const JAMCRC: u8 = 0xff;
const EOP: u8 = 0x14;
const TXOFF: u8 = 0xfe;
const SOP: u8 = 0xe0;

pub trait RegisterIo {
    type Error;

    fn read_register(&mut self, address: u8, register: u8) -> Result<u8, Self::Error>;
    fn write_register(&mut self, address: u8, register: u8, value: u8) -> Result<(), Self::Error>;
    fn read_fifo(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), Self::Error>;
    fn write_fifo(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error>;

    /// The interrupt bytes are read-to-clear, so hardware implementations
    /// should override this with one contiguous I2C transaction.
    fn read_status_snapshot(&mut self, address: u8) -> Result<[u8; 7], Self::Error> {
        Ok([
            self.read_register(address, STATUS0A_REGISTER)?,
            self.read_register(address, STATUS1A_REGISTER)?,
            self.read_register(address, INTERRUPTA_REGISTER)?,
            self.read_register(address, 0x3f)?,
            self.read_register(address, STATUS0_REGISTER)?,
            self.read_register(address, STATUS1_REGISTER)?,
            self.read_register(address, INTERRUPT_REGISTER)?,
        ])
    }
}

const STATUS1_RX_EMPTY: u8 = 1 << 5;
#[cfg(test)]
const STATUS1_RX_FULL: u8 = 1 << 4;
const STATUS1_OVERTEMP: u8 = 1 << 1;
const STATUS1_VCONN_OCP: u8 = 1;
const STATUS0_VBUS_OK: u8 = 1 << 7;
const STATUS0_CRC_CHECK: u8 = 1 << 4;
const STATUS0A_RETRY_FAIL: u8 = 1 << 4;
const INTERRUPTA_TX_SENT: u8 = 1 << 2;
const INTERRUPTA_SOFT_RESET: u8 = 1 << 1;
const INTERRUPTA_HARD_RESET: u8 = 1;
const INTERRUPTB_GCRC_SENT: u8 = 1;
const STATUS1A_RXSOP: u8 = 1;
const TOGSS_MASK: u8 = 0b0011_1000;
const TOGSS_SNK_CC1: u8 = 0b0010_1000;
const TOGSS_SNK_CC2: u8 = 0b0011_0000;
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

/// Sink-policy state held independently from the I2C transaction.  Callers
/// take the bus only to exchange a single frame, then run this policy without
/// holding the shared EEPROM/PD I2C bus across any PD timing interval.
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
        if self.source_capabilities_received {
            Some(self.source_capabilities)
        } else {
            None
        }
    }

    /// Select a PPS contract, with fixed PDO fallback, from source capabilities.
    pub fn on_source_capabilities(&mut self, pdos: &[u32]) -> Option<[u8; 4]> {
        self.source_capabilities = SourceCapabilities::from_pdos(pdos);
        self.source_capabilities_received = true;
        self.begin_request(self.source_capabilities)
    }

    fn begin_request(&mut self, capabilities: SourceCapabilities) -> Option<[u8; 4]> {
        let contract =
            Fusb302b::select_contract(capabilities, self.requested_mv, self.preferred_ma)?;
        let rdo = Fusb302b::request_data_object(contract)?;
        self.pending_contract = contract;
        self.phase = SinkPhase::WaitingForAccept;
        Some(rdo)
    }

    fn select_pps_contract(&self, requested_mv: u16) -> Option<Contract> {
        if !self.source_capabilities_received {
            return None;
        }
        let contract =
            Fusb302b::select_contract(self.source_capabilities, requested_mv, self.preferred_ma)?;
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
        let rdo = Fusb302b::request_data_object(contract)?;
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
        let rdo = Fusb302b::request_data_object(contract)?;
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
        Fusb302b::request_data_object(self.pending_contract)
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
    /// resumes negotiation, so delayed frames from this transaction cannot
    /// install a contract after the timeout.
    pub fn timeout_pending_request(&mut self) {
        if matches!(
            self.phase,
            SinkPhase::WaitingForAccept | SinkPhase::WaitingForPsRdy
        ) {
            self.mark_fault();
        }
    }

    /// Handles the control messages relevant to a sink request.  `Accept`
    /// alone never arms heating; only `PS_RDY` installs the active contract.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkPolarity {
    Cc1,
    Cc2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceivedMessage {
    pub header: u16,
    pub data: [u8; 28],
    pub data_len: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhyActivity {
    pub tx_sent: bool,
    pub gcrc_sent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveFault {
    PhyProtection,
    MissingCrc,
    MissingSop,
    UnsupportedSop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveEvent {
    Empty(PhyActivity),
    /// The PHY has started filling the FIFO, but the complete SOP frame has
    /// not been validated yet.  This is a normal receive transient, not a
    /// policy failure.
    Partial(PhyActivity),
    Message(ReceivedMessage),
    Reset,
    RetryFailed,
    Fault(ReceiveFault),
}

impl ReceivedMessage {
    pub const fn message_type(self) -> u8 {
        (self.header & 0x1f) as u8
    }

    pub const fn data_object_count(self) -> u8 {
        ((self.header >> 12) & 0x07) as u8
    }

    pub fn source_capabilities(self) -> Option<([u32; 7], usize)> {
        if self.message_type() != 1 || self.data_object_count() == 0 {
            return None;
        }
        let count = usize::from(self.data_object_count());
        let mut pdos = [0_u32; 7];
        for (index, pdo) in pdos.iter_mut().enumerate().take(count) {
            let offset = index * 4;
            *pdo = u32::from_le_bytes([
                self.data[offset],
                self.data[offset + 1],
                self.data[offset + 2],
                self.data[offset + 3],
            ]);
        }
        Some((pdos, count))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceId(pub u8);

impl DeviceId {
    pub const fn is_fusb302b(self) -> bool {
        // The FUSB302BMPX Device ID reset value is 0x9x. The low nibble is the
        // revision, so it must not participate in controller selection.
        self.0 & DEVICE_ID_SIGNATURE_MASK == DEVICE_ID_SIGNATURE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedController {
    Fusb302b(DeviceId),
    Ch224q,
    Unknown,
}

impl DetectedController {
    pub const fn kind(self) -> ControllerKind {
        match self {
            Self::Fusb302b(_) => ControllerKind::Fusb302b,
            Self::Ch224q => ControllerKind::Ch224q,
            Self::Unknown => ControllerKind::Unknown,
        }
    }
}

/// Performs no writes. A positive FUSB match requires a stable, plausible
/// device ID and a readable FUSB status bank. Only an explicitly non-FUSB,
/// stable ID permits the CH224Q fallback probe. An I2C error or an unstable
/// FUSB-looking value is ambiguous and must remain interlocked.
pub fn detect_controller<IO: RegisterIo>(io: &mut IO) -> DetectedController {
    match probe_fusb_signature(io) {
        FusbProbe::Matched(device_id) => DetectedController::Fusb302b(device_id),
        FusbProbe::Absent => {
            if read_ch224q_signature(io) {
                DetectedController::Ch224q
            } else {
                DetectedController::Unknown
            }
        }
        FusbProbe::Inconclusive => DetectedController::Unknown,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FusbProbe {
    Matched(DeviceId),
    Absent,
    Inconclusive,
}

fn probe_fusb_signature<IO: RegisterIo>(io: &mut IO) -> FusbProbe {
    let Ok(first) = io.read_register(I2C_ADDRESS, DEVICE_ID_REGISTER) else {
        return FusbProbe::Inconclusive;
    };
    let Ok(second) = io.read_register(I2C_ADDRESS, DEVICE_ID_REGISTER) else {
        return FusbProbe::Inconclusive;
    };
    let first = DeviceId(first);
    let second = DeviceId(second);
    if first != second {
        return FusbProbe::Inconclusive;
    }
    if !first.is_fusb302b() {
        return FusbProbe::Absent;
    }

    let Ok(status0) = io.read_register(I2C_ADDRESS, STATUS0_REGISTER) else {
        return FusbProbe::Inconclusive;
    };
    let Ok(status1) = io.read_register(I2C_ADDRESS, STATUS1_REGISTER) else {
        return FusbProbe::Inconclusive;
    };
    if status0 == 0xff || status1 == 0xff {
        return FusbProbe::Inconclusive;
    }
    FusbProbe::Matched(first)
}

fn read_ch224q_signature<IO: RegisterIo>(io: &mut IO) -> bool {
    let Ok(status) = io.read_register(I2C_ADDRESS, 0x09) else {
        return false;
    };
    let Ok(current) = io.read_register(I2C_ADDRESS, 0x50) else {
        return false;
    };
    status & 0x80 == 0 && current != 0xff
}

pub struct Fusb302b;

impl Fusb302b {
    /// Configure the PD PHY only after `detect_controller` selected FUSB302B.
    pub fn initialize_sink<IO: RegisterIo>(io: &mut IO) -> Result<(), IO::Error> {
        io.write_register(I2C_ADDRESS, RESET_REGISTER, RESET_SW)?;
        io.write_register(I2C_ADDRESS, RESET_REGISTER, RESET_PD)?;
        io.write_register(I2C_ADDRESS, POWER_REGISTER, POWER_ALL)?;
        io.write_register(
            I2C_ADDRESS,
            CONTROL0_REGISTER,
            CONTROL0_HOST_CURRENT_DEFAULT,
        )?;
        io.write_register(I2C_ADDRESS, CONTROL1_REGISTER, CONTROL1_RX_FLUSH)?;
        io.write_register(I2C_ADDRESS, CONTROL3_REGISTER, CONTROL3_AUTO_RETRY_3)?;
        io.write_register(I2C_ADDRESS, MASK_REGISTER, MASK_TOGGLE)?;
        io.write_register(I2C_ADDRESS, MASKA_REGISTER, 0xff)?;
        io.write_register(I2C_ADDRESS, MASKB_REGISTER, 0xff)?;
        // Keep Rd enabled on both CC pins while the PHY toggles as a sink.
        io.write_register(I2C_ADDRESS, SWITCHES1_REGISTER, SWITCHES1_SPEC_REV_30)?;
        io.write_register(I2C_ADDRESS, SWITCHES0_REGISTER, SWITCHES0_SINK_PULL_DOWNS)?;
        io.write_register(I2C_ADDRESS, MEASURE_REGISTER, 0)?;
        Self::clear_interrupt_latches(io)?;
        Self::start_sink_toggle(io)
    }

    fn start_sink_toggle<IO: RegisterIo>(io: &mut IO) -> Result<(), IO::Error> {
        io.write_register(I2C_ADDRESS, SWITCHES0_REGISTER, SWITCHES0_SINK_PULL_DOWNS)?;
        io.write_register(I2C_ADDRESS, SWITCHES1_REGISTER, SWITCHES1_SPEC_REV_30)?;
        io.write_register(I2C_ADDRESS, MASK_REGISTER, MASK_TOGGLE)?;
        io.write_register(I2C_ADDRESS, MASKA_REGISTER, MASKA_TOGGLE)?;
        io.write_register(I2C_ADDRESS, MASKB_REGISTER, 0xff)?;
        io.write_register(I2C_ADDRESS, CONTROL2_REGISTER, CONTROL2_MODE_SNK_TOGGLE)
    }

    fn clear_interrupt_latches<IO: RegisterIo>(io: &mut IO) -> Result<(), IO::Error> {
        let _ = io.read_status_snapshot(I2C_ADDRESS)?;
        Ok(())
    }

    pub fn read_sink_polarity<IO: RegisterIo>(
        io: &mut IO,
    ) -> Result<Option<SinkPolarity>, IO::Error> {
        let status = io.read_register(I2C_ADDRESS, STATUS1A_REGISTER)? & TOGSS_MASK;
        Ok(match status {
            TOGSS_SNK_CC1 => Some(SinkPolarity::Cc1),
            TOGSS_SNK_CC2 => Some(SinkPolarity::Cc2),
            _ => None,
        })
    }

    pub fn select_sink_polarity<IO: RegisterIo>(
        io: &mut IO,
        polarity: SinkPolarity,
    ) -> Result<(), IO::Error> {
        io.write_register(I2C_ADDRESS, CONTROL1_REGISTER, CONTROL1_RX_FLUSH)?;
        let control0 = io.read_register(I2C_ADDRESS, CONTROL0_REGISTER)?;
        io.write_register(I2C_ADDRESS, CONTROL0_REGISTER, control0 | CONTROL0_TX_FLUSH)?;
        io.write_register(I2C_ADDRESS, CONTROL2_REGISTER, CONTROL2_MODE_SNK)?;
        let (switches0, tx_cc) = match polarity {
            SinkPolarity::Cc1 => (
                SWITCHES0_SINK_PULL_DOWNS | SWITCHES0_MEAS_CC1,
                SWITCHES1_TXCC1,
            ),
            SinkPolarity::Cc2 => (
                SWITCHES0_SINK_PULL_DOWNS | SWITCHES0_MEAS_CC2,
                SWITCHES1_TXCC2,
            ),
        };
        io.write_register(I2C_ADDRESS, SWITCHES0_REGISTER, switches0)?;
        io.write_register(
            I2C_ADDRESS,
            SWITCHES1_REGISTER,
            SWITCHES1_SPEC_REV_30 | tx_cc,
        )?;
        io.write_register(
            I2C_ADDRESS,
            SWITCHES1_REGISTER,
            SWITCHES1_SPEC_REV_30 | SWITCHES1_AUTO_CRC | tx_cc,
        )?;
        io.write_register(I2C_ADDRESS, MASK_REGISTER, MASK_RECEIVE)?;
        io.write_register(I2C_ADDRESS, MASKA_REGISTER, MASKA_RECEIVE)?;
        io.write_register(I2C_ADDRESS, MASKB_REGISTER, MASKB_RECEIVE)
    }

    pub fn vbus_present<IO: RegisterIo>(io: &mut IO) -> Result<bool, IO::Error> {
        Ok(io.read_register(I2C_ADDRESS, STATUS0_REGISTER)? & STATUS0_VBUS_OK != 0)
    }

    pub fn request_header(message_id: u8) -> [u8; 2] {
        (PD_HEADER_REQUEST
            | PD_HEADER_SPEC_REV_30
            | (u16::from(message_id & 0x07) << 9)
            | (1 << 12))
            .to_le_bytes()
    }

    pub fn get_source_capabilities_header(message_id: u8) -> [u8; 2] {
        (PD_HEADER_GET_SOURCE_CAP | PD_HEADER_SPEC_REV_30 | (u16::from(message_id & 0x07) << 9))
            .to_le_bytes()
    }

    pub fn select_contract(
        capabilities: SourceCapabilities,
        requested_mv: u16,
        preferred_ma: u16,
    ) -> Option<Contract> {
        capabilities.select_fusb302b_contract(requested_mv, preferred_ma)
    }

    pub fn request_data_object(contract: Contract) -> Option<[u8; 4]> {
        if contract.object_position == 0 {
            return None;
        }
        let raw = match contract.kind {
            ContractKind::Pps => {
                let voltage_units =
                    u32::from(contract.voltage_mv.div_ceil(PPS_RDO_VOLTAGE_STEP_MV));
                let current_units =
                    u32::from(contract.current_ma.div_ceil(PPS_RDO_CURRENT_STEP_MA));
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

    /// Emit a PD message through the FUSB302B token FIFO.
    pub fn transmit_data_message<IO: RegisterIo>(
        io: &mut IO,
        header: [u8; 2],
        data: &[u8],
    ) -> Result<(), IO::Error> {
        let payload_len = header.len() + data.len();
        if payload_len > 31 {
            return Ok(());
        }
        let mut frame = [0u8; 40];
        let mut cursor = 0;
        for token in [SYNC1, SYNC1, SYNC1, SYNC2, PACKSYM | payload_len as u8] {
            frame[cursor] = token;
            cursor += 1;
        }
        frame[cursor..cursor + 2].copy_from_slice(&header);
        cursor += 2;
        frame[cursor..cursor + data.len()].copy_from_slice(data);
        cursor += data.len();
        for token in [JAMCRC, EOP, TXOFF, TXON] {
            frame[cursor] = token;
            cursor += 1;
        }
        io.write_fifo(I2C_ADDRESS, &frame[..cursor])
    }

    pub fn transmit_hard_reset<IO: RegisterIo>(io: &mut IO) -> Result<(), IO::Error> {
        io.write_fifo(I2C_ADDRESS, &[RESET1, RESET1, RESET1, RESET2, TXON])
    }

    /// Read exactly one SOP message from the receive FIFO. The transport only
    /// holds I2C for the two bounded FIFO reads; policy wait time is outside
    /// the shared EEPROM/PD bus transaction.
    pub fn receive_message<IO: RegisterIo>(io: &mut IO) -> Result<ReceiveEvent, IO::Error> {
        let [
            status0a,
            status1a,
            interrupta,
            interruptb,
            status0,
            status1,
            _interrupt,
        ] = io.read_status_snapshot(I2C_ADDRESS)?;
        let activity = PhyActivity {
            tx_sent: interrupta & INTERRUPTA_TX_SENT != 0,
            gcrc_sent: interruptb & INTERRUPTB_GCRC_SENT != 0,
        };
        if interrupta & (INTERRUPTA_SOFT_RESET | INTERRUPTA_HARD_RESET) != 0 {
            return Ok(ReceiveEvent::Reset);
        }
        if status0a & STATUS0A_RETRY_FAIL != 0 && status1 & STATUS1_RX_EMPTY != 0 {
            return Ok(ReceiveEvent::RetryFailed);
        }
        // A multi-PDO SourceCaps frame can legitimately fill the receive
        // FIFO. RX_FULL is therefore an indication to drain it, not a fault.
        if status1 & (STATUS1_OVERTEMP | STATUS1_VCONN_OCP) != 0 {
            return Ok(ReceiveEvent::Fault(ReceiveFault::PhyProtection));
        }
        if status1 & STATUS1_RX_EMPTY != 0 {
            return Ok(ReceiveEvent::Empty(activity));
        }
        if status0 & STATUS0_CRC_CHECK == 0 || status1a & STATUS1A_RXSOP == 0 {
            // STATUS1 can observe the first FIFO byte before the PHY has
            // completed CRC validation and asserted RXSOP.  Leaving that byte
            // in place lets the next bounded service turn consume the complete
            // frame instead of turning a valid source message into a reset.
            return Ok(ReceiveEvent::Partial(activity));
        }

        let mut prefix = [0_u8; 3];
        io.read_fifo(I2C_ADDRESS, &mut prefix)?;
        if prefix[0] & 0b1110_0000 != SOP {
            return Ok(ReceiveEvent::Fault(ReceiveFault::UnsupportedSop));
        }
        let header = u16::from_le_bytes([prefix[1], prefix[2]]);
        let data_len = usize::from((header >> 12) & 0x07) * 4;
        let mut remainder = [0_u8; 32];
        io.read_fifo(I2C_ADDRESS, &mut remainder[..data_len + 4])?;
        let mut data = [0_u8; 28];
        data[..data_len].copy_from_slice(&remainder[..data_len]);
        Ok(ReceiveEvent::Message(ReceivedMessage {
            header,
            data,
            data_len: data_len as u8,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::pd::{ContractKind, SourceCapabilities};

    struct FakeIo {
        reads: [(u8, u8, u8); 16],
        read_len: usize,
        device_id_sequence: [u8; 2],
        device_id_sequence_len: usize,
        device_id_sequence_offset: usize,
        writes: [(u8, u8, u8); 24],
        write_len: usize,
        fifo: [u8; 40],
        fifo_len: usize,
        rx_fifo: [u8; 40],
        rx_len: usize,
        rx_offset: usize,
    }

    impl Default for FakeIo {
        fn default() -> Self {
            Self {
                reads: [(0, 0, 0); 16],
                read_len: 0,
                device_id_sequence: [0; 2],
                device_id_sequence_len: 0,
                device_id_sequence_offset: 0,
                writes: [(0, 0, 0); 24],
                write_len: 0,
                fifo: [0; 40],
                fifo_len: 0,
                rx_fifo: [0; 40],
                rx_len: 0,
                rx_offset: 0,
            }
        }
    }

    impl FakeIo {
        fn with_read(mut self, register: u8, value: u8) -> Self {
            self.reads[self.read_len] = (I2C_ADDRESS, register, value);
            self.read_len += 1;
            self
        }

        fn with_device_id_sequence(mut self, values: [u8; 2]) -> Self {
            self.device_id_sequence = values;
            self.device_id_sequence_len = values.len();
            self
        }

        fn with_cleared_interrupts(mut self) -> Self {
            for register in [
                STATUS0A_REGISTER,
                STATUS1A_REGISTER,
                INTERRUPTA_REGISTER,
                0x3f,
                STATUS0_REGISTER,
                STATUS1_REGISTER,
                INTERRUPT_REGISTER,
            ] {
                self = self.with_read(register, 0);
            }
            self
        }

        fn with_rx_fifo(mut self, bytes: &[u8]) -> Self {
            self.rx_fifo[..bytes.len()].copy_from_slice(bytes);
            self.rx_len = bytes.len();
            self
        }
    }

    impl RegisterIo for FakeIo {
        type Error = ();

        fn read_register(&mut self, address: u8, register: u8) -> Result<u8, Self::Error> {
            if address == I2C_ADDRESS
                && register == DEVICE_ID_REGISTER
                && self.device_id_sequence_offset < self.device_id_sequence_len
            {
                let value = self.device_id_sequence[self.device_id_sequence_offset];
                self.device_id_sequence_offset += 1;
                return Ok(value);
            }
            self.reads[..self.read_len]
                .iter()
                .find_map(|(candidate_address, candidate_register, value)| {
                    (*candidate_address == address && *candidate_register == register)
                        .then_some(*value)
                })
                .ok_or(())
        }

        fn write_register(
            &mut self,
            address: u8,
            register: u8,
            value: u8,
        ) -> Result<(), Self::Error> {
            self.writes[self.write_len] = (address, register, value);
            self.write_len += 1;
            Ok(())
        }

        fn read_fifo(&mut self, _address: u8, bytes: &mut [u8]) -> Result<(), Self::Error> {
            let end = self.rx_offset + bytes.len();
            if end > self.rx_len {
                return Err(());
            }
            bytes.copy_from_slice(&self.rx_fifo[self.rx_offset..end]);
            self.rx_offset = end;
            Ok(())
        }

        fn write_fifo(&mut self, _address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
            self.fifo[..bytes.len()].copy_from_slice(bytes);
            self.fifo_len = bytes.len();
            Ok(())
        }
    }

    #[test]
    fn selects_fusb_only_after_stable_read_only_signature() {
        let mut io = FakeIo::default()
            .with_read(DEVICE_ID_REGISTER, 0x90)
            .with_read(STATUS0_REGISTER, 0)
            .with_read(STATUS1_REGISTER, 0);

        assert_eq!(
            detect_controller(&mut io),
            DetectedController::Fusb302b(DeviceId(0x90))
        );
        assert_eq!(io.write_len, 0);
    }

    #[test]
    fn recognizes_only_the_documented_9x_fusb302bmpx_signature() {
        assert!(DeviceId(0x90).is_fusb302b());
        assert!(DeviceId(0x9f).is_fusb302b());
        assert!(!DeviceId(0x80).is_fusb302b());
        assert!(!DeviceId(0xff).is_fusb302b());
    }

    #[test]
    fn rejects_an_unstable_fusb_signature_without_writing() {
        let mut io = FakeIo::default()
            .with_device_id_sequence([0x90, 0x91])
            .with_read(0x09, 0)
            .with_read(0x50, 0);

        assert_eq!(detect_controller(&mut io), DetectedController::Unknown);
        assert_eq!(io.write_len, 0);
    }

    #[test]
    fn rejects_an_incomplete_fusb_probe_without_falling_through_to_ch224q() {
        let mut io = FakeIo::default()
            .with_device_id_sequence([0x90, 0x90])
            .with_read(0x09, 0)
            .with_read(0x50, 0);

        assert_eq!(detect_controller(&mut io), DetectedController::Unknown);
        assert_eq!(io.write_len, 0);
    }

    #[test]
    fn rejects_a_mixed_controller_signature_without_falling_through_to_ch224q() {
        let mut io = FakeIo::default()
            .with_device_id_sequence([0x90, 0x91])
            .with_read(0x09, 0)
            .with_read(0x50, 0);

        assert_eq!(detect_controller(&mut io), DetectedController::Unknown);
        assert_eq!(io.write_len, 0);
    }

    #[test]
    fn probes_ch224q_only_after_fusb_signature_is_absent() {
        let mut io = FakeIo::default()
            .with_read(DEVICE_ID_REGISTER, 0x00)
            .with_read(0x09, 0x08)
            .with_read(0x50, 0x20);

        assert_eq!(detect_controller(&mut io), DetectedController::Ch224q);
        assert_eq!(io.write_len, 0);
    }

    #[test]
    fn initializes_sink_only_after_detection() {
        let mut io = FakeIo::default().with_cleared_interrupts();
        Fusb302b::initialize_sink(&mut io).unwrap();

        assert_eq!(io.write_len, 18);
        assert_eq!(io.writes[0], (I2C_ADDRESS, RESET_REGISTER, RESET_SW));
        assert_eq!(io.writes[1], (I2C_ADDRESS, RESET_REGISTER, RESET_PD));
        assert_eq!(io.writes[6], (I2C_ADDRESS, MASK_REGISTER, MASK_TOGGLE));
        assert_eq!(
            io.writes[10],
            (I2C_ADDRESS, SWITCHES0_REGISTER, SWITCHES0_SINK_PULL_DOWNS)
        );
        assert_eq!(
            io.writes[13],
            (I2C_ADDRESS, SWITCHES1_REGISTER, SWITCHES1_SPEC_REV_30)
        );
        assert_eq!(io.writes[15], (I2C_ADDRESS, MASKA_REGISTER, MASKA_TOGGLE));
        assert_eq!(
            io.writes[17],
            (I2C_ADDRESS, CONTROL2_REGISTER, CONTROL2_MODE_SNK_TOGGLE)
        );
    }

    #[test]
    fn encodes_a_fixed_fallback_request_and_fusb_fifo_frame() {
        let capabilities =
            SourceCapabilities::from_pdos(&[((20_000_u32 / 50) << 10) | (5_000 / 10)]);
        let contract = Fusb302b::select_contract(capabilities, 20_000, 5_000).unwrap();
        assert_eq!(contract.kind, ContractKind::Fixed);
        assert_eq!(
            Fusb302b::request_data_object(contract),
            Some([244, 209, 7, 17])
        );

        let mut io = FakeIo::default();
        Fusb302b::transmit_data_message(&mut io, [0x42, 0x10], &[244, 209, 7, 16]).unwrap();
        assert_eq!(&io.fifo[..5], &[SYNC1, SYNC1, SYNC1, SYNC2, PACKSYM | 6]);
        assert_eq!(&io.fifo[5..11], &[0x42, 0x10, 244, 209, 7, 16]);
        assert_eq!(&io.fifo[11..15], &[JAMCRC, EOP, TXOFF, TXON]);
    }

    #[test]
    fn encodes_a_single_hard_reset_ordered_set() {
        let mut io = FakeIo::default();

        Fusb302b::transmit_hard_reset(&mut io).unwrap();

        assert_eq!(
            &io.fifo[..io.fifo_len],
            &[RESET1, RESET1, RESET1, RESET2, TXON]
        );
    }

    #[test]
    fn encodes_a_pd30_pps_request() {
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let capabilities = SourceCapabilities::from_pdos(&[pps_5v_to_21v_5a]);
        let contract = Fusb302b::select_contract(capabilities, 20_000, 5_000).unwrap();
        assert_eq!(contract.kind, ContractKind::Pps);
        assert_eq!(
            Fusb302b::request_data_object(contract),
            Some([0x64, 0xd0, 0x07, 0x11])
        );
        assert_eq!(Fusb302b::request_header(3), [0x82, 0x16]);
    }

    #[test]
    fn policy_installs_fixed_contract_only_after_accept_and_ps_rdy() {
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(policy.on_source_capabilities(&[fixed_20v_5a]).is_some());
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        assert_eq!(policy.active_contract(), Contract::none());

        policy.on_control_message(3, 100);
        assert_eq!(policy.phase(), SinkPhase::WaitingForPsRdy);
        policy.on_control_message(6, 200);
        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().power_mw(), 100_000);
        assert!(policy.active_contract().performance_guaranteed());
    }

    #[test]
    fn policy_installs_pps_only_source_capabilities_after_accept_and_ps_rdy() {
        let pps_5v_to_21v_3a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (3_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert_eq!(
            policy.on_source_capabilities(&[pps_5v_to_21v_3a]),
            Some([0x3c, 0xd0, 0x07, 0x11])
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
        assert_eq!(policy.active_contract().power_mw(), 60_000);
    }

    #[test]
    fn policy_renews_a_pps_contract_at_the_requested_voltage() {
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(policy.on_source_capabilities(&[pps_5v_to_21v_5a]).is_some());
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);

        assert_eq!(
            policy.request_pps_voltage(12_400),
            Some([0x64, 0xd8, 0x04, 0x11])
        );
        policy.on_control_message(3, 2_000);
        policy.on_control_message(6, 2_020);
        assert_eq!(policy.active_contract().voltage_mv, 12_400);
        assert_eq!(policy.refresh_active_pps(), Some([0x64, 0xd8, 0x04, 0x11]));
        assert!(!Fusb302b::pps_keepalive_due(2_020, 7_019));
        assert!(Fusb302b::pps_keepalive_due(2_020, 7_020));
    }

    #[test]
    fn policy_moves_from_pps_to_an_exact_fixed_pdo_for_terminal_disarm() {
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(
            policy
                .on_source_capabilities(&[fixed_20v_5a, pps_5v_to_21v_5a])
                .is_some()
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);

        assert!(policy.request_fixed_voltage(20_000).is_some());
        policy.on_control_message(3, 20);
        policy.on_control_message(6, 40);

        assert_eq!(policy.active_contract().kind, ContractKind::Fixed);
        assert_eq!(policy.active_contract().voltage_mv, 20_000);
        assert_eq!(policy.active_contract().current_ma, 5_000);
    }

    #[test]
    fn policy_prepares_a_fixed_contract_for_a_refreshed_pps_request() {
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(
            policy
                .on_source_capabilities(&[fixed_20v_5a, pps_5v_to_21v_5a])
                .is_some()
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);

        assert!(policy.request_fixed_voltage(20_000).is_some());
        policy.on_control_message(3, 10);
        policy.on_control_message(6, 20);
        assert_eq!(policy.active_contract().kind, ContractKind::Fixed);

        assert!(policy.prepare_pps_request(20_000));
        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(
            policy.on_source_capabilities(&[fixed_20v_5a, pps_5v_to_21v_5a]),
            Some([0x64, 0xd0, 0x07, 0x21])
        );
        policy.on_control_message(3, 20);
        policy.on_control_message(6, 40);

        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
        assert_eq!(policy.active_contract().voltage_mv, 20_000);
    }

    #[test]
    fn policy_cancels_a_stalled_transition_without_losing_the_active_contract() {
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(
            policy
                .on_source_capabilities(&[fixed_20v_5a, pps_5v_to_21v_5a])
                .is_some()
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);

        assert!(policy.request_fixed_voltage(20_000).is_some());
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        policy.cancel_pending_request();

        assert_eq!(policy.phase(), SinkPhase::Ready);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);
        assert_eq!(policy.active_contract().voltage_mv, 20_000);
    }

    #[test]
    fn policy_timeout_disarms_and_discards_both_contracts() {
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let pps_5v_to_21v_5a =
            (0b11 << 30) | ((5_000_u32 / 100) << 8) | ((21_000_u32 / 100) << 17) | (5_000 / 50);
        let mut policy = SinkPolicy::new(20_000, 5_000);

        assert!(
            policy
                .on_source_capabilities(&[fixed_20v_5a, pps_5v_to_21v_5a])
                .is_some()
        );
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        assert_eq!(policy.active_contract().kind, ContractKind::Pps);

        assert!(policy.request_fixed_voltage(20_000).is_some());
        assert_eq!(policy.phase(), SinkPhase::WaitingForAccept);
        policy.timeout_pending_request();

        assert_eq!(policy.phase(), SinkPhase::Fault);
        assert_eq!(policy.active_contract(), Contract::none());
        assert_eq!(policy.pending_contract, Contract::none());
    }

    #[test]
    fn retries_source_capabilities_after_a_bounded_interval() {
        assert!(!Fusb302b::source_capabilities_retry_due(500, 1_499));
        assert!(Fusb302b::source_capabilities_retry_due(500, 5_500));
        assert!(!Fusb302b::source_capabilities_hard_reset_due(500, 1_499));
        assert!(Fusb302b::source_capabilities_hard_reset_due(500, 1_500));
        assert_eq!(SOURCE_CAPS_INITIAL_WAIT_MS, 400);
    }

    #[test]
    fn policy_detach_and_rejected_contract_clear_heater_authority() {
        let fixed_20v_3a = ((20_000_u32 / 50) << 10) | (3_000 / 10);
        let mut policy = SinkPolicy::new(20_000, 3_000);
        policy.on_source_capabilities(&[fixed_20v_3a]);
        policy.on_control_message(3, 0);
        policy.on_control_message(6, 0);
        policy.on_detach_or_reset();

        assert_eq!(policy.phase(), SinkPhase::Detached);
        assert_eq!(policy.active_contract(), Contract::none());
        policy.on_source_capabilities(&[fixed_20v_3a]);
        policy.on_control_message(4, 0);
        assert_eq!(policy.phase(), SinkPhase::Fault);
        assert_eq!(policy.active_contract(), Contract::none());
    }

    #[test]
    fn parses_source_capabilities_without_consuming_a_second_message() {
        let source_caps_header = (2_u16 << 12) | 1;
        let fixed_5v_3a = ((5_000_u32 / 50) << 10) | (3_000 / 10);
        let fixed_20v_5a = ((20_000_u32 / 50) << 10) | (5_000 / 10);
        let mut fifo = [0_u8; 15];
        // The low five bits of the SOP token are undefined by the PHY.
        fifo[0] = 0xe7;
        fifo[1..3].copy_from_slice(&source_caps_header.to_le_bytes());
        fifo[3..7].copy_from_slice(&fixed_5v_3a.to_le_bytes());
        fifo[7..11].copy_from_slice(&fixed_20v_5a.to_le_bytes());
        let mut io = FakeIo::default()
            .with_read(STATUS0A_REGISTER, 0)
            .with_read(STATUS1_REGISTER, STATUS1_RX_FULL)
            .with_read(STATUS0_REGISTER, STATUS0_CRC_CHECK)
            .with_read(INTERRUPTA_REGISTER, 0)
            .with_read(0x3f, 0)
            .with_read(STATUS1A_REGISTER, STATUS1A_RXSOP)
            .with_read(INTERRUPT_REGISTER, 0)
            .with_rx_fifo(&fifo);

        let ReceiveEvent::Message(message) = Fusb302b::receive_message(&mut io).unwrap() else {
            panic!("expected source capabilities");
        };
        let (pdos, count) = message.source_capabilities().unwrap();
        assert_eq!(count, 2);
        assert_eq!(pdos[1], fixed_20v_5a);
        assert_eq!(io.rx_offset, fifo.len());
    }

    #[test]
    fn defers_a_received_frame_until_the_phy_reports_a_complete_crc() {
        let mut io = FakeIo::default()
            .with_read(STATUS0A_REGISTER, 0)
            .with_read(STATUS1A_REGISTER, 0)
            .with_read(STATUS1_REGISTER, 0)
            .with_read(STATUS0_REGISTER, 0)
            .with_read(INTERRUPTA_REGISTER, 0)
            .with_read(0x3f, 0)
            .with_read(INTERRUPT_REGISTER, 0);

        assert_eq!(
            Fusb302b::receive_message(&mut io).unwrap(),
            ReceiveEvent::Partial(PhyActivity {
                tx_sent: false,
                gcrc_sent: false,
            })
        );
    }

    #[test]
    fn reports_a_transmit_retry_failure_for_runtime_recovery() {
        let mut io = FakeIo::default()
            .with_read(STATUS0A_REGISTER, STATUS0A_RETRY_FAIL)
            .with_read(STATUS1A_REGISTER, 0)
            .with_read(INTERRUPTA_REGISTER, 0)
            .with_read(STATUS1_REGISTER, STATUS1_RX_EMPTY)
            .with_read(STATUS0_REGISTER, STATUS0_VBUS_OK)
            .with_read(0x3f, 0)
            .with_read(INTERRUPT_REGISTER, 0);

        assert_eq!(
            Fusb302b::receive_message(&mut io).unwrap(),
            ReceiveEvent::RetryFailed
        );
    }

    #[test]
    fn rejects_non_sop_receive_tokens() {
        let mut io = FakeIo::default()
            .with_read(STATUS0A_REGISTER, 0)
            .with_read(STATUS1_REGISTER, 0)
            .with_read(STATUS0_REGISTER, STATUS0_CRC_CHECK)
            .with_read(INTERRUPTA_REGISTER, 0)
            .with_read(0x3f, 0)
            .with_read(STATUS1A_REGISTER, STATUS1A_RXSOP)
            .with_read(INTERRUPT_REGISTER, 0)
            .with_rx_fifo(&[0xc1, 0, 0]);

        assert_eq!(
            Fusb302b::receive_message(&mut io).unwrap(),
            ReceiveEvent::Fault(ReceiveFault::UnsupportedSop)
        );
    }

    #[test]
    fn defers_a_frame_until_the_phy_reports_a_confirmed_sop() {
        let mut io = FakeIo::default()
            .with_read(STATUS0A_REGISTER, 0)
            .with_read(STATUS1_REGISTER, 0)
            .with_read(STATUS0_REGISTER, STATUS0_CRC_CHECK)
            .with_read(INTERRUPTA_REGISTER, 0)
            .with_read(0x3f, 0)
            .with_read(STATUS1A_REGISTER, 0)
            .with_read(INTERRUPT_REGISTER, 0);

        assert_eq!(
            Fusb302b::receive_message(&mut io).unwrap(),
            ReceiveEvent::Partial(PhyActivity {
                tx_sent: false,
                gcrc_sent: false,
            })
        );
        assert_eq!(io.rx_offset, 0);
    }

    #[test]
    fn picks_the_attached_sink_cc_before_transmitting() {
        let mut io = FakeIo::default()
            .with_read(STATUS1A_REGISTER, TOGSS_SNK_CC2)
            .with_read(CONTROL0_REGISTER, CONTROL0_HOST_CURRENT_DEFAULT);
        assert_eq!(
            Fusb302b::read_sink_polarity(&mut io).unwrap(),
            Some(SinkPolarity::Cc2)
        );

        Fusb302b::select_sink_polarity(&mut io, SinkPolarity::Cc2).unwrap();
        assert_eq!(
            io.writes[0],
            (I2C_ADDRESS, CONTROL1_REGISTER, CONTROL1_RX_FLUSH)
        );
        assert_eq!(
            io.writes[1],
            (
                I2C_ADDRESS,
                CONTROL0_REGISTER,
                CONTROL0_HOST_CURRENT_DEFAULT | CONTROL0_TX_FLUSH
            )
        );
        assert_eq!(
            io.writes[3],
            (
                I2C_ADDRESS,
                SWITCHES0_REGISTER,
                SWITCHES0_SINK_PULL_DOWNS | SWITCHES0_MEAS_CC2
            )
        );
        assert_eq!(
            io.writes[5],
            (
                I2C_ADDRESS,
                SWITCHES1_REGISTER,
                SWITCHES1_SPEC_REV_30 | SWITCHES1_AUTO_CRC | SWITCHES1_TXCC2
            )
        );
        assert_eq!(io.writes[6], (I2C_ADDRESS, MASK_REGISTER, MASK_RECEIVE));
        assert_eq!(io.writes[8], (I2C_ADDRESS, MASKB_REGISTER, MASKB_RECEIVE));
    }

    #[test]
    fn reports_vbus_presence_from_the_phy_status() {
        let mut present = FakeIo::default().with_read(STATUS0_REGISTER, STATUS0_VBUS_OK);
        let mut absent = FakeIo::default().with_read(STATUS0_REGISTER, 0);

        assert!(Fusb302b::vbus_present(&mut present).unwrap());
        assert!(!Fusb302b::vbus_present(&mut absent).unwrap());
    }
}
