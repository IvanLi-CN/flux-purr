use super::*;
use espflash::{
    command::{Command as RomCommand, CommandType},
    connection::{Connection, Port, ResetAfterOperation, ResetBeforeOperation},
    target::Chip,
};
use flux_purr_devd::PRODUCT_BUILD_ID;
use flux_purr_devd::serial::{
    ESP32S3_USB_SERIAL_JTAG_PID, ESP32S3_USB_SERIAL_JTAG_VID, SerialPortProcessLock,
    UsbSerialIdentity, serial_port_paths_match, serial_port_usb_identity_matches,
};
#[cfg(target_os = "macos")]
use flux_purr_devd::serial::{RawUsbSerialJtagPort, open_raw_usb_serial_jtag_port};
use serialport::{FlowControl, SerialPort, SerialPortType, UsbPortInfo};
use std::time::{Duration, Instant};

const IDENTITY_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const PD_HIL_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const PD_HIL_SERIAL_RECONNECT_ATTEMPTS: u16 = 300;
const PD_HIL_SERIAL_RECONNECT_DELAY: Duration = Duration::from_millis(10);
const PD_HIL_EVIDENCE_FLUSH_INTERVAL: usize = 32;
const RAM_OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const BUTTON_INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(30);
const BUTTON_MAX_EVENTS: usize = 1024;
const BUTTON_SAMPLE_INTERVAL: Duration = Duration::from_millis(50);
const BUTTON_LONG_PRESS: Duration = Duration::from_millis(700);
const BUTTON_DOUBLE_CLICK: Duration = Duration::from_millis(350);
const FAN_MIN_INPUT_MV: u64 = 12_500;
const FAN_PWM_STAGE_DURATION_MS: u64 = 5_000;
const RAM_ELF_RELATIVE_PATH: &str =
    "firmware/ram-bringup/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup";
const PD_HIL_ELF_RELATIVE_PATH: &str =
    "firmware/ram-pd-hil/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-pd-hil";
const RAM_PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
const RAM_FRAMING: &str = "jsonl";
const PD_HIL_CAPABILITY: &str = "test_pd_sink";

#[cfg(target_os = "macos")]
type ReconnectedPdHilSerial = RawUsbSerialJtagPort;
#[cfg(not(target_os = "macos"))]
type ReconnectedPdHilSerial = Port;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedFirmware {
    Product,
    RamBringup,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityBody {
    #[serde(default)]
    firmware_kind: Option<String>,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    git_sha: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    framing: Option<String>,
    #[serde(default)]
    reset_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityFrame {
    #[serde(rename = "type")]
    frame_type: Option<String>,
    #[serde(default)]
    firmware_kind: Option<String>,
    #[serde(default)]
    identity: Option<IdentityBody>,
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    framing: Option<String>,
}

#[derive(Debug, Clone)]
struct ObservedIdentity {
    firmware: ObservedFirmware,
    build_id: Option<String>,
    source_sha: Option<String>,
    capabilities: Vec<String>,
    protocol_version: Option<String>,
    framing: Option<String>,
    reset_reason: Option<String>,
}

pub(crate) fn execute_ram_run(
    command: RamRunCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        RamRunCommand::Preview(args) => {
            let color = match (args.scenario, args.color) {
                (RamPreviewScenario::Display, color) => color.map(RamPreviewColor::wire),
                (_, None) => None,
                (_, Some(_)) => {
                    return Err("--color is only valid with 'ram-run preview display'".into());
                }
            };
            run_ram_operation(
                &args.port,
                args.elf.as_deref(),
                args.reload,
                args.scenario.op(),
                color,
            )
        }
        RamRunCommand::Test(args) if args.test == RamTestKind::Buttons => {
            run_interactive_buttons(&args.port, args.elf.as_deref(), args.reload)
        }
        RamRunCommand::Test(args) if args.test == RamTestKind::PdSink => run_pd_hil(&args),
        RamRunCommand::Test(args) if args.skip_vin_validation => {
            Err("--skip-vin-validation is only valid with 'ram-run test pd-sink'".into())
        }
        RamRunCommand::Test(args) => run_ram_operation(
            &args.port,
            args.elf.as_deref(),
            args.reload,
            args.test.op(),
            None,
        ),
        RamRunCommand::Exit(args) => exit_ram(&args.port),
    }
}

fn run_ram_operation(
    port: &str,
    elf: Option<&Path>,
    reload: bool,
    op: &str,
    color: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut session = open_ram_session(port, elf, reload, op)?;
    send_ram_request(&mut session.serial, op, color)
}

fn run_pd_hil(args: &RamTestArgs) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let evidence_dir = args
        .evidence_dir
        .as_deref()
        .ok_or("ram-run test pd-sink requires --evidence-dir")?;
    let session = open_ram_session(
        &args.port,
        args.elf.as_deref(),
        args.reload,
        PD_HIL_CAPABILITY,
    )?;
    send_pd_hil_request(&args.port, session, evidence_dir, !args.skip_vin_validation)
}

struct RamSession {
    _serial_lock: SerialPortProcessLock,
    usb_identity: UsbSerialIdentity,
    identity: ObservedIdentity,
    serial: Port,
}

fn open_ram_session(
    port: &str,
    elf: Option<&Path>,
    reload: bool,
    op: &str,
) -> Result<RamSession, Box<dyn std::error::Error + Send + Sync>> {
    let usb_identity = validate_exact_ram_port(port)?;
    let serial_lock = acquire_ram_port_lock(port)?;
    ensure_ram_target(port, &usb_identity)?;
    let observed = if reload {
        None
    } else {
        read_identity(port, &usb_identity).ok()
    };
    let matching_ram = observed.as_ref().is_some_and(|(identity, _)| {
        identity.firmware == ObservedFirmware::RamBringup
            && identity.build_id.as_deref() == Some(PRODUCT_BUILD_ID)
            && identity
                .capabilities
                .iter()
                .any(|capability| capability == op)
    });
    let (identity, serial) = if matching_ram {
        observed.expect("matching RAM identity must exist")
    } else {
        drop(observed);
        let elf = elf
            .map(Path::to_path_buf)
            .unwrap_or_else(|| default_ram_elf_for(op));
        validate_local_elf(&elf)?;
        let image = read_validated_ram_elf(&elf)?;
        load_ram_elf(port, image, &usb_identity)?
    };
    verify_ram_identity(&identity, op)?;
    ensure_ram_target(port, &usb_identity)?;
    Ok(RamSession {
        _serial_lock: serial_lock,
        usb_identity,
        identity,
        serial,
    })
}

const BUTTON_NAMES: [&str; 5] = ["center", "right", "down", "left", "up"];

#[derive(Debug, Clone, Copy)]
struct ButtonSnapshot {
    pressed: [bool; 5],
}

#[derive(Debug, Clone, Copy)]
struct ButtonGestureEvent {
    key: &'static str,
    gesture: &'static str,
    triggered_at: Instant,
    triggered_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonInteractionStopReason {
    InactivityTimeout,
    EventLimit,
}

impl ButtonInteractionStopReason {
    fn wire_value(self) -> &'static str {
        match self {
            Self::InactivityTimeout => "inactivity_timeout",
            Self::EventLimit => "event_limit",
        }
    }
}

impl ButtonGestureEvent {
    fn new(key: &'static str, gesture: &'static str, now: Instant) -> Self {
        Self {
            key,
            gesture,
            triggered_at: now,
            triggered_at_unix_ms: current_unix_millis(),
        }
    }

    fn effect(self) -> &'static str {
        match self.gesture {
            "short_press" => "success",
            "double_click" => "accent",
            "long_press" => "info_cyan",
            _ => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ButtonKeyState {
    pressed: bool,
    pressed_at: Option<Instant>,
    long_reported: bool,
    pending_short_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ButtonGestureTracker {
    keys: [ButtonKeyState; 5],
}

impl ButtonGestureTracker {
    fn observe(&mut self, snapshot: ButtonSnapshot, now: Instant) -> Vec<ButtonGestureEvent> {
        let mut events = Vec::new();
        for (index, state) in self.keys.iter_mut().enumerate() {
            if snapshot.pressed[index] {
                Self::observe_pressed(index, state, now, &mut events);
            } else {
                Self::observe_released(index, state, now, &mut events);
            }
        }
        events
    }

    fn observe_pressed(
        index: usize,
        state: &mut ButtonKeyState,
        now: Instant,
        events: &mut Vec<ButtonGestureEvent>,
    ) {
        if !state.pressed {
            Self::flush_expired_short(index, state, now, events);
            state.pressed = true;
            state.pressed_at = Some(now);
            state.long_reported = false;
        }
        if !Self::long_press_due(state, now) {
            return;
        }
        events.push(ButtonGestureEvent::new(
            BUTTON_NAMES[index],
            "long_press",
            now,
        ));
        state.long_reported = true;
        state.pending_short_at = None;
    }

    fn observe_released(
        index: usize,
        state: &mut ButtonKeyState,
        now: Instant,
        events: &mut Vec<ButtonGestureEvent>,
    ) {
        if state.pressed {
            Self::record_release(index, state, now, events);
        }
        Self::flush_expired_short(index, state, now, events);
    }

    fn record_release(
        index: usize,
        state: &mut ButtonKeyState,
        now: Instant,
        events: &mut Vec<ButtonGestureEvent>,
    ) {
        state.pressed = false;
        if state.long_reported {
            state.pending_short_at = None;
        } else {
            match state.pending_short_at {
                Some(pending) if now.duration_since(pending) <= BUTTON_DOUBLE_CLICK => {
                    events.push(ButtonGestureEvent::new(
                        BUTTON_NAMES[index],
                        "double_click",
                        now,
                    ));
                    state.pending_short_at = None;
                }
                Some(_) => {
                    events.push(ButtonGestureEvent::new(
                        BUTTON_NAMES[index],
                        "short_press",
                        now,
                    ));
                    state.pending_short_at = Some(now);
                }
                None => state.pending_short_at = Some(now),
            }
        }
        state.pressed_at = None;
        state.long_reported = false;
    }

    fn flush_expired_short(
        index: usize,
        state: &mut ButtonKeyState,
        now: Instant,
        events: &mut Vec<ButtonGestureEvent>,
    ) {
        if !Self::short_press_due(state, now) {
            return;
        }
        events.push(ButtonGestureEvent::new(
            BUTTON_NAMES[index],
            "short_press",
            now,
        ));
        state.pending_short_at = None;
    }

    fn flush(&mut self, now: Instant) -> Vec<ButtonGestureEvent> {
        self.keys
            .iter_mut()
            .enumerate()
            .filter_map(|(index, state)| Self::flush_state(index, state, now))
            .collect()
    }

    fn flush_state(
        index: usize,
        state: &mut ButtonKeyState,
        now: Instant,
    ) -> Option<ButtonGestureEvent> {
        if !state.pressed {
            return state
                .pending_short_at
                .take()
                .map(|_| ButtonGestureEvent::new(BUTTON_NAMES[index], "short_press", now));
        }
        if !Self::long_press_due(state, now) {
            return None;
        }
        state.long_reported = true;
        Some(ButtonGestureEvent::new(
            BUTTON_NAMES[index],
            "long_press",
            now,
        ))
    }

    fn long_press_due(state: &ButtonKeyState, now: Instant) -> bool {
        !state.long_reported
            && state
                .pressed_at
                .is_some_and(|started| now.duration_since(started) >= BUTTON_LONG_PRESS)
    }

    fn short_press_due(state: &ButtonKeyState, now: Instant) -> bool {
        state
            .pending_short_at
            .is_some_and(|pending| now.duration_since(pending) > BUTTON_DOUBLE_CLICK)
    }
}

fn button_snapshot(
    value: &Value,
) -> Result<ButtonSnapshot, Box<dyn std::error::Error + Send + Sync>> {
    let buttons = value
        .get("result")
        .and_then(|result| result.get("buttons"))
        .and_then(Value::as_object)
        .ok_or("RAM button response is missing result.buttons")?;
    let mut pressed = [false; 5];
    for (index, name) in BUTTON_NAMES.into_iter().enumerate() {
        pressed[index] = buttons
            .get(name)
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("RAM button response is missing result.buttons.{name}"))?;
    }
    Ok(ButtonSnapshot { pressed })
}

fn print_button_event(event: ButtonGestureEvent, session_started: Instant) {
    let elapsed_ms = event
        .triggered_at
        .saturating_duration_since(session_started)
        .as_millis();
    eprintln!(
        "RAM BUTTON EVENT at=+{}ms unixMs={} key={} gesture={} effect={}",
        elapsed_ms,
        event.triggered_at_unix_ms,
        event.key,
        event.gesture,
        event.effect()
    );
}

fn button_event_value(event: ButtonGestureEvent, session_started: Instant) -> Value {
    let elapsed_ms = event
        .triggered_at
        .saturating_duration_since(session_started)
        .as_millis();
    serde_json::json!({
        "key": event.key,
        "gesture": event.gesture,
        "effect": event.effect(),
        "elapsedMs": elapsed_ms,
        "triggeredAtUnixMs": event.triggered_at_unix_ms,
    })
}

fn add_button_interaction_result(
    mut value: Value,
    events: &[ButtonGestureEvent],
    session_started: Instant,
    stop_reason: ButtonInteractionStopReason,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let result = value
        .get_mut("result")
        .and_then(Value::as_object_mut)
        .ok_or("RAM button response is missing result object")?;
    let detail = match stop_reason {
        ButtonInteractionStopReason::EventLimit => "buttons_interactive_event_limit",
        ButtonInteractionStopReason::InactivityTimeout if events.is_empty() => {
            "buttons_interactive_timeout"
        }
        ButtonInteractionStopReason::InactivityTimeout => "buttons_interactive_complete",
    };
    result.insert("detail".to_string(), Value::String(detail.to_string()));
    result.insert(
        "interaction".to_string(),
        serde_json::json!({
            "timeoutSeconds": BUTTON_INTERACTIVE_TIMEOUT.as_secs(),
            "inactivityTimeoutSeconds": BUTTON_INTERACTIVE_TIMEOUT.as_secs(),
            "stopReason": stop_reason.wire_value(),
            "events": events.iter().copied().map(|event| button_event_value(event, session_started)).collect::<Vec<_>>(),
        }),
    );
    Ok(value)
}

fn run_interactive_buttons(
    port: &str,
    elf: Option<&Path>,
    reload: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut session = open_ram_session(port, elf, reload, "test_buttons")?;
    let session_started = Instant::now();
    eprintln!(
        "RAM BUTTONS READY: short press, long press, or double click any key; the inactivity timeout is {} seconds after the last button event.",
        BUTTON_INTERACTIVE_TIMEOUT.as_secs()
    );
    let mut inactivity_deadline = session_started + BUTTON_INTERACTIVE_TIMEOUT;
    let mut tracker = ButtonGestureTracker::default();
    let mut events = Vec::new();
    let mut latest = None;
    let mut previous_snapshot: Option<ButtonSnapshot> = None;
    let mut stop_reason = ButtonInteractionStopReason::InactivityTimeout;
    while Instant::now() < inactivity_deadline {
        ensure_ram_target(port, &session.usb_identity)?;
        let value = match send_ram_request_until(
            &mut session.serial,
            "test_buttons",
            None,
            inactivity_deadline,
        ) {
            Ok(value) => value,
            Err(_error) if Instant::now() >= inactivity_deadline => break,
            Err(error) => return Err(error),
        };
        let snapshot = button_snapshot(&value)?;
        latest = Some(value);
        let observed_at = Instant::now();
        let observed_events = tracker.observe(snapshot, observed_at);
        let button_activity =
            previous_snapshot.is_some_and(|previous| previous.pressed != snapshot.pressed);
        if button_activity || !observed_events.is_empty() {
            inactivity_deadline = observed_at + BUTTON_INTERACTIVE_TIMEOUT;
        }
        previous_snapshot = Some(snapshot);
        for event in observed_events {
            if !try_push_button_event(&mut events, event) {
                stop_reason = ButtonInteractionStopReason::EventLimit;
                break;
            }
            print_button_event(event, session_started);
            if events.len() == BUTTON_MAX_EVENTS {
                stop_reason = ButtonInteractionStopReason::EventLimit;
                break;
            }
        }
        if stop_reason == ButtonInteractionStopReason::EventLimit {
            break;
        }
        std::thread::sleep(BUTTON_SAMPLE_INTERVAL);
    }
    if stop_reason == ButtonInteractionStopReason::InactivityTimeout {
        let finished_at = Instant::now();
        for event in tracker.flush(finished_at) {
            if !try_push_button_event(&mut events, event) {
                stop_reason = ButtonInteractionStopReason::EventLimit;
                break;
            }
            print_button_event(event, session_started);
            if events.len() == BUTTON_MAX_EVENTS {
                stop_reason = ButtonInteractionStopReason::EventLimit;
                break;
            }
        }
    }
    let latest = latest.ok_or("RAM button interaction did not receive a sample")?;
    add_button_interaction_result(latest, &events, session_started, stop_reason)
}

fn try_push_button_event(events: &mut Vec<ButtonGestureEvent>, event: ButtonGestureEvent) -> bool {
    if events.len() >= BUTTON_MAX_EVENTS {
        return false;
    }
    events.push(event);
    true
}

fn exit_ram(port: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let usb_identity = validate_exact_ram_port(port)?;
    let _serial_lock = acquire_ram_port_lock(port)?;
    ensure_ram_target(port, &usb_identity)?;
    let (identity, mut serial) = match read_identity(port, &usb_identity) {
        Ok(result) => result,
        Err(_) => {
            let elf = default_ram_elf();
            validate_local_elf(&elf)?;
            let image = read_validated_ram_elf(&elf)?;
            load_ram_elf(port, image, &usb_identity)?
        }
    };
    verify_ram_identity(&identity, "exit")?;
    ensure_ram_target(port, &usb_identity)?;
    send_ram_request(&mut serial, "exit", None)
}

fn acquire_ram_port_lock(
    port: &str,
) -> Result<SerialPortProcessLock, Box<dyn std::error::Error + Send + Sync>> {
    SerialPortProcessLock::acquire(port, Instant::now() + RAM_OPERATION_LOCK_TIMEOUT)
        .map_err(|error| format!("failed to acquire RAM serial lock: {error:?}").into())
}

fn default_ram_elf() -> PathBuf {
    flux_purr_repo_root().join(RAM_ELF_RELATIVE_PATH)
}

fn default_ram_elf_for(op: &str) -> PathBuf {
    if op == PD_HIL_CAPABILITY {
        flux_purr_repo_root().join(PD_HIL_ELF_RELATIVE_PATH)
    } else {
        default_ram_elf()
    }
}

const ELF_PT_LOAD: u32 = 1;
const ELF_PF_X: u64 = 0x1;
const ELF_SHF_ALLOC: u64 = 0x2;
const ELF_SHF_EXECINSTR: u64 = 0x4;
const ELF_SHT_PROGBITS: u32 = 1;
const ELF_SHT_INIT_ARRAY: u32 = 14;
const ELF_MACHINE_XTENSA: u16 = 94;
const RAM_VECTORS: (u64, u64) = (0x4037_8000, 0x4037_8400);
const RAM_IRAM: (u64, u64) = (0x4037_8400, 0x403b_8400);
const RAM_DRAM: (u64, u64) = (0x3fc8_8000, 0x3fce_8000);
const RAM_RESERVED: (u64, u64) = (0x3fce_8000, 0x3fce_d710);
const RAM_FLASH_WINDOWS: [(u64, u64); 2] = [(0x4200_0000, 0x4400_0000), (0x3c00_0000, 0x3d00_0000)];
const RAM_BLOCK_SIZE: usize = 0x1800;
const RAM_RESPONSE_BUFFER_LIMIT: usize = 16 * 1024;
const RAM_ELF_FILE_LIMIT: u64 = 8 * 1024 * 1024;
const RTC_CNTL_BASE: u32 = 0x6000_8000;
const RTC_CNTL_SWD_CONF: u32 = RTC_CNTL_BASE + 0x00b4;
const RTC_CNTL_SWD_WPROTECT: u32 = RTC_CNTL_BASE + 0x00b8;
const RTC_CNTL_WDTCONFIG0: u32 = RTC_CNTL_BASE + 0x0098;
const RTC_CNTL_WDTWPROTECT: u32 = RTC_CNTL_BASE + 0x00b0;
const RTC_CNTL_SWD_WKEY: u32 = 0x8f1d_312a;
const RTC_CNTL_WDT_WKEY: u32 = 0x50d8_3aa1;
const RTC_CNTL_SWD_AUTO_FEED_EN: u32 = 1 << 31;

struct RamElfHeader {
    class: u8,
    entry: u64,
    phoff: u64,
    phentsize: u64,
    phnum: u64,
    shoff: u64,
    shentsize: u64,
    shnum: u64,
}

type RamSegmentFields = (u64, u64, u64, u64, u64, u64);
type RamSectionFields = (u32, u64, u64, u64, u64);
type RamLoadSection = (u32, Vec<u8>);

#[derive(Debug, Clone, Copy)]
struct RamLoadSegment {
    p_offset: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
}

struct RamElfImage {
    entry: u32,
    sections: Vec<RamLoadSection>,
}

#[derive(Default)]
struct RamLoadAccumulator {
    sections: Vec<RamLoadSection>,
    vectors_section_seen: bool,
    loaded_iram_bytes: u64,
    loaded_dram_bytes: u64,
}

#[cfg(test)]
fn validate_ram_elf(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = read_ram_elf_file(path)?;
    parse_validated_ram_elf(&data, path).map(|_| ())
}

fn read_validated_ram_elf(
    path: &Path,
) -> Result<RamElfImage, Box<dyn std::error::Error + Send + Sync>> {
    let data = read_ram_elf_file(path)?;
    parse_validated_ram_elf(&data, path)
}

fn read_ram_elf_file(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let size = fs::metadata(path)?.len();
    if size > RAM_ELF_FILE_LIMIT {
        return Err(format!(
            "RAM ELF exceeds the {RAM_ELF_FILE_LIMIT} byte file-size limit: {}",
            path.display()
        )
        .into());
    }
    Ok(fs::read(path)?)
}

fn parse_validated_ram_elf(
    data: &[u8],
    path: &Path,
) -> Result<RamElfImage, Box<dyn std::error::Error + Send + Sync>> {
    let header = parse_ram_elf_header(data, path)?;
    let expected_phentsize = if header.class == 1 { 32 } else { 56 };
    if header.phentsize < expected_phentsize {
        return Err("RAM ELF program header entry is too small".into());
    }
    let mut load_count = 0u32;
    let mut iram_bytes = 0u64;
    let mut dram_bytes = 0u64;
    let mut executable_entry = false;
    let mut segments = Vec::new();
    let mut vectors_segment_seen = false;
    for index in 0..header.phnum {
        let offset = program_header_offset(header.phoff, header.phentsize, index)?;
        let p_type = read_u32(data, offset)?;
        if p_type != ELF_PT_LOAD {
            continue;
        }
        load_count += 1;
        let (segment_iram, segment_dram, contains_executable_entry) =
            validate_ram_segment(data, header.class, index, offset, header.entry)?;
        executable_entry |= contains_executable_entry;
        iram_bytes = iram_bytes
            .checked_add(segment_iram)
            .ok_or("RAM ELF IRAM budget overflow")?;
        dram_bytes = dram_bytes
            .checked_add(segment_dram)
            .ok_or("RAM ELF DRAM budget overflow")?;
        let (p_offset, paddr, _vaddr, filesz, memsz, _flags) =
            ram_segment_fields(data, header.class, offset)?;
        if paddr == RAM_VECTORS.0 && memsz == RAM_VECTORS.1 - RAM_VECTORS.0 {
            if vectors_segment_seen {
                return Err("RAM ELF contains duplicate vectors segments".into());
            }
            vectors_segment_seen = true;
        }
        segments.push(RamLoadSegment {
            p_offset,
            paddr,
            filesz,
            memsz,
        });
    }
    if load_count == 0 {
        return Err("RAM ELF contains no PT_LOAD segments".into());
    }
    if iram_bytes > RAM_IRAM.1 - RAM_IRAM.0 {
        return Err("RAM ELF IRAM budget exceeded".into());
    }
    if dram_bytes > RAM_DRAM.1 - RAM_DRAM.0 {
        return Err("RAM ELF DRAM budget exceeded".into());
    }
    if !executable_entry {
        return Err("RAM ELF entry point is not inside an executable RAM segment".into());
    }
    let (entry, sections) = ram_load_sections(data, path, &segments)?;
    Ok(RamElfImage { entry, sections })
}

fn parse_ram_elf_header(
    data: &[u8],
    path: &Path,
) -> Result<RamElfHeader, Box<dyn std::error::Error + Send + Sync>> {
    if data.get(0..4) != Some(b"\x7fELF") {
        return Err(format!("RAM artifact is not an ELF: {}", path.display()).into());
    }
    let class = *data.get(4).ok_or_else(|| {
        format!(
            "RAM artifact has a truncated ELF header: {}",
            path.display()
        )
    })?;
    if data.get(5) != Some(&1) {
        return Err("RAM ELF must use little-endian encoding".into());
    }
    let machine = read_u16(data, 18)?;
    if machine != ELF_MACHINE_XTENSA {
        return Err(format!("RAM ELF machine {machine} is not Xtensa").into());
    }
    let values = match class {
        1 => (
            read_u32(data, 24)? as u64,
            read_u32(data, 28)? as u64,
            read_u32(data, 32)? as u64,
            read_u16(data, 42)? as u64,
            read_u16(data, 44)? as u64,
            read_u16(data, 46)? as u64,
            read_u16(data, 48)? as u64,
        ),
        2 => (
            read_u64(data, 24)?,
            read_u64(data, 32)?,
            read_u64(data, 40)?,
            read_u16(data, 54)? as u64,
            read_u16(data, 56)? as u64,
            read_u16(data, 58)? as u64,
            read_u16(data, 60)? as u64,
        ),
        _ => return Err(format!("unsupported RAM ELF class {class}").into()),
    };
    Ok(RamElfHeader {
        class,
        entry: values.0,
        phoff: values.1,
        shoff: values.2,
        phentsize: values.3,
        phnum: values.4,
        shentsize: values.5,
        shnum: values.6,
    })
}

fn program_header_offset(
    phoff: u64,
    phentsize: u64,
    index: u64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let offset = phoff
        .checked_add(
            index
                .checked_mul(phentsize)
                .ok_or("RAM ELF program header overflow")?,
        )
        .ok_or("RAM ELF program header overflow")?;
    usize::try_from(offset).map_err(|_| "RAM ELF program header is too large".into())
}

fn section_header_offset(
    shoff: u64,
    shentsize: u64,
    index: u64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let offset = shoff
        .checked_add(
            index
                .checked_mul(shentsize)
                .ok_or("RAM ELF section header overflow")?,
        )
        .ok_or("RAM ELF section header overflow")?;
    usize::try_from(offset).map_err(|_| "RAM ELF section header is too large".into())
}

fn validate_ram_segment(
    data: &[u8],
    class: u8,
    index: u64,
    offset: usize,
    entry: u64,
) -> Result<(u64, u64, bool), Box<dyn std::error::Error + Send + Sync>> {
    let (p_offset, vaddr, paddr, filesz, memsz, flags) = ram_segment_fields(data, class, offset)?;
    if filesz > memsz {
        return Err(format!("RAM ELF segment {index} has p_filesz > p_memsz").into());
    }
    if paddr != vaddr {
        return Err(format!("RAM ELF segment {index} has a non-identity load address").into());
    }
    let end = paddr
        .checked_add(memsz)
        .ok_or_else(|| format!("RAM ELF segment {index} address overflow"))?;
    let file_end = p_offset
        .checked_add(filesz)
        .ok_or_else(|| format!("RAM ELF segment {index} file range overflow"))?;
    if usize::try_from(file_end).map_or(true, |end| end > data.len()) {
        return Err(format!("RAM ELF segment {index} exceeds the artifact").into());
    }
    if RAM_FLASH_WINDOWS
        .iter()
        .any(|window| ranges_overlap(paddr, end, *window))
    {
        return Err(format!("RAM ELF segment {index} maps to flash").into());
    }
    if ranges_overlap(paddr, end, RAM_RESERVED) {
        return Err(format!("RAM ELF segment {index} overlaps reserved memory").into());
    }
    if paddr == RAM_VECTORS.0 && end == RAM_VECTORS.1 {
        return Ok((0, 0, flags & ELF_PF_X != 0 && (paddr..end).contains(&entry)));
    }
    if range_contained(paddr, end, RAM_IRAM) {
        return Ok((
            memsz,
            0,
            flags & ELF_PF_X != 0 && (paddr..end).contains(&entry),
        ));
    }
    if range_contained(paddr, end, RAM_DRAM) {
        return Ok((
            0,
            memsz,
            flags & ELF_PF_X != 0 && (paddr..end).contains(&entry),
        ));
    }
    Err(format!("RAM ELF segment {index} is outside internal RAM").into())
}

fn ram_segment_fields(
    data: &[u8],
    class: u8,
    offset: usize,
) -> Result<RamSegmentFields, Box<dyn std::error::Error + Send + Sync>> {
    Ok(if class == 1 {
        (
            read_u32(data, offset_field(offset, 4)?)? as u64,
            read_u32(data, offset_field(offset, 8)?)? as u64,
            read_u32(data, offset_field(offset, 12)?)? as u64,
            read_u32(data, offset_field(offset, 16)?)? as u64,
            read_u32(data, offset_field(offset, 20)?)? as u64,
            read_u32(data, offset_field(offset, 24)?)? as u64,
        )
    } else {
        (
            read_u64(data, offset_field(offset, 8)?)?,
            read_u64(data, offset_field(offset, 16)?)?,
            read_u64(data, offset_field(offset, 24)?)?,
            read_u64(data, offset_field(offset, 32)?)?,
            read_u64(data, offset_field(offset, 40)?)?,
            read_u32(data, offset_field(offset, 4)?)? as u64,
        )
    })
}

fn ram_section_fields(
    data: &[u8],
    class: u8,
    offset: usize,
) -> Result<RamSectionFields, Box<dyn std::error::Error + Send + Sync>> {
    Ok(if class == 1 {
        (
            read_u32(data, offset_field(offset, 4)?)?,
            read_u32(data, offset_field(offset, 8)?)? as u64,
            read_u32(data, offset_field(offset, 12)?)? as u64,
            read_u32(data, offset_field(offset, 16)?)? as u64,
            read_u32(data, offset_field(offset, 20)?)? as u64,
        )
    } else {
        (
            read_u32(data, offset_field(offset, 4)?)?,
            read_u64(data, offset_field(offset, 8)?)?,
            read_u64(data, offset_field(offset, 16)?)?,
            read_u64(data, offset_field(offset, 24)?)?,
            read_u64(data, offset_field(offset, 32)?)?,
        )
    })
}

fn validate_ram_section(
    address: u64,
    size: u64,
    index: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let end = address
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} address overflow"))?;
    if RAM_FLASH_WINDOWS
        .iter()
        .any(|window| ranges_overlap(address, end, *window))
    {
        return Err(format!("RAM ELF section {index} maps to flash").into());
    }
    if ranges_overlap(address, end, RAM_RESERVED) {
        return Err(format!("RAM ELF section {index} overlaps reserved memory").into());
    }
    if address == RAM_VECTORS.0 && end == RAM_VECTORS.1 {
        return Ok(());
    }
    if range_contained(address, end, RAM_IRAM) || range_contained(address, end, RAM_DRAM) {
        return Ok(());
    }
    Err(format!("RAM ELF section {index} is outside internal RAM").into())
}

fn ram_load_sections(
    data: &[u8],
    path: &Path,
    segments: &[RamLoadSegment],
) -> Result<(u32, Vec<RamLoadSection>), Box<dyn std::error::Error + Send + Sync>> {
    let header = parse_ram_elf_header(data, path)?;
    let expected_shentsize = if header.class == 1 { 40 } else { 64 };
    if header.shentsize < expected_shentsize {
        return Err("RAM ELF section header entry is too small".into());
    }
    let entry = u32::try_from(header.entry).map_err(|_| "RAM ELF entry does not fit")?;
    let mut entry_in_executable_section = false;
    let mut load = RamLoadAccumulator::default();
    // MemEnd enters the ELF Reset symbol. The Xtensa runtime's default
    // __zero_bss hook clears _bss_start.._bss_end before main; PT_LOAD
    // memsz also includes the linker-owned stack, which must remain intact.
    // Send only file-backed sections here so the ROM loader does not overwrite
    // the stack while the runtime still owns BSS initialization.
    for index in 0..header.shnum {
        let offset = section_header_offset(header.shoff, header.shentsize, index)?;
        let fields = ram_section_fields(data, header.class, offset)?;
        if append_ram_load_section(data, header.entry, index, fields, segments, &mut load)? {
            entry_in_executable_section = true;
        }
    }
    if segments
        .iter()
        .any(|segment| segment.paddr == RAM_VECTORS.0)
        && !load.vectors_section_seen
    {
        return Err("RAM ELF vectors segment has no complete vectors section".into());
    }
    if load.sections.is_empty() {
        return Err("RAM ELF contains no loadable sections".into());
    }
    if !entry_in_executable_section {
        return Err(
            "RAM ELF entry point is not inside a file-backed executable RAM section".into(),
        );
    }
    Ok((entry, load.sections))
}

fn append_ram_load_section(
    data: &[u8],
    entry: u64,
    index: u64,
    (section_type, flags, address, data_offset, size): RamSectionFields,
    segments: &[RamLoadSegment],
    load: &mut RamLoadAccumulator,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if !matches!(section_type, ELF_SHT_PROGBITS | ELF_SHT_INIT_ARRAY)
        || flags & ELF_SHF_ALLOC == 0
        || address == 0
        || data_offset == 0
        || size == 0
    {
        return Ok(false);
    }
    validate_ram_section(address, size, index)?;
    let section_end = address
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} address overflow"))?;
    let data_end = data_offset
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} file range overflow"))?;
    let segment = segments
        .iter()
        .find(|segment| {
            let Some(segment_end) = segment.paddr.checked_add(segment.memsz) else {
                return false;
            };
            let Some(segment_file_end) = segment.p_offset.checked_add(segment.filesz) else {
                return false;
            };
            range_contained(address, section_end, (segment.paddr, segment_end))
                && range_contained(data_offset, data_end, (segment.p_offset, segment_file_end))
        })
        .ok_or_else(|| format!("RAM ELF section {index} is not backed by a PT_LOAD segment"))?;
    if address == RAM_VECTORS.0 && section_end == RAM_VECTORS.1 {
        if load.vectors_section_seen
            || segment.paddr != RAM_VECTORS.0
            || segment.memsz != RAM_VECTORS.1 - RAM_VECTORS.0
            || segment.filesz != RAM_VECTORS.1 - RAM_VECTORS.0
            || data_offset != segment.p_offset
            || size != segment.filesz
        {
            return Err(format!(
                "RAM ELF section {index} is not the unique complete vectors section"
            )
            .into());
        }
        load.vectors_section_seen = true;
    } else {
        record_ram_load_budget(
            address,
            size,
            &mut load.loaded_iram_bytes,
            &mut load.loaded_dram_bytes,
        )?;
    }
    let start = usize::try_from(data_offset)
        .map_err(|_| format!("RAM ELF section {index} offset is too large"))?;
    let end = usize::try_from(data_end)
        .map_err(|_| format!("RAM ELF section {index} end is too large"))?;
    let segment_data = data
        .get(start..end)
        .ok_or_else(|| format!("RAM ELF section {index} exceeds the artifact"))?;
    let address_u32 = u32::try_from(address)
        .map_err(|_| format!("RAM ELF section {index} address is too large"))?;
    load.sections.push((address_u32, segment_data.to_vec()));
    Ok(flags & ELF_SHF_EXECINSTR != 0
        && (address..section_end).contains(&entry)
        && (address == RAM_VECTORS.0 && section_end == RAM_VECTORS.1
            || range_contained(address, section_end, RAM_IRAM)))
}

fn record_ram_load_budget(
    address: u64,
    size: u64,
    loaded_iram_bytes: &mut u64,
    loaded_dram_bytes: &mut u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let end = address
        .checked_add(size)
        .ok_or("RAM ELF load range overflow")?;
    if range_contained(address, end, RAM_IRAM) {
        *loaded_iram_bytes = loaded_iram_bytes
            .checked_add(size)
            .ok_or("RAM ELF IRAM load budget overflow")?;
        if *loaded_iram_bytes > RAM_IRAM.1 - RAM_IRAM.0 {
            return Err("RAM ELF IRAM load budget exceeded".into());
        }
    } else if range_contained(address, end, RAM_DRAM) {
        *loaded_dram_bytes = loaded_dram_bytes
            .checked_add(size)
            .ok_or("RAM ELF DRAM load budget overflow")?;
        if *loaded_dram_bytes > RAM_DRAM.1 - RAM_DRAM.0 {
            return Err("RAM ELF DRAM load budget exceeded".into());
        }
    } else {
        return Err("RAM ELF load range is outside internal RAM".into());
    }
    Ok(())
}

fn offset_field(
    offset: usize,
    field: usize,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    offset
        .checked_add(field)
        .ok_or_else(|| "RAM ELF program header field overflow".into())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(2).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(4).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(8).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn ranges_overlap(start: u64, end: u64, window: (u64, u64)) -> bool {
    start < window.1 && end > window.0
}

fn range_contained(start: u64, end: u64, window: (u64, u64)) -> bool {
    window.0 <= start && end <= window.1
}

fn validate_exact_ram_port(
    port: &str,
) -> Result<UsbSerialIdentity, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(port)?;
    #[cfg(not(target_os = "windows"))]
    let path = Path::new(port);
    #[cfg(not(target_os = "windows"))]
    if !path.exists() {
        return Err(format!("authorized serial port is unavailable: {port}").into());
    }
    let enumerated = serialport::available_ports()?
        .into_iter()
        .find(|candidate| serial_port_paths_match(port, &candidate.port_name))
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    let identity = UsbSerialIdentity::from_port_info(&enumerated)
        .ok_or_else(|| format!("authorized serial port has no stable USB identity: {port}"))?;
    if identity.vid != ESP32S3_USB_SERIAL_JTAG_VID || identity.pid != ESP32S3_USB_SERIAL_JTAG_PID {
        return Err(format!(
            "authorized serial port is not an ESP32-S3 USB Serial/JTAG target: {port}"
        )
        .into());
    }
    Ok(identity)
}

fn ensure_ram_target(
    port: &str,
    expected: &UsbSerialIdentity,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if serial_port_usb_identity_matches(port, expected) {
        return Ok(());
    }
    Err(format!("authorized USB target changed or is no longer enumerated: {port}").into())
}

fn read_identity(
    port: &str,
    expected: &UsbSerialIdentity,
) -> Result<(ObservedIdentity, Port), Box<dyn std::error::Error + Send + Sync>> {
    ensure_ram_target(port, expected)?;
    let mut serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(200))
        .open_native()?;
    let identity = read_identity_from_serial(&mut serial)?;
    Ok((identity, serial))
}

fn read_identity_from_serial(
    serial: &mut dyn SerialPort,
) -> Result<ObservedIdentity, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = Instant::now() + IDENTITY_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match serial.read(&mut chunk) {
            Ok(count) => {
                if bytes.len().saturating_add(count) > RAM_RESPONSE_BUFFER_LIMIT {
                    return Err("RAM bring-up response exceeded the JSONL frame limit".into());
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(identity) = next_identity(&mut bytes) {
                    return Ok(identity);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    Err(format!("no identity frame received; serial output preview: {preview:?}").into())
}

fn next_identity(bytes: &mut Vec<u8>) -> Option<ObservedIdentity> {
    while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = bytes.drain(..=index).collect();
        if let Some(identity) = parse_identity_line(&line) {
            return Some(identity);
        }
    }
    None
}

fn parse_identity_line(line: &[u8]) -> Option<ObservedIdentity> {
    let frame: IdentityFrame = serde_json::from_slice(line).ok()?;
    if frame.frame_type.as_deref() != Some("hello") {
        return None;
    }
    let body = frame.identity?;
    let kind = frame
        .firmware_kind
        .as_deref()
        .or(body.firmware_kind.as_deref());
    let firmware = match kind {
        Some("product") => ObservedFirmware::Product,
        Some("ram_bringup") => ObservedFirmware::RamBringup,
        _ => ObservedFirmware::Unknown,
    };
    Some(ObservedIdentity {
        firmware,
        build_id: body.build_id,
        source_sha: body.git_sha,
        capabilities: body.capabilities,
        protocol_version: frame.protocol_version.or(body.protocol_version),
        framing: frame.framing.or(body.framing),
        reset_reason: body.reset_reason,
    })
}

fn verify_ram_identity(
    identity: &ObservedIdentity,
    op: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if identity.firmware != ObservedFirmware::RamBringup {
        return Err(
            "RAM bring-up identity is unknown or the target is still product firmware".into(),
        );
    }
    if identity.protocol_version.as_deref() != Some(RAM_PROTOCOL_VERSION)
        || identity.framing.as_deref() != Some(RAM_FRAMING)
    {
        return Err(format!(
            "RAM bring-up protocol mismatch: expected {RAM_PROTOCOL_VERSION}/{RAM_FRAMING}"
        )
        .into());
    }
    if identity.build_id.as_deref() != Some(PRODUCT_BUILD_ID) {
        return Err(format!(
            "RAM bring-up buildId mismatch: expected {PRODUCT_BUILD_ID}, got {:?}",
            identity.build_id
        )
        .into());
    }
    if !identity
        .capabilities
        .iter()
        .any(|capability| capability == op)
    {
        return Err(format!("RAM bring-up does not advertise capability {op}").into());
    }
    Ok(())
}

fn send_ram_request(
    serial: &mut dyn SerialPort,
    op: &str,
    color: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    send_ram_request_until(serial, op, color, Instant::now() + COMMAND_TIMEOUT)
}

fn send_ram_request_until(
    serial: &mut dyn SerialPort,
    op: &str,
    color: Option<&str>,
    deadline: Instant,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-{}", current_unix_millis());
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(format!("RAM bring-up response timed out for {op}").into());
    }
    let request = build_ram_request(&request_id, op, color);
    serial.write_all(request.as_bytes())?;
    serial.flush()?;
    let mut bytes = Vec::with_capacity(1024);
    let mut fan_power_seen = false;
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        serial.set_timeout(remaining.min(Duration::from_millis(250)))?;
        match serial.read(&mut chunk) {
            Ok(count) => {
                append_ram_response_bytes(&mut bytes, &chunk[..count])?;
                if let Some(value) =
                    find_ram_response(&mut bytes, op, &request_id, &mut fan_power_seen)?
                {
                    return Ok(value);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(format!("RAM bring-up response timed out for {op}").into())
}

struct PdHilEvidence {
    events_path: PathBuf,
    summary_path: PathBuf,
    transcript_path: PathBuf,
    events: BufWriter<File>,
    transcript: BufWriter<File>,
    events_since_flush: usize,
}

impl PdHilEvidence {
    fn create(
        directory: &Path,
        port: &str,
        identity: &UsbSerialIdentity,
        ram_identity: &ObservedIdentity,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(directory)?;
        let events_path = directory.join("events.ndjson");
        let summary_path = directory.join("summary.json");
        let transcript_path = directory.join("transcript.log");
        let mut evidence = Self {
            events_path,
            summary_path,
            transcript_path,
            events: BufWriter::new(File::create(directory.join("events.ndjson"))?),
            transcript: BufWriter::new(File::create(directory.join("transcript.log"))?),
            events_since_flush: 0,
        };
        evidence.record(&serde_json::json!({
            "kind": "metadata",
            "schemaVersion": 1,
            "port": port,
            "usbIdentity": {
                "vid": identity.vid,
                "pid": identity.pid,
                "serialNumber": identity.serial_number,
            },
            "buildId": PRODUCT_BUILD_ID,
            "protocolVersion": RAM_PROTOCOL_VERSION,
            "framing": RAM_FRAMING,
            "ramIdentity": {
                "resetReason": ram_identity.reset_reason,
                "sourceSha": ram_identity.source_sha,
            },
            "createdAtUnixMs": current_unix_millis(),
        }))?;
        Ok(evidence)
    }

    fn record(&mut self, value: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        serde_json::to_writer(&mut self.events, value)?;
        self.events.write_all(b"\n")?;
        self.events_since_flush += 1;
        if self.events_since_flush >= PD_HIL_EVIDENCE_FLUSH_INTERVAL {
            self.events.flush()?;
            self.events_since_flush = 0;
        }
        Ok(())
    }

    fn record_transcript(
        &mut self,
        line: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.transcript.write_all(line.as_bytes())?;
        self.transcript.write_all(b"\n")?;
        self.transcript.flush()?;
        Ok(())
    }

    fn finish(
        mut self,
        summary: &Value,
    ) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error + Send + Sync>> {
        self.record(summary)?;
        self.events.flush()?;
        self.transcript.flush()?;
        let mut file = BufWriter::new(File::create(&self.summary_path)?);
        serde_json::to_writer_pretty(&mut file, summary)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok((self.events_path, self.summary_path))
    }
}

fn finish_pd_hil_evidence(
    evidence: PdHilEvidence,
    evidence_dir: &Path,
    mut summary: Value,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut evidence = evidence;
    let result = summary.get("result").and_then(Value::as_object);
    let transcript_line = if summary.get("type").and_then(Value::as_str) == Some("pd_hil_failure") {
        format!(
            "PD HIL FAILURE reason={}",
            result
                .and_then(|result| result.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )
    } else {
        let tier_count = result
            .and_then(|result| result.get("tiers"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let pass_count = result
            .and_then(|result| result.get("tiers"))
            .and_then(Value::as_array)
            .map_or(0, |tiers| {
                tiers
                    .iter()
                    .filter(|tier| tier.get("status").and_then(Value::as_str) == Some("pass"))
                    .count()
            });
        format!(
            "PD HIL SUMMARY overall={} pass={} tiers={} reset={} pd={}",
            result
                .and_then(|result| result.get("overall"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_ascii_uppercase(),
            pass_count,
            tier_count,
            result
                .and_then(|result| result.get("finalReset"))
                .and_then(Value::as_object)
                .and_then(|reset| reset.get("status"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_ascii_uppercase(),
            result
                .and_then(|result| result.get("pd"))
                .and_then(Value::as_str)
                .unwrap_or("-"),
        )
    };
    evidence.record_transcript(&transcript_line)?;
    let transcript_path = evidence.transcript_path.clone();
    let (events_path, summary_path) = evidence.finish(&summary)?;
    let object = summary
        .as_object_mut()
        .ok_or("PD HIL evidence summary must be a JSON object")?;
    object.insert(
        "evidenceDir".to_string(),
        Value::String(evidence_dir.display().to_string()),
    );
    object.insert(
        "eventsPath".to_string(),
        Value::String(events_path.display().to_string()),
    );
    object.insert(
        "summaryPath".to_string(),
        Value::String(summary_path.display().to_string()),
    );
    object.insert(
        "transcriptPath".to_string(),
        Value::String(transcript_path.display().to_string()),
    );
    let mut summary_file = BufWriter::new(File::create(&summary_path)?);
    serde_json::to_writer_pretty(&mut summary_file, &summary)?;
    summary_file.write_all(b"\n")?;
    summary_file.flush()?;
    Ok(summary)
}

fn pd_hil_failure_summary(request_id: &str, reason: &str) -> Value {
    serde_json::json!({
        "type": "pd_hil_failure",
        "requestId": request_id,
        "firmwareKind": "ram_bringup",
        "capability": PD_HIL_CAPABILITY,
        "ok": false,
        "result": {
            "detail": "pd_hil_host_failure",
            "heater": "off",
            "pd": "unknown",
            "eeprom": "untouched",
            "overall": "host_failure",
            "reason": reason,
        },
    })
}

#[derive(Default)]
struct PdHilSummaryChunks {
    expected_count: Option<usize>,
    chunks: Vec<Option<Vec<u8>>>,
}

impl PdHilSummaryChunks {
    fn accept(
        &mut self,
        result: &Value,
    ) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
        if result.get("kind").and_then(Value::as_str) != Some("summary_chunk") {
            return Ok(None);
        }
        let count = result
            .get("count")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("PD HIL summary chunk is missing count")?;
        let index = result
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("PD HIL summary chunk is missing index")?;
        if count == 0 || count > 1024 || index >= count {
            return Err("PD HIL summary chunk has an invalid index or count".into());
        }
        if result.get("encoding").and_then(Value::as_str) != Some("hex") {
            return Err("PD HIL summary chunk has an unsupported encoding".into());
        }
        let encoded = result
            .get("data")
            .and_then(Value::as_str)
            .ok_or("PD HIL summary chunk is missing data")?;
        let decoded = decode_summary_chunk(encoded)?;
        if self.expected_count.is_none() {
            self.expected_count = Some(count);
            self.chunks.resize_with(count, || None);
        }
        if self.expected_count != Some(count) {
            return Err("PD HIL summary chunk count changed during reassembly".into());
        }
        if let Some(existing) = &self.chunks[index] {
            if existing != &decoded {
                return Err("PD HIL summary chunk was received with conflicting data".into());
            }
        } else {
            self.chunks[index] = Some(decoded);
        }
        if !self.chunks.iter().all(Option::is_some) {
            return Ok(None);
        }
        let total = self
            .chunks
            .iter()
            .filter_map(Option::as_ref)
            .map(Vec::len)
            .sum::<usize>();
        if total > RAM_RESPONSE_BUFFER_LIMIT {
            return Err("PD HIL summary chunks exceeded the JSONL frame limit".into());
        }
        let mut frame = Vec::with_capacity(total);
        for chunk in &self.chunks {
            frame.extend_from_slice(chunk.as_ref().expect("all summary chunks are present"));
        }
        Ok(Some(serde_json::from_slice(&frame)?))
    }
}

fn decode_summary_chunk(
    encoded: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    if !encoded.len().is_multiple_of(2) {
        return Err("PD HIL summary chunk has odd-length hex data".into());
    }
    let mut decoded = Vec::with_capacity(encoded.len() / 2);
    for pair in encoded.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or("PD HIL summary chunk has invalid hex data")?;
        let low = hex_nibble(pair[1]).ok_or("PD HIL summary chunk has invalid hex data")?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn send_pd_hil_request(
    port: &str,
    session: RamSession,
    evidence_dir: &Path,
    validate_vin: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let RamSession {
        _serial_lock,
        usb_identity,
        identity,
        serial,
    } = session;
    let mut serial = Some(serial);
    let mut evidence = PdHilEvidence::create(evidence_dir, port, &usb_identity, &identity)?;
    let deadline = Instant::now() + PD_HIL_COMMAND_TIMEOUT;
    let request_id = format!("pd-hil-{}", current_unix_millis());
    let request = build_pd_hil_request(&request_id, validate_vin);
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    let mut summary_chunks = PdHilSummaryChunks::default();
    let mut recovered_serial: Option<ReconnectedPdHilSerial> = None;
    let outcome: Result<Value, Box<dyn std::error::Error + Send + Sync>> = (|| {
        evidence.record(&serde_json::json!({
            "kind": "request",
            "requestId": request_id,
            "capability": PD_HIL_CAPABILITY,
            "vinValidation": if validate_vin { "adc" } else { "external_source" },
            "wire": request.trim_end(),
        }))?;
        let serial_port = serial.as_mut().expect("PD HIL serial must be present");
        serial_port.write_all(request.as_bytes())?;
        serial_port.flush()?;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let read_result = if let Some(recovered_serial) = recovered_serial.as_mut() {
                read_reconnected_pd_hil_serial(recovered_serial, &mut chunk, remaining)
            } else {
                ensure_ram_target(port, &usb_identity)?;
                let serial_port = serial
                    .as_mut()
                    .ok_or("PD HIL serial was closed before recovery")?;
                serial_port.set_timeout(remaining.min(Duration::from_millis(250)))?;
                serial_port.read(&mut chunk)
            };
            match read_result {
                Ok(count) => {
                    append_ram_response_bytes(&mut bytes, &chunk[..count])?;
                    if let Some(value) = find_pd_hil_response(
                        &mut bytes,
                        &request_id,
                        &mut evidence,
                        &mut summary_chunks,
                    )? {
                        validate_ram_success_response(&value, PD_HIL_CAPABILITY)
                            .map_err(|error| format!("PD HIL summary is invalid: {error}"))?;
                        validate_pd_hil_identity(&value, &identity)?;
                        return Ok(value);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) if is_recoverable_pd_hil_serial_error(&error) => {
                    let original_error = error.to_string();
                    evidence.record(&serde_json::json!({
                        "kind": "serial_error",
                        "error": original_error.as_str(),
                        "errorKind": format!("{:?}", error.kind()),
                        "port": port,
                    }))?;
                    drop(serial.take());
                    let (serial, attempt) = reconnect_pd_hil_serial(
                        port,
                        &usb_identity,
                        deadline,
                    )
                    .map_err(|error| {
                        format!(
                            "PD HIL serial connection could not be recovered after {original_error}: {error}"
                        )
                    })?;
                    recovered_serial = Some(serial);
                    evidence.record(&serde_json::json!({
                        "kind": "serial_reconnect",
                        "attempt": attempt,
                        "status": "reconnected",
                        "port": port,
                    }))?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err("PD HIL response timed out; no terminal summary was received".into())
    })();
    match outcome {
        Ok(value) => finish_pd_hil_evidence(evidence, evidence_dir, value),
        Err(error) => {
            let message = error.to_string();
            let _ = finish_pd_hil_evidence(
                evidence,
                evidence_dir,
                pd_hil_failure_summary(&request_id, &message),
            );
            Err(message.into())
        }
    }
}

fn reopen_pd_hil_serial(
    port: &str,
    expected: &UsbSerialIdentity,
) -> Result<ReconnectedPdHilSerial, Box<dyn std::error::Error + Send + Sync>> {
    ensure_ram_target(port, expected)?;
    #[cfg(target_os = "macos")]
    let serial = open_raw_usb_serial_jtag_port(port)?;
    #[cfg(not(target_os = "macos"))]
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(250))
        .open_native()?;
    ensure_ram_target(port, expected)?;
    Ok(serial)
}

fn reconnect_pd_hil_serial(
    port: &str,
    expected: &UsbSerialIdentity,
    deadline: Instant,
) -> Result<(ReconnectedPdHilSerial, u16), Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = String::from("authorized port is unavailable");
    for attempt in 1..=PD_HIL_SERIAL_RECONNECT_ATTEMPTS {
        if Instant::now() >= deadline {
            break;
        }
        match reopen_pd_hil_serial(port, expected) {
            Ok(serial) => return Ok((serial, attempt)),
            Err(error) => {
                last_error = error.to_string();
                std::thread::sleep(PD_HIL_SERIAL_RECONNECT_DELAY);
            }
        }
    }
    Err(last_error.into())
}

#[cfg(target_os = "macos")]
fn read_reconnected_pd_hil_serial(
    serial: &mut ReconnectedPdHilSerial,
    chunk: &mut [u8],
    _remaining: Duration,
) -> std::io::Result<usize> {
    match serial.read(chunk) {
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            std::thread::sleep(Duration::from_millis(5));
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "USB Serial/JTAG read would block",
            ))
        }
        result => result,
    }
}

#[cfg(not(target_os = "macos"))]
fn read_reconnected_pd_hil_serial(
    serial: &mut ReconnectedPdHilSerial,
    chunk: &mut [u8],
    remaining: Duration,
) -> std::io::Result<usize> {
    serial.set_timeout(remaining.min(Duration::from_millis(250)))?;
    serial.read(chunk)
}

fn is_recoverable_pd_hil_serial_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::UnexpectedEof
    ) || error.to_string().contains("Device not configured")
}

fn find_pd_hil_response(
    bytes: &mut Vec<u8>,
    request_id: &str,
    evidence: &mut PdHilEvidence,
    summary_chunks: &mut PdHilSummaryChunks,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = bytes.drain(..=index).collect();
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            let text = String::from_utf8_lossy(&line);
            evidence.record(&serde_json::json!({
                "kind": "serial_text",
                "text": text.trim_end_matches(['\r', '\n']),
            }))?;
            continue;
        };
        evidence.record(&value)?;
        let matches_request = value.get("requestId").and_then(Value::as_str) == Some(request_id);
        let capability = value.get("capability").and_then(Value::as_str);
        if value.get("type").and_then(Value::as_str) == Some("progress")
            && matches_request
            && capability == Some(PD_HIL_CAPABILITY)
        {
            if let Some(line) = print_pd_hil_progress(&value) {
                evidence.record_transcript(&line)?;
            }
            if let Some(result) = value.get("result")
                && let Some(summary) = summary_chunks.accept(result)?
            {
                return Ok(Some(summary));
            }
            continue;
        }
        if (value.get("type").and_then(Value::as_str) == Some("pd_hil_summary")
            || value.get("type").and_then(Value::as_str) == Some("response"))
            && matches_request
            && capability == Some(PD_HIL_CAPABILITY)
        {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn append_ram_response_bytes(
    bytes: &mut Vec<u8>,
    chunk: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bytes.len().saturating_add(chunk.len()) > RAM_RESPONSE_BUFFER_LIMIT {
        return Err("RAM bring-up response exceeded the JSONL frame limit".into());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn build_ram_request(request_id: &str, op: &str, color: Option<&str>) -> String {
    match color {
        Some(color) => format!(
            "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{op}\",\"capability\":\"{op}\",\"color\":\"{color}\"}}\n"
        ),
        None => format!(
            "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{op}\",\"capability\":\"{op}\"}}\n"
        ),
    }
}

fn build_pd_hil_request(request_id: &str, validate_vin: bool) -> String {
    format!(
        "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{PD_HIL_CAPABILITY}\",\"capability\":\"{PD_HIL_CAPABILITY}\",\"validateVin\":{validate_vin}}}\n"
    )
}

fn find_ram_response(
    bytes: &mut Vec<u8>,
    op: &str,
    request_id: &str,
    fan_power_seen: &mut bool,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = bytes.drain(..=index).collect();
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("progress")
            && value.get("firmwareKind").and_then(Value::as_str) == Some("ram_bringup")
            && value.get("capability").and_then(Value::as_str) == Some(op)
            && value.get("requestId").and_then(Value::as_str) == Some(request_id)
        {
            print_ram_progress(&value);
            if value
                .get("result")
                .and_then(Value::as_object)
                .and_then(|result| result.get("kind"))
                .and_then(Value::as_str)
                == Some("power")
            {
                *fan_power_seen = true;
            }
            continue;
        }
        if op == "test_fan" && value.get("type").and_then(Value::as_str) == Some("fp") {
            print_ram_compact_fan_power(&value);
            *fan_power_seen = true;
            continue;
        }
        if op == "test_fan"
            && *fan_power_seen
            && value.get("type").and_then(Value::as_str) == Some("fs")
        {
            print_ram_fan_stage_progress(&value);
            continue;
        }
        if value.get("type").and_then(Value::as_str) != Some("response")
            || value.get("firmwareKind").and_then(Value::as_str) != Some("ram_bringup")
            || value.get("capability").and_then(Value::as_str) != Some(op)
            || value.get("requestId").and_then(Value::as_str) != Some(request_id)
        {
            continue;
        }
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            validate_ram_success_response(&value, op)
                .map_err(|error| format!("RAM bring-up response for {op} is invalid: {error}"))?;
            return Ok(Some(value));
        }
        return Err(format!("RAM bring-up rejected {op}: {value}").into());
    }
    Ok(None)
}

fn print_ram_progress(value: &Value) {
    let Some(result) = value.get("result").and_then(Value::as_object) else {
        return;
    };
    match result.get("kind").and_then(Value::as_str) {
        Some("power") => {
            let measured = result
                .get("measured")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let voltage_ok = result
                .get("voltageOk")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let input_mv = result.get("inputMv").and_then(Value::as_u64).unwrap_or(0);
            let minimum_mv = result
                .get("minimumMv")
                .and_then(Value::as_u64)
                .unwrap_or(FAN_MIN_INPUT_MV);
            if measured && voltage_ok {
                eprintln!(
                    "RAM FAN POWER: measured={}mV minimum={}mV status=ok",
                    input_mv, minimum_mv
                );
            } else if measured {
                eprintln!(
                    "RAM FAN POWER WARNING: measured={}mV minimum={}mV; fan may not start",
                    input_mv, minimum_mv
                );
            } else {
                eprintln!(
                    "RAM FAN POWER WARNING: VIN measurement unavailable; minimum={}mV was not verified",
                    minimum_mv
                );
            }
        }
        Some("pwm_stage") => {
            let duty_percent = result
                .get("dutyPercent")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let duration_ms = result
                .get("durationMs")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            eprintln!(
                "RAM FAN PWM STAGE: duty={}%; duration={}s; started",
                duty_percent,
                duration_ms / 1_000
            );
        }
        _ => {}
    }
}

fn print_pd_hil_progress(value: &Value) -> Option<String> {
    let Some(result) = value.get("result").and_then(Value::as_object) else {
        return None;
    };
    let line = match result.get("kind").and_then(Value::as_str) {
        Some("session") => Some(format!(
            "PD HIL SESSION stage={}",
            result
                .get("stage")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )),
        Some("capabilities") => Some(format!(
            "PD HIL CAPABILITIES count={}",
            result
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        )),
        Some("tier") => {
            let status = result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if status == "requesting"
                || status == "pass"
                || status == "unsupported"
                || status == "negotiation_failed"
                || status == "measurement_failed"
                || status == "cancelled"
                || status == "global_timeout"
            {
                let status_label = status.to_ascii_uppercase();
                let index = progress_number(result, "index");
                let total = progress_number(result, "total");
                let contract_mv = progress_number(result, "contractMv");
                let contract_current_ma = progress_number(result, "contractCurrentMa");
                let measured_vin_mv = progress_number(result, "measuredVinMv");
                Some(format!(
                    "PD HIL TIER {}/{} mode={} target={}mV status={} contract={}mV current={}mA vin={}mV",
                    index,
                    total,
                    result.get("mode").and_then(Value::as_str).unwrap_or("-"),
                    result
                        .get("targetMv")
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                    status_label,
                    contract_mv,
                    contract_current_ma,
                    measured_vin_mv,
                ))
            } else {
                None
            }
        }
        Some("recovery") => Some(format!(
            "PD HIL RESET tier={} status={} pd={} vbus={}mV reason={}",
            result
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            result
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_ascii_uppercase(),
            result.get("pd").and_then(Value::as_str).unwrap_or("-"),
            result
                .get("defaultVbusMv")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            result.get("reason").and_then(Value::as_str).unwrap_or("-"),
        )),
        _ => None,
    };
    if let Some(line) = line.as_deref() {
        eprintln!("{line}");
    }
    let mut stderr = std::io::stderr();
    let _ = std::io::Write::flush(&mut stderr);
    line
}

fn progress_number(result: &serde_json::Map<String, Value>, key: &str) -> String {
    result
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn print_ram_fan_stage_progress(value: &Value) {
    let duty_percent = value.get("d").and_then(Value::as_u64).unwrap_or_default();
    eprintln!(
        "RAM FAN PWM STAGE: duty={}%; duration={}s; started",
        duty_percent,
        FAN_PWM_STAGE_DURATION_MS / 1_000
    );
}

fn print_ram_compact_fan_power(value: &Value) {
    let measured = value.get("m").and_then(Value::as_bool).unwrap_or(false);
    let voltage_ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let input_mv = value.get("v").and_then(Value::as_u64).unwrap_or(0);
    if measured && voltage_ok {
        eprintln!(
            "RAM FAN POWER: measured={}mV minimum={}mV status=ok",
            input_mv, FAN_MIN_INPUT_MV
        );
    } else if measured {
        eprintln!(
            "RAM FAN POWER WARNING: measured={}mV minimum={}mV; fan may not start",
            input_mv, FAN_MIN_INPUT_MV
        );
    } else {
        eprintln!(
            "RAM FAN POWER WARNING: VIN measurement unavailable; minimum={}mV was not verified",
            FAN_MIN_INPUT_MV
        );
    }
}

pub(crate) fn validate_ram_success_response(value: &Value, op: &str) -> Result<(), String> {
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err("ok must be true".to_string());
    }
    let result = value
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| "result must be an object".to_string())?;
    require_result_string(result, "detail")?;
    require_result_string_value(result, "heater", "off")?;
    require_result_string_value(result, "eeprom", "untouched")?;
    if op == PD_HIL_CAPABILITY {
        validate_pd_hil_summary(value, result)?;
    } else {
        require_result_string_value(result, "pd", "untouched")?;
    }

    match op {
        "preview_display" | "preview_frontpanel" => {
            require_result_string_value(result, "detail", "display_preview_ready")?;
            require_result_string_value(result, "effect", "display_preview")?;
        }
        "preview_status_light" => {
            require_result_string_value(result, "detail", "status_light_pwm_breath_ready")?;
            require_result_string_value(result, "effect", "pwm_breathing_rainbow_8s")?;
        }
        "test_buttons" => validate_button_result(result)?,
        "test_adc" => {
            require_result_string_value(result, "detail", "adc_read_only_ready")?;
            let adc = result
                .get("adc")
                .and_then(Value::as_object)
                .ok_or_else(|| "result.adc must be an object".to_string())?;
            for name in ["vin", "rtd"] {
                require_u16(adc, name)?;
            }
        }
        "test_i2c" => {
            require_result_string_value(result, "detail", "i2c_identification_read_only_ready")?;
            let i2c = result
                .get("i2c")
                .and_then(Value::as_object)
                .ok_or_else(|| "result.i2c must be an object".to_string())?;
            if require_u8(i2c, "address")? != 0x22 {
                return Err("result.i2c.address is not the allowlisted address".to_string());
            }
            if require_u8(i2c, "register")? != 0x09 {
                return Err("result.i2c.register is not the allowlisted register".to_string());
            }
            require_u8(i2c, "value")?;
        }
        "test_rgb" => {
            require_result_string_value(result, "detail", "rgb_ready")?;
            require_result_string_value(result, "effect", "rgb_red_green_blue_1s_x5")?;
        }
        "test_buzzer" => {
            require_result_string_value(result, "detail", "buzzer_ready")?;
            require_result_string_value(result, "effect", "1khz_1s_silence_1s_2khz_1s")?;
        }
        "test_fan" => {
            require_result_string_value(result, "detail", "fan_ready")?;
            require_result_string_value(
                result,
                "effect",
                "fan_50_percent_5s_100_percent_5s_0_percent_5s",
            )?;
            let power = result
                .get("power")
                .and_then(Value::as_object)
                .ok_or_else(|| "result.power must be an object".to_string())?;
            require_u16(power, "vinRaw")?;
            require_u16(power, "vinAdcMv")?;
            require_u32(power, "inputMv")?;
            if require_u16(power, "minimumMv")? as u64 != FAN_MIN_INPUT_MV {
                return Err(format!("result.power.minimumMv must be {FAN_MIN_INPUT_MV}"));
            }
            if power.get("measured").and_then(Value::as_bool).is_none() {
                return Err("result.power.measured must be a boolean".to_string());
            }
            if power.get("voltageOk").and_then(Value::as_bool).is_none() {
                return Err("result.power.voltageOk must be a boolean".to_string());
            }
        }
        PD_HIL_CAPABILITY => {}
        "exit" => {
            require_result_string_value(result, "detail", "safe_exit")?;
        }
        _ => return Err(format!("unsupported RAM capability: {op}")),
    }
    Ok(())
}

fn validate_pd_hil_summary(
    value: &Value,
    result: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    let pd = require_result_string(result, "pd")?;
    if !matches!(pd, "default_verified" | "resetting" | "owned") {
        return Err(format!(
            "result.pd has an invalid PD ownership state: {pd:?}"
        ));
    }
    let overall = validate_pd_hil_overall(result)?;
    let external_vin = validate_pd_hil_policy(result)?;
    let source_capabilities = result
        .get("sourceCapabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| "result.sourceCapabilities must be an object".to_string())?;
    let capability_count = source_capabilities
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| "result.sourceCapabilities.count is missing".to_string())?;
    let raw_pdos = source_capabilities
        .get("rawPdos")
        .and_then(Value::as_array)
        .ok_or_else(|| "result.sourceCapabilities.rawPdos must be an array".to_string())?;
    if capability_count == 0 || raw_pdos.len() != capability_count as usize {
        return Err("result.sourceCapabilities is incomplete".to_string());
    }
    let (tier_count, pass_count, unsupported_count) = validate_pd_hil_tiers(result, external_vin)?;
    let (reset_status, final_reset) = validate_pd_hil_final_reset(result)?;
    let complete_matrix =
        tier_count == 22 && pass_count == 22 && reset_status == "pass" && pd == "default_verified";
    if overall == "pass"
        && (external_vin
            || !complete_matrix
            || final_reset
                .get("defaultVbusMv")
                .and_then(Value::as_u64)
                .is_none_or(|value| !(4_750..=5_250).contains(&value)))
    {
        return Err(
            "result.overall=pass lacks complete tier or default-state evidence".to_string(),
        );
    }
    if overall == "external_source_pass" && (!external_vin || !complete_matrix) {
        return Err(
            "result.overall=external_source_pass lacks complete protocol evidence".to_string(),
        );
    }
    if overall == "unsupported"
        && (unsupported_count == 0
            || tier_count != 22
            || reset_status != "pass"
            || pd != "default_verified")
    {
        return Err(
            "result.overall=unsupported lacks the complete matrix and final recovery evidence"
                .to_string(),
        );
    }
    if value.get("requestId").and_then(Value::as_str).is_none() {
        return Err("PD HIL summary requestId is missing".to_string());
    }
    Ok(())
}

fn validate_pd_hil_identity(value: &Value, identity: &ObservedIdentity) -> Result<(), String> {
    let result = value
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| "PD HIL summary result is missing".to_string())?;
    let expected_build_id = identity
        .build_id
        .as_deref()
        .ok_or_else(|| "RAM identity buildId is missing".to_string())?;
    let expected_source_sha = identity
        .source_sha
        .as_deref()
        .ok_or_else(|| "RAM identity gitSha is missing".to_string())?;
    if result.get("buildId").and_then(Value::as_str) != Some(expected_build_id) {
        return Err("PD HIL summary buildId does not match the RAM identity".to_string());
    }
    if result.get("sourceSha").and_then(Value::as_str) != Some(expected_source_sha) {
        return Err("PD HIL summary sourceSha does not match the RAM identity".to_string());
    }
    Ok(())
}

fn validate_pd_hil_overall(result: &serde_json::Map<String, Value>) -> Result<&str, String> {
    let overall = require_result_string(result, "overall")?;
    if !matches!(
        overall,
        "pass"
            | "external_source_pass"
            | "unsupported"
            | "fail"
            | "recovery_failed"
            | "cancelled"
            | "global_timeout"
            | "capability_discovery_failed"
    ) {
        return Err(format!("result.overall has an invalid value: {overall:?}"));
    }
    Ok(overall)
}

fn validate_pd_hil_policy(result: &serde_json::Map<String, Value>) -> Result<bool, String> {
    if result.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("result.schemaVersion must be 1".to_string());
    }
    if result.get("controller").and_then(Value::as_str) != Some("fusb302b") {
        return Err("result.controller must be fusb302b".to_string());
    }
    let policy = result
        .get("policy")
        .and_then(Value::as_object)
        .ok_or_else(|| "result.policy must be an object".to_string())?;
    let vin_validation = policy
        .get("vinValidation")
        .and_then(Value::as_str)
        .unwrap_or("adc");
    if !matches!(vin_validation, "adc" | "external_source") {
        return Err("result.policy.vinValidation is invalid".to_string());
    }
    if policy.get("holdMs").and_then(Value::as_u64) != Some(2_000)
        || policy.get("sampleIntervalMs").and_then(Value::as_u64) != Some(50)
        || policy.get("currentCeilingMa").and_then(Value::as_u64) != Some(5_000)
        || policy.get("minimumCurrentMa").and_then(Value::as_u64) != Some(3_000)
        || policy.get("recoveryMode").and_then(Value::as_str) != Some("fixed_5v_contract")
    {
        return Err("result.policy does not match the approved PD HIL bounds".to_string());
    }
    Ok(vin_validation == "external_source")
}

fn validate_pd_hil_tiers(
    result: &serde_json::Map<String, Value>,
    external_vin: bool,
) -> Result<(usize, usize, usize), String> {
    let tiers = result
        .get("tiers")
        .and_then(Value::as_array)
        .ok_or_else(|| "result.tiers must be an array".to_string())?;
    if tiers.len() > 22 {
        return Err("result.tiers contains more than the approved 22 rows".to_string());
    }
    let mut pass_count = 0usize;
    let mut unsupported_count = 0usize;
    let expected_targets = [
        ("fixed", 5_000),
        ("fixed", 9_000),
        ("fixed", 12_000),
        ("fixed", 15_000),
        ("fixed", 20_000),
        ("pps", 5_000),
        ("pps", 6_000),
        ("pps", 7_000),
        ("pps", 8_000),
        ("pps", 9_000),
        ("pps", 10_000),
        ("pps", 11_000),
        ("pps", 12_000),
        ("pps", 13_000),
        ("pps", 14_000),
        ("pps", 15_000),
        ("pps", 16_000),
        ("pps", 17_000),
        ("pps", 18_000),
        ("pps", 19_000),
        ("pps", 20_000),
        ("pps", 21_000),
    ];
    for (index, tier) in tiers.iter().enumerate() {
        let tier = tier
            .as_object()
            .ok_or_else(|| format!("result.tiers[{index}] must be an object"))?;
        let mode = tier
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("result.tiers[{index}].mode is missing"))?;
        if !matches!(mode, "fixed" | "pps") {
            return Err(format!("result.tiers[{index}].mode is invalid"));
        }
        let target_mv = tier
            .get("targetMv")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("result.tiers[{index}].targetMv is missing"))?;
        let (expected_mode, expected_mv) = expected_targets[index];
        if mode != expected_mode || target_mv != expected_mv {
            return Err(format!(
                "result.tiers[{index}] does not match the approved PD tier matrix"
            ));
        }
        let status = tier
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("result.tiers[{index}].status is missing"))?;
        if !matches!(
            status,
            "pass"
                | "unsupported"
                | "negotiation_failed"
                | "measurement_failed"
                | "recovery_failed"
                | "cancelled"
                | "global_timeout"
                | "not_run"
        ) {
            return Err(format!("result.tiers[{index}].status is invalid"));
        }
        if tier.get("measuredCurrentMa").is_some() || tier.get("loadPower").is_some() {
            return Err(format!(
                "result.tiers[{index}] contains forbidden current evidence"
            ));
        }
        let reason = tier
            .get("reason")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("result.tiers[{index}].reason is missing"))?;
        if status != "pass" && reason.is_empty() {
            return Err(format!(
                "result.tiers[{index}].reason is required for a non-passing tier"
            ));
        }
        if status == "pass" {
            if tier.get("contractConfirmed").and_then(Value::as_bool) != Some(true) {
                return Err(format!(
                    "result.tiers[{index}].contractConfirmed must be true for a passing tier"
                ));
            }
            if tier.get("holdMs").and_then(Value::as_u64) != Some(2_000) {
                return Err(format!(
                    "result.tiers[{index}].holdMs must be 2000 for a passing tier"
                ));
            }
            if tier
                .get("sampleCount")
                .and_then(Value::as_u64)
                .is_none_or(|count| count < 20)
            {
                return Err(format!(
                    "result.tiers[{index}].sampleCount must be at least 20"
                ));
            }
            if tier.get("contractMv").and_then(Value::as_u64) != Some(target_mv) {
                return Err(format!(
                    "result.tiers[{index}].contractMv must equal targetMv"
                ));
            }
            if tier
                .get("contractCurrentMa")
                .and_then(Value::as_u64)
                .is_none_or(|current| current < 3_000)
            {
                return Err(format!(
                    "result.tiers[{index}].contractCurrentMa is below the approved minimum"
                ));
            }
            if !external_vin {
                for field in [
                    "sourceAdvertisedMaxMa",
                    "requestSentAtMs",
                    "contractConfirmedAtMs",
                    "holdStartedAtMs",
                    "holdFinishedAtMs",
                    "invalidSampleCount",
                    "minMeasuredVinMv",
                    "maxMeasuredVinMv",
                    "meanMeasuredVinMv",
                    "firstMeasuredVinMv",
                    "lastMeasuredVinMv",
                ] {
                    if tier.get(field).and_then(Value::as_u64).is_none() {
                        return Err(format!(
                            "result.tiers[{index}].{field} is required for ADC validation"
                        ));
                    }
                }
            }
            pass_count += 1;
        } else if status == "unsupported" {
            unsupported_count += 1;
        }
    }
    Ok((tiers.len(), pass_count, unsupported_count))
}

fn validate_pd_hil_final_reset(
    result: &serde_json::Map<String, Value>,
) -> Result<(&str, &serde_json::Map<String, Value>), String> {
    let final_reset = result
        .get("finalReset")
        .and_then(Value::as_object)
        .ok_or_else(|| "result.finalReset must be an object".to_string())?;
    let reset_status = final_reset
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "result.finalReset.status is missing".to_string())?;
    if !matches!(reset_status, "pass" | "fail") {
        return Err("result.finalReset.status is invalid".to_string());
    }
    if final_reset.get("pendingRequest") != Some(&Value::Null) {
        return Err("result.finalReset must clear pendingRequest".to_string());
    }
    if reset_status == "pass" {
        let active_contract = final_reset
            .get("activeContract")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                "result.finalReset must record the stable fixed 5V recovery contract".to_string()
            })?;
        if active_contract.get("mode").and_then(Value::as_str) != Some("fixed")
            || active_contract.get("voltageMv").and_then(Value::as_u64) != Some(5_000)
            || active_contract
                .get("currentMa")
                .and_then(Value::as_u64)
                .is_none_or(|value| value == 0)
        {
            return Err(
                "result.finalReset.activeContract must be a nonzero fixed 5V contract".to_string(),
            );
        }
    } else if final_reset.get("activeContract") != Some(&Value::Null) {
        return Err("failed result.finalReset must not claim an active contract".to_string());
    }
    Ok((reset_status, final_reset))
}

pub(crate) fn pd_hil_requires_nonzero(value: &Value) -> bool {
    value.get("capability").and_then(Value::as_str) == Some(PD_HIL_CAPABILITY)
        && value
            .get("result")
            .and_then(|result| result.get("overall"))
            .and_then(Value::as_str)
            .is_some_and(|overall| !matches!(overall, "pass" | "external_source_pass"))
}

fn validate_button_result(result: &serde_json::Map<String, Value>) -> Result<(), String> {
    let detail = require_result_string(result, "detail")?;
    if !matches!(
        detail,
        "buttons_read_only_ready"
            | "buttons_interactive_complete"
            | "buttons_interactive_timeout"
            | "buttons_interactive_event_limit"
    ) {
        return Err(format!(
            "result.detail must identify a button sample or interactive session, got {detail:?}"
        ));
    }
    let buttons = result
        .get("buttons")
        .and_then(Value::as_object)
        .ok_or_else(|| "result.buttons must be an object".to_string())?;
    for name in ["center", "right", "down", "left", "up"] {
        if buttons.get(name).and_then(Value::as_bool).is_none() {
            return Err(format!("result.buttons.{name} must be a boolean"));
        }
    }
    if detail == "buttons_read_only_ready" {
        return Ok(());
    }
    let interaction = result
        .get("interaction")
        .and_then(Value::as_object)
        .ok_or_else(|| "result.interaction must be an object".to_string())?;
    for field in ["timeoutSeconds", "inactivityTimeoutSeconds"] {
        if interaction.get(field).and_then(Value::as_u64)
            != Some(BUTTON_INTERACTIVE_TIMEOUT.as_secs())
        {
            return Err(format!("result.interaction.{field} must be 30"));
        }
    }
    let stop_reason = interaction
        .get("stopReason")
        .and_then(Value::as_str)
        .ok_or_else(|| "result.interaction.stopReason must be a non-empty string".to_string())?;
    let expected_stop_reason = match detail {
        "buttons_interactive_event_limit" => ButtonInteractionStopReason::EventLimit.wire_value(),
        "buttons_interactive_complete" | "buttons_interactive_timeout" => {
            ButtonInteractionStopReason::InactivityTimeout.wire_value()
        }
        _ => return Err("result.detail is not an interactive button result".to_string()),
    };
    if stop_reason != expected_stop_reason {
        return Err(format!(
            "result.interaction.stopReason must be {expected_stop_reason:?} for {detail:?}"
        ));
    }
    let events = interaction
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| "result.interaction.events must be an array".to_string())?;
    for (index, event) in events.iter().enumerate() {
        let event = event
            .as_object()
            .ok_or_else(|| format!("result.interaction.events[{index}] must be an object"))?;
        for field in ["key", "gesture", "effect"] {
            if event
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!(
                    "result.interaction.events[{index}].{field} must be a non-empty string"
                ));
            }
        }
        if event.get("elapsedMs").and_then(Value::as_u64).is_none() {
            return Err(format!(
                "result.interaction.events[{index}].elapsedMs must be a non-negative integer"
            ));
        }
        if event
            .get("triggeredAtUnixMs")
            .and_then(Value::as_u64)
            .is_none()
        {
            return Err(format!(
                "result.interaction.events[{index}].triggeredAtUnixMs must be a non-negative integer"
            ));
        }
    }
    Ok(())
}

fn require_result_string<'a>(
    result: &'a serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, String> {
    result
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("result.{name} must be a non-empty string"))
}

fn require_result_string_value(
    result: &serde_json::Map<String, Value>,
    name: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = require_result_string(result, name)?;
    if actual != expected {
        return Err(format!(
            "result.{name} must be {expected:?}, got {actual:?}"
        ));
    }
    Ok(())
}

fn require_u8(values: &serde_json::Map<String, Value>, name: &str) -> Result<u8, String> {
    let value = values
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be an unsigned integer"))?;
    u8::try_from(value).map_err(|_| format!("{name} is outside the u8 range"))
}

fn require_u16(values: &serde_json::Map<String, Value>, name: &str) -> Result<u16, String> {
    let value = values
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be an unsigned integer"))?;
    u16::try_from(value).map_err(|_| format!("{name} is outside the u16 range"))
}

fn require_u32(values: &serde_json::Map<String, Value>, name: &str) -> Result<u32, String> {
    let value = values
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be an unsigned integer"))?;
    u32::try_from(value).map_err(|_| format!("{name} is outside the u32 range"))
}

fn disable_usb_serial_jtag_watchdogs(
    connection: &mut Connection,
    usb_pid: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if usb_pid != ESP32S3_USB_SERIAL_JTAG_PID {
        return Ok(());
    }
    connection.write_reg(RTC_CNTL_WDTWPROTECT, RTC_CNTL_WDT_WKEY, None)?;
    connection.write_reg(RTC_CNTL_WDTCONFIG0, 0, None)?;
    connection.write_reg(RTC_CNTL_WDTWPROTECT, 0, None)?;
    connection.write_reg(RTC_CNTL_SWD_WPROTECT, RTC_CNTL_SWD_WKEY, None)?;
    let swd_config = connection.read_reg(RTC_CNTL_SWD_CONF)?;
    connection.write_reg(
        RTC_CNTL_SWD_CONF,
        swd_config | RTC_CNTL_SWD_AUTO_FEED_EN,
        None,
    )?;
    connection.write_reg(RTC_CNTL_SWD_WPROTECT, 0, None)?;
    Ok(())
}

fn load_ram_elf(
    port: &str,
    image: RamElfImage,
    expected: &UsbSerialIdentity,
) -> Result<(ObservedIdentity, Port), Box<dyn std::error::Error + Send + Sync>> {
    ensure_ram_target(port, expected)?;
    let port_info = serialport::available_ports()?
        .into_iter()
        .find(|candidate| serial_port_paths_match(port, &candidate.port_name))
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    if !expected.matches_port_info(&port_info) {
        return Err(
            format!("authorized USB target changed or is no longer enumerated: {port}").into(),
        );
    }
    let usb_info = match port_info.port_type {
        SerialPortType::UsbPort(info) => info,
        SerialPortType::Unknown => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
        _ => return Err("RAM loader requires a USB serial target".into()),
    };
    let usb_pid = usb_info.pid;
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .open_native()?;
    let mut connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115_200,
    );
    connection.begin()?;
    connection.set_timeout(Duration::from_secs(3))?;
    let chip = connection.detect_chip(false)?;
    if chip != Chip::Esp32s3 {
        return Err(format!("RAM loader detected unexpected chip: {chip}").into());
    }
    ensure_ram_target(port, expected)?;
    disable_usb_serial_jtag_watchdogs(&mut connection, usb_pid)?;
    let RamElfImage { entry, sections } = image;
    for (address, mut data) in sections {
        let padding = (4 - data.len() % 4) % 4;
        data.resize(data.len() + padding, 0);
        let blocks = data.len().div_ceil(RAM_BLOCK_SIZE);
        connection.command(RomCommand::MemBegin {
            size: data.len() as u32,
            blocks: blocks as u32,
            block_size: RAM_BLOCK_SIZE as u32,
            offset: address,
            supports_encryption: false,
        })?;
        for (sequence, chunk) in data.chunks(RAM_BLOCK_SIZE).enumerate() {
            connection.command(RomCommand::MemData {
                data: chunk,
                pad_to: 4,
                pad_byte: 0,
                sequence: sequence as u32,
            })?;
        }
    }
    connection.with_timeout(CommandType::MemEnd.timeout(), |connection| {
        connection.command(RomCommand::MemEnd {
            no_entry: false,
            entry,
        })
    })?;
    let mut serial = connection.into_serial();
    let identity = read_identity_from_serial(&mut serial)?;
    Ok((identity, serial))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_and_ram_frames_are_classified_separately() {
        let product = parse_identity_line(
            br#"{"type":"hello","firmwareKind":"product","identity":{"firmwareKind":"product","buildId":"p","capabilities":["identity"]}}"#,
        )
        .unwrap();
        assert_eq!(product.firmware, ObservedFirmware::Product);
        let ram = parse_identity_line(
            br#"{"type":"hello","firmwareKind":"ram_bringup","identity":{"firmwareKind":"ram_bringup","buildId":"r","capabilities":["test_fan"]}}"#,
        )
        .unwrap();
        assert_eq!(ram.firmware, ObservedFirmware::RamBringup);
    }

    #[test]
    fn ram_identity_requires_the_declared_protocol_and_framing() {
        let identity = ObservedIdentity {
            firmware: ObservedFirmware::RamBringup,
            build_id: Some(PRODUCT_BUILD_ID.to_string()),
            source_sha: Some("test-source-sha".to_string()),
            capabilities: vec!["test_fan".to_string()],
            protocol_version: Some(RAM_PROTOCOL_VERSION.to_string()),
            framing: Some(RAM_FRAMING.to_string()),
            reset_reason: None,
        };
        assert!(verify_ram_identity(&identity, "test_fan").is_ok());

        let mut mismatched = identity;
        mismatched.framing = Some("raw".to_string());
        assert!(verify_ram_identity(&mismatched, "test_fan").is_err());
    }

    #[test]
    fn ram_display_request_can_select_a_named_color_without_changing_capability() {
        let request = build_ram_request("ram-1", "preview_display", Some("red"));
        let value: Value = serde_json::from_str(&request).expect("request should be JSON");
        assert_eq!(value["op"], "preview_display");
        assert_eq!(value["capability"], "preview_display");
        assert_eq!(value["color"], "red");
    }

    #[test]
    fn ram_display_request_omits_color_by_default() {
        let request = build_ram_request("ram-1", "preview_display", None);
        let value: Value = serde_json::from_str(&request).expect("request should be JSON");
        assert!(value.get("color").is_none());
    }

    #[test]
    fn pd_hil_request_can_use_external_voltage_evidence() {
        let request = build_pd_hil_request("pd-hil-1", false);
        let value: Value = serde_json::from_str(&request).expect("request should be JSON");
        assert_eq!(value["capability"], PD_HIL_CAPABILITY);
        assert_eq!(value["validateVin"], false);
    }

    #[test]
    fn compact_fan_stage_frame_is_consumed_before_the_final_response() {
        let mut bytes = br#"{"type":"fp","v":12500,"m":true,"ok":true}
{"type":"fs","d":50}
"#
        .to_vec();
        bytes.extend_from_slice(
            br#"{"type":"response","firmwareKind":"ram_bringup","requestId":"ram-1","capability":"test_fan","ok":true,"result":{"detail":"fan_ready","heater":"off","pd":"untouched","eeprom":"untouched","effect":"fan_50_percent_5s_100_percent_5s_0_percent_5s","power":{"vinRaw":1800,"vinAdcMv":1100,"inputMv":12500,"minimumMv":12500,"measured":true,"voltageOk":true}}}
"#,
        );
        let mut fan_power_seen = false;
        let response = find_ram_response(&mut bytes, "test_fan", "ram-1", &mut fan_power_seen)
            .unwrap()
            .expect("final response should be returned");
        assert_eq!(response["result"]["detail"], "fan_ready");
        assert!(bytes.is_empty());
    }

    #[test]
    fn ram_buzzer_validation_requires_the_two_tone_sequence() {
        let valid = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "buzzer_ready",
                "effect": "1khz_1s_silence_1s_2khz_1s",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
            },
        });
        assert!(validate_ram_success_response(&valid, "test_buzzer").is_ok());

        let mut stale = valid;
        stale["result"]["effect"] = Value::String("20ms_pulse".to_string());
        assert!(validate_ram_success_response(&stale, "test_buzzer").is_err());
    }

    #[test]
    fn ram_rgb_and_fan_validation_require_the_slow_sequences() {
        let rgb = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "rgb_ready",
                "effect": "rgb_red_green_blue_1s_x5",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
            },
        });
        assert!(validate_ram_success_response(&rgb, "test_rgb").is_ok());
        let mut stale_rgb = rgb;
        stale_rgb["result"]["effect"] = Value::String("rgb_3_color".to_string());
        assert!(validate_ram_success_response(&stale_rgb, "test_rgb").is_err());

        let fan = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "fan_ready",
                "effect": "fan_50_percent_5s_100_percent_5s_0_percent_5s",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "power": {
                    "vinRaw": 1800,
                    "vinAdcMv": 1100,
                    "inputMv": 12500,
                    "minimumMv": 12500,
                    "measured": true,
                    "voltageOk": true,
                },
            },
        });
        assert!(validate_ram_success_response(&fan, "test_fan").is_ok());
        let mut stale_fan = fan;
        stale_fan["result"]["effect"] = Value::String("50_percent_pwm_500ms".to_string());
        assert!(validate_ram_success_response(&stale_fan, "test_fan").is_err());
    }

    #[test]
    fn ram_success_validation_requires_safety_and_command_evidence() {
        let valid = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "buttons_read_only_ready",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "buttons": {
                    "center": false,
                    "right": false,
                    "down": false,
                    "left": false,
                    "up": false,
                },
            },
        });
        assert!(validate_ram_success_response(&valid, "test_buttons").is_ok());

        let mut missing_safety = valid.clone();
        missing_safety["result"]
            .as_object_mut()
            .unwrap()
            .remove("heater");
        assert!(validate_ram_success_response(&missing_safety, "test_buttons").is_err());

        let mut missing_evidence = valid;
        missing_evidence["result"]
            .as_object_mut()
            .unwrap()
            .remove("buttons");
        assert!(validate_ram_success_response(&missing_evidence, "test_buttons").is_err());
    }

    #[test]
    fn pd_hil_validation_accepts_unsupported_tiers_and_requires_nonzero_exit() {
        let summary = serde_json::json!({
            "type": "pd_hil_summary",
            "requestId": "pd-hil-test",
            "capability": "test_pd_sink",
            "ok": true,
            "result": {
                "detail": "pd_hil_complete",
                "heater": "off",
                "pd": "default_verified",
                "eeprom": "untouched",
                "schemaVersion": 1,
                "controller": "fusb302b",
                "overall": "unsupported",
                "policy": {
                    "holdMs": 2000,
                    "sampleIntervalMs": 50,
                    "currentCeilingMa": 5000,
                    "minimumCurrentMa": 3000,
                    "recoveryMode": "fixed_5v_contract",
                },
                "sourceCapabilities": {"count": 1, "rawPdos": [123]},
                "tiers": (0..22).map(|index| serde_json::json!({
                    "mode": if index < 5 { "fixed" } else { "pps" },
                    "targetMv": if index < 5 {
                        [5000, 9000, 12000, 15000, 20000][index]
                    } else {
                        5000 + (index - 5) * 1000
                    },
                    "status": "unsupported",
                    "reason": "no_exact_fixed_pdo",
                })).collect::<Vec<_>>(),
                "finalReset": {
                    "status": "pass",
                    "activeContract": {"mode": "fixed", "voltageMv": 5000, "currentMa": 3000},
                    "pendingRequest": null,
                    "defaultVbusMv": 5000,
                },
            },
        });
        assert!(validate_ram_success_response(&summary, PD_HIL_CAPABILITY).is_ok());
        assert!(pd_hil_requires_nonzero(&summary));

        let mut incomplete = summary.clone();
        incomplete["result"]["tiers"] = Value::Array(
            incomplete["result"]["tiers"]
                .as_array()
                .unwrap()
                .iter()
                .take(1)
                .cloned()
                .collect(),
        );
        assert!(validate_ram_success_response(&incomplete, PD_HIL_CAPABILITY).is_err());

        let mut forbidden_current = summary.clone();
        forbidden_current["result"]["tiers"][0]["measuredCurrentMa"] = 3000.into();
        assert!(validate_ram_success_response(&forbidden_current, PD_HIL_CAPABILITY).is_err());
    }

    #[test]
    fn pd_hil_validation_accepts_only_a_complete_pass_matrix() {
        let tiers = (0..22)
            .map(|index| {
                serde_json::json!({
                    "mode": if index < 5 { "fixed" } else { "pps" },
                    "targetMv": if index < 5 {
                        [5000, 9000, 12000, 15000, 20000][index]
                    } else {
                        5000 + (index - 5) * 1000
                    },
                    "status": "pass",
                    "reason": "test_pass",
                    "contractConfirmed": true,
                    "holdMs": 2000,
                    "sampleCount": 20,
                    "contractMv": if index < 5 {
                        [5000, 9000, 12000, 15000, 20000][index]
                    } else {
                        5000 + (index - 5) * 1000
                    },
                    "contractCurrentMa": 3000,
                    "sourceAdvertisedMaxMa": 5000,
                    "requestSentAtMs": 1,
                    "contractConfirmedAtMs": 2,
                    "holdStartedAtMs": 3,
                    "holdFinishedAtMs": 2003,
                    "invalidSampleCount": 0,
                    "minMeasuredVinMv": 5000,
                    "maxMeasuredVinMv": 5000,
                    "meanMeasuredVinMv": 5000,
                    "firstMeasuredVinMv": 5000,
                    "lastMeasuredVinMv": 5000,
                })
            })
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "type": "pd_hil_summary",
            "requestId": "pd-hil-test",
            "capability": "test_pd_sink",
            "ok": true,
            "result": {
                "detail": "pd_hil_complete",
                "heater": "off",
                "pd": "default_verified",
                "eeprom": "untouched",
                "schemaVersion": 1,
                "controller": "fusb302b",
                "overall": "pass",
                "policy": {
                    "holdMs": 2000,
                    "sampleIntervalMs": 50,
                    "currentCeilingMa": 5000,
                    "minimumCurrentMa": 3000,
                "recoveryMode": "fixed_5v_contract",
                },
                "sourceCapabilities": {"count": 1, "rawPdos": [123]},
                "tiers": tiers,
                "finalReset": {
                    "status": "pass",
                    "activeContract": {"mode": "fixed", "voltageMv": 5000, "currentMa": 3000},
                    "pendingRequest": null,
                    "defaultVbusMv": 5000,
                },
            },
        });
        let validation = validate_ram_success_response(&summary, PD_HIL_CAPABILITY);
        assert!(validation.is_ok(), "{validation:?}");
        assert!(!pd_hil_requires_nonzero(&summary));

        let mut external = summary.clone();
        external["result"]["overall"] = Value::String("external_source_pass".to_string());
        external["result"]["policy"]["vinValidation"] =
            Value::String("external_source".to_string());
        external["result"]["finalReset"]["defaultVbusMv"] = 5990.into();
        assert!(validate_ram_success_response(&external, PD_HIL_CAPABILITY).is_ok());
        assert!(!pd_hil_requires_nonzero(&external));

        let mut incomplete = summary;
        incomplete["result"]["tiers"][21]["status"] = Value::String("unsupported".to_string());
        assert!(validate_ram_success_response(&incomplete, PD_HIL_CAPABILITY).is_err());
    }

    #[test]
    fn button_gesture_tracker_distinguishes_short_long_and_double() {
        let start = Instant::now();

        let mut short = ButtonGestureTracker::default();
        assert!(
            short
                .observe(
                    ButtonSnapshot {
                        pressed: [true, false, false, false, false],
                    },
                    start,
                )
                .is_empty()
        );
        assert!(
            short
                .observe(
                    ButtonSnapshot {
                        pressed: [false, false, false, false, false],
                    },
                    start + Duration::from_millis(100),
                )
                .is_empty()
        );
        let short_events = short.observe(
            ButtonSnapshot {
                pressed: [false, false, false, false, false],
            },
            start + Duration::from_millis(500),
        );
        assert_eq!(short_events[0].gesture, "short_press");
        assert_eq!(
            short_events[0].triggered_at,
            start + Duration::from_millis(500)
        );
        assert!(short_events[0].triggered_at_unix_ms > 0);

        let mut long = ButtonGestureTracker::default();
        long.observe(
            ButtonSnapshot {
                pressed: [true, false, false, false, false],
            },
            start,
        );
        let long_events = long.observe(
            ButtonSnapshot {
                pressed: [true, false, false, false, false],
            },
            start + Duration::from_millis(800),
        );
        assert_eq!(long_events[0].gesture, "long_press");

        let mut double = ButtonGestureTracker::default();
        double.observe(
            ButtonSnapshot {
                pressed: [true, false, false, false, false],
            },
            start,
        );
        double.observe(
            ButtonSnapshot {
                pressed: [false, false, false, false, false],
            },
            start + Duration::from_millis(100),
        );
        double.observe(
            ButtonSnapshot {
                pressed: [true, false, false, false, false],
            },
            start + Duration::from_millis(200),
        );
        let double_events = double.observe(
            ButtonSnapshot {
                pressed: [false, false, false, false, false],
            },
            start + Duration::from_millis(300),
        );
        assert_eq!(double_events[0].gesture, "double_click");
        assert_eq!(
            double_events[0].triggered_at,
            start + Duration::from_millis(300)
        );
    }

    #[test]
    fn interactive_button_result_requires_a_30_second_inactivity_window() {
        let value = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "buttons_interactive_timeout",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "buttons": {
                    "center": false,
                    "right": false,
                    "down": false,
                    "left": false,
                    "up": false,
                },
                "interaction": {
                    "timeoutSeconds": 30,
                    "inactivityTimeoutSeconds": 30,
                    "stopReason": "inactivity_timeout",
                    "events": [],
                },
            },
        });
        assert!(validate_ram_success_response(&value, "test_buttons").is_ok());
    }

    #[test]
    fn interactive_button_event_limit_does_not_overshoot() {
        let now = Instant::now();
        let mut events = Vec::new();
        for expected_len in 1..=BUTTON_MAX_EVENTS {
            assert!(try_push_button_event(
                &mut events,
                ButtonGestureEvent::new("center", "short_press", now),
            ));
            assert_eq!(events.len(), expected_len);
        }
        assert!(!try_push_button_event(
            &mut events,
            ButtonGestureEvent::new("center", "short_press", now),
        ));
        assert_eq!(events.len(), BUTTON_MAX_EVENTS);
    }

    #[test]
    fn interactive_button_events_require_trigger_times() {
        let mut value = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "buttons_interactive_complete",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "buttons": {
                    "center": false,
                    "right": false,
                    "down": false,
                    "left": false,
                    "up": false,
                },
                "interaction": {
                    "timeoutSeconds": 30,
                    "inactivityTimeoutSeconds": 30,
                    "stopReason": "inactivity_timeout",
                    "events": [{
                        "key": "center",
                        "gesture": "short_press",
                        "effect": "success",
                    }],
                },
            },
        });
        assert!(validate_ram_success_response(&value, "test_buttons").is_err());
        value["result"]["interaction"]["events"][0]["elapsedMs"] = serde_json::json!(1250u64);
        value["result"]["interaction"]["events"][0]["triggeredAtUnixMs"] =
            serde_json::json!(1790000000123u64);
        assert!(validate_ram_success_response(&value, "test_buttons").is_ok());

        value["result"]["detail"] = serde_json::json!("buttons_interactive_event_limit");
        value["result"]["interaction"]["stopReason"] = serde_json::json!("event_limit");
        assert!(validate_ram_success_response(&value, "test_buttons").is_ok());
    }

    #[test]
    fn ram_success_validation_checks_measurement_shapes_and_allowlist() {
        let adc = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "adc_read_only_ready",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "adc": {"vin": 572, "rtd": 1299},
            },
        });
        assert!(validate_ram_success_response(&adc, "test_adc").is_ok());

        let i2c = serde_json::json!({
            "ok": true,
            "result": {
                "detail": "i2c_identification_read_only_ready",
                "heater": "off",
                "pd": "untouched",
                "eeprom": "untouched",
                "i2c": {"address": 34, "register": 9, "value": 6},
            },
        });
        assert!(validate_ram_success_response(&i2c, "test_i2c").is_ok());

        let mut wrong_address = i2c;
        wrong_address["result"]["i2c"]["address"] = serde_json::json!(80);
        assert!(validate_ram_success_response(&wrong_address, "test_i2c").is_err());
    }

    #[test]
    fn ram_response_buffer_rejects_an_unbounded_jsonl_frame() {
        let mut bytes = vec![b'x'; RAM_RESPONSE_BUFFER_LIMIT];
        assert!(append_ram_response_bytes(&mut bytes, b"y").is_err());
    }

    #[test]
    fn ram_elf_gate_rejects_an_oversized_file_before_reading_it() {
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid
            .as_file_mut()
            .set_len(RAM_ELF_FILE_LIMIT + 1)
            .unwrap();

        let error = read_validated_ram_elf(invalid.path()).err().unwrap();

        assert!(error.to_string().contains("file-size limit"));
    }

    #[cfg(unix)]
    #[test]
    fn ram_port_lock_is_exclusive() {
        let port = format!("/tmp/flux-purr-devd-ram-test-port-{}", std::process::id());
        let first = acquire_ram_port_lock(&port).expect("first RAM lock should succeed");
        let second =
            match SerialPortProcessLock::acquire(&port, Instant::now() + Duration::from_millis(25))
            {
                Ok(_) => panic!("second RAM lock should be rejected"),
                Err(error) => error,
            };
        assert!(format!("{second:?}").contains("serial_lock_timeout"));
        drop(first);
    }

    fn test_elf(address: u32) -> Vec<u8> {
        test_elf_with_payload(address, 32)
    }

    fn test_elf_with_payload(address: u32, payload_size: usize) -> Vec<u8> {
        test_elf_with_payload_flags(address, payload_size, 5)
    }

    fn test_elf_with_mem_size(address: u32, payload_size: usize, mem_size: usize) -> Vec<u8> {
        let mut data = test_elf_with_payload_flags(address, payload_size, 5);
        data[52 + 20..52 + 24].copy_from_slice(&(mem_size as u32).to_le_bytes());
        data
    }

    fn test_elf_with_payload_flags(address: u32, payload_size: usize, flags: u32) -> Vec<u8> {
        let payload_offset = 52 + 32;
        let section_offset = payload_offset + payload_size;
        let mut data = vec![0u8; section_offset + 2 * 40];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = 1;
        data[5] = 1;
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        data[18..20].copy_from_slice(&ELF_MACHINE_XTENSA.to_le_bytes());
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[24..28].copy_from_slice(&address.to_le_bytes());
        data[28..32].copy_from_slice(&52u32.to_le_bytes());
        data[32..36].copy_from_slice(&(section_offset as u32).to_le_bytes());
        data[40..42].copy_from_slice(&52u16.to_le_bytes());
        data[42..44].copy_from_slice(&32u16.to_le_bytes());
        data[44..46].copy_from_slice(&1u16.to_le_bytes());
        data[46..48].copy_from_slice(&40u16.to_le_bytes());
        data[48..50].copy_from_slice(&2u16.to_le_bytes());
        let ph = 52;
        data[ph..ph + 4].copy_from_slice(&ELF_PT_LOAD.to_le_bytes());
        data[ph + 4..ph + 8].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        data[ph + 8..ph + 12].copy_from_slice(&address.to_le_bytes());
        data[ph + 12..ph + 16].copy_from_slice(&address.to_le_bytes());
        data[ph + 16..ph + 20].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 20..ph + 24].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 24..ph + 28].copy_from_slice(&flags.to_le_bytes());
        data[ph + 28..ph + 32].copy_from_slice(&4u32.to_le_bytes());
        let section = section_offset + 40;
        data[section + 4..section + 8].copy_from_slice(&ELF_SHT_PROGBITS.to_le_bytes());
        let section_flags = ELF_SHF_ALLOC
            | if u64::from(flags) & ELF_PF_X != 0 {
                ELF_SHF_EXECINSTR
            } else {
                0
            };
        data[section + 8..section + 12].copy_from_slice(&(section_flags as u32).to_le_bytes());
        data[section + 12..section + 16].copy_from_slice(&address.to_le_bytes());
        data[section + 16..section + 20].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        data[section + 20..section + 24].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[section + 32..section + 36].copy_from_slice(&4u32.to_le_bytes());
        data
    }

    #[test]
    fn ram_elf_gate_accepts_internal_ram_and_rejects_flash() {
        let mut valid = tempfile::NamedTempFile::new().unwrap();
        valid.write_all(&test_elf(0x4037_8400)).unwrap();
        assert!(validate_ram_elf(valid.path()).is_ok());

        let mut flash = tempfile::NamedTempFile::new().unwrap();
        flash.write_all(&test_elf(0x4200_0000)).unwrap();
        assert!(validate_ram_elf(flash.path()).is_err());
    }

    #[test]
    fn ram_elf_gate_accepts_only_the_complete_vectors_segment() {
        let mut valid = tempfile::NamedTempFile::new().unwrap();
        valid
            .write_all(&test_elf_with_payload(0x4037_8000, 0x400))
            .unwrap();
        assert!(validate_ram_elf(valid.path()).is_ok());

        for payload_size in [32, 0x401] {
            let mut invalid = tempfile::NamedTempFile::new().unwrap();
            invalid
                .write_all(&test_elf_with_payload(0x4037_8000, payload_size))
                .unwrap();
            assert!(validate_ram_elf(invalid.path()).is_err());
        }
    }

    #[test]
    fn ram_elf_gate_rejects_entry_outside_executable_segment() {
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid
            .write_all(&test_elf_with_payload_flags(0x3fc8_8000, 32, 6))
            .unwrap();
        assert!(validate_ram_elf(invalid.path()).is_err());
    }

    #[test]
    fn ram_elf_gate_rejects_entry_in_zero_fill_tail() {
        let mut artifact = test_elf_with_payload(0x4037_8400, 32);
        artifact[24..28].copy_from_slice(&0x4037_8420u32.to_le_bytes());
        artifact[52 + 20..52 + 24].copy_from_slice(&64u32.to_le_bytes());
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid.write_all(&artifact).unwrap();
        assert!(validate_ram_elf(invalid.path()).is_err());
    }

    #[test]
    fn ram_elf_loader_leaves_segment_memory_tail_to_runtime_reset() {
        let mut artifact = tempfile::NamedTempFile::new().unwrap();
        artifact
            .write_all(&test_elf_with_mem_size(0x4037_8400, 32, 48))
            .unwrap();

        let image = read_validated_ram_elf(artifact.path()).unwrap();

        assert_eq!(image.sections.len(), 1);
        assert_eq!(image.sections[0].0, 0x4037_8400);
        assert_eq!(image.sections[0].1.len(), 32);
    }

    #[test]
    fn ram_elf_loader_rejects_sections_outside_load_segments() {
        let mut data = test_elf(0x4037_8400);
        let section_offset = 52 + 32 + 32 + 40;
        data[section_offset + 12..section_offset + 16]
            .copy_from_slice(&0x4037_8420u32.to_le_bytes());
        let mut artifact = tempfile::NamedTempFile::new().unwrap();
        artifact.write_all(&data).unwrap();

        assert!(read_validated_ram_elf(artifact.path()).is_err());
    }
}
