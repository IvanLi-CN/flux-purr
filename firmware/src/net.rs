//! ESP32-S3 WiFi STA transport for the LAN control plane.
//!
//! The TCP task owns only HTTP parsing and the `NetHttpState` security gate.
//! It forwards authorized commands through `CONTROL_MAILBOX`; the front-panel
//! main loop is the only consumer that executes PD, heater, calibration, or
//! EEPROM work.

use core::{
    fmt::Write as _,
    sync::atomic::{AtomicU32, Ordering},
};

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_net::{
    Config as NetConfig, IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr, Stack, StackResources,
    StaticConfigV4,
    tcp::TcpSocket,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex, signal::Signal,
};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Write as _;
use esp_hal::{peripherals::WIFI, rng::Rng};
use esp_wifi::{
    EspWifiController,
    wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent},
};
use heapless::{String, Vec};
use static_cell::StaticCell;

use crate::{
    control_plane::{Identity, NetworkState, NetworkSummary},
    memory::{MEMORY_WIFI_PASSWORD_MAX_LEN, MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig},
    net_http::{
        ControlMailboxCommand, DeviceNames, HTTP_SERVICE_PORT, HTTP_SERVICE_TXT, HTTP_SERVICE_TYPE,
        HttpGate, HttpMethod, HttpRequest, HttpResponse, LAN_HTTP_BODY_MAX_LEN, NetHttpState,
        device_names_from_mac, identity_from_device_names,
    },
};

const HTTP_BUFFER_LEN: usize = LAN_HTTP_BODY_MAX_LEN + 1_024;
const MDNS_MULTICAST_V4: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
const MDNS_TTL_SECS: u32 = 120;

type ControlStateMutex = Mutex<CriticalSectionRawMutex, Option<NetHttpState>>;
type WifiConfigMutex = Mutex<CriticalSectionRawMutex, WifiRuntimeConfig>;
type RuntimeStatusMutex = Mutex<CriticalSectionRawMutex, LanRuntimeState>;

static CONTROL_STATE: ControlStateMutex = Mutex::new(None);
// The listener services one request at a time, so one in-flight command and
// response is enough. Request IDs prevent a late answer for a timed-out
// command from being attributed to the next request.
static CONTROL_MAILBOX: Channel<CriticalSectionRawMutex, ControlMailboxCommand, 1> = Channel::new();
static CONTROL_RESPONSE: Signal<CriticalSectionRawMutex, ControlMailboxResponse> = Signal::new();
static CONTROL_REQUEST_ID: AtomicU32 = AtomicU32::new(1);
static WIFI_CONFIG: WifiConfigMutex = Mutex::new(WifiRuntimeConfig::empty());
static WIFI_APPLY_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static LAN_RUNTIME: RuntimeStatusMutex = Mutex::new(LanRuntimeState::empty());
static NET_RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();

struct ControlMailboxResponse {
    request_id: u32,
    status: u16,
    body: String<LAN_HTTP_BODY_MAX_LEN>,
}

/// A startup failure must never prevent the USB control plane from reaching
/// its recovery loop. The public message is intentionally operational rather
/// than exposing WiFi-driver internals to LAN clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanStartupError {
    WifiInitialization,
    NetworkTaskCapacity,
    WifiTaskCapacity,
    HttpTaskCapacity,
    MdnsTaskCapacity,
}

impl LanStartupError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::WifiInitialization => "WiFi driver initialization failed.",
            Self::NetworkTaskCapacity
            | Self::WifiTaskCapacity
            | Self::HttpTaskCapacity
            | Self::MdnsTaskCapacity => "LAN control task capacity is unavailable.",
        }
    }
}

#[derive(Clone)]
struct WifiRuntimeConfig {
    ssid: String<MEMORY_WIFI_SSID_MAX_LEN>,
    password: String<MEMORY_WIFI_PASSWORD_MAX_LEN>,
    auto_reconnect: bool,
    static_ipv4: Option<crate::memory::WifiStaticIpv4Config>,
    hostname: Option<String<32>>,
}

struct LanRuntimeState {
    device_names: Option<DeviceNames>,
    network: Option<NetworkSummary>,
}

impl LanRuntimeState {
    const fn empty() -> Self {
        Self {
            device_names: None,
            network: None,
        }
    }
}

impl WifiRuntimeConfig {
    const fn empty() -> Self {
        Self {
            ssid: String::new(),
            password: String::new(),
            auto_reconnect: false,
            static_ipv4: None,
            hostname: None,
        }
    }

    fn from_memory(memory: &MemoryConfig) -> Self {
        Self {
            ssid: memory.wifi_ssid.clone(),
            password: memory.wifi_password.clone(),
            auto_reconnect: memory.wifi_auto_reconnect,
            static_ipv4: memory.wifi_static_ipv4,
            hostname: None,
        }
    }

    fn is_configured(&self) -> bool {
        !self.ssid.is_empty()
    }
}

/// Initialize the pairing/token state before any task can serve LAN traffic.
pub async fn initialize_control_state(token: Option<[u8; crate::lan::LAN_TOKEN_BYTES]>) {
    *CONTROL_STATE.lock().await = Some(NetHttpState::new(token));
}

pub async fn enter_pairing() -> [u8; 4] {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .pairing_code_from_random(random_u32())
}

pub async fn leave_pairing() {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .leave_pairing();
}

/// Reads the transient pairing code without extending or recreating the
/// front-panel-scoped pairing window.
pub async fn pairing_code() -> Option<[u8; 4]> {
    CONTROL_STATE
        .lock()
        .await
        .as_ref()
        .and_then(NetHttpState::pairing_code)
}

pub async fn clear_token_from_usb() {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .clear_token_from_usb();
}

pub async fn take_persisted_token_change() -> Option<Option<[u8; crate::lan::LAN_TOKEN_BYTES]>> {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .take_persisted_token_change()
}

pub async fn apply_wifi_config(memory: &MemoryConfig) {
    let mut config = WifiRuntimeConfig::from_memory(memory);
    let mut current = WIFI_CONFIG.lock().await;
    config.hostname = current.hostname.clone();
    *current = config;
    let summary = network_summary_for_config(&current, NetworkState::Saving);
    drop(current);
    set_network_summary(summary).await;
    WIFI_APPLY_SIGNAL.signal(());
}

/// Publish a failed LAN startup to USB status while keeping the main control
/// loop available for recovery and later firmware servicing.
pub async fn report_startup_failure(error: LanStartupError) {
    let config = WIFI_CONFIG.lock().await.clone();
    set_network_summary(network_failure(
        &config,
        NetworkState::Error,
        error.message(),
    ))
    .await;
}

/// Returns the identity advertised by the initialized WiFi station. This is
/// separate from the USB development placeholder because LAN clients persist
/// it as their stable device key.
pub async fn lan_identity() -> Identity {
    let names = LAN_RUNTIME.lock().await.device_names.clone();
    names
        .as_ref()
        .map(identity_from_device_names)
        .unwrap_or_else(Identity::firmware_default)
}

/// Returns the actual connection state published by the WiFi task. The TCP
/// listener never infers connectivity from EEPROM because a saved SSID is not
/// evidence that association and IPv4 configuration succeeded.
pub async fn lan_network_summary() -> NetworkSummary {
    LAN_RUNTIME.lock().await.network.clone().unwrap_or_default()
}

pub fn try_receive_command() -> Option<ControlMailboxCommand> {
    CONTROL_MAILBOX.try_receive().ok()
}

pub fn respond_to_command(request_id: u32, status: u16, body: String<LAN_HTTP_BODY_MAX_LEN>) {
    CONTROL_RESPONSE.signal(ControlMailboxResponse {
        request_id,
        status,
        body,
    });
}

fn next_control_request_id() -> u32 {
    let request_id = CONTROL_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    if request_id == 0 {
        CONTROL_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    } else {
        request_id
    }
}

async fn await_control_response(request_id: u32) -> Option<ControlMailboxResponse> {
    loop {
        let response = with_timeout(Duration::from_secs(3), CONTROL_RESPONSE.wait())
            .await
            .ok()?;
        if response.request_id == request_id {
            return Some(response);
        }
    }
}

/// Starts the DHCP/static-IP capable station, TCP server, and mDNS/DNS-SD
/// responder. WiFi setup remains USB-only because this function receives its
/// credentials from EEPROM-loaded `MemoryConfig`.
pub async fn spawn(
    spawner: &Spawner,
    init: &'static EspWifiController<'static>,
    wifi_peripheral: WIFI<'static>,
    memory: &MemoryConfig,
) -> Result<(), LanStartupError> {
    let (controller, interfaces) = esp_wifi::wifi::new(init, wifi_peripheral)
        .map_err(|_| LanStartupError::WifiInitialization)?;
    let station = interfaces.sta;
    let names = device_names_from_mac(station.mac_address());
    WIFI_CONFIG.lock().await.hostname = Some(names.hostname.clone());
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .set_device_names(names.clone());
    LAN_RUNTIME.lock().await.device_names = Some(names.clone());
    let resources = NET_RESOURCES.init(StackResources::<8>::new());
    let seed = random_u64();
    let (stack, runner) = embassy_net::new(
        station,
        net_config(&WifiRuntimeConfig::from_memory(memory)),
        resources,
        seed,
    );

    spawner
        .spawn(network_task(runner))
        .map_err(|_| LanStartupError::NetworkTaskCapacity)?;
    spawner
        .spawn(wifi_task(controller, stack))
        .map_err(|_| LanStartupError::WifiTaskCapacity)?;
    spawner
        .spawn(http_task(stack))
        .map_err(|_| LanStartupError::HttpTaskCapacity)?;
    spawner
        .spawn(mdns_task(stack, names))
        .map_err(|_| LanStartupError::MdnsTaskCapacity)?;
    Ok(())
}

#[embassy_executor::task]
async fn network_task(mut runner: embassy_net::Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>, stack: Stack<'static>) {
    loop {
        let config = WIFI_CONFIG.lock().await.clone();
        if !config.is_configured() {
            let _ = controller.stop_async().await;
            set_network_summary(NetworkSummary::default()).await;
            WIFI_APPLY_SIGNAL.wait().await;
            continue;
        }

        set_network_summary(network_summary_for_config(
            &config,
            NetworkState::Connecting,
        ))
        .await;

        let client = Configuration::Client(ClientConfiguration {
            ssid: alloc::string::String::from(config.ssid.as_str()),
            password: alloc::string::String::from(config.password.as_str()),
            ..Default::default()
        });
        stack.set_config_v4(net_config(&config).ipv4);
        if controller.set_configuration(&client).is_err()
            || (!matches!(controller.is_started(), Ok(true))
                && controller.start_async().await.is_err())
        {
            set_network_summary(network_failure(
                &config,
                NetworkState::Error,
                "WiFi configuration could not be applied.",
            ))
            .await;
            Timer::after(Duration::from_secs(2)).await;
            continue;
        }
        if controller.connect_async().await.is_err() {
            set_network_summary(network_failure(
                &config,
                NetworkState::Error,
                "WiFi association failed.",
            ))
            .await;
            Timer::after(Duration::from_secs(2)).await;
            continue;
        }
        if with_timeout(Duration::from_secs(15), stack.wait_config_up())
            .await
            .is_err()
        {
            let _ = controller.disconnect_async().await;
            set_network_summary(network_failure(
                &config,
                NetworkState::Timeout,
                "Timed out waiting for IPv4 configuration.",
            ))
            .await;
            Timer::after(Duration::from_secs(2)).await;
            continue;
        }

        set_network_summary(network_connected(&config, &stack, &controller)).await;

        match select(
            controller.wait_for_event(WifiEvent::StaDisconnected),
            WIFI_APPLY_SIGNAL.wait(),
        )
        .await
        {
            Either::First(()) if config.auto_reconnect => {
                set_network_summary(network_summary_for_config(
                    &config,
                    NetworkState::Connecting,
                ))
                .await;
                Timer::after(Duration::from_secs(2)).await
            }
            Either::First(()) => {
                set_network_summary(network_failure(
                    &config,
                    NetworkState::Error,
                    "WiFi station disconnected.",
                ))
                .await;
                WIFI_APPLY_SIGNAL.wait().await
            }
            Either::Second(()) => {
                let _ = controller.disconnect_async().await;
                set_network_summary(network_summary_for_config(&config, NetworkState::Saving))
                    .await;
            }
        }
    }
}

async fn set_network_summary(summary: NetworkSummary) {
    LAN_RUNTIME.lock().await.network = Some(summary);
}

fn network_summary_for_config(config: &WifiRuntimeConfig, state: NetworkState) -> NetworkSummary {
    if !config.is_configured() {
        return NetworkSummary::default();
    }
    NetworkSummary {
        state,
        ssid: Some(config.ssid.clone()),
        ..NetworkSummary::default()
    }
}

fn network_failure(
    config: &WifiRuntimeConfig,
    state: NetworkState,
    message: &str,
) -> NetworkSummary {
    let mut summary = network_summary_for_config(config, state);
    let mut error = String::new();
    let _ = error.push_str(message);
    summary.last_error = Some(error);
    summary
}

fn network_connected(
    config: &WifiRuntimeConfig,
    stack: &Stack<'static>,
    controller: &WifiController<'static>,
) -> NetworkSummary {
    let mut summary = network_summary_for_config(config, NetworkState::Connected);
    if let Some(ipv4) = stack.config_v4() {
        summary.ip = Some(ipv4_string(ipv4.address.address()));
        summary.gateway = ipv4.gateway.map(ipv4_string);
        for dns in ipv4.dns_servers {
            let _ = summary.dns.push(ipv4_string(dns));
        }
    }
    summary.wifi_rssi = controller
        .rssi()
        .ok()
        .and_then(|rssi| i16::try_from(rssi).ok());
    summary
}

fn ipv4_string(address: Ipv4Address) -> String<48> {
    let mut value = String::new();
    let _ = write!(value, "{address}");
    value
}

fn net_config(config: &WifiRuntimeConfig) -> NetConfig {
    let Some(static_ipv4) = config.static_ipv4 else {
        let mut dhcp = embassy_net::DhcpConfig::default();
        if let Some(hostname) = &config.hostname {
            let mut value = heapless::String::new();
            let _ = value.push_str(hostname.as_str());
            dhcp.hostname = Some(value);
        }
        return NetConfig::dhcpv4(dhcp);
    };
    let mut dns_servers = Vec::new();
    let _ = dns_servers.push(Ipv4Address::from(static_ipv4.dns));
    NetConfig::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(
            Ipv4Address::from(static_ipv4.address),
            static_ipv4.prefix_len,
        ),
        gateway: Some(Ipv4Address::from(static_ipv4.gateway)),
        dns_servers,
    })
}

#[embassy_executor::task]
async fn http_task(stack: Stack<'static>) {
    let mut rx = [0u8; HTTP_BUFFER_LEN];
    let mut tx = [0u8; HTTP_BUFFER_LEN];
    loop {
        stack.wait_config_up().await;
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(5)));
        if socket.accept(HTTP_SERVICE_PORT).await.is_ok() {
            let _ = handle_http_connection(&mut socket).await;
            socket.close();
            let _ = socket.flush().await;
        } else {
            Timer::after(Duration::from_millis(200)).await;
        }
    }
}

async fn handle_http_connection(socket: &mut TcpSocket<'_>) -> Result<(), embassy_net::tcp::Error> {
    let mut request_bytes = [0u8; HTTP_BUFFER_LEN];
    let mut total = 0usize;
    loop {
        let received = socket.read(&mut request_bytes[total..]).await?;
        if received == 0 || total.saturating_add(received) >= request_bytes.len() {
            break;
        }
        total += received;
        if request_bytes[..total]
            .windows(4)
            .any(|value| value == b"\r\n\r\n")
        {
            break;
        }
    }
    let header_end = request_bytes[..total]
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(total);
    let mut path = String::<128>::new();
    let mut origin = None::<String<128>>;
    let mut authorization = None::<String<96>>;
    let mut lease_id = None::<String<32>>;
    let mut private_network = false;
    let mut content_length = 0usize;
    let method = {
        let header = core::str::from_utf8(&request_bytes[..header_end]).unwrap_or("");
        let mut lines = header.lines();
        let mut request_line = lines.next().unwrap_or("").split_whitespace();
        let method = match request_line.next().unwrap_or("") {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "OPTIONS" => HttpMethod::Options,
            _ => return write_http_response(socket, HttpResponse::new(405, r#"{"error":{"code":"method_not_allowed","message":"Unsupported HTTP method."}}"#)).await,
        };
        let _ = path.push_str(
            request_line
                .next()
                .unwrap_or("")
                .split('?')
                .next()
                .unwrap_or(""),
        );
        for line in lines {
            let Some((key, value)) = line.trim_end_matches('\r').split_once(':') else {
                continue;
            };
            let value = value.trim();
            if key.eq_ignore_ascii_case("Origin") {
                let mut copied = String::new();
                let _ = copied.push_str(value);
                origin = Some(copied);
            }
            if key.eq_ignore_ascii_case("Authorization") {
                let mut copied = String::new();
                let _ = copied.push_str(value);
                authorization = Some(copied);
            }
            if key.eq_ignore_ascii_case("X-Flux-Purr-Lease") {
                let mut copied = String::new();
                let _ = copied.push_str(value);
                lease_id = Some(copied);
            }
            if key.eq_ignore_ascii_case("Access-Control-Request-Private-Network") {
                private_network = value.eq_ignore_ascii_case("true");
            }
            if key.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse().unwrap_or(0);
            }
        }
        method
    };
    if content_length > LAN_HTTP_BODY_MAX_LEN {
        return write_http_response(socket, HttpResponse::new(400, r#"{"error":{"code":"body_too_large","message":"Request body exceeds the LAN API limit."}}"#)).await;
    }
    let mut body_len = total.saturating_sub(header_end);
    while body_len < content_length && total < request_bytes.len() {
        let received = socket.read(&mut request_bytes[total..]).await?;
        if received == 0 {
            break;
        }
        total += received;
        body_len = total.saturating_sub(header_end);
    }
    let body_end = header_end.saturating_add(content_length).min(total);
    let body = core::str::from_utf8(&request_bytes[header_end..body_end]).unwrap_or("");
    let request = HttpRequest {
        method,
        path: path.as_str(),
        origin: origin.as_ref().map(String::as_str),
        authorization: authorization.as_ref().map(String::as_str),
        lease_id: lease_id.as_ref().map(String::as_str),
        request_private_network: private_network,
        body,
        entropy: random_entropy(),
    };
    let gate = CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .gate(Instant::now().as_millis(), request);
    let response = match gate {
        HttpGate::Respond(response) => response,
        HttpGate::Dispatch {
            mut command,
            allow_origin,
        } => {
            command.request_id = next_control_request_id();
            let request_id = command.request_id;
            if command.endpoint == crate::lan::LanEndpoint::Events
                && command.method == HttpMethod::Get
            {
                if CONTROL_MAILBOX.try_send(command).is_err() {
                    let mut response = HttpResponse::new(
                        503,
                        r#"{"error":{"code":"control_busy","message":"Control mailbox is busy."}}"#,
                    );
                    response.allow_origin = allow_origin;
                    return write_http_response(socket, response).await;
                }
                return match await_control_response(request_id).await {
                    Some(response) if response.status == 200 => {
                        write_sse_status_event(socket, response.body.as_str(), allow_origin).await
                    }
                    Some(response) => {
                        let mut response = HttpResponse::json(response.status, response.body);
                        response.allow_origin = allow_origin;
                        write_http_response(socket, response).await
                    }
                    None => {
                        let mut response = HttpResponse::new(
                            504,
                            r#"{"error":{"code":"control_timeout","message":"Control loop did not respond."}}"#,
                        );
                        response.allow_origin = allow_origin;
                        write_http_response(socket, response).await
                    }
                };
            }
            if CONTROL_MAILBOX.try_send(command).is_err() {
                let mut response = HttpResponse::new(
                    503,
                    r#"{"error":{"code":"control_busy","message":"Control mailbox is busy."}}"#,
                );
                response.allow_origin = allow_origin;
                response
            } else {
                match await_control_response(request_id).await {
                    Some(control) => {
                        let mut response = HttpResponse::json(control.status, control.body);
                        response.allow_origin = allow_origin;
                        response
                    }
                    None => {
                        let mut response = HttpResponse::new(
                            504,
                            r#"{"error":{"code":"control_timeout","message":"Control loop did not respond."}}"#,
                        );
                        response.allow_origin = allow_origin;
                        response
                    }
                }
            }
        }
    };
    write_http_response(socket, response).await
}

async fn write_http_response(
    socket: &mut TcpSocket<'_>,
    response: HttpResponse,
) -> Result<(), embassy_net::tcp::Error> {
    write_http_response_with_type(
        socket,
        response.status,
        response.body.as_str(),
        response.allow_origin,
        response.allow_private_network,
        "application/json",
    )
    .await
}

async fn write_sse_status_event(
    socket: &mut TcpSocket<'_>,
    status: &str,
    allow_origin: Option<String<128>>,
) -> Result<(), embassy_net::tcp::Error> {
    let mut event = String::<HTTP_BUFFER_LEN>::new();
    let _ = write!(event, "event: status\ndata: {status}\n\n");
    write_http_response_with_type(
        socket,
        200,
        event.as_str(),
        allow_origin,
        false,
        "text/event-stream",
    )
    .await
}

async fn write_http_response_with_type(
    socket: &mut TcpSocket<'_>,
    response_status: u16,
    body: &str,
    allow_origin: Option<String<128>>,
    allow_private_network: bool,
    content_type: &str,
) -> Result<(), embassy_net::tcp::Error> {
    let status = match response_status {
        200 => "200 OK",
        204 => "204 No Content",
        400 => "400 Bad Request",
        401 => "401 Unauthorized",
        403 => "403 Forbidden",
        404 => "404 Not Found",
        405 => "405 Method Not Allowed",
        409 => "409 Conflict",
        429 => "429 Too Many Requests",
        503 => "503 Service Unavailable",
        504 => "504 Gateway Timeout",
        _ => "500 Internal Server Error",
    };
    let mut header = String::<384>::new();
    let _ = write!(
        header,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-Flux-Purr-Lease\r\nAccess-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n",
        body.len()
    );
    if content_type == "text/event-stream" {
        let _ = header.push_str("Cache-Control: no-cache\r\n");
    }
    if let Some(origin) = allow_origin {
        let _ = write!(
            header,
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\n"
        );
    }
    if allow_private_network {
        let _ = header.push_str("Access-Control-Allow-Private-Network: true\r\n");
    }
    let _ = header.push_str("\r\n");
    socket.write_all(header.as_bytes()).await?;
    socket.write_all(body.as_bytes()).await?;
    socket.flush().await
}

#[embassy_executor::task]
async fn mdns_task(stack: Stack<'static>, names: DeviceNames) {
    loop {
        stack.wait_config_up().await;
        let Some(config) = stack.config_v4() else {
            continue;
        };
        let ip = config.address.address();
        let _ = stack.join_multicast_group(IpAddress::Ipv4(MDNS_MULTICAST_V4));
        let mut rx_meta = [PacketMetadata::EMPTY; 2];
        let mut tx_meta = [PacketMetadata::EMPTY; 2];
        let mut rx_storage = [0u8; 512];
        let mut tx_storage = [0u8; 512];
        let mut query = [0u8; 512];
        let mut announce = [0u8; 512];
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_storage,
            &mut tx_meta,
            &mut tx_storage,
        );
        socket.set_hop_limit(Some(255));
        if socket.bind((IpAddress::Ipv4(ip), MDNS_PORT)).is_err() {
            Timer::after(Duration::from_secs(1)).await;
            continue;
        }
        loop {
            let _ = send_mdns_announcement(&mut socket, &mut announce, &names, ip).await;
            match select(
                socket.recv_from(&mut query),
                Timer::after(Duration::from_secs(30)),
            )
            .await
            {
                Either::First(Ok((_len, meta))) => {
                    // A full DNS-SD announcement is valid additional data for a
                    // browse query and keeps the responder bounded/no-alloc.
                    let _ =
                        send_mdns_to(&mut socket, &mut announce, &names, ip, meta.endpoint).await;
                }
                Either::First(Err(_)) | Either::Second(()) => {}
            }
            if !stack.is_config_up() {
                break;
            }
        }
    }
}

async fn send_mdns_announcement(
    socket: &mut UdpSocket<'_>,
    buffer: &mut [u8],
    names: &DeviceNames,
    ip: Ipv4Address,
) -> Result<(), ()> {
    send_mdns_to(
        socket,
        buffer,
        names,
        ip,
        IpEndpoint::new(IpAddress::Ipv4(MDNS_MULTICAST_V4), MDNS_PORT),
    )
    .await
}

async fn send_mdns_to(
    socket: &mut UdpSocket<'_>,
    buffer: &mut [u8],
    names: &DeviceNames,
    ip: Ipv4Address,
    endpoint: IpEndpoint,
) -> Result<(), ()> {
    let length = build_mdns_announcement(buffer, names, ip).ok_or(())?;
    socket
        .send_to(&buffer[..length], endpoint)
        .await
        .map_err(|_| ())
}

fn build_mdns_announcement(
    buffer: &mut [u8],
    names: &DeviceNames,
    ip: Ipv4Address,
) -> Option<usize> {
    let hostname = names.hostname.as_str();
    let mut host_fqdn = String::<48>::new();
    host_fqdn.push_str(hostname).ok()?;
    host_fqdn.push_str(".local").ok()?;
    let mut instance = String::<80>::new();
    instance.push_str(hostname).ok()?;
    instance.push_str(".").ok()?;
    instance.push_str(HTTP_SERVICE_TYPE).ok()?;
    buffer[..12].copy_from_slice(&[0, 0, 0x84, 0, 0, 0, 0, 4, 0, 0, 0, 0]);
    let mut at = 12;
    at = dns_record_ptr(buffer, at, HTTP_SERVICE_TYPE, instance.as_str())?;
    at = dns_record_srv(
        buffer,
        at,
        instance.as_str(),
        HTTP_SERVICE_PORT,
        host_fqdn.as_str(),
    )?;
    at = dns_record_txt(buffer, at, instance.as_str(), names)?;
    dns_record_a(buffer, at, host_fqdn.as_str(), ip)
}

fn dns_name(buffer: &mut [u8], mut at: usize, name: &str) -> Option<usize> {
    for label in name.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > 63 || at + bytes.len() + 1 > buffer.len() {
            return None;
        }
        buffer[at] = bytes.len() as u8;
        at += 1;
        buffer[at..at + bytes.len()].copy_from_slice(bytes);
        at += bytes.len();
    }
    if at >= buffer.len() {
        return None;
    }
    buffer[at] = 0;
    Some(at + 1)
}

fn dns_header(buffer: &mut [u8], at: usize, ty: u16, data_len: u16) -> Option<usize> {
    if at + 10 > buffer.len() {
        return None;
    }
    buffer[at..at + 2].copy_from_slice(&ty.to_be_bytes());
    buffer[at + 2..at + 4].copy_from_slice(&0x8001u16.to_be_bytes());
    buffer[at + 4..at + 8].copy_from_slice(&MDNS_TTL_SECS.to_be_bytes());
    buffer[at + 8..at + 10].copy_from_slice(&data_len.to_be_bytes());
    Some(at + 10)
}

fn dns_record_ptr(buffer: &mut [u8], at: usize, name: &str, target: &str) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 12, 0)?;
    let end = dns_name(buffer, data_at, target)?;
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_record_srv(
    buffer: &mut [u8],
    at: usize,
    name: &str,
    port: u16,
    target: &str,
) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 33, 0)?;
    if data_at + 6 > buffer.len() {
        return None;
    }
    buffer[data_at..data_at + 2].copy_from_slice(&0u16.to_be_bytes());
    buffer[data_at + 2..data_at + 4].copy_from_slice(&0u16.to_be_bytes());
    buffer[data_at + 4..data_at + 6].copy_from_slice(&port.to_be_bytes());
    let end = dns_name(buffer, data_at + 6, target)?;
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_record_txt(buffer: &mut [u8], at: usize, name: &str, names: &DeviceNames) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 16, 0)?;
    let mut end = data_at;
    for entry in HTTP_SERVICE_TXT {
        let bytes = entry.as_bytes();
        if bytes.len() > 255 || end + bytes.len() + 1 > buffer.len() {
            return None;
        }
        buffer[end] = bytes.len() as u8;
        end += 1;
        buffer[end..end + bytes.len()].copy_from_slice(bytes);
        end += bytes.len();
    }
    let mut device = String::<20>::new();
    let _ = write!(
        device,
        "device={}",
        core::str::from_utf8(&names.device_id).ok()?
    );
    let bytes = device.as_bytes();
    if bytes.len() > 255 || end + bytes.len() + 1 > buffer.len() {
        return None;
    }
    buffer[end] = bytes.len() as u8;
    end += 1;
    buffer[end..end + bytes.len()].copy_from_slice(bytes);
    end += bytes.len();
    buffer[at + 8..at + 10].copy_from_slice(&((end - data_at) as u16).to_be_bytes());
    Some(end)
}

fn dns_record_a(buffer: &mut [u8], at: usize, name: &str, ip: Ipv4Address) -> Option<usize> {
    let at = dns_name(buffer, at, name)?;
    let data_at = dns_header(buffer, at, 1, 4)?;
    if data_at + 4 > buffer.len() {
        return None;
    }
    buffer[data_at..data_at + 4].copy_from_slice(&ip.octets());
    Some(data_at + 4)
}

fn random_u64() -> u64 {
    (u64::from(random_u32()) << 32) | u64::from(random_u32())
}

fn random_entropy() -> [u8; crate::lan::LAN_TOKEN_BYTES] {
    let mut out = [0u8; crate::lan::LAN_TOKEN_BYTES];
    for chunk in out.chunks_exact_mut(4) {
        chunk.copy_from_slice(&random_u32().to_le_bytes());
    }
    out
}

fn random_u32() -> u32 {
    // The ESP RNG peripheral is shareable and WiFi keeps the RF entropy source
    // enabled. `Rng` is a zero-sized register facade, so stealing it here does
    // not duplicate owned DMA/state and avoids weakening pairing to a counter.
    let peripheral = unsafe { esp_hal::peripherals::RNG::steal() };
    Rng::new(peripheral).random()
}
