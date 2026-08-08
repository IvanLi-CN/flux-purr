//! ESP32-S3 WiFi STA transport for the LAN control plane.
//!
//! The TCP task owns only HTTP parsing and the `NetHttpState` security gate.
//! It forwards authorized commands through `CONTROL_MAILBOX`; the front-panel
//! main loop is the only consumer that executes PD, heater, calibration, or
//! EEPROM work.

use core::{
    cell::RefCell,
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
    blocking_mutex::{Mutex as BlockingMutex, raw::CriticalSectionRawMutex},
    channel::Channel,
    mutex::Mutex,
    signal::Signal,
};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Write;
use esp_hal::{peripherals::WIFI, rng::Rng};
use esp_radio::{
    Controller as RadioController, init as radio_init,
    wifi::{self, ClientConfig, ModeConfig, WifiController, WifiDevice, WifiEvent},
};
use heapless::{String, Vec};
use serde::Serialize;
use static_cell::StaticCell;

use crate::{
    control_plane::{Identity, NetworkState, NetworkSummary},
    mdns::build_http_announcement,
    memory::{MEMORY_WIFI_PASSWORD_MAX_LEN, MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig},
    net_http::{
        ControlMailboxCommand, DeviceNames, HTTP_SERVICE_PORT, HttpGate, HttpMethod, HttpReadGate,
        HttpRequest, HttpResponse, LAN_HTTP_BODY_MAX_LEN, LAN_HTTP_LIGHT_BODY_MAX_LEN,
        LightHttpResponse, NetHttpState, device_names_from_mac, format_http_response_headers,
        http_socket_slot_count, http_workspace_slot_count, identity_from_device_names,
    },
    wifi_state::{
        SAVING_TIMEOUT_MS, WifiEvent as ProvisioningEvent, WifiProvisioningMachine, WifiTransition,
    },
};

// Three sockets at 1 KiB per direction use less static RAM than the previous
// two-socket 2 KiB layout while still covering the largest HTTP header and
// streaming larger bodies through the shared request workspace.
const HTTP_TCP_BUFFER_LEN: usize = 1024;
const HTTP_REQUEST_BUFFER_LEN: usize = LAN_HTTP_BODY_MAX_LEN + 1_024;
const HTTP_LIGHT_REQUEST_BUFFER_LEN: usize = 512;
// One mutation-capable request plus two independent readers cover the browser
// event/status path and lease heartbeat without allowing concurrent
// writes. Identity and network use the published snapshot path and never
// consume the mutation workspace.
const HTTP_ACTIVE_REQUEST_BUDGET: usize = 1;
const HTTP_SOCKET_COUNT: usize = http_socket_slot_count(HTTP_ACTIVE_REQUEST_BUDGET);
const HTTP_WORKSPACE_COUNT: usize = http_workspace_slot_count(HTTP_ACTIVE_REQUEST_BUDGET);
const MDNS_MULTICAST_V4: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
const WIFI_ASSOCIATION_TIMEOUT_SECS: u64 = 8;

type ControlStateMutex = Mutex<CriticalSectionRawMutex, Option<NetHttpState>>;
type WifiConfigMutex = Mutex<CriticalSectionRawMutex, WifiRuntimeConfig>;
type WifiProvisioningMutex = Mutex<CriticalSectionRawMutex, WifiProvisioningMachine>;
type RuntimeStatusMutex = BlockingMutex<CriticalSectionRawMutex, RefCell<LanRuntimeState>>;

static CONTROL_STATE: ControlStateMutex = Mutex::new(None);
// Request IDs prevent a late answer for a timed-out command from being
// attributed to the next request. The control loop remains the sole mutation
// executor even while the network listener is being recovered.
static CONTROL_MAILBOX: Channel<CriticalSectionRawMutex, ControlMailboxCommand, 4> = Channel::new();
static CONTROL_RESPONSES: [Signal<CriticalSectionRawMutex, ControlMailboxResponse>;
    HTTP_SOCKET_COUNT] = [const { Signal::new() }; HTTP_SOCKET_COUNT];
static CONTROL_REQUEST_ID: AtomicU32 = AtomicU32::new(1);
static CONTROL_REVISION: AtomicU32 = AtomicU32::new(0);
// USB/devd reads this mirror without taking the HTTP gate mutex. This keeps
// physical pairing automation responsive even while a LAN request is being
// authenticated or dispatched.
static PAIRING_CODE_MIRROR: AtomicU32 = AtomicU32::new(0);
static WIFI_CONFIG: WifiConfigMutex = Mutex::new(WifiRuntimeConfig::empty());
static WIFI_APPLY_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static LAN_RUNTIME: RuntimeStatusMutex = BlockingMutex::new(RefCell::new(LanRuntimeState::empty()));
static WIFI_PROVISIONING: WifiProvisioningMutex = Mutex::new(WifiProvisioningMachine::new());
static NET_RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
static NET_RUNNER: StaticCell<embassy_net::Runner<'static, WifiDevice<'static>>> =
    StaticCell::new();
static RADIO_CONTROLLER: StaticCell<RadioController<'static>> = StaticCell::new();
static WIFI_CONTROLLER: StaticCell<WifiController<'static>> = StaticCell::new();
// The mutation-capable HTTP worker owns its full request workspace outside the
// executor arena. Lightweight readers are served from published snapshots.
static HTTP_WORKSPACE_0: StaticCell<HttpWorkspace> = StaticCell::new();
static HTTP_RX_0: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_RX_1: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_RX_2: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_0: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_1: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_2: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_SOCKETS: Channel<CriticalSectionRawMutex, HttpSocket, HTTP_SOCKET_COUNT> =
    Channel::new();
static HTTP_WORKSPACES: Channel<CriticalSectionRawMutex, HttpWorkspaceSlot, HTTP_WORKSPACE_COUNT> =
    Channel::new();

struct HttpWorkspace {
    request: [u8; HTTP_REQUEST_BUFFER_LEN],
    response: HttpResponse,
    command: Option<ControlMailboxCommand>,
    control_response: Option<ControlMailboxResponse>,
}

impl HttpWorkspace {
    const fn new() -> Self {
        Self {
            request: [0; HTTP_REQUEST_BUFFER_LEN],
            response: HttpResponse {
                status: 500,
                allow_origin: None,
                allow_private_network: false,
                control_revision: None,
                body: String::new(),
            },
            command: None,
            control_response: None,
        }
    }
}

struct HttpWorkspaceSlot {
    workspace: &'static mut HttpWorkspace,
    response_slot: u8,
}

struct ParsedHttpHeader {
    method: Option<HttpMethod>,
    path: String<128>,
    origin: Option<String<128>>,
    authorization: Option<String<96>>,
    lease_id: Option<String<32>>,
    expected_revision: Option<u32>,
    private_network: bool,
    content_length: usize,
}

impl ParsedHttpHeader {
    fn request<'a>(&'a self, body: &'a str) -> Option<HttpRequest<'a>> {
        Some(HttpRequest {
            method: self.method?,
            path: self.path.as_str(),
            origin: self.origin.as_ref().map(String::as_str),
            authorization: self.authorization.as_ref().map(String::as_str),
            lease_id: self.lease_id.as_ref().map(String::as_str),
            expected_revision: self.expected_revision,
            request_private_network: self.private_network,
            body,
            entropy: random_entropy(),
        })
    }
}

/// `TcpSocket` remains on the same executor. The wrapper only transfers its
/// exclusive ownership between the listener and its one worker.
struct HttpSocket(TcpSocket<'static>);

unsafe impl Send for HttpSocket {}

struct ControlMailboxResponse {
    request_id: u32,
    status: u16,
    body: String<LAN_HTTP_BODY_MAX_LEN>,
    revision: u32,
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
            // Reconnect is a device policy. Legacy EEPROM values are read for
            // migration, but never become a runtime configuration input.
            auto_reconnect: true,
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
    PAIRING_CODE_MIRROR.store(0, Ordering::Release);
    CONTROL_REVISION.store(0, Ordering::Release);
}

pub async fn enter_pairing() -> Option<[u8; 4]> {
    let code = CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .pairing_code_from_random(random_u32());
    PAIRING_CODE_MIRROR.store(
        code.map(encode_pairing_code).unwrap_or(0),
        Ordering::Release,
    );
    code
}

pub async fn leave_pairing() {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .leave_pairing();
    PAIRING_CODE_MIRROR.store(0, Ordering::Release);
}

/// Applies a future physical pairing-policy setting without leaving a stale
/// code visible to USB automation. The production default remains `required`.
pub async fn set_pairing_mode(mode: crate::lan::LanPairingMode) {
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .set_pairing_mode(mode);
    PAIRING_CODE_MIRROR.store(0, Ordering::Release);
}

/// Reads the transient pairing code without extending or recreating the
/// front-panel-scoped pairing window.
pub fn pairing_code() -> Option<[u8; 4]> {
    decode_pairing_code(PAIRING_CODE_MIRROR.load(Ordering::Acquire))
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

pub async fn apply_wifi_config(memory: &MemoryConfig) -> NetworkSummary {
    let mut config = WifiRuntimeConfig::from_memory(memory);
    let mut current = WIFI_CONFIG.lock().await;
    config.hostname = current.hostname.clone();
    *current = config;
    let config = current.clone();
    drop(current);
    let event = if config.is_configured() {
        ProvisioningEvent::ApplyConfig
    } else {
        ProvisioningEvent::ClearConfig
    };
    let summary = publish_wifi_event(&config, event, None)
        .await
        .expect("WiFi configuration events are always accepted");
    WIFI_APPLY_SIGNAL.signal(());
    summary
}

/// Publish a failed LAN startup to USB status while keeping the main control
/// loop available for recovery and later firmware servicing.
pub async fn report_startup_failure(error: LanStartupError) {
    let config = WIFI_CONFIG.lock().await.clone();
    let _ = publish_wifi_event(
        &config,
        ProvisioningEvent::LanStartupFailed,
        Some(error.message()),
    )
    .await;
}

/// Returns the identity advertised by the initialized WiFi station. This is
/// separate from the USB development placeholder because LAN clients persist
/// it as their stable device key.
pub async fn lan_identity() -> Identity {
    let names = LAN_RUNTIME.lock(|runtime| runtime.borrow().device_names.clone());
    names
        .as_ref()
        .map(identity_from_device_names)
        .unwrap_or_else(Identity::firmware_default)
}

/// Returns the actual connection state published by the WiFi task. The TCP
/// listener never infers connectivity from EEPROM because a saved SSID is not
/// evidence that association and IPv4 configuration succeeded.
pub async fn lan_network_summary() -> NetworkSummary {
    LAN_RUNTIME.lock(|runtime| runtime.borrow().network.clone().unwrap_or_default())
}

pub fn try_receive_command() -> Option<ControlMailboxCommand> {
    CONTROL_MAILBOX.try_receive().ok()
}

pub fn current_control_revision() -> u32 {
    CONTROL_REVISION.load(Ordering::Acquire)
}

pub fn respond_to_command(
    response_slot: u8,
    request_id: u32,
    status: u16,
    body: String<LAN_HTTP_BODY_MAX_LEN>,
    advance_revision: bool,
) {
    let revision = if advance_revision && status == 200 {
        CONTROL_REVISION
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    } else {
        current_control_revision()
    };
    CONTROL_RESPONSES[usize::from(response_slot)].signal(ControlMailboxResponse {
        request_id,
        status,
        body,
        revision,
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

async fn await_control_response(
    response_slot: u8,
    request_id: u32,
) -> Option<ControlMailboxResponse> {
    loop {
        let response = with_timeout(
            Duration::from_secs(3),
            CONTROL_RESPONSES[usize::from(response_slot)].wait(),
        )
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
pub async fn spawn<F>(
    spawner: &Spawner,
    wifi_peripheral: WIFI<'static>,
    memory: &MemoryConfig,
    mut report_stage: F,
) -> Result<(), LanStartupError>
where
    F: FnMut(&'static [u8]),
{
    report_stage(b"boot_stage=wifi_radio_init_start\n");
    let radio = radio_init().map_err(|_| LanStartupError::WifiInitialization)?;
    report_stage(b"boot_stage=wifi_radio_init_complete\n");
    let radio = RADIO_CONTROLLER.init(radio);
    report_stage(b"boot_stage=wifi_interface_init_start\n");
    let (controller, interfaces) = wifi::new(radio, wifi_peripheral, wifi_driver_config())
        .map_err(|_| LanStartupError::WifiInitialization)?;
    report_stage(b"boot_stage=wifi_interface_init_complete\n");
    let station = interfaces.sta;
    let names = device_names_from_mac(station.mac_address());
    // Boot restoration is initialization, not a live reconfiguration. Do not
    // leave an apply signal pending: consuming it after the first DHCP lease
    // would disconnect the station while the state machine still says
    // `connected`, then wait forever for another configuration request.
    let mut initial_config = WifiRuntimeConfig::from_memory(memory);
    initial_config.hostname = Some(names.hostname.clone());
    *WIFI_CONFIG.lock().await = initial_config.clone();
    report_stage(b"boot_stage=wifi_config_ready\n");
    let initial_event = if initial_config.is_configured() {
        ProvisioningEvent::ApplyConfig
    } else {
        ProvisioningEvent::ClearConfig
    };
    let _ = publish_wifi_event(&initial_config, initial_event, None).await;
    report_stage(b"boot_stage=wifi_state_published\n");
    CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .set_device_names(names.clone());
    LAN_RUNTIME.lock(|runtime| runtime.borrow_mut().device_names = Some(names.clone()));
    let resources = NET_RESOURCES.init(StackResources::<8>::new());
    let seed = random_u64();
    let (stack, runner) = embassy_net::new(station, net_config(&initial_config), resources, seed);
    report_stage(b"boot_stage=wifi_network_stack_ready\n");
    let runner = NET_RUNNER.init(runner);
    let controller = WIFI_CONTROLLER.init(controller);

    spawner
        .spawn(wifi_task(controller, stack))
        .map_err(|_| LanStartupError::WifiTaskCapacity)?;
    spawner
        .spawn(network_task(runner))
        .map_err(|_| LanStartupError::NetworkTaskCapacity)?;
    report_stage(b"boot_stage=wifi_core_tasks_spawned\n");
    let sockets = [
        TcpSocket::new(
            stack,
            HTTP_RX_0.init([0; HTTP_TCP_BUFFER_LEN]),
            HTTP_TX_0.init([0; HTTP_TCP_BUFFER_LEN]),
        ),
        TcpSocket::new(
            stack,
            HTTP_RX_1.init([0; HTTP_TCP_BUFFER_LEN]),
            HTTP_TX_1.init([0; HTTP_TCP_BUFFER_LEN]),
        ),
        TcpSocket::new(
            stack,
            HTTP_RX_2.init([0; HTTP_TCP_BUFFER_LEN]),
            HTTP_TX_2.init([0; HTTP_TCP_BUFFER_LEN]),
        ),
    ];
    for mut socket in sockets {
        socket.set_timeout(Some(Duration::from_secs(5)));
        HTTP_SOCKETS.send(HttpSocket(socket)).await;
    }
    HTTP_WORKSPACES
        .send(HttpWorkspaceSlot {
            workspace: HTTP_WORKSPACE_0.init_with(HttpWorkspace::new),
            response_slot: 0,
        })
        .await;
    spawner
        .spawn(http_listener_task(stack, *spawner))
        .map_err(|_| LanStartupError::HttpTaskCapacity)?;
    spawner
        .spawn(mdns_task(stack, names))
        .map_err(|_| LanStartupError::MdnsTaskCapacity)?;
    report_stage(b"boot_stage=wifi_all_tasks_spawned\n");
    Ok(())
}

/// The LAN control plane has bounded, low-throughput traffic. esp-radio owns
/// these pools at runtime, so this budget must live next to its constructor
/// rather than in legacy esp-wifi build environment variables.
fn wifi_driver_config() -> wifi::Config {
    wifi::Config::default()
        .with_static_rx_buf_num(6)
        .with_dynamic_rx_buf_num(8)
        .with_dynamic_tx_buf_num(8)
        .with_rx_ba_win(6)
}

#[embassy_executor::task]
async fn network_task(runner: &'static mut embassy_net::Runner<'static, WifiDevice<'static>>) {
    runner.run().await;
}

enum WifiFailureFollowUp {
    RetryStarted,
    ConfigurationChanged,
    AwaitReconfiguration,
}

async fn progress_wifi_failure(
    config: &WifiRuntimeConfig,
    event: ProvisioningEvent,
    message: &'static str,
) -> WifiFailureFollowUp {
    let Some(summary) = publish_wifi_event(config, event, Some(message)).await else {
        return WifiFailureFollowUp::ConfigurationChanged;
    };
    let settled = matches!(summary.state, NetworkState::Error);
    if settled {
        return WifiFailureFollowUp::AwaitReconfiguration;
    }

    let delay = Duration::from_secs(2);
    match select(WIFI_APPLY_SIGNAL.wait(), Timer::after(delay)).await {
        Either::First(()) => WifiFailureFollowUp::ConfigurationChanged,
        Either::Second(()) => {
            let latest_config = WIFI_CONFIG.lock().await.clone();
            let _ = publish_wifi_event(&latest_config, ProvisioningEvent::RetryDelayElapsed, None)
                .await;
            WifiFailureFollowUp::RetryStarted
        }
    }
}

#[embassy_executor::task]
async fn wifi_task(controller: &'static mut WifiController<'static>, stack: Stack<'static>) {
    let mut retry_pending = false;
    loop {
        let config = WIFI_CONFIG.lock().await.clone();
        if !config.is_configured() {
            let _ = controller.stop_async().await;
            if !matches!(wifi_state().await, NetworkState::Disabled) {
                let _ = publish_wifi_event(&config, ProvisioningEvent::ClearConfig, None).await;
            }
            WIFI_APPLY_SIGNAL.wait().await;
            continue;
        }

        if matches!(wifi_state().await, NetworkState::Disabled) {
            let _ = publish_wifi_event(&config, ProvisioningEvent::ApplyConfig, None).await;
        }
        if retry_pending {
            let _ = publish_wifi_event(&config, ProvisioningEvent::RetryDelayElapsed, None).await;
            retry_pending = false;
        }
        if matches!(wifi_state().await, NetworkState::Saving) {
            let _ = publish_wifi_event(&config, ProvisioningEvent::DisconnectCompleted, None).await;
        }
        if !matches!(wifi_state().await, NetworkState::Connecting) {
            WIFI_APPLY_SIGNAL.wait().await;
            continue;
        }

        let client = ModeConfig::Client(
            ClientConfig::default()
                .with_ssid(alloc::string::String::from(config.ssid.as_str()))
                .with_password(alloc::string::String::from(config.password.as_str())),
        );
        stack.set_config_v4(net_config(&config).ipv4);
        if controller.set_config(&client).is_err()
            || (!matches!(controller.is_started(), Ok(true))
                && controller.start_async().await.is_err())
        {
            let follow_up = progress_wifi_failure(
                &config,
                ProvisioningEvent::DriverConfigurationFailed,
                "WiFi configuration could not be applied.",
            )
            .await;
            if matches!(follow_up, WifiFailureFollowUp::AwaitReconfiguration) {
                WIFI_APPLY_SIGNAL.wait().await;
            }
            continue;
        }
        let _ = publish_wifi_event(&config, ProvisioningEvent::DriverConfigured, None).await;
        // `connect_async` clears pending STA events before it waits. The radio
        // can associate between `start_async` and that clear, leaving the TCP
        // stack online while this task waits forever for an event it discarded.
        // Start association once, then observe the stack's current link state;
        // `wait_link_up` completes immediately when that event already arrived.
        let association_started =
            matches!(controller.is_connected(), Ok(true)) || controller.connect().is_ok();
        let association_timed_out = if association_started {
            with_timeout(
                Duration::from_secs(WIFI_ASSOCIATION_TIMEOUT_SECS),
                stack.wait_link_up(),
            )
            .await
            .is_err()
        } else {
            false
        };
        if !association_started || association_timed_out {
            let event = if association_timed_out {
                ProvisioningEvent::AssociationTimedOut
            } else {
                ProvisioningEvent::AssociationFailed
            };
            let follow_up = progress_wifi_failure(&config, event, "WiFi association failed.").await;
            if matches!(follow_up, WifiFailureFollowUp::AwaitReconfiguration) {
                WIFI_APPLY_SIGNAL.wait().await;
            }
            continue;
        }
        let _ = publish_wifi_event(&config, ProvisioningEvent::AssociationSucceeded, None).await;
        if with_timeout(Duration::from_secs(15), stack.wait_config_up())
            .await
            .is_err()
        {
            let _ = controller.disconnect_async().await;
            let follow_up = progress_wifi_failure(
                &config,
                ProvisioningEvent::Ipv4TimedOut,
                "Timed out waiting for IPv4 configuration.",
            )
            .await;
            if matches!(follow_up, WifiFailureFollowUp::AwaitReconfiguration) {
                WIFI_APPLY_SIGNAL.wait().await;
            }
            continue;
        }

        let connected = apply_wifi_transition(ProvisioningEvent::Ipv4Configured)
            .await
            .expect("IPv4 configuration follows a connecting state");
        let mut summary = network_connected(&config, &stack, &controller);
        summary.state = connected.state;
        summary.failure_code = connected.failure_code;
        summary.configuration_generation = connected.configuration_generation;
        summary.transition_sequence = connected.transition_sequence;
        set_network_summary(summary).await;

        match select(
            // Do not use `wait_for_event` here: esp-wifi clears the requested
            // event before waiting, which loses a disconnect that races with
            // DHCP completion and leaves the device reporting a stale link.
            controller.wait_for_events(WifiEvent::StaDisconnected.into(), false),
            WIFI_APPLY_SIGNAL.wait(),
        )
        .await
        {
            Either::First(_) if config.auto_reconnect => {
                let _ = publish_wifi_event(
                    &config,
                    ProvisioningEvent::StationDisconnected {
                        auto_reconnect: true,
                    },
                    None,
                )
                .await;
                Timer::after(Duration::from_secs(2)).await;
                retry_pending = true;
            }
            Either::First(_) => {
                let _ = publish_wifi_event(
                    &config,
                    ProvisioningEvent::StationDisconnected {
                        auto_reconnect: false,
                    },
                    Some("WiFi station disconnected."),
                )
                .await;
                WIFI_APPLY_SIGNAL.wait().await
            }
            Either::Second(()) => {
                let disconnected = with_timeout(
                    Duration::from_millis(SAVING_TIMEOUT_MS as u64),
                    controller.disconnect_async(),
                )
                .await;
                let latest_config = WIFI_CONFIG.lock().await.clone();
                if disconnected.is_ok() {
                    let _ = publish_wifi_event(
                        &latest_config,
                        ProvisioningEvent::DisconnectCompleted,
                        None,
                    )
                    .await;
                } else {
                    let follow_up = progress_wifi_failure(
                        &latest_config,
                        ProvisioningEvent::DisconnectTimedOut,
                        "Timed out while stopping WiFi.",
                    )
                    .await;
                    if matches!(follow_up, WifiFailureFollowUp::AwaitReconfiguration) {
                        WIFI_APPLY_SIGNAL.wait().await;
                    }
                }
            }
        }
    }
}

async fn set_network_summary(summary: NetworkSummary) -> NetworkSummary {
    LAN_RUNTIME.lock(|runtime| runtime.borrow_mut().network = Some(summary.clone()));
    summary
}

fn network_summary_for_config(config: &WifiRuntimeConfig, state: NetworkState) -> NetworkSummary {
    let public_state = match state {
        // The adapter keeps the disconnect stage private. The device only
        // publishes the user-visible connection phase.
        NetworkState::Saving => NetworkState::Connecting,
        // A bounded timeout is a settled failure, never a fourth WiFi state.
        NetworkState::Timeout => NetworkState::Error,
        NetworkState::Idle => {
            if config.is_configured() {
                NetworkState::Connecting
            } else {
                NetworkState::Disabled
            }
        }
        other => other,
    };
    NetworkSummary {
        state: public_state,
        ssid: config.is_configured().then(|| config.ssid.clone()),
        wifi_password_length: config
            .is_configured()
            .then_some(config.password.len() as u8)
            .unwrap_or(0),
        ..NetworkSummary::default()
    }
}

fn network_summary_for_transition(
    config: &WifiRuntimeConfig,
    transition: crate::wifi_state::WifiTransition,
    message: Option<&str>,
) -> NetworkSummary {
    let mut summary = network_summary_for_config(config, transition.state);
    summary.configuration_generation = transition.configuration_generation;
    summary.transition_sequence = transition.transition_sequence;
    summary.failure_code = transition.failure_code;
    if let Some(message) = message
        && transition.failure_code.is_some()
    {
        let mut error = String::new();
        let _ = error.push_str(message);
        summary.last_error = Some(error);
    }
    summary
}

async fn publish_wifi_event(
    config: &WifiRuntimeConfig,
    event: ProvisioningEvent,
    message: Option<&str>,
) -> Option<NetworkSummary> {
    let transition = apply_wifi_transition(event).await?;
    Some(set_network_summary(network_summary_for_transition(config, transition, message)).await)
}

async fn apply_wifi_transition(event: ProvisioningEvent) -> Option<WifiTransition> {
    let transition = WIFI_PROVISIONING
        .lock()
        .await
        .apply_at(event, Instant::now().as_millis());
    transition.accepted.then_some(transition)
}

async fn wifi_state() -> NetworkState {
    WIFI_PROVISIONING.lock().await.state()
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
async fn http_listener_task(stack: Stack<'static>, spawner: Spawner) {
    loop {
        stack.wait_config_up().await;
        let HttpSocket(mut socket) = HTTP_SOCKETS.receive().await;
        if socket.accept(HTTP_SERVICE_PORT).await.is_err() {
            HTTP_SOCKETS.send(HttpSocket(socket)).await;
            Timer::after(Duration::from_millis(200)).await;
            continue;
        }
        let _ = spawner.spawn(http_connection_task(HttpSocket(socket)));
    }
}

#[embassy_executor::task(pool_size = 3)]
async fn http_connection_task(socket: HttpSocket) {
    let mut socket = socket.0;
    let mut header = [0u8; HTTP_LIGHT_REQUEST_BUFFER_LEN];
    let header_len = read_http_header(&mut socket, &mut header)
        .await
        .unwrap_or(0);
    let light_handled = if header_len > 0 {
        handle_light_http_connection(&mut socket, &header[..header_len])
            .await
            .unwrap_or(true)
    } else {
        true
    };
    if !light_handled {
        let HttpWorkspaceSlot {
            workspace,
            response_slot,
        } = HTTP_WORKSPACES.receive().await;
        let _ = handle_http_connection(
            &mut socket,
            &mut workspace.request,
            &mut workspace.response,
            &mut workspace.command,
            &mut workspace.control_response,
            response_slot,
            &header[..header_len],
        )
        .await;
        HTTP_WORKSPACES
            .send(HttpWorkspaceSlot {
                workspace,
                response_slot,
            })
            .await;
    }
    socket.close();
    let _ = socket.flush().await;
    HTTP_SOCKETS.send(HttpSocket(socket)).await;
}

async fn read_http_header(
    socket: &mut TcpSocket<'_>,
    bytes: &mut [u8],
) -> Result<usize, embassy_net::tcp::Error> {
    let mut total = 0;
    while total < bytes.len() {
        let received = socket.read(&mut bytes[total..]).await?;
        if received == 0 {
            break;
        }
        total += received;
        if bytes[..total].windows(4).any(|value| value == b"\r\n\r\n") {
            break;
        }
    }
    Ok(total)
}

fn parse_http_header(bytes: &[u8]) -> ParsedHttpHeader {
    let mut parsed = ParsedHttpHeader {
        method: None,
        path: String::new(),
        origin: None,
        authorization: None,
        lease_id: None,
        expected_revision: None,
        private_network: false,
        content_length: 0,
    };
    let header = core::str::from_utf8(bytes).unwrap_or("");
    let mut lines = header.lines();
    let mut request_line = lines.next().unwrap_or("").split_whitespace();
    parsed.method = match request_line.next().unwrap_or("") {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "DELETE" => Some(HttpMethod::Delete),
        "OPTIONS" => Some(HttpMethod::Options),
        _ => None,
    };
    let _ = parsed.path.push_str(
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
            parsed.origin = Some(copied);
        }
        if key.eq_ignore_ascii_case("Authorization") {
            let mut copied = String::new();
            let _ = copied.push_str(value);
            parsed.authorization = Some(copied);
        }
        if key.eq_ignore_ascii_case("X-Flux-Purr-Lease") {
            let mut copied = String::new();
            let _ = copied.push_str(value);
            parsed.lease_id = Some(copied);
        }
        if key.eq_ignore_ascii_case("X-Flux-Purr-Revision") {
            parsed.expected_revision = value.parse().ok();
        }
        if key.eq_ignore_ascii_case("Access-Control-Request-Private-Network") {
            parsed.private_network = value.eq_ignore_ascii_case("true");
        }
        if key.eq_ignore_ascii_case("Content-Length") {
            parsed.content_length = value.parse().unwrap_or(0);
        }
    }
    parsed
}

async fn handle_light_http_connection(
    socket: &mut TcpSocket<'_>,
    header: &[u8],
) -> Result<bool, embassy_net::tcp::Error> {
    if !header.windows(4).any(|value| value == b"\r\n\r\n") {
        return Ok(false);
    }
    let parsed = parse_http_header(header);
    if parsed.content_length != 0 {
        return Ok(false);
    }
    let Some(request) = parsed.request("") else {
        return Ok(false);
    };
    let gate = CONTROL_STATE
        .lock()
        .await
        .as_mut()
        .expect("LAN control state initialized")
        .gate_light_read(request);
    match gate {
        HttpReadGate::Respond(response) => {
            write_light_http_response(socket, &response).await?;
            Ok(true)
        }
        HttpReadGate::Snapshot {
            endpoint,
            allow_origin,
        } => {
            let response = match endpoint {
                crate::lan::LanEndpoint::Identity => {
                    light_json_response(&lan_identity().await, allow_origin)
                }
                crate::lan::LanEndpoint::Network => {
                    light_json_response(&lan_network_summary().await, allow_origin)
                }
                _ => return Ok(false),
            };
            write_light_http_response(socket, &response).await?;
            Ok(true)
        }
        HttpReadGate::Defer => Ok(false),
    }
}

fn light_json_response<T: Serialize>(
    value: &T,
    allow_origin: Option<String<128>>,
) -> LightHttpResponse {
    let mut encoded = [0u8; LAN_HTTP_LIGHT_BODY_MAX_LEN];
    let body = match serde_json_core::to_slice(value, &mut encoded) {
        Ok(written) => {
            let mut body = String::new();
            let _ = body.push_str(core::str::from_utf8(&encoded[..written]).unwrap_or(""));
            body
        }
        Err(_) => {
            let mut body = String::new();
            let _ = body.push_str(r#"{"error":{"code":"snapshot_too_large","message":"Read snapshot exceeded the LAN envelope."}}"#);
            return LightHttpResponse {
                status: 500,
                allow_origin,
                allow_private_network: false,
                body,
            };
        }
    };
    LightHttpResponse {
        status: 200,
        allow_origin,
        allow_private_network: false,
        body,
    }
}

async fn write_light_http_response(
    socket: &mut TcpSocket<'_>,
    response: &LightHttpResponse,
) -> Result<(), embassy_net::tcp::Error> {
    let header = format_http_response_headers(
        response.status,
        response.body.len(),
        response.allow_origin.as_ref().map(String::as_str),
        response.allow_private_network,
        None,
        "application/json",
    );
    socket.write_all(header.as_bytes()).await?;
    socket.write_all(response.body.as_bytes()).await?;
    socket.flush().await
}

async fn handle_http_connection(
    socket: &mut TcpSocket<'_>,
    request_bytes: &mut [u8],
    response: &mut HttpResponse,
    command_slot: &mut Option<ControlMailboxCommand>,
    control_response_slot: &mut Option<ControlMailboxResponse>,
    worker_slot: u8,
    prefetched: &[u8],
) -> Result<(), embassy_net::tcp::Error> {
    request_bytes.fill(0);
    *command_slot = None;
    *control_response_slot = None;
    response.allow_origin = None;
    response.allow_private_network = false;
    response.control_revision = None;
    let mut total = prefetched.len().min(request_bytes.len());
    request_bytes[..total].copy_from_slice(&prefetched[..total]);
    while total < request_bytes.len()
        && !request_bytes[..total]
            .windows(4)
            .any(|value| value == b"\r\n\r\n")
    {
        let received = socket.read(&mut request_bytes[total..]).await?;
        if received == 0 {
            break;
        }
        total += received;
    }
    let header_end = request_bytes[..total]
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(total);
    let parsed = parse_http_header(&request_bytes[..header_end]);
    let Some(_method) = parsed.method else {
        *response = HttpResponse::new(
            405,
            r#"{"error":{"code":"method_not_allowed","message":"Unsupported HTTP method."}}"#,
        );
        return write_http_response(socket, response).await;
    };
    let content_length = parsed.content_length;
    if content_length > LAN_HTTP_BODY_MAX_LEN {
        *response = HttpResponse::new(
            400,
            r#"{"error":{"code":"body_too_large","message":"Request body exceeds the LAN API limit."}}"#,
        );
        return write_http_response(socket, response).await;
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
    let request = parsed
        .request(body)
        .expect("a recognized HTTP method builds a request");
    let dispatch = {
        let gate = CONTROL_STATE
            .lock()
            .await
            .as_mut()
            .expect("LAN control state initialized")
            .gate(Instant::now().as_millis(), request);
        stage_http_gate(gate, command_slot, response, worker_slot)
    };
    let Some((request_id, is_sse)) = dispatch else {
        return write_http_response(socket, response).await;
    };
    if CONTROL_MAILBOX
        .try_send(command_slot.take().expect("LAN command staged"))
        .is_err()
    {
        set_http_error(
            response,
            503,
            r#"{"error":{"code":"control_busy","message":"Control mailbox is busy."}}"#,
        );
        return write_http_response(socket, response).await;
    }
    *control_response_slot = await_control_response(worker_slot, request_id).await;
    let is_success = stage_control_response(response, control_response_slot.take());
    if is_sse && is_success {
        return write_sse_status_event(
            socket,
            response.body.as_str(),
            response.allow_origin.as_ref().map(String::as_str),
        )
        .await;
    }
    write_http_response(socket, response).await
}

fn stage_http_gate(
    gate: HttpGate,
    command_slot: &mut Option<ControlMailboxCommand>,
    response_slot: &mut HttpResponse,
    worker_slot: u8,
) -> Option<(u32, bool)> {
    match gate {
        HttpGate::Respond(response) => {
            *response_slot = response;
            None
        }
        HttpGate::Dispatch {
            mut command,
            allow_origin,
        } => {
            command.request_id = next_control_request_id();
            command.response_slot = worker_slot;
            let request_id = command.request_id;
            let is_sse = command.endpoint == crate::lan::LanEndpoint::Events
                && command.method == HttpMethod::Get;
            response_slot.allow_origin = allow_origin;
            *command_slot = Some(command);
            Some((request_id, is_sse))
        }
    }
}

fn stage_control_response(
    response_slot: &mut HttpResponse,
    control: Option<ControlMailboxResponse>,
) -> bool {
    match control {
        Some(control) => {
            response_slot.status = control.status;
            response_slot.body = control.body;
            response_slot.control_revision = Some(control.revision);
            control.status == 200
        }
        None => {
            set_http_error(
                response_slot,
                504,
                r#"{"error":{"code":"control_timeout","message":"Control loop did not respond."}}"#,
            );
            false
        }
    }
}

fn set_http_error(response_slot: &mut HttpResponse, status: u16, body: &str) {
    response_slot.status = status;
    response_slot.allow_private_network = false;
    response_slot.body.clear();
    let _ = response_slot.body.push_str(body);
}

async fn write_http_response(
    socket: &mut TcpSocket<'_>,
    response: &HttpResponse,
) -> Result<(), embassy_net::tcp::Error> {
    write_http_response_with_type(
        socket,
        response.status,
        response.body.as_str(),
        response.allow_origin.as_ref().map(String::as_str),
        response.allow_private_network,
        response.control_revision,
        "application/json",
    )
    .await
}

async fn write_sse_status_event(
    socket: &mut TcpSocket<'_>,
    status: &str,
    allow_origin: Option<&str>,
) -> Result<(), embassy_net::tcp::Error> {
    const SSE_PREFIX: &[u8] = b"event: status\ndata: ";
    const SSE_SUFFIX: &[u8] = b"\n\n";
    write_http_response_headers(
        socket,
        200,
        SSE_PREFIX.len() + status.len() + SSE_SUFFIX.len(),
        allow_origin,
        false,
        Some(current_control_revision()),
        "text/event-stream",
    )
    .await?;
    socket.write_all(SSE_PREFIX).await?;
    socket.write_all(status.as_bytes()).await?;
    socket.write_all(SSE_SUFFIX).await?;
    socket.flush().await
}

async fn write_http_response_with_type(
    socket: &mut TcpSocket<'_>,
    response_status: u16,
    body: &str,
    allow_origin: Option<&str>,
    allow_private_network: bool,
    control_revision: Option<u32>,
    content_type: &str,
) -> Result<(), embassy_net::tcp::Error> {
    write_http_response_headers(
        socket,
        response_status,
        body.len(),
        allow_origin,
        allow_private_network,
        control_revision,
        content_type,
    )
    .await?;
    socket.write_all(body.as_bytes()).await?;
    socket.flush().await
}

async fn write_http_response_headers(
    socket: &mut TcpSocket<'_>,
    response_status: u16,
    body_len: usize,
    allow_origin: Option<&str>,
    allow_private_network: bool,
    control_revision: Option<u32>,
    content_type: &str,
) -> Result<(), embassy_net::tcp::Error> {
    let header = format_http_response_headers(
        response_status,
        body_len,
        allow_origin,
        allow_private_network,
        control_revision,
        content_type,
    );
    socket.write_all(header.as_bytes()).await
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
    let length = build_http_announcement(buffer, names, ip.octets()).ok_or(())?;
    socket
        .send_to(&buffer[..length], endpoint)
        .await
        .map_err(|_| ())
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

fn encode_pairing_code(code: [u8; 4]) -> u32 {
    let mut value = 0u32;
    for digit in code {
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(digit.saturating_sub(b'0')));
    }
    // Zero represents inactive, while 0000 remains a valid four-digit code.
    value.saturating_add(1)
}

fn decode_pairing_code(value: u32) -> Option<[u8; 4]> {
    let mut value = value.checked_sub(1)?;
    if value > 9_999 {
        return None;
    }
    let mut code = [b'0'; 4];
    for digit in code.iter_mut().rev() {
        *digit = b'0' + (value % 10) as u8;
        value /= 10;
    }
    Some(code)
}

fn random_u32() -> u32 {
    // The ESP RNG peripheral is shareable and WiFi keeps the RF entropy source
    // enabled. The register facade has no owned state, avoiding a weaker
    // counter-based pairing code.
    Rng::new().random()
}

#[cfg(test)]
mod tests {
    use super::{decode_pairing_code, encode_pairing_code};

    #[test]
    fn pairing_code_mirror_preserves_leading_zeroes() {
        assert_eq!(
            decode_pairing_code(encode_pairing_code(*b"0042")),
            Some(*b"0042")
        );
    }

    #[test]
    fn pairing_code_mirror_reserves_zero_for_inactive() {
        assert_eq!(decode_pairing_code(0), None);
    }

    #[test]
    fn public_wifi_summary_normalizes_internal_states() {
        let mut configured = WifiRuntimeConfig::empty();
        configured.ssid.push_str("FluxPurr-Lab").unwrap();

        assert_eq!(
            network_summary_for_config(&configured, NetworkState::Saving).state,
            NetworkState::Connecting
        );
        assert_eq!(
            network_summary_for_config(&configured, NetworkState::Timeout).state,
            NetworkState::Error
        );
        assert_eq!(
            network_summary_for_config(&configured, NetworkState::Idle).state,
            NetworkState::Connecting
        );
        assert_eq!(
            network_summary_for_config(&WifiRuntimeConfig::empty(), NetworkState::Idle).state,
            NetworkState::Disabled
        );
    }
}
