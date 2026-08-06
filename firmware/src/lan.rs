//! LAN control-plane security primitives shared by the HTTP server and the
//! front-panel pairing flow. Transport tasks must use these types instead of
//! keeping a second copy of pairing, token, or lease state.

use core::fmt::{self, Write as _};

use heapless::String;

pub const LAN_TOKEN_BYTES: usize = 32;
pub const LAN_TOKEN_HEX_LEN: usize = LAN_TOKEN_BYTES * 2;
pub const LAN_PAIRING_CODE_LEN: usize = 4;
pub const LAN_PAIRING_MAX_FAILURES: u8 = 5;
pub const LAN_LEASE_TTL_MS: u64 = 30_000;
pub const LAN_LEASE_ID_BYTES: usize = 12;
pub const PROD_ALLOWED_ORIGIN: &str = "https://flux-purr.ivanli.cc";

/// How a browser may obtain a LAN bearer token after it has established a
/// public connection to the device. The current production default is
/// [`Required`]; the other modes keep the HTTP v1 contract forward-compatible
/// without exposing a transient code before a connection exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LanPairingMode {
    #[default]
    Required,
    Optional,
    Unavailable,
}

impl LanPairingMode {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LanToken([u8; LAN_TOKEN_BYTES]);

impl fmt::Debug for LanToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LanToken(<redacted>)")
    }
}

impl LanToken {
    pub const fn new(bytes: [u8; LAN_TOKEN_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn bytes(self) -> [u8; LAN_TOKEN_BYTES] {
        self.0
    }

    pub fn write_hex(self, out: &mut String<LAN_TOKEN_HEX_LEN>) {
        out.clear();
        for byte in self.0 {
            let _ = write!(out, "{byte:02x}");
        }
    }

    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != LAN_TOKEN_HEX_LEN {
            return None;
        }

        let mut bytes = [0u8; LAN_TOKEN_BYTES];
        for (index, slot) in bytes.iter_mut().enumerate() {
            let high = hex_nibble(value.as_bytes()[index * 2])?;
            let low = hex_nibble(value.as_bytes()[index * 2 + 1])?;
            *slot = (high << 4) | low;
        }
        Some(Self(bytes))
    }

    /// Compare a token encoded in an HTTP bearer header without early exits.
    pub fn matches_bearer(self, value: &str) -> bool {
        let bytes = value.as_bytes();
        if bytes.len() != LAN_TOKEN_HEX_LEN {
            return false;
        }

        let mut difference = 0u8;
        for (index, expected) in self.0.iter().enumerate() {
            let high = hex_nibble_or_invalid(bytes[index * 2]);
            let low = hex_nibble_or_invalid(bytes[index * 2 + 1]);
            difference |= *expected ^ ((high << 4) | low);
        }
        difference == 0
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex_nibble_or_invalid(byte: u8) -> u8 {
    hex_nibble(byte).unwrap_or(0xff)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanAccessError {
    PairingInactive,
    PairingLocked,
    PairingCodeInvalid,
    PairingUnavailable,
    Unauthorized,
    LeaseBusy,
    LeaseRequired,
    LeaseExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingWindow {
    code: Option<[u8; LAN_PAIRING_CODE_LEN]>,
    failed_attempts: u8,
}

impl Default for PairingWindow {
    fn default() -> Self {
        Self::inactive()
    }
}

impl PairingWindow {
    pub const fn inactive() -> Self {
        Self {
            code: None,
            failed_attempts: 0,
        }
    }

    pub fn enter(&mut self, code: [u8; LAN_PAIRING_CODE_LEN]) {
        if code.iter().all(|digit| digit.is_ascii_digit()) {
            self.code = Some(code);
            self.failed_attempts = 0;
        } else {
            self.leave();
        }
    }

    pub fn leave(&mut self) {
        self.code = None;
        self.failed_attempts = 0;
    }

    pub const fn is_active(&self) -> bool {
        self.code.is_some()
    }

    pub const fn failed_attempts(&self) -> u8 {
        self.failed_attempts
    }

    pub const fn code(&self) -> Option<[u8; LAN_PAIRING_CODE_LEN]> {
        self.code
    }

    /// A correct claim intentionally leaves the window open: the owner may
    /// pair more than one client while the WiFi Info page remains visible.
    pub fn claim(&mut self, submitted: &str) -> Result<(), LanAccessError> {
        let Some(code) = self.code else {
            return Err(LanAccessError::PairingInactive);
        };
        if self.failed_attempts >= LAN_PAIRING_MAX_FAILURES {
            return Err(LanAccessError::PairingLocked);
        }
        if submitted.as_bytes() == code {
            return Ok(());
        }

        self.failed_attempts = self.failed_attempts.saturating_add(1);
        if self.failed_attempts >= LAN_PAIRING_MAX_FAILURES {
            self.code = None;
            return Err(LanAccessError::PairingLocked);
        }
        Err(LanAccessError::PairingCodeInvalid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanLease {
    pub id: [u8; LAN_LEASE_ID_BYTES],
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LanLeaseState {
    lease: Option<LanLease>,
}

impl LanLeaseState {
    pub fn create(
        &mut self,
        now_ms: u64,
        id: [u8; LAN_LEASE_ID_BYTES],
    ) -> Result<LanLease, LanAccessError> {
        self.expire(now_ms);
        if self.lease.is_some() {
            return Err(LanAccessError::LeaseBusy);
        }

        let lease = LanLease {
            id,
            expires_at_ms: now_ms.saturating_add(LAN_LEASE_TTL_MS),
        };
        self.lease = Some(lease);
        Ok(lease)
    }

    pub fn heartbeat(
        &mut self,
        now_ms: u64,
        id: [u8; LAN_LEASE_ID_BYTES],
    ) -> Result<LanLease, LanAccessError> {
        self.expire(now_ms);
        let Some(mut lease) = self.lease else {
            return Err(LanAccessError::LeaseExpired);
        };
        if !constant_time_bytes_equal(&lease.id, &id) {
            return Err(LanAccessError::LeaseRequired);
        }
        lease.expires_at_ms = now_ms.saturating_add(LAN_LEASE_TTL_MS);
        self.lease = Some(lease);
        Ok(lease)
    }

    pub fn require(
        &mut self,
        now_ms: u64,
        id: [u8; LAN_LEASE_ID_BYTES],
    ) -> Result<(), LanAccessError> {
        self.expire(now_ms);
        let Some(lease) = self.lease else {
            return Err(LanAccessError::LeaseRequired);
        };
        if constant_time_bytes_equal(&lease.id, &id) {
            Ok(())
        } else {
            Err(LanAccessError::LeaseRequired)
        }
    }

    pub fn release(&mut self, id: [u8; LAN_LEASE_ID_BYTES]) -> Result<(), LanAccessError> {
        let Some(lease) = self.lease else {
            return Err(LanAccessError::LeaseExpired);
        };
        if !constant_time_bytes_equal(&lease.id, &id) {
            return Err(LanAccessError::LeaseRequired);
        }
        self.lease = None;
        Ok(())
    }

    pub fn expire(&mut self, now_ms: u64) {
        if self
            .lease
            .is_some_and(|lease| lease.expires_at_ms <= now_ms)
        {
            self.lease = None;
        }
    }
}

fn constant_time_bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= *left ^ *right;
    }
    difference == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanEndpoint {
    Health,
    Pairing,
    PairingClaim,
    Lease,
    Identity,
    Network,
    Status,
    Events,
    Runtime,
    Calibration,
    CalibrationJob,
    HeaterCurve,
    HeaterCurveSave,
    ThermalProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointPermission {
    Public,
    Token,
    Lease,
}

impl LanEndpoint {
    pub const fn permission(self) -> EndpointPermission {
        match self {
            Self::Health | Self::Pairing | Self::PairingClaim => EndpointPermission::Public,
            Self::Identity | Self::Network | Self::Status | Self::Events => {
                EndpointPermission::Token
            }
            Self::Lease
            | Self::Runtime
            | Self::Calibration
            | Self::CalibrationJob
            | Self::HeaterCurve
            | Self::HeaterCurveSave
            | Self::ThermalProfile => EndpointPermission::Lease,
        }
    }
}

pub fn cors_allow_origin(origin: Option<&str>) -> Option<&str> {
    let origin = origin?.trim();
    if origin == PROD_ALLOWED_ORIGIN
        || origin == "http://localhost"
        || origin.starts_with("http://localhost:")
        || origin == "http://127.0.0.1"
        || origin.starts_with("http://127.0.0.1:")
    {
        Some(origin)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateNetworkPreflight {
    pub allow_origin: bool,
    pub allow_private_network: bool,
}

pub fn private_network_preflight(
    origin: Option<&str>,
    requested_private_network: bool,
) -> PrivateNetworkPreflight {
    let allow_origin = cors_allow_origin(origin).is_some();
    PrivateNetworkPreflight {
        allow_origin,
        allow_private_network: allow_origin && requested_private_network,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(byte: u8) -> LanToken {
        LanToken::new([byte; LAN_TOKEN_BYTES])
    }

    #[test]
    fn token_round_trip_and_bearer_comparison_are_redaction_safe() {
        let token = token(0xab);
        let mut encoded = String::new();
        token.write_hex(&mut encoded);
        assert_eq!(encoded.len(), LAN_TOKEN_HEX_LEN);
        assert_eq!(LanToken::from_hex(encoded.as_str()), Some(token));
        assert!(token.matches_bearer(encoded.as_str()));
        assert!(!token.matches_bearer("not-a-token"));
    }

    #[test]
    fn pairing_window_is_visible_only_until_leave_and_locks_after_five_failures() {
        let mut pairing = PairingWindow::default();
        pairing.enter(*b"1234");
        assert_eq!(pairing.code(), Some(*b"1234"));
        for _ in 0..LAN_PAIRING_MAX_FAILURES - 1 {
            assert_eq!(
                pairing.claim("0000"),
                Err(LanAccessError::PairingCodeInvalid)
            );
        }
        assert_eq!(pairing.claim("0000"), Err(LanAccessError::PairingLocked));
        assert!(!pairing.is_active());

        pairing.enter(*b"5678");
        assert_eq!(pairing.claim("5678"), Ok(()));
        assert!(pairing.is_active());
        pairing.leave();
        assert_eq!(pairing.claim("5678"), Err(LanAccessError::PairingInactive));
    }

    #[test]
    fn lease_is_exclusive_until_expired_or_released() {
        let mut state = LanLeaseState::default();
        let first = [1; LAN_LEASE_ID_BYTES];
        let second = [2; LAN_LEASE_ID_BYTES];
        assert!(state.create(100, first).is_ok());
        assert_eq!(state.create(200, second), Err(LanAccessError::LeaseBusy));
        assert!(state.heartbeat(200, first).is_ok());
        assert_eq!(
            state.require(200, second),
            Err(LanAccessError::LeaseRequired)
        );
        assert_eq!(
            state.require(30_201, first),
            Err(LanAccessError::LeaseRequired)
        );
        assert!(state.create(30_201, second).is_ok());
    }

    #[test]
    fn pna_never_grants_an_untrusted_origin() {
        assert_eq!(
            cors_allow_origin(Some(PROD_ALLOWED_ORIGIN)),
            Some(PROD_ALLOWED_ORIGIN)
        );
        assert!(private_network_preflight(Some(PROD_ALLOWED_ORIGIN), true).allow_private_network);
        assert!(
            !private_network_preflight(Some("https://attacker.invalid"), true)
                .allow_private_network
        );
        assert_eq!(LanEndpoint::Runtime.permission(), EndpointPermission::Lease);
        assert_eq!(LanEndpoint::Status.permission(), EndpointPermission::Token);
    }
}
