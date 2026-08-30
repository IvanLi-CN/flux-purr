//! HTTP v1 request gate shared by the ESP TCP listener and host tests.
//!
//! This module intentionally does not own heater, PD, or calibration state.
//! A transport can only enqueue a normalized command after it passes pairing,
//! bearer-token, CORS/PNA, and LAN-lease checks.

use core::fmt::Write as _;

use heapless::String;
use serde::Deserialize;

use crate::lan::{
    LAN_LEASE_ID_BYTES, LAN_TOKEN_HEX_LEN, LanAccessError, LanEndpoint, LanLease, LanLeaseState,
    LanPairingMode, LanToken, PairingWindow, cors_allow_origin, private_network_preflight,
};

pub const HTTP_API_VERSION: &str = "v1";
pub const HTTP_SERVICE_TYPE: &str = "_http._tcp.local";
pub const HTTP_SERVICE_PORT: u16 = 80;
pub const HTTP_SERVICE_TXT: [&str; 3] = ["api=v1", "path=/api/v1", "pairing=frontpanel"];
/// The TCP listener owns one socket per concurrent in-flight connection.
/// Snapshot-backed reads do not allocate a mutation workspace. Two reader
/// slots keep a status/event stream independent from the lease heartbeat while
/// the single mutation workspace serializes writes.
pub const fn http_socket_slot_count(active_request_budget: usize) -> usize {
    active_request_budget.saturating_add(2)
}

/// Request workspaces carry the bounded request and response bodies. They are
/// deliberately limited to mutation-capable work; lightweight reads use
/// published snapshots and do not consume this capacity.
pub const fn http_workspace_slot_count(active_request_budget: usize) -> usize {
    active_request_budget
}

/// The largest supported LAN mutation is the fully materialized nine-point
/// thermal profile (5,401 bytes). Six KiB preserves protocol headroom while
/// preventing each HTTP workspace copy from exhausting internal RAM.
pub const LAN_HTTP_BODY_MAX_LEN: usize = 6 * 1024;
/// Public and snapshot-backed reads never need the mutation-sized envelope.
/// Keeping this bounded separately lets the TCP adapter serve concurrent
/// low-frequency readers without duplicating the six-KiB write workspace.
pub const LAN_HTTP_LIGHT_BODY_MAX_LEN: usize = 512;
/// Largest complete HTTP header emitted by the LAN server. It covers a
/// 128-byte development origin, PNA approval, and an optimistic revision.
pub const HTTP_RESPONSE_HEADER_MAX_LEN: usize = 640;

#[cfg(any(test, target_arch = "xtensa"))]
pub(crate) fn format_http_response_headers(
    response_status: u16,
    body_len: usize,
    allow_origin: Option<&str>,
    allow_private_network: bool,
    control_revision: Option<u32>,
    content_type: &str,
) -> String<HTTP_RESPONSE_HEADER_MAX_LEN> {
    let status = match response_status {
        200 => "200 OK",
        204 => "204 No Content",
        400 => "400 Bad Request",
        401 => "401 Unauthorized",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        405 => "405 Method Not Allowed",
        409 => "409 Conflict",
        428 => "428 Precondition Required",
        429 => "429 Too Many Requests",
        503 => "503 Service Unavailable",
        504 => "504 Gateway Timeout",
        _ => "500 Internal Server Error",
    };
    let mut header = String::new();
    write!(
        header,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {body_len}\r\nConnection: close\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-Flux-Purr-Lease, X-Flux-Purr-Revision\r\nAccess-Control-Expose-Headers: X-Flux-Purr-Revision\r\nAccess-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n",
    )
    .expect("HTTP response base headers fit the fixed buffer");
    if let Some(revision) = control_revision {
        write!(header, "X-Flux-Purr-Revision: {revision}\r\n")
            .expect("HTTP response revision header fits the fixed buffer");
    }
    if content_type == "text/event-stream" {
        header
            .push_str("Cache-Control: no-cache\r\n")
            .expect("HTTP response SSE header fits the fixed buffer");
    }
    if let Some(origin) = allow_origin {
        write!(
            header,
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\n"
        )
        .expect("HTTP response CORS header fits the fixed buffer");
    }
    if allow_private_network {
        header
            .push_str("Access-Control-Allow-Private-Network: true\r\n")
            .expect("HTTP response PNA header fits the fixed buffer");
    }
    header
        .push_str("\r\n")
        .expect("HTTP response header terminator fits the fixed buffer");
    header
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOrigin {
    Usb,
    Lan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlMailboxCommand {
    /// Assigned by the TCP task immediately before the command enters the
    /// mailbox. A zero value is reserved for synchronous host-test dispatch.
    pub request_id: u32,
    /// Identifies the HTTP worker that owns the response socket.
    pub response_slot: u8,
    pub origin: CommandOrigin,
    pub endpoint: LanEndpoint,
    pub method: HttpMethod,
    /// LAN write commands retain the lease that authorized enqueueing. The
    /// front-panel executor validates it again before touching hardware so a
    /// timed-out request cannot outlive lease ownership.
    pub lease_id: Option<[u8; LAN_LEASE_ID_BYTES]>,
    /// Optimistic concurrency precondition supplied by the client for writes.
    pub expected_revision: Option<u32>,
    /// Cursor used by the read-only thermal-plant trace endpoint.
    pub after_sample: Option<u8>,
    /// Exclusive cursor used by the thermal-tuning trace endpoint.
    pub after_sequence: Option<u64>,
    pub body: String<LAN_HTTP_BODY_MAX_LEN>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMailboxError {
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRevisionError {
    Missing,
    Stale,
}

pub const fn validate_control_revision(
    expected: Option<u32>,
    current: u32,
) -> Result<(), ControlRevisionError> {
    match expected {
        None => Err(ControlRevisionError::Missing),
        Some(value) if value != current => Err(ControlRevisionError::Stale),
        Some(_) => Ok(()),
    }
}

pub trait ControlMailbox {
    fn submit(
        &mut self,
        command: ControlMailboxCommand,
    ) -> Result<String<LAN_HTTP_BODY_MAX_LEN>, ControlMailboxError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceNames {
    pub mac: [u8; 6],
    pub device_id: [u8; 12],
    pub hostname: String<32>,
}

pub fn device_names_from_mac(mac: [u8; 6]) -> DeviceNames {
    let mut device_id = [0u8; 12];
    let mut hostname = String::<32>::new();
    let _ = hostname.push_str("flux-purr-");
    for (index, byte) in mac.iter().enumerate() {
        let encoded = byte_to_hex(*byte);
        device_id[index * 2] = encoded[0];
        device_id[index * 2 + 1] = encoded[1];
        let _ = hostname.push(encoded[0] as char);
        let _ = hostname.push(encoded[1] as char);
    }
    DeviceNames {
        mac,
        device_id,
        hostname,
    }
}

/// Builds the LAN-facing identity from the station MAC rather than the
/// development USB placeholder. Pairing, mDNS, and authenticated LAN probes
/// must expose the same stable identity to keep client registries collision-free.
pub fn identity_from_device_names(names: &DeviceNames) -> crate::control_plane::Identity {
    crate::control_plane::Identity::firmware_from_mac(names.mac)
}

#[derive(Debug, Clone, Copy)]
pub struct HttpRequest<'a> {
    pub method: HttpMethod,
    pub path: &'a str,
    pub origin: Option<&'a str>,
    pub authorization: Option<&'a str>,
    pub lease_id: Option<&'a str>,
    pub expected_revision: Option<u32>,
    pub after_sample: Option<u8>,
    pub after_sequence: Option<u64>,
    pub request_private_network: bool,
    pub body: &'a str,
    /// Entropy produced by the hardware RNG. It is never emitted in responses.
    ///
    /// A pairing claim consumes all 256 bits. Lease IDs consume the first 96
    /// bits, which keeps the wire format compact without weakening the stored
    /// bearer credential.
    pub entropy: [u8; crate::lan::LAN_TOKEN_BYTES],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub allow_origin: Option<String<128>>,
    pub allow_private_network: bool,
    pub control_revision: Option<u32>,
    pub body: String<LAN_HTTP_BODY_MAX_LEN>,
}

/// A compact response for endpoints that can be answered from immutable LAN
/// state without entering the main control mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightHttpResponse {
    pub status: u16,
    pub allow_origin: Option<String<128>>,
    pub allow_private_network: bool,
    pub body: String<LAN_HTTP_LIGHT_BODY_MAX_LEN>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpReadGate {
    Respond,
    Snapshot {
        endpoint: LanEndpoint,
        allow_origin: Option<String<128>>,
    },
    Defer,
}

/// The result of protocol-level request handling before a firmware command is
/// executed. The TCP task must queue [`Dispatch`] to the main control loop;
/// it must never touch heater, PD, calibration, or EEPROM peripherals itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpGate {
    Respond(HttpResponse),
    Dispatch {
        command: ControlMailboxCommand,
        allow_origin: Option<String<128>>,
    },
}

impl HttpResponse {
    pub fn new(status: u16, body: &str) -> Self {
        let mut output = String::new();
        let _ = output.push_str(body);
        Self {
            status,
            allow_origin: None,
            allow_private_network: false,
            control_revision: None,
            body: output,
        }
    }

    pub fn json(status: u16, body: String<LAN_HTTP_BODY_MAX_LEN>) -> Self {
        Self {
            status,
            allow_origin: None,
            allow_private_network: false,
            control_revision: None,
            body,
        }
    }
}

#[derive(Debug, Default)]
pub struct NetHttpState {
    token: Option<LanToken>,
    pairing: PairingWindow,
    pairing_mode: LanPairingMode,
    leases: LanLeaseState,
    token_dirty: bool,
    device_names: Option<DeviceNames>,
}

impl NetHttpState {
    pub fn new(persisted_token: Option<[u8; crate::lan::LAN_TOKEN_BYTES]>) -> Self {
        Self {
            token: persisted_token.map(LanToken::new),
            pairing: PairingWindow::inactive(),
            pairing_mode: LanPairingMode::Required,
            leases: LanLeaseState::default(),
            token_dirty: false,
            device_names: None,
        }
    }

    pub fn set_device_names(&mut self, names: DeviceNames) {
        self.device_names = Some(names);
    }

    pub fn pairing_code_from_random(&mut self, random: u32) -> Option<[u8; 4]> {
        if self.pairing_mode != LanPairingMode::Required {
            self.pairing.leave();
            return None;
        }
        let mut code = [b'0'; 4];
        let mut value = random;
        for digit in code.iter_mut().rev() {
            *digit = b'0' + (value % 10) as u8;
            value /= 10;
        }
        self.pairing.enter(code);
        Some(code)
    }

    pub fn leave_pairing(&mut self) {
        self.pairing.leave();
    }

    pub const fn pairing_code(&self) -> Option<[u8; 4]> {
        self.pairing.code()
    }

    /// The firmware currently initializes this to `required`. Keeping the
    /// policy explicit here lets a future physical configuration select a
    /// code-exempt or code-unavailable device without changing HTTP clients.
    pub fn set_pairing_mode(&mut self, mode: LanPairingMode) {
        self.pairing_mode = mode;
        if mode != LanPairingMode::Required {
            self.pairing.leave();
        }
    }

    pub const fn persisted_token(&self) -> Option<[u8; crate::lan::LAN_TOKEN_BYTES]> {
        match self.token {
            Some(token) => Some(token.bytes()),
            None => None,
        }
    }

    /// Only the USB/devd control path may call this method.
    pub fn clear_token_from_usb(&mut self) {
        self.token = None;
        self.leases = LanLeaseState::default();
        self.token_dirty = true;
    }

    /// Returns a changed EEPROM value exactly once. This lets the main loop
    /// own persistence even though pairing claims arrive on the LAN task.
    pub fn take_persisted_token_change(
        &mut self,
    ) -> Option<Option<[u8; crate::lan::LAN_TOKEN_BYTES]>> {
        if !self.token_dirty {
            return None;
        }
        self.token_dirty = false;
        Some(self.persisted_token())
    }

    pub fn handle<M: ControlMailbox>(
        &mut self,
        now_ms: u64,
        request: HttpRequest<'_>,
        mailbox: &mut M,
    ) -> HttpResponse {
        match self.gate(now_ms, request) {
            HttpGate::Respond(response) => response,
            HttpGate::Dispatch {
                command,
                allow_origin,
            } => match mailbox.submit(command) {
                Ok(body) => {
                    let mut response = HttpResponse::json(200, body);
                    response.allow_origin = allow_origin;
                    response
                }
                Err(ControlMailboxError::Busy) => {
                    let mut response = HttpResponse::new(
                        503,
                        r#"{"error":{"code":"control_busy","message":"Control mailbox is busy."}}"#,
                    );
                    response.allow_origin = allow_origin;
                    response
                }
            },
        }
    }

    /// Admit only small, read-only routes to the concurrent snapshot path.
    /// Everything that can mutate state or needs a full control response stays
    /// on the existing mailbox path.
    pub fn gate_light_read(
        &mut self,
        request: HttpRequest<'_>,
        response: &mut LightHttpResponse,
    ) -> HttpReadGate {
        if request.method == HttpMethod::Options {
            *response = self.light_preflight(request);
            return HttpReadGate::Respond;
        }
        if request.method != HttpMethod::Get {
            return HttpReadGate::Defer;
        }
        let Some(endpoint) = endpoint_for_path(request.path) else {
            return HttpReadGate::Defer;
        };
        match endpoint {
            LanEndpoint::Health => {
                *response = self.light_health_response(request);
                HttpReadGate::Respond
            }
            LanEndpoint::Pairing => {
                *response = self.light_pairing_response(request);
                HttpReadGate::Respond
            }
            LanEndpoint::Identity | LanEndpoint::Network => {
                if self.authorized(request.authorization) {
                    HttpReadGate::Snapshot {
                        endpoint,
                        allow_origin: self.cors_origin(request.origin),
                    }
                } else {
                    *response = self.light_error_response(
                        request.origin,
                        401,
                        r#"{"error":{"code":"unauthorized","message":"Bearer token required."}}"#,
                    );
                    HttpReadGate::Respond
                }
            }
            _ => HttpReadGate::Defer,
        }
    }

    /// Apply CORS/PNA, pairing, bearer authentication, and lease policy.
    /// Successful control requests are returned as a normalized mailbox
    /// command so an async transport can enqueue and await main-loop handling.
    pub fn gate(&mut self, now_ms: u64, request: HttpRequest<'_>) -> HttpGate {
        if request.method == HttpMethod::Options {
            return HttpGate::Respond(self.with_cors(request.origin, self.preflight(request)));
        }

        let response = self.dispatch_gate(now_ms, request);
        match response {
            HttpGate::Respond(response) => {
                HttpGate::Respond(self.with_cors(request.origin, response))
            }
            HttpGate::Dispatch { command, .. } => HttpGate::Dispatch {
                command,
                allow_origin: self.cors_origin(request.origin),
            },
        }
    }

    fn cors_origin(&self, origin: Option<&str>) -> Option<String<128>> {
        let origin = cors_allow_origin(origin)?;
        let mut value = String::new();
        let _ = value.push_str(origin);
        Some(value)
    }

    fn with_cors(&self, origin: Option<&str>, mut response: HttpResponse) -> HttpResponse {
        response.allow_origin = self.cors_origin(origin);
        response
    }

    fn preflight(&self, request: HttpRequest<'_>) -> HttpResponse {
        let policy = private_network_preflight(request.origin, request.request_private_network);
        if !policy.allow_origin {
            return HttpResponse::new(403, r#"{"error":"origin_not_allowed"}"#);
        }
        let mut response = HttpResponse::new(204, "");
        let mut origin = String::new();
        let _ = origin.push_str(request.origin.unwrap_or_default());
        response.allow_origin = Some(origin);
        response.allow_private_network = policy.allow_private_network;
        response
    }

    fn light_preflight(&self, request: HttpRequest<'_>) -> LightHttpResponse {
        let policy = private_network_preflight(request.origin, request.request_private_network);
        if !policy.allow_origin {
            return self.light_error_response(
                request.origin,
                403,
                r#"{"error":"origin_not_allowed"}"#,
            );
        }
        LightHttpResponse {
            status: 204,
            allow_origin: self.cors_origin(request.origin),
            allow_private_network: policy.allow_private_network,
            body: String::new(),
        }
    }

    fn light_health_response(&self, request: HttpRequest<'_>) -> LightHttpResponse {
        let mut body = String::new();
        self.write_public_health(&mut body);
        LightHttpResponse {
            status: 200,
            allow_origin: self.cors_origin(request.origin),
            allow_private_network: false,
            body,
        }
    }

    fn light_pairing_response(&self, request: HttpRequest<'_>) -> LightHttpResponse {
        let mut body = String::new();
        self.write_pairing_metadata(&mut body);
        LightHttpResponse {
            status: 200,
            allow_origin: self.cors_origin(request.origin),
            allow_private_network: false,
            body,
        }
    }

    fn light_error_response(
        &self,
        origin: Option<&str>,
        status: u16,
        message: &str,
    ) -> LightHttpResponse {
        let mut body = String::new();
        let _ = body.push_str(message);
        LightHttpResponse {
            status,
            allow_origin: self.cors_origin(origin),
            allow_private_network: false,
            body,
        }
    }

    fn dispatch_gate(&mut self, now_ms: u64, request: HttpRequest<'_>) -> HttpGate {
        let Some(endpoint) = endpoint_for_path(request.path) else {
            return HttpGate::Respond(HttpResponse::new(
                404,
                r#"{"error":{"code":"not_found","message":"Unknown API path."}}"#,
            ));
        };
        if !endpoint_allows_method(endpoint, request.method) {
            return HttpGate::Respond(HttpResponse::new(405, r#"{"error":"method_not_allowed"}"#));
        }
        if endpoint == LanEndpoint::Health {
            return HttpGate::Respond(self.public_health_response());
        }
        if endpoint == LanEndpoint::Pairing && request.method == HttpMethod::Get {
            return HttpGate::Respond(self.pairing_metadata_response());
        }
        if endpoint == LanEndpoint::PairingClaim && request.method == HttpMethod::Post {
            return HttpGate::Respond(self.claim_pairing(request));
        }

        if !self.authorized(request.authorization) {
            return HttpGate::Respond(HttpResponse::new(
                401,
                r#"{"error":{"code":"unauthorized","message":"Bearer token required."}}"#,
            ));
        }
        if endpoint == LanEndpoint::Lease {
            return HttpGate::Respond(self.lease_route(now_ms, request));
        }
        let lease_id = if is_write(request.method) {
            match self.require_lease(now_ms, request.lease_id) {
                Ok(id) => Some(id),
                Err(_) => {
                    return HttpGate::Respond(HttpResponse::new(
                        409,
                        r#"{"error":{"code":"lease_required","message":"An active LAN lease is required for writes."}}"#,
                    ));
                }
            }
        } else {
            None
        };
        if is_control_write(request.method, endpoint) && request.expected_revision.is_none() {
            return HttpGate::Respond(HttpResponse::new(
                428,
                r#"{"error":{"code":"revision_required","message":"A current control revision is required for writes."}}"#,
            ));
        }
        let mut body = String::new();
        if endpoint == LanEndpoint::ThermalTuningRun && request.method == HttpMethod::Get {
            if let Some((_, query)) = request.path.split_once('?') {
                let _ = body.push_str(query);
            }
        } else {
            let _ = body.push_str(request.body);
        }
        HttpGate::Dispatch {
            command: ControlMailboxCommand {
                request_id: 0,
                response_slot: 0,
                origin: CommandOrigin::Lan,
                endpoint,
                method: request.method,
                lease_id,
                expected_revision: request.expected_revision,
                after_sample: request.after_sample,
                after_sequence: request.after_sequence,
                body,
            },
            allow_origin: None,
        }
    }

    fn claim_pairing(&mut self, request: HttpRequest<'_>) -> HttpResponse {
        match self.pairing_mode {
            LanPairingMode::Required => {
                let Some(code) = pairing_claim_code(request.body) else {
                    return access_error_response(LanAccessError::PairingCodeInvalid);
                };
                if let Err(error) = self.pairing.claim(code) {
                    return access_error_response(error);
                }
            }
            LanPairingMode::Optional => {}
            LanPairingMode::Unavailable => {
                return access_error_response(LanAccessError::PairingUnavailable);
            }
        }
        let token = match self.token {
            Some(token) => token,
            None => {
                let token = LanToken::new(request.entropy);
                self.token = Some(token);
                self.token_dirty = true;
                token
            }
        };
        let mut hex = String::<LAN_TOKEN_HEX_LEN>::new();
        token.write_hex(&mut hex);
        let mut body = String::new();
        let _ = write!(body, r#"{{"token":"{}","api":"v1""#, hex);
        if let Some(names) = &self.device_names {
            let device_id = core::str::from_utf8(&names.device_id).unwrap_or_default();
            let _ = write!(
                body,
                r#","deviceId":"{}","hostname":"{}""#,
                device_id, names.hostname
            );
        }
        let _ = body.push('}');
        HttpResponse::json(200, body)
    }

    /// Anonymous clients may poll this deliberately small summary at a low
    /// rate. It proves which local device answered and communicates how a
    /// bearer can be obtained, but never exposes operational status, a token,
    /// or the front-panel pairing code.
    fn public_health_response(&self) -> HttpResponse {
        let mut body = String::new();
        self.write_public_health(&mut body);
        HttpResponse::json(200, body)
    }

    fn pairing_metadata_response(&self) -> HttpResponse {
        let mut body = String::new();
        self.write_pairing_metadata(&mut body);
        HttpResponse::json(200, body)
    }

    fn write_public_health(&self, body: &mut impl core::fmt::Write) {
        let identity = self
            .device_names
            .as_ref()
            .map(identity_from_device_names)
            .unwrap_or_else(crate::control_plane::Identity::firmware_default);
        let attempts_remaining = 5u8.saturating_sub(self.pairing.failed_attempts());
        let _ = write!(
            body,
            r#"{{"ok":true,"api":"v1","deviceId":"{}","hostname":"{}","firmwareVersion":"{}","pairing":{{"mode":"{}","active":{},"attemptsRemaining":{}}}}}"#,
            identity.device_id,
            identity.hostname,
            identity.firmware_version,
            self.pairing_mode.as_wire(),
            self.pairing.is_active(),
            attempts_remaining
        );
    }

    fn write_pairing_metadata(&self, body: &mut impl core::fmt::Write) {
        let attempts_remaining = 5u8.saturating_sub(self.pairing.failed_attempts());
        let _ = write!(
            body,
            r#"{{"mode":"{}","active":{},"attemptsRemaining":{}}}"#,
            self.pairing_mode.as_wire(),
            self.pairing_mode == LanPairingMode::Required && self.pairing.is_active(),
            attempts_remaining
        );
    }

    fn lease_route(&mut self, now_ms: u64, request: HttpRequest<'_>) -> HttpResponse {
        let parsed = request.lease_id.and_then(parse_lease_id);
        let result = match request.method {
            HttpMethod::Post => self.leases.create(now_ms, lease_entropy(request.entropy)),
            HttpMethod::Put => parsed.map_or(Err(LanAccessError::LeaseRequired), |id| {
                self.leases.heartbeat(now_ms, id)
            }),
            HttpMethod::Delete => {
                return match parsed.map_or(Err(LanAccessError::LeaseRequired), |id| {
                    self.leases.release(id)
                }) {
                    Ok(()) => HttpResponse::new(200, r#"{"released":true}"#),
                    Err(error) => access_error_response(error),
                };
            }
            _ => return HttpResponse::new(405, r#"{"error":"method_not_allowed"}"#),
        };
        match result {
            Ok(lease) => lease_response(lease),
            Err(error) => access_error_response(error),
        }
    }

    fn authorized(&self, header: Option<&str>) -> bool {
        self.token
            .zip(header.and_then(|value| value.strip_prefix("Bearer ")))
            .is_some_and(|(token, bearer)| token.matches_bearer(bearer))
    }

    pub fn command_lease_is_active(
        &mut self,
        now_ms: u64,
        lease_id: [u8; LAN_LEASE_ID_BYTES],
    ) -> bool {
        self.leases.require(now_ms, lease_id).is_ok()
    }

    fn require_lease(
        &mut self,
        now_ms: u64,
        value: Option<&str>,
    ) -> Result<[u8; LAN_LEASE_ID_BYTES], LanAccessError> {
        let id = value
            .and_then(parse_lease_id)
            .ok_or(LanAccessError::LeaseRequired)?;
        self.leases.require(now_ms, id)?;
        Ok(id)
    }
}

fn endpoint_for_path(path: &str) -> Option<LanEndpoint> {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    match path {
        "/health" => Some(LanEndpoint::Health),
        "/api/v1/pairing" => Some(LanEndpoint::Pairing),
        "/api/v1/pairing/claim" => Some(LanEndpoint::PairingClaim),
        "/api/v1/leases" => Some(LanEndpoint::Lease),
        "/api/v1/identity" => Some(LanEndpoint::Identity),
        "/api/v1/network" => Some(LanEndpoint::Network),
        "/api/v1/status" => Some(LanEndpoint::Status),
        "/api/v1/events" => Some(LanEndpoint::Events),
        "/api/v1/runtime" => Some(LanEndpoint::Runtime),
        "/api/v1/calibration" => Some(LanEndpoint::Calibration),
        "/api/v1/calibration/job" => Some(LanEndpoint::CalibrationJob),
        "/api/v1/calibration/thermal-plant/run" => Some(LanEndpoint::ThermalPlantRun),
        "/api/v1/calibration/thermal-tuning/run" => Some(LanEndpoint::ThermalTuningRun),
        "/api/v1/heater-curve" => Some(LanEndpoint::HeaterCurve),
        "/api/v1/heater-curve/save" => Some(LanEndpoint::HeaterCurveSave),
        "/api/v1/thermal-profile" => Some(LanEndpoint::ThermalProfile),
        _ => None,
    }
}

fn endpoint_allows_method(endpoint: LanEndpoint, method: HttpMethod) -> bool {
    matches!(
        (endpoint, method),
        (LanEndpoint::Health, HttpMethod::Get)
            | (LanEndpoint::Pairing, HttpMethod::Get)
            | (LanEndpoint::PairingClaim, HttpMethod::Post)
            | (
                LanEndpoint::Lease,
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Delete
            )
            | (
                LanEndpoint::Identity
                    | LanEndpoint::Network
                    | LanEndpoint::Status
                    | LanEndpoint::Events,
                HttpMethod::Get
            )
            | (
                LanEndpoint::Runtime | LanEndpoint::ThermalProfile,
                HttpMethod::Put
            )
            | (
                LanEndpoint::Calibration | LanEndpoint::HeaterCurve,
                HttpMethod::Get | HttpMethod::Put
            )
            | (
                LanEndpoint::CalibrationJob,
                HttpMethod::Get | HttpMethod::Post
            )
            | (LanEndpoint::ThermalPlantRun, HttpMethod::Get)
            | (
                LanEndpoint::ThermalTuningRun,
                HttpMethod::Get | HttpMethod::Post
            )
            | (LanEndpoint::HeaterCurveSave, HttpMethod::Post)
    )
}

fn is_write(method: HttpMethod) -> bool {
    matches!(
        method,
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Delete
    )
}

fn is_control_write(method: HttpMethod, endpoint: LanEndpoint) -> bool {
    is_write(method) && endpoint != LanEndpoint::Lease && endpoint != LanEndpoint::PairingClaim
}

fn access_error_response(error: LanAccessError) -> HttpResponse {
    let (status, code, message) = match error {
        LanAccessError::PairingInactive => (
            403,
            "pairing_inactive",
            "Pairing is only available while WiFi Info is visible.",
        ),
        LanAccessError::PairingLocked => (
            429,
            "pairing_locked",
            "Pairing has reached the failed-attempt limit.",
        ),
        LanAccessError::PairingCodeInvalid => {
            (400, "pairing_code_invalid", "The pairing code is invalid.")
        }
        LanAccessError::PairingUnavailable => (
            403,
            "pairing_unavailable",
            "This device does not support LAN pairing codes.",
        ),
        LanAccessError::Unauthorized => (401, "unauthorized", "Bearer token required."),
        LanAccessError::LeaseBusy => (409, "lease_busy", "Another LAN client owns the lease."),
        LanAccessError::LeaseRequired => {
            (409, "lease_required", "An active LAN lease is required.")
        }
        LanAccessError::LeaseExpired => (409, "lease_expired", "The LAN lease has expired."),
    };
    let mut body = String::new();
    let _ = write!(
        body,
        r#"{{"error":{{"code":"{}","message":"{}"}}}}"#,
        code, message
    );
    HttpResponse::json(status, body)
}

fn lease_response(lease: LanLease) -> HttpResponse {
    let mut id = String::<{ LAN_LEASE_ID_BYTES * 2 }>::new();
    for byte in lease.id {
        let _ = write!(id, "{byte:02x}");
    }
    let mut body = String::new();
    let _ = write!(body, r#"{{"leaseId":"{}","ttlMs":30000}}"#, id);
    HttpResponse::json(200, body)
}

fn byte_to_hex(byte: u8) -> [u8; 2] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    [HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]
}

fn lease_entropy(entropy: [u8; crate::lan::LAN_TOKEN_BYTES]) -> [u8; LAN_LEASE_ID_BYTES] {
    let mut lease_entropy = [0u8; LAN_LEASE_ID_BYTES];
    lease_entropy.copy_from_slice(&entropy[..LAN_LEASE_ID_BYTES]);
    lease_entropy
}

fn parse_lease_id(value: &str) -> Option<[u8; LAN_LEASE_ID_BYTES]> {
    if value.len() != LAN_LEASE_ID_BYTES * 2 {
        return None;
    }
    let mut id = [0; LAN_LEASE_ID_BYTES];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte =
            nibble(value.as_bytes()[index * 2])? << 4 | nibble(value.as_bytes()[index * 2 + 1])?;
    }
    Some(id)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Deserialize)]
struct PairingClaimWire<'a> {
    code: &'a str,
}

fn pairing_claim_code(body: &str) -> Option<&str> {
    serde_json_core::from_str::<PairingClaimWire<'_>>(body)
        .ok()
        .map(|(value, _)| value.code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestMailbox {
        calls: usize,
        last_command: Option<ControlMailboxCommand>,
    }
    impl ControlMailbox for TestMailbox {
        fn submit(
            &mut self,
            command: ControlMailboxCommand,
        ) -> Result<String<LAN_HTTP_BODY_MAX_LEN>, ControlMailboxError> {
            self.calls += 1;
            self.last_command = Some(command);
            let mut value = String::new();
            value.push_str(r#"{"ok":true}"#).unwrap();
            Ok(value)
        }
    }
    fn req(method: HttpMethod, path: &str) -> HttpRequest<'_> {
        HttpRequest {
            method,
            path,
            origin: Some("https://flux-purr.ivanli.cc"),
            authorization: None,
            lease_id: None,
            expected_revision: None,
            after_sample: None,
            after_sequence: None,
            request_private_network: false,
            body: "",
            entropy: [7; crate::lan::LAN_TOKEN_BYTES],
        }
    }

    #[test]
    fn mac_names_and_mdns_contract_are_stable() {
        let names = device_names_from_mac([0, 1, 2, 3, 4, 5]);
        assert_eq!(names.hostname.as_str(), "flux-purr-000102030405");
        assert_eq!(HTTP_SERVICE_TYPE, "_http._tcp.local");
        assert!(HTTP_SERVICE_TXT.contains(&"api=v1"));
    }

    #[test]
    fn cors_pna_response_headers_are_complete_for_direct_browser_control() {
        let header = format_http_response_headers(
            200,
            2_710,
            Some("http://127.0.0.1:18091"),
            true,
            Some(u32::MAX),
            "application/json",
        );

        assert!(header.contains("Content-Length: 2710\r\n"));
        assert!(header.contains("Access-Control-Allow-Origin: http://127.0.0.1:18091\r\n"));
        assert!(header.contains("Access-Control-Allow-Private-Network: true\r\n"));
        assert!(header.ends_with("\r\n\r\n"));
        assert!(header.len() <= HTTP_RESPONSE_HEADER_MAX_LEN);
    }

    #[test]
    fn http_socket_budget_supports_two_reads_with_one_mutation_workspace() {
        assert_eq!(http_socket_slot_count(1), 3);
        assert_eq!(http_workspace_slot_count(1), 1);
    }

    #[test]
    fn lan_identity_uses_the_mac_derived_device_name() {
        let identity = identity_from_device_names(&device_names_from_mac([0, 17, 34, 51, 68, 85]));

        assert_eq!(identity.device_id.as_str(), "001122334455");
        assert_eq!(identity.hostname.as_str(), "flux-purr-001122334455");
    }

    #[test]
    fn pairing_is_frontpanel_scoped_and_persists_one_token() {
        let mut state = NetHttpState::new(None);
        let mut mailbox = TestMailbox::default();
        state.set_device_names(device_names_from_mac([0, 17, 34, 51, 68, 85]));
        let inactive = state.handle(
            0,
            HttpRequest {
                body: r#"{"code":"1234"}"#,
                ..req(HttpMethod::Post, "/api/v1/pairing/claim")
            },
            &mut mailbox,
        );
        assert_eq!(inactive.status, 403);
        assert_eq!(state.pairing_code_from_random(4827), Some(*b"4827"));
        let paired = state.handle(
            0,
            HttpRequest {
                body: r#"{"code":"4827"}"#,
                ..req(HttpMethod::Post, "/api/v1/pairing/claim")
            },
            &mut mailbox,
        );
        assert_eq!(paired.status, 200);
        assert!(paired.body.contains("token"));
        assert!(paired.body.contains(r#""api":"v1","deviceId""#));
        assert!(paired.body.contains("001122334455"));
        assert!(paired.body.contains("flux-purr-001122334455"));
        let token = state.persisted_token();
        assert!(token.is_some());
        state.leave_pairing();
        assert_eq!(
            state
                .handle(
                    0,
                    HttpRequest {
                        body: r#"{"code":"4827"}"#,
                        ..req(HttpMethod::Post, "/api/v1/pairing/claim")
                    },
                    &mut mailbox
                )
                .status,
            403
        );
    }

    #[test]
    fn public_health_exposes_basic_identity_and_required_pairing_without_bearer() {
        let mut state = NetHttpState::new(None);
        let mut mailbox = TestMailbox::default();
        state.set_device_names(device_names_from_mac([0, 17, 34, 51, 68, 85]));

        let health = state.handle(0, req(HttpMethod::Get, "/health"), &mut mailbox);

        assert_eq!(health.status, 200);
        assert!(health.body.contains(r#""deviceId":"001122334455""#));
        assert!(
            health
                .body
                .contains(r#""hostname":"flux-purr-001122334455""#)
        );
        assert!(health.body.contains(r#""mode":"required""#));
        assert_eq!(mailbox.calls, 0);
    }

    #[test]
    fn light_read_gate_keeps_snapshot_reads_out_of_the_control_mailbox() {
        let mut state = NetHttpState::new(Some([9; crate::lan::LAN_TOKEN_BYTES]));
        let bearer = "Bearer 0909090909090909090909090909090909090909090909090909090909090909";
        let mut response = LightHttpResponse {
            status: 500,
            allow_origin: None,
            allow_private_network: false,
            body: String::new(),
        };

        let health = state.gate_light_read(req(HttpMethod::Get, "/health"), &mut response);
        assert!(matches!(health, HttpReadGate::Respond));
        assert_eq!(response.status, 200);

        let unauthorized =
            state.gate_light_read(req(HttpMethod::Get, "/api/v1/network"), &mut response);
        assert!(matches!(unauthorized, HttpReadGate::Respond));
        assert_eq!(response.status, 401);

        let network = state.gate_light_read(
            HttpRequest {
                authorization: Some(bearer),
                ..req(HttpMethod::Get, "/api/v1/network")
            },
            &mut response,
        );
        assert!(matches!(
            network,
            HttpReadGate::Snapshot {
                endpoint: LanEndpoint::Network,
                ..
            }
        ));

        assert!(matches!(
            state.gate_light_read(
                HttpRequest {
                    authorization: Some(bearer),
                    ..req(HttpMethod::Get, "/api/v1/status")
                },
                &mut response,
            ),
            HttpReadGate::Defer
        ));
    }

    #[test]
    fn pairing_policy_distinguishes_required_optional_and_unavailable_claims() {
        let mut state = NetHttpState::new(None);
        let mut mailbox = TestMailbox::default();

        state.set_pairing_mode(LanPairingMode::Optional);
        assert_eq!(state.pairing_code_from_random(4827), None);
        let optional_metadata =
            state.handle(0, req(HttpMethod::Get, "/api/v1/pairing"), &mut mailbox);
        assert!(optional_metadata.body.contains(r#""mode":"optional""#));
        assert_eq!(
            state
                .handle(
                    0,
                    HttpRequest {
                        body: "{}",
                        ..req(HttpMethod::Post, "/api/v1/pairing/claim")
                    },
                    &mut mailbox
                )
                .status,
            200
        );

        state.set_pairing_mode(LanPairingMode::Unavailable);
        let unavailable_metadata =
            state.handle(0, req(HttpMethod::Get, "/api/v1/pairing"), &mut mailbox);
        assert!(
            unavailable_metadata
                .body
                .contains(r#""mode":"unavailable""#)
        );
        let unavailable_claim = state.handle(
            0,
            HttpRequest {
                body: "{}",
                ..req(HttpMethod::Post, "/api/v1/pairing/claim")
            },
            &mut mailbox,
        );
        assert_eq!(unavailable_claim.status, 403);
        assert!(unavailable_claim.body.contains("pairing_unavailable"));
    }

    #[test]
    fn pairing_claim_uses_structured_json_not_field_ordering() {
        let mut state = NetHttpState::new(None);
        let mut mailbox = TestMailbox::default();
        state.pairing_code_from_random(4827);

        let response = state.handle(
            0,
            HttpRequest {
                body: r#"{ "ignored": true, "code": "4827" }"#,
                ..req(HttpMethod::Post, "/api/v1/pairing/claim")
            },
            &mut mailbox,
        );

        assert_eq!(response.status, 200);
    }

    #[test]
    fn malformed_pairing_claim_uses_the_standard_error_envelope() {
        let mut state = NetHttpState::new(None);
        let mut mailbox = TestMailbox::default();
        state.pairing_code_from_random(4827);

        let response = state.handle(
            0,
            HttpRequest {
                body: r#"{"code":false}"#,
                ..req(HttpMethod::Post, "/api/v1/pairing/claim")
            },
            &mut mailbox,
        );

        assert_eq!(response.status, 400);
        assert_eq!(
            response.body.as_str(),
            r#"{"error":{"code":"pairing_code_invalid","message":"The pairing code is invalid."}}"#
        );
    }

    #[test]
    fn pna_and_mutations_require_origin_token_and_lease() {
        let mut state = NetHttpState::new(Some([9; crate::lan::LAN_TOKEN_BYTES]));
        let mut mailbox = TestMailbox::default();
        let preflight = state.handle(
            0,
            HttpRequest {
                method: HttpMethod::Options,
                request_private_network: true,
                ..req(HttpMethod::Options, "/api/v1/runtime")
            },
            &mut mailbox,
        );
        assert_eq!(preflight.status, 204);
        assert!(preflight.allow_private_network);
        assert_eq!(
            state
                .handle(0, req(HttpMethod::Get, "/api/v1/status"), &mut mailbox)
                .status,
            401
        );
        let mut token = String::<LAN_TOKEN_HEX_LEN>::new();
        LanToken::new([9; crate::lan::LAN_TOKEN_BYTES]).write_hex(&mut token);
        let lease = state.handle(
            0,
            HttpRequest {
                authorization: Some(
                    "Bearer 0909090909090909090909090909090909090909090909090909090909090909",
                ),
                ..req(HttpMethod::Post, "/api/v1/leases")
            },
            &mut mailbox,
        );
        assert_eq!(lease.status, 200);
        let id = lease.body.split('"').nth(3).unwrap();
        assert_eq!(state.handle(1, HttpRequest { authorization: Some("Bearer 0909090909090909090909090909090909090909090909090909090909090909"), lease_id: Some(id), expected_revision: Some(0), body: r#"{"targetTempC":120}"#, ..req(HttpMethod::Put, "/api/v1/runtime") }, &mut mailbox).status, 200);
        assert_eq!(mailbox.calls, 1);
        let command = mailbox.last_command.as_ref().expect("write was dispatched");
        let lease_id = parse_lease_id(id).expect("lease response is canonical hex");
        assert_eq!(command.lease_id, Some(lease_id));
        assert!(state.command_lease_is_active(1, lease_id));
        assert_eq!(
            state.handle(
                1,
                HttpRequest {
                    authorization: Some(
                        "Bearer 0909090909090909090909090909090909090909090909090909090909090909",
                    ),
                    lease_id: Some(id),
                    ..req(HttpMethod::Delete, "/api/v1/leases")
                },
                &mut mailbox,
            )
            .status,
            200
        );
        assert!(!state.command_lease_is_active(1, lease_id));
        let response = state.handle(
            1,
            HttpRequest {
                authorization: Some(
                    "Bearer 0909090909090909090909090909090909090909090909090909090909090909",
                ),
                ..req(HttpMethod::Get, "/api/v1/status")
            },
            &mut mailbox,
        );
        assert_eq!(
            response.allow_origin.as_deref(),
            Some("https://flux-purr.ivanli.cc")
        );
    }

    #[test]
    fn unsupported_methods_cannot_bypass_the_lan_lease() {
        let mut state = NetHttpState::new(Some([9; crate::lan::LAN_TOKEN_BYTES]));
        let mut mailbox = TestMailbox::default();
        let bearer = "Bearer 0909090909090909090909090909090909090909090909090909090909090909";

        let runtime = state.handle(
            0,
            HttpRequest {
                authorization: Some(bearer),
                body: r#"{"targetTempC":120}"#,
                ..req(HttpMethod::Get, "/api/v1/runtime")
            },
            &mut mailbox,
        );
        let heater_curve_save = state.handle(
            0,
            HttpRequest {
                authorization: Some(bearer),
                ..req(HttpMethod::Get, "/api/v1/heater-curve/save")
            },
            &mut mailbox,
        );

        assert_eq!(runtime.status, 405);
        assert_eq!(heater_curve_save.status, 405);
        assert_eq!(mailbox.calls, 0);
    }

    #[test]
    fn lan_writes_require_an_optimistic_revision_before_dispatch() {
        let mut state = NetHttpState::new(Some([9; crate::lan::LAN_TOKEN_BYTES]));
        let mut mailbox = TestMailbox::default();
        let bearer = "Bearer 0909090909090909090909090909090909090909090909090909090909090909";
        let lease = state.handle(
            0,
            HttpRequest {
                authorization: Some(bearer),
                ..req(HttpMethod::Post, "/api/v1/leases")
            },
            &mut mailbox,
        );
        let lease_id = lease.body.split('"').nth(3).unwrap();

        let missing = state.handle(
            1,
            HttpRequest {
                authorization: Some(bearer),
                lease_id: Some(lease_id),
                body: r#"{"targetTempC":120}"#,
                ..req(HttpMethod::Put, "/api/v1/runtime")
            },
            &mut mailbox,
        );

        assert_eq!(missing.status, 428);
        assert!(missing.body.contains("revision_required"));
        assert_eq!(mailbox.calls, 0);
    }

    #[test]
    fn stale_control_writes_are_rejected_before_hardware_execution() {
        assert_eq!(
            validate_control_revision(None, 7),
            Err(ControlRevisionError::Missing)
        );
        assert_eq!(
            validate_control_revision(Some(6), 7),
            Err(ControlRevisionError::Stale)
        );
        assert_eq!(validate_control_revision(Some(7), 7), Ok(()));
    }
}
