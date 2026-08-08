//! Direct LAN client support. Credentials never leave this module in a
//! serializable response: callers receive `LanDeviceSummary` only.

use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
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
    #[error("LAN request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("LAN discovery failed: {0}")]
    Discovery(String),
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
    let summary = http_client()
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
    let token = device
        .pairing_token
        .as_deref()
        .filter(|token| is_token(token))
        .ok_or(LanClientError::InvalidPairingResponse)?;
    let base_url = normalize_lan_base_url(&device.base_url)?;
    let url = format!(
        "{}/api/{LAN_API_VERSION}/{}",
        base_url,
        path.trim_start_matches('/')
    );
    let client = http_client();
    let is_control_write = method != reqwest::Method::GET && path.trim_matches('/') != "leases";
    let revision = if is_control_write {
        let response = client
            .get(format!("{base_url}/api/{LAN_API_VERSION}/status"))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;
        Some(control_revision(response.headers())?)
    } else {
        None
    };
    let mut request = client.request(method, url).bearer_auth(token);
    if let Some(lease_id) = lease_id {
        request = request.header("X-Flux-Purr-Lease", lease_id);
    }
    if let Some(revision) = revision {
        request = request.header("X-Flux-Purr-Revision", revision);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
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
    use axum::{Router, response::Redirect, routing::get};
    use mdns_sd_discovery::TxtRecord;
    use tokio::net::TcpListener;

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
