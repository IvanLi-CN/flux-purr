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
    mdns::build_http_announcement,
    memory::{MEMORY_WIFI_PASSWORD_MAX_LEN, MEMORY_WIFI_SSID_MAX_LEN, MemoryConfig},
    net_http::{
        ControlMailboxCommand, DeviceNames, HTTP_SERVICE_PORT, HttpGate, HttpMethod, HttpRequest,
        HttpResponse, LAN_HTTP_BODY_MAX_LEN, NetHttpState, device_names_from_mac,
        identity_from_device_names,
    },
    wifi_state::{
        SAVING_TIMEOUT_MS, WifiEvent as ProvisioningEvent, WifiProvisioningMachine, WifiTransition,
    },
};

// TCP buffering only needs to cover in-flight segments: request parsing reads
// the body incrementally into its dedicated workspace. Keeping these separate
// avoids reserving the full request size three times per connection.
const HTTP_TCP_BUFFER_LEN: usize = 2 * 1024;
const HTTP_REQUEST_BUFFER_LEN: usize = LAN_HTTP_BODY_MAX_LEN + 1_024;
const HTTP_CONNECTION_COUNT: usize = 3;
const MDNS_MULTICAST_V4: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;
const WIFI_ASSOCIATION_TIMEOUT_SECS: u64 = 8;
const DEBUG_SPAWN_NETWORK_TASK: bool = true;
const DEBUG_SPAWN_WIFI_TASK: bool = true;
const DEBUG_SPAWN_HTTP_TASK: bool = true;
const DEBUG_SPAWN_MDNS_TASK: bool = true;

type ControlStateMutex = Mutex<CriticalSectionRawMutex, Option<NetHttpState>>;
type WifiConfigMutex = Mutex<CriticalSectionRawMutex, WifiRuntimeConfig>;
type WifiProvisioningMutex = Mutex<CriticalSectionRawMutex, WifiProvisioningMachine>;
type RuntimeStatusMutex = BlockingMutex<CriticalSectionRawMutex, RefCell<LanRuntimeState>>;

static CONTROL_STATE: ControlStateMutex = Mutex::new(None);
// Each HTTP worker has one in-flight command and response. Request IDs prevent
// a late answer for a timed-out command from being attributed to another
// connection while the bounded worker pool still permits concurrent reads.
static CONTROL_MAILBOX: Channel<CriticalSectionRawMutex, ControlMailboxCommand, 4> = Channel::new();
static CONTROL_RESPONSES: [Signal<CriticalSectionRawMutex, ControlMailboxResponse>;
    HTTP_CONNECTION_COUNT] = [const { Signal::new() }; HTTP_CONNECTION_COUNT];
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
static WIFI_CONTROLLER: StaticCell<WifiController<'static>> = StaticCell::new();
static HTTP_WORKSPACE_0: StaticCell<HttpWorkspace> = StaticCell::new();
static HTTP_WORKSPACE_1: StaticCell<HttpWorkspace> = StaticCell::new();
static HTTP_WORKSPACE_2: StaticCell<HttpWorkspace> = StaticCell::new();
static HTTP_RX_0: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_RX_1: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_RX_2: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_0: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_1: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_TX_2: StaticCell<[u8; HTTP_TCP_BUFFER_LEN]> = StaticCell::new();
static HTTP_SOCKETS: Channel<CriticalSectionRawMutex, HttpSocket, HTTP_CONNECTION_COUNT> =
    Channel::new();
static HTTP_WORKSPACES: Channel<CriticalSectionRawMutex, HttpWorkspaceSlot, HTTP_CONNECTION_COUNT> =
    Channel::new();

// Request and response buffers stay outside the Embassy task frames. Socket
// buffers are separate so an accepted TcpSocket can move into a worker task
// and return to the idle pool without self-referential workspace state.
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

/// `TcpSocket` is intentionally `!Send` because embassy-net protects the
/// stack with a single-core `RefCell`. The socket pool only moves ownership
/// between non-Send tasks on that same executor; no socket is ever accessed
/// concurrently. The wrapper makes that ownership transfer explicit to the
/// static channel without exposing the socket to another executor or core.
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
    // Boot restoration is initialization, not a live reconfiguration. Do not
    // leave an apply signal pending: consuming it after the first DHCP lease
    // would disconnect the station while the state machine still says
    // `connected`, then wait forever for another configuration request.
    let mut initial_config = WifiRuntimeConfig::from_memory(memory);
    initial_config.hostname = Some(names.hostname.clone());
    *WIFI_CONFIG.lock().await = initial_config.clone();
    let initial_event = if initial_config.is_configured() {
        ProvisioningEvent::ApplyConfig
    } else {
        ProvisioningEvent::ClearConfig
    };
    let _ = publish_wifi_event(&initial_config, initial_event, None).await;
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
    // Keep large drivers out of executor task frames so USB recovery and all
    // LAN tasks fit in the bounded shared arena.
    let runner = NET_RUNNER.init(runner);
    let controller = WIFI_CONTROLLER.init(controller);

    if DEBUG_SPAWN_NETWORK_TASK {
        spawner
            .spawn(network_task(runner))
            .map_err(|_| LanStartupError::NetworkTaskCapacity)?;
    }
    if DEBUG_SPAWN_WIFI_TASK {
        spawner
            .spawn(wifi_task(controller, stack))
            .map_err(|_| LanStartupError::WifiTaskCapacity)?;
    }
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
    let workspaces = [
        HTTP_WORKSPACE_0.init_with(HttpWorkspace::new),
        HTTP_WORKSPACE_1.init_with(HttpWorkspace::new),
        HTTP_WORKSPACE_2.init_with(HttpWorkspace::new),
    ];
    for (response_slot, (mut socket, workspace)) in sockets.into_iter().zip(workspaces).enumerate()
    {
        socket.set_timeout(Some(Duration::from_secs(5)));
        HTTP_SOCKETS.send(HttpSocket(socket)).await;
        HTTP_WORKSPACES
            .send(HttpWorkspaceSlot {
                workspace,
                response_slot: response_slot as u8,
            })
            .await;
    }
    if DEBUG_SPAWN_HTTP_TASK {
        spawner
            .spawn(http_listener_task(stack, *spawner))
            .map_err(|_| LanStartupError::HttpTaskCapacity)?;
    }
    if DEBUG_SPAWN_MDNS_TASK {
        spawner
            .spawn(mdns_task(stack, names))
            .map_err(|_| LanStartupError::MdnsTaskCapacity)?;
    }
    Ok(())
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
        let association = with_timeout(
            Duration::from_secs(WIFI_ASSOCIATION_TIMEOUT_SECS),
            controller.connect_async(),
        )
        .await;
        if !matches!(association, Ok(Ok(()))) {
            let event = if association.is_err() {
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
        let workspace = HTTP_WORKSPACES.receive().await;
        let HttpSocket(mut socket) = HTTP_SOCKETS.receive().await;
        if socket.accept(HTTP_SERVICE_PORT).await.is_err() {
            HTTP_WORKSPACES.send(workspace).await;
            HTTP_SOCKETS.send(HttpSocket(socket)).await;
            Timer::after(Duration::from_millis(200)).await;
            continue;
        }

        // The idle socket count and worker pool are identical, so a valid
        // accepted socket always has a worker slot available.
        let _ = spawner.spawn(http_connection_task(HttpSocket(socket), workspace));
    }
}

#[embassy_executor::task(pool_size = HTTP_CONNECTION_COUNT)]
async fn http_connection_task(socket: HttpSocket, slot: HttpWorkspaceSlot) {
    let mut socket = socket.0;
    let HttpWorkspaceSlot {
        workspace,
        response_slot,
    } = slot;
    let _ = handle_http_connection(
        &mut socket,
        &mut workspace.request,
        &mut workspace.response,
        &mut workspace.command,
        &mut workspace.control_response,
        response_slot,
    )
    .await;
    socket.close();
    let _ = socket.flush().await;
    HTTP_WORKSPACES
        .send(HttpWorkspaceSlot {
            workspace,
            response_slot,
        })
        .await;
    HTTP_SOCKETS.send(HttpSocket(socket)).await;
}

async fn handle_http_connection(
    socket: &mut TcpSocket<'_>,
    request_bytes: &mut [u8],
    response: &mut HttpResponse,
    command_slot: &mut Option<ControlMailboxCommand>,
    control_response_slot: &mut Option<ControlMailboxResponse>,
    worker_slot: u8,
) -> Result<(), embassy_net::tcp::Error> {
    request_bytes.fill(0);
    *command_slot = None;
    *control_response_slot = None;
    response.allow_origin = None;
    response.allow_private_network = false;
    response.control_revision = None;
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
    let mut expected_revision = None::<u32>;
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
            _ => {
                *response = HttpResponse::new(
                    405,
                    r#"{"error":{"code":"method_not_allowed","message":"Unsupported HTTP method."}}"#,
                );
                return write_http_response(socket, response).await;
            }
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
            if key.eq_ignore_ascii_case("X-Flux-Purr-Revision") {
                expected_revision = value.parse().ok();
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
    let request = HttpRequest {
        method,
        path: path.as_str(),
        origin: origin.as_ref().map(String::as_str),
        authorization: authorization.as_ref().map(String::as_str),
        lease_id: lease_id.as_ref().map(String::as_str),
        expected_revision,
        request_private_network: private_network,
        body,
        entropy: random_entropy(),
    };
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
    let mut header = String::<384>::new();
    let _ = write!(
        header,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Headers: Authorization, Content-Type, X-Flux-Purr-Lease, X-Flux-Purr-Revision\r\nAccess-Control-Expose-Headers: X-Flux-Purr-Revision\r\nAccess-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n",
        body_len
    );
    if let Some(revision) = control_revision {
        let _ = write!(header, "X-Flux-Purr-Revision: {revision}\r\n");
    }
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
    // enabled. `Rng` is a zero-sized register facade, so stealing it here does
    // not duplicate owned DMA/state and avoids weakening pairing to a counter.
    let peripheral = unsafe { esp_hal::peripherals::RNG::steal() };
    Rng::new(peripheral).random()
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
