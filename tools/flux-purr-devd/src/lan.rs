//! Direct LAN client support. Credentials never leave this module in a
//! serializable response: callers receive `LanDeviceSummary` only.

use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

use ipnet::Ipv4Net;
use mdns_sd_discovery::{BrowseEvent, DiscoveredService, ServiceBrowserBuilder};
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const LAN_API_VERSION: &str = "v1";
pub const LAN_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
// Complete status/tuning bodies can take 4s on the device's bounded TCP
// buffers. Let a valid read finish instead of abandoning its socket while the
// MCU is still transmitting. Writes and queue admission retain the 3s bound.
pub const LAN_READ_REQUEST_TIMEOUT: Duration = Duration::from_secs(6);

// Leave a TCP slot available for another client or safety-stop request.
// This schedules requests, not control ownership; every client remains usable.
fn device_request_budget(origin: &str) -> Arc<tokio::sync::Semaphore> {
    static BUDGETS: OnceLock<Mutex<BTreeMap<String, Weak<tokio::sync::Semaphore>>>> =
        OnceLock::new();
    let mut budgets = BUDGETS
        .get_or_init(Mutex::default)
        .lock()
        .expect("LAN request budgets");
    budgets.retain(|_, budget| budget.strong_count() > 0);
    if let Some(budget) = budgets.get(origin).and_then(Weak::upgrade) {
        return budget;
    }
    let budget = Arc::new(tokio::sync::Semaphore::new(2));
    budgets.insert(origin.to_owned(), Arc::downgrade(&budget));
    budget
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanDeviceConfig {
    pub id: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ipv4: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing_token: Option<String>,
}

impl core::fmt::Debug for LanDeviceConfig {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LanDeviceConfig")
            .field("id", &self.id)
            .field("base_url", &self.base_url)
            .field("hostname", &self.hostname)
            .field("last_ipv4", &self.last_ipv4)
            .field("pairing_token", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanDeviceSummary {
    pub id: String,
    pub base_url: String,
    pub hostname: Option<String>,
    pub last_ipv4: Option<String>,
    pub paired: bool,
}

impl From<&LanDeviceConfig> for LanDeviceSummary {
    fn from(device: &LanDeviceConfig) -> Self {
        Self {
            id: device.id.clone(),
            base_url: device.base_url.clone(),
            hostname: device.hostname.clone(),
            last_ipv4: device.last_ipv4.clone(),
            paired: device.pairing_token.as_deref().is_some_and(is_token),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanPairRequest {
    pub base_url: String,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LanPairingMode {
    Required,
    Optional,
    Unavailable,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanPublicInfo {
    ok: bool,
    api: String,
    device_id: String,
    hostname: String,
    pairing: LanPairingMetadata,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanPairingMetadata {
    mode: LanPairingMode,
    active: bool,
    attempts_remaining: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanScanRequest {
    pub cidr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanDiscovery {
    pub base_url: String,
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub source: LanDiscoverySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LanDiscoverySource {
    Mdns,
    CidrScan,
}

#[derive(Debug, thiserror::Error)]
pub enum LanClientError {
    #[error("LAN URL must use HTTP with a private IPv4 address or Flux Purr .local hostname")]
    InvalidUrl,
    #[error("pairing code must contain exactly four digits")]
    InvalidCode,
    #[error("the connected device requires a four-digit pairing code")]
    PairingCodeRequired,
    #[error("the connected device does not support LAN pairing codes")]
    PairingUnavailable,
    #[error("device returned an invalid public LAN summary")]
    InvalidPublicInfo,
    #[error("device returned an invalid pairing response")]
    InvalidPairingResponse,
    #[error("device did not provide a valid control revision")]
    InvalidControlRevision,
    #[error("LAN device returned {status} {code}: {message}")]
    RemoteApi {
        status: reqwest::StatusCode,
        code: String,
        message: String,
        retryable: bool,
    },
    #[error("LAN request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("LAN discovery failed: {0}")]
    Discovery(String),
    #[error("LAN request queue is busy; retry shortly")]
    RequestQueueBusy,
    #[error("CIDR scan must use a private range containing at most 256 hosts")]
    InvalidCidr,
}

pub fn normalize_lan_base_url(raw: &str) -> Result<String, LanClientError> {
    let mut url = Url::parse(raw.trim()).map_err(|_| LanClientError::InvalidUrl)?;
    if url.scheme() != "http"
        || !url.host_str().is_some_and(is_private_lan_host)
        || !url.username().is_empty()
        || url.password().is_some()
        || (url.path() != "/" && !url.path().is_empty())
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(LanClientError::InvalidUrl);
    }
    url.set_path("");
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn is_private_lan_host(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    if let Ok(IpAddr::V4(address)) = host.parse::<IpAddr>() {
        return address.is_private();
    }
    host.strip_suffix(".local")
        .and_then(|hostname| hostname.strip_prefix("flux-purr-"))
        .is_some_and(is_device_id)
}

pub fn is_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn device_id_for_base_url(base_url: &str) -> String {
    let digest = Sha256::digest(base_url.as_bytes());
    format!(
        "lan-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5]
    )
}

pub fn http_client() -> Client {
    Client::builder()
        .timeout(LAN_REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .expect("valid fixed LAN client configuration")
}

pub async fn pair_device(request: LanPairRequest) -> Result<LanDeviceConfig, LanClientError> {
    let base_url = normalize_lan_base_url(&request.base_url)?;
    let public_info = public_info(&base_url).await?;
    let body = pairing_claim_body(public_info.pairing.mode, request.code)?;
    let response = http_client()
        .post(format!("{base_url}/api/v1/pairing/claim"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    let token = response
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| is_token(token))
        .ok_or(LanClientError::InvalidPairingResponse)?
        .to_owned();
    let hostname = response
        .get("hostname")
        .or_else(|| response.get("deviceHostname"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let last_ipv4 = Url::parse(&base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .filter(|host| host.parse::<Ipv4Addr>().is_ok());
    let id = response
        .get("deviceId")
        .and_then(Value::as_str)
        .filter(|device_id| is_device_id(device_id))
        .map(|device_id| format!("lan-{}", device_id.to_ascii_lowercase()))
        .unwrap_or_else(|| device_id_for_base_url(&base_url));
    Ok(LanDeviceConfig {
        id,
        base_url,
        hostname,
        last_ipv4,
        pairing_token: Some(token),
    })
}

async fn public_info(base_url: &str) -> Result<LanPublicInfo, LanClientError> {
    public_info_with_client(base_url, http_client()).await
}

async fn public_info_with_client(
    base_url: &str,
    client: Client,
) -> Result<LanPublicInfo, LanClientError> {
    let summary = client
        .get(format!("{base_url}/health"))
        .send()
        .await?
        .error_for_status()?
        .json::<LanPublicInfo>()
        .await?;
    if !summary.ok
        || summary.api != LAN_API_VERSION
        || !is_device_id(&summary.device_id)
        || summary.hostname.is_empty()
        || (summary.pairing.mode == LanPairingMode::Required
            && summary.pairing.attempts_remaining > 5)
        || (summary.pairing.mode != LanPairingMode::Required && summary.pairing.active)
    {
        return Err(LanClientError::InvalidPublicInfo);
    }
    Ok(summary)
}

/// Verify a USB-provided locator anonymously before sending its device bearer.
pub async fn verify_device_locator(
    base_url: &str,
    expected_device_id: &str,
) -> Result<(), LanClientError> {
    verify_device_locator_with_client(base_url, expected_device_id, http_client()).await
}

async fn verify_device_locator_with_client(
    base_url: &str,
    expected_device_id: &str,
    client: Client,
) -> Result<(), LanClientError> {
    let summary = public_info_with_client(base_url, client).await?;
    if summary.device_id != expected_device_id {
        return Err(LanClientError::InvalidPublicInfo);
    }
    Ok(())
}

fn pairing_claim_body(mode: LanPairingMode, code: Option<String>) -> Result<Value, LanClientError> {
    match mode {
        LanPairingMode::Required => {
            let code = code.ok_or(LanClientError::PairingCodeRequired)?;
            if code.len() != 4 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(LanClientError::InvalidCode);
            }
            Ok(json!({ "code": code }))
        }
        LanPairingMode::Optional => Ok(json!({})),
        LanPairingMode::Unavailable => Err(LanClientError::PairingUnavailable),
    }
}

pub async fn authorized_json(
    device: &LanDeviceConfig,
    method: reqwest::Method,
    path: &str,
    lease_id: Option<&str>,
    body: Option<Value>,
) -> Result<Value, LanClientError> {
    authorized_json_with_client(device, method, path, lease_id, body, http_client()).await
}

pub(crate) fn is_tuning_safety_stop(
    method: &reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> bool {
    method == reqwest::Method::POST
        && path.trim_matches('/') == "thermal-tuning"
        && body
            .and_then(|value| value.get("op"))
            .and_then(Value::as_str)
            == Some("cancel")
}

async fn authorized_json_with_client(
    device: &LanDeviceConfig,
    method: reqwest::Method,
    path: &str,
    lease_id: Option<&str>,
    body: Option<Value>,
    client: Client,
) -> Result<Value, LanClientError> {
    let token = device
        .pairing_token
        .as_deref()
        .filter(|token| is_token(token))
        .ok_or(LanClientError::InvalidPairingResponse)?;
    let base_url = normalize_lan_base_url(&device.base_url)?;
    let _request_slot = tokio::time::timeout(
        LAN_REQUEST_TIMEOUT,
        device_request_budget(&base_url).acquire_owned(),
    )
    .await
    .map_err(|_| LanClientError::RequestQueueBusy)?
    .expect("LAN request budget is never closed");
    let url = format!(
        "{}/api/{LAN_API_VERSION}/{}",
        base_url,
        path.trim_start_matches('/')
    );
    let is_control_write = method != reqwest::Method::GET
        && path.trim_matches('/') != "leases"
        && !is_tuning_safety_stop(&method, path, body.as_ref());
    let revision = if is_control_write {
        let response = client
            .get(format!("{base_url}/api/{LAN_API_VERSION}/status"))
            .timeout(LAN_READ_REQUEST_TIMEOUT)
            .bearer_auth(token)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let payload = response.json::<Value>().await.ok();
            return Err(remote_api_error(status, payload.as_ref()));
        }
        let revision = control_revision(response.headers())?;
        // Header-only reads must still drain the body before issuing the write.
        // Dropping it early leaves the MCU sending a response we no longer read.
        response.bytes().await?;
        Some(revision)
    } else {
        None
    };
    let is_read = method == reqwest::Method::GET;
    let mut request = client.request(method, url).bearer_auth(token);
    if is_read {
        request = request.timeout(LAN_READ_REQUEST_TIMEOUT);
    }
    if let Some(lease_id) = lease_id {
        request = request.header("X-Flux-Purr-Lease", lease_id);
    }
    if let Some(revision) = revision {
        request = request.header("X-Flux-Purr-Revision", revision);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let payload = response.json::<Value>().await.ok();
        return Err(remote_api_error(status, payload.as_ref()));
    }
    Ok(response.json().await?)
}

fn remote_api_error(status: reqwest::StatusCode, payload: Option<&Value>) -> LanClientError {
    let error = payload
        .and_then(|value| value.get("error"))
        .and_then(Value::as_object);
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .unwrap_or("lan_request_failed")
        .to_owned();
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .unwrap_or("LAN device rejected the request.")
        .to_owned();
    let retryable = error
        .and_then(|error| error.get("retryable"))
        .and_then(Value::as_bool)
        .unwrap_or(status.is_server_error());
    LanClientError::RemoteApi {
        status,
        code,
        message,
        retryable,
    }
}

fn control_revision(headers: &reqwest::header::HeaderMap) -> Result<u32, LanClientError> {
    headers
        .get("X-Flux-Purr-Revision")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or(LanClientError::InvalidControlRevision)
}

/// Browse the device's advertised `_http._tcp.local.` service for a bounded
/// period. It is called only by an explicit refresh command or API request.
pub async fn discover_mdns(timeout: Duration) -> Result<Vec<LanDiscovery>, LanClientError> {
    discover_mdns_native(timeout).await
}

/// Probe a user-provided private IPv4 CIDR. Broad ranges are rejected before
/// issuing any requests, and probes run with fixed concurrency and timeouts.
pub async fn discover_cidr(request: LanScanRequest) -> Result<Vec<LanDiscovery>, LanClientError> {
    let network = request
        .cidr
        .parse::<Ipv4Net>()
        .map_err(|_| LanClientError::InvalidCidr)?;
    let host_bits = 32u32.saturating_sub(u32::from(network.prefix_len()));
    let host_count = (1u64 << host_bits).saturating_sub(2);
    if host_count == 0 || host_count > 256 || !network.addr().is_private() {
        return Err(LanClientError::InvalidCidr);
    }
    let addresses: Vec<Ipv4Addr> = network.hosts().collect();

    let client = Client::builder()
        .connect_timeout(Duration::from_millis(450))
        .timeout(Duration::from_millis(900))
        .redirect(Policy::none())
        .build()
        .map_err(LanClientError::Request)?;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    for address in addresses {
        let client = client.clone();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let permit = semaphore.acquire_owned().await.ok()?;
            let _permit = permit;
            probe_discovery_candidate(&client, IpAddr::V4(address)).await
        });
    }
    let mut found = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(Some(discovery)) = result {
            found.insert(discovery.base_url.clone(), discovery);
        }
    }
    Ok(found.into_values().collect())
}

pub fn merge_lan_device(devices: &mut Vec<LanDeviceConfig>, device: LanDeviceConfig) {
    if let Some(existing) = devices
        .iter_mut()
        .find(|existing| existing.base_url == device.base_url)
    {
        let pairing_token = device
            .pairing_token
            .or_else(|| existing.pairing_token.clone());
        *existing = LanDeviceConfig {
            pairing_token,
            ..device
        };
        return;
    }

    if let Some(existing) = devices.iter_mut().find(|existing| existing.id == device.id) {
        // A discovery advertisement has no proof that it owns the existing
        // token. Only a fresh pairing exchange may retarget a paired device.
        if existing.pairing_token.is_some() && device.pairing_token.is_none() {
            return;
        }
        let pairing_token = device
            .pairing_token
            .or_else(|| existing.pairing_token.clone());
        *existing = LanDeviceConfig {
            pairing_token,
            ..device
        };
        return;
    }

    devices.push(device);
}

pub fn device_from_discovery(discovery: LanDiscovery) -> Option<LanDeviceConfig> {
    let base_url = normalize_lan_base_url(&discovery.base_url).ok()?;
    let id = discovery
        .device_id
        .as_deref()
        .filter(|device_id| is_device_id(device_id))
        .map(|device_id| format!("lan-{}", device_id.to_ascii_lowercase()))
        .unwrap_or_else(|| device_id_for_base_url(&base_url));
    let last_ipv4 = Url::parse(&base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .filter(|host| host.parse::<Ipv4Addr>().is_ok());
    Some(LanDeviceConfig {
        id,
        base_url,
        hostname: discovery.hostname,
        last_ipv4,
        pairing_token: None,
    })
}

async fn discover_mdns_native(timeout: Duration) -> Result<Vec<LanDiscovery>, LanClientError> {
    let mut builder = ServiceBrowserBuilder::new();
    builder.service_type("_http._tcp").domain("local");
    let mut browser = builder
        .browse()
        .await
        .map_err(|error| LanClientError::Discovery(error.to_string()))?;
    let deadline =
        Instant::now() + timeout.clamp(Duration::from_millis(250), Duration::from_secs(8));
    let mut found = BTreeMap::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(event)) = tokio::time::timeout(remaining, browser.recv()).await else {
            break;
        };
        let event = event.map_err(|error| LanClientError::Discovery(error.to_string()))?;
        let BrowseEvent::Found(service) = event else {
            continue;
        };
        if let Some(discovery) = discovery_from_native_service(&service) {
            found.insert(discovery.base_url.clone(), discovery);
        }
    }
    Ok(found.into_values().collect())
}

fn discovery_from_native_service(service: &DiscoveredService) -> Option<LanDiscovery> {
    if service.service_type != "_http._tcp"
        || service.domain.trim_end_matches('.') != "local"
        || service.txt("api") != Some(b"v1")
    {
        return None;
    }
    let address = service.addresses.iter().find_map(|address| match address {
        IpAddr::V4(address) if address.is_private() => Some(*address),
        _ => None,
    })?;
    let base_url = if service.port == 80 {
        format!("http://{address}")
    } else {
        format!("http://{address}:{}", service.port)
    };
    Some(LanDiscovery {
        base_url,
        hostname: Some(service.host_name.trim_end_matches('.').to_string()),
        device_id: service
            .txt_records
            .iter()
            .find(|property| property.key == "device")
            .and_then(|property| property.value.as_deref())
            .and_then(|value| core::str::from_utf8(value).ok())
            .filter(|device_id| is_device_id(device_id))
            .map(str::to_owned),
        source: LanDiscoverySource::Mdns,
    })
}

async fn probe_discovery_candidate(client: &Client, address: IpAddr) -> Option<LanDiscovery> {
    let base_url = format!("http://{address}");
    let response = client.get(format!("{base_url}/health")).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let payload = response.json::<Value>().await.ok()?;
    if payload.get("api").and_then(Value::as_str) != Some("v1") {
        return None;
    }
    Some(LanDiscovery {
        base_url,
        hostname: None,
        device_id: None,
        source: LanDiscoverySource::CidrScan,
    })
}

fn is_device_id(value: &str) -> bool {
    value.len() == 12 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        response::Redirect,
        routing::{get, post},
    };
    use mdns_sd_discovery::TxtRecord;
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct ConcurrentReadProbe {
        active: Arc<std::sync::atomic::AtomicUsize>,
        peak: Arc<std::sync::atomic::AtomicUsize>,
    }

    async fn delayed_probe_status(
        axum::extract::State(probe): axum::extract::State<ConcurrentReadProbe>,
    ) -> axum::Json<Value> {
        use std::sync::atomic::Ordering;
        let current = probe.active.fetch_add(1, Ordering::SeqCst) + 1;
        probe.peak.fetch_max(current, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(30)).await;
        probe.active.fetch_sub(1, Ordering::SeqCst);
        axum::Json(json!({"ok": true}))
    }

    async fn read_probe_status(
        device: LanDeviceConfig,
        client: Client,
    ) -> Result<Value, LanClientError> {
        authorized_json_with_client(&device, reqwest::Method::GET, "status", None, None, client)
            .await
    }

    fn native_service(address: IpAddr, api: &[u8]) -> DiscoveredService {
        DiscoveredService {
            name: "flux-purr-a0f262f20d6c".into(),
            service_type: "_http._tcp".into(),
            domain: "local".into(),
            host_name: "flux-purr-a0f262f20d6c.local.".into(),
            port: 80,
            addresses: vec![address],
            txt_records: vec![
                TxtRecord {
                    key: "api".into(),
                    value: Some(api.to_vec()),
                },
                TxtRecord {
                    key: "device".into(),
                    value: Some(b"a0f262f20d6c".to_vec()),
                },
            ],
            interface_index: None,
        }
    }

    #[test]
    fn native_mdns_service_maps_only_private_flux_purr_v1_endpoints() {
        let discovery = discovery_from_native_service(&native_service(
            "192.168.31.189".parse().unwrap(),
            b"v1",
        ))
        .unwrap();
        assert_eq!(discovery.base_url, "http://192.168.31.189");
        assert_eq!(
            discovery.hostname.as_deref(),
            Some("flux-purr-a0f262f20d6c.local")
        );
        assert_eq!(discovery.device_id.as_deref(), Some("a0f262f20d6c"));

        assert!(
            discovery_from_native_service(&native_service("8.8.8.8".parse().unwrap(), b"v1"))
                .is_none()
        );
        assert!(
            discovery_from_native_service(&native_service(
                "192.168.31.189".parse().unwrap(),
                b"v2",
            ))
            .is_none()
        );
    }

    #[test]
    fn parses_only_a_valid_device_control_revision() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(matches!(
            control_revision(&headers),
            Err(LanClientError::InvalidControlRevision)
        ));
        headers.insert("X-Flux-Purr-Revision", "42".parse().unwrap());
        assert_eq!(control_revision(&headers).unwrap(), 42);
    }

    #[test]
    fn remote_api_conflict_preserves_the_device_error_contract() {
        let error = remote_api_error(
            reqwest::StatusCode::CONFLICT,
            Some(&json!({
                "error": {
                    "code": "stale_write",
                    "message": "The control state changed after this client last read it.",
                    "retryable": false
                }
            })),
        );

        match error {
            LanClientError::RemoteApi {
                status,
                code,
                message,
                retryable,
            } => {
                assert_eq!(status, reqwest::StatusCode::CONFLICT);
                assert_eq!(code, "stale_write");
                assert_eq!(
                    message,
                    "The control state changed after this client last read it."
                );
                assert!(!retryable);
            }
            other => panic!("expected preserved remote API conflict, got {other:?}"),
        }
    }

    #[test]
    fn remote_api_unauthorized_preserves_the_device_error_contract() {
        let error = remote_api_error(
            reqwest::StatusCode::UNAUTHORIZED,
            Some(&json!({
                "error": {
                    "code": "unauthorized",
                    "message": "The pairing token is not authorized for this device.",
                    "retryable": false
                }
            })),
        );

        match error {
            LanClientError::RemoteApi {
                status,
                code,
                message,
                retryable,
            } => {
                assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
                assert_eq!(code, "unauthorized");
                assert_eq!(
                    message,
                    "The pairing token is not authorized for this device."
                );
                assert!(!retryable);
            }
            other => panic!("expected preserved remote API unauthorized, got {other:?}"),
        }
    }

    async fn redirect_to_target() -> Redirect {
        Redirect::temporary("/target")
    }

    async fn redirect_target() -> &'static str {
        "redirect followed"
    }

    #[test]
    fn normalizes_manual_device_url_without_accepting_credentials_or_paths() {
        assert_eq!(
            normalize_lan_base_url(" http://192.168.1.18:80/ ").unwrap(),
            "http://192.168.1.18"
        );
        assert!(normalize_lan_base_url("https://192.168.1.18").is_err());
        assert!(normalize_lan_base_url("http://token@192.168.1.18").is_err());
        assert!(normalize_lan_base_url("http://192.168.1.18/api/v1").is_err());
        assert!(normalize_lan_base_url("http://8.8.8.8").is_err());
        assert!(normalize_lan_base_url("http://example.com").is_err());
        assert_eq!(
            normalize_lan_base_url("http://flux-purr-001122334455.local/").unwrap(),
            "http://flux-purr-001122334455.local"
        );
    }

    #[test]
    fn public_summary_and_debug_never_contain_token() {
        let device = LanDeviceConfig {
            id: "lan-123".into(),
            base_url: "http://192.168.1.18".into(),
            hostname: Some("flux-purr-123.local".into()),
            last_ipv4: Some("192.168.1.18".into()),
            pairing_token: Some("a".repeat(64)),
        };
        let summary = serde_json::to_string(&LanDeviceSummary::from(&device)).unwrap();
        assert!(!summary.contains(&"a".repeat(64)));
        assert!(!format!("{device:?}").contains(&"a".repeat(64)));
    }

    #[test]
    fn discovery_refresh_cannot_retarget_a_paired_device() {
        let mut devices = vec![LanDeviceConfig {
            id: "lan-001122334455".into(),
            base_url: "http://192.168.1.18".into(),
            hostname: Some("flux-purr-001122334455.local".into()),
            last_ipv4: Some("192.168.1.18".into()),
            pairing_token: Some("a".repeat(64)),
        }];
        merge_lan_device(
            &mut devices,
            device_from_discovery(LanDiscovery {
                base_url: "http://192.168.1.19".into(),
                hostname: Some("flux-purr-001122334455.local".into()),
                device_id: Some("001122334455".into()),
                source: LanDiscoverySource::Mdns,
            })
            .unwrap(),
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].base_url, "http://192.168.1.18");
        assert_eq!(
            devices[0].pairing_token.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn discovery_records_reject_untrusted_addresses_before_persistence() {
        assert!(
            device_from_discovery(LanDiscovery {
                base_url: "http://8.8.8.8".into(),
                hostname: Some("flux-purr-001122334455.local".into()),
                device_id: Some("001122334455".into()),
                source: LanDiscoverySource::Mdns,
            })
            .is_none()
        );
        assert!(
            device_from_discovery(LanDiscovery {
                base_url: "http://192.168.1.19".into(),
                hostname: Some("flux-purr-001122334455.local".into()),
                device_id: Some("001122334455".into()),
                source: LanDiscoverySource::Mdns,
            })
            .is_some()
        );
    }

    #[test]
    fn fresh_pairing_may_retarget_a_paired_device() {
        let mut devices = vec![LanDeviceConfig {
            id: "lan-001122334455".into(),
            base_url: "http://192.168.1.18".into(),
            hostname: Some("flux-purr-001122334455.local".into()),
            last_ipv4: Some("192.168.1.18".into()),
            pairing_token: Some("a".repeat(64)),
        }];

        merge_lan_device(
            &mut devices,
            LanDeviceConfig {
                id: "lan-001122334455".into(),
                base_url: "http://192.168.1.19".into(),
                hostname: Some("flux-purr-001122334455.local".into()),
                last_ipv4: Some("192.168.1.19".into()),
                pairing_token: Some("b".repeat(64)),
            },
        );

        assert_eq!(devices[0].base_url, "http://192.168.1.19");
        assert_eq!(
            devices[0].pairing_token.as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[tokio::test]
    async fn usb_locator_verification_is_anonymous_and_rejects_another_device() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/health",
            get(|headers: axum::http::HeaderMap| async move {
                assert!(!headers.contains_key("authorization"));
                axum::Json(json!({
                    "ok": true, "api": "v1", "deviceId": "001122334455",
                    "hostname": "flux-purr-001122334455",
                    "pairing": {"mode": "required", "active": false, "attemptsRemaining": 5}
                }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder()
            .no_proxy()
            .resolve("flux-purr-001122334455.local", address)
            .build()
            .unwrap();
        let url = format!("http://flux-purr-001122334455.local:{}", address.port());
        assert!(
            verify_device_locator_with_client(&url, "001122334455", client.clone())
                .await
                .is_ok()
        );
        assert!(matches!(
            verify_device_locator_with_client(&url, "aabbccddeeff", client).await,
            Err(LanClientError::InvalidPublicInfo)
        ));
        server.abort();
    }

    #[tokio::test]
    async fn tuning_stop_reaches_device_without_revision_or_lease_preflight() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/api/v1/thermal-tuning",
            post(
                |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| async move {
                    assert_eq!(
                        headers.get("authorization").unwrap(),
                        format!("Bearer {}", "a".repeat(64)).as_str()
                    );
                    assert!(!headers.contains_key("X-Flux-Purr-Revision"));
                    assert!(!headers.contains_key("X-Flux-Purr-Lease"));
                    assert_eq!(body["op"], "cancel");
                    axum::Json(json!({"armed": false}))
                },
            ),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let device = LanDeviceConfig {
            id: "lan-001122334455".into(),
            base_url: format!("http://flux-purr-001122334455.local:{}", address.port()),
            hostname: None,
            last_ipv4: None,
            pairing_token: Some("a".repeat(64)),
        };
        let client = Client::builder()
            .no_proxy()
            .resolve("flux-purr-001122334455.local", address)
            .build()
            .unwrap();
        let result = authorized_json_with_client(
            &device,
            reqwest::Method::POST,
            "thermal-tuning",
            None,
            Some(json!({"op": "cancel"})),
            client,
        )
        .await;
        server.abort();
        assert_eq!(result.unwrap()["armed"], false);
    }

    async fn serve_split_tuning_body(listener: TcpListener, payload: String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (mut socket, _) = listener.accept().await.unwrap();
        let mut header = [0; 1024];
        let mut received = 0;
        while !header[..received]
            .windows(4)
            .any(|part| part == b"\r\n\r\n")
        {
            let count = socket.read(&mut header[received..]).await.unwrap();
            assert!(count > 0, "fixture request ended before its header");
            received += count;
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        let midpoint = payload.len() / 2;
        socket
            .write_all(&payload.as_bytes()[..midpoint])
            .await
            .unwrap();
        // HIL captured valid 3.3s status and 4.0s tuning responses. The
        // deadline covers the complete body, not merely receipt of headers.
        tokio::time::sleep(Duration::from_secs(4)).await;
        let _ = socket.write_all(&payload.as_bytes()[midpoint..]).await;
    }

    #[tokio::test]
    async fn tuning_read_consumes_a_valid_body_beyond_the_mutation_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let payload = json!({"armed": false, "padding": "x".repeat(4_800)}).to_string();
        let server = tokio::spawn(serve_split_tuning_body(listener, payload));
        let device = LanDeviceConfig {
            id: "lan-001122334455".into(),
            base_url: format!("http://flux-purr-001122334455.local:{}", address.port()),
            hostname: None,
            last_ipv4: None,
            pairing_token: Some("a".repeat(64)),
        };
        let client = Client::builder()
            .no_proxy()
            .timeout(LAN_REQUEST_TIMEOUT)
            .resolve("flux-purr-001122334455.local", address)
            .build()
            .unwrap();
        let result = authorized_json_with_client(
            &device,
            reqwest::Method::GET,
            "thermal-tuning",
            None,
            None,
            client,
        )
        .await;
        server.abort();
        let value = result.expect("a valid device response must finish within the read budget");
        assert_eq!(value["armed"], false);
        assert_eq!(value["padding"].as_str().unwrap().len(), 4_800);
    }

    #[tokio::test]
    async fn concurrent_reads_respect_the_device_http_request_budget() {
        use std::sync::atomic::Ordering;
        let probe = ConcurrentReadProbe::default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/v1/status", get(delayed_probe_status))
            .with_state(probe.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let device = LanDeviceConfig {
            id: "lan-001122334455".into(),
            base_url: format!("http://flux-purr-001122334455.local:{}", address.port()),
            hostname: None,
            last_ipv4: None,
            pairing_token: Some("a".repeat(64)),
        };
        let client = Client::builder()
            .no_proxy()
            .resolve("flux-purr-001122334455.local", address)
            .build()
            .unwrap();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            tasks.spawn(read_probe_status(device.clone(), client.clone()));
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap().unwrap();
        }
        server.abort();
        assert!(
            probe.peak.load(Ordering::SeqCst) <= 2,
            "polling exhausted the device socket budget"
        );
        assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn lan_http_client_does_not_follow_redirects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/redirect", get(redirect_to_target))
            .route("/target", get(redirect_target));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let response = http_client()
            .get(format!("http://{address}/redirect"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        server.abort();
    }

    #[test]
    fn pairing_replaces_an_unpaired_scan_record_at_the_same_address() {
        let mut devices = vec![LanDeviceConfig {
            id: device_id_for_base_url("http://192.168.1.18"),
            base_url: "http://192.168.1.18".into(),
            hostname: None,
            last_ipv4: Some("192.168.1.18".into()),
            pairing_token: None,
        }];

        merge_lan_device(
            &mut devices,
            LanDeviceConfig {
                id: "lan-001122334455".into(),
                base_url: "http://192.168.1.18".into(),
                hostname: Some("flux-purr-001122334455.local".into()),
                last_ipv4: Some("192.168.1.18".into()),
                pairing_token: Some("a".repeat(64)),
            },
        );

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "lan-001122334455");
        assert!(devices[0].pairing_token.as_deref().is_some_and(is_token));
    }

    #[test]
    fn pairing_claim_body_uses_the_connected_device_policy() {
        assert_eq!(
            pairing_claim_body(LanPairingMode::Required, Some("4827".to_string())).unwrap(),
            json!({ "code": "4827" })
        );
        assert!(matches!(
            pairing_claim_body(LanPairingMode::Required, None),
            Err(LanClientError::PairingCodeRequired)
        ));
        assert_eq!(
            pairing_claim_body(LanPairingMode::Optional, None).unwrap(),
            json!({})
        );
        assert!(matches!(
            pairing_claim_body(LanPairingMode::Unavailable, None),
            Err(LanClientError::PairingUnavailable)
        ));
    }

    #[tokio::test]
    async fn cidr_scan_rejects_public_and_broad_ranges_before_probing() {
        assert!(matches!(
            discover_cidr(LanScanRequest {
                cidr: "8.8.8.0/24".into()
            })
            .await,
            Err(LanClientError::InvalidCidr)
        ));
        assert!(matches!(
            discover_cidr(LanScanRequest {
                cidr: "192.168.0.0/16".into()
            })
            .await,
            Err(LanClientError::InvalidCidr)
        ));
        assert!(matches!(
            discover_cidr(LanScanRequest {
                cidr: "10.0.0.0/8".into()
            })
            .await,
            Err(LanClientError::InvalidCidr)
        ));
    }
}
